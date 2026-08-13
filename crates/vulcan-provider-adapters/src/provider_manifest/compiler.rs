//! Manifest compiler: validated manifest documents into immutable runtime snapshots.
//! Manifest 编译器：将已验证清单文档编译为不可变运行时快照。

use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::Path;

use serde::Deserialize;
use sha2::{Digest, Sha256};
use vulcan_ai_contract::InferenceUsageScope;

use crate::{
    AuthenticationProjection, CredentialDirectoryVisibility, CredentialProfile, ModelDiscoveryKind,
    ProtocolRoute, ProviderCategory, ProviderDescriptor, ProviderKind, ProviderPromotion,
    RequestProjectorKind, ServiceEntry, ServiceEntryKind,
};

use super::{
    CatalogMembershipPolicy, EnginePrimitiveRegistry, MAX_PROVIDER_IDENTIFIER_LEN,
    ManifestOwnership, ManifestSnapshotProvenance, ModelSourcePolicy, ProviderManifestV1,
    ResolvedRuleInstance, UnknownModelPolicy, resolve_rule_instances, validate_manifest,
};

/// Maximum byte length accepted for the packaged bundled-manifest integrity index.
/// 打包内置 Manifest 完整性索引允许的最大字节长度。
const BUNDLED_MANIFEST_INDEX_MAX_BYTES: u64 = 1024 * 1024;
/// Maximum byte length accepted for one packaged bundled provider manifest.
/// 单份打包内置供应商 Manifest 允许的最大字节长度。
const BUNDLED_MANIFEST_MAX_BYTES: u64 = 4 * 1024 * 1024;
/// Shared schema identifier used by both signed remote and packaged integrity indexes.
/// 签名远端 Index 与打包完整性 Index 共用的 Schema 标识。
const BUNDLED_MANIFEST_INDEX_SCHEMA_VERSION: &str = "vulcan.provider-index.v1";
/// Maximum provider entries accepted from one packaged integrity index.
/// 单份打包完整性 Index 允许的最大供应商条目数。
const BUNDLED_MANIFEST_INDEX_MAX_PROVIDERS: usize = 256;
/// Maximum filesystem entries inspected in the packaged manifest directory.
/// 打包 Manifest 目录中允许检查的文件系统条目最大数量。
const BUNDLED_MANIFEST_DIRECTORY_MAX_ENTRIES: usize = 512;
/// Maximum UTF-8 byte length retained for one provider-local bundled failure diagnostic.
/// 单条逐供应商内置失败诊断保留的最大 UTF-8 字节长度。
const BUNDLED_MANIFEST_FAILURE_MAX_BYTES: usize = 8_192;

/// Packaged integrity index binding the complete bundled provider-manifest file set.
/// 绑定完整内置供应商 Manifest 文件集合的打包完整性 Index。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct BundledManifestIndex {
    /// Frozen provider-index schema identifier.
    /// 冻结的供应商 Index Schema 标识。
    schema_version: String,
    /// Monotonic packaged index revision.
    /// 单调递增的打包 Index 修订号。
    index_revision: u64,
    /// Release time retained for parity with the remote index wire contract.
    /// 为与远端 Index 线协议保持一致而保留的发布时间。
    issued_at_unix_ms: i64,
    /// Non-expiring package horizon retained for schema parity.
    /// 为 Schema 一致性保留的不失效打包期限。
    expires_at_unix_ms: i64,
    /// Stable package-integrity authority identifier.
    /// 稳定的打包完整性权限标识。
    signing_key_id: String,
    /// Complete provider entries keyed by exact provider identity.
    /// 按精确供应商身份索引的完整供应商条目。
    providers: BTreeMap<String, BundledManifestIndexEntry>,
}

/// One packaged provider manifest pinned by the bundled integrity index.
/// 由内置完整性 Index 固定的一份打包供应商 Manifest。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct BundledManifestIndexEntry {
    /// Exact provider identity duplicated for key/value consistency checks.
    /// 为键值一致性检查而重复保存的精确供应商身份。
    provider_id: String,
    /// Exact manifest revision declared by the indexed bytes.
    /// 被索引字节声明的精确 Manifest 修订号。
    revision: u64,
    /// Exact resource-root-relative path for the provider bytes.
    /// 相对资源根的精确供应商字节路径。
    relative_url: String,
    /// Lowercase SHA-256 digest of the exact provider bytes.
    /// 精确供应商字节的小写 SHA-256 摘要。
    sha256: String,
    /// Exact provider document byte length.
    /// 供应商文档的精确字节长度。
    bytes: u64,
    /// Minimum manifest engine contract declared by the provider document.
    /// 供应商文档声明的最低 Manifest 引擎契约。
    minimum_engine_contract: u64,
}

/// One provider-aware load result from a globally trusted bundled index generation.
/// 一份来自全局可信内置 Index 代际的逐供应商加载结果。
#[derive(Debug)]
struct BundledManifestGenerationLoad {
    /// Provider manifests that passed every local integrity and compiler check.
    /// 通过全部本地完整性与编译校验的供应商 Manifest。
    manifests: BTreeMap<String, std::sync::Arc<ResolvedProviderManifest>>,
    /// Provider-local failures that remain isolated from unrelated identities.
    /// 与无关身份隔离的逐供应商失败。
    failures: BTreeMap<String, BundledProviderManifestFailure>,
}

/// Effective provider defaults frozen for one exact conversation service entry.
/// 为一个精确对话服务入口冻结的生效供应商默认值。
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedEntryModelDefaults {
    /// Protocol candidates authorized for unmatched models and inherited rules on this entry.
    /// 此入口上授权给未匹配模型并由规则继承的协议候选。
    pub supported_protocols: Vec<String>,
    /// Unique default protocol for unmatched models when the manifest permits default binding.
    /// Manifest 允许默认绑定时未匹配模型使用的唯一默认协议。
    pub selected_protocol: Option<String>,
    /// Complete compiled provider capability snapshot inherited by unmatched models.
    /// 未匹配模型继承的完整已编译供应商能力快照。
    pub capabilities: Option<vulcan_ai_contract::ModelCapabilities>,
    /// Usage extraction profile references inherited by unmatched models on this entry.
    /// 此入口上未匹配模型继承的用量提取 Profile 引用。
    pub usage_profile_refs: Vec<String>,
    /// Request projection profile references inherited by unmatched models on this entry.
    /// 此入口上未匹配模型继承的请求投影 Profile 引用。
    pub request_profile_refs: Vec<String>,
    /// Response and error projection profile references inherited by unmatched models on this entry.
    /// 此入口上未匹配模型继承的响应与错误投影 Profile 引用。
    pub response_profile_refs: Vec<String>,
    /// Operation references inherited by unmatched models on this entry.
    /// 此入口上未匹配模型继承的操作引用。
    pub operation_refs: Vec<String>,
    /// Evidence provenance frozen with these effective defaults.
    /// 与这些生效默认值一起冻结的证据来源。
    pub provenance: super::ResolvedModelDefaultsProvenance,
}

/// One immutable compiled provider manifest.
/// 一份不可变编译供应商清单。
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedProviderManifest {
    /// Compiled runtime provider descriptor.
    /// 编译后的运行时供应商描述。
    pub descriptor: ProviderDescriptor,
    /// Exact engine contract verified when this immutable snapshot was compiled.
    /// 编译此不可变快照时已验证的精确引擎契约。
    pub engine_contract: super::EngineContract,
    /// Complete evidence/source declaration verified for this manifest generation.
    /// 当前 Manifest 代际已验证的完整证据与来源声明。
    pub manifest_provenance: super::ManifestProvenance,
    /// Manifest revision this snapshot was compiled from.
    /// 编译此快照使用的清单修订号。
    pub revision: u64,
    /// Rule instances expanded per service entry with inherited effective fields.
    /// 按服务入口展开、携带继承有效字段的规则实例。
    pub rule_instances: Vec<ResolvedRuleInstance>,
    /// Authoritative manifest-backed model inventories keyed by service entry.
    /// 按服务入口索引的权威 Manifest 驱动模型清单。
    pub catalog_models: BTreeMap<String, Vec<super::ManifestCatalogModel>>,
    /// Exact model-identity source policy keyed by conversation service entry.
    /// 按对话服务入口索引的精确模型身份来源策略。
    pub model_source_policies: BTreeMap<String, ModelSourcePolicy>,
    /// Provider-level unknown-model policy.
    /// 供应商级未知模型策略。
    pub unknown_model_policy: UnknownModelPolicy,
    /// Provider-level catalog membership authority for declared and discovered identities.
    /// 供应商级声明身份与发现身份目录成员权限。
    pub catalog_membership_policy: CatalogMembershipPolicy,
    /// Provider-level default enabled state inherited by models without a rule override.
    /// 无规则覆盖模型继承的供应商级默认启用状态。
    pub default_enabled: bool,
    /// Effective model defaults keyed by exact conversation service entry.
    /// 按精确对话服务入口索引的生效模型默认值。
    pub entry_model_defaults: BTreeMap<String, ResolvedEntryModelDefaults>,
    /// Named usage-extraction profile declarations.
    /// 具名 usage 提取 Profile 声明。
    pub usage_profiles: Vec<super::UsageProfileDeclaration>,
    /// Named request-projection profile declarations.
    /// 具名请求投影 Profile 声明。
    pub request_profiles: Vec<super::RequestProfileDeclaration>,
    /// Named response/error profile declarations.
    /// 具名响应/错误 Profile 声明。
    pub response_profiles: Vec<super::ResponseProfileDeclaration>,
    /// Named upload channel declarations.
    /// 具名上传通道声明。
    pub upload_channels: Vec<super::UploadChannelDeclaration>,
    /// Named operation template declarations.
    /// 具名操作模板声明。
    pub operations: Vec<super::OperationDeclaration>,
    /// Compiled provider tool targets owned by this manifest.
    /// 由本清单拥有的已编译供应商工具目标。
    pub tool_targets: Vec<crate::ProviderToolTarget>,
    /// Provenance of every provider-tool target instantiated from a preauthorized template.
    /// 每个从预授权模板实例化的供应商工具目标来源。
    pub tool_target_template_instances: BTreeMap<String, ToolTargetTemplateInstanceProvenance>,
    /// Provenance for every compiled profile, operation, upload channel, and tool target.
    /// 每个已编译 Profile、operation、上传通道与工具目标的来源。
    pub declaration_provenance: BTreeMap<String, super::ManifestDeclarationProvenance>,
    /// Whole-manifest provenance.
    /// 整份清单来源。
    pub snapshot_provenance: ManifestSnapshotProvenance,
}

/// Provenance for one concrete target instantiated from immutable bundled authority.
/// 从不可变内置权限实例化的一个具体目标来源。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolTargetTemplateInstanceProvenance {
    /// Concrete stable target identifier.
    /// 具体稳定目标标识。
    pub target_id: String,
    /// Referenced immutable template identifier.
    /// 被引用的不可变模板标识。
    pub template_id: String,
    /// Service entry whose catalog supplied the model.
    /// 提供该模型目录的服务入口。
    pub service_entry_id: String,
    /// Exact upstream model identifier supplied by catalog data.
    /// 目录数据提供的精确上游模型标识。
    pub model_id: String,
    /// User-visible catalog model name.
    /// 用户可见目录模型名称。
    pub model_display_name: String,
}

/// One manifest clone with all authorized template instances expanded into concrete targets.
/// 一份已把全部授权模板实例展开为具体目标的 Manifest 克隆。
#[derive(Debug, Clone)]
pub(crate) struct ExpandedToolTargetManifest {
    /// Effective manifest consumed by ordinary closure validation and compilation.
    /// 由普通闭包校验与编译消费的有效 Manifest。
    pub manifest: ProviderManifestV1,
    /// Concrete instance provenance keyed by generated target identifier.
    /// 按生成目标标识索引的具体实例来源。
    pub instances: BTreeMap<String, ToolTargetTemplateInstanceProvenance>,
}

impl ResolvedProviderManifest {
    /// Returns the compiled descriptor for this manifest.
    /// 返回此清单的编译描述。
    pub fn provider(&self) -> &ProviderDescriptor {
        &self.descriptor
    }

    /// Returns the exact compiled model-source policy for one conversation service entry.
    /// 返回一个对话服务入口的精确编译模型来源策略。
    ///
    /// # Parameters
    /// 参数
    /// `service_entry_id` identifies the exact entry whose identity authority is requested.
    /// `service_entry_id` 标识要查询身份权威的精确入口。
    ///
    /// # Returns
    /// 返回值
    /// The closed source policy, or a fail-closed diagnostic when the entry has no declaration.
    /// 返回封闭来源策略；入口没有声明时返回失败关闭诊断。
    pub fn model_source_policy(&self, service_entry_id: &str) -> Result<ModelSourcePolicy, String> {
        self.model_source_policies
            .get(service_entry_id)
            .copied()
            .ok_or_else(|| {
                format!(
                    "Provider `{}` service entry `{service_entry_id}` has no compiled model source policy.",
                    self.descriptor.id
                )
            })
    }

    /// Returns the exact effective model defaults for one conversation service entry.
    /// 返回一个对话服务入口的精确生效模型默认值。
    ///
    /// # Parameters
    /// 参数
    /// `service_entry_id` identifies the entry whose inherited protocol and capability authority
    /// is required.
    /// `service_entry_id` 标识需要查询继承协议与能力权威的入口。
    ///
    /// # Returns
    /// 返回值
    /// The immutable compiled defaults, or a fail-closed diagnostic for an unknown entry.
    /// 返回不可变已编译默认值；入口未知时返回失败关闭诊断。
    pub fn model_defaults_for_entry(
        &self,
        service_entry_id: &str,
    ) -> Result<&ResolvedEntryModelDefaults, String> {
        self.entry_model_defaults
            .get(service_entry_id)
            .ok_or_else(|| {
                format!(
                    "Provider `{}` service entry `{service_entry_id}` has no compiled model defaults.",
                    self.descriptor.id
                )
            })
    }

