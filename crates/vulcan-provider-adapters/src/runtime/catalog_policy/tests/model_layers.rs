//! Provider, model, and discovery capability-layer precedence.
//! 供应商、模型与发现能力层优先级。

use crate::PersistedAiConfiguration;
use crate::runtime::catalog_policy::apply_model_catalog_overlays;
use std::collections::BTreeMap;
use vulcan_ai_contract::{AiOperation, CapabilitySupport, ModelCapabilities, ReasoningControlMode};

/// Verifies an ID-only custom catalog receives the documented provider defaults.
/// 验证仅含 ID 的自定义目录会获得文档规定的供应商默认值。
#[test]
fn custom_catalog_uses_two_hundred_thousand_and_thirty_two_thousand_defaults() {
    let configuration = PersistedAiConfiguration::default();
    let mut catalog = crate::parse_openai_model_catalog(
        "custom-account",
        1,
        r#"{"data":[{"id":"private/chat"}]}"#,
    )
    .expect("ID-only compatible catalog should parse");

    apply_model_catalog_overlays(&configuration, "custom:test", &mut catalog)
        .expect("custom defaults should resolve");

    let model = catalog.models.first().expect("model should remain visible");
    assert_eq!(model.capabilities.context_limit, Some(200_000));
    assert_eq!(model.capabilities.output_limit, Some(32_768));
    assert_eq!(
        model.capabilities.reasoning,
        vulcan_ai_contract::CapabilitySupport::Native
    );
    assert_eq!(
        model.capabilities.reasoning_control,
        vulcan_ai_contract::ReasoningControlMode::EffortSelection
    );
    assert_eq!(
        model.capabilities.reasoning_efforts,
        vec![
            vulcan_ai_contract::ReasoningEffort::None,
            vulcan_ai_contract::ReasoningEffort::Minimal,
            vulcan_ai_contract::ReasoningEffort::Low,
            vulcan_ai_contract::ReasoningEffort::Medium,
            vulcan_ai_contract::ReasoningEffort::High,
            vulcan_ai_contract::ReasoningEffort::XHigh,
            vulcan_ai_contract::ReasoningEffort::Max,
        ]
    );
    assert!(
        model
            .capabilities
            .operations
            .contains(&AiOperation::ConversationRespond)
    );
}

/// Verifies a legacy Native-only declaration receives the same host default levels.
/// 验证只有 Native 支持标记的旧声明也会获得相同的宿主默认档位。
#[test]
fn native_only_conversation_declaration_defaults_to_enabled_reasoning() {
    let configuration = PersistedAiConfiguration::default();
    let mut catalog = crate::parse_openai_model_catalog(
        "apikeyfun-account",
        1,
        r#"{"data":[{"id":"gpt-5.6-luna"}]}"#,
    )
    .expect("compatible source catalog should parse");
    let model = catalog
        .models
        .first_mut()
        .expect("compatible model should exist");
    model.capabilities.reasoning = CapabilitySupport::Native;

    apply_model_catalog_overlays(&configuration, "apikeyfun", &mut catalog)
        .expect("partial reasoning declaration should resolve");

    let model = catalog.models.first().expect("model should remain visible");
    assert_eq!(model.capabilities.reasoning, CapabilitySupport::Native);
    assert_eq!(
        model.capabilities.reasoning_control,
        ReasoningControlMode::EffortSelection
    );
    assert_eq!(
        model.capabilities.reasoning_efforts,
        crate::CUSTOM_PROVIDER_OPENAI_REASONING_EFFORTS
    );
}

