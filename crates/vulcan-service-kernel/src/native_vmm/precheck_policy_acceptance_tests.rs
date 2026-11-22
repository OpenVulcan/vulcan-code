//! Public-service policy saves consumed by the real native PreCheck client and physical retrieval.
//! 由真实原生预检客户端及物理检索消费公开服务保存的策略。

use super::*;
use vulcan_service_api::{VmmMemoryActionResponse, VmmNativeRecallPolicySummary};

#[path = "precheck_history_policy_acceptance_tests.rs"]
pub(in crate::native_vmm) mod history;

/// The two explicit L1 query strings both occur in every seeded memory.
/// 两个明确 L1 查询字符串均出现在每条种子记忆中。
pub(super) const QUERIES: [&str; 2] = ["PolicyNeedle", "database"];
/// Distinct source markers create different vectors without bypassing actual hard deduplication.
/// 不同来源标记产生不同向量，不绕过实际硬去重。
pub(super) const MARKERS: [&str; 3] = ["fixture-alpha", "fixture-beta", "fixture-gamma"];
/// The actual user text stays identical across policy transitions.
/// 实际用户文本在策略变化之间保持相同。
pub(super) const USER_PROMPT: &str = "Recall the PolicyNeedle database configuration rules.";
/// Second canonical input in the same shell wake recovery test.
/// 同一 Shell 唤醒恢复测试中的第二条规范输入。
const FOLLOW_UP_PROMPT: &str = "Recall the PolicyNeedle database configuration rules again.";

/// Pauses one actual intent call after immutable command admission.
/// 在不可变命令准入后暂停一次实际意图调用。
pub(super) struct IntentPause {
    /// Notifies the test that the production Host AI boundary has been reached.
    /// 通知测试已到达生产 Host AI 边界。
    pub(super) entered: tokio::sync::oneshot::Sender<()>,
    /// Resumes this admitted call after the formal save completes.
    /// 正式保存完成后恢复此次已准入调用。
    pub(super) resume: tokio::sync::oneshot::Receiver<()>,
}

/// Waits only for an explicitly armed intent pause; unarmed calls continue immediately.
/// 仅等待明确启用的意图暂停；未启用时立即继续。
/// Takes the fixture-owned pause slot and returns after a bounded resume without retaining its mutex guard.
/// 接收夹具持有的暂停槽；在有界恢复后返回，不保留互斥锁守卫。
pub(super) async fn wait_for_intent(slot: &StdMutex<Option<IntentPause>>) {
    // Release the synchronous lock before any asynchronous wait.
    // 在任何异步等待前释放同步锁。
    let pause = slot.lock().expect("intent pause slot").take();
    if let Some(pause) = pause {
        pause.entered.send(()).expect("intent observer");
        tokio::time::timeout(Duration::from_secs(10), pause.resume)
            .await
            .expect("bounded intent resume")
            .expect("intent resume sender");
    }
}

/// Returns deterministic intent and review responses for the exact model-visible production payload.
/// 为准确的模型可见生产参数返回确定性意图及审核响应。
/// Selects only the candidate numbers supplied by production; unknown purposes fail the fixture.
/// 仅选择生产提供的候选编号；未知用途使夹具失败。
pub(super) fn response(purpose: &str, input: &serde_json::Value) -> serde_json::Value {
    match purpose {
        "precheck_l1" => {
            assert!(matches!(
                input["current_user_input"].as_str(),
                Some(USER_PROMPT | FOLLOW_UP_PROMPT)
            ));
            // The admitted query budget is observed at the real L1 boundary, not read from current saved settings.
            // 在真实 L1 边界观察已准入查询预算，不读取当前已保存设置。
            let count = usize::try_from(
                input["max_search_queries"]
                    .as_u64()
                    .expect("intent query limit"),
            )
            .expect("bounded query limit");
            assert!((1..=QUERIES.len()).contains(&count));
            serde_json::json!({"reason": "Recall the explicit database rules.", "need_memory": true, "queries": &QUERIES[..count]})
        }
        "precheck_l2" => {
            assert!(matches!(
                input["user_content"].as_str(),
                Some(USER_PROMPT | FOLLOW_UP_PROMPT)
            ));
            // Echo one-based public candidate numbers, never guessed memory IDs.
            // 回显从一开始的公开候选编号，绝不猜测记忆 ID。
            let candidates = input["candidates"]
                .as_array()
                .expect("actual review candidates");
            serde_json::json!({"selected_candidate_numbers": candidates.iter().map(|candidate| candidate["candidate_number"].clone()).collect::<Vec<_>>()})
        }
        "postaction_l2" => {
            // Normal direct writes always review new candidates, including an initially empty physical index.
            // 普通主动写入始终审核新候选，包括初始为空的物理索引。
            assert!(input["memory"].is_array());
            assert!(input.get("user").is_none());
            assert!(input.get("project").is_none());
            nonempty::response(purpose, input)
        }
        "postaction_l1" => {
            // History acceptance completes real queued turns with details but creates no unrelated memory candidates.
            // 历史验收通过真实队列完成回合并生成详情，不创建无关记忆候选。
            serde_json::json!({
                "user_input_kind": "statement",
                "turn_id": input["target_turn"]["turn_id"],
                "details": input["target_turn"]["raw_turn"]["user"],
                "memory_nodes": [],
                "profile_nodes": []
            })
        }
        _ => panic!("unexpected precheck policy purpose: {purpose}"),
    }
}

