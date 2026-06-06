//! Resumable task-session runtime for host-managed task_id chaining and nested subtask execution.
//! 供宿主管理 task_id 续跑与嵌套子任务执行使用的可恢复任务会话运行时模块。

use crate::agent_runtime::events::{ExecutionStreamEvent, RuntimeEvent, RuntimeEventKind};
use crate::agent_runtime::execution_models::{ExecutionPreparationRequest, ExecutionRunReport};
use crate::agent_runtime::execution_preparation::prepare_execution;
use crate::agent_runtime::prompt_compaction::{
    PromptCompactionFoundationJobRequest, PromptCompactionStreamEvent,
    stream_prompt_compaction_foundation_job,
};
use crate::agent_runtime::prompt_composer_runtime::runtime_prompt_composer_engine;
use crate::agent_runtime::provider_request_bridge_conversation::{
    build_core_conversation_snapshot, core_snapshot_to_session_conversation_state,
};
use crate::agent_runtime::provider_stream::ProviderStreamDelta;
use crate::agent_runtime::provider_transport_live_delta::ProviderLiveDeltaSender;
use crate::agent_runtime::runtime::RuntimeFoundation;
use crate::agent_runtime::runtime_stream_bootstrap::bootstrap_runtime_stream;
use crate::agent_runtime::runtime_stream_driver_loop::drive_runtime_stream_provider_loop;
use crate::agent_runtime::runtime_stream_launch_payload::build_runtime_stream_launch_payload;
use crate::conversation_protocol::SessionConversationState;
use crate::luaskills_runtime::LuaSkillsRuntimeHandle;
use crate::model::SessionId;
use crate::provider_config::{PersistedPromptSkillState, ProviderConfigurationFile};
use async_stream::stream;
use futures_util::StreamExt;
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::{BTreeSet, HashMap, VecDeque};
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::{Arc, RwLock};
use time::OffsetDateTime;
use tokio::sync::mpsc::{UnboundedSender, unbounded_channel};
use tokio::task::{AbortHandle, JoinHandle};
use uuid::Uuid;
use vulcan_context::{ContextCompactionPolicy, conversation_state_to_session_messages};
use vulcan_context_compaction::{CompactionPlanInput, plan_context_compaction};
use vulcan_core::types::{
    ControllerConfig as CoreControllerConfig, ConversationSnapshot as CoreConversationSnapshot,
    ProviderPromptLayers as CoreProviderPromptLayers,
};
use vulcan_prompt_composer::{
    ModelDescriptor, PersonalitySelection, PromptAssemblyInput, PromptAssemblyMode,
    PromptAssemblyOutput, ProviderPromptSnapshot, SubagentSnapshot, ToolCatalogSnapshot,
};
use vulcan_session_core::{
    ContextCompactionPolicyRequest, ContextCompactionPolicySnapshot, ExecutionRuntimePolicyRequest,
    ExecutionScope, ParentSessionRef, PromptSkillSnapshot, ProviderModelSnapshot,
    SessionContextDocument, SessionEffectiveConfig, SessionEventKind, SessionEventRecord,
    SessionNodeId, SessionNodeStore, SessionRequestedConfig, SessionRuntimeCheckpoint,
    ToolExposureSnapshot,
};
use vulcan_task_session::{
    FileTaskSessionStore, TaskInvocationConfig, TaskInvocationEvent, TaskInvocationEventKind,
    TaskSessionCompactionStore, TaskSessionConfigResolutionInput, TaskSessionConfigResolver,
    TaskSessionMetadata, TaskSessionResolvedConfig, TaskSessionStatus, TaskSessionStore,
    apply_task_session_compaction_result, profile::SubagentProfile,
};
use vulcan_tools_manager::{
    VulcanPlanToolPolicy, VulcanToolCandidate, VulcanToolDescriptor, VulcanToolHostResult,
    VulcanToolInvocationResult, VulcanToolSourceKind,
};
use vulcan_workbench_model::{
    SubagentTaskPanelState, SubagentTaskRunState, build_delegate_completion_followup_prompt,
    task_session_tool_display_label,
};

/// Default nested-task depth limit used to prevent unbounded recursive task spawning.
/// 用于防止无限递归任务派生的默认嵌套任务深度上限。
const DEFAULT_TASK_NESTING_LIMIT: usize = 8;

/// Fallback stop summary returned to the parent task tool when the child run is manually interrupted before producing a complete answer.
/// 当子运行在产出完整答案前被人工中断时，返回给父级 task 工具的兜底停止摘要。
const STOPPED_TASK_RESULT_SUMMARY: &str = "The subagent was stopped at the user's request before producing a complete final result. Decide whether to retry, rewrite the task, or continue the main flow with the current context.";

/// Synthetic running summary returned to the parent task tool while the detached child run continues asynchronously in the background.
/// 当已分离的子运行仍在后台异步继续执行时，返回给父级 task 工具的合成运行中摘要。
const RUNNING_TASK_RESULT_SUMMARY: &str = "This is a running-status acknowledgement, not the subagent's final result. The subagent owns the delegated work now, and its result will be injected automatically when it finishes. Do not recreate the same task, do not replace the unfinished subagent work with direct main-thread or non-task tool calls, and do not produce a final answer from assumptions. If coordination requires a status check, call task with the same task_id only to inspect that existing subagent; otherwise keep the parent flow in a waiting/coordination state until the automatic callback arrives.";

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
    /// Optional subagent profile identifier.
    /// 可选的子代理配置标识符。
    subagent_profile_id: Option<String>,
    /// Short alias for the optional subagent profile identifier.
    /// 可选子代理配置标识符的短别名。
    profile: Option<String>,
    /// Optional inheritance override for the subagent run.
    /// 子代理运行的可选继承覆盖。
    inherit_parent_config: Option<bool>,
    /// Optional replacement system prompt for the subagent.
    /// 子代理的可选替换系统提示词。
    system_prompt: Option<String>,
    /// Optional appended system prompt for the subagent.
    /// 子代理的可选追加系统提示词。
    system_prompt_append: Option<String>,
    /// Optional provider identifier override for this subagent.
    /// 子代理的可选供应商标识覆盖。
    provider: Option<String>,
    /// Optional model identifier override for this subagent.
    /// 子代理的可选模型标识覆盖。
    model: Option<String>,
    /// Optional reasoning effort override for this subagent.
    /// 子代理的可选推理等级覆盖。
    reasoning_effort: Option<String>,
    /// Optional execution-mode override for this subagent.
    /// 子代理的可选执行模式覆盖。
    execution_mode: Option<String>,
    /// Flexible provider/model override payload.
    /// 灵活的供应商与模型覆盖载荷。
    provider_model: Option<Value>,
    /// Flexible tool allow/deny override payload.
    /// 灵活的工具允许与禁用覆盖载荷。
    tools: Option<Value>,
    /// Flexible AI capability allow/deny override payload.
    /// 灵活的 AI 能力允许与禁用覆盖载荷。
    ai_capabilities: Option<Value>,
    /// Flexible prompt-skill override payload.
    /// 灵活的提示词技能覆盖载荷。
    skills: Option<Value>,
    /// Alias for the prompt-skill override payload.
    /// 提示词技能覆盖载荷的别名。
    prompt_skills: Option<Value>,
    /// LuaSkills override payload stored in the TaskSession configuration snapshot.
    /// 写入 TaskSession 配置快照的 LuaSkills 覆盖载荷。
    luaskills: Option<Value>,
    /// MCP override payload stored in the TaskSession configuration snapshot.
    /// 写入 TaskSession 配置快照的 MCP 覆盖载荷。
    mcp: Option<Value>,
    /// Context-compaction override payload.
    /// 上下文压缩覆盖载荷。
    context_compaction: Option<Value>,
    /// Runtime-policy override payload.
    /// 运行时策略覆盖载荷。
    runtime_policy: Option<Value>,
    /// Additional caller-owned configuration payload.
    /// 调用方拥有的额外配置载荷。
    extra: Option<Value>,
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
    /// Task invocation configuration resolved from tool arguments.
    /// 从工具参数解析出的任务调用配置。
    invocation_config: TaskInvocationConfig,
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
    /// Root main-session identifier that owns the whole task tree.
    /// 拥有整棵任务树的根主会话标识。
    pub root_session_id: SessionId,
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
    /// Task invocation configuration resolved from model-visible arguments.
    /// 从模型可见参数解析出的任务调用配置。
    pub invocation_config: TaskInvocationConfig,
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
    /// Resolved TaskSession configuration used by execution and persistence.
    /// 执行与持久化使用的已解析 TaskSession 配置。
    pub resolved_config: TaskSessionResolvedConfig,
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

/// Terminal child-task completion plus post-run maintenance hints.
/// 子任务终态结果以及运行后维护提示。
#[derive(Debug, Clone)]
struct TaskExecutionCompletion {
    /// Terminal task execution outcome.
    /// 任务执行终态结果。
    outcome: TaskExecutionOutcome,
    /// Whether the terminal provider request indicated that context compaction should run.
    /// 终态 Provider 请求是否表明应执行上下文压缩。
    should_compact_context: bool,
}

/// Queued delegate-completion prompt waiting to resume one parent TaskSession.
/// 等待恢复某个父级 TaskSession 的委派完成提示词。
#[derive(Clone)]
struct TaskSessionParentFollowup {
    /// Root main-session identifier used to resolve the file-backed task store.
    /// 用于解析文件态任务存储的根主会话标识。
    root_session_id: SessionId,
    /// Child task whose terminal result produced this follow-up.
    /// 产生该跟进提示词的终态子任务。
    child_task_id: SessionId,
    /// Shared runtime context template inherited from the parent TaskSession run.
    /// 从父级 TaskSession 运行继承来的共享运行时上下文模板。
    context: TaskExecutionRuntimeContext,
    /// Synthetic delegate-completion prompt injected into the parent TaskSession.
    /// 注入父级 TaskSession 的合成委派完成提示词。
    prompt: String,
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
    /// File-backed metadata loaded when the in-memory registry had no matching record.
    /// 当内存注册表没有匹配记录时从文件态加载的元数据。
    file_metadata: Option<TaskSessionMetadata>,
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

/// Shared execution context captured for a controller session so task runs can reuse the normal runtime preparation bridge.
/// 为控制器会话捕获的共享执行上下文，使子任务运行能够复用普通运行时准备桥。
#[derive(Clone)]
pub(crate) struct TaskExecutionRuntimeContext {
    /// Provider configuration snapshot used by the parent execution.
    /// 父级执行使用的供应商配置快照。
    pub(crate) config: ProviderConfigurationFile,
    /// Runtime foundation that already owns resolved agents, tools, LuaSkills, MCP, and shared runtimes.
    /// 已持有解析后代理、工具、LuaSkills、MCP 与共享运行时的运行时基础架构。
    pub(crate) foundation: Arc<RuntimeFoundation>,
    /// Optional LuaSkills runtime handle reused by normal execution preparation.
    /// 普通执行准备链复用的可选 LuaSkills 运行时句柄。
    pub(crate) luaskills_runtime: Option<LuaSkillsRuntimeHandle>,
    /// Parent execution request used as the template for child TaskSession requests.
    /// 用作子代理 TaskSession 请求模板的父级执行请求。
    pub(crate) request_template: ExecutionPreparationRequest,
}

impl TaskExecutionRuntimeContext {
    /// Creates one shared task-execution runtime context from the already prepared parent run inputs.
    /// 基于已经准备好的父级运行输入创建一份共享子任务执行上下文。
    pub(crate) fn new(
        config: ProviderConfigurationFile,
        foundation: Arc<RuntimeFoundation>,
        luaskills_runtime: Option<LuaSkillsRuntimeHandle>,
        request_template: ExecutionPreparationRequest,
    ) -> Self {
        Self {
            config,
            foundation,
            luaskills_runtime,
            request_template,
        }
    }
}

/// Guard that unregisters one task-execution context when the owning provider loop exits.
/// 在所属 Provider 循环退出时自动注销某个子任务执行上下文的守卫对象。
pub(crate) struct TaskExecutionRuntimeContextGuard {
    /// Shared runtime registry that owns the execution-context map.
    /// 持有执行上下文映射表的共享运行时注册表。
    runtime: Arc<TaskRuntime>,
    /// Session identifier whose context should be removed on drop.
    /// 在 Drop 时应移除其上下文的会话标识。
    session_id: SessionId,
}

impl Drop for TaskExecutionRuntimeContextGuard {
    /// Removes the installed execution context as soon as the guard leaves scope.
    /// 在守卫离开作用域时立即移除已安装的执行上下文。
    fn drop(&mut self) {
        self.runtime.remove_execution_context(self.session_id);
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
    /// Shared execution contexts keyed by controller session identifier for normal-preparation child task launches.
    /// 按控制器会话标识索引的共享执行上下文，用于普通准备链子任务启动。
    execution_contexts: RwLock<HashMap<SessionId, TaskExecutionRuntimeContext>>,
    /// Active child-task abort handles keyed by stable task identifier.
    /// 按稳定任务标识索引存储的活动子任务中止句柄。
    active_task_runs: RwLock<HashMap<SessionId, AbortHandle>>,
    /// Queued parent TaskSession follow-ups keyed by the parent task identifier.
    /// 按父级任务标识索引的父级 TaskSession 跟进队列。
    task_session_parent_followups: RwLock<HashMap<SessionId, VecDeque<TaskSessionParentFollowup>>>,
    /// Parent TaskSession identifiers that are currently being prepared for a follow-up run before the abort handle exists.
    /// 正在准备父级跟进运行且尚未生成中止句柄的父级 TaskSession 标识。
    task_session_parent_followup_starting: RwLock<BTreeSet<SessionId>>,
    /// Detached child-task event streams keyed by stable task identifier for non-blocking GUI mirroring.
    /// 为无阻塞图形界面镜像保留的按稳定任务标识索引的分离子任务事件流。
    detached_task_event_streams: RwLock<HashMap<SessionId, DetachedTaskEventStreamState>>,
    /// File-domain roots keyed by root main-session identifier.
    /// 按根主会话标识索引的文件态子代理会话根目录。
    task_session_store_roots: RwLock<HashMap<SessionId, PathBuf>>,
    /// Runtime-resolved subagent profiles keyed by profile identifier.
    /// 按配置标识索引的运行时已解析子代理配置。
    subagent_profiles: RwLock<HashMap<String, SubagentProfile>>,
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
            execution_contexts: RwLock::new(HashMap::new()),
            active_task_runs: RwLock::new(HashMap::new()),
            task_session_parent_followups: RwLock::new(HashMap::new()),
            task_session_parent_followup_starting: RwLock::new(BTreeSet::new()),
            detached_task_event_streams: RwLock::new(HashMap::new()),
            task_session_store_roots: RwLock::new(HashMap::new()),
            subagent_profiles: RwLock::new(HashMap::new()),
            max_depth: DEFAULT_TASK_NESTING_LIMIT,
        }
    }

    /// Replaces the runtime-visible subagent profile snapshot with the latest foundation-resolved profiles.
    /// 使用最新基础架构解析结果替换运行时可见的子代理配置快照。
    pub fn configure_subagent_profiles(
        &self,
        profiles: Vec<SubagentProfile>,
    ) -> Result<(), String> {
        let mut profile_map = self.subagent_profiles.write().map_err(|_| {
            String::from("The subagent profile registry is locked and cannot update.")
        })?;
        profile_map.clear();
        profile_map.extend(
            profiles
                .into_iter()
                .map(|profile| (profile.id.clone(), profile)),
        );
        Ok(())
    }

    /// Returns the maximum supported nested task depth.
    /// 返回支持的最大嵌套任务深度。
    pub fn max_depth(&self) -> usize {
        self.max_depth
    }