/// Verifies a system model cannot replace the catalog reasoning contract with a disabled override.
/// 验证系统模型不能用关闭覆盖替换目录中的思考契约。
#[test]
fn system_model_reasoning_disablement_override_is_ignored() {
    let mut configuration = PersistedAiConfiguration::default();
    configuration.model_capability_overrides.insert(
        PersistedAiConfiguration::provider_model_key("apikeyfun", "gpt-5.6-luna"),
        crate::ModelCapabilityOverrides {
            reasoning: Some(CapabilitySupport::Unsupported),
            reasoning_control: Some(ReasoningControlMode::FixedOff),
            reasoning_efforts: Some(vec![vulcan_ai_contract::ReasoningEffort::None]),
            ..crate::ModelCapabilityOverrides::default()
        },
    );
    let mut catalog = crate::parse_openai_model_catalog(
        "apikeyfun-account",
        1,
        r#"{"data":[{"id":"gpt-5.6-luna"}]}"#,
    )
    .expect("compatible source catalog should parse");

    apply_model_catalog_overlays(&configuration, "apikeyfun", &mut catalog)
        .expect("system capability override should be ignored");

    let model = catalog.models.first().expect("model should remain visible");
    assert_eq!(model.capabilities.reasoning, CapabilitySupport::Native);
    assert_eq!(
        model.capabilities.reasoning_control,
        ReasoningControlMode::EffortSelection
    );
    assert_eq!(
        model.capabilities.reasoning_efforts,
        crate::CUSTOM_PROVIDER_OPENAI_REASONING_EFFORTS
    );
}

/// Verifies a system model cannot replace a disabled catalog contract with an enabled override.
/// 验证系统模型不能用开启覆盖替换目录中的关闭契约。
#[test]
fn system_model_reasoning_enablement_override_is_ignored() {
    let model_id = "gpt-5.6-luna";
    let mut configuration = PersistedAiConfiguration::default();
    configuration.model_capability_overrides.insert(
        PersistedAiConfiguration::provider_model_key("apikeyfun", model_id),
        crate::ModelCapabilityOverrides {
            reasoning: Some(CapabilitySupport::Native),
            reasoning_control: Some(ReasoningControlMode::EffortSelection),
            reasoning_efforts: Some(crate::CUSTOM_PROVIDER_OPENAI_REASONING_EFFORTS.to_vec()),
            ..crate::ModelCapabilityOverrides::default()
        },
    );
    let mut catalog = crate::parse_openai_model_catalog(
        "apikeyfun-account",
        1,
        r#"{"data":[{"id":"gpt-5.6-luna"}]}"#,
    )
    .expect("compatible source catalog should parse");
    let source_capabilities = &mut catalog
        .models
        .first_mut()
        .expect("source model should exist")
        .capabilities;
    source_capabilities.reasoning = CapabilitySupport::Unsupported;
    source_capabilities.reasoning_control = ReasoningControlMode::FixedOff;
    source_capabilities.reasoning_efforts = vec![vulcan_ai_contract::ReasoningEffort::None];

    apply_model_catalog_overlays(&configuration, "apikeyfun", &mut catalog)
        .expect("system capability override should be ignored");

    let model = catalog.models.first().expect("model should remain visible");
    assert_eq!(model.capabilities.reasoning, CapabilitySupport::Unsupported);
    assert_eq!(
        model.capabilities.reasoning_control,
        ReasoningControlMode::FixedOff
    );
    assert_eq!(
        model.capabilities.reasoning_efforts,
        vec![vulcan_ai_contract::ReasoningEffort::None]
    );
}

