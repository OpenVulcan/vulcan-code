//! Real component saves consumed by physical precheck and nonempty queued extraction.
//! 真实组件保存由物理预检和非空队列提炼消费。

use super::*;
use vulcan_service_api::{
    VmmNativeExtractedMemoryLevel as Level, VmmNativeExtractedMemoryStability as Stability,
    VmmNativeRecallPolicySummary,
};

/// Two declared physical query strings use the established probe vector axis.
/// 两个明确物理查询字符串使用既有探针向量轴。
const QUERIES: [&str; 2] = [
    "replacement-probe recall-one",
    "replacement-probe recall-two",
];
/// The current user input is constant across all policy transitions.
/// 当前用户输入在所有策略变换间保持固定。
const USER: &str = "Recall the explicitly stored HTTP policy decisions.";
/// A common session makes history differences attributable only to the saved policy.
/// 公共会话使历史差异仅归因于保存的策略。
const SESSION: &str = "http-recall-session";

/// Returns independent global expectations for all thirteen fields without using a frontend response.
/// 返回全部十三字段的独立全局预期，不使用前端响应。
fn global() -> VmmNativeRecallPolicySummary {
    VmmNativeRecallPolicySummary {
        precheck_history_turn_limit: 2,
        precheck_max_input_tokens: 6000,
        precheck_recall_top_k: 2,
        precheck_review_candidate_limit: 3,
        precheck_min_review_score: 0.0,
        max_search_queries: 2,
        postaction_history_turn_limit: 2,
        postaction_max_input_tokens: 6000,
        postaction_memory_priority: 73,
        postaction_memory_stability: Stability::Archival,
        postaction_memory_level: Level::Persistent,
        postaction_memory_refresh_weight: 9,
        postaction_memory_expiry_ms: 129_600_000,
    }
}

/// Returns the exact host override: small budgets must take precedence over larger history limits.
/// 返回准确宿主覆盖：小预算必须优先于较大历史上限。
fn host() -> VmmNativeRecallPolicySummary {
    VmmNativeRecallPolicySummary {
        precheck_history_turn_limit: 3,
        precheck_max_input_tokens: 1,
        precheck_recall_top_k: 3,
        precheck_review_candidate_limit: 1,
        precheck_min_review_score: 0.0,
        max_search_queries: 1,
        postaction_history_turn_limit: 3,
        postaction_max_input_tokens: 1,
        postaction_memory_priority: 81,
        postaction_memory_stability: Stability::Working,
        postaction_memory_level: Level::Phase,
        postaction_memory_refresh_weight: 7,
        postaction_memory_expiry_ms: 216_000_000,
    }
}

/// Returns the final two-candidate review window, independent of the three-candidate recall limit.
/// 返回最终两候选评审窗口，独立于三候选召回上限。
pub(super) fn window() -> VmmNativeRecallPolicySummary {
    VmmNativeRecallPolicySummary {
        precheck_history_turn_limit: 1,
        precheck_max_input_tokens: 4000,
        precheck_recall_top_k: 3,
        precheck_review_candidate_limit: 2,
        precheck_min_review_score: 0.0,
        max_search_queries: 2,
        postaction_history_turn_limit: 1,
        postaction_max_input_tokens: 4000,
        postaction_memory_priority: 62,
        postaction_memory_stability: Stability::Stable,
        postaction_memory_level: Level::Stable,
        postaction_memory_refresh_weight: 3,
        postaction_memory_expiry_ms: 259_200_000,
    }
}

/// Returns a high score threshold which excludes all three original physical seeds.
/// 返回高评分门槛，排除全部三条原始物理种子。
fn strict() -> VmmNativeRecallPolicySummary {
    VmmNativeRecallPolicySummary {
        precheck_review_candidate_limit: 3,
        precheck_min_review_score: 0.99,
        ..window()
    }
}

