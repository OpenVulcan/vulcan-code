use crate::contract::{
    CodexEventScope, CodexEventVisibility, CodexSessionEvent, CodexSessionEventKind, CodexThreadId,
    CodexTurnId,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use vulcan_session_core::SessionNodeId;

/// Codex-compatible child agent prompt source.
/// Codex 兼容子代理提示词来源。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CodexAgentKind {
    /// Child agent cloned from the parent agent runtime.
    /// 从父代理运行时克隆出来的子代理。
    ParentClone,
    /// Child agent using a configured agent role or agent markdown profile.
    /// 使用指定代理角色或 agent markdown 画像的子代理。
    AgentProfile,
}

/// Codex-compatible child agent status.
/// Codex 兼容子代理状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CodexSubagentStatus {
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

impl CodexSubagentStatus {
    /// Returns whether this status still represents live work.
    /// 返回该状态是否仍表示实时工作。
    pub const fn is_running(self) -> bool {
        matches!(self, Self::Running)
    }
}

/// Metadata required to create or recover one Codex-compatible child agent.
/// 创建或恢复单个 Codex 兼容子代理所需的元数据。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CodexSubagentMetadata {
    /// Internal child session identifier.
    /// 内部子会话标识符。
    pub session_id: SessionNodeId,
    /// User-visible or model-visible task identifier.
    /// 用户可见或模型可见任务标识符。
    pub public_id: String,
    /// Parent thread identifier.
    /// 父线程标识符。
    pub parent_thread_id: CodexThreadId,
    /// Parent turn identifier that spawned the child agent.
    /// 创建子代理的父轮次标识符。
    pub parent_turn_id: Option<CodexTurnId>,
    /// Agent kind.
    /// 代理类型。
    pub agent_kind: CodexAgentKind,
    /// Optional role or profile name for profile-based agents.
    /// 基于画像的代理可选角色或画像名称。
    pub agent_profile: Option<String>,
    /// Current child agent status.
    /// 当前子代理状态。
    pub status: CodexSubagentStatus,
    /// Additional adapter-owned metadata.
    /// adapter 拥有的额外元数据。
    pub metadata: Value,
}

/// Scheduler boundary for Codex-compatible child agents.
/// Codex 兼容子代理的调度边界。
pub trait CodexSubagentScheduler: Send + Sync {
    /// Spawns one child agent and returns its metadata immediately.
    /// 创建一个子代理并立即返回其元数据。
    fn spawn_subagent(
        &self,
        parent_thread_id: CodexThreadId,
        parent_turn_id: Option<CodexTurnId>,
        prompt: String,
        agent_kind: CodexAgentKind,
        agent_profile: Option<String>,
    ) -> Result<CodexSubagentMetadata, String>;

    /// Stops one child agent if it is still running.
    /// 如果子代理仍在运行则停止它。
    fn stop_subagent(&self, session_id: SessionNodeId) -> Result<(), String>;
}

/// Mailbox event kind used to report child-agent lifecycle back to the parent runtime.
/// 用于将子代理生命周期回报给父运行时的 mailbox 事件类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CodexSubagentMailboxEventKind {
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
pub struct CodexSubagentMailboxEvent {
    /// Parent thread that should receive this event.
    /// 应接收该事件的父线程。
    pub parent_thread_id: CodexThreadId,
    /// Parent turn that spawned or waits for the child agent.
    /// 创建或等待该子代理的父轮次。
    pub parent_turn_id: Option<CodexTurnId>,
    /// Child thread identifier.
    /// 子线程标识符。
    pub child_thread_id: SessionNodeId,
    /// Mailbox event kind.
    /// mailbox 事件类型。
    pub kind: CodexSubagentMailboxEventKind,
    /// Unix timestamp in milliseconds.
    /// Unix 毫秒时间戳。
    pub timestamp_ms: u64,
    /// Structured event payload.
    /// 结构化事件载荷。
    pub payload: Value,
}

/// Mailbox boundary that decouples child-agent completion from UI projection and persistence work.
/// 将子代理完成回流与 UI 投影、持久化工作解耦的 mailbox 边界。
pub trait CodexSubagentMailbox: Send + Sync {
    /// Pushes one child-agent mailbox event.
    /// 推入一条子代理 mailbox 事件。
    fn push_event(&self, event: CodexSubagentMailboxEvent) -> Result<(), String>;

