//! Resumable task-session runtime for host-managed task_id chaining and nested subtask execution.
//! 供宿主管理 task_id 续跑与嵌套子任务执行使用的可恢复任务会话运行时模块。

use crate::agent_runtime::events::{ExecutionStreamEvent, RuntimeEvent, RuntimeEventKind};
use crate::agent_runtime::prompt_composer_runtime::runtime_prompt_composer_engine;
use crate::agent_runtime::provider_request_bridge_conversation::core_snapshot_to_session_conversation_state;
use crate::agent_runtime::provider_stream::ProviderStreamDelta;
use crate::agent_runtime::provider_transport_live_delta::ProviderLiveDeltaSender;
use crate::agent_runtime::provider_transport_stream_executor::StreamingRuntimeToolExecutor;
use crate::agent_runtime::runtime_stream_delta::{
    RuntimeStreamDeltaState, process_provider_stream_delta,
};
use crate::agent_runtime::tool_runtime_bridge::VulcanManagedToolRuntimeBridge;
use crate::conversation_protocol::SessionConversationState;
use crate::model::SessionId;
use async_stream::stream;
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, RwLock};
use tokio::sync::mpsc::{UnboundedSender, unbounded_channel};
use tokio::task::AbortHandle;
use uuid::Uuid;
use vulcan_core::controller::VulcanCoreController;
use vulcan_core::tool::AllowAllToolPolicy;
use vulcan_core::types::{
    ControllerConfig as CoreControllerConfig, ConversationSnapshot as CoreConversationSnapshot,
};
use vulcan_prompt_composer::{
    ModelDescriptor, PersonalitySelection, PromptAssemblyInput, PromptAssemblyMode,
    PromptAssemblyOutput, ProviderPromptSnapshot, SubagentSnapshot, ToolCatalogSnapshot,
};
use vulcan_tools_manager::{
    ResolvedToolDefinition, VulcanPlanToolPolicy, VulcanToolCandidate, VulcanToolDescriptor,
    VulcanToolHostResult, VulcanToolInvocationResult, VulcanToolSourceKind, VulcanToolsManager,
};

/// Default nested-task depth limit used to prevent unbounded recursive task spawning.
/// 用于防止无限递归任务派生的默认嵌套任务深度上限。
const DEFAULT_TASK_NESTING_LIMIT: usize = 8;

/// Fallback stop summary returned to the parent task tool when the child run is manually interrupted before producing a complete answer.
/// 当子运行在产出完整答案前被人工中断时，返回给父级 task 工具的兜底停止摘要。
const STOPPED_TASK_RESULT_SUMMARY: &str = "The subagent was stopped at the user's request before producing a complete final result. Decide whether to retry, rewrite the task, or continue the main flow with the current context.";

/// Synthetic running summary returned to the parent task tool while the detached child run continues asynchronously in the background.
/// 当已分离的子运行仍在后台异步继续执行时，返回给父级 task 工具的合成运行中摘要。
const RUNNING_TASK_RESULT_SUMMARY: &str = "The subagent has started running in the background. The main flow may continue, and the result will be filled back automatically when the task finishes.";

/// Windows filesystem path guidance injected into child task prompts to stabilize tool calls.
/// 注入到子任务提示词中的 Windows 文件系统路径指引，用于稳定工具调用。
const WINDOWS_TASK_FILESYSTEM_PATH_GUIDANCE: &str = "[Filesystem Paths]\nWhen calling filesystem-oriented tools on Windows, prefer standard absolute Windows paths like C:\\repo\\path. Do not wrap tool arguments in Markdown code fences or extra quotes. When a JSON string needs escaping, keep the semantic path as a normal Windows path and escape backslashes only as required by the transport format.";

/// Boxed asynchronous task execution future.
/// 装箱后的异步任务执行 Future。
pub type TaskExecutionFuture =
    Pin<Box<dyn Future<Output = Result<TaskExecutionOutcome, String>> + Send>>;

/// Shared asynchronous task execution handler.
/// 共享的异步任务执行处理器。
pub type TaskExecutionHandler =
    Arc<dyn Fn(TaskExecutionRequest) -> TaskExecutionFuture + Send + Sync>;

/// Structured task tool arguments accepted from the model-visible `task` tool surface.
/// 模型可见 `task` 工具表面接受的结构化任务参数。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
struct TaskToolInvocationArgs {
    /// Required prompt used to start or resume the task session.
    /// 用于启动或续跑任务会话的必填提示词。
    prompt: String,
    /// Optional human-readable task description.
    /// 可选的人类可读任务描述。
    description: Option<String>,
    /// Optional stable task identifier supplied by the caller.
    /// 调用方提供的可选稳定任务标识。
    task_id: Option<String>,
    /// Optional subagent label used for display and system-prompt augmentation.
    /// 用于展示和系统提示词增强的可选子代理标签。
    subagent: Option<String>,
}

/// Canonical task-session state stored by the host runtime between task invocations.
/// 宿主运行时在多次任务调用之间持久保存的规范任务会话状态。
#[derive(Debug, Clone)]
struct TaskSessionRecord {
    /// Stable task identifier that owns this record.
    /// 持有该记录的稳定任务标识。
    task_id: SessionId,
    /// Caller-facing task identifier used to look up or resume this task from future tool invocations.
    /// 用于从后续工具调用中查找或续跑该任务的调用方面向任务标识。
    public_task_id: String,
    /// The top-level GUI or task session that originated this task creation.
    /// 发起该任务创建的顶层 GUI 或任务会话标识。
    origin_session_id: SessionId,
    /// Optional parent task identifier for the nested task chain.
    /// 用于嵌套任务链的可选父任务标识。
    parent_task_id: Option<SessionId>,
    /// Stable nesting depth used for recursion guardrails.
    /// 用于递归防护的稳定嵌套深度。
    depth: usize,
    /// Optional human-readable task description.
    /// 可选的人类可读任务描述。
    description: Option<String>,
    /// Optional subagent label selected for the task.
    /// 任务选用的可选子代理标签。
    subagent: Option<String>,
    /// Stable human-readable title persisted for cards, badges, and child-panel headers.
    /// 为卡片、徽标与子面板标题持久化保存的稳定人类可读标题。
    display_title: String,
    /// Fully resolved controller configuration reused when this task is resumed.
    /// 任务续跑时复用的完整控制器配置。
    controller_config: Option<CoreControllerConfig>,
    /// Canonical conversation snapshot retained for later resumption.
    /// 为后续续跑保留的规范会话快照。
    snapshot: CoreConversationSnapshot,
    /// Latest prompt text that was executed for this task.
    /// 该任务最近一次执行的提示词文本。
    last_prompt: String,
    /// Latest result text emitted by the task backend.
    /// 任务后端最近一次产出的结果文本。
    last_result_text: String,
    /// Unix-millis timestamp captured when the record was created.
    /// 记录创建时捕获的 Unix 毫秒时间戳文本。
    created_at: String,
    /// Unix-millis timestamp captured when the record was last updated.
    /// 记录最近一次更新时捕获的 Unix 毫秒时间戳文本。
    updated_at: String,
    /// Last observed execution status for the task.
    /// 任务最近一次观测到的执行状态。
    last_status: String,
}

/// Task execution request handed to the backend runner for one concrete task session.
/// 交给后端执行器的单条具体任务会话执行请求。
#[derive(Debug, Clone)]
pub struct TaskExecutionRequest {
    /// Invoking controller session identifier that owns the current tool call.
    /// 拥有当前工具调用的执行中控制器会话标识。
    pub current_session_id: SessionId,
    /// Stable task identifier for the child or resumed task session.
    /// 子任务或续跑任务会话的稳定任务标识。
    pub task_id: SessionId,
    /// Caller-facing task identifier echoed back to the parent model and host UI.
    /// 回传给父模型与宿主界面的调用方面向任务标识。
    pub public_task_id: String,
    /// User-visible prompt used to drive the task session.
    /// 用于驱动任务会话的用户可见提示词。
    pub prompt: String,
    /// Optional human-readable task description.
    /// 可选的人类可读任务描述。
    pub description: Option<String>,
    /// Optional subagent label used to annotate the task session.
    /// 用于标注任务会话的可选子代理标签。
    pub subagent: Option<String>,
    /// Stable human-readable title reused across parent cards and child-panel surfaces.
    /// 在父卡片与子面板表面之间复用的稳定人类可读标题。
    pub display_title: String,
    /// Optional parent task identifier for nested task chains.
    /// 嵌套任务链的可选父任务标识。
    pub parent_task_id: Option<SessionId>,
    /// Stable nesting depth used by the recursion guard.
    /// 递归防护使用的稳定嵌套深度。
    pub depth: usize,
    /// Canonical snapshot used when resuming an existing task session.
    /// 续跑既有任务会话时使用的规范快照。
    pub snapshot: CoreConversationSnapshot,
    /// Controller configuration used when resuming an existing task session.
    /// 续跑既有任务会话时使用的控制器配置。
    pub controller_config: Option<CoreControllerConfig>,
    /// Whether this request resumes a previously stored task session.
    /// 该请求是否在续跑一个已存储的任务会话。
    pub is_resumed: bool,
}

