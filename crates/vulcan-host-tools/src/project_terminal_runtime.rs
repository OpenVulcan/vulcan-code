//! Project-owned persistent terminal runtime backed by one isolated host per terminal.
//! 由每终端一个隔离宿主支撑的项目级持久终端运行时。

use crate::output_decode::ShellOutputStreamDecoder;
use crate::terminal_host_client::{
    TerminalHostConnection, spawn_terminal_host, wait_for_terminal_host_stop_deadline,
};
use crate::terminal_host_protocol::{
    TERMINAL_HOST_MAX_COLS, TERMINAL_HOST_MAX_ROWS, TerminalHostErrorEvent,
    TerminalHostExitedEvent, TerminalHostFrameKind, TerminalHostStartRequest,
    decode_terminal_host_json, validate_terminal_host_sequence,
};
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, RwLock};
use tokio::io::AsyncReadExt;
use tokio::sync::mpsc::error::TrySendError;
use tokio::sync::mpsc::{Receiver, Sender, channel};
use tokio::sync::{Mutex, Notify, oneshot};
use tokio::time::{Duration, timeout};
use uuid::Uuid;

/// Bounds retained output while keeping enough history for an xterm scrollback bootstrap.
/// 限制保留输出，同时为 xterm 回滚缓冲首次加载保留足够历史。
const PROJECT_TERMINAL_RETAINED_CHUNK_LIMIT: usize = 2048;

/// Bounds retained decoded output bytes independently from fragment count.
/// 独立于分片数量限制保留的已解码输出字节数。
const PROJECT_TERMINAL_RETAINED_BYTE_LIMIT: usize = 8 * 1024 * 1024;

/// Bounds one control acknowledgement so a stuck host cannot block the service request forever.
/// 限制单次控制确认时长，避免卡死的宿主无限阻塞服务请求。
const PROJECT_TERMINAL_CONTROL_TIMEOUT: Duration = Duration::from_secs(2);

/// Bounds queued controls for one terminal so a stalled host cannot grow service memory without limit.
/// 限制单个终端的排队控制数量，避免卡死宿主导致服务内存无界增长。
const PROJECT_TERMINAL_CONTROL_QUEUE_LIMIT: usize = 128;

/// Longest project terminal name, in Unicode scalar values, for creation and renaming alike.
/// 项目终端名称的最大长度（Unicode 标量值），创建与重命名共用。
const PROJECT_TERMINAL_NAME_MAX_CHARS: usize = 128;

/// Defines the grace period before an unresponsive stopped project terminal host is forcibly reaped.
/// 定义停止请求后无响应项目终端宿主被强制回收前的宽限时间。
const PROJECT_TERMINAL_HOST_STOP_GRACE: Duration = Duration::from_secs(2);

/// Defines stable lifecycle states for one project terminal.
/// 定义单个项目终端的稳定生命周期状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectTerminalStatus {
    /// Terminal child is running and may accept input.
    /// 终端子进程正在运行并可接受输入。
    Running,
    /// Terminal child exited successfully without a stop request.
    /// 终端子进程未收到停止请求并成功退出。
    Completed,
    /// Terminal host or child failed.
    /// 终端宿主或子进程发生故障。
    Failed,
    /// Terminal child was stopped explicitly.
    /// 终端子进程被显式停止。
    Stopped,
}

impl ProjectTerminalStatus {
    /// Returns the stable lowercase service label.
    /// 返回稳定的小写服务标签。
    ///
    /// # Returns / 返回值
    /// Returns one protocol-safe lifecycle label.
    /// 返回一个协议安全的生命周期标签。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Stopped => "stopped",
        }
    }
}

/// Carries one ordered decoded output fragment for a project terminal display.
/// 为项目终端显示层承载一个有序的已解码输出片段。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectTerminalOutputChunk {
    /// Monotonic output sequence assigned by the runtime.
    /// 由运行时分配的单调递增输出序号。
    pub seq: u64,
    /// Stable output stream label.
    /// 稳定的输出流标签。
    pub stream: &'static str,
    /// Decoded terminal or host diagnostic text.
    /// 已解码的终端或宿主诊断文本。
    pub text: String,
}

/// Describes one immutable project terminal at a runtime boundary.
/// 描述运行时边界上的一个不可变项目终端。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectTerminalSnapshot {
    /// Stable terminal identity.
    /// 稳定终端标识。
    pub terminal_id: Uuid,
    /// Stable owning project identity.
    /// 稳定归属项目标识。
    pub project_id: String,
    /// User-facing terminal name.
    /// 用户可见终端名称。
    pub name: String,
    /// Selected shell family id.
    /// 已选择的 Shell 族标识。
    pub shell_id: String,
    /// Exact project working directory.
    /// 精确项目工作目录。
    pub cwd: PathBuf,
    /// Current terminal lifecycle state.
    /// 当前终端生命周期状态。
    pub status: ProjectTerminalStatus,
    /// Optional terminal child exit code.
    /// 可选终端子进程退出码。
    pub exit_code: Option<i32>,
    /// Optional terminal-host or child failure reason.
    /// 可选终端宿主或子进程故障原因。
    pub failure_reason: Option<String>,
    /// Highest output sequence retained or emitted so far.
    /// 当前已保留或发出的最高输出序号。
    pub last_seq: u64,
    /// Highest evicted output sequence, or zero before any eviction.
    /// 已淘汰的最高输出序号；尚未淘汰时为零。
    pub dropped_before_seq: u64,
    /// Monotonic revision for user-visible metadata changes.
    /// 用户可见元数据变化的单调修订号。
    pub state_revision: u64,
    /// Highest contiguous browser input sequence accepted by the isolated host pipe.
    /// 隔离宿主管道已接受的最高连续浏览器输入序号。
    pub last_accepted_input_seq: u64,
    /// Current terminal row count.
    /// 当前终端行数。
    pub rows: u16,
    /// Current terminal column count.
    /// 当前终端列数。
    pub cols: u16,
}

/// Returns a project-terminal snapshot plus ordered display output.
/// 返回项目终端快照以及有序显示输出。
#[derive(Debug, Clone)]
pub struct ProjectTerminalReadOutcome {
    /// Latest immutable terminal state.
    /// 最新不可变终端状态。
    pub snapshot: ProjectTerminalSnapshot,
    /// Ordered retained or newly changed output fragments.
    /// 有序的保留或新增输出片段。
    pub chunks: Vec<ProjectTerminalOutputChunk>,
    /// Indicates whether more matching fragments remain after this result.
    /// 指示本次结果之后是否仍有更多匹配分片。
    pub has_more: bool,
}

/// Defines all validated data needed to create one project terminal.
/// 定义创建一个项目终端所需的全部已验证数据。
#[derive(Debug, Clone)]
pub struct ProjectTerminalStartRequest {
    /// Stable owning project identity.
    /// 稳定归属项目标识。
    pub project_id: String,
    /// User-facing terminal name.
    /// 用户可见终端名称。
    pub name: String,
    /// Selected shell family id.
    /// 已选择的 Shell 族标识。
    pub shell_id: String,
    /// Concrete shell executable.
    /// 具体 Shell 可执行文件。
    pub program: String,
    /// Interactive shell arguments.
    /// 交互式 Shell 参数。
    pub args: Vec<String>,
    /// Exact project working directory.
    /// 精确项目工作目录。
    pub cwd: PathBuf,
    /// Initial terminal row count.
    /// 初始终端行数。
    pub rows: u16,
    /// Initial terminal column count.
    /// 初始终端列数。
    pub cols: u16,
}

/// Stores mutable lifecycle and bounded output for one project terminal.
/// 保存一个项目终端的可变生命周期及有界输出。
struct ProjectTerminalMutableState {
    /// User-facing terminal name.
    /// 用户可见终端名称。
    name: String,
    /// Current lifecycle state.
    /// 当前生命周期状态。
    status: ProjectTerminalStatus,
    /// Optional terminal child exit code.
    /// 可选终端子进程退出码。
    exit_code: Option<i32>,
    /// Optional failure diagnostic.
    /// 可选故障诊断。
    failure_reason: Option<String>,
    /// Highest assigned output sequence.
    /// 已分配的最高输出序号。
    last_seq: u64,
    /// Highest output sequence evicted from the retained display checkpoint.
    /// 已从保留显示检查点淘汰的最高输出序号。
    dropped_before_seq: u64,
    /// Monotonic revision for user-visible metadata changes.
    /// 用户可见元数据变化的单调修订号。
    state_revision: u64,
    /// Highest contiguous browser input sequence accepted by the isolated host pipe.
    /// 隔离宿主管道已接受的最高连续浏览器输入序号。
    last_accepted_input_seq: u64,
    /// Bounded retained output queue.
    /// 有界保留输出队列。
    chunks: VecDeque<ProjectTerminalOutputChunk>,
    /// Total UTF-8 byte count retained by `chunks`.
    /// `chunks` 当前保留的 UTF-8 总字节数。
    retained_bytes: usize,
    /// Bounded headless terminal state used to rebuild the current screen after retained history is evicted.
    /// 在保留历史被淘汰后用于重建当前画面的有界无界面终端状态。
    display_parser: vt100::Parser,
    /// Current terminal row count.
    /// 当前终端行数。
    rows: u16,
    /// Current terminal column count.
    /// 当前终端列数。
    cols: u16,
}

