//! Job registry and process supervision.
//! 任务注册表与进程监管。

use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock, PoisonError, Weak};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::{Child, Command as TokioCommand};
use tokio::sync::{broadcast, watch};
use tokio::task::JoinHandle;

use super::index::{
    ShellJobIndexError, ShellJobIndexLine, append_index_line, read_session_job_records,
};
use super::log::{ShellJobLogReader, ShellJobLogStats, ShellJobLogWriter};
use super::text::floor_char_boundary;
use super::{
    MAX_RUNNING_BACKGROUND_SHELL_JOBS, SHELL_JOB_SESSION_LOG_BUDGET_BYTES,
    SHELL_JOB_STREAM_CAPTURE_BYTES,
};
use crate::output_decode::ShellOutputStreamDecoder;
use crate::process_tree::{
    CHILD_OUTPUT_DRAIN_GRACE, ChildProcessTreeDropGuard, isolate_child_process_tree,
    terminate_child_process_tree,
};

/// Finished jobs kept in memory per session; older ones are still listed from the index on disk.
/// 每个会话在内存中保留的已结束任务数；更早的仍可从磁盘索引列出。
const RETAINED_FINISHED_JOBS_PER_SESSION: usize = 16;

/// Capacity of the job event channel.
/// 任务事件通道的容量。
const SHELL_JOB_EVENT_CAPACITY: usize = 256;

/// Bytes read from one output pipe at a time.
/// 每次从一个输出管道读取的字节数。
const OUTPUT_READ_CHUNK_BYTES: usize = 8192;

/// Tool call that started a job.
/// 启动任务的工具调用。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShellJobOrigin {
    /// API call that issued the tool call.
    /// 发出该工具调用的 API 调用。
    pub api_call_id: u64,
    /// Tool call inside that API call.
    /// 该 API 调用内的工具调用。
    pub tool_call_id: u64,
}

/// How a job ended.
/// 任务的结束方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ShellJobStatus {
    /// The process exited with code 0.
    /// 进程以退出码 0 退出。
    Completed,
    /// The process exited with another code, or could not be waited for.
    /// 进程以其他退出码退出，或无法等待。
    Failed,
    /// The timeout ended the process tree.
    /// 超时终止了进程树。
    TimedOut,
    /// A stop request or a cancelled foreground call ended the process tree.
    /// 停止请求或被取消的前台调用终止了进程树。
    Stopped,
    /// The service stopped before the job finished, so its outcome is unknown.
    /// 服务在任务结束前停止，因此结果未知。
    Lost,
}

impl ShellJobStatus {
    /// Stable label.
    /// 稳定标签。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::TimedOut => "timed_out",
            Self::Stopped => "stopped",
            Self::Lost => "lost",
        }
    }
}

/// Final outcome of one job.
/// 一个任务的最终结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShellJobFinal {
    /// How the job ended.
    /// 任务的结束方式。
    pub status: ShellJobStatus,
    /// Exit code when the process exited normally.
    /// 进程正常退出时的退出码。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    /// Run time in milliseconds.
    /// 运行时长（毫秒）。
    pub duration_ms: u64,
    /// End time in Unix milliseconds; 0 when unknown.
    /// 结束时间（Unix 毫秒）；未知时为 0。
    pub finished_at_ms: u64,
    /// Host-written explanation of a failure, timeout, stop or capture problem.
    /// 宿主写出的失败、超时、停止或捕获问题说明。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub failure_reason: Option<String>,
}

impl ShellJobFinal {
    /// Outcome of a job whose service stopped before it finished.
    /// 服务在任务结束前停止时的任务结果。
    fn lost() -> Self {
        Self {
            status: ShellJobStatus::Lost,
            exit_code: None,
            duration_ms: 0,
            finished_at_ms: 0,
            failure_reason: Some(String::from(
                "The service stopped before this job finished, so its outcome is unknown.",
            )),
        }
    }
}

/// Live state of one job.
/// 一个任务的实时状态。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShellJobState {
    /// The process is running.
    /// 进程正在运行。
    Running,
    /// The job finished.
    /// 任务已结束。
    Finished(ShellJobFinal),
}

/// Request to start one job.
/// 启动一个任务的请求。
#[derive(Debug, Clone)]
pub struct ShellJobSpawn {
    /// Session that owns the job.
    /// 拥有该任务的会话。
    pub owner_session_id: String,
    /// Tool call that starts the job, when one does.
    /// 启动该任务的工具调用（如有）。
    pub origin: Option<ShellJobOrigin>,
    /// Command as the model wrote it.
    /// 模型书写的命令。
    pub command: String,
    /// Description of what the command is for.
    /// 命令用途说明。
    pub purpose: String,
    /// Shell label.
    /// Shell 标签。
    pub shell_label: String,
    /// Working directory.
    /// 工作目录。
    pub cwd: PathBuf,
    /// The owning session's job directory.
    /// 所属会话的任务目录。
    pub jobs_dir: PathBuf,
    /// Text written once to stdin, which is then closed; none means empty stdin.
    /// 一次性写入 stdin 后关闭的文本；none 表示空 stdin。
    pub stdin: Option<String>,
    /// True when the model asked to run the job in the background from the start.
    /// 模型要求任务从一开始就在后台运行时为 true。
    pub background: bool,
    /// Timeout of the foreground wait, or of the whole job when it starts in the background.
    /// 前台等待的超时；任务以后台方式启动时为整个任务的超时。
    pub timeout: Option<Duration>,
    /// Keeps raw stdout and stderr separately in memory, bounded by `SHELL_JOB_STREAM_CAPTURE_BYTES`.
    /// 在内存中分别保留原始 stdout 与 stderr，受 `SHELL_JOB_STREAM_CAPTURE_BYTES` 限制。
    pub capture_streams: bool,
}

/// Raw stdout and stderr kept in memory for a foreground call.
/// 为前台调用保留在内存中的原始 stdout 与 stderr。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ShellJobCapturedStreams {
    /// Decoded stdout from its start.
    /// 从开头起解码的 stdout。
    pub stdout: String,
    /// Decoded stderr from its start.
    /// 从开头起解码的 stderr。
    pub stderr: String,
    /// True when either stream exceeded the capture size and was cut.
    /// 任一流超过保留大小被截断时为 true。
    pub truncated: bool,
}

impl ShellJobCapturedStreams {
    /// Appends text to one stream within the capture size.
    /// 在保留大小内向一个流追加文本。
    fn push(&mut self, stream: OutputStream, text: &str) {
        let target = match stream {
            OutputStream::Stdout => &mut self.stdout,
            OutputStream::Stderr => &mut self.stderr,
        };
        let room = SHELL_JOB_STREAM_CAPTURE_BYTES.saturating_sub(target.len());
        if text.len() <= room {
            target.push_str(text);
            return;
        }
        target.push_str(&text[..floor_char_boundary(text, room)]);
        self.truncated = true;
    }
}

