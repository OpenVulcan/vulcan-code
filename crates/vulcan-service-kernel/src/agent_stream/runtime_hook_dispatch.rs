//! Runtime-event conversation Hook dispatch boundaries.
//! 运行时事件会话 Hook 派发边界。

use super::*;

/// Dispatches one tool runtime-event Hook through an immutable execution correlation.
/// 通过不可变执行关联派发单条工具运行时事件 Hook。
///
/// # Parameters
/// # 参数
/// `agent_runtime` owns the shared conversation Hook runtime.
/// `agent_runtime` 持有共享会话 Hook 运行时。
/// `session_id` identifies the session that emitted the authoritative runtime event.
/// `session_id` 标识发出权威运行时事件的会话。
/// `event` is filtered to the tool-started and tool-finished Hook contract before locking.
/// `event` 会在加锁前按工具开始与工具结束 Hook 契约进行过滤。
/// `execution_id` is the immutable request, detached-task, or standalone execution correlation.
/// `execution_id` 是不可变的请求、分离任务或独立执行关联标识。
///
/// # Returns
/// # 返回值
/// Success after an applicable notification is scheduled or an unrelated event is ignored; an explicit error when the Hook runtime lock is unavailable.
/// 适用通知完成调度或无关事件被忽略后返回成功；Hook 运行时锁不可用时返回明确错误。
pub(crate) fn dispatch_runtime_event_hook(
    agent_runtime: &AgentRuntimeBridge,
    session_id: SessionId,
    event: &RuntimeEvent,
    execution_id: &str,
) -> Result<(), String> {
    if !matches!(
        event.kind,
        RuntimeEventKind::ToolStarted | RuntimeEventKind::ToolFinished
    ) {
        return Ok(());
    }

    // hook_runtime serializes access to the exact conversation Hook state shared by all event consumers.
    // hook_runtime 串行化所有事件消费者共享的精确会话 Hook 状态访问。
    let hook_runtime = agent_runtime
        .conversation_hook_runtime
        .lock()
        .map_err(|error| {
            format!(
                "failed to lock conversation hook runtime for explicit execution {execution_id} tool event in session {session_id}: {error}"
            )
        })?;
    hook_runtime.record_tool_runtime_event_for_execution(session_id, execution_id, event);
    Ok(())
}
