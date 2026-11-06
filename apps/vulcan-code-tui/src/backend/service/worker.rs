// Service worker: one thread owning the connection, the workbench projection and event windows.
// 服务工作线程：独占连接、工作台投影与事件窗口的单一线程。
//
// Every input (UI command, `/events` frame, socket close, finished background call) arrives on one
// channel, so state is only touched here. Quick RPCs run inline; calls that can take long (model
// backed, network bound) run on short-lived threads and hand their result back as `Input::Done`.
// 所有输入（界面命令、`/events` 帧、套接字关闭、已完成的后台调用）都经由同一通道到达，因此状态只在
// 此处修改。快速 RPC 内联执行；可能耗时较长的调用（依赖模型或网络）在短生命周期线程中执行，结果以
// `Input::Done` 交回。

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::thread;
use std::time::{Duration, Instant};

use serde::de::DeserializeOwned;
use serde_json::{Map, Value, json};
use vulcan_service_api as api;
use vulcan_session_core::v1::EventBody as CoreEvent;
use vulcan_session_core::v1::service_dto::ConversationEventSummary;

use super::discovery;
use super::events::{self, EventStream};
use super::git::GitState;
use super::launch;
use super::map::{self, ModelRoute};
use super::rpc::{RpcClient, RpcError, parse_failure};
use crate::backend::{BackendEvent, Command, ConnectionState, Notice};
use crate::model::*;
use crate::service_api::fresh_command_id as fresh_id;
use crate::service_api::{
    CallKind, ServiceCall, Slot, scrub_secrets, workbench_snapshot::load_workbench_snapshot,
};

#[path = "answer_mode.rs"]
mod answer_mode;
#[path = "permission_mode.rs"]
mod permission_mode;

/// First reconnect delay; it doubles up to `RETRY_MAX`.
/// 首次重连延迟；逐次翻倍直到 `RETRY_MAX`。
const RETRY_MIN: Duration = Duration::from_millis(500);
/// Maximum reconnect delay after repeated transport failures.
/// 多次传输失败后的最大重连延迟。
const RETRY_MAX: Duration = Duration::from_secs(5);
/// Workbench fields the system areas read, as the workbench slot carries them. Conversation
/// state stays in the typed events; this subset is what settings, plugins and skills pages need.
/// 系统区域读取的工作台字段，即工作台槽位携带的内容。会话状态仍走类型化事件；该子集是设置、
/// 插件与技能页面所需的部分。
const WORKBENCH_SLOT_KEYS: [&str; 28] = [
    "appVersion",
    "uiLocale",
    "modules",
    "projects",
    "archivedConversations",
    "plugins",
    "luaSkills",
    "agentSkills",
    "subagentCatalog",
    "providers",
    "mcpServers",
    "mcpTools",
    "thinkingDisplayMode",
    "toolCollapseDefaultEnabled",
    "sessionTitleGenerationEnabled",
    "toolChainEnabled",
    "toolChainMaxSteps",
    "defaultAnswerMode",
    "gitCommit",
    "providerRetry",
    "providerRequestTraceEnabled",
    "providerErrorTraceEnabled",
    "hostTools",
    "outputStyleCatalog",
    "contextCompactionWindowNodes",
    "vmmMemory",
    "logs",
    "archivedProjects",
];
/// Prefix of local draft conversation ids; drafts exist only until their first submission.
/// 本地草稿会话 ID 前缀；草稿只存在到首次提交为止。
const DRAFT_PREFIX: &str = "draft:";
/// Storage error code for a session saved in a format this build no longer reads; the authority is
/// `vulcan_workspace_store::workspace_store::SessionV1StorageErrorCode::UnsupportedDevelopmentFormat`.
/// 会话以本构建不再读取的格式保存时的存储错误码；权威为工作区存储层的 `UnsupportedDevelopmentFormat`。
const UNSUPPORTED_SESSION_FORMAT_CODE: &str = "unsupported_development_format";

/// Everything the worker reacts to.
/// 工作线程响应的全部输入。
pub enum Input {
    /// The UI dropped its command sender; stop the event reader and release worker state.
    /// 界面释放了命令发送器；停止事件读取并释放工作线程状态。
    Shutdown,
    /// One typed UI command forwarded onto the serial worker channel.
    /// 转发到串行工作线程通道的单个类型化界面命令。
    Command(Command),
    /// One `/events` frame of connection `generation`, parsed as the shared service event.
    /// 连接代际 `generation` 的一帧 `/events`，按共享服务事件解析。
    Service {
        /// Connection generation that received this frame.
        /// 接收该帧的连接代际。
        generation: u64,
        /// Strictly parsed service event.
        /// 严格解析的服务事件。
        event: Box<api::ServiceEvent>,
    },
    /// One `/events` frame that does not match the shared service event type.
    /// 一帧不符合共享服务事件类型的 `/events`。
    Rejected {
        /// Connection generation that received this frame.
        /// 接收该帧的连接代际。
        generation: u64,
        /// Declared event kind when the frame names one.
        /// 帧中声明的事件类别（若有）。
        kind: String,
        /// Value-free description of the parse failure.
        /// 不含取值的解析失败描述。
        detail: String,
    },
    /// The reader closed; only its own connection generation may trigger recovery.
    /// 读取器关闭；仅其所属连接代际可触发恢复。
    EventsClosed {
        /// Generation of the reader that closed.
        /// 已关闭读取器的代际。
        generation: u64,
    },
    /// A background call finished; the closure folds its result into the worker.
    /// 后台调用已完成；闭包将结果折叠进工作线程。
    Done {
        /// Generation that dispatched the background operation.
        /// 发送后台操作的连接代际。
        generation: u64,
        /// Single-use result reducer, invoked only if its generation is still current.
        /// 单次结果归并器，仅在其代际仍为当前代际时调用。
        apply: Box<dyn FnOnce(&mut Worker) + Send>,
    },
}

/// Loaded slice of one conversation's v1 event document.
/// 单个会话 v1 事件文档的已加载片段。
struct Window {
    /// Accepted answer mode for this window.
    /// 当前窗口接纳的回答模式。
    answer_mode: AnswerModeSession,
    /// Saved permission mode of this window.
    /// 当前窗口已保存的权限模式。
    permission_mode: PermissionMode,
    /// Saved unattended mode of this window; `None` while it is off.
    /// 当前窗口已保存的无人值守模式；关闭时为 `None`。
    unattended: Option<UnattendedConfig>,
    /// Server event-document generation, distinct from the transport generation.
    /// 服务端事件文档代际，与传输代际独立。
    generation: u64,
    /// Server event epoch identifying the current document lineage.
    /// 标识当前文档谱系的服务端事件纪元。
    epoch: String,
    /// Last authoritative memory revision observed for this conversation.
    /// 该会话最后观察到的权威记忆修订号。
    memory_revision: u64,
    /// Event summaries by `seq`; text appends patch them in place.
    /// 以 `seq` 为键的事件摘要；文本追加就地修补。
    events: BTreeMap<u64, ConversationEventSummary>,
}

/// A conversation that exists only in the TUI until its first prompt is accepted.
/// 首条提示被接受前只存在于 TUI 的会话。
struct Draft {
    /// Draft answer mode, copied from the global default and sent with the first submission.
    /// 草稿回答模式，复制自全局默认值并随首次提交发送。
    answer_mode: AnswerModePreference,
    /// Explicit permission mode for the first submission; absent keeps the session default.
    /// 首次提交的显式权限模式；缺省时保留会话默认值。
    permission_mode: Option<PermissionMode>,
    /// Explicit project selection for the first accepted prompt.
    /// 首条被接受提示所使用的明确项目选择。
    project_id: Option<String>,
    /// Serialized provider/model/connection route selected before submission.
    /// 提交前选择并序列化的提供方、模型及连接路线。
    model_key: Option<String>,
}

/// Serial owner of connection state, workbench projections and scoped asynchronous results.
/// 连接状态、工作台投影及带作用域异步结果的串行持有者。
pub struct Worker {
    /// UUID of this TUI process for the `/events` socket, stable across reconnects.
    /// 本 TUI 进程用于 `/events` 套接字的 UUID，重连期间保持不变。
    client_id: String,
    /// UI event destination; no service state is shared directly with the UI.
    /// 界面事件目标；不与界面直接共享服务状态。
    out: Sender<BackendEvent>,
    /// Common inbox for UI commands, socket events and completed background calls.
    /// 界面命令、套接字事件及已完成后台调用的共同收件箱。
    inbox: Sender<Input>,
    /// Optional discovery root selected through backend options.
    /// 通过后端选项选择的可选发现根目录。
    root: Option<PathBuf>,
    /// Current authenticated RPC client, absent during disconnection.
    /// 当前已认证 RPC 客户端，断线期间为空。
    pub(super) client: Option<RpcClient>,
    /// Current socket reader and its explicit cancellation signal.
    /// 当前套接字读取器及其明确取消信号。
    stream: Option<EventStream>,
    /// Transport generation used to reject results from obsolete connections.
    /// 用于拒绝已过期连接结果的传输代际。
    generation: u64,
    /// Scheduled reconnect instant after a failed connection attempt.
    /// 连接尝试失败后安排的重连时刻。
    retry_at: Option<Instant>,
    /// Current reconnect delay bounded by the shared retry constants.
    /// 受共享重试常量限制的当前重连延迟。
    backoff: Duration,
    /// Last connect failure shown, so retries do not repeat the same toast.
    /// 最近一次已显示的连接失败，避免重试时重复提示。
    connect_error: Option<String>,
    /// The installed Core this TUI starts when none runs; absent for a bound root or a workspace build.
    /// 没有运行中的服务时本 TUI 启动的已安装 Core；绑定发现根或工作区构建时为空。
    launcher: Option<launch::Launcher>,
    /// Latest workbench snapshot with deltas folded in; absent until the first snapshot.
    /// 折叠了增量的最新工作台快照；首份快照到达前为空。
    pub(super) wb: Option<api::WorkbenchSnapshot>,
    /// Subagent navigation per parent conversation, from navigation deltas and route responses.
    /// 每个父会话的子代理导航，来自导航增量与路由响应。
    navigations: HashMap<String, api::SubagentNavigationSnapshot>,
    /// Last applied server workbench revision.
    /// 最后应用的服务端工作台修订号。
    revision: u64,
    /// Exact selected conversation identity, including local pre-submission drafts.
    /// 准确的选中会话身份，包含提交前的本地草稿。
    pub(super) selected: Option<String>,
    /// Event windows owned independently by each conversation identity.
    /// 按各会话身份独立持有的事件窗口。
    windows: HashMap<String, Window>,
    /// Local conversations awaiting their first accepted submission.
    /// 等待首次提交被接受的本地会话。
    drafts: HashMap<String, Draft>,
    /// Monotonic local draft identity sequence.
    /// 单调递增的本地草稿身份序列。
    draft_serial: u64,
    /// Model route key per conversation, from `session_config` or a local selection.
    /// 每个会话的模型路线键，来自 `session_config` 或本地选择。
    model_keys: HashMap<String, String>,
    /// Selected repository identity and latest staged/unstaged file snapshots.
    /// 所选仓库身份及最新暂存、未暂存文件快照。
    pub(super) git: GitState,
    /// Git state of projects not shown, by workspace path, so returning to one reads memory.
    /// 未显示项目的 Git 状态，按工作区路径保存，使回到该项目时读取内存。
    pub(super) git_parked: HashMap<String, GitState>,
    /// Last loader per slot, so mutations can reload what they invalidate.
    /// 每个槽位最近一次的加载器，供变更重新加载其失效的槽位。
    loaders: HashMap<Slot, ServiceCall>,
    /// Dispatch sequence per slot; stale responses cannot overwrite a later selection or reload.
    /// 每个槽位的发送序号；过期响应无法覆盖较新的选择或刷新。
    load_sequences: HashMap<Slot, u64>,

    /// Last emitted workbench slot, to emit only changes.
    /// 最近一次发出的工作台槽位，只在变化时发出。
    workbench_slot: Value,
    /// Whether a full authoritative snapshot permits publishing editable workbench settings.
    /// 是否已有完整权威快照，允许发布可编辑的工作台设置。
    workbench_slot_valid: bool,
    /// Update prompts already shown, as `offer:<version>` and `ready:<version>`, once each per
    /// process like the WebUI prompts.
    /// 已显示的更新提示，形如 `offer:<version>` 与 `ready:<version>`；与 WebUI 一样每个进程各提示一次。
    app_update_prompted: HashSet<String>,
}

/// Owns the worker event loop and stops transport readers when the UI closes.
/// 持有工作线程事件循环，在界面关闭时停止传输读取器。
pub fn run(
    out: Sender<BackendEvent>,
    inbox: Sender<Input>,
    root: Option<PathBuf>,
    rx: Receiver<Input>,
) {
    // Worker.
    // 串行服务工作线程状态。
    // A root given on the command line binds the TUI to a Core that already runs (the tray starts it that way).
    // 命令行给出的发现根把 TUI 绑定到已在运行的 Core（托盘即以此方式启动）。
    let bound = root.is_some();
    let mut worker = Worker::new(out, inbox, root);
    if !bound {
        worker.launcher = launch::Launcher::installed();
    }
    worker.emit(BackendEvent::Connection(ConnectionState::Connecting));
    loop {
        // Current input.
        // 当前输入。
        let input = match worker.retry_at {
            Some(at) => {
                // Wait.
                // 距下次重连的剩余时间。
                let wait = at.saturating_duration_since(Instant::now());
                match rx.recv_timeout(wait) {
                    Ok(input) => input,
                    Err(RecvTimeoutError::Timeout) => {
                        worker.retry_at = None;
                        worker.connect();
                        continue;
                    }
                    Err(RecvTimeoutError::Disconnected) => break,
                }
            }
            None => match rx.recv() {
                Ok(input) => input,
                Err(_) => break,
            },
        };
        if !worker.handle_input(input) {
            break;
        }
    }
    if let Some(stream) = worker.stream.take() {
        stream.stop();
    }
}

impl Worker {
    /// Creates isolated worker state from the output channel, inbox and discovery root.
    /// 根据输出通道、收件箱及发现根创建独立工作线程状态。
    /// Returns `Self`.
    /// 返回 `Self`。
    fn new(out: Sender<BackendEvent>, inbox: Sender<Input>, root: Option<PathBuf>) -> Self {
        Self {
            client_id: fresh_id(),
            out,
            inbox,
            root,
            client: None,
            stream: None,
            generation: 0,
            retry_at: Some(Instant::now()),
            backoff: RETRY_MIN,
            connect_error: None,
            launcher: None,
            wb: None,
            navigations: HashMap::new(),
            revision: 0,
            selected: None,
            windows: HashMap::new(),
            drafts: HashMap::new(),
            draft_serial: 0,
            model_keys: HashMap::new(),
            git: GitState::default(),
            git_parked: HashMap::new(),
            loaders: HashMap::new(),
            load_sequences: HashMap::new(),

            workbench_slot: Value::Null,
            workbench_slot_valid: false,
            app_update_prompted: HashSet::new(),
        }
    }

    /// Dispatches one generation-checked input; false requests complete worker shutdown.
    /// 分发一次检查代际后的输入；返回 false 请求完整停止工作线程。
    fn handle_input(&mut self, input: Input) -> bool {
        match input {
            Input::Shutdown => return false,
            Input::Command(command) => self.handle(command),
            Input::Service { generation, event } if generation == self.generation => {
                self.service_event(*event)
            }
            Input::Service { .. } => {}
            Input::Rejected {
                generation,
                kind,
                detail,
            } if generation == self.generation => {
                self.emit(BackendEvent::Notice(Notice::Unreadable {
                    source: kind,
                    detail,
                }))
            }
            Input::Rejected { .. } => {}
            Input::EventsClosed { generation } if generation == self.generation => {
                self.disconnected();
            }
            Input::EventsClosed { .. } => {}
            Input::Done { generation, apply }
                if generation == self.generation && self.client.is_some() =>
            {
                apply(self)
            }
            Input::Done { .. } => {}
        }
        true
    }

    /// Publishes one backend fact through the UI event channel.
    /// 通过界面事件通道发布一条后端事实。
    /// `event` is the backend fact being delivered.
    /// `event` 为正在交付的后端事实。
    pub(super) fn emit(&self, event: BackendEvent) {
        let _ = self.out.send(event);
    }

    /// Publishes a visible backend notice without blocking the UI.
    /// 发布可见后端提示，不阻塞界面。
    /// `severity` is the notice severity.
    /// `severity` 为提示严重程度。
    /// `message` is the displayed or sanitized error text.
    /// `message` 为显示文案或脱敏错误文本。
    pub(super) fn toast(&self, severity: NoticeSeverity, message: impl Into<String>) {
        self.emit(BackendEvent::Toast {
            severity,
            message: message.into(),
        });
    }

    // ---- Connection ----

