// Terminal reading of `ui-*` answer components.
// `ui-*` 回答组件的终端阅读视图。
//
// The WebUI draws these fences natively; the terminal rewrites the same fence into the Markdown
// the transcript already renders: status words as symbols, flows and sequences as Mermaid diagrams
// (when they fit), trees with guide lines and the remaining components as lists, tables and
// labels. Containers render their Markdown body behind a tinted bar. A fence that does not match
// its documented syntax returns `None` and stays code.
// WebUI 原生绘制这些围栏；终端将同一围栏改写为会话记录已能绘制的 Markdown：状态词转为符号、流程与
// 时序转为 Mermaid 图（能放下时）、树形带引导线，其余组件转为列表、表格和标签。容器在着色竖线后
// 渲染其 Markdown 正文。不符合文档语法的围栏返回 `None`，保持为代码。

use std::collections::HashMap;

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use vulcan_session_core::v1::answer_components::{
    ANSWER_CHART_MAX_SERIES, ANSWER_COMPONENT_MAX_ROWS,
};

use super::markdown;
use super::mermaid;
use super::theme::Theme;
use crate::tr;

/// Render one closed `ui-*` fence whose info string is `info` (without the backticks).
/// 渲染一个信息串为 `info`（不含反引号）的已闭合 `ui-*` 围栏。
/// Returns `None` for unknown names or malformed rows so the caller keeps showing code.
/// 未知名称或行格式不符时返回 `None`，由调用方继续显示代码。
pub fn render(
    info: &str,
    body: &[&str],
    width: usize,
    theme: &Theme,
) -> Option<Vec<Line<'static>>> {
    let component = info.strip_prefix("ui-")?;
    let (name, args) = component
        .split_once(char::is_whitespace)
        .map_or((component, ""), |(name, args)| (name, args.trim()));
    if body.iter().all(|line| line.trim().is_empty()) {
        return None;
    }
    match name {
        "callout" => {
            let (tone, title) = args.split_once(char::is_whitespace).unwrap_or((args, ""));
            let (symbol, color) = match tone {
                "info" => ("ℹ", theme.info),
                "success" => ("✓", theme.success),
                "warning" => ("!", theme.warning),
                "danger" => ("✗", theme.danger),
                _ => return None,
            };
            let header = format!("{symbol} {}", title.trim());
            Some(framed(header.trim_end(), color, body, width, theme))
        }
        "details" => (!args.is_empty())
            .then(|| framed(&format!("▾ {args}"), theme.border, body, width, theme)),
        "tabs" | "columns" => {
            let sections = body.iter().filter(|line| line.starts_with("### ")).count();
            let first = body.iter().find(|line| !line.trim().is_empty())?;
            // Every heading must be followed by content before the next heading or the end.
            // 每个标题之后、下一个标题或结尾之前必须有内容。
            let filled = body
                .split(|line| line.starts_with("### "))
                .skip(1)
                .all(|section| section.iter().any(|line| !line.trim().is_empty()));
            (args.is_empty()
                && first.starts_with("### ")
                && filled
                && (2..=ANSWER_COMPONENT_MAX_ROWS).contains(&sections))
            .then(|| markdown::render(&body.join("\n"), width, theme))
        }
        _ => {
            let source = component_markdown(name, args, body, width, theme)?;
            Some(markdown::render(&source, width, theme))
        }
    }
}

/// A bold `header` and the Markdown `body`, both behind a bar tinted with `color`.
/// 粗体 `header` 与 Markdown `body`，均位于以 `color` 着色的竖线之后。
fn framed(
    header: &str,
    color: Color,
    body: &[&str],
    width: usize,
    theme: &Theme,
) -> Vec<Line<'static>> {
    let bar = Span::styled("┃ ".to_owned(), Style::new().fg(color));
    let mut lines = vec![Line::from(vec![
        bar.clone(),
        Span::styled(
            header.to_owned(),
            Style::new().fg(color).add_modifier(Modifier::BOLD),
        ),
    ])];
    for line in markdown::render(&body.join("\n"), width.saturating_sub(2), theme) {
        let mut spans = vec![bar.clone()];
        spans.extend(line.spans);
        lines.push(Line::from(spans));
    }
    lines
}

