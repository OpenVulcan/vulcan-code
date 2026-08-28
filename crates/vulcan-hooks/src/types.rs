use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;
use vulcan_context_injection::InjectionInsert;

use crate::HookError;

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

/// Request phase supplied to system-prompt hook contributors.
/// 提供给系统提示词 hook 贡献方的请求阶段。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SystemPromptHookPhase {
    /// First provider request of a session or an uninitialized hook state.
    /// 会话的首次 Provider 请求或未初始化的 hook 状态。
    #[default]
    FirstSession,
    /// First provider request after a context compaction boundary.
    /// 上下文压缩边界后的首次 Provider 请求。
    AfterContextCompaction,
    /// Ordinary provider request after the hook has already initialized.
    /// hook 已初始化后的普通 Provider 请求。
    SubsequentSession,
}

impl SystemPromptHookPhase {
    /// Returns the stable payload label for this phase.
    /// 返回该阶段的稳定载荷标签。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::FirstSession => "first_session",
            Self::AfterContextCompaction => "after_context_compaction",
            Self::SubsequentSession => "subsequent_session",
        }
    }
}

/// Stable runtime-baseline class declared by a system-prompt hook producer.
/// 由系统提示词 Hook 生产者声明的稳定运行时基线类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SystemPromptHookBaselineKind {
    /// Captured memory-profile baseline.
    /// 已捕获的记忆画像基线。
    MemoryProfile,
    /// Long-lived plugin instruction block.
    /// 长期插件指令块。
    PluginInstructions,
    /// Long-lived baseline hook body.
    /// 长期基线 Hook 正文。
    BaselineHooks,
}

/// Typed baseline provenance supplied by the authoritative hook producer.
/// 由权威 Hook 生产者提供的强类型基线来源。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SystemPromptHookBaselineProvenance {
    /// Stable baseline class declared without string inference.
    /// 无需字符串推断即可声明的稳定基线类型。
    pub kind: SystemPromptHookBaselineKind,
    /// Stable final prompt-node identity declared by the producer.
    /// 由生产者声明的稳定最终提示词节点身份。
    pub node_id: String,
    /// Stable producer-owned source identity.
    /// 由生产者持有的稳定来源身份。
    pub source_id: String,
}

impl SystemPromptHookBaselineProvenance {
    /// Builds one explicit hook-baseline provenance value.
    /// 构建一条显式 Hook 基线来源信息。
    pub fn new(
        kind: SystemPromptHookBaselineKind,
        node_id: impl Into<String>,
        source_id: impl Into<String>,
    ) -> Self {
        Self {
            kind,
            node_id: node_id.into(),
            source_id: source_id.into(),
        }
    }
}

/// System-prompt contribution returned by a hook for the current provider request.
/// hook 为当前 Provider 请求返回的系统提示词贡献块。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SystemPromptHookBlock {
    /// Stable XML-like tag used to group this hook block.
    /// 用于分组该 hook 块的稳定类 XML 标签。
    pub tag: String,
    /// Short explanation rendered before the block content.
    /// 渲染在块内容前的简短说明。
    #[serde(default)]
    pub description: String,
    /// Provider-visible system prompt content.
    /// Provider 可见的系统提示词内容。
    pub content: String,
    /// Optional source label used for diagnostics.
    /// 用于诊断的可选来源标签。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// Phase observed when this block was generated.
    /// 生成该块时观察到的阶段。
    #[serde(default)]
    pub phase: SystemPromptHookPhase,
    /// Optional typed provenance; `None` means request-dynamic hook content.
    /// 可选强类型来源；`None` 表示请求动态 Hook 内容。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub baseline_provenance: Option<SystemPromptHookBaselineProvenance>,
}

impl SystemPromptHookBlock {
    /// Builds one hook block with required tag and content.
    /// 使用必需的标签和内容构建一个 hook 块。
    pub fn new(tag: impl Into<String>, content: impl Into<String>) -> Self {
        Self {
            tag: tag.into(),
            description: String::new(),
            content: content.into(),
            source: None,
            phase: SystemPromptHookPhase::default(),
            baseline_provenance: None,
        }
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
    /// System-prompt blocks submitted by the hook for the current request.
    /// hook 为当前请求提交的系统提示词块。
    #[serde(default)]
    pub system_prompt_blocks: Vec<SystemPromptHookBlock>,
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
    pub(super) fn timeout_duration(&self) -> Option<Duration> {
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