    /// Attempts service discovery and reports connection state before scheduling retries.
    /// 尝试服务发现并报告连接状态，再安排重试。
    fn connect(&mut self) {
        match self.try_connect() {
            Ok(()) => {
                self.backoff = RETRY_MIN;
                self.connect_error = None;
                if let Some(launcher) = self.launcher.as_mut() {
                    launcher.connected();
                }
                self.emit(BackendEvent::Connection(ConnectionState::Connected));
            }
            Err(message) => {
                self.client = None;
                self.stop_stream();
                // No reachable Core: start the installed one once, and report it if it exits before it is reachable.
                // 没有可连接的 Core：启动一次已安装的 Core，若它在可连接之前退出则报告。
                let mut starting = false;
                if let Some(launcher) = self.launcher.as_mut() {
                    let exited = launcher.exited();
                    let started = launcher.start();
                    starting = launcher.starting();
                    if let Some(status) = exited {
                        self.emit(BackendEvent::Notice(Notice::ServiceStartFailed(status)));
                    }
                    match started {
                        Ok(true) => self.emit(BackendEvent::Notice(Notice::StartingService)),
                        Ok(false) => {}
                        Err(error) => {
                            self.emit(BackendEvent::Notice(Notice::ServiceStartFailed(error)))
                        }
                    }
                }
                // A Core still starting is expected to be unreachable; the retry loop connects once it is ready.
                // 仍在启动的 Core 本就无法连接；重试循环会在其就绪后连上。
                if !starting && self.connect_error.as_ref() != Some(&message) {
                    self.emit(BackendEvent::Notice(Notice::ServiceUnavailable(
                        message.clone(),
                    )));
                    self.connect_error = Some(message);
                }
                self.schedule_retry();
            }
        }
    }

    /// Discovers and probes endpoints, opens the event stream and loads authoritative workbench state.
    /// 发现并探测端点、打开事件流并加载权威工作台状态。
    /// Returns `Result<(), String>`.
    /// 返回 `Result<(), String>`。
    fn try_connect(&mut self) -> Result<(), String> {
        // Endpoints.
        // 已发现就绪服务端点。
        let endpoints = discovery::discover(self.root.as_deref()).map_err(|e| e.to_string())?;
        // Last error.
        // 最后一次连接失败。
        let mut last_error = String::new();
        // Client.
        // RPC 传输客户端。
        let client = endpoints
            .into_iter()
            .map(RpcClient::new)
            .find(|client| match client.probe() {
                Ok(()) => true,
                Err(error) => {
                    last_error = error.message;
                    false
                }
            })
            .ok_or(last_error)?;
        self.generation += 1;
        // Stream.
        // 当前传输流。
        let stream = events::open(
            client.endpoint(),
            &self.client_id,
            self.generation,
            self.inbox.clone(),
        )
        .map_err(|e| e.to_string())?;
        self.stream = Some(stream);
        // Snapshot.
        // 权威快照。
        let snapshot = client
            .call("workbench.snapshot", Value::Null)
            .map_err(|e| e.message)?;
        let snapshot: api::WorkbenchSnapshot = serde_json::from_value(snapshot)
            .map_err(|error| format!("workbench.snapshot: {}", parse_failure(&error)))?;
        self.client = Some(client);
        // A new service instance may have reset revisions and event documents.
        // 新的服务实例可能已重置版本号与事件文档。
        self.revision = 0;
        self.windows.clear();
        self.navigations.clear();
        self.apply_snapshot(snapshot);
        if let Some(id) = self.selected.clone() {
            if is_draft(&id) {
                self.emit_answer_mode(&id);
                self.emit_permission_mode(&id);
            } else {
                self.hydrate(&id);
            }
        }
        self.git_reconnected();
        Ok(())
    }

    /// Drops the active transport and schedules bounded service reconnection.
    /// 释放活动传输并安排有界服务重连。
    fn disconnected(&mut self) {
        self.stop_stream();
        if self.client.take().is_some() || self.retry_at.is_none() {
            self.emit(BackendEvent::Connection(ConnectionState::Reconnecting));
        }
        self.schedule_retry();
    }

    /// Stops the event stream and retires its generation, so frames it already queued are dropped.
    /// 停止事件流并作废其代际，使其已排队的帧被丢弃。
    fn stop_stream(&mut self) {
        if let Some(stream) = self.stream.take() {
            stream.stop();
        }
        self.generation += 1;
    }

    /// Schedules the next reconnect using the shared exponential-backoff limits.
    /// 使用共用指数退避边界安排下次重连。
    fn schedule_retry(&mut self) {
        self.retry_at = Some(Instant::now() + self.backoff);
        self.backoff = (self.backoff * 2).min(RETRY_MAX);
    }

    /// Calls one method inline; failures become a toast, transport failures a reconnect.
    /// 内联调用一个方法；失败转为提示，传输失败触发重连。
    /// `method` is the static WebUI RPC method name.
    /// `method` 为静态 WebUI RPC 方法名。
    /// `params` is the exact RPC parameter document.
    /// `params` 为准确 RPC 参数文档。
    /// Returns `Option<Value>`.
    /// 返回 `Option<Value>`。
    pub(super) fn call(&mut self, method: &str, params: Value) -> Option<Value> {
        // Client.
        // RPC 传输客户端。
        let Some(client) = self.client.clone() else {
            self.emit(BackendEvent::Notice(Notice::NotConnected));
            return None;
        };
        match client.call(method, params) {
            Ok(value) => Some(value),
            Err(error) => {
                self.failed(error);
                None
            }
        }
    }

    /// Calls one method inline and reads its result as the declared response type.
    /// 内联调用一个方法，并按声明的响应类型读取结果。
    /// `method` is the static WebUI RPC method name.
    /// `method` 为静态 WebUI RPC 方法名。
    /// `params` is the exact RPC parameter document.
    /// `params` 为准确 RPC 参数文档。
    pub(super) fn call_as<T: DeserializeOwned>(
        &mut self,
        method: &str,
        params: Value,
    ) -> Option<T> {
        let value = self.call(method, params)?;
        self.read(method, value)
    }

    /// Reads a response as its declared type; a mismatch is reported without echoing values.
    /// 按声明类型读取响应；不匹配时报告且不回显取值。
    /// `source` names the method or event that produced `value`.
    /// `source` 为产生 `value` 的方法或事件名。
    pub(super) fn read<T: DeserializeOwned>(&self, source: &str, value: Value) -> Option<T> {
        match serde_json::from_value(value) {
            Ok(value) => Some(value),
            Err(error) => {
                self.emit(BackendEvent::Notice(Notice::Unreadable {
                    source: source.to_string(),
                    detail: parse_failure(&error),
                }));
                None
            }
        }
    }

    /// Reports a sanitized RPC failure and reconnects on transport loss.
    /// 报告脱敏 RPC 失败，在传输丢失时重连。
    /// `error` is the already-sanitized transport/service error.
    /// `error` 为已经脱敏的传输或服务错误。
    pub(super) fn failed(&mut self, error: RpcError) {
        self.toast(NoticeSeverity::Error, error.message.clone());
        if error.transport {
            self.disconnected();
        }
    }

    /// Runs one call on a background thread and folds its outcome back on the worker.
    /// 在后台线程执行一次调用，并在工作线程上折叠其结果。
    /// `method` is the static WebUI RPC method name.
    /// `method` 为静态 WebUI RPC 方法名。
    /// `params` is the exact RPC parameter document.
    /// `params` 为准确 RPC 参数文档。
    pub(super) fn spawn_call(
        &mut self,
        method: &'static str,
        params: Value,
        then: impl FnOnce(&mut Worker, Result<Value, RpcError>) + Send + 'static,
    ) {
        // Client.
        // RPC 传输客户端。
        let Some(client) = self.client.clone() else {
            // The continuation still runs, so whatever it releases (busy markers, kept drafts)
            // is released; nothing reached the service, so this is not a transport loss.
            // 后续回调照常执行，使其负责释放的状态（忙碌标记、保留的草稿）得到释放；
            // 请求未到达服务，因此不算传输丢失。
            then(
                self,
                Err(RpcError {
                    message: crate::tr!("tui.notice.notConnected"),
                    transport: false,
                }),
            );
            self.emit(BackendEvent::CallFinished);
            return;
        };
        // Results belong to the connection that admitted their requests.
        // 结果属于准入其请求的连接。
        let generation = self.generation;
        // Inbox.
        // 工作线程收件通道。
        let inbox = self.inbox.clone();
        thread::spawn(move || {
            // Result.
            // 服务结果。
            let result = client.call(method, params);
            let _ = inbox.send(Input::Done {
                generation,
                apply: Box::new(move |worker| then(worker, result)),
            });
        });
    }

    // ---- Service API calls ----

    /// Sends one `service_api` call as the WebUI does and routes its result by kind.
    /// 以 WebUI 的方式发送一次 `service_api` 调用，并按类型分发其结果。
    /// `call` is the exact request metadata for this result.
    /// `call` 为本结果的准确请求元数据。
    pub(super) fn service_call(&mut self, call: ServiceCall) {
        if call.kind == CallKind::Load(Slot::Workbench) {
            self.reload_workbench();
            return;
        }
        // Register at dispatch so an older response cannot become the current loader.
        // 在发送时登记，防止旧响应成为当前加载器。
        let stamp = if let CallKind::Load(slot) = call.kind {
            self.loaders.insert(slot, call.clone());
            // Sequence.
            // 单调序号。
            let sequence = self.load_sequences.entry(slot).or_default();
            *sequence += 1;
            Some((slot, *sequence))
        } else {
            None
        };
        // Operation.
        // 准确推理操作。
        let operation = !matches!(call.kind, CallKind::Load(_));
        self.spawn_call(call.method, call.params.clone(), move |worker, result| {
            if stamp
                .is_some_and(|(slot, sequence)| worker.load_sequences.get(&slot) != Some(&sequence))
            {
                return;
            }
            match result {
                Ok(value) => worker.call_done(call, value),
                Err(error) => {
                    if let CallKind::Load(slot) = call.kind {
                        // Owner-scoped errors outlive the toast but share the RPC's sanitization boundary.
                        // 归属限定错误比提示保留更久，但共用 RPC 脱敏边界。
                        worker.emit(BackendEvent::SlotFailed {
                            slot,
                            call,
                            message: error.message.clone(),
                        });
                    }
                    worker.failed(error);
                }
            }
            if operation {
                worker.emit(BackendEvent::CallFinished);
            }
        });
    }

    /// Folds a completed service request into slots, preview review or authoritative reloads.
    /// 将完成的服务请求折叠为槽位、预检审查或权威重读。
    /// `call` is the exact request metadata for this result.
    /// `call` 为本结果的准确请求元数据。
    /// `value` is the protocol value being read or projected.
    /// `value` 为正在读取或投影的协议值。
    fn call_done(&mut self, call: ServiceCall, mut value: Value) {
        scrub_secrets(&mut value);
        match call.kind.clone() {
            CallKind::Load(slot) => {
                self.emit(BackendEvent::Slot { slot, call, value });
            }
            CallKind::Mutate(slots) => {
                self.reload_slots(&slots);
                self.emit(BackendEvent::Notice(Notice::Completed(call.function)));
            }
            CallKind::Then(next) => match self.call(next.method, next.params.clone()) {
                Some(value) => self.call_done(*next, value),
                None => {
                    if let CallKind::Mutate(slots) = &next.kind {
                        self.reload_slots(slots);
                    }
                }
            },
            CallKind::Preview(_) => self.emit(BackendEvent::Preview {
                call,
                result: value,
            }),
        }
    }

    /// Rereads the slots a write invalidated with their latest loaders.
    /// 用最近的加载器重读写入使其失效的槽位。
    /// `slots` is the write's declared reload set.
    /// `slots` 为写入声明的重读集合。
    fn reload_slots(&mut self, slots: &[Slot]) {
        for slot in slots {
            match slot {
                // Mutation results are not workbench snapshots; read the revisioned one.
                // 变更结果不是工作台快照；读取带修订号的快照。
                Slot::Workbench => self.reload_workbench(),
                _ => {
                    if let Some(loader) = self.loaders.get(slot).map(ServiceCall::renewed) {
                        // Publish authoritative refreshes before unlocking another full-document edit.
                        // 解锁另一份完整文档编辑前发布权威刷新。
                        *self.load_sequences.entry(*slot).or_default() += 1;
                        self.loaders.insert(*slot, loader.clone());
                        // A failed refresh is reported as a slot failure, never as an empty load.
                        // 刷新失败按槽位失败上报，绝不当作空加载。
                        let Some(client) = self.client.clone() else {
                            self.emit(BackendEvent::Notice(Notice::NotConnected));
                            self.emit(BackendEvent::SlotFailed {
                                slot: *slot,
                                call: loader,
                                message: String::from("Not connected to the service."),
                            });
                            continue;
                        };
                        match client.call(loader.method, loader.params.clone()) {
                            Ok(value) => self.call_done(loader, value),
                            Err(error) => {
                                self.emit(BackendEvent::SlotFailed {
                                    slot: *slot,
                                    call: loader,
                                    message: error.message.clone(),
                                });
                                self.failed(error);
                            }
                        }
                    }
                }
            }
        }
    }

    /// Reloads the service workbench and emits changed UI projections.
    /// 重新加载服务工作台并发布变化的界面投影。
    fn reload_workbench(&mut self) {
        // Exact service call.
        // 准确服务调用。
        let call = load_workbench_snapshot();
        if let Some(snapshot) = self.call_as(call.method, call.params.clone()) {
            self.apply_snapshot(snapshot);
        } else {
            // Keep conversation projections, but never republish stale settings through a delta.
            // 保留会话投影，但绝不能通过增量重新发布过期设置。
            self.workbench_slot_valid = false;
            self.workbench_slot = Value::Null;
            self.emit(BackendEvent::Slot {
                slot: Slot::Workbench,
                call,
                value: Value::Null,
            });
        }
    }

    /// Emits the workbench slot when the fields system areas read have changed.
    /// 系统区域读取的字段变化时发出工作台槽位。
    pub(super) fn emit_workbench_slot(&mut self) {
        if !self.workbench_slot_valid {
            return;
        }
        // The typed snapshot serializes to the wire shape the system pages read.
        // 类型化快照序列化为系统页面读取的传输形状。
        let Some(Ok(Value::Object(mut whole))) = self.wb.as_ref().map(serde_json::to_value) else {
            return;
        };
        // Subset.
        // 页面所需工作台字段子集。
        let mut subset = Map::new();
        for key in WORKBENCH_SLOT_KEYS {
            if let Some(value) = whole.remove(key) {
                subset.insert(key.to_string(), value);
            }
        }
        // Value.
        // 当前协议值。
        let mut value = Value::Object(subset);
        scrub_secrets(&mut value);
        if value == self.workbench_slot {
            return;
        }
        self.workbench_slot = value.clone();
        self.emit(BackendEvent::Slot {
            slot: Slot::Workbench,
            call: load_workbench_snapshot(),
            value,
        });
    }

    // ---- Workbench projection ----

    /// Accepts an authoritative workbench snapshot and updates typed projections.
    /// 接受权威工作台快照并更新类型化投影。
    /// `snapshot` is the authoritative service snapshot.
    /// `snapshot` 为权威服务快照。
    fn apply_snapshot(&mut self, snapshot: api::WorkbenchSnapshot) {
        if snapshot.revision < self.revision {
            return;
        }
        self.revision = snapshot.revision;
        if self.selected.is_none() {
            self.selected = snapshot.selected_conversation_id.clone();
        }
        self.wb = Some(snapshot);
        self.workbench_slot_valid = true;
        let Some(wb) = self.wb.as_ref() else {
            return;
        };
        let (models, active) = map::models(&wb.providers);
        let selected_model_id = self
            .selected
            .as_ref()
            .and_then(|id| self.model_keys.get(id).cloned())
            .or(active)
            .unwrap_or_default();
        self.emit(BackendEvent::Workbench(WorkbenchSnapshot {
            projects: map::projects(&wb.projects),
            conversations: self.conversation_rows(),
            selected_conversation_id: self.selected.clone(),
            models,
            selected_model_id,
            ui_locale: wb.ui_locale.clone(),
            permission_judge_backend: wb.permission_judge_backend,
            base_url: self
                .client
                .as_ref()
                .map(|c| c.base_url().to_string())
                .unwrap_or_default(),
        }));
        self.emit(BackendEvent::CommandSessions(map::command_sessions(
            &wb.command_sessions,
        )));
        let mut ids: Vec<String> = wb
            .pending_questions
            .iter()
            .flat_map(question_targets)
            .collect();
        ids.extend(self.selected.clone());
        ids.sort();
        ids.dedup();
        for id in ids {
            self.emit_question(&id);
        }
        if let Some(id) = self.selected.clone() {
            self.emit_conversation_state(&id);
        }
        self.emit_effort();
        self.emit_workbench_slot();
    }

    /// Sidebar rows: service conversations with pending-question attention, then local drafts.
    /// 侧栏行：带等待提问标记的服务会话，以及本地草稿。
    /// Floating assistants live in the WebUI top bar and stay out of the session list.
    /// 浮动助手位于 WebUI 顶部栏，不进入会话列表。
    fn conversation_rows(&self) -> Vec<ConversationSummary> {
        let mut rows: Vec<ConversationSummary> = self
            .wb
            .iter()
            .flat_map(|wb| &wb.conversations)
            .filter(|conversation| conversation.assistant_name.is_none())
            .map(map::conversation)
            .collect();
        for row in &mut rows {
            row.attention |= self.question_of(&row.id).is_some();
        }
        for (id, draft) in &self.drafts {
            rows.insert(0, draft_row(id, draft));
        }
        rows
    }

