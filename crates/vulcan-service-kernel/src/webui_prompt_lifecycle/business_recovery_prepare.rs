//! Original main-business recovery through the ordinary reserved preparation pipeline.
//! 通过常规预留准备链恢复原主业务。

use super::pending_prepare::{ExactPendingPromptDispatch, prepare_webui_pending_prompt};
use super::*;
use crate::conversation_submission::exact_v1_dispatch_runtime_options;
use crate::session_goal_lifecycle::registered_session_v1_handle_from_state;
use crate::session_runtime_registry_v1::SessionHandle;
use vulcan_session_core::v1::business_hook::BusinessHookMetadata;
use vulcan_session_core::v1::collaboration::{
    CollaborationQuestionExecutionState, CollaborationRunControl,
};
use vulcan_session_core::v1::document::TurnOrigin;
use vulcan_workbench_model::PendingSessionPromptEntry;

#[cfg(test)]
mod tests;

/// Original published Hook and its exact storage frontier for one automatic recovery admission.
/// 一次自动恢复准入的原已发布 Hook 及其精确存储前沿。
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MainBusinessRecoveryAuthority {
    /// Revision prevents a later completed physical execution from reusing an old recovery request.
    /// 修订阻止后来的已完成物理执行复用旧恢复请求。
    revision: u64,
    /// Original immutable Hook identity and ordered canonical human observations.
    /// 原不可变 Hook 身份及有序规范人类观察。
    hook: BusinessHookMetadata,
}

/// Explicit startup choice between preserving published ownership and first human preparation.
/// 明确区分保留已发布归属与首次人类准备的启动选择。
pub(crate) enum MainBusinessStartupAuthority {
    /// Original published Hook remains the only valid business identity.
    /// 原已发布 Hook 仍是唯一有效业务身份。
    Recovery(MainBusinessRecoveryAuthority),
    /// Original human input has not yet acquired any business execution ownership.
    /// 原人类输入尚未取得任何业务执行归属。
    InitialHuman(vulcan_session_core::v1::business_hook::InitialHumanBusiness),
}

/// Selects startup work from one full durable frontier, never from a missing-Hook error fallback.
/// 从一次完整可靠前沿选择启动工作，绝不以缺失 Hook 错误作兜底分支。
/// `handle` owns the indexed main session; busy or intentionally stopped owners return none.
/// `handle` 持有索引主会话；繁忙或主动停止的所有者返回无候选。
pub(crate) fn main_business_startup_authority(
    handle: &SessionHandle,
) -> Result<Option<MainBusinessStartupAuthority>, String> {
    handle
        .read_full_durable(|document| {
            // Physical and intentional lifecycle owners take precedence over automatic startup.
            // 物理执行及主动生命周期所有者优先于自动启动。
            let Some(turn) = document.active_turn().filter(|turn| !turn.closed) else {
                return Ok(None);
            };
            if document.metadata.archived_at_ms.is_some()
                || document.metadata.child_execution_profile.is_some()
                || turn.origin == TurnOrigin::QuestionExecution
                || document.active_api().is_some()
                || document.collaboration_run_control() != CollaborationRunControl::Running
                || document
                    .collaboration_state()
                    .question_executions
                    .values()
                    .any(|execution| {
                        execution.state == CollaborationQuestionExecutionState::Running
                    })
            {
                return Ok(None);
            }
            if turn.origin == TurnOrigin::UserRoot
                && !document
                    .metadata
                    .business_hook
                    .binding
                    .as_ref()
                    .is_some_and(|binding| binding.turn_id == turn.turn_id)
            {
                return document
                    .main_business_initial_human_input()
                    .map(|source| Some(MainBusinessStartupAuthority::InitialHuman(source)))
                    .map_err(|error| error.to_string());
            }
            // Non-human first sources retain their own source-specific admission contract.
            // 非人类首次来源保留各自独立的来源准入契约。
            let hook = document
                .main_business_recovery_state()
                .map_err(|error| error.to_string())?;
            Ok(Some(MainBusinessStartupAuthority::Recovery(
                MainBusinessRecoveryAuthority {
                    revision: document.durable_revision(),
                    hook,
                },
            )))
        })
        .map_err(|error| error.to_string())?
}

