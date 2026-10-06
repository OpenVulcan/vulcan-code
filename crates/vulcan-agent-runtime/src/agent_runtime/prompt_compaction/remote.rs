//! Native compaction dispatch with one declared owner, permit, and terminal per physical attempt.
//! 原生压缩分派，每次物理尝试分别持有一位已声明所有者、一份许可与一个终态。

use super::*;
use crate::agent_runtime::events::{
    PreparedPhysicalApi, ProviderRequestMediaContext, ProviderRequestPreparationCandidate,
    provider_request_durability_barrier, provider_request_preparation_barrier,
};
use crate::agent_runtime::execution_models::ExecutionPreparation;
use crate::agent_runtime::prompt_compaction::PromptCompactionOwnership;
use crate::agent_runtime::provider_protocol_tool_definition::ProviderToolDefinition;
use crate::agent_runtime::provider_request_bridge_conversation::build_core_conversation_snapshot;
use crate::agent_runtime::provider_request_telemetry::{
    ProviderRequestTelemetryContext, ProviderRequestTelemetryEvent,
    ProviderRequestTelemetryEventKind, ProviderRequestTelemetryTracker,
};
use crate::agent_runtime::provider_runtime_context::RuntimePromptLayers;
use crate::agent_runtime::provider_runtime_context::synchronize_runtime_host_events;
use crate::agent_runtime::provider_transport_request_budget::ProviderRequestBudgetGuard;
use crate::agent_runtime::provider_transport_vulcan::{
    ProviderApiTraceAttemptCapture, ProviderApiTraceAttemptState, ProviderApiTraceRenderIdentity,
    VulcanPublicContractAdapter, resolved_public_output_token_limit,
};
use tokio::sync::mpsc::UnboundedSender;
use vulcan_ai_contract::AiTransportKind;
use vulcan_core::provider::VulcanProviderAdapter;
use vulcan_core::types::ControllerConfig;
use vulcan_provider_adapters::{ResolvedPublicAiExecution, ResponsesCompactEvent};
use vulcan_session_core::v1::context::RemoteResponsesCheckpoint;