    /// Reports whether one model identity is covered by the manifest inventory or a scoped rule.
    /// 报告一个模型身份是否被 Manifest 清单或作用域规则覆盖。
    ///
    /// # Parameters
    /// 参数
    /// `service_entry_id`, `credential_profile_id`, and `model_id` identify the exact catalog
    /// scope whose declaration authority is queried.
    /// `service_entry_id`、`credential_profile_id` 与 `model_id` 标识要查询声明权限的精确
    /// 目录作用域。
    ///
    /// # Returns
    /// 返回值
    /// `true` when an exact manifest catalog record or the winning Exact/Prefix rule declares the
    /// identity for this scope.
    /// 当精确 Manifest 目录记录或胜出的 Exact/Prefix 规则在当前作用域声明该身份时返回
    /// `true`。
    pub fn declares_model_identity(
        &self,
        service_entry_id: &str,
        credential_profile_id: Option<&str>,
        model_id: &str,
    ) -> bool {
        self.catalog_models
            .get(service_entry_id)
            .is_some_and(|models| models.iter().any(|model| model.model_id == model_id))
            || self
                .rule_for_model(service_entry_id, credential_profile_id, model_id)
                .is_some()
    }

    /// Resolves one operation's request-profile references into ordered frozen payloads.
    /// 将一个操作的请求 Profile 引用解析为有序冻结载荷。
    ///
    /// # Parameters
    /// 参数
    /// `operation` is the unique compiled owner of a provider-tool target; `None` represents a
    /// legacy target with no manifest operation.
    /// `operation` 是供应商工具目标的唯一编译拥有者；`None` 表示无清单操作的旧版目标。
    ///
    /// # Returns
    /// 返回值
    /// Ordered payloads from this manifest generation, or a fail-closed closure diagnostic.
    /// 返回当前清单代际的有序载荷，或失败关闭的闭合诊断。
    pub fn request_profiles_for_operation(
        &self,
        operation: Option<&super::OperationDeclaration>,
    ) -> Result<Vec<super::RequestProjectionProfile>, String> {
        let Some(operation) = operation else {
            return Ok(Vec::new());
        };
        operation
            .request_profile_refs
            .iter()
            .map(|profile_id| {
                self.request_profiles
                    .iter()
                    .find(|declaration| declaration.profile_id == *profile_id)
                    .and_then(|declaration| declaration.payload.clone())
                    .ok_or_else(|| {
                        format!(
                            "Operation `{}` request profile `{profile_id}` has no compiled payload.",
                            operation.operation_id
                        )
                    })
            })
            .collect()
    }

    /// Resolves one operation's response-profile references into ordered frozen payloads.
    /// 将一个操作的响应 Profile 引用解析为有序冻结载荷。
    ///
    /// # Parameters
    /// 参数
    /// `operation` is the unique compiled owner of a provider-tool target; `None` represents a
    /// legacy target with no manifest operation.
    /// `operation` 是供应商工具目标的唯一编译拥有者；`None` 表示无清单操作的旧版目标。
    ///
    /// # Returns
    /// 返回值
    /// Ordered payloads from this manifest generation, or a fail-closed closure diagnostic.
    /// 返回当前清单代际的有序载荷，或失败关闭的闭合诊断。
    pub fn response_profiles_for_operation(
        &self,
        operation: Option<&super::OperationDeclaration>,
    ) -> Result<Vec<super::ResponseProjectionProfile>, String> {
        let Some(operation) = operation else {
            return Ok(Vec::new());
        };
        operation
            .response_profile_refs
            .iter()
            .map(|profile_id| {
                self.response_profiles
                    .iter()
                    .find(|declaration| declaration.profile_id == *profile_id)
                    .and_then(|declaration| declaration.payload.clone())
                    .ok_or_else(|| {
                        format!(
                            "Operation `{}` response profile `{profile_id}` has no compiled payload.",
                            operation.operation_id
                        )
                    })
            })
            .collect()
    }

    /// Resolves the rule instance matching one model, service entry, and credential scope.
    /// 解析匹配一个模型、服务入口与凭据作用域的规则实例。
    ///
    /// # Parameters
    /// 参数
    /// `service_entry_id` is the exact entry; `credential_profile_id` is the exact credential
    /// profile of the connection (`None` only when the caller cannot identify one); `model_id`
    /// is the exact model identity.
    /// `service_entry_id` 是精确入口；`credential_profile_id` 是连接所属的精确凭据配置档
    /// （调用方无法识别时传 `None`）；`model_id` 是精确模型身份。
    ///
    /// # Returns
    /// 返回值
    /// The most specific matching instance, or `None` when no rule covers the model.
    /// 返回最具体的匹配实例；无规则覆盖模型时返回 `None`。
    pub fn rule_for_model<'a>(
        &'a self,
        service_entry_id: &str,
        credential_profile_id: Option<&str>,
        model_id: &str,
    ) -> Option<&'a ResolvedRuleInstance> {
        let mut best: Option<&ResolvedRuleInstance> = None;
        for instance in &self.rule_instances {
            if instance.service_entry_id != service_entry_id {
                continue;
            }
            // A credential-scoped rule only covers the exact profile; a global rule covers
            // every profile. A `None` caller-side profile can only match global rules.
            // 凭据作用域规则只覆盖精确配置档；全局规则覆盖全部配置档。
            // 调用方无法提供配置档时只能匹配全局规则。
            if !instance.credential_profile_ids.is_empty()
                && !credential_profile_id.is_some_and(|profile_id| {
                    instance
                        .credential_profile_ids
                        .iter()
                        .any(|declared| declared == profile_id)
                })
            {
                continue;
            }
            if !instance.matcher.matches(model_id) {
                continue;
            }
            let better = match best {
                None => true,
                Some(current) => instance_precedes(instance, current),
            };
            if better {
                best = Some(instance);
            }
        }
        best
    }
}

/// Reports whether `candidate` outranks `current` under exact-over-prefix, normalized-scope,
/// and longest-prefix rules.
/// 在精确优先、归一化作用域优先与最长前缀规则下报告 `candidate` 是否优于 `current`。
fn instance_precedes(candidate: &ResolvedRuleInstance, current: &ResolvedRuleInstance) -> bool {
    let candidate_precedence = candidate.matcher.precedence();
    let current_precedence = current.matcher.precedence();
    if candidate_precedence != current_precedence {
        return candidate_precedence < current_precedence;
    }
    // At the same matcher rank the normalized target set is the deterministic tie-breaker:
    // the strict subset wins; identical, incomparable, and disjoint pairs are never ordered
    // by array position. Validation rejects identical and incomparable pairs before
    // compilation, and disjoint pairs cannot both cover the same (entry, profile), so the
    // remaining comparison only ever sees a strict subset/superset pair.
    // 同一匹配器排名下，归一化目标集合是确定性决胜依据：严格子集胜出；相同、不可比较
    // 与不相交组合绝不按数组位置排序。验证器在编译前拒绝相同与不可比较组合，而不相交
    // 组合不可能同时覆盖同一（入口，配置档），因此剩余比较只会遇到严格子集/超集对。
    match candidate
        .normalized_scope
        .relation(&current.normalized_scope)
    {
        super::scope::ScopeRelation::StrictSubset => return true,
        super::scope::ScopeRelation::StrictSuperset => return false,
        super::scope::ScopeRelation::Identical
        | super::scope::ScopeRelation::Incomparable
        | super::scope::ScopeRelation::Disjoint => {}
    }
    match (candidate.matcher.kind, current.matcher.kind) {
        (super::MatcherKind::Prefix, super::MatcherKind::Prefix) => {
            candidate.matcher.value.len() > current.matcher.value.len()
        }
        _ => false,
    }
}

/// Immutable catalog of every compiled provider manifest.
/// 全部已编译供应商清单的不可变目录。
///
/// The catalog is published once and replaced whole via `build`; no in-place mutation ever
/// mixes generations (plan §R3.5/§6.2). The content hash explains every generation so a
/// generation bump without content change is impossible.
/// 目录一次性发布，并通过 `build` 整体替换；任何原地修改都不会混代（方案 §R3.5/§6.2）。
/// 内容哈希解释每一代，因此不存在"只递增代数却无内容变化"的代数。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ResolvedProviderCatalog {
    /// Compiled manifests keyed by stable provider id.
    /// 按稳定供应商标识索引的已编译清单。
    pub manifests: BTreeMap<String, std::sync::Arc<ResolvedProviderManifest>>,
    /// Monotonic catalog generation incremented on every atomic replacement.
    /// 每次原子替换递增的单调目录代数。
    pub generation: u64,
    /// Content-addressed digest over every manifest identity, revision, and source digest.
    /// 覆盖每份清单身份、修订号与来源摘要的内容寻址摘要。
    pub content_sha256: String,
    /// Bundled provider baselines that failed provider-local integrity or compilation checks.
    /// 未通过逐供应商完整性或编译校验的内置供应商基线。
    ///
    /// A provider may also exist in `manifests` when a trusted persisted Active or Previous
    /// layer recovered it during startup. Keeping the bundled failure preserves the missing
    /// fallback fact without withholding execution authority from the recovered layer.
    /// 当启动恢复使用可信持久化 Active 或 Previous 层恢复供应商时，该供应商也可能存在于
    /// `manifests`。保留内置失败可记录缺失回退层的事实，同时不剥夺恢复层的执行权威。
    pub bundled_failures: BTreeMap<String, BundledProviderManifestFailure>,
}

/// One provider-local failure from an otherwise trusted bundled integrity index.
/// 一条来自其他部分可信的内置完整性 Index 的逐供应商失败。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BundledProviderManifestFailure {
    /// Stable closed failure category exposed to status and telemetry projections.
    /// 暴露给状态与遥测投影的稳定封闭失败类别。
    pub failure_kind: String,
    /// Exact bounded local diagnostic for operators; manifest bytes are never retained here.
    /// 面向运维的精确有界本地诊断；此处绝不保留 Manifest 字节。
    pub message: String,
}

impl ResolvedProviderCatalog {
    /// Builds one complete immutable catalog from the exact manifest set.
    /// 从精确清单集合构建一份完整不可变目录。
    ///
    /// # Parameters
    /// 参数
    /// `manifests` is the full candidate set and `generation` is the next monotonic number;
    /// every manifest identity must match its map key.
    /// `manifests` 是完整候选集合，`generation` 是下一个单调数字；每份清单身份必须
    /// 与其映射键一致。
    ///
    /// # Returns
    /// 返回值
    /// The published catalog with its content digest, or an identity error.
    /// 返回携带内容摘要的已发布目录，或身份错误。
    pub fn build(
        manifests: BTreeMap<String, std::sync::Arc<ResolvedProviderManifest>>,
        generation: u64,
    ) -> Result<Self, String> {
        Self::build_with_bundled_failures(manifests, BTreeMap::new(), generation)
    }

    /// Builds one immutable catalog while retaining provider-local bundled baseline failures.
    /// 构建一份不可变目录，同时保留逐供应商内置基线失败。
    ///
    /// # Parameters
    /// 参数
    /// `manifests` is the executable validated set, `bundled_failures` records non-executable
    /// bundled identities from the same trusted index, and `generation` is monotonic.
    /// `manifests` 是可执行已校验集合，`bundled_failures` 记录同一可信 Index 中不可执行的
    /// 内置身份，`generation` 为单调代次。
    ///
    /// # Returns
    /// 返回值
    /// A content-addressed catalog or the first identity/failure-shape error.
    /// 返回内容寻址目录，或首个身份/失败结构错误。
    pub fn build_with_bundled_failures(
        manifests: BTreeMap<String, std::sync::Arc<ResolvedProviderManifest>>,
        bundled_failures: BTreeMap<String, BundledProviderManifestFailure>,
        generation: u64,
    ) -> Result<Self, String> {
        use sha2::Digest;
        let mut hasher = sha2::Sha256::new();
        for (provider_id, manifest) in &manifests {
            if manifest.descriptor.id != *provider_id {
                return Err(format!(
                    "catalog entry `{provider_id}` declares provider `{}`",
                    manifest.descriptor.id
                ));
            }
            hasher.update(provider_id.as_bytes());
            hasher.update([0]);
            hasher.update(manifest.revision.to_le_bytes());
            hasher.update([0]);
            hasher.update(
                manifest
                    .snapshot_provenance
                    .sha256
                    .as_deref()
                    .unwrap_or_default()
                    .as_bytes(),
            );
            hasher.update([0]);
        }
        for (provider_id, failure) in &bundled_failures {
            validate_bundled_provider_identity(provider_id)?;
            if failure.failure_kind != "bundled_baseline_invalid" {
                return Err(format!(
                    "bundled provider failure `{provider_id}` uses unsupported failure kind `{}`",
                    failure.failure_kind
                ));
            }
            if failure.message.is_empty()
                || failure.message.len() > BUNDLED_MANIFEST_FAILURE_MAX_BYTES
            {
                return Err(format!(
                    "bundled provider failure `{provider_id}` diagnostic must contain 1..={BUNDLED_MANIFEST_FAILURE_MAX_BYTES} UTF-8 bytes"
                ));
            }
            hasher.update(b"bundled-failure\0");
            hasher.update(provider_id.as_bytes());
            hasher.update([0]);
            hasher.update(failure.failure_kind.as_bytes());
            hasher.update([0]);
        }
        Ok(Self {
            manifests,
            generation,
            content_sha256: hex::encode(hasher.finalize()),
            bundled_failures,
        })
    }

    /// Looks up one compiled manifest by provider id.
    /// 按供应商标识查找一份已编译清单。
    pub fn manifest(&self, provider_id: &str) -> Option<&ResolvedProviderManifest> {
        self.manifests
            .get(provider_id)
            .map(|manifest| manifest.as_ref())
    }

