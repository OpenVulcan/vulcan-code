//! Shared hook registration, dispatch, context, and result primitives for Vulcan extensions.
//! Vulcan 扩展使用的共享 hook 注册、调度、上下文和结果原语。

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::error::Error;
use std::fmt::{self, Display};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::time;
use vulcan_context_injection::InjectionInsert;

/// Stable identifier used for one registered hook handler.
/// 单个已注册 hook 处理器使用的稳定标识符。
pub type HookId = String;

/// Stable hook point name such as `conversation.turn.completed`.
/// 稳定 hook 点位名称，例如 `conversation.turn.completed`。
pub type HookPoint = String;

/// Stable session identifier attached to hook context when available.
/// 可用时附加到 hook 上下文的稳定会话标识符。
pub type HookSessionId = String;

/// Stable project identifier attached to hook context when available.
/// 可用时附加到 hook 上下文的稳定项目标识符。
pub type HookProjectId = String;

/// Stable identifier used for lifecycle event listeners.
/// 生命周期事件监听器使用的稳定标识符。
pub type HookListenerId = String;

/// Describes the behavior class expected from a hook point.
/// 描述 hook 点位期望的行为类别。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HookMode {
    /// Observes a runtime event without changing the payload or control flow.
    /// 观察运行时事件，不修改 payload 或控制流。
    Observe,
    /// May return a payload replacement or additive runtime operations.
    /// 可以返回 payload 替换或追加运行时操作。
    Transform,
    /// May allow, block, or defer a runtime operation.
    /// 可以允许、阻断或延迟运行时操作。
    Decision,
    /// Runs side effects such as indexing, memory writeback, or logging.
    /// 运行索引、记忆写回或日志记录等副作用。
    Effect,
}

/// Describes one hook point exposed by the host.
/// 描述宿主暴露的一个 hook 点位。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HookPointDescriptor {
    /// Stable hook point name.
    /// 稳定 hook 点位名称。
    pub point: HookPoint,
    /// Behavior modes accepted by this hook point.
    /// 该 hook 点位接受的行为模式集合。
    #[serde(default)]
    pub modes: BTreeSet<HookMode>,
    /// Default scope expected for this hook point.
    /// 该 hook 点位期望的默认作用域。
    #[serde(default)]
    pub default_scope: HookScope,
    /// Human-readable point description.
    /// 人类可读的点位说明。
    pub description: String,
    /// Optional JSON schema describing the payload.
    /// 描述 payload 的可选 JSON schema。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload_schema: Option<Value>,
    /// Additional point-specific metadata.
    /// 点位特有的额外元数据。
    #[serde(default)]
    pub metadata: BTreeMap<String, Value>,
}

impl HookPointDescriptor {
    /// Builds one hook point descriptor with accepted modes.
    /// 使用可接受模式构建一个 hook 点位描述。
    pub fn new(
        point: impl Into<HookPoint>,
        modes: impl IntoIterator<Item = HookMode>,
        description: impl Into<String>,
    ) -> Self {
        Self {
            point: point.into(),
            modes: modes.into_iter().collect(),
            default_scope: HookScope::default(),
            description: description.into(),
            payload_schema: None,
            metadata: BTreeMap::new(),
        }
    }

    /// Sets the default scope for this point descriptor.
    /// 设置该点位描述的默认作用域。
    pub fn with_default_scope(mut self, scope: HookScope) -> Self {
        self.default_scope = scope;
        self
    }

    /// Sets the optional payload schema for this point descriptor.
    /// 设置该点位描述的可选 payload schema。
    pub fn with_payload_schema(mut self, schema: Value) -> Self {
        self.payload_schema = Some(schema);
        self
    }
}

/// Describes the configuration layer that owns a hook registration.
/// 描述拥有某个 hook 注册项的配置层级。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HookScope {
    /// Registration applies to the whole application.
    /// 注册项应用于整个应用。
    Global,
    /// Registration applies to one workspace.
    /// 注册项应用于单个工作区。
    Workspace,
    /// Registration applies to one project.
    /// 注册项应用于单个项目。
    Project,
    /// Registration applies to one session.
    /// 注册项应用于单个会话。
    Session,
}

impl Default for HookScope {
    /// Returns the default scope for explicit runtime hook registrations.
    /// 返回显式运行时 hook 注册的默认作用域。
    fn default() -> Self {
        Self::Global
    }
}

/// Describes how dispatch should react when one hook fails.
/// 描述单个 hook 失败时调度器应如何响应。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HookFailurePolicy {
    /// Record the failure and continue running later hooks.
    /// 记录失败并继续运行后续 hook。
    Continue,
    /// Stop dispatch and return the failure to the caller.
    /// 停止调度并把失败返回给调用方。
    Stop,
    /// Record the failure as queueable work and continue running later hooks.
    /// 将失败记录为可入队工作并继续运行后续 hook。
    Queue,
}

impl Default for HookFailurePolicy {
    /// Returns the conservative failure policy that protects the main flow.
    /// 返回保护主流程的保守失败策略。
    fn default() -> Self {
        Self::Continue
    }
}

/// Describes a control-flow decision returned by a hook.
/// 描述 hook 返回的控制流决策。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HookDecision {
    /// Continue the runtime operation.
    /// 继续运行时操作。
    Continue,
    /// Block the runtime operation.
    /// 阻断运行时操作。
    Block,
    /// Defer the runtime operation to a later retry or user action.
    /// 将运行时操作延迟到后续重试或用户操作。
    Defer,
}

impl Default for HookDecision {
    /// Returns the default decision that keeps the runtime moving.
    /// 返回保持运行时继续前进的默认决策。
    fn default() -> Self {
        Self::Continue
    }
}

/// Declares one capability a hook may request from the host.
/// 声明 hook 可向宿主请求的一项能力。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HookCapability {
    /// Read visible conversation history.
    /// 读取可见会话历史。
    HistoryVisibleRead,
    /// Read filtered conversation history prepared for extensions.
    /// 读取为扩展准备的过滤后会话历史。
    HistoryFilteredRead,
    /// Read internal conversation history when explicitly granted.
    /// 在明确授权后读取内部会话历史。
    HistoryInternalRead,
    /// Read project and session configuration.
    /// 读取项目和会话配置。
    ConfigRead,
    /// Write project or session configuration.
    /// 写入项目或会话配置。
    ConfigWrite,
    /// Submit context injection operations.
    /// 提交上下文注入操作。
    InjectionWrite,
    /// Read summarized tool arguments and results.
    /// 读取工具参数和结果摘要。
    ToolSummaryRead,
    /// Read full tool arguments and results.
    /// 读取完整工具参数和结果。
    ToolFullRead,
    /// Use plugin-scoped persistent storage.
    /// 使用插件作用域持久化存储。
    StoragePlugin,
    /// Use outbound network access.
    /// 使用出站网络访问。
    Network,
}

