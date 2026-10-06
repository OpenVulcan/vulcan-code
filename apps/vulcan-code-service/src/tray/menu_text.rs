//! Tray text that follows the interface language the user saved.
//! 跟随用户保存的界面语言的托盘文本。
//!
//! Labels come from the shared `tray` catalog surface; the tray never keeps its own word list.
//! 文案来自共享的 `tray` 目录分面；托盘不维护自己的词表。

use vulcan_i18n::{TRAY, Translator};

/// Catalog keys used by the tray; each must exist in the default `tray` catalog.
/// 托盘使用的目录键；每个都必须存在于默认 `tray` 目录中。
pub(super) mod keys {
    /// Opens the desktop client.
    /// 打开桌面客户端。
    pub const OPEN_DESKTOP_CLIENT: &str = "openDesktopClient";
    /// Opens the WebUI in the system browser.
    /// 在系统浏览器中打开 WebUI。
    pub const OPEN_WEBUI: &str = "openWebui";
    /// Opens the terminal client in a new console.
    /// 在新控制台中打开终端客户端。
    pub const OPEN_TERMINAL_CLIENT: &str = "openTerminalClient";
    /// Reveals the service log location.
    /// 展示服务日志位置。
    pub const OPEN_LOG_LOCATION: &str = "openLogLocation";
    /// Quits the service.
    /// 退出服务。
    pub const QUIT_SERVICE: &str = "quitService";
    /// Status line for each protected lifecycle state.
    /// 每种受保护生命周期状态的状态行。
    pub const STATUS_READY: &str = "statusReady";
    /// Status line while the service starts.
    /// 服务启动中的状态行。
    pub const STATUS_STARTING: &str = "statusStarting";
    /// Status line while the service stops.
    /// 服务停止中的状态行。
    pub const STATUS_STOPPING: &str = "statusStopping";
    /// Status line while the service recovers.
    /// 服务恢复中的状态行。
    pub const STATUS_RECOVERING: &str = "statusRecovering";
    /// Status line after a service failure.
    /// 服务失败后的状态行。
    pub const STATUS_FAILED: &str = "statusFailed";
    /// Status line after a failed recovery.
    /// 恢复失败后的状态行。
    pub const STATUS_RECOVERY_FAILED: &str = "statusRecoveryFailed";
    /// Status line after the HTTP bridge failed; takes `{reason}`.
    /// HTTP 桥失败后的状态行；接收 `{reason}`。
    pub const STATUS_BRIDGE_FAILED: &str = "statusBridgeFailed";
    /// Notice that the WebUI address is not published yet.
    /// WebUI 地址尚未发布的提示。
    pub const WEBUI_UNPUBLISHED: &str = "webuiUnpublished";
    /// Notice that the desktop client could not open; takes `{reason}`.
    /// 桌面客户端无法打开的提示；接收 `{reason}`。
    pub const DESKTOP_CLIENT_FAILED: &str = "desktopClientFailed";
    /// Notice that the terminal client could not open; takes `{reason}`.
    /// 终端客户端无法打开的提示；接收 `{reason}`。
    pub const TERMINAL_CLIENT_FAILED: &str = "terminalClientFailed";
    /// Notice that the log location could not open; takes `{reason}`.
    /// 日志位置无法打开的提示；接收 `{reason}`。
    pub const LOG_LOCATION_FAILED: &str = "logLocationFailed";
    /// Notice that the HTTP bridge failed; takes `{reason}`.
    /// HTTP 桥失败的提示；接收 `{reason}`。
    pub const BRIDGE_FAILED: &str = "bridgeFailed";
    /// Notice that shutdown failed; takes `{reason}`.
    /// 关闭失败的提示；接收 `{reason}`。
    pub const SHUTDOWN_FAILED: &str = "shutdownFailed";
    /// Notice that the shutdown coordinator could not start; takes `{reason}`.
    /// 关闭协调器无法启动的提示；接收 `{reason}`。
    pub const SHUTDOWN_START_FAILED: &str = "shutdownStartFailed";

    /// Every key above, so tests can prove each one resolves.
    /// 上述全部键，供测试证明每个键都能解析。
    #[cfg(test)]
    pub const ALL: &[&str] = &[
        OPEN_DESKTOP_CLIENT,
        OPEN_WEBUI,
        OPEN_TERMINAL_CLIENT,
        OPEN_LOG_LOCATION,
        QUIT_SERVICE,
        STATUS_READY,
        STATUS_STARTING,
        STATUS_STOPPING,
        STATUS_RECOVERING,
        STATUS_FAILED,
        STATUS_RECOVERY_FAILED,
        STATUS_BRIDGE_FAILED,
        WEBUI_UNPUBLISHED,
        DESKTOP_CLIENT_FAILED,
        TERMINAL_CLIENT_FAILED,
        LOG_LOCATION_FAILED,
        BRIDGE_FAILED,
        SHUTDOWN_FAILED,
        SHUTDOWN_START_FAILED,
    ];
}

