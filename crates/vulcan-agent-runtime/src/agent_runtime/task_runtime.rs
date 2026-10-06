//! Resumable task-session runtime for host-managed task_id chaining and nested subtask execution.
//! 供宿主管理 task_id 续跑与嵌套子任务执行使用的可恢复任务会话运行时模块。

use crate::agent_runtime::events::{ExecutionStreamEvent, RuntimeEvent, RuntimeEventKind};
use crate::agent_runtime::execution_models::{
    ExecutionAgentPromptSnapshot, ExecutionPreparationRequest, ExecutionRunReport,
    ExecutionSubagentPromptContext,
};
use crate::agent_runtime::execution_preparation::prepare_execution_with_tool_snapshot;
use crate::agent_runtime::prompt_compaction::{
    PromptCompactionFoundationJobRequest, PromptCompactionLaunchContext,
    PromptCompactionStreamEvent, resolve_prompt_compaction_model_target,
    stream_prompt_compaction_foundation_job,
};
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
    attach_runtime_event_projection_barrier, cancel_runtime_event_projection_barrier,
    wait_runtime_event_projection_barrier,
};
use crate::luaskills_runtime::LuaSkillsRuntimeHandle;
#[cfg(test)]
use crate::model::SessionTurnUsageSegmentKind;
use crate::model::{SessionId, SessionTurnUsageSegment};
use crate::provider_config::{PersistedPromptSkillState, ProviderConfigurationFile};
use async_stream::stream;
use collaboration_lifecycle::ChildTaskLifecycleBinding;
use futures_util::StreamExt;
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::VecDeque;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::future::Future;
use std::pin::Pin;
use std::sync::{Arc, Mutex, RwLock, Weak};
use std::time::Duration;
use time::OffsetDateTime;
use tokio::sync::{
    mpsc::{UnboundedSender, unbounded_channel},
    watch,
};
use tokio::task::AbortHandle;
use uuid::Uuid;
use vulcan_ai_contract::HostMessageKind;
use vulcan_context::{ContextCompactionPolicy, conversation_state_to_compaction_source_messages};
use vulcan_context_compaction::{CompactionPlanInput, plan_context_compaction};
#[cfg(test)]
use vulcan_core::types::ConversationPayload as CoreConversationPayload;
use vulcan_core::types::{
    ControllerConfig as CoreControllerConfig, ConversationSnapshot as CoreConversationSnapshot,
    ProviderPromptLayers as CoreProviderPromptLayers,
};
use vulcan_session_boundary::{
    InMemorySubagentMailbox, SessionThreadId, SubagentMailbox, SubagentMailboxAgentKind,
    SubagentMailboxEvent, SubagentMailboxEventKind, SubagentMailboxStatus,
};
use vulcan_session_core::{
    CapabilityPolicyMode, CapabilityPolicySnapshot, ContextCompactionPolicyRequest,
    ContextCompactionPolicySnapshot, ExecutionRuntimePolicyRequest, ExecutionScope,
    PromptSkillSnapshot, ProviderModelSnapshot, SessionEffectiveConfig, SessionNodeId,
    SubagentKind, SubagentRegistry, SubagentRegistryEntry, SubagentStatus, ToolExposureSnapshot,
};
use vulcan_subagent_core::{
    AGENT_ORCHESTRATION_INFO_TOOL, CLOSE_AGENT_TOOL, INTERRUPT_AGENT_TOOL, LIST_AGENTS_TOOL,
    SEND_MESSAGE_TOOL, SPAWN_AGENT_TOOL, STOPPED_SUBAGENT_TASK_IDS_KEY, SUBAGENT_TASK_ID_KEY,
    SUBAGENT_TASK_KIND, WAIT_AGENT_TOOL, WaitAgentActivityResult,
};
use vulcan_task_session::{
    SUBAGENT_MODE_DEFAULT, SUBAGENT_MODE_EXPERT, SUBAGENT_MODE_MIRROR, SubagentCatalog,
    SubagentDefinition, TaskInvocationConfig, TaskSessionConfigResolutionInput,
    TaskSessionConfigResolver, TaskSessionResolvedConfig, TaskSessionStatus,
};
use vulcan_tools_manager::{
    VulcanPlanToolPolicy, VulcanToolCandidate, VulcanToolDescriptor, VulcanToolHostResult,
    VulcanToolInvocationResult, VulcanToolOrigin, VulcanToolSourceKind,
};
/// Default nested-task depth limit used to prevent unbounded recursive task spawning.
/// 用于防止无限递归任务派生的默认嵌套任务深度上限。
const DEFAULT_TASK_NESTING_LIMIT: usize = 1;

