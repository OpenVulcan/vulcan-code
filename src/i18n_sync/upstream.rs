// Parsers for the WebUI i18n sources that the TUI mirrors.
// 解析 TUI 所镜像的 WebUI 多语言源文件。
//
// Contract owner: `apps/vulcan-code-web/src/i18n-locales.ts`, `i18n-catalogs.ts` and
// `i18n-messages.<code>.ts` in the vulcan-code repository. Any format change there must fail this
// parser loudly instead of silently producing a partial catalog; update these parsers and their
// fixtures when the upstream layout changes.
// 契约归属：vulcan-code 仓库 `apps/vulcan-code-web/src/` 下的 `i18n-locales.ts`、`i18n-catalogs.ts`
// 与 `i18n-messages.<code>.ts`。上游格式变化时本解析器必须显式失败，而不是静默生成残缺目录；
// 上游布局变化时同步更新这些解析器与测试夹具。

use std::collections::BTreeMap;

use anyhow::{Context, Result, anyhow, bail};
use regex::Regex;

/// One entry of the upstream `SUPPORTED_LOCALES` registry.
/// 上游 `SUPPORTED_LOCALES` 注册表中的一项。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpstreamLocale {
    pub code: String,
    pub native_name: String,
    pub mark: String,
}

/// The locale registry parsed from `i18n-locales.ts`.
/// 从 `i18n-locales.ts` 解析出的语言注册表。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocaleRegistry {
    pub locales: Vec<UpstreamLocale>,
    pub default_locale: String,
}

/// Parses `SUPPORTED_LOCALES` and `DEFAULT_LOCALE` from `i18n-locales.ts`.
/// 从 `i18n-locales.ts` 解析 `SUPPORTED_LOCALES` 与 `DEFAULT_LOCALE`。
pub fn parse_locale_registry(source: &str) -> Result<LocaleRegistry> {
    let source = source.replace("\r\n", "\n");
    let source = source.as_str();
    let block_re = Regex::new(r"(?s)export const SUPPORTED_LOCALES = \[(.*?)\] as const")?;
    let block = block_re
        .captures(source)
        .ok_or_else(|| anyhow!("SUPPORTED_LOCALES array literal was not found"))?
        .get(1)
        .map(|m| m.as_str())
        .unwrap_or_default();
    let entry_re = Regex::new(
        r#"^\{\s*code:\s*"([^"]+)",\s*nativeName:\s*"([^"]+)",\s*mark:\s*"([^"]+)"\s*\},?$"#,
    )?;
    let mut locales = Vec::new();
    for raw in block.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with("//") {
            continue;
        }
        let caps = entry_re
            .captures(line)
            .ok_or_else(|| anyhow!("unrecognized SUPPORTED_LOCALES entry: {line}"))?;
        let code = caps[1].to_string();
        if locales
            .iter()
            .any(|entry: &UpstreamLocale| entry.code == code)
        {
            bail!("duplicate locale code in SUPPORTED_LOCALES: {code}");
        }
        locales.push(UpstreamLocale {
            code,
            native_name: caps[2].to_string(),
            mark: caps[3].to_string(),
        });
    }
    if locales.is_empty() {
        bail!("SUPPORTED_LOCALES is empty");
    }
    let default_re = Regex::new(r#"export const DEFAULT_LOCALE: LocaleCode = "([^"]+)""#)?;
    let default_locale = default_re
        .captures(source)
        .ok_or_else(|| anyhow!("DEFAULT_LOCALE declaration was not found"))?[1]
        .to_string();
    if !locales.iter().any(|entry| entry.code == default_locale) {
        bail!("DEFAULT_LOCALE {default_locale} is not listed in SUPPORTED_LOCALES");
    }
    Ok(LocaleRegistry {
        locales,
        default_locale,
    })
}

/// Parses the `catalogLoaders` map of `i18n-catalogs.ts` into `code -> (module file, export name)`.
/// 将 `i18n-catalogs.ts` 的 `catalogLoaders` 映射解析为 `代码 -> (模块文件, 导出名)`。
pub fn parse_catalog_loaders(source: &str) -> Result<BTreeMap<String, (String, String)>> {
    let source = source.replace("\r\n", "\n");
    // The type annotation contains `=>`, so match lazily up to the first `= {`.
    // 类型注解中含有 `=>`，因此惰性匹配到第一个 `= {`。
    let block_re = Regex::new(r"(?s)const catalogLoaders\b.*?= \{\n(.*?)\n\}")?;
    let block = block_re
        .captures(&source)
        .ok_or_else(|| anyhow!("catalogLoaders map was not found"))?
        .get(1)
        .map(|m| m.as_str())
        .unwrap_or_default();
    let entry_re = Regex::new(
        r#"^"([^"]+)":\s*\(\)\s*=>\s*import\("@/([^"]+)"\)\.then\(\(module\)\s*=>\s*module\.(\w+)\),?$"#,
    )?;
    let mut loaders = BTreeMap::new();
    for raw in block.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with("//") {
            continue;
        }
        let caps = entry_re
            .captures(line)
            .ok_or_else(|| anyhow!("unrecognized catalogLoaders entry: {line}"))?;
        let code = caps[1].to_string();
        let module = format!("{}.ts", &caps[2]);
        if loaders
            .insert(code.clone(), (module, caps[3].to_string()))
            .is_some()
        {
            bail!("duplicate catalogLoaders entry: {code}");
        }
    }
    Ok(loaders)
}