/// Records capabilities granted to the current hook invocation.
/// 记录当前 hook 调用已授权的能力。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct HookCapabilitySet {
    /// Capability values granted by the host.
    /// 宿主授予的能力值。
    #[serde(default)]
    pub values: BTreeSet<HookCapability>,
}

impl HookCapabilitySet {
    /// Builds a capability set from an iterable list.
    /// 根据可迭代列表构建能力集合。
    pub fn from_values(values: impl IntoIterator<Item = HookCapability>) -> Self {
        Self {
            values: values.into_iter().collect(),
        }
    }

    /// Returns true when the capability has been granted.
    /// 当指定能力已被授予时返回 true。
    pub fn contains(&self, capability: &HookCapability) -> bool {
        self.values.contains(capability)
    }
}

/// Describes where a hook invocation originated.
/// 描述一次 hook 调用的来源。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HookActor {
    /// Stable source label such as `runtime`, `ui`, or a plugin id.
    /// 稳定来源标签，例如 `runtime`、`ui` 或插件 id。
    pub source: String,
    /// Optional human-readable source label.
    /// 可选的人类可读来源标签。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display: Option<String>,
}

impl HookActor {
    /// Builds a hook actor from a stable source label.
    /// 根据稳定来源标签构建 hook 调用来源。
    pub fn new(source: impl Into<String>) -> Self {
        Self {
            source: source.into(),
            display: None,
        }
    }
}

/// Carries common identity and lookup data for one hook invocation.
/// 承载一次 hook 调用的通用身份和查询数据。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HookContext {
    /// Unique id for this dispatch run.
    /// 本次调度运行的唯一标识。
    pub run_id: String,
    /// Hook point being dispatched.
    /// 正在调度的 hook 点位。
    pub point: HookPoint,
    /// Scope selected for this invocation.
    /// 本次调用选择的作用域。
    pub scope: HookScope,
    /// Actor that produced the invocation.
    /// 产生本次调用的行为来源。
    pub actor: HookActor,
    /// Optional workspace identifier.
    /// 可选工作区标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_id: Option<String>,
    /// Optional project identifier.
    /// 可选项目标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<HookProjectId>,
    /// Optional session identifier.
    /// 可选会话标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<HookSessionId>,
    /// Optional conversation identifier.
    /// 可选对话标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conversation_id: Option<String>,
    /// Optional user identifier.
    /// 可选用户标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
    /// Capabilities granted to handlers during this invocation.
    /// 本次调用授予处理器的能力集合。
    #[serde(default)]
    pub capabilities: HookCapabilitySet,
    /// Additional host-provided context metadata.
    /// 宿主提供的额外上下文元数据。
    #[serde(default)]
    pub metadata: BTreeMap<String, Value>,
}

impl HookContext {
    /// Builds a minimal context for one hook point.
    /// 为单个 hook 点位构建最小上下文。
    pub fn new(run_id: impl Into<String>, point: impl Into<HookPoint>) -> Self {
        let point = point.into();
        Self {
            run_id: run_id.into(),
            point,
            scope: HookScope::Global,
            actor: HookActor::new("runtime"),
            workspace_id: None,
            project_id: None,
            session_id: None,
            conversation_id: None,
            user_id: None,
            capabilities: HookCapabilitySet::default(),
            metadata: BTreeMap::new(),
        }
    }
}

/// Wraps the mutable payload passed through transform-capable hook chains.
/// 包装可被 transform hook 链传递和替换的可变 payload。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HookPayload {
    /// JSON value carried by the hook point.
    /// hook 点位携带的 JSON 值。
    pub value: Value,
}

impl HookPayload {
    /// Builds a payload from any serializable value.
    /// 根据任意可序列化值构建 payload。
    pub fn from_serializable<T>(value: T) -> Result<Self, HookError>
    where
        T: Serialize,
    {
        Ok(Self {
            value: serde_json::to_value(value).map_err(HookError::serialization)?,
        })
    }

    /// Builds an empty object payload.
    /// 构建一个空对象 payload。
    pub fn empty_object() -> Self {
        Self {
            value: Value::Object(Map::new()),
        }
    }
}

impl Default for HookPayload {
    /// Returns an empty object payload.
    /// 返回空对象 payload。
    fn default() -> Self {
        Self::empty_object()
    }
}

/// Diagnostic entry produced by a hook or dispatcher.
/// 由 hook 或调度器产生的诊断条目。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HookDiagnostic {
    /// Stable diagnostic severity.
    /// 稳定诊断等级。
    pub severity: HookDiagnosticSeverity,
    /// Human-readable diagnostic message.
    /// 人类可读诊断消息。
    pub message: String,
    /// Optional hook id that produced the diagnostic.
    /// 产生该诊断的可选 hook 标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hook_id: Option<HookId>,
}

impl HookDiagnostic {
    /// Builds an informational diagnostic message.
    /// 构建一条信息级诊断消息。
    pub fn info(message: impl Into<String>) -> Self {
        Self {
            severity: HookDiagnosticSeverity::Info,
            message: message.into(),
            hook_id: None,
        }
    }
}

/// Stable diagnostic severity values.
/// 稳定诊断等级值。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HookDiagnosticSeverity {
    /// Informational diagnostic.
    /// 信息级诊断。
    Info,
    /// Warning diagnostic.
    /// 警告级诊断。
    Warning,
    /// Error diagnostic.
    /// 错误级诊断。
    Error,
}

/// Result returned by one hook handler.
/// 单个 hook 处理器返回的结果。
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct HookResult {
    /// Optional payload replacement used by transform hooks.
    /// transform hook 使用的可选 payload 替换值。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub payload: Option<HookPayload>,
    /// Optional control-flow decision used by decision hooks.
    /// decision hook 使用的可选控制流决策。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decision: Option<HookDecision>,
    /// Context injection operations submitted by the hook.
    /// hook 提交的上下文注入操作。
    #[serde(default)]
    pub injections: Vec<InjectionInsert>,
    /// Diagnostics produced by the hook.
    /// hook 产生的诊断信息。
    #[serde(default)]
    pub diagnostics: Vec<HookDiagnostic>,
    /// Additional hook-specific result data.
    /// hook 特有的额外结果数据。
    #[serde(default)]
    pub metadata: BTreeMap<String, Value>,
}

impl HookResult {
    /// Builds a result that replaces the current payload.
    /// 构建一个替换当前 payload 的结果。
    pub fn with_payload(payload: HookPayload) -> Self {
        Self {
            payload: Some(payload),
            ..Self::default()
        }
    }

    /// Builds a result that returns one control-flow decision.
    /// 构建一个返回控制流决策的结果。
    pub fn with_decision(decision: HookDecision) -> Self {
        Self {
            decision: Some(decision),
            ..Self::default()
        }
    }
}

