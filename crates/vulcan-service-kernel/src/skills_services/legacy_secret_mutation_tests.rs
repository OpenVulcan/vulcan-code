//! Real Legacy credentials and MCP readiness exercise the retained cross-process mutation fence.
//! 真实旧凭据及 MCP 就绪检查验证保留的跨进程变更屏障。

#![cfg(windows)]

use super::*;
use std::path::PathBuf;
use std::time::Duration;
use vulcan_plugin_runtime::{VulcanPluginGenerationStore, VulcanPluginRuntimeManager};

mod initial_install;

/// One declared server and secret select only this fixture's unique OS account.
/// 单个已声明服务器及密钥仅选择本夹具的唯一操作系统账户。
const SERVER: &str = "actual";
/// The manifest declares this exact Legacy credential key.
/// 清单声明此精确旧凭据键。
const SECRET: &str = "token";
/// Native readiness observation has a bounded deadline independent of production policy.
/// 原生就绪观测具有独立于生产策略的有界截止时间。
const OBSERVE: Duration = Duration::from_secs(20);
/// All host-side native observation loops share this one polling interval.
/// 全部宿主侧原生观测循环共用此唯一轮询间隔。
const OBSERVATION_POLL_INTERVAL: Duration = Duration::from_millis(10);

/// Retain actual service events delivered through the domain RPC, without intercepting native or credential work.
/// 保留通过领域 RPC 投递的实际服务事件，不拦截原生或凭据工作。
#[derive(Default)]
struct RecoveryEvents {
    /// A one-shot actual client callback panic precedes queue mutation and never poisons the event mutex.
    /// 一次性实际客户端回调 panic 先于队列修改，绝不使事件互斥锁中毒。
    fail_before_delivery: std::sync::atomic::AtomicBool,
    /// The independent client queue contains only the service's public emitted events.
    /// 独立客户端队列仅包含服务公开发送的事件。
    events: Mutex<Vec<vulcan_service_api::ServiceEvent>>,
}

impl vulcan_service_api::ServiceEventSink for RecoveryEvents {
    /// Append the actual event to the client queue; event is the service projection and no value is returned.
    /// 将实际事件追加至客户端队列；event 为服务投影，不返回值。
    fn emit_service_event(&self, event: vulcan_service_api::ServiceEvent) {
        if self
            .fail_before_delivery
            .swap(false, std::sync::atomic::Ordering::AcqRel)
        {
            panic!("intentional actual client event delivery failure");
        }
        self.events
            .lock()
            .expect("actual recovery events")
            .push(event);
    }
}

/// Own an isolated real service, package and exact OS account; Drop cleans only its own resources.
/// 持有隔离真实服务、插件包及精确操作系统账户；析构仅清理自身资源。
struct Fixture {
    /// Temporary files contain no credential value.
    /// 临时文件不包含任何凭据值。
    root: tempfile::TempDir,
    /// Unique plugin identity isolates both secure storage and namespace locks.
    /// 唯一插件身份隔离安全存储及命名空间锁。
    plugin_id: String,
    /// Production manager resolves the exact installed fixture.
    /// 生产管理器解析精确已安装夹具。
    runtime: VulcanPluginRuntimeManager,
    /// Actual service state owns the extension pointer used by the full publisher.
    /// 真实服务状态持有完整发布器使用的扩展指针。
    state: SharedServiceBridgeState,
    /// Original inspected package supports real pending update reservations.
    /// 原已检查插件包支持真实待处理更新预留。
    source: PathBuf,
    /// Actual child writes this marker before answering MCP initialize.
    /// 实际子进程在回答 MCP 初始化之前写入此标记。
    ready_entry: PathBuf,
    /// Test releases the same child rather than replacing it with a mock runtime.
    /// 测试放行相同子进程，而非用模拟运行时替换它。
    ready_release: PathBuf,
    /// Every staged fixture revision is registered before any secure write for exact RAII cleanup.
    /// 每个暂存夹具修订在任何安全写入前登记，以便精确 RAII 清理。
    versioned_revisions: Vec<vulcan_plugin_runtime::VulcanPluginCredentialRevision>,
}

