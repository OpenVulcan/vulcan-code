//! Validates the shared catalogs and embeds them as per-surface static tables.
//! 校验共享语言目录，并将其按界面分面内嵌为静态表。
//!
//! `catalogs/locales.json` is the only language list. Every surface it declares owns
//! `catalogs/<surface>/<code>.json`; the default locale file is required, and a complete surface
//! requires every locale. Each file must be a flat object of strings.
//! `catalogs/locales.json` 是唯一的语言清单。其声明的每个分面持有 `catalogs/<surface>/<code>.json`；
//! 默认语言文件必须存在，完整分面要求每种语言都存在。每个文件必须是字符串扁平对象。
//!
//! The release notes in `changelog/` follow the same registry: every release needs every language.
//! `changelog/` 中的更新记录遵循同一注册表：每个发布都需要每种语言。

use std::{
    collections::BTreeSet,
    env,
    error::Error,
    fmt::Write as _,
    fs,
    path::{Path, PathBuf},
};

use serde_json::Value;

/// Generates `catalogs.rs` in `OUT_DIR`.
/// 在 `OUT_DIR` 中生成 `catalogs.rs`。
fn main() -> Result<(), Box<dyn Error>> {
    let root = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?).join("catalogs");
    println!("cargo:rerun-if-changed={}", root.display());
    let registry: Value = serde_json::from_str(&fs::read_to_string(root.join("locales.json"))?)?;
    let default = registry["default"]
        .as_str()
        .ok_or("locales.json: `default` must be a locale code")?;
    let locales = registry["locales"]
        .as_array()
        .ok_or("locales.json: `locales` must be an array")?;

    let mut out = String::new();
    let mut codes = BTreeSet::new();
    writeln!(out, "/// Every supported language in display order.")?;
    writeln!(out, "/// 按展示顺序排列的全部受支持语言。")?;
    writeln!(out, "pub static LOCALES: &[Locale] = &[")?;
    for locale in locales {
        let field = |name: &str| {
            locale[name]
                .as_str()
                .ok_or_else(|| format!("locales.json: every locale needs a string `{name}`"))
        };
        let code = field("code")?;
        if !codes.insert(code.to_string()) {
            return Err(format!("locales.json: duplicate locale `{code}`").into());
        }
        writeln!(
            out,
            "    Locale {{ code: {:?}, native_name: {:?}, mark: {:?} }},",
            code,
            field("nativeName")?,
            field("mark")?
        )?;
    }
    writeln!(out, "];")?;
    if !codes.contains(default) {
        return Err(format!("locales.json: default `{default}` is not a listed locale").into());
    }
    writeln!(
        out,
        "/// Language used when nothing else matches; it completes every catalog."
    )?;
    writeln!(out, "/// 无其他匹配时使用的语言；它补全所有目录。")?;
    writeln!(out, "pub const DEFAULT_LOCALE: &str = {default:?};")?;

    let surfaces = registry["surfaces"]
        .as_object()
        .ok_or("locales.json: `surfaces` must be an object")?;
    for (name, spec) in surfaces {
        let complete = spec["complete"]
            .as_bool()
            .ok_or_else(|| format!("locales.json: surface `{name}` needs a boolean `complete`"))?;
        let mut files = String::new();
        for code in locales.iter().filter_map(|locale| locale["code"].as_str()) {
            let path = root.join(name).join(format!("{code}.json"));
            if !path.is_file() {
                if complete || code == default {
                    return Err(format!("surface `{name}` is missing {}", path.display()).into());
                }
                continue;
            }
            validate_flat_strings(&path)?;
            writeln!(
                files,
                "        ({code:?}, include_str!({:?})),",
                path.display().to_string()
            )?;
        }
        writeln!(out, "/// The `{name}` surface catalogs.")?;
        writeln!(out, "/// `{name}` 分面目录。")?;
        writeln!(
            out,
            "pub static {}: Surface = Surface {{\n    name: {name:?},\n    complete: {complete},\n    files: &[\n{files}    ],\n}};",
            name.to_uppercase()
        )?;
    }

    let codes: Vec<&str> = locales
        .iter()
        .filter_map(|locale| locale["code"].as_str())
        .collect();
    embed_changelog(
        &PathBuf::from(env::var("CARGO_MANIFEST_DIR")?).join("changelog"),
        &codes,
        &mut out,
    )?;

    fs::write(PathBuf::from(env::var("OUT_DIR")?).join("catalogs.rs"), out)?;
    Ok(())
}