/// Owns metadata, mutable state, notifications, and controls for one terminal.
/// 持有一个终端的元数据、可变状态、通知及控制通道。
struct ProjectTerminalRecord {
    /// Stable terminal identity.
    /// 稳定终端标识。
    terminal_id: Uuid,
    /// Process-local monotonic creation order used for stable project tab ordering.
    /// 用于稳定项目标签顺序的进程内单调创建序号。
    creation_order: u64,
    /// Stable owning project identity.
    /// 稳定归属项目标识。
    project_id: String,
    /// Selected shell family id.
    /// 已选择的 Shell 族标识。
    shell_id: String,
    /// Exact project working directory.
    /// 精确项目工作目录。
    cwd: PathBuf,
    /// Serialized supervisor control sender.
    /// 串行化监控器控制发送端。
    control_sender: Sender<ProjectTerminalControl>,
    /// Mutable lifecycle and output state.
    /// 可变生命周期与输出状态。
    state: Mutex<ProjectTerminalMutableState>,
    /// Wakes attached displays after output or lifecycle changes.
    /// 在输出或生命周期变化后唤醒已挂接显示层。
    changed: Notify,
    /// Wakes metadata observers only after the terminal lifecycle changes.
    /// 仅在终端生命周期变化后唤醒元数据观察者。
    lifecycle_changed: Notify,
}

/// Defines serialized controls accepted by one terminal supervisor.
/// 定义一个终端监控器接受的串行控制操作。
enum ProjectTerminalControl {
    /// Writes exact terminal input bytes.
    /// 写入精确终端输入字节。
    WriteBatch {
        /// Exact byte payload produced by xterm.
        /// 由 xterm 产生的精确字节负载。
        bytes: Vec<u8>,
        /// First contiguous browser input sequence carried by this batch.
        /// 当前批次承载的首个连续浏览器输入序号。
        first_client_seq: u64,
        /// Last contiguous browser input sequence carried by this batch.
        /// 当前批次承载的最后一个连续浏览器输入序号。
        last_client_seq: u64,
        /// Returns the highest accepted browser input sequence or failure.
        /// 返回已接受的最高浏览器输入序号或故障。
        responder: oneshot::Sender<Result<u64, String>>,
    },
    /// Changes the terminal character grid.
    /// 更改终端字符网格。
    Resize {
        /// Positive row count.
        /// 大于零的行数。
        rows: u16,
        /// Positive column count.
        /// 大于零的列数。
        cols: u16,
        /// Returns resize acceptance or failure.
        /// 返回尺寸调整接受结果或故障。
        responder: oneshot::Sender<Result<(), String>>,
    },
    /// Stops the terminal child.
    /// 停止终端子进程。
    Stop {
        /// Returns stop acceptance or failure.
        /// 返回停止接受结果或故障。
        responder: oneshot::Sender<Result<(), String>>,
    },
}

impl ProjectTerminalRecord {
    /// Returns whether the terminal is currently running without blocking.
    /// 非阻塞地返回终端当前是否运行中。
    ///
    /// # Returns
    /// 返回值
    /// `false` when the mutable state is already locked by the supervisor.
    /// 当可变状态已被监控器锁定时返回 `false`。
    fn is_running_now(&self) -> bool {
        self.state
            .try_lock()
            .is_ok_and(|state| state.status == ProjectTerminalStatus::Running)
    }

    /// Builds one immutable snapshot from locked state.
    /// 从已锁定状态构建一个不可变快照。
    ///
    /// # Parameters / 参数
    /// - `state`: Locked mutable state belonging to this record.
    /// - `state`：属于本记录的已锁定可变状态。
    ///
    /// # Returns / 返回值
    /// Returns a complete immutable terminal snapshot.
    /// 返回完整不可变终端快照。
    fn snapshot_from_state(&self, state: &ProjectTerminalMutableState) -> ProjectTerminalSnapshot {
        ProjectTerminalSnapshot {
            terminal_id: self.terminal_id,
            project_id: self.project_id.clone(),
            name: state.name.clone(),
            shell_id: self.shell_id.clone(),
            cwd: self.cwd.clone(),
            status: state.status,
            exit_code: state.exit_code,
            failure_reason: state.failure_reason.clone(),
            last_seq: state.last_seq,
            dropped_before_seq: state.dropped_before_seq,
            state_revision: state.state_revision,
            last_accepted_input_seq: state.last_accepted_input_seq,
            rows: state.rows,
            cols: state.cols,
        }
    }

    /// Returns the current immutable terminal snapshot.
    /// 返回当前不可变终端快照。
    ///
    /// # Returns / 返回值
    /// Returns a snapshot captured under the state lock.
    /// 返回在状态锁内捕获的快照。
    async fn snapshot(&self) -> ProjectTerminalSnapshot {
        let state = self.state.lock().await;
        self.snapshot_from_state(&state)
    }

    /// Appends one bounded output fragment and wakes visible displays.
    /// 追加一个有界输出片段并唤醒可见显示层。
    ///
    /// # Parameters / 参数
    /// - `stream`: Stable stdout or stderr label.
    /// - `text`: Decoded output text.
    /// - `stream`：稳定的 stdout 或 stderr 标签。
    /// - `text`：已解码输出文本。
    async fn append_output(&self, stream: &'static str, text: String) {
        if text.is_empty() {
            return;
        }
        let mut state = self.state.lock().await;
        state.display_parser.process(text.as_bytes());
        state.last_seq = state.last_seq.saturating_add(1);
        let sequence = state.last_seq;
        state.retained_bytes = state.retained_bytes.saturating_add(text.len());
        state.chunks.push_back(ProjectTerminalOutputChunk {
            seq: sequence,
            stream,
            text,
        });
        while state.chunks.len() > PROJECT_TERMINAL_RETAINED_CHUNK_LIMIT
            || state.retained_bytes > PROJECT_TERMINAL_RETAINED_BYTE_LIMIT
        {
            let Some(dropped) = state.chunks.pop_front() else {
                break;
            };
            state.retained_bytes = state.retained_bytes.saturating_sub(dropped.text.len());
            state.dropped_before_seq = dropped.seq;
        }
        drop(state);
        self.changed.notify_waiters();
    }

    /// Returns ordered chunks after a display cursor without changing runtime state.
    /// 返回显示游标之后的有序分片，但不改变运行时状态。
    ///
    /// # Parameters / 参数
    /// - `after_seq`: Last output sequence rendered by the display.
    /// - `max_chunks`: Maximum chunks in this delivery.
    /// - `after_seq`：显示层已渲染的最后输出序号。
    /// - `max_chunks`：本次交付的最大分片数。
    ///
    /// # Returns / 返回值
    /// Returns current snapshot, ordered changed chunks, and remaining-data indicator.
    /// 返回当前快照、有序变化分片及剩余数据指示。
    async fn read_since(&self, after_seq: u64, max_chunks: usize) -> ProjectTerminalReadOutcome {
        let state = self.state.lock().await;
        // A visible display that falls behind the eviction boundary must rebuild from the current screen.
        // 落后于淘汰边界的可见显示层必须从当前屏幕状态重建。
        if after_seq < state.dropped_before_seq && state.last_seq > 0 {
            return ProjectTerminalReadOutcome {
                snapshot: self.snapshot_from_state(&state),
                chunks: vec![ProjectTerminalOutputChunk {
                    seq: state.last_seq,
                    stream: "stdout",
                    text: String::from_utf8_lossy(&state.display_parser.screen().state_formatted())
                        .into_owned(),
                }],
                has_more: false,
            };
        }
        let matching = state
            .chunks
            .iter()
            .filter(|chunk| chunk.seq > after_seq)
            .cloned()
            .collect::<Vec<_>>();
        let has_more = matching.len() > max_chunks.max(1);
        let chunks = matching.into_iter().take(max_chunks.max(1)).collect();
        ProjectTerminalReadOutcome {
            snapshot: self.snapshot_from_state(&state),
            chunks,
            has_more,
        }
    }

