//! Chooses service log lines worth attaching to a problem report.
//! 选取值得附在问题报告中的服务日志行。

use crate::limits::{LOG_EXCERPT_MAX_LINES, LOG_SCAN_MAX_BYTES_PER_SEGMENT, LOG_SCAN_SEGMENTS};
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;

/// Reads the tail of the most recent service log segments, oldest first.
/// 读取最近几个服务日志分段的末尾，按从旧到新排列。
///
/// # Parameters
/// 参数
/// `log_directory` is the service log directory under the authoritative data root.
/// `log_directory` 是权威数据根下的服务日志目录。
///
/// # Returns
/// 返回值
/// Complete lines only; a segment cut at the scan limit drops its first partial line. Missing
/// segments are skipped, so a fresh installation returns an empty string.
/// 只返回完整的行；被扫描上限截断的分段会丢弃第一行残缺内容。缺失的分段直接跳过，因此新安装返回空字符串。
pub fn read_recent_log_text(log_directory: &Path) -> io::Result<String> {
    let mut text = String::new();
    for index in (0..LOG_SCAN_SEGMENTS).rev() {
        let path = log_directory.join(vulcan_runtime_assets::service_log_segment_file_name(index));
        let mut file = match File::open(&path) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error),
        };
        let length = file.metadata()?.len();
        let start = length.saturating_sub(LOG_SCAN_MAX_BYTES_PER_SEGMENT);
        file.seek(SeekFrom::Start(start))?;
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)?;
        let segment = String::from_utf8_lossy(&bytes);
        let complete = if start > 0 {
            segment.split_once('\n').map_or("", |(_, rest)| rest)
        } else {
            &segment
        };
        text.push_str(complete);
        if !text.is_empty() && !text.ends_with('\n') {
            text.push('\n');
        }
    }
    Ok(text)
}

/// Keeps warning and error records plus every record that mentions the failed call.
/// 保留警告与错误记录，以及提到失败调用的全部记录。
///
/// # Parameters
/// 参数
/// `text` holds JSON log lines and `api_call_id` optionally names the failed provider call.
/// `text` 是 JSON 日志行，`api_call_id` 可选地指定失败的供应商调用。
///
/// # Returns
/// 返回值
/// At most the newest `LOG_EXCERPT_MAX_LINES` raw lines; lines that are not JSON objects are ignored.
/// 最多返回最新的 `LOG_EXCERPT_MAX_LINES` 条原始行；不是 JSON 对象的行被忽略。
pub fn select_log_lines(text: &str, api_call_id: Option<&str>) -> Vec<String> {
    let mut selected: Vec<String> = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter(|line| {
            let Ok(serde_json::Value::Object(record)) = serde_json::from_str(line) else {
                return false;
            };
            let severe = record
                .get("level")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|level| {
                    level.eq_ignore_ascii_case("warn") || level.eq_ignore_ascii_case("error")
                });
            severe || api_call_id.is_some_and(|id| !id.is_empty() && line.contains(id))
        })
        .map(str::to_owned)
        .collect();
    if selected.len() > LOG_EXCERPT_MAX_LINES {
        selected.drain(..selected.len() - LOG_EXCERPT_MAX_LINES);
    }
    selected
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn selection_keeps_severe_and_related_records_only() {
        let text = concat!(
            "{\"level\":\"INFO\",\"fields\":{\"message\":\"boot\"}}\n",
            "not json\n",
            "{\"level\":\"WARN\",\"fields\":{\"message\":\"slow\"}}\n",
            "{\"level\":\"INFO\",\"fields\":{\"api_call_id\":\"call-7\"}}\n",
            "{\"level\":\"ERROR\",\"fields\":{\"message\":\"boom\"}}\n",
        );
        let lines = select_log_lines(text, Some("call-7"));
        assert_eq!(lines.len(), 3);
        assert!(lines.iter().all(|line| !line.contains("boot")));
    }

    #[test]
    fn selection_keeps_only_the_newest_lines() {
        let text: String = (0..LOG_EXCERPT_MAX_LINES + 5)
            .map(|index| format!("{{\"level\":\"ERROR\",\"n\":{index}}}\n"))
            .collect();
        let lines = select_log_lines(&text, None);
        assert_eq!(lines.len(), LOG_EXCERPT_MAX_LINES);
        assert!(
            lines[0].contains("\"n\":5}"),
            "oldest kept line moved: {}",
            lines[0]
        );
    }

    #[test]
    fn reading_orders_segments_oldest_first_and_tolerates_missing_files() {
        let root = tempfile::tempdir().unwrap();
        assert_eq!(read_recent_log_text(root.path()).unwrap(), "");
        fs::write(
            root.path()
                .join(vulcan_runtime_assets::service_log_segment_file_name(1)),
            "{\"a\":1}\n",
        )
        .unwrap();
        fs::write(
            root.path()
                .join(vulcan_runtime_assets::service_log_segment_file_name(0)),
            "{\"b\":2}",
        )
        .unwrap();
        assert_eq!(
            read_recent_log_text(root.path()).unwrap(),
            "{\"a\":1}\n{\"b\":2}\n"
        );
    }
}
