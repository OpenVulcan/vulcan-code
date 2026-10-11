//! Authoritative fail-closed configuration gate for Vulcan Context Engine.
//! Vulcan Context Engine 的权威失败关闭配置闸门。

use std::collections::BTreeSet;
use std::sync::Arc;

use vulcan_ai_contract::{
    AiOperation, InferenceCallerBudget, InferenceCallerContext, InferenceCapabilitySnapshot,
    InferenceOperationCapability, InferencePermission, InferenceUsageScope,
    StructuredOutputCapability,
};
use vulcan_provider_adapters::PublicAiRuntime;
use vulcan_service_api as api;

use crate::{SharedServiceBridgeState, configured_inference::ConfiguredInferenceService};

use super::{
    VCE_CONSUMER_PROFILE_ID, VCE_QUERY_REWRITE_PURPOSE_ID, vce_embedding_physical_fingerprint,
    vce_rerank_truncation,
};

/// Build a weak host authority for publication without retaining a service-state ownership cycle.
/// 构建弱宿主发布权威，不保留服务状态所有权环。
///
/// `state` owns live global readiness and project intent. Returns a synchronous resolver that
/// checks only the admitted embedding/source contract, not unrelated queries or scheduling options.
/// `state` 持有实时全局就绪及项目意图。返回同步解析器，仅检查已准入向量及源码契约，
/// 不检查无关查询或调度选项。
pub(crate) fn vce_publication_authority(
    state: &SharedServiceBridgeState,
) -> super::VcePublicationAuthority {
    // Weak ownership lets service shutdown release every manager and host binding.
    // 弱归属允许服务关闭释放全部管理器和宿主绑定。
    let state = Arc::downgrade(state);
    Arc::new(
        move |contract: &super::VcePublicationContract| -> Result<(), super::VcePublicationError> {
            let state = state
                .upgrade()
                .ok_or_else(|| String::from("ContextEngine host stopped before publication"))?;
            let readiness = require_vce_configuration_ready(&state)?;
            if readiness.embedding_dimension != Some(contract.dimension)
                || readiness.embedding_physical_fingerprint.as_deref()
                    != Some(contract.embedding_fingerprint.as_str())
            {
                return Err(super::VcePublicationError::Rejected(String::from(
                    "ContextEngine embedding contract changed before publication; synchronize again under the new configuration",
                )));
            }
            // Resolve one exact workspace project while holding the state lock only for metadata cloning.
            // 仅为元数据克隆持有状态锁，并解析一个精确工作区项目。
            let project = {
                let guard = state
                    .lock()
                    .map_err(|error| format!("Lock ContextEngine publication state: {error}"))?;
                guard
                    .workspace_snapshot
                    .as_ref()
                    .and_then(|workspace| {
                        workspace
                            .task_projects
                            .iter()
                            .find(|project| project.id == contract.project_id)
                    })
                    .cloned()
                    .ok_or_else(|| {
                        String::from("ContextEngine publication project no longer exists")
                    })?
            };
            if !project.vce.enabled
                || project.vce.max_file_bytes != contract.max_file_bytes
                || project.vce.extra_ignore_patterns != contract.extra_ignore_patterns
            {
                return Err(super::VcePublicationError::Rejected(String::from(
                    "ContextEngine project authorization or source filter changed before publication",
                )));
            }
            let root = crate::workspace_files::workspace_file_canonical_root(&project.path)?;
            if crate::normalized_workspace_path_text_from_path(&root) != contract.canonical_root {
                return Err(super::VcePublicationError::Rejected(String::from(
                    "ContextEngine project root changed before publication",
                )));
            }
            // A confirmed independent target does not consume predecessor vectors, so it requires no compatibility grant.
            // 已确认的独立目标不使用前驱向量，因此不需要兼容授权。
            // Live route, enablement, filters and root were still checked above; persistence rechecks the exact receipt.
            // 上方仍已检查实时路由、开关、过滤及根目录；持久层复查精确回执。
            if contract.validates_replacement_admission()? {
                return Ok(());
            }
            if let Some(provenance) = &contract.provenance {
                if provenance.original.project_id != contract.project_id {
                    return Err(super::VcePublicationError::Rejected(String::from(
                        "ContextEngine provenance belongs to another project",
                    )));
                }
                // Compatibility is checked from exact current persisted decisions, including a revoked approval.
                // 兼容根据精确当前持久化决定检查，包括已撤销的批准。
                let (runtime, _) = vce_ai_runtime_handles(&state)?;
                let configuration = runtime.configuration_snapshot()?;
                let route = configuration
                    .inference_profiles
                    .get(VCE_CONSUMER_PROFILE_ID)
                    .and_then(|profile| profile.embedding.clone())
                    .ok_or_else(|| {
                        String::from("ContextEngine embedding route disappeared during admission")
                    })?;
                let candidate = super::embedding_policy::inspect_embedding_candidate(
                    &runtime,
                    &configuration,
                    route,
                    Some(contract.dimension),
                )?;
                if candidate.physical_fingerprint != contract.embedding_fingerprint {
                    return Err(super::VcePublicationError::Rejected(String::from(
                        "ContextEngine candidate changed during admission",
                    )));
                }
                // The canonical root has already been verified above; this hash uses the same host normalization.
                // 上方已经校验规范根；此摘要使用相同宿主规范化方式。
                let root_hash = hex::encode(<sha2::Sha256 as sha2::Digest>::digest(
                    contract.canonical_root.as_bytes(),
                ));
                match super::embedding_policy::inspect_embedding_project_impact(
                    provenance,
                    Ok(&root_hash),
                    &candidate,
                    &configuration.context_engine_embedding_compatibility,
                )? {
                    Some(impact) if !impact.compatible() => {
                        return Err(super::VcePublicationError::RebuildRequired(String::from(
                            "ContextEngine index requires explicit direct embedding compatibility approval or rebuilding",
                        )));
                    }
                    // An empty binding has no approved vector history and cannot silently change its physical schema.
                    // 空绑定没有已批准向量历史，也不能静默改变其物理结构。
                    None if provenance.original.physical_fingerprint
                        != contract.embedding_fingerprint
                        || provenance.original.root_hash != root_hash
                        || provenance.original.dimension != contract.dimension =>
                    {
                        return Err(super::VcePublicationError::RebuildRequired(String::from(
                            "ContextEngine uninitialized binding requires explicit replacement before changing its contract",
                        )));
                    }
                    _ => {}
                }
            }
            Ok(())
        },
    )
}

