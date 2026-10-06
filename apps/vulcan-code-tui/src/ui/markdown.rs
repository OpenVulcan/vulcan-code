// Minimal Markdown renderer for assistant text.
// 助手文本的最小 Markdown 渲染器。
//
// Covers what assistant replies use most: headings, paragraphs, bullet and numbered lists, block
// quotes, fenced code, inline code, bold and italic. Tables and HTML are shown as plain text. It
// is line-oriented so a half-streamed reply renders stably while text is still arriving.
// 覆盖助手回复最常用的元素：标题、段落、无序与有序列表、引用、代码块、行内代码、粗体与斜体。
// 表格与 HTML 按纯文本显示。按行处理，因此流式输出到一半的回复也能稳定渲染。

use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use super::text::{width_of, wrap};
use super::theme::{Theme, mix};

/// Renders `source` into lines at most `width` columns wide.
/// 将 `source` 渲染为不超过 `width` 列的行。
pub fn render(source: &str, width: usize, theme: &Theme) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let mut in_code = false;
    let code_bg = mix(theme.muted, theme.bg, 0.7);
    for raw in source.lines() {
        let trimmed = raw.trim_start();
        if trimmed.starts_with("```") {
            in_code = !in_code;
            if in_code {
                let lang = trimmed.trim_start_matches('`').trim();
                let label = if lang.is_empty() {
                    String::new()
                } else {
                    format!(" {lang} ")
                };
                lines.push(Line::from(vec![
                    Span::styled("╭─".to_string(), Style::new().fg(theme.border)),
                    Span::styled(label, theme.muted_text()),
                ]));
            } else {
                lines.push(Line::from(Span::styled(
                    "╰─".to_string(),
                    Style::new().fg(theme.border),
                )));
            }
            continue;
        }
        if in_code {
            let bar = Span::styled("│ ".to_string(), Style::new().fg(theme.border));
            let body = Span::styled(
                raw.replace('\t', "    "),
                Style::new().fg(theme.teal).bg(code_bg),
            );
            lines.extend(wrap(
                &[body],
                width,
                std::slice::from_ref(&bar),
                std::slice::from_ref(&bar),
            ));
            continue;
        }
        if trimmed.is_empty() {
            lines.push(Line::default());
            continue;
        }
        if let Some((level, title)) = heading(trimmed) {
            let style = match level {
                1 => Style::new().fg(theme.primary).add_modifier(Modifier::BOLD),
                2 => theme.strong(),
                _ => theme.strong().add_modifier(Modifier::ITALIC),
            };
            lines.extend(wrap(&inline(title, style, theme), width, &[], &[]));
            continue;
        }
        if let Some(quote) = trimmed.strip_prefix('>') {
            let bar = Span::styled("┃ ".to_string(), Style::new().fg(theme.border));
            let spans = inline(quote.trim_start(), theme.muted_text(), theme);
            lines.extend(wrap(
                &spans,
                width,
                std::slice::from_ref(&bar),
                std::slice::from_ref(&bar),
            ));
            continue;
        }
        if is_rule(trimmed) {
            lines.push(Line::from(Span::styled(
                "─".repeat(width.min(48)),
                Style::new().fg(theme.border),
            )));
            continue;
        }
        let indent = " ".repeat((raw.len() - trimmed.len()).min(8));
        if let Some((marker, body)) = list_item(trimmed) {
            let first = vec![
                Span::raw(indent.clone()),
                Span::styled(marker.clone(), theme.primary_text()),
            ];
            let rest = vec![Span::raw(format!(
                "{indent}{}",
                " ".repeat(width_of(&marker))
            ))];
            lines.extend(wrap(
                &inline(body, theme.base(), theme),
                width,
                &first,
                &rest,
            ));
            continue;
        }
        lines.extend(wrap(&inline(trimmed, theme.base(), theme), width, &[], &[]));
    }
    lines
}

fn heading(line: &str) -> Option<(usize, &str)> {
    let level = line.chars().take_while(|c| *c == '#').count();
    if (1..=6).contains(&level) && line[level..].starts_with(' ') {
        Some((level, line[level..].trim()))
    } else {
        None
    }
}

