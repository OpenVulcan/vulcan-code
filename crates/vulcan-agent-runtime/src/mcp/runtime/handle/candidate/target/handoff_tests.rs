//! Real selected MCP targets exercise original durable completion and same-lease business admission.
//! 真实已选 MCP 目标验证原持久完成及同租约业务入场。

use super::*;
mod predecessor;
use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;
use vulcan_plugin_runtime::{
    VulcanPluginActivationTransactionPhase, VulcanPluginConfigurationSelection,
    VulcanPluginCredentialBackend, VulcanPluginCredentialKey, VulcanPluginCredentialRevision,
    VulcanPluginCredentialValues, VulcanPluginGenerationStore, VulcanPluginRuntimeError,
    VulcanPluginUpdatePolicy,
};

/// Existing readiness budget also bounds real native resource retirement observations.
/// 既有就绪预算也约束真实原生资源退役观测。
const OBSERVE: Duration = MCP_PARTITION_READINESS_TIMEOUT;

/// These Hook declarations extend the known source manifest before immutable preparation.
/// 这些 Hook 声明在不可变准备之前扩展已知来源清单。
const COMPOSITE_HOOK_MANIFEST: &str = r#"lua_runtime:
  bundle: runtime/plugin.bundle.lua
  invoke_function: dispatch
  shutdown_function: shutdown
  hook_shutdown_function: shutdown_hooks
  max_active_sessions: 1
  hooks:
    - id: precheck
      point: conversation.turn.precheck
      mode: transform
      function: on_precheck
      failure_policy: stop
"#;

/// Real module initialization, ordinary calls and finalization expose retained VM state through native events.
/// 真实模块初始化、普通调用及关闭通过原生事件暴露保留的 VM 状态。
const COMPOSITE_HOOK_MODULE: &str = r#"
-- Verify the exact selected document through the production configuration capability.
-- 通过生产配置能力校验精确选定文档。
assert(type(vulcan.host.call('vulcan.plugin.config.get', {}).marker) == 'string')
-- One native effect distinguishes original initialization from replacement initialization.
-- 单个原生副作用区分原初始化及替代初始化。
vulcan.host.call('vulcan.plugin.ui.invalidate', {topic='composite-init',revision=0})
-- The module and its counter belong to the actual retained Hook VM.
-- 模块及其计数器属于实际保留的 Hook VM。
local M, count = {}, 1
-- No inputs are required; return a successful version-one empty result.
-- 不需要输入；返回成功版本一空结果。
function M.dispatch() return '{"ok":true,"result":{}}' end
-- Ordinary shutdown requires no input and returns success without Hook finalization effects.
-- 普通关闭不需要输入，返回成功且没有 Hook 关闭副作用。
function M.shutdown() return M.dispatch() end
-- The supplied invocation is handled in the same VM; return its continuing count as a typed Hook payload.
-- 在相同 VM 中处理给定调用；将连续计数作为强类型 Hook 载荷返回。
function M.on_precheck()
    count = count + 1
    return string.format('{"ok":true,"result":{"payload":{"value":{"count":%d}}}}', count)
end
-- No input is required; emit actual Hook retirement before returning version-one success.
-- 不需要输入；返回版本一成功之前发出实际 Hook 退役事件。
function M.shutdown_hooks()
    vulcan.host.call('vulcan.plugin.ui.invalidate', {topic='composite-retired',revision=0})
    return M.dispatch()
end
return M
"#;

/// Actual native Hook callbacks record committed initialization and finalization without simulated results.
/// 实际原生 Hook 回调记录已提交初始化及关闭，不模拟结果。
#[derive(Default)]
struct HookEvents {
    /// Real callback values survive all guarded publication and physical retirement observations.
    /// 真实回调值跨全部受保护发布及物理退役观测存续。
    values: Mutex<Vec<vulcan_plugin_runtime::VulcanPluginUiInvalidation>>,
}

impl vulcan_plugin_runtime::VulcanPluginUiEventPublisher for HookEvents {
    /// Commit the supplied actual callback event; return an explicit synchronization failure.
    /// 提交给定实际回调事件；返回明确同步失败。
    fn publish(
        &self,
        event: vulcan_plugin_runtime::VulcanPluginUiInvalidation,
    ) -> Result<(), String> {
        self.values
            .lock()
            .map_err(|error| error.to_string())?
            .push(event);
        Ok(())
    }
}

impl HookEvents {
    /// The supplied exact topic selects committed native effects; return their actual count.
    /// 给定精确主题选择已提交原生副作用；返回其实际数量。
    fn count(&self, topic: &str) -> usize {
        self.values
            .lock()
            .expect("actual native Hook events")
            .iter()
            .filter(|event| event.topic == topic)
            .count()
    }
}

/// Empty declarations must never read or mutate secret storage.
/// 空声明绝不能读取或修改秘密存储。
struct NoSecrets;
impl crate::mcp::PluginMcpSecretStore for NoSecrets {
    /// Reject undeclared reads for all supplied identities; return no fabricated value.
    /// 对全部给定身份拒绝未声明读取；不返回伪造值。
    fn read_secret(
        &self,
        _: &str,
        _: &str,
        _: &str,
        _: &vulcan_plugin_runtime::VulcanPluginCredentialBinding,
    ) -> Result<String, String> {
        Err(String::from("Unexpected fixture secret read."))
    }
}
impl VulcanPluginCredentialBackend for NoSecrets {
    /// Reject undeclared reads for supplied revision and key; return an explicit fixture error.
    /// 对给定修订及键拒绝未声明读取；返回明确夹具错误。
    fn read(
        &self,
        _: &str,
        _: &VulcanPluginCredentialRevision,
        _: &VulcanPluginCredentialKey,
    ) -> Result<Option<String>, String> {
        Err(String::from("Unexpected fixture secret read."))
    }
    /// Reject writes for supplied identity and value; no external state changes.
    /// 拒绝给定身份及值的写入；不改变外部状态。
    fn write(
        &self,
        _: &str,
        _: &VulcanPluginCredentialRevision,
        _: &VulcanPluginCredentialKey,
        _: &str,
    ) -> Result<(), String> {
        Err(String::from("Unexpected fixture secret write."))
    }
    /// Reject deletion for supplied identity; empty declarations own no external entry.
    /// 拒绝给定身份的删除；空声明不拥有外部条目。
    fn delete(
        &self,
        _: &str,
        _: &VulcanPluginCredentialRevision,
        _: &VulcanPluginCredentialKey,
    ) -> Result<(), String> {
        Err(String::from("Unexpected fixture secret deletion."))
    }
}

/// Registered host test adapter injects one real disk readback fault after acquiring actual native guards.
/// 已注册宿主测试适配器在取得实际原生守卫后注入单次真实磁盘读回故障。
struct ReadbackFailureAdapter<'a> {
    /// Same real target supplies original authorization and actual resources.
    /// 相同真实目标提供原授权及实际资源。
    target: &'a TargetPluginMcpRuntime,
    /// Exact production sidecar is restored by the test after the failed completion.
    /// 完成失败后由测试恢复精确生产附属回执。
    receipt_path: &'a std::path::Path,
}

impl VulcanPluginNativeMcpHandoffAdapter for ReadbackFailureAdapter<'_> {
    /// Guard lifetime remains bounded by actual registration despite intentional disk failure.
    /// 即使故意磁盘失败，守卫生命周期仍受实际注册约束。
    type Guard<'gate>
        = TargetPluginMcpHandoffGuard<'gate>
    where
        Self: 'gate;
    /// Borrow original explicitly registered authorization without creating a new host issuer.
    /// 借用原显式注册授权，不创建新宿主签发者。
    fn authority(&self) -> Option<&VulcanPluginRegisteredNativeMcpHostAuthority> {
        Some(&self.target.authority)
    }
    /// Original manager, transaction, catalog and registration build real guards; return them after the deliberate test fault.
    /// 原管理器、事务、目录及注册构建真实守卫；在故意测试故障后返回这些守卫。
    fn prepare<'gate>(
        &'gate self,
        manager: &VulcanPluginRuntimeManager,
        transaction: &VulcanPluginActivationTransaction,
        catalog: &Arc<VulcanPluginCatalogSnapshot>,
        registration: &'gate VulcanPluginActivationTargetRegistration<'gate>,
    ) -> Result<Self::Guard<'gate>, VulcanPluginRuntimeError> {
        // Native ownership remains fixed through the ensuing manager's actual durable readback.
        // 随后管理器实际持久读回期间保持原生所有权固定。
        let guard = self
            .target
            .prepare_handoff(manager, transaction, catalog, registration)
            .map_err(VulcanPluginRuntimeError::new)?;
        fs::write(self.receipt_path, b"invalid target receipt")
            .expect("inject actual receipt readback failure");
        Ok(guard)
    }
}

