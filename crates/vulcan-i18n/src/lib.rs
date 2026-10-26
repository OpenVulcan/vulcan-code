//! Shared language registry and text catalogs for every Vulcan Code surface.
//! Vulcan Code 各界面共用的语言注册表与文案目录。
//!
//! `catalogs/locales.json` is the only list of supported languages and `catalogs/<surface>/` holds
//! one flat JSON catalog per language. The WebUI imports the same files; Rust consumers read them
//! through this crate, embedded at compile time.
//! `catalogs/locales.json` 是唯一的受支持语言清单，`catalogs/<surface>/` 为每种语言保存一个扁平
//! JSON 目录。WebUI 导入同一批文件；Rust 消费方通过本 crate 读取编译期内嵌的目录。
//!
//! `changelog/` holds the release notes the About area shows: one Markdown file per release and
//! registered language, exposed as [`RELEASES`].
//! `changelog/` 保存“关于”区域展示的更新记录：每个发布、每种注册语言各一个 Markdown 文件，以
//! [`RELEASES`] 公开。

use std::collections::HashMap;

/// One supported language.
/// 一种受支持的语言。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Locale {
    /// Exact wire code, for example `zh-TW`.
    /// 精确传输代码，例如 `zh-TW`。
    pub code: &'static str,
    /// Name of the language written in that language.
    /// 以该语言书写的语言名称。
    pub native_name: &'static str,
    /// Short picker mark.
    /// 选择器短标记。
    pub mark: &'static str,
}

/// The catalogs of one user-facing surface, for example the WebUI or the TUI.
/// 一个面向用户界面的目录集合，例如 WebUI 或 TUI。
#[derive(Debug)]
pub struct Surface {
    name: &'static str,
    complete: bool,
    files: &'static [(&'static str, &'static str)],
}

/// Flat key-to-text map of one surface in one language.
/// 一个分面在一种语言下的扁平键到文案映射。
pub type Messages = HashMap<String, String>;

impl Surface {
    /// Directory name of the surface under `catalogs/`.
    /// 分面在 `catalogs/` 下的目录名。
    pub fn name(&self) -> &'static str {
        self.name
    }

    /// Whether every language must translate every key; incomplete surfaces fall back per key.
    /// 是否要求每种语言翻译全部键；不完整分面按键回退。
    pub fn is_complete(&self) -> bool {
        self.complete
    }

    /// Languages that ship a catalog for this surface.
    /// 为该分面提供目录的语言。
    pub fn locales(&self) -> impl Iterator<Item = &'static str> {
        self.files.iter().map(|(code, _)| *code)
    }

    /// Parses the catalog of `code`; empty when the surface has no file for that language.
    /// 解析 `code` 的目录；该分面没有对应语言文件时返回空映射。
    pub fn messages(&self, code: &str) -> Messages {
        self.files
            .iter()
            .find(|(candidate, _)| *candidate == code)
            .map(|(_, json)| {
                serde_json::from_str(json).unwrap_or_else(|error| {
                    panic!(
                        "{}/{code}.json was validated by build.rs: {error}",
                        self.name
                    )
                })
            })
            .unwrap_or_default()
    }
}

/// One release in `changelog/releases.json`, with its notes in every supported language.
/// `changelog/releases.json` 中的一个发布，附每种受支持语言的说明。
#[derive(Debug)]
pub struct Release {
    /// Exact `x.y.z` version.
    /// 精确的 `x.y.z` 版本。
    pub version: &'static str,
    /// Release day as `YYYY-MM-DD`; `None` while the newest release is unreleased.
    /// 以 `YYYY-MM-DD` 表示的发布日；最新发布尚未发布时为 `None`。
    pub date: Option<&'static str>,
    notes: &'static [(&'static str, &'static str)],
}

impl Release {
    /// Markdown notes of `code`, which may contain closed `ui-*` answer component fences.
    /// `code` 语言的 Markdown 说明，可包含已闭合的 `ui-*` 回答组件围栏。
    ///
    /// # Returns
    /// 返回值
    /// The notes for any registered code (`build.rs` requires every language), `None` otherwise.
    /// 任一注册语言的说明（`build.rs` 要求每种语言都有）；否则返回 `None`。
    pub fn notes(&self, code: &str) -> Option<&'static str> {
        self.notes
            .iter()
            .find(|(candidate, _)| *candidate == code)
            .map(|(_, notes)| *notes)
    }
}

include!(concat!(env!("OUT_DIR"), "/catalogs.rs"));

/// Validates an exact persisted or service-provided code.
/// 校验精确的持久化或服务提供的语言代码。
///
/// # Returns
/// 返回值
/// The registry code, or `None` for anything outside the registry, including other casings.
/// 注册表中的代码；不在注册表内（包括大小写不同）时返回 `None`。
pub fn normalize(value: &str) -> Option<&'static str> {
    LOCALES
        .iter()
        .find(|locale| locale.code == value)
        .map(|locale| locale.code)
}

