//! Async-task scheduling, frozen-route recovery, and deadline regressions.
//! 异步任务调度、冻结路由恢复与截止时间回归测试。

use super::super::test_support::connection;
use super::*;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

/// Builds one independent legacy task record for bounded-recovery scheduling tests.
/// 为有界恢复调度测试构建一条独立旧版任务记录。
///
/// # Parameters
/// 参数
/// `index` provides the stable task identity suffix.
/// `index` 提供稳定任务身份后缀。
///
/// # Returns
/// 返回值
/// A non-terminal record whose transport details are intentionally irrelevant to the
/// scheduler-only test.
/// 返回一条非终态记录；其传输细节对于仅测试调度器的场景刻意无关。
fn recovery_scheduler_record(index: usize) -> ProviderAsyncTaskRecord {
    ProviderAsyncTaskRecord {
        execution_id: format!("execution-recovery-{index:02}"),
        owner_session_id: Some(String::from("session-recovery")),
        route: ProviderAsyncTaskRoute {
            provider_id: String::from("test-provider"),
            service_entry_id: String::from("test-entry"),
            connection_id: String::from("test-connection"),
            selection: ProviderToolTargetSelection {
                connection_id: String::from("test-connection"),
                target_id: String::from("test-target"),
            },
            adapter: crate::ProviderToolAdapterKind::GenericJson,
            manifest_revision: 0,
            frozen_target: None,
            frozen_service_entry: None,
            operation_id: None,
            frozen_operation: None,
            frozen_request_profiles: Vec::new(),
            frozen_response_profiles: Vec::new(),
        },
        provider_task_id: Some(format!("provider-task-{index:02}")),
        recovery: ProviderAsyncTaskRecoveryContext::None,
        phase: ProviderAsyncTaskPhase::Running,
        created_at_unix_ms: 1,
        updated_at_unix_ms: 1,
        deadline_unix_ms: 60_001,
        poll_interval_ms: 15_000,
        consecutive_observation_failures: 0,
        cancellation_supported: false,
        error: None,
        result: None,
    }
}

/// Verifies one recovery pass overlaps independent tasks without crossing its fixed ceiling.
/// 验证一次恢复轮次会重叠执行独立任务，同时不会越过固定并发上限。
#[tokio::test]
async fn provider_task_recovery_scheduler_is_concurrent_and_bounded() {
    // counters observe live scheduling independently from task transport behavior.
    // counters 独立于任务传输行为观测实时调度。
    let active = Arc::new(AtomicUsize::new(0));
    let maximum = Arc::new(AtomicUsize::new(0));
    let records = (0..PROVIDER_TASK_RECOVERY_MAX_CONCURRENCY * 2)
        .map(recovery_scheduler_record)
        .collect::<Vec<_>>();

    let observations = observe_provider_task_records_bounded(records, |record| {
        // each admitted future owns counter handles and the record identity for its full life.
        // 每个获准 Future 在完整生命周期内持有计数器句柄与记录身份。
        let active = Arc::clone(&active);
        let maximum = Arc::clone(&maximum);
        async move {
            let current = active.fetch_add(1, Ordering::SeqCst).saturating_add(1);
            maximum.fetch_max(current, Ordering::SeqCst);
            tokio::time::sleep(Duration::from_millis(20)).await;
            active.fetch_sub(1, Ordering::SeqCst);
            if record.execution_id.is_empty() {
                return Err(String::from("scheduler lost the task identity"));
            }
            Ok(())
        }
    })
    .await;

    assert_eq!(
        observations.len(),
        PROVIDER_TASK_RECOVERY_MAX_CONCURRENCY * 2
    );
    assert!(observations.iter().all(|(_, result)| result.is_ok()));
    let observed_maximum = maximum.load(Ordering::SeqCst);
    assert!(
        observed_maximum > 1,
        "independent recovery observations must overlap"
    );
    assert!(
        observed_maximum <= PROVIDER_TASK_RECOVERY_MAX_CONCURRENCY,
        "observed {observed_maximum} concurrent tasks above the configured ceiling"
    );
    assert_eq!(active.load(Ordering::SeqCst), 0);
}

