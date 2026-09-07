//! Formal post-action budget saves consumed by real queued analysis and immutable L1 input.
//! 由真实队列分析及不可变 L1 输入消费的正式 PostAction 预算保存。

use super::*;
use vulcan_service_api::{VmmMemoryActionResponse, VmmNativeRecallPolicySummary};
use vulcan_vmm::PostActionTimelineItem;

/// Keeps every source body identical so only saved budgets and explicit caller ownership change the input window.
/// 使每份来源正文相同，从而仅由保存预算及明确调用方归属改变输入窗口。
const USER: &str =
    "项目数据库使用 PostgreSQL，部署前必须核对连接配置和备份策略，并保留部署检查结果。";
/// Exercises quoted text in the actual serialized intermediate assistant message.
/// 在实际序列化中间助手消息中覆盖引号文本。
const ASK: &str = "请确认配置中的 \"重试次数\"，以及部署前是否检查备份。";
/// Exercises escaped path separators and an internal newline without synthetic non-business fields.
/// 覆盖转义路径分隔符及内部换行，不构造非业务字段。
const FOLLOW_UP: &str =
    "配置位于 config\\database，按以下要求执行：\n部署前检查备份，部署后核对连接。";
/// Preserves a distinct canonical final answer that history trimming must never replace with the middle ask.
/// 保留不同的规范最终回答，历史裁剪绝不能将其替换为中间提问。
const ASSISTANT: &str = "已确认，按部署检查要求执行并记录结果。";

/// Pauses one real post-action L1 effect after its analysis configuration has been frozen.
/// 在分析配置冻结后暂停一个真实 PostAction L1 效应。
pub(super) struct AnalysisPause {
    /// Announces the exact model boundary reached by production execution.
    /// 通知生产执行到达准确模型边界。
    pub(super) entered: tokio::sync::oneshot::Sender<()>,
    /// Resumes that same effect only after the formal new-policy save completes.
    /// 仅在正式新策略保存完成后恢复同一个效应。
    pub(super) resume: tokio::sync::oneshot::Receiver<()>,
}

/// Waits for an explicitly armed analysis pause and otherwise returns immediately.
/// 等待明确启用的分析暂停，其他情况立即返回。
/// Takes the fixture-owned slot; releases its synchronous guard before the bounded asynchronous wait.
/// 接收夹具持有的暂停槽；在有界异步等待前释放同步守卫。
pub(super) async fn wait_for_analysis(slot: &StdMutex<Option<AnalysisPause>>) {
    // Move the one-shot endpoints out so service work never waits on this fixture lock.
    // 移出一次性端点，使服务工作绝不等待此夹具锁。
    let pause = slot.lock().expect("analysis pause slot").take();
    if let Some(pause) = pause {
        pause.entered.send(()).expect("analysis boundary observer");
        tokio::time::timeout(Duration::from_secs(10), pause.resume)
            .await
            .expect("bounded analysis resume")
            .expect("analysis resume sender");
    }
}

/// Returns the independently specified complete business object expected at every actual L1 boundary.
/// 返回独立指定的完整业务对象，作为每次实际 L1 边界的预期。
/// Uses the declared public timeline wire keys and returns no storage or session metadata.
/// 使用声明的公开时间线传输键，不返回存储或会话元数据。
fn expected_raw() -> serde_json::Value {
    serde_json::json!({
        "user": USER,
        "timeline": [
            {"type": "assistant", "content": ASK},
            {"type": "user", "content": FOLLOW_UP}
        ],
        "assistant": ASSISTANT
    })
}

/// Saves a complete global recall group through the same service used by the settings UI.
/// 通过设置界面所用的同一服务保存完整全局召回组。
/// Takes isolated service state and desired rules; validates the exact accepted response before returning.
/// 接收隔离服务状态及目标规则；返回前校验准确接纳响应。
async fn save_global(
    service: &crate::state::SharedServiceBridgeState,
    recall: &VmmNativeRecallPolicySummary,
) {
    // No direct resource write can stand in for the public save path under test.
    // 任何直接资源写入均不能替代待测公开保存路径。
    let saved = policy_service::action(
        service,
        "native_runtime_policy_update",
        serde_json::to_value(recall).expect("analysis budget JSON"),
    )
    .await
    .expect("save analysis global budget");
    // An unrelated successful action cannot satisfy policy persistence acceptance.
    // 无关成功动作不能满足策略持久化验收。
    let VmmMemoryActionResponse::NativeRuntimePolicy(saved) = saved else {
        panic!("expected analysis policy response")
    };
    assert_eq!(saved.recall, *recall);
}

