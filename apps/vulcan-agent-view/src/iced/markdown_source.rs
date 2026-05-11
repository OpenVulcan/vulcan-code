//! Markdown source normalization and compact-table recovery helpers.
//! Markdown 源码规范化与紧凑表格修复辅助逻辑。

use super::scene::{ParsedMarkdownTable, SelectableTableAlignment};

/// Returns the source preprocessed for transcript rendering, including compact-table repair and footnote rewrites.
/// 返回供转录渲染使用的预处理源码，包含紧凑表格修复与脚注改写。
pub(super) fn prepare_markdown_source_for_rendering(markdown_source: &str) -> String {
    rewrite_markdown_footnotes_for_transcript(&repair_compact_markdown_tables(markdown_source))
}

/// Expands one compacted Markdown table line into multiple logical lines when a valid table boundary pattern is detected.
/// 当检测到有效的表格边界模式时，将单条压缩 Markdown 表格行展开为多条逻辑行。
#[cfg(test)]
pub(super) fn expand_compact_markdown_table_line(line: &str) -> Option<Vec<String>> {
    repair_compact_markdown_table_line(line).map(|(expanded, _)| {
        expanded
            .lines()
            .map(ToOwned::to_owned)
            .collect::<Vec<String>>()
    })
}

/// Rewrites unsupported Markdown footnote syntax into stable readable transcript text while skipping fenced code blocks.
/// 将暂不直接支持的 Markdown 脚注语法改写成稳定可读的转录文本，并跳过围栏代码块。
pub(super) fn rewrite_markdown_footnotes_for_transcript(markdown_source: &str) -> String {
    let mut rewritten_lines = Vec::new();
    let mut active_fence_marker = None::<String>;

    for line in markdown_source.lines() {
        let trimmed_line = line.trim_start();

        if let Some(active_marker) = active_fence_marker.as_deref() {
            rewritten_lines.push(line.to_string());
            if trimmed_line.starts_with(active_marker) {
                active_fence_marker = None;
            }
            continue;
        }

        if let Some(fence_marker) = fenced_code_marker(trimmed_line) {
            active_fence_marker = Some(fence_marker.to_string());
            rewritten_lines.push(line.to_string());
            continue;
        }

        if let Some(rewritten_definition) = rewrite_markdown_footnote_definition_line(line) {
            rewritten_lines.push(rewritten_definition);
        } else {
            rewritten_lines.push(rewrite_markdown_footnote_references_in_line(line));
        }
    }

    rewritten_lines.join("\n")
}

/// Rewrites one Markdown footnote-definition line into a readable transcript paragraph label.
/// 将一条 Markdown 脚注定义行改写成可读的转录段落标签。
fn rewrite_markdown_footnote_definition_line(line: &str) -> Option<String> {
    let content_start = line
        .char_indices()
        .find_map(|(byte_index, character)| (!character.is_whitespace()).then_some(byte_index))
        .unwrap_or(line.len());
    let indent = &line[..content_start];
    let trimmed = &line[content_start..];

    if !trimmed.starts_with("[^") {
        return None;
    }

    let definition_end = trimmed.find("]:")?;
    let footnote_id = &trimmed[2..definition_end];
    if footnote_id.trim().is_empty() {
        return None;
    }

    let body = trimmed[definition_end + 2..].trim_start();
    Some(if body.is_empty() {
        format!("{indent}[{}]", footnote_id.trim())
    } else {
        format!("{indent}[{}] {body}", footnote_id.trim())
    })
}

/// Rewrites inline Markdown footnote references like `[^1]` into readable transcript labels like `[1]`.
/// 将 `[^1]` 这类行内 Markdown 脚注引用改写成 `[1]` 这样的可读转录标签。
fn rewrite_markdown_footnote_references_in_line(line: &str) -> String {
    let mut rewritten = String::new();
    let mut remaining = line;

    while let Some(reference_start) = remaining.find("[^") {
        rewritten.push_str(&remaining[..reference_start]);

        let candidate = &remaining[reference_start + 2..];
        let Some(reference_end) = candidate.find(']') else {
            rewritten.push_str(&remaining[reference_start..]);
            return rewritten;
        };
        let footnote_id = &candidate[..reference_end];

        if footnote_id.trim().is_empty() {
            rewritten.push_str("[^");
            remaining = candidate;
            continue;
        }

        rewritten.push('[');
        rewritten.push_str(footnote_id.trim());
        rewritten.push(']');
        remaining = &candidate[reference_end + 1..];
    }

    rewritten.push_str(remaining);
    rewritten
}

