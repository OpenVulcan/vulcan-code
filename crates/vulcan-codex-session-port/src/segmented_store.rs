use crate::contract::{CodexSessionEvent, CodexThreadId};
use crate::storage::{CodexSessionStateStore, CodexSessionStore, CodexStorageWrite};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// Options controlling one segmented Codex-compatible session directory.
/// 控制单个分段式 Codex 兼容会话目录的选项。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SegmentedCodexSessionStoreOptions {
    /// Maximum event rows stored in one `sXXX.jsonl` segment before rollover.
    /// 每个 `sXXX.jsonl` 分段在滚动前允许保存的最大事件行数。
    pub max_segment_lines: u64,
}

impl Default for SegmentedCodexSessionStoreOptions {
    /// Creates default segmented store options.
    /// 创建默认分段存储选项。
    fn default() -> Self {
        Self {
            max_segment_lines: 3000,
        }
    }
}

/// Directory-backed append-only event and overwrite-only state store.
/// 基于目录的追加式事件与覆盖式状态存储。
#[derive(Debug, Clone)]
pub struct SegmentedCodexSessionStore {
    /// Root directory for one main thread or one child-agent thread.
    /// 单个主线程或子代理线程的根目录。
    root: PathBuf,
    /// Store options used by this directory.
    /// 该目录使用的存储选项。
    options: SegmentedCodexSessionStoreOptions,
    /// In-process write lock that keeps metadata, state, and JSONL updates ordered.
    /// 保持 metadata、state 与 JSONL 更新有序的进程内写锁。
    write_lock: Arc<Mutex<()>>,
}

impl SegmentedCodexSessionStore {
    /// Creates a segmented store rooted at one session directory.
    /// 创建一个以单个会话目录为根的分段存储。
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self::with_options(root, SegmentedCodexSessionStoreOptions::default())
    }

    /// Creates a segmented store with explicit options.
    /// 使用显式选项创建分段存储。
    pub fn with_options(
        root: impl Into<PathBuf>,
        options: SegmentedCodexSessionStoreOptions,
    ) -> Self {
        let options = if options.max_segment_lines == 0 {
            SegmentedCodexSessionStoreOptions::default()
        } else {
            options
        };
        Self {
            root: root.into(),
            options,
            write_lock: Arc::new(Mutex::new(())),
        }
    }

    /// Returns the root directory used by this store.
    /// 返回该存储使用的根目录。
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Creates a child-agent store under the flat `task/<task_session_id>/` directory.
    /// 在扁平 `task/<task_session_id>/` 目录下创建子代理存储。
    pub fn child_task_store(&self, task_session_id: impl AsRef<str>) -> Self {
        Self::with_options(
            self.root.join("task").join(task_session_id.as_ref()),
            self.options,
        )
    }

    /// Returns the metadata document path.
    /// 返回 metadata 文档路径。
    fn metadata_path(&self) -> PathBuf {
        self.root.join("metadata.json")
    }

    /// Returns the overwrite-only state document path.
    /// 返回覆盖式 state 文档路径。
    fn state_path(&self) -> PathBuf {
        self.root.join("state.json")
    }

    /// Returns the path for one numbered JSONL segment.
    /// 返回单个编号 JSONL 分段路径。
    fn segment_path(&self, index: u32) -> PathBuf {
        self.root.join(segment_file_name(index))
    }

    /// Loads metadata or creates a fresh metadata document for a new thread.
    /// 读取 metadata，或为新线程创建全新 metadata 文档。
    fn load_or_create_metadata(
        &self,
        thread_id: CodexThreadId,
    ) -> Result<SegmentedCodexSessionMetadata, String> {
        let path = self.metadata_path();
        if path.exists() {
            let content = fs::read_to_string(&path)
                .map_err(|error| format!("failed to read segmented metadata: {error}"))?;
            let metadata: SegmentedCodexSessionMetadata = serde_json::from_str(&content)
                .map_err(|error| format!("invalid segmented metadata: {error}"))?;
            if metadata.thread_id != thread_id {
                return Err(format!(
                    "segmented metadata thread mismatch: expected {thread_id}, found {}",
                    metadata.thread_id
                ));
            }
            return Ok(metadata);
        }
        Ok(SegmentedCodexSessionMetadata::new(thread_id))
    }

    /// Saves metadata as the authoritative segment map.
    /// 将 metadata 保存为权威分段映射。
    fn save_metadata(&self, metadata: &SegmentedCodexSessionMetadata) -> Result<(), String> {
        fs::create_dir_all(&self.root)
            .map_err(|error| format!("failed to create segmented session directory: {error}"))?;
        let json = serde_json::to_vec_pretty(metadata)
            .map_err(|error| format!("failed to serialize segmented metadata: {error}"))?;
        fs::write(self.metadata_path(), json)
            .map_err(|error| format!("failed to write segmented metadata: {error}"))
    }
}

