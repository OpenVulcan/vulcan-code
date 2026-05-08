//! Theme helpers for the AI coding desktop scaffold.
//! AI 编程桌面骨架使用的主题辅助逻辑。

use iced::highlighter;
use iced::widget::markdown;
use iced::{
    Font, Theme,
    font::{Style as FontStyle, Weight as FontWeight},
};

/// Bundled UI font family name used across all supported platforms.
/// 所有支持平台统一使用的内置界面字体族名称。
const BUNDLED_UI_FONT_NAME: &str = "Roboto";

/// Bundled fallback CJK UI font family name used when the primary Latin family misses one glyph.
/// 主拉丁界面字体缺失字形时使用的内置 CJK 兜底界面字体族名称。
const BUNDLED_CJK_UI_FONT_NAME: &str = "Noto Sans CJK SC";

/// Bundled code font family name used across all supported platforms.
/// 所有支持平台统一使用的内置代码字体族名称。
const BUNDLED_CODE_FONT_NAME: &str = "Victor Mono";

/// Bundled CJK-safe monospace font family name used for italic highlight fallback.
/// 用于斜体高亮回退的内置 CJK 安全等宽字体族名称。
const BUNDLED_CJK_CODE_FONT_NAME: &str = "Noto Sans Mono CJK SC";

/// Bundled UI font bytes loaded from the application resources directory.
/// 从应用资源目录加载的内置界面字体字节。
const BUNDLED_UI_FONT_BYTES: &[u8] = include_bytes!("../resources/fonts/Roboto-Regular.ttf");

/// Bundled italic UI font bytes loaded from the application resources directory.
/// 从应用资源目录加载的内置斜体界面字体字节。
const BUNDLED_UI_ITALIC_FONT_BYTES: &[u8] = include_bytes!("../resources/fonts/Roboto-Italic.ttf");

/// Bundled medium UI font bytes loaded from the application resources directory.
/// 从应用资源目录加载的内置中等字重界面字体字节。
const BUNDLED_UI_MEDIUM_FONT_BYTES: &[u8] = include_bytes!("../resources/fonts/Roboto-Medium.ttf");

/// Bundled medium italic UI font bytes loaded from the application resources directory.
/// 从应用资源目录加载的内置中等字重斜体界面字体字节。
const BUNDLED_UI_MEDIUM_ITALIC_FONT_BYTES: &[u8] =
    include_bytes!("../resources/fonts/Roboto-MediumItalic.ttf");

/// Bundled bold UI font bytes loaded from the application resources directory.
/// 从应用资源目录加载的内置粗体界面字体字节。
const BUNDLED_UI_BOLD_FONT_BYTES: &[u8] = include_bytes!("../resources/fonts/Roboto-Bold.ttf");

/// Bundled bold italic UI font bytes loaded from the application resources directory.
/// 从应用资源目录加载的内置粗斜体界面字体字节。
const BUNDLED_UI_BOLD_ITALIC_FONT_BYTES: &[u8] =
    include_bytes!("../resources/fonts/Roboto-BoldItalic.ttf");

/// Bundled fallback CJK UI font bytes loaded from the application resources directory.
/// 从应用资源目录加载的内置 CJK 兜底界面字体字节。
const BUNDLED_CJK_UI_FONT_BYTES: &[u8] =
    include_bytes!("../resources/fonts/NotoSansCJKsc-Regular.otf");

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

/// Returns the bundled italic UI font bytes that should be registered at startup.
/// 返回启动时应注册的内置斜体界面字体字节。
pub fn ui_italic_font_bytes() -> &'static [u8] {
    BUNDLED_UI_ITALIC_FONT_BYTES
}

/// Returns the bundled medium UI font bytes that should be registered at startup.
/// 返回启动时应注册的内置中等字重界面字体字节。
pub fn ui_medium_font_bytes() -> &'static [u8] {
    BUNDLED_UI_MEDIUM_FONT_BYTES
}

