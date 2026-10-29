//! Adapter-default override validation.
//! 适配器默认覆盖校验。

use super::apply_model_parameter_defaults;
use crate::{
    MINIMAX_REASONING_SPLIT_PARAMETER_ID, ModelExecutionOverrides, parse_openai_model_catalog,
};
use vulcan_llm_core::OPENAI_CHAT_PROFILE_ID;

/// Verifies adapter-declared explicit defaults become immutable execution values.
/// 验证适配器声明的显式默认值会成为不可变执行值。
#[test]
fn explicit_model_parameter_defaults_are_applied_before_validation() {
    let mut catalog = parse_openai_model_catalog(
        "connection",
        1,
        r#"{"object":"list","data":[{"id":"MiniMax-M2.7","object":"model","created":1,"owned_by":"minimax"}]}"#,
    )
    .expect("MiniMax model fixture should parse");
    let manifests = crate::provider_manifest::load_bundled_resolved_catalog();
    let manifest = manifests
        .manifest_arc("minimax")
        .expect("MiniMax manifest must exist");
    crate::enrich_compatible_model_catalog_with_manifest(
        "minimax",
        "global",
        Some("payg-api-key"),
        OPENAI_CHAT_PROFILE_ID,
        &mut catalog,
        &manifest,
    );
    let model = catalog.models.remove(0);
    let mut overrides = ModelExecutionOverrides::default();
    apply_model_parameter_defaults(&model, OPENAI_CHAT_PROFILE_ID, &mut overrides)
        .expect("registered defaults should apply");
    assert_eq!(
        overrides
            .model_parameters
            .get(MINIMAX_REASONING_SPLIT_PARAMETER_ID),
        Some(&vulcan_ai_contract::ModelParameterValue::Boolean(true))
    );
}

/// Verifies only a model the user added to a system provider resolves its own request mapping.
/// 验证只有用户添加到系统供应商的模型会解析其自身请求映射。
#[test]
fn system_request_mapping_resolves_only_for_user_added_models() {
    let mut projection = crate::CustomModelRequestProjection::default();
    projection.reasoning.insert(
        vulcan_ai_contract::ReasoningEffort::None,
        crate::CustomRequestMutation {
            set: std::collections::BTreeMap::from([(
                String::from("thinking"),
                serde_json::json!({"type": "disabled"}),
            )]),
            delete: Vec::new(),
        },
    );
    let definition = |id: &str, system_binding| crate::CustomModelDefinition {
        id: id.to_string(),
        display_name: id.to_string(),
        enabled: true,
        capabilities: vulcan_ai_contract::ModelCapabilities::default(),
        request_projection: projection.clone(),
        tool_schema_projection: crate::ToolSchemaProjectionMode::Lightweight,
        system_binding,
    };
    let mut configuration = crate::PersistedAiConfiguration::default();
    configuration.custom_models.insert(
        String::from("qiniu"),
        std::collections::BTreeMap::from([
            (
                String::from("deepseek-new"),
                definition(
                    "deepseek-new",
                    Some(crate::SystemManualModelBinding {
                        service_entry_id: String::from("token-api"),
                        protocol_profile_id: String::from(OPENAI_CHAT_PROFILE_ID),
                    }),
                ),
            ),
            (String::from("unbound"), definition("unbound", None)),
        ]),
    );

    assert_eq!(
        super::resolve_request_projection(&configuration, "qiniu", "deepseek-new")
            .expect("projection should resolve"),
        Some(projection.clone())
    );
    assert_eq!(
        super::resolve_request_projection(&configuration, "qiniu", "unbound")
            .expect("projection should resolve"),
        None
    );
    assert_eq!(
        super::resolve_request_projection(&configuration, "qiniu", "signed-model")
            .expect("projection should resolve"),
        None
    );
}
