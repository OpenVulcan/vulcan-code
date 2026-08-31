//! Media-understanding replacement candidates from exact model and protocol contracts.
//! 基于精确模型与协议契约的媒体理解代替候选。

use super::effective_catalog_model_protocol_id;
use serde::Serialize;
use std::collections::BTreeMap;
use vulcan_ai_contract::{
    AiOperation, ModelCapabilities, ResourceKind, StructuredOutputCapability,
};
use vulcan_core::types::ConversationAttachmentKind;
use vulcan_provider_adapters::{
    ActiveAiModelSelection, ConnectionModelCatalog, PersistedAiConfiguration,
    ProviderToolCapabilityKind,
};

/// Browser-facing exact account route eligible for model-replacement media understanding.
/// 面向浏览器且可用于模型代替媒体理解的精确账号路由。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PublicAiMediaUnderstandingModelCandidate {
    /// User-visible provider name.
    /// 用户可见供应商名称。
    pub(super) provider_display_name: String,
    /// User-visible service-entry name.
    /// 用户可见服务入口名称。
    pub(super) service_entry_display_name: String,
    /// User-visible credential connection name.
    /// 用户可见凭据连接名称。
    pub(super) connection_display_name: String,
    /// User-visible model name.
    /// 用户可见模型名称。
    pub(super) model_display_name: String,
    /// Exact immutable account and model selection persisted when chosen.
    /// 选中时持久化的精确不可变账号与模型选择。
    pub(super) selection: ActiveAiModelSelection,
    /// Process-local catalog generation that established the route.
    /// 建立该路由的进程内目录代次。
    pub(super) catalog_generation: u64,
    /// Exact model capability contract used to filter media and reasoning controls.
    /// 用于筛选媒体与思考控件的精确模型能力契约。
    pub(super) capabilities: ModelCapabilities,
}

/// Maps one media-understanding capability to its required model input resource.
/// 将一项媒体理解能力映射到模型必须支持的输入资源。
///
/// # Parameters
/// 参数
/// `capability` is the exact semantic provider-tool capability.
/// `capability` 是精确的语义供应商工具能力。
///
/// # Returns
/// 返回值
/// The required media kind, or absence for every non-understanding capability.
/// 返回所需媒体类型；非理解能力返回空。
pub(super) fn media_understanding_resource_kind(
    capability: ProviderToolCapabilityKind,
) -> Option<ResourceKind> {
    match capability {
        ProviderToolCapabilityKind::ImageUnderstand => Some(ResourceKind::Image),
        ProviderToolCapabilityKind::VideoUnderstand => Some(ResourceKind::Video),
        ProviderToolCapabilityKind::AudioUnderstand => Some(ResourceKind::Audio),
        _ => None,
    }
}

/// Reports whether one exact model route can replace a selected media-understanding backend.
/// 报告一条精确模型路由能否替代所选媒体理解后端。
///
/// # Parameters
/// 参数
/// `capabilities` is the resolved route contract and `capability` selects the media kind.
/// `capabilities` 是解析后的路由契约，`capability` 选择媒体类型。
///
/// # Returns
/// 返回值
/// `true` only for conversation routes with the required input and stable JSON output.
/// 仅对支持所需输入、会话能力及稳定 JSON 输出的路由返回 `true`。
pub(super) fn model_route_supports_media_understanding(
    capabilities: &ModelCapabilities,
    capability: ProviderToolCapabilityKind,
) -> bool {
    let Some(resource_kind) = media_understanding_resource_kind(capability) else {
        return false;
    };
    capabilities
        .operations
        .contains(&AiOperation::ConversationRespond)
        && capabilities.input_resources.contains(&resource_kind)
        && capabilities.structured_output != StructuredOutputCapability::Unsupported
}

