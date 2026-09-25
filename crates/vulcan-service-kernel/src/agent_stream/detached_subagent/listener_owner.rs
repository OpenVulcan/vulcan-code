//! Lifetime ownership for an admitted detached event subscription.
//! 已准入分离事件订阅的生命周期所有权。

use super::*;

/// Clears only the exited listener's registration and unpublished return gate.
/// 仅清理已退出监听器的注册及尚未发布的回流门控。
pub(super) struct ListenerOwner {
    /// Service registry containing the exact execution registration.
    /// 包含精确执行注册的服务注册表。
    state: SharedServiceBridgeState,
    /// Runtime owning the corresponding durable return gate.
    /// 拥有对应可靠回流门控的运行时。
    runtime: AgentRuntimeBridge,
    /// Stable child identity.
    /// 稳定子任务身份。
    task_id: SessionId,
    /// Immutable admitted execution generation.
    /// 不可变的已准入执行代次。
    execution_id: String,
}

impl ListenerOwner {
    /// Captures `state`, `runtime` and exact listener identifiers before spawning its future.
    /// 在派生监听 Future 前捕获 `state`、`runtime` 及精确监听标识。
    /// Returns a guard; it never registers a second listener or changes durable outcomes.
    /// 返回守卫；绝不注册第二个监听器或更改可靠终态。
    pub(super) fn new(
        state: SharedServiceBridgeState,
        runtime: AgentRuntimeBridge,
        task_id: SessionId,
        execution_id: String,
    ) -> Self {
        Self {
            state,
            runtime,
            task_id,
            execution_id,
        }
    }
}

impl Drop for ListenerOwner {
    /// Releases actual ownership on EOF, cancellation or panic without inventing a successful result.
    /// 在流结束、取消或恐慌时释放实际所有权，不编造成功结果。
    /// Returns no value; lock failures remain explicit diagnostics and newer generations are untouched.
    /// 无返回值；锁失败保留明确诊断，较新代次不受影响。
    fn drop(&mut self) {
        match self.state.lock() {
            Ok(mut state) => {
                clear_webui_detached_listener_execution_if_current(
                    &mut state,
                    self.task_id,
                    &self.execution_id,
                );
            }
            Err(error) => {
                tracing::error!(%error, "Exited child listener registration cleanup failed")
            }
        }
        if let Err(error) = self
            .runtime
            .task_runtime
            .release_unpublished_task_return(self.task_id, &self.execution_id)
        {
            tracing::error!(%error, "Exited child listener return gate cleanup failed");
        }
    }
}
