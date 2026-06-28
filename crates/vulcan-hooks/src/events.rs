use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

use crate::{HookId, HookPoint, current_unix_millis};

/// Describes observable lifecycle events emitted by the hook system.
/// 描述 hook 系统发出的可观测生命周期事件。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HookEventKind {
    /// A hook handler was registered.
    /// 一个 hook 处理器已注册。
    Registered,
    /// A hook handler was unregistered.
    /// 一个 hook 处理器已注销。
    Unregistered,
    /// Dispatch for one hook point has started.
    /// 单个 hook 点位的调度已开始。
    DispatchStarted,
    /// One hook handler is about to run.
    /// 单个 hook 处理器即将运行。
    HandlerStarted,
    /// One hook handler completed successfully.
    /// 单个 hook 处理器已成功完成。
    HandlerCompleted,
    /// One hook handler failed or timed out.
    /// 单个 hook 处理器失败或超时。
    HandlerFailed,
    /// Dispatch for one hook point has completed.
    /// 单个 hook 点位的调度已完成。
    DispatchCompleted,
}

/// Observable lifecycle event emitted by hook registration and dispatch.
/// hook 注册与调度发出的可观测生命周期事件。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HookEvent {
    /// Lifecycle event kind.
    /// 生命周期事件类型。
    pub kind: HookEventKind,
    /// Unix timestamp in milliseconds when the event was emitted.
    /// 事件发出时的 Unix 毫秒时间戳。
    pub timestamp_unix_ms: u128,
    /// Hook point associated with the event.
    /// 与事件关联的 hook 点位。
    pub point: HookPoint,
    /// Optional dispatch run id.
    /// 可选调度运行标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    /// Optional hook handler id.
    /// 可选 hook 处理器标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hook_id: Option<HookId>,
    /// Optional human-readable event message.
    /// 可选人类可读事件消息。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// Additional event-specific metadata.
    /// 事件特有的额外元数据。
    #[serde(default)]
    pub metadata: BTreeMap<String, Value>,
}

impl HookEvent {
    /// Builds a hook lifecycle event.
    /// 构建一个 hook 生命周期事件。
    pub fn new(kind: HookEventKind, point: impl Into<HookPoint>) -> Self {
        Self {
            kind,
            timestamp_unix_ms: current_unix_millis(),
            point: point.into(),
            run_id: None,
            hook_id: None,
            message: None,
            metadata: BTreeMap::new(),
        }
    }

    /// Attaches a dispatch run id to the event.
    /// 为事件附加调度运行标识。
    pub fn with_run_id(mut self, run_id: impl Into<String>) -> Self {
        self.run_id = Some(run_id.into());
        self
    }

    /// Attaches a hook handler id to the event.
    /// 为事件附加 hook 处理器标识。
    pub fn with_hook_id(mut self, hook_id: impl Into<HookId>) -> Self {
        self.hook_id = Some(hook_id.into());
        self
    }

    /// Attaches a human-readable message to the event.
    /// 为事件附加人类可读消息。
    pub fn with_message(mut self, message: impl Into<String>) -> Self {
        self.message = Some(message.into());
        self
    }
}

/// Trait implemented by lifecycle event listeners.
/// 生命周期事件监听器实现的 trait。
pub trait HookEventListener: Send + Sync {
    /// Handles one hook lifecycle event.
    /// 处理一个 hook 生命周期事件。
    fn on_hook_event(&self, event: HookEvent);
}

/// Function-backed hook lifecycle event listener.
/// 基于函数的 hook 生命周期事件监听器。
pub struct FnHookEventListener<F> {
    /// Function invoked for each lifecycle event.
    /// 每个生命周期事件触发时执行的函数。
    function: F,
}

impl<F> FnHookEventListener<F> {
    /// Builds a function-backed lifecycle event listener.
    /// 构建一个基于函数的生命周期事件监听器。
    pub fn new(function: F) -> Self {
        Self { function }
    }
}

impl<F> HookEventListener for FnHookEventListener<F>
where
    F: Send + Sync + Fn(HookEvent),
{
    /// Runs the wrapped function for one lifecycle event.
    /// 为一个生命周期事件运行被包装函数。
    fn on_hook_event(&self, event: HookEvent) {
        (self.function)(event);
    }
}
