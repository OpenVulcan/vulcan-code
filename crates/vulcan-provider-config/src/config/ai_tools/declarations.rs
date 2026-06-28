use super::*;

/// Routing policy used when one AI-specialized tool has both provider-native and host fallback options.
/// 当单个 AI 专用工具同时存在 Provider 原生与宿主回退选项时使用的路由策略。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AiToolPolicy {
    /// Prefer provider-native tools and fall back to host tools when configured.
    /// 优先使用 Provider 原生工具，并在已配置时回退到宿主工具。
    Auto,
    /// Require provider-native support.
    /// 强制要求 Provider 原生支持。
    ForceNative,
    /// Require a host fallback tool.
    /// 强制使用宿主回退工具。
    ForceHost,
    /// Do not expose the tool.
    /// 不暴露该工具。
    Disabled,
}

impl Default for AiToolPolicy {
    /// Returns the safe default AI-tool policy.
    /// 返回安全的默认 AI 工具策略。
    fn default() -> Self {
        Self::Auto
    }
}

/// Declarative AI-tool capability or override attached to one provider or model catalog entry.
/// 附着在单个供应商或模型目录条目上的声明式 AI 工具能力或覆盖配置。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiToolDeclaration {
    /// Stable semantic tool kind.
    /// 稳定语义工具类型。
    pub kind: AiToolKind,
    /// Routing policy used by the AI-tool router.
    /// AI 工具路由器使用的路由策略。
    #[serde(default, skip_serializing_if = "is_default_ai_tool_policy")]
    pub policy: AiToolPolicy,
    /// Whether provider-native execution is available for this tool.
    /// 当前工具是否可使用 Provider 原生执行。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub native_provider_tool: Option<bool>,
    /// Provider-native tool type such as `web_search_preview` or `google_search`.
    /// Provider 原生工具类型，例如 `web_search_preview` 或 `google_search`。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub native_tool_type: Option<String>,
    /// Optional provider-native request patch appended to the outgoing payload when this hosted tool is enabled.
    /// 启用当前 Hosted Tool 时追加到出站请求载荷中的可选 Provider 原生请求补丁。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub native_request_patch: Option<Value>,
    /// Optional host fallback tool identifier used when native support is unavailable or disabled.
    /// 当原生支持不可用或被禁用时使用的可选宿主回退工具标识。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fallback_tool_id: Option<String>,
    /// Whether this tool requires explicit user approval before execution.
    /// 当前工具执行前是否需要用户显式批准。
    #[serde(default, skip_serializing_if = "is_false")]
    pub requires_approval: bool,
    /// Modalities that may be returned by this tool.
    /// 当前工具可能返回的模态集合。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub result_modalities: Vec<String>,
    /// Whether the tool can return structured citations.
    /// 当前工具是否可以返回结构化引用。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub citation_support: Option<bool>,
}

impl AiToolDeclaration {
    /// Builds one provider-native declaration for a semantic AI tool.
    /// 为单个语义 AI 工具构建 Provider 原生声明。
    pub fn provider_native(kind: AiToolKind, native_tool_type: impl Into<String>) -> Self {
        Self {
            kind,
            policy: AiToolPolicy::Auto,
            native_provider_tool: Some(true),
            native_tool_type: Some(native_tool_type.into()),
            native_request_patch: None,
            fallback_tool_id: None,
            requires_approval: false,
            result_modalities: vec![String::from("text")],
            citation_support: Some(matches!(
                kind,
                AiToolKind::WebSearch | AiToolKind::FileSearch
            )),
        }
    }
}