    /// Clones one compiled manifest handle by provider id.
    /// 按供应商标识克隆一份已编译清单句柄。
    pub fn manifest_arc(
        &self,
        provider_id: &str,
    ) -> Option<std::sync::Arc<ResolvedProviderManifest>> {
        self.manifests.get(provider_id).cloned()
    }

    /// Returns every compiled provider descriptor in the catalog.
    /// 返回目录中全部已编译供应商描述。
    pub fn descriptors(&self) -> impl Iterator<Item = &ProviderDescriptor> {
        self.manifests.values().map(|resolved| &resolved.descriptor)
    }

    /// Returns every system identity pinned by the trusted bundled index.
    /// 返回可信内置 Index 固定的全部系统身份。
    ///
    /// Executable and provider-locally failed baselines are merged in deterministic order.
    /// 可执行基线与逐供应商失败基线按确定性顺序合并。
    pub fn bundled_provider_ids(&self) -> BTreeSet<String> {
        self.manifests
            .iter()
            .filter(|(_, manifest)| manifest.descriptor.kind == ProviderKind::System)
            .map(|(provider_id, _)| provider_id)
            .chain(self.bundled_failures.keys())
            .cloned()
            .collect()
    }
}

/// Loads and compiles every bundled system manifest into one immutable catalog.
/// 将全部内置系统清单加载并编译为一份不可变目录。
///
/// Global index or directory-integrity failures remain fatal because provider identities cannot be
/// trusted. Provider-local file failures are retained as explicit non-executable catalog entries,
/// allowing unrelated providers to start without silently weakening the trust ceiling. The catalog
/// is shared by registry, binding, discovery, execution, and status paths (plan §17.5/§17.6).
/// 全局 Index 或目录完整性失败仍属致命错误，因为此时供应商身份不可信。逐供应商文件失败
/// 会作为显式不可执行目录项保留，使无关供应商可以启动且不会静默削弱信任上限。该目录由
/// 注册表、绑定、发现、执行与状态路径共享（方案 §17.5/§17.6）。
pub fn load_bundled_resolved_catalog() -> ResolvedProviderCatalog {
    try_load_bundled_resolved_catalog()
        .expect("bundled provider manifest global integrity baseline must be valid")
}

/// Loads the trusted bundled index while isolating provider-local baseline failures.
/// 加载可信内置 Index，同时隔离逐供应商基线失败。
///
/// Global index/header/directory-boundary failures return an error because provider identities
/// cannot be trusted. Once those global facts validate, an invalid provider file is retained as
/// a closed catalog failure and cannot affect unrelated providers (plan §17.5).
/// 全局 Index、头部或目录边界失败会返回错误，因为此时供应商身份不可信。全局事实通过后，
/// 无效供应商文件会作为封闭目录失败保留，且不能影响无关供应商（方案 §17.5）。
///
/// # Returns
/// 返回值
/// A complete identity catalog containing executable manifests plus isolated provider failures,
/// or one global trust-baseline error.
/// 返回包含可执行 Manifest 与隔离供应商失败的完整身份目录，或一条全局信任基线错误。
pub fn try_load_bundled_resolved_catalog() -> Result<ResolvedProviderCatalog, String> {
    let index_path = vulcan_runtime_assets::resolve_bundled_provider_manifest_index_path();
    let directory = vulcan_runtime_assets::resolve_bundled_provider_manifests_directory();
    let loaded = load_bundled_manifest_generation_from_paths(&index_path, &directory)?;
    ResolvedProviderCatalog::build_with_bundled_failures(loaded.manifests, loaded.failures, 1)
}

/// Compiles one validated manifest into an immutable runtime snapshot.
/// 将一份已验证清单编译为不可变运行时快照。
///
/// # Parameters
/// 参数
/// `manifest` is the parsed document; `ownership` selects system or custom projection.
/// `manifest` 是已解析文档；`ownership` 选择系统或自定义投影。
///
/// # Returns
/// 返回值
/// The compiled snapshot, or the first validation failure.
/// 返回编译快照，或首个验证失败。
pub fn compile_manifest(
    manifest: &ProviderManifestV1,
    ownership: ManifestOwnership,
) -> Result<ResolvedProviderManifest, super::ManifestValidationError> {
    compile_manifest_with_layer(manifest, ownership, default_source_layer(ownership))
}

/// Returns the source layer inferred from one ownership class.
/// 返回从一种所有权类别推断出的来源层。
///
/// Bundled system manifests and user custom manifests use their ownership layer; persisted
/// active/previous restores must pass their exact layer explicitly.
/// 内置系统清单与用户自定义清单使用其所有权层；持久化 active/previous 恢复必须显式
/// 传入其精确层。
fn default_source_layer(ownership: ManifestOwnership) -> super::ManifestSourceLayer {
    match ownership {
        ManifestOwnership::System => super::ManifestSourceLayer::Bundled,
        ManifestOwnership::Custom => super::ManifestSourceLayer::Custom,
    }
}

/// Expands catalog model bindings through immutable provider-tool target templates.
/// 通过不可变供应商工具目标模板展开目录模型绑定。
///
/// # Parameters
/// 参数
/// `manifest` is the untrusted parsed document whose template declarations and bindings must form
/// one closed graph before ordinary target validation begins.
/// `manifest` 是未受信任的已解析文档；其模板声明与绑定必须在普通目标校验开始前形成闭合图。
///
/// # Returns
/// 返回值
/// A cloned manifest containing deterministic concrete targets and operation references, or every
/// template-graph failure found without granting partial execution authority.
/// 返回包含确定性具体目标与操作引用的 Manifest 克隆，或返回发现的全部模板图错误，且不授予
/// 部分执行权限。
pub(crate) fn expand_tool_target_templates(
    manifest: &ProviderManifestV1,
) -> Result<ExpandedToolTargetManifest, Vec<super::ManifestValidationError>> {
    const MAX_TEMPLATE_INSTANCES: usize = 512;
    let mut errors = Vec::new();
    let mut templates = BTreeMap::new();
    let service_entry_ids = manifest
        .services
        .iter()
        .map(|service| service.service_entry_id.as_str())
        .collect::<BTreeSet<_>>();
    for (index, template) in manifest.tool_target_templates.iter().enumerate() {
        if !is_stable_template_identifier(template.template_id.as_str()) {
            errors.push(super::ManifestValidationError::new(
                "invalid_tool_target_template_id",
                format!("$.tool_target_templates[{index}].template_id"),
                "tool-target template id must contain 1..128 lowercase ASCII letters, digits, dots, underscores, or hyphens",
            ));
        }
        if templates
            .insert(template.template_id.as_str(), template)
            .is_some()
        {
            errors.push(super::ManifestValidationError::new(
                "duplicate_tool_target_template_id",
                "$.tool_target_templates",
                format!(
                    "duplicate tool-target template id `{}`",
                    template.template_id
                ),
            ));
        }
        if template.allowed_service_entry_ids.is_empty()
            || template.allowed_service_entry_ids.len() > 16
            || template
                .allowed_service_entry_ids
                .iter()
                .any(|service_entry_id| !service_entry_ids.contains(service_entry_id.as_str()))
        {
            errors.push(super::ManifestValidationError::new(
                "invalid_tool_target_template_service_authority",
                format!(
                    "$.tool_target_templates[{index}].allowed_service_entry_ids"
                ),
                "tool-target template must authorize 1..16 service entries declared by the same manifest",
            ));
        }
    }
    let mut template_owners: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
    for (operation_index, operation) in manifest.operations.iter().enumerate() {
        let mut local_refs = BTreeSet::new();
        for template_id in &operation.tool_target_template_refs {
            if !local_refs.insert(template_id.as_str()) {
                errors.push(super::ManifestValidationError::new(
                    "duplicate_operation_tool_target_template_reference",
                    format!("$.operations[{operation_index}].tool_target_template_refs"),
                    format!(
                        "operation `{}` repeats template reference `{template_id}`",
                        operation.operation_id
                    ),
                ));
            }
            if !templates.contains_key(template_id.as_str()) {
                errors.push(super::ManifestValidationError::new(
                    "dangling_tool_target_template_reference",
                    format!("$.operations[{operation_index}].tool_target_template_refs"),
                    format!("tool-target template `{template_id}` is not declared"),
                ));
            }
            template_owners
                .entry(template_id.as_str())
                .or_default()
                .push(operation_index);
        }
    }
    for (template_index, template) in manifest.tool_target_templates.iter().enumerate() {
        let owner_count = template_owners
            .get(template.template_id.as_str())
            .map_or(0, Vec::len);
        if owner_count != 1 {
            errors.push(super::ManifestValidationError::new(
                "tool_target_template_operation_ownership",
                format!("$.tool_target_templates[{template_index}].template_id"),
                format!(
                    "tool-target template `{}` must be referenced by exactly one operation, found {owner_count}",
                    template.template_id
                ),
            ));
        }
    }

    let mut expanded = manifest.clone();
    let mut known_target_ids = manifest
        .tool_targets
        .iter()
        .map(|target| target.target_id.clone())
        .collect::<BTreeSet<_>>();
    let mut instances = BTreeMap::new();
    let mut template_instance_counts = manifest
        .tool_target_templates
        .iter()
        .map(|template| (template.template_id.as_str(), 0_usize))
        .collect::<BTreeMap<_, _>>();
    let mut binding_identities = BTreeSet::new();
    for (binding_index, binding) in manifest.tool_target_template_bindings.iter().enumerate() {
        let binding_path = format!("$.tool_target_template_bindings[{binding_index}]");
        if binding.model_id.trim().is_empty() || binding.display_name.trim().is_empty() {
            errors.push(super::ManifestValidationError::new(
                "empty_tool_target_template_binding_model",
                binding_path.clone(),
                "tool-target template binding model id and display name must be non-empty",
            ));
        }
        if !service_entry_ids.contains(binding.service_entry_id.as_str()) {
            errors.push(super::ManifestValidationError::new(
                "unknown_tool_target_template_binding_service",
                format!("{binding_path}.service_entry_id"),
                format!(
                    "template binding references unknown service entry `{}`",
                    binding.service_entry_id
                ),
            ));
            continue;
        }
        if !binding_identities
            .insert((binding.service_entry_id.as_str(), binding.model_id.as_str()))
        {
            errors.push(super::ManifestValidationError::new(
                "duplicate_tool_target_template_binding",
                binding_path.clone(),
                format!(
                    "service entry `{}` repeats template binding model `{}`",
                    binding.service_entry_id, binding.model_id
                ),
            ));
            continue;
        }
        if binding.template_refs.is_empty() || binding.template_refs.len() > 16 {
            errors.push(super::ManifestValidationError::new(
                "invalid_tool_target_template_binding_refs",
                format!("{binding_path}.template_refs"),
                "tool-target template binding must reference 1..16 templates",
            ));
        }
        let mut binding_refs = BTreeSet::new();
        for template_id in &binding.template_refs {
            if !binding_refs.insert(template_id.as_str()) {
                errors.push(super::ManifestValidationError::new(
                    "duplicate_model_tool_target_template_reference",
                    format!("{binding_path}.template_refs"),
                    format!(
                        "model `{}` repeats template reference `{template_id}`",
                        binding.model_id
                    ),
                ));
                continue;
            }
            let Some(template) = templates.get(template_id.as_str()).copied() else {
                errors.push(super::ManifestValidationError::new(
                    "dangling_model_tool_target_template_reference",
                    format!("{binding_path}.template_refs"),
                    format!("tool-target template `{template_id}` is not declared"),
                ));
                continue;
            };
            if !template
                .allowed_service_entry_ids
                .contains(binding.service_entry_id.as_str())
            {
                errors.push(super::ManifestValidationError::new(
                    "tool_target_template_service_not_authorized",
                    format!("{binding_path}.template_refs"),
                    format!(
                        "template `{template_id}` is not authorized for service entry `{}`",
                        binding.service_entry_id
                    ),
                ));
                continue;
            }
            if instances.len() >= MAX_TEMPLATE_INSTANCES {
                errors.push(super::ManifestValidationError::new(
                    "too_many_tool_target_template_instances",
                    "$.tool_target_template_bindings[].template_refs",
                    format!(
                        "manifest exceeds the {MAX_TEMPLATE_INSTANCES} tool-target template instance limit"
                    ),
                ));
                continue;
            }
            let target_id = template_instance_target_id(
                manifest.provider_id.as_str(),
                binding.service_entry_id.as_str(),
                binding.model_id.as_str(),
                template.template_id.as_str(),
            );
            if !known_target_ids.insert(target_id.clone()) {
                errors.push(super::ManifestValidationError::new(
                    "duplicate_instantiated_tool_target_id",
                    format!("{binding_path}.template_refs"),
                    format!("template instance target id `{target_id}` is duplicated"),
                ));
                continue;
            }
            let display_name = format!(
                "{} {}",
                binding.display_name.trim(),
                template.display_name_suffix.trim()
            )
            .trim()
            .to_string();
            expanded.tool_targets.push(super::ToolTargetDeclaration {
                target_id: target_id.clone(),
                description: template.description.clone(),
                service_entry_id: binding.service_entry_id.clone(),
                credential_profile_ids: template.credential_profile_ids.clone(),
                upstream_target_id: binding.model_id.clone(),
                availability_model_ids_any: BTreeSet::new(),
                display_name,
                tool_id: template.tool_id.clone(),
                capability: template.capability,
                input_schema: template.input_schema.clone(),
                input_numeric_product_ranges: template.input_numeric_product_ranges.clone(),
                extension_profile: template.extension_profile.clone(),
                output: template.output.clone(),
                execution: template.execution.clone(),
            });
            for operation_index in template_owners
                .get(template.template_id.as_str())
                .into_iter()
                .flatten()
            {
                expanded.operations[*operation_index]
                    .tool_target_refs
                    .push(target_id.clone());
            }
            instances.insert(
                target_id.clone(),
                ToolTargetTemplateInstanceProvenance {
                    target_id,
                    template_id: template.template_id.clone(),
                    service_entry_id: binding.service_entry_id.clone(),
                    model_id: binding.model_id.clone(),
                    model_display_name: binding.display_name.clone(),
                },
            );
            if let Some(count) = template_instance_counts.get_mut(template.template_id.as_str()) {
                *count += 1;
            }
        }
    }
    for (template_index, template) in manifest.tool_target_templates.iter().enumerate() {
        if template_instance_counts
            .get(template.template_id.as_str())
            .copied()
            .unwrap_or_default()
            == 0
        {
            errors.push(super::ManifestValidationError::new(
                "unused_tool_target_template",
                format!("$.tool_target_templates[{template_index}].template_id"),
                format!(
                    "tool-target template `{}` must be instantiated by at least one model binding",
                    template.template_id
                ),
            ));
        }
    }
    if errors.is_empty() {
        Ok(ExpandedToolTargetManifest {
            manifest: expanded,
            instances,
        })
    } else {
        Err(errors)
    }
}