/// Task execution outcome returned by the backend after the child controller finishes.
/// 子控制器完成后由后端返回的任务执行结果。
#[derive(Debug, Clone)]
pub struct TaskExecutionOutcome {
    /// Stable task identifier associated with the outcome.
    /// 与该结果关联的稳定任务标识。
    pub task_id: SessionId,
    /// Final controller configuration used for this task session.
    /// 该任务会话实际使用的最终控制器配置。
    pub controller_config: CoreControllerConfig,
    /// Updated canonical snapshot captured after the task finished.
    /// 任务结束后捕获到的更新后规范快照。
    pub snapshot: CoreConversationSnapshot,
    /// Total provider turns executed by the child controller.
    /// 子控制器实际执行的 Provider 回合总数。
    pub provider_turns_executed: usize,
    /// Total tool calls processed by the child controller.
    /// 子控制器处理过的工具调用总数。
    pub tool_calls_processed: usize,
    /// Final assistant message identifier produced by the child controller.
    /// 子控制器产出的最终助手消息标识。
    pub final_assistant_message_id: Option<String>,
    /// Final text surfaced to the model-visible task result.
    /// 暴露给模型可见任务结果的最终文本。
    pub final_output_text: String,
    /// Final execution status surfaced to both host persistence and the parent task-tool result.
    /// 同时暴露给宿主持久化与父级 task 工具结果的最终执行状态。
    pub execution_status: TaskExecutionStatus,
}

/// Stable terminal status produced by one task execution attempt.
/// 一次任务执行尝试产出的稳定终态状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskExecutionStatus {
    /// Child controller has been detached into the background and is still running.
    /// 子控制器已被分离到后台，当前仍在执行中。
    Running,
    /// Child controller completed normally and produced a reusable answer.
    /// 子控制器正常完成并产出了可复用答案。
    Completed,
    /// Child controller was intentionally interrupted by the user or transport abort.
    /// 子控制器被用户或传输中止有意打断。
    Stopped,
}

/// Fully resolved task-session identifiers derived from one task tool invocation.
/// 从一次 task 工具调用中完整解析出的任务会话标识集合。
#[derive(Debug, Clone)]
struct ResolvedTaskSessionIdentifier {
    /// Internal UUID-shaped child session identifier used by the runtime registry.
    /// 被运行时注册表使用的内部 UUID 形态子会话标识。
    task_id: SessionId,
    /// Caller-facing task identifier preserved for model-visible results and follow-up prompts.
    /// 为模型可见结果与后续提示词保留的调用方面向任务标识。
    public_task_id: String,
    /// Whether the resolved internal session already exists and should resume in place.
    /// 解析出的内部会话是否已经存在并应原地续跑。
    is_resumed: bool,
}

/// Detached runtime event mirrored out of one background child task after the parent controller has already resumed or finished.
/// 父控制器已经恢复或结束之后，由某条后台子任务镜像输出的分离运行时事件。
#[derive(Debug, Clone)]
pub enum DetachedTaskRuntimeEvent {
    /// Child-session execution event that should be projected into the dedicated subagent transcript surface.
    /// 应投影到独立子代理转录界面中的子会话执行事件。
    ChildExecution(ExecutionStreamEvent),
    /// Parent-session runtime event that should update task cards and coordinator follow-up flow without keeping the original parent stream blocked.
    /// 在不阻塞原始父流的前提下，用于更新 task 卡片与协调代理后续流程的父会话运行时事件。
    ParentRuntime {
        /// Root parent session that should absorb this detached delegate lifecycle event.
        /// 应吸收该分离委派生命周期事件的根父会话。
        parent_session_id: SessionId,
        /// Concrete runtime event mirrored back from the detached child-task lifecycle.
        /// 从分离子任务生命周期镜像回来的具体运行时事件。
        event: RuntimeEvent,
    },
}

impl TaskExecutionStatus {
    /// Returns the persisted lowercase text expected by task-session metadata and tool cards.
    /// 返回 task 会话元数据与工具卡片所需的持久化小写文本。
    fn as_str(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Stopped => "stopped",
        }
    }
}

/// Buffered detached task-event stream state kept per child task so late subscribers can still replay early background output safely.
/// 按子任务维度保留的分离事件流状态，用于让稍后接入的订阅者也能安全回放早期后台输出。
#[derive(Default)]
struct DetachedTaskEventStreamState {
    /// Buffered detached events accumulated before any subscriber has attached successfully.
    /// 在任意订阅者成功接入之前累计保存的分离事件缓冲区。
    buffered_events: Vec<DetachedTaskRuntimeEvent>,
    /// Active subscribers currently receiving detached child-task events in real time.
    /// 当前实时接收分离子任务事件的活动订阅者集合。
    subscribers: Vec<UnboundedSender<DetachedTaskRuntimeEvent>>,
    /// Whether the detached stream has already reached its terminal state and will not publish any further events.
    /// 该分离事件流是否已经到达终态、后续不会再发布任何事件。
    closed: bool,
}

/// Guard that unregisters one backend when it goes out of scope.
/// 作用域结束时自动注销某个后端的守卫对象。
pub struct TaskBackendGuard {
    /// Shared runtime registry that owns the backend map.
    /// 持有后端映射表的共享运行时注册表。
    runtime: Arc<TaskRuntime>,
    /// Session identifier whose backend should be removed on drop.
    /// 在 Drop 时应移除其后端的会话标识。
    session_id: SessionId,
}

impl Drop for TaskBackendGuard {
    /// Removes the installed backend as soon as the guard leaves scope.
    /// 在守卫离开作用域时立即移除已安装的后端。
    fn drop(&mut self) {
        self.runtime.remove_backend(self.session_id);
    }
}

/// Guard that unregisters one active child task abort handle when the execution scope goes away.
/// 在执行作用域结束时自动注销一条活动子任务中止句柄的守卫对象。
pub struct ActiveTaskRunGuard {
    /// Shared runtime registry that owns the active child-task run map.
    /// 持有活动子任务运行映射表的共享运行时注册表。
    runtime: Arc<TaskRuntime>,
    /// Child task identifier whose running entry should be removed on drop.
    /// 在 Drop 时应移除其运行条目的子任务标识。
    task_id: SessionId,
}

impl Drop for ActiveTaskRunGuard {
    /// Removes the installed active child-task run entry as soon as the guard leaves scope.
    /// 在守卫离开作用域时立即移除已安装的活动子任务运行条目。
    fn drop(&mut self) {
        self.runtime.remove_active_task_run(self.task_id);
    }
}

/// Drop guard that aborts one spawned child controller task if the outer task-backend future is cancelled early.
/// 若外层任务后端 Future 提前取消，则在 Drop 时中止对应已派生子控制器任务的守卫对象。
struct AbortChildRunOnDrop {
    /// Abort handle used to stop the spawned child controller task.
    /// 用于停止已派生子控制器任务的中止句柄。
    handle: AbortHandle,
}

impl Drop for AbortChildRunOnDrop {
    /// Aborts the spawned child controller task when the outer owner disappears unexpectedly.
    /// 当外层拥有者异常消失时中止已派生子控制器任务。
    fn drop(&mut self) {
        self.handle.abort();
    }
}

/// Host-managed registry for resumable task sessions and per-session task execution backends.
/// 宿主管理的可恢复任务会话与按会话划分任务执行后端的注册表。
pub struct TaskRuntime {
    /// Stored task-session records keyed by stable task identifier.
    /// 按稳定任务标识索引存储的任务会话记录。
    sessions: RwLock<HashMap<SessionId, TaskSessionRecord>>,
    /// Backend handlers keyed by the invoking controller session identifier.
    /// 按执行中控制器会话标识索引存储的后端处理器。
    backends: RwLock<HashMap<SessionId, TaskExecutionHandler>>,
    /// Active child-task abort handles keyed by stable task identifier.
    /// 按稳定任务标识索引存储的活动子任务中止句柄。
    active_task_runs: RwLock<HashMap<SessionId, AbortHandle>>,
    /// Detached child-task event streams keyed by stable task identifier for non-blocking GUI mirroring.
    /// 为无阻塞图形界面镜像保留的按稳定任务标识索引的分离子任务事件流。
    detached_task_event_streams: RwLock<HashMap<SessionId, DetachedTaskEventStreamState>>,
    /// Maximum supported nested task depth.
    /// 支持的最大嵌套任务深度。
    max_depth: usize,
}

impl Default for TaskRuntime {
    /// Builds one empty task runtime with the default recursion guardrail.
    /// 构建一份带默认递归防护的空任务运行时。
    fn default() -> Self {
        Self::new()
    }
}

impl TaskRuntime {
    /// Builds one empty task runtime with the default recursion guardrail.
    /// 构建一份带默认递归防护的空任务运行时。
    pub fn new() -> Self {
        Self {
            sessions: RwLock::new(HashMap::new()),
            backends: RwLock::new(HashMap::new()),
            active_task_runs: RwLock::new(HashMap::new()),
            detached_task_event_streams: RwLock::new(HashMap::new()),
            max_depth: DEFAULT_TASK_NESTING_LIMIT,
        }
    }

