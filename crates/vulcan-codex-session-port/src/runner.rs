use crate::adapter::{CodexHookAdapter, CodexModelAdapter};
use crate::contract::{
    CodexEventScope, CodexEventVisibility, CodexSessionEvent, CodexSessionEventKind, CodexThreadId,
    CodexTurnId,
};
use crate::runtime::CodexSessionRuntime;
use crate::storage::{CodexSessionStateStore, CodexSessionStore};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// Thread runtime status stored as overwrite-only Codex-compatible state.
/// 作为覆盖式 Codex 兼容状态保存的线程运行状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CodexThreadStatus {
    /// Thread is idle and can accept a new turn.
    /// 线程空闲并可以接受新轮次。
    Idle,
    /// Thread is currently running a turn.
    /// 线程当前正在运行轮次。
    Running,
    /// Thread has a paused or interrupted turn.
    /// 线程存在暂停或中断的轮次。
    Interrupted,
    /// Thread failed and needs an explicit follow-up or resume.
    /// 线程失败，需要显式跟进或恢复。
    Failed,
}

/// Terminal status for one Codex-compatible turn.
/// 单个 Codex 兼容轮次的终态状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CodexTurnTerminalStatus {
    /// Turn completed normally.
    /// 轮次正常完成。
    Completed,
    /// Turn failed.
    /// 轮次失败。
    Failed,
    /// Turn was interrupted or stopped.
    /// 轮次被中断或停止。
    Interrupted,
}

/// Overwrite-only state document for one Codex-compatible thread.
/// 单个 Codex 兼容线程的覆盖式状态文档。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodexThreadState {
    /// Thread identifier.
    /// 线程标识符。
    pub thread_id: CodexThreadId,
    /// Next event sequence to append.
    /// 下一条待追加事件序号。
    pub next_sequence: u64,
    /// Current runtime status.
    /// 当前运行状态。
    pub status: CodexThreadStatus,
    /// Active turn identifier when a turn is running.
    /// 轮次运行中时的活动轮次标识符。
    pub active_turn_id: Option<CodexTurnId>,
    /// Last completed or failed turn identifier.
    /// 最近完成或失败的轮次标识符。
    pub last_turn_id: Option<CodexTurnId>,
}

impl CodexThreadState {
    /// Creates an initial state for one thread.
    /// 为单个线程创建初始状态。
    pub const fn new(thread_id: CodexThreadId) -> Self {
        Self {
            thread_id,
            next_sequence: 1,
            status: CodexThreadStatus::Idle,
            active_turn_id: None,
            last_turn_id: None,
        }
    }

    /// Takes and increments the next append sequence.
    /// 取出并递增下一条追加序号。
    pub fn take_sequence(&mut self) -> u64 {
        let sequence = self.next_sequence;
        self.next_sequence = self.next_sequence.saturating_add(1);
        sequence
    }
}

/// Request used to start one Codex-compatible turn.
/// 用于启动单个 Codex 兼容轮次的请求。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CodexTurnRunRequest {
    /// Target thread identifier.
    /// 目标线程标识符。
    pub thread_id: CodexThreadId,
    /// Stable turn identifier.
    /// 稳定轮次标识符。
    pub turn_id: CodexTurnId,
    /// User-visible prompt text.
    /// 用户可见提示词文本。
    pub prompt: String,
    /// Adapter-owned model input payload.
    /// adapter 拥有的模型输入载荷。
    pub model_input: Value,
}

/// Request used when an external streaming provider has accepted one turn.
/// 外部流式 provider 已接受一个轮次时使用的请求。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CodexExternalTurnBeginRequest {
    /// Target thread identifier.
    /// 目标线程标识符。
    pub thread_id: CodexThreadId,
    /// Stable turn identifier.
    /// 稳定轮次标识符。
    pub turn_id: CodexTurnId,
    /// Already-built lifecycle events from the host-side precheck path.
    /// 宿主侧 precheck 路径已经构建好的生命周期事件。
    pub accepted_events: Vec<CodexSessionEvent>,
}