/// Registered host test adapter abandons actual preparation before any durable completion write.
/// 已注册宿主测试适配器在任何持久完成写入之前放弃实际准备。
struct AbandonCompletionAdapter<'a> {
    /// Same real target is cancelled explicitly by the test after guard abandonment.
    /// 守卫放弃后由测试显式取消相同真实目标。
    target: &'a TargetPluginMcpRuntime,
}

impl VulcanPluginNativeMcpHandoffAdapter for AbandonCompletionAdapter<'_> {
    /// Even an abandoned guard is scoped by actual registration rather than an escaping factory result.
    /// 即使放弃守卫，也由实际注册限定作用域，而非逃逸的工厂结果。
    type Guard<'gate>
        = TargetPluginMcpHandoffGuard<'gate>
    where
        Self: 'gate;
    /// Borrow original native authorization without inventing resource identity.
    /// 借用原原生授权，不杜撰资源身份。
    fn authority(&self) -> Option<&VulcanPluginRegisteredNativeMcpHostAuthority> {
        Some(&self.target.authority)
    }
    /// Exact original arguments acquire and dispose real guards; return only the explicit intentional abandonment error.
    /// 精确原参数取得并销毁真实守卫；仅返回明确故意放弃错误。
    fn prepare<'gate>(
        &'gate self,
        manager: &VulcanPluginRuntimeManager,
        transaction: &VulcanPluginActivationTransaction,
        catalog: &Arc<VulcanPluginCatalogSnapshot>,
        registration: &'gate VulcanPluginActivationTargetRegistration<'gate>,
    ) -> Result<Self::Guard<'gate>, VulcanPluginRuntimeError> {
        // No source or native guard can survive this borrowed registration scope through the failed adapter.
        // 失败适配器不能让任何来源或原生守卫超出此借用注册作用域。
        let guard = self
            .target
            .prepare_handoff(manager, transaction, catalog, registration)
            .map_err(VulcanPluginRuntimeError::new)?;
        drop(guard);
        Err(VulcanPluginRuntimeError::new(
            "Deliberately abandon original completion.",
        ))
    }
}

/// One source fixture keeps the original foreground owner separate from actual native target resources.
/// 单个来源夹具将原前台所有者与实际原生目标资源分离。
struct Fixture {
    /// One explicit native host authorization is reused for original source and recovery.
    /// 单次显式原生宿主授权用于原来源及恢复。
    native_host: VulcanPluginRegisteredNativeMcpHostAuthority,
    /// Same actual target controls one ready closed runtime and lease.
    /// 相同实际目标控制单个就绪关闭运行时及租约。
    target: TargetPluginMcpRuntime,
    /// Exact authoritative closed Arc is retained for final publication or rollback.
    /// 精确权威关闭 Arc 保留用于最终发布或回退。
    staged: Arc<VulcanPluginCatalogSnapshot>,
    /// Only this manager issues and confirms the original target route.
    /// 仅此管理器签发并确认原目标路由。
    manager: VulcanPluginRuntimeManager,
    /// Actual storage supplies the durable original request and startup receipt.
    /// 实际存储提供持久原请求及启动回执。
    store: VulcanPluginGenerationStore,
    /// Foreground ownership is dropped independently after confirmed handoff.
    /// 确认交接后独立释放前台所有权。
    transaction: VulcanPluginActivationTransaction,
    /// Original shared observation lineage survives publication.
    /// 原共享观察谱系跨发布存续。
    access: McpRuntimeAccess,
    /// Real children alone record startup PIDs in this marker.
    /// 仅真实子进程在此标记内记录启动 PID。
    marker: PathBuf,
    /// Actual Hook initialization and finalization observations remain independent from MCP startup markers.
    /// 实际 Hook 初始化及关闭观测独立于 MCP 启动标记。
    hook_events: Arc<HookEvents>,
    /// Resources are disposed before this fixture directory is removed.
    /// 此夹具目录移除前，先销毁资源。
    _temp: tempfile::TempDir,
}

/// Construct production candidate retirement, Published staging and one actual closed-ready target.
/// 构造生产候选退役、Published 暂存及单个实际关闭就绪目标。
/// No inputs; return actual process ownership and durable authority without Hook or service stubs.
/// 无输入；返回实际进程所有权及持久权威，不使用 Hook 或服务替身。
async fn fixture() -> Fixture {
    fixture_with_hooks(false).await
}

