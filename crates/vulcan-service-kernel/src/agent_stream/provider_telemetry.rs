//! Vulcan Plugin Hook forwarding for structured Provider request telemetry.
//! 结构化 Provider 请求遥测的 Vulcan 插件 Hook 转发。

use super::*;
use vulcan_agent_runtime::agent_runtime::provider_request_telemetry::{
    ProviderRequestTelemetryCancellationTracker, ProviderRequestTelemetryEvent,
};
use vulcan_hooks::{HookContext, HookScope};

/// Drop-safe forwarding guard that guarantees an active request receives a cancelled terminal Hook.
/// 支持安全丢弃的转发守卫，保证活动请求收到取消终态 Hook。
pub(crate) struct ProviderTelemetryHookGuard {
    /// Session registry for cancellation accounting; global API classification remains independent.
    /// 用于取消统计的会话注册表；全局 API 分类保持独立。
    usage_state: SharedServiceBridgeState,
    /// Protocol tracker that retains only the active request's redacted started payload.
    /// 仅保留活动请求脱敏开始载荷的协议跟踪器。
    cancellation: ProviderRequestTelemetryCancellationTracker,
    /// Bounded stream-local sender whose single worker preserves event order.
    /// 由单个 Worker 保持事件顺序的有界流局部发送器。
    sender: tokio::sync::mpsc::Sender<ProviderRequestTelemetryEvent>,
}

impl ProviderTelemetryHookGuard {
    /// Arms cancellation before releasing a successfully committed session preparation permit.
    /// 在放行成功提交的会话准备许可之前激活取消跟踪。
    /// `event` identifies the pending request; `result` is returned by the unique preparation owner.
    /// `event`标识待准备请求；`result`由唯一准备所有者返回。
    /// Rejections, configuration restarts, and excluded requests leave this session tracker unchanged.
    /// 拒绝、配置重启及排除请求保持此会话跟踪器不变。
    pub(crate) fn observe_preparation_result(
        &mut self,
        event: &ProviderRequestTelemetryEvent,
        result: &Result<vulcan_agent_runtime::agent_runtime::events::PreparedPhysicalApi, String>,
    ) {
        if matches!(
            result,
            Ok(vulcan_agent_runtime::agent_runtime::events::PreparedPhysicalApi::SessionV1 { .. })
        ) {
            // A dropped permit receiver cannot undo api_started; the consumer still owns cancellation.
            // 许可接收端被丢弃不能撤销 api_started；消费方仍承担取消责任。
            self.cancellation.observe(event);
        }
    }

    /// Creates a guard that can persist and project cancellation accounting during Drop.
    /// 创建一个可在 Drop 期间持久化并投影取消记账的守卫。
    ///
    /// # Parameters
    /// 参数
    /// `state` owns the authoritative workspace runtime state.
    /// `state` 持有权威工作区运行时状态。
    /// `storage_runtime` stores optional redacted native usage diagnostics independently of the session ledger.
    /// `storage_runtime` 独立于会话账本保存可选的脱敏原生用量诊断。
    pub(crate) fn new_with_persistence(
        state: SharedServiceBridgeState,
        storage_runtime: Option<HostRuntimeHandle>,
    ) -> Self {
        let native_usage_storage = storage_runtime;
        let usage_state = state.clone();
        // sender and receiver form one bounded low-frequency Effect queue per execution stream.
        // sender 与 receiver 为每条执行流构成一个有界低频 Effect 队列。
        let (sender, mut receiver) = tokio::sync::mpsc::channel(64);
        spawn_service_task(async move {
            while let Some(event) = receiver.recv().await {
                if let Some(storage_runtime) = native_usage_storage.as_ref()
                    && let Err(error) =
                        persist_native_provider_usage_attempt(storage_runtime, &event).await
                {
                    tracing::warn!(
                        "[webui-service] native provider usage persistence failed: {error}"
                    );
                }
                dispatch_provider_request_telemetry_hook(&state, event).await;
            }
        });
        Self {
            usage_state,
            cancellation: ProviderRequestTelemetryCancellationTracker::default(),
            sender,
        }
    }

