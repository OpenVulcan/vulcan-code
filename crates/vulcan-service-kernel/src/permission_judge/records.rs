//! Decision log of the permission judge: every decision the judge or the reviewer took part in,
//! kept in one JSON Lines file under the user's Vulcan directory with a rolling limit.
//! 权限判定的判定记录：判定器或复核器参与的每次决定，以带滚动上限的 JSON Lines 文件保存在用户的
//! Vulcan 目录下。
//!
//! The log keeps the call text as the agent sent it, so the user can review what was judged and
//! export marked records as training samples; clearing the log is the only way to remove it.
//! 记录保存代理发送的原样调用文本，使用户能回看判定内容并把已标记的记录导出为训练样本；只能通过
//! 清除记录将其移除。

use std::collections::BTreeSet;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use vulcan_agent_runtime::agent_runtime::tool_permission::{
    JudgeFinding, JudgeTrace, PermissionRecord, ReviewFinding,
};
use vulcan_permission_judge::JudgeLabel;
use vulcan_service_api::{
    PermissionJudgeRecord, PermissionJudgeRecordExport, PermissionJudgeRecordJudge,
    PermissionJudgeRecordPage, PermissionJudgeRecordReview, PermissionJudgeRecordTrace,
    PermissionJudgeRecordsRequest,
};

/// Most records the log keeps; older ones are dropped when the log is compacted.
/// 记录保存的最大条数；压缩时丢弃更早的记录。
const RETENTION_RECORDS: usize = 5_000;

/// Most days the log keeps a record.
/// 记录保存的最长天数。
const RETENTION_DAYS: u32 = 30;

/// Records appended past the limit before the file is rewritten, so a full log is not rewritten on
/// every call.
/// 超出上限后、重写文件前允许追加的记录数，避免已满的记录在每次调用时都被重写。
const COMPACTION_SLACK: usize = RETENTION_RECORDS / 10;

/// Most records one page returns.
/// 一页返回的最大记录数。
const PAGE_LIMIT_MAX: u32 = 200;

/// Log file name; the version changes whenever the line format changes, and other versions are
/// not read.
/// 记录文件名；行格式变化时版本随之变化，其他版本不会被读取。
const RECORDS_FILE: &str = "records.v1.jsonl";

/// Judge result filter that keeps records whose judge could not score the call.
/// 保留判定器无法评分的记录的判定结果筛选值。
const FAILED_FILTER: &str = "failed";

/// The decision log; loaded on first use and kept in memory with the file as its authority.
/// 判定记录；首次使用时载入并保存在内存中，以文件为权威。
pub(crate) struct RecordStore {
    /// Directory of the log; `None` when the user directory is unavailable.
    /// 记录所在目录；无法取得用户目录时为 `None`。
    directory: Option<PathBuf>,
    /// Records in file order once loaded, or why the file could not be read.
    /// 载入后按文件顺序排列的记录，或文件无法读取的原因。
    loaded: Mutex<Option<Result<Vec<PermissionJudgeRecord>, String>>>,
}

impl Default for RecordStore {
    fn default() -> Self {
        Self::at(vulcan_runtime_assets::resolve_judge_records_root())
    }
}

impl RecordStore {
    /// Builds a store over `directory`.
    /// 基于 `directory` 构建存储。
    pub(crate) fn at(directory: Option<PathBuf>) -> Self {
        Self {
            directory,
            loaded: Mutex::new(None),
        }
    }