impl Fixture {
    /// Build an installed Legacy package and real service; enabled selects actual stdio MCP startup.
    /// 构建已安装旧格式插件包及真实服务；enabled 选择实际标准输入输出 MCP 启动。
    /// Returns isolated OS-account ownership without changing global keyring builders.
    /// 返回隔离操作系统账户所有权，不更改全局密钥库构建器。
    fn new(enabled: bool) -> Self {
        // Every fixture has a unique durable and credential namespace.
        // 每个夹具具有唯一持久及凭据命名空间。
        let plugin_id = format!("legacy-secret-{}", uuid::Uuid::new_v4().simple());
        // All package and observation bytes belong to this temporary root.
        // 全部插件包及观测字节属于此临时根。
        let root = tempfile::tempdir().expect("isolated Legacy secret workspace");
        // Engine-free packages exercise native MCP without a Lua dependency.
        // 无引擎插件包无需 Lua 依赖即可验证原生 MCP。
        let runtime = VulcanPluginRuntimeManager::without_runtime(
            root.path().join("bundled"),
            root.path().join("user"),
            root.path().join("bridge"),
            root.path().join("distributions"),
            root.path().join("state"),
            root.path().join("environments"),
        );
        // Exact child observations never contain the secret passed in its environment.
        // 精确子进程观测绝不包含传入其环境的密钥。
        let ready_entry = root.path().join("ready-entry.txt");
        // Only this path releases the already-started native process.
        // 仅此路径放行已启动原生进程。
        let ready_release = root.path().join("ready-release.txt");
        // The original source is installed through production inspection and authorization.
        // 原来源通过生产检查及授权安装。
        let source = root.path().join("source").join(&plugin_id);
        std::fs::create_dir_all(source.join("bin")).expect("package directory");
        // Disabled coverage needs no native dependency; its declared remote URL is never contacted.
        // 停用覆盖无需原生依赖；其已声明远端地址绝不会被连接。
        let declaration = if enabled {
            // Discover one actual Node executable using the existing native MCP fixture convention.
            // 使用既有原生 MCP 夹具约定发现单个实际 Node 可执行文件。
            let node = std::process::Command::new("node")
                .args(["-p", "process.execPath"])
                .output()
                .expect("installed Node dependency");
            assert!(node.status.success(), "actual Node discovery failed");
            // Discovery returns the sole executable authority, without trying candidate locations.
            // 发现返回唯一可执行权威，不尝试候选位置。
            let node_path = String::from_utf8(node.stdout).expect("UTF-8 Node path");
            std::fs::copy(node_path.trim(), source.join("bin/server.bin"))
                .expect("bundle actual Node");
            // Copy the repository's actual JSON-RPC fixture and add only native readiness coordination.
            // 复制仓库实际 JSON-RPC 夹具，仅增加原生就绪协调。
            let script = std::fs::read_to_string(
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../vulcan-agent-runtime/tests/fixtures/owner-mcp.mjs"),
            )
            .expect("actual owner MCP script")
            .replace(
                "appendFileSync, writeFileSync",
                "appendFileSync, writeFileSync, existsSync",
            )
            .replace(
                "if (request.method === 'initialize') {",
                r#"if (request.method === 'initialize') {
    // A real handshake remains pending until the test releases this exact process.
    // 实际握手保持待处理状态，直到测试放行此精确进程。
    writeFileSync(process.env.VULCAN_LEGACY_READY_ENTRY, `${process.pid}\n`, 'utf8');
    // A bounded deadline prevents a failed test from retaining the child forever.
    // 有界截止时间防止失败测试永久保留子进程。
    const deadline = Date.now() + 30000;
    while (!existsSync(process.env.VULCAN_LEGACY_READY_RELEASE)) {
      if (Date.now() >= deadline) throw new Error('Legacy fixture release deadline');
      await new Promise(resolve => setTimeout(resolve, 10));
    }
"#,
            );
            assert!(script.contains("Legacy fixture release deadline"));
            std::fs::write(source.join("server.mjs"), script).expect("native fixture script");
            std::fs::write(source.join("dependencies.yaml"), "{}\n").expect("bundled dependencies");
            std::fs::write(source.join("schema.json"), r#"{"type":"object","properties":{"entry":{"type":"string"},"release":{"type":"string"}},"required":["entry","release"],"additionalProperties":false}"#).expect("declared schema");
            std::fs::write(
                source.join("defaults.json"),
                serde_json::to_vec(&serde_json::json!({
                    "entry": ready_entry, "release": ready_release
                }))
                .expect("fixture defaults"),
            )
            .expect("configuration defaults");
            String::from(
                "permissions: [config, process_binary, process_session, secrets_read]\nconfig:\n  schema_file: schema.json\n  defaults_file: defaults.json\nexecution:\n  dependencies:\n    manifest: dependencies.yaml\n    install_mode: bundled\n  native_binaries:\n    - id: server\n      targets:\n        x86_64-pc-windows-msvc: bin/server.bin\n        aarch64-pc-windows-msvc: bin/server.bin\nmcp:\n  servers:\n    - id: actual\n      name: Actual Legacy server\n      transport:\n        type: stdio\n        launcher: {type: native_binary, binary_id: server}\n        args: [{source: literal, value: server.mjs}]\n        env:\n          FIXTURE_TOKEN: {source: secret, key: token}\n          VULCAN_LEGACY_READY_ENTRY: {source: config, key: entry}\n          VULCAN_LEGACY_READY_RELEASE: {source: config, key: release}\n",
            )
        } else {
            String::from(
                "permissions: [network_https, secrets_read]\nmcp:\n  servers:\n    - id: actual\n      name: Disabled Legacy server\n      transport:\n        type: http\n        url: {source: literal, value: 'https://example.test/mcp'}\n        headers:\n          Authorization: {source: secret, key: token}\n",
            )
        };
        std::fs::write(source.join("plugin.yaml"), format!("schema_version: 1\nid: {plugin_id}\nversion: 1.0.0\ndefault_enabled: {enabled}\ndisplay:\n  name: Legacy Secret fixture\n  capability: Exact retained mutation ownership\n{declaration}"))
            .expect("valid Legacy package");
        // Approval and content identity come from actual inspection.
        // 批准及内容身份来自实际检查。
        let preview = runtime
            .inspect_local_user_plugin(&source)
            .expect("actual package preview");
        runtime
            .install_local_user_plugin(
                &source,
                &preview.content_fingerprint,
                preview.permissions,
                false,
            )
            .expect("real Legacy installation");
        // Explicit policy gives the production catalog the intended runtime state.
        // 显式策略为生产目录赋予预期运行时状态。
        let disabled = if enabled {
            BTreeSet::new()
        } else {
            BTreeSet::from([plugin_id.clone()])
        };
        runtime
            .refresh_catalog(&disabled, &BTreeSet::new())
            .expect("Legacy catalog");
        // Service dependencies remain real while this fixture replaces only its plugin manager.
        // 服务依赖保持真实，本夹具仅替换其插件管理器。
        let mut bridge = crate::tests::test_support::test_agent_runtime_bridge();
        bridge.vulcan_plugin_runtime = runtime.clone();
        bridge.vulcan_plugin_native_mcp_host = runtime.register_native_mcp_host_adapter();
        bridge
            .provider_configuration
            .luaskills
            .disabled_vulcan_plugins = disabled;
        bridge.extension_generation = bridge
            .prepare_plugin_extension_generation(
                runtime.snapshot(),
                bridge.extension_generation.mcp_runtime(),
            )
            .expect("initial extension");
        // The full publisher operates on this exact shared service pointer.
        // 完整发布器操作此精确共享服务指针。
        let mut state =
            crate::tests::test_support::test_service_state_with_parent_session(SessionId::new_v4());
        state.agent_runtime = Some(bridge);
        Self {
            root,
            plugin_id,
            runtime,
            state: Arc::new(Mutex::new(state)),
            source,
            ready_entry,
            ready_release,
            versioned_revisions: Vec::new(),
        }
    }

    /// Prepare the exact inspected package and return its actual Store, package lease and complete approval.
    /// 准备精确已检查插件包，并返回其实际存储、插件包租约及完整批准。
    fn prepare_managed(
        &self,
    ) -> (
        VulcanPluginGenerationStore,
        vulcan_plugin_runtime::PinnedVulcanPluginGeneration,
        Vec<String>,
    ) {
        // Actual inspection supplies both immutable content identity and capability approval.
        // 实际检查同时提供不可变内容身份及能力批准。
        let preview = self
            .runtime
            .inspect_local_user_plugin(&self.source)
            .expect("managed credential fixture preview");
        // The exact configured user root owns every staged package and selected record.
        // 精确配置用户根持有每个暂存插件包及已选记录。
        let store = VulcanPluginGenerationStore::open(self.runtime.user_plugin_root())
            .expect("managed credential Store");
        // Production preparation verifies copied package bytes before returning ownership.
        // 生产准备在返回所有权前校验已复制插件包字节。
        let package = store
            .prepare(&self.source, &preview.content_fingerprint)
            .expect("real managed credential package");
        (store, package, preview.permissions)
    }

    /// Stage `value` under a fresh registered revision, then select `package` with its own default configuration.
    /// 在新登记修订下暂存 `value`，再以插件包自身默认配置选择 `package`。
    /// Returns the published selection and its revision; cleanup ownership is registered before the first OS write.
    /// 返回已发布选择及其修订；清理所有权在首次操作系统写入前登记。
    fn select_managed(
        &mut self,
        store: &VulcanPluginGenerationStore,
        package: &vulcan_plugin_runtime::PinnedVulcanPluginGeneration,
        permissions: Vec<String>,
        value: Option<&str>,
    ) -> (
        vulcan_plugin_runtime::VulcanPluginActivation,
        vulcan_plugin_runtime::VulcanPluginCredentialRevision,
    ) {
        let revision = vulcan_plugin_runtime::VulcanPluginCredentialRevision::allocate();
        self.versioned_revisions.push(revision.clone());
        // The complete declared slot inventory is supplied only to the existing real staging API.
        // 完整已声明槽位清单仅提供给既有真实暂存接口。
        let values: vulcan_plugin_runtime::VulcanPluginCredentialValues = BTreeMap::from([(
            vulcan_plugin_runtime::VulcanPluginCredentialKey {
                server_id: SERVER.into(),
                secret_key: SECRET.into(),
            },
            value.map(str::to_owned),
        )]);
        store
            .begin_credential_staging(package, revision.clone(), &values)
            .expect("real versioned staging owner")
            .write_and_finish(
                &crate::plugin_mcp_secret_store::OsPluginMcpSecretStore,
                &values,
            )
            .expect("actual OS staged credentials");
        // Only the enabled variant declares configuration; its own defaults are the only document selected.
        // 仅启用变体声明配置；其自身默认值是唯一选择的文档。
        let defaults = self.source.join("defaults.json");
        let configuration = if defaults.exists() {
            vulcan_plugin_runtime::VulcanPluginConfigurationSelection::Document {
                value: serde_json::from_slice(&std::fs::read(defaults).expect("fixture defaults"))
                    .expect("decode fixture defaults"),
            }
        } else {
            vulcan_plugin_runtime::VulcanPluginConfigurationSelection::NotDeclared {}
        };
        let selected = store
            .activate_configured(package, 0, permissions, revision.clone(), configuration)
            .expect("real managed selection publication");
        (selected, revision)
    }

    /// Refresh actual manager and bridge catalogs after a legal selection while preserving explicit disabled policy.
    /// 在合法选择后刷新实际管理器及桥目录，同时保留显式停用策略。
    fn refresh_selection(&self) {
        self.runtime
            .refresh_catalog(&BTreeSet::from([self.plugin_id.clone()]), &BTreeSet::new())
            .expect("actual selected managed catalog");
        // Actual publication receives the manager's complete selected descriptor and its retained credential binding.
        // 实际发布接收管理器完整已选描述符及其保留凭据绑定。
        let mut state = self.state.lock().expect("selected fixture state");
        // The existing real bridge remains the sole extension pointer owner.
        // 既有真实桥保持唯一扩展指针所有者身份。
        let bridge = state
            .agent_runtime
            .as_mut()
            .expect("selected fixture bridge");
        bridge.extension_generation = bridge
            .prepare_plugin_extension_generation(
                self.runtime.snapshot(),
                bridge.extension_generation.mcp_runtime(),
            )
            .expect("actual selected service extension");
    }

    /// Read exact persistent selection bytes; only actual missing records produce None.
    /// 读取精确持久选择字节；仅实际缺失记录返回空值。
    fn selection_bytes(&self) -> Option<Vec<u8>> {
        match std::fs::read(
            self.runtime
                .user_plugin_root()
                .join(".vulcan-generations/active")
                .join(format!("{}.json", self.plugin_id)),
        ) {
            Ok(bytes) => Some(bytes),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(_) => panic!("fixture selection bytes must remain readable"),
        }
    }

    /// Serialize actual public catalog rows without invoking any OS credential adapter.
    /// 序列化实际公共目录行，不调用任何操作系统凭据适配器。
    /// Returns the same production projection used for workbench plugin rows, containing no secret values.
    /// 返回与工作台插件行相同的生产投影，不包含秘密值。
    fn catalog_bytes(&self) -> Vec<u8> {
        // Stable service state supplies the real ordinary catalog and current MCP status.
        // 稳定服务状态提供真实普通目录及当前 MCP 状态。
        let state = self.state.lock().expect("catalog fixture state");
        // Actual bridge owns provider policy used by the public projection.
        // 实际桥持有公共投影使用的供应商策略。
        let bridge = state
            .agent_runtime
            .as_ref()
            .expect("catalog fixture bridge");
        // Projection reads descriptors and status only, never native credential values.
        // 投影仅读取描述符及状态，绝不读取原生凭据值。
        let rows = crate::skill_catalog_projection::plugins_from_catalogs(
            &cached_luaskills_catalog_from_state(&state),
            &bridge.provider_configuration,
            Some(self.runtime.snapshot().as_ref()),
            &bridge.extension_generation.mcp_runtime().server_states(),
        );
        serde_json::to_vec(&rows).expect("non-secret public catalog bytes")
    }

    /// Set only this fixture's initial OS value; absence remains an actual missing credential.
    /// 仅设置本夹具初始操作系统值；缺失保持实际不存在的凭据。
    fn initial(&self, value: Option<&str>) {
        if let Some(value) = value {
            crate::plugin_mcp_secret_store::OsPluginMcpSecretStore
                .write_secret(&self.plugin_id, SERVER, SECRET, value)
                .expect("fixture OS credential");
        }
    }

    /// Read this fixture's sole OS account for exact rollback assertions.
    /// 读取本夹具唯一操作系统账户，供精确回滚断言。
    fn value(&self) -> Option<String> {
        crate::plugin_mcp_secret_store::OsPluginMcpSecretStore
            .read_optional_secret(&self.plugin_id, SERVER, SECRET)
            .expect("exact fixture credential")
    }

    /// Resolve the sole actual declared native executable before mutation; return its real production path.
    /// 变更前解析唯一实际已声明原生可执行文件；返回其真实生产路径。
    fn native_command(&self) -> PathBuf {
        // Actual descriptor and credential materialization select the same fixture launcher as the service.
        // 实际描述符及凭据物化选择与服务相同夹具启动器。
        let catalog = self.runtime.snapshot();
        // Exact installed plugin identity is the only source of its declarations.
        // 精确已安装插件身份为其声明唯一来源。
        let plugin = catalog
            .plugins
            .get(&self.plugin_id)
            .expect("native fixture descriptor");
        // Production materialization derives the actual command without starting a process.
        // 生产物化派生实际命令，不启动进程。
        let configs = plugin_mcp_server_configs_for_descriptors(
            &self.runtime,
            &plugin.mcp_servers,
            &crate::plugin_mcp_secret_store::OsPluginMcpSecretStore,
        )
        .expect("actual native launcher");
        assert_eq!(configs.len(), 1);
        match &configs.iter().find(|config| matches!(&config.source, vulcan_agent_runtime::mcp::McpServerSource::Plugin { plugin_id, .. } if plugin_id == &self.plugin_id)).expect("exact native source").transport {
            vulcan_agent_runtime::mcp::McpServerTransportConfig::Stdio { command, .. } => PathBuf::from(command),
            _ => panic!("native fixture must declare stdio"),
        }
    }

    /// Invoke the real service mutation with an optional next value and return its management result.
    /// 使用可选下一值调用真实服务变更，并返回其管理结果。
    async fn mutate(&self, next: Option<&str>) -> Result<LuaSkillsCatalogUpdate, String> {
        mutate_plugin_mcp_secret(
            ServiceEventEmitter::new(Vec::new()),
            Arc::clone(&self.state),
            self.plugin_id.clone(),
            SERVER.into(),
            SECRET.into(),
            next.map(str::to_owned),
        )
        .await
    }

    /// Release the existing real initialize request without restarting the MCP process.
    /// 放行既有真实初始化请求，不重启 MCP 进程。
    fn release(&self) {
        std::fs::write(&self.ready_release, "release").expect("release exact child");
    }
}

impl Drop for Fixture {
    /// Clean the exact fixture account and request actual owned transports to stop on every exit.
    /// 每次退出均清理精确夹具账户，并请求实际自有传输停止。
    fn drop(&mut self) {
        // Release a blocked handshake so failure cleanup never leaves an intentionally paused child.
        // 放行被阻挡握手，确保失败清理绝不遗留故意暂停子进程。
        let _ = std::fs::write(&self.ready_release, "release");
        if let Ok(state) = self.state.lock()
            && let Some(runtime) = state.agent_runtime.as_ref()
        {
            runtime.extension_generation.mcp_runtime().shutdown();
        }
        // Exact UUID identity prevents cleanup from touching any user or parallel fixture account.
        // 精确 UUID 身份防止清理触及任何用户或并行夹具账户。
        let _ = crate::plugin_mcp_secret_store::OsPluginMcpSecretStore.delete_secret(
            &self.plugin_id,
            SERVER,
            SECRET,
        );
        for revision in &self.versioned_revisions {
            // Only this fixture's exact revision account is removed, independently of Legacy account cleanup.
            // 仅移除此夹具精确修订账户，与旧格式账户清理相互独立。
            let key = vulcan_plugin_runtime::VulcanPluginCredentialKey {
                server_id: SERVER.into(),
                secret_key: SECRET.into(),
            };
            let _ = vulcan_plugin_runtime::VulcanPluginCredentialBackend::delete(
                &crate::plugin_mcp_secret_store::OsPluginMcpSecretStore,
                &self.plugin_id,
                revision,
                &key,
            );
        }
    }
}

/// A completed actual native write with a lost return restores Some and None preimages under the same original guard.
/// 返回丢失的已完成实际原生写入，在相同原屏障下恢复存在及缺失两种前像。
#[tokio::test(flavor = "current_thread")]
async fn legacy_secret_mutation_native_write_return_loss_restores_exact_preimage() {
    for previous in [Some("original-fixture-value"), None] {
        // Unique OS namespace and scoped native observation are isolated for each preimage.
        // 每个前像的唯一操作系统命名空间及作用域原生观测相互隔离。
        let fixture = Fixture::new(false);
        fixture.initial(previous);
        // Exact bytes and pointer identity prove the failed setter did not publish or change a selection.
        // 精确字节及指针身份证明失败设置器未发布或更改选择。
        let selection_before = fixture.selection_bytes();
        let catalog_before = fixture.catalog_bytes();
        let snapshot_before = fixture.runtime.snapshot();
        let extension_before = fixture
            .state
            .lock()
            .expect("before native loss")
            .agent_runtime
            .as_ref()
            .expect("real native loss bridge")
            .extension_generation
            .clone();
        // A distinct Store probes the real namespace during the completed write, before rollback.
        // 不同存储在已完成写入期间、回滚之前探测真实命名空间。
        let observer = VulcanPluginGenerationStore::open(fixture.runtime.user_plugin_root())
            .expect("independent actual write contender");
        // Exact plugin identity belongs exclusively to the contender closure and contains no value.
        // 精确插件身份专属于竞争者闭包，且不包含值。
        let plugin_id = fixture.plugin_id.clone();
        // The scoped hook reads actual keyring state after set_password succeeded, then loses its return once.
        // 作用域钩子在 set_password 成功后读取实际密钥库状态，然后仅丢失一次返回。
        let fault =
            crate::plugin_mcp_secret_store::legacy_write_observation::Scope::lose_next_write(
                &fixture.plugin_id,
                SERVER,
                SECRET,
                move || {
                    matches!(observer.acquire_legacy_mutation_guard(&plugin_id, None),
                    Err(error) if error.code == vulcan_plugin_runtime::VulcanPluginStoreErrorCode::Busy)
                },
            );
        assert!(
            fixture
                .mutate(Some("candidate-fixture-value"))
                .await
                .is_err()
        );
        assert!(
            fault.confirmed_write(),
            "actual native readback must confirm the completed candidate write"
        );
        assert!(
            fault.confirmed_busy(),
            "the original guard must remain owned at the completed native write"
        );
        drop(fault);
        assert!(
            fixture.value().as_deref() == previous,
            "the exact OS preimage must be restored without logging values"
        );
        assert!(fixture.selection_bytes() == selection_before);
        assert!(fixture.catalog_bytes() == catalog_before);
        assert!(Arc::ptr_eq(&snapshot_before, &fixture.runtime.snapshot()));
        assert!(Arc::ptr_eq(
            &extension_before,
            &fixture
                .state
                .lock()
                .expect("after native loss")
                .agent_runtime
                .as_ref()
                .expect("real native loss bridge")
                .extension_generation
        ));
        // Successful same-owner rollback releases the native fence for an independent subsequent mutation.
        // 成功的相同所有者回滚释放原生屏障，供独立后续变更使用。
        VulcanPluginGenerationStore::open(fixture.runtime.user_plugin_root())
            .expect("post rollback contender")
            .acquire_legacy_mutation_guard(&fixture.plugin_id, None)
            .expect("actual same-owner rollback releases fence");
    }
}

/// Actual post-write panic leaves the original file guard and Some/None preimage recoverable by its original UUID.
/// 实际写后 panic 使原文件屏障及存在／缺失前像仍可通过其原 UUID 恢复。
#[tokio::test(flavor = "current_thread")]
async fn legacy_secret_mutation_post_write_panic_retains_original_recovery_owner() {
    for previous in [Some("original-fixture-value"), None] {
        // Only this unique real account is written or restored during the test.
        // 测试期间仅写入或恢复此唯一真实账户。
        let fixture = Fixture::new(false);
        fixture.initial(previous);
        // Independent filesystem acquisition proves physical lock ownership, rather than a copied status flag.
        // 独立文件系统获取证明物理锁所有权，而非复制状态标志。
        let observer = VulcanPluginGenerationStore::open(fixture.runtime.user_plugin_root())
            .expect("panic contender");
        // The probe closes over the exact original plugin namespace only.
        // 探针仅捕获精确原插件命名空间。
        let plugin_id = fixture.plugin_id.clone();
        // Panic occurs after real native readback, before the service setter receives a write result.
        // panic 发生在真实原生回读后，且在服务设置器收到写入结果前。
        let fault =
            crate::plugin_mcp_secret_store::legacy_write_observation::Scope::panic_next_write(
                &fixture.plugin_id,
                SERVER,
                SECRET,
                move || {
                    matches!(observer.acquire_legacy_mutation_guard(&plugin_id, None),
                    Err(error) if error.code == vulcan_plugin_runtime::VulcanPluginStoreErrorCode::Busy)
                },
            );
        assert!(
            fixture
                .mutate(Some("candidate-fixture-value"))
                .await
                .expect_err("actual management task must panic")
                .contains("任务异常结束")
        );
        assert!(fault.confirmed_write() && fault.confirmed_busy());
        drop(fault);
        assert!(fixture.value().as_deref() == Some("candidate-fixture-value"));
        // After the task has actually ended, the service still holds the same physical namespace file.
        // 任务实际结束后，服务仍持有相同物理命名空间文件。
        let store = VulcanPluginGenerationStore::open(fixture.runtime.user_plugin_root())
            .expect("post panic Store");
        assert!(
            matches!(store.acquire_legacy_mutation_guard(&fixture.plugin_id, None),
            Err(error) if error.code == vulcan_plugin_runtime::VulcanPluginStoreErrorCode::Busy)
        );
        // Query returns only the retained original identity; failure has not erased its exact resources.
        // 查询仅返回保留原身份；故障未抹除其精确资源。
        let requests = service_plugin_mcp_secret_mutation_requests(
            Arc::clone(&fixture.state),
            fixture.plugin_id.clone(),
        )
        .expect("panic request lookup");
        assert_eq!(requests.len(), 1);
        assert!(requests[0].phase == LegacySecretMutationPhase::CredentialMutation);
        // Wire serialization must never contain either real credential value.
        // 线协议序列化绝不能包含任一真实凭据值。
        let wire = serde_json::to_string(&requests).expect("redacted request status");
        assert!(
            !wire.contains("original-fixture-value") && !wire.contains("candidate-fixture-value")
        );
        // The actual domain router must expose exactly the same original non-secret status.
        // 实际领域路由必须暴露完全相同的原无秘密状态。
        let rpc_status = crate::rpc_dispatch::dispatch_service_rpc(
            ServiceEventEmitter::new(Vec::new()),
            Arc::clone(&fixture.state),
            "service_plugin_mcp_secret_mutation_requests".into(),
            Some(serde_json::json!({"pluginId": fixture.plugin_id})),
        )
        .await
        .expect("actual recovery status RPC");
        assert!(rpc_status == serde_json::to_value(&requests).expect("exact status serialization"));
        // Wrong UUID or namespace rejects before entering the actual OS adapter.
        // 错误 UUID 或命名空间在进入实际操作系统适配器之前拒绝。
        let observe = crate::plugin_mcp_secret_store::legacy_write_observation::Scope::count_access(
            &fixture.plugin_id,
            SERVER,
            SECRET,
        );
        assert!(
            service_recover_plugin_mcp_secret_mutation(
                ServiceEventEmitter::new(Vec::new()),
                Arc::clone(&fixture.state),
                fixture.plugin_id.clone(),
                uuid::Uuid::new_v4()
            )
            .await
            .is_err()
        );
        assert!(
            service_recover_plugin_mcp_secret_mutation(
                ServiceEventEmitter::new(Vec::new()),
                Arc::clone(&fixture.state),
                "different-plugin".into(),
                requests[0].request_id
            )
            .await
            .is_err()
        );
        assert_eq!(observe.access_count(), 0);
        drop(observe);
        assert!(crate::rpc_dispatch::dispatch_service_rpc(
            ServiceEventEmitter::new(Vec::new()), Arc::clone(&fixture.state),
            "service_recover_plugin_mcp_secret_mutation".into(),
            Some(serde_json::json!({"pluginId": fixture.plugin_id, "requestId": requests[0].request_id})),
        ).await.expect("recover exact original preimage through actual RPC").is_null());
        assert!(fixture.value().as_deref() == previous);
        assert!(
            service_plugin_mcp_secret_mutation_requests(
                Arc::clone(&fixture.state),
                fixture.plugin_id.clone()
            )
            .expect("terminal lookup")
            .is_empty()
        );
        store
            .acquire_legacy_mutation_guard(&fixture.plugin_id, None)
            .expect("confirmed recovery released actual fence");
        assert!(
            service_recover_plugin_mcp_secret_mutation(
                ServiceEventEmitter::new(Vec::new()),
                Arc::clone(&fixture.state),
                fixture.plugin_id.clone(),
                requests[0].request_id
            )
            .await
            .is_err()
        );
    }
}

/// Actual service shutdown drains a post-write panic using the same original owner before tearing down runtimes.
/// 实际服务关闭在拆卸运行时之前，使用相同原所有者排空写后 panic。
#[tokio::test(flavor = "current_thread")]
async fn legacy_secret_mutation_shutdown_restores_retained_pre_native_write() {
    // Real OS absence is the original preimage, without a fake credential backend.
    // 真实操作系统缺失为原前像，不使用伪造凭据后端。
    let fixture = Fixture::new(false);
    // The actual contender independently observes the original physical namespace lock.
    // 实际竞争者独立观察原物理命名空间锁。
    let store = VulcanPluginGenerationStore::open(fixture.runtime.user_plugin_root())
        .expect("shutdown contender");
    // The one-shot observer carries no credential bytes.
    // 一次性观察者不携带凭据字节。
    let plugin_id = fixture.plugin_id.clone();
    // The panic follows an actual successful native write and immediate native readback.
    // panic 跟随实际成功原生写入及即时原生回读。
    let fault = crate::plugin_mcp_secret_store::legacy_write_observation::Scope::panic_next_write(
        &fixture.plugin_id,
        SERVER,
        SECRET,
        move || {
            matches!(store.acquire_legacy_mutation_guard(&plugin_id, None),
                Err(error) if error.code == vulcan_plugin_runtime::VulcanPluginStoreErrorCode::Busy)
        },
    );
    assert!(
        fixture
            .mutate(Some("candidate-fixture-value"))
            .await
            .is_err()
    );
    assert!(fault.confirmed_write() && fault.confirmed_busy());
    drop(fault);
    // Close the same production shutdown admission before taking the retained original inventory.
    // 获取保留原清单之前，关闭相同生产关闭准入。
    let registry = {
        // The service mutex serializes closure with pre-write registration.
        // 服务互斥将关闭与写前登记串行化。
        let mut state = fixture.state.lock().expect("shutdown state");
        state.service_lifecycle_state.begin_shutdown_drain();
        Arc::clone(&state.pending_legacy_secret_mutations)
    };
    assert!(registry.require_empty_without_runtime().is_err());
    registry
        .drain_for_shutdown()
        .await
        .expect("actual same-owner pre-native shutdown drain");
    assert!(fixture.value().is_none());
    assert!(
        registry
            .list(&fixture.plugin_id)
            .expect("shutdown original list")
            .is_empty()
    );
    registry
        .require_empty_without_runtime()
        .expect("confirmed actual resource retirement");
    VulcanPluginGenerationStore::open(fixture.runtime.user_plugin_root())
        .expect("drained Store")
        .acquire_legacy_mutation_guard(&fixture.plugin_id, None)
        .expect("shutdown released original physical fence");
    assert!(
        fixture
            .mutate(Some("rejected-fixture-value"))
            .await
            .is_err()
    );
    assert!(fixture.value().is_none());
}

/// A real staged revision selection rejects Legacy mutation before any exact-account OS adapter entry.
/// 真实暂存第三版选择在任何精确账户操作系统适配器入口之前拒绝旧格式变更。
#[tokio::test(flavor = "current_thread")]
async fn legacy_secret_mutation_managed_revision_rejects_before_os_access() {
    // Both account namespaces belong exclusively to this fixture and are cleaned by its retained RAII owner.
    // 两种账户命名空间均专属于本夹具，并由其保留 RAII 所有者清理。
    let mut fixture = Fixture::new(false);
    fixture.initial(Some("untouched-legacy-fixture-value"));
    // Actual immutable preparation and complete approval precede secure-store staging.
    // 实际不可变准备及完整批准先于安全存储暂存。
    let (store, package, permissions) = fixture.prepare_managed();
    let (selected, revision) = fixture.select_managed(
        &store,
        &package,
        permissions,
        Some("selected-revision-fixture-value"),
    );
    fixture.refresh_selection();
    // Actual catalog capture retains the complete selected inventory and revision namespace.
    // 实际目录捕获保留完整已选清单及修订命名空间。
    let snapshot = fixture.runtime.snapshot();
    let plugin = &snapshot.plugins[&fixture.plugin_id];
    assert!(
        plugin
            .managed_package()
            .expect("revision-bound managed descriptor")
            .matches_activation(&selected)
    );
    assert!(
        matches!(&plugin.mcp_servers.iter().find(|server| server.id == SERVER)
        .expect("declared revision server").credential_binding,
        vulcan_plugin_runtime::VulcanPluginCredentialBinding::Revision(actual) if actual == &revision)
    );
    // Exact bytes represent the valid preexisting authority before the rejected setter runs.
    // 精确字节表示被拒设置器执行前的有效既存权威。
    let selection_before = fixture.selection_bytes();
    let catalog_before = fixture.catalog_bytes();
    // Exact-account counting is armed only after real setup, so rejection must have zero native adapter entries.
    // 精确账户计数仅在真实初始化后布防，因此拒绝必须具有零原生适配器入口。
    let observe = crate::plugin_mcp_secret_store::legacy_write_observation::Scope::count_access(
        &fixture.plugin_id,
        SERVER,
        SECRET,
    );
    for next in [Some("rejected-fixture-value"), None] {
        assert!(
            fixture
                .mutate(next)
                .await
                .expect_err("selected revisions reject Legacy mutations")
                .contains("Versioned plugin credentials require a credential revision transaction")
        );
    }
    assert!(
        observe.access_count() == 0,
        "immutable rejection must precede the first exact-account OS access"
    );
    drop(observe);
    assert!(fixture.value().as_deref() == Some("untouched-legacy-fixture-value"));
    assert!(
        crate::plugin_mcp_secret_store::OsPluginMcpSecretStore
            .read_selected_optional_secret(
                &fixture.plugin_id,
                SERVER,
                SECRET,
                &vulcan_plugin_runtime::VulcanPluginCredentialBinding::Revision(revision),
            )
            .expect("read exact unchanged selected revision")
            .as_deref()
            == Some("selected-revision-fixture-value")
    );
    assert!(fixture.selection_bytes() == selection_before);
    assert!(fixture.catalog_bytes() == catalog_before);
    assert!(Arc::ptr_eq(&snapshot, &fixture.runtime.snapshot()));
}

/// Observe a real child handshake marker within the fixture deadline; path names never select production behavior.
/// 在夹具截止时间内观察真实子进程握手标记；路径名称绝不选择生产行为。
async fn wait_for_entry(path: &std::path::Path) {
    tokio::time::timeout(OBSERVE, async {
        while !path.exists() {
            tokio::time::sleep(OBSERVATION_POLL_INTERVAL).await;
        }
    })
    .await
    .expect("actual MCP initialize must reach fixture marker");
}

/// Actual Ready retains the same namespace fence, then successful publication drops it before ordinary catalog refresh.
/// 实际就绪阶段保留相同命名空间屏障，随后成功发布在普通目录刷新前释放它。
#[tokio::test]
async fn legacy_secret_mutation_actual_ready_holds_fence_until_publish() {
    // Original OS state and all observations belong to this exact package.
    // 原操作系统状态及全部观测属于此精确插件包。
    let fixture = Fixture::new(true);
    fixture.initial(Some("old-fixture-value"));
    // The actual owned service task remains active while its child waits for real MCP initialization.
    // 实际自有服务任务在其子进程等待真实 MCP 初始化时保持活动。
    let operation = mutate_plugin_mcp_secret(
        ServiceEventEmitter::new(Vec::new()),
        Arc::clone(&fixture.state),
        fixture.plugin_id.clone(),
        SERVER.into(),
        SECRET.into(),
        Some("next-fixture-value".into()),
    );
    // Concurrent observation runs inside the same runtime without replacing the production writer.
    // 并发观测在相同运行时内执行，不替换生产写入者。
    let observe = async {
        wait_for_entry(&fixture.ready_entry).await;
        assert!(fixture.value().as_deref() == Some("next-fixture-value"));
        // Independent Store opening exercises the actual OS namespace lock across the awaited handshake.
        // 独立存储打开验证等待握手期间的实际操作系统命名空间锁。
        let observer = VulcanPluginGenerationStore::open(fixture.runtime.user_plugin_root())
            .expect("second Store");
        assert_eq!(
            observer
                .acquire_legacy_mutation_guard(&fixture.plugin_id, None)
                .expect_err("same namespace must remain exclusively owned during Ready")
                .code,
            vulcan_plugin_runtime::VulcanPluginStoreErrorCode::Busy
        );
        // The real early update writer must be rejected before candidate package effects.
        // 真实早期更新写入者必须在候选插件包副作用之前被拒绝。
        let preview = fixture
            .runtime
            .inspect_local_user_plugin(&fixture.source)
            .expect("candidate preview");
        assert_eq!(
            observer
                .begin_activation_preparation(
                    uuid::Uuid::new_v4(),
                    &fixture.source,
                    &preview.content_fingerprint,
                    None,
                    preview.permissions
                )
                .expect_err("update namespace Busy")
                .code,
            vulcan_plugin_runtime::VulcanPluginStoreErrorCode::Busy
        );
        assert!(
            !fixture
                .root
                .path()
                .join("user/.vulcan-generations/transactions")
                .join(&fixture.plugin_id)
                .exists()
        );
        fixture.release();
    };
    // Success includes the actual full publisher and ordinary final catalog refresh.
    // 成功包含实际完整发布器及普通最终目录刷新。
    let (result, ()) = tokio::join!(operation, observe);
    result.expect("real Ready and full service publication");
    assert_eq!(
        fixture.runtime.snapshot().plugins[&fixture.plugin_id].runtime_state,
        vulcan_plugin_runtime::VulcanPluginRuntimeState::Enabled
    );
    // A fresh original-namespace lock is possible only after the successful owner released its actual File.
    // 仅成功所有者释放实际文件后，才可能获得新的原命名空间锁。
    let observer = VulcanPluginGenerationStore::open(fixture.runtime.user_plugin_root())
        .expect("released Store");
    observer
        .acquire_legacy_mutation_guard(&fixture.plugin_id, None)
        .expect("success fence released");
}

/// A durable pending update rejects the Legacy setter before its first OS write.
/// 持久待处理更新在首次操作系统写入前拒绝旧格式设置器。
#[tokio::test]
async fn legacy_secret_mutation_pending_update_keeps_actual_os_value() {
    // Disabled fixture has no process or network side effects.
    // 停用夹具不产生进程或网络副作用。
    let fixture = Fixture::new(false);
    fixture.initial(Some("original-fixture-value"));
    // Real preparation owns the exact original request and persists its index.
    // 真实准备持有精确原请求并持久化其索引。
    let store = VulcanPluginGenerationStore::open(fixture.runtime.user_plugin_root())
        .expect("pending Store");
    let preview = fixture
        .runtime
        .inspect_local_user_plugin(&fixture.source)
        .expect("actual preview");
    let preparation = store
        .begin_activation_preparation(
            uuid::Uuid::new_v4(),
            &fixture.source,
            &preview.content_fingerprint,
            None,
            preview.permissions,
        )
        .expect("durable pending preparation");
    assert!(
        fixture
            .mutate(Some("rejected-fixture-value"))
            .await
            .is_err()
    );
    assert!(fixture.value().as_deref() == Some("original-fixture-value"));
    assert_eq!(
        preparation.intent().generation().plugin_id,
        fixture.plugin_id
    );
}

/// Missing required credentials fail materialization and restore the deleted value under its original guard.
/// 缺失必需凭据使物化失败，并在原屏障下恢复被删除值。
#[tokio::test]
async fn legacy_secret_mutation_delete_ready_failure_restores_original() {
    // The real server requires precisely the credential being removed.
    // 真实服务器精确要求正在删除的凭据。
    let fixture = Fixture::new(true);
    fixture.initial(Some("previous-fixture-value"));
    assert!(fixture.mutate(None).await.is_err());
    assert!(fixture.value().as_deref() == Some("previous-fixture-value"));
    assert!(
        !fixture.ready_entry.exists(),
        "missing Secret must fail before child startup"
    );
    // Rollback releases the same original namespace after the exact OS value was restored.
    // 回滚在精确操作系统值恢复后释放相同原命名空间。
    let store = VulcanPluginGenerationStore::open(fixture.runtime.user_plugin_root())
        .expect("rollback Store");
    store
        .acquire_legacy_mutation_guard(&fixture.plugin_id, None)
        .expect("rollback fence released");
}

/// A real managed selection replacing the captured flat descriptor rejects mutation before OS credential access.
/// 替换已捕获平铺描述符的真实受管选择，在操作系统凭据访问前拒绝变更。
#[tokio::test]
async fn legacy_secret_mutation_stale_exact_selection_preserves_os_value() {
    // The retained service catalog intentionally stays at its original flat Legacy package.
    // 保留服务目录故意停留在其原平铺旧格式插件包。
    let mut fixture = Fixture::new(false);
    fixture.initial(Some("original-fixture-value"));
    assert!(
        fixture.runtime.snapshot().plugins[&fixture.plugin_id]
            .managed_package()
            .is_none()
    );
    // An independent Store uses production inspection and immutable preparation for a real selection change.
    // 独立存储使用生产检查及不可变准备，产生真实选择变更。
    let store = VulcanPluginGenerationStore::open(fixture.runtime.user_plugin_root())
        .expect("independent selection writer");
    // Exact approval comes from the same actual source inspection rather than a synthetic activation record.
    // 精确批准来自相同实际来源检查，而非合成激活记录。
    let preview = fixture
        .runtime
        .inspect_local_user_plugin(&fixture.source)
        .expect("new managed selection preview");
    // The physical package identity is newly prepared while the service still holds its original snapshot.
    // 在服务仍持有原快照时，重新准备物理插件包身份。
    let package = store
        .prepare(&fixture.source, &preview.content_fingerprint)
        .expect("real managed package preparation");
    fixture.select_managed(&store, &package, preview.permissions, None);
    assert!(
        fixture.runtime.snapshot().plugins[&fixture.plugin_id]
            .managed_package()
            .is_none()
    );
    assert!(
        fixture
            .mutate(Some("rejected-fixture-value"))
            .await
            .expect_err("stale flat descriptor must reject selected managed authority")
            .contains("activation changed before mutation")
    );
    assert!(fixture.value().as_deref() == Some("original-fixture-value"));
    assert!(!fixture.ready_entry.exists());
}

/// Full publication failure after actual Ready restores both present and absent OS preimages under original ownership.
/// 实际就绪后完整发布失败，在原所有权下恢复存在及缺失两种操作系统前像。
#[tokio::test]
async fn legacy_secret_mutation_publisher_failure_restores_exact_preimage() {
    for previous in [Some("original-fixture-value"), None] {
        // Each case owns a distinct secure account and native process.
        // 每个案例持有不同安全账户及原生进程。
        let fixture = Fixture::new(true);
        fixture.initial(previous);
        fixture.release();
        // Capture the exact full extension pointer before intentionally poisoning its real preparation dependency.
        // 故意使真实准备依赖中毒之前，捕获精确完整扩展指针。
        let before = fixture
            .state
            .lock()
            .expect("healthy state")
            .agent_runtime
            .as_ref()
            .expect("real bridge")
            .extension_generation
            .clone();
        // Actual Hook mutex poisoning makes full publication fail after the MCP child has become Ready.
        // 实际 Hook 互斥锁中毒使完整发布在 MCP 子进程就绪之后失败。
        let hooks = Arc::clone(
            &fixture
                .state
                .lock()
                .expect("healthy state")
                .agent_runtime
                .as_ref()
                .expect("real bridge")
                .conversation_hook_runtime,
        );
        assert!(
            std::thread::spawn(move || {
                // Retained real mutex ownership makes this intentional failure visible to the publisher.
                // 保留真实互斥锁所有权，使此故意失败对发布器可见。
                let _guard = hooks.lock().expect("healthy Hook mutex");
                panic!("intentional Legacy publication fixture failure");
            })
            .join()
            .is_err()
        );
        assert!(
            fixture
                .mutate(Some("candidate-fixture-value"))
                .await
                .is_err()
        );
        assert!(
            fixture.ready_entry.exists(),
            "real initialization must precede publication failure"
        );
        assert!(fixture.value().as_deref() == previous);
        assert!(Arc::ptr_eq(
            &before,
            &fixture
                .state
                .lock()
                .expect("healthy state")
                .agent_runtime
                .as_ref()
                .expect("real bridge")
                .extension_generation
        ));
        // Exact original native retirement now precedes verified OS restoration and physical fence release.
        // 精确原原生退役现在先于验证操作系统恢复及物理屏障释放。
        let store = VulcanPluginGenerationStore::open(fixture.runtime.user_plugin_root())
            .expect("failed publish Store");
        store
            .acquire_legacy_mutation_guard(&fixture.plugin_id, None)
            .expect("actual native recovery releases original fence");
        // Positive original native and credential proof removes only this confirmed original request.
        // 正向原原生及凭据证明仅移除此已确认原请求。
        let requests = service_plugin_mcp_secret_mutation_requests(
            Arc::clone(&fixture.state),
            fixture.plugin_id.clone(),
        )
        .expect("retained original publication request");
        assert!(requests.is_empty());
        // Confirmed original recovery leaves no uncertain native owner to block service shutdown.
        // 已确认原恢复不遗留阻挡服务关闭的不确定原生所有者。
        let registry = Arc::clone(
            &fixture
                .state
                .lock()
                .expect("native failure registry")
                .pending_legacy_secret_mutations,
        );
        registry
            .drain_for_shutdown()
            .await
            .expect("confirmed original recovery permits shutdown");
        assert!(fixture.value().as_deref() == previous);
    }
}

/// A real Ready task panic retains its native lease until exact recovery proves process and credential retirement.
/// 真实就绪任务 panic 保留其原生租约，直至精确恢复证明进程及凭据退役。
#[tokio::test(flavor = "current_thread")]
async fn legacy_secret_mutation_ready_panic_recovers_exact_native_owner() {
    // This fixture owns one real secure account and one native launcher image.
    // 此夹具持有单个真实安全账户及单个原生启动器映像。
    let fixture = Fixture::new(true);
    fixture.initial(Some("original-fixture-value"));
    fixture.release();
    // Production resolution occurs before the mutation and grants no process startup.
    // 生产解析发生在变更之前，不授予进程启动。
    let executable = fixture.native_command();
    assert!(
        std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&executable)
            .is_ok()
    );
    // The actual full original is retained solely for pointer preservation assertions.
    // 实际完整原值仅为指针保留断言而保留。
    let before = Arc::clone(
        &fixture
            .state
            .lock()
            .expect("before Ready panic")
            .agent_runtime
            .as_ref()
            .expect("real bridge")
            .extension_generation,
    );
    // Inject only after actual initialize and full discovery, without replacing the native implementation.
    // 仅在实际初始化及完整发现后注入，不替换原生实现。
    let fault = legacy_native_observation::Scope::arm(
        &fixture.plugin_id,
        legacy_native_observation::Milestone::Ready,
    );
    assert!(
        fixture
            .mutate(Some("candidate-fixture-value"))
            .await
            .expect_err("actual Ready task panic")
            .contains("任务异常结束")
    );
    assert!(fault.fired());
    drop(fault);
    assert!(fixture.ready_entry.exists());
    assert!(fixture.value().as_deref() == Some("candidate-fixture-value"));
    assert!(
        std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&executable)
            .is_err(),
        "actual Ready process must still own its native image after the management task ended"
    );
    assert!(Arc::ptr_eq(
        &before,
        &fixture
            .state
            .lock()
            .expect("after Ready panic")
            .agent_runtime
            .as_ref()
            .expect("real bridge")
            .extension_generation
    ));
    // The public original UUID selects the retained actual controller, never a current-partition reconstruction.
    // 公共原 UUID 选择保留实际控制器，绝不重建当前分区。
    let requests = service_plugin_mcp_secret_mutation_requests(
        Arc::clone(&fixture.state),
        fixture.plugin_id.clone(),
    )
    .expect("actual Ready original lookup");
    assert_eq!(requests.len(), 1);
    assert!(requests[0].phase == LegacySecretMutationPhase::NativeRefresh);
    service_recover_plugin_mcp_secret_mutation(
        ServiceEventEmitter::new(Vec::new()),
        Arc::clone(&fixture.state),
        fixture.plugin_id.clone(),
        requests[0].request_id,
    )
    .await
    .expect("exact original native recovery");
    assert!(fixture.value().as_deref() == Some("original-fixture-value"));
    assert!(
        std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&executable)
            .is_ok(),
        "actual original cleanup must release the process image"
    );
    assert!(Arc::ptr_eq(
        &before,
        &fixture
            .state
            .lock()
            .expect("after original recovery")
            .agent_runtime
            .as_ref()
            .expect("real bridge")
            .extension_generation
    ));
    assert!(
        service_plugin_mcp_secret_mutation_requests(
            Arc::clone(&fixture.state),
            fixture.plugin_id.clone()
        )
        .expect("terminal original lookup")
        .is_empty()
    );
    VulcanPluginGenerationStore::open(fixture.runtime.user_plugin_root())
        .expect("recovered native Store")
        .acquire_legacy_mutation_guard(&fixture.plugin_id, None)
        .expect("actual native recovery releases original namespace");
}

