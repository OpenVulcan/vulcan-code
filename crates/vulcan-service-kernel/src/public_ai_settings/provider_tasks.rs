//! Stable-ID status and cancellation for durable provider asynchronous tasks.
//! 可持久化供应商异步任务的稳定标识状态查询与取消。

use serde::Serialize;
use vulcan_provider_adapters::{
    ProviderAsyncTaskPhase, ProviderAsyncTaskRecord, ProviderToolCapabilityKind,
};

use super::public_ai_runtime_and_configuration;
use crate::SharedServiceBridgeState;

/// Non-secret projection of one retained provider task for status views.
/// 面向状态视图的单个保留供应商任务非秘密投影。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProviderTaskSummary {
    /// Stable host-owned task identity used by status and cancellation calls.
    /// 状态查询与取消调用使用的稳定宿主任务标识。
    execution_id: String,
    /// Conversation that started the task, when it was started by a session tool call.
    /// 启动任务的会话；仅在由会话工具调用启动时存在。
    owner_session_id: Option<String>,
    /// Semantic capability frozen with the task target, when the target was frozen.
    /// 随任务目标冻结的语义能力；目标已冻结时存在。
    capability: Option<ProviderToolCapabilityKind>,
    /// Display name of the frozen provider target, when available.
    /// 冻结供应商目标的显示名称；可用时存在。
    target_display_name: Option<String>,
    /// Provider owning the task.
    /// 持有任务的供应商。
    provider_id: String,
    /// Connection whose credential observes the task.
    /// 用于观测任务凭据的连接。
    connection_id: String,
    /// Exact provider tool target identifier.
    /// 精确供应商工具目标标识。
    target_id: String,
    /// Current durable lifecycle phase.
    /// 当前持久生命周期阶段。
    phase: ProviderAsyncTaskPhase,
    /// Whether the phase can no longer change.
    /// 该阶段是否已不会再变化。
    terminal: bool,
    /// Whether the provider declares cancellation for this task.
    /// 供应商是否为该任务声明了取消能力。
    cancellation_supported: bool,
    /// Task creation time in Unix milliseconds.
    /// 任务创建时间，单位为 Unix 毫秒。
    created_at_unix_ms: u64,
    /// Last durable update time in Unix milliseconds.
    /// 最近一次持久更新时间，单位为 Unix 毫秒。
    updated_at_unix_ms: u64,
    /// Frozen lifecycle deadline in Unix milliseconds.
    /// 冻结的生命周期截止时间，单位为 Unix 毫秒。
    deadline_unix_ms: u64,
    /// Consecutive failed observations since the last successful poll.
    /// 自上次成功轮询以来的连续观测失败次数。
    consecutive_observation_failures: u32,
    /// Bounded non-secret diagnostic of the latest failure.
    /// 最近一次失败的有界非秘密诊断。
    error: Option<String>,
    /// Whether this process still holds the completed provider result.
    /// 当前进程是否仍持有已完成的供应商结果。
    result_available: bool,
}

impl From<ProviderAsyncTaskRecord> for ProviderTaskSummary {
    /// Projects one retained record without credential material or result bytes.
    /// 投影一条保留记录，不含凭据材料与结果字节。
    ///
    /// # Parameters
    /// 参数
    /// `record` is one immutable store snapshot entry.
    /// `record` 是一条不可变存储快照条目。
    ///
    /// # Returns
    /// 返回值
    /// The status-view summary of the same task.
    /// 同一任务的状态视图摘要。
    fn from(record: ProviderAsyncTaskRecord) -> Self {
        let frozen_target = record.route.frozen_target.as_ref();
        Self {
            capability: frozen_target.map(|target| target.contract.capability),
            target_display_name: frozen_target.map(|target| target.display_name.clone()),
            terminal: record.phase.is_terminal(),
            result_available: record.result.is_some(),
            execution_id: record.execution_id,
            owner_session_id: record.owner_session_id,
            provider_id: record.route.provider_id,
            connection_id: record.route.connection_id,
            target_id: record.route.selection.target_id,
            phase: record.phase,
            cancellation_supported: record.cancellation_supported,
            created_at_unix_ms: record.created_at_unix_ms,
            updated_at_unix_ms: record.updated_at_unix_ms,
            deadline_unix_ms: record.deadline_unix_ms,
            consecutive_observation_failures: record.consecutive_observation_failures,
            error: record.error,
        }
    }
}

