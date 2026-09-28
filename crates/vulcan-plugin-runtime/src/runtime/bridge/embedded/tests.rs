//! Real Lua coverage for the runtime-local native service adapter.
//! 运行时内原生服务适配器的真实 Lua 覆盖。

use super::*;
use crate::runtime::tests::{test_manager, write_hook_package};
use crate::{VulcanPluginStorageService, VulcanPluginUiEventPublisher, VulcanPluginVldbService};
use luaskills::LuaInvocationContext;
use luaskills::runtime::embedded::{
    EmbeddedCall, EmbeddedPluginConfig, ExecutionBackend, InstanceReuse, ModuleExport,
    OperationPhase, PoolKind,
};
use std::collections::BTreeSet;
use std::fs;
use tempfile::TempDir;

mod bundle;
mod workspaces;

/// One bounded initialization, invocation and cleanup budget for these local fixtures.
/// 这些本地夹具的统一有界初始化、调用及清理预算。
const WAIT: Duration = Duration::from_secs(5);

/// One parent byte budget supplies both fixture queue configuration and workspace-capacity probes.
/// 唯一父级字节预算同时提供夹具队列配置与工作区容量探测。
const QUEUED_BYTES: usize = 16384;

/// Services retain exact requests so tests can distinguish host identity from forged JSON.
/// 服务保留精确请求，使测试区分宿主身份与伪造 JSON。
#[derive(Default)]
struct RecordingServices {
    /// Captured isolated storage requests including their actual deadline.
    /// 捕获的隔离存储请求，包含实际截止时间。
    storage: Mutex<Vec<VulcanPluginStorageRequest>>,
    /// Captured host database requests including their actual deadline.
    /// 捕获的宿主数据库请求，包含实际截止时间。
    vldb: Mutex<Vec<VulcanPluginVldbRequest>>,
    /// Published plugin-local invalidations.
    /// 已发布的插件内失效事件。
    events: Mutex<Vec<VulcanPluginUiInvalidation>>,
}

impl VulcanPluginStorageService for RecordingServices {
    /// Record authenticated `request`; return its business payload to the calling real Lua module.
    /// 记录已认证 `request`；向调用的真实 Lua 模块返回其业务载荷。
    fn execute(&self, request: VulcanPluginStorageRequest) -> Result<Value, String> {
        // Clone only the business value while retaining the complete service request.
        // 仅克隆业务值，同时保留完整服务请求。
        let value = request.payload.clone();
        self.storage
            .lock()
            .expect("storage record lock")
            .push(request);
        Ok(value)
    }
}

impl VulcanPluginVldbService for RecordingServices {
    /// Record authenticated `request`; return a bounded test acknowledgement.
    /// 记录已认证 `request`；返回有界测试确认。
    fn execute(&self, request: VulcanPluginVldbRequest) -> Result<Value, String> {
        self.vldb.lock().expect("VLDB record lock").push(request);
        Ok(json!({"recorded":true}))
    }
}

impl VulcanPluginUiEventPublisher for RecordingServices {
    /// Record the exact host-scoped `event`; return after publication.
    /// 记录精确宿主作用域 `event`；发布后返回。
    fn publish(&self, event: VulcanPluginUiInvalidation) -> Result<(), String> {
        self.events.lock().expect("event record lock").push(event);
        Ok(())
    }
}

/// Retain actual package files, catalog, services and core owner for one isolated test.
/// 为一个隔离测试保留真实包文件、目录、服务及核心所有者。
struct Fixture {
    /// Core is dropped before the manager and physical temporary directory.
    /// 核心先于管理器及物理临时目录销毁。
    host: VulcanPluginEmbeddedHost,
    /// Owning manager remains alive throughout native service execution.
    /// 所属管理器在原生服务执行期间保持存活。
    manager: VulcanPluginRuntimeManager,
    /// Observable native services installed only in this manager.
    /// 仅在此管理器中安装的可观测原生服务。
    services: Arc<RecordingServices>,
    /// Physical test resources outlive the runtime's actual shutdown.
    /// 物理测试资源存活至运行时实际关闭之后。
    root: TempDir,
}