impl CodexSessionStore for SegmentedCodexSessionStore {
    /// Appends one event to the active JSONL segment without rewriting previous rows.
    /// 将单个事件追加到活动 JSONL 分段且不重写旧行。
    fn append_event(&self, write: CodexStorageWrite) -> Result<(), String> {
        let _guard = self
            .write_lock
            .lock()
            .map_err(|error| format!("failed to lock segmented store: {error}"))?;
        fs::create_dir_all(&self.root)
            .map_err(|error| format!("failed to create segmented session directory: {error}"))?;
        let mut metadata = self.load_or_create_metadata(write.thread_id)?;
        if metadata.active_segment_line_count >= self.options.max_segment_lines {
            metadata.rollover();
        }
        let line = SegmentedCodexEventLine {
            line_type: String::from("event"),
            event: write.event,
        };
        let encoded = serde_json::to_string(&line)
            .map_err(|error| format!("failed to serialize segmented event line: {error}"))?;
        let segment_path = self.segment_path(metadata.active_segment_index);
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&segment_path)
            .map_err(|error| format!("failed to open segmented event segment: {error}"))?;
        writeln!(file, "{encoded}")
            .map_err(|error| format!("failed to append segmented event line: {error}"))?;
        metadata.record_appended_event(line.event.sequence);
        self.save_metadata(&metadata)
    }

    /// Reads all events for one thread in sequence order.
    /// 按序读取单个线程的所有事件。
    fn read_events(&self, thread_id: CodexThreadId) -> Result<Vec<CodexSessionEvent>, String> {
        let metadata = self.load_or_create_metadata(thread_id)?;
        let mut events = Vec::new();
        for segment in &metadata.segments {
            let path = self.root.join(&segment.file);
            if !path.exists() {
                continue;
            }
            let content = fs::read_to_string(&path)
                .map_err(|error| format!("failed to read segmented event segment: {error}"))?;
            for (line_index, line) in content.lines().enumerate() {
                if line.trim().is_empty() {
                    continue;
                }
                let event_line: SegmentedCodexEventLine =
                    serde_json::from_str(line).map_err(|error| {
                        format!(
                            "invalid segmented event line {} in {}: {error}",
                            line_index + 1,
                            segment.file
                        )
                    })?;
                if event_line.line_type != "event" {
                    return Err(format!(
                        "invalid segmented event line type {} in {}",
                        event_line.line_type, segment.file
                    ));
                }
                if event_line.event.thread_id == thread_id {
                    events.push(event_line.event);
                }
            }
        }
        events.sort_by_key(|event| event.sequence);
        Ok(events)
    }
}

impl CodexSessionStateStore for SegmentedCodexSessionStore {
    /// Replaces `state.json` without touching event segments.
    /// 覆盖 `state.json` 且不触碰事件分段。
    fn replace_state(&self, thread_id: CodexThreadId, state: Value) -> Result<(), String> {
        let _guard = self
            .write_lock
            .lock()
            .map_err(|error| format!("failed to lock segmented store: {error}"))?;
        fs::create_dir_all(&self.root)
            .map_err(|error| format!("failed to create segmented session directory: {error}"))?;
        let mut metadata = self.load_or_create_metadata(thread_id)?;
        metadata.state_file = String::from("state.json");
        self.save_metadata(&metadata)?;
        let document = SegmentedCodexStateDocument {
            document_type: String::from("state"),
            thread_id,
            state,
        };
        let json = serde_json::to_vec_pretty(&document)
            .map_err(|error| format!("failed to serialize segmented state: {error}"))?;
        fs::write(self.state_path(), json)
            .map_err(|error| format!("failed to write segmented state: {error}"))
    }

    /// Loads the latest `state.json` document when it exists.
    /// 在存在时读取最新 `state.json` 文档。
    fn load_state(&self, thread_id: CodexThreadId) -> Result<Option<Value>, String> {
        let path = self.state_path();
        if !path.exists() {
            return Ok(None);
        }
        let content = fs::read_to_string(&path)
            .map_err(|error| format!("failed to read segmented state: {error}"))?;
        let document: SegmentedCodexStateDocument = serde_json::from_str(&content)
            .map_err(|error| format!("invalid segmented state: {error}"))?;
        if document.document_type != "state" {
            return Err(format!(
                "invalid segmented state document type {}",
                document.document_type
            ));
        }
        if document.thread_id != thread_id {
            return Err(format!(
                "segmented state thread mismatch: expected {thread_id}, found {}",
                document.thread_id
            ));
        }
        Ok(Some(document.state))
    }
}