/// Returns the bundled medium italic UI font bytes that should be registered at startup.
/// 返回启动时应注册的内置中等字重斜体界面字体字节。
pub fn ui_medium_italic_font_bytes() -> &'static [u8] {
    BUNDLED_UI_MEDIUM_ITALIC_FONT_BYTES
}

/// Returns the bundled bold UI font bytes that should be registered at startup.
/// 返回启动时应注册的内置粗体界面字体字节。
pub fn ui_bold_font_bytes() -> &'static [u8] {
    BUNDLED_UI_BOLD_FONT_BYTES
}

/// Returns the bundled bold italic UI font bytes that should be registered at startup.
/// 返回启动时应注册的内置粗斜体界面字体字节。
pub fn ui_bold_italic_font_bytes() -> &'static [u8] {
    BUNDLED_UI_BOLD_ITALIC_FONT_BYTES
}

/// Returns the bundled fallback CJK UI font bytes that should be registered at startup.
/// 返回启动时应注册的内置 CJK 兜底界面字体字节。
pub fn ui_cjk_font_bytes() -> &'static [u8] {
    BUNDLED_CJK_UI_FONT_BYTES
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

/// Returns the bundled fallback CJK UI font used when one rendered span contains East Asian content.
/// 返回渲染区间包含东亚文本时使用的内置 CJK 兜底界面字体。
pub fn ui_cjk_font() -> Font {
    Font::with_name(BUNDLED_CJK_UI_FONT_NAME)
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

/// Returns one Markdown style that keeps prose on the Latin UI family and code on the bundled monospace family.
/// 返回一份 Markdown 样式，使正文使用拉丁界面字体族、代码使用内置等宽字体族。
pub fn markdown_style(theme: &Theme) -> markdown::Style {
    let mut style = markdown::Style::from(theme);
    style.font = ui_font();
    style.inline_code_font = code_font();
    style.code_block_font = code_font();
    style
}

/// Returns one Markdown settings bundle aligned with the custom transcript typography defaults.
/// 返回与自定义转录排版默认值对齐的一组 Markdown 设置。
pub fn markdown_settings(theme: &Theme) -> markdown::Settings {
    markdown::Settings::with_text_size(14.0, markdown_style(theme))
}

/// Normalizes one requested prose font onto the Latin UI family while preserving the requested emphasis metadata.
/// 将某个正文请求字体归一化到拉丁界面字体族，同时保留请求的强调元数据。
pub fn normalize_ui_latin_font(requested: Font) -> Font {
    Font {
        family: ui_font().family,
        ..requested
    }
}

/// Normalizes one requested prose font onto the fallback CJK family and gracefully downgrades unsupported emphasis styles.
/// 将某个正文请求字体归一化到 CJK 兜底字体族，并对不支持的强调样式做优雅降级。
pub fn normalize_ui_cjk_font(requested: Font) -> Font {
    Font {
        family: ui_cjk_font().family,
        weight: FontWeight::Normal,
        style: FontStyle::Normal,
        ..requested
    }
}

/// Normalizes one requested code font onto the Latin monospace family while preserving the requested emphasis metadata.
/// 将某个代码请求字体归一化到拉丁等宽字体族，同时保留请求的强调元数据。
pub fn normalize_code_latin_font(requested: Font) -> Font {
    Font {
        family: code_font().family,
        ..requested
    }
}

/// Normalizes one requested code font onto the CJK monospace family and gracefully downgrades unsupported code weights.
/// 将某个代码请求字体归一化到 CJK 等宽字体族，并对不支持的代码字重做优雅降级。
pub fn normalize_code_cjk_font(requested: Font) -> Font {
    Font {
        family: cjk_code_font().family,
        weight: FontWeight::Normal,
        style: if matches!(requested.style, FontStyle::Italic | FontStyle::Oblique) {
            FontStyle::Italic
        } else {
            FontStyle::Normal
        },
        ..requested
    }
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
