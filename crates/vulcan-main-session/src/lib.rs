//! Main-session private routing and persistence policy boundaries.
//! 主会话私有路由与持久化策略边界。

/// Main-session projection policy.
/// 主会话投影策略。
pub mod projection;
/// Main-session router contract.
/// 主会话路由契约。
pub mod router;
/// Main-session runtime policy helpers.
/// 主会话运行时策略辅助能力。
pub mod runtime_policy;
/// Main-session storage contract.
/// 主会话存储契约。
pub mod store;
/// Main-session task entry models.
/// 主会话子代理入口模型。
pub mod task_entry;

pub use projection::MainSessionProjectionPolicy;
pub use router::MainSessionRouter;
pub use runtime_policy::MainSessionRuntimePolicy;
pub use store::MainSessionStore;
pub use task_entry::{MainSessionTaskEntry, MainSessionTaskEntryStatus};

#[cfg(test)]
mod tests {
    use super::{
        MainSessionProjectionPolicy, MainSessionRuntimePolicy, MainSessionTaskEntry,
        MainSessionTaskEntryStatus,
    };
    use vulcan_session_core::SessionNodeId;

    /// Verifies that main-session runtime policy keeps durable side effects enabled.
    /// 验证主会话运行时策略会保持持久副作用开启。
    #[test]
    fn main_session_runtime_policy_keeps_durable_side_effects_without_search() {
        let policy = MainSessionRuntimePolicy::default().runtime_policy;

        assert!(policy.hooks_enabled);
        assert!(policy.vmm_enabled);
        assert!(policy.memory_recall_enabled);
        assert!(policy.memory_writeback_enabled);
        assert!(policy.database_projection_enabled);
    }

    /// Verifies that main-session projection defaults to durable state.
    /// 验证主会话投影默认保留持久状态。
    #[test]
    fn main_session_projection_policy_defaults_to_search_disabled_behavior() {
        let policy = MainSessionProjectionPolicy::default();

        assert!(policy.database_projection_enabled);
    }

    /// Verifies that task entries only index child task sessions instead of embedding transcripts.
    /// 验证子代理入口只索引子代理会话而不嵌入转录。
    #[test]
    fn task_entry_indexes_child_task_session() {
        let root_session_id = SessionNodeId::new_v4();
        let task_session_id = SessionNodeId::new_v4();
        let entry = MainSessionTaskEntry::new(
            root_session_id,
            task_session_id,
            "review-1",
            MainSessionTaskEntryStatus::Running,
        );

        assert_eq!(entry.root_session_id, root_session_id);
        assert_eq!(entry.task_session_id, task_session_id);
        assert_eq!(entry.public_task_id, "review-1");
        assert!(entry.result_summary.is_none());
    }
}