impl Fixture {
    /// Create real Lua package and manager; return a host with explicit small runtime budgets.
    /// 创建真实 Lua 包及管理器；返回具有显式小规模运行时预算的宿主。
    fn new() -> Self {
        // Each fixture has independent runtime, service and filesystem ownership.
        // 每个夹具有独立运行时、服务及文件系统所有权。
        let root = tempfile::tempdir().expect("create adapter fixture");
        // Keep this bundled fixture outside legacy System package roots.
        // 将此内置夹具保留在旧 System 包根之外。
        let bundled = root.path().join("bundled");
        write_hook_package(&bundled);
        // Extend the existing validated fixture with precise capability declarations.
        // 以精确能力声明扩展现有已校验夹具。
        let package = bundled.join("hook-plugin");
        // Appending distinct top-level YAML fields preserves the real manifest parser.
        // 追加不同顶层 YAML 字段，保留真实清单解析器。
        let mut manifest = fs::read_to_string(package.join("plugin.yaml")).expect("read manifest");
        manifest.push_str("permissions: [config, storage_sqlite, storage_lancedb, system_vldb_access, ui_host_context]\nconfig:\n  schema_file: config.schema.json\n  defaults_file: config.defaults.json\n");
        fs::write(package.join("plugin.yaml"), manifest).expect("write permissions");
        fs::write(package.join("config.schema.json"), r#"{"type":"object","required":["owner"],"properties":{"owner":{"type":"string"}},"additionalProperties":false}"#).expect("write schema");
        fs::write(
            package.join("config.defaults.json"),
            r#"{"owner":"initial"}"#,
        )
        .expect("write defaults");
        Self::from_catalog(root, bundled, "hook-plugin")
    }

    /// Open the real `bundled` catalog for `plugin_id` using filesystem ownership from `root`.
    /// 使用 `root` 的文件系统所有权为 `plugin_id` 打开真实 `bundled` 目录。
    /// Return an independent native host with explicitly bounded test budgets and recording services.
    /// 返回具有显式有界测试预算及记录服务的独立原生宿主。
    fn from_catalog(root: tempfile::TempDir, bundled: PathBuf, plugin_id: &str) -> Self {
        // Construct the actual isolated engine using the common runtime fixture.
        // 使用公共运行时夹具构造真实隔离引擎。
        let manager = test_manager(root.path(), bundled);
        manager
            .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
            .expect("publish catalog");
        assert_eq!(
            manager.snapshot().plugins[plugin_id].runtime_state,
            VulcanPluginRuntimeState::Enabled
        );
        // Service capture stays independent of process-global legacy callbacks.
        // 服务捕获独立于进程级旧回调。
        let services = Arc::new(RecordingServices::default());
        manager
            .install_host_services(VulcanPluginHostServices {
                storage: Some(services.clone()),
                vldb: Some(services.clone()),
                ui_events: Some(services.clone()),
            })
            .expect("install native services");
        // Parent limits are resolved here once, and plugin budgets are derived from them.
        // 父级限制在此一次性解析，插件预算从它们派生。
        let config = EmbeddedRuntimeConfig {
            max_registered_plugins: 2,
            max_registered_pools: 4,
            max_sessions: 4,
            max_registered_capabilities: CAPABILITIES.len(),
            max_resident_vms: 2,
            max_running_calls: 2,
            max_queued_calls: 4,
            max_queued_bytes: QUEUED_BYTES,
            max_operations: 16,
            max_effect_records_per_operation: 16,
            max_effect_bytes_per_operation: 16384,
            max_host_requests: 2,
            max_host_request_bytes: 16384,
            max_value_bytes: 4096,
        };
        // All named domains share this explicit plugin aggregate rather than separate engines.
        // 所有具名域共享此显式插件聚合预算，不建立独立引擎。
        let aggregate = EmbeddedPluginConfig {
            max_registered_pools: config.max_registered_pools,
            max_sessions: config.max_sessions,
            max_resident_vms: config.max_resident_vms,
            max_running_calls: config.max_running_calls,
            max_queued_calls: config.max_queued_calls,
            max_queued_bytes: config.max_queued_bytes,
            max_operations: config.max_operations,
        };
        // The capability time limit comes from the same fixture budget as calls and cleanup.
        // 能力时间上限来自与调用及清理相同的夹具预算。
        let host = VulcanPluginEmbeddedHost::new(
            &manager,
            config,
            WAIT.as_millis().try_into().expect("bounded duration"),
        )
        .expect("construct native host");
        host.runtime()
            .register_plugin(plugin_id.to_string(), aggregate)
            .expect("register aggregate");
        Self {
            host,
            manager,
            services,
            root,
        }
    }

    /// Build a trusted module declaration from the actual current catalog and `source`.
    /// 从实际当前目录及 `source` 构造可信模块声明。
    /// Return a definition with real canonical paths and an explicit object-valued call export.
    /// 返回具有真实规范路径及显式对象值调用导出的定义。
    fn definition(&self, source: &str) -> ModuleDefinition {
        // Freeze this catalog generation when constructing the test request.
        // 构造测试请求时冻结此目录代次。
        let snapshot = self.manager.snapshot();
        // The descriptor is the single source of plugin identity and dependencies.
        // 描述符是插件身份及依赖的唯一来源。
        let plugin = &snapshot.plugins["hook-plugin"];
        ModuleDefinition {
            finalizer: None,
            plugin_id: plugin.id.clone(),
            generation: plugin.generation.to_string(),
            package_root: super::super::super::environment::plugin_execution_package_root(
                &self.manager.state,
                plugin,
            )
            .expect("execution package")
            .to_string_lossy()
            .into_owned(),
            dependencies_file: plugin
                .manifest
                .as_ref()
                .expect("manifest")
                .execution
                .dependencies
                .manifest
                .clone()
                .expect("dependencies"),
            workspace_root: Some(
                self.root
                    .path()
                    .canonicalize()
                    .expect("workspace")
                    .to_string_lossy()
                    .into_owned(),
            ),
            cwd: None,
            mounts: json!({}),
            security_partition: "fixture-workspace".to_string(),
            source: source.to_string(),
            exports: vec![ModuleExport {
                name: "call".to_string(),
                input_schema: json!({"type":"object"}),
                output_schema: json!(true),
            }],
        }
    }

    /// Register trusted Lua `source`; return the actual core pool identifier.
    /// 注册可信 Lua `source`；返回真实核心池标识。
    fn pool(&self, source: &str) -> String {
        self.host
            .register_pool(
                self.definition(source),
                policy(),
                VulcanPluginBridgeLeaseScope::Enabled,
            )
            .expect("register host pool")
    }

    /// Submit `arguments` to the actual `pool`; return its terminal Lua value and effect evidence.
    /// 向真实 `pool` 提交 `arguments`；返回其终态 Lua 值及副作用证据。
    fn call(&self, pool: &str, arguments: Value) -> (Value, EffectState) {
        // Keep the real operation handle until terminal state is observed.
        // 保留真实操作句柄，直到观测到终态。
        let operation = self
            .host
            .runtime()
            .submit(
                EmbeddedCall {
                    pool_id: pool.to_string(),
                    export: "call".to_string(),
                    arguments,
                    context: LuaInvocationContext::default(),
                },
                WAIT,
            )
            .expect("submit real Lua");
        // Bounded waiting cannot turn an admission acknowledgement into a successful result.
        // 有界等待不能将入场确认变成成功结果。
        let result = operation.wait(WAIT).expect("wait actual Lua completion");
        assert_eq!(
            result.phase,
            OperationPhase::Succeeded,
            "real Lua failure: {result:?}"
        );
        self.host
            .runtime()
            .forget_operation(operation.id())
            .expect("forget terminal result");
        (result.value.expect("terminal Lua value"), result.effects)
    }

    /// Close this runtime and observe actual teardown before filesystem resources disappear.
    /// 关闭此运行时，并在文件系统资源消失前观测真实清理。
    fn close(&self) {
        self.host.runtime().request_close().expect("request close");
        // The bounded deadline applies to actual cleanup observation, not a sleep-based assertion.
        // 有界截止时间约束真实清理观测，不是基于睡眠的断言。
        let deadline = Instant::now() + WAIT;
        while !self.host.runtime().poll_closed().expect("poll close") {
            assert!(
                Instant::now() < deadline,
                "native host did not actually close"
            );
            std::thread::yield_now();
        }
    }
}

/// Return one explicit reusable serial domain policy for the small fixture aggregate.
/// 为小规模夹具聚合返回一个显式可复用串行域策略。
fn policy() -> PluginPoolConfig {
    PluginPoolConfig {
        kind: PoolKind::Shared,
        min_resident_vms: 0,
        max_resident_vms: 1,
        max_running_calls: 1,
        max_queued_calls: 4,
        reuse: InstanceReuse::Reusable,
        serial: true,
        backend: ExecutionBackend::InProcess,
        idle_ttl_ms: None,
        max_uses: None,
    }
}

/// Real Lua cannot replace native identity, loses revoked discovery, and retains unknown effects.
/// 真实 Lua 不能替换原生身份，撤权后失去发现能力，并保留未知副作用。
#[test]
fn native_host_binds_identity_deadline_and_revocation() {
    // The pool exposes only a named function and never evaluates per-call fragments.
    // 池仅暴露具名函数，绝不求值每次调用的片段。
    let fixture = Fixture::new();
    // Lua business data deliberately claims another plugin, runtime and scope.
    // Lua 业务数据故意声称其他插件、运行时及作用域。
    let pool = fixture.pool("return {call=function(a) return {visible=vulcan.host.has('vulcan.plugin.storage.sqlite.query'),response=vulcan.host.call('vulcan.plugin.storage.sqlite.query',a)} end}");
    // Record admission time to verify a real finite deadline reaches the service.
    // 记录入场时间，验证真实有限截止时间到达服务。
    let started = Instant::now();
    // Forged fields remain business data and cannot change the separate service identity.
    // 伪造字段仍为业务数据，不能改变独立服务身份。
    let forged = json!({"plugin_id":"victim","runtime_id":"victim-runtime","lease_scope":"lifecycle_install","workspace_root":"other-workspace"});
    // Native success preserves payload and conservative effect evidence.
    // 原生成功保留载荷及保守副作用证据。
    let (value, effects) = fixture.call(&pool, forged.clone());
    assert_eq!(value["visible"], true);
    assert_eq!(value["response"]["value"], forged);
    assert_eq!(value["response"]["effects"], "unknown");
    assert_eq!(effects, EffectState::Unknown);
    {
        // Only the actual registered plugin receives the original bounded operation deadline.
        // 只有实际注册插件取得原始有界操作截止时间。
        let calls = fixture
            .services
            .storage
            .lock()
            .expect("storage observations");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].plugin_id, "hook-plugin");
        assert_eq!(
            calls[0].operation,
            VulcanPluginStorageOperation::SqliteQuery
        );
        assert!(
            calls[0]
                .deadline
                .is_some_and(|deadline| deadline > started && deadline <= Instant::now() + WAIT)
        );
    }
    fixture
        .host
        .runtime()
        .revoke_pool_permission(&pool, VULCAN_PLUGIN_STORAGE_SQLITE_QUERY_TOOL)
        .expect("revoke native grant");
    // The same VM must immediately observe live revocation without capability fallback.
    // 同一 VM 必须立即观测实时撤权，不能回退到其他能力。
    let (denied, _) = fixture.call(&pool, json!({}));
    assert_eq!(denied["visible"], false);
    assert_eq!(denied["response"]["error"]["code"], "permission_denied");
    assert_eq!(
        fixture.services.storage.lock().expect("observations").len(),
        1
    );
    fixture.close();
}