/// Internal VCE readiness facts including the storage compatibility identity.
/// VCE 内部就绪事实，包含存储兼容性身份。
#[derive(Clone, Debug)]
pub(crate) struct VceConfigurationReadiness {
    /// Public provider-independent status returned to management surfaces.
    /// 返回给管理界面的公共且不依赖 Provider 的状态。
    pub(crate) status: api::VceGlobalConfigurationStatus,
    /// Exact embedding identity that must match an active project index.
    /// 必须与活动项目索引匹配的精确向量身份。
    pub(crate) embedding_physical_fingerprint: Option<String>,
    /// Saved consumer dimension, validated whenever the embedding capability is available.
    /// 消费者保存的维度；向量能力可用时已完成校验。
    pub(crate) embedding_dimension: Option<u32>,
}

/// Computes the only configuration gate used by VCE lifecycle and tool surfaces.
/// 计算 VCE 生命周期与工具界面共同使用的唯一配置闸门。
///
/// # Parameters
/// 参数
/// `state` owns the live public-AI runtime and configured inference service.
/// `state` 持有实时公共 AI 运行时与已配置推理服务。
///
/// # Returns
/// 返回值
/// A fail-closed status; infrastructure failures are retained as bounded diagnostics.
/// 返回失败关闭状态；基础设施错误会保留为有界诊断。
pub(crate) fn vce_configuration_readiness(
    state: &SharedServiceBridgeState,
) -> VceConfigurationReadiness {
    match try_vce_configuration_readiness(state) {
        Ok(readiness) => readiness,
        Err(error) => unavailable_readiness(error),
    }
}

/// Requires the complete VCE model contract before admitting runtime work.
/// 在接纳运行时工作前要求完整的 VCE 模型契约。
///
/// # Parameters
/// 参数
/// `state` owns the authoritative configured inference generation.
/// `state` 持有权威已配置推理代际。
///
/// # Returns
/// 返回值
/// Current readiness facts, or one combined diagnostic when any prerequisite is unavailable.
/// 返回当前就绪事实；任一前置条件不可用时返回一条合并诊断。
pub(crate) fn require_vce_configuration_ready(
    state: &SharedServiceBridgeState,
) -> Result<VceConfigurationReadiness, String> {
    let readiness = vce_configuration_readiness(state);
    if readiness.status.ready {
        return Ok(readiness);
    }
    Err(format!(
        "The VCE global model configuration is not ready: {}",
        readiness.status.issues.join("; ")
    ))
}

