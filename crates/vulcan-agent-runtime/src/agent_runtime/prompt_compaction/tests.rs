use super::pruning::{
    compaction_attempt_conversation_state, compaction_prompt_mode_for_prune_mode,
    initial_compaction_prune_mode, next_compaction_retry_mode,
    prepared_compaction_source_for_attempt, prompt_compaction_error_is_context_overflow,
};
use super::*;
use vulcan_ai_contract::{ContentItem, MessageRole};
use vulcan_context::conversation_state_from_session_messages;
use vulcan_workbench_model::{
    SessionMediaUnderstandingModelSelection, SessionMediaUnderstandingReasoningEffort,
    SessionMediaUnderstandingSelection,
};

fn test_budget() -> CompactionInputBudget {
    CompactionInputBudget {
        target_input_budget_tokens: 100,
        safety_reserve_tokens: 30_000,
    }
}

/// Builds one transcript message entry for compaction helper tests.
/// 为压缩辅助函数测试构建一条转录消息。
fn test_message(id: &str, body: &str) -> vulcan_context::SessionMessageEntry {
    vulcan_context::SessionMessageEntry::new(vulcan_context::SessionMessageKind::User, "", body)
        .with_runtime_metadata(id, "t1")
}

/// Builds one minimal compaction plan from selected source messages.
/// 从选定源消息构建一个最小压缩计划。
fn test_plan(source_messages: Vec<vulcan_context::SessionMessageEntry>) -> CompactionPlan {
    // policy and tailSelection model a source-only request with no retained canonical suffix.
    // policy 与 tailSelection 模拟不含规范保留后缀的纯来源请求。
    let policy = vulcan_context::ContextCompactionPolicy::default();
    CompactionPlan {
        source_messages,
        source_until_message_id: None,
        source_until_turn_index: None,
        retained_tail_start_turn_index: None,
        compaction_id: String::from("compaction-test"),
        created_at: String::from("t1"),
        policy,
        tail_selection: vulcan_context_compaction::CompactionTailSelectionReport {
            context_limit: policy.tail_context_limit,
            policy_source: policy.tail_policy_source,
            nominal_token_budget: policy.preserve_recent_tokens,
            segment_limit: policy.tail_turns,
            retained_segment_count: 0,
            retained_message_ids: Vec::new(),
            retained_token_estimate: 0,
            previewed_tool_output_count: 0,
            omitted_tool_output_token_estimate: 0,
            newest_segment_exceeds_nominal_budget: false,
        },
    }
}

/// Verifies remote compact usage is normalized for turn statistics without discarding raw metadata.
/// 验证远端 compact 用量会被规范化用于回合统计，同时不丢弃原始元数据。
#[test]
fn remote_compact_usage_is_projected_into_turn_accounting() {
    let raw_usage = serde_json::json!({
        "input_tokens": 40,
        "input_tokens_details": {"cached_tokens": 10},
        "output_tokens": 8,
        "output_tokens_details": {"reasoning_tokens": 3},
        "total_tokens": 48
    });

    let usage = remote_responses_usage_to_core(Some(&raw_usage))
        .expect("object usage should be normalized");
    assert_eq!(usage.input_tokens, Some(40));
    assert_eq!(usage.uncached_input_tokens, Some(30));
    assert_eq!(usage.cached_input_tokens, Some(10));
    assert_eq!(usage.output_tokens, Some(8));
    assert_eq!(usage.reasoning_tokens, Some(3));
    assert_eq!(usage.total_tokens, Some(48));
    assert!(remote_responses_usage_to_core(Some(&serde_json::Value::Null)).is_none());
}

