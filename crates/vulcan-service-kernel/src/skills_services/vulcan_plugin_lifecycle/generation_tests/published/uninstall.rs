//! Actual public disable drains two original Node transports before retained public removal collects their files.
//! 实际公开停用先排空两个原 Node 传输，再由保留公开移除回收其文件。

use super::*;
use crate::tests::provider_configuration_persistence_scope::PROVIDER_CONFIGURATION_PATH;
use vulcan_agent_runtime::agent_runtime::ExtensionGeneration;
use vulcan_agent_runtime::mcp::McpRuntimeToolAllowlist;
use vulcan_plugin_runtime::{VulcanPluginRuntimeState, VulcanPluginStoreErrorCode};

/// Read the actual service Full under its publication mutex; state selects the sole owner.
/// 在发布互斥锁下读取实际服务完整代次；state 选择唯一所有者。
/// Return the exact captured Arc without rebuilding a catalog or native control.
/// 返回精确捕获引用，不重建目录或原生控制。
fn full(state: &SharedServiceBridgeState) -> Arc<ExtensionGeneration> {
    Arc::clone(
        &state
            .lock()
            .expect("actual service Full lock")
            .agent_runtime
            .as_ref()
            .expect("actual service bridge")
            .extension_generation,
    )
}

/// Invoke the discovered original_process through generation with message as its exact input.
/// 通过 generation 调用已发现 original_process，以 message 作为精确输入。
/// Return the genuine positive child PID after validating its business echo.
/// 校验业务回显后，返回真实正数子进程 PID。
async fn invoke_pid(generation: &Arc<ExtensionGeneration>, message: &str) -> u64 {
    // A stable origin identifies the actual declaration independently of inventory ordering.
    // 稳定来源身份独立于清单顺序定位实际声明。
    let binding = generation
        .mcp_runtime()
        .available_tool_bindings_filtered(&McpRuntimeToolAllowlist::default())
        .into_iter()
        .find(|binding| binding.origin_tool_id() == "published-service:actual/original_process")
        .expect("actual original_process declaration may have changed");
    // The native reply proves this business invocation used the original connected child.
    // 原生回复证明此次业务调用使用原已连接子进程。
    let result = binding
        .invoke(serde_json::json!({"message":message}))
        .await
        .expect("actual original child business");
    assert_eq!(result["message"], message);
    // A positive operating-system PID supplies child evidence without replacing readiness authority.
    // 正数操作系统 PID 提供子进程证据，不替代就绪权威。
    let pid = result["pid"].as_u64().expect("actual original child PID");
    assert!(pid > 0);
    pid
}

