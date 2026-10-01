// Synchronizes the TUI message catalogs from the WebUI catalogs.
// 从 WebUI 语言目录同步 TUI 文案目录。
//
// The WebUI catalogs are the single authority for shared text. The TUI never authors its own copy
// of an upstream key; it pulls exactly the keys used by `tr!` in `src/`, in every upstream locale.
// TUI-only text lives in `i18n/overlay/<code>.json` under the reserved `tui.` prefix and may never
// shadow an upstream key.
// WebUI 目录是共享文案的唯一权威。TUI 不自行编写任何上游键的副本；它按 `src/` 中 `tr!` 实际使用的键，
// 从每种上游语言中拉取。TUI 独有文案位于 `i18n/overlay/<code>.json`，必须使用保留前缀 `tui.`，
// 且不得遮蔽任何上游键。

pub mod scan;
pub mod upstream;

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use serde::{Deserialize, Serialize};

use upstream::{UpstreamLocale, parse_catalog, parse_catalog_loaders, parse_locale_registry};

/// Reserved prefix for keys that exist only in the TUI overlay.
/// TUI 覆盖层独有键的保留前缀。
pub const OVERLAY_PREFIX: &str = "tui.";

/// Repository-relative paths of every generated artifact.
/// 所有生成产物相对仓库根目录的路径。
pub const GENERATED_MESSAGES_DIR: &str = "i18n/generated/messages";
pub const GENERATED_MANIFEST: &str = "i18n/generated/manifest.json";
pub const GENERATED_RUST: &str = "src/i18n/generated_catalogs.rs";
pub const OVERLAY_DIR: &str = "i18n/overlay";
pub const SYNC_CONFIG: &str = "i18n/sync.json";

/// `i18n/sync.json`: where the upstream WebUI sources live, relative to the repository root.
/// `i18n/sync.json`：上游 WebUI 源码相对仓库根目录的位置。
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SyncConfig {
    pub webui_src: String,
}

/// Options for one sync run.
/// 单次同步运行的选项。
#[derive(Debug, Clone)]
pub struct SyncOptions {
    pub repo_root: PathBuf,
    pub webui_src: PathBuf,
    /// Compare instead of write; any difference is reported as a failure.
    /// 只比较不写入；任何差异都报告为失败。
    pub check: bool,
}

