#![forbid(unsafe_code)]

//! Temporary session-boundary compatibility types for the Vulcan control-flow migration.
//! Vulcan 控制流迁移期间临时保留的会话边界兼容类型。

/// Lifecycle hook adapter used by the temporary boundary runner.
/// 临时边界 runner 使用的生命周期 hook 适配器。
mod adapter;
/// session-boundary thread, turn, event, and visibility contract types.
/// 会话边界的线程、轮次、事件和可见性契约类型。
mod contract;
/// Internal in-memory store retained for boundary runner tests only.
/// 仅为边界 runner 测试保留的内部内存存储。
#[cfg(test)]
pub(crate) mod memory;
/// Minimal session-boundary thread and turn runner.
/// 最小 会话边界线程与轮次运行器。
mod runner;
/// Runtime facade that preserves session boundary event append and hook ordering.
/// 保持 会话边界事件追加与 hook 顺序的运行时门面。
mod runtime;
/// Append-only event and overwrite-only state storage traits.
/// 追加式事件与覆盖式状态存储 trait。
mod storage;
/// session-boundary child-agent metadata and scheduler boundary.
/// 会话边界子代理元数据与调度边界。
mod subagent;
/// Projection from session-boundary events into Vulcan conversation state.
/// 从 会话边界事件投影到 Vulcan 会话状态。
mod vulcan_projection;

pub use adapter::SessionLifecycleHook;
pub use contract::{
    SessionBoundaryEvent, SessionBoundaryEventKind, SessionEventScope, SessionEventVisibility,
    SessionThreadId, SessionTurnId, SessionVisibleRole,
};
pub use runner::{
    SessionLifecycleBeginRequest, SessionLifecycleFinishRequest, SessionLifecycleRunner,
    SessionThreadState, SessionThreadStatus, SessionTurnTerminalStatus,
};
pub use storage::{SessionBoundaryStateStore, SessionBoundaryStorageWrite, SessionBoundaryStore};
pub use subagent::{
    InMemorySubagentMailbox, SubagentMailbox, SubagentMailboxAgentKind, SubagentMailboxEvent,
    SubagentMailboxEventKind, SubagentMailboxStatus,
};
pub use vulcan_projection::{
    BoundaryConversationProjection, boundary_events_to_vulcan_conversation_state,
    session_boundary_event_to_vulcan_record,
};

/// Exact upstream Codex commit used as the latest reference for this migration.
/// 本次迁移作为最新参考使用的 Codex 上游精确提交。
pub const UPSTREAM_CODEX_COMMIT: &str = "e093d819826127be01354e2885c86d7fcbfb2897";