/// Verifies failed post-restart result recovery remains visible on a completed task.
/// 验证重启后结果恢复失败仍会在已完成任务上保持可见。
#[test]
fn completed_task_result_recovery_retains_transient_diagnostic() {
    let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
    let record = ProviderAsyncTaskRecord {
        execution_id: String::from("execution-completed-recovery"),
        owner_session_id: Some(String::from("session-one")),
        route: ProviderAsyncTaskRoute {
            provider_id: String::from("test-provider"),
            service_entry_id: String::from("test-entry"),
            connection_id: String::from("test-connection"),
            selection: ProviderToolTargetSelection {
                connection_id: String::from("test-connection"),
                target_id: String::from("test-target"),
            },
            adapter: crate::ProviderToolAdapterKind::GenericJson,
            manifest_revision: 0,
            frozen_target: None,
            frozen_service_entry: None,
            operation_id: None,
            frozen_operation: None,
            frozen_request_profiles: Vec::new(),
            frozen_response_profiles: Vec::new(),
        },
        provider_task_id: Some(String::from("provider-task-one")),
        recovery: ProviderAsyncTaskRecoveryContext::None,
        phase: ProviderAsyncTaskPhase::Completed,
        created_at_unix_ms: 1,
        updated_at_unix_ms: 1,
        deadline_unix_ms: 60_001,
        poll_interval_ms: 15_000,
        consecutive_observation_failures: 0,
        cancellation_supported: false,
        error: None,
        result: None,
    };
    runtime
        .provider_tasks
        .replace(record)
        .expect("completed recovery record should insert");

    runtime
        .record_provider_task_observation_error(
            "execution-completed-recovery",
            String::from("temporary recovery failure"),
        )
        .expect("transient recovery diagnostic should persist");

    let retained = runtime
        .provider_tasks
        .require("execution-completed-recovery")
        .expect("completed recovery record should remain");
    assert_eq!(retained.phase, ProviderAsyncTaskPhase::Completed);
    assert_eq!(retained.consecutive_observation_failures, 1);
    assert_eq!(
        retained.error.as_deref(),
        Some("temporary recovery failure")
    );
}

/// Verifies an observation failure at the frozen deadline becomes terminal immediately.
/// 验证冻结截止时间到达时的观测失败会立即成为终态。
#[test]
fn expired_task_observation_error_persists_timeout_terminal_state() {
    let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
    let now_unix_ms = current_unix_millis().expect("fixture clock should resolve");
    let created_at_unix_ms = now_unix_ms.saturating_sub(60_000);
    let deadline_unix_ms = now_unix_ms.saturating_sub(1);
    let record = ProviderAsyncTaskRecord {
        execution_id: String::from("execution-expired-observation"),
        owner_session_id: Some(String::from("session-one")),
        route: ProviderAsyncTaskRoute {
            provider_id: String::from("test-provider"),
            service_entry_id: String::from("test-entry"),
            connection_id: String::from("test-connection"),
            selection: ProviderToolTargetSelection {
                connection_id: String::from("test-connection"),
                target_id: String::from("test-target"),
            },
            adapter: crate::ProviderToolAdapterKind::GenericJson,
            manifest_revision: 0,
            frozen_target: None,
            frozen_service_entry: None,
            operation_id: None,
            frozen_operation: None,
            frozen_request_profiles: Vec::new(),
            frozen_response_profiles: Vec::new(),
        },
        provider_task_id: Some(String::from("provider-task-one")),
        recovery: ProviderAsyncTaskRecoveryContext::None,
        phase: ProviderAsyncTaskPhase::Running,
        created_at_unix_ms,
        updated_at_unix_ms: created_at_unix_ms,
        deadline_unix_ms,
        poll_interval_ms: 15_000,
        consecutive_observation_failures: 0,
        cancellation_supported: false,
        error: None,
        result: None,
    };
    runtime
        .provider_tasks
        .replace(record)
        .expect("expired record should insert");

    runtime
        .record_provider_task_observation_error(
            "execution-expired-observation",
            String::from("credential refresh stalled"),
        )
        .expect("deadline transition should persist");

    let retained = runtime
        .provider_tasks
        .require("execution-expired-observation")
        .expect("expired record should remain");
    assert_eq!(retained.phase, ProviderAsyncTaskPhase::Failed);
    assert_eq!(retained.consecutive_observation_failures, 0);
    assert!(
        retained
            .error
            .as_deref()
            .is_some_and(|error| error.contains("exceeded its"))
    );
}

/// Verifies a provider-task network future cannot block its caller beyond the exact host limit.
/// 验证供应商任务网络 Future 无法阻塞调用方超过精确宿主限制。
#[tokio::test]
async fn provider_task_network_operation_has_a_controlled_timeout() {
    let error = await_bounded_provider_task_operation(
        Duration::from_millis(1),
        "Test provider task observation",
        std::future::pending::<Result<(), String>>(),
    )
    .await
    .expect_err("pending provider operation should time out");

    assert!(error.contains("Test provider task observation"));
    assert!(error.contains("1 millisecond"));
}