    /// Returns the maximum supported nested task depth.
    /// 返回支持的最大嵌套任务深度。
    pub fn max_depth(&self) -> usize {
        self.max_depth
    }

    /// Installs one backend handler for the given controller session and returns a guard that unregisters it on drop.
    /// 为给定控制器会话安装一个后端处理器，并返回一个在 Drop 时自动注销它的守卫。
    pub fn install_backend(
        self: &Arc<Self>,
        session_id: SessionId,
        handler: TaskExecutionHandler,
    ) -> TaskBackendGuard {
        self.backends
            .write()
            .expect("task backend registry lock")
            .insert(session_id, handler);
        TaskBackendGuard {
            runtime: Arc::clone(self),
            session_id,
        }
    }

    /// Executes one task tool invocation through the backend registered for the current controller session.
    /// 通过为当前控制器会话注册的后端执行一次任务工具调用。
    pub async fn invoke_task(
        self: &Arc<Self>,
        current_session_id: SessionId,
        args: Value,
    ) -> Result<VulcanToolInvocationResult, String> {
        let backend = self
            .backend_for_session(current_session_id)
            .ok_or_else(|| {
                format!(
                    "The current session `{}` does not have a task backend installed, so the subtask cannot run.",
                    current_session_id
                )
            })?;

        let parsed_args = parse_task_tool_arguments(args)?;
        let resolved_identifier = resolve_task_session_id(self, current_session_id, &parsed_args)?;
        if self
            .active_task_runs
            .read()
            .map_err(|_| {
                String::from("The active task registry is locked and cannot inspect running state.")
            })?
            .contains_key(&resolved_identifier.task_id)
        {
            return Err(format!(
                "Task `{}` is still running and cannot be started again right now.",
                resolved_identifier.public_task_id
            ));
        }
        let invocation = prepare_task_execution_request(
            self,
            current_session_id,
            resolved_identifier,
            parsed_args,
        )?;
        let outcome = match backend(invocation.clone()).await {
            Ok(outcome) => outcome,
            Err(error) => {
                self.mark_task_failure(current_session_id, &invocation, &error)?;
                return Err(error);
            }
        };

        self.store_task_outcome(current_session_id, &invocation, &outcome)?;

        Ok(build_task_invocation_result(&invocation, &outcome))
    }

    /// Stops one in-flight child task by its stable task identifier when the runtime still owns a live abort handle.
    /// 当运行时仍持有实时中止句柄时，按稳定任务标识停止一条进行中的子任务。
    pub fn stop_task(&self, task_id: SessionId) -> Result<bool, String> {
        let handle = self
            .active_task_runs
            .read()
            .map_err(|_| {
                String::from(
                    "The active task registry is locked and cannot stop the subagent task.",
                )
            })?
            .get(&task_id)
            .cloned();
        if let Some(handle) = handle {
            handle.abort();
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Returns whether one child task still owns an in-flight abort handle, which the GUI can treat as the source of truth for active-running state.
    /// 返回某个子任务当前是否仍持有活动中的中止句柄，供图形界面将其视作真实运行态事实源。
    pub fn is_task_running(&self, task_id: SessionId) -> Result<bool, String> {
        Ok(self
            .active_task_runs
            .read()
            .map_err(|_| {
                String::from(
                    "The active task registry is locked and cannot inspect the subagent run state.",
                )
            })?
            .contains_key(&task_id))
    }

    /// Subscribes to one detached child-task event stream, replaying any buffered early events before switching to live delivery.
    /// 订阅某条分离子任务事件流，并在切换到实时投递之前先回放已经缓冲的早期事件。
    pub fn subscribe_detached_task_events(
        &self,
        task_id: SessionId,
    ) -> impl futures_util::Stream<Item = DetachedTaskRuntimeEvent> + Send + 'static {
        let (sender, mut receiver) = unbounded_channel::<DetachedTaskRuntimeEvent>();
        let (buffered_events, closed) = {
            let mut streams = self
                .detached_task_event_streams
                .write()
                .expect("task detached stream registry lock");
            let state = streams.entry(task_id).or_default();
            let buffered_events = std::mem::take(&mut state.buffered_events);
            let closed = state.closed;
            if !closed {
                state.subscribers.push(sender.clone());
            } else {
                streams.remove(&task_id);
            }
            (buffered_events, closed)
        };

        for event in buffered_events {
            let _ = sender.send(event);
        }
        if closed {
            drop(sender);
        }

        stream! {
            while let Some(event) = receiver.recv().await {
                yield event;
            }
        }
    }

    /// Returns one backend handler for the given controller session identifier when it exists.
    /// 当给定控制器会话标识存在时，返回对应的后端处理器。
    fn backend_for_session(&self, session_id: SessionId) -> Option<TaskExecutionHandler> {
        self.backends
            .read()
            .expect("task backend registry lock")
            .get(&session_id)
            .cloned()
    }

    /// Removes one backend handler from the registry.
    /// 从注册表中移除一个后端处理器。
    fn remove_backend(&self, session_id: SessionId) {
        self.backends
            .write()
            .expect("task backend registry lock")
            .remove(&session_id);
    }

    /// Installs one active child-task abort handle and returns a guard that unregisters it on drop.
    /// 安装一条活动子任务中止句柄，并返回一个在 Drop 时自动注销它的守卫。
    fn install_active_task_run(
        self: &Arc<Self>,
        task_id: SessionId,
        handle: AbortHandle,
    ) -> ActiveTaskRunGuard {
        self.active_task_runs
            .write()
            .expect("task active run registry lock")
            .insert(task_id, handle);
        ActiveTaskRunGuard {
            runtime: Arc::clone(self),
            task_id,
        }
    }

    /// Removes one active child-task abort handle from the registry.
    /// 从注册表中移除一条活动子任务中止句柄。
    fn remove_active_task_run(&self, task_id: SessionId) {
        self.active_task_runs
            .write()
            .expect("task active run registry lock")
            .remove(&task_id);
    }

    /// Publishes one detached child-task event, buffering it when no subscriber has attached yet and replaying it later on first subscription.
    /// 发布一条分离子任务事件；若当前尚无订阅者，则先缓冲，待首个订阅建立后再回放。
    fn publish_detached_task_event(&self, task_id: SessionId, event: DetachedTaskRuntimeEvent) {
        let mut streams = self
            .detached_task_event_streams
            .write()
            .expect("task detached stream registry lock");
        let state = streams.entry(task_id).or_default();
        if state.closed {
            return;
        }
        if state.subscribers.is_empty() {
            state.buffered_events.push(event);
            return;
        }

        let mut delivered = false;
        state.subscribers.retain(|sender| {
            let send_result = sender.send(event.clone());
            if send_result.is_ok() {
                delivered = true;
            }
            send_result.is_ok()
        });
        if !delivered {
            state.buffered_events.push(event);
        }
    }

    /// Marks one detached child-task event stream as terminal and drops active senders so GUI listener streams can close naturally.
    /// 将某条分离子任务事件流标记为终态，并释放活动发送端，让图形界面监听流自然关闭。
    fn close_detached_task_event_stream(&self, task_id: SessionId) {
        let mut streams = self
            .detached_task_event_streams
            .write()
            .expect("task detached stream registry lock");
        let should_remove = if let Some(state) = streams.get_mut(&task_id) {
            state.closed = true;
            !state.subscribers.is_empty() || state.buffered_events.is_empty()
        } else {
            false
        };
        if should_remove {
            streams.remove(&task_id);
        }
    }

    /// Stores one newly completed task outcome back into the session registry.
    /// 将一条新完成的任务结果回写到会话注册表中。
    fn store_task_outcome(
        &self,
        current_session_id: SessionId,
        invocation: &TaskExecutionRequest,
        outcome: &TaskExecutionOutcome,
    ) -> Result<(), String> {
        let mut sessions = self.sessions.write().map_err(|_| {
            String::from("The task session registry is locked and cannot store the result.")
        })?;
        let now = current_unix_millis_text();
        let status = outcome.execution_status.as_str().to_string();
        let record = sessions
            .entry(invocation.task_id.clone())
            .or_insert_with(|| {
                build_task_session_record(
                    invocation.task_id,
                    invocation.public_task_id.clone(),
                    current_session_id,
                    invocation.parent_task_id,
                    invocation.depth,
                    invocation.description.clone(),
                    invocation.subagent.clone(),
                    invocation.display_title.clone(),
                    invocation
                        .controller_config
                        .clone()
                        .or_else(|| Some(outcome.controller_config.clone())),
                    invocation.snapshot.clone(),
                    invocation.prompt.clone(),
                    String::from("created"),
                )
            });
        debug_assert_eq!(record.task_id, invocation.task_id);
        let _ = record.origin_session_id;
        let _ = &record.created_at;
        record.controller_config = Some(outcome.controller_config.clone());
        record.snapshot = outcome.snapshot.clone();
        record.last_prompt = invocation.prompt.clone();
        record.last_result_text = outcome.final_output_text.clone();
        record.updated_at = now;
        record.last_status = status;
        record.display_title = invocation.display_title.clone();
        if record.description.is_none() {
            record.description = invocation.description.clone();
        }
        if record.subagent.is_none() {
            record.subagent = invocation.subagent.clone();
        }
        Ok(())
    }

    /// Stores one failed task outcome while preserving the latest snapshot for later resumption.
    /// 保存一条失败的任务结果，同时保留最新快照供后续续跑使用。
    fn mark_task_failure(
        &self,
        current_session_id: SessionId,
        invocation: &TaskExecutionRequest,
        error: &str,
    ) -> Result<(), String> {
        let mut sessions = self.sessions.write().map_err(|_| {
            String::from("The task session registry is locked and cannot store the failure state.")
        })?;
        let record = sessions
            .entry(invocation.task_id.clone())
            .or_insert_with(|| {
                build_task_session_record(
                    invocation.task_id,
                    invocation.public_task_id.clone(),
                    current_session_id,
                    invocation.parent_task_id,
                    invocation.depth,
                    invocation.description.clone(),
                    invocation.subagent.clone(),
                    invocation.display_title.clone(),
                    invocation.controller_config.clone(),
                    invocation.snapshot.clone(),
                    invocation.prompt.clone(),
                    String::from("created"),
                )
            });
        debug_assert_eq!(record.task_id, invocation.task_id);
        let _ = record.origin_session_id;
        let _ = &record.created_at;
        record.last_result_text = error.to_string();
        record.updated_at = current_unix_millis_text();
        record.last_status = String::from("failed");
        record.display_title = invocation.display_title.clone();
        if record.controller_config.is_none() {
            record.controller_config = invocation.controller_config.clone();
        }
        Ok(())
    }
}

/// Builds one task tool candidate backed by the shared task runtime registry.
/// 根据共享任务运行时注册表构建一条任务工具候选项。
pub fn build_task_tool_candidate(task_runtime: Arc<TaskRuntime>) -> VulcanToolCandidate {
    VulcanToolCandidate {
        descriptor: VulcanToolDescriptor {
            id: String::from("task"),
            name: String::from("task"),
            description: String::from(
                "Create or resume a recoverable subtask session. Always provide `description` and `subagent` so the subtask keeps a stable title, identity, and resume target.",
            ),
            help_text: None,
            source: VulcanToolSourceKind::Native,
        },
        parameters_schema: task_tool_args_schema(),
        ai_visible: true,
        plan_policy: VulcanPlanToolPolicy::Hidden,
        invoke: Arc::new(move |args, session_id| {
            let task_runtime = Arc::clone(&task_runtime);
            Box::pin(async move {
                let Some(session_id) = session_id.as_deref() else {
                    return Err(String::from(
                        "The `task` tool requires a session identifier and cannot run without it.",
                    ));
                };
                let session_id = Uuid::parse_str(session_id).map_err(|error| {
                    format!("Failed to parse the session identifier for the `task` tool: {error}")
                })?;
                task_runtime.invoke_task(session_id, args).await
            })
        }),
    }
}

/// Builds the JSON Schema advertised to the model for the `task` tool.
/// 为模型端 `task` 工具构建对外声明的 JSON Schema。
fn task_tool_args_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "prompt": {
                "type": "string",
                "description": "Detailed prompt used to start or resume the subtask. State the goal, boundaries, expected return format, and whether code changes are allowed."
            },
            "description": {
                "type": "string",
                "description": "Short task title, ideally 3 to 8 words, used for stable naming, card titles, and resume display."
            },
            "task_id": {
                "type": "string",
                "description": "Optional stable task identifier. Provide it to continue the same subtask."
            },
            "subagent": {
                "type": "string",
                "description": "Subagent name or role label. This should usually be provided so the UI can display a stable worker identity and responsibility."
            }
        },
        "required": ["prompt", "description", "subagent"],
        "additionalProperties": false
    })
}

