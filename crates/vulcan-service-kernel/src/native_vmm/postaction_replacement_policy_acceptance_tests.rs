//! Real queued extraction coverage for replacement controls and admission-time policy snapshots.
//! 替代控制与准入时策略快照的真实队列提炼覆盖。

use super::*;
use vulcan_memorymesh_core::{MemorySourceKind, TurnInputKind, TurnStatus};

/// Emits one memory from the actual declared user probe and no profile candidates.
/// 从实际声明的用户探针产生一条记忆，不产生画像候选。
/// Takes real L1 input and returns its exact turn identity, preserving the observed candidate text.
/// 接收真实 L1 输入，返回准确回合身份并保留观察到的候选正文。
pub(super) fn response(input: &serde_json::Value) -> serde_json::Value {
    // The canonical user field is the only extraction input; no alternate payload paths are probed.
    // 规范用户字段是唯一提取输入；不探测替代载荷路径。
    let user = input["target_turn"]["raw_turn"]["user"]
        .as_str()
        .expect("queued probe user");
    assert!(user.starts_with(PROBE));
    serde_json::json!({
        "user_input_kind":"statement", "turn_id":input["target_turn"]["turn_id"],
        "details":format!("Analyzed {user}."),
        "memory_nodes":[{"category":0,"abstract":user,"details":format!("Extracted {user}."),
            "evidence_source":"user_asserted","admission":"keep","admission_reason":""}],
        "profile_nodes":[]
    })
}

impl Acceptance {
    /// Enqueues a normal dialogue under the specified host and returns the actual ready job.
    /// 在指定宿主下正常入队一份对话，并返回实际就绪作业。
    /// Takes existing seed scope and a unique case label; does not execute or construct Core analysis.
    /// 接收既有种子作用域与唯一场景标签；不执行或构造 Core 分析。
    async fn enqueue_probe(
        &self,
        seeds: &Seeds,
        host: &str,
        label: &str,
    ) -> NativeVmmPendingAnalysisJob {
        // Stable operation identity allows reading the real persisted acknowledgement independently.
        // 稳定操作身份允许独立读取真实持久确认。
        let operation = format!("queued-replacement-{label}");
        self.client
            .post_action(PostActionRequest {
                host_project_id: host.to_owned(),
                project_id: seeds.project_id,
                user_id: 1,
                session_id: operation.clone(),
                operation_id: operation.clone(),
                user_content: format!("{PROBE} {label}"),
                assistant_content: "I will preserve this explicit project decision.".to_owned(),
                ..Default::default()
            })
            .await
            .expect("queued replacement intake");
        // Assert a persisted ready body and real turn, not merely a successful RPC result.
        // 断言持久就绪正文与真实回合，不只检查成功调用结果。
        let job = self
            .data_plane
            .load_pending_analysis_job(&operation)
            .await
            .expect("queued replacement read")
            .expect("queued replacement job");
        assert_eq!(job.state, VMM_PENDING_ANALYSIS_READY);
        assert!(job.turn_id.is_some());
        assert!(!job.payload_json.is_empty());
        job
    }

