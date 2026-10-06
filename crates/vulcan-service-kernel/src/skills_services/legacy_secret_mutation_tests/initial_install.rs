//! Actual two-account OS writes and native processes exercise retained first-install service ownership.
//! 实际双账户操作系统写入及原生进程验证保留首次安装服务所有权。

use super::*;

/// This second declared account sorts after token and carries an actual Some preimage.
/// 此第二已声明账户排序在 token 之后，包含实际存在前像。
const SECOND: &str = "zeta";

/// An isolated real package is removed before the initial installation under test starts.
/// 被测初始安装开始之前，移除隔离真实插件包。
struct InitialFixture {
    /// Existing real-service fixture supplies actual Node, package inspection and OS ownership.
    /// 既有真实服务夹具提供实际 Node、插件包检查及操作系统所有权。
    actual: Fixture,
}

impl InitialFixture {
    /// Prepare an absent actual full-catalog identity and two exact local-secret declarations.
    /// 准备实际完整目录中缺失的身份及两个精确本地秘密声明。
    /// Returns a real service with untouched original enabled/disabled override membership.
    /// 返回原启用及停用覆盖成员关系保持原样的真实服务。
    fn new() -> Self {
        // Use the actual bundled Node package rather than a simulated validation backend.
        // 使用实际捆绑 Node 插件包，而非模拟校验后端。
        let actual = Fixture::new(true);
        actual
            .runtime
            .refresh_catalog(
                &BTreeSet::from([actual.plugin_id.clone()]),
                &BTreeSet::new(),
            )
            .expect("disable original fixture before real removal");
        actual
            .runtime
            .uninstall_user_plugin(&actual.plugin_id)
            .expect("remove actual fixture before first installation");
        // Add exactly one declared local account while preserving every real execution input.
        // 保留每个真实执行输入，同时增加精确单个已声明本地账户。
        let manifest = std::fs::read_to_string(actual.source.join("plugin.yaml")).expect("actual two-account manifest").replace("          FIXTURE_TOKEN: {source: secret, key: token}\n", "          FIXTURE_TOKEN: {source: secret, key: token}\n          FIXTURE_SECOND: {source: secret, key: zeta}\n");
        std::fs::write(actual.source.join("plugin.yaml"), manifest)
            .expect("second exact local account declaration");
        actual
            .runtime
            .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
            .expect("actual absent first-install catalog");
        {
            // The bridge's actual original full pointer has no published plugin identity.
            // 桥的实际原完整指针没有已发布插件身份。
            let mut state = actual.state.lock().expect("initial absent service state");
            // Preserve the original empty membership so later compensation can prove exact restoration.
            // 保留原空成员关系，使后续补偿能够证明精确恢复。
            let bridge = state.agent_runtime.as_mut().expect("actual initial bridge");
            bridge
                .provider_configuration
                .luaskills
                .disabled_vulcan_plugins
                .clear();
            bridge
                .provider_configuration
                .luaskills
                .enabled_vulcan_plugins
                .clear();
            bridge.extension_generation = bridge
                .prepare_plugin_extension_generation(
                    actual.runtime.snapshot(),
                    bridge.extension_generation.mcp_runtime(),
                )
                .expect("actual absent original full pointer");
        }
        // Only the second account has an actual old value; token's original absence is equally authoritative.
        // 仅第二账户具有实际旧值；token 的原缺失同样具有权威。
        let store = crate::plugin_mcp_secret_store::OsPluginMcpSecretStore;
        store
            .delete_secret(&actual.plugin_id, SERVER, SECRET)
            .expect("initial actual first account absence");
        store
            .write_secret(&actual.plugin_id, SERVER, SECOND, "original-second-account")
            .expect("initial actual second account Some preimage");
        Self { actual }
    }

