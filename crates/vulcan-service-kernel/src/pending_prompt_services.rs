//! WebUI pending prompt queue RPC service handlers.
//! WebUI 待发送提示词队列 RPC 服务处理器。

use std::collections::BTreeSet;
use std::path::Path;

use vulcan_agent_runtime::attachment_asset_index::release_staged_attachment_scopes;
use vulcan_core::types::ConversationAttachment;
use vulcan_service_api::{PendingPromptEditOutcome, PendingPromptSummary, ServiceEventEmitter};
use vulcan_session_core::v1::config::QueuedInputIdentity;
use vulcan_session_core::v1::document::QueuedInputDraftReceipt;
use vulcan_session_core::v1::event::{EventBody, QueuedInputResolutionKind};
use vulcan_session_core::v1::ids::ClientActionId;
use vulcan_workspace_store::workspace_store::{
    ProjectResourceCatalog, load_project_resource_catalog,
};

use super::pending_prompts::{
    pending_prompt_attachment_staging_scope, pending_prompt_preview_text,
    pending_prompt_process_mirror_from_v1_queued_inputs,
};
use super::session_goal_lifecycle::{
    conversation_attachments_from_resource_ids, current_v1_goal_unix_millis,
    session_v1_handle_from_state,
};
use super::session_runtime_registry_v1::SessionHandle;
use super::{
    SessionId, SharedServiceBridgeState, composer_attachment_summary_from_core,
    composer_attachment_workspace_path_from_snapshot, emit_pending_prompts_changed_delta,
    parse_session_id, selected_session_id_from_state,
};

/// Resolves the same project attachment root used at queue admission.
/// 解析队列准入时使用的同一项目附件根目录。
///
/// # Parameters
/// 参数
/// `state` owns the current workspace snapshot and `session_id` selects its exact project.
/// `state` 持有当前工作区快照，`session_id` 选择其精确项目。
///
/// # Returns
/// 返回值
/// The authoritative attachment path, or an explicit missing-owner error.
/// 返回权威附件路径，或明确的所有者缺失错误。
pub(crate) fn pending_prompt_attachment_workspace_path(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
) -> Result<String, String> {
    let guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    let snapshot = guard
        .workspace_snapshot
        .as_ref()
        .ok_or_else(|| String::from("workspace snapshot is not ready"))?;
    composer_attachment_workspace_path_from_snapshot(snapshot, None, Some(&session_id.to_string()))
}

/// Retries only V1-recorded queue attachment releases after a session is loaded.
/// 会话载入后仅重试 V1 已记录的队列附件释放。
///
/// # Parameters
/// 参数
/// `handle` owns the durable pending actions, `session_id` namespaces their leases, and
/// `workspace_path` is the authoritative attachment root used when those leases were acquired.
/// `handle` 持有可靠的待清理动作，`session_id` 划定租约作用域，`workspace_path` 是取得租约时
/// 使用的权威附件根目录。
///
/// # Returns
/// 返回值
/// Success after index release and durable acknowledgement, or an explicit retryable error.
/// 在索引释放和可靠确认后返回成功，否则返回明确的可重试错误。
pub(crate) fn reconcile_pending_prompt_attachment_leases(
    handle: &SessionHandle,
    session_id: SessionId,
    workspace_path: &str,
) -> Result<(), String> {
    let pending = handle
        .read(|document| {
            document
                .metadata
                .input_state
                .pending_staging_release_actions
                .clone()
        })
        .map_err(|error| format!("读取 V1 队列附件释放意图失败：{error}"))?;
    if pending.is_empty() {
        return Ok(());
    }
    let scopes = pending
        .iter()
        .map(|action| pending_prompt_attachment_staging_scope(session_id, action.as_str()))
        .collect::<Result<BTreeSet<_>, _>>()?;
    release_staged_attachment_scopes(workspace_path, &scopes)
        .map_err(|error| format!("重试 V1 队列附件租约释放失败：{error}"))?;
    for action in pending {
        handle
            .acknowledge_queued_staging_release(action, current_v1_goal_unix_millis()?)
            .map_err(|error| format!("确认 V1 队列附件租约释放失败：{error}"))?;
    }
    Ok(())
}