/// Fallback stop summary returned to the parent child-agent tool when the child run is manually interrupted before producing a complete answer.
/// 当子运行在产出完整答案前被人工中断时，返回给父级子代理工具的兜底停止摘要。
const STOPPED_TASK_RESULT_SUMMARY: &str = "The subagent was stopped at the user's request before producing a complete final result. Decide whether to retry, rewrite the task, or continue the main flow with the current context.";

/// Running inspection summary returned only when the model checks a task that is already active.
/// 仅当模型检查一条已经运行中的任务时返回的运行中查看摘要。
const RUNNING_TASK_RESULT_SUMMARY: &str = "This is a running-status acknowledgement, not the subagent's final result. The subagent owns the delegated work now. Do not recreate the same child agent, do not replace the unfinished subagent work with direct main-thread or non-agent tool calls, and do not produce a final answer from assumptions. If coordination requires a status check, use list_agents or wait_agents_result for the existing child agent.";

mod agent_control;
mod arguments;
mod business_recovery;
mod child_request_overrides;
pub mod collaboration_lifecycle;
mod controller_config;
mod invocation_methods;
mod invocation_results;
mod lifecycle_methods;
mod parent_events;
mod question_request;
mod question_reservation;
mod request_records;
mod runtime_execution;
mod runtime_launch;
mod runtime_state;
mod status_events;
mod task_records;
mod tool_surface;

use agent_control::*;
use arguments::*;
use child_request_overrides::*;
use controller_config::*;
use invocation_results::*;
use parent_events::*;
use request_records::*;
use runtime_execution::*;
use runtime_launch::*;
use status_events::*;
use tool_surface::*;

pub(crate) use parent_events::build_task_execution_handler;
pub use question_request::{restore_child_business_request, restore_child_question_request};
pub use question_reservation::{ChildAuxiliaryExecutionGuard, ChildAuxiliaryExecutionPurpose};
pub(crate) use tool_surface::build_subagent_tool_candidates;

/// Boxed asynchronous task execution future.
/// 装箱后的异步任务执行 Future。
pub type TaskExecutionFuture =
    Pin<Box<dyn Future<Output = Result<TaskExecutionOutcome, String>> + Send>>;

/// Shared asynchronous task execution handler.
/// 共享的异步任务执行处理器。
pub type TaskExecutionHandler =
    Arc<dyn Fn(TaskExecutionRequest) -> TaskExecutionFuture + Send + Sync>;

/// Explicit source used to initialize a newly created child-agent runtime state.
/// 用于初始化新建子智能体运行时状态的显式来源。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TaskInitialContextSource {
    /// Start the child agent from an empty runtime conversation state.
    /// 使用空运行时会话状态启动子智能体。
    Fresh,
    /// Clone the latest parent runtime conversation state at creation time.
    /// 在创建时克隆最新父级运行时会话状态。
    CloneLatestRuntime,
}

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
    /// Optional explicit subagent mode key.
    /// 可选的显式子代理模式键。
    agent_mode: Option<String>,
    /// Optional string catalog id for the requested subagent definition.
    /// 请求的子代理定义可选字符串目录标识。
    subagent_profile_id: Option<String>,
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
    /// Optional execution-mode override for this subagent.
    /// 子代理的可选执行模式覆盖。
    execution_mode: Option<String>,
    /// Flexible provider/model override payload.
    /// 灵活的供应商与模型覆盖载荷。
    provider_model: Option<Value>,
    /// Flexible tool allow/deny override payload.
    /// 灵活的工具允许与禁用覆盖载荷。
    tools: Option<Value>,
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
    /// Fixed parent-runtime agent prompt captured when this task was created.
    /// 创建该任务时捕获的固定父级运行时代理提示词。
    agent_prompt_snapshot: Option<ExecutionAgentPromptSnapshot>,
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
    /// Concrete execution that currently owns mutable state for this stable task record.
    /// 当前拥有该稳定任务记录可变状态的具体执行。
    execution_id: Option<String>,
    /// Reason attached to the latest explicit stopped or archived lifecycle state.
    /// 最近一次显式停止或归档生命周期状态对应的原因。
    stop_reason: Option<String>,
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
    /// Stable identifier for this concrete child execution attempt.
    /// 本次子任务具体执行尝试的稳定标识。
    pub execution_id: String,
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

/// Immutable V1 admission facts retained for one exact active child execution.
/// 为一次精确活动子执行保留的不可变 V1 准入事实。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskV1ExecutionFacts {
    /// Root main-session identity that owns the physical `TASK` directory.
    /// 拥有物理 `TASK` 目录的根主会话身份。
    pub root_session_id: SessionId,
    /// Direct parent session whose open Turn receives the final child return.
    /// 其开放 Turn 接收最终子代理回流的直接父会话。
    pub parent_session_id: SessionId,
    /// Stable child TaskSession identity.
    /// 稳定子 TaskSession 身份。
    pub task_id: SessionId,
    /// Stable identity of this concrete child execution attempt.
    /// 本次具体子执行尝试的稳定身份。
    pub execution_id: String,
    /// Exact root input text submitted for this execution.
    /// 本次执行提交的精确根输入正文。
    pub prompt: String,
    /// Stable child-session display title.
    /// 稳定子会话显示标题。
    pub display_title: String,
    /// Whether this execution continues an already published child session.
    /// 本次执行是否继续一个已经发布的子会话。
    pub is_resumed: bool,
    /// Stable Unix-millisecond admission timestamp retained before lifecycle publication.
    /// 在生命周期发布前保留的稳定 Unix 毫秒准入时间戳。
    pub admitted_at_ms: u64,
}