/// Status word shared by checklists, compare cells, card tags and metric tones.
/// 清单、对比单元格、卡片标签与指标色调共用的状态词。
fn status_symbol(word: &str) -> Option<&'static str> {
    Some(match word {
        "ok" => "✓",
        "no" => "✗",
        "warn" => "!",
        "todo" => "☐",
        _ => return None,
    })
}

/// Symbol of a tone word (`ok`, `warn`, `no`); `todo` is a checklist status, not a tone.
/// 色调词（`ok`、`warn`、`no`）的符号；`todo` 是清单状态而非色调。
fn tone_symbol(word: &str) -> Option<&'static str> {
    status_symbol(word).filter(|_| word != "todo")
}

/// Replace a leading tone word by its symbol, keeping the remaining text.
/// 将前导色调词替换为符号，保留其余文字。
fn toned(cell: &str) -> String {
    let (word, rest) = cell.split_once(' ').unwrap_or((cell, ""));
    match tone_symbol(word) {
        Some(symbol) if rest.trim().is_empty() => symbol.to_owned(),
        Some(symbol) => format!("{symbol} {}", rest.trim()),
        None => cell.to_owned(),
    }
}

/// Nonblank rows, skipping a Markdown table delimiter row.
/// 非空行，跳过 Markdown 表格分隔行。
fn data_rows<'a>(body: &[&'a str]) -> Vec<&'a str> {
    body.iter()
        .copied()
        .filter(|line| {
            let trimmed = line.trim();
            !trimmed.is_empty()
                && !(trimmed.contains('-')
                    && trimmed
                        .chars()
                        .all(|ch| matches!(ch, '|' | ':' | '-' | ' ')))
        })
        .collect()
}

/// Split one row at `|` (outer pipes optional) into `min..=max` nonempty trimmed cells.
/// 按 `|` 拆分一行（外侧竖线可省略），得到 `min..=max` 个非空去空白单元格。
fn row_cells(line: &str, min: usize, max: usize) -> Option<Vec<&str>> {
    let text = line.trim();
    let text = text.strip_prefix('|').unwrap_or(text);
    let text = text.strip_suffix('|').unwrap_or(text);
    let cells: Vec<&str> = text.split('|').map(str::trim).collect();
    (cells.len() >= min && cells.len() <= max && cells.iter().all(|cell| !cell.is_empty()))
        .then_some(cells)
}

/// Convert every row with `row`, failing the whole component when one row is malformed.
/// 用 `row` 转换每一行；任一行格式不符时整个组件失败。
fn rows_markdown(
    body: &[&str],
    min: usize,
    max: usize,
    mut row: impl FnMut(&[&str]) -> Option<String>,
) -> Option<String> {
    let rows = data_rows(body);
    if rows.is_empty() || rows.len() > ANSWER_COMPONENT_MAX_ROWS {
        return None;
    }
    rows.iter()
        .map(|line| row(&row_cells(line, min, max)?))
        .collect()
}

