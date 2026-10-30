//! Bounded input/configuration re-preparation across exact-owner stream restarts.
//! 跨精确所有者流重启的有界输入/配置重准备。

use super::*;
use crate::session_goal_lifecycle::registered_session_v1_handle_from_state;
use vulcan_agent_runtime::agent_runtime::events::PreparedPhysicalApi;
use vulcan_session_core::v1::ApiCallId;
use vulcan_session_core::v1::collaboration::{
    CollaborationExecutionId, CollaborationQuestionExecutionRecord,
    CollaborationQuestionExecutionState,
};

/// Maximum automatic input/configuration restarts before the next durable API permit must succeed.
/// 下一次可靠API许可成功之前允许的最大自动输入/配置重启次数。
const MAX_CONFIGURATION_RESTARTS: u8 = 3;

/// Request-local restart state moved with the existing kernel generation and turn owner, never persisted into prompt media.
/// 随既有内核代际及回合所有者移动的请求局部重启状态，绝不持久化进提示词媒体。
#[derive(Clone, Default, Debug, PartialEq, Eq)]
pub(super) struct StreamRestartState {
    /// Existing auto-compaction guard, independent of whether configuration retries have been consumed.
    /// 既有自动压缩守卫，独立于配置重试是否已被消耗。
    pub auto_compaction_already_attempted: bool,
    /// Consecutive input/configuration restarts since the last successful durable API permit.
    /// 最近一次成功可靠API许可之后的连续输入/配置重启次数。
    configuration_restarts: u8,
    /// Exact auxiliary owner retained across preparation failures and physical stream replacement.
    /// 跨准备失败及物理流替换保留的精确辅助所有者。
    question_execution: Option<QuestionStreamExecution>,
}

/// Immutable logical question owner with the last durable physical request it released.
/// 不可变逻辑答疑所有者及其最近放行的可靠物理请求。
#[derive(Clone, Debug, PartialEq, Eq)]
struct QuestionStreamExecution {
    /// Stable execution identity from the admitted target record.
    /// 来自已准入目标记录的稳定执行身份。
    execution_id: CollaborationExecutionId,
    /// Original auxiliary or retained parent turn; configuration changes never rebind it.
    /// 原辅助或保留父回合；配置变化绝不重新绑定它。
    turn_id: TurnId,
    /// Last actual V1 permit, absent before any physical request was released.
    /// 最近实际 V1 许可；尚未放行任何物理请求时为空。
    api_call_id: Option<ApiCallId>,
}

impl StreamRestartState {
    /// Builds restart state from the running target execution; rejects a completed or unbound record.
    /// 从运行中的目标执行构建重启状态；拒绝已完成或未绑定记录。
    /// `execution` supplies the exact identity and turn; no current-session or latest-question inference occurs.
    /// `execution` 提供精确身份与回合；不从当前会话或最近问题推测。
    pub(super) fn for_question_execution(
        execution: &CollaborationQuestionExecutionRecord,
    ) -> Result<Self, String> {
        use vulcan_session_core::v1::collaboration::CollaborationExecutionPurpose;
        // The purpose dictates one exclusive turn binding in the current contract.
        // 当前契约规定用途决定唯一且互斥的回合绑定。
        let turn_id =
            match (
                execution.purpose,
                execution.parent_turn_id,
                execution.execution_turn_id,
            ) {
                (CollaborationExecutionPurpose::InTurnQuestion, Some(turn_id), None)
                | (CollaborationExecutionPurpose::NaturallyIdleQuestion, None, Some(turn_id))
                    if execution.state == CollaborationQuestionExecutionState::Running =>
                {
                    turn_id
                }
                _ => return Err(
                    "A question stream requires one running execution with an exact turn binding."
                        .into(),
                ),
            };
        Ok(Self {
            question_execution: Some(QuestionStreamExecution {
                execution_id: execution.execution_id.clone(),
                turn_id,
                api_call_id: None,
            }),
            ..Self::default()
        })
    }

    /// Returns the failure scope captured from this stream's admitted owner and actual permits.
    /// 返回从本流已准入所有者及实际许可捕获的失败范围。
    /// The returned owned scope can survive the stream future being dropped during registration failure.
    /// 返回的自有范围可在登记失败丢弃流 Future 后继续保留。
    pub(super) fn terminal_scope(&self) -> TerminalSettlementScope {
        match &self.question_execution {
            Some(execution) => TerminalSettlementScope::QuestionExecution {
                execution_id: execution.execution_id.clone(),
                api_call_id: execution.api_call_id,
            },
            None => TerminalSettlementScope::BusinessTurn,
        }
    }

