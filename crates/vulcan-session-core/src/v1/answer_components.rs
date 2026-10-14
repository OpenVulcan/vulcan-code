//! Bounds of the `ui-*` answer components that assistant Markdown may contain.
//! 助手 Markdown 中可包含的 `ui-*` 回答组件的边界。
//!
//! The WebUI and the terminal client each render the same fences natively; these values are the
//! single authority both read (the WebUI through generated constants). A fence exceeding them is
//! shown as ordinary code instead of a component.
//! WebUI 与终端客户端各自原生渲染同一组围栏；以下数值是两端共同读取的唯一权威（WebUI 经生成常量读取）。
//! 超出边界的围栏按普通代码显示，而非组件。
//!
//! `ui-code` quotes real file lines. The host captures each quoted range once, when the reply is
//! sealed, and stores it on the assistant event, so the reply keeps showing the code it was written
//! against even after the file changes.
//! `ui-code` 引用真实文件行。宿主在回答封口时把每个引用范围抓取一次并存入助手事件，
//! 使文件之后变化时回答仍显示其写作当时的代码。

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::v1::errors::DomainError;

/// Most data rows (items, categories, files) one component renders.
/// 单个组件渲染的最多数据行（条目、分类、文件）数。
pub const ANSWER_COMPONENT_MAX_ROWS: usize = 128;

/// Most value series one `ui-chart` carries; equals the number of chart palette colors.
/// 单个 `ui-chart` 承载的最多数值系列数；等于图表配色数量。
pub const ANSWER_CHART_MAX_SERIES: usize = 8;

/// Most lines one `ui-code` excerpt quotes.
/// 单个 `ui-code` 摘录引用的最多行数。
pub const ANSWER_CODE_MAX_LINES: usize = 200;

/// Most distinct `ui-code` excerpts one reply captures; also the row limit of one `ui-code` fence.
/// 单条回答抓取的最多不同 `ui-code` 摘录数；同时是单个 `ui-code` 围栏的行数上限。
pub const ANSWER_CODE_MAX_EXCERPTS: usize = 16;

/// Most UTF-8 bytes one captured excerpt stores; a larger range is recorded as too large.
/// 单个已抓取摘录存储的最多 UTF-8 字节数；更大的范围记录为过大。
pub const ANSWER_CODE_MAX_BYTES: usize = 64 * 1024;

/// Fence info string of the code excerpt component.
/// 代码摘录组件的围栏信息串。
const ANSWER_CODE_FENCE: &str = "ui-code";

/// One parsed `path:start-end` location cell of a `ui-code` row.
/// `ui-code` 行中一个已解析的 `path:start-end` 位置单元格。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnswerCodeLocation {
    /// Exact trimmed cell text; the key that pairs a row with its snapshot.
    /// 精确的去空白单元格文本；将行与其快照配对的键。
    pub location: String,
    /// File path, absolute or relative to the session workspace.
    /// 文件路径，绝对路径或相对会话工作区的路径。
    pub path: String,
    /// First quoted line, one-based.
    /// 引用的首行，从 1 开始。
    pub start_line: u32,
    /// Last requested line, inclusive; the captured text may end earlier at the end of the file.
    /// 请求的末行（含）；文件提前结束时抓取的正文可更早结束。
    pub end_line: u32,
}

/// Parses `path:start-end` or `path:line`; returns `None` for any other shape or an empty or
/// oversized range.
/// 解析 `path:start-end` 或 `path:line`；其他形状、空范围或超长范围返回 `None`。
pub fn parse_answer_code_location(cell: &str) -> Option<AnswerCodeLocation> {
    let location = cell.trim();
    // The last colon separates the range, so drive letters such as `C:\` stay in the path.
    // 最后一个冒号分隔范围，使 `C:\` 这类盘符留在路径中。
    let (path, range) = location.rsplit_once(':')?;
    let (start, end) = range.split_once('-').unwrap_or((range, range));
    let digits = |text: &str| !text.is_empty() && text.bytes().all(|byte| byte.is_ascii_digit());
    if path.trim() != path || path.is_empty() || !digits(start) || !digits(end) {
        return None;
    }
    let start_line: u32 = start.parse().ok()?;
    let end_line: u32 = end.parse().ok()?;
    let lines = usize::try_from(end_line.checked_sub(start_line)?).ok()? + 1;
    (start_line >= 1 && lines <= ANSWER_CODE_MAX_LINES).then(|| AnswerCodeLocation {
        location: location.to_owned(),
        path: path.to_owned(),
        start_line,
        end_line,
    })
}

