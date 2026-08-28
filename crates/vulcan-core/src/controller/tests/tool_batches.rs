use super::*;

/// Verifies one provider tool-call batch executes concurrently while preserving ordered tool-result replay.
/// 验证单个 Provider 工具调用批次会并发执行，同时保留有序工具结果回放。
#[test]
fn run_user_turn_streaming_executes_tool_batch_concurrently() {
    let runtime = tokio::runtime::Runtime::new().expect("create tokio runtime");
    runtime.block_on(async {
        let adapter = MockStreamingProviderAdapter::new(vec![
            (
                Vec::new(),
                ProviderTurnResponse {
                    assistant_text: String::new(),
                    assistant_reasoning: String::new(),
                    provider_message_id: Some(String::from("resp-batch-concurrent")),
                    provider_message_state: None,
                    tool_calls: vec![
                        ConversationToolCall {
                            id: String::from("tool-item-a"),
                            tool_call_id: Some(String::from("call-a")),
                            tool_name: String::from("slow_a"),
                            arguments: json!({"step": "a"}),
                            argument_parse_error: None,
                        },
                        ConversationToolCall {
                            id: String::from("tool-item-b"),
                            tool_call_id: Some(String::from("call-b")),
                            tool_name: String::from("fast_b"),
                            arguments: json!({"step": "b"}),
                            argument_parse_error: None,
                        },
                    ],
                    continuation: None,
                    usage: None,
                },
            ),
            (
                Vec::new(),
                ProviderTurnResponse {
                    assistant_text: String::from("两个工具已完成。"),
                    assistant_reasoning: String::new(),
                    provider_message_id: Some(String::from("resp-final")),
                    provider_message_state: None,
                    tool_calls: Vec::new(),
                    continuation: None,
                    usage: None,
                },
            ),
        ]);
        let executor = ConcurrentToolExecutor::default();
        let mut controller =
            VulcanCoreController::new(build_test_config(4), ConversationSnapshot::default())
                .expect("valid core conversation snapshot");

        let result = controller
            .run_user_turn_streaming(&adapter, &executor, &AllowAllToolPolicy, "并发调用两个工具")
            .await
            .expect("run concurrent tool batch");

        assert_eq!(result.tool_calls_processed, 2);
        assert_eq!(executor.max_running.load(Ordering::SeqCst), 2);
        let completed_tool_call_ids = controller
            .snapshot()
            .messages
            .iter()
            .filter_map(|message| match &message.payload {
                ConversationPayload::ToolResult { tool_call_id, .. } => tool_call_id.as_deref(),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(completed_tool_call_ids, vec!["call-a", "call-b"]);
        let finished_event_tool_call_ids = controller
            .events()
            .iter()
            .filter(|event| matches!(&event.kind, CoreEventKind::ToolFinished))
            .filter_map(|event| {
                event
                    .metadata
                    .as_ref()
                    .and_then(|metadata| metadata.get("tool_call_id"))
                    .and_then(Value::as_str)
            })
            .collect::<Vec<_>>();
        assert_eq!(finished_event_tool_call_ids, vec!["call-b", "call-a"]);
    });
}

/// Verifies Provider-invalid JSON arguments return one tool error without dispatching the host tool body.
/// 验证 Provider 无效 JSON 参数会返回一条工具错误，且不会分派宿主工具主体。
#[test]
fn malformed_provider_arguments_return_tool_error_without_executor_dispatch() {
    let runtime = tokio::runtime::Runtime::new().expect("create tokio runtime");
    runtime.block_on(async {
        // decoderMessage is the exact controlled protocol diagnostic attached before Core execution.
        // decoderMessage 是 Core 执行前附加的精确受控协议诊断。
        let decoder_message = "OpenAI Responses tool input ended before valid JSON was complete";
        let adapter = MockStreamingProviderAdapter::new(vec![
            (
                Vec::new(),
                ProviderTurnResponse {
                    assistant_text: String::new(),
                    assistant_reasoning: String::new(),
                    provider_message_id: Some(String::from("resp-invalid-tool")),
                    provider_message_state: None,
                    tool_calls: vec![ConversationToolCall {
                        id: String::from("tool-item-invalid"),
                        tool_call_id: Some(String::from("call-invalid")),
                        tool_name: String::from("file_edit"),
                        arguments: Value::Null,
                        argument_parse_error: Some(decoder_message.to_string()),
                    }],
                    continuation: None,
                    usage: None,
                },
            ),
            (
                Vec::new(),
                ProviderTurnResponse {
                    assistant_text: String::from("I will correct the edit arguments."),
                    assistant_reasoning: String::new(),
                    provider_message_id: Some(String::from("resp-after-invalid-tool")),
                    provider_message_state: None,
                    tool_calls: Vec::new(),
                    continuation: None,
                    usage: None,
                },
            ),
        ]);
        let executor = MockToolExecutor::new();
        let mut controller =
            VulcanCoreController::new(build_test_config(4), ConversationSnapshot::default())
                .expect("valid core conversation snapshot");

        let result = controller
            .run_user_turn_streaming(
                &adapter,
                &executor,
                &AllowAllToolPolicy,
                "Apply the requested edit.",
            )
            .await
            .expect("invalid Provider arguments should remain inside the tool loop");

        assert_eq!(result.provider_turns_executed, 2);
        assert_eq!(result.tool_calls_processed, 1);
        assert!(executor.executed_requests().is_empty());
        // builtRequests proves the next Provider turn receives the tool error instead of a transparent replay of the original request.
        // builtRequests 证明下一次 Provider 回合收到的是工具错误，而不是对原始请求进行透明重放。
        let built_requests = adapter.built_requests();
        assert_eq!(built_requests.len(), 2);
        assert!(built_requests[1].history.iter().any(|message| {
            matches!(
                &message.payload,
                ConversationPayload::ToolResult {
                    tool_call_id,
                    is_error: true,
                    text,
                } if tool_call_id.as_deref() == Some("call-invalid")
                    && text.contains("was not executed")
                    && text.contains(decoder_message)
            )
        }));
        let prepared_batches = executor.prepared_batches();
        assert_eq!(prepared_batches.len(), 1);
        assert_eq!(prepared_batches[0].len(), 1);
        assert_eq!(
            prepared_batches[0][0].argument_parse_error.as_deref(),
            Some(decoder_message)
        );
        let rejected_result = controller
            .snapshot()
            .messages
            .iter()
            .find_map(|message| match &message.payload {
                ConversationPayload::ToolResult {
                    tool_call_id,
                    is_error,
                    text,
                } if tool_call_id.as_deref() == Some("call-invalid") => {
                    Some((*is_error, text.as_str()))
                }
                _ => None,
            })
            .expect("rejected tool call should have one model-visible result");
        assert!(rejected_result.0);
        assert!(rejected_result.1.contains("was not executed"));
        assert!(rejected_result.1.contains(decoder_message));
        assert!(controller.events().iter().all(|event| {
            !matches!(event.kind, CoreEventKind::ToolStarted)
                || event
                    .metadata
                    .as_ref()
                    .and_then(|metadata| metadata.get("tool_call_id"))
                    .and_then(Value::as_str)
                    != Some("call-invalid")
        }));
        assert!(controller.events().iter().any(|event| {
            matches!(event.kind, CoreEventKind::ToolFinished)
                && event
                    .metadata
                    .as_ref()
                    .and_then(|metadata| metadata.get("tool_call_id"))
                    .and_then(Value::as_str)
                    == Some("call-invalid")
        }));
    });
}

/// Verifies a second malformed tool call ends the user turn instead of opening another correction round.
/// 验证第二次格式错误的工具调用会结束用户回合，而不是再次开启纠错轮。
#[test]
fn malformed_provider_arguments_allow_only_one_model_correction_round() {
    let runtime = tokio::runtime::Runtime::new().expect("create tokio runtime");
    runtime.block_on(async {
        // malformedResponse builds one authoritative non-executable tool call for a physical Provider turn.
        // malformedResponse 为一次真实 Provider 回合构建一条权威且不可执行的工具调用。
        let malformed_response = |suffix: &str| ProviderTurnResponse {
            assistant_text: String::new(),
            assistant_reasoning: String::new(),
            provider_message_id: Some(format!("response-invalid-{suffix}")),
            provider_message_state: None,
            tool_calls: vec![ConversationToolCall {
                id: format!("item-invalid-{suffix}"),
                tool_call_id: Some(format!("call-invalid-{suffix}")),
                tool_name: String::from("file_edit"),
                arguments: Value::Null,
                argument_parse_error: Some(format!("invalid JSON {suffix}")),
            }],
            continuation: None,
            usage: None,
        };
        // thirdResponse would prove an unintended second correction request if it were consumed.
        // thirdResponse 若被消费即可证明错误地发起了第二次纠错请求。
        let third_response = ProviderTurnResponse {
            assistant_text: String::from("unexpected third Provider turn"),
            assistant_reasoning: String::new(),
            provider_message_id: Some(String::from("response-unexpected-third")),
            provider_message_state: None,
            tool_calls: Vec::new(),
            continuation: None,
            usage: None,
        };
        let adapter = MockStreamingProviderAdapter::new(vec![
            (Vec::new(), malformed_response("first")),
            (Vec::new(), malformed_response("second")),
            (Vec::new(), third_response),
        ]);
        let executor = MockToolExecutor::new();
        let mut controller =
            VulcanCoreController::new(build_test_config(8), ConversationSnapshot::default())
                .expect("valid core conversation snapshot");

        let result = controller
            .run_user_turn_streaming(
                &adapter,
                &executor,
                &AllowAllToolPolicy,
                "Apply the requested edit.",
            )
            .await
            .expect("the second malformed tool call should end cleanly");

        assert_eq!(result.provider_turns_executed, 2);
        assert_eq!(result.tool_calls_processed, 2);
        assert_eq!(adapter.built_requests().len(), 2);
        assert!(executor.executed_requests().is_empty());
        let rejected_results = controller
            .snapshot()
            .messages
            .iter()
            .filter(|message| {
                matches!(
                    &message.payload,
                    ConversationPayload::ToolResult { is_error: true, text, .. }
                        if text.starts_with("[tool_argument_parse_error]")
                )
            })
            .count();
        assert_eq!(rejected_results, 2);
    });
}

/// Verifies restored invalid Provider arguments remain non-executable and produce one model-visible tool error.
/// 验证恢复后的无效 Provider 参数仍不可执行，并生成一条模型可见工具错误。
#[test]
fn restored_malformed_provider_arguments_never_dispatch_executor() {
    // runtime drives the recovered batch and one following Provider turn.
    // runtime 驱动恢复后的批次及其下一次 Provider 回合。
    let runtime = tokio::runtime::Runtime::new().expect("create tokio runtime");
    runtime.block_on(async {
        // decoderMessage is the retained terminal diagnostic that forbids host execution.
        // decoderMessage 是禁止宿主执行的已保留终态诊断。
        let decoder_message = "Provider tool arguments are incomplete JSON";
        // request is the exact restored placeholder plus its non-execution proof.
        // request 是精确恢复的占位参数及其未执行证明。
        let request = ToolInvocationRequest {
            request_stream_id: String::from("stream-restored-invalid"),
            tool_call_id: String::from("call-restored-invalid"),
            tool_name: String::from("file_edit"),
            arguments: Value::Null,
            argument_parse_error: Some(decoder_message.to_string()),
        };
        // snapshot retains the authoritative assistant call at the recovery boundary.
        // snapshot 在恢复边界保留权威助手工具调用。
        let snapshot = ConversationSnapshot {
            messages: vec![ConversationMessage {
                id: String::from("assistant-restored-invalid"),
                payload: ConversationPayload::Assistant {
                    text: String::new(),
                    reasoning: String::new(),
                    provider_message_id: Some(String::from("provider-restored-invalid")),
                    provider_message_state: None,
                    usage: None,
                    tool_calls: vec![ConversationToolCall {
                        id: String::from("tool-item-restored-invalid"),
                        tool_call_id: Some(request.tool_call_id.clone()),
                        tool_name: request.tool_name.clone(),
                        arguments: request.arguments.clone(),
                        argument_parse_error: request.argument_parse_error.clone(),
                    }],
                },
            }],
            ..ConversationSnapshot::default()
        };
        // adapter supplies the correction response after receiving the synthetic tool error.
        // adapter 在收到合成工具错误后提供纠正响应。
        let adapter = MockStreamingProviderAdapter::new(vec![(
            Vec::new(),
            ProviderTurnResponse {
                assistant_text: String::from("I will submit corrected arguments."),
                assistant_reasoning: String::new(),
                provider_message_id: Some(String::from("provider-after-restored-invalid")),
                provider_message_state: None,
                tool_calls: Vec::new(),
                continuation: None,
                usage: None,
            },
        )]);
        // executor records any accidental dispatch across the restored boundary.
        // executor 记录任何意外跨越恢复边界的分派。
        let executor = MockToolExecutor::new();
        // controller owns the exact retained assistant tail.
        // controller 持有精确保留的助手尾部。
        let mut controller = VulcanCoreController::new(build_test_config(2), snapshot)
            .expect("valid restored conversation snapshot");

        controller
            .run_restored_tool_batch_streaming_with_checkpoints_and_interventions(
                &adapter,
                &executor,
                &AllowAllToolPolicy,
                crate::types::RestoredToolBatchRecovery {
                    tools: vec![crate::types::RestoredToolInvocation {
                        request,
                        disposition: crate::types::RestoredToolInvocationDisposition::Execute,
                    }],
                    user_followup: None,
                },
                |_, _| {},
                |_| Ok(false),
            )
            .await
            .expect("restore malformed tool call as a tool error");

        assert!(executor.executed_requests().is_empty());
        assert_eq!(executor.prepared_batches().len(), 1);
        let built_requests = adapter.built_requests();
        assert_eq!(built_requests.len(), 1);
        assert!(built_requests[0].history.iter().any(|message| {
            matches!(
                &message.payload,
                ConversationPayload::ToolResult {
                    tool_call_id,
                    is_error: true,
                    text,
                } if tool_call_id.as_deref() == Some("call-restored-invalid")
                    && text.contains(decoder_message)
            )
        }));
        assert!(controller.events().iter().all(|event| {
            !matches!(event.kind, CoreEventKind::ToolStarted)
                || event
                    .metadata
                    .as_ref()
                    .and_then(|metadata| metadata.get("tool_call_id"))
                    .and_then(Value::as_str)
                    != Some("call-restored-invalid")
        }));
    });
}

/// Verifies restored batches execute only not-started calls and bind one ordered result to every original call.
/// 验证恢复批次仅执行未启动调用，并为每个原始调用按顺序绑定一个结果。
#[test]
fn restored_tool_batch_reuses_results_and_synthesizes_unknown_outcomes_in_order() {
    // runtime drives the asynchronous recovery loop.
    // runtime 驱动异步恢复循环。
    let runtime = tokio::runtime::Runtime::new().expect("create tokio runtime");
    runtime.block_on(async {
        // calls are the exact original Provider batch retained at the assistant tail.
        // calls 是保留在助手尾部的精确原始 Provider 批次。
        let calls = vec![
            ConversationToolCall {
                id: String::from("item-execute"),
                tool_call_id: Some(String::from("call-execute")),
                tool_name: String::from("fresh_tool"),
                arguments: json!({"index": 1}),
                argument_parse_error: None,
            },
            ConversationToolCall {
                id: String::from("item-reuse"),
                tool_call_id: Some(String::from("call-reuse")),
                tool_name: String::from("retained_tool"),
                arguments: json!({"index": 2}),
                argument_parse_error: None,
            },
            ConversationToolCall {
                id: String::from("item-unknown"),
                tool_call_id: Some(String::from("call-unknown")),
                tool_name: String::from("uncertain_tool"),
                arguments: json!({"index": 3}),
                argument_parse_error: None,
            },
        ];
        // snapshot ends at the exact assistant tool batch that recovery must reconcile.
        // snapshot 结束于恢复必须协调的精确助手工具批次。
        let snapshot = ConversationSnapshot {
            messages: vec![ConversationMessage {
                id: String::from("assistant-tool-batch"),
                payload: ConversationPayload::Assistant {
                    text: String::new(),
                    reasoning: String::new(),
                    provider_message_id: Some(String::from("provider-tool-batch")),
                    provider_message_state: None,
                    usage: None,
                    tool_calls: calls.clone(),
                },
            }],
            ..ConversationSnapshot::default()
        };
        // restoredTools assigns exactly one authoritative disposition to each original call.
        // restoredTools 为每个原始调用分配一个权威处置方式。
        let restored_tools = vec![
            crate::types::RestoredToolInvocation {
                request: ToolInvocationRequest {
                    request_stream_id: String::from("stream-execute"),
                    tool_call_id: String::from("call-execute"),
                    tool_name: String::from("fresh_tool"),
                    arguments: json!({"index": 1}),
                    argument_parse_error: None,
                },
                disposition: crate::types::RestoredToolInvocationDisposition::Execute,
            },
            crate::types::RestoredToolInvocation {
                request: ToolInvocationRequest {
                    request_stream_id: String::from("stream-reuse"),
                    tool_call_id: String::from("call-reuse"),
                    tool_name: String::from("retained_tool"),
                    arguments: json!({"index": 2}),
                    argument_parse_error: None,
                },
                disposition: crate::types::RestoredToolInvocationDisposition::ReuseResult {
                    result: ToolInvocationResult {
                        tool_call_id: String::from("call-reuse"),
                        tool_name: String::from("retained_tool"),
                        model_output: String::from("retained result"),
                        host_result: Some(json!({"retained": true})),
                        is_error: false,
                    },
                },
            },
            crate::types::RestoredToolInvocation {
                request: ToolInvocationRequest {
                    request_stream_id: String::from("stream-unknown"),
                    tool_call_id: String::from("call-unknown"),
                    tool_name: String::from("uncertain_tool"),
                    arguments: json!({"index": 3}),
                    argument_parse_error: None,
                },
                disposition: crate::types::RestoredToolInvocationDisposition::OutcomeUnknown,
            },
        ];
        // adapter supplies the single next Provider response after tool reconciliation.
        // adapter 在工具协调后提供唯一的下一次 Provider 响应。
        let adapter = MockStreamingProviderAdapter::new(vec![(
            Vec::new(),
            ProviderTurnResponse {
                assistant_text: String::from("recovery completed"),
                assistant_reasoning: String::new(),
                provider_message_id: Some(String::from("provider-final")),
                provider_message_state: None,
                tool_calls: Vec::new(),
                continuation: None,
                usage: None,
            },
        )]);
        // executor exposes one outcome only for the not-started call.
        // executor 仅为未启动调用提供一个结果。
        let executor = MockToolExecutor::new().with_outcome(
            "fresh_tool",
            "fresh result",
            Some(json!({"fresh": true})),
            false,
        );
        // controller owns the exact retained assistant tail.
        // controller 持有精确保留的助手尾部。
        let mut controller = VulcanCoreController::new(build_test_config(3), snapshot)
            .expect("valid restored conversation snapshot");

        controller
            .run_restored_tool_batch_streaming_with_checkpoints_and_interventions(
                &adapter,
                &executor,
                &AllowAllToolPolicy,
                crate::types::RestoredToolBatchRecovery {
                    tools: restored_tools,
                    user_followup: None,
                },
                |_, _| {},
                |_| Ok(false),
            )
            .await
            .expect("restore exact tool batch");

        assert_eq!(
            executor
                .executed_requests()
                .iter()
                .map(|request| request.tool_call_id.as_str())
                .collect::<Vec<_>>(),
            vec!["call-execute"]
        );
        // results are the three original-call bindings in original model order.
        // results 是按模型原始顺序排列的三个原始调用绑定结果。
        let results = controller
            .snapshot()
            .messages
            .iter()
            .filter_map(|message| match &message.payload {
                ConversationPayload::ToolResult {
                    tool_call_id,
                    is_error,
                    text,
                } => Some((tool_call_id.as_deref(), *is_error, text.as_str())),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(results.len(), 3);
        assert_eq!(results[0], (Some("call-execute"), false, "fresh result"));
        assert_eq!(results[1], (Some("call-reuse"), false, "retained result"));
        assert_eq!(results[2].0, Some("call-unknown"));
        assert!(results[2].1);
        assert!(results[2].2.contains("no reliable result is available"));
        assert_eq!(adapter.built_requests().len(), 1);
    });
}

/// Verifies a fully reconciled restored batch never enters the host execution boundary with an empty batch.
/// 验证已经完全协调的恢复批次绝不会以空批次进入宿主执行边界。
#[test]
fn restored_tool_batch_without_executable_calls_skips_empty_execution_boundary() {
    // runtime drives the asynchronous recovery loop through one following Provider response.
    // runtime 通过一次后续 Provider 响应驱动异步恢复循环。
    let runtime = tokio::runtime::Runtime::new().expect("create tokio runtime");
    runtime.block_on(async {
        // request is the exact original invocation whose result became unknown after interruption.
        // request 是中断后结果变为未知的精确原始调用。
        let request = ToolInvocationRequest {
            request_stream_id: String::from("stream-unknown-only"),
            tool_call_id: String::from("call-unknown-only"),
            tool_name: String::from("uncertain_tool"),
            arguments: json!({"index": 1}),
            argument_parse_error: None,
        };
        // snapshot ends at the assistant call that must receive one synthetic result without re-execution.
        // snapshot 结束于必须在不重新执行的情况下接收一条合成结果的助手调用。
        let snapshot = ConversationSnapshot {
            messages: vec![ConversationMessage {
                id: String::from("assistant-unknown-only"),
                payload: ConversationPayload::Assistant {
                    text: String::new(),
                    reasoning: String::new(),
                    provider_message_id: Some(String::from("provider-unknown-only")),
                    provider_message_state: None,
                    usage: None,
                    tool_calls: vec![ConversationToolCall {
                        id: String::from("item-unknown-only"),
                        tool_call_id: Some(request.tool_call_id.clone()),
                        tool_name: request.tool_name.clone(),
                        arguments: request.arguments.clone(),
                        argument_parse_error: request.argument_parse_error.clone(),
                    }],
                },
            }],
            ..ConversationSnapshot::default()
        };
        // adapter supplies the next Provider response after the synthetic result is appended.
        // adapter 在追加合成结果后提供下一次 Provider 响应。
        let adapter = MockStreamingProviderAdapter::new(vec![(
            Vec::new(),
            ProviderTurnResponse {
                assistant_text: String::from("recovery completed"),
                assistant_reasoning: String::new(),
                provider_message_id: Some(String::from("provider-final")),
                provider_message_state: None,
                tool_calls: Vec::new(),
                continuation: None,
                usage: None,
            },
        )]);
        // executor records every host batch so an accidental empty preparation remains observable.
        // executor 记录每个宿主批次，使意外的空批次准备保持可观测。
        let executor = MockToolExecutor::new();
        // controller owns the exact retained assistant boundary.
        // controller 持有精确保留的助手边界。
        let mut controller = VulcanCoreController::new(build_test_config(2), snapshot)
            .expect("valid restored conversation snapshot");

        controller
            .run_restored_tool_batch_streaming_with_checkpoints_and_interventions(
                &adapter,
                &executor,
                &AllowAllToolPolicy,
                crate::types::RestoredToolBatchRecovery {
                    tools: vec![crate::types::RestoredToolInvocation {
                        request,
                        disposition:
                            crate::types::RestoredToolInvocationDisposition::OutcomeUnknown,
                    }],
                    user_followup: None,
                },
                |_, _| {},
                |_| Ok(false),
            )
            .await
            .expect("restore unknown-only tool batch");

        assert!(executor.prepared_batches().is_empty());
        assert!(executor.executed_requests().is_empty());
        assert!(controller.snapshot().messages.iter().any(|message| {
            matches!(
                &message.payload,
                ConversationPayload::ToolResult {
                    tool_call_id: Some(tool_call_id),
                    is_error: true,
                    text,
                } if tool_call_id == "call-unknown-only"
                    && text.contains("no reliable result is available")
            )
        }));
        // unknownRecord exposes the machine-readable warning that the UI and later recovery audits consume.
        // unknownRecord 暴露 UI 与后续恢复审计所消费的机器可读警告。
        let unknown_record = controller
            .snapshot()
            .records
            .iter()
            .find(|record| {
                record.metadata.as_ref().and_then(|metadata| {
                    metadata
                        .get("tool_call_id")
                        .and_then(serde_json::Value::as_str)
                }) == Some("call-unknown-only")
            })
            .expect("unknown tool outcome should create one structured record");
        // unknownMetadata must distinguish possible external effects from a proven unstarted cancellation.
        // unknownMetadata 必须区分可能存在的外部副作用与已证明未启动的取消。
        let unknown_metadata = unknown_record
            .metadata
            .as_ref()
            .expect("unknown tool outcome should retain metadata");
        assert_eq!(
            unknown_metadata
                .get("status")
                .and_then(serde_json::Value::as_str),
            Some("interrupted_outcome_unknown")
        );
        assert_eq!(
            unknown_metadata
                .get("possible_external_effects")
                .and_then(serde_json::Value::as_bool),
            Some(true)
        );
        assert_eq!(adapter.built_requests().len(), 1);
    });
}

/// Verifies a recovery follow-up cancels a proven not-started tool and is appended only after its synthetic result.
/// 验证恢复补充消息会取消已证明未启动的工具，并且仅在其合成结果之后追加。
#[test]
fn restored_user_followup_pairs_not_started_tool_before_real_user_text() {
    // runtime drives the restored tool boundary and following Provider request.
    // runtime 驱动恢复工具边界及其后的 Provider 请求。
    let runtime = tokio::runtime::Runtime::new().expect("create tokio runtime");
    runtime.block_on(async {
        // request is the exact original tool call that never entered host execution.
        // request 是从未进入宿主执行的精确原始工具调用。
        let request = ToolInvocationRequest {
            request_stream_id: String::from("stream-not-started"),
            tool_call_id: String::from("call-not-started"),
            tool_name: String::from("write_file"),
            arguments: json!({"path": "safe.txt"}),
            argument_parse_error: None,
        };
        // snapshot ends at the unmatched assistant tool call.
        // snapshot 结束于尚未配对的助手工具调用。
        let snapshot = ConversationSnapshot {
            messages: vec![ConversationMessage {
                id: String::from("assistant-not-started"),
                payload: ConversationPayload::Assistant {
                    text: String::new(),
                    reasoning: String::new(),
                    provider_message_id: Some(String::from("provider-not-started")),
                    provider_message_state: None,
                    usage: None,
                    tool_calls: vec![ConversationToolCall {
                        id: String::from("item-not-started"),
                        tool_call_id: Some(request.tool_call_id.clone()),
                        tool_name: request.tool_name.clone(),
                        arguments: request.arguments.clone(),
                        argument_parse_error: request.argument_parse_error.clone(),
                    }],
                },
            }],
            ..ConversationSnapshot::default()
        };
        // adapter exposes the request history used after recovery ordering completes.
        // adapter 暴露恢复顺序完成后使用的请求历史。
        let adapter = MockStreamingProviderAdapter::new(vec![(
            Vec::new(),
            ProviderTurnResponse {
                assistant_text: String::from("updated request accepted"),
                assistant_reasoning: String::new(),
                provider_message_id: Some(String::from("provider-followup-final")),
                provider_message_state: None,
                tool_calls: Vec::new(),
                continuation: None,
                usage: None,
            },
        )]);
        // executor must remain untouched because the follow-up supersedes the not-started call.
        // executor 必须保持未触发，因为补充消息取代了未启动调用。
        let executor = MockToolExecutor::new();
        // controller owns the exact unmatched tool boundary.
        // controller 持有精确的未配对工具边界。
        let mut controller = VulcanCoreController::new(build_test_config(2), snapshot)
            .expect("valid restored conversation snapshot");
        // checkpoints expose the protocol-safe order before Provider dispatch.
        // checkpoints 暴露 Provider 分派前的协议安全顺序。
        let mut checkpoints = Vec::<ConversationSnapshot>::new();

        controller
            .run_restored_tool_batch_streaming_with_checkpoints_and_interventions(
                &adapter,
                &executor,
                &AllowAllToolPolicy,
                crate::types::RestoredToolBatchRecovery {
                    tools: vec![crate::types::RestoredToolInvocation {
                        request,
                        disposition:
                            crate::types::RestoredToolInvocationDisposition::CancelledBeforeExecution,
                    }],
                    user_followup: Some(crate::types::RestoredUserFollowup {
                        visible_prompt: String::from("先不要写文件，改为只分析。"),
                        provider_prompt: String::from(
                            "先不要写文件，改为只分析。\n\n<hidden_host_guidance>\nUse analysis only.\n</hidden_host_guidance>",
                        ),
                        admission_id: String::from("followup-admission-a"),
                        attachments: Vec::new(),
                    }),
                },
                |snapshot, _| checkpoints.push(snapshot.clone()),
                |_| Ok(false),
            )
            .await
            .expect("recover with a real user follow-up");

        assert!(executor.prepared_batches().is_empty());
        assert!(executor.executed_requests().is_empty());
        let first_checkpoint = checkpoints
            .first()
            .expect("reconciled follow-up checkpoint should be emitted");
        assert!(matches!(
            first_checkpoint.messages.as_slice(),
            [
                ConversationMessage {
                    payload: ConversationPayload::Assistant { .. },
                    ..
                },
                ConversationMessage {
                    payload: ConversationPayload::ToolResult { is_error: true, text, .. },
                    ..
                },
                ConversationMessage {
                    payload: ConversationPayload::UserText { text: prompt, .. },
                    ..
                }
            ] if text.contains("was not executed")
                && prompt.contains("<hidden_host_guidance>")
        ));
        // cancelledRecord proves the obsolete call was closed without claiming possible external effects.
        // cancelledRecord 证明已过时调用在未声明可能外部副作用的情况下被封闭。
        let cancelled_record = first_checkpoint
            .records
            .iter()
            .find(|record| {
                record.metadata.as_ref().and_then(|metadata| {
                    metadata.get("tool_call_id").and_then(serde_json::Value::as_str)
                }) == Some("call-not-started")
            })
            .expect("cancelled not-started call should create one structured record");
        // cancelledMetadata carries the exact non-execution status consumed by user-facing projection.
        // cancelledMetadata 携带用户界面投影所消费的精确未执行状态。
        let cancelled_metadata = cancelled_record
            .metadata
            .as_ref()
            .expect("cancelled not-started call should retain metadata");
        assert_eq!(
            cancelled_metadata
                .get("status")
                .and_then(serde_json::Value::as_str),
            Some("cancelled_before_execution")
        );
        assert_eq!(
            cancelled_metadata
                .get("possible_external_effects")
                .and_then(serde_json::Value::as_bool),
            Some(false)
        );
        let visible_user_record = first_checkpoint
            .records
            .iter()
            .rev()
            .find(|record| record.role == TranscriptRole::User)
            .expect("follow-up should create one visible user record");
        assert_eq!(visible_user_record.body, "先不要写文件，改为只分析。");
        assert!(!visible_user_record.body.contains("hidden_host_guidance"));
        let built_requests = adapter.built_requests();
        assert_eq!(built_requests.len(), 1);
        assert!(matches!(
            built_requests[0].history.last(),
            Some(ConversationMessage {
                payload: ConversationPayload::UserText { text, .. },
                ..
            }) if text.contains("<hidden_host_guidance>")
        ));
    });
}

/// Verifies a retained tool-free assistant tail completes locally without another Provider request.
/// 验证保留的无工具助手尾部会在本地完成，而不会再次请求 Provider。
#[test]
fn restored_final_assistant_completes_without_provider_dispatch() {
    // snapshot contains the Provider-proven tool-free assistant tail.
    // snapshot 包含已由 Provider 证明的无工具助手尾部。
    let snapshot = ConversationSnapshot {
        messages: vec![ConversationMessage {
            id: String::from("assistant-final"),
            payload: ConversationPayload::Assistant {
                text: String::from("complete answer"),
                reasoning: String::new(),
                provider_message_id: Some(String::from("provider-final")),
                provider_message_state: None,
                usage: None,
                tool_calls: Vec::new(),
            },
        }],
        ..ConversationSnapshot::default()
    };
    // controller finalizes the retained proof without receiving any adapter.
    // controller 在不接收任何适配器的情况下完成保留证明。
    let mut controller = VulcanCoreController::new(build_test_config(1), snapshot)
        .expect("valid final assistant snapshot");

    let result = controller
        .finalize_restored_assistant()
        .expect("finalize retained assistant locally");

    assert_eq!(result.provider_turns_executed, 0);
    assert_eq!(result.tool_calls_processed, 0);
    assert_eq!(
        result.final_assistant_message_id.as_deref(),
        Some("assistant-final")
    );
    assert_eq!(controller.phase(), ControllerPhase::Completed);
}

/// Generates a synthetic skipped tool result when the host policy refuses execution and continues the loop.
/// 当宿主策略拒绝执行时生成一条合成跳过工具结果，并继续推进后续循环。
#[test]
fn run_user_turn_synthesizes_skipped_tool_result_when_policy_blocks_tool() {
    let runtime = tokio::runtime::Runtime::new().expect("create tokio runtime");
    runtime.block_on(async {
        let adapter = MockProviderAdapter::new(vec![
            ProviderTurnResponse {
                assistant_text: String::new(),
                assistant_reasoning: String::new(),
                provider_message_id: Some(String::from("resp-skip-1")),
                provider_message_state: None,
                tool_calls: vec![ConversationToolCall {
                    id: String::from("tool-item-skip-1"),
                    tool_call_id: Some(String::from("call-skip-1")),
                    tool_name: String::from("blocked_tool"),
                    arguments: json!({"id":"42"}),
                    argument_parse_error: None,
                }],
                continuation: None,
                usage: None,
            },
            ProviderTurnResponse {
                assistant_text: String::from("已收到宿主返回的跳过结果。"),
                assistant_reasoning: String::new(),
                provider_message_id: Some(String::from("resp-skip-2")),
                provider_message_state: None,
                tool_calls: Vec::new(),
                continuation: None,
                usage: None,
            },
        ]);
        let executor = MockToolExecutor::new();
        let policy = ExactToolSetPolicy::new(["allowed_tool"]);
        let mut controller =
            VulcanCoreController::new(build_test_config(3), ConversationSnapshot::default())
                .expect("valid core conversation snapshot");

        let result = controller
            .run_user_turn(&adapter, &executor, &policy, "请调用被限制的工具")
            .await
            .expect("run blocked tool turn");

        assert_eq!(result.provider_turns_executed, 2);
        assert_eq!(result.tool_calls_processed, 1);
        assert!(executor.executed_requests().is_empty());
        assert!(matches!(
            &controller.snapshot().messages[2].payload,
            ConversationPayload::ToolResult { text, .. }
                if text == "Tool execution was skipped by the host policy."
        ));
        assert!(controller.snapshot().records.iter().any(|record| {
            record.title == "调用 blocked_tool 工具"
                && record.body == "Tool execution was skipped by the host policy."
        }));
    });
}

/// Verifies the controller falls back to the local tool-call identifier when the provider-visible id is empty.
/// 验证当 Provider 可见 id 为空时，控制器会回退到本地工具调用标识。
#[test]
fn resolved_tool_call_id_falls_back_when_provider_visible_id_is_empty() {
    let tool_call = crate::types::ConversationToolCall {
        id: String::from("local-call-1"),
        tool_call_id: Some(String::from("   ")),
        tool_name: String::from("read_file"),
        arguments: json!({ "path": "src/main.rs" }),
        argument_parse_error: None,
    };

    assert_eq!(resolved_tool_call_id(&tool_call), "local-call-1");
}