/// Actual catalog refresh rejects retained old-generation callbacks and stale config writes.
/// 真实目录刷新拒绝保留旧代次回调及过期配置写入。
#[test]
fn native_host_rejects_changed_generation_before_service_entry() {
    // Keep a real reusable VM alive across the catalog content change.
    // 在目录内容变化期间保持真实可复用 VM 存活。
    let fixture = Fixture::new();
    // Its old source remains valid Lua, so failure must come from native authority validation.
    // 旧源码仍为有效 Lua，因此失败必须来自原生权威校验。
    let pool = fixture.pool("return {call=function(a) return vulcan.host.call('vulcan.plugin.storage.sqlite.query',a) end}");
    // Retain the full native envelope when reporting an admission failure.
    // 报告入场失败时保留完整原生信封。
    let initial = fixture.call(&pool, json!({})).0;
    assert_eq!(initial["ok"], true, "{initial}");
    // This exact descriptor is retained before real manifest content changes.
    // 在真实清单内容变化前保留此精确描述符。
    let original = fixture.manager.snapshot().plugins["hook-plugin"].clone();
    fixture
        .manager
        .update_plugin_config("hook-plugin", json!({"owner":"persisted"}))
        .expect("write current config");
    // New defaults change the actual package fingerprint without invalidating its schema.
    // 新默认值改变实际包指纹，同时保持其 Schema 有效。
    fs::write(
        original.package_root.join("config.defaults.json"),
        r#"{"owner":"updated"}"#,
    )
    .expect("change package content");
    fixture
        .manager
        .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
        .expect("refresh changed generation");
    assert_ne!(
        fixture.manager.snapshot().plugins["hook-plugin"].generation,
        original.generation
    );
    // A valid old VM cannot enter storage after the new generation has published.
    // 新代次发布后，有效旧 VM 不能进入存储。
    let (stale, _) = fixture.call(&pool, json!({"plugin_id":"hook-plugin"}));
    assert_eq!(stale["error"]["code"], "stale_generation");
    assert_eq!(stale["effects"], "not_started");
    assert_eq!(
        fixture.services.storage.lock().expect("observations").len(),
        1
    );
    assert!(
        fixture
            .manager
            .update_plugin_config_for_snapshot(&original, json!({"owner":"stale"}))
            .expect_err("reject stale config writer")
            .message
            .contains("generation changed")
    );
    assert_eq!(
        fixture
            .manager
            .plugin_config("hook-plugin")
            .expect("unchanged config"),
        json!({"owner":"persisted"})
    );
    // A fresh registration uses the actual new descriptor and can call the same service.
    // 新注册使用实际新描述符，可调用同一服务。
    let new_pool = fixture.pool("return {call=function(a) return vulcan.host.call('vulcan.plugin.storage.sqlite.query',a) end}");
    assert_eq!(fixture.call(&new_pool, json!({})).0["ok"], true);
    fixture.close();
}

