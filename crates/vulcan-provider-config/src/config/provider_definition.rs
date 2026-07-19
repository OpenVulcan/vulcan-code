use super::*;

/// Base URL strategy used by one provider definition.
/// 单个供应商定义使用的基址策略。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProviderBaseUrlKind {
    /// The runtime resolves the endpoint internally.
    /// 由运行时内部解析接口地址。
    Native,
    /// A fixed default base URL is available.
    /// 存在固定的默认基址。
    Fixed,
    /// A base URL template is resolved from one form field.
    /// 基址模板需要从某个表单字段解析。
    Template,
}

/// Structured base URL configuration for one provider.
/// 单个供应商的结构化基址配置。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderBaseUrlConfig {
    /// Base URL resolution kind.
    /// 基址解析方式。
    pub kind: ProviderBaseUrlKind,
    /// Fixed default base URL when the provider has one.
    /// 当供应商存在固定默认基址时使用的默认值。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
    /// Template string used to resolve a provider-specific endpoint.
    /// 用于解析供应商特定接口地址的模板字符串。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub template: Option<String>,
    /// Field key used by the template strategy.
    /// 模板策略使用的字段键。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parameter: Option<ProviderFieldKey>,
    /// Whether the project chooses to expose a manual override input for this provider.
    /// 当前项目是否选择为该供应商暴露手动覆盖输入项。
    pub allow_override: bool,
}

/// Structured model discovery strategy for one provider.
/// 单个供应商的结构化模型发现策略。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProviderModelDiscoveryConfig {
    /// Discovery kind used by the provider page.
    /// 供应商页面使用的发现方式。
    pub kind: ProviderModelDiscoveryKind,
}

/// Structured provider-usage capture configuration loaded from one provider definition.
/// 从单个供应商定义中载入的结构化 Provider 用量捕获配置。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ProviderUsageCaptureConfig {
    /// Optional OpenAI-compatible streaming usage policy used by chat-completions style transports.
    /// 供 chat-completions 风格传输链路使用的可选 OpenAI 兼容流式 usage 策略。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub openai_compatible_streaming: Option<OpenAiCompatibleStreamingUsageConfig>,
}

impl ProviderUsageCaptureConfig {
    /// Returns whether the current usage-capture configuration contains no explicit provider rules.
    /// 返回当前用量捕获配置是否未声明任何显式供应商规则。
    pub const fn is_empty(&self) -> bool {
        self.openai_compatible_streaming.is_none()
    }
}

/// Structured OpenAI-compatible streaming usage policy loaded from one provider definition.
/// 从单个供应商定义中载入的结构化 OpenAI 兼容流式 usage 策略。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct OpenAiCompatibleStreamingUsageConfig {
    /// Whether the host should explicitly request the terminal usage chunk through `stream_options.include_usage`.
    /// 宿主是否应通过 `stream_options.include_usage` 显式请求终态 usage 块。
    #[serde(default)]
    pub request_include_usage: bool,
    /// Ordered terminal usage sources accepted by the current provider.
    /// 当前供应商可接受的终态 usage 来源优先级列表。
    #[serde(default)]
    pub response_sources: Vec<OpenAiCompatibleStreamingUsageSource>,
}

/// Supported terminal usage source variants for OpenAI-compatible chat-completions streaming providers.
/// OpenAI 兼容 chat-completions 流式供应商支持的终态 usage 来源变体。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum OpenAiCompatibleStreamingUsageSource {
    /// Uses one top-level `usage` payload carried by a terminal chunk that still includes choices.
    /// 使用仍包含 choices 的终态块上携带的顶层 `usage` 载荷。
    TopLevelTerminalChunk,
    /// Uses one top-level `usage` payload carried by a terminal chunk whose `choices` array is empty.
    /// 使用 `choices` 数组为空的终态块上携带的顶层 `usage` 载荷。
    TopLevelEmptyChoicesChunk,
    /// Uses one terminal `usage` payload nested inside a completed streamed choice.
    /// 使用嵌套在已结束流式 choice 内部的终态 `usage` 载荷。
    TerminalChoice,
}

/// Stable OpenRouter endpoint route selection persisted for one concrete model.
/// 为某个具体模型持久化保存的稳定 OpenRouter 端点路由选择。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpenRouterEndpointRouteSelection {
    /// Stable endpoint tag forwarded to the OpenRouter `provider.order` request field.
    /// 转发到 OpenRouter `provider.order` 请求字段中的稳定端点标签。
    pub tag: String,
    /// Human-readable provider label rendered in the workbench trigger and pending-change hints.
    /// 渲染在工作台触发器与待生效提示中的人类可读供应商标签。
    pub provider_name: String,
    /// Optional endpoint display name exposed by the endpoint catalog.
    /// 由端点目录暴露的可选端点显示名称。
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub endpoint_name: String,
}

