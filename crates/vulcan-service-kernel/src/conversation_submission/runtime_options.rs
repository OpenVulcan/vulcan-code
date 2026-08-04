//! Runtime option preparation for immediate conversation submissions.
//! 即时会话提交的运行选项准备。

use vulcan_provider_config::config::ReasoningEffortLevel;
use vulcan_workbench_model::{PendingSessionPromptSource, SessionId, SessionRuntimeStateEntry};
use vulcan_workspace_store::{
    data_plane::ControllerRuntimeHandle, workspace_store::WorkspaceSnapshot,
};

use crate::{
    current_unix_millis_text, session_goal_lifecycle::ensure_session_can_enter_plan_mode,
    workspace_persistence_versions::parse_reasoning_effort,
};

/// Raw request controls required to prepare one immediate submission turn.
/// 准备一次即时提交回合所需的原始请求控制项。
pub(crate) struct SubmissionRuntimeOptionsInput {
    /// Optional execution-mode wire value; absence selects build mode.
    /// 可选执行模式传输值；缺省时选择构建模式。
    pub(crate) execution_mode: Option<String>,
    /// Optional reasoning-effort wire value for this turn.
    /// 本回合可选推理强度传输值。
    pub(crate) reasoning_effort: Option<String>,
}

/// Strongly typed runtime controls consumed by one shared prompt turn.
/// 单个共享提示词回合消费的强类型运行控制项。
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct SubmissionRuntimeOptions {
    /// Effective execution-mode label forwarded to execution preparation.
    /// 转发给执行准备阶段的生效执行模式标签。
    pub(crate) execution_mode: String,
    /// Parsed reasoning effort for this turn, when the wire value is recognized.
    /// 传输值可识别时得到的本回合推理强度。
    pub(crate) reasoning_effort: Option<ReasoningEffortLevel>,
}

impl SubmissionRuntimeOptions {
    /// Returns whether the prepared turn requests Plan execution semantics.
    /// 返回准备后的回合是否请求计划执行语义。
    ///
    /// # Returns
    /// 返回值
    /// `true` only for the case-insensitive `plan` execution-mode label.
    /// 仅当执行模式标签大小写不敏感地等于 `plan` 时返回 `true`。
    fn requests_plan_mode(&self) -> bool {
        self.execution_mode.eq_ignore_ascii_case("plan")
    }
}

/// Prepares one turn's runtime controls and commits authorized Plan state to the session.
/// 准备单回合运行控制项，并将已授权的计划状态提交到会话。
///
/// # Parameters
/// 参数
/// `controller_runtime` owns the goal store used by Plan-mode mutual-exclusion checks.
/// `controller_runtime` 持有计划模式互斥检查所用的目标存储。
/// `workspace_snapshot` owns the authoritative session configuration and persistent work mode.
/// `workspace_snapshot` 持有权威会话配置与持久工作模式。
/// `session_id` selects the materialized session receiving these controls.
/// `session_id` 选择接收这些控制项的已物化会话。
/// `input` contains request-scoped wire controls consumed exactly once.
/// `input` 包含只消费一次的请求级传输控制项。
///
/// # Returns
/// 返回值
/// Strongly typed turn controls after parsing and any required Plan authorization.
/// 返回解析并完成必要计划授权后的强类型回合控制项。
pub(crate) async fn prepare_submission_runtime_options(
    controller_runtime: &ControllerRuntimeHandle,
    workspace_snapshot: &mut WorkspaceSnapshot,
    session_id: SessionId,
    input: SubmissionRuntimeOptionsInput,
) -> Result<SubmissionRuntimeOptions, String> {
    let options = resolve_submission_runtime_options(input);
    if options.requests_plan_mode() {
        // Goal exclusion is the sole authorization boundary; persistent Plan state is written only after it succeeds.
        // 目标互斥是唯一授权边界；仅在检查成功后写入持久计划状态。
        ensure_session_can_enter_plan_mode(controller_runtime, session_id).await?;
        apply_authorized_plan_mode(workspace_snapshot, session_id);
    }
    if let Some(reasoning_effort) = options.reasoning_effort {
        apply_submission_reasoning_effort(workspace_snapshot, session_id, reasoning_effort);
    }
    Ok(options)
}

