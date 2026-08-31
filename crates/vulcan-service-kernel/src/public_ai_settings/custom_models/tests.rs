//! Regression coverage for custom-model membership, request defaults, and protocol preference validation.
//! 自定义模型成员关系、请求默认值与协议偏好校验的回归覆盖。

use super::*;
use vulcan_provider_adapters::CustomProviderProtocol;

/// Verifies account membership cannot reopen a model disabled by its provider.
/// 验证账号成员关系不能重新开启已被供应商关闭的模型。
#[test]
fn account_membership_cannot_enable_provider_disabled_custom_model() {
    // configuration stores the exact provider-level disabled decision under test.
    // configuration 保存本测试所需的精确供应商级关闭决定。
    let mut configuration = PersistedAiConfiguration::default();
    configuration.set_model_enabled("custom:test", "manual/chat", false);

    assert!(
        validate_custom_model_connection_availability_choice(
            &configuration,
            "custom:test",
            "manual/chat",
            Some(true),
        )
        .is_err()
    );
    assert!(
        validate_custom_model_connection_availability_choice(
            &configuration,
            "custom:test",
            "manual/chat",
            Some(false),
        )
        .is_ok()
    );
    assert!(
        validate_custom_model_connection_availability_choice(
            &configuration,
            "custom:test",
            "manual/chat",
            None,
        )
        .is_ok()
    );
}

/// Verifies older model-save clients may omit the newly added template decision batch.
/// 验证旧版模型保存客户端可以省略新增的模板决策批次。
#[test]
fn custom_model_save_request_defaults_missing_template_decisions() {
    // request preserves the previous browser payload shape while the service owns the default.
    // request 保留旧版浏览器载荷形态，并由服务端负责提供默认值。
    let request: PublicAiCustomModelsSaveRequest = serde_json::from_value(serde_json::json!({
        "providerId": "custom:test",
        "models": []
    }))
    .expect("the previous custom-model save payload should remain valid");

    assert_eq!(request.provider_id, "custom:test");
    assert!(request.models.is_empty());
    assert!(request.template_decisions.is_empty());
}

/// Verifies explicit preferences must reference an enabled protocol and keys are unique.
/// 验证显式偏好必须引用启用协议，且建键必须唯一。
#[test]
fn explicit_preferences_require_enabled_protocols() {
    let enabled = BTreeSet::from([
        String::from("openai-responses.v1"),
        String::from("anthropic-messages.v1"),
    ]);
    let items = vec![PublicAiCustomModelProtocolPreferenceItem {
        model_id: String::from("model-a"),
        preference: CustomModelProtocolPreference::Explicit {
            protocol: CustomProviderProtocol::AnthropicMessages,
        },
    }];
    let keyed = validate_custom_model_protocol_preferences("custom:test", &enabled, &items)
        .expect("explicit protocol inside the enabled set must save");
    assert_eq!(keyed.len(), 1);

    let items = vec![PublicAiCustomModelProtocolPreferenceItem {
        model_id: String::from("model-b"),
        preference: CustomModelProtocolPreference::Explicit {
            protocol: CustomProviderProtocol::GeminiGenerateContent,
        },
    }];
    let error = validate_custom_model_protocol_preferences("custom:test", &enabled, &items)
        .expect_err("explicit protocol outside the enabled set must be rejected");
    assert!(error.contains("not enabled"), "unexpected error: {error}");

    let items = vec![
        PublicAiCustomModelProtocolPreferenceItem {
            model_id: String::from("model-c"),
            preference: CustomModelProtocolPreference::Auto,
        },
        PublicAiCustomModelProtocolPreferenceItem {
            model_id: String::from("model-c"),
            preference: CustomModelProtocolPreference::Auto,
        },
    ];
    let error = validate_custom_model_protocol_preferences("custom:test", &enabled, &items)
        .expect_err("duplicate preferences must be rejected");
    assert!(error.contains("duplicate"), "unexpected error: {error}");
}