impl OpenRouterEndpointRouteSelection {
    /// Returns the compact display label rendered by the workbench trigger.
    /// 返回由工作台触发器渲染的紧凑显示标签。
    pub fn display_label(&self) -> String {
        if !self.provider_name.trim().is_empty() {
            return self.provider_name.trim().to_string();
        }
        if !self.endpoint_name.trim().is_empty() {
            return self.endpoint_name.trim().to_string();
        }
        self.tag.trim().to_string()
    }

    /// Returns whether the persisted selection carries no usable endpoint identity.
    /// 返回该持久化选择是否不包含可用的端点身份信息。
    pub fn is_empty(&self) -> bool {
        self.tag.trim().is_empty()
    }
}

/// Supported model discovery kinds used by the current WebUI implementation.
/// 当前 WebUI 实现支持的模型发现方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProviderModelDiscoveryKind {
    /// No remote discovery is attempted and the embedded catalog is used directly.
    /// 不尝试远端发现，直接使用内置目录。
    #[serde(rename = "none")]
    None,
    /// Uses a standard OpenAI-style `/models` endpoint.
    /// 使用标准 OpenAI 风格 `/models` 端点。
    #[serde(rename = "openai-models")]
    OpenAiModels,
}

impl ProviderModelDiscoveryKind {
    /// Returns whether the discovery strategy can actively refresh models from a remote endpoint.
    /// 返回该发现策略是否可以主动从远端端点刷新模型。
    pub const fn supports_remote_refresh(self) -> bool {
        matches!(self, Self::OpenAiModels)
    }
}

/// One selectable model item shown in the provider configuration page.
/// 展示在供应商配置页面中的单个可选模型项。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ModelChoice {
    /// Stable model identifier persisted into the configuration file.
    /// 持久化到配置文件中的稳定模型标识。
    pub id: String,
    /// Human-readable model name rendered in the UI.
    /// 渲染在界面中的可读模型名称。
    pub name: String,
    /// Optional model lifecycle status.
    /// 可选的模型生命周期状态。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    /// Optional model family label.
    /// 可选的模型家族标签。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub family: Option<String>,
    /// Optional model release date string.
    /// 可选的模型发布日期字符串。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub release_date: Option<String>,
    /// Optional context window limit.
    /// 可选的上下文窗口上限。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub context_limit: Option<usize>,
    /// Optional input token limit.
    /// 可选的输入 Token 上限。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_limit: Option<usize>,
    /// Optional output token limit.
    /// 可选的输出 Token 上限。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_limit: Option<usize>,
    /// Optional reasoning capability flag.
    /// 可选的推理能力标记。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning: Option<bool>,
    /// Optional tool-calling capability flag.
    /// 可选的工具调用能力标记。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call: Option<bool>,
    /// Optional temperature support flag.
    /// 可选的温度参数支持标记。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<bool>,
    /// Optional attachment support flag.
    /// 可选的附件支持标记。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attachment: Option<bool>,
    /// Optional interleaved reasoning descriptor.
    /// 可选的交错推理描述。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub interleaved: Option<ModelInterleavedCapability>,
    /// Optional model-scoped reasoning-profile overrides that can replace provider defaults for this exact model.
    /// 可选的模型级推理档位覆盖配置，可为当前精确模型替换供应商默认策略。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_profile: Option<ReasoningProfileConfig>,
    /// Optional AI-specialized tool declarations attached to this exact model.
    /// 附着在当前精确模型上的可选 AI 专用工具声明集合。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ai_tools: Vec<AiToolDeclaration>,
    /// Optional input modality list.
    /// 可选的输入模态列表。
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub input_modalities: Vec<String>,
    /// Optional output modality list.
    /// 可选的输出模态列表。
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub output_modalities: Vec<String>,
    /// Whether the model should be checked by default the first time one provider instance is configured.
    /// 当前供应商实例首次配置时，该模型是否应默认被勾选。
    #[serde(default, skip_serializing_if = "is_false")]
    pub default_enabled: bool,
}

impl fmt::Display for ModelChoice {
    /// Renders a compact model label that keeps name and identifier inspectable.
    /// 渲染兼顾名称与标识可辨识度的紧凑模型标签。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.name == self.id {
            formatter.write_str(&self.name)
        } else {
            write!(formatter, "{} ({})", self.name, self.id)
        }
    }
}

