//! Unified V1 session facts and shared runtime policy models.
//! 统一 V1 会话事实与共享运行时策略模型。

/// Shared child-agent hard policy facts.
/// 共享智能体硬策略事实。
pub mod child_agent_policy;
/// Shared session configuration snapshots.
/// 共享会话配置快照。
pub mod config;
/// Shared session identity and tree models.
/// 共享会话身份与树模型。
pub mod model;
/// Shared execution scope and runtime policy models.
/// 共享执行范围与运行时策略模型。
pub mod policy;
/// Shared single-source active-turn runtime kernel.
/// 共享单一事实活动回合运行时内核。
pub mod runtime_kernel;
/// Shared selector matching helpers for session-scoped routing.
/// 共享会话级路由使用的选择器匹配辅助逻辑。
pub mod selector;
/// Shared child-agent registry models.
/// 共享子代理注册表模型。
pub mod subagent_registry;
/// Shared invocation and resolved configuration for durable child recovery.
/// 用于可靠子恢复的共享调用与已解析配置。
pub mod task_config;
/// Shared active-turn input queue and mailbox models.
/// 共享活动回合输入队列与 mailbox 模型。
pub mod turn_input_queue;

/// V1 unified session fact model (strongly typed main event stream, identities,
/// statistics, runtime lock, and metadata contract).
/// V1 统一会话事实模型(强类型主事件流、身份、统计、运行时锁定与元数据契约)。
pub mod v1;

pub use child_agent_policy::{
    CHILD_AGENT_FORBIDDEN_TOOL_IDS, child_agent_forbidden_tool_ids, child_agent_tool_is_forbidden,
    sanitize_child_agent_tool_exposure, sanitize_child_agent_tool_policy,
};
pub use config::{
    CapabilityPolicyMode, CapabilityPolicySnapshot, ContextCompactionPolicyRequest,
    ContextCompactionPolicySnapshot, PromptSkillSnapshot, ProviderModelSnapshot,
    SessionEffectiveConfig, SessionRequestedConfig, ToolExposureSnapshot,
    apply_prompt_skill_capability_policy, apply_tool_capability_policy,
    capability_policy_from_mode_or_selector_lists, capability_policy_from_selector_lists,
    normalize_capability_policy_snapshot, parse_capability_policy_mode_key,
};
pub use model::{ParentSessionRef, RootSessionId, SessionNodeId, SessionNodeKind, SessionTreePath};
pub use policy::{ExecutionRuntimePolicy, ExecutionRuntimePolicyRequest, ExecutionScope};
pub use runtime_kernel::{
    ActiveTurn, ActiveTurnKind, SessionRuntimeKernel, SessionRuntimeKernelError,
    SessionRuntimeKernelSnapshot, TurnTerminalStatus, WaitScope,
};
pub use selector::{normalize_tool_selector, normalized_tool_selector_set, tool_selector_matches};
pub use subagent_registry::{
    SubagentKind, SubagentRegistry, SubagentRegistryEntry, SubagentRegistryError, SubagentStatus,
};
pub use task_config::{TaskInvocationConfig, TaskSessionResolvedConfig};
pub use turn_input_queue::{MailboxDeliveryPhase, TurnInput, TurnInputKind, TurnInputQueue};

#[cfg(test)]
mod tests {
    use super::{ExecutionRuntimePolicy, SessionNodeId, SessionRequestedConfig, SessionTreePath};

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
}
