use super::*;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use time::OffsetDateTime;
use uuid::Uuid;
use vulcan_ai_contract::AccountUsageSnapshot;
use vulcan_provider_adapters::{AccountUsageQueryKind, PersistedAiConfiguration, PublicAiRuntime};
use vulcan_workspace_store::workspace_store::{
    ProviderAccountUsageSnapshotRecord, ProviderRateLimitSnapshotRecord,
    ProviderUsageAttemptRecord, ProviderUsageDashboard, load_provider_usage_dashboard,
    persist_provider_account_usage_snapshot, persist_provider_rate_limit_snapshot,
    persist_provider_usage_attempt_and_rebuild_daily,
};

/// Loads the native Provider usage dashboard through the service-owned storage runtime.
/// 通过服务持有的存储运行时加载原生 Provider 用量仪表盘。
pub(crate) async fn service_provider_usage_dashboard(
    state: SharedServiceBridgeState,
) -> Result<ProviderUsageDashboard, String> {
    let storage = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?
        .controller_runtime
        .clone()
        .ok_or_else(|| String::from("Host database runtime is not ready"))?;
    load_provider_usage_dashboard(&storage).await
}

/// Persists supported account-usage observations independently from session history.
/// 独立于会话历史持久化受支持的账户用量观测。
pub(crate) async fn persist_native_account_usage_snapshots(
    controller_runtime: &HostRuntimeHandle,
    runtime: &PublicAiRuntime,
    configuration: &PersistedAiConfiguration,
    snapshots: &BTreeMap<String, AccountUsageSnapshot>,
) -> Result<(), String> {
    let providers = runtime.effective_providers(configuration)?;
    for (connection_id, snapshot) in snapshots {
        let connection = configuration
            .connection_graph
            .connections
            .get(connection_id)
            .ok_or_else(|| {
                format!("account-usage snapshot references unknown connection `{connection_id}`")
            })?;
        let provider = providers
            .provider(connection.provider_id.as_str())
            .ok_or_else(|| {
                format!(
                    "connection `{connection_id}` references unknown provider `{}`",
                    connection.provider_id
                )
            })?;
        let entry = provider
            .service_entries
            .iter()
            .find(|entry| entry.id == connection.service_entry_id)
            .ok_or_else(|| {
                format!(
                    "connection `{connection_id}` references unknown service entry `{}`",
                    connection.service_entry_id
                )
            })?;
        let credential_profile = entry
            .credential_profiles
            .iter()
            .find(|profile| profile.id == connection.credential_profile_id)
            .ok_or_else(|| {
                format!(
                    "connection `{connection_id}` references unknown credential profile `{}`",
                    connection.credential_profile_id
                )
            })?;
        let revision_material = serde_json::to_vec(&(entry, credential_profile))
            .map_err(|error| format!("failed to serialize account endpoint profile: {error}"))?;
        let endpoint_profile_revision =
            format!("sha256:{}", hex::encode(Sha256::digest(&revision_material)));
        let payload_json = serde_json::to_string(snapshot)
            .map_err(|error| format!("failed to serialize account-usage snapshot: {error}"))?;
        if payload_json.len() > 64 * 1024 {
            return Err(format!(
                "account-usage snapshot for connection `{connection_id}` exceeds 65536 bytes"
            ));
        }
        persist_provider_account_usage_snapshot(
            controller_runtime,
            &ProviderAccountUsageSnapshotRecord {
                snapshot_id: Uuid::new_v4().to_string(),
                provider_id: connection.provider_id.clone(),
                connection_id: connection_id.clone(),
                snapshot_kind: account_usage_snapshot_kind(credential_profile.account_usage_query)
                    .to_string(),
                endpoint_profile_revision,
                payload_json,
                observed_at_ms: current_provider_usage_unix_millis(),
            },
        )
        .await?;
    }
    Ok(())
}

