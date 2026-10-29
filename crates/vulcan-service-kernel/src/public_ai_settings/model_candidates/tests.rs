//! Behavior coverage for fetched-model candidates on a declared-only system provider.
//! 声明式成员系统供应商上已拉取模型候选的行为覆盖。

use super::*;
use vulcan_provider_adapters::{
    CredentialConnection, CredentialMode, ProtocolModelVariant,
    UnavailableCredentialSecretResolver, parse_openrouter_model_catalog,
    validate_system_manual_model_definition,
};

/// Connection id of the OpenRouter account fixture.
/// OpenRouter 账号夹具的连接 ID。
const ACCOUNT: &str = "openrouter-account";

/// Builds a runtime whose OpenRouter account fetched one featured and two unlisted models.
/// 构建一个运行时，其 OpenRouter 账号拉取了一个精选模型与两个未收录模型。
fn openrouter_runtime() -> (PublicAiRuntime, PersistedAiConfiguration) {
    let runtime =
        PublicAiRuntime::first_batch(std::sync::Arc::new(UnavailableCredentialSecretResolver))
            .expect("runtime should initialize");
    let mut configuration = PersistedAiConfiguration::default();
    configuration
        .connection_graph
        .add_connection(
            &runtime
                .providers()
                .expect("provider registry should resolve"),
            CredentialConnection {
                id: String::from(ACCOUNT),
                provider_id: String::from("openrouter"),
                service_entry_id: String::from("public-api"),
                credential_profile_id: String::from("api-key"),
                credential_mode: CredentialMode::Token,
                credential_id: String::from("credential-openrouter-account"),
                display_name: String::from("OpenRouter account"),
                secret_reference: String::from("secret-openrouter-account"),
                management_secret_reference: None,
                service_origin_override: None,
            },
        )
        .expect("connection should register");
    let mut catalog = parse_openrouter_model_catalog(
        ACCOUNT,
        1,
        r#"{"data":[
            {"id":"openai/gpt-5.6-terra","name":"Featured","context_length":8192,"supported_parameters":["tools"],"architecture":{"input_modalities":["text"],"output_modalities":["text"]},"top_provider":{"max_completion_tokens":4096}},
            {"id":"acme/fresh-chat","name":"Fresh Chat","context_length":64000,"supported_parameters":["tools"],"architecture":{"input_modalities":["text","image"],"output_modalities":["text"]},"top_provider":{"max_completion_tokens":96000}},
            {"id":"acme/pixel-paint","name":"Pixel Paint","context_length":4096,"supported_parameters":[],"architecture":{"input_modalities":["text"],"output_modalities":["image"]},"top_provider":{"max_completion_tokens":1024}}
        ]}"#,
    )
    .expect("catalog should parse");
    for model in &mut catalog.models {
        model.protocol_variants.insert(
            String::from(vulcan_llm_core::OPENAI_RESPONSES_PROFILE_ID),
            ProtocolModelVariant {
                capabilities: model.capabilities.clone(),
                capability_evidence: Default::default(),
                discovery_signature: String::from("test-signature"),
                catalog_generation: catalog.generation,
            },
        );
    }
    runtime
        .stage_source_catalog_for_testing(catalog)
        .expect("source catalog should stage");
    runtime
        .replace_configuration(configuration.clone())
        .expect("configuration should install");
    runtime
        .reapply_model_catalog_overlays(&configuration)
        .expect("configured test generation should publish");
    (runtime, configuration)
}

/// Verifies only unlisted fetched models are candidates and only conversation models are addable.
/// 验证只有未收录的已拉取模型是候选，且只有对话模型可添加。
#[test]
fn candidates_exclude_featured_models_and_flag_non_conversation_kinds() {
    let (runtime, configuration) = openrouter_runtime();

    let record =
        list_model_candidates(&runtime, &configuration, ACCOUNT).expect("candidates should list");

    assert_eq!(record.provider_id, "openrouter");
    assert_eq!(record.service_entry_id, "public-api");
    let summary = record
        .candidates
        .iter()
        .map(|candidate| {
            (
                candidate.model_id.as_str(),
                candidate.kind,
                candidate.addable,
            )
        })
        .collect::<Vec<_>>();
    assert_eq!(
        summary,
        [
            (
                "acme/fresh-chat",
                PublicAiModelCandidateKind::Conversation,
                true
            ),
            ("acme/pixel-paint", PublicAiModelCandidateKind::Image, false),
        ]
    );
}

/// Verifies a prepared draft carries discovery facts, passes save validation and leaves the list.
/// 验证预填草稿携带发现事实、通过保存校验，并在添加后离开候选列表。
#[test]
fn prepared_candidate_validates_and_leaves_the_list_once_added() {
    let (runtime, mut configuration) = openrouter_runtime();

    let draft = prepare_model_candidate(&runtime, &configuration, ACCOUNT, "acme/fresh-chat")
        .expect("conversation candidate should prepare");

    assert!(draft.limits_complete);
    let binding = draft
        .model
        .system_binding
        .as_ref()
        .expect("draft should bind the account node");
    assert_eq!(binding.service_entry_id, "public-api");
    assert_eq!(
        binding.protocol_profile_id,
        vulcan_llm_core::OPENAI_RESPONSES_PROFILE_ID
    );
    assert_eq!(draft.model.capabilities.context_limit, Some(64_000));
    assert_eq!(
        draft.model.capabilities.output_limit,
        Some(64_000),
        "output limit is clamped to the context limit"
    );
    assert!(
        draft
            .model
            .capabilities
            .input_resources
            .contains(&ResourceKind::Image)
    );
    let manifest = runtime
        .system_manifest("openrouter")
        .expect("openrouter manifest should resolve");
    validate_system_manual_model_definition(&manifest, &draft.model)
        .expect("prepared draft should pass the save validation");

    configuration
        .custom_models
        .entry(String::from("openrouter"))
        .or_default()
        .insert(draft.model.id.clone(), draft.model);
    let record =
        list_model_candidates(&runtime, &configuration, ACCOUNT).expect("candidates should list");
    assert!(
        record
            .candidates
            .iter()
            .all(|candidate| candidate.model_id != "acme/fresh-chat")
    );
    assert!(
        prepare_model_candidate(&runtime, &configuration, ACCOUNT, "acme/fresh-chat").is_err(),
        "an added model is no longer a candidate"
    );
}

/// Verifies featured and non-conversation models cannot be prepared as manual models.
/// 验证精选模型与非对话模型不能被预填为手工模型。
#[test]
fn featured_and_non_conversation_models_cannot_be_prepared() {
    let (runtime, configuration) = openrouter_runtime();

    for model_id in ["openai/gpt-5.6-terra", "acme/pixel-paint", "acme/unfetched"] {
        assert!(
            prepare_model_candidate(&runtime, &configuration, ACCOUNT, model_id).is_err(),
            "{model_id} must not prepare"
        );
    }
}