    /// Finds the typed summary of the exact conversation identity.
    /// 查找准确会话身份的类型化摘要。
    /// `id` is the exact entity identity.
    /// `id` 为准确实体身份。
    fn summary(&self, id: &str) -> Option<ConversationSummary> {
        let mut row = self
            .wb
            .as_ref()?
            .conversations
            .iter()
            .find(|conversation| conversation.id == id)
            .map(map::conversation)?;
        row.attention |= self.question_of(id).is_some();
        Some(row)
    }

    /// Finds the pending question shown in the exact conversation: its own first, then one a child
    /// agent raised. A child agent has no conversation here, so its request surfaces in the root
    /// conversation the service names; otherwise it would wait for an approval nobody can see.
    /// 查找准确会话中显示的待回复提问：先取会话自身的，再取子代理发起的。子代理在此没有自己的会话，
    /// 其请求在服务指明的根会话中显示；否则子代理会等待无人可见的审批。
    /// `id` is the exact entity identity.
    /// `id` 为准确实体身份。
    fn question_of(&self, id: &str) -> Option<&api::PendingQuestionSnapshot> {
        let questions = &self.wb.as_ref()?.pending_questions;
        questions
            .iter()
            .find(|question| question.session_id == id)
            .or_else(|| {
                questions
                    .iter()
                    .find(|question| question.owner_conversation_id.as_deref() == Some(id))
            })
    }

    /// Session add-ons of the exact conversation.
    /// 准确会话的会话附加状态。
    fn addons(&self, id: &str) -> Option<&api::SessionAddonsChangedDelta> {
        self.wb
            .as_ref()?
            .session_addons_by_conversation
            .iter()
            .find(|entry| entry.conversation_id == id)
    }

    /// Context injection of the exact conversation.
    /// 准确会话的上下文注入。
    fn injection(&self, id: &str) -> Option<&api::ContextInjectionSnapshot> {
        self.wb
            .as_ref()?
            .context_injection_by_conversation
            .iter()
            .find(|entry| entry.conversation_id == id)?
            .context_injection
            .as_ref()
    }

    /// Publishes the exact current pending question for a conversation.
    /// 发布会话当前准确的待回复提问。
    /// `id` is the exact entity identity.
    /// `id` 为准确实体身份。
    fn emit_question(&self, id: &str) {
        let question = self.question_of(id).map(|question| {
            let requester = if question.session_id == id {
                None
            } else {
                question.requester_label.clone()
            };
            map::pending_question(question, requester)
        });
        self.emit(BackendEvent::PendingQuestion {
            conversation_id: id.to_string(),
            question,
        });
    }

    /// Everything the workbench knows about one conversation besides its events.
    /// 工作台所知的单个会话除事件外的全部状态。
    /// `id` is the exact entity identity.
    /// `id` 为准确实体身份。
    fn emit_conversation_state(&self, id: &str) {
        let conversation_id = id.to_string();
        let prompts = self
            .wb
            .iter()
            .flat_map(|wb| &wb.pending_prompts_by_conversation)
            .find(|entry| entry.conversation_id == id)
            .map(|entry| map::pending_prompts(&entry.pending_prompts))
            .unwrap_or_default();
        self.emit(BackendEvent::PendingPrompts {
            conversation_id: conversation_id.clone(),
            prompts,
        });
        self.emit_addons(id);
        self.emit_agents(id);
        if let Some(usage) = self.selected_usage(id) {
            self.emit(BackendEvent::ContextUsage {
                conversation_id,
                usage: map::context_usage(usage),
            });
        }
    }

    /// Context usage, which the workbench only publishes for the service's own selection.
    /// 上下文用量；工作台只为服务自身的选中会话发布它。
    /// `id` is the exact entity identity.
    /// `id` 为准确实体身份。
    fn selected_usage(&self, id: &str) -> Option<&api::ContextUsageSnapshot> {
        let wb = self.wb.as_ref()?;
        if wb.selected_conversation_id.as_deref() != Some(id) {
            return None;
        }
        wb.context_usage.as_ref()
    }

    /// Publishes the current conversation planning add-ons and panel insight.
    /// 发布当前会话的规划附加信息与面板洞察。
    /// `id` is the exact entity identity.
    /// `id` 为准确实体身份。
    fn emit_addons(&self, id: &str) {
        let addons = self.addons(id);
        let conversation_id = id.to_string();
        self.emit(BackendEvent::Todo {
            conversation_id: conversation_id.clone(),
            items: addons
                .and_then(|a| a.session_todolist.as_ref())
                .map(map::todo)
                .unwrap_or_default(),
        });
        self.emit(BackendEvent::PlanMode {
            conversation_id,
            mode: addons
                .and_then(|a| a.session_plan_mode.as_ref())
                .map_or(ExecutionMode::Build, map::plan_mode),
        });
        self.emit_insight(id);
    }

    /// Projects current conversation context, memory and usage into the session panel.
    /// 将当前会话上下文、记忆及用量投影到会话面板。
    /// `id` is the exact entity identity.
    /// `id` 为准确实体身份。
    fn emit_insight(&self, id: &str) {
        let addons = self.addons(id);
        let mut lifetime = addons
            .and_then(|a| a.session_provider_usage.as_deref())
            .map(map::lifetime)
            .unwrap_or_default();
        // Child usage is keyed by child session id; show the subagent label where known.
        // 子项用量以子会话 ID 为键；已知时显示子代理标签。
        if let Some(navigation) = self.navigation(id) {
            for (key, _) in &mut lifetime.children {
                if let Some(task) = navigation.tasks.iter().find(|t| t.task_id == *key) {
                    *key = task.label.clone();
                }
            }
        }
        let insight = SessionInsight {
            memory: self
                .injection(id)
                .map(map::context_injection)
                .unwrap_or_default(),
            lifetime,
            breakdown: self
                .selected_usage(id)
                .map(map::breakdown)
                .unwrap_or_default(),
            goal: addons.and_then(|a| a.session_goal.as_ref()).map(map::goal),
        };
        self.emit(BackendEvent::SessionInsight {
            conversation_id: id.to_string(),
            insight: Box::new(insight),
        });
    }

    /// Subagent navigation of the exact parent conversation: the latest navigation delta for it,
    /// else the workbench navigation when it belongs to that parent.
    /// 准确父会话的子代理导航：该会话最近一次的导航增量，否则为属于该父会话的工作台导航。
    /// `id` is the exact entity identity.
    /// `id` 为准确实体身份。
    fn navigation(&self, id: &str) -> Option<&api::SubagentNavigationSnapshot> {
        self.navigations.get(id).or_else(|| {
            self.wb
                .as_ref()?
                .subagent_navigation
                .as_ref()
                .filter(|navigation| navigation.parent_conversation_id == id)
        })
    }

    /// Publishes the selected parent conversation subagent list.
    /// 发布所选父会话的子代理列表。
    /// `id` is the exact entity identity.
    /// `id` 为准确实体身份。
    fn emit_agents(&self, id: &str) {
        self.emit(BackendEvent::Agents {
            conversation_id: id.to_string(),
            agents: self.navigation(id).map(map::agents).unwrap_or_default(),
        });
    }

    /// Records the navigation of one parent conversation; `None` clears it.
    /// 记录一个父会话的导航；`None` 表示清除。
    fn set_navigation(
        &mut self,
        parent: &str,
        navigation: Option<api::SubagentNavigationSnapshot>,
    ) {
        match navigation {
            Some(navigation) => {
                self.navigations.insert(parent.to_string(), navigation);
            }
            None => {
                self.navigations.remove(parent);
            }
        }
        // The workbench copy is older than the delta for this parent.
        // 对该父会话而言，工作台副本早于增量。
        if let Some(wb) = self.wb.as_mut()
            && wb
                .subagent_navigation
                .as_ref()
                .is_some_and(|navigation| navigation.parent_conversation_id == parent)
        {
            wb.subagent_navigation = None;
        }
        self.emit_agents(parent);
        self.emit_insight(parent);
    }

    /// Reasoning effort choices of the service-selected conversation's model.
    /// 服务选中会话所用模型的推理强度选项。
    fn emit_effort(&self) {
        let Some(id) = self.selected.clone() else {
            return;
        };
        let model_id = self.model_keys.get(&id).cloned().unwrap_or_default();
        let selection = self
            .wb
            .as_ref()
            .and_then(|wb| wb.reasoning_selection.as_ref());
        let mut efforts: Vec<ReasoningEffort> = selection
            .iter()
            .flat_map(|selection| &selection.options)
            .filter_map(|option| map::effort(&option.value))
            .collect();
        efforts.dedup();
        let effort = selection.and_then(|selection| map::effort(&selection.selected_effort));
        self.emit(BackendEvent::ModelSelection {
            model_id,
            effort,
            efforts,
        });
    }

    /// Workbench value for an effort, picked from the current model's own options.
    /// 推理强度对应的工作台取值，从当前模型自身的选项中挑选。
    fn effort_value(&self, effort: ReasoningEffort) -> Option<String> {
        self.wb
            .as_ref()?
            .reasoning_selection
            .as_ref()?
            .options
            .iter()
            .map(|option| option.value.clone())
            .find(|value| map::effort(value) == Some(effort))
    }

    /// Replaces the add-on row of one conversation; a row without add-ons is removed.
    /// 替换一个会话的附加状态行；没有附加状态的行被移除。
    fn upsert_addons(&mut self, entry: api::SessionAddonsChangedDelta) {
        let id = entry.conversation_id.clone();
        let present = entry.session_goal.is_some()
            || entry.session_todolist.is_some()
            || entry.session_plan_mode.is_some()
            || entry.session_output_style.is_some()
            || entry.session_context_window_selection.is_some()
            || entry.session_provider_usage.is_some();
        if let Some(wb) = self.wb.as_mut() {
            wb.session_addons_by_conversation
                .retain(|row| row.conversation_id != id);
            if present {
                wb.session_addons_by_conversation.push(entry);
            }
        }
        self.emit_addons(&id);
    }

    /// Folds an add-on RPC response in when it is not older than the workbench.
    /// 附加状态 RPC 响应不早于工作台时将其折叠进来。
    fn absorb_addons(&mut self, response: api::SessionAddonsChangedResponse) {
        // A receipt for one session does not advance the delta order: as in the WebUI, only
        // snapshots and deltas do, so other sessions' pending deltas are not dropped.
        // 单个会话的回执不推进增量顺序：与 WebUI 一致，只有快照与增量推进，
        // 因此其他会话待到达的增量不会被丢弃。
        if response.revision < self.revision {
            return;
        }
        self.upsert_addons(api::SessionAddonsChangedDelta {
            conversation_id: response.conversation_id,
            session_goal: response.session_goal,
            session_todolist: response.session_todolist,
            session_plan_mode: response.session_plan_mode,
            session_vmm_policy: response.session_vmm_policy,
            session_output_style: response.session_output_style,
            session_context_window_selection: response.session_context_window_selection,
            session_provider_usage: response.session_provider_usage.map(Box::new),
        });
    }

    /// Folds a subagent route response in when it is not older than the workbench.
    /// 子代理路由响应不早于工作台时将其折叠进来。
    fn absorb_route(&mut self, response: api::SubagentPanelRouteResponse) {
        // A receipt for one session does not advance the delta order: as in the WebUI, only
        // snapshots and deltas do, so other sessions' pending deltas are not dropped.
        // 单个会话的回执不推进增量顺序：与 WebUI 一致，只有快照与增量推进，
        // 因此其他会话待到达的增量不会被丢弃。
        if response.revision < self.revision {
            return;
        }
        let parent = response.parent_conversation_id.or_else(|| {
            response
                .subagent_navigation
                .as_ref()
                .map(|navigation| navigation.parent_conversation_id.clone())
        });
        if let Some(parent) = parent {
            self.set_navigation(&parent, response.subagent_navigation);
        }
    }

    /// Applies one field update to an existing conversation row and publishes the row.
    /// 对已有会话行应用一次字段更新并发布该行。
    /// `id` is the exact entity identity.
    /// `id` 为准确实体身份。
    fn patch_conversation(&mut self, id: &str, patch: impl FnOnce(&mut api::ConversationSummary)) {
        let Some(row) = self
            .wb
            .as_mut()
            .and_then(|wb| wb.conversations.iter_mut().find(|c| c.id == id))
        else {
            return;
        };
        patch(row);
        if let Some(row) = self.summary(id) {
            self.emit(BackendEvent::ConversationSummary(row));
        }
    }

    /// Removes exact conversation identities and their cached state.
    /// 移除准确会话身份及其缓存状态。
    /// `ids` is the exact selected identity set.
    /// `ids` 为准确的所选身份集合。
    fn remove_conversations(&mut self, ids: &[String]) {
        // Only rows still listed are announced, so overlapping deltas remove each row once.
        // 只公告仍在列表中的行，使重叠的增量对每行只移除一次。
        let mut removed = Vec::new();
        if let Some(wb) = self.wb.as_mut() {
            wb.conversations.retain(|c| {
                let keep = !ids.contains(&c.id);
                if !keep {
                    removed.push(c.id.clone());
                }
                keep
            });
        }
        for id in &removed {
            self.windows.remove(id);
            self.navigations.remove(id);
            if self.selected.as_ref() == Some(id) {
                self.selected = None;
            }
            self.emit(BackendEvent::ConversationRemoved(id.clone()));
        }
    }

    /// Moves archived conversations from the live list into the archived list.
    /// 将已归档会话从活动列表移入归档列表。
    /// `archived` is the exact archive delta list.
    /// `archived` 为准确的归档增量列表。
    fn archive(&mut self, archived: Vec<api::ConversationArchivedDelta>) {
        let ids: Vec<String> = archived
            .iter()
            .map(|entry| entry.conversation_id.clone())
            .collect();
        self.remove_conversations(&ids);
        if let Some(wb) = self.wb.as_mut() {
            for summary in archived
                .into_iter()
                .filter_map(|entry| entry.archived_conversation)
            {
                wb.archived_conversations.retain(|row| row.id != summary.id);
                wb.archived_conversations.insert(0, summary);
            }
        }
        self.emit_workbench_slot();
    }

    // ---- Service events ----

    /// Dispatches one service event to its protocol-defined state handler.
    /// 将一个服务事件分发到其协议规定的状态处理器。
    /// `event` is the strictly parsed service event.
    /// `event` 为严格解析的服务事件。
    fn service_event(&mut self, event: api::ServiceEvent) {
        match event {
            api::ServiceEvent::Workbench(snapshot) => self.apply_snapshot(*snapshot),
            api::ServiceEvent::WorkbenchDelta(envelope) => self.workbench_delta(*envelope),
            api::ServiceEvent::ConversationEventDeltaV1(delta) => self.event_delta(*delta),
            api::ServiceEvent::WorkspaceGitChanged(change) => {
                self.git_changed(&change.workspace_path, &change.repository_keys);
            }
            api::ServiceEvent::SessionTitleGenerated(generated) => {
                self.session_title_generated(&generated);
            }
            api::ServiceEvent::AppUpdateChanged(snapshot) => self.app_update_changed(&snapshot),
            api::ServiceEvent::ShellJobChanged(changed) => {
                self.emit(BackendEvent::ShellJobChanged(*changed));
            }
            api::ServiceEvent::ServiceRestarting(restarting) => self.toast(
                NoticeSeverity::Info,
                match restarting.reason {
                    api::ServiceRestartReason::Update => crate::tr!("appUpdate.toast.restarting"),
                    api::ServiceRestartReason::Installer => {
                        crate::tr!("appUpdate.toast.installing")
                    }
                },
            ),

            _ => {}
        }
    }

    /// Tells the user the generated session title, or which model failed to write one.
    /// 告知用户生成的会话标题，或哪个模型未能生成标题。
    /// `generated` is the outcome of one title request.
    /// `generated` 为一次标题请求的结果。
    fn session_title_generated(&self, generated: &api::SessionTitleGeneratedEvent) {
        match &generated.outcome {
            api::SessionTitleGenerationOutcome::Renamed { title } => self.toast(
                NoticeSeverity::Info,
                crate::tr!("sessionTitle.toast.renamed", title = title),
            ),
            api::SessionTitleGenerationOutcome::Failed => self.toast(
                NoticeSeverity::Warning,
                crate::tr!("sessionTitle.toast.failed", model = generated.model),
            ),
        }
    }

    /// Publishes a changed update snapshot and announces a newer or ready version once.
    /// 发布变化的更新快照，并对更新版本或已就绪版本各提示一次。
    /// `snapshot` is the updater state the service just published.
    /// `snapshot` 为服务刚发布的更新器状态。
    fn app_update_changed(&mut self, snapshot: &api::AppUpdateSnapshot) {
        if let Some(offer) = &snapshot.offer
            && self
                .app_update_prompted
                .insert(format!("offer:{}", offer.version))
        {
            self.toast(
                NoticeSeverity::Info,
                crate::tr!("settings.appUpdate.available", version = offer.version),
            );
        }
        if let Some(ready) = &snapshot.ready_version
            && self.app_update_prompted.insert(format!("ready:{ready}"))
        {
            self.toast(
                NoticeSeverity::Info,
                crate::tr!("settings.appUpdate.ready", version = ready),
            );
        }
        if let Ok(value) = serde_json::to_value(snapshot) {
            self.emit(BackendEvent::Slot {
                slot: Slot::AppUpdate,
                call: crate::service_api::app_update::load_app_update_status(),
                value,
            });
        }
    }

