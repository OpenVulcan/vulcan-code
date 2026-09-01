//! Bounded input/configuration re-preparation across exact-owner stream restarts.
//! 跨精确所有者流重启的有界输入/配置重准备。

use super::*;
use vulcan_agent_runtime::agent_runtime::events::PreparedPhysicalApi;

/// Maximum automatic input/configuration restarts before the next durable API permit must succeed.
/// 下一次可靠API许可成功之前允许的最大自动输入/配置重启次数。
const MAX_CONFIGURATION_RESTARTS: u8 = 3;

/// Request-local restart state moved with the existing kernel generation and turn owner, never persisted into prompt media.
/// 随既有内核代际及回合所有者移动的请求局部重启状态，绝不持久化进提示词媒体。
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
pub(super) struct StreamRestartState {
    /// Existing auto-compaction guard, independent of whether configuration retries have been consumed.
    /// 既有自动压缩守卫，独立于配置重试是否已被消耗。
    pub auto_compaction_already_attempted: bool,
    /// Consecutive input/configuration restarts since the last successful durable API permit.
    /// 最近一次成功可靠API许可之后的连续输入/配置重启次数。
    configuration_restarts: u8,
}

impl StreamRestartState {
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
    /// Returns no value and leaves the separate automatic-compaction guard unchanged.
    /// 不返回值，保持独立自动压缩守卫不变。
    pub(super) fn observe_preparation(&mut self, result: &Result<PreparedPhysicalApi, String>) {
        if matches!(result, Ok(PreparedPhysicalApi::SessionV1 { .. })) {
            self.configuration_restarts = 0;
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
pub(super) async fn commit_configuration_reprepare_limit(
    app: &ServiceEventEmitter,
    task_state: &SharedServiceBridgeState,
    session_id: SessionId,
    active_stream_task_id: Uuid,
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
        )
        .await;
    }
}

#[cfg(test)]
mod tests;
