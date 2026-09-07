//! Real policy-save, intake and queued-analysis coverage for both editable text controls.
//! 对两个可编辑文本控制项提供真实策略保存、入库及排队分析覆盖。

use super::*;
use vulcan_service_api::{VmmMemoryActionResponse, VmmNativeTextPolicySummary};

/// Saves a global text group through the public service and checks the exact accepted values.
/// 通过公开服务保存全局文本组，并检查准确接纳值。
/// Takes isolated service state and a policy; returns after validating the typed response.
/// 接收隔离服务状态及策略；验证类型化响应后返回。
async fn save_global(
    service: &crate::state::SharedServiceBridgeState,
    policy: &VmmNativeTextPolicySummary,
) {
    // Only the production dispatcher constructs the response used by these assertions.
    // 断言所使用的响应仅由生产派发器构造。
    let response = policy_service::action(
        service,
        "native_text_policy_update",
        serde_json::to_value(policy).expect("text policy JSON"),
    )
    .await
    .expect("save global text");
    // Reject an unrelated success response rather than treating any successful action as a save.
    // 拒绝无关成功响应，而不把任意动作成功视为保存成功。
    let VmmMemoryActionResponse::NativeRuntimePolicy(response) = response else {
        panic!("expected runtime policy");
    };
    assert_eq!(response.text, *policy);
}

/// Saves or removes A's text-only override and verifies both explicit and effective group ownership.
/// 保存或移除 A 的仅文本覆盖，并验证显式组与有效组归属。
/// Takes service state, optional override and expected effective text; returns the checked service result.
/// 接收服务状态、可选覆盖及预期有效文本；检查服务结果后返回。
async fn save_a(
    service: &crate::state::SharedServiceBridgeState,
    policy: Option<VmmNativeTextPolicySummary>,
    effective: &VmmNativeTextPolicySummary,
) {
    // All other groups remain explicitly inherited.
    // 所有其他组保持明确继承。
    let overrides = VmmNativeHostProjectPolicyOverrides {
        text: policy,
        ..Default::default()
    };
    // Use the exact host identity, never its shared memory-project ID.
    // 使用准确宿主身份，绝不使用其共享的记忆项目 ID。
    let response = policy_service::action(
        service,
        "native_host_project_policy_update",
        serde_json::json!({
            "hostProjectId": "text-host-a", "overrides": overrides
        }),
    )
    .await
    .expect("save caller text");
    // Keep the documented host response distinct from the global response.
    // 保持已定义宿主响应与全局响应的区别。
    let VmmMemoryActionResponse::NativeHostProjectPolicy(response) = response else {
        panic!("expected host policy");
    };
    assert_eq!(response.host_project_id, "text-host-a");
    assert_eq!(response.overrides, overrides);
    assert_eq!(response.effective.text, *effective);
}

