use crate::contract::{CodexSessionEvent, CodexThreadId, CodexTurnId};
use crate::subagent::CodexSubagentScheduler;
use serde_json::Value;

/// Model-call adapter kept outside the Codex-compatible session contract.
/// 保持在 Codex 兼容会话契约之外的模型调用适配器。
pub trait CodexModelAdapter: Send + Sync {
    /// Starts one model turn and returns append-only runtime events.
    /// 启动一次模型轮次并返回追加式运行时事件。
    fn run_turn(
        &self,
        thread_id: CodexThreadId,
        turn_id: CodexTurnId,
        input: Value,
    ) -> Result<Vec<CodexSessionEvent>, String>;
}

/// Tool adapter used to mount Vulcan tools into the Codex-compatible runtime.
/// 用于将 Vulcan 工具挂载到 Codex 兼容运行时的工具适配器。
pub trait CodexToolAdapter: Send + Sync {
    /// Invokes one tool and returns append-only runtime events.
    /// 调用单个工具并返回追加式运行时事件。
    fn invoke_tool(
        &self,
        thread_id: CodexThreadId,
        turn_id: CodexTurnId,
        tool_name: &str,
        arguments: Value,
    ) -> Result<Vec<CodexSessionEvent>, String>;
}

/// Hook adapter used for Vulcan VMM, LuaSkills lifecycle hooks, and other post-processing.
/// 用于 Vulcan VMM、LuaSkills 生命周期 hook 和其他后处理的 hook 适配器。
pub trait CodexHookAdapter: Send + Sync {
    /// Observes one event after it has been appended to session storage.
    /// 在事件已追加到会话存储后观察该事件。
    fn after_event_appended(&self, event: &CodexSessionEvent) -> Result<(), String>;
}

/// Adapter set required to run Codex-compatible sessions inside Vulcan.
/// 在 Vulcan 内运行 Codex 兼容会话所需的适配器集合。
pub struct CodexExtensionAdapterSet<M, T, H, S>
where
    M: CodexModelAdapter,
    T: CodexToolAdapter,
    H: CodexHookAdapter,
    S: CodexSubagentScheduler,
{
    /// Model-call adapter.
    /// 模型调用适配器。
    pub model: M,
    /// Tool adapter.
    /// 工具适配器。
    pub tools: T,
    /// Hook adapter.
    /// Hook 适配器。
    pub hooks: H,
    /// Child-agent scheduler.
    /// 子代理调度器。
    pub subagents: S,
}