/// Verifies a stale session connection is rejected instead of being replaced during compaction.
/// 验证压缩期间会拒绝失效的会话连接，而不会将其替换为其他连接。
#[test]
fn compaction_target_rejects_stale_exact_connection() {
    // runtime provides the real connection resolver with an empty authoritative graph.
    // runtime 使用空权威连接图提供真实连接解析器。
    let runtime = vulcan_provider_adapters::PublicAiRuntime::unavailable()
        .expect("test public AI runtime should initialize");
    // result must preserve the exact stale connection boundary instead of entering model routing.
    // result 必须保留精确失效连接边界，而不能进入模型级路由选择。
    let result = resolve_prompt_compaction_model_target(
        &runtime,
        "session-stale-connection",
        None,
        Some("custom:test"),
        Some("removed-connection"),
        Some("gpt-test"),
        None,
        None,
    );
    let Err(error) = result else {
        panic!("stale exact compaction connection must be rejected");
    };

    assert_eq!(error, "Connection `removed-connection` is not configured.");
}

/// Verifies internal compaction requests cannot reapply dynamic VMM overlays to canonical source messages.
/// 验证内部压缩请求不会把动态 VMM 覆盖层重新应用到规范来源消息。
#[test]
fn compaction_launch_request_excludes_dynamic_vmm_injection() {
    let conversation_state = conversation_state_from_session_messages(&[test_message(
        "user-1",
        "canonical user content",
    )])
    .expect("canonical compaction source should replay");

    let request = build_prompt_compaction_launch_request(
        crate::model::SessionId::from_u128(1),
        "session",
        "D:/workspace",
        None,
        &PromptCompactionLaunchContext::default(),
        None,
        ReasoningEffortLevel::Off,
        conversation_state,
        ContextCompactionRequestPromptMode::FullFidelityMerge,
    );

    assert!(request.context_injection.is_none());
    assert!(request.provider_visible_agent_messages.is_empty());
    assert_eq!(request.project_vmm, Default::default());
    assert_eq!(request.session_vmm, Default::default());
}

/// Verifies local prompt compaction preserves the exact route while disabling recursive automatic compaction.
/// 验证本地提示词压缩保留精确路由，同时禁用递归自动压缩。
#[test]
fn compaction_launch_request_preserves_session_model_target() {
    // conversationState supplies one canonical source message for request construction.
    // conversationState 为请求构建提供一条规范来源消息。
    let conversation_state = conversation_state_from_session_messages(&[test_message(
        "user-1",
        "canonical user content",
    )])
    .expect("canonical compaction source should replay");
    // modelTarget represents the exact account selected by the owning session.
    // modelTarget 表示所属会话选择的精确账号。
    let model_target = PromptCompactionModelTarget {
        provider_id: String::from("custom:test"),
        connection_id: String::from("connection-test"),
        model_id: String::from("gpt-test"),
        provider_route_lease_identity: vulcan_workbench_model::SessionProviderRouteLeaseIdentity {
            schema_version: 1,
            provider_id: String::from("custom:test"),
            connection_id: String::from("connection-test"),
            model_id: String::from("gpt-test"),
            protocol_profile_id: String::from("openai-responses.v1"),
            manifest_revision: 7,
            manifest_generation: 11,
            catalog_revision: String::from("catalog-test"),
            route_revision: String::from("route-test"),
        },
        remote_compaction_enabled: true,
        context_limit: Some(300_000),
        context_compaction_policy: vulcan_context::ContextCompactionPolicy {
            tail_turns: 6,
            preserve_recent_tokens: 15_000,
            tail_context_limit: Some(300_000),
            tail_policy_source:
                vulcan_context::ContextCompactionTailPolicySource::DeclaredContextLimit,
            ..vulcan_context::ContextCompactionPolicy::default()
        },
    };

    // request must project all three target fields into execution preparation.
    // request 必须把三个目标字段全部投影到执行准备请求。
    let request = build_prompt_compaction_launch_request(
        crate::model::SessionId::from_u128(1),
        "session",
        "D:/workspace",
        Some("agent-test"),
        &PromptCompactionLaunchContext::default(),
        Some(&model_target),
        ReasoningEffortLevel::Off,
        conversation_state,
        ContextCompactionRequestPromptMode::FullFidelityMerge,
    );

    assert_eq!(request.provider_id_override.as_deref(), Some("custom:test"));
    // expectedCompactionPolicy keeps every hard-window and retained-tail field while disabling only recursive auto-triggering.
    // expectedCompactionPolicy 保留全部硬窗口与尾部保留字段，仅禁用递归自动触发。
    let expected_compaction_policy = vulcan_context::ContextCompactionPolicy {
        auto: false,
        ..model_target.context_compaction_policy
    };
    assert_eq!(
        request.context_compaction_policy,
        Some(expected_compaction_policy)
    );
    assert_eq!(
        request.connection_id_override.as_deref(),
        Some("connection-test")
    );
    assert_eq!(request.model_id_override.as_deref(), Some("gpt-test"));
    assert_eq!(
        request
            .provider_route_lease_identity
            .as_ref()
            .map(|identity| identity.route_revision.as_str()),
        Some("route-test")
    );
    assert_eq!(request.execution_agent_id.as_deref(), Some("agent-test"));
}