    /// Attach the real Lua engine and exact production bridge before declaring actual installation configuration writes.
    /// 在声明实际安装配置写入之前，附加真实 Lua 引擎及精确生产桥。
    /// fail selects a business failure after two actual writes; returns unit after source approval inputs are fully prepared.
    /// fail 选择两次实际写入之后的业务失败；来源批准输入完整准备后返回空值。
    fn attach_configuration_lifecycle(&self, fail: bool) {
        // Existing service integration fixtures use this same canonical native runtime normalization.
        // 既有服务集成夹具使用此相同规范原生运行时规范化。
        let native_root = vulcan_runtime_assets::native_host_path(
            &self
                .actual
                .root
                .path()
                .canonicalize()
                .expect("actual Lua fixture root"),
        );
        // Only one compatibility VM is needed; actual formal execution keeps its own declared pool limits.
        // 仅需要一个兼容 VM；实际正式执行保留自身声明池上限。
        let engine = luaskills::LuaEngine::new(luaskills::LuaEngineOptions::new(
            luaskills::LuaVmPoolConfig {
                min_size: 1,
                max_size: 1,
                idle_ttl_secs: 60,
            },
            luaskills::LuaRuntimeHostOptions::with_runtime_root(native_root),
        ))
        .expect("actual first-install Lua engine");
        self.actual
            .runtime
            .attach_runtime_engine(Arc::new(engine))
            .expect("same original manager engine");
        std::fs::create_dir_all(self.actual.root.path().join("bridge/runtime"))
            .expect("actual first-install bridge directory");
        std::fs::write(
            self.actual.root.path().join("bridge/dependencies.yaml"),
            "{}\n",
        )
        .expect("actual bridge dependencies");
        std::fs::write(
            self.actual.root.path().join("bridge/runtime/bridge.lua"),
            include_str!("../../../../../plugin-bridge/vulcan-plugin-bridge/runtime/bridge.lua"),
        )
        .expect("actual authoritative bridge source");
        std::fs::create_dir_all(self.actual.root.path().join("distributions"))
            .expect("actual Lua distributions");
        std::fs::create_dir_all(self.actual.source.join("runtime"))
            .expect("actual Lua bundle directory");
        // The exact existing schema keeps native observation paths and declares the callback-owned marker.
        // 精确既有模式保留原生观测路径，并声明回调所有的标记。
        let mut schema: serde_json::Value = serde_json::from_slice(
            &std::fs::read(self.actual.source.join("schema.json"))
                .expect("actual native configuration schema"),
        )
        .expect("actual schema shape");
        schema["properties"]
            .as_object_mut()
            .expect("actual object properties")
            .insert("owner".into(), serde_json::json!({"type":"string"}));
        std::fs::write(
            self.actual.source.join("schema.json"),
            serde_json::to_vec(&schema).expect("actual declared callback schema"),
        )
        .expect("actual callback schema publication");
        // The existing manifest already owns dependencies; append only the one exact Lua declaration.
        // 既有清单已持有依赖；仅追加单个精确 Lua 声明。
        let mut manifest = std::fs::read_to_string(self.actual.source.join("plugin.yaml"))
            .expect("actual Lua source manifest");
        manifest.push_str("lua_runtime:\n  bundle: runtime/plugin.bundle.lua\n  invoke_function: dispatch\n  shutdown_function: shutdown\n  max_active_sessions: 1\n  lifecycle:\n    install: install\n    uninstall: uninstall\n");
        std::fs::write(self.actual.source.join("plugin.yaml"), manifest)
            .expect("actual lifecycle declaration");
        std::fs::write(self.actual.source.join("runtime/plugin.bundle.lua"), format!(r#"
-- Actual declared callbacks use this module's captured exports.
-- 实际声明回调使用此模块捕获的导出。
local M = {{}}
-- Read the actual current document, update owner, and return the committed native value.
-- 读取实际当前文档，更新 owner，并返回已提交原生值。
-- owner is the declared marker; native admission fixes the plugin identity independently.
-- owner 为已声明标记；原生入场独立固定插件身份。
local function write_owner(owner)
    -- The real host getter preserves both native observation paths.
    -- 真实宿主读取器保留两个原生观测路径。
    local document = vulcan.host.call('vulcan.plugin.config.get', {{}})
    document.owner = owner
    return vulcan.host.call('vulcan.plugin.config.update', {{value=document}})
end
-- No arguments are consumed; return the exact success frame for ordinary exports.
-- 不消费参数；为普通导出返回精确成功帧。
function M.dispatch() return vulcan.json.encode({{ok=true,result={{}}}}) end
-- Install writes twice before its actual declared success or failure; no external process is simulated.
-- 安装在实际声明成功或失败之前写入两次；不模拟外部进程。
function M.install()
    write_owner('first-install-write')
    write_owner('second-install-write')
    if {fail} then return vulcan.json.encode({{ok=false,error='actual-install-config-failure'}}) end
    return M.dispatch()
end
-- The real compensator performs another owned write before package rollback can restore absence.
-- 真实补偿器在插件包回滚能够恢复缺失之前执行另一次自有写入。
function M.uninstall()
    write_owner('original-compensator-write')
    return M.dispatch()
end
-- Shutdown shares the captured no-effect export and returns its exact result.
-- 关闭共享已捕获无副作用导出，并返回其精确结果。
M.shutdown = M.dispatch
return M
"#)).expect("actual lifecycle configuration bundle");
        self.actual.runtime.initialize_formal_runtime(vulcan_plugin_runtime::VulcanPluginRuntimeSettings::default()).expect("actual first-install formal parent");
    }

    /// Invoke the actual initial-install service with the exact current source digest and both declared credentials.
    /// 使用精确当前来源摘要及两个已声明凭据调用实际初始安装服务。
    /// app owns actual event delivery; returns the real management result without interception.
    /// app 持有实际事件投递；返回真实管理结果，不进行拦截。
    async fn install(&self, app: ServiceEventEmitter) -> Result<LuaSkillsCatalogUpdate, String> {
        // Production inspection derives the sole fingerprint and complete authorization.
        // 生产检查派生唯一指纹及完整授权。
        let preview = self
            .actual
            .runtime
            .inspect_local_user_plugin(&self.actual.source)
            .expect("actual initial install preview");
        service_install_local_plugin(
            app,
            Arc::clone(&self.actual.state),
            self.actual.source.to_string_lossy().into_owned(),
            preview.content_fingerprint,
            preview.permissions,
            false,
            vec![
                VulcanPluginInitialSecretInput {
                    server_id: SERVER.into(),
                    secret_key: SECRET.into(),
                    value: "next-first-account".into(),
                },
                VulcanPluginInitialSecretInput {
                    server_id: SERVER.into(),
                    secret_key: SECOND.into(),
                    value: "next-second-account".into(),
                },
            ],
        )
        .await
    }

    /// Read the original request through the actual public domain RPC and return its sanitized JSON projection.
    /// 通过实际公开领域 RPC 读取原请求，并返回其脱敏 JSON 投影。
    /// Returns the one exact namespace row or fails if actual ownership has disappeared.
    /// 返回单个精确命名空间行，实际所有权已消失时失败。
    async fn request(&self) -> serde_json::Value {
        // The real dispatch exercises registration, parameter parsing and result serialization.
        // 真实分发验证登记、参数解析及结果序列化。
        let result = crate::rpc_dispatch::dispatch_service_rpc(
            ServiceEventEmitter::new(Vec::new()),
            Arc::clone(&self.actual.state),
            "service_local_plugin_install_requests".into(),
            Some(serde_json::json!({"pluginId":self.actual.plugin_id})),
        )
        .await
        .expect("actual initial request RPC");
        // The domain reply is the actual public JSON row array.
        // 领域应答为实际公开 JSON 行数组。
        let rows = result.as_array().expect("actual public request rows");
        rows.iter()
            .find(|row| row["pluginId"] == self.actual.plugin_id)
            .cloned()
            .expect("original initial request must remain retained")
    }

    /// Recover the exact UUID through real RPC without copying or recreating any initial request.
    /// 通过真实 RPC 恢复精确 UUID，不复制或重建任何初始请求。
    /// app supplies actual event delivery; returns its real domain response.
    /// app 提供实际事件投递；返回真实领域应答。
    async fn recover(
        &self,
        app: ServiceEventEmitter,
        request: &serde_json::Value,
    ) -> Result<serde_json::Value, vulcan_service_api::WebuiServiceRpcError> {
        crate::rpc_dispatch::dispatch_service_rpc(app, Arc::clone(&self.actual.state), "service_recover_local_plugin_install".into(), Some(serde_json::json!({"pluginId":self.actual.plugin_id,"requestId":request["requestId"],"acknowledgeExternalEffects":false}))).await
    }

    /// Assert actual Some/None readback, flat-package absence and exact original policy after compensation.
    /// 补偿后断言实际存在及缺失回读、平铺插件包缺失及精确原策略。
    fn assert_restored(&self) {
        // The native OS store is read directly after real service compensation.
        // 真实服务补偿之后，直接读取原生操作系统存储。
        let store = crate::plugin_mcp_secret_store::OsPluginMcpSecretStore;
        assert!(
            store
                .read_optional_secret(&self.actual.plugin_id, SERVER, SECRET)
                .expect("actual restored first absence")
                .is_none()
        );
        assert_eq!(
            store
                .read_optional_secret(&self.actual.plugin_id, SERVER, SECOND)
                .expect("actual restored second preimage")
                .as_deref(),
            Some("original-second-account")
        );
        assert!(
            !self
                .actual
                .runtime
                .user_plugin_root()
                .join(&self.actual.plugin_id)
                .exists()
        );
        // Compare exact original membership, without inferring it from a later manifest default.
        // 比较精确原成员关系，不从随后清单默认值推断。
        let configuration =
            provider_configuration_from_state(&self.actual.state).expect("actual original policy");
        assert!(
            !configuration
                .luaskills
                .disabled_vulcan_plugins
                .contains(&self.actual.plugin_id)
        );
        assert!(
            !configuration
                .luaskills
                .enabled_vulcan_plugins
                .contains(&self.actual.plugin_id)
        );
        assert!(
            service_local_plugin_install_requests(
                Arc::clone(&self.actual.state),
                self.actual.plugin_id.clone()
            )
            .expect("actual retired original inventory")
            .is_empty()
        );
    }
}

/// Actual service installation publishes the post-callback catalog or restores every callback image after business failure.
/// 实际服务安装发布回调后目录，或在业务失败后恢复每个回调像。
#[tokio::test(flavor = "current_thread")]
async fn initial_install_original_actual_lua_configuration_commits_successor_and_restores_failure()
{
    for fail in [false, true] {
        // Both outcomes use actual Node validation, actual Lua callbacks and actual secure accounts.
        // 两种结果均使用实际 Node 校验、实际 Lua 回调及实际安全账户。
        let fixture = InitialFixture::new();
        fixture.attach_configuration_lifecycle(fail);
        fixture.actual.release();
        // Invoke the same public installation entry that retains the original before all effects.
        // 调用在全部副作用之前保留原实例的相同公共安装入口。
        let result = fixture.install(ServiceEventEmitter::new(Vec::new())).await;
        if fail {
            assert!(
                result
                    .expect_err("actual callback business failure")
                    .contains("actual-install-config-failure")
            );
            fixture.assert_restored();
            assert!(
                matches!(std::fs::symlink_metadata(fixture.actual.runtime.plugin_state_root().join(&fixture.actual.plugin_id).join("config.json")), Err(error) if error.kind() == std::io::ErrorKind::NotFound)
            );
        } else {
            result.expect("actual post-callback full commitment");
            // Capture the sole actual full pointer after service projection completes.
            // 服务投影完成后捕获唯一实际完整指针。
            let full = Arc::clone(
                &fixture
                    .actual
                    .state
                    .lock()
                    .expect("actual post-callback full slot")
                    .agent_runtime
                    .as_ref()
                    .expect("actual post-callback bridge")
                    .extension_generation,
            );
            // The manager's actual successor is produced after both real native configuration writes.
            // 管理器实际后继在两次真实原生配置写入之后产生。
            let actual = fixture.actual.runtime.snapshot();
            // Read only the declared non-secret marker through the actual manager configuration API.
            // 仅通过实际管理器配置接口读取已声明无秘密标记。
            let config = fixture
                .actual
                .runtime
                .plugin_config(&fixture.actual.plugin_id)
                .expect("actual committed callback document");
            assert_eq!(config["owner"], "second-install-write");
            assert_eq!(
                full.plugin_catalog()
                    .expect("actual final full catalog")
                    .revision,
                actual.revision
            );
            assert_eq!(
                full.plugin_catalog()
                    .expect("actual final full catalog")
                    .plugins
                    .get(&fixture.actual.plugin_id)
                    .expect("actual final installed descriptor")
                    .generation,
                actual
                    .plugins
                    .get(&fixture.actual.plugin_id)
                    .expect("actual final manager descriptor")
                    .generation
            );
            assert!(
                service_local_plugin_install_requests(
                    Arc::clone(&fixture.actual.state),
                    fixture.actual.plugin_id.clone()
                )
                .expect("actual committed originals")
                .is_empty()
            );
        }
        // Terminal service ownership and retired real Lua pools must release the same actual namespace File.
        // 终态服务所有权及已退役真实 Lua 池必须释放相同实际命名空间文件。
        let independent =
            VulcanPluginGenerationStore::open(fixture.actual.runtime.user_plugin_root())
                .expect("actual post-callback independent store");
        independent
            .acquire_legacy_mutation_guard(&fixture.actual.plugin_id, None)
            .expect("actual callback and terminal original release the namespace");
    }
}

impl Drop for InitialFixture {
    /// Delete only this fixture's second exact OS account; the parent owns token and native shutdown.
    /// 仅删除此夹具第二精确操作系统账户；父级拥有 token 及原生关闭。
    fn drop(&mut self) {
        let _ = crate::plugin_mcp_secret_store::OsPluginMcpSecretStore.delete_secret(
            &self.actual.plugin_id,
            SERVER,
            SECOND,
        );
    }
}

/// A real successful second write with a lost return restores every actual original account, including that failing write.
/// 返回丢失的真实成功第二次写入恢复每个实际原账户，包括该失败写入。
#[tokio::test(flavor = "current_thread")]
async fn initial_install_original_second_write_return_loss_restores_complete_preimages() {
    // This actual service has one absent and one present original OS account.
    // 此实际服务具有单个缺失及单个存在原操作系统账户。
    let fixture = InitialFixture::new();
    // The contender is an independent physical namespace owner.
    // 竞争者为独立物理命名空间所有者。
    let store = VulcanPluginGenerationStore::open(fixture.actual.runtime.user_plugin_root())
        .expect("initial actual contender");
    // Exact namespace identity moves into the non-secret physical-lock probe.
    // 精确命名空间身份移入无秘密物理锁探针。
    let plugin_id = fixture.actual.plugin_id.clone();
    // Actual readback precedes return loss; no credential backend is mocked.
    // 实际回读先于返回丢失；不模拟凭据后端。
    let fault = crate::plugin_mcp_secret_store::legacy_write_observation::Scope::lose_next_write(
        &fixture.actual.plugin_id,
        SERVER,
        SECOND,
        move || matches!(store.acquire_legacy_mutation_guard(&plugin_id, None), Err(error) if error.code == vulcan_plugin_runtime::VulcanPluginStoreErrorCode::Busy),
    );
    assert!(
        fixture
            .install(ServiceEventEmitter::new(Vec::new()))
            .await
            .is_err()
    );
    assert!(fault.confirmed_write() && fault.confirmed_busy());
    drop(fault);
    fixture.assert_restored();
}

/// A post-write executor panic preserves complete preimages, original UUID and actual file exclusion for explicit RPC recovery.
/// 写后执行器 panic 保留完整前像、原 UUID 及实际文件排他，以供明确 RPC 恢复。
#[tokio::test(flavor = "current_thread")]
async fn initial_install_original_second_write_panic_retains_same_request_for_rpc_recovery() {
    // Real package and two OS accounts have independent original images.
    // 真实插件包及两个操作系统账户具有独立原像。
    let fixture = InitialFixture::new();
    // The actual contender detects that the namespace was registered before the OS effect.
    // 实际竞争者检测命名空间在操作系统副作用之前已被登记。
    let store = VulcanPluginGenerationStore::open(fixture.actual.runtime.user_plugin_root())
        .expect("panic actual namespace contender");
    // This clone is only the exact namespace spelling, never credential or transaction authority.
    // 此克隆仅为精确命名空间拼写，绝非凭据或事务权威。
    let plugin_id = fixture.actual.plugin_id.clone();
    // The scoped observer panics only after actual successful write and readback.
    // 作用域观察者仅在实际成功写入及回读之后 panic。
    let fault = crate::plugin_mcp_secret_store::legacy_write_observation::Scope::panic_next_write(
        &fixture.actual.plugin_id,
        SERVER,
        SECOND,
        move || matches!(store.acquire_legacy_mutation_guard(&plugin_id, None), Err(error) if error.code == vulcan_plugin_runtime::VulcanPluginStoreErrorCode::Busy),
    );
    assert!(
        fixture
            .install(ServiceEventEmitter::new(Vec::new()))
            .await
            .expect_err("actual initial executor panic")
            .contains("任务异常结束")
    );
    assert!(fault.confirmed_write() && fault.confirmed_busy());
    drop(fault);
    // Public lookup must still resolve the allocated original, without printing either supplied value.
    // 公共查询必须仍解析已分配原实例，不打印任一提交值。
    let request = fixture.request().await;
    assert_eq!(request["phase"], "credentialMutation");
    assert!(!request.to_string().contains("account"));
    assert_eq!(fixture.request().await, request);
    // Wrong UUID and namespace must fail before any actual credential adapter entry.
    // 错误 UUID 及命名空间必须在任何实际凭据适配器入口之前失败。
    let wrong_access =
        crate::plugin_mcp_secret_store::legacy_write_observation::Scope::count_access(
            &fixture.actual.plugin_id,
            SERVER,
            SECOND,
        );
    for (plugin_id, request_id) in [
        (
            fixture.actual.plugin_id.clone(),
            uuid::Uuid::new_v4().to_string(),
        ),
        (
            format!("wrong-{}", fixture.actual.plugin_id),
            request["requestId"]
                .as_str()
                .expect("original UUID text")
                .to_owned(),
        ),
    ] {
        assert!(crate::rpc_dispatch::dispatch_service_rpc(
            ServiceEventEmitter::new(Vec::new()), Arc::clone(&fixture.actual.state),
            "service_recover_local_plugin_install".into(),
            Some(serde_json::json!({"pluginId":plugin_id,"requestId":request_id,"acknowledgeExternalEffects":false})),
        ).await.is_err());
    }
    assert_eq!(wrong_access.access_count(), 0);
    drop(wrong_access);
    // The same physical file remains Busy after the executor and its local borrows have disappeared.
    // 执行器及其局部借用已消失之后，同一物理文件仍保持忙碌。
    let independent = VulcanPluginGenerationStore::open(fixture.actual.runtime.user_plugin_root())
        .expect("post-panic actual contender");
    assert_eq!(
        independent
            .acquire_legacy_mutation_guard(&fixture.actual.plugin_id, None)
            .expect_err("retained actual original file")
            .code,
        vulcan_plugin_runtime::VulcanPluginStoreErrorCode::Busy
    );
    fixture
        .recover(ServiceEventEmitter::new(Vec::new()), &request)
        .await
        .expect("actual same UUID RPC recovery");
    fixture.assert_restored();
    independent
        .acquire_legacy_mutation_guard(&fixture.actual.plugin_id, None)
        .expect("confirmed actual retirement releases original file");
}

/// Actual shutdown drains a panic-retained first installation before releasing its credentials or namespace owner.
/// 实际关闭在释放凭据或命名空间所有者之前排空 panic 保留首次安装。
#[tokio::test(flavor = "current_thread")]
async fn initial_install_original_shutdown_drains_same_preimages_and_package() {
    // The real OS preimages are captured by the service before any write.
    // 真实操作系统前像由服务在任何写入之前捕获。
    let fixture = InitialFixture::new();
    // Exact-account post-write panic preserves the same registered original.
    // 精确账户写后 panic 保留同一已登记原实例。
    let fault = crate::plugin_mcp_secret_store::legacy_write_observation::Scope::panic_next_write(
        &fixture.actual.plugin_id,
        SERVER,
        SECOND,
        || true,
    );
    assert!(
        fixture
            .install(ServiceEventEmitter::new(Vec::new()))
            .await
            .is_err()
    );
    assert!(fault.confirmed_write());
    drop(fault);
    // The actual shutdown admission closes before taking the strong original owner inventory.
    // 实际关闭准入在取得强原所有者清单之前关闭。
    let registry = {
        let mut state = fixture
            .actual
            .state
            .lock()
            .expect("actual initial shutdown admission");
        state.service_lifecycle_state.begin_shutdown_drain();
        Arc::clone(&state.pending_local_plugin_installs)
    };
    assert!(registry.require_empty_without_runtime().is_err());
    registry
        .drain_for_shutdown(&fixture.actual.state)
        .await
        .expect("actual retained installation shutdown drain");
    fixture.assert_restored();
}

/// A panic after real initialize and discovery retains the actual original Node lease until recovery confirms physical retirement.
/// 真实初始化及发现之后 panic 保留实际原 Node 租约，直至恢复确认物理退役。
#[tokio::test(flavor = "current_thread")]
async fn initial_install_original_native_ready_panic_keeps_physical_owner_until_recovery() {
    // The actual bundled executable performs real initialize and discovery.
    // 实际捆绑可执行文件执行真实初始化及发现。
    let fixture = InitialFixture::new();
    fixture.actual.release();
    // The exact post-ready observer retains no native handle and supplies no cleanup proof.
    // 精确就绪后观察者不保留原生句柄，亦不提供清理证明。
    let fault = legacy_native_observation::Scope::arm(
        &fixture.actual.plugin_id,
        legacy_native_observation::Milestone::Ready,
    );
    assert!(
        fixture
            .install(ServiceEventEmitter::new(Vec::new()))
            .await
            .is_err()
    );
    assert!(fault.fired());
    drop(fault);
    // Real materialization selects the sole actual command used by the retained original controller.
    // 真实物化选择保留原控制器使用的唯一实际命令。
    let executable = fixture.actual.native_command();
    assert!(
        std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&executable)
            .is_err(),
        "actual original Node must still own the Windows executable"
    );
    // The marker belongs to actual original initialization, never an inferred Connected status.
    // 标记属于实际原初始化，绝不是推断出的已连接状态。
    let pid =
        std::fs::read_to_string(&fixture.actual.ready_entry).expect("actual original Node PID");
    assert!(pid.trim().parse::<u32>().expect("actual original PID text") > 0);
    // The same original UUID remains queryable while native and package resources are physically live.
    // 原生及插件包资源物理存活时，同一原 UUID 仍可查询。
    let request = fixture.request().await;
    assert_eq!(request["phase"], "nativeValidation");
    // Retained private catalog state must not authorize the old service uninstall flow to run callbacks or rewrite policy.
    // 保留私有目录状态不得授权旧服务卸载流程运行回调或重写策略。
    let policy = provider_configuration_from_state(&fixture.actual.state)
        .expect("original pending install policy");
    // Observe actual OS adapter entries without substituting a backend or reading a credential value.
    // 观察实际操作系统适配器入口，不替换后端或读取凭据值。
    let competing_access =
        crate::plugin_mcp_secret_store::legacy_write_observation::Scope::count_access(
            &fixture.actual.plugin_id,
            SERVER,
            SECOND,
        );
    // The public RPC must return this original actionable UUID before all old uninstall side effects.
    // 公共 RPC 必须在全部旧卸载副作用前返回此原可操作 UUID。
    let error = crate::rpc_dispatch::dispatch_service_rpc(
        ServiceEventEmitter::new(Vec::new()),
        Arc::clone(&fixture.actual.state),
        "service_uninstall_user_plugin".into(),
        Some(serde_json::json!({"pluginId": fixture.actual.plugin_id})),
    )
    .await
    .expect_err("retained original blocks public uninstall");
    assert!(
        error.to_string().contains(
            request["requestId"]
                .as_str()
                .expect("original actionable UUID")
        )
    );
    assert_eq!(competing_access.access_count(), 0);
    drop(competing_access);
    // Exact original override membership survives the competing management request.
    // 精确原覆盖成员关系跨竞争管理请求存续。
    let after = provider_configuration_from_state(&fixture.actual.state)
        .expect("actual unchanged pending policy");
    assert_eq!(
        after.luaskills.disabled_vulcan_plugins,
        policy.luaskills.disabled_vulcan_plugins
    );
    assert_eq!(
        after.luaskills.enabled_vulcan_plugins,
        policy.luaskills.enabled_vulcan_plugins
    );
    assert_eq!(fixture.request().await, request);
    assert!(
        std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&executable)
            .is_err()
    );
    // An unrelated complete workbench projection cannot publish the manager's still-private installation descriptor.
    // 无关完整工作台投影不能发布管理器仍私有的安装描述符。
    let projection_error = refresh_plugin_catalog_local_state(&fixture.actual.state)
        .await
        .err()
        .expect("private original must not leak through complete projection");
    assert!(
        projection_error.contains(
            request["requestId"]
                .as_str()
                .expect("original projection conflict UUID")
        )
    );
    // Enablement must reject the same original before scanning or invalidating its private callback generation.
    // 启用必须在扫描或使私有回调代次失效之前拒绝同一原实例。
    let enable_error = service_set_vulcan_plugin_enabled(
        ServiceEventEmitter::new(Vec::new()),
        Arc::clone(&fixture.actual.state),
        fixture.actual.plugin_id.clone(),
        true,
    )
    .await
    .expect_err("retained original excludes enablement");
    assert!(
        enable_error.contains(
            request["requestId"]
                .as_str()
                .expect("original enable conflict UUID")
        )
    );
    // Whole discovery rejects before its ordinary-skill handle or private manager generation is touched.
    // 整个发现于普通技能句柄或私有管理器代次被触及之前拒绝。
    let refresh_error = service_refresh_luaskills_catalog(
        ServiceEventEmitter::new(Vec::new()),
        Arc::clone(&fixture.actual.state),
    )
    .await
    .expect_err("retained original excludes complete discovery");
    assert!(
        refresh_error.contains(
            request["requestId"]
                .as_str()
                .expect("original discovery conflict UUID")
        )
    );
    // The actual generic complete lifecycle publisher must also refuse this private descriptor before MCP preparation.
    // 实际通用完整生命周期发布器同样必须在 MCP 准备之前拒绝此私有描述符。
    let publication_error = vulcan_plugin_lifecycle::publish_disabled_plugin_extension_generation(
        &fixture.actual.state,
        &fixture.actual.plugin_id,
        fixture.actual.runtime.snapshot(),
    )
    .expect_err("retained private original excludes generic publication");
    assert!(
        publication_error.contains(
            request["requestId"]
                .as_str()
                .expect("original generic publication conflict UUID")
        )
    );
    assert!(
        !fixture
            .actual
            .state
            .lock()
            .expect("actual uncommitted full after projection")
            .agent_runtime
            .as_ref()
            .expect("actual uncommitted bridge")
            .extension_generation
            .plugin_catalog()
            .expect("actual uncommitted catalog")
            .plugins
            .contains_key(&fixture.actual.plugin_id)
    );
    fixture
        .recover(ServiceEventEmitter::new(Vec::new()), &request)
        .await
        .expect("actual original native retirement before package rollback");
    fixture.assert_restored();
    assert!(
        matches!(std::fs::symlink_metadata(&executable), Err(error) if error.kind() == std::io::ErrorKind::NotFound),
        "owned environment removal requires actual original physical retirement"
    );
    assert_eq!(
        std::fs::read_to_string(&fixture.actual.ready_entry)
            .expect("retained original PID after recovery"),
        pid,
        "recovery must never initialize a second native instance"
    );
}