/// Verifies direct provider settings outrank catalog values inside the text-conversation contract.
/// 验证直接供应商设置会在文本会话契约内覆盖目录值。
#[test]
fn provider_settings_outrank_catalog_values_within_text_contract() {
    let mut configuration = PersistedAiConfiguration::default();
    configuration.custom_provider_model_defaults.insert(
        String::from("custom:test"),
        crate::CustomProviderModelDefaults {
            context_limit: 200_000,
            output_limit: 32_768,
            input_resources: vec![vulcan_ai_contract::ResourceKind::Image],
            ..crate::CustomProviderModelDefaults::default()
        },
    );
    configuration.custom_model_template_decisions.insert(
        PersistedAiConfiguration::provider_model_key("custom:test", "private/chat"),
        crate::CustomModelTemplateDecision::Applied {
            selection: crate::CustomModelTemplateSelection {
                template_key: String::from("openai/gpt-5.2"),
                template_revision: String::from(crate::CUSTOM_MODEL_TEMPLATE_REVISION),
                context_limit: 100_000,
                supports_text_input: true,
                supports_text_output: true,
                input_resources: vec![vulcan_ai_contract::ResourceKind::File],
                output_resources: Vec::new(),
            },
        },
    );
    let mut catalog = crate::parse_openai_model_catalog(
        "custom-account",
        1,
        r#"{"data":[{"id":"private/chat","context_window":64000,"max_output_tokens":4096,"input_modalities":["text"],"output_modalities":["text"]}]}"#,
    )
    .expect("explicit compatible catalog should parse");

    apply_model_catalog_overlays(&configuration, "custom:test", &mut catalog)
        .expect("custom evidence should resolve");

    let model = catalog.models.first().expect("model should remain visible");
    assert_eq!(model.capabilities.context_limit, Some(200_000));
    assert_eq!(model.capabilities.output_limit, Some(32_768));
    assert_eq!(
        model.capabilities.input_resources,
        vec![vulcan_ai_contract::ResourceKind::Image]
    );
    assert!(model.capabilities.output_resources.is_empty());
    assert!(
        model
            .capabilities
            .operations
            .contains(&AiOperation::ConversationRespond)
    );
}

/// Verifies custom providers do not publish resource-producing models as generic conversation choices.
/// 验证自定义供应商不会把资源生成模型发布为通用会话选项。
#[test]
fn custom_provider_filters_explicit_non_text_output_models() {
    let configuration = PersistedAiConfiguration::default();
    let mut catalog = crate::parse_openai_model_catalog(
        "custom-account",
        1,
        r#"{"data":[{"id":"private/image","input_modalities":["text"],"output_modalities":["image"]},{"id":"private/mixed","input_modalities":["text"],"output_modalities":["text","image"]}]}"#,
    )
    .expect("explicit non-text model should parse");

    apply_model_catalog_overlays(&configuration, "custom:test", &mut catalog)
        .expect("custom conversation contract should resolve");

    assert_eq!(catalog.models.len(), 1);
    let model = &catalog.models[0];
    assert_eq!(model.id, "private/mixed");
    assert_eq!(
        model.capabilities.operations,
        vec![AiOperation::ConversationRespond]
    );
    assert!(model.capabilities.output_resources.is_empty());
}

/// Verifies system reasoning defaults outrank conservative source flags.
/// 验证系统思考默认值会覆盖保守来源标记。
#[test]
fn custom_provider_system_baseline_enables_reasoning() {
    let configuration = PersistedAiConfiguration::default();
    let mut catalog = crate::parse_openai_model_catalog(
        "custom-account",
        1,
        r#"{"data":[{"id":"private/chat"}]}"#,
    )
    .expect("compatible catalog should parse");
    let source_capabilities = &mut catalog
        .models
        .first_mut()
        .expect("source model should exist")
        .capabilities;
    source_capabilities.reasoning = vulcan_ai_contract::CapabilitySupport::Unsupported;
    source_capabilities.reasoning_control = vulcan_ai_contract::ReasoningControlMode::FixedOff;
    source_capabilities.reasoning_efforts = vec![vulcan_ai_contract::ReasoningEffort::None];
    apply_model_catalog_overlays(&configuration, "custom:test", &mut catalog)
        .expect("system baseline should resolve");

    let capabilities = &catalog
        .models
        .first()
        .expect("model should remain visible")
        .capabilities;
    assert_eq!(
        capabilities.reasoning,
        vulcan_ai_contract::CapabilitySupport::Native
    );
    assert_eq!(
        capabilities.reasoning_control,
        vulcan_ai_contract::ReasoningControlMode::EffortSelection
    );
    assert_eq!(
        capabilities.reasoning_efforts,
        vec![
            vulcan_ai_contract::ReasoningEffort::None,
            vulcan_ai_contract::ReasoningEffort::Minimal,
            vulcan_ai_contract::ReasoningEffort::Low,
            vulcan_ai_contract::ReasoningEffort::Medium,
            vulcan_ai_contract::ReasoningEffort::High,
            vulcan_ai_contract::ReasoningEffort::XHigh,
            vulcan_ai_contract::ReasoningEffort::Max,
        ]
    );
}