/// A lost return after actual full publication preserves the new value and same native PID when original ownership retires.
/// 实际完整发布后返回丢失，在原所有权退役时保留新值及相同原生 PID。
#[tokio::test(flavor = "current_thread")]
async fn legacy_secret_mutation_published_return_loss_keeps_committed_native_owner() {
    // All actual package, account and process state belongs to this unique fixture.
    // 全部实际插件包、账户及进程状态属于此唯一夹具。
    let fixture = Fixture::new(true);
    fixture.initial(Some("original-fixture-value"));
    fixture.release();
    // Retain the actual predecessor pointer without creating a replacement publication receipt.
    // 保留实际前驱指针，不创建替代发布回执。
    let before = Arc::clone(
        &fixture
            .state
            .lock()
            .expect("before full commit loss")
            .agent_runtime
            .as_ref()
            .expect("real bridge")
            .extension_generation,
    );
    // Panic occurs after service lock release and adjacent full/native confirmation, avoiding mutex poisoning.
    // panic 发生在服务锁释放及相邻完整／原生确认之后，避免互斥锁中毒。
    let fault = legacy_native_observation::Scope::arm(
        &fixture.plugin_id,
        legacy_native_observation::Milestone::Published,
    );
    assert!(
        fixture
            .mutate(Some("candidate-fixture-value"))
            .await
            .is_err()
    );
    assert!(fault.fired());
    drop(fault);
    // This is the actual committed full pointer and its currently published execution access.
    // 此为实际已提交完整指针及其当前已发布执行访问。
    let committed = Arc::clone(
        &fixture
            .state
            .lock()
            .expect("actual committed state")
            .agent_runtime
            .as_ref()
            .expect("real bridge")
            .extension_generation,
    );
    assert!(!Arc::ptr_eq(&before, &committed));
    // The marker and real tool reply must identify the same original process.
    // 标记及真实工具应答必须标识相同原进程。
    let pid: u32 = std::fs::read_to_string(&fixture.ready_entry)
        .expect("actual committed PID marker")
        .trim()
        .parse()
        .expect("decimal native PID");
    // Resolve by the declared tool feature rather than a positional catalog index.
    // 按已声明工具特征解析，而非使用目录位置索引。
    let bindings = committed
        .mcp_runtime()
        .available_tool_bindings_filtered(&Default::default());
    let binding = bindings
        .iter()
        .find(|binding| binding.origin_tool_name() == Some("read_page"))
        .expect("actual committed native tool");
    // The machine projection decodes the fixture's JSON text into the declared result object.
    // 机器投影将夹具 JSON 文本解码为已声明结果对象。
    let reply = binding
        .invoke(serde_json::json!({"url":"https://original.test", "owner_session_id":"fixture"}))
        .await
        .expect("committed business opened");
    assert!(reply["process_id"] == pid);
    // Adjacent confirmation recorded Committed even though the management return was lost.
    // 即使管理返回丢失，相邻确认仍记录已提交。
    let requests = service_plugin_mcp_secret_mutation_requests(
        Arc::clone(&fixture.state),
        fixture.plugin_id.clone(),
    )
    .expect("committed original lookup");
    assert_eq!(requests.len(), 1);
    assert!(requests[0].phase == LegacySecretMutationPhase::Committed);
    // Actual projection failure must preserve this same committed owner, native process and unfinished client update.
    // 实际投影失败须保留此相同已提交所有者、原生进程及未完成客户端更新。
    let (hooks, workbench_before) = {
        // Capture exact current ownership under the actual service mutex.
        // 在实际服务互斥锁下捕获精确当前所有权。
        let state = fixture.state.lock().expect("before projection recovery");
        (
            Arc::clone(
                &state
                    .agent_runtime
                    .as_ref()
                    .expect("real bridge")
                    .conversation_hook_runtime,
            ),
            state.workbench_generation,
        )
    };
    // This real mutex failure occurs after native publication, before projection's own full write.
    // 此真实互斥锁故障发生在原生发布之后、投影自身完整写入之前。
    let poisoned = Arc::clone(&hooks);
    assert!(
        std::thread::spawn(move || {
            // The panic intentionally poisons only the existing projection dependency.
            // panic 故意仅使现有投影依赖中毒。
            let _guard = poisoned.lock().expect("healthy projection Hook mutex");
            panic!("intentional committed projection fixture failure");
        })
        .join()
        .is_err()
    );
    // The real event emitter delivers the recovery RPC's sole committed projection.
    // 真实事件发送器投递恢复 RPC 的唯一已提交投影。
    let events = Arc::new(RecoveryEvents::default());
    // This same actual client queue is retained through failed and successful attempts.
    // 此相同实际客户端队列跨失败及成功尝试保留。
    let app = ServiceEventEmitter::new(vec![events.clone()]);
    // A committed recovery must never write or read the OS account again.
    // 已提交恢复绝不能再次写入或读取操作系统账户。
    let access = crate::plugin_mcp_secret_store::legacy_write_observation::Scope::count_access(
        &fixture.plugin_id,
        SERVER,
        SECRET,
    );
    assert!(crate::rpc_dispatch::dispatch_service_rpc(
        app.clone(), Arc::clone(&fixture.state),
        "service_recover_plugin_mcp_secret_mutation".into(),
        Some(serde_json::json!({"pluginId": fixture.plugin_id, "requestId": requests[0].request_id})),
    ).await.is_err());
    assert_eq!(access.access_count(), 0);
    assert!(
        events
            .events
            .lock()
            .expect("failed recovery events")
            .is_empty()
    );
    assert!(
        service_plugin_mcp_secret_mutation_requests(
            Arc::clone(&fixture.state),
            fixture.plugin_id.clone()
        )
        .expect("same committed request after projection failure")
        .iter()
        .any(|status| status.request_id == requests[0].request_id
            && status.phase == LegacySecretMutationPhase::Committed)
    );
    assert!(Arc::ptr_eq(
        &committed,
        &fixture
            .state
            .lock()
            .expect("after failed committed projection")
            .agent_runtime
            .as_ref()
            .expect("real bridge")
            .extension_generation
    ));
    VulcanPluginGenerationStore::open(fixture.runtime.user_plugin_root())
        .expect("committed projection contender")
        .acquire_legacy_mutation_guard(&fixture.plugin_id, None)
        .err()
        .filter(|error| error.code == vulcan_plugin_runtime::VulcanPluginStoreErrorCode::Busy)
        .expect("failed projection retains the same actual file guard");
    hooks.clear_poison();
    assert!(crate::rpc_dispatch::dispatch_service_rpc(
        app, Arc::clone(&fixture.state),
        "service_recover_plugin_mcp_secret_mutation".into(),
        Some(serde_json::json!({"pluginId": fixture.plugin_id, "requestId": requests[0].request_id})),
    ).await.expect("actual committed recovery RPC").is_null());
    assert_eq!(access.access_count(), 0);
    drop(access);
    assert!(fixture.value().as_deref() == Some("candidate-fixture-value"));
    // Projection advances only its complete service pointer while retaining the same original native lease.
    // 投影仅推进其完整服务指针，同时保留相同原原生租约。
    {
        let state = fixture
            .state
            .lock()
            .expect("completed committed projection");
        assert_eq!(state.workbench_generation, workbench_before + 1);
        assert_eq!(state.workbench.revision, state.workbench_generation);
        assert_eq!(
            state
                .agent_runtime
                .as_ref()
                .expect("real bridge")
                .extension_generation
                .revision(),
            committed.revision() + 1
        );
        // Exactly one actual client event has the same authoritative workbench revision and no credential bytes.
        // 恰好一个实际客户端事件具有相同权威工作台修订，且没有凭据字节。
        let delivered = events.events.lock().expect("delivered committed recovery");
        assert_eq!(delivered.len(), 1);
        assert!(delivered.iter().any(|event| matches!(event,
            vulcan_service_api::ServiceEvent::WorkbenchDelta(envelope)
            if envelope.generation == state.workbench_generation
                && matches!(&envelope.delta, vulcan_service_api::WorkbenchDelta::PluginCatalogChanged(_)))));
        // Serialization exercises the actual public event, independently of status redaction.
        // 序列化验证实际公开事件，与状态脱敏相互独立。
        let wire = serde_json::to_string(&*delivered).expect("actual public recovery projection");
        assert!(
            !wire.contains("original-fixture-value") && !wire.contains("candidate-fixture-value")
        );
    }
    // Dropping historical control cannot shut down the same actually published native process.
    // 销毁历史控制不能关闭相同实际已发布原生进程。
    let again = binding
        .invoke(serde_json::json!({"url":"https://original.test", "owner_session_id":"fixture"}))
        .await
        .expect("same committed process after control retirement");
    assert!(again["process_id"] == pid);
    assert!(
        service_plugin_mcp_secret_mutation_requests(
            Arc::clone(&fixture.state),
            fixture.plugin_id.clone()
        )
        .expect("retired committed request")
        .is_empty()
    );
}

