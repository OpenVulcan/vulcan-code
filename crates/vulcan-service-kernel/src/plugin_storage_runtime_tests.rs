//! Isolated real-engine plugin loading and query-stage measurements.
//! 隔离真实引擎的插件加载与查询分段测量。

use super::PluginStorageService;
use crate::plugin_vldb::PluginVldbService;
use luaskills::{LuaEngine, LuaEngineOptions, LuaRuntimeHostOptions, LuaVmPoolConfig};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};
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

/// Opens Context7's credential and usage views with fresh UI runtimes and isolated SQLite files.
/// 用全新的界面运行时和隔离 SQLite 文件打开 Context7 的凭据与用量视图。
/// Takes no arguments; asserts cold initialization and preservation of existing encrypted data.
/// 无参数；断言冷启动初始化成功且已有加密数据保持不变。
#[test]
#[ignore = "Requires VULCAN_PLUGIN_TEST_LUA_RUNTIME pointing to installed native Lua packages"]
fn context7_credential_and_usage_open_from_cold_storage() {
    // Use the installed native packages so the test also exercises OpenSSL module loading.
    // 使用已安装的原生包，使测试同时覆盖 OpenSSL 模块加载。
    let packaged_runtime = std::env::var_os("VULCAN_PLUGIN_TEST_LUA_RUNTIME")
        .map(std::path::PathBuf::from)
        .expect("set VULCAN_PLUGIN_TEST_LUA_RUNTIME to the installed native Lua runtime");
    // The repository supplies the production plugin and bridge sources.
    // 仓库提供生产插件与桥接源码。
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    // Each case starts with independent storage and an unused persistent UI instance.
    // 每个场景都从独立存储与尚未使用的持久界面实例开始。
    for (case, prepare_schema, store_key) in [
        ("fresh", false, false),
        ("missing credential", true, false),
        ("existing credential and usage", true, true),
    ] {
        // All state, including any generated test key material, lives under this root.
        // 全部状态（包括生成的测试密钥材料）均位于此临时根目录。
        let temporary = tempfile::tempdir().unwrap();
        // The native host needs a path understood by its Lua module loader.
        // 原生宿主需要 Lua 模块加载器能识别的路径。
        let root = temporary.path();
        // Copy only the native payload required by the production plugin.
        // 仅复制生产插件需要的原生载荷。
        for directory in ["libs", "lua_packages"] {
            copy_runtime_directory(&packaged_runtime.join(directory), &root.join(directory));
        }
        // The bridge is sourced from the same checkout as the plugin.
        // 桥接与插件使用同一个检出目录的源码。
        let bridge = root.join("system_lua_lib/vulcan-plugin-bridge");
        copy_runtime_directory(
            &repository.join("plugin-bridge/vulcan-plugin-bridge"),
            &bridge,
        );
        for directory in ["user_plugins", "managed_runtime"] {
            fs::create_dir_all(root.join(directory)).unwrap();
        }
        // The native SQLite adapter posts work back to this separate executor.
        // 原生 SQLite 适配器将工作提交给这个独立执行器。
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .unwrap();
        // Bind every database used by the host to the disposable fixture.
        // 将宿主使用的全部数据库绑定到可丢弃的测试夹具。
        let host = HostRuntimeHandle::new_for_test(HostRuntimeMetadata {
            data_root: root.to_path_buf(),
            sqlite_path: root.join("app.sqlite3"),
            sqlite_binding_id: "context7-test-app".into(),
            system_statistics_sqlite_path: root.join("statistics.sqlite3"),
            system_statistics_sqlite_binding_id: "context7-test-statistics".into(),
        });
        // The real adapter applies migration and query policy to the plugin's own SQLite file.
        // 真实适配器对插件自己的 SQLite 文件执行迁移与查询规则。
        let storage = Arc::new(PluginStorageService::new(
            &host,
            runtime.handle().clone(),
            root.join("plugin_state"),
        ));
        // Use the same embedded Lua engine as production.
        // 使用与生产环境相同的嵌入式 Lua 引擎。
        let engine = LuaEngine::new(LuaEngineOptions::new(
            LuaVmPoolConfig {
                min_size: 1,
                max_size: 1,
                idle_ttl_secs: 60,
            },
            LuaRuntimeHostOptions::with_runtime_root(vulcan_runtime_assets::native_host_path(root)),
        ))
        .unwrap();
        // Load the bundled plugin by its real manifest and expose its real Action schemas.
        // 按真实清单加载内置插件，并使用真实 Action Schema。
        let manager = VulcanPluginRuntimeManager::new(
            Arc::new(engine),
            repository.join("plugins"),
            root.join("user_plugins"),
            bridge,
            root.join("managed_runtime"),
            root.join("plugin_state"),
            root.join("plugin_runtime"),
        );
        manager
            .initialize_formal_runtime(vulcan_plugin_runtime::VulcanPluginRuntimeSettings::default())
            .unwrap();
        manager
            .install_host_services(VulcanPluginHostServices {
                storage: Some(storage.clone()),
                vldb: None,
                ui_events: Some(Arc::new(
                    crate::plugin_ui_events::ServicePluginUiEventPublisher::new(
                        vulcan_service_api::ServiceEventEmitter::new(Vec::new()),
                    ),
                )),
            })
            .unwrap();
        manager
            .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
            .unwrap();
        // Run the declared Action with explicit arguments; reuse selects the real UI path for the assertions.
        // 以明确参数运行声明的 Action；reuse 为断言选择真实界面路径。
        let invoke = |action_id: &str, arguments, reuse_enabled_runtime| {
            runtime
                .block_on(manager.invoke_action(VulcanPluginActionRequest {
                    plugin_id: "context7".into(),
                    action_id: action_id.into(),
                    arguments,
                    caller: VulcanPluginActionCaller::PluginUi,
                    workspace_root: root.to_path_buf(),
                    reuse_enabled_runtime,
                }))
                .unwrap_or_else(|error| panic!("{case}: {action_id}: {error}"))
        };
        // Seed only fixture rows; run each statement through the real storage adapter.
        // 只填充夹具数据；每条语句通过真实存储适配器执行。
        let execute = |sql: &str| {
            runtime
                .block_on(storage.sqlite_execute(
                    "context7",
                    json!({ "sql": sql, "params": [] }),
                    None,
                ))
                .unwrap();
        };
        // Capture the complete encrypted envelope without exposing its values in test output.
        // 捕获完整加密信封，避免在测试输出中显示其中的值。
        let stored_credential = || {
            runtime.block_on(storage.sqlite_query(
                "context7",
                json!({ "sql": "SELECT * FROM context7_credential WHERE singleton_id = 1", "params": [] }),
                None,
            )).unwrap()
        };
        if prepare_schema {
            // Seed via a one-shot Action so the persistent UI VM has never initialized storage.
            // 用一次性 Action 准备数据，使持久界面虚拟机仍未初始化存储。
            if store_key {
                invoke(
                    "credential-set",
                    json!({ "apiKey": "ctx7sk-local-fixture-only" }),
                    false,
                );
                execute(
                    r#"
INSERT INTO context7_request_events
  (request_id, operation, auth_mode, credential_generation, status, duration_ms, created_at_ms)
SELECT 'fixture', 'query_docs', 'api_key', credential_generation, 'success', 27, unixepoch('now') * 1000
FROM context7_credential
"#,
                );
                execute(
                    r#"
INSERT INTO context7_quota_snapshot
  (auth_mode, credential_generation, quota_limit, quota_remaining, http_status, observed_at_ms)
SELECT 'api_key', credential_generation, 100, 73, 200, unixepoch('now') * 1000
FROM context7_credential
"#,
                );
            } else {
                invoke("usage-get", json!({ "rangeDays": 30 }), false);
                execute("DELETE FROM context7_credential WHERE singleton_id = 1");
                assert_eq!(stored_credential()["row_count"], 0);
            }
        }
        // Compare the entire stored row, including wrapping key, nonce, ciphertext and tag.
        // 比较完整存储行，包括包装密钥、随机数、密文和标签。
        let credential_before = store_key.then(stored_credential);
        // Run the credential read first, as it can be the first action on the settings page.
        // 先读取凭据状态，因为它可能是设置页的首个 Action。
        let status = invoke("credential-status", json!({}), true);
        // The usage view must agree with the credential view and retain existing history and quota.
        // 用量视图必须与凭据视图一致，并保留已有历史与配额。
        let usage = invoke("usage-get", json!({ "rangeDays": 30 }), true);
        assert_eq!(status["configured"], store_key, "{case}");
        assert_eq!(
            status["mode"],
            if store_key { "api_key" } else { "anonymous" },
            "{case}"
        );
        assert_eq!(usage["credential"], status, "{case}");
        assert_eq!(
            usage["localStats"]["requests"],
            u64::from(store_key),
            "{case}"
        );
        if let Some(credential_before) = credential_before {
            assert!(
                stored_credential() == credential_before,
                "{case}: encrypted credential changed"
            );
            assert_eq!(usage["localStats"]["apiKeyRequests"], 1);
            assert_eq!(usage["quotaSnapshot"]["quotaLimit"], 100);
            assert_eq!(usage["quotaSnapshot"]["quotaRemaining"], 73);
            assert_eq!(
                usage["quotaSnapshot"]["credentialGeneration"],
                status["credentialGeneration"]
            );
        } else {
            assert_eq!(stored_credential()["row_count"], 1, "{case}");
            assert_eq!(status["credentialGeneration"], 0, "{case}");
            assert!(usage["quotaSnapshot"].is_null(), "{case}");
        }
        manager.close_formal_runtime_blocking().unwrap();
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
        Arc::new(engine),
        repository.join("plugins"),
        root.join("user_plugins"),
        bridge,
        root.join("managed_runtime"),
        root.join("plugin_state"),
        root.join("plugin_runtime"),
    );
    // Plugin actions execute only through the manager-owned formal runtime.
    // 插件 Action 只能通过管理器持有的正式运行时执行。
    manager
        .initialize_formal_runtime(vulcan_plugin_runtime::VulcanPluginRuntimeSettings::default())
        .unwrap();
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
                            "usage-get"
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
                assert_eq!(result["localStats"]["requests"], 0);
                assert!(result["quotaSnapshot"].is_null());
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
 dispatched_at_ms, first_token_at_ms, terminal_at_ms, status, dispatched, dispatched_known, semantic_output_observed,
 usage_availability, input_tokens, uncached_input_tokens, cache_read_tokens, cache_write_tokens,
 output_tokens, total_tokens, calendar_date)
SELECT 'sample-' || n, 'request-' || n, 0, 'fixture-' || (n % 3), 'conversation',
 'provider-' || (n % 8), 'entry', 'connection', 'profile', 'gpt-5',
 CASE WHEN n % 3 = 0 THEN 'gpt-5' ELSE 'fixture-model-' || (n % 80) END,
 (unixepoch('now') - (n % 28) * 86400) * 1000 - 1000,
 (unixepoch('now') - (n % 28) * 86400) * 1000 - 500,
 (unixepoch('now') - (n % 28) * 86400) * 1000,
 CASE WHEN n % 10 = 0 THEN 'failed' ELSE 'succeeded' END, 1, 1, 1,
 'final', 1000, 800, 200, 0, 200, 1200, date('now', '-' || (n % 28) || ' days') FROM samples
"#,
                            [rows],
                        )
                        .map_err(|error| error.to_string())?;
                    Ok(())
                },
            ))
            .unwrap();
        for view in ["overview", "providers", "recent"] {
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