/// Commits the submitted reasoning effort to the exact materialized session.
/// 将提交的思考档位写入精确的已物化会话。
///
/// # Parameters
/// 参数
/// `workspace_snapshot` owns the target session runtime state.
/// `workspace_snapshot` 持有目标会话运行时状态。
/// `session_id` selects the session receiving the reasoning choice.
/// `session_id` 选择接收思考档位的会话。
/// `reasoning_effort` is the validated provider-neutral reasoning value.
/// `reasoning_effort` 是已验证的供应商中立思考档位。
///
/// # Returns
/// 返回值
/// This function returns no value.
/// 此函数没有返回值。
fn apply_submission_reasoning_effort(
    workspace_snapshot: &mut WorkspaceSnapshot,
    session_id: SessionId,
    reasoning_effort: ReasoningEffortLevel,
) {
    let updated_at = current_unix_millis_text();
    let runtime_state = workspace_snapshot
        .session_runtime_state
        .entry(session_id)
        .or_insert_with(|| SessionRuntimeStateEntry {
            updated_at: updated_at.clone(),
            ..SessionRuntimeStateEntry::default()
        });
    runtime_state.active_reasoning_effort = Some(reasoning_effort);
    runtime_state.active_reasoning_effort_explicit = true;
    runtime_state.updated_at = updated_at;
}

/// Resolves raw submission controls without mutating session runtime state.
/// 在不修改会话运行态的前提下解析原始提交控制项。
///
/// # Parameters
/// 参数
/// `input` contains request-scoped wire controls.
/// `input` 包含请求级传输控制项。
///
/// # Returns
/// 返回值
/// Parsed turn controls with stable build-mode defaults.
/// 返回带有稳定构建模式默认值的已解析回合控制项。
fn resolve_submission_runtime_options(
    input: SubmissionRuntimeOptionsInput,
) -> SubmissionRuntimeOptions {
    SubmissionRuntimeOptions {
        execution_mode: input
            .execution_mode
            .unwrap_or_else(|| String::from("build")),
        reasoning_effort: input
            .reasoning_effort
            .as_deref()
            .and_then(parse_reasoning_effort),
    }
}

/// Resolves reasoning effort for a queued prompt at its actual dispatch boundary.
/// 在排队提示词的实际分发边界解析推理强度。
///
/// # Parameters
/// 参数
/// `source` identifies whether the queue entry is user-authored or host-created.
/// `source` 标识队列项由用户编写还是由宿主创建。
/// `captured_reasoning_effort` is the immutable control captured for a user-authored queue entry.
/// `captured_reasoning_effort` 是为用户编写队列项捕获的不可变控制项。
/// `active_reasoning_effort` is the current session/provider reasoning selection.
/// `active_reasoning_effort` 是当前会话与 Provider 的推理强度选择。
///
/// # Returns
/// 返回值
/// The reasoning effort that must be forwarded into the queued turn.
/// 返回必须转发到排队回合的推理强度。
pub(crate) fn effective_pending_prompt_reasoning_effort(
    source: PendingSessionPromptSource,
    captured_reasoning_effort: ReasoningEffortLevel,
    active_reasoning_effort: Option<ReasoningEffortLevel>,
) -> Option<ReasoningEffortLevel> {
    if source == PendingSessionPromptSource::GoalContinuation {
        active_reasoning_effort
    } else {
        Some(captured_reasoning_effort)
    }
}