/// Runs a six-admission policy transition sequence on a temporary real data plane and declared AI route.
/// 在临时真实数据面与已声明 AI 路由上执行六次准入的策略变化序列。
/// Verifies original requests, durable queue text, persisted turns and actual L1 inputs; only inference is replaced.
/// 验证原始请求、耐久队列正文、持久回合及实际 L1 输入；仅替换推理。
pub(in crate::native_vmm) async fn verify(
    data_plane: VmmDataPlaneHandle,
    capability: InferenceCapabilitySnapshot,
) {
    // Empty model extraction isolates text intake from unrelated nonempty memory/profile fixture semantics.
    // 空模型提取将文本入库与无关非空记忆／画像夹具语义隔离。
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
        .expect("text capability");
    // Bootstrap actual Core and resources from the isolated controller's disk.
    // 从隔离控制器磁盘引导实际 Core 与资源。
    let runtime = Arc::new(
        NativeVmmCoreRuntime::bootstrap(port.clone(), data_plane.clone())
            .await
            .expect("text runtime"),
    );
    // Intermediate roles bypass simple-turn noise; semantic prototypes are unrelated to this policy test.
    // 中间角色绕过简单回合噪音；语义原型与本策略测试无关。
    let mut settings = runtime
        .resource_catalog()
        .await
        .expect("text resources")
        .settings;
    settings.noise_semantic_enabled = false;
    runtime
        .update_resource_settings(settings)
        .await
        .expect("text noise setup");
    // A and B share memory ownership so only the original caller can select their text rules.
    // A 与 B 共享记忆归属，因此只有原调用方才能选择各自文本规则。
    let project = data_plane
        .create_project("Text policy", "text-policy")
        .await
        .expect("text project");
    for caller in ["text-host-a", "text-host-b"] {
        data_plane
            .bind_vulcan_project(caller, project.id)
            .await
            .expect("text binding");
    }
    // Share the real operation gate across the service and client.
    // 在服务与客户端之间共享实际操作锁。
    let gate = Arc::new(RwLock::new(()));
    // Install the actual dispatcher against the same runtime.
    // 将实际派发器安装到同一运行时。
    let service = policy_service::service(runtime.clone(), data_plane.clone(), gate.clone());
    // These explicit values independently distinguish folding from fragment-budget truncation.
    // 这些明确值独立区分折叠与片段预算裁剪。
    let folded = VmmNativeTextPolicySummary {
        fold_machine_text: true,
        max_fragment_tokens: 20_000,
    };
    // A keeps machine text under the same generous budget.
    // A 在相同宽裕预算下保留机器文本。
    let retained = VmmNativeTextPolicySummary {
        fold_machine_text: false,
        ..folded.clone()
    };
    // The second global save reduces prose in addition to machine folding.
    // 第二次全局保存除机器折叠外还缩减自然语言。
    let clipped = VmmNativeTextPolicySummary {
        max_fragment_tokens: 256,
        ..folded.clone()
    };
    save_global(&service, &folded).await;
    save_a(&service, Some(retained.clone()), &retained).await;
    // Construct only the normal native client; no background worker races the explicit queue drains.
    // 仅构造普通原生客户端；没有后台工作器与明确队列排空竞争。
    let client = NativeVmmClient::new(
        runtime.clone(),
        data_plane.clone(),
        gate,
        Arc::new(AtomicBool::new(true)),
        Arc::new(Notify::new()),
    );
    // Reuse the established machine-fold fixture grammar, followed by long natural-language prose.
    // 复用既有机器折叠夹具语法，随后放置较长自然语言。
    let logs = (0..20)
        .map(|index| {
            format!(
                "{{\"level\":\"info\",\"msg\":\"sync-{index}\",\"trace\":\"{}\"}}",
                "x".repeat(32)
            )
        })
        .collect::<Vec<_>>()
        .join("\n");
    // Natural language independently exceeds the small target even after all logs are folded.
    // 即使所有日志均折叠，自然语言仍独立超过小预算。
    let prose = "我喜欢西瓜和葡萄，希望记录我的饮食偏好。\n".repeat(80);
    // Only this cleaned body may survive; the hidden analysis marker must never reach storage or L1.
    // 只有此清理后正文可保留；隐藏分析标记绝不能进入存储或 L1。
    let body = format!("{logs}\n{prose}");
    // All four input roles carry the same source so role-specific sanitizer omissions are observable.
    // 四种输入角色携带相同来源，以便观察角色特定的清洗遗漏。
    let source = format!("<analysis>text-policy-hidden</analysis>\n{body}");
    // Keep each accepted durable job and its exact canonical timeline before any later policy edit.
    // 在后续任何策略编辑前保留每个接纳的耐久作业及其准确规范时间线。
    let mut admitted = Vec::new();
    for (index, caller) in [
        "text-host-a",
        "text-host-b",
        "text-host-a",
        "text-host-b",
        "text-host-a",
        "text-host-b",
    ]
    .into_iter()
    .enumerate()
    {
        match index {
            2 => save_global(&service, &clipped).await,
            4 => save_a(&service, None, &clipped).await,
            5 => save_global(&service, &folded).await,
            _ => {}
        }
        // Each input owns a distinct session so history selection cannot obscure fragment policy behavior.
        // 每份输入持有独立会话，避免历史选择掩盖片段策略行为。
        let operation = format!("text-policy-{index}");
        // Preserve the original request alongside the clone admitted through the real client.
        // 在通过真实客户端准入的副本之外保留原始请求。
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
        client
            .post_action(request.clone())
            .await
            .expect("real text intake");
        assert_eq!(request.user_content, source);
        assert_eq!(request.assistant_content, source);
        assert!(request.timeline.iter().all(|item| item.content == source));
        // Read the actual queue payload, including the caller and sanitized fragments.
        // 读取实际队列参数，包括调用方及清洗后片段。
        let job = data_plane
            .load_pending_analysis_job(&operation)
            .await
            .expect("text job read")
            .expect("text job");
        assert_eq!(job.state, VMM_PENDING_ANALYSIS_READY);
        // Decode the production persisted envelope without constructing expected serialized payloads.
        // 解码生产持久化封套，不构造预期序列化参数。
        let payload: NativePendingPostActionPayload =
            serde_json::from_str(&job.payload_json).expect("text payload");
        assert_eq!(payload.host_project_id, caller);
        assert_eq!(payload.intake.timeline.len(), 4);
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
            assert!(!item.content.contains("text-policy-hidden"));
            match index {
                0 | 2 => assert_eq!(item.content, body.trim()),
                1 | 5 => {
                    assert!(item.content.contains("folded by policy"));
                    assert!(item.content.contains(prose.trim()));
                    assert!(!item.content.contains("[truncated by token budget]"));
                }
                3 | 4 => {
                    assert!(item.content.contains("[truncated by token budget]"));
                    assert!(item.content.len() < body.len());
                }
                _ => unreachable!("six explicit text cases"),
            }
        }
        admitted.push((job, payload.intake.timeline));
    }
    assert!(
        port.requests
            .lock()
            .expect("text captures before drain")
            .is_empty()
    );
    // All queue drains occur under the final large folded policy, not each original intake policy.
    // 所有队列排空均发生于最终大预算折叠策略下，而不是各次原始入库策略下。
    for (job, _) in &admitted {
        // A ready row retains its exact admitted payload after every intervening policy save.
        // 就绪记录在全部中间策略保存后仍保留准确准入参数。
        let ready = data_plane
            .load_pending_analysis_job(&job.operation_id)
            .await
            .expect("unchanged ready text read")
            .expect("ready text row");
        assert_eq!(ready.state, VMM_PENDING_ANALYSIS_READY);
        assert_eq!(ready.payload_json, job.payload_json);
    }
    // One bounded pass processes independent session heads, unlike the single-session history fixture.
    // 一次有界扫描处理独立会话队首，与单会话历史夹具不同。
    assert!(admitted.len() <= PENDING_ANALYSIS_BATCH_LIMIT as usize);
    assert!(
        client
            .drain_due_pending_analysis_jobs()
            .await
            .expect("text drain")
    );
    for (job, _) in &admitted {
        // Keep the exact acknowledged job; intake success alone is insufficient.
        // 保留准确已确认作业；仅入库成功并不足够。
        let completed = data_plane
            .load_pending_analysis_job(&job.operation_id)
            .await
            .expect("text completion read")
            .expect("text completion");
        assert_eq!(
            completed.state,
            vulcan_workspace_store::vmm_data_plane::VMM_PENDING_ANALYSIS_COMPLETED,
            "{:?}",
            completed.last_error
        );
        // Completed queue receipts deliberately discard duplicate raw text; canonical turns remain authoritative.
        // 完成队列回执有意丢弃重复原文；规范回合保持权威。
        assert!(completed.payload_json.is_empty());
    }
    // Read SQLite again after all analysis commits instead of relying only on the live Core projection.
    // 在全部分析提交后再次读取 SQLite，不只依赖活动 Core 投影。
    let persisted: vulcan_memorymesh_core::StorageSnapshot = serde_json::from_str(
        &data_plane
            .load_core_snapshot_json()
            .await
            .expect("text snapshot bytes")
            .expect("text snapshot"),
    )
    .expect("text snapshot JSON");
    assert_eq!(persisted.dataset.turns.len(), admitted.len());
    // Own captures after completion; no synchronous guard crosses an asynchronous operation.
    // 完成后独立持有捕获；同步守卫不跨越异步操作。
    let captures = port.requests.lock().expect("text L1 captures").clone();
    assert_eq!(captures.len(), admitted.len());
    for (job, timeline) in admitted {
        // Match by the exact admitted turn identity, independently of container iteration order.
        // 按准确准入回合身份匹配，不依赖容器迭代顺序。
        let turn = persisted
            .dataset
            .turns
            .iter()
            .find(|turn| Some(turn.id.as_str()) == job.turn_id.as_deref())
            .expect("persisted text turn");
        assert_eq!(turn.timeline, timeline);
        assert_eq!(turn.status, TurnStatus::Completed);
        // Identify the sole actual model request for this turn.
        // 识别此回合唯一的实际模型请求。
        let inputs = captures
            .iter()
            .map(analysis_input)
            .filter(|input| input["target_turn"]["turn_id"].as_str() == job.turn_id.as_deref())
            .collect::<Vec<_>>();
        assert_eq!(inputs.len(), 1);
        // Model-visible role bodies must remain exactly those frozen at intake, not recleaned under current settings.
        // 模型可见角色正文必须准确保持入库时冻结值，不按当前设置重新清洗。
        let raw = &inputs[0]["target_turn"]["raw_turn"];
        assert_eq!(raw["user"], timeline[0].content);
        assert_eq!(raw["assistant"], timeline[3].content);
        assert_eq!(
            raw["timeline"]
                .as_array()
                .expect("L1 intermediate roles")
                .len(),
            2
        );
        assert_eq!(raw["timeline"][0]["content"], timeline[1].content);
        assert_eq!(raw["timeline"][1]["content"], timeline[2].content);
    }
    assert!(
        !client
            .drain_due_pending_analysis_jobs()
            .await
            .expect("empty text queue")
    );
}