    /// Checks a released QA permit against its original logical owner before transport sees it.
    /// 在传输层看到答疑许可前，将其与原逻辑所有者核对。
    /// `state` supplies the registered document; `result` is the actual preparation outcome for `session_id`.
    /// `state` 提供已登记文档；`result` 是 `session_id` 的实际准备结果。
    /// Returns an explicit mismatch error; business streams and rejected candidates need no additional check.
    /// 不匹配时返回明确错误；业务流及已拒绝候选无需额外核对。
    pub(super) fn validate_prepared_owner(
        &self,
        state: &SharedServiceBridgeState,
        session_id: SessionId,
        result: &Result<PreparedPhysicalApi, String>,
    ) -> Result<(), String> {
        let Some(execution) = &self.question_execution else {
            return Ok(());
        };
        // A dispatchable QA request must carry a committed session API identity.
        // 可派发答疑请求必须携带已提交会话 API 身份。
        let expected_api = match result {
            Ok(PreparedPhysicalApi::SessionV1 { core_start, .. }) => core_start.api_call_id,
            Ok(PreparedPhysicalApi::ExcludedNonSession { .. }) => {
                return Err(
                    "A question execution cannot release a non-session provider request.".into(),
                );
            }
            Ok(PreparedPhysicalApi::RequestConfigurationChanged)
            | Ok(PreparedPhysicalApi::ContextCompactionRequired { .. })
            | Err(_) => return Ok(()),
        };
        // The sole registered owner supplies both the running execution and its just-committed API.
        // 唯一已登记所有者同时提供运行中执行及其刚提交的 API。
        let handle = registered_session_v1_handle_from_state(state, session_id)?;
        handle.read(|document| {
            // Re-read the immutable binding together with its current execution state.
            // 同时重读不可变绑定及当前执行状态。
            let execution_matches = document.collaboration_state().question_executions
                .get(&execution.execution_id).is_some_and(|record| {
                    record.state == CollaborationQuestionExecutionState::Running
                        && record.execution_turn_id.or(record.parent_turn_id) == Some(execution.turn_id)
                });
            // The permit cannot escape into a replacement turn or a business request.
            // 许可不能进入替代回合或业务请求。
            let api_matches = document.active_api().is_some_and(|api| {
                api.api_call_id == expected_api && api.turn_id == execution.turn_id
                    && api.purpose == vulcan_session_core::v1::ApiPurposeKind::QuestionExecution
            });
            if execution_matches && api_matches { Ok(()) } else {
                Err("The prepared question API does not belong to the captured running execution and turn.".into())
            }
        }).map_err(|error| error.to_string())?
    }
    /// Reserves one configuration restart in `self`; returns false without changing exhausted state.
    /// 在`self`中预留一次配置重启；额度耗尽时返回false且不修改状态。
    /// This bounds preparation churn, not the number of legitimate model switches across successful API calls.
    /// 此限制约束准备抖动，而非跨成功API调用的合法模型切换次数。
    pub(super) fn try_configuration_restart(&mut self) -> bool {
        if self.configuration_restarts >= MAX_CONFIGURATION_RESTARTS {
            return false;
        }
        self.configuration_restarts += 1;
        true
    }

    /// Observes the actual `result`; only a durable V1 permit resets configuration churn, never a stale or failed candidate.
    /// 观察实际`result`；只有可靠V1许可重置配置抖动，陈旧或失败候选绝不重置。
    /// The same permit re-arms automatic compaction: it proves the compacted request passed every
    /// budget gate, so a later compaction line in a long turn is new growth, not a retry loop.
    /// 同一许可也重新启用自动压缩：它证明压缩后的请求已通过所有预算闸门，因此长回合中之后
    /// 再到压缩线属于新增长，而非重试循环。
    pub(super) fn observe_preparation(&mut self, result: &Result<PreparedPhysicalApi, String>) {
        if let Ok(PreparedPhysicalApi::SessionV1 { core_start, .. }) = result {
            self.configuration_restarts = 0;
            self.auto_compaction_already_attempted = false;
            if let Some(execution) = &mut self.question_execution {
                execution.api_call_id = Some(core_start.api_call_id);
            }
        }
    }

    /// Returns this state with the existing compaction flag set to `attempted`, retaining all configuration restart debt.
    /// 返回将既有压缩标志设为`attempted`的本状态，保留全部配置重启消耗。
    pub(super) fn with_auto_compaction_attempted(mut self, attempted: bool) -> Self {
        self.auto_compaction_already_attempted = attempted;
        self
    }
}

/// Terminalizes exhausted re-preparation only for `active_stream_task_id` in `session_id` through existing failure owners.
/// 通过既有失败所有者，仅为`session_id`中的`active_stream_task_id`终結耗尽的重准备。
/// `app` publishes the result and `task_state` supplies exact ownership; returns after durable failure finalization when still owned.
/// `app`发布结果，`task_state`提供精确所有权；仍持有所有权时，在可靠失败终结后返回。
/// `terminal_scope` retains the admitted business or question owner across exhausted re-preparation.
/// `terminal_scope` 在重准备耗尽后保留已准入业务或答疑所有者。
pub(super) async fn commit_configuration_reprepare_limit(
    app: &ServiceEventEmitter,
    task_state: &SharedServiceBridgeState,
    session_id: SessionId,
    active_stream_task_id: Uuid,
    terminal_scope: TerminalSettlementScope,
) {
    // Core rejects candidates for either newer inputs or configuration; neither is an upload, credential or cancellation failure.
    // Core可能因较新输入或配置拒绝候选；两者都不是上传、凭据或取消失败。
    let error = format!(
        "Request inputs or configuration kept changing before the next durable API permit. Automatic re-preparation stopped after {MAX_CONFIGURATION_RESTARTS} restarts; retry after preparation inputs stabilize."
    );
    // The existing commit gate prevents an old stream from terminalizing a replacement owner.
    // 既有提交闸门阻止旧流终结替代所有者。
    let failure_commit = commit_prepared_stream_failure(
        app,
        task_state,
        session_id,
        active_stream_task_id,
        "provider request preparation restart limit reached",
    );
    if failure_commit == PreparedStreamFailureCommit::Committed {
        finalize_failed_execution_stream_retry_lifecycle(
            task_state.clone(),
            session_id,
            SessionTerminalPersistenceOwner::StreamTask(active_stream_task_id),
            "provider_request_preparation_unstable",
            Some(error),
            terminal_scope,
        )
        .await;
    }
}

#[cfg(test)]
mod tests;