/// Reuses the declared route envelope but supplies distinct memory vectors and one shared query direction.
/// 复用已声明路由封套，提供不同记忆向量及一个共享查询方向。
/// Takes the real embedding request; returns aligned values while all index writes and similarity math stay real.
/// 接收真实向量请求；返回对齐值，全部索引写入及相似度数学保持真实。
pub(super) fn embedding_response(request: &HostAiEmbeddingRequest) -> EmbeddingResponse {
    // Keep every route, fingerprint, input ID and dimension from the established validating fixture.
    // 保留既有校验夹具的每项路由、指纹、输入 ID 及维度。
    let mut response = nonempty::embedding_response(request);
    for (result, input) in response.results.iter_mut().zip(&request.request.inputs) {
        // This native fixture admits text only; resource embedding is outside its declared scope.
        // 本原生夹具仅准入文本；资源向量不在其声明范围内。
        let EmbeddingInputContent::Text { text } = &input.content else {
            panic!("precheck policy fixture requires text");
        };
        // Shared first component makes every query relevant while orthogonal source components prevent dedupe.
        // 共享首分量使每个查询相关，正交来源分量避免去重。
        let mut values = vec![0.0; 1_536];
        values[0] = 1.0;
        if let Some(index) = MARKERS.iter().position(|marker| text.contains(marker)) {
            values[index + 1] = 1.0;
        } else {
            assert!(
                QUERIES.contains(&text.as_str()),
                "unexpected embedding input: {text}"
            );
        }
        result.embedding = EmbeddingPayload::Dense { values };
    }
    response
}

/// Saves a complete global recall group through the product service and checks its accepted values.
/// 通过产品服务保存完整全局召回组，并检查接纳值。
/// Takes isolated service state and recall settings; returns after validating the exact response variant.
/// 接收隔离服务状态及召回设置；验证准确响应变体后返回。
async fn save_global(
    service: &crate::state::SharedServiceBridgeState,
    recall: &VmmNativeRecallPolicySummary,
) {
    // The actual dispatcher performs validation, persistence and effective-value projection.
    // 实际派发器执行校验、持久化及有效值投影。
    let saved = policy_service::action(
        service,
        "native_runtime_policy_update",
        serde_json::to_value(recall).expect("precheck recall JSON"),
    )
    .await
    .expect("save precheck global");
    // An unrelated successful response cannot satisfy a recall-save assertion.
    // 无关成功响应不能满足召回保存断言。
    let VmmMemoryActionResponse::NativeRuntimePolicy(saved) = saved else {
        panic!("expected recall response");
    };
    assert_eq!(saved.recall, *recall);
}

/// Saves A's complete recall override or restores inheritance through the actual bound-host service.
/// 通过实际已绑定宿主服务保存 A 的完整召回覆盖或恢复继承。
/// Takes optional override and expected effective rules; checks exact ownership and returns without synthetic state.
/// 接收可选覆盖及预期有效规则；检查准确归属后返回，不构造合成状态。
async fn save_a(
    service: &crate::state::SharedServiceBridgeState,
    recall: Option<VmmNativeRecallPolicySummary>,
    effective: &VmmNativeRecallPolicySummary,
) {
    // Other policy groups remain inherited from their sole global owners.
    // 其他策略组保持继承其唯一全局归属方。
    let overrides = VmmNativeHostProjectPolicyOverrides {
        recall,
        ..Default::default()
    };
    // Persist the stable host identity independently of the shared memory-project ID.
    // 独立于共享记忆项目 ID 持久保存稳定宿主身份。
    let saved = policy_service::action(
        service,
        "native_host_project_policy_update",
        serde_json::json!({"hostProjectId":"precheck-host-a", "overrides": overrides}),
    )
    .await
    .expect("save precheck host");
    // Require the declared host response before comparing complete explicit and effective groups.
    // 比较完整显式组及有效组前要求声明的宿主响应。
    let VmmMemoryActionResponse::NativeHostProjectPolicy(saved) = saved else {
        panic!("expected host recall response");
    };
    assert_eq!(saved.host_project_id, "precheck-host-a");
    assert_eq!(saved.overrides, overrides);
    assert_eq!(saved.effective.recall, *effective);
}

