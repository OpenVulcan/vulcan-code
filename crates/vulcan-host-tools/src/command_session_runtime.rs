//! Host-managed runtime for long-lived shell sessions and incremental command I/O.
//! 宿主管理的长生命周期 shell 会话与增量命令读写运行时。

use crate::model::SessionId;
use crate::output_decode::ShellOutputStreamDecoder;
use portable_pty::{CommandBuilder as PtyCommandBuilder, PtySize, native_pty_system};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::env;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::Command as StdCommand;
use std::process::Stdio;
use std::sync::{Arc, Mutex as StdMutex, RwLock};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command as TokioCommand};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender, unbounded_channel};
use tokio::sync::{Mutex, Notify, oneshot};
use tokio::time::{Duration, Instant, timeout};
use uuid::Uuid;

/// Default wait budget used when the runtime is asked to stop one running command session.
/// 运行时在收到停止命令会话请求时使用的默认等待预算。
pub const DEFAULT_COMMAND_SESSION_STOP_WAIT_MS: u64 = 2_000;

/// Default polling wait used by `read_session` when the caller does not request blocking.
/// 当调用方未请求阻塞时，`read_session` 使用的默认轮询等待时间。
pub const DEFAULT_COMMAND_SESSION_READ_WAIT_MS: u64 = 0;

/// Default maximum chunk count returned by `read_session` when the caller does not provide one.
/// 当调用方未提供读取上限时，`read_session` 返回的默认最大分片数量。
pub const DEFAULT_COMMAND_SESSION_READ_MAX_CHUNKS: usize = 128;

/// Soft byte threshold used while coalescing adjacent undelivered output fragments toward one larger read chunk.
/// 在把相邻未读输出碎片合并成较大读取分片时使用的软字节阈值。
const COMMAND_SESSION_OUTPUT_CHUNK_COALESCE_SOFT_LIMIT_BYTES: usize = 512;

/// Maximum time allowed for one supervisor control acknowledgement before the caller falls back to a best-effort state transition.
/// 监管器控制消息确认的最长等待时间，超过后调用方会回退到尽力而为的状态切换。
const COMMAND_SESSION_CONTROL_ACK_TIMEOUT_MS: u64 = 250;

/// Maximum time allowed for one stdin pipe write before the runtime treats the child as incompatible with pipe input.
/// 单次标准输入管道写入的最长等待时间，超过后运行时会认为子进程与管道输入不兼容。
const COMMAND_SESSION_STDIN_WRITE_TIMEOUT_MS: u64 = 1_000;

/// Windows process creation flag that prevents helper console windows from being shown.
/// 防止 helper 控制台窗口显示的 Windows 进程创建标志。
#[cfg(windows)]
const WINDOWS_CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Default PTY screen rows mirrored into WebUI terminal frames.
/// 镜像到 WebUI 终端帧的默认 PTY 屏幕行数。
const DEFAULT_PTY_SCREEN_ROWS: u16 = 24;

/// Default PTY screen columns mirrored into WebUI terminal frames.
/// 镜像到 WebUI 终端帧的默认 PTY 屏幕列数。
const DEFAULT_PTY_SCREEN_COLS: u16 = 100;

/// One stable lifecycle status observed for a host-managed command session.
/// 宿主管理命令会话观测到的一种稳定生命周期状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandSessionStatus {
    /// Session process is still running and may emit more output.
    /// 会话进程仍在运行，并且后续可能继续输出内容。
    Running,
    /// Session process exited cleanly without an explicit host stop request.
    /// 会话进程已自然正常退出，且并非宿主显式停止所致。
    Completed,
    /// Session process exited with an error or the supervisor observed a fatal runtime failure.
    /// 会话进程以错误方式退出，或监护逻辑观测到致命运行时失败。
    Failed,
    /// Session process was stopped by the host before normal completion.
    /// 会话进程在正常完成前被宿主主动停止。
    Stopped,
}

impl CommandSessionStatus {
    /// Renders one stable lowercase status label for host metadata and diagnostics.
    /// 为宿主元数据与诊断渲染稳定的小写状态标签。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Stopped => "stopped",
        }
    }
}

/// Concrete host I/O backend used by a command session.
/// 命令会话使用的具体宿主 I/O 后端。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandSessionBackend {
    /// Standard process pipes for command-style shell sessions.
    /// 面向命令式 shell 会话的标准进程管道。
    Pipe,
    /// Pseudo terminal for interactive TUI sessions.
    /// 面向交互式 TUI 会话的伪终端。
    Pty,
    /// Windows native console helper for Win32 console-input TUI sessions.
    /// 面向 Win32 控制台输入 TUI 会话的 Windows 原生控制台 helper。
    WindowsConsole,
}

impl CommandSessionBackend {
    /// Renders one stable lowercase backend label.
    /// 渲染稳定的小写后端标签。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pipe => "pipe",
            Self::Pty => "pty",
            Self::WindowsConsole => "windows_console",
        }
    }
}

/// One output stream kind emitted by the running command session process.
/// 运行中的命令会话进程发出的一种输出流类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandSessionOutputStream {
    /// Standard-output bytes emitted by the child process.
    /// 子进程发出的标准输出字节流。
    Stdout,
    /// Standard-error bytes emitted by the child process.
    /// 子进程发出的标准错误字节流。
    Stderr,
}

impl CommandSessionOutputStream {
    /// Renders one stable lowercase stream label for model text and host metadata.
    /// 为模型文本与宿主元数据渲染稳定的小写流标签。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Stdout => "stdout",
            Self::Stderr => "stderr",
        }
    }
}

/// One buffered output chunk stored inside the command-session runtime.
/// 命令会话运行时内部保存的一条缓冲输出分片。
#[derive(Debug, Clone)]
pub struct CommandSessionOutputChunk {
    /// Stable monotonically increasing sequence number assigned by the runtime.
    /// 由运行时分配的稳定单调递增序号。
    pub seq: u64,
    /// Concrete output stream that emitted this chunk.
    /// 发出该分片的具体输出流。
    pub stream: CommandSessionOutputStream,
    /// Decoded text payload captured for this chunk after host-side charset conversion.
    /// 为该分片捕获到的宿主侧字符集转换后文本载荷。
    pub text: String,
}

/// One structured terminal input event that can be delivered to event-capable backends.
/// 可发送到事件能力后端的一条结构化终端输入事件。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CommandSessionInputEvent {
    /// Raw text typed into the terminal.
    /// 输入到终端的原始文本。
    Text {
        /// Text payload to type.
        /// 需要输入的文本载荷。
        text: String,
    },
    /// Text pasted into the terminal.
    /// 粘贴到终端的文本。
    Paste {
        /// Text payload to paste.
        /// 需要粘贴的文本载荷。
        text: String,
    },
    /// One standardized terminal key.
    /// 一个标准化终端按键。
    Key {
        /// Stable key name such as Enter or ArrowDown.
        /// 稳定按键名称，例如 Enter 或 ArrowDown。
        key: String,
    },
    /// One standardized key chord.
    /// 一个标准化组合键。
    Chord {
        /// Base key for the chord.
        /// 组合键的基础按键。
        key: String,
        /// Whether Ctrl is held.
        /// 是否按住 Ctrl。
        ctrl: bool,
        /// Whether Alt is held.
        /// 是否按住 Alt。
        alt: bool,
        /// Whether Shift is held.
        /// 是否按住 Shift。
        shift: bool,
        /// Whether Meta is held.
        /// 是否按住 Meta。
        meta: bool,
    },
}

/// One immutable command-session snapshot exposed back to native tools and tests.
/// 对原生工具与测试回传的一份不可变命令会话快照。
#[derive(Debug, Clone)]
pub struct CommandSessionSnapshot {
    /// Stable command-session identifier generated by the host.
    /// 由宿主生成的稳定命令会话标识。
    pub command_session_id: SessionId,
    /// Optional user-facing session display name assigned by the caller.
    /// 调用方分配的可选用户可见会话显示名称。
    pub display_name: Option<String>,
    /// Controller session that owns this command session.
    /// 拥有该命令会话的控制器会话。
    pub owner_session_id: SessionId,
    /// Original shell command text requested by the caller.
    /// 调用方请求的原始 shell 命令文本。
    pub command: String,
    /// Stable shell label selected by the host launcher.
    /// 由宿主启动器选定的稳定 shell 标签。
    pub shell_label: String,
    /// Stable outer launcher shell label selected before the command possibly entered another nested interactive shell.
    /// 在命令可能进入其他嵌套交互 shell 之前选定的稳定外层启动器 shell 标签。
    pub launcher_shell_label: String,
    /// Effective working directory used by the command session.
    /// 命令会话使用的实际工作目录。
    pub cwd: PathBuf,
    /// Concrete host I/O backend used by this command session.
    /// 该命令会话使用的具体宿主 I/O 后端。
    pub backend: CommandSessionBackend,
    /// Current command-session lifecycle status.
    /// 当前命令会话生命周期状态。
    pub status: CommandSessionStatus,
    /// Optional process exit code captured after process termination.
    /// 进程终止后捕获到的可选退出码。
    pub exit_code: Option<i32>,
    /// Optional failure reason synthesized by the host runtime.
    /// 由宿主运行时综合生成的可选失败原因。
    pub failure_reason: Option<String>,
    /// Total decoded stdout byte count accumulated so far.
    /// 当前累计的解码后标准输出字节数。
    pub stdout_bytes: usize,
    /// Total decoded stderr byte count accumulated so far.
    /// 当前累计的解码后标准错误字节数。
    pub stderr_bytes: usize,
    /// Highest output sequence number buffered so far.
    /// 当前已缓冲输出中的最高序号。
    pub last_seq: u64,
    /// Whether the latest stdin write left one open input line without a trailing newline.
    /// 最近一次标准输入写入是否留下了一条未以换行结束的开放输入行。
    pub pending_stdin_line: bool,
    /// Latest screen frame sequence reported by a frame-capable backend.
    /// 具备屏幕帧能力的后端报告的最新屏幕帧序号。
    pub screen_frame_seq: Option<u64>,
    /// Latest screen row count reported by a frame-capable backend.
    /// 具备屏幕帧能力的后端报告的最新屏幕行数。
    pub screen_rows: Option<u16>,
    /// Latest screen column count reported by a frame-capable backend.
    /// 具备屏幕帧能力的后端报告的最新屏幕列数。
    pub screen_cols: Option<u16>,
    /// Latest renderable screen text reported by a frame-capable backend.
    /// 具备屏幕帧能力的后端报告的最新可渲染屏幕内容。
    pub screen_text: Option<String>,
}

/// One read outcome returned by the command-session runtime.
/// 命令会话运行时返回的一次读取结果。
#[derive(Debug, Clone)]
pub struct CommandSessionReadOutcome {
    /// Latest session snapshot observed after the read operation.
    /// 读取操作结束后观测到的最新会话快照。
    pub snapshot: CommandSessionSnapshot,
    /// Buffered output chunks returned to the caller for this read operation.
    /// 本次读取操作返回给调用方的缓冲输出分片。
    pub returned_chunks: Vec<CommandSessionOutputChunk>,
    /// Whether the runtime still holds more unread chunks beyond the returned subset.
    /// 运行时在已返回子集之外是否仍持有更多未读分片。
    pub has_more: bool,
}

/// One write outcome returned by the command-session runtime.
/// 命令会话运行时返回的一次写入结果。
#[derive(Debug, Clone)]
pub struct CommandSessionWriteOutcome {
    /// Latest session snapshot observed after the write operation.
    /// 写入操作结束后观测到的最新会话快照。
    pub snapshot: CommandSessionSnapshot,
    /// Total byte count written into the child process stdin for this operation.
    /// 本次操作写入子进程标准输入的总字节数。
    pub bytes_written: usize,
}

/// One start request accepted by the host-managed command-session runtime.
/// 宿主管理命令会话运行时接受的一份启动请求。
#[derive(Debug, Clone)]
pub struct CommandSessionStartRequest {
    /// Controller session that should own the started command session.
    /// 应拥有该启动后命令会话的控制器会话。
    pub owner_session_id: SessionId,
    /// Optional user-facing session display name stored with the runtime record.
    /// 与运行时记录一起保存的可选用户可见会话显示名称。
    pub display_name: Option<String>,
    /// Original shell command text requested by the caller.
    /// 调用方请求的原始 shell 命令文本。
    pub command: String,
    /// Stable shell label selected by the launcher resolver.
    /// 由启动器解析器选定的稳定 shell 标签。
    pub shell_label: String,
    /// Stable outer launcher shell label selected by the launcher resolver before any nested shell handoff.
    /// 由启动器解析器在任何嵌套 shell 切换前选定的稳定外层启动器 shell 标签。
    pub launcher_shell_label: String,
    /// Effective working directory used by the command session.
    /// 命令会话使用的实际工作目录。
    pub cwd: PathBuf,
    /// Concrete program that should be spawned by the host runtime.
    /// 宿主运行时应启动的具体可执行程序。
    pub launcher_program: String,
    /// Concrete launcher arguments that should be passed to the spawned program.
    /// 应传给已启动程序的具体启动参数。
    pub launcher_args: Vec<String>,
    /// Optional stdin text that should be written immediately after the process starts.
    /// 进程启动后应立即写入的可选标准输入文本。
    pub initial_stdin: Option<String>,
    /// Whether the runtime should append the session-specific interactive newline to the initial stdin text.
    /// 运行时是否应把当前会话特定的交互换行追加到初始标准输入文本末尾。
    pub initial_stdin_append_newline: bool,
    /// Shell-specific interactive newline sequence used when callers request automatic line submission.
    /// 当调用方请求自动提交一行输入时使用的 shell 特定交互换行序列。
    pub interactive_newline: String,
    /// Requested host I/O backend for this command session.
    /// 本命令会话请求使用的宿主 I/O 后端。
    pub backend: CommandSessionBackend,
}