    /// Returns a bounded ordered output tail for one-time display bootstrap.
    /// 返回用于显示层一次性首次加载的有界有序输出尾部。
    ///
    /// # Parameters / 参数
    /// - `max_chunks`: Maximum newest retained chunks.
    /// - `max_chunks`：最大最新保留分片数。
    ///
    /// # Returns / 返回值
    /// Returns current snapshot and the newest ordered retained output.
    /// 返回当前快照以及最新的有序保留输出。
    async fn read_recent(&self, max_chunks: usize) -> ProjectTerminalReadOutcome {
        let state = self.state.lock().await;
        let skip_count = state.chunks.len().saturating_sub(max_chunks.max(1));
        let checkpoint_required = state.dropped_before_seq > 0 || skip_count > 0;
        let chunks = if checkpoint_required && state.last_seq > 0 {
            vec![ProjectTerminalOutputChunk {
                seq: state.last_seq,
                stream: "stdout",
                text: String::from_utf8_lossy(&state.display_parser.screen().state_formatted())
                    .into_owned(),
            }]
        } else {
            state.chunks.iter().skip(skip_count).cloned().collect()
        };
        ProjectTerminalReadOutcome {
            snapshot: self.snapshot_from_state(&state),
            chunks,
            has_more: false,
        }
    }
}

/// Owns all project terminals independently from conversation sessions.
/// 独立于对话会话持有全部项目终端。
pub struct ProjectTerminalRuntime {
    /// Registry keyed by stable terminal identity.
    /// 以稳定终端标识为键的注册表。
    terminals: RwLock<HashMap<Uuid, Arc<ProjectTerminalRecord>>>,
    /// Allocates a stable process-local creation order independently from random UUID values.
    /// 独立于随机 UUID 值分配稳定的进程内创建序号。
    next_creation_order: AtomicU64,
    /// Allocates revisions for running-count changes across all projects.
    /// 为所有项目的运行数量变化分配修订号。
    next_running_count_revision: AtomicU64,
    /// Stores the latest running-count revision for each exact project.
    /// 保存每个精确项目最近一次运行数量修订号。
    running_count_revisions: RwLock<HashMap<String, u64>>,
}

impl Default for ProjectTerminalRuntime {
    /// Builds an empty project terminal runtime.
    /// 构建一个空项目终端运行时。
    ///
    /// # Returns / 返回值
    /// Returns a runtime with no project terminals.
    /// 返回不含项目终端的运行时。
    fn default() -> Self {
        Self::new()
    }
}

impl ProjectTerminalRuntime {
    /// Builds an empty project terminal runtime.
    /// 构建一个空项目终端运行时。
    ///
    /// # Returns / 返回值
    /// Returns a runtime ready to create project-owned terminals.
    /// 返回可创建项目级终端的运行时。
    pub fn new() -> Self {
        Self {
            terminals: RwLock::new(HashMap::new()),
            next_creation_order: AtomicU64::new(1),
            next_running_count_revision: AtomicU64::new(1),
            running_count_revisions: RwLock::new(HashMap::new()),
        }
    }

    /// Returns the number of currently running project terminals.
    /// 返回当前运行中的项目终端数量。
    ///
    /// # Returns
    /// 返回值
    /// Returns the exact global running-terminal count observed without blocking the supervisor.
    /// 返回在不阻塞监控器的情况下观察到的精确全局运行中终端计数。
    pub fn running_terminal_count(&self) -> Result<u64, String> {
        let records = self
            .terminals
            .read()
            .map_err(|_| String::from("Project terminal registry is poisoned."))?
            .values()
            .cloned()
            .collect::<Vec<_>>();
        Ok(records
            .iter()
            .filter(|record| record.is_running_now())
            .count() as u64)
    }

    /// Returns the latest running-count revision for one project.
    /// 返回一个项目最近一次运行数量修订号。
    ///
    /// # Parameters / 参数
    /// - `project_id`: Stable project identity.
    /// - `project_id`：稳定项目标识。
    ///
    /// # Returns / 返回值
    /// Returns zero before the project's first running-count change.
    /// 在项目首次发生运行数量变化前返回零。
    pub fn project_running_count_revision(&self, project_id: &str) -> Result<u64, String> {
        Ok(*self
            .running_count_revisions
            .read()
            .map_err(|_| {
                String::from("Project terminal running-count revision registry is poisoned.")
            })?
            .get(project_id)
            .unwrap_or(&0))
    }

    /// Advances the running-count revision for one project.
    /// 推进一个项目的运行数量修订号。
    ///
    /// # Parameters / 参数
    /// - `project_id`: Stable project identity whose running count changed.
    /// - `project_id`：运行数量发生变化的稳定项目标识。
    ///
    /// # Returns / 返回值
    /// Returns the newly assigned process-local revision.
    /// 返回新分配的进程内修订号。
    fn advance_project_running_count_revision(&self, project_id: &str) -> Result<u64, String> {
        let revision = self
            .next_running_count_revision
            .fetch_add(1, Ordering::Relaxed);
        self.running_count_revisions
            .write()
            .map_err(|_| {
                String::from("Project terminal running-count revision registry is poisoned.")
            })?
            .insert(project_id.to_owned(), revision);
        Ok(revision)
    }

