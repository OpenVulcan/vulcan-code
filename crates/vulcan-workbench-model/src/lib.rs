//! Shared workbench-facing domain models reused across GUI, persistence, and runtime layers.
//! 在 GUI、持久化与运行时层之间复用的共享工作台领域模型。

mod activity;
mod project;
mod session;
mod turn_stats;

pub use activity::{ActivityEntry, ActivityLevel};
pub use project::TaskProjectSummary;
pub use session::{SessionId, SessionRunPhase, SessionState, SessionSummary, SessionTitleSource};
pub use turn_stats::{SessionTurnStats, SessionTurnStatsStatus};
pub use vulcan_context::{
    SessionCompactionEntry, SessionMessageEntry, SessionMessageKind, SessionMessageRecordKind,
};
