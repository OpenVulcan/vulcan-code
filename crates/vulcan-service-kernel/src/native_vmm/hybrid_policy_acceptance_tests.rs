//! Formal hybrid, shared-window, RRF and diversity controls consumed by actual native search.
//! 由实际原生搜索消费正式混合检索、共享窗口、倒数排名及多样性控制项。

use super::*;
use vulcan_service_api::{VmmMemoryActionResponse, VmmNativeRetrievalPolicySummary};
use vulcan_vmm::{MemorySearchHit, SearchMemoryEventsRequest};

#[path = "mmr_policy_acceptance_tests.rs"]
mod mmr_policy;

/// Closed document identities used only to select deterministic inference vectors.
/// 仅用于选择确定性推理向量的封闭文档身份。
const MARKERS: [&str; 5] = [
    "hybrid-alpha",
    "hybrid-beta",
    "hybrid-gamma",
    "hybrid-delta",
    "hybrid-epsilon",
];
/// Distinct cosines give stable vector ranks while orthogonal residuals prevent hard duplication.
/// 不同余弦提供稳定向量排名，正交残差避免硬重复。
const COSINES: [f32; 5] = [0.9, 0.75, 0.6, 0.45, 0.3];
/// The real Jieba query appears with different frequencies in normally written summaries.
/// 真实结巴查询以不同频次出现在正常写入的摘要中。
const QUERY: &str = "西瓜";

/// Returns declared route metadata with controlled document/query vectors for the actual embedding request.
/// 为真实向量请求返回声明路由元数据及受控文档／查询向量。
/// Takes aligned input text and changes only inference values; all indexing and score computation remain real.
/// 接收对齐输入文本，仅改变推理值；全部索引与评分计算保持真实。
pub(super) fn embedding_response(request: &HostAiEmbeddingRequest) -> EmbeddingResponse {
    // Preserve validated route, fingerprint, dimension and input identities from the established host fixture.
    // 保留既有宿主夹具中已校验的路由、指纹、维度及输入身份。
    let mut response = nonempty::embedding_response(request);
    for (result, input) in response.results.iter_mut().zip(&request.request.inputs) {
        // This fixture explicitly declares text only, not alternate resource inputs.
        // 此夹具明确只声明文本，不包含其他资源输入。
        let EmbeddingInputContent::Text { text } = &input.content else {
            panic!("hybrid fixture requires text");
        };
        // A unit query axis and one distinct orthogonal residual per document fix the intended cosine order.
        // 单位查询轴和每文档独有正交残差确定预期余弦顺序。
        let mut values = vec![0.0; 1_536];
        if text == QUERY {
            values[0] = 1.0;
        } else {
            // Every non-query input must contain exactly one declared seed identity.
            // 每个非查询输入必须包含且仅包含一个声明种子身份。
            let positions = MARKERS
                .iter()
                .enumerate()
                .filter(|(_, marker)| text.contains(*marker))
                .map(|(index, _)| index)
                .collect::<Vec<_>>();
            assert_eq!(
                positions.len(),
                1,
                "unexpected hybrid embedding input: {text}"
            );
            // The verified unique marker owns this vector; no alternative field or path is probed.
            // 已验证的唯一标记持有此向量；不探测替代字段或路径。
            let index = positions[0];
            values[0] = COSINES[index];
            values[index + 1] = (1.0 - COSINES[index] * COSINES[index]).sqrt();
        }
        result.embedding = EmbeddingPayload::Dense { values };
    }
    response
}

/// Pauses only the actual single-query embedding after command admission when explicitly armed.
/// 仅在明确启用时暂停命令准入后的真实单查询向量请求。
/// Takes request and test-owned slot; returns immediately for normal document writes.
/// 接收请求与测试持有的暂停槽；普通文档写入立即返回。
pub(super) async fn wait_for_query(
    request: &HostAiEmbeddingRequest,
    slot: &StdMutex<Option<precheck_policy::IntentPause>>,
) {
    if request.request.inputs.len() == 1
        && matches!(&request.request.inputs[0].content, EmbeddingInputContent::Text { text } if text == QUERY)
    {
        precheck_policy::wait_for_intent(slot).await;
    }
}