/// Responds only to actual precheck purposes, selecting the real public candidate numbers.
/// 仅响应实际预检用途，选择真实公开候选编号。
/// Takes the rendered purpose and input; never supplies search results or persisted records.
/// 接收渲染用途及输入；绝不提供搜索结果或持久记录。
pub(in crate::native_vmm) fn response(
    purpose: &str,
    input: &serde_json::Value,
) -> serde_json::Value {
    match purpose {
        "precheck_l1" => {
            assert_eq!(input["current_user_input"], USER);
            // Read the actual admitted query limit, bounded by this explicit two-query fixture.
            // 读取实际准入查询上限，以本明确双查询夹具为界。
            let count = usize::try_from(
                input["max_search_queries"]
                    .as_u64()
                    .expect("HTTP query count"),
            )
            .expect("HTTP bounded queries");
            assert!((1..=2).contains(&count));
            serde_json::json!({"reason":"Recall explicit decisions.","need_memory":true,"queries":&QUERIES[..count]})
        }
        "precheck_l2" => {
            assert_eq!(input["user_content"], USER);
            serde_json::json!({"selected_candidate_numbers":input["candidates"].as_array().expect("HTTP actual candidates").iter().map(|candidate|candidate["candidate_number"].clone()).collect::<Vec<_>>()})
        }
        _ => panic!("unexpected HTTP recall purpose: {purpose}"),
    }
}

/// Admits and completes one real source; returns its canonical identity and analysis details.
/// 准入并完成一个真实来源；返回其规范身份与提炼详情。
/// Takes an exact host/project, body and optional extraction expectation; validates actual model history.
/// 接收准确宿主／项目、正文及可选提炼预期；验证实际模型历史。
async fn analyze(
    test: &Acceptance,
    project: u64,
    caller: &str,
    body: &str,
    history: &[(String, String)],
    expected: Option<&VmmNativeRecallPolicySummary>,
) -> (String, String) {
    // The queue identity is unrelated to display names and always unique.
    // 队列身份独立于显示名称，始终唯一。
    let operation = uuid::Uuid::new_v4().to_string();
    test.client
        .post_action(PostActionRequest {
            host_project_id: caller.to_owned(),
            project_id: project,
            user_id: 1,
            session_id: SESSION.to_owned(),
            operation_id: operation.clone(),
            user_content: body.to_owned(),
            assistant_content: "Acknowledged.".to_owned(),
            ..Default::default()
        })
        .await
        .expect("HTTP recall source intake");
    // Capture after admission, before the real bounded queue drain.
    // 在准入后、真实有界队列排空前捕获。
    let start = test
        .port
        .requests
        .lock()
        .expect("HTTP recall L1 start")
        .len();
    assert!(
        test.client
            .drain_due_pending_analysis_jobs()
            .await
            .expect("HTTP recall drain")
    );
    // Success requires the actual durable receipt, not merely a successful intake.
    // 成功要求实际耐久回执，而不仅是成功准入。
    let job = test
        .data_plane
        .load_pending_analysis_job(&operation)
        .await
        .expect("HTTP recall job read")
        .expect("HTTP recall receipt");
    assert_eq!(
        job.state,
        vulcan_workspace_store::vmm_data_plane::VMM_PENDING_ANALYSIS_COMPLETED,
        "{:?}",
        job.last_error
    );
    assert_eq!(job.attempt_count, 0);
    assert!(job.payload_json.is_empty());
    // The job's stable ID is the only authority for the new turn.
    // 作业稳定 ID 是新回合的唯一权威。
    let id = job.turn_id.expect("HTTP recall turn ID");
    // Inspect the actual canonical turn and its owned memory after the real commit.
    // 在真实提交后检查实际规范回合及其所属记忆。
    let snapshot = test
        .runtime
        .storage_snapshot()
        .expect("HTTP recall materialized snapshot");
    // Locate the exact source without relying on ordering or guessed numeric IDs.
    // 定位准确来源，不依赖顺序或猜测数字 ID。
    let turn = snapshot
        .dataset
        .turns
        .iter()
        .find(|turn| turn.id.as_str() == id)
        .expect("HTTP recall canonical turn");
    assert_eq!(turn.status, TurnStatus::Completed);
    assert_eq!(turn.timeline[0].content, body);
    if let Some(policy) = expected {
        // Expected reference identities are computed from already completed sources, not current output.
        // 预期参考身份从已完成来源计算，不使用当前输出。
        let count = if policy.postaction_max_input_tokens == 1 {
            0
        } else {
            usize::try_from(policy.postaction_history_turn_limit).expect("HTTP history bound")
        };
        assert!(history.len() >= count);
        // Own requests so no synchronous capture lock crosses a later await.
        // 独立持有请求，使同步捕获锁不跨后续等待。
        let captures = test
            .port
            .requests
            .lock()
            .expect("HTTP recall extraction captures")[start..]
            .to_vec();
        assert_eq!(
            captures
                .iter()
                .map(|request| request.purpose_id.as_str())
                .collect::<Vec<_>>(),
            ["postaction_l1", "postaction_l2"]
        );
        // L1 must preserve the complete target even when its history budget is just one token.
        // 即使历史预算仅一个令牌，L1 仍须保留完整目标。
        let input = analysis_input(&captures[0]);
        assert_eq!(input["target_turn"]["turn_id"], id);
        assert_eq!(
            input["target_turn"]["raw_turn"],
            serde_json::json!({"user":body,"timeline":[],"assistant":"Acknowledged."})
        );
        assert_eq!(
            input["reference_turns"],
            serde_json::json!(
                history[history.len() - count..]
                    .iter()
                    .map(|(id, details)| serde_json::json!({"turn_id":id,"details":details}))
                    .collect::<Vec<_>>()
            )
        );
        // All five extraction properties must come from the real saved policy, not the model fixture.
        // 全部五项提炼属性必须来自真实保存策略，而非模型夹具。
        let memories = snapshot
            .dataset
            .memories
            .iter()
            .filter(|memory| {
                memory
                    .source_turn_id
                    .as_ref()
                    .is_some_and(|turn| turn.as_str() == id)
            })
            .collect::<Vec<_>>();
        assert_eq!(memories.len(), 1);
        // Only this exact source-owned record is inspected.
        // 仅检查此准确来源所属记录。
        let memory = memories[0];
        assert_eq!(memory.priority, policy.postaction_memory_priority);
        assert_eq!(
            memory.refresh_weight,
            u64::from(policy.postaction_memory_refresh_weight)
        );
        assert_eq!(
            memory.expires_at_ns,
            Some(
                turn.created_at_ns
                    + i64::try_from(policy.postaction_memory_expiry_ms).expect("HTTP expiry")
                        * 1_000_000
            )
        );
        assert_eq!(
            memory.scope_id,
            project_scope_id(project).expect("HTTP extraction scope")
        );
        assert_eq!(
            memory.stability,
            match policy.postaction_memory_stability {
                Stability::Working => vulcan_memorymesh_core::MemoryStability::Working,
                Stability::Stable => vulcan_memorymesh_core::MemoryStability::Stable,
                Stability::Archival => vulcan_memorymesh_core::MemoryStability::Archival,
            }
        );
        assert_eq!(
            memory.memory_level,
            match policy.postaction_memory_level {
                Level::Session => vulcan_memorymesh_core::MemoryLevel::Session,
                Level::Phase => vulcan_memorymesh_core::MemoryLevel::Phase,
                Level::Stable => vulcan_memorymesh_core::MemoryLevel::Stable,
                Level::Persistent => vulcan_memorymesh_core::MemoryLevel::Persistent,
            }
        );
    }
    (
        id,
        turn.analysis_details
            .clone()
            .expect("HTTP completed details"),
    )
}