    /// Starts one project terminal in its own isolated terminal-host process.
    /// 在独立隔离终端宿主进程中启动一个项目终端。
    ///
    /// # Parameters / 参数
    /// - `request`: Validated project, shell, cwd, name, and dimensions.
    /// - `request`：已验证的项目、Shell、工作目录、名称与尺寸。
    ///
    /// # Returns / 返回值
    /// Returns the initial running terminal snapshot after host acknowledgement.
    /// 在宿主确认后返回初始运行中终端快照。
    pub async fn start_terminal(
        self: &Arc<Self>,
        request: ProjectTerminalStartRequest,
    ) -> Result<ProjectTerminalSnapshot, String> {
        validate_project_terminal_start_request(&request)?;
        let connection = spawn_terminal_host(
            TerminalHostStartRequest {
                program: request.program,
                args: request.args,
                cwd: request.cwd.clone(),
                cols: request.cols,
                rows: request.rows,
            },
            &[],
            &[],
        )
        .await?;
        let terminal_id = Uuid::new_v4();
        let creation_order = self
            .next_creation_order
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
                current.checked_add(1)
            })
            .map_err(|_| String::from("Project terminal creation sequence was exhausted."))?;
        let (control_sender, control_receiver) = channel(PROJECT_TERMINAL_CONTROL_QUEUE_LIMIT);
        let record = Arc::new(ProjectTerminalRecord {
            terminal_id,
            creation_order,
            project_id: request.project_id,
            shell_id: request.shell_id,
            cwd: request.cwd,
            control_sender,
            state: Mutex::new(ProjectTerminalMutableState {
                name: request.name,
                status: ProjectTerminalStatus::Running,
                exit_code: None,
                failure_reason: None,
                last_seq: 0,
                dropped_before_seq: 0,
                state_revision: 0,
                last_accepted_input_seq: 0,
                chunks: VecDeque::new(),
                retained_bytes: 0,
                display_parser: vt100::Parser::new(request.rows, request.cols, 0),
                rows: request.rows,
                cols: request.cols,
            }),
            changed: Notify::new(),
            lifecycle_changed: Notify::new(),
        });
        self.terminals
            .write()
            .map_err(|_| String::from("Project terminal registry is poisoned."))?
            .insert(terminal_id, Arc::clone(&record));
        self.advance_project_running_count_revision(&record.project_id)?;
        tokio::spawn(run_project_terminal_supervisor(
            Arc::clone(self),
            Arc::clone(&record),
            connection,
            control_receiver,
        ));
        Ok(record.snapshot().await)
    }

    /// Lists terminals owned by one exact project without consulting conversation state.
    /// 列出一个精确项目拥有的终端，不查询对话状态。
    ///
    /// # Parameters / 参数
    /// - `project_id`: Stable project identity.
    /// - `project_id`：稳定项目标识。
    ///
    /// # Returns / 返回值
    /// Returns project terminals ordered by terminal identity for stable rendering.
    /// 返回按终端标识排序的项目终端，以保持渲染稳定。
    pub async fn list_project_terminals(
        &self,
        project_id: &str,
    ) -> Result<Vec<ProjectTerminalSnapshot>, String> {
        let mut records = self
            .terminals
            .read()
            .map_err(|_| String::from("Project terminal registry is poisoned."))?
            .values()
            .filter(|record| record.project_id == project_id)
            .cloned()
            .collect::<Vec<_>>();
        records.sort_by_key(|record| record.creation_order);
        let mut snapshots = Vec::with_capacity(records.len());
        for record in records {
            snapshots.push(record.snapshot().await);
        }
        Ok(snapshots)
    }

    /// Returns a bounded output tail for the first visible display attachment.
    /// 为首次可见显示层挂接返回有界输出尾部。
    ///
    /// # Parameters / 参数
    /// - `project_id`: Owning project identity.
    /// - `terminal_id`: Target terminal identity.
    /// - `max_chunks`: Maximum retained tail chunks.
    /// - `project_id`：归属项目标识。
    /// - `terminal_id`：目标终端标识。
    /// - `max_chunks`：最大保留尾部分片数。
    ///
    /// # Returns / 返回值
    /// Returns the display bootstrap snapshot and ordered output tail.
    /// 返回显示层首次加载快照及有序输出尾部。
    pub async fn read_recent_for_display(
        &self,
        project_id: &str,
        terminal_id: Uuid,
        max_chunks: usize,
    ) -> Result<ProjectTerminalReadOutcome, String> {
        Ok(self
            .terminal_for_project(project_id, terminal_id)?
            .read_recent(max_chunks)
            .await)
    }

    /// Waits on runtime notification until a visible display observes output or state change.
    /// 使用运行时通知等待，直至可见显示层观察到输出或状态变化。
    ///
    /// # Parameters / 参数
    /// - `project_id`: Owning project identity.
    /// - `terminal_id`: Target terminal identity.
    /// - `after_seq`: Last rendered output sequence.
    /// - `running_was_observed`: Whether the previous snapshot was running.
    /// - `observed_state_revision`: Last user-visible metadata revision seen by the display.
    /// - `max_chunks`: Maximum changed chunks returned once.
    /// - `project_id`：归属项目标识。
    /// - `terminal_id`：目标终端标识。
    /// - `after_seq`：最后已渲染输出序号。
    /// - `running_was_observed`：上一份快照是否为运行中。
    /// - `observed_state_revision`：显示层最后看到的用户可见元数据修订号。
    /// - `max_chunks`：单次返回的最大变化分片数。
    ///
    /// # Returns / 返回值
    /// Returns only after output or lifecycle state differs from the observed values.
    /// 仅在输出或生命周期状态与已观察值不同时返回。
    pub async fn wait_for_display_change(
        &self,
        project_id: &str,
        terminal_id: Uuid,
        after_seq: u64,
        running_was_observed: bool,
        observed_state_revision: u64,
        max_chunks: usize,
    ) -> Result<ProjectTerminalReadOutcome, String> {
        let record = self.terminal_for_project(project_id, terminal_id)?;
        loop {
            let notified = record.changed.notified();
            tokio::pin!(notified);
            // Register before reading state so notify_waiters cannot be lost between the read and await.
            // 在读取状态前完成注册，避免 notify_waiters 在读取与等待之间丢失。
            notified.as_mut().enable();
            let outcome = record.read_since(after_seq, max_chunks).await;
            let running_now = outcome.snapshot.status == ProjectTerminalStatus::Running;
            if !outcome.chunks.is_empty()
                || running_now != running_was_observed
                || outcome.snapshot.state_revision != observed_state_revision
            {
                return Ok(outcome);
            }
            notified.as_mut().await;
        }
    }

    /// Waits until one terminal leaves an observed lifecycle status.
    /// 等待一个终端离开已观察到的生命周期状态。
    ///
    /// # Parameters / 参数
    /// - `project_id`: Owning project identity.
    /// - `terminal_id`: Target terminal identity.
    /// - `observed_status`: Lifecycle status already observed by the caller.
    /// - `project_id`：归属项目标识。
    /// - `terminal_id`：目标终端标识。
    /// - `observed_status`：调用方已经观察到的生命周期状态。
    ///
    /// # Returns / 返回值
    /// Returns the first snapshot whose lifecycle status differs without carrying terminal output.
    /// 返回首个生命周期状态不同的快照，且不承载终端输出。
    pub async fn wait_for_lifecycle_change(
        &self,
        project_id: &str,
        terminal_id: Uuid,
        observed_status: ProjectTerminalStatus,
    ) -> Result<ProjectTerminalSnapshot, String> {
        let record = self.terminal_for_project(project_id, terminal_id)?;
        loop {
            let notified = record.lifecycle_changed.notified();
            tokio::pin!(notified);
            // Register before reading state so a fast child exit cannot be lost between read and await.
            // 在读取状态前完成注册，避免快速退出的子进程在读取与等待之间丢失通知。
            notified.as_mut().enable();
            let snapshot = record.snapshot().await;
            if snapshot.status != observed_status {
                return Ok(snapshot);
            }
            notified.as_mut().await;
        }
    }

    /// Writes one contiguous browser input batch to a running project terminal.
    /// 向一个运行中的项目终端写入一批连续浏览器输入。
    ///
    /// # Parameters / 参数
    /// - `project_id`: Owning project identity.
    /// - `terminal_id`: Target terminal identity.
    /// - `bytes`: Exact UTF-8/control bytes produced by xterm.
    /// - `first_client_seq`: First input event sequence in this batch.
    /// - `last_client_seq`: Last input event sequence in this batch.
    /// - `project_id`：归属项目标识。
    /// - `terminal_id`：目标终端标识。
    /// - `bytes`：由 xterm 产生的精确 UTF-8/控制字节。
    /// - `first_client_seq`：当前批次首个输入事件序号。
    /// - `last_client_seq`：当前批次最后一个输入事件序号。
    ///
    /// # Returns / 返回值
    /// Returns the highest accepted input sequence after the isolated host flushes the input frame.
    /// 在隔离宿主刷新输入帧后返回已接受的最高输入序号。
    pub async fn write_terminal_input_batch(
        &self,
        project_id: &str,
        terminal_id: Uuid,
        bytes: Vec<u8>,
        first_client_seq: u64,
        last_client_seq: u64,
        event_count: usize,
    ) -> Result<u64, String> {
        if bytes.is_empty() {
            return Err(String::from(
                "Project terminal input batch must not be empty.",
            ));
        }
        validate_project_terminal_input_range(first_client_seq, last_client_seq, event_count)?;
        let record = self
            .running_terminal_for_project(project_id, terminal_id)
            .await?;
        let (responder, response) = oneshot::channel();
        send_project_terminal_control(
            &record.control_sender,
            ProjectTerminalControl::WriteBatch {
                bytes,
                first_client_seq,
                last_client_seq,
                responder,
            },
        )
        .await?;
        wait_for_project_terminal_control(response).await?
    }

    /// Resizes one running project terminal.
    /// 调整一个运行中项目终端的尺寸。
    ///
    /// # Parameters / 参数
    /// - `project_id`: Owning project identity.
    /// - `terminal_id`: Target terminal identity.
    /// - `rows`: Positive row count.
    /// - `cols`: Positive column count.
    /// - `project_id`：归属项目标识。
    /// - `terminal_id`：目标终端标识。
    /// - `rows`：大于零的行数。
    /// - `cols`：大于零的列数。
    ///
    /// # Returns / 返回值
    /// Returns success after the isolated host accepts the resize frame.
    /// 隔离宿主接受尺寸调整帧后返回成功。
    pub async fn resize_terminal(
        &self,
        project_id: &str,
        terminal_id: Uuid,
        rows: u16,
        cols: u16,
    ) -> Result<(), String> {
        if rows == 0 || cols == 0 || rows > TERMINAL_HOST_MAX_ROWS || cols > TERMINAL_HOST_MAX_COLS
        {
            return Err(format!(
                "Project terminal dimensions must be within 1..={TERMINAL_HOST_MAX_COLS} columns and 1..={TERMINAL_HOST_MAX_ROWS} rows."
            ));
        }
        let record = self
            .running_terminal_for_project(project_id, terminal_id)
            .await?;
        let (responder, response) = oneshot::channel();
        send_project_terminal_control(
            &record.control_sender,
            ProjectTerminalControl::Resize {
                rows,
                cols,
                responder,
            },
        )
        .await?;
        wait_for_project_terminal_control(response).await?
    }

    /// Stops one running project terminal while retaining its project tab and output.
    /// 停止一个运行中的项目终端，同时保留其项目标签与输出。
    ///
    /// # Parameters / 参数
    /// - `project_id`: Owning project identity.
    /// - `terminal_id`: Target terminal identity.
    /// - `project_id`：归属项目标识。
    /// - `terminal_id`：目标终端标识。
    ///
    /// # Returns / 返回值
    /// Returns success after the isolated host accepts the stop request.
    /// 隔离宿主接受停止请求后返回成功。
    pub async fn stop_terminal(&self, project_id: &str, terminal_id: Uuid) -> Result<(), String> {
        let record = self
            .running_terminal_for_project(project_id, terminal_id)
            .await?;
        let (responder, response) = oneshot::channel();
        send_project_terminal_control(
            &record.control_sender,
            ProjectTerminalControl::Stop { responder },
        )
        .await?;
        wait_for_project_terminal_control(response).await?
    }

    /// Renames one exact project terminal without touching its process or display subscription.
    /// 重命名一个精确项目终端，不触碰其进程或显示订阅。
    ///
    /// # Parameters / 参数
    /// - `project_id`: Owning project identity.
    /// - `terminal_id`: Target terminal identity.
    /// - `name`: Trimmed user-facing name with at most 128 Unicode scalar values.
    /// - `project_id`：归属项目标识。
    /// - `terminal_id`：目标终端标识。
    /// - `name`：去除首尾空白且最多包含 128 个 Unicode 标量值的用户可见名称。
    ///
    /// # Returns / 返回值
    /// Returns the renamed immutable snapshot.
    /// 返回重命名后的不可变快照。
    pub async fn rename_terminal(
        &self,
        project_id: &str,
        terminal_id: Uuid,
        name: String,
    ) -> Result<ProjectTerminalSnapshot, String> {
        let name = name.trim();
        if name.is_empty() {
            return Err(String::from("Project terminal name must not be empty."));
        }
        if name.chars().count() > PROJECT_TERMINAL_NAME_MAX_CHARS {
            return Err(format!(
                "Project terminal name must not exceed {PROJECT_TERMINAL_NAME_MAX_CHARS} characters."
            ));
        }
        let record = self.terminal_for_project(project_id, terminal_id)?;
        let mut state = record.state.lock().await;
        state.name = name.to_owned();
        state.state_revision = state.state_revision.saturating_add(1);
        drop(state);
        record.changed.notify_waiters();
        Ok(record.snapshot().await)
    }

    /// Closes one project terminal, stopping it first when still running.
    /// 关闭一个项目终端，并在其仍运行时先停止它。
    ///
    /// # Parameters / 参数
    /// - `project_id`: Owning project identity.
    /// - `terminal_id`: Target terminal identity.
    /// - `project_id`：归属项目标识。
    /// - `terminal_id`：目标终端标识。
    ///
    /// # Returns / 返回值
    /// Returns success after removing the exact project-owned runtime record.
    /// 移除精确项目级运行时记录后返回成功。
    pub async fn close_terminal(&self, project_id: &str, terminal_id: Uuid) -> Result<(), String> {
        let record = self.terminal_for_project(project_id, terminal_id)?;
        if record.snapshot().await.status == ProjectTerminalStatus::Running {
            self.stop_terminal(project_id, terminal_id).await?;
        }
        self.terminals
            .write()
            .map_err(|_| String::from("Project terminal registry is poisoned."))?
            .remove(&terminal_id);
        self.advance_project_running_count_revision(project_id)?;
        Ok(())
    }

    /// Queues a graceful stop for every isolated project terminal before application shutdown.
    /// 在应用关闭前为每个隔离项目终端排入优雅停止请求。
    ///
    /// Closed supervisors are already terminal; a full live control queue fails explicitly while
    /// the operating-system pipe EOF and per-host process-tree guard remain the final cleanup path.
    /// 已关闭的监控器本就处于终态；活动控制队列已满时会明确失败，而操作系统管道 EOF 与每宿主进程树守卫仍作为最终清理路径。
    ///
    /// # Returns / 返回值
    /// Returns the number of live supervisors that accepted a stop request.
    /// 返回已接受停止请求的活动监控器数量。
    pub fn request_all_terminal_stops_for_shutdown(&self) -> Result<usize, String> {
        let records = self
            .terminals
            .read()
            .map_err(|_| String::from("Project terminal registry is poisoned."))?
            .values()
            .cloned()
            .collect::<Vec<_>>();
        let mut accepted = 0_usize;
        let mut full_terminal_ids = Vec::new();
        for record in records {
            let (responder, _response) = oneshot::channel();
            match record
                .control_sender
                .try_send(ProjectTerminalControl::Stop { responder })
            {
                Ok(()) => accepted = accepted.saturating_add(1),
                Err(TrySendError::Closed(_)) => {}
                Err(TrySendError::Full(_)) => {
                    full_terminal_ids.push(record.terminal_id.to_string());
                }
            }
        }
        if full_terminal_ids.is_empty() {
            Ok(accepted)
        } else {
            Err(format!(
                "Project terminal shutdown control queues were full: {}.",
                full_terminal_ids.join(", ")
            ))
        }
    }

    /// Resolves one exact terminal and enforces project ownership.
    /// 解析一个精确终端并强制校验项目归属。
    ///
    /// # Parameters / 参数
    /// - `project_id`: Expected owning project identity.
    /// - `terminal_id`: Target terminal identity.
    /// - `project_id`：预期归属项目标识。
    /// - `terminal_id`：目标终端标识。
    ///
    /// # Returns / 返回值
    /// Returns the runtime record only when the project identity matches exactly.
    /// 仅在项目标识精确匹配时返回运行时记录。
    fn terminal_for_project(
        &self,
        project_id: &str,
        terminal_id: Uuid,
    ) -> Result<Arc<ProjectTerminalRecord>, String> {
        let record = self
            .terminals
            .read()
            .map_err(|_| String::from("Project terminal registry is poisoned."))?
            .get(&terminal_id)
            .cloned()
            .ok_or_else(|| format!("Project terminal `{terminal_id}` was not found."))?;
        if record.project_id != project_id {
            return Err(format!(
                "Project terminal `{terminal_id}` does not belong to project `{project_id}`."
            ));
        }
        Ok(record)
    }

    /// Resolves one exact project terminal and requires its running state.
    /// 解析一个精确项目终端并要求其处于运行状态。
    ///
    /// # Parameters / 参数
    /// - `project_id`: Expected owning project identity.
    /// - `terminal_id`: Target terminal identity.
    /// - `project_id`：预期归属项目标识。
    /// - `terminal_id`：目标终端标识。
    ///
    /// # Returns / 返回值
    /// Returns the running record or an explicit lifecycle error.
    /// 返回运行中记录或明确的生命周期错误。
    async fn running_terminal_for_project(
        &self,
        project_id: &str,
        terminal_id: Uuid,
    ) -> Result<Arc<ProjectTerminalRecord>, String> {
        let record = self.terminal_for_project(project_id, terminal_id)?;
        let status = record.state.lock().await.status;
        if status != ProjectTerminalStatus::Running {
            return Err(format!(
                "Project terminal `{terminal_id}` is `{}` and cannot accept this control.",
                status.as_str()
            ));
        }
        Ok(record)
    }
}

