//! Core Vulcan subagent orchestration protocol.
//! Vulcan 核心子代理调度协议。

use serde::{Deserialize, Serialize};

/// Stable public name for the child-agent creation tool.
/// 子代理创建工具的稳定公开名称。
pub const SPAWN_AGENT_TOOL: &str = "spawn_agent";

/// Stable public name for the child-agent message tool.
/// 子代理消息工具的稳定公开名称。
pub const SEND_MESSAGE_TOOL: &str = "send_message";

/// Stable public name for the child-agent orchestration information tool.
/// 子代理调度信息查询工具的稳定公开名称。
pub const AGENT_ORCHESTRATION_INFO_TOOL: &str = "agent_orchestration_info";

/// Stable public name for the child-agent wait tool.
/// 子代理等待工具的稳定公开名称。
pub const WAIT_AGENT_TOOL: &str = "wait_agent";

/// Public inbox wait shared by child agents, independent sessions and question replies.
/// 子代理、独立会话及问题回复共享的公开收件等待。
pub const WAIT_AGENTS_RESULT_TOOL: &str = "wait_agents_result";

/// Stable public name for the child-agent listing tool.
/// 子代理列表工具的稳定公开名称。
pub const LIST_AGENTS_TOOL: &str = "list_agents";

/// Stable public name for the child-agent interrupt tool.
/// 子代理中断工具的稳定公开名称。
pub const INTERRUPT_AGENT_TOOL: &str = "interrupt_agent";

/// Stable public name for the child-agent close tool.
/// 子代理关闭工具的稳定公开名称。
pub const CLOSE_AGENT_TOOL: &str = "close_agent";

/// Canonical host-result kind for one Vulcan child-agent task.
/// Vulcan 子代理任务的规范宿主结果类型。
pub const SUBAGENT_TASK_KIND: &str = "subagent_task";

/// Host-result kind of the conversation reading projection that replaces a completed spawn, message or list result.
/// 会话阅读投影替换已完成的启动、消息或列表结果后使用的宿主结果类型。
pub const SUBAGENT_RECEIPT_KIND: &str = "subagent_receipt_v1";

/// Canonical payload key for the internal child-agent task identifier.
/// 内部子代理任务标识的规范载荷字段。
pub const SUBAGENT_TASK_ID_KEY: &str = "subagent_task_id";

/// Canonical payload key for stopped child-agent identifiers.
/// 已停止子代理标识列表的规范载荷字段。
pub const STOPPED_SUBAGENT_TASK_IDS_KEY: &str = "stopped_subagent_task_ids";

/// Implemented core subagent tool names exposed to the model.
/// 已实现并暴露给模型的规范核心子代理工具名称。
pub const CORE_SUBAGENT_TOOLS: [&str; 7] = [
    SPAWN_AGENT_TOOL,
    AGENT_ORCHESTRATION_INFO_TOOL,
    SEND_MESSAGE_TOOL,
    WAIT_AGENTS_RESULT_TOOL,
    INTERRUPT_AGENT_TOOL,
    CLOSE_AGENT_TOOL,
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
/// `true` only for canonical `subagent_task` records.
/// 仅对规范 `subagent_task` 记录返回 `true`。
pub fn is_readable_subagent_task_kind(kind: &str) -> bool {
    kind == SUBAGENT_TASK_KIND
}
