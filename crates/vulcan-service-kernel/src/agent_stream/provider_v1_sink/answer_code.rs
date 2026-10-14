//! Answer-time capture of the file lines quoted by `ui-code` fences.
//! 回答封口时抓取 `ui-code` 围栏引用的文件行。
//!
//! The capture runs once, when the assistant text is sealed, so a reply keeps showing the code it
//! was written against. The snapshots are display-only and never reach a Provider.
//! 抓取只在助手正文封口时执行一次，使回答保持显示其写作当时的代码。快照仅用于展示，绝不进入 Provider。

use std::fs::File;
use std::io::{BufRead, BufReader, Read};
use std::path::{Path, PathBuf};

use vulcan_session_core::v1::answer_components::{
    ANSWER_CODE_MAX_BYTES, AnswerCodeLocation, AnswerCodeSnapshot, AnswerCodeSnapshotResult,
    AnswerCodeUnavailableReason, answer_code_locations,
};

/// Captures every location quoted by the closed `ui-code` fences of `text`.
/// 抓取 `text` 中已闭合 `ui-code` 围栏引用的每个位置。
///
/// `root` resolves relative paths and is absent when the session has no workspace. Returns one
/// snapshot per distinct location, in order of first appearance; a reply without `ui-code` fences
/// returns an empty list without touching the file system.
/// `root` 用于解析相对路径，会话没有工作区时不存在。按首次出现顺序为每个不同位置返回一个快照；
/// 没有 `ui-code` 围栏的回答返回空列表，且不访问文件系统。
pub(super) async fn capture_answer_code_snapshots(
    root: Option<PathBuf>,
    text: &str,
) -> Vec<AnswerCodeSnapshot> {
    let locations = answer_code_locations(text);
    if locations.is_empty() {
        return Vec::new();
    }
    let fallback = locations.clone();
    tokio::task::spawn_blocking(move || {
        locations
            .iter()
            .map(|location| snapshot(location, capture_excerpt(root.as_deref(), location)))
            .collect()
    })
    .await
    // A panicked reader still seals the reply; every excerpt then reports a read failure.
    // 读取线程崩溃时仍封口回答；每个摘录随之报告读取失败。
    .unwrap_or_else(|_| {
        fallback
            .iter()
            .map(|location| {
                snapshot(
                    location,
                    AnswerCodeSnapshotResult::Unavailable {
                        reason: AnswerCodeUnavailableReason::ReadFailed,
                    },
                )
            })
            .collect()
    })
}

/// Pairs one location with its capture result.
/// 将一个位置与其抓取结果配对。
fn snapshot(location: &AnswerCodeLocation, result: AnswerCodeSnapshotResult) -> AnswerCodeSnapshot {
    AnswerCodeSnapshot {
        location: location.location.clone(),
        result,
    }
}

