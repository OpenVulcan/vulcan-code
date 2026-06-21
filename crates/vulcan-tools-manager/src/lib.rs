//! Host-owned unified tool manager that resolves the final AI-visible tool surface and dispatches tool calls.
//! 负责解析最终 AI 可见工具面并调度工具调用的宿主自有统一工具管理器。

use serde_json::{Map, Value};
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

/// Resolves the tool-schema projection profile for one concrete provider/model request.
/// 为单个具体 Provider/模型请求解析工具 Schema 投影策略。
pub fn resolve_tool_schema_projection_profile(
    context: &VulcanToolSchemaProjectionContext<'_>,
) -> VulcanToolSchemaProjectionProfile {
    if context
        .api_family_key
        .eq_ignore_ascii_case("google-gemini-generate-content")
    {
        return VulcanToolSchemaProjectionProfile::GeminiSlim;
    }

    if context
        .api_family_key
        .eq_ignore_ascii_case("openai-responses-compatible")
    {
        return VulcanToolSchemaProjectionProfile::OpenAiResponsesStrict;
    }

    if openai_compatible_request_targets_gemini(context) {
        return VulcanToolSchemaProjectionProfile::GeminiSlim;
    }

    VulcanToolSchemaProjectionProfile::PassThrough
}

/// Projects one tool-parameter schema into the selected provider-facing dialect.
/// 将单个工具参数 Schema 投影到选定的 Provider 可见方言。
pub fn project_tool_parameters_schema(
    schema: &Value,
    profile: VulcanToolSchemaProjectionProfile,
) -> Value {
    match profile {
        VulcanToolSchemaProjectionProfile::PassThrough => schema.clone(),
        VulcanToolSchemaProjectionProfile::StrictContainer => {
            let mut projected = schema.clone();
            normalize_provider_tool_parameters_schema(&mut projected);
            projected
        }
        VulcanToolSchemaProjectionProfile::GeminiSlim => project_gemini_slim_schema(schema),
        VulcanToolSchemaProjectionProfile::OpenAiResponsesStrict => {
            let mut projected = schema.clone();
            normalize_provider_tool_parameters_schema(&mut projected);
            sanitize_openai_responses_schema(&mut projected);
            projected
        }
    }
}

/// Recursively normalizes one provider tool-parameters schema by inferring explicit container types for object- and array-shaped branches.
/// 递归规范化单条 Provider 工具参数 Schema，并为对象形与数组形分支补齐显式容器类型。
pub fn normalize_provider_tool_parameters_schema(schema: &mut Value) {
    match schema {
        Value::Object(object) => {
            if object.contains_key("$ref") {
                return;
            }

            normalize_schema_multi_type_union(object);

            if !object.contains_key("type") {
                if schema_branch_looks_like_object(object) {
                    object.insert(String::from("type"), Value::String(String::from("object")));
                } else if schema_branch_looks_like_array(object) {
                    object.insert(String::from("type"), Value::String(String::from("array")));
                }
            }

            for key in [
                "$defs",
                "definitions",
                "properties",
                "patternProperties",
                "dependentSchemas",
            ] {
                if let Some(Value::Object(entries)) = object.get_mut(key) {
                    for value in entries.values_mut() {
                        normalize_provider_tool_parameters_schema(value);
                    }
                }
            }

            for key in [
                "items",
                "contains",
                "propertyNames",
                "additionalProperties",
                "unevaluatedProperties",
                "unevaluatedItems",
            ] {
                if let Some(value) = object.get_mut(key) {
                    normalize_provider_tool_parameters_schema(value);
                }
            }

            let parent_properties = object.get("properties").cloned();
            let parent_additional_properties = object.get("additionalProperties").cloned();

            for key in ["prefixItems"] {
                if let Some(Value::Array(items)) = object.get_mut(key) {
                    for item in items {
                        normalize_provider_tool_parameters_schema(item);
                    }
                }
            }

            for key in ["anyOf", "oneOf", "allOf"] {
                if let Some(Value::Array(items)) = object.get_mut(key) {
                    for item in items {
                        normalize_provider_tool_parameters_schema(item);
                        inherit_parent_object_shape_into_combinator_branch(
                            parent_properties.as_ref(),
                            parent_additional_properties.as_ref(),
                            item,
                        );
                    }
                }
            }

            if let Some(branch) = object.get_mut("not") {
                normalize_provider_tool_parameters_schema(branch);
                inherit_parent_object_shape_into_combinator_branch(
                    parent_properties.as_ref(),
                    parent_additional_properties.as_ref(),
                    branch,
                );
            }
        }
        Value::Array(items) => {
            for item in items {
                normalize_provider_tool_parameters_schema(item);
            }
        }
        _ => {}
    }
}