    /// Takes the synthetic cancelled terminal event for normal stream-exit handling.
    /// 取出合成的取消终态事件，供正常流退出路径处理。
    pub(crate) fn take_cancelled_event(&mut self) -> Option<ProviderRequestTelemetryEvent> {
        let event = self.cancellation.take_cancelled_event()?;
        // The existing cancellation tracker yields this terminal once; no replay ledger is introduced.
        // 既有取消跟踪器仅产生一次当前终态；不引入重放账本。
        if let Some(vulcan_agent_runtime::agent_runtime::provider_v1_bridge::ProviderV1LifecycleFacts::ApiTerminal(terminal)) = &event.v1_lifecycle {
            let handle = self.usage_state.lock().ok()
                .and_then(|state| state.session_runtime_registry.get(event.session_id).ok());
            if let Some(handle) = handle {
                match vulcan_session_core::v1::lifetime_usage::SessionUsageBucket::from_api(
                    &vulcan_session_core::v1::UsageSnapshot::default(), &terminal.timing,
                ) {
                    Ok(increment) => handle.record_lifetime_usage(&increment, terminal.completed_at_ms),
                    Err(error) => tracing::warn!("[session-usage] cancellation: {error}"),
                }
            }
        }
        Some(event)
    }

    /// Records a replay-relevant semantic boundary observed by the owning service stream.
    /// 记录所属服务流观测到的与重放相关的语义边界。
    ///
    /// # Returns
    /// 返回值
    /// Returns no value after conservatively marking the sole active physical request.
    /// 在保守标记唯一活动物理请求后不返回值。
    pub(crate) fn observe_semantic_output(&mut self) {
        self.cancellation.observe_semantic_output();
    }

    /// Records a durable start or an observed physical terminal before semantic projection.
    /// 记录已持久化的开始边界，或语义投影前已观测到的物理终态。
    ///
    /// # Parameters
    /// 参数
    /// `event` is the exact started or terminal boundary whose durable write already succeeded.
    /// `event` 是可靠写入已经成功的精确开始或终态边界。
    ///
    /// # Returns
    /// 返回值
    /// Returns no value after advancing the drop-cancellation tracker.
    /// 推进 Drop 取消跟踪器后不返回值。
    pub(crate) fn observe_durable_boundary(&mut self, event: &ProviderRequestTelemetryEvent) {
        self.cancellation.observe(event);
    }

    /// Forwards one exact ordered telemetry event to the best-effort Hook worker.
    /// 将一条精确有序遥测事件转发到尽力而为的 Hook Worker。
    ///
    /// # Parameters
    /// 参数
    /// `event` is the versioned safe Provider telemetry event.
    /// `event` 是版本化安全 Provider 遥测事件。
    ///
    /// # Returns
    /// 返回值
    /// Returns no value because Effect Hook dispatch remains non-blocking.
    /// 不返回值，因为 Effect Hook 调度保持非阻塞。
    pub(crate) fn forward(&mut self, event: ProviderRequestTelemetryEvent) {
        if self.sender.try_send(event).is_err() {
            tracing::warn!("[webui-service] provider telemetry Hook queue is saturated or closed");
        }
    }
}

impl Drop for ProviderTelemetryHookGuard {
    /// Emits one synthetic cancelled terminal event when the consumer future is aborted or dropped.
    /// 当消费 Future 被中止或丢弃时发出一条合成取消终态事件。
    fn drop(&mut self) {
        if let Some(event) = self.take_cancelled_event()
            && self.sender.try_send(event).is_err()
        {
            tracing::warn!(
                "[webui-service] provider cancellation telemetry could not enter the Hook queue"
            );
        }
    }
}