/// Validates mandatory project terminal fields before starting an external process.
/// 在启动外部进程前验证项目终端必填字段。
///
/// # Parameters / 参数
/// - `request`: Project terminal start request to validate.
/// - `request`：需要验证的项目终端启动请求。
///
/// # Returns / 返回值
/// Returns success only when identities, paths, program, name, and dimensions are valid.
/// 仅在标识、路径、程序、名称与尺寸均有效时返回成功。
fn validate_project_terminal_start_request(
    request: &ProjectTerminalStartRequest,
) -> Result<(), String> {
    if request.project_id.trim().is_empty() {
        return Err(String::from(
            "Project terminal project id must not be empty.",
        ));
    }
    if request.name.trim().is_empty() {
        return Err(String::from("Project terminal name must not be empty."));
    }
    if request.name.trim().chars().count() > PROJECT_TERMINAL_NAME_MAX_CHARS {
        return Err(format!(
            "Project terminal name must not exceed {PROJECT_TERMINAL_NAME_MAX_CHARS} characters."
        ));
    }
    if request.shell_id.trim().is_empty() || request.program.trim().is_empty() {
        return Err(String::from(
            "Project terminal shell id and program must not be empty.",
        ));
    }
    if !request.cwd.is_dir() {
        return Err(format!(
            "Project terminal working directory `{}` does not exist or is not a directory.",
            request.cwd.display()
        ));
    }
    if request.rows == 0
        || request.cols == 0
        || request.rows > TERMINAL_HOST_MAX_ROWS
        || request.cols > TERMINAL_HOST_MAX_COLS
    {
        return Err(format!(
            "Project terminal dimensions must be within 1..={TERMINAL_HOST_MAX_COLS} columns and 1..={TERMINAL_HOST_MAX_ROWS} rows."
        ));
    }
    Ok(())
}