/// Returns the stable snapshot category for one declared account-query contract.
/// 返回一个已声明账户查询契约对应的稳定快照类别。
fn account_usage_snapshot_kind(query: AccountUsageQueryKind) -> &'static str {
    match query {
        AccountUsageQueryKind::MiniMaxTokenPlan
        | AccountUsageQueryKind::KimiCodingPlan
        | AccountUsageQueryKind::OpencodeGo => "token_plan",
        AccountUsageQueryKind::OpenRouterCredits
        | AccountUsageQueryKind::OpenVulcan
        | AccountUsageQueryKind::TavilyCredits
        | AccountUsageQueryKind::CommandCodeUsage => "quota",
        AccountUsageQueryKind::DeepSeekBalance | AccountUsageQueryKind::OpenDesignWallet => {
            "balance"
        }
        AccountUsageQueryKind::Unsupported => "unsupported",
    }
}

/// Returns the current Unix timestamp in milliseconds for native usage persistence.
/// 返回原生用量持久化使用的当前 Unix 毫秒时间戳。
fn current_provider_usage_unix_millis() -> i64 {
    i64::try_from(OffsetDateTime::now_utc().unix_timestamp_nanos() / 1_000_000).unwrap_or(i64::MAX)
}

/// Persists one terminal physical Provider attempt in the host-owned diagnostics ledger.
/// 将一次终态物理 Provider 尝试持久化到宿主持有的诊断账本。
pub(crate) async fn persist_native_provider_usage_attempt(
    controller_runtime: &HostRuntimeHandle,
    event: &ProviderRequestTelemetryEvent,
) -> Result<(), String> {
    if !matches!(event.kind, ProviderRequestTelemetryEventKind::Completed) {
        return Ok(());
    }
    let status = telemetry_string(event, "status")?;
    let usage = event.completed_usage.as_ref();
    let usage_availability = usage_availability(usage).to_string();
    // Dispatch facts preserve unknown delivery instead of converting it into a proven negative.
    // 分派事实保留未知送达状态，不把它转换成已证明的否定结果。
    let dispatch_outcome = telemetry_optional_string(event, "dispatch_outcome");
    let (dispatched, dispatched_known) =
        dispatch_facts(status.as_str(), dispatch_outcome.as_deref());
    let semantic_output_observed =
        telemetry_optional_bool(event, "semantic_output_observed").unwrap_or(false);
    let record = ProviderUsageAttemptRecord {
        logical_request_id: telemetry_string(event, "execution_id")?,
        attempt_id: telemetry_string(event, "request_id")?,
        attempt_index: telemetry_required_i64(event, "attempt_index")?,
        failover_index: None,
        consumer_id: telemetry_string(event, "owner_id")?,
        operation: telemetry_string(event, "operation")?,
        provider_id: telemetry_string(event, "provider_key")?,
        service_entry_id: telemetry_string(event, "service_entry_id")?,
        connection_id: telemetry_string(event, "connection_id")?,
        route_id: telemetry_optional_string(event, "secondary_route_id"),
        protocol_profile_id: telemetry_string(event, "protocol_profile_id")?,
        route_revision: telemetry_optional_string(event, "route_revision"),
        transport: telemetry_optional_string(event, "transport"),
        requested_model: telemetry_string(event, "model_key")?,
        mapped_model: telemetry_string(event, "model_key")?,
        response_model: usage.and_then(|value| value.response_model.clone()),
        response_id: usage.and_then(|value| value.response_id.clone()),
        provider_request_id: telemetry_optional_string(event, "provider_request_id"),
        dispatched_at_ms: dispatched
            .then(|| {
                telemetry_i64(event, "dispatched_at_ms")
                    .or_else(|| telemetry_i64(event, "started_at_ms"))
            })
            .flatten(),
        headers_at_ms: telemetry_i64(event, "headers_at_ms"),
        first_semantic_at_ms: telemetry_i64(event, "first_event_at_ms"),
        first_token_at_ms: telemetry_i64(event, "first_token_at_ms"),
        terminal_at_ms: telemetry_required_i64(event, "completed_at_ms")?,
        status,
        dispatched,
        dispatched_known,
        semantic_output_observed,
        replay_safe: telemetry_optional_bool(event, "replay_safe"),
        http_status: telemetry_i64(event, "http_status"),
        websocket_close_code: telemetry_i64(event, "websocket_close_code"),
        error_category: telemetry_optional_string(event, "error_category"),
        error_code: telemetry_optional_string(event, "error_code"),
        usage_availability,
        input_tokens: usage_i64(usage.and_then(|value| value.input_tokens), "input_tokens")?,
        uncached_input_tokens: usage_i64(
            usage.and_then(|value| value.uncached_input_tokens),
            "uncached_input_tokens",
        )?,
        cache_read_tokens: usage_i64(
            usage.and_then(|value| value.cached_input_tokens),
            "cached_input_tokens",
        )?,
        cache_write_tokens: usage_i64(
            usage.and_then(|value| value.cache_creation_input_tokens),
            "cache_creation_input_tokens",
        )?,
        output_tokens: usage_i64(usage.and_then(|value| value.output_tokens), "output_tokens")?,
        reasoning_tokens: usage_i64(
            usage.and_then(|value| value.reasoning_tokens),
            "reasoning_tokens",
        )?,
        audio_input_tokens: usage_i64(
            usage.and_then(|value| value.audio_input_tokens),
            "audio_input_tokens",
        )?,
        audio_output_tokens: usage_i64(
            usage.and_then(|value| value.audio_output_tokens),
            "audio_output_tokens",
        )?,
        total_tokens: usage_i64(usage.and_then(|value| value.total_tokens), "total_tokens")?,
        usage_profile_id: usage.and_then(|value| value.extraction_profile_id.clone()),
        usage_profile_revision: usage
            .and_then(|value| value.extraction_profile_revision)
            .map(|value| {
                i64::try_from(value)
                    .map_err(|_| String::from("usage profile revision exceeds SQLite INTEGER"))
            })
            .transpose()?,
        usage_source_paths_json: usage
            .filter(|value| !value.source_paths.is_empty())
            .map(|value| serde_json::to_string(&value.source_paths))
            .transpose()
            .map_err(|error| format!("failed to serialize usage source paths: {error}"))?,
        usage_validation_flags_json: usage
            .filter(|value| !value.validation_flags.is_empty())
            .map(|value| serde_json::to_string(&value.validation_flags))
            .transpose()
            .map_err(|error| format!("failed to serialize usage validation flags: {error}"))?,
        provider_billing_details_json: usage
            .filter(|value| !value.billing_details.is_empty())
            .map(|value| serde_json::to_string(&value.billing_details))
            .transpose()
            .map_err(|error| format!("failed to serialize provider billing details: {error}"))?,
        calendar_date: telemetry_string(event, "calendar_date")?,
    };
    persist_provider_usage_attempt_and_rebuild_daily(
        controller_runtime,
        &record,
        telemetry_required_i64(event, "completed_at_ms")?,
    )
    .await?;
    let retry_after = event
        .payload
        .get("retry_after")
        .filter(|value| !value.is_null());
    let rate_limit_headers = event
        .payload
        .get("rate_limit_headers")
        .filter(|value| value.as_object().is_some_and(|object| !object.is_empty()));
    let provider_request_id = telemetry_optional_string(event, "provider_request_id");
    if retry_after.is_some() || rate_limit_headers.is_some() || provider_request_id.is_some() {
        let retry_after_ms = retry_after
            .cloned()
            .unwrap_or(Value::Null)
            .get("delay_milliseconds")
            .and_then(Value::as_u64)
            .map(|value| {
                i64::try_from(value)
                    .map_err(|_| String::from("retry-after delay exceeds SQLite INTEGER"))
            })
            .transpose()?;
        let headers_json = serde_json::to_string(&serde_json::json!({
            "retry_after": retry_after,
            "rate_limit_headers": rate_limit_headers,
            "provider_request_id": provider_request_id,
        }))
        .map_err(|error| format!("failed to serialize normalized rate-limit snapshot: {error}"))?;
        persist_provider_rate_limit_snapshot(
            controller_runtime,
            &ProviderRateLimitSnapshotRecord {
                snapshot_id: Uuid::new_v4().to_string(),
                attempt_id: record.attempt_id.clone(),
                headers_json,
                retry_after_ms,
                observed_at_ms: record.terminal_at_ms,
            },
        )
        .await?;
    }
    Ok(())
}

