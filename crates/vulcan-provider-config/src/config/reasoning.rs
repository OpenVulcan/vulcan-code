use super::*;

/// Unified reasoning-effort preset shared by the workbench, provider configuration, and request adaptation pipeline.
/// 由工作台、供应商配置与请求适配链路共享的统一推理档位预设。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ReasoningEffortLevel {
    /// Requests the closest available no-thinking mode supported by the target provider or model.
    /// 请求目标供应商或模型支持的最接近“关闭思考”的模式。
    Off,
    /// Requests a lightweight reasoning profile focused on faster responses.
    /// 请求偏向更快响应的轻量推理配置。
    Low,
    /// Requests a balanced reasoning profile.
    /// 请求平衡型推理配置。
    Medium,
    /// Requests a deeper reasoning profile.
    /// 请求更深层的推理配置。
    High,
    /// Requests the strongest reasoning profile exposed by the current workbench controls.
    /// 请求当前工作台控件暴露的最强推理配置。
    XHigh,
}

impl ReasoningEffortLevel {
    /// Shared static list of all reasoning-effort presets exposed by the workbench.
    /// 工作台暴露的全部推理档位预设静态列表。
    pub const ALL: [Self; 5] = [Self::Off, Self::Low, Self::Medium, Self::High, Self::XHigh];

    /// Returns whether the current preset explicitly requests more than the provider default behavior.
    /// 返回当前预设是否明确请求了高于供应商默认行为的思考模式。
    pub const fn requests_explicit_reasoning(self) -> bool {
        !matches!(self, Self::Off)
    }

    /// Returns the stable ranking used by capability fallback selection and nearest-level matching.
    /// 返回供能力回退选择与最近档位匹配使用的稳定排序值。
    pub const fn rank(self) -> u8 {
        match self {
            Self::Off => 0,
            Self::Low => 1,
            Self::Medium => 2,
            Self::High => 3,
            Self::XHigh => 4,
        }
    }

    /// Returns the default compact Chinese label used by tiered reasoning selectors and diagnostics.
    /// 返回供分档思考选择器与诊断使用的默认紧凑中文标签。
    pub const fn compact_label(self) -> &'static str {
        match self {
            Self::Off => "关闭",
            Self::Low => "低",
            Self::Medium => "中",
            Self::High => "高",
            Self::XHigh => "超高",
        }
    }
}

impl fmt::Display for ReasoningEffortLevel {
    /// Renders the compact user-facing Chinese label for the current reasoning-effort preset.
    /// 渲染当前推理档位预设面向用户的紧凑中文标签。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.compact_label())
    }
}

/// One reasoning selector option rendered in the workbench according to the active model capability set.
/// 根据当前激活模型能力集合渲染在工作台中的单个思考选择项。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReasoningSelectionOption {
    /// Stable shared reasoning-effort value used by the runtime selection pipeline.
    /// 供运行时选择链路使用的稳定共享推理档位值。
    pub effort: ReasoningEffortLevel,
    /// Human-readable label shown in the compact selector UI.
    /// 展示在紧凑选择器界面中的人类可读标签。
    pub label: String,
}

impl fmt::Display for ReasoningSelectionOption {
    /// Renders the compact user-facing label for one reasoning selector option.
    /// 渲染单个思考选择器选项的紧凑面向用户标签。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.label)
    }
}

/// Resolved reasoning selector view derived from one provider/model capability set and one remembered preferred effort.
/// 根据某个供应商/模型能力集合与一份已记忆偏好档位解析出的思考选择器视图。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReasoningSelectionView {
    /// Dynamic selector options that should be visible for the active model.
    /// 当前激活模型应当可见的动态选择项集合。
    pub options: Vec<ReasoningSelectionOption>,
    /// Effective effort that should be shown as selected and sent to the runtime.
    /// 应当显示为已选中并发送给运行时的生效档位。
    pub selected_effort: ReasoningEffortLevel,
    /// Optional warning or downgrade note that explains why the preferred effort had to change.
    /// 用于解释偏好档位为何必须变化的可选警告或降级说明。
    pub note: Option<String>,
}

