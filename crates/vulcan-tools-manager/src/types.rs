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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VulcanToolSourceKind {
    /// Host-owned native Rust tool.
    /// 宿主自有的 Rust 原生工具。
    Native,
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
    AiFallback,
}

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
}

impl VulcanToolInvocationResult {
    /// Builds one structured tool result from the model-visible payload only.
    /// 仅根据模型可见载荷构建一条结构化工具结果。
    pub fn from_model_output(model_output: Value) -> Self {
        Self {
            model_output,
            host_result: None,
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
        }
    }

    /// Attaches one host-only structured result while preserving the model-visible payload.
    /// 在保留模型可见载荷的同时附着一条宿主专用结构化结果。
    pub fn with_host_result(mut self, host_result: VulcanToolHostResult) -> Self {
        self.host_result = Some(host_result);
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
    Arc<dyn Fn(Value, Option<String>) -> VulcanToolInvokeFuture + Send + Sync>;

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
    /// Preserve the host schema exactly as it was resolved by the tool manager.
    /// 原样保留工具管理器解析出的宿主 Schema。
    PassThrough,
    /// Add explicit container types and compatibility details for stricter JSON Schema validators.
    /// 为更严格的 JSON Schema 校验器补齐显式容器类型与兼容细节。
    StrictContainer,
    /// Project JSON Schema into the smaller Gemini function-declaration dialect.
    /// 将 JSON Schema 投影为更精简的 Gemini 函数声明方言。
    GeminiSlim,
    /// Apply OpenAI Responses strict-tool requirements after container normalization.
    /// 在容器规范化之后应用 OpenAI Responses 严格工具要求。
    OpenAiResponsesStrict,
}

/// Request-scoped context used to choose the provider-facing tool-schema projection profile.
/// 用于选择 Provider 可见工具 Schema 投影策略的请求级上下文。
pub struct VulcanToolSchemaProjectionContext<'a> {
    /// Stable provider identifier such as `openrouter` or `google`.
    /// 稳定供应商标识，例如 `openrouter` 或 `google`。
    pub provider_id: &'a str,
    /// Stable API-family key such as `openai-chat-compatible`.
    /// 稳定接口族键，例如 `openai-chat-compatible`。
    pub api_family_key: &'a str,
    /// Stable model identifier used by the concrete provider request.
    /// 具体 Provider 请求使用的稳定模型标识。
    pub model_id: &'a str,
    /// Optional additional request parameters that may carry OpenRouter routing hints.
    /// 可能携带 OpenRouter 路由提示的可选附加请求参数。
    pub additional_params: Option<&'a Value>,
}