/// Mutable per-session process state that must stay synchronized across output readers and control actions.
/// 必须在输出读取器与控制动作之间保持同步的每会话可变进程状态。
#[derive(Debug)]
struct CommandSessionMutableState {
    /// Current lifecycle status tracked by the runtime.
    /// 运行时当前跟踪的生命周期状态。
    status: CommandSessionStatus,
    /// Optional exit code observed after the process exited.
    /// 进程退出后观测到的可选退出码。
    exit_code: Option<i32>,
    /// Optional failure reason synthesized by the runtime.
    /// 由运行时综合生成的可选失败原因。
    failure_reason: Option<String>,
    /// Accumulated decoded stdout byte count.
    /// 已累计的解码后标准输出字节数。
    stdout_bytes: usize,
    /// Accumulated decoded stderr byte count.
    /// 已累计的解码后标准错误字节数。
    stderr_bytes: usize,
    /// Highest assigned output sequence number.
    /// 当前已分配的最高输出序号。
    last_seq: u64,
    /// Highest output sequence number already exposed to at least one read caller.
    /// 已经暴露给至少一个读取调用方的最高输出序号。
    highest_delivered_seq: u64,
    /// Highest output sequence number delivered through the AI-facing terminal-session cursor.
    /// 已经通过面向 AI 的终端会话游标交付的最高输出序号。
    ai_delivered_seq: u64,
    /// Whether the latest stdin write left one open input line without a trailing newline.
    /// 最近一次标准输入写入是否留下了一条未以换行结束的开放输入行。
    pending_stdin_line: bool,
    /// Latest screen frame sequence reported by an event-capable terminal backend.
    /// 事件能力终端后端报告的最新屏幕帧序号。
    screen_frame_seq: Option<u64>,
    /// Latest screen row count reported by an event-capable terminal backend.
    /// 事件能力终端后端报告的最新屏幕行数。
    screen_rows: Option<u16>,
    /// Latest screen column count reported by an event-capable terminal backend.
    /// 事件能力终端后端报告的最新屏幕列数。
    screen_cols: Option<u16>,
    /// Latest renderable screen text reported by an event-capable terminal backend.
    /// 事件能力终端后端报告的最新可渲染屏幕内容。
    screen_text: Option<String>,
    /// Buffered output chunks retained for future read calls.
    /// 为后续读取调用保留的缓冲输出分片。
    chunks: Vec<CommandSessionOutputChunk>,
}

impl Default for CommandSessionMutableState {
    /// Builds one fresh running-state record for a newly spawned command session.
    /// 为新派生的命令会话构建一份初始运行态记录。
    fn default() -> Self {
        Self {
            status: CommandSessionStatus::Running,
            exit_code: None,
            failure_reason: None,
            stdout_bytes: 0,
            stderr_bytes: 0,
            last_seq: 0,
            highest_delivered_seq: 0,
            ai_delivered_seq: 0,
            pending_stdin_line: false,
            screen_frame_seq: None,
            screen_rows: None,
            screen_cols: None,
            screen_text: None,
            chunks: Vec::new(),
        }
    }
}

/// Shared mutable state plus one notification primitive owned by a command session record.
/// 由命令会话记录持有的共享可变状态与通知原语。
#[derive(Debug, Default)]
struct CommandSessionSharedState {
    /// Mutable session process state protected for concurrent async access.
    /// 为并发异步访问加锁保护的可变会话进程状态。
    mutable_state: Mutex<CommandSessionMutableState>,
    /// Notification primitive fired on new output or lifecycle transitions.
    /// 在出现新输出或生命周期切换时触发的通知原语。
    change_notify: Notify,
}

impl CommandSessionSharedState {
    /// Appends one decoded output chunk and wakes pending readers.
    /// 追加一条解码后的输出分片并唤醒等待中的读取方。
    async fn append_output_chunk(&self, stream: CommandSessionOutputStream, text: String) {
        if text.is_empty() {
            return;
        }
        let mut state = self.mutable_state.lock().await;
        append_output_chunk_to_state(&mut state, stream, text);
        drop(state);
        self.change_notify.notify_waiters();
    }

    /// Appends one decoded output chunk from a blocking PTY reader thread and wakes pending readers.
    /// 从阻塞 PTY 读取线程追加一条解码后的输出分片并唤醒等待中的读取方。
    fn append_output_chunk_blocking(&self, stream: CommandSessionOutputStream, text: String) {
        if text.is_empty() {
            return;
        }
        let mut state = self.mutable_state.blocking_lock();
        append_output_chunk_to_state(&mut state, stream, text);
        drop(state);
        self.change_notify.notify_waiters();
    }

    /// Records one runtime-generated failure reason without immediately overriding the running status.
    /// 记录一条运行时生成的失败原因，而不立即覆盖运行状态。
    async fn remember_failure_reason(&self, reason: String) {
        let mut state = self.mutable_state.lock().await;
        if state.failure_reason.is_none() {
            state.failure_reason = Some(reason);
        }
        drop(state);
        self.change_notify.notify_waiters();
    }

    /// Updates whether the latest stdin submission left one open line and wakes pending readers.
    /// 更新最近一次标准输入提交是否留下开放输入行，并唤醒等待中的读取方。
    async fn set_pending_stdin_line(&self, pending_stdin_line: bool) {
        let mut state = self.mutable_state.lock().await;
        state.pending_stdin_line = pending_stdin_line;
        drop(state);
        self.change_notify.notify_waiters();
    }

    /// Stores the latest terminal screen frame and wakes pending readers.
    /// 保存最新终端屏幕帧并唤醒等待中的读取方。
    async fn update_screen_frame(&self, seq: u64, rows: u16, cols: u16, text: String) {
        let mut state = self.mutable_state.lock().await;
        state.screen_frame_seq = Some(seq);
        state.screen_rows = Some(rows);
        state.screen_cols = Some(cols);
        state.screen_text = Some(text);
        drop(state);
        self.change_notify.notify_waiters();
    }

    /// Stores the latest terminal screen frame from a blocking PTY reader thread.
    /// 从阻塞 PTY 读取线程保存最新终端屏幕帧。
    fn update_screen_frame_blocking(&self, seq: u64, rows: u16, cols: u16, text: String) {
        let mut state = self.mutable_state.blocking_lock();
        state.screen_frame_seq = Some(seq);
        state.screen_rows = Some(rows);
        state.screen_cols = Some(cols);
        state.screen_text = Some(text);
        drop(state);
        self.change_notify.notify_waiters();
    }

    /// Marks the session as host-stopped before process reaping completes, so UI stop/close paths never wait on incompatible terminal I/O forever.
    /// 在进程回收完成前先把会话标记为宿主停止，确保 UI 停止/关闭路径不会永久等待不兼容的终端 I/O。
    async fn mark_stop_requested(&self) {
        let mut state = self.mutable_state.lock().await;
        mark_stop_requested_state(&mut state);
        drop(state);
        self.change_notify.notify_waiters();
    }

    /// Finalizes the session after the child process has reached a terminal state.
    /// 在子进程到达终态后完成会话收尾。
    async fn finalize_process_exit(
        &self,
        stop_requested: bool,
        exit_code: Option<i32>,
        wait_error: Option<String>,
        process_succeeded: bool,
    ) {
        let mut state = self.mutable_state.lock().await;
        finalize_process_exit_state(
            &mut state,
            stop_requested,
            exit_code,
            wait_error,
            process_succeeded,
        );
        drop(state);
        self.change_notify.notify_waiters();
    }
}

/// Appends one decoded output chunk into a mutable session state.
/// 将一条解码输出分片追加进可变会话状态。
fn append_output_chunk_to_state(
    state: &mut CommandSessionMutableState,
    stream: CommandSessionOutputStream,
    text: String,
) {
    let bytes = text.len();
    match stream {
        CommandSessionOutputStream::Stdout => {
            state.stdout_bytes = state.stdout_bytes.saturating_add(bytes);
        }
        CommandSessionOutputStream::Stderr => {
            state.stderr_bytes = state.stderr_bytes.saturating_add(bytes);
        }
    }
    if can_coalesce_output_chunk(
        state.chunks.last(),
        state.highest_delivered_seq,
        stream,
        bytes,
    ) {
        if let Some(last_chunk) = state.chunks.last_mut() {
            last_chunk.text.push_str(&text);
        }
    } else {
        state.last_seq = state.last_seq.saturating_add(1);
        let seq = state.last_seq;
        state
            .chunks
            .push(CommandSessionOutputChunk { seq, stream, text });
    }
}

/// Marks one mutable session state as host-stopped.
/// 将一份可变会话状态标记为宿主停止。
fn mark_stop_requested_state(state: &mut CommandSessionMutableState) {
    if state.status == CommandSessionStatus::Running {
        state.status = CommandSessionStatus::Stopped;
        state.exit_code = None;
        state.failure_reason = None;
        state.pending_stdin_line = false;
    }
}

/// Finalizes one mutable session state after its process reaches a terminal state.
/// 在进程到达终态后完成一份可变会话状态收尾。
fn finalize_process_exit_state(
    state: &mut CommandSessionMutableState,
    stop_requested: bool,
    exit_code: Option<i32>,
    wait_error: Option<String>,
    process_succeeded: bool,
) {
    state.exit_code = exit_code;
    if let Some(error) = wait_error {
        state.status = CommandSessionStatus::Failed;
        state.failure_reason = Some(error);
    } else if state.status == CommandSessionStatus::Failed && state.failure_reason.is_some() {
        // Preserve the earlier runtime failure reason instead of overwriting it with a kill or exit-code summary.
        // 保留更早记录的运行时失败原因，而不是用 kill 或退出码摘要把它覆盖掉。
    } else if stop_requested {
        state.status = CommandSessionStatus::Stopped;
        state.failure_reason = None;
    } else if process_succeeded {
        state.status = CommandSessionStatus::Completed;
        state.failure_reason = None;
    } else {
        state.status = CommandSessionStatus::Failed;
        if state.failure_reason.is_none() {
            state.failure_reason = Some(match exit_code {
                Some(code) => format!("The command session exited with status code {code}."),
                None => String::from("The command session failed without returning an exit code."),
            });
        }
    }
}

/// Reports whether one output chunk already ends on a stable line boundary.
/// 判断一条输出分片是否已经稳定结束在行边界上。
fn output_chunk_ends_with_line_boundary(text: &str) -> bool {
    text.ends_with(['\n', '\r'])
}

/// Reports whether one new fragment may be merged into the last undelivered chunk without breaking read-cursor stability.
/// 判断一条新碎片是否可以安全合并进最后一个未投递分片，而不破坏读取游标稳定性。
fn can_coalesce_output_chunk(
    last_chunk: Option<&CommandSessionOutputChunk>,
    highest_delivered_seq: u64,
    incoming_stream: CommandSessionOutputStream,
    incoming_bytes: usize,
) -> bool {
    let Some(last_chunk) = last_chunk else {
        return false;
    };
    last_chunk.seq > highest_delivered_seq
        && last_chunk.stream == incoming_stream
        && !output_chunk_ends_with_line_boundary(&last_chunk.text)
        && last_chunk.text.len().saturating_add(incoming_bytes)
            <= COMMAND_SESSION_OUTPUT_CHUNK_COALESCE_SOFT_LIMIT_BYTES
}

/// One control message sent to the background process supervisor.
/// 发送给后台进程监管器的一条控制消息。
enum CommandSessionControlMessage {
    /// Request to write one byte payload into the child stdin stream.
    /// 请求向子进程标准输入流写入一段字节载荷。
    Write {
        /// Byte payload that should be written into stdin.
        /// 应写入标准输入的字节载荷。
        bytes: Vec<u8>,
        /// One-shot responder used to return the write outcome.
        /// 用于回传写入结果的单次响应通道。
        responder: oneshot::Sender<Result<usize, String>>,
    },
    /// Request to deliver structured terminal input events.
    /// 请求发送结构化终端输入事件。
    InputEvents {
        /// Ordered input events that should be delivered to the terminal.
        /// 应发送到终端的保序输入事件。
        events: Vec<CommandSessionInputEvent>,
        /// One-shot responder used to return the accepted input count.
        /// 用于回传已接受输入数量的单次响应通道。
        responder: oneshot::Sender<Result<usize, String>>,
    },
    /// Request to resize the terminal screen.
    /// 请求调整终端屏幕尺寸。
    Resize {
        /// Desired terminal row count.
        /// 期望的终端行数。
        rows: u16,
        /// Desired terminal column count.
        /// 期望的终端列数。
        cols: u16,
        /// One-shot responder used to acknowledge the resize request.
        /// 用于确认 resize 请求的单次响应通道。
        responder: oneshot::Sender<Result<(), String>>,
    },
    /// Request to stop the running child process.
    /// 请求停止当前运行的子进程。
    Stop {
        /// One-shot responder used to acknowledge whether the kill signal was sent successfully.
        /// 用于确认 kill 信号是否成功发出的单次响应通道。
        responder: oneshot::Sender<Result<(), String>>,
    },
}