/// Projects model media-input declarations through the encoder set of one frozen protocol route.
/// 将模型媒体输入声明投影到一条冻结协议路由实际注册的编码器集合。
///
/// # Parameters
/// 参数
/// `capabilities` is the resolved model contract and `protocol_profile_id` is its immutable route binding.
/// `capabilities` 是已解析模型契约，`protocol_profile_id` 是其不可变路由绑定。
///
/// # Returns
/// 返回值
/// A cloned capability contract whose image, video, and audio inputs are retained only when the
/// selected protocol can encode them; unrelated resource declarations are preserved unchanged.
/// 返回克隆后的能力契约；仅当所选协议可编码时保留图片、视频与音频输入，其他资源声明保持不变。
fn protocol_projected_media_capabilities(
    capabilities: &ModelCapabilities,
    protocol_profile_id: &str,
) -> ModelCapabilities {
    // projected prevents model metadata from claiming a media route that the selected wire protocol cannot carry.
    // projected 防止模型元数据声明所选 Wire 协议无法承载的媒体路由。
    let mut projected = capabilities.clone();
    projected.input_resources.retain(|resource_kind| {
        let attachment_kind = match resource_kind {
            ResourceKind::Image => Some(ConversationAttachmentKind::Image),
            ResourceKind::Video => Some(ConversationAttachmentKind::Video),
            ResourceKind::Audio => Some(ConversationAttachmentKind::Audio),
            ResourceKind::File => None,
        };
        attachment_kind.is_none_or(|attachment_kind| {
            vulcan_agent_runtime::provider_input_normalizer::protocol_profile_supports_attachment_kind(
                protocol_profile_id,
                attachment_kind,
            )
        })
    });
    if !vulcan_agent_runtime::provider_input_normalizer::protocol_profile_supports_structured_output(
        protocol_profile_id,
        projected.structured_output,
    ) {
        // Unsupported is the only honest projection when the frozen encoder cannot represent the model policy.
        // 当冻结编码器无法表达模型策略时，Unsupported 是唯一诚实的投影结果。
        projected.structured_output = StructuredOutputCapability::Unsupported;
    }
    projected
}

/// Builds every executable account route eligible for at least one media-understanding kind.
/// 构建至少符合一种媒体理解类型的全部可执行账号路由。
///
/// # Parameters
/// 参数
/// `configuration` owns credentials, `providers` owns labels, and `catalogs` owns effective routes.
/// `configuration` 持有凭据，`providers` 持有展示名，`catalogs` 持有生效路由。
///
/// # Returns
/// 返回值
/// Stable sorted route candidates independent from conversation publication switches.
/// 返回与会话发布开关独立、稳定排序的路由候选。
pub(super) fn build_media_understanding_model_candidates(
    configuration: &PersistedAiConfiguration,
    providers: &vulcan_provider_adapters::ProviderRegistry,
    catalogs: &BTreeMap<String, ConnectionModelCatalog>,
) -> Result<Vec<PublicAiMediaUnderstandingModelCandidate>, String> {
    let mut candidates = Vec::new();
    for connection in configuration.connection_graph.connections.values() {
        if configuration.connection_is_pending_validation(connection.id.as_str()) {
            continue;
        }
        let Some(catalog) = catalogs.get(connection.id.as_str()) else {
            continue;
        };
        let provider = providers
            .provider(connection.provider_id.as_str())
            .ok_or_else(|| {
                format!(
                    "Provider `{}` referenced by connection `{}` is not registered.",
                    connection.provider_id, connection.id
                )
            })?;
        let entry = provider
            .service_entries
            .iter()
            .find(|entry| entry.id == connection.service_entry_id)
            .ok_or_else(|| {
                format!(
                    "Service entry `{}` referenced by connection `{}` is not registered.",
                    connection.service_entry_id, connection.id
                )
            })?;
        for model in &catalog.models {
            let Some(protocol_profile_id) = effective_catalog_model_protocol_id(model) else {
                continue;
            };
            // candidateCapabilities intersects model declarations with the exact protocol encoder set.
            // candidateCapabilities 将模型声明与精确协议编码器集合取交集。
            let candidate_capabilities =
                protocol_projected_media_capabilities(&model.capabilities, protocol_profile_id);
            let supports_any_media = [
                ProviderToolCapabilityKind::ImageUnderstand,
                ProviderToolCapabilityKind::VideoUnderstand,
                ProviderToolCapabilityKind::AudioUnderstand,
            ]
            .into_iter()
            .any(|capability| {
                model_route_supports_media_understanding(&candidate_capabilities, capability)
            });
            if !supports_any_media {
                continue;
            }
            candidates.push(PublicAiMediaUnderstandingModelCandidate {
                provider_display_name: provider.display_name.clone(),
                service_entry_display_name: entry.display_name.clone(),
                connection_display_name: connection.display_name.clone(),
                model_display_name: model.display_name.clone(),
                selection: ActiveAiModelSelection {
                    provider_id: connection.provider_id.clone(),
                    model_id: model.id.clone(),
                    connection_id: connection.id.clone(),
                },
                catalog_generation: catalog.generation,
                capabilities: candidate_capabilities,
            });
        }
    }
    candidates.sort_by(|left, right| {
        (
            left.provider_display_name.as_str(),
            left.model_display_name.as_str(),
            left.connection_display_name.as_str(),
        )
            .cmp(&(
                right.provider_display_name.as_str(),
                right.model_display_name.as_str(),
                right.connection_display_name.as_str(),
            ))
    });
    Ok(candidates)
}

#[cfg(test)]
mod tests;