/// What a sync run found or changed.
/// 一次同步运行发现或修改的内容。
#[derive(Debug, Default)]
pub struct SyncReport {
    pub used_keys: usize,
    pub locales: Vec<String>,
    pub written: Vec<String>,
    pub removed: Vec<String>,
    /// Files that differ from the expected output (check mode only).
    /// 与期望输出不一致的文件（仅检查模式）。
    pub out_of_date: Vec<String>,
    pub missing: BTreeMap<String, Vec<String>>,
    pub warnings: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Manifest<'a> {
    source: &'a str,
    default_locale: &'a str,
    locales: Vec<ManifestLocale<'a>>,
    keys: &'a BTreeSet<String>,
    missing: &'a BTreeMap<String, Vec<String>>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ManifestLocale<'a> {
    code: &'a str,
    native_name: &'a str,
    mark: &'a str,
}

/// Reads `i18n/sync.json` from the repository root.
/// 从仓库根目录读取 `i18n/sync.json`。
pub fn load_config(repo_root: &Path) -> Result<SyncConfig> {
    let path = repo_root.join(SYNC_CONFIG);
    let text = fs::read_to_string(&path).with_context(|| format!("read {}", path.display()))?;
    serde_json::from_str(&text).with_context(|| format!("parse {}", path.display()))
}

/// Runs one sync (or check) and returns the report; hard contract violations are errors.
/// 执行一次同步（或检查）并返回报告；硬性契约违规以错误返回。
pub fn run(options: &SyncOptions) -> Result<SyncReport> {
    let webui = &options.webui_src;
    let registry = parse_locale_registry(&read(&webui.join("i18n-locales.ts"))?)?;
    let loaders = parse_catalog_loaders(&read(&webui.join("i18n-catalogs.ts"))?)?;
    let registry_codes: BTreeSet<&str> = registry.locales.iter().map(|l| l.code.as_str()).collect();
    let loader_codes: BTreeSet<&str> = loaders.keys().map(String::as_str).collect();
    if registry_codes != loader_codes {
        bail!(
            "SUPPORTED_LOCALES {:?} and catalogLoaders {:?} disagree",
            registry_codes,
            loader_codes
        );
    }

    let mut upstream = BTreeMap::new();
    for locale in &registry.locales {
        let (module, export) = &loaders[&locale.code];
        let catalog = parse_catalog(&read(&webui.join(module))?, export)
            .with_context(|| format!("parse upstream catalog {module}"))?;
        upstream.insert(locale.code.clone(), catalog);
    }
    let upstream_keys: BTreeSet<&String> = upstream.values().flat_map(|c| c.keys()).collect();

    let overlays = load_overlays(&options.repo_root, &registry.locales)?;
    for (code, overlay) in &overlays {
        for key in overlay.keys() {
            if !key.starts_with(OVERLAY_PREFIX) {
                bail!("overlay {code}.json key {key} must start with `{OVERLAY_PREFIX}`");
            }
            if upstream_keys.contains(key) {
                bail!("overlay {code}.json key {key} shadows an upstream WebUI key");
            }
        }
    }

    let src = options.repo_root.join("src");
    let used = scan::used_keys(&src, &[src.join("i18n_sync")])?;
    let default_code = registry.default_locale.as_str();
    let mut report = SyncReport {
        used_keys: used.len(),
        locales: registry.locales.iter().map(|l| l.code.clone()).collect(),
        ..SyncReport::default()
    };

    let lookup = |code: &str, key: &str| -> Option<String> {
        if key.starts_with(OVERLAY_PREFIX) {
            overlays.get(code).and_then(|o| o.get(key)).cloned()
        } else {
            upstream.get(code).and_then(|c| c.get(key)).cloned()
        }
    };

    let unresolved: Vec<&String> = used
        .iter()
        .filter(|key| lookup(default_code, key).is_none())
        .collect();
    if !unresolved.is_empty() {
        bail!(
            "keys used by the TUI are missing from the default locale {default_code} \
             (upstream catalog or `tui.` overlay): {unresolved:?}"
        );
    }

    let mut expected: BTreeMap<String, String> = BTreeMap::new();
    for locale in &registry.locales {
        let mut messages = BTreeMap::new();
        let mut missing = Vec::new();
        for key in &used {
            match lookup(&locale.code, key) {
                Some(value) => {
                    if locale.code != default_code
                        && let Some(base) = lookup(default_code, key)
                        && upstream::placeholders(&base) != upstream::placeholders(&value)
                    {
                        report.warnings.push(format!(
                            "{}: placeholders of {key} differ from {default_code}",
                            locale.code
                        ));
                    }
                    messages.insert(key.clone(), value);
                }
                None => missing.push(key.clone()),
            }
        }
        if !missing.is_empty() {
            report.missing.insert(locale.code.clone(), missing);
        }
        expected.insert(
            format!("{GENERATED_MESSAGES_DIR}/{}.json", locale.code),
            to_json(&messages)?,
        );
    }

    let manifest = Manifest {
        source: "vulcan-code/apps/vulcan-code-web/src",
        default_locale: default_code,
        locales: registry
            .locales
            .iter()
            .map(|l| ManifestLocale {
                code: &l.code,
                native_name: &l.native_name,
                mark: &l.mark,
            })
            .collect(),
        keys: &used,
        missing: &report.missing,
    };
    expected.insert(GENERATED_MANIFEST.to_string(), to_json(&manifest)?);
    expected.insert(
        GENERATED_RUST.to_string(),
        render_rust(&registry.locales, default_code),
    );

    let messages_dir = options.repo_root.join(GENERATED_MESSAGES_DIR);
    let stale = stale_message_files(&messages_dir, &report.locales)?;

    for (relative, content) in &expected {
        let path = options.repo_root.join(relative);
        let current = fs::read_to_string(&path).ok();
        if current.as_deref().map(normalize_newlines) == Some(content.clone()) {
            continue;
        }
        if options.check {
            report.out_of_date.push(relative.clone());
        } else {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&path, content).with_context(|| format!("write {}", path.display()))?;
            report.written.push(relative.clone());
        }
    }
    for path in stale {
        let relative = format!(
            "{GENERATED_MESSAGES_DIR}/{}",
            path.file_name().unwrap_or_default().to_string_lossy()
        );
        if options.check {
            report.out_of_date.push(relative);
        } else {
            fs::remove_file(&path).with_context(|| format!("remove {}", path.display()))?;
            report.removed.push(relative);
        }
    }
    Ok(report)
}