/// Recovery retires only the failed replacement while an actually running predecessor retains its original full pointer and PID.
/// 恢复仅退役失败替换，实际运行中的前驱保留其原完整指针及 PID。
#[tokio::test(flavor = "current_thread")]
async fn legacy_secret_mutation_ready_panic_preserves_actual_live_predecessor() {
    // The unique fixture contains an actual package, OS credential and native process.
    // 唯一夹具包含实际插件包、操作系统凭据及原生进程。
    let fixture = Fixture::new(true);
    fixture.initial(Some("original-fixture-value"));
    fixture.release();
    fixture
        .mutate(Some("original-fixture-value"))
        .await
        .expect("actual predecessor full publication");
    // Capture the original actually published full service owner after its workbench projection.
    // 在工作台投影后捕获原实际已发布完整服务所有者。
    let before = Arc::clone(
        &fixture
            .state
            .lock()
            .expect("actual predecessor state")
            .agent_runtime
            .as_ref()
            .expect("real bridge")
            .extension_generation,
    );
    // Resolve by the fixture's declared tool identity, not by mutable catalog order.
    // 按夹具已声明工具身份解析，不依赖可变目录顺序。
    let bindings = before
        .mcp_runtime()
        .available_tool_bindings_filtered(&Default::default());
    // This binding retains the exact execution lease consumed by its actual predecessor.
    // 此绑定保留其实际前驱消费的精确执行租约。
    let binding = bindings
        .iter()
        .find(|binding| binding.origin_tool_name() == Some("read_page"))
        .expect("actual live predecessor tool");
    // Machine projection already decoded the fixture's real JSON response.
    // 机器投影已解码夹具真实 JSON 响应。
    let reply = binding
        .invoke(serde_json::json!({"url":"https://predecessor.test", "owner_session_id":"fixture"}))
        .await
        .expect("actual predecessor business before replacement");
    // The stable PID comes from a real tools/call, not from a copied configuration or ready flag.
    // 稳定 PID 来自真实 tools/call，而非复制配置或就绪标志。
    let original_pid = reply["process_id"]
        .as_u64()
        .expect("actual predecessor PID");
    // Panic follows replacement's actual initialize and tools discovery, before any full write.
    // panic 发生在替换实际初始化及工具发现之后、任何完整写入之前。
    let fault = legacy_native_observation::Scope::arm(
        &fixture.plugin_id,
        legacy_native_observation::Milestone::Ready,
    );
    assert!(
        fixture
            .mutate(Some("candidate-fixture-value"))
            .await
            .is_err()
    );
    assert!(fault.fired());
    drop(fault);
    // The second actual initialization belongs to a distinct native process.
    // 第二次实际初始化属于不同原生进程。
    let replacement_pid: u64 = std::fs::read_to_string(&fixture.ready_entry)
        .expect("actual replacement marker")
        .trim()
        .parse()
        .expect("decimal replacement PID");
    assert_ne!(replacement_pid, original_pid);
    assert!(Arc::ptr_eq(
        &before,
        &fixture
            .state
            .lock()
            .expect("unpublished replacement state")
            .agent_runtime
            .as_ref()
            .expect("real bridge")
            .extension_generation
    ));
    // Candidate failure cannot close or reconstruct the actually published predecessor.
    // 候选失败不能关闭或重建实际已发布前驱。
    let during = binding
        .invoke(serde_json::json!({"url":"https://predecessor.test", "owner_session_id":"fixture"}))
        .await
        .expect("same live predecessor while replacement is unfinished");
    assert_eq!(during["process_id"].as_u64(), Some(original_pid));
    // Recovery uses the exact registered original UUID without consulting a latest activation.
    // 恢复使用精确已登记原 UUID，不查阅最新激活。
    let requests = service_plugin_mcp_secret_mutation_requests(
        Arc::clone(&fixture.state),
        fixture.plugin_id.clone(),
    )
    .expect("exact replacement original lookup");
    assert_eq!(requests.len(), 1);
    service_recover_plugin_mcp_secret_mutation(
        ServiceEventEmitter::new(Vec::new()),
        Arc::clone(&fixture.state),
        fixture.plugin_id.clone(),
        requests[0].request_id,
    )
    .await
    .expect("actual replacement cleanup and exact OS restoration");
    assert!(fixture.value().as_deref() == Some("original-fixture-value"));
    assert!(Arc::ptr_eq(
        &before,
        &fixture
            .state
            .lock()
            .expect("recovered predecessor full state")
            .agent_runtime
            .as_ref()
            .expect("real bridge")
            .extension_generation
    ));
    // Same binding and PID prove recovery neither replayed initialization nor replaced predecessor ownership.
    // 相同绑定及 PID 证明恢复未重放初始化或替换前驱所有权。
    let after = binding
        .invoke(serde_json::json!({"url":"https://predecessor.test", "owner_session_id":"fixture"}))
        .await
        .expect("same live predecessor after replacement retirement");
    assert_eq!(after["process_id"].as_u64(), Some(original_pid));
    assert!(
        service_plugin_mcp_secret_mutation_requests(
            Arc::clone(&fixture.state),
            fixture.plugin_id.clone()
        )
        .expect("confirmed replacement terminal state")
        .is_empty()
    );
    VulcanPluginGenerationStore::open(fixture.runtime.user_plugin_root())
        .expect("predecessor recovery Store")
        .acquire_legacy_mutation_guard(&fixture.plugin_id, None)
        .expect("actual replacement recovery releases its original fence");
}