/// Hook presence is declared before immutable preparation; return actual original dual-backend ownership when requested.
/// Hook 存在性在不可变准备前声明；请求时返回实际原双后端所有权。
async fn fixture_with_hooks(with_hooks: bool) -> Fixture {
    // All source and runtime paths belong to this isolated store.
    // 全部来源及运行时路径属于此隔离存储。
    let temp = tempfile::tempdir().expect("target handoff fixture");
    // Separate installed and bundled namespaces preserve production precedence.
    // 分离已安装及内置命名空间，保留生产优先级。
    let user = temp.path().join("user");
    fs::create_dir(&user).expect("user root");
    // Empty bundled discovery cannot mask this selected target.
    // 空内置发现不能遮蔽此已选目标。
    let bundled = temp.path().join("bundled");
    fs::create_dir(&bundled).expect("bundled root");
    // Only a declared Hook target attaches the actual Lua engine and production bridge.
    // 仅声明 Hook 的目标附加实际 Lua 引擎及生产桥接。
    let manager = if with_hooks {
        // Exact bridge bytes are the same authority used by the existing host integration fixtures.
        // 精确桥接字节与既有宿主集成夹具使用相同权威。
        let bridge = temp.path().join("bridge");
        fs::create_dir_all(bridge.join("runtime")).expect("actual bridge layout");
        fs::write(bridge.join("dependencies.yaml"), "{}\n").expect("bridge dependencies");
        fs::write(
            bridge.join("runtime/bridge.lua"),
            include_str!(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../plugin-bridge/vulcan-plugin-bridge/runtime/bridge.lua"
            )),
        )
        .expect("actual bridge module");
        // Host path normalization follows existing real Lua engine bootstrap on Windows.
        // 宿主路径规范化遵循 Windows 上既有真实 Lua 引擎引导。
        let native_root = crate::runtime_assets::native_host_path(
            &temp.path().canonicalize().expect("canonical Lua root"),
        );
        // One engine pool is distinct from the core-owned formal Hook pool observed below.
        // 单个引擎池区别于下文观测的核心拥有正式 Hook 池。
        let engine = luaskills::LuaEngine::new(luaskills::LuaEngineOptions::new(
            luaskills::LuaVmPoolConfig {
                min_size: 1,
                max_size: 1,
                idle_ttl_secs: 60,
            },
            luaskills::LuaRuntimeHostOptions::with_runtime_root(native_root),
        ))
        .expect("actual Lua engine");
        VulcanPluginRuntimeManager::new(
            Arc::new(engine),
            bundled,
            user.clone(),
            bridge,
            temp.path().join("managed"),
            temp.path().join("state"),
            temp.path().join("runtime"),
        )
    } else {
        VulcanPluginRuntimeManager::without_runtime(
            bundled,
            user.clone(),
            temp.path().join("bridge"),
            temp.path().join("managed"),
            temp.path().join("state"),
            temp.path().join("runtime"),
        )
    };
    // Actual initialization effects are observed through the existing native host service.
    // 通过既有原生宿主服务观察实际初始化副作用。
    let hook_events = Arc::new(HookEvents::default());
    if with_hooks {
        manager
            .install_host_services(vulcan_plugin_runtime::VulcanPluginHostServices {
                storage: None,
                vldb: None,
                ui_events: Some(hook_events.clone()),
            })
            .expect("real Hook native event service");
        manager
            .initialize_formal_runtime(vulcan_plugin_runtime::VulcanPluginRuntimeSettings::default())
            .expect("one actual formal parent");
    }
    // Only this real child fixture records actual starts.
    // 仅此真实子进程夹具记录实际启动。
    let marker = temp.path().join("starts.txt");
    // Exact native package source is inspected before immutable preparation.
    // 精确原生插件包来源在不可变准备前接受检查。
    let source = temp.path().join("source/target-mcp");
    fs::create_dir_all(source.join("bin")).expect("package source");
    // Resolve and copy the actual Node executable, avoiding arbitrary child PATH execution.
    // 解析并复制实际 Node 可执行文件，避免任意子进程 PATH 执行。
    let node = std::process::Command::new("node")
        .args(["-p", "process.execPath"])
        .output()
        .expect("Node dependency");
    assert!(node.status.success(), "Node executable discovery failed");
    // Successful native dependency discovery supplies one exact executable path.
    // 成功原生依赖发现提供单个精确可执行路径。
    let node_path = String::from_utf8(node.stdout).expect("UTF-8 Node path");
    fs::copy(node_path.trim(), source.join("bin/server.bin")).expect("bundle real Node");
    fs::copy(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/owner-mcp.mjs"),
        source.join("server.mjs"),
    )
    .expect("bundle real MCP fixture");
    fs::write(source.join("dependencies.yaml"), "{}\n").expect("bundled dependencies");
    fs::write(
        source.join("plugin.yaml"),
        r#"schema_version: 1
id: target-mcp
version: 1.0.0
default_enabled: true
display:
  name: Actual target MCP
  capability: Original selected native target
permissions: [config, process_binary, process_session]
config:
  schema_file: schema.json
  defaults_file: defaults.json
execution:
  dependencies:
    manifest: dependencies.yaml
    install_mode: bundled
  native_binaries:
    - id: server
      targets:
        x86_64-pc-windows-msvc: bin/server.bin
        aarch64-pc-windows-msvc: bin/server.bin
        x86_64-unknown-linux-gnu: bin/server.bin
        aarch64-unknown-linux-gnu: bin/server.bin
        x86_64-apple-darwin: bin/server.bin
        aarch64-apple-darwin: bin/server.bin
mcp:
  servers:
    - id: actual
      name: Actual selected server
      scope: global
      transport:
        type: stdio
        launcher: {type: native_binary, binary_id: server}
        args: [{source: literal, value: server.mjs}]
        env:
          VULCAN_MCP_PROCESS_MARKER: {source: config, key: marker}
"#,
    )
    .expect("declared native package");
    if with_hooks {
        // Change the known source declaration before inspection; immutable target bytes remain untouched.
        // 检查之前改变已知来源声明；不可变目标字节保持不动。
        let manifest = fs::read_to_string(source.join("plugin.yaml"))
            .expect("original declared source")
            .replace(
                "permissions: [config, process_binary, process_session]",
                "permissions: [config, process_binary, process_session, ui_host_context]",
            );
        fs::write(
            source.join("plugin.yaml"),
            format!("{manifest}\n{COMPOSITE_HOOK_MANIFEST}"),
        )
        .expect("dual backend source declaration");
        fs::create_dir_all(source.join("runtime")).expect("Hook bundle directory");
        fs::write(
            source.join("runtime/plugin.bundle.lua"),
            COMPOSITE_HOOK_MODULE,
        )
        .expect("actual stateful Hook module");
    }
    fs::write(source.join("schema.json"), r#"{"type":"object","properties":{"marker":{"type":"string"}},"required":["marker"],"additionalProperties":false}"#).expect("config schema");
    // Exact selected document drives the child marker through production materialization.
    // 精确选定文档通过生产物化驱动子进程标记。
    let configuration = serde_json::json!({"marker": marker});
    fs::write(
        source.join("defaults.json"),
        serde_json::to_vec(&configuration).expect("defaults JSON"),
    )
    .expect("defaults");
    // Production package inspection supplies fingerprint and complete approved permissions.
    // 生产插件包检查提供指纹及完整已批准权限。
    let preview = manager
        .inspect_local_user_plugin(&source)
        .expect("inspect source");
    // Original durable update owner freezes desired policy before package preparation.
    // 原持久更新所有者在插件包准备前固定期望策略。
    let store = VulcanPluginGenerationStore::open(&user).expect("store");
    // This bounded target-only test has an explicitly absent predecessor.
    // 此有界仅目标测试具有明确缺失的前驱。
    let preparation = store
        .begin_activation_update_preparation(
            uuid::Uuid::new_v4(),
            &source,
            &preview.content_fingerprint,
            None,
            preview.permissions,
            VulcanPluginUpdatePolicy {
                explicitly_enabled: true,
                explicitly_disabled: false,
                predecessor_default_enabled: true,
            },
        )
        .expect("original update owner");
    // Actual immutable package bytes are copied under the same original request.
    // 实际不可变插件包字节在同一原请求下复制。
    let generation = store
        .prepare_activation_package(&preparation, &source)
        .expect("prepare immutable package");
    store
        .begin_activation_credential_staging(
            &preparation,
            &generation,
            &VulcanPluginCredentialValues::new(),
        )
        .expect("actual empty credential preparation")
        .write_and_finish(&NoSecrets, &VulcanPluginCredentialValues::new())
        .expect("credential retirement");
    // Consume early preparation only after actual credential writer retirement.
    // 仅实际凭据写入者退役后消费早期准备。
    let mut transaction = store
        .finish_activation_preparation(
            preparation,
            VulcanPluginConfigurationSelection::Document {
                value: configuration,
            },
        )
        .expect("original configured target");
    {
        // Candidate generation zero is genuinely validated and physically retired before commit.
        // 候选零代次在提交前接受真实校验并物理退役。
        let candidate = store
            .inspect_activation_candidate(&transaction)
            .expect("candidate inspection");
        // Sealed candidate ownership follows the actual temporary validation process.
        // 密封候选所有权跟随实际临时校验进程。
        let prepared = manager
            .prepare_mcp_activation_candidate(&candidate)
            .expect("candidate launcher");
        // Exact global materialization performs no public selection lookup.
        // 精确全局物化不执行公开选择查询。
        let configs = crate::mcp::candidate_global_mcp_server_configs(prepared, &NoSecrets)
            .expect("candidate definitions");
        CandidatePluginMcpRuntime::start(configs)
            .expect("actual candidate process")
            .validate_and_release()
            .await
            .expect("real candidate readiness and retirement");
    }
    store
        .commit_activation_transaction(&mut transaction)
        .expect("Published original selection");
    // Actual staged Enabling Arc issues the real routing generation exactly once.
    // 实际暂存 Enabling Arc 仅签发一次真实路由代次。
    let staged = manager
        .stage_activation_target_handoff(
            &transaction,
            &BTreeSet::new(),
            &BTreeSet::from([String::from("target-mcp")]),
        )
        .expect("actual closed stage");
    // Source retains the full selected target and actual OS environment lock.
    // 来源保留完整已选目标及实际操作系统环境锁。
    // Native Rust bootstrap explicitly authorizes this fixture host's real resource observations.
    // 原生 Rust 引导显式授权此夹具宿主的真实资源观测。
    let native_host = manager.register_native_mcp_host_adapter();
    let prepared = manager
        .prepare_mcp_activation_target(&transaction, &staged, &native_host)
        .expect("exact actual target authority");
    // Prepared configs cannot detach from original ownership.
    // 准备配置不能脱离原所有权。
    let configs = crate::mcp::target_global_mcp_server_configs(prepared, &NoSecrets)
        .expect("exact actual target configs");
    // Existing access tables observe every actual unpublished target.
    // 既有访问表观察每个实际未发布目标。
    let access = McpRuntimeAccess::new(McpRuntimeOverview::default(), BTreeMap::new());
    // Synchronous dispatch holds the exact short source-registration fence.
    // 同步派发持有精确短时来源注册屏障。
    let target =
        TargetPluginMcpRuntime::start(&access, &manager, &transaction, configs, &native_host)
            .expect("actual selected startup");
    // Concurrent observers acknowledge the same actual Ready attempt without allocating another writer or process.
    // 并发观察者确认相同实际 Ready 尝试，不分配另一写入者或进程。
    let (first_ready, second_ready) = tokio::join!(
        target.wait_until_ready(OBSERVE),
        target.wait_until_ready(OBSERVE)
    );
    first_ready.expect("first actual closed target readiness");
    second_ready.expect("second actual closed target readiness");
    target
        .verify_source("target-mcp:actual", &target.profile())
        .expect("same pure source consumed by the actual native target before dispatch");
    Fixture {
        native_host,
        target,
        staged,
        manager,
        store,
        transaction,
        access,
        marker,
        hook_events,
        _temp: temp,
    }
}

