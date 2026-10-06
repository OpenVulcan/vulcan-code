// Runtime translation for the TUI.
// TUI 运行时文案翻译。
//
// Text comes from the shared `vulcan-i18n` catalogs: WebUI keys first, then TUI-only `tui.*` keys.
// A key missing in the active locale falls back to the default locale, and `{name}` placeholders
// without a value are left untouched, as in the WebUI.
// 文案来自共享的 `vulcan-i18n` 目录：先查 WebUI 键，再查 TUI 专属 `tui.*` 键。当前语言缺失的键回退到
// 默认语言，没有取值的 `{name}` 占位符保持原样，与 WebUI 一致。

use std::sync::{OnceLock, RwLock};

pub use vulcan_i18n::{LOCALES, match_tag as match_locale_tag};
use vulcan_i18n::{TUI, Translator, WEBUI};

static TRANSLATOR: OnceLock<RwLock<Translator>> = OnceLock::new();

fn translator() -> &'static RwLock<Translator> {
    TRANSLATOR.get_or_init(|| RwLock::new(Translator::new(&[&WEBUI, &TUI])))
}

/// Switches the active locale; unknown codes are rejected and leave the current locale in place.
/// 切换当前语言；未知代码被拒绝并保持当前语言不变。
pub fn set_active_locale(code: &str) -> bool {
    translator()
        .write()
        .expect("i18n lock poisoned")
        .set_locale(code)
}

/// Holds the process-wide locale for one test: tests that read localized text take this guard so a
/// concurrent test cannot switch the locale under them.
/// 为一个测试占用进程级语言：读取本地化文案的测试持有此守卫，避免并发测试在其运行期间切换语言。
#[cfg(test)]
pub fn lock_locale(code: &str) -> std::sync::MutexGuard<'static, ()> {
    static LOCALE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    // A failed test poisons the lock without leaving the locale in a broken state.
    // 失败的测试会毒化该锁，但不会让语言处于损坏状态。
    let guard = LOCALE_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    set_active_locale(resolve_locale(Some(code), None));
    guard
}

/// Returns the active locale code.
/// 返回当前语言代码。
pub fn active_locale() -> &'static str {
    translator().read().expect("i18n lock poisoned").locale()
}

/// Looks up one key with default-locale fallback; unknown keys render as the key itself.
/// 按默认语言回退查找一个键；未知键渲染为键本身。
pub fn text(key: &str) -> String {
    translator()
        .read()
        .expect("i18n lock poisoned")
        .text(key)
        .unwrap_or(key)
        .to_string()
}

/// Looks up one key and replaces `{name}` placeholders with the given values.
/// 查找一个键并用给定值替换 `{name}` 占位符。
pub fn format(key: &str, values: &[(&str, String)]) -> String {
    let values: Vec<(&str, &str)> = values
        .iter()
        .map(|(name, value)| (*name, value.as_str()))
        .collect();
    vulcan_i18n::format_template(&text(key), &values)
}

/// Resolves the startup locale: explicit flag, then service `uiLocale`, then the OS, then default.
/// 解析启动语言：显式参数优先，其次服务 `uiLocale`，再次操作系统，最后默认语言。
///
/// The TUI never persists the result.
/// TUI 从不持久化该结果。
pub fn resolve_locale(explicit: Option<&str>, service_locale: Option<&str>) -> &'static str {
    explicit
        .and_then(|code| vulcan_i18n::normalize(code).or_else(|| match_locale_tag(code)))
        .or_else(|| service_locale.and_then(vulcan_i18n::normalize))
        .unwrap_or_else(vulcan_i18n::system_locale)
}

/// Translates a catalog key: `tr!("key")` or `tr!("key", name = value, ...)`.
/// 翻译一个目录键：`tr!("key")` 或 `tr!("key", name = value, ...)`。
///
/// Keys must be string literals so the catalog test can discover them.
/// 键必须是字符串字面量，以便目录测试发现它们。
#[macro_export]
macro_rules! tr {
    ($key:literal $(,)?) => {
        $crate::i18n::text($key)
    };
    ($key:literal, $($name:ident = $value:expr),+ $(,)?) => {
        $crate::i18n::format($key, &[$((stringify!($name), ($value).to_string())),+])
    };
}

#[cfg(test)]
mod scan;

#[cfg(test)]
mod tests {
    use super::*;

    use std::collections::BTreeSet;

    fn used_keys() -> BTreeSet<String> {
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        scan::used_keys(&src).unwrap()
    }

    #[test]
    fn format_keeps_placeholders_without_values() {
        assert_eq!(format("tui.test.missing {x}", &[]), "tui.test.missing {x}");
    }

    #[test]
    fn every_used_key_resolves_in_default_catalogs() {
        let default = Translator::new(&[&WEBUI, &TUI]);
        let missing: Vec<_> = used_keys()
            .into_iter()
            .filter(|key| default.text(key).is_none())
            .collect();
        assert!(
            missing.is_empty(),
            "add these keys to crates/vulcan-i18n/catalogs (tui.* keys go to the tui surface): {missing:?}"
        );
    }

    #[test]
    fn every_tui_catalog_key_is_used() {
        let used = used_keys();
        let unused: Vec<_> = TUI
            .messages(vulcan_i18n::DEFAULT_LOCALE)
            .into_keys()
            .filter(|key| !used.contains(key))
            .collect();
        assert!(
            unused.is_empty(),
            "remove TUI catalog keys no code uses: {unused:?}"
        );
    }
}
