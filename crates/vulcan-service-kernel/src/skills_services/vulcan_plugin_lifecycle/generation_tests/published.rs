//! Actual service slot publication transfers the first issuer's original native child without replay.
//! 实际服务槽发布转交首次签发者的原原生子进程，不重播。

use super::*;
use vulcan_agent_runtime::mcp::{
    PluginMcpSecretStore, PreparedPublishedPluginMcpConfigs, PublishedPluginMcpRuntime,
    published_global_mcp_server_configs,
};
use vulcan_plugin_runtime::{
    VulcanPluginConfigurationSelection, VulcanPluginCredentialBackend,
    VulcanPluginCredentialBinding, VulcanPluginCredentialKey, VulcanPluginCredentialRevision,
    VulcanPluginCredentialValues, VulcanPluginGenerationStore, VulcanPluginRuntimeManager,
};

mod bootstrap;
pub(in crate::skills_services) mod predecessor;
mod target;
mod uninstall;

/// One deadline bounds observation of this fixture's actual original native work.
/// 单个截止时间限制此夹具实际原原生工作的观察。
const OBSERVE: std::time::Duration = std::time::Duration::from_secs(10);

/// This actual completed empty credential revision must never access an external secret account.
/// 此实际完成的空凭据修订绝不得访问外部秘密账户。
struct EmptyCredentials;

impl VulcanPluginCredentialBackend for EmptyCredentials {
    /// Reject plugin/revision/key reads; returns an explicit error because no secret was declared.
    /// 拒绝插件／修订／键读取；未声明秘密，故返回明确错误。
    fn read(
        &self,
        _: &str,
        _: &VulcanPluginCredentialRevision,
        _: &VulcanPluginCredentialKey,
    ) -> Result<Option<String>, String> {
        Err(String::from(
            "Unexpected empty service-fixture credential read.",
        ))
    }
    /// Reject plugin/revision/key/value writes; returns an explicit error without changing an account.
    /// 拒绝插件／修订／键／值写入；不改变账户并返回明确错误。
    fn write(
        &self,
        _: &str,
        _: &VulcanPluginCredentialRevision,
        _: &VulcanPluginCredentialKey,
        _: &str,
    ) -> Result<(), String> {
        Err(String::from(
            "Unexpected empty service-fixture credential write.",
        ))
    }
    /// Reject plugin/revision/key deletions; returns an explicit error without touching external storage.
    /// 拒绝插件／修订／键删除；不接触外部存储并返回明确错误。
    fn delete(
        &self,
        _: &str,
        _: &VulcanPluginCredentialRevision,
        _: &VulcanPluginCredentialKey,
    ) -> Result<(), String> {
        Err(String::from(
            "Unexpected empty service-fixture credential deletion.",
        ))
    }
}

impl PluginMcpSecretStore for EmptyCredentials {
    /// Reject plugin/server/key/binding materialization reads and return a value-free error.
    /// 拒绝插件／服务器／键／绑定物化读取，并返回不含值错误。
    fn read_secret(
        &self,
        _: &str,
        _: &str,
        _: &str,
        _: &VulcanPluginCredentialBinding,
    ) -> Result<String, String> {
        Err(String::from(
            "Unexpected empty service-fixture MCP secret read.",
        ))
    }
}

/// Own one actual service bridge, full configured source and sealed native inputs under isolated files.
/// 在隔离文件下拥有单个实际服务桥、完整已配置来源及密封原生输入。
pub(in crate::skills_services) struct Fixture {
    /// The actual production state mutex owns the sole full extension pointer.
    /// 实际生产状态互斥锁拥有唯一完整扩展指针。
    pub(in crate::skills_services) state: SharedServiceBridgeState,
    /// Sealed definitions carry the first bridge issuer's real original package and environment owner.
    /// 密封定义携带首次桥签发者的真实原插件包及环境所有者。
    configs: PreparedPublishedPluginMcpConfigs,
    /// Only the actual child writes its complete startup record here.
    /// 仅实际子进程在此写入完整启动记录。
    marker: std::path::PathBuf,
    /// Exact production environment permits independent cleanup verification after original native retirement.
    /// 精确生产环境允许原原生退役后的独立清理验证。
    environment: std::path::PathBuf,
    /// Last ownership field keeps files valid until every preceding actual owner is disposed.
    /// 最后所有权字段使文件保持有效，直至每个前置实际所有者销毁。
    _temp: tempfile::TempDir,
}

