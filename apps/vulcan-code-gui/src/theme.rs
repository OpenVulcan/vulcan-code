//! Theme helpers for the AI coding desktop scaffold.
//! AI 编程桌面骨架使用的主题辅助逻辑。

use iced::highlighter;
use iced::{Font, Theme};

/// Bundled UI font family name used across all supported platforms.
/// 所有支持平台统一使用的内置界面字体族名称。
const BUNDLED_UI_FONT_NAME: &str = "Noto Sans CJK SC";

/// Bundled code font family name used across all supported platforms.
/// 所有支持平台统一使用的内置代码字体族名称。
const BUNDLED_CODE_FONT_NAME: &str = "Victor Mono";

/// Bundled UI font bytes loaded from the application resources directory.
/// 从应用资源目录加载的内置界面字体字节。
const BUNDLED_UI_FONT_BYTES: &[u8] = include_bytes!("../resources/fonts/NotoSansCJKsc-Regular.otf");

/// Bundled code font bytes loaded from the application resources directory.
/// 从应用资源目录加载的内置代码字体字节。
const BUNDLED_CODE_FONT_BYTES: &[u8] = include_bytes!("../resources/fonts/VictorMono-Regular.ttf");

/// Returns the default application theme for the prototype.
/// 返回原型应用的默认主题。
pub fn default_theme() -> Theme {
    Theme::TokyoNight
}

/// Returns the bundled UI font bytes that should be registered at startup.
/// 返回启动时应注册的内置界面字体字节。
pub fn ui_font_bytes() -> &'static [u8] {
    BUNDLED_UI_FONT_BYTES
}

/// Returns the bundled code font bytes that should be registered at startup.
/// 返回启动时应注册的内置代码字体字节。
pub fn code_font_bytes() -> &'static [u8] {
    BUNDLED_CODE_FONT_BYTES
}

/// Returns the bundled UI font used by normal text widgets.
/// 返回普通文本控件使用的内置界面字体。
pub fn ui_font() -> Font {
    Font::with_name(BUNDLED_UI_FONT_NAME)
}

/// Returns the bundled code font used by source-like widgets.
/// 返回源码类控件使用的内置代码字体。
pub fn code_font() -> Font {
    Font::with_name(BUNDLED_CODE_FONT_NAME)
}

/// Maps the application theme to a matching code highlighter theme.
/// 将应用主题映射为匹配的代码高亮主题。
pub fn code_theme(theme: &Theme) -> highlighter::Theme {
    match theme {
        Theme::Light | Theme::TokyoNightLight | Theme::SolarizedLight | Theme::CatppuccinLatte => {
            highlighter::Theme::InspiredGitHub
        }
        Theme::SolarizedDark => highlighter::Theme::SolarizedDark,
        Theme::CatppuccinMocha | Theme::CatppuccinMacchiato => highlighter::Theme::Base16Mocha,
        Theme::Dracula | Theme::Nord | Theme::GruvboxDark => highlighter::Theme::Base16Eighties,
        _ => highlighter::Theme::Base16Ocean,
    }
}
