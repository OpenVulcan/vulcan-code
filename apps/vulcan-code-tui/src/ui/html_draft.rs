// Terminal reading of version-two HTML answer drafts.
// 版本二 HTML 回答草稿的终端阅读视图。
//
// A draft is Markdown plus registered `ui-*` component fences; Core compiles it into the page the
// browser renders. The terminal cannot run that page, so this module rewrites the same draft into
// the Markdown the transcript already draws: lettered `##` panels, status words as symbols, flows
// and sequences as Mermaid diagrams (when they fit), trees with guide lines and the remaining
// components as lists, quotes and labels. The result is presentation only; it grants nothing and
// never decides whether the draft is valid, which stays Core's authority.
// 草稿是 Markdown 加上已注册的 `ui-*` 组件围栏；Core 将其编译为浏览器渲染的页面。终端无法运行该页面，
// 因此本模块把同一草稿改写为会话记录已能绘制的 Markdown：带字母编号的 `##` 面板、状态词转为符号、
// 流程与时序转为 Mermaid 图（能放下时）、树形带引导线，其余组件转为列表、引用和标签。结果仅用于展示；
// 不授予任何能力，也不判定草稿是否合法，合法性始终由 Core 决定。

use std::collections::HashMap;

use ratatui::style::Style;
use ratatui::text::{Line, Span};

use super::markdown;
use super::mermaid;
use super::theme::Theme;

/// Return the readable draft of a version-two result whose opening line is `fence`, or `None` for
/// raw version-one HTML and undeclared templates, which the terminal cannot show meaningfully.
/// 返回开头行为 `fence` 的版本二结果的可读草稿；版本一原始 HTML 与未声明模板返回 `None`，终端无法有意义地显示它们。
/// Lines sit behind a border bar so the preview reads as one block inside the transcript.
/// 各行带边框竖线，使预览在会话记录中成为一个整体区块。
pub fn preview(fence: &str, body: &str, width: usize, theme: &Theme) -> Option<Vec<Line<'static>>> {
    vulcan_session_core::v1::html_answers::html_answer_template(fence)
        .ok()
        .flatten()?;
    let bar = Span::styled("│ ".to_string(), Style::new().fg(theme.border));
    let inner = width.saturating_sub(2);
    Some(
        markdown::render(&to_markdown(body, inner, theme), inner, theme)
            .into_iter()
            .map(|line| {
                let mut spans = vec![bar.clone()];
                spans.extend(line.spans);
                Line::from(spans)
            })
            .collect(),
    )
}

/// Rewrite draft `body` into transcript Markdown; diagrams are checked against `width` columns.
/// 将草稿 `body` 改写为会话记录 Markdown；图表按 `width` 列检查能否绘制。
/// An unclosed trailing component (still streaming) is shown as its source code.
/// 末尾未闭合的组件（仍在流式输出）以源码显示。
pub fn to_markdown(body: &str, width: usize, theme: &Theme) -> String {
    let lines: Vec<&str> = body.lines().collect();
    let mut output = String::new();
    let mut panels = 0usize;
    let mut index = 0;
    while index < lines.len() {
        let line = lines[index];
        index += 1;
        let trimmed = line.trim_start();
        let ticks = trimmed.bytes().take_while(|byte| *byte == b'`').count();
        if ticks >= 3 {
            let info = trimmed[ticks..].trim();
            let close = lines[index..].iter().position(|candidate| {
                let candidate = candidate.trim_start();
                let run = candidate.bytes().take_while(|byte| *byte == b'`').count();
                run >= ticks && candidate[run..].trim().is_empty()
            });
            let end = close.map_or(lines.len(), |close| index + close);
            let inner = &lines[index..end];
            index = close.map_or(lines.len(), |_| end + 1);
            match info.strip_prefix("ui-").filter(|_| close.is_some()) {
                Some(component) => {
                    output.push_str(&component_markdown(component, inner, width, theme))
                }
                None => push_code(&mut output, info, inner),
            }
            continue;
        }
        if let Some(title) = trimmed.strip_prefix("## ") {
            output.push_str(&format!("## {} · {}\n", panel_letter(panels), title.trim()));
            panels += 1;
            continue;
        }
        if trimmed.starts_with('|') {
            output.push_str(&status_row(line));
            output.push('\n');
            continue;
        }
        output.push_str(line);
        output.push('\n');
    }
    output
}