    /// Folds revision-checked workbench deltas.
    /// 折叠检查修订后的工作台增量。
    /// `envelope` is the service revision/delta envelope.
    /// `envelope` 为服务修订或增量信封。
    fn workbench_delta(&mut self, envelope: api::WorkbenchDeltaEnvelope) {
        use api::WorkbenchDelta as Delta;
        // Lifecycle and catalog deltas are not ordered against conversation revisions.
        // 生命周期与目录增量不按会话修订号排序。
        let independent = matches!(
            envelope.delta,
            Delta::ConversationArchived(_)
                | Delta::ConversationsArchived(_)
                | Delta::ProjectRemoved(_)
                | Delta::AgentSkillsCatalogChanged(_)
                | Delta::LuaSkillsCatalogChanged(_)
                | Delta::SubagentCatalogChanged(_)
                | Delta::PluginCatalogChanged(_)
                | Delta::ProjectSettingsChanged(_)
                | Delta::LuaSkillsManagedRuntimeProgressChanged(_)
                | Delta::LuaSkillsOperationProgressChanged(_)
        );
        if !independent && envelope.generation <= self.revision {
            return;
        }
        self.revision = self.revision.max(envelope.generation);
        if self.wb.is_none() {
            return;
        }
        match envelope.delta {
            Delta::UpdateConversationRunPhase(delta) => {
                self.patch_conversation(&delta.conversation_id, |row| {
                    row.run_phase = delta.run_phase;
                    if let Some(status) = delta.status {
                        row.status = status;
                    }
                    if let Some(label) = delta.last_updated_label {
                        // The kernel stamps the update time together with the label.
                        // 内核在写入标签的同时记录更新时间。
                        row.last_updated_label = label;
                        row.last_updated_at = crate::model::now_unix_ms().to_string();
                    }
                });
            }
            Delta::PendingPromptsChanged(delta) => {
                let prompts = map::pending_prompts(&delta.pending_prompts);
                let conversation_id = delta.conversation_id.clone();
                if let Some(wb) = self.wb.as_mut() {
                    wb.pending_prompts_by_conversation
                        .retain(|entry| entry.conversation_id != conversation_id);
                    wb.pending_prompts_by_conversation.push(delta);
                }
                self.emit(BackendEvent::PendingPrompts {
                    conversation_id,
                    prompts,
                });
            }
            Delta::ConversationSummaryChanged(delta) => {
                let id = delta.conversation.id.clone();
                if let Some(wb) = self.wb.as_mut() {
                    match wb.conversations.iter_mut().find(|c| c.id == id) {
                        Some(row) => *row = delta.conversation,
                        None => wb.conversations.insert(0, delta.conversation),
                    }
                }
                if let Some(row) = self.summary(&id) {
                    self.emit(BackendEvent::ConversationSummary(row));
                }
            }
            Delta::ConversationArchived(delta) => self.archive(vec![delta]),
            Delta::ConversationsArchived(delta) => self.archive(delta.archived_conversations),
            Delta::ProjectRemoved(delta) => {
                let ids: Vec<String> = self
                    .wb
                    .iter()
                    .flat_map(|wb| &wb.conversations)
                    .filter(|c| c.project_id.as_deref() == Some(delta.project_id.as_str()))
                    .map(|c| c.id.clone())
                    .collect();
                self.remove_conversations(&ids);
                self.emit(BackendEvent::Projects(map::projects(&delta.projects)));
                if let Some(wb) = self.wb.as_mut() {
                    wb.projects = delta.projects;
                    wb.expanded_project_ids = delta.expanded_project_ids;
                }
                self.archive(delta.archived_conversations);
            }
            Delta::PendingQuestionChanged(delta) => {
                // The delta always carries the full pending list (`pendingQuestions`), which the
                // WebUI also takes as authoritative.
                // 增量始终携带完整的待回复列表（`pendingQuestions`），WebUI 同样以其为准。
                let Some(wb) = self.wb.as_mut() else {
                    return;
                };
                let mut touched: Vec<String> = wb
                    .pending_questions
                    .iter()
                    .chain(&delta.pending_questions)
                    .flat_map(question_targets)
                    .chain(delta.conversation_id)
                    .collect();
                wb.pending_questions = delta.pending_questions;
                touched.sort();
                touched.dedup();
                for id in touched {
                    self.emit_question(&id);
                    if let Some(row) = self.summary(&id) {
                        self.emit(BackendEvent::ConversationSummary(row));
                    }
                }
            }
            Delta::CommandSessionsChanged(delta) => {
                self.emit(BackendEvent::CommandSessions(map::command_sessions(
                    &delta.command_sessions,
                )));
                if let Some(wb) = self.wb.as_mut() {
                    wb.command_sessions = delta.command_sessions;
                }
            }
            Delta::SubagentNavigationChanged(delta) => {
                let parent = delta.parent_conversation_id.clone().or_else(|| {
                    delta
                        .subagent_navigation
                        .as_ref()
                        .map(|navigation| navigation.parent_conversation_id.clone())
                });
                if let Some(parent) = parent {
                    self.set_navigation(&parent, delta.subagent_navigation);
                }
            }
            Delta::ContextUsageChanged(delta) => {
                let selected = self.wb.as_mut().and_then(|wb| {
                    wb.context_usage = delta.context_usage;
                    wb.selected_conversation_id.clone()
                });
                if let Some(id) = selected {
                    if let Some(usage) = self.selected_usage(&id) {
                        self.emit(BackendEvent::ContextUsage {
                            conversation_id: id.clone(),
                            usage: map::context_usage(usage),
                        });
                    }
                    self.emit_insight(&id);
                }
            }
            Delta::ContextInjectionChanged(delta) => {
                let id = delta.conversation_id.clone();
                if let Some(wb) = self.wb.as_mut() {
                    wb.context_injection_by_conversation
                        .retain(|entry| entry.conversation_id != id);
                    if delta.context_injection.is_some() {
                        wb.context_injection_by_conversation.push(delta);
                    }
                }
                self.emit_insight(&id);
            }
            Delta::SessionAddonsChanged(delta) => self.upsert_addons(*delta),
            Delta::AgentSkillsCatalogChanged(delta) => {
                if let Some(wb) = self.wb.as_mut() {
                    wb.agent_skills = delta.agent_skills;
                }
                self.emit_workbench_slot();
            }
            Delta::LuaSkillsCatalogChanged(delta) => {
                if let Some(wb) = self.wb.as_mut() {
                    wb.lua_skills = delta.lua_skills;
                    wb.plugins = delta.plugins;
                }
                self.emit_workbench_slot();
            }
            Delta::SubagentCatalogChanged(delta) => {
                if let Some(wb) = self.wb.as_mut() {
                    wb.subagent_catalog = delta.subagent_catalog;
                }
                self.emit_workbench_slot();
            }
            Delta::PluginCatalogChanged(delta) => {
                if let Some(wb) = self.wb.as_mut() {
                    wb.lua_skills = delta.lua_skills;
                    wb.plugins = delta.plugins;
                    wb.agent_skills = delta.agent_skills;
                    wb.subagent_catalog = delta.subagent_catalog;
                }
                self.emit_workbench_slot();
            }
            _ => {}
        }
    }

    // ---- Conversation events ----

    /// Loads the latest event window of one conversation.
    /// 加载单个会话的最新事件窗口。
    /// `id` is the exact entity identity.
    /// `id` 为准确实体身份。
    pub(super) fn hydrate(&mut self, id: &str) {
        let Some(window) = self.call_as::<api::ConversationEventWindowV1Response>(
            "service_session_event_hydration_v1",
            json!({ "sessionId": id }),
        ) else {
            return;
        };
        let (epoch, memory_revision) = window
            .revision_frontier
            .as_ref()
            .map_or((String::new(), 0), |frontier| {
                (frontier.service_epoch.clone(), frontier.memory_revision)
            });
        let events: BTreeMap<u64, ConversationEventSummary> = window
            .batch
            .events
            .into_iter()
            .map(|event| (event.seq.value(), event))
            .collect();
        self.emit(BackendEvent::ConversationHydrated {
            conversation_id: id.to_string(),
            events: events.values().filter_map(map::event).collect(),
        });
        self.windows.insert(
            id.to_string(),
            Window {
                answer_mode: AnswerModeSession {
                    mode: Some(window.session_config.answer_mode.clone()),
                },
                permission_mode: window.session_config.permission_mode,
                unattended: window.session_config.unattended.clone(),
                generation: window.batch.window_generation,
                epoch,
                memory_revision,
                events,
            },
        );
        self.emit_answer_mode(id);
        self.emit_permission_mode(id);
        // The next request's route wins over the current one once the user changed it.
        // 用户改过之后，下一次请求的路线优先于当前请求。
        let config = window.session_config;
        let request = config.next_request.unwrap_or(config.current_request);
        if let Some(key) = map::session_route(&request) {
            self.model_keys.insert(id.to_string(), key);
        }
        if self.selected.as_deref() == Some(id) {
            self.emit_effort();
        }
    }

    /// Applies one live event delta; any inconsistency reloads the window.
    /// 应用一份实时事件增量；任何不一致都会重新加载窗口。
    /// `patch` is the exact delta to fold.
    /// `patch` 为需要折叠的准确增量。
    fn event_delta(&mut self, patch: api::ConversationEventLiveDeltaV1) {
        let id = patch.batch.session_id.clone();
        let Some(window) = self.windows.get_mut(&id) else {
            return;
        };
        let stale_owner = patch.batch.window_generation != window.generation
            || patch.service_epoch != window.epoch;
        let gap = patch.revision > window.memory_revision
            && patch.base_memory_revision.unwrap_or(0) > window.memory_revision;
        if stale_owner || gap {
            self.hydrate(&id);
            return;
        }
        if patch.durability == api::ConversationEventPatchDurabilityV1::Durable {
            if patch.revision > window.memory_revision {
                self.hydrate(&id);
                return;
            }
            if let Some(mode) = patch.answer_mode {
                window.answer_mode.accept_mode(*mode);
            }
            self.emit_answer_mode(&id);
            return;
        }
        if patch.revision < window.memory_revision {
            return;
        }
        window.memory_revision = patch.revision;
        let earliest = window.events.keys().next().copied();
        let mut changed: Vec<u64> = Vec::new();
        let mut broken = false;
        for summary in patch.batch.events {
            let seq = summary.seq.value();
            if earliest.is_some_and(|first| seq < first) && !window.events.contains_key(&seq) {
                continue;
            }
            let newer = window
                .events
                .get(&seq)
                .is_none_or(|existing| summary.event_revision >= existing.event_revision);
            if newer {
                window.events.insert(seq, summary);
                changed.push(seq);
            }
        }
        for append in patch.text_appends {
            let seq = append.seq.value();
            let Some(existing) = window.events.get_mut(&seq) else {
                if earliest.is_none_or(|first| seq >= first) {
                    broken = true;
                }
                continue;
            };
            if append.event_revision <= existing.event_revision {
                continue;
            }
            // Only streamed reasoning and answer text receive appends.
            // 只有流式思考与回答文本会收到追加。
            let text = match &mut existing.event {
                CoreEvent::AssistantText(event) => &mut event.text,
                CoreEvent::AssistantReasoning(event) => &mut event.text,
                _ => {
                    broken = true;
                    continue;
                }
            };
            let base = utf16_prefix(text, append.base_text_utf16_len as usize).len();
            text.truncate(base);
            text.push_str(&append.appended_text);
            existing.event_revision = append.event_revision;
            existing.updated_at_ms = append.updated_at_ms;
            changed.push(seq);
        }
        if broken {
            self.hydrate(&id);
            return;
        }
        changed.sort_unstable();
        changed.dedup();
        let events: Vec<ConversationEvent> = changed
            .iter()
            .filter_map(|seq| window.events.get(seq))
            .filter_map(map::event)
            .collect();
        for event in events {
            self.emit(BackendEvent::EventUpsert {
                conversation_id: id.clone(),
                event,
            });
        }
    }

    // ---- Commands ----

