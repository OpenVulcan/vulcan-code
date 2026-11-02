use super::*;

/// Host-result kind used by tools to return structured failure details before invocation identity is attached.
/// 工具在附加调用身份前返回结构化失败详情时使用的宿主结果类型。
pub const VULCAN_TOOL_ERROR_DETAIL_KIND: &str = "tool_error_detail";

/// Stable execution mode used when resolving the final AI-visible tool surface.
/// 在解析最终 AI 可见工具面时使用的稳定执行模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VulcanToolMode {
    /// Normal implementation mode with the full coding tool surface.
    /// 拥有完整编码工具面的常规实现模式。
    Build,
    /// Planning mode with a stable tool surface and invocation-time mutation guards.
    /// 使用稳定工具面并在调用期执行变更守卫的规划模式。
    Plan,
}

impl VulcanToolMode {
    /// Parses one UI-facing execution-mode label into the internal stable mode enum.
    /// 将单条面向界面的执行模式标签解析为内部稳定模式枚举。
    pub fn from_execution_mode_label(label: &str) -> Self {
        if label.to_ascii_lowercase().contains("plan") {
            Self::Plan
        } else {
            Self::Build
        }
    }
}

/// Stable tool-source kind surfaced by the unified host tool manager.
/// 由统一宿主工具管理器对外暴露的稳定工具来源类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VulcanToolSourceKind {
    /// Host-owned native Rust tool.
    /// 宿主自有的 Rust 原生工具。
    Native,
    /// Host-managed Vulcan tool backed by an internal runtime or protocol.
    /// 由内部运行时或协议承载的 Vulcan 宿主托管工具。
    VulcanManaged,
    /// Manifest-driven bundled Vulcan Plugin tool.
    /// 清单驱动的 Vulcan 插件工具。
    VulcanPlugin,
    /// LuaSkills runtime entry.
    /// LuaSkills 运行时入口。
    LuaSkills,
    /// MCP runtime tool discovered from one live server.
    /// 从某个在线服务器发现的 MCP 运行时工具。
    Mcp,
    /// AI-specialized semantic fallback alias backed by another host tool.
    /// 由另一个宿主工具承载的 AI 专用语义回退别名。
    AiHostBinding,
}

/// Immutable identity of the source tool before host-facing name normalization.
/// 宿主侧名称归一化之前的来源工具不可变身份。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct VulcanToolOrigin {
    /// Exact source kind that owns the tool.
    /// 拥有该工具的精确来源种类。
    source_kind: VulcanToolSourceKind,
    /// Stable namespace assigned by the source registry.
    /// 由来源注册表分配的稳定命名空间。
    namespace: String,
    /// Exact source-owned tool name used before public-name normalization.
    /// 名称归一化前由来源拥有的精确工具名。
    original_name: String,
}

impl VulcanToolOrigin {
    /// Builds one immutable source identity from authoritative source fields.
    /// 根据来源的权威字段构建一份不可变来源身份。
    pub fn new(
        source_kind: VulcanToolSourceKind,
        namespace: impl Into<String>,
        original_name: impl Into<String>,
    ) -> Self {
        Self {
            source_kind,
            namespace: namespace.into(),
            original_name: original_name.into(),
        }
    }

    /// Returns the exact source kind captured before public-name normalization.
    /// 返回公开名称归一化前捕获的精确来源种类。
    pub fn source_kind(&self) -> VulcanToolSourceKind {
        self.source_kind
    }

    /// Returns the stable source namespace captured before public-name normalization.
    /// 返回公开名称归一化前捕获的稳定来源命名空间。
    pub fn namespace(&self) -> &str {
        &self.namespace
    }

    /// Returns the exact source-owned tool name captured before public-name normalization.
    /// 返回公开名称归一化前捕获的来源自有精确工具名。
    pub fn original_name(&self) -> &str {
        &self.original_name
    }
}

/// Compatibility alias for callers that name the immutable source identity explicitly.
/// 为明确称呼不可变来源身份的调用方提供兼容别名。
pub type VulcanToolSourceIdentity = VulcanToolOrigin;