/// Which output pipe a chunk came from.
/// 数据块来自哪个输出管道。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OutputStream {
    /// Standard output.
    /// 标准输出。
    Stdout,
    /// Standard error.
    /// 标准错误。
    Stderr,
}

/// Failure to start a job.
/// 启动任务失败。
#[derive(Debug)]
pub enum ShellJobSpawnError {
    /// The session already runs the most background jobs allowed.
    /// 会话已运行允许的最多后台任务。
    BackgroundLimit {
        /// The limit.
        /// 上限。
        limit: usize,
    },
    /// The job log could not be created.
    /// 无法创建任务日志。
    Log(io::Error),
    /// The requested timeout lies outside the host clock's supported range.
    /// 请求的超时超出宿主时钟支持范围。
    TimeoutOutOfRange,
    /// The process could not be started.
    /// 无法启动进程。
    Spawn(io::Error),
}

impl std::fmt::Display for ShellJobSpawnError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BackgroundLimit { limit } => write!(
                formatter,
                "This session already runs {limit} background jobs, the most allowed. Wait for one or stop one with shell_job before starting another in the background."
            ),
            Self::Log(error) => write!(formatter, "Failed to create the job output log: {error}"),
            Self::TimeoutOutOfRange => formatter.write_str(
                "The command timeout exceeds the host clock's supported range; use a shorter timeout or omit it.",
            ),
            Self::Spawn(error) => write!(formatter, "Failed to start the command: {error}"),
        }
    }
}

impl std::error::Error for ShellJobSpawnError {}

/// Result of waiting for a job.
/// 等待任务的结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShellJobWaitOutcome {
    /// The job finished.
    /// 任务已结束。
    Finished(ShellJobFinal),
    /// The wait budget ran out while the job keeps running.
    /// 等待预算耗尽，任务仍在运行。
    StillRunning,
    /// The user interjected a message while the job keeps running.
    /// 用户插队发送了消息，任务仍在运行。
    Interjected,
    /// A foreground job was moved to the background from elsewhere, such as the user interface.
    /// 前台任务被其他地方（如界面）转到了后台。
    Detached,
}

/// Kind of a job event.
/// 任务事件的种类。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ShellJobEventKind {
    /// A job started.
    /// 一个任务已启动。
    Started,
    /// A job moved to the background.
    /// 一个任务转到了后台。
    Backgrounded,
    /// A job finished.
    /// 一个任务已结束。
    Finished,
}

/// One job event for service subscribers.
/// 供服务订阅方使用的一个任务事件。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ShellJobEvent {
    /// Owning session.
    /// 所属会话。
    pub owner_session_id: String,
    /// Job identity.
    /// 任务身份。
    pub job_id: String,
    /// Event kind.
    /// 事件种类。
    pub kind: ShellJobEventKind,
}

/// A background job that finished and whose outcome the model has not been told yet.
/// 一个已结束、但尚未告知模型其结果的后台任务。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShellJobNotice {
    /// Job identity.
    /// 任务身份。
    pub job_id: String,
    /// Description of what the command was for.
    /// 命令用途说明。
    pub purpose: String,
    /// Command as the model wrote it.
    /// 模型书写的命令。
    pub command: String,
    /// Final outcome.
    /// 最终结果。
    pub outcome: ShellJobFinal,
}

/// One job as listed for the model and the user interface.
/// 为模型与界面列出的一个任务。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ShellJobSummary {
    /// Job identity.
    /// 任务身份。
    pub job_id: String,
    /// Command as the model wrote it.
    /// 模型书写的命令。
    pub command: String,
    /// Description of what the command is for.
    /// 命令用途说明。
    pub purpose: String,
    /// Shell label.
    /// Shell 标签。
    pub shell: String,
    /// Working directory.
    /// 工作目录。
    pub cwd: String,
    /// Start time in Unix milliseconds.
    /// 启动时间（Unix 毫秒）。
    pub started_at_ms: u64,
    /// Tool call that started the job, when one did.
    /// 启动该任务的工具调用（如有）。
    pub origin: Option<ShellJobOrigin>,
    /// True when the job ran in the background, from the start or after detaching.
    /// 任务曾在后台运行时为 true。
    pub background: bool,
    /// True while the process runs.
    /// 进程运行期间为 true。
    pub running: bool,
    /// Final outcome once finished or lost.
    /// 结束或丢失后的最终结果。
    pub outcome: Option<ShellJobFinal>,
    /// Log size facts, when the log is still available.
    /// 日志仍可用时的大小信息。
    pub log: Option<ShellJobLogStats>,
    /// Time of the latest output in Unix milliseconds, when known.
    /// 最近一次输出的时间（Unix 毫秒），已知时提供。
    pub last_output_at_ms: Option<u64>,
}

/// One job: process, rolling log and live state.
/// 一个任务：进程、滚动日志与实时状态。
pub struct ShellJob {
    /// Job identity.
    /// 任务身份。
    id: String,
    /// Owning session.
    /// 所属会话。
    owner_session_id: String,
    /// Tool call that started the job.
    /// 启动该任务的工具调用。
    origin: Option<ShellJobOrigin>,
    /// Command as the model wrote it.
    /// 模型书写的命令。
    command: String,
    /// Description of what the command is for.
    /// 命令用途说明。
    purpose: String,
    /// Shell label.
    /// Shell 标签。
    shell_label: String,
    /// Working directory.
    /// 工作目录。
    cwd: PathBuf,
    /// Owning session's job directory.
    /// 所属会话的任务目录。
    jobs_dir: PathBuf,
    /// Start time in Unix milliseconds.
    /// 启动时间（Unix 毫秒）。
    started_at_ms: u64,
    /// Monotonic start time.
    /// 单调启动时间。
    started_at: Instant,
    /// Timeout in milliseconds the job was started with.
    /// 任务启动时的超时（毫秒）。
    timeout_ms: Option<u64>,
    /// True once the job runs in the background.
    /// 任务在后台运行后为 true。
    background: watch::Sender<bool>,
    /// Raw streams kept for a foreground call that asked for them.
    /// 为请求它们的前台调用保留的原始流。
    captured: Option<Mutex<ShellJobCapturedStreams>>,
    /// Rolling log writer; none after a write failure stopped logging.
    /// 滚动日志写入器；写入失败停止记录后为 none。
    log: Mutex<Option<ShellJobLogWriter>>,
    /// First log write failure.
    /// 第一次日志写入失败。
    log_failure: Mutex<Option<String>>,
    /// Snapshot of the log taken when the job finished.
    /// 任务结束时的日志快照。
    final_log: Mutex<Option<ShellJobLogReader>>,
    /// Live state.
    /// 实时状态。
    state: watch::Sender<ShellJobState>,
    /// Counts output appends and the end of the job.
    /// 记录输出追加与任务结束的次数。
    output_changes: watch::Sender<u64>,
    /// Time of the latest output in Unix milliseconds; 0 before any output.
    /// 最近一次输出的时间（Unix 毫秒）；尚无输出时为 0。
    last_output_at_ms: AtomicU64,
    /// Stop request with its reason.
    /// 带原因的停止请求。
    stop: watch::Sender<Option<String>>,
    /// Deadline that ends the job with `timed_out`.
    /// 以 `timed_out` 结束任务的截止时间。
    deadline: watch::Sender<Option<tokio::time::Instant>>,
    /// Process id of the process tree root.
    /// 进程树根进程 ID。
    root_process_id: Option<u32>,
    /// Owning runtime.
    /// 所属运行时。
    runtime: Weak<RuntimeInner>,
}

