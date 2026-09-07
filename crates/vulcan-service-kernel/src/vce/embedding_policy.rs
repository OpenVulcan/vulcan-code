//! Candidate inspection and direct, project-scoped embedding compatibility decisions.
//! 候选检查与直接、项目作用域的向量兼容决定。

use serde::Serialize;
use std::collections::BTreeSet;
use vulcan_ai_contract::{
    AiOperation, EmbeddingCapabilities, ExecutionRouteSnapshot, InferenceUsageScope,
};
use vulcan_provider_adapters::{
    ContextEngineEmbeddingCompatibilityRecord, ContextEngineEmbeddingDecision,
    ContextEngineEmbeddingScope, InferenceRouteConfiguration, PersistedAiConfiguration,
    PublicAiRuntime,
};
use vulcan_workspace_store::vce_data_plane::VceEmbeddingProvenance;

use super::ai_port::candidate_embedding_fingerprint;
use super::embedding_dimensions::{
    preferred_embedding_dimension, validate_explicit_embedding_dimension,
};
use super::manager::{VCE_CHUNKER_VERSION, VCE_CONSUMER_PROFILE_ID, VCE_PREPROCESSING_VERSION};

/// A resolved candidate that has not been saved, registered, or sent to a provider.
/// 已解析但未保存、未注册、未发送给供应商的候选。
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct VceEmbeddingCandidate {
    /// Exact shared-registry route selected by the user.
    /// 用户选择的精确共享注册表路由。
    pub(crate) route: InferenceRouteConfiguration,
    /// Non-secret execution evidence resolved by the common inference service.
    /// 公共推理服务解析的非敏感执行证据。
    pub(crate) execution: ExecutionRouteSnapshot,
    /// Declared dimension choices and vector limits, never inferred from a test response.
    /// 声明的维度选择与向量限制，绝不从测试响应推断。
    pub(crate) capabilities: EmbeddingCapabilities,
    /// Explicit saved or suggested dimension; a suggestion is not automatically persisted.
    /// 明确的已保存或建议维度；建议不会自动持久化。
    pub(crate) dimension: u32,
    /// Actual target identity, distinct from the original stored-space identity.
    /// 实际目标身份，与原始已存空间身份分开。
    pub(crate) physical_fingerprint: String,
}

/// Project-scoped preflight result that never substitutes compatibility for a hard constraint.
/// 项目作用域预检结果，绝不使用兼容判断代替硬性约束。
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct VceEmbeddingProjectImpact {
    /// Original exact index identity reviewed by the user.
    /// 用户审查的原始精确索引身份。
    pub(crate) scope: ContextEngineEmbeddingScope,
    /// Exact committed checkpoint inspected with the current source-membership set.
    /// 与当前来源成员集合一起检查的精确已提交检查点。
    pub(crate) checkpoint_version: u64,
    /// Every origin still represented, including the immutable original index identity.
    /// 仍然存在的全部来源，包括不可变原始索引身份。
    pub(crate) source_fingerprints: BTreeSet<String>,
    /// Sources needing a direct new approval; a prior source-to-intermediate approval is insufficient.
    /// 需要直接新批准的来源；既有来源到中间目标的批准不充分。
    pub(crate) confirmation_required: BTreeSet<String>,
    /// Sources for which the user explicitly chose rebuilding under this exact target.
    /// 用户针对当前精确目标明确选择重建的来源。
    pub(crate) rejected_sources: BTreeSet<String>,
    /// Non-overridable incompatibility such as dimensions, preprocessing, format, or project root.
    /// 维度、预处理、格式或项目根等不可覆盖的不兼容原因。
    pub(crate) hard_rebuild_reason: Option<String>,
}

impl VceEmbeddingProjectImpact {
    /// Report compatibility only when every actual source has direct coverage and all hard guards pass.
    /// 仅在每个实际来源均被直接覆盖且全部硬性保护通过时报告兼容。
    /// No parameters. Returns no authorization for missing, rejected, or physically incompatible sources.
    /// 无参数。来源缺失批准、被拒绝或物理不兼容时不返回授权。
    pub(crate) fn compatible(&self) -> bool {
        self.hard_rebuild_reason.is_none()
            && self.confirmation_required.is_empty()
            && self.rejected_sources.is_empty()
    }
}