/// Structured capability family describing how one provider or model expects reasoning controls to be expressed.
/// 描述单个供应商或模型如何表达推理控制参数的结构化能力类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ReasoningCapabilityKind {
    /// Uses the provider/model default reasoning behavior without exposing any manual selector.
    /// 使用供应商或模型默认思考行为，不暴露任何手动选择器。
    DefaultOnly,
    /// Native multi-level reasoning controls.
    /// 原生多档位推理控制。
    Tiered,
    /// Binary on/off reasoning controls.
    /// 二值开关式推理控制。
    Binary,
    /// Binary reasoning controls that may also expose token-budget style tuning.
    /// 可能同时暴露 Token 预算调节的二值推理控制。
    BinaryWithBudget,
    /// Proxy-level tiered controls whose downstream effect may collapse into fewer effective levels.
    /// 代理层支持分档但下游实际效果可能塌缩的推理控制。
    ProxyTiered,
}

/// Structured JSON patch set that maps the shared reasoning-effort presets onto one provider- or model-specific request payload.
/// 将共享推理档位预设映射到某个供应商或模型专属请求载荷的结构化 JSON Patch 集合。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ReasoningProfileConfig {
    /// Optional capability-family descriptor used for diagnostics and future UI hints.
    /// 用于诊断和未来界面提示的可选能力类型描述。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kind: Option<ReasoningCapabilityKind>,
    /// Optional fallback effort used when the remembered workbench choice is unsupported by this profile.
    /// 当工作台记忆的档位不受当前配置支持时使用的可选回退档位。
    #[serde(rename = "defaultEffort", skip_serializing_if = "Option::is_none")]
    pub default_effort: Option<ReasoningEffortLevel>,
    /// Optional provider-specific payload used when the workbench explicitly requests reasoning-off behavior.
    /// 当工作台显式请求关闭思考时使用的可选供应商专属载荷。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub off: Option<Value>,
    /// Optional provider-specific payload used for the low reasoning preset.
    /// 用于低档推理预设的可选供应商专属载荷。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub low: Option<Value>,
    /// Optional provider-specific payload used for the medium reasoning preset.
    /// 用于中档推理预设的可选供应商专属载荷。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub medium: Option<Value>,
    /// Optional provider-specific payload used for the high reasoning preset.
    /// 用于高档推理预设的可选供应商专属载荷。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub high: Option<Value>,
    /// Optional provider-specific payload used for the strongest reasoning preset.
    /// 用于最高档推理预设的可选供应商专属载荷。
    #[serde(rename = "xHigh", skip_serializing_if = "Option::is_none")]
    pub x_high: Option<Value>,
    /// Optional user-facing label override used by the disabled/default reasoning option.
    /// 用于关闭或默认思考选项的可选界面标签覆盖。
    #[serde(rename = "offLabel", skip_serializing_if = "Option::is_none")]
    pub off_label: Option<String>,
    /// Optional user-facing label override used by the low reasoning option.
    /// 用于低档思考选项的可选界面标签覆盖。
    #[serde(rename = "lowLabel", skip_serializing_if = "Option::is_none")]
    pub low_label: Option<String>,
    /// Optional user-facing label override used by the medium reasoning option.
    /// 用于中档思考选项的可选界面标签覆盖。
    #[serde(rename = "mediumLabel", skip_serializing_if = "Option::is_none")]
    pub medium_label: Option<String>,
    /// Optional user-facing label override used by the high reasoning option.
    /// 用于高档思考选项的可选界面标签覆盖。
    #[serde(rename = "highLabel", skip_serializing_if = "Option::is_none")]
    pub high_label: Option<String>,
    /// Optional user-facing label override used by the strongest reasoning option.
    /// 用于最高档思考选项的可选界面标签覆盖。
    #[serde(rename = "xHighLabel", skip_serializing_if = "Option::is_none")]
    pub x_high_label: Option<String>,
}

