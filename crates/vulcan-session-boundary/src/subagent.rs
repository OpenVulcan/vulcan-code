use crate::contract::{SessionThreadId, SessionTurnId};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use vulcan_session_core::SessionNodeId;

/// session-boundary child agent prompt source.
/// 会话边界子代理提示词来源。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubagentMailboxAgentKind {
    /// Child agent cloned from the parent agent runtime.
    /// 从父代理运行时克隆出来的子代理。
    ParentClone,
    /// Child agent using a configured agent role or agent markdown profile.
    /// 使用指定代理角色或 agent markdown 画像的子代理。
    AgentProfile,
}

/// session-boundary child agent status.
/// 会话边界子代理状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubagentMailboxStatus {
    /// Child agent was created but has not produced a terminal result.
    /// 子代理已创建但尚未产出终态结果。
    Running,
    /// Child agent completed successfully.
    /// 子代理已成功完成。
    Completed,
    /// Child agent failed.
    /// 子代理已失败。
    Failed,
    /// Child agent was stopped.
    /// 子代理已停止。
    Stopped,
}

impl SubagentMailboxStatus {
    /// Returns whether this status still represents live work.
    /// 返回该状态是否仍表示实时工作。
    pub const fn is_running(self) -> bool {
        matches!(self, Self::Running)
    }
}

/// Mailbox event kind used to report child-agent lifecycle back to the parent runtime.
/// 用于将子代理生命周期回报给父运行时的 mailbox 事件类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubagentMailboxEventKind {
    /// Child agent was spawned.
    /// 子代理已创建。
    Started,
    /// Child agent produced a parent-visible lifecycle message.
    /// 子代理产生了父运行时可见的生命周期消息。
    Message,
    /// Child agent completed successfully.
    /// 子代理已成功完成。
    Completed,
    /// Child agent failed.
    /// 子代理已失败。
    Failed,
    /// Child agent was stopped.
    /// 子代理已停止。
    Stopped,
}

/// Parent-runtime mailbox event for one child agent.
/// 单个子代理对应的父运行时 mailbox 事件。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SubagentMailboxEvent {
    /// Parent thread that should receive this event.
    /// 应接收该事件的父线程。
    pub parent_thread_id: SessionThreadId,
    /// Parent turn that spawned or waits for the child agent.
    /// 创建或等待该子代理的父轮次。
    pub parent_turn_id: Option<SessionTurnId>,
    /// Child thread identifier.
    /// 子线程标识符。
    pub child_thread_id: SessionNodeId,
    /// Mailbox event kind.
    /// mailbox 事件类型。
    pub kind: SubagentMailboxEventKind,
    /// Unix timestamp in milliseconds.
    /// Unix 毫秒时间戳。
    pub timestamp_ms: u64,
    /// Structured event payload.
    /// 结构化事件载荷。
    pub payload: Value,
}

/// Mailbox boundary that decouples child-agent completion from UI projection and persistence work.
/// 将子代理完成回流与 UI 投影、持久化工作解耦的 mailbox 边界。
pub trait SubagentMailbox: Send + Sync {
    /// Pushes one child-agent mailbox event.
    /// 推入一条子代理 mailbox 事件。
    fn push_event(&self, event: SubagentMailboxEvent) -> Result<(), String>;

    /// Takes all queued events for one parent thread in FIFO order.
    /// 按 FIFO 顺序取出单个父线程的所有排队事件。
    fn take_events_for_parent(
        &self,
        parent_thread_id: SessionThreadId,
    ) -> Result<Vec<SubagentMailboxEvent>, String>;

    /// Returns whether one parent thread has queued mailbox events.
    /// 返回单个父线程是否存在排队的 mailbox 事件。
    fn has_events_for_parent(&self, parent_thread_id: SessionThreadId) -> Result<bool, String>;
}

/// In-memory mailbox used by tests and by adapters before a concrete runtime mailbox is mounted.
/// 测试和具体运行时 mailbox 挂载前由 adapter 使用的内存 mailbox。
#[derive(Debug, Clone, Default)]
pub struct InMemorySubagentMailbox {
    /// FIFO event queue.
    /// FIFO 事件队列。
    events: Arc<Mutex<VecDeque<SubagentMailboxEvent>>>,
}

