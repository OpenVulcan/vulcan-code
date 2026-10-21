//! Saved decay controls verified against real search-time bounds and durable precheck adoption.
//! 对照真实搜索时间边界与持久预检采纳验证已保存衰减控制。

use super::*;
use vulcan_memorymesh_core::{MemoryLevel, MemoryRecord, MemoryScopeClass, WeibullDecayPolicy};
use vulcan_service_api::{VmmMemoryActionResponse, VmmNativeRetrievalPolicySummary};
use vulcan_vmm::{MemorySearchHit, SearchMemoryEventsRequest};

/// Pauses the explicitly armed single-query embedding, leaving normal document writes unaffected.
/// 暂停明确启用的单查询向量请求，不影响普通文档写入。
/// Takes the actual request and fixture-owned slot; returns immediately unless this is the declared query.
/// 接收真实请求及夹具持有槽；除声明查询外立即返回。
pub(super) async fn wait_for_query(
    request: &HostAiEmbeddingRequest,
    slot: &StdMutex<Option<precheck_policy::IntentPause>>,
) {
    if request.request.inputs.len() == 1
        && matches!(&request.request.inputs[0].content, EmbeddingInputContent::Text { text } if text == precheck_policy::QUERIES[0])
    {
        precheck_policy::wait_for_intent(slot).await;
    }
}

/// Saves a complete global decay-bearing retrieval group through the real product action.
/// 通过真实产品动作保存包含衰减设置的完整全局检索组。
/// Takes the isolated service and desired group; returns after validating the complete acknowledgement.
/// 接收隔离服务及目标组；完整确认校验后返回。
async fn save_global(
    service: &crate::state::SharedServiceBridgeState,
    policy: &VmmNativeRetrievalPolicySummary,
) {
    // Saved input is independent of the later runtime policy used by the actual search.
    // 保存输入独立于随后实际搜索使用的运行时策略。
    let saved = policy_service::action(
        service,
        "native_retrieval_policy_update",
        serde_json::to_value(policy).expect("decay JSON"),
    )
    .await
    .expect("save decay global");
    let VmmMemoryActionResponse::NativeRuntimePolicy(saved) = saved else {
        panic!("expected decay policy response");
    };
    assert_eq!(saved.retrieval, *policy);
}

/// Saves or removes host A's explicit retrieval group while preserving independent inherited groups.
/// 保存或移除宿主 A 显式检索组，保留独立继承组。
/// Takes service, optional override and expected effective values; validates ownership and persistence projection.
/// 接收服务、可选覆盖及预期有效值；校验归属与持久化投影。
async fn save_a(
    service: &crate::state::SharedServiceBridgeState,
    retrieval: Option<VmmNativeRetrievalPolicySummary>,
    effective: &VmmNativeRetrievalPolicySummary,
) {
    // The product explicitly defines null as inheritance; no guessed alternate configuration is accepted.
    // 产品明确将空值定义为继承；不接纳猜测的替代配置。
    let overrides = VmmNativeHostProjectPolicyOverrides {
        retrieval,
        ..Default::default()
    };
    // Read the actual host-specific response envelope.
    // 读取真实宿主专属响应封套。
    let saved = policy_service::action(
        service,
        "native_host_project_policy_update",
        serde_json::json!({"hostProjectId":"decay-host-a", "overrides":overrides}),
    )
    .await
    .expect("save decay host");
    let VmmMemoryActionResponse::NativeHostProjectPolicy(saved) = saved else {
        panic!("expected decay host response");
    };
    assert_eq!(saved.host_project_id, "decay-host-a");
    assert_eq!(saved.overrides, overrides);
    assert_eq!(saved.effective.retrieval, *effective);
}