/// Builds a pure candidate using the ordinary route adapter and exact planner-selected canonical messages.
/// 使用普通路由适配器及规划器精确选中的规范消息构建纯候选。
/// `execution` freezes routing; `preparation` freezes budget controls; prompt/tools retain ordinary visibility.
/// `execution`冻结路由；`preparation`冻结预算控制；提示词和工具保留普通可见性。
/// `state` and `plan` select lossless history; temperature/reasoning retain the owning agent's settings.
/// `state`与`plan`选择无损历史；温度和推理保留所属代理设置。
/// Returns an unprepared candidate; its declared owner must authorize physical dispatch.
/// 返回未准备候选；其已声明所有者必须授权物理分派。
pub(super) async fn build_candidate(
    execution: &Arc<ResolvedPublicAiExecution>,
    preparation: &ExecutionPreparation,
    prompt: RuntimePromptLayers,
    ownership: PromptCompactionOwnership,
    tools: Vec<ProviderToolDefinition>,
    state: &SessionConversationState,
    plan: &CompactionPlan,
    temperature: Option<f32>,
    reasoning: ReasoningEffortLevel,
) -> Result<ProviderRequestPreparationCandidate, String> {
    let mut snapshot = prepare_candidate_snapshot(state, plan, &prompt, ownership)?;
    // Compact has no response continuation; each attempt sends its full permitted source window.
    // Compact 不使用响应延续；每次尝试发送完整的已许可来源窗口。
    snapshot.continuation = Default::default();
    let prompt_layers = candidate_prompt_layers(&prompt, ownership);
    let config = ControllerConfig {
        provider_id: execution.route.provider.id.clone(),
        model_id: execution.model.id.clone(),
        system_prompt: prompt.fallback_text,
        // RuntimeOnly dispatches this exact candidate. Its dynamic instructions must stay in
        // tagged User history, never in the stable System prefix rendered by the adapter.
        // RuntimeOnly 直接分派此候选；动态指令必须留在带标签 User 历史中，绝不能进入适配器渲染的
        // 稳定 System 前缀。
        prompt_layers,
        temperature: temperature.map(f64::from),
        max_turns: 1,
        additional_params: None,
        context_injection: None,
    };
    // Candidate construction cannot emit provider deltas or dispatch a network request.
    // 候选构建不能发出供应商增量或分派网络请求。
    let (live_sender, _live_receiver) = tokio::sync::mpsc::unbounded_channel();
    let adapter = VulcanPublicContractAdapter::new(
        false,
        execution.route.clone(),
        execution.model.id.clone(),
        preparation.session_id.to_string(),
        execution.credential.clone(),
        execution.client.clone(),
        tools,
        execution.model.capabilities.provider_native_tools.clone(),
        resolved_public_output_token_limit(
            execution.overrides.output_limit,
            execution.model.capabilities.output_limit,
        ),
        execution.overrides.prompt_cache.clone().unwrap_or_default(),
        public_reasoning_effort_from_level(reasoning),
        execution.overrides.model_parameters.clone(),
        execution.model.capabilities.output_resources.clone(),
        preparation.workspace_path.clone(),
        live_sender,
        prompt.dynamic_prompt_layers.clone(),
        prompt.locked_baseline.clone(),
        Arc::clone(execution),
    );
    let request = adapter
        .build_request(&snapshot, &config)
        .await
        .map_err(|error| error.to_string())?;
    // Reuse the compaction constructor's auto=false policy while retaining the model's hard window.
    // 复用压缩构造器的 auto=false 策略，同时保留模型硬窗口。
    let budget_guard = ProviderRequestBudgetGuard::new_v1(
        preparation.model_blueprint.provider_id.clone(),
        preparation.model_blueprint.model_id.clone(),
        preparation.model_blueprint.protocol_profile_id.clone(),
        preparation.model_blueprint.route_revision.clone(),
        preparation.model_blueprint.public_capabilities.clone(),
        preparation.model_blueprint.reasoning.clone(),
        preparation.context_compaction_policy,
        preparation.context_budget_override,
        preparation
            .context_budget
            .as_ref()
            .and_then(|budget| budget.auto_compact_prefill_input_tokens),
    );
    Ok(ProviderRequestPreparationCandidate {
        source_message_positions: request.source_message_positions,
        media_context: Some(ProviderRequestMediaContext {
            execution: Arc::clone(execution),
            session_scope_id: preparation.session_id.to_string(),
        }),
        exact_request: request.public,
        controller_config: request.controller_config,
        controller_snapshot: request.controller_snapshot,
        request_snapshot: request.budget,
        context_budget: None,
        request_budget_guard: Some(budget_guard),
        locked_baseline: prompt.locked_baseline,
        dynamic_prompt_layers: prompt.dynamic_prompt_layers,
    })
}

/// Selects the prompt layers whose rendering is authorized at the candidate's top boundary.
/// 选择允许在候选请求顶部渲染的提示词层。
/// `prompt` supplies the composed layers; `ownership` states whether the candidate is dispatched directly.
/// `prompt` 提供已组合层；`ownership` 声明候选请求是否被直接分派。
/// Returns the session-only projection or an empty projection that renders the stable fallback.
/// 返回仅供会话使用的投影，或渲染稳定回退提示词的空投影。
fn candidate_prompt_layers(
    prompt: &RuntimePromptLayers,
    ownership: PromptCompactionOwnership,
) -> vulcan_core::types::ProviderPromptLayers {
    match ownership {
        PromptCompactionOwnership::SessionV1 => prompt.provider_layers.clone(),
        PromptCompactionOwnership::RuntimeOnly => Default::default(),
    }
}

