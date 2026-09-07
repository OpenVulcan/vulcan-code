//! Formal replacement settings exercised through normal direct writes and physical persistence.
//! 通过正常主动写入与物理持久化验收正式替代设置。

use super::*;
use vulcan_memorymesh_core::{MemoryId, MemoryRecord, MemoryState};
use vulcan_service_api::{VmmMemoryActionResponse, VmmNativeRetrievalPolicySummary};

#[path = "postaction_replacement_policy_acceptance_tests.rs"]
mod postaction;

#[path = "policy_http_acceptance_tests.rs"]
pub(in crate::native_vmm) mod network;

/// Exact seed identities; their positions define independently expected query cosine ranks.
/// 准确种子身份；位置定义独立预期的查询余弦排名。
const MARKERS: [&str; 3] = ["replacement-alpha", "replacement-beta", "replacement-gamma"];
/// Distinct unit-vector projections, yielding public decision scores 0.95, 0.875 and 0.8.
/// 不同单位向量投影，产生公开决策分值 0.95、0.875 和 0.8。
const COSINES: [f32; 3] = [0.9, 0.75, 0.6];
/// Every probe has a declared query axis but unique text to avoid exact-content deduplication.
/// 每个探针使用声明的查询轴及不同正文，避免内容精确去重。
const PROBE: &str = "replacement-probe";

/// Pauses an armed probe embedding after admission; request and slot are actual host inputs.
/// 在准入后暂停已启用探针向量；请求与槽来自真实宿主输入。
/// Returns after release, or immediately for seed embeddings and unarmed probes.
/// 释放后返回；种子向量及未启用探针立即返回。
pub(super) async fn wait_for_query(
    request: &HostAiEmbeddingRequest,
    slot: &StdMutex<Option<precheck_policy::IntentPause>>,
) {
    if request.request.inputs.iter().any(|input| {
        matches!(&input.content, EmbeddingInputContent::Text { text } if text.contains(PROBE))
    }) {
        precheck_policy::wait_for_intent(slot).await;
    }
}

/// Produces declared seed/probe vectors in the validated response envelope for the supplied request.
/// 为传入请求在已校验响应封套中生成声明的种子／探针向量。
/// Returns no search results: all candidate ranking and physical reads remain production-owned.
/// 不返回搜索结果：全部候选排序及物理读取仍由生产代码执行。
pub(super) fn embedding_response(request: &HostAiEmbeddingRequest) -> EmbeddingResponse {
    // Preserve authoritative route, dimensions and per-input identities from the established fixture.
    // 保留既有夹具的权威路由、维度和逐输入身份。
    let mut response = nonempty::embedding_response(request);
    for (result, input) in response.results.iter_mut().zip(&request.request.inputs) {
        // This fixture has an explicitly text-only input contract.
        // 此夹具具有明确的纯文本输入契约。
        let EmbeddingInputContent::Text { text } = &input.content else {
            panic!("replacement fixture requires text");
        };
        // All vectors are unit-length with independent residual axes for seeds.
        // 所有向量均为单位长度，种子具有独立残差轴。
        let mut values = vec![0.0; 1_536];
        if text.contains(PROBE) {
            assert!(MARKERS.iter().all(|marker| !text.contains(marker)));
            values[0] = 1.0;
        } else {
            // Require one exact declared seed marker rather than guessing an input owner.
            // 要求一个准确声明的种子标记，不猜测输入归属。
            let positions = MARKERS
                .iter()
                .enumerate()
                .filter(|(_, marker)| text.contains(*marker))
                .map(|(index, _)| index)
                .collect::<Vec<_>>();
            assert_eq!(positions.len(), 1, "unexpected replacement text: {text}");
            // The marker position supplies the declared independent projection.
            // 标记位置提供声明的独立投影。
            let index = positions[0];
            values[0] = COSINES[index];
            values[index + 1] = (1.0 - COSINES[index] * COSINES[index]).sqrt();
        }
        result.embedding = EmbeddingPayload::Dense { values };
    }
    response
}