    /// Keeps one finished decision.
    /// 保存一次已完成的决定。
    pub(crate) fn append(&self, record: PermissionJudgeRecord) -> Result<(), String> {
        let path = self.path()?;
        self.with_records(|records| {
            let mut line = serde_json::to_string(&record).map_err(|error| error.to_string())?;
            line.push('\n');
            records.push(record);
            if records.len() > RETENTION_RECORDS + COMPACTION_SLACK {
                prune(records, SystemTime::now());
                return rewrite(&path, records);
            }
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent).map_err(|error| error.to_string())?;
            }
            fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
                .and_then(|mut file| file.write_all(line.as_bytes()))
                .map_err(|error| format!("failed to write `{}`: {error}", path.display()))
        })?
    }

    /// Returns one page of matching records, newest first.
    /// 返回一页匹配的记录，最新在前。
    pub(crate) fn page(
        &self,
        request: &PermissionJudgeRecordsRequest,
    ) -> Result<PermissionJudgeRecordPage, String> {
        if let Some(label) = &request.label
            && !matches!(label.as_str(), "allow" | "review" | "block" | FAILED_FILTER)
        {
            return Err(format!("Unknown judge result filter `{label}`."));
        }
        let limit = request.limit.clamp(1, PAGE_LIMIT_MAX) as usize;
        self.with_records(|records| {
            let mut matching: Vec<&PermissionJudgeRecord> = records
                .iter()
                .filter(|record| matches_request(record, request))
                .collect();
            matching.sort_by_key(|record| std::cmp::Reverse(record.judged_at_ms));
            let start = match &request.before {
                Some(before) => matching
                    .iter()
                    .position(|record| &record.id == before)
                    .map(|index| index + 1)
                    .ok_or_else(|| format!("Record `{before}` is not in the decision log."))?,
                None => 0,
            };
            let rest = &matching[start.min(matching.len())..];
            let workspaces: BTreeSet<&String> = records
                .iter()
                .filter_map(|record| record.workspace_root.as_ref())
                .collect();
            Ok(PermissionJudgeRecordPage {
                records: rest
                    .iter()
                    .take(limit)
                    .map(|record| (*record).clone())
                    .collect(),
                has_more: rest.len() > limit,
                total: count(records.len()),
                marked: count(
                    records
                        .iter()
                        .filter(|record| record.correction.is_some())
                        .count(),
                ),
                workspaces: workspaces.into_iter().cloned().collect(),
                retention_records: count(RETENTION_RECORDS),
                retention_days: RETENTION_DAYS,
            })
        })?
    }

    /// Marks the label the judge should have given a record, or clears the mark.
    /// 标记判定器本应给出的记录标签，或清除标记。
    pub(crate) fn mark(
        &self,
        id: &str,
        correction: Option<JudgeLabel>,
    ) -> Result<PermissionJudgeRecord, String> {
        let path = self.path()?;
        self.with_records(|records| {
            let record = records
                .iter_mut()
                .find(|record| record.id == id)
                .ok_or_else(|| format!("Record `{id}` is not in the decision log."))?;
            if correction.is_some()
                && record
                    .judge
                    .as_ref()
                    .and_then(|judge| judge.trace.as_ref())
                    .is_none()
            {
                return Err(format!(
                    "Record `{id}` has no judge input, so it cannot become a training sample."
                ));
            }
            record.correction = correction;
            let marked = record.clone();
            rewrite(&path, records)?;
            Ok(marked)
        })?
    }

    /// Exports marked records that carry judge input as training samples.
    /// 将带判定输入的已标记记录导出为训练样本。
    pub(crate) fn export(&self) -> Result<PermissionJudgeRecordExport, String> {
        self.with_records(|records| {
            let mut content = String::new();
            let mut exported = 0usize;
            for record in records.iter() {
                let (Some(label), Some(judge)) = (record.correction, record.judge.as_ref()) else {
                    continue;
                };
                let Some(trace) = judge.trace.as_ref() else {
                    continue;
                };
                let sample = serde_json::json!({
                    "id": record.id,
                    "policy_version": trace.policy_version,
                    "settings": trace.settings,
                    "rendered": trace.rendered,
                    "label": label.key(),
                    "metadata": {
                        "source": "vulcan_judge_record",
                        "judge_label": judge.label.map(JudgeLabel::key),
                        "backend": trace.backend,
                        "model": trace.model,
                        "outcome": record.outcome,
                        "judged_at_ms": record.judged_at_ms,
                    },
                });
                content.push_str(&sample.to_string());
                content.push('\n');
                exported += 1;
            }
            Ok(PermissionJudgeRecordExport {
                file_name: format!("judge-samples-{}.jsonl", unix_ms(SystemTime::now())),
                content,
                count: count(exported),
            })
        })?
    }

    /// Removes every record and the log file; this also recovers from an unreadable file.
    /// 删除全部记录与记录文件；也用于从无法读取的文件中恢复。
    pub(crate) fn clear(&self) -> Result<(), String> {
        let path = self.path()?;
        let mut loaded = self
            .loaded
            .lock()
            .map_err(|error| format!("failed to lock the decision log: {error}"))?;
        match fs::remove_file(&path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(format!("failed to remove `{}`: {error}", path.display())),
        }
        *loaded = Some(Ok(Vec::new()));
        Ok(())
    }

    /// Path of the log file.
    /// 记录文件路径。
    fn path(&self) -> Result<PathBuf, String> {
        self.directory
            .as_ref()
            .map(|directory| directory.join(RECORDS_FILE))
            .ok_or_else(|| {
                String::from("The user directory is unavailable, so there is no decision log.")
            })
    }

    /// Runs `work` over the loaded records, loading them on first use.
    /// 对已载入的记录运行 `work`，首次使用时载入。
    fn with_records<T>(
        &self,
        work: impl FnOnce(&mut Vec<PermissionJudgeRecord>) -> T,
    ) -> Result<T, String> {
        let path = self.path()?;
        let mut loaded = self
            .loaded
            .lock()
            .map_err(|error| format!("failed to lock the decision log: {error}"))?;
        if loaded.is_none() {
            *loaded = Some(load(&path));
        }
        match loaded.as_mut() {
            Some(Ok(records)) => Ok(work(records)),
            Some(Err(error)) => Err(error.clone()),
            None => Err(String::from("The decision log was not loaded.")),
        }
    }
}

