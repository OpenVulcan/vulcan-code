//! Runtime streaming finalization helpers for request snapshots, output events, and terminal reports.
//! 面向请求快照、输出事件与终态报告的运行时流式收尾辅助模块。

use crate::agent_runtime::events::{ExecutionStreamEvent, RuntimeEvent, RuntimeEventKind};
use crate::agent_runtime::execution_models::{ExecutionPreparation, ExecutionRunReport};
use crate::agent_runtime::provider_stream_projection::{
    normalize_assistant_output, normalize_optional_stream_output,
};
use crate::agent_runtime::runtime_stream_delta::RuntimeStreamDeltaState;
use crate::context_budget::build_request_context_budget_snapshot;
use crate::conversation_protocol::SessionConversationState;
use crate::model::SessionId;
use vulcan_core::types::{
    CoreEvent as CoreControllerEvent, ProviderRequestSnapshot as CoreProviderRequestSnapshot,
};

/// Applies the latest request snapshot to the mutable preparation object when one is available.
/// 当存在最新请求快照时，将其应用到可变执行准备对象上。
pub(crate) fn apply_latest_request_snapshot(
    preparation: &mut ExecutionPreparation,
    request_snapshot: Option<&CoreProviderRequestSnapshot>,
) {
    let Some(request_snapshot) = request_snapshot else {
        return;
    };

    let observed_usage = preparation
        .context_budget
        .as_ref()
        .and_then(|budget| budget.observed_usage.as_ref());
    preparation.context_budget = Some(build_request_context_budget_snapshot(
        &preparation.model_blueprint.provider_id,
        &preparation.model_blueprint.model_id,
        preparation.model_blueprint.api_family(),
        &preparation.model_blueprint.capabilities,
        &preparation.model_blueprint.reasoning,
        request_snapshot,
        observed_usage,
    ));
    preparation.provider_request_preview_markdown =
        crate::agent_runtime::provider_request_preview::build_provider_request_preview_markdown(
            preparation,
            request_snapshot,
        );
    preparation.provider_request_snapshot_json = serde_json::to_value(request_snapshot).ok();
}

/// Builds the standard failed runtime event emitted when provider execution terminates with one error.
/// 构建当 Provider 执行以单条错误终止时发出的标准失败运行时事件。
pub(crate) fn build_failed_runtime_event(error: &str) -> RuntimeEvent {
    RuntimeEvent::new(RuntimeEventKind::Failed, "真实执行失败", error.to_string())
}

/// Builds the standard output-completed event emitted after successful provider output normalization.
/// 构建在成功规范化 Provider 输出之后发出的标准输出完成事件。
pub(crate) fn build_output_completed_event(normalized_output: &str) -> RuntimeEvent {
    RuntimeEvent::new(
        RuntimeEventKind::TokenDelta,
        "模型输出完成",
        format!(
            "真实 Provider 已返回 {} 个字符的正式输出内容。",
            normalized_output.chars().count()
        ),
    )
}

/// Builds one failure execution report for a terminated runtime stream.
/// 为已经终止的运行时流构建一份失败执行报告。
pub(crate) fn build_failure_execution_report(
    session_id: SessionId,
    preparation: ExecutionPreparation,
    provider_request_count: usize,
    mcp_discovery_rows: Vec<String>,
    mcp_discovery_errors: Vec<String>,
    warnings: Vec<String>,
    controller_events: Vec<CoreControllerEvent>,
    error: impl Into<String>,
) -> ExecutionRunReport {
    ExecutionRunReport::failure(
        session_id,
        Some(preparation),
        provider_request_count,
        mcp_discovery_rows,
        mcp_discovery_errors,
        warnings,
        controller_events,
        error,
    )
}