/// Request used when an external streaming provider reaches a terminal state.
/// 外部流式 provider 到达终态时使用的请求。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CodexExternalTurnFinishRequest {
    /// Target thread identifier.
    /// 目标线程标识符。
    pub thread_id: CodexThreadId,
    /// Stable turn identifier.
    /// 稳定轮次标识符。
    pub turn_id: CodexTurnId,
    /// Terminal status reported by the external provider stream.
    /// 外部 provider 流报告的终态状态。
    pub terminal_status: CodexTurnTerminalStatus,
    /// Final turn events produced by the external provider before the terminal event.
    /// 外部 provider 在终态事件前产生的最终轮次事件。
    pub final_events: Vec<CodexSessionEvent>,
    /// Optional terminal event built by the host-side report path.
    /// 宿主侧报告路径构建的可选终态事件。
    pub terminal_event: Option<CodexSessionEvent>,
}

/// Codex lifecycle runner for externally streamed provider turns.
/// 面向外部流式 provider 轮次的 Codex 生命周期运行器。
pub struct CodexExternalStreamTurnRunner<S, H>
where
    S: CodexSessionStore + CodexSessionStateStore,
    H: CodexHookAdapter,
{
    /// Event runtime that enforces append-before-hook ordering.
    /// 强制追加先于 hook 的事件运行时。
    runtime: CodexSessionRuntime<S, H>,
}

impl<S, H> CodexExternalStreamTurnRunner<S, H>
where
    S: CodexSessionStore + CodexSessionStateStore,
    H: CodexHookAdapter,
{
    /// Creates a lifecycle runner for externally streamed turns.
    /// 创建面向外部流式轮次的生命周期运行器。
    pub const fn new(store: S, hooks: H) -> Self {
        Self {
            runtime: CodexSessionRuntime::new(store, hooks),
        }
    }

    /// Returns a reference to the underlying runtime.
    /// 返回底层运行时引用。
    pub const fn runtime(&self) -> &CodexSessionRuntime<S, H> {
        &self.runtime
    }

    /// Begins one externally streamed turn and appends accepted lifecycle events.
    /// 开始一个外部流式轮次，并追加已接受的生命周期事件。
    pub fn begin_turn(&self, request: CodexExternalTurnBeginRequest) -> Result<(), String> {
        let mut state = load_or_create_thread_state(&self.runtime, request.thread_id)?;
        state.status = CodexThreadStatus::Running;
        state.active_turn_id = Some(request.turn_id.clone());
        for event in request.accepted_events {
            let event = resequence_event(event, &mut state);
            self.runtime.append_event(event)?;
        }
        store_thread_state(&self.runtime, request.thread_id, &state)
    }

    /// Finishes one externally streamed turn and appends one terminal event.
    /// 结束一个外部流式轮次，并追加一个终态事件。
    pub fn finish_turn(&self, request: CodexExternalTurnFinishRequest) -> Result<(), String> {
        let mut state = load_or_create_thread_state(&self.runtime, request.thread_id)?;
        state.status = match request.terminal_status {
            CodexTurnTerminalStatus::Completed => CodexThreadStatus::Idle,
            CodexTurnTerminalStatus::Failed => CodexThreadStatus::Failed,
            CodexTurnTerminalStatus::Interrupted => CodexThreadStatus::Interrupted,
        };
        state.active_turn_id = None;
        state.last_turn_id = Some(request.turn_id.clone());
        for event in request.final_events {
            let event = resequence_event(event, &mut state);
            self.runtime.append_event(event)?;
        }
        let terminal_event = request.terminal_event.unwrap_or_else(|| {
            CodexSessionEvent::new(
                format!("turn-terminal-{}", request.turn_id),
                request.thread_id,
                0,
                CodexSessionEventKind::TurnCompleted,
                CodexEventScope::Turn,
                CodexEventVisibility::Hidden,
                0,
            )
            .with_turn_id(request.turn_id.clone())
            .with_payload(json!({
                "turn_id": request.turn_id,
                "status": request.terminal_status,
            }))
        });
        let terminal_event = resequence_event(terminal_event, &mut state);
        self.runtime.append_event(terminal_event)?;
        store_thread_state(&self.runtime, request.thread_id, &state)
    }
}

/// Minimal Codex-compatible thread and turn runner.
/// 最小 Codex 兼容线程与轮次运行器。
pub struct CodexTurnRunner<S, H, M>
where
    S: CodexSessionStore + CodexSessionStateStore,
    H: CodexHookAdapter,
    M: CodexModelAdapter,
{
    /// Event runtime that enforces append-before-hook ordering.
    /// 强制追加先于 hook 的事件运行时。
    runtime: CodexSessionRuntime<S, H>,
    /// Model adapter used only at the model-call boundary.
    /// 仅在模型调用边界使用的模型适配器。
    model: M,
}