impl ReasoningProfileConfig {
    /// Returns the provider- or model-specific JSON patch for one shared reasoning-effort preset.
    /// 返回某个共享推理档位预设对应的供应商或模型专属 JSON Patch。
    pub fn patch_for_effort(&self, effort: ReasoningEffortLevel) -> Option<Value> {
        if matches!(self.kind, Some(ReasoningCapabilityKind::DefaultOnly)) {
            return None;
        }

        if matches!(
            self.kind,
            Some(ReasoningCapabilityKind::Binary | ReasoningCapabilityKind::BinaryWithBudget)
        ) {
            return match effort {
                ReasoningEffortLevel::Off => self.off.clone(),
                _ => self.first_enabled_patch(),
            };
        }

        match effort {
            ReasoningEffortLevel::Off => self.off.clone(),
            ReasoningEffortLevel::Low => self.low.clone(),
            ReasoningEffortLevel::Medium => self.medium.clone(),
            ReasoningEffortLevel::High => self.high.clone(),
            ReasoningEffortLevel::XHigh => self.x_high.clone(),
        }
    }

    /// Returns the ordered visible effort list declared by this profile after taking binary collapsing rules into account.
    /// 在考虑二值折叠规则后，返回当前配置声明的有序可见档位列表。
    pub fn available_efforts(&self) -> Vec<ReasoningEffortLevel> {
        if matches!(self.kind, Some(ReasoningCapabilityKind::DefaultOnly)) {
            return vec![ReasoningEffortLevel::Off];
        }

        if matches!(
            self.kind,
            Some(ReasoningCapabilityKind::Binary | ReasoningCapabilityKind::BinaryWithBudget)
        ) {
            let mut efforts = Vec::new();
            if self.off.is_some() {
                efforts.push(ReasoningEffortLevel::Off);
            }
            if let Some(enabled_effort) = self.first_enabled_effort() {
                efforts.push(enabled_effort);
            }
            return efforts;
        }

        ReasoningEffortLevel::ALL
            .into_iter()
            .filter(|effort| self.patch_for_effort(*effort).is_some())
            .collect()
    }

    /// Returns the first available non-off patch used by binary-style profiles.
    /// 返回二值风格配置使用的第一条可用非关闭 Patch。
    fn first_enabled_patch(&self) -> Option<Value> {
        self.low
            .clone()
            .or_else(|| self.medium.clone())
            .or_else(|| self.high.clone())
            .or_else(|| self.x_high.clone())
    }

    /// Returns the first available non-off effort used by binary-style profiles.
    /// 返回二值风格配置使用的第一条可用非关闭档位。
    fn first_enabled_effort(&self) -> Option<ReasoningEffortLevel> {
        [
            (ReasoningEffortLevel::Low, self.low.as_ref()),
            (ReasoningEffortLevel::Medium, self.medium.as_ref()),
            (ReasoningEffortLevel::High, self.high.as_ref()),
            (ReasoningEffortLevel::XHigh, self.x_high.as_ref()),
        ]
        .into_iter()
        .find_map(|(effort, patch)| patch.is_some().then_some(effort))
    }

    /// Returns one optional UI label override for the current shared reasoning effort.
    /// 返回当前共享思考档位对应的可选界面标签覆盖。
    pub fn label_for_effort(&self, effort: ReasoningEffortLevel) -> Option<&str> {
        match effort {
            ReasoningEffortLevel::Off => self.off_label.as_deref(),
            ReasoningEffortLevel::Low => self.low_label.as_deref(),
            ReasoningEffortLevel::Medium => self.medium_label.as_deref(),
            ReasoningEffortLevel::High => self.high_label.as_deref(),
            ReasoningEffortLevel::XHigh => self.x_high_label.as_deref(),
        }
    }
}