/// Two formal hosts with identical plugin IDs cannot route to each other's services.
/// 两个具有相同插件 ID 的正式宿主不能路由到彼此的服务。
#[test]
fn native_host_isolates_services_and_releases_execution_authority() {
    // Independent managers intentionally share the same manifest plugin ID.
    // 独立管理器故意共享相同清单插件 ID。
    let first = Fixture::new();
    // Constructing another manager changes legacy global registration but not this formal host.
    // 构造另一管理器会改变旧全局注册，但不改变此正式宿主。
    let second = Fixture::new();
    // Both pools call configuration, host database and UI through native dispatch.
    // 两个池均通过原生分发调用配置、宿主数据库及 UI。
    let source = "return {call=function(a) local c=vulcan.host.call('vulcan.plugin.config.get',vulcan.json.object()); local d=vulcan.host.call('vulcan.vldb.sqlite.query.v1',a); local u=vulcan.host.call('vulcan.plugin.ui.invalidate',{topic='data',revision=1,plugin_id='victim'}); return {config=c,db=d,ui=u} end}";
    // The first registration is retained by its actual core pool.
    // 第一注册由其真实核心池保留。
    let first_pool = first.pool(source);
    // The second registration has an independent runtime namespace and owner.
    // 第二注册具有独立运行时命名空间及所有者。
    let second_pool = second.pool(source);
    first
        .manager
        .update_plugin_config("hook-plugin", json!({"owner":"first"}))
        .expect("write first config");
    second
        .manager
        .update_plugin_config("hook-plugin", json!({"owner":"second"}))
        .expect("write second config");
    assert_eq!(
        first.call(&first_pool, json!({})).0["config"]["value"]["owner"],
        "first"
    );
    assert_eq!(
        second.call(&second_pool, json!({})).0["config"]["value"]["owner"],
        "second"
    );
    assert_eq!(first.services.vldb.lock().expect("first database").len(), 1);
    assert_eq!(
        second.services.vldb.lock().expect("second database").len(),
        1
    );
    assert_eq!(
        first.services.events.lock().expect("first events")[0].plugin_id,
        "hook-plugin"
    );
    assert_eq!(
        second.services.events.lock().expect("second events")[0].plugin_id,
        "hook-plugin"
    );
    assert!(
        first
            .host
            .authority
            .domains
            .lock()
            .expect("live domains")
            .values()
            .all(|domain| domain.strong_count() > 0)
    );
    first.close();
    assert!(
        first
            .host
            .authority
            .domains
            .lock()
            .expect("retired domains")
            .values()
            .all(|domain| domain.upgrade().is_none())
    );
    assert_eq!(second.call(&second_pool, json!({})).0["db"]["ok"], true);
    second.close();
}

