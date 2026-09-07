//! Read-only embedding preflight owned by the existing public inference settings transaction.
//! 由现有公共推理设置事务持有的只读向量预检。

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use vulcan_provider_adapters::{
    ContextEngineEmbeddingCompatibilityRecord, ContextEngineEmbeddingDecision,
    InferenceRouteConfiguration, PersistedAiConfiguration, PublicAiRuntime,
};

use super::public_ai_runtime_and_configuration;
use crate::vce::{
    VCE_CONSUMER_PROFILE_ID, VceEmbeddingCandidate, VceEmbeddingProjectImpact,
    inspect_embedding_candidate, inspect_embedding_project_impact,
};
use crate::{SharedServiceBridgeState, begin_public_ai_configuration_transaction};

/// Candidate route and explicit dimension to inspect before a separate human-confirmed save.
/// 在单独人为确认保存前检查的候选路由及明确维度。
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct VceEmbeddingPreflightRequest {
    /// Exact candidate, or explicit removal; no alternate route is selected automatically.
    /// 精确候选或明确移除；不自动选择替代路由。
    pub(crate) route: Option<InferenceRouteConfiguration>,
    /// User dimension; absence requests a UI suggestion and does not authorize saving a default.
    /// 用户维度；缺失时请求界面建议，不授权保存默认值。
    pub(crate) dimension: Option<u32>,
}

/// One human choice covering all currently shown source relationships of an exact project.
/// 覆盖一个精确项目当前展示的全部来源关系的人为选择。
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct VceEmbeddingProjectDecision {
    /// Exact project in the reviewed preflight, never a wildcard or later-created project.
    /// 已审查预检中的精确项目，绝不是通配或后续创建项目。
    pub(crate) project_id: String,
    /// Explicit same-space confirmation or rebuild decision.
    /// 明确的同空间确认或重建决定。
    pub(crate) decision: ContextEngineEmbeddingDecision,
}

/// Optimistic confirmation carried by the existing shared inference-route save RPC.
/// 现有共享推理路由保存 RPC 携带的乐观确认。
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct VceEmbeddingSaveConfirmation {
    /// Exact non-secret digest returned by preflight; stale reviews cannot be saved.
    /// 预检返回的精确非敏感摘要；过期审查不能保存。
    pub(crate) preflight_revision: String,
    /// Explicit choices for incompatible or unconfirmed project spaces.
    /// 对不兼容或尚未确认项目空间作出的明确选择。
    pub(crate) decisions: Vec<VceEmbeddingProjectDecision>,
}

/// Revalidate a confirmation and prepare policy records without separately persisting route or policy.
/// 重新校验确认并准备策略记录，不分别持久化路由或策略。
///
/// Inputs identify the current transaction, exact draft, and human confirmation. Returns the complete
/// replacement policy for atomic persistence with the route and explicit dimension.
/// 输入标识当前事务、精确草稿和人为确认。返回完整替换策略，与路由及明确维度原子持久化。
pub(super) async fn prepare_vce_embedding_save(
    state: &SharedServiceBridgeState,
    runtime: &PublicAiRuntime,
    configuration: &PersistedAiConfiguration,
    request: VceEmbeddingPreflightRequest,
    confirmation: &VceEmbeddingSaveConfirmation,
) -> Result<Vec<ContextEngineEmbeddingCompatibilityRecord>, String> {
    if request.route.is_some() && request.dimension.is_none() {
        return Err(String::from(
            "ContextEngine embedding saves require explicit dimensions, not a suggested default",
        ));
    }
    // Caller already holds the same transaction now acquired by every VCE checkpoint publisher.
    // 调用方已持有每个 VCE 检查点发布者目前共用的同一事务。
    let preflight = build_vce_embedding_preflight(state, runtime, configuration, request).await?;
    if preflight.revision != confirmation.preflight_revision {
        return Err(String::from(
            "ContextEngine model or index changed after preflight; review the current impact before saving",
        ));
    }
    // Timestamp records explicit confirmation only, never a provider call or implicit migration.
    // 时间戳仅记录明确确认，绝不记录供应商调用或隐式迁移。
    let confirmed_at = u64::try_from(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|error| format!("Read ContextEngine confirmation time: {error}"))?
            .as_millis(),
    )
    .map_err(|_| String::from("ContextEngine confirmation timestamp exceeds its contract"))?;
    apply_embedding_decisions(
        &preflight,
        &confirmation.decisions,
        &configuration.context_engine_embedding_compatibility,
        confirmed_at,
    )
}

