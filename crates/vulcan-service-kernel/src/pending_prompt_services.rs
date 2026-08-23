//! WebUI pending prompt queue RPC service handlers.
//! WebUI 待发送提示词队列 RPC 服务处理器。

use vulcan_agent_runtime::attachment_asset_index::{
    acquire_staged_attachment_ref, release_staged_attachment_ref,
};
use vulcan_core::types::ConversationAttachment;
use vulcan_service_api::{PendingPromptSummary, ServiceEventEmitter};
use vulcan_workbench_model::{
    PendingPromptPanelDropTarget, apply_pending_prompt_panel_drop_target,
    remove_pending_prompt_panel_entry,
};

use super::conversation_submission::mutate_latest_workspace_for_existing_session;
use super::pending_prompts::{
    pending_prompt_attachment_staging_scope, pending_prompt_summary_from_panel_entry,
};
use super::{
    ControllerRuntimeHandle, SessionId, SharedServiceBridgeState, WorkspaceSnapshot,
    composer_attachment_draft_scope, current_unix_millis_text, emit_pending_prompts_changed_delta,
    parse_session_id, runtime_parts_from_state, selected_session_id_from_state,
    spawn_workspace_session_runtime_state_persist_task, webui_trace, workspace_path_for_session,
};

/// Describes one attachment-owner transition committed with a pending-queue snapshot mutation.
/// 描述一次与待发送队列快照变更共同提交的附件所有者迁移。
struct PendingPromptAttachmentLeaseTransition {
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

/// Error text returned while the workspace snapshot is still unavailable.
/// 工作区快照尚不可用时返回的错误文本。
const WORKSPACE_SNAPSHOT_NOT_READY_MESSAGE: &str =
    "service is still starting; workspace snapshot is not ready";

/// Describes the result and persistence needs of one pending prompt queue mutation.
/// 描述一次待发送提示词队列变更的结果与持久化需求。
struct PendingPromptQueueServiceMutation<T> {
    /// Value returned by the service entry after the shared mutation flow completes.
    /// 共享变更流程完成后由服务入口返回的值。
    result: T,
    /// Optional persistence reason used when the mutation changed stored session state.
    /// 当变更修改了已存储会话状态时使用的可选持久化原因。
    persist_reason: Option<&'static str>,
    /// Optional attachment-owner transition committed at the snapshot storage boundary.
    /// 在快照存储边界提交的可选附件所有者迁移。
    attachment_lease_transition: Option<PendingPromptAttachmentLeaseTransition>,
}

impl<T> PendingPromptQueueServiceMutation<T> {
    /// Builds a mutation outcome that must persist the changed session state.
    /// 构建必须持久化已变更会话状态的变更结果。
    ///
    /// # Parameters
    /// 参数
    /// `result` is the service return value.
    /// `result` 是服务返回值。
    /// `persist_reason` labels the asynchronous session persistence task.
    /// `persist_reason` 标记异步会话持久化任务。
    ///
    /// # Returns
    /// 返回值
    /// A mutation outcome with persistence enabled.
    /// 返回已启用持久化的变更结果。
    fn persisted(result: T, persist_reason: &'static str) -> Self {
        Self {
            result,
            persist_reason: Some(persist_reason),
            attachment_lease_transition: None,
        }
    }

    /// Builds a mutation outcome with conditional persistence.
    /// 构建按条件持久化的变更结果。
    ///
    /// # Parameters
    /// 参数
    /// `result` is the service return value.
    /// `result` 是服务返回值。
    /// `should_persist` indicates whether the session state actually changed.
    /// `should_persist` 表示会话状态是否真实发生变更。
    /// `persist_reason` labels the asynchronous session persistence task when needed.
    /// `persist_reason` 在需要时标记异步会话持久化任务。
    ///
    /// # Returns
    /// 返回值
    /// A mutation outcome whose persistence reason is present only when requested.
    /// 返回仅在需要时携带持久化原因的变更结果。
    fn maybe_persisted(result: T, should_persist: bool, persist_reason: &'static str) -> Self {
        Self {
            result,
            persist_reason: should_persist.then_some(persist_reason),
            attachment_lease_transition: None,
        }
    }