/// Accepts seed writes and supersedes exactly the actual similar memories exposed for probes.
/// 接受种子写入，并为探针替代实际暴露的准确相似记忆。
/// Takes the real rendered purpose/input; extracts one declared L1 probe or returns an exact L2 partition.
/// 接收真实渲染用途／输入；提取一个声明 L1 探针或返回准确 L2 分区。
pub(super) fn response(purpose: &str, input: &serde_json::Value) -> serde_json::Value {
    if purpose == "postaction_l1" {
        return postaction::response(input);
    }
    assert_eq!(purpose, "postaction_l2");
    // The renderer's memory array is the only candidate source for direct review.
    // 渲染器的记忆数组是主动评审的唯一候选来源。
    let accepted = input["memory"].as_array().expect("replacement candidates").iter().map(|candidate| {
        // Candidate summary is the verified producer field identifying probes versus seeds.
        // 候选摘要是已查明的生产者字段，用于区分探针和种子。
        let summary = candidate["abstract"].as_str().expect("replacement summary");
        // Seed creation never replaces earlier records; probes act only on observed candidate IDs.
        // 种子创建不替代先前记录；探针仅操作已观察到的候选身份。
        let superseded = if summary.contains(PROBE) {
            candidate["similar_memories"].as_array().expect("replacement similar memories")
                .iter().map(|memory| memory["memory_id"].clone()).collect::<Vec<_>>()
        } else {
            assert_eq!(MARKERS.iter().filter(|marker| summary.contains(**marker)).count(), 1);
            Vec::new()
        };
        serde_json::json!({"candidate_index":candidate["candidate_index"], "supersede_memory_ids":superseded})
    }).collect::<Vec<_>>();
    serde_json::json!({"memory":{"accepted_candidates":accepted,"dropped_candidates":[],"reason":"Replace only the actual reviewed candidates."}})
}

/// One physical project and the original ordered identities created by normal writes.
/// 一个物理项目以及正常写入产生的原始有序身份。
struct Seeds {
    /// Public stable memory project identity, never a display-name lookup.
    /// 公开稳定记忆项目身份，绝不按显示名称查询。
    project_id: u64,
    /// Seed identities in declared vector-rank order.
    /// 按声明向量排名排序的种子身份。
    ids: Vec<MemoryId>,
}

/// Shared real service/client/storage environment with inference-only control.
/// 共享的真实服务／客户端／存储环境，仅控制推理响应。
struct Acceptance {
    /// The validating inference port captures exact review inputs.
    /// 执行校验的推理端口捕获准确评审输入。
    port: Arc<CapturingAnalysisPort>,
    /// Production runtime owning authoritative command snapshots and memory records.
    /// 持有权威命令快照及记忆记录的生产运行时。
    runtime: Arc<NativeVmmCoreRuntime>,
    /// Actual management service for formal policy writes.
    /// 用于正式策略写入的实际管理服务。
    service: crate::state::SharedServiceBridgeState,
    /// Normal public client for seed and probe writes.
    /// 用于种子及探针写入的正常公开客户端。
    client: NativeVmmClient,
    /// Physical project, relational and vector storage owner.
    /// 物理项目、关系及向量存储持有者。
    data_plane: VmmDataPlaneHandle,
}

impl Acceptance {
    /// Saves the supplied complete global retrieval group and verifies its exact service response.
    /// 保存传入的完整全局检索组，并核对准确服务响应。
    /// Returns only after formal persistence and runtime publication succeed.
    /// 仅在正式持久化及运行时发布成功后返回。
    async fn save_global(&self, policy: &VmmNativeRetrievalPolicySummary) {
        // The formal service response must match the requested group without fallback values.
        // 正式服务响应必须与请求组相同，不使用回退值。
        let saved = policy_service::action(
            &self.service,
            "native_retrieval_policy_update",
            serde_json::to_value(policy).expect("replacement policy JSON"),
        )
        .await
        .expect("save replacement policy");
        let VmmMemoryActionResponse::NativeRuntimePolicy(saved) = saved else {
            panic!("expected replacement global response");
        };
        assert_eq!(saved.retrieval, *policy);
    }

