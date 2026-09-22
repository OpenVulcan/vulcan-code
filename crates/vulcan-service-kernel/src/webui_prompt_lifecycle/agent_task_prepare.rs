//! Exact preparation authority for durably claimed agent tasks.
//! 已可靠认领代理任务的精确准备权威。

use super::*;
use crate::session_runtime_registry_v1::SessionHandle;
use vulcan_session_core::v1::collaboration::{
    CollaborationDeliveryId, CollaborationExecutionId, CollaborationRunControl,
    CollaborationTaskMessageEffectKind,
};
use vulcan_session_core::v1::document::TurnOrigin;
use vulcan_session_core::v1::event::{ApiPurposeKind, EventBody, UserInputSource};
use vulcan_session_core::v1::ids::{PendingInputId, Seq, TurnId};

/// Publishes only the admitted target slice and starts its normal asynchronous profile preparation.
/// 只发布已准入目标切片，并启动其正常异步画像准备。
/// `workspace`, `start` and `source` belong to the registered preparation; failures settle that exact owner.
/// `workspace`、`start` 与 `source` 属于已登记准备；失败时结算该精确所有者。
pub(super) async fn finish_target_prompt_preparation(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    controller_runtime: &HostRuntimeHandle,
    agent_runtime: &AgentRuntimeBridge,
    workspace: WorkspaceSnapshot,
    start: SharedPromptTurnStart,
    source: PendingSessionPromptSource,
) -> Result<bool, String> {
    // Capture the registered owner before any detached projection or preparation can run.
    // 在任何分离投影或准备运行前捕获已登记所有者。
    let session_id = start.session_id;
    let preparation_turn_id = start.preparation_turn_id.clone();
    let Some(generation) =
        capture_webui_prompt_preparation_generation(state, session_id, &preparation_turn_id)?
    else {
        return Ok(false);
    };
    let dispatch = (|| {
        // Derive prechecks from the real source inside the registered owner's failure boundary.
        // 在已登记所有者的失败边界内，按真实来源推导预检。
        let precheck_plan = if source.runs_user_prompt_precheck() {
            webui_shared_prompt_precheck_plan(
                agent_runtime,
                &workspace,
                session_id,
                &workspace_path_for_session(&workspace, session_id),
                start.vmm_precheck_ready,
            )?
        } else {
            SharedPromptPrecheckPlan {
                has_precheck_hooks: false,
                should_skip_precheck: true,
                progress_labels: Vec::new(),
            }
        };

        if start.vmm_runtime_ready {
            refresh_exact_vmm_hook_session_projection(
                &agent_runtime.vmm_hook_runtime,
                &workspace,
                session_id,
            )?;
        }
        {
            // An indexed cold session may not yet be in the retained UI index; add only its exact summary.
            // 已索引冷会话可能尚未进入驻留界面索引；只补入其精确摘要。
            let mut guard = state.lock().map_err(|error| error.to_string())?;
            if !crate::webui_prompt_preparation_owner_is_current(
                &guard,
                session_id,
                generation,
                &preparation_turn_id,
            ) {
                return Ok(false);
            }
            let retained = guard
                .workspace_snapshot
                .as_mut()
                .ok_or_else(|| String::from("Target prompt requires a retained workspace."))?;
            // Later preparation resolves the workspace path from this target's retained project.
            // 后续准备从此目标的驻留项目解析工作区路径。
            let summary = workspace
                .sessions
                .iter()
                .find(|session| session.id == session_id)
                .ok_or_else(|| String::from("Prompt target left the indexed workspace."))?;
            if let Some(project_id) = summary.project_id.as_deref()
                && !retained
                    .task_projects
                    .iter()
                    .any(|project| project.id == project_id)
            {
                let project = workspace
                    .task_projects
                    .iter()
                    .find(|project| project.id == project_id)
                    .ok_or_else(|| String::from("Prompt target has no active indexed project."))?;
                retained.task_projects.push(project.clone());
            }
            if !retained
                .sessions
                .iter()
                .any(|session| session.id == session_id)
            {
                retained.sessions.push(summary.clone());
            }
        }
        if !commit_workspace_snapshot_and_emit_session_phase_delta_if_prompt_owner_current(
            app,
            state,
            workspace,
            session_id,
            SessionRunPhase::Preparing,
            generation,
            &preparation_turn_id,
        )? {
            return Ok(false);
        }
        start_webui_precheck_or_continue(
            app,
            state,
            controller_runtime,
            agent_runtime,
            start,
            precheck_plan,
        )
    })();
    match dispatch {
        Ok(started) => Ok(started),
        Err(error) => {
            mark_webui_prompt_preparation_failed(
                app,
                state,
                agent_runtime,
                session_id,
                &preparation_turn_id,
                &error,
            )
            .await
            .map_err(|settlement| {
                format!(
                    "Target prompt preparation failed: {error}; settlement failed: {settlement}"
                )
            })?;
            Err(error)
        }
    }
}