/// Executes one actual PreCheck and independently checks model requests, query embeddings and returned IDs.
/// 执行一次实际预检，独立检查模型请求、查询向量及返回 ID。
/// Takes client, capture port, project, host and expected query/candidate counts; returns observed L2 candidates.
/// 接收客户端、捕获端口、项目、宿主及预期查询／候选数量；返回观察到的 L2 候选。
async fn check(
    client: &NativeVmmClient,
    port: &CapturingAnalysisPort,
    project_id: u64,
    host: &str,
    query_count: usize,
    candidate_count: usize,
) -> Vec<serde_json::Value> {
    // Capture only this command's subsequent model calls.
    // 仅捕获此命令后续模型调用。
    let before_llm = port.requests.lock().expect("precheck capture start").len();
    // Embedding requests separately prove the admitted L1 query count reaches physical retrieval.
    // 向量请求独立证明已准入 L1 查询数量抵达物理检索。
    let before_embedding = port
        .embedding_requests
        .lock()
        .expect("precheck embedding start")
        .len();
    // Exercise the same public native client used by application hooks, not a manually constructed Core command.
    // 使用应用 Hook 所用的同一公开原生客户端，而非手工构造 Core 命令。
    let result = client
        .pre_check(PreCheckRequest {
            host_project_id: host.to_owned(),
            project_id,
            user_id: 1,
            session_id: "precheck-policy-session".to_owned(),
            user_content: USER_PROMPT.to_owned(),
            operation_id: uuid::Uuid::new_v4().to_string(),
            recall_mode: PreCheckRecallMode::PreCheckRecallModeFull,
        })
        .await
        .expect("actual precheck policy command");
    assert!(!result.degraded);
    assert!(result.should_inject);
    assert_eq!(result.context_items.len(), candidate_count);
    // Own completed requests so no synchronous guard survives into another asynchronous case.
    // 独立持有完成请求，使同步守卫不存续至另一异步用例。
    let requests = port.requests.lock().expect("precheck captures")[before_llm..].to_vec();
    assert_eq!(
        requests
            .iter()
            .map(|request| request.purpose_id.as_str())
            .collect::<Vec<_>>(),
        vec!["precheck_l1", "precheck_l2"]
    );
    assert_eq!(
        analysis_input(&requests[0])["max_search_queries"],
        query_count
    );
    // Inspect the exact review payload emitted after real retrieval and caller-specific truncation.
    // 检查真实检索及调用方专属截断后发送的准确审核参数。
    let review = analysis_input(&requests[1]);
    assert_eq!(
        review["search_queries"],
        serde_json::json!(&QUERIES[..query_count])
    );
    // These candidates are production observations, not assembled from the fixture's seed list.
    // 这些候选是生产观察结果，不从夹具种子列表组装。
    let candidates = review["candidates"]
        .as_array()
        .expect("L2 candidates")
        .clone();
    assert_eq!(candidates.len(), candidate_count);
    assert_eq!(
        result
            .context_items
            .iter()
            .map(|item| format!("vmm-memory-{}", item.memory_id))
            .collect::<Vec<_>>(),
        candidates
            .iter()
            .map(|candidate| candidate["memory_id"]
                .as_str()
                .expect("candidate ID")
                .to_owned())
            .collect::<Vec<_>>()
    );
    // Every admitted query produces one actual embedding input before the production index reads.
    // 在生产索引读取前，每个已准入查询产生一个实际向量输入。
    let embeddings = port
        .embedding_requests
        .lock()
        .expect("precheck embedding capture");
    assert_eq!(embeddings.len(), before_embedding + 1);
    assert_eq!(
        embeddings[before_embedding].request.inputs.len(),
        query_count
    );
    candidates
}

