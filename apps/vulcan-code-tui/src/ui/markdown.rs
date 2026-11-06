// Minimal Markdown renderer for assistant text.
// 助手文本的最小 Markdown 渲染器。
//
// Covers what assistant replies use most: headings, paragraphs, bullet and numbered lists, block
// quotes, fenced code, Mermaid diagrams (`mermaid.rs`), pipe tables, inline code, bold and
// italic. HTML is shown as plain text. It is line-oriented so a half-streamed reply renders stably
// while text is still arriving.
// 覆盖助手回复最常用的元素：标题、段落、无序与有序列表、引用、代码块、Mermaid 图表（`mermaid.rs`）、
// 管道表格、行内代码、粗体与斜体。HTML 按纯文本显示。按行处理，因此流式输出到一半的回复也能稳定渲染。

use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};

use super::answer_components::{self, CodeSnapshots};
use super::mermaid;
use super::text::{width_of, wrap};
use super::theme::{Theme, mix};

/// Renders `source` into lines at most `width` columns wide.
/// 将 `source` 渲染为不超过 `width` 列的行。
pub fn render(source: &str, width: usize, theme: &Theme) -> Vec<Line<'static>> {
    render_with_code(source, width, theme, CodeSnapshots::None)
}

/// Counts the leading backticks of one left-trimmed line.
/// 统计一行（已去除左侧空白）开头的反引号数。
fn fence_tick_count(trimmed: &str) -> usize {
    trimmed.bytes().take_while(|byte| *byte == b'`').count()
}

/// Reports whether a left-trimmed line closes a fence opened with `opening_ticks` backticks.
/// 判断一行（已去除左侧空白）是否关闭以 `opening_ticks` 个反引号开启的围栏。
fn closes_fence(trimmed: &str, opening_ticks: usize) -> bool {
    let ticks = fence_tick_count(trimmed);
    ticks >= 3 && ticks >= opening_ticks && trimmed[ticks..].trim().is_empty()
}