/// Builds one success execution report for a completed runtime stream.
/// 为已经完成的运行时流构建一份成功执行报告。
pub(crate) fn build_success_execution_report(
    session_id: SessionId,
    preparation: ExecutionPreparation,
    thinking_output: Option<String>,
    assistant_output: String,
    conversation_state: Option<SessionConversationState>,
    provider_request_count: usize,
    mcp_discovery_rows: Vec<String>,
    mcp_discovery_errors: Vec<String>,
    warnings: Vec<String>,
    controller_events: Vec<CoreControllerEvent>,
) -> ExecutionRunReport {
    ExecutionRunReport::success(
        session_id,
        preparation,
        thinking_output,
        assistant_output,
        conversation_state,
        provider_request_count,
        mcp_discovery_rows,
        mcp_discovery_errors,
        warnings,
        controller_events,
    )
}

/// Builds the complete failure event batch emitted after one runtime stream terminates with a terminal provider error.
/// 构建在运行时流以终态 Provider 错误结束后发出的完整失败事件批次。
pub(crate) fn build_failure_terminal_events(
    session_id: SessionId,
    mut preparation: ExecutionPreparation,
    mcp_discovery_rows: Vec<String>,
    mcp_discovery_errors: Vec<String>,
    warnings: Vec<String>,
    stream_state: &mut RuntimeStreamDeltaState,
    error: String,
) -> Vec<ExecutionStreamEvent> {
    let mut events = Vec::new();
    if let Some(thinking_completed_event) = stream_state.complete_pending_thinking(session_id) {
        events.push(thinking_completed_event);
    }
    apply_latest_request_snapshot(&mut preparation, stream_state.latest_request_snapshot());
    stream_state.controller_mut().mark_failed(error.clone());
    events.push(ExecutionStreamEvent::RuntimeEvent {
        session_id,
        event: build_failed_runtime_event(&error),
    });
    events.push(ExecutionStreamEvent::Finished(
        build_failure_execution_report(
            session_id,
            preparation,
            stream_state.provider_request_count(),
            mcp_discovery_rows,
            mcp_discovery_errors,
            warnings,
            stream_state.controller().events().to_vec(),
            error,
        ),
    ));
    events
}

/// Builds the complete success event batch emitted after one runtime stream finishes without terminal provider errors.
/// 构建在运行时流无终态 Provider 错误完成后发出的完整成功事件批次。
pub(crate) fn build_success_terminal_events(
    session_id: SessionId,
    mut preparation: ExecutionPreparation,
    mcp_discovery_rows: Vec<String>,
    mcp_discovery_errors: Vec<String>,
    warnings: Vec<String>,
    stream_state: &mut RuntimeStreamDeltaState,
) -> Vec<ExecutionStreamEvent> {
    let mut events = Vec::new();
    if let Some(thinking_completed_event) = stream_state.complete_pending_thinking(session_id) {
        events.push(thinking_completed_event);
    }

    apply_latest_request_snapshot(&mut preparation, stream_state.latest_request_snapshot());

    let normalized_output = normalize_assistant_output(stream_state.take_assistant_output());
    events.push(ExecutionStreamEvent::RuntimeEvent {
        session_id,
        event: build_output_completed_event(&normalized_output),
    });
    events.push(ExecutionStreamEvent::Finished(
        build_success_execution_report(
            session_id,
            preparation,
            normalize_optional_stream_output(stream_state.thinking_visible_output().to_string()),
            normalized_output,
            stream_state.take_final_conversation_state(),
            stream_state.provider_request_count(),
            mcp_discovery_rows,
            mcp_discovery_errors,
            warnings,
            stream_state.controller().events().to_vec(),
        ),
    ));
    events
}

#[cfg(test)]
mod tests {
    //! Terminal-batch tests for runtime stream success and failure projection.
    //! 面向运行时流式成功与失败投影的终态批次测试。

    use super::*;
    use crate::agent_runtime::provider::ProviderRuntimeBlueprint;
    use crate::agent_runtime::provider_stream::ProviderStreamDelta;
    use crate::agent_runtime::runtime_stream_delta::{
        RuntimeStreamDeltaState, process_provider_stream_delta,
    };
    use crate::context_budget::ContextBudgetSnapshot;
    use crate::provider_config::ProviderRuntimeKind;
    use serde_json::json;
    use std::collections::BTreeMap;
    use uuid::Uuid;
    use vulcan_core::controller::VulcanCoreController;
    use vulcan_core::types::{
        ControllerConfig, ConversationSnapshot, ProviderContinuation as CoreProviderContinuation,
        ProviderRequestSnapshot as CoreProviderRequestSnapshot, ProviderUsageSnapshot,
    };

