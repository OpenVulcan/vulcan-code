//! Tool execution traits owned by `vulcan-core`.
//! `vulcan-core` 自有的工具执行 trait。

use crate::types::{ConversationSnapshot, CoreError, ToolInvocationRequest, ToolInvocationResult};
use async_trait::async_trait;
use std::collections::BTreeSet;

/// Describes one host-side tool executor that can satisfy assistant tool calls.
/// 表示一个可以满足助手工具调用的宿主侧工具执行器。
#[async_trait]
pub trait VulcanToolExecutor: Send + Sync {
    /// Persists or validates one complete tool batch before any invocation body may start.
    /// 在任何调用主体启动前持久化或校验一批完整工具请求。
    async fn prepare_tool_batch(
        &self,
        _requests: &[ToolInvocationRequest],
    ) -> Result<(), CoreError> {
        Ok(())
    }

    /// Executes one tool invocation request and returns a structured tool result.
    /// 执行一次工具调用请求并返回结构化工具结果。
    async fn execute_tool(
        &self,
        request: &ToolInvocationRequest,
    ) -> Result<ToolInvocationResult, CoreError>;
}

/// Describes the policy that decides whether the controller should execute a tool call.
/// 表示决定控制器是否应执行某次工具调用的策略。
pub trait VulcanToolPolicy: Send + Sync {
    /// Returns whether the controller should execute the requested tool call.
    /// 返回控制器是否应执行所请求的工具调用。
    fn should_execute(
        &self,
        request: &ToolInvocationRequest,
        snapshot: &ConversationSnapshot,
    ) -> bool;
}

/// Allows every requested tool invocation to execute without additional filtering.
/// 允许所有被请求的工具调用直接执行，不附加额外过滤规则。
#[derive(Debug, Clone, Default)]
pub struct AllowAllToolPolicy;

impl VulcanToolPolicy for AllowAllToolPolicy {
    /// Returns `true` for every requested tool invocation.
    /// 对所有被请求的工具调用都返回 `true`。
    fn should_execute(
        &self,
        _request: &ToolInvocationRequest,
        _snapshot: &ConversationSnapshot,
    ) -> bool {
        true
    }
}

/// Allows only the explicitly enumerated tool identifiers to execute.
/// 仅允许显式枚举出来的工具标识被执行。
#[derive(Debug, Clone, Default)]
pub struct ExactToolSetPolicy {
    /// Stable set of allowed tool identifiers.
    /// 允许执行的稳定工具标识集合。
    allowed_tools: BTreeSet<String>,
}

impl ExactToolSetPolicy {
    /// Builds one exact-allowlist policy from the provided tool identifiers.
    /// 根据给定工具标识集合构建一份精确白名单策略。
    pub fn new<I, S>(allowed_tools: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            allowed_tools: allowed_tools.into_iter().map(Into::into).collect(),
        }
    }
}

impl VulcanToolPolicy for ExactToolSetPolicy {
    /// Returns whether the requested tool identifier belongs to the explicit allowlist.
    /// 返回被请求的工具标识是否属于显式白名单集合。
    fn should_execute(
        &self,
        request: &ToolInvocationRequest,
        _snapshot: &ConversationSnapshot,
    ) -> bool {
        self.allowed_tools.contains(&request.tool_name)
    }
}
