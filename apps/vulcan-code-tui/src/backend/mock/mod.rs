// In-process mock backend used by the display demo.
// 展示 demo 使用的进程内模拟后端。
//
// It keeps an authoritative event log per conversation, like the service does, and replays scripted
// turns with realistic timing so every UI state (streaming thinking, running tools, pending asks,
// queued prompts, stop, Git actions) can be exercised without a service.
// 它像服务一样为每个会话维护权威事件日志，并按真实节奏回放脚本化轮次，使所有界面状态
// （流式思考、运行中工具、等待中的提问、排队提示、停止、Git 操作）无需服务即可演示。

#[cfg(test)]
mod answer_mode_tests;
mod files;
mod fixtures;
mod insight;
mod script;
mod service;

use std::collections::{HashMap, VecDeque};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use script::{Action, Script};

use super::{BackendEvent, BackendHandle, Command, ConnectionState, GitBusy, GitNotice, Notice};
use crate::model::*;
use crate::service_api::{
    CallKind, ServiceCall, Slot, workbench_snapshot::load_workbench_snapshot,
};

/// Starts the mock backend on its own thread.
/// 在独立线程上启动模拟后端。
pub fn spawn() -> BackendHandle {
    let (command_tx, command_rx) = mpsc::channel();
    let (event_tx, event_rx) = mpsc::channel();
    thread::Builder::new()
        .name("vulcan-mock-backend".into())
        .spawn(move || World::new(event_tx, Pacing::Live).run(command_rx))
        .expect("spawn mock backend thread");
    BackendHandle {
        commands: command_tx,
        events: event_rx,
        label: "mock".into(),
        is_mock: true,
    }
}

/// Starts a mock that the caller steps on its own thread, with every scripted delay already due.
/// 启动由调用方在自身线程上逐步驱动的模拟，所有脚本延迟均已到期。
/// Headless tests use it so each awaited state follows from events, never from wall-clock waits.
/// 无头测试使用它，使每个等待的状态都由事件推进，而非墙钟等待。
#[cfg(test)]
pub fn inline() -> (BackendHandle, Inline) {
    let (command_tx, command_rx) = mpsc::channel();
    let (event_tx, event_rx) = mpsc::channel();
    let mut world = World::new(event_tx, Pacing::Immediate);
    world.start();
    let handle = BackendHandle {
        commands: command_tx,
        events: event_rx,
        label: "mock".into(),
        is_mock: true,
    };
    (
        handle,
        Inline {
            world,
            commands: command_rx,
        },
    )
}

/// Caller-driven mock world; nothing happens between `next_event` calls.
/// 由调用方驱动的模拟世界；两次 `next_event` 之间不会发生任何事。
#[cfg(test)]
pub struct Inline {
    world: World,
    commands: Receiver<Command>,
}

#[cfg(test)]
impl Inline {
    /// Handles every queued command and all scripted work that has come due.
    /// 处理全部排队命令及所有已到期的脚本工作。
    /// Returns whether anything ran; `false` means the mock is quiet until the next command.
    /// 返回是否执行了任何工作；`false` 表示在下一条命令之前模拟保持静止。
    fn step(&mut self) -> bool {
        let mut progressed = false;
        while let Ok(command) = self.commands.try_recv() {
            self.world.handle(command);
            progressed = true;
        }
        while self
            .world
            .next_deadline()
            .is_some_and(|due| due <= Instant::now())
        {
            self.world.advance(Instant::now());
            progressed = true;
        }
        progressed
    }

    /// Returns the next event for `events`, stepping the mock whenever its outbox is empty.
    /// 返回 `events` 的下一条事件，发件箱为空时推进模拟。
    /// `None` means the mock has nothing more to say until the app sends another command.
    /// `None` 表示在应用发送下一条命令之前模拟不会再产生事件。
    pub fn next_event(&mut self, events: &Receiver<BackendEvent>) -> Option<BackendEvent> {
        loop {
            if let Ok(event) = events.try_recv() {
                return Some(event);
            }
            if !self.step() {
                return None;
            }
        }
    }
}

/// How scripted work is spaced in time.
/// 脚本化工作在时间上的间隔方式。
#[derive(Clone, Copy)]
enum Pacing {
    /// Demo cadence that reads like a live service.
    /// 读起来像真实服务的演示节奏。
    Live,
    /// Every scripted delay elapses at once.
    /// 所有脚本延迟立即到期。
    #[cfg(test)]
    Immediate,
}

impl Pacing {
    /// Returns the wait this pacing applies to one scripted `delay`.
    /// 返回此节奏对一个脚本 `delay` 实际施加的等待。
    fn delay(self, delay: Duration) -> Duration {
        match self {
            Pacing::Live => delay,
            #[cfg(test)]
            Pacing::Immediate => Duration::ZERO,
        }
    }
}

#[derive(Default)]
struct ConversationLog {
    /// Simulated session-owned answer mode, independent from the plan mode.
    /// 模拟的会话所有回答模式，独立于计划模式。
    answer_mode: AnswerModeSession,
    /// Simulated saved permission mode.
    /// 模拟的已保存权限模式。
    permission_mode: PermissionMode,
    events: Vec<ConversationEvent>,
    next_seq: u64,
    pending_question: Option<PendingQuestion>,
    pending_prompts: Vec<PendingPrompt>,
    todo: Vec<TodoItem>,
    usage: ContextUsage,
    agents: Vec<AgentInfo>,
    mode: Option<ExecutionMode>,
    insight: SessionInsight,
}

struct Run {
    steps: VecDeque<(Duration, Action)>,
    next_at: Instant,
    blocked_on: Option<(String, u64)>,
    started: Instant,
    turn_start_seq: u64,
    tool_calls: u32,
    mode: ExecutionMode,
    /// Usage already booked into the lifetime while the turn ran, one request per settled block.
    /// 轮次运行期间已记入生命周期用量的部分，每个结束的块计一次请求。
    booked: UsageBucket,
}

enum Timer {
    Connect,
    CommitMessage(VecDeque<String>),
    FinishCommit {
        message: String,
        new_branch: Option<String>,
        include_unstaged: bool,
        push: bool,
    },
    FinishPush,
}

struct World {
    out: Sender<BackendEvent>,
    pacing: Pacing,
    silent: bool,
    conversations: Vec<ConversationSummary>,
    logs: HashMap<String, ConversationLog>,
    runs: HashMap<String, Run>,
    selected: Option<String>,
    git: GitSnapshot,
    created_branches: Vec<String>,
    sessions: Vec<CommandSession>,
    timers: Vec<(Instant, Timer)>,
    turn_counter: u64,
    max_tokens: u64,
    projects: Vec<Project>,
    /// Summaries of archived conversations, restored as they were.
    /// 已归档会话的摘要，恢复时原样还原。
    archived_rows: HashMap<String, ConversationSummary>,
    next_id: u64,
    service: service::FakeService,
    /// Last loader per slot, so mutations can reload what they invalidate.
    /// 每个槽位最近一次的加载器，供变更重新加载其失效的槽位。
    loaders: HashMap<Slot, ServiceCall>,
}