/// Parses one raw JSON payload into the strongly typed task invocation arguments.
/// 将一份原始 JSON 载荷解析为强类型任务调用参数。
fn parse_task_tool_arguments(args: Value) -> Result<TaskToolInvocationArgs, String> {
    serde_json::from_value(args)
        .map_err(|error| format!("Failed to parse task tool arguments: {error}"))
}

/// Resolves the stable task identifier and resume flag for the current invocation.
/// 为当前调用解析稳定任务标识与续跑标志。
fn resolve_task_session_id(
    task_runtime: &Arc<TaskRuntime>,
    current_session_id: SessionId,
    args: &TaskToolInvocationArgs,
) -> Result<ResolvedTaskSessionIdentifier, String> {
    if let Some(task_id_text) = args.task_id.as_ref() {
        let public_task_id = normalize_public_task_identifier(task_id_text)?;
        let resumed_record = task_runtime
            .sessions
            .read()
            .map_err(|_| {
                String::from(
                    "The task session registry is locked and cannot inspect existing tasks.",
                )
            })?
            .values()
            .find(|record| {
                record.origin_session_id == current_session_id
                    && record.public_task_id == public_task_id
            })
            .cloned();
        if let Some(record) = resumed_record {
            return Ok(ResolvedTaskSessionIdentifier {
                task_id: record.task_id,
                public_task_id,
                is_resumed: true,
            });
        }

        return Ok(ResolvedTaskSessionIdentifier {
            task_id: Uuid::new_v4(),
            public_task_id,
            is_resumed: false,
        });
    }

    let current_depth = task_runtime
        .sessions
        .read()
        .map_err(|_| {
            String::from("The task session registry is locked and cannot compute task depth.")
        })?
        .get(&current_session_id)
        .map(|record| record.depth)
        .unwrap_or(0);
    if current_depth + 1 > task_runtime.max_depth() {
        return Err(format!(
            "The current task chain has reached the maximum depth `{}` and cannot create another subtask.",
            task_runtime.max_depth()
        ));
    }

    let generated_task_id = Uuid::new_v4();
    Ok(ResolvedTaskSessionIdentifier {
        task_id: generated_task_id,
        public_task_id: generated_task_id.to_string(),
        is_resumed: false,
    })
}

/// Normalizes one caller-facing task identifier while preserving the original textual identity used for future resume lookups.
/// 在保留未来续跑查找所需原始文本身份的前提下，规范化一条调用方面向任务标识。
fn normalize_public_task_identifier(task_id_text: &str) -> Result<String, String> {
    let normalized_task_id = task_id_text.trim();
    if normalized_task_id.is_empty() {
        return Err(String::from("`task_id` cannot be empty."));
    }
    Ok(normalized_task_id.to_string())
}