/// One host-owned command-session record stored in the runtime registry.
/// 存储在运行时注册表中的一条宿主自有命令会话记录。
#[derive(Debug)]
struct CommandSessionRecord {
    /// Stable command-session identifier keyed inside the runtime registry.
    /// 在运行时注册表内作为键使用的稳定命令会话标识。
    command_session_id: SessionId,
    /// Controller session that owns this command session.
    /// 拥有该命令会话的控制器会话。
    owner_session_id: SessionId,
    /// Optional user-facing session display name assigned at start.
    /// 启动时分配的可选用户可见会话显示名称。
    display_name: Option<String>,
    /// Original shell command text requested by the caller.
    /// 调用方请求的原始 shell 命令文本。
    command: String,
    /// Stable shell label selected by the launcher resolver.
    /// 由启动器解析器选定的稳定 shell 标签。
    shell_label: String,
    /// Stable outer launcher shell label selected by the launcher resolver.
    /// 由启动器解析器选定的稳定外层启动器 shell 标签。
    launcher_shell_label: String,
    /// Shell-specific interactive newline sequence reused by write operations that request automatic line submission.
    /// 由请求自动提交一行输入的写入操作复用的 shell 特定交互换行序列。
    interactive_newline: String,
    /// Effective working directory used by the spawned process.
    /// 已派生进程使用的实际工作目录。
    cwd: PathBuf,
    /// Concrete host I/O backend used by this command session.
    /// 该命令会话使用的具体宿主 I/O 后端。
    backend: CommandSessionBackend,
    /// Shared sender used to control the background process supervisor.
    /// 用于控制后台进程监管器的共享发送端。
    control_sender: UnboundedSender<CommandSessionControlMessage>,
    /// Shared mutable session state observed by start/read/write/stop operations.
    /// 由 start/read/write/stop 操作共同观测的共享会话状态。
    shared_state: Arc<CommandSessionSharedState>,
}

impl CommandSessionRecord {
    /// Builds one immutable snapshot from the latest shared mutable state.
    /// 根据最新共享可变状态构建一份不可变快照。
    async fn snapshot(&self) -> CommandSessionSnapshot {
        let state = self.shared_state.mutable_state.lock().await;
        self.snapshot_from_state(&state)
    }

    /// Builds one best-effort immutable snapshot without waiting on the async mutex, so synchronous WebUI render paths can reuse the latest runtime state when it is immediately available.
    /// 在不等待异步互斥锁的前提下构建一份尽力而为的不可变快照，使同步 WebUI 渲染路径可以在状态立即可得时复用最新运行时状态。
    fn try_snapshot(&self) -> Option<CommandSessionSnapshot> {
        let state = self.shared_state.mutable_state.try_lock().ok()?;
        Some(self.snapshot_from_state(&state))
    }

    /// Builds a conservative running snapshot when the async state lock is busy.
    /// 当异步状态锁忙碌时构建一份保守的运行中快照。
    fn fallback_running_snapshot(&self) -> CommandSessionSnapshot {
        CommandSessionSnapshot {
            command_session_id: self.command_session_id,
            display_name: self.display_name.clone(),
            owner_session_id: self.owner_session_id,
            command: self.command.clone(),
            shell_label: self.shell_label.clone(),
            launcher_shell_label: self.launcher_shell_label.clone(),
            cwd: self.cwd.clone(),
            backend: self.backend,
            status: CommandSessionStatus::Running,
            exit_code: None,
            failure_reason: None,
            stdout_bytes: 0,
            stderr_bytes: 0,
            last_seq: 0,
            pending_stdin_line: false,
            screen_frame_seq: None,
            screen_rows: None,
            screen_cols: None,
            screen_text: None,
        }
    }

    /// Builds one immutable snapshot from a locked mutable state record.
    /// 基于已加锁的可变状态记录构建一份不可变快照。
    fn snapshot_from_state(&self, state: &CommandSessionMutableState) -> CommandSessionSnapshot {
        CommandSessionSnapshot {
            command_session_id: self.command_session_id,
            display_name: self.display_name.clone(),
            owner_session_id: self.owner_session_id,
            command: self.command.clone(),
            shell_label: self.shell_label.clone(),
            launcher_shell_label: self.launcher_shell_label.clone(),
            cwd: self.cwd.clone(),
            backend: self.backend,
            status: state.status,
            exit_code: state.exit_code,
            failure_reason: state.failure_reason.clone(),
            stdout_bytes: state.stdout_bytes,
            stderr_bytes: state.stderr_bytes,
            last_seq: state.last_seq,
            pending_stdin_line: state.pending_stdin_line,
            screen_frame_seq: state.screen_frame_seq,
            screen_rows: state.screen_rows,
            screen_cols: state.screen_cols,
            screen_text: state.screen_text.clone(),
        }
    }

    /// Builds one incremental read outcome based on the caller-provided output cursor and read cap.
    /// 根据调用方提供的输出游标与读取上限构建一次增量读取结果。
    async fn read_since(
        &self,
        after_seq: u64,
        max_chunks: usize,
        track_delivery: bool,
    ) -> CommandSessionReadOutcome {
        let mut state = self.shared_state.mutable_state.lock().await;
        let matching_chunks = state
            .chunks
            .iter()
            .filter(|chunk| chunk.seq > after_seq)
            .cloned()
            .collect::<Vec<_>>();
        let has_more = matching_chunks.len() > max_chunks;
        let returned_chunks = matching_chunks
            .into_iter()
            .take(max_chunks)
            .collect::<Vec<_>>();
        if track_delivery {
            if let Some(last_returned_chunk) = returned_chunks.last() {
                state.highest_delivered_seq =
                    state.highest_delivered_seq.max(last_returned_chunk.seq);
            }
        }
        let snapshot = self.snapshot_from_state(&state);
        CommandSessionReadOutcome {
            snapshot,
            returned_chunks,
            has_more,
        }
    }

    /// Returns the AI-facing delivered cursor currently associated with this session.
    /// 返回当前与该会话关联的面向 AI 的已交付游标。
    async fn ai_delivered_seq(&self) -> u64 {
        self.shared_state
            .mutable_state
            .lock()
            .await
            .ai_delivered_seq
    }

    /// Advances the AI-facing delivered cursor after terminal_session has returned chunks.
    /// 在 terminal_session 返回分片后推进面向 AI 的已交付游标。
    async fn mark_ai_delivered(&self, returned_chunks: &[CommandSessionOutputChunk]) {
        let Some(last_returned_chunk) = returned_chunks.last() else {
            return;
        };
        let mut state = self.shared_state.mutable_state.lock().await;
        state.ai_delivered_seq = state.ai_delivered_seq.max(last_returned_chunk.seq);
    }
}

/// Reports whether one stdin byte payload leaves the interactive session on an open input line.
/// 判断一段标准输入字节载荷是否会让交互会话停留在一条未闭合的输入行上。
fn stdin_payload_bytes_leave_pending_line(bytes: &[u8]) -> bool {
    !bytes.is_empty() && !bytes.ends_with(b"\n") && !bytes.ends_with(b"\r")
}

/// Reports whether one stdin text payload leaves the interactive session on an open input line.
/// 判断一段标准输入文本载荷是否会让交互会话停留在一条未闭合的输入行上。
fn stdin_payload_leaves_pending_line(text: &str) -> bool {
    stdin_payload_bytes_leave_pending_line(text.as_bytes())
}

/// One request sent from the host runtime to the Windows console helper.
/// 从宿主运行时发送给 Windows 控制台 helper 的一条请求。
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum WindowsConsoleHelperRequest {
    /// Starts the target console program inside the helper.
    /// 在 helper 内启动目标控制台程序。
    Start {
        /// Program executable path or name.
        /// 程序可执行文件路径或名称。
        program: String,
        /// Program argument list.
        /// 程序参数列表。
        args: Vec<String>,
        /// Working directory for the target process.
        /// 目标进程的工作目录。
        cwd: String,
        /// Initial screen row count.
        /// 初始屏幕行数。
        rows: u16,
        /// Initial screen column count.
        /// 初始屏幕列数。
        cols: u16,
        /// Optional initial stdin text.
        /// 可选初始标准输入文本。
        initial_stdin: Option<String>,
        /// Whether the helper should append the interactive newline to initial stdin.
        /// helper 是否应向初始标准输入追加交互换行。
        initial_stdin_append_newline: bool,
        /// Interactive newline used by the selected shell.
        /// 所选 shell 使用的交互换行。
        interactive_newline: String,
    },
    /// Sends ordered terminal input events.
    /// 发送保序终端输入事件。
    Input {
        /// Ordered input events.
        /// 保序输入事件。
        events: Vec<CommandSessionInputEvent>,
    },
    /// Writes raw text as terminal input.
    /// 将原始文本作为终端输入写入。
    Text {
        /// Text payload.
        /// 文本载荷。
        text: String,
    },
    /// Resizes the helper terminal.
    /// 调整 helper 终端大小。
    Resize {
        /// Desired row count.
        /// 期望行数。
        rows: u16,
        /// Desired column count.
        /// 期望列数。
        cols: u16,
    },
    /// Stops the helper and target process.
    /// 停止 helper 与目标进程。
    Stop,
}

/// One event received from the Windows console helper.
/// 从 Windows 控制台 helper 接收的一条事件。
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum WindowsConsoleHelperEvent {
    /// Target process started successfully.
    /// 目标进程已成功启动。
    Started {
        /// Target process identifier.
        /// 目标进程标识。
        pid: u32,
    },
    /// Text output emitted by the helper.
    /// helper 发出的文本输出。
    Output {
        /// Output stream label.
        /// 输出流标签。
        stream: String,
        /// Output text payload.
        /// 输出文本载荷。
        text: String,
    },
    /// Current terminal screen frame.
    /// 当前终端屏幕帧。
    Frame {
        /// Stable frame sequence.
        /// 稳定帧序号。
        seq: u64,
        /// Frame row count.
        /// 帧行数。
        rows: u16,
        /// Frame column count.
        /// 帧列数。
        cols: u16,
        /// Renderable screen text with optional ANSI SGR styling.
        /// 可渲染屏幕文本，可包含 ANSI SGR 样式。
        text: String,
    },
    /// Target process reached a terminal status.
    /// 目标进程到达终态。
    Exited {
        /// Optional target exit code.
        /// 可选目标退出码。
        exit_code: Option<i32>,
        /// Whether the helper stopped the target by request.
        /// helper 是否按请求停止了目标。
        stopped: bool,
    },
    /// Helper reported an error.
    /// helper 报告错误。
    Error {
        /// Error message.
        /// 错误消息。
        message: String,
    },
}

/// Resolves the Windows console helper executable path located beside the current process.
/// 解析位于当前进程旁边的 Windows 控制台 helper 可执行文件路径。
#[cfg(windows)]
fn resolve_windows_console_helper_path() -> Result<PathBuf, String> {
    if let Ok(explicit_path) = env::var("VULCAN_WINDOWS_CONSOLE_HELPER") {
        let explicit_path = PathBuf::from(explicit_path);
        if explicit_path.is_file() {
            return Ok(explicit_path);
        }
        return Err(format!(
            "VULCAN_WINDOWS_CONSOLE_HELPER points to `{}`, but that file does not exist.",
            explicit_path.display()
        ));
    }
    let current_exe = env::current_exe()
        .map_err(|error| format!("Failed to resolve the current executable path: {error}"))?;
    let helper_name = if cfg!(debug_assertions) {
        "vulcan-windows-console-helper.exe"
    } else {
        "vulcan-windows-console-helper.exe"
    };
    let beside_current = current_exe.with_file_name(helper_name);
    if beside_current.is_file() {
        return Ok(beside_current);
    }
    let manifest_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("target")
        .join(if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        })
        .join(helper_name);
    if manifest_path.is_file() {
        return Ok(manifest_path);
    }
    Err(format!(
        "Windows console helper `{helper_name}` was not found beside `{}` or at `{}`. Build it with `cargo build -p vulcan-windows-console-helper`.",
        current_exe.display(),
        manifest_path.display()
    ))
}

/// Writes one JSONL request to the Windows console helper stdin pipe.
/// 向 Windows 控制台 helper 标准输入管道写入一条 JSONL 请求。
#[cfg(windows)]
async fn write_windows_console_helper_request(
    stdin: &mut ChildStdin,
    request: &WindowsConsoleHelperRequest,
) -> Result<(), String> {
    let mut line = serde_json::to_vec(request)
        .map_err(|error| format!("Failed to serialize Windows console helper request: {error}"))?;
    line.push(b'\n');
    stdin
        .write_all(&line)
        .await
        .map_err(|error| format!("Failed to write Windows console helper request: {error}"))?;
    stdin
        .flush()
        .await
        .map_err(|error| format!("Failed to flush Windows console helper request: {error}"))
}