/// A real client event panic after full commitment keeps installed values and retries projection without any OS replay.
/// 完整提交之后真实客户端事件 panic 保留已安装值，并在不重放操作系统操作时重试投影。
#[tokio::test(flavor = "current_thread")]
async fn initial_install_original_committed_event_failure_reprojects_without_secret_rollback() {
    // Release the actual Node handshake before initial validation starts.
    // 初始校验开始之前放行实际 Node 握手。
    let fixture = InitialFixture::new();
    fixture.actual.release();
    // This actual subscriber panics before its first delivery rather than mocking the service publisher.
    // 此实际订阅者在首次投递之前 panic，而非模拟服务发布器。
    let events = Arc::new(RecoveryEvents::default());
    events
        .fail_before_delivery
        .store(true, std::sync::atomic::Ordering::Release);
    // The public event emitter calls the actual client after full pointer and service-phase confirmation.
    // 公开事件发射器在完整指针及服务阶段确认之后调用实际客户端。
    let app = ServiceEventEmitter::new(vec![events.clone()]);
    assert!(fixture.install(app.clone()).await.is_err());
    // Exact public recovery identity must describe the committed original.
    // 精确公共恢复身份必须描述已提交原实例。
    let request = fixture.request().await;
    assert_eq!(request["phase"], "committed");
    // Retain the actual committed pointer as evidence, without manufacturing a source lease.
    // 保留实际已提交指针作为证据，不制造来源租约。
    let committed = Arc::clone(
        &fixture
            .actual
            .state
            .lock()
            .expect("actual initial committed full")
            .agent_runtime
            .as_ref()
            .expect("actual bridge")
            .extension_generation,
    );
    assert_eq!(
        committed
            .plugin_catalog()
            .expect("actual committed plugin catalog")
            .plugins
            .get(&fixture.actual.plugin_id)
            .expect("actual committed initial descriptor")
            .runtime_state,
        vulcan_plugin_runtime::VulcanPluginRuntimeState::Disabled
    );
    assert!(
        committed
            .mcp_runtime()
            .available_tool_bindings_filtered(&Default::default())
            .is_empty()
    );
    // This exact account counter observes all actual adapter entry without altering the native store.
    // 此精确账户计数器观测全部实际适配器入口，不改变原生存储。
    let accesses = crate::plugin_mcp_secret_store::legacy_write_observation::Scope::count_access(
        &fixture.actual.plugin_id,
        SERVER,
        SECOND,
    );
    fixture
        .recover(app, &request)
        .await
        .expect("actual committed initial RPC projection retry");
    assert_eq!(accesses.access_count(), 0);
    drop(accesses);
    // The existing complete projection deliberately republishes its pointer once, preserving the same installed generation.
    // 既有完整投影按设计再次发布其指针一次，同时保留同一已安装代次。
    let projected = Arc::clone(
        &fixture
            .actual
            .state
            .lock()
            .expect("actual initial pointer after retry")
            .agent_runtime
            .as_ref()
            .expect("actual bridge after retry")
            .extension_generation,
    );
    assert_eq!(projected.revision(), committed.revision() + 1);
    assert_eq!(
        projected
            .plugin_catalog()
            .expect("actual projected catalog")
            .plugins
            .get(&fixture.actual.plugin_id)
            .expect("actual projected initial descriptor")
            .generation,
        committed
            .plugin_catalog()
            .expect("actual committed catalog")
            .plugins
            .get(&fixture.actual.plugin_id)
            .expect("actual committed initial descriptor")
            .generation
    );
    assert!(
        fixture
            .actual
            .runtime
            .user_plugin_root()
            .join(&fixture.actual.plugin_id)
            .exists()
    );
    // Actual OS readback must still show both committed values after original registry retirement.
    // 原注册表退役之后，实际操作系统回读必须仍显示两个已提交值。
    let store = crate::plugin_mcp_secret_store::OsPluginMcpSecretStore;
    assert_eq!(
        store
            .read_optional_secret(&fixture.actual.plugin_id, SERVER, SECRET)
            .expect("actual committed first account")
            .as_deref(),
        Some("next-first-account")
    );
    assert_eq!(
        store
            .read_optional_secret(&fixture.actual.plugin_id, SERVER, SECOND)
            .expect("actual committed second account")
            .as_deref(),
        Some("next-second-account")
    );
    assert!(
        service_local_plugin_install_requests(
            Arc::clone(&fixture.actual.state),
            fixture.actual.plugin_id.clone()
        )
        .expect("confirmed retired initial original")
        .is_empty()
    );
    assert_eq!(
        events
            .events
            .lock()
            .expect("actual initial recovery client event")
            .len(),
        1
    );
}