/// Verifies an explicit provider setting can disable the system reasoning defaults.
/// 验证显式供应商设置可以关闭系统默认的思考能力。
#[test]
fn explicit_provider_settings_can_disable_system_reasoning() {
    let mut configuration = PersistedAiConfiguration::default();
    configuration.custom_provider_model_defaults.insert(
        String::from("custom:test"),
        crate::CustomProviderModelDefaults {
            reasoning: vulcan_ai_contract::CapabilitySupport::Unsupported,
            reasoning_control: vulcan_ai_contract::ReasoningControlMode::FixedOff,
            reasoning_efforts: vec![vulcan_ai_contract::ReasoningEffort::None],
            ..crate::CustomProviderModelDefaults::default()
        },
    );
    let mut catalog = crate::parse_openai_model_catalog(
        "custom-account",
        1,
        r#"{"data":[{"id":"private/chat"}]}"#,
    )
    .expect("compatible catalog should parse");

    apply_model_catalog_overlays(&configuration, "custom:test", &mut catalog)
        .expect("explicit provider settings should resolve");

    let capabilities = &catalog
        .models
        .first()
        .expect("model should remain visible")
        .capabilities;
    assert_eq!(
        capabilities.reasoning,
        vulcan_ai_contract::CapabilitySupport::Unsupported
    );
    assert_eq!(
        capabilities.reasoning_control,
        vulcan_ai_contract::ReasoningControlMode::FixedOff
    );
    assert_eq!(
        capabilities.reasoning_efforts,
        vec![vulcan_ai_contract::ReasoningEffort::None]
    );
}

/// Verifies one direct model layer outranks disabled provider reasoning settings.
/// 验证一层直接模型设置会覆盖供应商层已关闭的思考设置。
#[test]
fn direct_model_settings_outrank_provider_reasoning_settings() {
    let mut configuration = PersistedAiConfiguration::default();
    configuration.custom_provider_model_defaults.insert(
        String::from("custom:test"),
        crate::CustomProviderModelDefaults {
            reasoning: vulcan_ai_contract::CapabilitySupport::Unsupported,
            reasoning_control: vulcan_ai_contract::ReasoningControlMode::FixedOff,
            reasoning_efforts: vec![vulcan_ai_contract::ReasoningEffort::None],
            ..crate::CustomProviderModelDefaults::default()
        },
    );
    configuration.model_capability_overrides.insert(
        PersistedAiConfiguration::provider_model_key("custom:test", "private/chat"),
        crate::ModelCapabilityOverrides {
            reasoning: Some(vulcan_ai_contract::CapabilitySupport::Native),
            reasoning_control: Some(vulcan_ai_contract::ReasoningControlMode::EffortSelection),
            reasoning_efforts: Some(vec![
                vulcan_ai_contract::ReasoningEffort::None,
                vulcan_ai_contract::ReasoningEffort::Medium,
                vulcan_ai_contract::ReasoningEffort::High,
            ]),
            ..crate::ModelCapabilityOverrides::default()
        },
    );
    configuration.custom_models.insert(
        String::from("custom:test"),
        BTreeMap::from([(
            String::from("private/chat"),
            crate::CustomModelDefinition {
                id: String::from("private/chat"),
                display_name: String::from("Advanced Private Chat"),
                enabled: true,
                capabilities: ModelCapabilities {
                    reasoning: vulcan_ai_contract::CapabilitySupport::Unsupported,
                    reasoning_control: vulcan_ai_contract::ReasoningControlMode::FixedOff,
                    reasoning_efforts: vec![vulcan_ai_contract::ReasoningEffort::None],
                    evidence_revision: String::from("manual-advanced.v1"),
                    ..ModelCapabilities::default()
                },
                request_projection: crate::CustomModelRequestProjection::default(),
                tool_schema_projection: crate::ToolSchemaProjectionMode::Lightweight,
            },
        )]),
    );
    let mut catalog = crate::parse_openai_model_catalog(
        "custom-account",
        1,
        r#"{"data":[{"id":"private/chat"}]}"#,
    )
    .expect("compatible catalog should parse");

    apply_model_catalog_overlays(&configuration, "custom:test", &mut catalog)
        .expect("model settings should resolve");

    let capabilities = &catalog
        .models
        .first()
        .expect("model should remain visible")
        .capabilities;
    assert_eq!(
        capabilities.reasoning,
        vulcan_ai_contract::CapabilitySupport::Native
    );
    assert_eq!(
        capabilities.reasoning_control,
        vulcan_ai_contract::ReasoningControlMode::EffortSelection
    );
    assert_eq!(
        capabilities.reasoning_efforts,
        vec![
            vulcan_ai_contract::ReasoningEffort::None,
            vulcan_ai_contract::ReasoningEffort::Medium,
            vulcan_ai_contract::ReasoningEffort::High,
        ]
    );
}