/// Reports whether one project may expose the model-facing VCE retrieval tool now.
/// 判断一个项目当前是否可以暴露面向模型的 VCE 检索工具。
///
/// # Parameters
/// 参数
/// `state` owns both global inference readiness and persisted project intent.
/// `state` 同时持有全局推理就绪状态与持久化项目意图。
/// `project_id` is the execution-context-owned project identity.
/// `project_id` 是执行上下文持有的项目身份。
///
/// # Returns
/// 返回值
/// `true` only when configuration is ready and the exact enabled runtime can serve a checkpoint.
/// 仅当配置就绪且精确已启用运行时能够提供检查点时返回 `true`。
pub(crate) fn vce_project_tool_enabled(state: &SharedServiceBridgeState, project_id: &str) -> bool {
    let readiness = vce_configuration_readiness(state);
    if !readiness.status.ready {
        return false;
    }
    let Some(expected_fingerprint) = readiness.embedding_physical_fingerprint.as_deref() else {
        return false;
    };
    let Ok(guard) = state.lock() else {
        return false;
    };
    let Some(project) = guard.workspace_snapshot.as_ref().and_then(|workspace| {
        workspace
            .task_projects
            .iter()
            .find(|project| project.id == project_id)
    }) else {
        return false;
    };
    // Copy exact eligibility before invoking an authority which itself briefly reads host state.
    // 调用同样会短暂读取宿主状态的权威前，复制精确资格，避免重入同一状态锁。
    let enabled = project.vce.enabled;
    let revision = project.vce.revision;
    let manager = guard.vce_runtime.clone();
    drop(guard);
    enabled
        && manager.is_some_and(|manager| {
            manager.project_can_serve_now(project_id, revision, expected_fingerprint)
        })
}

/// Evaluates live configuration after releasing the shared service-state lock.
/// 在释放共享服务状态锁后评估实时配置。
fn try_vce_configuration_readiness(
    state: &SharedServiceBridgeState,
) -> Result<VceConfigurationReadiness, String> {
    // Runtime handles are cloned atomically so provider validation never retains the service lock.
    // 运行时句柄以原子方式克隆，使 Provider 校验绝不持续持有服务锁。
    let (runtime, configured_inference) = vce_ai_runtime_handles(state)?;
    let configuration = runtime.configuration_snapshot()?;
    let Some(profile) = configuration
        .inference_profiles
        .get(VCE_CONSUMER_PROFILE_ID)
    else {
        return Ok(unavailable_readiness(format!(
            "The `{VCE_CONSUMER_PROFILE_ID}` inference profile is not configured in public AI yet."
        )));
    };
    let snapshot = configured_inference
        .capability_snapshot(&vce_status_caller(), VCE_CONSUMER_PROFILE_ID)
        .map_err(|error| error.to_string())?;
    let embedding = evaluate_embedding(
        profile.embedding.is_some(),
        profile.embedding_dimensions,
        &snapshot,
    );
    let query_rewrite = evaluate_query_rewrite(
        Arc::as_ref(&runtime),
        &configuration,
        profile.llm_routes.get(VCE_QUERY_REWRITE_PURPOSE_ID),
        &snapshot,
    );
    let rerank = evaluate_rerank(profile.rerank.is_some(), &snapshot);
    let mut issues = Vec::new();
    for capability in [&embedding.0, &query_rewrite, &rerank] {
        if let Some(issue) = capability.issue.as_ref() {
            issues.push(issue.clone());
        }
    }
    Ok(VceConfigurationReadiness {
        status: api::VceGlobalConfigurationStatus {
            ready: embedding.0.available && query_rewrite.available && rerank.available,
            consumer_profile_id: String::from(VCE_CONSUMER_PROFILE_ID),
            query_rewrite_purpose_id: String::from(VCE_QUERY_REWRITE_PURPOSE_ID),
            reasoning_policy: api::VceReasoningPolicy::FixedOff,
            embedding: embedding.0,
            query_rewrite,
            rerank,
            issues,
        },
        embedding_physical_fingerprint: embedding.1,
        embedding_dimension: profile.embedding_dimensions,
    })
}

/// Clones the exact public-AI runtime handles used by VCE readiness validation.
/// 克隆 VCE 就绪校验使用的精确公共 AI 运行时句柄。
fn vce_ai_runtime_handles(
    state: &SharedServiceBridgeState,
) -> Result<(Arc<PublicAiRuntime>, Arc<ConfiguredInferenceService>), String> {
    let guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    let runtime = guard
        .agent_runtime
        .as_ref()
        .ok_or_else(|| String::from("The public AI inference runtime has not started yet."))?;
    Ok((
        Arc::clone(&runtime.public_ai_runtime),
        Arc::clone(&runtime.configured_inference_service),
    ))
}

