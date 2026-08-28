//! WebUI pending prompt queue RPC service handlers.
//! WebUI 待发送提示词队列 RPC 服务处理器。

use std::path::Path;

use vulcan_agent_runtime::attachment_asset_index::release_staged_attachment_ref;
use vulcan_core::types::ConversationAttachment;
use vulcan_service_api::{PendingPromptSummary, ServiceEventEmitter};
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
    emit_pending_prompts_changed_delta, parse_session_id, selected_session_id_from_state,
    webui_trace, workspace_path_for_session,
};

/// Describes one attachment-owner transition committed with a pending-queue snapshot mutation.
/// 描述一次与待发送队列快照变更共同提交的附件所有者迁移。
pub(crate) struct PendingPromptAttachmentLeaseTransition {
    /// Authoritative workspace path owning the managed asset index.
    /// 持有托管资产索引的权威工作区路径。
    workspace_path: String,
    /// Queue owner released only after the snapshot mutation is retained.
    /// 仅在快照变更被保留后释放的队列所有者。
    committed_release_scope: String,
    /// Canonical attachments governed by both sides of the transition.
    /// 由迁移两侧共同管辖的规范附件集合。
    attachments: Vec<ConversationAttachment>,
}

/// Releases one exact attachment-owner scope and retains failures as diagnostics.
/// 释放一个精确附件所有者作用域，并把失败保留为诊断。
///
/// # Parameters
/// 参数
/// `workspace_path`, `scope`, and `attachments` identify the exact durable lease; `phase` labels
/// the commit or rollback side.
/// `workspace_path`、`scope` 与 `attachments` 标识精确持久租约；`phase` 标记提交或回滚侧。
fn release_pending_prompt_attachment_lease_scope(
    workspace_path: &str,
    scope: &str,
    attachments: &[ConversationAttachment],
    phase: &str,
) {
    if let Err(error) = release_staged_attachment_ref(workspace_path, scope, attachments) {
        webui_trace(
            "pending_prompt_attachment_lease_release_failed",
            format!("scope={scope} phase={phase} error={error}"),
        );
    }
}

/// Commits one queue-to-next-owner transition after the snapshot store succeeds.
/// 在快照存储成功后提交一次队列到下一所有者的迁移。
pub(crate) fn commit_pending_prompt_attachment_lease_transition(
    transition: PendingPromptAttachmentLeaseTransition,
) {
    release_pending_prompt_attachment_lease_scope(
        &transition.workspace_path,
        &transition.committed_release_scope,
        &transition.attachments,
        "commit",
    );
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

/// Captures the transient queue attachment lease that may be released after V1 commit.
/// 捕获可在 V1 提交后释放的瞬时队列附件租约。
pub(crate) fn pending_prompt_attachment_transition(
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    entry_id: &str,
) -> Result<Option<PendingPromptAttachmentLeaseTransition>, String> {
    let guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    let Some(entry) = guard
        .pending_prompt_process_mirrors
        .get(&session_id)
        .and_then(|mirror| {
            mirror
                .ui_state
                .pending_prompt_panel_queue
                .iter()
                .find(|entry| entry.id == entry_id)
        })
        .cloned()
    else {
        return Ok(None);
    };
    let workspace_path = guard
        .workspace_snapshot
        .as_ref()
        .map(|workspace| workspace_path_for_session(workspace, session_id))
        .ok_or_else(|| String::from("workspace snapshot is not ready"))?;
    Ok(Some(PendingPromptAttachmentLeaseTransition {
        workspace_path,
        committed_release_scope: pending_prompt_attachment_staging_scope(&entry.id)?,
        attachments: entry.attachments,
    }))
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

/// Reads the exact durable resolution timestamp for a moved queue entry.
/// 读取一条已移动队列条目的精确持久化处置时间戳。
fn moved_pending_prompt_timestamp(
    handle: &SessionHandle,
    identity: &QueuedInputIdentity,
) -> Result<i64, String> {
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
        .map_err(|error| format!("读取 V1 待发送消息处置事实失败：{error}"))?
        .ok_or_else(|| String::from("V1 待发送消息已移动，但缺少对应的持久化处置事实。"))
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
) -> Result<PendingPromptSummary, String> {
    let selected_session_id = selected_session_id_from_state(&state)?;
    let client_action_id = pending_prompt_action_id(&entry_id)?;
    let handle = session_v1_handle_from_state(&state, selected_session_id).await?;
    let identity = pending_prompt_queue_identity(handle.as_ref(), &client_action_id)?;
    let attachment_transition =
        pending_prompt_attachment_transition(&state, selected_session_id, &entry_id)?;
    let now_ms = current_v1_goal_unix_millis()?;
    let receipt = handle
        .move_queued_input_to_draft(identity, now_ms)
        .map_err(|error| format!("把 V1 待发送消息恢复为草稿失败：{error}"))?;
    let locator = handle
        .storage_locator()
        .map_err(|error| format!("读取 V1 会话资源定位失败：{error}"))?;
    let project_directory = locator.project_directory();
    let catalog = load_project_resource_catalog(&project_directory)
        .map_err(|error| format!("加载 V1 项目资源目录失败：{error}"))?;
    let QueuedInputDraftReceipt { identity, draft } = receipt.value;
    let attachments = conversation_attachments_from_resource_ids(
        &project_directory,
        &catalog,
        &draft.resource_ids,
    );
    let created_at_ms = moved_pending_prompt_timestamp(handle.as_ref(), &identity)?;
    let summary = pending_prompt_summary_from_durable_draft(
        &identity,
        &draft.text,
        &attachments,
        created_at_ms,
    );
    rebuild_pending_prompt_panel_from_v1(
        &state,
        selected_session_id,
        handle.as_ref(),
        &project_directory,
        &catalog,
    )?;
    if let Some(transition) = attachment_transition {
        commit_pending_prompt_attachment_lease_transition(transition);
    }
    emit_pending_prompts_changed_delta(&app, &state, selected_session_id)?;
    Ok(summary)
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
    let attachment_transition =
        pending_prompt_attachment_transition(&state, selected_session_id, &entry_id)?;
    let receipt = handle
        .remove_queued_input(identity, current_v1_goal_unix_millis()?)
        .map_err(|error| format!("移除 V1 待发送消息失败：{error}"))?;
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
    if let Some(transition) = attachment_transition {
        commit_pending_prompt_attachment_lease_transition(transition);
    }
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