impl TaskV1ExecutionFacts {
    /// Projects the exact V1 admission facts from one owned execution request.
    /// 从一条已拥有的执行请求投影精确 V1 准入事实。
    ///
    /// # Parameters / 参数
    ///
    /// - `request`: Authoritative task execution request before any lifecycle event escapes.
    ///   `request`：任何生命周期事件逸出前的权威任务执行请求。
    ///
    /// # Returns / 返回值
    ///
    /// One immutable fact set safe to clone across the detached listener boundary.
    /// 一组可安全跨分离监听边界克隆的不可变事实。
    fn from_request(request: &TaskExecutionRequest, admitted_at_ms: u64) -> Self {
        Self {
            root_session_id: request.root_session_id,
            parent_session_id: request.current_session_id,
            task_id: request.task_id,
            execution_id: request.execution_id.clone(),
            prompt: request.prompt.clone(),
            display_title: request.display_title.clone(),
            is_resumed: request.is_resumed,
            admitted_at_ms,
        }
    }
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
}

/// Detached runtime event mirrored out of one child task while its parent tool call waits for the terminal result.
/// 当父级工具调用等待终态结果时，由某条子任务镜像输出的分离运行时事件。
#[derive(Debug, Clone)]
pub enum DetachedTaskRuntimeEvent {
    /// Explicit startup termination emitted after the exact active lease has been released.
    /// 在精确活动租约释放后发出的明确启动终止事件。
    StartupAborted {
        /// Original direct parent captured by the startup request.
        /// 启动请求捕获的原直接父会话。
        parent_session_id: SessionId,
        /// Host-produced failure feedback preserving the original request identity and cause.
        /// 保留原请求身份与原因的宿主失败反馈。
        event: RuntimeEvent,
    },
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

/// Metadata key recording whether an inter-agent delivery continues the active logical task turn.
/// 记录代理间投递是否延续当前逻辑任务轮次的元数据键。
const INTER_AGENT_CONTINUES_TASK_TURN_KEY: &str = "continues_task_turn";

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

/// Exact identity of one detached execution stream under a stable reusable task.
/// 稳定可复用任务下单次分离执行流的精确身份。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct DetachedTaskExecutionKey {
    /// Stable child task session reused by resumed executions.
    /// 被续跑执行复用的稳定子任务会话。
    task_id: SessionId,
    /// Unique concrete execution attempt.
    /// 唯一具体执行尝试。
    execution_id: String,
}

impl DetachedTaskExecutionKey {
    /// Builds one owned execution key from the stable task and concrete execution identities.
    /// 从稳定任务与具体执行身份构建一个有所有权的执行键。
    fn new(task_id: SessionId, execution_id: &str) -> Self {
        Self {
            task_id,
            execution_id: execution_id.to_string(),
        }
    }
}

/// Buffered detached event state kept per execution so resumed runs never share output or subscribers.
/// 按执行维度保留的分离事件状态，确保续跑不会共享输出或订阅者。
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

/// Active child-task run entry protected by a generation token.
/// 带运行世代令牌保护的活动子任务运行条目。
#[derive(Debug, Clone)]
struct ActiveTaskRunEntry {
    /// Unique generation for one concrete provider run.
    /// 单次具体 Provider 运行的唯一世代标识。
    run_id: Uuid,
    /// Protocol execution identity shared with runtime events and detached streams.
    /// 与运行时事件及分离流共享的协议执行身份。
    execution_id: String,
    /// Immutable V1 session-admission facts for this exact execution.
    /// 本次精确执行的不可变 V1 会话准入事实。
    v1_execution_facts: Option<TaskV1ExecutionFacts>,
    /// Abort handle attached after the child provider future is created.
    /// 创建子 Provider future 后附加的中止句柄。
    abort_handle: Option<AbortHandle>,
    /// Stop request retained while the run is still inside prestart preparation.
    /// 运行仍处于启动前准备阶段时保留的停止请求。
    stop_requested: bool,
}

