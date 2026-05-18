//! Provider capability resolution helpers shared by the runtime, diagnostics, and future UI guards.
//! 供运行时、诊断与后续界面防护共用的 Provider 能力解析辅助模块。

use crate::provider_config::{
    AiToolDeclaration, AiToolKind, AiToolPolicy, IMAGE_GENERATION_PROVIDER_CHAIN_TOOL_ID,
    MUSIC_COVER_PROVIDER_CHAIN_TOOL_ID, MUSIC_GENERATION_PROVIDER_CHAIN_TOOL_ID, ModelChoice,
    ModelInterleavedCapability, NativeBridgeRoutingMode, PersistedAiToolConfiguration,
    PersistedProviderOptions, ProviderApiFamily, SPEECH_SYNTHESIS_PROVIDER_CHAIN_TOOL_ID,
    SupportedProviderDefinition, UNDERSTAND_IMAGE_PROVIDER_CHAIN_TOOL_ID,
    VIDEO_GENERATION_PROVIDER_CHAIN_TOOL_ID, WEB_SEARCH_PROVIDER_CHAIN_TOOL_ID,
    WebSearchRoutingMode,
};
use std::collections::BTreeSet;
use vulcan_core::types::ConversationAttachmentKind;

/// Resolved provider/model capability snapshot consumed by the host instead of raw catalog JSON.
/// 宿主消费的已解析 Provider/Model 能力快照，用于替代原始目录 JSON 细节。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ResolvedModelCapabilities {
    /// Ordered provider-declared credential environment keys used as fallback lookup candidates.
    /// 作为回退查询候选项使用的按顺序排列的供应商声明凭据环境变量键集合。
    pub credential_env_keys: Vec<String>,
    /// Whether the selected model declares reasoning support.
    /// 当前选中模型是否声明支持推理能力。
    pub supports_reasoning: bool,
    /// Whether the selected model declares tool-calling support.
    /// 当前选中模型是否声明支持工具调用能力。
    pub supports_tool_call: bool,
    /// Whether the selected model accepts explicit temperature overrides.
    /// 当前选中模型是否接受显式温度覆盖。
    pub supports_temperature: bool,
    /// Whether the selected model declares the catalog-level `attachment` flag kept for diagnostics and metadata parity.
    /// 当前选中模型是否声明目录级 `attachment` 标记，用于诊断与元数据对齐。
    pub supports_attachment: bool,
    /// Optional lifecycle status label such as `stable` or `deprecated`.
    /// 可选的生命周期状态标签，例如 `stable` 或 `deprecated`。
    pub status: Option<String>,
    /// Optional family label attached to the selected model.
    /// 附着在当前选中模型上的可选家族标签。
    pub family: Option<String>,
    /// Optional release-date label attached to the selected model.
    /// 附着在当前选中模型上的可选发布日期标签。
    pub release_date: Option<String>,
    /// Optional provider/model context-window limit.
    /// Provider/Model 可选上下文窗口上限。
    pub context_limit: Option<usize>,
    /// Optional provider/model input-token limit.
    /// Provider/Model 可选输入 Token 上限。
    pub input_limit: Option<usize>,
    /// Optional provider/model output-token limit.
    /// Provider/Model 可选输出 Token 上限。
    pub output_limit: Option<usize>,
    /// Optional model-scoped interleaved reasoning descriptor.
    /// 可选的模型级交错推理描述。
    pub interleaved_reasoning: Option<ModelInterleavedCapability>,
    /// Normalized input modalities declared by the selected model.
    /// 当前选中模型声明的规范化输入模态集合。
    pub input_modalities: Vec<String>,
    /// Normalized output modalities declared by the selected model.
    /// 当前选中模型声明的规范化输出模态集合。
    pub output_modalities: Vec<String>,
    /// Resolved AI-specialized tool capabilities available to the selected provider/model pair.
    /// 当前供应商/模型组合可用的已解析 AI 专用工具能力集合。
    pub ai_tools: Vec<ResolvedAiToolCapability>,
}

/// Resolved AI-specialized tool capability consumed by the runtime router.
/// 供运行时路由器消费的已解析 AI 专用工具能力。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedAiToolCapability {
    /// Stable semantic tool kind.
    /// 稳定语义工具类型。
    pub kind: AiToolKind,
    /// Effective routing policy.
    /// 生效路由策略。
    pub policy: AiToolPolicy,
    /// Whether provider-native execution is available.
    /// 是否可用 Provider 原生执行。
    pub native_provider_tool: bool,
    /// Provider-native tool type used by the target API family.
    /// 目标接口族使用的 Provider 原生工具类型。
    pub native_tool_type: Option<String>,
    /// Optional request payload patch used to enable provider-native hosted tools.
    /// 用于启用 Provider 原生 Hosted Tool 的可选请求载荷补丁。
    pub native_request_patch: Option<serde_json::Value>,
    /// Optional host fallback tool identifier.
    /// 可选宿主回退工具标识。
    pub fallback_tool_id: Option<String>,
    /// Whether explicit approval is required.
    /// 是否需要显式批准。
    pub requires_approval: bool,
    /// Normalized result modalities.
    /// 规范化结果模态集合。
    pub result_modalities: Vec<String>,
    /// Whether structured citations are supported.
    /// 是否支持结构化引用。
    pub citation_support: bool,
    /// Human-readable source label used by diagnostics.
    /// 诊断界面使用的人类可读来源标签。
    pub source_label: String,
}

impl ResolvedModelCapabilities {
    /// Returns the effective input budget limit used by context-budget estimation and compaction checks.
    /// 返回供上下文预算估算与压缩检查使用的生效输入预算上限。
    pub fn effective_input_limit(&self) -> Option<usize> {
        self.input_limit.or(self.context_limit)
    }

    /// Returns the effective output reserve limit used by context-budget estimation.
    /// 返回供上下文预算估算使用的生效输出预留上限。
    pub fn effective_output_limit(&self) -> Option<usize> {
        self.output_limit
    }

    /// Returns whether the selected model still behaves as text-only from the host perspective.
    /// 返回从宿主视角看当前选中模型是否仍表现为纯文本模型。
    pub fn is_text_only(&self) -> bool {
        self.input_modalities.is_empty()
            || self
                .input_modalities
                .iter()
                .all(|modality| modality == "text")
    }