/// Same ready process becomes callable only after original combined Completed and releases native resources after drain.
/// 相同就绪进程仅在原组合 Completed 后可调用，并在排空后释放原生资源。
#[tokio::test]
async fn actual_target_completed_handoff_opens_same_lease_and_pid() {
    // Production target preparation provides all real original authorities.
    // 生产目标准备提供全部真实原权威。
    let mut fixture = fixture().await;
    // Retain only the pure source through completed handoff and physical retirement.
    // 跨完成交接及物理退役仅保留纯来源。
    let profile = fixture.target.profile();
    assert!(profile.same_source(&fixture.target.profile()));
    assert!(
        fixture
            .target
            .target
            .host_profile(&fixture.manager.register_native_mcp_host_adapter())
            .is_err()
    );
    // Compete with the exact OS request file derived by transactions/operations.rs.
    // 竞争 transactions/operations.rs 推导的精确操作系统请求文件。
    let request_lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(
            fixture
                ._temp
                .path()
                .join("user/.vulcan-generations/locks")
                .join(format!(
                    "target-mcp.transaction.{}.lock",
                    fixture.transaction.record().request_id()
                )),
        )
        .expect("actual original request lock");
    assert!(matches!(
        request_lock.try_lock(),
        Err(std::fs::TryLockError::WouldBlock)
    ));
    // Compete with the permanent environment file derived by environment/leases.rs.
    // 竞争 environment/leases.rs 推导的永久环境文件。
    let environment_lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(
            fixture
                ._temp
                .path()
                .join("runtime/.environment-leases/target-mcp.lock"),
        )
        .expect("actual original environment lock");
    assert!(matches!(
        environment_lock.try_lock(),
        Err(std::fs::TryLockError::WouldBlock)
    ));
    assert!(
        fixture
            .store
            .resume_activation_transaction("target-mcp", fixture.transaction.record().request_id())
            .is_err()
    );
    // One generation-zero validation child and one actual selected child have started.
    // 单个零代次校验子进程及单个实际已选子进程已启动。
    let starts = fs::read_to_string(&fixture.marker).expect("actual startup marker");
    assert_eq!(starts.lines().count(), 2);
    // Historical Ready records the same actual opaque owners observed by the native host.
    // 历史 Ready 记录原生宿主观察的相同实际不透明所有者。
    let receipt = fixture
        .store
        .target_mcp_initialization_receipt("target-mcp", fixture.transaction.record().request_id())
        .expect("strict actual startup receipt")
        .expect("actual startup exists");
    assert_eq!(
        receipt.phase(),
        vulcan_plugin_runtime::VulcanPluginTargetMcpInitializationPhase::Ready
    );
    assert_eq!(
        receipt.instance_ids(),
        Some(fixture.target.actual_instance_ids())
    );
    // Captured bindings use the same actual lease while business is still closed.
    // 业务仍关闭时，捕获绑定使用相同实际租约。
    let bindings = fixture
        .target
        .prepared_tool_bindings_filtered(&McpRuntimeToolAllowlist::default())
        .expect("prepare actual handlers");
    // Semantic lookup protects against changing fixture catalog ordering.
    // 语义查找防止夹具目录顺序变化。
    let binding = bindings
        .iter()
        .find(|binding| binding.origin_tool_name() == Some("read_page"))
        .expect("read_page declaration may have changed");
    // Business arguments are exact fixture schema values.
    // 业务参数为精确夹具 Schema 值。
    let args =
        serde_json::json!({"url":"https://actual-target.test","owner_session_id":"fixture-owner"});
    assert!(binding.invoke(args.clone()).await.is_err());
    // Build the full extension's target access using its same actual lease and current partition map.
    // 使用相同实际租约及当前分区映射，构建完整扩展的目标访问。
    let published = fixture
        .target
        .publication_access(&fixture.access)
        .expect("actual publication access");
    // Synchronous publisher observes actual ready handlers but cannot invoke closed business.
    // 同步发布器观察实际就绪处理器，但不能调用关闭业务。
    let final_catalog = fixture
        .manager
        .publish_activation_target_handoff(&fixture.transaction, &fixture.staged, |_| Ok(()))
        .expect("closed extension publication");
    // The actual source alone must retain the original closed Arc after the caller releases its metadata.
    // 调用方释放其元数据后，实际来源自身必须保留原关闭 Arc。
    drop(fixture.staged);
    assert!(
        published
            .available_tool_bindings_filtered(&McpRuntimeToolAllowlist::default())
            .is_empty()
    );
    // Remove only the genuine input observation to prove readiness and transport presence cannot substitute for it.
    // 仅移除真正输入观察，证明就绪及传输存在不能替代它。
    let actual_input = fixture
        .target
        .prepared
        .observation
        .runtime
        .inner
        .launch_inputs
        .write()
        .expect("actual target input map")
        .remove("target-mcp:actual")
        .expect("same actual native input lease");
    // The real durable completion entry must reject before recording Completed when original inputs are unavailable.
    // 原输入不可用时，真实持久完成入口必须在记录 Completed 前拒绝。
    let rejected = fixture
        .manager
        .complete_activation_target_handoff_with(
            &mut fixture.transaction,
            &final_catalog,
            &fixture.target,
        )
        .expect_err("actual target input absence must block completion");
    assert!(
        rejected
            .to_string()
            .contains("actual launch inputs are missing")
    );
    assert_eq!(
        fixture.transaction.record().phase(),
        VulcanPluginActivationTransactionPhase::Published
    );
    assert!(binding.invoke(args.clone()).await.is_err());
    // Restore the identical original lease, never synthesize input evidence or restart the process.
    // 恢复同一原租约，绝不合成输入证据或重启进程。
    assert!(
        fixture
            .target
            .prepared
            .observation
            .runtime
            .inner
            .launch_inputs
            .write()
            .expect("restore actual target input map")
            .insert(String::from("target-mcp:actual"), actual_input)
            .is_none()
    );
    fixture
        .target
        .verify_source("target-mcp:actual", &profile)
        .expect("same original source after observation restoration");
    fixture
        .manager
        .complete_activation_target_handoff_with(
            &mut fixture.transaction,
            &final_catalog,
            &fixture.target,
        )
        .expect("same actual combined Completed");
    assert_eq!(
        fixture.transaction.record().phase(),
        VulcanPluginActivationTransactionPhase::Completed
    );
    fixture
        .target
        .verify_source("target-mcp:actual", &profile)
        .expect("completed target keeps the same actually consumed source");
    // Transferred source ownership makes subsequent actual readiness observations read-only, never cancellation.
    // 已转交来源所有权使随后的实际就绪观测仅只读，绝不成为取消。
    fixture
        .target
        .wait_until_ready(OBSERVE)
        .await
        .expect("same published target repeated readiness");
    assert!(
        !published
            .available_tool_bindings_filtered(&McpRuntimeToolAllowlist::default())
            .is_empty()
    );
    // Business result must identify the already observed actual child, never another startup.
    // 业务结果必须标识已观察实际子进程，绝不是另一次启动。
    let result = binding
        .invoke(args)
        .await
        .expect("original binding after confirmation");
    assert!(
        result
            .to_string()
            .contains(starts.lines().last().expect("actual selected PID"))
    );
    assert_eq!(
        fs::read_to_string(&fixture.marker).expect("same startup count"),
        starts
    );
    // Foreground transaction release is independent from actual long-lived package/environment resource ownership.
    // 前台事务释放独立于实际长生命周期包／环境资源所有权。
    drop(fixture.transaction);
    request_lock
        .try_lock()
        .expect("Completed releases original request ownership");
    request_lock.unlock().expect("release request contender");
    assert!(
        matches!(
            environment_lock.try_lock(),
            Err(std::fs::TryLockError::WouldBlock)
        ),
        "same live transport still owns its exact environment"
    );
    fixture
        .access
        .begin_plugin_mcp_drain("target-mcp")
        .expect("drain published actual original");
    // Retain historical control through actual retirement; it must no longer own physical preparation.
    // 跨实际退役保留历史控制；它必须不再拥有物理准备。
    let observer = fixture.target.observer();
    fixture
        .access
        .wait_plugin_mcp_drain("target-mcp", OBSERVE)
        .await
        .expect("actual original protocol and physical resources retired");
    observer
        .wait_until_released(OBSERVE)
        .await
        .expect("same resource release receipt");
    environment_lock
        .try_lock()
        .expect("physical transport retirement releases environment ownership");
    environment_lock
        .unlock()
        .expect("release environment contender");
    assert!(binding.invoke(serde_json::json!({})).await.is_err());
    assert_eq!(
        fixture.target.actual_instance_ids(),
        receipt.instance_ids().unwrap()
    );
    drop(fixture.target);
}