impl std::fmt::Debug for ShellJob {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ShellJob")
            .field("id", &self.id)
            .field("owner_session_id", &self.owner_session_id)
            .finish_non_exhaustive()
    }
}

impl ShellJob {
    /// Job identity.
    /// 任务身份。
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Owning session.
    /// 所属会话。
    pub fn owner_session_id(&self) -> &str {
        &self.owner_session_id
    }

    /// Tool call that started the job.
    /// 启动该任务的工具调用。
    pub fn origin(&self) -> Option<ShellJobOrigin> {
        self.origin
    }

    /// Command as the model wrote it.
    /// 模型书写的命令。
    pub fn command(&self) -> &str {
        &self.command
    }

    /// Description of what the command is for.
    /// 命令用途说明。
    pub fn purpose(&self) -> &str {
        &self.purpose
    }

    /// Shell label.
    /// Shell 标签。
    pub fn shell_label(&self) -> &str {
        &self.shell_label
    }

    /// Working directory.
    /// 工作目录。
    pub fn cwd(&self) -> &Path {
        &self.cwd
    }

    /// Start time in Unix milliseconds.
    /// 启动时间（Unix 毫秒）。
    pub fn started_at_ms(&self) -> u64 {
        self.started_at_ms
    }

    /// Milliseconds since the job started.
    /// 任务启动以来的毫秒数。
    pub fn elapsed_ms(&self) -> u64 {
        u64::try_from(self.started_at.elapsed().as_millis()).unwrap_or(u64::MAX)
    }

    /// Process id of the process tree root.
    /// 进程树根进程 ID。
    pub fn root_process_id(&self) -> Option<u32> {
        self.root_process_id
    }

    /// True once the job runs in the background.
    /// 任务在后台运行后为 true。
    pub fn is_background(&self) -> bool {
        *self.background.borrow()
    }

    /// Raw streams kept so far, when the job was started with `capture_streams`.
    /// 任务以 `capture_streams` 启动时迄今保留的原始流。
    pub fn captured_streams(&self) -> Option<ShellJobCapturedStreams> {
        self.captured
            .as_ref()
            .map(|captured| lock(captured).clone())
    }

    /// Current state.
    /// 当前状态。
    pub fn state(&self) -> ShellJobState {
        self.state.borrow().clone()
    }

    /// Final outcome once finished.
    /// 结束后的最终结果。
    pub fn outcome(&self) -> Option<ShellJobFinal> {
        match &*self.state.borrow() {
            ShellJobState::Running => None,
            ShellJobState::Finished(outcome) => Some(outcome.clone()),
        }
    }

    /// Time of the latest output in Unix milliseconds.
    /// 最近一次输出的时间（Unix 毫秒）。
    pub fn last_output_at_ms(&self) -> Option<u64> {
        let value = self.last_output_at_ms.load(Ordering::Acquire);
        (value > 0).then_some(value)
    }

    /// Subscribes to output appends and the end of the job.
    /// 订阅输出追加与任务结束。
    pub fn subscribe_output(&self) -> watch::Receiver<u64> {
        self.output_changes.subscribe()
    }

    /// Reader over the output written so far.
    /// 基于迄今已写输出的读取器。
    ///
    /// # Returns
    /// 返回值
    /// Returns none when logging failed before any snapshot could be taken.
    /// 日志在取得任何快照前即已失败时返回 none。
    pub fn log_reader(&self) -> Option<ShellJobLogReader> {
        if let Some(reader) = lock(&self.final_log).clone() {
            return Some(reader);
        }
        lock(&self.log).as_ref().map(ShellJobLogWriter::reader)
    }

    /// Listing entry of the job.
    /// 该任务的列表条目。
    pub fn summary(&self) -> ShellJobSummary {
        let outcome = self.outcome();
        ShellJobSummary {
            job_id: self.id.clone(),
            command: self.command.clone(),
            purpose: self.purpose.clone(),
            shell: self.shell_label.clone(),
            cwd: self.cwd.display().to_string(),
            started_at_ms: self.started_at_ms,
            origin: self.origin,
            background: self.is_background(),
            running: outcome.is_none(),
            outcome,
            log: self.log_reader().map(|reader| reader.stats()),
            last_output_at_ms: self.last_output_at_ms(),
        }
    }

    /// Waits until the job finishes.
    /// 等待任务结束。
    pub async fn wait(&self) -> ShellJobFinal {
        let mut state = self.state.subscribe();
        loop {
            if let ShellJobState::Finished(outcome) = &*state.borrow_and_update() {
                return outcome.clone();
            }
            if state.changed().await.is_err() {
                return self.outcome().unwrap_or_else(ShellJobFinal::lost);
            }
        }
    }