/// Collects the distinct locations quoted by the closed `ui-code` fences of `text`, in order of
/// first appearance and at most [`ANSWER_CODE_MAX_EXCERPTS`].
/// 按首次出现顺序收集 `text` 中已闭合 `ui-code` 围栏引用的不同位置，至多 [`ANSWER_CODE_MAX_EXCERPTS`] 个。
///
/// Fences inside `ui-*` containers count because containers hold Markdown; fences inside any other
/// code block are source text and do not.
/// `ui-*` 容器内的围栏计入，因为容器承载 Markdown；其他代码块内的围栏是源码文本，不计入。
pub fn answer_code_locations(text: &str) -> Vec<AnswerCodeLocation> {
    let lines: Vec<&str> = text.lines().collect();
    let mut seen = BTreeSet::new();
    let mut found = Vec::new();
    let mut index = 0;
    while index < lines.len() {
        let line = lines[index].trim_start();
        index += 1;
        let ticks = line.bytes().take_while(|byte| *byte == b'`').count();
        if ticks < 3 {
            continue;
        }
        let info = line[ticks..].trim();
        if info.starts_with("ui-") && info != ANSWER_CODE_FENCE {
            continue;
        }
        let close = lines[index..].iter().position(|candidate| {
            let candidate = candidate.trim();
            let closing = candidate.bytes().take_while(|byte| *byte == b'`').count();
            closing >= ticks && closing == candidate.len()
        });
        let Some(close) = close else {
            break;
        };
        if info == ANSWER_CODE_FENCE {
            for row in &lines[index..index + close] {
                let first = row.trim().trim_start_matches('|').split('|').next();
                let Some(location) = first.and_then(parse_answer_code_location) else {
                    continue;
                };
                if found.len() < ANSWER_CODE_MAX_EXCERPTS && seen.insert(location.location.clone())
                {
                    found.push(location);
                }
            }
        }
        index += close + 1;
    }
    found
}

/// One `ui-code` location and what the host read there when the reply was sealed.
/// 一个 `ui-code` 位置及回答封口时宿主在该处读到的内容。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AnswerCodeSnapshot {
    /// Exact trimmed location cell, as parsed by [`parse_answer_code_location`].
    /// 精确的去空白位置单元格，按 [`parse_answer_code_location`] 解析。
    pub location: String,
    /// Captured lines or the reason they are unavailable.
    /// 已抓取的行，或无法抓取的原因。
    pub result: AnswerCodeSnapshotResult,
}

/// Outcome of capturing one excerpt.
/// 抓取单个摘录的结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case", deny_unknown_fields)]
pub enum AnswerCodeSnapshotResult {
    /// Lines from the location's start line, joined by `\n` without line terminators; fewer than
    /// requested when the file ends inside the range.
    /// 从位置首行起的各行，以 `\n` 连接且不含行终止符；文件在范围内结束时少于请求行数。
    Captured {
        /// Captured text.
        /// 已抓取的正文。
        text: String,
    },
    /// The range could not be captured.
    /// 该范围无法抓取。
    Unavailable {
        /// Stable reason the client localizes.
        /// 由客户端本地化的稳定原因。
        reason: AnswerCodeUnavailableReason,
    },
}

/// Stable reason one excerpt could not be captured.
/// 单个摘录无法抓取的稳定原因。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AnswerCodeUnavailableReason {
    /// No regular file exists at the path.
    /// 该路径下不存在普通文件。
    NotFound,
    /// A relative path was quoted but the session has no workspace to resolve it against.
    /// 引用了相对路径，但会话没有可用于解析的工作区。
    NoWorkspace,
    /// The file ends before the start line.
    /// 文件在首行之前结束。
    BeyondEnd,
    /// The quoted lines are not UTF-8 text.
    /// 引用的行不是 UTF-8 文本。
    NotText,
    /// The quoted lines exceed [`ANSWER_CODE_MAX_BYTES`].
    /// 引用的行超过 [`ANSWER_CODE_MAX_BYTES`]。
    TooLarge,
    /// The file exists but could not be read.
    /// 文件存在但无法读取。
    ReadFailed,
}

