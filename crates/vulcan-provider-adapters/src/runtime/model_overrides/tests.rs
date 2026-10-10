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