    /// Waits until the job finishes, `budget` elapses, or the owning session receives an interjection.
    /// 等待任务结束、`budget` 到期，或所属会话收到插队消息。
    ///
    /// A wait that starts on a foreground job also ends with `Detached` once the job moves to the
    /// background from elsewhere.
    /// 对前台任务开始的等待，在任务被其他地方转到后台后也会以 `Detached` 结束。
    pub async fn wait_interruptible(
        &self,
        budget: Option<Duration>,
        interjection: &mut watch::Receiver<u64>,
    ) -> ShellJobWaitOutcome {
        let sleep = async {
            match budget {
                Some(budget) => tokio::time::sleep(budget).await,
                None => std::future::pending().await,
            }
        };
        let mut background = self.background.subscribe();
        let watch_detach = !*background.borrow_and_update();
        let detached = async {
            if !watch_detach {
                return std::future::pending::<()>().await;
            }
            while background.changed().await.is_ok() {
                if *background.borrow() {
                    return;
                }
            }
            std::future::pending::<()>().await
        };
        tokio::select! {
            outcome = self.wait() => ShellJobWaitOutcome::Finished(outcome),
            () = detached => match self.outcome() {
                Some(outcome) => ShellJobWaitOutcome::Finished(outcome),
                None => ShellJobWaitOutcome::Detached,
            },
            () = sleep => match self.outcome() {
                Some(outcome) => ShellJobWaitOutcome::Finished(outcome),
                None => ShellJobWaitOutcome::StillRunning,
            },
            changed = interjection.changed() => match (changed, self.outcome()) {
                (_, Some(outcome)) => ShellJobWaitOutcome::Finished(outcome),
                (Ok(()), None) => ShellJobWaitOutcome::Interjected,
                // A closed signal can never fire, so only the job's own end is left to wait for.
                // 已关闭的信号不会再触发，因此只剩等待任务自身结束。
                (Err(_), None) => ShellJobWaitOutcome::Finished(self.wait().await),
            },
        }
    }

    /// Requests termination of the process tree; the job ends as `stopped` with `reason`.
    /// 请求终止进程树；任务以 `stopped` 结束并带上 `reason`。
    pub fn request_stop(&self, reason: &str) {
        self.stop.send_if_modified(|current| {
            if current.is_some() {
                return false;
            }
            *current = Some(reason.to_string());
            true
        });
    }

    /// Moves a running foreground job to the background.
    /// 把一个运行中的前台任务转到后台。
    ///
    /// `keep_deadline` keeps the timeout running; otherwise the job runs until it exits or is stopped.
    /// `keep_deadline` 保留超时；否则任务一直运行到退出或被停止。
    ///
    /// # Returns
    /// 返回值
    /// Returns false when the job already finished or already runs in the background.
    /// 任务已结束或已在后台运行时返回 false。
    pub fn detach(&self, keep_deadline: bool) -> bool {
        if self.outcome().is_some()
            || !self
                .background
                .send_if_modified(|background| !std::mem::replace(background, true))
        {
            return false;
        }
        if !keep_deadline {
            self.deadline.send_replace(None);
        }
        let _ = append_index_line(
            &self.jobs_dir,
            &ShellJobIndexLine::Backgrounded {
                version: ShellJobIndexLine::current_version(),
                job_id: self.id.clone(),
                at_ms: unix_millis_now(),
            },
        );
        if let Some(runtime) = self.runtime.upgrade() {
            runtime.emit(self, ShellJobEventKind::Backgrounded);
        }
        true
    }

    /// Appends decoded output to the log and, when kept, to the raw stream.
    /// 向日志追加解码后的输出；保留原始流时同时追加到该流。
    fn append_output(&self, stream: OutputStream, text: &str) {
        if text.is_empty() {
            return;
        }
        if let Some(captured) = &self.captured {
            lock(captured).push(stream, text);
        }
        let mut log = lock(&self.log);
        if let Some(writer) = log.as_mut()
            && let Err(error) = writer.append(text)
        {
            *lock(&self.log_failure) = Some(format!(
                "Recording command output stopped after a log write failure: {error}"
            ));
            // The last snapshot stays readable while the pipes keep being drained.
            // 管道继续排空期间，最后的快照仍可读取。
            *lock(&self.final_log) = Some(writer.reader());
            *log = None;
        }
        drop(log);
        self.last_output_at_ms
            .store(unix_millis_now(), Ordering::Release);
        self.output_changes.send_modify(|count| *count += 1);
    }
}

/// Foreground attachment of a tool call to its job; dropping it unreleased stops the job.
/// 工具调用到其任务的前台附着；未释放即丢弃时会停止任务。
///
/// A cancelled tool call keeps the earlier semantics: its command's process tree is terminated. Release
/// the attachment once the job finished or moved to the background.
/// 被取消的工具调用保持原有语义：终止其命令的进程树。任务结束或转到后台后释放该附着。
#[derive(Debug)]
pub struct ShellJobForeground {
    /// Attached job.
    /// 附着的任务。
    job: Arc<ShellJob>,
    /// True once released.
    /// 释放后为 true。
    released: bool,
}

impl ShellJobForeground {
    /// Attaches to one job.
    /// 附着到一个任务。
    pub fn attach(job: Arc<ShellJob>) -> Self {
        Self {
            job,
            released: false,
        }
    }

    /// Attached job.
    /// 附着的任务。
    pub fn job(&self) -> &Arc<ShellJob> {
        &self.job
    }

    /// Releases the attachment without stopping the job.
    /// 释放附着而不停止任务。
    pub fn release(mut self) {
        self.released = true;
    }
}

impl Drop for ShellJobForeground {
    fn drop(&mut self) {
        if !self.released && !self.job.is_background() {
            self.job
                .request_stop("The tool call was cancelled, so the host stopped the command.");
        }
    }
}

/// Shared runtime state.
/// 共享运行时状态。
struct RuntimeInner {
    /// Running and recently finished jobs by identity.
    /// 按身份索引的运行中与最近结束的任务。
    jobs: Mutex<HashMap<String, Arc<ShellJob>>>,
    /// Undelivered completion notices per session.
    /// 每个会话尚未送达的完成通知。
    notices: Mutex<HashMap<String, Vec<ShellJobNotice>>>,
    /// Interjection signal per session.
    /// 每个会话的插队信号。
    interjections: Mutex<HashMap<String, watch::Sender<u64>>>,
    /// Job events.
    /// 任务事件。
    events: broadcast::Sender<ShellJobEvent>,
    /// Finds the job directory of a session; installed by the service at startup.
    /// 查找会话的任务目录；由服务在启动时安装。
    directory_resolver: std::sync::RwLock<Option<ShellJobDirectoryResolver>>,
}

/// Finds the job directory inside one session's directory; none when the session has no directory.
/// 在某会话目录中查找任务目录；会话没有目录时返回 none。
pub type ShellJobDirectoryResolver = Arc<dyn Fn(&str) -> Option<PathBuf> + Send + Sync>;

impl RuntimeInner {
    /// Publishes one event; having no subscriber is fine.
    /// 发布一个事件；没有订阅方也无妨。
    fn emit(&self, job: &ShellJob, kind: ShellJobEventKind) {
        let _ = self.events.send(ShellJobEvent {
            owner_session_id: job.owner_session_id.clone(),
            job_id: job.id.clone(),
            kind,
        });
    }
}

/// Registry and supervisor of all shell jobs of one service process.
/// 一个服务进程中全部 shell 任务的注册表与监管者。
#[derive(Clone)]
pub struct ShellJobRuntime {
    /// Shared state.
    /// 共享状态。
    inner: Arc<RuntimeInner>,
}

