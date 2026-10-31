//! Usage-ledger rows for the judges that do not run through the host AI port: the cloud (official
//! edge) judge and the local worker. The API judge is recorded by the host AI port itself.
//! 不经宿主 AI 端口运行的判定器（云端官方边缘节点与本机 worker）的用量账本记录；API 判定由宿主
//! AI 端口自行记录。
//!
//! A row is written after the judgement finished and never delays or changes the gate decision; a
//! failure to write it is only logged.
//! 记录在判定结束后写入，绝不延迟或改变闸门决定；写入失败只写日志。

use time::{OffsetDateTime, UtcOffset};
use uuid::Uuid;
use vulcan_ai_contract::AiOperation;
use vulcan_workspace_store::workspace_store::{
    ProviderUsageAttemptRecord, persist_provider_usage_attempt_and_rebuild_daily,
};

// The API judge's caller identity is the ledger consumer of every judge backend.
// API 判定的调用方标识即所有判定后端的账本消费者。
use super::api::JUDGE_CALLER_ID as PERMISSION_JUDGE_CONSUMER_ID;
use crate::SharedServiceBridgeState;

/// Provider identity of judgements answered by the local worker.
/// 本机 worker 作答的判定所用的供应商标识。
const LOCAL_JUDGE_PROVIDER_ID: &str = "local-judge";

/// Service entry, connection and route of the local worker; it has no remote counterpart.
/// 本机 worker 的服务入口、连接与路由；它没有远端对应物。
const LOCAL_JUDGE_ENDPOINT_ID: &str = "local";

/// Protocol of the local worker's judge-host process.
/// 本机 worker 判定宿主进程的协议。
const LOCAL_JUDGE_PROTOCOL_ID: &str = "vulcan-judge-host.v1";

/// Protocol of the cloud judge's official edge endpoints.
/// 云端判定官方边缘端点的协议。
const EDGE_JUDGE_PROTOCOL_ID: &str = "openvulcan-edge-judge.v1";

/// Where one judgement was answered.
/// 一次判定的作答方。
pub(super) enum JudgeRoute {
    /// The cloud judge of the signed-in OpenVulcan account, served from the official edge.
    /// 已登录 OpenVulcan 账户的云端判定，由官方边缘节点提供。
    Edge {
        /// Account connection the request was sent with.
        /// 发送请求所用的账户连接。
        connection_id: String,
    },
    /// The local worker.
    /// 本机 worker。
    Local,
}

/// How one judgement ended.
/// 一次判定的结束方式。
pub(super) enum JudgeUsageOutcome {
    /// The model scored the call after reading `input_tokens`.
    /// 模型读取 `input_tokens` 后为调用评分。
    Evaluated { input_tokens: u32 },
    /// The call exceeded the model's token limit; `input_tokens` when the backend reported it.
    /// 调用超过模型 token 上限；后端报告时附 `input_tokens`。
    InputTokenLimit { input_tokens: Option<u32> },
    /// The judgement failed before the model answered.
    /// 判定在模型作答前失败。
    Failed,
}

/// One finished judgement to record.
/// 一次待记录的已结束判定。
pub(super) struct JudgeUsage {
    /// Where it was answered.
    /// 作答方。
    pub(super) route: JudgeRoute,
    /// Model that answered it.
    /// 作答的模型。
    pub(super) model: String,
    /// Unix milliseconds when the judgement started.
    /// 判定开始的 Unix 毫秒。
    pub(super) started_at_ms: i64,
    /// Unix milliseconds when it ended.
    /// 判定结束的 Unix 毫秒。
    pub(super) finished_at_ms: i64,
    /// How it ended.
    /// 结束方式。
    pub(super) outcome: JudgeUsageOutcome,
}

/// Provider and service entry of the account connection `connection_id`.
/// 账户连接 `connection_id` 的供应商与服务入口。
fn edge_connection(
    state: &SharedServiceBridgeState,
    connection_id: &str,
) -> Result<(String, String), String> {
    let (_, configuration) = crate::public_ai_settings::public_ai_runtime_and_configuration(state)?;
    let connection = configuration
        .connection_graph
        .connections
        .get(connection_id)
        .ok_or_else(|| {
            format!("the judge account connection `{connection_id}` no longer exists")
        })?;
    Ok((
        connection.provider_id.clone(),
        connection.service_entry_id.clone(),
    ))
}