/// Reports whether one template identifier belongs to the closed manifest identifier grammar.
/// 报告一个模板标识是否属于封闭 Manifest 标识语法。
///
/// # Parameters
/// 参数
/// `value` is the exact untrusted template identifier.
/// `value` 是精确的未受信任模板标识。
///
/// # Returns
/// 返回值
/// True only for bounded lowercase ASCII identifiers.
/// 仅对有界小写 ASCII 标识返回真。
fn is_stable_template_identifier(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'_' | b'-')
        })
}

/// Builds one stable collision-resistant identifier for a concrete template instance.
/// 为一个具体模板实例构建稳定且抗碰撞的标识。
///
/// # Parameters
/// 参数
/// All inputs are exact manifest identities; no field is inferred from display text.
/// 全部输入均为精确 Manifest 身份；不从显示文本推断任何字段。
///
/// # Returns
/// 返回值
/// A deterministic target identifier that does not embed an untrusted model id as syntax.
/// 返回不会把未受信任模型标识嵌入语法的确定性目标标识。
fn template_instance_target_id(
    provider_id: &str,
    service_entry_id: &str,
    model_id: &str,
    template_id: &str,
) -> String {
    let mut digest = Sha256::new();
    for value in [provider_id, service_entry_id, model_id, template_id] {
        digest.update((value.len() as u64).to_be_bytes());
        digest.update(value.as_bytes());
    }
    format!("template.{template_id}.{:x}", digest.finalize())
}

/// Compiles one validated manifest with an explicit manifest source layer.
/// 以显式清单来源层编译一份已验证清单。
///
/// # Parameters
/// 参数
/// `manifest` is the parsed document; `ownership` selects system or custom projection;
/// `source_layer` records the exact Bundled/Active/Previous/Custom layer so restores never
/// mislabel a persisted layer as bundled (plan §R2.6/R2.7).
/// `manifest` 是已解析文档；`ownership` 选择系统或自定义投影；`source_layer` 记录精确的
/// Bundled/Active/Previous/Custom 层，使恢复路径绝不把持久化层误标为内置（方案 §R2.6/R2.7）。
///
/// # Returns
/// 返回值
/// The compiled snapshot, or the first validation failure.
/// 返回编译快照，或首个验证失败。
pub fn compile_manifest_with_layer(
    manifest: &ProviderManifestV1,
    ownership: ManifestOwnership,
    source_layer: super::ManifestSourceLayer,
) -> Result<ResolvedProviderManifest, super::ManifestValidationError> {
    let registry = EnginePrimitiveRegistry::current_protocol_baseline();
    if let Err(errors) = validate_manifest(manifest, ownership, &registry) {
        return Err(errors.into_iter().next().unwrap_or_else(|| {
            super::ManifestValidationError::new(
                "manifest_validation_failed",
                "$",
                "manifest failed validation without a specific error",
            )
        }));
    }
    let expanded = expand_tool_target_templates(manifest).map_err(|errors| {
        errors.into_iter().next().unwrap_or_else(|| {
            super::ManifestValidationError::new(
                "tool_target_template_expansion_failed",
                "$.tool_target_templates",
                "tool-target template expansion failed without a specific error",
            )
        })
    })?;
    let manifest = &expanded.manifest;

    let descriptor = compile_descriptor(manifest, ownership)?;
    let rule_instances = resolve_rule_instances(manifest);
    validate_resolved_model_parameter_sets(rule_instances.as_slice())?;
    let default_capabilities = manifest.model_defaults.capabilities.as_ref().map(|patch| {
        let mut compiled = vulcan_ai_contract::ModelCapabilities::default();
        let mut written = Vec::new();
        super::inheritance::apply_capability_patch(&mut compiled, patch, &mut written);
        compiled
    });
    let declaration_provenance =
        compile_declaration_provenance(manifest, &expanded.instances, source_layer);

    let catalog_models = manifest
        .services
        .iter()
        .filter_map(|entry| {
            entry
                .model_catalog
                .as_ref()
                .and_then(|catalog| catalog.models.as_ref())
                .map(|models| (entry.service_entry_id.clone(), models.clone()))
        })
        .collect();
    // modelSourcePolicies freezes the explicit identity authority independently from discovery implementation.
    // modelSourcePolicies 独立于发现实现冻结显式身份权威。
    let model_source_policies = manifest
        .services
        .iter()
        .filter(|entry| entry.kind == super::ManifestEntryKind::Conversation)
        .filter_map(|entry| {
            entry
                .model_catalog
                .as_ref()
                .map(|catalog| (entry.service_entry_id.clone(), catalog.source_policy))
        })
        .collect();
    // entryModelDefaults freezes service overrides once so every discovery, status, and
    // execution consumer reads the same effective default protocol authority.
    // entryModelDefaults 一次性冻结服务入口覆盖，使发现、状态与执行消费者读取同一份
    // 生效默认协议权威。
    let entry_model_defaults = manifest
        .services
        .iter()
        .filter(|entry| entry.kind == super::ManifestEntryKind::Conversation)
        .map(|entry| {
            let overlay = entry.model_defaults_override.as_ref();
            let supported_protocols = overlay
                .and_then(|overlay| overlay.supported_protocols.clone())
                .unwrap_or_else(|| manifest.model_defaults.supported_protocols.clone());
            let selected_protocol = overlay
                .and_then(|overlay| overlay.selected_protocol.clone())
                .or_else(|| manifest.model_defaults.selected_protocol.clone());
            (
                entry.service_entry_id.clone(),
                ResolvedEntryModelDefaults {
                    supported_protocols,
                    selected_protocol,
                    capabilities: default_capabilities.clone(),
                    usage_profile_refs: manifest.model_defaults.default_usage_profile_refs.clone(),
                    request_profile_refs: manifest
                        .model_defaults
                        .default_request_profile_refs
                        .clone(),
                    response_profile_refs: manifest
                        .model_defaults
                        .default_response_profile_refs
                        .clone(),
                    operation_refs: manifest.model_defaults.default_operation_refs.clone(),
                    provenance: super::ResolvedModelDefaultsProvenance {
                        manifest_revision: manifest.revision,
                        provenance_ref: manifest.model_defaults.provenance_ref.clone(),
                        provenance_status: manifest.provenance.status,
                    },
                },
            )
        })
        .collect();
    Ok(ResolvedProviderManifest {
        revision: manifest.revision,
        descriptor,
        engine_contract: manifest.engine_contract.clone(),
        manifest_provenance: manifest.provenance.clone(),
        rule_instances,
        catalog_models,
        model_source_policies,
        unknown_model_policy: manifest.model_defaults.unknown_model_policy,
        catalog_membership_policy: manifest.model_defaults.catalog_membership_policy,
        default_enabled: manifest.model_defaults.default_enabled,
        entry_model_defaults,
        usage_profiles: manifest.usage_profiles.clone(),
        request_profiles: manifest.request_profiles.clone(),
        response_profiles: manifest.response_profiles.clone(),
        upload_channels: manifest.upload_channels.clone(),
        operations: manifest.operations.clone(),
        tool_targets: compile_tool_targets(manifest),
        tool_target_template_instances: expanded.instances,
        declaration_provenance,
        snapshot_provenance: ManifestSnapshotProvenance {
            source_layer,
            revision: manifest.revision,
            sha256: None,
            provenance_status: manifest.provenance.status,
        },
    })
}

/// Validates every post-inheritance model-parameter set for each authorized protocol.
/// 校验每个授权协议在继承后的模型参数集合。
///
/// # Parameters
/// 参数
/// `instances` contains the immutable rule instances produced by inheritance resolution.
/// `instances` 包含继承解析产生的不可变规则实例。
///
/// # Returns
/// 返回值
/// Success only when each effective set has closed definitions and resolvable relationships.
/// 仅当每个生效集合均具有封闭定义及可解析关系时返回成功。
fn validate_resolved_model_parameter_sets(
    instances: &[ResolvedRuleInstance],
) -> Result<(), super::ManifestValidationError> {
    for instance in instances {
        let mut protocols = instance
            .supported_protocols
            .iter()
            .map(String::as_str)
            .collect::<BTreeSet<_>>();
        protocols.extend(instance.protocol_capabilities.keys().map(String::as_str));
        if let Some(selected_protocol) = instance.selected_protocol.as_deref() {
            protocols.insert(selected_protocol);
        }
        if protocols.is_empty() {
            validate_resolved_model_parameter_set(instance, None)?;
        } else {
            for protocol_id in protocols {
                validate_resolved_model_parameter_set(instance, Some(protocol_id))?;
            }
        }
    }
    Ok(())
}

/// Validates one rule instance's effective model-parameter set for one protocol authority.
/// 校验一个规则实例在一个协议权威下的生效模型参数集合。
///
/// # Parameters
/// 参数
/// `instance` is the resolved rule and `protocol_id` selects one protocol-specific overlay.
/// `instance` 是已解析规则，`protocol_id` 选择一个协议专属覆盖层。
///
/// # Returns
/// 返回值
/// Success when stable-ID merging leaves one coherent definition set.
/// 当按稳定 ID 合并后得到一致定义集合时返回成功。
fn validate_resolved_model_parameter_set(
    instance: &ResolvedRuleInstance,
    protocol_id: Option<&str>,
) -> Result<(), super::ManifestValidationError> {
    let mut parameters = Vec::new();
    if let Some(capabilities) = instance.capabilities.as_ref() {
        merge_effective_model_parameters(
            &mut parameters,
            capabilities.model_parameters.as_slice(),
            protocol_id,
        );
    }
    if let Some(protocol_capabilities) =
        protocol_id.and_then(|protocol_id| instance.protocol_capabilities.get(protocol_id))
    {
        merge_effective_model_parameters(
            &mut parameters,
            protocol_capabilities.model_parameters.as_slice(),
            protocol_id,
        );
    }
    merge_effective_model_parameters(
        &mut parameters,
        instance.model_parameters.as_slice(),
        protocol_id,
    );
    vulcan_ai_contract::validate_model_parameter_definition_set(parameters.as_slice()).map_err(
        |error| {
            super::ManifestValidationError::new(
                "invalid_effective_model_parameter_set",
                format!(
                    "$.model_rules[rule_id={}].protocol[{}]",
                    instance.rule_id,
                    protocol_id.unwrap_or("unscoped")
                ),
                error,
            )
        },
    )
}

/// Merges applicable model-parameter definitions by stable ID at declared precedence.
/// 按声明优先级以稳定 ID 合并适用的模型参数定义。
///
/// # Parameters
/// 参数
/// `target` is the accumulated effective set, `source` is the next precedence layer, and
/// `protocol_id` limits protocol-scoped definitions when present.
/// `target` 是累计生效集合，`source` 是下一优先级层，`protocol_id` 在存在时限制协议作用域定义。
fn merge_effective_model_parameters(
    target: &mut Vec<vulcan_ai_contract::ModelParameterDefinition>,
    source: &[vulcan_ai_contract::ModelParameterDefinition],
    protocol_id: Option<&str>,
) {
    for definition in source {
        if protocol_id.is_some_and(|protocol_id| {
            !definition.protocol_ids.is_empty()
                && !definition
                    .protocol_ids
                    .iter()
                    .any(|candidate| candidate == protocol_id)
        }) {
            continue;
        }
        if let Some(existing) = target
            .iter_mut()
            .find(|existing| existing.id == definition.id)
        {
            *existing = definition.clone();
        } else {
            target.push(definition.clone());
        }
    }
}

/// Compiles deterministic provenance records for every manifest-owned executable declaration.
/// 为 Manifest 持有的每个可执行声明编译确定性来源记录。
fn compile_declaration_provenance(
    manifest: &ProviderManifestV1,
    template_instances: &BTreeMap<String, ToolTargetTemplateInstanceProvenance>,
    source_layer: super::ManifestSourceLayer,
) -> BTreeMap<String, super::ManifestDeclarationProvenance> {
    let mut provenance = BTreeMap::new();
    let mut insert = |kind: &str, declaration_id: &str| {
        provenance.insert(
            format!("{kind}.{declaration_id}"),
            super::ManifestDeclarationProvenance {
                provider_id: manifest.provider_id.clone(),
                declaration_kind: kind.to_string(),
                declaration_id: declaration_id.to_string(),
                source_layer,
                manifest_revision: manifest.revision,
                sha256: None,
            },
        );
    };
    for declaration in &manifest.usage_profiles {
        insert("usage_profile", declaration.profile_id.as_str());
    }
    for declaration in &manifest.request_profiles {
        insert("request_profile", declaration.profile_id.as_str());
    }
    for declaration in &manifest.response_profiles {
        insert("response_profile", declaration.profile_id.as_str());
    }
    for declaration in &manifest.upload_channels {
        insert("upload_channel", declaration.upload_channel_id.as_str());
    }
    for declaration in &manifest.operations {
        insert("operation", declaration.operation_id.as_str());
    }
    for declaration in &manifest.tool_targets {
        insert("tool_target", declaration.target_id.as_str());
    }
    for declaration in &manifest.tool_target_templates {
        insert("tool_target_template", declaration.template_id.as_str());
    }
    for declaration in template_instances.values() {
        insert(
            "tool_target_template_instance",
            declaration.target_id.as_str(),
        );
    }
    provenance
}