/// Metadata stored beside segmented JSONL event files.
/// 与分段 JSONL 事件文件并列保存的 metadata。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct SegmentedCodexSessionMetadata {
    /// Fixed document type discriminator.
    /// 固定文档类型判别字段。
    #[serde(rename = "type")]
    document_type: String,
    /// Owning thread identifier.
    /// 所属线程标识符。
    thread_id: CodexThreadId,
    /// Active segment index.
    /// 活动分段序号。
    active_segment_index: u32,
    /// Active segment file name.
    /// 活动分段文件名。
    active_segment: String,
    /// Event row count in the active segment.
    /// 活动分段内的事件行数。
    active_segment_line_count: u64,
    /// Next expected sequence according to persisted events.
    /// 根据已持久化事件得到的下一条预期序号。
    next_sequence: u64,
    /// Overwrite-only state file name.
    /// 覆盖式状态文件名。
    state_file: String,
    /// Ordered segment descriptors.
    /// 有序分段描述。
    segments: Vec<SegmentedCodexSegmentMetadata>,
}

impl SegmentedCodexSessionMetadata {
    /// Creates metadata for a fresh session directory.
    /// 为全新会话目录创建 metadata。
    fn new(thread_id: CodexThreadId) -> Self {
        let first_segment = SegmentedCodexSegmentMetadata::new(1);
        Self {
            document_type: String::from("codex_session_metadata"),
            thread_id,
            active_segment_index: first_segment.index,
            active_segment: first_segment.file.clone(),
            active_segment_line_count: 0,
            next_sequence: 1,
            state_file: String::from("state.json"),
            segments: vec![first_segment],
        }
    }

    /// Rolls the active segment to the next numbered JSONL file.
    /// 将活动分段滚动到下一个编号 JSONL 文件。
    fn rollover(&mut self) {
        let next_index = self.active_segment_index.saturating_add(1);
        let next_segment = SegmentedCodexSegmentMetadata::new(next_index);
        self.active_segment_index = next_segment.index;
        self.active_segment = next_segment.file.clone();
        self.active_segment_line_count = 0;
        self.segments.push(next_segment);
    }

    /// Records a newly appended event in metadata.
    /// 在 metadata 中记录一条新追加事件。
    fn record_appended_event(&mut self, sequence: u64) {
        self.active_segment_line_count = self.active_segment_line_count.saturating_add(1);
        self.next_sequence = self.next_sequence.max(sequence.saturating_add(1));
        if let Some(segment) = self
            .segments
            .iter_mut()
            .find(|segment| segment.index == self.active_segment_index)
        {
            segment.line_count = segment.line_count.saturating_add(1);
        }
    }
}

/// One JSONL segment descriptor stored in metadata.
/// metadata 中保存的一条 JSONL 分段描述。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct SegmentedCodexSegmentMetadata {
    /// Segment index starting at one.
    /// 从一开始的分段序号。
    index: u32,
    /// Segment file name.
    /// 分段文件名。
    file: String,
    /// Number of event lines stored in this segment.
    /// 该分段中保存的事件行数。
    line_count: u64,
}

impl SegmentedCodexSegmentMetadata {
    /// Creates one empty segment descriptor.
    /// 创建一条空分段描述。
    fn new(index: u32) -> Self {
        Self {
            index,
            file: segment_file_name(index),
            line_count: 0,
        }
    }
}

/// JSONL row wrapper for a Codex event.
/// Codex 事件使用的 JSONL 行包装。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct SegmentedCodexEventLine {
    /// Fixed row type discriminator.
    /// 固定行类型判别字段。
    #[serde(rename = "type")]
    line_type: String,
    /// Persisted event payload.
    /// 已持久化事件载荷。
    event: CodexSessionEvent,
}

/// State file wrapper that prevents confusing state documents with event rows.
/// 防止将状态文档与事件行混淆的 state 文件包装。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct SegmentedCodexStateDocument {
    /// Fixed document type discriminator.
    /// 固定文档类型判别字段。
    #[serde(rename = "type")]
    document_type: String,
    /// Owning thread identifier.
    /// 所属线程标识符。
    thread_id: CodexThreadId,
    /// Overwrite-only state payload.
    /// 覆盖式状态载荷。
    state: Value,
}