/// Reads the lines of `location`, resolving a relative path against `root`.
/// 读取 `location` 的各行，相对路径按 `root` 解析。
///
/// Lines before the range are skipped without buffering them, and the captured lines are bounded
/// by [`ANSWER_CODE_MAX_BYTES`], so a huge or minified file cannot exhaust memory.
/// 范围之前的行跳过且不缓冲，抓取的行受 [`ANSWER_CODE_MAX_BYTES`] 限制，因此巨大或压缩过的文件不会耗尽内存。
fn capture_excerpt(root: Option<&Path>, location: &AnswerCodeLocation) -> AnswerCodeSnapshotResult {
    let unavailable = |reason| AnswerCodeSnapshotResult::Unavailable { reason };
    let path = Path::new(&location.path);
    let full = if path.is_absolute() {
        path.to_path_buf()
    } else if let Some(root) = root {
        root.join(path)
    } else {
        return unavailable(AnswerCodeUnavailableReason::NoWorkspace);
    };
    match std::fs::metadata(&full) {
        Ok(metadata) if metadata.is_file() => {}
        Ok(_) => return unavailable(AnswerCodeUnavailableReason::NotFound),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return unavailable(AnswerCodeUnavailableReason::NotFound);
        }
        Err(_) => return unavailable(AnswerCodeUnavailableReason::ReadFailed),
    }
    let Ok(file) = File::open(&full) else {
        return unavailable(AnswerCodeUnavailableReason::ReadFailed);
    };
    let mut reader = BufReader::new(file);
    for _ in 1..location.start_line {
        match reader.skip_until(b'\n') {
            Ok(0) => return unavailable(AnswerCodeUnavailableReason::BeyondEnd),
            Ok(_) => {}
            Err(_) => return unavailable(AnswerCodeUnavailableReason::ReadFailed),
        }
    }
    let mut lines = Vec::new();
    let mut captured_bytes = 0usize;
    for _ in location.start_line..=location.end_line {
        let mut line = Vec::new();
        // One byte beyond the remaining budget proves the excerpt is too large.
        // 超出剩余额度一个字节即证明摘录过大。
        let budget = (ANSWER_CODE_MAX_BYTES - captured_bytes + 1) as u64;
        match (&mut reader).take(budget + 2).read_until(b'\n', &mut line) {
            Ok(0) => break,
            Ok(_) => {}
            Err(_) => return unavailable(AnswerCodeUnavailableReason::ReadFailed),
        }
        if line.last() == Some(&b'\n') {
            line.pop();
        }
        if line.last() == Some(&b'\r') {
            line.pop();
        }
        if line.contains(&0) {
            return unavailable(AnswerCodeUnavailableReason::NotText);
        }
        // A carriage return inside a line is not a line terminator of any text format.
        // 行内回车不属于任何文本格式的行终止符。
        let Ok(line) = String::from_utf8(line) else {
            return unavailable(AnswerCodeUnavailableReason::NotText);
        };
        if line.contains('\r') {
            return unavailable(AnswerCodeUnavailableReason::NotText);
        }
        // Joined text counts one `\n` between lines.
        // 连接后的正文在行间计入一个 `\n`。
        captured_bytes += line.len() + usize::from(!lines.is_empty());
        if captured_bytes > ANSWER_CODE_MAX_BYTES {
            return unavailable(AnswerCodeUnavailableReason::TooLarge);
        }
        lines.push(line);
    }
    if lines.is_empty() {
        return unavailable(AnswerCodeUnavailableReason::BeyondEnd);
    }
    AnswerCodeSnapshotResult::Captured {
        text: lines.join("\n"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vulcan_session_core::v1::answer_components::validate_answer_code_snapshots;

    fn result(root: &Path, cell: &str) -> AnswerCodeSnapshotResult {
        let location = vulcan_session_core::v1::answer_components::parse_answer_code_location(cell)
            .expect("location");
        capture_excerpt(Some(root), &location)
    }

    fn captured(text: &str) -> AnswerCodeSnapshotResult {
        AnswerCodeSnapshotResult::Captured { text: text.into() }
    }

    fn unavailable(reason: AnswerCodeUnavailableReason) -> AnswerCodeSnapshotResult {
        AnswerCodeSnapshotResult::Unavailable { reason }
    }

    #[test]
    fn excerpts_capture_ranges_clamp_at_eof_and_report_failures() {
        let dir = tempfile::tempdir().expect("temp dir");
        std::fs::create_dir(dir.path().join("src")).expect("src dir");
        std::fs::write(dir.path().join("src/a.rs"), "one\r\ntwo\nthree\n").expect("write");
        std::fs::write(dir.path().join("bin.dat"), b"ok\n\0\xff\n").expect("write binary");
        let root = dir.path();

        assert_eq!(result(root, "src/a.rs:2-3"), captured("two\nthree"));
        assert_eq!(result(root, "src/a.rs:1"), captured("one"));
        assert_eq!(result(root, "src/a.rs:3-9"), captured("three"));
        assert_eq!(
            result(root, "src/a.rs:5-6"),
            unavailable(AnswerCodeUnavailableReason::BeyondEnd)
        );
        assert_eq!(
            result(root, "src/missing.rs:1"),
            unavailable(AnswerCodeUnavailableReason::NotFound)
        );
        assert_eq!(
            result(root, "src:1"),
            unavailable(AnswerCodeUnavailableReason::NotFound)
        );
        assert_eq!(result(root, "bin.dat:1"), captured("ok"));
        assert_eq!(
            result(root, "bin.dat:2"),
            unavailable(AnswerCodeUnavailableReason::NotText)
        );
        let absolute = format!("{}:1", root.join("src/a.rs").display());
        assert_eq!(result(Path::new("/unused"), &absolute), captured("one"));
        let relative =
            vulcan_session_core::v1::answer_components::parse_answer_code_location("src/a.rs:1")
                .expect("location");
        assert_eq!(
            capture_excerpt(None, &relative),
            unavailable(AnswerCodeUnavailableReason::NoWorkspace)
        );
    }

    #[test]
    fn excerpts_over_the_byte_bound_are_too_large() {
        let dir = tempfile::tempdir().expect("temp dir");
        let long = "x".repeat(ANSWER_CODE_MAX_BYTES + 10);
        std::fs::write(dir.path().join("big.txt"), format!("{long}\nshort\n")).expect("write");
        assert_eq!(
            result(dir.path(), "big.txt:1"),
            unavailable(AnswerCodeUnavailableReason::TooLarge)
        );
        assert_eq!(result(dir.path(), "big.txt:2"), captured("short"));
    }

    #[tokio::test]
    async fn replies_capture_each_distinct_location_once() {
        let dir = tempfile::tempdir().expect("temp dir");
        std::fs::write(dir.path().join("a.rs"), "fn a() {}\n").expect("write");
        let text = "Before.\n\n```ui-code\na.rs:1 | 1 | entry\na.rs:1\nb.rs:2-3\n```\n";
        let snapshots = capture_answer_code_snapshots(Some(dir.path().to_path_buf()), text).await;
        validate_answer_code_snapshots(&snapshots).expect("valid snapshots");
        assert_eq!(snapshots.len(), 2);
        assert_eq!(snapshots[0].result, captured("fn a() {}"));
        assert_eq!(
            snapshots[1].result,
            unavailable(AnswerCodeUnavailableReason::NotFound)
        );
        assert!(
            capture_answer_code_snapshots(None, "no fences")
                .await
                .is_empty()
        );
    }
}
