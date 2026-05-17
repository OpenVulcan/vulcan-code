//! Host-owned unified tool manager that resolves the final AI-visible tool surface and dispatches tool calls.
//! 负责解析最终 AI 可见工具面并调度工具调用的宿主自有统一工具管理器。

use serde_json::Value;
use std::collections::BTreeSet;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

/// Maximum public tool-name length kept compatible with strict provider validators.
/// 与严格 Provider 校验器兼容的公开工具名最大长度。
const MAX_PUBLIC_TOOL_NAME_LEN: usize = 64;

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
        Self {
            candidates: normalize_candidate_public_tool_names(candidates),
        }
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

/// Normalizes all candidate descriptors into one globally unique provider-safe public tool namespace.
/// 将所有候选工具描述归一化到全局唯一且 Provider 安全的公开工具命名空间。
fn normalize_candidate_public_tool_names(
    candidates: Vec<VulcanToolCandidate>,
) -> Vec<VulcanToolCandidate> {
    let mut occupied_names = BTreeSet::new();
    candidates
        .into_iter()
        .map(|candidate| normalize_candidate_public_tool_name(candidate, &mut occupied_names))
        .collect()
}

/// Normalizes one candidate descriptor and preserves its invocation closure as the origin mapping.
/// 归一化单个候选工具描述，并保留其调用闭包作为真实来源映射。
fn normalize_candidate_public_tool_name(
    mut candidate: VulcanToolCandidate,
    occupied_names: &mut BTreeSet<String>,
) -> VulcanToolCandidate {
    let preferred_name = preferred_public_tool_name_for_descriptor(&candidate.descriptor);
    let public_name = if candidate.descriptor.source == VulcanToolSourceKind::LuaSkills
        && occupied_names.contains(&preferred_name)
    {
        allocate_unique_public_tool_name(
            &build_prefixed_public_tool_name("ls", &[&candidate.descriptor.id]),
            occupied_names,
        )
    } else {
        allocate_unique_public_tool_name(&preferred_name, occupied_names)
    };

    candidate.descriptor.id = public_name.clone();
    candidate.descriptor.name = public_name;
    candidate
}

/// Builds the preferred public tool name for one descriptor before collision suffixes are applied.
/// 在应用冲突后缀前为单条描述构建首选公开工具名。
fn preferred_public_tool_name_for_descriptor(descriptor: &VulcanToolDescriptor) -> String {
    match descriptor.source {
        VulcanToolSourceKind::Native
        | VulcanToolSourceKind::LuaSkills
        | VulcanToolSourceKind::AiFallback => build_direct_public_tool_name(&descriptor.id),
        VulcanToolSourceKind::Mcp => {
            let direct_name = build_direct_public_tool_name(&descriptor.id);
            if direct_name.starts_with("mcp_") {
                direct_name
            } else {
                build_prefixed_public_tool_name("mcp", &[&descriptor.id])
            }
        }
    }
}

/// Returns whether one public tool name satisfies the strict host-wide compatibility rule.
/// 返回单个公开工具名是否满足宿主统一采用的严格兼容规则。
pub fn public_tool_name_is_valid(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };

    if !first.is_ascii_lowercase() || name.len() > MAX_PUBLIC_TOOL_NAME_LEN {
        return false;
    }

    chars.all(|character| {
        character.is_ascii_lowercase() || character.is_ascii_digit() || character == '_'
    })
}

/// Sanitizes one raw name fragment into a lowercase ASCII token made of letters, digits, and underscores.
/// 将单个原始名称片段规整为仅包含小写字母、数字与下划线的 ASCII token。
fn sanitize_public_tool_name_fragment(raw: &str) -> String {
    let mut sanitized = String::new();
    let mut previous_was_underscore = false;

    for character in raw.chars() {
        let normalized = if character.is_ascii_alphanumeric() {
            character.to_ascii_lowercase()
        } else {
            '_'
        };

        if normalized == '_' {
            if sanitized.is_empty() || previous_was_underscore {
                continue;
            }
            previous_was_underscore = true;
            sanitized.push(normalized);
            continue;
        }

        previous_was_underscore = false;
        sanitized.push(normalized);
    }

    sanitized.trim_matches('_').to_string()
}

