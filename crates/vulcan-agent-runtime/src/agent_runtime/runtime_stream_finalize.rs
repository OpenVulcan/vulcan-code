//! Runtime streaming finalization helpers for request snapshots, output events, and terminal reports.
//! 面向请求快照、输出事件与终态报告的运行时流式收尾辅助模块。

use crate::agent_runtime::context_request_mode::resolve_preparation_context_request_mode;
use crate::agent_runtime::events::{ExecutionStreamEvent, RuntimeEvent, RuntimeEventKind};
use crate::agent_runtime::execution_models::{ExecutionPreparation, ExecutionRunReport};
use crate::agent_runtime::provider_stream_projection::{
    normalize_assistant_output, normalize_optional_stream_output,
};
use crate::agent_runtime::runtime_stream_delta::RuntimeStreamDeltaState;
use crate::context_budget::{
    RequestContextCompactionBudgetSnapshotInput, build_request_context_compaction_budget_snapshot,
};
use crate::conversation_protocol::SessionConversationState;
use crate::model::SessionId;
use vulcan_context::{ContextBaseline, build_provider_request_context_assembly};
use vulcan_core::types::{
    CoreEvent as CoreControllerEvent, ProviderRequestSnapshot as CoreProviderRequestSnapshot,
    ProviderUsageSnapshot as CoreProviderUsageSnapshot,
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
    preparation.context_budget = Some(build_request_context_compaction_budget_snapshot(
        RequestContextCompactionBudgetSnapshotInput {
            provider_id: &preparation.model_blueprint.provider_id,
            model_id: &preparation.model_blueprint.model_id,
            protocol_profile_id: &preparation.model_blueprint.protocol_profile_id,
            capabilities: &preparation.model_blueprint.public_capabilities,
            reasoning: &preparation.model_blueprint.reasoning,
            request_snapshot,
            compaction_policy: preparation.context_compaction_policy,
            context_budget_override: preparation.context_budget_override,
            observed_usage,
            accounted_input_tokens: observed_usage
                .and_then(|usage| vulcan_context::observed_input_tokens_from_usage(Some(usage))),
            accounting_source: if observed_usage.is_some() {
                crate::context_budget::ContextBudgetAccountingSource::ProviderReported
            } else {
                crate::context_budget::ContextBudgetAccountingSource::CharacterEstimate
            },
            accounting_warning_code: observed_usage
                .is_none()
                .then_some("provider_usage_baseline_unavailable"),
            provider_anchor_call_id: None,
            prefill_input_tokens: preparation
                .context_budget
                .as_ref()
                .and_then(|budget| budget.auto_compact_prefill_input_tokens),
        },
    ));
    let context_assembly = build_provider_request_context_assembly(request_snapshot);
    preparation.context_baseline = Some(ContextBaseline::from_assembly(&context_assembly));
    preparation.context_assembly = Some(context_assembly);
    preparation.context_request_mode = Some(resolve_preparation_context_request_mode(
        preparation,
        request_snapshot,
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
    last_successful_provider_usage: Option<CoreProviderUsageSnapshot>,
    cumulative_successful_provider_usage: Option<CoreProviderUsageSnapshot>,
    mcp_discovery_rows: Vec<String>,
    mcp_discovery_errors: Vec<String>,
    warnings: Vec<String>,
    controller_events: Vec<CoreControllerEvent>,
    conversation_state: Option<SessionConversationState>,
    error: impl Into<String>,
) -> ExecutionRunReport {
    ExecutionRunReport::failure(
        session_id,
        Some(preparation),
        provider_request_count,
        last_successful_provider_usage,
        cumulative_successful_provider_usage,
        mcp_discovery_rows,
        mcp_discovery_errors,
        warnings,
        controller_events,
        conversation_state,
        error,
    )
}

/// Builds one success execution report for a completed runtime stream.
/// 为已经完成的运行时流构建一份成功执行报告。
pub(crate) fn build_success_execution_report(
    session_id: SessionId,
    preparation: ExecutionPreparation,
    thinking_output: Option<String>,
    assistant_output: Option<String>,
    conversation_state: Option<SessionConversationState>,
    provider_request_count: usize,
    mcp_discovery_rows: Vec<String>,
    mcp_discovery_errors: Vec<String>,
    warnings: Vec<String>,
    controller_events: Vec<CoreControllerEvent>,
    last_successful_provider_usage: Option<CoreProviderUsageSnapshot>,
    cumulative_successful_provider_usage: Option<CoreProviderUsageSnapshot>,
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
        last_successful_provider_usage,
        cumulative_successful_provider_usage,
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
    let last_safe_conversation_state = stream_state.take_final_conversation_state();
    let last_successful_provider_usage = stream_state.take_last_successful_provider_usage();
    let cumulative_successful_provider_usage =
        stream_state.take_cumulative_successful_provider_usage();
    let provider_failure_category = stream_state.take_terminal_provider_failure_category();
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
            last_successful_provider_usage,
            cumulative_successful_provider_usage,
            mcp_discovery_rows,
            mcp_discovery_errors,
            warnings,
            stream_state.controller().events().to_vec(),
            last_safe_conversation_state,
            error,
        )
        .with_provider_failure_category(provider_failure_category),
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

    // visibleOutput preserves a genuinely absent assistant message when the run only requested tools.
    // visibleOutput 在本次运行仅请求工具时保留助手消息确实缺失的事实。
    let raw_output = stream_state.take_assistant_output();
    let visible_output = if raw_output.trim().is_empty() && stream_state.observed_tool_request() {
        None
    } else {
        Some(normalize_assistant_output(raw_output))
    };
    let last_successful_provider_usage = stream_state.take_last_successful_provider_usage();
    let cumulative_successful_provider_usage =
        stream_state.take_cumulative_successful_provider_usage();
    if let Some(visible_output) = visible_output.as_deref() {
        events.push(ExecutionStreamEvent::RuntimeEvent {
            session_id,
            event: build_output_completed_event(visible_output),
        });
    }
    events.push(ExecutionStreamEvent::Finished(
        build_success_execution_report(
            session_id,
            preparation,
            normalize_optional_stream_output(stream_state.thinking_visible_output().to_string()),
            visible_output,
            stream_state.take_final_conversation_state(),
            stream_state.provider_request_count(),
            mcp_discovery_rows,
            mcp_discovery_errors,
            warnings,
            stream_state.controller().events().to_vec(),
            last_successful_provider_usage,
            cumulative_successful_provider_usage,
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
    use crate::agent_runtime::provider_request_telemetry::ProviderRequestTelemetryContext;
    use crate::agent_runtime::provider_stream::ProviderStreamDelta;
    use crate::agent_runtime::runtime_stream_delta::{
        RuntimeStreamDeltaState, process_provider_stream_delta, test_start_durable_request,
    };
    use crate::agent_runtime::tool_request_stream::ToolRequestFragment;
    use crate::context_budget::ContextBudgetSnapshot;
    use uuid::Uuid;
    use vulcan_ai_contract::AiErrorCategory;
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
            session_storage_directory: None,
            conversation_hook_turn_id: None,
            continuation_action_id: None,
            session_title: String::from("测试会话"),
            agent_id: String::from("default-agent"),
            agent_name: String::from("默认代理"),
            agent_kind: String::from("内置"),
            agent_prompt_snapshot: None,
            subagent_prompt_context: None,
            model_blueprint: ProviderRuntimeBlueprint {
                provider_id: String::from("openai"),
                service_entry_id: String::from("public-api"),
                connection_id: String::from("test-connection"),
                provider_name: String::from("OpenAI"),
                runtime_label: String::from("OpenAI"),
                model_id: String::from("gpt-test"),
                model_name: String::from("GPT Test"),
                protocol_profile_id: String::from(vulcan_llm_core::OPENAI_CHAT_PROFILE_ID),
                manifest_revision: 0,
                frozen_selection: None,
                route_revision: String::from("sha256:test-route"),
                secondary_route: Default::default(),
                public_capabilities: vulcan_ai_contract::ModelCapabilities::default(),
                base_url: Some(String::from("https://api.example.test/v1")),
                supports_remote_model_refresh: true,
                credential_source: String::from("环境变量"),
                reasoning: Default::default(),
            },
            prompt_preview: String::from("请分析代码"),
            workspace_path: String::from(r"D:\workspace"),
            conversation_attachments: Vec::new(),
            context_history_count: 2,
            execution_mode: String::from("Build"),
            temperature: Some(0.2),
            prompt_configuration: crate::provider_config::PersistedPromptConfiguration::default(),
            output_style_snapshot: None,
            prompt_purpose: crate::agent_runtime::ExecutionPromptPurpose::UserTurn,
            prompt_skills: Vec::new(),
            prompt_skill_user_root: None,
            additional_developer_segments: Vec::new(),
            skill_names: Vec::new(),
            tool_descriptors: Vec::new(),
            tool_request_rejection_reason: None,
            tool_request_rejection_all_tools: false,
            tool_request_rejection_tool_ids: Vec::new(),
            delegate_agents: Vec::new(),
            enabled_mcp_servers: Vec::new(),
            warnings: Vec::new(),
            provider_request_preview_markdown: String::new(),
            provider_request_snapshot_json: None,
            context_budget: None,
            context_compaction_policy: None,
            context_budget_override: None,
            context_assembly: None,
            context_baseline: None,
            context_request_mode: None,
            context_injection: None,
            system_prompt_hook_blocks: Vec::new(),
            project_vmm: Default::default(),
            session_vmm: Default::default(),
            vmm_runtime_ready: false,
            boundary_events: Vec::new(),
            runtime_system_updates: Vec::new(),
            session_goal_context: None,
            session_plan_context: None,
            embedded_turn_usage_segments: Vec::new(),
        }
    }

    /// Builds one minimal runtime-stream delta state used by terminal-batch tests.
    /// 构建一份供终态批次测试使用的最小运行时流式增量状态。
    fn build_stream_state() -> RuntimeStreamDeltaState {
        RuntimeStreamDeltaState::new(
            VulcanCoreController::new(
                ControllerConfig {
                    provider_id: String::from("openai"),
                    model_id: String::from("gpt-test"),
                    system_prompt: String::from("system"),
                    prompt_layers: vulcan_core::types::ProviderPromptLayers::from_system_prompt(
                        String::from("system"),
                    ),
                    temperature: Some(0.2),
                    max_turns: 4,
                    additional_params: None,
                    context_injection: None,
                },
                ConversationSnapshot::default(),
            )
            .expect("valid core conversation snapshot"),
        )
        .with_provider_request_telemetry_context(ProviderRequestTelemetryContext::for_test())
    }

    /// Builds one minimal structured request snapshot used to verify final preview backfill.
    /// 构建一份最小结构化请求快照，用于验证最终预览回填行为。
    fn build_request_snapshot() -> CoreProviderRequestSnapshot {
        CoreProviderRequestSnapshot {
            provider_id: String::from("openai"),
            model_id: String::from("gpt-test"),
            system_prompt: String::from("system prompt"),
            prompt_layers: vulcan_core::types::ProviderPromptLayers::from_system_prompt(
                String::from("system prompt"),
            ),
            prompt: String::from("effective prompt"),
            prompt_in_history: false,
            history: Vec::new(),
            temperature: Some(0.2),
            max_turns: 4,
            tool_schemas: Vec::new(),
            additional_params: None,
            request_headers: None,
            continuation: CoreProviderContinuation {
                provider_id: Some(String::from("openai")),
                protocol_profile_id: Some(String::from("openai-responses")),
                model_id: Some(String::from("gpt-test")),
                last_provider_message_id: Some(String::from("resp-prev")),
                last_local_message_id: Some(String::from("assistant-1")),
                context_cache_epoch_id: None,
                context_anchor_revision: None,
                context_rewrite_fingerprint: None,
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

        let _ = test_start_durable_request(session_id, &mut stream_state, build_request_snapshot());
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
                Some(ProviderUsageSnapshot {
                    input_tokens: Some(100),
                    uncached_input_tokens: None,
                    output_tokens: Some(20),
                    reasoning_tokens: None,
                    total_tokens: Some(120),
                    cached_input_tokens: None,
                    cache_creation_input_tokens: None,
                    ..ProviderUsageSnapshot::default()
                }),
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
                && report
                    .last_successful_provider_usage
                    .as_ref()
                    .and_then(|usage| usage.total_tokens) == Some(120)
                && report.mcp_discovery_rows == vec![String::from("mcp-row")]
                && report.warnings == vec![String::from("warning")]
        ));
    }

    /// Preserves tool-only completion without inventing an assistant placeholder message.
    /// 在仅有工具请求的完成场景中不伪造助手占位消息。
    #[test]
    fn tool_only_success_omits_assistant_output_event() {
        let session_id = Uuid::nil();
        let mut stream_state = build_stream_state();
        let _ = process_provider_stream_delta(
            session_id,
            &mut stream_state,
            Ok(ProviderStreamDelta::ToolRequestFragment(
                ToolRequestFragment::Detected {
                    request_stream_id: String::from("request-1"),
                    tool_call_id: String::from("call-1"),
                    tool_name: String::from("shell-command"),
                    initial_arguments: String::new(),
                    argument_assembler: None,
                },
            )),
        );

        let events = build_success_terminal_events(
            session_id,
            build_test_preparation(),
            Vec::new(),
            Vec::new(),
            Vec::new(),
            &mut stream_state,
        );

        assert!(matches!(
            events.as_slice(),
            [ExecutionStreamEvent::Finished(report)]
                if report.is_success() && report.assistant_output.is_none()
        ));
    }

    /// Emits failure terminal events while preserving the last safe conversation state before the error.
    /// 发出失败终态事件，同时保留错误前最后一个安全会话状态。
    #[test]
    fn failure_terminal_events_emit_failed_runtime_event_and_finished_report() {
        let session_id = Uuid::nil();
        let mut stream_state = build_stream_state();

        let _ = test_start_durable_request(session_id, &mut stream_state, build_request_snapshot());
        let _ = process_provider_stream_delta(
            session_id,
            &mut stream_state,
            Ok(ProviderStreamDelta::Thinking(String::from("先尝试执行"))),
        );
        let _ = process_provider_stream_delta(
            session_id,
            &mut stream_state,
            Ok(ProviderStreamDelta::FinalConversationState(
                SessionConversationState::default(),
                None,
            )),
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
                && report.conversation_state.is_some()
                && report.mcp_discovery_errors == vec![String::from("mcp-error")]
                && report.warnings == vec![String::from("warning")]
        ));
    }

    /// The failure report carries the category of the request that ended the run, and only that.
    /// 失败报告携带结束本次运行的那次请求的类别，且仅携带该类别。
    #[test]
    fn failure_report_carries_only_the_ending_request_category() {
        let session_id = Uuid::nil();
        let start_request = |stream_state: &mut RuntimeStreamDeltaState| {
            let _ = test_start_durable_request(session_id, stream_state, build_request_snapshot());
        };
        let fail_request = |stream_state: &mut RuntimeStreamDeltaState, will_retry, category| {
            let (completion, _receiver) =
                crate::agent_runtime::events::provider_request_durability_barrier();
            let _ = process_provider_stream_delta(
                session_id,
                stream_state,
                Ok(ProviderStreamDelta::RequestFailedDurable {
                    will_retry,
                    retry_number: 0,
                    max_retries: 0,
                    delay_ms: 0,
                    error_category: String::from("provider_error"),
                    error: Some(Box::new(test_execution_error(category))),
                    completion,
                }),
            );
        };
        let finished_category = |stream_state: &mut RuntimeStreamDeltaState| {
            build_failure_terminal_events(
                session_id,
                build_test_preparation(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
                stream_state,
                String::from("provider failed"),
            )
            .into_iter()
            .find_map(|event| match event {
                ExecutionStreamEvent::Finished(report) => Some(report.provider_failure_category),
                _ => None,
            })
            .expect("a failure batch always finishes with a report")
        };

        // A retried failure does not end the run; the final non-retried one does.
        // 被重试的失败不会结束运行；最后一次不再重试的失败才会。
        let mut ended_by_provider = build_stream_state();
        start_request(&mut ended_by_provider);
        fail_request(
            &mut ended_by_provider,
            true,
            AiErrorCategory::ProviderOverloaded,
        );
        start_request(&mut ended_by_provider);
        fail_request(&mut ended_by_provider, false, AiErrorCategory::RateLimited);
        assert_eq!(
            finished_category(&mut ended_by_provider),
            Some(AiErrorCategory::RateLimited)
        );

        // A later request supersedes an earlier failure, so a non-Provider ending has no category.
        // 后续请求会取代先前失败，因此非 Provider 原因的结束不带类别。
        let mut ended_elsewhere = build_stream_state();
        start_request(&mut ended_elsewhere);
        fail_request(
            &mut ended_elsewhere,
            false,
            AiErrorCategory::ContextLengthExceeded,
        );
        start_request(&mut ended_elsewhere);
        assert_eq!(finished_category(&mut ended_elsewhere), None);
    }

    /// Builds one structured Provider failure with the given category.
    /// 以给定类别构建一条结构化 Provider 失败。
    fn test_execution_error(category: AiErrorCategory) -> vulcan_ai_contract::AiExecutionError {
        vulcan_ai_contract::AiExecutionError {
            category,
            subcategory: None,
            stage: String::from("response_headers"),
            http_status: None,
            transport: None,
            websocket_close_code: None,
            upstream_code: None,
            upstream_type: None,
            upstream_parameter: None,
            upstream_message: None,
            request_id: None,
            provider_id: None,
            entry_id: None,
            model_id: None,
            message: String::from("Provider request failed."),
            dispatch_outcome: vulcan_ai_contract::DispatchOutcome::Dispatched,
            semantic_output_observed: false,
            side_effect_observed: false,
            terminal: true,
            retryable: false,
            retry_after: None,
        }
    }

    /// Rebuilds the latest request snapshot while preserving previously observed usage for recalibration.
    /// 在重建最新请求快照时保留先前观测到的用量，以便继续参与预算校准。
    #[test]
    fn apply_latest_request_snapshot_preserves_observed_usage() {
        let observed_usage = ProviderUsageSnapshot {
            input_tokens: Some(4096),
            uncached_input_tokens: None,
            output_tokens: Some(256),
            reasoning_tokens: Some(128),
            total_tokens: Some(4480),
            cached_input_tokens: Some(2048),
            cache_creation_input_tokens: None,
            ..ProviderUsageSnapshot::default()
        };
        let mut preparation = build_test_preparation();
        preparation
            .model_blueprint
            .public_capabilities
            .context_limit = Some(8_000);
        preparation.model_blueprint.public_capabilities.output_limit = Some(1_024);
        preparation.context_budget = Some(ContextBudgetSnapshot {
            baseline_matches_active_runtime: true,
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
