//! Regression coverage for admitted model-replacement backends and unrelated direct capabilities.
//! 已准入模型代替后端与无关直连能力的回归覆盖。

use super::*;
use vulcan_ai_contract::{
    AiOperation, ModelCapabilities, ResourceKind, StructuredOutputCapability,
};
use vulcan_provider_adapters::ActiveAiModelSelection;

/// Verifies a media capability can be enabled with only an admitted model-replacement route.
/// 验证媒体能力仅凭已准入的模型代替路由即可启用。
#[test]
fn media_capability_backend_accepts_model_replacement_without_provider_target() {
    // selection is the exact provider-model-account route shared by configuration and inventory.
    // selection 是配置与目录共同持有的精确供应商、模型、账号路由。
    let selection = ActiveAiModelSelection {
        provider_id: String::from("replacement-provider"),
        model_id: String::from("replacement-video-model"),
        connection_id: String::from("replacement-account"),
    };
    // configuration enables the one shared candidate switch and selects it for video understanding.
    // configuration 启用唯一共享候选开关，并将其选作视频理解后端。
    let mut configuration = PersistedAiConfiguration::default();
    configuration.set_multimodal_understanding_candidate_enabled(
        selection.provider_id.as_str(),
        selection.model_id.as_str(),
        true,
    );
    configuration.media_understanding_selections.insert(
        ProviderToolCapabilityKind::VideoUnderstand,
        MediaUnderstandingBackendSelection::ModelReplacement {
            selection: selection.clone(),
            reasoning_effort: None,
        },
    );
    // candidate proves an executable conversation route with video input and stable JSON output.
    // candidate 证明存在支持视频输入和稳定 JSON 输出的可执行会话路由。
    let candidate = PublicAiMediaUnderstandingModelCandidate {
        provider_display_name: String::from("Replacement Provider"),
        service_entry_display_name: String::from("Chat"),
        connection_display_name: String::from("Replacement Account"),
        model_display_name: String::from("Replacement Video Model"),
        selection,
        catalog_generation: 1,
        capabilities: ModelCapabilities {
            operations: vec![AiOperation::ConversationRespond],
            input_resources: vec![ResourceKind::Video],
            structured_output: StructuredOutputCapability::JsonObject,
            ..ModelCapabilities::default()
        },
    };

    assert!(provider_tool_capability_backend_is_available(
        &configuration,
        ProviderToolCapabilityKind::VideoUnderstand,
        &[],
        &[candidate],
    ));
    // search remains target-owned and cannot borrow the unrelated replacement inventory.
    // 搜索仍由目标持有，不能借用无关的模型代替目录。
    assert!(!provider_tool_capability_backend_is_available(
        &configuration,
        ProviderToolCapabilityKind::WebSearch,
        &[],
        &[],
    ));
}