/// The last actual published execution lease triggers real retirement while historical target control remains alive.
/// 最后一个实际已发布执行租约触发真实退役，同时历史目标控制继续存活。
/// No inputs or return value; native cleanup, actual OS lock acquisition and production repair prove resource separation.
/// 无输入及返回值；原生清理、实际操作系统锁获取及生产修复证明资源分离。
#[tokio::test]
async fn actual_target_retained_control_allows_last_execution_owner_retirement() {
    // The original producer has completed real candidate cleanup and one genuine target startup.
    // 原生产者已完成真实候选清理及单次真正目标启动。
    let mut fixture = fixture().await;
    // A separate actual bootstrap shares only admission here; its other original registries remain foreign.
    // 此处独立实际引导仅共享入场；其其他原注册表仍为外来来源。
    let mut foreign = bootstrap_mcp_runtime_access(Vec::new())
        .await
        .expect("independent actual empty access");
    foreign.blocked_owner_plugins = Arc::clone(&fixture.access.blocked_owner_plugins);
    assert!(fixture.target.publication_access(&foreign).is_err());
    drop(foreign);
    // Resolve the existing production environment before retirement; no alternate path is inferred.
    // 退役前解析既有生产环境；不推断替代路径。
    let environment = fixture
        .manager
        .prepare_plugin_environment("target-mcp")
        .expect("exact existing target environment");
    // One actual published access is the sole execution lease owner after confirmed completion.
    // 确认完成后，单个实际已发布访问为唯一执行租约所有者。
    let published = fixture
        .target
        .publication_access(&fixture.access)
        .expect("same actual target publication lease");
    // This bounded core/native test uses the real existing manager completion boundary.
    // 此有界 core／原生测试使用真实既有管理器完成边界。
    let final_catalog = fixture
        .manager
        .publish_activation_target_handoff(&fixture.transaction, &fixture.staged, |_| Ok(()))
        .expect("closed actual target publication");
    drop(fixture.staged);
    fixture
        .manager
        .complete_activation_target_handoff_with(
            &mut fixture.transaction,
            &final_catalog,
            &fixture.target,
        )
        .expect("original actual combined completion");
    assert!(fixture.target.publication_access(&published).is_err());
    // Keep original observation and diagnostic identity alive without holding a native execution lease.
    // 保留原观察及诊断身份，不持有原生执行租约。
    let observation = fixture.target.observer();
    // Startup records are written only by the actual native children.
    // 启动记录仅由实际原生子进程写入。
    let starts = fs::read_to_string(&fixture.marker).expect("actual original startup records");
    // Same allocated owner identity remains diagnostic after native ownership has retired.
    // 相同已分配所有者身份在原生所有权退役后仍用于诊断。
    let instance_ids = fixture.target.actual_instance_ids();
    drop(fixture.transaction);
    drop(fixture.access);
    drop(published);
    observation
        .wait_until_released(OBSERVE)
        .await
        .expect("last actual lease retires despite retained target control");
    assert_eq!(fixture.target.actual_instance_ids(), instance_ids);
    // Transferred control cannot create another preparing lease or reopen the retired original.
    // 已转交控制不能创建另一个准备租约或重新开放已退役原实例。
    assert!(
        fixture
            .target
            .prepared_tool_bindings_filtered(&McpRuntimeToolAllowlist::default())
            .is_err()
    );
    // The permanent production file is independently locked after physical retirement.
    // 物理退役后独立锁定永久生产文件。
    let environment_lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(
            fixture
                ._temp
                .path()
                .join("runtime/.environment-leases/target-mcp.lock"),
        )
        .expect("actual environment lock contender");
    environment_lock
        .try_lock()
        .expect("retained target control has no environment File");
    environment_lock.unlock().expect("release actual contender");
    fixture
        .manager
        .begin_draining("target-mcp")
        .expect("actual disable transition");
    fixture
        .manager
        .finish_lifecycle_transition(
            "target-mcp",
            &BTreeSet::from([String::from("target-mcp")]),
            &BTreeSet::new(),
            Vec::new(),
        )
        .expect("actual disabled target catalog");
    assert!(
        environment.starts_with(
            fixture
                ._temp
                .path()
                .canonicalize()
                .expect("literal isolated root")
        )
    );
    fs::remove_file(environment.join("bin/server.bin"))
        .expect("remove actual retired fixture binary");
    assert_eq!(
        fixture
            .manager
            .prepare_plugin_environment("target-mcp")
            .expect("real environment repair while historical control remains alive"),
        environment
    );
    assert_eq!(
        fs::read_to_string(&fixture.marker).expect("no target restart"),
        starts
    );
    drop(fixture.target);
}

/// Guard destruction with no external execution lease releases its business lock before last-lease shutdown.
/// 没有外部执行租约时，守卫析构在最后租约关闭之前释放业务锁。
/// No inputs or result; bounded native completion and actual release test destructor ordering, not a full service publication.
/// 无输入或结果；有界原生完成及实际释放测试析构顺序，不表示完整服务发布。
#[tokio::test]
async fn actual_target_handoff_last_guard_lease_does_not_deadlock_shutdown() {
    // The real producer supplies the original closed-ready target without constructing any external bindings.
    // 真实生产者提供原关闭就绪目标，不构造任何外部绑定。
    let Fixture {
        target,
        staged,
        manager,
        mut transaction,
        access,
        marker,
        _temp,
        ..
    } = fixture().await;
    // Keep the original non-owning observation while the native guard becomes the last actual lease owner.
    // 原生守卫成为最后实际租约所有者时，保留原非拥有观察。
    let observation = target.observer();
    // This bounded manager/native fixture intentionally has no service publisher or publication access.
    // 此有界管理器／原生夹具刻意没有服务发布者或发布访问。
    let final_catalog = manager
        .publish_activation_target_handoff(&transaction, &staged, |_| Ok(()))
        .expect("actual closed core/native target catalog");
    drop(staged);
    // Exact child records prove the completion and destructor paths never restart the original.
    // 精确子进程记录证明完成及析构路径绝不重启原实例。
    let starts = fs::read_to_string(&marker).expect("actual original target starts");
    // An independent native thread makes a self-deadlocked synchronous completion observable within the existing budget.
    // 独立原生线程使同步完成的自锁可在既有预算内被观察。
    let (completion_tx, completion_rx) = std::sync::mpsc::sync_channel(1);
    // The worker owns the same actual source and returns its historical control without invoking plugin business.
    // 工作线程拥有相同实际来源，返回其历史控制，不调用插件业务。
    let worker = std::thread::spawn(move || {
        // Only the real completion method drops its native guard and last strong lease.
        // 仅真实完成方法销毁其原生守卫及最后强租约。
        let result = manager.complete_activation_target_handoff_with(
            &mut transaction,
            &final_catalog,
            &target,
        );
        completion_tx
            .send((result, transaction, target))
            .expect("bounded original completion receiver");
    });
    // Receive cannot await a blocked Tokio worker and diagnoses the exact synchronous destructor boundary.
    // 接收不依赖阻塞的 Tokio 工作线程，诊断精确同步析构边界。
    let (result, transaction, target) = completion_rx
        .recv_timeout(OBSERVE)
        .expect("last native guard lease must release business before shutdown");
    worker.join().expect("actual original completion thread");
    result.expect("bounded real native completion returns");
    assert_eq!(
        transaction.record().phase(),
        VulcanPluginActivationTransactionPhase::Completed
    );
    observation
        .wait_until_released(OBSERVE)
        .await
        .expect("last native guard lease actually retires the original");
    assert_eq!(
        fs::read_to_string(&marker).expect("no destructor restart"),
        starts
    );
    assert!(
        target
            .prepared_tool_bindings_filtered(&McpRuntimeToolAllowlist::default())
            .is_err()
    );
    drop(target);
    drop(transaction);
    drop(access);
    drop(_temp);
}