/// One unified tool descriptor emitted by the host tool manager.
/// 由宿主工具管理器发出的统一工具描述。
#[derive(Debug, Clone)]
pub struct VulcanToolDescriptor {
    /// Stable tool identifier.
    /// 稳定工具标识。
    pub id: String,
    /// Human-readable tool name.
    /// 人类可读工具名称。
    pub name: String,
    /// Human-readable tool description.
    /// 人类可读工具描述。
    pub description: String,
    /// Optional Markdown guidance injected into the prompt when this tool is exposed.
    /// 当前工具暴露时注入提示词的可选 Markdown 使用说明。
    pub help_text: Option<String>,
    /// Stable tool-source kind.
    /// 稳定工具来源类型。
    pub source: VulcanToolSourceKind,
    /// Immutable source identity captured before public-name normalization.
    /// 在公开名称归一化前捕获的不可变来源身份。
    origin: VulcanToolOrigin,
}

impl VulcanToolDescriptor {
    /// Builds one descriptor whose source kind is derived from its immutable origin.
    /// 构建一份来源种类由不可变来源身份派生的工具描述。
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        description: impl Into<String>,
        help_text: Option<String>,
        origin: VulcanToolOrigin,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            description: description.into(),
            help_text,
            source: origin.source_kind(),
            origin,
        }
    }

    /// Returns the immutable source identity captured before public-name normalization.
    /// 返回公开名称归一化前捕获的不可变来源身份。
    pub fn origin(&self) -> &VulcanToolOrigin {
        &self.origin
    }

    /// Returns the immutable source identity under an explicit identity-oriented name.
    /// 以明确身份语义的名称返回不可变来源身份。
    pub fn source_identity(&self) -> &VulcanToolSourceIdentity {
        &self.origin
    }
}

/// Stable Plan-mode execution policy attached to one candidate tool.
/// 附着在单个候选工具上的稳定 Plan 模式执行策略。
#[derive(Debug, Clone)]
pub enum VulcanPlanToolPolicy {
    /// The tool is blocked when invoked in Plan mode.
    /// 当前工具在 Plan 模式中调用时会被拦截。
    Hidden,
    /// The tool may execute in Plan mode.
    /// 当前工具允许在 Plan 模式中执行。
    Visible,
    /// The tool may execute in Plan mode only when the provided text matches one read-only heuristic keyword.
    /// 当前工具仅在给定文本命中只读启发式关键字时才允许在 Plan 模式中执行。
    HeuristicText(String),
    /// The tool may execute in Plan mode only when one string argument matches the read-only
    /// heuristic, as for a dispatcher that names the tool it runs.
    /// 当前工具仅在一个字符串参数命中只读启发式时才允许在 Plan 模式中执行，适用于以参数指定所运行工具的调度器。
    HeuristicStringArgument {
        /// Exact top-level argument field inspected by the execution guard.
        /// 执行守卫检查的精确顶层参数字段。
        field: String,
    },
    /// The tool may execute in Plan mode only when one string argument belongs to a closed allowlist.
    /// 当前工具仅在一个字符串参数属于封闭白名单时才允许在 Plan 模式中执行。
    StringArgumentAllowlist {
        /// Exact top-level argument field inspected by the execution guard.
        /// 执行守卫检查的精确顶层参数字段。
        field: String,
        /// Exact string values that represent read-only operations.
        /// 表示只读操作的精确字符串值。
        allowed_values: Vec<String>,
    },
}

/// Structured host-only tool result preserved outside the model-visible payload.
/// 保留在模型可见载荷之外的宿主专用结构化工具结果。
#[derive(Debug, Clone)]
pub struct VulcanToolHostResult {
    /// Stable host-consumed result kind such as `change_set`.
    /// 供宿主消费的稳定结果类型，例如 `change_set`。
    pub kind: String,
    /// JSON-compatible host payload preserved for UI and audit surfaces.
    /// 为界面与审计表面保留的 JSON 兼容宿主载荷。
    pub payload: Value,
}

/// Structured tool invocation result that keeps model-visible output and host-only metadata separate.
/// 将模型可见输出与宿主专用元数据分离保存的结构化工具调用结果。
#[derive(Debug, Clone)]
pub struct VulcanToolInvocationResult {
    /// Final model-visible output that will be serialized back into the rig tool result body.
    /// 最终将被序列化回 rig 工具返回正文的模型可见输出。
    pub model_output: Value,
    /// Optional host-only structured result reserved for transcript metadata and richer UI projections.
    /// 为转录元数据与更丰富界面投影保留的可选宿主专用结构化结果。
    pub host_result: Option<VulcanToolHostResult>,
    /// Raw output streams of a command, kept in memory for `tool_chain` references and never persisted.
    /// 命令的原始输出流，仅在内存中保留供 `tool_chain` 引用，从不持久化。
    pub raw_streams: Option<VulcanToolRawStreams>,
}