/// Trims one optional task-facing label and drops it when only whitespace remains.
/// 裁剪一条可选任务标签，并在仅剩空白时将其丢弃。
fn normalize_optional_task_label(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

/// Builds one compact prompt preview that can serve as a last-resort task title fallback.
/// 构建一条紧凑的提示词预览文本，作为任务标题的最后兜底来源。
fn build_task_prompt_preview(prompt: &str) -> Option<String> {
    let compact = prompt
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or(prompt)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let compact = compact.trim();
    if compact.is_empty() {
        return None;
    }
    const MAX_PREVIEW_CHARS: usize = 48;
    let preview = compact.chars().take(MAX_PREVIEW_CHARS).collect::<String>();
    if compact.chars().count() > MAX_PREVIEW_CHARS {
        return Some(format!("{preview}..."));
    }
    Some(preview)
}

/// Builds one stable human-readable task title from the best available task metadata.
/// 根据当前可用的最佳任务元数据构建一条稳定的人类可读任务标题。
fn build_task_display_title(
    description: Option<&str>,
    subagent: Option<&str>,
    public_task_id: &str,
    prompt: &str,
    task_id: SessionId,
) -> String {
    let description = normalize_optional_task_label(description);
    let subagent = normalize_optional_task_label(subagent);
    match (description.as_deref(), subagent.as_deref()) {
        (Some(description), Some(subagent)) if description != subagent => {
            return format!("{description} ({subagent})");
        }
        (Some(description), _) => return description.to_string(),
        (_, Some(subagent)) => return subagent.to_string(),
        _ => {}
    }

    let normalized_public_task_id = public_task_id.trim();
    if !normalized_public_task_id.is_empty()
        && SessionId::parse_str(normalized_public_task_id).is_err()
    {
        return normalized_public_task_id.to_string();
    }
    if let Some(prompt_preview) = build_task_prompt_preview(prompt) {
        return prompt_preview;
    }
    if !normalized_public_task_id.is_empty() {
        return normalized_public_task_id.to_string();
    }
    format!("Subagent {}", task_id)
}

/// Prepares one task execution request from the current invocation and the stored registry state.
/// 根据当前调用与已存储注册表状态准备一份任务执行请求。
fn prepare_task_execution_request(
    task_runtime: &Arc<TaskRuntime>,
    current_session_id: SessionId,
    resolved_identifier: ResolvedTaskSessionIdentifier,
    args: TaskToolInvocationArgs,
) -> Result<TaskExecutionRequest, String> {
    let ResolvedTaskSessionIdentifier {
        task_id,
        public_task_id,
        is_resumed,
    } = resolved_identifier;
    let normalized_description = normalize_optional_task_label(args.description.as_deref());
    let normalized_subagent = normalize_optional_task_label(args.subagent.as_deref());
    let prompt = args.prompt;
    let (parent_task_id, depth, snapshot, controller_config, description, subagent, display_title) = {
        let mut sessions = task_runtime.sessions.write().map_err(|_| {
            String::from(
                "The task session registry is locked and cannot prepare the execution request.",
            )
        })?;
        if let Some(record) = sessions.get_mut(&task_id) {
            if is_resumed {
                record.updated_at = current_unix_millis_text();
                if record.description.is_none() {
                    record.description = normalized_description.clone();
                }
                if record.subagent.is_none() {
                    record.subagent = normalized_subagent.clone();
                }
            }
            let description = record
                .description
                .clone()
                .or_else(|| normalized_description.clone());
            let subagent = record
                .subagent
                .clone()
                .or_else(|| normalized_subagent.clone());
            let display_title = build_task_display_title(
                description.as_deref(),
                subagent.as_deref(),
                &public_task_id,
                &prompt,
                task_id,
            );
            record.display_title = display_title.clone();
            (
                record.parent_task_id,
                record.depth,
                record.snapshot.clone(),
                record.controller_config.clone(),
                description,
                subagent,
                display_title,
            )
        } else {
            let current_depth = sessions
                .get(&current_session_id)
                .map(|record| record.depth)
                .unwrap_or(0);
            let depth = if current_depth == 0 {
                1
            } else {
                current_depth + 1
            };
            if depth > task_runtime.max_depth() {
                return Err(format!(
                    "The current task chain has reached the maximum depth `{}` and cannot create another subtask.",
                    task_runtime.max_depth()
                ));
            }
            let parent_task_id = sessions
                .get(&current_session_id)
                .map(|record| record.task_id);
            let display_title = build_task_display_title(
                normalized_description.as_deref(),
                normalized_subagent.as_deref(),
                &public_task_id,
                &prompt,
                task_id,
            );
            let timestamp = current_unix_millis_text();
            sessions.insert(
                task_id,
                TaskSessionRecord {
                    task_id,
                    public_task_id: public_task_id.clone(),
                    origin_session_id: current_session_id,
                    parent_task_id,
                    depth,
                    description: normalized_description.clone(),
                    subagent: normalized_subagent.clone(),
                    display_title: display_title.clone(),
                    controller_config: None,
                    snapshot: CoreConversationSnapshot::default(),
                    last_prompt: prompt.clone(),
                    last_result_text: String::new(),
                    created_at: timestamp.clone(),
                    updated_at: timestamp,
                    last_status: String::from("created"),
                },
            );
            (
                parent_task_id,
                depth,
                CoreConversationSnapshot::default(),
                None,
                normalized_description.clone(),
                normalized_subagent.clone(),
                display_title,
            )
        }
    };

    Ok(TaskExecutionRequest {
        current_session_id,
        task_id,
        public_task_id,
        prompt,
        description,
        subagent,
        display_title,
        parent_task_id,
        depth,
        snapshot,
        controller_config,
        is_resumed,
    })
}

/// Builds one task-session record with the provided metadata and execution state.
/// 使用给定元数据与执行状态构建一条任务会话记录。
fn build_task_session_record(
    task_id: SessionId,
    public_task_id: String,
    origin_session_id: SessionId,
    parent_task_id: Option<SessionId>,
    depth: usize,
    description: Option<String>,
    subagent: Option<String>,
    display_title: String,
    controller_config: Option<CoreControllerConfig>,
    snapshot: CoreConversationSnapshot,
    last_prompt: String,
    last_status: String,
) -> TaskSessionRecord {
    let timestamp = current_unix_millis_text();
    TaskSessionRecord {
        task_id,
        public_task_id,
        origin_session_id,
        parent_task_id,
        depth,
        description,
        subagent,
        display_title,
        controller_config,
        snapshot,
        last_prompt,
        last_result_text: String::new(),
        created_at: timestamp.clone(),
        updated_at: timestamp,
        last_status,
    }
}

/// Builds one model-visible task invocation result from the execution outcome.
/// 根据执行结果构建一条模型可见的任务调用结果。
fn build_task_invocation_result(
    invocation: &TaskExecutionRequest,
    outcome: &TaskExecutionOutcome,
) -> VulcanToolInvocationResult {
    let task_status = task_invocation_status_text(invocation.is_resumed);
    let execution_status = outcome.execution_status.as_str();
    let model_output = json!({
        "task_id": invocation.public_task_id.clone(),
        "task_session_id": outcome.task_id.to_string(),
        "display_title": invocation.display_title.clone(),
        "status": task_status,
        "execution_status": execution_status,
        "origin_session_id": invocation.current_session_id.to_string(),
        "parent_task_id": invocation.parent_task_id.map(|value| value.to_string()),
        "depth": invocation.depth,
        "description": invocation.description.clone(),
        "subagent": invocation.subagent.clone(),
        "provider_turns_executed": outcome.provider_turns_executed,
        "tool_calls_processed": outcome.tool_calls_processed,
        "final_assistant_message_id": outcome.final_assistant_message_id.clone(),
        "result": outcome.final_output_text.clone(),
    });
    let conversation_state = (outcome.execution_status != TaskExecutionStatus::Running)
        .then(|| core_snapshot_to_session_conversation_state(&outcome.snapshot));
    let payload = build_task_host_payload(
        invocation,
        execution_status,
        outcome.provider_turns_executed,
        outcome.tool_calls_processed,
        outcome.final_assistant_message_id.as_deref(),
        outcome.final_output_text.as_str(),
        conversation_state.as_ref(),
        current_unix_millis_text(),
    );
    VulcanToolInvocationResult::from_model_output(model_output).with_host_result(
        VulcanToolHostResult {
            kind: String::from("task_session"),
            payload,
        },
    )
}

/// Returns one RFC3339-like Unix-millis text stamp for task-session records.
/// 为任务会话记录返回一条类似 RFC3339 的 Unix 毫秒时间戳文本。
fn current_unix_millis_text() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis().to_string())
        .unwrap_or_else(|_| String::from("0"))
}