/// Resolve one explicit embedding choice through the existing shared model/credential registry.
/// 通过现有共享模型及凭据注册表解析一个明确向量选择。
///
/// `runtime` and `configuration` are host snapshots, `route` is the candidate, and `dimension`
/// is user input. An absent input requests the documented UI suggestion while preserving a valid
/// existing VCE dimension. Returns validated metadata only; no registration or model call occurs.
/// `runtime` 与 `configuration` 为宿主快照，`route` 为候选，`dimension` 为用户输入。
/// 输入缺失时按界面规则给出建议，并保留有效的既有 VCE 维度。仅返回已验证元数据；
/// 不发生注册或模型调用。
pub(crate) fn inspect_embedding_candidate(
    runtime: &PublicAiRuntime,
    configuration: &PersistedAiConfiguration,
    route: InferenceRouteConfiguration,
    dimension: Option<u32>,
) -> Result<VceEmbeddingCandidate, String> {
    if route.operation != AiOperation::EmbeddingCreate
        || route.usage_scope != InferenceUsageScope::InternalBackground
    {
        return Err(String::from(
            "ContextEngine requires an internal-background embedding route",
        ));
    }
    // Selection is shared with actual dispatch, including account, model-enabled, and binding checks.
    // 选择与实际分派共享，包括账号、模型启用和绑定检查。
    let selection = runtime.resolve_internal_inference_selection(configuration, &route)?;
    // Strong capabilities are the only source for dimension eligibility.
    // 强类型能力是维度资格的唯一来源。
    let capabilities = selection
        .model
        .capabilities
        .embedding
        .clone()
        .ok_or_else(|| {
            String::from("ContextEngine candidate has no declared embedding contract")
        })?;
    let dimension = match dimension {
        Some(value) => {
            validate_explicit_embedding_dimension(&capabilities, value)?;
            value
        }
        None => preferred_embedding_dimension(
            &capabilities,
            configuration
                .inference_profiles
                .get(VCE_CONSUMER_PROFILE_ID)
                .and_then(|profile| profile.embedding_dimensions),
        )?,
    };
    // Snapshot creation is the common inference implementation, not a VCE-specific route projection.
    // 快照创建使用公共推理实现，而不是 VCE 专属路由投影。
    let execution = crate::configured_inference::configured_route_snapshot(&selection, &route);
    let physical_fingerprint =
        candidate_embedding_fingerprint(execution.clone(), capabilities.clone(), dimension)?;
    Ok(VceEmbeddingCandidate {
        route,
        execution,
        capabilities,
        dimension,
        physical_fingerprint,
    })
}