/// Verifies caller isolation, four editable precheck controls and in-flight snapshot preservation on real storage.
/// 在真实存储上验证调用方隔离、四个可编辑预检控制项及在途快照保持。
/// Takes a private controller data plane and declared capability; only deterministic inference is substituted.
/// 接收私有控制器数据面及声明能力；仅替换确定性推理。
pub(in crate::native_vmm) async fn verify(
    data_plane: VmmDataPlaneHandle,
    capability: InferenceCapabilitySnapshot,
) {
    // The closed fixture handles only this explicit affirmative precheck flow.
    // 封闭夹具仅处理此明确肯定预检流程。
    let port = Arc::new(CapturingAnalysisPort {
        inner: MockHostAiPort::new(),
        requests: StdMutex::new(Vec::new()),
        reject_next_turn: AtomicBool::new(false),
        fixture: AnalysisFixture::PrecheckPolicy(StdMutex::new(None)),
        embedding_requests: StdMutex::new(Vec::new()),
        profile_instruction_pause: StdMutex::new(None),
        postaction_pause: StdMutex::new(None),
    });
    port.inner
        .set_capability_snapshot(capability)
        .expect("precheck capability");
    // Use the actual native bootstrap and private disk, never a reference storage adapter.
    // 使用实际原生引导及私有磁盘，绝不使用参考存储适配器。
    let runtime = Arc::new(
        NativeVmmCoreRuntime::bootstrap(port.clone(), data_plane.clone())
            .await
            .expect("precheck runtime"),
    );
    // Both host identities read the same three memories, making configuration the only varying input.
    // 两个宿主身份读取相同三条记忆，使配置成为唯一变化输入。
    let project = data_plane
        .create_project("Precheck policy", "precheck-policy")
        .await
        .expect("precheck project");
    for host in ["precheck-host-a", "precheck-host-b"] {
        data_plane
            .bind_vulcan_project(host, project.id)
            .await
            .expect("precheck binding");
    }
    // Reuse one operation gate across normal service and client entry points.
    // 在普通服务与客户端入口之间复用一个操作锁。
    let gate = Arc::new(RwLock::new(()));
    // The service owns exactly the same runtime that later admits the precheck commands.
    // 服务持有与随后准入预检命令完全相同的运行时。
    let service = policy_service::service(runtime.clone(), data_plane.clone(), gate.clone());
    // Isolate precheck count/threshold controls from optional reranking, time decay and diversity reshuffling.
    // 将预检数量／阈值控制与可选重排、时间衰减及多样性重排隔离。
    let mut ranking = runtime
        .runtime_policy()
        .await
        .expect("ranking defaults")
        .retrieval;
    ranking.rerank_top_n = 0;
    ranking.weibull_enabled = false;
    ranking.mmr_enabled = false;
    // These explicit test settings are saved through the same validated global service.
    // 这些明确测试设置经同一已校验全局服务保存。
    let saved_ranking = policy_service::action(
        &service,
        "native_retrieval_policy_update",
        serde_json::to_value(&ranking).expect("ranking JSON"),
    )
    .await
    .expect("ranking isolation save");
    // Require the runtime-policy envelope returned by the ranking action.
    // 要求排序动作返回的运行时策略封套。
    let VmmMemoryActionResponse::NativeRuntimePolicy(saved_ranking) = saved_ranking else {
        panic!("expected ranking response");
    };
    assert_eq!(saved_ranking.retrieval, ranking);
    // Begin with three candidates, all review scores admitted and two independently embedded queries.
    // 从三个候选、接纳全部审核分值及两个独立嵌入查询开始。
    let mut global = saved_ranking.recall.clone();
    global.precheck_recall_top_k = 3;
    global.precheck_review_candidate_limit = 3;
    global.precheck_min_review_score = 0.0;
    global.max_search_queries = 2;
    // A's explicit group must survive later global edits while B keeps inheriting.
    // A 的显式组必须在后续全局编辑时保持，B 则继续继承。
    let mut a = global.clone();
    a.precheck_recall_top_k = 1;
    a.max_search_queries = 1;
    // The normal client owns all direct writes, retrieval commands and lifecycle observations.
    // 普通客户端持有全部主动写入、检索命令及生命周期观察。
    let client = Arc::new(NativeVmmClient::new(
        runtime.clone(),
        data_plane.clone(),
        gate,
        Arc::new(AtomicBool::new(true)),
        Arc::new(Notify::new()),
    ));
    // Seed through the public atomic write path; distinct embedding components must prevent real hard dedupe.
    // 通过公开原子写入路径准备种子；不同向量分量必须避免实际硬去重。
    let written = client
        .write_memories(WriteMemoriesRequest {
            host_project_id: "precheck-host-b".to_owned(),
            project_id: project.id,
            user_id: 1,
            session_id: "precheck-policy-seed".to_owned(),
            items: MARKERS
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
        .expect("real precheck seed writes");
    assert_eq!(written.items.len(), 3);
    assert!(written.items.iter().all(|item| !item.deduped));
    // Capture the complete original content/vector ownership before any recall-side lifecycle updates.
    // 在任何召回侧生命周期更新前捕获完整原始内容／向量归属。
    let original_memories = runtime
        .storage_snapshot()
        .expect("seeded memory snapshot")
        .dataset
        .memories;
    // Read each exact physical row before configuration changes; relational snapshots intentionally omit embeddings.
    // 配置变化前读取每条准确物理记录；关系快照有意省略向量。
    let mut original_vectors = Vec::new();
    for memory in &original_memories {
        // Resolve only the vector generation owned by this seeded record.
        // 仅解析此种子记录持有的向量代次。
        let binding = data_plane
            .resolve_vector_space(
                memory
                    .vector_space_id
                    .as_ref()
                    .expect("seed vector generation")
                    .as_str(),
            )
            .await
            .expect("seed binding read")
            .expect("seed binding");
        // Preserve the full physical response, not only vector length or the runtime copy.
        // 保留完整物理响应，不仅保存向量长度或运行时副本。
        let keys = vec![memory.id.to_string()];
        // Every seeded record must already have exactly one physical vector row.
        // 每条种子记录都必须已经具有恰好一条物理向量记录。
        let rows = data_plane
            .load_vectors(&binding, &keys)
            .await
            .expect("seed physical vectors");
        assert_eq!(rows.len(), 1);
        original_vectors.push((binding, keys, rows));
    }
    // Any later embedding must belong to one of the nine explicit query commands, never a settings save.
    // 后续任何向量调用必须属于九个明确查询命令之一，绝不能来自设置保存。
    let seed_embedding_calls = port
        .embedding_requests
        .lock()
        .expect("seed embedding count")
        .len();
    // Direct batch writes share the query ceiling; finish seeding under defaults before testing lower precheck limits.
    // 主动批量写入共享查询上限；先在默认值下完成种子准备，再测试较低预检限制。
    save_global(&service, &global).await;
    save_a(&service, Some(a.clone()), &a).await;
    // A reads one while B reads all three from the identical physical project.
    // A 从同一物理项目读取一条，B 则读取全部三条。
    let first_a = check(&client, &port, project.id, "precheck-host-a", 1, 1).await;
    // Retain observed review scores for an independent later threshold transition.
    // 保留观察到的审核分值，以进行后续独立阈值变化。
    let full = check(&client, &port, project.id, "precheck-host-b", 2, 3).await;
    assert_eq!(first_a[0]["memory_id"], full[0]["memory_id"]);
    assert!(full.iter().all(|candidate| {
        written
            .items
            .iter()
            .any(|item| candidate["memory_id"] == format!("vmm-memory-{}", item.memory_id))
    }));
    global.precheck_recall_top_k = 2;
    save_global(&service, &global).await;
    check(&client, &port, project.id, "precheck-host-b", 2, 2).await;
    assert_eq!(
        check(&client, &port, project.id, "precheck-host-a", 1, 1).await[0]["memory_id"],
        first_a[0]["memory_id"]
    );
    save_a(&service, None, &global).await;
    check(&client, &port, project.id, "precheck-host-a", 2, 2).await;
    global.precheck_recall_top_k = 3;
    global.precheck_review_candidate_limit = 1;
    save_global(&service, &global).await;
    check(&client, &port, project.id, "precheck-host-b", 2, 1).await;
    // Choose a strict interior threshold from independently observed scores and require actual candidate removal.
    // 从独立观察分值中选择严格内部阈值，并要求实际移除候选。
    let high = full[0]["score"].as_f64().expect("highest review score");
    // The lowest score must be distinct so this fixture can prove the threshold has an effect.
    // 最低分必须不同，使本夹具能够证明阈值有效。
    let low = full[2]["score"].as_f64().expect("lowest review score");
    assert!(
        high > low,
        "threshold needs distinct physical ranking scores"
    );
    global.precheck_review_candidate_limit = 3;
    global.precheck_min_review_score = (high + low) / 2.0;
    // Derive the expected survivors from the preceding real observation, not from the new response under test.
    // 从之前真实观察推导预期保留项，而不是从本次待测新响应推导。
    let expected = full
        .iter()
        .filter(|candidate| {
            candidate["score"].as_f64().expect("review score") >= global.precheck_min_review_score
        })
        .map(|candidate| candidate["memory_id"].clone())
        .collect::<Vec<_>>();
    assert!(!expected.is_empty() && expected.len() < 3);
    save_global(&service, &global).await;
    assert_eq!(
        check(
            &client,
            &port,
            project.id,
            "precheck-host-b",
            2,
            expected.len()
        )
        .await
        .iter()
        .map(|candidate| candidate["memory_id"].clone())
        .collect::<Vec<_>>(),
        expected
    );
    global.precheck_min_review_score = 0.0;
    save_global(&service, &global).await;
    // Arm an actual L1 boundary without wall-clock sleeps or forged command admission.
    // 启用实际 L1 边界，不使用墙上时钟休眠或伪造命令准入。
    let (entered, observed) = tokio::sync::oneshot::channel();
    // Resume only after the formal global save has acknowledged the next-command rules.
    // 仅在正式全局保存确认下一命令规则后恢复。
    let (resume, resumed) = tokio::sync::oneshot::channel();
    // This exact port is explicitly configured for the precheck policy fixture.
    // 此准确端口已明确配置为预检策略夹具。
    let AnalysisFixture::PrecheckPolicy(slot) = &port.fixture else {
        panic!("expected precheck fixture");
    };
    *slot.lock().expect("arm precheck pause") = Some(IntentPause {
        entered,
        resume: resumed,
    });
    // Prepare the next saved group independently of the currently admitted three-candidate/two-query command.
    // 独立于当前已准入的三候选／双查询命令，准备下一已保存组。
    let mut next = global.clone();
    next.precheck_recall_top_k = 1;
    next.precheck_review_candidate_limit = 1;
    next.max_search_queries = 1;
    // The first branch must retain its admitted old snapshot while the second changes only saved defaults.
    // 第一分支必须保留准入时旧快照，第二分支仅改变已保存默认值。
    let (_, ()) = tokio::join!(
        check(&client, &port, project.id, "precheck-host-b", 2, 3),
        async {
            tokio::time::timeout(Duration::from_secs(10), observed)
                .await
                .expect("bounded admitted intent")
                .expect("intent reached");
            save_global(&service, &next).await;
            resume.send(()).expect("resume admitted precheck");
        }
    );
    check(&client, &port, project.id, "precheck-host-b", 1, 1).await;
    // All seeded records stay durable; changing recall settings never re-embeds or removes memory content.
    // 所有种子记录保持持久；修改召回设置不会重新嵌入或移除记忆内容。
    let persisted: vulcan_memorymesh_core::StorageSnapshot = serde_json::from_str(
        &data_plane
            .load_core_snapshot_json()
            .await
            .expect("precheck persisted bytes")
            .expect("precheck persisted state"),
    )
    .expect("precheck persisted JSON");
    assert_eq!(persisted.dataset.memories.len(), 3);
    assert_eq!(
        port.embedding_requests
            .lock()
            .expect("final embedding count")
            .len(),
        seed_embedding_calls + 9
    );
    for original in original_memories {
        // Recall legitimately changes adoption timestamps and counters, but never these original storage facts.
        // 召回可以合法改变采纳时间戳与计数，但绝不能改变这些原始存储事实。
        let current = persisted
            .dataset
            .memories
            .iter()
            .find(|memory| memory.id == original.id)
            .expect("same seeded memory ID");
        assert_eq!(current.scope_id, original.scope_id);
        assert_eq!(current.summary, original.summary);
        assert_eq!(current.content, original.content);
        assert_eq!(current.created_at_ns, original.created_at_ns);
        assert_eq!(
            current.origin_session_source_id,
            original.origin_session_source_id
        );
        assert_eq!(current.vector_space_id, original.vector_space_id);
        assert!(current.embedding.is_none());
    }
    for (binding, keys, rows) in original_vectors {
        assert_eq!(
            data_plane
                .load_vectors(&binding, &keys)
                .await
                .expect("unchanged physical vectors"),
            rows
        );
    }
    verify_shell_wake_then_user_memory_round_trips(client, &runtime).await;
}

/// Exercises a real Shell wake followed by two canonical user inputs, recall retry, and durable PostAction.
/// 验证真实 Shell 唤醒结束后的两条规范用户输入、召回重试及持久 PostAction。
/// Uses the seeded native client and runtime; only inference responses are deterministic fixtures.
/// 使用已有种子的原生客户端及运行时，只有推理响应来自确定性夹具。
async fn verify_shell_wake_then_user_memory_round_trips(
    client: Arc<NativeVmmClient>,
    runtime: &NativeVmmCoreRuntime,
) {
    use crate::session_business_hook::{
        capture_business_hook_before_dispatch, prepare_user_root_hook_owner, shell_wake_identity,
    };
    use crate::session_runtime_registry_v1::{
        SessionFlushReason, SessionHandle, SessionMutationAdmission,
    };
    use crate::tests::test_support::{
        TestHostRuntimeRoot, test_agent_runtime_bridge, test_service_state_with_parent_session,
    };
    use vulcan_agent_runtime::agent_runtime::conversation_hooks::{
        ConversationTurnCompletionStatus, VmmWritebackHookEffect,
        vmm_recall_batch_effect_from_report,
    };
    use vulcan_agent_runtime::agent_runtime::vmm_hooks::{
        VmmHookConfigSnapshot, VmmHookRuntimeHandle, register_builtin_vmm_hooks,
    };
    use vulcan_hooks::SystemPromptHookPhase;
    use vulcan_host_tools::shell_job::{
        ShellJobFinal, ShellJobNotice, ShellJobRuntime, ShellJobStatus,
    };
    use vulcan_session_core::v1::{ClientActionId, ClientInputSubmission, TurnOutcome};
    use vulcan_workbench_model::PendingSessionPromptSource;
    use vulcan_workspace_store::workspace_store::load_session_v1_full;

    // Keep unrelated semantic noise inference outside the fixture's declared embedding inputs.
    // 将无关的语义噪声推理排除在夹具声明的向量输入之外。
    let mut settings = runtime.resource_catalog().await.unwrap().settings;
    settings.noise_semantic_enabled = false;
    runtime.update_resource_settings(settings).await.unwrap();

    // V1 records the same typed job completion and shell continuation the app uses.
    // V1 记录与应用一致的强类型任务完成及 Shell 续跑。
    let session_id = uuid::Uuid::new_v4();
    let root = TestHostRuntimeRoot::new(session_id);
    let locator = root.v1_storage_locator();
    let handle = SessionHandle::new(
        session_id,
        locator.clone(),
        load_session_v1_full(&locator).unwrap().document,
    );
    let service = test_service_state_with_parent_session(session_id);
    let catalog = service.project_resource_catalog_registry.clone();
    let state = Arc::new(StdMutex::new(service));
    ShellJobRuntime::global().requeue_notices(
        &session_id.to_string(),
        vec![ShellJobNotice {
            job_id: "completed-background-job".to_owned(),
            purpose: "Finish background work".to_owned(),
            command: "test".to_owned(),
            outcome: ShellJobFinal {
                status: ShellJobStatus::Completed,
                exit_code: Some(0),
                duration_ms: 1,
                finished_at_ms: 1,
                failure_reason: None,
            },
        }],
    );
    crate::shell_job_continuation::open_shell_job_continuation_turn(&state, &handle)
        .await
        .unwrap()
        .unwrap();
    let sink =
        crate::agent_stream::provider_v1_sink::ProviderV1StreamSink::new(handle.clone(), catalog);
    assert_eq!(sink.consume_shell_job_notices().unwrap(), 1);

    // Register the real recall and writeback hooks against this fixture's native client.
    // 将真实召回及回写 Hook 注册到夹具的原生客户端。
    let host = "precheck-host-b";
    let workspace = root.v1_session_directory().to_string_lossy().into_owned();
    let hooks = VmmHookRuntimeHandle::new(VmmHookConfigSnapshot {
        global: vulcan_vmm::VmmConfigLayer {
            backend_mode: Some(vulcan_vmm::VmmBackendMode::Native),
            enabled: Some(true),
            precheck_enabled: Some(true),
            memory_writeback_enabled: Some(true),
            implicit_memory_turns: Some(3),
            ..Default::default()
        },
        ..Default::default()
    });
    hooks.install_native_client(client.clone()).unwrap();
    let agent = test_agent_runtime_bridge();
    register_builtin_vmm_hooks(
        agent.lock_conversation_hooks().unwrap().dispatcher_mut(),
        hooks,
    )
    .unwrap();
    let conversation = Default::default();
    let prepare = |id: &str, source, prompt: &str| {
        agent
            .lock_conversation_hooks()
            .unwrap()
            .prepare_user_prompt_hook_requests(
                session_id,
                id,
                Some(host.to_owned()),
                workspace.clone(),
                source,
                SystemPromptHookPhase::FirstSession,
                prompt,
                &conversation,
                true,
            )
    };
    let wake_id = shell_wake_identity(handle.as_ref()).unwrap().unwrap();
    let wake = prepare(
        &wake_id,
        PendingSessionPromptSource::ShellJobContinuation,
        "Background work completed",
    );
    assert_eq!(wake.turn_id, wake_id);
    assert!(wake.deferred_user_event.is_none());
    assert!(
        !agent
            .lock_conversation_hooks()
            .unwrap()
            .has_active_turn(session_id)
    );
    capture_business_hook_before_dispatch(handle.as_ref(), &agent, session_id, &wake_id).unwrap();
    handle
        .read(|document| {
            let binding = document.metadata.business_hook.binding.as_ref().unwrap();
            assert_eq!(binding.hook_turn_id, wake_id);
            assert!(!binding.observes_user);
        })
        .unwrap();
    // Close the wake using the same host clock as its admission.
    // 使用与准入相同的宿主时钟闭合唤醒回合。
    let wake_finished_at = crate::current_v1_unix_millis().unwrap();
    handle
        .mutate_durable(
            SessionMutationAdmission::ExistingTurn,
            SessionFlushReason::TurnCompleted,
            |document| document.close_turn(TurnOutcome::Completed, 1, wake_finished_at),
        )
        .unwrap();

    // Recreate the stale process owner left by the old wake path. The new root has durable proof to discard it.
    // 重现旧唤醒路径遗留的进程内所有者。新根回合可凭持久证明清理它。
    let stale = prepare(
        "old-wake",
        PendingSessionPromptSource::AgentContinuation,
        "Previous shell wake",
    );
    let mut previous_hook_id = stale.turn_id;
    for (action, prompt) in [
        ("recall-first", USER_PROMPT),
        ("recall-second", FOLLOW_UP_PROMPT),
    ] {
        let now = crate::current_v1_unix_millis().unwrap();
        handle
            .submit_root_input(
                ClientInputSubmission::new(
                    ClientActionId::try_new(action).unwrap(),
                    prompt,
                    Vec::new(),
                ),
                now,
            )
            .unwrap();
        prepare_user_root_hook_owner(handle.as_ref(), &agent).unwrap();
        let start = prepare(action, PendingSessionPromptSource::UserFollowup, prompt);
        assert_ne!(start.turn_id, previous_hook_id);
        capture_business_hook_before_dispatch(handle.as_ref(), &agent, session_id, &start.turn_id)
            .unwrap();
        let recalled = start.precheck.clone().run().await.report.unwrap();
        assert!(
            vmm_recall_batch_effect_from_report(&recalled)
                .unwrap()
                .is_some()
        );
        // A repeat of one accepted input must not reinforce the same memories a second time.
        // 同一已接纳输入的重试不得再次强化相同记忆。
        let reinforced = runtime.storage_snapshot().unwrap().dataset.memories;
        let replay = start.precheck.run().await.report.unwrap();
        assert!(
            vmm_recall_batch_effect_from_report(&replay)
                .unwrap()
                .is_some()
        );
        assert_eq!(
            runtime.storage_snapshot().unwrap().dataset.memories,
            reinforced
        );

        handle
            .mutate_durable(
                SessionMutationAdmission::ExistingTurn,
                SessionFlushReason::TurnCompleted,
                |document| document.close_turn(TurnOutcome::Completed, 1, now + 1),
            )
            .unwrap();
        let (completed, writeback) = agent
            .lock_conversation_hooks()
            .unwrap()
            .prepare_turn_completion_dispatch_with_status(
                session_id,
                Some(host.to_owned()),
                "The confirmed database rule applies.",
                1,
                &conversation,
                ConversationTurnCompletionStatus::Completed,
                0,
                true,
            );
        assert_eq!(completed, start.turn_id);
        let effects = writeback.run().await.vmm_writeback_effects;
        assert!(
            matches!(
                effects.as_slice(),
                [VmmWritebackHookEffect::SessionStatus { .. }]
            ),
            "{effects:?}"
        );
        previous_hook_id = completed;
    }
    // Two canonical user writebacks are on disk; the shell wake and recall retry did not create turns.
    // 两次规范用户回写均已落盘，Shell 唤醒与召回重试均未写入回合。
    let turns = runtime.storage_snapshot().unwrap().dataset.turns;
    let user_turns = turns
        .iter()
        .filter(|turn| turn.session_source_id.as_str() == session_id.to_string())
        .collect::<Vec<_>>();
    assert_eq!(user_turns.len(), 2);
}