/// Maps one BCP 47 tag onto the closest supported language.
/// 将一个 BCP 47 标签映射到最接近的受支持语言。
///
/// Chinese resolves by script (`Hant`/`Hans`) first, then by region: Traditional Hong Kong uses the
/// Hong Kong catalog, while Taiwan and Macau use Taiwan. Other languages match the exact code, then
/// the language subtag. The shared vectors in `catalogs/locale-matching.json` pin this behavior for
/// every implementation.
/// 中文先按书写系统（`Hant`/`Hans`）再按地区解析：香港繁体使用香港目录，台湾与澳门使用台湾目录。
/// 其他语言先匹配完整代码，再匹配语言子标签。`catalogs/locale-matching.json` 中的共享向量为所有实现
/// 固定该行为。
pub fn match_tag(tag: &str) -> Option<&'static str> {
    let normalized = tag.trim().replace('_', "-").to_lowercase();
    let subtags: Vec<&str> = normalized.split('-').filter(|s| !s.is_empty()).collect();
    let language = *subtags.first()?;
    if language == "zh" {
        let traditional = if subtags.contains(&"hk") {
            "zh-HK"
        } else {
            "zh-TW"
        };
        let chosen = if subtags.contains(&"hant") {
            traditional
        } else if subtags.contains(&"hans") {
            "zh-CN"
        } else if subtags.iter().any(|s| matches!(*s, "tw" | "hk" | "mo")) {
            traditional
        } else {
            "zh-CN"
        };
        return normalize(chosen);
    }
    let joined = subtags.join("-");
    LOCALES
        .iter()
        .find(|locale| locale.code.to_lowercase() == joined)
        .or_else(|| {
            LOCALES.iter().find(|locale| {
                locale
                    .code
                    .split('-')
                    .next()
                    .is_some_and(|primary| primary.eq_ignore_ascii_case(language))
            })
        })
        .map(|locale| locale.code)
}

/// Picks the first supported language from a preference list, or the default language.
/// 从偏好列表中选出第一种受支持语言；都不支持时返回默认语言。
pub fn detect<I, S>(preferred: I) -> &'static str
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    preferred
        .into_iter()
        .find_map(|tag| match_tag(tag.as_ref()))
        .unwrap_or(DEFAULT_LOCALE)
}

/// Best language for the operating system's preference list. Never persisted by callers.
/// 操作系统偏好列表对应的最佳语言。调用方不得持久化该结果。
pub fn system_locale() -> &'static str {
    detect(sys_locale::get_locales())
}

/// Replaces `{name}` placeholders that have a value; others stay untouched.
/// 替换有取值的 `{name}` 占位符；其余保持原样。
pub fn format_template(template: &str, values: &[(&str, &str)]) -> String {
    let mut out = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let value = after.find('}').and_then(|close| {
            let name = &after[..close];
            let valid = !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_');
            valid
                .then(|| values.iter().find(|(key, _)| *key == name))
                .flatten()
                .map(|(_, value)| (close, *value))
        });
        match value {
            Some((close, value)) => {
                out.push_str(value);
                rest = &after[close + 1..];
            }
            None => {
                out.push('{');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Layered lookup over several surfaces: the active language first, then the default language.
/// 跨多个分面的分层查找：先当前语言，再默认语言。
///
/// Layers are searched in order within each language, so a surface can reuse another surface's keys
/// (the TUI reads WebUI keys before its own).
/// 每种语言内按层顺序查找，因此一个分面可以复用另一个分面的键（TUI 先读 WebUI 键再读自身键）。
#[derive(Debug)]
pub struct Translator {
    layers: Vec<&'static Surface>,
    locale: &'static str,
    active: Vec<Messages>,
    fallback: Vec<Messages>,
}

impl Translator {
    /// Creates a translator in the default language.
    /// 创建使用默认语言的翻译器。
    pub fn new(layers: &[&'static Surface]) -> Self {
        let fallback: Vec<Messages> = layers
            .iter()
            .map(|surface| surface.messages(DEFAULT_LOCALE))
            .collect();
        Self {
            layers: layers.to_vec(),
            locale: DEFAULT_LOCALE,
            active: vec![Messages::new(); layers.len()],
            fallback,
        }
    }

    /// The active language code.
    /// 当前语言代码。
    pub fn locale(&self) -> &'static str {
        self.locale
    }

    /// Switches to an exact registry code; unknown codes are rejected and change nothing.
    /// 切换到注册表中的精确代码；未知代码被拒绝且不做任何改变。
    pub fn set_locale(&mut self, code: &str) -> bool {
        let Some(code) = normalize(code) else {
            return false;
        };
        self.active = if code == DEFAULT_LOCALE {
            vec![Messages::new(); self.layers.len()]
        } else {
            self.layers
                .iter()
                .map(|surface| surface.messages(code))
                .collect()
        };
        self.locale = code;
        true
    }

    /// Text of `key`, or `None` when no layer defines it in either language.
    /// `key` 的文案；任何层在两种语言下都未定义时返回 `None`。
    pub fn text(&self, key: &str) -> Option<&str> {
        self.active
            .iter()
            .chain(&self.fallback)
            .find_map(|messages| messages.get(key))
            .map(String::as_str)
    }
}

#[cfg(test)]
mod tests;