/// Registration metadata for one hook handler.
/// 单个 hook 处理器的注册元数据。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HookRegistration {
    /// Stable hook handler identifier.
    /// 稳定 hook 处理器标识。
    pub id: HookId,
    /// Hook point this handler subscribes to.
    /// 该处理器订阅的 hook 点位。
    pub point: HookPoint,
    /// Behavior mode declared by the handler.
    /// 处理器声明的行为模式。
    pub mode: HookMode,
    /// Scope that owns this registration.
    /// 持有该注册项的作用域。
    #[serde(default)]
    pub scope: HookScope,
    /// Lower priority values run earlier.
    /// 优先级数值越低越早执行。
    #[serde(default)]
    pub priority: i32,
    /// Optional timeout in milliseconds for this handler.
    /// 该处理器的可选超时时间，单位毫秒。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
    /// Failure policy used when this handler errors or times out.
    /// 该处理器报错或超时时使用的失败策略。
    #[serde(default)]
    pub failure_policy: HookFailurePolicy,
    /// Capabilities requested by this handler.
    /// 该处理器请求的能力集合。
    #[serde(default)]
    pub requested_capabilities: BTreeSet<HookCapability>,
    /// Optional human-readable label.
    /// 可选的人类可读标签。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    /// Optional short progress text shown while this hook is running in a blocking preflight stage.
    /// 该 hook 在阻塞式前置阶段运行时展示的可选短进度文本。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub progress_label: Option<String>,
}

impl HookRegistration {
    /// Builds a minimal hook registration.
    /// 构建一个最小 hook 注册项。
    pub fn new(id: impl Into<HookId>, point: impl Into<HookPoint>, mode: HookMode) -> Self {
        Self {
            id: id.into(),
            point: point.into(),
            mode,
            scope: HookScope::default(),
            priority: 0,
            timeout_ms: None,
            failure_policy: HookFailurePolicy::default(),
            requested_capabilities: BTreeSet::new(),
            display_name: None,
            progress_label: None,
        }
    }

    /// Returns the timeout duration configured for this handler.
    /// 返回该处理器配置的超时时长。
    fn timeout_duration(&self) -> Option<Duration> {
        self.timeout_ms.map(Duration::from_millis)
    }
}

/// Read-only snapshot of one registered hook handler.
/// 单个已注册 hook 处理器的只读快照。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HookRegistrationSnapshot {
    /// Registration metadata visible to extension managers.
    /// 扩展管理器可见的注册元数据。
    pub registration: HookRegistration,
    /// Stable insertion sequence used for deterministic ordering.
    /// 用于确定性排序的稳定插入序号。
    pub sequence: u64,
}

/// Describes observable lifecycle events emitted by the hook system.
/// 描述 hook 系统发出的可观测生命周期事件。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HookEventKind {
    /// A hook handler was registered.
    /// 一个 hook 处理器已注册。
    Registered,
    /// A hook handler was unregistered.
    /// 一个 hook 处理器已注销。
    Unregistered,
    /// Dispatch for one hook point has started.
    /// 单个 hook 点位的调度已开始。
    DispatchStarted,
    /// One hook handler is about to run.
    /// 单个 hook 处理器即将运行。
    HandlerStarted,
    /// One hook handler completed successfully.
    /// 单个 hook 处理器已成功完成。
    HandlerCompleted,
    /// One hook handler failed or timed out.
    /// 单个 hook 处理器失败或超时。
    HandlerFailed,
    /// Dispatch for one hook point has completed.
    /// 单个 hook 点位的调度已完成。
    DispatchCompleted,
}

/// Observable lifecycle event emitted by hook registration and dispatch.
/// hook 注册与调度发出的可观测生命周期事件。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HookEvent {
    /// Lifecycle event kind.
    /// 生命周期事件类型。
    pub kind: HookEventKind,
    /// Unix timestamp in milliseconds when the event was emitted.
    /// 事件发出时的 Unix 毫秒时间戳。
    pub timestamp_unix_ms: u128,
    /// Hook point associated with the event.
    /// 与事件关联的 hook 点位。
    pub point: HookPoint,
    /// Optional dispatch run id.
    /// 可选调度运行标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_id: Option<String>,
    /// Optional hook handler id.
    /// 可选 hook 处理器标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hook_id: Option<HookId>,
    /// Optional human-readable event message.
    /// 可选人类可读事件消息。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    /// Additional event-specific metadata.
    /// 事件特有的额外元数据。
    #[serde(default)]
    pub metadata: BTreeMap<String, Value>,
}

impl HookEvent {
    /// Builds a hook lifecycle event.
    /// 构建一个 hook 生命周期事件。
    pub fn new(kind: HookEventKind, point: impl Into<HookPoint>) -> Self {
        Self {
            kind,
            timestamp_unix_ms: current_unix_millis(),
            point: point.into(),
            run_id: None,
            hook_id: None,
            message: None,
            metadata: BTreeMap::new(),
        }
    }

    /// Attaches a dispatch run id to the event.
    /// 为事件附加调度运行标识。
    pub fn with_run_id(mut self, run_id: impl Into<String>) -> Self {
        self.run_id = Some(run_id.into());
        self
    }

    /// Attaches a hook handler id to the event.
    /// 为事件附加 hook 处理器标识。
    pub fn with_hook_id(mut self, hook_id: impl Into<HookId>) -> Self {
        self.hook_id = Some(hook_id.into());
        self
    }

    /// Attaches a human-readable message to the event.
    /// 为事件附加人类可读消息。
    pub fn with_message(mut self, message: impl Into<String>) -> Self {
        self.message = Some(message.into());
        self
    }
}

/// Trait implemented by lifecycle event listeners.
/// 生命周期事件监听器实现的 trait。
pub trait HookEventListener: Send + Sync {
    /// Handles one hook lifecycle event.
    /// 处理一个 hook 生命周期事件。
    fn on_hook_event(&self, event: HookEvent);
}

/// Function-backed hook lifecycle event listener.
/// 基于函数的 hook 生命周期事件监听器。
pub struct FnHookEventListener<F> {
    /// Function invoked for each lifecycle event.
    /// 每个生命周期事件触发时执行的函数。
    function: F,
}

impl<F> FnHookEventListener<F> {
    /// Builds a function-backed lifecycle event listener.
    /// 构建一个基于函数的生命周期事件监听器。
    pub fn new(function: F) -> Self {
        Self { function }
    }
}

impl<F> HookEventListener for FnHookEventListener<F>
where
    F: Send + Sync + Fn(HookEvent),
{
    /// Runs the wrapped function for one lifecycle event.
    /// 为一个生命周期事件运行被包装函数。
    fn on_hook_event(&self, event: HookEvent) {
        (self.function)(event);
    }
}

/// Request passed to one hook handler.
/// 传递给单个 hook 处理器的请求。
#[derive(Debug, Clone)]
pub struct HookInvocation {
    /// Registration metadata for the current handler.
    /// 当前处理器的注册元数据。
    pub registration: HookRegistration,
    /// Common invocation context.
    /// 通用调用上下文。
    pub context: HookContext,
    /// Current payload value.
    /// 当前 payload 值。
    pub payload: HookPayload,
}