/// Raw decoded stdout and stderr of one command, independent of any inline budget or spill file.
/// 单条命令解码后的原始 stdout 与 stderr，与内联预算或溢出文件无关。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VulcanToolRawStreams {
    /// Decoded standard output.
    /// 解码后的标准输出。
    pub stdout: String,
    /// Decoded standard error.
    /// 解码后的标准错误。
    pub stderr: String,
}

impl VulcanToolInvocationResult {
    /// Builds one structured tool result from the model-visible payload only.
    /// 仅根据模型可见载荷构建一条结构化工具结果。
    pub fn from_model_output(model_output: Value) -> Self {
        Self {
            model_output,
            host_result: None,
            raw_streams: None,
        }
    }

    /// Builds one tool-owned structured failure before the runtime attaches trusted invocation identity.
    /// 在运行时附加可信调用身份前构建一条工具自有结构化失败。
    ///
    /// # Parameters
    /// 参数
    /// - `error`: Human-readable failure text required by every tool-error projection.
    /// - `error`：每个工具错误投影都必须包含的人类可读失败文本。
    /// - `details`: Tool-domain failure facts that do not contain invocation identity.
    /// - `details`：不包含调用身份的工具领域失败事实。
    ///
    /// # Returns
    /// 返回值
    /// Model-visible failure plus host-only error details for bridge canonicalization.
    /// 供模型可见的失败与供桥接规范化的宿主专用错误详情。
    pub fn from_tool_error_details(error: impl Into<String>, details: Value) -> Self {
        let mut payload = Map::new();
        payload.insert(
            String::from("status"),
            Value::String(String::from("failed")),
        );
        payload.insert(String::from("error"), Value::String(error.into()));
        payload.insert(String::from("details"), details);
        let payload = Value::Object(payload);
        Self {
            model_output: payload.clone(),
            host_result: Some(VulcanToolHostResult {
                kind: String::from(VULCAN_TOOL_ERROR_DETAIL_KIND),
                payload,
            }),
            raw_streams: None,
        }
    }

    /// Attaches one host-only structured result while preserving the model-visible payload.
    /// 在保留模型可见载荷的同时附着一条宿主专用结构化结果。
    pub fn with_host_result(mut self, host_result: VulcanToolHostResult) -> Self {
        self.host_result = Some(host_result);
        self
    }

    /// Attaches the raw command streams that `tool_chain` steps may reference.
    /// 附着 `tool_chain` 步骤可以引用的原始命令流。
    pub fn with_raw_streams(mut self, raw_streams: VulcanToolRawStreams) -> Self {
        self.raw_streams = Some(raw_streams);
        self
    }
}

/// Boxed asynchronous future used by host-owned tool invokers.
/// 宿主自有工具调用器使用的装箱异步 Future。
pub type VulcanToolInvokeFuture =
    Pin<Box<dyn Future<Output = Result<VulcanToolInvocationResult, String>> + Send>>;

/// Shared host-owned asynchronous tool invoker closure.
/// 宿主自有共享异步工具调用闭包。
pub type VulcanToolInvokeHandler =
    Arc<dyn Fn(Value, VulcanToolInvocationContext) -> VulcanToolInvokeFuture + Send + Sync>;

/// Immutable identity context of one exact Provider tool invocation.
/// 一次精确 Provider 工具调用的不可变身份上下文。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VulcanToolInvocationContext {
    /// Exact turn from the durable tool preparation receipt.
    /// 来自持久工具准备回执的精确回合。
    pub turn_id: TurnId,
    /// Stable owner session identity selected before Provider dispatch.
    /// 在 Provider 分派前选定的稳定所有者会话身份。
    pub session_id: String,
    /// Exact Provider stream-item identity bound to the durable local tool receipt.
    /// 与可靠本地工具回执绑定的精确 Provider 流项目身份。
    pub request_stream_id: RequestStreamId,
    /// Exact local physical API identity allocated by the authoritative session document.
    /// 由权威会话文档分配的精确本地物理 API 身份。
    pub api_call_id: ApiCallId,
    /// Exact local tool-call identity allocated by the authoritative session document.
    /// 由权威会话文档分配的精确本地工具调用身份。
    pub tool_call_id: ToolCallId,
    /// Exact event seq of the durable local ToolCall fact.
    /// 可靠本地 ToolCall 事实的精确事件 seq。
    pub tool_seq: Seq,
    /// Exact controller-owned tool-call identity used by runtime request and result envelopes.
    /// 运行时请求与结果信封使用的精确控制器所有工具调用身份。
    pub controller_tool_call_id: String,
    /// Optional Provider-native tool-call identity retained only for correlation.
    /// 仅用于关联而保留的可选 Provider 原生工具调用身份。
    pub provider_tool_call_id: Option<String>,
}

