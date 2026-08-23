//! Segmented session state document persistence.
//! 分段会话状态文档持久化。

use super::*;
use serde::Serializer;
use serde::ser::SerializeStruct;

/// Borrowed serializer that excludes canonical conversation payload before JSON construction.
/// 在构建 JSON 前排除规范会话正文的借用序列化器。
struct SegmentedRuntimeStateMetadata<'a>(&'a SessionRuntimeStateEntry);

impl Serialize for SegmentedRuntimeStateMetadata<'_> {
    /// Serializes every persisted runtime field without visiting canonical conversation messages.
    /// 序列化每个持久运行态字段，但不访问规范会话消息。
    ///
    /// # Parameters
    /// 参数
    /// `serializer` receives the lightweight segmented-state projection.
    /// `serializer` 接收轻量分段状态投影。
    ///
    /// # Returns
    /// 返回值
    /// The serializer output, or its precise serialization error.
    /// 返回序列化器输出或其精确序列化错误。
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        // Exhaustive destructuring makes every newly added runtime field a compile-time decision:
        // it must be persisted below or explicitly classified as process-local/canonical history.
        // 穷尽解构会把每个新增运行态字段变成编译期决策：必须在下方持久化，或明确归类为进程内/规范历史。
        let SessionRuntimeStateEntry {
            active_agent_id,
            active_provider_id,
            active_connection_id,
            active_model_id,
            pending_model_selection,
            provider_route_lease,
            active_context_window_policy,
            context_compaction_window_node_id,
            active_reasoning_effort,
            active_reasoning_effort_explicit,
            active_temperature,
            provider_tool_selections,
            media_understanding_modes,
            provider_tool_enabled_capabilities,
            media_reference_ledger,
            provider_extension_target_sets,
            provider_tool_settings_initialized,
            provider_tool_selections_overridden,
            active_work_mode,
            prompt_skill_overrides,
            context_injection_state,
            vmm,
            knowledge_library_selection: _,
            todolist,
            plan_mode,
            output_style,
            vmm_writeback_outbox,
            vmm_profile_sync,
            last_compaction_id,
            auto_compaction_consecutive_failures,
            context_start_message_id,
            live_context_budget_fingerprint,
            live_context_assembly_fingerprint,
            anchor_context_budget_fingerprint,
            anchor_context_assembly_fingerprint,
            last_provider_request_preview_markdown,
            ui_state,
            conversation_state: _,
            tool_call_record_ids,
            tool_request_record_ids,
            last_provider_usage: _,
            last_successful_provider_usage: _,
            provider_call_ledger,
            execution_retry_snapshot,
            transient_execution_retry_snapshot: _,
            auto_compact_prefill_input_tokens,
            active_precheck_turn_id,
            session_runtime_kernel,
            updated_at,
        } = self.0;
        // state contains only fields whose owning serde contract allows persistence.
        // state 只包含所属 serde 契约允许持久化的字段。
        let mut state = serializer.serialize_struct("SessionRuntimeStateEntry", 44)?;
        // serializeAlways preserves fields that have no skip predicate in the model contract.
        // serializeAlways 保留模型契约中没有跳过谓词的字段。
        macro_rules! serialize_always {
            ($($field:ident),+ $(,)?) => {
                $(state.serialize_field(stringify!($field), $field)?;)+
            };
        }
        // serializeOption preserves optional fields only when an exact value exists.
        // serializeOption 仅在精确值存在时保留可选字段。
        macro_rules! serialize_option {
            ($($field:ident),+ $(,)?) => {
                $(if $field.is_some() {
                    state.serialize_field(stringify!($field), $field)?;
                })+
            };
        }
        // serializeNonEmpty preserves collection or state fields only when they contain payload.
        // serializeNonEmpty 仅在集合或状态字段包含载荷时保留它们。
        macro_rules! serialize_non_empty {
            ($($field:ident),+ $(,)?) => {
                $(if !$field.is_empty() {
                    state.serialize_field(stringify!($field), $field)?;
                })+
            };
        }
        serialize_always!(
            active_agent_id,
            active_provider_id,
            active_model_id,
            last_compaction_id,
            context_start_message_id,
            ui_state,
            updated_at,
        );
        serialize_option!(
            active_connection_id,
            pending_model_selection,
            provider_route_lease,
            active_context_window_policy,
            context_compaction_window_node_id,
            active_reasoning_effort,
            active_temperature,
            active_work_mode,
            live_context_budget_fingerprint,
            live_context_assembly_fingerprint,
            anchor_context_budget_fingerprint,
            anchor_context_assembly_fingerprint,
            last_provider_request_preview_markdown,
            auto_compact_prefill_input_tokens,
            active_precheck_turn_id,
            session_runtime_kernel,
        );
        serialize_non_empty!(
            provider_tool_selections,
            media_understanding_modes,
            provider_tool_enabled_capabilities,
            media_reference_ledger,
            provider_extension_target_sets,
            prompt_skill_overrides,
            context_injection_state,
            vmm,
            todolist,
            plan_mode,
            output_style,
            vmm_writeback_outbox,
            vmm_profile_sync,
            tool_call_record_ids,
            tool_request_record_ids,
            provider_call_ledger,
        );
        if *active_reasoning_effort_explicit {
            state.serialize_field(
                "active_reasoning_effort_explicit",
                active_reasoning_effort_explicit,
            )?;
        }
        if *provider_tool_settings_initialized {
            state.serialize_field(
                "provider_tool_settings_initialized",
                provider_tool_settings_initialized,
            )?;
        }
        if *provider_tool_selections_overridden {
            state.serialize_field(
                "provider_tool_selections_overridden",
                provider_tool_selections_overridden,
            )?;
        }
        if *auto_compaction_consecutive_failures != 0 {
            state.serialize_field(
                "auto_compaction_consecutive_failures",
                auto_compaction_consecutive_failures,
            )?;
        }
        if let Some(reference) = execution_retry_snapshot
            && reference.content_bytes > 0
        {
            state.serialize_field("execution_retry_snapshot", execution_retry_snapshot)?;
        }
        state.end()
    }
}

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
    // Serialize the borrowed metadata view directly; canonical conversation history is never
    // visited and remains solely in append-only segments.
    // 直接序列化借用元数据视图；规范会话历史完全不被访问，并仅保存在追加式分段中。
    let runtime_state_value = serde_json::to_value(SegmentedRuntimeStateMetadata(runtime_state))
        .map_err(|error| format!("serialize segmented runtime state failed: {error}"))?;
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