/// Failed registration does not leak authority, and malformed UI input never reaches a service.
/// 注册失败不泄漏权威，畸形 UI 输入绝不进入服务。
#[test]
fn native_host_rejects_invalid_registration_and_business_input() {
    // A real host starts with no native execution domain reservations.
    // 真实宿主开始时没有原生执行域预留。
    let fixture = Fixture::new();
    // Wrong generation must fail before core pool publication.
    // 错误代次必须在核心池发布前失败。
    let mut definition = fixture.definition("return {call=function(a) return a end}");
    definition.generation.push_str("-stale");
    assert_eq!(
        fixture
            .host
            .register_pool(definition, policy(), VulcanPluginBridgeLeaseScope::Enabled)
            .expect_err("reject generation")
            .code,
        EmbeddedErrorCode::StaleGeneration
    );
    // Invalid core policy fails after authority preparation and exercises reservation rollback.
    // 无效核心策略在权威准备后失败，验证预留回滚。
    let mut invalid_policy = policy();
    invalid_policy.max_running_calls = 0;
    assert!(
        fixture
            .host
            .register_pool(
                fixture.definition("return {call=function(a) return a end}"),
                invalid_policy,
                VulcanPluginBridgeLeaseScope::Enabled
            )
            .is_err()
    );
    assert!(
        fixture
            .host
            .authority
            .domains
            .lock()
            .expect("no leaked reservations")
            .is_empty()
    );
    // Absent process permission prevents both discovery and execution without calling legacy routing.
    // 缺失进程权限同时阻止发现与执行，不调用旧路由。
    let pool = fixture.pool("return {call=function(a) return {process=vulcan.host.has('vulcan.plugin.process.resolve'),event=vulcan.host.call('vulcan.plugin.ui.invalidate',a)} end}");
    // Derive the violating length from the same authoritative production limit.
    // 从同一权威生产限制派生越界长度。
    let (rejected, _) = fixture.call(
        &pool,
        json!({"topic":"x".repeat(UI_INVALIDATION_TOPIC_MAX_BYTES + 1),"revision":1}),
    );
    assert_eq!(rejected["process"], false);
    assert_eq!(rejected["event"]["error"]["code"], "invalid_argument");
    assert_eq!(rejected["event"]["effects"], "not_started");
    assert!(
        fixture
            .services
            .events
            .lock()
            .expect("no publication")
            .is_empty()
    );
    // The inclusive boundary remains usable after an explicit business-argument rejection.
    // 显式业务参数拒绝后，包含等号的边界仍可用。
    let (accepted, _) = fixture.call(
        &pool,
        json!({"topic":"x".repeat(UI_INVALIDATION_TOPIC_MAX_BYTES),"revision":1}),
    );
    assert_eq!(accepted["event"]["ok"], true);
    assert_eq!(
        fixture
            .services
            .events
            .lock()
            .expect("one publication")
            .len(),
        1
    );
    fixture.close();
}

