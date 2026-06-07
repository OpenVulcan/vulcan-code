use crate::adapter::{CodexHookAdapter, CodexModelAdapter};
use crate::contract::CodexThreadId;
use crate::runner::{CodexTurnRunRequest, CodexTurnRunner};
use crate::storage::{CodexSessionStateStore, CodexSessionStore};
use crate::vulcan_projection::{
    CodexVulcanConversationProjection, codex_events_to_vulcan_conversation_state,
};

/// Main-session facade that keeps WebUI outside Codex turn scheduling decisions.
/// 让 WebUI 脱离 Codex 轮次调度决策的主会话门面。
pub struct CodexMainSessionKernel<S, H, M>
where
    S: CodexSessionStore + CodexSessionStateStore + Clone,
    H: CodexHookAdapter,
    M: CodexModelAdapter,
{
    /// Store clone used for read-only projection after runner writes.
    /// runner 写入后用于只读投影的存储克隆。
    store: S,
    /// Codex-compatible turn runner that owns scheduling and state transitions.
    /// 拥有调度与状态迁移职责的 Codex 兼容轮次 runner。
    runner: CodexTurnRunner<S, H, M>,
}

impl<S, H, M> CodexMainSessionKernel<S, H, M>
where
    S: CodexSessionStore + CodexSessionStateStore + Clone,
    H: CodexHookAdapter,
    M: CodexModelAdapter,
{
    /// Creates one main-session kernel from a store, hooks, and model adapter.
    /// 使用存储、hook 与模型适配器创建一个主会话内核。
    pub fn new(store: S, hooks: H, model: M) -> Self {
        Self {
            runner: CodexTurnRunner::new(store.clone(), hooks, model),
            store,
        }
    }

    /// Ensures a thread-started event exists before accepting user turns.
    /// 在接受用户轮次前确保线程启动事件已存在。
    pub fn ensure_thread_started(
        &self,
        thread_id: CodexThreadId,
        timestamp_ms: u64,
    ) -> Result<(), String> {
        if self.store.read_events(thread_id)?.is_empty() {
            self.runner.start_thread(thread_id, timestamp_ms)?;
        }
        Ok(())
    }

    /// Runs one normal main-session turn and returns a fresh display projection.
    /// 运行一次普通主会话轮次并返回最新展示投影。
    pub fn run_turn_and_project(
        &self,
        request: CodexTurnRunRequest,
        timestamp_ms: u64,
    ) -> Result<CodexVulcanConversationProjection, String> {
        self.ensure_thread_started(request.thread_id, timestamp_ms)?;
        let thread_id = request.thread_id;
        self.runner.run_turn(request, timestamp_ms)?;
        self.project_thread(thread_id)
    }

    /// Rebuilds the Vulcan display projection from persisted Codex events.
    /// 从已持久化 Codex 事件重建 Vulcan 展示投影。
    pub fn project_thread(
        &self,
        thread_id: CodexThreadId,
    ) -> Result<CodexVulcanConversationProjection, String> {
        let events = self.store.read_events(thread_id)?;
        Ok(codex_events_to_vulcan_conversation_state(&events))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapter::{CodexHookAdapter, CodexModelAdapter};
    use crate::contract::{
        CodexEventScope, CodexEventVisibility, CodexSessionEvent, CodexSessionEventKind,
        CodexTurnId,
    };
    use crate::memory::InMemoryCodexSessionStore;
    use serde_json::{Value, json};
    use vulcan_context::{SessionConversationPayload, SessionMessageRecordKind};

    /// Hook adapter that intentionally performs no post-append work.
    /// 有意不执行追加后工作的 hook 适配器。
    struct NoopHook;

    impl CodexHookAdapter for NoopHook {
        /// Ignores the durable event.
        /// 忽略已持久化事件。
        fn after_event_appended(&self, _event: &CodexSessionEvent) -> Result<(), String> {
            Ok(())
        }
    }

    /// Model adapter that emits one visible assistant response.
    /// 发出一条可见助手响应的模型适配器。
    struct AssistantModel;

    impl CodexModelAdapter for AssistantModel {
        /// Returns one assistant message event for the requested turn.
        /// 为请求轮次返回一条助手消息事件。
        fn run_turn(
            &self,
            thread_id: CodexThreadId,
            turn_id: CodexTurnId,
            _input: Value,
        ) -> Result<Vec<CodexSessionEvent>, String> {
            Ok(vec![
                CodexSessionEvent::new(
                    "assistant-answer",
                    thread_id,
                    0,
                    CodexSessionEventKind::AgentMessage,
                    CodexEventScope::Turn,
                    CodexEventVisibility::Visible,
                    1001,
                )
                .with_turn_id(turn_id)
                .with_payload(json!({ "text": "done" })),
            ])
        }
    }

    /// Verifies the main-session kernel starts, runs, persists, and projects one turn.
    /// 验证主会话内核能启动、运行、持久化并投影单个轮次。
    #[test]
    fn main_session_kernel_runs_turn_and_projects_conversation_state() {
        let store = InMemoryCodexSessionStore::default();
        let kernel = CodexMainSessionKernel::new(store.clone(), NoopHook, AssistantModel);
        let thread_id = CodexThreadId::new_v4();

        let projection = kernel
            .run_turn_and_project(
                CodexTurnRunRequest {
                    thread_id,
                    turn_id: String::from("turn-1"),
                    prompt: String::from("hello"),
                    model_input: json!({ "prompt": "hello" }),
                },
                1000,
            )
            .expect("turn should run");
        let events = store.read_events(thread_id).expect("events should read");

        assert_eq!(events.len(), 5);
        assert_eq!(projection.conversation_state.records.len(), 2);
        assert_eq!(projection.conversation_state.records[0].body, "hello");
        assert_eq!(
            projection.conversation_state.records[1].record_kind,
            SessionMessageRecordKind::AssistantOutput
        );
        assert!(matches!(
            projection.conversation_state.messages[0].payload,
            SessionConversationPayload::UserText { .. }
        ));
    }
}