pub(super) fn infer_runtime_reasoning_efforts(
    runtime: ProviderRuntimeKind,
    provider_id: &str,
    model: Option<&ModelChoice>,
) -> Vec<ReasoningEffortLevel> {
    let Some(model) = model else {
        return ReasoningEffortLevel::ALL.into_iter().collect();
    };

    let model_id = model.id.to_ascii_lowercase();

    match runtime.api_family() {
        ProviderApiFamily::GoogleGeminiGenerateContent => {
            infer_google_gemini_reasoning_efforts(&model_id)
        }
        ProviderApiFamily::CohereChat => {
            vec![ReasoningEffortLevel::Off, ReasoningEffortLevel::Low]
        }
        ProviderApiFamily::OpenAiChatCompatible
            if runtime == ProviderRuntimeKind::OpenAiCompatible
                && matches!(
                    provider_id,
                    "alibaba"
                        | "alibaba-cn"
                        | "alibaba-coding-plan"
                        | "alibaba-coding-plan-cn"
                        | "bailing"
                ) =>
        {
            vec![ReasoningEffortLevel::Off, ReasoningEffortLevel::Low]
        }
        ProviderApiFamily::OpenAiResponses
        | ProviderApiFamily::OpenAiChatCompatible
        | ProviderApiFamily::AnthropicMessages
        | ProviderApiFamily::BedrockConverse => ReasoningEffortLevel::ALL.into_iter().collect(),
    }
}

/// Infers one Gemini-model-specific reasoning-effort list from the model identifier family when no explicit model profile is present.
/// 在不存在显式模型配置时，根据模型标识家族推导一份 Gemini 专属思考档位列表。
pub(super) fn infer_google_gemini_reasoning_efforts(model_id: &str) -> Vec<ReasoningEffortLevel> {
    if model_id.starts_with("gemini-3") {
        return vec![ReasoningEffortLevel::Low, ReasoningEffortLevel::High];
    }

    if model_id.starts_with("gemini-2.5-pro") {
        return vec![
            ReasoningEffortLevel::Low,
            ReasoningEffortLevel::Medium,
            ReasoningEffortLevel::High,
        ];
    }

    if model_id.starts_with("gemini-2.5-") {
        return vec![
            ReasoningEffortLevel::Off,
            ReasoningEffortLevel::Low,
            ReasoningEffortLevel::Medium,
            ReasoningEffortLevel::High,
        ];
    }

    vec![
        ReasoningEffortLevel::Off,
        ReasoningEffortLevel::Low,
        ReasoningEffortLevel::Medium,
        ReasoningEffortLevel::High,
    ]
}

/// Applies provider-specific reasoning-mode overrides onto one mutable model list after catalog loading or remote refresh merging.
/// 在目录加载或远端刷新合并后，将供应商专属思考模式覆盖应用到可变模型列表上。
pub(super) fn apply_provider_reasoning_model_overrides(
    provider_id: &str,
    models: &mut [ModelChoice],
) {
    for model in models {
        let Some(mode) = provider_reasoning_mode(provider_id, model.id.as_str()) else {
            continue;
        };

        model.reasoning = Some(true);
        model.reasoning_profile = Some(match mode {
            ProviderReasoningMode::DefaultOnly => default_only_reasoning_profile(),
            ProviderReasoningMode::BinaryToggle => binary_toggle_reasoning_profile(),
        });
    }
}

/// Returns the normalized provider-specific reasoning control mode for one provider/model pair when the catalog needs an explicit override.
/// 当目录需要显式覆盖时，返回某个供应商/模型组合归一化后的专属思考控制模式。
fn provider_reasoning_mode(provider_id: &str, model_id: &str) -> Option<ProviderReasoningMode> {
    if is_alibaba_family_provider(provider_id) {
        return Some(alibaba_family_reasoning_mode(model_id));
    }

    None
}

/// Returns whether one provider belongs to the four Alibaba chat-compatible interfaces that share the same reasoning-mode matrix.
/// 返回某个供应商是否属于共享同一思考模式矩阵的四条阿里 Chat-Compatible 接口。
pub(super) fn is_alibaba_family_provider(provider_id: &str) -> bool {
    matches!(
        provider_id,
        "alibaba" | "alibaba-cn" | "alibaba-coding-plan" | "alibaba-coding-plan-cn"
    )
}

/// Resolves the Alibaba-family reasoning control mode using the maintained model matrix while leaving all unmaintained models on provider defaults.
/// 使用维护中的模型矩阵解析阿里系思考控制模式，并让未维护模型继续沿用接口默认行为。
fn alibaba_family_reasoning_mode(model_id: &str) -> ProviderReasoningMode {
    let normalized = model_id.trim().to_ascii_lowercase();

    if alibaba_family_model_supports_binary_toggle(normalized.as_str()) {
        ProviderReasoningMode::BinaryToggle
    } else {
        ProviderReasoningMode::DefaultOnly
    }
}