/// Reads full published history from `handle` and selects only idle, running main business.
/// 从 `handle` 读取完整已发布历史，仅选择空闲且允许运行的主业务。
/// Returns no candidate for another live owner; missing authority or uncertain effects are explicit errors.
/// 其他活动所有者返回无候选；权威缺失或效果不明明确报错。
pub(crate) fn main_business_recovery_authority(
    handle: &SessionHandle,
) -> Result<Option<MainBusinessRecoveryAuthority>, String> {
    handle
        .read_full_durable(|document| {
            // A closed historical Hook is evidence, never an automatic execution request.
            // 已闭合历史 Hook 是证据，绝不构成自动执行请求。
            let Some(turn) = document.active_turn().filter(|turn| !turn.closed) else {
                return Ok(None);
            };
            if document.metadata.archived_at_ms.is_some()
                || document.metadata.child_execution_profile.is_some()
                || turn.origin == TurnOrigin::QuestionExecution
                || document.active_api().is_some()
                || document.collaboration_run_control() != CollaborationRunControl::Running
                || document
                    .collaboration_state()
                    .question_executions
                    .values()
                    .any(|execution| {
                        execution.state == CollaborationQuestionExecutionState::Running
                    })
            {
                return Ok(None);
            }
            // Reject uncertain tool effects before exposing original Hook authority.
            // 暴露原 Hook 权威之前拒绝不确定工具效果。
            let hook = document
                .main_business_recovery_state()
                .map_err(|error| error.to_string())?;
            Ok(Some(MainBusinessRecoveryAuthority {
                revision: document.durable_revision(),
                hook,
            }))
        })
        .map_err(|error| error.to_string())?
}

/// Rechecks `authority` immediately before restoring its Hook under the shared submission reservation.
/// 在共享提交预留内恢复 Hook 之前立即重验 `authority`。
/// Returns an error when any published business frontier changed; callers must select fresh authority.
/// 已发布业务前沿变化时返回错误；调用方必须重新筛选权威。
pub(super) fn validate_main_business_recovery(
    handle: &SessionHandle,
    authority: &MainBusinessRecoveryAuthority,
) -> Result<(), String> {
    if main_business_recovery_authority(handle)?.as_ref() != Some(authority) {
        return Err(String::from(
            "Main business recovery authority changed before Hook preparation.",
        ));
    }
    Ok(())
}

/// Restores the original Hook from `authority`; no user input or replacement Hook identity is generated.
/// 从 `authority` 恢复原 Hook；不生成人类输入或替代 Hook 身份。
/// `runtime` owns the Hook map; returns explicit conflicts with an existing different owner.
/// `runtime` 持有 Hook 映射；与已有不同所有者冲突时明确返回。
pub(super) fn restore_main_business_recovery_hook(
    handle: &SessionHandle,
    runtime: &AgentRuntimeBridge,
    authority: &MainBusinessRecoveryAuthority,
) -> Result<(), String> {
    validate_main_business_recovery(handle, authority)?;
    runtime
        .conversation_hook_runtime
        .lock()
        .map_err(|error| format!("Failed to lock business Hook runtime: {error}"))?
        .restore_business_hook_state(&authority.hook)?;
    Ok(())
}

/// Prepares an original business recovery with target-owned hydration and the existing admission pipeline.
/// 使用目标所属水合及现有准入链准备原业务恢复。
/// Service arguments retain normal ownership; returns whether asynchronous preparation was started.
/// 服务参数保留常规归属；返回是否已开始异步准备。
pub(crate) async fn maybe_prepare_webui_business_recovery(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    controller_runtime: &HostRuntimeHandle,
    agent_runtime: &AgentRuntimeBridge,
    session_id: SessionId,
    authority: MainBusinessRecoveryAuthority,
) -> Result<bool, String> {
    // Use the canonical registered owner rather than the currently selected conversation.
    // 使用规范已登记所有者，而非当前选择的会话。
    let handle = registered_session_v1_handle_from_state(state, session_id)?;
    validate_main_business_recovery(&handle, &authority)?;
    // Hydrate only this target's authoritative project and conversation state.
    // 仅水合此目标的权威项目及会话状态。
    let workspace = crate::session_hydration::load_workspace_for_submission(
        state,
        controller_runtime.clone(),
        Some(session_id),
    )
    .await?;
    // The adapter is excluded from canonical user messages and Hook observations.
    // 此适配对象不进入规范人类消息或 Hook 观察。
    let entry = PendingSessionPromptEntry {
        id: format!("webui-v1-business-recovery-{}", authority.revision),
        prompt: String::from(
            "Continue the original open task from its durable conversation history.",
        ),
        hidden_model_guidance: None,
        preview: String::from("Continue the original open task."),
        attachments: Vec::new(),
        model_reference: None,
        runtime_options: exact_v1_dispatch_runtime_options(&handle)?,
        source: PendingSessionPromptSource::AgentContinuation,
        task_id: None,
        panel_entry_id: None,
        created_at: current_unix_millis_text(),
    };
    prepare_webui_pending_prompt(
        app,
        state,
        controller_runtime,
        agent_runtime,
        session_id,
        ExactPendingPromptDispatch::BusinessRecovery {
            authority,
            workspace,
            entry,
        },
    )
    .await
}