/// Parses one visible queue entry id as the authoritative browser action identity.
/// 把一条可见队列条目 ID 解析为权威浏览器动作身份。
pub(crate) fn pending_prompt_action_id(entry_id: &str) -> Result<ClientActionId, String> {
    ClientActionId::try_new(entry_id.to_string())
        .map_err(|error| format!("待发送消息身份无效：{error}"))
}

/// Resolves the current or terminal local queue identity for one exact browser action.
/// 为一个精确浏览器动作解析当前或已终结的本地队列身份。
pub(crate) fn pending_prompt_queue_identity(
    handle: &SessionHandle,
    client_action_id: &ClientActionId,
) -> Result<QueuedInputIdentity, String> {
    let identity = handle
        .read(|document| document.queued_input_identity_for_action(client_action_id))
        .map_err(|error| format!("读取 V1 待发送消息身份失败：{error}"))?
        .map_err(|error| format!("解析 V1 待发送消息身份失败：{error}"))?;
    identity.ok_or_else(|| format!("没有找到待发送消息 `{}`。", client_action_id.as_str()))
}

/// Rebuilds only the visible queue panel from authoritative V1 metadata.
/// 仅从权威 V1 metadata 重建可见队列面板。
///
/// # Parameters
/// 参数
/// `state` owns the process-local projection, `session_id` and `handle` select the V1 authority,
/// while `project_directory` and `catalog` resolve ordered resource display data.
/// `state` 持有进程内投影，`session_id` 与 `handle` 选择 V1 权威，
/// `project_directory` 与 `catalog` 解析有序资源展示数据。
///
/// # Returns
/// 返回值
/// Returns whether the visible process projection changed, or an explicit read/lock error.
/// 返回可见进程投影是否发生变化，或明确的读取/加锁错误。
pub(crate) fn rebuild_pending_prompt_panel_from_v1(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    handle: &SessionHandle,
    project_directory: &Path,
    catalog: &ProjectResourceCatalog,
) -> Result<bool, String> {
    let queued_inputs = handle
        .read(|document| document.metadata.input_state.queued_inputs.clone())
        .map_err(|error| format!("读取 V1 待发送消息队列失败：{error}"))?;
    let rebuilt =
        pending_prompt_process_mirror_from_v1_queued_inputs(&queued_inputs, |queued_input| {
            Ok(conversation_attachments_from_resource_ids(
                project_directory,
                catalog,
                &queued_input.resource_ids,
            ))
        })?;
    let mut guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    let panel_queue = &mut guard
        .pending_prompt_process_mirrors
        .entry(session_id)
        .or_default()
        .ui_state
        .pending_prompt_panel_queue;
    if *panel_queue == rebuilt.ui_state.pending_prompt_panel_queue {
        return Ok(false);
    }
    *panel_queue = rebuilt.ui_state.pending_prompt_panel_queue;
    Ok(true)
}

/// Reads a previously committed move timestamp before attempting the idempotent mutation.
/// 在尝试幂等变更前读取先前已提交的移动时间戳。
fn existing_moved_pending_prompt_timestamp(
    handle: &SessionHandle,
    identity: &QueuedInputIdentity,
) -> Result<Option<i64>, String> {
    handle
        .read(|document| {
            document.events().iter().find_map(|event| {
                let EventBody::QueuedInputResolution(resolution) = &event.event else {
                    return None;
                };
                (resolution.client_action_id == identity.client_action_id
                    && resolution.input_id == identity.input_id
                    && resolution.resolution == QueuedInputResolutionKind::MovedToDraft)
                    .then_some(event.updated_at_ms)
            })
        })
        .map_err(|error| format!("读取 V1 待发送消息处置事实失败：{error}"))
}