    /// Returns whether the selected model declares support for one specific normalized input modality.
    /// 返回当前选中模型是否声明支持某个指定的规范化输入模态。
    pub fn supports_input_modality(&self, modality: &str) -> bool {
        let normalized_modality = modality.trim().to_ascii_lowercase();
        if normalized_modality.is_empty() {
            return false;
        }
        if normalized_modality == "text" {
            return self.input_modalities.is_empty()
                || self
                    .input_modalities
                    .iter()
                    .any(|candidate| candidate == "text");
        }
        self.input_modalities
            .iter()
            .any(|candidate| candidate == &normalized_modality)
    }

    /// Returns whether the selected model declares support for the modality carried by one staged attachment kind.
    /// 返回当前选中模型是否声明支持某条已暂存附件类型所携带的模态。
    pub fn supports_attachment_kind(&self, kind: ConversationAttachmentKind) -> bool {
        self.supports_input_modality(kind.modality_name())
    }

    /// Returns whether the current capability set exposes one enabled semantic AI tool.
    /// 返回当前能力集合是否暴露某个已启用的语义 AI 工具。
    pub fn has_enabled_ai_tool(&self, kind: AiToolKind) -> bool {
        self.ai_tools.iter().any(|tool| {
            tool.kind == kind
                && tool.policy != AiToolPolicy::Disabled
                && (tool.native_provider_tool
                    || tool
                        .fallback_tool_id
                        .as_deref()
                        .is_some_and(|value| !value.trim().is_empty()))
        })
    }

    /// Returns whether the current capability set should still allow one staged attachment kind through host-side bridge tooling.
    /// 返回当前能力集合是否应通过宿主桥接工具仍允许某种已暂存附件类型进入输入框。
    pub fn allows_staged_attachment_kind(&self, kind: ConversationAttachmentKind) -> bool {
        if self.supports_attachment_kind(kind.clone()) {
            return true;
        }

        match kind {
            ConversationAttachmentKind::Image => {
                self.has_enabled_ai_tool(AiToolKind::UnderstandImage)
                    || self.has_enabled_ai_tool(AiToolKind::ImageGeneration)
                    || self.has_enabled_ai_tool(AiToolKind::VideoGeneration)
            }
            ConversationAttachmentKind::Audio => self.has_enabled_ai_tool(AiToolKind::MusicCover),
            ConversationAttachmentKind::Video | ConversationAttachmentKind::Pdf => false,
        }
    }

    /// Builds one compact lifecycle summary string for diagnostics surfaces.
    /// 为诊断表面构建一条紧凑生命周期摘要字符串。
    pub fn lifecycle_summary(&self) -> String {
        let mut parts = Vec::new();
        if let Some(status) = self.status.as_deref() {
            parts.push(format!("状态 `{status}`"));
        }
        if let Some(family) = self.family.as_deref() {
            parts.push(format!("家族 `{family}`"));
        }
        if let Some(release_date) = self.release_date.as_deref() {
            parts.push(format!("发布日期 `{release_date}`"));
        }
        if parts.is_empty() {
            String::from("未声明生命周期元数据")
        } else {
            parts.join(" · ")
        }
    }
}

/// Resolves one stable provider/model capability snapshot from the catalog definition and optional model entry.
/// 根据目录定义与可选模型条目解析一份稳定的 Provider/Model 能力快照。
pub fn resolve_model_capabilities(
    definition: &SupportedProviderDefinition,
    model: Option<&ModelChoice>,
) -> ResolvedModelCapabilities {
    ResolvedModelCapabilities {
        credential_env_keys: normalize_environment_key_list(&definition.credential_env_keys),
        supports_reasoning: model.and_then(|entry| entry.reasoning).unwrap_or(false),
        supports_tool_call: model.and_then(|entry| entry.tool_call).unwrap_or(true),
        supports_temperature: model.and_then(|entry| entry.temperature).unwrap_or(true),
        supports_attachment: model.and_then(|entry| entry.attachment).unwrap_or(false),
        status: model.and_then(|entry| trimmed_option(entry.status.as_deref())),
        family: model.and_then(|entry| trimmed_option(entry.family.as_deref())),
        release_date: model.and_then(|entry| trimmed_option(entry.release_date.as_deref())),
        context_limit: model.and_then(|entry| entry.context_limit),
        input_limit: model.and_then(|entry| entry.input_limit),
        output_limit: model.and_then(|entry| entry.output_limit),
        interleaved_reasoning: model.and_then(|entry| entry.interleaved.clone()),
        input_modalities: model
            .map(|entry| normalize_modality_list(&entry.input_modalities))
            .unwrap_or_default(),
        output_modalities: model
            .map(|entry| normalize_modality_list(&entry.output_modalities))
            .unwrap_or_default(),
        ai_tools: resolve_ai_tool_capabilities(definition, model),
    }
}

/// Resolves one effective provider/model capability snapshot after applying user-level AI tool routing overlays.
/// 在应用用户级 AI 工具路由覆盖后解析一份生效的 Provider/Model 能力快照。
pub fn resolve_effective_model_capabilities(
    definition: &SupportedProviderDefinition,
    model: Option<&ModelChoice>,
    configuration: &PersistedAiToolConfiguration,
) -> ResolvedModelCapabilities {
    let mut capabilities = resolve_model_capabilities(definition, model);
    apply_ai_tool_runtime_configuration(&mut capabilities, configuration);
    capabilities
}

/// Applies user-level API-native tool routing configuration to one resolved capability snapshot.
/// 将用户级 API 原生工具路由配置应用到一份已解析能力快照。
pub fn apply_ai_tool_runtime_configuration(
    capabilities: &mut ResolvedModelCapabilities,
    configuration: &PersistedAiToolConfiguration,
) {
    let model_supports_native_image = capabilities.supports_input_modality("image");
    apply_web_search_runtime_configuration(&mut capabilities.ai_tools, configuration);
    apply_understand_image_runtime_configuration(
        model_supports_native_image,
        &mut capabilities.ai_tools,
        configuration,
    );
    apply_generic_host_only_ai_tool_runtime_configuration(
        &mut capabilities.ai_tools,
        configuration,
        AiToolKind::ImageGeneration,
        IMAGE_GENERATION_PROVIDER_CHAIN_TOOL_ID,
        &["image"],
    );
    apply_generic_host_only_ai_tool_runtime_configuration(
        &mut capabilities.ai_tools,
        configuration,
        AiToolKind::VideoGeneration,
        VIDEO_GENERATION_PROVIDER_CHAIN_TOOL_ID,
        &["video"],
    );
    apply_generic_host_only_ai_tool_runtime_configuration(
        &mut capabilities.ai_tools,
        configuration,
        AiToolKind::SpeechSynthesis,
        SPEECH_SYNTHESIS_PROVIDER_CHAIN_TOOL_ID,
        &["audio"],
    );
    apply_generic_host_only_ai_tool_runtime_configuration(
        &mut capabilities.ai_tools,
        configuration,
        AiToolKind::MusicGeneration,
        MUSIC_GENERATION_PROVIDER_CHAIN_TOOL_ID,
        &["audio"],
    );
    apply_generic_host_only_ai_tool_runtime_configuration(
        &mut capabilities.ai_tools,
        configuration,
        AiToolKind::MusicCover,
        MUSIC_COVER_PROVIDER_CHAIN_TOOL_ID,
        &["audio"],
    );
    capabilities.ai_tools.sort_by_key(|tool| tool.kind);
}

