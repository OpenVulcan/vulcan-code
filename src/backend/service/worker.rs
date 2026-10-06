// Service worker: one thread owning the connection, the workbench projection and event windows.
// 服务工作线程：独占连接、工作台投影与事件窗口的单一线程。
//
// Every input (UI command, `/events` frame, socket close, finished background call) arrives on one
// channel, so state is only touched here. Quick RPCs run inline; calls that can take long (model
// backed, network bound) run on short-lived threads and hand their result back as `Input::Done`.
// 所有输入（界面命令、`/events` 帧、套接字关闭、已完成的后台调用）都经由同一通道到达，因此状态只在
// 此处修改。快速 RPC 内联执行；可能耗时较长的调用（依赖模型或网络）在短生命周期线程中执行，结果以
// `Input::Done` 交回。

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{Map, Value, json};

use super::discovery;
use super::events::{self, EventStream};
use super::git::GitState;
use super::map::{self, ModelRoute};
use super::rpc::{RpcClient, RpcError};
use super::wire::*;
use crate::backend::{BackendEvent, Command, ConnectionState, Notice};
use crate::model::*;
use crate::service_api::fresh_command_id as fresh_id;
use crate::service_api::{
    CallKind, ServiceCall, Slot, scrub_secrets, workbench_snapshot::load_workbench_snapshot,
};

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
const WORKBENCH_SLOT_KEYS: [&str; 23] = [
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
    "gitCommit",
    "providerRetry",
    "providerRequestTraceEnabled",
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

/// Everything the worker reacts to.
/// 工作线程响应的全部输入。
pub enum Input {
    /// The UI dropped its command sender; stop the event reader and release worker state.
    /// 界面释放了命令发送器；停止事件读取并释放工作线程状态。
    Shutdown,
    /// One typed UI command forwarded onto the serial worker channel.
    /// 转发到串行工作线程通道的单个类型化界面命令。
    Command(Command),
    /// One `/events` frame of connection `generation`.
    /// 连接代际 `generation` 的一帧 `/events`。
    Service {
        /// Connection generation that received this frame.
        /// 接收该帧的连接代际。
        generation: u64,
        /// Exact WebUI event name from the socket frame.
        /// 套接字帧中的准确 WebUI 事件名称。
        kind: String,
        /// Original event document sanitized before projection.
        /// 投影前执行脱敏的原始事件文档。
        payload: Value,
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
    /// Server event-document generation, distinct from the transport generation.
    /// 服务端事件文档代际，与传输代际独立。
    generation: u64,
    /// Server event epoch identifying the current document lineage.
    /// 标识当前文档谱系的服务端事件纪元。
    epoch: String,
    /// Last authoritative memory revision observed for this conversation.
    /// 该会话最后观察到的权威记忆修订号。
    memory_revision: u64,
    /// Raw `ConversationEventSummary` values by `seq`; text appends patch them in place.
    /// 以 `seq` 为键的原始 `ConversationEventSummary`；文本追加就地修补。
    events: BTreeMap<u64, Value>,
}

/// A conversation that exists only in the TUI until its first prompt is accepted.
/// 首条提示被接受前只存在于 TUI 的会话。
struct Draft {
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
    /// Latest workbench snapshot with deltas folded in.
    /// 折叠了增量的最新工作台快照。
    pub(super) wb: Value,
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
    /// Last loader per slot, so mutations can reload what they invalidate.
    /// 每个槽位最近一次的加载器，供变更重新加载其失效的槽位。
    loaders: HashMap<Slot, ServiceCall>,
    /// Dispatch sequence per slot; stale responses cannot overwrite a later selection or reload.
    /// 每个槽位的发送序号；过期响应无法覆盖较新的选择或刷新。
    load_sequences: HashMap<Slot, u64>,
    /// Latest job sequence per project and job; older socket/read results are ignored.
    /// 每个项目及任务的最新序号；忽略较旧套接字或读取结果。
    vce_job_sequences: HashMap<(String, String), u64>,
    /// Last emitted workbench slot, to emit only changes.
    /// 最近一次发出的工作台槽位，只在变化时发出。
    workbench_slot: Value,
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
    let mut worker = Worker::new(out, inbox, root);
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
            wb: Value::Null,
            revision: 0,
            selected: None,
            windows: HashMap::new(),
            drafts: HashMap::new(),
            draft_serial: 0,
            model_keys: HashMap::new(),
            git: GitState::default(),
            loaders: HashMap::new(),
            load_sequences: HashMap::new(),
            vce_job_sequences: HashMap::new(),
            workbench_slot: Value::Null,
        }
    }

    /// Dispatches one generation-checked input; false requests complete worker shutdown.
    /// 分发一次检查代际后的输入；返回 false 请求完整停止工作线程。
    fn handle_input(&mut self, input: Input) -> bool {
        match input {
            Input::Shutdown => return false,
            Input::Command(command) => self.handle(command),
            Input::Service {
                generation,
                kind,
                payload,
            } if generation == self.generation => self.service_event(&kind, payload),
            Input::Service { .. } => {}
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
    pub(super) fn toast(&self, severity: Severity, message: impl Into<String>) {
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
                self.emit(BackendEvent::Connection(ConnectionState::Connected));
            }
            Err(message) => {
                self.client = None;
                if let Some(stream) = self.stream.take() {
                    stream.stop();
                }
                if self.connect_error.as_ref() != Some(&message) {
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
        self.client = Some(client);
        // A new service instance may have reset revisions and event documents.
        // 新的服务实例可能已重置版本号与事件文档。
        self.revision = 0;
        self.windows.clear();
        self.vce_job_sequences.clear();
        self.apply_snapshot(snapshot);
        if let Some(id) = self.selected.clone()
            && !is_draft(&id)
        {
            self.hydrate(&id);
        }
        self.git_refresh();
        Ok(())
    }

    /// Drops the active transport and schedules bounded service reconnection.
    /// 释放活动传输并安排有界服务重连。
    fn disconnected(&mut self) {
        if let Some(stream) = self.stream.take() {
            stream.stop();
        }
        if self.client.take().is_some() || self.retry_at.is_none() {
            self.emit(BackendEvent::Connection(ConnectionState::Reconnecting));
        }
        self.schedule_retry();
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

    /// Reports a sanitized RPC failure and reconnects on transport loss.
    /// 报告脱敏 RPC 失败，在传输丢失时重连。
    /// `error` is the already-sanitized transport/service error.
    /// `error` 为已经脱敏的传输或服务错误。
    pub(super) fn failed(&mut self, error: RpcError) {
        self.toast(Severity::Error, error.message.clone());
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
            self.emit(BackendEvent::Notice(Notice::NotConnected));
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
                if slot == Slot::VceJob && !value.is_null() {
                    if let (Some(project), Some(job), Some(sequence)) = (
                        value.get("projectId").and_then(Value::as_str),
                        value.get("jobId").and_then(Value::as_str),
                        value.get("sequence").and_then(Value::as_u64),
                    ) {
                        // Identity.
                        // 准确协议身份。
                        let identity = (project.to_string(), job.to_string());
                        if self
                            .vce_job_sequences
                            .get(&identity)
                            .is_some_and(|last| *last > sequence)
                        {
                            return;
                        }
                        self.vce_job_sequences.insert(identity, sequence);
                    } else {
                        self.toast(Severity::Error, "Invalid VCE job response");
                        return;
                    }
                }
                self.emit(BackendEvent::Slot { slot, call, value });
            }
            CallKind::Mutate(slots) => {
                // Snapshot.
                // 权威快照。
                let snapshot = is_snapshot(&value);
                self.absorb(&value);
                for slot in slots {
                    match slot {
                        Slot::Workbench if !snapshot => self.reload_workbench(),
                        Slot::Workbench => {}
                        _ => {
                            if let Some(loader) = self.loaders.get(slot).cloned() {
                                // Publish authoritative refreshes before unlocking another full-document edit.
                                // 解锁另一份完整文档编辑前发布权威刷新。
                                *self.load_sequences.entry(*slot).or_default() += 1;
                                match self.call(loader.method, loader.params.clone()) {
                                    Some(value) => self.call_done(loader, value),
                                    None => self.emit(BackendEvent::Slot {
                                        slot: *slot,
                                        call: loader,
                                        value: Value::Null,
                                    }),
                                }
                            }
                        }
                    }
                }
                self.emit(BackendEvent::Notice(Notice::Completed(call.function)));
            }
            CallKind::Preview(_) => self.emit(BackendEvent::Preview {
                call,
                result: value,
            }),
        }
    }

    /// Reloads the service workbench and emits changed UI projections.
    /// 重新加载服务工作台并发布变化的界面投影。
    fn reload_workbench(&mut self) {
        // Exact service call.
        // 准确服务调用。
        let call = load_workbench_snapshot();
        if let Some(snapshot) = self.call(call.method, call.params) {
            self.apply_snapshot(snapshot);
        }
    }

    /// Emits the workbench slot when the fields system areas read have changed.
    /// 系统区域读取的字段变化时发出工作台槽位。
    pub(super) fn emit_workbench_slot(&mut self) {
        // Subset.
        // 页面所需工作台字段子集。
        let mut subset = Map::new();
        for key in WORKBENCH_SLOT_KEYS {
            if let Some(value) = self.wb.get(key) {
                subset.insert(key.to_string(), value.clone());
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

    /// Folds an RPC result in when it is a workbench snapshot or a session addon response.
    /// RPC 结果为工作台快照或会话附属状态响应时将其折叠进来。
    /// `value` is the protocol value being read or projected.
    /// `value` 为正在读取或投影的协议值。
    pub(super) fn absorb(&mut self, value: &Value) {
        if is_snapshot(value) {
            self.apply_snapshot(value.clone());
        } else if value.get("conversationId").is_some() && value.get("revision").is_some() {
            // Revision.
            // 权威修订号。
            let revision = u64_at(value, "revision");
            if revision >= self.revision {
                self.revision = revision;
                self.upsert_addons(value.clone());
            }
        }
    }

    /// Accepts a sanitized authoritative workbench snapshot and updates typed projections.
    /// 接受脱敏权威工作台快照并更新类型化投影。
    /// `snapshot` is the authoritative service snapshot.
    /// `snapshot` 为权威服务快照。
    fn apply_snapshot(&mut self, mut snapshot: Value) {
        // Revision.
        // 权威修订号。
        let revision = u64_at(&snapshot, "revision");
        if revision < self.revision {
            return;
        }
        self.revision = revision;
        scrub_secrets(&mut snapshot);
        self.wb = snapshot;
        if self.selected.is_none() {
            self.selected = opt_str(&self.wb, "selectedConversationId");
        }
        // Models; Active.
        // 可选模型目录；是否处于活动状态。
        let (models, active) = map::models(arr(&self.wb, "providers"));
        // Selected model id.
        // 所选模型路线身份。
        let selected_model_id = self
            .selected
            .as_ref()
            .and_then(|id| self.model_keys.get(id).cloned())
            .or(active)
            .unwrap_or_default();
        self.emit(BackendEvent::Workbench(WorkbenchSnapshot {
            projects: map::projects(arr(&self.wb, "projects")),
            conversations: self.conversation_rows(),
            selected_conversation_id: self.selected.clone(),
            models,
            selected_model_id,
            ui_locale: opt_str(&self.wb, "uiLocale"),
            base_url: self
                .client
                .as_ref()
                .map(|c| c.base_url().to_string())
                .unwrap_or_default(),
        }));
        self.emit(BackendEvent::CommandSessions(map::command_sessions(arr(
            &self.wb,
            "commandSessions",
        ))));
        // Ids.
        // 准确身份集合。
        let mut ids: Vec<String> = arr(&self.wb, "pendingQuestions")
            .iter()
            .filter_map(|q| opt_str(q, "sessionId"))
            .collect();
        ids.extend(self.selected.clone());
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
    /// Returns `Vec<ConversationSummary>`.
    /// 返回 `Vec<ConversationSummary>`。
    fn conversation_rows(&self) -> Vec<ConversationSummary> {
        // Current operation rows.
        // 当前操作的行集合。
        let mut rows = map::conversations(arr(&self.wb, "conversations"));
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
    /// Returns `Option<ConversationSummary>`.
    /// 返回 `Option<ConversationSummary>`。
    fn summary(&self, id: &str) -> Option<ConversationSummary> {
        // Current row.
        // 当前条目。
        let mut row = arr(&self.wb, "conversations")
            .iter()
            .find(|c| str_at(c, "id") == id)
            .and_then(map::conversation)?;
        row.attention |= self.question_of(id).is_some();
        Some(row)
    }

    /// Finds the service-owned pending question of the exact conversation.
    /// 查找准确会话中由服务持有的待回复提问。
    /// `id` is the exact entity identity.
    /// `id` 为准确实体身份。
    /// Returns `Option<&Value>`.
    /// 返回 `Option<&Value>`。
    fn question_of(&self, id: &str) -> Option<&Value> {
        arr(&self.wb, "pendingQuestions")
            .iter()
            .find(|q| str_at(q, "sessionId") == id)
    }

    /// Entry of a `[{conversationId, ...}]` list in the workbench.
    /// 工作台中 `[{conversationId, ...}]` 列表的条目。
    /// `list` is the protocol row collection.
    /// `list` 为协议行集合。
    /// `id` is the exact entity identity.
    /// `id` 为准确实体身份。
    /// Returns `Option<&Value>`.
    /// 返回 `Option<&Value>`。
    fn entry(&self, list: &str, id: &str) -> Option<&Value> {
        arr(&self.wb, list)
            .iter()
            .find(|e| str_at(e, "conversationId") == id)
    }

    /// Publishes the exact current pending question for a selected conversation.
    /// 发布所选会话当前准确的待回复提问。
    /// `id` is the exact entity identity.
    /// `id` 为准确实体身份。
    fn emit_question(&self, id: &str) {
        self.emit(BackendEvent::PendingQuestion {
            conversation_id: id.to_string(),
            question: self.question_of(id).and_then(map::pending_question),
        });
    }

    /// Everything the workbench knows about one conversation besides its events.
    /// 工作台所知的单个会话除事件外的全部状态。
    /// `id` is the exact entity identity.
    /// `id` 为准确实体身份。
    fn emit_conversation_state(&self, id: &str) {
        // Conversation id.
        // 准确会话身份。
        let conversation_id = id.to_string();
        self.emit(BackendEvent::PendingPrompts {
            conversation_id: conversation_id.clone(),
            prompts: self
                .entry("pendingPromptsByConversation", id)
                .map(|e| map::pending_prompts(arr(e, "pendingPrompts")))
                .unwrap_or_default(),
        });
        self.emit_addons(id);
        self.emit_agents(id);
        if self.service_selected(id) {
            self.emit(BackendEvent::ContextUsage {
                conversation_id,
                usage: map::context_usage(at(&self.wb, "contextUsage")),
            });
        }
    }

    /// Top-level selected-conversation fields only describe the service's own selection.
    /// 顶层的选中会话字段只描述服务自身的选中项。
    /// `id` is the exact entity identity.
    /// `id` 为准确实体身份。
    /// Returns `bool`.
    /// 返回 `bool`。
    fn service_selected(&self, id: &str) -> bool {
        str_at(&self.wb, "selectedConversationId") == id
    }

    /// Publishes the current conversation planning and command-session add-ons.
    /// 发布当前会话的规划及命令会话附加信息。
    /// `id` is the exact entity identity.
    /// `id` 为准确实体身份。
    fn emit_addons(&self, id: &str) {
        // Addons.
        // 会话附加状态。
        let addons = self.entry("sessionAddonsByConversation", id);
        // Conversation id.
        // 准确会话身份。
        let conversation_id = id.to_string();
        self.emit(BackendEvent::Todo {
            conversation_id: conversation_id.clone(),
            items: addons
                .map(|a| map::todo(at(a, "sessionTodolist")))
                .unwrap_or_default(),
        });
        self.emit(BackendEvent::PlanMode {
            conversation_id,
            mode: addons
                .map(|a| map::plan_mode(at(a, "sessionPlanMode")))
                .unwrap_or(ExecutionMode::Build),
        });
        self.emit_insight(id);
    }

    /// Projects current conversation context, memory and usage into the session panel.
    /// 将当前会话上下文、记忆及用量投影到会话面板。
    /// `id` is the exact entity identity.
    /// `id` 为准确实体身份。
    fn emit_insight(&self, id: &str) {
        // Addons.
        // 会话附加状态。
        let addons = self.entry("sessionAddonsByConversation", id);
        // Usage.
        // 已发布用量。
        let usage = addons.map(|a| at(a, "sessionProviderUsage"));
        // Lifetime.
        // 会话累计用量。
        let mut lifetime = usage.map(map::lifetime).unwrap_or_default();
        // Child usage is keyed by child session id; show the subagent label where known.
        // 子项用量以子会话 ID 为键；已知时显示子代理标签。
        let navigation = self.navigation(id);
        for (key, _) in &mut lifetime.children {
            if let Some(task) = navigation
                .map(|n| arr(n, "tasks"))
                .unwrap_or_default()
                .iter()
                .find(|t| str_at(t, "taskId") == *key)
            {
                *key = str_at(task, "label");
            }
        }
        // Breakdown.
        // 上下文组成明细。
        let breakdown = if self.service_selected(id) {
            map::breakdown(at(&self.wb, "contextUsage"))
        } else {
            Vec::new()
        };
        // Insight.
        // 会话面板状态。
        let insight = SessionInsight {
            memory: self
                .entry("contextInjectionByConversation", id)
                .map(|e| map::context_injection(at(e, "contextInjection")))
                .unwrap_or_default(),
            lifetime,
            breakdown,
            goal: addons.and_then(|a| map::goal(at(a, "sessionGoal"))),
        };
        self.emit(BackendEvent::SessionInsight {
            conversation_id: id.to_string(),
            insight: Box::new(insight),
        });
    }

    /// Finds the service subagent navigation for the exact parent conversation.
    /// 查找准确父会话的服务子代理导航。
    /// `id` is the exact entity identity.
    /// `id` 为准确实体身份。
    /// Returns `Option<&Value>`.
    /// 返回 `Option<&Value>`。
    fn navigation(&self, id: &str) -> Option<&Value> {
        arr(&self.wb, "subagentNavigationByConversation")
            .iter()
            .find(|n| str_at(n, "parentConversationId") == id)
            .or_else(|| {
                // Nav.
                // 当前导航状态。
                let nav = at(&self.wb, "subagentNavigation");
                (str_at(nav, "parentConversationId") == id).then_some(nav)
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

    /// Reasoning effort choices of the service-selected conversation's model.
    /// 服务选中会话所用模型的推理强度选项。
    fn emit_effort(&self) {
        // Exact entity identity.
        // 准确实体身份。
        let Some(id) = self.selected.clone() else {
            return;
        };
        // Model id.
        // 准确模型身份。
        let model_id = self.model_keys.get(&id).cloned().unwrap_or_default();
        // Selection.
        // 已批准选择。
        let selection = at(&self.wb, "reasoningSelection");
        // Efforts.
        // 模型准入的推理强度列表。
        let mut efforts: Vec<ReasoningEffort> = arr(selection, "options")
            .iter()
            .filter_map(|o| map::effort(&str_at(o, "value")))
            .collect();
        efforts.dedup();
        // Effort.
        // 当前推理强度。
        let effort = map::effort(&str_at(selection, "selectedEffort"));
        self.emit(BackendEvent::ModelSelection {
            model_id,
            effort,
            efforts,
        });
    }

    /// Workbench value for an effort, picked from the current model's own options.
    /// 推理强度对应的工作台取值，从当前模型自身的选项中挑选。
    /// Returns `Option<String>`.
    /// 返回 `Option<String>`。
    fn effort_value(&self, effort: ReasoningEffort) -> Option<String> {
        arr(&self.wb, "reasoningSelection.options")
            .iter()
            .map(|o| str_at(o, "value"))
            .find(|v| map::effort(v) == Some(effort))
    }

    /// Updates one exact workbench field and emits its changed page projection.
    /// 更新一个准确工作台字段并发布变化的页面投影。
    /// `value` is the protocol value being read or projected.
    /// `value` 为正在读取或投影的协议值。
    fn set_wb(&mut self, key: &str, value: Value) {
        if let Some(object) = self.wb.as_object_mut() {
            object.insert(key.to_string(), value);
        }
    }

    /// Replaces one `[{conversationId, ...}]` entry; `None` removes it.
    /// 替换一个 `[{conversationId, ...}]` 条目；`None` 表示移除。
    /// `list` is the protocol row collection.
    /// `list` 为协议行集合。
    /// `id` is the exact entity identity.
    /// `id` 为准确实体身份。
    fn upsert_entry(&mut self, list: &str, id: &str, entry: Option<Value>) {
        // Items.
        // 当前条目集合。
        let mut items: Vec<Value> = arr(&self.wb, list)
            .iter()
            .filter(|e| str_at(e, "conversationId") != id)
            .cloned()
            .collect();
        items.extend(entry);
        self.set_wb(list, Value::Array(items));
    }

    /// Replaces the exact conversation add-on row in the workbench authority.
    /// 替换工作台权威数据中的准确会话附加信息行。
    fn upsert_addons(&mut self, entry: Value) {
        // Exact entity identity.
        // 准确实体身份。
        let id = str_at(&entry, "conversationId");
        // Present.
        // 是否存在已观察状态。
        let present = [
            "sessionGoal",
            "sessionTodolist",
            "sessionPlanMode",
            "sessionOutputStyle",
            "sessionContextCompactionWindowNodeId",
            "sessionProviderUsage",
        ]
        .iter()
        .any(|k| !at(&entry, k).is_null());
        self.upsert_entry("sessionAddonsByConversation", &id, present.then_some(entry));
        self.emit_addons(&id);
    }

    /// Applies one verified field update to an existing conversation row.
    /// 对已有会话行应用一次已确认字段更新。
    /// `id` is the exact entity identity.
    /// `id` 为准确实体身份。
    /// `patch` is the exact delta or field update to fold.
    /// `patch` 为需要折叠的准确增量或字段更新。
    fn patch_conversation(&mut self, id: &str, patch: impl FnOnce(&mut Map<String, Value>)) {
        // List.
        // 当前列表。
        let Some(list) = self
            .wb
            .get_mut("conversations")
            .and_then(Value::as_array_mut)
        else {
            return;
        };
        if let Some(object) = list
            .iter_mut()
            .find(|c| str_at(c, "id") == id)
            .and_then(Value::as_object_mut)
        {
            patch(object);
        }
        if let Some(row) = self.summary(id) {
            self.emit(BackendEvent::ConversationSummary(row));
        }
    }

    /// Removes exact conversation identities and their cached state.
    /// 移除准确会话身份及其缓存状态。
    /// `ids` is the exact selected identity set.
    /// `ids` 为准确的所选身份集合。
    fn remove_conversations(&mut self, ids: &[String]) {
        if let Some(list) = self
            .wb
            .get_mut("conversations")
            .and_then(Value::as_array_mut)
        {
            list.retain(|c| !ids.contains(&str_at(c, "id")));
        }
        for id in ids {
            self.windows.remove(id);
            if self.selected.as_ref() == Some(id) {
                self.selected = None;
            }
            self.emit(BackendEvent::ConversationRemoved(id.clone()));
        }
    }

    // ---- Service events ----

    /// Dispatches a sanitized service event to its protocol-defined state handler.
    /// 将脱敏服务事件分发到其协议规定的状态处理器。
    /// `kind` is the declared event or operation family.
    /// `kind` 为已声明的事件或操作类别。
    /// `payload` is the declared request or event payload.
    /// `payload` 为已声明的请求或事件载荷。
    fn service_event(&mut self, kind: &str, mut payload: Value) {
        scrub_secrets(&mut payload);
        match kind {
            "workbench" => self.apply_snapshot(payload),
            "workbenchDelta" => self.workbench_delta(payload),
            "conversationEventDeltaV1" => self.event_delta(payload),
            "workspaceGitChanged" => {
                if self.git.matches(&str_at(&payload, "workspacePath")) {
                    self.git_refresh();
                }
            }
            "sessionAddonChanged" => {
                if !at(&payload, "conversationId").is_null() {
                    self.upsert_addons(payload);
                }
            }
            "vceProjectChanged" => {
                for slot in [Slot::VceOverview, Slot::VceStatus] {
                    if let Some(loader) = self.loaders.get(&slot).cloned()
                        && (slot == Slot::VceStatus
                            || loader.param("/request/projectId") == &payload["projectId"])
                    {
                        self.service_call(loader);
                    }
                }
            }
            "vceJobChanged" => {
                if let Some(loader) = self.loaders.get(&Slot::VceJob).cloned()
                    && loader.param("/request/projectId") == &payload["projectId"]
                {
                    // Project.
                    // 所选项目数据。
                    let Some(project) = payload.get("projectId").and_then(Value::as_str) else {
                        return;
                    };
                    // Job.
                    // 持久任务回执。
                    let Some(job) = payload.get("jobId").and_then(Value::as_str) else {
                        return;
                    };
                    // Sequence.
                    // 单调序号。
                    let Some(sequence) = payload.get("sequence").and_then(Value::as_u64) else {
                        return;
                    };
                    // Identity.
                    // 准确协议身份。
                    let identity = (project.to_string(), job.to_string());
                    if self
                        .vce_job_sequences
                        .get(&identity)
                        .is_some_and(|last| *last >= sequence)
                    {
                        return;
                    }
                    self.vce_job_sequences.insert(identity, sequence);
                    // An event is newer authority than any previously dispatched job read.
                    // 事件的权威状态新于此前发送的任务读取。
                    *self.load_sequences.entry(Slot::VceJob).or_default() += 1;
                    self.emit(BackendEvent::Slot {
                        slot: Slot::VceJob,
                        call: loader,
                        value: payload.clone(),
                    });
                    if matches!(
                        payload.get("state").and_then(Value::as_str),
                        Some("succeeded" | "failed" | "cancelled" | "interrupted")
                    ) && let Some(overview) = self.loaders.get(&Slot::VceOverview).cloned()
                    {
                        self.service_call(overview);
                    }
                }
            }
            _ => {}
        }
    }

    /// Folds revision-checked workbench deltas without inventing replacement fields.
    /// 折叠检查修订后的工作台增量，不虚构替代字段。
    /// `envelope` is the service revision/delta envelope.
    /// `envelope` 为服务修订或增量信封。
    fn workbench_delta(&mut self, envelope: Value) {
        // Generation.
        // 当前连接或文档代际。
        let generation = u64_at(&envelope, "generation");
        // Kind.
        // 已声明的操作类别。
        let kind = str_at(&envelope, "delta.kind");
        // Independent.
        // 是否为独立生命周期事件。
        let independent = matches!(
            kind.as_str(),
            "conversationArchived"
                | "conversationsArchived"
                | "projectRemoved"
                | "agentSkillsCatalogChanged"
                | "luaSkillsCatalogChanged"
                | "subagentCatalogChanged"
                | "pluginCatalogChanged"
                | "projectSettingsChanged"
                | "luaSkillsManagedRuntimeProgressChanged"
                | "luaSkillsOperationProgressChanged"
        );
        if !independent && generation <= self.revision {
            return;
        }
        self.revision = self.revision.max(generation);
        // Payload.
        // 已声明载荷。
        let payload = at(&envelope, "delta.payload").clone();
        match kind.as_str() {
            "updateConversationRunPhase" => {
                // Exact entity identity.
                // 准确实体身份。
                let id = str_at(&payload, "conversationId");
                self.patch_conversation(&id, |c| {
                    for key in ["runPhase", "status", "lastUpdatedLabel"] {
                        if let Some(value) = payload.get(key).filter(|v| !v.is_null()) {
                            c.insert(key.into(), value.clone());
                        }
                    }
                });
            }
            "pendingPromptsChanged" => {
                // Exact entity identity.
                // 准确实体身份。
                let id = str_at(&payload, "conversationId");
                self.upsert_entry("pendingPromptsByConversation", &id, Some(payload.clone()));
                self.emit(BackendEvent::PendingPrompts {
                    conversation_id: id,
                    prompts: map::pending_prompts(arr(&payload, "pendingPrompts")),
                });
            }
            "conversationSummaryChanged" => {
                // Conversation.
                // 当前会话数据。
                let conversation = at(&payload, "conversation").clone();
                // Exact entity identity.
                // 准确实体身份。
                let id = str_at(&conversation, "id");
                if let Some(list) = self
                    .wb
                    .get_mut("conversations")
                    .and_then(Value::as_array_mut)
                {
                    match list.iter_mut().find(|c| str_at(c, "id") == id) {
                        Some(row) => *row = conversation,
                        None => list.insert(0, conversation),
                    }
                }
                if let Some(row) = self.summary(&id) {
                    self.emit(BackendEvent::ConversationSummary(row));
                }
            }
            "conversationArchived" => {
                self.remove_conversations(&[str_at(&payload, "conversationId")]);
                self.emit_workbench_slot();
            }
            "conversationsArchived" => {
                // Ids.
                // 准确身份集合。
                let ids: Vec<String> = arr(&payload, "archivedConversations")
                    .iter()
                    .map(|c| str_at(c, "id"))
                    .collect();
                self.remove_conversations(&ids);
                self.emit_workbench_slot();
            }
            "projectRemoved" => {
                // Project id.
                // 准确宿主项目身份。
                let project_id = str_at(&payload, "projectId");
                // Ids.
                // 准确身份集合。
                let ids: Vec<String> = arr(&self.wb, "conversations")
                    .iter()
                    .filter(|c| str_at(c, "projectId") == project_id)
                    .map(|c| str_at(c, "id"))
                    .collect();
                self.remove_conversations(&ids);
                self.set_wb("projects", at(&payload, "projects").clone());
                self.emit(BackendEvent::Projects(map::projects(arr(
                    &payload, "projects",
                ))));
                self.emit_workbench_slot();
            }
            "pendingQuestionChanged" => {
                // Exact entity identity.
                // 准确实体身份。
                let id = opt_str(&payload, "conversationId");
                // Questions.
                // 已发布待回复提问。
                let questions = match payload.get("pendingQuestions").filter(|v| v.is_array()) {
                    Some(list) => list.clone(),
                    None => {
                        // List.
                        // 当前列表。
                        let mut list: Vec<Value> = arr(&self.wb, "pendingQuestions")
                            .iter()
                            .filter(|q| {
                                id.as_deref().is_some_and(|id| str_at(q, "sessionId") != id)
                            })
                            .cloned()
                            .collect();
                        list.extend(
                            payload
                                .get("pendingQuestion")
                                .filter(|v| !v.is_null())
                                .cloned(),
                        );
                        Value::Array(list)
                    }
                };
                // Before.
                // 修改前观察值。
                let before: Vec<String> = arr(&self.wb, "pendingQuestions")
                    .iter()
                    .map(|q| str_at(q, "sessionId"))
                    .collect();
                self.set_wb("pendingQuestions", questions);
                // Touched.
                // 受增量影响的身份集合。
                let mut touched: Vec<String> = arr(&self.wb, "pendingQuestions")
                    .iter()
                    .map(|q| str_at(q, "sessionId"))
                    .chain(before)
                    .chain(id)
                    .collect();
                touched.sort();
                touched.dedup();
                for id in touched {
                    self.emit_question(&id);
                    if let Some(row) = self.summary(&id) {
                        self.emit(BackendEvent::ConversationSummary(row));
                    }
                }
            }
            "commandSessionsChanged" => {
                self.set_wb("commandSessions", at(&payload, "commandSessions").clone());
                self.emit(BackendEvent::CommandSessions(map::command_sessions(arr(
                    &payload,
                    "commandSessions",
                ))));
            }
            "subagentNavigationChanged" => {
                // Navigation.
                // 子代理导航状态。
                let navigation = at(&payload, "subagentNavigation").clone();
                // Parent.
                // 准确父会话身份。
                let Some(parent) = opt_str(&payload, "parentConversationId")
                    .or_else(|| opt_str(&navigation, "parentConversationId"))
                else {
                    return;
                };
                // List.
                // 当前列表。
                let mut list: Vec<Value> = arr(&self.wb, "subagentNavigationByConversation")
                    .iter()
                    .filter(|n| str_at(n, "parentConversationId") != parent)
                    .cloned()
                    .collect();
                if !navigation.is_null() {
                    list.push(navigation);
                }
                self.set_wb("subagentNavigationByConversation", Value::Array(list));
                if str_at(&self.wb, "subagentNavigation.parentConversationId") == parent {
                    self.set_wb("subagentNavigation", Value::Null);
                }
                self.emit_agents(&parent);
                self.emit_insight(&parent);
            }
            "contextUsageChanged" => {
                self.set_wb("contextUsage", at(&payload, "contextUsage").clone());
                // Exact entity identity.
                // 准确实体身份。
                let id = str_at(&self.wb, "selectedConversationId");
                if !id.is_empty() {
                    self.emit(BackendEvent::ContextUsage {
                        conversation_id: id.clone(),
                        usage: map::context_usage(at(&self.wb, "contextUsage")),
                    });
                    self.emit_insight(&id);
                }
            }
            "contextInjectionChanged" => {
                // Exact entity identity.
                // 准确实体身份。
                let id = str_at(&payload, "conversationId");
                // Entry.
                // 当前目录条目。
                let entry = (!at(&payload, "contextInjection").is_null()).then(|| payload.clone());
                self.upsert_entry("contextInjectionByConversation", &id, entry);
                self.emit_insight(&id);
            }
            "sessionAddonsChanged" => self.upsert_addons(payload),
            "agentSkillsCatalogChanged" => {
                self.set_wb("agentSkills", at(&payload, "agentSkills").clone());
                self.emit_workbench_slot();
            }
            "luaSkillsCatalogChanged" => {
                self.set_wb("luaSkills", at(&payload, "luaSkills").clone());
                self.set_wb("plugins", at(&payload, "plugins").clone());
                self.emit_workbench_slot();
            }
            "subagentCatalogChanged" => {
                self.set_wb("subagentCatalog", at(&payload, "subagentCatalog").clone());
                self.emit_workbench_slot();
            }
            "pluginCatalogChanged" => {
                for key in ["luaSkills", "plugins", "agentSkills", "subagentCatalog"] {
                    self.set_wb(key, at(&payload, key).clone());
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
        // Result.
        // 服务结果。
        let Some(result) = self.call(
            "service_session_event_hydration_v1",
            json!({ "sessionId": id }),
        ) else {
            return;
        };
        // Events.
        // 事件集合或事件接收通道。
        let events: BTreeMap<u64, Value> = arr(&result, "events")
            .iter()
            .filter_map(|e| Some((opt_u64(e, "seq")?, e.clone())))
            .collect();
        self.windows.insert(
            id.to_string(),
            Window {
                generation: u64_at(&result, "window_generation"),
                epoch: str_at(&result, "service_epoch"),
                memory_revision: u64_at(&result, "memory_revision"),
                events: events.clone(),
            },
        );
        self.emit(BackendEvent::ConversationHydrated {
            conversation_id: id.to_string(),
            events: events.values().filter_map(map::event).collect(),
        });
        // Config.
        // 权威配置。
        let config = at(&result, "session_config");
        // Exact protocol request.
        // 协议请求文档。
        let request = if at(config, "next_request").is_null() {
            config.clone()
        } else {
            json!({ "current_request": at(config, "next_request") })
        };
        if let Some(key) = map::session_route(&request) {
            self.model_keys.insert(id.to_string(), key);
        }
        if self.selected.as_deref() == Some(id) {
            self.emit_effort();
        }
    }

    /// Applies one `conversationEventDeltaV1` patch; any inconsistency reloads the window.
    /// 应用一份 `conversationEventDeltaV1` 补丁；任何不一致都会重新加载窗口。
    /// `patch` is the exact delta or field update to fold.
    /// `patch` 为需要折叠的准确增量或字段更新。
    fn event_delta(&mut self, patch: Value) {
        // Exact entity identity.
        // 准确实体身份。
        let id = str_at(&patch, "session_id");
        // Window.
        // 会话事件窗口。
        let Some(window) = self.windows.get_mut(&id) else {
            return;
        };
        if normalized(&str_at(&patch, "durability")) == "durable" {
            return;
        }
        // Revision.
        // 权威修订号。
        let revision = u64_at(&patch, "revision");
        // Stale owner.
        // 是否来自旧归属。
        let stale_owner = u64_at(&patch, "window_generation") != window.generation
            || str_at(&patch, "service_epoch") != window.epoch;
        // Gap.
        // 是否存在事件修订缺口。
        let gap = revision > window.memory_revision
            && u64_at(&patch, "base_memory_revision") > window.memory_revision;
        if stale_owner || gap {
            self.hydrate(&id);
            return;
        }
        if revision < window.memory_revision {
            return;
        }
        window.memory_revision = revision;
        // Earliest.
        // 最早需要加载的事件序号。
        let earliest = window.events.keys().next().copied();
        // Changed.
        // 是否有权威状态变更。
        let mut changed: Vec<u64> = Vec::new();
        // Broken.
        // 是否存在损坏的窗口状态。
        let mut broken = false;
        for summary in arr(&patch, "events") {
            // Seq.
            // 事件序号。
            let Some(seq) = opt_u64(summary, "seq") else {
                continue;
            };
            if earliest.is_some_and(|first| seq < first) && !window.events.contains_key(&seq) {
                continue;
            }
            // Newer.
            // 是否为更高事件代际。
            let newer = window
                .events
                .get(&seq)
                .is_none_or(|e| u64_at(summary, "event_revision") >= u64_at(e, "event_revision"));
            if newer {
                window.events.insert(seq, summary.clone());
                changed.push(seq);
            }
        }
        for append in arr(&patch, "text_appends") {
            // Seq.
            // 事件序号。
            let seq = u64_at(append, "seq");
            // Existing.
            // 已有权威条目。
            let Some(existing) = window.events.get_mut(&seq) else {
                if earliest.is_none_or(|first| seq >= first) {
                    broken = true;
                }
                continue;
            };
            if u64_at(append, "event_revision") <= u64_at(existing, "event_revision") {
                continue;
            }
            // Base.
            // 编辑前的权威文档。
            let base = u64_at(append, "base_text_utf16_len") as usize;
            // Text.
            // 当前正文。
            let text = format!(
                "{}{}",
                utf16_prefix(
                    at(existing, "event.text").as_str().unwrap_or_default(),
                    base
                ),
                str_at(append, "appended_text")
            );
            if let Some(object) = existing.as_object_mut() {
                object.insert(
                    "event_revision".into(),
                    at(append, "event_revision").clone(),
                );
                object.insert("updated_at_ms".into(), at(append, "updated_at_ms").clone());
            }
            if let Some(event) = existing.get_mut("event").and_then(Value::as_object_mut) {
                event.insert("text".into(), Value::String(text));
            }
            changed.push(seq);
        }
        if broken {
            self.hydrate(&id);
            return;
        }
        changed.sort_unstable();
        changed.dedup();
        // Events.
        // 事件集合或事件接收通道。
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
                    model_key: self
                        .selected
                        .as_ref()
                        .and_then(|s| self.model_keys.get(s).cloned()),
                    project_id,
                };
                self.emit(BackendEvent::ConversationSummary(draft_row(&id, &draft)));
                self.drafts.insert(id.clone(), draft);
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
                if let Some(result) = self.call(
                    "service_stop_conversation_run",
                    json!({ "conversationId": conversation_id, "mode": "main_and_subagents" }),
                ) {
                    self.absorb(&result);
                }
            }
            Command::ReplyPendingQuestion {
                request_id,
                answers,
            } => {
                if let Some(result) = self.call(
                    "service_reply_pending_question",
                    json!({ "requestId": request_id, "answers": answers }),
                ) {
                    self.absorb(&result);
                }
            }
            Command::RejectPendingQuestion { request_id } => {
                if let Some(result) = self.call(
                    "service_reject_pending_question",
                    json!({ "requestId": request_id }),
                ) {
                    self.absorb(&result);
                }
            }
            Command::SetSessionPlanMode {
                conversation_id,
                enabled,
            } => {
                if is_draft(&conversation_id) {
                    return;
                }
                if let Some(result) = self.call(
                    "service_set_session_plan_mode",
                    json!({ "request": { "conversationId": conversation_id, "enabled": enabled } }),
                ) {
                    self.absorb(&result);
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
                if let Some(result) = self.call(
                    "service_select_workbench_reasoning_effort",
                    json!({ "conversationId": id, "effort": value }),
                ) {
                    self.absorb(&result);
                }
            }
            Command::LoadWorkspaceDiffSnapshot => self.git_refresh(),
            Command::RunWorkspaceGitFileAction { paths, stage } => self.git_stage(paths, stage),
            Command::LoadWorkspaceDiffFileDetail { path } => self.git_diff(path),
            Command::LoadWorkspaceGitBranches => self.git_branches(),
            Command::SwitchWorkspaceGitBranch { name, create } => self.git_switch(name, create),
            Command::GenerateGitCommitMessage => self.git_generate_message(),
            Command::RunGitCommitAction {
                message,
                new_branch,
                include_unstaged,
                push,
            } => self.git_commit(message, new_branch, include_unstaged, push),
            // The attachment picker reads the local disk: the service runs on this machine and
            // attachments are staged from local bytes.
            // 附件选择器读取本地磁盘：服务运行在本机，附件以本地字节暂存。
            Command::ListLocalFiles { path } => self.emit(BackendEvent::Directory(
                super::files::list(path.as_deref(), true, true),
            )),
            Command::BrowseProjectDirectories { path } => {
                if let Some(result) = self.call(
                    "service_browse_project_directories",
                    json!({ "path": path }),
                ) {
                    self.emit(BackendEvent::Directory(map::directory(&result)));
                }
            }
            Command::AddProjectFromPath { path } => {
                if let Some(result) =
                    self.call("service_add_project_from_path", json!({ "path": path }))
                {
                    self.absorb(&result);
                    // Name.
                    // 当前名称。
                    let name = arr(&self.wb, "projects")
                        .iter()
                        .find(|p| same_path(&str_at(p, "path"), &path))
                        .map(|p| str_at(p, "name"))
                        .unwrap_or(path);
                    self.emit(BackendEvent::Notice(Notice::ProjectAdded(name)));
                }
            }
            Command::RenameProject { project_id, name } => {
                if let Some(result) = self.call(
                    "service_rename_project",
                    json!({ "projectId": project_id, "name": name }),
                ) {
                    self.absorb(&result);
                    self.emit(BackendEvent::Notice(Notice::Renamed));
                }
            }
            Command::ToggleProjectPinned { project_id } => {
                if let Some(result) = self.call(
                    "service_toggle_project_pinned",
                    json!({ "projectId": project_id }),
                ) {
                    self.absorb(&result);
                }
            }
            Command::OpenProjectDirectory { project_id } => {
                self.call(
                    "service_open_project_directory",
                    json!({ "projectId": project_id }),
                );
            }
            Command::ArchiveProjectConversations { project_id } => {
                // Count.
                // 当前计数。
                let count = arr(&self.wb, "conversations")
                    .iter()
                    .filter(|c| str_at(c, "projectId") == project_id)
                    .count();
                if let Some(result) = self.call(
                    "service_archive_project_conversations",
                    json!({ "projectId": project_id }),
                ) {
                    self.absorb(&result);
                    self.emit(BackendEvent::Notice(Notice::ArchivedCount(count)));
                }
            }
            Command::RemoveProject { project_id } => {
                // Name.
                // 当前名称。
                let name = arr(&self.wb, "projects")
                    .iter()
                    .find(|p| str_at(p, "id") == project_id)
                    .map(|p| str_at(p, "name"))
                    .unwrap_or_default();
                if let Some(result) =
                    self.call("service_remove_project", json!({ "projectId": project_id }))
                {
                    self.absorb(&result);
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
                if let Some(result) = self.call(
                    "service_rename_conversation",
                    json!({ "conversationId": conversation_id, "name": title }),
                ) {
                    self.absorb(&result);
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
                if let Some(result) = self.call(
                    "service_archive_conversation",
                    json!({ "conversationId": conversation_id }),
                ) {
                    self.absorb(&result);
                    self.emit(BackendEvent::Notice(Notice::Archived(title)));
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
                Some(GoalStatus::Active),
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
                Some(GoalStatus::Active),
            ),
            Command::ClearSessionGoal { conversation_id } => self.goal(
                "service_clear_session_goal",
                json!({ "conversationId": conversation_id }),
                None,
            ),
            Command::UpdateSessionGoalStatus {
                conversation_id,
                status,
            } => {
                // Value.
                // 当前协议值。
                let value = match status {
                    GoalStatus::Paused => "paused",
                    _ => "active",
                };
                self.goal(
                    "service_update_session_goal_status",
                    json!({ "conversationId": conversation_id, "status": value }),
                    Some(status),
                )
            }
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
            if let Some(key) = draft.model_key.clone() {
                self.model_keys.insert(id, key);
            }
            self.emit_effort();
            return;
        }
        if let Some(result) = self.call(
            "service_select_conversation",
            json!({ "conversationId": id }),
        ) {
            self.absorb(&result);
            // `markConversationRead` is ancillary, as in the WebUI: a failure never surfaces.
            // 与 WebUI 一致，已读标记属于附属操作：失败不会提示。
            if let Some(client) = self.client.clone()
                && let Ok(result) = client.call(
                    "service_mark_conversation_read",
                    json!({ "conversationId": id }),
                )
            {
                self.absorb(&result);
            }
        }
        self.emit_conversation_state(&id);
        self.emit_question(&id);
        self.hydrate(&id);
        self.git_refresh();
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
        if let Some(result) = self.call(
            "service_select_public_ai_model",
            json!({ "request": request }),
        ) {
            self.absorb(&result);
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
        let staged: Vec<Value> = attachments
            .iter()
            .filter_map(|path| self.stage_attachment(path, project_id.as_deref(), &conversation_id))
            .collect();
        if staged.len() != attachments.len() {
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
        // Exact RPC parameters.
        // 准确 RPC 参数。
        let params = json!({
            "clientSubmissionId": fresh_id(),
            "projectId": project_id,
            "conversationId": id_param,
            "draftSlotKey": format!("new:{}", project_id.as_deref().unwrap_or("workspace")),
            "prompt": prompt,
            "attachments": staged,
            "executionMode": execution_mode,
            "reasoningEffort": effort_param,
            "modelSelection": model_selection,
            "sessionSettings": Value::Null,
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
                    Err(error) => return worker.failed(error),
                };
                // Real id.
                // 服务返回的持久会话身份。
                let Some(real_id) = opt_str(&result, "conversationId") else {
                    return;
                };
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
    ) -> Option<Value> {
        // Bytes.
        // 完整字节缓冲。
        let bytes = match std::fs::read(path) {
            Ok(bytes) => bytes,
            Err(error) => {
                self.toast(Severity::Error, format!("{path}: {error}"));
                return None;
            }
        };
        // File name.
        // 准确附件文件名。
        let file_name = std::path::Path::new(path)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.to_string());
        self.call(
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
    fn goal(&mut self, method: &str, request: Value, status: Option<GoalStatus>) {
        if let Some(result) = self.call(method, json!({ "request": request })) {
            self.absorb(&result);
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
            .map(|n| arr(n, "tasks"))
            .unwrap_or_default()
            .iter()
            .find(|t| str_at(t, "taskId") == task_id)
            .map(|t| str_at(t, "label"))
            .unwrap_or_default();
        // Method.
        // 准确 RPC 方法名。
        let method = if stop {
            "service_stop_subagent_task"
        } else {
            "service_close_subagent_task"
        };
        if let Some(result) = self.call(
            method,
            json!({ "parentConversationId": parent, "taskId": task_id }),
        ) {
            self.absorb(&result);
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
        let item = self
            .entry("contextInjectionByConversation", conversation_id)
            .map(|e| arr(e, "contextInjection.dynamicCards"))
            .unwrap_or_default()
            .iter()
            .flat_map(|card| arr(card, "items"))
            .find(|item| {
                str_at(item, "memoryId") == memory_id || str_at(item, "dedupeKey") == memory_id
            })
            .cloned()
            .unwrap_or_default();
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
                    "dedupeKey": at(&item, "dedupeKey"),
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
    /// Returns `Option<Value>`.
    /// 返回 `Option<Value>`。
    pub(super) fn selected_project(&self) -> Option<Value> {
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
        let projects = arr(&self.wb, "projects");
        project_id
            .and_then(|pid| projects.iter().find(|p| str_at(p, "id") == pid))
            .or_else(|| projects.first())
            .cloned()
    }

    /// Returns the selected durable conversation, excluding local draft identities.
    /// 返回所选持久会话，排除本地草稿身份。
    /// Returns `Option<String>`.
    /// 返回 `Option<String>`。
    pub(super) fn selected_real(&self) -> Option<String> {
        self.selected.clone().filter(|id| !is_draft(id))
    }
}

/// Recognizes a workbench response by its declared project and conversation arrays.
/// 按已声明的项目及会话数组识别工作台响应。
/// `value` is the protocol value being read or projected.
/// `value` 为正在读取或投影的协议值。
/// Returns `bool`.
/// 返回 `bool`。
fn is_snapshot(value: &Value) -> bool {
    value.get("conversations").is_some_and(Value::is_array) && value.get("projects").is_some()
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
        title: crate::i18n::text("sidebar.newConversation"),
        project_id: draft.project_id.clone(),
        run_phase: RunPhase::Idle,
        last_updated_label: String::new(),
        unread: false,
        attention: false,
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

#[cfg(test)]
mod tests {
    use super::super::test_support::RpcServer;
    use super::*;
    use crate::service_api::{knowledge_base, settings_actions, vce};
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
                json!({"libraryId":"library","documentId":"doc","corpus":"display"})
            );
            json!({"ok":false,"error":{"message":"Display document is not built"}})
        });
        // Isolated worker and channels never discover an external endpoint.
        // 隔离工作线程及通道不发现外部端点。
        let (mut worker, events, input) = worker_fixture(server.client.clone());
        worker.service_call(knowledge_base::read_managed_knowledge_document(
            "library", "doc", "display",
        ));
        worker.handle_input(input.recv_timeout(Duration::from_secs(2)).unwrap());
        assert!(events.try_iter().any(|event|matches!(event,BackendEvent::SlotFailed{slot:Slot::KnowledgeDocument,call,message}
            if call.param("/libraryId") == "library" && call.param("/documentId") == "doc" && call.param("/corpus") == "display"
               && message == "Display document is not built")));
        server.thread.join().unwrap();
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
            "query":request.body.pointer("/params/request/query").unwrap()}})
        });
        // Worker; Events; Current input.
        // 串行服务工作线程状态；事件集合或事件接收通道；当前输入。
        let (mut worker, events, input) = worker_fixture(server.client.clone());
        worker.service_call(knowledge_base::search_managed_knowledge_library(
            "demo-library",
            "old",
            None,
        ));
        worker.service_call(knowledge_base::search_managed_knowledge_library(
            "demo-library",
            "new",
            None,
        ));
        for _ in 0..2 {
            worker.handle_input(input.recv_timeout(Duration::from_secs(2)).unwrap());
        }
        // Received.
        // 接收到的消息。
        let received: Vec<_> = events.try_iter().collect();
        assert_eq!(received.len(), 1);
        assert!(
            matches!(&received[0],BackendEvent::Slot{slot:Slot::KnowledgeSearch,value,..} if value["query"] == "new")
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
            apply: Box::new(|worker| worker.wb = json!({"accepted":true})),
        });
        assert_eq!(worker.wb["accepted"], true);
        assert!(events.try_recv().is_err());
        assert!(!worker.handle_input(Input::Shutdown));
        server.thread.join().unwrap();
    }

    /// Publishes matching VCE jobs once and ignores older socket and read sequences.
    /// 单次发布匹配的 VCE 任务，并忽略较旧套接字与读取序号。
    #[test]
    fn vce_job_events_are_scoped_and_monotonic() {
        // Server.
        // 当前服务器或本地合成监听器。
        let server = RpcServer::start(0, |_| Value::Null);
        // Worker; Events.
        // 串行服务工作线程状态；事件集合或事件接收通道。
        let (mut worker, events, _) = worker_fixture(server.client.clone());
        // Loader.
        // 准确槽位加载器。
        let loader = vce::load_vce_job("demo-project", None);
        worker.loaders.insert(Slot::VceJob, loader.clone());
        // Job.
        // 持久任务回执。
        let mut job =
            json!({"projectId":"demo-project","jobId":"demo-job","state":"running","sequence":4});
        worker.service_event("vceJobChanged", job.clone());
        job["sequence"] = json!(3);
        worker.service_event("vceJobChanged", job.clone());
        worker.call_done(loader, job.clone());
        job["projectId"] = json!("other-project");
        job["sequence"] = json!(5);
        worker.service_event("vceJobChanged", job);
        // Received.
        // 接收到的消息。
        let received: Vec<_> = events.try_iter().collect();
        assert_eq!(received.len(), 1);
        assert!(
            matches!(&received[0],BackendEvent::Slot{slot:Slot::VceJob,value,..} if value["sequence"] == 4)
        );
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
}
