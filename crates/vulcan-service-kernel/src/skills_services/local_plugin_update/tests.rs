//! Real service coordination verifies immutable updates, explicit state preservation and original-request recovery.
//! 真实服务协调验证不可变更新、明确状态保留及原请求恢复。

use super::*;
use crate::tests::provider_configuration_persistence_scope::PROVIDER_CONFIGURATION_PATH;
use vulcan_plugin_runtime::VulcanPluginActivationTransactionPhase as Phase;

/// Recovery waits for the actual configuration writer and rejects a policy change before publishing any routes.
/// 恢复等待实际配置写入器，并在发布任何路由前拒绝策略变化。
#[tokio::test]
async fn immutable_service_update_policy_recovery_serializes_with_user_edits() {
    // Keep the original public catalog independent from the already committed candidate selection.
    // 保持原公开目录独立于已提交候选选择。
    let fixture = Fixture::new(false);
    // Preparation and readiness use the same production path as normal updates.
    // 准备及就绪使用与正常更新相同的生产路径。
    let mut prepared = fixture.prepare(uuid::Uuid::new_v4()).expect("prepare");
    prepared = validate_and_install(&fixture.runtime, prepared)
        .await
        .expect("readiness");
    prepared
        .store
        .commit_activation_transaction(&mut prepared.transaction)
        .expect("selected target");
    // Retain only durable identity after dropping the actual owner.
    // 释放实际所有者后仅保留持久身份。
    let request_id = prepared.transaction.record().request_id();
    // Store and catalog observers do not own the original request lock.
    // 存储及目录观察者不拥有原请求锁。
    let store = prepared.store.clone();
    // Pointer identity detects even a transient replacement by a wrongly enabled candidate catalog.
    // 指针身份可检测被错误启用候选目录的短暂替换。
    let original_catalog = fixture.runtime.snapshot();
    drop(prepared);
    // A real user edit holds the same process-wide authority used by production persistence.
    // 真实用户编辑持有生产持久化使用的相同进程级权威。
    let user_edit = crate::service_state_access::begin_provider_configuration_transaction().await;
    // The recovery task owns only the original service state and request.
    // 恢复任务仅拥有原服务状态及请求。
    let recovery_state = Arc::clone(&fixture.state);
    // Spawn real service recovery while the user writer remains active.
    // 用户写入器仍活动时启动真实服务恢复。
    let recovery = tokio::spawn(async move {
        resume_published(
            &ServiceEventEmitter::new(Vec::new()),
            &recovery_state,
            PLUGIN,
            &request_id.to_string(),
        )
        .await
    });
    // Wait for actual request-lock acquisition, not an assumed task scheduling delay.
    // 等待实际请求锁获取，不假定任务调度延迟。
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            match store.resume_activation_transaction(PLUGIN, request_id) {
                Err(error)
                    if error.code == vulcan_plugin_runtime::VulcanPluginStoreErrorCode::Busy =>
                {
                    break;
                }
                Ok(owner) => drop(owner),
                Err(error) => panic!("unexpected original ownership failure: {error}"),
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("recovery owns original while waiting for user writer");
    assert!(!recovery.is_finished());
    assert!(Arc::ptr_eq(&original_catalog, &fixture.runtime.snapshot()));
    {
        // Commit an explicit new user choice before releasing the configuration writer.
        // 释放配置写入器前提交明确新用户选择。
        let mut state = fixture.state.lock().expect("state");
        // The serialized writer also replaces this exact retained authority.
        // 串行写入器同样替换此精确保留权威。
        let configuration = &mut state
            .agent_runtime
            .as_mut()
            .expect("bridge")
            .provider_configuration;
        configuration
            .luaskills
            .enabled_vulcan_plugins
            .insert(PLUGIN.to_owned());
        configuration
            .luaskills
            .disabled_vulcan_plugins
            .remove(PLUGIN);
    }
    drop(user_edit);
    assert!(
        recovery
            .await
            .expect("recovery task")
            .expect_err("late policy conflict")
            .contains("不一致")
    );
    assert!(Arc::ptr_eq(&original_catalog, &fixture.runtime.snapshot()));
    assert_eq!(
        store
            .activation_request(PLUGIN, request_id)
            .expect("read")
            .expect("original")
            .phase(),
        Phase::Published
    );
}

/// Policy comparison preserves unrelated edits and rejects changed membership even when effective disabled state is unchanged.
/// 策略比较保留无关编辑，即使有效停用状态未变也拒绝已变成员关系。
#[tokio::test]
async fn immutable_service_update_policy_uses_original_plugin_only_compare_and_set() {
    for changed_plugin in [false, true] {
        // Each scenario owns real service configuration and a separate persistence destination.
        // 每个场景拥有真实服务配置及独立持久目标。
        let fixture = Fixture::new(false);
        // The original request must contain the old default, not the candidate's changed enabled baseline.
        // 原请求必须包含旧默认值，而非候选已变化的启用基线。
        let request_id = uuid::Uuid::new_v4();
        // Production preparation fixes this plugin's membership before any candidate callback.
        // 生产准备在任何候选回调前固定此插件的成员关系。
        let prepared = fixture.prepare(request_id).expect("prepared");
        // Read the durable original independently of the foreground wrapper.
        // 独立于前台包装器读取持久原请求。
        let original = prepared
            .store
            .activation_request(PLUGIN, request_id)
            .expect("read")
            .expect("original");
        assert_eq!(
            original.update_policy(),
            Some(&VulcanPluginUpdatePolicy {
                explicitly_enabled: false,
                explicitly_disabled: true,
                predecessor_default_enabled: false,
            })
        );
        {
            // Simulate a completed independent user edit after the original request was frozen.
            // 模拟原请求固定后完成的独立用户编辑。
            let mut state = fixture.state.lock().expect("state");
            // Edit only authoritative retained configuration, as the serialized writer does.
            // 与串行写入器一样，仅编辑权威保留配置。
            let configuration = &mut state
                .agent_runtime
                .as_mut()
                .expect("bridge")
                .provider_configuration;
            configuration
                .luaskills
                .enabled_vulcan_plugins
                .insert("unrelated-enabled".to_owned());
            configuration
                .luaskills
                .disabled_vulcan_plugins
                .insert("unrelated-disabled".to_owned());
            if changed_plugin {
                // Removing the redundant override leaves effective false under the original manifest.
                // 原清单下删除冗余覆盖仍保持有效假值。
                configuration
                    .luaskills
                    .disabled_vulcan_plugins
                    .remove(PLUGIN);
                assert!(
                    !configuration
                        .luaskills
                        .vulcan_plugin_enabled_or_default(PLUGIN, false)
                );
            }
        }
        // Exact before-image verifies conflicts perform no policy write.
        // 精确前像验证冲突不执行策略写入。
        let before = provider_configuration_from_state(&fixture.state).expect("before");
        // The file is absent initially, so a rejected write cannot hide behind old persisted contents.
        // 文件初始不存在，被拒写入不能隐藏在旧持久内容后。
        let destination = fixture.root.path().join("policy-cas.json");
        PROVIDER_CONFIGURATION_PATH
            .scope(destination.clone(), async {
                // Exercise the production serialized writer, not a local test-only policy transformation.
                // 验证生产串行写入器，而非测试专用局部策略变换。
                let result = persist_update_policy(&fixture.state, &prepared).await;
                if changed_plugin {
                    assert!(result.expect_err("policy conflict").contains("已改变"));
                    assert!(!destination.exists());
                } else {
                    result.expect("unrelated edit preserved");
                    assert!(destination.is_file());
                }
                // Compare retained settings including unrelated plugins after the attempted mutation.
                // 尝试变更后比较保留设置，包含无关插件。
                let after = provider_configuration_from_state(&fixture.state).expect("after");
                assert_eq!(
                    after.luaskills.enabled_vulcan_plugins,
                    before.luaskills.enabled_vulcan_plugins
                );
                assert_eq!(
                    after.luaskills.disabled_vulcan_plugins,
                    before.luaskills.disabled_vulcan_plugins
                );
                assert_eq!(
                    prepared
                        .store
                        .activation_request(PLUGIN, request_id)
                        .expect("retained"),
                    Some(original)
                );
                assert!(
                    prepared
                        .store
                        .activation_effects(PLUGIN, request_id)
                        .expect("effects")
                        .is_none()
                );
                assert!(
                    prepared
                        .store
                        .installation_receipt(PLUGIN, request_id)
                        .expect("install")
                        .is_none()
                );
            })
            .await;
    }
}

/// Preparation rejects a policy/catalog mismatch before publishing durable intent or allocating candidate resources.
/// 准备在发布持久意图或分配候选资源前拒绝策略与目录不符。
#[test]
fn immutable_service_update_policy_mismatch_precedes_durable_preparation() {
    // Catalog remains disabled while a direct later settings edit requests enabled.
    // 目录保持停用，而直接后续设置编辑要求启用。
    let fixture = Fixture::new(false);
    {
        // Update only this isolated retained service authority.
        // 仅更新此隔离的保留服务权威。
        let mut state = fixture.state.lock().expect("state");
        state
            .agent_runtime
            .as_mut()
            .expect("bridge")
            .provider_configuration
            .luaskills
            .enabled_vulcan_plugins
            .insert(PLUGIN.to_owned());
    }
    // No journal may be allocated for this incoherent input snapshot.
    // 不得为此不一致输入快照分配日志。
    let request_id = uuid::Uuid::new_v4();
    assert!(
        fixture
            .prepare(request_id)
            .err()
            .expect("mismatch")
            .contains("不一致")
    );
    assert!(list(&fixture.runtime, PLUGIN).expect("query").is_empty());
}

/// Frozen policy resolution stays identical to the authoritative provider contract for every membership combination.
/// 对于每种成员关系组合，固定策略解析均保持与权威供应商契约一致。
#[test]
fn immutable_service_update_policy_resolution_matches_provider_contract() {
    // No runtime or persistence is needed to exercise the actual configuration resolver.
    // 验证实际配置解析器无需运行时或持久化。
    let mut configuration =
        vulcan_provider_config::config::PersistedLuaSkillsConfiguration::default();
    for enabled in [false, true] {
        for disabled in [false, true] {
            for default_enabled in [false, true] {
                configuration.enabled_vulcan_plugins.clear();
                configuration.disabled_vulcan_plugins.clear();
                if enabled {
                    configuration
                        .enabled_vulcan_plugins
                        .insert(PLUGIN.to_owned());
                }
                if disabled {
                    configuration
                        .disabled_vulcan_plugins
                        .insert(PLUGIN.to_owned());
                }
                // Preserve both original memberships instead of prematurely normalizing away evidence.
                // 保留两个原成员关系，而非提前归一化并丢弃证据。
                let policy = VulcanPluginUpdatePolicy {
                    explicitly_enabled: enabled,
                    explicitly_disabled: disabled,
                    predecessor_default_enabled: default_enabled,
                };
                assert_eq!(
                    policy.desired_enabled(),
                    configuration.vulcan_plugin_enabled_or_default(PLUGIN, default_enabled)
                );
            }
        }
    }
}

/// Real candidate initialization is projected through service queries and requires explicit abandonment acknowledgement.
/// 真实候选初始化经服务查询投影，并要求明确放弃确认。
#[tokio::test]
async fn immutable_service_update_effects_are_queryable_without_replay() {
    use luaskills::{LuaEngine, LuaEngineOptions, LuaRuntimeHostOptions, LuaVmPoolConfig};
    for fail in [false, true] {
        // The original service fixture isolates policy, storage and all candidate bytes.
        // 原服务夹具隔离策略、存储及全部候选字节。
        let fixture = Fixture::new(false);
        // Attach a real engine to the same service-owned manager; content-only tests keep their original no-engine fixture.
        // 为同一服务自有管理器附加真实引擎；纯内容测试保持原无引擎夹具。
        let native_root = vulcan_runtime_assets::native_host_path(
            &fixture
                .root
                .path()
                .canonicalize()
                .expect("canonical fixture root"),
        );
        // Match the established native integration fixture's bounded compatibility engine.
        // 匹配既有原生集成夹具的有界兼容引擎。
        let engine = LuaEngine::new(LuaEngineOptions::new(
            LuaVmPoolConfig {
                min_size: 1,
                max_size: 1,
                idle_ttl_secs: 60,
            },
            LuaRuntimeHostOptions::with_runtime_root(native_root),
        ))
        .expect("real LuaSkills engine");
        fixture
            .runtime
            .attach_runtime_engine(Arc::new(engine))
            .expect("attach original manager");
        std::fs::create_dir_all(fixture.root.path().join("bridge/runtime"))
            .expect("bridge directory");
        std::fs::write(fixture.root.path().join("bridge/dependencies.yaml"), "{}\n")
            .expect("bridge dependencies");
        std::fs::write(
            fixture.root.path().join("bridge/runtime/bridge.lua"),
            include_str!("../../../../../plugin-bridge/vulcan-plugin-bridge/runtime/bridge.lua"),
        )
        .expect("authoritative bridge module");
        std::fs::create_dir_all(fixture.root.path().join("distributions"))
            .expect("distribution root");
        std::fs::create_dir_all(fixture.source.join("runtime")).expect("runtime directory");
        std::fs::write(fixture.source.join("dependencies.yaml"), "{}\n")
            .expect("bundled dependencies");
        // Append the exact production Lua declaration before immutable package inspection.
        // 不可变插件包检查前追加精确生产 Lua 声明。
        let mut manifest =
            std::fs::read_to_string(fixture.source.join("plugin.yaml")).expect("manifest");
        manifest.push_str("execution:\n  dependencies:\n    manifest: dependencies.yaml\n    install_mode: bundled\nlua_runtime:\n  bundle: runtime/plugin.bundle.lua\n  invoke_function: dispatch\n  shutdown_function: shutdown\n  max_active_sessions: 1\n");
        std::fs::write(fixture.source.join("plugin.yaml"), manifest).expect("Lua declaration");
        std::fs::write(
            fixture.source.join("runtime/plugin.bundle.lua"),
            format!(
                r#"
-- Module initialization can fail after entering real Lua; the receipt must retain that attempt.
-- 模块初始化可能在进入真实 Lua 后失败；回执必须保留该尝试。
if {fail} then error('private-callback-marker') end
-- Exports belong to this module instance, without external service dependencies.
-- 导出属于此模块实例，不依赖外部服务。
local M = {{}}
-- No arguments are consumed; return the declared version-one success frame.
-- 不消费参数；返回声明的版本一成功帧。
function M.dispatch() return '{{"ok":true,"result":{{}}}}' end
-- Shutdown uses the same empty success frame and has no external cleanup to perform.
-- 关闭使用相同空成功帧，且无需执行外部清理。
M.shutdown = M.dispatch
return M
"#
            ),
        )
        .expect("actual Lua bundle");
        fixture.runtime.initialize_formal_runtime(vulcan_plugin_runtime::VulcanPluginRuntimeSettings::default())
            .expect("real formal parent");
        // The service owns one original request before the actual module executes.
        // 实际模块执行之前，服务拥有单个原请求。
        let request = uuid::Uuid::new_v4();
        // Preparation uses the production approval and credential staging path.
        // 准备使用生产批准及凭据暂存路径。
        let prepared = fixture.prepare(request).expect("prepared Lua candidate");
        {
            // No service response may invent a second candidate identity.
            // 服务响应不得虚构第二个候选身份。
            let candidate = prepared
                .store
                .inspect_activation_candidate(&prepared.transaction)
                .expect("candidate");
            // Preserve native diagnostics if the success fixture fails before its intended result.
            // 成功夹具在预期结果前失败时保留原生诊断。
            let result = fixture.runtime.probe_activation_candidate(&candidate).await;
            if fail {
                result.expect_err("intentional module initialization failure");
            } else {
                result.expect("actual successful candidate initialization");
            }
        }
        drop(prepared);
        // Queries return only operation/phase; callback diagnostics are intentionally excluded.
        // 查询仅返回操作／阶段；有意排除回调诊断。
        let records = list(&fixture.runtime, PLUGIN).expect("durable query");
        // Stable request identity locates the record independently of ordering.
        // 稳定请求身份定位记录，独立于排序。
        let record = records
            .iter()
            .find(|record| record.request_id == request.to_string())
            .expect("original request");
        assert!(record.installation.is_none());
        assert_eq!(record.effects.len(), 1);
        assert_eq!(
            record.effects[0].operation,
            vulcan_plugin_runtime::VulcanPluginActivationEffect::Probe
        );
        assert_eq!(
            record.effects[0].phase,
            if fail {
                vulcan_plugin_runtime::VulcanPluginEffectObservation::ObservationFailed
            } else {
                vulcan_plugin_runtime::VulcanPluginEffectObservation::Succeeded
            }
        );
        assert!(
            !serde_json::to_string(record)
                .expect("sanitized output")
                .contains("private-callback-marker")
        );
        assert!(
            abort(&fixture.runtime, PLUGIN, &request.to_string(), false)
                .expect_err("acknowledgement is required")
                .contains("副作用")
        );
        // Acknowledged abandonment retains historical evidence instead of claiming external rollback.
        // 已确认放弃保留历史证据，而非声称外部回滚。
        let abandoned = abort(&fixture.runtime, PLUGIN, &request.to_string(), true)
            .expect("explicit abandonment");
        assert_eq!(abandoned.phase, Phase::Aborted);
        assert_eq!(abandoned.effects.len(), 1);
    }
}

/// Stable package identity shared by the fixture's old and new immutable versions.
/// 夹具旧新不可变版本共用的稳定插件包身份。
pub(in crate::skills_services) const PLUGIN: &str = "update-fixture";

/// Real lifecycle transitions reopen owner admission only after the original drain.
/// 真实生命周期转换仅在原排空后重新开放所有者入场。
/// The content-only package uses real service persistence while avoiding network and credentials.
/// 纯内容插件包使用真实服务持久化，同时不需要网络或凭据。
#[tokio::test]
async fn immutable_service_update_owner_drain_survives_enable_disable_cycles() {
    // The existing fixture supplies the original runtime and isolated policy destination.
    // 既有夹具提供原运行时及隔离策略目标。
    let fixture = Fixture::new(false);
    // Every lifecycle write stays inside this task-local test directory.
    // 每次生命周期写入均留在此任务局部测试目录内。
    let destination = fixture.root.path().join("owner-drain-policy.json");
    PROVIDER_CONFIGURATION_PATH
        .scope(destination, async {
            for enabled in [true, false, true, false] {
                vulcan_plugin_lifecycle::set_vulcan_plugin_enabled(
                    Arc::clone(&fixture.state),
                    PLUGIN.to_owned(),
                    enabled,
                )
                .await
                .expect("real enable/disable transition");
                // Capture the actual published access after each transition.
                // 在每次转换后捕获实际发布的访问。
                let mcp = fixture
                    .state
                    .lock()
                    .expect("service state")
                    .agent_runtime
                    .as_ref()
                    .expect("actual bridge")
                    .extension_generation
                    .mcp_runtime();
                // An empty owned partition exercises admission without launching unrelated processes.
                // 空所有者分区验证入场，不启动无关进程。
                let admitted = mcp
                    .open_owned_stdio_partition(
                        PLUGIN,
                        fixture.runtime.snapshot().plugins[PLUGIN].generation,
                        SessionId::new_v4(),
                        Vec::new(),
                    )
                    .await;
                if enabled {
                    admitted
                        .expect("enabled owner admission")
                        .shutdown()
                        .expect("close admitted owner");
                } else {
                    assert!(
                        admitted
                            .expect_err("disabled owner admission")
                            .contains("draining")
                    );
                    mcp.wait_plugin_owner_drain(
                        PLUGIN,
                        AgentRuntimeBridge::RETIRED_MCP_PARTITION_DRAIN_TIMEOUT,
                    )
                    .await
                    .expect("service observed original cleanup");
                }
            }
        })
        .await;
}

/// Owns all mutable package and configuration paths used by the real service coordinator.
/// 拥有真实服务协调器使用的全部可变插件包及配置路径。
pub(in crate::skills_services) struct Fixture {
    /// Temporary storage outlives every manager and retained generation used in each test.
    /// 临时存储比每项测试中的所有管理器及保留代次存活更久。
    root: tempfile::TempDir,
    /// The sole original manager is also installed into the service bridge.
    /// 唯一原管理器同时安装到服务桥中。
    pub(in crate::skills_services) runtime: VulcanPluginRuntimeManager,
    /// Actual service state owns publication and user-policy projection.
    /// 实际服务状态拥有发布及用户策略投影。
    pub(in crate::skills_services) state: SharedServiceBridgeState,
    /// Mutable input is distinct from the installed predecessor and prepared immutable copy.
    /// 可变输入与已安装前驱及已准备不可变副本分离。
    source: PathBuf,
}

/// Write one valid package under root using version and explicit defaults; return its exact directory.
/// 在 root 下使用 version 及明确默认值写入有效插件包；返回其精确目录。
fn write_package(
    root: &Path,
    version: &str,
    enabled: bool,
    configuration: Option<serde_json::Value>,
) -> PathBuf {
    // Directory identity matches the manifest's exact plugin name.
    // 目录身份匹配清单的精确插件名称。
    let package = root.join(PLUGIN);
    std::fs::create_dir_all(package.join("skills/review-helper")).expect("skill directory");
    std::fs::write(
        package.join("skills/review-helper/SKILL.md"),
        format!(
            "---\nname: review-helper\ndescription: Review a change.\n---\n\nVersion {version}\n"
        ),
    )
    .expect("skill");
    // Configuration declaration is explicit; no undeclared state is synthesized.
    // 配置声明显式；不合成未声明状态。
    let config_manifest = if let Some(value) = configuration {
        std::fs::write(package.join("config.schema.json"),
            r#"{"type":"object","properties":{"mode":{"type":"string"}},"required":["mode"],"additionalProperties":false}"#)
            .expect("schema");
        std::fs::write(
            package.join("config.defaults.json"),
            serde_json::to_vec(&value).expect("defaults JSON"),
        )
        .expect("defaults");
        "permissions: [config]\nconfig:\n  schema_file: config.schema.json\n  defaults_file: config.defaults.json\n"
    } else {
        ""
    };
    std::fs::write(package.join("plugin.yaml"), format!(
        "schema_version: 1\nid: {PLUGIN}\nversion: {version}\ndefault_enabled: {enabled}\ndisplay:\n  name: Update Fixture\n  capability: Read-only guidance\n{config_manifest}"
    )).expect("manifest");
    package
}

impl Fixture {
    /// Build a real legacy predecessor and a distinct candidate; configuration selects declared document coverage.
    /// 构建真实旧格式前驱及不同候选；configuration 选择声明文档覆盖。
    /// Returns isolated service state with an explicitly disabled plugin and no external network connections.
    /// 返回隔离服务状态，含明确停用插件且无外部网络连接。
    pub(in crate::skills_services) fn new(configuration: bool) -> Self {
        // All mutable files belong exclusively to this temporary workspace.
        // 全部可变文件仅属于此临时工作区。
        let root = tempfile::tempdir().expect("update workspace");
        // The manager uses only fixture paths and does not load a Lua engine for content-only packages.
        // 管理器仅使用夹具路径，且不为纯内容插件包加载 Lua 引擎。
        let runtime = VulcanPluginRuntimeManager::without_runtime(
            root.path().join("bundled"),
            root.path().join("user"),
            root.path().join("bridge"),
            root.path().join("distributions"),
            root.path().join("state"),
            root.path().join("runtime"),
        );
        // Existing source is installed through production authorization and package copying.
        // 既有来源经生产授权及插件包复制安装。
        let previous = write_package(
            &root.path().join("old-source"),
            "1.0.0",
            false,
            configuration.then(|| json!({"mode":"old-default"})),
        );
        // Preview is authoritative for both fingerprint and complete permission approval.
        // 预览对指纹及完整权限批准均具有权威。
        let preview = runtime
            .inspect_local_user_plugin(&previous)
            .expect("old preview");
        runtime
            .install_local_user_plugin(
                &previous,
                &preview.content_fingerprint,
                preview.permissions,
                false,
            )
            .expect("old installation");
        runtime
            .refresh_catalog(&BTreeSet::from([PLUGIN.to_owned()]), &BTreeSet::new())
            .expect("old catalog");
        // The candidate intentionally changes its default to enabled; update must still preserve the user's disabled state.
        // 候选故意将默认值改为启用；更新仍必须保留用户停用状态。
        let source = write_package(
            &root.path().join("new-source"),
            "2.0.0",
            true,
            configuration.then(|| json!({"mode":"new-default"})),
        );
        // Reuse ordinary service dependencies while replacing only this task-owned runtime.
        // 复用普通服务依赖，仅替换此任务拥有的运行时。
        let mut bridge = crate::tests::test_support::test_agent_runtime_bridge();
        bridge.vulcan_plugin_runtime = runtime.clone();
        bridge
            .provider_configuration
            .luaskills
            .disabled_vulcan_plugins
            .insert(PLUGIN.to_owned());
        bridge.extension_generation = bridge
            .prepare_plugin_extension_generation(
                runtime.snapshot(),
                bridge.extension_generation.mcp_runtime(),
            )
            .expect("initial routes");
        // Real bridge state receives the exact manager and captured predecessor.
        // 真实桥状态接收精确管理器及捕获前驱。
        let mut state =
            crate::tests::test_support::test_service_state_with_parent_session(SessionId::new_v4());
        state.agent_runtime = Some(bridge);
        Self {
            root,
            runtime,
            state: Arc::new(Mutex::new(state)),
            source,
        }
    }

    /// Prepare one explicitly identified request through the production preparation function and return its owner.
    /// 经生产准备函数准备单个明确标识请求，并返回其所有者。
    fn prepare(&self, request_id: uuid::Uuid) -> Result<PreparedLocalPluginUpdate, String> {
        // Input approval comes from the actual candidate rather than a hard-coded permission list.
        // 输入批准来自实际候选，而非写死的权限列表。
        let preview = self
            .runtime
            .inspect_local_user_plugin(&self.source)
            .expect("candidate preview");
        prepare_update(
            &self.runtime,
            &provider_configuration_from_state(&self.state)?.luaskills,
            &self.source,
            &preview.content_fingerprint,
            preview.permissions,
            false,
            Vec::new(),
            request_id,
        )
    }
}

/// The actual coordinator commits immutable bytes, preserves disabled policy and returns a completed durable request.
/// 实际协调器提交不可变字节、保留停用策略，并返回已完成持久请求。
#[tokio::test]
async fn immutable_service_update_publishes_without_replacing_old_bytes() {
    // This package needs no network, secure-store entries or Lua execution.
    // 此插件包不需要网络、安全存储条目或 Lua 执行。
    let fixture = Fixture::new(false);
    // The real old descriptor remains retained through the complete service update.
    // 在完整服务更新期间保持保留真实旧描述符。
    let old = fixture.runtime.snapshot().plugins[PLUGIN].clone();
    // Exact old bytes must remain present at their original path after successful publication.
    // 成功发布后，精确旧字节必须仍位于原路径。
    let old_manifest = std::fs::read(old.package_root.join("plugin.yaml")).expect("old manifest");
    // All configuration writes are explicitly scoped inside the coordinator's executing task.
    // 全部配置写入在协调器执行任务内明确设置作用域。
    let destination = fixture.root.path().join("provider.json");
    PROVIDER_CONFIGURATION_PATH
        .scope(destination.clone(), async {
            // This is the production coordinator used directly by the service RPC wrapper.
            // 此处为服务 RPC 包装器直接使用的生产协调器。
            let preview = fixture
                .runtime
                .inspect_local_user_plugin(&fixture.source)
                .expect("new preview");
            update(
                &ServiceEventEmitter::new(Vec::new()),
                &fixture.state,
                fixture.source.to_string_lossy().into_owned(),
                preview.content_fingerprint.clone(),
                preview.permissions,
                false,
                Vec::new(),
            )
            .await
            .expect("real immutable update");
            // Published catalog must identify the frozen managed target, not the mutable source or predecessor.
            // 已发布目录必须标识固定受管目标，而不是可变来源或前驱。
            let current = fixture.runtime.snapshot().plugins[PLUGIN].clone();
            assert_eq!(current.content_fingerprint, preview.content_fingerprint);
            assert_eq!(current.runtime_state, VulcanPluginRuntimeState::Disabled);
            assert_ne!(current.package_root, old.package_root);
            assert_ne!(current.package_root, fixture.source);
            assert!(current.managed_package().is_some());
            assert_eq!(
                std::fs::read(old.package_root.join("plugin.yaml")).expect("original old bytes"),
                old_manifest
            );
            // Query production status independently from foreground success.
            // 独立于前台成功查询生产状态。
            let records =
                service_local_plugin_update_requests(Arc::clone(&fixture.state), PLUGIN.to_owned())
                    .await
                    .expect("RPC status");
            assert_eq!(records.len(), 1);
            assert_eq!(records[0].phase, Phase::Completed);
            assert_eq!(
                records[0].candidate.content_fingerprint,
                current.content_fingerprint
            );
            assert!(records[0].installation.is_none());
            // Real persisted policy prevents a new manifest default from silently enabling the updated plugin after restart.
            // 真实持久策略防止新清单默认值在重启后静默启用更新插件。
            let persisted = std::fs::read_to_string(&destination).expect("scoped user policy");
            assert!(persisted.contains(PLUGIN));
            assert!(
                provider_configuration_from_state(&fixture.state)
                    .expect("retained policy")
                    .luaskills
                    .disabled_vulcan_plugins
                    .contains(PLUGIN)
            );
        })
        .await;
}

/// Preparation keeps original public state and rejects duplicate pending updates before allocating another candidate.
/// 准备保持原公开状态，并在分配另一候选之前拒绝重复待完成更新。
#[tokio::test]
async fn immutable_service_update_preparation_is_private_and_explicitly_abandonable() {
    // Pure content isolates durable preparation from callback side effects.
    // 纯内容将持久准备与回调副作用分离。
    let fixture = Fixture::new(false);
    // Retain the exact public pointer, not merely a copied plugin identity.
    // 保留精确公开指针，而非仅复制插件身份。
    let original = fixture.runtime.snapshot();
    // Every retry must refer to this same initial durable ID.
    // 每次重试必须引用此相同初始持久标识。
    let request_id = uuid::Uuid::new_v4();
    // Real preparation owns actual package and credential locks.
    // 真实准备拥有实际插件包及凭据锁。
    let prepared = fixture.prepare(request_id).expect("immutable preparation");
    assert!(Arc::ptr_eq(&original, &fixture.runtime.snapshot()));
    assert!(
        prepared
            .store
            .activation(PLUGIN)
            .expect("unchanged legacy selection")
            .is_none()
    );
    assert!(
        fixture
            .prepare(uuid::Uuid::new_v4())
            .err()
            .expect("pending update")
            .contains(&request_id.to_string())
    );
    assert!(abort(&fixture.runtime, PLUGIN, &request_id.to_string(), false).is_err());
    // Read-only queries remain available while the original exclusive owner is alive.
    // 原排他所有者存活期间，只读查询仍可用。
    let records = list(&fixture.runtime, PLUGIN).expect("read original request");
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].phase, Phase::Prepared);
    drop(prepared);
    assert_eq!(
        abort(&fixture.runtime, PLUGIN, &request_id.to_string(), false)
            .expect("explicit abort")
            .phase,
        Phase::Aborted
    );
    assert!(Arc::ptr_eq(&original, &fixture.runtime.snapshot()));
}