/// Builds one backend handler that can execute a concrete task session using the provided host execution context.
/// 根据给定宿主执行上下文构建一条可执行具体任务会话的后端处理器。
pub(crate) fn build_task_execution_handler<A>(
    task_runtime: Arc<TaskRuntime>,
    provider_factory: Arc<impl Fn(ProviderLiveDeltaSender) -> A + Send + Sync + 'static>,
    base_controller_config: CoreControllerConfig,
    tools_manager: Arc<VulcanToolsManager>,
    resolved_tool_definitions: Vec<ResolvedToolDefinition>,
    parent_live_sender: ProviderLiveDeltaSender,
    provider_label: String,
) -> TaskExecutionHandler
where
    A: Clone
        + Send
        + Sync
        + 'static
        + vulcan_core::provider::VulcanStreamingProviderAdapter<
            Response = vulcan_core::types::ProviderTurnResponse,
        >,
{
    Arc::new(move |request: TaskExecutionRequest| {
        let task_runtime = Arc::clone(&task_runtime);
        let provider_factory = Arc::clone(&provider_factory);
        let base_controller_config = base_controller_config.clone();
        let tools_manager = Arc::clone(&tools_manager);
        let resolved_tool_definitions = resolved_tool_definitions.clone();
        let parent_live_sender = parent_live_sender.clone();
        let provider_label = provider_label.clone();
        Box::pin(async move {
            let task_id = request.task_id.clone();
            let started_payload = build_task_host_payload(
                &request,
                "running",
                0,
                0,
                None,
                "",
                None,
                current_unix_millis_text(),
            );
            let _ = parent_live_sender.send(Ok(ProviderStreamDelta::RuntimeEvent(
                build_task_delegate_runtime_event(
                    RuntimeEventKind::DelegateStarted,
                    &request,
                    "Running and waiting for the subagent to return a result.",
                    started_payload,
                ),
            )));
            let task_controller_config = build_task_controller_config(
                &base_controller_config,
                request.controller_config.clone(),
                &request.description,
                &request.subagent,
            );
            let background_task_runtime = Arc::clone(&task_runtime);
            let background_provider_factory = Arc::clone(&provider_factory);
            let background_tools_manager = Arc::clone(&tools_manager);
            let background_resolved_tool_definitions = resolved_tool_definitions.clone();
            let background_provider_label = provider_label.clone();
            let background_request = request.clone();
            let background_task_controller_config = task_controller_config.clone();
            tokio::spawn(async move {
                let background_task_id = background_request.task_id;
                let (child_live_sender, mut child_live_receiver) =
                    unbounded_channel::<Result<ProviderStreamDelta, String>>();
                let child_handler = build_task_execution_handler(
                    Arc::clone(&background_task_runtime),
                    Arc::clone(&background_provider_factory),
                    base_controller_config.clone(),
                    Arc::clone(&background_tools_manager),
                    background_resolved_tool_definitions.clone(),
                    child_live_sender.clone(),
                    background_provider_label.clone(),
                );
                let child_backend_guard =
                    background_task_runtime.install_backend(background_task_id, child_handler);
                let detached_stream_runtime = Arc::clone(&background_task_runtime);
                let child_stream_forward_task = tokio::spawn({
                    let streaming_controller = VulcanCoreController::new(
                        background_task_controller_config.clone(),
                        background_request.snapshot.clone(),
                    );
                    async move {
                        let mut child_stream_state =
                            RuntimeStreamDeltaState::new(streaming_controller);
                        while let Some(item) = child_live_receiver.recv().await {
                            for event in process_provider_stream_delta(
                                background_task_id,
                                &mut child_stream_state,
                                item,
                            ) {
                                detached_stream_runtime.publish_detached_task_event(
                                    background_task_id,
                                    DetachedTaskRuntimeEvent::ChildExecution(event),
                                );
                            }
                            if child_stream_state.has_terminal_error() {
                                break;
                            }
                        }
                        if let Some(event) =
                            child_stream_state.complete_pending_thinking(background_task_id)
                        {
                            detached_stream_runtime.publish_detached_task_event(
                                background_task_id,
                                DetachedTaskRuntimeEvent::ChildExecution(event),
                            );
                        }
                    }
                });
                let child_runtime_bridge = Arc::new(VulcanManagedToolRuntimeBridge::new(
                    Arc::clone(&background_tools_manager),
                    &background_resolved_tool_definitions,
                    background_task_id,
                ));
                let child_live_sender_for_run = child_live_sender.clone();
                let child_run_task = tokio::spawn({
                    let provider_factory = Arc::clone(&background_provider_factory);
                    let task_controller_config = background_task_controller_config.clone();
                    let request_snapshot = background_request.snapshot.clone();
                    let request_prompt = background_request.prompt.clone();
                    let child_runtime_bridge = Arc::clone(&child_runtime_bridge);
                    async move {
                        let child_provider_adapter =
                            provider_factory(child_live_sender_for_run.clone());
                        let child_tool_executor = StreamingRuntimeToolExecutor::new(
                            child_runtime_bridge,
                            child_live_sender_for_run.clone(),
                        );
                        let mut controller = VulcanCoreController::new(
                            task_controller_config.clone(),
                            request_snapshot,
                        );
                        let task_policy = AllowAllToolPolicy;
                        let controller_result = controller
                            .run_user_turn_streaming(
                                &child_provider_adapter,
                                &child_tool_executor,
                                &task_policy,
                                request_prompt,
                            )
                            .await
                            .map_err(|error| {
                                format!("Task `{}` failed: {error}", background_task_id)
                            });
                        (controller_result, controller.snapshot().clone())
                    }
                });
                let child_run_abort_handle = child_run_task.abort_handle();
                let _child_run_abort_guard = AbortChildRunOnDrop {
                    handle: child_run_abort_handle.clone(),
                };
                let _active_task_run_guard = background_task_runtime
                    .install_active_task_run(background_task_id, child_run_abort_handle);
                drop(child_live_sender);

                let child_run_result = child_run_task.await;
                // Drop the child backend before waiting for the detached live-forwarder so backend-held sender clones can close the channel.
                // 在等待分离实时转发任务结束前先释放子任务 backend，避免 backend 持有的 sender 克隆让通道无法关闭。
                drop(child_backend_guard);
                let terminal_outcome = match child_run_result {
                    Ok((controller_result, snapshot)) => {
                        let _ = child_stream_forward_task.await;
                        let child_conversation_state =
                            core_snapshot_to_session_conversation_state(&snapshot);
                        match controller_result {
                            Ok(controller_result) => {
                                let final_output_text =
                                    extract_final_assistant_text(&snapshot).unwrap_or_else(|| {
                                        if let Some(message_id) =
                                            controller_result.final_assistant_message_id.as_ref()
                                        {
                                            format!(
                                                "The task completed, but the final assistant message `{message_id}` could not be located directly."
                                            )
                                        } else {
                                            String::from(
                                                "The task completed, but it did not produce reusable final assistant text.",
                                            )
                                        }
                                    });
                                let outcome = TaskExecutionOutcome {
                                    task_id: background_task_id,
                                    controller_config: background_task_controller_config.clone(),
                                    snapshot,
                                    provider_turns_executed: controller_result
                                        .provider_turns_executed,
                                    tool_calls_processed: controller_result.tool_calls_processed,
                                    final_assistant_message_id: controller_result
                                        .final_assistant_message_id,
                                    final_output_text,
                                    execution_status: TaskExecutionStatus::Completed,
                                };
                                let finished_payload = build_task_host_payload(
                                    &background_request,
                                    outcome.execution_status.as_str(),
                                    outcome.provider_turns_executed,
                                    outcome.tool_calls_processed,
                                    outcome.final_assistant_message_id.as_deref(),
                                    outcome.final_output_text.as_str(),
                                    Some(&child_conversation_state),
                                    current_unix_millis_text(),
                                );
                                background_task_runtime.publish_detached_task_event(
                                    background_task_id,
                                    DetachedTaskRuntimeEvent::ParentRuntime {
                                        parent_session_id: background_request.current_session_id,
                                        event: build_task_delegate_runtime_event(
                                            RuntimeEventKind::DelegateFinished,
                                            &background_request,
                                            "The subagent task has completed.",
                                            finished_payload,
                                        ),
                                    },
                                );
                                Ok(outcome)
                            }
                            Err(error) if task_execution_error_is_stopped(error.as_str()) => {
                                let outcome = build_stopped_task_execution_outcome(
                                    background_task_id,
                                    background_task_controller_config.clone(),
                                    snapshot,
                                );
                                let finished_payload = build_task_host_payload(
                                    &background_request,
                                    outcome.execution_status.as_str(),
                                    outcome.provider_turns_executed,
                                    outcome.tool_calls_processed,
                                    outcome.final_assistant_message_id.as_deref(),
                                    outcome.final_output_text.as_str(),
                                    Some(&child_conversation_state),
                                    current_unix_millis_text(),
                                );
                                background_task_runtime.publish_detached_task_event(
                                    background_task_id,
                                    DetachedTaskRuntimeEvent::ParentRuntime {
                                        parent_session_id: background_request.current_session_id,
                                        event: build_task_delegate_runtime_event(
                                            RuntimeEventKind::DelegateFinished,
                                            &background_request,
                                            outcome.final_output_text.as_str(),
                                            finished_payload,
                                        ),
                                    },
                                );
                                Ok(outcome)
                            }
                            Err(error) => {
                                let finished_payload = build_task_host_payload(
                                    &background_request,
                                    "failed",
                                    0,
                                    0,
                                    None,
                                    error.as_str(),
                                    Some(&child_conversation_state),
                                    current_unix_millis_text(),
                                );
                                background_task_runtime.publish_detached_task_event(
                                    background_task_id,
                                    DetachedTaskRuntimeEvent::ParentRuntime {
                                        parent_session_id: background_request.current_session_id,
                                        event: build_task_delegate_runtime_event(
                                            RuntimeEventKind::DelegateFinished,
                                            &background_request,
                                            error.as_str(),
                                            finished_payload,
                                        ),
                                    },
                                );
                                Err(error)
                            }
                        }
                    }
                    Err(join_error) if join_error.is_cancelled() => {
                        let _ = child_stream_forward_task.await;
                        let outcome = build_stopped_task_execution_outcome(
                            background_task_id,
                            background_task_controller_config.clone(),
                            background_request.snapshot.clone(),
                        );
                        let finished_payload = build_task_host_payload(
                            &background_request,
                            outcome.execution_status.as_str(),
                            outcome.provider_turns_executed,
                            outcome.tool_calls_processed,
                            outcome.final_assistant_message_id.as_deref(),
                            outcome.final_output_text.as_str(),
                            None,
                            current_unix_millis_text(),
                        );
                        background_task_runtime.publish_detached_task_event(
                            background_task_id,
                            DetachedTaskRuntimeEvent::ParentRuntime {
                                parent_session_id: background_request.current_session_id,
                                event: build_task_delegate_runtime_event(
                                    RuntimeEventKind::DelegateFinished,
                                    &background_request,
                                    outcome.final_output_text.as_str(),
                                    finished_payload,
                                ),
                            },
                        );
                        Ok(outcome)
                    }
                    Err(join_error) => {
                        let _ = child_stream_forward_task.await;
                        let error = format!(
                            "Task `{}` failed: {}",
                            background_task_id,
                            format_task_join_error(&join_error)
                        );
                        let finished_payload = build_task_host_payload(
                            &background_request,
                            "failed",
                            0,
                            0,
                            None,
                            error.as_str(),
                            None,
                            current_unix_millis_text(),
                        );
                        background_task_runtime.publish_detached_task_event(
                            background_task_id,
                            DetachedTaskRuntimeEvent::ParentRuntime {
                                parent_session_id: background_request.current_session_id,
                                event: build_task_delegate_runtime_event(
                                    RuntimeEventKind::DelegateFinished,
                                    &background_request,
                                    error.as_str(),
                                    finished_payload,
                                ),
                            },
                        );
                        Err(error)
                    }
                };

                match terminal_outcome {
                    Ok(outcome) => {
                        let _ = background_task_runtime.store_task_outcome(
                            background_request.current_session_id,
                            &background_request,
                            &outcome,
                        );
                    }
                    Err(error) => {
                        let _ = background_task_runtime.mark_task_failure(
                            background_request.current_session_id,
                            &background_request,
                            &error,
                        );
                    }
                }
                background_task_runtime.close_detached_task_event_stream(background_task_id);
            });

            Ok(build_running_task_execution_outcome(
                task_id,
                task_controller_config,
                request.snapshot.clone(),
            ))
        })
    })
}