/// Verifies compaction preparation preserves ordinary-turn mode, provider tools, and prompt-skill controls.
/// 验证压缩准备会保留普通回合的模式、供应商工具与提示词技能控制。
#[test]
fn compaction_launch_request_preserves_ordinary_turn_controls() {
    // launchContext represents controls already materialized for the owning ordinary turn.
    // launchContext 表示已经为所属普通回合物化的控制项。
    let launch_context = PromptCompactionLaunchContext {
        temperature: Some(0.25),
        captured_child_tool_admission: Some(BTreeMap::from([(
            String::from("read"),
            vulcan_session_core::v1::child_execution::ChildToolOrigin {
                source_kind: vulcan_session_core::v1::child_execution::ChildToolSourceKind::Mcp,
                namespace: String::from("server"),
                original_name: String::from("read-file"),
            },
        )])),
        execution_mode: String::from("plan"),
        provider_tool_selections_override: Some(BTreeMap::from([(
            String::from("web_search"),
            SessionProviderToolTargetSelection {
                connection_id: String::from("tool-connection"),
                target_id: String::from("tool-model"),
            },
        )])),
        provider_tool_enabled_capabilities_override: Some(BTreeSet::from([String::from(
            "web_search",
        )])),
        provider_tool_media_understanding_selections_override: Some(BTreeMap::from([(
            String::from("video_understand"),
            SessionMediaUnderstandingSelection::ModelReplacement {
                selection: SessionMediaUnderstandingModelSelection {
                    provider_id: String::from("provider-a"),
                    model_id: String::from("model-a"),
                    connection_id: String::from("connection-a"),
                },
                reasoning_effort: Some(SessionMediaUnderstandingReasoningEffort::Adaptive),
            },
        )])),
        tool_request_hidden_tool_ids: vec![String::from("hidden-tool")],
        session_prompt_skill_overrides: BTreeMap::from([(
            String::from("review"),
            PersistedPromptSkillState::Enabled,
        )]),
        ..PromptCompactionLaunchContext::default()
    };
    let conversation_state = conversation_state_from_session_messages(&[test_message(
        "user-1",
        "canonical user content",
    )])
    .expect("canonical compaction source should replay");

    // request must retain every control that changes model-visible prompt or tool shape.
    // request 必须保留会改变模型可见提示词或工具形态的全部控制项。
    let request = build_prompt_compaction_launch_request(
        crate::model::SessionId::from_u128(1),
        "session",
        "D:/workspace",
        None,
        &launch_context,
        None,
        ReasoningEffortLevel::Off,
        conversation_state,
        ContextCompactionRequestPromptMode::FullFidelityMerge,
    );

    assert_eq!(request.execution_mode, "plan");
    assert_eq!(request.temperature, Some(0.25));
    assert_eq!(
        PromptCompactionLaunchContext::from_execution_request(&request).temperature,
        Some(0.25)
    );
    assert_eq!(
        request.captured_child_tool_admission,
        launch_context.captured_child_tool_admission
    );
    assert_eq!(
        PromptCompactionLaunchContext::from_execution_request(&request)
            .captured_child_tool_admission,
        launch_context.captured_child_tool_admission
    );
    assert_eq!(
        request
            .provider_tool_selections_override
            .as_ref()
            .and_then(|selections| selections.get("web_search"))
            .map(|selection| selection.target_id.as_str()),
        Some("tool-model")
    );
    assert_eq!(
        request.provider_tool_enabled_capabilities_override,
        Some(BTreeSet::from([String::from("web_search")]))
    );
    assert_eq!(
        request.provider_tool_media_understanding_selections_override,
        Some(BTreeMap::from([(
            String::from("video_understand"),
            SessionMediaUnderstandingSelection::ModelReplacement {
                selection: SessionMediaUnderstandingModelSelection {
                    provider_id: String::from("provider-a"),
                    model_id: String::from("model-a"),
                    connection_id: String::from("connection-a"),
                },
                reasoning_effort: Some(SessionMediaUnderstandingReasoningEffort::Adaptive),
            },
        )]))
    );
    assert_eq!(
        request.tool_request_hidden_tool_ids,
        vec![String::from("hidden-tool")]
    );
    assert_eq!(
        request.session_prompt_skill_overrides.get("review"),
        Some(&PersistedPromptSkillState::Enabled)
    );
}