/// Builds one direct public tool name from a raw source name while ensuring a safe leading letter.
/// 根据原始来源名称构建直接公开工具名，并确保具备安全的前导字母。
fn build_direct_public_tool_name(raw: &str) -> String {
    finalize_public_tool_name(sanitize_public_tool_name_fragment(raw))
}

/// Builds one prefixed public tool name from a mandatory prefix and ordered raw fragments.
/// 根据必选前缀与有序原始片段构建带前缀的公开工具名。
fn build_prefixed_public_tool_name(prefix: &str, fragments: &[&str]) -> String {
    let mut parts = Vec::new();
    let prefix_fragment = sanitize_public_tool_name_fragment(prefix);
    if !prefix_fragment.is_empty() {
        parts.push(prefix_fragment);
    }

    for fragment in fragments {
        let sanitized = sanitize_public_tool_name_fragment(fragment);
        if !sanitized.is_empty() {
            parts.push(sanitized);
        }
    }

    finalize_public_tool_name(parts.join("_"))
}

/// Allocates one unique public tool name by appending stable numeric suffixes when needed.
/// 在需要时通过追加稳定数字后缀分配唯一公开工具名。
fn allocate_unique_public_tool_name(
    preferred_base_name: &str,
    occupied_names: &mut BTreeSet<String>,
) -> String {
    let base_name = finalize_public_tool_name(preferred_base_name.to_string());
    if occupied_names.insert(base_name.clone()) {
        return base_name;
    }

    for index in 1usize.. {
        let suffix = format!("_{index}");
        let truncated_base = truncate_public_tool_name_base(&base_name, suffix.len());
        let candidate = format!("{truncated_base}{suffix}");
        if occupied_names.insert(candidate.clone()) {
            return candidate;
        }
    }

    unreachable!("公开工具名分配器应始终能找到可用编号后缀。");
}

/// Finalizes one partially sanitized name into the canonical host-wide public format.
/// 将一段部分规整的名称最终收敛为宿主统一使用的规范公开格式。
fn finalize_public_tool_name(candidate: String) -> String {
    if public_tool_name_is_valid(&candidate) {
        return candidate;
    }

    let mut finalized = candidate;
    if finalized.is_empty() {
        finalized = String::from("tool");
    }

    if !finalized
        .chars()
        .next()
        .is_some_and(|character| character.is_ascii_lowercase())
    {
        finalized = format!("tool_{finalized}");
    }

    if finalized.len() > MAX_PUBLIC_TOOL_NAME_LEN {
        finalized = truncate_public_tool_name_base(&finalized, 0);
    }

    if finalized.is_empty() {
        String::from("tool")
    } else {
        finalized
    }
}