/// One host-managed runtime that owns long-lived shell sessions and their process supervisors.
/// 持有长生命周期 shell 会话及其进程监管器的宿主管理运行时。
pub struct CommandSessionRuntime {
    /// Registry of command sessions keyed by their stable command-session identifier.
    /// 以稳定命令会话标识为键建立的命令会话注册表。
    sessions: RwLock<HashMap<SessionId, Arc<CommandSessionRecord>>>,
}

impl Default for CommandSessionRuntime {
    /// Builds one empty command-session runtime.
    /// 构建一份空的命令会话运行时。
    fn default() -> Self {
        Self::new()
    }
}

impl CommandSessionRuntime {
    /// Builds one empty command-session runtime.
    /// 构建一份空的命令会话运行时。
    pub fn new() -> Self {
        Self {
            sessions: RwLock::new(HashMap::new()),
        }
    }

    /// Starts one host-managed command session and returns its initial snapshot.
    /// 启动一条宿主管理命令会话并返回其初始快照。
    pub async fn start_session(
        self: &Arc<Self>,
        request: CommandSessionStartRequest,
    ) -> Result<CommandSessionSnapshot, String> {
        match request.backend {
            CommandSessionBackend::Pipe => self.start_pipe_session(request).await,
            CommandSessionBackend::Pty => self.start_pty_session(request).await,
            CommandSessionBackend::WindowsConsole => {
                self.start_windows_console_session(request).await
            }
        }
    }

    /// Starts one pipe-backed host-managed command session and returns its initial snapshot.
    /// 启动一条管道后端的宿主管理命令会话并返回其初始快照。
    async fn start_pipe_session(
        self: &Arc<Self>,
        request: CommandSessionStartRequest,
    ) -> Result<CommandSessionSnapshot, String> {
        let mut command = TokioCommand::new(&request.launcher_program);
        command.args(&request.launcher_args);
        command.current_dir(&request.cwd);
        command.stdin(Stdio::piped());
        command.stdout(Stdio::piped());
        command.stderr(Stdio::piped());
        command.kill_on_drop(true);

        let mut child = command.spawn().map_err(|error| {
            format!(
                "Failed to start the command session: program `{}` could not be executed: {error}",
                request.launcher_program
            )
        })?;
        let stdin = child.stdin.take();
        let stdout = child.stdout.take().ok_or_else(|| {
            String::from("The command session started, but stdout could not be captured.")
        })?;
        let stderr = child.stderr.take().ok_or_else(|| {
            String::from("The command session started, but stderr could not be captured.")
        })?;

        let command_session_id = Uuid::new_v4();
        let shared_state = Arc::new(CommandSessionSharedState::default());
        let (control_sender, control_receiver) = unbounded_channel();
        let record = Arc::new(CommandSessionRecord {
            command_session_id,
            owner_session_id: request.owner_session_id,
            display_name: request.display_name,
            command: request.command,
            shell_label: request.shell_label,
            launcher_shell_label: request.launcher_shell_label,
            interactive_newline: request.interactive_newline,
            cwd: request.cwd,
            backend: CommandSessionBackend::Pipe,
            control_sender,
            shared_state: Arc::clone(&shared_state),
        });

        self.sessions
            .write()
            .map_err(|_| {
                String::from(
                    "The command session registry is locked and cannot store a new session.",
                )
            })?
            .insert(command_session_id, Arc::clone(&record));

        tokio::spawn(run_command_session_supervisor(
            Arc::clone(&record),
            child,
            stdin,
            stdout,
            stderr,
            control_receiver,
            request.initial_stdin,
            request.initial_stdin_append_newline,
        ));

        Ok(record.snapshot().await)
    }

    /// Starts one PTY-backed host-managed command session and returns its initial snapshot.
    /// 启动一条 PTY 后端的宿主管理命令会话并返回其初始快照。
    async fn start_pty_session(
        self: &Arc<Self>,
        request: CommandSessionStartRequest,
    ) -> Result<CommandSessionSnapshot, String> {
        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows: 24,
                cols: 100,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|error| format!("Failed to create a pseudo terminal: {error}"))?;
        let mut command = PtyCommandBuilder::new(&request.launcher_program);
        command.args(&request.launcher_args);
        command.cwd(&request.cwd);
        let child = pair
            .slave
            .spawn_command(command)
            .map_err(|error| format!("Failed to start the PTY command session: {error}"))?;
        drop(pair.slave);
        let reader = pair
            .master
            .try_clone_reader()
            .map_err(|error| format!("Failed to capture PTY output: {error}"))?;
        let writer = pair
            .master
            .take_writer()
            .map_err(|error| format!("Failed to open PTY input: {error}"))?;

        let command_session_id = Uuid::new_v4();
        let shared_state = Arc::new(CommandSessionSharedState::default());
        let (control_sender, control_receiver) = unbounded_channel();
        let record = Arc::new(CommandSessionRecord {
            command_session_id,
            owner_session_id: request.owner_session_id,
            display_name: request.display_name,
            command: request.command,
            shell_label: request.shell_label,
            launcher_shell_label: request.launcher_shell_label,
            interactive_newline: request.interactive_newline,
            cwd: request.cwd,
            backend: CommandSessionBackend::Pty,
            control_sender,
            shared_state: Arc::clone(&shared_state),
        });

        self.sessions
            .write()
            .map_err(|_| {
                String::from(
                    "The command session registry is locked and cannot store a new session.",
                )
            })?
            .insert(command_session_id, Arc::clone(&record));

        tokio::spawn(run_pty_command_session_supervisor(
            Arc::clone(&record),
            pair.master,
            child,
            reader,
            writer,
            control_receiver,
            request.initial_stdin,
            request.initial_stdin_append_newline,
        ));

