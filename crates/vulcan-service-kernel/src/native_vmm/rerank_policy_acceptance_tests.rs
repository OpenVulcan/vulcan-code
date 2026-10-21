//! Formal retrieval-policy saves observed through actual search and the validating rerank port.
//! 通过真实搜索与执行校验的重排端口观察正式检索策略保存。

use super::*;
use vulcan_service_api::{VmmMemoryActionResponse, VmmNativeRetrievalPolicySummary};
use vulcan_vmm::{MemorySearchHit, SearchMemoryEventsRequest};

/// Owns requests and one optional pause for this isolated rerank acceptance only.
/// 仅为本隔离重排验收持有请求和一次可选暂停。
pub(super) struct Capture {
    /// Exact ordered requests emitted by the production driver.
    /// 生产驱动发出的准确有序请求。
    pub(super) requests: StdMutex<Vec<RerankRequest>>,
    /// Pauses an already admitted request while its saved policy changes.
    /// 在保存策略变化时暂停已准入请求。
    pub(super) pause: StdMutex<Option<precheck_policy::IntentPause>>,
}

/// Reverses the supplied candidate prefix with valid descending scores and exact request identities.
/// 使用有效降序分值与准确请求身份反转传入候选前缀。
/// Takes the actual host request and returns only inference output, leaving search and storage unchanged.
/// 接收真实宿主请求，仅返回推理输出，保持搜索与存储不变。
pub(super) fn response(request: &RerankRequest) -> RerankResponse {
    RerankResponse {
        request_id: request.request_id.clone(),
        route: ExecutionRouteSnapshot {
            provider_id: "fixture".to_owned(),
            service_entry_id: "rerank".to_owned(),
            connection_id: "fixture".to_owned(),
            model_id: "fixture".to_owned(),
            protocol_binding_id: None,
            action_binding_id: Some("rerank".to_owned()),
            secondary_route_id: None,
            secondary_provider_name: None,
            secondary_endpoint_name: None,
            capability_revision: "fixture-v1".to_owned(),
            catalog_revision: "fixture-v1".to_owned(),
            route_revision: "fixture-v1".to_owned(),
            usage_scope: InferenceUsageScope::InternalBackground,
        },
        fingerprint: RerankProfileFingerprint {
            provider_id: "fixture".to_owned(),
            service_entry_id: "rerank".to_owned(),
            deployment_id: None,
            secondary_route_id: None,
            model_id: "fixture".to_owned(),
            binding_id: "rerank".to_owned(),
            capability_revision: "fixture-v1".to_owned(),
            score_semantics: RerankScoreSemantics::HigherIsMoreRelevant,
        },
        results: request
            .candidates
            .iter()
            .enumerate()
            .rev()
            .enumerate()
            .map(|(rank, (index, candidate))| RerankResult {
                candidate_id: candidate.id.clone(),
                original_index: u32::try_from(index).expect("small fixture index"),
                rank: u32::try_from(rank).expect("small fixture rank"),
                score: 0.95 - f64::from(u32::try_from(rank).expect("small fixture score")) * 0.1,
                content: None,
            })
            .collect(),
        usage: RequestUsage::default(),
        duration_ms: 0,
    }
}

/// Saves the full retrieval group globally or for host A, validating exact values and ownership.
/// 将完整检索组保存为全局或宿主 A 覆盖，并校验准确值与归属。
/// Takes service, expected effective group and optional host override; returns after the real dispatcher succeeds.
/// 接收服务、预期有效组及可选宿主覆盖；真实派发器成功后返回。
async fn save(
    service: &crate::state::SharedServiceBridgeState,
    effective: &VmmNativeRetrievalPolicySummary,
    host_override: Option<Option<VmmNativeRetrievalPolicySummary>>,
) {
    // The outer option chooses the declared endpoint; the inner option is the product's inherit operation.
    // 外层可选值选择明确端点；内层可选值表示产品的恢复继承操作。
    if let Some(retrieval) = host_override {
        // Other groups remain inherited and are never copied into this host document.
        // 其他组保持继承，绝不复制到此宿主文档。
        let overrides = VmmNativeHostProjectPolicyOverrides {
            retrieval,
            ..Default::default()
        };
        // Inspect the actual service response, not a reconstructed effective policy.
        // 检查真实服务响应，而不是重建的有效策略。
        let saved = policy_service::action(
            service,
            "native_host_project_policy_update",
            serde_json::json!({"hostProjectId":"rerank-host-a", "overrides":overrides}),
        )
        .await
        .expect("save rerank host");
        let VmmMemoryActionResponse::NativeHostProjectPolicy(saved) = saved else {
            panic!("expected host policy response");
        };
        assert_eq!(saved.host_project_id, "rerank-host-a");
        assert_eq!(saved.overrides, overrides);
        assert_eq!(saved.effective.retrieval, *effective);
    } else {
        // Global saves use the same action as the native settings page.
        // 全局保存使用与原生设置页相同的动作。
        let saved = policy_service::action(
            service,
            "native_retrieval_policy_update",
            serde_json::to_value(effective).expect("rerank policy JSON"),
        )
        .await
        .expect("save global rerank");
        let VmmMemoryActionResponse::NativeRuntimePolicy(saved) = saved else {
            panic!("expected global policy response");
        };
        assert_eq!(saved.retrieval, *effective);
    }
}