/// Verifies rendered ordinary instructions are separated from compact input for Codex encoding.
/// 验证渲染后的普通指令会与压缩输入分离，以供 Codex 编码。
#[test]
fn remote_compaction_request_carries_ordinary_base_instructions() {
    // messages starts with canonical user input before the system instruction is attached.
    // messages 在附加系统指令前仅包含规范用户输入。
    let mut messages = vec![AiMessage {
        role: MessageRole::User,
        content: vec![ContentItem::Text {
            text: String::from("canonical user content"),
        }],
        provider_context_items: Vec::new(),
    }];
    prepend_remote_compaction_instructions(
        &mut messages,
        String::from("ordinary Codex base instructions"),
    );

    let request = build_remote_responses_compaction_request(
        crate::model::SessionId::from_u128(1),
        "gpt-test",
        messages,
        Vec::new(),
        Vec::new(),
        ReasoningEffortLevel::Off,
    );

    assert_eq!(request.messages[0].role, MessageRole::System);
    assert_eq!(
        request.messages[0].content,
        vec![ContentItem::Text {
            text: String::from("ordinary Codex base instructions")
        }]
    );
    assert_eq!(request.messages[1].role, MessageRole::User);
}

/// Verifies remote compaction preserves official Codex identity and current-turn reasoning controls.
/// 验证远端压缩保留官方 Codex 身份与当前回合推理控制。
#[test]
fn remote_compaction_request_preserves_identity_and_turn_reasoning_controls() {
    // messages contain the lossless route-projected input installed into the compact request.
    // messages 包含安装到压缩请求中的无损路由投影输入。
    let messages = vec![AiMessage {
        role: MessageRole::User,
        content: vec![ContentItem::Text {
            text: String::from("canonical user content"),
        }],
        provider_context_items: Vec::new(),
    }];
    // sessionId is the exact stable identity shared by the root Codex session and thread.
    // sessionId 是 Codex 根会话与线程共享的精确稳定身份。
    let session_id = crate::model::SessionId::from_u128(1);
    // tools proves remote compaction retains the same function schema snapshot as ordinary execution.
    // tools 证明远端压缩保留与普通执行相同的函数 Schema 快照。
    let tools = vec![ToolDefinition {
        name: String::from("lookup"),
        description: String::from("Looks up one value."),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {"key": {"type": "string"}},
            "required": ["key"]
        }),
        input_format: Default::default(),
        strict: false,
    }];
    // nativeTools proves provider-native capability declarations also enter the compact request.
    // nativeTools 证明 Provider 原生能力声明也进入压缩请求。
    let native_tools = vec![NativeToolKind::WebSearch];

    // enabledRequest must carry both the active effort and automatic reasoning summary control.
    // enabledRequest 必须同时携带活动推理档位与自动推理摘要控制。
    let enabled_request = build_remote_responses_compaction_request(
        session_id,
        "gpt-test",
        messages.clone(),
        tools.clone(),
        native_tools.clone(),
        ReasoningEffortLevel::Medium,
    );
    assert_eq!(
        enabled_request.reasoning_effort,
        Some(vulcan_ai_contract::ReasoningEffort::Medium)
    );
    assert_eq!(
        enabled_request.reasoning_output,
        ReasoningOutputPolicy::SummaryAuto
    );
    assert_eq!(
        enabled_request.cache_scope_id.as_deref(),
        Some(session_id.to_string().as_str())
    );
    assert_eq!(enabled_request.tools, tools);
    assert_eq!(enabled_request.native_tools, native_tools);

    // disabledRequest must omit both controls instead of creating an invalid mixed state.
    // disabledRequest 必须同时省略两个控制，避免形成无效混合状态。
    let disabled_request = build_remote_responses_compaction_request(
        session_id,
        "gpt-test",
        messages,
        Vec::new(),
        Vec::new(),
        ReasoningEffortLevel::Off,
    );
    assert_eq!(disabled_request.reasoning_effort, None);
    assert_eq!(
        disabled_request.reasoning_output,
        ReasoningOutputPolicy::ProviderDefault
    );
}