    /// Takes all queued events for one parent thread in FIFO order.
    /// 按 FIFO 顺序取出单个父线程的所有排队事件。
    fn take_events_for_parent(
        &self,
        parent_thread_id: CodexThreadId,
    ) -> Result<Vec<CodexSubagentMailboxEvent>, String>;
}

/// In-memory mailbox used by tests and by adapters before a concrete runtime mailbox is mounted.
/// 测试和具体运行时 mailbox 挂载前由 adapter 使用的内存 mailbox。
#[derive(Debug, Clone, Default)]
pub struct InMemoryCodexSubagentMailbox {
    /// FIFO event queue.
    /// FIFO 事件队列。
    events: Arc<Mutex<VecDeque<CodexSubagentMailboxEvent>>>,
}

impl CodexSubagentMailbox for InMemoryCodexSubagentMailbox {
    /// Pushes one event into the FIFO queue.
    /// 将一条事件推入 FIFO 队列。
    fn push_event(&self, event: CodexSubagentMailboxEvent) -> Result<(), String> {
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
        parent_thread_id: CodexThreadId,
    ) -> Result<Vec<CodexSubagentMailboxEvent>, String> {
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
}

/// Request to spawn one child agent under a parent turn.
/// 在父轮次下创建单个子代理的请求。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CodexSubagentSpawnRequest {
    /// Parent thread identifier.
    /// 父线程标识符。
    pub parent_thread_id: CodexThreadId,
    /// Parent turn identifier.
    /// 父轮次标识符。
    pub parent_turn_id: Option<CodexTurnId>,
    /// Internal child thread identifier.
    /// 内部子线程标识符。
    pub child_thread_id: SessionNodeId,
    /// User-visible or model-visible task identifier.
    /// 用户可见或模型可见任务标识符。
    pub public_id: String,
    /// Prompt delegated to the child agent.
    /// 委派给子代理的提示词。
    pub prompt: String,
    /// Child agent kind.
    /// 子代理类型。
    pub agent_kind: CodexAgentKind,
    /// Optional child agent profile.
    /// 可选子代理画像。
    pub agent_profile: Option<String>,
    /// Unix timestamp in milliseconds.
    /// Unix 毫秒时间戳。
    pub timestamp_ms: u64,
    /// Adapter-owned metadata.
    /// adapter 拥有的元数据。
    pub metadata: Value,
}

/// Spawn result returned immediately to the parent runtime.
/// 立即返回给父运行时的创建结果。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CodexSubagentSpawnOutcome {
    /// Child-agent metadata.
    /// 子代理元数据。
    pub metadata: CodexSubagentMetadata,
    /// Parent-thread association event that should be appended to the parent JSONL stream.
    /// 应追加到父 JSONL 事件流的父线程关联事件。
    pub parent_event: CodexSessionEvent,
    /// Mailbox event delivered to the parent runtime.
    /// 传递给父运行时的 mailbox 事件。
    pub mailbox_event: CodexSubagentMailboxEvent,
}

/// Completion result delivered when a child agent reaches a terminal state.
/// 子代理到达终态时传递的完成结果。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CodexSubagentCompletionOutcome {
    /// Updated child-agent metadata.
    /// 更新后的子代理元数据。
    pub metadata: CodexSubagentMetadata,
    /// Parent-thread terminal association event.
    /// 父线程终态关联事件。
    pub parent_event: CodexSessionEvent,
    /// Mailbox event delivered to the parent runtime.
    /// 传递给父运行时的 mailbox 事件。
    pub mailbox_event: CodexSubagentMailboxEvent,
}

/// Codex-compatible subagent coordinator that never waits for child completion during spawn.
/// 创建时绝不等待子代理完成的 Codex 兼容子代理协调器。
#[derive(Debug, Clone)]
pub struct CodexSubagentCoordinator<B>
where
    B: CodexSubagentMailbox,
{
    /// Parent-runtime mailbox.
    /// 父运行时 mailbox。
    mailbox: B,
}