/// Saves the exact global retrieval group through the production action and validates its response.
/// 通过生产动作保存准确全局检索组，并校验响应。
/// Takes isolated service and explicit settings; returns after persistence and projection succeed.
/// 接收隔离服务与显式设置；持久化及投影成功后返回。
async fn save_global(
    service: &crate::state::SharedServiceBridgeState,
    policy: &VmmNativeRetrievalPolicySummary,
) {
    // Global configuration reaches the same runtime subsequently used for searches.
    // 全局配置到达随后搜索使用的同一运行时。
    let saved = policy_service::action(
        service,
        "native_retrieval_policy_update",
        serde_json::to_value(policy).expect("hybrid JSON"),
    )
    .await
    .expect("save hybrid global");
    let VmmMemoryActionResponse::NativeRuntimePolicy(saved) = saved else {
        panic!("expected retrieval response");
    };
    assert_eq!(saved.retrieval, *policy);
}

/// Saves or removes only host A's retrieval override and validates the effective group.
/// 仅保存或移除宿主 A 的检索覆盖，并校验有效组。
/// Takes explicit optional override and expected effective settings; other groups remain inherited.
/// 接收明确可选覆盖及预期有效设置；其他组保持继承。
async fn save_a(
    service: &crate::state::SharedServiceBridgeState,
    retrieval: Option<VmmNativeRetrievalPolicySummary>,
    effective: &VmmNativeRetrievalPolicySummary,
) {
    // Null is the product's declared inheritance operation, not a compatibility fallback.
    // 空值是产品声明的继承操作，而不是兼容兜底。
    let overrides = VmmNativeHostProjectPolicyOverrides {
        retrieval,
        ..Default::default()
    };
    // Require both the owning host and complete acknowledged override group.
    // 同时要求归属宿主及完整已确认覆盖组正确。
    let saved = policy_service::action(
        service,
        "native_host_project_policy_update",
        serde_json::json!({"hostProjectId":"hybrid-host-a", "overrides":overrides}),
    )
    .await
    .expect("save hybrid host");
    let VmmMemoryActionResponse::NativeHostProjectPolicy(saved) = saved else {
        panic!("expected hybrid host response");
    };
    assert_eq!(saved.host_project_id, "hybrid-host-a");
    assert_eq!(saved.overrides, overrides);
    assert_eq!(saved.effective.retrieval, *effective);
}

/// Executes one public management search and returns exact hits after group and count checks.
/// 执行一次公开管理搜索，在组及数量检查后返回准确命中。
/// Takes client, bound project/host and requested final count; no Core command is constructed by the fixture.
/// 接收客户端、已绑定项目／宿主及请求最终数量；夹具不构造 Core 命令。
async fn search(
    client: &NativeVmmClient,
    project_id: u64,
    host: &str,
    top_k: u32,
) -> Vec<MemorySearchHit> {
    // The declared management path carries no runtime-session context; a zero threshold exposes
    // the full physical ranking under test.
    // 声明的管理路径不携带运行时会话上下文；零阈值暴露待测的完整物理排序。
    let request = |min_score| SearchMemoryEventsRequest {
        runtime_session_id: None,
        user_id: 1,
        project_id,
        host_project_id: host.to_owned(),
        queries: vec![QUERY.to_owned()],
        top_k,
        min_score,
    };
    let response = client
        .search_memory_events(request(Some(0.0)))
        .await
        .expect("actual hybrid search");
    // The default PreCheck threshold only partitions the same ranking into kept and counted hits.
    // 默认 PreCheck 阈值只把同一排序划分为保留命中与计数命中。
    let gated = client
        .search_memory_events(request(None))
        .await
        .expect("default-threshold hybrid search");
    let gated_group = &gated.results[0];
    assert_eq!(
        gated_group.hits.len() + gated_group.below_threshold_count as usize,
        top_k as usize
    );
    let full = ids(&response.results[0].hits);
    assert!(
        ids(&gated_group.hits)
            .iter()
            .all(|kept| full.contains(kept)),
        "kept hits must come from the same ranking"
    );
    assert_eq!(response.results.len(), 1);
    assert_eq!(response.results[0].query_index, 0);
    assert_eq!(response.results[0].query, QUERY);
    assert_eq!(response.results[0].hits.len(), top_k as usize);
    response
        .results
        .into_iter()
        .next()
        .expect("single hybrid group")
        .hits
}

