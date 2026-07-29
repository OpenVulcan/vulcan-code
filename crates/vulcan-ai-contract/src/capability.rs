//! Provider-neutral capability declarations.
//! 供应商无关的能力声明。

use crate::{AiOperation, ModelParameterDefinition, PromptCacheCapability, ResourceKind};
use serde::{Deserialize, Serialize};

/// Identifies one provider-neutral reasoning-effort level selected by the user.
/// 标识用户选择的一种供应商无关推理强度。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReasoningEffort {
    /// Requests reasoning to be disabled when the selected route exposes that exact level.
    /// 当所选路由明确公开该档位时，请求关闭推理。
    None,
    /// Requests provider-managed adaptive reasoning without a fixed strength.
    /// 请求由供应商管理且没有固定强度的自适应推理。
    Adaptive,
    /// Requests the provider's minimal reasoning effort.
    /// 请求供应商的最小推理强度。
    Minimal,
    /// Requests a low reasoning effort.
    /// 请求低推理强度。
    Low,
    /// Requests a balanced reasoning effort.
    /// 请求平衡推理强度。
    Medium,
    /// Requests a high reasoning effort.
    /// 请求高推理强度。
    High,
    /// Requests an extra-high reasoning effort.
    /// 请求超高推理强度。
    XHigh,
    /// Requests the provider's maximum reasoning effort.
    /// 请求供应商的最大推理强度。
    Max,
    /// Requests a provider-specific ultra reasoning effort.
    /// 请求供应商特有的极限推理强度。
    Ultra,
}

impl ReasoningEffort {
    /// Returns the exact lower-case wire label shared by supported protocol projectors.
    /// 返回受支持协议投影器共享的精确小写 Wire 标签。
    ///
    /// # Returns
    /// 返回值
    /// The stable provider-neutral effort label.
    /// 稳定的供应商无关推理强度标签。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Adaptive => "adaptive",
            Self::Minimal => "minimal",
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::XHigh => "xhigh",
            Self::Max => "max",
            Self::Ultra => "ultra",
        }
    }
}

/// Declares how reasoning may be controlled for one exact model route.
/// 声明一条精确模型路由的推理控制方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ReasoningControlMode {
    /// Reasoning support has not been established.
    /// 尚未确定推理支持情况。
    #[default]
    Unknown,
    /// Reasoning is always disabled and the UI is read-only.
    /// 推理始终关闭且 UI 只读。
    FixedOff,
    /// Reasoning is always enabled and the UI is read-only.
    /// 推理始终开启且 UI 只读。
    FixedOn,
    /// Reasoning can be enabled or disabled without selectable strength levels.
    /// 推理可开关但没有可选强度档位。
    Toggle,
    /// Reasoning is enabled with one of the declared selectable effort levels.
    /// 推理启用时从已声明的可选强度档位中选择。
    EffortSelection,
    /// The provider controls adaptive reasoning for this exact route.
    /// 供应商为这条精确路由控制自适应推理。
    ProviderAdaptive,
}

/// Describes the fidelity of one supported capability.
/// 描述一项受支持能力的保真级别。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilitySupport {
    /// Provider metadata did not establish support or non-support.
    /// 供应商元数据未能确定支持或不支持。
    Unknown,
    /// Native provider support.
    /// 供应商原生支持。
    Native,
    /// Semantically equivalent projection.
    /// 语义等价投影。
    Equivalent,
    /// Supported with declared information loss.
    /// 以已声明的信息损失方式支持。
    Lossy,
    /// Unsupported.
    /// 不支持。
    Unsupported,
}

impl CapabilitySupport {
    /// Reports whether the capability may be enabled for execution.
    /// 报告该能力是否可以在执行中启用。
    ///
    /// # Returns
    /// 返回值
    /// `true` only for an explicitly supported fidelity level.
    /// 仅在显式支持的保真级别下返回 `true`。
    pub fn is_supported(self) -> bool {
        matches!(self, Self::Native | Self::Equivalent | Self::Lossy)
    }
}

/// Describes the resolved capabilities of one model route.
/// 描述一条模型路由解析后的能力。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelCapabilities {
    /// Supported operations.
    /// 支持的操作。
    pub operations: Vec<AiOperation>,
    /// Supported input resource kinds.
    /// 支持的输入资源类别。
    #[serde(default)]
    pub input_resources: Vec<ResourceKind>,
    /// Supported output resource kinds.
    /// 支持的输出资源类别。
    #[serde(default)]
    pub output_resources: Vec<ResourceKind>,
    /// Tool-calling support level.
    /// 工具调用支持级别。
    pub tool_calling: CapabilitySupport,
    /// Parallel tool-calling support level.
    /// 并行工具调用支持级别。
    pub parallel_tool_calling: CapabilitySupport,
    /// Reasoning-content support level.
    /// 推理内容支持级别。
    pub reasoning: CapabilitySupport,
    /// Exact reasoning control behavior.
    /// 精确推理控制行为。
    #[serde(default)]
    pub reasoning_control: ReasoningControlMode,
    /// Exact user-selectable reasoning-effort levels established by provider metadata.
    /// 由供应商元数据确定的用户可选推理强度列表。
    #[serde(default)]
    pub reasoning_efforts: Vec<ReasoningEffort>,
    /// Optional context limit after user narrowing.
    /// 用户收窄后的可选上下文限制。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_limit: Option<u64>,
    /// Optional output limit after user narrowing.
    /// 用户收窄后的可选输出限制。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_limit: Option<u64>,
    /// Prompt-cache capability.
    /// 提示词缓存能力。
    pub prompt_cache: PromptCacheCapability,
    /// Registered model-specific parameters.
    /// 已注册的模型独立参数。
    #[serde(default)]
    pub model_parameters: Vec<ModelParameterDefinition>,
    /// Evidence revision for this capability set.
    /// 此能力集合的证据修订号。
    pub evidence_revision: String,
}

impl Default for ModelCapabilities {
    /// Builds a conservative capability snapshot that enables no optional behavior.
    /// 构建不启用任何可选行为的保守能力快照。
    ///
    /// # Returns
    /// 返回值
    /// A snapshot whose limits and advanced-capability support are unknown.
    /// 返回限制与高级能力支持状态均未知的快照。
    fn default() -> Self {
        Self {
            operations: Vec::new(),
            input_resources: Vec::new(),
            output_resources: Vec::new(),
            tool_calling: CapabilitySupport::Unknown,
            parallel_tool_calling: CapabilitySupport::Unknown,
            reasoning: CapabilitySupport::Unknown,
            reasoning_control: ReasoningControlMode::Unknown,
            reasoning_efforts: Vec::new(),
            context_limit: None,
            output_limit: None,
            prompt_cache: PromptCacheCapability::default(),
            model_parameters: Vec::new(),
            evidence_revision: String::from("unverified"),
        }
    }
}