/// Trait implemented by hook handlers.
/// hook 处理器实现的 trait。
#[async_trait]
pub trait HookHandler: Send + Sync {
    /// Runs the handler for one invocation and returns its contribution.
    /// 为一次调用运行处理器并返回其贡献结果。
    async fn handle(&self, invocation: HookInvocation) -> Result<HookResult, HookError>;
}

/// Function-backed hook handler.
/// 基于函数的 hook 处理器。
pub struct FnHookHandler<F> {
    /// Function invoked for each hook call.
    /// 每次 hook 调用时执行的函数。
    function: F,
}

impl<F> FnHookHandler<F> {
    /// Builds a function-backed hook handler.
    /// 构建一个基于函数的 hook 处理器。
    pub fn new(function: F) -> Self {
        Self { function }
    }
}

#[async_trait]
impl<F, Fut> HookHandler for FnHookHandler<F>
where
    F: Send + Sync + Fn(HookInvocation) -> Fut,
    Fut: Send + std::future::Future<Output = Result<HookResult, HookError>>,
{
    /// Runs the wrapped function for one hook invocation.
    /// 为一次 hook 调用运行被包装函数。
    async fn handle(&self, invocation: HookInvocation) -> Result<HookResult, HookError> {
        (self.function)(invocation).await
    }
}

/// Runtime entry combining metadata, handler, and insertion order.
/// 组合元数据、处理器和插入顺序的运行时条目。
#[derive(Clone)]
struct HookEntry {
    /// Registration metadata.
    /// 注册元数据。
    registration: HookRegistration,
    /// Handler implementation.
    /// 处理器实现。
    handler: Arc<dyn HookHandler>,
    /// Monotonic insertion sequence for stable ordering.
    /// 用于稳定排序的单调插入序号。
    sequence: u64,
}

/// Registry that stores hook handlers by point.
/// 按点位存储 hook 处理器的注册表。
#[derive(Clone, Default)]
pub struct HookRegistry {
    /// Known hook point descriptors keyed by hook point.
    /// 按 hook 点位索引的已知 hook 点位描述。
    points: BTreeMap<HookPoint, HookPointDescriptor>,
    /// Registered hook entries grouped by hook point.
    /// 按 hook 点位分组的已注册条目。
    entries_by_point: BTreeMap<HookPoint, Vec<HookEntry>>,
    /// Next insertion sequence assigned to a registration.
    /// 下一个分配给注册项的插入序号。
    next_sequence: u64,
}

impl HookRegistry {
    /// Builds an empty hook registry.
    /// 构建一个空 hook 注册表。
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers or replaces one hook point descriptor.
    /// 注册或替换一个 hook 点位描述。
    pub fn register_point(&mut self, descriptor: HookPointDescriptor) -> Result<(), HookError> {
        if descriptor.point.trim().is_empty() {
            return Err(HookError::invalid_registration(
                "hook point descriptor name cannot be empty",
            ));
        }
        if descriptor.modes.is_empty() {
            return Err(HookError::invalid_registration(
                "hook point descriptor must allow at least one mode",
            ));
        }
        self.points.insert(descriptor.point.clone(), descriptor);
        Ok(())
    }

    /// Registers or replaces many hook point descriptors.
    /// 注册或替换多个 hook 点位描述。
    pub fn register_points(
        &mut self,
        descriptors: impl IntoIterator<Item = HookPointDescriptor>,
    ) -> Result<(), HookError> {
        for descriptor in descriptors {
            self.register_point(descriptor)?;
        }
        Ok(())
    }

    /// Returns one hook point descriptor when known.
    /// 返回一个已知 hook 点位描述。
    pub fn point_descriptor(&self, point: &str) -> Option<&HookPointDescriptor> {
        self.points.get(point)
    }

    /// Returns every known hook point descriptor in stable order.
    /// 以稳定顺序返回所有已知 hook 点位描述。
    pub fn point_descriptors(&self) -> Vec<HookPointDescriptor> {
        self.points.values().cloned().collect()
    }

    /// Registers one hook handler.
    /// 注册一个 hook 处理器。
    pub fn register(
        &mut self,
        registration: HookRegistration,
        handler: Arc<dyn HookHandler>,
    ) -> Result<(), HookError> {
        if registration.id.trim().is_empty() {
            return Err(HookError::invalid_registration("hook id cannot be empty"));
        }
        if registration.point.trim().is_empty() {
            return Err(HookError::invalid_registration(
                "hook point cannot be empty",
            ));
        }
        if self
            .entries_by_point
            .values()
            .flat_map(|entries| entries.iter())
            .any(|entry| entry.registration.id == registration.id)
        {
            return Err(HookError::invalid_registration(format!(
                "hook id already registered: {}",
                registration.id
            )));
        }
        if let Some(descriptor) = self.points.get(&registration.point)
            && !descriptor.modes.contains(&registration.mode)
        {
            return Err(HookError::invalid_registration(format!(
                "hook mode {:?} is not allowed for point {}",
                registration.mode, registration.point
            )));
        }

        let entry = HookEntry {
            registration: registration.clone(),
            handler,
            sequence: self.next_sequence,
        };
        self.next_sequence = self.next_sequence.saturating_add(1);
        self.entries_by_point
            .entry(registration.point.clone())
            .or_default()
            .push(entry);
        Ok(())
    }

    /// Unregisters hook handlers by id and optional point filter.
    /// 按标识和可选点位过滤注销 hook 处理器。
    pub fn unregister(&mut self, hook_id: &str, point: Option<&str>) -> usize {
        let mut removed = 0;
        let target_points = if let Some(point) = point {
            vec![point.to_string()]
        } else {
            self.entries_by_point.keys().cloned().collect::<Vec<_>>()
        };
        for target_point in target_points {
            if let Some(entries) = self.entries_by_point.get_mut(&target_point) {
                let before = entries.len();
                entries.retain(|entry| entry.registration.id != hook_id);
                removed += before.saturating_sub(entries.len());
            }
        }
        self.entries_by_point
            .retain(|_, entries| !entries.is_empty());
        removed
    }

    /// Returns sorted entries for one hook point.
    /// 返回单个 hook 点位的已排序条目。
    fn entries_for(&self, point: &str) -> Vec<HookEntry> {
        let mut entries = self
            .entries_by_point
            .get(point)
            .cloned()
            .unwrap_or_default();
        entries.sort_by_key(|entry| (entry.registration.priority, entry.sequence));
        entries
    }

    /// Returns the number of handlers registered for one hook point.
    /// 返回单个 hook 点位注册的处理器数量。
    pub fn count_for(&self, point: &str) -> usize {
        self.entries_by_point.get(point).map_or(0, Vec::len)
    }

