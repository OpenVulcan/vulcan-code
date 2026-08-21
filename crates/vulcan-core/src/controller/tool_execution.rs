use super::*;

impl VulcanCoreController {
    /// Reconciles one retained Provider tool batch without replaying completed or uncertain invocations.
    /// 协调一批已保留的 Provider 工具调用，且不重放已完成或结果不确定的调用。
    pub(super) async fn execute_restored_tool_batch<E, P>(
        &mut self,
        executor: &E,
        policy: &P,
        restored_tools: Vec<RestoredToolInvocation>,
    ) -> Result<Vec<ToolInvocationResult>, CoreError>
    where
        E: VulcanToolExecutor,
        P: VulcanToolPolicy,
    {
        // assistantCalls is the exact canonical tool batch that every retained recovery slot must bind to.
        // assistantCalls 是每个保留恢复槽位都必须绑定的精确规范工具批次。
        let assistant_calls = match self
            .snapshot
            .messages
            .last()
            .map(|message| &message.payload)
        {
            Some(ConversationPayload::Assistant { tool_calls, .. }) if !tool_calls.is_empty() => {
                tool_calls.clone()
            }
            _ => {
                return Err(CoreError::InvalidState {
                    message: String::from(
                        "Restored tool recovery requires an assistant message with tool calls at the canonical tail.",
                    ),
                });
            }
        };
        if assistant_calls.len() != restored_tools.len() {
            return Err(CoreError::InvalidState {
                message: String::from(
                    "Restored tool recovery count does not match the canonical assistant tool batch.",
                ),
            });
        }
        for (assistant_call, restored) in assistant_calls.iter().zip(&restored_tools) {
            // expectedToolCallId resolves the sole Provider-visible identity accepted for this canonical call.
            // expectedToolCallId 解析该规范调用唯一获准的 Provider 可见身份。
            let expected_tool_call_id = resolved_tool_call_id(assistant_call);
            if restored.request.tool_call_id != expected_tool_call_id
                || restored.request.tool_name != assistant_call.tool_name
                || restored.request.arguments != assistant_call.arguments
            {
                return Err(CoreError::InvalidState {
                    message: format!(
                        "Restored tool request `{}` does not match the canonical assistant tool batch.",
                        restored.request.tool_call_id
                    ),
                });
            }
        }

        // executableRequests includes only calls durably proven not to have started and not superseded by a user follow-up.
        // executableRequests 仅包含可靠证明尚未启动且未被用户补充取代的调用。
        let executable_requests = restored_tools
            .iter()
            .filter(|restored| {
                matches!(
                    restored.disposition,
                    RestoredToolInvocationDisposition::Execute
                )
            })
            .map(|restored| restored.request.clone())
            .collect::<Vec<_>>();
        // executableResults bypasses the execution boundary when every retained call already has a reusable or synthetic result.
        // executableResults 在所有保留调用都已有可复用或合成结果时绕过执行边界。
        let executable_results = if executable_requests.is_empty() {
            Vec::new()
        } else {
            self.execute_tool_batch(executor, policy, executable_requests)
                .await?
        };
        // executableResultsIterator binds fresh results back to Execute slots without reordering the retained batch.
        // executableResultsIterator 将新结果绑定回 Execute 槽位，且不重排保留批次。
        let mut executable_results = executable_results.into_iter();
        // orderedResults preserves one output per original assistant tool call in exact model order.
        // orderedResults 按模型精确顺序为每条原始助手工具调用保留一条输出。
        let mut ordered_results = Vec::with_capacity(restored_tools.len());
        for restored in restored_tools {
            // result is either freshly executed, durably reused, or an explicit synthetic interruption result.
            // result 只能是新执行结果、可靠复用结果或显式合成的中断结果。
            let result = match restored.disposition {
                RestoredToolInvocationDisposition::Execute => executable_results
                    .next()
                    .ok_or_else(|| CoreError::InvalidState {
                        message: String::from(
                            "Restored tool recovery lost one newly executed result.",
                        ),
                    })?,
                RestoredToolInvocationDisposition::ReuseResult { result } => {
                    if result.tool_call_id != restored.request.tool_call_id
                        || result.tool_name != restored.request.tool_name
                    {
                        return Err(CoreError::InvalidState {
                            message: String::from(
                                "Restored durable tool result does not match its original request.",
                            ),
                        });
                    }
                    result
                }
                RestoredToolInvocationDisposition::OutcomeUnknown => {
                    // result tells the model that external effects may exist while avoiding an unsafe replay.
                    // result 告知模型可能已产生外部影响，同时避免不安全重放。
                    let result = ToolInvocationResult {
                        tool_call_id: restored.request.tool_call_id.clone(),
                        tool_name: restored.request.tool_name.clone(),
                        model_output: String::from(
                            "The previous tool invocation was interrupted after execution may have started, so no reliable result is available. Decide whether it is safe to invoke the tool again before retrying.",
                        ),
                        host_result: Some(json!({
                            "kind": "interrupted_tool_outcome_unknown",
                            "status": "interrupted_outcome_unknown",
                            "possible_external_effects": true,
                            "tool_call_id": restored.request.tool_call_id,
                            "tool_name": restored.request.tool_name,
                        })),
                        is_error: true,
                    };
                    self.record_tool_finished(&result);
                    result
                }
                RestoredToolInvocationDisposition::CancelledBeforeExecution => {
                    // result closes the obsolete call before the newer user instruction is appended.
                    // result 在追加较新的用户指令前封闭已过时调用。
                    let result = ToolInvocationResult {
                        tool_call_id: restored.request.tool_call_id.clone(),
                        tool_name: restored.request.tool_name.clone(),
                        model_output: String::from(
                            "The tool invocation was not executed because the user supplied additional instructions after the previous run stopped. Re-evaluate the request and invoke the tool again only if it is still appropriate.",
                        ),
                        host_result: Some(json!({
                            "kind": "interrupted_tool_not_started",
                            "status": "cancelled_before_execution",
                            "possible_external_effects": false,
                            "reason": "user_followup_before_recovery",
                            "tool_call_id": restored.request.tool_call_id,
                            "tool_name": restored.request.tool_name,
                        })),
                        is_error: true,
                    };
                    self.record_tool_finished(&result);
                    result
                }
            };
            ordered_results.push(result);
        }
        if executable_results.next().is_some() {
            return Err(CoreError::InvalidState {
                message: String::from(
                    "Restored tool recovery produced more execution results than requests.",
                ),
            });
        }
        Ok(ordered_results)
    }