/// Builds the edit response only from the durable draft receipt and project resources.
/// 仅从持久化草稿 receipt 与项目资源构建编辑响应。
fn pending_prompt_summary_from_durable_draft(
    identity: &QueuedInputIdentity,
    text: &str,
    attachments: &[ConversationAttachment],
    created_at_ms: i64,
) -> PendingPromptSummary {
    PendingPromptSummary {
        id: identity.client_action_id.as_str().to_string(),
        prompt: text.to_string(),
        preview: pending_prompt_preview_text(text, attachments),
        attachments: attachments
            .iter()
            .map(composer_attachment_summary_from_core)
            .collect(),
        state: String::from("ready"),
        created_at: created_at_ms.to_string(),
    }
}

/// Builds a committed edit outcome while retaining every post-commit projection fault as one warning.
/// 构建已提交编辑结果，并把每项提交后投影故障保留为一条警告。
fn committed_pending_prompt_edit_outcome(
    entry: PendingPromptSummary,
    attachment_release: Result<(), String>,
    panel_projection: Result<bool, String>,
    delta_projection: Result<u64, String>,
) -> PendingPromptEditOutcome {
    let mut projection_warnings = Vec::new();
    if let Err(error) = attachment_release {
        projection_warnings.push(format!("清理已处置消息的附件租约失败：{error}"));
    }
    if let Err(error) = panel_projection {
        projection_warnings.push(format!("刷新队列投影失败：{error}"));
    }
    if let Err(error) = delta_projection {
        projection_warnings.push(format!("发布队列变更事件失败：{error}"));
    }
    // Post-commit projection faults are explicit warnings and never disguise the durable move as failure.
    // 提交后的投影故障以明确警告返回，绝不把持久移动伪装成失败。
    let projection_warning = (!projection_warnings.is_empty()).then(|| {
        format!(
            "待发送提示词已恢复，但局部投影需要重新刷新：{}",
            projection_warnings.join("；")
        )
    });
    PendingPromptEditOutcome {
        entry,
        projection_warning,
    }
}

/// Restores one visible pending prompt into the WebUI composer and removes it from the queue.
/// 将单条可见待发送提示词恢复到 WebUI 输入区，并从队列中移除。
///
/// # Parameters
/// 参数
/// `app` emits the updated workbench snapshot.
/// `app` 发出更新后的工作台快照。
/// `state` owns retained service runtime and workspace state.
/// `state` 持有保留的服务运行时与工作区状态。
/// `entry_id` selects the pending prompt panel row to edit.
/// `entry_id` 选择要编辑的待发送提示词面板行。
///
/// # Returns
/// 返回值
/// The removed prompt summary for restoring into the composer.
/// 返回用于恢复到输入区的已移除提示词摘要。
pub(super) async fn service_edit_pending_prompt(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    entry_id: String,
) -> Result<PendingPromptEditOutcome, String> {
    let selected_session_id = selected_session_id_from_state(&state)?;
    let client_action_id = pending_prompt_action_id(&entry_id)?;
    let handle = session_v1_handle_from_state(&state, selected_session_id).await?;
    let identity = pending_prompt_queue_identity(handle.as_ref(), &client_action_id)?;
    let attachment_workspace_path =
        pending_prompt_attachment_workspace_path(&state, selected_session_id)?;
    let locator = handle
        .storage_locator()
        .map_err(|error| format!("读取 V1 会话资源定位失败：{error}"))?;
    let project_directory = locator.project_directory();
    let catalog = load_project_resource_catalog(&project_directory)
        .map_err(|error| format!("加载 V1 项目资源目录失败：{error}"))?;
    let existing_moved_at_ms = existing_moved_pending_prompt_timestamp(handle.as_ref(), &identity)?;
    let now_ms = current_v1_goal_unix_millis()?;
    let receipt = handle
        .move_queued_input_to_draft(identity, now_ms)
        .map_err(|error| format!("把 V1 待发送消息恢复为草稿失败：{error}"))?;
    let QueuedInputDraftReceipt { identity, draft } = receipt.value;
    let attachments = conversation_attachments_from_resource_ids(
        &project_directory,
        &catalog,
        &draft.resource_ids,
    );
    // A new move is committed at `now_ms`; an idempotent retry keeps the original durable timestamp.
    // 新移动在 `now_ms` 提交；幂等重试保留原始持久化时间戳。
    let created_at_ms = existing_moved_at_ms.unwrap_or(now_ms);
    let summary = pending_prompt_summary_from_durable_draft(
        &identity,
        &draft.text,
        &attachments,
        created_at_ms,
    );
    let attachment_release = reconcile_pending_prompt_attachment_leases(
        handle.as_ref(),
        selected_session_id,
        &attachment_workspace_path,
    );
    // The durable move is already committed. Projection failure must not disguise it as a pre-commit error.
    // 持久移动已经提交；投影失败不能伪装成提交前错误。
    let panel_projection = rebuild_pending_prompt_panel_from_v1(
        &state,
        selected_session_id,
        handle.as_ref(),
        &project_directory,
        &catalog,
    );
    let delta_projection = emit_pending_prompts_changed_delta(&app, &state, selected_session_id);
    Ok(committed_pending_prompt_edit_outcome(
        summary,
        attachment_release,
        panel_projection,
        delta_projection,
    ))
}