/// Records `usage` in the background without blocking the gate.
/// 在后台记录 `usage`，不阻塞闸门。
pub(super) fn record_judge_usage(state: &SharedServiceBridgeState, usage: JudgeUsage) {
    let storage = match state.lock() {
        Ok(guard) => guard.controller_runtime.clone(),
        Err(error) => {
            tracing::warn!("failed to lock service state for judge usage: {error}");
            return;
        }
    };
    // Without a host database there is no ledger to write to.
    // 没有宿主数据库时不存在可写入的账本。
    let Some(storage) = storage else {
        return;
    };
    let endpoint = match &usage.route {
        JudgeRoute::Edge { connection_id } => match edge_connection(state, connection_id) {
            Ok((provider_id, service_entry_id)) => Endpoint::Edge {
                connection_id: connection_id.clone(),
                provider_id,
                service_entry_id,
            },
            Err(error) => {
                tracing::warn!("failed to record permission judge usage: {error}");
                return;
            }
        },
        JudgeRoute::Local => Endpoint::Local,
    };
    let record = attempt_record(endpoint, usage, local_calendar_date);
    tokio::spawn(async move {
        if let Err(error) = persist_provider_usage_attempt_and_rebuild_daily(
            &storage,
            &record,
            record.terminal_at_ms,
        )
        .await
        {
            tracing::warn!("failed to record permission judge usage: {error}");
        }
    });
}

/// Local calendar date of `timestamp_ms`, the day key every usage row is grouped by.
/// `timestamp_ms` 的本地日历日期，即所有用量记录分组所用的日期键。
fn local_calendar_date(timestamp_ms: i64) -> String {
    let offset = UtcOffset::current_local_offset().unwrap_or(UtcOffset::UTC);
    OffsetDateTime::from_unix_timestamp_nanos(i128::from(timestamp_ms) * 1_000_000)
        .map(|instant| instant.to_offset(offset).date().to_string())
        .unwrap_or_else(|_| String::from("1970-01-01"))
}

/// Resolved ledger identity of one judge route.
/// 一条判定路由解析后的账本标识。
enum Endpoint {
    /// The account connection with its provider and service entry.
    /// 账户连接及其供应商与服务入口。
    Edge {
        connection_id: String,
        provider_id: String,
        service_entry_id: String,
    },
    /// The local worker.
    /// 本机 worker。
    Local,
}