    /// Executes one host-owned tool request through the provided tool executor and applies the current tool policy.
    /// 通过给定的工具执行器执行一次宿主自有工具请求，并应用当前工具策略。
    pub async fn execute_tool<E, P>(
        &mut self,
        executor: &E,
        policy: &P,
        request: &ToolInvocationRequest,
    ) -> Result<ToolInvocationResult, CoreError>
    where
        E: VulcanToolExecutor,
        P: VulcanToolPolicy,
    {
        if !policy.should_execute(request, &self.snapshot) {
            let skipped_result = ToolInvocationResult {
                tool_call_id: request.tool_call_id.clone(),
                tool_name: request.tool_name.clone(),
                model_output: String::from("Tool execution was skipped by the host policy."),
                host_result: Some(json!({
                    "tool_call_id": request.tool_call_id,
                    "tool_name": request.tool_name,
                    "status": "skipped_by_policy",
                    "arguments": request.arguments,
                })),
                is_error: false,
            };
            self.record_tool_started(request);
            self.record_tool_finished(&skipped_result);
            return Ok(skipped_result);
        }

        self.record_tool_started(request);
        let result =
            executor
                .execute_tool(request)
                .await
                .map_err(|error| CoreError::ToolFailure {
                    tool_name: request.tool_name.clone(),
                    message: error.to_string(),
                })?;
        self.record_tool_finished(&result);
        Ok(result)
    }

    /// Executes one provider-emitted tool-call batch concurrently while preserving transcript append order.
    /// 并发执行一批 Provider 产出的工具调用，同时保留转录追加顺序。
    pub(super) async fn execute_tool_batch<E, P>(
        &mut self,
        executor: &E,
        policy: &P,
        requests: Vec<ToolInvocationRequest>,
    ) -> Result<Vec<ToolInvocationResult>, CoreError>
    where
        E: VulcanToolExecutor,
        P: VulcanToolPolicy,
    {
        executor.prepare_tool_batch(&requests).await?;
        let mut result_slots = std::iter::repeat_with(|| None)
            .take(requests.len())
            .collect::<Vec<Option<Result<ToolInvocationResult, CoreError>>>>();
        let mut pending_executions = FuturesUnordered::new();
        let policy_snapshot = self.snapshot.clone();
        for (request_index, request) in requests.into_iter().enumerate() {
            self.record_tool_started(&request);
            if policy.should_execute(&request, &policy_snapshot) {
                pending_executions.push(async move {
                    let result = executor.execute_tool(&request).await.map_err(|error| {
                        CoreError::ToolFailure {
                            tool_name: request.tool_name.clone(),
                            message: error.to_string(),
                        }
                    });
                    (request_index, result)
                });
            } else {
                let skipped_result = ToolInvocationResult {
                    tool_call_id: request.tool_call_id.clone(),
                    tool_name: request.tool_name.clone(),
                    model_output: String::from("Tool execution was skipped by the host policy."),
                    host_result: Some(json!({
                        "tool_call_id": request.tool_call_id,
                        "tool_name": request.tool_name,
                        "status": "skipped_by_policy",
                        "arguments": request.arguments,
                    })),
                    is_error: false,
                };
                // Policy-skipped calls already have a final result and must close independently of slower executable siblings.
                // 被策略跳过的调用已经拥有最终结果，必须独立于较慢的可执行兄弟调用立即封闭。
                self.record_tool_finished(&skipped_result);
                result_slots[request_index] = Some(Ok(skipped_result));
            }
        }

        // Completion events follow actual per-tool completion while result slots preserve the Provider-declared replay order.
        // 完成事件遵循每个工具的实际完成时序，而结果槽位保留 Provider 声明的回放顺序。
        while let Some((request_index, result)) = pending_executions.next().await {
            if let Ok(completed_result) = &result {
                self.record_tool_finished(completed_result);
            }
            result_slots[request_index] = Some(result);
        }

        let mut ordered_results = Vec::with_capacity(result_slots.len());
        for (request_index, result_slot) in result_slots.into_iter().enumerate() {
            let result = result_slot
                .ok_or_else(|| CoreError::InvalidState {
                    message: format!(
                        "Tool batch execution lost result index {request_index} before transcript append."
                    ),
                })??;
            ordered_results.push(result);
        }
        Ok(ordered_results)
    }
}