/// Convert exact reviewed project decisions into direct source-to-target records without transitivity.
/// 将精确已审查项目决定转换为直接来源到目标记录，不执行传递推导。
///
/// `preflight` is freshly verified, `decisions` are explicit input, `retained` is existing policy,
/// and `confirmed_at` is the host timestamp. Returns complete policy or an invalid/missing-choice error.
/// `preflight` 已重新校验，`decisions` 为明确输入，`retained` 为既有策略，`confirmed_at`
/// 为宿主时间戳。返回完整策略或选择无效、缺失错误。
fn apply_embedding_decisions(
    preflight: &VceEmbeddingPreflight,
    decisions: &[VceEmbeddingProjectDecision],
    retained: &[ContextEngineEmbeddingCompatibilityRecord],
    confirmed_at: u64,
) -> Result<Vec<ContextEngineEmbeddingCompatibilityRecord>, String> {
    // Each project has exactly one UI decision covering its displayed, frozen source set.
    // 每个项目恰有一个界面决定，覆盖它已展示且冻结的来源集合。
    let mut choices = std::collections::BTreeMap::new();
    for choice in decisions {
        if !preflight
            .projects
            .iter()
            .any(|project| project.impact.scope.project_id == choice.project_id)
            || choices
                .insert(choice.project_id.as_str(), choice.decision)
                .is_some()
        {
            return Err(String::from(
                "ContextEngine confirmation contains an unknown or duplicate project",
            ));
        }
    }
    let mut records = retained.to_vec();
    let Some(candidate) = &preflight.candidate else {
        if !decisions.is_empty() {
            return Err(String::from(
                "Removing a ContextEngine route cannot grant compatibility",
            ));
        }
        return Ok(records);
    };
    for project in &preflight.projects {
        let impact = &project.impact;
        let decision = match choices.get(impact.scope.project_id.as_str()) {
            Some(decision) => *decision,
            None if impact.compatible() => continue,
            None => {
                return Err(format!(
                    "ContextEngine project `{}` requires an explicit compatibility or rebuild decision",
                    impact.scope.project_id
                ));
            }
        };
        if impact.hard_rebuild_reason.is_some()
            && decision != ContextEngineEmbeddingDecision::Rebuild
        {
            return Err(String::from(
                "ContextEngine hard incompatibility cannot be overridden by compatibility confirmation",
            ));
        }
        // A hard rebuild acknowledgement never creates an approval for mismatched dimensions.
        // 硬性重建确认绝不为不匹配维度创建批准。
        if impact.hard_rebuild_reason.is_some() {
            continue;
        }
        for source in &impact.source_fingerprints {
            if source == &candidate.physical_fingerprint {
                continue;
            }
            let record = ContextEngineEmbeddingCompatibilityRecord {
                schema_version: 1,
                scope: impact.scope.clone(),
                source_fingerprint: source.clone(),
                target_fingerprint: candidate.physical_fingerprint.clone(),
                target_route: candidate.route.clone(),
                target_capability_revision: candidate.execution.capability_revision.clone(),
                target_route_revision: candidate.execution.route_revision.clone(),
                decision,
                confirmed_at_unix_ms: confirmed_at,
                configuration_revision: preflight.revision.clone(),
            };
            record.validate()?;
            // Replace only this exact relationship; other roots, targets and consumers remain unchanged.
            // 仅替换此精确关系；其他根、目标和消费者保持不变。
            records.retain(|prior| {
                !(prior.scope == record.scope
                    && prior.source_fingerprint == record.source_fingerprint
                    && prior.target_fingerprint == record.target_fingerprint)
            });
            records.push(record);
        }
    }
    Ok(records)
}