    /// Dispatches the UI command through its declared WebUI RPC or local-file operation.
    /// 通过已声明 WebUI RPC 或本地文件操作分发界面命令。
    /// `command` is the UI command to execute.
    /// `command` 为需要执行的界面命令。
    fn handle(&mut self, command: Command) {
        match command {
            Command::SetSessionAnswerMode(request) => self.set_answer_mode(request),
            Command::SetSessionPermissionMode(request) => self.set_permission_mode(request),
            Command::SetSessionUnattended(request) => self.set_unattended(request),
            Command::LoadSessionUnattendedDefault(request) => self.load_unattended_default(request),
            Command::SelectConversation(id) => self.select(id),
            Command::NewConversation { project_id } => {
                self.draft_serial += 1;
                // Exact entity identity.
                // 准确实体身份。
                let id = format!(
                    "{DRAFT_PREFIX}{}:{}",
                    project_id.as_deref().unwrap_or("workspace"),
                    self.draft_serial
                );
                // Draft.
                // 本地待提交会话草稿。
                let draft = Draft {
                    answer_mode: self.default_answer_mode(),
                    permission_mode: None,
                    model_key: self
                        .selected
                        .as_ref()
                        .and_then(|s| self.model_keys.get(s).cloned()),
                    project_id,
                };
                self.emit(BackendEvent::ConversationSummary(draft_row(&id, &draft)));
                self.drafts.insert(id.clone(), draft);
                self.emit_answer_mode(&id);
                self.emit_permission_mode(&id);
                self.emit(BackendEvent::Selected(id));
            }
            Command::SubmitConversationPrompt {
                conversation_id,
                prompt,
                mode,
                effort,
                model_id,
                attachments,
            } => self.submit(conversation_id, prompt, mode, effort, model_id, attachments),
            Command::StopConversationRun { conversation_id } => {
                // The run state arrives through workbench deltas.
                // 运行状态经由工作台增量到达。
                let _ = self.call(
                    "service_stop_conversation_run",
                    json!({ "conversationId": conversation_id, "mode": "main_and_subagents" }),
                );
            }
            Command::ReplyPendingQuestion {
                request_id,
                answers,
            } => {
                // The pending list arrives through `pendingQuestionChanged`.
                // 待回复列表经由 `pendingQuestionChanged` 到达。
                let _ = self.call_as::<bool>(
                    "service_reply_pending_question",
                    json!({ "requestId": request_id, "answers": answers }),
                );
            }
            Command::RejectPendingQuestion { request_id } => {
                let _ = self.call_as::<bool>(
                    "service_reject_pending_question",
                    json!({ "requestId": request_id }),
                );
            }
            Command::DeclineEnvironmentValues { request_id } => {
                let _ = self.call_as::<bool>(
                    "service_submit_environment_values",
                    json!({ "request": { "requestId": request_id, "declined": true, "values": [] } }),
                );
            }
            Command::SetSessionPlanMode {
                conversation_id,
                enabled,
            } => {
                if is_draft(&conversation_id) {
                    return;
                }
                if let Some(response) = self.call_as(
                    "service_set_session_plan_mode",
                    json!({ "request": { "conversationId": conversation_id, "enabled": enabled } }),
                ) {
                    self.absorb_addons(response);
                }
            }
            Command::SelectPublicAiModel {
                conversation_id,
                model_id,
            } => self.select_model(conversation_id, model_id),
            Command::SelectWorkbenchReasoningEffort {
                conversation_id,
                effort,
            } => {
                // Value.
                // 当前协议值。
                let Some(value) = self.effort_value(effort) else {
                    return;
                };
                // Exact entity identity.
                // 准确实体身份。
                let Some(id) = conversation_id.filter(|id| !is_draft(id)) else {
                    return;
                };
                if let Some(snapshot) = self.call_as(
                    "service_select_workbench_reasoning_effort",
                    json!({ "conversationId": id, "effort": value }),
                ) {
                    self.apply_snapshot(snapshot);
                }
            }
            Command::LoadWorkspaceDiffSnapshot => self.git_refresh(),
            Command::SelectGitRepository(key) => self.git_select_repository(key),
            Command::SetGitMonitoring {
                repository_key,
                enabled,
            } => self.git_set_monitoring(repository_key, enabled),
            Command::RunWorkspaceGitFileAction { paths, stage } => self.git_stage(paths, stage),
            Command::LoadWorkspaceDiffFileDetail { key } => self.git_diff(key),
            Command::LoadWorkspaceGitBranches => self.git_branches(),
            Command::SwitchWorkspaceGitBranch { name, create } => self.git_switch(name, create),
            Command::GenerateGitCommitMessage {
                request,
                include_unstaged,
            } => self.git_generate_message(request, include_unstaged),
            Command::RunGitCommitAction {
                message,
                new_branch,
                include_unstaged,
                push,
            } => self.git_commit(message, new_branch, include_unstaged, push),
            Command::RunGitPullAction => self.git_pull(),
            // The attachment picker reads the local disk: the service runs on this machine and
            // attachments are staged from local bytes.
            // 附件选择器读取本地磁盘：服务运行在本机，附件以本地字节暂存。
            Command::ListLocalFiles { path } => self.emit(BackendEvent::Directory(
                super::files::list(path.as_deref(), true, true),
            )),
            Command::BrowseProjectDirectories { path } => {
                if let Some(listing) = self.call_as(
                    "service_browse_project_directories",
                    json!({ "path": path }),
                ) {
                    self.emit(BackendEvent::Directory(map::directory(&listing)));
                }
            }
            Command::AddProjectFromPath { path } => {
                if let Some(snapshot) = self.call_as::<Option<api::WorkbenchSnapshot>>(
                    "service_add_project_from_path",
                    json!({ "path": path }),
                ) {
                    if let Some(snapshot) = snapshot {
                        self.apply_snapshot(snapshot);
                    }
                    // Name.
                    // 当前名称。
                    let name = self
                        .wb
                        .iter()
                        .flat_map(|wb| &wb.projects)
                        .find(|p| same_path(&p.path, &path))
                        .map(|p| p.name.clone())
                        .unwrap_or(path);
                    self.emit(BackendEvent::Notice(Notice::ProjectAdded(name)));
                }
            }
            Command::RenameProject { project_id, name } => {
                if let Some(snapshot) = self.call_as(
                    "service_rename_project",
                    json!({ "projectId": project_id, "name": name }),
                ) {
                    self.apply_snapshot(snapshot);
                    self.emit(BackendEvent::Notice(Notice::Renamed));
                }
            }
            Command::ToggleProjectPinned { project_id } => {
                // The toggle acknowledges no target value; as in the WebUI, only a revisioned
                // read establishes its outcome.
                // 切换不返回目标值；与 WebUI 一致，仅带修订号的读取能确定其结果。
                if self
                    .call_as::<()>(
                        "service_toggle_project_pinned",
                        json!({ "projectId": project_id }),
                    )
                    .is_some()
                {
                    self.reload_workbench();
                }
            }
            Command::ArchiveProjectConversations { project_id } => {
                // Count.
                // 当前计数。
                let count = self
                    .wb
                    .iter()
                    .flat_map(|wb| &wb.conversations)
                    .filter(|c| c.project_id.as_deref() == Some(project_id.as_str()))
                    .count();
                if let Some(snapshot) = self.call_as(
                    "service_archive_project_conversations",
                    json!({ "projectId": project_id }),
                ) {
                    self.apply_snapshot(snapshot);
                    self.emit(BackendEvent::Notice(Notice::ArchivedCount(count)));
                }
            }
            Command::RemoveProject { project_id } => {
                // Name.
                // 当前名称。
                let name = self
                    .wb
                    .iter()
                    .flat_map(|wb| &wb.projects)
                    .find(|p| p.id == project_id)
                    .map(|p| p.name.clone())
                    .unwrap_or_default();
                if let Some(snapshot) =
                    self.call_as("service_remove_project", json!({ "projectId": project_id }))
                {
                    self.apply_snapshot(snapshot);
                    self.emit(BackendEvent::Notice(Notice::ProjectRemoved(name)));
                }
            }
            Command::RenameConversation {
                conversation_id,
                title,
            } => {
                if is_draft(&conversation_id) {
                    return;
                }
                if let Some(snapshot) = self.call_as(
                    "service_rename_conversation",
                    json!({ "conversationId": conversation_id, "name": title }),
                ) {
                    self.apply_snapshot(snapshot);
                    self.emit(BackendEvent::Notice(Notice::Renamed));
                }
            }
            Command::ArchiveConversation { conversation_id } => {
                if self.drafts.remove(&conversation_id).is_some() {
                    self.emit(BackendEvent::ConversationRemoved(conversation_id));
                    return;
                }
                // Title.
                // 当前标题。
                let title = self
                    .summary(&conversation_id)
                    .map(|c| c.title)
                    .unwrap_or_default();
                let Some(client) = self.client.clone() else {
                    self.emit(BackendEvent::Notice(Notice::NotConnected));
                    return;
                };
                match client.call(
                    "service_archive_conversation",
                    json!({ "conversationId": conversation_id }),
                ) {
                    Ok(value) => {
                        if let Some(snapshot) = self.read("service_archive_conversation", value) {
                            self.apply_snapshot(snapshot);
                            self.emit(BackendEvent::Notice(Notice::Archived(title)));
                        }
                    }
                    // An old-format session can be neither opened nor archived; offer deletion instead.
                    // 旧格式会话既无法打开也无法归档；改为提供删除。
                    Err(error)
                        if !error.transport
                            && error.message.contains(UNSUPPORTED_SESSION_FORMAT_CODE) =>
                    {
                        self.emit(BackendEvent::UnsupportedConversation {
                            conversation_id,
                            title,
                        });
                    }
                    Err(error) => self.failed(error),
                }
            }
            Command::DeleteUnsupportedConversation { conversation_id } => {
                // Title.
                // 当前标题。
                let title = self
                    .summary(&conversation_id)
                    .map(|c| c.title)
                    .unwrap_or_default();
                if let Some(snapshot) = self.call_as(
                    "service_delete_unsupported_conversation",
                    json!({ "conversationId": conversation_id }),
                ) {
                    self.apply_snapshot(snapshot);
                    self.emit(BackendEvent::Notice(Notice::UnsupportedDeleted(title)));
                }
            }
            Command::StopSubagentTask {
                parent_conversation_id,
                task_id,
            } => self.subagent_task(parent_conversation_id, task_id, true),
            Command::CloseSubagentTask {
                parent_conversation_id,
                task_id,
            } => self.subagent_task(parent_conversation_id, task_id, false),
            Command::SuppressContextInjectionItem {
                conversation_id,
                memory_id,
            } => self.memory_action(&conversation_id, &memory_id, false),
            Command::DeleteMemories {
                conversation_id,
                memory_id,
            } => self.memory_action(&conversation_id, &memory_id, true),
            Command::SaveSessionGoal {
                conversation_id,
                objective,
                token_budget,
            } => self.goal(
                "service_save_session_goal",
                json!({
                    "conversationId": conversation_id,
                    "objective": objective,
                    "tokenBudget": token_budget,
                }),
                Some(SessionGoalStatus::Active),
            ),
            Command::ResumeSessionGoal {
                conversation_id,
                objective,
                token_budget,
            } => self.goal(
                "service_resume_session_goal",
                json!({
                    "conversationId": conversation_id,
                    "objective": objective,
                    "tokenBudget": token_budget,
                }),
                Some(SessionGoalStatus::Active),
            ),
            Command::ClearSessionGoal { conversation_id } => self.goal(
                "service_clear_session_goal",
                json!({ "conversationId": conversation_id }),
                None,
            ),
            Command::UpdateSessionGoalStatus {
                conversation_id,
                status,
            } => self.goal(
                "service_update_session_goal_status",
                json!({ "conversationId": conversation_id, "status": status.as_str() }),
                Some(status),
            ),
            Command::Call(call) => self.service_call(call),
        }
    }

    /// Selects the exact conversation, marks it read and hydrates its current state.
    /// 选择准确会话、标记已读并加载其当前状态。
    /// `id` is the exact entity identity.
    /// `id` 为准确实体身份。
    fn select(&mut self, id: String) {
        self.selected = Some(id.clone());
        if let Some(draft) = self.drafts.get(&id) {
            self.emit(BackendEvent::ConversationHydrated {
                conversation_id: id.clone(),
                events: Vec::new(),
            });
            self.emit_answer_mode(&id);
            self.emit_permission_mode(&id);
            if let Some(key) = draft.model_key.clone() {
                self.model_keys.insert(id, key);
            }
            self.emit_effort();
            // A draft can belong to another project; the Git panel follows it like a session.
            // 草稿可能属于其他项目；Git 面板与会话一样随之切换。
            self.git_show();
            return;
        }
        if let Some(snapshot) = self.call_as(
            "service_select_conversation",
            json!({ "conversationId": id }),
        ) {
            self.apply_snapshot(snapshot);
            // `markConversationRead` is ancillary, as in the WebUI: a failure never surfaces.
            // 与 WebUI 一致，已读标记属于附属操作：失败不会提示。
            if let Some(client) = self.client.clone()
                && let Ok(result) = client.call(
                    "service_mark_conversation_read",
                    json!({ "conversationId": id }),
                )
                && let Ok(snapshot) = serde_json::from_value(result)
            {
                self.apply_snapshot(snapshot);
            }
        }
        self.emit_conversation_state(&id);
        self.emit_question(&id);
        self.hydrate(&id);
        self.git_show();
    }

    /// Saves the selected catalog model route for the exact conversation or local draft.
    /// 为准确会话或本地草稿保存选中的目录模型路由。
    /// `conversation_id` is the exact durable or local-draft conversation identity.
    /// `conversation_id` 为准确持久或本地草稿会话身份。
    /// `model_id` is the catalog-selected model route identity.
    /// `model_id` 为目录选中的模型路由身份。
    fn select_model(&mut self, conversation_id: Option<String>, model_id: String) {
        // Route.
        // 目录中的推理路由。
        let Some(route) = ModelRoute::from_key(&model_id) else {
            return;
        };
        if let Some(id) = &conversation_id {
            self.model_keys.insert(id.clone(), model_id.clone());
            if let Some(draft) = self.drafts.get_mut(id) {
                draft.model_key = Some(model_id);
                return;
            }
        }
        // Exact protocol request.
        // 协议请求文档。
        let mut request = json!({
            "providerId": route.provider_id,
            "modelId": route.model_id,
            "connectionId": route.connection_id,
        });
        if let Some(id) = conversation_id {
            request["conversationId"] = Value::String(id);
        }
        // The response is the kernel's settings snapshot; as in the WebUI, the workbench is read
        // afterwards for the conversation's new route and reasoning options.
        // 响应为内核的设置快照；与 WebUI 一致，之后读取工作台以获得会话的新路线与推理选项。
        if self
            .call(
                "service_select_public_ai_model",
                json!({ "request": request }),
            )
            .is_some()
        {
            self.reload_workbench();
            self.emit_effort();
        }
    }

    /// Stages attachments and submits the prompt, resolving a local draft to its durable conversation.
    /// 暂存附件并提交提示，将本地草稿解析为持久会话。
    /// `conversation_id` is the exact durable or local-draft conversation identity.
    /// `conversation_id` 为准确持久或本地草稿会话身份。
    /// `model_id` is the catalog-selected model route identity.
    /// `model_id` 为目录选中的模型路由身份。
    fn submit(
        &mut self,
        conversation_id: String,
        prompt: String,
        mode: ExecutionMode,
        effort: ReasoningEffort,
        model_id: String,
        attachments: Vec<String>,
    ) {
        // Is new.
        // 是否为本地新会话。
        let is_new = self.drafts.contains_key(&conversation_id);
        // Project id.
        // 准确宿主项目身份。
        let project_id = match self.drafts.get(&conversation_id) {
            Some(draft) => draft.project_id.clone(),
            None => self.summary(&conversation_id).and_then(|c| c.project_id),
        };
        // Staged.
        // 已暂存差异。
        let staged: Vec<api::ComposerAttachmentSummary> = attachments
            .iter()
            .filter_map(|path| self.stage_attachment(path, project_id.as_deref(), &conversation_id))
            .collect();
        if staged.len() != attachments.len() {
            self.emit(BackendEvent::SubmitRejected {
                conversation_id,
                prompt,
                attachments,
            });
            return;
        }
        // Execution mode.
        // 明确执行模式。
        let execution_mode = match mode {
            ExecutionMode::Build => "build",
            ExecutionMode::Plan => "plan",
        };
        // Existing conversations change model and effort through their own calls; only a new
        // conversation carries them in the submission.
        // 已有会话通过专门调用切换模型与推理强度；只有新会话在提交中携带它们。
        let (id_param, model_selection, effort_param) = match is_new {
            true => {
                // Selection.
                // 已批准选择。
                let selection = ModelRoute::from_key(&model_id).map(|r| {
                    json!({
                        "providerId": r.provider_id,
                        "modelId": r.model_id,
                        "connectionId": r.connection_id,
                    })
                });
                (
                    Value::Null,
                    selection.unwrap_or(Value::Null),
                    self.effort_value(effort).map_or(Value::Null, Value::String),
                )
            }
            false => (
                Value::String(conversation_id.clone()),
                Value::Null,
                Value::Null,
            ),
        };
        // The service admits a submission only from the slot that owns its target: the
        // conversation itself once it exists, the project's new-conversation slot before that.
        // 服务只接受拥有提交目标的草稿槽：会话存在后为会话本身，此前为项目的新会话槽。
        let draft_slot_key = match is_new {
            true => format!("new:{}", project_id.as_deref().unwrap_or("workspace")),
            false => format!("conversation:{conversation_id}"),
        };
        // Exact RPC parameters.
        // 准确 RPC 参数。
        let params = json!({
            "clientSubmissionId": fresh_id(),
            "projectId": project_id,
            "conversationId": id_param,
            "draftSlotKey": draft_slot_key,
            "prompt": prompt,
            "attachments": staged,
            "executionMode": execution_mode,
            "reasoningEffort": effort_param,
            "modelSelection": model_selection,
            // An unchanged draft sends no answer mode, so the service applies the default at creation.
            // 未修改的草稿不发送回答模式，由服务在创建时采用默认值。
            "sessionSettings": self.drafts.get(&conversation_id).map(|draft| json!({
                "answerMode": (draft.answer_mode.revision > 0).then_some(draft.answer_mode.mode),
                "permissionMode": draft.permission_mode,
            })),
        });
        // Local id.
        // 本地会话草稿身份。
        let local_id = conversation_id;
        self.spawn_call(
            "service_submit_conversation_prompt",
            params,
            move |worker, result| {
                // Result.
                // 服务结果。
                let result = match result {
                    Ok(result) => result,
                    Err(error) => {
                        // A refused submission hands its text and files back to the composer.
                        // 被拒绝的提交将其文本与文件交还输入框。
                        worker.emit(BackendEvent::SubmitRejected {
                            conversation_id: local_id,
                            prompt,
                            attachments,
                        });
                        return worker.failed(error);
                    }
                };
                let Some(ack) = worker.read::<api::SubmitConversationPromptAck>(
                    "service_submit_conversation_prompt",
                    result,
                ) else {
                    return;
                };
                // Real id.
                // 服务返回的持久会话身份。
                let real_id = ack.conversation_id;
                if let Some(draft) = worker.drafts.remove(&local_id) {
                    if let Some(key) = draft.model_key {
                        worker.model_keys.insert(real_id.clone(), key);
                    }
                    if let Some(row) = worker.summary(&real_id) {
                        worker.emit(BackendEvent::ConversationSummary(row));
                    }
                    // Select first so removing the draft does not fall back to another conversation.
                    // 先选中，使移除草稿时不会回退到其他会话。
                    worker.emit(BackendEvent::Selected(real_id));
                    worker.emit(BackendEvent::ConversationRemoved(local_id));
                } else if !worker.windows.contains_key(&real_id) {
                    worker.hydrate(&real_id);
                }
            },
        );
    }

