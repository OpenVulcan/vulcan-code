//! Provider-neutral chronological system-update contracts.
//! Provider 中立的时间顺序系统更新契约。

use serde::{Deserialize, Serialize};

/// Trust classification for chronological system updates.
/// 时间顺序系统更新的信任分级。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeSystemUpdateTrust {
    /// Host-generated update that may be lowered to privileged provider system fields.
    /// 宿主生成且可降级到特权 Provider system 字段的更新。
    TrustedHost,
    /// User-authored update that must remain user-scoped unless explicitly promoted by policy.
    /// 用户提供且除非策略明确提升否则必须保持用户作用域的更新。
    UserScoped,
    /// External or tool-derived update that must never become privileged system content.
    /// 外部或工具派生且绝不能成为特权 system 内容的更新。
    UntrustedExternal,
}

impl Default for RuntimeSystemUpdateTrust {
    /// Returns the conservative default trust level.
    /// 返回保守的默认信任级别。
    fn default() -> Self {
        Self::UserScoped
    }
}

/// Lowering preference attached to one chronological system update.
/// 附着在单条时间顺序系统更新上的转换偏好。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeSystemUpdateLoweringHint {
    /// Lets the active provider capability select native support, a wrapper, or rejection.
    /// 允许当前 Provider 能力选择原生支持、包装或拒绝。
    Auto,
    /// Prefers provider-native chronological system support when available.
    /// Provider 可用时优先使用原生时间顺序 system 支持。
    PreferNative,
    /// Forces the update to remain a user-scoped wrapper.
    /// 强制该更新保持为用户作用域包装。
    UserWrapper,
    /// Rejects the request when the update cannot be represented natively.
    /// 更新无法原生表达时拒绝请求。
    RequireNative,
}

impl Default for RuntimeSystemUpdateLoweringHint {
    /// Returns the provider-capability-driven lowering preference.
    /// 返回由 Provider 能力驱动的默认转换偏好。
    fn default() -> Self {
        Self::Auto
    }
}

/// Provider-neutral chronological system update used by request projection.
/// 请求投影使用的 Provider 中立时间顺序系统更新。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeSystemUpdateEvent {
    /// Optional logical turn that owns this update.
    /// 拥有该更新的可选逻辑回合。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn_id: Option<String>,
    /// Stable local update identifier.
    /// 稳定的本地更新标识。
    pub update_id: String,
    /// Runtime position after which this update becomes effective.
    /// 该更新开始生效的运行时位置。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effective_after_event_id: Option<String>,
    /// Exact system-update text.
    /// 精确的系统更新正文。
    pub text: String,
    /// Trust level used by provider lowering.
    /// Provider 转换使用的信任级别。
    #[serde(default)]
    pub trust: RuntimeSystemUpdateTrust,
    /// Preferred lowering behavior for this update.
    /// 该更新的首选转换行为。
    #[serde(default)]
    pub lowering_hint: RuntimeSystemUpdateLoweringHint,
    /// Stable fingerprint of the text and source metadata.
    /// 正文与来源元数据的稳定指纹。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<String>,
    /// Human-readable source label used for audit output.
    /// 审计输出使用的人类可读来源标签。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
}

/// Provider-lowering repair, fallback, or rejection diagnostic.
/// Provider 转换修复、降级或拒绝诊断。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuntimeProviderLoweringRepairEvent {
    /// Optional logical turn identity.
    /// 可选的逻辑回合身份。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn_id: Option<String>,
    /// Stable repair identifier.
    /// 稳定的修复标识。
    pub repair_id: String,
    /// Provider targeted by lowering.
    /// 转换目标 Provider。
    pub provider_id: String,
    /// Model targeted by lowering.
    /// 转换目标模型。
    pub model_id: String,
    /// Provider protocol family used by lowering.
    /// 转换使用的 Provider 协议族。
    pub provider_family: String,
    /// Machine-readable repair action.
    /// 机器可读的修复动作。
    pub action: String,
    /// Human-readable repair reason.
    /// 人类可读的修复原因。
    pub reason: String,
    /// Number of affected request items.
    /// 受影响的请求项数量。
    #[serde(default)]
    pub affected_items: usize,
}