impl std::fmt::Debug for ShellJobRuntime {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ShellJobRuntime")
            .finish_non_exhaustive()
    }
}

impl Default for ShellJobRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl ShellJobRuntime {
    /// Creates an empty runtime.
    /// 创建一个空运行时。
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RuntimeInner {
                jobs: Mutex::new(HashMap::new()),
                notices: Mutex::new(HashMap::new()),
                interjections: Mutex::new(HashMap::new()),
                events: broadcast::channel(SHELL_JOB_EVENT_CAPACITY).0,
                directory_resolver: std::sync::RwLock::new(None),
            }),
        }
    }

    /// Installs the resolver that finds a session's job directory.
    /// 安装查找会话任务目录的解析器。
    pub fn set_directory_resolver(&self, resolver: ShellJobDirectoryResolver) {
        *self
            .inner
            .directory_resolver
            .write()
            .unwrap_or_else(PoisonError::into_inner) = Some(resolver);
    }

    /// Job directory of one session.
    /// 某会话的任务目录。
    ///
    /// Sessions without a directory, and processes without a resolver, use a per-session directory under
    /// the system temporary directory.
    /// 没有目录的会话以及未安装解析器的进程，使用系统临时目录下的按会话目录。
    pub fn jobs_directory(&self, owner_session_id: &str) -> PathBuf {
        let resolver = self
            .inner
            .directory_resolver
            .read()
            .unwrap_or_else(PoisonError::into_inner)
            .clone();
        if let Some(directory) = resolver.and_then(|resolver| resolver(owner_session_id)) {
            return directory;
        }
        let safe: String = owner_session_id
            .chars()
            .map(|character| {
                if character.is_ascii_alphanumeric() || character == '-' || character == '_' {
                    character
                } else {
                    '_'
                }
            })
            .collect();
        std::env::temp_dir()
            .join("vulcan-code-shell-jobs")
            .join(if safe.is_empty() {
                String::from("_")
            } else {
                safe
            })
    }

    /// The runtime shared by the whole service process.
    /// 整个服务进程共享的运行时。
    pub fn global() -> &'static ShellJobRuntime {
        static GLOBAL: OnceLock<ShellJobRuntime> = OnceLock::new();
        GLOBAL.get_or_init(ShellJobRuntime::new)
    }

    /// Starts one job from a prepared command; stdio and process-tree isolation are set here.
    /// 从已准备的命令启动一个任务；标准流与进程树隔离在此设置。
    ///
    /// Must be called inside a Tokio runtime, which supervises the process.
    /// 必须在 Tokio 运行时中调用，由其监管进程。
    pub fn spawn(
        &self,
        request: ShellJobSpawn,
        mut command: TokioCommand,
    ) -> Result<Arc<ShellJob>, ShellJobSpawnError> {
        if request.background
            && self.running_background_count(&request.owner_session_id)
                >= MAX_RUNNING_BACKGROUND_SHELL_JOBS
        {
            return Err(ShellJobSpawnError::BackgroundLimit {
                limit: MAX_RUNNING_BACKGROUND_SHELL_JOBS,
            });
        }
        // A rejected timeout must leave both the process and its job log untouched.
        // 超时参数被拒绝时，不能启动进程或创建任务日志。
        let deadline = request
            .timeout
            .map(|timeout| {
                tokio::time::Instant::now()
                    .checked_add(timeout)
                    .ok_or(ShellJobSpawnError::TimeoutOutOfRange)
            })
            .transpose()?;
        let job_id = new_job_id();
        let writer = ShellJobLogWriter::create(&request.jobs_dir.join(&job_id))
            .map_err(ShellJobSpawnError::Log)?;
        command.stdout(Stdio::piped());
        command.stderr(Stdio::piped());
        command.stdin(if request.stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        });
        command.kill_on_drop(true);
        isolate_child_process_tree(&mut command);
        let started_at = Instant::now();
        let child = command.spawn().map_err(ShellJobSpawnError::Spawn)?;
        let job = Arc::new(ShellJob {
            id: job_id,
            owner_session_id: request.owner_session_id,
            origin: request.origin,
            command: request.command,
            purpose: request.purpose,
            shell_label: request.shell_label,
            cwd: request.cwd,
            jobs_dir: request.jobs_dir,
            started_at_ms: unix_millis_now(),
            started_at,
            timeout_ms: request
                .timeout
                .map(|timeout| u64::try_from(timeout.as_millis()).unwrap_or(u64::MAX)),
            background: watch::channel(request.background).0,
            captured: request
                .capture_streams
                .then(|| Mutex::new(ShellJobCapturedStreams::default())),
            log: Mutex::new(Some(writer)),
            log_failure: Mutex::new(None),
            final_log: Mutex::new(None),
            state: watch::channel(ShellJobState::Running).0,
            output_changes: watch::channel(0).0,
            last_output_at_ms: AtomicU64::new(0),
            stop: watch::channel(None).0,
            deadline: watch::channel(deadline).0,
            root_process_id: child.id(),
            runtime: Arc::downgrade(&self.inner),
        });
        let _ = append_index_line(
            &job.jobs_dir,
            &ShellJobIndexLine::Started {
                version: ShellJobIndexLine::current_version(),
                job_id: job.id.clone(),
                command: job.command.clone(),
                purpose: job.purpose.clone(),
                shell: job.shell_label.clone(),
                cwd: job.cwd.display().to_string(),
                started_at_ms: job.started_at_ms,
                origin: job.origin,
                background: request.background,
            },
        );
        lock(&self.inner.jobs).insert(job.id.clone(), Arc::clone(&job));
        self.inner.emit(&job, ShellJobEventKind::Started);
        tokio::spawn(supervise(Arc::clone(&job), child, request.stdin));
        Ok(job)
    }

    /// Returns one job of `owner_session_id` kept in memory.
    /// 返回 `owner_session_id` 在内存中保留的一个任务。
    pub fn job(&self, owner_session_id: &str, job_id: &str) -> Option<Arc<ShellJob>> {
        lock(&self.inner.jobs)
            .get(job_id)
            .filter(|job| job.owner_session_id == owner_session_id)
            .cloned()
    }

    /// Returns the job started by one tool call, when it is kept in memory.
    /// 返回由某次工具调用启动且仍保留在内存中的任务。
    pub fn job_for_origin(
        &self,
        owner_session_id: &str,
        origin: ShellJobOrigin,
    ) -> Option<Arc<ShellJob>> {
        lock(&self.inner.jobs)
            .values()
            .find(|job| job.owner_session_id == owner_session_id && job.origin == Some(origin))
            .cloned()
    }

    /// Running jobs of one session, oldest first.
    /// 某会话的运行中任务，按从旧到新排列。
    pub fn running_jobs(&self, owner_session_id: &str) -> Vec<Arc<ShellJob>> {
        let mut jobs: Vec<Arc<ShellJob>> = lock(&self.inner.jobs)
            .values()
            .filter(|job| job.owner_session_id == owner_session_id && job.outcome().is_none())
            .cloned()
            .collect();
        jobs.sort_by_key(|job| job.started_at_ms);
        jobs
    }

    /// Number of running background jobs of one session.
    /// 某会话运行中后台任务的数量。
    pub fn running_background_count(&self, owner_session_id: &str) -> usize {
        self.running_jobs(owner_session_id)
            .iter()
            .filter(|job| job.is_background())
            .count()
    }

    /// Number of running background jobs across all sessions.
    /// 所有会话中运行中后台任务的数量。
    pub fn running_background_count_all(&self) -> usize {
        lock(&self.inner.jobs)
            .values()
            .filter(|job| job.is_background() && job.outcome().is_none())
            .count()
    }

    /// Lists the jobs of one session: running jobs first, then finished ones newest first.
    /// 列出某会话的任务：运行中任务在前，随后已结束任务按从新到旧排列。
    ///
    /// Jobs recorded on disk without an outcome and not running in this process are reported as lost.
    /// 磁盘上有记录但没有结果、且不在本进程中运行的任务报告为丢失。
    pub fn list_session_jobs(
        &self,
        owner_session_id: &str,
        jobs_dir: &Path,
    ) -> Result<Vec<ShellJobSummary>, ShellJobIndexError> {
        let records = read_session_job_records(jobs_dir)?;
        let live: HashMap<String, Arc<ShellJob>> = lock(&self.inner.jobs)
            .values()
            .filter(|job| job.owner_session_id == owner_session_id)
            .map(|job| (job.id.clone(), Arc::clone(job)))
            .collect();
        let mut summaries: Vec<ShellJobSummary> = records
            .into_iter()
            .map(|record| match live.get(&record.job_id) {
                Some(job) => job.summary(),
                None => {
                    let log = ShellJobLogReader::open(&jobs_dir.join(&record.job_id))
                        .ok()
                        .map(|reader| reader.stats());
                    ShellJobSummary {
                        job_id: record.job_id,
                        command: record.command,
                        purpose: record.purpose,
                        shell: record.shell,
                        cwd: record.cwd,
                        started_at_ms: record.started_at_ms,
                        origin: record.origin,
                        background: record.background,
                        running: false,
                        outcome: Some(record.outcome.unwrap_or_else(ShellJobFinal::lost)),
                        log,
                        last_output_at_ms: None,
                    }
                }
            })
            .collect();
        // Jobs whose start line could not be written are still listed from memory.
        // 启动行未能写入的任务仍从内存中列出。
        for job in live.values() {
            if !summaries.iter().any(|summary| summary.job_id == job.id) {
                summaries.push(job.summary());
            }
        }
        summaries.sort_by(|left, right| {
            right
                .running
                .cmp(&left.running)
                .then_with(|| right.started_at_ms.cmp(&left.started_at_ms))
        });
        Ok(summaries)
    }

    /// Opens the log of one job of a session, live or finished.
    /// 打开某会话一个任务（运行中或已结束）的日志。
    ///
    /// # Returns
    /// 返回值
    /// Returns `Ok(None)` when the session has no such job.
    /// 会话没有该任务时返回 `Ok(None)`。
    pub fn open_log(
        &self,
        owner_session_id: &str,
        jobs_dir: &Path,
        job_id: &str,
    ) -> io::Result<Option<ShellJobLogReader>> {
        if let Some(job) = self.job(owner_session_id, job_id) {
            return job.log_reader().map(Some).ok_or_else(|| {
                io::Error::other("The output log of this job is no longer available.")
            });
        }
        if !valid_job_id(job_id) {
            return Ok(None);
        }
        let known = read_session_job_records(jobs_dir)
            .map_err(io::Error::other)?
            .iter()
            .any(|record| record.job_id == job_id);
        if !known {
            return Ok(None);
        }
        match ShellJobLogReader::open(&jobs_dir.join(job_id)) {
            Ok(reader) => Ok(Some(reader)),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Err(io::Error::new(
                io::ErrorKind::NotFound,
                "The output log of this job was deleted to stay within the session's log space limit.",
            )),
            Err(error) => Err(error),
        }
    }

    /// Takes the undelivered completion notices of one session.
    /// 取出某会话尚未送达的完成通知。
    pub fn take_notices(&self, owner_session_id: &str) -> Vec<ShellJobNotice> {
        lock(&self.inner.notices)
            .remove(owner_session_id)
            .unwrap_or_default()
    }

    /// Puts notices that could not be delivered back in front of the session's pending notices.
    /// 把未能送达的通知放回该会话待送达通知的最前面。
    pub fn requeue_notices(&self, owner_session_id: &str, notices: Vec<ShellJobNotice>) {
        if notices.is_empty() {
            return;
        }
        let mut pending = lock(&self.inner.notices);
        let entry = pending.entry(owner_session_id.to_string()).or_default();
        let later = std::mem::replace(entry, notices);
        entry.extend(later);
    }

    /// Drops the undelivered notice of one job, e.g. after the model waited for it and saw the outcome.
    /// 丢弃某任务尚未送达的通知，例如模型已等待并看到其结果之后。
    pub fn discard_notice(&self, owner_session_id: &str, job_id: &str) {
        let mut notices = lock(&self.inner.notices);
        if let Some(pending) = notices.get_mut(owner_session_id) {
            pending.retain(|notice| notice.job_id != job_id);
            if pending.is_empty() {
                notices.remove(owner_session_id);
            }
        }
    }

    /// Number of undelivered completion notices of one session.
    /// 某会话尚未送达的完成通知数量。
    pub fn pending_notice_count(&self, owner_session_id: &str) -> usize {
        lock(&self.inner.notices)
            .get(owner_session_id)
            .map_or(0, Vec::len)
    }

    /// Signals that the user interjected a message into one session's running turn.
    /// 发出用户向某会话运行中回合插队发送消息的信号。
    pub fn notify_interjection(&self, owner_session_id: &str) {
        let mut interjections = lock(&self.inner.interjections);
        let sender = interjections
            .entry(owner_session_id.to_string())
            .or_insert_with(|| watch::channel(0).0);
        sender.send_modify(|count| *count += 1);
    }

    /// Subscribes to later interjections into one session; earlier ones are already seen.
    /// 订阅某会话之后的插队；之前的插队视为已看到。
    pub fn subscribe_interjection(&self, owner_session_id: &str) -> watch::Receiver<u64> {
        let mut interjections = lock(&self.inner.interjections);
        let sender = interjections
            .entry(owner_session_id.to_string())
            .or_insert_with(|| watch::channel(0).0);
        let mut receiver = sender.subscribe();
        receiver.borrow_and_update();
        receiver
    }

    /// Subscribes to job events.
    /// 订阅任务事件。
    pub fn subscribe_events(&self) -> broadcast::Receiver<ShellJobEvent> {
        self.inner.events.subscribe()
    }

    /// Stops every running job of one session, e.g. when it is archived or a subagent ends.
    /// 停止某会话全部运行中的任务，例如会话归档或子代理结束时。
    pub fn stop_session_jobs(&self, owner_session_id: &str, reason: &str) -> usize {
        let jobs = self.running_jobs(owner_session_id);
        for job in &jobs {
            job.request_stop(reason);
        }
        jobs.len()
    }
}

