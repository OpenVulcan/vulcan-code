//! Shared hook registration, dispatch, context, and result primitives for Vulcan extensions.
//! Vulcan 扩展使用的共享 hook 注册、调度、上下文和结果原语。

mod dispatch;
mod error;
mod events;
mod handler;
mod registry;
#[cfg(test)]
mod tests;
mod types;

use std::time::{SystemTime, UNIX_EPOCH};

pub use dispatch::{HookDispatchReport, HookDispatchRequest, HookDispatcher};
pub use error::{HookError, HookFailure};
pub use events::{FnHookEventListener, HookEvent, HookEventKind, HookEventListener};
pub use handler::{FnHookHandler, HookHandler, HookInvocation};
pub use registry::HookRegistry;
pub use types::{
    HookActor, HookCapability, HookCapabilitySet, HookContext, HookDecision, HookDiagnostic,
    HookDiagnosticSeverity, HookFailurePolicy, HookId, HookListenerId, HookMode, HookPayload,
    HookPoint, HookPointDescriptor, HookProjectId, HookRegistration, HookRegistrationSnapshot,
    HookResult, HookScope, HookSessionId, SystemPromptHookBaselineKind,
    SystemPromptHookBaselineProvenance, SystemPromptHookBlock, SystemPromptHookPhase,
};

/// Returns the current Unix timestamp in milliseconds.
/// 返回当前 Unix 毫秒时间戳。
fn current_unix_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}
