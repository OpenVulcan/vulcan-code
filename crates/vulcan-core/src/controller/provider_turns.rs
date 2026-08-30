use super::*;

impl VulcanCoreController {
    /// Executes one normalized provider turn without emitting an artificial final event.
    /// 执行一次规范化 Provider 回合，但不额外发出人为合成的完成事件。
    pub(super) async fn execute_provider_turn<A>(
        &mut self,
        adapter: &A,
    ) -> Result<ProviderTurnResponse, CoreError>
    where
        A: VulcanProviderAdapter<Response = ProviderTurnResponse>,
    {
        let request = match self.build_request(adapter).await {
            Ok(request) => request,
            Err(error) => {
                self.mark_failed(error.to_string());
                return Err(error);
            }
        };
        if let Err(error) = adapter.validate_built_request(&request) {
            self.mark_failed(error.to_string());
            return Err(error);
        }
        self.phase = ControllerPhase::WaitingForProvider;
        match adapter.execute_request(request).await {
            Ok(response) => {
                self.phase = ControllerPhase::Idle;
                Ok(response)
            }
            Err(error) => {
                self.mark_failed(error.to_string());
                Err(error)
            }
        }
    }

    /// Executes one normalized provider turn through the streaming adapter and returns both the final turn response and buffered deltas.
    /// 通过流式适配器执行一次规范化 Provider 回合，并返回最终回合响应与缓冲增量。
    pub(super) async fn execute_provider_turn_streaming<A>(
        &mut self,
        adapter: &A,
    ) -> Result<(ProviderTurnResponse, Vec<ProviderTurnDelta>), CoreError>
    where
        A: VulcanStreamingProviderAdapter<Response = ProviderTurnResponse>,
    {
        let request = match self.build_request(adapter).await {
            Ok(request) => request,
            Err(error) => {
                self.mark_failed(error.to_string());
                return Err(error);
            }
        };
        if let Err(error) = adapter.validate_built_request(&request) {
            self.mark_failed(error.to_string());
            return Err(error);
        }
        self.phase = ControllerPhase::WaitingForProvider;
        let mut sink = BufferedProviderDeltaSink::default();
        match adapter.execute_request_streaming(request, &mut sink).await {
            Ok(response) => {
                self.phase = ControllerPhase::Idle;
                Ok((response, sink.deltas))
            }
            Err(error) => {
                // A changed pre-dispatch selection is control flow, not a failed Provider execution.
                // 派发前选择变更属于控制流，不是 Provider 执行失败。
                if matches!(&error, CoreError::ProviderRouteSwitchRequired { .. }) {
                    self.phase = ControllerPhase::Idle;
                    return Err(error);
                }
                self.mark_failed(error.to_string());
                Err(error)
            }
        }
    }

    /// Applies buffered provider-stream deltas to the controller-owned event trace in the original emission order.
    /// 按原始发出顺序将缓冲的 Provider 流式增量应用到控制器自有事件轨迹。
    pub(super) fn apply_provider_turn_deltas(&mut self, deltas: &[ProviderTurnDelta]) {
        for delta in deltas {
            match delta {
                ProviderTurnDelta::Thinking { delta } => self.record_thinking(delta.clone()),
                ProviderTurnDelta::Output { delta } => self.record_output(delta.clone()),
            }
        }
    }
}