/// Actual receipt readback failure after acquiring native guards closes the same lease and restores the exact closed Arc.
/// 取得原生守卫后的实际回执读回失败关闭相同租约，并恢复精确关闭 Arc。
/// This injects an unconfirmed namespace read failure, not a post-replacement atomic Completed writer failure.
/// 此处注入未确认命名空间读取失败，不是原子 Completed 写入替换之后的失败。
#[tokio::test]
async fn actual_target_unconfirmed_receipt_readback_keeps_business_closed() {
    // The isolated original already completed real candidate retirement and actual target readiness.
    // 隔离原请求已完成真实候选退役及实际目标就绪。
    let mut fixture = fixture().await;
    // Exact sidecar location is derived by transactions/target_mcp.rs and records.rs.
    // 精确附属回执位置由 transactions/target_mcp.rs 及 records.rs 推导。
    let receipt_path = fixture
        ._temp
        .path()
        .join("user/.vulcan-generations/transactions/target-mcp")
        .join(format!(
            "{}.target-mcp",
            fixture.transaction.record().request_id()
        ));
    // Preserve exact valid bytes so the deliberate read fault does not become a repair or replay grant.
    // 保留精确有效字节，使故意读取故障不成为修复或重放授权。
    let original_receipt = fs::read(&receipt_path).expect("original actual receipt bytes");
    // The startup marker later proves recovery never replaced this same real process.
    // 启动标记随后证明恢复绝不替换此相同真实进程。
    let starts = fs::read_to_string(&fixture.marker).expect("original actual starts");
    // Captured same-lease handlers exist before publication but remain uncallable.
    // 发布前已存在捕获的相同租约处理器，但仍不可调用。
    let bindings = fixture
        .target
        .prepared_tool_bindings_filtered(&McpRuntimeToolAllowlist::default())
        .expect("prepare actual closed handlers");
    // Publication access is captured from the original current partition lineage.
    // 从原当前分区谱系捕获发布访问。
    let published = fixture
        .target
        .publication_access(&fixture.access)
        .expect("same actual publication access");
    // The production synchronous publisher freezes the final original target catalog.
    // 生产同步发布器固定最终原目标目录。
    let final_catalog = fixture
        .manager
        .publish_activation_target_handoff(&fixture.transaction, &fixture.staged, |_| Ok(()))
        .expect("closed final publication");
    // The real manager sees the fault only after genuine backend/source guards have been acquired.
    // 真实管理器仅在取得真正后端／来源守卫之后观察故障。
    let completed = fixture.manager.complete_activation_target_handoff_with(
        &mut fixture.transaction,
        &final_catalog,
        &ReadbackFailureAdapter {
            target: &fixture.target,
            receipt_path: &receipt_path,
        },
    );
    assert!(completed.is_err());
    assert!(Arc::ptr_eq(&fixture.staged, &fixture.manager.snapshot()));
    assert_eq!(
        fixture.transaction.record().phase(),
        VulcanPluginActivationTransactionPhase::Published
    );
    assert!(
        published
            .available_tool_bindings_filtered(&McpRuntimeToolAllowlist::default())
            .is_empty()
    );
    assert!(
        bindings
            .first()
            .expect("actual declared handler")
            .invoke(serde_json::json!({}))
            .await
            .is_err()
    );
    // Restoring original bytes leaves business Preparing until exact same-owner confirmation succeeds.
    // 恢复原字节后，业务保持 Preparing，直至精确同所有者确认成功。
    fs::write(&receipt_path, original_receipt).expect("restore exact test fault bytes");
    assert!(
        fixture
            .store
            .resume_activation_transaction("target-mcp", fixture.transaction.record().request_id())
            .is_err(),
        "unconfirmed original source still holds the same OS request lock"
    );
    // Retry only final publication and completion on the same original current closed Arc, never startup.
    // 仅在相同原当前关闭 Arc 上重试最终发布及完成，绝不重试启动。
    let recovered_catalog = fixture
        .manager
        .publish_activation_target_handoff(&fixture.transaction, &fixture.staged, |_| Ok(()))
        .expect("same actual recovery publication");
    fixture
        .manager
        .complete_activation_target_handoff_with(
            &mut fixture.transaction,
            &recovered_catalog,
            &fixture.target,
        )
        .expect("same actual recovered Completed");
    // The captured original handler becomes callable only after recovered confirmation.
    // 捕获的原处理器仅在恢复确认后变为可调用。
    let result = bindings.iter().find(|binding| binding.origin_tool_name() == Some("read_page")).expect("read_page declaration may have changed").invoke(serde_json::json!({"url":"https://recovered-target.test","owner_session_id":"fixture-owner"})).await.expect("same original recovered binding");
    assert!(
        result
            .to_string()
            .contains(starts.lines().last().expect("same original actual PID"))
    );
    assert_eq!(
        fs::read_to_string(&fixture.marker).expect("same actual recovery startup count"),
        starts
    );
    // Explicit drain waits for this original process and resource owner rather than clearing observations.
    // 显式排空等待此原进程及资源所有者，而不是清空观察。
    let observer = fixture.target.observer();
    fixture
        .access
        .begin_plugin_mcp_drain("target-mcp")
        .expect("drain unconfirmed actual target");
    drop(fixture.target);
    drop(fixture.transaction);
    fixture
        .access
        .wait_plugin_mcp_drain("target-mcp", OBSERVE)
        .await
        .expect("unconfirmed original physically retired");
    observer
        .wait_until_released(OBSERVE)
        .await
        .expect("actual resource retirement receipt");
}

