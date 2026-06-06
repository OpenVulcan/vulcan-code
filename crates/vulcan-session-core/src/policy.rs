use crate::model::{RootSessionId, SessionNodeId};
use serde::{Deserialize, Serialize};

/// Execution scope used to route shared runtime behavior.
/// 用于路由共享运行时行为的执行范围。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ExecutionScope {
    /// Normal root main session execution.
    /// 普通根主会话执行。
    MainSession {
        /// Main session identifier.
        /// 主会话标识符。
        session_id: RootSessionId,
    },
    /// File-backed task session execution.
    /// 文件态子代理会话执行。
    TaskSession {
        /// Root main session identifier.
        /// 根主会话标识符。
        root_session_id: RootSessionId,
        /// Task session identifier.
        /// 子代理会话标识符。
        task_session_id: SessionNodeId,
    },
    /// Context compaction execution for one node.
    /// 单个节点的上下文压缩执行。
    ContextCompaction {
        /// Compacted session node identifier.
        /// 被压缩的会话节点标识符。
        node_id: SessionNodeId,
    },
}

/// Runtime feature policy for one execution scope.
/// 单个执行范围的运行时能力策略。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionRuntimePolicy {
    /// Whether conversation hooks are enabled.
    /// 是否启用会话 hook。
    pub hooks_enabled: bool,
    /// Whether VMM memory behavior is enabled.
    /// 是否启用 VMM 记忆行为。
    pub vmm_enabled: bool,
    /// Whether memory recall is enabled.
    /// 是否启用记忆召回。
    pub memory_recall_enabled: bool,
    /// Whether memory writeback is enabled.
    /// 是否启用记忆写回。
    pub memory_writeback_enabled: bool,
    /// Whether workspace search projection is enabled.
    /// 是否启用工作区搜索投影。
    pub search_projection_enabled: bool,
    /// Whether normal session database projection is enabled.
    /// 是否启用普通会话数据库投影。
    pub database_projection_enabled: bool,
}

/// Optional runtime feature overrides requested before final policy resolution.
/// 最终策略解析前请求的可选运行时能力覆盖。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionRuntimePolicyRequest {
    /// Optional conversation-hook toggle.
    /// 可选的会话 hook 开关。
    #[serde(
        default,
        alias = "hooks_enabled",
        skip_serializing_if = "Option::is_none"
    )]
    pub hooks_enabled: Option<bool>,
    /// Optional VMM toggle.
    /// 可选的 VMM 开关。
    #[serde(
        default,
        alias = "vmm_enabled",
        skip_serializing_if = "Option::is_none"
    )]
    pub vmm_enabled: Option<bool>,
    /// Optional memory-recall toggle.
    /// 可选的记忆召回开关。
    #[serde(
        default,
        alias = "memory_recall_enabled",
        skip_serializing_if = "Option::is_none"
    )]
    pub memory_recall_enabled: Option<bool>,
    /// Optional memory-writeback toggle.
    /// 可选的记忆写回开关。
    #[serde(
        default,
        alias = "memory_writeback_enabled",
        skip_serializing_if = "Option::is_none"
    )]
    pub memory_writeback_enabled: Option<bool>,
    /// Optional workspace-search projection toggle.
    /// 可选的工作区搜索投影开关。
    #[serde(
        default,
        alias = "search_projection_enabled",
        skip_serializing_if = "Option::is_none"
    )]
    pub search_projection_enabled: Option<bool>,
    /// Optional normal database projection toggle.
    /// 可选的普通数据库投影开关。
    #[serde(
        default,
        alias = "database_projection_enabled",
        skip_serializing_if = "Option::is_none"
    )]
    pub database_projection_enabled: Option<bool>,
}

impl ExecutionRuntimePolicyRequest {
    /// Returns whether the request carries any explicit runtime override.
    /// 返回该请求是否携带任何显式运行时覆盖。
    pub const fn has_override(self) -> bool {
        self.hooks_enabled.is_some()
            || self.vmm_enabled.is_some()
            || self.memory_recall_enabled.is_some()
            || self.memory_writeback_enabled.is_some()
            || self.search_projection_enabled.is_some()
            || self.database_projection_enabled.is_some()
    }
}

impl ExecutionRuntimePolicy {
    /// Builds the normal main-session runtime policy.
    /// 构建普通主会话运行时策略。
    pub const fn main_session() -> Self {
        Self {
            hooks_enabled: true,
            vmm_enabled: true,
            memory_recall_enabled: true,
            memory_writeback_enabled: true,
            search_projection_enabled: true,
            database_projection_enabled: true,
        }
    }

    /// Builds the main-session policy for internal delegate-result continuations.
    /// 构建主会话内部委派结果回流使用的运行时策略。
    pub const fn main_session_internal_continuation() -> Self {
        Self {
            hooks_enabled: false,
            vmm_enabled: true,
            memory_recall_enabled: false,
            memory_writeback_enabled: false,
            search_projection_enabled: true,
            database_projection_enabled: true,
        }
    }

    /// Builds the isolated task-session runtime policy.
    /// 构建隔离的子代理会话运行时策略。
    pub const fn task_session() -> Self {
        Self {
            hooks_enabled: false,
            vmm_enabled: false,
            memory_recall_enabled: false,
            memory_writeback_enabled: false,
            search_projection_enabled: false,
            database_projection_enabled: false,
        }
    }

    /// Builds the isolated context-compaction runtime policy.
    /// 构建隔离的上下文压缩运行时策略。
    pub const fn context_compaction() -> Self {
        Self::task_session()
    }
}