/// Applies web-search routing preferences without merging unrelated MCP or LuaSkills tools.
/// 应用网页搜索路由偏好，同时不融合无关的 MCP 或 LuaSkills 工具。
fn apply_web_search_runtime_configuration(
    ai_tools: &mut Vec<ResolvedAiToolCapability>,
    root_configuration: &PersistedAiToolConfiguration,
) {
    let provider_chain_available =
        root_configuration.has_usable_targets_for_kind(AiToolKind::WebSearch);
    let existing_index = ai_tools
        .iter()
        .position(|tool| tool.kind == AiToolKind::WebSearch);
    let mut capability = existing_index
        .and_then(|index| ai_tools.get(index).cloned())
        .unwrap_or_else(default_configured_web_search_capability);

    match root_configuration.web_search.mode {
        WebSearchRoutingMode::Disabled => {
            capability.policy = AiToolPolicy::Disabled;
            capability.fallback_tool_id = None;
        }
        WebSearchRoutingMode::NativeOnly => {
            if existing_index.is_none() {
                return;
            }
            capability.policy = AiToolPolicy::ForceNative;
            capability.fallback_tool_id = None;
        }
        WebSearchRoutingMode::NativeFirst => {
            if existing_index.is_none() && !provider_chain_available {
                return;
            }
            capability.policy = if capability.native_provider_tool {
                AiToolPolicy::Auto
            } else {
                AiToolPolicy::ForceHost
            };
            capability.fallback_tool_id =
                provider_chain_available.then(|| WEB_SEARCH_PROVIDER_CHAIN_TOOL_ID.to_string());
        }
        WebSearchRoutingMode::ThirdPartyFirst => {
            if provider_chain_available {
                capability.policy = AiToolPolicy::ForceHost;
                capability.fallback_tool_id = Some(WEB_SEARCH_PROVIDER_CHAIN_TOOL_ID.to_string());
            } else if existing_index.is_some() {
                capability.policy = AiToolPolicy::ForceNative;
                capability.fallback_tool_id = None;
            } else {
                return;
            }
        }
        WebSearchRoutingMode::ThirdPartyOnly => {
            if provider_chain_available {
                capability.policy = AiToolPolicy::ForceHost;
                capability.fallback_tool_id = Some(WEB_SEARCH_PROVIDER_CHAIN_TOOL_ID.to_string());
            } else if existing_index.is_some() {
                capability.policy = AiToolPolicy::Disabled;
                capability.fallback_tool_id = None;
            } else {
                return;
            }
        }
    }

    capability.source_label = format!("{} · 用户策略", capability.source_label);
    if let Some(index) = existing_index {
        ai_tools[index] = capability;
    } else {
        ai_tools.push(capability);
    }
}

/// Applies standalone image-understanding preferences without exposing the tool when the selected model already supports native image input.
/// 应用独立图片理解偏好，同时避免在当前模型已原生支持图片输入时额外暴露该工具。
fn apply_understand_image_runtime_configuration(
    model_supports_native_image: bool,
    ai_tools: &mut Vec<ResolvedAiToolCapability>,
    root_configuration: &PersistedAiToolConfiguration,
) {
    let existing_index = ai_tools
        .iter()
        .position(|tool| tool.kind == AiToolKind::UnderstandImage);
    let host_targets_available =
        root_configuration.has_usable_targets_for_kind(AiToolKind::UnderstandImage);

    if !host_targets_available {
        if let Some(index) = existing_index {
            ai_tools.remove(index);
        }
        return;
    }

    match root_configuration.understand_image.mode {
        NativeBridgeRoutingMode::NativeOnly => {
            if let Some(index) = existing_index {
                ai_tools.remove(index);
            }
            return;
        }
        NativeBridgeRoutingMode::NativeFirst if model_supports_native_image => {
            if let Some(index) = existing_index {
                ai_tools.remove(index);
            }
            return;
        }
        NativeBridgeRoutingMode::NativeFirst | NativeBridgeRoutingMode::HostOnly => {}
    }

    let mut capability = existing_index
        .and_then(|index| ai_tools.get(index).cloned())
        .unwrap_or_else(default_configured_understand_image_capability);
    capability.policy = AiToolPolicy::ForceHost;
    capability.native_provider_tool = false;
    capability.native_tool_type = None;
    capability.native_request_patch = None;
    capability.fallback_tool_id = Some(UNDERSTAND_IMAGE_PROVIDER_CHAIN_TOOL_ID.to_string());
    capability.source_label = format!("{} · 用户策略", capability.source_label);

    if let Some(index) = existing_index {
        ai_tools[index] = capability;
    } else {
        ai_tools.push(capability);
    }
}

/// Applies one generic host-only capability whenever the user has enabled at least one target for that semantic tool.
/// 当用户至少为某个语义工具启用了一条目标时，应用一条通用宿主侧能力配置。
fn apply_generic_host_only_ai_tool_runtime_configuration(
    ai_tools: &mut Vec<ResolvedAiToolCapability>,
    configuration: &PersistedAiToolConfiguration,
    capability_kind: AiToolKind,
    fallback_tool_id: &str,
    result_modalities: &[&str],
) {
    let existing_index = ai_tools
        .iter()
        .position(|tool| tool.kind == capability_kind);
    if !configuration.has_usable_targets_for_kind(capability_kind) {
        if let Some(index) = existing_index {
            ai_tools.remove(index);
        }
        return;
    }

    let capability = ResolvedAiToolCapability {
        kind: capability_kind,
        policy: AiToolPolicy::ForceHost,
        native_provider_tool: false,
        native_tool_type: None,
        native_request_patch: None,
        fallback_tool_id: Some(fallback_tool_id.to_string()),
        requires_approval: false,
        result_modalities: result_modalities
            .iter()
            .map(|value| value.to_string())
            .collect(),
        citation_support: false,
        source_label: String::from("用户配置"),
    };

    if let Some(index) = existing_index {
        ai_tools[index] = capability;
    } else {
        ai_tools.push(capability);
    }
}

