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

/// Builds the legacy task agent profile used by persisted fixture directories.
/// 构建持久化夹具目录使用的遗留任务代理画像。
fn test_task_agent_profile() -> SubagentAgentProfile {
    SubagentAgentProfile {
        agent_id: None,
        agent_name: Some("Main clone".to_string()),
        agent_md_path: None,
        prompt_source: Some("main_session".to_string()),
    }
}

/// Builds the legacy parent spawn metadata used by persisted fixture directories.
/// 构建持久化夹具目录使用的遗留父级派生元数据。
fn test_task_spawned_by() -> SubagentSpawnedBy {
    SubagentSpawnedBy {
        parent_turn_id: Some("turn-1".to_string()),
        parent_event_seq: Some(1),
        tool_call_id: Some("tool-1".to_string()),
    }
}

/// Writes a legacy task-session directory for tests that exercise existing-disk loading.
/// 为验证既有磁盘加载的测试写入一个遗留子任务会话目录。
fn write_legacy_task_session_fixture(parent_session_dir: &Path, task_session_id: &str) -> PathBuf {
    // Use one stable timestamp so metadata, task state, and parent link stay comparable.
    // 使用一个稳定时间戳，确保元数据、任务状态与父关联事件可比较。
    let created_at = "2026-06-06T10:03:00Z";
    // Load the parent once before writing the child payload because legacy task meta stores parent ownership.
    // 写入子任务载荷前先加载父元数据，因为遗留任务元事件保存父级归属。
    let parent_metadata =
        read_session_metadata(parent_session_dir).expect("read parent metadata for task fixture");
    // Reuse the same agent fields in the child metadata, parent link, and parent mirror.
    // 在子元数据、父关联事件与父镜像中复用同一组代理字段。
    let agent_kind = SubagentAgentKind::MainClone;
    let agent_profile = test_task_agent_profile();
    let spawned_by = test_task_spawned_by();
    // Keep the fixture path identical to the production singular task directory layout.
    // 保持夹具路径与生产单数 task 目录布局一致。
    let sanitized_task_id = sanitize_file_component(task_session_id);
    let task_relative_dir = format!("{SEGMENTED_TASK_DIR}/{sanitized_task_id}");
    let task_dir = parent_session_dir.join(&task_relative_dir);
    create_dir_all(&task_dir).expect("create legacy task fixture directory");
    // The legacy task starts with one canonical SubagentTaskMeta event in s001.
    // 遗留任务以 s001 中的一条规范 SubagentTaskMeta 事件开头。
    let segment_file = segment_file_name(1);
    let task_line = SegmentedSessionEventLine {
        seq: 0,
        timestamp: created_at.to_string(),
        kind: SegmentedSessionEventKind::SubagentTaskMeta,
        payload: json!({
            "task_session_id": task_session_id,
            "parent_session_id": parent_metadata.session_id.clone(),
            "parent_project_tag": parent_metadata.project_tag.clone(),
            "agent_kind": agent_kind.clone(),
            "agent_profile": agent_profile.clone(),
            "spawned_by": spawned_by.clone(),
            "created_at": created_at
        }),
    };
    // Compute segment metadata from the exact line that is written to disk.
    // 根据实际写入磁盘的行计算分段元数据。
    let task_line_byte_len = serde_json::to_string(&task_line)
        .expect("serialize legacy task line")
        .len() as u64
        + 1;
    write_json_lines_file_atomic(&task_dir.join(&segment_file), &[task_line])
        .expect("write legacy task segment");
    // Store one non-empty task segment so readers and fork rewriting follow the real metadata path.
    // 保存一个非空任务分段，使读取器与 fork 改写走真实元数据路径。
    let mut task_segment = SegmentMetadata::empty(segment_file.clone(), 0);
    task_segment.line_count = 1;
    task_segment.byte_len = task_line_byte_len;
    task_segment.first_timestamp = Some(created_at.to_string());
    task_segment.last_timestamp = Some(created_at.to_string());
    // Write only the persisted metadata shape that readers and fork rewriting consume.
    // 只写入读取器与 fork 改写会消费的持久化元数据形态。
    let task_metadata = SegmentedTaskSessionMetadata {
        schema_version: 1,
        task_session_id: task_session_id.to_string(),
        parent_session_id: parent_metadata.session_id.clone(),
        parent_project_tag: parent_metadata.project_tag.clone(),
        created_at: created_at.to_string(),
        updated_at: created_at.to_string(),
        status: "running".to_string(),
        agent_kind: agent_kind.clone(),
        agent_profile: agent_profile.clone(),
        spawned_by: spawned_by.clone(),
        active_segment: segment_file.clone(),
        next_local_seq: 1,
        segment_policy: SegmentPolicy::default(),
        segments: vec![task_segment],
    };
    write_task_metadata(&task_dir, &task_metadata).expect("write legacy task metadata");
    write_state(
        &task_dir,
        &SegmentedSessionStateDocument {
            schema_version: 1,
            session_id: task_session_id.to_string(),
            updated_at: created_at.to_string(),
            runtime_state: Value::Null,
            current_context: Value::Null,
            session_boundary_state: Value::Null,
        },
    )
    .expect("write legacy task state");
    // Append the parent start event through the current parent append path.
    // 通过当前父会话追加路径写入父级启动事件。
    let parent_link_lines = append_segmented_session_events(
        parent_session_dir,
        &[SegmentedSessionEventInput {
            timestamp: created_at.to_string(),
            kind: SegmentedSessionEventKind::SubagentTaskStarted,
            payload: json!({
                "task_session_id": task_session_id,
                "task_dir": task_relative_dir,
                "task_active_segment": format!("{task_relative_dir}/{segment_file}"),
                "task_metadata_file": format!("{task_relative_dir}/{SEGMENTED_METADATA_FILE}"),
                "agent_kind": agent_kind.clone(),
                "agent_profile": agent_profile.clone(),
                "parent_event_seq": 1,
                "status": "running"
            }),
            turn_index: None,
        }],
    )
    .expect("append legacy parent task start event");
    // Mirror the legacy task into parent metadata because task-mirror loading reads this index.
    // 将遗留任务镜像到父元数据，因为任务镜像加载读取该索引。
    let parent_start_seq = parent_link_lines.first().map(|line| line.seq);
    let mut updated_parent_metadata =
        read_session_metadata(parent_session_dir).expect("reload parent metadata for task mirror");
    updated_parent_metadata.tasks.push(SubagentTaskMirror {
        session_id: task_session_id.to_string(),
        dir: task_relative_dir.clone(),
        active_segment: format!("{task_relative_dir}/{segment_file}"),
        metadata_file: format!("{task_relative_dir}/{SEGMENTED_METADATA_FILE}"),
        agent_kind,
        agent_profile,
        parent_start_seq,
        parent_end_seq: None,
        status: "running".to_string(),
        line_count: 1,
        created_at: created_at.to_string(),
        updated_at: created_at.to_string(),
    });
    write_metadata(parent_session_dir, &updated_parent_metadata)
        .expect("write parent metadata task mirror");
    task_dir
}

mod append_events;
mod atomic_io;
mod metadata_archive;
mod reverse_replay;
mod rollback_updates;
mod runtime_rebuild;
mod runtime_sync;
mod session_lifecycle;
mod window_and_state;