/// An original Ready candidate cannot overwrite a later actual disabled catalog while resuming its full publication.
/// 原就绪候选在恢复完整发布时，不能覆盖随后实际停用目录。
#[tokio::test(flavor = "current_thread")]
async fn legacy_secret_mutation_original_ready_rejects_later_actual_catalog() {
    // Real package, secure account and process belong only to this isolated fixture.
    // 真实插件包、安全账户及进程仅属于此隔离夹具。
    let fixture = Fixture::new(true);
    fixture.initial(Some("original-fixture-value"));
    fixture.release();
    // This is the actual catalog consumed by the first management request, before any OS effect.
    // 此为首个管理请求在任何操作系统副作用之前消费的实际目录。
    let captured = fixture.runtime.snapshot();
    // Pause by a real task panic after discovery; the retained original owns all later recovery work.
    // 在真实发现后通过实际任务 panic 暂停；保留原实例持有全部后续恢复工作。
    let fault = legacy_native_observation::Scope::arm(
        &fixture.plugin_id,
        legacy_native_observation::Milestone::Ready,
    );
    assert!(
        fixture
            .mutate(Some("candidate-fixture-value"))
            .await
            .is_err()
    );
    assert!(fault.fired());
    drop(fault);
    // An explicit policy refresh publishes a real later catalog from the production manager.
    // 显式策略刷新从生产管理器发布真实后续目录。
    fixture
        .runtime
        .refresh_catalog(
            &BTreeSet::from([fixture.plugin_id.clone()]),
            &BTreeSet::new(),
        )
        .expect("actual later disabled catalog");
    // A retained namespace may make this row unavailable; either actual state rejects enabled publication.
    // 保留命名空间可能使此行不可用；两种实际状态均拒绝启用发布。
    let later = fixture.runtime.snapshot();
    assert_ne!(
        later.plugins[&fixture.plugin_id].generation,
        captured.plugins[&fixture.plugin_id].generation
    );
    assert!(
        later.plugins[&fixture.plugin_id].runtime_state
            != vulcan_plugin_runtime::VulcanPluginRuntimeState::Enabled
    );
    vulcan_plugin_lifecycle::publish_disabled_plugin_extension_generation(
        &fixture.state,
        &fixture.plugin_id,
        Arc::clone(&later),
    )
    .expect("actual later full disabled publication");
    // The same service slot now holds the real later full pointer and its current routes.
    // 相同服务槽现在持有真实后续完整指针及其当前路由。
    let expected = Arc::clone(
        &fixture
            .state
            .lock()
            .expect("later actual service owner")
            .agent_runtime
            .as_ref()
            .expect("real bridge")
            .extension_generation,
    );
    // Exact request lookup supplies the retained original, without constructing a new native attempt.
    // 精确请求查询提供保留原实例，不构造新的原生尝试。
    let requests = service_plugin_mcp_secret_mutation_requests(
        Arc::clone(&fixture.state),
        fixture.plugin_id.clone(),
    )
    .expect("original Ready request");
    assert_eq!(requests.len(), 1);
    // Obtain the actual service registry only; public status cannot issue native publication authority.
    // 仅取得实际服务注册表；公共状态不能签发原生发布权威。
    let registry = Arc::clone(
        &fixture
            .state
            .lock()
            .expect("original registry")
            .pending_legacy_secret_mutations,
    );
    // This is the same original UUID's strong owner, retained before its first write.
    // 此为相同原 UUID 的强所有者，在其首次写入之前即被保留。
    let owner = registry
        .get(&fixture.plugin_id, requests[0].request_id)
        .expect("exact original owner");
    // Resuming the original borrowed publication must reject before touching the actual later full pointer.
    // 恢复原借用发布须在触碰实际后续完整指针之前拒绝。
    let mut resources = owner.resources.lock().await;
    assert!(
        vulcan_plugin_lifecycle::publish_legacy_secret_extension_generation(
            &fixture.state,
            &fixture.plugin_id,
            captured,
            &mut resources,
        )
        .is_err(),
        "a stale original catalog cannot overwrite actual later routes"
    );
    assert!(resources.phase == LegacySecretMutationPhase::NativeRefresh);
    assert!(Arc::ptr_eq(
        &expected,
        &fixture
            .state
            .lock()
            .expect("later pointer after refusal")
            .agent_runtime
            .as_ref()
            .expect("real bridge")
            .extension_generation
    ));
    drop(resources);
    drop(owner);
    service_recover_plugin_mcp_secret_mutation(
        ServiceEventEmitter::new(Vec::new()),
        Arc::clone(&fixture.state),
        fixture.plugin_id.clone(),
        requests[0].request_id,
    )
    .await
    .expect("same original native retirement and OS restoration after stale refusal");
    assert!(fixture.value().as_deref() == Some("original-fixture-value"));
    assert!(Arc::ptr_eq(
        &expected,
        &fixture
            .state
            .lock()
            .expect("actual later owner after recovery")
            .agent_runtime
            .as_ref()
            .expect("real bridge")
            .extension_generation
    ));
}