/// Host-owned unified tool manager that normalises descriptors, keeps the AI tool surface stable, and dispatches resolved tool calls.
/// 负责标准化工具描述、保持 AI 工具面稳定并调度已解析工具调用的宿主自有统一工具管理器。
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
    pub fn resolve_for_execution_mode(&self, _execution_mode_label: &str) -> ResolvedToolSet {
        let mut descriptors = Vec::new();
        let mut definitions = Vec::new();

        for candidate in self
            .candidates
            .iter()
            .filter(|candidate| candidate.ai_visible)
        {
            descriptors.push(candidate.descriptor.clone());
            definitions.push(ResolvedToolDefinition {
                descriptor: candidate.descriptor.clone(),
                parameters_schema: candidate.parameters_schema.clone(),
                plan_policy: candidate.plan_policy.clone(),
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
        execution_mode_label: &str,
    ) -> Result<VulcanToolInvocationResult, String> {
        let mode = VulcanToolMode::from_execution_mode_label(execution_mode_label);
        if matches!(mode, VulcanToolMode::Plan) && !tool_is_allowed_in_plan(tool) {
            return Ok(VulcanToolInvocationResult::from_model_output(
                Value::String(String::from(
                    "Plan Mode is active. This tool call is not allowed because it may mutate workspace, repository, Git, or external state. Continue by refining the plan and wait for the user to leave Plan Mode before implementing changes.",
                )),
            ));
        }
        (tool.invoke)(args, session_id).await
    }
}

/// Returns whether one OpenAI-compatible request should use the Gemini schema dialect.
/// 返回单个 OpenAI 兼容请求是否应使用 Gemini Schema 方言。
fn openai_compatible_request_targets_gemini(
    context: &VulcanToolSchemaProjectionContext<'_>,
) -> bool {
    if !context
        .api_family_key
        .to_ascii_lowercase()
        .contains("openai")
    {
        return false;
    }

    if model_id_looks_like_gemini(context.model_id) {
        return true;
    }

    context.provider_id.eq_ignore_ascii_case("openrouter")
        && openrouter_additional_params_target_gemini(context.additional_params)
}

/// Returns whether one model identifier clearly belongs to the Gemini/Gemma family.
/// 返回单个模型标识是否明确属于 Gemini/Gemma 模型族。
fn model_id_looks_like_gemini(model_id: &str) -> bool {
    let normalized = model_id.to_ascii_lowercase();
    normalized.contains("gemini")
        || normalized.contains("gemma")
        || normalized.starts_with("google/")
        || normalized.starts_with("google-")
}

/// Returns whether OpenRouter additional parameters pin routing to a Gemini-like endpoint.
/// 返回 OpenRouter 附加参数是否将路由固定到类似 Gemini 的端点。
fn openrouter_additional_params_target_gemini(additional_params: Option<&Value>) -> bool {
    let Some(Value::Object(root)) = additional_params else {
        return false;
    };
    let Some(Value::Object(provider)) = root.get("provider") else {
        return false;
    };
    provider.values().any(value_contains_gemini_hint)
}

/// Returns whether one arbitrary JSON value contains a Gemini-like routing hint.
/// 返回单个任意 JSON 值是否包含类似 Gemini 的路由提示。
fn value_contains_gemini_hint(value: &Value) -> bool {
    match value {
        Value::String(text) => model_id_looks_like_gemini(text),
        Value::Array(items) => items.iter().any(value_contains_gemini_hint),
        Value::Object(object) => object.values().any(value_contains_gemini_hint),
        _ => false,
    }
}

/// Rewrites JSON-Schema multi-type arrays into strict-validator-compatible scalar, nullable, or `anyOf` forms.
/// 将 JSON Schema 的多类型数组重写为严格校验器兼容的标量、可空或 `anyOf` 形式。
fn normalize_schema_multi_type_union(object: &mut Map<String, Value>) {
    let Some(Value::Array(type_values)) = object.get("type").cloned() else {
        return;
    };

    let mut nullable = object
        .get("nullable")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let mut normalized_types = Vec::<String>::new();

    for type_value in type_values {
        let Some(type_name) = type_value.as_str() else {
            return;
        };
        if type_name.eq_ignore_ascii_case("null") {
            nullable = true;
            continue;
        }
        if !normalized_types
            .iter()
            .any(|existing| existing == type_name)
        {
            normalized_types.push(type_name.to_string());
        }
    }

    match normalized_types.len() {
        0 => {
            object.insert(String::from("type"), Value::String(String::from("null")));
        }
        1 => {
            object.insert(
                String::from("type"),
                Value::String(normalized_types[0].clone()),
            );
        }
        _ => {
            object.remove("type");
            let mut any_of = normalized_types
                .into_iter()
                .map(|type_name| {
                    Value::Object(Map::from_iter([(
                        String::from("type"),
                        Value::String(type_name),
                    )]))
                })
                .collect::<Vec<_>>();
            if let Some(Value::Array(mut existing_any_of)) = object.remove("anyOf") {
                any_of.append(&mut existing_any_of);
            }
            object.insert(String::from("anyOf"), Value::Array(any_of));
        }
    }

    if nullable {
        object.insert(String::from("nullable"), Value::Bool(true));
    }
}

/// Returns whether one schema branch semantically behaves like an object schema.
/// 返回单个 Schema 分支是否在语义上表现为对象 Schema。
fn schema_branch_looks_like_object(object: &Map<String, Value>) -> bool {
    [
        "properties",
        "required",
        "additionalProperties",
        "patternProperties",
        "propertyNames",
        "minProperties",
        "maxProperties",
        "dependentRequired",
        "dependentSchemas",
        "unevaluatedProperties",
    ]
    .iter()
    .any(|key| object.contains_key(*key))
}

/// Returns whether one schema branch semantically behaves like an array schema.
/// 返回单个 Schema 分支是否在语义上表现为数组 Schema。
fn schema_branch_looks_like_array(object: &Map<String, Value>) -> bool {
    [
        "items",
        "prefixItems",
        "contains",
        "minItems",
        "maxItems",
        "uniqueItems",
        "unevaluatedItems",
    ]
    .iter()
    .any(|key| object.contains_key(*key))
}

/// Copies the parent object-shape view into one combinator branch for stricter validators.
/// 为更严格的校验器将父级对象形状视图复制到单个组合分支。
fn inherit_parent_object_shape_into_combinator_branch(
    parent_properties: Option<&Value>,
    parent_additional_properties: Option<&Value>,
    branch: &mut Value,
) {
    let Some(branch_object) = branch.as_object_mut() else {
        return;
    };
    if branch_object.get("type").and_then(Value::as_str) != Some("object") {
        return;
    }

    if branch_object.contains_key("properties") {
        return;
    }

    if let Some(parent_properties) = parent_properties {
        branch_object.insert(String::from("properties"), parent_properties.clone());
    }
    if !branch_object.contains_key("additionalProperties") {
        if let Some(additional_properties) = parent_additional_properties {
            branch_object.insert(
                String::from("additionalProperties"),
                additional_properties.clone(),
            );
        }
    }
}

/// Projects one schema into the compact Gemini tool-declaration dialect.
/// 将单个 Schema 投影为紧凑的 Gemini 工具声明方言。
fn project_gemini_slim_schema(schema: &Value) -> Value {
    project_gemini_node(&sanitize_gemini_node(schema)).unwrap_or_else(|| {
        Value::Object(Map::from_iter([(
            String::from("type"),
            Value::String(String::from("object")),
        )]))
    })
}

/// Sanitizes common JSON Schema authoring shapes before Gemini projection.
/// 在 Gemini 投影前清理常见 JSON Schema 编写形态。
fn sanitize_gemini_node(schema: &Value) -> Value {
    match schema {
        Value::Object(object) => sanitize_gemini_object(object),
        Value::Array(items) => Value::Array(items.iter().map(sanitize_gemini_node).collect()),
        other => other.clone(),
    }
}

/// Sanitizes one object-shaped JSON Schema node before Gemini projection.
/// 在 Gemini 投影前清理单个对象形 JSON Schema 节点。
fn sanitize_gemini_object(object: &Map<String, Value>) -> Value {
    let mut sanitized = Map::new();
    for (key, value) in object {
        if key == "enum" {
            if let Value::Array(items) = value {
                sanitized.insert(
                    key.clone(),
                    Value::Array(
                        items
                            .iter()
                            .map(|item| Value::String(enum_value_to_string(item)))
                            .collect(),
                    ),
                );
                continue;
            }
        }
        sanitized.insert(key.clone(), sanitize_gemini_node(value));
    }

    if matches!(
        sanitized.get("type").and_then(Value::as_str),
        Some("number" | "integer")
    ) && sanitized.contains_key("enum")
    {
        sanitized.insert(String::from("type"), Value::String(String::from("string")));
    }

    infer_gemini_container_type_without_mode_branch_expansion(&mut sanitized);
    filter_gemini_required_to_declared_properties(&mut sanitized);
    ensure_gemini_array_items(&mut sanitized);
    remove_gemini_scalar_object_keywords(&mut sanitized);

    Value::Object(sanitized)
}

/// Infers missing Gemini container types without expanding required-only mode-selector branches.
/// 在不膨胀仅含 required 的模式选择分支的前提下推断缺失的 Gemini 容器类型。
fn infer_gemini_container_type_without_mode_branch_expansion(object: &mut Map<String, Value>) {
    if object.contains_key("type") {
        return;
    }
    if object.contains_key("properties")
        || object.contains_key("additionalProperties")
        || object.contains_key("patternProperties")
        || object.contains_key("required")
    {
        object.insert(String::from("type"), Value::String(String::from("object")));
        return;
    }
    if object.contains_key("items") || object.contains_key("prefixItems") {
        object.insert(String::from("type"), Value::String(String::from("array")));
    }
}

/// Converts one enum value into the string form accepted by Gemini function declarations.
/// 将单个枚举值转换为 Gemini 函数声明可接受的字符串形式。
fn enum_value_to_string(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

/// Filters `required` entries to names declared in the same `properties` object.
/// 将 `required` 条目过滤为同一 `properties` 对象中声明过的字段名。
fn filter_gemini_required_to_declared_properties(object: &mut Map<String, Value>) {
    let Some(Value::Object(properties)) = object.get("properties") else {
        return;
    };
    let declared = properties.keys().cloned().collect::<BTreeSet<_>>();
    let Some(Value::Array(required)) = object.get_mut("required") else {
        return;
    };
    required.retain(|item| {
        item.as_str()
            .map(|name| declared.contains(name))
            .unwrap_or(false)
    });
}

/// Ensures array schemas have an item schema when no combinator already defines the item shape.
/// 当没有组合器定义元素形状时，确保数组 Schema 拥有元素 Schema。
fn ensure_gemini_array_items(object: &mut Map<String, Value>) {
    if object.get("type").and_then(Value::as_str) != Some("array") {
        return;
    }
    if schema_has_combinator(object) {
        return;
    }
    let needs_default_items = object
        .get("items")
        .map(|items| !schema_node_has_intent(items))
        .unwrap_or(true);
    if needs_default_items {
        object.insert(
            String::from("items"),
            Value::Object(Map::from_iter([(
                String::from("type"),
                Value::String(String::from("string")),
            )])),
        );
    }
}

/// Removes object-only keywords from non-object scalar schemas when no combinator needs them.
/// 当没有组合器需要这些字段时，从非对象标量 Schema 中移除对象专属关键字。
fn remove_gemini_scalar_object_keywords(object: &mut Map<String, Value>) {
    if schema_has_combinator(object) {
        return;
    }
    let Some(type_name) = object.get("type").and_then(Value::as_str) else {
        return;
    };
    if type_name == "object" {
        return;
    }
    object.remove("properties");
    object.remove("required");
}

/// Returns whether one schema object contains a JSON Schema combinator.
/// 返回单个 Schema 对象是否包含 JSON Schema 组合器。
fn schema_has_combinator(object: &Map<String, Value>) -> bool {
    ["anyOf", "oneOf", "allOf"]
        .iter()
        .any(|key| object.contains_key(*key))
}

/// Returns whether one schema node carries enough structural intent to preserve.
/// 返回单个 Schema 节点是否携带足够的结构意图而应被保留。
fn schema_node_has_intent(value: &Value) -> bool {
    let Some(object) = value.as_object() else {
        return !value.is_null();
    };
    [
        "type",
        "properties",
        "items",
        "enum",
        "const",
        "anyOf",
        "oneOf",
        "allOf",
        "$ref",
    ]
    .iter()
    .any(|key| object.contains_key(*key))
}

/// Projects one sanitized schema node into the Gemini allowlisted schema dialect.
/// 将单个已清理 Schema 节点投影到 Gemini 白名单 Schema 方言。
fn project_gemini_node(schema: &Value) -> Option<Value> {
    let object = schema.as_object()?;
    let mut projected = Map::new();

    insert_optional_clone(&mut projected, object, "description");
    insert_optional_clone(&mut projected, object, "required");
    insert_optional_clone(&mut projected, object, "format");
    insert_gemini_projected_type(&mut projected, object);
    insert_optional_clone(&mut projected, object, "nullable");
    insert_optional_clone(&mut projected, object, "enum");
    insert_projected_gemini_const(&mut projected, object);
    insert_projected_gemini_properties(&mut projected, object);
    insert_projected_gemini_items(&mut projected, object);
    insert_projected_gemini_combinator(&mut projected, object, "allOf");
    insert_projected_gemini_combinator(&mut projected, object, "anyOf");
    insert_projected_gemini_combinator(&mut projected, object, "oneOf");
    insert_optional_clone(&mut projected, object, "minLength");
    hydrate_gemini_required_combinator_branch_properties(&mut projected);

    if projected.get("type").and_then(Value::as_str) == Some("object")
        && !projected.contains_key("properties")
        && !projected.contains_key("required")
    {
        return None;
    }

    (!projected.is_empty()).then_some(Value::Object(projected))
}

/// Inserts one cloned allowlisted field when it exists.
/// 当字段存在时插入单个克隆的白名单字段。
fn insert_optional_clone(target: &mut Map<String, Value>, source: &Map<String, Value>, key: &str) {
    if let Some(value) = source.get(key) {
        target.insert(key.to_string(), value.clone());
    }
}

/// Inserts the Gemini-projected `type` field and nullable marker.
/// 插入 Gemini 投影后的 `type` 字段与可空标记。
fn insert_gemini_projected_type(target: &mut Map<String, Value>, source: &Map<String, Value>) {
    match source.get("type") {
        Some(Value::Array(types)) => {
            let mut nullable = source
                .get("nullable")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            let mut first_non_null = None::<String>;
            for value in types {
                let Some(type_name) = value.as_str() else {
                    continue;
                };
                if type_name.eq_ignore_ascii_case("null") {
                    nullable = true;
                } else if first_non_null.is_none() {
                    first_non_null = Some(type_name.to_string());
                }
            }
            if let Some(type_name) = first_non_null {
                target.insert(String::from("type"), Value::String(type_name));
            }
            if nullable {
                target.insert(String::from("nullable"), Value::Bool(true));
            }
        }
        Some(value) => {
            target.insert(String::from("type"), value.clone());
        }
        None => {}
    }
}

/// Inserts a single-value enum converted from a JSON Schema `const` value.
/// 插入由 JSON Schema `const` 值转换而来的单值枚举。
fn insert_projected_gemini_const(target: &mut Map<String, Value>, source: &Map<String, Value>) {
    if target.contains_key("enum") {
        return;
    }
    if let Some(value) = source.get("const") {
        target.insert(String::from("enum"), Value::Array(vec![value.clone()]));
    }
}

/// Inserts projected Gemini object properties when at least one child schema remains.
/// 当至少一个子 Schema 保留时插入投影后的 Gemini 对象属性。
fn insert_projected_gemini_properties(
    target: &mut Map<String, Value>,
    source: &Map<String, Value>,
) {
    let Some(Value::Object(properties)) = source.get("properties") else {
        return;
    };
    let projected_properties = properties
        .iter()
        .filter_map(|(key, value)| project_gemini_node(value).map(|schema| (key.clone(), schema)))
        .collect::<Map<String, Value>>();
    if !projected_properties.is_empty() {
        target.insert(
            String::from("properties"),
            Value::Object(projected_properties),
        );
    }
}

/// Inserts projected Gemini array items when a child schema remains.
/// 当子 Schema 保留时插入投影后的 Gemini 数组元素。
fn insert_projected_gemini_items(target: &mut Map<String, Value>, source: &Map<String, Value>) {
    let Some(items) = source.get("items") else {
        return;
    };
    match items {
        Value::Array(values) => {
            let projected_items = values
                .iter()
                .filter_map(project_gemini_node)
                .collect::<Vec<_>>();
            if !projected_items.is_empty() {
                target.insert(String::from("items"), Value::Array(projected_items));
            }
        }
        value => {
            if let Some(projected) = project_gemini_node(value) {
                target.insert(String::from("items"), projected);
            }
        }
    }
}

/// Inserts one projected Gemini combinator when it contains at least one child schema.
/// 当组合器至少包含一个子 Schema 时插入单个投影后的 Gemini 组合器。
fn insert_projected_gemini_combinator(
    target: &mut Map<String, Value>,
    source: &Map<String, Value>,
    key: &str,
) {
    let Some(Value::Array(values)) = source.get(key) else {
        return;
    };
    let projected_values = values
        .iter()
        .filter_map(project_gemini_node)
        .collect::<Vec<_>>();
    if !projected_values.is_empty() {
        target.insert(key.to_string(), Value::Array(projected_values));
    }
}

/// Hydrates Gemini combinator branches with only the property schemas referenced by their own `required` list.
/// 仅用组合分支自身 `required` 列表引用到的属性 Schema 补充分支，满足 Gemini 严格校验并控制 Token 膨胀。
fn hydrate_gemini_required_combinator_branch_properties(schema: &mut Map<String, Value>) {
    let Some(Value::Object(parent_properties)) = schema.get("properties").cloned() else {
        return;
    };

    for key in ["allOf", "anyOf", "oneOf"] {
        let Some(Value::Array(branches)) = schema.get_mut(key) else {
            continue;
        };
        for branch in branches {
            hydrate_gemini_required_branch_properties(branch, &parent_properties);
        }
    }
}

/// Hydrates one Gemini object branch with a minimal `properties` map matching its declared required fields.
/// 使用与必填字段匹配的最小 `properties` 映射补充单个 Gemini 对象分支。
fn hydrate_gemini_required_branch_properties(
    branch: &mut Value,
    parent_properties: &Map<String, Value>,
) {
    let Some(branch_object) = branch.as_object_mut() else {
        return;
    };
    if branch_object.get("type").and_then(Value::as_str) != Some("object") {
        return;
    }
    if branch_object.contains_key("properties") {
        return;
    }

    let required_names = branch_object
        .get("required")
        .and_then(Value::as_array)
        .map(|required| {
            required
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if required_names.is_empty() {
        return;
    }

    let mut required_properties = Map::new();
    let mut retained_required = Vec::new();
    for required_name in required_names {
        let Some(property_schema) = parent_properties.get(&required_name) else {
            continue;
        };
        required_properties.insert(required_name.clone(), property_schema.clone());
        retained_required.push(Value::String(required_name));
    }

    if required_properties.is_empty() {
        branch_object.remove("required");
        return;
    }

    branch_object.insert(String::from("required"), Value::Array(retained_required));
    branch_object.insert(
        String::from("properties"),
        Value::Object(required_properties),
    );
}

/// Recursively normalizes one OpenAI Responses JSON schema for strict tool mode.
/// 为严格工具模式递归规范化单个 OpenAI Responses JSON Schema。
fn sanitize_openai_responses_schema(schema: &mut Value) {
    if let Value::Object(object) = schema {
        if object.contains_key("$ref") {
            object.retain(|key, _| key == "$ref");
            return;
        }

        let is_object_schema = object.get("type") == Some(&Value::String(String::from("object")))
            || object.contains_key("properties");

        if is_object_schema && !object.contains_key("additionalProperties") {
            object.insert(String::from("additionalProperties"), Value::Bool(false));
        }

        if let Some(Value::Object(properties)) = object.get("properties") {
            object.insert(
                String::from("required"),
                Value::Array(properties.keys().cloned().map(Value::String).collect()),
            );
        }

        if let Some(Value::Object(definitions)) = object.get_mut("$defs") {
            for value in definitions.values_mut() {
                sanitize_openai_responses_schema(value);
            }
        }

        if let Some(Value::Object(properties)) = object.get_mut("properties") {
            for value in properties.values_mut() {
                sanitize_openai_responses_schema(value);
            }
        }

        if let Some(items) = object.get_mut("items") {
            sanitize_openai_responses_schema(items);
        }

        if let Some(one_of) = object.remove("oneOf") {
            match object.get_mut("anyOf") {
                Some(Value::Array(existing)) => {
                    if let Value::Array(mut incoming) = one_of {
                        existing.append(&mut incoming);
                    }
                }
                _ => {
                    object.insert(String::from("anyOf"), one_of);
                }
            }
        }

        for key in ["anyOf", "oneOf", "allOf"] {
            if let Some(Value::Array(variants)) = object.get_mut(key) {
                for variant in variants {
                    sanitize_openai_responses_schema(variant);
                }
            }
        }
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

/// Returns whether one resolved tool may execute while Plan mode is active.
/// 返回单个已解析工具在计划模式激活时是否允许执行。
fn tool_is_allowed_in_plan(tool: &ResolvedToolDefinition) -> bool {
    match &tool.plan_policy {
        VulcanPlanToolPolicy::Hidden => false,
        VulcanPlanToolPolicy::Visible => true,
        VulcanPlanToolPolicy::HeuristicText(text) => {
            let text = text.to_ascii_lowercase();
            DEFAULT_PLAN_READ_ONLY_KEYWORDS
                .iter()
                .any(|keyword| text.contains(keyword))
        }
    }
}

/// Shared read-only heuristic keywords used by the Plan-mode execution guard.
/// Plan 模式执行守卫使用的共享只读启发式关键字集合。
const DEFAULT_PLAN_READ_ONLY_KEYWORDS: &[&str] = &[
    "read",
    "list",
    "rg",
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
        VulcanToolInvocationResult, VulcanToolMode, VulcanToolSchemaProjectionContext,
        VulcanToolSchemaProjectionProfile, VulcanToolSourceKind, VulcanToolsManager,
        project_tool_parameters_schema, public_tool_name_is_valid,
        resolve_tool_schema_projection_profile,
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
                help_text: None,
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

    /// Verifies Plan mode keeps the provider-visible tool surface stable after public-name normalization.
    /// 验证 Plan 模式会在公开名称归一化后保持 Provider 可见工具面稳定。
    #[test]
    fn manager_keeps_plan_mode_tool_surface_stable_after_public_name_normalization() {
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
            vec!["read_file", "write_file"]
        );
    }

    /// Verifies ripgrep-style rg tools are treated as Plan-mode read-only exploration.
    /// 验证 rg 风格工具会在计划模式中被视为只读探索工具。
    #[test]
    fn manager_allows_rg_heuristic_tool_in_plan_mode() {
        let manager = VulcanToolsManager::new(vec![test_candidate(
            "codekit-rg",
            VulcanToolSourceKind::LuaSkills,
            VulcanPlanToolPolicy::HeuristicText(String::from("rg")),
        )]);
        let resolved = manager.resolve_for_execution_mode("Plan");
        let tool = resolved
            .definitions
            .first()
            .expect("resolved rg tool should exist");

        assert!(super::tool_is_allowed_in_plan(tool));
    }

    /// Verifies native command-line tools remain executable in Plan mode for exploration and validation.
    /// 验证原生命令行工具在计划模式下仍可用于探索与验证。
    #[test]
    fn manager_allows_command_line_tools_in_plan_mode() {
        let manager = VulcanToolsManager::new(vec![
            test_candidate(
                "shell_command",
                VulcanToolSourceKind::Native,
                VulcanPlanToolPolicy::Visible,
            ),
            test_candidate(
                "terminal_session",
                VulcanToolSourceKind::Native,
                VulcanPlanToolPolicy::Visible,
            ),
        ]);
        let resolved = manager.resolve_for_execution_mode("Plan");

        assert_eq!(
            resolved_names(resolved.clone()),
            vec!["shell_command", "terminal_session"]
        );
        assert!(
            resolved
                .definitions
                .iter()
                .all(super::tool_is_allowed_in_plan)
        );
    }

    /// Verifies OpenRouter Gemini-family models select the compact Gemini projection even on an OpenAI-compatible wire API.
    /// 验证 OpenRouter 的 Gemini 模型即使走 OpenAI 兼容线协议也会选择紧凑 Gemini 投影。
    #[test]
    fn projection_profile_detects_openrouter_gemini_models() {
        let context = VulcanToolSchemaProjectionContext {
            provider_id: "openrouter",
            api_family_key: "openai-chat-compatible",
            model_id: "google/gemini-2.5-pro",
            additional_params: None,
        };

        assert_eq!(
            resolve_tool_schema_projection_profile(&context),
            VulcanToolSchemaProjectionProfile::GeminiSlim
        );
    }

    /// Verifies OpenRouter pinned endpoint hints can select the Gemini projection for aliased models.
    /// 验证 OpenRouter 固定端点提示可为别名模型选择 Gemini 投影。
    #[test]
    fn projection_profile_detects_openrouter_gemini_endpoint_hints() {
        let params = json!({
            "provider": {
                "order": ["google-ai-studio"],
                "allow_fallbacks": false
            }
        });
        let context = VulcanToolSchemaProjectionContext {
            provider_id: "openrouter",
            api_family_key: "openai-chat-compatible",
            model_id: "alias/model",
            additional_params: Some(&params),
        };

        assert_eq!(
            resolve_tool_schema_projection_profile(&context),
            VulcanToolSchemaProjectionProfile::GeminiSlim
        );
    }

    /// Verifies the Gemini projection keeps mode selectors compact while hydrating only required branch properties.
    /// 验证 Gemini 投影保持模式选择器紧凑，同时仅补充分支必填字段对应的属性 Schema。
    #[test]
    fn gemini_slim_projection_keeps_oneof_branches_required_only() {
        let schema = json!({
            "type": "object",
            "additionalProperties": false,
            "properties": {
                "file": { "type": "string", "description": "Target file." },
                "replacement": { "type": "string" },
                "patches": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "properties": {
                            "file": { "type": "string" },
                            "replacement": { "type": "string" }
                        },
                        "required": ["file", "replacement"]
                    }
                }
            },
            "oneOf": [
                { "required": ["file", "replacement"] },
                { "required": ["patches"] }
            ]
        });

        let projected =
            project_tool_parameters_schema(&schema, VulcanToolSchemaProjectionProfile::GeminiSlim);

        assert_eq!(projected["type"], json!("object"));
        assert!(projected["additionalProperties"].is_null());
        assert_eq!(
            projected["oneOf"],
            json!([
                {
                    "type": "object",
                    "required": ["file", "replacement"],
                    "properties": {
                        "file": { "type": "string", "description": "Target file." },
                        "replacement": { "type": "string" }
                    }
                },
                {
                    "type": "object",
                    "required": ["patches"],
                    "properties": {
                        "patches": {
                            "type": "array",
                            "items": {
                                "type": "object",
                                "properties": {
                                    "file": { "type": "string" },
                                    "replacement": { "type": "string" }
                                },
                                "required": ["file", "replacement"]
                            }
                        }
                    }
                }
            ])
        );
        assert!(projected["oneOf"][0]["properties"]["patches"].is_null());
    }

    /// Verifies nested Gemini `anyOf` required-only aliases hydrate minimal required properties.
    /// 验证嵌套 Gemini `anyOf` 必填别名分支会补充最小必填属性 Schema。
    #[test]
    fn gemini_slim_projection_marks_nested_required_only_anyof_branches_as_objects() {
        let schema = json!({
            "type": "object",
            "properties": {
                "basic": {
                    "type": "object",
                    "description": "Optional basic auth shortcut object.",
                    "properties": {
                        "username": { "type": "string" },
                        "password": { "type": "string" },
                        "user": { "type": "string" },
                        "pass": { "type": "string" }
                    },
                    "anyOf": [
                        { "required": ["username", "password"] },
                        { "required": ["username", "pass"] },
                        { "required": ["user", "password"] },
                        { "required": ["user", "pass"] }
                    ]
                }
            }
        });

        let projected =
            project_tool_parameters_schema(&schema, VulcanToolSchemaProjectionProfile::GeminiSlim);

        assert_eq!(
            projected["properties"]["basic"]["anyOf"],
            json!([
                {
                    "type": "object",
                    "required": ["username", "password"],
                    "properties": {
                        "username": { "type": "string" },
                        "password": { "type": "string" }
                    }
                },
                {
                    "type": "object",
                    "required": ["username", "pass"],
                    "properties": {
                        "username": { "type": "string" },
                        "pass": { "type": "string" }
                    }
                },
                {
                    "type": "object",
                    "required": ["user", "password"],
                    "properties": {
                        "user": { "type": "string" },
                        "password": { "type": "string" }
                    }
                },
                {
                    "type": "object",
                    "required": ["user", "pass"],
                    "properties": {
                        "user": { "type": "string" },
                        "pass": { "type": "string" }
                    }
                }
            ])
        );
        assert!(projected["properties"]["basic"]["anyOf"][0]["properties"]["user"].is_null());
    }

    /// Verifies the Gemini projection collapses nullable multi-type arrays without expanding them into token-heavy `anyOf` branches.
    /// 验证 Gemini 投影会压缩可空多类型数组，而不会膨胀成高 Token 成本的 `anyOf` 分支。
    #[test]
    fn gemini_slim_projection_collapses_nullable_type_arrays() {
        let schema = json!({
            "type": "object",
            "properties": {
                "value": {
                    "type": ["string", "null"],
                    "description": "Optional value."
                }
            }
        });

        let projected =
            project_tool_parameters_schema(&schema, VulcanToolSchemaProjectionProfile::GeminiSlim);

        assert_eq!(projected["properties"]["value"]["type"], json!("string"));
        assert_eq!(projected["properties"]["value"]["nullable"], json!(true));
        assert!(projected["properties"]["value"]["anyOf"].is_null());
    }

    /// Verifies OpenAI Responses strict projection still adds strict object constraints recursively.
    /// 验证 OpenAI Responses 严格投影仍会递归补齐严格对象约束。
    #[test]
    fn openai_responses_strict_projection_adds_required_object_constraints() {
        let schema = json!({
            "type": "object",
            "properties": {
                "location": {
                    "type": "object",
                    "properties": {
                        "city": { "type": "string" }
                    }
                }
            }
        });

        let projected = project_tool_parameters_schema(
            &schema,
            VulcanToolSchemaProjectionProfile::OpenAiResponsesStrict,
        );

        assert_eq!(projected["additionalProperties"], json!(false));
        assert_eq!(projected["required"], json!(["location"]));
        assert_eq!(
            projected["properties"]["location"]["additionalProperties"],
            json!(false)
        );
        assert_eq!(
            projected["properties"]["location"]["required"],
            json!(["city"])
        );
    }
}