    /// Saves A's explicit group or declared inheritance, verifying both override and effective value.
    /// 保存 A 的明确策略组或声明继承，同时核对覆盖值和有效值。
    /// Takes the optional override and expected effective group; returns after successful publication.
    /// 接收可选覆盖与预期有效组；成功发布后返回。
    async fn save_a(
        &self,
        retrieval: Option<VmmNativeRetrievalPolicySummary>,
        effective: &VmmNativeRetrievalPolicySummary,
    ) {
        // Null is the product's declared whole-group inheritance representation.
        // 空值是产品声明的整组继承表示。
        let overrides = VmmNativeHostProjectPolicyOverrides {
            retrieval,
            ..Default::default()
        };
        // Resolve the exact host-specific response, not a global default readback.
        // 解析准确宿主专属响应，不读取全局默认作为替代。
        let saved = policy_service::action(
            &self.service,
            "native_host_project_policy_update",
            serde_json::json!({"hostProjectId":"replacement-host-a","overrides":overrides}),
        )
        .await
        .expect("save replacement host");
        let VmmMemoryActionResponse::NativeHostProjectPolicy(saved) = saved else {
            panic!("expected replacement host response");
        };
        assert_eq!(saved.host_project_id, "replacement-host-a");
        assert_eq!(saved.overrides, overrides);
        assert_eq!(saved.effective.retrieval, *effective);
    }

    /// Creates three normal seeds in a new project under neutral settings, then returns their exact IDs.
    /// 在新项目中以中性设置正常创建三个种子，然后返回准确身份。
    /// Takes the case name and neutral group; distinct categories expose hard-pool boundaries.
    /// 接收场景名称及中性组；不同类别用于暴露硬去重池边界。
    async fn seeds(&self, name: &str, neutral: &VmmNativeRetrievalPolicySummary) -> Seeds {
        // Separate physical scopes prevent prior successful replacements contaminating later cases.
        // 独立物理作用域防止先前成功替代污染后续场景。
        let project = self
            .data_plane
            .create_project(name, name)
            .await
            .expect("replacement project");
        for host in ["replacement-host-a", "replacement-host-b"] {
            self.data_plane
                .bind_vulcan_project(host, project.id)
                .await
                .expect("replacement binding");
        }
        self.save_global(neutral).await;
        self.save_a(None, neutral).await;
        // First two seeds have category one; only the third matches the category-zero probe.
        // 前两个种子为类别一；只有第三个与类别零探针匹配。
        let written = self
            .client
            .write_memories(WriteMemoriesRequest {
                host_project_id: "replacement-host-b".to_owned(),
                project_id: project.id,
                user_id: 1,
                session_id: format!("{name}-seed"),
                items: MARKERS
                    .iter()
                    .enumerate()
                    .map(|(index, marker)| vulcan_vmm::WriteMemoryItem {
                        scope_level: 2,
                        category: i32::from(index < 2),
                        priority: 2,
                        memory_level: 3,
                        abstract_text: (*marker).to_owned(),
                        details: format!("Confirmed seed {marker}."),
                    })
                    .collect(),
            })
            .await
            .expect("normal replacement seed writes");
        assert_eq!(written.items.len(), 3);
        assert!(written.items.iter().all(|item| !item.deduped));
        Seeds {
            project_id: project.id,
            ids: written
                .items
                .iter()
                .map(|item| {
                    MemoryId::new(format!("vmm-memory-{}", item.memory_id)).expect("seed identity")
                })
                .collect(),
        }
    }

