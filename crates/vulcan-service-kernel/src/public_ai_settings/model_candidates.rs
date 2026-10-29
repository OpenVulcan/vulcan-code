//! Fetched system-provider models that the signed catalog does not include yet.
//! 签名目录尚未收录的已拉取系统供应商模型。

use serde::{Deserialize, Serialize};
use vulcan_ai_contract::{AiOperation, ModelCapabilities, ResourceKind};
use vulcan_provider_adapters::{
    ConnectionModel, CustomModelDefinition, CustomModelRequestProjection, PersistedAiConfiguration,
    ProtocolRoute, PublicAiRuntime, ServiceEntryKind, SystemManualModelBinding,
    ToolSchemaProjectionMode, apply_common_model_capability_defaults,
    provider_manifest::ResolvedProviderManifest, validate_route_input_resources,
};

use super::{public_ai_runtime_and_configuration, validate_non_empty};
use crate::SharedServiceBridgeState;

/// Requests the fetched-but-not-added models of one system-provider account.
/// 请求一个系统供应商账号中已拉取但尚未添加的模型。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PublicAiModelCandidatesRequest {
    /// Exact credential connection whose discovery result is inspected.
    /// 被检查发现结果的精确凭据连接。
    connection_id: String,
}

/// Requests one prefilled model definition for a fetched candidate.
/// 为一个已拉取候选请求一份预填的模型定义。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PublicAiModelCandidatePrepareRequest {
    /// Exact credential connection that fetched the candidate.
    /// 拉取该候选的精确凭据连接。
    connection_id: String,
    /// Exact upstream model identifier.
    /// 精确上游模型标识。
    model_id: String,
}

/// Detected model category used to decide whether a candidate can be added as a conversation model.
/// 检测出的模型类别，用于判断候选能否作为对话模型添加。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PublicAiModelCandidateKind {
    /// Text conversation model.
    /// 文本对话模型。
    Conversation,
    /// Image generation or editing model.
    /// 图片生成或编辑模型。
    Image,
    /// Video generation, editing or extension model.
    /// 视频生成、编辑或延展模型。
    Video,
    /// Embedding model.
    /// 嵌入模型。
    Embedding,
    /// Rerank model.
    /// 重排模型。
    Rerank,
    /// Speech, music or other audio model.
    /// 语音、音乐或其他音频模型。
    Audio,
    /// No operation could be recognized; the user decides.
    /// 无法识别任何操作，由用户决定。
    Unknown,
}

/// One fetched model that is neither featured nor added by the user.
/// 一个既非精选也未被用户添加的已拉取模型。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PublicAiModelCandidate {
    /// Exact upstream model identifier.
    /// 精确上游模型标识。
    model_id: String,
    /// Upstream display name.
    /// 上游显示名称。
    display_name: String,
    /// Detected model category.
    /// 检测出的模型类别。
    kind: PublicAiModelCandidateKind,
    /// Whether the candidate can currently be added as a conversation model.
    /// 候选当前能否作为对话模型添加。
    addable: bool,
}

/// Fetched candidates of one account with the node they would be added to.
/// 一个账号的已拉取候选及其将被添加到的节点。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PublicAiModelCandidatesRecord {
    /// System provider identifier.
    /// 系统供应商标识。
    provider_id: String,
    /// Service entry (node) that owns added models.
    /// 持有已添加模型的服务入口（节点）。
    service_entry_id: String,
    /// Candidates sorted by model identifier.
    /// 按模型标识排序的候选。
    candidates: Vec<PublicAiModelCandidate>,
}

/// Prefilled model definition the user confirms before saving it with the custom-model service.
/// 用户确认后通过自定义模型服务保存的预填模型定义。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PublicAiModelCandidateDraft {
    /// System provider identifier used by the save request.
    /// 保存请求使用的系统供应商标识。
    provider_id: String,
    /// Draft definition bound to the account's node and one verified protocol route.
    /// 绑定账号节点与一条已验证协议路由的草稿定义。
    model: CustomModelDefinition,
    /// Whether the offline common-model table matched this model exactly.
    /// 离线公共模型表是否精确命中此模型。
    common_template_matched: bool,
    /// Whether both limits are known; otherwise the user must enter them before saving.
    /// 两个上限是否都已知；否则用户必须在保存前填写。
    limits_complete: bool,
}