/// Returns the Markdown source normalized for transcript display so single source newlines remain visible in prose paragraphs.
/// 返回适用于转录显示的 Markdown 规范化源码，使正文段落中的单个源码换行保持可见。
pub(super) fn normalize_markdown_source_for_transcript_display(markdown_source: &str) -> String {
    let lines = markdown_source.lines().collect::<Vec<_>>();
    let mut normalized = String::new();
    let mut active_fence_marker = None::<String>;

    for (line_index, line) in lines.iter().enumerate() {
        let trimmed_line = line.trim_start();

        if let Some(fence_marker) = fenced_code_marker(trimmed_line) {
            if active_fence_marker
                .as_deref()
                .is_some_and(|active| trimmed_line.starts_with(active))
            {
                active_fence_marker = None;
            } else if active_fence_marker.is_none() {
                active_fence_marker = Some(fence_marker.to_string());
            }
        }

        normalized.push_str(line);

        let Some(next_line) = lines.get(line_index + 1) else {
            continue;
        };

        if active_fence_marker.is_some()
            || !line_uses_transcript_softbreak_promotion(line)
            || !line_uses_transcript_softbreak_promotion(next_line)
        {
            normalized.push('\n');
        } else {
            normalized.push_str("  \n");
        }
    }

    normalized
}

/// Returns whether the provided source line should promote its trailing soft break into a visible transcript line break.
/// 返回给定源码行是否应将尾部软换行提升为可见的转录换行。
fn line_uses_transcript_softbreak_promotion(line: &str) -> bool {
    let trimmed = line.trim();

    if trimmed.is_empty()
        || trimmed.ends_with("  ")
        || fenced_code_marker(trimmed).is_some()
        || is_markdown_table_body_line(trimmed)
        || is_markdown_table_alignment_line(trimmed)
        || trimmed.starts_with('#')
        || trimmed.starts_with('>')
        || trimmed.starts_with('|')
        || is_unordered_list_line(trimmed)
        || is_ordered_list_line(trimmed)
        || is_thematic_break_line(trimmed)
    {
        return false;
    }

    true
}

/// Returns the fence marker used by one fenced-code line when it opens or closes a Markdown fence.
/// 返回围栏代码行在开启或关闭 Markdown 围栏时使用的围栏标记。
pub(super) fn fenced_code_marker(line: &str) -> Option<&'static str> {
    if line.starts_with("```") {
        Some("```")
    } else if line.starts_with("~~~") {
        Some("~~~")
    } else {
        None
    }
}

/// Returns whether the provided line looks like one unordered Markdown list item.
/// 返回给定行是否像一个无序 Markdown 列表项。
fn is_unordered_list_line(line: &str) -> bool {
    ["- ", "* ", "+ "]
        .into_iter()
        .any(|prefix| line.starts_with(prefix))
}

/// Returns whether the provided line looks like one ordered Markdown list item.
/// 返回给定行是否像一个有序 Markdown 列表项。
fn is_ordered_list_line(line: &str) -> bool {
    let mut digits = 0usize;

    for character in line.chars() {
        if character.is_ascii_digit() {
            digits += 1;
            continue;
        }

        return digits > 0 && matches!(character, '.' | ')');
    }

    false
}

/// Returns whether the provided line looks like one thematic-break Markdown line.
/// 返回给定行是否像一个 Markdown 水平分隔线。
fn is_thematic_break_line(line: &str) -> bool {
    let compact = line.chars().filter(|character| !character.is_whitespace());
    let collected = compact.collect::<String>();
    collected.len() >= 3
        && (collected.chars().all(|character| character == '-')
            || collected.chars().all(|character| character == '*')
            || collected.chars().all(|character| character == '_'))
}

