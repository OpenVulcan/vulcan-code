//! Integration tests for the real bundled plugin surfaces and authenticated VLDB requests.
//! 真实内建插件暴露面与已认证VLDB请求的集成测试。

mod prepared;

use super::super::VulcanPluginActionRequest;
use super::test_manager;
use crate::{
    VulcanPluginHostServices, VulcanPluginPermission, VulcanPluginRuntimeState,
    VulcanPluginVldbOperation, VulcanPluginVldbRequest, VulcanPluginVldbService,
};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};

/// RecordingVldbService supplies typed rows for the registered system-statistics binding.
/// RecordingVldbService 为已注册的系统统计绑定提供类型化行。
#[derive(Default)]
struct RecordingVldbService {
    /// Ordered authenticated VLDB requests received from the fixed bridge.
    /// 从固定桥接层收到的有序已认证 VLDB 请求。
    calls: Mutex<Vec<VulcanPluginVldbRequest>>,
}

impl VulcanPluginVldbService for RecordingVldbService {
    /// Records one request and returns rows selected by the exact production query.
    /// 记录一条请求，并按精确生产查询返回行。
    ///
    /// # Parameters
    /// 参数
    /// `request` is the authenticated fixed-bridge VLDB operation.
    /// `request` 是经过认证的固定桥接 VLDB 操作。
    ///
    /// # Returns
    /// 返回值
    /// Returns one deterministic typed query result.
    /// 返回一份确定性的类型化查询结果。
    fn execute(&self, request: VulcanPluginVldbRequest) -> Result<Value, String> {
        let sql = request
            .payload
            .get("sql")
            .and_then(Value::as_str)
            .ok_or_else(|| String::from("Recording VLDB fixture requires SQL."))?;
        if request.operation == VulcanPluginVldbOperation::SqliteExecute {
            if !sql.contains("BEGIN IMMEDIATE;")
                || !sql.contains("DELETE FROM provider_usage_attempts_v3")
            {
                return Err(String::from(
                    "Recording VLDB fixture received an unexpected write.",
                ));
            }
            self.calls
                .lock()
                .expect("lock recording VLDB calls")
                .push(request);
            return Ok(json!({ "rows_changed": 5 }));
        }
        if request.operation != VulcanPluginVldbOperation::SqliteQuery {
            return Err(String::from(
                "Recording VLDB fixture accepts query and execute operations only.",
            ));
        }
        let rows = if sql.contains("AS topProviderName") {
            json!([{
                "date": "2026-08-15",
                "value": 37,
                "requests": 3,
                "successes": 3,
                "topProviderName": "provider-1",
                "topModelName": "gpt-5"
            }])
        } else if sql.contains("AS seriesKey") {
            json!([
                {
                    "date": "2026-08-15",
                    "seriesKey": "provider-1/gpt-5",
                    "label": "provider-1 / gpt-5",
                    "value": 35
                },
                {
                    "date": "2026-08-15",
                    "seriesKey": "provider-1/private-model",
                    "label": "provider-1 / private-model",
                    "value": 2
                }
            ])
        } else if sql.contains("LEFT JOIN daily_timing USING") {
            json!([
                {
                    "providerId": "provider-1",
                    "serviceEntryId": "entry-1",
                    "consumerId": "webui",
                    "operation": "response_create",
                    "modelId": "gpt-5",
                    "attemptCount": 1,
                    "successfulAttemptCount": 1,
                    "reportedUsageAttemptCount": 1,
                    "inputTokens": 10,
                    "uncachedInputTokens": 8,
                    "cacheReadTokens": 2,
                    "cacheWriteTokens": 0,
                    "outputTokens": 20,
                    "reasoningTokens": 5,
                    "totalTokens": 35,
                    "aggregateRevision": 7,
                    "requestDurationMs": 1000,
                    "firstTokenLatencyMs": null,
                    "outputDurationMs": null,
                    "outputTokensPerSecond": null,
                    "timedAttemptCount": 0
                },
                {
                    "providerId": "provider-1",
                    "serviceEntryId": "entry-1",
                    "consumerId": "webui",
                    "operation": "response_create",
                    "modelId": "private-model",
                    "attemptCount": 1,
                    "successfulAttemptCount": 1,
                    "reportedUsageAttemptCount": 1,
                    "inputTokens": 1,
                    "uncachedInputTokens": 1,
                    "cacheReadTokens": 0,
                    "cacheWriteTokens": 0,
                    "outputTokens": 1,
                    "reasoningTokens": 0,
                    "totalTokens": 2,
                    "aggregateRevision": 7,
                    "requestDurationMs": null,
                    "firstTokenLatencyMs": null,
                    "outputDurationMs": null,
                    "outputTokensPerSecond": null,
                    "timedAttemptCount": 0
                },
                {
                    "providerId": "provider-1",
                    "serviceEntryId": "entry-1",
                    "consumerId": "webui",
                    "operation": "response_create",
                    "modelId": "gpt-5",
                    "attemptCount": 1,
                    "successfulAttemptCount": 1,
                    "reportedUsageAttemptCount": 1,
                    "inputTokens": 1,
                    "uncachedInputTokens": 1,
                    "cacheReadTokens": 0,
                    "cacheWriteTokens": 0,
                    "outputTokens": null,
                    "reasoningTokens": null,
                    "totalTokens": null,
                    "aggregateRevision": 7,
                    "requestDurationMs": null,
                    "firstTokenLatencyMs": null,
                    "outputDurationMs": null,
                    "outputTokensPerSecond": null,
                    "timedAttemptCount": 0
                }
            ])
        } else if sql.contains("AS attemptId") {
            json!([{
                "attemptId": "attempt-1",
                "consumerId": "webui",
                "operation": "response_create",
                "providerId": "provider-1",
                "serviceEntryId": "entry-1",
                "modelId": "gpt-5",
                "transport": "http_sse",
                "httpStatus": 200,
                "websocketCloseCode": null,
                "errorCategory": null,
                "errorCode": null,
                "status": "succeeded",
                "usageAvailability": "final",
                "inputTokens": 10,
                "uncachedInputTokens": 8,
                "cacheReadTokens": 2,
                "cacheWriteTokens": 0,
                "outputTokens": 20,
                "reasoningTokens": 5,
                "audioInputTokens": null,
                "audioOutputTokens": null,
                "totalTokens": 35,
                "billingDetailsJson": "[]",
                "terminalAtMs": 1_500,
                "requestDurationMs": 1000,
                "firstTokenLatencyMs": 500,
                "outputDurationMs": 500,
                "outputTokensPerSecond": 40,
                "timedAttemptCount": 1
            }])
        } else if sql.contains("FROM provider_account_usage_snapshots_v1") {
            json!([])
        } else if sql.contains("COUNT(*) AS attemptCount") {
            json!([{
                "attemptCount": 1,
                "totalTokens": 35,
                "aggregateRevision": 7
            }])
        } else if sql.contains("AS dashboardRevision") {
            // Both bounded revision reads observe the same host aggregate transaction in this stable fixture.
            // 此稳定夹具中的两次有界版本读取观察到同一宿主聚合事务。
            json!([{ "dashboardRevision": 7 }])
        } else if sql.contains("AS generatedAtMs") {
            json!([{
                "generatedAtMs": 1_500,
                "schemaRevision": 1,
                "aggregateRevision": 7,
                "healthStatus": "ready"
                ,"dailyCount": 3
                ,"recentCount": 1
                ,"consumerIdsJson": "[\"webui\"]"
            }])
        } else if sql.contains("AS rowCount") {
            json!([
                { "modelId": "gpt-5", "rowCount": 1, "uncachedInputTokens": 8,
                  "cacheReadTokens": 2, "cacheWriteTokens": 0, "outputTokens": 20 },
                { "modelId": "gpt-5", "rowCount": 1, "uncachedInputTokens": 1,
                  "cacheReadTokens": 0, "cacheWriteTokens": 0, "outputTokens": null }
            ])
        } else if sql.contains("AS resultJson") {
            json!([{ "resultJson": "{\"totalCount\":1,\"models\":[{\"modelId\":\"private-model\",\"attempts\":1}]}" }])
        } else if sql.contains("COUNT(*) AS attempts") {
            json!([{ "providerId": "provider-1", "attempts": 3, "successes": 3, "totalTokens": 37 }])
        } else if sql.contains("AS attempts") {
            json!([{
                "attempts": 3, "successes": 3, "reported": 3, "totalTokens": 37,
                "inputTokens": 12, "outputTokens": 21, "cacheReadTokens": 2,
                "cacheWriteTokens": 0
            }])
        } else {
            return Err(format!("Unexpected recording VLDB query: {sql}"));
        };
        self.calls
            .lock()
            .expect("lock recording VLDB calls")
            .push(request);
        Ok(json!({ "rows": rows, "row_count": rows.as_array().map_or(0, Vec::len) }))
    }
}