/// Verifies the fast path uses the full cloned runtime conversation, not the planned source window.
/// 验证快速路径使用完整克隆运行时会话，而不是计划源窗口。
#[test]
fn full_fidelity_attempt_uses_full_context_state() {
    let old_message = test_message("old-user", "old context");
    let tail_message = test_message("tail-user", "recent tail");
    let full_context_state =
        conversation_state_from_session_messages(&[old_message.clone(), tail_message.clone()])
            .expect("valid replayed conversation identities");
    let plan = test_plan(vec![old_message]);
    let prepared = prepared_compaction_source_for_attempt(
        &plan,
        &full_context_state,
        Some(test_budget()),
        CompactionInputPruneMode::None,
    )
    .expect("valid full-fidelity compaction source");
    let attempt_state = compaction_attempt_conversation_state(
        &full_context_state,
        &prepared.source_messages,
        CompactionInputPruneMode::None,
    )
    .expect("valid compaction attempt identities");

    assert_eq!(
        attempt_state.messages.len(),
        full_context_state.messages.len()
    );
    assert_eq!(prepared.source_messages.len(), 2);
    assert_eq!(prepared.prune_report.mode, CompactionInputPruneMode::None);
}

/// Verifies full-fidelity summaries still commit the planner boundary.
/// 验证完整保真摘要仍按规划器边界提交。
#[test]
fn full_fidelity_result_boundary_preserves_planned_tail() {
    let old_message = test_message("old-user", "old context");
    let mut plan = test_plan(vec![old_message.clone()]);
    plan.source_until_message_id = Some(old_message.id.clone());
    plan.source_until_turn_index = Some(0);
    plan.retained_tail_start_turn_index = Some(1);
    let result = CompactionResult::from_model_summary(&plan, "summary");

    assert_eq!(
        result.source_until_message_id.as_deref(),
        Some(old_message.id.as_str())
    );
    assert_eq!(result.source_until_turn_index, Some(0));
    assert_eq!(result.retained_tail_start_turn_index, Some(1));
}