/// Verifies an explicit user difference outranks credential evidence.
/// 验证显式用户差异会优先于凭据证据。
#[test]
fn custom_model_user_override_outranks_credential_evidence() {
    let mut configuration = PersistedAiConfiguration::default();
    configuration.model_capability_overrides.insert(
        PersistedAiConfiguration::provider_model_key("custom:test", "private/chat"),
        crate::ModelCapabilityOverrides {
            context_limit: Some(32_000),
            output_limit: Some(2_000),
            input_resources: Some(vec![vulcan_ai_contract::ResourceKind::File]),
            output_resources: None,
            ..crate::ModelCapabilityOverrides::default()
        },
    );
    let mut catalog = crate::parse_openai_model_catalog(
        "custom-account",
        1,
        r#"{"data":[{"id":"private/chat","context_window":64000,"max_output_tokens":4096,"input_modalities":["text"],"output_modalities":["text"]}]}"#,
    )
    .expect("explicit compatible catalog should parse");

    apply_model_catalog_overlays(&configuration, "custom:test", &mut catalog)
        .expect("custom override should resolve");

    let model = catalog.models.first().expect("model should remain visible");
    assert_eq!(model.capabilities.context_limit, Some(32_000));
    assert_eq!(model.capabilities.output_limit, Some(2_000));
    assert_eq!(
        model.capabilities.input_resources,
        vec![vulcan_ai_contract::ResourceKind::File]
    );
    assert!(model.capabilities.output_resources.is_empty());
}