/// Selects the exact compaction source and appends standalone dynamic guidance as tagged User history.
/// 选择精确压缩来源，并把独立执行的动态指引追加为带标签 User 历史。
///
/// # Parameters
/// 参数
/// `state` is the full canonical window; `plan` names the exact selected source messages.
/// `state` 是完整规范窗口；`plan` 指定精确选中的来源消息。
/// `prompt` owns current dynamic candidates; `ownership` selects whether this candidate is dispatched directly.
/// `prompt` 持有当前动态候选；`ownership` 决定是否直接分派此候选。
///
/// # Returns
/// 返回值
/// The selected source followed by newly synchronized host User events, or a source/ledger error.
/// 返回选中来源及随后新同步的宿主 User 事件，或来源、账本错误。
fn prepare_candidate_snapshot(
    state: &SessionConversationState,
    plan: &CompactionPlan,
    prompt: &RuntimePromptLayers,
    ownership: PromptCompactionOwnership,
) -> Result<vulcan_core::types::ConversationSnapshot, String> {
    // Source selection uses canonical IDs in their original order, never reconstructed text.
    // 来源选择使用原始顺序的规范 ID，绝不使用重建正文。
    let source_ids = plan
        .source_messages
        .iter()
        .map(|message| message.id.as_str())
        .collect::<BTreeSet<_>>();
    if source_ids.is_empty() || source_ids.len() != plan.source_messages.len() {
        return Err(String::from(
            "Remote compaction requires unique nonempty source identities.",
        ));
    }
    let mut snapshot = build_core_conversation_snapshot(state)?;
    let original_message_count = snapshot.messages.len();
    if ownership == PromptCompactionOwnership::RuntimeOnly {
        // Synchronize against the complete canonical ledger before selecting a partial source;
        // dropping older messages first would make their still-live ledger records invalid.
        // 必须先对完整规范账本同步再截取部分来源；提前丢弃旧消息会使其仍有效的账本记录失效。
        synchronize_runtime_host_events(&mut snapshot, &prompt.host_events, None)?;
    }
    let appended_host_ids = snapshot.messages[original_message_count..]
        .iter()
        .map(|message| message.id.clone())
        .collect::<BTreeSet<_>>();
    snapshot.messages.retain(|message| {
        source_ids.contains(message.id.as_str()) || appended_host_ids.contains(&message.id)
    });
    if !snapshot
        .messages
        .iter()
        .filter(|message| source_ids.contains(message.id.as_str()))
        .map(|message| message.id.as_str())
        .eq(plan
            .source_messages
            .iter()
            .map(|message| message.id.as_str()))
    {
        return Err(String::from(
            "Remote compaction source differs from the canonical history order.",
        ));
    }
    Ok(snapshot)
}

/// Sends `event` to the owning consumer; returns an explicit error if it has disappeared.
/// 将`event`发送给所属消费方；消费方消失时返回明确错误。
fn emit(
    events: &UnboundedSender<PromptCompactionStreamEvent>,
    event: PromptCompactionStreamEvent,
) -> Result<(), String> {
    events
        .send(event)
        .map_err(|_| String::from("The remote compaction consumer closed before acknowledgement."))
}

/// Publishes one physical `event` and waits for its unique owner's durable acknowledgement.
/// 发布一个物理`event`并等待唯一所有者的可靠确认。
/// Returns the owner's failure without polling more transport data or starting another attempt.
/// 返回所有者错误，不继续轮询传输数据或启动下一次尝试。
async fn persist(
    events: &UnboundedSender<PromptCompactionStreamEvent>,
    event: ProviderRequestTelemetryEvent,
) -> Result<(), String> {
    let (completion, receiver) = provider_request_durability_barrier();
    emit(
        events,
        PromptCompactionStreamEvent::Stream(
            ExecutionStreamEvent::ProviderRequestDurabilityBoundary { event, completion },
        ),
    )?;
    receiver.await.map_err(|_| {
        String::from("The remote compaction durability owner dropped its acknowledgement.")
    })?
}

