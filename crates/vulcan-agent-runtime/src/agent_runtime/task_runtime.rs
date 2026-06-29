//! Resumable task-session runtime for host-managed task_id chaining and nested subtask execution.
//! 供宿主管理 task_id 续跑与嵌套子任务执行使用的可恢复任务会话运行时模块。

use crate::agent_runtime::events::{ExecutionStreamEvent, RuntimeEvent, RuntimeEventKind};
use crate::agent_runtime::execution_models::{ExecutionPreparationRequest, ExecutionRunReport};
use crate::agent_runtime::execution_preparation::prepare_execution_with_tool_snapshot;
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
use crate::agent_runtime::tool_runtime_bridge::{
    attach_runtime_event_projection_barrier, wait_runtime_event_projection_barrier,
};
use crate::conversation_protocol::SessionConversationState;
use crate::luaskills_runtime::LuaSkillsRuntimeHandle;
use crate::model::{SessionId, SessionTurnUsageSegment, SessionTurnUsageSegmentKind};
use crate::provider_config::{PersistedPromptSkillState, ProviderConfigurationFile};
use async_stream::stream;
use futures_util::StreamExt;
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::VecDeque;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::{Arc, RwLock};
use std::time::Duration;
use time::OffsetDateTime;
use tokio::sync::{
    mpsc::{UnboundedSender, unbounded_channel},
    watch,
};
use tokio::task::AbortHandle;
use uuid::Uuid;
use vulcan_context::{ContextCompactionPolicy, conversation_state_to_compaction_source_messages};
use vulcan_context_compaction::{CompactionPlanInput, plan_context_compaction};
use vulcan_core::types::{
    ControllerConfig as CoreControllerConfig, ConversationPayload as CoreConversationPayload,
    ConversationSnapshot as CoreConversationSnapshot,
    ProviderPromptLayers as CoreProviderPromptLayers,
};
use vulcan_prompt_composer::{
    ModelDescriptor, PersonalitySelection, PromptAssemblyInput, PromptAssemblyMode,
    PromptAssemblyOutput, ProviderPromptSnapshot, SubagentSnapshot, ToolCatalogSnapshot,
};
use vulcan_session_boundary::{
    InMemorySubagentMailbox, SessionThreadId, SubagentMailbox, SubagentMailboxAgentKind,
    SubagentMailboxEvent, SubagentMailboxEventKind, SubagentMailboxStatus,
};
use vulcan_session_core::{
    ContextCompactionPolicyRequest, ContextCompactionPolicySnapshot, ExecutionRuntimePolicyRequest,
    ExecutionScope, ParentSessionRef, PromptSkillSnapshot, ProviderModelSnapshot,
    SessionContextDocument, SessionEffectiveConfig, SessionEventKind, SessionEventRecord,
    SessionNodeId, SessionNodeStore, SessionRequestedConfig, SessionRuntimeCheckpoint,
    SessionSegmentSummary, SubagentKind, SubagentRegistry, SubagentRegistryEntry, SubagentStatus,
    ToolExposureSnapshot,
};
use vulcan_subagent_core::{
    FOLLOWUP_TASK_TOOL, INTERRUPT_AGENT_TOOL, LIST_AGENTS_TOOL, SEND_MESSAGE_TOOL,
    SPAWN_AGENT_TOOL, STOPPED_SUBAGENT_TASK_IDS_KEY, SUBAGENT_TASK_ID_KEY, SUBAGENT_TASK_KIND,
    WAIT_AGENT_TOOL, WaitAgentActivityResult,
};
use vulcan_task_session::{
    SegmentedTaskSessionStore, TaskInvocationConfig, TaskInvocationEvent, TaskInvocationEventKind,
    TaskSessionCompactionStore, TaskSessionConfigResolutionInput, TaskSessionConfigResolver,
    TaskSessionMetadata, TaskSessionResolvedConfig, TaskSessionStatus, TaskSessionStore,
    apply_task_session_compaction_result,
};
use vulcan_tools_manager::{
    VulcanPlanToolPolicy, VulcanToolCandidate, VulcanToolDescriptor, VulcanToolHostResult,
    VulcanToolInvocationResult, VulcanToolSourceKind,
};

/// Default nested-task depth limit used to prevent unbounded recursive task spawning.
/// 用于防止无限递归任务派生的默认嵌套任务深度上限。
const DEFAULT_TASK_NESTING_LIMIT: usize = 1;

/// Fallback stop summary returned to the parent child-agent tool when the child run is manually interrupted before producing a complete answer.
/// 当子运行在产出完整答案前被人工中断时，返回给父级子代理工具的兜底停止摘要。
const STOPPED_TASK_RESULT_SUMMARY: &str = "The subagent was stopped at the user's request before producing a complete final result. Decide whether to retry, rewrite the task, or continue the main flow with the current context.";