/// Executes a real precheck, validating independent history, query, candidate and injection expectations.
/// 执行真实预检，验证独立历史、查询、候选和注入预期。
/// Takes exact seeded ownership, saved policy, completed sources and expected candidate count.
/// 接收准确种子归属、保存策略、已完成来源和预期候选数量。
async fn check(
    test: &Acceptance,
    seeds: &Seeds,
    caller: &str,
    policy: &VmmNativeRecallPolicySummary,
    history: &[(String, String)],
    count: usize,
) {
    // Capture only the current physical command's LLM and embedding work.
    // 仅捕获当前物理命令的模型及向量工作。
    let start = test
        .port
        .requests
        .lock()
        .expect("HTTP precheck start")
        .len();
    // Physical query count is observed independently of the L1 response.
    // 独立于 L1 响应观察物理查询数量。
    let embedding_start = test
        .port
        .embedding_requests
        .lock()
        .expect("HTTP query start")
        .len();
    // Use the application hook's public native client entry, never a hand-built Core command.
    // 使用应用 Hook 的公开原生客户端入口，绝不手工构造 Core 命令。
    let result = test
        .client
        .pre_check(PreCheckRequest {
            host_project_id: caller.to_owned(),
            project_id: seeds.project_id,
            user_id: 1,
            session_id: SESSION.to_owned(),
            user_content: USER.to_owned(),
            operation_id: uuid::Uuid::new_v4().to_string(),
            recall_mode: PreCheckRecallMode::PreCheckRecallModeFull,
        })
        .await
        .expect("HTTP actual precheck");
    assert!(!result.degraded);
    assert_eq!(result.should_inject, count > 0);
    assert_eq!(result.context_items.len(), count);
    // Own actual L1/L2 inputs after the completed command.
    // 在命令完成后独立持有实际 L1／L2 输入。
    let captures = test.port.requests.lock().expect("HTTP precheck captures")[start..].to_vec();
    assert_eq!(captures.len(), if count == 0 { 1 } else { 2 });
    assert_eq!(captures[0].purpose_id, "precheck_l1");
    // Historical budget one keeps only the latest source even though the declared window is larger.
    // 历史预算一仅保留最新来源，即使声明窗口更大。
    let history_count = if policy.precheck_max_input_tokens == 1 {
        1
    } else {
        usize::try_from(policy.precheck_history_turn_limit).expect("HTTP precheck history bound")
    };
    // Compare exact existing source identities and details in chronological suffix order.
    // 按时间正序后缀比较准确既有来源身份及详情。
    let intent = analysis_input(&captures[0]);
    assert_eq!(intent["recent_turns"], serde_json::json!(history[history.len()-history_count..].iter().map(|(id,details)|serde_json::json!({"turn_id":parse_core_numeric_id(id,"vmm-turn-").expect("HTTP numeric source"),"content_type":"DETAILS","content":details})).collect::<Vec<_>>()));
    assert_eq!(intent["max_search_queries"], policy.max_search_queries);
    // These three seed scores are independent normalized cosine expectations.
    // 这三个种子分数是独立的归一化余弦预期。
    let scores = [0.95, 0.875, 0.8];
    if count > 0 {
        assert_eq!(captures[1].purpose_id, "precheck_l2");
        // Candidate order and IDs must come from actual retrieval over the original physical seeds.
        // 候选顺序及 ID 必须来自原始物理种子上的实际检索。
        let review = analysis_input(&captures[1]);
        assert_eq!(
            review["search_queries"],
            serde_json::json!(&QUERIES[..policy.max_search_queries as usize])
        );
        assert_eq!(
            review["candidates"]
                .as_array()
                .expect("HTTP candidates")
                .len(),
            count
        );
        for (index, item) in result.context_items.iter().enumerate() {
            assert_eq!(
                format!("vmm-memory-{}", item.memory_id),
                seeds.ids[index].as_str()
            );
            assert_eq!(
                review["candidates"][index]["memory_id"],
                seeds.ids[index].as_str()
            );
            assert!(
                (review["candidates"][index]["score"]
                    .as_f64()
                    .expect("HTTP score")
                    - scores[index])
                    .abs()
                    < 1e-6
            );
        }
    }
    // Each actual command produces exactly one batch containing the admitted number of query vectors.
    // 每次实际命令恰好生成一批向量，其中包含准入数量的查询。
    let embeddings = test
        .port
        .embedding_requests
        .lock()
        .expect("HTTP query vectors");
    assert_eq!(embeddings.len(), embedding_start + 1);
    assert_eq!(
        embeddings[embedding_start].request.inputs.len(),
        policy.max_search_queries as usize
    );
}