/// Executes the actual management search for the declared query and requested candidate count.
/// 为声明查询与请求候选数执行真实管理搜索。
/// Takes the real client, bound project/host and limit; returns exact hits after protocol assertions.
/// 接收真实客户端、已绑定项目／宿主及数量；协议断言后返回准确命中。
async fn search(
    client: &NativeVmmClient,
    project_id: u64,
    host: &str,
    top_k: u32,
) -> Vec<MemorySearchHit> {
    // Management searches intentionally carry no runtime session or synthetic Core timestamp.
    // 管理搜索有意不携带运行时会话或合成 Core 时间戳。
    let response = client
        .search_memory_events(SearchMemoryEventsRequest {
            runtime_session_id: None,
            user_id: 1,
            project_id,
            host_project_id: host.to_owned(),
            queries: vec![precheck_policy::QUERIES[0].to_owned()],
            top_k,
            min_score: None,
        })
        .await
        .expect("actual decay search");
    assert_eq!(response.results.len(), 1);
    assert_eq!(response.results[0].query, precheck_policy::QUERIES[0]);
    assert_eq!(response.results[0].query_index, 0);
    assert_eq!(response.results[0].hits.len(), top_k as usize);
    response
        .results
        .into_iter()
        .next()
        .expect("single decay result group")
        .hits
}

/// Resolves the latest declared lifecycle clock anchor from one real stored memory.
/// 从一条真实存储记忆解析最新声明生命周期时钟锚点。
/// Takes immutable memory facts and returns creation time or a newer recorded lifecycle event.
/// 接收不可变记忆事实，返回创建时间或更新的已记录生命周期事件。
fn anchor(memory: &MemoryRecord) -> i64 {
    [
        memory.last_reinforced_at_ns,
        memory.last_adopted_at_ns,
        memory.last_recalled_at_ns,
    ]
    .into_iter()
    .flatten()
    .fold(memory.created_at_ns, i64::max)
}

/// Computes only fixed, noneditable scale factors for the verified stable environment-scoped seed records.
/// 仅为已验证稳定级环境作用域种子记录计算不可编辑尺度因子。
/// Takes live memory and resolved internal policy; returns the fixed factors including actual refresh weight.
/// 接收实时记忆与已解析内部策略；返回包含实际刷新权重的固定因子。
fn fixed_scale(memory: &MemoryRecord, internal: &WeibullDecayPolicy) -> f64 {
    assert_eq!(memory.memory_level, MemoryLevel::Stable);
    assert_eq!(memory.scope_class, MemoryScopeClass::Environment);
    assert_eq!(memory.priority, 60);
    internal.level_scales[2]
        * internal.priority_scales[1]
        * internal.scope_scales[1]
        * (1.0 + internal.refresh_weight_factor * memory.refresh_weight as f64)
}

/// Computes an independent mathematical bound using saved public controls, not the production decay function.
/// 使用已保存公开控制项独立计算数学边界，不调用生产衰减函数。
/// Takes real memory, fixed internal factors, expected public settings and bounded wall time; returns a multiplier.
/// 接收真实记忆、固定内部因子、预期公开设置及有界墙钟；返回倍率。
fn multiplier(
    memory: &MemoryRecord,
    internal: &WeibullDecayPolicy,
    expected: &VmmNativeRetrievalPolicySummary,
    time: i64,
) -> f64 {
    if !expected.weibull_enabled {
        return 1.0;
    }
    assert!(!memory.decay_disabled);
    // All clocks come from real calls after the seed/adoption, and tests fail explicitly if wall time regresses.
    // 所有时钟来自种子／采纳后的真实调用，墙钟倒退时测试明确失败。
    let age_hours = (time - anchor(memory)) as f64 / 3_600_000_000_000.0;
    assert!(age_hours > 0.0);
    // Editable controls are taken from the requested group so incorrect mapping cannot validate itself.
    // 可编辑控制取自请求组，避免错误映射自我验证。
    let scale = expected.weibull_scale_hours
        * fixed_scale(memory, internal)
        * (1.0 + expected.weibull_reinforce_weight * (memory.reinforcement_count as f64).ln_1p())
        * (1.0 + expected.weibull_cross_session_boost * memory.cross_session_adopted_count as f64);
    (-(age_hours / scale).powf(expected.weibull_shape))
        .exp()
        .clamp(expected.weibull_min_multiplier, 1.0)
}

