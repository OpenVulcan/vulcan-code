//! Manual-source membership and confirmed-template projection regressions.
//! 手工来源成员与已确认模板投影回归测试。

use super::*;

/// Verifies an accepted custom-model template remains executable when discovery is unavailable after restart.
/// 验证重启后模型发现不可用时，已接受的自定义模型模板仍可执行。
#[test]
fn confirmed_template_builds_manual_catalog_fallback_without_discovery() {
    let provider_id = "custom:test";
    let model_id = "gpt-5.6-luna";
    let mut configuration = PersistedAiConfiguration::default();
    configuration.custom_model_template_decisions.insert(
        PersistedAiConfiguration::provider_model_key(provider_id, model_id),
        crate::CustomModelTemplateDecision::Applied {
            selection: crate::CustomModelTemplateSelection {
                template_key: String::from("openai/gpt-5.6-luna"),
                template_revision: String::from("common-model-capabilities.v1"),
                context_limit: 200_000,
                supports_text_input: true,
                supports_text_output: true,
                input_resources: vec![vulcan_ai_contract::ResourceKind::Image],
                output_resources: Vec::new(),
            },
        },
    );

    let mut catalog =
        manual_custom_catalog_for_provider(&configuration, provider_id, "connection-test", 7)
            .expect("confirmed template should provide a restart-safe manual baseline");
    apply_model_catalog_overlays(&configuration, provider_id, &mut catalog)
        .expect("confirmed template baseline should resolve through normal overlays");

    assert_eq!(catalog.discovery_schema_revision, "manual.models.v1");
    assert_eq!(catalog.generation, 7);
    assert_eq!(catalog.models.len(), 1);
    let model = &catalog.models[0];
    assert_eq!(model.id, model_id);
    assert_eq!(model.display_name, model_id);
    assert_eq!(model.capabilities.context_limit, Some(200_000));
    assert!(
        model
            .capabilities
            .input_resources
            .contains(&vulcan_ai_contract::ResourceKind::Image)
    );
    assert!(
        model
            .capabilities
            .operations
            .contains(&AiOperation::ConversationRespond)
    );
}

/// Verifies rejected templates do not invent a fallback model identity.
/// 验证被拒绝的模板不会虚构回退模型身份。
#[test]
fn rejected_template_does_not_build_manual_catalog_fallback() {
    let provider_id = "custom:test";
    let mut configuration = PersistedAiConfiguration::default();
    configuration.custom_model_template_decisions.insert(
        PersistedAiConfiguration::provider_model_key(provider_id, "model-rejected"),
        crate::CustomModelTemplateDecision::Rejected {
            template_key: String::from("openai/model-rejected"),
            template_revision: String::from("common-model-capabilities.v1"),
        },
    );

    assert!(
        manual_custom_catalog_for_provider(&configuration, provider_id, "connection-test", 1,)
            .is_none()
    );
}

mod model_layers;
mod source_membership;