/// Verifies five UI phases across isolated physical projects while preserving unrelated groups.
/// 在隔离物理项目中验证五个界面阶段，同时保留无关配置组。
/// Takes the live private environment and handshake directory; returns after all command receipts succeed.
/// 接收联网私有环境与握手目录；全部命令回执成功后返回。
pub(super) async fn verify(test: &Acceptance, root: &Path) {
    // Seed all scopes before UI query limits are reduced; ordinary writes share that same admission limit.
    // 在界面降低查询上限前准备全部作用域；普通写入共享该准入上限。
    let initial = test
        .runtime
        .runtime_policy()
        .await
        .expect("HTTP recall initial");
    // Independent projects prevent earlier extracted probe vectors from affecting later review thresholds.
    // 独立项目防止之前提取的探针向量影响后续评审门槛。
    let mut seeds = Vec::new();
    for index in 0..5 {
        seeds.push(
            test.seeds(&format!("http-recall-{index}"), &initial.retrieval)
                .await,
        );
    }
    // Complete all ordinary seed batches before the frontend may lower the shared query admission limit.
    // 在前端允许降低共享查询准入上限之前，完成全部普通种子批次。
    wait_phase(root, "recall-setup").await;
    acknowledge(root, "recall-setup");
    // Exact expected policies are independent of frontend phase markers.
    // 准确预期策略独立于前端阶段标记。
    let global = global();
    // A's whole override must survive every subsequent global save until explicit inheritance restoration.
    // A 的整组覆盖必须在全部后续全局保存后保持，直到显式恢复继承。
    let host = host();
    // The strict threshold excludes seeds without treating an empty result as an inference failure.
    // 严格门槛排除种子，但不将空结果视为推理失败。
    let strict = strict();
    // The last policy isolates the review window from recall TopK and a disabled threshold.
    // 最后策略将评审窗口与召回上限及关闭的门槛隔离。
    let window = window();
    // Each tuple declares stage, whole global/host groups, and independent caller result counts.
    // 每个元组声明阶段、完整全局／宿主组，以及独立调用方结果数量。
    let phases = [
        (
            "recall-global",
            &global,
            None,
            vec![("replacement-host-b", &global, 2)],
        ),
        (
            "recall-host",
            &global,
            Some(&host),
            vec![
                ("replacement-host-a", &host, 1),
                ("replacement-host-b", &global, 2),
            ],
        ),
        (
            "recall-strict",
            &strict,
            Some(&host),
            vec![
                ("replacement-host-a", &host, 1),
                ("replacement-host-b", &strict, 0),
            ],
        ),
        (
            "recall-inherit",
            &strict,
            None,
            vec![("replacement-host-a", &strict, 0)],
        ),
        (
            "recall-window",
            &window,
            None,
            vec![("replacement-host-b", &window, 2)],
        ),
    ];
    for ((phase, global, host, callers), seeds) in phases.into_iter().zip(seeds) {
        wait_phase(root, phase).await;
        // Reload actual global state; non-recall groups must remain byte-for-byte equivalent as typed values.
        // 重载实际全局状态；非召回组作为强类型值必须完整相同。
        let saved = test
            .runtime
            .runtime_policy()
            .await
            .expect("HTTP recall saved");
        assert_eq!(&saved.recall, global);
        assert_eq!(saved.retrieval, initial.retrieval);
        assert_eq!(saved.text, initial.text);
        assert_eq!(saved.profiles, initial.profiles);
        assert_eq!(saved.retention, initial.retention);
        // Binding changes select only physical test scope, never host-policy identity.
        // 绑定变化仅选择物理测试作用域，绝不改变宿主策略身份。
        for caller in ["replacement-host-a", "replacement-host-b"] {
            test.data_plane
                .bind_vulcan_project(caller, seeds.project_id)
                .await
                .expect("HTTP recall scope binding");
        }
        // Validate explicit host override ownership before executing commands.
        // 在执行命令前验证显式宿主覆盖归属。
        let a = test
            .runtime
            .host_project_policy("replacement-host-a")
            .await
            .expect("HTTP recall A");
        assert_eq!(a.overrides.recall.as_ref(), host);
        assert_eq!(&a.effective.recall, host.unwrap_or(global));
        assert!(
            test.runtime
                .host_project_policy("replacement-host-b")
                .await
                .expect("HTTP recall B")
                .overrides
                .recall
                .is_none()
        );
        // Three normal completed sources provide a known chronological history for both callers.
        // 三条正常完成来源为两调用方提供已知时间正序历史。
        let mut history = Vec::new();
        for index in 0..3 {
            history.push(
                analyze(
                    test,
                    seeds.project_id,
                    "replacement-host-b",
                    &format!("HTTP history {phase} source {index}."),
                    &history,
                    None,
                )
                .await,
            );
        }
        // Run every precheck before extraction can add any higher-scoring probe memory.
        // 在提炼可能增加更高分探针记忆前运行全部预检。
        for (caller, policy, count) in &callers {
            check(test, &seeds, caller, policy, &history, *count).await;
        }
        for (caller, policy, _) in callers {
            // Unique summaries avoid exact-content deduplication while retaining the declared probe vector.
            // 唯一摘要避免正文精确去重，同时保留声明的探针向量。
            let body = format!("{PROBE} HTTP extraction {phase} {caller}");
            history.push(
                analyze(
                    test,
                    seeds.project_id,
                    caller,
                    &body,
                    &history,
                    Some(policy),
                )
                .await,
            );
        }
        acknowledge(root, phase);
    }
}