/// Builds the least-privilege status caller for VCE configuration inspection.
/// 构建用于 VCE 配置检查的最小权限状态调用方。
fn vce_status_caller() -> InferenceCallerContext {
    InferenceCallerContext {
        caller_id: String::from("vulcan-code-vce:configuration-readiness"),
        // Readiness only inspects capabilities and does not request foreground execution.
        // 准备度只检查能力，不请求前台执行权限。
        scheduling_class: vulcan_ai_contract::InferenceSchedulingClass::ByUsageScope,
        allowed_consumer_profile_ids: BTreeSet::from([String::from(VCE_CONSUMER_PROFILE_ID)]),
        permissions: BTreeSet::from([InferencePermission::Status]),
        usage_scope: InferenceUsageScope::InternalBackground,
        interactive_session_id: None,
        deadline_unix_ms: None,
        budget: InferenceCallerBudget {
            max_concurrent_executions: 1,
            max_input_units_per_request: None,
            max_output_tokens_per_request: None,
            max_requests_per_minute: None,
            max_input_units_per_minute: None,
            max_execution_duration_ms: None,
        },
    }
}

/// Evaluates the required task-specific or provider-default dense-float embedding route.
/// 评估必需的任务专用或 Provider 默认稠密浮点向量路由。
fn evaluate_embedding(
    configured: bool,
    dimension: Option<u32>,
    snapshot: &InferenceCapabilitySnapshot,
) -> (api::VceConfigurationCapabilityStatus, Option<String>) {
    if !configured {
        return (
            unavailable_capability(false, "The VCE embedding route is not configured."),
            None,
        );
    }
    // Configuration must retain a numeric dimension; a model's default cannot complete it.
    // 配置必须保存数值维度，不能以模型默认值补全。
    let Some(dimension) = dimension else {
        return (
            unavailable_capability(
                true,
                "Confirm and save an explicit vector dimension in the semantic engine settings.",
            ),
            None,
        );
    };
    let Some(operation) = exact_operation(snapshot, AiOperation::EmbeddingCreate, None) else {
        return (
            unavailable_capability(true, "The VCE embedding capability snapshot is missing."),
            None,
        );
    };
    if !operation.available {
        return (
            unavailable_capability(
                true,
                operation
                    .unavailable_reason
                    .as_deref()
                    .unwrap_or("The VCE embedding route is currently unavailable."),
            ),
            None,
        );
    }
    match vce_embedding_physical_fingerprint(snapshot, dimension) {
        Ok(fingerprint) => (available_capability(), Some(fingerprint)),
        Err(error) => (unavailable_capability(true, error), None),
    }
}

/// Evaluates the required strict-JSON LLM route and its fixed-off reasoning projection.
/// 评估必需的严格 JSON LLM 路由及其固定关闭推理投影。
fn evaluate_query_rewrite(
    runtime: &PublicAiRuntime,
    configuration: &vulcan_provider_adapters::PersistedAiConfiguration,
    route: Option<&vulcan_provider_adapters::InferenceRouteConfiguration>,
    snapshot: &InferenceCapabilitySnapshot,
) -> api::VceConfigurationCapabilityStatus {
    let Some(route) = route else {
        return unavailable_capability(false, "The VCE query rewrite LLM route is not configured.");
    };
    let Some(operation) = exact_operation(
        snapshot,
        AiOperation::ConversationRespond,
        Some(VCE_QUERY_REWRITE_PURPOSE_ID),
    ) else {
        return unavailable_capability(
            true,
            "The VCE query rewrite LLM capability snapshot is missing.",
        );
    };
    if !operation.available {
        return unavailable_capability(
            true,
            operation
                .unavailable_reason
                .as_deref()
                .unwrap_or("The VCE query rewrite LLM route is currently unavailable."),
        );
    }
    let selection = match runtime.resolve_internal_inference_selection(configuration, route) {
        Ok(selection) => selection,
        Err(error) => return unavailable_capability(true, error),
    };
    if selection.model.capabilities.structured_output == StructuredOutputCapability::Unsupported {
        return unavailable_capability(
            true,
            "VCE query rewrite requires a model that supports host-verifiable JSON output.",
        );
    }
    if !crate::configured_inference::model_supports_disabled_reasoning(
        &selection.model.capabilities,
    ) {
        return unavailable_capability(
            true,
            "VCE query rewrite requires a model that can explicitly turn off reasoning.",
        );
    }
    if let Err(error) = runtime.validate_disabled_reasoning_route_projection(&selection) {
        return unavailable_capability(
            true,
            format!("VCE query rewrite cannot project the reasoning-off request: {error}"),
        );
    }
    available_capability()
}

