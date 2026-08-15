//! Field-level provenance tracking for resolved manifest outputs.
//! 已解析 Manifest 输出的字段级来源追踪。

use serde::{Deserialize, Serialize};

/// Identifies which manifest layer produced one resolved value.
/// 标识一个已解析值由哪一层清单产生。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FieldSourceKind {
    /// The value came from provider-level model defaults.
    /// 值来自供应商级模型默认。
    ProviderDefaults,
    /// The value came from one service-entry defaults overlay.
    /// 值来自单个服务入口默认覆盖。
    ServiceEntryOverride,
    /// The value came from one exact model rule.
    /// 值来自单条精确模型规则。
    ModelRuleExact,
    /// The value came from one longest-prefix model rule.
    /// 值来自单条最长前缀模型规则。
    ModelRulePrefix,
    /// The value came from the service-entry or protocol-route declaration itself.
    /// 值来自服务入口或协议路由声明本身。
    RouteDeclaration,
    /// The value came from a user-owned custom override outside the manifest.
    /// 值来自清单之外的用户自有覆盖。
    UserOverride,
}

/// Provenance attached to one resolved model protocol binding or capability layer.
/// 附加到单个已解析模型协议绑定或能力层的来源。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelBindingProvenance {
    /// Which manifest layer produced the binding.
    /// 绑定由哪一层清单产生。
    pub source: FieldSourceKind,
    /// Rule identifier when the binding came from a model rule.
    /// 绑定来自模型规则时的规则标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rule_id: Option<String>,
    /// Manifest revision that produced the binding.
    /// 产生绑定的清单修订号。
    pub manifest_revision: u64,
}

/// Provenance attached to one resolved model rule.
/// 附加到单个已解析模型规则的来源。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedRuleProvenance {
    /// Exact or longest-prefix source classification.
    /// 精确或最长前缀来源分类。
    pub source: FieldSourceKind,
    /// Exact rule identifier.
    /// 精确规则标识。
    pub rule_id: String,
    /// Manifest revision that produced the rule.
    /// 产生规则的清单修订号。
    pub manifest_revision: u64,
    /// Optional exact URL/URI of the top-level evidence source selected by this rule.
    /// 此规则选择的顶层证据来源精确 URL/URI，可选。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provenance_ref: Option<String>,
    /// Verification status inherited from the whole manifest.
    /// 从整份 Manifest 继承的验证状态。
    pub provenance_status: super::ProvenanceStatus,
}

/// Evidence provenance frozen for provider or service-entry model defaults.
/// 为供应商或服务入口模型默认值冻结的证据来源。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedModelDefaultsProvenance {
    /// Manifest revision that produced the defaults.
    /// 产生默认值的 Manifest 修订号。
    pub manifest_revision: u64,
    /// Optional exact URL/URI of the top-level evidence source selected by defaults.
    /// 默认层选择的顶层证据来源精确 URL/URI，可选。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provenance_ref: Option<String>,
    /// Verification status inherited from the whole manifest.
    /// 从整份 Manifest 继承的验证状态。
    pub provenance_status: super::ProvenanceStatus,
}

/// Whole-manifest provenance frozen into the compiled snapshot.
/// 冻结进编译快照的整份清单来源。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManifestSnapshotProvenance {
    /// Manifest source layer: bundled, active, or custom.
    /// 清单来源层：内置、激活或自定义。
    pub source_layer: ManifestSourceLayer,
    /// Manifest revision.
    /// 清单修订号。
    pub revision: u64,
    /// Optional content SHA-256 filled by the manifest store.
    /// 由 Manifest 存储填充的可选内容 SHA-256。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    /// Verification status declared by the manifest.
    /// 清单声明的验证状态。
    pub provenance_status: super::ProvenanceStatus,
}

/// Identifies the local manifest layer one snapshot was compiled from.
/// 标识一份快照编译自哪个本地清单层。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ManifestSourceLayer {
    /// Software-bundled baseline manifest.
    /// 软件内置基线清单。
    Bundled,
    /// Locally activated manifest (active or LKG).
    /// 本地已激活清单（active 或 LKG）。
    Active,
    /// User-owned custom provider manifest.
    /// 用户自有的自定义供应商清单。
    Custom,
    /// Manifest restored from the previous layer after an activation rollback.
    /// 激活回滚后从 previous 层恢复的清单。
    Previous,
}

/// Provenance of one compiled profile, operation, upload channel, or provider-tool target.
/// 单个已编译 Profile、operation、上传通道或供应商工具目标的来源。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManifestDeclarationProvenance {
    /// Exact provider owning the declaration.
    /// 持有该声明的精确供应商。
    pub provider_id: String,
    /// Stable declaration family.
    /// 稳定声明族。
    pub declaration_kind: String,
    /// Stable declaration identifier.
    /// 稳定声明标识。
    pub declaration_id: String,
    /// Manifest source layer.
    /// Manifest 来源层。
    pub source_layer: ManifestSourceLayer,
    /// Manifest revision.
    /// Manifest 修订号。
    pub manifest_revision: u64,
    /// Optional content-addressed source digest.
    /// 可选内容寻址源摘要。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
}