/// Verifies each actual result against independent before/after decay bounds and returns scores in baseline order.
/// 对照独立前后衰减边界验证每条真实结果，并按基线顺序返回分值。
/// Takes real runtime/client, project/host, expected saved policy and disabled baseline; never rewrites source state.
/// 接收真实运行时／客户端、项目／宿主、预期保存策略及关闭基线；绝不改写来源状态。
async fn check(
    client: &NativeVmmClient,
    runtime: &NativeVmmCoreRuntime,
    project_id: u64,
    host: &str,
    expected: &VmmNativeRetrievalPolicySummary,
    baseline: &[MemorySearchHit],
) -> Vec<f64> {
    // Read authoritative lifecycle counters before this nonmutating management search.
    // 在本次不产生变更的管理搜索前读取权威生命周期计数。
    let memories = runtime
        .storage_snapshot()
        .expect("decay source facts")
        .dataset
        .memories;
    // Only noneditable factors are consumed from runtime policy; the six tested controls remain independent.
    // 仅从运行时策略消费不可编辑因子；六个被测控制项保持独立。
    let (policy, _, _) = runtime
        .active_memory_context(host)
        .await
        .expect("decay internal factors");
    // The actual Core timestamp is selected inside this interval, before the embedding request.
    // 实际 Core 时间戳在此区间内、向量请求之前选定。
    let before = now_ns().expect("decay lower clock bound");
    // Read production results, not a manually issued Core command.
    // 读取生产结果，而不是手工发出的 Core 命令。
    let hits = search(client, project_id, host, 3).await;
    // Finishing time supplies the conservative later-age/lower-score bound.
    // 完成时间提供保守的较晚年龄／较低分值边界。
    let after = now_ns().expect("decay upper clock bound");
    assert!(after >= before);
    baseline
        .iter()
        .map(|base| {
            // Resolve the one exact public identity in both production results and stored facts.
            // 在生产结果与存储事实中解析同一个准确公开身份。
            let hit = hits
                .iter()
                .find(|hit| hit.memory_id == base.memory_id)
                .expect("same decay hit");
            // A stable ID is the sole mapping, never content or an alternate field.
            // 稳定 ID 是唯一映射，绝不使用内容或其他字段。
            let memory = memories
                .iter()
                .find(|memory| memory.id.as_str() == format!("vmm-memory-{}", hit.memory_id))
                .expect("same decay record");
            // Context evidence would require a separate expected term, so assert this fixture has none.
            // 情境证据需要独立预期项，因此断言本夹具没有该项。
            let retrieval = hit.retrieval.as_ref().expect("decay provenance");
            assert_eq!(retrieval.origin, "vector_search");
            assert!(retrieval.matched_context_values.is_empty());
            assert_eq!(retrieval.matched_context_score_delta, Some(0.0));
            // Baseline producer scores are observed independently with decay disabled.
            // 在衰减关闭时独立观察基线生产者分数。
            let base_score = base
                .retrieval
                .as_ref()
                .expect("baseline provenance")
                .final_score;
            // Mathematical approximation tolerance is far smaller than the field effects exercised here.
            // 数学近似容差远小于此处被测字段效果。
            let lower =
                base_score * multiplier(memory, &policy.retrieval_policy.decay, expected, after);
            // Earlier age provides the upper bound without assuming an exact scheduling delay.
            // 较早年龄提供上界，不假定准确调度延迟。
            let upper =
                base_score * multiplier(memory, &policy.retrieval_policy.decay, expected, before);
            assert!(
                retrieval.final_score >= lower - 1e-9 && retrieval.final_score <= upper + 1e-9,
                "decay score {} outside [{lower}, {upper}] for {} with {expected:?}",
                retrieval.final_score,
                hit.memory_id
            );
            retrieval.final_score
        })
        .collect()
}