/// Full Legacy publication merges a later actual user successor without replacing its native lease or PID.
/// 完整旧格式发布合并随后实际用户后继，不替换其原生租约或 PID。
#[tokio::test(flavor = "current_thread")]
async fn legacy_secret_mutation_original_ready_preserves_later_actual_user_successor() {
    // Actual package and secure account belong to this isolated service instance.
    // 实际插件包及安全账户属于此隔离服务实例。
    let fixture = Fixture::new(true);
    fixture.initial(Some("original-fixture-value"));
    fixture.release();
    // Capture the same original catalog consumed before the first native effect.
    // 捕获首次原生副作用之前消费的相同原目录。
    let captured = fixture.runtime.snapshot();
    // The actual task ends after discovery while its original native lease remains service-owned.
    // 实际任务在发现之后结束，而其原原生租约仍由服务拥有。
    let fault = legacy_native_observation::Scope::arm(
        &fixture.plugin_id,
        legacy_native_observation::Milestone::Ready,
    );
    assert!(
        fixture
            .mutate(Some("candidate-fixture-value"))
            .await
            .is_err()
    );
    assert!(fault.fired());
    drop(fault);
    // This independent startup record belongs only to the user's real native process.
    // 此独立启动记录仅属于用户真实原生进程。
    let marker = fixture.root.path().join("actual-user-successor.txt");
    // The original service access supplies the exact shared admission and ownership lineage.
    // 原服务访问提供精确共享准入及所有权谱系。
    let current = fixture
        .state
        .lock()
        .expect("actual pre-user full pointer")
        .agent_runtime
        .as_ref()
        .expect("real bridge")
        .extension_generation
        .mcp_runtime();
    // The declared user definition uses the real repository fixture and inherits the actual Windows environment.
    // 已声明用户定义使用真实仓库夹具，并继承实际 Windows 环境。
    let server = vulcan_agent_runtime::mcp::McpServerConfig {
        source: vulcan_agent_runtime::mcp::McpServerSource::UserConfigured,
        id: "user-during-legacy".into(),
        name: "Actual later user successor".into(),
        enabled: true,
        transport: vulcan_agent_runtime::mcp::McpServerTransportConfig::Stdio {
            command: "node".into(),
            args: vec![
                std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../vulcan-agent-runtime/tests/fixtures/owner-mcp.mjs")
                    .to_string_lossy()
                    .into_owned(),
            ],
            cwd: None,
            env: BTreeMap::from([(
                "VULCAN_MCP_PROCESS_MARKER".into(),
                marker.to_string_lossy().into_owned(),
            )]),
            env_passthrough: Vec::new(),
            inherit_environment: true,
        },
    };
    // A real configuration transaction starts and writes the actual user successor full pointer.
    // 真实配置事务启动并写入实际用户后继完整指针。
    let successor = {
        // This is the same serialization used by production provider and MCP configuration changes.
        // 此为生产 Provider 及 MCP 配置变更使用的相同串行化机制。
        let _configuration_transaction =
            crate::service_state_access::begin_provider_configuration_transaction().await;
        // Dispatch through the actual public user factory, without changing or restarting the retained Legacy original.
        // 经实际公开用户工厂派发，不改变或重启保留旧格式原实例。
        let successor =
            vulcan_agent_runtime::mcp::replace_user_mcp_runtime_partition(&current, vec![server])
                .await
                .expect("actual later user dispatch");
        // The current persisted-policy projection remains the configuration authority.
        // 当前持久策略投影保持配置权威。
        let configuration =
            crate::service_state_access::provider_configuration_from_state(&fixture.state)
                .expect("actual current provider configuration");
        crate::settings_runtime_projection::store_provider_and_mcp_configuration_after_change(
            &fixture.state,
            configuration,
            successor.clone(),
        )
        .expect("actual later user full publication");
        successor
    };
    // Resolve only actual complete native discovery within the shared observation deadline.
    // 仅在共享观测期限内解析实际完整原生发现。
    let user = tokio::time::timeout(OBSERVE, async {
        loop {
            if let Some(binding) = successor
                .available_tool_bindings_filtered(&Default::default())
                .into_iter()
                .find(|binding| binding.origin_tool_id() == "user-during-legacy/read_page")
            {
                return binding;
            }
            tokio::time::sleep(OBSERVATION_POLL_INTERVAL).await;
        }
    })
    .await
    .expect("actual later user initialization and discovery");
    // Actual invocation identifies the user process independently of plugin readiness.
    // 实际调用独立于插件就绪标识用户进程。
    let before = user
        .invoke(serde_json::json!({"url":"https://user.test", "owner_session_id":"fixture"}))
        .await
        .expect("actual later user before Legacy full commit");
    // Exact lookup returns the same service-owned native replacement prepared before the task panic.
    // 精确查询返回任务 panic 之前准备的相同服务自有原生替换。
    let requests = service_plugin_mcp_secret_mutation_requests(
        Arc::clone(&fixture.state),
        fixture.plugin_id.clone(),
    )
    .expect("retained original before same-lease handoff");
    assert_eq!(requests.len(), 1);
    // Original registry ownership is used directly, without projecting public status into authority.
    // 直接使用原注册表所有权，不将公共状态投影为权威。
    let registry = Arc::clone(
        &fixture
            .state
            .lock()
            .expect("actual original registry")
            .pending_legacy_secret_mutations,
    );
    // The same allocated owner supplies the exclusive native borrow through actual full publication.
    // 相同已分配所有者跨实际完整发布提供排他原生借用。
    let owner = registry
        .get(&fixture.plugin_id, requests[0].request_id)
        .expect("actual same original owner");
    // This exact borrow cannot be moved into another native attempt or ownership registry.
    // 此精确借用不能移入另一原生尝试或所有权注册表。
    let mut resources = owner.resources.lock().await;
    vulcan_plugin_lifecycle::publish_legacy_secret_extension_generation(
        &fixture.state,
        &fixture.plugin_id,
        captured,
        &mut resources,
    )
    .expect("actual full commit merges later user");
    assert!(resources.phase == LegacySecretMutationPhase::Committed);
    drop(resources);
    drop(owner);
    // The resulting actual full pointer contains the same user execution lease as the later configuration successor.
    // 结果实际完整指针包含与后续配置后继相同的用户执行租约。
    let full = Arc::clone(
        &fixture
            .state
            .lock()
            .expect("actual merged full pointer")
            .agent_runtime
            .as_ref()
            .expect("real bridge")
            .extension_generation,
    );
    assert!(
        full.mcp_runtime()
            .shares_server_partition(&successor, "user-during-legacy")
    );
    // The actual plugin marker identifies its unchanged original process.
    // 实际插件标记标识其未变化原进程。
    let plugin_pid: u64 = std::fs::read_to_string(&fixture.ready_entry)
        .expect("actual original plugin PID")
        .trim()
        .parse()
        .expect("decimal plugin PID");
    // Both server identities are selected explicitly from the real merged discovery surface.
    // 两个服务器身份均从真实合并发现面中显式选择。
    let bindings = full
        .mcp_runtime()
        .available_tool_bindings_filtered(&Default::default());
    // Only this declared plugin-qualified identifier selects the original candidate's business binding.
    // 仅此已声明插件限定标识选择原候选业务绑定。
    let plugin_tool = bindings
        .iter()
        .find(|binding| {
            binding.origin_tool_id() == format!("{}:{SERVER}/read_page", fixture.plugin_id)
        })
        .expect("actually merged plugin tool");
    assert!(
        plugin_tool
            .invoke(serde_json::json!({"url":"https://plugin.test", "owner_session_id":"fixture"}))
            .await
            .expect("same original plugin business after full commit")["process_id"]
            == plugin_pid
    );
    service_recover_plugin_mcp_secret_mutation(
        ServiceEventEmitter::new(Vec::new()),
        Arc::clone(&fixture.state),
        fixture.plugin_id.clone(),
        requests[0].request_id,
    )
    .await
    .expect("same committed original projection and retirement");
    // Projection and historical control retirement cannot replace or stop the retained actual user process.
    // 投影及历史控制退役不能替换或停止保留实际用户进程。
    let after = user
        .invoke(serde_json::json!({"url":"https://user.test", "owner_session_id":"fixture"}))
        .await
        .expect("same actual user after original control retirement");
    assert_eq!(after["process_id"], before["process_id"]);
    assert_eq!(
        std::fs::read_to_string(marker)
            .expect("actual user startup count")
            .lines()
            .count(),
        1
    );
    assert!(
        fixture
            .state
            .lock()
            .expect("actual projected merged state")
            .agent_runtime
            .as_ref()
            .expect("real bridge")
            .extension_generation
            .mcp_runtime()
            .shares_server_partition(&successor, "user-during-legacy")
    );
}

