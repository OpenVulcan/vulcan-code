use crate::adapter::SessionLifecycleHook;
use crate::contract::{
    SessionBoundaryEvent, SessionBoundaryEventKind, SessionEventScope, SessionEventVisibility,
    SessionThreadId, SessionTurnId,
};
use crate::runtime::SessionBoundaryRuntime;
use crate::storage::{SessionBoundaryStateStore, SessionBoundaryStore};
use serde::{Deserialize, Serialize};
use serde_json::json;

/// Thread runtime status stored as overwrite-only session-boundary state.
/// 作为覆盖式 会话边界状态保存的线程运行状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionThreadStatus {
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

/// Terminal status for one session-boundary turn.
/// 单个 会话边界轮次的终态状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionTurnTerminalStatus {
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

/// Overwrite-only state document for one session-boundary thread.
/// 单个 会话边界线程的覆盖式状态文档。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionThreadState {
    /// Thread identifier.
    /// 线程标识符。
    pub thread_id: SessionThreadId,
    /// Next event sequence to append.
    /// 下一条待追加事件序号。
    pub next_sequence: u64,
    /// Current runtime status.
    /// 当前运行状态。
    pub status: SessionThreadStatus,
    /// Active turn identifier when a turn is running.
    /// 轮次运行中时的活动轮次标识符。
    pub active_turn_id: Option<SessionTurnId>,
    /// Last completed or failed turn identifier.
    /// 最近完成或失败的轮次标识符。
    pub last_turn_id: Option<SessionTurnId>,
}