/// One queued inter-agent message waiting for a child-agent safe boundary.
/// 等待子代理安全边界消费的一条代理间消息。
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskPendingMessage {
    /// Stable queue identity used by panel updates.
    /// 面板更新使用的稳定队列身份。
    pub delivery_id: String,
    /// Original message text.
    /// 原始消息正文。
    pub message: String,
    /// Whether delivery occurs before the next API instead of after the task.
    /// 是否在下一次 API 前投递，而非任务结束后。
    pub steering: bool,
}

/// One queued inter-agent message waiting for a child-agent safe boundary.
/// 等待子代理安全边界消费的一条代理间消息。
#[derive(Debug, Clone)]
struct QueuedInterAgentMessage {
    /// Typed host message kind selected by the delivery mode at admission.
    /// 在接纳阶段根据投递模式选择的类型化宿主消息种类。
    kind: HostMessageKind,
    /// Stable task identity owned by this concrete delivery execution.
    /// 由本次具体投递执行持有的稳定任务身份。
    task_id: String,
    /// Whether the message is eligible at the next physical request boundary.
    /// 消息是否应在下一次物理请求边界被消费。
    steering: bool,
    /// Stable delivery id shared by queue, display records, and host payloads.
    /// 队列、显示记录与宿主载荷共享的稳定投递标识。
    delivery_id: String,
    /// Session that authored the message.
    /// 发出该消息的会话标识。
    author_session_id: SessionId,
    /// Target child-agent task session.
    /// 目标子代理任务会话标识。
    target_task_id: SessionId,
    /// Caller-facing target child-agent id.
    /// 调用方面向的目标智能体标识。
    target_agent_id: String,
    /// Human-readable message content.
    /// 人类可读消息正文。
    content: String,
    /// Unix-millis enqueue timestamp.
    /// 入队时的 Unix 毫秒时间戳。
    created_at_ms: u64,
    /// Whether this delivery was admitted while the child still owned an active logical turn.
    /// 本次投递被接纳时子代理是否仍拥有活动逻辑轮次。
    continues_task_turn: bool,
}

/// One generation-owned runtime registry value.
/// 一条由注册世代拥有的运行时注册表值。
struct TaskRuntimeRegistration<T> {
    /// Unique generation that distinguishes this value from replacements under the same key.
    /// 区分同一键下当前值与替代值的唯一世代。
    registration_id: Uuid,
    /// Runtime value owned by this registration generation.
    /// 由本注册世代拥有的运行时值。
    value: T,
}

/// Guard that unregisters one exact backend generation when it goes out of scope.
/// 作用域结束时自动注销某个后端的守卫对象。
pub struct TaskBackendGuard {
    /// Shared runtime registry that owns the backend map.
    /// 持有后端映射表的共享运行时注册表。
    runtime: Arc<TaskRuntime>,
    /// Session identifier whose backend should be removed on drop.
    /// 在 Drop 时应移除其后端的会话标识。
    session_id: SessionId,
    /// Exact backend registration generation owned by this guard.
    /// 本守卫拥有的精确后端注册世代。
    registration_id: Uuid,
    /// Whether this guard has already released its owned registration successfully.
    /// 本守卫是否已经成功释放其拥有的注册项。
    released: bool,
}

impl TaskBackendGuard {
    /// Explicitly releases the exact backend generation owned by this guard.
    /// 显式释放本守卫拥有的精确后端注册世代。
    ///
    /// # Returns
    /// 返回值
    /// Success when the owned generation is absent or removed, otherwise a registry-lock error.
    /// 所属世代已不存在或已移除时返回成功，否则返回注册表锁错误。
    pub(crate) fn release(mut self) -> Result<(), String> {
        self.runtime
            .remove_backend(self.session_id, self.registration_id)?;
        self.released = true;
        Ok(())
    }
}

impl Drop for TaskBackendGuard {
    /// Removes only this guard's backend generation as soon as the guard leaves scope.
    /// 在守卫离开作用域时仅移除本守卫拥有的后端世代。
    ///
    /// # Returns
    /// 返回值
    /// Returns no value; a replacement generation remains registered.
    /// 不返回值；替代注册世代会继续保留。
    fn drop(&mut self) {
        if !self.released
            && let Err(error) = self
                .runtime
                .remove_backend(self.session_id, self.registration_id)
        {
            eprintln!("[task-runtime] backend guard fallback release failed: {error}");
        }
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
    /// Run generation that this guard installed.
    /// 该守卫安装的运行世代标识。
    run_id: Uuid,
    /// Whether this guard has already released its owned active-run reservation successfully.
    /// 本守卫是否已经成功释放其拥有的活动运行预留。
    released: bool,
}

impl Drop for ActiveTaskRunGuard {
    /// Removes the installed active child-task run entry as soon as the guard leaves scope.
    /// 在守卫离开作用域时立即移除已安装的活动子任务运行条目。
    fn drop(&mut self) {
        if !self.released
            && let Err(error) = self
                .runtime
                .remove_active_task_run(self.task_id, self.run_id)
        {
            eprintln!("[task-runtime] active-run guard fallback release failed: {error}");
        }
    }
}

impl ActiveTaskRunGuard {
    /// Releases this exact run in place while retaining ownership if a registry failure needs retry.
    /// 原地释放此精确运行，在注册表失败需要重试时保留所有权。
    /// Returns success for an already released lease or after exact-generation removal.
    /// 已释放租约或精确代次删除之后返回成功。
    pub(crate) fn release_in_place(&mut self) -> Result<(), String> {
        if !self.released {
            self.runtime
                .remove_active_task_run(self.task_id, self.run_id)?;
            self.released = true;
        }
        Ok(())
    }

