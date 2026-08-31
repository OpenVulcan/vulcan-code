//! Regression coverage for frozen internal model bindings and matching settings-route provenance.
//! 冻结内部模型绑定与对应设置路由溯源的回归覆盖。

use super::super::build_public_ai_model_route_records;
use super::*;
use vulcan_provider_adapters::{CredentialConnection, CredentialMode};

/// Verifies internal conversation candidates consume the frozen model binding.
/// 验证内部对话候选消费冻结的模型绑定。
#[test]
fn internal_conversation_candidate_uses_model_protocol_binding() {
    // runtime supplies the signed OpenRouter registry without external network access.
    // runtime 在不访问外部网络的情况下提供已签名 OpenRouter 注册表。
    let runtime = PublicAiRuntime::unavailable().expect("runtime should initialize");
    // connection owns credential identity only; the model binding owns protocol selection.
    // connection 只持有凭据身份；模型绑定持有协议选择。
    let connection = CredentialConnection {
        id: String::from("openrouter-model-binding-account"),
        provider_id: String::from("openrouter"),
        service_entry_id: String::from("public-api"),
        credential_profile_id: String::from("api-key"),
        credential_mode: CredentialMode::Token,
        credential_id: String::from("openrouter-model-binding-credential"),
        display_name: String::from("OpenRouter model binding account"),
        secret_reference: String::from("openrouter-model-binding-secret"),
        management_secret_reference: None,
        service_origin_override: None,
        prompt_cache_by_protocol: BTreeMap::new(),
    };
    // capabilities expose one internal conversation operation through the frozen variant.
    // capabilities 通过冻结变体公开一项内部对话操作。
    let capabilities = ModelCapabilities {
        operations: vec![AiOperation::ConversationRespond],
        ..ModelCapabilities::default()
    };
    // model binds Responses independently from the credential identity, which has no protocol field.
    // model 独立于不含协议字段的凭据身份绑定 Responses。
    let model = vulcan_provider_adapters::ConnectionModel {
        id: String::from("openai/gpt-model-bound-to-responses"),
        display_name: String::from("Responses-bound model"),
        capabilities: capabilities.clone(),
        capability_evidence: vulcan_provider_adapters::ModelCapabilityEvidence::default(),
        protocol_variants: BTreeMap::from([(
            String::from(vulcan_ai_protocols::OPENAI_RESPONSES_PROFILE_ID),
            vulcan_provider_adapters::ProtocolModelVariant {
                capabilities,
                capability_evidence: vulcan_provider_adapters::ModelCapabilityEvidence::default(),
                discovery_signature: String::from("responses-bound-model-test"),
                catalog_generation: 7,
            },
        )]),
        resolved_protocol_binding: Some(vulcan_provider_adapters::ModelProtocolBinding {
            protocol_profile_id: String::from(vulcan_ai_protocols::OPENAI_RESPONSES_PROFILE_ID),
            source: vulcan_provider_adapters::ModelProtocolBindingSource::SystemManifest,
            manifest_revision: 2,
            rule_id: String::from("responses-bound-model-rule"),
        }),
    };
    // mediaCapabilities exposes only image generation and must never become a conversation row.
    // mediaCapabilities 仅公开图片生成，绝不能成为会话行。
    let media_capabilities = ModelCapabilities {
        operations: vec![AiOperation::ImageGenerate],
        ..ModelCapabilities::default()
    };
    // mediaModel deliberately owns a valid protocol binding to prove binding alone does not
    // authorize conversation publication.
    // mediaModel 有意持有有效协议绑定，以证明仅有绑定并不能授权会话发布。
    let media_model = vulcan_provider_adapters::ConnectionModel {
        id: String::from("image-generation-only"),
        display_name: String::from("Image generation only"),
        capabilities: media_capabilities.clone(),
        capability_evidence: vulcan_provider_adapters::ModelCapabilityEvidence::default(),
        protocol_variants: BTreeMap::from([(
            String::from(vulcan_ai_protocols::OPENAI_RESPONSES_PROFILE_ID),
            vulcan_provider_adapters::ProtocolModelVariant {
                capabilities: media_capabilities,
                capability_evidence: vulcan_provider_adapters::ModelCapabilityEvidence::default(),
                discovery_signature: String::from("responses-media-model-test"),
                catalog_generation: 7,
            },
        )]),
        resolved_protocol_binding: Some(vulcan_provider_adapters::ModelProtocolBinding {
            protocol_profile_id: String::from(vulcan_ai_protocols::OPENAI_RESPONSES_PROFILE_ID),
            source: vulcan_provider_adapters::ModelProtocolBindingSource::SystemManifest,
            manifest_revision: 2,
            rule_id: String::from("responses-media-model-rule"),
        }),
    };
    // configuration owns the exact credential identity used by the catalog.
    // configuration 持有目录使用的精确凭据身份。
    let mut configuration = PersistedAiConfiguration::default();
    configuration
        .connection_graph
        .add_connection(
            &runtime
                .providers()
                .expect("provider registry should resolve"),
            connection.clone(),
        )
        .expect("OpenRouter connection should register");
    // Independent switches create the exact internal-only state from the acceptance matrix.
    // 两个独立开关构造验收矩阵中的精确“仅内部使用”状态。
    configuration.set_model_enabled("openrouter", "openai/gpt-model-bound-to-responses", false);
    configuration.set_internal_model_enabled(
        "openrouter",
        "openai/gpt-model-bound-to-responses",
        true,
    );
    // catalogs contains the coherent frozen account snapshot under review.
    // catalogs 包含本次检查的一致冻结账号快照。
    let catalogs = BTreeMap::from([(
        connection.id.clone(),
        ConnectionModelCatalog {
            connection_id: connection.id,
            models: vec![model, media_model],
            generation: 7,
            page_count: 1,
            discovery_schema_revision: String::from("model-binding-test-v1"),
        },
    )]);

    // candidates must preserve Responses from the frozen model binding without deriving a connection protocol.
    // candidates 必须保留冻结模型绑定中的 Responses，不得派生连接协议。
    let candidates = build_internal_inference_candidates(
        &configuration,
        &runtime
            .providers()
            .expect("provider registry should resolve"),
        &catalogs,
    )
    .expect("internal candidates should build");
    assert_eq!(
        candidates.len(),
        2,
        "one conversation model should project both supported internal usage scopes"
    );
    assert!(
        candidates
            .iter()
            .all(|candidate| candidate.route.model_id != "image-generation-only"),
        "generation-only models must not become internal LLM candidates"
    );
    let candidate = candidates
        .iter()
        .find(|candidate| candidate.route.operation == AiOperation::ConversationRespond)
        .expect("conversation candidate should exist");
    assert_eq!(
        candidate.route.binding,
        InferenceRouteBinding::Protocol {
            protocol_profile_id: String::from(vulcan_ai_protocols::OPENAI_RESPONSES_PROFILE_ID,),
        }
    );
    assert!(
        candidate.enabled,
        "conversation disablement must not hide an explicitly enabled internal model"
    );
    configuration.set_internal_model_enabled(
        "openrouter",
        "openai/gpt-model-bound-to-responses",
        false,
    );
    // disabledCandidates proves an explicit internal denial wins over the provider default
    // without removing manageable route identities.
    // disabledCandidates 证明显式内部禁用优先于供应商默认值，且不会删除可管理路由身份。
    let disabled_candidates = build_internal_inference_candidates(
        &configuration,
        &runtime
            .providers()
            .expect("provider registry should resolve"),
        &catalogs,
    )
    .expect("disabled internal candidates should remain manageable");
    assert!(
        disabled_candidates
            .iter()
            .all(|candidate| !candidate.enabled)
    );
    // route_records must expose the same frozen protocol authority and its complete provenance.
    // route_records 必须公开同一冻结协议权威及其完整溯源。
    let route_records = build_public_ai_model_route_records(
        &configuration,
        &runtime
            .providers()
            .expect("provider registry should resolve"),
        &catalogs,
        &BTreeMap::from([(String::from("openrouter"), 2)]),
    );
    assert_eq!(route_records.len(), 1);
    // route_record is the exact settings DTO row for the tested account and model.
    // route_record 是测试账号与模型对应的精确设置 DTO 行。
    let route_record = &route_records[0];
    assert_eq!(
        route_record.protocol_id,
        Some(String::from(
            vulcan_ai_protocols::OPENAI_RESPONSES_PROFILE_ID
        ))
    );
    assert_eq!(
        route_record.available_protocols,
        vec![String::from(
            vulcan_ai_protocols::OPENAI_RESPONSES_PROFILE_ID
        )]
    );
    assert_eq!(
        route_record.binding_source.as_deref(),
        Some("system_manifest")
    );
    assert_eq!(
        route_record.binding_rule_id.as_deref(),
        Some("responses-bound-model-rule")
    );
    assert_eq!(route_record.manifest_revision, 2);
    assert!(route_record.executable);
    assert!(route_record.unavailable_reason.is_none());
}
