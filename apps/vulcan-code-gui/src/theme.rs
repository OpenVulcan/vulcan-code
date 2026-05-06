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

/// Bundled CJK-safe monospace font family name used for italic highlight fallback.
/// 用于斜体高亮回退的内置 CJK 安全等宽字体族名称。
const BUNDLED_CJK_CODE_FONT_NAME: &str = "Noto Sans Mono CJK SC";

/// Bundled UI font bytes loaded from the application resources directory.
/// 从应用资源目录加载的内置界面字体字节。
const BUNDLED_UI_FONT_BYTES: &[u8] = include_bytes!("../resources/fonts/NotoSansCJKsc-Regular.otf");

/// Bundled code font bytes loaded from the application resources directory.
/// 从应用资源目录加载的内置代码字体字节。
const BUNDLED_CODE_FONT_BYTES: &[u8] = include_bytes!("../resources/fonts/VictorMono-Regular.ttf");

/// Bundled italic code font bytes loaded from the application resources directory.
/// 从应用资源目录加载的内置斜体代码字体字节。
const BUNDLED_CODE_ITALIC_FONT_BYTES: &[u8] =
    include_bytes!("../resources/fonts/VictorMono-Italic.ttf");

/// Bundled CJK monospace code font bytes loaded from the application resources directory.
/// 从应用资源目录加载的内置 CJK 等宽代码字体字节。
const BUNDLED_CJK_CODE_FONT_BYTES: &[u8] =
    include_bytes!("../resources/fonts/NotoSansMonoCJKsc-Regular.ttf");

/// Bundled CJK monospace italic code font bytes loaded from the application resources directory.
/// 从应用资源目录加载的内置 CJK 等宽斜体代码字体字节。
const BUNDLED_CJK_CODE_ITALIC_FONT_BYTES: &[u8] =
    include_bytes!("../resources/fonts/NotoSansMonoCJKsc-Italic.ttf");

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

/// Returns the bundled italic code font bytes that should be registered at startup.
/// 返回启动时应注册的内置斜体代码字体字节。
pub fn code_italic_font_bytes() -> &'static [u8] {
    BUNDLED_CODE_ITALIC_FONT_BYTES
}

/// Returns the bundled CJK monospace code font bytes that should be registered at startup.
/// 返回启动时应注册的内置 CJK 等宽代码字体字节。
pub fn cjk_code_font_bytes() -> &'static [u8] {
    BUNDLED_CJK_CODE_FONT_BYTES
}

/// Returns the bundled CJK monospace italic code font bytes that should be registered at startup.
/// 返回启动时应注册的内置 CJK 等宽斜体代码字体字节。
pub fn cjk_code_italic_font_bytes() -> &'static [u8] {
    BUNDLED_CJK_CODE_ITALIC_FONT_BYTES
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

/// Returns the bundled CJK-safe monospace font used for italic highlight fallback.
/// 返回用于斜体高亮回退的内置 CJK 安全等宽字体。
pub fn cjk_code_font() -> Font {
    Font::with_name(BUNDLED_CJK_CODE_FONT_NAME)
}

/// Converts one syntax highlight token into a format that preserves the bundled code font family.
/// 将单个语法高亮标记转换为保留内置代码字体族的格式。
pub fn code_highlight_format(
    highlight: &highlighter::Highlight,
    _theme: &Theme,
) -> iced::advanced::text::highlighter::Format<Font> {
    let font = highlight.font().map(|highlight_font| {
        let mut merged_font = if matches!(
            highlight_font.style,
            iced::font::Style::Italic | iced::font::Style::Oblique
        ) {
            cjk_code_font()
        } else {
            code_font()
        };
        merged_font.weight = highlight_font.weight;
        merged_font.stretch = highlight_font.stretch;
        merged_font.style = highlight_font.style;
        merged_font
    });

    iced::advanced::text::highlighter::Format {
        color: highlight.color(),
        font,
    }
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
