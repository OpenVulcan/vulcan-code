use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::Arc;
use tokio::time;
use vulcan_context_injection::InjectionInsert;

use crate::registry::HookEntry;
use crate::{
    HookContext, HookDecision, HookDiagnostic, HookDiagnosticSeverity, HookError, HookEvent,
    HookEventKind, HookEventListener, HookFailure, HookFailurePolicy, HookHandler, HookId,
    HookInvocation, HookListenerId, HookPayload, HookPointDescriptor, HookRegistration,
    HookRegistry, HookResult, SystemPromptHookBlock,
};

/// Request passed to the dispatcher for one hook point.
/// 传递给调度器的一次 hook 点位请求。
#[derive(Debug, Clone)]
pub struct HookDispatchRequest {
    /// Common hook context.
    /// 通用 hook 上下文。
    pub context: HookContext,
    /// Initial payload value.
    /// 初始 payload 值。
    pub payload: HookPayload,
}

impl HookDispatchRequest {
    /// Builds a dispatch request from context and payload.
    /// 根据上下文和 payload 构建调度请求。
    pub fn new(context: HookContext, payload: HookPayload) -> Self {
        Self { context, payload }
    }
}

/// Aggregated result returned by dispatching one hook point.
/// 调度单个 hook 点位后返回的聚合结果。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HookDispatchReport {
    /// Final payload after transform hooks.
    /// transform hook 处理后的最终 payload。
    pub payload: HookPayload,
    /// Effective decision after decision hooks.
    /// decision hook 处理后的有效决策。
    pub decision: HookDecision,
    /// Context injections collected from handlers.
    /// 从处理器收集到的上下文注入操作。
    #[serde(default)]
    pub injections: Vec<InjectionInsert>,
    /// System-prompt blocks collected from handlers.
    /// 从处理器收集到的系统提示词块。
    #[serde(default)]
    pub system_prompt_blocks: Vec<SystemPromptHookBlock>,
    /// Diagnostics collected from handlers and dispatcher failures.
    /// 从处理器和调度失败中收集的诊断信息。
    #[serde(default)]
    pub diagnostics: Vec<HookDiagnostic>,
    /// Hook-specific metadata collected from successful handlers and grouped by key.
    /// 从成功处理器收集并按键分组的 hook 特定元数据。
    #[serde(default)]
    pub metadata: BTreeMap<String, Vec<Value>>,
    /// Handler ids that completed successfully.
    /// 成功完成的处理器标识。
    #[serde(default)]
    pub completed_hooks: Vec<HookId>,
    /// Handler failures collected during dispatch.
    /// 调度过程中收集到的处理器失败信息。
    #[serde(default)]
    pub failures: Vec<HookFailure>,
    /// True when a queueable failure was recorded.
    /// 当记录了可入队失败时为 true。
    pub has_queueable_failure: bool,
}

impl HookDispatchReport {
    /// Builds a public empty report initialized from the provided payload.
    /// 根据给定 payload 构建一个公开的空报告。
    pub fn empty(payload: HookPayload) -> Self {
        Self::new(payload)
    }

    /// Builds a report initialized from the starting payload.
    /// 根据起始 payload 构建报告。
    fn new(payload: HookPayload) -> Self {
        Self {
            payload,
            decision: HookDecision::Continue,
            injections: Vec::new(),
            system_prompt_blocks: Vec::new(),
            diagnostics: Vec::new(),
            metadata: BTreeMap::new(),
            completed_hooks: Vec::new(),
            failures: Vec::new(),
            has_queueable_failure: false,
        }
    }

    /// Applies one hook result to the accumulated report.
    /// 将单个 hook 结果应用到聚合报告。
    fn apply_result(&mut self, hook_id: &str, result: HookResult) {
        if let Some(payload) = result.payload {
            self.payload = payload;
        }
        if let Some(decision) = result.decision {
            self.decision = decision;
        }
        self.injections.extend(result.injections);
        self.system_prompt_blocks
            .extend(result.system_prompt_blocks);
        for (key, value) in result.metadata {
            self.metadata.entry(key).or_default().push(value);
        }
        self.diagnostics
            .extend(result.diagnostics.into_iter().map(|mut diagnostic| {
                if diagnostic.hook_id.is_none() {
                    diagnostic.hook_id = Some(hook_id.to_owned());
                }
                diagnostic
            }));
    }