/// Supervises one process until it exits or is ended, then records the outcome.
/// 监管一个进程直至其退出或被终止，然后记录结果。
async fn supervise(job: Arc<ShellJob>, mut child: Child, stdin_text: Option<String>) {
    let mut tree_guard = ChildProcessTreeDropGuard::arm(&child);
    let stdin_task: Option<JoinHandle<io::Result<()>>> = match (stdin_text, child.stdin.take()) {
        (Some(text), Some(mut stdin)) => Some(tokio::spawn(async move {
            let result = stdin.write_all(text.as_bytes()).await;
            // Dropping the handle closes stdin so the child observes end-of-input.
            // 释放句柄会关闭 stdin，使子进程读到输入结束。
            drop(stdin);
            result
        })),
        _ => None,
    };
    let stdout_task = child
        .stdout
        .take()
        .map(|reader| spawn_output_reader(Arc::clone(&job), OutputStream::Stdout, reader));
    let stderr_task = child
        .stderr
        .take()
        .map(|reader| spawn_output_reader(Arc::clone(&job), OutputStream::Stderr, reader));

    let mut stop = job.stop.subscribe();
    let mut deadline = job.deadline.subscribe();
    enum Ending {
        Exited(io::Result<std::process::ExitStatus>),
        Stopped(String),
        TimedOut,
    }
    let ending = loop {
        let current_deadline = *deadline.borrow_and_update();
        if let Some(reason) = stop.borrow_and_update().clone() {
            break Ending::Stopped(reason);
        }
        tokio::select! {
            result = child.wait() => break Ending::Exited(result),
            changed = stop.changed() => {
                if changed.is_err() {
                    continue;
                }
            }
            changed = deadline.changed() => {
                if changed.is_err() {
                    continue;
                }
            }
            () = sleep_until(current_deadline) => break Ending::TimedOut,
        }
    };

    let mut exit = None;
    let mut reason = None;
    let status = match ending {
        Ending::Exited(Ok(exit_status)) => {
            // Background descendants left after a natural exit are intentionally not killed.
            // 自然退出后残留的后台后代进程按设计不被终止。
            tree_guard.disarm();
            exit = exit_status.code();
            if exit_status.success() {
                ShellJobStatus::Completed
            } else {
                ShellJobStatus::Failed
            }
        }
        Ending::Exited(Err(error)) => {
            reason = Some(format!("Failed while waiting for the command: {error}"));
            ShellJobStatus::Failed
        }
        Ending::Stopped(stop_reason) => {
            reason = Some(stop_reason);
            terminate(&mut child, &mut tree_guard, &mut reason).await;
            ShellJobStatus::Stopped
        }
        Ending::TimedOut => {
            reason = Some(format!(
                "The command exceeded the timeout limit ({} ms).",
                job.timeout_ms.unwrap_or_default()
            ));
            terminate(&mut child, &mut tree_guard, &mut reason).await;
            ShellJobStatus::TimedOut
        }
    };

    let drain_deadline = tokio::time::Instant::now() + CHILD_OUTPUT_DRAIN_GRACE;
    let stdout_result = join_until(stdout_task, drain_deadline).await;
    let stderr_result = join_until(stderr_task, drain_deadline).await;
    let stdin_result = join_until(stdin_task, drain_deadline).await;
    let io_failure = match (&stdout_result, &stderr_result, &stdin_result) {
        (Some(Err(error)), _, _) => Some(format!("Failed while reading command stdout: {error}")),
        (_, Some(Err(error)), _) => Some(format!("Failed while reading command stderr: {error}")),
        // A child may legitimately exit or close stdin before consuming all provided input.
        // 子进程可以在消费完全部输入前合法地退出或关闭 stdin。
        (_, _, Some(Err(error))) if error.kind() != io::ErrorKind::BrokenPipe => {
            Some(format!("Failed to write command stdin: {error}"))
        }
        _ => None,
    };
    let status = if status == ShellJobStatus::Completed && io_failure.is_some() {
        ShellJobStatus::Failed
    } else {
        status
    };
    let mut notes: Vec<String> = reason.into_iter().chain(io_failure).collect();
    if status != ShellJobStatus::Stopped
        && status != ShellJobStatus::TimedOut
        && (stdout_result.is_none() || stderr_result.is_none())
    {
        notes.push(format!(
            "Output capture stopped {} ms after the command exited because background processes still hold stdout or stderr.",
            CHILD_OUTPUT_DRAIN_GRACE.as_millis()
        ));
    }
    if let Some(failure) = lock(&job.log_failure).clone() {
        notes.push(failure);
    }
    let outcome = ShellJobFinal {
        status,
        exit_code: exit,
        duration_ms: u64::try_from(job.started_at.elapsed().as_millis()).unwrap_or(u64::MAX),
        finished_at_ms: unix_millis_now(),
        failure_reason: (!notes.is_empty()).then(|| notes.join(" ")),
    };
    finish(&job, outcome);
}

