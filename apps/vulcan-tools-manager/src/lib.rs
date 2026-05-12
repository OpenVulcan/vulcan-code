//! Host-owned unified tool manager that resolves the final AI-visible tool surface and dispatches tool calls.
//! 负责解析最终 AI 可见工具面并调度工具调用的宿主自有统一工具管理器。

use serde_json::Value;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

/// Stable execution mode used when resolving the final AI-visible tool surface.
/// 在解析最终 AI 可见工具面时使用的稳定执行模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VulcanToolMode {
    /// Normal implementation mode with the full coding tool surface.
    /// 拥有完整编码工具面的常规实现模式。
    Build,
    /// Planning mode with a conservative read-only and analysis-biased tool surface.
    /// 采用保守只读与分析偏向工具面的规划模式。
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
    /// LuaSkills runtime entry.
    /// LuaSkills 运行时入口。
    LuaSkills,
    /// MCP runtime tool discovered from one live server.
    /// 从某个在线服务器发现的 MCP 运行时工具。
    Mcp,
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
    /// Stable tool-source kind.
    /// 稳定工具来源类型。
    pub source: VulcanToolSourceKind,
}

/// Stable Plan-mode visibility policy attached to one candidate tool.
/// 附着在单个候选工具上的稳定 Plan 模式可见性策略。
#[derive(Debug, Clone)]
pub enum VulcanPlanToolPolicy {
    /// The tool is never exposed in Plan mode.
    /// 当前工具在 Plan 模式中永不暴露。
    Hidden,
    /// The tool remains directly visible in Plan mode.
    /// 当前工具在 Plan 模式中继续直接可见。
    Visible,
    /// The tool is visible in Plan mode only when the provided text matches one read-only heuristic keyword.
    /// 当前工具仅在给定文本命中只读启发式关键字时才在 Plan 模式中可见。
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

/// One candidate tool surface item provided by the host before mode-based filtering.
/// 宿主在模式过滤前提供的单个候选工具面条目。
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
    /// Plan-mode visibility policy used during mode-based filtering.
    /// 模式过滤过程中使用的 Plan 模式可见性策略。
    pub plan_policy: VulcanPlanToolPolicy,
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
            .finish()
    }
}

/// Final resolved tool set for one execution turn after mode-based filtering.
/// 单轮执行在模式过滤之后得到的最终解析工具集合。
#[derive(Debug, Clone, Default)]
pub struct ResolvedToolSet {
    /// Unified AI-visible tool descriptors after source aggregation and mode filtering.
    /// 聚合来源并进行模式过滤后得到的统一 AI 可见工具描述集合。
    pub descriptors: Vec<VulcanToolDescriptor>,
    /// Fully resolved tool definitions ready for rig registration and host-side dispatch.
    /// 已为 rig 注册与宿主侧调度准备好的完整工具定义集合。
    pub definitions: Vec<ResolvedToolDefinition>,
}

/// Host-owned unified tool manager that normalises descriptors, filters the AI tool surface by mode, and dispatches resolved tool calls.
/// 负责标准化工具描述、按模式过滤 AI 工具面并调度已解析工具调用的宿主自有统一工具管理器。
#[derive(Clone, Default)]
pub struct VulcanToolsManager {
    /// Candidate tools aggregated from host-owned native, LuaSkills, MCP, or future expert sources.
    /// 从宿主自有原生、LuaSkills、MCP 或未来专家来源聚合得到的候选工具集合。
    candidates: Vec<VulcanToolCandidate>,
}

impl VulcanToolsManager {
    /// Builds one unified tool manager from the current host-owned candidate surfaces.
    /// 根据当前宿主自有候选工具面构建统一工具管理器。
    pub fn new(candidates: Vec<VulcanToolCandidate>) -> Self {
        Self { candidates }
    }

    /// Resolves the final AI-visible tool set for one execution-mode label.
    /// 为单条执行模式标签解析最终 AI 可见工具集合。
    pub fn resolve_for_execution_mode(&self, execution_mode_label: &str) -> ResolvedToolSet {
        let mode = VulcanToolMode::from_execution_mode_label(execution_mode_label);
        let mut descriptors = Vec::new();
        let mut definitions = Vec::new();

        for candidate in self
            .candidates
            .iter()
            .filter(|candidate| tool_is_visible(candidate, mode))
        {
            descriptors.push(candidate.descriptor.clone());
            definitions.push(ResolvedToolDefinition {
                descriptor: candidate.descriptor.clone(),
                parameters_schema: candidate.parameters_schema.clone(),
                invoke: candidate.invoke.clone(),
            });
        }

        ResolvedToolSet {
            descriptors,
            definitions,
        }
    }

    /// Dispatches one resolved tool call through its host-owned invocation closure.
    /// 通过宿主自有调用闭包调度单个已解析工具调用。
    pub async fn invoke_tool(
        &self,
        tool: &ResolvedToolDefinition,
        args: Value,
        session_id: Option<String>,
    ) -> Result<VulcanToolInvocationResult, String> {
        (tool.invoke)(args, session_id).await
    }
}

/// Returns whether one candidate tool should remain visible to AI in the current execution mode.
/// 返回单个候选工具在当前执行模式下是否应继续对 AI 可见。
fn tool_is_visible(candidate: &VulcanToolCandidate, mode: VulcanToolMode) -> bool {
    if !candidate.ai_visible {
        return false;
    }

    match mode {
        VulcanToolMode::Build => true,
        VulcanToolMode::Plan => match &candidate.plan_policy {
            VulcanPlanToolPolicy::Hidden => false,
            VulcanPlanToolPolicy::Visible => true,
            VulcanPlanToolPolicy::HeuristicText(text) => DEFAULT_PLAN_READ_ONLY_KEYWORDS
                .iter()
                .any(|keyword| text.to_ascii_lowercase().contains(keyword)),
        },
    }
}

/// Shared read-only heuristic keywords used during the first-stage Plan-mode filter.
/// 第一阶段 Plan 模式过滤使用的共享只读启发式关键字集合。
const DEFAULT_PLAN_READ_ONLY_KEYWORDS: &[&str] = &[
    "read",
    "list",
    "grep",
    "search",
    "find",
    "tree",
    "ast",
    "detail",
    "menu",
    "symbol",
    "hover",
    "reference",
    "fetch",
    "get",
    "query",
    "check",
    "lint",
    "typecheck",
    "test",
    "describe",
];