/// Calibrates a non-clamped test scale against actual elapsed lifecycle age, without changing clocks or records.
/// 按实际已过生命周期年龄校准不触底测试尺度，不修改时钟或记录。
/// Takes runtime and mutable public settings; selects age/scale near two using only fixed scale factors.
/// 接收运行时及可变公开设置；仅用固定尺度因子将年龄／尺度选在二附近。
async fn calibrate(runtime: &NativeVmmCoreRuntime, settings: &mut VmmNativeRetrievalPolicySummary) {
    // All three seeds are written and adopted atomically; the first real record supplies the shared anchor.
    // 三条种子原子写入并原子采纳；首条真实记录提供共享锚点。
    let memories = runtime
        .storage_snapshot()
        .expect("calibration memories")
        .dataset
        .memories;
    // Resolve fixed source factors through the exact caller that will run the following search.
    // 通过将执行后续搜索的准确调用方解析固定来源因子。
    let (policy, _, _) = runtime
        .active_memory_context("decay-host-b")
        .await
        .expect("calibration policy");
    // Real elapsed age includes normal storage and inference execution, not a manufactured timestamp.
    // 真实已过年龄包含普通存储及推理执行，而不是制造的时间戳。
    let age_hours =
        (now_ns().expect("calibration time") - anchor(&memories[0])) as f64 / 3_600_000_000_000.0;
    assert!(age_hours > 0.0);
    settings.weibull_scale_hours =
        age_hours / (2.0 * fixed_scale(&memories[0], &policy.retrieval_policy.decay));
}

/// Performs real precheck acceptance and verifies durable same-session or cross-session reinforcement counts.
/// 执行真实预检接纳，并验证持久同会话或跨会话强化计数。
/// Takes client/runtime/project, session and expected counters; returns after all three accepted memories are checked.
/// 接收客户端／运行时／项目、会话及预期计数；检查三条已采纳记忆后返回。
async fn adopt(
    client: &NativeVmmClient,
    runtime: &NativeVmmCoreRuntime,
    project_id: u64,
    session: &str,
    count: u64,
    cross: u64,
) {
    // The normal positive precheck fixture selects only actual reviewer candidate numbers.
    // 普通肯定预检夹具仅选择实际审核候选编号。
    let response = client
        .pre_check(PreCheckRequest {
            host_project_id: "decay-host-b".to_owned(),
            project_id,
            user_id: 1,
            session_id: session.to_owned(),
            user_content: precheck_policy::USER_PROMPT.to_owned(),
            operation_id: uuid::Uuid::new_v4().to_string(),
            recall_mode: PreCheckRecallMode::PreCheckRecallModeFull,
        })
        .await
        .expect("real decay adoption");
    assert!(!response.degraded);
    assert!(response.should_inject);
    assert_eq!(response.context_items.len(), 3);
    for memory in runtime
        .storage_snapshot()
        .expect("adopted facts")
        .dataset
        .memories
    {
        assert_eq!(memory.reinforcement_count, count);
        assert_eq!(memory.adopted_count, count);
        assert_eq!(memory.recalled_count, count);
        assert_eq!(memory.cross_session_adopted_count, cross);
        assert_eq!(memory.refresh_weight, count + 1);
        assert_eq!(memory.last_recalled_at_ns, memory.last_adopted_at_ns);
        assert_eq!(memory.last_reinforced_at_ns, memory.last_adopted_at_ns);
        assert_eq!(memory.last_adopted_at_ns, Some(memory.updated_at_ns));
    }
}