/// Terminates the process tree after a stop or timeout and waits briefly for the root to exit.
/// 停止或超时后终止进程树，并短暂等待根进程退出。
async fn terminate(
    child: &mut Child,
    tree_guard: &mut ChildProcessTreeDropGuard,
    reason: &mut Option<String>,
) {
    if let Err(error) = terminate_child_process_tree(child).await {
        let note = reason.get_or_insert_with(String::new);
        if !note.is_empty() {
            note.push(' ');
        }
        note.push_str(&error);
    }
    if let Ok(Ok(_)) = tokio::time::timeout(CHILD_OUTPUT_DRAIN_GRACE, child.wait()).await {
        tree_guard.disarm();
    }
}

/// Records the outcome: snapshot, index line, state, notice, event and log space.
/// 记录结果：快照、索引行、状态、通知、事件与日志空间。
fn finish(job: &Arc<ShellJob>, outcome: ShellJobFinal) {
    {
        let mut log = lock(&job.log);
        if let Some(writer) = log.take() {
            *lock(&job.final_log) = Some(writer.reader());
        }
    }
    let _ = append_index_line(
        &job.jobs_dir,
        &ShellJobIndexLine::Finished {
            version: ShellJobIndexLine::current_version(),
            job_id: job.id.clone(),
            outcome: outcome.clone(),
        },
    );
    job.state
        .send_replace(ShellJobState::Finished(outcome.clone()));
    job.output_changes.send_modify(|count| *count += 1);
    let Some(runtime) = job.runtime.upgrade() else {
        return;
    };
    if job.is_background() {
        lock(&runtime.notices)
            .entry(job.owner_session_id.clone())
            .or_default()
            .push(ShellJobNotice {
                job_id: job.id.clone(),
                purpose: job.purpose.clone(),
                command: job.command.clone(),
                outcome,
            });
    }
    runtime.emit(job, ShellJobEventKind::Finished);
    prune_finished_jobs(&runtime, &job.owner_session_id);
    enforce_session_log_budget(&runtime, &job.owner_session_id, &job.jobs_dir);
}