/// Dispatches one Vulcan Plugin Hook event inside a stream-local ordered worker.
/// 在流局部有序 Worker 内调度一条 Vulcan 插件 Hook 事件。
///
/// # Parameters
/// 参数
/// `state` resolves the current plugin runtime and `event` contains the versioned safe payload.
/// `state` 解析当前插件运行时，`event` 包含版本化安全载荷。
///
/// # Returns
/// 返回值
/// Returns no value after the Hook finishes; unavailable runtime and errors remain diagnostic-only.
/// Hook 完成后不返回值；运行时不可用与错误仍仅用于诊断。
async fn dispatch_provider_request_telemetry_hook(
    state: &SharedServiceBridgeState,
    mut event: ProviderRequestTelemetryEvent,
) {
    // pluginRuntime is cloned before spawning so the service state lock is not retained.
    // pluginRuntime 在派生任务前克隆，避免保留服务状态锁。
    let Ok(plugin_runtime) = service_vulcan_plugin_runtime(state) else {
        return;
    };
    // hookPoint is fixed by the strongly typed telemetry event kind.
    // hookPoint 由强类型遥测事件类型固定决定。
    let hook_point = event.kind.hook_point();
    // projectId is resolved from the retained canonical session projection, never inferred from names or paths.
    // projectId 从保留的规范会话投影解析，绝不根据名称或路径推断。
    let project_id = state.lock().ok().and_then(|guard| {
        guard
            .workspace_snapshot
            .as_ref()
            .and_then(|workspace| owning_project_id_for_session(workspace, event.session_id))
    });
    // payloadObject is guaranteed by the strongly typed telemetry serializer.
    // payloadObject 由强类型遥测序列化器保证为对象。
    let Some(payload_object) = event.payload.as_object_mut() else {
        tracing::warn!("[webui-service] provider telemetry payload is not an object");
        return;
    };
    payload_object.insert(
        String::from("project_id"),
        project_id.clone().map(Value::String).unwrap_or(Value::Null),
    );
    // requestId is guaranteed by the versioned telemetry serializer and becomes the Hook run id.
    // requestId 由版本化遥测序列化器保证存在，并作为 Hook 运行标识。
    let Some(request_id) = event
        .payload
        .get("request_id")
        .and_then(Value::as_str)
        .map(str::to_string)
    else {
        tracing::warn!("[webui-service] provider telemetry payload is missing request_id");
        return;
    };
    // context binds dispatch to the exact session and registered point.
    // context 将调度绑定到精确会话与已注册点位。
    let mut context = HookContext::new(request_id, hook_point);
    context.scope = HookScope::Session;
    context.session_id = Some(event.session_id.to_string());
    context.project_id = project_id;
    // payload is moved into the ordered Effect dispatch without transcript or credential data.
    // payload 在不含转录或凭据数据的前提下移入有序 Effect 调度。
    let payload = event.payload;
    if let Err(error) =
        vulcan_agent_runtime::agent_runtime::vulcan_plugin_hooks::dispatch_vulcan_plugin_hook(
            plugin_runtime,
            context,
            payload,
        )
        .await
    {
        tracing::warn!("[webui-service] provider telemetry hook dispatch failed: {error}");
    }
}

#[cfg(test)]
mod tests {
    //! Regression tests for durable Provider lifecycle ownership.
    //! Provider 生命周期可靠所有权回归测试。

    use super::*;