/// Immutable startup identity read from a published claim and its canonical input.
/// 从已发布认领及其规范输入读取的不可变启动身份。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SessionAgentTaskDispatchAuthority {
    /// Exact first-preparation or original-Hook frontier captured with the task claim.
    /// 与任务认领一起捕获的精确首次准备或原 Hook 前沿。
    pub hook_preparation: super::internal_continuation_prepare::MainBusinessHookPreparation,
    /// Original durable task delivery.
    /// 原始可靠任务投递。
    pub delivery_id: CollaborationDeliveryId,
    /// Logical task execution, independent of physical request attempts.
    /// 独立于物理请求尝试的逻辑任务执行。
    pub execution_id: CollaborationExecutionId,
    /// Target history generation owning the claim.
    /// 持有认领的目标历史代际。
    pub generation: u64,
    /// Exact business turn allocated at claim.
    /// 认领时分配的精确业务回合。
    pub turn_id: TurnId,
    /// Exact input identity and its stream position.
    /// 精确输入身份及其流内位置。
    pub input_id: PendingInputId,
    /// Canonical input position for provider projection.
    /// 用于供应商投影的规范输入位置。
    pub input_seq: Seq,
    /// Already encoded canonical agent message, never a second human input.
    /// 已编码的规范代理消息，绝不作为第二条人类输入。
    pub text: String,
}

use super::internal_continuation_prepare::MainBusinessHookPreparation;

impl SessionAgentTaskDispatchAuthority {
    /// Selects preparation semantics without reclassifying a persisted agent input as human text.
    /// 选择准备语义，不把持久代理输入重新分类为人类文本。
    /// Returns first-task semantics or observation-free continuation for the original published owner.
    /// 返回首次任务语义，或不重复观察的原已发布所有者续跑语义。
    pub(super) fn prompt_source(&self) -> PendingSessionPromptSource {
        match &self.hook_preparation {
            MainBusinessHookPreparation::Initial(_) => PendingSessionPromptSource::AgentTask,
            MainBusinessHookPreparation::Published { .. } => {
                PendingSessionPromptSource::AgentContinuation
            }
        }
    }
}

/// Installs only the exact claim's original Hook or clears its proven unpublished preparation.
/// 只安装精确认领的原 Hook，或清除其已证明未发布的准备。
/// The caller holds submission exclusion; full authority is rechecked before any runtime mutation.
/// 调用方持有提交互斥；任何运行时修改之前重新核验完整权威。
pub(super) fn prepare_agent_task_hook_owner(
    handle: &SessionHandle,
    runtime: &AgentRuntimeBridge,
    authority: &SessionAgentTaskDispatchAuthority,
) -> Result<(), String> {
    if agent_task_dispatch_authority_from_v1_handle(handle, &authority.delivery_id)? != *authority {
        return Err(String::from(
            "Agent task authority changed before Hook restoration.",
        ));
    }
    authority.hook_preparation.install(runtime)
}