        Ok(record.snapshot().await)
    }

    /// Starts one Windows native console helper-backed command session and returns its initial snapshot.
    /// 启动一条 Windows 原生控制台 helper 后端的命令会话并返回其初始快照。
    async fn start_windows_console_session(
        self: &Arc<Self>,
        request: CommandSessionStartRequest,
    ) -> Result<CommandSessionSnapshot, String> {
        #[cfg(not(windows))]
        {
            let _ = request;
            Err(String::from(
                "The Windows native console backend is only available on Windows.",
            ))
        }
        #[cfg(windows)]
        {
            self.start_windows_console_session_inner(request).await
        }
    }

    /// Starts one Windows helper-backed command session on Windows hosts.
    /// 在 Windows 宿主上启动一条 helper 后端命令会话。
    #[cfg(windows)]
    async fn start_windows_console_session_inner(
        self: &Arc<Self>,
        request: CommandSessionStartRequest,
    ) -> Result<CommandSessionSnapshot, String> {
        let helper_path = resolve_windows_console_helper_path()?;
        let mut helper = TokioCommand::new(&helper_path);
        helper.creation_flags(WINDOWS_CREATE_NO_WINDOW);
        helper.stdin(Stdio::piped());
        helper.stdout(Stdio::piped());
        helper.stderr(Stdio::piped());
        helper.kill_on_drop(true);
        let mut child = helper.spawn().map_err(|error| {
            format!(
                "Failed to start Windows console helper `{}`: {error}",
                helper_path.display()
            )
        })?;
        let mut helper_stdin = child.stdin.take().ok_or_else(|| {
            String::from("The Windows console helper started without a writable stdin pipe.")
        })?;
        let helper_stdout = child.stdout.take().ok_or_else(|| {
            String::from("The Windows console helper started without a readable stdout pipe.")
        })?;
        let helper_stderr = child.stderr.take().ok_or_else(|| {
            String::from("The Windows console helper started without a readable stderr pipe.")
        })?;
        let start_request = WindowsConsoleHelperRequest::Start {
            program: request.launcher_program.clone(),
            args: request.launcher_args.clone(),
            cwd: request.cwd.display().to_string(),
            rows: 24,
            cols: 100,
            initial_stdin: request.initial_stdin.clone(),
            initial_stdin_append_newline: request.initial_stdin_append_newline,
            interactive_newline: request.interactive_newline.clone(),
        };
        write_windows_console_helper_request(&mut helper_stdin, &start_request).await?;

        let command_session_id = Uuid::new_v4();
        let shared_state = Arc::new(CommandSessionSharedState::default());
        let (control_sender, control_receiver) = unbounded_channel();
        let record = Arc::new(CommandSessionRecord {
            command_session_id,
            owner_session_id: request.owner_session_id,
            display_name: request.display_name,
            command: request.command,
            shell_label: request.shell_label,
            launcher_shell_label: request.launcher_shell_label,
            interactive_newline: request.interactive_newline,
            cwd: request.cwd,
            backend: CommandSessionBackend::WindowsConsole,
            control_sender,
            shared_state: Arc::clone(&shared_state),
        });

        self.sessions
            .write()
            .map_err(|_| {
                String::from(
                    "The command session registry is locked and cannot store a new session.",
                )
            })?
            .insert(command_session_id, Arc::clone(&record));

        tokio::spawn(run_windows_console_helper_supervisor(
            Arc::clone(&record),
            child,
            helper_stdin,
            helper_stdout,
            helper_stderr,
            control_receiver,
        ));

        Ok(record.snapshot().await)
    }

    /// Reads buffered output from one owned command session, optionally waiting for fresh output when needed.
    /// 从一条归属命令会话读取缓冲输出，并在需要时可选等待新输出到达。
    pub async fn read_session(
        &self,
        owner_session_id: SessionId,
        command_session_id: SessionId,
        after_seq: Option<u64>,
        wait_for_output_ms: Option<u64>,
        max_chunks: Option<usize>,
    ) -> Result<CommandSessionReadOutcome, String> {
        let record = self.session_for_owner(owner_session_id, command_session_id)?;
        let after_seq = after_seq.unwrap_or(0);
        let max_chunks = max_chunks
            .unwrap_or(DEFAULT_COMMAND_SESSION_READ_MAX_CHUNKS)
            .max(1);
        let wait_for_output_ms = wait_for_output_ms.unwrap_or(DEFAULT_COMMAND_SESSION_READ_WAIT_MS);
        let wait_started_at = Instant::now();

        loop {
            let notified = record.shared_state.change_notify.notified();
            let outcome = record.read_since(after_seq, max_chunks, true).await;
            if !outcome.returned_chunks.is_empty()
                || outcome.snapshot.status != CommandSessionStatus::Running
                || wait_for_output_ms == 0
            {
                return Ok(outcome);
            }
            let waited_ms = wait_started_at.elapsed().as_millis() as u64;
            let remaining_ms = wait_for_output_ms.saturating_sub(waited_ms);
            if remaining_ms == 0 {
                return Ok(outcome);
            }
            let _ = timeout(Duration::from_millis(remaining_ms), notified).await;
        }
    }

    /// Writes one text payload into one owned running command session.
    /// 向一条归属且仍在运行的命令会话写入一段文本载荷。
    pub async fn write_session(
        &self,
        owner_session_id: SessionId,
        command_session_id: SessionId,
        text: String,
        append_newline: bool,
    ) -> Result<CommandSessionWriteOutcome, String> {
        let text = if append_newline {
            let record = self.session_for_owner(owner_session_id, command_session_id)?;
            format!("{text}{}", record.interactive_newline.as_str())
        } else {
            text
        };
        self.write_session_bytes(owner_session_id, command_session_id, text.into_bytes())
            .await
    }

    /// Writes one byte payload into one owned running command session.
    /// 向一条归属且仍在运行的命令会话写入一段字节载荷。
    pub async fn write_session_bytes(
        &self,
        owner_session_id: SessionId,
        command_session_id: SessionId,
        bytes: Vec<u8>,
    ) -> Result<CommandSessionWriteOutcome, String> {
        let record = self.session_for_owner(owner_session_id, command_session_id)?;
        let snapshot = record.snapshot().await;
        if snapshot.status != CommandSessionStatus::Running {
            return Err(format!(
                "Command session `{}` is already in `{}` state and cannot accept more stdin.",
                command_session_id,
                snapshot.status.as_str()
            ));
        }

        let (sender, receiver) = oneshot::channel();
        record
            .control_sender
            .send(CommandSessionControlMessage::Write {
                bytes,
                responder: sender,
            })
            .map_err(|_| {
                String::from(
                    "The command session supervisor has exited and cannot write more stdin.",
                )
            })?;
        let bytes_written =
            timeout(
                Duration::from_millis(COMMAND_SESSION_CONTROL_ACK_TIMEOUT_MS),
                receiver,
            )
            .await
            .map_err(|_| {
                String::from(
                    "Timed out waiting for the command session supervisor to acknowledge stdin; the process may be a Windows console-input TUI that is incompatible with pipe stdin.",
                )
            })?
            .map_err(|_| {
                String::from("The command session write acknowledgement channel is closed.")
            })??;

        Ok(CommandSessionWriteOutcome {
            snapshot: record.snapshot().await,
            bytes_written,
        })
    }

    /// Sends structured terminal input events into one owned running command session.
    /// 向一条归属且仍在运行的命令会话发送结构化终端输入事件。
    pub async fn write_session_input_events(
        &self,
        owner_session_id: SessionId,
        command_session_id: SessionId,
        events: Vec<CommandSessionInputEvent>,
    ) -> Result<CommandSessionWriteOutcome, String> {
        let record = self.session_for_owner(owner_session_id, command_session_id)?;
        let snapshot = record.snapshot().await;
        if snapshot.status != CommandSessionStatus::Running {
            return Err(format!(
                "Command session `{}` is already in `{}` state and cannot accept more terminal input.",
                command_session_id,
                snapshot.status.as_str()
            ));
        }
        let (sender, receiver) = oneshot::channel();
        record
            .control_sender
            .send(CommandSessionControlMessage::InputEvents {
                events,
                responder: sender,
            })
            .map_err(|_| {
                String::from(
                    "The command session supervisor has exited and cannot send terminal input.",
                )
            })?;
        let accepted_count =
            timeout(
                Duration::from_millis(COMMAND_SESSION_CONTROL_ACK_TIMEOUT_MS),
                receiver,
            )
            .await
            .map_err(|_| {
                String::from(
                    "Timed out waiting for the command session supervisor to acknowledge terminal input.",
                )
            })?
            .map_err(|_| {
                String::from("The command session input acknowledgement channel is closed.")
            })??;
        Ok(CommandSessionWriteOutcome {
            snapshot: record.snapshot().await,
            bytes_written: accepted_count,
        })
    }

    /// Resizes one owned running terminal session when its backend supports screen dimensions.
    /// 在后端支持屏幕尺寸时调整一条归属运行中终端会话的大小。
    pub async fn resize_session(
        &self,
        owner_session_id: SessionId,
        command_session_id: SessionId,
        rows: u16,
        cols: u16,
    ) -> Result<CommandSessionSnapshot, String> {
        let record = self.session_for_owner(owner_session_id, command_session_id)?;
        let (sender, receiver) = oneshot::channel();
        record
            .control_sender
            .send(CommandSessionControlMessage::Resize {
                rows,
                cols,
                responder: sender,
            })
            .map_err(|_| {
                String::from(
                    "The command session supervisor has exited and cannot resize the terminal.",
                )
            })?;
        timeout(
            Duration::from_millis(COMMAND_SESSION_CONTROL_ACK_TIMEOUT_MS),
            receiver,
        )
        .await
        .map_err(|_| {
            String::from(
                "Timed out waiting for the command session supervisor to acknowledge resize.",
            )
        })?
        .map_err(|_| {
            String::from("The command session resize acknowledgement channel is closed.")
        })??;
        Ok(record.snapshot().await)
    }

    /// Stops one owned command session and waits briefly for the process to reach a terminal state.
    /// 停止一条归属命令会话，并在短时间内等待进程到达终态。
    pub async fn stop_session(
        &self,
        owner_session_id: SessionId,
        command_session_id: SessionId,
        wait_for_exit_ms: Option<u64>,
    ) -> Result<CommandSessionSnapshot, String> {
        let record = self.session_for_owner(owner_session_id, command_session_id)?;
        let existing_snapshot = record.snapshot().await;
        if existing_snapshot.status != CommandSessionStatus::Running {
            return Ok(existing_snapshot);
        }

        let (sender, receiver) = oneshot::channel();
        record
            .control_sender
            .send(CommandSessionControlMessage::Stop { responder: sender })
            .map_err(|_| {
                String::from(
                    "The command session supervisor has exited and cannot stop the process.",
                )
            })?;
        let stop_acknowledged = timeout(
            Duration::from_millis(COMMAND_SESSION_CONTROL_ACK_TIMEOUT_MS),
            receiver,
        )
        .await;
        match stop_acknowledged {
            Ok(Ok(Ok(()))) => {
                record.shared_state.mark_stop_requested().await;
            }
            Ok(Ok(Err(error))) => return Err(error),
            Ok(Err(_)) | Err(_) => {
                record.shared_state.mark_stop_requested().await;
                return Ok(record.snapshot().await);
            }
        }

        let wait_for_exit_ms = wait_for_exit_ms.unwrap_or(DEFAULT_COMMAND_SESSION_STOP_WAIT_MS);
        let wait_started_at = Instant::now();
        loop {
            let notified = record.shared_state.change_notify.notified();
            let snapshot = record.snapshot().await;
            if snapshot.status != CommandSessionStatus::Running || wait_for_exit_ms == 0 {
                return Ok(snapshot);
            }
            let waited_ms = wait_started_at.elapsed().as_millis() as u64;
            let remaining_ms = wait_for_exit_ms.saturating_sub(waited_ms);
            if remaining_ms == 0 {
                return Ok(snapshot);
            }
            let _ = timeout(Duration::from_millis(remaining_ms), notified).await;
        }
    }

    /// Returns one best-effort live snapshot for the requested owned command session without blocking on async state, so WebUI status surfaces can prefer current runtime state over historical transcript payloads.
    /// 在不阻塞异步状态锁的前提下返回目标归属命令会话的一份尽力而为实时快照，使 WebUI 状态界面可以优先展示当前运行时状态而不是历史转录载荷。
    pub fn try_snapshot_owned_session(
        &self,
        owner_session_id: SessionId,
        command_session_id: SessionId,
    ) -> Option<CommandSessionSnapshot> {
        let record = self
            .sessions
            .read()
            .ok()?
            .get(&command_session_id)
            .cloned()?;
        if record.owner_session_id != owner_session_id {
            return None;
        }
        Some(
            record
                .try_snapshot()
                .unwrap_or_else(|| record.fallback_running_snapshot()),
        )
    }

    /// Reads buffered output with optional AI-facing cursor advancement.
    /// 按需读取缓冲输出，并可选推进面向 AI 的读取游标。
    async fn read_session_for_ai_with_cursor_mode(
        &self,
        owner_session_id: SessionId,
        command_session_id: SessionId,
        after_seq: Option<u64>,
        read_from_start: bool,
        wait_for_output_ms: Option<u64>,
        max_chunks: Option<usize>,
        advance_ai_cursor: bool,
    ) -> Result<CommandSessionReadOutcome, String> {
        let record = self.session_for_owner(owner_session_id, command_session_id)?;
        let after_seq = if read_from_start {
            after_seq.unwrap_or(0)
        } else if let Some(after_seq) = after_seq {
            after_seq
        } else {
            record.ai_delivered_seq().await
        };
        let max_chunks = max_chunks
            .unwrap_or(DEFAULT_COMMAND_SESSION_READ_MAX_CHUNKS)
            .max(1);
        let wait_for_output_ms = wait_for_output_ms.unwrap_or(DEFAULT_COMMAND_SESSION_READ_WAIT_MS);
        let wait_started_at = Instant::now();

        loop {
            let notified = record.shared_state.change_notify.notified();
            let outcome = record
                .read_since(after_seq, max_chunks, advance_ai_cursor)
                .await;
            if !outcome.returned_chunks.is_empty()
                || outcome.snapshot.status != CommandSessionStatus::Running
                || wait_for_output_ms == 0
            {
                if advance_ai_cursor {
                    record.mark_ai_delivered(&outcome.returned_chunks).await;
                }
                return Ok(outcome);
            }
            let waited_ms = wait_started_at.elapsed().as_millis() as u64;
            let remaining_ms = wait_for_output_ms.saturating_sub(waited_ms);
            if remaining_ms == 0 {
                return Ok(outcome);
            }
            let _ = timeout(Duration::from_millis(remaining_ms), notified).await;
        }
    }

    /// Reads buffered output for terminal_session and advances the AI-facing cursor by default.
    /// 为 terminal_session 读取缓冲输出，并默认推进面向 AI 的读取游标。
    pub async fn read_session_for_ai(
        &self,
        owner_session_id: SessionId,
        command_session_id: SessionId,
        after_seq: Option<u64>,
        read_from_start: bool,
        wait_for_output_ms: Option<u64>,
        max_chunks: Option<usize>,
    ) -> Result<CommandSessionReadOutcome, String> {
        self.read_session_for_ai_with_cursor_mode(
            owner_session_id,
            command_session_id,
            after_seq,
            read_from_start,
            wait_for_output_ms,
            max_chunks,
            true,
        )
        .await
    }

    /// Peeks buffered output for terminal_session without advancing the AI-facing cursor.
    /// 窥视 terminal_session 缓冲输出，但不推进面向 AI 的读取游标。
    pub async fn peek_session_for_ai(
        &self,
        owner_session_id: SessionId,
        command_session_id: SessionId,
        after_seq: Option<u64>,
        read_from_start: bool,
        wait_for_output_ms: Option<u64>,
        max_chunks: Option<usize>,
    ) -> Result<CommandSessionReadOutcome, String> {
        self.read_session_for_ai_with_cursor_mode(
            owner_session_id,
            command_session_id,
            after_seq,
            read_from_start,
            wait_for_output_ms,
            max_chunks,
            false,
        )
        .await
    }

    /// Returns one best-effort live snapshot for the requested command session identifier without enforcing ownership, so read-only WebUI projections can still recover the latest visible status when the owner id must be inferred from historical metadata.
    /// 在不强制校验归属的前提下返回目标命令会话标识的一份尽力而为实时快照，使只读 WebUI 投影在只能从历史元数据推断 owner id 时仍可恢复最新可见状态。
    pub fn try_snapshot_session_by_id(
        &self,
        command_session_id: SessionId,
    ) -> Option<CommandSessionSnapshot> {
        let record = self
            .sessions
            .read()
            .ok()?
            .get(&command_session_id)
            .cloned()?;
        Some(
            record
                .try_snapshot()
                .unwrap_or_else(|| record.fallback_running_snapshot()),
        )
    }

    /// Returns best-effort live snapshots for all retained command sessions.
    /// 返回当前保留的所有命令会话的尽力而为实时快照。
    pub fn try_snapshot_sessions(&self) -> Vec<CommandSessionSnapshot> {
        self.sessions
            .read()
            .map(|sessions| {
                sessions
                    .values()
                    .map(|record| {
                        record
                            .try_snapshot()
                            .unwrap_or_else(|| record.fallback_running_snapshot())
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    }

    /// Resolves one command-session record and enforces that the caller owns it.
    /// 解析一条命令会话记录并强制校验调用方是否拥有它。
    fn session_for_owner(
        &self,
        owner_session_id: SessionId,
        command_session_id: SessionId,
    ) -> Result<Arc<CommandSessionRecord>, String> {
        let record = self
            .sessions
            .read()
            .map_err(|_| {
                String::from("The command session registry is locked and cannot read the session.")
            })?
            .get(&command_session_id)
            .cloned()
            .ok_or_else(|| format!("Command session `{command_session_id}` was not found."))?;
        if record.owner_session_id != owner_session_id {
            return Err(format!(
                "Command session `{}` does not belong to the current controller session, so cross-session access is rejected.",
                command_session_id
            ));
        }
        Ok(record)
    }
}

/// Writes one stdin payload with a hard timeout so console-input TUI programs cannot block the supervisor forever.
/// 带硬超时写入一段标准输入载荷，避免控制台输入型 TUI 程序永久阻塞监管器。
async fn write_command_session_stdin(
    child_stdin: &mut ChildStdin,
    bytes: &[u8],
) -> Result<usize, String> {
    match timeout(
        Duration::from_millis(COMMAND_SESSION_STDIN_WRITE_TIMEOUT_MS),
        child_stdin.write_all(bytes),
    )
    .await
    {
        Ok(Ok(())) => Ok(bytes.len()),
        Ok(Err(error)) => Err(format!("Failed to write command-session stdin: {error}")),
        Err(_) => Err(format!(
            "Timed out writing command-session stdin after {COMMAND_SESSION_STDIN_WRITE_TIMEOUT_MS}ms; Windows console-input TUI programs such as msvcrt.getch() cannot consume pipe stdin."
        )),
    }
}

/// Requests child-process termination, using a Windows process-tree kill when available before falling back to Tokio's child kill primitive.
/// 请求终止子进程；在 Windows 上优先杀进程树，然后回退到 Tokio 子进程终止原语。
fn request_command_session_child_stop(child: &mut Child) -> Result<(), String> {
    #[cfg(windows)]
    {
        if let Some(process_id) = child.id() {
            let taskkill_status = StdCommand::new("taskkill")
                .args(["/PID", &process_id.to_string(), "/T", "/F"])
                .status();
            if let Ok(status) = taskkill_status {
                if status.success() {
                    return Ok(());
                }
            }
        }
    }
    child
        .start_kill()
        .map_err(|error| format!("Failed to stop the command-session process: {error}"))
}

/// Writes one PTY input payload through a blocking writer without blocking the async runtime thread.
/// 通过阻塞写入器写入一段 PTY 输入载荷，同时避免阻塞异步运行时线程。
async fn write_pty_command_session_stdin(
    writer: Arc<StdMutex<Box<dyn Write + Send>>>,
    bytes: Vec<u8>,
) -> Result<usize, String> {
    let byte_count = bytes.len();
    let write_task = tokio::task::spawn_blocking(move || {
        let mut writer = writer
            .lock()
            .map_err(|_| String::from("The PTY stdin writer lock is poisoned."))?;
        writer
            .write_all(&bytes)
            .map_err(|error| format!("Failed to write PTY stdin: {error}"))?;
        writer
            .flush()
            .map_err(|error| format!("Failed to flush PTY stdin: {error}"))?;
        Ok::<usize, String>(byte_count)
    });
    timeout(
        Duration::from_millis(COMMAND_SESSION_STDIN_WRITE_TIMEOUT_MS),
        write_task,
    )
    .await
    .map_err(|_| {
        format!("Timed out writing PTY stdin after {COMMAND_SESSION_STDIN_WRITE_TIMEOUT_MS}ms.")
    })?
    .map_err(|error| format!("The PTY stdin writer task failed: {error}"))?
}

/// Requests PTY child termination through a cloned child killer.
/// 通过克隆出的子进程终止器请求 PTY 子进程停止。
async fn request_pty_command_session_child_stop(
    killer: Arc<StdMutex<Box<dyn portable_pty::ChildKiller + Send + Sync>>>,
) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        let mut killer = killer
            .lock()
            .map_err(|_| String::from("The PTY child killer lock is poisoned."))?;
        killer
            .kill()
            .map_err(|error| format!("Failed to stop the PTY command-session process: {error}"))
    })
    .await
    .map_err(|error| format!("The PTY child killer task failed: {error}"))?
}