/// Evaluates the required rerank route with an explicitly declared truncation policy.
/// 评估必需且明确声明截断策略的重排路由。
fn evaluate_rerank(
    configured: bool,
    snapshot: &InferenceCapabilitySnapshot,
) -> api::VceConfigurationCapabilityStatus {
    if !configured {
        return unavailable_capability(false, "The VCE rerank route is not configured.");
    }
    let Some(operation) = exact_operation(snapshot, AiOperation::RerankDocuments, None) else {
        return unavailable_capability(true, "The VCE rerank capability snapshot is missing.");
    };
    if !operation.available {
        return unavailable_capability(
            true,
            operation
                .unavailable_reason
                .as_deref()
                .unwrap_or("The VCE rerank route is currently unavailable."),
        );
    }
    let Some(capabilities) = operation.rerank.as_ref() else {
        return unavailable_capability(
            true,
            "The VCE rerank model has no typed capability declaration.",
        );
    };
    if let Err(error) = vce_rerank_truncation(&capabilities.truncation) {
        return unavailable_capability(true, error);
    }
    available_capability()
}

/// Finds one exact operation and purpose without accepting adjacent profile entries.
/// 查找一项精确操作与用途，不接受相邻配置档条目。
fn exact_operation<'a>(
    snapshot: &'a InferenceCapabilitySnapshot,
    operation: AiOperation,
    purpose_id: Option<&str>,
) -> Option<&'a InferenceOperationCapability> {
    snapshot.operations.iter().find(|candidate| {
        candidate.operation == operation && candidate.purpose_id.as_deref() == purpose_id
    })
}

/// Builds one successful route status.
/// 构建一项成功的路由状态。
fn available_capability() -> api::VceConfigurationCapabilityStatus {
    api::VceConfigurationCapabilityStatus {
        configured: true,
        available: true,
        issue: None,
    }
}

/// Builds one unavailable route status with a bounded diagnostic.
/// 构建一项带有有界诊断的不可用路由状态。
fn unavailable_capability(
    configured: bool,
    issue: impl Into<String>,
) -> api::VceConfigurationCapabilityStatus {
    api::VceConfigurationCapabilityStatus {
        configured,
        available: false,
        issue: Some(bounded_issue(issue.into())),
    }
}

/// Builds a complete fail-closed result when the profile cannot be inspected.
/// 在无法检查配置档时构建完整的失败关闭结果。
fn unavailable_readiness(issue: impl Into<String>) -> VceConfigurationReadiness {
    let issue = bounded_issue(issue.into());
    VceConfigurationReadiness {
        status: api::VceGlobalConfigurationStatus {
            ready: false,
            consumer_profile_id: String::from(VCE_CONSUMER_PROFILE_ID),
            query_rewrite_purpose_id: String::from(VCE_QUERY_REWRITE_PURPOSE_ID),
            reasoning_policy: api::VceReasoningPolicy::FixedOff,
            embedding: unavailable_capability(false, issue.clone()),
            query_rewrite: unavailable_capability(false, issue.clone()),
            rerank: unavailable_capability(false, issue.clone()),
            issues: vec![issue],
        },
        embedding_physical_fingerprint: None,
        embedding_dimension: None,
    }
}

/// Bounds one external diagnostic before it reaches project or browser state.
/// 在外部诊断进入项目或浏览器状态前限制其长度。
fn bounded_issue(issue: String) -> String {
    const MAX_ISSUE_CHARS: usize = 1_024;
    issue.chars().take(MAX_ISSUE_CHARS).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies infrastructure failures close all capability gates with one stable diagnosis.
    /// 验证基础设施故障会通过一条稳定诊断关闭全部能力闸门。
    #[test]
    fn unavailable_runtime_closes_every_capability() {
        let readiness = unavailable_readiness("runtime unavailable");

        assert!(!readiness.status.ready);
        assert!(!readiness.status.embedding.available);
        assert!(!readiness.status.query_rewrite.available);
        assert!(!readiness.status.rerank.available);
        assert_eq!(
            readiness.status.reasoning_policy,
            api::VceReasoningPolicy::FixedOff
        );
        assert_eq!(readiness.status.issues, vec!["runtime unavailable"]);
        assert!(readiness.embedding_physical_fingerprint.is_none());
    }

    /// Verifies diagnostics are bounded before being projected to persistent UI state.
    /// 验证诊断在投影到持久界面状态前受到长度限制。
    #[test]
    fn issue_projection_is_bounded() {
        let status = unavailable_capability(true, "x".repeat(2_048));

        assert_eq!(
            status.issue.expect("issue should exist").chars().count(),
            1_024
        );
    }
}