fn is_rule(line: &str) -> bool {
    let compact: String = line.chars().filter(|c| !c.is_whitespace()).collect();
    compact.len() >= 3 && (compact.chars().all(|c| c == '-') || compact.chars().all(|c| c == '*'))
}

fn list_item(line: &str) -> Option<(String, &str)> {
    for bullet in ["- [ ] ", "- [x] ", "- ", "* ", "+ "] {
        if let Some(body) = line.strip_prefix(bullet) {
            let marker = match bullet {
                "- [ ] " => "☐ ",
                "- [x] " => "☑ ",
                _ => "• ",
            };
            return Some((marker.to_string(), body));
        }
    }
    let digits = line.chars().take_while(char::is_ascii_digit).count();
    if digits > 0 && digits <= 3 {
        let rest = &line[digits..];
        if let Some(body) = rest.strip_prefix(". ").or_else(|| rest.strip_prefix(") ")) {
            return Some((format!("{}. ", &line[..digits]), body));
        }
    }
    None
}

/// Splits inline markup (`code`, **bold**, *italic*) into styled spans.
/// 将行内标记（`code`、**粗体**、*斜体*）拆分为带样式的片段。
pub fn inline(text: &str, base: Style, theme: &Theme) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    let mut buffer = String::new();
    let mut bold = false;
    let mut italic = false;
    let chars: Vec<char> = text.chars().collect();
    let style_of = |bold: bool, italic: bool| {
        let mut style = base;
        if bold {
            style = style.add_modifier(Modifier::BOLD);
        }
        if italic {
            style = style.add_modifier(Modifier::ITALIC);
        }
        style
    };
    let mut i = 0;
    while i < chars.len() {
        let ch = chars[i];
        if ch == '`'
            && let Some(end) = chars[i + 1..].iter().position(|c| *c == '`')
        {
            flush(&mut spans, &mut buffer, style_of(bold, italic));
            let code: String = chars[i + 1..i + 1 + end].iter().collect();
            spans.push(Span::styled(
                code,
                Style::new()
                    .fg(theme.teal)
                    .bg(mix(theme.muted, theme.bg, 0.7)),
            ));
            i += end + 2;
            continue;
        }
        if ch == '*' && chars.get(i + 1) == Some(&'*') {
            flush(&mut spans, &mut buffer, style_of(bold, italic));
            bold = !bold;
            i += 2;
            continue;
        }
        if ch == '*' && chars.get(i + 1).is_some_and(|c| !c.is_whitespace())
            || (ch == '*' && italic)
        {
            flush(&mut spans, &mut buffer, style_of(bold, italic));
            italic = !italic;
            i += 1;
            continue;
        }
        buffer.push(ch);
        i += 1;
    }
    flush(&mut spans, &mut buffer, style_of(bold, italic));
    spans
}

fn flush(spans: &mut Vec<Span<'static>>, buffer: &mut String, style: Style) {
    if !buffer.is_empty() {
        spans.push(Span::styled(std::mem::take(buffer), style));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text_of(line: &Line) -> String {
        line.spans.iter().map(|s| s.content.as_ref()).collect()
    }

    #[test]
    fn renders_lists_code_and_inline_markup() {
        let theme = Theme::dark();
        let lines = render(
            "## Title\n- item **bold** `code`\n```rust\nfn x() {}\n```",
            40,
            &theme,
        );
        let texts: Vec<String> = lines.iter().map(text_of).collect();
        assert_eq!(texts[0], "Title");
        assert_eq!(texts[1], "• item bold code");
        assert!(texts[2].starts_with("╭─"));
        assert_eq!(texts[3], "│ fn x() {}");
        assert!(texts[4].starts_with("╰─"));
        let bold = &lines[1]
            .spans
            .iter()
            .find(|s| s.content == "bold")
            .expect("bold span");
        assert!(bold.style.add_modifier.contains(Modifier::BOLD));
    }
}