/// Structured interleaved reasoning capability for one model entry.
/// 单个模型条目的结构化交错推理能力描述。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ModelInterleavedCapability {
    /// Boolean flag style interleaved capability.
    /// 布尔标记风格的交错推理能力。
    Enabled(bool),
    /// Field-mapped interleaved capability.
    /// 基于字段映射的交错推理能力。
    Field(ModelInterleavedField),
}

/// Field descriptor used by structured interleaved reasoning capabilities.
/// 结构化交错推理能力使用的字段描述。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelInterleavedField {
    /// Response field carrying interleaved reasoning payloads.
    /// 承载交错推理载荷的响应字段。
    pub field: String,
}

/// One supported provider definition loaded from the split resource catalog.
/// 从分离资源目录中载入的单个受支持供应商定义。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SupportedProviderDefinition {
    /// Stable provider identifier used in config and runtime routing.
    /// 用于配置与运行时路由的稳定供应商标识。
    pub id: String,
    /// Display name shown to the user.
    /// 展示给用户的显示名称。
    pub name: String,
    /// Provider runtime family aligned with the project integration model.
    /// 与项目接入模型对齐的供应商运行时族。
    pub runtime: ProviderRuntimeKind,
    /// Explicit user-agent string used when the provider needs one fixed execution profile.
    /// 当供应商需要固定执行标识时使用的显式 User-Agent 字符串。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_agent: Option<String>,
    /// Environment keys that may provide credentials for the provider.
    /// 可能为该供应商提供凭据的环境变量键集合。
    #[serde(default)]
    pub credential_env_keys: Vec<String>,
    /// Additional HTTP headers appended to real execution requests for the provider.
    /// 附加到该供应商真实执行请求中的额外 HTTP 头集合。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra_headers: BTreeMap<String, String>,
    /// Additional JSON body parameters merged into provider requests by compatible runtimes.
    /// 由兼容运行时合并进供应商请求体中的附加 JSON Body 参数。
    #[serde(default, skip_serializing_if = "serde_json::Map::is_empty")]
    pub extra_body: serde_json::Map<String, serde_json::Value>,
    /// Optional provider-specific usage capture rules reused by runtime request construction and terminal usage extraction.
    /// 供运行时请求构造与终态 usage 提取复用的可选供应商专属用量捕获规则。
    #[serde(default, skip_serializing_if = "ProviderUsageCaptureConfig::is_empty")]
    pub usage_capture: ProviderUsageCaptureConfig,
    /// Optional per-model runtime overrides for providers whose individual models use different compatible protocols.
    /// 当同一供应商下不同模型使用不同兼容协议时采用的可选模型级运行时覆盖。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub model_runtime_overrides: BTreeMap<String, ProviderRuntimeKind>,
    /// Optional provider-scoped reasoning profile used when the current runtime family needs non-standard or provider-tuned controls.
    /// 当当前运行时家族需要非标准或供应商特调控制时使用的可选供应商级推理档位配置。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reasoning_profile: Option<ReasoningProfileConfig>,
    /// Optional provider-scoped AI-specialized tool declarations used as model defaults.
    /// 作为模型默认值使用的可选供应商级 AI 专用工具声明集合。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ai_tools: Vec<AiToolDeclaration>,
    /// Ordered field list exposed by the provider form.
    /// 供应商表单暴露的有序字段列表。
    #[serde(default)]
    pub fields: Vec<ProviderFieldKey>,
    /// Structured base URL resolution configuration.
    /// 结构化的基址解析配置。
    pub base_url: ProviderBaseUrlConfig,
    /// Structured model discovery configuration.
    /// 结构化的模型发现配置。
    pub model_discovery: ProviderModelDiscoveryConfig,
    /// Embedded fallback model catalog.
    /// 内置回退模型目录。
    #[serde(default)]
    pub models: Vec<ModelChoice>,
}

impl SupportedProviderDefinition {
    /// Returns the effective runtime for one concrete model under this provider.
    /// 返回当前供应商下某个具体模型的生效运行时。
    pub fn effective_runtime_for_model(&self, model_id: &str) -> ProviderRuntimeKind {
        self.model_runtime_overrides
            .get(model_id)
            .copied()
            .unwrap_or(self.runtime)
    }

    /// Returns the effective API family for one concrete model under this provider.
    /// 返回当前供应商下某个具体模型的生效 API 接口族。
    pub fn effective_api_family_for_model(&self, model_id: Option<&str>) -> ProviderApiFamily {
        model_id
            .map(|id| self.effective_runtime_for_model(id))
            .unwrap_or(self.runtime)
            .api_family()
    }