/// Builds the stable file name for one numbered segment.
/// 构建单个编号分段的稳定文件名。
fn segment_file_name(index: u32) -> String {
    format!("s{index:03}.jsonl")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::{CodexEventScope, CodexEventVisibility, CodexSessionEventKind};
    use serde_json::json;
    use std::time::{SystemTime, UNIX_EPOCH};

    /// Creates a unique temporary directory for store tests.
    /// 为存储测试创建唯一临时目录。
    fn temp_test_dir(name: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_nanos();
        path.push(format!("vulcan-codex-session-port-{name}-{nanos}"));
        fs::create_dir_all(&path).expect("temp directory should be created");
        path
    }

    /// Creates a visible test event.
    /// 创建一条可见测试事件。
    fn event(thread_id: CodexThreadId, sequence: u64) -> CodexSessionEvent {
        CodexSessionEvent::new(
            format!("event-{sequence}"),
            thread_id,
            sequence,
            CodexSessionEventKind::UserInput,
            CodexEventScope::Turn,
            CodexEventVisibility::Visible,
            1000 + sequence,
        )
        .with_payload(json!({ "text": format!("prompt {sequence}") }))
    }

    /// Verifies event appends roll over segments without rewriting older files.
    /// 验证事件追加会滚动分段且不重写旧文件。
    #[test]
    fn segmented_store_appends_events_and_rolls_segments() {
        let root = temp_test_dir("rolls");
        let thread_id = CodexThreadId::new_v4();
        let store = SegmentedCodexSessionStore::with_options(
            &root,
            SegmentedCodexSessionStoreOptions {
                max_segment_lines: 2,
            },
        );

        for sequence in 1..=3 {
            store
                .append_event(CodexStorageWrite {
                    thread_id,
                    event: event(thread_id, sequence),
                })
                .expect("event should append");
        }

        let metadata: SegmentedCodexSessionMetadata = serde_json::from_str(
            &fs::read_to_string(root.join("metadata.json")).expect("metadata should read"),
        )
        .expect("metadata should deserialize");
        let first_segment =
            fs::read_to_string(root.join("s001.jsonl")).expect("first segment should read");
        let second_segment =
            fs::read_to_string(root.join("s002.jsonl")).expect("second segment should read");
        let events = store.read_events(thread_id).expect("events should read");

        assert_eq!(metadata.active_segment, "s002.jsonl");
        assert_eq!(metadata.active_segment_line_count, 1);
        assert_eq!(metadata.segments[0].line_count, 2);
        assert_eq!(metadata.segments[1].line_count, 1);
        assert_eq!(metadata.next_sequence, 4);
        assert!(first_segment.contains("\"type\":\"event\""));
        assert!(second_segment.contains("\"type\":\"event\""));
        assert_eq!(
            events
                .iter()
                .map(|event| event.sequence)
                .collect::<Vec<_>>(),
            vec![1, 2, 3]
        );

        let _ = fs::remove_dir_all(&root);
    }

    /// Verifies state replacement never appends extra JSONL event rows.
    /// 验证状态覆盖不会额外追加 JSONL 事件行。
    #[test]
    fn segmented_store_replaces_state_without_appending_events() {
        let root = temp_test_dir("state");
        let thread_id = CodexThreadId::new_v4();
        let store = SegmentedCodexSessionStore::new(&root);
        store
            .append_event(CodexStorageWrite {
                thread_id,
                event: event(thread_id, 1),
            })
            .expect("event should append");

        store
            .replace_state(thread_id, json!({ "status": "running" }))
            .expect("state should write");
        store
            .replace_state(thread_id, json!({ "status": "idle" }))
            .expect("state should replace");

        let segment_lines = fs::read_to_string(root.join("s001.jsonl"))
            .expect("segment should read")
            .lines()
            .count();
        let state = store
            .load_state(thread_id)
            .expect("state should load")
            .expect("state should exist");
        let events = store.read_events(thread_id).expect("events should read");

        assert_eq!(segment_lines, 1);
        assert_eq!(state, json!({ "status": "idle" }));
        assert_eq!(events.len(), 1);

        let _ = fs::remove_dir_all(&root);
    }

    /// Verifies child-agent stores use flat task directories.
    /// 验证子代理存储使用扁平 task 目录。
    #[test]
    fn child_task_store_uses_flat_task_directory() {
        let root = temp_test_dir("task");
        let store = SegmentedCodexSessionStore::new(&root);
        let child = store.child_task_store("child-session-1");

        assert_eq!(child.root(), root.join("task").join("child-session-1"));

        let _ = fs::remove_dir_all(&root);
    }
}