/// Verifies applying the planned boundary keeps recent tail messages.
/// 验证应用规划边界后会保留最近尾部消息。
#[test]
fn full_fidelity_boundary_rewrite_keeps_planned_tail() {
    let old_message = vulcan_context::SessionMessageEntry::new(
        vulcan_context::SessionMessageKind::Assistant,
        "",
        "old completed provider segment",
    )
    .with_runtime_metadata("old-assistant", "t1");
    let tail_message = vulcan_context::SessionMessageEntry::new(
        vulcan_context::SessionMessageKind::Assistant,
        "",
        "recent provider segment",
    )
    .with_runtime_metadata("tail-assistant", "t2");
    let mut state =
        conversation_state_from_session_messages(&[old_message.clone(), tail_message.clone()])
            .expect("valid replayed conversation identities");
    let source_messages = vulcan_context::conversation_state_to_compaction_source_messages(&state)
        .expect("valid compaction source messages");
    let plan = vulcan_context_compaction::plan_context_compaction(
        vulcan_context_compaction::CompactionPlanInput {
            session_label: "full-fidelity-boundary",
            messages: &source_messages,
            all_messages: &source_messages,
            context_start_message_id: None,
            source_until_message_id: None,
            policy: vulcan_context::ContextCompactionPolicy {
                tail_turns: 1,
                preserve_recent_tokens: 5_000,
                ..vulcan_context::ContextCompactionPolicy::default()
            },
        },
    )
    .expect("two assistant segments should produce a safe frozen plan");
    let result = CompactionResult::from_model_summary(&plan, "summary");

    vulcan_context_compaction::compact_conversation_with_compaction_result(
        &mut state,
        "summary-message",
        &result,
    )
    .expect("valid compacted message identities");

    assert_eq!(state.messages.len(), 2);
    assert_eq!(state.messages[0].id, "summary-message");
    assert_eq!(state.messages[1].id, tail_message.id);
}

/// Verifies prompt variants follow the prepared input pruning mode.
/// 验证提示词变体会跟随已准备输入的剪枝模式。
#[test]
fn prompt_mode_follows_input_prune_mode() {
    assert_eq!(
        compaction_prompt_mode_for_prune_mode(CompactionInputPruneMode::None),
        ContextCompactionRequestPromptMode::FullFidelityMerge
    );
    assert_eq!(
        compaction_prompt_mode_for_prune_mode(CompactionInputPruneMode::RequiredForWindowDowngrade),
        ContextCompactionRequestPromptMode::PrunedFallback
    );
    assert_eq!(
        compaction_prompt_mode_for_prune_mode(CompactionInputPruneMode::RetryAfterFailure),
        ContextCompactionRequestPromptMode::PrunedFallback
    );
}

/// Verifies observed provider usage decides initial downgrade before local estimates.
/// 验证 Provider 实测用量优先于本地估算决定初始降级。
#[test]
fn observed_usage_drives_initial_downgrade_decision() {
    let full_context_state =
        conversation_state_from_session_messages(&[test_message("small-user", "small")])
            .expect("valid replayed conversation identities");

    assert_eq!(
        initial_compaction_prune_mode(&full_context_state, Some(test_budget()), Some(101), Some(1),)
            .expect("observed usage does not render conversation state"),
        CompactionInputPruneMode::RequiredForWindowDowngrade
    );
}

/// Verifies live budget estimates are used when provider usage is unavailable.
/// 验证 Provider 用量不可用时使用实时预算估算。
#[test]
fn budget_estimate_drives_initial_downgrade_without_observed_usage() {
    let full_context_state =
        conversation_state_from_session_messages(&[test_message("small-user", "small")])
            .expect("valid replayed conversation identities");

    assert_eq!(
        initial_compaction_prune_mode(&full_context_state, Some(test_budget()), None, Some(101),)
            .expect("estimated usage does not render conversation state"),
        CompactionInputPruneMode::RequiredForWindowDowngrade
    );
}