/// Loads every `i18n/overlay/<code>.json`; overlay locales must be upstream locales.
/// 加载所有 `i18n/overlay/<code>.json`；覆盖层语言必须属于上游语言。
fn load_overlays(
    repo_root: &Path,
    locales: &[UpstreamLocale],
) -> Result<BTreeMap<String, BTreeMap<String, String>>> {
    let dir = repo_root.join(OVERLAY_DIR);
    let mut overlays = BTreeMap::new();
    if !dir.exists() {
        return Ok(overlays);
    }
    for entry in fs::read_dir(&dir)? {
        let path = entry?.path();
        if path.extension().is_none_or(|ext| ext != "json") {
            continue;
        }
        let code = path
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| anyhow!("invalid overlay file name {}", path.display()))?
            .to_string();
        if !locales.iter().any(|l| l.code == code) {
            bail!("overlay {code}.json is not an upstream locale");
        }
        let map: BTreeMap<String, String> = serde_json::from_str(&read(&path)?)
            .with_context(|| format!("parse {}", path.display()))?;
        overlays.insert(code, map);
    }
    Ok(overlays)
}

/// Lists generated message files whose locale no longer exists upstream.
/// 列出上游已不存在其语言的生成文案文件。
fn stale_message_files(dir: &Path, locales: &[String]) -> Result<Vec<PathBuf>> {
    let mut stale = Vec::new();
    if !dir.exists() {
        return Ok(stale);
    }
    for entry in fs::read_dir(dir)? {
        let path = entry?.path();
        let keep = path
            .file_stem()
            .and_then(|s| s.to_str())
            .is_some_and(|stem| locales.iter().any(|code| code == stem))
            && path.extension().is_some_and(|ext| ext == "json");
        if !keep {
            stale.push(path);
        }
    }
    stale.sort();
    Ok(stale)
}

/// Renders `src/i18n/generated_catalogs.rs`, embedding every generated locale file.
/// 渲染 `src/i18n/generated_catalogs.rs`，内嵌所有生成的语言文件。
fn render_rust(locales: &[UpstreamLocale], default_code: &str) -> String {
    let mut out = String::new();
    out.push_str("// @generated by `cargo run --bin vulcan-tui-i18n -- sync`. Do not edit.\n");
    out.push_str("// 由 `cargo run --bin vulcan-tui-i18n -- sync` 生成，请勿手工修改。\n\n");
    out.push_str("use super::GeneratedLocale;\n\n");
    out.push_str(&format!(
        "pub const DEFAULT_LOCALE: &str = {};\n\n",
        rust_str(default_code)
    ));
    out.push_str("pub const LOCALES: &[GeneratedLocale] = &[\n");
    for locale in locales {
        out.push_str("    GeneratedLocale {\n");
        out.push_str(&format!("        code: {},\n", rust_str(&locale.code)));
        out.push_str(&format!(
            "        native_name: {},\n",
            rust_str(&locale.native_name)
        ));
        out.push_str(&format!("        mark: {},\n", rust_str(&locale.mark)));
        out.push_str(&format!(
            "        messages_json: include_str!(\"../../{GENERATED_MESSAGES_DIR}/{}.json\"),\n",
            locale.code
        ));
        out.push_str("    },\n");
    }
    out.push_str("];\n");
    out
}

fn rust_str(value: &str) -> String {
    format!("{value:?}")
}

fn to_json<T: Serialize>(value: &T) -> Result<String> {
    let mut text = serde_json::to_string_pretty(value)?;
    text.push('\n');
    Ok(text)
}

fn normalize_newlines(text: &str) -> String {
    text.replace("\r\n", "\n")
}

fn read(path: &Path) -> Result<String> {
    fs::read_to_string(path).with_context(|| format!("read {}", path.display()))
}