/// Evaluate exact committed origins against direct human decisions without modifying any identity.
/// 根据直接用户决定评估精确已提交来源，不修改任何身份。
///
/// `provenance` comes from storage; `current_root_hash` is the canonical host root or its diagnostic; `candidate` is
/// validated target metadata; `records` are persisted direct decisions. Returns no impact before
/// the first committed checkpoint, otherwise a complete hard/soft compatibility assessment.
/// `provenance` 来自存储；`current_root_hash` 为规范宿主根或其诊断；`candidate` 为已验证目标元数据；
/// `records` 为持久化直接决定。首个检查点前返回无影响，否则返回完整硬性及可确认兼容评估。
pub(crate) fn inspect_embedding_project_impact(
    provenance: &VceEmbeddingProvenance,
    current_root_hash: Result<&str, &str>,
    candidate: &VceEmbeddingCandidate,
    records: &[ContextEngineEmbeddingCompatibilityRecord],
) -> Result<Option<VceEmbeddingProjectImpact>, String> {
    // A never-initialized store has no published vector space to authorize or rebuild.
    // 从未初始化的存储没有需要授权或重建的已发布向量空间。
    let Some(checkpoint_version) = provenance.checkpoint_version else {
        return Ok(None);
    };
    provenance.original.validate()?;
    if checkpoint_version == 0
        || !provenance
            .source_fingerprints
            .contains(&provenance.original.physical_fingerprint)
    {
        return Err(String::from(
            "ContextEngine provenance omits its original committed identity",
        ));
    }
    // Scope always preserves the original stored provenance, never the candidate route.
    // 作用域始终保留原始存储来源，绝不改成候选路由。
    let scope = ContextEngineEmbeddingScope {
        project_id: provenance.original.project_id.clone(),
        root_hash: provenance.original.root_hash.clone(),
        namespace_id: provenance.original.binding.namespace_id.clone(),
        checkpoint_chain_id: provenance.original.binding.checkpoint_chain_id.clone(),
        original_fingerprint: provenance.original.physical_fingerprint.clone(),
        index_schema_version: provenance.original.binding.schema_version,
        dimension: provenance.original.dimension,
        preprocessing_version: provenance.original.preprocessing_version.clone(),
        chunker_version: provenance.original.chunker_version.clone(),
    };
    scope.validate()?;
    // Hard constraints precede user decisions, so even a forged compatible record cannot waive them.
    // 硬性约束先于用户决定，因此即使伪造兼容记录也不能免除它们。
    let hard_rebuild_reason = if let Err(reason) = current_root_hash {
        Some(reason.to_owned())
    } else if current_root_hash != Ok(scope.root_hash.as_str()) {
        Some(String::from("Project root differs from the original index"))
    } else if scope.dimension != candidate.dimension {
        Some(format!(
            "Embedding dimensions changed from {} to {}",
            scope.dimension, candidate.dimension
        ))
    } else if scope.preprocessing_version != VCE_PREPROCESSING_VERSION
        || scope.chunker_version != VCE_CHUNKER_VERSION
    {
        Some(String::from(
            "Index preprocessing or chunker contract changed",
        ))
    } else if scope.index_schema_version != 1 {
        Some(String::from("Index format requires explicit rebuilding"))
    } else {
        None
    };
    // Duplicate direct keys are ambiguous persisted policy, not a last-write-wins authorization.
    // 重复直接键属于含糊的持久化策略，不是最后写入获胜的授权。
    let mut direct_sources = std::collections::BTreeMap::new();
    for record in records.iter().filter(|record| {
        record.scope == scope && record.target_fingerprint == candidate.physical_fingerprint
    }) {
        record.validate()?;
        if record.target_route != candidate.route
            || record.target_route_revision != candidate.execution.route_revision
            || record.target_capability_revision != candidate.execution.capability_revision
        {
            return Err(String::from(
                "ContextEngine approval target does not match its fingerprint evidence",
            ));
        }
        if direct_sources
            .insert(record.source_fingerprint.as_str(), record.decision)
            .is_some()
        {
            return Err(String::from(
                "ContextEngine contains duplicate direct embedding decisions",
            ));
        }
    }
    // Every actual source is evaluated independently; there is deliberately no graph traversal.
    // 独立评估每个实际来源；有意不执行任何图遍历。
    let mut confirmation_required = BTreeSet::new();
    let mut rejected_sources = BTreeSet::new();
    for source in &provenance.source_fingerprints {
        if source == &candidate.physical_fingerprint {
            continue;
        }
        match direct_sources.get(source.as_str()) {
            Some(ContextEngineEmbeddingDecision::Compatible) => {}
            Some(ContextEngineEmbeddingDecision::Rebuild) => {
                rejected_sources.insert(source.clone());
            }
            None => {
                confirmation_required.insert(source.clone());
            }
        }
    }
    Ok(Some(VceEmbeddingProjectImpact {
        scope,
        checkpoint_version,
        source_fingerprints: provenance.source_fingerprints.clone(),
        confirmation_required,
        rejected_sources,
        hard_rebuild_reason,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};
    use vulcan_ai_contract::{
        DistanceMetric, EmbeddingEncoding, EmbeddingNormalization, EmbeddingOutputKind,
        EmbeddingTask,
    };
    use vulcan_context_engine::{EmbeddingSpace, VceIndexBinding};
    use vulcan_provider_adapters::InferenceRouteBinding;
    use vulcan_workspace_store::vce_data_plane::VceProjectIndexConfig;

    /// Create a tagged test identity from one valid lowercase hex character.
    /// 从一个合法小写十六进制字符创建带标签测试身份。
    /// `character` selects identity; returns a deterministic non-secret fixture digest.
    /// `character` 选择身份；返回确定性非敏感夹具摘要。
    fn fingerprint(character: char) -> String {
        format!("sha256:{}", character.to_string().repeat(64))
    }

    /// Build a pure candidate fixture; no real registry or paid provider execution is involved.
    /// 构建纯候选夹具；不涉及真实注册表或付费供应商执行。
    /// `identity` selects the exact target and `dimension` its declared output. Returns test metadata.
    /// `identity` 选择精确目标，`dimension` 选择声明输出。返回测试元数据。
    fn candidate(identity: char, dimension: u32) -> VceEmbeddingCandidate {
        // Route labels are intentionally identical across fixtures; identifiers still differ.
        // 各夹具故意使用相同路由标签；标识仍然不同。
        let route = InferenceRouteConfiguration {
            provider_id: format!("provider-{identity}"),
            service_entry_id: String::from("service"),
            connection_id: format!("account-{identity}"),
            model_id: String::from("same-model-name"),
            binding: InferenceRouteBinding::Protocol {
                protocol_profile_id: String::from("embedding"),
            },
            operation: AiOperation::EmbeddingCreate,
            usage_scope: InferenceUsageScope::InternalBackground,
        };
        let execution = ExecutionRouteSnapshot {
            provider_id: route.provider_id.clone(),
            service_entry_id: route.service_entry_id.clone(),
            connection_id: route.connection_id.clone(),
            model_id: route.model_id.clone(),
            protocol_binding_id: Some(String::from("embedding")),
            action_binding_id: None,
            secondary_route_id: None,
            secondary_provider_name: None,
            secondary_endpoint_name: None,
            capability_revision: String::from("capability-v1"),
            catalog_revision: String::from("catalog-v1"),
            route_revision: String::from("wire-v1"),
            usage_scope: InferenceUsageScope::InternalBackground,
        };
        let capabilities = EmbeddingCapabilities {
            tasks: BTreeSet::from([EmbeddingTask::ProviderDefault]),
            input_resources: BTreeSet::new(),
            output_kinds: BTreeSet::from([EmbeddingOutputKind::Dense]),
            encodings: BTreeSet::from([EmbeddingEncoding::Float]),
            dimensions: BTreeSet::from([dimension]),
            minimum_dimensions: None,
            maximum_dimensions: None,
            default_dimensions: Some(dimension),
            maximum_inputs: None,
            maximum_tokens_per_input: None,
            normalization: EmbeddingNormalization::L2,
            distance_metric: DistanceMetric::Cosine,
        };
        VceEmbeddingCandidate {
            route,
            execution,
            capabilities,
            dimension,
            physical_fingerprint: fingerprint(identity),
        }
    }

    /// Build exact source metadata without creating its represented project or database.
    /// 构建精确来源元数据，不创建其表示的项目或数据库。
    /// `sources` contains retained producer identities. Returns one initialized original-A scope.
    /// `sources` 包含保留生产来源身份。返回一个已初始化的原始甲作用域。
    fn provenance(sources: &[char]) -> VceEmbeddingProvenance {
        // The temporary path is only an absolute identity in this pure test, not a written directory.
        // 临时路径在此纯测试中仅作为绝对身份，不是写入目录。
        let root_path = std::env::temp_dir()
            .join("vce-policy-fixture")
            .display()
            .to_string();
        let root_hash = hex::encode(Sha256::digest(root_path.as_bytes()));
        let space = EmbeddingSpace {
            vector_space_id: String::from("space-a"),
            physical_fingerprint: Some(fingerprint('a')),
            dimension: 1024,
            preprocessing_version: String::from(VCE_PREPROCESSING_VERSION),
            chunker_version: String::from(VCE_CHUNKER_VERSION),
        };
        VceEmbeddingProvenance {
            original: VceProjectIndexConfig {
                project_id: String::from("project-a"),
                root_path,
                root_hash,
                binding: VceIndexBinding::new("namespace-a", "chain-a", 1, space).unwrap(),
                vector_space_id: String::from("space-a"),
                physical_fingerprint: fingerprint('a'),
                preprocessing_version: String::from(VCE_PREPROCESSING_VERSION),
                chunker_version: String::from(VCE_CHUNKER_VERSION),
                dimension: 1024,
                config_revision: 1,
            },
            checkpoint_version: Some(7),
            source_fingerprints: sources.iter().map(|value| fingerprint(*value)).collect(),
        }
    }

    /// Evaluate a pure fixture using its exact existing canonical root.
    /// 使用夹具精确的现有规范根进行纯评估。
    /// Inputs are provenance, target, and direct records; returns an initialized impact or test failure.
    /// 输入为来源、目标和直接记录；返回已初始化影响，错误时测试失败。
    fn impact(
        provenance: &VceEmbeddingProvenance,
        target: &VceEmbeddingCandidate,
        records: &[ContextEngineEmbeddingCompatibilityRecord],
    ) -> VceEmbeddingProjectImpact {
        inspect_embedding_project_impact(
            provenance,
            Ok(&provenance.original.root_hash),
            target,
            records,
        )
        .unwrap()
        .unwrap()
    }

    /// Build one direct approval for the exact source, original scope, and resolved target fixture.
    /// 为精确来源、原始作用域和已解析目标夹具构建一个直接批准。
    /// Inputs identify one relationship; returns a structurally valid version-one approval.
    /// 输入标识一个关系；返回格式有效的第一版批准。
    fn approval(
        provenance: &VceEmbeddingProvenance,
        source: char,
        target: &VceEmbeddingCandidate,
    ) -> ContextEngineEmbeddingCompatibilityRecord {
        ContextEngineEmbeddingCompatibilityRecord {
            schema_version: 1,
            scope: impact(provenance, target, &[]).scope,
            source_fingerprint: fingerprint(source),
            target_fingerprint: target.physical_fingerprint.clone(),
            target_route: target.route.clone(),
            target_capability_revision: target.execution.capability_revision.clone(),
            target_route_revision: target.execution.route_revision.clone(),
            decision: ContextEngineEmbeddingDecision::Compatible,
            confirmed_at_unix_ms: 1000,
            configuration_revision: fingerprint('d'),
        }
    }

    /// Verify matching names do not approve a provider switch, and exact approval preserves original state.
    /// 验证名称相同不批准供应商切换，精确批准保留原始状态。
    /// No parameters; returns unit after direct-approval and immutability assertions.
    /// 无参数；直接批准及不可变性断言通过后返回空值。
    #[test]
    fn identical_names_need_direct_approval_without_rewriting_origin() {
        let source = provenance(&['a']);
        let before = source.clone();
        let target = candidate('b', 1024);
        assert_eq!(
            impact(&source, &target, &[]).confirmation_required,
            BTreeSet::from([fingerprint('a')])
        );
        assert!(impact(&source, &target, &[approval(&source, 'a', &target)]).compatible());
        assert_eq!(source, before);
    }

    /// Verify source-to-intermediate plus intermediate-to-target does not imply source-to-target.
    /// 验证来源到中间目标加中间目标到最终目标，不会推导来源到最终目标。
    /// No parameters; returns unit after non-transitive approval assertions.
    /// 无参数；非传递批准断言通过后返回空值。
    #[test]
    fn embedding_approval_is_never_transitive() {
        let source = provenance(&['a']);
        let intermediate = candidate('b', 1024);
        let target = candidate('c', 1024);
        let records = [
            approval(&source, 'a', &intermediate),
            approval(&source, 'b', &target),
        ];
        let result = impact(&source, &target, &records);
        assert!(!result.compatible());
        assert_eq!(
            result.confirmation_required,
            BTreeSet::from([fingerprint('a')])
        );
    }

    /// Verify every remaining actual source needs a direct grant, including a return to the original model.
    /// 验证每个保留实际来源都需要直接授权，包括切回原始模型。
    /// No parameters; returns unit after mixed-source and reverse-route assertions.
    /// 无参数；混合来源及反向路由断言通过后返回空值。
    #[test]
    fn mixed_sources_need_independent_coverage_in_both_directions() {
        let source = provenance(&['a', 'b']);
        let target = candidate('c', 1024);
        let first = approval(&source, 'a', &target);
        assert_eq!(
            impact(&source, &target, &[first.clone()]).confirmation_required,
            BTreeSet::from([fingerprint('b')])
        );
        assert!(impact(&source, &target, &[first, approval(&source, 'b', &target)]).compatible());
        let original = candidate('a', 1024);
        assert_eq!(
            impact(&source, &original, &[]).confirmation_required,
            BTreeSet::from([fingerprint('b')])
        );
    }

    /// Verify exact root, project, namespace, dimensions, and preprocessing cannot be waived by approval.
    /// 验证精确根、项目、命名空间、维度及预处理不能由批准免除。
    /// No parameters; returns unit after hard-constraint and scope-isolation assertions.
    /// 无参数；硬性约束及作用域隔离断言通过后返回空值。
    #[test]
    fn hard_constraints_and_project_scope_override_compatibility_records() {
        let source = provenance(&['a']);
        let target = candidate('b', 2048);
        assert!(
            impact(&source, &target, &[approval(&source, 'a', &target)])
                .hard_rebuild_reason
                .is_some()
        );
        let target = candidate('b', 1024);
        let mut record = approval(&source, 'a', &target);
        record.scope.project_id = String::from("another-project");
        assert!(!impact(&source, &target, &[record]).compatible());
        let record = approval(&source, 'a', &target);
        assert!(
            !inspect_embedding_project_impact(
                &source,
                Ok(&"f".repeat(64)),
                &target,
                &[record.clone()]
            )
            .unwrap()
            .unwrap()
            .compatible()
        );
        assert!(
            !inspect_embedding_project_impact(
                &source,
                Err("root is unavailable"),
                &target,
                &[record]
            )
            .unwrap()
            .unwrap()
            .compatible()
        );
    }

    /// Verify explicit rejection, duplicate keys, unknown versions, and target-evidence tampering fail closed.
    /// 验证明示拒绝、重复键、未知版本及目标证据篡改均安全关闭。
    /// No parameters; returns unit after policy corruption and rejection assertions.
    /// 无参数；策略损坏及拒绝断言通过后返回空值。
    #[test]
    fn contradictory_or_malformed_embedding_decisions_never_grant_access() {
        let source = provenance(&['a']);
        let target = candidate('b', 1024);
        let mut record = approval(&source, 'a', &target);
        record.decision = ContextEngineEmbeddingDecision::Rebuild;
        assert_eq!(
            impact(&source, &target, &[record.clone()]).rejected_sources,
            BTreeSet::from([fingerprint('a')])
        );
        assert!(
            inspect_embedding_project_impact(
                &source,
                Ok(&source.original.root_hash),
                &target,
                &[record.clone(), record.clone()]
            )
            .is_err()
        );
        record.schema_version = 2;
        assert!(
            inspect_embedding_project_impact(
                &source,
                Ok(&source.original.root_hash),
                &target,
                &[record.clone()]
            )
            .is_err()
        );
        record.schema_version = 1;
        record.target_route.model_id = String::from("tampered");
        assert!(
            inspect_embedding_project_impact(
                &source,
                Ok(&source.original.root_hash),
                &target,
                &[record]
            )
            .is_err()
        );
    }
}