/// Spreadsheet-style panel letter: A … Z, AA, AB, …
/// 电子表格式面板字母：A … Z、AA、AB……
fn panel_letter(mut index: usize) -> String {
    let mut letters = Vec::new();
    loop {
        letters.push(char::from(b'A' + (index % 26) as u8));
        if index < 26 {
            break;
        }
        index = index / 26 - 1;
    }
    letters.iter().rev().collect()
}

/// Replace table cells that start with a status word (`ok`, `no`, `warn`) by its symbol.
/// 将以状态词（`ok`、`no`、`warn`）开头的表格单元格替换为对应符号。
fn status_row(line: &str) -> String {
    line.split('|')
        .map(|cell| {
            let text = cell.trim();
            let (word, rest) = text.split_once(' ').unwrap_or((text, ""));
            let symbol = match word {
                "ok" => "✓",
                "no" => "✗",
                "warn" => "!",
                _ => return cell.to_owned(),
            };
            let rest = rest.trim();
            if rest.is_empty() {
                format!(" {symbol} ")
            } else {
                format!(" {symbol} {rest} ")
            }
        })
        .collect::<Vec<_>>()
        .join("|")
}

/// Append `lines` as a fenced code block labeled `info`.
/// 将 `lines` 作为标注为 `info` 的围栏代码块追加。
fn push_code(output: &mut String, info: &str, lines: &[&str]) {
    output.push_str(&format!("```{info}\n"));
    for line in lines {
        output.push_str(line);
        output.push('\n');
    }
    output.push_str("```\n");
}

/// Split one component row at `|`, keeping exactly `columns` trimmed cells.
/// 按 `|` 拆分一行组件数据，保留恰好 `columns` 个去空白单元格。
fn cells(line: &str, columns: usize) -> Option<Vec<&str>> {
    let cells: Vec<&str> = line.split('|').map(str::trim).collect();
    (cells.len() == columns).then_some(cells)
}

/// Rewrite one closed `ui-<component>` fence; unknown or malformed content is shown as code.
/// 改写一个已闭合的 `ui-<component>` 围栏；未知或格式不符的内容以代码显示。
fn component_markdown(component: &str, lines: &[&str], width: usize, theme: &Theme) -> String {
    let (name, args) = component
        .split_once(char::is_whitespace)
        .map_or((component, ""), |(name, args)| (name, args.trim()));
    let rows: Vec<&str> = lines
        .iter()
        .copied()
        .filter(|line| !line.trim().is_empty())
        .collect();
    let converted = match name {
        "kv" => rows
            .iter()
            .map(|row| cells(row, 2).map(|cell| format!("- **{}**: {}\n", cell[0], cell[1])))
            .collect::<Option<String>>(),
        "metrics" => rows
            .iter()
            .map(|row| cells(row, 4).map(|cell| metric(&cell)))
            .collect::<Option<String>>(),
        "timeline" => rows
            .iter()
            .map(|row| {
                cells(row, 3).map(|cell| format!("- `{}` **{}** — {}\n", cell[0], cell[1], cell[2]))
            })
            .collect::<Option<String>>(),
        "flow" => flow_mermaid(&rows, args).and_then(|source| drawn(source, width, theme)),
        "sequence" => sequence_mermaid(&rows, args).and_then(|source| drawn(source, width, theme)),
        "tree" => Some(tree(&rows)),
        "callout" => Some(format!(
            "> **{}**\n{}\n",
            args.split_once(char::is_whitespace)
                .map_or(args, |(_, title)| title.trim()),
            lines
                .iter()
                .map(|line| format!("> {line}"))
                .collect::<Vec<_>>()
                .join("\n")
        )),
        "details" => Some(format!("**▸ {args}**\n\n{}\n", lines.join("\n"))),
        "tabs" => Some(format!("**{args}**\n\n{}\n", lines.join("\n"))),
        _ => None,
    };
    converted.unwrap_or_else(|| {
        let mut output = String::new();
        push_code(&mut output, &format!("ui-{component}"), lines);
        output
    })
}

/// One metric row as a label, value and a ten-cell bar.
/// 一行指标：标签、数值与十格进度条。
fn metric(cell: &[&str]) -> String {
    let ratio = match (cell[1].parse::<f64>(), cell[2].parse::<f64>()) {
        (Ok(value), Ok(maximum)) if maximum > 0.0 && value.is_finite() => {
            (value / maximum).clamp(0.0, 1.0)
        }
        _ => 0.0,
    };
    let filled = (ratio * 10.0).round() as usize;
    format!(
        "- **{}**: {} / {} {} `{}{}`\n",
        cell[0],
        cell[1],
        cell[2],
        cell[3],
        "█".repeat(filled),
        "░".repeat(10 - filled)
    )
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
