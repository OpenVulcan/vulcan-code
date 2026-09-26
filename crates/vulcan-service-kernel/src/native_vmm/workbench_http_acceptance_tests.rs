//! Built workbench browser acceptance with private native storage and production RPC results.
//! 使用私有原生存储及生产 RPC 结果验收构建后的完整工作台页面。

use super::*;
use std::io::Write;

#[path = "workbench_lifecycle_acceptance_tests.rs"]
mod lifecycle;

/// Waits for an operator-observed browser phase in the exact private directory, bounded to twenty minutes.
/// 在准确私有目录等待操作者已观察的浏览器阶段，最多二十分钟。
/// Takes a directory and fixed phase name; returns only after its explicit marker, or fails on abort/timeout.
/// 接收目录与固定阶段名；仅在明确标记后返回，中止或超时则失败。
async fn browser_phase(root: &Path, phase: &str) {
    // Browser inspection includes screenshots and actual user navigation, rather than a synchronous DOM runner.
    // 浏览器检查包含截图及实际用户导航，而非同步 DOM 运行器。
    let deadline = tokio::time::Instant::now() + Duration::from_secs(1_200);
    while !root.join(format!("{phase}.ready")).exists() {
        assert!(
            !root.join("abort.ready").exists(),
            "private browser aborted"
        );
        assert!(
            tokio::time::Instant::now() < deadline,
            "private browser phase expired: {phase}"
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// Runs a real compiled workbench over the existing native HTTP service, with no browser response stubs.
/// 在既有原生 HTTP 服务上运行真实编译工作台，不伪造浏览器响应。
/// Takes isolated storage/capability and explicit directories; verifies policy and source lifecycle before closing the server.
/// 接收隔离存储／能力及显式目录；检查策略及来源生命周期后关闭服务。
pub(in crate::native_vmm) async fn verify(
    data_plane: VmmDataPlaneHandle,
    capability: InferenceCapabilitySnapshot,
    root: &Path,
    assets: &Path,
) {
    assert_eq!(
        std::fs::read_dir(root)
            .expect("private workbench directory")
            .count(),
        0
    );
    // This fixture controls inference only; all observed browser results come from real service handlers.
    // 本夹具仅控制推理；全部观察到的浏览器结果来自真实服务处理器。
    let port = Arc::new(CapturingAnalysisPort {
        inner: MockHostAiPort::new(),
        requests: StdMutex::new(Vec::new()),
        reject_next_turn: AtomicBool::new(false),
        fixture: AnalysisFixture::PolicyHttp,
        embedding_requests: StdMutex::new(Vec::new()),
        profile_instruction_pause: StdMutex::new(None),
        postaction_pause: StdMutex::new(None),
    });
    port.inner
        .set_capability_snapshot(capability)
        .expect("workbench capability");
    // All physical records are retained by this exact private native runtime.
    // 全部物理记录由此准确私有原生运行时持有。
    let runtime = Arc::new(
        NativeVmmCoreRuntime::bootstrap(port.clone(), data_plane.clone())
            .await
            .expect("workbench native runtime"),
    );
    // Preserve the independent initial policy, rather than trusting the browser's later success notice.
    // 保留独立初始策略，而不信任浏览器后续成功提示。
    let mut initial = runtime
        .runtime_policy()
        .await
        .expect("workbench initial policy");
    // The public client and maintenance service share the same production operation gate.
    // 公开客户端及维护服务共用同一生产操作门。
    let gate = Arc::new(RwLock::new(()));
    // Reuse normal direct-write acceptance so visible records are really materialized in both storage owners.
    // 复用正常主动写入验收，使可见记录真实物化到两个存储所有者。
    let test = Acceptance {
        service: policy_service::service(runtime.clone(), data_plane.clone(), gate.clone()),
        client: NativeVmmClient::new(
            runtime.clone(),
            data_plane.clone(),
            gate,
            Arc::new(AtomicBool::new(true)),
            Arc::new(Notify::new()),
        ),
        runtime,
        port,
        data_plane,
    };
    // Retain the authoritative created project for the browser's source-lifecycle acceptance.
    // 保留权威创建项目，用于浏览器来源生命周期验收。
    let seeds = test
        .seeds("browser-native-parity", &expected_global())
        .await;
    lifecycle::prepare(&test, seeds.project_id).await;
    test.save_global(&initial.retrieval).await;
    // Preparatory explicit saves create the user policy file even when restoring all default values.
    // 准备阶段的显式保存即使恢复全部默认值，也会创建用户策略文件。
    initial.user_override = true;
    assert_eq!(
        test.runtime
            .runtime_policy()
            .await
            .expect("workbench prepared policy"),
        initial
    );
    {
        // Only the empty synthetic workspace is projected; no user project path can be traversed.
        // 仅投影空的合成工作区；不能遍历用户项目路径。
        let mut state = test.service.lock().expect("private workbench projection");
        // The saved backend is read from the actual private agent configuration.
        // 从实际私有智能体配置读取已保存后端。
        let configuration = state
            .agent_runtime
            .as_ref()
            .expect("private agent")
            .provider_configuration
            .clone();
        // The same maintenance instance serves status and all browser VMM actions.
        // 同一个维护实例提供状态及全部浏览器 VMM 动作。
        let maintenance = state
            .native_vmm_maintenance
            .as_ref()
            .expect("private maintenance");
        maintenance.set_native_backend_enabled(true);
        // Use the production snapshot projector, not a handcrafted native UI response.
        // 使用生产快照投影器，而非手工构造原生界面响应。
        let snapshot = crate::vmm::vmm_memory_snapshot(
            state
                .workspace_snapshot
                .as_ref()
                .expect("private workspace"),
            &configuration,
            None,
            Some(&maintenance.host_ai_status()),
        );
        state.workbench.vmm_memory = snapshot;
    }
    // The complete records must survive navigation and configuration saves before explicit lifecycle mutations.
    // 完整记录必须在明确生命周期变更前的导航及配置保存期间保持不变。
    let before = test
        .runtime
        .storage_snapshot()
        .expect("workbench original storage");
    // Binding an ephemeral loopback listener avoids the normal application discovery range.
    // 绑定临时回环监听器以避开正常应用发现范围。
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("workbench listener");
    // The actual listener is the authority for the browser URL.
    // 实际监听器是浏览器地址的权威。
    let address = listener.local_addr().expect("workbench address");
    // This ephemeral capability belongs only to the private test service and is never logged.
    // 此临时能力仅属于私有测试服务，绝不写入日志。
    let token = uuid::Uuid::new_v4().to_string();
    // The existing production discovery schema is used without changing authorization behavior.
    // 使用既有生产发现结构，不改变授权行为。
    let discovery = vulcan_service_api::webui_service_ready_discovery_snapshot(
        address.port(),
        std::process::id(),
        crate::current_unix_millis_text(),
        token.clone(),
    );
    // This shared discovery cannot replace any user service's discovery record.
    // 此共享发现不能替换任何用户服务的发现记录。
    let shared = Arc::new(StdMutex::new(discovery.clone()));
    test.service
        .lock()
        .expect("workbench discovery state")
        .webui_service_discovery = Some(shared.clone());
    // Record method/action and outcome only; never record request payloads, URLs with tokens or credentials.
    // 仅记录方法／动作与结果；绝不记录请求载荷、带令牌地址或凭据。
    let log = Arc::new(StdMutex::new(
        std::fs::File::create(root.join("rpc.jsonl")).expect("private RPC ledger"),
    ));
    // Production WebSocket events use the same emitter as production RPC mutations.
    // 生产 WebSocket 事件使用与生产 RPC 变更相同的发射器。
    let (events, _) = tokio::sync::broadcast::channel(32);
    // This event sink is private to the browser acceptance lifetime.
    // 此事件接收器仅属于浏览器验收生命周期。
    let emitter =
        ServiceEventEmitter::new(vec![vulcan_service_host::webui_service_event_bus_sink(
            events.clone(),
        )]);
    // The dispatcher never receives a different or globally discovered service state.
    // 分发器绝不接收另一个或全局发现的服务状态。
    let rpc_state = test.service.clone();
    // Restrict browser actions to real VMM reads, planned text saves and explicit synthetic source management.
    // 将浏览器动作限定为真实 VMM 读取、计划内文本保存及明确合成来源管理。
    let config = vulcan_service_host::WebuiServiceHttpConfig {
        discovery: shared,
        service_token: token,
        event_bus: events,
        static_root: Some(assets.to_path_buf()),
        rpc_dispatcher: Arc::new(move |method, params| {
            // Clone only lifetime-owned handles before entering the actual asynchronous dispatcher.
            // 进入实际异步分发器之前，仅克隆生命周期所有的句柄。
            let state = rpc_state.clone();
            // The actual emitter preserves normal workbench invalidation events.
            // 实际发射器保留正常工作台失效事件。
            let app = emitter.clone();
            // Ledger writes are synchronous and never retain the lock across an await.
            // 台账写入同步执行，绝不跨等待持锁。
            let ledger = log.clone();
            Box::pin(async move {
                // Parse the existing declared action request; unknown methods remain explicitly rejected.
                // 解析既有声明动作请求；未知方法保持明确拒绝。
                let action = if method == "service_run_vmm_memory_action" {
                    let request: vulcan_service_api::VmmMemoryActionRequest =
                        serde_json::from_value(
                            params
                                .as_ref()
                                .ok_or_else(|| {
                                    vulcan_service_api::WebuiServiceRpcError::InvalidParams(
                                        "Missing workbench RPC parameters".to_owned(),
                                    )
                                })?
                                .get("request")
                                .cloned()
                                .ok_or_else(|| {
                                    vulcan_service_api::WebuiServiceRpcError::InvalidParams(
                                        "Missing workbench action request".to_owned(),
                                    )
                                })?,
                        )
                        .map_err(|error| {
                            vulcan_service_api::WebuiServiceRpcError::InvalidParams(
                                error.to_string(),
                            )
                        })?;
                    Some(request.action)
                } else {
                    None
                };
                // These exact read actions were verified at their current frontend call sites.
                // 这些准确读取动作已在当前前端调用点核实。
                let allowed = match action.as_deref() {
                    Some(action) => matches!(
                        action,
                        "native_project_catalog"
                            | "native_runtime_policy"
                            | "native_memory_records"
                            | "native_session_list"
                            | "native_retention_status"
                            | "native_vector_status"
                            | "native_lexical_status"
                            | "native_recycle_bin"
                            | "native_resource_catalog"
                            | "native_resource_read"
                            | "native_text_policy_update"
                            | "load_profile_bundle"
                            | "native_session_archive_preview"
                            | "native_session_archive"
                            | "native_removal_preview"
                            | "native_removal_recycle"
                            | "native_recycle_records"
                            | "native_recycle_restore"
                            | "native_recycle_purge"
                            | "native_session_restart_preview"
                            | "native_session_restart"
                    ),
                    None => matches!(method.as_str(), "startup.snapshot" | "workbench.snapshot"),
                };
                // The guard rejects unrelated shell/configuration operations instead of fabricating successful responses.
                // 守卫拒绝无关终端／配置操作，而非伪造成功响应。
                let result = if allowed {
                    crate::rpc_dispatch::dispatch_service_rpc(app, state, method.clone(), params)
                        .await
                } else {
                    Err(vulcan_service_api::WebuiServiceRpcError::Service(format!(
                        "Private browser acceptance denied out-of-scope method: {method}"
                    )))
                };
                writeln!(ledger.lock().expect("workbench RPC ledger"), "{}", serde_json::json!({"method": method, "action": action, "allowed": allowed, "ok": result.is_ok()})).expect("write private RPC evidence");
                result
            })
        }),
        // Chat attachments are outside this first VMM browser batch and cannot access any user filesystem.
        // 聊天附件不属于本批 VMM 浏览器范围，不能访问任何用户文件系统。
        session_resource_resolver: Arc::new(|_| {
            Box::pin(async {
                Err("Private VMM browser acceptance does not expose chat resources".to_owned())
            })
        }),
        client_disconnected: Arc::new(|_| {}),
        status_provider: None,
        browser_stream: None,
        browser_download_resolver: None,
    };
    // The server remains owned and is aborted even if a later assertion fails.
    // 服务保持所有权，即使后续断言失败也会中止。
    let server = tokio::spawn(vulcan_service_host::serve_webui_service_http_listener(
        listener, config,
    ));
    // Keep the existing abort-on-drop guard alive throughout manual browser observation.
    // 在人工浏览器观察期间保持既有析构中止守卫。
    let _guard = ServerGuard(server.abort_handle());
    std::fs::write(
        root.join("discovery.json"),
        serde_json::to_vec(&discovery).expect("workbench discovery JSON"),
    )
    .expect("write private discovery");
    eprintln!("[vmm-workbench-http] ready at http://{address}");
    browser_phase(root, "browser-saved").await;
    // Independently require the actual UI input and unchanged non-text policy groups.
    // 独立要求实际界面输入及未改变的非文本策略组。
    let mut expected = initial.clone();
    expected.text.fold_machine_text = false;
    expected.text.max_fragment_tokens = 1234;
    assert_eq!(
        test.runtime
            .runtime_policy()
            .await
            .expect("browser saved policy"),
        expected
    );
    assert_eq!(
        test.runtime
            .storage_snapshot()
            .expect("browser unchanged records"),
        before
    );
    acknowledge(root, "browser-saved");
    browser_phase(root, "browser-restored").await;
    assert_eq!(
        test.runtime
            .runtime_policy()
            .await
            .expect("browser restored policy"),
        initial
    );
    assert_eq!(
        test.runtime
            .storage_snapshot()
            .expect("browser restored records"),
        before
    );
    acknowledge(root, "browser-restored");
    lifecycle::verify_selection_matrix(&test, root, &before).await;
    lifecycle::verify(&test, root, &before).await;
    // The completed lifecycle deliberately purges only the selected synthetic session's sources.
    // 完成的生命周期刻意只永久清除所选合成会话的来源。
    let final_storage = test
        .runtime
        .storage_snapshot()
        .expect("browser final storage");
    // Rebuild from both physical owners so page reload alone is not mistaken for durable persistence.
    // 从两个物理所有者重建，避免将页面刷新误认为耐久持久化。
    let reopened = NativeVmmCoreRuntime::bootstrap(test.port.clone(), test.data_plane.clone())
        .await
        .expect("browser physical rebuild");
    assert_eq!(
        reopened
            .runtime_policy()
            .await
            .expect("browser rebuilt policy"),
        initial
    );
    assert_eq!(
        reopened
            .storage_snapshot()
            .expect("browser rebuilt memories")
            .dataset
            .memories,
        final_storage.dataset.memories
    );
    assert_eq!(
        reopened
            .storage_snapshot()
            .expect("rebuilt profiles")
            .dataset
            .profile_nodes,
        final_storage.dataset.profile_nodes
    );
    acknowledge(root, "browser-complete");
    server.abort();
    assert!(
        server
            .await
            .expect_err("browser server aborted")
            .is_cancelled()
    );
}