/// Returns whether the Markdown source contains at least one table header and alignment line pair.
/// 返回 Markdown 源文本是否至少包含一组表头行与对齐行。
pub(super) fn raw_source_contains_markdown_table(markdown_source: &str) -> bool {
    let repaired_source = repair_compact_markdown_tables(markdown_source);
    let lines = repaired_source.lines().collect::<Vec<_>>();
    let mut active_fence_marker = None::<&'static str>;

    for window in lines.windows(2) {
        let current_trimmed = window[0].trim_start();

        if let Some(active_marker) = active_fence_marker {
            if current_trimmed.starts_with(active_marker) {
                active_fence_marker = None;
            }
            continue;
        }

        if let Some(fence_marker) = fenced_code_marker(current_trimmed) {
            active_fence_marker = Some(fence_marker);
            continue;
        }

        if is_markdown_table_header_line(window[0]) && is_markdown_table_alignment_line(window[1]) {
            return true;
        }
    }

    false
}

/// Repairs compact Markdown table clusters so rows glued onto one physical line can still be recognized by the custom transcript renderer.
/// 修复紧凑压缩的 Markdown 表格片段，使粘在同一物理行上的多行表格仍能被自定义转录渲染器识别。
pub(super) fn repair_compact_markdown_tables(markdown_source: &str) -> String {
    let mut repaired_lines = Vec::new();
    let mut active_table_column_count = None::<usize>;
    let mut active_fence_marker = None::<&'static str>;

    for line in markdown_source.lines() {
        let trimmed_line = line.trim_start();

        if let Some(active_marker) = active_fence_marker {
            repaired_lines.push(line.to_string());
            if trimmed_line.starts_with(active_marker) {
                active_fence_marker = None;
            }
            continue;
        }

        if let Some(fence_marker) = fenced_code_marker(trimmed_line) {
            active_fence_marker = Some(fence_marker);
            active_table_column_count = None;
            repaired_lines.push(line.to_string());
            continue;
        }

        if let Some(column_count) = active_table_column_count {
            if let Some(expanded_body) = repair_compact_markdown_table_body_line(line, column_count)
            {
                let expanded_lines = expanded_body
                    .lines()
                    .map(ToOwned::to_owned)
                    .collect::<Vec<_>>();
                active_table_column_count = expanded_lines.last().and_then(|last_line| {
                    table_line_column_count(last_line)
                        .filter(|last_column_count| *last_column_count == column_count)
                });
                repaired_lines.extend(expanded_lines);
                continue;
            }

            if table_line_column_count(line)
                .is_some_and(|line_column_count| line_column_count == column_count)
                || is_markdown_table_alignment_line(line)
            {
                repaired_lines.push(line.to_string());
                continue;
            }

            active_table_column_count = None;
        }

        if let Some((expanded_table, column_count)) = repair_compact_markdown_table_line(line) {
            let expanded_lines = expanded_table
                .lines()
                .map(ToOwned::to_owned)
                .collect::<Vec<_>>();
            active_table_column_count = expanded_lines.last().and_then(|last_line| {
                table_line_column_count(last_line)
                    .filter(|last_column_count| *last_column_count == column_count)
            });
            repaired_lines.extend(expanded_lines);
            continue;
        }

        repaired_lines.push(line.to_string());
    }

    repaired_lines.join("\n")
}

/// Repairs one compact Markdown table line when a prose prefix or multiple table rows were collapsed onto the same line.
/// 当正文前缀或多行表格被压到同一行时，修复该紧凑 Markdown 表格行。
fn repair_compact_markdown_table_line(line: &str) -> Option<(String, usize)> {
    if !line.contains("||") || line.matches('|').count() < 6 {
        return None;
    }

    for (table_start, _) in line.match_indices('|') {
        let prefix = line[..table_start].trim_end();
        let table_candidate = &line[table_start..];

        let Some((expanded_table, column_count)) =
            parse_compact_markdown_table_cluster(table_candidate)
        else {
            continue;
        };

        if prefix.is_empty() {
            return Some((expanded_table, column_count));
        }

        return Some((format!("{prefix}\n{expanded_table}"), column_count));
    }

    None
}

