//! Isolated real-engine plugin loading and query-stage measurements.
//! 隔离真实引擎的插件加载与查询分段测量。

use super::PluginStorageService;
use crate::plugin_vldb::PluginVldbService;
use luaskills::{LuaEngine, LuaEngineOptions, LuaRuntimeHostOptions, LuaVmPoolConfig};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex, RwLock};
use std::time::Instant;
use vulcan_plugin_runtime::{
    VulcanPluginActionCaller, VulcanPluginActionRequest, VulcanPluginHostServices,
    VulcanPluginRuntimeManager, VulcanPluginStorageRequest, VulcanPluginStorageService,
    VulcanPluginVldbRequest, VulcanPluginVldbService,
};
use vulcan_workspace_store::data_plane::{HostRuntimeHandle, HostRuntimeMetadata};

/// Exercises named binding through the real plugin policy, payload decoder and SQLite query engine.
/// 通过真实插件策略、载荷解码器与 SQLite 查询引擎验证命名绑定。
/// No parameters; verifies returned values using test-owned database files and never opens user data.
/// 无参数；使用测试自有数据库文件验证返回值，不打开用户数据。
#[test]
fn named_parameters_keep_their_value_through_plugin_sqlite_query() {
    // temporary owns every file used by this test and outlives all database handles.
    // temporary 持有本测试全部文件，其生命周期长于所有数据库句柄。
    let temporary = tempfile::tempdir().unwrap();
    // runtime drives only this isolated database query path.
    // runtime 仅驱动此隔离数据库查询路径。
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    // host maps all logical databases into the temporary root.
    // host 将全部逻辑数据库映射到临时根目录。
    let host = HostRuntimeHandle::new_for_test(HostRuntimeMetadata {
        data_root: temporary.path().to_path_buf(),
        sqlite_path: temporary.path().join("app.sqlite3"),
        sqlite_binding_id: "named-parameter-app".into(),
        system_statistics_sqlite_path: temporary.path().join("statistics.sqlite3"),
        system_statistics_sqlite_binding_id: "named-parameter-statistics".into(),
    });
    // storage is the production adapter with a test-owned plugin state directory.
    // storage 是使用测试自有插件状态目录的生产适配器。
    let storage = PluginStorageService::new(
        &host,
        runtime.handle().clone(),
        temporary.path().join("plugins"),
    );
    runtime.block_on(async {
        storage
            .ensure_sqlite("named-parameter-audit")
            .await
            .unwrap();
        for sql in [
            "SELECT :case AS value",
            "SELECT @case AS value",
            "SELECT $case AS value",
            "SELECT $scope::case(END-COMMIT) AS value",
            "SELECT CASE WHEN :case > 0 THEN :case ELSE 0 END AS value",
        ] {
            // result passes the exact SQL and ordered tagged value through production decoding.
            // result 将精确 SQL 与有序标记值交给生产解码路径。
            let result = storage
                .sqlite_query(
                    "named-parameter-audit",
                    json!({
                        "sql": sql, "params": [{"kind": "int64", "value": 7}],
                    }),
                    None,
                )
                .await
                .unwrap_or_else(|error| panic!("{sql}: {error}"));
            assert_eq!(
                result,
                json!({"rows": [{"value": 7}], "row_count": 1}),
                "{sql}"
            );
        }
    });
}

/// Copies packaged native libraries into the test-owned runtime without touching their source.
/// 将已打包原生库复制到测试自有运行时，不修改来源。
/// source is a read-only package directory; destination belongs to the unique temporary root.
/// source 是只读包目录；destination 属于唯一临时根目录。
/// Returns normally after every file is copied; filesystem failures fail the integration test.
/// 全部文件复制后正常返回；文件系统错误使集成测试失败。
fn copy_runtime_directory(source: &Path, destination: &Path) {
    fs::create_dir_all(destination).unwrap();
    for entry in fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let destination = destination.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_runtime_directory(&entry.path(), &destination);
        } else {
            fs::copy(entry.path(), destination).unwrap();
        }
    }
}