    /// Drains the actual job and verifies L1/L2, source attribution and complete memory mutations.
    /// 执行实际作业，核对 L1／L2、来源归属及完整记忆变更。
    /// Takes seeds, the saved ready job, its label, expected L2 seed indexes and a hard-drop flag.
    /// 接收种子、已保存就绪作业、标签、预期 L2 种子索引及硬丢弃标记。
    /// Returns only after durable completion and exact before/after comparisons succeed.
    /// 仅在持久完成及准确前后比较成功后返回。
    async fn analyze_probe(
        &self,
        seeds: &Seeds,
        job: &NativeVmmPendingAnalysisJob,
        label: &str,
        similar: &[usize],
        hard_drop: bool,
    ) {
        assert_eq!(
            self.data_plane
                .load_pending_analysis_job(&job.operation_id)
                .await
                .expect("unchanged queued probe read")
                .expect("unchanged queued probe"),
            *job
        );
        // Capture all projects so replacements cannot silently alter records outside this candidate scope.
        // 捕获所有项目，防止替代悄悄修改候选作用域之外的记录。
        let before = self
            .runtime
            .storage_snapshot()
            .expect("before queued probe");
        // Clone or read counters in a lexical lock scope before asynchronous execution.
        // 在异步执行前的词法锁作用域内克隆或读取计数。
        let cursor = self
            .port
            .requests
            .lock()
            .expect("queued capture cursor")
            .len();
        assert!(
            self.client
                .drain_due_pending_analysis_jobs()
                .await
                .expect("drain queued replacement")
        );
        // The completed receipt must preserve identity while clearing only the consumed body.
        // 完成回执必须保留身份，并仅清除已消费正文。
        let completed = self
            .data_plane
            .load_pending_analysis_job(&job.operation_id)
            .await
            .expect("completed queued probe read")
            .expect("completed queued probe");
        assert_eq!(
            completed.state, VMM_PENDING_ANALYSIS_COMPLETED,
            "{:?}",
            completed.last_error
        );
        assert_eq!(completed.turn_id, job.turn_id);
        assert_eq!(completed.attempt_count, 0);
        assert!(completed.last_error.is_none());
        assert!(completed.payload_json.is_empty());
        // Captures are actual host requests, not manually generated prompt fixtures.
        // 捕获来自实际宿主请求，不是手工生成的提示词夹具。
        let requests = self.port.requests.lock().expect("queued requests")[cursor..].to_vec();
        assert_eq!(requests.len(), if hard_drop { 1 } else { 2 });
        assert_eq!(requests[0].purpose_id, "postaction_l1");
        // Expected text is independently derived from this caller's declared unique input.
        // 预期正文独立取自本调用方声明的唯一输入。
        let summary = format!("{PROBE} {label}");
        // The actual target object must retain the full user/assistant dialogue and empty explicit timeline.
        // 实际目标对象必须保留完整用户／助手对话及明确空时间线。
        let l1 = analysis_input(&requests[0]);
        assert_eq!(
            l1["target_turn"]["turn_id"].as_str(),
            job.turn_id.as_deref()
        );
        assert_eq!(
            l1["target_turn"]["raw_turn"],
            serde_json::json!({
                "user":summary,"timeline":[],"assistant":"I will preserve this explicit project decision."
            })
        );
        assert_eq!(l1["reference_turns"], serde_json::json!([]));
        assert_eq!(l1["recent_direct_memory_writes"], serde_json::json!([]));
        if hard_drop {
            assert!(similar.is_empty());
        } else {
            assert_eq!(requests[1].purpose_id, "postaction_l2");
            // Inspect the exact candidate-local identities exposed by the production review renderer.
            // 检查生产评审渲染器暴露的准确候选内身份。
            let l2 = analysis_input(&requests[1]);
            // One L1 memory and no profiles must produce exactly one L2 memory candidate.
            // 一个 L1 记忆且无画像必须产生准确一个 L2 记忆候选。
            let candidates = l2["memory"].as_array().expect("queued L2 candidates");
            assert_eq!(candidates.len(), 1);
            assert_eq!(candidates[0]["abstract"], summary);
            assert_eq!(candidates[0]["details"], format!("Extracted {summary}."));
            assert_eq!(candidates[0]["evidence_source"], "user_asserted");
            // Threshold and review-window expectations are independent of the model's accepted output.
            // 门槛及评审窗口预期独立于模型的接纳输出。
            let actual = candidates[0]["similar_memories"]
                .as_array()
                .expect("queued similar candidates");
            assert_eq!(actual.len(), similar.len(), "queued case {label}");
            for (memory, index) in actual.iter().zip(similar) {
                assert_eq!(memory["memory_id"], seeds.ids[*index].as_str());
                assert_eq!(memory["origin"], "vector_search");
                assert!(
                    (memory["score"].as_f64().expect("queued candidate score")
                        - [0.95, 0.875, 0.8][*index])
                        .abs()
                        < 1e-7
                );
            }
        }
        // Inspect persisted Core facts after the real drain, independently of the queue acknowledgement.
        // 实际执行后检查持久 Core 事实，独立于队列确认。
        let after = self
            .runtime
            .storage_snapshot()
            .expect("after queued replacement");
        assert_eq!(
            after.dataset.memories.len(),
            before.dataset.memories.len() + usize::from(!hard_drop)
        );
        for original in &before.dataset.memories {
            // Analysis uses its declared supersede reason while preserving every other original field.
            // 分析使用声明的替代原因，并保留其他每个原始字段。
            let mut expected = original.clone();
            if !hard_drop && similar.iter().any(|index| seeds.ids[*index] == original.id) {
                expected.state = MemoryState::Superseded;
                expected.status_reason = Some("superseded-by-review".to_owned());
                expected.version += 1;
            }
            assert_eq!(
                after
                    .dataset
                    .memories
                    .iter()
                    .find(|memory| memory.id == original.id)
                    .expect("queued prior memory"),
                &expected
            );
        }
        // Only the actual new source turn may own a derived memory, never the seed's direct-write source.
        // 只有实际新来源回合可以持有派生记忆，不能沿用种子的主动写入来源。
        let derived = after
            .dataset
            .memories
            .iter()
            .filter(|memory| {
                memory.source_turn_id.as_ref().map(|id| id.as_str()) == job.turn_id.as_deref()
            })
            .collect::<Vec<_>>();
        assert_eq!(derived.len(), usize::from(!hard_drop));
        if !hard_drop {
            assert_eq!(derived[0].source_kind, MemorySourceKind::Turn);
            assert_eq!(
                derived[0].scope_id,
                project_scope_id(seeds.project_id).expect("queued scope")
            );
            assert_eq!(derived[0].summary, summary);
            assert_eq!(derived[0].content, format!("Extracted {summary}."));
            assert_eq!(derived[0].state, MemoryState::Active);
            assert_eq!(derived[0].evidence_source.as_deref(), Some("user_asserted"));
        }
        // The existing admitted turn must complete even when hard dedupe prevents all memory creation.
        // 即使硬去重阻止全部记忆创建，既有准入回合也必须完成。
        let turn = after
            .dataset
            .turns
            .iter()
            .find(|turn| Some(turn.id.as_str()) == job.turn_id.as_deref())
            .expect("completed queued turn");
        assert_eq!(turn.status, TurnStatus::Completed);
        assert_eq!(
            turn.analysis_details.as_deref(),
            Some(format!("Analyzed {summary}.").as_str())
        );
        assert_eq!(after.dataset.turns.len(), before.dataset.turns.len());
        for original in &before.dataset.turns {
            // Only the admitted target advances three declared fields; timelines and all other turns remain exact.
            // 仅准入目标推进三个声明字段；时间线及其他全部回合准确保持。
            let mut expected = original.clone();
            if Some(original.id.as_str()) == job.turn_id.as_deref() {
                assert_eq!(original.status, TurnStatus::Pending);
                expected.status = TurnStatus::Completed;
                expected.user_input_kind = Some(TurnInputKind::Statement);
                expected.analysis_details = Some(format!("Analyzed {summary}."));
            }
            assert_eq!(
                after
                    .dataset
                    .turns
                    .iter()
                    .find(|turn| turn.id == original.id)
                    .expect("queued original turn retained"),
                &expected
            );
        }
        assert_eq!(after.dataset.profile_nodes, before.dataset.profile_nodes);
    }
}