/// Build one genuine configured managed native source and bind its preparation to the first actual service issuer.
/// 构建单个真实已配置受管原生来源，并将准备绑定至首次实际服务签发者。
/// No inputs; returns real immutable package, empty credential revision and service slot ownership without launching work.
/// 无输入；返回真实不可变插件包、空凭据修订及服务槽所有权，不启动工作。
fn fixture() -> Fixture {
    configured_fixture(None, None)
}

/// Build the same genuine old fixture with one staged secret; backend writes it and secrets materializes its bound revision.
/// 用单个暂存秘密构建同一真实旧夹具；backend 写入它，secrets 物化其绑定修订。
/// value is an isolated test value; return the actual managed package and first-issuer sealed inputs without launching work.
/// value 为隔离测试值；返回实际受管包及首次签发者密封输入，不启动工作。
pub(in crate::skills_services) fn fixture_with_old_secret(
    backend: &dyn VulcanPluginCredentialBackend,
    secrets: &dyn PluginMcpSecretStore,
    value: &str,
) -> Fixture {
    configured_fixture(Some((backend, secrets, value)), None)
}

/// Build the genuine configured source with a real persistent Hook before immutable package inspection.
/// 在不可变插件包检查之前，使用真实持久 Hook 构建真实已配置来源。
/// events records actual Lua effects without blocking; return the same service fixture with its formal parent initialized.
/// events 无阻塞地记录实际 Lua 副作用；返回已初始化正式父级的相同服务夹具。
pub(in crate::skills_services) fn fixture_with_hook(
    events: Arc<dyn vulcan_plugin_runtime::VulcanPluginUiEventPublisher>,
) -> Fixture {
    configured_fixture(None, Some(events))
}