/// Searches the shared project through the public client for one explicit host identity.
/// 通过公开客户端以明确宿主身份搜索共享项目。
/// Takes client, project and host; returns the exact nonempty result group after protocol assertions.
/// 接收客户端、项目及宿主；协议断言后返回准确非空结果组。
async fn search(client: &NativeVmmClient, project_id: u64, host: &str) -> Vec<MemorySearchHit> {
    // No runtime session means the declared management-search path, not an invented session fallback.
    // 无运行时会话表示明确的管理搜索路径，而不是虚构会话兜底。
    let result = client
        .search_memory_events(SearchMemoryEventsRequest {
            runtime_session_id: None,
            user_id: 1,
            project_id,
            host_project_id: host.to_owned(),
            queries: vec!["PolicyNeedle".to_owned()],
            top_k: 3,
            min_score: None,
        })
        .await
        .expect("actual rerank search");
    assert_eq!(result.results.len(), 1);
    assert_eq!(result.results[0].query_index, 0);
    assert_eq!(result.results[0].query, "PolicyNeedle");
    assert_eq!(result.results[0].hits.len(), 3);
    result
        .results
        .into_iter()
        .next()
        .expect("single search group")
        .hits
}

/// Checks actual rerank calls, candidate content, prefix reversal, untouched tail and public provenance.
/// 检查真实重排调用、候选内容、前缀反转、未修改尾部及公开来源。
/// Takes fixture and baseline search results plus the admitted count; returns after independent output assertions.
/// 接收夹具、基线搜索结果及已准入数量；独立输出断言后返回。
async fn check(
    client: &NativeVmmClient,
    capture: &Capture,
    project_id: u64,
    host: &str,
    baseline: &[MemorySearchHit],
    count: usize,
) {
    // Record the boundary before issuing the production command.
    // 在发出生产命令前记录边界。
    let before = capture.requests.lock().expect("capture start").len();
    // Observe returned memory IDs independently of the inference response builder.
    // 独立于推理响应构建器观察返回记忆 ID。
    let hits = search(client, project_id, host).await;
    // Counts zero and one must skip the optional provider stage entirely.
    // 数量零和一必须完全跳过可选供应商阶段。
    let active = count > 1;
    // Production caps the provider prefix by the number of materialized candidates.
    // 生产按已物化候选数量限制供应商前缀。
    let prefix = count.min(baseline.len());
    // An independently constructed ID order proves prefix-only application and tail preservation.
    // 独立构建的 ID 顺序证明仅应用前缀并保留尾部。
    let mut expected = baseline.iter().map(|hit| hit.memory_id).collect::<Vec<_>>();
    if active {
        expected[..prefix].reverse();
    }
    assert_eq!(
        hits.iter().map(|hit| hit.memory_id).collect::<Vec<_>>(),
        expected
    );
    for (index, hit) in hits.iter().enumerate() {
        // Public provenance must agree with actual participation, not merely the enabled setting.
        // 公开来源必须符合实际参与情况，而不只是设置启用情况。
        let retrieval = hit.retrieval.as_ref().expect("public retrieval provenance");
        assert_eq!(
            retrieval.origin,
            if active && index < prefix {
                "vector_rerank"
            } else {
                "vector_search"
            }
        );
        assert!(retrieval.context_observed);
        assert_eq!(retrieval.support_count, 0);
        assert_eq!(retrieval.rebuttal_count, 0);
        assert!(retrieval.matched_context_values.is_empty());
        assert_eq!(retrieval.matched_context_support_count, 0);
        assert_eq!(retrieval.matched_context_rebuttal_count, 0);
        assert_eq!(retrieval.matched_context_score_delta, Some(0.0));
        if active && index < prefix {
            assert!((retrieval.final_score - (0.95 - index as f64 * 0.1)).abs() < 1e-9);
        }
    }
    // Own the completed request list without holding a synchronous guard over any future await.
    // 独立持有已完成请求列表，不跨后续等待持有同步守卫。
    let requests = capture.requests.lock().expect("capture end")[before..].to_vec();
    assert_eq!(requests.len(), usize::from(active));
    if active {
        // Exact source text is checked against real baseline hits, not against guessed candidate IDs.
        // 对照真实基线命中检查准确来源文本，而不是猜测候选 ID。
        let request = &requests[0];
        assert_eq!(request.query.text, "PolicyNeedle");
        assert_eq!(request.top_n as usize, prefix);
        assert_eq!(request.candidates.len(), prefix);
        assert!(!request.return_content);
        for (candidate, hit) in request.candidates.iter().zip(baseline) {
            assert_eq!(
                candidate.text,
                format!("{}\n{}", hit.abstract_text, hit.details_preview)
            );
        }
    }
}