    /// Returns read-only registration snapshots for one point or all points.
    /// 返回单个点位或所有点位的只读注册快照。
    pub fn registration_snapshots(&self, point: Option<&str>) -> Vec<HookRegistrationSnapshot> {
        let mut snapshots = self
            .entries_by_point
            .iter()
            .filter(|(entry_point, _)| point.is_none_or(|point| point == entry_point.as_str()))
            .flat_map(|(_, entries)| entries.iter())
            .map(|entry| HookRegistrationSnapshot {
                registration: entry.registration.clone(),
                sequence: entry.sequence,
            })
            .collect::<Vec<_>>();
        snapshots.sort_by_key(|snapshot| {
            (
                snapshot.registration.point.clone(),
                snapshot.registration.priority,
                snapshot.sequence,
            )
        });
        snapshots
    }
}

/// Request passed to the dispatcher for one hook point.
/// 传递给调度器的一次 hook 点位请求。
#[derive(Debug, Clone)]
pub struct HookDispatchRequest {
    /// Common hook context.
    /// 通用 hook 上下文。
    pub context: HookContext,
    /// Initial payload value.
    /// 初始 payload 值。
    pub payload: HookPayload,
}

impl HookDispatchRequest {
    /// Builds a dispatch request from context and payload.
    /// 根据上下文和 payload 构建调度请求。
    pub fn new(context: HookContext, payload: HookPayload) -> Self {
        Self { context, payload }
    }
}

/// Aggregated result returned by dispatching one hook point.
/// 调度单个 hook 点位后返回的聚合结果。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HookDispatchReport {
    /// Final payload after transform hooks.
    /// transform hook 处理后的最终 payload。
    pub payload: HookPayload,
    /// Effective decision after decision hooks.
    /// decision hook 处理后的有效决策。
    pub decision: HookDecision,
    /// Context injections collected from handlers.
    /// 从处理器收集到的上下文注入操作。
    #[serde(default)]
    pub injections: Vec<InjectionInsert>,
    /// Diagnostics collected from handlers and dispatcher failures.
    /// 从处理器和调度失败中收集的诊断信息。
    #[serde(default)]
    pub diagnostics: Vec<HookDiagnostic>,
    /// Hook-specific metadata collected from successful handlers and grouped by key.
    /// 从成功处理器收集并按键分组的 hook 特定元数据。
    #[serde(default)]
    pub metadata: BTreeMap<String, Vec<Value>>,
    /// Handler ids that completed successfully.
    /// 成功完成的处理器标识。
    #[serde(default)]
    pub completed_hooks: Vec<HookId>,
    /// Handler failures collected during dispatch.
    /// 调度过程中收集到的处理器失败信息。
    #[serde(default)]
    pub failures: Vec<HookFailure>,
    /// True when a queueable failure was recorded.
    /// 当记录了可入队失败时为 true。
    pub has_queueable_failure: bool,
}

impl HookDispatchReport {
    /// Builds a public empty report initialized from the provided payload.
    /// 根据给定 payload 构建一个公开的空报告。
    pub fn empty(payload: HookPayload) -> Self {
        Self::new(payload)
    }

    /// Builds a report initialized from the starting payload.
    /// 根据起始 payload 构建报告。
    fn new(payload: HookPayload) -> Self {
        Self {
            payload,
            decision: HookDecision::Continue,
            injections: Vec::new(),
            diagnostics: Vec::new(),
            metadata: BTreeMap::new(),
            completed_hooks: Vec::new(),
            failures: Vec::new(),
            has_queueable_failure: false,
        }
    }

    /// Applies one hook result to the accumulated report.
    /// 将单个 hook 结果应用到聚合报告。
    fn apply_result(&mut self, hook_id: &str, result: HookResult) {
        if let Some(payload) = result.payload {
            self.payload = payload;
        }
        if let Some(decision) = result.decision {
            self.decision = decision;
        }
        self.injections.extend(result.injections);
        for (key, value) in result.metadata {
            self.metadata.entry(key).or_default().push(value);
        }
        self.diagnostics
            .extend(result.diagnostics.into_iter().map(|mut diagnostic| {
                if diagnostic.hook_id.is_none() {
                    diagnostic.hook_id = Some(hook_id.to_owned());
                }
                diagnostic
            }));
    }

    /// Records a hook failure and updates queue state when needed.
    /// 记录 hook 失败，并在需要时更新入队状态。
    fn record_failure(&mut self, failure: HookFailure) {
        if failure.policy == HookFailurePolicy::Queue {
            self.has_queueable_failure = true;
        }
        self.diagnostics.push(HookDiagnostic {
            severity: HookDiagnosticSeverity::Error,
            message: failure.message.clone(),
            hook_id: Some(failure.hook_id.clone()),
        });
        self.failures.push(failure);
    }
}

/// Hook dispatcher that runs registered handlers.
/// 运行已注册处理器的 hook 调度器。
#[derive(Clone, Default)]
pub struct HookDispatcher {
    /// Registry used for handler lookup.
    /// 用于处理器查询的注册表。
    registry: HookRegistry,
    /// Lifecycle event listeners notified by registration and dispatch.
    /// 注册与调度时通知的生命周期事件监听器。
    listeners: BTreeMap<HookListenerId, Arc<dyn HookEventListener>>,
    /// Next generated listener sequence.
    /// 下一个生成的监听器序号。
    next_listener_sequence: u64,
}

impl HookDispatcher {
    /// Builds a dispatcher from a registry.
    /// 根据注册表构建调度器。
    pub fn new(registry: HookRegistry) -> Self {
        Self {
            registry,
            listeners: BTreeMap::new(),
            next_listener_sequence: 0,
        }
    }

    /// Returns the dispatcher registry.
    /// 返回调度器注册表。
    pub fn registry(&self) -> &HookRegistry {
        &self.registry
    }

    /// Returns a mutable reference to the dispatcher registry.
    /// 返回调度器注册表的可变引用。
    pub fn registry_mut(&mut self) -> &mut HookRegistry {
        &mut self.registry
    }

    /// Registers or replaces one hook point descriptor.
    /// 注册或替换一个 hook 点位描述。
    pub fn register_point(&mut self, descriptor: HookPointDescriptor) -> Result<(), HookError> {
        self.registry.register_point(descriptor)
    }

    /// Registers or replaces many hook point descriptors.
    /// 注册或替换多个 hook 点位描述。
    pub fn register_points(
        &mut self,
        descriptors: impl IntoIterator<Item = HookPointDescriptor>,
    ) -> Result<(), HookError> {
        self.registry.register_points(descriptors)
    }

    /// Adds one lifecycle event listener.
    /// 添加一个生命周期事件监听器。
    pub fn add_listener(&mut self, listener: Arc<dyn HookEventListener>) -> HookListenerId {
        self.next_listener_sequence = self.next_listener_sequence.saturating_add(1);
        let listener_id = format!("listener-{}", self.next_listener_sequence);
        self.add_listener_with_id(listener_id.clone(), listener)
            .expect("generated listener id should be valid");
        listener_id
    }

