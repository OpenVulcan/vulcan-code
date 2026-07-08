//! Shared session protocol models for main sessions and task sessions.
//! 主会话与子代理会话共享的会话协议模型。

/// Shared child-agent hard policy facts.
/// 共享智能体硬策略事实。
pub mod child_agent_policy;
/// Shared context compaction adapter contracts.
/// 共享上下文压缩适配器契约。
pub mod compactor;
/// Shared session configuration snapshots.
/// 共享会话配置快照。
pub mod config;
/// Shared API context and checkpoint documents.
/// 共享 API 上下文与检查点文档。
pub mod context;
/// Shared append-only session event records.
/// 共享追加式会话事件记录。
pub mod event;
/// Shared runtime execution adapter contracts.
/// 共享运行时执行适配器契约。
pub mod executor;
/// Shared session identity and tree models.
/// 共享会话身份与树模型。
pub mod model;
/// Shared execution scope and runtime policy models.
/// 共享执行范围与运行时策略模型。
pub mod policy;
/// Shared session node routing contracts.
/// 共享会话节点路由契约。
pub mod router;
/// Shared single-source active-turn runtime kernel.
/// 共享单一事实活动回合运行时内核。
pub mod runtime_kernel;
/// Shared segmented-session file IO coordination.
/// 共享分段会话文件 IO 协调。
pub mod segmented_file_io;
/// Shared selector matching helpers for session-scoped routing.
/// 共享会话级路由使用的选择器匹配辅助逻辑。
pub mod selector;
/// Shared session node storage contracts.
/// 共享会话节点存储契约。
pub mod store;
/// Shared child-agent registry models.
/// 共享子代理注册表模型。
pub mod subagent_registry;
/// Shared active-turn input queue and mailbox models.
/// 共享活动回合输入队列与 mailbox 模型。
pub mod turn_input_queue;

pub use child_agent_policy::{
    CHILD_AGENT_FORBIDDEN_TOOL_IDS, child_agent_forbidden_tool_ids, child_agent_tool_is_forbidden,
    sanitize_child_agent_tool_exposure, sanitize_child_agent_tool_policy,
};
pub use compactor::{
    SessionContextCompactionError, SessionContextCompactionOutcome,
    SessionContextCompactionRequest, SessionContextCompactor,
};
pub use config::{
    CapabilityPolicyMode, CapabilityPolicySnapshot, ContextCompactionPolicyRequest,
    ContextCompactionPolicySnapshot, PromptSkillSnapshot, ProviderModelSnapshot,
    SessionEffectiveConfig, SessionRequestedConfig, ToolExposureSnapshot,
    apply_prompt_skill_capability_policy, apply_tool_capability_policy,
    capability_policy_from_mode_or_selector_lists, capability_policy_from_selector_lists,
    normalize_capability_policy_snapshot, parse_capability_policy_mode_key,
};
pub use context::{
    SessionCompactionCheckpoint, SessionContextDocument, SessionContextSnapshot,
    SessionRuntimeCheckpoint,
};
pub use event::{SessionEventKind, SessionEventRecord};
pub use executor::{
    SessionExecutionError, SessionExecutionEvent, SessionExecutionEventKind,
    SessionExecutionOutcome, SessionExecutionRequest, SessionExecutionStatus,
    SessionExecutionStream, SessionNodeExecutor,
};
pub use model::{ParentSessionRef, RootSessionId, SessionNodeId, SessionNodeKind, SessionTreePath};
pub use policy::{ExecutionRuntimePolicy, ExecutionRuntimePolicyRequest, ExecutionScope};
pub use router::{SessionNodeRouter, SessionNodeViewSnapshot, SessionRouteError};
pub use runtime_kernel::{
    ActiveTurn, ActiveTurnKind, SessionRuntimeKernel, SessionRuntimeKernelError,
    SessionRuntimeKernelSnapshot, TurnTerminalStatus, WaitScope,
};
pub use selector::{normalize_tool_selector, normalized_tool_selector_set, tool_selector_matches};
pub use store::{SessionNodeStore, SessionStoreError};
pub use subagent_registry::{
    SessionSegmentSummary, SubagentKind, SubagentRegistry, SubagentRegistryEntry, SubagentStatus,
};
pub use turn_input_queue::{MailboxDeliveryPhase, TurnInput, TurnInputKind, TurnInputQueue};

#[cfg(test)]
mod tests {
    use super::{
        ExecutionRuntimePolicy, SessionEventKind, SessionEventRecord, SessionNodeId,
        SessionRequestedConfig, SessionTreePath,
    };
    use serde_json::json;

    /// Verifies that task-session execution keeps all side-effect policies disabled.
    /// 验证子代理会话执行会关闭全部副作用策略。
    #[test]
    fn task_session_policy_disables_side_effects() {
        let policy = ExecutionRuntimePolicy::task_session();

        assert!(!policy.hooks_enabled);
        assert!(!policy.vmm_enabled);
        assert!(!policy.memory_recall_enabled);
        assert!(!policy.memory_writeback_enabled);
        assert!(!policy.database_projection_enabled);
    }

    /// Verifies that requested configuration inherits parent state by default.
    /// 验证请求配置默认继承父级状态。
    #[test]
    fn requested_config_inherits_parent_by_default() {
        let config = SessionRequestedConfig::default();

        assert!(config.inherit_parent_config);
        assert!(config.subagent_profile_id.is_none());
    }

    /// Verifies that a root path targets the root session.
    /// 验证根路径目标就是根会话。
    #[test]
    fn root_path_targets_root_session() {
        let root_id = SessionNodeId::new_v4();
        let path = SessionTreePath::root(root_id);

        assert_eq!(path.target(), Some(root_id));
    }

    /// Verifies that shared event records can round-trip through JSON.
    /// 验证共享事件记录可以通过 JSON 往返序列化。
    #[test]
    fn event_record_round_trips_through_json() {
        let node_id = SessionNodeId::new_v4();
        let record = SessionEventRecord::new(
            "event-1",
            node_id,
            7,
            SessionEventKind::TaskTerminal,
            123,
            json!({ "summary": "done" }),
        );

        let encoded = serde_json::to_value(&record).expect("event record should serialize");
        let decoded: SessionEventRecord =
            serde_json::from_value(encoded).expect("event record should deserialize");

        assert_eq!(decoded, record);
    }
}
