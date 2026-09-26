//! Explicit limit and adapter-default override validation.
//! 显式上限与适配器默认覆盖校验。

use super::{apply_model_parameter_defaults, validate_model_overrides};
use crate::runtime::test_support::catalog;
use crate::{
    MINIMAX_REASONING_SPLIT_PARAMETER_ID, ModelExecutionOverrides,
    parse_minimax_openai_model_catalog,
};
use vulcan_llm_core::OPENAI_CHAT_PROFILE_ID;

/// Verifies an undeclared provider limit may be supplied explicitly by the user.
/// 验证供应商未声明上限时允许用户显式提供覆盖值。
#[test]
fn undeclared_model_limits_accept_explicit_overrides() {
    let mut model = catalog("connection")
        .models
        .into_iter()
        .next()
        .expect("catalog fixture should expose one model");
    model.capabilities.context_limit = None;
    model.capabilities.output_limit = None;
    let overrides = ModelExecutionOverrides {
        context_limit: Some(16_384),
        output_limit: Some(4_096),
        ..ModelExecutionOverrides::default()
    };

    validate_model_overrides(&model, OPENAI_CHAT_PROFILE_ID, &overrides)
        .expect("explicit limits should be accepted when the provider does not declare caps");
}

/// Verifies declared provider limits remain hard upper bounds for local overrides.
/// 验证供应商已声明的上限仍是本地覆盖不可突破的硬上限。
#[test]
fn declared_model_limits_reject_expansive_overrides() {
    let model = catalog("connection")
        .models
        .into_iter()
        .next()
        .expect("catalog fixture should expose one model");
    let overrides = ModelExecutionOverrides {
        context_limit: Some(16_384),
        ..ModelExecutionOverrides::default()
    };

    let error = validate_model_overrides(&model, OPENAI_CHAT_PROFILE_ID, &overrides)
        .expect_err("declared context cap must reject an expansive override");
    assert!(error.contains("context override exceeds its discovered limit"));
}

/// Verifies adapter-declared explicit defaults become immutable execution values.
/// 验证适配器声明的显式默认值会成为不可变执行值。
#[test]
fn explicit_model_parameter_defaults_are_applied_before_validation() {
    let mut catalog = parse_minimax_openai_model_catalog(
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