impl<B> CodexSubagentCoordinator<B>
where
    B: CodexSubagentMailbox,
{
    /// Creates a coordinator around one mailbox.
    /// 围绕单个 mailbox 创建协调器。
    pub const fn new(mailbox: B) -> Self {
        Self { mailbox }
    }

    /// Spawns one child agent and returns immediately with parent association data.
    /// 创建一个子代理并立即返回父会话关联数据。
    pub fn spawn_subagent(
        &self,
        request: CodexSubagentSpawnRequest,
    ) -> Result<CodexSubagentSpawnOutcome, String> {
        let metadata = CodexSubagentMetadata {
            session_id: request.child_thread_id,
            public_id: request.public_id.clone(),
            parent_thread_id: request.parent_thread_id,
            parent_turn_id: request.parent_turn_id.clone(),
            agent_kind: request.agent_kind.clone(),
            agent_profile: request.agent_profile.clone(),
            status: CodexSubagentStatus::Running,
            metadata: request.metadata.clone(),
        };
        let payload = json!({
            "child_thread_id": request.child_thread_id,
            "public_id": request.public_id,
            "parent_thread_id": request.parent_thread_id,
            "parent_turn_id": request.parent_turn_id,
            "agent_kind": request.agent_kind,
            "agent_profile": request.agent_profile,
            "status": CodexSubagentStatus::Running,
            "prompt": request.prompt,
            "metadata": request.metadata,
        });
        let parent_event = CodexSessionEvent::new(
            format!("subagent-started-{}", request.child_thread_id),
            request.parent_thread_id,
            0,
            CodexSessionEventKind::AgentStarted,
            CodexEventScope::Agent,
            CodexEventVisibility::Hidden,
            request.timestamp_ms,
        )
        .with_payload(payload.clone());
        let mailbox_event = CodexSubagentMailboxEvent {
            parent_thread_id: request.parent_thread_id,
            parent_turn_id: request.parent_turn_id,
            child_thread_id: request.child_thread_id,
            kind: CodexSubagentMailboxEventKind::Started,
            timestamp_ms: request.timestamp_ms,
            payload,
        };
        self.mailbox.push_event(mailbox_event.clone())?;
        Ok(CodexSubagentSpawnOutcome {
            metadata,
            parent_event,
            mailbox_event,
        })
    }

    /// Completes one child agent and delivers the result through the parent mailbox.
    /// 完成一个子代理并通过父 mailbox 传递结果。
    pub fn complete_subagent(
        &self,
        mut metadata: CodexSubagentMetadata,
        status: CodexSubagentStatus,
        result_text: impl Into<String>,
        timestamp_ms: u64,
    ) -> Result<CodexSubagentCompletionOutcome, String> {
        let result_text = result_text.into();
        metadata.status = status;
        let mailbox_kind = mailbox_kind_from_status(status);
        let payload = json!({
            "child_thread_id": metadata.session_id,
            "public_id": metadata.public_id,
            "parent_thread_id": metadata.parent_thread_id,
            "parent_turn_id": metadata.parent_turn_id,
            "agent_kind": metadata.agent_kind,
            "agent_profile": metadata.agent_profile,
            "status": status,
            "result_text": result_text,
            "metadata": metadata.metadata,
        });
        let parent_event = CodexSessionEvent::new(
            format!("subagent-completed-{}", metadata.session_id),
            metadata.parent_thread_id,
            0,
            CodexSessionEventKind::AgentCompleted,
            CodexEventScope::Agent,
            CodexEventVisibility::Hidden,
            timestamp_ms,
        )
        .with_payload(payload.clone());
        let mailbox_event = CodexSubagentMailboxEvent {
            parent_thread_id: metadata.parent_thread_id,
            parent_turn_id: metadata.parent_turn_id.clone(),
            child_thread_id: metadata.session_id,
            kind: mailbox_kind,
            timestamp_ms,
            payload,
        };
        self.mailbox.push_event(mailbox_event.clone())?;
        Ok(CodexSubagentCompletionOutcome {
            metadata,
            parent_event,
            mailbox_event,
        })
    }
}