    /// Registers the file-domain root used by TaskSession artifacts under one root main session.
    /// 注册某个根主会话下 TaskSession 文件产物使用的文件域根目录。
    pub fn configure_task_session_store_root(
        &self,
        root_session_id: SessionId,
        root_session_dir: PathBuf,
    ) -> Result<(), String> {
        self.task_session_store_roots
            .write()
            .map_err(|_| {
                String::from("The task-session store root registry is locked and cannot update.")
            })?
            .insert(root_session_id, root_session_dir);
        Ok(())
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

    /// Installs one shared execution context for the given controller session and returns a guard that unregisters it on drop.
    /// 为给定控制器会话安装一份共享执行上下文，并返回一个在 Drop 时自动注销它的守卫。
    pub(crate) fn install_execution_context(
        self: &Arc<Self>,
        session_id: SessionId,
        context: TaskExecutionRuntimeContext,
    ) -> TaskExecutionRuntimeContextGuard {
        self.execution_contexts
            .write()
            .expect("task execution context registry lock")
            .insert(session_id, context);
        TaskExecutionRuntimeContextGuard {
            runtime: Arc::clone(self),
            session_id,
        }
    }

    /// Returns one shared execution context for the given controller session when the normal runtime bridge registered it.
    /// 当普通运行时桥已经注册时，返回给定控制器会话对应的一份共享执行上下文。
    fn execution_context_for_session(
        &self,
        session_id: SessionId,
    ) -> Option<TaskExecutionRuntimeContext> {
        self.execution_contexts
            .read()
            .expect("task execution context registry lock")
            .get(&session_id)
            .cloned()
    }

    /// Removes one shared execution context from the registry.
    /// 从注册表中移除一份共享执行上下文。
    fn remove_execution_context(&self, session_id: SessionId) {
        self.execution_contexts
            .write()
            .expect("task execution context registry lock")
            .remove(&session_id);
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
        if self.task_has_active_or_starting_run(resolved_identifier.task_id)? {
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

        if outcome.execution_status != TaskExecutionStatus::Running {
            self.store_task_outcome(current_session_id, &invocation, &outcome)?;
        }

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
        } else if self.task_session_parent_followup_launch_is_reserved(task_id) {
            self.clear_task_session_parent_followups(task_id);
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Stops one in-flight child task and every active descendant task in its TaskSession tree.
    /// 停止一条进行中的子任务及其 TaskSession 树中所有仍活动的后代任务。
    pub fn stop_task_tree(&self, task_id: SessionId) -> Result<Vec<SessionId>, String> {
        let task_ids = self.task_subtree_ids(task_id)?;
        let handles = {
            let active_task_runs = self.active_task_runs.read().map_err(|_| {
                String::from(
                    "The active task registry is locked and cannot stop the subagent task tree.",
                )
            })?;
            task_ids
                .iter()
                .copied()
                .filter_map(|candidate_task_id| {
                    active_task_runs
                        .get(&candidate_task_id)
                        .cloned()
                        .map(|handle| (candidate_task_id, handle))
                })
                .collect::<Vec<_>>()
        };
        let starting_task_ids = {
            let starting = self.task_session_parent_followup_starting.read().map_err(|_| {
                String::from(
                    "The parent follow-up launch registry is locked and cannot stop the subagent task tree.",
                )
            })?;
            task_ids
                .iter()
                .copied()
                .filter(|candidate_task_id| starting.contains(candidate_task_id))
                .collect::<Vec<_>>()
        };
        for (_, handle) in &handles {
            handle.abort();
        }
        for starting_task_id in &starting_task_ids {
            self.clear_task_session_parent_followups(*starting_task_id);
        }
        let mut stopped_task_ids = handles
            .into_iter()
            .map(|(stopped_task_id, _)| stopped_task_id)
            .collect::<Vec<_>>();
        for starting_task_id in starting_task_ids {
            if !stopped_task_ids.contains(&starting_task_id) {
                stopped_task_ids.push(starting_task_id);
            }
        }
        Ok(stopped_task_ids)
    }

    /// Returns whether one child task still owns an in-flight abort handle, which the GUI can treat as the source of truth for active-running state.
    /// 返回某个子任务当前是否仍持有活动中的中止句柄，供图形界面将其视作真实运行态事实源。
    pub fn is_task_running(&self, task_id: SessionId) -> Result<bool, String> {
        self.task_has_active_or_starting_run(task_id)
    }

    /// Returns whether one task owns an active abort handle or is currently reserving a follow-up launch.
    /// 返回某个任务是否持有活动中止句柄，或正在预留一轮跟进启动。
    fn task_has_active_or_starting_run(&self, task_id: SessionId) -> Result<bool, String> {
        if self
            .active_task_runs
            .read()
            .map_err(|_| {
                String::from(
                    "The active task registry is locked and cannot inspect the subagent run state.",
                )
            })?
            .contains_key(&task_id)
        {
            return Ok(true);
        }
        Ok(self
            .task_session_parent_followup_starting
            .read()
            .map_err(|_| {
                String::from(
                    "The parent follow-up launch registry is locked and cannot inspect the subagent run state.",
                )
            })?
            .contains(&task_id))
    }

    /// Collects one task identifier and all known descendant task identifiers from the runtime registry.
    /// 从运行时注册表中收集一个任务标识及其所有已知后代任务标识。
    fn task_subtree_ids(&self, task_id: SessionId) -> Result<Vec<SessionId>, String> {
        let sessions = self.sessions.read().map_err(|_| {
            String::from("The task-session registry is locked and cannot inspect descendants.")
        })?;
        let mut task_ids = Vec::new();
        let mut pending = vec![task_id];
        while let Some(current_task_id) = pending.pop() {
            if task_ids.contains(&current_task_id) {
                continue;
            }
            task_ids.push(current_task_id);
            pending.extend(
                sessions
                    .values()
                    .filter(|record| record.parent_task_id == Some(current_task_id))
                    .map(|record| record.task_id),
            );
        }
        Ok(task_ids)
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

    /// Attempts to reserve one parent TaskSession follow-up launch before an abort handle can be installed.
    /// 在中止句柄尚不可安装前，尝试预留一轮父级 TaskSession 跟进启动。
    fn begin_task_session_parent_followup_launch(
        &self,
        parent_task_id: SessionId,
    ) -> Result<bool, String> {
        if self
            .active_task_runs
            .read()
            .map_err(|_| {
                String::from(
                    "The active task registry is locked and cannot inspect parent follow-up state.",
                )
            })?
            .contains_key(&parent_task_id)
        {
            return Ok(false);
        }
        let mut starting = self
            .task_session_parent_followup_starting
            .write()
            .map_err(|_| {
                String::from("The parent follow-up launch registry is locked and cannot update.")
            })?;
        if starting.contains(&parent_task_id) {
            return Ok(false);
        }
        starting.insert(parent_task_id);
        Ok(true)
    }

    /// Releases one reserved parent TaskSession follow-up launch after it either starts or aborts preparation.
    /// 在父级 TaskSession 跟进启动成功或准备中止后释放一条启动预留。
    fn finish_task_session_parent_followup_launch(&self, parent_task_id: SessionId) {
        if let Ok(mut starting) = self.task_session_parent_followup_starting.write() {
            starting.remove(&parent_task_id);
        }
    }

    /// Returns whether one parent TaskSession follow-up launch is still reserved.
    /// 返回某个父级 TaskSession 跟进启动预留是否仍然存在。
    fn task_session_parent_followup_launch_is_reserved(&self, parent_task_id: SessionId) -> bool {
        self.task_session_parent_followup_starting
            .read()
            .map(|starting| starting.contains(&parent_task_id))
            .unwrap_or(false)
    }

    /// Pushes one parent TaskSession follow-up while preserving child completion order and avoiding duplicate terminal callbacks.
    /// 推入一条父级 TaskSession 跟进提示词，同时保留子任务完成顺序并避免重复终态回调。
    fn queue_task_session_parent_followup(
        &self,
        parent_task_id: SessionId,
        followup: TaskSessionParentFollowup,
    ) -> Result<bool, String> {
        let mut queues = self.task_session_parent_followups.write().map_err(|_| {
            String::from("The task-session follow-up queue is locked and cannot enqueue.")
        })?;
        let queue = queues.entry(parent_task_id).or_default();
        if queue
            .iter()
            .any(|entry| entry.child_task_id == followup.child_task_id)
        {
            return Ok(false);
        }
        queue.push_back(followup);
        Ok(true)
    }

    /// Pops the next queued parent TaskSession follow-up for sequential execution.
    /// 按顺序弹出下一条父级 TaskSession 跟进提示词用于串行执行。
    fn pop_task_session_parent_followup(
        &self,
        parent_task_id: SessionId,
    ) -> Result<Option<TaskSessionParentFollowup>, String> {
        let mut queues = self.task_session_parent_followups.write().map_err(|_| {
            String::from("The task-session follow-up queue is locked and cannot dequeue.")
        })?;
        let Some(queue) = queues.get_mut(&parent_task_id) else {
            return Ok(None);
        };
        let followup = queue.pop_front();
        if queue.is_empty() {
            queues.remove(&parent_task_id);
        }
        Ok(followup)
    }

    /// Clears all queued parent TaskSession follow-ups after the parent itself stops or fails.
    /// 当父级自身停止或失败后清空所有等待中的父级 TaskSession 跟进。
    fn clear_task_session_parent_followups(&self, parent_task_id: SessionId) {
        if let Ok(mut queues) = self.task_session_parent_followups.write() {
            queues.remove(&parent_task_id);
        }
        self.finish_task_session_parent_followup_launch(parent_task_id);
    }

    /// Returns whether any known descendant task still has an active provider run.
    /// 返回任一已知后代任务是否仍持有活动 Provider 运行。
    fn task_has_active_descendant_runs(&self, task_id: SessionId) -> bool {
        let Ok(task_ids) = self.task_subtree_ids(task_id) else {
            return false;
        };
        let Ok(active_runs) = self.active_task_runs.read() else {
            return true;
        };
        task_ids
            .into_iter()
            .filter(|candidate_task_id| *candidate_task_id != task_id)
            .any(|candidate_task_id| active_runs.contains_key(&candidate_task_id))
    }

    /// Returns whether the parent TaskSession may still accept delegated child results.
    /// 返回父级 TaskSession 是否仍可接收委派子任务结果。
    fn task_session_parent_accepts_followup(
        &self,
        parent_task_id: SessionId,
        root_session_id: SessionId,
    ) -> Result<bool, String> {
        if let Some(record) = self
            .sessions
            .read()
            .map_err(|_| {
                String::from("The task-session registry is locked and cannot inspect the parent.")
            })?
            .get(&parent_task_id)
            .cloned()
        {
            return Ok(!matches!(
                record.last_status.as_str(),
                "failed" | "stopped" | "archived"
            ));
        }

        let Some(store) = self.task_session_store_for_root(root_session_id)? else {
            return Ok(false);
        };
        let metadata = store
            .load_task_session(SessionNodeId::from(parent_task_id))
            .map_err(|error| error.message)?;
        Ok(metadata.is_some_and(|metadata| {
            !matches!(
                metadata.status,
                TaskSessionStatus::Failed
                    | TaskSessionStatus::Stopped
                    | TaskSessionStatus::Archived
            )
        }))
    }

    /// Starts the next queued parent TaskSession follow-up when the parent is not already executing.
    /// 当父级当前没有执行时，启动下一条排队中的父级 TaskSession 跟进。
    fn try_spawn_next_task_session_parent_followup(
        self: &Arc<Self>,
        parent_task_id: SessionId,
    ) -> bool {
        match self.begin_task_session_parent_followup_launch(parent_task_id) {
            Ok(true) => {}
            Ok(false) => return false,
            Err(error) => {
                eprintln!(
                    "[task-runtime] failed to reserve parent TaskSession follow-up for `{parent_task_id}`: {error}"
                );
                return false;
            }
        }

        let spawn_plan = loop {
            let followup = match self.pop_task_session_parent_followup(parent_task_id) {
                Ok(Some(followup)) => followup,
                Ok(None) => break None,
                Err(error) => {
                    eprintln!("[task-runtime] failed to pop parent follow-up: {error}");
                    break None;
                }
            };

            if !self
                .task_session_parent_accepts_followup(parent_task_id, followup.root_session_id)
                .unwrap_or(false)
            {
                continue;
            }

            let request = match self.build_task_session_parent_followup_request(
                parent_task_id,
                followup.root_session_id,
                followup.prompt.clone(),
            ) {
                Ok(request) => request,
                Err(error) => {
                    eprintln!(
                        "[task-runtime] failed to build parent TaskSession follow-up request for `{parent_task_id}`: {error}"
                    );
                    continue;
                }
            };
            break Some((followup, request.0, request.1));
        };

        let Some((followup, request, resolved_config)) = spawn_plan else {
            self.finish_task_session_parent_followup_launch(parent_task_id);
            return false;
        };
        if !self
            .task_session_parent_accepts_followup(parent_task_id, followup.root_session_id)
            .unwrap_or(false)
            || !self.task_session_parent_followup_launch_is_reserved(parent_task_id)
        {
            self.finish_task_session_parent_followup_launch(parent_task_id);
            return false;
        }

        let runtime = Arc::clone(self);
        let run_context = followup.context.clone();
        let finalize_context = followup.context;
        let finalize_request = request.clone();
        let finalize_resolved_config = resolved_config.clone();
        let run_task = tokio::spawn(async move {
            run_task_session_parent_followup_stream(
                Arc::clone(&runtime),
                run_context,
                request,
                resolved_config,
            )
            .await
        });
        let abort_handle = run_task.abort_handle();
        let abort_guard = AbortChildRunOnDrop {
            handle: abort_handle.clone(),
        };
        let active_run_guard = self.install_active_task_run(parent_task_id, abort_handle);
        self.finish_task_session_parent_followup_launch(parent_task_id);
        let runtime = Arc::clone(self);
        tokio::spawn(async move {
            finalize_task_session_parent_followup_run(
                runtime,
                parent_task_id,
                finalize_context,
                finalize_request,
                finalize_resolved_config,
                run_task,
                abort_guard,
                active_run_guard,
            )
            .await;
        });
        true
    }

    /// Builds the execution request and resolved config used to resume one parent TaskSession from a queued delegate result.
    /// 构建用于根据排队委派结果恢复父级 TaskSession 的执行请求与已解析配置。
    fn build_task_session_parent_followup_request(
        &self,
        parent_task_id: SessionId,
        root_session_id: SessionId,
        prompt: String,
    ) -> Result<(TaskExecutionRequest, TaskSessionResolvedConfig), String> {
        let record = self.task_session_record_for_parent_followup(
            parent_task_id,
            root_session_id,
            prompt.clone(),
        )?;
        let direct_parent_session_id = record.parent_task_id.unwrap_or(record.origin_session_id);
        let resolved_config =
            self.resolve_task_session_config_for_record(&record, record.controller_config.as_ref());
        Ok((
            TaskExecutionRequest {
                current_session_id: direct_parent_session_id,
                root_session_id: record.origin_session_id,
                task_id: record.task_id,
                public_task_id: record.public_task_id.clone(),
                prompt,
                description: record.description.clone(),
                subagent: record.subagent.clone(),
                invocation_config: record.invocation_config.clone(),
                display_title: record.display_title.clone(),
                parent_task_id: record.parent_task_id,
                depth: record.depth,
                snapshot: record.snapshot.clone(),
                controller_config: record.controller_config.clone(),
                is_resumed: true,
            },
            resolved_config,
        ))
    }

    /// Loads one parent TaskSession record from memory or from its file-backed context before a follow-up run starts.
    /// 在跟进运行开始前，从内存或文件态上下文加载一条父级 TaskSession 记录。
    fn task_session_record_for_parent_followup(
        &self,
        parent_task_id: SessionId,
        root_session_id: SessionId,
        prompt: String,
    ) -> Result<TaskSessionRecord, String> {
        if let Some(mut record) = self
            .sessions
            .read()
            .map_err(|_| {
                String::from("The task-session registry is locked and cannot load the parent.")
            })?
            .get(&parent_task_id)
            .cloned()
        {
            if let Some(context) = self.load_file_backed_task_context(
                SessionNodeId::from(record.origin_session_id),
                SessionNodeId::from(parent_task_id),
            )? {
                record.snapshot = task_conversation_snapshot_from_context(&context)?;
            }
            record.last_prompt = prompt;
            return Ok(record);
        }

        let Some(store) = self.task_session_store_for_root(root_session_id)? else {
            return Err(format!(
                "Parent TaskSession `{parent_task_id}` cannot be resumed because root `{root_session_id}` has no file-backed task store."
            ));
        };
        let task_session_id = SessionNodeId::from(parent_task_id);
        let Some(metadata) = store
            .load_task_session(task_session_id)
            .map_err(|error| error.message)?
        else {
            return Err(format!(
                "Parent TaskSession `{parent_task_id}` cannot be resumed because no file-backed record exists under root `{root_session_id}`."
            ));
        };
        let context = store
            .load_context(task_session_id)
            .map_err(|error| error.message)?;
        let record = task_session_record_from_file_metadata(
            &metadata,
            context.as_ref(),
            prompt.clone(),
            TaskInvocationConfig::main(prompt.clone()),
            None,
            None,
        )?;
        self.sessions
            .write()
            .map_err(|_| {
                String::from("The task-session registry is locked and cannot cache parent.")
            })?
            .insert(parent_task_id, record.clone());
        Ok(record)
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
        let record = self.upsert_task_outcome_record(current_session_id, invocation, outcome)?;
        self.persist_task_record_outcome(&record, invocation, outcome);
        Ok(())
    }

    /// Stores one running task outcome in memory without blocking the parent tool call on file persistence.
    /// 仅在内存中保存运行中的任务结果，避免父级工具调用被文件持久化阻塞。
    fn store_running_task_outcome_in_memory(
        &self,
        current_session_id: SessionId,
        invocation: &TaskExecutionRequest,
        outcome: &TaskExecutionOutcome,
    ) -> Result<(), String> {
        debug_assert_eq!(outcome.execution_status, TaskExecutionStatus::Running);
        let _ = self.upsert_task_outcome_record(current_session_id, invocation, outcome)?;
        Ok(())
    }

    /// Upserts one task outcome into the in-memory registry and returns the updated record.
    /// 将任务结果写入内存注册表，并返回更新后的记录。
    fn upsert_task_outcome_record(
        &self,
        _current_session_id: SessionId,
        invocation: &TaskExecutionRequest,
        outcome: &TaskExecutionOutcome,
    ) -> Result<TaskSessionRecord, String> {
        let mut sessions = self.sessions.write().map_err(|_| {
            String::from("The task session registry is locked and cannot store the result.")
        })?;
        let now = current_unix_millis_text();
        let status = outcome.execution_status.as_str().to_string();
        let record = sessions.entry(invocation.task_id).or_insert_with(|| {
            build_task_session_record(
                invocation.task_id,
                invocation.public_task_id.clone(),
                invocation.root_session_id,
                invocation.parent_task_id,
                invocation.depth,
                invocation.description.clone(),
                invocation.subagent.clone(),
                invocation.invocation_config.clone(),
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
        let _ = &record.created_at;
        record.controller_config = Some(outcome.controller_config.clone());
        record.snapshot = outcome.snapshot.clone();
        record.last_prompt = invocation.prompt.clone();
        record.last_result_text = outcome.final_output_text.clone();
        record.updated_at = now;
        record.last_status = status;
        record.display_title = invocation.display_title.clone();
        record.invocation_config = invocation.invocation_config.clone();
        if record.description.is_none() {
            record.description = invocation.description.clone();
        }
        if record.subagent.is_none() {
            record.subagent = invocation.subagent.clone();
        }
        Ok(record.clone())
    }

    /// Stores one failed task outcome while preserving the latest snapshot for later resumption.
    /// 保存一条失败的任务结果，同时保留最新快照供后续续跑使用。
    fn mark_task_failure(
        &self,
        _current_session_id: SessionId,
        invocation: &TaskExecutionRequest,
        error: &str,
    ) -> Result<(), String> {
        let record = {
            let mut sessions = self.sessions.write().map_err(|_| {
                String::from(
                    "The task session registry is locked and cannot store the failure state.",
                )
            })?;
            let record = sessions.entry(invocation.task_id).or_insert_with(|| {
                build_task_session_record(
                    invocation.task_id,
                    invocation.public_task_id.clone(),
                    invocation.root_session_id,
                    invocation.parent_task_id,
                    invocation.depth,
                    invocation.description.clone(),
                    invocation.subagent.clone(),
                    invocation.invocation_config.clone(),
                    invocation.display_title.clone(),
                    invocation.controller_config.clone(),
                    invocation.snapshot.clone(),
                    invocation.prompt.clone(),
                    String::from("created"),
                )
            });
            debug_assert_eq!(record.task_id, invocation.task_id);
            let _ = &record.created_at;
            record.last_result_text = error.to_string();
            record.updated_at = current_unix_millis_text();
            record.last_status = String::from("failed");
            record.display_title = invocation.display_title.clone();
            record.invocation_config = invocation.invocation_config.clone();
            if record.controller_config.is_none() {
                record.controller_config = invocation.controller_config.clone();
            }
            record.clone()
        };
        self.persist_task_record_failure(&record, invocation, error);
        Ok(())
    }

    /// Persists one successful or running task state into the file-backed TaskSession store when configured.
    /// 在已配置时将成功或运行中的任务状态持久化到文件态 TaskSession 存储。
    fn persist_task_record_outcome(
        &self,
        record: &TaskSessionRecord,
        invocation: &TaskExecutionRequest,
        outcome: &TaskExecutionOutcome,
    ) {
        let status = task_session_status_from_execution_status(outcome.execution_status);
        if let Err(error) = self.persist_task_record_state(
            record,
            invocation,
            status,
            Some(outcome.final_output_text.as_str()),
            None,
            Some(&outcome.snapshot),
            Some(&outcome.controller_config),
            Some(outcome),
        ) {
            eprintln!(
                "[task-session-store] persist outcome failed for task {}: {error}",
                record.task_id
            );
        }
    }

    /// Persists one failed task state into the file-backed TaskSession store when configured.
    /// 在已配置时将失败任务状态持久化到文件态 TaskSession 存储。
    fn persist_task_record_failure(
        &self,
        record: &TaskSessionRecord,
        invocation: &TaskExecutionRequest,
        error: &str,
    ) {
        if let Err(persist_error) = self.persist_task_record_state(
            record,
            invocation,
            TaskSessionStatus::Failed,
            Some(error),
            Some(error),
            invocation
                .controller_config
                .as_ref()
                .map(|_| &invocation.snapshot),
            invocation.controller_config.as_ref(),
            None,
        ) {
            eprintln!(
                "[task-session-store] persist failure failed for task {}: {persist_error}",
                record.task_id
            );
        }
    }

    /// Persists one task state snapshot and append-only lifecycle event.
    /// 持久化一份任务状态快照并追加生命周期事件。
    fn persist_task_record_state(
        &self,
        record: &TaskSessionRecord,
        invocation: &TaskExecutionRequest,
        status: TaskSessionStatus,
        result_text: Option<&str>,
        error_text: Option<&str>,
        snapshot: Option<&CoreConversationSnapshot>,
        controller_config: Option<&CoreControllerConfig>,
        outcome: Option<&TaskExecutionOutcome>,
    ) -> Result<(), String> {
        let Some(store) = self.task_session_store_for_root(record.origin_session_id)? else {
            return Ok(());
        };
        let now_ms = current_unix_millis_u64();
        let parent = parent_ref_for_task_record(record);
        let resolved_config = outcome
            .map(|outcome| outcome.resolved_config.clone())
            .unwrap_or_else(|| {
                self.resolve_task_session_config_for_record(record, controller_config)
            });
        let task_session_id = SessionNodeId::from(record.task_id);
        let root_session_id = SessionNodeId::from(record.origin_session_id);
        let mut metadata = match store
            .load_task_session(task_session_id)
            .map_err(|error| error.message)?
        {
            Some(metadata) => metadata,
            None => {
                let mut metadata = TaskSessionMetadata::new(
                    root_session_id,
                    task_session_id,
                    parent.clone(),
                    record.public_task_id.clone(),
                    record.display_title.clone(),
                    resolved_config.requested_config.clone(),
                    resolved_config.effective_config.clone(),
                    record.depth as u16,
                    now_ms,
                );
                metadata.description = record.description.clone();
                store
                    .create_task_session(metadata.clone())
                    .map_err(|error| error.message)?;
                metadata
            }
        };
        metadata.display_title = record.display_title.clone();
        metadata.description = record.description.clone();
        metadata.subagent_profile_id = resolved_config.effective_config.subagent_profile_id.clone();
        metadata.inherit_parent_config = resolved_config.effective_config.inherit_parent_config;
        metadata.status = status;
        metadata.runtime_policy = resolved_config.runtime_policy;
        metadata.requested_config = resolved_config.requested_config.clone();
        metadata.effective_config = resolved_config.effective_config.clone();
        metadata.updated_at = now_ms;
        metadata.last_result_summary = result_text.and_then(task_result_summary);
        metadata.last_error = error_text.map(str::to_string);
        store
            .save_task_session(metadata)
            .map_err(|error| error.message)?;

        let parent_sequence = store
            .list_task_invocation_events(parent.clone())
            .map(|events| events.len() as u64)
            .unwrap_or(0);
        store
            .append_task_invocation_event(
                parent,
                TaskInvocationEvent {
                    event_id: format!("task-state-{}-{parent_sequence}", record.task_id),
                    root_session_id,
                    parent: parent_ref_for_task_record(record),
                    task_session_id,
                    sequence: parent_sequence,
                    kind: task_invocation_event_kind_for_status(status),
                    status,
                    prompt: Some(invocation.prompt.clone()),
                    timestamp_ms: now_ms,
                    payload: task_state_event_payload(
                        record,
                        invocation,
                        result_text,
                        error_text,
                        outcome,
                    ),
                },
            )
            .map_err(|error| error.message)?;

        let event_sequence = store
            .load_events(task_session_id)
            .map(|events| events.len() as u64)
            .unwrap_or(0);
        store
            .append_event(SessionEventRecord::new(
                format!("task-node-state-{}-{event_sequence}", record.task_id),
                task_session_id,
                event_sequence,
                task_session_event_kind_for_status(status),
                now_ms,
                task_state_event_payload(record, invocation, result_text, error_text, outcome),
            ))
            .map_err(|error| error.message)?;

        if let (Some(snapshot), Some(controller_config)) = (snapshot, controller_config) {
            let previous_context = store
                .load_context(task_session_id)
                .map_err(|error| error.message)?;
            store
                .save_context(SessionContextDocument {
                    schema_version: 1,
                    node_id: task_session_id,
                    provider_id: Some(controller_config.provider_id.clone()),
                    model_id: Some(controller_config.model_id.clone()),
                    reasoning_effort: None,
                    system_prompt_layers: vec![controller_config.system_prompt.clone()],
                    conversation_state: serde_json::to_value(
                        core_snapshot_to_session_conversation_state(snapshot),
                    )
                    .unwrap_or(Value::Null),
                    compaction_anchor: previous_context
                        .as_ref()
                        .and_then(|context| context.compaction_anchor.clone()),
                    context_budget_fingerprint: previous_context
                        .as_ref()
                        .and_then(|context| context.context_budget_fingerprint.clone()),
                    tool_catalog_snapshot: Value::Null,
                    provider_request_preview: None,
                })
                .map_err(|error| error.message)?;
        }

        store
            .save_runtime_checkpoint(SessionRuntimeCheckpoint {
                node_id: task_session_id,
                status: status_name_for_task_session_status(status).to_string(),
                last_sequence: event_sequence,
                payload: json!({
                    "task_id": record.public_task_id,
                    "display_title": record.display_title,
                    "updated_at": now_ms,
                }),
            })
            .map_err(|error| error.message)?;

        Ok(())
    }

    /// Builds one file-backed TaskSession store for the given root session when a host configured it.
    /// 当宿主已配置根目录时，为给定根会话构建一份文件态 TaskSession 存储。
    fn task_session_store_for_root(
        &self,
        root_session_id: SessionId,
    ) -> Result<Option<FileTaskSessionStore>, String> {
        let root = self
            .task_session_store_roots
            .read()
            .map_err(|_| {
                String::from("The task-session store root registry is locked and cannot read.")
            })?
            .get(&root_session_id)
            .cloned();
        Ok(root.map(FileTaskSessionStore::new))
    }

    /// Finds one file-backed TaskSession metadata record by direct parent and public task id.
    /// 按直接父级与公开任务标识查找一条文件态 TaskSession 元数据。
    fn file_backed_task_metadata_for_public_id(
        &self,
        root_session_id: SessionId,
        parent: ParentSessionRef,
        public_task_id: &str,
    ) -> Result<Option<TaskSessionMetadata>, String> {
        let Some(store) = self.task_session_store_for_root(root_session_id)? else {
            return Ok(None);
        };
        store
            .find_task_session_by_parent_and_public_id(parent, public_task_id)
            .map_err(|error| error.message)
    }

    /// Loads the persisted current context for one file-backed TaskSession.
    /// 加载某个文件态 TaskSession 的持久化当前上下文。
    fn load_file_backed_task_context(
        &self,
        root_session_id: SessionNodeId,
        task_session_id: SessionNodeId,
    ) -> Result<Option<SessionContextDocument>, String> {
        let root_session_id = Uuid::from(root_session_id);
        let Some(store) = self.task_session_store_for_root(root_session_id)? else {
            return Ok(None);
        };
        store
            .load_context(task_session_id)
            .map_err(|error| error.message)
    }

    /// Resolves one TaskSession configuration for a new child execution.
    /// 为一次新的子代理执行解析 TaskSession 配置。
    fn resolve_task_session_config_for_execution(
        &self,
        parent_controller_config: &CoreControllerConfig,
        invocation_config: &TaskInvocationConfig,
    ) -> Result<TaskSessionResolvedConfig, String> {
        let parent_config = Some(core_controller_config_to_effective_config(
            parent_controller_config,
        ));
        let profile = self.subagent_profile_for_invocation(invocation_config)?;
        Ok(TaskSessionConfigResolver::resolve(
            TaskSessionConfigResolutionInput {
                global_default_config: SessionEffectiveConfig::default(),
                parent_config,
                profile,
                invocation: invocation_config.clone(),
            },
        ))
    }

    /// Resolves the persisted TaskSession configuration snapshot for one runtime record.
    /// 为一条运行时记录解析要持久化的 TaskSession 配置快照。
    fn resolve_task_session_config_for_record(
        &self,
        record: &TaskSessionRecord,
        controller_config: Option<&CoreControllerConfig>,
    ) -> TaskSessionResolvedConfig {
        let parent_config = controller_config.map(core_controller_config_to_effective_config);
        let profile = self
            .subagent_profile_for_invocation(&record.invocation_config)
            .ok()
            .flatten();
        TaskSessionConfigResolver::resolve(TaskSessionConfigResolutionInput {
            global_default_config: SessionEffectiveConfig::default(),
            parent_config,
            profile,
            invocation: record.invocation_config.clone(),
        })
    }

    /// Loads the requested subagent profile from the runtime snapshot.
    /// 从运行时快照加载请求的子代理配置。
    fn subagent_profile_for_invocation(
        &self,
        invocation_config: &TaskInvocationConfig,
    ) -> Result<Option<SubagentProfile>, String> {
        let Some(profile_id) = invocation_config
            .subagent_profile_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            return Ok(None);
        };
        let profiles = self.subagent_profiles.read().map_err(|_| {
            String::from("The subagent profile registry is locked and cannot read.")
        })?;
        profiles.get(profile_id).cloned().map(Some).ok_or_else(|| {
            format!(
                "Subagent profile `{profile_id}` was requested but is not available in the runtime profile catalog."
            )
        })
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
                "Create, resume, or inspect a recoverable subtask session. A running task owns its delegated work until it reports back automatically. Always provide `description` and `subagent` so the subtask keeps a stable title and identity. When a task is already running, reuse the same `task_id` to inspect that exact task; do not start a duplicate task, do not rewrite the task prompt as new work, and do not replace unfinished subagent work with main-thread or non-task tools.",
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
                "description": "Optional stable task identifier. Provide it to continue, inspect, or resume the same subtask. For a running task, reuse this exact id only for status inspection; never omit it to recreate a duplicate or replace that work in the parent thread."
            },
            "subagent": {
                "type": "string",
                "description": "Subagent name or role label. This should usually be provided so the UI can display a stable worker identity and responsibility."
            },
            "subagent_profile_id": {
                "type": "string",
                "description": "Optional named subagent profile id. Use this when a configured specialized subagent should be selected."
            },
            "profile": {
                "type": "string",
                "description": "Short alias for subagent_profile_id."
            },
            "inherit_parent_config": {
                "type": "boolean",
                "description": "Optional override. True inherits the parent model, prompt, tools, and skills; false uses only default/profile/invocation configuration."
            },
            "system_prompt": {
                "type": "string",
                "description": "Optional replacement system prompt for this subagent session."
            },
            "system_prompt_append": {
                "type": "string",
                "description": "Optional extra system prompt instructions appended to this subagent session."
            },
            "provider": {
                "type": "string",
                "description": "Optional provider id override. Provider switching is only honored when the host runtime can route that provider."
            },
            "model": {
                "type": "string",
                "description": "Optional model id override for this subagent."
            },
            "reasoning_effort": {
                "type": "string",
                "description": "Optional reasoning effort override for this subagent."
            },
            "execution_mode": {
                "type": "string",
                "description": "Optional execution mode override such as Build or Plan."
            },
            "provider_model": {
                "description": "Optional provider/model override. Use an object with provider_id, model_id, reasoning_effort, and execution_mode, or a provider/model string.",
                "anyOf": [
                    { "type": "string" },
                    { "type": "object", "additionalProperties": true }
                ]
            },
            "tools": {
                "description": "Optional tool exposure override. Use an array of allowed tool ids, or an object with allowed/enabled and disabled lists.",
                "anyOf": [
                    { "type": "string" },
                    { "type": "array", "items": { "type": "string" } },
                    { "type": "object", "additionalProperties": true }
                ]
            },
            "ai_capabilities": {
                "description": "Optional AI capability override. Use an array of allowed capability ids, or an object with allowed/enabled and disabled lists.",
                "anyOf": [
                    { "type": "string" },
                    { "type": "array", "items": { "type": "string" } },
                    { "type": "object", "additionalProperties": true }
                ]
            },
            "skills": {
                "description": "Optional prompt skill override. Use an array of enabled skill ids, or an object with enabled and disabled lists.",
                "anyOf": [
                    { "type": "string" },
                    { "type": "array", "items": { "type": "string" } },
                    { "type": "object", "additionalProperties": true }
                ]
            },
            "prompt_skills": {
                "description": "Alias of skills for prompt-skill override.",
                "anyOf": [
                    { "type": "string" },
                    { "type": "array", "items": { "type": "string" } },
                    { "type": "object", "additionalProperties": true }
                ]
            },
            "luaskills": {
                "description": "Optional LuaSkills override payload persisted in the TaskSession config snapshot.",
                "type": "object",
                "additionalProperties": true
            },
            "mcp": {
                "description": "Optional MCP override payload persisted in the TaskSession config snapshot.",
                "type": "object",
                "additionalProperties": true
            },
            "context_compaction": {
                "description": "Optional context compaction override with context_budget, max_provider_requests, or auto_compaction_enabled.",
                "type": "object",
                "additionalProperties": true
            },
            "runtime_policy": {
                "description": "Optional task-session runtime override. Prefer camelCase keys such as hooksEnabled, vmmEnabled, memoryRecallEnabled, and memoryWritebackEnabled; snake_case aliases are also accepted. searchProjectionEnabled and databaseProjectionEnabled are recorded for audit but remain disabled for TaskSession file-only isolation.",
                "type": "object",
                "additionalProperties": true
            },
            "extra": {
                "description": "Optional caller-owned JSON payload persisted with this task invocation.",
                "type": "object",
                "additionalProperties": true
            }
        },
        "required": ["prompt"],
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
    let (root_session_id, direct_parent, current_depth) = {
        let sessions = task_runtime.sessions.read().map_err(|_| {
            String::from("The task session registry is locked and cannot inspect existing tasks.")
        })?;
        let current_record = sessions.get(&current_session_id);
        let root_session_id = current_record
            .map(|record| record.origin_session_id)
            .unwrap_or(current_session_id);
        let direct_parent = current_record
            .map(|_| ParentSessionRef::TaskSession {
                root_session_id: SessionNodeId::from(root_session_id),
                parent_task_session_id: SessionNodeId::from(current_session_id),
            })
            .unwrap_or(ParentSessionRef::MainSession {
                root_session_id: SessionNodeId::from(root_session_id),
            });
        let current_depth = current_record.map(|record| record.depth).unwrap_or(0);
        (root_session_id, direct_parent, current_depth)
    };

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
                record.public_task_id == public_task_id
                    && task_record_belongs_to_direct_parent(record, current_session_id)
            })
            .cloned();
        if let Some(record) = resumed_record {
            return Ok(ResolvedTaskSessionIdentifier {
                task_id: record.task_id,
                public_task_id,
                is_resumed: true,
                file_metadata: None,
            });
        }

        if let Some(metadata) = task_runtime.file_backed_task_metadata_for_public_id(
            root_session_id,
            direct_parent.clone(),
            &public_task_id,
        )? {
            return Ok(ResolvedTaskSessionIdentifier {
                task_id: Uuid::from(metadata.task_session_id),
                public_task_id,
                is_resumed: true,
                file_metadata: Some(metadata),
            });
        }

        return Ok(ResolvedTaskSessionIdentifier {
            task_id: Uuid::new_v4(),
            public_task_id,
            is_resumed: false,
            file_metadata: None,
        });
    }

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
        file_metadata: None,
    })
}