/// Builds the ledger row of `usage` answered at `endpoint`; `calendar_date` maps the end time to
/// its day key.
/// 构建在 `endpoint` 作答的 `usage` 的账本记录；`calendar_date` 将结束时间映射为日期键。
fn attempt_record(
    endpoint: Endpoint,
    usage: JudgeUsage,
    calendar_date: impl Fn(i64) -> String,
) -> ProviderUsageAttemptRecord {
    let (provider_id, service_entry_id, connection_id, protocol_profile_id, transport) =
        match endpoint {
            Endpoint::Edge {
                connection_id,
                provider_id,
                service_entry_id,
            } => (
                provider_id,
                service_entry_id,
                connection_id,
                EDGE_JUDGE_PROTOCOL_ID,
                Some(String::from("http")),
            ),
            Endpoint::Local => (
                String::from(LOCAL_JUDGE_PROVIDER_ID),
                String::from(LOCAL_JUDGE_ENDPOINT_ID),
                String::from(LOCAL_JUDGE_ENDPOINT_ID),
                LOCAL_JUDGE_PROTOCOL_ID,
                None,
            ),
        };
    let local = transport.is_none();
    let (status, error_category, input_tokens) = match usage.outcome {
        JudgeUsageOutcome::Evaluated { input_tokens } => ("succeeded", None, Some(input_tokens)),
        JudgeUsageOutcome::InputTokenLimit { input_tokens } => {
            ("failed", Some("context_length_exceeded"), input_tokens)
        }
        JudgeUsageOutcome::Failed => ("failed", Some("execution"), None),
    };
    let succeeded = status == "succeeded";
    // A failed remote request may or may not have reached the endpoint; the local worker always ran.
    // 失败的远端请求不一定到达端点；本机 worker 总会运行。
    let dispatched_known = succeeded || local;
    let input_tokens = input_tokens.map(i64::from);
    let attempt_id = Uuid::new_v4().to_string();
    ProviderUsageAttemptRecord {
        logical_request_id: attempt_id.clone(),
        attempt_id,
        attempt_index: 0,
        failover_index: None,
        consumer_id: String::from(PERMISSION_JUDGE_CONSUMER_ID),
        operation: String::from(AiOperation::DecisionEvaluate.as_str()),
        provider_id,
        route_id: Some(connection_id.clone()),
        service_entry_id,
        connection_id,
        protocol_profile_id: String::from(protocol_profile_id),
        route_revision: None,
        transport,
        requested_model: usage.model.clone(),
        mapped_model: usage.model,
        response_model: None,
        response_id: None,
        provider_request_id: None,
        dispatched_at_ms: dispatched_known.then_some(usage.started_at_ms),
        headers_at_ms: None,
        first_semantic_at_ms: None,
        first_token_at_ms: None,
        terminal_at_ms: usage.finished_at_ms,
        status: String::from(status),
        dispatched: dispatched_known,
        dispatched_known,
        semantic_output_observed: succeeded,
        replay_safe: Some(true),
        http_status: None,
        websocket_close_code: None,
        error_category: error_category.map(String::from),
        error_code: None,
        usage_availability: String::from(if input_tokens.is_some() {
            "final"
        } else {
            "missing"
        }),
        input_tokens,
        uncached_input_tokens: input_tokens,
        cache_read_tokens: None,
        cache_write_tokens: None,
        output_tokens: None,
        reasoning_tokens: None,
        audio_input_tokens: None,
        audio_output_tokens: None,
        total_tokens: input_tokens,
        usage_profile_id: None,
        usage_profile_revision: None,
        usage_source_paths_json: None,
        usage_validation_flags_json: None,
        provider_billing_details_json: None,
        calendar_date: calendar_date(usage.finished_at_ms),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a judgement that ended with `outcome`.
    /// 构建一次以 `outcome` 结束的判定。
    fn usage(outcome: JudgeUsageOutcome) -> JudgeUsage {
        JudgeUsage {
            route: JudgeRoute::Local,
            model: String::from("jev-1"),
            started_at_ms: 1_000,
            finished_at_ms: 1_250,
            outcome,
        }
    }

    /// The account endpoint used by the edge tests.
    /// 直连测试使用的账户端点。
    fn account() -> Endpoint {
        Endpoint::Edge {
            connection_id: String::from("account"),
            provider_id: String::from("openvulcan"),
            service_entry_id: String::from("china"),
        }
    }

    /// Verifies an evaluated edge judgement is a final, dispatched decision row of the account.
    /// 验证已评分的直连判定是该账户的一条终态、已分派的决策记录。
    #[test]
    fn an_evaluated_edge_judgement_is_a_final_decision_row() {
        let record = attempt_record(
            account(),
            usage(JudgeUsageOutcome::Evaluated { input_tokens: 404 }),
            |_| String::from("2026-10-31"),
        );
        assert_eq!(record.consumer_id, PERMISSION_JUDGE_CONSUMER_ID);
        assert_eq!(record.operation, AiOperation::DecisionEvaluate.as_str());
        assert_eq!(
            (record.provider_id.as_str(), record.connection_id.as_str()),
            ("openvulcan", "account")
        );
        assert_eq!(record.status, "succeeded");
        assert_eq!(record.usage_availability, "final");
        assert_eq!(
            (record.input_tokens, record.total_tokens),
            (Some(404), Some(404))
        );
        assert_eq!(
            (record.dispatched_at_ms, record.terminal_at_ms),
            (Some(1_000), 1_250)
        );
        assert!(record.dispatched && record.dispatched_known);
    }

    /// Verifies a failed edge request keeps its delivery unknown and carries no usage.
    /// 验证失败的直连请求保持送达未知且不带用量。
    #[test]
    fn a_failed_edge_request_keeps_delivery_unknown() {
        let record = attempt_record(account(), usage(JudgeUsageOutcome::Failed), |_| {
            String::from("2026-10-31")
        });
        assert_eq!(record.status, "failed");
        assert_eq!(record.error_category.as_deref(), Some("execution"));
        assert_eq!(record.usage_availability, "missing");
        assert_eq!(record.dispatched_at_ms, None);
        assert!(!record.dispatched && !record.dispatched_known);
    }

    /// Verifies the local worker is its own provider and a refused long call keeps its length.
    /// 验证本机 worker 是独立供应商，被拒绝的超长调用保留其长度。
    #[test]
    fn a_local_judgement_over_the_limit_keeps_its_length() {
        let record = attempt_record(
            Endpoint::Local,
            usage(JudgeUsageOutcome::InputTokenLimit {
                input_tokens: Some(9_000),
            }),
            |_| String::from("2026-10-31"),
        );
        assert_eq!(record.provider_id, LOCAL_JUDGE_PROVIDER_ID);
        assert_eq!(record.transport, None);
        assert_eq!(
            record.error_category.as_deref(),
            Some("context_length_exceeded")
        );
        assert_eq!(record.input_tokens, Some(9_000));
        assert!(record.dispatched && record.dispatched_known);
    }
}