/// Maps a terminal child-agent status into a mailbox event kind.
/// 将子代理终态映射为 mailbox 事件类型。
fn mailbox_kind_from_status(status: CodexSubagentStatus) -> CodexSubagentMailboxEventKind {
    match status {
        CodexSubagentStatus::Running => CodexSubagentMailboxEventKind::Message,
        CodexSubagentStatus::Completed => CodexSubagentMailboxEventKind::Completed,
        CodexSubagentStatus::Failed => CodexSubagentMailboxEventKind::Failed,
        CodexSubagentStatus::Stopped => CodexSubagentMailboxEventKind::Stopped,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vulcan_projection::codex_events_to_vulcan_conversation_state;

    /// Builds one spawn request used by coordinator tests.
    /// 构建协调器测试使用的创建请求。
    fn spawn_request(parent_thread_id: CodexThreadId) -> CodexSubagentSpawnRequest {
        CodexSubagentSpawnRequest {
            parent_thread_id,
            parent_turn_id: Some(String::from("turn-1")),
            child_thread_id: SessionNodeId::new_v4(),
            public_id: String::from("image-worker-1"),
            prompt: String::from("generate one image"),
            agent_kind: CodexAgentKind::ParentClone,
            agent_profile: None,
            timestamp_ms: 1000,
            metadata: json!({ "task_dir": "task/image-worker-1" }),
        }
    }

    /// Verifies spawning returns immediately and only emits hidden parent association data.
    /// 验证创建会立即返回且只产生隐藏父会话关联数据。
    #[test]
    fn spawn_subagent_returns_handle_without_projecting_child_output() {
        let parent_thread_id = CodexThreadId::new_v4();
        let mailbox = InMemoryCodexSubagentMailbox::default();
        let coordinator = CodexSubagentCoordinator::new(mailbox.clone());
        let outcome = coordinator
            .spawn_subagent(spawn_request(parent_thread_id))
            .expect("subagent should spawn");
        let mailbox_events = mailbox
            .take_events_for_parent(parent_thread_id)
            .expect("mailbox events should read");
        let projection =
            codex_events_to_vulcan_conversation_state(std::slice::from_ref(&outcome.parent_event));

        assert_eq!(outcome.metadata.status, CodexSubagentStatus::Running);
        assert_eq!(
            outcome.parent_event.kind,
            CodexSessionEventKind::AgentStarted
        );
        assert_eq!(
            outcome.parent_event.visibility,
            CodexEventVisibility::Hidden
        );
        assert_eq!(mailbox_events.len(), 1);
        assert_eq!(
            mailbox_events[0].kind,
            CodexSubagentMailboxEventKind::Started
        );
        assert!(projection.conversation_state.records.is_empty());
    }

    /// Verifies terminal child results are delivered through the mailbox without summary prompts.
    /// 验证子代理终态结果会通过 mailbox 回流且不会生成总结提示词。
    #[test]
    fn completed_subagent_delivers_mailbox_event_without_visible_summary() {
        let parent_thread_id = CodexThreadId::new_v4();
        let mailbox = InMemoryCodexSubagentMailbox::default();
        let coordinator = CodexSubagentCoordinator::new(mailbox.clone());
        let spawn = coordinator
            .spawn_subagent(spawn_request(parent_thread_id))
            .expect("subagent should spawn");
        let _ = mailbox
            .take_events_for_parent(parent_thread_id)
            .expect("started event should drain");

        let completion = coordinator
            .complete_subagent(
                spawn.metadata,
                CodexSubagentStatus::Completed,
                "image saved",
                1001,
            )
            .expect("subagent should complete");
        let mailbox_events = mailbox
            .take_events_for_parent(parent_thread_id)
            .expect("completion event should read");
        let projection = codex_events_to_vulcan_conversation_state(std::slice::from_ref(
            &completion.parent_event,
        ));

        assert_eq!(completion.metadata.status, CodexSubagentStatus::Completed);
        assert_eq!(mailbox_events.len(), 1);
        assert_eq!(
            mailbox_events[0].kind,
            CodexSubagentMailboxEventKind::Completed
        );
        assert_eq!(
            mailbox_events[0]
                .payload
                .get("result_text")
                .and_then(Value::as_str),
            Some("image saved")
        );
        assert!(projection.conversation_state.records.is_empty());
    }
}
