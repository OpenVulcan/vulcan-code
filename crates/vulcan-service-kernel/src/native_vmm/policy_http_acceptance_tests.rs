//! Explicit live HTTP acceptance for real policy components against private native storage.
//! 真实策略组件针对私有原生存储的显式 HTTP 联网验收。

use super::*;
#[path = "workbench_http_acceptance_tests.rs"]
pub(in crate::native_vmm) mod workbench;
use crate::ServiceEventEmitter;
use std::path::Path;
use vulcan_vmm::SearchMemoryEventsRequest;

#[path = "policy_http_text_acceptance_tests.rs"]
mod text_policy;

#[path = "policy_http_recall_acceptance_tests.rs"]
pub(in crate::native_vmm) mod recall;

#[path = "policy_http_profile_acceptance_tests.rs"]
pub(in crate::native_vmm) mod profiles;

#[path = "policy_http_retention_acceptance_tests.rs"]
mod retention;

/// Aborts this private server even when a phase assertion panics.
/// 即使阶段断言失败，也终止本私有服务器。
struct ServerGuard(tokio::task::AbortHandle);

impl Drop for ServerGuard {
    /// Releases the owned task on scope exit; takes no external target and returns nothing.
    /// 离开作用域时释放持有任务；不接收外部目标，无返回值。
    fn drop(&mut self) {
        self.0.abort();
    }
}