/// Reads one required non-empty string field from a telemetry payload.
/// 从遥测载荷读取一个必需的非空字符串字段。
fn telemetry_string(event: &ProviderRequestTelemetryEvent, key: &str) -> Result<String, String> {
    event
        .payload
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| format!("provider telemetry payload is missing non-empty {key}"))
}

/// Reads one optional signed integer field from a telemetry payload.
/// 从遥测载荷读取一个可选的有符号整数字段。
fn telemetry_i64(event: &ProviderRequestTelemetryEvent, key: &str) -> Option<i64> {
    event.payload.get(key).and_then(Value::as_i64)
}

/// Reads one required signed integer field from a telemetry payload.
/// 从遥测载荷读取一个必需的有符号整数字段。
fn telemetry_required_i64(event: &ProviderRequestTelemetryEvent, key: &str) -> Result<i64, String> {
    telemetry_i64(event, key)
        .ok_or_else(|| format!("provider telemetry payload is missing integer {key}"))
}

/// Converts one optional host token count into an exact SQLite integer.
/// 将一个可选的宿主 Token 数精确转换为 SQLite 整数。
fn usage_i64(value: Option<usize>, field: &str) -> Result<Option<i64>, String> {
    value
        .map(|value| {
            i64::try_from(value)
                .map_err(|_| format!("provider usage field {field} exceeds SQLite INTEGER"))
        })
        .transpose()
}