    /// Adds or replaces one lifecycle event listener with a stable id.
    /// 使用稳定标识添加或替换一个生命周期事件监听器。
    pub fn add_listener_with_id(
        &mut self,
        listener_id: impl Into<HookListenerId>,
        listener: Arc<dyn HookEventListener>,
    ) -> Result<(), HookError> {
        let listener_id = listener_id.into();
        if listener_id.trim().is_empty() {
            return Err(HookError::invalid_registration(
                "hook listener id cannot be empty",
            ));
        }
        self.listeners.insert(listener_id, listener);
        Ok(())
    }

    /// Removes one lifecycle event listener by id.
    /// 按标识移除一个生命周期事件监听器。
    pub fn remove_listener(&mut self, listener_id: &str) -> bool {
        self.listeners.remove(listener_id).is_some()
    }

    /// Registers one hook handler and emits a registration event.
    /// 注册一个 hook 处理器并发出注册事件。
    pub fn register(
        &mut self,
        registration: HookRegistration,
        handler: Arc<dyn HookHandler>,
    ) -> Result<(), HookError> {
        let point = registration.point.clone();
        let hook_id = registration.id.clone();
        self.registry.register(registration, handler)?;
        self.emit_event(HookEvent::new(HookEventKind::Registered, point).with_hook_id(hook_id));
        Ok(())
    }

    /// Unregisters matching hook handlers and emits unregistration events.
    /// 注销匹配的 hook 处理器并发出注销事件。
    pub fn unregister(&mut self, hook_id: &str, point: Option<&str>) -> usize {
        let snapshots = self.registry.registration_snapshots(point);
        let removed = self.registry.unregister(hook_id, point);
        if removed == 0 {
            return 0;
        }
        for snapshot in snapshots
            .into_iter()
            .filter(|snapshot| snapshot.registration.id == hook_id)
        {
            self.emit_event(
                HookEvent::new(HookEventKind::Unregistered, snapshot.registration.point)
                    .with_hook_id(snapshot.registration.id),
            );
        }
        removed
    }

    /// Dispatches one hook point and returns the aggregated report.
    /// 调度单个 hook 点位并返回聚合报告。
    pub async fn dispatch(
        &self,
        request: HookDispatchRequest,
    ) -> Result<HookDispatchReport, HookError> {
        let entries = self.registry.entries_for(&request.context.point);
        let mut report = HookDispatchReport::new(request.payload);
        self.emit_event(
            HookEvent::new(
                HookEventKind::DispatchStarted,
                request.context.point.clone(),
            )
            .with_run_id(request.context.run_id.clone()),
        );

        for entry in entries {
            let hook_id = entry.registration.id.clone();
            self.emit_event(
                HookEvent::new(HookEventKind::HandlerStarted, request.context.point.clone())
                    .with_run_id(request.context.run_id.clone())
                    .with_hook_id(hook_id.clone()),
            );
            let invocation = HookInvocation {
                registration: entry.registration.clone(),
                context: request.context.clone(),
                payload: report.payload.clone(),
            };
            let result = run_entry(entry.clone(), invocation).await;
            match result {
                Ok(result) => {
                    report.apply_result(&hook_id, result);
                    report.completed_hooks.push(hook_id.clone());
                    self.emit_event(
                        HookEvent::new(
                            HookEventKind::HandlerCompleted,
                            request.context.point.clone(),
                        )
                        .with_run_id(request.context.run_id.clone())
                        .with_hook_id(hook_id),
                    );
                    if report.decision != HookDecision::Continue {
                        break;
                    }
                }
                Err(error) => {
                    let failure = HookFailure {
                        hook_id: hook_id.clone(),
                        policy: entry.registration.failure_policy,
                        message: error.to_string(),
                    };
                    let should_stop = failure.policy == HookFailurePolicy::Stop;
                    self.emit_event(
                        HookEvent::new(HookEventKind::HandlerFailed, request.context.point.clone())
                            .with_run_id(request.context.run_id.clone())
                            .with_hook_id(hook_id)
                            .with_message(failure.message.clone()),
                    );
                    report.record_failure(failure);
                    if should_stop {
                        return Err(HookError::dispatch_stopped(report.failures));
                    }
                }
            }
        }

        self.emit_event(
            HookEvent::new(HookEventKind::DispatchCompleted, request.context.point)
                .with_run_id(request.context.run_id),
        );
        Ok(report)
    }

    /// Emits one lifecycle event to every registered listener.
    /// 向所有已注册监听器发出一个生命周期事件。
    fn emit_event(&self, event: HookEvent) {
        for listener in self.listeners.values() {
            listener.on_hook_event(event.clone());
        }
    }
}

/// Runs one hook entry with its configured timeout.
/// 按配置超时时间运行单个 hook 条目。
async fn run_entry(entry: HookEntry, invocation: HookInvocation) -> Result<HookResult, HookError> {
    let future = entry.handler.handle(invocation);
    if let Some(duration) = entry.registration.timeout_duration() {
        time::timeout(duration, future)
            .await
            .map_err(|_| HookError::timeout(entry.registration.id))?
    } else {
        future.await
    }
}

/// Returns the current Unix timestamp in milliseconds.
/// 返回当前 Unix 毫秒时间戳。
fn current_unix_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or_default()
}

/// Failure details captured during hook dispatch.
/// hook 调度过程中捕获的失败详情。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HookFailure {
    /// Hook id that failed.
    /// 失败的 hook 标识。
    pub hook_id: HookId,
    /// Failure policy active for that hook.
    /// 该 hook 生效的失败策略。
    pub policy: HookFailurePolicy,
    /// Human-readable failure message.
    /// 人类可读失败消息。
    pub message: String,
}

/// Error type used by hook registration and dispatch.
/// hook 注册和调度使用的错误类型。
#[derive(Debug)]
pub enum HookError {
    /// Registration metadata was invalid.
    /// 注册元数据无效。
    InvalidRegistration(String),
    /// Serialization or deserialization failed.
    /// 序列化或反序列化失败。
    Serialization(serde_json::Error),
    /// Handler returned a runtime error.
    /// 处理器返回运行时错误。
    Handler(String),
    /// Handler did not finish before its timeout.
    /// 处理器未在超时时间前完成。
    Timeout(HookId),
    /// Dispatch stopped because a stop-policy hook failed.
    /// 调度因 stop 策略 hook 失败而停止。
    DispatchStopped(Vec<HookFailure>),
}

impl HookError {
    /// Builds an invalid-registration error.
    /// 构建注册无效错误。
    pub fn invalid_registration(message: impl Into<String>) -> Self {
        Self::InvalidRegistration(message.into())
    }

    /// Builds a serialization error.
    /// 构建序列化错误。
    pub fn serialization(error: serde_json::Error) -> Self {
        Self::Serialization(error)
    }

    /// Builds a handler error.
    /// 构建处理器错误。
    pub fn handler(message: impl Into<String>) -> Self {
        Self::Handler(message.into())
    }

    /// Builds a timeout error.
    /// 构建超时错误。
    pub fn timeout(hook_id: impl Into<HookId>) -> Self {
        Self::Timeout(hook_id.into())
    }