/// Exercises independent queued field cases and both configuration timing boundaries.
/// 执行独立队列字段场景及两个配置时间边界。
/// Takes the real environment and neutral/final policies; restores final global policy for parent rebuild.
/// 接收真实环境及中性／最终策略；恢复最终全局策略供父验收重建。
pub(super) async fn verify(
    test: &Acceptance,
    neutral: &VmmNativeRetrievalPolicySummary,
    final_policy: &VmmNativeRetrievalPolicySummary,
) {
    // Match established extraction fixtures: semantic-noise prototypes are outside this declared vector scenario.
    // 遵循既有提炼夹具：语义噪音原型不属于此声明向量场景。
    let mut settings = test
        .runtime
        .resource_catalog()
        .await
        .expect("queued replacement resources")
        .settings;
    settings.noise_semantic_enabled = false;
    test.runtime
        .update_resource_settings(settings)
        .await
        .expect("disable unrelated queued semantic noise");
    // Each queued request is admitted first; only then is its actual analysis policy formally saved.
    // 每个排队请求先准入，然后才正式保存实际分析策略。
    // Three representative rows prove the saved fields reach real writes; the remaining floor, window,
    // and hard-dedupe combinations are covered by the engine's replacement_decision_tests.
    // 三组代表性参数证明已保存字段到达真实写入；其余下限、窗口与硬去重组合由引擎层
    // replacement_decision_tests 覆盖。
    for (name, floor, limit, threshold, pool, similar, hard_drop) in [
        ("queue-all", 0.0, 3, 0.0, 3, vec![0, 1, 2], false),
        ("queue-floor-high", 0.9, 3, 0.0, 3, vec![0], false),
        ("queue-hard-below", 0.0, 1, 0.55, 3, vec![], true),
    ] {
        // Fresh physical scopes isolate prior replacements without mutating any source records directly.
        // 新物理作用域隔离先前替代，不直接修改任何来源记录。
        let seeds = test.seeds(name, neutral).await;
        // Persist a real ready job under the preceding neutral policy.
        // 在先前中性策略下持久一个真实就绪作业。
        let job = test.enqueue_probe(&seeds, "replacement-host-b", name).await;
        // Only these four declared fields differ, keeping unrelated transforms disabled.
        // 只有这四个声明字段不同，保持无关变换关闭。
        let mut policy = neutral.clone();
        policy.replace_min_similarity = floor;
        policy.replacement_top_k = limit;
        policy.hard_dedupe_cosine_threshold = threshold;
        policy.hard_dedupe_pool_top_k = pool;
        test.save_global(&policy).await;
        test.analyze_probe(&seeds, &job, name, &similar, hard_drop)
            .await;
    }
    // A's explicit hard-drop policy must survive a global save while B inherits a no-match policy.
    // A 的明确硬丢弃策略必须在全局保存后保持，而 B 继承无匹配策略。
    let mut override_policy = neutral.clone();
    override_policy.replace_min_similarity = 1.0;
    override_policy.replacement_top_k = 1;
    override_policy.hard_dedupe_pool_top_k = 3;
    override_policy.hard_dedupe_cosine_threshold = 0.55;
    // Four different next controls make the inherited outcome observably different.
    // 四个不同后续控制项使继承结果具有可观察差异。
    let mut inherited = neutral.clone();
    inherited.replace_min_similarity = 0.99;
    inherited.replacement_top_k = 2;
    inherited.hard_dedupe_pool_top_k = 1;
    inherited.hard_dedupe_cosine_threshold = 0.65;
    // Both hosts share exactly the same seed records; A's hard drop leaves them unchanged for B.
    // 两宿主共享准确相同种子记录；A 的硬丢弃使其保持不变供 B 使用。
    let shared = test.seeds("queue-hosts", neutral).await;
    test.save_a(Some(override_policy.clone()), &override_policy)
        .await;
    // Enqueue A before changing global settings to exercise the actual persisted caller identity.
    // 修改全局设置前入队 A，以检验实际持久调用方身份。
    let a = test
        .enqueue_probe(&shared, "replacement-host-a", "queue-host-a")
        .await;
    test.save_global(&inherited).await;
    test.analyze_probe(&shared, &a, "queue-host-a", &[], true)
        .await;
    // B then consumes the same records under its inherited policy and creates the derived memory.
    // B 随后在继承策略下消费相同记录并创建派生记忆。
    let b = test
        .enqueue_probe(&shared, "replacement-host-b", "queue-host-b")
        .await;
    test.analyze_probe(&shared, &b, "queue-host-b", &[], false)
        .await;
    // Restore inheritance after queue intake, not before, so the queued job cannot cache the override.
    // 在队列准入后而非之前恢复继承，确保排队作业没有缓存覆盖值。
    let restored = test.seeds("queue-inherit", neutral).await;
    test.save_global(&inherited).await;
    test.save_a(Some(override_policy.clone()), &override_policy)
        .await;
    // The admitted job's full body must remain unchanged across the formal inheritance save.
    // 正式继承保存前后，准入作业的完整正文必须保持不变。
    let job = test
        .enqueue_probe(&restored, "replacement-host-a", "queue-inherit")
        .await;
    test.save_a(None, &inherited).await;
    test.analyze_probe(&restored, &job, "queue-inherit", &[], false)
        .await;
    // Once actual L1 starts, later formal settings must not alter this admitted analysis.
    // 实际 L1 开始后，后续正式设置不得改变本次已准入分析。
    let frozen = test.seeds("queue-frozen", neutral).await;
    test.save_global(&override_policy).await;
    // The job is admitted while the hard-drop policy is active, before arming the real L1 boundary.
    // 作业在硬丢弃策略活动时准入，随后才启用真实 L1 边界。
    let old = test
        .enqueue_probe(&frozen, "replacement-host-b", "queue-frozen-old")
        .await;
    // The observer proves command admission reached L1 before the next global policy save.
    // 观察者证明命令准入早于下一全局策略保存到达 L1。
    let (entered, observed) = tokio::sync::oneshot::channel();
    // Only a successful save releases this one bounded analysis pause.
    // 只有成功保存才释放此次有界分析暂停。
    let (resume, resumed) = tokio::sync::oneshot::channel();
    *test
        .port
        .postaction_pause
        .lock()
        .expect("arm queued L1 pause") = Some(postaction_budget::AnalysisPause {
        entered,
        resume: resumed,
    });
    tokio::join!(
        test.analyze_probe(&frozen, &old, "queue-frozen-old", &[], true),
        async {
            tokio::time::timeout(Duration::from_secs(10), observed)
                .await
                .expect("bounded queued L1 pause")
                .expect("queued L1 reached");
            test.save_global(&inherited).await;
            resume.send(()).expect("resume queued analysis");
        }
    );
    // The prior hard drop preserved the exact seeds, so a new source can demonstrate the next policy.
    // 先前硬丢弃保留准确种子，因此新来源可以证明后续策略。
    let next = test
        .enqueue_probe(&frozen, "replacement-host-b", "queue-frozen-next")
        .await;
    test.analyze_probe(&frozen, &next, "queue-frozen-next", &[], false)
        .await;
    test.save_global(final_policy).await;
}
