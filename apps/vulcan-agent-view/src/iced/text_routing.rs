//! Script-aware text-span routing helpers shared by transcript rendering paths.
//! 转录渲染路径共享的脚本感知文本片段路由辅助逻辑。

use super::scene::{MarkdownRenderKind, PlainTextRenderKind, TranscriptScript};
use crate::iced_theme as theme;
use iced::widget::markdown;
use iced::widget::text::Span;
use iced::{Color, Font, Theme};

/// Builds routed rich-text spans from one parsed Markdown text payload while preserving links and emphasis metadata.
/// 从一段已解析 Markdown 文本载荷构建带路由的富文本片段，同时保留链接与强调元数据。
pub(super) fn build_agent_view_markdown_spans(
    text: &markdown::Text,
    style: markdown::Style,
    render_kind: MarkdownRenderKind,
    _app_theme: &Theme,
) -> Vec<Span<'static, markdown::Uri, Font>> {
    let base_spans = text.spans(style);
    let mut output = Vec::new();

    for source_span in base_spans.iter() {
        let span_uses_code_branch = matches!(render_kind, MarkdownRenderKind::CodeBlock)
            || source_span.highlight.is_some()
            || source_span.font == Some(style.inline_code_font);
        let requested_font = source_span.font.unwrap_or(if span_uses_code_branch {
            style.code_block_font
        } else {
            style.font
        });

        for (script, fragment) in split_markdown_fragment(source_span.text.as_ref()) {
            let routed_font = match (span_uses_code_branch, script) {
                (true, TranscriptScript::Latin) => theme::normalize_code_latin_font(requested_font),
                (true, TranscriptScript::Cjk) => theme::normalize_code_cjk_font(requested_font),
                (false, TranscriptScript::Latin) => theme::normalize_ui_latin_font(requested_font),
                (false, TranscriptScript::Cjk) => theme::normalize_ui_cjk_font(requested_font),
            };

            let mut routed_span = source_span.clone();
            routed_span.text = fragment.into();
            routed_span.font = Some(routed_font);
            output.push(routed_span);
        }
    }

    output
}

/// Builds routed rich-text spans from one plain-text payload while preserving Latin and CJK fallback strategy.
/// 从一段纯文本载荷构建带路由的富文本片段，同时保留拉丁与 CJK 的兜底策略。
pub(super) fn build_agent_view_plain_text_spans(
    text: &str,
    requested_font: Font,
    render_kind: PlainTextRenderKind,
    color: Option<Color>,
    _app_theme: &Theme,
) -> Vec<Span<'static, markdown::Uri, Font>> {
    let fragments = if text.is_empty() {
        vec![(TranscriptScript::Latin, String::from(" "))]
    } else {
        split_markdown_fragment(text)
    };
    let mut output = Vec::new();

    for (script, fragment) in fragments {
        let routed_font = match (render_kind, script) {
            (PlainTextRenderKind::Code, TranscriptScript::Latin) => {
                theme::normalize_code_latin_font(requested_font)
            }
            (PlainTextRenderKind::Code, TranscriptScript::Cjk) => {
                theme::normalize_code_cjk_font(requested_font)
            }
            (PlainTextRenderKind::Flow, TranscriptScript::Latin) => {
                theme::normalize_ui_latin_font(requested_font)
            }
            (PlainTextRenderKind::Flow, TranscriptScript::Cjk) => {
                theme::normalize_ui_cjk_font(requested_font)
            }
        };

        let mut routed_span = Span::new(fragment).font(routed_font);
        if let Some(color) = color {
            routed_span = routed_span.color(color);
        }
        output.push(routed_span);
    }

    output
}

/// Splits one fragment into routed Latin-or-CJK runs while keeping neutral punctuation attached to surrounding scripts.
/// 将一段文本切分为路由后的拉丁或 CJK 片段，同时让中性标点依附于周围脚本。
fn split_markdown_fragment(fragment: &str) -> Vec<(TranscriptScript, String)> {
    let mut runs = Vec::new();
    let mut active_fragment = String::new();
    let mut active_script = None;

    for character in fragment.chars() {
        let character_script = if is_neutral_character(character) {
            active_script.unwrap_or(TranscriptScript::Latin)
        } else {
            classify_script(character)
        };

        if active_script.is_some() && active_script != Some(character_script) {
            runs.push((
                active_script.unwrap_or(TranscriptScript::Latin),
                std::mem::take(&mut active_fragment),
            ));
        }

        active_script = Some(character_script);
        active_fragment.push(character);
    }

    if !active_fragment.is_empty() {
        runs.push((
            active_script.unwrap_or(TranscriptScript::Latin),
            active_fragment,
        ));
    }

    runs
}

/// Returns whether one character should follow the surrounding script bucket instead of opening a new run.
/// 返回某个字符是否应跟随周围脚本分组，而不是开启新的片段。
fn is_neutral_character(character: char) -> bool {
    character.is_whitespace()
        || character.is_ascii_punctuation()
        || matches!(
            character,
            '·' | '•' | '‧' | '…' | '—' | '–' | '―' | '“' | '”' | '‘' | '’'
        )
}

/// Classifies one character into the transcript Latin-or-CJK routing buckets.
/// 将某个字符归类到转录渲染使用的拉丁或 CJK 路由分组中。
fn classify_script(character: char) -> TranscriptScript {
    if matches!(
        character as u32,
        0x2E80..=0x2FDF
            | 0x3040..=0x30FF
            | 0x3100..=0x312F
            | 0x31A0..=0x31BF
            | 0x3400..=0x4DBF
            | 0x4E00..=0x9FFF
            | 0xAC00..=0xD7AF
            | 0xF900..=0xFAFF
            | 0xFE30..=0xFE4F
            | 0xFF00..=0xFFEF
            | 0x20000..=0x3FFFD
    ) {
        TranscriptScript::Cjk
    } else {
        TranscriptScript::Latin
    }
}