/// Truncates one public-tool-name base while preserving suffix budget and trimming trailing underscores.
/// 在保留后缀预算并去除尾部下划线的同时截断公开工具名基础部分。
fn truncate_public_tool_name_base(base_name: &str, reserved_suffix_len: usize) -> String {
    let maximum_base_len = MAX_PUBLIC_TOOL_NAME_LEN.saturating_sub(reserved_suffix_len);
    let mut truncated = base_name
        .chars()
        .take(maximum_base_len.max(1))
        .collect::<String>();

    while truncated.ends_with('_') {
        truncated.pop();
    }

    if truncated.is_empty() {
        String::from("tool")
    } else {
        truncated
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

#[cfg(test)]
mod tests {
    use super::{
        ResolvedToolSet, VulcanPlanToolPolicy, VulcanToolCandidate, VulcanToolDescriptor,
        VulcanToolInvocationResult, VulcanToolMode, VulcanToolSourceKind, VulcanToolsManager,
        public_tool_name_is_valid,
    };
    use serde_json::{Value, json};
    use std::sync::Arc;

    /// Builds one inert candidate descriptor for public-name allocation tests.
    /// 为公开名称分配测试构建单个惰性候选工具描述。
    fn test_candidate(
        id: &str,
        source: VulcanToolSourceKind,
        plan_policy: VulcanPlanToolPolicy,
    ) -> VulcanToolCandidate {
        VulcanToolCandidate {
            descriptor: VulcanToolDescriptor {
                id: id.to_string(),
                name: id.to_string(),
                description: format!("测试工具 {id}"),
                source,
            },
            parameters_schema: json!({"type": "object"}),
            ai_visible: true,
            plan_policy,
            invoke: Arc::new(|_args: Value, _session_id| {
                Box::pin(async { Ok(VulcanToolInvocationResult::from_model_output(json!("ok"))) })
            }),
        }
    }

    /// Extracts resolved tool names from one resolved tool set.
    /// 从一份已解析工具集合中提取公开工具名。
    fn resolved_names(tool_set: ResolvedToolSet) -> Vec<String> {
        tool_set
            .definitions
            .into_iter()
            .map(|definition| definition.descriptor.name)
            .collect()
    }

    /// Verifies strict provider-safe public tool-name validation.
    /// 验证严格 Provider 安全公开工具名校验。
    #[test]
    fn public_tool_name_validation_rejects_common_provider_breakers() {
        assert!(public_tool_name_is_valid("mcp_server_read_file"));
        assert!(!public_tool_name_is_valid("workspace.describe"));
        assert!(!public_tool_name_is_valid("registry/list-skills"));
        assert!(!public_tool_name_is_valid("1tool"));
    }

    /// Verifies native names keep highest priority while LuaSkills collisions gain the readable `ls_` prefix.
    /// 验证 native 名称保持最高优先级，而 LuaSkills 冲突会获得可读的 `ls_` 前缀。
    #[test]
    fn manager_allocates_luaskills_collision_with_ls_prefix() {
        let manager = VulcanToolsManager::new(vec![
            test_candidate(
                "abc_abc",
                VulcanToolSourceKind::Native,
                VulcanPlanToolPolicy::Visible,
            ),
            test_candidate(
                "abc-abc",
                VulcanToolSourceKind::LuaSkills,
                VulcanPlanToolPolicy::Visible,
            ),
        ]);

        assert_eq!(
            resolved_names(manager.resolve_for_execution_mode("Build")),
            vec!["abc_abc", "ls_abc_abc"]
        );
    }

    /// Verifies MCP tools always retain an `mcp_` public prefix and collide through numeric suffixes.
    /// 验证 MCP 工具始终保留 `mcp_` 公开前缀，并通过数字后缀处理冲突。
    #[test]
    fn manager_allocates_mcp_names_with_prefix_and_suffixes() {
        let manager = VulcanToolsManager::new(vec![
            test_candidate(
                "server/read-file",
                VulcanToolSourceKind::Mcp,
                VulcanPlanToolPolicy::Visible,
            ),
            test_candidate(
                "server.read_file",
                VulcanToolSourceKind::Mcp,
                VulcanPlanToolPolicy::Visible,
            ),
        ]);

        assert_eq!(
            resolved_names(manager.resolve_for_execution_mode("Build")),
            vec!["mcp_server_read_file", "mcp_server_read_file_1"]
        );
    }

    /// Verifies mode filtering runs after public-name normalization without losing the normalized descriptor.
    /// 验证模式过滤会在公开名称归一化之后运行，并且不会丢失归一化后的描述。
    #[test]
    fn manager_filters_plan_mode_after_public_name_normalization() {
        let manager = VulcanToolsManager::new(vec![
            test_candidate(
                "read-file",
                VulcanToolSourceKind::Native,
                VulcanPlanToolPolicy::HeuristicText(String::from("read file")),
            ),
            test_candidate(
                "write-file",
                VulcanToolSourceKind::Native,
                VulcanPlanToolPolicy::Hidden,
            ),
        ]);

        assert_eq!(
            VulcanToolMode::from_execution_mode_label("Plan"),
            VulcanToolMode::Plan
        );
        assert_eq!(
            resolved_names(manager.resolve_for_execution_mode("Plan")),
            vec!["read_file"]
        );
    }
}