    /// Builds a dispatch-stopped error.
    /// 构建调度停止错误。
    pub fn dispatch_stopped(failures: Vec<HookFailure>) -> Self {
        Self::DispatchStopped(failures)
    }
}

impl Display for HookError {
    /// Formats hook errors for diagnostics.
    /// 为诊断格式化 hook 错误。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidRegistration(message) => {
                write!(formatter, "invalid hook registration: {message}")
            }
            Self::Serialization(error) => write!(formatter, "hook serialization failed: {error}"),
            Self::Handler(message) => write!(formatter, "hook handler failed: {message}"),
            Self::Timeout(hook_id) => write!(formatter, "hook timed out: {hook_id}"),
            Self::DispatchStopped(failures) => {
                write!(
                    formatter,
                    "hook dispatch stopped after {} failure(s)",
                    failures.len()
                )
            }
        }
    }
}

impl Error for HookError {
    /// Returns the nested serialization source when available.
    /// 当存在嵌套序列化来源时返回它。
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Serialization(error) => Some(error),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    //! Unit tests for the shared hook dispatcher.
    //! 共享 hook 调度器的单元测试。

    use super::*;
    use serde_json::json;
    use std::sync::Mutex;

    /// Builds a registration for the test hook point.
    /// 为测试 hook 点位构建注册项。
    fn registration(id: &str, mode: HookMode) -> HookRegistration {
        HookRegistration::new(id, "conversation.turn.precheck", mode)
    }

    /// Builds a dispatch request for the test hook point.
    /// 为测试 hook 点位构建调度请求。
    fn request() -> HookDispatchRequest {
        HookDispatchRequest::new(
            HookContext::new("run-1", "conversation.turn.precheck"),
            HookPayload::from_serializable(json!({"value": 1})).expect("payload should serialize"),
        )
    }

    #[tokio::test]
    async fn dispatcher_runs_hooks_by_priority_then_registration_order() {
        let order = Arc::new(Mutex::new(Vec::<String>::new()));
        let mut registry = HookRegistry::new();

        for (id, priority) in [("late", 10), ("early", -1), ("middle", 10)] {
            let order = Arc::clone(&order);
            let mut hook = registration(id, HookMode::Observe);
            hook.priority = priority;
            registry
                .register(
                    hook,
                    Arc::new(FnHookHandler::new(move |invocation: HookInvocation| {
                        let order = Arc::clone(&order);
                        async move {
                            order
                                .lock()
                                .expect("order lock should not be poisoned")
                                .push(invocation.registration.id);
                            Ok(HookResult::default())
                        }
                    })),
                )
                .expect("registration should succeed");
        }

        HookDispatcher::new(registry)
            .dispatch(request())
            .await
            .expect("dispatch should succeed");

        assert_eq!(
            *order.lock().expect("order lock should not be poisoned"),
            vec!["early", "late", "middle"]
        );
    }

    #[tokio::test]
    async fn transform_hook_replaces_payload_for_later_handlers() {
        let mut registry = HookRegistry::new();
        registry
            .register(
                registration("replace", HookMode::Transform),
                Arc::new(FnHookHandler::new(|_invocation: HookInvocation| async {
                    Ok(HookResult::with_payload(
                        HookPayload::from_serializable(json!({"value": 2}))
                            .expect("payload should serialize"),
                    ))
                })),
            )
            .expect("registration should succeed");
        registry
            .register(
                registration("read", HookMode::Observe),
                Arc::new(FnHookHandler::new(
                    |invocation: HookInvocation| async move {
                        assert_eq!(invocation.payload.value["value"], json!(2));
                        Ok(HookResult::default())
                    },
                )),
            )
            .expect("registration should succeed");

        let report = HookDispatcher::new(registry)
            .dispatch(request())
            .await
            .expect("dispatch should succeed");

        assert_eq!(report.payload.value["value"], json!(2));
        assert_eq!(report.completed_hooks, vec!["replace", "read"]);
    }

    #[tokio::test]
    async fn decision_hook_blocks_later_handlers() {
        let mut registry = HookRegistry::new();
        registry
            .register(
                registration("block", HookMode::Decision),
                Arc::new(FnHookHandler::new(|_invocation: HookInvocation| async {
                    Ok(HookResult::with_decision(HookDecision::Block))
                })),
            )
            .expect("registration should succeed");
        registry
            .register(
                registration("after", HookMode::Observe),
                Arc::new(FnHookHandler::new(|_invocation: HookInvocation| async {
                    Ok(HookResult::default())
                })),
            )
            .expect("registration should succeed");

        let report = HookDispatcher::new(registry)
            .dispatch(request())
            .await
            .expect("dispatch should succeed");

        assert_eq!(report.decision, HookDecision::Block);
        assert_eq!(report.completed_hooks, vec!["block"]);
    }

    #[tokio::test]
    async fn effect_failure_can_continue_and_report_diagnostics() {
        let mut registry = HookRegistry::new();
        registry
            .register(
                registration("failing-effect", HookMode::Effect),
                Arc::new(FnHookHandler::new(|_invocation: HookInvocation| async {
                    Err(HookError::handler("writeback failed"))
                })),
            )
            .expect("registration should succeed");
        registry
            .register(
                registration("after", HookMode::Observe),
                Arc::new(FnHookHandler::new(|_invocation: HookInvocation| async {
                    Ok(HookResult::default())
                })),
            )
            .expect("registration should succeed");

        let report = HookDispatcher::new(registry)
            .dispatch(request())
            .await
            .expect("dispatch should continue");

        assert_eq!(report.completed_hooks, vec!["after"]);
        assert_eq!(report.failures.len(), 1);
        assert_eq!(report.failures[0].hook_id, "failing-effect");
        assert!(report.diagnostics[0].message.contains("writeback failed"));
    }

    #[tokio::test]
    async fn timeout_policy_records_queueable_failure() {
        let mut hook = registration("slow", HookMode::Effect);
        hook.timeout_ms = Some(1);
        hook.failure_policy = HookFailurePolicy::Queue;

        let mut registry = HookRegistry::new();
        registry
            .register(
                hook,
                Arc::new(FnHookHandler::new(|_invocation: HookInvocation| async {
                    time::sleep(Duration::from_millis(30)).await;
                    Ok(HookResult::default())
                })),
            )
            .expect("registration should succeed");

        let report = HookDispatcher::new(registry)
            .dispatch(request())
            .await
            .expect("queue policy should continue");

        assert!(report.has_queueable_failure);
        assert_eq!(report.failures.len(), 1);
        assert!(report.failures[0].message.contains("timed out"));
    }