/// Repairs one compact Markdown table body line using the active table column count so glued body rows can be restored.
/// 使用当前活动表格的列数修复一条紧凑 Markdown 表格正文行，使被粘连的正文行恢复正常。
fn repair_compact_markdown_table_body_line(line: &str, column_count: usize) -> Option<String> {
    if column_count < 2 || !line.contains("||") {
        return None;
    }

    let content_start = line
        .char_indices()
        .find_map(|(byte_index, character)| (!character.is_whitespace()).then_some(byte_index))
        .unwrap_or(line.len());
    let prefix = &line[..content_start];
    let mut rest = &line[content_start..];
    let mut repaired_rows = Vec::new();

    loop {
        rest = rest.trim_start_matches([' ', '\t']);

        if !rest.starts_with('|') {
            break;
        }

        let Some((row_end, row_source)) = consume_compact_markdown_table_row(rest, column_count)
        else {
            break;
        };

        if table_line_column_count(row_source)
            .filter(|row_column_count| *row_column_count == column_count)
            .is_none()
        {
            break;
        }

        repaired_rows.push(format!("{prefix}{}", row_source.trim()));
        rest = &rest[row_end..];
    }

    if repaired_rows.len() < 2 {
        return None;
    }

    let suffix = rest.trim_start();
    let mut repaired = repaired_rows.join("\n");
    if !suffix.is_empty() {
        repaired.push('\n');
        repaired.push_str(suffix);
    }

    Some(repaired)
}

/// Parses one compact Markdown table cluster into a newline-separated block when row boundaries were collapsed into `||`.
/// 当行边界被压缩成 `||` 时，将一个紧凑 Markdown 表格片段解析为按换行分隔的区块。
fn parse_compact_markdown_table_cluster(table_source: &str) -> Option<(String, usize)> {
    for (boundary_index, _) in table_source.match_indices("||") {
        let header_row = &table_source[..boundary_index + 1];
        let header_cells = parse_markdown_table_cells(header_row);
        if header_cells.len() < 2 {
            continue;
        }

        let alignment_and_rest = &table_source[boundary_index + 1..];
        let Some((alignment_end, alignment_row)) =
            consume_compact_markdown_table_row(alignment_and_rest, header_cells.len())
        else {
            continue;
        };

        if !is_markdown_table_alignment_line(alignment_row) {
            continue;
        }

        let mut rows = vec![
            header_row.trim().to_string(),
            alignment_row.trim().to_string(),
        ];
        let mut rest = &alignment_and_rest[alignment_end..];

        loop {
            rest = rest.trim_start_matches([' ', '\t']);

            if !rest.starts_with('|') {
                break;
            }

            let Some((body_end, body_row)) =
                consume_compact_markdown_table_row(rest, header_cells.len())
            else {
                break;
            };

            rows.push(body_row.trim().to_string());
            rest = &rest[body_end..];
        }

        let suffix = rest.trim_start();
        let mut expanded = rows.join("\n");
        if !suffix.is_empty() {
            expanded.push('\n');
            expanded.push_str(suffix);
        }

        return Some((expanded, header_cells.len()));
    }

    None
}

/// Consumes one compact Markdown table row with the expected column count and returns the byte length together with the row slice.
/// 按预期列数消费一行紧凑 Markdown 表格，并返回其字节长度和该行切片。
fn consume_compact_markdown_table_row(
    row_source: &str,
    column_count: usize,
) -> Option<(usize, &str)> {
    if !row_source.starts_with('|') || column_count < 2 {
        return None;
    }

    let mut pipe_count = 0usize;

    for (byte_index, character) in row_source.char_indices() {
        if character == '|' {
            pipe_count += 1;

            if pipe_count == column_count + 1 {
                let row_end = byte_index + character.len_utf8();
                return Some((row_end, &row_source[..row_end]));
            }
        } else if character == '\n' {
            return None;
        }
    }

    None
}