    /// Explicitly releases the exact active-run reservation owned by this guard.
    /// 显式释放本守卫拥有的精确活动运行预留。
    ///
    /// # Returns
    /// 返回值
    /// Success when the owned reservation is absent or removed, otherwise a registry-lock error.
    /// 所属预留已不存在或已移除时返回成功，否则返回注册表锁错误。
    pub(crate) fn release(mut self) -> Result<(), String> {
        self.release_in_place()
    }

    /// Reports whether this exact run received a stop request.
    /// 报告此精确运行是否收到了停止请求。
    pub(crate) fn stop_requested(&self) -> Result<bool, String> {
        self.runtime
            .active_task_run_stop_requested(self.task_id, self.run_id)
    }

    /// Attaches the provider abort handle to this exact reserved run.
    /// 将 Provider 中止句柄附加到本守卫对应的精确预留运行。
    ///
    /// # Parameters
    /// 参数
    /// `handle` is the abort handle created with the child provider future.
    /// `handle` 是随子 Provider future 创建的中止句柄。
    ///
    /// # Returns
    /// 返回值
    /// Success when this guard still owns the active reservation, or an identity/lock error.
    /// 当本守卫仍持有活动预留时返回成功，否则返回身份或锁错误。
    pub(crate) fn attach_abort_handle(&self, handle: AbortHandle) -> Result<(), String> {
        self.runtime
            .attach_active_task_run_abort_handle(self.task_id, self.run_id, handle)
    }
}

/// Shared execution context captured for a controller session so task runs can reuse the normal runtime preparation bridge.
/// 为控制器会话捕获的共享执行上下文，使子任务运行能够复用普通运行时准备桥。
#[derive(Clone)]
pub(crate) struct TaskExecutionRuntimeContext {
    /// Captured business settings used only while recovering this exact execution.
    /// 仅恢复精确执行期间使用的已捕获业务设置。
    pub(crate) recovered_child_profile:
        Option<vulcan_session_core::v1::child_execution::ChildExecutionProfile>,
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
    /// Fixed agent prompt captured for child executions spawned from this context.
    /// 为从该上下文派生的子代理执行捕获的固定代理提示词。
    pub(crate) agent_prompt_snapshot: Option<ExecutionAgentPromptSnapshot>,
    /// Parent canonical snapshot available for context forking into child agents.
    /// 可派生给子代理上下文的父级规范快照。
    pub(crate) parent_snapshot: CoreConversationSnapshot,
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
        let agent_prompt_snapshot =
            fixed_agent_prompt_snapshot_for_request(&foundation, &request_template);
        Self {
            recovered_child_profile: None,
            config,
            foundation,
            luaskills_runtime,
            request_template,
            agent_prompt_snapshot,
            parent_snapshot: CoreConversationSnapshot::default(),
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
}

/// Captures the fixed agent prompt that belongs to one runtime request template.
/// 捕获一份运行时请求模板所属的固定代理提示词。
///
/// # Parameters
/// 参数
/// `foundation` is the runtime foundation that owns the built agent definitions.
/// `foundation` 是持有已构建代理定义的运行时基础架构。
/// `request_template` is the parent execution request used as the child-template source.
/// `request_template` 是用作子级模板来源的父级执行请求。
///
/// # Returns
/// 返回值
/// The request-scoped snapshot when present, otherwise the explicitly selected runtime agent prompt.
/// 若存在则返回请求级快照，否则返回显式选中运行时代理的提示词。
fn fixed_agent_prompt_snapshot_for_request(
    foundation: &RuntimeFoundation,
    request_template: &ExecutionPreparationRequest,
) -> Option<ExecutionAgentPromptSnapshot> {
    if let Some(snapshot) = request_template.agent_prompt_snapshot.clone() {
        return Some(snapshot);
    }
    let agent = match request_template.execution_agent_id.as_deref() {
        Some(agent_id) => foundation.agent_definition(agent_id),
        None => foundation.active_agent_definition(),
    }?;
    ExecutionAgentPromptSnapshot::fixed_parent_runtime(agent.system_prompt.as_str())
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
    /// Exact execution-context registration generation owned by this guard.
    /// 本守卫拥有的精确执行上下文注册世代。
    registration_id: Uuid,
    /// Whether this guard has already released its owned execution-context registration successfully.
    /// 本守卫是否已经成功释放其拥有的执行上下文注册项。
    released: bool,
}

impl TaskExecutionRuntimeContextGuard {
    /// Explicitly releases the exact execution-context generation owned by this guard.
    /// 显式释放本守卫拥有的精确执行上下文注册世代。
    ///
    /// # Returns
    /// 返回值
    /// Success when the owned generation is absent or removed, otherwise a registry-lock error.
    /// 所属世代已不存在或已移除时返回成功，否则返回注册表锁错误。
    pub(crate) fn release(mut self) -> Result<(), String> {
        self.runtime
            .remove_execution_context(self.session_id, self.registration_id)?;
        self.released = true;
        Ok(())
    }
}

impl Drop for TaskExecutionRuntimeContextGuard {
    /// Removes only this guard's execution-context generation when the guard leaves scope.
    /// 在守卫离开作用域时仅移除本守卫拥有的执行上下文世代。
    ///
    /// # Returns
    /// 返回值
    /// Returns no value; a replacement generation remains registered.
    /// 不返回值；替代注册世代会继续保留。
    fn drop(&mut self) {
        if !self.released
            && let Err(error) = self
                .runtime
                .remove_execution_context(self.session_id, self.registration_id)
        {
            eprintln!("[task-runtime] execution-context guard fallback release failed: {error}");
        }
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

/// Stable failing stage for one close-agent archive transaction.
/// 单次 close-agent 归档事务的稳定失败阶段。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CloseAgentArchiveFailurePhase {
    /// Committing archive state to runtime projections failed.
    /// 向运行时投影提交归档状态失败。
    CommitRuntimeArchive,
}

impl CloseAgentArchiveFailurePhase {
    /// Returns the stable protocol value for this failure stage.
    /// 返回本失败阶段的稳定协议值。
    ///
    /// # Returns
    /// 返回值
    /// Stable snake-case phase value.
    /// 稳定的蛇形命名阶段值。
    const fn as_str(self) -> &'static str {
        match self {
            Self::CommitRuntimeArchive => "commit_runtime_archive",
        }
    }
}

/// Unified typed failure for every post-stop stage of close_agent archive processing.
/// close_agent 停止后全部归档处理阶段的统一类型化失败。
#[derive(Debug, Clone, PartialEq, Eq)]
struct CloseAgentArchiveFailure {
    /// Exact post-stop archive phase that failed.
    /// 发生失败的精确停止后归档阶段。
    phase: CloseAgentArchiveFailurePhase,
    /// Primary phase failure retained for protocol projection.
    /// 为协议投影保留的阶段主错误。
    error: String,
}

/// Stable archive scope and lifecycle projections prepared for one close-agent transaction.
/// 为一次 close-agent 事务准备的稳定归档范围与生命周期投影。
#[derive(Debug)]
struct PreparedCloseAgentArchive {
    /// Authoritative archived registry projections committed by persistence and runtime stages.
    /// 由持久化与运行时阶段提交的权威归档注册表投影。
    entries: Vec<SubagentRegistryEntry>,
    /// Stable task ids derived exactly once from the prepared registry projections.
    /// 从已准备注册表投影中仅派生一次的稳定任务标识。
    task_ids: Vec<SessionId>,
}

impl PreparedCloseAgentArchive {
    /// Builds one prepared archive and derives its task-id projection from the same entries.
    /// 构建一份已准备归档，并从同一批条目派生任务标识投影。
    ///
    /// # Parameters
    /// 参数
    /// - `entries`: Archived registry projections selected by cascade semantics.
    /// - `entries`：按级联语义选中的归档注册表投影。
    ///
    /// # Returns
    /// 返回值
    /// Prepared archive whose entries and task ids cannot drift independently.
    /// 条目与任务标识无法独立漂移的已准备归档。
    fn new(entries: Vec<SubagentRegistryEntry>) -> Self {
        let task_ids = entries
            .iter()
            .map(|entry| SessionId::from(entry.session_id))
            .collect();
        Self { entries, task_ids }
    }