/// Rolling renderable screen frame synthesized from PTY output bytes.
/// 从 PTY 输出字节综合出的滚动可渲染屏幕帧。
struct PtyScreenFrameState {
    /// Screen rows retained for WebUI frame display.
    /// WebUI 帧显示保留的屏幕行数。
    rows: u16,
    /// Screen columns reported for WebUI frame display.
    /// WebUI 帧显示报告的屏幕列数。
    cols: u16,
    /// Completed visible lines retained in the rolling frame.
    /// 滚动帧中保留的已完成可见行。
    lines: Vec<String>,
    /// Current mutable line that has not ended with a newline.
    /// 尚未以换行结束的当前可变行。
    current_line: String,
    /// Monotonic frame sequence emitted when visible text changes.
    /// 可见文本变化时递增的帧序号。
    frame_seq: u64,
    /// Last emitted frame text used to suppress duplicate updates.
    /// 用于抑制重复更新的上一帧文本。
    last_frame_text: String,
}

impl PtyScreenFrameState {
    /// Creates one default rolling PTY screen frame state.
    /// 创建一份默认滚动 PTY 屏幕帧状态。
    fn new(rows: u16, cols: u16) -> Self {
        Self {
            rows,
            cols,
            lines: Vec::new(),
            current_line: String::new(),
            frame_seq: 0,
            last_frame_text: String::new(),
        }
    }

    /// Pushes one decoded PTY output fragment and returns a new frame when visible text changed.
    /// 推入一段已解码 PTY 输出，并在可见文本变化时返回新帧。
    fn push_output(&mut self, text: &str) -> Option<(u64, u16, u16, String)> {
        if text.is_empty() {
            return None;
        }
        let mut chars = text.chars().peekable();
        while let Some(ch) = chars.next() {
            match ch {
                '\x1b' => {
                    self.current_line.push(ch);
                    if chars.peek() == Some(&'[') {
                        self.current_line.push(chars.next().unwrap_or('['));
                        for next in chars.by_ref() {
                            self.current_line.push(next);
                            if next.is_ascii_alphabetic() || matches!(next, '~' | '@') {
                                break;
                            }
                        }
                    }
                }
                '\r' => {
                    self.current_line.clear();
                }
                '\n' => {
                    self.finish_current_line();
                }
                '\t' => {
                    self.current_line.push_str("    ");
                }
                ch if ch.is_control() => {}
                ch => {
                    self.current_line.push(ch);
                }
            }
        }
        let frame_text = self.frame_text();
        if frame_text.trim().is_empty() || frame_text == self.last_frame_text {
            return None;
        }
        self.frame_seq = self.frame_seq.saturating_add(1);
        self.last_frame_text = frame_text.clone();
        Some((self.frame_seq, self.rows, self.cols, frame_text))
    }

    /// Moves the current line into the retained line buffer.
    /// 将当前行移动到保留行缓冲区。
    fn finish_current_line(&mut self) {
        self.lines.push(std::mem::take(&mut self.current_line));
        let max_lines = self.rows.saturating_sub(1).max(1) as usize;
        if self.lines.len() > max_lines {
            let remove_count = self.lines.len() - max_lines;
            self.lines.drain(0..remove_count);
        }
    }

    /// Builds the current renderable frame text.
    /// 构建当前可渲染帧文本。
    fn frame_text(&self) -> String {
        let mut lines = self.lines.clone();
        if !self.current_line.is_empty() {
            lines.push(self.current_line.clone());
        }
        lines.join("\n")
    }
}

/// Appends PTY output to the chunk buffer and mirrors it into the live WebUI screen frame.
/// 将 PTY 输出追加到分片缓冲区，并镜像到 WebUI 实时屏幕帧。
fn append_pty_output_and_update_screen(
    shared_state: &Arc<CommandSessionSharedState>,
    screen_state: &mut PtyScreenFrameState,
    text: String,
) {
    if text.is_empty() {
        return;
    }
    shared_state.append_output_chunk_blocking(CommandSessionOutputStream::Stdout, text.clone());
    if let Some((seq, rows, cols, frame_text)) = screen_state.push_output(&text) {
        shared_state.update_screen_frame_blocking(seq, rows, cols, frame_text);
    }
}

/// Pumps one PTY reader stream into the shared command-session output buffer.
/// 将一条 PTY 读取流持续泵入共享命令会话输出缓冲区。
fn pump_pty_command_session_output(
    mut reader: Box<dyn Read + Send>,
    writer: Arc<StdMutex<Box<dyn Write + Send>>>,
    shared_state: Arc<CommandSessionSharedState>,
) {
    let mut output_decoder = ShellOutputStreamDecoder::new();
    let mut screen_state =
        PtyScreenFrameState::new(DEFAULT_PTY_SCREEN_ROWS, DEFAULT_PTY_SCREEN_COLS);
    let mut buffer = vec![0_u8; 4096];
    loop {
        match reader.read(&mut buffer) {
            Ok(0) => {
                let flushed_text = output_decoder.finish();
                let flushed_text = answer_pty_terminal_status_queries(flushed_text, &writer);
                append_pty_output_and_update_screen(&shared_state, &mut screen_state, flushed_text);
                break;
            }
            Ok(read_bytes) => {
                let text = output_decoder.push_bytes(&buffer[..read_bytes]);
                let text = answer_pty_terminal_status_queries(text, &writer);
                append_pty_output_and_update_screen(&shared_state, &mut screen_state, text);
            }
            Err(error) => {
                let flushed_text = output_decoder.finish();
                let flushed_text = answer_pty_terminal_status_queries(flushed_text, &writer);
                append_pty_output_and_update_screen(&shared_state, &mut screen_state, flushed_text);
                shared_state.append_output_chunk_blocking(
                    CommandSessionOutputStream::Stderr,
                    format!("[host] Failed to read PTY command session: {error}\n"),
                );
                break;
            }
        }
    }
}

/// Answers minimal ANSI terminal status queries required by ConPTY-hosted console programs.
/// 应答 ConPTY 承载的控制台程序所需的最小 ANSI 终端状态查询。
fn answer_pty_terminal_status_queries(
    text: String,
    writer: &Arc<StdMutex<Box<dyn Write + Send>>>,
) -> String {
    if !text.contains("\x1b[6n") {
        return text;
    }
    if let Ok(mut writer) = writer.lock() {
        let _ = writer.write_all(b"\x1b[1;1R");
        let _ = writer.flush();
    }
    text.replace("\x1b[6n", "")
}

/// Converts a helper stream label into the runtime output stream enum.
/// 将 helper 流标签转换为运行时输出流枚举。
fn helper_stream_label_to_output_stream(label: &str) -> CommandSessionOutputStream {
    if label.eq_ignore_ascii_case("stderr") {
        CommandSessionOutputStream::Stderr
    } else {
        CommandSessionOutputStream::Stdout
    }
}

/// Pumps Windows console helper stdout JSONL events into an internal channel.
/// 将 Windows 控制台 helper 的 stdout JSONL 事件泵入内部通道。
#[cfg(windows)]
async fn pump_windows_console_helper_events(
    stdout: ChildStdout,
    event_sender: UnboundedSender<WindowsConsoleHelperEvent>,
    shared_state: Arc<CommandSessionSharedState>,
) {
    let mut lines = BufReader::new(stdout).lines();
    loop {
        match lines.next_line().await {
            Ok(Some(line)) => match serde_json::from_str::<WindowsConsoleHelperEvent>(&line) {
                Ok(event) => {
                    let _ = event_sender.send(event);
                }
                Err(error) => {
                    shared_state
                        .append_output_chunk(
                            CommandSessionOutputStream::Stderr,
                            format!("[host] Failed to parse Windows console helper event: {error}: {line}\n"),
                        )
                        .await;
                }
            },
            Ok(None) => break,
            Err(error) => {
                shared_state
                    .append_output_chunk(
                        CommandSessionOutputStream::Stderr,
                        format!("[host] Failed to read Windows console helper stdout: {error}\n"),
                    )
                    .await;
                break;
            }
        }
    }
}

/// Runs the Windows console helper supervisor that owns helper lifecycle and JSONL control.
/// 运行 Windows 控制台 helper 监管器，持有 helper 生命周期与 JSONL 控制通道。
#[cfg(windows)]
async fn run_windows_console_helper_supervisor(
    record: Arc<CommandSessionRecord>,
    mut child: Child,
    mut helper_stdin: ChildStdin,
    helper_stdout: ChildStdout,
    helper_stderr: ChildStderr,
    mut control_receiver: UnboundedReceiver<CommandSessionControlMessage>,
) {
    let (event_sender, mut event_receiver) = unbounded_channel();
    let stdout_state = Arc::clone(&record.shared_state);
    tokio::spawn(pump_windows_console_helper_events(
        helper_stdout,
        event_sender,
        stdout_state,
    ));
    tokio::spawn(pump_command_session_output(
        helper_stderr,
        CommandSessionOutputStream::Stderr,
        Arc::clone(&record.shared_state),
    ));

    let mut stop_requested = false;
    let mut target_exit_code = None;
    let mut target_succeeded = false;
    let mut wait_task = Box::pin(child.wait());

    loop {
        tokio::select! {
            wait_result = &mut wait_task => {
                match wait_result {
                    Ok(status) => {
                        if target_exit_code.is_none() {
                            target_exit_code = status.code();
                            target_succeeded = status.success();
                        }
                        break;
                    }
                    Err(error) => {
                        record.shared_state.remember_failure_reason(format!("Failed to wait for Windows console helper: {error}")).await;
                        break;
                    }
                }
            }
            maybe_event = event_receiver.recv() => {
                match maybe_event {
                    Some(WindowsConsoleHelperEvent::Started { pid }) => {
                        record.shared_state.append_output_chunk(
                            CommandSessionOutputStream::Stdout,
                            format!("[host] Windows console helper started target pid {pid}.\n"),
                        ).await;
                    }
                    Some(WindowsConsoleHelperEvent::Output { stream, text }) => {
                        record.shared_state.append_output_chunk(
                            helper_stream_label_to_output_stream(&stream),
                            text,
                        ).await;
                    }
                    Some(WindowsConsoleHelperEvent::Frame { seq, rows, cols, text }) => {
                        record.shared_state.update_screen_frame(seq, rows, cols, text.clone()).await;
                        record.shared_state.append_output_chunk(
                            CommandSessionOutputStream::Stdout,
                            format!("[screen #{seq}]\n{text}\n"),
                        ).await;
                    }
                    Some(WindowsConsoleHelperEvent::Exited { exit_code, stopped }) => {
                        stop_requested = stop_requested || stopped;
                        target_exit_code = exit_code;
                        target_succeeded = exit_code == Some(0);
                        break;
                    }
                    Some(WindowsConsoleHelperEvent::Error { message }) => {
                        record.shared_state.remember_failure_reason(message).await;
                    }
                    None => {
                        break;
                    }
                }
            }
            maybe_message = control_receiver.recv() => {
                match maybe_message {
                    Some(CommandSessionControlMessage::Write { bytes, responder }) => {
                        let text = String::from_utf8_lossy(&bytes).into_owned();
                        let request = WindowsConsoleHelperRequest::Text { text };
                        let result = write_windows_console_helper_request(&mut helper_stdin, &request).await.map(|_| bytes.len());
                        if result.is_ok() {
                            record.shared_state.set_pending_stdin_line(stdin_payload_bytes_leave_pending_line(&bytes)).await;
                        }
                        let _ = responder.send(result);
                    }
                    Some(CommandSessionControlMessage::InputEvents { events, responder }) => {
                        let event_count = events.len();
                        let request = WindowsConsoleHelperRequest::Input { events };
                        let result = write_windows_console_helper_request(&mut helper_stdin, &request).await.map(|_| event_count);
                        if result.is_ok() {
                            record.shared_state.set_pending_stdin_line(false).await;
                        }
                        let _ = responder.send(result);
                    }
                    Some(CommandSessionControlMessage::Resize { rows, cols, responder }) => {
                        let request = WindowsConsoleHelperRequest::Resize { rows, cols };
                        let result = write_windows_console_helper_request(&mut helper_stdin, &request).await;
                        let _ = responder.send(result);
                    }
                    Some(CommandSessionControlMessage::Stop { responder }) => {
                        stop_requested = true;
                        let stop_result = write_windows_console_helper_request(&mut helper_stdin, &WindowsConsoleHelperRequest::Stop).await;
                        if stop_result.is_ok() {
                            record.shared_state.mark_stop_requested().await;
                        }
                        let _ = responder.send(stop_result);
                    }
                    None => {
                        stop_requested = true;
                        let _ = write_windows_console_helper_request(&mut helper_stdin, &WindowsConsoleHelperRequest::Stop).await;
                        record.shared_state.mark_stop_requested().await;
                    }
                }
            }
        }
    }

    record
        .shared_state
        .finalize_process_exit(stop_requested, target_exit_code, None, target_succeeded)
        .await;
    record.shared_state.change_notify.notify_waiters();
}