/// Returns whether a task record belongs to the currently invoking direct parent.
/// 返回一条任务记录是否属于当前发起调用的直接父级。
fn task_record_belongs_to_direct_parent(
    record: &TaskSessionRecord,
    current_session_id: SessionId,
) -> bool {
    record
        .parent_task_id
        .map(|parent_task_id| parent_task_id == current_session_id)
        .unwrap_or(record.origin_session_id == current_session_id)
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

/// Builds the task invocation config shared with the TaskSession layer.
/// 构建与 TaskSession 层共享的任务调用配置。
fn task_invocation_config_from_args(args: &TaskToolInvocationArgs) -> TaskInvocationConfig {
    let profile_id = args
        .subagent_profile_id
        .as_deref()
        .or(args.profile.as_deref());
    let mut config = profile_id
        .map(|profile_id| TaskInvocationConfig::with_profile(profile_id, args.prompt.clone()))
        .unwrap_or_else(|| TaskInvocationConfig::main(args.prompt.clone()));
    config.inherit_parent_config = args.inherit_parent_config;
    config.system_prompt = normalize_optional_task_label(args.system_prompt.as_deref());
    config.system_prompt_append =
        normalize_optional_task_label(args.system_prompt_append.as_deref());
    config.provider_model = provider_model_snapshot_from_args(args);
    config.tool_exposure = tool_exposure_snapshot_from_args(args);
    config.prompt_skills = prompt_skill_snapshot_from_args(args);
    config.context_compaction =
        context_compaction_request_from_value(args.context_compaction.as_ref());
    config.runtime_policy = runtime_policy_request_from_value(args.runtime_policy.as_ref());
    config.extra = args.extra.clone().unwrap_or(Value::Null);
    config
}

/// Builds the provider/model override snapshot from task tool arguments.
/// 从 task 工具参数构建供应商与模型覆盖快照。
fn provider_model_snapshot_from_args(args: &TaskToolInvocationArgs) -> ProviderModelSnapshot {
    let mut snapshot = ProviderModelSnapshot::default();
    if let Some(provider_model) = args.provider_model.as_ref() {
        apply_provider_model_value(&mut snapshot, provider_model);
    }
    if let Some(provider) = normalize_optional_task_label(args.provider.as_deref()) {
        snapshot.provider_id = Some(provider);
    }
    if let Some(model) = normalize_optional_task_label(args.model.as_deref()) {
        snapshot.model_id = Some(model);
    }
    if let Some(reasoning_effort) = normalize_optional_task_label(args.reasoning_effort.as_deref())
    {
        snapshot.reasoning_effort = Some(reasoning_effort);
    }
    if let Some(execution_mode) = normalize_optional_task_label(args.execution_mode.as_deref()) {
        snapshot.execution_mode = Some(execution_mode);
    }
    snapshot
}

/// Applies a flexible provider/model JSON value into a provider/model snapshot.
/// 将灵活的供应商与模型 JSON 值应用到供应商与模型快照。
fn apply_provider_model_value(snapshot: &mut ProviderModelSnapshot, value: &Value) {
    match value {
        Value::String(text) => {
            let text = text.trim();
            if text.is_empty() {
                return;
            }
            if let Some((provider, model)) = text.split_once('/') {
                snapshot.provider_id = normalize_optional_task_label(Some(provider));
                snapshot.model_id = normalize_optional_task_label(Some(model));
            } else {
                snapshot.model_id = Some(text.to_string());
            }
        }
        Value::Object(map) => {
            snapshot.provider_id = string_field(map, &["provider_id", "provider"])
                .or_else(|| snapshot.provider_id.clone());
            snapshot.model_id =
                string_field(map, &["model_id", "model"]).or_else(|| snapshot.model_id.clone());
            snapshot.reasoning_effort =
                string_field(map, &["reasoning_effort", "reasoning", "effort"])
                    .or_else(|| snapshot.reasoning_effort.clone());
            snapshot.execution_mode = string_field(map, &["execution_mode", "mode"])
                .or_else(|| snapshot.execution_mode.clone());
        }
        _ => {}
    }
}

/// Builds the tool exposure override snapshot from task tool arguments.
/// 从 task 工具参数构建工具可见性覆盖快照。
fn tool_exposure_snapshot_from_args(args: &TaskToolInvocationArgs) -> ToolExposureSnapshot {
    let mut snapshot = ToolExposureSnapshot::default();
    if let Some(tools) = args.tools.as_ref() {
        apply_tool_selector_value(
            tools,
            &mut snapshot.allowed_tools,
            &mut snapshot.disabled_tools,
        );
    }
    if let Some(ai_capabilities) = args.ai_capabilities.as_ref() {
        apply_tool_selector_value(
            ai_capabilities,
            &mut snapshot.allowed_ai_capabilities,
            &mut snapshot.disabled_ai_capabilities,
        );
    }
    if let Some(luaskills) = args.luaskills.as_ref() {
        snapshot.luaskills_overrides = luaskills.clone();
    }
    if let Some(mcp) = args.mcp.as_ref() {
        snapshot.mcp_server_overrides = mcp.clone();
    }
    dedupe_preserving_order(&mut snapshot.allowed_tools);
    dedupe_preserving_order(&mut snapshot.disabled_tools);
    dedupe_preserving_order(&mut snapshot.allowed_ai_capabilities);
    dedupe_preserving_order(&mut snapshot.disabled_ai_capabilities);
    snapshot
}

/// Builds the prompt-skill override snapshot from task tool arguments.
/// 从 task 工具参数构建提示词技能覆盖快照。
fn prompt_skill_snapshot_from_args(args: &TaskToolInvocationArgs) -> PromptSkillSnapshot {
    let mut snapshot = PromptSkillSnapshot::default();
    if let Some(skills) = args.skills.as_ref() {
        apply_tool_selector_value(skills, &mut snapshot.enabled, &mut snapshot.disabled);
    }
    if let Some(prompt_skills) = args.prompt_skills.as_ref() {
        apply_tool_selector_value(prompt_skills, &mut snapshot.enabled, &mut snapshot.disabled);
    }
    dedupe_preserving_order(&mut snapshot.enabled);
    dedupe_preserving_order(&mut snapshot.disabled);
    snapshot
}

/// Builds the context-compaction override request from a flexible JSON payload.
/// 从灵活 JSON 载荷构建上下文压缩覆盖请求。
fn context_compaction_request_from_value(value: Option<&Value>) -> ContextCompactionPolicyRequest {
    let mut request = ContextCompactionPolicyRequest::default();
    let Some(Value::Object(map)) = value else {
        return request;
    };
    request.context_budget = integer_field(map, &["context_budget", "budget"]);
    request.max_provider_requests =
        integer_field(map, &["max_provider_requests", "max_requests"]).map(|value| value as usize);
    request.auto_compaction_enabled = bool_field(
        map,
        &["auto_compaction_enabled", "auto_compaction", "enabled"],
    );
    request
}

/// Builds the runtime-policy override request from a flexible JSON payload.
/// 从灵活 JSON 载荷构建运行时策略覆盖请求。
fn runtime_policy_request_from_value(value: Option<&Value>) -> ExecutionRuntimePolicyRequest {
    let mut request = ExecutionRuntimePolicyRequest::default();
    let Some(Value::Object(map)) = value else {
        return request;
    };
    request.hooks_enabled = bool_field(map, &["hooksEnabled", "hooks_enabled", "hooks"]);
    request.vmm_enabled = bool_field(map, &["vmmEnabled", "vmm_enabled", "vmm"]);
    request.memory_recall_enabled = bool_field(
        map,
        &[
            "memoryRecallEnabled",
            "memory_recall_enabled",
            "memory_recall",
            "recall",
        ],
    );
    request.memory_writeback_enabled = bool_field(
        map,
        &[
            "memoryWritebackEnabled",
            "memory_writeback_enabled",
            "memory_writeback",
            "writeback",
        ],
    );
    request.search_projection_enabled = bool_field(
        map,
        &[
            "searchProjectionEnabled",
            "search_projection_enabled",
            "search_projection",
        ],
    );
    request.database_projection_enabled = bool_field(
        map,
        &[
            "databaseProjectionEnabled",
            "database_projection_enabled",
            "database_projection",
            "db_projection",
        ],
    );
    request
}

/// Applies a flexible selector value into allow and deny lists.
/// 将灵活选择器值应用到允许与禁用列表。
fn apply_tool_selector_value(value: &Value, allowed: &mut Vec<String>, disabled: &mut Vec<String>) {
    match value {
        Value::String(_) | Value::Array(_) => append_string_values(allowed, value),
        Value::Object(map) => {
            for key in [
                "allow",
                "allowed",
                "enabled",
                "include",
                "tools",
                "native",
                "native_tools",
                "mcp_tools",
                "capabilities",
            ] {
                if let Some(value) = map.get(key) {
                    append_string_values(allowed, value);
                }
            }
            for key in ["deny", "denied", "disabled", "disable", "exclude"] {
                if let Some(value) = map.get(key) {
                    append_string_values(disabled, value);
                }
            }
        }
        _ => {}
    }
}

/// Appends string or string-array values into a target list after trimming empty entries.
/// 将字符串或字符串数组裁剪后追加到目标列表，并跳过空值。
fn append_string_values(target: &mut Vec<String>, value: &Value) {
    match value {
        Value::String(text) => {
            if let Some(text) = normalize_optional_task_label(Some(text)) {
                target.push(text);
            }
        }
        Value::Array(items) => {
            for item in items {
                append_string_values(target, item);
            }
        }
        _ => {}
    }
}

/// Reads the first non-empty string field from a JSON object.
/// 从 JSON 对象读取第一个非空字符串字段。
fn string_field(map: &serde_json::Map<String, Value>, keys: &[&str]) -> Option<String> {
    keys.iter().find_map(|key| {
        map.get(*key)
            .and_then(Value::as_str)
            .and_then(|value| normalize_optional_task_label(Some(value)))
    })
}

/// Reads the first unsigned integer field from a JSON object.
/// 从 JSON 对象读取第一个无符号整数字段。
fn integer_field(map: &serde_json::Map<String, Value>, keys: &[&str]) -> Option<u64> {
    keys.iter()
        .find_map(|key| map.get(*key).and_then(Value::as_u64))
}

/// Reads the first boolean field from a JSON object.
/// 从 JSON 对象读取第一个布尔字段。
fn bool_field(map: &serde_json::Map<String, Value>, keys: &[&str]) -> Option<bool> {
    keys.iter()
        .find_map(|key| map.get(*key).and_then(Value::as_bool))
}

/// Deduplicates one string list while preserving the original order.
/// 在保留原始顺序的同时为字符串列表去重。
fn dedupe_preserving_order(values: &mut Vec<String>) {
    let mut seen = BTreeSet::new();
    values.retain(|value| seen.insert(value.clone()));
}

/// Merges an incoming resume invocation without clearing stored explicit task configuration.
/// 合并续跑传入配置，同时避免清除已存储的显式任务配置。
fn merge_task_invocation_config_for_resume(
    mut stored: TaskInvocationConfig,
    incoming: TaskInvocationConfig,
) -> TaskInvocationConfig {
    stored.prompt = incoming.prompt;
    if incoming.subagent_profile_id.is_some() {
        stored.subagent_profile_id = incoming.subagent_profile_id;
    }
    if incoming.inherit_parent_config.is_some() {
        stored.inherit_parent_config = incoming.inherit_parent_config;
    }
    if incoming.system_prompt.is_some() {
        stored.system_prompt = incoming.system_prompt;
    }
    if incoming.system_prompt_append.is_some() {
        stored.system_prompt_append = incoming.system_prompt_append;
    }
    if provider_model_has_override(&incoming.provider_model) {
        stored.provider_model = incoming.provider_model;
    }
    if tool_exposure_has_override(&incoming.tool_exposure) {
        stored.tool_exposure = incoming.tool_exposure;
    }
    if prompt_skills_have_override(&incoming.prompt_skills) {
        stored.prompt_skills = incoming.prompt_skills;
    }
    if context_compaction_has_override(&incoming.context_compaction) {
        stored.context_compaction = incoming.context_compaction;
    }
    if incoming.runtime_policy.has_override() {
        stored.runtime_policy = incoming.runtime_policy;
    }
    if !incoming.extra.is_null() {
        stored.extra = incoming.extra;
    }
    stored
}

/// Returns whether a provider/model snapshot carries any explicit override.
/// 返回供应商与模型快照是否携带显式覆盖。
fn provider_model_has_override(snapshot: &ProviderModelSnapshot) -> bool {
    snapshot.provider_id.is_some()
        || snapshot.model_id.is_some()
        || snapshot.reasoning_effort.is_some()
        || snapshot.execution_mode.is_some()
}

/// Returns whether a tool exposure snapshot carries any explicit override.
/// 返回工具可见性快照是否携带显式覆盖。
fn tool_exposure_has_override(snapshot: &ToolExposureSnapshot) -> bool {
    !snapshot.allowed_tools.is_empty()
        || !snapshot.disabled_tools.is_empty()
        || !snapshot.allowed_ai_capabilities.is_empty()
        || !snapshot.disabled_ai_capabilities.is_empty()
        || !snapshot.luaskills_overrides.is_null()
        || !snapshot.mcp_server_overrides.is_null()
}

/// Returns whether a prompt-skill snapshot carries any explicit override.
/// 返回提示词技能快照是否携带显式覆盖。
fn prompt_skills_have_override(snapshot: &PromptSkillSnapshot) -> bool {
    !snapshot.enabled.is_empty() || !snapshot.disabled.is_empty()
}

/// Returns whether a context-compaction request carries any explicit override.
/// 返回上下文压缩请求是否携带显式覆盖。
fn context_compaction_has_override(request: &ContextCompactionPolicyRequest) -> bool {
    request.context_budget.is_some()
        || request.max_provider_requests.is_some()
        || request.auto_compaction_enabled.is_some()
}

/// Returns whether an incoming invocation explicitly changes controller-owned runtime shape.
/// 返回传入调用是否显式变更控制器拥有的运行时形态。
fn task_invocation_changes_controller_config(config: &TaskInvocationConfig) -> bool {
    config.subagent_profile_id.is_some()
        || config.inherit_parent_config.is_some()
        || config.system_prompt.is_some()
        || config.system_prompt_append.is_some()
        || provider_model_has_override(&config.provider_model)
        || config.runtime_policy.has_override()
        || !config.extra.is_null()
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
        file_metadata,
    } = resolved_identifier;
    let normalized_description = normalize_optional_task_label(args.description.as_deref());
    let normalized_subagent = normalize_optional_task_label(args.subagent.as_deref())
        .or_else(|| normalize_optional_task_label(args.subagent_profile_id.as_deref()))
        .or_else(|| normalize_optional_task_label(args.profile.as_deref()));
    let invocation_config = task_invocation_config_from_args(&args);
    let prompt = args.prompt;
    let (
        parent_task_id,
        root_session_id,
        depth,
        snapshot,
        controller_config,
        description,
        subagent,
        invocation_config,
        display_title,
    ) = {
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
                record.invocation_config = merge_task_invocation_config_for_resume(
                    record.invocation_config.clone(),
                    invocation_config.clone(),
                );
                if task_invocation_changes_controller_config(&invocation_config) {
                    record.controller_config = None;
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
                record.origin_session_id,
                record.depth,
                record.snapshot.clone(),
                record.controller_config.clone(),
                description,
                subagent,
                record.invocation_config.clone(),
                display_title,
            )
        } else if is_resumed {
            let metadata = file_metadata.ok_or_else(|| {
                format!(
                    "Task `{public_task_id}` was marked as resumable but no in-memory or file-backed TaskSession record was available."
                )
            })?;
            let context = task_runtime.load_file_backed_task_context(
                metadata.root_session_id,
                metadata.task_session_id,
            )?;
            let mut record = task_session_record_from_file_metadata(
                &metadata,
                context.as_ref(),
                prompt.clone(),
                invocation_config.clone(),
                normalized_description.clone(),
                normalized_subagent.clone(),
            )?;
            record.updated_at = current_unix_millis_text();
            if task_invocation_changes_controller_config(&invocation_config) {
                record.controller_config = None;
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
            record.description = description.clone();
            record.subagent = subagent.clone();
            record.last_prompt = prompt.clone();
            let parent_task_id = record.parent_task_id;
            let root_session_id = record.origin_session_id;
            let depth = record.depth;
            let snapshot = record.snapshot.clone();
            let controller_config = record.controller_config.clone();
            let invocation_config = record.invocation_config.clone();
            sessions.insert(task_id, record);
            (
                parent_task_id,
                root_session_id,
                depth,
                snapshot,
                controller_config,
                description,
                subagent,
                invocation_config,
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
            let root_session_id = sessions
                .get(&current_session_id)
                .map(|record| record.origin_session_id)
                .unwrap_or(current_session_id);
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
                    origin_session_id: root_session_id,
                    parent_task_id,
                    depth,
                    description: normalized_description.clone(),
                    subagent: normalized_subagent.clone(),
                    invocation_config: invocation_config.clone(),
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
                root_session_id,
                depth,
                CoreConversationSnapshot::default(),
                None,
                normalized_description.clone(),
                normalized_subagent.clone(),
                invocation_config.clone(),
                display_title,
            )
        }
    };

    Ok(TaskExecutionRequest {
        current_session_id,
        root_session_id,
        task_id,
        public_task_id,
        prompt,
        description,
        subagent,
        invocation_config,
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
    invocation_config: TaskInvocationConfig,
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
        invocation_config,
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

/// Rebuilds one in-memory task record from file-backed TaskSession metadata and current context.
/// 从文件态 TaskSession 元数据与当前上下文重建一条内存任务记录。
fn task_session_record_from_file_metadata(
    metadata: &TaskSessionMetadata,
    context: Option<&SessionContextDocument>,
    prompt: String,
    incoming_invocation: TaskInvocationConfig,
    description_override: Option<String>,
    subagent_override: Option<String>,
) -> Result<TaskSessionRecord, String> {
    let stored_invocation =
        task_invocation_config_from_requested_config(&metadata.requested_config, prompt.clone());
    let invocation_config =
        merge_task_invocation_config_for_resume(stored_invocation, incoming_invocation);
    let snapshot = context
        .map(task_conversation_snapshot_from_context)
        .transpose()?
        .unwrap_or_default();
    let controller_config = context.and_then(|context| {
        task_controller_config_from_context(context, &metadata.effective_config)
    });
    let parent_task_id = match metadata.parent {
        ParentSessionRef::TaskSession {
            parent_task_session_id,
            ..
        } => Some(Uuid::from(parent_task_session_id)),
        ParentSessionRef::MainSession { .. } | ParentSessionRef::None => None,
    };
    let subagent = subagent_override
        .or_else(|| metadata.subagent_profile_id.clone())
        .or_else(|| invocation_config.subagent_profile_id.clone());

    Ok(TaskSessionRecord {
        task_id: Uuid::from(metadata.task_session_id),
        public_task_id: metadata.public_task_id.clone(),
        origin_session_id: Uuid::from(metadata.root_session_id),
        parent_task_id,
        depth: metadata.depth as usize,
        description: description_override.or_else(|| metadata.description.clone()),
        subagent,
        invocation_config,
        display_title: metadata.display_title.clone(),
        controller_config,
        snapshot,
        last_prompt: prompt,
        last_result_text: metadata.last_result_summary.clone().unwrap_or_default(),
        created_at: metadata.created_at.to_string(),
        updated_at: metadata.updated_at.to_string(),
        last_status: status_name_for_task_session_status(metadata.status).to_string(),
    })
}

/// Recreates one task invocation config from the persisted requested configuration snapshot.
/// 从已持久化的请求配置快照重建一份任务调用配置。
fn task_invocation_config_from_requested_config(
    requested_config: &SessionRequestedConfig,
    prompt: String,
) -> TaskInvocationConfig {
    TaskInvocationConfig {
        prompt,
        subagent_profile_id: requested_config.subagent_profile_id.clone(),
        inherit_parent_config: Some(requested_config.inherit_parent_config),
        system_prompt: requested_config.system_prompt.clone(),
        system_prompt_append: requested_config.system_prompt_append.clone(),
        provider_model: requested_config.provider_model.clone(),
        tool_exposure: requested_config.tool_exposure.clone(),
        prompt_skills: requested_config.prompt_skills.clone(),
        context_compaction: requested_config.context_compaction.clone(),
        runtime_policy: requested_config.runtime_policy,
        extra: requested_config.extra.clone(),
    }
}

/// Converts a persisted task context document into a core conversation snapshot.
/// 将已持久化的任务上下文文档转换为 core 会话快照。
fn task_conversation_snapshot_from_context(
    context: &SessionContextDocument,
) -> Result<CoreConversationSnapshot, String> {
    let conversation_state: SessionConversationState = serde_json::from_value(
        context.conversation_state.clone(),
    )
    .map_err(|error| {
        format!(
            "TaskSession `{}` has an unreadable context/current.json conversation state: {error}",
            context.node_id
        )
    })?;
    Ok(build_core_conversation_snapshot(&conversation_state))
}

/// Rebuilds a minimal controller config from persisted task context and effective config.
/// 从已持久化的任务上下文与有效配置重建最小控制器配置。
fn task_controller_config_from_context(
    context: &SessionContextDocument,
    effective_config: &SessionEffectiveConfig,
) -> Option<CoreControllerConfig> {
    let provider_id = context
        .provider_id
        .clone()
        .or_else(|| effective_config.provider_model.provider_id.clone())?;
    let model_id = context
        .model_id
        .clone()
        .or_else(|| effective_config.provider_model.model_id.clone())?;
    let system_prompt_layers = if context.system_prompt_layers.is_empty() {
        effective_config.system_prompt_layers.clone()
    } else {
        context.system_prompt_layers.clone()
    };
    let system_prompt = system_prompt_layers.join("\n\n");
    let temperature = effective_config
        .extra
        .get("temperature")
        .and_then(Value::as_f64)
        .map(|value| value.clamp(0.0, 2.0));
    let max_turns = effective_config
        .extra
        .get("max_turns")
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .unwrap_or(usize::MAX - 1);

    Some(CoreControllerConfig {
        provider_id,
        model_id,
        prompt_layers: CoreProviderPromptLayers::from_system_prompt(system_prompt.clone()),
        system_prompt,
        temperature,
        max_turns,
        additional_params: None,
        context_injection: None,
    })
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
    let payload = build_task_host_payload(
        invocation,
        execution_status,
        outcome.provider_turns_executed,
        outcome.tool_calls_processed,
        outcome.final_assistant_message_id.as_deref(),
        outcome.final_output_text.as_str(),
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

/// Returns one Unix-millis timestamp as an integer for file-backed TaskSession records.
/// 为文件态 TaskSession 记录返回整数形式的 Unix 毫秒时间戳。
fn current_unix_millis_u64() -> u64 {
    current_unix_millis_text().parse::<u64>().unwrap_or(0)
}

/// Returns a UTC timestamp for task-runtime diagnostic traces.
/// 返回任务运行时诊断日志使用的 UTC 时间戳。
fn current_task_runtime_trace_timestamp() -> String {
    let now = OffsetDateTime::now_utc();
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}.{:03}Z",
        now.year(),
        u8::from(now.month()),
        now.day(),
        now.hour(),
        now.minute(),
        now.second(),
        now.millisecond()
    )
}

/// Returns whether task-runtime tracing should be emitted with the WebUI trace switch.
/// 返回是否应随 WebUI 跟踪开关输出任务运行时日志。
fn task_runtime_trace_enabled() -> bool {
    std::env::var("VULCAN_WEBUI_TRACE")
        .ok()
        .map(|value| {
            let normalized = value.trim().to_ascii_lowercase();
            matches!(normalized.as_str(), "1" | "true" | "yes" | "on")
        })
        .unwrap_or(false)
}

/// Emits one task-runtime timing trace line when WebUI tracing is enabled.
/// 当 WebUI 跟踪启用时输出一条任务运行时耗时日志。
fn task_runtime_trace(stage: &str, detail: impl AsRef<str>) {
    if !task_runtime_trace_enabled() {
        return;
    }
    eprintln!(
        "[task-runtime][webui-trace] timestamp={} unix_ms={} stage={} {}",
        current_task_runtime_trace_timestamp(),
        current_unix_millis_text(),
        stage,
        detail.as_ref()
    );
}

/// Converts a task execution status into the file-backed TaskSession status.
/// 将任务执行状态转换为文件态 TaskSession 状态。
fn task_session_status_from_execution_status(status: TaskExecutionStatus) -> TaskSessionStatus {
    match status {
        TaskExecutionStatus::Running => TaskSessionStatus::Running,
        TaskExecutionStatus::Completed => TaskSessionStatus::Completed,
        TaskExecutionStatus::Stopped => TaskSessionStatus::Stopped,
    }
}

/// Returns the stable lowercase name for one TaskSession status.
/// 返回某个 TaskSession 状态的稳定小写名称。
fn status_name_for_task_session_status(status: TaskSessionStatus) -> &'static str {
    match status {
        TaskSessionStatus::Created => "created",
        TaskSessionStatus::Running => "running",
        TaskSessionStatus::WaitingChildren => "waiting_children",
        TaskSessionStatus::WaitingFollowup => "waiting_followup",
        TaskSessionStatus::Completed => "completed",
        TaskSessionStatus::Failed => "failed",
        TaskSessionStatus::Stopped => "stopped",
        TaskSessionStatus::Compacting => "compacting",
        TaskSessionStatus::Archived => "archived",
    }
}

/// Converts a task status into a parent-side invocation event kind.
/// 将任务状态转换为父级侧调用事件类型。
fn task_invocation_event_kind_for_status(status: TaskSessionStatus) -> TaskInvocationEventKind {
    match status {
        TaskSessionStatus::Running => TaskInvocationEventKind::TaskStarted,
        TaskSessionStatus::Completed => TaskInvocationEventKind::TaskCompleted,
        TaskSessionStatus::Failed => TaskInvocationEventKind::TaskFailed,
        TaskSessionStatus::Stopped => TaskInvocationEventKind::TaskStopped,
        TaskSessionStatus::Compacting => TaskInvocationEventKind::TaskCompacted,
        TaskSessionStatus::Created
        | TaskSessionStatus::WaitingChildren
        | TaskSessionStatus::WaitingFollowup
        | TaskSessionStatus::Archived => TaskInvocationEventKind::TaskCheckpoint,
    }
}

/// Converts a task status into a node-local event kind.
/// 将任务状态转换为节点本地事件类型。
fn task_session_event_kind_for_status(status: TaskSessionStatus) -> SessionEventKind {
    match status {
        TaskSessionStatus::Completed | TaskSessionStatus::Stopped => SessionEventKind::TaskFeedback,
        TaskSessionStatus::Failed => SessionEventKind::RuntimeMarker,
        _ => SessionEventKind::RuntimeMarker,
    }
}

/// Builds the direct parent reference for one task record.
/// 为一条任务记录构建直接父级引用。
fn parent_ref_for_task_record(record: &TaskSessionRecord) -> ParentSessionRef {
    let root_session_id = SessionNodeId::from(record.origin_session_id);
    record
        .parent_task_id
        .map(|parent_task_session_id| ParentSessionRef::TaskSession {
            root_session_id,
            parent_task_session_id: SessionNodeId::from(parent_task_session_id),
        })
        .unwrap_or(ParentSessionRef::MainSession { root_session_id })
}

/// Converts one concrete controller configuration into a session effective-config snapshot.
/// 将一份具体控制器配置转换为会话有效配置快照。
fn core_controller_config_to_effective_config(
    controller_config: &CoreControllerConfig,
) -> SessionEffectiveConfig {
    let mut config = SessionEffectiveConfig::default();
    config.provider_model = ProviderModelSnapshot {
        provider_id: Some(controller_config.provider_id.clone()),
        model_id: Some(controller_config.model_id.clone()),
        reasoning_effort: None,
        execution_mode: None,
    };
    config.context_compaction.auto_compaction_enabled = true;
    if !controller_config.system_prompt.trim().is_empty() {
        config
            .system_prompt_layers
            .push(controller_config.system_prompt.clone());
    }
    config
}

/// Builds the append-only task state event payload.
/// 构建追加式任务状态事件载荷。
fn task_state_event_payload(
    record: &TaskSessionRecord,
    invocation: &TaskExecutionRequest,
    result_text: Option<&str>,
    error_text: Option<&str>,
    outcome: Option<&TaskExecutionOutcome>,
) -> Value {
    json!({
        "task_id": record.public_task_id,
        "task_session_id": record.task_id.to_string(),
        "root_session_id": record.origin_session_id.to_string(),
        "direct_parent_session_id": invocation.current_session_id.to_string(),
        "parent_task_id": record.parent_task_id.map(|value| value.to_string()),
        "display_title": record.display_title,
        "description": record.description,
        "subagent": record.subagent,
        "prompt": invocation.prompt,
        "result": result_text,
        "error": error_text,
        "provider_turns_executed": outcome.map(|value| value.provider_turns_executed).unwrap_or(0),
        "tool_calls_processed": outcome.map(|value| value.tool_calls_processed).unwrap_or(0),
        "final_assistant_message_id": outcome.and_then(|value| value.final_assistant_message_id.clone()),
    })
}

/// Builds a compact result summary suitable for task-session metadata.
/// 构建适合 TaskSession 元数据使用的紧凑结果摘要。
fn task_result_summary(result_text: &str) -> Option<String> {
    let summary = result_text.trim();
    if summary.is_empty() {
        return None;
    }
    const MAX_SUMMARY_CHARS: usize = 240;
    let mut chars = summary.chars();
    let clipped: String = chars.by_ref().take(MAX_SUMMARY_CHARS).collect();
    if chars.next().is_some() {
        Some(format!("{clipped}..."))
    } else {
        Some(clipped)
    }
}

/// Builds one task-session execution request and provider configuration from a shared parent runtime context.
/// 从共享父级运行时上下文构建一份子代理会话执行请求与供应商配置。
fn build_task_runtime_preparation_request(
    context: &TaskExecutionRuntimeContext,
    invocation: &TaskExecutionRequest,
    resolved_config: &TaskSessionResolvedConfig,
) -> (ProviderConfigurationFile, ExecutionPreparationRequest) {
    let mut config = context.config.clone();
    apply_task_provider_model_override(
        &mut config,
        &resolved_config.effective_config.provider_model,
    );
    let mut request = context.request_template.clone();
    request.session_id = invocation.task_id;
    request.execution_scope = ExecutionScope::TaskSession {
        root_session_id: SessionNodeId::from(invocation.root_session_id),
        task_session_id: SessionNodeId::from(invocation.task_id),
    };
    request.runtime_policy = resolved_config.runtime_policy;
    request.execution_agent_id = resolved_config
        .effective_config
        .subagent_profile_id
        .clone()
        .or_else(|| context.request_template.execution_agent_id.clone());
    request.agent_system_prompt_override = invocation
        .controller_config
        .as_ref()
        .and_then(|config| normalize_optional_task_label(Some(&config.system_prompt)));
    request.task_session_store_root = context.request_template.task_session_store_root.clone();
    request.session_title = invocation.display_title.clone();
    request.prompt = invocation.prompt.clone();
    request.conversation_state = core_snapshot_to_session_conversation_state(&invocation.snapshot);
    request.tool_exposure_override = resolved_config.effective_config.tool_exposure.clone();
    if let Some(execution_mode) = resolved_config
        .effective_config
        .provider_model
        .execution_mode
        .as_ref()
        .map(String::as_str)
        .and_then(|value| normalize_optional_task_label(Some(value)))
    {
        request.execution_mode = execution_mode;
    }
    if let Some(temperature) = resolved_config
        .effective_config
        .extra
        .get("temperature")
        .and_then(Value::as_f64)
    {
        request.temperature = Some(temperature.clamp(0.0, 2.0) as f32);
    }
    apply_task_prompt_skill_snapshot(
        &mut request,
        &resolved_config.effective_config.prompt_skills,
    );
    (config, request)
}

/// Applies one TaskSession provider/model snapshot to a mutable provider configuration clone.
/// 将一份 TaskSession 供应商与模型快照应用到可变供应商配置副本上。
fn apply_task_provider_model_override(
    config: &mut ProviderConfigurationFile,
    provider_model: &ProviderModelSnapshot,
) {
    let provider_id = provider_model
        .provider_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| {
            config
                .model
                .as_deref()
                .and_then(|model| model.split_once('/'))
                .map(|(provider, _)| provider.to_string())
        });
    let model_id = provider_model
        .model_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);

    if let Some(model_id) = model_id {
        if let Some((provider, model)) = model_id.split_once('/') {
            config.model = Some(format!("{provider}/{model}"));
        } else if let Some(provider_id) = provider_id {
            config.model = Some(format!("{provider_id}/{model_id}"));
        }
    }
}