/// Validates one browser input batch's declared contiguous sequence range.
/// 验证一个浏览器输入批次声明的连续序号范围。
///
/// # Parameters / 参数
/// - `first_client_seq`: First sequence included by the batch.
/// - `last_client_seq`: Last sequence included by the batch.
/// - `event_count`: Number of ordered events represented by the batch.
/// - `first_client_seq`：批次包含的首个序号。
/// - `last_client_seq`：批次包含的最后序号。
/// - `event_count`：批次表示的有序事件数量。
///
/// # Returns / 返回值
/// Returns success only when the range is positive, contiguous, and overflow-free.
/// 仅当范围为正数、连续且没有溢出时返回成功。
fn validate_project_terminal_input_range(
    first_client_seq: u64,
    last_client_seq: u64,
    event_count: usize,
) -> Result<(), String> {
    if first_client_seq == 0 || last_client_seq < first_client_seq {
        return Err(String::from(
            "Project terminal input batch contains an invalid client sequence range.",
        ));
    }
    let event_offset = u64::try_from(event_count.saturating_sub(1))
        .map_err(|_| String::from("Project terminal input event count is too large."))?;
    let expected_last_client_seq = first_client_seq
        .checked_add(event_offset)
        .ok_or_else(|| String::from("Project terminal input sequence range overflowed."))?;
    if event_count == 0 || expected_last_client_seq != last_client_seq {
        return Err(String::from(
            "Project terminal input event count does not match its client sequence range.",
        ));
    }
    Ok(())
}

/// Validates that one input batch begins immediately after the last accepted event.
/// 验证一个输入批次是否紧接在最后已接受事件之后开始。
///
/// # Parameters / 参数
/// - `last_accepted_seq`: Highest contiguous event already flushed to the host pipe.
/// - `first_client_seq`: First sequence carried by the new batch.
/// - `last_accepted_seq`：已刷新到宿主管道的最高连续事件序号。
/// - `first_client_seq`：新批次携带的首个序号。
///
/// # Returns / 返回值
/// Returns success only for the exact next sequence; duplicates and gaps fail closed.
/// 仅精确的下一个序号返回成功；重复与缺口均封闭失败。
fn validate_project_terminal_input_start(
    last_accepted_seq: u64,
    first_client_seq: u64,
) -> Result<(), String> {
    let expected = last_accepted_seq.checked_add(1).ok_or_else(|| {
        String::from("Project terminal input sequence exhausted its valid range.")
    })?;
    if expected != first_client_seq {
        return Err(format!(
            "Project terminal input sequence gap: expected {expected}, received {first_client_seq}."
        ));
    }
    Ok(())
}

/// Enqueues one terminal control through the bounded supervisor channel.
/// 通过有界监控器通道排入一个终端控制。
///
/// # Parameters / 参数
/// - `sender`: Bounded sender owned by the target terminal record.
/// - `control`: Exact serialized terminal control.
/// - `sender`：目标终端记录持有的有界发送端。
/// - `control`：精确的串行终端控制。
///
/// # Returns / 返回值
/// Returns success after queue admission or an explicit timeout/disconnect failure.
/// 在队列接收后返回成功，或返回明确的超时/断开故障。
async fn send_project_terminal_control(
    sender: &Sender<ProjectTerminalControl>,
    control: ProjectTerminalControl,
) -> Result<(), String> {
    timeout(PROJECT_TERMINAL_CONTROL_TIMEOUT, sender.send(control))
        .await
        .map_err(|_| String::from("Timed out enqueueing project terminal control."))?
        .map_err(|_| String::from("Project terminal supervisor is not available."))
}

/// Waits for one bounded terminal control acknowledgement.
/// 等待一个有界的终端控制确认。
///
/// # Parameters / 参数
/// - `response`: One-shot response from the terminal supervisor.
/// - `response`：来自终端监控器的单次响应。
///
/// # Returns / 返回值
/// Returns the supervisor result or an explicit timeout/disconnect failure.
/// 返回监控器结果或明确的超时/断开故障。
async fn wait_for_project_terminal_control<T>(
    response: oneshot::Receiver<Result<T, String>>,
) -> Result<Result<T, String>, String> {
    timeout(PROJECT_TERMINAL_CONTROL_TIMEOUT, response)
        .await
        .map_err(|_| String::from("Timed out waiting for project terminal control."))?
        .map_err(|_| String::from("Project terminal supervisor dropped the control response."))
}

/// Runs one project terminal supervisor until its isolated host exits.
/// 运行一个项目终端监控器，直至其隔离宿主退出。
///
/// # Parameters / 参数
/// - `record`: Project-owned runtime record.
/// - `connection`: Acknowledged isolated host connection.
/// - `controls`: Serialized runtime control receiver.
/// - `record`：项目级运行时记录。
/// - `connection`：已确认的隔离宿主连接。
/// - `controls`：串行化运行时控制接收端。
async fn run_project_terminal_supervisor(
    runtime: Arc<ProjectTerminalRuntime>,
    record: Arc<ProjectTerminalRecord>,
    connection: TerminalHostConnection,
    mut controls: Receiver<ProjectTerminalControl>,
) {
    let TerminalHostConnection {
        mut child,
        mut request_writer,
        mut event_receiver,
        mut stderr,
        terminal_process_id: _,
    } = connection;
    let stderr_record = Arc::clone(&record);
    let stderr_task = tokio::spawn(async move {
        let mut buffer = vec![0_u8; 4096];
        loop {
            match stderr.read(&mut buffer).await {
                Ok(0) => break,
                Ok(bytes_read) => {
                    stderr_record
                        .append_output(
                            "stderr",
                            format!(
                                "[terminal-host] {}",
                                String::from_utf8_lossy(&buffer[..bytes_read])
                            ),
                        )
                        .await;
                }
                Err(error) => {
                    stderr_record
                        .append_output(
                            "stderr",
                            format!("[terminal-host] Failed to read diagnostics: {error}\n"),
                        )
                        .await;
                    break;
                }
            }
        }
    });
    let mut wait_task = tokio::spawn(async move { child.wait().await });
    let mut decoder = ShellOutputStreamDecoder::new();
    let mut stdout_open = true;
    let mut stopped_by_request = false;
    let mut terminal_exit: Option<TerminalHostExitedEvent> = None;
    let mut last_host_event_sequence = 1_u64;
    let mut stop_requested_at: Option<tokio::time::Instant> = None;
    // host_wait_result retains host completion until every framed stdout event has been drained.
    // host_wait_result 保留宿主完成结果，直到所有 stdout 分帧事件均已排空。
    let mut host_wait_result: Option<std::io::Result<std::process::ExitStatus>> = None;

    let host_wait_result = loop {
        if !stdout_open && let Some(wait_result) = host_wait_result.take() {
            break wait_result;
        }
        tokio::select! {
            frame_result = event_receiver.recv(), if stdout_open => {
                match frame_result {
                    Some(Ok(frame)) => if let Err(error) = validate_terminal_host_sequence(
                        &mut last_host_event_sequence,
                        frame.sequence,
                    ) {
                        stdout_open = false;
                        remember_project_terminal_failure(
                            &record,
                            format!("Terminal-host event sequence validation failed: {error}"),
                        ).await;
                        wait_task.abort();
                        let _ = (&mut wait_task).await;
                        host_wait_result = Some(Err(std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            format!("Terminal-host event sequence validation failed: {error}"),
                        )));
                        stopped_by_request = true;
                    } else {
                        match frame.kind {
                        TerminalHostFrameKind::OutputBytes => {
                            let text = decoder.push_bytes(&frame.payload);
                            record.append_output("stdout", text).await;
                        }
                        TerminalHostFrameKind::Exited => {
                            match decode_terminal_host_json::<TerminalHostExitedEvent>(&frame.payload) {
                                Ok(exit) => {
                                    stopped_by_request = stopped_by_request || exit.stopped_by_request;
                                    terminal_exit = Some(exit);
                                }
                                Err(error) => {
                                    remember_project_terminal_failure(
                                        &record,
                                        format!("Failed to decode terminal-host exit event: {error}"),
                                    ).await;
                                }
                            }
                        }
                        TerminalHostFrameKind::Error => {
                            match decode_terminal_host_json::<TerminalHostErrorEvent>(&frame.payload) {
                                Ok(error) => {
                                    remember_project_terminal_failure(
                                        &record,
                                        format!("Terminal host failed [{}]: {}", error.code, error.message),
                                    ).await;
                                }
                                Err(error) => {
                                    remember_project_terminal_failure(
                                        &record,
                                        format!("Failed to decode terminal-host error event: {error}"),
                                    ).await;
                                }
                            }
                        }
                        TerminalHostFrameKind::StateChanged => {}
                        unexpected => {
                            remember_project_terminal_failure(
                                &record,
                                format!("Terminal host emitted an unexpected event: {unexpected:?}."),
                            ).await;
                        }
                        }
                    },
                    None => stdout_open = false,
                    Some(Err(error)) => {
                        stdout_open = false;
                        remember_project_terminal_failure(
                            &record,
                            format!("Failed to read terminal-host event: {error}"),
                        ).await;
                    }
                }
            }
            wait_result = &mut wait_task, if host_wait_result.is_none() => {
                // The host writes Exited before process termination; defer finalization until pipe EOF drains it.
                // 宿主会在进程终止前写入 Exited；因此延迟结束，直到管道 EOF 将其排空。
                host_wait_result = Some(match wait_result {
                    Ok(result) => result,
                    Err(error) => Err(std::io::Error::other(format!(
                        "Terminal-host wait task failed: {error}"
                    ))),
                });
            }
            control = controls.recv() => match control {
                Some(ProjectTerminalControl::WriteBatch {
                    bytes,
                    first_client_seq,
                    last_client_seq,
                    responder,
                }) => {
                    let last_accepted_client_seq = {
                        let state = record.state.lock().await;
                        state.last_accepted_input_seq
                    };
                    let result = match validate_project_terminal_input_start(
                        last_accepted_client_seq,
                        first_client_seq,
                    ) {
                        Ok(()) => {
                            match request_writer.write_input(bytes).await {
                                Ok(_) => {
                                    let mut state = record.state.lock().await;
                                    state.last_accepted_input_seq = last_client_seq;
                                    Ok(last_client_seq)
                                }
                                Err(error) => Err(error),
                            }
                        }
                        Err(error) => Err(error),
                    };
                    let _ = responder.send(result);
                }
                Some(ProjectTerminalControl::Resize { rows, cols, responder }) => {
                    let result = request_writer.resize(cols, rows).await;
                    if result.is_ok() {
                        let mut state = record.state.lock().await;
                        state.display_parser.screen_mut().set_size(rows, cols);
                        state.rows = rows;
                        state.cols = cols;
                        state.state_revision = state.state_revision.saturating_add(1);
                    }
                    let _ = responder.send(result);
                }
                Some(ProjectTerminalControl::Stop { responder }) => {
                    stopped_by_request = true;
                    stop_requested_at.get_or_insert_with(tokio::time::Instant::now);
                    let result = request_writer.stop().await;
                    let _ = responder.send(result);
                }
                None => {
                    stopped_by_request = true;
                    stop_requested_at.get_or_insert_with(tokio::time::Instant::now);
                    let _ = request_writer.stop().await;
                }
            },
            _ = wait_for_terminal_host_stop_deadline(
                stop_requested_at,
                PROJECT_TERMINAL_HOST_STOP_GRACE,
            ), if host_wait_result.is_none() => {
                let message = String::from(
                    "Terminal host did not exit within the 2-second stop grace period.",
                );
                remember_project_terminal_failure(&record, message.clone()).await;
                stdout_open = false;
                wait_task.abort();
                let _ = (&mut wait_task).await;
                host_wait_result = Some(Err(std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    message,
                )));
                stopped_by_request = true;
            }
        }
    };

    record.append_output("stdout", decoder.finish()).await;
    let _ = stderr_task.await;
    let mut state = record.state.lock().await;
    match (host_wait_result, terminal_exit) {
        (Ok(_), Some(exit)) => {
            state.exit_code = exit.exit_code.map(|code| code as i32);
            if state.failure_reason.is_some() {
                state.status = ProjectTerminalStatus::Failed;
            } else if stopped_by_request || exit.stopped_by_request {
                state.status = ProjectTerminalStatus::Stopped;
            } else if exit.exit_code == Some(0) {
                state.status = ProjectTerminalStatus::Completed;
            } else {
                state.status = ProjectTerminalStatus::Failed;
                state.failure_reason = Some(match exit.exit_code {
                    Some(code) => format!("Project terminal exited with status code {code}."),
                    None => String::from("Project terminal exited without a status code."),
                });
            }
        }
        (Ok(_), None) => {
            state.status = ProjectTerminalStatus::Failed;
            state.failure_reason.get_or_insert_with(|| {
                String::from(
                    "The terminal host exited without reporting the terminal child exit state.",
                )
            });
        }
        (Err(error), _) => {
            state.status = ProjectTerminalStatus::Failed;
            state.failure_reason = Some(format!("Failed to wait for terminal host: {error}"));
        }
    }
    drop(state);
    let _ = runtime.advance_project_running_count_revision(&record.project_id);
    record.lifecycle_changed.notify_waiters();
    record.changed.notify_waiters();
}