/// Cancellation while native dispatch waits for service lookup prevents the later host effect.
/// 原生分发等待服务查询期间发生取消，会阻止随后宿主副作用。
#[test]
fn native_host_rechecks_cancellation_after_service_lock_wait() {
    // Use a real operation so cancellation and evidence are controlled by the core scheduler.
    // 使用真实操作，使取消及证据由核心调度器控制。
    let fixture = Fixture::new();
    // This module performs one native storage request after initialization.
    // 此模块在初始化后执行一次原生存储请求。
    let pool = fixture.pool("return {call=function(a) return vulcan.host.call('vulcan.plugin.storage.sqlite.query',a) end}");
    // Hold only this test manager's real service lookup lock, never a process-global resource.
    // 仅持有此测试管理器的真实服务查询锁，绝不持有进程全局资源。
    let service_gate = fixture
        .manager
        .state
        .host_services
        .write()
        .expect("pause service lookup");
    // Admission starts Lua normally but cannot reach storage while this gate is held.
    // 入场正常启动 Lua，但持有此门期间不能到达存储。
    let operation = fixture
        .host
        .runtime()
        .submit(
            EmbeddedCall {
                pool_id: pool,
                export: "call".to_string(),
                arguments: json!({}),
                context: LuaInvocationContext::default(),
            },
            WAIT,
        )
        .expect("submit blocked native call");
    // Observe the real running effect record instead of assuming a thread timing delay.
    // 观测真实运行中副作用记录，不假定线程计时延迟。
    let deadline = Instant::now() + WAIT;
    loop {
        // Stable operation status remains queryable while the native handler is blocked.
        // 原生处理器阻塞期间，稳定操作状态仍可查询。
        let snapshot = operation.snapshot().expect("inspect running native call");
        if snapshot
            .host_effects
            .iter()
            .any(|effect| effect.phase == luaskills::runtime::embedded::HostEffectPhase::Running)
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "native handler did not reach running state: {snapshot:?}"
        );
        std::thread::yield_now();
    }
    operation.cancel().expect("cancel original operation");
    drop(service_gate);
    // A stopped operation must still report the known absence of a service side effect.
    // 已停止操作仍必须报告确定未发生服务副作用。
    let terminal = operation.wait(WAIT).expect("wait cancelled native call");
    assert!(
        terminal.phase.is_terminal(),
        "cancellation did not finish: {terminal:?}"
    );
    assert_eq!(
        terminal.error.as_ref().expect("cancelled result").code,
        EmbeddedErrorCode::Cancelled
    );
    assert!(
        terminal
            .host_effects
            .iter()
            .all(|effect| effect.effects == EffectState::NotStarted)
    );
    assert!(
        fixture
            .services
            .storage
            .lock()
            .expect("no storage call")
            .is_empty()
    );
    fixture.close();
}

