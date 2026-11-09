use super::*;
use vulcan_context::conversation_state_from_session_messages;
use vulcan_workbench_model::{
    SessionMediaUnderstandingModelSelection, SessionMediaUnderstandingReasoningEffort,
    SessionMediaUnderstandingSelection,
};

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
    );

    assert!(request.context_injection.is_none());
    assert!(request.provider_visible_agent_messages.is_empty());
    assert_eq!(request.project_vmm, Default::default());
    assert_eq!(request.session_vmm, Default::default());
}

/// Verifies a cache-preserving compaction request declares exactly the source turn's tool list,
/// because any difference changes the provider prompt-cache prefix.
/// 验证保缓存压缩请求声明与来源回合完全相同的工具清单，因为任何差异都会改变供应商提示缓存前缀。
#[test]
fn cache_preserving_compaction_declares_source_turn_tool_list() {
    use crate::agent_runtime::execution_preparation::tests::{
        test_execution_preparation_request, test_runtime_foundation,
    };
    let config = ProviderConfigurationFile::default();
    let foundation = test_runtime_foundation(&config);
    // sourceRequest enables a request-gated tool family so a dropped gate changes the tool list.
    // sourceRequest 启用一个由请求控制的工具族，使丢失的闸门会改变工具清单。
    let mut source_request = test_execution_preparation_request();
    source_request.project_id = Some(String::from("project-a"));
    source_request.knowledge_access = Some(crate::agent_runtime::ExecutionKnowledgeAccessContext {
        project_id: String::from("project-a"),
        library_ids: vec![String::from("550e8400-e29b-41d4-a716-446655440000")],
        search_enabled: true,
    });
    let source = prepare_execution_with_tool_snapshot(&foundation, None, &config, &source_request)
        .expect("source turn preparation should resolve");

    let launch_context = PromptCompactionLaunchContext::from_execution_request(&source_request);
    let compaction_request = build_prompt_compaction_launch_request(
        source_request.session_id,
        "session",
        &source_request.workspace_path,
        None,
        &launch_context,
        None,
        ReasoningEffortLevel::Off,
        SessionConversationState::default(),
    );
    let compaction =
        prepare_execution_with_tool_snapshot(&foundation, None, &config, &compaction_request)
            .expect("compaction preparation should resolve");

    let tool_ids = |definitions: &[vulcan_tools_manager::ResolvedToolDefinition]| {
        definitions
            .iter()
            .map(|definition| definition.descriptor.id.clone())
            .collect::<Vec<_>>()
    };
    let source_ids = tool_ids(&source.tool_snapshot.resolved_tool_definitions);
    assert!(
        !source_ids.is_empty(),
        "source turn should declare tools; the fixture may have changed"
    );
    assert_eq!(
        tool_ids(&compaction.tool_snapshot.resolved_tool_definitions),
        source_ids,
        "compaction must declare the source turn's exact ordered tool list"
    );
    // Declared tools stay non-executable during compaction.
    // 压缩期间声明的工具仍不可执行。
    assert!(compaction.preparation.tool_request_rejection_all_tools);
    // Hooks, recall and writeback remain disabled; only VMM layer admission follows the source.
    // Hook、召回与回写保持关闭；只有 VMM 层准入跟随来源。
    assert!(compaction_request.runtime_policy.vmm_enabled);
    assert!(!compaction_request.runtime_policy.hooks_enabled);
    assert!(!compaction_request.runtime_policy.memory_recall_enabled);
    assert!(!compaction_request.runtime_policy.memory_writeback_enabled);
    assert!(compaction_request.context_injection.is_none());
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

/// Verifies size rejections terminate immediately and transient failures have bounded retries.
/// 验证超限拒绝立即终止，瞬时故障只进行有界重试。
#[test]
fn compaction_retries_only_with_the_same_full_input() {
    assert!(!should_retry_compaction(
        "prompt is too long: 1143843 tokens > 1000000 maximum",
        1
    ));
    assert!(!should_retry_compaction(
        &prompt_compaction_request_budget_error("fixture limit"),
        1
    ));
    assert!(should_retry_compaction("connection reset", 1));
    assert!(should_retry_compaction("connection reset", 2));
    assert!(!should_retry_compaction("connection reset", 3));
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
    let mut state = conversation_state_from_session_messages(&[old_message, tail_message.clone()])
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