/// Rewrite one closed row or graph component into transcript Markdown, or `None` when invalid.
/// 将一个已闭合的行组件或图组件改写为会话记录 Markdown；无效时返回 `None`。
fn component_markdown(
    name: &str,
    args: &str,
    lines: &[&str],
    width: usize,
    theme: &Theme,
) -> Option<String> {
    if !matches!(name, "chart" | "flow" | "sequence") && !args.is_empty() {
        return None;
    }
    match name {
        "kv" => rows_markdown(lines, 2, 2, |cell| {
            Some(format!("- **{}**: {}\n", cell[0], cell[1]))
        }),
        "metrics" => rows_markdown(lines, 2, 5, metric),
        "findings" => rows_markdown(lines, 2, 3, |cell| {
            let severity = match cell[0] {
                "high" => tr!("answerComponents.severity.high"),
                "medium" => tr!("answerComponents.severity.medium"),
                "low" => tr!("answerComponents.severity.low"),
                "info" => tr!("answerComponents.severity.info"),
                _ => return None,
            };
            let location = cell.get(2).map_or(String::new(), |at| format!(" — `{at}`"));
            Some(format!("- **[{severity}]** {}{location}\n", cell[1]))
        }),
        "checklist" => rows_markdown(lines, 2, 3, |cell| {
            let symbol = status_symbol(cell[0])?;
            let note = cell
                .get(2)
                .map_or(String::new(), |note| format!(" — {note}"));
            Some(format!("- {symbol} {}{note}\n", cell[1]))
        }),
        "steps" => {
            let mut number = 0;
            rows_markdown(lines, 2, 3, |cell| {
                let symbol = match cell[0] {
                    "done" => "✓",
                    "now" => "▶",
                    "todo" => "·",
                    _ => return None,
                };
                number += 1;
                let text = cell
                    .get(2)
                    .map_or(String::new(), |text| format!(" — {text}"));
                Some(format!("{number}. {symbol} **{}**{text}\n", cell[1]))
            })
        }
        "timeline" => rows_markdown(lines, 2, 3, |cell| {
            let text = cell
                .get(2)
                .map_or(String::new(), |text| format!(" — {text}"));
            Some(format!("- `{}` **{}**{text}\n", cell[0], cell[1]))
        }),
        "cards" => rows_markdown(lines, 2, 3, |cell| {
            let tag = cell
                .get(2)
                .map_or(String::new(), |tag| format!(" `{}`", toned(tag)));
            Some(format!("- **{}**{tag} — {}\n", cell[0], cell[1]))
        }),
        "files" => files(lines),
        "gallery" => rows_markdown(lines, 1, 2, |cell| {
            Some(match cell.get(1) {
                Some(caption) => format!("- {caption}: `{}`\n", cell[0]),
                None => format!("- `{}`\n", cell[0]),
            })
        }),
        "compare" => compare(lines),
        "chart" => chart(args, lines),
        "flow" => {
            flow_mermaid(&data_rows(lines), args).and_then(|source| drawn(source, width, theme))
        }
        "sequence" => {
            sequence_mermaid(&data_rows(lines), args).and_then(|source| drawn(source, width, theme))
        }
        "tree" => {
            let rows: Vec<&str> = lines
                .iter()
                .copied()
                .filter(|line| !line.trim().is_empty())
                .collect();
            (rows.len() <= ANSWER_COMPONENT_MAX_ROWS).then(|| tree(&rows))
        }
        _ => None,
    }
}

/// `label | value [| maximum [| unit]] [| tone]` as a bullet with an optional ten-cell meter.
/// `标签 | 数值 [| 最大值 [| 单位]] [| 色调]` 转为条目，可带十格进度条。
fn metric(cell: &[&str]) -> Option<String> {
    let mut cell = cell.to_vec();
    let tone = cell
        .last()
        .filter(|_| cell.len() > 2)
        .and_then(|last| tone_symbol(last));
    if tone.is_some() {
        cell.pop();
    }
    let prefix = tone.map_or(String::new(), |symbol| format!("{symbol} "));
    let Some(maximum) = cell.get(2) else {
        return Some(format!("- {prefix}**{}**: {}\n", cell[0], cell[1]));
    };
    let value: f64 = cell[1].parse().ok()?;
    let maximum: f64 = maximum.parse().ok()?;
    if !value.is_finite()
        || !maximum.is_finite()
        || maximum <= 0.0
        || value < 0.0
        || value > maximum
    {
        return None;
    }
    let filled = (value / maximum * 10.0).round() as usize;
    Some(format!(
        "- {prefix}**{}**: {}{} `{}{}`\n",
        cell[0],
        cell[1],
        cell.get(3).map_or(String::new(), |unit| format!(" {unit}")),
        "█".repeat(filled),
        "░".repeat(10 - filled)
    ))
}

