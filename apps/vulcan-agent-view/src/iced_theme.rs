//! Iced-facing typography helpers shared by the agent-view renderer.
//! agent-view 渲染器共享的 Iced 排版辅助逻辑。

use iced::widget::markdown;
use iced::{
    Font, Theme,
    font::{Style as FontStyle, Weight as FontWeight},
};

/// Bundled UI font family name expected to be registered by the host application.
/// 由宿主应用负责注册的内置界面字体族名称。
const BUNDLED_UI_FONT_NAME: &str = "Roboto";

/// Bundled fallback CJK UI font family name used when the primary Latin family misses one glyph.
/// 主拉丁界面字体缺失字形时使用的内置 CJK 兜底界面字体族名称。
const BUNDLED_CJK_UI_FONT_NAME: &str = "Noto Sans CJK SC";

/// Bundled code font family name expected to be registered by the host application.
/// 由宿主应用负责注册的内置代码字体族名称。
const BUNDLED_CODE_FONT_NAME: &str = "Victor Mono";

/// Bundled CJK-safe monospace font family name used for italic highlight fallback.
/// 用于斜体高亮回退的内置 CJK 安全等宽字体族名称。
const BUNDLED_CJK_CODE_FONT_NAME: &str = "Noto Sans Mono CJK SC";

/// Returns the bundled UI font used by normal text widgets.
/// 返回普通文本控件使用的内置界面字体。
pub fn ui_font() -> Font {
    Font::new(BUNDLED_UI_FONT_NAME)
}

/// Returns the bundled fallback CJK UI font used when one rendered span contains East Asian content.
/// 返回渲染区间包含东亚文本时使用的内置 CJK 兜底界面字体。
pub fn ui_cjk_font() -> Font {
    Font::new(BUNDLED_CJK_UI_FONT_NAME)
}

/// Returns the bundled code font used by source-like widgets.
/// 返回源码类控件使用的内置代码字体。
pub fn code_font() -> Font {
    Font::new(BUNDLED_CODE_FONT_NAME)
}

/// Returns the bundled CJK-safe monospace font used for italic highlight fallback.
/// 返回用于斜体高亮回退的内置 CJK 安全等宽字体。
pub fn cjk_code_font() -> Font {
    Font::new(BUNDLED_CJK_CODE_FONT_NAME)
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

/// Returns one Markdown settings bundle aligned with the transcript typography defaults.
/// 返回与转录排版默认值对齐的一组 Markdown 设置。
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