/// Saves or clears A's complete explicit recall group without changing the other inherited policy groups.
/// 保存或清除 A 的完整明确召回组，不改变其他继承策略组。
/// Takes optional rules and their expected effective group; returns only after exact caller and value checks.
/// 接收可选规则及其预期有效组；仅在准确调用方和值校验后返回。
async fn save_a(
    service: &crate::state::SharedServiceBridgeState,
    recall: Option<VmmNativeRecallPolicySummary>,
    effective: &VmmNativeRecallPolicySummary,
) {
    // Budget ownership uses the stable caller identity, not the shared memory-project identity.
    // 预算归属使用稳定调用方身份，而不是共享记忆项目身份。
    let overrides = VmmNativeHostProjectPolicyOverrides {
        recall,
        ..Default::default()
    };
    // Exercise the same closed payload accepted from the host-project policy component.
    // 覆盖宿主项目策略组件所用的同一封闭参数。
    let saved = policy_service::action(
        service,
        "native_host_project_policy_update",
        serde_json::json!({"hostProjectId":"budget-host-a", "overrides": overrides}),
    )
    .await
    .expect("save analysis caller budget");
    // Require the caller-policy response rather than reading an unrelated global value.
    // 要求调用方策略响应，而不是读取无关全局值。
    let VmmMemoryActionResponse::NativeHostProjectPolicy(saved) = saved else {
        panic!("expected analysis host response")
    };
    assert_eq!(saved.host_project_id, "budget-host-a");
    assert_eq!(saved.overrides, overrides);
    assert_eq!(saved.effective.recall, *effective);
}

/// Enqueues one full real dialogue and returns its durable ready job before any analysis starts.
/// 将一份完整真实对话入队，并在任何分析开始前返回其耐久就绪作业。
/// Takes the production client/data plane, shared project, caller and unique ordinal; does not write Core turns directly.
/// 接收生产客户端／数据面、共享项目、调用方及唯一序号；不直接写入 Core 回合。
async fn enqueue(
    client: &NativeVmmClient,
    data_plane: &VmmDataPlaneHandle,
    project_id: u64,
    caller: &str,
    ordinal: usize,
) -> NativeVmmPendingAnalysisJob {
    // Only the stable operation and caller differ across otherwise identical business dialogues.
    // 除稳定操作和调用方之外，各份业务对话完全相同。
    let operation = format!("postaction-budget-{ordinal}");
    client
        .post_action(PostActionRequest {
            host_project_id: caller.to_owned(),
            project_id,
            user_id: 1,
            session_id: "postaction-budget-session".to_owned(),
            operation_id: operation.clone(),
            user_content: USER.to_owned(),
            assistant_content: ASSISTANT.to_owned(),
            timeline: vec![
                PostActionTimelineItem {
                    kind: PostActionTimelineType::Assistant,
                    content: ASK.to_owned(),
                },
                PostActionTimelineItem {
                    kind: PostActionTimelineType::User,
                    content: FOLLOW_UP.to_owned(),
                },
            ],
            ..Default::default()
        })
        .await
        .expect("analysis budget intake");
    // A returned acknowledgement is insufficient without the actual queued body and admitted turn identity.
    // 没有实际排队正文及已准入回合身份时，仅返回确认并不足够。
    let job = data_plane
        .load_pending_analysis_job(&operation)
        .await
        .expect("analysis budget ready read")
        .expect("analysis budget ready job");
    assert_eq!(job.state, VMM_PENDING_ANALYSIS_READY);
    assert!(job.turn_id.is_some());
    job
}