/// Safe project label paired with exact compatibility facts, including disabled projects with indexes.
/// 安全项目标签及精确兼容事实，包含已有索引的关闭项目。
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct VceEmbeddingProjectPreflight {
    /// Workbench display label; never used as compatibility identity.
    /// 工作台展示标签；绝不作为兼容身份。
    pub(crate) project_name: String,
    /// Exact compatibility assessment derived from committed storage.
    /// 从已提交存储推导的精确兼容评估。
    pub(crate) impact: VceEmbeddingProjectImpact,
    /// Complete direct coverage; derived from impact, not an independent persisted switch.
    /// 完整直接覆盖；从影响派生，不是独立持久化开关。
    pub(crate) compatible: bool,
}

/// Immutable inspection result with an optimistic, non-secret confirmation identity.
/// 带乐观非敏感确认身份的不可变检查结果。
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct VceEmbeddingPreflight {
    /// Exact candidate metadata; absent only for explicit route removal.
    /// 精确候选元数据；仅明确移除路由时缺失。
    pub(crate) candidate: Option<VceEmbeddingCandidate>,
    /// Every committed project index affected or covered by the candidate, sorted by exact identity.
    /// 候选影响或覆盖的全部已提交项目索引，按精确身份排序。
    pub(crate) projects: Vec<VceEmbeddingProjectPreflight>,
    /// Content-addressed preflight; saving must recompute it under the same publication barrier.
    /// 内容寻址预检；保存必须在相同发布屏障下重新计算它。
    pub(crate) revision: String,
}

/// Inspect a candidate and retained project origins without model calls, index creation, or config writes.
/// 检查候选及保留项目来源，不调用模型、不创建索引、不写配置。
///
/// `state` owns the common runtime and workspace; `request` contains draft UI input.
/// Returns declared dimension suggestions and exact per-project compatibility requirements.
/// `state` 持有公共运行时及工作区；`request` 包含界面草稿输入。
/// 返回声明维度建议和精确逐项目兼容要求。
pub(crate) async fn service_preflight_vce_embedding_route(
    state: SharedServiceBridgeState,
    request: VceEmbeddingPreflightRequest,
) -> Result<VceEmbeddingPreflight, String> {
    // The existing configuration transaction excludes simultaneous provider/catalog publication.
    // 现有配置事务排除同时发生的供应商及目录发布。
    let _transaction = begin_public_ai_configuration_transaction().await;
    let (runtime, configuration) = public_ai_runtime_and_configuration(&state)?;
    build_vce_embedding_preflight(&state, runtime.as_ref(), &configuration, request).await
}