    /// Builds one minimal execution preparation used by terminal-batch tests.
    /// 构建一份供终态批次测试使用的最小执行准备对象。
    fn build_test_preparation() -> ExecutionPreparation {
        ExecutionPreparation {
            session_id: Uuid::nil(),
            session_title: String::from("测试会话"),
            agent_id: String::from("default-agent"),
            agent_name: String::from("默认代理"),
            agent_kind: String::from("内置"),
            model_blueprint: ProviderRuntimeBlueprint {
                provider_id: String::from("openai"),
                provider_name: String::from("OpenAI"),
                runtime_kind: ProviderRuntimeKind::OpenAi,
                runtime_label: String::from("OpenAI"),
                model_id: String::from("gpt-test"),
                model_name: String::from("GPT Test"),
                openrouter_endpoint_route: None,
                base_url: Some(String::from("https://api.example.test/v1")),
                supports_remote_model_refresh: true,
                credential_source: String::from("环境变量"),
                user_agent: None,
                extra_headers: BTreeMap::new(),
                extra_body: json!({}),
                reasoning: Default::default(),
                capabilities: Default::default(),
                interleaved_reasoning: None,
            },
            prompt_preview: String::from("请分析代码"),
            workspace_path: String::from("D:/workspace"),
            conversation_attachments: Vec::new(),
            context_history_count: 2,
            execution_mode: String::from("Build"),
            temperature: Some(0.2),
            skill_names: Vec::new(),
            tool_descriptors: Vec::new(),
            delegate_agents: Vec::new(),
            enabled_mcp_servers: Vec::new(),
            warnings: Vec::new(),
            provider_request_preview_markdown: String::new(),
            provider_request_snapshot_json: None,
            context_budget: None,
        }
    }

    /// Builds one minimal runtime-stream delta state used by terminal-batch tests.
    /// 构建一份供终态批次测试使用的最小运行时流式增量状态。
    fn build_stream_state() -> RuntimeStreamDeltaState {
        RuntimeStreamDeltaState::new(VulcanCoreController::new(
            ControllerConfig {
                provider_id: String::from("openai"),
                model_id: String::from("gpt-test"),
                system_prompt: String::from("system"),
                temperature: Some(0.2),
                max_turns: 4,
                additional_params: None,
            },
            ConversationSnapshot::default(),
        ))
    }

    /// Builds one minimal structured request snapshot used to verify final preview backfill.
    /// 构建一份最小结构化请求快照，用于验证最终预览回填行为。
    fn build_request_snapshot() -> CoreProviderRequestSnapshot {
        CoreProviderRequestSnapshot {
            provider_id: String::from("openai"),
            model_id: String::from("gpt-test"),
            system_prompt: String::from("system prompt"),
            prompt: String::from("effective prompt"),
            history: Vec::new(),
            temperature: Some(0.2),
            max_turns: 4,
            tool_schemas: Vec::new(),
            additional_params: None,
            request_headers: None,
            continuation: CoreProviderContinuation {
                provider_id: Some(String::from("openai")),
                model_id: Some(String::from("gpt-test")),
                last_provider_message_id: Some(String::from("resp-prev")),
                updated_at: Some(String::from("100")),
            },
        }
    }