/// Verifies the repository usage-statistics plugin queries the registered system VLDB binding.
/// 验证仓库用量统计插件查询已注册的系统 VLDB 绑定。
#[test]
fn bundled_usage_statistics_plugin_loads_through_system_vldb_bridge() {
    // temp owns the isolated LuaSkills bridge and plugin runtime state.
    // temp 持有隔离的 LuaSkills 桥接与插件运行时状态。
    let temp = tempfile::tempdir().expect("create usage statistics plugin test directory");
    // runtimeRoot is the deterministic manager root for this integration test.
    // runtimeRoot 是当前集成测试使用的确定性管理器根目录。
    let runtime_root = temp.path().join("runtime");
    fs::create_dir_all(&runtime_root).expect("create usage statistics test runtime root");
    // repositoryRoot is derived from the current crate instead of a process working directory.
    // repositoryRoot 从当前 Crate 派生，不依赖进程工作目录。
    let repository_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("resolve repository root");
    // manager loads the real repository package through the production catalog path.
    // manager 通过生产目录路径加载真实仓库插件包。
    let manager = test_manager(&runtime_root, repository_root.join("plugins"));
    manager
        .initialize_formal_runtime(crate::VulcanPluginRuntimeSettings::default())
        .expect("attach the production formal parent for persistent UI");
    // vldb records every authoritative logical-binding query emitted by the real Lua bundle.
    // vldb 记录真实 Lua Bundle 发出的每个权威逻辑绑定查询。
    let vldb = Arc::new(RecordingVldbService::default());
    manager
        .install_host_services(VulcanPluginHostServices {
            storage: None,
            vldb: Some(vldb.clone()),
            ui_events: None,
        })
        .expect("install recording usage statistics host services");
    // catalog proves the package passed manifest, schema, asset, and locale validation.
    // catalog 证明插件包已通过清单、Schema、资产与语言包校验。
    let catalog = manager
        .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
        .expect("load repository plugin catalog");
    let descriptor = catalog
        .plugins
        .get("vulcan-usage-statistics")
        .expect("resolve usage statistics plugin descriptor");
    let manifest = descriptor
        .manifest
        .as_ref()
        .expect("usage statistics plugin has executable manifest");
    assert!(
        descriptor.desired_enabled,
        "usage statistics plugin must be enabled: {:?}; catalog diagnostics: {:?}",
        descriptor.diagnostics, catalog.diagnostics
    );
    assert_eq!(
        descriptor.runtime_state,
        VulcanPluginRuntimeState::Enabled,
        "usage statistics plugin runtime must be enabled: availability={:?}, execution_available={}, diagnostics={:?}",
        descriptor.availability,
        descriptor.runtime_execution_available,
        descriptor.diagnostics
    );
    assert!(
        manifest
            .permissions
            .contains(&VulcanPluginPermission::SystemVldbAccess)
    );
    assert!(
        !manifest
            .permissions
            .contains(&VulcanPluginPermission::StorageSqlite)
    );
    assert!(
        manifest
            .permissions
            .contains(&VulcanPluginPermission::Config)
    );
    assert!(
        descriptor.hooks.is_empty(),
        "system-backed statistics plugin must not duplicate Provider telemetry hooks"
    );
    // runtime executes the real uncompressed Lua bundle deterministically.
    // runtime 确定性执行真实未压缩 Lua Bundle。
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .expect("create usage statistics Tokio runtime");
    runtime
        .block_on(manager.prewarm_plugin_runtime("vulcan-usage-statistics"))
        .expect("prewarm usage statistics plugin runtime");
    // dashboard is assembled from capability-checked direct VLDB queries.
    // dashboard 由经过能力校验的直接 VLDB 查询组装。
    let dashboard = runtime
        .block_on(manager.invoke_action(VulcanPluginActionRequest {
            plugin_id: String::from("vulcan-usage-statistics"),
            action_id: String::from("dashboard-get"),
            arguments: json!({}),
            caller: crate::VulcanPluginActionCaller::PluginUi,
            workspace_root: repository_root.to_path_buf(),
            reuse_enabled_runtime: true,
        }))
        .expect("invoke system-backed usage statistics dashboard action");
    assert_eq!(dashboard["aggregateRevision"], 7);
    assert_eq!(dashboard["daily"][0]["totalTokens"], 35);
    // priced_cost comes from the live catalog, so later checks compare against it, not a price.
    // priced_cost 来自实时价格目录，因此后续只与它比较，不锁定具体价格。
    let priced_cost = dashboard["daily"][0]["estimatedCost"].clone();
    assert!(
        priced_cost.as_str().is_some_and(|cost| !cost.is_empty()),
        "an exact catalog match carries a cost: {priced_cost}"
    );
    assert_eq!(dashboard["daily"][0]["estimatedCurrency"], "USD");
    assert_eq!(dashboard["daily"][0]["pricingMatchStatus"], "catalog_exact");
    assert_eq!(dashboard["daily"][1]["pricingMatchStatus"], "unmatched");
    assert!(dashboard["daily"][1]["estimatedCost"].is_null());
    assert_eq!(
        dashboard["daily"][2]["pricingMatchStatus"],
        "incomplete_usage"
    );
    assert!(dashboard["daily"][2]["estimatedCost"].is_null());
    assert_eq!(dashboard["heatmap"][0]["value"], 37);
    assert_eq!(dashboard["heatmap"][0]["topModelName"], "gpt-5");
    assert_eq!(dashboard["series"][0]["seriesKey"], "provider-1/gpt-5");
    assert_eq!(dashboard["series"][0]["value"], 35);
    assert!(
        dashboard["pricingCatalog"]["entryCount"]
            .as_u64()
            .is_some_and(|count| count > 0)
    );
    assert_eq!(dashboard["summary"]["costs"][0]["amount"], priced_cost);
    assert_eq!(dashboard["summary"]["costs"][0]["pricedRows"], 1);
    assert_eq!(dashboard["summary"]["costs"][0]["unpricedRows"], 1);
    assert_eq!(dashboard["summary"]["costs"][0]["incompleteRows"], 1);
    assert_eq!(dashboard["recentAttempts"][0]["attemptId"], "attempt-1");
    assert_eq!(dashboard["providers"][0]["providerId"], "provider-1");
    assert_eq!(dashboard["providers"][0]["attempts"], 3);
    assert_eq!(dashboard["recentAttempts"][0]["estimatedCost"], priced_cost);
    // settings proves prices persist only in the plugin-owned configuration domain.
    // settings 证明价格只持久化于插件自有配置域。
    let settings = runtime
        .block_on(manager.invoke_action(VulcanPluginActionRequest {
            plugin_id: String::from("vulcan-usage-statistics"),
            action_id: String::from("settings-get"),
            arguments: json!({}),
            caller: crate::VulcanPluginActionCaller::PluginUi,
            workspace_root: repository_root.to_path_buf(),
            reuse_enabled_runtime: true,
        }))
        .expect("read plugin-owned pricing settings");
    assert_eq!(
        settings["catalogModels"]
            .as_array()
            .map(|models| models.len() as u64),
        dashboard["pricingCatalog"]["entryCount"].as_u64()
    );
    assert_eq!(settings["modelOverrides"], json!({}));
    let updated_settings = runtime
        .block_on(manager.invoke_action(VulcanPluginActionRequest {
            plugin_id: String::from("vulcan-usage-statistics"),
            action_id: String::from("settings-update"),
            arguments: json!({
                "modelOverrides": {
                    "gpt-5": {
                        "inputPerMillion": "2",
                        "outputPerMillion": "20",
                        "cacheReadPerMillion": "0.2",
                        "cacheWritePerMillion": "0"
                    }
                }
            }),
            caller: crate::VulcanPluginActionCaller::PluginUi,
            workspace_root: repository_root.to_path_buf(),
            reuse_enabled_runtime: true,
        }))
        .expect("persist one plugin-owned model price override");
    assert_eq!(
        updated_settings["modelOverrides"]["gpt-5"]["outputPerMillion"],
        "20"
    );
    let overridden_dashboard = runtime
        .block_on(manager.invoke_action(VulcanPluginActionRequest {
            plugin_id: String::from("vulcan-usage-statistics"),
            action_id: String::from("dashboard-get"),
            arguments: json!({}),
            caller: crate::VulcanPluginActionCaller::PluginUi,
            workspace_root: repository_root.to_path_buf(),
            reuse_enabled_runtime: true,
        }))
        .expect("recompute dashboard estimates from the plugin override");
    assert_eq!(
        overridden_dashboard["daily"][0]["pricingMatchStatus"],
        "override_exact"
    );
    assert_eq!(
        overridden_dashboard["daily"][0]["estimatedCost"],
        "0.0004164"
    );
    assert_eq!(
        overridden_dashboard["daily"][0]["pricingRevision"],
        "plugin-config-v1:gpt-5:2:20:0.2:0"
    );
    assert_eq!(overridden_dashboard["pricingCatalog"]["overrideCount"], 1);
    // pricing_page validates the declared complete usage-directory action and unknown prices.
    // pricing_page 验证声明的完整用量目录 Action 与未知价格。
    let pricing_page = runtime
        .block_on(manager.invoke_action(VulcanPluginActionRequest {
            plugin_id: String::from("vulcan-usage-statistics"),
            action_id: String::from("pricing-get"),
            arguments: json!({ "filter": "unpriced", "search": "private", "offset": 0 }),
            caller: crate::VulcanPluginActionCaller::PluginUi,
            workspace_root: repository_root.to_path_buf(),
            reuse_enabled_runtime: true,
        }))
        .expect("query the complete model pricing directory");
    assert_eq!(pricing_page["totalCount"], 1);
    assert_eq!(pricing_page["models"][0]["source"], "unpriced");
    assert!(pricing_page["models"][0]["prices"].is_null());
    // clearPreview proves the real Lua bundle can query maintenance facts through the same binding.
    // clearPreview 证明真实 Lua Bundle 可以通过同一绑定查询维护事实。
    let clear_preview = runtime
        .block_on(manager.invoke_action(VulcanPluginActionRequest {
            plugin_id: String::from("vulcan-usage-statistics"),
            action_id: String::from("statistics-clear-preview"),
            arguments: json!({}),
            caller: crate::VulcanPluginActionCaller::PluginUi,
            workspace_root: repository_root.to_path_buf(),
            reuse_enabled_runtime: true,
        }))
        .expect("preview system statistics cleanup");
    assert_eq!(clear_preview["attemptCount"], 1);
    assert_eq!(clear_preview["aggregateRevision"], 7);
    // clearResult validates revision-bound direct VLDB deletion through the production Lua script.
    // clearResult 验证生产 Lua 脚本通过直接 VLDB 删除执行修订绑定清理。
    let clear_result = runtime
        .block_on(manager.invoke_action(VulcanPluginActionRequest {
            plugin_id: String::from("vulcan-usage-statistics"),
            action_id: String::from("statistics-clear-apply"),
            arguments: json!({ "expectedAggregateRevision": 7 }),
            caller: crate::VulcanPluginActionCaller::PluginUi,
            workspace_root: repository_root.to_path_buf(),
            reuse_enabled_runtime: true,
        }))
        .expect("clear system statistics through direct VLDB access");
    assert_eq!(clear_result["clearedAttemptCount"], 1);
    assert_eq!(clear_result["clearedTokenCount"], 35);
    assert_eq!(clear_result["aggregateRevision"], 8);
    let calls = vldb.calls.lock().expect("lock recorded VLDB calls");
    // recorded_calls is the authorized call total that a later capability withdrawal must leave unchanged.
    // recorded_calls 是授权期间的调用总数，后续撤销能力后必须保持不变。
    let recorded_calls = calls.len();
    // Every dashboard call only reads; the final call remains the guarded clear write.
    // 每次仪表盘调用都只读取；最后一项仍为受保护的清理写入。
    let (clear_call, read_calls) = calls.split_last().expect("recorded VLDB calls");
    assert!(!read_calls.is_empty());
    assert!(read_calls.iter().all(|call| {
        call.plugin_id == "vulcan-usage-statistics"
            && call.operation == VulcanPluginVldbOperation::SqliteQuery
            && call.payload["binding"] == "system.statistics"
    }));
    assert_eq!(
        clear_call.operation,
        VulcanPluginVldbOperation::SqliteExecute
    );
    assert_eq!(clear_call.payload["binding"], "system.statistics");
    drop(calls);
    // unauthorizedSnapshot removes only the VLDB capability while preserving the enabled runtime lease.
    // unauthorizedSnapshot 仅移除 VLDB 能力，同时保留已启用的运行时租约。
    let unauthorized_snapshot = {
        let snapshot = manager
            .state
            .snapshot
            .read()
            .expect("read authorized statistics catalog");
        let mut unauthorized = snapshot.as_ref().clone();
        let plugin = unauthorized
            .plugins
            .get_mut("vulcan-usage-statistics")
            .expect("resolve mutable statistics descriptor");
        let mut manifest = plugin
            .manifest
            .as_ref()
            .expect("resolve authorized statistics manifest")
            .as_ref()
            .clone();
        manifest
            .permissions
            .retain(|permission| *permission != VulcanPluginPermission::SystemVldbAccess);
        plugin.manifest = Some(Arc::new(manifest));
        Arc::new(unauthorized)
    };
    *manager
        .state
        .snapshot
        .write()
        .expect("install unauthorized statistics catalog") = unauthorized_snapshot;
    // capabilityError proves an enabled plugin cannot use the binding without manifest authorization.
    // capabilityError 证明已启用插件在未经清单授权时不能使用该绑定。
    let capability_error = runtime
        .block_on(manager.invoke_action(VulcanPluginActionRequest {
            plugin_id: String::from("vulcan-usage-statistics"),
            action_id: String::from("dashboard-get"),
            arguments: json!({}),
            caller: crate::VulcanPluginActionCaller::PluginUi,
            workspace_root: repository_root.to_path_buf(),
            reuse_enabled_runtime: true,
        }))
        .expect_err("reject statistics VLDB access without capability");
    assert!(capability_error.message.contains("no longer authorizes"));
    assert!(
        capability_error
            .message
            .contains("vulcan.vldb.sqlite.query.v1")
    );
    assert_eq!(
        vldb.calls
            .lock()
            .expect("no call after permission withdrawal")
            .len(),
        recorded_calls
    );
}