/// Returns whether the provided line can serve as the header row of one Markdown table.
/// 返回给定行是否可以作为 Markdown 表格的表头行。
pub(super) fn is_markdown_table_header_line(line: &str) -> bool {
    let cells = split_markdown_table_cells(line);
    cells.len() >= 2
}

/// Returns whether the provided line can serve as the alignment row of one Markdown table.
/// 返回给定行是否可以作为 Markdown 表格的对齐行。
pub(super) fn is_markdown_table_alignment_line(line: &str) -> bool {
    let cells = split_markdown_table_cells(line);

    cells.len() >= 2
        && cells.iter().all(|cell| {
            let trimmed = cell.trim();
            !trimmed.is_empty()
                && trimmed.contains('-')
                && trimmed
                    .chars()
                    .all(|character| matches!(character, '-' | ':' | ' '))
        })
}

/// Returns whether the provided line can serve as one body row of one Markdown table.
/// 返回给定行是否可以作为 Markdown 表格的一行正文。
pub(super) fn is_markdown_table_body_line(line: &str) -> bool {
    split_markdown_table_cells(line).len() >= 2
}

/// Returns the cell count when the provided line already matches one well-formed Markdown table row.
/// 当给定行已经符合结构正常的 Markdown 表格行时，返回其单元格数量。
fn table_line_column_count(line: &str) -> Option<usize> {
    let cells = split_markdown_table_cells(line);
    (!cells.is_empty()).then_some(cells.len())
}

/// Splits one Markdown table row into trimmed raw cell sources without interpreting inline Markdown syntax.
/// 将一行 Markdown 表格拆分为去除首尾空白的原始单元格源码，而不解释其中的行内 Markdown 语法。
fn split_markdown_table_cells(line: &str) -> Vec<String> {
    let trimmed = line.trim();
    if !trimmed.contains('|') {
        return Vec::new();
    }

    let trimmed = trimmed.trim_matches('|');
    if trimmed.is_empty() {
        return Vec::new();
    }

    trimmed
        .split('|')
        .map(|cell| cell.trim().to_string())
        .collect::<Vec<_>>()
}

/// Parses one Markdown table row into trimmed cell payloads.
/// 将一行 Markdown 表格解析为去除首尾空白的单元格载荷。
fn parse_markdown_table_cells(line: &str) -> Vec<String> {
    split_markdown_table_cells(line)
        .into_iter()
        .map(|cell| {
            crate::markdown_source_to_plain_text(cell.trim())
                .replace('\n', " ")
                .trim()
                .to_string()
        })
        .collect::<Vec<_>>()
}

/// Parses one Markdown table source slice into a structured table payload ready for the custom renderer.
/// 将一段 Markdown 表格源码解析为供自定义渲染器使用的结构化表格载荷。
pub(super) fn parse_markdown_table_source(table_lines: &[&str]) -> Option<ParsedMarkdownTable> {
    let header_cells = parse_markdown_table_cells(*table_lines.first()?);
    let alignment_cells = split_markdown_table_cells(*table_lines.get(1)?);
    if header_cells.is_empty() || header_cells.len() != alignment_cells.len() {
        return None;
    }

    let body_rows = table_lines
        .iter()
        .skip(2)
        .map(|line| parse_markdown_table_cells(line))
        .collect::<Vec<_>>();
    let alignments = alignment_cells
        .iter()
        .map(|cell| parse_markdown_table_alignment(cell))
        .collect::<Vec<_>>();
    Some(ParsedMarkdownTable {
        header_cells,
        alignments,
        body_rows,
    })
}

/// Parses one Markdown table alignment cell into the corresponding selectable grid alignment.
/// 将一个 Markdown 表格对齐单元格解析为对应的可选网格对齐方式。
pub(super) fn parse_markdown_table_alignment(cell: &str) -> SelectableTableAlignment {
    let trimmed = cell.trim();

    match (trimmed.starts_with(':'), trimmed.ends_with(':')) {
        (true, true) => SelectableTableAlignment::Center,
        (false, true) => SelectableTableAlignment::Right,
        _ => SelectableTableAlignment::Left,
    }
}