/// Verifies saved rerank limits, host isolation, inheritance and immutable admitted search on real storage.
/// 在真实存储上验证已保存重排限制、宿主隔离、继承及不可变已准入搜索。
/// Takes a private data plane and declared capability; only inference results are controlled.
/// 接收私有数据面与声明能力；仅控制推理结果。
pub(in crate::native_vmm) async fn verify(
    data_plane: VmmDataPlaneHandle,
    capability: InferenceCapabilitySnapshot,
) {
    // Keep capture state exclusive to this test mode, without extending unrelated fixture constructors.
    // 捕获状态仅属于此测试模式，不扩展无关夹具构造器。
    let port = Arc::new(CapturingAnalysisPort {
        inner: MockHostAiPort::new(),
        requests: StdMutex::new(Vec::new()),
        reject_next_turn: AtomicBool::new(false),
        fixture: AnalysisFixture::RerankPolicy(Capture {
            requests: StdMutex::new(Vec::new()),
            pause: StdMutex::new(None),
        }),
        embedding_requests: StdMutex::new(Vec::new()),
        profile_instruction_pause: StdMutex::new(None),
        postaction_pause: StdMutex::new(None),
    });
    port.inner
        .set_capability_snapshot(capability)
        .expect("rerank capability");
    // Bootstrap real native storage and production policy ownership.
    // 引导真实原生存储与生产策略归属。
    let runtime = Arc::new(
        NativeVmmCoreRuntime::bootstrap(port.clone(), data_plane.clone())
            .await
            .expect("rerank runtime"),
    );
    // Both hosts share identical searchable memories; only their policy differs.
    // 两宿主共享相同可搜索记忆；只有策略不同。
    let project = data_plane
        .create_project("Rerank policy", "rerank-policy")
        .await
        .expect("rerank project");
    for host in ["rerank-host-a", "rerank-host-b"] {
        data_plane
            .bind_vulcan_project(host, project.id)
            .await
            .expect("rerank binding");
    }
    // Share the operation gate between real service saves and public searches.
    // 在真实服务保存与公开搜索间共享操作锁。
    let gate = Arc::new(RwLock::new(()));
    // Service and client consume the same runtime instance.
    // 服务与客户端消费同一运行时实例。
    let service = policy_service::service(runtime.clone(), data_plane.clone(), gate.clone());
    // Disable other optional ranking stages explicitly to isolate this field's actual business effect.
    // 明确关闭其他可选排序阶段，以隔离本字段实际业务效果。
    let mut global = runtime
        .runtime_policy()
        .await
        .expect("rerank defaults")
        .retrieval;
    global.hybrid_enabled = false;
    global.rerank_top_n = 0;
    global.mmr_enabled = false;
    global.weibull_enabled = false;
    save(&service, &global, None).await;
    // All source records enter through normal public atomic writes, including real dedupe and review.
    // 所有来源记录通过普通公开原子写入进入，包含真实去重与审核。
    let client = NativeVmmClient::new(
        runtime.clone(),
        data_plane.clone(),
        gate,
        Arc::new(AtomicBool::new(true)),
        Arc::new(Notify::new()),
    );
    // Reuse the established distinct-vector marker contract; no physical record is inserted manually.
    // 复用既有不同向量标记契约；不手工插入任何物理记录。
    let written = client
        .write_memories(WriteMemoriesRequest {
            host_project_id: "rerank-host-b".to_owned(),
            project_id: project.id,
            user_id: 1,
            session_id: "rerank-seed".to_owned(),
            items: ["fixture-alpha", "fixture-beta", "fixture-gamma"]
                .iter()
                .map(|marker| vulcan_vmm::WriteMemoryItem {
                    scope_level: 2,
                    category: 0,
                    priority: 2,
                    memory_level: 3,
                    abstract_text: format!("PolicyNeedle database {marker}"),
                    details: format!("The database rule for {marker} is explicitly confirmed."),
                })
                .collect(),
        })
        .await
        .expect("normal rerank seeds");
    assert_eq!(written.items.len(), 3);
    assert!(written.items.iter().all(|item| !item.deduped));
    // Preserve all relational memory facts to prove searches and policy saves do not rewrite sources.
    // 保留全部关系记忆事实，证明搜索与策略保存不改写来源。
    let original = runtime
        .storage_snapshot()
        .expect("original rerank snapshot")
        .dataset
        .memories;
    // Disk snapshots intentionally omit embeddings, so compare each representation with its own baseline.
    // 磁盘快照有意省略向量，因此分别对照各自表示的基线。
    let original_disk: vulcan_memorymesh_core::StorageSnapshot = serde_json::from_str(
        &data_plane
            .load_core_snapshot_json()
            .await
            .expect("seed disk read")
            .expect("seed disk snapshot"),
    )
    .expect("seed disk JSON");
    // This mode is explicit and cannot silently use another fixture's request collection.
    // 此模式明确，不会静默使用另一夹具的请求集合。
    let AnalysisFixture::RerankPolicy(capture) = &port.fixture else {
        panic!("rerank fixture required");
    };
    // Physical retrieval supplies the authoritative baseline order and content.
    // 物理检索提供权威基线顺序及内容。
    let baseline = search(&client, project.id, "rerank-host-b").await;
    assert!(
        capture
            .requests
            .lock()
            .expect("disabled baseline")
            .is_empty()
    );
    for count in [0, 1, 2, 3, 5] {
        global.rerank_top_n = count;
        save(&service, &global, None).await;
        check(
            &client,
            capture,
            project.id,
            "rerank-host-b",
            &baseline,
            count as usize,
        )
        .await;
    }
    // An explicit two-candidate host group survives an unrelated global transition to three.
    // 显式双候选宿主组在全局变更为三条时保持不变。
    let mut override_a = global.clone();
    override_a.rerank_top_n = 2;
    save(&service, &override_a, Some(Some(override_a.clone()))).await;
    global.rerank_top_n = 3;
    save(&service, &global, None).await;
    check(&client, capture, project.id, "rerank-host-a", &baseline, 2).await;
    check(&client, capture, project.id, "rerank-host-b", &baseline, 3).await;
    save(&service, &global, Some(None)).await;
    check(&client, capture, project.id, "rerank-host-a", &baseline, 3).await;
    // Pause an actual rerank request after admission, then save the next-command disabled setting.
    // 在准入后暂停真实重排请求，再保存下一命令使用的关闭设置。
    let (entered, observed) = tokio::sync::oneshot::channel();
    // The resume sender belongs only to the concurrent formal-save branch.
    // 恢复发送方仅属于并发正式保存分支。
    let (resume, resumed) = tokio::sync::oneshot::channel();
    *capture.pause.lock().expect("arm rerank pause") = Some(precheck_policy::IntentPause {
        entered,
        resume: resumed,
    });
    global.rerank_top_n = 0;
    tokio::join!(
        check(&client, capture, project.id, "rerank-host-b", &baseline, 3),
        async {
            tokio::time::timeout(Duration::from_secs(10), observed)
                .await
                .expect("bounded rerank admission")
                .expect("rerank reached");
            save(&service, &global, None).await;
            resume.send(()).expect("resume admitted rerank");
        }
    );
    check(&client, capture, project.id, "rerank-host-b", &baseline, 0).await;
    // Read actual durable state, not just the in-memory response projection.
    // 读取真实持久状态，而不只是内存响应投影。
    let persisted: vulcan_memorymesh_core::StorageSnapshot = serde_json::from_str(
        &data_plane
            .load_core_snapshot_json()
            .await
            .expect("rerank persisted read")
            .expect("rerank persisted state"),
    )
    .expect("rerank persisted JSON");
    assert_eq!(persisted.dataset.memories, original_disk.dataset.memories);
    assert_eq!(
        runtime
            .storage_snapshot()
            .expect("unchanged runtime memories")
            .dataset
            .memories,
        original
    );
    // Rebuilding the runtime proves the saved global group survives disk reload.
    // 重建运行时证明已保存全局组能够从磁盘重新加载。
    let reopened = NativeVmmCoreRuntime::bootstrap(port, data_plane)
        .await
        .expect("rerank reload");
    assert_eq!(
        reopened
            .runtime_policy()
            .await
            .expect("reloaded retrieval")
            .retrieval,
        global
    );
}