/// Parse a `+added -removed` cell.
/// 解析 `+新增 -删除` 单元格。
fn line_delta(text: &str) -> Option<(u64, u64)> {
    let (added, removed) = text.split_once(char::is_whitespace)?;
    Some((
        added.strip_prefix('+')?.parse().ok()?,
        removed.trim().strip_prefix(['-', '−'])?.parse().ok()?,
    ))
}

/// `path | +added -removed | note` rows under a summed header; later cells are optional.
/// `路径 | +新增 -删除 | 说明` 行并附合计标题；后续单元格可省略。
fn files(lines: &[&str]) -> Option<String> {
    let (mut count, mut added, mut removed) = (0usize, 0u64, 0u64);
    let body = rows_markdown(lines, 1, 3, |cell| {
        let delta = cell.get(1).and_then(|text| line_delta(text));
        if delta.is_none() && cell.len() == 3 {
            return None;
        }
        let note = cell.get(if delta.is_some() { 2 } else { 1 });
        count += 1;
        let delta = delta.map_or(String::new(), |(plus, minus)| {
            added += plus;
            removed += minus;
            format!(" +{plus} −{minus}")
        });
        let note = note.map_or(String::new(), |note| format!(" — {note}"));
        Some(format!("- `{}`{delta}{note}\n", cell[0]))
    })?;
    Some(format!(
        "**{}** +{added} −{removed}\n\n{body}",
        tr!("answerComponents.fileCount", count = count)
    ))
}

/// Decision matrix: a header of criterion and options, then one row per criterion.
/// 决策矩阵：首行为评估项与选项，之后每个评估项一行。
fn compare(lines: &[&str]) -> Option<String> {
    let rows = data_rows(lines);
    if rows.len() > ANSWER_COMPONENT_MAX_ROWS + 1 {
        return None;
    }
    let header = row_cells(rows.first()?, 3, usize::MAX)?;
    let mut output = format!(
        "| {} |\n|{}\n",
        header.join(" | "),
        " --- |".repeat(header.len())
    );
    for line in rows.get(1..).filter(|rows| !rows.is_empty())? {
        let cells = row_cells(line, header.len(), header.len())?;
        let marked: Vec<String> = cells
            .iter()
            .enumerate()
            .map(|(index, cell)| {
                if index == 0 {
                    (*cell).to_owned()
                } else {
                    toned(cell)
                }
            })
            .collect();
        output.push_str(&format!("| {} |\n", marked.join(" | ")));
    }
    Some(output)
}

