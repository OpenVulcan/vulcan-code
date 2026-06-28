use async_trait::async_trait;

use crate::{HookContext, HookError, HookPayload, HookRegistration, HookResult};

/// Request passed to one hook handler.
/// 传递给单个 hook 处理器的请求。
#[derive(Debug, Clone)]
pub struct HookInvocation {
    /// Registration metadata for the current handler.
    /// 当前处理器的注册元数据。
    pub registration: HookRegistration,
    /// Common invocation context.
    /// 通用调用上下文。
    pub context: HookContext,
    /// Current payload value.
    /// 当前 payload 值。
    pub payload: HookPayload,
}

/// Trait implemented by hook handlers.
/// hook 处理器实现的 trait。
#[async_trait]
pub trait HookHandler: Send + Sync {
    /// Runs the handler for one invocation and returns its contribution.
    /// 为一次调用运行处理器并返回其贡献结果。
    async fn handle(&self, invocation: HookInvocation) -> Result<HookResult, HookError>;
}

/// Function-backed hook handler.
/// 基于函数的 hook 处理器。
pub struct FnHookHandler<F> {
    /// Function invoked for each hook call.
    /// 每次 hook 调用时执行的函数。
    function: F,
}

impl<F> FnHookHandler<F> {
    /// Builds a function-backed hook handler.
    /// 构建一个基于函数的 hook 处理器。
    pub fn new(function: F) -> Self {
        Self { function }
    }
}

#[async_trait]
impl<F, Fut> HookHandler for FnHookHandler<F>
where
    F: Send + Sync + Fn(HookInvocation) -> Fut,
    Fut: Send + std::future::Future<Output = Result<HookResult, HookError>>,
{
    /// Runs the wrapped function for one hook invocation.
    /// 为一次 hook 调用运行被包装函数。
    async fn handle(&self, invocation: HookInvocation) -> Result<HookResult, HookError> {
        (self.function)(invocation).await
    }
}