/// Compiles the runtime provider descriptor from one manifest.
/// 从一份清单编译运行时供应商描述。
fn compile_descriptor(
    manifest: &ProviderManifestV1,
    ownership: ManifestOwnership,
) -> Result<ProviderDescriptor, super::ManifestValidationError> {
    let mut service_entries = Vec::with_capacity(manifest.services.len());
    for entry in manifest.services.iter() {
        service_entries.push(compile_service_entry(
            entry,
            &manifest.usage_profiles,
            &manifest.request_profiles,
            &manifest.response_profiles,
        )?);
    }

    Ok(ProviderDescriptor {
        id: manifest.provider_id.clone(),
        display_name: manifest.display.name.clone(),
        kind: match ownership {
            ManifestOwnership::System => ProviderKind::System,
            ManifestOwnership::Custom => ProviderKind::Custom,
        },
        category: match manifest.display.category {
            super::ManifestCategory::Unclassified => ProviderCategory::Unclassified,
            super::ManifestCategory::Official => ProviderCategory::Official,
            super::ManifestCategory::CloudMaaS => ProviderCategory::CloudMaaS,
            super::ManifestCategory::Relay => ProviderCategory::Relay,
        },
        credential_directory_visibility: match manifest.display.credential_directory_visibility {
            super::ManifestDirectoryVisibility::ConfiguredOnly => {
                CredentialDirectoryVisibility::ConfiguredOnly
            }
            super::ManifestDirectoryVisibility::PinnedOnboarding => {
                CredentialDirectoryVisibility::PinnedOnboarding
            }
        },
        credential_registration: manifest.credential_registration,
        promotion: manifest.display.promotion.as_ref().map(compile_promotion),
        service_entries,
    })
}

/// Compiles every declared provider tool target into the runtime descriptor.
/// 把全部声明的供应商工具目标编译进运行时描述。
///
/// The provider id is filled from the manifest identity; an empty `service_entry_id`
/// resolves to the first conversation entry (plan §R9.2).
/// 供应商标识由清单身份填充；空 `service_entry_id` 解析为首个会话入口（方案 §R9.2）。
fn compile_tool_targets(manifest: &ProviderManifestV1) -> Vec<crate::ProviderToolTarget> {
    let default_entry_id = manifest
        .services
        .iter()
        .find(|entry| entry.kind == super::ManifestEntryKind::Conversation)
        .map(|entry| entry.service_entry_id.as_str())
        .unwrap_or_default();
    manifest
        .tool_targets
        .iter()
        .map(|target| crate::ProviderToolTarget {
            target_id: target.target_id.clone(),
            provider_id: manifest.provider_id.clone(),
            service_entry_id: if target.service_entry_id.is_empty() {
                default_entry_id.to_string()
            } else {
                target.service_entry_id.clone()
            },
            credential_profile_ids: target.credential_profile_ids.clone(),
            upstream_target_id: target.upstream_target_id.clone(),
            availability_model_ids_any: target.availability_model_ids_any.clone(),
            display_name: target.display_name.clone(),
            contract: crate::ProviderToolContract {
                tool_id: target.tool_id.clone(),
                description: target.description.clone(),
                capability: target.capability,
                input_schema: target.input_schema.clone(),
                input_numeric_product_ranges: target.input_numeric_product_ranges.clone(),
                extension_profile: target
                    .extension_profile
                    .clone()
                    .or_else(|| synthesize_provider_tool_extension_profile(manifest, target)),
                output: target.output.clone(),
            },
            execution: target.execution.clone(),
        })
        .collect()
}

/// Synthesizes one deterministic model-extension profile from an exact native target schema.
/// 根据精确原生目标 Schema 确定性生成一个模型扩展说明档案。
///
/// # Parameters
/// 参数
/// `manifest` supplies provider provenance and `target` supplies the already reviewed model-native
/// schema, description, output contract, and hard product ranges.
/// `manifest` 提供供应商溯源，`target` 提供已经审核的模型原生 Schema、说明、输出契约与硬乘积约束。
///
/// # Returns
/// 返回值
/// A complete runtime discovery profile for generative capabilities, or `None` for direct tools.
/// 为生成类能力返回完整运行时查询档案；直连工具返回 `None`。
fn synthesize_provider_tool_extension_profile(
    manifest: &ProviderManifestV1,
    target: &super::ToolTargetDeclaration,
) -> Option<crate::ProviderToolExtensionProfile> {
    if !target.capability.is_model_extension() {
        return None;
    }
    let properties = target
        .input_schema
        .get("properties")
        .and_then(serde_json::Value::as_object);
    let mut input_modes = BTreeSet::new();
    if let Some(properties) = properties {
        for property_name in properties.keys() {
            let name = property_name.to_ascii_lowercase();
            if name.contains("image") || name.contains("mask") {
                input_modes.insert(String::from("image"));
            }
            if name.contains("video") {
                input_modes.insert(String::from("video"));
            }
            if name.contains("audio") || name.contains("voice") {
                input_modes.insert(String::from("audio"));
            }
            if name.contains("prompt") || name.contains("text") || name.contains("lyrics") {
                input_modes.insert(String::from("text"));
            }
        }
    }
    if input_modes.is_empty() {
        input_modes.insert(String::from("structured_arguments"));
    }
    let resolution_policy = synthesize_provider_tool_resolution_policy(properties).or_else(|| {
        matches!(
            target.capability,
            crate::ProviderToolCapabilityKind::ImageGenerate
                | crate::ProviderToolCapabilityKind::ImageEdit
                | crate::ProviderToolCapabilityKind::VideoGenerate
                | crate::ProviderToolCapabilityKind::VideoEdit
                | crate::ProviderToolCapabilityKind::VideoExtend
        )
        .then_some(crate::ProviderToolResolutionPolicy {
            mode: crate::ProviderToolResolutionMode::ProviderManaged,
            presets: Vec::new(),
            aspect_ratios: Vec::new(),
            custom_bounds: None,
        })
    });
    let duration_policy = synthesize_provider_tool_duration_policy(properties);
    let format_policy = synthesize_provider_tool_format_policy(properties, target);
    let required = target
        .input_schema
        .get("required")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(serde_json::Value::as_str)
        .collect::<Vec<_>>();
    let mut constraints = vec![String::from(
        "Arguments must satisfy the exact model-specific JSON Schema returned by get_model_extension_parameters.",
    )];
    if !required.is_empty() {
        constraints.push(format!("Required arguments: {}.", required.join(", ")));
    }
    if !target.input_numeric_product_ranges.is_empty() {
        constraints.push(String::from(
            "Declared width, height, or other numeric factors must satisfy the host-enforced product ranges.",
        ));
    }
    let mut limitations = vec![String::from(
        "No capability beyond the returned schema and reviewed output contract is implied.",
    )];
    if resolution_policy
        .as_ref()
        .is_some_and(|policy| policy.mode == crate::ProviderToolResolutionMode::ProviderManaged)
    {
        limitations.push(String::from(
            "The provider controls final output dimensions; requested dimensions are not a pixel guarantee.",
        ));
    }
    let summary = if target.description.trim().is_empty() {
        format!(
            "Use the native {} model for the {:?} extension capability.",
            target.display_name, target.capability
        )
    } else {
        target.description.trim().to_string()
    };
    Some(crate::ProviderToolExtensionProfile {
        summary,
        characteristics: vec![
            format!("Native provider model: {}.", target.display_name),
            format!("Extension capability: {:?}.", target.capability),
        ],
        input_modes: input_modes.into_iter().collect(),
        resolution_policy,
        duration_policy,
        format_policy,
        constraints,
        limitations,
        evidence_refs: vec![format!(
            "manifest:{}#tool_target:{}",
            manifest.provider_id, target.target_id
        )],
    })
}

/// Derives reviewed resolution behavior from closed schema properties.
/// 从封闭 Schema 属性推导经过审核的分辨率行为。
fn synthesize_provider_tool_resolution_policy(
    properties: Option<&serde_json::Map<String, serde_json::Value>>,
) -> Option<crate::ProviderToolResolutionPolicy> {
    let properties = properties?;
    let presets = ["size", "resolution"]
        .iter()
        .filter_map(|name| properties.get(*name))
        .flat_map(json_schema_string_enum)
        .map(|value| {
            let dimensions = parse_resolution_dimensions(value.as_str());
            crate::ProviderToolResolutionPreset {
                value,
                width: dimensions.map(|(width, _)| width),
                height: dimensions.map(|(_, height)| height),
            }
        })
        .collect::<Vec<_>>();
    let aspect_ratios = properties
        .get("aspect_ratio")
        .into_iter()
        .flat_map(json_schema_string_enum)
        .collect::<Vec<_>>();
    if !presets.is_empty() {
        return Some(crate::ProviderToolResolutionPolicy {
            mode: crate::ProviderToolResolutionMode::FixedPresets,
            presets,
            aspect_ratios,
            custom_bounds: None,
        });
    }
    let width = properties.get("width");
    let height = properties.get("height");
    if let (Some(width), Some(height)) = (width, height) {
        let minimum_width = json_schema_u32(width, "minimum")?;
        let maximum_width = json_schema_u32(width, "maximum")?;
        let minimum_height = json_schema_u32(height, "minimum")?;
        let maximum_height = json_schema_u32(height, "maximum")?;
        let step = json_schema_u32(width, "multipleOf")
            .or_else(|| json_schema_u32(height, "multipleOf"))
            .unwrap_or(1);
        return Some(crate::ProviderToolResolutionPolicy {
            mode: crate::ProviderToolResolutionMode::BoundedCustom,
            presets: Vec::new(),
            aspect_ratios,
            custom_bounds: Some(crate::ProviderToolResolutionBounds {
                minimum_width,
                maximum_width,
                minimum_height,
                maximum_height,
                step,
                minimum_pixels: None,
                maximum_pixels: None,
            }),
        });
    }
    Some(crate::ProviderToolResolutionPolicy {
        mode: crate::ProviderToolResolutionMode::ProviderManaged,
        presets: Vec::new(),
        aspect_ratios,
        custom_bounds: None,
    })
}

/// Derives one duration range from exact schema bounds or enumerated presets.
/// 从精确 Schema 边界或枚举预设推导时长范围。
fn synthesize_provider_tool_duration_policy(
    properties: Option<&serde_json::Map<String, serde_json::Value>>,
) -> Option<crate::ProviderToolDurationPolicy> {
    let schema = properties?.get("duration")?;
    let presets_seconds = schema
        .get("enum")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(serde_json::Value::as_u64)
        .filter_map(|value| u32::try_from(value).ok())
        .collect::<Vec<_>>();
    let minimum_seconds =
        json_schema_u32(schema, "minimum").or_else(|| presets_seconds.iter().min().copied())?;
    let maximum_seconds =
        json_schema_u32(schema, "maximum").or_else(|| presets_seconds.iter().max().copied())?;
    Some(crate::ProviderToolDurationPolicy {
        minimum_seconds,
        maximum_seconds,
        default_seconds: json_schema_u32(schema, "default"),
        presets_seconds,
    })
}

/// Derives declared request formats and normalized output MIME types.
/// 推导声明的请求格式与归一化输出 MIME 类型。
fn synthesize_provider_tool_format_policy(
    properties: Option<&serde_json::Map<String, serde_json::Value>>,
    target: &super::ToolTargetDeclaration,
) -> Option<crate::ProviderToolFormatPolicy> {
    let input_formats = ["format", "output_format", "response_format"]
        .into_iter()
        .filter_map(|name| properties.and_then(|properties| properties.get(name)))
        .flat_map(json_schema_string_enum)
        .collect::<Vec<_>>();
    let output_formats = target.output.mime_types.clone();
    (!input_formats.is_empty() || !output_formats.is_empty()).then_some(
        crate::ProviderToolFormatPolicy {
            input_formats,
            output_formats,
            sample_rates_hz: Vec::new(),
            channel_counts: Vec::new(),
        },
    )
}

/// Extracts exact string enum values from one JSON Schema node.
/// 从一个 JSON Schema 节点提取精确字符串枚举值。
fn json_schema_string_enum(schema: &serde_json::Value) -> Vec<String> {
    schema
        .get("enum")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(serde_json::Value::as_str)
        .map(str::to_string)
        .collect()
}