/// Commits Plan mode after the caller has completed the goal mutual-exclusion check.
/// 在调用方完成目标互斥检查后提交计划模式。
///
/// # Parameters
/// 参数
/// `workspace_snapshot` owns the target session runtime state.
/// `workspace_snapshot` 持有目标会话运行态。
/// `session_id` selects the session entering Plan mode.
/// `session_id` 选择进入计划模式的会话。
///
/// # Returns
/// 返回值
/// This function returns no value.
/// 此函数没有返回值。
fn apply_authorized_plan_mode(workspace_snapshot: &mut WorkspaceSnapshot, session_id: SessionId) {
    let updated_at = current_unix_millis_text();
    let runtime_state = workspace_snapshot
        .session_runtime_state
        .entry(session_id)
        .or_insert_with(|| SessionRuntimeStateEntry {
            updated_at: updated_at.clone(),
            ..SessionRuntimeStateEntry::default()
        });
    runtime_state.active_work_mode = Some(String::from("plan"));
    runtime_state.updated_at = updated_at;
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet};

    use super::*;

    /// Builds an empty workspace snapshot with one explicit selection anchor.
    /// 构建带有一个显式选择锚点的空工作区快照。
    ///
    /// # Parameters
    /// 参数
    /// `selected_session` is the stable selection identifier required by the snapshot model.
    /// `selected_session` 是快照模型要求的稳定选择标识。
    ///
    /// # Returns
    /// 返回值
    /// A structurally complete workspace snapshot with no sessions or runtime payloads.
    /// 返回结构完整且不含会话或运行载荷的工作区快照。
    fn empty_workspace_snapshot(selected_session: SessionId) -> WorkspaceSnapshot {
        WorkspaceSnapshot {
            task_projects: Vec::new(),
            archived_task_projects: Vec::new(),
            sessions: Vec::new(),
            archived_sessions: Vec::new(),
            session_messages: BTreeMap::new(),
            archived_session_messages: BTreeMap::new(),
            session_compactions: BTreeMap::new(),
            archived_session_compactions: BTreeMap::new(),
            session_runtime_state: BTreeMap::new(),
            archived_session_runtime_state: BTreeMap::new(),
            session_run_phases: BTreeMap::new(),
            expanded_task_projects: BTreeSet::new(),
            selected_session,
            hydrated_session_ids: BTreeSet::new(),
        }
    }

    /// Verifies raw request controls produce typed runtime options.
    /// 验证原始请求控制项会生成强类型运行选项。
    #[test]
    fn request_controls_produce_typed_turn_options() {
        let options = resolve_submission_runtime_options(SubmissionRuntimeOptionsInput {
            execution_mode: Some(String::from("plan")),
            reasoning_effort: Some(String::from("HIGH")),
        });

        assert!(options.requests_plan_mode());
        assert_eq!(options.reasoning_effort, Some(ReasoningEffortLevel::High));
    }

    /// Verifies authorized Plan application creates the exact persistent session marker.
    /// 验证已授权计划应用会创建精确的持久会话标记。
    #[test]
    fn authorized_plan_mode_updates_only_the_target_session() {
        let target_session_id = SessionId::new_v4();
        let unrelated_session_id = SessionId::new_v4();
        let mut workspace_snapshot = empty_workspace_snapshot(target_session_id);
        workspace_snapshot.session_runtime_state.insert(
            unrelated_session_id,
            SessionRuntimeStateEntry {
                active_work_mode: Some(String::from("build")),
                ..SessionRuntimeStateEntry::default()
            },
        );

        apply_authorized_plan_mode(&mut workspace_snapshot, target_session_id);

        assert_eq!(
            workspace_snapshot
                .session_runtime_state
                .get(&target_session_id)
                .and_then(|runtime_state| runtime_state.active_work_mode.as_deref()),
            Some("plan")
        );
        assert_eq!(
            workspace_snapshot
                .session_runtime_state
                .get(&unrelated_session_id)
                .and_then(|runtime_state| runtime_state.active_work_mode.as_deref()),
            Some("build")
        );
    }

    /// Verifies submission reasoning updates only the newly materialized target session.
    /// 验证提交的思考档位只更新新物化的目标会话。
    #[test]
    fn submission_reasoning_updates_only_the_target_session() {
        let target_session_id = SessionId::new_v4();
        let unrelated_session_id = SessionId::new_v4();
        let mut workspace_snapshot = empty_workspace_snapshot(target_session_id);
        workspace_snapshot.session_runtime_state.insert(
            unrelated_session_id,
            SessionRuntimeStateEntry {
                active_reasoning_effort: Some(ReasoningEffortLevel::Low),
                ..SessionRuntimeStateEntry::default()
            },
        );

        apply_submission_reasoning_effort(
            &mut workspace_snapshot,
            target_session_id,
            ReasoningEffortLevel::Adaptive,
        );

        assert_eq!(
            workspace_snapshot
                .session_runtime_state
                .get(&target_session_id)
                .and_then(|runtime_state| runtime_state.active_reasoning_effort),
            Some(ReasoningEffortLevel::Adaptive)
        );
        assert_eq!(
            workspace_snapshot
                .session_runtime_state
                .get(&unrelated_session_id)
                .and_then(|runtime_state| runtime_state.active_reasoning_effort),
            Some(ReasoningEffortLevel::Low)
        );
    }
}