    /// Builds a typed permit fixture for cancellation handoff tests, without claiming real storage admission.
    /// 构建取消交接测试用强类型许可夹具，不声称经过真实存储准入。
    /// Returns a session permit when `session_owned` is true, otherwise an explicitly excluded permit.
    /// `session_owned`为真时返回会话许可，否则返回明确排除的许可。
    fn preparation_permit(
        session_owned: bool,
    ) -> vulcan_agent_runtime::agent_runtime::events::PreparedPhysicalApi {
        use vulcan_agent_runtime::agent_runtime::events::PreparedPhysicalApi;
        // All transport fields are inert: these tests exercise only the consumer's cancellation handoff.
        // 全部传输字段均不执行：这些测试只检查消费方的取消责任交接。
        let exact_request = vulcan_ai_contract::AiExecutionRequest {
            operation: vulcan_ai_contract::AiOperation::ConversationRespond,
            model_id: "test-model".into(),
            cache_scope_id: None,
            continuation: None,
            messages: Vec::new(),
            tools: Vec::new(),
            native_tools: Vec::new(),
            tool_choice: Default::default(),
            parallel_tool_calls: Default::default(),
            structured_output: Default::default(),
            stop_sequences: Vec::new(),
            prompt_cache: Default::default(),
            reasoning_effort: None,
            reasoning_output: Default::default(),
            model_parameters: Default::default(),
            resources: Vec::new(),
            output_token_limit: None,
        };
        // Snapshot values do not authorize the permit; the explicit enum variant selects ownership.
        // 快照值不授予许可；显式枚举分支选择归属。
        let request_snapshot = vulcan_core::types::ProviderRequestSnapshot {
            provider_id: "test-provider".into(),
            model_id: "test-model".into(),
            system_prompt: String::new(),
            prompt_layers: Default::default(),
            prompt: String::new(),
            prompt_in_history: true,
            history: Vec::new(),
            temperature: None,
            max_turns: 1,
            tool_schemas: Vec::new(),
            additional_params: None,
            request_headers: None,
            continuation: Default::default(),
        };
        if !session_owned {
            return PreparedPhysicalApi::ExcludedNonSession {
                exact_request,
                controller_config: Default::default(),
                controller_snapshot: Default::default(),
                request_snapshot,
                context_budget: None,
            };
        }
        PreparedPhysicalApi::SessionV1 {
            exact_request,
            controller_config: Default::default(),
            controller_snapshot: Default::default(),
            request_snapshot,
            context_budget: None,
            core_start: vulcan_session_core::v1::document::PreparedApiStart {
                api_call_id: vulcan_session_core::v1::ApiCallId::try_new(1).unwrap(),
                started_seq: vulcan_session_core::v1::Seq::try_new(1).unwrap(),
                projection: vulcan_session_core::v1::ProviderProjectionProof {
                    segment_generation: 1,
                    document_revision: 1,
                    retained_from_seq: vulcan_session_core::v1::Seq::try_new(1).unwrap(),
                    through_seq: vulcan_session_core::v1::Seq::try_new(1).unwrap(),
                    fingerprint_sha256: "test-proof".into(),
                },
                continuation_action_id: None,
            },
            provider_api_trace_store: None,
        }
    }

    /// A committed permit owns cancellation even when its receiver disappears before a Started echo.
    /// 已提交许可即使在 Started 回送前失去接收端，也必须持有取消责任。
    #[tokio::test(flavor = "current_thread")]
    async fn committed_preparation_arms_cancellation_before_dropped_receiver() {
        use vulcan_agent_runtime::agent_runtime::events::PreparedPhysicalApi;
        use vulcan_agent_runtime::agent_runtime::provider_request_telemetry::ProviderRequestTelemetryEventKind;
        // This service fixture exercises the real guard with the same oneshot primitive as the private barrier.
        // 此服务夹具检查真实守卫，使用与私有屏障相同的一次性通道原语。
        let session_id = uuid::Uuid::new_v4();
        let state = std::sync::Arc::new(std::sync::Mutex::new(
            crate::tests::test_support::test_service_state_with_parent_session(session_id),
        ));
        let mut guard = ProviderTelemetryHookGuard::new_with_persistence(state, None);
        let started = ProviderRequestTelemetryEvent::detached(
            ProviderRequestTelemetryEventKind::Started,
            session_id,
            serde_json::json!({"request_id":"admitted-request", "started_at_ms":1000}),
            None,
        );
        // No failed or non-session outcome may create a synthetic session cancellation.
        // 任何失败或非会话结果都不能创建合成的会话取消。
        for result in [
            Err("storage admission failed".into()),
            Ok(PreparedPhysicalApi::RequestConfigurationChanged),
            Ok(PreparedPhysicalApi::ContextCompactionRequired {
                reason: "final budget requires compaction".into(),
            }),
            Ok(preparation_permit(false)),
        ] {
            guard.observe_preparation_result(&started, &result);
            assert!(guard.take_cancelled_event().is_none());
        }
        let (completion, receiver) = tokio::sync::oneshot::channel();
        drop(receiver);
        let result = Ok(preparation_permit(true));
        guard.observe_preparation_result(&started, &result);
        assert!(completion.send(result).is_err());
        let cancelled = guard
            .take_cancelled_event()
            .expect("committed request remains owned");
        assert_eq!(cancelled.payload["request_id"], "admitted-request");
        assert_eq!(cancelled.payload["status"], "cancelled");
        assert!(guard.take_cancelled_event().is_none());
    }