    #[tokio::test]
    async fn stop_policy_returns_dispatch_error() {
        let mut hook = registration("stopper", HookMode::Effect);
        hook.failure_policy = HookFailurePolicy::Stop;

        let mut registry = HookRegistry::new();
        registry
            .register(
                hook,
                Arc::new(FnHookHandler::new(|_invocation: HookInvocation| async {
                    Err(HookError::handler("fatal"))
                })),
            )
            .expect("registration should succeed");

        let error = HookDispatcher::new(registry)
            .dispatch(request())
            .await
            .expect_err("stop policy should fail dispatch");

        assert!(matches!(error, HookError::DispatchStopped(_)));
    }

    #[tokio::test]
    async fn lifecycle_listener_observes_registration_and_dispatch_events() {
        let events = Arc::new(Mutex::new(Vec::<HookEventKind>::new()));
        let mut dispatcher = HookDispatcher::default();
        dispatcher.add_listener(Arc::new(FnHookEventListener::new({
            let events = Arc::clone(&events);
            move |event: HookEvent| {
                events
                    .lock()
                    .expect("events lock should not be poisoned")
                    .push(event.kind);
            }
        })));

        dispatcher
            .register(
                registration("observe", HookMode::Observe),
                Arc::new(FnHookHandler::new(|_invocation: HookInvocation| async {
                    Ok(HookResult::default())
                })),
            )
            .expect("registration should succeed");

        dispatcher
            .dispatch(request())
            .await
            .expect("dispatch should succeed");

        assert_eq!(
            *events.lock().expect("events lock should not be poisoned"),
            vec![
                HookEventKind::Registered,
                HookEventKind::DispatchStarted,
                HookEventKind::HandlerStarted,
                HookEventKind::HandlerCompleted,
                HookEventKind::DispatchCompleted,
            ]
        );
    }

    #[tokio::test]
    async fn lifecycle_listener_observes_handler_failure_events() {
        let events = Arc::new(Mutex::new(Vec::<HookEvent>::new()));
        let mut dispatcher = HookDispatcher::default();
        dispatcher.add_listener(Arc::new(FnHookEventListener::new({
            let events = Arc::clone(&events);
            move |event: HookEvent| {
                events
                    .lock()
                    .expect("events lock should not be poisoned")
                    .push(event);
            }
        })));

        dispatcher
            .register(
                registration("failing", HookMode::Effect),
                Arc::new(FnHookHandler::new(|_invocation: HookInvocation| async {
                    Err(HookError::handler("listener failure sample"))
                })),
            )
            .expect("registration should succeed");

        dispatcher
            .dispatch(request())
            .await
            .expect("continue policy should keep dispatch successful");

        let events = events.lock().expect("events lock should not be poisoned");
        let failure = events
            .iter()
            .find(|event| event.kind == HookEventKind::HandlerFailed)
            .expect("failure event should be emitted");
        assert_eq!(failure.hook_id.as_deref(), Some("failing"));
        assert!(
            failure
                .message
                .as_deref()
                .unwrap_or_default()
                .contains("listener failure sample")
        );
    }

    #[test]
    fn point_descriptors_validate_allowed_hook_modes() {
        let mut registry = HookRegistry::new();
        registry
            .register_point(
                HookPointDescriptor::new(
                    "conversation.turn.completed",
                    [HookMode::Observe, HookMode::Effect],
                    "completed turn",
                )
                .with_default_scope(HookScope::Session),
            )
            .expect("point descriptor should register");

        assert_eq!(registry.point_descriptors().len(), 1);
        assert!(
            registry
                .register(
                    HookRegistration::new(
                        "bad-mode",
                        "conversation.turn.completed",
                        HookMode::Transform,
                    ),
                    Arc::new(FnHookHandler::new(|_invocation: HookInvocation| async {
                        Ok(HookResult::default())
                    })),
                )
                .is_err()
        );
    }

    #[test]
    fn registry_rejects_duplicate_hook_ids() {
        let mut registry = HookRegistry::new();
        registry
            .register(
                registration("same-id", HookMode::Observe),
                Arc::new(FnHookHandler::new(|_invocation: HookInvocation| async {
                    Ok(HookResult::default())
                })),
            )
            .expect("first registration should succeed");

        assert!(
            registry
                .register(
                    HookRegistration::new(
                        "same-id",
                        "conversation.turn.completed",
                        HookMode::Observe,
                    ),
                    Arc::new(FnHookHandler::new(|_invocation: HookInvocation| async {
                        Ok(HookResult::default())
                    })),
                )
                .is_err()
        );
    }

    #[test]
    fn listener_registration_rejects_empty_ids() {
        let mut dispatcher = HookDispatcher::default();
        assert!(
            dispatcher
                .add_listener_with_id(
                    " ",
                    Arc::new(FnHookEventListener::new(|_event: HookEvent| {})),
                )
                .is_err()
        );
    }

    #[tokio::test]
    async fn unregister_removes_handler_and_emits_lifecycle_event() {
        let events = Arc::new(Mutex::new(Vec::<HookEventKind>::new()));
        let mut dispatcher = HookDispatcher::default();
        dispatcher
            .add_listener_with_id(
                "audit",
                Arc::new(FnHookEventListener::new({
                    let events = Arc::clone(&events);
                    move |event: HookEvent| {
                        events
                            .lock()
                            .expect("events lock should not be poisoned")
                            .push(event.kind);
                    }
                })),
            )
            .expect("listener id should be valid");
        dispatcher
            .register(
                registration("observe", HookMode::Observe),
                Arc::new(FnHookHandler::new(|_invocation: HookInvocation| async {
                    Ok(HookResult::default())
                })),
            )
            .expect("registration should succeed");

        assert_eq!(dispatcher.unregister("observe", None), 1);
        let report = dispatcher
            .dispatch(request())
            .await
            .expect("dispatch should succeed without handlers");

        assert!(report.completed_hooks.is_empty());
        assert_eq!(
            dispatcher
                .registry()
                .registration_snapshots(Some("conversation.turn.precheck")),
            Vec::<HookRegistrationSnapshot>::new()
        );
        assert!(
            events
                .lock()
                .expect("events lock should not be poisoned")
                .contains(&HookEventKind::Unregistered)
        );
    }

    #[tokio::test]
    async fn listener_can_be_removed_by_id() {
        let events = Arc::new(Mutex::new(Vec::<HookEventKind>::new()));
        let mut dispatcher = HookDispatcher::default();
        dispatcher
            .add_listener_with_id(
                "audit",
                Arc::new(FnHookEventListener::new({
                    let events = Arc::clone(&events);
                    move |event: HookEvent| {
                        events
                            .lock()
                            .expect("events lock should not be poisoned")
                            .push(event.kind);
                    }
                })),
            )
            .expect("listener id should be valid");

        assert!(dispatcher.remove_listener("audit"));
        dispatcher
            .register(
                registration("observe", HookMode::Observe),
                Arc::new(FnHookHandler::new(|_invocation: HookInvocation| async {
                    Ok(HookResult::default())
                })),
            )
            .expect("registration should succeed");

        assert!(
            events
                .lock()
                .expect("events lock should not be poisoned")
                .is_empty()
        );
    }
}