/// Existing configuration wins over new defaults while incompatible candidate schemas fail before activation.
/// 既有配置优先于新默认值，而不兼容候选模式在激活之前失败。
#[tokio::test]
async fn immutable_service_update_preserves_configuration_without_schema_fallback() {
    // Both versions explicitly declare a complete configuration document.
    // 两个版本均明确声明完整配置文档。
    let fixture = Fixture::new(true);
    fixture
        .runtime
        .update_plugin_config(PLUGIN, json!({"mode":"user-value"}))
        .expect("legacy user configuration");
    // The actual candidate selection must preserve the exact old user document.
    // 实际候选选择必须保留精确旧用户文档。
    let prepared = fixture
        .prepare(uuid::Uuid::new_v4())
        .expect("valid configuration update");
    assert_eq!(
        prepared
            .transaction
            .record()
            .target()
            .configuration()
            .expect("target config")
            .value(),
        Some(&json!({"mode":"user-value"}))
    );
    // Explicitly discard the first intent before inspecting a different candidate.
    // 检查不同候选之前明确放弃第一个意图。
    let request_id = prepared.transaction.record().request_id();
    drop(prepared);
    abort(&fixture.runtime, PLUGIN, &request_id.to_string(), false)
        .expect("abandon unused preparation");
    std::fs::write(fixture.source.join("config.schema.json"),
        r#"{"type":"object","properties":{"mode":{"type":"integer"}},"required":["mode"],"additionalProperties":false}"#)
        .expect("new incompatible schema");
    std::fs::write(fixture.source.join("config.defaults.json"), r#"{"mode":7}"#)
        .expect("valid new defaults");
    // Preserve the original failing UUID so readback never selects a newer request.
    // 保留原失败 UUID，使回读绝不选择较新请求。
    let failed_request = uuid::Uuid::new_v4();
    assert!(
        fixture
            .prepare(failed_request)
            .err()
            .expect("no default fallback")
            .contains("schema")
    );
    assert_eq!(
        fixture
            .runtime
            .plugin_config(PLUGIN)
            .expect("preserved old document"),
        json!({"mode":"user-value"})
    );
    // The failed configuration validation must leave the already-copied candidate owned by early intent.
    // 配置校验失败必须让已复制候选继续归属于早期意图。
    let records = list(&fixture.runtime, PLUGIN).expect("query original preparation");
    // Locate evidence by its stable request identity, never by enumeration position.
    // 按稳定请求身份定位证据，绝不依赖枚举位置。
    let original = records
        .iter()
        .find(|record| record.request_id == failed_request.to_string())
        .expect("failed original must remain queryable");
    assert_eq!(original.phase, Phase::Preparing);
    assert!(original.installation.is_none());
    assert_eq!(original.candidate.version, "2.0.0");
    assert!(fixture.prepare(uuid::Uuid::new_v4()).is_err());
    assert!(fixture.runtime.begin_enabling(PLUGIN).is_err());
    assert!(abort(&fixture.runtime, PLUGIN, &failed_request.to_string(), false).is_err());
    // Explicit abandonment preserves the old selection and reports a terminal receipt on the same UUID.
    // 明确放弃保留旧选择，并在相同 UUID 上报告终态回执。
    let abandoned = abort(&fixture.runtime, PLUGIN, &failed_request.to_string(), true)
        .expect("explicitly abandon early original");
    assert_eq!(abandoned.request_id, failed_request.to_string());
    assert_eq!(abandoned.phase, Phase::Aborted);
    assert_eq!(abandoned.candidate, original.candidate);
    assert_eq!(
        fixture
            .runtime
            .plugin_config(PLUGIN)
            .expect("old configuration after abandonment"),
        json!({"mode":"user-value"})
    );
    fixture
        .runtime
        .begin_enabling(PLUGIN)
        .expect("terminal original unblocks ordinary lifecycle admission");
}

/// Already-published recovery only republishes exact routes and confirms the original request without replay.
/// 已发布恢复仅重新发布精确路由并确认原请求，不重放。
#[tokio::test]
async fn immutable_service_update_recovers_original_published_selection() {
    for restart in [false, true] {
        // Real staged content requires no artificial callback completion receipt.
        // 真实暂存内容不需要人工回调完成回执。
        let mut fixture = Fixture::new(false);
        // Prepare and validate through the same production steps used by a normal update.
        // 经正常更新使用的相同生产步骤准备并校验。
        let mut prepared = fixture.prepare(uuid::Uuid::new_v4()).expect("prepare");
        prepared = validate_and_install(&fixture.runtime, prepared)
            .await
            .expect("actual readiness");
        prepared
            .store
            .commit_activation_transaction(&mut prepared.transaction)
            .expect("publish selection");
        // These original identifiers survive loss of the foreground service operation.
        // 这些原身份在前台服务操作丢失后仍存续。
        let request_id = prepared.transaction.record().request_id();
        let target = prepared.transaction.record().target().clone();
        let store = prepared.store.clone();
        drop(prepared);
        if restart {
            // Recreate the production manager against the same disk, preserving the persisted disabled policy.
            // 对相同磁盘重新创建生产管理器，保留持久停用策略。
            let restarted = VulcanPluginRuntimeManager::without_runtime(
                fixture.root.path().join("bundled"),
                fixture.root.path().join("user"),
                fixture.root.path().join("bridge"),
                fixture.root.path().join("distributions"),
                fixture.root.path().join("state"),
                fixture.root.path().join("runtime"),
            );
            restarted
                .initialize_activation_recovery()
                .expect("startup recovery");
            // The new startup catalog must withhold this target until the original request is resumed.
            // 新启动目录必须隔离此目标，直到原请求被恢复。
            let catalog = restarted
                .refresh_catalog(&BTreeSet::from([PLUGIN.to_owned()]), &BTreeSet::new())
                .expect("restart catalog");
            assert_eq!(
                catalog.plugins[PLUGIN].runtime_state,
                VulcanPluginRuntimeState::Unavailable
            );
            // Replace the actual service-owned manager and extension generation together for this simulated restart.
            // 为此次模拟重启一起替换实际服务自有管理器及扩展代次。
            let mut state = fixture.state.lock().expect("service state");
            // Use the exact bridge already owned by this service fixture.
            // 使用此服务夹具已持有的精确桥。
            let bridge = state.agent_runtime.as_mut().expect("service bridge");
            bridge.vulcan_plugin_runtime = restarted.clone();
            bridge.extension_generation = bridge
                .prepare_plugin_extension_generation(
                    catalog,
                    bridge.extension_generation.mcp_runtime(),
                )
                .expect("restart routes");
            fixture.runtime = restarted;
        }
        resume_published(
            &ServiceEventEmitter::new(Vec::new()),
            &fixture.state,
            PLUGIN,
            &request_id.to_string(),
        )
        .await
        .expect("recover original published request");
        assert_eq!(
            store.activation(PLUGIN).expect("original activation"),
            Some(target)
        );
        assert_eq!(
            list(&fixture.runtime, PLUGIN).expect("original completed request")[0].phase,
            Phase::Completed
        );
        assert!(abort(&fixture.runtime, PLUGIN, &request_id.to_string(), true).is_err());
    }
}

/// Newly declared configuration uses inspected defaults, while removal retains the old user file without selecting it.
/// 首次声明配置使用已检查默认值，而移除声明保留旧用户文件但不再选择它。
/// No inputs or return; both branches use real packages and production transaction preparation.
/// 无输入与返回值；两条分支均使用真实插件包及生产事务准备。
#[tokio::test]
async fn immutable_service_update_handles_explicit_configuration_declaration_changes() {
    for previously_declared in [false, true] {
        // The predecessor's declaration, not file presence, decides configuration authority.
        // 前驱声明而非文件存在决定配置权威。
        let fixture = Fixture::new(previously_declared);
        if previously_declared {
            fixture
                .runtime
                .update_plugin_config(PLUGIN, json!({"mode":"retained-user"}))
                .expect("old user configuration");
        }
        write_package(
            &fixture.root.path().join("new-source"),
            "2.0.0",
            true,
            (!previously_declared).then(|| json!({"mode":"declared-default"})),
        );
        // Preparation retains the complete declaration transition in the original target.
        // 准备在原目标中保留完整声明转换。
        let prepared = fixture
            .prepare(uuid::Uuid::new_v4())
            .expect("changed declaration");
        // Keep the expected complete document alive through the borrowed comparison.
        // 在借用比较期间保持预期完整文档存活。
        let expected = (!previously_declared).then(|| json!({"mode":"declared-default"}));
        assert_eq!(
            prepared
                .transaction
                .record()
                .target()
                .configuration()
                .expect("explicit selection")
                .value(),
            expected.as_ref()
        );
        if previously_declared {
            assert_eq!(
                fixture
                    .runtime
                    .plugin_config(PLUGIN)
                    .expect("old document survives"),
                json!({"mode":"retained-user"})
            );
        }
    }
}

/// A same-revision configuration change cannot replace the frozen predecessor or leak its document through status.
/// 相同修订的配置变更不能替代固定前驱，也不能通过状态泄露其文档。
/// No inputs or return; modifies only the fixture's real activation record and preserves its revision.
/// 无输入与返回值；仅修改夹具真实激活记录，并保持其修订。
#[tokio::test]
async fn immutable_service_update_rejects_changed_predecessor_authority() {
    // A managed predecessor is needed to distinguish complete authority from revision equality.
    // 需要受管前驱，以区分完整权威与修订相等。
    let fixture = Fixture::new(true);
    // First publication creates an actual format-four selected configuration.
    // 首次发布创建实际第四版选定配置。
    let mut prepared = fixture
        .prepare(uuid::Uuid::new_v4())
        .expect("first preparation");
    prepared
        .store
        .commit_activation_transaction(&mut prepared.transaction)
        .expect("first selection");
    prepared
        .store
        .complete_activation_transaction(&mut prepared.transaction)
        .expect("first completion");
    fixture
        .runtime
        .refresh_catalog(&BTreeSet::from([PLUGIN.to_owned()]), &BTreeSet::new())
        .expect("frozen managed predecessor");
    // The service exposes the exact wire contract without selected configuration contents.
    // 服务暴露精确线上契约，不包含选定配置内容。
    let projected = serde_json::to_value(list(&fixture.runtime, PLUGIN).expect("sanitized status"))
        .expect("wire status");
    assert_eq!(projected[0]["phase"], "completed");
    assert_eq!(projected[0]["candidate"]["plugin_id"], PLUGIN);
    assert!(projected[0]["activationRevision"].is_string());
    assert!(!projected.to_string().contains("old-default"));
    assert!(projected[0].get("configuration").is_none());
    // Mutate one authority under the same revision to reproduce stale-catalog input.
    // 在相同修订下改变一项权威，以复现过期目录输入。
    let activation_path = fixture
        .runtime
        .user_plugin_root()
        .join(format!(".vulcan-generations/active/{PLUGIN}.json"));
    // The on-disk schema is the actual activation schema, not a guessed transport shape.
    // 磁盘模式是实际激活模式，而非猜测的传输形状。
    let mut activation: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&activation_path).expect("activation bytes"))
            .expect("activation JSON");
    activation["configuration"]["value"]["mode"] = json!("changed-with-same-revision");
    std::fs::write(
        &activation_path,
        serde_json::to_vec(&activation).expect("changed bytes"),
    )
    .expect("changed authority");
    write_package(
        &fixture.root.path().join("new-source"),
        "3.0.0",
        true,
        Some(json!({"mode":"third-default"})),
    );
    assert!(
        fixture
            .prepare(uuid::Uuid::new_v4())
            .err()
            .expect("full mismatch rejected")
            .contains("持久选择与当前目录不一致")
    );
    assert_eq!(
        list(&fixture.runtime, PLUGIN).expect("no new intent").len(),
        1
    );
}