    /// First-token evidence remains identity-bound, and a completed attempt cannot cancel twice.
    /// 首 Token 证据保持身份绑定，已完成尝试不能重复取消。
    #[tokio::test(flavor = "current_thread")]
    async fn admitted_cancellation_preserves_first_token_and_terminal_identity() {
        use vulcan_agent_runtime::agent_runtime::provider_request_telemetry::ProviderRequestTelemetryEventKind;
        // The guard receives the same ordered event shapes used by detached and compaction consumers.
        // 守卫接收与子代理及压缩消费方相同的有序事件形态。
        let session_id = uuid::Uuid::new_v4();
        let state = std::sync::Arc::new(std::sync::Mutex::new(
            crate::tests::test_support::test_service_state_with_parent_session(session_id),
        ));
        let mut guard = ProviderTelemetryHookGuard::new_with_persistence(state, None);
        let started = ProviderRequestTelemetryEvent::detached(
            ProviderRequestTelemetryEventKind::Started,
            session_id,
            serde_json::json!({"request_id":"admitted-request", "started_at_ms":1000}),
            None,
        );
        let mut first_token = ProviderRequestTelemetryEvent::detached(
            ProviderRequestTelemetryEventKind::FirstToken,
            session_id,
            serde_json::json!({"request_id":"foreign-request", "first_token_at_ms":1200, "time_to_first_token_ms":200}),
            None,
        );
        guard.observe_preparation_result(&started, &Ok(preparation_permit(true)));
        guard.observe_durable_boundary(&first_token);
        let cancelled = guard.take_cancelled_event().unwrap();
        assert_eq!(cancelled.payload["semantic_output_observed"], false);
        guard.observe_preparation_result(&started, &Ok(preparation_permit(true)));
        first_token.payload["request_id"] = serde_json::json!("admitted-request");
        guard.observe_durable_boundary(&first_token);
        let cancelled = guard.take_cancelled_event().unwrap();
        assert_eq!(cancelled.payload["semantic_output_observed"], true);
        assert_eq!(cancelled.payload["first_token_at_ms"], 1200);
        assert_eq!(cancelled.payload["replay_safe"], false);
        guard.observe_preparation_result(&started, &Ok(preparation_permit(true)));
        guard.observe_durable_boundary(&cancelled);
        assert!(guard.take_cancelled_event().is_none());
    }

    /// Verifies best-effort Hook forwarding cannot arm cancellation before Started is durable.
    /// 验证 Started 可靠持久化前，尽力 Hook 转发不能激活取消跟踪。
    #[tokio::test(flavor = "current_thread")]
    async fn forwarded_started_event_does_not_arm_cancellation_without_durable_boundary() {
        // sessionId owns the isolated service-state fixture and telemetry event.
        // sessionId 持有隔离的服务状态夹具与遥测事件。
        let session_id = uuid::Uuid::new_v4();
        // state provides the production-shaped Hook worker with a valid retained session.
        // state 为生产形态 Hook Worker 提供有效保留会话。
        let state = std::sync::Arc::new(std::sync::Mutex::new(
            crate::tests::test_support::test_service_state_with_parent_session(session_id),
        ));
        // guard owns an empty cancellation tracker and a best-effort forwarding queue.
        // guard 持有空取消跟踪器与尽力转发队列。
        let mut guard = ProviderTelemetryHookGuard::new_with_persistence(state, None);
        // started is the exact event whose Hook copy may race ahead of durable accounting.
        // started 是 Hook 副本可能早于可靠记账的精确事件。
        let started = ProviderRequestTelemetryEvent::detached(
            vulcan_agent_runtime::agent_runtime::provider_request_telemetry::ProviderRequestTelemetryEventKind::Started,
            session_id,
            serde_json::json!({
                "request_id": "provider-call-1",
                "started_at_ms": 1000,
                "semantic_output_observed": false
            }),
            None,
        );

        guard.forward(started.clone());

        assert!(guard.take_cancelled_event().is_none());

        guard.observe_durable_boundary(&started);
        // cancelled proves the same event becomes active only through the durable observer.
        // cancelled 证明同一事件仅通过可靠观察器进入活动状态。
        let cancelled = guard
            .take_cancelled_event()
            .expect("durable Started boundary should arm cancellation");
        assert_eq!(
            cancelled
                .payload
                .get("request_id")
                .and_then(serde_json::Value::as_str),
            Some("provider-call-1")
        );
        assert_eq!(
            cancelled
                .payload
                .get("status")
                .and_then(serde_json::Value::as_str),
            Some("cancelled")
        );
    }
}
