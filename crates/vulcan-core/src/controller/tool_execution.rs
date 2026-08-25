use super::*;

/// Stable model-visible marker for a rejected Provider tool-argument document.
/// Provider 工具参数文档被拒绝时使用的稳定模型可见标记。
pub(super) const TOOL_ARGUMENT_PARSE_FAILURE_PREFIX: &str = "[tool_argument_parse_error]";

/// Builds one non-executed tool result for Provider arguments that could not be parsed as complete JSON.
/// 为无法解析为完整 JSON 的 Provider 参数构建一条未执行工具结果。
///
/// # Parameters
/// 参数
/// `request` retains the exact tool identity and `message` is the controlled protocol diagnostic.
/// `request` 保留精确工具身份，`message` 是受控协议诊断。
///
/// # Returns
/// 返回值
/// A model-visible error result that explicitly proves no tool body or external side effect ran.
/// 返回一条模型可见错误结果，并明确证明工具主体及外部副作用均未执行。
fn build_tool_argument_parse_failure_result(
    request: &ToolInvocationRequest,
    message: &str,
) -> ToolInvocationResult {
    ToolInvocationResult {
        tool_call_id: request.tool_call_id.clone(),
        tool_name: request.tool_name.clone(),
        model_output: format!(
            "{TOOL_ARGUMENT_PARSE_FAILURE_PREFIX}\nTool `{}` was not executed because the Provider emitted incomplete or invalid JSON arguments.\nReason: {}\nCreate a new tool call with complete JSON that conforms to the tool schema. This is the only automatic correction attempt for this user turn.",
            request.tool_name, message,
        ),
        host_result: Some(json!({
            "kind": "tool_argument_parse_error",
            "status": "failed",
            "executed": false,
            "side_effect_observed": false,
            "tool_call_id": request.tool_call_id,
            "tool_name": request.tool_name,
            "error": message,
        })),
        is_error: true,
    }
}

/// Converts an unexpected executor failure into one terminal model-visible tool result.
/// 将意外的执行器失败转换为一条终态且模型可见的工具结果。
fn build_tool_execution_failure_result(
    request: &ToolInvocationRequest,
    error: &CoreError,
) -> ToolInvocationResult {
    let message = error.to_string();
    ToolInvocationResult {
        tool_call_id: request.tool_call_id.clone(),
        tool_name: request.tool_name.clone(),
        model_output: format!(
            "Tool `{}` failed during host execution. No automatic Provider retry will run. The tool may have started, so inspect the affected state before deciding whether to invoke it again. Error: {message}",
            request.tool_name,
        ),
        host_result: Some(json!({
            "kind": "tool_execution_error",
            "status": "failed",
            "executed": true,
            "possible_external_effects": true,
            "retryable": false,
            "tool_call_id": request.tool_call_id,
            "tool_name": request.tool_name,
            "error": message,
        })),
        is_error: true,
    }
}

/// Converts a batch-preparation failure into one terminal non-executed result.
/// 将批次准备失败转换为一条终态且未执行的结果。
fn build_tool_batch_preparation_failure_result(
    request: &ToolInvocationRequest,
    error: &CoreError,
) -> ToolInvocationResult {
    let message = error.to_string();
    ToolInvocationResult {
        tool_call_id: request.tool_call_id.clone(),
        tool_name: request.tool_name.clone(),
        model_output: format!(
            "Tool `{}` was not executed because the host could not prepare its durable execution boundary. No automatic Provider retry will run. Error: {message}",
            request.tool_name,
        ),
        host_result: Some(json!({
            "kind": "tool_batch_preparation_error",
            "status": "failed_before_execution",
            "executed": false,
            "possible_external_effects": false,
            "retryable": false,
            "tool_call_id": request.tool_call_id,
            "tool_name": request.tool_name,
            "error": message,
        })),
        is_error: true,
    }
}

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
                || restored.request.argument_parse_error != assistant_call.argument_parse_error
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
        if let Some(message) = request.argument_parse_error.as_deref() {
            let rejected_result = build_tool_argument_parse_failure_result(request, message);
            self.record_tool_finished(&rejected_result);
            return Ok(rejected_result);
        }
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
        let result = match executor.execute_tool(request).await {
            Ok(result) => result,
            Err(error) => build_tool_execution_failure_result(request, &error),
        };
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
        if let Err(error) = executor.prepare_tool_batch(&requests).await {
            let results = requests
                .iter()
                .map(|request| {
                    let result = build_tool_batch_preparation_failure_result(request, &error);
                    self.record_tool_finished(&result);
                    result
                })
                .collect();
            return Ok(results);
        }
        let mut result_slots = std::iter::repeat_with(|| None)
            .take(requests.len())
            .collect::<Vec<Option<Result<ToolInvocationResult, CoreError>>>>();
        let mut pending_executions = FuturesUnordered::new();
        let policy_snapshot = self.snapshot.clone();
        for (request_index, request) in requests.into_iter().enumerate() {
            if let Some(message) = request.argument_parse_error.as_deref() {
                let rejected_result = build_tool_argument_parse_failure_result(&request, message);
                // Provider-invalid arguments are terminal before dispatch and therefore cannot enter the host executor.
                // Provider 无效参数在分派前即为终态，因此不得进入宿主执行器。
                self.record_tool_finished(&rejected_result);
                result_slots[request_index] = Some(Ok(rejected_result));
                continue;
            }
            self.record_tool_started(&request);
            if policy.should_execute(&request, &policy_snapshot) {
                pending_executions.push(async move {
                    let result = match executor.execute_tool(&request).await {
                        Ok(result) => result,
                        Err(error) => build_tool_execution_failure_result(&request, &error),
                    };
                    (request_index, Ok(result))
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
