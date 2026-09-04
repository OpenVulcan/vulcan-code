//! Authoritative fail-closed configuration gate for Vulcan Context Engine.
//! Vulcan Context Engine 的权威失败关闭配置闸门。

use std::collections::BTreeSet;
use std::sync::Arc;

use vulcan_ai_contract::{
    AiOperation, InferenceCallerBudget, InferenceCallerContext, InferenceCapabilitySnapshot,
    InferenceOperationCapability, InferencePermission, InferenceUsageScope, RerankTruncation,
    StructuredOutputCapability,
};
use vulcan_provider_adapters::PublicAiRuntime;
use vulcan_service_api as api;

use crate::{SharedServiceBridgeState, configured_inference::ConfiguredInferenceService};

use super::{
    VCE_CONSUMER_PROFILE_ID, VCE_QUERY_REWRITE_PURPOSE_ID, vce_embedding_physical_fingerprint,
};

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
        "VCE 全局模型配置未就绪：{}",
        readiness.status.issues.join("；")
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
    project.vce.enabled
        && guard.vce_runtime.as_ref().is_some_and(|manager| {
            manager.project_can_serve_now(project_id, project.vce.revision, expected_fingerprint)
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
            "公共 AI 中尚未配置 `{VCE_CONSUMER_PROFILE_ID}` 推理配置档。"
        )));
    };
    let snapshot = configured_inference
        .capability_snapshot(&vce_status_caller(), VCE_CONSUMER_PROFILE_ID)
        .map_err(|error| error.to_string())?;
    let embedding = evaluate_embedding(profile.embedding.is_some(), &snapshot);
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
        .ok_or_else(|| String::from("公共 AI 推理运行时尚未启动。"))?;
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

/// Evaluates the required document-and-query dense-float embedding route.
/// 评估必需的文档与查询稠密浮点向量路由。
fn evaluate_embedding(
    configured: bool,
    snapshot: &InferenceCapabilitySnapshot,
) -> (api::VceConfigurationCapabilityStatus, Option<String>) {
    if !configured {
        return (
            unavailable_capability(false, "VCE Embedding 路由未配置。"),
            None,
        );
    }
    let Some(operation) = exact_operation(snapshot, AiOperation::EmbeddingCreate, None) else {
        return (
            unavailable_capability(true, "VCE Embedding 能力快照缺失。"),
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
                    .unwrap_or("VCE Embedding 路由当前不可用。"),
            ),
            None,
        );
    }
    match vce_embedding_physical_fingerprint(snapshot) {
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
        return unavailable_capability(false, "VCE 查询改写 LLM 路由未配置。");
    };
    let Some(operation) = exact_operation(
        snapshot,
        AiOperation::ConversationRespond,
        Some(VCE_QUERY_REWRITE_PURPOSE_ID),
    ) else {
        return unavailable_capability(true, "VCE 查询改写 LLM 能力快照缺失。");
    };
    if !operation.available {
        return unavailable_capability(
            true,
            operation
                .unavailable_reason
                .as_deref()
                .unwrap_or("VCE 查询改写 LLM 路由当前不可用。"),
        );
    }
    let selection = match runtime.resolve_internal_inference_selection(configuration, route) {
        Ok(selection) => selection,
        Err(error) => return unavailable_capability(true, error),
    };
    if selection.model.capabilities.structured_output == StructuredOutputCapability::Unsupported {
        return unavailable_capability(true, "VCE 查询改写要求模型支持可由宿主校验的 JSON 输出。");
    }
    if !crate::configured_inference::model_supports_disabled_reasoning(
        &selection.model.capabilities,
    ) {
        return unavailable_capability(true, "VCE 查询改写要求模型能够明确关闭推理。");
    }
    if let Err(error) = runtime.validate_disabled_reasoning_route_projection(&selection) {
        return unavailable_capability(true, format!("VCE 查询改写无法投影关闭推理请求：{error}"));
    }
    available_capability()
}

/// Evaluates the required rerank route without allowing provider-side truncation.
/// 评估必需的重排路由，且不允许 Provider 侧截断。
fn evaluate_rerank(
    configured: bool,
    snapshot: &InferenceCapabilitySnapshot,
) -> api::VceConfigurationCapabilityStatus {
    if !configured {
        return unavailable_capability(false, "VCE Rerank 路由未配置。");
    }
    let Some(operation) = exact_operation(snapshot, AiOperation::RerankDocuments, None) else {
        return unavailable_capability(true, "VCE Rerank 能力快照缺失。");
    };
    if !operation.available {
        return unavailable_capability(
            true,
            operation
                .unavailable_reason
                .as_deref()
                .unwrap_or("VCE Rerank 路由当前不可用。"),
        );
    }
    let Some(capabilities) = operation.rerank.as_ref() else {
        return unavailable_capability(true, "VCE Rerank 模型缺少强类型能力声明。");
    };
    if !capabilities
        .truncation
        .contains(&RerankTruncation::Disabled)
    {
        return unavailable_capability(true, "VCE Rerank 模型不支持禁用截断。");
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
