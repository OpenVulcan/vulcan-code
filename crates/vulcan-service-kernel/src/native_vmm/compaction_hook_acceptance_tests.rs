//! Isolated real-storage acceptance for the built-in asynchronous compaction notification.
//! 内置异步压缩通知的隔离真实存储验收。

use super::*;

/// Builds private native owners and verifies a real V1 completion dispatches the built-in memory Hook.
/// 构建私有原生归属方，并验证真实 V1 完成会分发内置记忆 Hook。
/// `data_plane` and `capability` belong to the isolated controller; only inference responses are deterministic.
/// `data_plane` 与 `capability` 属于隔离控制器；只有推理响应使用确定性结果。
/// Returns after durable boundary, pending queue, resident cache and outgoing request assertions complete.
/// 耐久边界、待处理队列、驻留缓存及传出请求断言完成后返回。
pub(in crate::native_vmm) async fn verify(
    data_plane: VmmDataPlaneHandle,
    capability: InferenceCapabilitySnapshot,
) {
    // The existing inference fixture leaves every native command and physical storage operation intact.
    // 既有推理夹具保留全部原生命令与物理存储操作。
    let port = Arc::new(CapturingAnalysisPort {
        inner: MockHostAiPort::new(),
        requests: StdMutex::new(Vec::new()),
        reject_next_turn: AtomicBool::new(false),
        fixture: AnalysisFixture::Nonempty,
        embedding_requests: StdMutex::new(Vec::new()),
        profile_instruction_pause: StdMutex::new(None),
        postaction_pause: StdMutex::new(None),
    });
    port.inner
        .set_capability_snapshot(capability)
        .expect("compaction capability");
    // This Core owner is not shared with another acceptance scenario.
    // 此 Core 归属方不与其他验收场景共享。
    let runtime = Arc::new(
        NativeVmmCoreRuntime::bootstrap(port, data_plane.clone())
            .await
            .expect("compaction runtime"),
    );
    // Disable only unrelated semantic-noise model calls while preserving normal intake and its queue.
    // 仅禁用无关语义噪音模型调用，保留正常入库及其队列。
    let mut settings = runtime
        .resource_catalog()
        .await
        .expect("compaction resources")
        .settings;
    settings.noise_semantic_enabled = false;
    runtime
        .update_resource_settings(settings)
        .await
        .expect("compaction noise settings");
    // Bind the same explicit host project used by the real workbench fixture.
    // 绑定真实工作台夹具使用的同一明确宿主项目。
    let project = data_plane
        .create_project("Compaction Hook", "compaction-hook")
        .await
        .expect("compaction project");
    data_plane
        .bind_vulcan_project("project-a", project.id)
        .await
        .expect("compaction project binding");
    // Share the actual operation barrier between the client and maintenance owner.
    // 在客户端与维护归属方之间共享实际操作屏障。
    let gate = Arc::new(RwLock::new(()));
    // Keep one client alive through the real background Hook; no perpetual analysis worker is spawned.
    // 在真实后台 Hook 期间保留唯一客户端；不启动永久提炼工作器。
    let client = Arc::new(NativeVmmClient::new(
        runtime.clone(),
        data_plane.clone(),
        gate.clone(),
        Arc::new(AtomicBool::new(true)),
        Arc::new(Notify::new()),
    ));
    // Seed both memory and profile prerequisites through normal intake and the actual bounded worker.
    // 通过正常入库与实际有界工作器建立记忆和画像前置条件。
    client
        .post_action(PostActionRequest {
            host_project_id: "project-a".to_owned(),
            project_id: project.id,
            user_id: 1,
            session_id: "compaction-memory-source".to_owned(),
            operation_id: "compaction-memory-source-operation".to_owned(),
            user_content: "我喜欢西瓜和葡萄。这个项目确定使用 PostgreSQL 作为数据库，请长期记住。"
                .to_owned(),
            assistant_content: "了解你的水果偏好以及项目数据库决定。".to_owned(),
            ..Default::default()
        })
        .await
        .expect("compaction fixture intake");
    assert!(
        client
            .drain_due_pending_analysis_jobs()
            .await
            .expect("compaction fixture analysis")
    );
    // Require durable completion before the separate compaction session admits its still-pending turn.
    // 在独立压缩会话准入仍待处理回合之前，要求夹具提炼已持久完成。
    let job = data_plane
        .load_pending_analysis_job("compaction-memory-source-operation")
        .await
        .expect("compaction fixture job")
        .expect("compaction fixture audit row");
    assert_eq!(
        job.state,
        vulcan_workspace_store::vmm_data_plane::VMM_PENDING_ANALYSIS_COMPLETED,
        "{:?}",
        job.last_error
    );
    // This snapshot proves that the shared binding assertions receive real extracted profile nodes.
    // 此快照证明共享绑定断言取得真实提炼的画像节点。
    let extracted = runtime
        .storage_snapshot()
        .expect("compaction fixture snapshot");
    assert_eq!(extracted.dataset.memories.len(), 1);
    assert_eq!(extracted.dataset.profile_nodes.len(), 3);
    // Obtain the existing acceptance input from real lexical/vector retrieval.
    // 从真实全文及向量检索取得既有验收输入。
    let search = client
        .search_memory_events(SearchMemoryEventsRequest {
            host_project_id: "project-a".to_owned(),
            project_id: project.id,
            user_id: 1,
            runtime_session_id: None,
            queries: vec!["PostgreSQL".to_owned()],
            top_k: 4,
        })
        .await
        .expect("compaction memory search");
    assert_eq!(search.results.len(), 1);
    assert_eq!(search.results[0].hits.len(), 1);
    assert_eq!(
        search.results[0].hits[0].memory_id,
        crate::native_vmm::client::parse_core_numeric_id(
            &extracted.dataset.memories[0].id,
            "vmm-memory-",
        )
        .expect("native memory ID")
    );
    // The installed Hook and the source client use this same Core and data plane.
    // 已安装 Hook 与源客户端使用此同一 Core 和数据面。
    let maintenance =
        crate::native_vmm::NativeVmmMaintenance::new(runtime.clone(), data_plane.clone(), gate);
    // Select the explicitly installed-Hook variant without changing the original cache-only scenario.
    // 选择明确安装 Hook 的变体，不改变原始仅缓存场景。
    let hook = compaction_acceptance::NativeHookAcceptance {
        client: client.clone(),
        core: runtime,
    };
    compaction_acceptance::verify(
        &client,
        &data_plane,
        &maintenance,
        project.id,
        &search.results[0].hits[0],
        Some(&hook),
    )
    .await;
}