/// Lists fetched models of one system-provider account that are not in its catalog yet.
/// 列出一个系统供应商账号中尚未进入目录的已拉取模型。
///
/// # Parameters
/// 参数
/// `state` owns the runtime catalogs and persisted configuration; `request` names the account.
/// `state` 持有运行时目录与持久化配置；`request` 指定账号。
///
/// # Returns
/// 返回值
/// The account's node and its candidates; empty when the provider publishes no discovery list.
/// 返回账号节点及其候选；供应商不提供发现列表时为空。
pub(crate) fn service_list_public_ai_model_candidates(
    state: SharedServiceBridgeState,
    request: PublicAiModelCandidatesRequest,
) -> Result<PublicAiModelCandidatesRecord, String> {
    validate_non_empty("connectionId", request.connection_id.as_str())?;
    let (runtime, configuration) = public_ai_runtime_and_configuration(&state)?;
    list_model_candidates(&runtime, &configuration, request.connection_id.as_str())
}

/// Lists the candidates of one account against an explicit runtime and configuration.
/// 针对显式运行时与配置列出一个账号的候选。
fn list_model_candidates(
    runtime: &PublicAiRuntime,
    configuration: &PersistedAiConfiguration,
    connection_id: &str,
) -> Result<PublicAiModelCandidatesRecord, String> {
    let account = system_account(runtime, configuration, connection_id)?;
    let source_catalogs = runtime.source_catalog_snapshot()?;
    let mut candidates = Vec::new();
    if let Some(catalog) = source_catalogs.get(connection_id) {
        for model in &catalog.models {
            if !is_candidate(runtime, configuration, &account, model.id.as_str())? {
                continue;
            }
            let kind = detected_kind(model.id.as_str(), &model.capabilities);
            candidates.push(PublicAiModelCandidate {
                model_id: model.id.clone(),
                display_name: model.display_name.clone(),
                kind,
                addable: kind == PublicAiModelCandidateKind::Conversation
                    && candidate_route(&account, model).is_some(),
            });
        }
    }
    candidates.sort_by(|left, right| left.model_id.cmp(&right.model_id));
    Ok(PublicAiModelCandidatesRecord {
        provider_id: account.provider_id,
        service_entry_id: account.service_entry_id,
        candidates,
    })
}

/// Builds the prefilled definition of one fetched candidate without persisting anything.
/// 为一个已拉取候选构建预填定义，不做任何持久化。
///
/// # Parameters
/// 参数
/// `state` owns the runtime catalogs and persisted configuration; `request` names the candidate.
/// `state` 持有运行时目录与持久化配置；`request` 指定候选。
///
/// # Returns
/// 返回值
/// A draft bound to the account's node, with discovery facts first and the common table second.
/// 返回绑定账号节点的草稿，发现事实优先，公共表其次。
pub(crate) fn service_prepare_public_ai_model_candidate(
    state: SharedServiceBridgeState,
    request: PublicAiModelCandidatePrepareRequest,
) -> Result<PublicAiModelCandidateDraft, String> {
    validate_non_empty("connectionId", request.connection_id.as_str())?;
    validate_non_empty("modelId", request.model_id.as_str())?;
    let (runtime, configuration) = public_ai_runtime_and_configuration(&state)?;
    prepare_model_candidate(
        &runtime,
        &configuration,
        request.connection_id.as_str(),
        request.model_id.as_str(),
    )
}