/// Closes the active `tracker` failure before returning or considering a same-account OAuth replay.
/// 在返回或考虑同账号 OAuth 重放前关闭活动`tracker`失败。
/// `error` is the normalized transport failure; `retry` declares at most one conditional credential recovery.
/// `error`是规范传输失败；`retry`声明至多一次有条件的凭据恢复。
/// Returns the deferred retry notification only after the failed API has been durably completed.
/// 仅在失败 API 已可靠完成后返回延迟发布的重试通知。
async fn settle_failure(
    events: &UnboundedSender<PromptCompactionStreamEvent>,
    tracker: &mut ProviderRequestTelemetryTracker,
    error: Option<&vulcan_ai_contract::AiExecutionError>,
    retry: bool,
) -> Result<Option<ProviderRequestTelemetryEvent>, String> {
    let mut retry_event = None;
    for event in tracker.complete_failure(
        retry,
        u32::from(retry),
        1,
        0,
        "remote_compaction",
        error,
        None,
    ) {
        if event.kind == ProviderRequestTelemetryEventKind::Completed {
            persist(events, event).await?;
        } else if event.kind == ProviderRequestTelemetryEventKind::Retrying {
            retry_event = Some(event);
        }
    }
    Ok(retry_event)
}

/// Executes the frozen native route through a new durable permit for every physical request.
/// 为每次物理请求取得新的可靠许可，再执行冻结的原生路由。
/// `candidate` is never sent directly; `plan` owns result boundaries; `events` is the session consumer.
/// `candidate`绝不直接发送；`plan`持有结果边界；`events`连接会话消费方。
/// Returns a route-bound native result only after its terminal checkpoint has been durably acknowledged.
/// 仅在终态检查点被可靠确认后返回绑定路由的原生结果。
pub(super) async fn execute_durable(
    execution: Arc<ResolvedPublicAiExecution>,
    preparation: ExecutionPreparation,
    candidate: ProviderRequestPreparationCandidate,
    plan: &CompactionPlan,
    events: &UnboundedSender<PromptCompactionStreamEvent>,
    provider_request_trace_enabled: bool,
    ownership: PromptCompactionOwnership,
) -> Result<PromptCompactionExecution, String> {
    // The tracker retains a distinct request identity and independent usage for each admitted attempt.
    // 跟踪器为每次准入尝试保留不同的请求身份及独立用量。
    let mut tracker = ProviderRequestTelemetryTracker::default();
    // Remote transport bypasses the ordinary launch, so declare its confirmation owner here as well.
    // 远端传输绕过普通启动，因此也必须在此声明其确认所有者。
    let telemetry_context = match ownership {
        PromptCompactionOwnership::SessionV1 => {
            ProviderRequestTelemetryContext::from_preparation(&preparation, None)?
        }
        PromptCompactionOwnership::RuntimeOnly => {
            super::standalone_owner::standalone_provider_telemetry_context(&preparation)?
        }
    };
    tracker.install_context(telemetry_context);
    // traceAttemptState preserves the ordinary per-API physical-attempt numbering across retries.
    // traceAttemptState 跨重试保留普通请求相同的按 API 物理尝试编号。
    let trace_attempt_state = std::sync::Mutex::new(ProviderApiTraceAttemptState::default());
    let mut oauth_replay_used = false;
    let mut request_count = 0usize;
    loop {
        let event = tracker.prepare_request(
            &candidate.request_snapshot,
            candidate.context_budget.as_ref(),
        )?;
        let (completion, receiver) = provider_request_preparation_barrier();
        emit(
            events,
            PromptCompactionStreamEvent::Stream(
                ExecutionStreamEvent::ProviderRequestPreparationBoundary {
                    event,
                    candidate: candidate.clone(),
                    completion,
                },
            ),
        )?;
        let permit = match receiver.await {
            Ok(Ok(permit)) => permit,
            outcome => {
                tracker.reject_prepared_request();
                return Err(match outcome {
                    Ok(Err(error)) => error,
                    Err(_) => {
                        String::from("The remote compaction preparation owner dropped its permit.")
                    }
                    Ok(Ok(_)) => unreachable!(),
                });
            }
        };
        let (request, snapshot, budget, trace_capture) = match permit {
            PreparedPhysicalApi::SessionV1 {
                exact_request,
                controller_snapshot,
                request_snapshot,
                context_budget,
                core_start,
                provider_api_trace_store,
                ..
            } => {
                // The native compaction attempt reuses the ordinary session trace owner carried by
                // its own physical permit; it never infers the frozen opt-in decision from it.
                // 原生压缩尝试复用自身物理许可携带的普通会话追踪 owner；绝不据此推断冻结开关决策。
                let trace_capture = ProviderApiTraceAttemptCapture::start(
                    provider_request_trace_enabled,
                    &trace_attempt_state,
                    provider_api_trace_store,
                    core_start.api_call_id,
                    ProviderApiTraceRenderIdentity {
                        provider_id: execution.route.provider.id.clone(),
                        session_scope_id: preparation.session_id.to_string(),
                        model_id: execution.model.id.clone(),
                        route: execution.route.clone(),
                    },
                    controller_snapshot
                        .messages
                        .last()
                        .map(|message| message.id.clone()),
                );
                (
                    exact_request,
                    request_snapshot,
                    context_budget,
                    trace_capture,
                )
            }
            PreparedPhysicalApi::RequestConfigurationChanged => {
                tracker.reject_prepared_request();
                return Err(String::from(
                    "Remote compaction inputs changed before dispatch; rebuild the compaction plan.",
                ));
            }
            PreparedPhysicalApi::ExcludedNonSession {
                exact_request,
                request_snapshot,
                context_budget,
                ..
            } => {
                // A standalone RuntimeOnly run owns no V1 session ledger, so its own standalone owner
                // releases an excluded non-session permit; accepting it here is what lets that owner
                // account for the physical attempt instead of dispatching an unacknowledged request.
                // 独立的 RuntimeOnly 运行不持有 V1 会话账本，因此由其自身独立所有者放行排除非会话
                // 许可；在这里接受它，才能让该所有者对该真实尝试记账，而不是派发一次无人确认的请求。
                if ownership != PromptCompactionOwnership::RuntimeOnly {
                    tracker.reject_prepared_request();
                    return Err(String::from(
                        "A session remote compaction cannot use an excluded non-session permit.",
                    ));
                }
                // No session ledger means no session trace owner, so the frozen opt-in renders nothing.
                // 没有会话账本就没有会话追踪 owner，因此冻结开关不会渲染任何制品。
                (exact_request, request_snapshot, context_budget, None)
            }
        };
        let started = tracker.confirm_prepared_request(&snapshot, budget.as_ref())?;
        emit(
            events,
            PromptCompactionStreamEvent::Stream(ExecutionStreamEvent::ProviderRequestTelemetry(
                started,
            )),
        )?;
        request_count += 1;
        // Credentials are read only after durable admission and immediately before this attempt.
        // 凭据只在可靠准入之后、本次尝试之前读取。
        let secret = match execution.credential.resolve_for_request().await {
            Ok(secret) => secret,
            Err(error) => {
                settle_failure(events, &mut tracker, None, false).await?;
                return Err(format!(
                    "Remote compaction credential resolution failed before dispatch: {error}"
                ));
            }
        };
        let rejected_revision = secret.revision();
        // traceEndpoints exist only when the frozen opt-in is enabled and the session owner
        // reserved one sequence; disabled tracing keeps the compact dispatch byte-for-byte opaque.
        // traceEndpoints 仅在冻结开关启用且会话 owner 预留了一个序号时存在；关闭追踪时压缩分派保持
        // 与之前完全一致。
        let (trace_request_endpoint, trace_response_endpoint) = match trace_capture {
            Some(capture) => (Some(capture.request), Some(capture.response)),
            None => (None, None),
        };
        let mut stream = execution.client.stream_responses_compact(
            execution.route.clone(),
            request,
            secret,
            trace_request_endpoint,
            trace_response_endpoint,
        );
        let mut terminal = None;
        while let Some(event) = stream.next().await {
            match event {
                Ok(ResponsesCompactEvent::TransportObserved {
                    dispatched_at_ms,
                    headers_at_ms,
                    http_status,
                    provider_request_id,
                    rate_limit_headers,
                }) => {
                    let observed = tracker
                        .observe_transport(
                            AiTransportKind::Http,
                            dispatched_at_ms,
                            Some(headers_at_ms),
                            Some(http_status),
                            provider_request_id,
                            rate_limit_headers,
                        )
                        .ok_or_else(|| {
                            String::from("Remote compaction transport has no admitted request.")
                        })?;
                    persist(events, observed).await?;
                }
                Ok(ResponsesCompactEvent::Completed(result)) => {
                    terminal = Some(Ok(result));
                    break;
                }
                Err(error) => {
                    terminal = Some(Err(error));
                    break;
                }
            }
        }
        // Drop the completed physical stream before credential recovery or another admission.
        // 在凭据恢复或再次准入前释放已终结的物理流。
        drop(stream);
        let result = match terminal {
            Some(Ok(result)) => result,
            Some(Err(failure)) => {
                let retry = !oauth_replay_used
                    && failure.error.http_status == Some(401)
                    && !failure.error.semantic_output_observed
                    && !failure.error.side_effect_observed;
                let retry_event =
                    settle_failure(events, &mut tracker, Some(&failure.error), retry).await?;
                if retry {
                    match execution
                        .credential
                        .refresh_after_unauthorized(rejected_revision)
                        .await
                    {
                        Ok(true) => {
                            oauth_replay_used = true;
                            if let Some(event) = retry_event {
                                emit(
                                    events,
                                    PromptCompactionStreamEvent::Stream(
                                        ExecutionStreamEvent::ProviderRequestTelemetry(event),
                                    ),
                                )?;
                            }
                            continue;
                        }
                        Ok(false) => {}
                        Err(error) => {
                            return Err(format!(
                                "{failure} Host OAuth credential refresh failed: {error}"
                            ));
                        }
                    }
                }
                return Err(failure.to_string());
            }
            None => {
                settle_failure(events, &mut tracker, None, false).await?;
                return Err(String::from(
                    "Remote compaction transport ended without a terminal result.",
                ));
            }
        };
        let usage = remote_responses_usage_to_core(result.usage.as_ref());
        let checkpoint = RemoteResponsesCheckpoint {
            provider_id: execution.route.provider.id.clone(),
            connection_id: execution.route.connection.id.clone(),
            route_fingerprint: execution.route.request_url("/responses"),
            protocol_profile_id: execution.route.protocol_route.protocol_profile_id.clone(),
            model_id: execution.model.id.clone(),
            output_items: result.output_items.clone(),
            response_id: result.response_id.clone(),
        };
        let completed = tracker.complete_native_compaction_success(usage.as_ref())?;
        let (completion, receiver) = provider_request_durability_barrier();
        emit(
            events,
            PromptCompactionStreamEvent::RemoteCompleted {
                event: completed,
                checkpoint,
                completion,
            },
        )?;
        receiver.await.map_err(|_| {
            String::from("The remote compaction owner dropped its native terminal acknowledgement.")
        })??;
        execution.credential.mark_request_succeeded();
        let compaction_result = CompactionResult::from_remote_responses_with_metadata(
            plan,
            ProviderContextRoute {
                provider_id: execution.route.provider.id.clone(),
                connection_id: execution.route.connection.id.clone(),
                route_fingerprint: execution.route.request_url("/responses"),
                protocol_profile_id: execution.route.protocol_route.protocol_profile_id.clone(),
                model_id: execution.model.id.clone(),
            },
            result.output_items,
            result.usage,
            result.response_id,
        )?;
        return Ok(PromptCompactionExecution {
            compaction_result,
            fallback_used: false,
            provider_request_count: request_count,
            last_successful_provider_usage: usage,
            provider_id: Some(execution.route.provider.id.clone()),
            model_id: Some(execution.model.id.clone()),
            execution_profile: PromptCompactionExecutionProfile::RemoteResponses,
            input_prune_report: None,
        });
    }
}

