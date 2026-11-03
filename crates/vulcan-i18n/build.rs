//! Validates the shared catalogs and embeds them as per-surface static tables.
//! 校验共享语言目录，并将其按界面分面内嵌为静态表。
//!
//! `catalogs/locales.json` is the only language list. Every surface it declares owns
//! `catalogs/<surface>/<code>.json`; the default locale file is required, and a complete surface
//! requires every locale. Each file must be a flat object of strings.
//! `catalogs/locales.json` 是唯一的语言清单。其声明的每个分面持有 `catalogs/<surface>/<code>.json`；
//! 默认语言文件必须存在，完整分面要求每种语言都存在。每个文件必须是字符串扁平对象。

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
        writeln!(
            out,
            "pub static {}: Surface = Surface {{\n    name: {name:?},\n    complete: {complete},\n    files: &[\n{files}    ],\n}};",
            name.to_uppercase()
        )?;
    }

    fs::write(PathBuf::from(env::var("OUT_DIR")?).join("catalogs.rs"), out)?;
    Ok(())
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
