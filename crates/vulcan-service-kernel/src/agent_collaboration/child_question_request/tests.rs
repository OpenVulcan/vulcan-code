use super::*;

mod compaction;
use std::collections::BTreeMap;
use vulcan_agent_runtime::conversation_protocol::{
    SessionConversationMessage, SessionConversationPayload,
};
use vulcan_session_core::v1::child_execution::ChildExecutionPromptContext;
use vulcan_session_core::{ExecutionRuntimePolicy, ProviderModelSnapshot, SessionEffectiveConfig};

/// Verifies an embedded request uses its own locator, captured settings and exact provider history.
/// 验证嵌入请求使用自己的定位器、捕获设置及精确供应商历史。
#[test]
fn child_question_request_preserves_history_without_parent_template() {
    // The isolated root supplies a real opaque locator without touching user data.
    // 隔离根提供真实不透明定位器，不接触用户数据。
    let root_id = Uuid::new_v4();
    let fixture = crate::tests::test_support::TestHostRuntimeRoot::new(root_id);
    let child_id = Uuid::new_v4();
    let locator =
        locate_embedded_task_session_v1(&fixture.v1_storage_locator(), &child_id.to_string())
            .unwrap();
    // Captured values deliberately differ from both global defaults and UI selection.
    // 捕获值刻意区别于全局默认值及界面选择。
    let effective = SessionEffectiveConfig {
        provider_model: ProviderModelSnapshot {
            provider_id: Some("child-provider".into()),
            model_id: Some("child-model".into()),
            reasoning_effort: Some("low".into()),
            execution_mode: Some("build".into()),
        },
        ..SessionEffectiveConfig::default()
    };
    let profile = ChildExecutionProfile {
        version: 1,
        root_session_id: root_id.to_string(),
        parent_session_id: root_id.to_string(),
        task_id: SubagentTaskId::try_new(child_id.to_string()).unwrap(),
        segment_generation: 1,
        business_turn_id: TurnId::try_new(1).unwrap(),
        business_execution_id: "business-execution".into(),
        invocation_config: vulcan_session_core::TaskInvocationConfig::main("Original task"),
        resolved_config: vulcan_session_core::TaskSessionResolvedConfig {
            requested_config: Default::default(),
            effective_config: effective.clone(),
            functional_system_prompt_layers: vec![],
            runtime_policy: ExecutionRuntimePolicy::task_session(),
        },
        runtime_identity: vulcan_session_core::v1::child_execution::ChildExecutionRuntimeIdentity {
            public_task_id: String::from("code-reviewer"),
            display_title: String::from("Review code"),
            description: None,
            subagent: Some(String::from("reviewer")),
            depth: 1,
        },
        effective_config: effective,
        runtime_policy: ExecutionRuntimePolicy::task_session(),
        execution_agent_id: Some("child-agent".into()),
        connection_id: "child-connection".into(),
        temperature: Some(0.25),
        request_overrides: Default::default(),
        admitted_tools: BTreeMap::new(),
        fixed_agent_prompt: Some("Original child prompt".into()),
        prompt_context: ChildExecutionPromptContext {
            identity: "child".into(),
            delegated_task: "Original task".into(),
            delegation: None,
            functional_system_prompt: None,
        },
    };
    // Native assistant metadata and the cursor must pass through without a synthesized root input.
    // 原生助手元数据与游标必须原样传递，不能合成根输入。
    let mut history = SessionConversationState::default();
    history.messages.push(SessionConversationMessage {
        id: "assistant-local".into(),
        payload: SessionConversationPayload::Assistant {
            text: "Business finished".into(),
            reasoning: "Retained reasoning".into(),
            provider_message_id: Some("provider-message".into()),
            provider_message_state: Some(json!({"native":"retained"})),
            usage: None,
            tool_calls: vec![],
        },
    });
    history.continuation.last_provider_message_id = Some("provider-message".into());
    let expected_history = serde_json::to_value(&history).unwrap();
    let request = build_child_question_request(
        child_id,
        "Child".into(),
        &locator,
        &profile,
        1,
        history,
        Default::default(),
    )
    .unwrap();
    assert_eq!(
        serde_json::to_value(&request.conversation_state).unwrap(),
        expected_history
    );
    assert_eq!(
        request.provider_id_override.as_deref(),
        Some("child-provider")
    );
    assert_eq!(
        request.connection_id_override.as_deref(),
        Some("child-connection")
    );
    assert_eq!(request.project_id.as_deref(), Some(locator.project_id()));
    assert_eq!(
        request.session_storage_directory,
        Some(locator.session_directory())
    );
    assert_eq!(
        request.prompt_purpose,
        ExecutionPromptPurpose::QuestionExecution
    );
    assert_eq!(
        request.turn_start_mode,
        Some(ExecutionTurnStartMode::RestoredCheckpoint)
    );
    assert!(request.prompt.is_empty());
    assert!(request.conversation_hook_turn_id.is_none());
    assert!(request.context_injection.is_none());
    assert!(request.system_prompt_hook_blocks.is_empty());
    assert!(request.session_goal_context.is_none());
    assert!(request.session_plan_context.is_none());
    assert!(request.boundary_events.is_empty());
    assert!(
        request
            .captured_child_tool_admission
            .as_ref()
            .unwrap()
            .is_empty()
    );
    assert!(!request.runtime_policy.hooks_enabled);
    assert!(
        build_child_question_request(
            child_id,
            "Child".into(),
            &fixture.v1_storage_locator(),
            &profile,
            1,
            Default::default(),
            Default::default()
        )
        .is_err()
    );
    assert!(
        build_child_question_request(
            child_id,
            "Child".into(),
            &locator,
            &profile,
            2,
            Default::default(),
            Default::default()
        )
        .is_err()
    );
}