/// Returns whether one Alibaba-family model currently belongs to the maintained "supports on/off toggle" matrix.
/// 返回某个阿里系模型当前是否属于已维护的“支持开启/关闭切换”矩阵。
pub(super) fn alibaba_family_model_supports_binary_toggle(model_id: &str) -> bool {
    if matches!(
        model_id,
        "qwen3.6-max-preview"
            | "qwen3.6-35b-a3b"
            | "qwen3.5-397b-a17b"
            | "qwen3.5-122b-a10b"
            | "qwen3.5-27b"
            | "qwen3.5-35b-a3b"
            | "qwen3-max"
            | "qwen3-max-preview"
            | "qwen-plus"
            | "qwen-plus-latest"
            | "qwen-flash"
            | "qwen-turbo"
            | "qwen-turbo-latest"
            | "qwen3-235b-a22b"
            | "qwen3-32b"
            | "qwen3-30b-a3b"
            | "qwen3-14b"
            | "qwen3-8b"
            | "qwen3-4b"
            | "qwen3-1.7b"
            | "qwen3-0.6b"
            | "deepseek-v4-pro"
            | "deepseek-v4-flash"
            | "deepseek-v3.2"
            | "deepseek-v3.2-exp"
            | "deepseek-v3.1"
            | "siliconflow/deepseek-v3.2"
            | "siliconflow/deepseek-v3.1-terminus"
            | "vanchin/deepseek-v3.2-think"
            | "vanchin/deepseek-v3.1-terminus"
            | "glm-5.1"
            | "glm-5"
            | "glm-4.7"
            | "glm-4.6"
            | "glm-4.5"
            | "glm-4.5-air"
            | "kimi-k2.6"
            | "kimi-k2.5"
            | "kimi/kimi-k2.6"
            | "kimi/kimi-k2.5"
    ) {
        return true;
    }

    matches_snapshot_series(
        model_id,
        &[
            "qwen3.6-plus",
            "qwen3.6-flash",
            "qwen3.5-plus",
            "qwen3.5-flash",
            "qwen3-max",
            "qwen-plus",
            "qwen-flash",
            "qwen-turbo",
        ],
    )
}

/// Returns whether the current model identifier belongs to one of the maintained snapshot-backed series prefixes.
/// 返回当前模型标识是否属于某个已维护的快照系列前缀。
pub(super) fn matches_snapshot_series(model_id: &str, prefixes: &[&str]) -> bool {
    prefixes.iter().any(|prefix| {
        model_id == *prefix
            || model_id
                .strip_prefix(prefix)
                .is_some_and(|suffix| suffix.starts_with('-'))
    })
}

/// Builds the unified "follow provider/model default" reasoning profile used when the host should expose a non-editable default entry.
/// 构建统一的“沿用供应商或模型默认行为”思考配置，用于宿主仅展示不可编辑的默认项。
pub(super) fn default_only_reasoning_profile() -> ReasoningProfileConfig {
    ReasoningProfileConfig {
        kind: Some(ReasoningCapabilityKind::DefaultOnly),
        ..ReasoningProfileConfig::default()
    }
}

/// Builds the unified Alibaba-style on/off reasoning profile used by models that explicitly support a host-controlled toggle.
/// 构建阿里系显式支持宿主开关控制模型使用的统一开启/关闭思考配置。
pub(super) fn binary_toggle_reasoning_profile() -> ReasoningProfileConfig {
    ReasoningProfileConfig {
        kind: Some(ReasoningCapabilityKind::Binary),
        off: Some(json!({ "enable_thinking": false })),
        low: Some(json!({ "enable_thinking": true })),
        ..ReasoningProfileConfig::default()
    }
}

/// Enumerates the normalized provider-specific reasoning control modes reused by catalog overlays.
/// 枚举目录覆盖复用的规范化供应商专属思考控制模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProviderReasoningMode {
    /// Keeps provider/model defaults and exposes a non-editable “默认” entry.
    /// 保持供应商或模型默认行为，并暴露不可编辑的“默认”项。
    DefaultOnly,
    /// Exposes one host-controlled on/off toggle that maps to provider-native patches.
    /// 暴露一个映射到供应商原生 Patch 的宿主开启/关闭开关。
    BinaryToggle,
}