/// Verifies recovered async tasks resolve the complete route from the dispatch-frozen snapshot
/// and never read a newer manifest (plan §R9.6).
/// 验证恢复的异步任务从分派时冻结的快照解析完整路由，绝不读取更新的 Manifest
/// （方案 §R9.6）。
#[tokio::test]
async fn recovered_async_task_uses_frozen_target_snapshot() {
    let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
    let mut configuration = PersistedAiConfiguration::default();
    let mut tavily_connection = connection("tavily-account");
    tavily_connection.provider_id = String::from("tavily");
    tavily_connection.service_entry_id = String::from("public-api");
    configuration
        .connection_graph
        .add_connection(
            &runtime
                .providers()
                .expect("provider registry should resolve"),
            tavily_connection,
        )
        .expect("connection should register");

    // Dispatch freezes a manifest-owned target snapshot with revision one.
    // 分派时冻结一个 Manifest 持有的目标快照，修订号为一。
    let directory = vulcan_runtime_assets::resolve_bundled_provider_manifests_directory();
    let text = std::fs::read_to_string(directory.join("tavily.json"))
        .expect("bundled tavily manifest must exist");
    let mut document: serde_json::Value =
        serde_json::from_str(&text).expect("manifest should parse");
    document["tool_targets"] = serde_json::json!([
        {
            "target_id": "public-api.search",
            "description": "Tavily structured web search.",
            "service_entry_id": "public-api",
            "credential_profile_ids": ["api-key"],
            "upstream_target_id": "search",
            "display_name": "Tavily Search",
            "tool_id": "vulcan_web_search",
            "capability": "web_search",
            "input_schema": serde_json::json!({
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "query": {"type": "string", "minLength": 1}
                },
                "required": ["query"]
            }),
            "output": {
                "kind": "search",
                "source_path": "/results",
                "normalized_result_path": "search",
                "resource_kind": null,
                "mime_types": []
            },
            "execution": {
                "endpoint_path": "/search",
                "authentication": "bearer_authorization",
                "adapter": "generic_json",
                "execution_kind": "http_json_sync"
            }
        }
    ]);
    document["operations"]
        .as_array_mut()
        .expect("tavily operations should be an array")
        .retain(|operation| {
            operation["tool_target_refs"]
                .as_array()
                .is_some_and(|references| {
                    references
                        .iter()
                        .any(|reference| reference == "public-api.search")
                })
        });
    let compiled = crate::provider_manifest::compile_manifest_text(
        &serde_json::to_string_pretty(&document).expect("manifest should serialize"),
        crate::provider_manifest::ManifestOwnership::System,
    )
    .expect("manifest with tool targets must compile");
    runtime
        .replace_system_manifest("tavily", Arc::new(compiled))
        .expect("hot replace should succeed");
    runtime
        .activate_staged_manifest_catalogs(&configuration, &[])
        .await
        .expect("first frozen-target generation should publish");

    let frozen = runtime
        .resolved_provider_tool_targets("tavily")
        .into_iter()
        .find(|target| target.target_id == "public-api.search")
        .expect("frozen target must resolve at dispatch");
    let mut frozen_service_entry = runtime
        .providers()
        .expect("dispatch registry should resolve")
        .provider("tavily")
        .expect("dispatch provider should exist")
        .service_entries
        .iter()
        .find(|entry| entry.id == "public-api")
        .cloned()
        .expect("dispatch service entry should exist");
    // The frozen service entry records the fallback origin that was effective at dispatch.
    // 冻结服务入口记录分派时生效的回退源地址。
    frozen_service_entry.base_url = String::from("https://dispatch-frozen.example");
    let frozen_display_name = frozen.display_name.clone();

    // A newer manifest changes the same target's display name after dispatch.
    // 分派后更新的 Manifest 修改了同一目标的显示名称。
    document["revision"] = serde_json::json!(2);
    document["tool_targets"][0]["display_name"] = serde_json::json!("Tavily Search Renamed");
    let replaced = crate::provider_manifest::compile_manifest_text(
        &serde_json::to_string_pretty(&document).expect("manifest should serialize"),
        crate::provider_manifest::ManifestOwnership::System,
    )
    .expect("replacement manifest must compile");
    runtime
        .replace_system_manifest("tavily", Arc::new(replaced))
        .expect("hot replace should succeed");
    runtime
        .activate_staged_manifest_catalogs(&configuration, &[])
        .await
        .expect("replacement frozen-target generation should publish");

    // Recovery resolves the exact frozen snapshot captured at dispatch instead of the
    // newer manifest's renamed target.
    // 恢复时解析分派时捕获的精确冻结快照，而不是新清单中已重命名的目标。
    let mut record = ProviderAsyncTaskRecord {
        execution_id: String::from("execution-frozen"),
        owner_session_id: Some(String::from("session-one")),
        route: ProviderAsyncTaskRoute {
            provider_id: String::from("tavily"),
            service_entry_id: String::from("public-api"),
            connection_id: String::from("tavily-account"),
            selection: ProviderToolTargetSelection {
                connection_id: String::from("tavily-account"),
                target_id: String::from("public-api.search"),
            },
            adapter: frozen.execution.adapter,
            manifest_revision: 1,
            frozen_target: Some(frozen.clone()),
            frozen_service_entry: Some(frozen_service_entry),
            operation_id: Some(String::from("tavily.search")),
            frozen_operation: None,
            frozen_request_profiles: Vec::new(),
            frozen_response_profiles: Vec::new(),
        },
        provider_task_id: Some(String::from("task-one")),
        recovery: crate::provider_task::ProviderAsyncTaskRecoveryContext::None,
        phase: crate::provider_task::ProviderAsyncTaskPhase::Queued,
        created_at_unix_ms: 1,
        updated_at_unix_ms: 1,
        deadline_unix_ms: u64::MAX,
        poll_interval_ms: 15_000,
        consecutive_observation_failures: 0,
        cancellation_supported: false,
        error: None,
        result: None,
    };
    // The same connection changes origin after dispatch; recovery must not redirect the frozen task.
    // 同一连接在分派后变更源地址；恢复不得重定向已冻结任务。
    configuration
        .connection_graph
        .connections
        .get_mut("tavily-account")
        .expect("retained connection must exist")
        .service_origin_override = Some(String::from("https://current-origin.example"));
    let resolved = runtime
        .resolve_recovered_provider_task(&configuration, &record)
        .expect("frozen recovery must resolve");
    assert_eq!(
        resolved.target.display_name, frozen_display_name,
        "recovery must use the dispatch-frozen target snapshot"
    );
    assert_eq!(
        resolved.target.target_id, "public-api.search",
        "recovery must keep the frozen target identity"
    );
    assert_eq!(
        resolved.entry.base_url, "https://dispatch-frozen.example",
        "recovery must keep the dispatch-frozen service-entry origin"
    );

    // A nonzero revision without its frozen target must not read a hot-replaced target.
    // 缺少冻结目标的非零修订不得读取已热替换目标。
    record.route.frozen_target = None;
    let missing_snapshot_error =
        match runtime.resolve_recovered_provider_task(&configuration, &record) {
            Ok(_) => panic!("manifest-backed recovery without frozen target must fail closed"),
            Err(error) => error,
        };
    assert!(
        missing_snapshot_error.contains("dispatch-frozen target"),
        "error: {missing_snapshot_error}"
    );

    // A persisted revision-zero record predating frozen manifest snapshots resolves through
    // the current authoritative manifest target for backward compatibility.
    // 早于冻结 Manifest 快照的持久化零修订记录会通过当前权威 Manifest 目标解析，以保持向后兼容。
    record.route.manifest_revision = 0;
    record.route.frozen_service_entry = None;
    record.execution_id = String::from("execution-legacy");
    let resolved = runtime
        .resolve_recovered_provider_task(&configuration, &record)
        .expect("legacy recovery must resolve");
    assert_eq!(
        resolved.target.display_name, "Tavily Search Renamed",
        "legacy recovery reads the current catalog target"
    );
    assert_eq!(
        resolved.target.execution.endpoint_base_url.as_deref(),
        Some("https://current-origin.example"),
        "legacy recovery without endpoint evidence uses the current connection origin"
    );
}

/// Verifies timeout classification fires exactly at the frozen manifest deadline.
/// 验证超时分类在冻结的 Manifest 截止时间精确触发。
#[test]
fn provider_task_timeout_uses_frozen_operation_deadline() {
    assert_eq!(provider_task_timeout_diagnostic(1_000, 3_000, 2_999), None);
    assert_eq!(
        provider_task_timeout_diagnostic(1_000, 3_000, 3_000).as_deref(),
        Some("Provider asynchronous task exceeded its 2000 millisecond deadline.")
    );
}

/// Verifies observation sleep is clipped to the exact remaining lifecycle budget.
/// 验证观测等待被裁剪到精确的剩余生命周期预算。
#[test]
fn provider_task_observation_delay_never_crosses_deadline() {
    assert_eq!(
        clamp_provider_task_observation_delay_ms(60_000, 3_000, 2_999),
        1
    );
    assert_eq!(
        clamp_provider_task_observation_delay_ms(250, 3_000, 2_000),
        250
    );
    assert_eq!(
        clamp_provider_task_observation_delay_ms(250, 3_000, 3_000),
        0
    );
}