    /// Emits success terminal events in the expected order and preserves the final report payload.
    /// 按预期顺序发出成功终态事件，并保留最终报告载荷。
    #[test]
    fn success_terminal_events_emit_output_completed_and_finished_report() {
        let session_id = Uuid::nil();
        let mut stream_state = build_stream_state();

        let _ = process_provider_stream_delta(
            session_id,
            &mut stream_state,
            Ok(ProviderStreamDelta::RequestSnapshot(
                build_request_snapshot(),
            )),
        );
        let _ = process_provider_stream_delta(
            session_id,
            &mut stream_state,
            Ok(ProviderStreamDelta::Thinking(String::from("先整理上下文"))),
        );
        let _ = process_provider_stream_delta(
            session_id,
            &mut stream_state,
            Ok(ProviderStreamDelta::Output(String::from("最终答案"))),
        );
        let _ = process_provider_stream_delta(
            session_id,
            &mut stream_state,
            Ok(ProviderStreamDelta::FinalConversationState(
                SessionConversationState::default(),
            )),
        );

        let events = build_success_terminal_events(
            session_id,
            build_test_preparation(),
            vec![String::from("mcp-row")],
            Vec::new(),
            vec![String::from("warning")],
            &mut stream_state,
        );

        assert!(matches!(
            events.as_slice(),
            [
                ExecutionStreamEvent::RuntimeEvent { event, .. },
                ExecutionStreamEvent::Finished(report),
            ] if event.title == "模型输出完成"
                && report.is_success()
                && report.assistant_output.as_deref() == Some("最终答案")
                && report.conversation_state.is_some()
                && report.mcp_discovery_rows == vec![String::from("mcp-row")]
                && report.warnings == vec![String::from("warning")]
        ));
    }

    /// Emits failure terminal events in the expected order and captures the terminal error inside the final report.
    /// 按预期顺序发出失败终态事件，并在最终报告中捕获终态错误。
    #[test]
    fn failure_terminal_events_emit_failed_runtime_event_and_finished_report() {
        let session_id = Uuid::nil();
        let mut stream_state = build_stream_state();

        let _ = process_provider_stream_delta(
            session_id,
            &mut stream_state,
            Ok(ProviderStreamDelta::RequestSnapshot(
                build_request_snapshot(),
            )),
        );
        let _ = process_provider_stream_delta(
            session_id,
            &mut stream_state,
            Ok(ProviderStreamDelta::Thinking(String::from("先尝试执行"))),
        );

        let events = build_failure_terminal_events(
            session_id,
            build_test_preparation(),
            Vec::new(),
            vec![String::from("mcp-error")],
            vec![String::from("warning")],
            &mut stream_state,
            String::from("provider failed"),
        );

        assert!(matches!(
            events.as_slice(),
            [
                ExecutionStreamEvent::ThinkingCompleted { .. },
                ExecutionStreamEvent::RuntimeEvent { event, .. },
                ExecutionStreamEvent::Finished(report),
            ] if event.kind == RuntimeEventKind::Failed
                && report.error.as_deref() == Some("provider failed")
                && report.mcp_discovery_errors == vec![String::from("mcp-error")]
                && report.warnings == vec![String::from("warning")]
        ));
    }

    /// Rebuilds the latest request snapshot while preserving the previously observed usage floor for recalibration.
    /// 在重建最新请求快照时保留先前观测到的用量下限，以便继续参与预算校准。
    #[test]
    fn apply_latest_request_snapshot_preserves_observed_usage_floor() {
        let observed_usage = ProviderUsageSnapshot {
            input_tokens: Some(4096),
            output_tokens: Some(256),
            reasoning_tokens: Some(128),
            total_tokens: Some(4480),
            cached_input_tokens: Some(2048),
            cache_creation_input_tokens: None,
        };
        let mut preparation = build_test_preparation();
        preparation.model_blueprint.capabilities.input_limit = Some(8_000);
        preparation.model_blueprint.capabilities.output_limit = Some(1_024);
        preparation.context_budget = Some(ContextBudgetSnapshot {
            observed_usage: Some(observed_usage.clone()),
            ..ContextBudgetSnapshot::default()
        });

        apply_latest_request_snapshot(&mut preparation, Some(&build_request_snapshot()));

        let context_budget = preparation
            .context_budget
            .expect("latest request snapshot should rebuild the budget");
        assert_eq!(context_budget.observed_usage, Some(observed_usage));
        assert_eq!(context_budget.estimated_input_tokens, 4096);
    }
}