/// Builds a host-only configured web-search capability used when a model lacks native search support.
/// 构建模型缺少原生搜索支持时使用的宿主侧配置化网页搜索能力。
fn default_configured_web_search_capability() -> ResolvedAiToolCapability {
    ResolvedAiToolCapability {
        kind: AiToolKind::WebSearch,
        policy: AiToolPolicy::ForceHost,
        native_provider_tool: false,
        native_tool_type: None,
        native_request_patch: None,
        fallback_tool_id: Some(WEB_SEARCH_PROVIDER_CHAIN_TOOL_ID.to_string()),
        requires_approval: false,
        result_modalities: vec![String::from("text")],
        citation_support: true,
        source_label: String::from("用户配置"),
    }
}

/// Builds a host-only configured image-understanding capability used when a model lacks native image input support.
/// 构建模型缺少原生图片输入支持时使用的宿主侧配置化图片理解能力。
fn default_configured_understand_image_capability() -> ResolvedAiToolCapability {
    ResolvedAiToolCapability {
        kind: AiToolKind::UnderstandImage,
        policy: AiToolPolicy::ForceHost,
        native_provider_tool: false,
        native_tool_type: None,
        native_request_patch: None,
        fallback_tool_id: Some(UNDERSTAND_IMAGE_PROVIDER_CHAIN_TOOL_ID.to_string()),
        requires_approval: false,
        result_modalities: vec![String::from("text")],
        citation_support: false,
        source_label: String::from("用户配置"),
    }
}

/// Resolves provider/model AI-tool declarations into one stable capability list.
/// 将供应商/模型 AI 工具声明解析为一组稳定能力列表。
fn resolve_ai_tool_capabilities(
    definition: &SupportedProviderDefinition,
    model: Option<&ModelChoice>,
) -> Vec<ResolvedAiToolCapability> {
    let mut declarations = inferred_provider_ai_tool_declarations(definition, model)
        .into_iter()
        .map(|declaration| (declaration.kind, declaration, String::from("接口默认")))
        .collect::<Vec<_>>();

    declarations.extend(
        definition
            .ai_tools
            .iter()
            .cloned()
            .map(|declaration| (declaration.kind, declaration, String::from("供应商声明"))),
    );

    if let Some(model) = model {
        declarations.extend(
            model
                .ai_tools
                .iter()
                .cloned()
                .map(|declaration| (declaration.kind, declaration, String::from("模型声明"))),
        );
    }

    let mut resolved = Vec::<ResolvedAiToolCapability>::new();
    for (kind, declaration, source_label) in declarations {
        let capability = resolved_ai_tool_capability_from_declaration(
            kind,
            declaration,
            source_label,
            definition.effective_api_family_for_model(model.map(|model| model.id.as_str())),
        );
        if let Some(index) = resolved.iter().position(|tool| tool.kind == kind) {
            resolved[index] = capability;
        } else {
            resolved.push(capability);
        }
    }

    resolved.sort_by_key(|tool| tool.kind);
    resolved
}

/// Infers conservative provider-native AI-tool defaults from the canonical API family.
/// 根据规范接口族推导保守的 Provider 原生 AI 工具默认值。
fn inferred_provider_ai_tool_declarations(
    definition: &SupportedProviderDefinition,
    model: Option<&ModelChoice>,
) -> Vec<AiToolDeclaration> {
    let model_accepts_text = model.is_none_or(|model| {
        model.output_modalities.is_empty()
            || model
                .output_modalities
                .iter()
                .any(|modality| modality.trim().eq_ignore_ascii_case("text"))
    });
    if !model_accepts_text {
        return Vec::new();
    }
    let model_accepts_tools = model.is_none_or(|model| model.tool_call.unwrap_or(true));
    if !model_accepts_tools {
        return Vec::new();
    }

    let api_family =
        definition.effective_api_family_for_model(model.map(|model| model.id.as_str()));

    match (definition.id.as_str(), api_family) {
        ("openai", ProviderApiFamily::OpenAiResponses) => vec![AiToolDeclaration::provider_native(
            AiToolKind::WebSearch,
            "web_search_preview",
        )],
        ("google", ProviderApiFamily::GoogleGeminiGenerateContent) => {
            vec![AiToolDeclaration::provider_native(
                AiToolKind::WebSearch,
                "google_search",
            )]
        }
        ("anthropic", ProviderApiFamily::AnthropicMessages) => {
            vec![AiToolDeclaration::provider_native(
                AiToolKind::WebSearch,
                "web_search_20250305",
            )]
        }
        (_, _) => Vec::new(),
    }
}

/// Converts one declaration into the runtime capability shape.
/// 将单条声明转换为运行时能力形状。
fn resolved_ai_tool_capability_from_declaration(
    kind: AiToolKind,
    declaration: AiToolDeclaration,
    source_label: String,
    api_family: ProviderApiFamily,
) -> ResolvedAiToolCapability {
    let native_tool_type = declaration
        .native_tool_type
        .filter(|value| !value.trim().is_empty())
        .or_else(|| default_native_ai_tool_type(api_family, kind).map(ToOwned::to_owned));
    let native_provider_tool = declaration.native_provider_tool.unwrap_or_else(|| {
        native_tool_type
            .as_ref()
            .is_some_and(|value| !value.trim().is_empty())
            || declaration.native_request_patch.is_some()
    });

    ResolvedAiToolCapability {
        kind,
        policy: declaration.policy,
        native_provider_tool,
        native_tool_type,
        native_request_patch: declaration.native_request_patch,
        fallback_tool_id: declaration
            .fallback_tool_id
            .filter(|value| !value.trim().is_empty()),
        requires_approval: declaration.requires_approval,
        result_modalities: normalize_modality_list(&declaration.result_modalities),
        citation_support: declaration.citation_support.unwrap_or(matches!(
            kind,
            AiToolKind::WebSearch | AiToolKind::FileSearch
        )),
        source_label,
    }
}

/// Returns the default provider-native tool type for one API family and semantic kind.
/// 返回指定接口族与语义类型对应的默认 Provider 原生工具类型。
fn default_native_ai_tool_type(
    api_family: ProviderApiFamily,
    kind: AiToolKind,
) -> Option<&'static str> {
    match (api_family, kind) {
        (ProviderApiFamily::OpenAiResponses, AiToolKind::WebSearch) => Some("web_search_preview"),
        (ProviderApiFamily::GoogleGeminiGenerateContent, AiToolKind::WebSearch) => {
            Some("google_search")
        }
        (ProviderApiFamily::AnthropicMessages, AiToolKind::WebSearch) => {
            Some("web_search_20250305")
        }
        _ => None,
    }
}

