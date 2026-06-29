//! Core Vulcan subagent orchestration protocol.
//! Vulcan 核心子代理调度协议。

use serde::{Deserialize, Serialize};

/// Stable public name for the child-agent creation tool.
/// 子代理创建工具的稳定公开名称。
pub const SPAWN_AGENT_TOOL: &str = "spawn_agent";

/// Stable public name for the child-agent queue-only message tool.
/// 子代理仅入队消息工具的稳定公开名称。
pub const SEND_MESSAGE_TOOL: &str = "send_message";

/// Stable public name for the child-agent follow-up task tool.
/// 子代理后续任务工具的稳定公开名称。
pub const FOLLOWUP_TASK_TOOL: &str = "followup_task";

/// Stable public name for the child-agent wait tool.
/// 子代理等待工具的稳定公开名称。
pub const WAIT_AGENT_TOOL: &str = "wait_agent";

/// Stable public name for the child-agent listing tool.
/// 子代理列表工具的稳定公开名称。
pub const LIST_AGENTS_TOOL: &str = "list_agents";

/// Stable public name for the child-agent interrupt tool.
/// 子代理中断工具的稳定公开名称。
pub const INTERRUPT_AGENT_TOOL: &str = "interrupt_agent";

/// Canonical host-result kind for one Vulcan child-agent task.
/// Vulcan 子代理任务的规范宿主结果类型。
pub const SUBAGENT_TASK_KIND: &str = "subagent_task";

/// Legacy host-result kind accepted only for reading older records.
/// 仅用于读取旧记录的历史宿主结果类型。
pub const LEGACY_TASK_SESSION_KIND: &str = "task_session";

/// Canonical payload key for the internal child-agent task identifier.
/// 内部子代理任务标识的规范载荷字段。
pub const SUBAGENT_TASK_ID_KEY: &str = "subagent_task_id";

/// Legacy payload key accepted only for reading older records.
/// 仅用于读取旧记录的历史载荷字段。
pub const LEGACY_TASK_SESSION_ID_KEY: &str = "task_session_id";

/// Canonical payload key for stopped child-agent identifiers.
/// 已停止子代理标识列表的规范载荷字段。
pub const STOPPED_SUBAGENT_TASK_IDS_KEY: &str = "stopped_subagent_task_ids";

/// Implemented core subagent tool names exposed to the model.
/// 已实现并暴露给模型的规范核心子代理工具名称。
pub const CORE_SUBAGENT_TOOLS: [&str; 6] = [
    SPAWN_AGENT_TOOL,
    SEND_MESSAGE_TOOL,
    FOLLOWUP_TASK_TOOL,
    WAIT_AGENT_TOOL,
    INTERRUPT_AGENT_TOOL,
    LIST_AGENTS_TOOL,
];

/// Model-visible result returned by `wait_agent`.
/// `wait_agent` 返回给模型可见的结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WaitAgentActivityResult {
    /// Short activity summary for the parent agent.
    /// 面向父代理的简短活动摘要。
    pub message: String,
    /// Whether the wait ended because the timeout elapsed.
    /// 等待是否因为超时而结束。
    pub timed_out: bool,
}

impl WaitAgentActivityResult {
    /// Builds the normal activity-wakeup result.
    /// 构建正常活动唤醒结果。
    ///
    /// # Returns
    /// 返回值
    /// A concise wait result that does not include child output.
    /// 不包含子代理输出内容的简短等待结果。
    pub fn completed() -> Self {
        Self {
            message: String::from("Wait completed."),
            timed_out: false,
        }
    }

    /// Builds the user-steer interruption result.
    /// 构建用户插话中断结果。
    ///
    /// # Returns
    /// 返回值
    /// A concise wait result that tells the model a new parent input arrived.
    /// 告诉模型已有新的父级输入到达的简短等待结果。
    pub fn steered() -> Self {
        Self {
            message: String::from("Wait interrupted by new input."),
            timed_out: false,
        }
    }

    /// Builds the timeout result.
    /// 构建超时结果。
    ///
    /// # Returns
    /// 返回值
    /// A concise timeout result that does not include child output.
    /// 不包含子代理输出内容的简短超时结果。
    pub fn timed_out() -> Self {
        Self {
            message: String::from("Wait timed out."),
            timed_out: true,
        }
    }
}

/// Returns whether a tool name belongs to the core subagent surface.
/// 返回工具名称是否属于核心子代理工具面。
///
/// # Parameters
/// 参数
/// `tool_name` is the candidate model-visible tool name.
/// `tool_name` 是候选模型可见工具名称。
///
/// # Returns
/// 返回值
/// `true` when the name is one of the canonical core subagent tools.
/// 当名称是规范核心子代理工具之一时返回 `true`。
pub fn is_core_subagent_tool(tool_name: &str) -> bool {
    CORE_SUBAGENT_TOOLS.contains(&tool_name)
}

/// Returns whether a host-result kind is the canonical child-agent task kind.
/// 返回宿主结果类型是否为规范子代理任务类型。
///
/// # Parameters
/// 参数
/// `kind` is the host-result kind to inspect.
/// `kind` 是需要检查的宿主结果类型。
///
/// # Returns
/// 返回值
/// `true` when the kind is `subagent_task`.
/// 当类型为 `subagent_task` 时返回 `true`。
pub fn is_subagent_task_kind(kind: &str) -> bool {
    kind == SUBAGENT_TASK_KIND
}

/// Returns whether a host-result kind is accepted as a child-agent task payload.
/// 返回宿主结果类型是否可作为子代理任务载荷读取。
///
/// # Parameters
/// 参数
/// `kind` is the host-result kind to inspect.
/// `kind` 是需要检查的宿主结果类型。
///
/// # Returns
/// 返回值
/// `true` for canonical `subagent_task` and legacy `task_session` records.
/// 对规范 `subagent_task` 与历史 `task_session` 记录返回 `true`。
pub fn is_readable_subagent_task_kind(kind: &str) -> bool {
    kind == SUBAGENT_TASK_KIND || kind == LEGACY_TASK_SESSION_KIND
}