/// Verifies the repository Context7 bundle loads through the LuaSkills typed JSON surface.
/// 验证仓库 Context7 Bundle 通过 LuaSkills 强类型 JSON 表面完成加载。
#[test]
fn bundled_context7_plugin_loads_with_typed_json_surface() {
    // temp owns the isolated fixed bridge lease used by this load-only check.
    // temp 持有当前仅加载检查使用的隔离固定桥租约。
    let temp = tempfile::tempdir().expect("create Context7 plugin test directory");
    // runtimeRoot is the deterministic Vulcan Plugin runtime directory.
    // runtimeRoot 是确定性的 Vulcan Plugin 运行时目录。
    let runtime_root = temp.path().join("runtime");
    fs::create_dir_all(&runtime_root).expect("create Context7 test runtime root");
    // repositoryRoot identifies the real checked-in Context7 package.
    // repositoryRoot 标识仓库内真实的 Context7 插件包。
    let repository_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("resolve repository root");
    // manager loads the production catalog and fixed bridge sources.
    // manager 加载生产插件目录与固定桥源码。
    let manager = test_manager(&runtime_root, repository_root.join("plugins"));
    // Match production bootstrap: attach the parent before checking version-two admission.
    // 对齐生产启动顺序：检查第二版入场前先附加父级。
    manager
        .initialize_formal_runtime(crate::VulcanPluginRuntimeSettings::default())
        .expect("attach formal Context7 probe parent");
    // disabled selects the real package for a host-owned enabling probe, not a Hook-only no-op.
    // disabled为真实插件选择宿主持有的启用探测，而不是仅Hook预热的空操作。
    let disabled = BTreeSet::from([String::from("context7")]);
    // catalog keeps the exact manifest whose Lua declaration must actually be loaded.
    // catalog保留必须实际加载Lua声明的精确清单。
    let catalog = manager
        .refresh_catalog(&disabled, &BTreeSet::new())
        .expect("load disabled Context7 plugin catalog");
    // plugin proves the probe cannot succeed through the content-only early return.
    // plugin证明探测不能通过内容型插件提前返回而空通过。
    let plugin = catalog
        .plugins
        .get("context7")
        .expect("resolve Context7 package");
    assert!(
        plugin
            .manifest
            .as_ref()
            .expect("resolve Context7 manifest")
            .has_lua_runtime()
    );
    manager
        .begin_enabling("context7")
        .expect("begin Context7 enabling probe");
    // runtime executes real Context7 initialization through the attached formal parent.
    // runtime 通过已附加正式父级执行真实 Context7 初始化。
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .expect("create Context7 Tokio runtime");
    runtime
        .block_on(manager.probe_plugin("context7"))
        .expect("load and close the real Context7 probe lease");
}