/// Waits for one explicitly named frontend phase within a finite private-host lifetime.
/// 在有限私有宿主生命周期内等待一个明确命名的前端阶段。
/// Takes the isolated handshake directory and phase; abort or expiry fails instead of waiting forever.
/// 接收隔离握手目录及阶段；中止或到期即失败，不无限等待。
async fn wait_phase(root: &Path, phase: &str) {
    // Only this test's declared marker protocol is observed; no user-service or default path is searched.
    // 仅观察本测试声明的标记协议；不搜索用户服务或默认路径。
    let marker = root.join(format!("{phase}.ready"));
    // A generous but bounded phase deadline permits compilation of the frontend runner.
    // 宽裕但有界的阶段期限允许前端运行器编译。
    let deadline = tokio::time::Instant::now() + Duration::from_secs(180);
    while !marker.exists() {
        assert!(
            !root.join("abort.ready").exists(),
            "frontend aborted phase {phase}"
        );
        assert!(
            tokio::time::Instant::now() < deadline,
            "frontend phase expired: {phase}"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// Acknowledges only a phase whose independent Rust assertions have already passed.
/// 仅确认独立 Rust 断言已经通过的阶段。
/// Takes a private directory and exact phase name; writes no policy or user data.
/// 接收私有目录及准确阶段名；不写策略或用户数据。
fn acknowledge(root: &Path, phase: &str) {
    std::fs::write(root.join(format!("{phase}.ack")), b"verified")
        .expect("private phase acknowledgement");
}

/// Constructs the independently declared final global values without consuming frontend output.
/// 构造独立声明的最终全局值，不消费前端输出。
/// Takes no input and returns the exact whole group expected after real component editing.
/// 不接收输入，返回真实组件编辑后的准确整组预期。
fn expected_global() -> VmmNativeRetrievalPolicySummary {
    VmmNativeRetrievalPolicySummary {
        hybrid_enabled: false,
        lexical_candidate_floor: 3,
        rrf_k: "9223372036854775807".to_owned(),
        replace_min_similarity: 0.86,
        hard_dedupe_cosine_threshold: 0.0,
        hard_dedupe_pool_top_k: 3,
        replacement_top_k: 2,
        rerank_top_n: 0,
        mmr_enabled: false,
        mmr_lambda: 0.25,
        weibull_enabled: false,
        weibull_shape: 2.0,
        weibull_scale_hours: 0.5,
        weibull_min_multiplier: 0.25,
        weibull_reinforce_weight: 0.5,
        weibull_cross_session_boost: 0.75,
    }
}

/// Constructs a different explicit host group for live isolation and inheritance assertions.
/// 构造不同的明确宿主组，用于联网隔离与继承断言。
/// Takes no input and returns this test's independently declared whole host group.
/// 不接收输入，返回本测试独立声明的宿主整组值。
fn expected_host() -> VmmNativeRetrievalPolicySummary {
    VmmNativeRetrievalPolicySummary {
        hybrid_enabled: true,
        lexical_candidate_floor: 1,
        rrf_k: "1".to_owned(),
        replace_min_similarity: 1.0,
        hard_dedupe_cosine_threshold: 0.55,
        hard_dedupe_pool_top_k: 4,
        replacement_top_k: 1,
        rerank_top_n: 2,
        mmr_enabled: true,
        mmr_lambda: 0.0,
        weibull_enabled: true,
        weibull_shape: 0.75,
        weibull_scale_hours: 0.125,
        weibull_min_multiplier: 0.6,
        weibull_reinforce_weight: 0.0,
        weibull_cross_session_boost: 0.0,
    }
}

/// Hosts real component requests, verifies persisted phases and executes actual native operations.
/// 承载真实组件请求，验证持久阶段并执行实际原生操作。
/// Takes private storage, an inference capability and an existing empty absolute handshake directory.
/// 接收私有存储、推理能力和既有空绝对握手目录。
pub(in crate::native_vmm) async fn verify(
    data_plane: VmmDataPlaneHandle,
    capability: InferenceCapabilitySnapshot,
    root: &Path,
) {
    assert!(root.is_absolute() && root.is_dir());
    assert_eq!(
        std::fs::read_dir(root)
            .expect("private handshake directory")
            .count(),
        0
    );
    // The port controls inference only; HTTP routing, service dispatch and storage are production code.
    // 端口仅控制推理；HTTP 路由、服务分发及存储均为生产代码。
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
        .expect("HTTP inference capability");
    // Each test owns its real native runtime, separate from any running user service.
    // 每次测试持有真实原生运行时，与运行中的用户服务隔离。
    let runtime = Arc::new(
        NativeVmmCoreRuntime::bootstrap(port.clone(), data_plane.clone())
            .await
            .expect("HTTP native runtime"),
    );
    // Preserve all unrelated runtime groups for later exact invariance assertions.
    // 保留全部无关运行时组，以便后续准确不变量断言。
    let initial = runtime.runtime_policy().await.expect("HTTP initial policy");
    // The service and public client use the same production operation gate.
    // 服务与公开客户端使用同一个生产操作锁。
    let gate = Arc::new(RwLock::new(()));
    // Reuse the known real direct-write environment; no browser response is supplied by this fixture.
    // 复用已知真实主动写入环境；本夹具不提供任何浏览器响应。
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
    // Seed under isolated transforms, then restore real defaults before the frontend opens its form.
    // 在隔离变换下创建种子，然后在前端打开表单前恢复真实默认值。
    let global = expected_global();
    // Three normal records provide an independently verifiable replacement window after the UI save.
    // 三条正常记录提供界面保存后可独立验证的替代窗口。
    let seeds = test.seeds("policy-http", &global).await;
    test.save_global(&initial.retrieval).await;
    // Bind only a fresh loopback port; never use the application's discovery port range.
    // 仅绑定新的回环端口；绝不使用应用发现端口范围。
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("private HTTP listener");
    // This address is authoritative because it came from the bound listener.
    // 此地址来自已绑定监听器，因此具有权威性。
    let address = listener.local_addr().expect("private HTTP address");
    // The capability stays in the private handshake file and is never printed.
    // 能力令牌仅保存在私有握手文件中，绝不打印。
    let token = uuid::Uuid::new_v4().to_string();
    // Health discovery uses the production response schema and the actual ephemeral port.
    // 健康发现使用生产响应结构和实际临时端口。
    let discovery = vulcan_service_api::webui_service_ready_discovery_snapshot(
        address.port(),
        std::process::id(),
        crate::current_unix_millis_text(),
        token.clone(),
    );
    // The shared discovery belongs only to this private bridge.
    // 共享发现仅属于此私有桥。
    let shared = Arc::new(StdMutex::new(discovery.clone()));
    test.service
        .lock()
        .expect("HTTP bridge discovery")
        .webui_service_discovery = Some(shared.clone());
    // Route events through the same host event sink used by the actual WebUI service.
    // 通过实际 WebUI 服务所用的同一宿主事件接收器路由事件。
    let (events, _) = tokio::sync::broadcast::channel(32);
    // Keep production side effects observable without opening a user's workbench.
    // 保留生产副作用的可观察性，不打开用户工作台。
    let emitter =
        ServiceEventEmitter::new(vec![vulcan_service_host::webui_service_event_bus_sink(
            events.clone(),
        )]);
    // Dispatcher and resource resolver retain the exact private state identity.
    // 分发器及资源解析器保留准确私有状态身份。
    let rpc_state = test.service.clone();
    // Resource requests are still handled by the production resolver, not a synthetic endpoint.
    // 资源请求仍由生产解析器处理，不使用合成端点。
    let resource_state = test.service.clone();
    // All routes and authorization remain supplied by the existing host library.
    // 全部路由及授权仍由既有宿主库提供。
    let config = vulcan_service_host::WebuiServiceHttpConfig {
        discovery: shared,
        service_token: token,
        event_bus: events,
        static_root: None,
        rpc_dispatcher: Arc::new(move |method, params| {
            Box::pin(crate::rpc_dispatch::dispatch_service_rpc(
                emitter.clone(),
                rpc_state.clone(),
                method,
                params,
            ))
        }),
        session_resource_resolver: Arc::new(move |request| {
            Box::pin(
                crate::conversation_resources_v1::service_session_resource_content_v1(
                    resource_state.clone(),
                    request,
                ),
            )
        }),
        client_disconnected: Arc::new(|client_id| {
            crate::cancel_command_session_displays_for_client(client_id);
            crate::cancel_project_terminal_displays_for_client(client_id);
        }),
    };
    // The owned task is aborted on both success and assertion failure.
    // 持有任务在成功及断言失败时均被终止。
    let server = tokio::spawn(vulcan_service_host::serve_webui_service_http_listener(
        listener, config,
    ));
    // Guard lifetime encloses every frontend phase.
    // 守卫生命周期覆盖每个前端阶段。
    let _guard = ServerGuard(server.abort_handle());
    std::fs::write(
        root.join("discovery.json"),
        serde_json::to_vec(&discovery).expect("private discovery JSON"),
    )
    .expect("private discovery file");
    eprintln!("[vmm-policy-http] ready at http://{address}");
    wait_phase(root, "global").await;
    assert_eq!(
        test.runtime
            .runtime_policy()
            .await
            .expect("HTTP saved global")
            .retrieval,
        global
    );
    // The UI-saved floor and window now drive real candidate replacement on the original physical records.
    // 界面保存的门槛和窗口现在驱动原始物理记录上的真实候选替代。
    test.probe(
        &seeds,
        "replacement-host-b",
        "network-global",
        &[0, 1],
        None,
    )
    .await;
    acknowledge(root, "global");
    wait_phase(root, "host").await;
    // Independently compute this host's exact expected group without reading the frontend phase marker.
    // 独立计算此宿主准确预期组，不从前端阶段标记读取。
    let host = expected_host();
    // Read actual saved overrides and effective runtime groups for both callers.
    // 读取两调用方的实际已保存覆盖及有效运行时组。
    let a = test
        .runtime
        .host_project_policy("replacement-host-a")
        .await
        .expect("HTTP A policy");
    assert_eq!(a.overrides.retrieval, Some(host.clone()));
    assert_eq!(a.effective.retrieval, host);
    assert_eq!(
        test.runtime
            .host_project_policy("replacement-host-b")
            .await
            .expect("HTTP B policy")
            .effective
            .retrieval,
        global
    );
    assert_eq!(
        test.runtime
            .runtime_policy()
            .await
            .expect("HTTP unchanged global")
            .retrieval,
        global
    );
    acknowledge(root, "host");
    wait_phase(root, "inherit").await;
    assert!(
        test.runtime
            .host_project_policy("replacement-host-a")
            .await
            .expect("HTTP A inherits")
            .overrides
            .retrieval
            .is_none()
    );
    assert_eq!(
        test.runtime
            .host_project_policy("replacement-host-a")
            .await
            .expect("HTTP inherited effective")
            .effective
            .retrieval,
        global
    );
    // A's next actual public search must use restored global transforms and see only active records.
    // A 下一次实际公开搜索必须使用恢复的全局变换，并且仅看到活动记录。
    let results = test
        .client
        .search_memory_events(SearchMemoryEventsRequest {
            host_project_id: "replacement-host-a".to_owned(),
            project_id: seeds.project_id,
            user_id: 1,
            runtime_session_id: None,
            queries: vec![format!("{PROBE} network-search")],
            top_k: 2,
        })
        .await
        .expect("HTTP inherited public search");
    assert_eq!(results.results.len(), 1);
    assert_eq!(results.results[0].hits.len(), 2);
    assert_eq!(
        results.results[0].hits[0].abstract_text,
        format!("{PROBE} network-global")
    );
    assert_eq!(
        format!("vmm-memory-{}", results.results[0].hits[1].memory_id),
        seeds.ids[2].as_str()
    );
    for hit in &results.results[0].hits {
        assert_eq!(
            hit.retrieval.as_ref().expect("HTTP search origin").origin,
            "vector_search"
        );
    }
    acknowledge(root, "inherit");
    text_policy::verify(&test, root).await;
    recall::verify(&test, root).await;
    // Profile overrides are owned by stable memory-project IDs, not host policy tags.
    // 画像覆盖由稳定记忆项目 ID 持有，而非宿主策略标签。
    let profile_project = profiles::verify(&test, root).await;
    // Cleanup policy ownership is independent of both host and profile policy overrides.
    // 清洗策略归属独立于宿主及画像策略覆盖。
    let retention_project = retention::verify(&test, root).await;
    wait_phase(root, "finish").await;
    // Unrelated groups and full records must survive both the live UI sequence and physical rebuild.
    // 无关配置组及完整记录必须在联网界面序列和物理重建后保持。
    let final_policy = test
        .runtime
        .runtime_policy()
        .await
        .expect("HTTP final policy");
    assert_eq!(final_policy.recall, recall::window());
    assert_eq!(final_policy.text, text_policy::folded());
    assert_eq!(final_policy.profiles, profiles::global(true));
    assert_eq!(final_policy.retention, retention::global(2));
    // Relational state and vectors are reloaded from their actual physical owners.
    // 关系状态及向量从实际物理持有者重载。
    let before = test
        .runtime
        .storage_snapshot()
        .expect("HTTP before rebuild")
        .dataset
        .memories;
    // This independent runtime cannot reuse the prior in-memory policy or record snapshots.
    // 此独立运行时不能复用先前内存策略或记录快照。
    let reopened = NativeVmmCoreRuntime::bootstrap(test.port.clone(), test.data_plane.clone())
        .await
        .expect("HTTP rebuild");
    assert_eq!(
        reopened
            .storage_snapshot()
            .expect("HTTP rebuilt turns")
            .dataset
            .turns,
        test.runtime
            .storage_snapshot()
            .expect("HTTP original turns")
            .dataset
            .turns,
    );
    assert_eq!(
        reopened
            .storage_snapshot()
            .expect("HTTP rebuilt records")
            .dataset
            .memories,
        before
    );
    assert_eq!(
        reopened
            .runtime_policy()
            .await
            .expect("HTTP rebuilt policy"),
        final_policy
    );
    assert!(
        reopened
            .host_project_policy("replacement-host-a")
            .await
            .expect("HTTP rebuilt inheritance")
            .overrides
            .retrieval
            .is_none()
    );
    // Physical reload must preserve text inheritance itself, not only the global document.
    // 物理重载必须保留文本继承本身，而不仅是全局文档。
    let inherited = reopened
        .host_project_policy("replacement-host-a")
        .await
        .expect("HTTP rebuilt text inheritance");
    assert!(inherited.overrides.text.is_none());
    assert_eq!(inherited.effective.text, text_policy::folded());
    assert!(inherited.overrides.recall.is_none());
    assert_eq!(inherited.effective.recall, recall::window());
    assert_eq!(
        reopened
            .storage_snapshot()
            .expect("HTTP rebuilt profiles")
            .dataset
            .profile_nodes,
        test.runtime
            .storage_snapshot()
            .expect("HTTP original profiles")
            .dataset
            .profile_nodes
    );
    // Rebuilding must retain removal of the project override, not only the effective global values.
    // 重建必须保留项目覆盖已移除的事实，而不仅是有效全局值。
    let inherited_profiles = reopened
        .project_profile_policy_summary(profile_project)
        .await
        .expect("HTTP rebuilt project profiles");
    assert!(inherited_profiles.overrides.is_none());
    assert_eq!(inherited_profiles.effective, profiles::global(true).project);
    // Both the exact owner inheritance and complete vector-bearing recycle batches survive physical reload.
    // 准确归属方继承及完整带向量回收批次均在物理重载后保留。
    let inherited_retention = reopened
        .project_retention_summary(retention_project)
        .await
        .expect("HTTP rebuilt retention");
    assert!(inherited_retention.overrides.is_none());
    assert_eq!(inherited_retention.effective, inherited_retention.global);
    assert_eq!(
        reopened
            .storage_snapshot()
            .expect("HTTP rebuilt trash")
            .retention_trash_batches,
        test.runtime
            .storage_snapshot()
            .expect("HTTP original trash")
            .retention_trash_batches
    );
    acknowledge(root, "finish");
    server.abort();
    assert!(
        server
            .await
            .expect_err("private HTTP server aborted")
            .is_cancelled()
    );
}