/// Drains the actual queue head and validates its entire model-visible target and selected reference suffix.
/// 排空实际队列队首，并校验其完整模型可见目标及选中参考后缀。
/// Takes the ready job, previously completed IDs and explicit expected count; appends the ID only after durable success.
/// 接收就绪作业、此前完成 ID 及明确预期数量；仅在耐久成功后追加 ID。
async fn analyze(
    client: &NativeVmmClient,
    port: &CapturingAnalysisPort,
    data_plane: &VmmDataPlaneHandle,
    job: &NativeVmmPendingAnalysisJob,
    completed_ids: &mut Vec<String>,
    expected_count: usize,
) {
    // Formal settings saves must leave every field of the existing ready job unchanged.
    // 正式设置保存必须使现有就绪作业的每个字段保持不变。
    assert_eq!(
        data_plane
            .load_pending_analysis_job(&job.operation_id)
            .await
            .expect("unmodified analysis job read")
            .expect("unmodified analysis job"),
        *job
    );
    // Earlier successful source IDs independently determine the expected newest chronological suffix.
    // 此前成功来源 ID 独立决定预期最新时间正序后缀。
    assert!(completed_ids.len() >= expected_count);
    // Derive expected references from accepted source identities, never from the response currently under test.
    // 从已接纳来源身份推导预期参考，绝不使用本次待测响应推导。
    let expected = completed_ids[completed_ids.len() - expected_count..]
        .iter()
        .map(|id| serde_json::json!({"turn_id": id, "details": USER}))
        .collect::<Vec<_>>();
    // A capture offset ensures earlier analyses cannot satisfy this job's model observation.
    // 捕获偏移确保之前分析不能满足此作业的模型观察。
    let before = port
        .requests
        .lock()
        .expect("analysis budget capture start")
        .len();
    assert!(
        client
            .drain_due_pending_analysis_jobs()
            .await
            .expect("analysis budget real drain")
    );
    // Successful queue completion must be observed before adding this turn as reference history.
    // 必须观察到队列成功完成后，才将此回合作为参考历史追加。
    let completed = data_plane
        .load_pending_analysis_job(&job.operation_id)
        .await
        .expect("analysis budget completion read")
        .expect("analysis budget completion");
    assert_eq!(
        completed.state, VMM_PENDING_ANALYSIS_COMPLETED,
        "{:?}",
        completed.last_error
    );
    assert_eq!(completed.attempt_count, 0);
    assert!(completed.payload_json.is_empty());
    // Empty candidate extraction invokes precisely one real L1 request and no review or embedding work.
    // 空候选提炼恰好调用一次真实 L1 请求，不产生审核或向量工作。
    let captures = port.requests.lock().expect("analysis budget captures")[before..].to_vec();
    assert_eq!(captures.len(), 1);
    assert_eq!(captures[0].purpose_id, "postaction_l1");
    // Compare the actual target and history independently of prompt helper or Core command construction.
    // 独立于提示词辅助方法或 Core 命令构造，比对实际目标及历史。
    let input = analysis_input(&captures[0]);
    assert_eq!(
        input["target_turn"]["turn_id"].as_str(),
        job.turn_id.as_deref()
    );
    assert_eq!(input["target_turn"]["raw_turn"], expected_raw());
    assert_eq!(input["reference_turns"], serde_json::json!(expected));
    assert_eq!(input["recent_direct_memory_writes"], serde_json::json!([]));
    completed_ids.push(
        job.turn_id
            .as_ref()
            .expect("completed analysis turn ID")
            .clone(),
    );
}

