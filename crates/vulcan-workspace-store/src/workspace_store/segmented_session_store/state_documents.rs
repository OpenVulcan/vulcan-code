//! Segmented session state document persistence.
//! 分段会话状态文档持久化。

use super::*;
use vulcan_context::protocol::{
    validate_session_conversation_message_identities,
    validate_session_conversation_record_identities,
};

/// Overwrites the state document for one main session or task session directory.
/// 覆盖写入某个主会话或子任务会话目录的状态文档。
pub(in crate::workspace_store) fn write_state(
    session_dir: &Path,
    state: &SegmentedSessionStateDocument,
) -> Result<(), String> {
    let _guard = segmented_state_file_io_lock()
        .lock()
        .map_err(|error| format!("锁定分段 state.json 失败：{error}"))?;
    let mut next_state = state.clone();
    if next_state.session_boundary_state.is_null()
        && let Some(existing_state) = read_state_unlocked(session_dir)?
        && !existing_state.session_boundary_state.is_null()
    {
        next_state.session_boundary_state = existing_state.session_boundary_state;
    }
    write_state_unlocked(session_dir, &next_state)
}

/// Reads the overwrite-only state document for one segmented session directory.
/// 读取某个分段会话目录的覆盖式状态文档。
pub(in crate::workspace_store) fn read_state(
    session_dir: &Path,
) -> Result<Option<SegmentedSessionStateDocument>, String> {
    let _guard = segmented_state_file_io_lock()
        .lock()
        .map_err(|error| format!("锁定分段 state.json 失败：{error}"))?;
    read_state_unlocked(session_dir)
}

/// Reads state.json while the caller already owns the state IO lock.
/// 在调用方已持有状态 IO 锁时读取 state.json。
fn read_state_unlocked(
    session_dir: &Path,
) -> Result<Option<SegmentedSessionStateDocument>, String> {
    let path = state_path(session_dir);
    if !path.exists() {
        return Ok(None);
    }
    read_required_json(&path).map(Some)
}

/// Writes state.json while the caller already owns the state IO lock.
/// 在调用方已持有状态 IO 锁时写入 state.json。
fn write_state_unlocked(
    session_dir: &Path,
    state: &SegmentedSessionStateDocument,
) -> Result<(), String> {
    write_json_file_atomic(&state_path(session_dir), state)
}

/// Replaces only the session-boundary thread state inside state.json.
/// 仅覆盖 state.json 内的 会话边界线程状态。
pub(in crate::workspace_store) fn replace_session_boundary_state(
    session_dir: &Path,
    state: Value,
) -> Result<(), String> {
    let _guard = segmented_state_file_io_lock()
        .lock()
        .map_err(|error| format!("锁定分段 state.json 失败：{error}"))?;
    let mut state_document =
        read_state_unlocked(session_dir)?.unwrap_or_else(|| SegmentedSessionStateDocument {
            schema_version: 1,
            session_id: String::new(),
            updated_at: super::current_unix_millis_text(),
            runtime_state: Value::Null,
            current_context: Value::Null,
            session_boundary_state: Value::Null,
        });
    state_document.session_boundary_state = state;
    state_document.updated_at = super::current_unix_millis_text();
    write_state_unlocked(session_dir, &state_document)
}

/// Loads the session-boundary thread state from state.json when present.
/// 从 state.json 读取存在的 会话边界线程状态。
pub(in crate::workspace_store) fn load_session_boundary_state(
    session_dir: &Path,
) -> Result<Option<Value>, String> {
    Ok(read_state(session_dir)?.and_then(|state| {
        (!state.session_boundary_state.is_null()).then_some(state.session_boundary_state)
    }))
}

/// Builds a segmented state document from the existing runtime-state entry.
/// 根据现有运行态条目构建分段状态文档。
pub(in crate::workspace_store) fn state_document_from_runtime_state(
    session_id: &str,
    runtime_state: &SessionRuntimeStateEntry,
) -> Result<SegmentedSessionStateDocument, String> {
    validate_session_conversation_message_identities(&runtime_state.conversation_state.messages)
        .map_err(|error| format!("Invalid canonical conversation message identity: {error}"))?;
    validate_session_conversation_record_identities(&runtime_state.conversation_state.records)
        .map_err(|error| format!("Invalid canonical conversation record identity: {error}"))?;
    // Serialize only lightweight runtime metadata; canonical conversation history lives in append-only segments.
    // 仅序列化轻量运行时元数据；规范会话历史保存在追加式分段中。
    let mut runtime_state_value = serde_json::to_value(runtime_state).map_err(to_store_error)?;
    if let Some(runtime_state_object) = runtime_state_value.as_object_mut() {
        runtime_state_object.remove("conversation_state");
    }
    // Keep the current-context view explicit without duplicating the canonical conversation window.
    // 显式保留当前上下文视图，但不重复保存规范会话窗口。
    let current_context = json!({
        "context_start_message_id": runtime_state.context_start_message_id.clone(),
        "last_compaction_id": runtime_state.last_compaction_id.clone(),
        "anchor_context_budget_fingerprint": runtime_state
            .anchor_context_budget_fingerprint
            .clone(),
        "anchor_context_assembly_fingerprint": runtime_state
            .anchor_context_assembly_fingerprint
            .clone()
    });
    Ok(SegmentedSessionStateDocument {
        schema_version: 1,
        session_id: session_id.to_string(),
        updated_at: runtime_state.updated_at.clone(),
        runtime_state: runtime_state_value,
        current_context,
        session_boundary_state: Value::Null,
    })
}

/// Applies an overwrite-only metadata patch to a main segmented session.
/// 对主分段会话应用覆盖式元数据补丁。
pub(in crate::workspace_store) fn patch_segmented_session_metadata(
    session_dir: &Path,
    patch: SegmentedSessionMetadataPatch,
) -> Result<SegmentedSessionMetadata, String> {
    // Load the current metadata so omitted fields remain unchanged.
    // 加载当前元数据，确保未提供字段保持不变。
    let mut metadata = read_session_metadata(session_dir)?;
    if let Some(updated_at) = patch.updated_at {
        metadata.updated_at = updated_at;
    }
    if let Some(title) = patch.title {
        metadata.title = title;
    }
    if let Some(title_source) = patch.title_source {
        metadata.title_source = title_source;
    }
    if let Some(archived_at) = patch.archived_at {
        metadata.archived_at = archived_at;
    }
    write_metadata(session_dir, &metadata)?;
    Ok(metadata)
}

/// Writes the display-only fork source marker into one session metadata document.
/// 将纯展示用的分叉来源标记写入单个会话元数据文档。
pub(in crate::workspace_store) fn write_segmented_session_fork_source(
    session_dir: &Path,
    fork_source: SegmentedSessionForkSource,
) -> Result<(), String> {
    let _guard = segmented_state_file_io_lock()
        .lock()
        .map_err(to_store_error)?;
    let mut metadata = read_session_metadata(session_dir)?;
    metadata.fork_source = Some(fork_source);
    write_metadata(session_dir, &metadata)
}