/// One candidate tool surface item provided by the host before public exposure projection.
/// 宿主在公开暴露投影前提供的单个候选工具面条目。
#[derive(Clone)]
pub struct VulcanToolCandidate {
    /// Unified host-facing descriptor.
    /// 统一宿主侧描述。
    pub descriptor: VulcanToolDescriptor,
    /// JSON Schema describing the tool arguments that will be passed to rig.
    /// 描述将被传给 rig 的工具参数 JSON Schema。
    pub parameters_schema: Value,
    /// Whether the current tool should be directly visible to AI in Build mode.
    /// 当前工具在 Build 模式下是否应直接对 AI 可见。
    pub ai_visible: bool,
    /// Plan-mode execution policy used during invocation-time blocking.
    /// 调用期拦截过程中使用的 Plan 模式执行策略。
    pub plan_policy: VulcanPlanToolPolicy,
    /// Optional execution-only request-streaming parser and preview capability.
    /// 可选的仅执行期请求流解析器与预览能力。
    pub request_streaming: Option<ToolRequestStreamingCapability>,
    /// Final host-owned invocation closure used for dispatch.
    /// 用于调度的最终宿主自有调用闭包。
    pub invoke: VulcanToolInvokeHandler,
}

/// One fully resolved tool definition emitted by the host tool manager.
/// 由宿主工具管理器发出的单个完整已解析工具定义。
#[derive(Clone)]
pub struct ResolvedToolDefinition {
    /// Unified host-facing descriptor.
    /// 统一宿主侧描述。
    pub descriptor: VulcanToolDescriptor,
    /// JSON Schema describing the tool arguments that will be passed to rig.
    /// 描述将被传给 rig 的工具参数 JSON Schema。
    pub parameters_schema: Value,
    /// Plan-mode execution policy preserved for invocation-time blocking.
    /// 为调用期拦截保留的 Plan 模式执行策略。
    pub plan_policy: VulcanPlanToolPolicy,
    /// Optional execution-only request-streaming parser and preview capability.
    /// 可选的仅执行期请求流解析器与预览能力。
    pub request_streaming: Option<ToolRequestStreamingCapability>,
    /// Final host-owned invocation closure used for dispatch.
    /// 用于调度的最终宿主自有调用闭包。
    pub invoke: VulcanToolInvokeHandler,
}

impl std::fmt::Debug for ResolvedToolDefinition {
    /// Render one compact debug view without printing the opaque invocation closure.
    /// 渲染单条紧凑调试视图而不打印不透明调用闭包。
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ResolvedToolDefinition")
            .field("descriptor", &self.descriptor)
            .field("parameters_schema", &self.parameters_schema)
            .field("request_streaming", &self.request_streaming)
            .finish()
    }
}

/// Final resolved tool set for one execution turn after public exposure projection.
/// 单轮执行在公开暴露投影之后得到的最终解析工具集合。
#[derive(Debug, Clone, Default)]
pub struct ResolvedToolSet {
    /// Unified AI-visible tool descriptors after source aggregation and mode filtering.
    /// 聚合来源并进行模式过滤后得到的统一 AI 可见工具描述集合。
    pub descriptors: Vec<VulcanToolDescriptor>,
    /// Fully resolved tool definitions ready for rig registration and host-side dispatch.
    /// 已为 rig 注册与宿主侧调度准备好的完整工具定义集合。
    pub definitions: Vec<ResolvedToolDefinition>,
}

/// Provider-facing tool-schema projection profile selected for one concrete request.
/// 针对单个具体请求选择的 Provider 可见工具 Schema 投影策略。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VulcanToolSchemaProjectionProfile {
    /// Preserve the host schema exactly for an explicitly verified provider contract.
    /// 为经过明确验证的 Provider 契约原样保留宿主 Schema。
    PassThrough,
    /// Project the host schema into the portable basic function-tool subset without composition keywords.
    /// 将宿主 Schema 投影到不包含组合关键字的可移植基础函数工具子集。
    StandardObjectRoot,
    /// Project JSON Schema into the smaller Gemini function-declaration dialect.
    /// 将 JSON Schema 投影为更精简的 Gemini 函数声明方言。
    GeminiSlim,
}