impl SessionThreadState {
    /// Creates an initial state for one thread.
    /// 为单个线程创建初始状态。
    pub const fn new(thread_id: SessionThreadId) -> Self {
        Self {
            thread_id,
            next_sequence: 1,
            status: SessionThreadStatus::Idle,
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

/// Request used when an external streaming provider has accepted one turn.
/// 外部流式 provider 已接受一个轮次时使用的请求。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionLifecycleBeginRequest {
    /// Target thread identifier.
    /// 目标线程标识符。
    pub thread_id: SessionThreadId,
    /// Stable turn identifier.
    /// 稳定轮次标识符。
    pub turn_id: SessionTurnId,
    /// Already-built lifecycle events from the host-side precheck path.
    /// 宿主侧 precheck 路径已经构建好的生命周期事件。
    pub accepted_events: Vec<SessionBoundaryEvent>,
}

/// Request used when an external streaming provider reaches a terminal state.
/// 外部流式 provider 到达终态时使用的请求。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SessionLifecycleFinishRequest {
    /// Target thread identifier.
    /// 目标线程标识符。
    pub thread_id: SessionThreadId,
    /// Stable turn identifier.
    /// 稳定轮次标识符。
    pub turn_id: SessionTurnId,
    /// Terminal status reported by the external provider stream.
    /// 外部 provider 流报告的终态状态。
    pub terminal_status: SessionTurnTerminalStatus,
    /// Final turn events produced by the external provider before the terminal event.
    /// 外部 provider 在终态事件前产生的最终轮次事件。
    pub final_events: Vec<SessionBoundaryEvent>,
    /// Optional terminal event built by the host-side report path.
    /// 宿主侧报告路径构建的可选终态事件。
    pub terminal_event: Option<SessionBoundaryEvent>,
}

/// session lifecycle runner for externally streamed provider turns.
/// 面向外部流式 provider 轮次的 会话生命周期运行器。
pub struct SessionLifecycleRunner<S, H>
where
    S: SessionBoundaryStore + SessionBoundaryStateStore,
    H: SessionLifecycleHook,
{
    /// Event runtime that enforces append-before-hook ordering.
    /// 强制追加先于 hook 的事件运行时。
    runtime: SessionBoundaryRuntime<S, H>,
}

impl<S, H> SessionLifecycleRunner<S, H>
where
    S: SessionBoundaryStore + SessionBoundaryStateStore,
    H: SessionLifecycleHook,
{
    /// Creates a lifecycle runner for externally streamed turns.
    /// 创建面向外部流式轮次的生命周期运行器。
    pub const fn new(store: S, hooks: H) -> Self {
        Self {
            runtime: SessionBoundaryRuntime::new(store, hooks),
        }
    }

    /// Returns a reference to the underlying runtime.
    /// 返回底层运行时引用。
    pub const fn runtime(&self) -> &SessionBoundaryRuntime<S, H> {
        &self.runtime
    }

    /// Begins one externally streamed turn and appends accepted lifecycle events.
    /// 开始一个外部流式轮次，并追加已接受的生命周期事件。
    pub fn begin_turn(&self, request: SessionLifecycleBeginRequest) -> Result<(), String> {
        let mut state = load_or_create_thread_state(&self.runtime, request.thread_id)?;
        state.status = SessionThreadStatus::Running;
        state.active_turn_id = Some(request.turn_id.clone());
        for event in request.accepted_events {
            let event = resequence_event(event, &mut state);
            self.runtime.append_event(event)?;
        }
        store_thread_state(&self.runtime, request.thread_id, &state)
    }

    /// Finishes one externally streamed turn and appends one terminal event.
    /// 结束一个外部流式轮次，并追加一个终态事件。
    pub fn finish_turn(&self, request: SessionLifecycleFinishRequest) -> Result<(), String> {
        let mut state = load_or_create_thread_state(&self.runtime, request.thread_id)?;
        state.status = match request.terminal_status {
            SessionTurnTerminalStatus::Completed => SessionThreadStatus::Idle,
            SessionTurnTerminalStatus::Failed => SessionThreadStatus::Failed,
            SessionTurnTerminalStatus::Interrupted => SessionThreadStatus::Interrupted,
        };
        state.active_turn_id = None;
        state.last_turn_id = Some(request.turn_id.clone());
        for event in request.final_events {
            let event = resequence_event(event, &mut state);
            self.runtime.append_event(event)?;
        }
        let terminal_event = request.terminal_event.unwrap_or_else(|| {
            SessionBoundaryEvent::new(
                format!("turn-terminal-{}", request.turn_id),
                request.thread_id,
                0,
                SessionBoundaryEventKind::TurnCompleted,
                SessionEventScope::Turn,
                SessionEventVisibility::Hidden,
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

/// Loads existing thread state or creates a new idle state.
/// 读取现有线程状态或创建新的空闲状态。
fn load_or_create_thread_state<S, H>(
    runtime: &SessionBoundaryRuntime<S, H>,
    thread_id: SessionThreadId,
) -> Result<SessionThreadState, String>
where
    S: SessionBoundaryStore + SessionBoundaryStateStore,
    H: SessionLifecycleHook,
{
    let Some(value) = runtime.store.load_state(thread_id)? else {
        return Ok(SessionThreadState::new(thread_id));
    };
    serde_json::from_value(value)
        .map_err(|error| format!("invalid session boundary state: {error}"))
}

/// Stores one thread state as a JSON document.
/// 将单个线程状态保存为 JSON 文档。
fn store_thread_state<S, H>(
    runtime: &SessionBoundaryRuntime<S, H>,
    thread_id: SessionThreadId,
    state: &SessionThreadState,
) -> Result<(), String>
where
    S: SessionBoundaryStore + SessionBoundaryStateStore,
    H: SessionLifecycleHook,
{
    let value = serde_json::to_value(state)
        .map_err(|error| format!("failed to serialize session boundary state: {error}"))?;
    runtime.store.replace_state(thread_id, value)
}

/// Assigns the next thread-owned sequence to one event.
/// 给单条事件分配下一条线程所属序号。
fn resequence_event(
    mut event: SessionBoundaryEvent,
    state: &mut SessionThreadState,
) -> SessionBoundaryEvent {
    event.sequence = state.take_sequence();
    event
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapter::SessionLifecycleHook;
    use crate::contract::{SessionBoundaryEventKind, SessionEventVisibility};
    use crate::memory::InMemorySessionBoundaryStore;
    use crate::storage::{SessionBoundaryStateStore, SessionBoundaryStore};

    /// No-op hook adapter for runner tests.
    /// runner 测试使用的空 hook 适配器。
    struct NoopHook;

    impl SessionLifecycleHook for NoopHook {
        /// Ignores appended events.
        /// 忽略已追加事件。
        fn after_event_appended(&self, _event: &SessionBoundaryEvent) -> Result<(), String> {
            Ok(())
        }
    }

    /// Verifies externally streamed turns use session-boundary-owned sequence and state updates.
    /// 验证外部流式轮次使用 会话边界拥有的序号与状态更新。
    #[test]
    fn external_stream_runner_resequences_host_events_and_replaces_state() {
        let store = InMemorySessionBoundaryStore::default();
        let runner = SessionLifecycleRunner::new(store.clone(), NoopHook);
        let thread_id = SessionThreadId::new_v4();
        let accepted_events = vec![
            SessionBoundaryEvent::new(
                "host-turn-started",
                thread_id,
                0,
                SessionBoundaryEventKind::TurnStarted,
                SessionEventScope::Turn,
                SessionEventVisibility::Hidden,
                100,
            )
            .with_turn_id("turn-1"),
            SessionBoundaryEvent::new(
                "host-user-input",
                thread_id,
                1,
                SessionBoundaryEventKind::UserInput,
                SessionEventScope::Turn,
                SessionEventVisibility::Visible,
                101,
            )
            .with_turn_id("turn-1"),
        ];

        runner
            .begin_turn(SessionLifecycleBeginRequest {
                thread_id,
                turn_id: String::from("turn-1"),
                accepted_events,
            })
            .expect("external turn should begin");
        runner
            .finish_turn(SessionLifecycleFinishRequest {
                thread_id,
                turn_id: String::from("turn-1"),
                terminal_status: SessionTurnTerminalStatus::Completed,
                final_events: vec![
                    SessionBoundaryEvent::new(
                        "host-assistant-final",
                        thread_id,
                        999,
                        SessionBoundaryEventKind::AgentMessage,
                        SessionEventScope::Turn,
                        SessionEventVisibility::Visible,
                        150,
                    )
                    .with_turn_id("turn-1"),
                ],
                terminal_event: Some(
                    SessionBoundaryEvent::new(
                        "host-turn-completed",
                        thread_id,
                        2,
                        SessionBoundaryEventKind::TurnCompleted,
                        SessionEventScope::Turn,
                        SessionEventVisibility::Hidden,
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
        assert_eq!(events[2].kind, SessionBoundaryEventKind::AgentMessage);
        assert_eq!(events[3].kind, SessionBoundaryEventKind::TurnCompleted);
        let state: SessionThreadState = serde_json::from_value(
            store
                .load_state(thread_id)
                .expect("state should load")
                .expect("state should exist"),
        )
        .expect("state should decode");
        assert_eq!(state.status, SessionThreadStatus::Idle);
        assert_eq!(state.active_turn_id, None);
        assert_eq!(state.last_turn_id.as_deref(), Some("turn-1"));
        assert_eq!(state.next_sequence, 5);
    }
}