impl<S, H, M> CodexTurnRunner<S, H, M>
where
    S: CodexSessionStore + CodexSessionStateStore,
    H: CodexHookAdapter,
    M: CodexModelAdapter,
{
    /// Creates a Codex-compatible turn runner.
    /// 创建 Codex 兼容轮次运行器。
    pub const fn new(store: S, hooks: H, model: M) -> Self {
        Self {
            runtime: CodexSessionRuntime::new(store, hooks),
            model,
        }
    }

    /// Returns a reference to the underlying runtime.
    /// 返回底层运行时引用。
    pub const fn runtime(&self) -> &CodexSessionRuntime<S, H> {
        &self.runtime
    }

    /// Starts one thread by appending a thread-started event and replacing thread state.
    /// 通过追加线程启动事件并覆盖线程状态来启动单个线程。
    pub fn start_thread(&self, thread_id: CodexThreadId, timestamp_ms: u64) -> Result<(), String> {
        let mut state = CodexThreadState::new(thread_id);
        let event = CodexSessionEvent::new(
            format!("thread-started-{thread_id}"),
            thread_id,
            state.take_sequence(),
            CodexSessionEventKind::ThreadStarted,
            CodexEventScope::Thread,
            CodexEventVisibility::Hidden,
            timestamp_ms,
        )
        .with_payload(json!({ "thread_id": thread_id.to_string() }));
        self.runtime.append_event(event)?;
        self.store_state(thread_id, &state)?;
        Ok(())
    }

    /// Runs one full turn using Codex append-only events and overwrite-only state.
    /// 使用 Codex 追加式事件与覆盖式状态运行一个完整轮次。
    pub fn run_turn(&self, request: CodexTurnRunRequest, timestamp_ms: u64) -> Result<(), String> {
        let mut state = self.load_or_create_state(request.thread_id)?;
        state.status = CodexThreadStatus::Running;
        state.active_turn_id = Some(request.turn_id.clone());
        let turn_started = CodexSessionEvent::new(
            format!("turn-started-{}", request.turn_id),
            request.thread_id,
            state.take_sequence(),
            CodexSessionEventKind::TurnStarted,
            CodexEventScope::Turn,
            CodexEventVisibility::Hidden,
            timestamp_ms,
        )
        .with_turn_id(request.turn_id.clone())
        .with_payload(json!({ "turn_id": request.turn_id }));
        self.runtime.append_event(turn_started)?;
        let user_input = CodexSessionEvent::new(
            format!("user-input-{}", request.turn_id),
            request.thread_id,
            state.take_sequence(),
            CodexSessionEventKind::UserInput,
            CodexEventScope::Turn,
            CodexEventVisibility::Visible,
            timestamp_ms,
        )
        .with_turn_id(request.turn_id.clone())
        .with_payload(json!({ "text": request.prompt }));
        self.runtime.append_event(user_input)?;
        self.store_state(request.thread_id, &state)?;

        let model_events = self.model.run_turn(
            request.thread_id,
            request.turn_id.clone(),
            request.model_input,
        )?;
        for event in model_events {
            let event = self.resequence_model_event(event, &mut state);
            self.runtime.append_event(event)?;
        }

        state.status = CodexThreadStatus::Idle;
        state.active_turn_id = None;
        state.last_turn_id = Some(request.turn_id.clone());
        let turn_completed = CodexSessionEvent::new(
            format!("turn-completed-{}", request.turn_id),
            request.thread_id,
            state.take_sequence(),
            CodexSessionEventKind::TurnCompleted,
            CodexEventScope::Turn,
            CodexEventVisibility::Hidden,
            timestamp_ms,
        )
        .with_turn_id(request.turn_id.clone())
        .with_payload(json!({
            "turn_id": request.turn_id,
            "status": CodexTurnTerminalStatus::Completed,
        }));
        self.runtime.append_event(turn_completed)?;
        self.store_state(request.thread_id, &state)?;
        Ok(())
    }

    /// Loads existing state or creates a new idle state.
    /// 读取现有状态或创建新的空闲状态。
    fn load_or_create_state(&self, thread_id: CodexThreadId) -> Result<CodexThreadState, String> {
        load_or_create_thread_state(&self.runtime, thread_id)
    }

    /// Stores one thread state as a JSON document.
    /// 将单个线程状态保存为 JSON 文档。
    fn store_state(
        &self,
        thread_id: CodexThreadId,
        state: &CodexThreadState,
    ) -> Result<(), String> {
        store_thread_state(&self.runtime, thread_id, state)
    }

    /// Assigns the thread-owned next sequence to one model event.
    /// 将线程拥有的下一条序号分配给单个模型事件。
    fn resequence_model_event(
        &self,
        event: CodexSessionEvent,
        state: &mut CodexThreadState,
    ) -> CodexSessionEvent {
        resequence_event(event, state)
    }
}