/// `ui-chart <kind> [title]`: a single series draws ten-cell bars, several series a table.
/// `ui-chart <类型> [标题]`：单系列绘制十格条形，多系列显示表格。
fn chart(args: &str, lines: &[&str]) -> Option<String> {
    let (kind, title) = args.split_once(char::is_whitespace).unwrap_or((args, ""));
    if !matches!(
        kind,
        "bar" | "hbar" | "stack" | "line" | "area" | "pie" | "donut"
    ) {
        return None;
    }
    let rows = data_rows(lines);
    if rows.len() > ANSWER_COMPONENT_MAX_ROWS + 1 {
        return None;
    }
    let header = row_cells(rows.first()?, 2, ANSWER_CHART_MAX_SERIES + 1)?;
    let circular = matches!(kind, "pie" | "donut");
    if circular && header.len() != 2 {
        return None;
    }
    let mut data = Vec::new();
    for line in rows.get(1..).filter(|rows| !rows.is_empty())? {
        let cells = row_cells(line, header.len(), header.len())?;
        let values = cells[1..]
            .iter()
            .map(|cell| {
                let value = cell
                    .strip_suffix('%')
                    .unwrap_or(cell)
                    .trim()
                    .parse::<f64>()
                    .ok()?;
                value.is_finite().then_some(value)
            })
            .collect::<Option<Vec<_>>>()?;
        if matches!(kind, "pie" | "donut" | "stack") && values.iter().any(|value| *value < 0.0) {
            return None;
        }
        data.push((cells[0], values));
    }
    let mut output = if title.trim().is_empty() {
        String::new()
    } else {
        format!("**{}**\n\n", title.trim())
    };
    if header.len() == 2 {
        let peak = data
            .iter()
            .map(|(_, values)| values[0].abs())
            .fold(0.0, f64::max);
        let total: f64 = data.iter().map(|(_, values)| values[0]).sum();
        if circular && total <= 0.0 {
            return None;
        }
        for (category, values) in &data {
            let ratio = if peak > 0.0 {
                values[0].abs() / peak
            } else {
                0.0
            };
            let filled = (ratio * 10.0).round() as usize;
            let share = if circular {
                format!(" ({:.0}%)", values[0] / total * 100.0)
            } else {
                String::new()
            };
            output.push_str(&format!(
                "- **{category}** `{}{}` {}{share}\n",
                "█".repeat(filled),
                "░".repeat(10 - filled),
                values[0]
            ));
        }
    } else {
        output.push_str(&format!(
            "| {} |\n|{}\n",
            header.join(" | "),
            " --- |".repeat(header.len())
        ));
        for (category, values) in &data {
            let values: Vec<String> = values.iter().map(f64::to_string).collect();
            output.push_str(&format!("| {category} | {} |\n", values.join(" | ")));
        }
    }
    Some(output)
}

/// Keep Mermaid `source` only when the transcript can draw it within `width` columns.
/// 仅当会话记录能在 `width` 列内绘制时保留 Mermaid `source`。
fn drawn(source: String, width: usize, theme: &Theme) -> Option<String> {
    mermaid::render(&source, width.saturating_sub(2), theme)?;
    Some(format!("```mermaid\n{source}```\n"))
}

/// Split a leading plan marker (`+ `, `- `, `~ `) from `line`; `\` keeps the text literal.
/// 从 `line` 拆出前导方案标记（`+ `、`- `、`~ `）；`\` 保留字面文字。
fn change(line: &str) -> (Option<char>, &str) {
    if let Some(rest) = line.strip_prefix('\\') {
        return (None, rest);
    }
    for marker in ['+', '-', '~'] {
        if let Some(rest) = line
            .strip_prefix(marker)
            .and_then(|rest| rest.strip_prefix(' '))
        {
            return (Some(marker), rest.trim_start());
        }
    }
    (None, line)
}

/// Split at the first ASCII or full-width colon into a trimmed head and optional tail.
/// 在第一个 ASCII 或全角冒号处拆分为去空白的前段和可选后段。
fn label(text: &str) -> (&str, Option<&str>) {
    match text.find([':', '：']) {
        Some(index) => {
            let colon = text[index..].chars().next().map_or(1, char::len_utf8);
            (text[..index].trim(), Some(text[index + colon..].trim()))
        }
        None => (text.trim(), None),
    }
}

/// Mermaid-safe label text: quotes, pipes and line breaks would end the label early.
/// Mermaid 安全的标签文字：引号、竖线与换行会提前结束标签。
fn safe(text: &str) -> String {
    text.replace(['"', '|', '\n'], " ")
}

