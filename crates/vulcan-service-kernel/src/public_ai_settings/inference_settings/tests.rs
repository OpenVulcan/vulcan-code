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
            String::from(vulcan_llm_core::OPENAI_RESPONSES_PROFILE_ID),
            vulcan_provider_adapters::ProtocolModelVariant {
                capabilities,
                capability_evidence: vulcan_provider_adapters::ModelCapabilityEvidence::default(),
                discovery_signature: String::from("responses-bound-model-test"),
                catalog_generation: 7,
            },
        )]),
        resolved_protocol_binding: Some(vulcan_provider_adapters::ModelProtocolBinding {
            protocol_profile_id: String::from(vulcan_llm_core::OPENAI_RESPONSES_PROFILE_ID),
            source: vulcan_provider_adapters::ModelProtocolBindingSource::SystemManifest,
            manifest_revision: 2,
            rule_id: String::from("responses-bound-model-rule"),
            codex_behavior: None,
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
            String::from(vulcan_llm_core::OPENAI_RESPONSES_PROFILE_ID),
            vulcan_provider_adapters::ProtocolModelVariant {
                capabilities: media_capabilities,
                capability_evidence: vulcan_provider_adapters::ModelCapabilityEvidence::default(),
                discovery_signature: String::from("responses-media-model-test"),
                catalog_generation: 7,
            },
        )]),
        resolved_protocol_binding: Some(vulcan_provider_adapters::ModelProtocolBinding {
            protocol_profile_id: String::from(vulcan_llm_core::OPENAI_RESPONSES_PROFILE_ID),
            source: vulcan_provider_adapters::ModelProtocolBindingSource::SystemManifest,
            manifest_revision: 2,
            rule_id: String::from("responses-media-model-rule"),
            codex_behavior: None,
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
            protocol_profile_id: String::from(vulcan_llm_core::OPENAI_RESPONSES_PROFILE_ID,),
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
        Some(String::from(vulcan_llm_core::OPENAI_RESPONSES_PROFILE_ID))
    );
    assert_eq!(
        route_record.available_protocols,
        vec![String::from(vulcan_llm_core::OPENAI_RESPONSES_PROFILE_ID)]
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

/// Verifies clearing a never-saved LuaSkills route is skipped instead of creating a profile.
/// 验证清除从未保存过的 LuaSkills 路由会被跳过，而不是创建一个 Profile。
///
/// LuaSkills is an optional consumer: its profile must exist only because the user
/// explicitly saved a real route.
/// LuaSkills 是可选消费者：其 Profile 必须只因为用户显式保存过真实路由而存在。
#[test]
fn luaskills_clear_without_profile_skips_creation() {
    let profiles: BTreeMap<String, InferenceProfile> = BTreeMap::new();
    assert!(luaskills_clear_skips_profile_creation(
        &profiles,
        crate::luaskills_model::LUASKILLS_CONSUMER_PROFILE_ID,
        true
    ));
}

/// Verifies saving a LuaSkills route is never skipped, so the first save still creates the profile.
/// 验证保存 LuaSkills 路由绝不跳过，使首次保存仍会创建 Profile。
#[test]
fn luaskills_route_save_is_never_skipped() {
    let profiles: BTreeMap<String, InferenceProfile> = BTreeMap::new();
    assert!(!luaskills_clear_skips_profile_creation(
        &profiles,
        crate::luaskills_model::LUASKILLS_CONSUMER_PROFILE_ID,
        false
    ));
}

/// Verifies other consumers keep the existing entry-or-default behavior.
/// 验证其他消费者保持既有 entry-or-default 行为。
///
/// Their empty profiles are an existing contract this change must not alter.
/// 它们的空 Profile 是本改动不得更改的既有契约。
#[test]
fn other_consumers_keep_entry_or_default_behavior() {
    let profiles: BTreeMap<String, InferenceProfile> = BTreeMap::new();
    assert!(!luaskills_clear_skips_profile_creation(
        &profiles, "vmm", true
    ));
    assert!(!luaskills_clear_skips_profile_creation(
        &profiles,
        "test-consumer",
        true
    ));
}

/// Verifies clearing the last LuaSkills route removes the now-empty profile.
/// 验证清除最后一条 LuaSkills 路由会移除已空的 Profile。
#[test]
fn emptied_luaskills_profile_is_removed() {
    let mut profiles: BTreeMap<String, InferenceProfile> = BTreeMap::new();
    profiles.insert(
        String::from(crate::luaskills_model::LUASKILLS_CONSUMER_PROFILE_ID),
        InferenceProfile::default(),
    );
    remove_emptied_luaskills_profile(
        &mut profiles,
        crate::luaskills_model::LUASKILLS_CONSUMER_PROFILE_ID,
    );
    assert!(
        !profiles.contains_key(crate::luaskills_model::LUASKILLS_CONSUMER_PROFILE_ID),
        "an explicitly cleared consumer must leave no empty profile behind"
    );
}

/// Verifies a LuaSkills profile that still owns a route is retained.
/// 验证仍持有路由的 LuaSkills Profile 会被保留。
#[test]
fn luaskills_profile_with_remaining_state_is_retained() {
    let mut profiles: BTreeMap<String, InferenceProfile> = BTreeMap::new();
    // embeddingDimensions is the cheapest non-empty field and stands for any retained
    // route state, because emptiness requires all five fields to be empty together.
    // embeddingDimensions 是最廉价的非空字段，代表任何保留的路由状态——因为只有五个字段
    // 同时为空才算空。
    profiles.insert(
        String::from(crate::luaskills_model::LUASKILLS_CONSUMER_PROFILE_ID),
        InferenceProfile {
            embedding_dimensions: Some(1_536),
            ..InferenceProfile::default()
        },
    );
    remove_emptied_luaskills_profile(
        &mut profiles,
        crate::luaskills_model::LUASKILLS_CONSUMER_PROFILE_ID,
    );
    assert!(profiles.contains_key(crate::luaskills_model::LUASKILLS_CONSUMER_PROFILE_ID));
}

/// Verifies an empty profile of another consumer is left exactly as it was.
/// 验证其他消费者的空 Profile 保持原样。
#[test]
fn other_consumer_profiles_are_never_removed() {
    let mut profiles: BTreeMap<String, InferenceProfile> = BTreeMap::new();
    profiles.insert(String::from("test-consumer"), InferenceProfile::default());
    remove_emptied_luaskills_profile(&mut profiles, "test-consumer");
    assert!(
        profiles.contains_key("test-consumer"),
        "removal is scoped to LuaSkills so no other consumer semantics change"
    );
}

/// Builds one exact route for validating the LuaSkills service-side boundary.
/// 构造一条用于验证 LuaSkills 服务端边界的精确路由。
///
/// # Parameters
/// 参数
/// `operation` and `usage_scope` select the two route axes under test.
/// `operation` 与 `usage_scope` 选择被测路由的两个维度。
///
/// # Returns
/// 返回值
/// Returns a stable synthetic route that does not require a live Provider registry.
/// 返回一条不依赖实时 Provider 注册表的稳定合成路由。
fn luaskills_test_route(
    operation: AiOperation,
    usage_scope: InferenceUsageScope,
) -> InferenceRouteConfiguration {
    InferenceRouteConfiguration {
        provider_id: String::from("fixture-provider"),
        service_entry_id: String::from("fixture-entry"),
        connection_id: String::from("fixture-connection"),
        model_id: String::from("fixture-model"),
        binding: InferenceRouteBinding::Action {
            action_binding_id: String::from("fixture-action"),
        },
        operation,
        usage_scope,
    }
}

/// Verifies the LuaSkills profile accepts only its two declared internal-background routes.
/// 验证 LuaSkills Profile 只接受其声明的两条内部后台路由。
#[test]
fn luaskills_route_boundary_rejects_forged_purposes_operations_and_scopes() {
    let llm = PublicAiInferenceRouteSaveRequest {
        consumer_profile_id: String::from(crate::luaskills_model::LUASKILLS_CONSUMER_PROFILE_ID),
        purpose_id: Some(String::from(
            crate::luaskills_model::LUASKILLS_TEMPORARY_LLM_PURPOSE_ID,
        )),
        operation: AiOperation::ConversationRespond,
        route: Some(luaskills_test_route(
            AiOperation::ConversationRespond,
            InferenceUsageScope::InternalBackground,
        )),
        reasoning_effort: None,
    };
    assert!(validate_luaskills_inference_route_request(&llm).is_ok());

    let embedding = PublicAiInferenceRouteSaveRequest {
        purpose_id: None,
        operation: AiOperation::EmbeddingCreate,
        route: Some(luaskills_test_route(
            AiOperation::EmbeddingCreate,
            InferenceUsageScope::InternalBackground,
        )),
        ..llm.clone()
    };
    assert!(validate_luaskills_inference_route_request(&embedding).is_ok());

    let wrong_purpose = PublicAiInferenceRouteSaveRequest {
        purpose_id: Some(String::from("forged-purpose")),
        ..llm.clone()
    };
    assert!(validate_luaskills_inference_route_request(&wrong_purpose).is_err());

    let rerank = PublicAiInferenceRouteSaveRequest {
        purpose_id: None,
        operation: AiOperation::RerankDocuments,
        route: None,
        ..llm.clone()
    };
    assert!(validate_luaskills_inference_route_request(&rerank).is_err());

    let external = PublicAiInferenceRouteSaveRequest {
        route: Some(luaskills_test_route(
            AiOperation::ConversationRespond,
            InferenceUsageScope::ExternalService,
        )),
        ..llm
    };
    assert!(validate_luaskills_inference_route_request(&external).is_err());

    let other_consumer = PublicAiInferenceRouteSaveRequest {
        consumer_profile_id: String::from("other-consumer"),
        ..external
    };
    assert!(validate_luaskills_inference_route_request(&other_consumer).is_ok());
}