/// Resolves the effective API key by preferring the persisted secret field and then falling back to provider-declared environment keys.
/// 通过优先使用持久化密钥字段、再回退到供应商声明环境变量键的顺序解析生效 API Key。
pub fn resolve_provider_api_key(
    options: &PersistedProviderOptions,
    capabilities: &ResolvedModelCapabilities,
) -> Result<Option<String>, String> {
    resolve_provider_api_key_with_lookup(options, capabilities, |env_name| {
        resolve_named_environment_secret(env_name, "API Key")
    })
}

/// Builds one human-readable credential-source summary that includes provider-declared environment-key fallback details.
/// 构建一条包含供应商声明环境变量回退细节的人类可读凭据来源摘要。
pub fn resolve_credential_source_summary(
    options: &PersistedProviderOptions,
    capabilities: &ResolvedModelCapabilities,
) -> String {
    if let Some(raw_value) = options
        .api_key
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        return parse_env_placeholder_name(raw_value)
            .map(|env_name| format!("环境变量 `{env_name}`"))
            .unwrap_or_else(|| String::from("本地配置"));
    }

    if let Some(env_name) = first_available_environment_key(&capabilities.credential_env_keys) {
        return format!("环境变量 `{env_name}`");
    }

    if capabilities.credential_env_keys.is_empty() {
        String::from("未配置")
    } else {
        format!(
            "候选环境变量 {}",
            capabilities
                .credential_env_keys
                .iter()
                .map(|value| format!("`{value}`"))
                .collect::<Vec<_>>()
                .join("、")
        )
    }
}

/// Returns the first provider-declared environment key that currently resolves to a non-empty process value.
/// 返回当前能解析到非空进程值的第一条供应商声明环境变量键。
fn first_available_environment_key(env_names: &[String]) -> Option<&str> {
    env_names.iter().find_map(|env_name| {
        std::env::var(env_name)
            .ok()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .map(|_| env_name.as_str())
    })
}

/// Resolves one plain environment variable into a trimmed secret string while tolerating missing or empty variables as non-fatal fallback misses.
/// 将单个普通环境变量解析为去空白后的密钥字符串，并把缺失或空变量视为非致命回退未命中。
fn resolve_named_environment_secret(
    env_name: &str,
    field_label: &str,
) -> Result<Option<String>, String> {
    match std::env::var(env_name) {
        Ok(value) => {
            let resolved_value = value.trim();
            if resolved_value.is_empty() {
                Ok(None)
            } else {
                Ok(Some(resolved_value.to_string()))
            }
        }
        Err(std::env::VarError::NotPresent) => Ok(None),
        Err(std::env::VarError::NotUnicode(_)) => Err(format!(
            "{field_label} 的环境变量 `{env_name}` 不是合法 UTF-8，当前无法作为宿主凭据使用。"
        )),
    }
}

/// Resolves the effective API key with one caller-provided environment lookup so tests can verify fallback order without mutating global process state.
/// 通过调用方提供的环境变量查询函数解析生效 API Key，从而让测试无需修改全局进程状态也能验证回退顺序。
fn resolve_provider_api_key_with_lookup<F>(
    options: &PersistedProviderOptions,
    capabilities: &ResolvedModelCapabilities,
    mut lookup_env: F,
) -> Result<Option<String>, String>
where
    F: FnMut(&str) -> Result<Option<String>, String>,
{
    if let Some(api_key) = options.resolved_api_key()? {
        return Ok(Some(api_key));
    }

    for env_name in &capabilities.credential_env_keys {
        if let Some(api_key) = lookup_env(env_name)? {
            return Ok(Some(api_key));
        }
    }

    Ok(None)
}

/// Normalizes one ordered modality list by trimming values, lower-casing entries, and removing duplicates while preserving order.
/// 通过裁剪空白、统一转换为小写并按顺序去重来规范化一组有序模态列表。
fn normalize_modality_list(values: &[String]) -> Vec<String> {
    let mut seen = BTreeSet::new();
    let mut normalized = Vec::new();

    for value in values {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            continue;
        }
        let candidate = trimmed.to_ascii_lowercase();
        if seen.insert(candidate.clone()) {
            normalized.push(candidate);
        }
    }

    normalized
}

/// Normalizes one ordered environment-key list by trimming values and removing exact duplicates while preserving the declared case.
/// 通过裁剪空白并在保留声明大小写的前提下按顺序去除完全重复项来规范化一组有序环境变量键列表。
fn normalize_environment_key_list(values: &[String]) -> Vec<String> {
    let mut seen = BTreeSet::new();
    let mut normalized = Vec::new();

    for value in values {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            continue;
        }
        let candidate = trimmed.to_string();
        if seen.insert(candidate.clone()) {
            normalized.push(candidate);
        }
    }

    normalized
}

/// Returns one trimmed optional string clone when the source is not blank.
/// 当源值不为空白时返回一份经过裁剪的可选字符串副本。
fn trimmed_option(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(ToOwned::to_owned)
}

/// Parses one `{env:NAME}` placeholder into its referenced environment-variable name.
/// 将一个 `{env:NAME}` 占位符解析为它引用的环境变量名称。
fn parse_env_placeholder_name(value: &str) -> Option<&str> {
    let inner = value.strip_prefix("{env:")?.strip_suffix('}')?.trim();
    (!inner.is_empty()).then_some(inner)
}

#[cfg(test)]
mod tests {
    //! Capability-resolution regression tests covering credential fallback and model field normalization.
    //! 覆盖凭据回退与模型字段规范化的能力解析回归测试。

    use super::{
        ResolvedModelCapabilities, apply_ai_tool_runtime_configuration,
        resolve_credential_source_summary, resolve_effective_model_capabilities,
        resolve_model_capabilities, resolve_provider_api_key, resolve_provider_api_key_with_lookup,
    };
    use crate::provider_config::{
        AiToolKind, AiToolPolicy, ModelChoice, NativeBridgeRoutingMode,
        PersistedAiToolConfiguration, PersistedProviderOptions,
        PersistedUnderstandImageConfiguration, PersistedWebSearchConfiguration,
        ProviderBaseUrlConfig, ProviderBaseUrlKind, ProviderModelDiscoveryConfig,
        ProviderModelDiscoveryKind, ProviderRuntimeKind, SupportedProviderDefinition,
        UNDERSTAND_IMAGE_PROVIDER_CHAIN_TOOL_ID, WEB_SEARCH_PROVIDER_CHAIN_TOOL_ID,
        WebSearchRoutingMode,
    };
    use std::collections::BTreeMap;