/// Catalog replacement during service lookup cannot authorize the retained old generation afterward.
/// 服务查询期间的目录替换不能在之后授权保留的旧代次。
#[test]
fn native_host_rechecks_generation_after_service_lock_wait() {
    // The real native request is delayed at service lookup while catalog publication remains free.
    // 真实原生请求在服务查询处延迟，而目录发布保持可用。
    let fixture = Fixture::new();
    // Reuse this real pool across the later catalog change.
    // 在后续目录变化期间复用此真实池。
    let pool = fixture.pool("return {call=function(a) return vulcan.host.call('vulcan.plugin.storage.sqlite.query',a) end}");
    // Prevent service entry without holding the independent catalog publication gate.
    // 不持有独立目录发布门，同时阻止服务入场。
    let service_gate = fixture
        .manager
        .state
        .host_services
        .write()
        .expect("pause service lookup");
    // Core admission and native dispatch run before the package refresh below.
    // 核心入场及原生分发在下方包刷新之前运行。
    let operation = fixture
        .host
        .runtime()
        .submit(
            EmbeddedCall {
                pool_id: pool,
                export: "call".to_string(),
                arguments: json!({}),
                context: LuaInvocationContext::default(),
            },
            WAIT,
        )
        .expect("submit blocked native call");
    // Observe actual native dispatch with a bounded control-plane deadline.
    // 使用有界控制面截止时间观测真实原生分发。
    let deadline = Instant::now() + WAIT;
    loop {
        // The host effect ledger remains live throughout service lookup contention.
        // 宿主副作用账本在服务查询竞争期间保持实时。
        let snapshot = operation.snapshot().expect("inspect native dispatch");
        if snapshot
            .host_effects
            .iter()
            .any(|effect| effect.phase == luaskills::runtime::embedded::HostEffectPhase::Running)
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "native handler did not run: {snapshot:?}"
        );
        std::thread::yield_now();
    }
    // Retain the physical package path before publication changes the live descriptor.
    // 在发布改变实时描述符前保留物理包路径。
    let package = fixture.manager.snapshot().plugins["hook-plugin"]
        .package_root
        .clone();
    fs::write(
        package.join("config.defaults.json"),
        r#"{"owner":"replaced-while-waiting"}"#,
    )
    .expect("change package bytes");
    fixture
        .manager
        .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
        .expect("publish new generation while native call waits");
    drop(service_gate);
    // Lua receives an explicit stale-generation result, without any storage execution.
    // Lua 收到显式过期代次结果，不发生任何存储执行。
    let terminal = operation.wait(WAIT).expect("wait stale native result");
    assert_eq!(terminal.phase, OperationPhase::Succeeded, "{terminal:?}");
    assert_eq!(
        terminal.value.as_ref().expect("Lua envelope")["error"]["code"],
        "stale_generation"
    );
    assert!(
        terminal
            .host_effects
            .iter()
            .all(|effect| effect.effects == EffectState::NotStarted)
    );
    assert!(
        fixture
            .services
            .storage
            .lock()
            .expect("no storage entry")
            .is_empty()
    );
    fixture.close();
}