/// Build an exact preflight under a caller-owned public configuration transaction.
/// 在调用方持有的公共配置事务中构建精确预检。
///
/// `state`, `runtime`, and `configuration` identify the host snapshot; `request` is unsaved intent.
/// Returns a deterministic digest of only VCE embedding intent, policies, project identity, and origins.
/// `state`、`runtime` 与 `configuration` 标识宿主快照；`request` 为未保存意图。
/// 仅返回 VCE 向量意图、策略、项目身份及来源的确定性摘要。
pub(super) async fn build_vce_embedding_preflight(
    state: &SharedServiceBridgeState,
    runtime: &PublicAiRuntime,
    configuration: &PersistedAiConfiguration,
    request: VceEmbeddingPreflightRequest,
) -> Result<VceEmbeddingPreflight, String> {
    if request.route.is_none() && request.dimension.is_some() {
        return Err(String::from(
            "Removing a ContextEngine embedding route cannot retain dimensions",
        ));
    }
    // Candidate validation reuses actual dispatch selection but does not activate its draft route.
    // 候选校验复用实际分派选择，但不激活其草稿路由。
    let candidate = request
        .route
        .map(|route| inspect_embedding_candidate(runtime, configuration, route, request.dimension))
        .transpose()?;
    // Read only authoritative workspace identities; opening 100 projects never means scanning them.
    // 仅读权威工作区身份；打开一百个项目绝不意味着扫描它们。
    let (mut projects, manager) = {
        let guard = state
            .lock()
            .map_err(|error| format!("Lock ContextEngine settings state: {error}"))?;
        let workspace = guard
            .workspace_snapshot
            .as_ref()
            .ok_or_else(|| String::from("ContextEngine workspace metadata is not ready"))?;
        (
            workspace.task_projects.clone(),
            guard
                .vce_runtime
                .clone()
                .ok_or_else(|| String::from("ContextEngine storage service is not ready"))?,
        )
    };
    projects.sort_by(|left, right| left.id.cmp(&right.id));
    // Root/path identity participates even for uninitialized projects, detecting replacement during review.
    // 未初始化项目也包含根及路径身份，用于检测审查期间发生的替换。
    let project_identities = projects
        .iter()
        .map(|project| (&project.id, &project.path))
        .collect::<Vec<_>>();
    let mut impacts = Vec::new();
    if let Some(candidate) = &candidate {
        for project in &projects {
            let Some(provenance) = manager.embedding_provenance(&project.id).await? else {
                continue;
            };
            // Canonicalizing one root is metadata inspection, not a recursive file enumeration.
            // 规范化一个根目录属于元数据检查，不是递归文件枚举。
            let root_hash = crate::workspace_files::workspace_file_canonical_root(&project.path)
                .map(|root| {
                    hex::encode(Sha256::digest(
                        crate::normalized_workspace_path_text_from_path(&root).as_bytes(),
                    ))
                });
            if let Some(impact) = inspect_embedding_project_impact(
                &provenance,
                root_hash.as_deref().map_err(String::as_str),
                candidate,
                &configuration.context_engine_embedding_compatibility,
            )? {
                let compatible = impact.compatible();
                impacts.push(VceEmbeddingProjectPreflight {
                    project_name: project.title.clone(),
                    impact,
                    compatible,
                });
            }
        }
    }
    // Only the embedding slot and decisions affect this save; peer consumer settings do not expire it.
    // 仅向量槽位和决定影响本次保存；其他消费者设置不会使它过期。
    let profile = configuration
        .inference_profiles
        .get(VCE_CONSUMER_PROFILE_ID);
    // Exclude presentation and catalog-wide revisions from the confirmation digest as from space identity.
    // 与空间身份一样，从确认摘要排除展示和全目录修订。
    let target_identity = candidate.as_ref().map(|candidate| {
        (
            &candidate.route,
            candidate.dimension,
            &candidate.physical_fingerprint,
        )
    });
    let impact_identities = impacts
        .iter()
        .map(|project| &project.impact)
        .collect::<Vec<_>>();
    let encoded = serde_json::to_vec(&(
        "vce-embedding-preflight-v1",
        profile.and_then(|profile| profile.embedding.as_ref()),
        profile.and_then(|profile| profile.embedding_dimensions),
        &configuration.context_engine_embedding_compatibility,
        target_identity,
        project_identities,
        impact_identities,
    ))
    .map_err(|error| format!("Encode ContextEngine embedding preflight: {error}"))?;
    Ok(VceEmbeddingPreflight {
        candidate,
        projects: impacts,
        revision: format!("sha256:{}", hex::encode(Sha256::digest(encoded))),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;
    use vulcan_ai_contract::{
        AiOperation, DistanceMetric, EmbeddingCapabilities, EmbeddingEncoding,
        EmbeddingNormalization, EmbeddingOutputKind, EmbeddingTask, ExecutionRouteSnapshot,
        InferenceUsageScope,
    };
    use vulcan_provider_adapters::{ContextEngineEmbeddingScope, InferenceRouteBinding};

    /// Build a complete trusted preflight containing two actual sources and no prior direct approvals.
    /// 构建含两个实际来源且没有既有直接批准的完整可信预检。
    /// No inputs; returns pure metadata without a database, registration, or provider call.
    /// 无输入；返回纯元数据，不需要数据库、注册或供应商调用。
    fn preflight_fixture() -> VceEmbeddingPreflight {
        // Exact route fields follow the same shared type consumed by normal settings saves.
        // 精确路由字段遵循普通设置保存使用的同一共享类型。
        let route = InferenceRouteConfiguration {
            provider_id: String::from("provider-c"),
            service_entry_id: String::from("service"),
            connection_id: String::from("account-c"),
            model_id: String::from("same-model-name"),
            binding: InferenceRouteBinding::Protocol {
                protocol_profile_id: String::from("embedding"),
            },
            operation: AiOperation::EmbeddingCreate,
            usage_scope: InferenceUsageScope::InternalBackground,
        };
        let candidate = VceEmbeddingCandidate {
            execution: ExecutionRouteSnapshot {
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
            },
            route,
            dimension: 1024,
            physical_fingerprint: format!("sha256:{}", "c".repeat(64)),
            capabilities: EmbeddingCapabilities {
                tasks: BTreeSet::from([EmbeddingTask::ProviderDefault]),
                input_resources: BTreeSet::new(),
                output_kinds: BTreeSet::from([EmbeddingOutputKind::Dense]),
                encodings: BTreeSet::from([EmbeddingEncoding::Float]),
                dimensions: BTreeSet::from([1024]),
                minimum_dimensions: None,
                maximum_dimensions: None,
                default_dimensions: None,
                maximum_inputs: None,
                maximum_tokens_per_input: None,
                normalization: EmbeddingNormalization::L2,
                distance_metric: DistanceMetric::Cosine,
            },
        };
        let sources = BTreeSet::from([
            format!("sha256:{}", "a".repeat(64)),
            format!("sha256:{}", "b".repeat(64)),
        ]);
        VceEmbeddingPreflight {
            candidate: Some(candidate),
            revision: format!("sha256:{}", "d".repeat(64)),
            projects: vec![VceEmbeddingProjectPreflight {
                project_name: String::from("Displayed project"),
                compatible: false,
                impact: VceEmbeddingProjectImpact {
                    scope: ContextEngineEmbeddingScope {
                        project_id: String::from("project-a"),
                        root_hash: "a".repeat(64),
                        namespace_id: String::from("namespace-a"),
                        checkpoint_chain_id: String::from("chain-a"),
                        original_fingerprint: format!("sha256:{}", "a".repeat(64)),
                        index_schema_version: 1,
                        dimension: 1024,
                        preprocessing_version: String::from("vce-preprocess-v1"),
                        chunker_version: String::from("vce-recursive-v1"),
                    },
                    checkpoint_version: 7,
                    source_fingerprints: sources.clone(),
                    confirmation_required: sources,
                    rejected_sources: BTreeSet::new(),
                    hard_rebuild_reason: None,
                },
            }],
        }
    }

    /// Persist every direct source-to-target choice together while preserving unrelated exact scopes.
    /// 同时持久化每个直接来源到目标选择，并保留无关精确作用域。
    /// No inputs or outputs; validates exact fields and roundtrip serialization of generated policy.
    /// 无输入输出；校验生成策略的精确字段及往返序列化。
    #[test]
    fn save_creates_direct_coverage_and_preserves_unrelated_records() {
        let preflight = preflight_fixture();
        assert!(apply_embedding_decisions(&preflight, &[], &[], 1).is_err());
        let choices = vec![VceEmbeddingProjectDecision {
            project_id: String::from("project-a"),
            decision: ContextEngineEmbeddingDecision::Compatible,
        }];
        let records = apply_embedding_decisions(&preflight, &choices, &[], 42).unwrap();
        assert_eq!(records.len(), 2);
        assert!(
            records
                .iter()
                .all(|record| record.confirmed_at_unix_ms == 42
                    && record.target_fingerprint
                        == preflight.candidate.as_ref().unwrap().physical_fingerprint
                    && record.scope == preflight.projects[0].impact.scope
                    && record.configuration_revision == preflight.revision)
        );
        let encoded = serde_json::to_vec(&records).unwrap();
        let decoded: Vec<ContextEngineEmbeddingCompatibilityRecord> =
            serde_json::from_slice(&encoded).unwrap();
        assert_eq!(decoded, records);
        let mut other = records[0].clone();
        other.scope.project_id = String::from("another-project");
        let mut retained = records;
        retained.push(other.clone());
        let rebuilt = apply_embedding_decisions(
            &preflight,
            &[VceEmbeddingProjectDecision {
                project_id: String::from("project-a"),
                decision: ContextEngineEmbeddingDecision::Rebuild,
            }],
            &retained,
            43,
        )
        .unwrap();
        assert_eq!(rebuilt.len(), 3);
        assert!(rebuilt.contains(&other));
        assert!(
            rebuilt
                .iter()
                .filter(|record| record.scope.project_id == "project-a")
                .all(|record| record.decision == ContextEngineEmbeddingDecision::Rebuild)
        );
        assert_eq!(
            retained[0].decision,
            ContextEngineEmbeddingDecision::Compatible
        );
    }

    /// Reject duplicate, missing, unknown, and hard-constraint-overriding choices before changing policy.
    /// 在修改策略前拒绝重复、缺失、未知及覆盖硬性约束的选择。
    /// No inputs or outputs; explicit rebuild acknowledgement never creates an incompatible approval.
    /// 无输入输出；明确重建确认绝不创建不兼容批准。
    #[test]
    fn invalid_choices_and_hard_overrides_fail_closed() {
        let mut preflight = preflight_fixture();
        let choice = VceEmbeddingProjectDecision {
            project_id: String::from("project-a"),
            decision: ContextEngineEmbeddingDecision::Compatible,
        };
        assert!(
            apply_embedding_decisions(&preflight, &[choice.clone(), choice.clone()], &[], 1)
                .is_err()
        );
        assert!(
            apply_embedding_decisions(
                &preflight,
                &[VceEmbeddingProjectDecision {
                    project_id: String::from("unknown-project"),
                    decision: choice.decision
                }],
                &[],
                1
            )
            .is_err()
        );
        preflight.projects[0].impact.hard_rebuild_reason = Some(String::from("Dimensions changed"));
        assert!(apply_embedding_decisions(&preflight, &[choice], &[], 1).is_err());
        assert!(
            apply_embedding_decisions(
                &preflight,
                &[VceEmbeddingProjectDecision {
                    project_id: String::from("project-a"),
                    decision: ContextEngineEmbeddingDecision::Rebuild
                }],
                &[],
                1
            )
            .unwrap()
            .is_empty()
        );
    }

    /// Removing a model preserves historical exact approvals and cannot smuggle new project decisions.
    /// 移除模型保留历史精确批准，且不能夹带新的项目决定。
    /// No inputs or outputs; asserts retained records are unchanged and route removal creates no grant.
    /// 无输入输出；断言保留记录不变且路由移除不创建授权。
    #[test]
    fn route_removal_preserves_policy_without_granting_compatibility() {
        let mut preflight = preflight_fixture();
        let choices = vec![VceEmbeddingProjectDecision {
            project_id: String::from("project-a"),
            decision: ContextEngineEmbeddingDecision::Compatible,
        }];
        let retained = apply_embedding_decisions(&preflight, &choices, &[], 1).unwrap();
        preflight.candidate = None;
        preflight.projects.clear();
        assert_eq!(
            apply_embedding_decisions(&preflight, &[], &retained, 2).unwrap(),
            retained
        );
        assert!(apply_embedding_decisions(&preflight, &choices, &retained, 2).is_err());
    }
}