/// Tray translator bound to the saved interface language, or the system language without one.
/// 绑定到已保存界面语言的托盘翻译器；未保存时使用系统语言。
pub(super) struct TrayText {
    /// Active catalog lookup for the `tray` surface.
    /// `tray` 分面的当前目录查找。
    translator: Translator,
    /// Language used while the user has not saved one; read once when the tray starts.
    /// 用户未保存语言时使用的语言；托盘启动时读取一次。
    system_locale: &'static str,
}

impl TrayText {
    /// Creates tray text in the system language until a saved language is observed.
    /// 创建托盘文本；观察到已保存语言之前使用系统语言。
    pub(super) fn new() -> Self {
        Self::with_system_locale(vulcan_i18n::system_locale())
    }

    /// Creates tray text with an explicit fallback language.
    /// 使用显式回退语言创建托盘文本。
    /// `system_locale` is a registered code; returns tray text already set to it.
    /// `system_locale` 是已注册代码；返回已切换到该语言的托盘文本。
    fn with_system_locale(system_locale: &'static str) -> Self {
        let mut translator = Translator::new(&[&TRAY]);
        translator.set_locale(system_locale);
        Self {
            translator,
            system_locale,
        }
    }

    /// Switches to the saved language, or back to the system language when none is saved.
    /// 切换到已保存语言；未保存时回到系统语言。
    /// `saved` is the persisted `uiLocale`; returns true only when the active language changed.
    /// `saved` 是已持久化的 `uiLocale`；仅在当前语言改变时返回 true。
    pub(super) fn follow(&mut self, saved: Option<&str>) -> bool {
        let target = saved
            .and_then(vulcan_i18n::normalize)
            .unwrap_or(self.system_locale);
        target != self.translator.locale() && self.translator.set_locale(target)
    }

    /// Returns the text for one key; the key itself shows only if the catalog lost it.
    /// 返回某个键的文本；仅当目录缺失该键时显示键名本身。
    pub(super) fn get<'a>(&'a self, key: &'a str) -> &'a str {
        self.translator.text(key).unwrap_or(key)
    }

    /// Returns the text for one key with its `{reason}` placeholder filled.
    /// 返回填好 `{reason}` 占位符的键文本。
    pub(super) fn with_reason(&self, key: &str, reason: &str) -> String {
        vulcan_i18n::format_template(self.get(key), &[("reason", reason)])
    }
}

#[cfg(test)]
mod tests {
    use super::{TrayText, keys};

    /// Every key the tray uses resolves in every registered language.
    /// 托盘使用的每个键在每种已注册语言中都能解析。
    #[test]
    fn every_tray_key_resolves_in_every_language() {
        for locale in vulcan_i18n::LOCALES {
            let text = TrayText::with_system_locale(locale.code);
            for key in keys::ALL {
                assert!(
                    text.translator.text(key).is_some(),
                    "{} lacks tray key {key}",
                    locale.code
                );
            }
        }
    }

    /// A saved language wins, an invalid or cleared one falls back to the system language,
    /// and repeating the current language reports no change.
    /// 已保存语言优先；无效或清除时回退到系统语言；重复当前语言时报告无变化。
    #[test]
    fn follows_the_saved_language_and_falls_back_to_the_system() {
        let mut text = TrayText::with_system_locale("en-US");
        assert!(text.follow(Some("zh-TW")));
        assert_eq!(text.translator.locale(), "zh-TW");
        assert!(!text.follow(Some("zh-TW")));
        assert!(text.follow(Some("not-a-locale")));
        assert_eq!(text.translator.locale(), "en-US");
        assert!(text.follow(Some("ja-JP")));
        assert!(text.follow(None));
        assert_eq!(text.translator.locale(), "en-US");
    }

    /// The reason placeholder is filled in the active language's template.
    /// 原因占位符在当前语言模板中被填充。
    #[test]
    fn fills_the_reason_placeholder() {
        let text = TrayText::with_system_locale("en-US");
        let message = text.with_reason(keys::SHUTDOWN_FAILED, "drain timed out");
        assert!(message.contains("drain timed out"));
        assert!(!message.contains("{reason}"));
    }
}