/// Builds one candidate draft against an explicit runtime and configuration.
/// 针对显式运行时与配置构建一个候选草稿。
fn prepare_model_candidate(
    runtime: &PublicAiRuntime,
    configuration: &PersistedAiConfiguration,
    connection_id: &str,
    model_id: &str,
) -> Result<PublicAiModelCandidateDraft, String> {
    let account = system_account(runtime, configuration, connection_id)?;
    let source_catalogs = runtime.source_catalog_snapshot()?;
    let model = source_catalogs
        .get(connection_id)
        .and_then(|catalog| catalog.models.iter().find(|model| model.id == model_id))
        .ok_or_else(|| {
            format!("Connection `{connection_id}` has not fetched model `{model_id}`.")
        })?;
    if !is_candidate(runtime, configuration, &account, model.id.as_str())? {
        return Err(format!(
            "Model `{}` is already in the catalog of provider `{}`.",
            model.id, account.provider_id
        ));
    }
    let kind = detected_kind(model.id.as_str(), &model.capabilities);
    if kind != PublicAiModelCandidateKind::Conversation {
        return Err(format!(
            "Model `{}` is not a conversation model and cannot be added yet.",
            model.id
        ));
    }
    let route = candidate_route(&account, model).ok_or_else(|| {
        format!(
            "Service entry `{}` has no verified protocol route for model `{}`.",
            account.service_entry_id, model.id
        )
    })?;
    let (capabilities, common_template_matched) =
        draft_capabilities(&account.service_entry_id, route, model);
    let limits_complete = capabilities.context_limit.is_some_and(|limit| limit > 0)
        && capabilities.output_limit.is_some_and(|limit| limit > 0);
    Ok(PublicAiModelCandidateDraft {
        provider_id: account.provider_id.clone(),
        model: CustomModelDefinition {
            id: model.id.clone(),
            display_name: model.display_name.clone(),
            enabled: true,
            capabilities,
            request_projection: CustomModelRequestProjection::default(),
            tool_schema_projection: ToolSchemaProjectionMode::default(),
            system_binding: Some(SystemManualModelBinding {
                service_entry_id: account.service_entry_id.clone(),
                protocol_profile_id: route.protocol_profile_id.clone(),
            }),
        },
        common_template_matched,
        limits_complete,
    })
}

/// One system-provider account resolved against its signed manifest.
/// 针对签名 Manifest 解析出的一个系统供应商账号。
struct SystemAccount {
    /// System provider identifier.
    /// 系统供应商标识。
    provider_id: String,
    /// Service entry (node) of the account.
    /// 账号所属服务入口（节点）。
    service_entry_id: String,
    /// Signed manifest of the provider.
    /// 供应商的签名 Manifest。
    manifest: std::sync::Arc<ResolvedProviderManifest>,
}

/// Resolves a connection to its system provider and conversation node.
/// 把连接解析到其系统供应商与对话节点。
fn system_account(
    runtime: &PublicAiRuntime,
    configuration: &PersistedAiConfiguration,
    connection_id: &str,
) -> Result<SystemAccount, String> {
    let connection = configuration
        .connection_graph
        .connections
        .get(connection_id)
        .ok_or_else(|| format!("Connection `{connection_id}` is not configured."))?;
    let manifest = runtime
        .system_manifest(connection.provider_id.as_str())
        .ok_or_else(|| {
            format!(
                "Connection `{connection_id}` does not belong to a system provider; custom providers add models directly."
            )
        })?;
    let entry_is_conversation = manifest.descriptor.service_entries.iter().any(|entry| {
        entry.id == connection.service_entry_id && entry.kind == ServiceEntryKind::Conversation
    });
    if !entry_is_conversation {
        return Err(format!(
            "Service entry `{}` of connection `{connection_id}` does not accept conversation models.",
            connection.service_entry_id
        ));
    }
    Ok(SystemAccount {
        provider_id: connection.provider_id.clone(),
        service_entry_id: connection.service_entry_id.clone(),
        manifest,
    })
}

/// Reports whether a fetched model is outside every catalog the provider already exposes.
/// 报告已拉取模型是否不在供应商已公开的任何目录中。
///
/// Signed identities are excluded even when the plan filters them out: the manifest owns them.
/// 即使套餐把签名模型过滤掉，它们也不进入候选：其归属是 Manifest。
fn is_candidate(
    runtime: &PublicAiRuntime,
    configuration: &PersistedAiConfiguration,
    account: &SystemAccount,
    model_id: &str,
) -> Result<bool, String> {
    if account.manifest.has_signed_exact_model_identity(model_id)
        || configuration
            .custom_model(account.provider_id.as_str(), model_id)
            .is_some()
    {
        return Ok(false);
    }
    Ok(!runtime.provider_model_is_supported(
        configuration,
        account.provider_id.as_str(),
        model_id,
    )?)
}