/// Extracts public identities without reinterpreting the producer or final scores.
/// 提取公开身份，不重新解释生产者分数或最终分数。
/// Takes actual hits and returns IDs in their observed order.
/// 接收真实命中，按观测顺序返回 ID。
fn ids(hits: &[MemorySearchHit]) -> Vec<u64> {
    hits.iter().map(|hit| hit.memory_id).collect()
}

/// Computes an independent two-channel RRF expectation from observed identities and a small exact constant.
/// 根据已观测身份及小范围准确常数独立计算双通道倒数排名预期。
/// Takes complete vector/lexical orders and k; returns descending fusion order without production helpers.
/// 接收完整向量／词法顺序及常数；不调用生产辅助函数，返回融合降序。
fn rrf_order(vector: &[u64], lexical: &[u64], k: u32) -> Vec<u64> {
    assert_eq!(vector.len(), lexical.len());
    // Every fixture document deliberately matches both channels; missing identity is an explicit failure.
    // 每条夹具文档有意匹配两个通道；缺失身份明确失败。
    let mut ranked = vector
        .iter()
        .enumerate()
        .map(|(vector_rank, id)| {
            // Resolve the exact same public memory identity in the actual physical lexical order.
            // 在真实物理词法顺序中解析完全相同的公开记忆身份。
            let lexical_rank = lexical
                .iter()
                .position(|candidate| candidate == id)
                .expect("both-channel identity");
            (
                *id,
                1.0 / (f64::from(k) + 1.0 + vector_rank as f64)
                    + 1.0 / (f64::from(k) + 1.0 + lexical_rank as f64),
            )
        })
        .collect::<Vec<_>>();
    ranked.sort_by(|left, right| right.1.total_cmp(&left.1));
    assert!(
        ranked.windows(2).all(|pair| pair[0].1 > pair[1].1),
        "fixture must avoid fusion ties"
    );
    ranked.into_iter().map(|(id, _)| id).collect()
}

/// Checks exact ordered hits and stage provenance for a saved-policy search.
/// 检查已保存策略搜索的准确有序命中及阶段来源。
/// Takes actual client and expected IDs/origin; returns after public response assertions.
/// 接收真实客户端及预期 ID／来源；公开响应断言后返回。
async fn check(
    client: &NativeVmmClient,
    project_id: u64,
    host: &str,
    expected: &[u64],
    origin: &str,
) {
    // The requested final count is independent of the saved physical candidate window.
    // 请求最终数量独立于已保存物理候选窗口。
    let hits = search(
        client,
        project_id,
        host,
        u32::try_from(expected.len()).expect("small search limit"),
    )
    .await;
    assert_eq!(ids(&hits), expected);
    for hit in hits {
        // Do not equate bounded decision scores with the distinct RRF contribution sum.
        // 不将有界决策分值等同于独立的倒数排名贡献总和。
        let retrieval = hit.retrieval.expect("hybrid provenance");
        assert_eq!(retrieval.origin, origin);
        assert!(retrieval.matched_context_values.is_empty());
        assert_eq!(retrieval.matched_context_score_delta, Some(0.0));
    }
}

