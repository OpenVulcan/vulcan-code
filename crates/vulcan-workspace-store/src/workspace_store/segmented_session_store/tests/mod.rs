//! Segmented session store tests.
//! 分段会话存储测试。

use super::*;
use crate::model::SessionRuntimeStateEntry;
use crate::provider_config::AiToolKind;
use uuid::Uuid;

/// Builds a unique temporary directory for one segmented-store test.
/// 为单个分段存储测试构建唯一临时目录。
fn test_root(test_name: &str) -> PathBuf {
    // Use a UUID so parallel test runs do not collide in the system temp directory.
    // 使用 UUID，避免并行测试在系统临时目录中冲突。
    let root = std::env::temp_dir()
        .join("vulcan-segmented-session-store-tests")
        .join(test_name)
        .join(Uuid::new_v4().to_string());
    fs::create_dir_all(&root).expect("create temp test root");
    root
}

/// Reads every segment file as raw text for mutation-safety assertions.
/// 以原始文本读取全部分段文件，用于断言失败路径不会改写磁盘。
fn read_segment_texts_for_test(session_dir: &Path) -> BTreeMap<String, String> {
    let metadata: SegmentedSessionMetadata =
        read_required_json(&session_dir.join(SEGMENTED_METADATA_FILE)).expect("read test metadata");
    metadata
        .segments
        .iter()
        .map(|segment| {
            let text = fs::read_to_string(session_dir.join(&segment.file))
                .expect("read test segment text");
            (segment.file.clone(), text)
        })
        .collect()
}

/// Builds a minimal main session creation request.
/// 构建最小主会话创建请求。
fn test_session_request(policy: SegmentPolicy) -> SegmentedSessionCreateRequest {
    SegmentedSessionCreateRequest {
        session_id: "session-alpha".to_string(),
        project_tag: "Project Alpha".to_string(),
        created_date: "2026-06-06".to_string(),
        created_at: "2026-06-06T10:00:00Z".to_string(),
        project_id: Some("project-alpha".to_string()),
        title: Some("Alpha".to_string()),
        title_source: Some("user".to_string()),
        segment_policy: policy,
    }
}

/// Builds a simple append input.
/// 构建简单追加输入。
fn test_event(timestamp: &str, kind: SegmentedSessionEventKind) -> SegmentedSessionEventInput {
    SegmentedSessionEventInput {
        timestamp: timestamp.to_string(),
        kind,
        payload: json!({ "body": timestamp }),
        turn_index: Some(1),
    }
}

/// Builds a transcript message with stable runtime metadata for append-tail tests.
/// 为尾部追加测试构建带稳定运行时元数据的转录消息。
fn test_message(
    id: &str,
    kind: SessionMessageKind,
    record_kind: SessionMessageRecordKind,
    body: &str,
) -> SessionMessageEntry {
    let mut message = SessionMessageEntry::new(kind, "Title", body)
        .with_runtime_metadata(id, format!("2026-06-06T10:00:0{}Z", id.len()));
    message.record_kind = record_kind;
    message
}

/// Builds a reusable task-session creation request for link-update tests.
/// 为关联更新测试构建可复用的子任务会话创建请求。
fn test_task_session_request() -> SegmentedTaskSessionCreateRequest {
    SegmentedTaskSessionCreateRequest {
        task_session_id: "task-alpha".to_string(),
        parent_start_seq: Some(1),
        agent_kind: SubagentAgentKind::MainClone,
        agent_profile: SubagentAgentProfile {
            agent_id: None,
            agent_name: Some("Main clone".to_string()),
            agent_md_path: None,
            prompt_source: Some("main_session".to_string()),
        },
        spawned_by: SubagentSpawnedBy {
            parent_turn_id: Some("turn-1".to_string()),
            parent_event_seq: Some(1),
            tool_call_id: Some("tool-1".to_string()),
        },
        created_at: "2026-06-06T10:03:00Z".to_string(),
        status: "running".to_string(),
        segment_policy: SegmentPolicy::default(),
    }
}

mod append_events;
mod metadata_archive;
mod reverse_replay;
mod rollback_updates;
mod runtime_rebuild;
mod runtime_sync;
mod session_lifecycle;
mod task_sessions;
mod window_and_state;