/// Removes one visible pending prompt from the selected conversation queue.
/// 从当前选中会话队列中移除单条可见待发送提示词。
///
/// # Parameters
/// 参数
/// `app` emits the refreshed workbench snapshot.
/// `app` 发出刷新后的工作台快照。
/// `state` owns retained service runtime and workspace state.
/// `state` 持有保留的服务运行时与工作区状态。
/// `entry_id` selects the pending prompt panel row to remove.
/// `entry_id` 选择要移除的待发送提示词面板行。
///
/// # Returns
/// 返回值
/// The updated workbench snapshot after removal.
/// 返回移除后的更新工作台快照。
pub(super) async fn service_remove_pending_prompt(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    entry_id: String,
) -> Result<bool, String> {
    let selected_session_id = selected_session_id_from_state(&state)?;
    let client_action_id = pending_prompt_action_id(&entry_id)?;
    let handle = session_v1_handle_from_state(&state, selected_session_id).await?;
    let identity = pending_prompt_queue_identity(handle.as_ref(), &client_action_id)?;
    let attachment_workspace_path =
        pending_prompt_attachment_workspace_path(&state, selected_session_id)?;
    let receipt = handle
        .remove_queued_input(identity, current_v1_goal_unix_millis()?)
        .map_err(|error| format!("移除 V1 待发送消息失败：{error}"))?;
    // The V1 removal is already durable; display projection must not skip the exact lease release.
    // V1 移除已可靠提交；显示投影失败不能跳过精确租约释放。
    reconcile_pending_prompt_attachment_leases(
        handle.as_ref(),
        selected_session_id,
        &attachment_workspace_path,
    )
    .map_err(|error| format!("待发送消息已从 V1 队列移除，但附件清理待重试：{error}"))?;
    let locator = handle
        .storage_locator()
        .map_err(|error| format!("读取 V1 会话资源定位失败：{error}"))?;
    let project_directory = locator.project_directory();
    let catalog = load_project_resource_catalog(&project_directory)
        .map_err(|error| format!("加载 V1 项目资源目录失败：{error}"))?;
    rebuild_pending_prompt_panel_from_v1(
        &state,
        selected_session_id,
        handle.as_ref(),
        &project_directory,
        &catalog,
    )?;
    emit_pending_prompts_changed_delta(&app, &state, selected_session_id)?;
    Ok(receipt.changed)
}