/// Validates `changelog/` and appends the `RELEASES` table to `out`.
/// 校验 `changelog/` 并向 `out` 追加 `RELEASES` 表。
///
/// `releases.json` lists releases newest first with strictly decreasing `x.y.z` versions and a
/// `YYYY-MM-DD` date, or `null` for the newest release while it is unreleased. Each release owns
/// `<version>/<code>.md` for exactly the registered `codes`; stray directories or files are rejected.
/// `releases.json` 按从新到旧列出发布，版本为严格递减的 `x.y.z`，日期为 `YYYY-MM-DD`；仅最新发布在
/// 未发布时可为 `null`。每个发布持有与注册语言 `codes` 完全一致的 `<version>/<code>.md`；多余的目录
/// 或文件一律拒绝。
fn embed_changelog(root: &Path, codes: &[&str], out: &mut String) -> Result<(), Box<dyn Error>> {
    println!("cargo:rerun-if-changed={}", root.display());
    let index = root.join("releases.json");
    let registry: Value = serde_json::from_str(&fs::read_to_string(&index)?)
        .map_err(|error| format!("{}: {error}", index.display()))?;
    let releases = registry["releases"]
        .as_array()
        .filter(|releases| !releases.is_empty())
        .ok_or("changelog/releases.json: `releases` must be a nonempty array")?;

    writeln!(
        out,
        "/// Every release, newest first, with notes in every supported language."
    )?;
    writeln!(out, "/// 全部发布，从新到旧，附每种受支持语言的说明。")?;
    writeln!(out, "pub static RELEASES: &[Release] = &[")?;
    let mut previous: Option<(u64, u64, u64)> = None;
    let mut listed = BTreeSet::new();
    for (position, release) in releases.iter().enumerate() {
        let version = release["version"]
            .as_str()
            .ok_or("changelog/releases.json: every release needs a string `version`")?;
        let parsed = parse_version(version)
            .ok_or_else(|| format!("changelog: `{version}` is not an `x.y.z` version"))?;
        if previous.is_some_and(|newer| parsed >= newer) {
            return Err(
                format!("changelog: `{version}` must be older than the release above it").into(),
            );
        }
        previous = Some(parsed);
        listed.insert(version.to_string());
        let date = match &release["date"] {
            Value::Null if position == 0 => None,
            Value::String(date) if is_date(date) => Some(date.as_str()),
            _ => {
                return Err(format!(
                    "changelog: `{version}` needs a YYYY-MM-DD `date`; only the newest release may be null"
                )
                .into());
            }
        };

        let directory = root.join(version);
        let present: BTreeSet<String> = fs::read_dir(&directory)
            .map_err(|error| format!("{}: {error}", directory.display()))?
            .map(|entry| entry.map(|entry| entry.file_name().to_string_lossy().into_owned()))
            .collect::<Result<_, _>>()?;
        let expected: BTreeSet<String> = codes.iter().map(|code| format!("{code}.md")).collect();
        if present != expected {
            return Err(format!(
                "{}: needs exactly one Markdown file per registered language; missing {:?}, unexpected {:?}",
                directory.display(),
                expected.difference(&present).collect::<Vec<_>>(),
                present.difference(&expected).collect::<Vec<_>>()
            )
            .into());
        }
        let mut notes = String::new();
        for code in codes {
            let path = directory.join(format!("{code}.md"));
            if fs::read_to_string(&path)?.trim().is_empty() {
                return Err(format!("{}: notes must not be empty", path.display()).into());
            }
            writeln!(
                notes,
                "            ({code:?}, include_str!({:?})),",
                path.display().to_string()
            )?;
        }
        writeln!(
            out,
            "    Release {{\n        version: {version:?},\n        date: {date:?},\n        notes: &[\n{notes}        ],\n    }},"
        )?;
    }
    writeln!(out, "];")?;

    for entry in fs::read_dir(root)? {
        let name = entry?.file_name().to_string_lossy().into_owned();
        if name != "releases.json" && !listed.contains(&name) {
            return Err(format!("changelog/{name} is not listed in releases.json").into());
        }
    }
    Ok(())
}

/// Parses an exact `x.y.z` version into comparable numbers.
/// 将精确的 `x.y.z` 版本解析为可比较的数值。
fn parse_version(version: &str) -> Option<(u64, u64, u64)> {
    let mut parts = version.split('.').map(|part| {
        (!part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit()))
            .then(|| part.parse().ok())
            .flatten()
    });
    let parsed = (parts.next()??, parts.next()??, parts.next()??);
    parts.next().is_none().then_some(parsed)
}

/// Whether `date` has the `YYYY-MM-DD` shape with a plausible month and day.
/// `date` 是否为月、日取值合理的 `YYYY-MM-DD` 形式。
fn is_date(date: &str) -> bool {
    let fields: Vec<&str> = date.split('-').collect();
    let number = |field: &str, width: usize| {
        (field.len() == width && field.bytes().all(|byte| byte.is_ascii_digit()))
            .then(|| field.parse::<u32>().ok())
            .flatten()
    };
    matches!(
        fields.as_slice(),
        [year, month, day]
            if number(year, 4).is_some()
                && number(month, 2).is_some_and(|month| (1..=12).contains(&month))
                && number(day, 2).is_some_and(|day| (1..=31).contains(&day))
    )
}

/// Rejects anything but a JSON object whose values are all strings.
/// 拒绝除值全部为字符串的 JSON 对象以外的任何内容。
fn validate_flat_strings(path: &Path) -> Result<(), Box<dyn Error>> {
    let value: Value = serde_json::from_str(&fs::read_to_string(path)?)
        .map_err(|error| format!("{}: {error}", path.display()))?;
    let object = value
        .as_object()
        .ok_or_else(|| format!("{}: must be a JSON object", path.display()))?;
    if let Some((key, _)) = object.iter().find(|(_, value)| !value.is_string()) {
        return Err(format!("{}: `{key}` must be a string", path.display()).into());
    }
    Ok(())
}