/// Loads existing thread state or creates a new idle state.
/// 读取现有线程状态或创建新的空闲状态。
fn load_or_create_thread_state<S, H>(
    runtime: &CodexSessionRuntime<S, H>,
    thread_id: CodexThreadId,
) -> Result<CodexThreadState, String>
where
    S: CodexSessionStore + CodexSessionStateStore,
    H: CodexHookAdapter,
{
    let Some(value) = runtime.store.load_state(thread_id)? else {
        return Ok(CodexThreadState::new(thread_id));
    };
    serde_json::from_value(value).map_err(|error| format!("invalid Codex thread state: {error}"))
}

/// Stores one thread state as a JSON document.
/// 将单个线程状态保存为 JSON 文档。
fn store_thread_state<S, H>(
    runtime: &CodexSessionRuntime<S, H>,
    thread_id: CodexThreadId,
    state: &CodexThreadState,
) -> Result<(), String>
where
    S: CodexSessionStore + CodexSessionStateStore,
    H: CodexHookAdapter,
{
    let value = serde_json::to_value(state)
        .map_err(|error| format!("failed to serialize Codex thread state: {error}"))?;
    runtime.store.replace_state(thread_id, value)
}

/// Assigns the next thread-owned sequence to one event.
/// 给单条事件分配下一条线程所属序号。
fn resequence_event(
    mut event: CodexSessionEvent,
    state: &mut CodexThreadState,
) -> CodexSessionEvent {
    event.sequence = state.take_sequence();
    event
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapter::{CodexHookAdapter, CodexModelAdapter};
    use crate::contract::{CodexEventVisibility, CodexSessionEventKind};
    use crate::memory::InMemoryCodexSessionStore;
    use crate::storage::{CodexSessionStateStore, CodexSessionStore};
    use std::sync::{Arc, Mutex};

    /// No-op hook adapter for runner tests.
    /// runner 测试使用的空 hook 适配器。
    struct NoopHook;

    impl CodexHookAdapter for NoopHook {
        /// Ignores appended events.
        /// 忽略已追加事件。
        fn after_event_appended(&self, _event: &CodexSessionEvent) -> Result<(), String> {
            Ok(())
        }
    }

    /// Model adapter that emits one visible assistant event.
    /// 发出单条可见助手事件的模型适配器。
    #[derive(Clone)]
    struct EchoModel {
        /// Recorded turn ids.
        /// 记录到的轮次标识符。
        turns: Arc<Mutex<Vec<CodexTurnId>>>,
    }

    impl CodexModelAdapter for EchoModel {
        /// Emits one assistant message event.
        /// 发出单条助手消息事件。
        fn run_turn(
            &self,
            thread_id: CodexThreadId,
            turn_id: CodexTurnId,
            _input: Value,
        ) -> Result<Vec<CodexSessionEvent>, String> {
            self.turns
                .lock()
                .map_err(|error| format!("failed to lock turns: {error}"))?
                .push(turn_id.clone());
            Ok(vec![
                CodexSessionEvent::new(
                    "assistant-raw",
                    thread_id,
                    999,
                    CodexSessionEventKind::AgentMessage,
                    CodexEventScope::Turn,
                    CodexEventVisibility::Visible,
                    1001,
                )
                .with_turn_id(turn_id)
                .with_payload(json!({ "text": "answer" })),
            ])
        }
    }

    /// Verifies the runner appends thread, turn, user, model, and completion events in sequence.
    /// 验证 runner 会按序追加线程、轮次、用户、模型和完成事件。
    #[test]
    fn runner_appends_turn_events_and_replaces_state() {
        let store = InMemoryCodexSessionStore::default();
        let model = EchoModel {
            turns: Arc::new(Mutex::new(Vec::new())),
        };
        let runner = CodexTurnRunner::new(store.clone(), NoopHook, model.clone());
        let thread_id = CodexThreadId::new_v4();

        runner
            .start_thread(thread_id, 1000)
            .expect("thread should start");
        runner
            .run_turn(
                CodexTurnRunRequest {
                    thread_id,
                    turn_id: String::from("turn-1"),
                    prompt: String::from("hello"),
                    model_input: json!({ "input": "hello" }),
                },
                1001,
            )
            .expect("turn should run");

        let events = store.read_events(thread_id).expect("events should read");
        let state: CodexThreadState = serde_json::from_value(
            store
                .load_state(thread_id)
                .expect("state should read")
                .expect("state should exist"),
        )
        .expect("state should deserialize");

        assert_eq!(
            events
                .iter()
                .map(|event| event.sequence)
                .collect::<Vec<_>>(),
            vec![1, 2, 3, 4, 5]
        );
        assert_eq!(events[2].kind, CodexSessionEventKind::UserInput);
        assert_eq!(events[3].kind, CodexSessionEventKind::AgentMessage);
        assert_eq!(events[4].kind, CodexSessionEventKind::TurnCompleted);
        assert_eq!(state.status, CodexThreadStatus::Idle);
        assert_eq!(state.active_turn_id, None);
        assert_eq!(state.last_turn_id.as_deref(), Some("turn-1"));
    }

    /// Verifies externally streamed turns use Codex-owned sequence and state updates.
    /// 验证外部流式轮次使用 Codex 拥有的序号与状态更新。
    #[test]
    fn external_stream_runner_resequences_host_events_and_replaces_state() {
        let store = InMemoryCodexSessionStore::default();
        let runner = CodexExternalStreamTurnRunner::new(store.clone(), NoopHook);
        let thread_id = CodexThreadId::new_v4();
        let accepted_events = vec![
            CodexSessionEvent::new(
                "host-turn-started",
                thread_id,
                0,
                CodexSessionEventKind::TurnStarted,
                CodexEventScope::Turn,
                CodexEventVisibility::Hidden,
                100,
            )
            .with_turn_id("turn-1"),
            CodexSessionEvent::new(
                "host-user-input",
                thread_id,
                1,
                CodexSessionEventKind::UserInput,
                CodexEventScope::Turn,
                CodexEventVisibility::Visible,
                101,
            )
            .with_turn_id("turn-1"),
        ];

        runner
            .begin_turn(CodexExternalTurnBeginRequest {
                thread_id,
                turn_id: String::from("turn-1"),
                accepted_events,
            })
            .expect("external turn should begin");
        runner
            .finish_turn(CodexExternalTurnFinishRequest {
                thread_id,
                turn_id: String::from("turn-1"),
                terminal_status: CodexTurnTerminalStatus::Completed,
                final_events: vec![
                    CodexSessionEvent::new(
                        "host-assistant-final",
                        thread_id,
                        999,
                        CodexSessionEventKind::AgentMessage,
                        CodexEventScope::Turn,
                        CodexEventVisibility::Visible,
                        150,
                    )
                    .with_turn_id("turn-1"),
                ],
                terminal_event: Some(
                    CodexSessionEvent::new(
                        "host-turn-completed",
                        thread_id,
                        2,
                        CodexSessionEventKind::TurnCompleted,
                        CodexEventScope::Turn,
                        CodexEventVisibility::Hidden,
                        200,
                    )
                    .with_turn_id("turn-1"),
                ),
            })
            .expect("external turn should finish");

        let events = store.read_events(thread_id).expect("events should load");
        assert_eq!(events.len(), 4);
        assert_eq!(
            events
                .iter()
                .map(|event| event.sequence)
                .collect::<Vec<_>>(),
            vec![1, 2, 3, 4]
        );
        assert_eq!(events[2].kind, CodexSessionEventKind::AgentMessage);
        assert_eq!(events[3].kind, CodexSessionEventKind::TurnCompleted);
        let state: CodexThreadState = serde_json::from_value(
            store
                .load_state(thread_id)
                .expect("state should load")
                .expect("state should exist"),
        )
        .expect("state should decode");
        assert_eq!(state.status, CodexThreadStatus::Idle);
        assert_eq!(state.active_turn_id, None);
        assert_eq!(state.last_turn_id.as_deref(), Some("turn-1"));
        assert_eq!(state.next_sequence, 5);
    }
}