/// Picks the verified route for a candidate: the discovery binding, then a discovered protocol,
/// then the node's first verified route.
/// 为候选选择已验证路由：先用发现绑定，再用已发现协议，最后用节点的首条已验证路由。
fn candidate_route<'a>(
    account: &'a SystemAccount,
    model: &ConnectionModel,
) -> Option<&'a ProtocolRoute> {
    let entry = account
        .manifest
        .descriptor
        .service_entries
        .iter()
        .find(|entry| entry.id == account.service_entry_id)?;
    let verified = |protocol_profile_id: &str| {
        entry
            .protocol_routes
            .iter()
            .find(|route| route.verified && route.protocol_profile_id == protocol_profile_id)
    };
    model
        .resolved_protocol_binding
        .as_ref()
        .and_then(|binding| verified(binding.protocol_profile_id.as_str()))
        .or_else(|| {
            model
                .protocol_variants
                .keys()
                .find_map(|protocol_profile_id| verified(protocol_profile_id.as_str()))
        })
        .or_else(|| entry.protocol_routes.iter().find(|route| route.verified))
}

/// Builds draft capabilities from discovery, then the common table, trimmed to what the route encodes.
/// 先用发现结果、再用公共表构建草稿能力，并裁剪为路由可编码的范围。
fn draft_capabilities(
    service_entry_id: &str,
    route: &ProtocolRoute,
    model: &ConnectionModel,
) -> (ModelCapabilities, bool) {
    let mut capabilities = model
        .resolved_protocol_binding
        .as_ref()
        .and_then(|binding| model.protocol_variants.get(&binding.protocol_profile_id))
        .or_else(|| model.protocol_variants.get(&route.protocol_profile_id))
        .map(|variant| variant.capabilities.clone())
        .unwrap_or_else(|| model.capabilities.clone());
    let common_template_matched =
        apply_common_model_capability_defaults(model.id.as_str(), &mut capabilities);
    capabilities.operations = vec![AiOperation::ConversationRespond];
    capabilities.input_resources.retain(|resource| {
        validate_route_input_resources(
            service_entry_id,
            route,
            std::slice::from_ref(resource),
            model.id.as_str(),
        )
        .is_ok()
    });
    if let (Some(context), Some(output)) = (capabilities.context_limit, capabilities.output_limit) {
        capabilities.output_limit = Some(output.min(context));
    }
    (capabilities, common_template_matched)
}

/// Detects the model category from discovery facts refined by the common table.
/// 根据发现事实并结合公共表检测模型类别。
fn detected_kind(model_id: &str, discovered: &ModelCapabilities) -> PublicAiModelCandidateKind {
    let mut capabilities = discovered.clone();
    apply_common_model_capability_defaults(model_id, &mut capabilities);
    let has = |operations: &[AiOperation]| {
        capabilities
            .operations
            .iter()
            .any(|operation| operations.contains(operation))
    };
    if has(&[AiOperation::ConversationRespond]) {
        PublicAiModelCandidateKind::Conversation
    } else if has(&[AiOperation::ImageGenerate, AiOperation::ImageEdit])
        || capabilities.output_resources.contains(&ResourceKind::Image)
    {
        PublicAiModelCandidateKind::Image
    } else if has(&[
        AiOperation::VideoGenerate,
        AiOperation::VideoEdit,
        AiOperation::VideoExtend,
    ]) || capabilities.output_resources.contains(&ResourceKind::Video)
    {
        PublicAiModelCandidateKind::Video
    } else if has(&[AiOperation::EmbeddingCreate]) || capabilities.embedding.is_some() {
        PublicAiModelCandidateKind::Embedding
    } else if has(&[AiOperation::RerankDocuments]) || capabilities.rerank.is_some() {
        PublicAiModelCandidateKind::Rerank
    } else if has(&[
        AiOperation::SpeechSynthesize,
        AiOperation::SpeechTranscribe,
        AiOperation::MusicGenerate,
        AiOperation::MusicCover,
    ]) || capabilities.output_resources.contains(&ResourceKind::Audio)
    {
        PublicAiModelCandidateKind::Audio
    } else {
        PublicAiModelCandidateKind::Unknown
    }
}

#[cfg(test)]
mod tests;
