//! Host-managed runtime for long-lived shell sessions and incremental command I/O.
//! 宿主管理的长生命周期 shell 会话与增量命令读写运行时。

use crate::model::SessionId;
use crate::output_decode::ShellOutputStreamDecoder;
use crate::process_creation::{WINDOWS_CREATE_NO_WINDOW, apply_windows_release_no_window};
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

mod pipe;
mod pty;
mod runtime;
mod state;
mod windows_console;

#[cfg(test)]
mod tests;

use pipe::*;
use pty::*;
use state::*;
use windows_console::*;

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
        if track_delivery && let Some(last_returned_chunk) = returned_chunks.last() {
            state.highest_delivered_seq = state.highest_delivered_seq.max(last_returned_chunk.seq);
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

/// One host-managed runtime that owns long-lived shell sessions and their process supervisors.
/// 持有长生命周期 shell 会话及其进程监管器的一份宿主管理运行时。
pub struct CommandSessionRuntime {
    /// Registry of command sessions keyed by their stable command-session identifier.
    /// 以稳定命令会话标识为键建立的命令会话注册表。
    sessions: RwLock<HashMap<SessionId, Arc<CommandSessionRecord>>>,
}