/// Verifies formal budget persistence, queued-time resolution, caller isolation and admitted-analysis freezing.
/// 验证正式预算持久化、队列执行时解析、调用方隔离及已准入分析冻结。
/// Takes an isolated real controller data plane and declared capability; all intake, history and persistence remain production.
/// 接收隔离真实控制器数据面及声明能力；全部入库、历史及持久化保持生产实现。
pub(in crate::native_vmm) async fn verify(
    data_plane: VmmDataPlaneHandle,
    capability: InferenceCapabilitySnapshot,
) {
    // The existing empty fixture returns real target details and never creates unrelated vector candidates.
    // 既有空夹具返回真实目标详情，绝不创建无关向量候选。
    let port = Arc::new(CapturingAnalysisPort {
        inner: MockHostAiPort::new(),
        requests: StdMutex::new(Vec::new()),
        reject_next_turn: AtomicBool::new(false),
        fixture: AnalysisFixture::Empty,
        embedding_requests: StdMutex::new(Vec::new()),
        profile_instruction_pause: StdMutex::new(None),
        postaction_pause: StdMutex::new(None),
    });
    port.inner
        .set_capability_snapshot(capability)
        .expect("analysis budget capability");
    // Actual native bootstrap owns all policy files, canonical turns and queue rows in private storage.
    // 实际原生引导持有私有存储中的全部策略文件、规范回合及队列记录。
    let runtime = Arc::new(
        NativeVmmCoreRuntime::bootstrap(port.clone(), data_plane.clone())
            .await
            .expect("analysis budget runtime"),
    );
    // Only unrelated semantic-noise inference is disabled; deterministic cleaning and intake remain real.
    // 仅关闭无关语义噪声推理；确定性清洗及入库保持真实。
    let mut settings = runtime
        .resource_catalog()
        .await
        .expect("analysis budget resources")
        .settings;
    settings.noise_semantic_enabled = false;
    runtime
        .update_resource_settings(settings)
        .await
        .expect("analysis budget noise setting");
    // Both callers share the same stored history, so their budget groups are the sole selection difference.
    // 两个调用方共享同一持久历史，因此预算组是唯一选择差异。
    let project = data_plane
        .create_project("Analysis budget", "analysis-budget")
        .await
        .expect("analysis budget project");
    for caller in ["budget-host-a", "budget-host-b"] {
        data_plane
            .bind_vulcan_project(caller, project.id)
            .await
            .expect("analysis budget binding");
    }
    // The service and client share the actual maintenance/admission coordination boundary.
    // 服务与客户端共享实际维护／准入协调边界。
    let gate = Arc::new(RwLock::new(()));
    // Build the exact service owner consumed by all public budget saves.
    // 构建全部公开预算保存消费的准确服务归属方。
    let service = policy_service::service(runtime.clone(), data_plane.clone(), gate.clone());
    // Do not start a polling worker; each explicit production drain advances one known same-session head.
    // 不启动轮询工作器；每次明确生产排空推进一个已知同会话队首。
    let client = NativeVmmClient::new(
        runtime.clone(),
        data_plane.clone(),
        gate,
        Arc::new(AtomicBool::new(true)),
        Arc::new(Notify::new()),
    );
    // Hold the same three-turn reference limit while independently changing only the analysis budget.
    // 保持同一三回合参考上限，仅独立改变分析预算。
    let mut global = runtime
        .runtime_policy()
        .await
        .expect("analysis budget defaults")
        .recall;
    global.postaction_history_turn_limit = 3;
    global.postaction_max_input_tokens = 131_072;
    save_global(&service, &global).await;
    // Keep the original ready payloads for final full-timeline comparison after successful analysis clears duplicate text.
    // 保留原始就绪参数，用于成功分析清除重复正文后最终完整时间线比较。
    let mut jobs = Vec::new();
    // Record only actual durable completion IDs as possible reference sources.
    // 仅将实际耐久完成 ID 记录为可用参考来源。
    let mut completed_ids = Vec::new();
    for ordinal in 0..3 {
        // Bootstrap history through the same public intake and queue path as later budget cases.
        // 通过与后续预算用例相同的公开入库及队列路径建立历史。
        let job = enqueue(&client, &data_plane, project.id, "budget-host-b", ordinal).await;
        analyze(
            &client,
            &port,
            &data_plane,
            &job,
            &mut completed_ids,
            ordinal,
        )
        .await;
        jobs.push(job);
    }
    // The raw target includes JSON wrappers, quotes, escapes and middle-role fields, not just text leaves.
    // 原始目标包括 JSON 包装、引号、转义及中间角色字段，而不仅是正文叶节点。
    let target_cost = estimate_text_tokens(
        &serde_json::to_string(&expected_raw()).expect("analysis target budget JSON"),
    );
    // Historical cost uses persisted extracted details rather than the historical raw target wrapper.
    // 历史成本使用持久提炼详情，而不是历史原始目标包装。
    let detail_cost = estimate_text_tokens(USER);
    assert!(
        target_cost
            > [USER, ASK, FOLLOW_UP, ASSISTANT]
                .iter()
                .map(|text| estimate_text_tokens(text))
                .sum::<u32>()
    );
    assert!(detail_cost > 1);
    for (budget, expected_count) in [
        (target_cost - 1, 0),
        (target_cost, 0),
        (target_cost + 1, 1),
        (target_cost + detail_cost, 1),
        (target_cost + 2 * detail_cost - 1, 1),
        (target_cost + 2 * detail_cost, 2),
    ] {
        // Save after enqueue so the observed boundary proves execution-time policy resolution, not intake-time freezing.
        // 入队后再保存，使观察边界证明执行时策略解析，而不是入队时冻结。
        let job = enqueue(
            &client,
            &data_plane,
            project.id,
            "budget-host-b",
            jobs.len(),
        )
        .await;
        global.postaction_max_input_tokens = budget;
        save_global(&service, &global).await;
        analyze(
            &client,
            &port,
            &data_plane,
            &job,
            &mut completed_ids,
            expected_count,
        )
        .await;
        jobs.push(job);
    }
    // Enqueue both caller identities before changing either saved group; the queue must retain each original caller.
    // 在改变任何保存组之前将两个调用方身份入队；队列必须保留各自原始调用方。
    let a_job = enqueue(
        &client,
        &data_plane,
        project.id,
        "budget-host-a",
        jobs.len(),
    )
    .await;
    jobs.push(a_job.clone());
    // B is the later same-session head and cannot inherit A's explicit group from preceding analysis.
    // B 是同会话后续队首，不能从之前分析继承 A 的明确组。
    let b_job = enqueue(
        &client,
        &data_plane,
        project.id,
        "budget-host-b",
        jobs.len(),
    )
    .await;
    jobs.push(b_job.clone());
    // A has a large explicit budget while B inherits a fully target-consumed budget.
    // A 具有明确大预算，B 则继承完全被目标消耗的预算。
    let mut a = global.clone();
    a.postaction_max_input_tokens = 131_072;
    save_a(&service, Some(a.clone()), &a).await;
    global.postaction_max_input_tokens = target_cost;
    save_global(&service, &global).await;
    analyze(&client, &port, &data_plane, &a_job, &mut completed_ids, 3).await;
    analyze(&client, &port, &data_plane, &b_job, &mut completed_ids, 0).await;
    global.postaction_max_input_tokens = target_cost + 1;
    save_global(&service, &global).await;
    // A's explicit group remains unchanged even after B's inherited budget changes again.
    // 即使 B 的继承预算再次改变，A 的明确组仍保持不变。
    let a_again = enqueue(
        &client,
        &data_plane,
        project.id,
        "budget-host-a",
        jobs.len(),
    )
    .await;
    analyze(&client, &port, &data_plane, &a_again, &mut completed_ids, 3).await;
    jobs.push(a_again);
    // Restoring inheritance changes the next already-queued A analysis to the current one-reference budget.
    // 恢复继承使下一个已入队 A 分析改用当前仅一条参考的预算。
    let inherited = enqueue(
        &client,
        &data_plane,
        project.id,
        "budget-host-a",
        jobs.len(),
    )
    .await;
    save_a(&service, None, &global).await;
    analyze(
        &client,
        &port,
        &data_plane,
        &inherited,
        &mut completed_ids,
        1,
    )
    .await;
    jobs.push(inherited);
    global.postaction_max_input_tokens = 131_072;
    save_global(&service, &global).await;
    // This job must retain three references after admission even while a new save disables later reference history.
    // 即使新保存使后续参考历史消失，此作业准入后仍必须保留三条参考。
    let frozen_job = enqueue(
        &client,
        &data_plane,
        project.id,
        "budget-host-b",
        jobs.len(),
    )
    .await;
    // Observe the real L1 boundary without synthetic timestamps or blocking sleeps.
    // 观察真实 L1 边界，不使用合成时间戳或阻塞休眠。
    let (entered, observed) = tokio::sync::oneshot::channel();
    // Resume only after the new global save has completed normally.
    // 仅在新全局保存正常完成后恢复。
    let (resume, resumed) = tokio::sync::oneshot::channel();
    *port
        .postaction_pause
        .lock()
        .expect("arm analysis budget pause") = Some(AnalysisPause {
        entered,
        resume: resumed,
    });
    // Construct the new group independently of the already admitted old command.
    // 独立于已准入旧命令构建新组。
    let mut next = global.clone();
    next.postaction_max_input_tokens = target_cost;
    tokio::join!(
        analyze(
            &client,
            &port,
            &data_plane,
            &frozen_job,
            &mut completed_ids,
            3
        ),
        async {
            tokio::time::timeout(Duration::from_secs(10), observed)
                .await
                .expect("analysis boundary timeout")
                .expect("analysis boundary reached");
            save_global(&service, &next).await;
            resume.send(()).expect("resume frozen analysis");
        }
    );
    jobs.push(frozen_job);
    // The following real command must consume the newly saved target-only budget.
    // 随后的真实命令必须消费新保存的仅目标预算。
    let next_job = enqueue(
        &client,
        &data_plane,
        project.id,
        "budget-host-b",
        jobs.len(),
    )
    .await;
    analyze(
        &client,
        &port,
        &data_plane,
        &next_job,
        &mut completed_ids,
        0,
    )
    .await;
    jobs.push(next_job);
    assert_eq!(jobs.len(), 15);
    assert_eq!(completed_ids.len(), 15);
    assert_eq!(
        port.requests.lock().expect("analysis final calls").len(),
        15
    );
    assert!(
        port.embedding_requests
            .lock()
            .expect("analysis no embeddings")
            .is_empty()
    );
    assert!(
        !client
            .drain_due_pending_analysis_jobs()
            .await
            .expect("analysis final empty drain")
    );
    // Read the real relational snapshot after all commits, not just the live Core projection.
    // 在全部提交后读取真实关系快照，而不只读取活动 Core 投影。
    let persisted: vulcan_memorymesh_core::StorageSnapshot = serde_json::from_str(
        &data_plane
            .load_core_snapshot_json()
            .await
            .expect("analysis budget persisted read")
            .expect("analysis budget persisted bytes"),
    )
    .expect("analysis budget snapshot");
    assert_eq!(persisted.dataset.turns.len(), jobs.len());
    assert!(persisted.dataset.memories.is_empty());
    for job in jobs {
        // The original ready payload independently records every admitted role, timestamp and source text.
        // 原始就绪参数独立记录每项准入角色、时间戳及来源文本。
        let payload: NativePendingPostActionPayload =
            serde_json::from_str(&job.payload_json).expect("original analysis intake");
        // Match only the actual job-owned turn; no same-name or first-row fallback is allowed.
        // 仅匹配实际作业持有的回合；不允许同名或首条记录回退。
        let turn = persisted
            .dataset
            .turns
            .iter()
            .find(|turn| Some(turn.id.as_str()) == job.turn_id.as_deref())
            .expect("analysis final canonical turn");
        assert_eq!(turn.timeline, payload.intake.timeline);
        assert_eq!(turn.status, TurnStatus::Completed);
        assert_eq!(turn.analysis_details.as_deref(), Some(USER));
    }
    drop(client);
    drop(service);
    drop(runtime);
    // Rebuild from private disk to prove the final saved budget and complete analyzed source set survive reconstruction.
    // 从私有磁盘重建，证明最终保存预算及完整已分析来源集合在重建后保持。
    let reopened = NativeVmmCoreRuntime::bootstrap(port, data_plane)
        .await
        .expect("analysis budget reopen");
    assert_eq!(
        reopened
            .runtime_policy()
            .await
            .expect("analysis budget reopened policy")
            .recall,
        next
    );
    assert_eq!(
        reopened
            .storage_snapshot()
            .expect("analysis budget reopened turns")
            .dataset
            .turns,
        persisted.dataset.turns
    );
}