/// Public disable drains actual current B and displaced A before public uninstall publishes absence and collects.
/// 公开停用排空实际当前 B 及被替换 A，再由公开卸载发布缺失并回收。
/// No inputs or return value; genuine cleanup receipts, external package pins and namespace Files prove the sequence.
/// 无输入或返回值；真实清理回执、外部插件包固定及命名空间文件证明执行顺序。
#[tokio::test]
async fn retained_service_uninstall_published_node_disable_drains_originals_before_collection() {
    // The existing configured fixture retains immutable bytes but has not launched a native child.
    // 既有已配置夹具保留不可变字节，尚未启动原生子进程。
    let Fixture {
        state,
        configs,
        marker,
        environment,
        _temp,
    } = fixture();
    // One actual service read preserves the manager and its first registered native issuer.
    // 单次实际服务读取保留管理器及其首次已登记原生签发者。
    let (manager, authority) =
        crate::service_state_access::service_vulcan_plugin_native_mcp_host(&state)
            .expect("actual original service source");
    // Both originals share this real access lineage and the same enabled managed selection.
    // 两个原实例共享此真实访问谱系及相同启用受管选择。
    let initial = full(&state);
    // The first native producer consumes the fixture's sealed inputs exactly once.
    // 首个原生生产者精确一次消费夹具密封输入。
    let original_a =
        PublishedPluginMcpRuntime::start(&initial.mcp_runtime(), &manager, configs, &authority)
            .expect("actual original A startup");
    // This observer owns actual retirement evidence without retaining execution resources.
    // 此观察器持有实际退役证据，不保留执行资源。
    let observation_a = original_a.observer();
    observation_a
        .wait_until_ready(OBSERVE)
        .await
        .expect("actual original A ready");
    // The actual publisher transfers A into the service's sole Full slot.
    // 实际发布器将 A 转交至服务唯一完整槽。
    let published_a = publish_native_published_plugin_extension_generation(
        &state,
        &original_a,
        manager.snapshot(),
    )
    .expect("actual original A Full");
    // Normal business confirms A's real PID before its Full becomes historical.
    // 普通业务在 A 的完整代次成为历史之前确认其真实 PID。
    let pid_a = invoke_pid(&published_a, "actual A before replacement").await;
    drop(original_a);
    drop(initial);

    // A second production preparation uses the unchanged actual source and first issuer, without mutating metadata.
    // 第二次生产准备使用未变实际来源及首次签发者，不修改元数据。
    let prepared_b = manager
        .prepare_published_mcp_resources(&manager.snapshot(), "published-service", &authority)
        .expect("actual original B resource preparation");
    // The already completed empty revision forbids external account access while sealing B's real definitions.
    // 既有已完成空修订在密封 B 真实定义时禁止外部账户访问。
    let configs_b = published_global_mcp_server_configs(prepared_b, &authority, &EmptyCredentials)
        .expect("actual original B sealed definitions");
    // Actual current access registers B in the same physical admission and cleanup union before dispatch.
    // 实际当前访问在派发前将 B 登记至相同物理准入及清理集合。
    let original_b = PublishedPluginMcpRuntime::start(
        &published_a.mcp_runtime(),
        &manager,
        configs_b,
        &authority,
    )
    .expect("actual original B startup");
    // The second non-owning observer independently confirms B's exact native lifetime.
    // 第二个非拥有观察器独立确认 B 精确原生生命周期。
    let observation_b = original_b.observer();
    observation_b
        .wait_until_ready(OBSERVE)
        .await
        .expect("actual original B ready");
    // Publishing B displaces the still-retained actual A Full; both children remain genuine participants.
    // 发布 B 替换仍被保留的实际 A 完整代次；两个子进程仍为真实参与者。
    let published_b = publish_native_published_plugin_extension_generation(
        &state,
        &original_b,
        manager.snapshot(),
    )
    .expect("actual original B Full");
    // Current business must reach B rather than silently reuse A's transport.
    // 当前业务必须到达 B，不得静默复用 A 传输。
    let pid_b = invoke_pid(&published_b, "actual B before disable").await;
    assert_ne!(pid_a, pid_b);
    assert_eq!(
        invoke_pid(&published_a, "actual displaced A before disable").await,
        pid_a
    );
    // Complete child-written startup records match the two independently invoked originals.
    // 完整子进程写入启动记录与两个独立调用原实例匹配。
    let starts = predecessor::starts(&marker);
    assert_eq!(
        starts.len(),
        2,
        "actual two-child workload may have changed"
    );
    assert_eq!(
        starts
            .iter()
            .map(|record| record["pid"].as_u64().expect("actual startup PID"))
            .collect::<BTreeSet<_>>(),
        BTreeSet::from([pid_a, pid_b])
    );
    drop(original_b);

    // Private policy persistence must cover the actual public service's owned management tasks.
    // 私有策略持久化必须覆盖实际公开服务拥有的管理任务。
    let policy = _temp.path().join("node-uninstall-policy.json");
    PROVIDER_CONFIGURATION_PATH
        .scope(policy.clone(), async {
            // A legal public disable performs union drain; no test fabricates a Disabled descriptor.
            // 合法公开停用执行集合排空；测试不伪造停用描述符。
            service_set_vulcan_plugin_enabled(
                ServiceEventEmitter::new(Vec::new()),
                Arc::clone(&state),
                String::from("published-service"),
                false,
            )
            .await
            .expect("actual public service disable");
            assert!(policy.is_file(), "actual owned task must persist privately");
            // These exact original observers require empty transports/inputs/releases, positive cleanup and aggregate release.
            // 这些精确原观察器要求传输／输入／释放集合为空、正向清理及聚合释放。
            observation_a
                .wait_until_released(OBSERVE)
                .await
                .expect("actual displaced A physical cleanup");
            observation_b
                .wait_until_released(OBSERVE)
                .await
                .expect("actual current B physical cleanup");
            assert_eq!(
                manager
                    .active_session_count("published-service")
                    .expect("actual session map"),
                0
            );
            assert!(
                !manager
                    .has_cleanup_diagnostics("published-service")
                    .expect("actual cleanup diagnostics")
            );
            assert!(
                !published_b
                    .mcp_runtime()
                    .has_usable_retired_plugin_partitions("published-service")
            );
            assert_eq!(predecessor::starts(&marker), starts);
            drop(published_a);
            drop(published_b);

            // The genuine disabled catalog retains the real managed package reader, independently of service ownership.
            // 真实停用目录独立于服务所有权保留真实受管插件包读者。
            let external = manager.snapshot();
            assert_eq!(
                external.plugins["published-service"].runtime_state,
                VulcanPluginRuntimeState::Disabled
            );
            // Copy only the authoritative physical path; external itself owns the actual pin that must block collection.
            // 仅复制权威物理路径；external 本身持有必须阻止回收的实际固定。
            let package = external.plugins["published-service"].package_root.clone();
            assert!(package.is_dir());
            assert!(environment.is_dir());
            service_uninstall_user_plugin(
                ServiceEventEmitter::new(Vec::new()),
                Arc::clone(&state),
                String::from("published-service"),
            )
            .await
            .expect_err("actual external package pin must block collection");
            // Live query identifies the retained original by plugin identity, never by row order.
            // 实时查询按插件身份定位保留原实例，绝不依赖行顺序。
            let requests = serde_json::to_value(
                local_plugin_uninstall_records(Arc::clone(&state))
                    .expect("actual retained removal query"),
            )
            .expect("actual public retained removal protocol");
            // The declared public JSON projection exposes private status fields without broadening Rust visibility.
            // 已声明公开 JSON 投影暴露私有状态字段，不扩大 Rust 可见性。
            let request = requests
                .as_array()
                .expect("declared retained removal array")
                .iter()
                .find(|request| request["pluginId"] == "published-service")
                .expect("actual retained removal");
            assert_eq!(request["phase"], "collection");
            assert_eq!(request["requiresExternalEffectsAcknowledgement"], false);
            // Only the original serialized request identity can select this retained removal on resume.
            // 仅原序列化请求身份可在恢复时选择此保留移除。
            let request_id = request["requestId"]
                .as_str()
                .expect("declared original request ID")
                .to_owned();
            uuid::Uuid::parse_str(&request_id).expect("actual stable request UUID");
            // Physical collection failed only after the actual service Full committed this plugin's absence.
            // 物理回收仅在实际服务完整代次提交此插件缺失之后失败。
            let removed = full(&state);
            assert!(
                !removed
                    .plugin_catalog()
                    .expect("actual removed catalog")
                    .plugins
                    .contains_key("published-service")
            );
            assert!(!manager.snapshot().plugins.contains_key("published-service"));
            assert!(
                removed
                    .mcp_runtime()
                    .available_tool_bindings_filtered(&McpRuntimeToolAllowlist::default())
                    .into_iter()
                    .all(|binding| binding.origin_tool_id()
                        != "published-service:actual/original_process")
            );
            assert!(package.is_dir());
            // An independent actual Store contender must observe the still-retained original namespace File.
            // 独立实际存储竞争者必须观察仍被保留的原命名空间文件。
            let store = VulcanPluginGenerationStore::open_existing(manager.user_plugin_root())
                .expect("actual independent store")
                .expect("actual existing managed store");
            // An independent public activation read must acquire the same namespace File before it can inspect selection.
            // 独立公开激活读取必须取得相同命名空间文件，才能检查选择。
            assert_eq!(
                store
                    .activation("published-service")
                    .expect_err("actual original File must exclude independent readers")
                    .code,
                VulcanPluginStoreErrorCode::Busy
            );
            drop(external);
            service_resume_local_plugin_uninstall(
                ServiceEventEmitter::new(Vec::new()),
                Arc::clone(&state),
                String::from("published-service"),
                request_id,
                false,
            )
            .await
            .expect("same actual removal collection retry");
            assert!(Arc::ptr_eq(&removed, &full(&state)));
            assert!(!package.exists());
            assert!(!environment.exists());
            assert!(
                local_plugin_uninstall_records(Arc::clone(&state))
                    .expect("actual terminal query")
                    .is_empty()
            );
            // Terminal read returns the actual complete tombstone only after the original physical File is released.
            // 仅原物理文件释放后，终态读取才返回实际完整墓碑。
            let selection = store
                .activation("published-service")
                .expect("actual terminal selection");
            store
                .acquire_namespace_guard("published-service", selection.as_ref())
                .expect("actual original namespace File released");
            assert_eq!(predecessor::starts(&marker), starts);
        })
        .await;
}