/// Applies TaskSession prompt-skill overrides to the child execution request.
/// 将 TaskSession 提示词技能覆盖应用到子代理执行请求。
fn apply_task_prompt_skill_snapshot(
    request: &mut ExecutionPreparationRequest,
    prompt_skills: &PromptSkillSnapshot,
) {
    for skill in &prompt_skills.enabled {
        request
            .session_prompt_skill_overrides
            .insert(skill.clone(), PersistedPromptSkillState::Enabled);
    }
    for skill in &prompt_skills.disabled {
        request
            .session_prompt_skill_overrides
            .insert(skill.clone(), PersistedPromptSkillState::Disabled);
    }
}

/// Builds a core controller configuration snapshot from a fully prepared child launch.
/// 从一份完整准备后的子代理启动载荷构建核心控制器配置快照。
fn core_controller_config_from_child_launch(
    launch: &crate::agent_runtime::runtime_stream_launch_payload::RuntimeStreamLaunch,
) -> CoreControllerConfig {
    let max_turns = launch
        .preparation
        .prompt_purpose
        .max_turns_override()
        .or_else(|| {
            launch
                .active_agent
                .as_ref()
                .and_then(|agent| agent.max_turns)
                .map(usize::from)
        })
        .unwrap_or(usize::MAX - 1);
    CoreControllerConfig {
        provider_id: launch.preparation.model_blueprint.provider_id.clone(),
        model_id: launch.preparation.model_blueprint.model_id.clone(),
        system_prompt: launch.system_preamble.clone(),
        prompt_layers: CoreProviderPromptLayers::from_system_prompt(launch.system_preamble.clone()),
        temperature: launch.preparation.temperature.map(f64::from),
        max_turns,
        additional_params: None,
        context_injection: launch.preparation.context_injection.clone(),
    }
}