/// Checks that `snapshots` are distinct, parseable and within the excerpt bounds.
/// 校验 `snapshots` 互不重复、可解析且在摘录边界之内。
pub fn validate_answer_code_snapshots(snapshots: &[AnswerCodeSnapshot]) -> Result<(), DomainError> {
    let invalid = |field| Err(DomainError::InvalidAnswerCodeSnapshots { field });
    if snapshots.len() > ANSWER_CODE_MAX_EXCERPTS {
        return invalid("count");
    }
    let mut seen = BTreeSet::new();
    for snapshot in snapshots {
        let Some(location) = parse_answer_code_location(&snapshot.location) else {
            return invalid("location");
        };
        if location.location != snapshot.location || !seen.insert(snapshot.location.as_str()) {
            return invalid("location");
        }
        if let AnswerCodeSnapshotResult::Captured { text } = &snapshot.result {
            let lines = text.split('\n').count();
            let requested = (location.end_line - location.start_line) as usize + 1;
            if lines > requested || text.contains('\r') {
                return invalid("text_lines");
            }
            if text.len() > ANSWER_CODE_MAX_BYTES {
                return invalid("text_bytes");
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn location_accepts_ranges_single_lines_and_drive_letters() {
        let range = parse_answer_code_location(" src/a.rs:40-60 ").expect("range");
        assert_eq!(
            (
                range.location.as_str(),
                range.path.as_str(),
                range.start_line,
                range.end_line
            ),
            ("src/a.rs:40-60", "src/a.rs", 40, 60)
        );
        let single = parse_answer_code_location("src/a.rs:7").expect("single line");
        assert_eq!((single.start_line, single.end_line), (7, 7));
        let windows = parse_answer_code_location(r"C:\repo\a.rs:1-3").expect("drive letter");
        assert_eq!(windows.path, r"C:\repo\a.rs");
    }

    #[test]
    fn location_rejects_missing_empty_reversed_and_oversized_ranges() {
        for cell in [
            "src/a.rs",
            ":1-2",
            "src/a.rs:0-3",
            "src/a.rs:9-3",
            "src/a.rs:1-",
            "src/a.rs:a-b",
        ] {
            assert!(
                parse_answer_code_location(cell).is_none(),
                "{cell} must be rejected"
            );
        }
        let limit = ANSWER_CODE_MAX_LINES as u32;
        assert!(parse_answer_code_location(&format!("a.rs:1-{limit}")).is_some());
        assert!(parse_answer_code_location(&format!("a.rs:1-{}", limit + 1)).is_none());
    }

    #[test]
    fn locations_come_from_closed_fences_including_containers_but_not_code_blocks() {
        let text = "\
```ui-code
src/a.rs:1-3 | 2 | first
| src/b.rs:4 |
src/a.rs:1-3
```

````ui-details More
```ui-code
src/c.rs:5-6
```
````

```text
```ui-code
src/ignored.rs:1-2
```

```ui-code
src/unclosed.rs:1-2
";
        let locations: Vec<String> = answer_code_locations(text)
            .into_iter()
            .map(|location| location.location)
            .collect();
        assert_eq!(locations, ["src/a.rs:1-3", "src/b.rs:4", "src/c.rs:5-6"]);
    }

    #[test]
    fn locations_stop_at_the_excerpt_limit() {
        let rows: Vec<String> = (1..=ANSWER_CODE_MAX_EXCERPTS + 3)
            .map(|line| format!("a.rs:{line}"))
            .collect();
        let text = format!("```ui-code\n{}\n```\n", rows.join("\n"));
        assert_eq!(answer_code_locations(&text).len(), ANSWER_CODE_MAX_EXCERPTS);
    }

    #[test]
    fn snapshots_validate_bounds_and_round_trip_through_json() {
        let captured = AnswerCodeSnapshot {
            location: "src/a.rs:1-2".into(),
            result: AnswerCodeSnapshotResult::Captured {
                text: "fn a() {\n}".into(),
            },
        };
        let missing = AnswerCodeSnapshot {
            location: "src/b.rs:3".into(),
            result: AnswerCodeSnapshotResult::Unavailable {
                reason: AnswerCodeUnavailableReason::NotFound,
            },
        };
        let snapshots = vec![captured.clone(), missing];
        validate_answer_code_snapshots(&snapshots).expect("valid snapshots");
        let json = serde_json::to_value(&snapshots).expect("serialize");
        assert_eq!(json[0]["result"]["status"], "captured");
        assert_eq!(json[1]["result"]["reason"], "not_found");
        let decoded: Vec<AnswerCodeSnapshot> = serde_json::from_value(json).expect("deserialize");
        assert_eq!(decoded, snapshots);

        let too_many_lines = AnswerCodeSnapshot {
            result: AnswerCodeSnapshotResult::Captured {
                text: "a\nb\nc".into(),
            },
            ..captured.clone()
        };
        assert!(validate_answer_code_snapshots(&[too_many_lines]).is_err());
        assert!(validate_answer_code_snapshots(&[captured.clone(), captured]).is_err());
    }
}