/// Verifies explicit context-overflow failures skip full-fidelity retries.
/// 验证明示上下文溢出失败会跳过完整保真重试。
#[test]
fn context_overflow_error_immediately_downgrades() {
    assert!(prompt_compaction_error_is_context_overflow(
        "maximum context length exceeded"
    ));
    assert_eq!(
        next_compaction_retry_mode(
            "maximum context length exceeded",
            CompactionInputPruneMode::None,
            1,
            0,
            Some(test_budget()),
        ),
        Some(CompactionInputPruneMode::RetryAfterFailure)
    );
}

/// Verifies an internal request-budget boundary retains its cause and immediately selects pruned retry.
/// 验证内部请求预算边界会保留原因并立即选择剪枝重试。
#[test]
fn internal_compaction_budget_boundary_immediately_downgrades() {
    // error retains the exact budget guard reason while registering the context-window boundary for retry selection.
    // error 保留精确预算门原因，同时为重试选择登记上下文窗口边界。
    let error = prompt_compaction_request_budget_error(
        "请求预算门阻止发送：自动压缩线已达到。请先执行上下文压缩后重新组装请求。",
    );

    assert!(error.contains("请求预算门阻止发送"));
    assert!(prompt_compaction_error_is_context_overflow(&error));
    assert_eq!(
        next_compaction_retry_mode(
            &error,
            CompactionInputPruneMode::None,
            1,
            0,
            Some(test_budget()),
        ),
        Some(CompactionInputPruneMode::RetryAfterFailure)
    );
}

/// Verifies non-context failures keep retrying full-fidelity input before downgrade.
/// 验证非上下文失败会先继续重试完整保真输入。
#[test]
fn non_context_error_retries_full_fidelity_before_downgrade() {
    assert_eq!(
        next_compaction_retry_mode(
            "temporary provider connection failed",
            CompactionInputPruneMode::None,
            1,
            0,
            Some(test_budget()),
        ),
        Some(CompactionInputPruneMode::None)
    );
    assert_eq!(
        next_compaction_retry_mode(
            "temporary provider connection failed",
            CompactionInputPruneMode::None,
            2,
            0,
            Some(test_budget()),
        ),
        Some(CompactionInputPruneMode::None)
    );
    assert_eq!(
        next_compaction_retry_mode(
            "temporary provider connection failed",
            CompactionInputPruneMode::None,
            3,
            0,
            Some(test_budget()),
        ),
        Some(CompactionInputPruneMode::RetryAfterFailure)
    );
}

/// Verifies missing input budget allows full retries but never clone-pruned downgrade.
/// 验证缺少输入预算时允许完整重试但不会进入克隆剪枝降级。
#[test]
fn missing_budget_never_downgrades_to_pruning() {
    assert_eq!(
        next_compaction_retry_mode(
            "temporary provider connection failed",
            CompactionInputPruneMode::None,
            1,
            0,
            None,
        ),
        Some(CompactionInputPruneMode::None)
    );
    assert_eq!(
        next_compaction_retry_mode(
            "temporary provider connection failed",
            CompactionInputPruneMode::None,
            3,
            0,
            None,
        ),
        None
    );
}

/// Verifies clone-pruned attempts still retry transient non-context failures.
/// 验证仅克隆剪枝尝试遇到非上下文临时失败时仍会重试。
#[test]
fn pruned_attempt_retries_non_context_failures_before_stopping() {
    assert_eq!(
        next_compaction_retry_mode(
            "temporary provider connection failed",
            CompactionInputPruneMode::RequiredForWindowDowngrade,
            0,
            1,
            Some(test_budget()),
        ),
        Some(CompactionInputPruneMode::RequiredForWindowDowngrade)
    );
    assert_eq!(
        next_compaction_retry_mode(
            "temporary provider connection failed",
            CompactionInputPruneMode::RequiredForWindowDowngrade,
            0,
            3,
            Some(test_budget()),
        ),
        None
    );
}