/// Failed publication and abandoned prevalidated completion keep original business closed and prohibit replay.
/// 发布失败及放弃预校验完成保持原业务关闭，并禁止重放。
#[tokio::test]
async fn actual_target_failed_or_abandoned_handoff_stays_closed_without_replay() {
    for abandon_completion in [false, true] {
        // Each scenario owns a separate original durable request and native resources.
        // 每个场景拥有单独原持久请求及原生资源。
        let mut fixture = fixture().await;
        // The same captured actual lease must reject every unconfirmed scenario.
        // 相同捕获实际租约必须拒绝每个未确认场景。
        let bindings = fixture
            .target
            .prepared_tool_bindings_filtered(&McpRuntimeToolAllowlist::default())
            .expect("real prepared handlers");
        if abandon_completion {
            // Explicitly abandon the actual prepared guard before durable completion.
            // 持久完成前，明确放弃实际准备守卫。
            let final_catalog = fixture
                .manager
                .publish_activation_target_handoff(
                    &fixture.transaction,
                    &fixture.staged,
                    |_| Ok(()),
                )
                .expect("provisional extension");
            assert!(
                fixture
                    .manager
                    .complete_activation_target_handoff_with(
                        &mut fixture.transaction,
                        &final_catalog,
                        &AbandonCompletionAdapter {
                            target: &fixture.target
                        }
                    )
                    .is_err()
            );
        } else {
            assert!(
                fixture
                    .manager
                    .publish_activation_target_handoff(
                        &fixture.transaction,
                        &fixture.staged,
                        |_| Err(String::from("injected target publisher failure"))
                    )
                    .is_err()
            );
            assert!(Arc::ptr_eq(&fixture.staged, &fixture.manager.snapshot()));
        }
        assert!(
            fixture
                .target
                .publication_access(&fixture.access)
                .expect("same observed access")
                .available_tool_bindings_filtered(&McpRuntimeToolAllowlist::default())
                .is_empty()
        );
        assert!(
            bindings
                .first()
                .expect("real discovered handler")
                .invoke(serde_json::json!({}))
                .await
                .is_err()
        );
        assert!(
            fixture
                .manager
                .prepare_mcp_activation_target(
                    &fixture.transaction,
                    &fixture.staged,
                    &fixture.native_host
                )
                .is_err(),
            "durable original attempt must prohibit recreation"
        );
        assert_eq!(
            fixture.transaction.record().phase(),
            VulcanPluginActivationTransactionPhase::Published
        );
        // Drain observes actual original resources even when extension publication was never confirmed.
        // 即使扩展发布从未确认，排空仍观察实际原资源。
        let observer = fixture.target.observer();
        // Keep the original identifier for exact recovery after actual owners retire.
        // 保留原标识，以便实际所有者退役后进行精确恢复。
        let request_id = fixture.transaction.record().request_id();
        fixture
            .access
            .begin_plugin_mcp_drain("target-mcp")
            .expect("original unpublished drain");
        drop(fixture.target);
        drop(fixture.transaction);
        fixture
            .access
            .wait_plugin_mcp_drain("target-mcp", OBSERVE)
            .await
            .expect("actual failed handoff retirement");
        observer
            .wait_until_released(OBSERVE)
            .await
            .expect("original resource receipt");
        // A fresh foreground owner and staged Arc still cannot replay the persisted actual attempt.
        // 新前台所有者及暂存 Arc 仍不能重放已持久化实际尝试。
        let resumed = fixture
            .store
            .resume_activation_transaction("target-mcp", request_id)
            .expect("resume exact failed original after retirement");
        // The existing manager assigns a fresh stage only; no transport startup is requested here.
        // 现有管理器仅分配新暂存；此处不请求 transport 启动。
        let restaged = fixture
            .manager
            .stage_activation_target_handoff(
                &resumed,
                &BTreeSet::new(),
                &BTreeSet::from([String::from("target-mcp")]),
            )
            .expect("same original recovery stage");
        assert!(
            fixture
                .manager
                .prepare_mcp_activation_target(&resumed, &restaged, &fixture.native_host)
                .is_err(),
            "persisted original startup receipt must reject process recreation"
        );
        assert_eq!(
            fs::read_to_string(&fixture.marker)
                .expect("no replay marker")
                .lines()
                .count(),
            2
        );
    }
}