/// Builds the visible reasoning selector options from the ordered supported effort list.
/// 根据有序支持档位列表构建可见的思考选择器选项。
pub(super) fn build_reasoning_selection_options(
    profile: Option<&ReasoningProfileConfig>,
    supported_efforts: &[ReasoningEffortLevel],
) -> Vec<ReasoningSelectionOption> {
    if matches!(
        profile.and_then(|item| item.kind),
        Some(ReasoningCapabilityKind::DefaultOnly)
    ) {
        return vec![ReasoningSelectionOption {
            effort: ReasoningEffortLevel::Off,
            label: String::from("默认"),
        }];
    }

    let uses_binary_labels = supported_efforts
        .iter()
        .filter(|effort| effort.requests_explicit_reasoning())
        .count()
        <= 1;

    supported_efforts
        .iter()
        .copied()
        .map(|effort| ReasoningSelectionOption {
            effort,
            label: profile
                .and_then(|item| item.label_for_effort(effort))
                .map(str::to_string)
                .unwrap_or_else(|| {
                    if uses_binary_labels && effort.requests_explicit_reasoning() {
                        String::from("开启")
                    } else {
                        effort.compact_label().to_string()
                    }
                }),
        })
        .collect()
}

/// Reconciles one remembered preferred effort into the closest supported effort for the current model capability set.
/// 将一份已记忆偏好档位回退为当前模型能力集合中最近可用的生效档位。
pub(super) fn reconcile_reasoning_effort(
    preferred_effort: ReasoningEffortLevel,
    supported_efforts: &[ReasoningEffortLevel],
) -> ReasoningEffortLevel {
    if supported_efforts.contains(&preferred_effort) {
        return preferred_effort;
    }

    let supported_non_off = supported_efforts
        .iter()
        .copied()
        .filter(|effort| effort.requests_explicit_reasoning())
        .collect::<Vec<_>>();

    if preferred_effort == ReasoningEffortLevel::Off {
        return supported_non_off
            .first()
            .copied()
            .or_else(|| supported_efforts.first().copied())
            .unwrap_or(ReasoningEffortLevel::Off);
    }

    if !supported_non_off.is_empty() {
        let preferred_rank = preferred_effort.rank();

        return supported_non_off
            .into_iter()
            .min_by_key(|effort| {
                (
                    effort.rank().abs_diff(preferred_rank),
                    effort.rank() > preferred_rank,
                    effort.rank(),
                )
            })
            .unwrap_or(ReasoningEffortLevel::Off);
    }

    supported_efforts
        .first()
        .copied()
        .unwrap_or(ReasoningEffortLevel::Off)
}

/// Builds one downgrade note when the remembered preferred effort is not supported by the newly selected model.
/// 当已记忆偏好档位不被新选模型支持时，构建一条降级提示说明。
pub(super) fn build_reasoning_selection_note(
    model: Option<&ModelChoice>,
    preferred_effort: ReasoningEffortLevel,
    selected_effort: ReasoningEffortLevel,
    options: &[ReasoningSelectionOption],
) -> Option<String> {
    if preferred_effort == selected_effort {
        return None;
    }

    let model_label = model.map(|item| item.name.as_str()).unwrap_or("当前模型");
    let selected_label = options
        .iter()
        .find(|option| option.effort == selected_effort)
        .map(|option| option.label.as_str())
        .unwrap_or_else(|| selected_effort.compact_label());

    if preferred_effort == ReasoningEffortLevel::Off {
        return Some(format!(
            "当前模型 `{model_label}` 不支持关闭思考，最低可用档位为 `{selected_label}`，已自动切换。"
        ));
    }

    if !selected_effort.requests_explicit_reasoning() {
        return Some(format!(
            "当前模型 `{model_label}` 不支持思考能力，已自动切换到关闭思考。"
        ));
    }

    Some(format!(
        "当前模型 `{model_label}` 不支持 `{preferred_effort}` 思考档位，已自动切换到 `{selected_label}`。"
    ))
}