/// Reads the log file, dropping records past retention; a line that is not a record makes the
/// whole file unreadable until the user clears it.
/// 读取记录文件并丢弃超出保留范围的记录；任一行不是记录时整个文件视为无法读取，直到用户清除。
fn load(path: &Path) -> Result<Vec<PermissionJudgeRecord>, String> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(format!("failed to read `{}`: {error}", path.display())),
    };
    let mut records = Vec::new();
    for (index, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        records.push(serde_json::from_str(line).map_err(|error| {
            format!(
                "Line {} of `{}` is not a decision record: {error}",
                index + 1,
                path.display()
            )
        })?);
    }
    prune(&mut records, SystemTime::now());
    Ok(records)
}

/// Drops records older than the retention days, then the oldest beyond the retention count.
/// 丢弃超过保留天数的记录，再丢弃超出保留条数的最早记录。
fn prune(records: &mut Vec<PermissionJudgeRecord>, now: SystemTime) {
    let horizon = now
        .checked_sub(Duration::from_secs(
            u64::from(RETENTION_DAYS) * 24 * 60 * 60,
        ))
        .map_or(0, unix_ms);
    records.retain(|record| record.judged_at_ms >= horizon);
    records.sort_by_key(|record| record.judged_at_ms);
    let excess = records.len().saturating_sub(RETENTION_RECORDS);
    records.drain(..excess);
}