    /// Moves the archive transition timestamp after task-stop side effects without changing scope.
    /// 将归档转换时间戳移动到任务停止副作用之后，同时保持事务范围不变。
    ///
    /// # Parameters
    /// 参数
    /// - `archived_at_ms`: Timestamp captured after every selected active run was marked stopped.
    /// - `archived_at_ms`：全部选中活动运行被标记停止后采集的时间戳。
    fn restamp_after_task_stop(&mut self, archived_at_ms: u64) {
        for entry in &mut self.entries {
            entry.set_status(SubagentStatus::Archived, archived_at_ms);
            entry.stop_reason = Some(String::from("close_agent"));
        }
    }
}

/// Runtime archive commit derived from the latest registry state under transaction locks.
/// 在事务锁内根据最新注册表状态派生的运行时归档提交结果。
#[derive(Debug)]
struct CommittedCloseAgentRuntimeArchive {
    /// Latest registry entries after applying only the prepared archive lifecycle transition.
    /// 仅应用已准备归档生命周期转换后的最新注册表条目。
    entries: Vec<SubagentRegistryEntry>,
    /// Number of queued control messages removed by the commit.
    /// 本次提交移除的排队控制消息数量。
    cleared_message_count: usize,
}

/// Complete prepared close-agent transaction shared by persistence, runtime, and result projections.
/// 由持久化、运行时与结果投影共享的完整已准备 close-agent 事务。
#[derive(Debug)]
struct CloseAgentTransaction {
    /// Parent session that invoked close_agent.
    /// 调用 close_agent 的父会话。
    parent_session_id: SessionId,
    /// Normalized public id of the selected child agent.
    /// 选中子代理的规范化公开标识。
    target: String,
    /// Registry status observed before the close transaction.
    /// 关闭事务前观察到的注册表状态。
    previous_status: SubagentStatus,
    /// Tasks with active runs actually stopped by this transaction.
    /// 本事务实际停止活动运行的任务。
    stopped_task_ids: Vec<SessionId>,
    /// Stable archive scope selected before task stop and restamped after those side effects.
    /// 在任务停止前选定、并在停止副作用后更新时间戳的稳定归档范围。
    prepared_archive: PreparedCloseAgentArchive,
}

impl CloseAgentArchiveFailure {
    /// Builds one unified post-stop archive failure.
    /// 构建一条统一的停止后归档失败。
    ///
    /// # Parameters
    /// 参数
    /// - `phase`: Exact archive stage that failed.
    /// - `phase`：发生失败的精确归档阶段。
    /// - `error`: Primary human-readable stage error.
    /// - `error`：人类可读的阶段主错误。
    /// # Returns
    /// 返回值
    /// Unified failure value ready for protocol projection.
    /// 可供协议投影的统一失败值。
    fn new(phase: CloseAgentArchiveFailurePhase, error: String) -> Self {
        Self { phase, error }
    }
}

/// Host-managed registry for resumable task sessions and per-session task execution backends.
/// 宿主管理的可恢复任务会话与按会话划分任务执行后端的注册表。
pub struct TaskRuntime {
    /// Exact host credentials that must be removed before any tool result reaches a model or durable history.
    /// 工具结果到达模型或可靠历史前必须移除的精确宿主凭据。
    sensitive_tool_output_values: Arc<RwLock<Vec<String>>>,
    /// V1-backed executions awaiting durable return publication; completed entries retain exact feedback.
    /// 等待可靠回流发布的 V1 执行；已完成条目保留精确反馈。
    durable_return_mailboxes: RwLock<HashMap<SessionId, (String, Option<SubagentMailboxEvent>)>>,
    /// Stored task-session records keyed by stable task identifier.
    /// 按稳定任务标识索引存储的任务会话记录。
    sessions: RwLock<HashMap<SessionId, TaskSessionRecord>>,
    /// Backend handlers keyed by the invoking controller session identifier.
    /// 按执行中控制器会话标识索引存储的后端处理器。
    backends: RwLock<HashMap<SessionId, TaskRuntimeRegistration<TaskExecutionHandler>>>,
    /// Shared execution contexts keyed by controller session identifier for normal-preparation child task launches.
    /// 按控制器会话标识索引的共享执行上下文，用于普通准备链子任务启动。
    execution_contexts:
        RwLock<HashMap<SessionId, TaskRuntimeRegistration<TaskExecutionRuntimeContext>>>,
    /// Active child-task abort handles keyed by stable task identifier.
    /// 按稳定任务标识索引存储的活动子任务中止句柄。
    active_task_runs: RwLock<HashMap<SessionId, ActiveTaskRunEntry>>,
    /// Auxiliary execution owners excluded from business-task completion accounting.
    /// 不参与业务任务完成计数的辅助执行所有者。
    auxiliary_execution_reservations: RwLock<HashMap<SessionId, Uuid>>,
    /// Per-execution durable collaboration bindings supplied by the trusted tool invocation.
    /// 按具体执行保存由可信工具调用提供的可靠协作绑定。
    collaboration_lifecycle_bindings: RwLock<HashMap<String, ChildTaskLifecycleBinding>>,
    /// Per-task transaction locks serializing preparation ownership and execution state commits.
    /// 按任务串行化准备所有权与执行状态提交的事务锁。
    task_execution_state_locks: Mutex<HashMap<SessionId, Weak<Mutex<()>>>>,
    /// Detached event streams keyed by stable task plus unique execution identity.
    /// 按稳定任务与唯一执行身份联合索引的分离事件流。
    detached_task_event_streams:
        RwLock<HashMap<DetachedTaskExecutionKey, DetachedTaskEventStreamState>>,
    /// session-boundary mailbox used for child-agent lifecycle association events.
    /// 用于子代理生命周期关联事件的 会话边界 mailbox。
    subagent_mailbox: InMemorySubagentMailbox,
    /// Monotonic activity counter used to wake wait_agent on parent mailbox changes.
    /// 用于在父级 mailbox 变化时唤醒 wait_agent 的单调递增活动计数器。
    subagent_mailbox_activity_tx: watch::Sender<u64>,
    /// Monotonic activity counter used to wake wait_agent on explicit parent interruptions.
    /// 用于在显式父级打断时唤醒 wait_agent 的单调递增活动计数器。
    wait_agent_parent_interruption_tx: watch::Sender<u64>,
    /// Parent sessions that have pending explicit interruptions for wait_agent.
    /// 对 wait_agent 存在待处理显式打断的父会话集合。
    wait_agent_parent_interruption_sessions: RwLock<BTreeSet<SessionThreadId>>,
    /// Per-child-agent inbox used by send_message.
    /// `send_message` 使用的按子代理划分 inbox。
    agent_inboxes: RwLock<HashMap<SessionId, VecDeque<QueuedInterAgentMessage>>>,
    /// Vulcan-native child-agent registry used as the future single source for subagent state.
    /// Vulcan 原生子代理注册表，用作未来子代理状态的单一事实源。
    subagent_registry: RwLock<SubagentRegistry>,
    /// Merged subagent catalog used to resolve expert and catalog-backed child agents.
    /// 用于解析专家与目录驱动子代理的合并子代理目录。
    subagent_catalog: RwLock<SubagentCatalog>,
    /// Maximum supported nested task depth.
    /// 支持的最大嵌套任务深度。
    max_depth: usize,
}

/// Runs `request` through the actual standalone producer and consumer for network acceptance fixtures.
/// 为网络验收夹具通过真实独立生产者及消费方运行 `request`。
/// Returns the completed execution only when every physical attempt has matching admission and terminal accounting.
/// 仅当每个物理尝试均有对应准入及终态记账时返回完成执行。
#[cfg(feature = "test-support")]
pub async fn run_standalone_compaction_for_testing(
    request: PromptCompactionFoundationJobRequest,
) -> Result<crate::agent_runtime::prompt_compaction::PromptCompactionExecution, String> {
    if request.ownership
        != crate::agent_runtime::prompt_compaction::PromptCompactionOwnership::RuntimeOnly
    {
        return Err(String::from(
            "Standalone acceptance requires RuntimeOnly ownership.",
        ));
    }
    // The production driver remains the sole confirmation authority; this helper injects no stream events.
    // 生产驱动器仍是唯一确认权威；此辅助入口不注入任何流事件。
    let session_id = request.session_id;
    let (execution, owner) = runtime_execution::drive_standalone_prompt_compaction_stream(
        stream_prompt_compaction_foundation_job(request),
        session_id,
    )
    .await?;
    if owner.admitted_request_count() != execution.provider_request_count
        || owner.terminal_request_count() != execution.provider_request_count
    {
        return Err(String::from(
            "Standalone acceptance physical request accounting mismatch.",
        ));
    }
    Ok(execution)
}

/// Test-only input for seeding one resident subagent registry entry.
/// 用于播种一条常驻子代理注册表条目的仅测试输入。
#[cfg(feature = "test-support")]
pub struct TestSubagentRegistryEntrySeed {
    /// Root session that owns the seeded child-agent tree.
    /// 拥有待播种子代理树的根会话。
    pub root_session_id: SessionId,
    /// Parent session that should own the seeded child task.
    /// 应拥有待播种子任务的父会话。
    pub parent_session_id: SessionId,
    /// Stable internal task-session identifier.
    /// 稳定的内部任务会话标识。
    pub task_id: SessionId,
    /// User-facing public task identifier.
    /// 面向用户展示的公开任务标识。
    pub public_task_id: String,
    /// Display title used by registry and navigation surfaces.
    /// 注册表与导航界面使用的展示标题。
    pub display_title: String,
    /// Optional task description shown in child-agent surfaces.
    /// 子代理界面中展示的可选任务描述。
    pub description: Option<String>,
    /// Optional subagent label attached to the seeded task.
    /// 附加到待播种任务上的可选子代理标签。
    pub subagent: Option<String>,
    /// Runtime status assigned to the seeded task.
    /// 分配给待播种任务的运行时状态。
    pub status: TaskSessionStatus,
    /// Optional final result summary for terminal seeded tasks.
    /// 终态播种任务的可选最终结果摘要。
    pub result_summary: Option<String>,
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