    /// Executes one public probe and verifies exact L2 inputs, reuse/new result and all prior records.
    /// 执行一次公开探针，核对准确 L2 输入、复用／新建结果及全部既有记录。
    /// Takes seeds, host, unique text, expected similar indexes and optional hard-duplicate index.
    /// 接收种子、宿主、唯一正文、预期相似索引及可选硬重复索引。
    /// Returns after comparing complete before/after records against declared supersede mutations.
    /// 对照声明替代变更比较完整前后记录后返回。
    async fn probe(
        &self,
        seeds: &Seeds,
        host: &str,
        label: &str,
        similar: &[usize],
        duplicate: Option<usize>,
    ) {
        // Observe all projects, not just the target, to catch cross-scope writes.
        // 观察所有项目而不只是目标，以捕获跨作用域写入。
        let before = self
            .runtime
            .storage_snapshot()
            .expect("before replacement")
            .dataset
            .memories;
        // Exact request cursor distinguishes hard dedupe's no-model path from model-reviewed writes.
        // 准确请求游标区分硬去重的无模型路径和经过模型审核的写入。
        let cursor = self
            .port
            .requests
            .lock()
            .expect("replacement capture")
            .len();
        // Every label is unique, preventing exact hash dedupe from masking the similarity policy.
        // 每个标签唯一，防止精确哈希去重掩盖相似度策略。
        let summary = format!("{PROBE} {label}");
        // The public caller supplies an explicit stable source session.
        // 公开调用方提供明确稳定来源会话。
        let result = self
            .client
            .write_memories(WriteMemoriesRequest {
                host_project_id: host.to_owned(),
                project_id: seeds.project_id,
                user_id: 1,
                session_id: format!("probe-{label}"),
                items: vec![vulcan_vmm::WriteMemoryItem {
                    scope_level: 2,
                    category: 0,
                    priority: 2,
                    memory_level: 3,
                    abstract_text: summary.clone(),
                    details: format!("Explicit {summary}."),
                }],
            })
            .await
            .expect("replacement probe write");
        assert_eq!(result.items.len(), 1);
        assert_eq!(result.items[0].deduped, duplicate.is_some());
        // Convert the returned public identity through the established native identity contract.
        // 通过既有原生身份契约转换返回的公开身份。
        let returned = MemoryId::new(format!("vmm-memory-{}", result.items[0].memory_id))
            .expect("returned identity");
        // Clone captures before further async work, so no synchronous lock crosses an await.
        // 后续异步工作前克隆捕获，避免同步锁跨越等待。
        let requests = self
            .port
            .requests
            .lock()
            .expect("replacement review captures")[cursor..]
            .to_vec();
        if let Some(index) = duplicate {
            assert_eq!(returned, seeds.ids[index]);
            assert!(requests.is_empty(), "hard reuse must skip L2");
            assert!(similar.is_empty());
        } else {
            assert!(!seeds.ids.contains(&returned));
            assert_eq!(requests.len(), 1);
            assert_eq!(requests[0].purpose_id, "postaction_l2");
            // Inspect the actual rendered driver input rather than a manually built review command.
            // 检查实际渲染的驱动输入，不手工构造评审命令。
            let input = analysis_input(&requests[0]);
            // One probe must produce exactly one review candidate in caller order.
            // 一个探针必须按调用顺序产生准确一个评审候选。
            let candidates = input["memory"].as_array().expect("probe review candidates");
            assert_eq!(candidates.len(), 1);
            assert_eq!(candidates[0]["abstract"], summary);
            // Exact per-candidate IDs and scores independently prove floor and top-k consumption.
            // 准确逐候选身份及分值独立证明门槛和数量被消费。
            let actual = candidates[0]["similar_memories"]
                .as_array()
                .expect("probe similar memories");
            assert_eq!(actual.len(), similar.len(), "probe {label}");
            for (memory, index) in actual.iter().zip(similar) {
                assert_eq!(memory["memory_id"], seeds.ids[*index].as_str());
                assert_eq!(memory["origin"], "vector_search");
                assert!(
                    (memory["score"].as_f64().expect("similar score") - [0.95, 0.875, 0.8][*index])
                        .abs()
                        < 1e-7
                );
            }
        }
        // Full records expose unexpected lifecycle/content mutations in any previously existing scope.
        // 完整记录暴露任何先前作用域中意外的生命周期／内容变化。
        let after = self
            .runtime
            .storage_snapshot()
            .expect("after replacement")
            .dataset
            .memories;
        assert_eq!(after.len(), before.len() + usize::from(duplicate.is_none()));
        for original in before {
            // Production supersede only changes state, reason and version; all other fields must remain exact.
            // 生产替代只修改状态、原因及版本；其他字段必须准确保持。
            let mut expected = original.clone();
            if duplicate.is_none() && similar.iter().any(|index| seeds.ids[*index] == original.id) {
                expected.state = MemoryState::Superseded;
                expected.status_reason = Some("superseded-by-direct-review".to_owned());
                expected.version += 1;
            }
            assert_eq!(
                after
                    .iter()
                    .find(|memory| memory.id == original.id)
                    .expect("prior record retained"),
                &expected
            );
        }
        if duplicate.is_none() {
            // Actual creation must preserve the explicit requested body and active state.
            // 实际创建必须保留明确请求正文及活动状态。
            let created = after
                .iter()
                .find(|memory| memory.id == returned)
                .expect("new probe record");
            assert_eq!(created.summary, summary);
            assert_eq!(created.content, format!("Explicit {summary}."));
            assert_eq!(created.state, MemoryState::Active);
        }
    }
}