    /// Returns the effective ordered field list used by the configuration form.
    /// 返回配置表单使用的生效有序字段列表。
    pub fn input_fields(&self) -> Vec<ProviderFieldKey> {
        let mut fields = self.fields.clone();

        if self.base_url.allow_override && !fields.contains(&ProviderFieldKey::BaseUrl) {
            fields.push(ProviderFieldKey::BaseUrl);
        }

        fields
    }

    /// Returns the placeholder text used by one provider-specific field.
    /// 返回某个供应商专属字段使用的占位提示文本。
    pub fn field_placeholder(&self, key: ProviderFieldKey) -> String {
        match key {
            ProviderFieldKey::BaseUrl => self
                .base_url
                .value
                .clone()
                .or_else(|| {
                    self.base_url
                        .template
                        .clone()
                        .map(|template| template.replace(['{', '}'], ""))
                })
                .unwrap_or_else(|| ProviderFieldKey::BaseUrl.placeholder().to_string()),
            _ => key.placeholder().to_string(),
        }
    }

    /// Returns the form label used by one provider-specific field.
    /// 返回某个供应商专属字段使用的表单标签。
    pub const fn field_label(&self, key: ProviderFieldKey) -> &'static str {
        key.label()
    }

    /// Returns whether the provider currently supports a remote model refresh path.
    /// 返回该供应商当前是否支持远端模型刷新路径。
    pub const fn supports_remote_model_refresh(&self) -> bool {
        self.model_discovery.kind.supports_remote_refresh()
    }

    /// Resolves the current base URL using the persisted options and provider strategy.
    /// 使用持久化选项与供应商策略解析当前基址。
    pub fn resolved_base_url(&self, options: &PersistedProviderOptions) -> Option<String> {
        if let Some(override_base_url) = options
            .base_url
            .as_ref()
            .map(|value| value.trim())
            .filter(|value| !value.is_empty())
        {
            return Some(override_base_url.to_string());
        }

        match self.base_url.kind {
            ProviderBaseUrlKind::Native => self.base_url.value.clone(),
            ProviderBaseUrlKind::Fixed => self.base_url.value.clone(),
            ProviderBaseUrlKind::Template => {
                let template = self.base_url.template.as_ref()?;
                let parameter = self.base_url.parameter?;
                let parameter_value = options.get(parameter).trim().to_string();

                if parameter_value.is_empty() {
                    return None;
                }

                let token = format!("{{{}}}", template_parameter_name(parameter));
                Some(template.replace(&token, &parameter_value))
            }
        }
    }

    /// Resolves the dynamic reasoning selector view for the current provider/model pair and one remembered preferred effort.
    /// 为当前供应商/模型组合与一份已记忆偏好档位解析动态思考选择器视图。
    pub fn reasoning_selection_view(
        &self,
        model: Option<&ModelChoice>,
        preferred_effort: ReasoningEffortLevel,
    ) -> ReasoningSelectionView {
        let active_profile = model
            .and_then(|item| item.reasoning_profile.as_ref())
            .or(self.reasoning_profile.as_ref());
        let supported_efforts = self.supported_reasoning_efforts(model);
        // selected_effort preserves an explicitly supported user choice and otherwise honors the provider-declared default before generic nearest-tier reconciliation.
        // selected_effort 保留用户明确选择且受支持的档位，否则优先采用供应商声明的默认值，再执行通用最近档位回退。
        let selected_effort = if supported_efforts.contains(&preferred_effort) {
            preferred_effort
        } else {
            active_profile
                .and_then(|profile| profile.default_effort)
                .filter(|effort| supported_efforts.contains(effort))
                .unwrap_or_else(|| reconcile_reasoning_effort(preferred_effort, &supported_efforts))
        };
        let options = build_reasoning_selection_options(active_profile, &supported_efforts);
        let note =
            build_reasoning_selection_note(model, preferred_effort, selected_effort, &options);

        ReasoningSelectionView {
            options,
            selected_effort,
            note,
        }
    }

    /// Resolves the ordered reasoning-effort list supported by the current provider/model pair.
    /// 解析当前供应商/模型组合所支持的有序思考档位列表。
    pub fn supported_reasoning_efforts(
        &self,
        model: Option<&ModelChoice>,
    ) -> Vec<ReasoningEffortLevel> {
        if let Some(profile) = model
            .and_then(|item| item.reasoning_profile.as_ref())
            .or(self.reasoning_profile.as_ref())
        {
            let profile_efforts = profile.available_efforts();
            if !profile_efforts.is_empty() {
                return profile_efforts;
            }
        }

        let model_supports_reasoning = model.and_then(|item| item.reasoning).unwrap_or(false);
        if !model_supports_reasoning {
            return vec![ReasoningEffortLevel::Off];
        }

        infer_runtime_reasoning_efforts(self.runtime, self.id.as_str(), model)
    }
}
