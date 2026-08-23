use super::*;
use serde_json::json;
use std::collections::BTreeMap;
use vulcan_context::{SessionMessageKind, conversation_state_from_session_messages};
use vulcan_session_core::SessionNodeStore;

mod metadata;
mod navigation_store;
mod panel_state;

/// Creates one indexed segmented TaskSession test store with an exact root metadata document.
/// 创建一个带精确根元数据文档索引的分段 TaskSession 测试存储。
///
/// # Parameters
/// 参数
/// `root` is the isolated main-session directory and `root_session_id` is its canonical identity.
/// `root` 是隔离的主会话目录，`root_session_id` 是其规范身份。
///
/// # Returns
/// 返回值
/// A store whose child discovery uses the root metadata mirror instead of directory traversal.
/// 返回通过根元数据镜像发现子任务而不遍历目录的存储。
fn indexed_segmented_task_session_test_store(
    root: &std::path::Path,
    root_session_id: vulcan_session_core::SessionNodeId,
) -> vulcan_task_session::SegmentedTaskSessionStore {
    // rootLine is the sole initial event indexed by the root metadata document.
    // rootLine 是根元数据文档索引的唯一初始事件。
    let root_line = serde_json::json!({
        "seq": 0,
        "timestamp": "100",
        "kind": "session_meta",
        "payload": { "session_id": root_session_id },
        "turn_index": null
    });
    // rootLineBytes retain the exact newline-inclusive length recorded by segment metadata.
    // rootLineBytes 保留分段元数据记录的包含换行符的精确长度。
    let mut root_line_bytes =
        serde_json::to_vec(&root_line).expect("encode root session event line");
    root_line_bytes.push(b'\n');
    std::fs::write(root.join("s001.jsonl"), &root_line_bytes)
        .expect("write root session event segment");
    // metadata is the authoritative direct index consumed by TaskSession list projections.
    // metadata 是 TaskSession 列表投影消费的权威直接索引。
    let metadata = serde_json::json!({
        "schema_version": 1,
        "session_id": root_session_id.to_string(),
        "project_tag": "workbench-model-test",
        "created_at": "100",
        "updated_at": "100",
        "project_id": "workbench-model-test",
        "title": "Root",
        "title_source": "manual",
        "canonical_source": "segmented_test",
        "archived_at": null,
        "active_segment": "s001.jsonl",
        "task_dir": "task",
        "next_global_seq": 1,
        "latest_rollback_seq": null,
        "segments": [{
            "file": "s001.jsonl",
            "start_seq": 0,
            "line_count": 1,
            "byte_len": root_line_bytes.len(),
            "first_timestamp": "100",
            "last_timestamp": "100",
            "first_turn_index": null,
            "last_turn_index": null,
            "last_compaction_seq": null
        }],
        "segment_policy": {
            "max_lines": 3000,
            "max_bytes": 8388608
        },
        "tasks": []
    });
    std::fs::write(
        root.join("metadata.json"),
        serde_json::to_vec(&metadata).expect("encode root session metadata"),
    )
    .expect("write root session metadata");
    vulcan_task_session::SegmentedTaskSessionStore::new(root)
}

/// Builds one compact test panel state with the requested lifecycle state.
/// 构建一条带指定生命周期状态的紧凑测试面板状态。
fn test_subagent_state(
    parent_session_id: SessionId,
    task_id: SessionId,
    run_state: SubagentTaskRunState,
) -> SubagentTaskPanelState {
    SubagentTaskPanelState {
        task_id,
        public_task_id: task_id.to_string(),
        parent_session_id,
        subagent_label: String::from("worker"),
        description: None,
        run_state,
        result_text: String::new(),
        conversation_state: None,
    }
}