    /// Builds one minimal provider definition for capability-resolution tests.
    /// 为能力解析测试构建一份最小 Provider 定义。
    fn test_provider_definition() -> SupportedProviderDefinition {
        SupportedProviderDefinition {
            id: String::from("deepseek"),
            name: String::from("DeepSeek"),
            runtime: ProviderRuntimeKind::OpenAiCompatible,
            user_agent: None,
            credential_env_keys: vec![String::from("DEEPSEEK_API_KEY")],
            extra_headers: BTreeMap::new(),
            extra_body: serde_json::Map::new(),
            model_runtime_overrides: BTreeMap::new(),
            reasoning_profile: None,
            ai_tools: Vec::new(),
            fields: Vec::new(),
            base_url: ProviderBaseUrlConfig {
                kind: ProviderBaseUrlKind::Fixed,
                allow_override: false,
                value: Some(String::from("https://api.example.test")),
                template: None,
                parameter: None,
            },
            model_discovery: ProviderModelDiscoveryConfig {
                kind: ProviderModelDiscoveryKind::OpenAiModels,
            },
            models: Vec::new(),
        }
    }

    /// Preserves declared model flags and normalizes modality strings into stable lowercase host values.
    /// 保留声明的模型能力标记，并把模态字符串规范化为稳定的小写宿主值。
    #[test]
    fn resolve_model_capabilities_preserves_flags_and_normalizes_modalities() {
        let definition = test_provider_definition();
        let model = ModelChoice {
            id: String::from("deepseek-v4-pro"),
            name: String::from("DeepSeek V4 Pro"),
            status: Some(String::from("stable")),
            family: Some(String::from("thinking")),
            release_date: Some(String::from("2026-04-24")),
            context_limit: Some(128_000),
            input_limit: Some(96_000),
            output_limit: Some(8_192),
            reasoning: Some(true),
            tool_call: Some(false),
            temperature: Some(false),
            attachment: Some(true),
            interleaved: None,
            reasoning_profile: None,
            ai_tools: Vec::new(),
            input_modalities: vec![
                String::from("Text"),
                String::from(" Image "),
                String::from("text"),
            ],
            output_modalities: vec![String::from("Text"), String::from("text")],
        };

        let capabilities = resolve_model_capabilities(&definition, Some(&model));

        assert!(capabilities.supports_reasoning);
        assert!(!capabilities.supports_tool_call);
        assert!(!capabilities.supports_temperature);
        assert!(capabilities.supports_attachment);
        assert_eq!(capabilities.context_limit, Some(128_000));
        assert_eq!(capabilities.input_modalities, vec!["text", "image"]);
        assert_eq!(capabilities.output_modalities, vec!["text"]);
        assert!(capabilities.supports_input_modality("text"));
        assert!(capabilities.supports_input_modality("image"));
        assert!(!capabilities.supports_input_modality("video"));
        assert!(
            capabilities
                .supports_attachment_kind(vulcan_core::types::ConversationAttachmentKind::Image)
        );
    }

    /// Treats `attachment` as catalog metadata while using input modalities as the host-side attachment gate.
    /// 将 `attachment` 视为目录元数据，同时使用输入模态作为宿主侧附件门控依据。
    #[test]
    fn resolve_model_capabilities_uses_modalities_as_attachment_gate() {
        let definition = test_provider_definition();
        let model = ModelChoice {
            id: String::from("qwen3.6-plus"),
            name: String::from("Qwen3.6 Plus"),
            status: None,
            family: None,
            release_date: None,
            context_limit: None,
            input_limit: None,
            output_limit: None,
            reasoning: Some(true),
            tool_call: Some(true),
            temperature: Some(true),
            attachment: Some(false),
            interleaved: None,
            reasoning_profile: None,
            ai_tools: Vec::new(),
            input_modalities: vec![
                String::from("text"),
                String::from("image"),
                String::from("video"),
            ],
            output_modalities: vec![String::from("text")],
        };

        let capabilities = resolve_model_capabilities(&definition, Some(&model));

        assert!(!capabilities.supports_attachment);
        assert!(
            capabilities
                .supports_attachment_kind(vulcan_core::types::ConversationAttachmentKind::Image)
        );
        assert!(
            capabilities
                .supports_attachment_kind(vulcan_core::types::ConversationAttachmentKind::Video)
        );
        assert!(
            !capabilities
                .supports_attachment_kind(vulcan_core::types::ConversationAttachmentKind::Audio)
        );
    }

    /// Falls back to provider-declared environment variables when no local API-key field is persisted.
    /// 当本地未持久化 API Key 字段时，回退到供应商声明的环境变量。
    #[test]
    fn resolve_provider_api_key_falls_back_to_provider_declared_env_key() {
        let definition = test_provider_definition();
        let capabilities = resolve_model_capabilities(&definition, None);
        let options = PersistedProviderOptions::default();
        let api_key = resolve_provider_api_key_with_lookup(&options, &capabilities, |env_name| {
            Ok((env_name == "DEEPSEEK_API_KEY").then(|| String::from("fallback-key")))
        })
        .expect("credential fallback");

        let public_api_key =
            resolve_provider_api_key(&options, &ResolvedModelCapabilities::default())
                .expect("public resolver stays non-fatal without fallback env");

        assert_eq!(public_api_key, None);
        assert_eq!(api_key.as_deref(), Some("fallback-key"));
    }

    /// Surfaces provider-declared fallback keys in the credential summary when no explicit local secret is configured.
    /// 当未配置显式本地密钥时，在凭据摘要中展示供应商声明的回退环境变量键。
    #[test]
    fn resolve_credential_source_summary_surfaces_fallback_env_keys() {
        let definition = test_provider_definition();
        let capabilities = resolve_model_capabilities(&definition, None);
        let options = PersistedProviderOptions::default();

        let summary = resolve_credential_source_summary(&options, &capabilities);

        assert!(summary.contains("DEEPSEEK_API_KEY"));
    }

    /// Infers provider-native web search for APIs that expose dedicated server-side search tools.
    /// 会为具备专用服务端搜索工具的接口族推导 Provider 原生网页搜索能力。
    #[test]
    fn resolve_model_capabilities_infers_native_web_search_for_openai_responses() {
        let mut definition = test_provider_definition();
        definition.id = String::from("openai");
        definition.runtime = ProviderRuntimeKind::OpenAi;

        let capabilities = resolve_model_capabilities(&definition, None);
        let web_search = capabilities
            .ai_tools
            .iter()
            .find(|tool| tool.kind == AiToolKind::WebSearch)
            .expect("OpenAI Responses should infer web search");

        assert!(web_search.native_provider_tool);
        assert_eq!(
            web_search.native_tool_type.as_deref(),
            Some("web_search_preview")
        );
    }