/// Builds one synthetic running task outcome returned immediately while the real child controller keeps executing in the background.
/// 构建一条会被立即返回的合成运行中任务结果，同时真实子控制器继续在后台执行。
fn build_running_task_execution_outcome(
    task_id: SessionId,
    controller_config: CoreControllerConfig,
    snapshot: CoreConversationSnapshot,
) -> TaskExecutionOutcome {
    TaskExecutionOutcome {
        task_id,
        controller_config,
        snapshot,
        provider_turns_executed: 0,
        tool_calls_processed: 0,
        final_assistant_message_id: None,
        final_output_text: String::from(RUNNING_TASK_RESULT_SUMMARY),
        execution_status: TaskExecutionStatus::Running,
    }
}

/// Builds one synthetic stopped task outcome used when a child subagent is intentionally interrupted before it can emit a normal final answer.
/// 当子代理在来得及产出正常最终答案之前被有意中断时，构建一条合成的停止结果。
fn build_stopped_task_execution_outcome(
    task_id: SessionId,
    controller_config: CoreControllerConfig,
    snapshot: CoreConversationSnapshot,
) -> TaskExecutionOutcome {
    TaskExecutionOutcome {
        task_id,
        controller_config,
        snapshot,
        provider_turns_executed: 0,
        tool_calls_processed: 0,
        final_assistant_message_id: None,
        final_output_text: String::from(STOPPED_TASK_RESULT_SUMMARY),
        execution_status: TaskExecutionStatus::Stopped,
    }
}

/// Formats one spawned child-task join error into a short user-facing diagnostic string.
/// 将一条已派生子任务的 JoinError 格式化为简短的面向用户诊断文本。
fn format_task_join_error(error: &tokio::task::JoinError) -> String {
    if error.is_panic() {
        String::from("The subagent task panicked internally.")
    } else if error.is_cancelled() {
        String::from("The subagent task was cancelled.")
    } else {
        error.to_string()
    }
}

/// Returns the persisted task-invocation status text used by both runtime metadata and tool results.
/// 返回运行时元数据与工具结果共同使用的持久化任务调用状态文本。
fn task_invocation_status_text(is_resumed: bool) -> &'static str {
    if is_resumed { "resumed" } else { "created" }
}

/// Returns whether one task execution error most likely came from a user-visible stop or transport cancellation rather than a real logical failure.
/// 返回某条任务执行错误是否更可能源于用户可见的停止或传输取消，而不是真正的逻辑失败。
fn task_execution_error_is_stopped(error: &str) -> bool {
    let normalized = error.to_ascii_lowercase();
    normalized.contains("cancelled")
        || normalized.contains("canceled")
        || normalized.contains("aborted")
        || normalized.contains("stopped")
}

/// Builds one host-only payload for task-session UI metadata without leaking large child snapshots into the model-visible tool result.
/// 构建一份仅供宿主界面使用的任务会话载荷，避免把庞大的子会话快照泄漏到模型可见工具结果中。
fn build_task_host_payload(
    invocation: &TaskExecutionRequest,
    execution_status: &str,
    provider_turns_executed: usize,
    tool_calls_processed: usize,
    final_assistant_message_id: Option<&str>,
    result_text: &str,
    conversation_state: Option<&SessionConversationState>,
    updated_at: String,
) -> Value {
    let mut payload = json!({
        "task_id": invocation.public_task_id.clone(),
        "task_session_id": invocation.task_id.to_string(),
        "display_title": invocation.display_title.clone(),
        "status": task_invocation_status_text(invocation.is_resumed),
        "execution_status": execution_status,
        "origin_session_id": invocation.current_session_id.to_string(),
        "parent_task_id": invocation.parent_task_id.map(|value| value.to_string()),
        "depth": invocation.depth,
        "prompt": invocation.prompt.clone(),
        "description": invocation.description.clone(),
        "subagent": invocation.subagent.clone(),
        "provider_turns_executed": provider_turns_executed,
        "tool_calls_processed": tool_calls_processed,
        "final_assistant_message_id": final_assistant_message_id,
        "result": result_text,
        "updated_at": updated_at,
    });
    if let Some(conversation_state) = conversation_state {
        payload["conversation_state"] = json!(conversation_state);
    }
    payload
}

/// Builds one delegate runtime event used by the parent session to track child task lifecycle without replaying child transcript output inline.
/// 构建一条供父会话追踪子任务生命周期的委派运行时事件，同时避免把子任务正文直接回放到父会话中。
fn build_task_delegate_runtime_event(
    kind: RuntimeEventKind,
    invocation: &TaskExecutionRequest,
    detail: &str,
    payload: Value,
) -> RuntimeEvent {
    let task_label = invocation.display_title.as_str();
    let title = match kind {
        RuntimeEventKind::DelegateStarted => {
            format!("Subagent started · {} · {task_label}", invocation.task_id)
        }
        RuntimeEventKind::DelegateFinished => {
            format!("Subagent finished · {} · {task_label}", invocation.task_id)
        }
        _ => format!("Subagent event · {} · {task_label}", invocation.task_id),
    };

    RuntimeEvent::new(kind, title, detail).with_metadata(Some(json!({
        "kind": "task_session",
        "payload": payload,
    })))
}

/// Builds one task controller configuration from the base host configuration and the task-specific metadata.
/// 根据基础宿主配置和任务专属元数据构建一份任务控制器配置。
fn build_task_controller_config(
    base: &CoreControllerConfig,
    stored: Option<CoreControllerConfig>,
    description: &Option<String>,
    subagent: &Option<String>,
) -> CoreControllerConfig {
    if let Some(stored) = stored {
        return stored;
    }

    let mut config = base.clone();
    let mut system_prompt = build_subagent_clone_system_prompt(base, description, subagent)
        .unwrap_or_else(|| build_legacy_task_system_prompt(base, description, subagent));
    append_task_windows_path_guidance(&mut system_prompt);
    if !system_prompt.trim().is_empty() {
        config.system_prompt = system_prompt;
    }
    config
}

/// Builds a clone-mode subagent system prompt through the prompt composer.
/// 通过提示词组合器构建分身模式子代理系统提示词。
fn build_subagent_clone_system_prompt(
    base: &CoreControllerConfig,
    description: &Option<String>,
    subagent: &Option<String>,
) -> Option<String> {
    let output = runtime_prompt_composer_engine()
        .compose(&PromptAssemblyInput {
            mode: PromptAssemblyMode::SubagentClone,
            model: ModelDescriptor {
                family: base.provider_id.clone(),
                model_id: base.model_id.clone(),
                supports_native_personality: true,
            },
            provider_prompt: Some(
                ProviderPromptSnapshot::new(base.system_prompt.clone())
                    .with_source_label("Parent runtime system prompt"),
            ),
            agent_prompt: None,
            personality: PersonalitySelection::Disabled,
            project_rules: None,
            environment_context: None,
            task_goal: None,
            tool_catalog: ToolCatalogSnapshot::new(Vec::new()),
            subagent: Some(SubagentSnapshot {
                identity: subagent
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .unwrap_or("subagent")
                    .to_string(),
                task: description
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .unwrap_or("Continue the delegated task.")
                    .to_string(),
                delegation: Some(String::from(
                    "Use the parent agent's existing context and return a concise result that the parent can merge back into the main flow.",
                )),
                functional_system_prompt: None,
            }),
            custom_developer_segments: Vec::new(),
            custom_user_segments: Vec::new(),
        })
        .ok()?;

    let prompt = render_subagent_prompt_output(&output);
    (!prompt.trim().is_empty()).then_some(prompt)
}

/// Renders clone-mode subagent composer output into the current single system-prompt channel.
/// 将分身模式子代理组合输出渲染到当前单一系统提示词通道。
fn render_subagent_prompt_output(output: &PromptAssemblyOutput) -> String {
    let mut sections = Vec::new();
    push_task_prompt_section(&mut sections, output.render_system_text());
    push_titled_task_prompt_section(
        &mut sections,
        "Subagent Developer Instructions",
        output.render_developer_text(),
    );
    push_titled_task_prompt_section(
        &mut sections,
        "Subagent Tool Guidance",
        output.render_tool_metadata_text(),
    );
    push_titled_task_prompt_section(&mut sections, "Subagent Context", output.render_user_text());
    sections.join("\n\n")
}