/// Reads the unpublished-to-provider startup authority for `delivery_id` from `handle`.
/// 从 `handle` 读取 `delivery_id` 尚未向供应商分发的启动权威。
/// Returns exact persisted identity or an error when stopped, replaced, or already dispatched.
/// 返回精确持久身份；停止、替换或已经分发时返回错误。
pub(crate) fn agent_task_dispatch_authority_from_v1_handle(
    handle: &SessionHandle,
    delivery_id: &CollaborationDeliveryId,
) -> Result<SessionAgentTaskDispatchAuthority, String> {
    handle
        .read_full_durable(|document| {
            if document.metadata.archived_at_ms.is_some()
                || document.collaboration_run_control() != CollaborationRunControl::Running
                || document.memory_revision() != document.durable_revision()
            {
                return Err(String::from(
                    "Agent task requires a running, fully published target.",
                ));
            }
            // The immutable effect is the sole source of execution and input ownership.
            // 不可变效果是执行与输入归属的唯一来源。
            let effect = document
                .collaboration_state()
                .task_message_effects
                .get(delivery_id)
                .ok_or_else(|| String::from("Agent task has no durable claim."))?;
            let CollaborationTaskMessageEffectKind::Claimed {
                execution_id,
                turn_id,
                input_id,
                input_seq,
            } = &effect.outcome
            else {
                return Err(String::from("Agent task delivery is not claimed."));
            };
            // Current ownership must still name the claimed business turn.
            // 当前所有权必须仍指向已认领业务回合。
            let turn = document
                .active_turn()
                .ok_or_else(|| String::from("Agent task has no open business turn."))?;
            if turn.closed
                || turn.origin != TurnOrigin::AgentTask
                || turn.turn_id != *turn_id
                || turn.start_seq != effect.event_seq
            {
                return Err(String::from(
                    "Agent task no longer owns its claimed business turn.",
                ));
            }
            if document.active_api().is_some()
                || document.events().iter().any(|event| {
                    event.turn_id == Some(*turn_id)
                        // Auxiliary history does not prove that the claimed task was executed.
                        // 辅助历史不能证明认领任务已经执行。
                        && matches!(&event.event, EventBody::ApiStarted(started)
                            if !matches!(started.purpose,
                                ApiPurposeKind::Compaction | ApiPurposeKind::QuestionExecution))
                })
            {
                return Err(String::from(
                    "Agent task already owns a physical API attempt.",
                ));
            }
            // Resolve the exact input position; never infer it from tagged text.
            // 解析精确输入位置，绝不根据带标签正文推断。
            let event = document
                .events()
                .iter()
                .find(|event| event.seq == *input_seq)
                .ok_or_else(|| {
                    String::from("Claimed agent input is missing from the resident stream.")
                })?;
            let EventBody::UserInput(input) = &event.event else {
                return Err(String::from("Agent task anchor is not a canonical input."));
            };
            if event.turn_id != Some(*turn_id)
                || input.input_id != *input_id
                || input.input_source != UserInputSource::AgentMessage
            {
                return Err(String::from(
                    "Agent task input differs from its durable claim.",
                ));
            }
            // Select by the actual canonical binding, never by catching a missing-Hook error.
            // 按实际规范绑定选择，绝不通过捕获缺失 Hook 错误选路。
            let hook_preparation = MainBusinessHookPreparation::capture(document)?;
            Ok(SessionAgentTaskDispatchAuthority {
                hook_preparation,
                delivery_id: delivery_id.clone(),
                execution_id: execution_id.clone(),
                generation: document.segment_generation(),
                turn_id: *turn_id,
                input_id: *input_id,
                input_seq: *input_seq,
                text: input.text.clone(),
            })
        })
        .map_err(|error| error.to_string())?
}