/// Exercises all six decay controls through real saves, searches, adoption, host isolation and admitted snapshots.
/// 通过真实保存、搜索、采纳、宿主隔离及已准入快照验证全部六个衰减控制项。
/// Takes private data-plane storage and declared capability; only deterministic inference output is substituted.
/// 接收私有数据面存储与声明能力；仅替代确定性推理输出。
pub(in crate::native_vmm) async fn verify(
    data_plane: VmmDataPlaneHandle,
    capability: InferenceCapabilitySnapshot,
) {
    // This explicit mode reuses the verified positive precheck inputs and distinct document vectors.
    // 此明确模式复用已验证肯定预检输入及不同文档向量。
    let port = Arc::new(CapturingAnalysisPort {
        inner: MockHostAiPort::new(),
        requests: StdMutex::new(Vec::new()),
        reject_next_turn: AtomicBool::new(false),
        fixture: AnalysisFixture::WeibullPolicy(StdMutex::new(None)),
        embedding_requests: StdMutex::new(Vec::new()),
        profile_instruction_pause: StdMutex::new(None),
        postaction_pause: StdMutex::new(None),
    });
    port.inner
        .set_capability_snapshot(capability)
        .expect("decay capability");
    // Bootstrap actual host storage, not a reference or directly edited memory dataset.
    // 引导真实宿主存储，不使用参考或直接编辑的记忆数据集。
    let runtime = Arc::new(
        NativeVmmCoreRuntime::bootstrap(port.clone(), data_plane.clone())
            .await
            .expect("decay runtime"),
    );
    // Both callers read identical physical records under independent policy groups.
    // 两调用方在独立策略组下读取相同物理记录。
    let project = data_plane
        .create_project("Decay policy", "decay-policy")
        .await
        .expect("decay project");
    for host in ["decay-host-a", "decay-host-b"] {
        data_plane
            .bind_vulcan_project(host, project.id)
            .await
            .expect("decay binding");
    }
    // Share one production operation gate between saves and public client operations.
    // 在保存与公开客户端操作间共享一个生产操作锁。
    let gate = Arc::new(RwLock::new(()));
    // The real service owns the same runtime that evaluates the search command snapshots.
    // 真实服务持有评估搜索命令快照的同一运行时。
    let service = policy_service::service(runtime.clone(), data_plane.clone(), gate.clone());
    // All unrelated optional ranking stages are explicitly disabled to isolate lifecycle scoring.
    // 明确关闭所有无关可选排序阶段，隔离生命周期评分。
    let mut global = runtime
        .runtime_policy()
        .await
        .expect("decay defaults")
        .retrieval;
    global.hybrid_enabled = false;
    global.rerank_top_n = 0;
    global.mmr_enabled = false;
    global.weibull_enabled = false;
    global.weibull_shape = 1.0;
    global.weibull_min_multiplier = 0.0;
    global.weibull_reinforce_weight = 0.0;
    global.weibull_cross_session_boost = 0.0;
    save_global(&service, &global).await;
    // Permit all three explicit precheck candidates so actual adoption can establish lifecycle counters.
    // 接纳三个明确预检候选，使实际采纳可以建立生命周期计数。
    let mut recall = runtime
        .runtime_policy()
        .await
        .expect("decay recall defaults")
        .recall;
    recall.precheck_recall_top_k = 3;
    recall.precheck_review_candidate_limit = 3;
    recall.precheck_min_review_score = 0.0;
    recall.max_search_queries = 1;
    // Keep all writes, searches and adoption inside the actual public client.
    // 将所有写入、搜索及采纳保留在实际公开客户端内。
    let client = NativeVmmClient::new(
        runtime.clone(),
        data_plane.clone(),
        gate,
        Arc::new(AtomicBool::new(true)),
        Arc::new(Notify::new()),
    );
    // Seed normally so the source session is authoritative for the subsequent same-session adoption.
    // 正常准备种子，使来源会话成为随后同会话采纳的权威。
    let written = client
        .write_memories(WriteMemoriesRequest {
            host_project_id: "decay-host-b".to_owned(),
            project_id: project.id,
            user_id: 1,
            session_id: "decay-seed".to_owned(),
            items: precheck_policy::MARKERS
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
        .expect("normal decay seeds");
    assert_eq!(written.items.len(), 3);
    assert!(written.items.iter().all(|item| !item.deduped));
    // Direct writes retain their original multi-item search budget; narrow only the later precheck query count.
    // 主动写入保留原多条目搜索预算；仅收窄后续预检查询数量。
    let saved_recall = policy_service::action(
        &service,
        "native_runtime_policy_update",
        serde_json::to_value(&recall).expect("decay recall JSON"),
    )
    .await
    .expect("save decay adoption controls");
    let VmmMemoryActionResponse::NativeRuntimePolicy(saved_recall) = saved_recall else {
        panic!("expected saved recall response");
    };
    assert_eq!(saved_recall.recall, recall);
    // Obtain base scores from the real physical query with decay disabled.
    // 在衰减关闭时从真实物理查询取得基线分值。
    let baseline = search(&client, project.id, "decay-host-b", 3).await;
    global.weibull_enabled = true;
    global.weibull_scale_hours = 1e-9;
    global.weibull_min_multiplier = 0.25;
    save_global(&service, &global).await;
    check(
        &client,
        &runtime,
        project.id,
        "decay-host-b",
        &global,
        &baseline,
    )
    .await;
    // A single physical candidate explicitly skips decay even under an enabled floor-clamped policy.
    // 即使使用已启用且触底的策略，单个物理候选也明确跳过衰减。
    let single = search(&client, project.id, "decay-host-b", 1).await;
    assert_eq!(
        single[0]
            .retrieval
            .as_ref()
            .expect("single decay score")
            .final_score,
        baseline[0]
            .retrieval
            .as_ref()
            .expect("single base score")
            .final_score
    );
    global.weibull_min_multiplier = 0.75;
    save_global(&service, &global).await;
    check(
        &client,
        &runtime,
        project.id,
        "decay-host-b",
        &global,
        &baseline,
    )
    .await;
    global.weibull_min_multiplier = 0.0;
    calibrate(&runtime, &mut global).await;
    save_global(&service, &global).await;
    // Shape one and two must produce distinct bounded scores on the same real sources.
    // 形状一和二必须在相同真实来源上产生不同有界分值。
    let shape_one = check(
        &client,
        &runtime,
        project.id,
        "decay-host-b",
        &global,
        &baseline,
    )
    .await;
    global.weibull_shape = 2.0;
    save_global(&service, &global).await;
    // Later age with a ratio already above one makes the larger shape strictly more decaying.
    // 年龄更晚且比例已经超过一，使较大形状严格增强衰减。
    let shape_two = check(
        &client,
        &runtime,
        project.id,
        "decay-host-b",
        &global,
        &baseline,
    )
    .await;
    assert!(shape_two[0] < shape_one[0]);
    global.weibull_scale_hours *= 1_000_000.0;
    save_global(&service, &global).await;
    // A much larger real scale restores near-baseline relevance without disabling the stage.
    // 大幅增加真实尺度可恢复接近基线的相关性，而不关闭该阶段。
    let large_scale = check(
        &client,
        &runtime,
        project.id,
        "decay-host-b",
        &global,
        &baseline,
    )
    .await;
    assert!(large_scale[0] > shape_one[0]);

    global.weibull_enabled = false;
    save_global(&service, &global).await;
    adopt(&client, &runtime, project.id, "decay-seed", 1, 0).await;
    global.weibull_enabled = true;
    global.weibull_shape = 1.0;
    calibrate(&runtime, &mut global).await;
    save_global(&service, &global).await;
    // Real same-session adoption establishes reinforcement while keeping cross-session count zero.
    // 真实同会话采纳建立强化计数，同时跨会话计数保持零。
    let unweighted = check(
        &client,
        &runtime,
        project.id,
        "decay-host-b",
        &global,
        &baseline,
    )
    .await;
    global.weibull_reinforce_weight = 20.0;
    save_global(&service, &global).await;
    // Compare the independently bounded effect of the actual nonzero reinforcement count.
    // 比较实际非零强化计数的独立有界效果。
    let reinforced = check(
        &client,
        &runtime,
        project.id,
        "decay-host-b",
        &global,
        &baseline,
    )
    .await;
    assert!(reinforced[0] > unweighted[0]);
    global.weibull_enabled = false;
    global.weibull_reinforce_weight = 0.0;
    save_global(&service, &global).await;
    adopt(&client, &runtime, project.id, "decay-other-session", 2, 1).await;
    // All later policy searches must preserve these fully adopted records exactly.
    // 后续所有策略搜索必须准确保留这些完成采纳的记录。
    let adopted = runtime
        .storage_snapshot()
        .expect("decay adopted baseline")
        .dataset
        .memories;
    global.weibull_enabled = true;
    calibrate(&runtime, &mut global).await;
    save_global(&service, &global).await;
    // Reinforcement weight stays zero so the next change isolates the real cross-session counter.
    // 强化权重保持零，使下一次变化隔离真实跨会话计数。
    let without_cross = check(
        &client,
        &runtime,
        project.id,
        "decay-host-b",
        &global,
        &baseline,
    )
    .await;
    global.weibull_cross_session_boost = 20.0;
    save_global(&service, &global).await;
    // The same adopted records now receive only the explicitly saved cross-session scale boost.
    // 同一批已采纳记录此时仅获得明确保存的跨会话尺度增益。
    let boosted = check(
        &client,
        &runtime,
        project.id,
        "decay-host-b",
        &global,
        &baseline,
    )
    .await;
    assert!(boosted[0] > without_cross[0]);

    global.weibull_scale_hours = 1e-9;
    global.weibull_min_multiplier = 0.25;
    save_global(&service, &global).await;
    // Host A's independent floor remains effective even when global decay is later disabled.
    // 即使随后关闭全局衰减，宿主 A 独立下限仍保持有效。
    let mut a = global.clone();
    a.weibull_min_multiplier = 0.75;
    save_a(&service, Some(a.clone()), &a).await;
    check(&client, &runtime, project.id, "decay-host-a", &a, &baseline).await;
    check(
        &client,
        &runtime,
        project.id,
        "decay-host-b",
        &global,
        &baseline,
    )
    .await;
    global.weibull_enabled = false;
    save_global(&service, &global).await;
    check(&client, &runtime, project.id, "decay-host-a", &a, &baseline).await;
    check(
        &client,
        &runtime,
        project.id,
        "decay-host-b",
        &global,
        &baseline,
    )
    .await;
    save_a(&service, None, &global).await;
    check(
        &client,
        &runtime,
        project.id,
        "decay-host-a",
        &global,
        &baseline,
    )
    .await;
    global.weibull_enabled = true;
    save_global(&service, &global).await;
    // Pause the actual query after its timestamp/snapshot was selected, before its physical reads.
    // 在实际查询的时间戳／快照选定后、物理读取前暂停。
    let (entered, observed) = tokio::sync::oneshot::channel();
    // Only the acknowledged save releases this one admitted call.
    // 只有已确认保存释放此次已准入调用。
    let (resume, resumed) = tokio::sync::oneshot::channel();
    // The explicit decay fixture alone owns this pause.
    // 仅明确衰减夹具持有此暂停。
    let AnalysisFixture::WeibullPolicy(slot) = &port.fixture else {
        panic!("decay fixture required");
    };
    *slot.lock().expect("arm decay pause") = Some(precheck_policy::IntentPause {
        entered,
        resume: resumed,
    });
    // New defaults differ from the frozen active floor so accidental live reload cannot pass.
    // 新默认值不同于活动冻结下限，使意外实时重载无法通过。
    let mut next = global.clone();
    next.weibull_enabled = false;
    next.weibull_min_multiplier = 0.75;
    tokio::join!(
        check(
            &client,
            &runtime,
            project.id,
            "decay-host-b",
            &global,
            &baseline
        ),
        async {
            tokio::time::timeout(Duration::from_secs(10), observed)
                .await
                .expect("bounded decay pause")
                .expect("decay query reached");
            save_global(&service, &next).await;
            resume.send(()).expect("resume decay search");
        }
    );
    global = next;
    check(
        &client,
        &runtime,
        project.id,
        "decay-host-b",
        &global,
        &baseline,
    )
    .await;
    assert_eq!(
        runtime
            .storage_snapshot()
            .expect("unchanged adopted memory")
            .dataset
            .memories,
        adopted
    );
    // Reload authoritative storage and saved settings, proving lifecycle counters and the effective group survive.
    // 重载权威存储与保存设置，证明生命周期计数及有效组得以保留。
    let reopened = NativeVmmCoreRuntime::bootstrap(port, data_plane)
        .await
        .expect("decay reopen");
    assert_eq!(
        reopened
            .runtime_policy()
            .await
            .expect("decay reloaded policy")
            .retrieval,
        global
    );
    for memory in reopened
        .storage_snapshot()
        .expect("reloaded adopted records")
        .dataset
        .memories
    {
        assert_eq!(memory.reinforcement_count, 2);
        assert_eq!(memory.cross_session_adopted_count, 1);
        assert_eq!(memory.refresh_weight, 3);
    }
}