    /// Stages one local file through the service; the summary is what the submission carries.
    /// 通过服务暂存一个本地文件；提交携带的是返回的摘要。
    /// `path` is the exact requested path.
    /// `path` 为准确请求路径。
    /// `conversation_id` is the exact durable or local-draft conversation identity.
    /// `conversation_id` 为准确持久或本地草稿会话身份。
    fn stage_attachment(
        &mut self,
        path: &str,
        project_id: Option<&str>,
        conversation_id: &str,
    ) -> Option<api::ComposerAttachmentSummary> {
        // Bytes.
        // 完整字节缓冲。
        let bytes = match std::fs::read(path) {
            Ok(bytes) => bytes,
            Err(error) => {
                self.toast(NoticeSeverity::Error, format!("{path}: {error}"));
                return None;
            }
        };
        // File name.
        // 准确附件文件名。
        let file_name = std::path::Path::new(path)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.to_string());
        self.call_as(
            "service_stage_composer_attachment_bytes",
            json!({
                "projectId": project_id,
                "conversationId": (!is_draft(conversation_id)).then_some(conversation_id),
                "fileName": file_name,
                "mimeType": mime_type(path),
                "dataBase64": base64(&bytes),
            }),
        )
    }

    /// Runs the declared goal operation and reloads current goal state.
    /// 执行已声明的目标操作并重新加载当前目标状态。
    /// `method` is the static WebUI RPC method name.
    /// `method` 为静态 WebUI RPC 方法名。
    /// `status` is the declared service lifecycle status.
    /// `status` 为已声明的服务生命周期状态。
    fn goal(&mut self, method: &str, request: Value, status: Option<SessionGoalStatus>) {
        if let Some(response) = self.call_as(method, json!({ "request": request })) {
            self.absorb_addons(response);
            self.emit(BackendEvent::Notice(Notice::Goal(status)));
        }
    }

    /// Stops or closes the exact task owned by the selected parent conversation.
    /// 停止或关闭所选父会话持有的准确任务。
    /// `parent` is the exact parent conversation identity.
    /// `parent` 为准确父会话身份。
    /// `task_id` is the exact subagent task identity.
    /// `task_id` 为准确子代理任务身份。
    /// `stop` is whether the task is stopped rather than closed.
    /// `stop` 为是否停止任务而非关闭任务。
    fn subagent_task(&mut self, parent: String, task_id: String, stop: bool) {
        // Name.
        // 当前名称。
        let name = self
            .navigation(&parent)
            .and_then(|n| n.tasks.iter().find(|t| t.task_id == task_id))
            .map(|t| t.label.clone())
            .unwrap_or_default();
        // Method.
        // 准确 RPC 方法名。
        let method = if stop {
            "service_stop_subagent_task"
        } else {
            "service_close_subagent_task"
        };
        if let Some(route) = self.call_as(
            method,
            json!({ "parentConversationId": parent, "taskId": task_id }),
        ) {
            self.absorb_route(route);
            self.emit(BackendEvent::Notice(if stop {
                Notice::AgentStopped(name)
            } else {
                Notice::AgentClosed(name)
            }));
        }
    }

    /// Applies the existing session memory action using the displayed memory identity.
    /// 使用显示的记忆身份应用既有会话记忆操作。
    /// `conversation_id` is the exact durable or local-draft conversation identity.
    /// `conversation_id` 为准确持久或本地草稿会话身份。
    /// `memory_id` is the displayed session memory identity.
    /// `memory_id` 为显示的会话记忆身份。
    /// `delete` is the existing delete versus suppression selector.
    /// `delete` 为既有删除或抑制选择器。
    fn memory_action(&mut self, conversation_id: &str, memory_id: &str, delete: bool) {
        // Project id.
        // 准确宿主项目身份。
        let project_id = self.summary(conversation_id).and_then(|c| c.project_id);
        // Item.
        // 当前条目。
        let dedupe_key = self
            .injection(conversation_id)
            .iter()
            .flat_map(|injection| &injection.dynamic_cards)
            .flat_map(|card| &card.items)
            .find(|item| {
                item.memory_id.is_some_and(|id| id.to_string() == memory_id)
                    || item.dedupe_key.as_deref() == Some(memory_id)
            })
            .and_then(|item| item.dedupe_key.clone());
        // The WebUI only offers these for positive memory ids (`target.memoryId > 0`).
        // WebUI 只为正的记忆 ID 提供这些操作（`target.memoryId > 0`）。
        let numeric_id = memory_id.parse::<u64>().ok().filter(|id| *id > 0);
        // Scope.
        // 明确服务作用域。
        let scope = if project_id.is_some() {
            "project"
        } else {
            "global"
        };
        // Exact protocol request.
        // 协议请求文档。
        let request = if delete {
            // Exact entity identity.
            // 准确实体身份。
            let Some(id) = numeric_id else {
                return;
            };
            json!({
                "action": "delete_memories",
                "commandId": fresh_id(),
                "scope": scope,
                "projectId": project_id,
                "payload": {
                    "memoryIds": [id],
                    "reason": "manual memory delete from context panel",
                },
            })
        } else {
            json!({
                "action": "suppress_context_injection_item",
                "commandId": fresh_id(),
                "scope": scope,
                "projectId": project_id,
                "conversationId": conversation_id,
                "payload": {
                    "dedupeKey": dedupe_key,
                    "memoryId": numeric_id,
                    "reason": "manual context injection suppression from memory panel",
                },
            })
        };
        if self
            .call(
                "service_run_vmm_memory_action",
                json!({ "request": request }),
            )
            .is_some()
        {
            self.emit(BackendEvent::Notice(if delete {
                Notice::MemoryDeleted
            } else {
                Notice::MemorySuppressed
            }));
        }
    }

    /// Project of the selected conversation (or draft), for project-scoped calls.
    /// 选中会话（或草稿）所属的项目，用于项目范围的调用。
    pub(super) fn selected_project(&self) -> Option<&api::ProjectSummary> {
        // Exact entity identity.
        // 准确实体身份。
        let id = self.selected.as_deref()?;
        // Project id.
        // 准确宿主项目身份。
        let project_id = match self.drafts.get(id) {
            Some(draft) => draft.project_id.clone(),
            None => self.summary(id).and_then(|c| c.project_id),
        };
        // Projects.
        // 项目目录。
        let projects = &self.wb.as_ref()?.projects;
        project_id
            .and_then(|pid| projects.iter().find(|p| p.id == pid))
            .or_else(|| projects.first())
    }

    /// Returns the selected durable conversation, excluding local draft identities.
    /// 返回所选持久会话，排除本地草稿身份。
    /// Returns `Option<String>`.
    /// 返回 `Option<String>`。
    pub(super) fn selected_real(&self) -> Option<String> {
        self.selected.clone().filter(|id| !is_draft(id))
    }
}

/// Checks the single shared local conversation draft prefix.
/// 检查唯一共用的本地会话草稿前缀。
/// `id` is the exact entity identity.
/// `id` 为准确实体身份。
/// Returns `bool`.
/// 返回 `bool`。
pub(super) fn is_draft(id: &str) -> bool {
    id.starts_with(DRAFT_PREFIX)
}

/// Projects local draft ownership into a sidebar conversation summary.
/// 将本地草稿归属投影为侧栏会话摘要。
/// `id` is the exact entity identity.
/// `id` 为准确实体身份。
/// Returns `ConversationSummary`.
/// 返回 `ConversationSummary`。
fn draft_row(id: &str, draft: &Draft) -> ConversationSummary {
    ConversationSummary {
        id: id.to_string(),
        title: crate::i18n::text("console.newConversation"),
        project_id: draft.project_id.clone(),
        run_phase: RunPhase::Idle,
        last_updated_label: String::new(),
        last_updated_at_ms: None,
        unread: false,
        attention: false,
        dedicated: false,
    }
}

/// Compares workspace paths with the normalization used for local project matching.
/// 使用本地项目匹配所用的规范化规则比较工作区路径。
/// Returns `bool`.
/// 返回 `bool`。
fn same_path(a: &str, b: &str) -> bool {
    // Norm.
    // 路径规范化函数。
    let norm = |s: &str| {
        s.trim_end_matches(['/', '\\'])
            .replace('\\', "/")
            .to_lowercase()
    };
    norm(a) == norm(b)
}

/// Leading `units` UTF-16 code units of `text` (the unit of `base_text_utf16_len`).
/// `text` 的前 `units` 个 UTF-16 码元（`base_text_utf16_len` 的单位）。
/// Returns `&str`.
/// 返回 `&str`。
fn utf16_prefix(text: &str, units: usize) -> &str {
    // Count.
    // 当前计数。
    let mut count = 0;
    for (index, ch) in text.char_indices() {
        if count >= units {
            return &text[..index];
        }
        count += ch.len_utf16();
    }
    text
}

/// Selects the attachment content type from its explicit file extension.
/// 根据明确文件扩展名选择附件内容类型。
/// `path` is the exact requested path.
/// `path` 为准确请求路径。
/// Returns `&'static str`.
/// 返回 `&'static str`。
fn mime_type(path: &str) -> &'static str {
    // Extension.
    // 附件文件扩展名。
    let extension = std::path::Path::new(path)
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    match extension.as_str() {
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "pdf" => "application/pdf",
        "json" => "application/json",
        "md" => "text/markdown",
        "txt" | "log" | "rs" | "ts" | "tsx" | "js" | "py" | "toml" | "yaml" | "yml" | "lua"
        | "go" | "c" | "h" | "cpp" | "java" | "css" | "html" | "sh" | "ps1" => "text/plain",
        _ => "application/octet-stream",
    }
}