impl World {
    fn new(out: Sender<BackendEvent>, pacing: Pacing) -> Self {
        World {
            out,
            pacing,
            silent: false,
            conversations: fixtures::conversations(),
            logs: HashMap::new(),
            runs: HashMap::new(),
            selected: Some("c-tui".into()),
            git: fixtures::git_snapshot(),
            created_branches: Vec::new(),
            sessions: fixtures::command_sessions(),
            timers: Vec::new(),
            turn_counter: 0,
            max_tokens: 400_000,
            projects: fixtures::projects(),
            archived_rows: HashMap::new(),
            next_id: 1,
            service: service::FakeService::new(),
            loaders: HashMap::new(),
        }
    }

    /// Announces the connection attempt, seeds history and schedules the simulated connect.
    /// 宣告连接尝试、预填历史并安排模拟连接。
    fn start(&mut self) {
        self.emit(BackendEvent::Connection(ConnectionState::Connecting));
        self.seed();
        self.timers
            .push((self.after(Duration::from_millis(1400)), Timer::Connect));
    }

    /// Returns when a timer scheduled `delay` from now falls due under this world's pacing.
    /// 返回在本世界节奏下，从现在起 `delay` 后安排的计时器何时到期。
    fn after(&self, delay: Duration) -> Instant {
        Instant::now() + self.pacing.delay(delay)
    }

    fn run(mut self, commands: Receiver<Command>) {
        self.start();
        loop {
            let now = Instant::now();
            let next = self
                .next_deadline()
                .unwrap_or(now + Duration::from_millis(250));
            let wait = next
                .saturating_duration_since(now)
                .min(Duration::from_millis(250));
            match commands.recv_timeout(wait) {
                Ok(command) => self.handle(command),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return,
            }
            self.advance(Instant::now());
        }
    }

    fn emit(&self, event: BackendEvent) {
        if !self.silent {
            let _ = self.out.send(event);
        }
    }

    fn next_deadline(&self) -> Option<Instant> {
        let runs = self
            .runs
            .values()
            .filter(|run| run.blocked_on.is_none() && !run.steps.is_empty())
            .map(|run| run.next_at);
        let timers = self.timers.iter().map(|(at, _)| *at);
        runs.chain(timers).min()
    }

    /// Pre-fills history silently so the first hydration already shows a finished turn.
    /// 静默预填历史，使首次加载即显示已完成的轮次。
    fn seed(&mut self) {
        self.silent = true;
        let ids: Vec<String> = self.conversations.iter().map(|c| c.id.clone()).collect();
        for id in ids {
            self.ensure_log(&id);
        }
        if let Some(log) = self.logs.get_mut("c-tui") {
            log.todo = fixtures::initial_todo();
            log.usage.used_tokens = 64_200;
        }
        self.start_turn(
            "c-tui",
            "幫我建立 TUI 的顯示框架，先用 mock 資料模擬思考、輸出與工具呼叫",
            UserInputSource::Root,
            ExecutionMode::Build,
            true,
        );
        let mut seed = self.new_script("c-ask");
        script::ask_seed_turn(&mut seed);
        self.start_script("c-ask", seed, ExecutionMode::Build, true);
        // The demo conversation's insight replaces what the seeded turn booked, keeping the
        // subagents that turn created.
        // 演示会话的洞察替换种子轮次记下的内容，但保留该轮创建的子代理。
        if let Some(log) = self.logs.get_mut("c-tui") {
            log.insight = insight::seeded(log.usage.used_tokens);
            log.agents = script::seed_agents();
            insight::sync_children(&mut log.insight, &log.agents);
        }
        self.silent = false;
    }

    fn ensure_log(&mut self, conversation_id: &str) {
        let max_tokens = self.max_tokens;
        self.logs
            .entry(conversation_id.into())
            .or_insert_with(|| ConversationLog {
                next_seq: 1,
                usage: ContextUsage {
                    used_tokens: 18_000,
                    max_tokens,
                },
                insight: insight::baseline(18_000),
                ..ConversationLog::default()
            });
    }

    fn fresh_id(&mut self, prefix: &str) -> String {
        self.next_id += 1;
        format!("{prefix}-{}", self.next_id)
    }

    fn new_script(&mut self, conversation_id: &str) -> Script {
        self.turn_counter += 1;
        let next_seq = self.logs.get(conversation_id).map_or(1, |log| log.next_seq);
        Script::new(next_seq, format!("t{}", self.turn_counter))
    }

    fn start_turn(
        &mut self,
        conversation_id: &str,
        prompt: &str,
        source: UserInputSource,
        mode: ExecutionMode,
        instant: bool,
    ) {
        self.start_turn_with(conversation_id, prompt, source, mode, instant, &[]);
    }

    fn start_turn_with(
        &mut self,
        conversation_id: &str,
        prompt: &str,
        source: UserInputSource,
        mode: ExecutionMode,
        instant: bool,
        attachments: &[String],
    ) {
        let mut turn = self.new_script(conversation_id);
        script::build_turn(&mut turn, prompt, source, mode, attachments);
        self.start_script(conversation_id, turn, mode, instant);
    }

    fn start_script(
        &mut self,
        conversation_id: &str,
        turn: Script,
        mode: ExecutionMode,
        instant: bool,
    ) {
        let log = self.logs.entry(conversation_id.into()).or_default();
        let turn_start_seq = log.next_seq;
        log.next_seq = turn.next_seq;
        let now = Instant::now();
        let first_delay = self
            .pacing
            .delay(turn.steps.front().map_or(Duration::ZERO, |(d, _)| *d));
        self.runs.insert(
            conversation_id.into(),
            Run {
                steps: turn.steps,
                next_at: now + first_delay,
                blocked_on: None,
                started: now,
                turn_start_seq,
                tool_calls: turn.tool_calls,
                mode,
                booked: UsageBucket::default(),
            },
        );
        self.set_phase(conversation_id, RunPhase::Running);
        if instant {
            self.drain_instantly(conversation_id);
        }
    }

    fn drain_instantly(&mut self, conversation_id: &str) {
        loop {
            let Some(run) = self.runs.get_mut(conversation_id) else {
                return;
            };
            if run.blocked_on.is_some() {
                return;
            }
            let Some((_, action)) = run.steps.pop_front() else {
                return;
            };
            self.execute(conversation_id, action, true);
        }
    }