/// Records a failure without finalizing lifecycle state before the host exits.
/// 在宿主退出前记录故障，但不提前结束生命周期状态。
///
/// # Parameters / 参数
/// - `record`: Project terminal receiving the diagnostic.
/// - `message`: Exact failure message.
/// - `record`：接收诊断的项目终端。
/// - `message`：精确故障消息。
async fn remember_project_terminal_failure(record: &ProjectTerminalRecord, message: String) {
    let mut state = record.state.lock().await;
    if state.failure_reason.as_deref() == Some(message.as_str()) {
        return;
    }
    state.failure_reason = Some(message);
    // Failure text is display-visible metadata, so advance the same revision observed by active subscriptions.
    // 故障文本属于显示层可见元数据，因此推进活动订阅观察的同一修订号。
    state.state_revision = state.state_revision.saturating_add(1);
    drop(state);
    record.changed.notify_waiters();
}

#[cfg(test)]
mod tests {
    use super::{
        PROJECT_TERMINAL_RETAINED_BYTE_LIMIT, PROJECT_TERMINAL_RETAINED_CHUNK_LIMIT,
        ProjectTerminalMutableState, ProjectTerminalRecord, ProjectTerminalRuntime,
        ProjectTerminalStartRequest, ProjectTerminalStatus, remember_project_terminal_failure,
        validate_project_terminal_input_range, validate_project_terminal_input_start,
        validate_project_terminal_start_request,
    };
    use crate::terminal_host_protocol::TERMINAL_HOST_MAX_ROWS;
    use std::{collections::VecDeque, path::PathBuf, sync::Arc};
    use tokio::sync::{Mutex, Notify, mpsc::channel};
    use tokio::time::{Duration, timeout};
    use uuid::Uuid;

    /// Verifies project running-count revisions advance independently and monotonically.
    /// 验证项目运行数量修订号独立且单调推进。
    #[test]
    fn project_running_count_revision_advances_monotonically() {
        let runtime = ProjectTerminalRuntime::new();
        assert_eq!(
            runtime
                .project_running_count_revision("project-a")
                .expect("initial project revision"),
            0
        );
        let first = runtime
            .advance_project_running_count_revision("project-a")
            .expect("first project revision");
        let second = runtime
            .advance_project_running_count_revision("project-a")
            .expect("second project revision");
        let other = runtime
            .advance_project_running_count_revision("project-b")
            .expect("other project revision");

        assert!(second > first);
        assert!(other > second);
        assert_eq!(
            runtime
                .project_running_count_revision("project-a")
                .expect("latest project revision"),
            second
        );
        assert_eq!(
            runtime
                .project_running_count_revision("project-b")
                .expect("latest other project revision"),
            other
        );
    }

    /// Verifies a project terminal cannot be created with a zero-sized character grid.
    /// 验证不能使用零尺寸字符网格创建项目终端。
    #[test]
    fn start_request_rejects_zero_dimensions() {
        let request = ProjectTerminalStartRequest {
            project_id: String::from("project-a"),
            name: String::from("Terminal 1"),
            shell_id: String::from("pwsh"),
            program: String::from("pwsh"),
            args: Vec::new(),
            cwd: PathBuf::from("."),
            rows: 0,
            cols: 80,
        };
        let error = validate_project_terminal_start_request(&request)
            .expect_err("zero rows must be rejected");
        assert!(error.contains("within 1..="));
    }

    /// Verifies oversized dimensions are rejected before allocating the headless screen checkpoint.
    /// 验证过大尺寸会在分配无界面屏幕检查点前被拒绝。
    #[test]
    fn start_request_rejects_oversized_dimensions() {
        let request = ProjectTerminalStartRequest {
            project_id: String::from("project-a"),
            name: String::from("Terminal 1"),
            shell_id: String::from("pwsh"),
            program: String::from("pwsh"),
            args: Vec::new(),
            cwd: PathBuf::from("."),
            rows: TERMINAL_HOST_MAX_ROWS + 1,
            cols: 80,
        };
        let error = validate_project_terminal_start_request(&request)
            .expect_err("oversized rows must be rejected");
        assert!(error.contains("within 1..="));
    }

    /// Verifies batch metadata must describe one exact positive contiguous input range.
    /// 验证批次元数据必须描述一个精确、为正且连续的输入范围。
    #[test]
    fn input_range_requires_exact_contiguous_metadata() {
        validate_project_terminal_input_range(1, 3, 3)
            .expect("one exact contiguous range must be accepted");
        assert!(validate_project_terminal_input_range(0, 0, 1).is_err());
        assert!(validate_project_terminal_input_range(1, 3, 2).is_err());
        assert!(validate_project_terminal_input_range(u64::MAX, u64::MAX, 2).is_err());
    }