#[cfg(test)]
mod candidate_tests {
    use super::*;
    use crate::agent_runtime::provider_runtime_context::RuntimeHostEventCandidate;
    use vulcan_context::{
        SessionMessageEntry, SessionMessageKind, conversation_state_from_session_messages,
    };
    use vulcan_core::types::ProviderPromptLayers;

    /// Builds a request with distinct stable and dynamic instructions for candidate projection.
    /// 构建稳定与动态指令明确不同的请求，供候选投影测试使用。
    /// Returns a prompt whose current Plan guidance must not enter the standalone System prefix.
    /// 返回当前 Plan 指引不能进入独立请求 System 前缀的提示词。
    fn prompt() -> RuntimePromptLayers {
        RuntimePromptLayers {
            fallback_text: String::from("stable baseline"),
            provider_layers: ProviderPromptLayers::from_system_prompt("dynamic Plan guidance"),
            dynamic_prompt_layers: ProviderPromptLayers::from_system_prompt(
                "dynamic Plan guidance",
            ),
            locked_baseline: vulcan_session_core::v1::context::LockedBaselineProjection::empty(),
            host_events: vec![RuntimeHostEventCandidate {
                source_id: String::from("plan:current"),
                source: String::from("plan"),
                target: String::from("model"),
                task_id: None,
                scope: String::from("request"),
                body: String::from("dynamic Plan guidance"),
                occurrence_id: None,
                content_hash: None,
                event_id: None,
                kind: vulcan_ai_contract::HostMessageKind::ContextUpdate,
                expires_source: false,
            }],
        }
    }

