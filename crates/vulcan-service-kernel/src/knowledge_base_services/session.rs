//! Session selection contains authorization ceilings only.
//! 会话选择仅包含授权上限。

use super::*;

/// Persists request's exact library selection after ownership and live authorization checks; app emits the addon change.
/// 在归属及实时授权检查后持久化 request 的精确库选择；app 发布附属变更。
pub(crate) async fn service_set_session_knowledge_libraries(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: SessionKnowledgeLibrarySelectionRequest,
) -> Result<Value, String> {
    let session_id = parse_session_id(&request.conversation_id).ok_or_else(|| {
        String::from("Invalid session identifier; the knowledge library selection cannot be saved.")
    })?;
    let mode = match request.mode.as_str() {
        "inherit" => KnowledgeLibrarySelectionModeV1::Inherit,
        "explicit" => KnowledgeLibrarySelectionModeV1::Explicit,
        value => return Err(format!("Unknown knowledge library selection mode: {value}")),
    };
    let runtime = retained_knowledge_runtime(&state)?;
    let authorized: BTreeSet<String> = runtime
        .authorized_library_ids(&request.project_id, false)
        .await?
        .into_iter()
        .collect();
    if let Some(denied) = request
        .library_ids
        .iter()
        .find(|library_id| !authorized.contains(*library_id))
    {
        return Err(format!(
            "Knowledge libraries not authorized for the current project: {denied}"
        ));
    }

    {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let workspace_snapshot = guard.workspace_snapshot.as_ref().ok_or_else(|| {
            String::from("service is still starting; workspace snapshot is not ready")
        })?;
        if owning_project_id_for_session(workspace_snapshot, session_id).as_deref()
            != Some(request.project_id.as_str())
        {
            return Err(String::from(
                "The current session does not match the requested project; saving the knowledge library selection was refused.",
            ));
        }
    }
    let mut library_ids = request.library_ids;
    library_ids.sort();
    library_ids.dedup();
    if mode == KnowledgeLibrarySelectionModeV1::Inherit {
        library_ids.clear();
    }
    let selection = KnowledgeLibrarySelectionConfigV1 { mode, library_ids };
    let updated_at_ms = current_v1_goal_unix_millis()?;
    let handle = session_v1_handle_from_state(&state, session_id).await?;
    set_knowledge_library_selection_v1(handle.as_ref(), selection.clone(), updated_at_ms)?;
    let (persisted, revision, updated_at) = handle
        .read(|document| {
            (
                document
                    .metadata
                    .config
                    .selection
                    .knowledge_libraries
                    .clone(),
                document.memory_revision(),
                document.metadata.updated_at_ms.to_string(),
            )
        })
        .map_err(|error| format!("Failed to read the V1 knowledge library selection: {error}"))?;
    {
        let mut guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let workspace = guard
            .workspace_snapshot
            .as_mut()
            .ok_or_else(|| String::from("service workspace snapshot disappeared"))?;
        let runtime_state = workspace
            .session_runtime_state
            .entry(session_id)
            .or_insert_with(SessionRuntimeStateEntry::default);
        runtime_state.knowledge_library_selection = SessionKnowledgeLibrarySelection {
            mode: match persisted.mode {
                KnowledgeLibrarySelectionModeV1::Inherit => {
                    SessionKnowledgeLibrarySelectionMode::Inherit
                }
                KnowledgeLibrarySelectionModeV1::Explicit => {
                    SessionKnowledgeLibrarySelectionMode::Explicit
                }
            },
            library_ids: persisted.library_ids.clone(),
        };
    }
    emit_session_addon_changed_event(
        &app,
        session_id,
        &[SessionAddonNamespace::KnowledgeBases],
        revision,
        updated_at,
    );
    service_get_session_knowledge_libraries(state, request.conversation_id).await
}

/// Reads conversation_id from canonical metadata and computes current project authorization in state.
/// 从规范元数据读取 conversation_id，并在 state 中计算当前项目授权。
pub(crate) async fn service_get_session_knowledge_libraries(
    state: SharedServiceBridgeState,
    conversation_id: String,
) -> Result<Value, String> {
    let session_id = parse_session_id(&conversation_id).ok_or_else(|| {
        String::from("Invalid session identifier; the knowledge library selection cannot be read.")
    })?;
    let (runtime, project_id) = {
        let guard = state
            .lock()
            .map_err(|error| format!("failed to lock service state: {error}"))?;
        let runtime = guard.knowledge_runtime.clone().ok_or_else(|| {
            String::from("service is still starting; knowledge runtime is not ready")
        })?;
        let workspace = guard.workspace_snapshot.as_ref().ok_or_else(|| {
            String::from("service is still starting; workspace snapshot is not ready")
        })?;
        let project_id = owning_project_id_for_session(workspace, session_id).ok_or_else(|| {
            String::from("The current session does not belong to an available project.")
        })?;
        (runtime, project_id)
    };
    let handle = session_v1_handle_from_state(&state, session_id).await?;
    let selection = handle
        .read(|document| {
            document
                .metadata
                .config
                .selection
                .knowledge_libraries
                .clone()
        })
        .map_err(|error| format!("Failed to read the V1 knowledge library selection: {error}"))?;
    let effective_library_ids = match selection.mode {
        KnowledgeLibrarySelectionModeV1::Inherit => {
            runtime.authorized_library_ids(&project_id, true).await?
        }
        KnowledgeLibrarySelectionModeV1::Explicit => {
            let authorized: BTreeSet<String> = runtime
                .authorized_library_ids(&project_id, false)
                .await?
                .into_iter()
                .collect();
            selection
                .library_ids
                .iter()
                .filter(|library_id| authorized.contains(*library_id))
                .cloned()
                .collect()
        }
    };
    let summary = SessionKnowledgeLibrarySelectionSummary {
        mode: match selection.mode {
            KnowledgeLibrarySelectionModeV1::Inherit => String::from("inherit"),
            KnowledgeLibrarySelectionModeV1::Explicit => String::from("explicit"),
        },
        library_ids: selection.library_ids,
        effective_library_ids,
    };
    serde_json::to_value(summary)
        .map_err(|error| format!("serialize session knowledge selection: {error}"))
}