/// Running inspection summary returned only when the model checks a task that is already active.
/// 仅当模型检查一条已经运行中的任务时返回的运行中查看摘要。
const RUNNING_TASK_RESULT_SUMMARY: &str = "This is a running-status acknowledgement, not the subagent's final result. The subagent owns the delegated work now. Do not recreate the same child agent, do not replace the unfinished subagent work with direct main-thread or non-agent tool calls, and do not produce a final answer from assumptions. If coordination requires a status check, use list_agents or wait_agent for the existing child agent.";

/// Windows filesystem path guidance injected into child task prompts to stabilize tool calls.
/// 注入到子任务提示词中的 Windows 文件系统路径指引，用于稳定工具调用。
const WINDOWS_TASK_FILESYSTEM_PATH_GUIDANCE: &str = "[Filesystem Paths]\nWhen calling filesystem-oriented tools on Windows, prefer standard absolute Windows paths like C:\\repo\\path. Do not wrap tool arguments in Markdown code fences or extra quotes. When a JSON string needs escaping, keep the semantic path as a normal Windows path and escape backslashes only as required by the transport format.";

mod agent_control;
mod arguments;
mod controller_config;
mod invocation_methods;
mod invocation_results;
mod lifecycle_methods;
mod parent_events;
mod persistence_methods;
mod request_records;
mod runtime_execution;
mod runtime_launch;
mod runtime_state;
mod status_events;
mod tool_surface;

use agent_control::*;
use arguments::*;
use controller_config::*;
use invocation_results::*;
use parent_events::*;
use request_records::*;
use runtime_execution::*;
use runtime_launch::*;
use status_events::*;
use tool_surface::*;

pub(crate) use parent_events::build_task_execution_handler;
pub(crate) use tool_surface::build_subagent_tool_candidates;

/// Boxed asynchronous task execution future.
/// 装箱后的异步任务执行 Future。
pub type TaskExecutionFuture =
    Pin<Box<dyn Future<Output = Result<TaskExecutionOutcome, String>> + Send>>;

/// Shared asynchronous task execution handler.
/// 共享的异步任务执行处理器。
pub type TaskExecutionHandler =
    Arc<dyn Fn(TaskExecutionRequest) -> TaskExecutionFuture + Send + Sync>;

/// Structured TaskSession arguments used internally by the child-agent tool surface.
/// 子代理工具面内部使用的结构化 TaskSession 参数。
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
    /// Optional Vulcan context fork selector for a new subagent.
    /// 新子代理使用的可选 Vulcan 上下文派生选择器。
    fork_turns: Option<String>,
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
    /// The top-level WebUI or task session that originated this task creation.
    /// 发起该任务创建的顶层 WebUI 或任务会话标识。
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
    /// Final execution status surfaced to both host persistence and the parent spawn-agent result.
    /// 同时暴露给宿主持久化与父级 spawn_agent 结果的最终执行状态。
    pub execution_status: TaskExecutionStatus,
    /// Provider usage segments that should be aggregated into the owning parent turn.
    /// 应聚合进所属父会话回合的 Provider 用量分段。
    pub usage_segments: Vec<SessionTurnUsageSegment>,
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

/// Fully resolved task-session identifiers derived from one child-agent invocation.
/// 从一次子代理调用中完整解析出的任务会话标识集合。
#[derive(Debug, Clone)]
struct ResolvedTaskSessionIdentifier {
    /// Internal UUID-shaped child session identifier used by the runtime registry.
    /// 被运行时注册表使用的内部 UUID 形态子会话标识。
    task_id: SessionId,
    /// Caller-facing task identifier preserved for model-visible results and future task reuse.
    /// 为模型可见结果与后续任务复用保留的调用方面向任务标识。
    public_task_id: String,
    /// Whether the resolved internal session already exists and should resume in place.
    /// 解析出的内部会话是否已经存在并应原地续跑。
    is_resumed: bool,
    /// Persisted metadata loaded when the in-memory registry had no matching record.
    /// 当内存注册表没有匹配记录时从持久化存储加载的元数据。
    persisted_metadata: Option<TaskSessionMetadata>,
}