/// Reorders the visible pending prompt queue for the selected conversation.
/// 重排当前选中会话的可见待发送提示词队列。
///
/// # Parameters
/// 参数
/// `app` emits the refreshed workbench snapshot.
/// `app` 发出刷新后的工作台快照。
/// `state` owns retained service runtime and workspace state.
/// `state` 持有保留的服务运行时与工作区状态。
/// `conversation_id` optionally selects the conversation queue to reorder.
/// `conversation_id` 可选指定要重排的会话队列。
/// `dragged_entry_id` selects the moved pending prompt row.
/// `dragged_entry_id` 选择被移动的待发送提示词行。
/// `target_entry_id` selects the row before which the dragged row should be placed.
/// `target_entry_id` 选择被移动行应放置到其前方的目标行。
///
/// # Returns
/// 返回值
/// The updated workbench snapshot after reordering.
/// 返回重排后的更新工作台快照。
pub(super) async fn service_reorder_pending_prompts(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    conversation_id: Option<String>,
    dragged_entry_id: String,
    target_entry_id: Option<String>,
) -> Result<bool, String> {
    let selected_session_id = conversation_id
        .as_deref()
        .and_then(parse_session_id)
        .map(Ok)
        .unwrap_or_else(|| selected_session_id_from_state(&state))?;
    let dragged_action_id = pending_prompt_action_id(&dragged_entry_id)?;
    let target_action_id = target_entry_id
        .filter(|entry_id| !entry_id.trim().is_empty())
        .map(|entry_id| pending_prompt_action_id(&entry_id))
        .transpose()?;
    let handle = session_v1_handle_from_state(&state, selected_session_id).await?;
    let current_order = handle
        .read(|document| {
            document
                .metadata
                .input_state
                .queued_inputs
                .iter()
                .map(|queued| QueuedInputIdentity {
                    client_action_id: queued.client_action_id.clone(),
                    input_id: queued.input_id,
                })
                .collect::<Vec<_>>()
        })
        .map_err(|error| format!("读取 V1 待发送消息顺序失败：{error}"))?;
    let dragged_identity = current_order
        .iter()
        .find(|identity| identity.client_action_id == dragged_action_id)
        .cloned()
        .ok_or_else(|| format!("没有找到待移动消息 `{}`。", dragged_action_id.as_str()))?;
    let ordered = if target_action_id.as_ref() == Some(&dragged_action_id) {
        current_order
    } else {
        let mut ordered = current_order
            .into_iter()
            .filter(|identity| identity.client_action_id != dragged_action_id)
            .collect::<Vec<_>>();
        if let Some(target_action_id) = target_action_id {
            let target_index = ordered
                .iter()
                .position(|identity| identity.client_action_id == target_action_id)
                .ok_or_else(|| format!("没有找到目标消息 `{}`。", target_action_id.as_str()))?;
            ordered.insert(target_index, dragged_identity);
        } else {
            ordered.push(dragged_identity);
        }
        ordered
    };
    let receipt = handle
        .reorder_queued_inputs(ordered, current_v1_goal_unix_millis()?)
        .map_err(|error| format!("重排 V1 待发送消息失败：{error}"))?;
    let locator = handle
        .storage_locator()
        .map_err(|error| format!("读取 V1 会话资源定位失败：{error}"))?;
    let project_directory = locator.project_directory();
    let catalog = load_project_resource_catalog(&project_directory)
        .map_err(|error| format!("加载 V1 项目资源目录失败：{error}"))?;
    rebuild_pending_prompt_panel_from_v1(
        &state,
        selected_session_id,
        handle.as_ref(),
        &project_directory,
        &catalog,
    )?;
    emit_pending_prompts_changed_delta(&app, &state, selected_session_id)?;
    Ok(receipt.changed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::test_support::TestHostRuntimeRoot;
    use vulcan_agent_runtime::attachment_assets::resolve_managed_attachment_root;
    use vulcan_session_core::v1::document::ClientInputSubmission;
    use vulcan_workspace_store::workspace_store::load_session_v1_full;

    /// A durable edit remains a successful typed outcome when both local projections fail afterwards.
    /// 持久编辑完成后即使两个本地投影均失败，也仍返回成功的强类型结果。
    #[test]
    fn committed_edit_preserves_entry_across_projection_failures() {
        let entry = PendingPromptSummary {
            id: String::from("queued-action"),
            prompt: String::from("exact queued target"),
            preview: String::from("exact queued target"),
            attachments: Vec::new(),
            state: String::from("ready"),
            created_at: String::from("1"),
        };
        let outcome = committed_pending_prompt_edit_outcome(
            entry.clone(),
            Err(String::from("lease unavailable")),
            Err(String::from("panel unavailable")),
            Err(String::from("delta unavailable")),
        );
        assert_eq!(outcome.entry, entry);
        let warning = outcome.projection_warning.expect("controlled warning");
        assert!(warning.contains("lease unavailable"));
        assert!(warning.contains("panel unavailable"));
        assert!(warning.contains("delta unavailable"));
    }

    /// A failed asset-index read retains the V1 release intent for a later exact retry.
    /// 资产索引读取失败时保留 V1 释放意图，供后续精确重试。
    #[test]
    fn failed_lease_release_retries_and_durably_acknowledges() {
        let session_id = SessionId::new_v4();
        let fixture = TestHostRuntimeRoot::new(session_id);
        let locator = fixture.v1_storage_locator();
        let action = ClientActionId::try_new("shared-action").expect("bounded action");
        let document = load_session_v1_full(&locator)
            .expect("load valid fixture document")
            .document;
        let handle = SessionHandle::new(session_id, locator.clone(), document);
        let queued = handle
            .enqueue_input(
                ClientInputSubmission::new(action.clone(), "queued attachment repair", Vec::new()),
                2,
            )
            .expect("persist actual queue admission")
            .value;
        handle
            .remove_queued_input(
                QueuedInputIdentity {
                    client_action_id: action.clone(),
                    input_id: queued.input_id,
                },
                3,
            )
            .expect("persist queue resolution and release intent together");
        drop(handle);
        let pending_document = load_session_v1_full(&locator)
            .expect("restart from the persisted pending release")
            .document;
        assert!(
            pending_document
                .metadata
                .input_state
                .pending_staging_release_actions
                .contains(&action)
        );
        let handle = SessionHandle::new(session_id, locator.clone(), pending_document);
        let workspace = tempfile::tempdir().expect("isolated attachment workspace");
        let workspace_path = workspace.path().to_string_lossy().into_owned();
        let index_directory = resolve_managed_attachment_root(&workspace_path).join("index");
        std::fs::create_dir_all(&index_directory).expect("create isolated asset index");
        let index_path = index_directory.join("assets.json");
        std::fs::write(&index_path, b"invalid json").expect("inject unreadable asset index");

        assert!(
            reconcile_pending_prompt_attachment_leases(
                handle.as_ref(),
                session_id,
                &workspace_path,
            )
            .is_err()
        );
        assert!(
            handle
                .read(|document| document
                    .metadata
                    .input_state
                    .pending_staging_release_actions
                    .contains(&action))
                .expect("read retained repair intent")
        );

        std::fs::remove_file(&index_path).expect("restore readable asset index");
        reconcile_pending_prompt_attachment_leases(handle.as_ref(), session_id, &workspace_path)
            .expect("retry should release and acknowledge");
        assert!(
            handle
                .read(|document| document
                    .metadata
                    .input_state
                    .pending_staging_release_actions
                    .is_empty())
                .expect("read acknowledged intent")
        );
        let reloaded = load_session_v1_full(&locator).expect("reload durable V1 session");
        assert!(
            reloaded
                .document
                .metadata
                .input_state
                .pending_staging_release_actions
                .is_empty()
        );
    }
}