/// Reads one non-negative 32-bit integer keyword from a JSON Schema node.
/// 从一个 JSON Schema 节点读取非负 32 位整数关键字。
fn json_schema_u32(schema: &serde_json::Value, keyword: &str) -> Option<u32> {
    schema
        .get(keyword)
        .and_then(serde_json::Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
}

/// Parses an exact `WIDTHxHEIGHT` resolution preset when present.
/// 在存在时解析精确 `宽x高` 分辨率预设。
fn parse_resolution_dimensions(value: &str) -> Option<(u32, u32)> {
    let (width, height) = value.split_once('x')?;
    Some((width.parse().ok()?, height.parse().ok()?))
}

/// Compiles one promotion block.
/// 编译一个推广块。
fn compile_promotion(promotion: &super::ManifestPromotion) -> ProviderPromotion {
    ProviderPromotion {
        signup_url: promotion.signup_url.clone(),
        referral_url: promotion.referral_url.clone(),
        campaign_id: promotion.campaign_id.clone(),
        partnership_kind: promotion.partnership_kind,
        disclosure_key: promotion.disclosure_key,
    }
}

/// Compiles one service entry.
/// 编译一个服务入口。
fn compile_service_entry(
    entry: &super::ManifestServiceEntry,
    usage_profiles: &[super::UsageProfileDeclaration],
    request_profiles: &[super::RequestProfileDeclaration],
    response_profiles: &[super::ResponseProfileDeclaration],
) -> Result<ServiceEntry, super::ManifestValidationError> {
    let credential_profiles = entry
        .credential_profiles
        .iter()
        .map(compile_credential_profile)
        .collect::<Vec<_>>();

    let mut allowed_usage_scopes: BTreeSet<InferenceUsageScope> = BTreeSet::new();
    for profile in &credential_profiles {
        allowed_usage_scopes.extend(profile.allowed_usage_scopes.iter().copied());
    }

    let protocol_routes = entry
        .protocol_routes
        .iter()
        .map(|route| {
            compile_protocol_route(route, usage_profiles, request_profiles, response_profiles)
        })
        .collect::<Result<Vec<_>, _>>()?;

    let model_discovery = if entry.kind == super::ManifestEntryKind::ProviderTools {
        // Provider-tools entries never discover conversation models; the runtime contract
        // requires the dedicated marker kind instead of a manual fallback.
        // 纯工具入口从不发现对话模型；运行时契约要求专用标记种类而不是手工回退。
        ModelDiscoveryKind::ProviderToolsOnly
    } else {
        entry
            .model_catalog
            .as_ref()
            .and_then(|catalog| catalog.discovery_kind)
            .or_else(|| {
                protocol_routes
                    .iter()
                    .find_map(|route| route.model_discovery)
            })
            .unwrap_or(ModelDiscoveryKind::Manual)
    };

    Ok(ServiceEntry {
        id: entry.service_entry_id.clone(),
        display_name: entry.display_name.clone(),
        kind: match entry.kind {
            super::ManifestEntryKind::Conversation => ServiceEntryKind::Conversation,
            super::ManifestEntryKind::ProviderTools => ServiceEntryKind::ProviderTools,
        },
        base_url: entry.base_url.clone(),
        credential_profiles,
        authentication: entry
            .default_authentication
            .unwrap_or(AuthenticationProjection::BearerAuthorization),
        request_headers: entry.request_headers.clone(),
        service_origin_override_policy: entry.service_origin_override_policy.clone(),
        native_inference_bindings: entry.native_inference_bindings.clone(),
        model_discovery,
        request_projector: entry
            .request_projector
            .unwrap_or(RequestProjectorKind::Direct),
        protocol_routes,
        allowed_usage_scopes,
    })
}

/// Compiles one credential profile.
/// 编译一个凭据配置档。
fn compile_credential_profile(profile: &super::ManifestCredentialProfile) -> CredentialProfile {
    CredentialProfile {
        id: profile.credential_profile_id.clone(),
        display_name: profile.display_name.clone(),
        credential_mode: profile.credential_mode,
        oauth_provider: profile.oauth_provider,
        billing_kind: profile.billing_kind,
        account_usage_query: profile.account_usage_query,
        authentication: profile.authentication,
        key_prefix: profile.key_prefix.clone(),
        credential_role: profile.credential_role.clone(),
        allowed_usage_scopes: profile.allowed_usage_scopes.clone(),
    }
}

/// Compiles one protocol route.
/// 编译一条协议路由。
///
/// # Parameters
/// 参数
/// `route` is the manifest declaration and `usage_profiles` is the named declaration table used
/// to resolve `usage_profile_refs` into frozen extraction payloads (plan §15.4); the
/// `request_profiles` / `response_profiles` tables resolve the closed projection payloads
/// (plan §6.6/§R8.2).
/// `route` 是清单声明，`usage_profiles` 是用于把 `usage_profile_refs` 解析为冻结提取载荷
/// 的具名声明表（方案 §15.4）；`request_profiles` / `response_profiles` 表解析封闭投影
/// 载荷（方案 §6.6/§R8.2）。
fn compile_protocol_route(
    route: &super::ManifestProtocolRoute,
    usage_profiles: &[super::UsageProfileDeclaration],
    request_profiles: &[super::RequestProfileDeclaration],
    response_profiles: &[super::ResponseProfileDeclaration],
) -> Result<ProtocolRoute, super::ManifestValidationError> {
    let usage_extraction_profiles = route
        .usage_profile_refs
        .iter()
        .map(|profile_id| {
            usage_profiles
                .iter()
                .find(|declaration| declaration.profile_id == *profile_id)
                .and_then(|declaration| declaration.payload.clone())
                .ok_or_else(|| unresolved_route_profile_error(route, "usage", profile_id.as_str()))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let request_projection_profiles = route
        .request_profile_refs
        .iter()
        .map(|profile_id| {
            request_profiles
                .iter()
                .find(|declaration| declaration.profile_id == *profile_id)
                .and_then(|declaration| declaration.payload.clone())
                .ok_or_else(|| {
                    unresolved_route_profile_error(route, "request", profile_id.as_str())
                })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let response_projection_profiles = route
        .response_profile_refs
        .iter()
        .map(|profile_id| {
            response_profiles
                .iter()
                .find(|declaration| declaration.profile_id == *profile_id)
                .and_then(|declaration| declaration.payload.clone())
                .ok_or_else(|| {
                    unresolved_route_profile_error(route, "response", profile_id.as_str())
                })
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ProtocolRoute {
        protocol_profile_id: route.protocol_profile_id.clone(),
        endpoint_path: route.endpoint_path.clone(),
        base_url_override: route.base_url_override.clone(),
        request_headers: route.request_headers.clone(),
        authentication: route.authentication,
        model_discovery: route.model_discovery,
        model_discovery_path: route.model_discovery_path.clone(),
        model_discovery_base_url_override: route.model_discovery_base_url_override.clone(),
        responses_features: route.responses_features,
        input_resource_constraints: route.input_resource_constraints.clone(),
        usage_extraction_profiles,
        request_projection_profiles,
        response_projection_profiles,
        provider_request_id_headers: route.provider_request_id_headers.clone(),
        verified: route.verified,
    })
}

/// Builds one fail-closed compiler error for an unresolved route profile payload.
/// 为未解析的路由 Profile 载荷构建一条失败关闭编译错误。
///
/// # Parameters
/// 参数
/// `route` owns the exact protocol route, `profile_kind` classifies the declaration family, and
/// `profile_id` is the unresolved stable reference.
/// `route` 持有精确协议路由，`profile_kind` 分类声明族，`profile_id` 是未解析的稳定引用。
///
/// # Returns
/// 返回值
/// A deterministic validation error that prevents a partial immutable runtime snapshot.
/// 返回一条确定性验证错误，阻止产生部分不可变运行时快照。
fn unresolved_route_profile_error(
    route: &super::ManifestProtocolRoute,
    profile_kind: &str,
    profile_id: &str,
) -> super::ManifestValidationError {
    super::ManifestValidationError::new(
        "compiler_unresolved_route_profile",
        format!(
            "$.protocol_routes[{}].{}_profile_refs",
            route.protocol_profile_id, profile_kind
        ),
        format!(
            "protocol route `{}` references {profile_kind} profile `{profile_id}` without a compiled payload",
            route.protocol_profile_id
        ),
    )
}

/// Reports whether one model is enabled by default under the manifest rules.
/// 报告一个模型在 Manifest 规则下是否默认启用。
///
/// # Parameters
/// 参数
/// `resolved` is the compiled manifest; service, credential, and model identifiers select the
/// exact rule scope.
/// `resolved` 是已编译清单；服务、凭据与模型标识共同选择精确规则作用域。
///
/// # Returns
/// 返回值
/// The matching rule's inherited `default_enabled`, or the provider-level default when no
/// rule matches the model.
/// 命中规则时返回其继承的 `default_enabled`；无规则匹配时返回供应商级默认。
pub fn manifest_default_enabled(
    resolved: &ResolvedProviderManifest,
    service_entry_id: &str,
    credential_profile_id: Option<&str>,
    model_id: &str,
) -> bool {
    resolved
        .rule_for_model(service_entry_id, credential_profile_id, model_id)
        .map(|instance| instance.default_enabled)
        .unwrap_or(resolved.default_enabled)
}

/// Compiles one validated manifest document from text.
/// 从文本编译一份已验证清单文档。
///
/// # Parameters
/// 参数
/// `text` is the exact UTF-8 document; `ownership` selects system or custom projection.
/// `text` 是精确 UTF-8 文档；`ownership` 选择系统或自定义投影。
pub fn compile_manifest_text(
    text: &str,
    ownership: ManifestOwnership,
) -> Result<ResolvedProviderManifest, super::ManifestValidationError> {
    compile_manifest_text_with_layer(text, ownership, default_source_layer(ownership))
}

/// Compiles one validated manifest document from text with an explicit source layer.
/// 以显式来源层从文本编译一份已验证清单文档。
///
/// # Parameters
/// 参数
/// `text` is the exact UTF-8 document; `ownership` selects system or custom projection;
/// `source_layer` records the exact Bundled/Active/Previous/Custom layer.
/// `text` 是精确 UTF-8 文档；`ownership` 选择系统或自定义投影；`source_layer` 记录精确的
/// Bundled/Active/Previous/Custom 层。
pub fn compile_manifest_text_with_layer(
    text: &str,
    ownership: ManifestOwnership,
    source_layer: super::ManifestSourceLayer,
) -> Result<ResolvedProviderManifest, super::ManifestValidationError> {
    let registry = EnginePrimitiveRegistry::current_protocol_baseline();
    let manifest =
        super::parse_and_validate(text, ownership, &registry).map_err(|mut errors| {
            errors.drain(..).next().unwrap_or_else(|| {
                super::ManifestValidationError::new(
                    "manifest_validation_failed",
                    "$",
                    "manifest failed validation without a specific error",
                )
            })
        })?;
    let mut resolved = compile_manifest_with_layer(&manifest, ownership, source_layer)?;
    // Snapshot provenance carries the content-addressed digest of the exact source bytes so
    // consumers can prove which revision their binding came from (plan §15.3).
    // 快照来源携带精确源字节的内容寻址摘要，使消费方能够证明其绑定来自哪个修订（方案 §15.3）。
    {
        use sha2::Digest;
        let sha256 = hex::encode(sha2::Sha256::digest(text.as_bytes()));
        resolved.snapshot_provenance.sha256 = Some(sha256.clone());
        for provenance in resolved.declaration_provenance.values_mut() {
            provenance.sha256 = Some(sha256.clone());
        }
    }
    Ok(resolved)
}

/// Loads and compiles every bundled system manifest from the effective resources directory.
/// 从生效资源目录加载并编译全部内置系统清单。
///
/// Missing directories or invalid files fail fast so the runtime never starts with a partial
/// installed trust baseline.
/// 缺失目录或无效文件会快速失败，使运行时绝不以不完整的已安装信任基线启动。
pub fn load_bundled_resolved_manifests()
-> BTreeMap<String, std::sync::Arc<ResolvedProviderManifest>> {
    let index_path = vulcan_runtime_assets::resolve_bundled_provider_manifest_index_path();
    let directory = vulcan_runtime_assets::resolve_bundled_provider_manifests_directory();
    load_bundled_resolved_manifests_from_paths(&index_path, &directory).unwrap_or_else(|error| {
        panic!("bundled provider manifest integrity verification failed: {error}")
    })
}

/// Verifies the source-tree bundled integrity index against its complete manifest generation.
/// 对源码树完整 Manifest 代际校验内置完整性 Index。
///
/// # Returns
/// 返回值
/// The exact indexed provider count after full verification, or the first closed integrity or
/// compilation failure.
/// 完整校验后的精确 Index 供应商数量，或首个封闭的完整性/编译失败。
pub fn verify_source_bundled_manifest_integrity_index() -> Result<usize, String> {
    let index_path = vulcan_runtime_assets::source_bundled_provider_manifest_index_path();
    let directory = vulcan_runtime_assets::source_bundled_provider_manifests_directory();
    load_bundled_resolved_manifests_from_paths(&index_path, &directory)
        .map(|manifests| manifests.len())
}

/// Reads the exact packaged bytes for one stable bundled provider identity under the runtime cap.
/// 在运行时上限内读取一个稳定内置供应商身份对应的精确打包字节。
///
/// # Parameters
/// 参数
/// `provider_id` is the already resolved system-provider identity used as the packaged file stem.
/// `provider_id` 是已解析并用作打包文件主干名的系统供应商身份。
///
/// # Returns
/// 返回值
/// Non-empty bounded bytes from the exact bundled file, or a controlled identity/type/I/O/size
/// failure.
/// 返回精确内置文件中的非空有界字节，或受控的身份、类型、I/O、大小失败。
pub fn read_bundled_provider_manifest_bytes(provider_id: &str) -> Result<Vec<u8>, String> {
    let index_path = vulcan_runtime_assets::resolve_bundled_provider_manifest_index_path();
    let index = read_bundled_manifest_index(&index_path)?;
    let indexed = index.providers.get(provider_id).ok_or_else(|| {
        format!("Bundled provider `{provider_id}` is absent from the packaged integrity index.")
    })?;
    validate_bundled_manifest_index_entry(provider_id, indexed)?;
    let path = vulcan_runtime_assets::resolve_bundled_provider_manifests_directory()
        .join(format!("{provider_id}.json"));
    let bytes = read_bounded_bundled_file(
        &path,
        BUNDLED_MANIFEST_MAX_BYTES,
        "bundled provider manifest",
    )?;
    if bytes.len() as u64 != indexed.bytes {
        return Err(format!(
            "Bundled provider manifest `{provider_id}` declares {} indexed bytes but contains {}.",
            indexed.bytes,
            bytes.len()
        ));
    }
    let actual_sha256 = hex::encode(Sha256::digest(&bytes));
    if actual_sha256 != indexed.sha256 {
        return Err(format!(
            "Bundled provider manifest `{provider_id}` has SHA-256 `{actual_sha256}` instead of the packaged integrity digest `{}`.",
            indexed.sha256
        ));
    }
    Ok(bytes)
}

/// Opens and reads one packaged Manifest artifact without crossing its exact byte boundary.
/// 打开并读取一个打包 Manifest 工件，且不越过其精确字节边界。
///
/// # Parameters
/// 参数
/// `path` is the exact packaged file, `max_bytes` is its hard limit, and `kind` scopes errors.
/// `path` 是精确打包文件，`max_bytes` 是其硬上限，`kind` 限定错误范围。
///
/// # Returns
/// 返回值
/// Non-empty bytes from a regular file, or a controlled type, I/O, or size failure.
/// 返回常规文件中的非空字节，或受控的类型、I/O 与大小失败。
fn read_bounded_bundled_file(path: &Path, max_bytes: u64, kind: &str) -> Result<Vec<u8>, String> {
    let metadata = std::fs::symlink_metadata(path)
        .map_err(|error| format!("{kind} `{}` is unavailable: {error}", path.display()))?;
    if !metadata.file_type().is_file() {
        return Err(format!("{kind} `{}` is not a regular file", path.display()));
    }
    if metadata.len() == 0 || metadata.len() > max_bytes {
        return Err(format!(
            "{kind} `{}` must contain 1..={max_bytes} bytes",
            path.display()
        ));
    }
    let file = std::fs::File::open(path)
        .map_err(|error| format!("{kind} `{}` could not be opened: {error}", path.display()))?;
    let mut limited = file.take(max_bytes.saturating_add(1));
    let mut bytes = Vec::with_capacity(metadata.len().min(max_bytes) as usize);
    limited
        .read_to_end(&mut bytes)
        .map_err(|error| format!("{kind} `{}` could not be read: {error}", path.display()))?;
    if bytes.is_empty() || bytes.len() as u64 > max_bytes {
        return Err(format!(
            "{kind} `{}` changed while reading and must contain 1..={max_bytes} bytes",
            path.display()
        ));
    }
    Ok(bytes)
}

/// Reads and validates one packaged provider-manifest integrity index.
/// 读取并校验一份打包供应商 Manifest 完整性 Index。
///
/// # Parameters
/// 参数
/// `index_path` is the exact packaged index path selected by the runtime asset resolver.
/// `index_path` 是运行时资源解析器选出的精确打包 Index 路径。
///
/// # Returns
/// 返回值
/// A bounded closed index only after syntax, structure, depth, count, and header validation.
/// 仅在语法、结构、深度、数量与头部校验后返回有界封闭 Index。
fn read_bundled_manifest_index(index_path: &Path) -> Result<BundledManifestIndex, String> {
    let index_bytes = read_bounded_bundled_file(
        index_path,
        BUNDLED_MANIFEST_INDEX_MAX_BYTES,
        "bundled provider manifest index",
    )?;
    let index_value: serde_json::Value = serde_json::from_slice(&index_bytes).map_err(|error| {
        format!(
            "bundled provider manifest index `{}` is invalid JSON: {error}",
            index_path.display()
        )
    })?;
    super::validate_runtime_json_bounds(&index_value, "Bundled provider manifest index")?;
    let index: BundledManifestIndex = serde_json::from_value(index_value).map_err(|error| {
        format!(
            "bundled provider manifest index `{}` has an invalid structure: {error}",
            index_path.display()
        )
    })?;
    validate_bundled_manifest_index_header(&index)?;
    Ok(index)
}

/// Verifies one packaged integrity index and compiles its complete manifest generation.
/// 校验一份打包完整性 Index，并编译其完整 Manifest 代际。
///
/// # Parameters
/// 参数
/// `index_path` is the exact packaged index file and `directory` is its manifest directory.
/// `index_path` 是精确打包 Index 文件；`directory` 是对应 Manifest 目录。
///
/// # Returns
/// 返回值
/// A complete immutable catalog only after file-set, byte, digest, revision, identity, and engine
/// contract checks all pass; otherwise the first closed failure without any partial result.
/// 仅当文件集合、字节、摘要、修订、身份与引擎契约检查全部通过后返回完整不可变目录；
/// 否则返回首个封闭失败且不产生任何部分结果。
fn load_bundled_resolved_manifests_from_paths(
    index_path: &Path,
    directory: &Path,
) -> Result<BTreeMap<String, std::sync::Arc<ResolvedProviderManifest>>, String> {
    let loaded = load_bundled_manifest_generation_from_paths(index_path, directory)?;
    if let Some((provider_id, failure)) = loaded.failures.iter().next() {
        return Err(format!(
            "bundled provider manifest `{provider_id}` failed provider-local verification: {}",
            failure.message
        ));
    }
    Ok(loaded.manifests)
}

/// Loads one bundled generation after global index facts pass, isolating provider-local failures.
/// 在全局 Index 事实通过后加载一份内置代际，并隔离逐供应商失败。
///
/// # Parameters
/// 参数
/// `index_path` identifies the trusted identity index and `directory` owns its exact provider
/// file set.
/// `index_path` 标识可信身份 Index，`directory` 持有其精确供应商文件集合。
///
/// # Returns
/// 返回值
/// Executable manifests and provider-local failures, or a global index/directory trust error.
/// 返回可执行 Manifest 与逐供应商失败，或全局 Index/目录信任错误。
fn load_bundled_manifest_generation_from_paths(
    index_path: &Path,
    directory: &Path,
) -> Result<BundledManifestGenerationLoad, String> {
    let index = read_bundled_manifest_index(index_path)?;

    // Every index identity is validated before any value can become a filesystem path. An invalid
    // identity makes the index itself untrustworthy and therefore cannot be provider-isolated.
    // 每个 Index 身份都在成为文件系统路径前校验。非法身份意味着 Index 本身不可信，
    // 因而不能按供应商隔离。
    for (provider_id, indexed) in &index.providers {
        validate_bundled_manifest_index_entry(provider_id, indexed)?;
    }
    // expectedFiles is derived only from validated identities before directory enumeration.
    // expectedFiles 在目录枚举前仅由已校验身份派生。
    let expected_files = index
        .providers
        .keys()
        .map(|provider_id| format!("{provider_id}.json"))
        .collect::<BTreeSet<_>>();

    let entries = std::fs::read_dir(directory).map_err(|error| {
        format!(
            "bundled provider manifest directory `{}` is unavailable: {error}",
            directory.display()
        )
    })?;
    // actualFiles records exact JSON names before provider-local type checks compile documents.
    // actualFiles 在逐供应商类型检查与文档编译前记录精确 JSON 名称。
    let mut actual_files = BTreeSet::new();
    // scannedEntries also bounds ignored non-JSON entries in a tampered installation directory.
    // scannedEntries 同样限制被篡改安装目录中被忽略的非 JSON 条目。
    let mut scanned_entries = 0usize;
    for entry in entries {
        scanned_entries = scanned_entries.saturating_add(1);
        if scanned_entries > BUNDLED_MANIFEST_DIRECTORY_MAX_ENTRIES {
            return Err(format!(
                "bundled provider manifest directory `{}` exceeds the {BUNDLED_MANIFEST_DIRECTORY_MAX_ENTRIES}-entry scan limit",
                directory.display()
            ));
        }
        let entry = entry.map_err(|error| {
            format!(
                "bundled provider manifest directory `{}` contains an unreadable entry: {error}",
                directory.display()
            )
        })?;
        let path = entry.path();
        if path.extension().and_then(|extension| extension.to_str()) != Some("json") {
            continue;
        }
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| {
                format!(
                    "bundled provider manifest entry `{}` has no UTF-8 file name",
                    path.display()
                )
            })?;
        actual_files.insert(file_name.to_string());
        if actual_files.len() > BUNDLED_MANIFEST_INDEX_MAX_PROVIDERS {
            return Err(format!(
                "bundled provider manifest directory `{}` exceeds the {BUNDLED_MANIFEST_INDEX_MAX_PROVIDERS}-manifest limit",
                directory.display()
            ));
        }
    }

    // expectedFiles is derived only from validated identities. Unexpected JSON files are a global
    // directory integrity failure; missing indexed files remain attributable to one provider.
    // expectedFiles 仅由已校验身份派生。意外 JSON 文件属于全局目录完整性失败；缺失的
    // 已索引文件仍可归因到单个供应商。
    if let Some(unexpected) = actual_files.difference(&expected_files).next() {
        return Err(format!(
            "bundled provider manifest directory `{}` contains unexpected indexed-format file `{unexpected}`",
            directory.display()
        ));
    }

    let mut manifests = BTreeMap::new();
    let mut failures = BTreeMap::new();
    for (provider_id, indexed) in &index.providers {
        match load_indexed_bundled_provider_manifest(directory, provider_id, indexed) {
            Ok(resolved) => {
                manifests.insert(provider_id.clone(), std::sync::Arc::new(resolved));
            }
            Err(message) => {
                failures.insert(
                    provider_id.clone(),
                    BundledProviderManifestFailure {
                        failure_kind: String::from("bundled_baseline_invalid"),
                        message: sanitize_bundled_failure_diagnostic(message, directory),
                    },
                );
            }
        }
    }
    Ok(BundledManifestGenerationLoad {
        manifests,
        failures,
    })
}

/// Loads, authenticates, compiles, and cross-checks one indexed bundled provider file.
/// 加载、认证、编译并交叉校验一份已索引内置供应商文件。
///
/// # Parameters
/// 参数
/// `directory` is the validated fixed manifest root, `provider_id` is a validated stable identity,
/// and `indexed` is its trusted integrity pin.
/// `directory` 是已校验固定 Manifest 根目录，`provider_id` 是已校验稳定身份，`indexed`
/// 是其可信完整性固定值。
///
/// # Returns
/// 返回值
/// One executable compiled manifest or a provider-local bounded-input diagnostic.
/// 返回一份可执行已编译 Manifest，或逐供应商有界输入诊断。
fn load_indexed_bundled_provider_manifest(
    directory: &Path,
    provider_id: &str,
    indexed: &BundledManifestIndexEntry,
) -> Result<ResolvedProviderManifest, String> {
    let path = directory.join(format!("{provider_id}.json"));
    let bytes = read_bounded_bundled_file(
        &path,
        BUNDLED_MANIFEST_MAX_BYTES,
        "indexed bundled provider manifest",
    )?;
    if bytes.len() as u64 != indexed.bytes {
        return Err(format!(
            "indexed bundled provider manifest `{provider_id}` declares {} bytes but contains {}",
            indexed.bytes,
            bytes.len()
        ));
    }
    let actual_sha256 = hex::encode(Sha256::digest(&bytes));
    if actual_sha256 != indexed.sha256 {
        return Err(format!(
            "indexed bundled provider manifest `{provider_id}` has SHA-256 `{actual_sha256}` instead of `{}`",
            indexed.sha256
        ));
    }
    let text = std::str::from_utf8(&bytes).map_err(|error| {
        format!("indexed bundled provider manifest `{provider_id}` is not UTF-8: {error}")
    })?;
    let resolved = compile_manifest_text(text, ManifestOwnership::System).map_err(|error| {
        format!("bundled provider manifest `{provider_id}` is invalid: {error}")
    })?;
    validate_compiled_bundled_manifest(provider_id, indexed, &resolved)?;
    Ok(resolved)
}

/// Truncates one local diagnostic at a valid UTF-8 boundary under the status safety ceiling.
/// 在状态安全上限内按合法 UTF-8 边界截断一条本地诊断。
///
/// # Parameters
/// 参数
/// `message` is a locally generated provider-file failure and never contains manifest bytes.
/// `message` 是本地产生的供应商文件失败，绝不包含 Manifest 字节。
///
/// # Returns
/// 返回值
/// A non-empty diagnostic containing at most `BUNDLED_MANIFEST_FAILURE_MAX_BYTES` bytes.
/// 返回非空且不超过 `BUNDLED_MANIFEST_FAILURE_MAX_BYTES` 字节的诊断。
fn truncate_bundled_failure_diagnostic(mut message: String) -> String {
    if message.len() <= BUNDLED_MANIFEST_FAILURE_MAX_BYTES {
        return message;
    }
    let mut boundary = BUNDLED_MANIFEST_FAILURE_MAX_BYTES;
    while !message.is_char_boundary(boundary) {
        boundary = boundary.saturating_sub(1);
    }
    message.truncate(boundary);
    message
}

/// Removes the local resource root and provider-controlled URL/credential shapes from a failure.
/// 从失败中移除本地资源根以及供应商控制的 URL/凭据形态。
///
/// # Parameters
/// 参数
/// `message` is the exact local verification error and `directory` is the fixed resource root
/// whose machine-specific path must not enter status DTOs.
/// `message` 是精确本地校验错误，`directory` 是不得进入状态 DTO 的机器相关固定资源根路径。
///
/// # Returns
/// 返回值
/// A bounded operator diagnostic with stable redaction markers.
/// 返回带稳定脱敏标记的有界运维诊断。
fn sanitize_bundled_failure_diagnostic(message: String, directory: &Path) -> String {
    // directoryText is the only machine-specific path introduced by provider-local file reads.
    // directoryText 是逐供应商文件读取引入的唯一机器相关路径。
    let directory_text = directory.to_string_lossy();
    // withoutDirectory retains the file identity while replacing the absolute resource root.
    // withoutDirectory 保留文件身份，同时替换绝对资源根。
    let without_directory = message.replace(
        directory_text.as_ref(),
        "<bundled-provider-manifest-directory>",
    );
    truncate_bundled_failure_diagnostic(
        crate::provider_tool_execution::sanitize_provider_diagnostic_text(
            without_directory.as_str(),
        ),
    )
}

/// Validates the bounded, non-expiring header of one packaged integrity index.
/// 校验一份打包完整性 Index 的有界、不失效头部。
fn validate_bundled_manifest_index_header(index: &BundledManifestIndex) -> Result<(), String> {
    if index.schema_version != BUNDLED_MANIFEST_INDEX_SCHEMA_VERSION {
        return Err(format!(
            "bundled provider manifest index schema `{}` is unsupported",
            index.schema_version
        ));
    }
    if index.index_revision == 0 {
        return Err(String::from(
            "bundled provider manifest index revision must be positive",
        ));
    }
    if index.issued_at_unix_ms < 0 || index.expires_at_unix_ms <= index.issued_at_unix_ms {
        return Err(String::from(
            "bundled provider manifest index time window is invalid",
        ));
    }
    if index.signing_key_id.is_empty() || index.signing_key_id.len() > 128 {
        return Err(String::from(
            "bundled provider manifest index authority id must contain 1..=128 UTF-8 bytes",
        ));
    }
    if index.providers.is_empty() || index.providers.len() > BUNDLED_MANIFEST_INDEX_MAX_PROVIDERS {
        return Err(format!(
            "bundled provider manifest index must contain 1..={BUNDLED_MANIFEST_INDEX_MAX_PROVIDERS} providers"
        ));
    }
    Ok(())
}

/// Validates one index entry before its provider identity is used to construct a local path.
/// 在使用供应商身份构造本地路径前校验一条 Index 记录。
fn validate_bundled_manifest_index_entry(
    provider_id: &str,
    entry: &BundledManifestIndexEntry,
) -> Result<(), String> {
    validate_bundled_provider_identity(provider_id)?;
    if entry.provider_id != provider_id {
        return Err(format!(
            "bundled provider manifest index key `{provider_id}` disagrees with entry provider `{}`",
            entry.provider_id
        ));
    }
    if entry.revision == 0 || entry.bytes == 0 || entry.bytes > BUNDLED_MANIFEST_MAX_BYTES {
        return Err(format!(
            "bundled provider manifest index entry `{provider_id}` has invalid revision or byte length"
        ));
    }
    let expected_relative_url = format!("manifests/{provider_id}.json");
    if entry.relative_url != expected_relative_url {
        return Err(format!(
            "bundled provider manifest index entry `{provider_id}` must use relative path `{expected_relative_url}`"
        ));
    }
    if entry.sha256.len() != 64
        || !entry
            .sha256
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
    {
        return Err(format!(
            "bundled provider manifest index entry `{provider_id}` has an invalid lowercase SHA-256 digest"
        ));
    }
    if entry.minimum_engine_contract == 0 || entry.minimum_engine_contract > u32::MAX as u64 {
        return Err(format!(
            "bundled provider manifest index entry `{provider_id}` has an invalid engine contract"
        ));
    }
    Ok(())
}

/// Validates one stable bundled provider identity independently of an index entry payload.
/// 独立于 Index 条目载荷校验一个稳定内置供应商身份。
///
/// # Parameters
/// 参数
/// `provider_id` is a map key that may later become a local file stem.
/// `provider_id` 是之后可能成为本地文件主干名的映射键。
///
/// # Returns
/// 返回值
/// Success only for a bounded path-safe ASCII identity.
/// 仅对有界且路径安全的 ASCII 身份返回成功。
fn validate_bundled_provider_identity(provider_id: &str) -> Result<(), String> {
    if provider_id.is_empty()
        || provider_id.len() > MAX_PROVIDER_IDENTIFIER_LEN
        || !provider_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        return Err(format!(
            "bundled provider manifest index key `{provider_id}` is not a stable system provider id within the {MAX_PROVIDER_IDENTIFIER_LEN}-byte filesystem-safe limit"
        ));
    }
    Ok(())
}

/// Confirms compiled manifest facts exactly equal their packaged integrity pin.
/// 确认已编译 Manifest 事实与打包完整性固定值完全一致。
fn validate_compiled_bundled_manifest(
    provider_id: &str,
    indexed: &BundledManifestIndexEntry,
    resolved: &ResolvedProviderManifest,
) -> Result<(), String> {
    if resolved.descriptor.id != provider_id {
        return Err(format!(
            "bundled provider manifest index key `{provider_id}` resolved provider `{}`",
            resolved.descriptor.id
        ));
    }
    if resolved.revision != indexed.revision {
        return Err(format!(
            "bundled provider manifest `{provider_id}` revision {} disagrees with index revision {}",
            resolved.revision, indexed.revision
        ));
    }
    if u64::from(resolved.engine_contract.minimum_manifest_engine)
        != indexed.minimum_engine_contract
    {
        return Err(format!(
            "bundled provider manifest `{provider_id}` engine contract {} disagrees with index contract {}",
            resolved.engine_contract.minimum_manifest_engine, indexed.minimum_engine_contract
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies packaged provider identities cannot exceed the cross-platform file bound.
    /// 验证打包供应商身份不能超过跨平台文件边界。
    #[test]
    fn bundled_provider_identity_rejects_filesystem_unsafe_length() {
        // providerId is one byte beyond the shared stable provider-identity ceiling.
        // providerId 比共享稳定供应商身份上限多一个字节。
        let provider_id = "a".repeat(MAX_PROVIDER_IDENTIFIER_LEN + 1);
        let error = validate_bundled_provider_identity(provider_id.as_str())
            .expect_err("oversized bundled identity must fail before path construction");
        assert!(error.contains("filesystem-safe limit"), "{error}");
    }

    /// Loads one real bundled manifest for compiler-invariant tests.
    /// 为编译器不变量测试加载一份真实内置 Manifest。
    ///
    /// # Returns
    /// 返回值
    /// The parsed DeepSeek manifest from the source asset tree.
    /// 返回从源码资源树解析出的 DeepSeek Manifest。
    fn bundled_deepseek_manifest() -> ProviderManifestV1 {
        let path = vulcan_runtime_assets::source_bundled_provider_manifests_directory()
            .join("deepseek.json");
        let text = std::fs::read_to_string(path).expect("DeepSeek manifest must be readable");
        serde_json::from_str(text.as_str()).expect("DeepSeek manifest must parse")
    }

    /// Verifies the compiler itself rejects an unresolved route profile instead of dropping it.
    /// 验证编译器自身拒绝未解析的路由 Profile，而不是将其静默丢弃。
    #[test]
    fn protocol_route_compiler_fails_closed_on_missing_profile_payload() {
        // route is a valid protocol declaration deliberately given one unresolved request ref.
        // route 是一条有效协议声明，并被刻意加入一个未解析的请求引用。
        let manifest = bundled_deepseek_manifest();
        let mut route = manifest.services[0].protocol_routes[0].clone();
        route.request_profile_refs = vec![String::from("missing.request.v1")];
        let error = compile_protocol_route(
            &route,
            &manifest.usage_profiles,
            &manifest.request_profiles,
            &manifest.response_profiles,
        )
        .expect_err("the compiler must reject a missing profile payload");
        assert_eq!(error.code, "compiler_unresolved_route_profile");
        assert!(error.path.ends_with(".request_profile_refs"));
    }

    /// Verifies a compiled snapshot freezes the exact validated engine contract.
    /// 验证编译快照冻结精确的已验证引擎契约。
    #[test]
    fn compiled_snapshot_freezes_engine_contract() {
        let manifest = bundled_deepseek_manifest();
        let resolved = compile_manifest(&manifest, ManifestOwnership::System)
            .expect("bundled manifest must compile");
        assert_eq!(resolved.engine_contract, manifest.engine_contract);
        assert_eq!(resolved.manifest_provenance, manifest.provenance);
        for defaults in resolved.entry_model_defaults.values() {
            assert_eq!(
                defaults.provenance.provenance_ref,
                manifest.model_defaults.provenance_ref
            );
            assert_eq!(
                defaults.provenance.provenance_status,
                manifest.provenance.status
            );
        }
        let resolved_rule = resolved
            .rule_instances
            .first()
            .expect("DeepSeek must compile at least one reviewed rule");
        let source_rule = manifest
            .model_rules
            .iter()
            .find(|rule| rule.rule_id == resolved_rule.rule_id)
            .expect("the resolved rule must retain its source identity");
        assert_eq!(
            resolved_rule.provenance.provenance_ref,
            source_rule.provenance_ref
        );
        assert_eq!(
            resolved_rule.provenance.provenance_status,
            manifest.provenance.status
        );
    }

    /// Verifies one corrupted indexed provider is isolated while unrelated providers still load.
    /// 验证一个损坏的已索引供应商会被隔离，而无关供应商仍可加载。
    #[test]
    fn bundled_generation_isolates_one_provider_local_failure() {
        // sourceDirectory owns the complete checked-in provider generation used as the fixture.
        // sourceDirectory 持有作为夹具使用的完整检入供应商代际。
        let source_directory = vulcan_runtime_assets::source_bundled_provider_manifests_directory();
        // sourceIndex is the globally trusted identity and digest baseline copied unchanged.
        // sourceIndex 是未经修改复制的全局可信身份与摘要基线。
        let source_index = vulcan_runtime_assets::source_bundled_provider_manifest_index_path();
        // temporaryRoot prevents the corruption fixture from touching checked-in resources.
        // temporaryRoot 防止损坏夹具触碰检入资源。
        let temporary_root = tempfile::tempdir().expect("temporary directory must create");
        // fixtureDirectory mirrors the exact indexed JSON file set.
        // fixtureDirectory 镜像精确的已索引 JSON 文件集合。
        let fixture_directory = temporary_root.path().join("manifests");
        std::fs::create_dir_all(&fixture_directory).expect("fixture directory must create");
        for entry in std::fs::read_dir(&source_directory).expect("source directory must read") {
            // entry is one checked-in regular provider Manifest copied byte-for-byte.
            // entry 是一份按字节复制的检入常规供应商 Manifest。
            let entry = entry.expect("source entry must read");
            if entry.path().extension().and_then(|value| value.to_str()) == Some("json") {
                std::fs::copy(entry.path(), fixture_directory.join(entry.file_name()))
                    .expect("provider fixture must copy");
            }
        }
        // fixtureIndex remains valid so only the tampered provider file is attributable locally.
        // fixtureIndex 保持有效，使只有被篡改供应商文件可逐项归因。
        let fixture_index = temporary_root.path().join("bundled-index.json");
        std::fs::copy(source_index, &fixture_index).expect("index fixture must copy");
        // missingProvider removes one provider without changing its trusted index identity.
        // missingProvider 在不改变可信 Index 身份的情况下移除一个供应商。
        std::fs::remove_file(fixture_directory.join("deepseek.json"))
            .expect("provider corruption fixture must remove");
        // indexedProviderCount proves the tolerant result retains every trusted identity.
        // indexedProviderCount 证明容错结果保留每个可信身份。
        let indexed_provider_count = read_bundled_manifest_index(&fixture_index)
            .expect("fixture index must remain valid")
            .providers
            .len();

        // loaded is a complete identity result with one non-executable provider-local failure.
        // loaded 是包含一个不可执行逐供应商失败的完整身份结果。
        let loaded =
            load_bundled_manifest_generation_from_paths(&fixture_index, &fixture_directory)
                .expect("provider-local corruption must not fail the global generation");
        assert!(!loaded.manifests.contains_key("deepseek"));
        assert!(loaded.manifests.contains_key("openrouter"));
        assert_eq!(loaded.failures.len(), 1);
        assert_eq!(
            loaded
                .failures
                .get("deepseek")
                .map(|failure| failure.failure_kind.as_str()),
            Some("bundled_baseline_invalid")
        );
        assert!(
            loaded
                .failures
                .get("deepseek")
                .is_some_and(|failure| !failure
                    .message
                    .contains(temporary_root.path().to_string_lossy().as_ref())),
            "provider status diagnostics must redact the machine-specific resource root"
        );
        assert_eq!(
            loaded.manifests.len() + loaded.failures.len(),
            indexed_provider_count
        );
        assert!(
            load_bundled_resolved_manifests_from_paths(&fixture_index, &fixture_directory)
                .expect_err("strict release verification must reject any isolated provider")
                .contains("deepseek")
        );
    }

    /// Verifies an unexpected JSON file remains a global directory-integrity failure.
    /// 验证意外 JSON 文件仍属于全局目录完整性失败。
    #[test]
    fn bundled_generation_rejects_unindexed_json_file_globally() {
        // sourceDirectory and sourceIndex provide the complete trusted checked-in fixture.
        // sourceDirectory 与 sourceIndex 提供完整可信的检入夹具。
        let source_directory = vulcan_runtime_assets::source_bundled_provider_manifests_directory();
        let source_index = vulcan_runtime_assets::source_bundled_provider_manifest_index_path();
        // temporaryRoot contains the isolated copied generation.
        // temporaryRoot 包含隔离复制的代际。
        let temporary_root = tempfile::tempdir().expect("temporary directory must create");
        let fixture_directory = temporary_root.path().join("manifests");
        std::fs::create_dir_all(&fixture_directory).expect("fixture directory must create");
        for entry in std::fs::read_dir(&source_directory).expect("source directory must read") {
            let entry = entry.expect("source entry must read");
            if entry.path().extension().and_then(|value| value.to_str()) == Some("json") {
                std::fs::copy(entry.path(), fixture_directory.join(entry.file_name()))
                    .expect("provider fixture must copy");
            }
        }
        let fixture_index = temporary_root.path().join("bundled-index.json");
        std::fs::copy(source_index, &fixture_index).expect("index fixture must copy");
        // unexpectedFile has no trusted index identity and therefore cannot be isolated safely.
        // unexpectedFile 没有可信 Index 身份，因此不能被安全地逐供应商隔离。
        std::fs::write(fixture_directory.join("unexpected.json"), b"{}")
            .expect("unexpected fixture must write");
        let error = load_bundled_manifest_generation_from_paths(&fixture_index, &fixture_directory)
            .expect_err("unindexed file must fail the global generation");
        assert!(error.contains("unexpected indexed-format file"), "{error}");
    }
}