/// Keeps only the newest finished jobs of one session in memory.
/// 只在内存中保留某会话最新的已结束任务。
fn prune_finished_jobs(runtime: &RuntimeInner, owner_session_id: &str) {
    let mut jobs = lock(&runtime.jobs);
    let mut finished: Vec<(u64, String)> = jobs
        .values()
        .filter(|job| job.owner_session_id == owner_session_id && job.outcome().is_some())
        .map(|job| (job.started_at_ms, job.id.clone()))
        .collect();
    if finished.len() <= RETAINED_FINISHED_JOBS_PER_SESSION {
        return;
    }
    finished.sort();
    let excess = finished.len() - RETAINED_FINISHED_JOBS_PER_SESSION;
    for (_, job_id) in finished.into_iter().take(excess) {
        jobs.remove(&job_id);
    }
}

/// Deletes the logs of the oldest finished jobs while one session's logs exceed the space budget.
/// 当某会话日志超出空间预算时，删除最早已结束任务的日志。
fn enforce_session_log_budget(runtime: &RuntimeInner, owner_session_id: &str, jobs_dir: &Path) {
    let Ok(records) = read_session_job_records(jobs_dir) else {
        return;
    };
    let running: Vec<String> = lock(&runtime.jobs)
        .values()
        .filter(|job| job.owner_session_id == owner_session_id && job.outcome().is_none())
        .map(|job| job.id.clone())
        .collect();
    let sizes: Vec<(String, u64)> = records
        .iter()
        .map(|record| {
            (
                record.job_id.clone(),
                directory_size(&jobs_dir.join(&record.job_id)),
            )
        })
        .collect();
    let mut total: u64 = sizes.iter().map(|(_, size)| size).sum();
    for (job_id, size) in sizes {
        if total <= SHELL_JOB_SESSION_LOG_BUDGET_BYTES {
            return;
        }
        if size == 0 || running.contains(&job_id) {
            continue;
        }
        if std::fs::remove_dir_all(jobs_dir.join(&job_id)).is_ok() {
            total -= size;
        }
    }
}

/// Total size of the files directly inside one directory; 0 when it is missing.
/// 某目录下直接包含文件的总大小；目录不存在时为 0。
fn directory_size(dir: &Path) -> u64 {
    std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .filter_map(|entry| entry.metadata().ok())
                .filter(std::fs::Metadata::is_file)
                .map(|metadata| metadata.len())
                .sum()
        })
        .unwrap_or(0)
}

/// Reads one output pipe, decoding it incrementally into the job log.
/// 读取一个输出管道，并增量解码写入任务日志。
fn spawn_output_reader<R>(
    job: Arc<ShellJob>,
    stream: OutputStream,
    mut reader: R,
) -> JoinHandle<io::Result<()>>
where
    R: tokio::io::AsyncRead + Unpin + Send + 'static,
{
    tokio::spawn(async move {
        let mut decoder = ShellOutputStreamDecoder::new();
        let mut chunk = vec![0_u8; OUTPUT_READ_CHUNK_BYTES];
        loop {
            let read = match reader.read(&mut chunk).await {
                Ok(read) => read,
                Err(error) => {
                    job.append_output(stream, &decoder.finish());
                    return Err(error);
                }
            };
            if read == 0 {
                job.append_output(stream, &decoder.finish());
                return Ok(());
            }
            let text = decoder.push_bytes(&chunk[..read]);
            job.append_output(stream, &text);
        }
    })
}

/// Waits for one optional task until `deadline`, aborting it if still running.
/// 在 `deadline` 前等待一个可选任务，到期仍在运行则中止它。
///
/// # Returns
/// 返回值
/// Returns `None` when the task had to be aborted.
/// 任务不得不被中止时返回 `None`。
async fn join_until(
    task: Option<JoinHandle<io::Result<()>>>,
    deadline: tokio::time::Instant,
) -> Option<io::Result<()>> {
    let Some(task) = task else {
        return Some(Ok(()));
    };
    let abort = task.abort_handle();
    match tokio::time::timeout_at(deadline, task).await {
        Ok(joined) => Some(joined.unwrap_or_else(|error| Err(io::Error::other(error)))),
        Err(_) => {
            abort.abort();
            None
        }
    }
}

/// Sleeps until an optional deadline; none never elapses.
/// 睡眠至可选截止时间；none 永不到期。
async fn sleep_until(deadline: Option<tokio::time::Instant>) {
    match deadline {
        Some(deadline) => tokio::time::sleep_until(deadline).await,
        None => std::future::pending().await,
    }
}

/// New job identity: `job_` plus twelve lowercase hex digits.
/// 新的任务身份：`job_` 加 12 位小写十六进制数字。
fn new_job_id() -> String {
    let simple = uuid::Uuid::new_v4().simple().to_string();
    format!("job_{}", &simple[..12])
}

/// True for identities this runtime could have issued; guards paths built from model input.
/// 对本运行时可能签发的身份返回 true；保护由模型输入构建的路径。
pub(crate) fn valid_job_id(job_id: &str) -> bool {
    job_id
        .strip_prefix("job_")
        .is_some_and(|rest| rest.len() == 12 && rest.bytes().all(|byte| byte.is_ascii_hexdigit()))
}

/// Current time in Unix milliseconds.
/// 当前时间（Unix 毫秒）。
fn unix_millis_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}

/// Locks a mutex, recovering from poisoning.
/// 锁定互斥量，并从中毒状态恢复。
fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}
