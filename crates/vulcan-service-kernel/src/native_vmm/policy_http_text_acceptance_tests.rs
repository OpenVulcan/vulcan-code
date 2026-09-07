//! Live text-policy saves drive actual intake, durable jobs and frozen L1 role bodies.
//! 联网文本策略保存驱动实际入库、耐久作业和冻结的 L1 角色正文。

use super::*;
use vulcan_service_api::VmmNativeTextPolicySummary;

/// Returns the independently declared generous folding policy; takes no frontend values.
/// 返回独立声明的宽裕折叠策略；不接收前端值。
pub(super) fn folded() -> VmmNativeTextPolicySummary {
    VmmNativeTextPolicySummary {
        fold_machine_text: true,
        max_fragment_tokens: 20_000,
    }
}

/// Verifies five real HTTP phases and seven normal intakes, then checks all acknowledged L1 jobs.
/// 验证五个真实 HTTP 阶段和七次正常入库，随后检查全部已确认 L1 作业。
/// Takes the existing private environment and handshake directory; returns after durable completion.
/// 接收既有私有环境及握手目录；在耐久完成后返回。
pub(super) async fn verify(test: &Acceptance, root: &Path) {
    // Isolate semantic prototypes, preserving normal role sanitation, folding and actual queue intake.
    // 隔离语义原型，保留正常角色清理、折叠和实际队列入库。
    let mut settings = test
        .runtime
        .resource_catalog()
        .await
        .expect("HTTP text resources")
        .settings;
    settings.noise_semantic_enabled = false;
    test.runtime
        .update_resource_settings(settings)
        .await
        .expect("HTTP text noise isolation");
    // The two callers share new physical text ownership but retain distinct host-policy identities.
    // 两调用方共享新的物理文本归属，但保留不同宿主策略身份。
    let project = test
        .data_plane
        .create_project("HTTP text", "http-text")
        .await
        .expect("HTTP text project");
    for caller in ["replacement-host-a", "replacement-host-b"] {
        test.data_plane
            .bind_vulcan_project(caller, project.id)
            .await
            .expect("HTTP text binding");
    }
    // Snapshot unrelated policy groups and records before any text UI save.
    // 在任何文本界面保存前快照无关策略组及记录。
    let initial = test
        .runtime
        .runtime_policy()
        .await
        .expect("HTTP before text");
    // Empty extraction must never rewrite the earlier retrieval acceptance memories.
    // 空提取绝不能改写先前检索验收的记忆。
    let memories = test
        .runtime
        .storage_snapshot()
        .expect("HTTP text memories")
        .dataset
        .memories;
    // The driver persists without_embeddings separately; compare each representation to its own baseline.
    // 驱动单独持久化 without_embeddings；每种表示必须与自身基线比较。
    let original_disk: vulcan_memorymesh_core::StorageSnapshot = serde_json::from_str(
        &test
            .data_plane
            .load_core_snapshot_json()
            .await
            .expect("HTTP before text bytes")
            .expect("HTTP before text disk"),
    )
    .expect("HTTP before text snapshot");
    // Existing seed L2 requests are outside this batch's exact L1 capture window.
    // 既有种子 L2 请求位于本批准确 L1 捕获窗口之外。
    let capture_start = test
        .port
        .requests
        .lock()
        .expect("HTTP text capture start")
        .len();
    // Distinguish folding with equal budgets, then distinguish clipping under the same folding switch.
    // 用相同预算区分折叠，再在相同折叠开关下区分裁剪。
    let folded = folded();
    // Retaining machine text changes only the Boolean field.
    // 保留机器文本仅改变布尔字段。
    let retained = VmmNativeTextPolicySummary {
        fold_machine_text: false,
        max_fragment_tokens: 20_000,
    };
    // Clipping changes only the budget relative to the folded policy.
    // 相对折叠策略，裁剪仅改变预算。
    let clipped = VmmNativeTextPolicySummary {
        fold_machine_text: true,
        max_fragment_tokens: 256,
    };
    // Independent expectations: 0 is retained, 1 folded, 2 clipped; each caller is explicit.
    // 独立预期：零为保留、一为折叠、二为裁剪；每个调用方均明确指定。
    let phases = [
        (
            "text-global",
            &folded,
            None,
            vec![("replacement-host-b", 1)],
        ),
        (
            "text-host",
            &folded,
            Some(&retained),
            vec![("replacement-host-a", 0), ("replacement-host-b", 1)],
        ),
        (
            "text-clipped",
            &clipped,
            Some(&retained),
            vec![("replacement-host-a", 0), ("replacement-host-b", 2)],
        ),
        (
            "text-inherit",
            &clipped,
            None,
            vec![("replacement-host-a", 2)],
        ),
        ("text-final", &folded, None, vec![("replacement-host-b", 1)]),
    ];
    // Exact established log grammar permits observable machine folding without an expected-output stub.
    // 准确的既有日志语法允许观察机器折叠，不使用预期输出桩。
    let logs = (0..20)
        .map(|index| {
            format!(
                "{{\"level\":\"info\",\"msg\":\"sync-{index}\",\"trace\":\"{}\"}}",
                "x".repeat(32)
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    // Prose independently exceeds the small budget even after machine text has been folded.
    // 即使机器文本已折叠，自然语言仍独立超过小预算。
    let prose = "我喜欢西瓜和葡萄，希望记录我的饮食偏好。\n".repeat(80);
    // This original visible body is shared by all four role positions.
    // 此原始可见正文由全部四个角色位置共享。
    let body = format!("{logs}\n{prose}");
    // Hidden reasoning must be removed under every text policy.
    // 隐藏思考在每种文本策略下均必须移除。
    let source = format!("<analysis>http-text-hidden</analysis>\n{body}");
    // Retain actual admitted rows and timelines, never re-derive them under later settings.
    // 保留实际准入记录及时间线，绝不按后续设置重新推导。
    let mut admitted = Vec::new();
    for (phase, global, host, callers) in phases {
        wait_phase(root, phase).await;
        // Actual runtime and both hosts must agree with independently declared saved values.
        // 实际运行时与两宿主必须符合独立声明的保存值。
        let saved = test
            .runtime
            .runtime_policy()
            .await
            .expect("HTTP text saved");
        assert_eq!(&saved.text, global);
        assert_eq!(saved.recall, initial.recall);
        assert_eq!(saved.retrieval, initial.retrieval);
        assert_eq!(saved.profiles, initial.profiles);
        assert_eq!(saved.retention, initial.retention);
        // The optional override follows the host summary's declared inheritance contract.
        // 可选覆盖遵循宿主摘要声明的继承契约。
        let a = test
            .runtime
            .host_project_policy("replacement-host-a")
            .await
            .expect("HTTP text A");
        assert_eq!(a.overrides.text.as_ref(), host);
        assert_eq!(&a.effective.text, host.unwrap_or(global));
        assert!(a.overrides.recall.is_none() && a.overrides.retrieval.is_none());
        // B must inherit structurally, not merely carry an explicit override equal to the global value.
        // B 必须在结构上继承，不能仅携带与全局值相等的显式覆盖。
        let b = test
            .runtime
            .host_project_policy("replacement-host-b")
            .await
            .expect("HTTP text B");
        assert!(b.overrides.text.is_none());
        assert_eq!(b.effective.text, *global);
        for (caller, form) in callers {
            // Unique operation/session identities keep each intake independent of history selection.
            // 唯一操作／会话身份使每次入库不受历史选择影响。
            let operation = format!("http-text-{}", admitted.len());
            // The public request exercises user, intermediate assistant/user and final assistant roles.
            // 公开请求覆盖用户、中间助手／用户和最终助手角色。
            let request = PostActionRequest {
                host_project_id: caller.to_owned(),
                project_id: project.id,
                user_id: 1,
                session_id: operation.clone(),
                operation_id: operation.clone(),
                user_content: source.clone(),
                assistant_content: source.clone(),
                timeline: vec![
                    vulcan_vmm::protocol::PostActionTimelineItem {
                        kind: PostActionTimelineType::Assistant,
                        content: source.clone(),
                    },
                    vulcan_vmm::protocol::PostActionTimelineItem {
                        kind: PostActionTimelineType::User,
                        content: source.clone(),
                    },
                ],
                ..Default::default()
            };
            test.client
                .post_action(request.clone())
                .await
                .expect("HTTP real text intake");
            assert_eq!(request.user_content, source);
            assert_eq!(request.assistant_content, source);
            assert!(request.timeline.iter().all(|item| item.content == source));
            // Load the production serialized queue envelope and inspect its actual admitted body.
            // 加载生产序列化队列封套并检查实际准入正文。
            let job = test
                .data_plane
                .load_pending_analysis_job(&operation)
                .await
                .expect("HTTP text queue")
                .expect("HTTP text job");
            assert_eq!(job.state, VMM_PENDING_ANALYSIS_READY);
            // Decode the declared payload type, not an inferred JSON path or alternate field.
            // 解码声明的载荷类型，不推断 JSON 路径或替代字段。
            let payload: NativePendingPostActionPayload =
                serde_json::from_str(&job.payload_json).expect("HTTP text envelope");
            assert_eq!(payload.host_project_id, caller);
            assert_eq!(
                payload
                    .intake
                    .timeline
                    .iter()
                    .map(|item| &item.role)
                    .collect::<Vec<_>>(),
                vec![&Role::User, &Role::Assistant, &Role::User, &Role::Assistant]
            );
            for item in &payload.intake.timeline {
                assert!(!item.content.contains("http-text-hidden"));
                match form {
                    0 => assert_eq!(item.content, body.trim()),
                    1 => {
                        assert!(item.content.contains("folded by policy"));
                        assert!(item.content.contains(prose.trim()));
                        assert!(!item.content.contains("[truncated by token budget]"));
                    }
                    2 => {
                        assert!(item.content.contains("[truncated by token budget]"));
                        assert!(item.content.len() < body.len());
                    }
                    _ => unreachable!("three declared text forms"),
                }
            }
            admitted.push((job, payload.intake.timeline));
        }
        acknowledge(root, phase);
    }
    assert_eq!(admitted.len(), 7);
    assert_eq!(
        test.port.requests.lock().expect("HTTP before drain").len(),
        capture_start
    );
    for (job, _) in &admitted {
        assert_eq!(
            test.data_plane
                .load_pending_analysis_job(&job.operation_id)
                .await
                .expect("HTTP frozen queue")
                .expect("HTTP ready job"),
            *job
        );
    }
    assert!(admitted.len() <= PENDING_ANALYSIS_BATCH_LIMIT as usize);
    assert!(
        test.client
            .drain_due_pending_analysis_jobs()
            .await
            .expect("HTTP text drain")
    );
    // Inspect disk after actual analysis, not merely the live runtime projection.
    // 在实际分析后检查磁盘，不仅检查活动运行时投影。
    let persisted: vulcan_memorymesh_core::StorageSnapshot = serde_json::from_str(
        &test
            .data_plane
            .load_core_snapshot_json()
            .await
            .expect("HTTP core bytes")
            .expect("HTTP core snapshot"),
    )
    .expect("HTTP decoded snapshot");
    assert_eq!(persisted.dataset.turns.len(), admitted.len());
    assert_eq!(persisted.dataset.memories, original_disk.dataset.memories);
    assert_eq!(
        test.runtime
            .storage_snapshot()
            .expect("HTTP after text vectors")
            .dataset
            .memories,
        memories
    );
    // Scope captures to the seven actual L1 requests issued after all UI saves.
    // 将捕获范围限定为全部界面保存后发出的七个实际 L1 请求。
    let captures = test.port.requests.lock().expect("HTTP text captures")[capture_start..].to_vec();
    assert_eq!(captures.len(), admitted.len());
    for (job, timeline) in admitted {
        // Successful receipts discard duplicate bodies but retain the canonical completed turn.
        // 成功回执丢弃重复正文，但保留规范已完成回合。
        let completed = test
            .data_plane
            .load_pending_analysis_job(&job.operation_id)
            .await
            .expect("HTTP completed read")
            .expect("HTTP completed row");
        assert_eq!(
            completed.state,
            vulcan_workspace_store::vmm_data_plane::VMM_PENDING_ANALYSIS_COMPLETED,
            "{:?}",
            completed.last_error
        );
        assert!(completed.payload_json.is_empty());
        // Match exact persistent identity rather than container iteration order.
        // 匹配准确持久身份，而不是容器迭代顺序。
        let turn = persisted
            .dataset
            .turns
            .iter()
            .find(|turn| Some(turn.id.as_str()) == job.turn_id.as_deref())
            .expect("HTTP persisted turn");
        assert_eq!(turn.timeline, timeline);
        assert_eq!(turn.status, TurnStatus::Completed);
        // A single real rendered request must contain the exact body frozen at intake.
        // 唯一真实渲染请求必须包含入库时冻结的准确正文。
        let inputs = captures
            .iter()
            .map(analysis_input)
            .filter(|input| input["target_turn"]["turn_id"].as_str() == job.turn_id.as_deref())
            .collect::<Vec<_>>();
        assert_eq!(inputs.len(), 1);
        // The raw-turn schema explicitly separates boundary roles from the intermediate timeline.
        // 原始回合结构明确区分边界角色和中间时间线。
        let raw = &inputs[0]["target_turn"]["raw_turn"];
        assert_eq!(raw["user"], timeline[0].content);
        assert_eq!(raw["assistant"], timeline[3].content);
        assert_eq!(
            raw["timeline"].as_array().expect("HTTP L1 timeline").len(),
            2
        );
        assert_eq!(raw["timeline"][0]["content"], timeline[1].content);
        assert_eq!(raw["timeline"][1]["content"], timeline[2].content);
    }
    assert!(
        !test
            .client
            .drain_due_pending_analysis_jobs()
            .await
            .expect("HTTP empty queue")
    );
}