/// Parses one `i18n-messages.<code>.ts` catalog into an ordered key/value map.
/// 将一个 `i18n-messages.<code>.ts` 目录解析为有序键值映射。
///
/// The catalog must be exactly one `export const <name>: Record<TranslationKey, string> = {` object
/// whose body holds one JSON-compatible `"key": "value",` pair per line.
/// 目录必须恰好是一个 `export const <name>: Record<TranslationKey, string> = {` 对象，
/// 其主体每行是一个 JSON 兼容的 `"key": "value",` 键值对。
pub fn parse_catalog(source: &str, export_name: &str) -> Result<BTreeMap<String, String>> {
    let source = source.replace("\r\n", "\n");
    let header = format!("export const {export_name}: Record<TranslationKey, string> = {{");
    let start = source
        .find(&header)
        .ok_or_else(|| anyhow!("catalog header `{header}` was not found"))?;
    let body = &source[start + header.len()..];
    let end = body
        .find("\n}")
        .ok_or_else(|| anyhow!("catalog object for {export_name} is not closed"))?;
    let mut entries = BTreeMap::new();
    for (index, raw) in body[..end].lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }
        let pair = line.strip_suffix(',').unwrap_or(line);
        let parsed: serde_json::Map<String, serde_json::Value> =
            serde_json::from_str(&format!("{{{pair}}}")).with_context(|| {
                format!(
                    "line {} of {export_name} is not a JSON key/value pair: {line}",
                    index + 1
                )
            })?;
        let (key, value) = parsed
            .into_iter()
            .next()
            .ok_or_else(|| anyhow!("empty catalog line in {export_name}"))?;
        let value = value
            .as_str()
            .ok_or_else(|| anyhow!("value of {key} in {export_name} is not a string"))?
            .to_string();
        if entries.insert(key.clone(), value).is_some() {
            bail!("duplicate key {key} in {export_name}");
        }
    }
    if entries.is_empty() {
        bail!("catalog {export_name} has no entries");
    }
    Ok(entries)
}

/// Extracts the sorted set of `{placeholder}` names used by a message.
/// 提取消息中使用的 `{placeholder}` 名称有序集合。
pub fn placeholders(message: &str) -> Vec<String> {
    let mut names: Vec<String> = PLACEHOLDER_RE
        .get_or_init(|| Regex::new(r"\{(\w+)\}").expect("placeholder regex"))
        .captures_iter(message)
        .map(|caps| caps[1].to_string())
        .collect();
    names.sort();
    names.dedup();
    names
}

static PLACEHOLDER_RE: std::sync::OnceLock<Regex> = std::sync::OnceLock::new();

#[cfg(test)]
mod tests {
    use super::*;

    const LOCALES_FIXTURE: &str = r#"
// comment
export const SUPPORTED_LOCALES = [
  { code: "en-US", nativeName: "English", mark: "EN" },
  { code: "zh-TW", nativeName: "繁體中文", mark: "繁" },
] as const

export const DEFAULT_LOCALE: LocaleCode = "en-US"
"#;

    #[test]
    fn parses_locale_registry() {
        let registry = parse_locale_registry(LOCALES_FIXTURE).unwrap();
        assert_eq!(registry.default_locale, "en-US");
        assert_eq!(registry.locales.len(), 2);
        assert_eq!(registry.locales[1].native_name, "繁體中文");
    }

    #[test]
    fn rejects_unknown_locale_entry_shape() {
        let broken = LOCALES_FIXTURE.replace("mark: \"EN\"", "mark: \"EN\", rtl: false");
        assert!(parse_locale_registry(&broken).is_err());
    }

    #[test]
    fn parses_catalog_loaders() {
        let source = "const catalogLoaders: Record<LocaleCode, () => Promise<LocaleCatalog>> = {\n  \"en-US\": () => import(\"@/i18n-messages.en-US\").then((module) => module.enUSTranslations),\n}\n";
        let loaders = parse_catalog_loaders(source).unwrap();
        assert_eq!(
            loaders["en-US"],
            (
                "i18n-messages.en-US.ts".to_string(),
                "enUSTranslations".to_string()
            )
        );
    }

    #[test]
    fn parses_catalog_with_crlf_and_escapes() {
        let source = "import x\r\nexport const enUSTranslations: Record<TranslationKey, string> = {\r\n    \"a.b\": \"Say \\\"hi\\\" {name}\",\r\n    \"c\": \"x\",\r\n}\r\n";
        let catalog = parse_catalog(source, "enUSTranslations").unwrap();
        assert_eq!(catalog["a.b"], "Say \"hi\" {name}");
        assert_eq!(catalog.len(), 2);
    }

    #[test]
    fn rejects_duplicate_catalog_keys() {
        let source = "export const t: Record<TranslationKey, string> = {\n    \"a\": \"1\",\n    \"a\": \"2\",\n}\n";
        assert!(parse_catalog(source, "t").is_err());
    }

    #[test]
    fn extracts_placeholders() {
        assert_eq!(
            placeholders("Ran {count} of {count} {name}"),
            vec!["count", "name"]
        );
    }
}