/// Verifies discovered basics survive while explicit manual advanced declarations become effective.
/// 验证已发现基础事实会保留，同时显式手工高级声明会真正生效。
#[test]
fn discovered_custom_model_applies_only_manual_advanced_declarations() {
    let mut configuration = PersistedAiConfiguration::default();
    configuration
        .custom_models
        .entry(String::from("custom:test"))
        .or_default()
        .insert(
            String::from("private/chat"),
            crate::CustomModelDefinition {
                id: String::from("private/chat"),
                display_name: String::from("Private Chat Renamed"),
                enabled: true,
                capabilities: ModelCapabilities {
                    operations: vec![AiOperation::ConversationRespond],
                    context_limit: Some(1_024),
                    output_limit: Some(512),
                    input_resources: vec![vulcan_ai_contract::ResourceKind::File],
                    reasoning: vulcan_ai_contract::CapabilitySupport::Native,
                    reasoning_control: vulcan_ai_contract::ReasoningControlMode::EffortSelection,
                    reasoning_efforts: vec![vulcan_ai_contract::ReasoningEffort::High],
                    model_parameters: vec![vulcan_ai_contract::ModelParameterDefinition {
                        id: String::from("vendor.flag"),
                        display_name: String::from("Vendor flag"),
                        description: String::from("Controls one verified vendor flag."),
                        schema: vulcan_ai_contract::ModelParameterSchema::Boolean {},
                        default: vulcan_ai_contract::ModelParameterDefault::ProviderDefault {},
                        protocol_ids: Vec::new(),
                        model_ids: Vec::new(),
                        dependent_required: Vec::new(),
                        conflicts: Vec::new(),
                        evidence_revision: String::from("user.custom-model.v1"),
                    }],
                    evidence_revision: String::from("user.custom-model.v2"),
                    ..ModelCapabilities::default()
                },
                request_projection: crate::CustomModelRequestProjection {
                    model_parameter_paths: BTreeMap::from([(
                        String::from("vendor.flag"),
                        String::from("vendor_flag"),
                    )]),
                    ..crate::CustomModelRequestProjection::default()
                },
                tool_schema_projection: crate::ToolSchemaProjectionMode::Lightweight,
            },
        );
    let mut catalog = crate::parse_openai_model_catalog(
        "custom-account",
        1,
        r#"{"data":[{"id":"private/chat","name":"Provider Chat","context_window":64000,"max_output_tokens":4096,"input_modalities":["text","image"],"output_modalities":["text"]}]}"#,
    )
    .expect("explicit compatible catalog should parse");

    apply_model_catalog_overlays(&configuration, "custom:test", &mut catalog)
        .expect("advanced declaration should merge");

    let model = catalog.models.first().expect("model should remain visible");
    assert_eq!(model.display_name, "Private Chat Renamed");
    assert_eq!(model.capabilities.context_limit, Some(64_000));
    assert_eq!(model.capabilities.output_limit, Some(4_096));
    assert_eq!(
        model.capabilities.input_resources,
        vec![vulcan_ai_contract::ResourceKind::Image]
    );
    assert!(model.capabilities.output_resources.is_empty());
    assert_eq!(
        model.capabilities.reasoning_control,
        vulcan_ai_contract::ReasoningControlMode::EffortSelection
    );
    assert_eq!(
        model.capabilities.reasoning_efforts,
        vec![vulcan_ai_contract::ReasoningEffort::High]
    );
    assert_eq!(model.capabilities.model_parameters.len(), 1);
    assert!(
        model
            .capabilities
            .evidence_revision
            .contains("user.custom-model.v2")
    );
}

/// Verifies system-provider runtime settings change limits without replacing capability facts.
/// 验证系统供应商运行时设置只修改限制，不替换能力事实。
#[test]
fn system_provider_runtime_overrides_only_editable_limits() {
    let provider_id = "system-provider";
    let model_id = "chat-model";
    let mut configuration = PersistedAiConfiguration::default();
    configuration.provider_capability_overrides.insert(
        String::from(provider_id),
        crate::ModelCapabilityOverrides {
            context_limit: Some(32_000),
            output_limit: Some(2_000),
            reasoning: Some(vulcan_ai_contract::CapabilitySupport::Native),
            reasoning_control: Some(vulcan_ai_contract::ReasoningControlMode::EffortSelection),
            reasoning_efforts: Some(vec![vulcan_ai_contract::ReasoningEffort::Low]),
            ..crate::ModelCapabilityOverrides::default()
        },
    );
    configuration.model_capability_overrides.insert(
        PersistedAiConfiguration::provider_model_key(provider_id, model_id),
        crate::ModelCapabilityOverrides {
            context_limit: Some(16_000),
            ..crate::ModelCapabilityOverrides::default()
        },
    );
    let mut catalog = crate::parse_openai_model_catalog(
        "system-account",
        1,
        r#"{"data":[{"id":"chat-model"},{"id":"source-model","context_window":64000,"max_output_tokens":4096}]}"#,
    )
    .expect("system catalog should parse");

    apply_model_catalog_overlays(&configuration, provider_id, &mut catalog)
        .expect("provider and model layers should resolve");

    let model = catalog
        .models
        .iter()
        .find(|model| model.id == model_id)
        .expect("model should remain visible");
    assert_eq!(model.capabilities.context_limit, Some(16_000));
    assert_eq!(model.capabilities.output_limit, Some(2_000));
    assert_eq!(
        model.capabilities.reasoning_efforts,
        crate::CUSTOM_PROVIDER_OPENAI_REASONING_EFFORTS
    );
    let source_model = catalog
        .models
        .iter()
        .find(|model| model.id == "source-model")
        .expect("source model should remain visible");
    assert_eq!(source_model.capabilities.context_limit, Some(64_000));
    assert_eq!(source_model.capabilities.output_limit, Some(4_096));
}