    /// Verifies duplicate and skipped browser input sequences fail closed.
    /// 验证重复及跳跃的浏览器输入序号都会封闭失败。
    #[test]
    fn input_start_rejects_duplicates_and_gaps() {
        validate_project_terminal_input_start(0, 1)
            .expect("the first browser event must begin at sequence one");
        assert!(validate_project_terminal_input_start(1, 1).is_err());
        assert!(validate_project_terminal_input_start(1, 3).is_err());
        assert!(validate_project_terminal_input_start(u64::MAX, 1).is_err());
    }

    /// Verifies retained terminal output stays bounded and publishes its exact eviction boundary.
    /// 验证终端保留输出保持有界，并公布精确淘汰边界。
    #[tokio::test]
    async fn retained_output_is_bounded_by_chunks_and_bytes() {
        let (control_sender, _control_receiver) = channel(1);
        let record = ProjectTerminalRecord {
            terminal_id: Uuid::new_v4(),
            creation_order: 1,
            project_id: String::from("project-a"),
            shell_id: String::from("pwsh"),
            cwd: PathBuf::from("."),
            control_sender,
            state: Mutex::new(ProjectTerminalMutableState {
                name: String::from("Terminal 1"),
                status: ProjectTerminalStatus::Running,
                exit_code: None,
                failure_reason: None,
                last_seq: 0,
                dropped_before_seq: 0,
                state_revision: 0,
                last_accepted_input_seq: 0,
                chunks: VecDeque::new(),
                retained_bytes: 0,
                display_parser: vt100::Parser::new(24, 80, 0),
                rows: 24,
                cols: 80,
            }),
            changed: Notify::new(),
            lifecycle_changed: Notify::new(),
        };

        for _ in 0..(PROJECT_TERMINAL_RETAINED_CHUNK_LIMIT + 2) {
            record.append_output("stdout", String::from("x")).await;
        }
        let state = record.state.lock().await;
        assert_eq!(state.chunks.len(), PROJECT_TERMINAL_RETAINED_CHUNK_LIMIT);
        assert_eq!(state.dropped_before_seq, 2);
        assert_eq!(state.retained_bytes, PROJECT_TERMINAL_RETAINED_CHUNK_LIMIT);
        drop(state);

        record
            .append_output("stdout", "y".repeat(PROJECT_TERMINAL_RETAINED_BYTE_LIMIT))
            .await;
        let state = record.state.lock().await;
        assert_eq!(state.chunks.len(), 1);
        assert_eq!(state.retained_bytes, PROJECT_TERMINAL_RETAINED_BYTE_LIMIT);
        assert_eq!(
            state.dropped_before_seq,
            PROJECT_TERMINAL_RETAINED_CHUNK_LIMIT as u64 + 2
        );
        drop(state);

        let bootstrap = record
            .read_recent(PROJECT_TERMINAL_RETAINED_CHUNK_LIMIT)
            .await;
        assert_eq!(bootstrap.chunks.len(), 1);
        assert_eq!(bootstrap.chunks[0].seq, bootstrap.snapshot.last_seq);
        assert!(bootstrap.chunks[0].text.contains("y"));
        assert!(!bootstrap.has_more);

        let lagged_display = record.read_since(0, 1).await;
        assert_eq!(lagged_display.chunks.len(), 1);
        assert_eq!(
            lagged_display.chunks[0].seq,
            lagged_display.snapshot.last_seq
        );
        assert!(lagged_display.chunks[0].text.contains("y"));
        assert!(!lagged_display.has_more);
    }

    /// Verifies output appended after an active display wait wakes that exact subscription immediately.
    /// 验证活动显示等待建立后追加的输出会立即唤醒该精确订阅。
    #[tokio::test]
    async fn appended_output_wakes_active_display_wait() {
        // terminal_id identifies the exact running fixture observed by the display waiter.
        // terminal_id 标识显示等待器观察的精确运行中夹具。
        let terminal_id = Uuid::new_v4();
        let (control_sender, _control_receiver) = channel(1);
        // record starts without retained output so the first display read must enter notification wait.
        // record 从无保留输出状态开始，因此首次显示读取必须进入通知等待。
        let record = Arc::new(ProjectTerminalRecord {
            terminal_id,
            creation_order: 1,
            project_id: String::from("project-a"),
            shell_id: String::from("pwsh"),
            cwd: PathBuf::from("."),
            control_sender,
            state: Mutex::new(ProjectTerminalMutableState {
                name: String::from("Terminal 1"),
                status: ProjectTerminalStatus::Running,
                exit_code: None,
                failure_reason: None,
                last_seq: 0,
                dropped_before_seq: 0,
                state_revision: 0,
                last_accepted_input_seq: 0,
                chunks: VecDeque::new(),
                retained_bytes: 0,
                display_parser: vt100::Parser::new(24, 80, 0),
                rows: 24,
                cols: 80,
            }),
            changed: Notify::new(),
            lifecycle_changed: Notify::new(),
        });
        // runtime owns the fixture through the production project and terminal lookup path.
        // runtime 通过生产环境的项目与终端查找路径持有该夹具。
        let runtime = ProjectTerminalRuntime::new();
        runtime
            .terminals
            .write()
            .expect("project terminal registry")
            .insert(terminal_id, Arc::clone(&record));
        // display_wait is polled before output arrives to reproduce a live attached terminal.
        // display_wait 在输出到达前先被轮询，用于复现已实时挂接的终端。
        let display_wait = runtime.wait_for_display_change("project-a", terminal_id, 0, true, 0, 1);
        tokio::pin!(display_wait);
        tokio::select! {
            outcome = &mut display_wait => panic!("display wait returned before output: {outcome:?}"),
            _ = tokio::time::sleep(Duration::from_millis(10)) => {}
        }

        record
            .append_output("stdout", String::from("prompt> "))
            .await;
        // outcome must be pushed by the content notification instead of waiting for a later reattach snapshot.
        // outcome 必须由内容通知主动推送，而不是等待后续重新挂接快照。
        let outcome = timeout(Duration::from_millis(100), &mut display_wait)
            .await
            .expect("appended output must wake the active display")
            .expect("display change outcome");
        assert_eq!(outcome.chunks.len(), 1);
        assert_eq!(outcome.chunks[0].seq, 1);
        assert_eq!(outcome.chunks[0].text, "prompt> ");
    }

    /// Verifies a newly reported host failure immediately satisfies an active display revision wait.
    /// 验证新报告的宿主故障会立即满足活动显示层的修订等待。
    #[tokio::test]
    async fn failure_metadata_advances_display_revision() {
        // terminal_id identifies the exact fixture record inserted into the runtime registry.
        // terminal_id 标识插入运行时注册表的精确夹具记录。
        let terminal_id = Uuid::new_v4();
        let (control_sender, _control_receiver) = channel(1);
        // record begins at display revision zero with no output and no failure metadata.
        // record 从显示修订零开始，且没有输出和故障元数据。
        let record = Arc::new(ProjectTerminalRecord {
            terminal_id,
            creation_order: 1,
            project_id: String::from("project-a"),
            shell_id: String::from("pwsh"),
            cwd: PathBuf::from("."),
            control_sender,
            state: Mutex::new(ProjectTerminalMutableState {
                name: String::from("Terminal 1"),
                status: ProjectTerminalStatus::Running,
                exit_code: None,
                failure_reason: None,
                last_seq: 0,
                dropped_before_seq: 0,
                state_revision: 0,
                last_accepted_input_seq: 0,
                chunks: VecDeque::new(),
                retained_bytes: 0,
                display_parser: vt100::Parser::new(24, 80, 0),
                rows: 24,
                cols: 80,
            }),
            changed: Notify::new(),
            lifecycle_changed: Notify::new(),
        });
        // runtime owns the fixture through the same project/terminal lookup used by subscriptions.
        // runtime 通过订阅使用的同一项目/终端查询持有夹具。
        let runtime = ProjectTerminalRuntime::new();
        runtime
            .terminals
            .write()
            .expect("project terminal registry")
            .insert(terminal_id, Arc::clone(&record));

        remember_project_terminal_failure(&record, String::from("terminal host output failed"))
            .await;
        // outcome must return from revision comparison without requiring output or a status transition.
        // outcome 必须通过修订比较返回，不依赖输出或状态转换。
        let outcome = timeout(
            Duration::from_millis(100),
            runtime.wait_for_display_change("project-a", terminal_id, 0, true, 0, 1),
        )
        .await
        .expect("failure metadata must wake the visible display")
        .expect("display change outcome");
        assert_eq!(outcome.snapshot.state_revision, 1);
        assert_eq!(
            outcome.snapshot.failure_reason.as_deref(),
            Some("terminal host output failed")
        );
        assert!(outcome.chunks.is_empty());
    }
}