/// Builds the legacy child-task prompt when composer assembly fails.
/// 当组合器装配失败时构建旧版子任务提示词。
fn build_legacy_task_system_prompt(
    base: &CoreControllerConfig,
    description: &Option<String>,
    subagent: &Option<String>,
) -> String {
    let mut system_prompt = base.system_prompt.clone();
    if let Some(subagent) = subagent.as_ref().filter(|value| !value.trim().is_empty()) {
        system_prompt.push_str(&format!("\n\n[Subagent]\n{subagent}"));
    }
    if let Some(description) = description
        .as_ref()
        .filter(|value| !value.trim().is_empty())
    {
        system_prompt.push_str(&format!("\n\n[Task]\n{description}"));
    }
    system_prompt
}

/// Pushes one raw child-task prompt section when it has visible text.
/// 当子任务原始提示词分节包含可见文本时推入该分节。
fn push_task_prompt_section(sections: &mut Vec<String>, body: String) {
    let body = body.trim();
    if !body.is_empty() {
        sections.push(body.to_string());
    }
}

/// Pushes one titled child-task prompt section when it has visible text.
/// 当子任务带标题提示词分节包含可见文本时推入该分节。
fn push_titled_task_prompt_section(sections: &mut Vec<String>, title: &str, body: String) {
    let body = body.trim();
    if !body.is_empty() {
        sections.push(format!("## {title}\n{body}"));
    }
}

/// Appends one Windows-specific filesystem path guidance block to the child task system prompt.
/// 为子任务系统提示词追加一段 Windows 专用文件系统路径指引。
fn append_task_windows_path_guidance(system_prompt: &mut String) {
    if !cfg!(windows) || system_prompt.contains("[Filesystem Paths]") {
        return;
    }
    system_prompt.push_str("\n\n");
    system_prompt.push_str(WINDOWS_TASK_FILESYSTEM_PATH_GUIDANCE);
}

/// Extracts the last visible assistant text from one canonical conversation snapshot.
/// 从一份规范会话快照中提取最后一条可见助手文本。
fn extract_final_assistant_text(snapshot: &CoreConversationSnapshot) -> Option<String> {
    snapshot.messages.iter().rev().find_map(|message| {
        if let vulcan_core::types::ConversationPayload::Assistant { text, .. } = &message.payload {
            if !text.trim().is_empty() {
                return Some(text.clone());
            }
        }
        None
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds one minimal controller configuration for task-runtime prompt tests.
    /// 为 task-runtime 提示词测试构建一份最小控制器配置。
    fn build_test_controller_config(system_prompt: &str) -> CoreControllerConfig {
        CoreControllerConfig {
            provider_id: String::from("provider"),
            model_id: String::from("model"),
            system_prompt: system_prompt.to_string(),
            prompt_layers: vulcan_core::types::ProviderPromptLayers::from_system_prompt(
                system_prompt.to_string().clone(),
            ),
            temperature: None,
            max_turns: 8,
            additional_params: None,
            context_injection: None,
        }
    }

    /// Verifies that Windows child task prompts always include normalized filesystem path guidance.
    /// 验证 Windows 子任务提示词始终包含规范化文件系统路径指引。
    #[test]
    #[cfg(windows)]
    fn build_task_controller_config_appends_windows_path_guidance() {
        let base = build_test_controller_config("Base prompt");
        let description = Some(String::from("Analyze the current repository."));
        let subagent = Some(String::from("explorer"));

        let config = build_task_controller_config(&base, None, &description, &subagent);

        assert!(config.system_prompt.contains("Subagent identity: explorer"));
        assert!(
            config
                .system_prompt
                .contains("Delegated task: Analyze the current repository.")
        );
        assert!(config.system_prompt.contains("[Filesystem Paths]"));
        assert!(config.system_prompt.contains(r"C:\repo\path"));
    }

    /// Verifies stored child-task controller configs are reused without recomposition.
    /// 验证已存储的子任务控制器配置会直接复用而不会重新组合。
    #[test]
    fn build_task_controller_config_reuses_stored_config() {
        let base = build_test_controller_config("Base prompt");
        let stored = CoreControllerConfig {
            provider_id: String::from("stored-provider"),
            model_id: String::from("stored-model"),
            system_prompt: String::from("Stored prompt"),
            prompt_layers: vulcan_core::types::ProviderPromptLayers::from_system_prompt(
                String::from("Stored prompt").clone(),
            ),
            temperature: Some(0.7),
            max_turns: 3,
            additional_params: Some(json!({ "stored": true })),
            context_injection: None,
        };

        let config = build_task_controller_config(
            &base,
            Some(stored.clone()),
            &Some(String::from("Analyze the current repository.")),
            &Some(String::from("explorer")),
        );

        assert_eq!(config.system_prompt, stored.system_prompt);
        assert_eq!(config.provider_id, stored.provider_id);
        assert_eq!(config.max_turns, stored.max_turns);
    }

    /// Verifies new child-task prompts use the clone-mode prompt composer shape.
    /// 验证新建子任务提示词使用分身模式提示词组合器形态。
    #[test]
    fn build_task_controller_config_uses_subagent_clone_composer() {
        let base = build_test_controller_config("Base prompt");
        let description = Some(String::from("Analyze the current repository."));
        let subagent = Some(String::from("explorer"));

        let config = build_task_controller_config(&base, None, &description, &subagent);

        assert!(config.system_prompt.contains("Base prompt"));
        assert!(config.system_prompt.contains("Subagent identity: explorer"));
        assert!(
            config
                .system_prompt
                .contains("Delegated task: Analyze the current repository.")
        );
        assert!(
            config
                .system_prompt
                .contains("Subagent Developer Instructions")
        );
    }

    /// Verifies that caller-facing task identifiers are normalized consistently before being stored or looked up.
    /// 验证调用方面向任务标识在存储或查找前会被一致地规范化。
    #[test]
    fn normalize_public_task_identifier_trims_aliases_deterministically() {
        let first = normalize_public_task_identifier("explore_files")
            .expect("alias task id should resolve");
        let second = normalize_public_task_identifier(" explore_files ")
            .expect("trimmed alias task id should resolve");

        assert_eq!(first, second);
    }

    /// Verifies that distinct caller-facing task identifiers allocate distinct internal task sessions on first creation.
    /// 验证不同的调用方面向任务标识在首次创建时会分配不同的内部任务会话。
    #[test]
    fn resolve_task_session_id_keeps_distinct_aliases_unique() {
        let task_runtime = Arc::new(TaskRuntime::new());
        let parent_session_id = Uuid::new_v4();
        let first = resolve_task_session_id(
            &task_runtime,
            parent_session_id,
            &TaskToolInvocationArgs {
                prompt: String::from("count to 10"),
                description: None,
                task_id: Some(String::from("count_10")),
                subagent: None,
            },
        )
        .expect("first alias task id should resolve");
        let second = resolve_task_session_id(
            &task_runtime,
            parent_session_id,
            &TaskToolInvocationArgs {
                prompt: String::from("count to 20"),
                description: None,
                task_id: Some(String::from("count_20")),
                subagent: None,
            },
        )
        .expect("second alias task id should resolve");
        let third = resolve_task_session_id(
            &task_runtime,
            parent_session_id,
            &TaskToolInvocationArgs {
                prompt: String::from("count to 30"),
                description: None,
                task_id: Some(String::from("count_30")),
                subagent: None,
            },
        )
        .expect("third alias task id should resolve");

        assert_ne!(first.task_id, second.task_id);
        assert_ne!(second.task_id, third.task_id);
        assert_ne!(first.task_id, third.task_id);
        assert!(!first.is_resumed);
        assert!(!second.is_resumed);
        assert!(!third.is_resumed);
    }

    /// Verifies that repeated caller-facing task identifiers resume the same internal task session after the first creation.
    /// 验证重复的调用方面向任务标识会在首次创建后续跑同一条内部任务会话。
    #[test]
    fn resolve_task_session_id_reuses_existing_public_task_identifier() {
        let task_runtime = Arc::new(TaskRuntime::new());
        let parent_session_id = Uuid::new_v4();
        let first = resolve_task_session_id(
            &task_runtime,
            parent_session_id,
            &TaskToolInvocationArgs {
                prompt: String::from("inspect readme"),
                description: None,
                task_id: Some(String::from("research_readme")),
                subagent: None,
            },
        )
        .expect("first alias task id should resolve");

        task_runtime
            .sessions
            .write()
            .expect("task session registry should be writable")
            .insert(
                first.task_id,
                build_task_session_record(
                    first.task_id,
                    first.public_task_id.clone(),
                    parent_session_id,
                    None,
                    1,
                    None,
                    None,
                    String::from("research_readme"),
                    None,
                    CoreConversationSnapshot::default(),
                    String::from("inspect readme"),
                    String::from("created"),
                ),
            );

        let resumed = resolve_task_session_id(
            &task_runtime,
            parent_session_id,
            &TaskToolInvocationArgs {
                prompt: String::from("inspect readme again"),
                description: None,
                task_id: Some(String::from("research_readme")),
                subagent: None,
            },
        )
        .expect("existing alias task id should resume");

        assert_eq!(resumed.task_id, first.task_id);
        assert_eq!(resumed.public_task_id, first.public_task_id);
        assert!(resumed.is_resumed);
    }
}