/// Replaces the log file with `records` through a temporary file.
/// 经由临时文件以 `records` 替换记录文件。
fn rewrite(path: &Path, records: &[PermissionJudgeRecord]) -> Result<(), String> {
    let mut text = String::new();
    for record in records {
        text.push_str(&serde_json::to_string(record).map_err(|error| error.to_string())?);
        text.push('\n');
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let temporary = path.with_extension("jsonl.tmp");
    fs::write(&temporary, text)
        .and_then(|()| fs::rename(&temporary, path))
        .map_err(|error| format!("failed to write `{}`: {error}", path.display()))
}

/// Whether one record passes the page filters.
/// 一条记录是否通过分页筛选。
fn matches_request(
    record: &PermissionJudgeRecord,
    request: &PermissionJudgeRecordsRequest,
) -> bool {
    let label = record
        .judge
        .as_ref()
        .map(|judge| judge.label.map_or(FAILED_FILTER, JudgeLabel::key));
    request
        .label
        .as_deref()
        .is_none_or(|wanted| label == Some(wanted))
        && request
            .workspace
            .as_ref()
            .is_none_or(|workspace| record.workspace_root.as_ref() == Some(workspace))
        && (!request.marked_only || record.correction.is_some())
}

/// Converts one gate decision into a log record.
/// 将一次闸门决定转换为记录。
pub(crate) fn record_of(decision: PermissionRecord) -> PermissionJudgeRecord {
    let elapsed_ms =
        |elapsed: Option<Duration>| elapsed.map_or(0.0, |elapsed| elapsed.as_secs_f64() * 1000.0);
    PermissionJudgeRecord {
        id: uuid::Uuid::new_v4().to_string(),
        judged_at_ms: unix_ms(decision.judged_at),
        session_id: decision.session_id.to_string(),
        workspace_root: decision
            .workspace_root
            .map(|root| root.display().to_string()),
        tool_name: decision.tool_name,
        arguments: decision.arguments,
        high_risk: decision
            .high_risk
            .map(|category| category.key().to_string()),
        judge: decision.judge.map(|finding| {
            let (label, error) = match finding {
                JudgeFinding::Scored(label) => (Some(label), None),
                JudgeFinding::Unavailable(reason) => (None, Some(reason)),
            };
            PermissionJudgeRecordJudge {
                label,
                error,
                elapsed_ms: elapsed_ms(decision.judge_elapsed),
                trace: decision.judge_trace.map(trace_of),
            }
        }),
        review: decision.review.map(|finding| {
            let (verdict, reason, error) = match finding {
                ReviewFinding::Reviewed(review) => (
                    Some(review.verdict.key().to_string()),
                    Some(review.reason),
                    None,
                ),
                ReviewFinding::Unavailable(reason) => (None, None, Some(reason)),
            };
            PermissionJudgeRecordReview {
                verdict,
                reason,
                error,
                elapsed_ms: elapsed_ms(decision.review_elapsed),
            }
        }),
        outcome: decision.outcome.key().to_string(),
        correction: None,
    }
}

/// Converts one judge trace into its log form.
/// 将一次判定轨迹转换为记录形式。
fn trace_of(trace: JudgeTrace) -> PermissionJudgeRecordTrace {
    PermissionJudgeRecordTrace {
        backend: trace.backend,
        model: trace.model,
        policy_version: trace.policy_version,
        settings: trace.settings,
        rendered: trace.rendered,
        logits: trace.logits,
        allow_probability: trace.allow_probability,
        input_tokens: trace.input_tokens,
        checkpoint_sha256: trace.checkpoint_sha256,
    }
}

/// Milliseconds since the Unix epoch; times before it read as zero.
/// 自 Unix 纪元起的毫秒数；早于纪元的时间读作零。
fn unix_ms(time: SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH).map_or(0, |elapsed| {
        u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX)
    })
}