/// Translate `ui-flow` rows into a Mermaid flowchart; `None` when a line has no recognizable shape.
/// 将 `ui-flow` 行转换为 Mermaid 流程图；某行无法识别时返回 `None`。
fn flow_mermaid(rows: &[&str], args: &str) -> Option<String> {
    let direction = match args {
        "" => "TD",
        "LR" => "LR",
        _ => return None,
    };
    let mut ids: HashMap<String, usize> = HashMap::new();
    let mut nodes: Vec<(String, &'static str, &'static str, bool)> = Vec::new();
    let mut edges = Vec::new();
    let mut changed = Vec::new();
    let mut node = |spec: &str| -> Option<usize> {
        let spec = spec.trim();
        let (open, close, text) = [("[(", ")]"), ("{", "}"), ("(", ")"), ("[", "]")]
            .iter()
            .find_map(|(open, close)| {
                let inner = spec.strip_prefix(open)?.strip_suffix(close)?;
                Some((*open, *close, inner.trim()))
            })
            .unwrap_or(("[", "]", spec));
        let (open, close) = if open == "(" {
            ("([", "])")
        } else {
            (open, close)
        };
        if text.is_empty() {
            return None;
        }
        let id = *ids.entry(text.to_owned()).or_insert_with(|| {
            nodes.push((text.to_owned(), "[", "]", false));
            nodes.len() - 1
        });
        if open != "[" {
            nodes[id].1 = open;
            nodes[id].2 = close;
        }
        Some(id)
    };
    for row in rows {
        let (marker, line) = change(row.trim());
        let mut parts = Vec::new();
        let mut dashed = Vec::new();
        let mut rest = line;
        while let Some(index) = rest.find("->") {
            let dash = index > 0 && rest.as_bytes()[index - 1] == b'-';
            parts.push(&rest[..if dash { index - 1 } else { index }]);
            dashed.push(dash);
            rest = &rest[index + 2..];
        }
        if parts.is_empty() {
            let id = node(line)?;
            if marker == Some('~') {
                changed.push(id);
            }
            continue;
        }
        let (last, text) = label(rest);
        parts.push(last);
        let groups = parts
            .iter()
            .map(|part| part.split('&').map(&mut node).collect::<Option<Vec<_>>>())
            .collect::<Option<Vec<_>>>()?;
        for (hop, pair) in groups.windows(2).enumerate() {
            let mut caption = String::new();
            if let Some(marker) = marker {
                caption.push(marker);
            }
            if let Some(text) = text.filter(|text| !text.is_empty() && hop + 2 == groups.len()) {
                if !caption.is_empty() {
                    caption.push(' ');
                }
                caption.push_str(text);
            }
            for &from in &pair[0] {
                for &to in &pair[1] {
                    edges.push((from, to, dashed[hop], caption.clone()));
                }
            }
        }
    }
    let mut source = format!("graph {direction}\n");
    for id in changed {
        nodes[id].3 = true;
    }
    for (index, (text, open, close, changed)) in nodes.iter().enumerate() {
        let mark = if *changed { "~ " } else { "" };
        source.push_str(&format!("n{index}{open}\"{mark}{}\"{close}\n", safe(text)));
    }
    for (from, to, dashed, caption) in &edges {
        let arrow = if *dashed { "-.->" } else { "-->" };
        if caption.is_empty() {
            source.push_str(&format!("n{from} {arrow} n{to}\n"));
        } else {
            source.push_str(&format!("n{from} {arrow}|{}| n{to}\n", safe(caption)));
        }
    }
    Some(source)
}