/// Resolves missing, partial, or final availability from explicit Provider evidence.
/// 根据显式 Provider 证据解析缺失、部分或终态可用性。
fn usage_availability(usage: Option<&ProviderRequestCompletedUsage>) -> &'static str {
    match usage {
        Some(value)
            if matches!(
                value.update_kind,
                Some(vulcan_ai_contract::UsageUpdateKind::Final)
            ) =>
        {
            "final"
        }
        Some(_) => "partial",
        None => "missing",
    }
}

/// Reads one optional non-empty string field from a telemetry payload.
/// 从遥测载荷读取一个可选的非空字符串字段。
fn telemetry_optional_string(event: &ProviderRequestTelemetryEvent, key: &str) -> Option<String> {
    event
        .payload
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

/// Reads one optional boolean field from a telemetry payload.
/// 从遥测载荷读取一个可选的布尔字段。
fn telemetry_optional_bool(event: &ProviderRequestTelemetryEvent, key: &str) -> Option<bool> {
    event.payload.get(key).and_then(Value::as_bool)
}

/// Resolves dispatch value and certainty from terminal transport facts.
/// 根据终态传输事实解析分派值及其确定性。
fn dispatch_facts(status: &str, dispatch_outcome: Option<&str>) -> (bool, bool) {
    let dispatched = status == "succeeded" || dispatch_outcome == Some("dispatched");
    let known =
        status == "succeeded" || matches!(dispatch_outcome, Some("dispatched" | "not_dispatched"));
    (dispatched, known)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies unknown delivery never becomes a proven negative dispatch fact.
    /// 验证未知送达绝不会变成已证明的未分派事实。
    #[test]
    fn dispatch_facts_preserve_unknown_delivery() {
        assert_eq!(dispatch_facts("failed", Some("dispatched")), (true, true));
        assert_eq!(
            dispatch_facts("failed", Some("not_dispatched")),
            (false, true)
        );
        assert_eq!(dispatch_facts("failed", Some("unknown")), (false, false));
        assert_eq!(dispatch_facts("failed", None), (false, false));
        assert_eq!(dispatch_facts("succeeded", None), (true, true));
    }

    /// Verifies snapshot or delta usage cannot become final evidence.
    /// 验证快照或增量用量不能被提升为终态证据。
    #[test]
    fn usage_availability_requires_explicit_final_evidence() {
        let mut usage = ProviderRequestCompletedUsage {
            update_kind: Some(vulcan_ai_contract::UsageUpdateKind::Snapshot),
            input_tokens: Some(10),
            ..Default::default()
        };
        assert_eq!(usage_availability(Some(&usage)), "partial");
        usage.update_kind = Some(vulcan_ai_contract::UsageUpdateKind::Delta);
        assert_eq!(usage_availability(Some(&usage)), "partial");
        usage.update_kind = Some(vulcan_ai_contract::UsageUpdateKind::Final);
        assert_eq!(usage_availability(Some(&usage)), "final");
        assert_eq!(usage_availability(None), "missing");
    }
}