    /// Does not infer provider-native hosted tools for third-party compatible providers without explicit catalog declarations.
    /// 对未显式声明目录能力的三方兼容供应商，不会自动推导 Provider 原生 Hosted Tool。
    #[test]
    fn resolve_model_capabilities_does_not_infer_native_web_search_for_compatible_proxy() {
        let mut definition = test_provider_definition();
        definition.id = String::from("vivgrid");
        definition.runtime = ProviderRuntimeKind::OpenAi;

        let capabilities = resolve_model_capabilities(&definition, None);

        assert!(
            capabilities
                .ai_tools
                .iter()
                .all(|tool| tool.kind != AiToolKind::WebSearch)
        );
    }

    /// Configures host web-search fallback when the active model lacks provider-native search.
    /// 当激活模型缺少 Provider 原生搜索时，配置宿主侧网页搜索回退。
    #[test]
    fn ai_tool_configuration_adds_host_web_search_for_provider_chain() {
        let definition = test_provider_definition();
        let mut capabilities = resolve_model_capabilities(&definition, None);
        let configuration = PersistedAiToolConfiguration {
            vendors: vec![crate::provider_config::PersistedAiCapabilityVendorEntry {
                id: String::from("brave"),
                vendor_id: String::from("brave"),
                name: String::from("Brave Search"),
                enabled: true,
                api_key: String::from("token"),
                connection_mode: None,
                base_url: String::new(),
                region: String::new(),
                targets: vec![
                    crate::provider_config::PersistedAiCapabilityTargetBinding::new(
                        AiToolKind::WebSearch,
                        "brave",
                        "brave",
                    ),
                ],
            }],
            web_search: PersistedWebSearchConfiguration {
                mode: WebSearchRoutingMode::ThirdPartyOnly,
                target_order: vec![String::from("brave/brave")],
                dispatch_mode: crate::provider_config::WebSearchProviderDispatchMode::default(),
                max_results: 5,
                providers: Vec::new(),
            },
            ..PersistedAiToolConfiguration::default()
        };

        apply_ai_tool_runtime_configuration(&mut capabilities, &configuration);

        let web_search = capabilities
            .ai_tools
            .iter()
            .find(|tool| tool.kind == AiToolKind::WebSearch)
            .expect("configured provider chain should expose host web search");
        assert_eq!(web_search.policy, AiToolPolicy::ForceHost);
        assert_eq!(
            web_search.fallback_tool_id.as_deref(),
            Some(WEB_SEARCH_PROVIDER_CHAIN_TOOL_ID)
        );
        assert!(!web_search.native_provider_tool);
    }

    /// Keeps provider-native search as the fallback when provider-first mode has no usable external provider.
    /// 当外部供应商链不可用时，在外部优先模式下保留 Provider 原生搜索作为兜底。
    #[test]
    fn ai_tool_configuration_uses_native_fallback_when_provider_chain_is_missing() {
        let mut definition = test_provider_definition();
        definition.id = String::from("openai");
        definition.runtime = ProviderRuntimeKind::OpenAi;
        let mut capabilities = resolve_model_capabilities(&definition, None);
        let configuration = PersistedAiToolConfiguration {
            web_search: PersistedWebSearchConfiguration {
                mode: WebSearchRoutingMode::ThirdPartyFirst,
                target_order: Vec::new(),
                providers: Vec::new(),
                dispatch_mode: crate::provider_config::WebSearchProviderDispatchMode::default(),
                max_results: 5,
            },
            ..PersistedAiToolConfiguration::default()
        };

        apply_ai_tool_runtime_configuration(&mut capabilities, &configuration);

        let web_search = capabilities
            .ai_tools
            .iter()
            .find(|tool| tool.kind == AiToolKind::WebSearch)
            .expect("native web search should still be available");
        assert_eq!(web_search.policy, AiToolPolicy::ForceNative);
        assert!(web_search.fallback_tool_id.is_none());
    }

    /// Adds host image-understanding fallback when the selected model lacks native image input but the standalone capability layer is configured.
    /// 当所选模型缺少原生图片输入且已配置独立能力层时，添加宿主侧图片理解回退。
    #[test]
    fn ai_tool_configuration_adds_host_understand_image_for_provider_chain() {
        let definition = test_provider_definition();
        let mut capabilities = resolve_model_capabilities(&definition, None);
        let configuration = PersistedAiToolConfiguration {
            vendors: vec![crate::provider_config::PersistedAiCapabilityVendorEntry {
                id: String::from("minimax"),
                vendor_id: String::from("minimax"),
                name: String::from("MiniMax"),
                enabled: true,
                api_key: String::from("token"),
                connection_mode: None,
                base_url: String::new(),
                region: String::from("global"),
                targets: vec![
                    crate::provider_config::PersistedAiCapabilityTargetBinding::new(
                        AiToolKind::UnderstandImage,
                        "minimax",
                        "minimax",
                    ),
                ],
            }],
            understand_image: PersistedUnderstandImageConfiguration {
                mode: NativeBridgeRoutingMode::NativeFirst,
                target_order: vec![String::from("minimax/minimax")],
                providers: Vec::new(),
            },
            ..PersistedAiToolConfiguration::default()
        };

        apply_ai_tool_runtime_configuration(&mut capabilities, &configuration);

        let understand_image = capabilities
            .ai_tools
            .iter()
            .find(|tool| tool.kind == AiToolKind::UnderstandImage)
            .expect("configured provider chain should expose host image understanding");
        assert_eq!(understand_image.policy, AiToolPolicy::ForceHost);
        assert_eq!(
            understand_image.fallback_tool_id.as_deref(),
            Some(UNDERSTAND_IMAGE_PROVIDER_CHAIN_TOOL_ID)
        );
        assert!(!understand_image.native_provider_tool);
        assert!(
            capabilities.allows_staged_attachment_kind(
                vulcan_core::types::ConversationAttachmentKind::Image
            )
        );
    }

