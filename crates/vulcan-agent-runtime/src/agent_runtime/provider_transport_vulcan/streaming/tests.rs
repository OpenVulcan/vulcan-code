//! Regression tests for the streaming boundary.
//! streaming边界的回归测试。

use super::*;

/// Verifies explicit response completion terminates the adapter without requiring transport EOF.
/// 验证显式响应完成会终止适配器，而不要求传输层先到达 EOF。
#[test]
fn response_completion_is_a_protocol_terminal_boundary() {
    assert!(public_stream_event_is_terminal(
        &AiStreamEvent::ResponseCompleted
    ));
    assert!(public_stream_event_is_terminal(
        &AiStreamEvent::ResponseCancelled
    ));
    assert!(!public_stream_event_is_terminal(
        &AiStreamEvent::ResponseStarted {
            execution_id: String::from("response-1"),
        }
    ));
}

/// Verifies reconciliation classification is confined to the exact AgentSpace conversation route.
/// 验证待对账分类仅限于精确 AgentSpace 对话路由。
#[test]
fn reconciliation_required_is_classified_only_for_agentspace() {
    // agentspaceError mirrors the normalized public stream failure before route context is attached.
    // agentspaceError 模拟附加路由上下文前的规范公共流失败。
    let mut agentspace_error = vulcan_ai_contract::AiExecutionError {
        category: vulcan_ai_contract::AiErrorCategory::Unclassified,
        subcategory: Some(String::from("invocation.usage_unavailable")),
        stage: String::from("stream"),
        http_status: None,
        transport: Some(vulcan_ai_contract::AiTransportKind::Http),
        websocket_close_code: None,
        upstream_code: Some(String::from("reconciliation_required")),
        upstream_type: Some(String::from("invocation.usage_unavailable")),
        upstream_parameter: None,
        upstream_message: Some(String::from("Result requires reconciliation.")),
        request_id: Some(String::from("request-1")),
        provider_id: None,
        entry_id: None,
        model_id: None,
        message: String::from("Result requires reconciliation."),
        dispatch_outcome: vulcan_ai_contract::DispatchOutcome::Dispatched,
        semantic_output_observed: false,
        side_effect_observed: false,
        terminal: true,
        retryable: false,
        retry_after: None,
    };
    attach_execution_route_context(
        &mut agentspace_error,
        "agentspace",
        "chatgpt-plan",
        "gpt-6-astra",
        vulcan_provider_adapters::ResponsesRecoveryPolicy::AgentSpaceResultAndUsageReconciliationV1,
    );
    assert_eq!(
        agentspace_error.confirmation_state(),
        Some(vulcan_ai_contract::AiExecutionConfirmationState::ResultAndUsagePending)
    );
    assert!(!agentspace_error.retryable);

    // otherProviderError proves an identical upstream code cannot activate the special state elsewhere.
    // otherProviderError 证明相同上游错误码不会在其他供应商激活专属状态。
    let mut other_provider_error = agentspace_error.clone();
    other_provider_error.subcategory = Some(String::from("invocation.usage_unavailable"));
    attach_execution_route_context(
        &mut other_provider_error,
        "openai",
        "api",
        "gpt-6-astra",
        vulcan_provider_adapters::ResponsesRecoveryPolicy::AgentSpaceResultAndUsageReconciliationV1,
    );
    assert_eq!(other_provider_error.confirmation_state(), None);

    // undeclaredPolicyError proves provider identity alone cannot authorize recovery semantics.
    // undeclaredPolicyError 证明仅凭供应商身份不能授权恢复语义。
    let mut undeclared_policy_error = agentspace_error.clone();
    undeclared_policy_error.subcategory = Some(String::from("invocation.usage_unavailable"));
    attach_execution_route_context(
        &mut undeclared_policy_error,
        "agentspace",
        "chatgpt-plan",
        "gpt-6-astra",
        vulcan_provider_adapters::ResponsesRecoveryPolicy::None,
    );
    assert_eq!(undeclared_policy_error.confirmation_state(), None);
}
