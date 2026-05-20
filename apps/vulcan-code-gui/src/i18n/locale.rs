//! Supported GUI locale identifiers and their display labels.
//! 受支持的图形界面语言标识及其显示标签。

use std::fmt;

/// One supported GUI locale exposed to the language picker and persisted configuration.
/// 暴露给语言选择器与持久化配置的一条受支持图形界面语言。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum AppLocale {
    /// Simplified Chinese locale used as the default fallback language.
    /// 作为默认回退语言使用的简体中文区域设置。
    #[default]
    ZhCn,
    /// English locale used by the first public translation pack.
    /// 首批公开翻译包使用的英文区域设置。
    EnUs,
}

impl AppLocale {
    /// Stable ordered locale list rendered by the GUI picker.
    /// 供图形界面选择器渲染的稳定有序语言列表。
    pub const ALL: [Self; 2] = [Self::ZhCn, Self::EnUs];

    /// Returns the stable persisted locale identifier.
    /// 返回稳定的持久化语言标识。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ZhCn => "zh-CN",
            Self::EnUs => "en-US",
        }
    }

    /// Returns the locale-native human-readable name.
    /// 返回该语言自身使用的人类可读名称。
    pub const fn native_name(self) -> &'static str {
        match self {
            Self::ZhCn => "简体中文",
            Self::EnUs => "English (US)",
        }
    }

    /// Parses one persisted locale identifier into a supported GUI locale.
    /// 将一条持久化语言标识解析为受支持的图形界面语言。
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim() {
            "zh-CN" | "zh_CN" | "zh" => Some(Self::ZhCn),
            "en-US" | "en_US" | "en" => Some(Self::EnUs),
            _ => None,
        }
    }
}

impl fmt::Display for AppLocale {
    /// Renders the stable picker label used by the workbench locale selector.
    /// 渲染工作台语言选择器使用的稳定显示标签。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{} ({})", self.native_name(), self.as_str())
    }
}