/// Build real immutable files and credential staging; old_secret explicitly selects the declared secret fixture variant.
/// 构建真实不可变文件及凭据暂存；old_secret 明确选择声明秘密的夹具变体。
/// Return the same actual service resources; absent old_secret keeps the existing empty revision contract.
/// 返回相同实际服务资源；缺少 old_secret 时保持既有空修订契约。
fn configured_fixture(
    old_secret: Option<(
        &dyn VulcanPluginCredentialBackend,
        &dyn PluginMcpSecretStore,
        &str,
    )>,
    // An explicit real Hook variant installs its host service before any Lua module executes.
    // 明确的真实 Hook 变体在任何 Lua 模块执行之前安装宿主服务。
    hook_events: Option<Arc<dyn vulcan_plugin_runtime::VulcanPluginUiEventPublisher>>,
) -> Fixture {
    // Every path and durable namespace is owned solely by this test.
    // 每个路径及持久命名空间均仅由本测试拥有。
    let temp = tempfile::tempdir().expect("published service fixture");
    // The engine-free actual manager resolves the bundled native dependency through production.
    // 无引擎实际管理器通过生产解析内置原生依赖。
    let manager = VulcanPluginRuntimeManager::without_runtime(
        temp.path().join("bundled"),
        temp.path().join("user"),
        temp.path().join("bridge"),
        temp.path().join("distributions"),
        temp.path().join("state"),
        temp.path().join("environments"),
    );
    std::fs::create_dir_all(temp.path().join("bundled")).expect("empty bundled root");
    std::fs::create_dir_all(manager.user_plugin_root()).expect("actual user root");
    // The immutable package contains the one locally resolved Node executable and protocol fixture.
    // 不可变插件包包含单个本地解析 Node 可执行文件及协议夹具。
    let source = temp.path().join("source/published-service");
    std::fs::create_dir_all(source.join("bin")).expect("source binary directory");
    // Reuse the existing sole Node dependency discovery contract, without probing alternative paths.
    // 复用既有唯一 Node 依赖发现契约，不探测替代路径。
    let node = std::process::Command::new("node")
        .args(["-p", "process.execPath"])
        .output()
        .expect("actual Node dependency");
    assert!(node.status.success(), "actual Node discovery failed");
    // The exact discovered executable is used only for this current-platform native acceptance.
    // 精确发现的可执行文件仅用于此当前平台原生验收。
    let executable = String::from_utf8(node.stdout).expect("UTF-8 actual Node path");
    std::fs::copy(executable.trim(), source.join("bin/server.bin"))
        .expect("bundle actual current-platform Node");
    std::fs::copy(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../vulcan-agent-runtime/tests/fixtures/candidate-mcp.mjs"),
        source.join("server.mjs"),
    )
    .expect("copy actual protocol fixture");
    std::fs::write(source.join("dependencies.yaml"), "{}\n")
        .expect("bundled dependency declaration");
    std::fs::write(source.join("schema.json"), r#"{"type":"object","properties":{"marker":{"type":"string"},"mode":{"type":"string"}},"required":["marker","mode"],"additionalProperties":false}"#).expect("actual configuration schema");
    // The child marker is outside immutable bytes and is written before real initialize completion.
    // 子进程标记位于不可变字节之外，在真实初始化完成之前写入。
    let marker = temp.path().join("actual-starts.jsonl");
    // This exact configuration is selected by the real activation writer.
    // 此精确配置由真实激活写入者选定。
    let configuration = serde_json::json!({"marker":marker,"mode":"ready_with_tool"});
    std::fs::write(
        source.join("defaults.json"),
        serde_json::to_vec(&configuration).expect("fixture defaults"),
    )
    .expect("write defaults");
    // Both explicit fixture variants share one declaration, with only the optional old secret permission and input added.
    // 两个明确夹具变体共享单个声明，仅增加可选旧秘密权限及输入。
    let mut manifest = String::from(
        r#"schema_version: 1
id: published-service
version: 1.0.0
default_enabled: true
display:
  name: Actual Published Service
  capability: Same original full publication
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
      name: Actual original service process
      transport:
        type: stdio
        launcher: {type: native_binary, binary_id: server}
        args: [{source: literal, value: server.mjs}]
        env:
          CANDIDATE_LABEL: {source: literal, value: published-service}
          CANDIDATE_MARKER: {source: config, key: marker}
          CANDIDATE_MODE: {source: config, key: mode}
"#,
    );
    if old_secret.is_some() {
        manifest = manifest.replacen(
            "permissions: [config, process_binary, process_session]",
            "permissions: [config, process_binary, process_session, secrets_read]",
            1,
        );
        manifest.push_str("          CANDIDATE_TOKEN: {source: secret, key: old_token}\n");
    }
    if let Some(events) = hook_events {
        install_fixture_hook(&manager, temp.path(), &source, &mut manifest, events);
    }
    std::fs::write(source.join("plugin.yaml"), manifest).expect("actual declared package");
    // Production inspection supplies complete approval and immutable content identity.
    // 生产检查提供完整批准及不可变内容身份。
    let preview = manager
        .inspect_local_user_plugin(&source)
        .expect("actual source inspection");
    // The actual canonical generation store owns all package and credential files.
    // 实际规范代次存储拥有全部插件包及凭据文件。
    let store =
        VulcanPluginGenerationStore::open(manager.user_plugin_root()).expect("actual store");
    // Actual immutable copying validates bytes before a package pin is returned.
    // 实际不可变复制在返回插件包固定之前校验字节。
    let package = store
        .prepare(&source, &preview.content_fingerprint)
        .expect("actual immutable package");
    // Both variants finish the real writer's complete revision; only the explicit secret variant writes an isolated value.
    // 两个变体都完成真实写入器的完整修订；仅明确秘密变体写入隔离值。
    let credentials = VulcanPluginCredentialRevision::allocate();
    // The declared old slot supplies the sole complete value inventory, without altering any external account.
    // 声明旧槽提供唯一完整值清单，不改变任何外部账户。
    let mut values = VulcanPluginCredentialValues::new();
    // This backend and reader pair is chosen solely by the explicit fixture variant above.
    // 此后端及读取器组合仅由上述明确夹具变体选择。
    let (backend, secrets): (
        &dyn VulcanPluginCredentialBackend,
        &dyn PluginMcpSecretStore,
    ) = match old_secret {
        Some((backend, secrets, value)) => {
            values.insert(
                VulcanPluginCredentialKey {
                    server_id: String::from("actual"),
                    secret_key: String::from("old_token"),
                },
                Some(String::from(value)),
            );
            (backend, secrets)
        }
        None => (&EmptyCredentials, &EmptyCredentials),
    };
    store
        .begin_credential_staging(&package, credentials.clone(), &values)
        .expect("actual complete credential staging")
        .write_and_finish(backend, &values)
        .expect("actual credential revision completion");
    store
        .activate_configured(
            &package,
            0,
            preview.permissions,
            credentials,
            VulcanPluginConfigurationSelection::Document {
                value: configuration,
            },
        )
        .expect("actual full selected source");
    manager
        .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
        .expect("actual selected catalog");
    // Production returns the exact environment root used by real native ownership.
    // 生产返回真实原生所有权使用的精确环境根。
    let environment = manager
        .prepare_plugin_environment("published-service")
        .expect("actual environment");
    // Existing actual service dependencies supply the real publication and lifecycle state.
    // 既有实际服务依赖提供真实发布及生命周期状态。
    let mut bridge = crate::tests::test_support::test_agent_runtime_bridge();
    bridge.vulcan_plugin_runtime = manager.clone();
    bridge.vulcan_plugin_native_mcp_host = manager.register_native_mcp_host_adapter();
    bridge.extension_generation = bridge
        .prepare_plugin_extension_generation(
            manager.snapshot(),
            bridge.extension_generation.mcp_runtime(),
        )
        .expect("initial full actual slot");
    // The real first bridge issuer seals materialization before that bridge is exposed to service readers.
    // 真实首次桥签发者在桥向服务读取者暴露之前密封物化。
    let prepared = manager
        .prepare_published_mcp_resources(
            &manager.snapshot(),
            "published-service",
            &bridge.vulcan_plugin_native_mcp_host,
        )
        .expect("actual original resources");
    // The original sealed inputs use exactly the staged revision through the selected fixture reader.
    // 原密封输入通过所选夹具读取器精确使用暂存修订。
    let configs = published_global_mcp_server_configs(
        prepared,
        &bridge.vulcan_plugin_native_mcp_host,
        secrets,
    )
    .expect("sealed actual definitions");
    // The same production mutex owns the full slot used by the service adapter under test.
    // 受测服务适配器使用的完整槽由同一生产互斥锁拥有。
    let mut service =
        crate::tests::test_support::test_service_state_with_parent_session(SessionId::new_v4());
    service.agent_runtime = Some(bridge);
    Fixture {
        state: Arc::new(Mutex::new(service)),
        configs,
        marker,
        environment,
        _temp: temp,
    }
}

/// Install real Lua and exact Hook declarations before source inspection, leaving configured activation unchanged.
/// 在来源检查之前安装真实 Lua 及精确 Hook 声明，保持已配置激活不变。
/// manager/root/source own this fixture; manifest receives declarations and events receives actual module effects.
/// manager／root／source 拥有此夹具；manifest 接收声明，events 接收实际模块副作用。
/// Returns no value; every preparation failure is an explicit test failure before immutable materialization.
/// 不返回值；每个准备失败都在不可变物化之前明确导致测试失败。
fn install_fixture_hook(
    manager: &VulcanPluginRuntimeManager,
    root: &std::path::Path,
    source: &std::path::Path,
    manifest: &mut String,
    events: Arc<dyn vulcan_plugin_runtime::VulcanPluginUiEventPublisher>,
) {
    use luaskills::{LuaEngine, LuaEngineOptions, LuaRuntimeHostOptions, LuaVmPoolConfig};
    // Canonical native paths match the existing real service Lua fixture's host-root contract.
    // 规范原生路径匹配既有真实服务 Lua 夹具的宿主根契约。
    let native_root = vulcan_runtime_assets::native_host_path(
        &root.canonicalize().expect("canonical mixed fixture root"),
    );
    // One real VM isolates module persistence from unrelated pool scheduling.
    // 单个真实 VM 将模块持久性与无关池调度隔离。
    let engine = LuaEngine::new(LuaEngineOptions::new(
        LuaVmPoolConfig {
            min_size: 1,
            max_size: 1,
            idle_ttl_secs: 60,
        },
        LuaRuntimeHostOptions::with_runtime_root(native_root),
    ))
    .expect("real mixed fixture engine");
    manager
        .attach_runtime_engine(Arc::new(engine))
        .expect("attach real mixed engine");
    std::fs::create_dir_all(root.join("bridge/runtime")).expect("real bridge directory");
    std::fs::write(root.join("bridge/dependencies.yaml"), "{}\n")
        .expect("empty real bridge dependencies");
    std::fs::write(
        root.join("bridge/runtime/bridge.lua"),
        include_str!("../../../../../../plugin-bridge/vulcan-plugin-bridge/runtime/bridge.lua"),
    )
    .expect("authoritative real bridge");
    std::fs::create_dir_all(root.join("distributions")).expect("real distribution root");
    std::fs::create_dir_all(source.join("runtime")).expect("real Hook source directory");
    *manifest = manifest.replacen(
        "permissions: [config, process_binary, process_session]",
        "permissions: [config, process_binary, process_session, ui_host_context]",
        1,
    );
    manifest.push_str("lua_runtime:\n  bundle: runtime/plugin.bundle.lua\n  invoke_function: dispatch\n  shutdown_function: shutdown\n  hook_shutdown_function: shutdown_hooks\n  max_active_sessions: 1\n  lifecycle:\n    enable: on_lifecycle\n    disable: on_lifecycle\n  hooks:\n    - id: precheck\n      point: conversation.turn.precheck\n      mode: transform\n      function: on_precheck\n      timeout_ms: 1000\n      failure_policy: stop\n");
    std::fs::write(
        source.join("runtime/plugin.bundle.lua"),
        r#"
-- Every actual module initialization emits one real native event before exporting functions.
-- 每次实际模块初始化在导出函数之前发出单个真实原生事件。
vulcan.host.call('vulcan.plugin.ui.invalidate', {topic='module-initialize', revision=0})
-- This module's private counter proves repeated Hook calls use its persistent original allocation.
-- 此模块私有计数器证明重复 Hook 调用使用其持久原分配。
local M, count = {}, 0
-- No business arguments are consumed; return the declared success frame.
-- 不消费业务参数；返回声明成功帧。
function M.dispatch() return '{"ok":true,"result":{}}' end
-- No shutdown arguments are consumed; return successful ordinary cleanup.
-- 不消费关闭参数；返回成功普通清理。
function M.shutdown() return '{"ok":true,"result":{}}' end
-- args carries the actual lifecycle event; record its execution and return success.
-- args 携带实际生命周期事件；记录其执行并返回成功。
function M.on_lifecycle(args)
    vulcan.host.call('vulcan.plugin.ui.invalidate', {topic=args.event, revision=0})
    return '{"ok":true,"result":{}}'
end
-- The invocation envelope is unused; return the increasing same-module payload.
-- 不使用调用信封；返回递增的同模块载荷。
function M.on_precheck(_args)
    count = count + 1
    return '{"ok":true,"result":{"payload":{"value":{"count":' .. tostring(count) .. '}}}}'
end
-- The original Hook finalizer emits its actual retirement once and returns successful cleanup.
-- 原 Hook 关闭器仅发出一次实际退役事件，并返回成功清理。
function M.shutdown_hooks(_args)
    vulcan.host.call('vulcan.plugin.ui.invalidate', {topic='hook-shutdown', revision=count})
    return '{"ok":true,"result":{}}'
end
return M
"#,
    )
    .expect("real persistent mixed Hook bundle");
    manager
        .install_host_services(vulcan_plugin_runtime::VulcanPluginHostServices {
            storage: None,
            vldb: None,
            ui_events: Some(events),
        })
        .expect("real mixed Hook native publisher");
    manager
        .initialize_formal_runtime(vulcan_plugin_runtime::VulcanPluginRuntimeSettings::default())
        .expect("real mixed formal parent");
}

/// The real service rejects a different issuer before its sole pointer changes, then transfers the original process once.
/// 真实服务在唯一指针变化前拒绝不同签发者，随后仅转交一次原进程。
/// No inputs or return value; actual slot identity, first issuer, PID calls and production environment repair are evidence.
/// 无输入及返回值；实际槽身份、首次签发者、PID 调用及生产环境修复构成证据。
#[tokio::test]
async fn service_published_native_full_pointer_keeps_first_issuer_and_same_original_process() {
    // The fixture has not launched any child before this exact service-owned source pair is read.
    // 此精确服务拥有来源配对读取之前，夹具没有启动任何子进程。
    let Fixture {
        state,
        configs,
        marker,
        environment,
        _temp,
    } = fixture();
    // The original manager and first issuer come from the actual single-lock service getter.
    // 原管理器及首次签发者来自实际单锁服务读取入口。
    let (manager, authority) =
        crate::service_state_access::service_vulcan_plugin_native_mcp_host(&state)
            .expect("actual first service pair");
    // The actual original full slot is captured under its publication mutex.
    // 在发布互斥锁内捕获实际原完整槽。
    let expected = Arc::clone(
        &state
            .lock()
            .expect("actual service lock")
            .agent_runtime
            .as_ref()
            .expect("actual bridge")
            .extension_generation,
    );
    // This exact current access shares the original registry lineage, without a replacement runtime allocation.
    // 此精确当前访问共享原注册表谱系，不分配替代运行时。
    let access = expected.mcp_runtime();
    // One actual producer consumes the first issuer's sealed configuration.
    // 单个实际生产者消费首次签发者的密封配置。
    let native = PublishedPluginMcpRuntime::start(&access, &manager, configs, &authority)
        .expect("original native preparation");
    // Keep independent real observations through later foreground control loss.
    // 跨后续前台控制丢失保留独立真实观察。
    let observation = native.observer();
    observation
        .wait_until_ready(OBSERVE)
        .await
        .expect("actual child ready");
    // Source is still the original manager snapshot; no catalog replacement or retrospective attachment occurs.
    // 来源仍为原管理器快照；没有目录替换或事后附加。
    let catalog = manager.snapshot();
    assert!(
        native
            .publication_access(&access)
            .expect("actual closed original access")
            .available_tool_bindings_filtered(
                &vulcan_agent_runtime::mcp::McpRuntimeToolAllowlist::default()
            )
            .is_empty()
    );
    // Replace only the fixture's service issuer with a genuinely distinct registration on the same manager.
    // 仅将夹具服务签发者替换为同一管理器上的真实不同注册。
    let foreign = manager.register_native_mcp_host_adapter();
    state
        .lock()
        .expect("service issuer mutation")
        .agent_runtime
        .as_mut()
        .expect("actual bridge")
        .vulcan_plugin_native_mcp_host = foreign;
    assert!(
        publish_native_published_plugin_extension_generation(&state, &native, Arc::clone(&catalog))
            .is_err()
    );
    assert!(Arc::ptr_eq(
        &expected,
        &state
            .lock()
            .expect("unchanged real pointer")
            .agent_runtime
            .as_ref()
            .expect("actual bridge")
            .extension_generation
    ));
    assert!(!native.matches_original_publication(&expected));
    // Restoring the exact original private issuer does not recreate a source, process or sealed configuration.
    // 恢复精确原私有签发者不重建来源、进程或密封配置。
    state
        .lock()
        .expect("restore actual original issuer")
        .agent_runtime
        .as_mut()
        .expect("actual bridge")
        .vulcan_plugin_native_mcp_host = authority.clone();
    // The production service adapter writes its actual full slot and confirms the same lease in the adjacent tail.
    // 生产服务适配器写入实际完整槽，并在相邻尾段确认相同租约。
    let committed = publish_native_published_plugin_extension_generation(&state, &native, catalog)
        .expect("actual service full commit");
    assert!(Arc::ptr_eq(
        &committed,
        &state
            .lock()
            .expect("actual committed slot")
            .agent_runtime
            .as_ref()
            .expect("actual bridge")
            .extension_generation
    ));
    assert!(native.matches_original_publication(&committed));
    assert!(
        crate::service_state_access::service_vulcan_plugin_native_mcp_host(&state)
            .expect("same first service pair")
            .1
            .same_issuer(&authority)
    );
    // The actual original child wrote one completed record before readiness.
    // 实际原子进程在就绪之前写入单个完成记录。
    let starts = std::fs::read_to_string(&marker).expect("actual startup record");
    assert_eq!(
        starts.lines().count(),
        1,
        "the expected original child count may have changed"
    );
    // The complete producer record provides exact PID evidence rather than publication authority.
    // 完整生产者记录提供精确 PID 证据，不提供发布权威。
    let actual: serde_json::Value =
        serde_json::from_str(starts.lines().next().expect("actual completed record"))
            .expect("actual startup JSON");
    // Resolve the real discovered business tool by stable original identity.
    // 按稳定原身份解析真实发现业务工具。
    let binding = committed
        .mcp_runtime()
        .available_tool_bindings_filtered(
            &vulcan_agent_runtime::mcp::McpRuntimeToolAllowlist::default(),
        )
        .into_iter()
        .find(|binding| binding.origin_tool_id() == "published-service:actual/original_process")
        .expect("actual original_process declaration may have changed");
    // Dropping the published foreground control must not close the still-owned actual service lease.
    // 销毁已发布前台控制绝不得关闭仍被拥有的实际服务租约。
    drop(native);
    // The real invocation must still use that exact original process through the full service owner.
    // 真实调用仍须通过完整服务所有者使用该精确原进程。
    let result = binding
        .invoke(serde_json::json!({"message":"actual service original"}))
        .await
        .expect("original business after controller loss");
    assert_eq!(result["pid"], actual["pid"]);
    assert!(actual["pid"].as_u64().expect("real original PID") > 0);
    assert_eq!(result["message"], "actual service original");
    assert_eq!(
        std::fs::read_to_string(&marker).expect("original startup remains unique"),
        starts
    );
    // The actual published access still observes and retires that original after its frontend controller is gone.
    // 前台控制消失之后，实际已发布访问仍观察及退役该原实例。
    let published_access = committed.mcp_runtime();
    published_access
        .begin_plugin_mcp_drain("published-service")
        .expect("original service-owned drain");
    published_access
        .wait_plugin_mcp_drain("published-service", OBSERVE)
        .await
        .expect("same actual original retired");
    observation
        .wait_until_released(OBSERVE)
        .await
        .expect("independent original physical cleanup");
    manager
        .begin_draining("published-service")
        .expect("actual disabling transition");
    manager
        .finish_lifecycle_transition(
            "published-service",
            &BTreeSet::from([String::from("published-service")]),
            &BTreeSet::new(),
            Vec::new(),
        )
        .expect("actual disabled source");
    assert!(environment.starts_with(_temp.path().canonicalize().expect("isolated fixture root")));
    std::fs::remove_file(environment.join("bin/server.bin"))
        .expect("remove isolated retired binary");
    assert_eq!(
        manager
            .prepare_plugin_environment("published-service")
            .expect("original service observation does not pin environment"),
        environment
    );
}