/// Current permission withdrawal also wins while native dispatch waits for service resolution.
/// 原生分发等待服务解析期间，当前权限撤回也必须生效。
#[test]
fn native_host_rechecks_permissions_after_service_lock_wait() {
    // The real native request is delayed at service lookup while catalog publication remains free.
    // 真实原生请求在服务查询处延迟，而目录发布保持可用。
    let fixture = Fixture::new();
    // Reuse this real pool across the later catalog change.
    // 在后续目录变化期间复用此真实池。
    let pool = fixture.pool("return {call=function(a) return vulcan.host.call('vulcan.plugin.storage.sqlite.query',a) end}");
    // Prevent service entry without holding the independent catalog publication gate.
    // 不持有独立目录发布门，同时阻止服务入场。
    let service_gate = fixture
        .manager
        .state
        .host_services
        .write()
        .expect("pause service lookup");
    // Core admission and native dispatch run before the package refresh below.
    // 核心入场及原生分发在下方包刷新之前运行。
    let operation = fixture
        .host
        .runtime()
        .submit(
            EmbeddedCall {
                pool_id: pool,
                export: "call".to_string(),
                arguments: json!({}),
                context: LuaInvocationContext::default(),
            },
            WAIT,
        )
        .expect("submit blocked native call");
    // Observe actual native dispatch with a bounded control-plane deadline.
    // 使用有界控制面截止时间观测真实原生分发。
    let deadline = Instant::now() + WAIT;
    loop {
        // The host effect ledger remains live throughout service lookup contention.
        // 宿主副作用账本在服务查询竞争期间保持实时。
        let snapshot = operation.snapshot().expect("inspect native dispatch");
        if snapshot
            .host_effects
            .iter()
            .any(|effect| effect.phase == luaskills::runtime::embedded::HostEffectPhase::Running)
        {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "native handler did not run: {snapshot:?}"
        );
        std::thread::yield_now();
    }
    // Publish a permission-only withdrawal while preserving the exact generation and package identity.
    // 保留精确代次及包身份，同时发布仅权限撤回。
    let mut snapshot = fixture.manager.snapshot().as_ref().clone();
    // Change only the currently declared storage permission.
    // 仅改变当前声明的存储权限。
    let plugin = snapshot
        .plugins
        .get_mut("hook-plugin")
        .expect("current plugin");
    let mut manifest = plugin
        .manifest
        .as_ref()
        .expect("current manifest")
        .as_ref()
        .clone();
    manifest
        .permissions
        .retain(|permission| *permission != VulcanPluginPermission::StorageSqlite);
    plugin.manifest = Some(Arc::new(manifest));
    *fixture
        .manager
        .state
        .snapshot
        .write()
        .expect("publish permission withdrawal") = Arc::new(snapshot);
    drop(service_gate);
    // Lua receives an explicit permission-denied result, without any storage execution.
    // Lua 收到显式权限拒绝结果，不发生任何存储执行。
    let terminal = operation
        .wait(WAIT)
        .expect("wait permission-denied native result");
    assert_eq!(terminal.phase, OperationPhase::Succeeded, "{terminal:?}");
    assert_eq!(
        terminal.value.as_ref().expect("Lua envelope")["error"]["code"],
        "permission_denied"
    );
    assert!(
        terminal
            .host_effects
            .iter()
            .all(|effect| effect.effects == EffectState::NotStarted)
    );
    assert!(
        fixture
            .services
            .storage
            .lock()
            .expect("no storage entry")
            .is_empty()
    );
    fixture.close();
}