    fn advance(&mut self, now: Instant) {
        let pacing = self.pacing;
        let due: Vec<String> = self
            .runs
            .iter()
            .filter(|(_, run)| {
                run.blocked_on.is_none() && run.next_at <= now && !run.steps.is_empty()
            })
            .map(|(id, _)| id.clone())
            .collect();
        for id in due {
            while let Some(run) = self.runs.get_mut(&id) {
                if run.blocked_on.is_some() || run.next_at > now {
                    break;
                }
                let Some((_, action)) = run.steps.pop_front() else {
                    break;
                };
                let next_delay =
                    pacing.delay(run.steps.front().map_or(Duration::ZERO, |(d, _)| *d));
                run.next_at += next_delay;
                if now.saturating_duration_since(run.next_at) > Duration::from_millis(200) {
                    run.next_at = now;
                }
                self.execute(&id, action, false);
            }
        }
        let mut index = 0;
        while index < self.timers.len() {
            if self.timers[index].0 <= now {
                let (_, timer) = self.timers.remove(index);
                self.fire(timer);
            } else {
                index += 1;
            }
        }
    }

    fn now_ms() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_millis() as u64)
    }

    fn upsert(&mut self, conversation_id: &str, seq: u64, body: EventBody) {
        let now = Self::now_ms();
        let log = self.logs.entry(conversation_id.into()).or_default();
        let event = match log.events.iter_mut().find(|e| e.seq == seq) {
            Some(existing) => {
                existing.body = body;
                existing.updated_at_ms = now;
                existing.clone()
            }
            None => {
                let event = ConversationEvent {
                    seq,
                    created_at_ms: now,
                    updated_at_ms: now,
                    body,
                };
                log.events.push(event.clone());
                event
            }
        };
        self.emit(BackendEvent::EventUpsert {
            conversation_id: conversation_id.into(),
            event,
        });
    }

    fn body_of(&self, conversation_id: &str, seq: u64) -> Option<EventBody> {
        self.logs
            .get(conversation_id)?
            .events
            .iter()
            .find(|e| e.seq == seq)
            .map(|e| e.body.clone())
    }

    fn execute(&mut self, conversation_id: &str, action: Action, instant: bool) {
        match action {
            Action::Upsert { seq, mut body } => {
                if let EventBody::ToolCall { started_at_ms, .. } = &mut body {
                    *started_at_ms = Some(Self::now_ms());
                }
                self.upsert(conversation_id, seq, body);
            }
            Action::Append { seq, text } => {
                if let Some(log) = self.logs.get_mut(conversation_id)
                    && let Some(event) = log.events.iter_mut().find(|e| e.seq == seq)
                {
                    match &mut event.body {
                        EventBody::AssistantReasoning { text: body, .. }
                        | EventBody::AssistantText { text: body, .. } => body.push_str(&text),
                        _ => {}
                    }
                }
                if let Some(log) = self.logs.get_mut(conversation_id) {
                    log.usage.used_tokens += (text.len() as u64 / 3).max(1);
                }
                self.emit(BackendEvent::TextAppend {
                    conversation_id: conversation_id.into(),
                    seq,
                    text,
                });
            }
            Action::Settle { seq } => {
                if let Some(mut body) = self.body_of(conversation_id, seq) {
                    let mut output = None;
                    match &mut body {
                        EventBody::AssistantReasoning { status, text, .. }
                        | EventBody::AssistantText { status, text, .. } => {
                            *status = AssistantSemanticStatus::Completed;
                            output = Some(text.len() as u64 / 3);
                        }
                        _ => {}
                    }
                    self.upsert(conversation_id, seq, body);
                    if let Some(output) = output {
                        self.book(conversation_id, 1, 0, output);
                    }
                }
                self.emit_usage(conversation_id);
            }
            Action::ToolDone {
                seq,
                status: done,
                output: text,
                changeset: files,
            } => {
                if let Some(mut body) = self.body_of(conversation_id, seq) {
                    if let EventBody::ToolCall {
                        status,
                        output,
                        changeset,
                        finished_at_ms,
                        ..
                    } = &mut body
                    {
                        *status = done;
                        *output = Some(text);
                        *changeset = files;
                        *finished_at_ms = Some(Self::now_ms());
                    }
                    self.upsert(conversation_id, seq, body);
                    self.book(conversation_id, 0, 1, 0);
                }
            }
            Action::AwaitAnswer { tool_seq, question } => {
                if let Some(mut body) = self.body_of(conversation_id, tool_seq) {
                    if let EventBody::ToolCall { status, .. } = &mut body {
                        *status = ToolCallStatus::WaitingUser;
                    }
                    self.upsert(conversation_id, tool_seq, body);
                }
                if let Some(run) = self.runs.get_mut(conversation_id) {
                    run.blocked_on = Some((question.id.clone(), tool_seq));
                }
                if let Some(log) = self.logs.get_mut(conversation_id) {
                    log.pending_question = Some(question.clone());
                }
                self.emit(BackendEvent::PendingQuestion {
                    conversation_id: conversation_id.into(),
                    question: Some(question),
                });
                self.set_attention(conversation_id, true);
            }
            Action::Todo(items) => {
                if let Some(log) = self.logs.get_mut(conversation_id) {
                    log.todo = items.clone();
                }
                self.emit(BackendEvent::Todo {
                    conversation_id: conversation_id.into(),
                    items,
                });
            }
            Action::GitTouch(file) => {
                match self.git.files.iter_mut().find(|f| f.path == file.path) {
                    Some(existing) => {
                        existing.additions += file.additions;
                        existing.deletions += file.deletions;
                        existing.staged = false;
                    }
                    None => self.git.files.push(file),
                }
                self.emit(BackendEvent::Git(self.git.clone()));
            }
            Action::Terminal {
                id,
                title,
                command,
                line,
                done,
            } => {
                let index = match self.sessions.iter().position(|s| s.id == id) {
                    Some(index) => index,
                    None => {
                        self.sessions.insert(
                            0,
                            CommandSession {
                                id: id.clone(),
                                title,
                                command,
                                running: true,
                                exit_code: None,
                                screen: Vec::new(),
                            },
                        );
                        0
                    }
                };
                let session = &mut self.sessions[index];
                if let Some(line) = line {
                    session.screen.push(line);
                }
                if let Some(code) = done {
                    session.running = false;
                    session.exit_code = Some(code);
                }
                self.emit(BackendEvent::CommandSessions(self.sessions.clone()));
            }
            Action::Agents(agents) => {
                let Some(log) = self.logs.get_mut(conversation_id) else {
                    return;
                };
                // A stop or end from the user outlives the script; queued messages stay until
                // the agent finishes its current task.
                // 用户的停止或结束优先于脚本；排队消息保留到代理完成当前任务。
                let agents: Vec<AgentInfo> = agents
                    .into_iter()
                    .map(|mut next| {
                        if let Some(prev) = log.agents.iter().find(|a| a.id == next.id) {
                            if matches!(prev.state, AgentState::Stopped | AgentState::Closed) {
                                next.state = prev.state;
                                next.result = prev.result.clone();
                                next.activity = prev.activity.clone();
                            }
                            if next.state == AgentState::Running {
                                next.pending = prev.pending.clone();
                            }
                        }
                        next
                    })
                    .collect();
                log.agents = agents.clone();
                insight::sync_children(&mut log.insight, &agents);
                self.emit(BackendEvent::Agents {
                    conversation_id: conversation_id.into(),
                    agents,
                });
                self.emit_insight(conversation_id);
            }
            Action::Phase(phase) => self.set_phase(conversation_id, phase),
            Action::Note { severity, message } => {
                let seq = self.reserve_seq(conversation_id);
                self.upsert(
                    conversation_id,
                    seq,
                    EventBody::DisplayNote { severity, message },
                );
            }
            Action::Complete { duration_ms } => {
                self.finish_turn(
                    conversation_id,
                    TurnOutcome::Completed,
                    duration_ms,
                    instant,
                );
            }
        }
    }

    /// Books usage of the running turn into the lifetime right away, as the service reports each
    /// finished provider request. A request re-sends the whole used context as input.
    /// 立即将运行中轮次的用量记入生命周期，对应服务在每个提供方请求结束时的上报。每个请求都会把
    /// 已用上下文整体作为输入重新发送。
    fn book(&mut self, conversation_id: &str, requests: u64, tool_calls: u64, output: u64) {
        let Some(log) = self.logs.get_mut(conversation_id) else {
            return;
        };
        let Some(run) = self.runs.get_mut(conversation_id) else {
            return;
        };
        let input = if requests > 0 {
            log.usage.used_tokens
        } else {
            0
        };
        let bucket = insight::usage(requests, tool_calls, input, output);
        log.insight.lifetime.main.add(&bucket);
        run.booked.add(&bucket);
        self.emit_insight(conversation_id);
    }

    fn reserve_seq(&mut self, conversation_id: &str) -> u64 {
        let log = self.logs.entry(conversation_id.into()).or_default();
        let seq = log.next_seq;
        log.next_seq += 1;
        seq
    }

    fn finish_turn(
        &mut self,
        conversation_id: &str,
        outcome: TurnOutcome,
        duration_ms: Option<u64>,
        instant: bool,
    ) {
        let Some(run) = self.runs.remove(conversation_id) else {
            return;
        };
        let duration = duration_ms.unwrap_or(if instant {
            42_000
        } else {
            run.started.elapsed().as_millis() as u64
        });
        // Turns that booked requests while running report exactly that; a turn stopped before any
        // request settled books one estimated request now.
        // 运行中已记账请求的轮次如实报告；在任何请求结束前就被停止的轮次，此时补记一次估算请求。
        let mut turn = run.booked;
        if turn.requests == 0
            && let Some(log) = self.logs.get_mut(conversation_id)
        {
            let output = log
                .events
                .iter()
                .filter(|e| e.seq >= run.turn_start_seq)
                .map(|e| match &e.body {
                    EventBody::AssistantReasoning { text, .. }
                    | EventBody::AssistantText { text, .. } => text.len() as u64 / 3,
                    _ => 0,
                })
                .sum::<u64>();
            let late = insight::usage(1, 0, log.usage.used_tokens, output);
            log.insight.lifetime.main.add(&late);
            turn.add(&late);
        }
        if let Some(log) = self.logs.get_mut(conversation_id) {
            insight::finish_turn(&mut log.insight, &turn, duration, log.usage.used_tokens);
        }
        self.emit_insight(conversation_id);
        let seq = self.reserve_seq(conversation_id);
        self.upsert(
            conversation_id,
            seq,
            EventBody::TurnStats {
                outcome,
                duration_ms: duration,
                total_tokens: turn.total(),
                requests: turn.requests as u32,
                tool_calls: run.tool_calls,
            },
        );
        let phase = match outcome {
            TurnOutcome::Completed => RunPhase::Completed,
            _ => RunPhase::Idle,
        };
        self.set_phase(conversation_id, phase);
        if self.selected.as_deref() != Some(conversation_id)
            && let Some(row) = self
                .conversations
                .iter_mut()
                .find(|c| c.id == conversation_id)
        {
            row.unread = true;
            let row = row.clone();
            self.emit(BackendEvent::ConversationSummary(row));
        }
        if outcome == TurnOutcome::Completed {
            self.start_next_queued(conversation_id, run.mode);
        }
    }

    fn start_next_queued(&mut self, conversation_id: &str, mode: ExecutionMode) {
        let next = self.logs.get_mut(conversation_id).and_then(|log| {
            (!log.pending_prompts.is_empty()).then(|| log.pending_prompts.remove(0))
        });
        if let Some(prompt) = next {
            self.emit_prompts(conversation_id);
            self.start_turn(
                conversation_id,
                &prompt.preview,
                UserInputSource::QueuedScheduled,
                mode,
                false,
            );
        }
    }

    fn set_phase(&mut self, conversation_id: &str, phase: RunPhase) {
        if let Some(row) = self
            .conversations
            .iter_mut()
            .find(|c| c.id == conversation_id)
        {
            row.run_phase = phase;
            row.last_updated_label = "now".into();
            if !phase.is_active() {
                row.attention = false;
            }
            let row = row.clone();
            self.emit(BackendEvent::ConversationSummary(row));
        }
    }

    fn set_attention(&mut self, conversation_id: &str, attention: bool) {
        if let Some(row) = self
            .conversations
            .iter_mut()
            .find(|c| c.id == conversation_id)
        {
            row.attention = attention;
            let row = row.clone();
            self.emit(BackendEvent::ConversationSummary(row));
        }
    }

    fn emit_usage(&self, conversation_id: &str) {
        if let Some(log) = self.logs.get(conversation_id) {
            self.emit(BackendEvent::ContextUsage {
                conversation_id: conversation_id.into(),
                usage: log.usage,
            });
        }
    }

    fn emit_insight(&self, conversation_id: &str) {
        if let Some(log) = self.logs.get(conversation_id) {
            self.emit(BackendEvent::SessionInsight {
                conversation_id: conversation_id.into(),
                insight: Box::new(log.insight.clone()),
            });
        }
    }

    fn emit_agents(&self, conversation_id: &str) {
        if let Some(log) = self.logs.get(conversation_id) {
            self.emit(BackendEvent::Agents {
                conversation_id: conversation_id.into(),
                agents: log.agents.clone(),
            });
        }
    }

    fn emit_prompts(&self, conversation_id: &str) {
        if let Some(log) = self.logs.get(conversation_id) {
            self.emit(BackendEvent::PendingPrompts {
                conversation_id: conversation_id.into(),
                prompts: log.pending_prompts.clone(),
            });
        }
    }

    fn hydrate(&self, conversation_id: &str) {
        let Some(log) = self.logs.get(conversation_id) else {
            return;
        };
        let id = conversation_id.to_string();
        self.emit(BackendEvent::ConversationHydrated {
            conversation_id: id.clone(),
            events: log.events.clone(),
        });
        let mut answer_mode = log.answer_mode.clone();
        answer_mode
            .mode
            .get_or_insert_with(AnswerModePreference::default);
        self.emit(BackendEvent::AnswerMode {
            conversation_id: id.clone(),
            state: answer_mode,
        });
        self.emit(BackendEvent::PermissionMode {
            conversation_id: id.clone(),
            mode: log.permission_mode,
        });
        self.emit(BackendEvent::PendingQuestion {
            conversation_id: id.clone(),
            question: log.pending_question.clone(),
        });
        self.emit_prompts(conversation_id);
        self.emit(BackendEvent::Todo {
            conversation_id: id.clone(),
            items: log.todo.clone(),
        });
        self.emit_agents(conversation_id);
        self.emit(BackendEvent::PlanMode {
            conversation_id: id,
            mode: log.mode.unwrap_or(ExecutionMode::Build),
        });
        self.emit_usage(conversation_id);
        self.emit_insight(conversation_id);
    }

    fn fire(&mut self, timer: Timer) {
        match timer {
            Timer::Connect => {
                self.emit(BackendEvent::Workbench(WorkbenchSnapshot {
                    projects: self.projects.clone(),
                    conversations: self.conversations.clone(),
                    selected_conversation_id: self.selected.clone(),
                    models: fixtures::models(),
                    selected_model_id: "claude-opus-5-5".into(),
                    ui_locale: None,
                    permission_judge_state: None,
                    base_url: fixtures::BASE_URL.into(),
                }));
                self.emit(BackendEvent::Connection(ConnectionState::Connected));
                self.emit(BackendEvent::Git(self.git.clone()));
                self.emit(BackendEvent::CommandSessions(self.sessions.clone()));
                self.emit_workbench_slot();
                if let Some(selected) = self.selected.clone() {
                    self.hydrate(&selected);
                }
                let mut background = self.new_script("c-git");
                script::background_turn(&mut background);
                self.start_script("c-git", background, ExecutionMode::Build, false);
            }
            Timer::CommitMessage(mut chunks) => match chunks.pop_front() {
                Some(chunk) => {
                    self.emit(BackendEvent::CommitMessage {
                        text: chunk,
                        done: false,
                    });
                    self.timers.push((
                        self.after(Duration::from_millis(35)),
                        Timer::CommitMessage(chunks),
                    ));
                }
                None => {
                    self.emit(BackendEvent::CommitMessage {
                        text: String::new(),
                        done: true,
                    });
                    self.emit(BackendEvent::GitBusy(None));
                }
            },
            Timer::FinishCommit {
                message,
                new_branch,
                include_unstaged,
                push,
            } => {
                if let Some(branch) = new_branch {
                    self.created_branches.push(branch.clone());
                    self.git.branch = branch;
                    self.git.upstream = None;
                    self.git.ahead = 0;
                    self.git.unpushed.clear();
                }
                self.git.files.retain(|f| !(f.staged || include_unstaged));
                let subject = message.lines().next().unwrap_or("update").to_string();
                let sha = format!("{:07x}", (Self::now_ms() * 2654435761) & 0xfff_ffff);
                self.git.unpushed.insert(
                    0,
                    GitCommit {
                        short_sha: sha,
                        subject,
                    },
                );
                self.git.ahead += 1;
                self.emit(BackendEvent::Git(self.git.clone()));
                self.emit(BackendEvent::GitNotice(GitNotice::Committed));
                if push {
                    self.emit(BackendEvent::GitBusy(Some(GitBusy::Pushing)));
                    self.timers
                        .push((self.after(Duration::from_millis(1300)), Timer::FinishPush));
                } else {
                    self.emit(BackendEvent::GitBusy(None));
                }
            }
            Timer::FinishPush => {
                if self.git.upstream.is_none() {
                    self.git.upstream = Some(format!("origin/{}", self.git.branch));
                }
                self.git.ahead = 0;
                self.git.unpushed.clear();
                self.emit(BackendEvent::Git(self.git.clone()));
                self.emit(BackendEvent::GitBusy(None));
                self.emit(BackendEvent::GitNotice(GitNotice::Pushed));
            }
        }
    }

    fn handle(&mut self, command: Command) {
        match command {
            Command::SetSessionPermissionMode(request) => {
                let Some(log) = self.logs.get_mut(&request.session_id) else {
                    return;
                };
                log.permission_mode = request.mode;
                self.emit(BackendEvent::PermissionMode {
                    conversation_id: request.session_id,
                    mode: request.mode,
                });
            }
            Command::SetSessionAnswerMode(request) => {
                let Some(log) = self.logs.get_mut(&request.session_id) else {
                    return;
                };
                let mode = log
                    .answer_mode
                    .mode
                    .get_or_insert_with(AnswerModePreference::default);
                let conflict = mode.revision != request.expected_revision;
                if !conflict && mode.mode != request.mode {
                    mode.mode = request.mode;
                    mode.revision += 1;
                }
                self.hydrate(&request.session_id);
                if conflict {
                    self.emit(BackendEvent::Toast {
                        severity: NoticeSeverity::Warning,
                        message: crate::tr!("answerMode.conflict"),
                    });
                }
            }
            Command::SelectConversation(id) => {
                self.selected = Some(id.clone());
                if let Some(row) = self.conversations.iter_mut().find(|c| c.id == id) {
                    row.unread = false;
                    let row = row.clone();
                    self.emit(BackendEvent::ConversationSummary(row));
                }
                self.hydrate(&id);
            }
            Command::NewConversation { project_id } => {
                let id = self.fresh_id("c-new");
                let row = ConversationSummary {
                    id: id.clone(),
                    title: "New conversation".into(),
                    project_id,
                    run_phase: RunPhase::Idle,
                    last_updated_label: "now".into(),
                    unread: false,
                    attention: false,
                };
                self.conversations.insert(0, row.clone());
                self.ensure_log(&id);
                // New conversations copy the global default once, as the service does on creation.
                // 新会话与服务创建时一致，一次性复制全局默认。
                let default_mode = serde_json::from_value::<AnswerMode>(
                    self.service.workbench["defaultAnswerMode"].clone(),
                )
                .unwrap_or_default();
                if let Some(log) = self.logs.get_mut(&id) {
                    log.usage.used_tokens = 12_400;
                    log.answer_mode.mode = Some(AnswerModePreference {
                        mode: default_mode,
                        ..AnswerModePreference::default()
                    });
                }
                self.emit(BackendEvent::ConversationSummary(row));
                self.emit(BackendEvent::Selected(id));
            }
            Command::SubmitConversationPrompt {
                conversation_id,
                prompt,
                mode,
                attachments,
                ..
            } => {
                if let Some(row) = self
                    .conversations
                    .iter_mut()
                    .find(|c| c.id == conversation_id && c.title == "New conversation")
                {
                    row.title = script::truncate(&prompt, 18);
                    let row = row.clone();
                    self.emit(BackendEvent::ConversationSummary(row));
                }
                if self.runs.contains_key(&conversation_id) {
                    let log = self.logs.entry(conversation_id.clone()).or_default();
                    log.pending_prompts.push(PendingPrompt { preview: prompt });
                    self.emit_prompts(&conversation_id);
                } else {
                    self.start_turn_with(
                        &conversation_id,
                        &prompt,
                        UserInputSource::Root,
                        mode,
                        false,
                        &attachments,
                    );
                }
            }
            Command::StopConversationRun { conversation_id } => self.stop(&conversation_id, None),
            Command::ReplyPendingQuestion {
                request_id,
                answers,
            } => {
                self.answer(&request_id, Some(answers));
            }
            Command::RejectPendingQuestion { request_id } => self.answer(&request_id, None),
            Command::SetSessionPlanMode {
                conversation_id,
                enabled,
            } => {
                let mode = if enabled {
                    ExecutionMode::Plan
                } else {
                    ExecutionMode::Build
                };
                if let Some(log) = self.logs.get_mut(&conversation_id) {
                    log.mode = Some(mode);
                }
                self.emit(BackendEvent::PlanMode {
                    conversation_id,
                    mode,
                });
            }
            Command::LoadWorkspaceDiffSnapshot => self.emit(BackendEvent::Git(self.git.clone())),
            Command::SelectGitRepository(key) => {
                if self
                    .git
                    .repositories
                    .iter()
                    .any(|r| r.monitoring && r.key == key)
                {
                    self.git.selected = Some(key);
                }
                self.emit(BackendEvent::Git(self.git.clone()));
            }
            Command::SetGitMonitoring {
                repository_key,
                enabled,
            } => {
                for repository in &mut self.git.repositories {
                    if repository.key == repository_key {
                        repository.monitoring = enabled;
                    }
                }
                // Same fallback as the service worker: keep, then primary, then first monitored.
                // 与服务工作线程相同的回退：保留、主仓库、第一个受监听仓库。
                let repositories = &self.git.repositories;
                let monitored =
                    |key: &String| repositories.iter().any(|r| r.monitoring && &r.key == key);
                self.git.selected = self
                    .git
                    .selected
                    .clone()
                    .filter(monitored)
                    .or_else(|| {
                        repositories
                            .iter()
                            .find(|r| r.primary)
                            .map(|r| r.key.clone())
                            .filter(monitored)
                    })
                    .or_else(|| {
                        repositories
                            .iter()
                            .find(|r| r.monitoring)
                            .map(|r| r.key.clone())
                    });
                self.emit(BackendEvent::Git(self.git.clone()));
            }
            Command::RunWorkspaceGitFileAction { paths, stage } => {
                for file in self
                    .git
                    .files
                    .iter_mut()
                    .filter(|f| paths.contains(&f.path))
                {
                    file.staged = stage;
                }
                self.emit(BackendEvent::Git(self.git.clone()));
            }
            Command::LoadWorkspaceDiffFileDetail { key } => {
                match self.git.files.iter().find(|file| {
                    file.path == key.path
                        && file.staged == key.staged
                        && self.git.workspace == key.workspace
                        && self.git.selected.as_ref() == Some(&key.repository_key)
                }) {
                    Some(file) => self.emit(BackendEvent::GitDiff {
                        key,
                        diff: fixtures::fake_diff(file),
                    }),
                    None => self.emit(BackendEvent::GitDiffFailed { key, message: None }),
                }
            }
            Command::LoadWorkspaceGitBranches => self.emit(BackendEvent::GitBranches(
                fixtures::branches(&self.git.branch, &self.created_branches),
            )),
            Command::SwitchWorkspaceGitBranch { name, create } => {
                if create {
                    self.created_branches.push(name.clone());
                    self.git.upstream = None;
                } else {
                    self.git.upstream = Some(format!("origin/{name}"));
                }
                self.git.branch = name.clone();
                self.git.ahead = 0;
                self.git.unpushed.clear();
                self.emit(BackendEvent::Git(self.git.clone()));
                self.emit(BackendEvent::GitNotice(if create {
                    GitNotice::Created(name)
                } else {
                    GitNotice::Switched(name)
                }));
            }
            Command::GenerateGitCommitMessage => {
                self.emit(BackendEvent::GitBusy(Some(GitBusy::Generating)));
                let message = "feat(tui): animate the thinking overlay and group tool calls\n\n\
                               - fade the overlay out after reasoning completes\n\
                               - fold consecutive tool calls into one summary row\n\
                               - reflect apply_patch changes in the Changes panel";
                let chunks: VecDeque<String> =
                    message.split_inclusive(' ').map(str::to_string).collect();
                self.timers.push((
                    self.after(Duration::from_millis(500)),
                    Timer::CommitMessage(chunks),
                ));
            }
            Command::RunGitCommitAction { message, push, .. } if message.is_empty() && push => {
                self.emit(BackendEvent::GitBusy(Some(GitBusy::Pushing)));
                self.timers
                    .push((self.after(Duration::from_millis(1300)), Timer::FinishPush));
            }
            Command::RunGitCommitAction {
                message,
                new_branch,
                include_unstaged,
                push,
            } => {
                self.emit(BackendEvent::GitBusy(Some(GitBusy::Committing)));
                self.timers.push((
                    self.after(Duration::from_millis(900)),
                    Timer::FinishCommit {
                        message,
                        new_branch,
                        include_unstaged,
                        push,
                    },
                ));
            }
            Command::BrowseProjectDirectories { path } => self.emit(BackendEvent::Directory(
                files::list(path.as_deref(), true, false),
            )),
            Command::ListLocalFiles { path } => self.emit(BackendEvent::Directory(files::list(
                path.as_deref(),
                true,
                true,
            ))),
            Command::AddProjectFromPath { path } => self.add_project(path),
            Command::RenameProject { project_id, name } => {
                if let Some(project) = self.projects.iter_mut().find(|p| p.id == project_id) {
                    project.name = name;
                    self.emit(BackendEvent::Projects(self.projects.clone()));
                    self.emit(BackendEvent::Notice(Notice::Renamed));
                }
            }
            Command::ToggleProjectPinned { project_id } => {
                if let Some(project) = self.projects.iter_mut().find(|p| p.id == project_id) {
                    project.pinned = !project.pinned;
                    self.emit(BackendEvent::Projects(self.projects.clone()));
                }
            }
            // The demo has no host file manager to open.
            // demo 没有可打开的宿主文件管理器。
            Command::ArchiveProjectConversations { project_id } => {
                let count = self.archive_project(&project_id);
                self.emit(BackendEvent::Notice(Notice::ArchivedCount(count)));
                self.emit_workbench_slot();
            }
            Command::RemoveProject { project_id } => {
                self.archive_project(&project_id);
                if let Some(index) = self.projects.iter().position(|p| p.id == project_id) {
                    let project = self.projects.remove(index);
                    self.emit(BackendEvent::Projects(self.projects.clone()));
                    self.emit(BackendEvent::Notice(Notice::ProjectRemoved(project.name)));
                }
                self.emit_workbench_slot();
            }
            Command::RenameConversation {
                conversation_id,
                title,
            } => {
                if let Some(row) = self
                    .conversations
                    .iter_mut()
                    .find(|c| c.id == conversation_id)
                {
                    row.title = title;
                    let row = row.clone();
                    self.emit(BackendEvent::ConversationSummary(row));
                    self.emit(BackendEvent::Notice(Notice::Renamed));
                }
            }
            Command::ArchiveConversation { conversation_id } => {
                if let Some(title) = self.archive(&conversation_id) {
                    self.emit(BackendEvent::Notice(Notice::Archived(title)));
                    self.emit_workbench_slot();
                }
            }
            Command::StopSubagentTask {
                parent_conversation_id,
                task_id,
            } => self.agent_control(&parent_conversation_id, &task_id, true),
            Command::CloseSubagentTask {
                parent_conversation_id,
                task_id,
            } => self.agent_control(&parent_conversation_id, &task_id, false),
            Command::SuppressContextInjectionItem {
                conversation_id,
                memory_id,
            } => self.remove_memory(&conversation_id, &memory_id, false),
            Command::DeleteMemories {
                conversation_id,
                memory_id,
            } => self.remove_memory(&conversation_id, &memory_id, true),
            Command::SaveSessionGoal {
                conversation_id,
                objective,
                token_budget,
            }
            | Command::ResumeSessionGoal {
                conversation_id,
                objective,
                token_budget,
            } => self.change_goal(
                &conversation_id,
                insight::GoalChange::Set {
                    objective,
                    budget: token_budget,
                },
            ),
            Command::ClearSessionGoal { conversation_id } => {
                self.change_goal(&conversation_id, insight::GoalChange::Clear)
            }
            Command::UpdateSessionGoalStatus {
                conversation_id,
                status,
            } => self.change_goal(&conversation_id, insight::GoalChange::Status(status)),
            Command::Call(call) => self.handle_call(call),
            // The demo keeps model and effort in the UI only.
            // demo 只在界面中保存模型与推理强度。
            Command::SelectPublicAiModel { .. }
            | Command::SelectWorkbenchReasoningEffort { .. } => {}
        }
    }

    fn change_goal(&mut self, conversation_id: &str, change: insight::GoalChange) {
        let Some(status) = self
            .logs
            .get_mut(conversation_id)
            .and_then(|log| insight::apply_goal(&mut log.insight, change))
        else {
            return;
        };
        self.emit_insight(conversation_id);
        self.emit(BackendEvent::Notice(Notice::Goal(status)));
    }

    /// Archives every conversation of a project; returns how many there were.
    /// 归档项目的全部会话；返回会话数量。
    fn archive_project(&mut self, project_id: &str) -> usize {
        let ids: Vec<String> = self
            .conversations
            .iter()
            .filter(|c| c.project_id.as_deref() == Some(project_id))
            .map(|c| c.id.clone())
            .collect();
        for id in &ids {
            self.archive(id);
        }
        ids.len()
    }

    /// Answers one `service_api` call from the fake service, routed like the service worker.
    /// 由假服务响应一次 `service_api` 调用，分发方式与服务工作线程一致。
    fn handle_call(&mut self, call: ServiceCall) {
        let operation = !matches!(call.kind, CallKind::Load(_));
        let mut value = match self.service.answer(&call) {
            Ok(value) => value,
            Err(message) => {
                if let CallKind::Load(slot) = call.kind {
                    // Synthetic load errors use the same persistent retry state as real service failures.
                    // 合成加载错误与真实服务失败使用相同持久重试状态。
                    self.emit(BackendEvent::SlotFailed {
                        slot,
                        call: call.clone(),
                        message: message.clone(),
                    });
                }
                self.emit(BackendEvent::Toast {
                    severity: NoticeSeverity::Error,
                    message,
                });
                if operation {
                    self.emit(BackendEvent::CallFinished);
                }
                return;
            }
        };
        crate::service_api::scrub_secrets(&mut value);
        match call.kind.clone() {
            CallKind::Load(slot) => {
                self.loaders.insert(slot, call.clone());
                if slot == Slot::Workbench {
                    self.emit_workbench_slot();
                } else {
                    self.emit(BackendEvent::Slot { slot, call, value });
                }
            }
            CallKind::Mutate(slots) => {
                if matches!(
                    call.method,
                    "service_restore_archived_conversations"
                        | "service_delete_archived_conversations"
                ) && let Some(ids) = call
                    .params
                    .get("conversationIds")
                    .and_then(serde_json::Value::as_array)
                {
                    for id in ids.iter().filter_map(serde_json::Value::as_str) {
                        if call.method == "service_restore_archived_conversations" {
                            if let Some(row) = self.archived_rows.remove(id) {
                                self.emit(BackendEvent::ConversationSummary(row.clone()));
                                self.conversations.push(row);
                            }
                        } else {
                            self.archived_rows.remove(id);
                            self.logs.remove(id);
                            self.runs.remove(id);
                        }
                    }
                }
                for slot in slots.iter() {
                    match slot {
                        Slot::Workbench => self.emit_workbench_slot(),
                        _ => {
                            if let Some(loader) = self.loaders.get(slot).map(ServiceCall::renewed) {
                                self.handle_call(loader);
                            }
                        }
                    }
                }
                self.emit(BackendEvent::Notice(Notice::Completed(call.function)));
            }
            CallKind::Then(next) => self.handle_call(*next),
            CallKind::Preview(_) => self.emit(BackendEvent::Preview {
                call,
                result: value,
            }),
        }
        if operation {
            self.emit(BackendEvent::CallFinished);
        }
    }

    fn emit_workbench_slot(&self) {
        let mut value = self.service.workbench.clone();
        value["projects"] = serde_json::Value::Array(self.projects.iter().map(|project|serde_json::json!({
            "id":project.id,"name":project.name,"path":project.path,"pinned":project.pinned
        })).collect());
        crate::service_api::scrub_secrets(&mut value);
        self.emit(BackendEvent::Slot {
            slot: Slot::Workbench,
            call: load_workbench_snapshot(),
            value,
        });
    }

    /// Stops a running subagent (`stop`) or ends one for good.
    /// 停止运行中的子代理（`stop`），或彻底结束它。
    fn agent_control(&mut self, conversation_id: &str, agent_id: &str, stop: bool) {
        let Some(agent) = self
            .logs
            .get_mut(conversation_id)
            .and_then(|log| log.agents.iter_mut().find(|a| a.id == agent_id))
        else {
            return;
        };
        let notice = match stop {
            true if agent.state == AgentState::Running => {
                agent.state = AgentState::Stopped;
                agent.activity.push("interrupted by user".into());
                Notice::AgentStopped(agent.name.clone())
            }
            false if agent.state != AgentState::Closed => {
                agent.state = AgentState::Closed;
                agent.pending.clear();
                Notice::AgentClosed(agent.name.clone())
            }
            _ => return,
        };
        self.emit_agents(conversation_id);
        self.emit(BackendEvent::Notice(notice));
    }

    fn remove_memory(&mut self, conversation_id: &str, memory_id: &str, delete: bool) {
        let removed = self
            .logs
            .get_mut(conversation_id)
            .is_some_and(|log| insight::remove_memory(&mut log.insight, memory_id, delete));
        if !removed {
            return;
        }
        self.emit_insight(conversation_id);
        self.emit(BackendEvent::Notice(if delete {
            Notice::MemoryDeleted
        } else {
            Notice::MemorySuppressed
        }));
    }

    fn add_project(&mut self, path: String) {
        if !files::is_dir(&path) {
            self.emit(BackendEvent::Toast {
                severity: NoticeSeverity::Error,
                message: format!("Directory not found: {path}"),
            });
            return;
        }
        let listing = files::list(Some(&path), true, false);
        if let Some(existing) = self
            .projects
            .iter()
            .find(|p| p.path.eq_ignore_ascii_case(&listing.path))
        {
            self.emit(BackendEvent::Notice(Notice::ProjectAdded(
                existing.name.clone(),
            )));
            return;
        }
        let name = listing
            .path
            .trim_end_matches('\\')
            .rsplit('\\')
            .next()
            .unwrap_or(&listing.path)
            .to_string();
        let id = self.fresh_id("p");
        self.service.register_project(&id);
        self.projects.push(Project {
            id,
            name: name.clone(),
            path: listing.path,
            pinned: false,
        });
        self.emit(BackendEvent::Projects(self.projects.clone()));
        self.emit(BackendEvent::Notice(Notice::ProjectAdded(name)));
    }

    /// Moves a conversation to the archive; returns its title.
    /// 将会话移入归档；返回其标题。
    fn archive(&mut self, conversation_id: &str) -> Option<String> {
        if self.runs.contains_key(conversation_id) {
            self.stop(conversation_id, None);
        }
        let index = self
            .conversations
            .iter()
            .position(|c| c.id == conversation_id)?;
        let row = self.conversations.remove(index);
        let project = row
            .project_id
            .as_ref()
            .and_then(|id| self.projects.iter().find(|p| &p.id == id))
            .map_or_else(String::new, |p| p.name.clone());
        self.service.archive(&row.id, &row.title, &project);
        if self.selected.as_deref() == Some(conversation_id) {
            self.selected = None;
        }
        let title = row.title.clone();
        self.archived_rows.insert(row.id.clone(), row);
        self.emit(BackendEvent::ConversationRemoved(conversation_id.into()));
        Some(title)
    }

    fn answer(&mut self, request_id: &str, answers: Option<Vec<Vec<String>>>) {
        let Some(conversation_id) = self
            .runs
            .iter()
            .find(|(_, run)| {
                run.blocked_on
                    .as_ref()
                    .is_some_and(|(id, _)| id == request_id)
            })
            .map(|(id, _)| id.clone())
        else {
            return;
        };
        let tool_seq = self.runs[&conversation_id]
            .blocked_on
            .as_ref()
            .map(|(_, seq)| *seq)
            .unwrap_or_default();
        if let Some(log) = self.logs.get_mut(&conversation_id) {
            log.pending_question = None;
        }
        self.emit(BackendEvent::PendingQuestion {
            conversation_id: conversation_id.clone(),
            question: None,
        });
        self.set_attention(&conversation_id, false);
        match answers {
            Some(answers) => {
                let output = answers
                    .iter()
                    .enumerate()
                    .map(|(i, a)| format!("Q{}: {}", i + 1, a.join(", ")))
                    .collect::<Vec<_>>()
                    .join("\n");
                self.execute(
                    &conversation_id,
                    Action::ToolDone {
                        seq: tool_seq,
                        status: ToolCallStatus::Completed,
                        output,
                        changeset: Vec::new(),
                    },
                    false,
                );
                if let Some(run) = self.runs.get_mut(&conversation_id) {
                    run.blocked_on = None;
                    run.next_at = Instant::now();
                }
            }
            None => self.stop(&conversation_id, Some(tool_seq)),
        }
    }

    /// Interrupts the active turn: streams become interrupted, open tools stop.
    /// 中断当前轮次：流式内容标记为中断，未结束的工具标记为停止。
    fn stop(&mut self, conversation_id: &str, rejected_tool: Option<u64>) {
        let Some(run) = self.runs.get_mut(conversation_id) else {
            return;
        };
        run.steps.clear();
        let start = run.turn_start_seq;
        let open: Vec<(u64, EventBody)> = self
            .logs
            .get(conversation_id)
            .map(|log| {
                log.events
                    .iter()
                    .filter(|e| e.seq >= start)
                    .filter_map(|e| {
                        let mut body = e.body.clone();
                        let changed = match &mut body {
                            EventBody::AssistantReasoning { status, .. }
                            | EventBody::AssistantText { status, .. }
                                if *status == AssistantSemanticStatus::Streaming =>
                            {
                                *status = AssistantSemanticStatus::Interrupted;
                                true
                            }
                            EventBody::ToolCall { status, output, .. } if status.is_open() => {
                                if Some(e.seq) == rejected_tool {
                                    *status = ToolCallStatus::Failed;
                                    *output = Some("The user rejected the question.".into());
                                } else {
                                    *status = ToolCallStatus::Stopped;
                                }
                                true
                            }
                            _ => false,
                        };
                        changed.then_some((e.seq, body))
                    })
                    .collect()
            })
            .unwrap_or_default();
        for (seq, body) in open {
            self.upsert(conversation_id, seq, body);
        }
        if let Some(log) = self.logs.get_mut(conversation_id)
            && log.pending_question.take().is_some()
        {
            self.emit(BackendEvent::PendingQuestion {
                conversation_id: conversation_id.into(),
                question: None,
            });
        }
        for session in self
            .sessions
            .iter_mut()
            .filter(|s| s.running && s.id.starts_with("cmd-t"))
        {
            session.running = false;
            session.exit_code = Some(-1);
        }
        self.emit(BackendEvent::CommandSessions(self.sessions.clone()));
        self.finish_turn(conversation_id, TurnOutcome::Stopped, None, false);
    }
}