/// Builds one child runtime-stream launch through the same preparation and bootstrap bridge used by main sessions.
/// 通过主会话使用的同一准备与启动桥构建一份子代理运行时流启动载荷。
async fn prepare_task_runtime_stream_launch(
    context: &TaskExecutionRuntimeContext,
    invocation: &TaskExecutionRequest,
    resolved_config: &TaskSessionResolvedConfig,
) -> Result<crate::agent_runtime::runtime_stream_launch_payload::RuntimeStreamLaunch, String> {
    let (config, request) =
        build_task_runtime_preparation_request(context, invocation, resolved_config);
    let preparation = prepare_execution(
        &context.foundation,
        context.luaskills_runtime.clone(),
        &config,
        &request,
    )?;
    let bootstrap = bootstrap_runtime_stream(
        &context.foundation,
        context.luaskills_runtime.clone(),
        &request,
        preparation,
    )
    .await;
    let child_context = TaskExecutionRuntimeContext::new(
        config,
        Arc::clone(&context.foundation),
        context.luaskills_runtime.clone(),
        request.clone(),
    );
    Ok(build_runtime_stream_launch_payload(
        &request,
        bootstrap,
        Some(child_context),
    ))
}

/// Builds one child runtime-stream launch and performs a file-backed preflight compaction when the first preparation already exceeds the budget.
/// 构建子代理运行时流启动载荷，并在首次准备已超过预算时执行一次文件态前置压缩。
async fn prepare_task_runtime_stream_launch_with_preflight_compaction(
    task_runtime: Arc<TaskRuntime>,
    context: &TaskExecutionRuntimeContext,
    invocation: &mut TaskExecutionRequest,
    resolved_config: &TaskSessionResolvedConfig,
) -> Result<crate::agent_runtime::runtime_stream_launch_payload::RuntimeStreamLaunch, String> {
    let launch = prepare_task_runtime_stream_launch(context, invocation, resolved_config).await?;
    if !resolved_config
        .effective_config
        .context_compaction
        .auto_compaction_enabled
        || !task_launch_should_compact(&launch)
    {
        return Ok(launch);
    }

    if let Some(compacted_snapshot) = run_task_session_context_compaction(
        task_runtime,
        context.clone(),
        invocation.clone(),
        resolved_config.clone(),
    )
    .await?
    {
        invocation.snapshot = compacted_snapshot;
        return prepare_task_runtime_stream_launch(context, invocation, resolved_config).await;
    }

    Ok(launch)
}

/// Returns whether a prepared child launch should compact its context before provider execution.
/// 返回一份已准备的子代理启动载荷是否应在 Provider 执行前压缩上下文。
fn task_launch_should_compact(
    launch: &crate::agent_runtime::runtime_stream_launch_payload::RuntimeStreamLaunch,
) -> bool {
    launch
        .preparation
        .context_budget
        .as_ref()
        .is_some_and(|context_budget| context_budget.should_compact())
}

/// Converts a terminal runtime report back into the TaskSession outcome shape expected by persistence and parent cards.
/// 将终态运行时报告转换回持久化与父级卡片期望的 TaskSession 结果形态。
fn task_execution_outcome_from_report(
    invocation: &TaskExecutionRequest,
    controller_config: CoreControllerConfig,
    resolved_config: TaskSessionResolvedConfig,
    report: ExecutionRunReport,
) -> Result<TaskExecutionOutcome, String> {
    if let Some(error) = report.error {
        return Err(error);
    }
    let snapshot = report
        .conversation_state
        .as_ref()
        .map(build_core_conversation_snapshot)
        .unwrap_or_else(|| invocation.snapshot.clone());
    let final_output_text = report
        .assistant_output
        .filter(|output| !output.trim().is_empty())
        .or_else(|| extract_final_assistant_text(&snapshot))
        .unwrap_or_else(|| {
            String::from(
                "The task completed, but it did not produce reusable final assistant text.",
            )
        });
    Ok(TaskExecutionOutcome {
        task_id: invocation.task_id,
        controller_config,
        resolved_config,
        final_assistant_message_id: extract_final_assistant_message_id(&snapshot),
        snapshot,
        provider_turns_executed: report.provider_request_count,
        tool_calls_processed: report
            .controller_events
            .iter()
            .filter(|event| matches!(event.kind, vulcan_core::types::CoreEventKind::ToolFinished))
            .count(),
        final_output_text,
        execution_status: TaskExecutionStatus::Completed,
    })
}