    /// Resolves one shared effective capability snapshot that already includes user-configured host AI-tool overlays.
    /// 解析一份已经包含用户配置宿主 AI 工具覆盖的共享生效能力快照。
    #[test]
    fn resolve_effective_model_capabilities_applies_host_ai_tool_overlays() {
        let definition = test_provider_definition();
        let configuration = PersistedAiToolConfiguration {
            vendors: vec![crate::provider_config::PersistedAiCapabilityVendorEntry {
                id: String::from("minimax"),
                vendor_id: String::from("minimax"),
                name: String::from("MiniMax"),
                enabled: true,
                api_key: String::from("token"),
                connection_mode: None,
                base_url: String::new(),
                region: String::from("global"),
                targets: vec![
                    crate::provider_config::PersistedAiCapabilityTargetBinding::new(
                        AiToolKind::UnderstandImage,
                        "minimax",
                        "minimax",
                    ),
                ],
            }],
            understand_image: PersistedUnderstandImageConfiguration {
                mode: NativeBridgeRoutingMode::NativeFirst,
                target_order: vec![String::from("minimax/minimax")],
                providers: Vec::new(),
            },
            ..PersistedAiToolConfiguration::default()
        };

        let capabilities =
            resolve_effective_model_capabilities(&definition, None, &configuration);

        assert!(
            capabilities
                .ai_tools
                .iter()
                .any(|tool| tool.kind == AiToolKind::UnderstandImage)
        );
        assert!(
            capabilities.allows_staged_attachment_kind(
                vulcan_core::types::ConversationAttachmentKind::Image
            )
        );
    }

    /// Removes host image-understanding fallback when the active model already supports native image input.
    /// 当当前模型已经支持原生图片输入时，移除宿主侧图片理解回退。
    #[test]
    fn ai_tool_configuration_omits_understand_image_for_native_vision_models() {
        let definition = test_provider_definition();
        let model = ModelChoice {
            id: String::from("vision-model"),
            name: String::from("Vision Model"),
            input_modalities: vec![String::from("text"), String::from("image")],
            output_modalities: vec![String::from("text")],
            status: None,
            family: None,
            release_date: None,
            context_limit: None,
            input_limit: None,
            output_limit: None,
            reasoning: Some(true),
            tool_call: Some(true),
            temperature: Some(true),
            attachment: Some(true),
            interleaved: None,
            reasoning_profile: None,
            ai_tools: Vec::new(),
        };
        let mut capabilities = resolve_model_capabilities(&definition, Some(&model));
        let configuration = PersistedAiToolConfiguration {
            vendors: vec![crate::provider_config::PersistedAiCapabilityVendorEntry {
                id: String::from("minimax"),
                vendor_id: String::from("minimax"),
                name: String::from("MiniMax"),
                enabled: true,
                api_key: String::from("token"),
                connection_mode: None,
                base_url: String::new(),
                region: String::from("global"),
                targets: vec![
                    crate::provider_config::PersistedAiCapabilityTargetBinding::new(
                        AiToolKind::UnderstandImage,
                        "minimax",
                        "minimax",
                    ),
                ],
            }],
            understand_image: PersistedUnderstandImageConfiguration {
                mode: NativeBridgeRoutingMode::NativeFirst,
                target_order: vec![String::from("minimax/minimax")],
                providers: Vec::new(),
            },
            ..PersistedAiToolConfiguration::default()
        };

        apply_ai_tool_runtime_configuration(&mut capabilities, &configuration);

        assert!(
            capabilities
                .ai_tools
                .iter()
                .all(|tool| tool.kind != AiToolKind::UnderstandImage)
        );
        assert!(
            capabilities
                .supports_attachment_kind(vulcan_core::types::ConversationAttachmentKind::Image)
        );
    }

    /// Adds host-only MiniMax generation capabilities and unlocks bridgeable image/audio attachments for the matching semantic tools.
    /// 添加宿主侧 MiniMax 生成类能力，并为对应语义工具放开可桥接的图片与音频附件。
    #[test]
    fn ai_tool_configuration_adds_host_generation_capabilities_for_configured_targets() {
        let definition = test_provider_definition();
        let mut capabilities = resolve_model_capabilities(&definition, None);
        let configuration = PersistedAiToolConfiguration {
            vendors: vec![crate::provider_config::PersistedAiCapabilityVendorEntry {
                id: String::from("minimax"),
                vendor_id: String::from("minimax"),
                name: String::from("MiniMax"),
                enabled: true,
                api_key: String::from("token"),
                connection_mode: None,
                base_url: String::new(),
                region: String::from("global"),
                targets: vec![
                    crate::provider_config::PersistedAiCapabilityTargetBinding::new(
                        AiToolKind::ImageGeneration,
                        "image-01",
                        "image-01",
                    ),
                    crate::provider_config::PersistedAiCapabilityTargetBinding::new(
                        AiToolKind::VideoGeneration,
                        "MiniMax-Hailuo-2.3",
                        "MiniMax-Hailuo-2.3",
                    ),
                    crate::provider_config::PersistedAiCapabilityTargetBinding::new(
                        AiToolKind::SpeechSynthesis,
                        "speech-2.8-hd",
                        "speech-2.8-hd",
                    ),
                    crate::provider_config::PersistedAiCapabilityTargetBinding::new(
                        AiToolKind::MusicGeneration,
                        "music-2.6",
                        "music-2.6",
                    ),
                    crate::provider_config::PersistedAiCapabilityTargetBinding::new(
                        AiToolKind::MusicCover,
                        "music-cover",
                        "music-cover",
                    ),
                ],
            }],
            ..PersistedAiToolConfiguration::default()
        };

        apply_ai_tool_runtime_configuration(&mut capabilities, &configuration);

        let image_generation = capabilities
            .ai_tools
            .iter()
            .find(|tool| tool.kind == AiToolKind::ImageGeneration)
            .expect("configured image generation should be exposed");
        assert_eq!(image_generation.policy, AiToolPolicy::ForceHost);
        assert_eq!(
            image_generation.fallback_tool_id.as_deref(),
            Some(crate::provider_config::IMAGE_GENERATION_PROVIDER_CHAIN_TOOL_ID)
        );

        let music_cover = capabilities
            .ai_tools
            .iter()
            .find(|tool| tool.kind == AiToolKind::MusicCover)
            .expect("configured music cover should be exposed");
        assert_eq!(music_cover.policy, AiToolPolicy::ForceHost);
        assert_eq!(
            music_cover.fallback_tool_id.as_deref(),
            Some(crate::provider_config::MUSIC_COVER_PROVIDER_CHAIN_TOOL_ID)
        );

        assert!(
            capabilities.allows_staged_attachment_kind(
                vulcan_core::types::ConversationAttachmentKind::Image
            )
        );
        assert!(
            capabilities.allows_staged_attachment_kind(
                vulcan_core::types::ConversationAttachmentKind::Audio
            )
        );
    }
}