/// Recovery rejects unpublished requests and changed user policy while preserving the original exact phase.
/// 恢复拒绝未发布请求及已改变用户策略，同时保留原精确阶段。
/// No inputs or return; every attempt uses the original real record and does not invoke candidate callbacks.
/// 无输入与返回值；每次尝试使用原真实记录，不调用候选回调。
#[tokio::test]
async fn immutable_service_update_recovery_does_not_infer_commit_or_policy() {
    // A content-only request makes any phase transition directly observable.
    // 纯内容请求使任何阶段转换都可直接观测。
    let fixture = Fixture::new(false);
    // Keep only stable identity after dropping the original foreground owner.
    // 销毁原前台所有者后仅保留稳定身份。
    let request_id = uuid::Uuid::new_v4();
    drop(
        fixture
            .prepare(request_id)
            .expect("original prepared request"),
    );
    assert!(
        resume_published(
            &ServiceEventEmitter::new(Vec::new()),
            &fixture.state,
            PLUGIN,
            &request_id.to_string()
        )
        .await
        .expect_err("not committed")
        .contains("尚无确定的已提交目标")
    );
    assert_eq!(
        list(&fixture.runtime, PLUGIN).expect("unchanged request")[0].phase,
        Phase::Prepared
    );
    // The actual store performs publication, independently from any recovery inference.
    // 实际存储执行发布，独立于任何恢复推断。
    let store = VulcanPluginGenerationStore::open_existing(fixture.runtime.user_plugin_root())
        .expect("existing store")
        .expect("created store");
    // Reacquire only the original request, never a replacement ID.
    // 仅重新获取原请求，绝不使用替代标识。
    let mut transaction = store
        .resume_activation_transaction(PLUGIN, request_id)
        .expect("same owner");
    store
        .commit_activation_transaction(&mut transaction)
        .expect("actual publication");
    drop(transaction);
    fixture
        .state
        .lock()
        .expect("state")
        .agent_runtime
        .as_mut()
        .expect("bridge")
        .provider_configuration
        .luaskills
        .disabled_vulcan_plugins
        .remove(PLUGIN);
    assert!(
        resume_published(
            &ServiceEventEmitter::new(Vec::new()),
            &fixture.state,
            PLUGIN,
            &request_id.to_string()
        )
        .await
        .expect_err("policy changed")
        .contains("停用目标与当前用户策略不一致")
    );
    assert_eq!(
        list(&fixture.runtime, PLUGIN).expect("retained published request")[0].phase,
        Phase::Published
    );
    assert!(request_identity("12345678-1234-4234-8234-123456789ABC").is_err());
    assert!(request_identity(&uuid::Uuid::nil().to_string()).is_err());
}