    /// Records a hook failure and updates queue state when needed.
    /// 记录 hook 失败，并在需要时更新入队状态。
    fn record_failure(&mut self, failure: HookFailure) {
        if failure.policy == HookFailurePolicy::Queue {
            self.has_queueable_failure = true;
        }
        self.diagnostics.push(HookDiagnostic {
            severity: HookDiagnosticSeverity::Error,
            message: failure.message.clone(),
            hook_id: Some(failure.hook_id.clone()),
        });
        self.failures.push(failure);
    }
}

/// Hook dispatcher that runs registered handlers.
/// 运行已注册处理器的 hook 调度器。
#[derive(Clone, Default)]
pub struct HookDispatcher {
    /// Registry used for handler lookup.
    /// 用于处理器查询的注册表。
    registry: HookRegistry,
    /// Lifecycle event listeners notified by registration and dispatch.
    /// 注册与调度时通知的生命周期事件监听器。
    listeners: BTreeMap<HookListenerId, Arc<dyn HookEventListener>>,
    /// Next generated listener sequence.
    /// 下一个生成的监听器序号。
    next_listener_sequence: u64,
}

impl HookDispatcher {
    /// Builds a dispatcher from a registry.
    /// 根据注册表构建调度器。
    pub fn new(registry: HookRegistry) -> Self {
        Self {
            registry,
            listeners: BTreeMap::new(),
            next_listener_sequence: 0,
        }
    }

    /// Returns the dispatcher registry.
    /// 返回调度器注册表。
    pub fn registry(&self) -> &HookRegistry {
        &self.registry
    }

    /// Returns a mutable reference to the dispatcher registry.
    /// 返回调度器注册表的可变引用。
    pub fn registry_mut(&mut self) -> &mut HookRegistry {
        &mut self.registry
    }

    /// Registers or replaces one hook point descriptor.
    /// 注册或替换一个 hook 点位描述。
    pub fn register_point(&mut self, descriptor: HookPointDescriptor) -> Result<(), HookError> {
        self.registry.register_point(descriptor)
    }

    /// Registers or replaces many hook point descriptors.
    /// 注册或替换多个 hook 点位描述。
    pub fn register_points(
        &mut self,
        descriptors: impl IntoIterator<Item = HookPointDescriptor>,
    ) -> Result<(), HookError> {
        self.registry.register_points(descriptors)
    }

    /// Adds one lifecycle event listener.
    /// 添加一个生命周期事件监听器。
    pub fn add_listener(&mut self, listener: Arc<dyn HookEventListener>) -> HookListenerId {
        self.next_listener_sequence = self.next_listener_sequence.saturating_add(1);
        let listener_id = format!("listener-{}", self.next_listener_sequence);
        self.add_listener_with_id(listener_id.clone(), listener)
            .expect("generated listener id should be valid");
        listener_id
    }

    /// Adds or replaces one lifecycle event listener with a stable id.
    /// 使用稳定标识添加或替换一个生命周期事件监听器。
    pub fn add_listener_with_id(
        &mut self,
        listener_id: impl Into<HookListenerId>,
        listener: Arc<dyn HookEventListener>,
    ) -> Result<(), HookError> {
        let listener_id = listener_id.into();
        if listener_id.trim().is_empty() {
            return Err(HookError::invalid_registration(
                "hook listener id cannot be empty",
            ));
        }
        self.listeners.insert(listener_id, listener);
        Ok(())
    }

    /// Removes one lifecycle event listener by id.
    /// 按标识移除一个生命周期事件监听器。
    pub fn remove_listener(&mut self, listener_id: &str) -> bool {
        self.listeners.remove(listener_id).is_some()
    }

