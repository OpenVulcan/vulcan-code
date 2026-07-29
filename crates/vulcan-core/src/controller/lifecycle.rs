use super::*;

impl VulcanCoreController {
    /// Creates a new controller with one configuration and one initial conversation snapshot.
    /// 使用一份配置与一份初始会话快照创建新的控制器。
    ///
    /// # Parameters
    /// 参数
    /// `config` is the stable execution configuration owned by the new controller.
    /// `config` 是新控制器持有的稳定执行配置。
    /// `snapshot` is the complete canonical state whose identity namespaces must be valid.
    /// `snapshot` 是身份命名空间必须有效的完整规范状态。
    ///
    /// # Returns
    /// 返回值
    /// Returns the initialized controller, or an invalid-state error without taking ownership of corrupt state.
    /// 返回初始化后的控制器；状态损坏时返回无效状态错误且不接管该状态。
    pub fn new(
        config: ControllerConfig,
        snapshot: ConversationSnapshot,
    ) -> Result<Self, CoreError> {
        validate_conversation_snapshot_identities(&snapshot)?;
        Ok(Self {
            config,
            snapshot,
            last_request_snapshot: None,
            events: Vec::new(),
            phase: ControllerPhase::Idle,
            last_successful_provider_usage: None,
        })
    }

    /// Returns the immutable controller configuration.
    /// 返回不可变的控制器配置。
    pub fn config(&self) -> &ControllerConfig {
        &self.config
    }

    /// Returns the immutable canonical snapshot currently owned by the controller.
    /// 返回控制器当前持有的不可变规范快照。
    pub fn snapshot(&self) -> &ConversationSnapshot {
        &self.snapshot
    }

    /// Replaces the canonical snapshot with a newer validated snapshot.
    /// 用一份经过校验的更新规范快照替换当前内容。
    ///
    /// # Parameters
    /// 参数
    /// `snapshot` is the complete replacement candidate validated before commit.
    /// `snapshot` 是提交前需要完整校验的替换候选快照。
    ///
    /// # Returns
    /// 返回值
    /// Returns success after atomic replacement, or an invalid-state error with the prior snapshot retained.
    /// 原子替换后返回成功；否则返回无效状态错误并保留原快照。
    pub fn replace_snapshot(&mut self, snapshot: ConversationSnapshot) -> Result<(), CoreError> {
        validate_conversation_snapshot_identities(&snapshot)?;
        self.snapshot = snapshot;
        self.last_successful_provider_usage = None;
        Ok(())
    }

    /// Returns the latest captured request snapshot if one exists.
    /// 返回最近一次捕获到的请求快照（如果存在）。
    pub fn last_request_snapshot(&self) -> Option<&Value> {
        self.last_request_snapshot.as_ref()
    }

    /// Returns the most recent successful provider usage observed during the current controller run.
    /// 返回当前控制器运行期间观测到的最近一次成功 Provider 用量。
    pub fn last_successful_provider_usage(&self) -> Option<&ProviderUsageSnapshot> {
        self.last_successful_provider_usage.as_ref()
    }

    /// Returns the current coarse-grained controller phase.
    /// 返回当前粗粒度控制器阶段。
    pub fn phase(&self) -> ControllerPhase {
        self.phase
    }

    /// Returns all controller events accumulated so far.
    /// 返回当前累计的全部控制器事件。
    pub fn events(&self) -> &[CoreEvent] {
        &self.events
    }

    /// Clears the accumulated controller events.
    /// 清空当前累计的控制器事件。
    pub fn clear_events(&mut self) {
        self.events.clear();
    }

    /// Appends one controller event to the in-memory event buffer.
    /// 向内存事件缓冲区追加一条控制器事件。
    pub fn push_event(&mut self, event: CoreEvent) {
        self.events.push(event);
    }

    /// Records one provider-emitted text event for later WebUI and audit projection.
    /// 记录一条 Provider 产出的文本事件，供后续 WebUI 与审计投影使用。
    pub fn record_output(&mut self, body: impl Into<String>) {
        self.push_event(CoreEvent {
            kind: CoreEventKind::Output,
            title: String::from("模型输出"),
            body: body.into(),
            metadata: None,
        });
    }

    /// Records one provider-emitted reasoning event for later WebUI and audit projection.
    /// 记录一条 Provider 产出的思考事件，供后续 WebUI 与审计投影使用。
    pub fn record_thinking(&mut self, body: impl Into<String>) {
        self.push_event(CoreEvent {
            kind: CoreEventKind::Thinking,
            title: String::from("模型思考"),
            body: body.into(),
            metadata: None,
        });
    }

    /// Records one tool-start event with structured tool-call metadata.
    /// 记录一条带结构化工具调用元数据的工具开始事件。
    pub fn record_tool_started(&mut self, request: &ToolInvocationRequest) {
        self.push_event(CoreEvent {
            kind: CoreEventKind::ToolStarted,
            title: format!("调用 {} 工具", request.tool_name),
            body: render_json_pretty(&request.arguments),
            metadata: Some(json!({
                "tool_call_id": request.tool_call_id,
                "tool_name": request.tool_name,
                "arguments": request.arguments,
            })),
        });
    }

    /// Records one tool-finished event with structured tool-result metadata.
    /// 记录一条带结构化工具结果元数据的工具完成事件。
    pub fn record_tool_finished(&mut self, result: &ToolInvocationResult) {
        self.push_event(CoreEvent {
            kind: CoreEventKind::ToolFinished,
            title: format!("调用 {} 工具", result.tool_name),
            body: result.model_output.clone(),
            metadata: Some(build_tool_result_metadata(result)),
        });
    }

    /// Marks the controller as failed and captures the failure body as one error event.
    /// 将控制器标记为失败，并把失败正文作为一条错误事件捕获下来。
    pub fn mark_failed(&mut self, body: impl Into<String>) {
        self.phase = ControllerPhase::Failed;
        self.push_event(CoreEvent {
            kind: CoreEventKind::Error,
            title: String::from("控制器失败"),
            body: body.into(),
            metadata: None,
        });
    }

    /// Captures the provider request built by the adapter and keeps a serialized snapshot for debugging.
    /// 捕获适配器构建出的 Provider 请求，并保留一份序列化快照用于调试。
    pub async fn build_request<A>(&mut self, adapter: &A) -> Result<A::Request, CoreError>
    where
        A: VulcanProviderAdapter,
    {
        self.phase = ControllerPhase::PreparingRequest;
        let request = adapter
            .build_request(&self.snapshot, &self.config)
            .await
            .map_err(|error| CoreError::ProviderFailure {
                provider_id: adapter.provider_id().to_string(),
                message: error.to_string(),
                details: None,
            })?;
        self.last_request_snapshot = serde_json::to_value(&request).ok();
        Ok(request)
    }
}