/// Translate `ui-sequence` rows into a Mermaid sequence diagram with actors in first-use order.
/// 将 `ui-sequence` 行转换为 Mermaid 时序图，参与者按首次出现排序。
fn sequence_mermaid(rows: &[&str], args: &str) -> Option<String> {
    let numbered = match args {
        "" => false,
        "num" => true,
        _ => return None,
    };
    let mut actors: Vec<String> = Vec::new();
    let mut actor = |name: &str| -> Option<usize> {
        let name = name.trim();
        if name.is_empty() {
            return None;
        }
        Some(match actors.iter().position(|known| known == name) {
            Some(index) => index,
            None => {
                actors.push(name.to_owned());
                actors.len() - 1
            }
        })
    };
    let mut steps = Vec::new();
    for row in rows {
        let line = row.trim();
        if let Some(inner) = line
            .strip_prefix("==")
            .and_then(|rest| rest.strip_suffix("=="))
        {
            steps.push((None, format!("── {} ──", inner.trim())));
        } else if let Some(rest) = line.strip_prefix("note ") {
            let (over, text) = label(rest);
            let indexes = over
                .split([',', '，'])
                .map(&mut actor)
                .collect::<Option<Vec<_>>>()?;
            let (first, last) = (indexes.iter().min()?, indexes.iter().max()?);
            steps.push((Some((*first, *last)), safe(text?)));
        } else {
            let (marker, line) = change(line);
            let index = line.find("->")?;
            let dashed = index > 0 && line.as_bytes()[index - 1] == b'-';
            let from = actor(&line[..if dashed { index - 1 } else { index }])?;
            let (to, text) = label(&line[index + 2..]);
            let to = actor(to)?;
            let arrow = if dashed { "-->>" } else { "->>" };
            let mark = marker.map_or(String::new(), |marker| format!("{marker} "));
            steps.push((
                None,
                format!("a{from}{arrow}a{to}: {mark}{}", safe(text.unwrap_or(""))),
            ));
        }
    }
    let mut source = String::from("sequenceDiagram\n");
    if numbered {
        source.push_str("autonumber\n");
    }
    for (index, name) in actors.iter().enumerate() {
        source.push_str(&format!("participant a{index} as {}\n", safe(name)));
    }
    let last = actors.len().checked_sub(1)?;
    for (span, text) in steps {
        match span {
            Some((first, end)) => source.push_str(&format!("Note over a{first},a{end}: {text}\n")),
            None if text.starts_with('─') => {
                source.push_str(&format!("Note over a0,a{last}: {text}\n"));
            }
            None => {
                source.push_str(&text);
                source.push('\n');
            }
        }
    }
    Some(source)
}

/// Draw `ui-tree` rows with `├─`/`└─` guides inside a code frame; nesting follows indentation.
/// 在代码框内以 `├─`/`└─` 引导线绘制 `ui-tree` 行；层级跟随缩进。
fn tree(rows: &[&str]) -> String {
    let mut indents: Vec<usize> = Vec::new();
    let mut items: Vec<(usize, String)> = Vec::new();
    for row in rows {
        let indent: usize = row
            .chars()
            .take_while(|ch| ch.is_whitespace())
            .map(|ch| if ch == '\t' { 4 } else { 1 })
            .sum();
        while indents.last().is_some_and(|last| *last > indent) {
            indents.pop();
        }
        if indents.last() != Some(&indent) {
            indents.push(indent);
        }
        let (marker, text) = change(row.trim());
        let text = match text.split_once('|') {
            Some((name, note)) => format!("{}  · {}", name.trim(), note.trim()),
            None => text.trim().to_owned(),
        };
        let text = marker.map_or(text.clone(), |marker| format!("{marker} {text}"));
        items.push((indents.len() - 1, text));
    }
    // Whether another sibling follows item `index` before its parent level ends.
    // 在父级结束前，条目 `index` 之后是否还有同级条目。
    let more = |index: usize| {
        let level = items[index].0;
        items[index + 1..]
            .iter()
            .take_while(|(next, _)| *next >= level)
            .any(|(next, _)| *next == level)
    };
    let mut output = String::from("```tree\n");
    let mut open: Vec<bool> = Vec::new();
    for (index, (level, text)) in items.iter().enumerate() {
        open.truncate(*level);
        if *level > 0 {
            for continues in &open[1..] {
                output.push_str(if *continues { "│  " } else { "   " });
            }
            output.push_str(if more(index) { "├─ " } else { "└─ " });
        }
        output.push_str(text);
        output.push('\n');
        open.push(more(index));
    }
    output.push_str("```\n");
    output
}