/// One generic completion transfers the original real Hook VM and actual MCP lease together.
/// 单次泛型完成共同转交原真实 Hook VM 及实际 MCP 租约。
/// No inputs or return value; native events, strict identities, continuing state and OS locks prove independent ownership lifetimes.
/// 无输入及返回值；原生事件、严格身份、连续状态及操作系统锁证明独立所有权寿命。
#[tokio::test]
async fn actual_target_hook_and_mcp_complete_together_without_reinitialization() {
    // Both declarations were inspected and pinned before the original immutable package was prepared.
    // 两项声明均在原不可变插件包准备前接受检查及固定。
    let mut fixture = fixture_with_hooks(true).await;
    assert!(fixture.staged.enabled_hooks.is_empty());
    assert!(fixture.staged.enabled_mcp_servers.is_empty());
    assert!(
        fixture
            .manager
            .prewarm_activation_target_hook(&fixture.transaction, &fixture.staged)
            .await
            .expect("real original Hook private preparation")
    );
    assert_eq!(fixture.hook_events.count("composite-init"), 1);
    // Repeating observation retains the original private core session rather than loading another module.
    // 重复观测保留原私有核心会话，而非载入另一个模块。
    assert!(
        fixture
            .manager
            .prewarm_activation_target_hook(&fixture.transaction, &fixture.staged)
            .await
            .expect("same actual private Hook")
    );
    assert_eq!(fixture.hook_events.count("composite-init"), 1);
    assert!(
        !fixture
            .manager
            .prewarm_plugin_runtime("target-mcp")
            .await
            .expect("closed ordinary Hook preparation")
    );
    // The public strict receipt exposes only historical identities; real calls and live core usage below verify retained ownership.
    // 公开严格回执仅暴露历史身份；下文真实调用及活动核心用量验证保留所有权。
    let hook_receipt = fixture
        .store
        .target_hook_initialization_receipt("target-mcp", fixture.transaction.record().request_id())
        .expect("strict actual Hook receipt")
        .expect("original Hook attempt");
    assert_eq!(
        hook_receipt.phase(),
        vulcan_plugin_runtime::VulcanPluginTargetHookInitializationPhase::Succeeded
    );
    // Serialization follows the explicitly defined strict receipt fields, without private test-only core introspection.
    // 序列化遵循明确定义的严格回执字段，不使用核心私有测试观测。
    let hook_identity =
        serde_json::to_value(&hook_receipt).expect("actual Hook identity projection");
    for field in ["pool_id", "session_id", "operation_id"] {
        assert!(
            !hook_identity["core"][field]
                .as_str()
                .expect("strict actual core identifier")
                .is_empty()
        );
    }
    // Public plugin usage comes from the same actual LuaSkills core that owns the private fixed session and VM.
    // 公开插件用量来自拥有私有固定会话及 VM 的相同实际 LuaSkills 核心。
    let before_core = fixture
        .manager
        .formal_plugin_status("target-mcp")
        .expect("actual core observation")
        .expect("registered actual Hook plugin");
    assert_eq!(before_core.snapshot.resources.resident, 1);
    assert_eq!(before_core.snapshot.retained_pools, 1);
    assert_eq!(before_core.snapshot.retained_sessions, 1);
    // Freeze a real descriptor and construct its public immutable Hook invocation envelope.
    // 固定真实描述符并构造其公开不可变 Hook 调用信封。
    let hook = fixture.staged.plugins["target-mcp"]
        .hooks
        .iter()
        .find(|hook| hook.manifest.id == "precheck")
        .expect("precheck declaration may have changed")
        .clone();
    // Reused host invocation contains no fabricated core session identity.
    // 复用宿主调用不包含伪造核心会话身份。
    let invocation = vulcan_hooks::HookInvocation {
        registration: vulcan_hooks::HookRegistration::new(
            hook.registration_id.clone(),
            hook.manifest.point.clone(),
            hook.manifest.mode,
        ),
        context: vulcan_hooks::HookContext::new("composite-original", hook.manifest.point.clone()),
        payload: vulcan_hooks::HookPayload::empty_object(),
    };
    assert!(
        fixture
            .manager
            .invoke_hook(&hook, invocation.clone())
            .await
            .is_err()
    );
    // Original request exclusion is shared by foreground, real Hook and actual MCP source ownership.
    // 原请求互斥由前台、真实 Hook 及实际 MCP 来源所有权共享。
    let request_lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(
            fixture
                ._temp
                .path()
                .join("user/.vulcan-generations/locks")
                .join(format!(
                    "target-mcp.transaction.{}.lock",
                    fixture.transaction.record().request_id()
                )),
        )
        .expect("actual composite request lock");
    assert!(matches!(
        request_lock.try_lock(),
        Err(std::fs::TryLockError::WouldBlock)
    ));
    // Both real backends share the exact physical plugin environment until their resources retire.
    // 两个真实后端共享精确物理插件环境，直至其资源退役。
    let environment_lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(
            fixture
                ._temp
                .path()
                .join("runtime/.environment-leases/target-mcp.lock"),
        )
        .expect("actual composite environment lock");
    assert!(matches!(
        environment_lock.try_lock(),
        Err(std::fs::TryLockError::WouldBlock)
    ));
    // The real process marker distinguishes the retired candidate from the sole actual selected process.
    // 真实进程标记区分已退役候选及唯一实际已选进程。
    let starts = fs::read_to_string(&fixture.marker).expect("actual composite process marker");
    assert_eq!(starts.lines().count(), 2);
    // Capture actual opaque runtime owners before either backend crosses the original completion boundary.
    // 任一后端跨越原完成边界之前，捕获实际不透明运行时所有者。
    let actual_mcp_ids = fixture.target.actual_instance_ids();
    // The sole publisher freezes these same actual closed bindings; runtime invocation remains fenced.
    // 唯一发布器固定这些相同实际关闭绑定；运行时调用仍受屏障约束。
    let bindings = fixture
        .target
        .prepared_tool_bindings_filtered(&McpRuntimeToolAllowlist::default())
        .expect("real dual backend prepared handlers");
    // Stable semantic lookup avoids binding-order assumptions.
    // 稳定语义查找避免绑定顺序假设。
    let binding = bindings
        .iter()
        .find(|binding| binding.origin_tool_name() == Some("read_page"))
        .expect("read_page declaration may have changed");
    // Exact fixture arguments expose the same real process identity after completion.
    // 精确夹具参数在完成后暴露相同真实进程身份。
    let args = serde_json::json!({"url":"https://composite-target.test","owner_session_id":"fixture-owner"});
    assert!(binding.invoke(args.clone()).await.is_err());
    // Current publication access replaces only this target with its existing actual lease.
    // 当前发布访问仅以既有实际租约替换此目标。
    let published = fixture
        .target
        .publication_access(&fixture.access)
        .expect("same composite publication lineage");
    // One synchronous full catalog publication precedes the single combined durable completion.
    // 单次同步完整目录发布先于单次组合持久完成。
    let final_catalog = fixture
        .manager
        .publish_activation_target_handoff(&fixture.transaction, &fixture.staged, |catalog| {
            assert!(
                catalog
                    .enabled_hooks
                    .values()
                    .any(|hook| hook.plugin_id == "target-mcp")
            );
            assert!(
                catalog
                    .enabled_mcp_servers
                    .values()
                    .any(|server| server.plugin_id == "target-mcp")
            );
            assert!(
                published
                    .available_tool_bindings_filtered(&McpRuntimeToolAllowlist::default())
                    .is_empty()
            );
            Ok(())
        })
        .expect("actual dual backend full publication");
    assert!(
        fixture
            .manager
            .invoke_hook(&hook, invocation.clone())
            .await
            .is_err()
    );
    assert!(binding.invoke(args.clone()).await.is_err());
    fixture
        .manager
        .complete_activation_target_handoff_with(
            &mut fixture.transaction,
            &final_catalog,
            &fixture.target,
        )
        .expect("one combined actual Hook and MCP Completed");
    assert_eq!(
        fixture.transaction.record().phase(),
        VulcanPluginActivationTransactionPhase::Completed
    );
    assert_eq!(fixture.target.actual_instance_ids(), actual_mcp_ids);
    // Two ordinary calls continue the initializer's original in-memory counter without another private initialization.
    // 两次普通调用继续初始化器的原内存计数，不发生另一私有初始化。
    for expected in [2, 3] {
        let result = fixture
            .manager
            .invoke_hook(&hook, invocation.clone())
            .await
            .expect("original transferred Hook VM invocation");
        assert_eq!(
            result
                .payload
                .expect("declared Hook transform payload")
                .value["count"],
            expected
        );
    }
    assert_eq!(fixture.hook_events.count("composite-init"), 1);
    assert_eq!(
        serde_json::to_value(
            fixture
                .store
                .target_hook_initialization_receipt(
                    "target-mcp",
                    fixture.transaction.record().request_id()
                )
                .expect("same original Hook receipt")
                .expect("same actual Hook attempt")
        )
        .expect("same strict core projection"),
        hook_identity
    );
    // Actual core usage still contains one pool, one fixed session and one resident Hook VM under the same parent.
    // 实际核心用量仍在相同父级下包含单个池、单个固定会话及单个驻留 Hook VM。
    let after_core = fixture
        .manager
        .formal_plugin_status("target-mcp")
        .expect("transferred real core observation")
        .expect("same core plugin");
    assert_eq!(after_core.runtime_id, before_core.runtime_id);
    assert_eq!(after_core.snapshot.resources.resident, 1);
    assert_eq!(
        after_core.snapshot.retained_pools,
        before_core.snapshot.retained_pools
    );
    assert_eq!(
        after_core.snapshot.retained_sessions,
        before_core.snapshot.retained_sessions
    );
    assert!(
        !published
            .available_tool_bindings_filtered(&McpRuntimeToolAllowlist::default())
            .is_empty()
    );
    // The already captured binding invokes the already observed actual process immediately after confirmation.
    // 确认后，已捕获绑定立即调用已观察实际进程。
    let mcp_result = binding
        .invoke(args)
        .await
        .expect("same dual backend MCP business");
    assert!(
        mcp_result
            .to_string()
            .contains(starts.lines().last().expect("original actual selected PID"))
    );
    assert_eq!(
        fs::read_to_string(&fixture.marker).expect("no composite restart"),
        starts
    );
    drop(fixture.transaction);
    request_lock
        .try_lock()
        .expect("combined Completed releases original request exclusion");
    request_lock
        .unlock()
        .expect("release composite request contender");
    assert!(matches!(
        environment_lock.try_lock(),
        Err(std::fs::TryLockError::WouldBlock)
    ));
    // Real Hook draining repeatedly observes the same retained owner until its actual cleanup finishes.
    // 真实 Hook 排空重复观察相同保留所有者，直至其实际清理完成。
    tokio::time::timeout(OBSERVE, async {
        loop {
            // The production report is actual cleanup evidence, never a snapshot-derived retirement claim.
            // 生产报告为实际清理凭据，绝不是由快照推导的退役声明。
            let report = fixture
                .manager
                .drain_plugin_sessions("target-mcp")
                .await
                .expect("actual original Hook drain");
            if report.failures.is_empty() {
                assert_eq!(report.closed_hook_runtimes, 1);
                break;
            }
            assert!(
                report.failures.iter().all(|failure| failure
                    == "persistent cleanup timed out; original ownership remains retained."),
                "{:?}",
                report.failures
            );
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("actual Hook cleanup observation budget");
    assert_eq!(fixture.hook_events.count("composite-retired"), 1);
    assert_eq!(
        fixture
            .manager
            .formal_plugin_status("target-mcp")
            .expect("actual retired Hook core usage")
            .expect("retained core plugin metadata")
            .snapshot
            .resources
            .resident,
        0
    );
    assert!(
        matches!(
            environment_lock.try_lock(),
            Err(std::fs::TryLockError::WouldBlock)
        ),
        "live MCP transport retains environment after actual Hook retirement"
    );
    fixture
        .manager
        .request_formal_runtime_close()
        .expect("close original actual formal parent");
    tokio::time::timeout(OBSERVE, async {
        while !fixture
            .manager
            .poll_formal_runtime_closed()
            .expect("actual core VM and worker retirement")
        {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("actual core parent termination");
    // The original observation tables drain this same MCP process independently from the retired Lua parent.
    // 原观察表独立于已退役 Lua 父级，排空此相同 MCP 进程。
    let observer = fixture.target.observer();
    fixture
        .access
        .begin_plugin_mcp_drain("target-mcp")
        .expect("actual composite MCP drain");
    drop(fixture.target);
    fixture
        .access
        .wait_plugin_mcp_drain("target-mcp", OBSERVE)
        .await
        .expect("same original MCP physical retirement");
    observer
        .wait_until_released(OBSERVE)
        .await
        .expect("same original resource release");
    environment_lock
        .try_lock()
        .expect("both actual backends retired before environment release");
    environment_lock
        .unlock()
        .expect("release composite environment contender");
    assert!(binding.invoke(serde_json::json!({})).await.is_err());
}