/// TimedVldb records production hostcall latency while retaining the real SQL execution service.
/// TimedVldb 在保留真实 SQL 执行服务的同时记录生产 Hostcall 延迟。
struct TimedVldb {
    /// Real registered-binding service.
    /// 真实注册绑定服务。
    service: PluginVldbService,
    /// Per-call operation and elapsed milliseconds.
    /// 每次调用的操作及耗时毫秒。
    timings: Mutex<Vec<Value>>,
}

impl VulcanPluginVldbService for TimedVldb {
    /// Executes the exact authenticated request and records its real wall-clock cost.
    /// 执行精确已认证请求并记录真实墙钟成本。
    /// request is the production request; returns the unchanged production result.
    /// request 是生产请求；返回未经修改的生产结果。
    fn execute(&self, request: VulcanPluginVldbRequest) -> Result<Value, String> {
        // started covers binding resolution, engine queueing, SQL and JSON projection.
        // started 覆盖绑定解析、引擎排队、SQL 与 JSON 投影。
        let started = Instant::now();
        let result = self.service.execute(request);
        self.timings.lock().unwrap().push(json!({
            "elapsed_ms": started.elapsed().as_secs_f64() * 1000.0,
            "response_bytes": result.as_ref().map(|value| value.to_string().len()).unwrap_or(0),
        }));
        result
    }
}

/// TimedStorage retains production migration, credential isolation and SQL policy checks.
/// TimedStorage 保留生产迁移、凭据隔离与 SQL 策略检查。
struct TimedStorage {
    /// Real per-plugin storage service.
    /// 真实按插件存储服务。
    service: PluginStorageService,
    /// Operation kind and elapsed milliseconds.
    /// 操作类别与耗时毫秒。
    timings: Mutex<Vec<Value>>,
}

impl VulcanPluginStorageService for TimedStorage {
    /// Executes the production request while timing its queue, migration and SQL boundary.
    /// 执行生产请求，同时测量排队、迁移与 SQL 边界。
    /// request is authenticated; returns the unchanged service result.
    /// request 已经过认证；返回未经修改的服务结果。
    fn execute(&self, request: VulcanPluginStorageRequest) -> Result<Value, String> {
        let operation = format!("{:?}", request.operation);
        let started = Instant::now();
        let result = self.service.execute(request);
        self.timings.lock().unwrap().push(json!({
            "operation": operation, "elapsed_ms": started.elapsed().as_secs_f64() * 1000.0,
        }));
        result
    }
}