/// Renders a reply whose `ui-code` excerpts read the captures in `code`.
/// 渲染一条回答，其 `ui-code` 摘录读取 `code` 中的抓取结果。
pub fn render_with_code(
    source: &str,
    width: usize,
    theme: &Theme,
    code: CodeSnapshots<'_>,
) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let mut in_code = false;
    // Backticks of the open code fence; only a bare fence at least this long closes it.
    // 当前代码围栏的反引号数；只有不短于它的空白围栏才能将其关闭。
    let mut fence_ticks = 0;
    // Unified diffs color their added and removed rows.
    // 统一差异为新增与删除行着色。
    let mut diff = false;
    let code_bg = mix(theme.muted, theme.bg, 0.7);
    let source: Vec<&str> = source.lines().collect();
    let mut index = 0;
    while index < source.len() {
        let raw = source[index];
        index += 1;
        let trimmed = raw.trim_start();
        let ticks = fence_tick_count(trimmed);
        if ticks >= 3 && (!in_code || closes_fence(trimmed, fence_ticks)) {
            in_code = !in_code;
            if in_code {
                fence_ticks = ticks;
                let lang = trimmed.trim_start_matches('`').trim();
                // A closed answer component renders natively; its closing fence needs at least as
                // many backticks as the opening one so nested container fences stay inside. While
                // it is still streaming, or when its syntax is invalid, it shows as code.
                // 已闭合的回答组件原生渲染；结束围栏的反引号数不得少于开始围栏，使容器内嵌围栏留在内部。
                // 仍在流式输出或语法无效时显示为代码。
                if lang.starts_with("ui-") {
                    let opening_ticks = trimmed.bytes().take_while(|byte| *byte == b'`').count();
                    let close = source[index..].iter().position(|line| {
                        let closing = line.trim_start();
                        let ticks = closing.bytes().take_while(|byte| *byte == b'`').count();
                        ticks >= opening_ticks && closing[ticks..].trim().is_empty()
                    });
                    if let Some((close, component)) = close.and_then(|close| {
                        Some((
                            close,
                            answer_components::render(
                                lang,
                                &source[index..index + close],
                                width,
                                theme,
                                code,
                            )?,
                        ))
                    }) {
                        lines.extend(component);
                        index += close + 1;
                        in_code = false;
                        continue;
                    }
                }
                diff = lang == "diff";
                let label = if lang.is_empty() {
                    String::new()
                } else {
                    format!(" {lang} ")
                };
                lines.push(Line::from(vec![
                    Span::styled("╭─".to_string(), Style::new().fg(theme.border)),
                    Span::styled(label, theme.muted_text()),
                ]));
                // A complete Mermaid block is drawn as a diagram inside the same frame; while it is
                // still streaming, or when it cannot be drawn, its source shows as code.
                // 完整的 Mermaid 代码块在同一框内绘制为图表；仍在流式输出或无法绘制时显示源码。
                let close = source[index..]
                    .iter()
                    .position(|line| closes_fence(line.trim_start(), fence_ticks));
                let diagram = close.filter(|_| lang == "mermaid").and_then(|close| {
                    let body = source[index..index + close].join("\n");
                    Some((
                        close,
                        mermaid::render(&body, width.saturating_sub(2), theme)?,
                    ))
                });
                if let Some((close, diagram)) = diagram {
                    let bar = Span::styled("│ ".to_string(), Style::new().fg(theme.border));
                    for line in diagram {
                        let mut spans = vec![bar.clone()];
                        spans.extend(line.spans);
                        lines.push(Line::from(spans));
                    }
                    lines.push(Line::from(Span::styled(
                        "╰─".to_string(),
                        Style::new().fg(theme.border),
                    )));
                    in_code = false;
                    index += close + 1;
                }
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
            let color = match raw.as_bytes().first() {
                Some(b'+') if diff && !raw.starts_with("+++") => theme.success,
                Some(b'-') if diff && !raw.starts_with("---") => theme.danger,
                Some(b'@') if diff => theme.info,
                _ => theme.teal,
            };
            let body = Span::styled(
                raw.replace('\t', "    "),
                Style::new().fg(color).bg(code_bg),
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
        if let Some(aligns) = source
            .get(index)
            .and_then(|next| table_alignments(raw, next))
        {
            let mut rows = vec![table_cells(raw)];
            index += 1;
            while let Some(row) = source.get(index).filter(|row| is_table_row(row)) {
                rows.push(table_cells(row));
                index += 1;
            }
            lines.extend(table(&rows, &aligns, width, theme));
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

/// Column alignment taken from a table's delimiter row.
/// 取自表格分隔行的列对齐方式。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Align {
    Left,
    Center,
    Right,
}

/// Narrowest a shrunk table column gets before the table falls back to plain rows.
/// 表格列收缩的最小宽度，再窄时表格退回为纯文本行。
const MIN_COLUMN: usize = 3;

fn is_table_row(line: &str) -> bool {
    let trimmed = line.trim();
    !trimmed.is_empty() && trimmed.contains('|')
}

/// Splits a table row into cells, dropping the outer pipes and unescaping `\|`.
/// 将表格行拆分为单元格，去掉两侧管道符并还原 `\|`。
fn table_cells(line: &str) -> Vec<String> {
    let trimmed = line.trim();
    let trimmed = trimmed.strip_prefix('|').unwrap_or(trimmed);
    let trimmed = match trimmed.strip_suffix('|') {
        Some(inner) if !inner.ends_with('\\') => inner,
        _ => trimmed,
    };
    let mut cells = Vec::new();
    let mut cell = String::new();
    let mut chars = trimmed.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '\\' if chars.peek() == Some(&'|') => {
                cell.push('|');
                chars.next();
            }
            '|' => cells.push(std::mem::take(&mut cell).trim().to_string()),
            _ => cell.push(ch),
        }
    }
    cells.push(cell.trim().to_string());
    cells
}

/// Reads the delimiter row under a header row; both must hold the same number of cells.
/// 读取表头下方的分隔行；两者的单元格数必须一致。
fn table_alignments(header: &str, delimiter: &str) -> Option<Vec<Align>> {
    if !is_table_row(header) || !is_table_row(delimiter) {
        return None;
    }
    let aligns = table_cells(delimiter)
        .iter()
        .map(|cell| {
            let left = cell.starts_with(':');
            let right = cell.ends_with(':');
            let dashes = cell.trim_matches(':');
            if dashes.is_empty() || !dashes.chars().all(|c| c == '-') {
                return None;
            }
            Some(match (left, right) {
                (true, true) => Align::Center,
                (false, true) => Align::Right,
                _ => Align::Left,
            })
        })
        .collect::<Option<Vec<_>>>()?;
    (aligns.len() == table_cells(header).len()).then_some(aligns)
}

/// Shares `room` columns among cells whose natural widths are `natural`: narrow columns keep their
/// width and the rest split what remains. `None` when even the minimum does not fit.
/// 在自然宽度为 `natural` 的列之间分配 `room` 列：窄列保持原宽，其余列平分剩余空间。连最小宽度都
/// 放不下时返回 `None`。
fn column_widths(natural: &[usize], room: usize) -> Option<Vec<usize>> {
    let mut widths: Vec<usize> = natural.iter().map(|w| (*w).max(1)).collect();
    if widths.iter().sum::<usize>() <= room {
        return Some(widths);
    }
    if natural.len() * MIN_COLUMN > room {
        return None;
    }
    let mut open: Vec<usize> = (0..widths.len()).collect();
    let mut left = room;
    loop {
        let share = left / open.len();
        let (fits, wide): (Vec<usize>, Vec<usize>) =
            open.iter().partition(|i| widths[**i] <= share);
        if fits.is_empty() {
            let extra = left - share * wide.len();
            for (n, i) in wide.iter().enumerate() {
                widths[*i] = share + usize::from(n < extra);
            }
            return Some(widths);
        }
        for i in &fits {
            left -= widths[*i];
        }
        open = wide;
    }
}

/// Draws a pipe table with box borders; the header row is bold and long cells wrap inside their
/// column. Rows too narrow for the terminal fall back to one plain line per row.
/// 用方框边框绘制管道表格；表头加粗，过长的单元格在列内换行。终端过窄时每行退回为一条纯文本。
fn table(
    rows: &[Vec<String>],
    aligns: &[Align],
    width: usize,
    theme: &Theme,
) -> Vec<Line<'static>> {
    let border = Style::new().fg(theme.border);
    let columns = aligns.len();
    let cell_spans = |row: usize, column: usize| {
        let style = if row == 0 {
            theme.strong()
        } else {
            theme.base()
        };
        let text = rows[row].get(column).map(String::as_str).unwrap_or("");
        inline(text, style, theme)
    };
    let natural: Vec<usize> = (0..columns)
        .map(|column| {
            (0..rows.len())
                .map(|row| {
                    cell_spans(row, column)
                        .iter()
                        .map(|s| width_of(&s.content))
                        .sum()
                })
                .max()
                .unwrap_or(0)
        })
        .collect();
    // Each column carries one space of padding per side plus one border, and the row one more.
    // 每列两侧各留一格空白并带一条边框，整行另加一条边框。
    let frame = columns * 3 + 1;
    let Some(widths) = width
        .checked_sub(frame)
        .and_then(|room| column_widths(&natural, room))
    else {
        return rows
            .iter()
            .flat_map(|cells| {
                let spans = inline(&cells.join(" | "), theme.base(), theme);
                wrap(&spans, width, &[], &[])
            })
            .collect();
    };
    let rule = |left: &str, middle: &str, right: &str| {
        let segments: Vec<String> = widths.iter().map(|w| "─".repeat(w + 2)).collect();
        Line::from(Span::styled(
            format!("{left}{}{right}", segments.join(middle)),
            border,
        ))
    };
    let mut lines = vec![rule("┌", "┬", "┐")];
    for row in 0..rows.len() {
        let cells: Vec<Vec<Line<'static>>> = (0..columns)
            .map(|column| wrap(&cell_spans(row, column), widths[column], &[], &[]))
            .collect();
        let height = cells.iter().map(Vec::len).max().unwrap_or(1);
        for physical in 0..height {
            let mut spans = vec![Span::styled("│".to_string(), border)];
            for (column, cell) in cells.iter().enumerate() {
                let content = cell
                    .get(physical)
                    .map(|l| l.spans.clone())
                    .unwrap_or_default();
                let used: usize = content.iter().map(|s| width_of(&s.content)).sum();
                let gap = widths[column].saturating_sub(used);
                let (before, after) = match aligns[column] {
                    Align::Left => (0, gap),
                    Align::Right => (gap, 0),
                    Align::Center => (gap / 2, gap - gap / 2),
                };
                spans.push(Span::raw(" ".repeat(before + 1)));
                spans.extend(content);
                spans.push(Span::raw(" ".repeat(after + 1)));
                spans.push(Span::styled("│".to_string(), border));
            }
            lines.push(Line::from(spans));
        }
        if row == 0 {
            lines.push(rule("├", "┼", "┤"));
        }
    }
    lines.push(rule("└", "┴", "┘"));
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
        // Inline math written as $`…`$ shows its TeX source in italics without the delimiters.
        // 写作 $`…`$ 的行内公式以斜体显示其 TeX 源码，不显示定界符。
        if ch == '$'
            && chars.get(i + 1) == Some(&'`')
            && i.checked_sub(1).and_then(|before| chars.get(before)) != Some(&'$')
            && let Some(end) = chars[i + 2..]
                .windows(2)
                .position(|pair| pair == ['`', '$'])
            && chars.get(i + end + 4) != Some(&'$')
        {
            flush(&mut spans, &mut buffer, style_of(bold, italic));
            let tex: String = chars[i + 2..i + 2 + end].iter().collect();
            spans.push(Span::styled(
                tex,
                Style::new().fg(theme.teal).add_modifier(Modifier::ITALIC),
            ));
            i += end + 4;
            continue;
        }
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

    /// Every rendered table line has the same display width, never wider than the view.
    /// 渲染出的表格各行显示宽度一致，且不超过视图宽度。
    fn assert_boxed(lines: &[Line], width: usize) {
        let widths: Vec<usize> = lines.iter().map(|l| width_of(&text_of(l))).collect();
        assert!(
            widths.iter().all(|w| *w == widths[0]),
            "ragged table: {widths:?}"
        );
        assert!(widths[0] <= width, "table wider than {width}: {widths:?}");
    }

    #[test]
    fn renders_pipe_tables_with_borders_and_alignment() {
        let theme = Theme::dark();
        let source = "| Name | 數量 |\n|:--|--:|\n| a | 1 |\n| `b\\|c` | 22 |\n\nafter";
        let lines = render(source, 60, &theme);
        let texts: Vec<String> = lines.iter().map(text_of).collect();
        let end = texts
            .iter()
            .position(String::is_empty)
            .expect("blank line after the table");
        assert_boxed(&lines[..end], 60);
        assert!(texts[0].starts_with('┌') && texts[0].ends_with('┐'));
        assert!(texts[2].starts_with('├'));
        assert!(texts[end - 1].starts_with('└'));
        // Header cells are bold; the right-aligned column ends at its border.
        // 表头单元格加粗；右对齐列紧贴其边框。
        assert!(
            lines[1]
                .spans
                .iter()
                .any(|s| s.content == "Name" && s.style.add_modifier.contains(Modifier::BOLD))
        );
        assert!(texts[3].ends_with(" 1 │"));
        // An escaped pipe stays inside its cell.
        // 转义的管道符留在其单元格内。
        assert_eq!(texts[4].matches('│').count(), 3);
        assert_eq!(texts.last().map(String::as_str), Some("after"));
    }

    #[test]
    fn tables_wrap_to_narrow_views_and_need_a_delimiter_row() {
        let theme = Theme::dark();
        let long = "word ".repeat(20);
        let source = format!("| key | value |\n|---|---|\n| k | {long} |");
        let lines = render(&source, 30, &theme);
        assert_boxed(&lines, 30);
        assert!(
            lines.len() > 5,
            "the long cell should wrap onto several lines"
        );

        // Without a delimiter row, pipes are ordinary text.
        // 没有分隔行时，管道符只是普通文本。
        let plain = render("a | b\nc | d", 30, &theme);
        assert_eq!(
            plain.iter().map(text_of).collect::<Vec<_>>(),
            ["a | b", "c | d"]
        );
    }

    #[test]
    fn complete_mermaid_blocks_draw_and_streaming_ones_show_source() {
        let theme = Theme::dark();
        let block = "```mermaid
graph TD
A[Plan] --> B[Build]
```
after";
        let texts: Vec<String> = render(block, 60, &theme).iter().map(text_of).collect();
        assert!(texts[0].starts_with("╭─"));
        assert!(
            texts
                .iter()
                .any(|t| t.contains("Plan") && t.starts_with("│ "))
        );
        assert!(
            texts.iter().all(|t| !t.contains("-->")),
            "no source once drawn"
        );
        assert_eq!(texts.last().map(String::as_str), Some("after"));
        assert!(texts[texts.len() - 2].starts_with("╰─"));

        // Still streaming: no closing fence yet, so the source shows as code.
        // 仍在流式输出：尚无结束围栏，因此以代码显示源码。
        let open = "```mermaid
graph TD
A[Plan] --> B[Build]";
        let texts: Vec<String> = render(open, 60, &theme).iter().map(text_of).collect();
        assert!(texts.iter().any(|t| t.contains("A[Plan] --> B[Build]")));
    }

    /// `ui-code` shows the reply's captured lines numbered from the start line, explains missing
    /// captures, and stays code when a range is malformed.
    /// `ui-code` 显示回答抓取的行并自首行编号，说明缺失的抓取，范围格式不符时保持为代码。
    #[test]
    fn code_excerpts_show_captured_lines_or_why_they_are_missing() {
        use vulcan_session_core::v1::answer_components::{
            AnswerCodeSnapshot, AnswerCodeSnapshotResult, AnswerCodeUnavailableReason,
        };
        let theme = Theme::dark();
        let joined = |source: &str, code: CodeSnapshots<'_>| {
            render_with_code(source, 60, &theme, code)
                .iter()
                .map(text_of)
                .collect::<Vec<_>>()
        };
        let fence = "```ui-code\nsrc/a.rs:10-13 | 11 | Entry\nsrc/b.rs:3\n```";
        let snapshots = [
            AnswerCodeSnapshot {
                location: "src/a.rs:10-13".into(),
                result: AnswerCodeSnapshotResult::Captured {
                    text: "fn a() {\n    lock();\n\n}".into(),
                },
            },
            AnswerCodeSnapshot {
                location: "src/b.rs:3".into(),
                result: AnswerCodeSnapshotResult::Unavailable {
                    reason: AnswerCodeUnavailableReason::NotFound,
                },
            },
        ];
        let sealed = joined(fence, CodeSnapshots::Sealed(&snapshots));
        assert!(
            sealed
                .iter()
                .any(|line| line.starts_with("╭─ src/a.rs 10–13"))
        );
        assert!(sealed.iter().any(|line| line.contains("11     lock();")));
        assert!(sealed.iter().any(|line| line.trim_end() == "│ 12"));
        assert!(sealed.iter().any(|line| line.contains("13 }")));
        assert!(!sealed.iter().any(|line| line.contains("ui-code")));
        assert!(
            sealed
                .iter()
                .any(|line| line.contains(&crate::tr!("answerComponents.code.reason.not_found")))
        );

        let pending = joined(fence, CodeSnapshots::Pending);
        let message = crate::tr!("answerComponents.code.pending");
        assert_eq!(
            pending
                .iter()
                .filter(|line| line.contains(&message))
                .count(),
            2
        );

        let invalid = joined("```ui-code\nsrc/a.rs\n```", CodeSnapshots::None);
        assert!(invalid.iter().any(|line| line.contains("src/a.rs")));
        assert!(invalid.iter().any(|line| line.contains("ui-code")));
    }

    /// Closed components render natively, nested fences stay inside their container, and an
    /// unclosed or invalid component shows its source as code.
    /// 已闭合组件原生渲染，内嵌围栏留在其容器内，未闭合或无效组件以代码显示源码。
    #[test]
    fn answer_components_render_when_closed_and_stay_code_otherwise() {
        let theme = Theme::dark();
        let joined = |source: &str| {
            render(source, 60, &theme)
                .iter()
                .map(text_of)
                .collect::<Vec<_>>()
                .join("\n")
        };
        let checklist = "```ui-checklist\nok | marker-item\n```\nafter";
        let rendered = joined(checklist);
        assert!(rendered.contains("marker-item") && rendered.contains("after"));
        assert!(!rendered.contains("ui-checklist"));

        let streaming = joined("```ui-checklist\nok | marker-item");
        assert!(streaming.contains("ui-checklist"));

        let invalid = joined("```ui-checklist\nmaybe | marker-item\n```");
        assert!(invalid.contains("ui-checklist"));

        // Text after a row component's name is its heading.
        // 行组件名称后的文本作为其标题。
        let headed = joined("```ui-kv heading-marker\nkey-marker | value\n```");
        assert!(headed.contains("heading-marker") && headed.contains("key-marker"));
        assert!(!headed.contains("ui-kv"));

        let nested = "````ui-callout warning Title-marker\nbody\n```ui-kv\nkey-marker | value\n```\ntail-marker\n````\nafter";
        let rendered = joined(nested);
        assert!(rendered.contains("Title-marker") && rendered.contains("key-marker"));
        assert!(rendered.contains("tail-marker") && rendered.contains("after"));
        assert!(!rendered.contains("ui-kv") && !rendered.contains("ui-callout"));

        let tabs = joined("```ui-tabs\n### A\none\n### B\ntwo\n```");
        assert!(!tabs.contains("ui-tabs"));
        let empty_section = joined("```ui-tabs\n### A\n### B\ntwo\n```");
        assert!(empty_section.contains("ui-tabs"));

        // The row-component pipe works in place of the colon before a message or note label.
        // 消息或注释标签前可用行组件的竖线代替冒号。
        let piped = joined("```ui-sequence\nA -> B | msg-marker\nnote A, B | note-marker\n```");
        assert!(!piped.contains("ui-sequence"));
        assert!(piped.contains("msg-marker") && piped.contains("note-marker"));
    }

    #[test]
    fn entity_math_and_overrun_components_follow_the_shared_syntax() {
        let theme = Theme::dark();
        let joined = |source: &str| {
            render(source, 60, &theme)
                .iter()
                .map(text_of)
                .collect::<Vec<_>>()
                .join("\n")
        };
        let er = joined("```ui-er\nUser\n  id PK\nOrder\n  user_id -> User\n```");
        assert!(!er.contains("ui-er") && er.contains("user_id FK → User"));
        assert!(joined("```ui-er\nOrder\n  user_id -> Missing\n```").contains("ui-er"));
        assert!(joined("```ui-er\n  id PK\nUser\n```").contains("ui-er"));

        let math = joined("```ui-math Law\nL = \\lambda W\n\nL | requests-marker\n```");
        assert!(!math.contains("ui-math"));
        assert!(math.contains("L = \\lambda W") && math.contains("requests-marker"));
        assert!(joined("```ui-math\nx = 1\n\nx | ok\ny\n```").contains("ui-math"));
        // Lines after the blank line are further formulas when none is a legend row.
        // 空行之后没有图例行时，这些行按公式处理。
        let formulas = joined("```ui-math\nr = a\n\nso-marker r = b\n```");
        assert!(!formulas.contains("ui-math") && formulas.contains("so-marker r = b"));
        // Blank-separated formula groups come before a legend after the last blank line.
        // 以空行分隔的多组公式位于最后一个空行之后的图例之前。
        let groups = joined("```ui-math\na = 1\n\nb-marker = 2\n\nb | legend-marker\n```");
        assert!(
            !groups.contains("ui-math")
                && groups.contains("b-marker = 2")
                && groups.contains("legend-marker")
        );
        assert!(joined("```ui-math\na = 1\n\nb = 2\n\nb | ok\nc = 3\n```").contains("ui-math"));

        let over = joined("```ui-metrics\nImages | 620 | 500 | KB\n```");
        assert!(!over.contains("ui-metrics") && over.contains("✗") && over.contains("██████████"));

        // Lenient forms models write: a unit as the third metric cell, one-sided file counts or a
        // change label, and a path-shaped fourth checklist cell.
        // 模型的宽松写法：指标第三格为单位、文件只写一侧行数或变更标签、清单第四格为路径。
        let unit = joined("```ui-metrics\nScan | 10.9 | s\n```");
        assert!(unit.contains("10.9 s") && !unit.contains("ui-metrics") && !unit.contains('░'));
        // Empty maximum and unit cells keep columns aligned and mean the value is absent.
        // 空的最大值与单位格用于对齐列，表示该项缺省。
        let aligned =
            joined("```ui-metrics\nraw-marker | 506 s | | | no\nsize | 0.96 | | MB | ok\n```");
        assert!(
            !aligned.contains("ui-metrics")
                && aligned.contains("raw-marker")
                && aligned.contains("0.96 MB")
        );
        assert!(joined("```ui-metrics\n | 80\n```").contains("ui-metrics"));
        let files = joined("```ui-files\nsrc/a.ts | +145 | entry\nsrc/b.ts | new | id\n```");
        assert!(!files.contains("ui-files") && files.contains("+145 −0") && files.contains("new"));
        let located = joined("```ui-checklist\nno | item | note | src/a.rs:150\n```");
        assert!(!located.contains("ui-checklist") && located.contains("src/a.rs:150"));
        assert!(
            joined("```ui-checklist\nno | item | note | more prose\n```").contains("ui-checklist")
        );
        let noted = joined("```ui-findings\nhigh | issue | ledger | note-marker\n```");
        assert!(!noted.contains("ui-findings") && noted.contains("note-marker"));
        assert!(joined("```ui-findings\nhigh | a | b | c | d\n```").contains("ui-findings"));
        let untoned = joined("```ui-callout\nbody-marker\n```");
        assert!(!untoned.contains("ui-callout") && untoned.contains("body-marker"));
        let aliased = joined("```ui-callout warn Title-marker\nbody\n```");
        assert!(aliased.contains("Title-marker") && !aliased.contains("warn"));

        let decision = joined("```ui-decision\nLifetime?\n* Ten | deploys\n- Hour\n```");
        assert!(!decision.contains("ui-decision") && decision.contains("Lifetime?"));
        assert!(decision.contains("○ Ten — deploys ★") && decision.contains("○ Hour"));
        assert!(joined("```ui-decision multi\nAlso?\n* A\n* B\n```").contains("☐ B ★"));
        assert!(joined("```ui-decision\nQ?\n* A\n* B\n```").contains("ui-decision"));
        assert!(joined("```ui-decision\nQ?\n- A\n- A\n```").contains("ui-decision"));
    }

    #[test]
    fn inline_math_drops_its_delimiters_and_leaves_amounts_alone() {
        let theme = Theme::dark();
        let text: String = inline("rate $`\\lambda`$ and $5 or $10", Style::new(), &theme)
            .iter()
            .map(|span| span.content.as_ref())
            .collect();
        assert_eq!(text, "rate \\lambda and $5 or $10");
    }
}