/// Runs the background PTY supervisor that owns child lifecycle and terminal input control.
/// 运行后台 PTY 监管器，持有子进程生命周期与终端输入控制通道。
async fn run_pty_command_session_supervisor(
    record: Arc<CommandSessionRecord>,
    _master: Box<dyn portable_pty::MasterPty + Send>,
    mut child: Box<dyn portable_pty::Child + Send + Sync>,
    reader: Box<dyn Read + Send>,
    writer: Box<dyn Write + Send>,
    mut control_receiver: UnboundedReceiver<CommandSessionControlMessage>,
    initial_stdin: Option<String>,
    initial_stdin_append_newline: bool,
) {
    let writer = Arc::new(StdMutex::new(writer));
    let killer = Arc::new(StdMutex::new(child.clone_killer()));
    let reader_state = Arc::clone(&record.shared_state);
    let reader_writer = Arc::clone(&writer);
    std::thread::spawn(move || {
        pump_pty_command_session_output(reader, reader_writer, reader_state)
    });

    let mut wait_task = tokio::task::spawn_blocking(move || {
        let wait_result = child.wait();
        match wait_result {
            Ok(status) => (Some(status.exit_code() as i32), None, status.success()),
            Err(error) => (
                None,
                Some(format!(
                    "Failed to wait for the PTY command session to exit: {error}"
                )),
                false,
            ),
        }
    });

    let mut stop_requested = false;
    if let Some(initial_stdin) = initial_stdin {
        let initial_stdin = if initial_stdin_append_newline {
            format!("{initial_stdin}{}", record.interactive_newline.as_str())
        } else {
            initial_stdin
        };
        match write_pty_command_session_stdin(
            Arc::clone(&writer),
            initial_stdin.clone().into_bytes(),
        )
        .await
        {
            Ok(_) => {
                record
                    .shared_state
                    .set_pending_stdin_line(stdin_payload_leaves_pending_line(&initial_stdin))
                    .await;
            }
            Err(error) => {
                record.shared_state.remember_failure_reason(error).await;
                let _ = request_pty_command_session_child_stop(Arc::clone(&killer)).await;
                record.shared_state.mark_stop_requested().await;
                stop_requested = true;
            }
        }
    }

    let (exit_code, wait_error, process_succeeded) = loop {
        tokio::select! {
            wait_result = &mut wait_task => {
                match wait_result {
                    Ok(result) => break result,
                    Err(error) => {
                        break (
                            None,
                            Some(format!("The PTY command-session wait task failed: {error}")),
                            false,
                        );
                    }
                }
            }
            maybe_message = control_receiver.recv() => {
                match maybe_message {
                    Some(CommandSessionControlMessage::Write { bytes, responder }) => {
                        let write_result = match write_pty_command_session_stdin(Arc::clone(&writer), bytes.clone()).await {
                            Ok(bytes_written) => {
                                record
                                    .shared_state
                                    .set_pending_stdin_line(stdin_payload_bytes_leave_pending_line(&bytes))
                                    .await;
                                Ok(bytes_written)
                            }
                            Err(error) => Err(error),
                        };
                        let _ = responder.send(write_result);
                    }
                    Some(CommandSessionControlMessage::InputEvents { responder, .. }) => {
                        let _ = responder.send(Err(String::from(
                            "Structured terminal input events are only supported by the Windows native console backend.",
                        )));
                    }
                    Some(CommandSessionControlMessage::Resize { responder, .. }) => {
                        let _ = responder.send(Err(String::from(
                            "Terminal resize is not yet supported by the current PTY runtime.",
                        )));
                    }
                    Some(CommandSessionControlMessage::Stop { responder }) => {
                        stop_requested = true;
                        let stop_result = request_pty_command_session_child_stop(Arc::clone(&killer)).await;
                        if stop_result.is_ok() {
                            record.shared_state.mark_stop_requested().await;
                        }
                        let _ = responder.send(stop_result);
                    }
                    None => {
                        stop_requested = true;
                        let _ = request_pty_command_session_child_stop(Arc::clone(&killer)).await;
                        record.shared_state.mark_stop_requested().await;
                    }
                }
            }
        }
    };

    record
        .shared_state
        .finalize_process_exit(stop_requested, exit_code, wait_error, process_succeeded)
        .await;
}

/// Pumps one stdout or stderr stream into the shared command-session output buffer.
/// 将一条 stdout 或 stderr 流持续泵入共享命令会话输出缓冲区。
async fn pump_command_session_output<R>(
    mut reader: R,
    stream: CommandSessionOutputStream,
    shared_state: Arc<CommandSessionSharedState>,
) where
    R: AsyncRead + Unpin + Send + 'static,
{
    // Incremental shell-output decoder that preserves split multibyte characters across read chunks.
    // 在多次读取分片之间保留拆分多字节字符的增量 shell 输出解码器。
    let mut output_decoder = ShellOutputStreamDecoder::new();
    let mut buffer = vec![0_u8; 4096];
    loop {
        match reader.read(&mut buffer).await {
            Ok(0) => {
                let flushed_text = output_decoder.finish();
                shared_state.append_output_chunk(stream, flushed_text).await;
                break;
            }
            Ok(read_bytes) => {
                let text = output_decoder.push_bytes(&buffer[..read_bytes]);
                shared_state.append_output_chunk(stream, text).await;
            }
            Err(error) => {
                let flushed_text = output_decoder.finish();
                shared_state.append_output_chunk(stream, flushed_text).await;
                shared_state
                    .append_output_chunk(
                        CommandSessionOutputStream::Stderr,
                        format!(
                            "[host] Failed to read command session {}: {error}\n",
                            stream.as_str()
                        ),
                    )
                    .await;
                break;
            }
        }
    }
}

/// Runs the background process supervisor that owns the child process lifecycle and stdin control channel.
/// 运行后台进程监管器，持有子进程生命周期与标准输入控制通道。
async fn run_command_session_supervisor(
    record: Arc<CommandSessionRecord>,
    mut child: Child,
    mut stdin: Option<ChildStdin>,
    stdout: ChildStdout,
    stderr: ChildStderr,
    mut control_receiver: UnboundedReceiver<CommandSessionControlMessage>,
    initial_stdin: Option<String>,
    initial_stdin_append_newline: bool,
) {
    let stdout_task = tokio::spawn(pump_command_session_output(
        stdout,
        CommandSessionOutputStream::Stdout,
        Arc::clone(&record.shared_state),
    ));
    let stderr_task = tokio::spawn(pump_command_session_output(
        stderr,
        CommandSessionOutputStream::Stderr,
        Arc::clone(&record.shared_state),
    ));

    let mut stop_requested = false;
    if let Some(initial_stdin) = initial_stdin {
        let initial_stdin = if initial_stdin_append_newline {
            format!("{initial_stdin}{}", record.interactive_newline.as_str())
        } else {
            initial_stdin
        };
        match stdin.as_mut() {
            Some(child_stdin) => {
                if let Err(error) =
                    write_command_session_stdin(child_stdin, initial_stdin.as_bytes()).await
                {
                    record
                        .shared_state
                        .remember_failure_reason(format!(
                            "Failed to write the initial command-session stdin: {error}"
                        ))
                        .await;
                    let _ = request_command_session_child_stop(&mut child);
                    record.shared_state.mark_stop_requested().await;
                    stop_requested = true;
                } else {
                    record
                        .shared_state
                        .set_pending_stdin_line(stdin_payload_leaves_pending_line(&initial_stdin))
                        .await;
                }
            }
            None => {
                record
                    .shared_state
                    .remember_failure_reason(String::from(
                        "The command session did not expose a writable stdin channel.",
                    ))
                    .await;
            }
        }
    }

    let mut control_channel_open = true;
    let (exit_code, wait_error, process_succeeded) = loop {
        if !control_channel_open {
            match child.wait().await {
                Ok(status) => break (status.code(), None, status.success()),
                Err(error) => {
                    break (
                        None,
                        Some(format!(
                            "Failed to wait for the command session to exit: {error}"
                        )),
                        false,
                    );
                }
            }
        }

        tokio::select! {
            wait_result = child.wait() => {
                match wait_result {
                    Ok(status) => break (status.code(), None, status.success()),
                    Err(error) => {
                        break (
                            None,
                            Some(format!("Failed to wait for the command session to exit: {error}")),
                            false,
                        );
                    }
                }
            }
            maybe_message = control_receiver.recv() => {
                match maybe_message {
                    Some(CommandSessionControlMessage::Write { bytes, responder }) => {
                        let write_result = match stdin.as_mut() {
                            Some(child_stdin) => {
                                match write_command_session_stdin(child_stdin, &bytes).await {
                                    Ok(bytes_written) => {
                                        record
                                            .shared_state
                                            .set_pending_stdin_line(stdin_payload_bytes_leave_pending_line(&bytes))
                                            .await;
                                        Ok(bytes_written)
                                    }
                                    Err(error) => {
                                        stop_requested = true;
                                        stdin = None;
                                        let _ = request_command_session_child_stop(&mut child);
                                        record.shared_state.mark_stop_requested().await;
                                        Err(error)
                                    }
                                }
                            }
                            None => Err(String::from(
                                "The command session does not currently have a writable stdin channel.",
                            )),
                        };
                        let _ = responder.send(write_result);
                    }
                    Some(CommandSessionControlMessage::InputEvents { responder, .. }) => {
                        let _ = responder.send(Err(String::from(
                            "Structured terminal input events are only supported by the Windows native console backend.",
                        )));
                    }
                    Some(CommandSessionControlMessage::Resize { responder, .. }) => {
                        let _ = responder.send(Err(String::from(
                            "Terminal resize is not supported by pipe-backed command sessions.",
                        )));
                    }
                    Some(CommandSessionControlMessage::Stop { responder }) => {
                        stop_requested = true;
                        stdin = None;
                        let stop_result = request_command_session_child_stop(&mut child);
                        if stop_result.is_ok() {
                            record.shared_state.mark_stop_requested().await;
                        }
                        let _ = responder.send(stop_result);
                    }
                    None => {
                        control_channel_open = false;
                    }
                }
            }
        }
    };

    let _ = stdout_task.await;
    let _ = stderr_task.await;
    record
        .shared_state
        .finalize_process_exit(stop_requested, exit_code, wait_error, process_succeeded)
        .await;
    record.shared_state.change_notify.notify_waiters();
}

#[cfg(test)]
mod tests {
    use super::{
        COMMAND_SESSION_OUTPUT_CHUNK_COALESCE_SOFT_LIMIT_BYTES, CommandSessionBackend,
        CommandSessionOutputChunk, CommandSessionOutputStream, CommandSessionRecord,
        CommandSessionRuntime, CommandSessionSharedState, CommandSessionStatus,
        PtyScreenFrameState, can_coalesce_output_chunk, output_chunk_ends_with_line_boundary,
    };
    #[cfg(windows)]
    use super::{CommandSessionInputEvent, CommandSessionStartRequest};
    use crate::model::SessionId;
    use std::path::PathBuf;
    use std::sync::Arc;
    use tokio::sync::mpsc::unbounded_channel;

    /// Output coalescing should keep appending to one undelivered line fragment until the line is completed.
    /// 输出合并应持续追加到同一条未投递行碎片，直到该行被完整结束。
    #[test]
    fn output_chunk_coalescing_prefers_undelivered_partial_lines() {
        let last_chunk = CommandSessionOutputChunk {
            seq: 3,
            stream: CommandSessionOutputStream::Stdout,
            text: String::from("partial"),
        };
        assert!(can_coalesce_output_chunk(
            Some(&last_chunk),
            2,
            CommandSessionOutputStream::Stdout,
            8,
        ));
        assert!(!can_coalesce_output_chunk(
            Some(&last_chunk),
            3,
            CommandSessionOutputStream::Stdout,
            8,
        ));
        assert!(!can_coalesce_output_chunk(
            Some(&last_chunk),
            2,
            CommandSessionOutputStream::Stderr,
            8,
        ));
    }

    /// Output coalescing should stop once the prior chunk already ended with a line boundary or the soft size limit would be exceeded.
    /// 一旦前一个分片已经结束在行边界，或继续合并会超过软大小阈值，输出合并就应停止。
    #[test]
    fn output_chunk_coalescing_respects_line_boundaries_and_soft_limit() {
        let line_chunk = CommandSessionOutputChunk {
            seq: 4,
            stream: CommandSessionOutputStream::Stdout,
            text: String::from("line complete\n"),
        };
        assert!(output_chunk_ends_with_line_boundary(&line_chunk.text));
        assert!(!can_coalesce_output_chunk(
            Some(&line_chunk),
            0,
            CommandSessionOutputStream::Stdout,
            1,
        ));

        let oversized_chunk = CommandSessionOutputChunk {
            seq: 5,
            stream: CommandSessionOutputStream::Stdout,
            text: "x".repeat(COMMAND_SESSION_OUTPUT_CHUNK_COALESCE_SOFT_LIMIT_BYTES),
        };
        assert!(!can_coalesce_output_chunk(
            Some(&oversized_chunk),
            0,
            CommandSessionOutputStream::Stdout,
            1,
        ));
    }