/// A real client callback panic after projection retains committed ownership and permits the same UUID to deliver a new ordered event.
/// 投影后真实客户端回调 panic 保留已提交所有权，并允许相同 UUID 投递新的有序事件。
#[tokio::test(flavor = "current_thread")]
async fn legacy_secret_mutation_projection_event_panic_retains_committed_request() {
    // This disabled fixture isolates the real OS write and actual workbench event path without native work.
    // 此停用夹具隔离真实操作系统写入及实际工作台事件路径，不启动原生工作。
    let fixture = Fixture::new(false);
    fixture.initial(Some("original-fixture-value"));
    // The actual service generation is the sole event ordering authority.
    // 实际服务代次为唯一事件排序权威。
    let before = fixture
        .state
        .lock()
        .expect("before event delivery")
        .workbench_generation;
    // The first actual service event callback fails after production projection has already committed.
    // 首个实际服务事件回调在生产投影已提交之后失败。
    let events = Arc::new(RecoveryEvents::default());
    events
        .fail_before_delivery
        .store(true, std::sync::atomic::Ordering::Release);
    // The same actual sink is used for the original mutation and explicit recovery RPC.
    // 原变更及明确恢复 RPC 使用相同实际接收端。
    let app = ServiceEventEmitter::new(vec![events.clone()]);
    assert!(
        mutate_plugin_mcp_secret(
            app.clone(),
            Arc::clone(&fixture.state),
            fixture.plugin_id.clone(),
            SERVER.into(),
            SECRET.into(),
            Some("candidate-fixture-value".into()),
        )
        .await
        .expect_err("actual client callback must fail the management task")
        .contains("任务异常结束")
    );
    assert!(fixture.value().as_deref() == Some("candidate-fixture-value"));
    assert_eq!(
        fixture
            .state
            .lock()
            .expect("committed projection before callback return")
            .workbench_generation,
        before + 1
    );
    assert!(
        events
            .events
            .lock()
            .expect("failed first delivery")
            .is_empty()
    );
    // Actual task termination cannot release the same original physical namespace file.
    // 实际任务结束不能释放相同原物理命名空间文件。
    let store = VulcanPluginGenerationStore::open(fixture.runtime.user_plugin_root())
        .expect("delivery failure contender");
    assert!(
        matches!(store.acquire_legacy_mutation_guard(&fixture.plugin_id, None),
        Err(error) if error.code == vulcan_plugin_runtime::VulcanPluginStoreErrorCode::Busy)
    );
    // Public status resolves only the original strong slot's stable UUID.
    // 公共状态仅解析原强槽稳定 UUID。
    let requests = service_plugin_mcp_secret_mutation_requests(
        Arc::clone(&fixture.state),
        fixture.plugin_id.clone(),
    )
    .expect("committed event failure original");
    assert_eq!(requests.len(), 1);
    assert!(requests[0].phase == LegacySecretMutationPhase::Committed);
    // Committed recovery sends the actual current projection without touching the already confirmed OS value.
    // 已提交恢复发送实际当前投影，不触碰已确认操作系统值。
    let access = crate::plugin_mcp_secret_store::legacy_write_observation::Scope::count_access(
        &fixture.plugin_id,
        SERVER,
        SECRET,
    );
    assert!(crate::rpc_dispatch::dispatch_service_rpc(
        app, Arc::clone(&fixture.state), "service_recover_plugin_mcp_secret_mutation".into(),
        Some(serde_json::json!({"pluginId": fixture.plugin_id, "requestId": requests[0].request_id})),
    ).await.expect("same original delivery recovery RPC").is_null());
    assert_eq!(access.access_count(), 0);
    drop(access);
    // Recovery's ordered event and service workbench use the same new revision.
    // 恢复有序事件及服务工作台使用相同新修订。
    let delivered = events.events.lock().expect("recovered client delivery");
    assert_eq!(delivered.len(), 1);
    assert!(delivered.iter().any(|event| matches!(event,
        vulcan_service_api::ServiceEvent::WorkbenchDelta(envelope)
        if envelope.generation == before + 2
            && matches!(&envelope.delta, vulcan_service_api::WorkbenchDelta::PluginCatalogChanged(_)))));
    drop(delivered);
    assert_eq!(
        fixture
            .state
            .lock()
            .expect("actual recovered event generation")
            .workbench_generation,
        before + 2
    );
    assert!(fixture.value().as_deref() == Some("candidate-fixture-value"));
    assert!(
        service_plugin_mcp_secret_mutation_requests(
            Arc::clone(&fixture.state),
            fixture.plugin_id.clone()
        )
        .expect("terminal event delivery original")
        .is_empty()
    );
    store
        .acquire_legacy_mutation_guard(&fixture.plugin_id, None)
        .expect("completed delivery releases the same original fence");
}

/// Disabled mutation succeeds without a candidate process, and an invalid OS write preserves the original value.
/// 停用变更无需候选进程即可成功，无效操作系统写入保留原值。
#[tokio::test]
async fn legacy_secret_mutation_disabled_write_and_delete_preserve_boundary() {
    // The real UUID namespace is cleaned even if any assertion fails.
    // 即使任一断言失败，真实 UUID 命名空间仍会被清理。
    let fixture = Fixture::new(false);
    fixture.initial(Some("original-fixture-value"));
    assert!(fixture.mutate(Some("")).await.is_err());
    assert!(fixture.value().as_deref() == Some("original-fixture-value"));
    fixture
        .mutate(Some("next-fixture-value"))
        .await
        .expect("disabled write");
    assert!(fixture.value().as_deref() == Some("next-fixture-value"));
    fixture.mutate(None).await.expect("disabled delete");
    assert!(fixture.value().is_none());
    assert!(!fixture.ready_entry.exists());
}