    /// Verifies standalone native compaction places dynamic guidance after the selected source as User history.
    /// 验证独立原生压缩把动态指引作为 User 历史放在选中来源之后。
    /// The session-owned candidate remains unchanged because its V1 owner rebuilds the physical request.
    /// 会话候选保持原样，因为其 V1 所有者会重建物理请求。
    #[test]
    fn standalone_candidate_keeps_dynamic_guidance_out_of_top_system() {
        let source = SessionMessageEntry::new(SessionMessageKind::User, "", "source")
            .with_runtime_metadata("source-1", "t1");
        let omitted_tail = SessionMessageEntry::new(SessionMessageKind::User, "", "tail")
            .with_runtime_metadata("tail-1", "t1");
        let state = conversation_state_from_session_messages(&[source.clone(), omitted_tail])
            .expect("canonical source should replay");
        let policy = vulcan_context::ContextCompactionPolicy::default();
        let plan = CompactionPlan {
            source_messages: vec![source],
            source_until_message_id: None,
            source_until_turn_index: None,
            retained_tail_start_turn_index: None,
            compaction_id: String::from("remote-candidate-test"),
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
        };
        let prompt = prompt();
        let standalone = prepare_candidate_snapshot(
            &state,
            &plan,
            &prompt,
            PromptCompactionOwnership::RuntimeOnly,
        )
        .expect("standalone candidate should synchronize current host guidance");
        assert_eq!(standalone.messages.len(), 2);
        assert_eq!(standalone.messages[0].id, "source-1");
        let vulcan_core::types::ConversationPayload::UserText { text, .. } =
            &standalone.messages[1].payload
        else {
            panic!("dynamic host guidance must use a User message");
        };
        assert!(text.contains("<vulcan_context_update"));
        assert!(text.contains("dynamic Plan guidance"));
        assert_eq!(
            candidate_prompt_layers(&prompt, PromptCompactionOwnership::RuntimeOnly)
                .render_or_legacy(&prompt.fallback_text),
            "stable baseline"
        );

        let session = prepare_candidate_snapshot(
            &state,
            &plan,
            &prompt,
            PromptCompactionOwnership::SessionV1,
        )
        .expect("session candidate should keep its selected source");
        assert_eq!(session.messages.len(), 1);
        assert!(
            candidate_prompt_layers(&prompt, PromptCompactionOwnership::SessionV1)
                .render_or_legacy(&prompt.fallback_text)
                .contains("dynamic Plan guidance")
        );
    }
}
