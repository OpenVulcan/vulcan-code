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
    /// Shared service state used to close live usage accounting on an aborted stream.
    /// 用于在流任务中止时关闭实时用量统计的共享服务状态。
    state: SharedServiceBridgeState,
    /// Optional persistence runtime used when cancellation happens during task drop.
    /// 任务丢弃期间发生取消时使用的可选持久化运行时。
    controller_runtime: Option<ControllerRuntimeHandle>,
    /// Optional WebUI emitter used to publish the cancellation accounting delta.
    /// 用于发布取消记账增量的可选 WebUI 发射器。
    app: Option<ServiceEventEmitter>,
    /// Protocol tracker that retains only the active request's redacted started payload.
    /// 仅保留活动请求脱敏开始载荷的协议跟踪器。
    cancellation: ProviderRequestTelemetryCancellationTracker,
    /// Bounded stream-local sender whose single worker preserves event order.
    /// 由单个 Worker 保持事件顺序的有界流局部发送器。
    sender: tokio::sync::mpsc::Sender<ProviderRequestTelemetryEvent>,
}

impl ProviderTelemetryHookGuard {
    /// Creates a guard that can persist and project cancellation accounting during Drop.
    /// 创建一个可在 Drop 期间持久化并投影取消记账的守卫。
    ///
    /// # Parameters
    /// 参数
    /// `state` owns the authoritative workspace runtime state.
    /// `state` 持有权威工作区运行时状态。
    /// `controller_runtime` persists a synthetic terminal usage snapshot when task cancellation
    /// bypasses the normal stream-drain path.
    /// `controller_runtime` 在任务取消绕过正常流排空路径时持久化合成终态用量快照。
    /// `app` receives the matching live addon delta when the owning surface is still available.
    /// `app` 在所属界面仍可用时接收对应的实时附属状态增量。
    pub(crate) fn new_with_persistence(
        state: SharedServiceBridgeState,
        controller_runtime: Option<ControllerRuntimeHandle>,
        app: Option<ServiceEventEmitter>,
    ) -> Self {
        let accounting_state = state.clone();
        let native_usage_controller = controller_runtime.clone();
        // sender and receiver form one bounded low-frequency Effect queue per execution stream.
        // sender 与 receiver 为每条执行流构成一个有界低频 Effect 队列。
        let (sender, mut receiver) = tokio::sync::mpsc::channel(64);
        spawn_service_task(async move {
            while let Some(event) = receiver.recv().await {
                if let Some(controller_runtime) = native_usage_controller.as_ref()
                    && let Err(error) =
                        persist_native_provider_usage_attempt(controller_runtime, &event).await
                {
                    eprintln!("[webui-service] native provider usage persistence failed: {error}");
                }
                dispatch_provider_request_telemetry_hook(&state, event).await;
            }
        });
        Self {
            state: accounting_state,
            controller_runtime,
            app,
            cancellation: ProviderRequestTelemetryCancellationTracker::default(),
            sender,
        }
    }

    /// Takes the synthetic cancelled terminal event for normal stream-exit handling.
    /// 取出合成的取消终态事件，供正常流退出路径处理。
    pub(crate) fn take_cancelled_event(&mut self) -> Option<ProviderRequestTelemetryEvent> {
        self.cancellation.take_cancelled_event()
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

    /// Records one Provider lifecycle boundary only after its authoritative session state is durable.
    /// 仅在权威会话状态可靠持久化后记录一条 Provider 生命周期边界。
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
            eprintln!("[webui-service] provider telemetry Hook queue is saturated or closed");
        }
    }
}

impl Drop for ProviderTelemetryHookGuard {
    /// Emits one synthetic cancelled terminal event when the consumer future is aborted or dropped.
    /// 当消费 Future 被中止或丢弃时发出一条合成取消终态事件。
    fn drop(&mut self) {
        if let Some(event) = self.cancellation.take_cancelled_event() {
            match apply_provider_request_usage_telemetry(&self.state, &event) {
                Ok(Some(snapshot)) => {
                    if snapshot.regular_session
                        && let Some(controller_runtime) = self.controller_runtime.clone()
                    {
                        spawn_latest_session_runtime_state_persist_task(
                            self.state.clone(),
                            controller_runtime,
                            event.session_id,
                            Some(String::from("provider_usage_cancelled_drop")),
                        );
                    }
                    if let Some(app) = self.app.as_ref()
                        && let Err(error) =
                            emit_session_addons_changed_delta(app, &self.state, event.session_id)
                    {
                        eprintln!(
                            "[webui-service] provider cancellation usage delta emit failed: {error}"
                        );
                    }
                }
                Ok(None) => {}
                Err(error) => {
                    eprintln!(
                        "[webui-service] provider cancellation usage accounting failed: {error}"
                    );
                }
            }
            if self.sender.try_send(event).is_err() {
                eprintln!(
                    "[webui-service] provider cancellation telemetry could not enter the Hook queue"
                );
            }
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
        eprintln!("[webui-service] provider telemetry payload is not an object");
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
        eprintln!("[webui-service] provider telemetry payload is missing request_id");
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
        eprintln!("[webui-service] provider telemetry hook dispatch failed: {error}");
    }
}

#[cfg(test)]
mod tests {
    //! Regression tests for durable Provider lifecycle ownership.
    //! Provider 生命周期可靠所有权回归测试。

    use super::*;

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
        let mut guard = ProviderTelemetryHookGuard::new_with_persistence(state, None, None);
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