    /// PTY frame synthesis should keep a rolling live screen for WebUI display.
    /// PTY 帧综合应为 WebUI 展示保留滚动实时屏幕。
    #[test]
    fn pty_screen_frame_state_builds_rolling_live_frame() {
        let mut state = PtyScreenFrameState::new(3, 80);
        let first = state
            .push_output("one\n\x1b[31mtwo\x1b[0m\nthree")
            .expect("first frame");
        assert_eq!(first.0, 1);
        assert_eq!(first.1, 3);
        assert_eq!(first.2, 80);
        assert!(first.3.contains("\x1b[31mtwo\x1b[0m"));
        assert!(first.3.contains("three"));

        let second = state.push_output("\rTHREE\nfour").expect("second frame");
        assert_eq!(second.0, 2);
        assert!(!second.3.contains("one"));
        assert!(second.3.contains("THREE"));
        assert!(second.3.contains("four"));
    }

    /// Best-effort live snapshots should expose the latest shared state to synchronous WebUI callers without crossing ownership boundaries.
    /// 尽力而为的实时快照应在不跨越归属边界的前提下，把最新共享状态暴露给同步 WebUI 调用方。
    #[test]
    fn try_snapshot_owned_session_returns_live_state_without_async_wait() {
        let runtime = CommandSessionRuntime::new();
        let command_session_id = SessionId::from_u128(0xCA11);
        let owner_session_id = SessionId::from_u128(0x0A11);
        let shared_state = Arc::new(CommandSessionSharedState::default());
        {
            let mut state = shared_state
                .mutable_state
                .try_lock()
                .expect("lock shared state");
            state.status = CommandSessionStatus::Stopped;
            state.exit_code = Some(9);
            state.failure_reason = Some(String::from("stopped by test"));
            state.stdout_bytes = 128;
            state.stderr_bytes = 16;
            state.last_seq = 5;
            state.pending_stdin_line = true;
        }
        let (control_sender, _control_receiver) = unbounded_channel();
        runtime
            .sessions
            .write()
            .expect("lock runtime registry")
            .insert(
                command_session_id,
                Arc::new(CommandSessionRecord {
                    command_session_id,
                    owner_session_id,
                    display_name: None,
                    command: String::from("echo live"),
                    shell_label: String::from("pwsh"),
                    launcher_shell_label: String::from("pwsh"),
                    interactive_newline: String::from("\r\n"),
                    cwd: PathBuf::from(r"D:\openvulcan\vulcan-code"),
                    backend: CommandSessionBackend::Pipe,
                    control_sender,
                    shared_state,
                }),
            );

        let snapshot = runtime
            .try_snapshot_owned_session(owner_session_id, command_session_id)
            .expect("read live snapshot");
        assert_eq!(snapshot.command_session_id, command_session_id);
        assert_eq!(snapshot.owner_session_id, owner_session_id);
        assert_eq!(snapshot.status, CommandSessionStatus::Stopped);
        assert_eq!(snapshot.exit_code, Some(9));
        assert_eq!(snapshot.failure_reason.as_deref(), Some("stopped by test"));
        assert_eq!(snapshot.stdout_bytes, 128);
        assert_eq!(snapshot.stderr_bytes, 16);
        assert_eq!(snapshot.last_seq, 5);
        assert!(snapshot.pending_stdin_line);
        assert!(
            runtime
                .try_snapshot_owned_session(SessionId::from_u128(0x0A12), command_session_id)
                .is_none()
        );
        assert!(
            runtime
                .try_snapshot_session_by_id(command_session_id)
                .is_some()
        );
    }

    /// Peeking should not advance the AI-facing cursor used by terminal_session reads.
    /// 窥视读取不应推进 terminal_session 使用的面向 AI 的读取游标。
    #[tokio::test]
    async fn peek_session_for_ai_does_not_advance_ai_cursor() {
        let runtime = CommandSessionRuntime::new();
        let command_session_id = SessionId::from_u128(0xCA12);
        let owner_session_id = SessionId::from_u128(0x0A13);
        let shared_state = Arc::new(CommandSessionSharedState::default());
        let (control_sender, _control_receiver) = unbounded_channel();
        runtime
            .sessions
            .write()
            .expect("lock runtime registry")
            .insert(
                command_session_id,
                Arc::new(CommandSessionRecord {
                    command_session_id,
                    owner_session_id,
                    display_name: Some(String::from("peek test")),
                    command: String::from("echo visible"),
                    shell_label: String::from("pwsh"),
                    launcher_shell_label: String::from("pwsh"),
                    interactive_newline: String::from("\r\n"),
                    cwd: PathBuf::from(r"D:\openvulcan\vulcan-code"),
                    backend: CommandSessionBackend::Pipe,
                    control_sender,
                    shared_state: Arc::clone(&shared_state),
                }),
            );
        shared_state
            .append_output_chunk(
                CommandSessionOutputStream::Stdout,
                String::from("visible\n"),
            )
            .await;

        let peeked = runtime
            .peek_session_for_ai(
                owner_session_id,
                command_session_id,
                None,
                false,
                Some(0),
                None,
            )
            .await
            .expect("peek should read buffered output");
        assert_eq!(peeked.returned_chunks.len(), 1);
        assert_eq!(peeked.returned_chunks[0].text, "visible\n");

        let read = runtime
            .read_session_for_ai(
                owner_session_id,
                command_session_id,
                None,
                false,
                Some(0),
                None,
            )
            .await
            .expect("normal read should still see peeked output");
        assert_eq!(read.returned_chunks.len(), 1);
        assert_eq!(read.returned_chunks[0].text, "visible\n");

        let second_read = runtime
            .read_session_for_ai(
                owner_session_id,
                command_session_id,
                None,
                false,
                Some(0),
                None,
            )
            .await
            .expect("second normal read should consume no duplicate output");
        assert!(second_read.returned_chunks.is_empty());
    }

    /// Verifies the Windows console backend delivers extended keys to `msvcrt.getch`.
    /// 验证 Windows 控制台后端能把扩展按键传递给 `msvcrt.getch`。
    #[cfg(windows)]
    #[tokio::test]
    async fn windows_console_backend_delivers_msvcrt_arrow_keys() {
        if super::resolve_windows_console_helper_path().is_err() {
            return;
        }
        let python = match resolve_test_python_executable() {
            Some(python) => python,
            None => return,
        };
        let runtime = Arc::new(CommandSessionRuntime::new());
        let owner_session_id = SessionId::from_u128(0x0BEE);
        let script = concat!(
            "import msvcrt, sys\n",
            "items=['one','two','three']\n",
            "idx=0\n",
            "sys.stdout.write('ready\\n')\n",
            "sys.stdout.flush()\n",
            "while True:\n",
            "    ch=msvcrt.getch()\n",
            "    if ch in (b'\\x00', b'\\xe0'):\n",
            "        ext=msvcrt.getch()\n",
            "        if ext == b'P':\n",
            "            idx=(idx+1)%len(items)\n",
            "            sys.stdout.write('index=%d\\n' % idx)\n",
            "            sys.stdout.flush()\n",
            "        elif ext == b'H':\n",
            "            idx=(idx-1)%len(items)\n",
            "            sys.stdout.write('index=%d\\n' % idx)\n",
            "            sys.stdout.flush()\n",
            "    elif ch in (b'\\r', b'\\n'):\n",
            "        sys.stdout.write('selected=%s\\n' % items[idx])\n",
            "        sys.stdout.flush()\n",
            "        break\n",
        );
        let snapshot = runtime
            .start_session(CommandSessionStartRequest {
                owner_session_id,
                display_name: Some(String::from("msvcrt test")),
                command: format!("{python} -c <msvcrt getch>"),
                shell_label: String::from("python"),
                launcher_shell_label: String::from("python"),
                cwd: std::env::current_dir().expect("current dir"),
                launcher_program: python,
                launcher_args: vec![String::from("-c"), String::from(script)],
                initial_stdin: None,
                initial_stdin_append_newline: false,
                interactive_newline: String::from("\r\n"),
                backend: CommandSessionBackend::WindowsConsole,
            })
            .await
            .expect("start windows console session");

        let (ready_output, ready_seq) = read_test_session_until_text(
            &runtime,
            owner_session_id,
            snapshot.command_session_id,
            0,
            "ready",
        )
        .await
        .expect("read ready");
        assert!(
            ready_output.contains("ready"),
            "expected ready output, got {ready_output:?}"
        );
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        runtime
            .write_session_input_events(
                owner_session_id,
                snapshot.command_session_id,
                vec![
                    CommandSessionInputEvent::Key {
                        key: String::from("ArrowDown"),
                    },
                    CommandSessionInputEvent::Key {
                        key: String::from("Enter"),
                    },
                ],
            )
            .await
            .expect("write getch input events");
        let (output, _) = read_test_session_until_text(
            &runtime,
            owner_session_id,
            snapshot.command_session_id,
            ready_seq,
            "selected=two",
        )
        .await
        .expect("read getch result");
        assert!(
            output.contains("selected=two"),
            "unexpected output: {output:?}"
        );
    }

    /// Verifies Windows console text input can deliver ANSI VT escape bytes to stdin readers.
    /// 验证 Windows 控制台文本输入可向 stdin 读取器传递 ANSI VT 转义字节。
    #[cfg(windows)]
    #[tokio::test]
    async fn windows_console_backend_delivers_ansi_vt_text_to_stdin_readers() {
        if super::resolve_windows_console_helper_path().is_err() {
            return;
        }
        let python = match resolve_test_python_executable() {
            Some(python) => python,
            None => return,
        };
        let runtime = Arc::new(CommandSessionRuntime::new());
        let owner_session_id = SessionId::from_u128(0x0BEF);
        let script = concat!(
            "import sys\n",
            "sys.stdout.write('ready\\n')\n",
            "sys.stdout.flush()\n",
            "values=[]\n",
            "while len(values) < 3:\n",
            "    ch=sys.stdin.read(1)\n",
            "    if not ch:\n",
            "        break\n",
            "    values.append(ord(ch))\n",
            "sys.stdout.write('codes=' + ','.join(str(v) for v in values) + '\\n')\n",
            "sys.stdout.flush()\n",
        );
        let snapshot = runtime
            .start_session(CommandSessionStartRequest {
                owner_session_id,
                display_name: Some(String::from("stdin vt test")),
                command: format!("{python} -c <stdin vt>"),
                shell_label: String::from("python"),
                launcher_shell_label: String::from("python"),
                cwd: std::env::current_dir().expect("current dir"),
                launcher_program: python,
                launcher_args: vec![String::from("-c"), String::from(script)],
                initial_stdin: None,
                initial_stdin_append_newline: false,
                interactive_newline: String::from("\r\n"),
                backend: CommandSessionBackend::WindowsConsole,
            })
            .await
            .expect("start windows console session");
        let (_, ready_seq) = read_test_session_until_text(
            &runtime,
            owner_session_id,
            snapshot.command_session_id,
            0,
            "ready",
        )
        .await
        .expect("read ready");
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        runtime
            .write_session_input_events(
                owner_session_id,
                snapshot.command_session_id,
                vec![CommandSessionInputEvent::Text {
                    text: String::from("\x1b[B\n"),
                }],
            )
            .await
            .expect("write ansi vt text input");
        let (output, _) = read_test_session_until_text(
            &runtime,
            owner_session_id,
            snapshot.command_session_id,
            ready_seq,
            "codes=27,91,66",
        )
        .await
        .expect("read stdin result");
        assert!(
            output.contains("codes=27,91,66"),
            "unexpected output: {output:?}"
        );
    }

    /// Reads one test command session until the expected text appears or the retry budget is exhausted.
    /// 读取测试命令会话，直到出现预期文本或耗尽重试预算。
    #[cfg(windows)]
    async fn read_test_session_until_text(
        runtime: &Arc<CommandSessionRuntime>,
        owner_session_id: SessionId,
        command_session_id: SessionId,
        mut after_seq: u64,
        expected_text: &str,
    ) -> Option<(String, u64)> {
        let mut output = String::new();
        for _ in 0..20 {
            let result = runtime
                .read_session(
                    owner_session_id,
                    command_session_id,
                    Some(after_seq),
                    Some(250),
                    Some(32),
                )
                .await
                .ok()?;
            for chunk in &result.returned_chunks {
                after_seq = after_seq.max(chunk.seq);
                output.push_str(&chunk.text);
            }
            if output.contains(expected_text)
                || result.snapshot.status != CommandSessionStatus::Running
            {
                return Some((output, after_seq));
            }
        }
        Some((output, after_seq))
    }

    /// Resolves a Python executable for Windows console-input regression tests.
    /// 为 Windows 控制台输入回归测试解析 Python 可执行程序。
    #[cfg(windows)]
    fn resolve_test_python_executable() -> Option<String> {
        for candidate in ["python", "py"] {
            let output = std::process::Command::new(candidate)
                .arg("--version")
                .output();
            if output.is_ok_and(|output| output.status.success()) {
                return Some(candidate.to_string());
            }
        }
        None
    }
}