    /// Registers one hook handler and emits a registration event.
    /// 注册一个 hook 处理器并发出注册事件。
    pub fn register(
        &mut self,
        registration: HookRegistration,
        handler: Arc<dyn HookHandler>,
    ) -> Result<(), HookError> {
        let point = registration.point.clone();
        let hook_id = registration.id.clone();
        self.registry.register(registration, handler)?;
        self.emit_event(HookEvent::new(HookEventKind::Registered, point).with_hook_id(hook_id));
        Ok(())
    }

    /// Unregisters matching hook handlers and emits unregistration events.
    /// 注销匹配的 hook 处理器并发出注销事件。
    pub fn unregister(&mut self, hook_id: &str, point: Option<&str>) -> usize {
        let snapshots = self.registry.registration_snapshots(point);
        let removed = self.registry.unregister(hook_id, point);
        if removed == 0 {
            return 0;
        }
        for snapshot in snapshots
            .into_iter()
            .filter(|snapshot| snapshot.registration.id == hook_id)
        {
            self.emit_event(
                HookEvent::new(HookEventKind::Unregistered, snapshot.registration.point)
                    .with_hook_id(snapshot.registration.id),
            );
        }
        removed
    }

    /// Dispatches one hook point and returns the aggregated report.
    /// 调度单个 hook 点位并返回聚合报告。
    pub async fn dispatch(
        &self,
        request: HookDispatchRequest,
    ) -> Result<HookDispatchReport, HookError> {
        let entries = self.registry.entries_for(&request.context.point);
        let mut report = HookDispatchReport::new(request.payload);
        self.emit_event(
            HookEvent::new(
                HookEventKind::DispatchStarted,
                request.context.point.clone(),
            )
            .with_run_id(request.context.run_id.clone()),
        );

        for entry in entries {
            let hook_id = entry.registration.id.clone();
            self.emit_event(
                HookEvent::new(HookEventKind::HandlerStarted, request.context.point.clone())
                    .with_run_id(request.context.run_id.clone())
                    .with_hook_id(hook_id.clone()),
            );
            let invocation = HookInvocation {
                registration: entry.registration.clone(),
                context: request.context.clone(),
                payload: report.payload.clone(),
            };
            let result = run_entry(entry.clone(), invocation).await;
            match result {
                Ok(result) => {
                    report.apply_result(&hook_id, result);
                    report.completed_hooks.push(hook_id.clone());
                    self.emit_event(
                        HookEvent::new(
                            HookEventKind::HandlerCompleted,
                            request.context.point.clone(),
                        )
                        .with_run_id(request.context.run_id.clone())
                        .with_hook_id(hook_id),
                    );
                    if report.decision != HookDecision::Continue {
                        break;
                    }
                }
                Err(error) => {
                    let failure = HookFailure {
                        hook_id: hook_id.clone(),
                        policy: entry.registration.failure_policy,
                        message: error.to_string(),
                    };
                    let should_stop = failure.policy == HookFailurePolicy::Stop;
                    self.emit_event(
                        HookEvent::new(HookEventKind::HandlerFailed, request.context.point.clone())
                            .with_run_id(request.context.run_id.clone())
                            .with_hook_id(hook_id)
                            .with_message(failure.message.clone()),
                    );
                    report.record_failure(failure);
                    if should_stop {
                        return Err(HookError::dispatch_stopped(report.failures));
                    }
                }
            }
        }

        self.emit_event(
            HookEvent::new(HookEventKind::DispatchCompleted, request.context.point)
                .with_run_id(request.context.run_id),
        );
        Ok(report)
    }

    /// Emits one lifecycle event to every registered listener.
    /// 向所有已注册监听器发出一个生命周期事件。
    fn emit_event(&self, event: HookEvent) {
        for listener in self.listeners.values() {
            listener.on_hook_event(event.clone());
        }
    }
}

/// Runs one hook entry with its configured timeout.
/// 按配置超时时间运行单个 hook 条目。
async fn run_entry(entry: HookEntry, invocation: HookInvocation) -> Result<HookResult, HookError> {
    let future = entry.handler.handle(invocation);
    if let Some(duration) = entry.registration.timeout_duration() {
        time::timeout(duration, future)
            .await
            .map_err(|_| HookError::timeout(entry.registration.id))?
    } else {
        future.await
    }
}