/// Narrows a count for the wire; the log never holds more than `u32::MAX` records.
/// 为线协议收窄计数；记录永远不会超过 `u32::MAX` 条。
fn count(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A record judged `age` ago with a judge trace.
    /// 一条在 `age` 之前判定且带判定轨迹的记录。
    fn record(
        id: &str,
        age: Duration,
        workspace: &str,
        label: Option<JudgeLabel>,
    ) -> PermissionJudgeRecord {
        PermissionJudgeRecord {
            id: id.to_string(),
            judged_at_ms: unix_ms(SystemTime::now() - age),
            session_id: String::from("session"),
            workspace_root: Some(workspace.to_string()),
            tool_name: String::from("shell_command"),
            arguments: String::from(r#"{"command":"cargo build"}"#),
            high_risk: None,
            judge: Some(PermissionJudgeRecordJudge {
                label,
                error: label.is_none().then(|| String::from("not loaded")),
                elapsed_ms: 4.0,
                trace: Some(PermissionJudgeRecordTrace {
                    backend: String::from("local"),
                    model: String::from("risk"),
                    policy_version: String::from("v2"),
                    settings: [(String::from("project_reset"), String::from("confirm"))].into(),
                    rendered: String::from("Tool: shell_command"),
                    logits: Some([1.0, 0.0, 0.0]),
                    allow_probability: None,
                    input_tokens: Some(9),
                    checkpoint_sha256: None,
                }),
            }),
            review: None,
            outcome: String::from("judge_allowed"),
            correction: None,
        }
    }

    fn request() -> PermissionJudgeRecordsRequest {
        PermissionJudgeRecordsRequest {
            label: None,
            workspace: None,
            marked_only: false,
            before: None,
            limit: 2,
        }
    }

    #[test]
    fn pages_newest_first_with_filters_and_survive_a_reload() {
        let directory = tempfile::tempdir().unwrap();
        let store = RecordStore::at(Some(directory.path().to_path_buf()));
        let minute = Duration::from_secs(60);
        store
            .append(record("old", minute * 3, "/w/a", Some(JudgeLabel::Allow)))
            .unwrap();
        store
            .append(record("mid", minute * 2, "/w/b", Some(JudgeLabel::Block)))
            .unwrap();
        store.append(record("new", minute, "/w/a", None)).unwrap();

        let reloaded = RecordStore::at(Some(directory.path().to_path_buf()));
        let first = reloaded.page(&request()).unwrap();
        let ids: Vec<_> = first
            .records
            .iter()
            .map(|record| record.id.as_str())
            .collect();
        assert_eq!(ids, ["new", "mid"]);
        assert!(first.has_more);
        assert_eq!((first.total, first.workspaces.len()), (3, 2));
        let next = reloaded
            .page(&PermissionJudgeRecordsRequest {
                before: Some(String::from("mid")),
                ..request()
            })
            .unwrap();
        assert_eq!(next.records[0].id, "old");
        assert!(!next.has_more);

        let failed = reloaded
            .page(&PermissionJudgeRecordsRequest {
                label: Some(String::from("failed")),
                ..request()
            })
            .unwrap();
        assert_eq!(failed.records.len(), 1);
        let workspace = reloaded
            .page(&PermissionJudgeRecordsRequest {
                workspace: Some(String::from("/w/b")),
                ..request()
            })
            .unwrap();
        assert_eq!(workspace.records[0].id, "mid");
        assert!(
            reloaded
                .page(&PermissionJudgeRecordsRequest {
                    label: Some(String::from("maybe")),
                    ..request()
                })
                .is_err()
        );
    }

    #[test]
    fn exports_only_marked_records_as_samples() {
        let directory = tempfile::tempdir().unwrap();
        let store = RecordStore::at(Some(directory.path().to_path_buf()));
        store
            .append(record(
                "a",
                Duration::from_secs(5),
                "/w/a",
                Some(JudgeLabel::Allow),
            ))
            .unwrap();
        store
            .append(record(
                "b",
                Duration::from_secs(4),
                "/w/a",
                Some(JudgeLabel::Allow),
            ))
            .unwrap();
        store.mark("b", Some(JudgeLabel::Block)).unwrap();

        let reloaded = RecordStore::at(Some(directory.path().to_path_buf()));
        let export = reloaded.export().unwrap();
        assert_eq!(export.count, 1);
        let sample: serde_json::Value = serde_json::from_str(export.content.trim()).unwrap();
        assert_eq!(sample["label"], "block");
        assert_eq!(sample["metadata"]["judge_label"], "allow");
        assert!(sample["settings"].is_object() && sample["rendered"].is_string());
        let marked = reloaded
            .page(&PermissionJudgeRecordsRequest {
                marked_only: true,
                ..request()
            })
            .unwrap();
        assert_eq!((marked.records.len(), marked.marked), (1, 1));
    }

    #[test]
    fn drops_records_past_retention_and_rejects_unreadable_files_until_cleared() {
        let mut records = vec![
            record(
                "expired",
                Duration::from_secs(u64::from(RETENTION_DAYS + 1) * 86_400),
                "/w",
                None,
            ),
            record("kept", Duration::from_secs(1), "/w", None),
        ];
        prune(&mut records, SystemTime::now());
        assert_eq!(records.len(), 1);
        assert_eq!(records[0].id, "kept");

        let directory = tempfile::tempdir().unwrap();
        fs::write(directory.path().join(RECORDS_FILE), "not a record\n").unwrap();
        let store = RecordStore::at(Some(directory.path().to_path_buf()));
        assert!(store.page(&request()).is_err());
        assert!(
            store
                .append(record("a", Duration::from_secs(1), "/w", None))
                .is_err()
        );
        store.clear().unwrap();
        assert_eq!(store.page(&request()).unwrap().total, 0);
    }
}