/// Runs independent field, host and in-flight cases using real storage and formal service saves.
/// 使用真实存储与正式服务保存执行独立字段、宿主及在途场景。
/// Takes an isolated physical data plane and declared embedding capability; verifies durable rebuild last.
/// 接收隔离物理数据面及声明向量能力；最后验证持久重建。
pub(in crate::native_vmm) async fn verify(
    data_plane: VmmDataPlaneHandle,
    capability: InferenceCapabilitySnapshot,
) {
    // One explicit inference mode controls vectors/review decisions without replacing production storage.
    // 一个明确推理模式控制向量／评审决定，不替代生产存储。
    let port = Arc::new(CapturingAnalysisPort {
        inner: MockHostAiPort::new(),
        requests: StdMutex::new(Vec::new()),
        reject_next_turn: AtomicBool::new(false),
        fixture: AnalysisFixture::ReplacementPolicy(StdMutex::new(None)),
        embedding_requests: StdMutex::new(Vec::new()),
        profile_instruction_pause: StdMutex::new(None),
        postaction_pause: StdMutex::new(None),
    });
    port.inner
        .set_capability_snapshot(capability)
        .expect("replacement capability");
    // Bootstrap the actual native driver and authoritative physical stores.
    // 引导实际原生驱动及权威物理存储。
    let runtime = Arc::new(
        NativeVmmCoreRuntime::bootstrap(port.clone(), data_plane.clone())
            .await
            .expect("replacement runtime"),
    );
    // Both public writes and formal saves share the production operation gate.
    // 公开写入及正式保存共享生产操作锁。
    let gate = Arc::new(RwLock::new(()));
    // This environment retains all real objects for every scenario.
    // 此环境为每个场景保留全部真实对象。
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
    // Disable unrelated score transforms; zero hard threshold is the declared neutral seed policy.
    // 关闭无关分值变换；硬阈值零是声明的中性种子策略。
    let mut neutral = test
        .runtime
        .runtime_policy()
        .await
        .expect("replacement defaults")
        .retrieval;
    neutral.hybrid_enabled = false;
    neutral.rerank_top_n = 0;
    neutral.mmr_enabled = false;
    neutral.weibull_enabled = false;
    neutral.replace_min_similarity = 0.0;
    neutral.hard_dedupe_cosine_threshold = 0.0;
    neutral.hard_dedupe_pool_top_k = 3;
    neutral.replacement_top_k = 3;
    // Each tuple is a declared independent floor/window case, including legal endpoints and oversized limits.
    // 每个元组是声明的独立门槛／窗口场景，包含合法端点及超过实际数量的限制。
    for (name, floor, limit, expected) in [
        ("all-three", 0.0, 3, vec![0, 1, 2]),
        ("floor-high", 0.9, 3, vec![0]),
        ("floor-middle", 0.86, 3, vec![0, 1]),
        ("floor-one", 1.0, 3, vec![]),
        ("window-one", 0.0, 1, vec![0]),
        ("window-two", 0.0, 2, vec![0, 1]),
        ("window-five", 0.0, 5, vec![0, 1, 2]),
    ] {
        // Every case uses fresh normal records so the preceding replacement cannot change its candidate ranks.
        // 每个场景使用新正常记录，防止前次替代改变本次候选排名。
        let seeds = test.seeds(name, &neutral).await;
        // Only the named floor/window fields differ from the neutral group.
        // 只有指定门槛／窗口字段不同于中性组。
        let mut policy = neutral.clone();
        policy.replace_min_similarity = floor;
        policy.replacement_top_k = limit;
        test.save_global(&policy).await;
        test.probe(&seeds, "replacement-host-b", name, &expected, None)
            .await;
    }
    // Same-category gamma lies outside the one-result review window but inside an enlarged hard pool.
    // 同类别伽马位于单结果评审窗口之外，但位于扩大的硬去重池之内。
    for (name, threshold, pool, duplicate) in [
        ("hard-disabled", 0.0, 3, None),
        ("hard-below", 0.55, 3, Some(2)),
        ("hard-above", 0.65, 3, None),
        ("hard-small-pool", 0.55, 1, None),
    ] {
        // Fresh seeds isolate hard-pool inclusion from superseded status and exact-content dedupe.
        // 新种子将硬池纳入效果与已替代状态及精确内容去重隔离。
        let seeds = test.seeds(name, &neutral).await;
        // A one-result L2 window remains fixed while the separate hard pool and threshold vary.
        // 独立硬池与阈值变化时，单结果 L2 窗口保持固定。
        let mut policy = neutral.clone();
        policy.replacement_top_k = 1;
        policy.hard_dedupe_pool_top_k = pool;
        policy.hard_dedupe_cosine_threshold = threshold;
        test.save_global(&policy).await;
        test.probe(
            &seeds,
            "replacement-host-b",
            name,
            if duplicate.is_some() { &[] } else { &[0] },
            duplicate,
        )
        .await;
    }
    // A reuses gamma under its override while B creates a new record under the same physical candidates.
    // A 在覆盖下复用伽马，B 在相同物理候选上按全局规则新建记录。
    let shared = test.seeds("host-isolation", &neutral).await;
    // The override differs in all four tested controls from the global group.
    // 覆盖在全部四个被测控制项上不同于全局组。
    let mut override_policy = neutral.clone();
    override_policy.replace_min_similarity = 1.0;
    override_policy.replacement_top_k = 1;
    override_policy.hard_dedupe_pool_top_k = 3;
    override_policy.hard_dedupe_cosine_threshold = 0.55;
    test.save_a(Some(override_policy.clone()), &override_policy)
        .await;
    // The global floor hides all L2 candidates and its small hard pool cannot reach gamma.
    // 全局门槛隐藏全部 L2 候选，较小硬池不能触及伽马。
    let mut inherited = neutral.clone();
    inherited.replace_min_similarity = 0.99;
    inherited.replacement_top_k = 2;
    inherited.hard_dedupe_pool_top_k = 1;
    inherited.hard_dedupe_cosine_threshold = 0.65;
    test.save_global(&inherited).await;
    test.probe(&shared, "replacement-host-a", "host-a-reuse", &[], Some(2))
        .await;
    test.probe(&shared, "replacement-host-b", "host-b-new", &[], None)
        .await;
    // Fresh scope avoids the newly created axis-vector probe masking restored inheritance.
    // 新作用域避免刚创建的查询轴探针掩盖恢复继承效果。
    let restored = test.seeds("host-inheritance", &neutral).await;
    test.save_a(Some(override_policy.clone()), &override_policy)
        .await;
    test.save_global(&inherited).await;
    test.probe(
        &restored,
        "replacement-host-a",
        "before-inherit",
        &[],
        Some(2),
    )
    .await;
    test.save_a(None, &inherited).await;
    test.probe(&restored, "replacement-host-a", "after-inherit", &[], None)
        .await;
    // Freeze a hard-reuse command at its real embedding boundary, then publish all four different controls.
    // 在真实向量边界冻结硬复用命令，然后发布全部四个不同控制值。
    let frozen = test.seeds("inflight-replacement", &neutral).await;
    test.save_global(&override_policy).await;
    // This one-shot acknowledgement proves the command was admitted before the later save.
    // 此一次性确认证明命令早于后续保存准入。
    let (entered, observed) = tokio::sync::oneshot::channel();
    // Only successful policy publication releases the paused production command.
    // 只有策略发布成功才释放暂停的生产命令。
    let (resume, resumed) = tokio::sync::oneshot::channel();
    // Require the exact explicit replacement fixture variant.
    // 要求准确的明确替代夹具分支。
    let AnalysisFixture::ReplacementPolicy(slot) = &test.port.fixture else {
        panic!("replacement fixture required");
    };
    *slot.lock().expect("arm replacement pause") = Some(precheck_policy::IntentPause {
        entered,
        resume: resumed,
    });
    tokio::join!(
        test.probe(&frozen, "replacement-host-b", "frozen-old", &[], Some(2)),
        async {
            tokio::time::timeout(Duration::from_secs(10), observed)
                .await
                .expect("bounded replacement pause")
                .expect("replacement query reached");
            test.save_global(&inherited).await;
            resume.send(()).expect("resume replacement write");
        }
    );
    test.probe(&frozen, "replacement-host-b", "frozen-next", &[], None)
        .await;
    // The independent queue suite retains all preceding direct-write evidence and shares real storage.
    // 独立队列验收保留全部先前主动写入证据，并共享真实存储。
    postaction::verify(&test, &neutral, &inherited).await;
    // Rehydration must preserve every record, including actual superseded states and stored vectors.
    // 重新水合必须保留每条记录，包括实际已替代状态及已存向量。
    let records: Vec<MemoryRecord> = test
        .runtime
        .storage_snapshot()
        .expect("final replacement records")
        .dataset
        .memories;
    // Keep complete admitted/completed turns so rehydration cannot lose extraction details or source bodies.
    // 保留完整准入／完成回合，防止重新水合丢失提炼详情或来源正文。
    let turns = test
        .runtime
        .storage_snapshot()
        .expect("final replacement turns")
        .dataset
        .turns;
    // Bootstrap from the same physical storage, not the prior in-memory snapshot.
    // 从相同物理存储引导，不使用先前内存快照。
    let reopened = NativeVmmCoreRuntime::bootstrap(test.port.clone(), test.data_plane.clone())
        .await
        .expect("replacement rebuild");
    assert_eq!(
        reopened
            .storage_snapshot()
            .expect("rebuilt replacement records")
            .dataset
            .memories,
        records
    );
    assert_eq!(
        reopened
            .storage_snapshot()
            .expect("rebuilt replacement turns")
            .dataset
            .turns,
        turns
    );
    assert_eq!(
        reopened
            .runtime_policy()
            .await
            .expect("rebuilt replacement policy")
            .retrieval,
        inherited
    );
}