/// Standard base64 with padding.
/// 带填充的标准 base64。
/// Returns `String`.
/// 返回 `String`。
fn base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    // Out.
    // 界面事件发送通道。
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        // Numeric value.
        // 当前数值。
        let n = (u32::from(chunk[0]) << 16)
            | (u32::from(*chunk.get(1).unwrap_or(&0)) << 8)
            | u32::from(*chunk.get(2).unwrap_or(&0));
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(TABLE[(n >> (18 - 6 * i) & 63) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

/// Conversations that show one pending question: the raising session and, for a child agent, its
/// root conversation.
/// 显示某个待回复提问的会话：发起会话，以及子代理所属的根会话。
fn question_targets(question: &api::PendingQuestionSnapshot) -> impl Iterator<Item = String> {
    std::iter::once(question.session_id.clone()).chain(question.owner_conversation_id.clone())
}

#[cfg(test)]
mod tests {
    use super::super::test_support::RpcServer;
    use super::*;
    use crate::service_api::{knowledge_base, settings_actions};
    /// Real asynchronous document failures publish the exact reader scope and a sanitized cause.
    /// 真实异步文档失败发布准确阅读范围及脱敏原因。
    #[test]
    fn document_rpc_failure_preserves_scope_for_persistent_retry() {
        // Loopback service returns the documented RPC error envelope.
        // 回环服务返回文档定义的 RPC 错误结构。
        let server = RpcServer::start(1, |request| {
            assert_eq!(
                request.body["method"],
                "service_read_knowledge_library_document"
            );
            assert_eq!(
                request.body["params"],
                json!({"request":{"libraryId":"library","documentId":"doc","path":null,"startLine":null,"endLine":null}})
            );
            json!({"ok":false,"error":{"message":"ERR_TEST_BODY_UNAVAILABLE"}})
        });
        // Isolated worker and channels never discover an external endpoint.
        // 隔离工作线程及通道不发现外部端点。
        let (mut worker, events, input) = worker_fixture(server.client.clone());
        worker.service_call(knowledge_base::read_managed_knowledge_document(
            vulcan_service_api::KnowledgeDocumentReadRequest {
                library_id: "library".into(),
                document_id: Some("doc".into()),
                path: None,
                start_line: None,
                end_line: None,
            },
        ));
        worker.handle_input(input.recv_timeout(Duration::from_secs(2)).unwrap());
        assert!(events.try_iter().any(|event|matches!(event,BackendEvent::SlotFailed{slot:Slot::KnowledgeDocument,call,message}
            if call.param("/request/libraryId") == "library" && call.param("/request/documentId") == "doc"
               && message == "ERR_TEST_BODY_UNAVAILABLE")));
        server.thread.join().unwrap();
    }

    /// A prompt to an existing session is submitted from that session's own draft slot, and a
    /// refused submission hands its text back instead of losing it.
    /// 对已有会话的提示从该会话自己的草稿槽提交，被拒绝的提交交还其文本而不是丢失。
    #[test]
    fn existing_session_submits_from_its_own_slot_and_refusal_returns_the_prompt() {
        let session_id = "11111111-1111-4111-8111-111111111111";
        let server = RpcServer::start(1, move |request| {
            assert_eq!(request.body["method"], "service_submit_conversation_prompt");
            assert_eq!(
                request.body["params"]["draftSlotKey"],
                format!("conversation:{session_id}")
            );
            assert_eq!(request.body["params"]["conversationId"], session_id);
            json!({"ok":false,"error":{"message":"ERR_TEST_REFUSED"}})
        });
        let (mut worker, events, input) = worker_fixture(server.client.clone());
        worker.submit(
            session_id.to_string(),
            "follow up".to_string(),
            ExecutionMode::Build,
            ReasoningEffort::High,
            String::new(),
            Vec::new(),
        );
        worker.handle_input(input.recv_timeout(Duration::from_secs(2)).unwrap());
        server.thread.join().unwrap();
        assert!(events.try_iter().any(|event| matches!(
            event,
            BackendEvent::SubmitRejected { conversation_id, prompt, attachments }
                if conversation_id == session_id && prompt == "follow up" && attachments.is_empty()
        )));
    }

    /// Without a connection the continuation still runs, so what it holds is released.
    /// 没有连接时后续回调照常执行，使其持有的状态得到释放。
    #[test]
    fn spawned_call_without_connection_still_runs_its_continuation() {
        let (out, _events) = std::sync::mpsc::channel();
        let (inbox, _input) = std::sync::mpsc::channel();
        let mut worker = Worker::new(out, inbox, None);
        let ran = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let flag = ran.clone();
        worker.spawn_call("service_test", json!({}), move |_, result| {
            assert!(result.is_err_and(|error| !error.transport));
            flag.store(true, std::sync::atomic::Ordering::SeqCst);
        });
        assert!(ran.load(std::sync::atomic::Ordering::SeqCst));
    }

    /// Creates isolated worker channels; no discovery, connection or user state is touched.
    /// 创建独立工作线程通道；不访问发现文件、真实连接或用户状态。
    /// Returns `(Worker, Receiver<BackendEvent>, Receiver<Input>)`.
    /// 返回 `(Worker, Receiver<BackendEvent>, Receiver<Input>)`。
    fn worker_fixture(client: RpcClient) -> (Worker, Receiver<BackendEvent>, Receiver<Input>) {
        // Out; Events.
        // 界面事件发送通道；事件集合或事件接收通道。
        let (out, events) = std::sync::mpsc::channel();
        // Inbox; Current input.
        // 工作线程收件通道；当前输入。
        let (inbox, input) = std::sync::mpsc::channel();
        // Worker.
        // 串行服务工作线程状态。
        let mut worker = Worker::new(out, inbox, None);
        worker.client = Some(client);
        (worker, events, input)
    }

    /// Confirms that real asynchronous RPC reads cannot restore an older slot loader or value.
    /// 确认实际异步 RPC 读取无法恢复较旧的槽位加载器或值。
    #[test]
    fn overlapping_rpc_reads_publish_only_the_latest_selection() {
        // Server.
        // 当前服务器或本地合成监听器。
        let server = RpcServer::start(2, |request| {
            json!({"ok":true,"result":{
            "queries":[request.body.pointer("/params/request/query").unwrap()],"hits":[],"libraries":[],"failures":[]}})
        });
        // Worker; Events; Current input.
        // 串行服务工作线程状态；事件集合或事件接收通道；当前输入。
        let (mut worker, events, input) = worker_fixture(server.client.clone());
        worker.service_call(knowledge_base::search_managed_knowledge_library(
            vulcan_service_api::KnowledgeLibrarySearchRequest {
                library_id: "demo-library".into(),
                query: "old".into(),
                limit: 7,
            },
        ));
        worker.service_call(knowledge_base::search_managed_knowledge_library(
            vulcan_service_api::KnowledgeLibrarySearchRequest {
                library_id: "demo-library".into(),
                query: "new".into(),
                limit: 7,
            },
        ));
        for _ in 0..2 {
            worker.handle_input(input.recv_timeout(Duration::from_secs(2)).unwrap());
        }
        // Received.
        // 接收到的消息。
        let received: Vec<_> = events.try_iter().collect();
        assert_eq!(received.len(), 1);
        assert!(
            matches!(&received[0],BackendEvent::Slot{slot:Slot::KnowledgeSearch,value,..} if value["queries"] == json!(["new"]))
        );
        assert_eq!(
            worker.loaders[&Slot::KnowledgeSearch].param("/request/query"),
            "new"
        );
        server.thread.join().unwrap();
    }

    /// Acknowledges a mutation only after its exact previous loader publishes authoritative state.
    /// 仅在准确的上一加载器发布权威状态后确认变更。
    #[test]
    fn mutation_reloads_before_unlocking_the_next_full_dto_edit() {
        // Interval.
        // 配置的刷新间隔。
        let mut interval = 60;
        // Server.
        // 当前服务器或本地合成监听器。
        let server = RpcServer::start(2, move |request| {
            match request.body["method"].as_str().unwrap() {
                "service_save_public_ai_usage_refresh_settings" => {
                    interval = request
                        .body
                        .pointer("/params/request/intervalSeconds")
                        .unwrap()
                        .as_i64()
                        .unwrap();
                    json!({"ok":true,"result":{}})
                }
                "service_public_ai_settings_snapshot" => {
                    json!({"ok":true,"result":{"usageRefreshIntervalSeconds":interval}})
                }
                _ => panic!("unexpected synthetic RPC method"),
            }
        });
        // Worker; Events; Current input.
        // 串行服务工作线程状态；事件集合或事件接收通道；当前输入。
        let (mut worker, events, input) = worker_fixture(server.client.clone());
        worker.loaders.insert(
            Slot::PublicAiSettings,
            settings_actions::load_public_ai_settings(),
        );
        worker.service_call(settings_actions::save_public_ai_usage_refresh_settings(120));
        worker.handle_input(input.recv_timeout(Duration::from_secs(2)).unwrap());
        // Received.
        // 接收到的消息。
        let received: Vec<_> = events.try_iter().collect();
        assert_eq!(received.len(), 3);
        assert!(
            matches!(&received[0],BackendEvent::Slot{slot:Slot::PublicAiSettings,value,..} if value["usageRefreshIntervalSeconds"] == 120)
        );
        assert!(matches!(
            received[1],
            BackendEvent::Notice(Notice::Completed(_))
        ));
        assert!(matches!(received[2], BackendEvent::CallFinished));
        server.thread.join().unwrap();
    }

    /// A chained write sends its follow-up only after the first succeeds, and a failed follow-up
    /// still refreshes the state the first write already changed.
    /// 链式写入仅在首个写入成功后发送后续写入；后续写入失败时仍刷新首个写入已改变的状态。
    #[test]
    fn failed_follow_up_write_still_reloads_its_slots() {
        // Methods in arrival order.
        // 按到达顺序记录的方法。
        let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let record = seen.clone();
        // Server.
        // 当前服务器或本地合成监听器。
        let server = RpcServer::start(3, move |request| {
            let method = request.body["method"].as_str().unwrap().to_string();
            record.lock().unwrap().push(method.clone());
            match method.as_str() {
                "service_save_public_ai_provider_extension_target_set" => {
                    json!({"ok":true,"result":{}})
                }
                "service_save_public_ai_provider_tool_capability_enabled" => {
                    json!({"ok":false,"error":{"message":"capability rejected"}})
                }
                "service_public_ai_settings_snapshot" => {
                    json!({"ok":true,"result":{"providerToolEnabledCapabilities":[]}})
                }
                _ => panic!("unexpected synthetic RPC method"),
            }
        });
        // Worker; Events; Current input.
        // 串行服务工作线程状态；事件集合或事件接收通道；当前输入。
        let (mut worker, events, input) = worker_fixture(server.client.clone());
        worker.loaders.insert(
            Slot::PublicAiSettings,
            settings_actions::load_public_ai_settings(),
        );
        worker.service_call(
            settings_actions::save_public_ai_provider_extension_target_set(
                "image_generate",
                vec![json!({"connection_id":"synthetic","target_id":"synthetic-target"})],
            )
            .then(
                settings_actions::save_public_ai_provider_tool_capability_enabled(
                    "image_generate",
                    true,
                ),
            ),
        );
        worker.handle_input(input.recv_timeout(Duration::from_secs(2)).unwrap());
        server.thread.join().unwrap();
        assert_eq!(
            *seen.lock().unwrap(),
            [
                "service_save_public_ai_provider_extension_target_set",
                "service_save_public_ai_provider_tool_capability_enabled",
                "service_public_ai_settings_snapshot",
            ]
        );
        // Received.
        // 接收到的消息。
        let received: Vec<_> = events.try_iter().collect();
        assert!(received.iter().any(|event| matches!(
            event,
            BackendEvent::Slot {
                slot: Slot::PublicAiSettings,
                ..
            }
        )));
        assert!(
            !received
                .iter()
                .any(|event| matches!(event, BackendEvent::Notice(Notice::Completed(_))))
        );
        assert!(matches!(received.last(), Some(BackendEvent::CallFinished)));
    }

    /// Drops completion closures from disconnected/replaced connections and supports explicit shutdown.
    /// 丢弃断开或已替换连接的完成闭包，并支持明确停止。
    #[test]
    fn completion_generation_and_shutdown_are_enforced() {
        // Server.
        // 当前服务器或本地合成监听器。
        let server = RpcServer::start(0, |_| Value::Null);
        // Worker; Events.
        // 串行服务工作线程状态；事件集合或事件接收通道。
        let (mut worker, events, _) = worker_fixture(server.client.clone());
        worker.generation = 2;
        worker.handle_input(Input::Done {
            generation: 1,
            apply: Box::new(|_| panic!("stale completion applied")),
        });
        worker.client = None;
        worker.handle_input(Input::Done {
            generation: 2,
            apply: Box::new(|_| panic!("disconnected completion applied")),
        });
        worker.client = Some(server.client.clone());
        worker.generation = 3;
        worker.handle_input(Input::Done {
            generation: 2,
            apply: Box::new(|_| panic!("reconnected completion applied")),
        });
        worker.handle_input(Input::Done {
            generation: 3,
            apply: Box::new(|worker| worker.revision = 42),
        });
        assert_eq!(worker.revision, 42);
        assert!(events.try_recv().is_err());
        assert!(!worker.handle_input(Input::Shutdown));
        server.thread.join().unwrap();
    }

    /// Frames queued by a stream that was torn down never reach the retained workbench.
    /// 已拆除事件流排队的帧绝不会写入保留工作台。
    #[test]
    fn frames_of_a_torn_down_stream_are_dropped() {
        let server = RpcServer::start(0, |_| Value::Null);
        let (mut worker, events, _) = worker_fixture(server.client.clone());
        let torn_down = worker.generation;
        worker.disconnected();
        let _ = events.try_iter().count();
        worker.handle_input(Input::Rejected {
            generation: torn_down,
            kind: String::from("workbench.delta"),
            detail: String::from("queued before teardown"),
        });
        assert!(events.try_recv().is_err());
        server.thread.join().unwrap();
    }

    /// Archiving an old-format session asks for deletion instead of failing, and the confirmed
    /// deletion calls the dedicated RPC for that exact session.
    /// 归档旧格式会话时改为请求删除而不是直接失败，确认后的删除对该会话调用专用 RPC。
    #[test]
    fn unsupported_archive_offers_deletion() {
        let session_id = "11111111-1111-4111-8111-111111111111";
        let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let record = seen.clone();
        let server = RpcServer::start(2, move |request| {
            let method = request.body["method"].as_str().unwrap().to_string();
            record.lock().unwrap().push((
                method.clone(),
                request.body["params"]["conversationId"].clone(),
            ));
            match method.as_str() {
                "service_archive_conversation" => json!({"ok":false,"error":{"message":
                    "Failed to archive session: unsupported_development_format: metadata schema 5"}}),
                "service_delete_unsupported_conversation" => json!({"ok":true,"result":{
                    "revision": 2, "appVersion": "", "modules": [], "projects": [],
                    "projectSettingsRevisions": api::ProjectSettingsRevisionSnapshot::default(),
                    "conversations": [], "toolCalls": [], "plugins": [], "settings": [],
                    "providers": [], "memories": [], "commandSessions": [], "logs": []}}),
                _ => panic!("unexpected synthetic RPC method"),
            }
        });
        let (mut worker, events, _) = worker_fixture(server.client.clone());
        worker.handle(Command::ArchiveConversation {
            conversation_id: session_id.to_string(),
        });
        let offered: Vec<String> = events
            .try_iter()
            .filter_map(|event| match event {
                BackendEvent::UnsupportedConversation {
                    conversation_id, ..
                } => Some(conversation_id),
                _ => None,
            })
            .collect();
        assert_eq!(offered, [session_id]);
        worker.handle(Command::DeleteUnsupportedConversation {
            conversation_id: session_id.to_string(),
        });
        assert!(
            events
                .try_iter()
                .any(|event| matches!(event, BackendEvent::Notice(Notice::UnsupportedDeleted(_))))
        );
        server.thread.join().unwrap();
        assert_eq!(
            *seen.lock().unwrap(),
            [
                (
                    "service_archive_conversation".to_string(),
                    json!(session_id)
                ),
                (
                    "service_delete_unsupported_conversation".to_string(),
                    json!(session_id)
                ),
            ]
        );
    }

    /// Assistant sessions stay out of the sidebar rows; ordinary sessions remain.
    /// 助手会话不进入侧栏行；普通会话保留。
    #[test]
    fn assistant_sessions_leave_the_sidebar_rows() {
        let server = RpcServer::start(0, |_| Value::Null);
        let (mut worker, _events, _) = worker_fixture(server.client.clone());
        let row = |id: &str| {
            json!({"id": id, "title": id, "status": "idle", "runPhase": "idle",
                "lastUpdatedLabel": "", "lastUpdatedAt": "", "unread": false})
        };
        let mut assistant = row("assistant");
        assistant["dedicated"] = json!(true);
        assistant["assistantName"] = json!("Helper");
        let snapshot = serde_json::from_value(json!({
            "revision": 1, "appVersion": "", "modules": [], "projects": [],
            "projectSettingsRevisions": api::ProjectSettingsRevisionSnapshot::default(),
            "conversations": [row("plain"), assistant],
            "toolCalls": [], "plugins": [], "settings": [], "providers": [], "memories": [],
            "commandSessions": [], "logs": []
        }))
        .expect("synthetic snapshot matches the shared DTO");
        worker.apply_snapshot(snapshot);
        let ids: Vec<String> = worker
            .conversation_rows()
            .into_iter()
            .map(|row| row.id)
            .collect();
        assert_eq!(ids, ["plain"]);
        drop(worker);
        server.thread.join().unwrap();
    }

    /// Archive deltas remove sidebar rows by `conversationId` and keep the archived summary.
    /// 归档增量按 `conversationId` 移除侧栏行，并保留归档摘要。
    #[test]
    fn archive_deltas_move_rows_to_the_archived_list() {
        let server = RpcServer::start(0, |_| Value::Null);
        let (mut worker, events, _) = worker_fixture(server.client.clone());
        let row = |id: &str| {
            json!({"id": id, "title": id, "status": "idle", "runPhase": "idle",
                "lastUpdatedLabel": "", "lastUpdatedAt": "", "unread": false})
        };
        let snapshot = serde_json::from_value(json!({
            "revision": 1, "appVersion": "", "modules": [], "projects": [],
            "projectSettingsRevisions": api::ProjectSettingsRevisionSnapshot::default(),
            "conversations": [row("a"), row("b")],
            "toolCalls": [], "plugins": [], "settings": [], "providers": [], "memories": [],
            "commandSessions": [], "logs": []
        }))
        .expect("synthetic snapshot matches the shared DTO");
        worker.apply_snapshot(snapshot);
        let delta = serde_json::from_value(json!({"generation": 2, "delta": {
            "kind": "conversationsArchived",
            "payload": {"archivedConversations": [
                {"conversationId": "a", "archivedConversation": row("a")}]}}}))
        .expect("synthetic delta matches the shared DTO");
        events.try_iter().for_each(drop);
        worker.service_event(api::ServiceEvent::WorkbenchDelta(Box::new(delta)));
        let removed: Vec<String> = events
            .try_iter()
            .filter_map(|event| match event {
                BackendEvent::ConversationRemoved(id) => Some(id),
                _ => None,
            })
            .collect();
        assert_eq!(removed, ["a"]);
        let wb = worker.wb.as_ref().unwrap();
        assert!(wb.conversations.iter().all(|row| row.id != "a"));
        assert!(wb.archived_conversations.iter().any(|row| row.id == "a"));
        server.thread.join().unwrap();
    }

    /// A child agent's request reaches its root conversation with the requester, after the
    /// conversation's own request.
    /// 子代理的请求连同请求者到达其根会话，排在该会话自身请求之后。
    #[test]
    fn child_agent_questions_surface_in_the_root_conversation() {
        let server = RpcServer::start(0, |_| Value::Null);
        let (mut worker, events, _) = worker_fixture(server.client.clone());
        let snapshot = serde_json::from_value(json!({
            "revision": 1, "appVersion": "", "modules": [], "projects": [],
            "projectSettingsRevisions": api::ProjectSettingsRevisionSnapshot::default(),
            "conversations": [{"id": "root", "title": "root", "status": "idle", "runPhase": "idle",
                "lastUpdatedLabel": "", "lastUpdatedAt": "", "unread": false}],
            "toolCalls": [], "plugins": [], "settings": [], "providers": [], "memories": [],
            "commandSessions": [], "logs": []
        }))
        .expect("synthetic snapshot matches the shared DTO");
        worker.apply_snapshot(snapshot);
        let question = |id: &str, session: &str, owner: Option<&str>| {
            json!({"id": id, "sessionId": session, "sequence": 1, "ownerConversationId": owner,
                "requesterLabel": owner.map(|_| "Explorer"),
                "questions": [{"question": "Go?", "header": "Go", "multiple": false}]})
        };
        let shown = |worker: &mut Worker, generation: u64, pending: Value| {
            let delta = serde_json::from_value(json!({"generation": generation, "delta": {
                "kind": "pendingQuestionChanged", "payload": {"pendingQuestions": pending}}}))
            .expect("synthetic delta matches the shared DTO");
            events.try_iter().for_each(drop);
            worker.service_event(api::ServiceEvent::WorkbenchDelta(Box::new(delta)));
            events
                .try_iter()
                .find_map(|event| match event {
                    BackendEvent::PendingQuestion {
                        conversation_id,
                        question,
                    } if conversation_id == "root" => Some(question),
                    _ => None,
                })
                .expect("the root conversation receives the pending question")
                .map(|question| (question.id, question.requester))
        };
        let child = question("child-q", "child", Some("root"));
        assert_eq!(
            shown(&mut worker, 2, json!([child.clone()])),
            Some(("child-q".into(), Some("Explorer".into())))
        );
        let own = question("own-q", "root", None);
        assert_eq!(
            shown(&mut worker, 3, json!([child, own])),
            Some(("own-q".into(), None))
        );
        assert_eq!(shown(&mut worker, 4, json!([])), None);
        server.thread.join().unwrap();
    }

    #[test]
    /// Verifies encodes base64 with synthetic data and assertions.
    /// 使用合成数据及断言验证 encodes_base64。
    fn encodes_base64() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    /// Verifies slices utf16 prefix with synthetic data and assertions.
    /// 使用合成数据及断言验证 slices_utf16_prefix。
    fn slices_utf16_prefix() {
        assert_eq!(utf16_prefix("a😀b", 1), "a");
        assert_eq!(utf16_prefix("a😀b", 3), "a😀");
        assert_eq!(utf16_prefix("ab", 9), "ab");
    }

    /// Builds a synthetic workbench with two projects; conversation `a` belongs to `one` and `b`
    /// to `two`.
    /// 构建含两个项目的合成工作台；会话 `a` 属于 `one`，`b` 属于 `two`。
    /// Returns the shared DTO.
    /// 返回共享 DTO。
    fn two_project_workbench() -> api::WorkbenchSnapshot {
        let row = |id: &str, project: &str| {
            json!({"id": id, "title": id, "status": "idle", "runPhase": "idle",
                "lastUpdatedLabel": "", "lastUpdatedAt": "", "unread": false,
                "projectId": project})
        };
        serde_json::from_value(json!({
            "revision": 1, "appVersion": "", "modules": [],
            "projects": [{"id": "one", "name": "one", "path": "/one"},
                         {"id": "two", "name": "two", "path": "/two"}],
            "projectSettingsRevisions": api::ProjectSettingsRevisionSnapshot::default(),
            "conversations": [row("a", "one"), row("b", "two")],
            "toolCalls": [], "plugins": [], "settings": [], "providers": [], "memories": [],
            "commandSessions": [], "logs": []
        }))
        .expect("synthetic snapshot matches the shared DTO")
    }

    /// Returns the Git panel events published so far.
    /// 返回目前已发布的 Git 面板事件。
    fn git_events(events: &Receiver<BackendEvent>) -> Vec<BackendEvent> {
        events
            .try_iter()
            .filter(|event| matches!(event, BackendEvent::Git(_) | BackendEvent::GitCleared))
            .collect()
    }

    /// Entering a project without repositories clears the previous project's list instead of
    /// leaving it on screen.
    /// 进入没有仓库的项目时清除上一项目的清单，而不是继续显示。
    #[test]
    fn git_refresh_replaces_the_previous_project_with_an_empty_catalog() {
        let server = RpcServer::start(4, |request| {
            let workspace = request.body.pointer("/params/workspacePath").unwrap();
            match (request.body["method"].as_str().unwrap(), workspace.as_str()) {
                ("workspace.git.catalog", Some("/one")) => json!({"ok":true,"result":{
                    "projectRoot":"/one","repositories":[{"repositoryKey":".","repositoryRoot":"/one",
                    "name":"one","relativePath":".","isPrimary":true,"monitoringEnabled":true}]}}),
                ("workspace.git.catalog", Some("/two")) => {
                    json!({"ok":true,"result":{"projectRoot":"/two","repositories":[]}})
                }
                ("workspace.diff.snapshot", Some("/one")) => json!({"ok":true,"result":{
                    "currentBranch":"main","files":[{"key":"k","newPath":"a.rs","status":"modified"}]}}),
                _ => panic!("unexpected synthetic RPC request"),
            }
        });
        let (mut worker, events, _) = worker_fixture(server.client.clone());
        worker.apply_snapshot(two_project_workbench());
        worker.selected = Some("a".into());
        worker.git_refresh();
        let first = git_events(&events);
        assert!(matches!(first.last(), Some(BackendEvent::Git(snapshot))
            if snapshot.repositories.len() == 1 && snapshot.selected.as_deref() == Some(".")
               && snapshot.files.len() == 2));
        worker.selected = Some("b".into());
        worker.git_refresh();
        let second = git_events(&events);
        assert!(matches!(second.first(), Some(BackendEvent::GitCleared)));
        assert!(matches!(second.last(), Some(BackendEvent::Git(snapshot))
            if snapshot.repositories.is_empty() && snapshot.selected.is_none()
               && snapshot.files.is_empty() && snapshot.branch.is_empty()));
        server.thread.join().unwrap();
    }

    /// Only monitored repositories are read; an unmonitored primary yields the selection to the
    /// first monitored one, and selecting it again is the only accepted choice.
    /// 仅读取受监听仓库；主仓库未受监听时选择让给第一个受监听仓库，且只接受受监听仓库的选择。
    #[test]
    fn git_refresh_details_only_monitored_repositories() {
        let keys = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let record = keys.clone();
        let server = RpcServer::start(3, move |request| {
            match request.body["method"].as_str().unwrap() {
                "workspace.git.catalog" => json!({"ok":true,"result":{"projectRoot":"/one",
                    "repositories":[
                        {"repositoryKey":".","repositoryRoot":"/one","name":"one","relativePath":".",
                         "isPrimary":true,"monitoringEnabled":false},
                        {"repositoryKey":"lib","repositoryRoot":"/one/lib","name":"lib",
                         "relativePath":"lib","isPrimary":false,"monitoringEnabled":true}]}}),
                "workspace.diff.snapshot" => {
                    record.lock().unwrap().push(
                        request
                            .body
                            .pointer("/params/request/repositoryKey")
                            .unwrap()
                            .clone(),
                    );
                    json!({"ok":true,"result":{"currentBranch":"dev","currentBranchUnpushedCommits":1,
                        "files":[{"key":"k","newPath":"b.rs","status":"added"}]}})
                }
                _ => panic!("unexpected synthetic RPC method"),
            }
        });
        let (mut worker, events, _) = worker_fixture(server.client.clone());
        worker.apply_snapshot(two_project_workbench());
        worker.selected = Some("a".into());
        worker.git_refresh();
        let published = git_events(&events);
        let Some(BackendEvent::Git(snapshot)) = published.last() else {
            panic!("a Git snapshot is published");
        };
        assert_eq!(snapshot.selected.as_deref(), Some("lib"));
        assert_eq!(snapshot.repositories[0].changes, None);
        assert_eq!(snapshot.repositories[1].changes, Some(2));
        assert_eq!(snapshot.repositories[1].ahead, 1);
        assert_eq!(*keys.lock().unwrap(), vec![json!("lib"), json!("lib")]);
        // An unmonitored repository is not a valid selection and sends no request.
        // 未受监听仓库不是有效选择，且不发送请求。
        worker.git_select_repository(".".into());
        assert!(
            matches!(git_events(&events).last(), Some(BackendEvent::Git(snapshot))
            if snapshot.selected.as_deref() == Some("lib"))
        );
        server.thread.join().unwrap();
    }

    /// Loaded repositories switch from memory, and a change event rereads only the repositories
    /// it names while the others keep their views.
    /// 已加载的仓库从内存切换；变更事件只重读其指明的仓库，其他仓库保留视图。
    #[test]
    fn repository_switches_read_memory_and_events_reread_named_repositories() {
        let keys = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let record = keys.clone();
        // Catalog and both views of two repositories, then both views of the named repository.
        // 目录及两个仓库各两个视图，随后读取变更事件所指仓库的两个视图。
        let server = RpcServer::start(7, move |request| {
            match request.body["method"].as_str().unwrap() {
                "workspace.git.catalog" => json!({"ok":true,"result":{"projectRoot":"/one",
                    "repositories":[
                        {"repositoryKey":".","repositoryRoot":"/one","name":"one","relativePath":".",
                         "isPrimary":true,"monitoringEnabled":true},
                        {"repositoryKey":"lib","repositoryRoot":"/one/lib","name":"lib",
                         "relativePath":"lib","isPrimary":false,"monitoringEnabled":true}]}}),
                "workspace.diff.snapshot" => {
                    let key = request
                        .body
                        .pointer("/params/request/repositoryKey")
                        .unwrap();
                    record.lock().unwrap().push(key.clone());
                    json!({"ok":true,"result":{"currentBranch":key,
                        "files":[{"key":"k","newPath":"b.rs","status":"added"}]}})
                }
                _ => panic!("unexpected synthetic RPC method"),
            }
        });
        let (mut worker, events, _) = worker_fixture(server.client.clone());
        worker.apply_snapshot(two_project_workbench());
        worker.selected = Some("a".into());
        worker.git_refresh();
        events.try_iter().for_each(drop);
        keys.lock().unwrap().clear();
        worker.git_select_repository("lib".into());
        assert!(keys.lock().unwrap().is_empty(), "switching reads memory");
        assert!(
            matches!(git_events(&events).last(), Some(BackendEvent::Git(snapshot))
            if snapshot.selected.as_deref() == Some("lib") && snapshot.branch == "lib")
        );
        worker.service_event(api::ServiceEvent::WorkspaceGitChanged(Box::new(
            api::WorkspaceGitChangedEvent {
                project_id: Some("one".into()),
                workspace_path: "/one".into(),
                repository_keys: vec![".".into()],
            },
        )));
        assert_eq!(*keys.lock().unwrap(), vec![json!("."), json!(".")]);
        assert!(
            matches!(git_events(&events).last(), Some(BackendEvent::Git(snapshot))
            if snapshot.selected.as_deref() == Some("lib")
               && snapshot.repositories.iter().all(|r| r.changes == Some(2)))
        );
        server.thread.join().unwrap();
    }

    /// A diff that cannot load reports its reason, and a file outside the change list reports
    /// none, so the diff view leaves its loading state either way.
    /// 无法加载的 diff 报告其原因，不在变更清单中的文件不带原因报告，两种情况下 diff 视图都会离开加载状态。
    #[test]
    fn diff_failures_reach_the_diff_view() {
        let server = RpcServer::start(4, |request| {
            match request.body["method"].as_str().unwrap() {
                "workspace.git.catalog" => json!({"ok":true,"result":{"projectRoot":"/one",
                "repositories":[{"repositoryKey":".","repositoryRoot":"/one","name":"one",
                "relativePath":".","isPrimary":true,"monitoringEnabled":true}]}}),
                "workspace.diff.snapshot" => json!({"ok":true,"result":{"currentBranch":"main",
                "files":[{"key":"k","newPath":"a.rs","status":"modified"}]}}),
                "workspace.diff.fileDetail" => {
                    json!({"ok":false,"error":{"message":"Repository is not available"}})
                }
                _ => panic!("unexpected synthetic RPC method"),
            }
        });
        let (mut worker, events, _) = worker_fixture(server.client.clone());
        worker.apply_snapshot(two_project_workbench());
        worker.selected = Some("a".into());
        worker.git_refresh();
        events.try_iter().for_each(drop);
        worker.git_diff(diff_key("a.rs", false));
        worker.git_diff(diff_key("gone.rs", false));
        let failures: Vec<(String, Option<String>)> = events
            .try_iter()
            .filter_map(|event| match event {
                BackendEvent::GitDiffFailed { key, message } => Some((key.path, message)),
                _ => None,
            })
            .collect();
        assert_eq!(
            failures,
            vec![
                ("a.rs".into(), Some("Repository is not available".into())),
                ("gone.rs".into(), None),
            ]
        );
        server.thread.join().unwrap();
    }

    /// Returns an exact diff identity for the synthetic first project's primary repository.
    /// 返回合成首个项目主仓库中由路径与暂存模式确定的差异身份。
    fn diff_key(path: &str, staged: bool) -> GitDiffKey {
        GitDiffKey {
            workspace: "/one".into(),
            repository_key: ".".into(),
            path: path.into(),
            staged,
        }
    }

    /// Failed or malformed refreshes invalidate editable settings until a full snapshot recovers.
    /// 刷新失败或响应格式错误后，使可编辑设置失效，直到完整快照恢复。
    #[test]
    fn workbench_refresh_failure_clears_settings_before_unlock_and_recovers() {
        for failure in [
            json!({"ok":false,"error":{"message":"synthetic refresh failure"}}),
            json!({"ok":true,"result":null}),
        ] {
            // The write succeeds, the first refresh fails, and an explicit retry returns the saved value.
            // 写入成功，首次刷新失败，显式重试返回已保存值。
            let mut updated = two_project_workbench();
            updated.revision += 1;
            updated.git_commit.new_branch_prefix = "saved-prefix".into();
            // Fail only the first refresh so the same transport can exercise recovery.
            // 仅令首次刷新失败，以便同一传输继续验证恢复。
            let mut failed_once = false;
            // Three synthetic requests cover the write, failed refresh and successful retry.
            // 三次合成请求覆盖写入、刷新失败和成功重试。
            let server = RpcServer::start(3, move |request| {
                match request.body["method"].as_str().unwrap() {
                    "service_save_git_commit_settings" => json!({"ok":true,"result":{}}),
                    "workbench.snapshot" if !failed_once => {
                        failed_once = true;
                        failure.clone()
                    }
                    "workbench.snapshot" => json!({"ok":true,"result":updated}),
                    _ => panic!("unexpected synthetic RPC method"),
                }
            });
            // Observe the production worker without connecting to any user service.
            // 观察生产工作器，但不连接任何用户服务。
            let (mut worker, events, input) = worker_fixture(server.client.clone());
            worker.apply_snapshot(two_project_workbench());
            events.try_iter().for_each(drop);
            worker.service_call(settings_actions::save_git_commit_settings(
                json!({"newBranchPrefix":"saved-prefix"}),
            ));
            worker.handle_input(input.recv_timeout(Duration::from_secs(2)).unwrap());
            // Preserve emission order to check that stale editors disappear before unlocking.
            // 保留事件发出顺序，检查旧编辑器在解锁之前消失。
            let received: Vec<_> = events.try_iter().collect();
            // Locate invalidation by behavior rather than a fixed event position.
            // 按行为定位失效事件，而非固定事件位置。
            let cleared = received.iter().position(|event| matches!(event, BackendEvent::Slot { slot: Slot::Workbench, value, .. } if value.is_null())).expect("failed refresh must clear the editable slot");
            // Locate the event that permits another mutation.
            // 定位允许下一次变更的事件。
            let unlocked = received
                .iter()
                .position(|event| matches!(event, BackendEvent::CallFinished))
                .expect("write finishes");
            assert!(
                cleared < unlocked,
                "stale settings must disappear before another write"
            );
            worker.emit_workbench_slot();
            assert!(
                events.try_iter().all(|event| !matches!(
                    event,
                    BackendEvent::Slot {
                        slot: Slot::Workbench,
                        ..
                    }
                )),
                "unrelated deltas must not republish old settings"
            );
            worker.reload_workbench();
            assert!(events.try_iter().any(|event| matches!(event, BackendEvent::Slot { slot: Slot::Workbench, value, .. } if value["gitCommit"]["newBranchPrefix"] == "saved-prefix")));
            server.thread.join().unwrap();
        }
    }

    /// Both versions of a partially staged file use the selected mode; another owner sends no RPC.
    /// 部分暂存文件的两个版本使用各自选中模式；其他归属不发送 RPC。
    #[test]
    fn diff_requests_preserve_mode_and_repository_owner() {
        // Two snapshot modes and two detail modes must remain distinct on the actual transport.
        // 两个快照模式与两个详情模式必须在实际传输上保持独立。
        let server = RpcServer::start(5, |request| {
            match request.body["method"].as_str().unwrap() {
                "workspace.git.catalog" => {
                    json!({"ok":true,"result":{"repositories":[{"repositoryKey":".","name":"one","isPrimary":true,"monitoringEnabled":true}]}})
                }
                "workspace.diff.snapshot" => {
                    json!({"ok":true,"result":{"files":[{"key":request.body["params"]["request"]["mode"],"newPath":"a.rs","status":"modified"}]}})
                }
                "workspace.diff.fileDetail" => {
                    assert_eq!(
                        request.body["params"]["file"]["key"],
                        request.body["params"]["request"]["mode"]
                    );
                    json!({"ok":true,"result":{"unifiedDiff":"@@ -1 +1 @@\n-old\n+new"}})
                }
                _ => panic!("unexpected synthetic RPC method"),
            }
        });
        // Exercise the real worker with a local synthetic transport.
        // 使用本地合成传输验证真实工作器。
        let (mut worker, events, _) = worker_fixture(server.client.clone());
        worker.apply_snapshot(two_project_workbench());
        worker.selected = Some("a".into());
        worker.git_refresh();
        events.try_iter().for_each(drop);
        for staged in [true, false] {
            worker.git_diff(diff_key("a.rs", staged));
        }
        assert_eq!(
            events
                .try_iter()
                .filter_map(|event| match event {
                    BackendEvent::GitDiff { key, .. } => Some(key.staged),
                    _ => None,
                })
                .collect::<Vec<_>>(),
            vec![true, false]
        );
        for key in [
            GitDiffKey {
                workspace: "/two".into(),
                ..diff_key("a.rs", true)
            },
            GitDiffKey {
                repository_key: "nested".into(),
                ..diff_key("a.rs", true)
            },
        ] {
            worker.git_diff(key.clone());
            assert!(events.try_iter().any(|event| matches!(event, BackendEvent::GitDiffFailed { key: rejected, .. } if rejected == key)));
        }
        server.thread.join().unwrap();
    }

    /// A failed active-project refresh remains dirty and retries on returning to that project.
    /// 当前项目刷新失败后保持失效标记，并在回到该项目时重试。
    #[test]
    fn failed_git_refresh_retries_after_project_switch() {
        // Fail the first changed view of project one after its initial pair of successful views.
        // 首个项目的两个初始视图成功后，让首次变更视图读取失败。
        let mut reads = 0;
        // Count only this scenario's catalog and snapshot reads; no real service is contacted.
        // 仅计数此场景的目录和快照读取，不连接真实服务。
        let server = RpcServer::start(9, move |request| {
            match request.body["method"].as_str().unwrap() {
                "workspace.git.catalog" => {
                    json!({"ok":true,"result":{"repositories":[{"repositoryKey":".","name":"root","isPrimary":true,"monitoringEnabled":true}]}})
                }
                "workspace.diff.snapshot" => {
                    if request.body["params"]["workspacePath"] == "/one" {
                        reads += 1;
                        if reads == 3 {
                            return json!({"ok":false,"error":{"message":"synthetic read failure"}});
                        }
                    }
                    json!({"ok":true,"result":{"currentBranch":if reads > 3 {"updated"} else {"initial"},"files":[]}})
                }
                _ => panic!("unexpected synthetic RPC method"),
            }
        });
        // Keep both projects in the production worker's cache throughout the failure and retry.
        // 在失败及重试期间，将两个项目保留在生产工作器缓存中。
        let (mut worker, events, _) = worker_fixture(server.client.clone());
        worker.apply_snapshot(two_project_workbench());
        worker.selected = Some("a".into());
        worker.git_show();
        worker.git_changed("/one", &[".".into()]);
        worker.selected = Some("b".into());
        worker.git_show();
        events.try_iter().for_each(drop);
        worker.selected = Some("a".into());
        worker.git_show();
        assert!(
            events.try_iter().any(
                |event| matches!(event, BackendEvent::Git(snapshot) if snapshot.branch == "updated")
            ),
            "returning must reread a failed active-project update"
        );
        // Successful recovery clears the mark; another display needs no extra RPC.
        // 成功恢复会清除标记；再次显示不需要额外 RPC。
        worker.git_show();
        server.thread.join().unwrap();
    }

    /// Leaving a project keeps its Git state: returning shows memory without a request, and a
    /// change seen while away rereads only the repository it names.
    /// 离开项目会保留其 Git 状态：回到时直接显示内存而不发请求，离开期间的变更只重读其指明的仓库。
    #[test]
    fn projects_return_from_memory_and_reread_only_what_changed() {
        let seen = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let record = seen.clone();
        // Two full loads, then the two views of the repository changed while away.
        // 两次完整加载，随后读取离开期间变化仓库的两个视图。
        let server = RpcServer::start(8, move |request| {
            let workspace = request
                .body
                .pointer("/params/workspacePath")
                .unwrap()
                .clone();
            let method = request.body["method"].as_str().unwrap().to_string();
            record
                .lock()
                .unwrap()
                .push(format!("{method} {}", workspace.as_str().unwrap()));
            match method.as_str() {
                "workspace.git.catalog" => json!({"ok":true,"result":{"projectRoot":workspace,
                    "repositories":[{"repositoryKey":".","repositoryRoot":workspace,"name":"root",
                    "relativePath":".","isPrimary":true,"monitoringEnabled":true}]}}),
                "workspace.diff.snapshot" => json!({"ok":true,"result":{"currentBranch":"main",
                    "files":[{"key":"k","newPath":"a.rs","status":"modified"}]}}),
                _ => panic!("unexpected synthetic RPC method"),
            }
        });
        let (mut worker, events, _) = worker_fixture(server.client.clone());
        worker.apply_snapshot(two_project_workbench());
        let show = |worker: &mut Worker, id: &str| {
            worker.selected = Some(id.into());
            worker.git_show();
        };
        show(&mut worker, "a");
        show(&mut worker, "b");
        assert_eq!(seen.lock().unwrap().len(), 6, "each project loads once");
        worker.git_changed("/one", &[".".into()]);
        assert_eq!(
            seen.lock().unwrap().len(),
            6,
            "a hidden project only marks the change"
        );
        events.try_iter().for_each(drop);
        show(&mut worker, "a");
        let returned = git_events(&events);
        assert!(matches!(returned.first(), Some(BackendEvent::GitCleared)));
        assert!(
            matches!(returned.get(1), Some(BackendEvent::Git(snapshot))
            if snapshot.selected.as_deref() == Some(".")),
            "memory shows before the reread"
        );
        show(&mut worker, "b");
        assert_eq!(
            seen.lock().unwrap()[6..],
            [
                "workspace.diff.snapshot /one".to_string(),
                "workspace.diff.snapshot /one".to_string()
            ]
        );
        server.thread.join().unwrap();
    }
}