/// Measures actual bundled actions against new temporary native databases without external requests.
/// 对新临时原生数据库测量真实内置 Action，不进行外部请求。
/// No parameters; validates cold/hot empty results and prints exact stage timings.
/// 无参数；验证冷/热空结果，并打印精确分段耗时。
#[test]
#[ignore = "Requires VULCAN_PLUGIN_TEST_LUA_RUNTIME pointing to installed native Lua packages"]
fn real_plugin_empty_database_stage_measurements() {
    // temporary is the sole root for database, credential, bridge and plugin runtime writes.
    // temporary 是数据库、凭据、桥与插件运行时写入的唯一根目录。
    let temporary = tempfile::tempdir().unwrap();
    let root = temporary.path();
    // packaged_runtime is an explicitly supplied read-only native package source.
    // packaged_runtime 是显式提供的只读原生包来源。
    let packaged_runtime = std::env::var_os("VULCAN_PLUGIN_TEST_LUA_RUNTIME")
        .map(std::path::PathBuf::from)
        .expect("set VULCAN_PLUGIN_TEST_LUA_RUNTIME to the installed native Lua runtime");
    for directory in ["libs", "lua_packages"] {
        copy_runtime_directory(&packaged_runtime.join(directory), &root.join(directory));
    }
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap();
    let started = Instant::now();
    let host = HostRuntimeHandle::new_for_test(HostRuntimeMetadata {
        data_root: root.to_path_buf(),
        sqlite_path: root.join("app.sqlite3"),
        sqlite_binding_id: "isolated-plugin-app".into(),
        system_statistics_sqlite_path: root.join("statistics.sqlite3"),
        system_statistics_sqlite_binding_id: "isolated-plugin-statistics".into(),
    });
    runtime
        .block_on(vulcan_workspace_store::workspace_store::ensure_provider_usage_schema(&host))
        .unwrap();
    let schema_ms = started.elapsed().as_secs_f64() * 1000.0;
    let bridge = root.join("system_lua_lib/vulcan-plugin-bridge");
    fs::create_dir_all(bridge.join("runtime")).unwrap();
    fs::write(bridge.join("dependencies.yaml"), "{}\n").unwrap();
    fs::write(
        bridge.join("runtime/bridge.lua"),
        include_str!("../../../plugin-bridge/vulcan-plugin-bridge/runtime/bridge.lua"),
    )
    .unwrap();
    fs::create_dir_all(root.join("user_plugins")).unwrap();
    fs::create_dir_all(root.join("managed_runtime")).unwrap();
    let started = Instant::now();
    let engine = LuaEngine::new(LuaEngineOptions::new(
        LuaVmPoolConfig {
            min_size: 1,
            max_size: 1,
            idle_ttl_secs: 60,
        },
        LuaRuntimeHostOptions::with_runtime_root(vulcan_runtime_assets::native_host_path(
            &root.canonicalize().unwrap(),
        )),
    ))
    .unwrap();
    let engine_ms = started.elapsed().as_secs_f64() * 1000.0;
    let manager = VulcanPluginRuntimeManager::new(
        Arc::new(RwLock::new(engine)),
        repository.join("plugins"),
        root.join("user_plugins"),
        bridge,
        root.join("managed_runtime"),
        root.join("plugin_state"),
        root.join("plugin_runtime"),
    );
    let vldb = Arc::new(TimedVldb {
        service: PluginVldbService::new(host.clone(), runtime.handle().clone()),
        timings: Mutex::new(Vec::new()),
    });
    let storage = Arc::new(TimedStorage {
        service: PluginStorageService::new(
            &host,
            runtime.handle().clone(),
            root.join("plugin_state"),
        ),
        timings: Mutex::new(Vec::new()),
    });
    manager
        .install_host_services(VulcanPluginHostServices {
            storage: Some(storage.clone()),
            vldb: Some(vldb.clone()),
            ui_events: None,
        })
        .unwrap();
    let started = Instant::now();
    manager
        .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
        .unwrap();
    let catalog_ms = started.elapsed().as_secs_f64() * 1000.0;
    // measurements captures action wall time separately from the synchronous hostcall intervals.
    // measurements 将 Action 墙钟时间与同步 Hostcall 区间分别记录。
    let mut measurements = Vec::new();
    for plugin in ["context7", "vulcan-usage-statistics"] {
        let started = Instant::now();
        let prewarmed = runtime
            .block_on(manager.prewarm_plugin_runtime(plugin))
            .unwrap();
        measurements.push(
            json!({ "plugin": plugin, "stage": "hook_prewarm", "created": prewarmed,
            "elapsed_ms": started.elapsed().as_secs_f64() * 1000.0 }),
        );
        for pass in ["cold", "hot"] {
            vldb.timings.lock().unwrap().clear();
            storage.timings.lock().unwrap().clear();
            let started = Instant::now();
            let result = runtime
                .block_on(
                    manager.invoke_action(VulcanPluginActionRequest {
                        plugin_id: plugin.into(),
                        action_id: if plugin == "context7" {
                            "bootstrap-get"
                        } else {
                            "dashboard-get"
                        }
                        .into(),
                        arguments: if plugin == "context7" {
                            json!({ "rangeDays": 30 })
                        } else {
                            json!({ "rangeDays": 30, "view": "overview" })
                        },
                        caller: VulcanPluginActionCaller::PluginUi,
                        workspace_root: repository.to_path_buf(),
                        reuse_enabled_runtime: true,
                    }),
                )
                .unwrap();
            let elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
            if plugin == "context7" {
                assert_eq!(result["usage"]["localStats"]["requests"], 0);
                assert!(result["usageError"].is_null());
                assert!(result["usage"]["quotaSnapshot"].is_null());
            } else {
                assert_eq!(result["summary"]["attempts"], 0);
                assert!(result["summary"]["costs"][0]["amount"].is_null());
                // Seven reads include the two revision fences around the five overview queries.
                // 七次读取包含概览的五次查询，以及前后两次聚合版本一致性检查。
                assert_eq!(vldb.timings.lock().unwrap().len(), 7);
            }
            // hostcall_ms isolates synchronous service work from lease creation, dispatch and queue overhead.
            // hostcall_ms 将同步服务工作与租约创建、分发和排队开销分离。
            let hostcall_ms: f64 = vldb
                .timings
                .lock()
                .unwrap()
                .iter()
                .chain(storage.timings.lock().unwrap().iter())
                .map(|timing| timing["elapsed_ms"].as_f64().unwrap())
                .sum();
            measurements.push(json!({ "plugin": plugin, "stage": pass, "elapsed_ms": elapsed_ms,
                "hostcall_ms": hostcall_ms, "runtime_and_dispatch_ms": elapsed_ms - hostcall_ms,
                "response_bytes": result.to_string().len(),
                "vldb_calls": *vldb.timings.lock().unwrap(), "storage_calls": *storage.timings.lock().unwrap() }));
        }
    }
    // Scale only disposable fixture data; actual user statistics are never modified.
    // 仅扩展可丢弃夹具数据，绝不修改用户真实统计。
    for rows in [100_i64, 100_000] {
        runtime
            .block_on(vulcan_workspace_store::local_sql::with_connection_async(
                &host.local,
                vulcan_workspace_store::local_database_runtime::LogicalDatabase::SystemStatistics,
                move |connection| {
                    connection
                        .execute("DELETE FROM provider_usage_attempts_v3", [])
                        .map_err(|error| error.to_string())?;
                    connection
                        .execute(
                            r#"
WITH RECURSIVE samples(n) AS (SELECT 1 UNION ALL SELECT n + 1 FROM samples WHERE n < ?1)
INSERT INTO provider_usage_attempts_v3 (
 attempt_id, logical_request_id, attempt_index, consumer_id, operation, provider_id,
 service_entry_id, connection_id, protocol_profile_id, requested_model, mapped_model,
 dispatched_at_ms, first_token_at_ms, terminal_at_ms, status, dispatched, semantic_output_observed,
 usage_availability, input_tokens, uncached_input_tokens, cache_read_tokens, cache_write_tokens,
 output_tokens, total_tokens, calendar_date)
SELECT 'sample-' || n, 'request-' || n, 0, 'fixture-' || (n % 3), 'conversation',
 'provider-' || (n % 8), 'entry', 'connection', 'profile', 'gpt-5',
 CASE WHEN n % 3 = 0 THEN 'gpt-5' ELSE 'fixture-model-' || (n % 80) END,
 (unixepoch('now') - (n % 28) * 86400) * 1000 - 1000,
 (unixepoch('now') - (n % 28) * 86400) * 1000 - 500,
 (unixepoch('now') - (n % 28) * 86400) * 1000,
 CASE WHEN n % 10 = 0 THEN 'failed' ELSE 'succeeded' END, 1, 1,
 'final', 1000, 800, 200, 0, 200, 1200, date('now', '-' || (n % 28) || ' days') FROM samples
"#,
                            [rows],
                        )
                        .map_err(|error| error.to_string())?;
                    Ok(())
                },
            ))
            .unwrap();
        for view in ["overview", "details"] {
            vldb.timings.lock().unwrap().clear();
            let started = Instant::now();
            let result = runtime
                .block_on(manager.invoke_action(VulcanPluginActionRequest {
                    plugin_id: "vulcan-usage-statistics".into(),
                    action_id: "dashboard-get".into(),
                    arguments: json!({ "rangeDays": 30, "view": view }),
                    caller: VulcanPluginActionCaller::PluginUi,
                    workspace_root: repository.to_path_buf(),
                    reuse_enabled_runtime: true,
                }))
                .unwrap();
            if view == "overview" {
                assert_eq!(result["summary"]["attempts"], rows);
            }
            measurements.push(json!({ "plugin": "vulcan-usage-statistics", "rows": rows, "view": view,
                "elapsed_ms": started.elapsed().as_secs_f64() * 1000.0,
                "vldb_calls": *vldb.timings.lock().unwrap(), "response_bytes": result.to_string().len() }));
        }
    }
    eprintln!(
        "PLUGIN_STAGE_MEASUREMENTS {}",
        json!({ "schema_ms": schema_ms, "engine_ms": engine_ms,
        "catalog_ms": catalog_ms, "actions": measurements })
    );
}