    /// Adds one attachment-owner transition to this queue mutation.
    /// 为当前队列变更增加一项附件所有者迁移。
    ///
    /// # Parameters
    /// 参数
    /// `transition` contains the exact pre-store rollback and post-store commit scopes.
    /// `transition` 包含精确的存储前回滚与存储后提交作用域。
    ///
    /// # Returns
    /// 返回值
    /// The same mutation carrying the transition for the shared commit boundary.
    /// 返回携带迁移、供共享提交边界处理的同一变更。
    fn with_attachment_lease_transition(
        mut self,
        transition: PendingPromptAttachmentLeaseTransition,
    ) -> Self {
        self.attachment_lease_transition = Some(transition);
        self
    }
}

/// Applies a pending prompt queue mutation and runs the shared service side effects.
/// 应用一次待发送提示词队列变更并执行共享服务副作用。
///
/// # Parameters
/// 参数
/// `app` emits the pending prompt queue delta after the retained snapshot is stored.
/// `app` 在保留快照写回后发送待发送提示词队列增量。
/// `state` owns the retained workspace snapshot and workbench projection.
/// `state` 持有保留的工作区快照与工作台投影。
/// `controller_runtime` owns the async persistence bridge used after real mutations.
/// `controller_runtime` 持有真实变更后使用的异步持久化桥。
/// `session_id` identifies the session whose pending prompt queue is mutated.
/// `session_id` 标识待发送提示词队列被变更的会话。
/// `mutate` applies the entry-specific mutation to the cloned workspace snapshot.
/// `mutate` 对克隆出的工作区快照应用入口专属变更。
///
/// # Returns
/// 返回值
/// The entry-specific service result after storage, delta emission, and optional persistence.
/// 返回完成存储、增量发送与可选持久化后的入口专属服务结果。
fn apply_pending_prompt_queue_service_mutation<T, Mutation>(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    controller_runtime: &ControllerRuntimeHandle,
    session_id: SessionId,
    mutate: Mutation,
) -> Result<T, String>
where
    Mutation: FnOnce(
        &mut WorkspaceSnapshot,
        SessionId,
    ) -> Result<PendingPromptQueueServiceMutation<T>, String>,
{
    let mutation = apply_pending_prompt_queue_snapshot_mutation(app, state, session_id, mutate)?;
    if let Some(persist_reason) = mutation.persist_reason {
        spawn_pending_prompt_queue_persist_task(
            state,
            controller_runtime,
            session_id,
            persist_reason,
        )?;
    }
    Ok(mutation.result)
}

/// Applies a pending prompt queue mutation to the retained workspace snapshot.
/// 将一次待发送提示词队列变更应用到保留工作区快照。
///
/// # Parameters
/// 参数
/// `app` emits the pending prompt queue delta after the retained snapshot is stored.
/// `app` 在保留快照写回后发送待发送提示词队列增量。
/// `state` owns the retained workspace snapshot and workbench projection.
/// `state` 持有保留的工作区快照与工作台投影。
/// `session_id` identifies the session whose pending prompt queue is mutated.
/// `session_id` 标识待发送提示词队列被变更的会话。
/// `mutate` applies the entry-specific mutation to the cloned workspace snapshot.
/// `mutate` 对克隆出的工作区快照应用入口专属变更。
///
/// # Returns
/// 返回值
/// The mutation outcome after storage and delta emission, before optional persistence.
/// 返回完成存储与增量发送后、可选持久化前的变更结果。
fn apply_pending_prompt_queue_snapshot_mutation<T, Mutation>(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    session_id: SessionId,
    mutate: Mutation,
) -> Result<PendingPromptQueueServiceMutation<T>, String>
where
    Mutation: FnOnce(
        &mut WorkspaceSnapshot,
        SessionId,
    ) -> Result<PendingPromptQueueServiceMutation<T>, String>,
{
    let mut mutation =
        mutate_latest_workspace_for_existing_session(state, None, session_id, |workspace_slice| {
            mutate(workspace_slice, session_id)
        })?;
    if let Some(transition) = mutation.attachment_lease_transition.take() {
        commit_pending_prompt_attachment_lease_transition(transition);
    }
    emit_pending_prompts_changed_delta(app, state, session_id)?;
    Ok(mutation)
}

/// Spawns persistence for a pending prompt queue mutation using the latest retained snapshot.
/// 使用最新保留快照为一次待发送提示词队列变更启动持久化。
///
/// # Parameters
/// 参数
/// `state` owns the retained workspace snapshot used as the persistence source.
/// `state` 持有作为持久化来源的保留工作区快照。
/// `controller_runtime` owns the workspace-store runtime bridge.
/// `controller_runtime` 持有工作区存储运行时桥。
/// `session_id` identifies the session whose runtime state should be persisted.
/// `session_id` 标识运行态需要持久化的会话。
/// `persist_reason` labels the asynchronous persistence task.
/// `persist_reason` 标记异步持久化任务。
///
/// # Returns
/// 返回值
/// `Ok(())` after the persistence task has been spawned.
/// 启动持久化任务后返回 `Ok(())`。
fn spawn_pending_prompt_queue_persist_task(
    state: &SharedServiceBridgeState,
    controller_runtime: &ControllerRuntimeHandle,
    session_id: SessionId,
    persist_reason: &'static str,
) -> Result<(), String> {
    // Workspace readiness is checked without cloning any retained session maps.
    // 在不克隆任何保留会话映射的情况下检查工作区就绪状态。
    let workspace_is_ready = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?
        .workspace_snapshot
        .is_some();
    if !workspace_is_ready {
        return Err(workspace_snapshot_not_ready_error());
    }
    // Queue edits own runtime metadata only and must not publish the retained transcript prefix.
    // 队列编辑只拥有运行态元数据，不得发布保留转录前缀。
    spawn_workspace_session_runtime_state_persist_task(
        state.clone(),
        controller_runtime.clone(),
        session_id,
        Some(String::from(persist_reason)),
    );
    Ok(())
}

/// Builds the canonical workspace snapshot readiness error.
/// 构建标准工作区快照就绪错误。
///
/// # Returns
/// 返回值
/// The user-facing service startup error text.
/// 返回面向用户的服务启动错误文本。
fn workspace_snapshot_not_ready_error() -> String {
    String::from(WORKSPACE_SNAPSHOT_NOT_READY_MESSAGE)
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
fn commit_pending_prompt_attachment_lease_transition(
    transition: PendingPromptAttachmentLeaseTransition,
) {
    release_pending_prompt_attachment_lease_scope(
        &transition.workspace_path,
        &transition.committed_release_scope,
        &transition.attachments,
        "commit",
    );
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
    let (controller_runtime, _, _, _, _) = runtime_parts_from_state(&state)?;
    let selected_session_id = selected_session_id_from_state(&state)?;
    apply_pending_prompt_queue_service_mutation(
        &app,
        &state,
        &controller_runtime,
        selected_session_id,
        |workspace_snapshot, selected_session_id| {
            let workspace_path =
                workspace_path_for_session(workspace_snapshot, selected_session_id);
            let composer_scope =
                composer_attachment_draft_scope(&format!("conversation:{selected_session_id}"))?;
            let Some(runtime_state) = workspace_snapshot
                .session_runtime_state
                .get_mut(&selected_session_id)
            else {
                return Err(String::from("当前会话没有可编辑的待发送队列。"));
            };
            let entry = runtime_state
                .ui_state
                .pending_prompt_panel_queue
                .iter()
                .find(|entry| entry.id == entry_id)
                .cloned()
                .ok_or_else(|| String::from("没有找到待编辑的待发送提示词。"))?;
            let queue_scope = pending_prompt_attachment_staging_scope(&entry.id)?;
            // composerScope is acquired before queue removal so the returned attachments remain durable until the WebUI reconciles the replacement draft.
            // composerScope 在移除队列项前获取，确保返回的附件在 WebUI 对账替换草稿前持续有效。
            acquire_staged_attachment_ref(&workspace_path, &composer_scope, &entry.attachments)?;
            let Some(entry) =
                remove_pending_prompt_panel_entry(&mut runtime_state.ui_state, &entry_id)
            else {
                return Err(String::from("没有找到待编辑的待发送提示词。"));
            };
            runtime_state.updated_at = current_unix_millis_text();
            let summary = pending_prompt_summary_from_panel_entry(&entry);
            Ok(
                PendingPromptQueueServiceMutation::persisted(summary, "pending_edit")
                    .with_attachment_lease_transition(PendingPromptAttachmentLeaseTransition {
                        workspace_path,
                        committed_release_scope: queue_scope,
                        attachments: entry.attachments,
                    }),
            )
        },
    )
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
    let (controller_runtime, _, _, _, _) = runtime_parts_from_state(&state)?;
    let selected_session_id = selected_session_id_from_state(&state)?;
    apply_pending_prompt_queue_service_mutation(
        &app,
        &state,
        &controller_runtime,
        selected_session_id,
        |workspace_snapshot, selected_session_id| {
            let workspace_path =
                workspace_path_for_session(workspace_snapshot, selected_session_id);
            let removed = if let Some(runtime_state) = workspace_snapshot
                .session_runtime_state
                .get_mut(&selected_session_id)
            {
                if let Some(entry) =
                    remove_pending_prompt_panel_entry(&mut runtime_state.ui_state, &entry_id)
                {
                    runtime_state.updated_at = current_unix_millis_text();
                    let queue_scope = pending_prompt_attachment_staging_scope(&entry.id)?;
                    Some(PendingPromptAttachmentLeaseTransition {
                        workspace_path,
                        committed_release_scope: queue_scope,
                        attachments: entry.attachments,
                    })
                } else {
                    None
                }
            } else {
                None
            };
            let changed = removed.is_some();
            let mutation = PendingPromptQueueServiceMutation::maybe_persisted(
                changed,
                changed,
                "pending_remove",
            );
            Ok(match removed {
                Some(transition) => mutation.with_attachment_lease_transition(transition),
                None => mutation,
            })
        },
    )
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
    let (controller_runtime, _, _, _, _) = runtime_parts_from_state(&state)?;
    let selected_session_id = conversation_id
        .as_deref()
        .and_then(parse_session_id)
        .map(Ok)
        .unwrap_or_else(|| selected_session_id_from_state(&state))?;
    let drop_target = target_entry_id
        .filter(|entry_id| !entry_id.trim().is_empty())
        .map(PendingPromptPanelDropTarget::Before)
        .unwrap_or(PendingPromptPanelDropTarget::End);
    apply_pending_prompt_queue_service_mutation(
        &app,
        &state,
        &controller_runtime,
        selected_session_id,
        |workspace_snapshot, selected_session_id| {
            workspace_snapshot.selected_session = selected_session_id;
            let did_reorder = if let Some(runtime_state) = workspace_snapshot
                .session_runtime_state
                .get_mut(&selected_session_id)
            {
                if apply_pending_prompt_panel_drop_target(
                    &mut runtime_state.ui_state,
                    &dragged_entry_id,
                    &drop_target,
                ) {
                    runtime_state.updated_at = current_unix_millis_text();
                    true
                } else {
                    false
                }
            } else {
                false
            };
            Ok(PendingPromptQueueServiceMutation::maybe_persisted(
                did_reorder,
                did_reorder,
                "pending_reorder",
            ))
        },
    )
}

#[cfg(test)]
mod tests {
    use std::{
        collections::{BTreeMap, BTreeSet},
        sync::Arc,
    };

    use super::*;
    use crate::{ServiceShellHost, create_service_bridge_state};
    use vulcan_workbench_model::{SessionRunPhase, SessionState, SessionSummary};

    /// Test shell host that prevents unit tests from launching host shell actions.
    /// 防止单元测试启动宿主壳操作的测试宿主壳。
    struct PendingPromptServiceTestShellHost;

    impl ServiceShellHost for PendingPromptServiceTestShellHost {
        /// Rejects file-manager requests issued by accidental test paths.
        /// 拒绝意外测试路径发起的文件管理器请求。
        fn open_path(&self, _path: &str) -> Result<(), String> {
            Err(String::from(
                "pending prompt service tests do not open host paths",
            ))
        }

        /// Rejects browser requests issued by accidental test paths.
        /// 拒绝意外测试路径发起的浏览器请求。
        fn open_url(&self, _url: &str) -> Result<(), String> {
            Err(String::from(
                "pending prompt service tests do not open host URLs",
            ))
        }
    }

    /// Builds shared service state containing one retained workspace session.
    /// 构建包含一个保留工作区会话的共享服务状态。
    ///
    /// # Parameters
    /// 参数
    /// `session_id` is the selected session stored in the retained workspace snapshot.
    /// `session_id` 是写入保留工作区快照的选中会话。
    ///
    /// # Returns
    /// 返回值
    /// A shared service state handle ready for pending prompt helper tests.
    /// 返回可用于待发送提示词 helper 测试的共享服务状态句柄。
    fn service_state_with_workspace_session(session_id: SessionId) -> SharedServiceBridgeState {
        let state = create_service_bridge_state(Arc::new(PendingPromptServiceTestShellHost));
        let mut guard = state.lock().expect("test service state lock must be valid");
        guard.workbench_generation = 0;
        guard.workbench.revision = 0;
        guard.workbench.selected_conversation_id = Some(session_id.to_string());
        guard.workspace_snapshot = Some(WorkspaceSnapshot {
            task_projects: vec![vulcan_workbench_model::TaskProjectSummary::new(
                "project-a",
                "Project A",
                "D:/project-a",
                false,
                0,
                None,
                None,
                vec![session_id],
            )],
            archived_task_projects: Vec::new(),
            sessions: vec![SessionSummary::new(
                session_id,
                Some(String::from("project-a")),
                "待发送提示词测试会话",
                "",
                SessionState::Completed,
                0,
                0,
                "刚刚",
            )],
            archived_sessions: Vec::new(),
            session_messages: BTreeMap::new(),
            archived_session_messages: BTreeMap::new(),
            session_compactions: BTreeMap::new(),
            archived_session_compactions: BTreeMap::new(),
            session_runtime_state: BTreeMap::new(),
            archived_session_runtime_state: BTreeMap::new(),
            session_run_phases: BTreeMap::from([(session_id, SessionRunPhase::Ready)]),
            expanded_task_projects: BTreeSet::new(),
            selected_session: session_id,
            hydrated_session_ids: BTreeSet::new(),
        });
        drop(guard);
        state
    }

    /// Verifies the shared snapshot mutation helper stores the workspace and emits a delta revision.
    /// 验证共享快照变更 helper 会存储工作区并发送增量版本。
    #[test]
    fn pending_prompt_snapshot_mutation_stores_snapshot_and_bumps_delta_revision() {
        let session_id = SessionId::new_v4();
        let state = service_state_with_workspace_session(session_id);
        let app = ServiceEventEmitter::new(Vec::new());

        let mutation = apply_pending_prompt_queue_snapshot_mutation(
            &app,
            &state,
            session_id,
            |workspace_snapshot, session_id| {
                workspace_snapshot.selected_session = session_id;
                Ok(PendingPromptQueueServiceMutation::maybe_persisted(
                    true, false, "unused",
                ))
            },
        )
        .expect("snapshot mutation should store the pending prompt workspace");

        let guard = state.lock().expect("test service state lock must be valid");
        assert!(mutation.result);
        assert!(mutation.persist_reason.is_none());
        assert_eq!(
            session_id,
            guard
                .workspace_snapshot
                .as_ref()
                .expect("test workspace snapshot must exist")
                .selected_session
        );
        assert_eq!(1, guard.workbench_generation);
        assert_eq!(1, guard.workbench.revision);
    }
}