/// Verifies hybrid enablement, shared candidate windows, RRF/diversity reordering and caller snapshot boundaries.
/// 验证混合启用、共享候选窗口、倒数排名／多样性重排及调用方快照边界。
/// Takes private physical storage and declared AI capability; only embedding and review inference are substituted.
/// 接收私有物理存储与声明 AI 能力；仅替代向量及审核推理。
pub(in crate::native_vmm) async fn verify(
    data_plane: VmmDataPlaneHandle,
    capability: InferenceCapabilitySnapshot,
) {
    // Closed inference mode rejects unrelated LLM purposes and all unexpected reranking.
    // 封闭推理模式拒绝无关 LLM 用途及所有意外重排。
    let port = Arc::new(CapturingAnalysisPort {
        inner: MockHostAiPort::new(),
        requests: StdMutex::new(Vec::new()),
        reject_next_turn: AtomicBool::new(false),
        fixture: AnalysisFixture::HybridPolicy(StdMutex::new(None)),
        embedding_requests: StdMutex::new(Vec::new()),
        profile_instruction_pause: StdMutex::new(None),
        postaction_pause: StdMutex::new(None),
    });
    port.inner
        .set_capability_snapshot(capability)
        .expect("hybrid capability");
    // Use the production native bootstrap and real controller indexes.
    // 使用生产原生引导与真实控制器索引。
    let runtime = Arc::new(
        NativeVmmCoreRuntime::bootstrap(port.clone(), data_plane.clone())
            .await
            .expect("hybrid runtime"),
    );
    // Shared memory ownership makes saved caller settings the only differing search input.
    // 共享记忆归属使已保存调用方设置成为唯一不同搜索输入。
    let project = data_plane
        .create_project("Hybrid policy", "hybrid-policy")
        .await
        .expect("hybrid project");
    for host in ["hybrid-host-a", "hybrid-host-b"] {
        data_plane
            .bind_vulcan_project(host, project.id)
            .await
            .expect("hybrid binding");
    }
    // One gate is shared by all real saves and public client operations.
    // 所有真实保存与公开客户端操作共享一个操作锁。
    let gate = Arc::new(RwLock::new(()));
    // The settings service uses the same runtime that admits subsequent searches.
    // 设置服务使用准入后续搜索的同一运行时。
    let service = policy_service::service(runtime.clone(), data_plane.clone(), gate.clone());
    // Explicitly isolate channel/fusion controls from reranking, diversity and time decay.
    // 明确将通道／融合控制与重排、多样性及时间衰减隔离。
    let mut global = runtime
        .runtime_policy()
        .await
        .expect("hybrid defaults")
        .retrieval;
    global.hybrid_enabled = false;
    global.lexical_candidate_floor = 1;
    global.rrf_k = "1".to_owned();
    global.rerank_top_n = 0;
    global.mmr_enabled = false;
    global.weibull_enabled = false;
    save_global(&service, &global).await;
    // Public direct writes perform actual review, hard dedupe and physical indexing.
    // 公开主动写入执行真实审核、硬去重与物理索引。
    let client = NativeVmmClient::new(
        runtime.clone(),
        data_plane.clone(),
        gate,
        Arc::new(AtomicBool::new(true)),
        Arc::new(Notify::new()),
    );
    // Term frequencies create deliberately different lexical ranks, verified against the real index below.
    // 词频产生有意不同的词法排名，随后对照真实索引验证。
    let frequencies = [1, 3, 5, 4, 2];
    // No record or vector is inserted by a storage test helper.
    // 不通过存储测试辅助函数插入任何记录或向量。
    let written = client
        .write_memories(WriteMemoriesRequest {
            host_project_id: "hybrid-host-b".to_owned(),
            project_id: project.id,
            user_id: 1,
            session_id: "hybrid-seed".to_owned(),
            items: MARKERS
                .iter()
                .zip(frequencies)
                .map(|(marker, frequency)| vulcan_vmm::WriteMemoryItem {
                    scope_level: 2,
                    category: 0,
                    priority: 2,
                    memory_level: 3,
                    abstract_text: format!("{} {marker}", vec![QUERY; frequency].join(" ")),
                    details: format!(
                        "The confirmed fruit preference for {marker} remains explicit."
                    ),
                })
                .collect(),
        })
        .await
        .expect("normal hybrid writes");
    assert_eq!(written.items.len(), 5);
    assert!(written.items.iter().all(|item| !item.deduped));
    // Preserve both physical-disk and vector-bearing in-memory representations independently.
    // 独立保留物理磁盘表示与携带向量的内存表示。
    let original = runtime
        .storage_snapshot()
        .expect("hybrid originals")
        .dataset
        .memories;
    // Relational snapshots deliberately omit embeddings and must be compared with the same representation.
    // 关系快照有意省略向量，必须与相同表示比较。
    let original_disk: vulcan_memorymesh_core::StorageSnapshot = serde_json::from_str(
        &data_plane
            .load_core_snapshot_json()
            .await
            .expect("hybrid disk baseline")
            .expect("hybrid disk snapshot"),
    )
    .expect("hybrid disk JSON");
    // Observe the actual pure-vector order and verify the fixture's distinct cosine ordering by summary identity.
    // 观察真实纯向量顺序，并通过摘要身份验证夹具不同余弦顺序。
    let vector_hits = search(&client, project.id, "hybrid-host-b", 5).await;
    for (hit, marker) in vector_hits.iter().zip(MARKERS) {
        assert!(hit.abstract_text.contains(marker));
    }
    // Exact public vector identities also form the expected order while hybrid retrieval is disabled.
    // 准确公开向量身份也构成混合检索关闭时的预期顺序。
    let vector = ids(&vector_hits);
    // Read the real Jieba/BM25 index, not the fusion implementation or a fixture-supplied candidate channel.
    // 读取真实结巴／词频评分索引，不读取融合实现或夹具提供的候选通道。
    let lexical = data_plane
        .search_lexical_page(QUERY, 10, 0)
        .await
        .expect("physical hybrid lexical ranks");
    assert_eq!(lexical.len(), 5);
    // Resolve physical storage IDs through exact known public identities, without guessing alternate prefixes.
    // 通过准确已知公开身份解析物理存储 ID，不猜测其他前缀。
    let lexical_ids = lexical
        .iter()
        .map(|hit| {
            *vector
                .iter()
                .find(|id| format!("vmm-memory-{id}") == hit.memory_id)
                .expect("known physical memory ID")
        })
        .collect::<Vec<_>>();
    assert_eq!(
        lexical_ids,
        [vector[2], vector[3], vector[1], vector[4], vector[0]]
    );
    check(
        &client,
        project.id,
        "hybrid-host-b",
        &vector,
        "vector_search",
    )
    .await;
    // A disabled hybrid stage must ignore even a larger saved lexical floor.
    // 关闭的混合阶段必须忽略更大的已保存词法下限。
    global.lexical_candidate_floor = 5;
    save_global(&service, &global).await;
    check(
        &client,
        project.id,
        "hybrid-host-b",
        &vector[..1],
        "vector_search",
    )
    .await;
    global.hybrid_enabled = true;
    global.lexical_candidate_floor = 1;
    save_global(&service, &global).await;
    // With one candidate per channel, the two disjoint leaders tie and the earlier stable ID wins.
    // 每通道一个候选时，两个不同首项并列，较早稳定 ID 胜出。
    check(
        &client,
        project.id,
        "hybrid-host-b",
        &vector[..1],
        "vector_search",
    )
    .await;
    global.lexical_candidate_floor = 3;
    save_global(&service, &global).await;
    // Widening both physical channels admits their shared third-vector/first-lexical candidate as the winner.
    // 扩大两个物理通道后，其共享的向量第三／词法第一候选成为首项。
    check(
        &client,
        project.id,
        "hybrid-host-b",
        &[vector[2]],
        "hybrid_rrf",
    )
    .await;
    // Independently computed expectations must differ; an inert RRF setting cannot pass both cases.
    // 独立计算的预期必须不同；未生效的倒数排名设置不能同时通过两种情况。
    let small = rrf_order(&vector, &lexical_ids, 1);
    // A larger constant changes the balance between the two actual rank permutations.
    // 较大常数改变两个真实排名排列间的平衡。
    let large = rrf_order(&vector, &lexical_ids, 60);
    assert_ne!(small, large);
    global.lexical_candidate_floor = 5;
    save_global(&service, &global).await;
    check(&client, project.id, "hybrid-host-b", &small, "hybrid_rrf").await;
    global.rrf_k = "60".to_owned();
    save_global(&service, &global).await;
    check(&client, project.id, "hybrid-host-b", &large, "hybrid_rrf").await;
    save_a(&service, Some(global.clone()), &global).await;
    global.hybrid_enabled = false;
    global.lexical_candidate_floor = 1;
    global.rrf_k = "1".to_owned();
    save_global(&service, &global).await;
    check(&client, project.id, "hybrid-host-a", &large, "hybrid_rrf").await;
    check(
        &client,
        project.id,
        "hybrid-host-b",
        &vector,
        "vector_search",
    )
    .await;
    save_a(&service, None, &global).await;
    check(
        &client,
        project.id,
        "hybrid-host-a",
        &vector,
        "vector_search",
    )
    .await;
    global.hybrid_enabled = true;
    global.lexical_candidate_floor = 5;
    global.rrf_k = "60".to_owned();
    save_global(&service, &global).await;
    // Stop at the first actual query embedding, before any channel reads, then change all three saved controls.
    // 在任何通道读取前的首个真实查询向量调用处暂停，再改变三个已保存控制项。
    let (entered, observed) = tokio::sync::oneshot::channel();
    // Resume only after the concurrent formal save acknowledges the new next-command group.
    // 仅在并发正式保存确认新的下一命令组后恢复。
    let (resume, resumed) = tokio::sync::oneshot::channel();
    // The explicit fixture owns this pause slot; no unrelated mode can consume it.
    // 明确夹具持有此暂停槽；无关模式不能消费。
    let AnalysisFixture::HybridPolicy(slot) = &port.fixture else {
        panic!("hybrid fixture required");
    };
    *slot.lock().expect("arm hybrid query") = Some(precheck_policy::IntentPause {
        entered,
        resume: resumed,
    });
    global.hybrid_enabled = false;
    global.lexical_candidate_floor = 1;
    global.rrf_k = "1".to_owned();
    tokio::join!(
        check(&client, project.id, "hybrid-host-b", &large, "hybrid_rrf"),
        async {
            tokio::time::timeout(Duration::from_secs(10), observed)
                .await
                .expect("bounded hybrid admission")
                .expect("hybrid query reached");
            save_global(&service, &global).await;
            resume.send(()).expect("resume hybrid search");
        }
    );
    check(
        &client,
        project.id,
        "hybrid-host-b",
        &vector,
        "vector_search",
    )
    .await;
    // Reuse the same real sources for independent diversity controls before checking final persistence.
    // 在最终持久化检查前，复用相同真实来源独立验证多样性控制。
    mmr_policy::verify(&service, &client, &port, project.id, &mut global, &vector).await;
    // Search policies leave both original record representations and physical lexical order intact.
    // 搜索策略保持两种原始记录表示及物理词法顺序不变。
    let persisted: vulcan_memorymesh_core::StorageSnapshot = serde_json::from_str(
        &data_plane
            .load_core_snapshot_json()
            .await
            .expect("hybrid persisted read")
            .expect("hybrid persisted snapshot"),
    )
    .expect("hybrid persisted JSON");
    assert_eq!(persisted.dataset.memories, original_disk.dataset.memories);
    assert_eq!(
        runtime
            .storage_snapshot()
            .expect("hybrid unchanged memories")
            .dataset
            .memories,
        original
    );
    assert_eq!(
        data_plane
            .search_lexical_page(QUERY, 10, 0)
            .await
            .expect("unchanged lexical ranks"),
        lexical
    );
    // Rebuild only the runtime, proving settings reload without regenerating or rewriting documents.
    // 仅重建运行时，证明设置重新加载且不重新生成或改写文档。
    let reopened = Arc::new(
        NativeVmmCoreRuntime::bootstrap(port, data_plane.clone())
            .await
            .expect("hybrid reopen"),
    );
    assert_eq!(
        reopened
            .runtime_policy()
            .await
            .expect("hybrid reloaded policy")
            .retrieval,
        global
    );
    // A rebuilt service/client pair consumes only state hydrated from the durable planes.
    // 重建服务／客户端对仅消费从持久数据面恢复的状态。
    let reopened_gate = Arc::new(RwLock::new(()));
    // Use one new lifecycle gate for the rebuilt owner; the previous client has no pending work.
    // 为重建归属方使用一个新生命周期锁；此前客户端没有待执行工作。
    let reopened_service =
        policy_service::service(reopened.clone(), data_plane.clone(), reopened_gate.clone());
    // Invoke public searches again rather than checking loaded settings alone.
    // 再次调用公开搜索，而不只是检查加载设置。
    let reopened_client = NativeVmmClient::new(
        reopened,
        data_plane.clone(),
        reopened_gate,
        Arc::new(AtomicBool::new(true)),
        Arc::new(Notify::new()),
    );
    mmr_policy::verify_reloaded(
        &reopened_service,
        &reopened_client,
        project.id,
        &global,
        &vector,
    )
    .await;
    // Post-reload searches must preserve the same complete relational records as the original direct writes.
    // 重载后搜索必须保留与原主动写入相同的完整关系记录。
    let final_disk: vulcan_memorymesh_core::StorageSnapshot = serde_json::from_str(
        &data_plane
            .load_core_snapshot_json()
            .await
            .expect("MMR final disk read")
            .expect("MMR final disk snapshot"),
    )
    .expect("MMR final disk JSON");
    assert_eq!(final_disk.dataset.memories, original_disk.dataset.memories);
}