/// Detached runtime event mirrored out of one child task while its parent tool call waits for the terminal result.
/// 当父级工具调用等待终态结果时，由某条子任务镜像输出的分离运行时事件。
#[derive(Debug, Clone)]
pub enum DetachedTaskRuntimeEvent {
    /// Child-session execution event that should be projected into the dedicated subagent transcript surface.
    /// 应投影到独立子代理转录界面中的子会话执行事件。
    ChildExecution(ExecutionStreamEvent),
    /// Parent-session runtime event that updates task cards and mailbox lifecycle state.
    /// 用于更新 task 卡片与 mailbox 生命周期状态的父会话运行时事件。
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

/// One queued inter-agent message waiting for a child-agent safe boundary.
/// 等待子代理安全边界消费的一条代理间消息。
#[derive(Debug, Clone)]
struct QueuedInterAgentMessage {
    /// Session that authored the message.
    /// 发出该消息的会话标识。
    author_session_id: SessionId,
    /// Target child-agent task session.
    /// 目标子代理任务会话标识。
    target_task_id: SessionId,
    /// Human-readable message content.
    /// 人类可读消息正文。
    content: String,
    /// Whether this message should start a new child-agent turn when the target is idle.
    /// 当目标空闲时该消息是否应启动新的子代理回合。
    trigger_turn: bool,
    /// Unix-millis enqueue timestamp.
    /// 入队时的 Unix 毫秒时间戳。
    created_at_ms: u64,
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
    /// Parent canonical snapshot available for context forking into child agents.
    /// 可派生给子代理上下文的父级规范快照。
    pub(crate) parent_snapshot: CoreConversationSnapshot,
    /// Whether child agents should inherit the subagent orchestration tool family.
    /// 子代理是否应继承子代理调度工具族。
    pub(crate) allow_subagent_tools: bool,
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
            parent_snapshot: CoreConversationSnapshot::default(),
            allow_subagent_tools: false,
        }
    }

    /// Returns this context with the parent canonical snapshot attached.
    /// 返回附带父级规范快照的上下文。
    ///
    /// # Parameters
    /// 参数
    /// `parent_snapshot` is the snapshot captured from the bootstrapped parent controller.
    /// `parent_snapshot` 是从已启动父控制器捕获的快照。
    ///
    /// # Returns
    /// 返回值
    /// The updated runtime context.
    /// 更新后的运行时上下文。
    pub(crate) fn with_parent_snapshot(
        mut self,
        parent_snapshot: CoreConversationSnapshot,
    ) -> Self {
        self.parent_snapshot = parent_snapshot;
        self
    }

    /// Returns this context with explicit child-agent subagent tool inheritance.
    /// 返回带显式子代理调度工具继承设置的上下文。
    ///
    /// # Parameters
    /// 参数
    /// `allow_subagent_tools` controls whether child agents can see the subagent tool family.
    /// `allow_subagent_tools` 控制子代理是否可以看到子代理工具族。
    ///
    /// # Returns
    /// 返回值
    /// The updated runtime context.
    /// 更新后的运行时上下文。
    pub(crate) fn with_subagent_tools_allowed(mut self, allow_subagent_tools: bool) -> Self {
        self.allow_subagent_tools = allow_subagent_tools;
        self
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
    /// Detached child-task event streams keyed by stable task identifier for non-blocking WebUI mirroring.
    /// 为无阻塞WebUI镜像保留的按稳定任务标识索引的分离子任务事件流。
    detached_task_event_streams: RwLock<HashMap<SessionId, DetachedTaskEventStreamState>>,
    /// session-boundary mailbox used for child-agent lifecycle association events.
    /// 用于子代理生命周期关联事件的 会话边界 mailbox。
    subagent_mailbox: InMemorySubagentMailbox,
    /// Monotonic activity counter used to wake wait_agent on parent mailbox changes.
    /// 用于在父级 mailbox 变化时唤醒 wait_agent 的单调递增活动计数器。
    subagent_mailbox_activity_tx: watch::Sender<u64>,
    /// Monotonic activity counter used to wake wait_agent on parent user steering.
    /// 用于在父级用户插话时唤醒 wait_agent 的单调递增活动计数器。
    parent_input_activity_tx: watch::Sender<u64>,
    /// Parent sessions that have pending user steering activity for wait_agent.
    /// 对 wait_agent 存在待处理用户插话活动的父会话集合。
    parent_input_activity_sessions: RwLock<BTreeSet<SessionThreadId>>,
    /// Per-child-agent inbox used by send_message and followup_task.
    /// `send_message` 与 `followup_task` 使用的按子代理划分 inbox。
    agent_inboxes: RwLock<HashMap<SessionId, VecDeque<QueuedInterAgentMessage>>>,
    /// Vulcan-native child-agent registry used as the future single source for subagent state.
    /// Vulcan 原生子代理注册表，用作未来子代理状态的单一事实源。
    subagent_registry: RwLock<SubagentRegistry>,
    /// Segmented task-session roots keyed by root main-session identifier.
    /// 按根主会话标识索引的分段子代理会话根目录。
    task_session_store_roots: RwLock<HashMap<SessionId, PathBuf>>,
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

#[cfg(test)]
mod tests;