/// Runs one child task through the shared runtime preparation and provider-dispatch bridge.
/// 通过共享运行时准备与 Provider 分发桥执行一条子代理任务。
async fn run_task_with_shared_runtime_context(
    task_runtime: Arc<TaskRuntime>,
    context: TaskExecutionRuntimeContext,
    base_controller_config: CoreControllerConfig,
    parent_live_sender: ProviderLiveDeltaSender,
    request: TaskExecutionRequest,
) -> Result<TaskExecutionOutcome, String> {
    let mut request = request;
    let task_id = request.task_id;
    let trace_started_at = current_unix_millis_u64();
    task_runtime_trace(
        "shared_task_start",
        format!(
            "parent={} task={} public_task_id={} depth={}",
            request.current_session_id, request.task_id, request.public_task_id, request.depth
        ),
    );
    let resolved_config = task_runtime.resolve_task_session_config_for_execution(
        &base_controller_config,
        &request.invocation_config,
    )?;
    task_runtime_trace(
        "shared_task_config_resolved",
        format!(
            "task={} elapsed_ms={}",
            task_id,
            current_unix_millis_u64().saturating_sub(trace_started_at)
        ),
    );
    let running_controller_config = build_task_controller_config(
        &base_controller_config,
        request.controller_config.clone(),
        &request.description,
        &request.subagent,
        &request.invocation_config,
        &resolved_config,
    );
    request.controller_config = Some(running_controller_config.clone());
    let started_payload = build_task_host_payload(
        &request,
        "running",
        0,
        0,
        None,
        "",
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
    task_runtime_trace(
        "shared_task_delegate_started",
        format!(
            "task={} elapsed_ms={}",
            task_id,
            current_unix_millis_u64().saturating_sub(trace_started_at)
        ),
    );

    let background_task_runtime = Arc::clone(&task_runtime);
    let background_request = request.clone();
    let background_resolved_config = resolved_config.clone();
    let background_running_controller_config = running_controller_config.clone();
    let compaction_runtime_context = context.clone();
    let running_outcome = build_running_task_execution_outcome(
        task_id,
        running_controller_config.clone(),
        resolved_config.clone(),
        request.snapshot.clone(),
    );
    task_runtime.store_running_task_outcome_in_memory(
        request.current_session_id,
        &request,
        &running_outcome,
    )?;
    task_runtime_trace(
        "shared_task_running_stored_in_memory",
        format!(
            "task={} elapsed_ms={}",
            task_id,
            current_unix_millis_u64().saturating_sub(trace_started_at)
        ),
    );
    let child_run_task = tokio::spawn(async move {
        let mut background_request = background_request;
        let launch = prepare_task_runtime_stream_launch_with_preflight_compaction(
            Arc::clone(&background_task_runtime),
            &context,
            &mut background_request,
            &background_resolved_config,
        )
        .await?;
        let task_controller_config = core_controller_config_from_child_launch(&launch);
        let mut terminal_report = None;
        let config = launch
            .task_execution_context
            .as_ref()
            .map(|context| context.config.clone())
            .unwrap_or_else(|| context.config.clone());
        let mut child_event_stream =
            Box::pin(drive_runtime_stream_provider_loop(config, launch, None));
        while let Some(event) = child_event_stream.next().await {
            if let ExecutionStreamEvent::Finished(report) = &event {
                terminal_report = Some(report.clone());
            }
            background_task_runtime.publish_detached_task_event(
                background_request.task_id,
                DetachedTaskRuntimeEvent::ChildExecution(event),
            );
        }
        let report = terminal_report.ok_or_else(|| {
            format!(
                "Task `{}` finished without a terminal execution report.",
                background_request.task_id
            )
        })?;
        let should_compact_context = report
            .preparation
            .as_ref()
            .and_then(|preparation| preparation.context_budget.as_ref())
            .is_some_and(|context_budget| context_budget.should_compact());
        let outcome = task_execution_outcome_from_report(
            &background_request,
            task_controller_config,
            background_resolved_config,
            report,
        )?;
        Ok::<TaskExecutionCompletion, String>(TaskExecutionCompletion {
            outcome,
            should_compact_context,
        })
    });
    let child_run_abort_handle = child_run_task.abort_handle();
    let child_run_abort_guard = AbortChildRunOnDrop {
        handle: child_run_abort_handle.clone(),
    };
    let active_task_run_guard =
        task_runtime.install_active_task_run(task_id, child_run_abort_handle);
    task_runtime_trace(
        "shared_task_child_spawned",
        format!(
            "task={} elapsed_ms={}",
            task_id,
            current_unix_millis_u64().saturating_sub(trace_started_at)
        ),
    );
    let background_task_runtime = Arc::clone(&task_runtime);
    let return_resolved_config = resolved_config.clone();
    let return_snapshot = request.snapshot.clone();
    tokio::spawn(async move {
        let child_run_abort_guard = child_run_abort_guard;
        let active_task_run_guard = active_task_run_guard;
        let terminal_outcome = match child_run_task.await {
            Ok(Ok(completion)) => {
                let outcome = completion.outcome;
                let _ = background_task_runtime.store_task_outcome(
                    request.current_session_id,
                    &request,
                    &outcome,
                );
                publish_task_finished_to_parent(
                    &background_task_runtime,
                    &request,
                    &outcome,
                    "The subagent task has completed.",
                );
                enqueue_task_session_parent_outcome_followup_if_needed(
                    Arc::clone(&background_task_runtime),
                    compaction_runtime_context.clone(),
                    &request,
                    &outcome,
                );
                if completion.should_compact_context {
                    spawn_task_session_context_compaction(
                        Arc::clone(&background_task_runtime),
                        compaction_runtime_context.clone(),
                        request.clone(),
                        outcome.resolved_config.clone(),
                    );
                }
                Ok(outcome)
            }
            Ok(Err(error)) if task_execution_error_is_stopped(error.as_str()) => {
                let stopped_snapshot = current_task_snapshot_from_store(
                    &background_task_runtime,
                    request.root_session_id,
                    task_id,
                )
                .unwrap_or_else(|| request.snapshot.clone());
                let outcome = build_stopped_task_execution_outcome(
                    task_id,
                    background_running_controller_config.clone(),
                    resolved_config.clone(),
                    stopped_snapshot,
                );
                let _ = background_task_runtime.store_task_outcome(
                    request.current_session_id,
                    &request,
                    &outcome,
                );
                publish_task_finished_to_parent(
                    &background_task_runtime,
                    &request,
                    &outcome,
                    outcome.final_output_text.as_str(),
                );
                enqueue_task_session_parent_outcome_followup_if_needed(
                    Arc::clone(&background_task_runtime),
                    compaction_runtime_context.clone(),
                    &request,
                    &outcome,
                );
                Ok(outcome)
            }
            Ok(Err(error)) => {
                let _ = background_task_runtime.mark_task_failure(
                    request.current_session_id,
                    &request,
                    &error,
                );
                publish_task_failure_to_parent(&background_task_runtime, &request, &error);
                enqueue_task_session_parent_failure_followup_if_needed(
                    Arc::clone(&background_task_runtime),
                    compaction_runtime_context.clone(),
                    &request,
                    error.as_str(),
                );
                Err(error)
            }
            Err(join_error) if join_error.is_cancelled() => {
                let stopped_snapshot = current_task_snapshot_from_store(
                    &background_task_runtime,
                    request.root_session_id,
                    task_id,
                )
                .unwrap_or_else(|| request.snapshot.clone());
                let outcome = build_stopped_task_execution_outcome(
                    task_id,
                    background_running_controller_config.clone(),
                    resolved_config.clone(),
                    stopped_snapshot,
                );
                let _ = background_task_runtime.store_task_outcome(
                    request.current_session_id,
                    &request,
                    &outcome,
                );
                publish_task_finished_to_parent(
                    &background_task_runtime,
                    &request,
                    &outcome,
                    outcome.final_output_text.as_str(),
                );
                enqueue_task_session_parent_outcome_followup_if_needed(
                    Arc::clone(&background_task_runtime),
                    compaction_runtime_context.clone(),
                    &request,
                    &outcome,
                );
                Ok(outcome)
            }
            Err(join_error) => {
                let error = format!(
                    "Task `{}` failed: {}",
                    task_id,
                    format_task_join_error(&join_error)
                );
                let _ = background_task_runtime.mark_task_failure(
                    request.current_session_id,
                    &request,
                    &error,
                );
                publish_task_failure_to_parent(&background_task_runtime, &request, &error);
                enqueue_task_session_parent_failure_followup_if_needed(
                    Arc::clone(&background_task_runtime),
                    compaction_runtime_context.clone(),
                    &request,
                    error.as_str(),
                );
                Err(error)
            }
        };
        drop(active_task_run_guard);
        drop(child_run_abort_guard);
        let spawned_followup = match &terminal_outcome {
            Ok(outcome) if outcome.execution_status == TaskExecutionStatus::Completed => {
                background_task_runtime.try_spawn_next_task_session_parent_followup(task_id)
            }
            Ok(outcome)
                if matches!(
                    outcome.execution_status,
                    TaskExecutionStatus::Stopped | TaskExecutionStatus::Running
                ) =>
            {
                background_task_runtime.clear_task_session_parent_followups(task_id);
                false
            }
            Err(_) => {
                background_task_runtime.clear_task_session_parent_followups(task_id);
                false
            }
            Ok(_) => false,
        };
        if !spawned_followup {
            if !background_task_runtime.task_has_active_descendant_runs(task_id) {
                background_task_runtime.close_detached_task_event_stream(task_id);
            }
        }
    });

    task_runtime_trace(
        "shared_task_running_return",
        format!(
            "task={} elapsed_ms={}",
            task_id,
            current_unix_millis_u64().saturating_sub(trace_started_at)
        ),
    );
    Ok(build_running_task_execution_outcome(
        task_id,
        running_controller_config,
        return_resolved_config,
        return_snapshot,
    ))
}

/// Enqueues one terminal child result for a TaskSession parent when the direct parent is itself a task.
/// 当直接父级本身也是 TaskSession 时，将一条终态子结果排入父级跟进队列。
fn enqueue_task_session_parent_outcome_followup_if_needed(
    task_runtime: Arc<TaskRuntime>,
    context: TaskExecutionRuntimeContext,
    request: &TaskExecutionRequest,
    outcome: &TaskExecutionOutcome,
) {
    let run_state = match outcome.execution_status {
        TaskExecutionStatus::Completed => SubagentTaskRunState::Completed,
        TaskExecutionStatus::Stopped => SubagentTaskRunState::Stopped,
        TaskExecutionStatus::Running => return,
    };
    enqueue_task_session_parent_followup_state_if_needed(
        task_runtime,
        context,
        request,
        task_session_parent_followup_state(request, run_state, outcome.final_output_text.clone()),
    );
}

/// Enqueues one failed child result for a TaskSession parent when the direct parent is itself a task.
/// 当直接父级本身也是 TaskSession 时，将一条失败子结果排入父级跟进队列。
fn enqueue_task_session_parent_failure_followup_if_needed(
    task_runtime: Arc<TaskRuntime>,
    context: TaskExecutionRuntimeContext,
    request: &TaskExecutionRequest,
    error: &str,
) {
    enqueue_task_session_parent_followup_state_if_needed(
        task_runtime,
        context,
        request,
        task_session_parent_followup_state(
            request,
            SubagentTaskRunState::Failed,
            error.to_string(),
        ),
    );
}

/// Enqueues one shared delegate-completion state into the parent TaskSession runtime queue.
/// 将一条共享委派完成状态排入父级 TaskSession 运行时队列。
fn enqueue_task_session_parent_followup_state_if_needed(
    task_runtime: Arc<TaskRuntime>,
    context: TaskExecutionRuntimeContext,
    request: &TaskExecutionRequest,
    state: SubagentTaskPanelState,
) {
    let Some(parent_task_id) = request.parent_task_id else {
        return;
    };
    if parent_task_id != request.current_session_id {
        return;
    }
    if !task_runtime
        .task_session_parent_accepts_followup(parent_task_id, request.root_session_id)
        .unwrap_or(false)
    {
        return;
    }
    let followup = TaskSessionParentFollowup {
        root_session_id: request.root_session_id,
        child_task_id: request.task_id,
        context,
        prompt: build_delegate_completion_followup_prompt(&state),
    };
    match task_runtime.queue_task_session_parent_followup(parent_task_id, followup) {
        Ok(true) => {
            task_runtime.try_spawn_next_task_session_parent_followup(parent_task_id);
        }
        Ok(false) => {}
        Err(error) => {
            eprintln!("[task-runtime] failed to enqueue parent TaskSession follow-up: {error}");
        }
    }
}

/// Builds the shared subagent panel state used to render and inject a delegated child completion.
/// 构建用于显示并注入委派子任务完成结果的共享子代理面板状态。
fn task_session_parent_followup_state(
    request: &TaskExecutionRequest,
    run_state: SubagentTaskRunState,
    result_text: String,
) -> SubagentTaskPanelState {
    SubagentTaskPanelState {
        task_id: request.task_id,
        public_task_id: request.public_task_id.clone(),
        parent_session_id: request.current_session_id,
        subagent_label: task_session_tool_display_label(
            Some(request.display_title.as_str()),
            request.description.as_deref(),
            request.subagent.as_deref(),
            Some(request.public_task_id.as_str()),
            Some(request.prompt.as_str()),
        ),
        description: request.description.clone(),
        run_state,
        result_text,
        conversation_state: None,
    }
}

/// Runs one queued parent TaskSession follow-up through the same provider loop used by normal child tasks.
/// 通过普通子任务使用的同一 Provider 循环执行一条排队的父级 TaskSession 跟进。
async fn run_task_session_parent_followup_stream(
    task_runtime: Arc<TaskRuntime>,
    context: TaskExecutionRuntimeContext,
    mut request: TaskExecutionRequest,
    resolved_config: TaskSessionResolvedConfig,
) -> Result<TaskExecutionCompletion, String> {
    let launch = prepare_task_runtime_stream_launch_with_preflight_compaction(
        Arc::clone(&task_runtime),
        &context,
        &mut request,
        &resolved_config,
    )
    .await?;
    let task_controller_config = core_controller_config_from_child_launch(&launch);
    let mut terminal_report = None;
    let config = launch
        .task_execution_context
        .as_ref()
        .map(|context| context.config.clone())
        .unwrap_or_else(|| context.config.clone());
    let mut child_event_stream = Box::pin(drive_runtime_stream_provider_loop(config, launch, None));
    while let Some(event) = child_event_stream.next().await {
        if let ExecutionStreamEvent::Finished(report) = &event {
            terminal_report = Some(report.clone());
        }
        task_runtime.publish_detached_task_event(
            request.task_id,
            DetachedTaskRuntimeEvent::ChildExecution(event),
        );
    }
    let report = terminal_report.ok_or_else(|| {
        format!(
            "Task `{}` finished without a terminal execution report.",
            request.task_id
        )
    })?;
    let should_compact_context = report
        .preparation
        .as_ref()
        .and_then(|preparation| preparation.context_budget.as_ref())
        .is_some_and(|context_budget| context_budget.should_compact());
    let outcome = task_execution_outcome_from_report(
        &request,
        task_controller_config,
        resolved_config,
        report,
    )?;
    Ok(TaskExecutionCompletion {
        outcome,
        should_compact_context,
    })
}

/// Finalizes one parent TaskSession follow-up run and schedules the next queued follow-up when appropriate.
/// 收尾一轮父级 TaskSession 跟进运行，并在合适时调度下一条排队跟进。
async fn finalize_task_session_parent_followup_run(
    task_runtime: Arc<TaskRuntime>,
    task_id: SessionId,
    context: TaskExecutionRuntimeContext,
    request: TaskExecutionRequest,
    resolved_config: TaskSessionResolvedConfig,
    run_task: JoinHandle<Result<TaskExecutionCompletion, String>>,
    abort_guard: AbortChildRunOnDrop,
    active_run_guard: ActiveTaskRunGuard,
) {
    let terminal_outcome = match run_task.await {
        Ok(Ok(completion)) => {
            let outcome = completion.outcome;
            let _ = task_runtime.store_task_outcome(request.current_session_id, &request, &outcome);
            publish_task_finished_to_parent(
                &task_runtime,
                &request,
                &outcome,
                "The subagent task has completed.",
            );
            enqueue_task_session_parent_outcome_followup_if_needed(
                Arc::clone(&task_runtime),
                context.clone(),
                &request,
                &outcome,
            );
            if completion.should_compact_context {
                spawn_task_session_context_compaction(
                    Arc::clone(&task_runtime),
                    context.clone(),
                    request.clone(),
                    outcome.resolved_config.clone(),
                );
            }
            Ok(outcome)
        }
        Ok(Err(error)) => {
            let _ = task_runtime.mark_task_failure(request.current_session_id, &request, &error);
            publish_task_failure_to_parent(&task_runtime, &request, &error);
            enqueue_task_session_parent_failure_followup_if_needed(
                Arc::clone(&task_runtime),
                context.clone(),
                &request,
                error.as_str(),
            );
            Err(error)
        }
        Err(join_error) if join_error.is_cancelled() => {
            let stopped_snapshot =
                current_task_snapshot_from_store(&task_runtime, request.root_session_id, task_id)
                    .unwrap_or_else(|| request.snapshot.clone());
            if let Some(controller_config) = request.controller_config.clone() {
                let outcome = build_stopped_task_execution_outcome(
                    task_id,
                    controller_config,
                    resolved_config,
                    stopped_snapshot,
                );
                let _ =
                    task_runtime.store_task_outcome(request.current_session_id, &request, &outcome);
                publish_task_finished_to_parent(
                    &task_runtime,
                    &request,
                    &outcome,
                    outcome.final_output_text.as_str(),
                );
                enqueue_task_session_parent_outcome_followup_if_needed(
                    Arc::clone(&task_runtime),
                    context.clone(),
                    &request,
                    &outcome,
                );
                Ok(outcome)
            } else {
                let error = String::from("cancelled");
                let _ =
                    task_runtime.mark_task_failure(request.current_session_id, &request, &error);
                publish_task_failure_to_parent(&task_runtime, &request, &error);
                Err(error)
            }
        }
        Err(join_error) => {
            let error = format_task_join_error(&join_error);
            let _ = task_runtime.mark_task_failure(request.current_session_id, &request, &error);
            publish_task_failure_to_parent(&task_runtime, &request, &error);
            enqueue_task_session_parent_failure_followup_if_needed(
                Arc::clone(&task_runtime),
                context.clone(),
                &request,
                error.as_str(),
            );
            Err(error)
        }
    };

    drop(active_run_guard);
    drop(abort_guard);
    let spawned_followup = match &terminal_outcome {
        Ok(outcome) if outcome.execution_status == TaskExecutionStatus::Completed => {
            task_runtime.try_spawn_next_task_session_parent_followup(task_id)
        }
        Ok(_) | Err(_) => {
            task_runtime.clear_task_session_parent_followups(task_id);
            false
        }
    };
    if !spawned_followup {
        if !task_runtime.task_has_active_descendant_runs(task_id) {
            task_runtime.close_detached_task_event_stream(task_id);
        }
    }
}

/// Loads the latest file-backed TaskSession context as a core snapshot when it is available.
/// 在可用时将最新文件态 TaskSession 上下文加载为核心快照。
fn current_task_snapshot_from_store(
    task_runtime: &TaskRuntime,
    root_session_id: SessionId,
    task_id: SessionId,
) -> Option<CoreConversationSnapshot> {
    let store = task_runtime
        .task_session_store_for_root(root_session_id)
        .ok()
        .flatten()?;
    let context = store.load_context(SessionNodeId::from(task_id)).ok()??;
    let conversation_state: SessionConversationState =
        serde_json::from_value(context.conversation_state).ok()?;
    Some(build_core_conversation_snapshot(&conversation_state))
}

/// Spawns one non-blocking TaskSession context compaction job after a child task has completed.
/// 在子任务完成后派生一个非阻塞 TaskSession 上下文压缩任务。
fn spawn_task_session_context_compaction(
    task_runtime: Arc<TaskRuntime>,
    context: TaskExecutionRuntimeContext,
    request: TaskExecutionRequest,
    resolved_config: TaskSessionResolvedConfig,
) {
    if !resolved_config
        .effective_config
        .context_compaction
        .auto_compaction_enabled
    {
        return;
    }

    tokio::spawn(async move {
        if let Err(error) =
            run_task_session_context_compaction(task_runtime, context, request, resolved_config)
                .await
        {
            eprintln!("[task-session-store] context compaction failed: {error}");
        }
    });
}

/// Runs one TaskSession context compaction pass using the shared prompt-compaction runtime and file store.
/// 使用共享提示词压缩运行时与文件存储执行一次 TaskSession 上下文压缩。
async fn run_task_session_context_compaction(
    task_runtime: Arc<TaskRuntime>,
    context: TaskExecutionRuntimeContext,
    request: TaskExecutionRequest,
    resolved_config: TaskSessionResolvedConfig,
) -> Result<Option<CoreConversationSnapshot>, String> {
    let Some(store) = task_runtime.task_session_store_for_root(request.root_session_id)? else {
        return Ok(None);
    };
    let task_node_id = SessionNodeId::from(request.task_id);
    let Some(context_document) = store
        .load_context(task_node_id)
        .map_err(|error| error.message)?
    else {
        return Ok(None);
    };
    let conversation_state: SessionConversationState =
        serde_json::from_value(context_document.conversation_state.clone())
            .map_err(|error| error.to_string())?;
    let messages = conversation_state_to_session_messages(&conversation_state);
    if messages.is_empty() {
        return Ok(None);
    }
    let Some(plan) = plan_context_compaction(CompactionPlanInput {
        session_label: &request.public_task_id,
        messages: &messages,
        all_messages: &messages,
        context_start_message_id: context_document.compaction_anchor.as_deref(),
        policy: task_context_compaction_policy_from_snapshot(
            &resolved_config.effective_config.context_compaction,
        ),
    }) else {
        return Ok(None);
    };
    let event_sequence = store
        .load_events(task_node_id)
        .map(|events| events.len() as u64)
        .unwrap_or(0);
    let (provider_configuration, _) =
        build_task_runtime_preparation_request(&context, &request, &resolved_config);
    let model_label = provider_configuration.model.clone();
    let stream = stream_prompt_compaction_foundation_job(PromptCompactionFoundationJobRequest {
        session_id: request.task_id,
        session_title: request.display_title.clone(),
        workspace_path: context.request_template.workspace_path.clone(),
        plan,
        provider_configuration,
        model_label,
        reasoning_effort: context.request_template.reasoning_effort,
        ai_tool_configuration: context.request_template.ai_tool_configuration.clone(),
        foundation: Arc::clone(&context.foundation),
        luaskills_runtime: context.luaskills_runtime.clone(),
    });
    futures_util::pin_mut!(stream);
    while let Some(event) = stream.next().await {
        let PromptCompactionStreamEvent::Finished(result) = event else {
            continue;
        };
        let execution = result?;
        let application = apply_task_session_compaction_result(
            context_document,
            &execution.compaction_result,
            event_sequence,
        )
        .map_err(|error| error.message)?;
        let compacted_conversation_state: SessionConversationState =
            serde_json::from_value(application.context.conversation_state.clone())
                .map_err(|error| error.to_string())?;
        let compacted_snapshot = build_core_conversation_snapshot(&compacted_conversation_state);
        let checkpoint_id = application.checkpoint.checkpoint_id.clone();
        let timestamp_ms = application.checkpoint.timestamp_ms;
        let summary = application.checkpoint.summary.clone();
        store
            .save_compacted_context(application.context)
            .map_err(|error| error.message)?;
        store
            .save_compaction_checkpoint(application.checkpoint)
            .map_err(|error| error.message)?;
        store
            .append_event(SessionEventRecord::new(
                format!("task-context-compaction-{checkpoint_id}"),
                task_node_id,
                event_sequence,
                SessionEventKind::RuntimeMarker,
                timestamp_ms,
                json!({
                    "context_compaction_status": "completed",
                    "checkpoint_id": checkpoint_id,
                    "summary": summary,
                    "fallback_used": execution.fallback_used,
                    "provider_request_count": execution.provider_request_count,
                }),
            ))
            .map_err(|error| error.message)?;
        return Ok(Some(compacted_snapshot));
    }
    Ok(None)
}

/// Builds the shared compaction planning policy from a resolved TaskSession policy snapshot.
/// 根据已解析的 TaskSession 策略快照构建共享压缩规划策略。
fn task_context_compaction_policy_from_snapshot(
    snapshot: &ContextCompactionPolicySnapshot,
) -> ContextCompactionPolicy {
    ContextCompactionPolicy {
        auto: snapshot.auto_compaction_enabled,
        ..ContextCompactionPolicy::default()
    }
}

/// Publishes a completed or stopped child-task lifecycle event back to the parent session.
/// 将完成或停止的子任务生命周期事件发布回父级会话。
fn publish_task_finished_to_parent(
    task_runtime: &TaskRuntime,
    request: &TaskExecutionRequest,
    outcome: &TaskExecutionOutcome,
    detail: &str,
) {
    let finished_payload = build_task_host_payload(
        request,
        outcome.execution_status.as_str(),
        outcome.provider_turns_executed,
        outcome.tool_calls_processed,
        outcome.final_assistant_message_id.as_deref(),
        outcome.final_output_text.as_str(),
        current_unix_millis_text(),
    );
    task_runtime.publish_detached_task_event(
        request.task_id,
        DetachedTaskRuntimeEvent::ParentRuntime {
            parent_session_id: request.current_session_id,
            event: build_task_delegate_runtime_event(
                RuntimeEventKind::DelegateFinished,
                request,
                detail,
                finished_payload,
            ),
        },
    );
}

/// Publishes a failed child-task lifecycle event back to the parent session.
/// 将失败的子任务生命周期事件发布回父级会话。
fn publish_task_failure_to_parent(
    task_runtime: &TaskRuntime,
    request: &TaskExecutionRequest,
    error: &str,
) {
    let finished_payload = build_task_host_payload(
        request,
        "failed",
        0,
        0,
        None,
        error,
        current_unix_millis_text(),
    );
    task_runtime.publish_detached_task_event(
        request.task_id,
        DetachedTaskRuntimeEvent::ParentRuntime {
            parent_session_id: request.current_session_id,
            event: build_task_delegate_runtime_event(
                RuntimeEventKind::DelegateFinished,
                request,
                error,
                finished_payload,
            ),
        },
    );
}

/// Builds one backend handler that executes task sessions through the shared runtime preparation bridge only.
/// 构建一条只通过共享运行时准备桥执行任务会话的后端处理器。
pub(crate) fn build_task_execution_handler(
    task_runtime: Arc<TaskRuntime>,
    base_controller_config: CoreControllerConfig,
    parent_live_sender: ProviderLiveDeltaSender,
) -> TaskExecutionHandler {
    Arc::new(move |request: TaskExecutionRequest| {
        let task_runtime = Arc::clone(&task_runtime);
        let base_controller_config = base_controller_config.clone();
        let parent_live_sender = parent_live_sender.clone();
        Box::pin(async move {
            if let Some(runtime_context) =
                task_runtime.execution_context_for_session(request.current_session_id)
            {
                return run_task_with_shared_runtime_context(
                    Arc::clone(&task_runtime),
                    runtime_context,
                    base_controller_config,
                    parent_live_sender,
                    request,
                )
                .await;
            }

            Err(format!(
                "Task `{}` cannot start because the shared runtime execution context is not registered for parent session `{}`.",
                request.public_task_id, request.current_session_id
            ))
        })
    })
}

/// Builds one synthetic running task outcome returned immediately while the real child controller keeps executing in the background.
/// 构建一条会被立即返回的合成运行中任务结果，同时真实子控制器继续在后台执行。
fn build_running_task_execution_outcome(
    task_id: SessionId,
    controller_config: CoreControllerConfig,
    resolved_config: TaskSessionResolvedConfig,
    snapshot: CoreConversationSnapshot,
) -> TaskExecutionOutcome {
    TaskExecutionOutcome {
        task_id,
        controller_config,
        resolved_config,
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
    resolved_config: TaskSessionResolvedConfig,
    snapshot: CoreConversationSnapshot,
) -> TaskExecutionOutcome {
    TaskExecutionOutcome {
        task_id,
        controller_config,
        resolved_config,
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
    updated_at: String,
) -> Value {
    json!({
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
    })
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
    invocation_config: &TaskInvocationConfig,
    resolved_config: &TaskSessionResolvedConfig,
) -> CoreControllerConfig {
    if let Some(stored) = stored {
        return stored;
    }

    let mut config = base.clone();
    apply_resolved_provider_model_to_controller(&mut config, base, resolved_config);
    apply_resolved_runtime_extra_to_controller(&mut config, resolved_config);
    let functional_system_prompt =
        task_functional_system_prompt(base, invocation_config, resolved_config);
    let inherit_parent_config = resolved_config.effective_config.inherit_parent_config;
    let mut system_prompt = build_subagent_clone_system_prompt(
        base,
        description,
        subagent,
        inherit_parent_config,
        functional_system_prompt.as_deref(),
    )
    .unwrap_or_else(|| {
        build_legacy_task_system_prompt(
            base,
            description,
            subagent,
            inherit_parent_config,
            functional_system_prompt.as_deref(),
        )
    });
    append_task_windows_path_guidance(&mut system_prompt);
    if !system_prompt.trim().is_empty() {
        config.system_prompt = system_prompt;
    }
    config
}

/// Applies provider/model overrides that can safely use the existing provider transport.
/// 应用可安全复用当前 Provider 传输层的供应商与模型覆盖。
fn apply_resolved_provider_model_to_controller(
    config: &mut CoreControllerConfig,
    base: &CoreControllerConfig,
    resolved_config: &TaskSessionResolvedConfig,
) {
    let provider_model = &resolved_config.effective_config.provider_model;
    if provider_model
        .provider_id
        .as_deref()
        .is_some_and(|provider_id| provider_id == base.provider_id)
    {
        config.provider_id = base.provider_id.clone();
    }
    if provider_model.provider_id.is_none()
        || provider_model.provider_id.as_deref() == Some(base.provider_id.as_str())
    {
        if let Some(model_id) = provider_model
            .model_id
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            config.model_id = model_id.to_string();
        }
    }
}

/// Applies runtime scalar overrides stored in the resolved TaskSession extra payload.
/// 应用已解析 TaskSession 额外载荷中存储的运行时标量覆盖。
fn apply_resolved_runtime_extra_to_controller(
    config: &mut CoreControllerConfig,
    resolved_config: &TaskSessionResolvedConfig,
) {
    let Value::Object(extra) = &resolved_config.effective_config.extra else {
        return;
    };
    if let Some(temperature) = extra
        .get("temperature")
        .and_then(Value::as_f64)
        .map(|value| value.clamp(0.0, 2.0))
    {
        config.temperature = Some(temperature);
    }
    if let Some(max_turns) = extra
        .get("max_turns")
        .and_then(Value::as_u64)
        .filter(|value| *value > 0)
    {
        config.max_turns = max_turns as usize;
    }
}

/// Builds a routed subagent system prompt through the prompt composer.
/// 通过提示词组合器构建按配置路由的子代理系统提示词。
fn build_subagent_clone_system_prompt(
    base: &CoreControllerConfig,
    description: &Option<String>,
    subagent: &Option<String>,
    inherit_parent_config: bool,
    functional_system_prompt: Option<&str>,
) -> Option<String> {
    let functional_system_prompt = functional_system_prompt
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let mode = if functional_system_prompt.is_some() {
        PromptAssemblyMode::SubagentFunctional
    } else {
        PromptAssemblyMode::SubagentClone
    };
    let output = runtime_prompt_composer_engine()
        .compose(&PromptAssemblyInput {
            mode,
            model: ModelDescriptor {
                family: base.provider_id.clone(),
                model_id: base.model_id.clone(),
                supports_native_personality: true,
            },
            provider_prompt: inherit_parent_config.then(|| {
                ProviderPromptSnapshot::new(base.system_prompt.clone())
                    .with_source_label("Parent runtime system prompt")
            }),
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
                functional_system_prompt: functional_system_prompt.map(str::to_string),
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
    inherit_parent_config: bool,
    functional_system_prompt: Option<&str>,
) -> String {
    let mut system_prompt = if inherit_parent_config {
        base.system_prompt.clone()
    } else {
        String::new()
    };
    if let Some(subagent) = subagent.as_ref().filter(|value| !value.trim().is_empty()) {
        system_prompt.push_str(&format!("\n\n[Subagent]\n{subagent}"));
    }
    if let Some(description) = description
        .as_ref()
        .filter(|value| !value.trim().is_empty())
    {
        system_prompt.push_str(&format!("\n\n[Task]\n{description}"));
    }
    if let Some(functional_system_prompt) = functional_system_prompt
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        system_prompt.push_str(&format!(
            "\n\n[Subagent Functional System Prompt]\n{functional_system_prompt}"
        ));
    }
    system_prompt
}

/// Builds the functional system prompt override used by the prompt composer.
/// 构建提示词组合器使用的功能性系统提示词覆盖。
fn task_functional_system_prompt(
    base: &CoreControllerConfig,
    config: &TaskInvocationConfig,
    resolved_config: &TaskSessionResolvedConfig,
) -> Option<String> {
    let mut sections = Vec::new();
    if config.system_prompt.is_some() {
        sections.extend(
            resolved_config
                .effective_config
                .system_prompt_layers
                .iter()
                .filter_map(|layer| normalize_optional_task_label(Some(layer))),
        );
    } else {
        let parent_prompt = base.system_prompt.trim();
        sections.extend(
            resolved_config
                .effective_config
                .system_prompt_layers
                .iter()
                .filter_map(|layer| normalize_optional_task_label(Some(layer)))
                .filter(|layer| layer.trim() != parent_prompt),
        );
    }
    dedupe_preserving_order(&mut sections);
    (!sections.is_empty()).then(|| sections.join("\n\n"))
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

/// Extracts the last visible assistant message id from one canonical conversation snapshot.
/// 从一份规范会话快照中提取最后一条可见助手消息标识。
fn extract_final_assistant_message_id(snapshot: &CoreConversationSnapshot) -> Option<String> {
    snapshot.messages.iter().rev().find_map(|message| {
        if let vulcan_core::types::ConversationPayload::Assistant { text, .. } = &message.payload {
            if !text.trim().is_empty() {
                return Some(message.id.clone());
            }
        }
        None
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_profile::AgentProfileCatalog;
    use crate::agent_runtime::execution_models::ExecutionPromptPurpose;
    use crate::provider_config::{
        PersistedAiToolConfiguration, ProjectPromptSkillsConfiguration, ReasoningEffortLevel,
    };
    use std::collections::BTreeMap;

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

    /// Builds one minimal task-tool argument object for task-runtime tests.
    /// 为 task-runtime 测试构建一份最小 task 工具参数对象。
    fn test_task_args(prompt: &str, task_id: Option<&str>) -> TaskToolInvocationArgs {
        TaskToolInvocationArgs {
            prompt: prompt.to_string(),
            description: None,
            task_id: task_id.map(str::to_string),
            subagent: None,
            subagent_profile_id: None,
            profile: None,
            inherit_parent_config: None,
            system_prompt: None,
            system_prompt_append: None,
            provider: None,
            model: None,
            reasoning_effort: None,
            execution_mode: None,
            provider_model: None,
            tools: None,
            ai_capabilities: None,
            skills: None,
            prompt_skills: None,
            luaskills: None,
            mcp: None,
            context_compaction: None,
            runtime_policy: None,
            extra: None,
        }
    }

    /// Builds a resolved TaskSession configuration for controller-config tests.
    /// 为控制器配置测试构建已解析 TaskSession 配置。
    fn test_resolved_config(
        base: &CoreControllerConfig,
        invocation: &TaskInvocationConfig,
    ) -> TaskSessionResolvedConfig {
        TaskSessionConfigResolver::resolve(TaskSessionConfigResolutionInput {
            global_default_config: SessionEffectiveConfig::default(),
            parent_config: Some(core_controller_config_to_effective_config(base)),
            profile: None,
            invocation: invocation.clone(),
        })
    }

    /// Builds one minimal parent execution request for task-session request tests.
    /// 为 TaskSession 请求测试构建一份最小父级执行请求。
    fn test_parent_execution_request(session_id: SessionId) -> ExecutionPreparationRequest {
        ExecutionPreparationRequest {
            session_id,
            execution_scope: ExecutionScope::MainSession {
                session_id: SessionNodeId::from(session_id),
            },
            runtime_policy: vulcan_session_core::ExecutionRuntimePolicy::main_session(),
            execution_agent_id: Some(String::from("main")),
            agent_system_prompt_override: None,
            task_session_store_root: None,
            session_title: String::from("Parent session"),
            workspace_path: String::from("D:\\openvulcan\\test-project"),
            prompt: String::from("parent prompt"),
            conversation_state: SessionConversationState::default(),
            execution_mode: String::from("build"),
            temperature: None,
            reasoning_effort: ReasoningEffortLevel::High,
            ai_tool_configuration: PersistedAiToolConfiguration::default(),
            tool_exposure_override: ToolExposureSnapshot::default(),
            prompt_purpose: ExecutionPromptPurpose::UserTurn,
            project_prompt_skills: ProjectPromptSkillsConfiguration::default(),
            session_prompt_skill_overrides: BTreeMap::new(),
            context_injection: None,
            system_prompt_hook_blocks: Vec::new(),
            project_vmm: Default::default(),
            session_vmm: Default::default(),
        }
    }

    /// Builds one reusable TaskSession runtime context for queue-only tests.
    /// 为仅测试队列的用例构建一份可复用 TaskSession 运行时上下文。
    fn test_task_execution_runtime_context(session_id: SessionId) -> TaskExecutionRuntimeContext {
        let provider_config = ProviderConfigurationFile::default();
        let foundation = RuntimeFoundation::build_from_optional_mcp_access(
            &provider_config,
            &AgentProfileCatalog::default(),
            None,
            None,
        );
        TaskExecutionRuntimeContext::new(
            provider_config,
            Arc::new(foundation),
            None,
            test_parent_execution_request(session_id),
        )
    }

    /// Verifies child execution requests use the resolved TaskSession runtime policy exactly.
    /// 验证子代理执行请求精确使用已解析的 TaskSession 运行时策略。
    #[test]
    fn task_runtime_preparation_request_uses_resolved_runtime_policy() {
        let parent_session_id = Uuid::new_v4();
        let task_id = Uuid::new_v4();
        let provider_config = ProviderConfigurationFile::default();
        let foundation = RuntimeFoundation::build_from_optional_mcp_access(
            &provider_config,
            &AgentProfileCatalog::default(),
            None,
            None,
        );
        let context = TaskExecutionRuntimeContext::new(
            provider_config,
            Arc::new(foundation),
            None,
            test_parent_execution_request(parent_session_id),
        );
        let invocation = TaskExecutionRequest {
            current_session_id: parent_session_id,
            root_session_id: parent_session_id,
            task_id,
            public_task_id: String::from("agent-review"),
            prompt: String::from("review this change"),
            description: Some(String::from("Review code")),
            subagent: Some(String::from("reviewer")),
            invocation_config: TaskInvocationConfig::main("review this change"),
            display_title: String::from("Review code (reviewer)"),
            parent_task_id: None,
            depth: 1,
            snapshot: CoreConversationSnapshot::default(),
            controller_config: None,
            is_resumed: false,
        };
        let mut resolved_config = test_resolved_config(
            &build_test_controller_config("Base prompt"),
            &invocation.invocation_config,
        );
        resolved_config.runtime_policy = vulcan_session_core::ExecutionRuntimePolicy {
            hooks_enabled: false,
            vmm_enabled: true,
            memory_recall_enabled: false,
            memory_writeback_enabled: false,
            search_projection_enabled: false,
            database_projection_enabled: false,
        };

        let (_, child_request) =
            build_task_runtime_preparation_request(&context, &invocation, &resolved_config);

        assert_eq!(child_request.runtime_policy, resolved_config.runtime_policy);
        assert_eq!(
            child_request.execution_scope,
            ExecutionScope::TaskSession {
                root_session_id: SessionNodeId::from(parent_session_id),
                task_session_id: SessionNodeId::from(task_id),
            }
        );
    }

    /// Verifies child execution requests carry the composed TaskSession system prompt override.
    /// 验证子代理执行请求会携带已组合的 TaskSession 系统提示词覆盖。
    #[test]
    fn task_runtime_preparation_request_carries_controller_prompt_override() {
        let parent_session_id = Uuid::new_v4();
        let task_id = Uuid::new_v4();
        let provider_config = ProviderConfigurationFile::default();
        let foundation = RuntimeFoundation::build_from_optional_mcp_access(
            &provider_config,
            &AgentProfileCatalog::default(),
            None,
            None,
        );
        let context = TaskExecutionRuntimeContext::new(
            provider_config,
            Arc::new(foundation),
            None,
            test_parent_execution_request(parent_session_id),
        );
        let mut invocation = TaskExecutionRequest {
            current_session_id: parent_session_id,
            root_session_id: parent_session_id,
            task_id,
            public_task_id: String::from("agent-review"),
            prompt: String::from("review this change"),
            description: Some(String::from("Review code")),
            subagent: Some(String::from("reviewer")),
            invocation_config: TaskInvocationConfig::main("review this change"),
            display_title: String::from("Review code (reviewer)"),
            parent_task_id: None,
            depth: 1,
            snapshot: CoreConversationSnapshot::default(),
            controller_config: None,
            is_resumed: false,
        };
        let mut controller_config = build_test_controller_config("Base prompt");
        controller_config.system_prompt = String::from("Composed TaskSession prompt");
        invocation.controller_config = Some(controller_config);
        let resolved_config = test_resolved_config(
            &build_test_controller_config("Base prompt"),
            &invocation.invocation_config,
        );

        let (_, child_request) =
            build_task_runtime_preparation_request(&context, &invocation, &resolved_config);

        assert_eq!(
            child_request.agent_system_prompt_override.as_deref(),
            Some("Composed TaskSession prompt")
        );
    }

    /// Verifies that Windows child task prompts always include normalized filesystem path guidance.
    /// 验证 Windows 子任务提示词始终包含规范化文件系统路径指引。
    #[test]
    #[cfg(windows)]
    fn build_task_controller_config_appends_windows_path_guidance() {
        let base = build_test_controller_config("Base prompt");
        let description = Some(String::from("Analyze the current repository."));
        let subagent = Some(String::from("explorer"));
        let invocation_config = TaskInvocationConfig::main("Analyze the current repository.");
        let resolved_config = test_resolved_config(&base, &invocation_config);

        let config = build_task_controller_config(
            &base,
            None,
            &description,
            &subagent,
            &invocation_config,
            &resolved_config,
        );

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
        let invocation_config = TaskInvocationConfig::main("Analyze the current repository.");
        let resolved_config = test_resolved_config(&base, &invocation_config);
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
            &invocation_config,
            &resolved_config,
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
        let invocation_config = TaskInvocationConfig::main("Analyze the current repository.");
        let resolved_config = test_resolved_config(&base, &invocation_config);

        let config = build_task_controller_config(
            &base,
            None,
            &description,
            &subagent,
            &invocation_config,
            &resolved_config,
        );

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

    /// Verifies invocation-level system prompt overrides are routed through the clone composer.
    /// 验证调用级系统提示词覆盖会通过分身组合器路由。
    #[test]
    fn build_task_controller_config_uses_invocation_system_prompt() {
        let base = build_test_controller_config("Base prompt");
        let description = Some(String::from("Review code"));
        let subagent = Some(String::from("reviewer"));
        let mut invocation_config =
            TaskInvocationConfig::with_profile("agent-review", "Review code");
        invocation_config.inherit_parent_config = Some(false);
        invocation_config.system_prompt = Some(String::from("You are a strict code reviewer."));
        invocation_config.system_prompt_append = Some(String::from("Focus only on correctness."));
        let resolved_config = test_resolved_config(&base, &invocation_config);

        let config = build_task_controller_config(
            &base,
            None,
            &description,
            &subagent,
            &invocation_config,
            &resolved_config,
        );

        assert!(!config.system_prompt.contains("Base prompt"));
        assert!(
            config
                .system_prompt
                .contains("You are a strict code reviewer.")
        );
        assert!(config.system_prompt.contains("Focus only on correctness."));
        assert!(config.system_prompt.contains("Subagent identity: reviewer"));
    }

    /// Verifies flexible task arguments populate the shared TaskSession invocation config.
    /// 验证灵活 task 参数会填充共享 TaskSession 调用配置。
    #[test]
    fn task_invocation_config_parses_profile_tool_skill_and_compaction_overrides() {
        let mut args = test_task_args("review code", None);
        args.subagent_profile_id = Some(String::from("agent-review"));
        args.provider_model = Some(json!({
            "provider": "deepseek",
            "model": "deepseek-v4",
            "reasoning": "high",
            "mode": "Plan"
        }));
        args.tools = Some(json!({
            "allowed": ["shell_command", "vulcan_file_read"],
            "disabled": ["vmm_memory_write"]
        }));
        args.skills = Some(json!({
            "enabled": ["review.skill"],
            "disabled": ["draft.skill"]
        }));
        args.luaskills = Some(json!({ "enabled": ["vulcan-codekit"] }));
        args.mcp = Some(json!({ "servers": ["local-dev"] }));
        args.context_compaction = Some(json!({
            "context_budget": 12000,
            "max_provider_requests": 4,
            "auto_compaction_enabled": true
        }));
        args.runtime_policy = Some(json!({
            "vmmEnabled": true,
            "memoryRecallEnabled": true,
            "databaseProjectionEnabled": true
        }));
        args.extra = Some(json!({ "temperature": 0.1, "max_turns": 2 }));

        let config = task_invocation_config_from_args(&args);

        assert_eq!(config.subagent_profile_id.as_deref(), Some("agent-review"));
        assert_eq!(
            config.provider_model.provider_id.as_deref(),
            Some("deepseek")
        );
        assert_eq!(
            config.provider_model.model_id.as_deref(),
            Some("deepseek-v4")
        );
        assert_eq!(
            config.provider_model.reasoning_effort.as_deref(),
            Some("high")
        );
        assert_eq!(
            config.provider_model.execution_mode.as_deref(),
            Some("Plan")
        );
        assert_eq!(
            config.tool_exposure.allowed_tools,
            vec!["shell_command", "vulcan_file_read"]
        );
        assert_eq!(
            config.tool_exposure.disabled_tools,
            vec!["vmm_memory_write"]
        );
        assert_eq!(config.prompt_skills.enabled, vec!["review.skill"]);
        assert_eq!(config.prompt_skills.disabled, vec!["draft.skill"]);
        assert_eq!(config.context_compaction.context_budget, Some(12000));
        assert_eq!(config.context_compaction.max_provider_requests, Some(4));
        assert_eq!(
            config.context_compaction.auto_compaction_enabled,
            Some(true)
        );
        assert_eq!(config.runtime_policy.vmm_enabled, Some(true));
        assert_eq!(config.runtime_policy.memory_recall_enabled, Some(true));
        assert_eq!(
            config.runtime_policy.database_projection_enabled,
            Some(true)
        );
        assert_eq!(config.extra["temperature"], json!(0.1));
    }

    /// Verifies TaskSession compaction planning follows the resolved automatic-compaction toggle.
    /// 验证 TaskSession 压缩规划会跟随已解析的自动压缩开关。
    #[test]
    fn task_context_compaction_policy_follows_resolved_toggle() {
        let mut snapshot = ContextCompactionPolicySnapshot::default();
        snapshot.auto_compaction_enabled = false;

        let disabled_policy = task_context_compaction_policy_from_snapshot(&snapshot);

        assert!(!disabled_policy.auto);
        assert!(disabled_policy.prune);

        snapshot.auto_compaction_enabled = true;
        let enabled_policy = task_context_compaction_policy_from_snapshot(&snapshot);

        assert!(enabled_policy.auto);
        assert_eq!(
            enabled_policy.tail_turns,
            ContextCompactionPolicy::default().tail_turns
        );
    }

    /// Verifies the task tool schema supports profile-based invocation without requiring a separate subagent label.
    /// 验证 task 工具 schema 支持基于 profile 的调用，而不再强制要求单独的子代理标签。
    #[test]
    fn task_tool_schema_allows_profile_without_subagent_label() {
        let schema = task_tool_args_schema();
        let required = schema["required"]
            .as_array()
            .expect("required fields should be an array")
            .iter()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>();
        let mut args = test_task_args("review code", None);
        args.profile = Some(String::from("agent-review"));

        let config = task_invocation_config_from_args(&args);

        assert_eq!(required, vec!["prompt"]);
        assert!(
            schema["properties"]["task_id"]["description"]
                .as_str()
                .is_some_and(|description| description.contains("inspect")
                    && description.contains("resume the same subtask")
                    && description.contains("status inspection")
                    && description.contains("parent thread"))
        );
        assert!(RUNNING_TASK_RESULT_SUMMARY.contains("not the subagent's final result"));
        assert!(RUNNING_TASK_RESULT_SUMMARY.contains("non-task tool calls"));
        assert!(RUNNING_TASK_RESULT_SUMMARY.contains("same task_id"));
        assert_eq!(config.subagent_profile_id.as_deref(), Some("agent-review"));
    }

    /// Verifies runtime-registered profiles enter the shared TaskSession config resolver.
    /// 验证运行时注册的 profile 会进入共享 TaskSession 配置解析器。
    #[test]
    fn task_runtime_resolves_registered_subagent_profile() {
        let runtime = TaskRuntime::new();
        let base = build_test_controller_config("Base prompt");
        let mut profile = SubagentProfile::new("agent-review", "Review Agent");
        profile.base_system_prompt = Some(String::from("Profile reviewer prompt."));
        profile.tool_exposure.allowed_tools = vec![String::from("shell_command")];
        runtime
            .configure_subagent_profiles(vec![profile])
            .expect("profiles should configure");
        let invocation = TaskInvocationConfig::with_profile("agent-review", "review code");

        let resolved = runtime
            .resolve_task_session_config_for_execution(&base, &invocation)
            .expect("profile should resolve");

        assert_eq!(
            resolved.effective_config.subagent_profile_id.as_deref(),
            Some("agent-review")
        );
        assert!(
            resolved
                .effective_config
                .system_prompt_layers
                .iter()
                .any(|layer| layer.contains("Profile reviewer prompt."))
        );
        assert_eq!(
            resolved.effective_config.tool_exposure.allowed_tools,
            vec!["shell_command"]
        );
    }

    /// Verifies shared-runtime task report conversion preserves the final assistant message id from the child transcript.
    /// 验证共享运行时任务报告转换会保留子代理转录中的最终助手消息标识。
    #[test]
    fn task_execution_outcome_from_report_preserves_final_assistant_message_id() {
        let root_session_id = Uuid::new_v4();
        let task_id = Uuid::new_v4();
        let snapshot = CoreConversationSnapshot {
            messages: vec![vulcan_core::types::ConversationMessage {
                id: String::from("assistant-final"),
                payload: vulcan_core::types::ConversationPayload::Assistant {
                    text: String::from("done"),
                    reasoning: String::new(),
                    provider_message_id: None,
                    provider_message_state: None,
                    usage: None,
                    tool_calls: Vec::new(),
                },
            }],
            records: Vec::new(),
            continuation: Default::default(),
        };
        let invocation = TaskExecutionRequest {
            current_session_id: root_session_id,
            root_session_id,
            task_id,
            public_task_id: String::from("agent-review"),
            prompt: String::from("review this change"),
            description: Some(String::from("Review code")),
            subagent: Some(String::from("reviewer")),
            invocation_config: TaskInvocationConfig::main("review this change"),
            display_title: String::from("Review code (reviewer)"),
            parent_task_id: None,
            depth: 1,
            snapshot: CoreConversationSnapshot::default(),
            controller_config: None,
            is_resumed: false,
        };
        let controller_config = build_test_controller_config("Base prompt");
        let resolved_config =
            test_resolved_config(&controller_config, &invocation.invocation_config);
        let report = ExecutionRunReport {
            session_id: task_id,
            preparation: None,
            assistant_output: Some(String::from("done")),
            thinking_output: None,
            conversation_state: Some(core_snapshot_to_session_conversation_state(&snapshot)),
            provider_request_count: 1,
            last_successful_provider_usage: None,
            mcp_discovery_rows: Vec::new(),
            mcp_discovery_errors: Vec::new(),
            warnings: Vec::new(),
            error: None,
            controller_events: Vec::new(),
        };

        let outcome = task_execution_outcome_from_report(
            &invocation,
            controller_config,
            resolved_config,
            report,
        )
        .expect("shared-runtime report should convert");

        assert_eq!(
            outcome.final_assistant_message_id.as_deref(),
            Some("assistant-final")
        );
        assert_eq!(outcome.final_output_text, "done");
    }

    /// Verifies the task backend fails loudly when the shared runtime bridge did not register a parent execution context.
    /// 验证共享运行时桥未注册父级执行上下文时，任务后端会明确失败。
    #[tokio::test]
    async fn build_task_execution_handler_rejects_missing_shared_context() {
        let task_runtime = Arc::new(TaskRuntime::new());
        let root_session_id = Uuid::new_v4();
        let task_id = Uuid::new_v4();
        let (sender, _receiver) = unbounded_channel::<Result<ProviderStreamDelta, String>>();
        let handler = build_task_execution_handler(
            Arc::clone(&task_runtime),
            build_test_controller_config("Base prompt"),
            sender,
        );
        let invocation = TaskExecutionRequest {
            current_session_id: root_session_id,
            root_session_id,
            task_id,
            public_task_id: String::from("agent-review"),
            prompt: String::from("review this change"),
            description: Some(String::from("Review code")),
            subagent: Some(String::from("reviewer")),
            invocation_config: TaskInvocationConfig::main("review this change"),
            display_title: String::from("Review code (reviewer)"),
            parent_task_id: None,
            depth: 1,
            snapshot: CoreConversationSnapshot::default(),
            controller_config: None,
            is_resumed: false,
        };

        let error = handler(invocation)
            .await
            .expect_err("missing shared context should fail");

        assert!(error.contains("shared runtime execution context"));
        assert!(error.contains("agent-review"));
    }

    /// Verifies parent TaskSession follow-up queues keep completion order and ignore duplicate child completions.
    /// 验证父级 TaskSession 跟进队列会保留完成顺序并忽略重复子任务完成事件。
    #[test]
    fn task_session_parent_followup_queue_preserves_order_and_deduplicates() {
        let task_runtime = TaskRuntime::new();
        let root_session_id = Uuid::new_v4();
        let parent_task_id = Uuid::new_v4();
        let first_child_id = Uuid::new_v4();
        let second_child_id = Uuid::new_v4();
        let context = test_task_execution_runtime_context(root_session_id);

        let first = TaskSessionParentFollowup {
            root_session_id,
            child_task_id: first_child_id,
            context: context.clone(),
            prompt: String::from("first"),
        };
        let duplicate = TaskSessionParentFollowup {
            root_session_id,
            child_task_id: first_child_id,
            context: context.clone(),
            prompt: String::from("duplicate"),
        };
        let second = TaskSessionParentFollowup {
            root_session_id,
            child_task_id: second_child_id,
            context,
            prompt: String::from("second"),
        };

        assert!(
            task_runtime
                .queue_task_session_parent_followup(parent_task_id, first)
                .expect("first enqueue should succeed")
        );
        assert!(
            !task_runtime
                .queue_task_session_parent_followup(parent_task_id, duplicate)
                .expect("duplicate enqueue should be ignored")
        );
        assert!(
            task_runtime
                .queue_task_session_parent_followup(parent_task_id, second)
                .expect("second enqueue should succeed")
        );

        let popped_first = task_runtime
            .pop_task_session_parent_followup(parent_task_id)
            .expect("first dequeue")
            .expect("first follow-up");
        let popped_second = task_runtime
            .pop_task_session_parent_followup(parent_task_id)
            .expect("second dequeue")
            .expect("second follow-up");
        let empty = task_runtime
            .pop_task_session_parent_followup(parent_task_id)
            .expect("empty dequeue");

        assert_eq!(popped_first.child_task_id, first_child_id);
        assert_eq!(popped_first.prompt, "first");
        assert_eq!(popped_second.child_task_id, second_child_id);
        assert_eq!(popped_second.prompt, "second");
        assert!(empty.is_none());
    }

    /// Verifies parent TaskSession follow-up launch reservation prevents concurrent starts for the same parent.
    /// 验证父级 TaskSession 跟进启动预留会阻止同一父任务并发启动。
    #[test]
    fn task_session_parent_followup_launch_reservation_blocks_parallel_starts() {
        let task_runtime = TaskRuntime::new();
        let parent_task_id = Uuid::new_v4();

        assert!(
            task_runtime
                .begin_task_session_parent_followup_launch(parent_task_id)
                .expect("first launch reservation should succeed")
        );
        assert!(
            !task_runtime
                .begin_task_session_parent_followup_launch(parent_task_id)
                .expect("second launch reservation should be blocked")
        );
        assert!(
            task_runtime
                .is_task_running(parent_task_id)
                .expect("starting parent should be reported as running")
        );

        task_runtime.finish_task_session_parent_followup_launch(parent_task_id);
        assert!(
            !task_runtime
                .is_task_running(parent_task_id)
                .expect("released parent should no longer be running")
        );

        assert!(
            task_runtime
                .begin_task_session_parent_followup_launch(parent_task_id)
                .expect("released launch reservation should allow a new start")
        );
        task_runtime.finish_task_session_parent_followup_launch(parent_task_id);
    }

    /// Verifies stopping a TaskSession tree cancels a follow-up launch that is still in its starting window.
    /// 验证停止 TaskSession 树会取消仍处于启动窗口中的跟进运行。
    #[test]
    fn stop_task_tree_cancels_starting_parent_followup_launch() {
        let task_runtime = TaskRuntime::new();
        let parent_task_id = Uuid::new_v4();

        assert!(
            task_runtime
                .begin_task_session_parent_followup_launch(parent_task_id)
                .expect("launch reservation should succeed")
        );

        let stopped = task_runtime
            .stop_task_tree(parent_task_id)
            .expect("starting parent should be stoppable");

        assert_eq!(stopped, vec![parent_task_id]);
        assert!(
            !task_runtime
                .is_task_running(parent_task_id)
                .expect("stopped starting parent should not remain running")
        );
    }

    /// Verifies parent TaskSession follow-up requests preserve the parent's direct parent route.
    /// 验证父级 TaskSession 跟进请求会保留父任务自己的直接父级路由。
    #[test]
    fn task_session_parent_followup_request_preserves_direct_parent_route() {
        let task_runtime = TaskRuntime::new();
        let root_session_id = Uuid::new_v4();
        let grandparent_task_id = Uuid::new_v4();
        let parent_task_id = Uuid::new_v4();
        let controller_config = build_test_controller_config("Parent task prompt");
        task_runtime
            .sessions
            .write()
            .expect("task session registry should be writable")
            .insert(
                parent_task_id,
                build_task_session_record(
                    parent_task_id,
                    String::from("parent"),
                    root_session_id,
                    Some(grandparent_task_id),
                    2,
                    Some(String::from("Parent task")),
                    Some(String::from("coordinator")),
                    TaskInvocationConfig::main("original parent prompt"),
                    String::from("Parent task (coordinator)"),
                    Some(controller_config.clone()),
                    CoreConversationSnapshot::default(),
                    String::from("original parent prompt"),
                    String::from("completed"),
                ),
            );

        let (request, resolved_config) = task_runtime
            .build_task_session_parent_followup_request(
                parent_task_id,
                root_session_id,
                String::from("child finished"),
            )
            .expect("follow-up request should be restored");

        assert_eq!(request.current_session_id, grandparent_task_id);
        assert_eq!(request.root_session_id, root_session_id);
        assert_eq!(request.task_id, parent_task_id);
        assert_eq!(request.parent_task_id, Some(grandparent_task_id));
        assert_eq!(request.prompt, "child finished");
        assert!(request.is_resumed);
        assert_eq!(
            request
                .controller_config
                .as_ref()
                .expect("controller config")
                .system_prompt,
            controller_config.system_prompt
        );
        assert!(!resolved_config.runtime_policy.search_projection_enabled);
        assert!(!resolved_config.runtime_policy.database_projection_enabled);
    }

    /// Verifies TaskRuntime builds parent follow-up prompts through the shared subagent delegate prompt helper.
    /// 验证 TaskRuntime 会通过共享子代理委派提示词 helper 构建父级跟进提示词。
    #[test]
    fn task_session_parent_followup_prompt_reuses_shared_delegate_format() {
        let root_session_id = Uuid::new_v4();
        let parent_task_id = Uuid::new_v4();
        let child_task_id = Uuid::new_v4();
        let request = TaskExecutionRequest {
            current_session_id: parent_task_id,
            root_session_id,
            task_id: child_task_id,
            public_task_id: String::from("child-a"),
            prompt: String::from("produce child result"),
            description: Some(String::from("Child work")),
            subagent: Some(String::from("worker")),
            invocation_config: TaskInvocationConfig::main("produce child result"),
            display_title: String::from("Child work (worker)"),
            parent_task_id: Some(parent_task_id),
            depth: 2,
            snapshot: CoreConversationSnapshot::default(),
            controller_config: Some(build_test_controller_config("Child prompt")),
            is_resumed: false,
        };

        let state = task_session_parent_followup_state(
            &request,
            SubagentTaskRunState::Completed,
            String::from("child result"),
        );
        let prompt = build_delegate_completion_followup_prompt(&state);

        assert!(prompt.contains("<system-reminder>"));
        assert!(prompt.contains("子代理“Child work (worker)”"));
        assert!(prompt.contains("已完成"));
        assert!(prompt.contains("任务标识：child-a。"));
        assert!(prompt.contains("child result"));
        assert!(prompt.contains("不是用户的新需求"));
        assert!(prompt.contains("不是让主线程补做任务的指令"));
        assert!(prompt.contains("不要重复启动同一个子代理"));
        assert!(prompt.contains("携带原 task_id 查询对应任务状态"));
        assert!(prompt.contains("其他非 task 工具"));
        assert!(prompt.contains("不能假设其结果"));
    }

    /// Verifies stopping one task aborts its active descendant task tree without touching sibling tasks.
    /// 验证停止单个任务会中止其活动后代任务树，并且不会影响旁支任务。
    #[tokio::test]
    async fn stop_task_tree_aborts_active_descendants_without_siblings() {
        let task_runtime = TaskRuntime::new();
        let root_session_id = Uuid::new_v4();
        let parent_task_id = Uuid::new_v4();
        let child_task_id = Uuid::new_v4();
        let grandchild_task_id = Uuid::new_v4();
        let sibling_task_id = Uuid::new_v4();

        {
            let mut sessions = task_runtime
                .sessions
                .write()
                .expect("task session registry should be writable");
            sessions.insert(
                parent_task_id,
                build_task_session_record(
                    parent_task_id,
                    String::from("parent"),
                    root_session_id,
                    None,
                    1,
                    None,
                    None,
                    TaskInvocationConfig::main("parent prompt"),
                    String::from("parent"),
                    None,
                    CoreConversationSnapshot::default(),
                    String::from("parent prompt"),
                    String::from("running"),
                ),
            );
            sessions.insert(
                child_task_id,
                build_task_session_record(
                    child_task_id,
                    String::from("child"),
                    root_session_id,
                    Some(parent_task_id),
                    2,
                    None,
                    None,
                    TaskInvocationConfig::main("child prompt"),
                    String::from("child"),
                    None,
                    CoreConversationSnapshot::default(),
                    String::from("child prompt"),
                    String::from("running"),
                ),
            );
            sessions.insert(
                grandchild_task_id,
                build_task_session_record(
                    grandchild_task_id,
                    String::from("grandchild"),
                    root_session_id,
                    Some(child_task_id),
                    3,
                    None,
                    None,
                    TaskInvocationConfig::main("grandchild prompt"),
                    String::from("grandchild"),
                    None,
                    CoreConversationSnapshot::default(),
                    String::from("grandchild prompt"),
                    String::from("running"),
                ),
            );
            sessions.insert(
                sibling_task_id,
                build_task_session_record(
                    sibling_task_id,
                    String::from("sibling"),
                    root_session_id,
                    None,
                    1,
                    None,
                    None,
                    TaskInvocationConfig::main("sibling prompt"),
                    String::from("sibling"),
                    None,
                    CoreConversationSnapshot::default(),
                    String::from("sibling prompt"),
                    String::from("running"),
                ),
            );
        }

        let parent_join = tokio::spawn(async { std::future::pending::<()>().await });
        let child_join = tokio::spawn(async { std::future::pending::<()>().await });
        let grandchild_join = tokio::spawn(async { std::future::pending::<()>().await });
        let sibling_join = tokio::spawn(async { std::future::pending::<()>().await });
        {
            let mut active_runs = task_runtime
                .active_task_runs
                .write()
                .expect("active task registry should be writable");
            active_runs.insert(parent_task_id, parent_join.abort_handle());
            active_runs.insert(child_task_id, child_join.abort_handle());
            active_runs.insert(grandchild_task_id, grandchild_join.abort_handle());
            active_runs.insert(sibling_task_id, sibling_join.abort_handle());
        }

        let stopped_task_ids = task_runtime
            .stop_task_tree(parent_task_id)
            .expect("task tree should stop");
        let stopped_task_ids = stopped_task_ids.into_iter().collect::<BTreeSet<_>>();

        assert_eq!(
            stopped_task_ids,
            BTreeSet::from([parent_task_id, child_task_id, grandchild_task_id])
        );
        assert!(
            parent_join
                .await
                .expect_err("parent task should be aborted")
                .is_cancelled()
        );
        assert!(
            child_join
                .await
                .expect_err("child task should be aborted")
                .is_cancelled()
        );
        assert!(
            grandchild_join
                .await
                .expect_err("grandchild task should be aborted")
                .is_cancelled()
        );
        assert!(!sibling_join.is_finished());
        sibling_join.abort();
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
            &test_task_args("count to 10", Some("count_10")),
        )
        .expect("first alias task id should resolve");
        let second = resolve_task_session_id(
            &task_runtime,
            parent_session_id,
            &test_task_args("count to 20", Some("count_20")),
        )
        .expect("second alias task id should resolve");
        let third = resolve_task_session_id(
            &task_runtime,
            parent_session_id,
            &test_task_args("count to 30", Some("count_30")),
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
            &test_task_args("inspect readme", Some("research_readme")),
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
                    TaskInvocationConfig::main("inspect readme"),
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
            &test_task_args("inspect readme again", Some("research_readme")),
        )
        .expect("existing alias task id should resume");

        assert_eq!(resumed.task_id, first.task_id);
        assert_eq!(resumed.public_task_id, first.public_task_id);
        assert!(resumed.is_resumed);
    }

    /// Verifies file-backed task metadata and context are used when memory has no matching task record.
    /// 验证内存没有匹配任务记录时会使用文件态任务元数据与上下文。
    #[test]
    fn prepare_task_execution_request_restores_file_backed_context_for_resume() {
        let temp_dir = tempfile::tempdir().expect("temp task-session root");
        let task_runtime = Arc::new(TaskRuntime::new());
        let root_session_id = Uuid::new_v4();
        let task_id = Uuid::new_v4();
        task_runtime
            .configure_task_session_store_root(root_session_id, temp_dir.path().to_path_buf())
            .expect("store root should configure");
        let store = FileTaskSessionStore::new(temp_dir.path());
        let parent = ParentSessionRef::MainSession {
            root_session_id: SessionNodeId::from(root_session_id),
        };
        let mut requested_invocation = TaskInvocationConfig::main("initial prompt");
        requested_invocation.system_prompt = Some(String::from("Stored system prompt"));
        let resolved_config =
            TaskSessionConfigResolver::resolve(TaskSessionConfigResolutionInput {
                global_default_config: SessionEffectiveConfig::default(),
                parent_config: None,
                profile: None,
                invocation: requested_invocation,
            });
        let mut metadata = TaskSessionMetadata::new(
            SessionNodeId::from(root_session_id),
            SessionNodeId::from(task_id),
            parent.clone(),
            "agent-review",
            "Agent review",
            resolved_config.requested_config,
            resolved_config.effective_config,
            1,
            100,
        );
        metadata.status = TaskSessionStatus::Completed;
        store
            .create_task_session(metadata.clone())
            .expect("task metadata should store");
        let conversation_state =
            core_snapshot_to_session_conversation_state(&CoreConversationSnapshot::default());
        store
            .save_context(SessionContextDocument {
                schema_version: 1,
                node_id: SessionNodeId::from(task_id),
                provider_id: Some(String::from("file-provider")),
                model_id: Some(String::from("file-model")),
                reasoning_effort: None,
                system_prompt_layers: vec![String::from("File system prompt")],
                conversation_state: serde_json::to_value(conversation_state)
                    .expect("conversation state should serialize"),
                compaction_anchor: None,
                context_budget_fingerprint: None,
                tool_catalog_snapshot: Value::Null,
                provider_request_preview: None,
            })
            .expect("task context should store");

        let args = test_task_args("continue the review", Some("agent-review"));
        let resolved_identifier = resolve_task_session_id(&task_runtime, root_session_id, &args)
            .expect("file-backed task id should resolve");
        assert!(resolved_identifier.is_resumed);
        assert_eq!(resolved_identifier.task_id, task_id);

        let request = prepare_task_execution_request(
            &task_runtime,
            root_session_id,
            resolved_identifier,
            args,
        )
        .expect("file-backed task should prepare");
        let controller_config = request
            .controller_config
            .expect("file-backed controller config should restore");
        assert_eq!(controller_config.provider_id, "file-provider");
        assert_eq!(controller_config.model_id, "file-model");
        assert_eq!(controller_config.system_prompt, "File system prompt");
        assert_eq!(request.task_id, task_id);
        assert!(request.is_resumed);
    }

    /// Verifies stopped-task fallback snapshots can be restored from the file-backed TaskSession context.
    /// 验证停止任务的回退快照可从文件态 TaskSession 上下文恢复。
    #[test]
    fn current_task_snapshot_from_store_reads_file_backed_context() {
        let temp_dir = tempfile::tempdir().expect("temp task-session root");
        let task_runtime = TaskRuntime::new();
        let root_session_id = Uuid::new_v4();
        let task_id = Uuid::new_v4();
        task_runtime
            .configure_task_session_store_root(root_session_id, temp_dir.path().to_path_buf())
            .expect("store root should configure");
        let store = FileTaskSessionStore::new(temp_dir.path());
        let parent = ParentSessionRef::MainSession {
            root_session_id: SessionNodeId::from(root_session_id),
        };
        let metadata = TaskSessionMetadata::new(
            SessionNodeId::from(root_session_id),
            SessionNodeId::from(task_id),
            parent,
            "agent-review",
            "Agent review",
            SessionRequestedConfig::default(),
            SessionEffectiveConfig::default(),
            1,
            100,
        );
        store
            .create_task_session(metadata)
            .expect("task metadata should store");
        let snapshot = CoreConversationSnapshot {
            messages: vec![vulcan_core::types::ConversationMessage {
                id: String::from("assistant-from-file"),
                payload: vulcan_core::types::ConversationPayload::Assistant {
                    text: String::from("file-backed answer"),
                    reasoning: String::new(),
                    provider_message_id: None,
                    provider_message_state: None,
                    usage: None,
                    tool_calls: Vec::new(),
                },
            }],
            records: Vec::new(),
            continuation: Default::default(),
        };
        store
            .save_context(SessionContextDocument {
                schema_version: 1,
                node_id: SessionNodeId::from(task_id),
                provider_id: Some(String::from("provider")),
                model_id: Some(String::from("model")),
                reasoning_effort: None,
                system_prompt_layers: vec![String::from("File system prompt")],
                conversation_state: serde_json::to_value(
                    core_snapshot_to_session_conversation_state(&snapshot),
                )
                .expect("conversation state should serialize"),
                compaction_anchor: Some(String::from("task-summary-existing")),
                context_budget_fingerprint: Some(String::from("budget-existing")),
                tool_catalog_snapshot: Value::Null,
                provider_request_preview: None,
            })
            .expect("task context should store");

        let loaded = current_task_snapshot_from_store(&task_runtime, root_session_id, task_id)
            .expect("file-backed snapshot should load");

        assert_eq!(loaded.messages.len(), 1);
        assert_eq!(loaded.messages[0].id, "assistant-from-file");
    }

    /// Verifies task outcomes materialize file-backed TaskSession artifacts when a store root is configured.
    /// 验证配置存储根目录后，任务结果会物化文件态 TaskSession 产物。
    #[test]
    fn store_task_outcome_persists_file_backed_task_session_artifacts() {
        let temp_dir = tempfile::tempdir().expect("temp task-session root");
        let task_runtime = TaskRuntime::new();
        let root_session_id = Uuid::new_v4();
        let task_id = Uuid::new_v4();
        task_runtime
            .configure_task_session_store_root(root_session_id, temp_dir.path().to_path_buf())
            .expect("store root should configure");

        let invocation = TaskExecutionRequest {
            current_session_id: root_session_id,
            root_session_id,
            task_id,
            public_task_id: String::from("agent-review"),
            prompt: String::from("review this change"),
            description: Some(String::from("Review code")),
            subagent: Some(String::from("reviewer")),
            invocation_config: TaskInvocationConfig::main("review this change"),
            display_title: String::from("Review code (reviewer)"),
            parent_task_id: None,
            depth: 1,
            snapshot: CoreConversationSnapshot::default(),
            controller_config: None,
            is_resumed: false,
        };
        let controller_config = build_test_controller_config("Base prompt");
        let resolved_config =
            test_resolved_config(&controller_config, &invocation.invocation_config);
        let outcome = TaskExecutionOutcome {
            task_id,
            controller_config: controller_config.clone(),
            resolved_config,
            snapshot: CoreConversationSnapshot::default(),
            provider_turns_executed: 1,
            tool_calls_processed: 0,
            final_assistant_message_id: Some(String::from("assistant-1")),
            final_output_text: String::from("review complete"),
            execution_status: TaskExecutionStatus::Completed,
        };

        task_runtime
            .store_task_outcome(root_session_id, &invocation, &outcome)
            .expect("task outcome should store");

        let store = FileTaskSessionStore::new(temp_dir.path());
        let metadata = store
            .load_task_session(SessionNodeId::from(task_id))
            .expect("metadata should load")
            .expect("metadata should exist");
        assert_eq!(
            metadata.root_session_id,
            SessionNodeId::from(root_session_id)
        );
        assert_eq!(metadata.status, TaskSessionStatus::Completed);
        assert_eq!(metadata.public_task_id, "agent-review");
        assert_eq!(
            metadata.last_result_summary.as_deref(),
            Some("review complete")
        );

        let mut context = store
            .load_context(SessionNodeId::from(task_id))
            .expect("context should load")
            .expect("context should exist");
        assert_eq!(context.provider_id.as_deref(), Some("provider"));
        assert_eq!(context.model_id.as_deref(), Some("model"));
        context.compaction_anchor = Some(String::from("task-summary-existing"));
        context.context_budget_fingerprint = Some(String::from("budget-existing"));
        store
            .save_context(context)
            .expect("existing compacted context should store");

        let second_outcome = TaskExecutionOutcome {
            final_output_text: String::from("review complete again"),
            ..outcome.clone()
        };
        task_runtime
            .store_task_outcome(root_session_id, &invocation, &second_outcome)
            .expect("second task outcome should store");
        let preserved_context = store
            .load_context(SessionNodeId::from(task_id))
            .expect("preserved context should load")
            .expect("preserved context should exist");
        assert_eq!(
            preserved_context.compaction_anchor.as_deref(),
            Some("task-summary-existing")
        );
        assert_eq!(
            preserved_context.context_budget_fingerprint.as_deref(),
            Some("budget-existing")
        );

        let task_events = store
            .load_events(SessionNodeId::from(task_id))
            .expect("task events should load");
        assert!(
            task_events
                .iter()
                .any(|event| event.kind == SessionEventKind::TaskFeedback)
        );
        let parent_events = store
            .list_task_invocation_events(ParentSessionRef::MainSession {
                root_session_id: SessionNodeId::from(root_session_id),
            })
            .expect("parent task events should load");
        assert!(
            parent_events
                .iter()
                .any(|event| event.kind == TaskInvocationEventKind::TaskCompleted)
        );
    }

    /// Verifies parent task metadata points at the TaskSession instead of embedding the child transcript.
    /// 验证父级 task 元数据只指向 TaskSession，而不会内嵌子代理完整转录。
    #[test]
    fn task_invocation_result_omits_embedded_conversation_state() {
        let root_session_id = Uuid::new_v4();
        let task_id = Uuid::new_v4();
        let invocation = TaskExecutionRequest {
            current_session_id: root_session_id,
            root_session_id,
            task_id,
            public_task_id: String::from("agent-review"),
            prompt: String::from("review this change"),
            description: Some(String::from("Review code")),
            subagent: Some(String::from("reviewer")),
            invocation_config: TaskInvocationConfig::main("review this change"),
            display_title: String::from("Review code (reviewer)"),
            parent_task_id: None,
            depth: 1,
            snapshot: CoreConversationSnapshot::default(),
            controller_config: None,
            is_resumed: false,
        };
        let outcome = TaskExecutionOutcome {
            task_id,
            controller_config: build_test_controller_config("Base prompt"),
            resolved_config: test_resolved_config(
                &build_test_controller_config("Base prompt"),
                &invocation.invocation_config,
            ),
            snapshot: CoreConversationSnapshot::default(),
            provider_turns_executed: 1,
            tool_calls_processed: 0,
            final_assistant_message_id: Some(String::from("assistant-1")),
            final_output_text: String::from("review complete"),
            execution_status: TaskExecutionStatus::Completed,
        };

        let result = build_task_invocation_result(&invocation, &outcome);
        let host_result = result.host_result.expect("task host result");

        assert_eq!(host_result.kind, "task_session");
        assert_eq!(host_result.payload["task_session_id"], task_id.to_string());
        assert!(
            host_result.payload.get("conversation_state").is_none(),
            "new parent task metadata must not embed child transcripts"
        );
    }
}