impl InMemorySubagentMailbox {
    /// Takes queued events for one parent thread that satisfy a caller-provided predicate.
    /// 按调用方提供的谓词取出单个父线程下匹配的排队事件。
    ///
    /// # Parameters
    /// 参数
    /// `parent_thread_id` is the parent thread whose mailbox events are being claimed.
    /// `parent_thread_id` 是正在认领 mailbox 事件的父线程。
    /// `matches` decides whether one event should be taken instead of retained.
    /// `matches` 决定单条事件应被取出还是保留。
    ///
    /// # Returns
    /// 返回值
    /// FIFO-ordered matching events while non-matching events remain queued.
    /// 返回按 FIFO 排序的匹配事件，未匹配事件继续留在队列中。
    pub fn take_matching_events_for_parent<Matches>(
        &self,
        parent_thread_id: SessionThreadId,
        mut matches: Matches,
    ) -> Result<Vec<SubagentMailboxEvent>, String>
    where
        Matches: FnMut(&SubagentMailboxEvent) -> bool,
    {
        let mut guard = self
            .events
            .lock()
            .map_err(|error| format!("failed to lock subagent mailbox: {error}"))?;
        let mut taken = Vec::new();
        let mut retained = VecDeque::new();
        while let Some(event) = guard.pop_front() {
            if event.parent_thread_id == parent_thread_id && matches(&event) {
                taken.push(event);
            } else {
                retained.push_back(event);
            }
        }
        *guard = retained;
        Ok(taken)
    }
}

impl SubagentMailbox for InMemorySubagentMailbox {
    /// Pushes one event into the FIFO queue.
    /// 将一条事件推入 FIFO 队列。
    fn push_event(&self, event: SubagentMailboxEvent) -> Result<(), String> {
        self.events
            .lock()
            .map_err(|error| format!("failed to lock subagent mailbox: {error}"))?
            .push_back(event);
        Ok(())
    }

    /// Takes all currently queued events for one parent thread.
    /// 取出当前为单个父线程排队的所有事件。
    fn take_events_for_parent(
        &self,
        parent_thread_id: SessionThreadId,
    ) -> Result<Vec<SubagentMailboxEvent>, String> {
        let mut guard = self
            .events
            .lock()
            .map_err(|error| format!("failed to lock subagent mailbox: {error}"))?;
        let mut taken = Vec::new();
        let mut retained = VecDeque::new();
        while let Some(event) = guard.pop_front() {
            if event.parent_thread_id == parent_thread_id {
                taken.push(event);
            } else {
                retained.push_back(event);
            }
        }
        *guard = retained;
        Ok(taken)
    }

    /// Checks for queued events without consuming them.
    /// 在不消费事件的情况下检查是否存在排队事件。
    fn has_events_for_parent(&self, parent_thread_id: SessionThreadId) -> Result<bool, String> {
        let guard = self
            .events
            .lock()
            .map_err(|error| format!("failed to lock subagent mailbox: {error}"))?;
        Ok(guard
            .iter()
            .any(|event| event.parent_thread_id == parent_thread_id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies the mailbox drains only events matching the selected parent thread.
    /// 验证 mailbox 只取出与选定父线程匹配的事件。
    #[test]
    fn mailbox_drains_events_for_one_parent_only() {
        let parent_thread_id = SessionThreadId::new_v4();
        let other_parent_thread_id = SessionThreadId::new_v4();
        let child_thread_id = SessionNodeId::new_v4();
        let mailbox = InMemorySubagentMailbox::default();

        mailbox
            .push_event(SubagentMailboxEvent {
                parent_thread_id,
                parent_turn_id: Some(String::from("turn-1")),
                child_thread_id,
                kind: SubagentMailboxEventKind::Completed,
                timestamp_ms: 1000,
                payload: Value::Null,
            })
            .expect("event should push");
        mailbox
            .push_event(SubagentMailboxEvent {
                parent_thread_id: other_parent_thread_id,
                parent_turn_id: Some(String::from("turn-2")),
                child_thread_id,
                kind: SubagentMailboxEventKind::Completed,
                timestamp_ms: 1001,
                payload: Value::Null,
            })
            .expect("event should push");

        let mailbox_events = mailbox
            .take_events_for_parent(parent_thread_id)
            .expect("mailbox events should read");

        assert_eq!(mailbox_events.len(), 1);
        assert_eq!(mailbox_events[0].parent_thread_id, parent_thread_id);
        assert_eq!(mailbox_events[0].kind, SubagentMailboxEventKind::Completed);
        assert_eq!(
            mailbox
                .take_events_for_parent(other_parent_thread_id)
                .expect("remaining event should read")
                .len(),
            1
        );
    }
}
