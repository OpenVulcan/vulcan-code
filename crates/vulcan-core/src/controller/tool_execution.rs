use super::*;

impl VulcanCoreController {
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
        let mut skipped_results = Vec::<Option<ToolInvocationResult>>::new();
        let mut executable_requests = Vec::<ToolInvocationRequest>::new();
        let policy_snapshot = self.snapshot.clone();
        for request in requests {
            self.record_tool_started(&request);
            if policy.should_execute(&request, &policy_snapshot) {
                skipped_results.push(None);
                executable_requests.push(request);
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
                skipped_results.push(Some(skipped_result));
            }
        }

        let executable_results =
            join_all(executable_requests.iter().map(|request| async move {
                let result = executor.execute_tool(request).await.map_err(|error| {
                    CoreError::ToolFailure {
                        tool_name: request.tool_name.clone(),
                        message: error.to_string(),
                    }
                })?;
                Ok::<ToolInvocationResult, CoreError>(result)
            }))
            .await;
        let mut executable_results = executable_results.into_iter();
        let mut ordered_results = Vec::with_capacity(skipped_results.len());
        for skipped_result in skipped_results {
            let result = match skipped_result {
                Some(result) => result,
                None => executable_results.next().ok_or_else(|| CoreError::InvalidState {
                    message: String::from(
                        "Tool batch execution lost one executable result before transcript append.",
                    ),
                })??,
            };
            self.record_tool_finished(&result);
            ordered_results.push(result);
        }
        Ok(ordered_results)
    }
}