/// Lists every retained provider task, newest first.
/// 列出全部保留的供应商任务，最新的在前。
///
/// # Parameters
/// 参数
/// `state` owns the public AI runtime.
/// `state` 持有公共 AI 运行时。
///
/// # Returns
/// 返回值
/// Status summaries ordered by creation time descending, or a runtime availability failure.
/// 按创建时间倒序排列的状态摘要，或运行时不可用错误。
pub(crate) fn service_list_provider_tasks(
    state: SharedServiceBridgeState,
) -> Result<Vec<ProviderTaskSummary>, String> {
    let (runtime, _) = public_ai_runtime_and_configuration(&state)?;
    let mut summaries = runtime
        .provider_task_records()?
        .into_iter()
        .map(ProviderTaskSummary::from)
        .collect::<Vec<_>>();
    summaries.sort_by(|left, right| {
        right
            .created_at_unix_ms
            .cmp(&left.created_at_unix_ms)
            .then_with(|| left.execution_id.cmp(&right.execution_id))
    });
    Ok(summaries)
}

/// Requests provider cancellation of one task on the user's behalf.
/// 代表用户请求取消一个供应商任务。
///
/// # Parameters
/// 参数
/// `state` owns the runtime and configuration; `execution_id` is the stable task identity and
/// `owner_session_id` must repeat the owner shown in the status view, so a stale view cannot
/// cancel a task that changed hands.
/// `state` 持有运行时与配置；`execution_id` 是稳定任务标识，`owner_session_id` 必须重复状态视图
/// 展示的所有者，使过期视图无法取消已易主的任务。
///
/// # Returns
/// 返回值
/// The provider-confirmed record summary; uncertain cancellation stays recoverable in the background.
/// 返回供应商确认后的记录摘要；不确定的取消会在后台继续恢复。
pub(crate) async fn service_cancel_provider_task(
    state: SharedServiceBridgeState,
    execution_id: String,
    owner_session_id: Option<String>,
) -> Result<ProviderTaskSummary, String> {
    let (runtime, configuration) = public_ai_runtime_and_configuration(&state)?;
    runtime
        .cancel_provider_task(
            &configuration,
            execution_id.as_str(),
            owner_session_id.as_deref(),
        )
        .await
        .map(ProviderTaskSummary::from)
}

#[cfg(test)]
mod tests {
    use super::ProviderTaskSummary;
    use serde_json::json;
    use vulcan_provider_adapters::ProviderAsyncTaskRecord;

    /// Builds one minimal retained record through its persisted JSON shape.
    /// 通过持久化 JSON 形状构建一条最小保留记录。
    fn record(phase: &str, error: Option<&str>) -> ProviderAsyncTaskRecord {
        serde_json::from_value(json!({
            "execution_id": "exec-1",
            "owner_session_id": "session-1",
            "route": {
                "provider_id": "provider-1",
                "service_entry_id": "entry-1",
                "connection_id": "connection-1",
                "selection": { "connection_id": "connection-1", "target_id": "video-target" },
                "adapter": "generic_json"
            },
            "provider_task_id": "upstream-1",
            "recovery": { "kind": "none" },
            "phase": phase,
            "created_at_unix_ms": 1_000,
            "updated_at_unix_ms": 2_000,
            "deadline_unix_ms": 9_000,
            "cancellation_supported": true,
            "error": error
        }))
        .expect("minimal provider task record must deserialize")
    }

    /// Verifies the status projection keeps identity and lifecycle facts and serializes in camelCase.
    /// 验证状态投影保留身份与生命周期事实，并以 camelCase 序列化。
    #[test]
    fn summary_projects_lifecycle_facts_in_camel_case() {
        let summary = ProviderTaskSummary::from(record("running", None));
        let value = serde_json::to_value(&summary).expect("summary must serialize");

        assert_eq!(value["executionId"], "exec-1");
        assert_eq!(value["ownerSessionId"], "session-1");
        assert_eq!(value["targetId"], "video-target");
        assert_eq!(value["phase"], "running");
        assert_eq!(value["terminal"], false);
        assert_eq!(value["cancellationSupported"], true);
        assert_eq!(value["resultAvailable"], false);
        assert!(value.get("provider_task_id").is_none());
        assert!(value.get("providerTaskId").is_none());
    }

    /// Verifies terminal phases are reported as terminal with their diagnostic.
    /// 验证终态阶段被报告为终态并携带其诊断。
    #[test]
    fn summary_marks_terminal_phases() {
        let summary = ProviderTaskSummary::from(record("failed", Some("deadline exceeded")));
        let value = serde_json::to_value(&summary).expect("summary must serialize");

        assert_eq!(value["terminal"], true);
        assert_eq!(value["error"], "deadline exceeded");
    }
}
