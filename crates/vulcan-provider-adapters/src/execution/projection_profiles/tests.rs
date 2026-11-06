//! Regression coverage for profile mapping order, pointer failures, and scalar lookup keys.
//! Profile 映射顺序、Pointer 失败与标量查找键的回归覆盖。

use super::*;
use serde_json::json;

/// Verifies a response profile rewrites vendor JSON before protocol decoding.
/// 验证响应 Profile 会在协议解码前改写供应商 JSON。
#[test]
fn response_profile_projects_stream_record_before_decode() {
    let profile = crate::provider_manifest::ResponseProjectionProfile {
        profile_id: String::from("vendor-response.v1"),
        revision: 1,
        steps: vec![crate::provider_manifest::MappingStep::RenameJsonPointer {
            from: String::from("/vendor_usage"),
            to: String::from("/usage"),
        }],
        transient_reasoning_status: None,
    };

    let projected = project_response_record_text(
        &[profile],
        r#"{"type":"response.completed","vendor_usage":{"input_tokens":7}}"#,
    )
    .expect("response profile should project");
    let value: Value = serde_json::from_str(&projected).expect("projected JSON should parse");
    assert_eq!(value["usage"]["input_tokens"], 7);
    assert!(value.get("vendor_usage").is_none());
}

/// Verifies records without profiles are borrowed untouched, while projected records carry the
/// decoded value that matches their serialized text so usage extraction need not reparse it.
/// 验证无 Profile 的记录原样借用，而投影后的记录携带与其序列化文本一致的解码值，
/// 使用量提取无需再次解析。
#[test]
fn projected_record_is_borrowed_without_profiles_and_shares_decoded_value() {
    let raw = r#"{"vendor_usage":{"input_tokens":3}}"#;
    let untouched = project_response_record(&[], raw).expect("empty profiles should pass through");
    assert!(matches!(untouched.text, Cow::Borrowed(text) if text == raw));
    assert!(untouched.value.is_none());

    let profile = crate::provider_manifest::ResponseProjectionProfile {
        profile_id: String::from("vendor-response.v1"),
        revision: 1,
        steps: vec![crate::provider_manifest::MappingStep::RenameJsonPointer {
            from: String::from("/vendor_usage"),
            to: String::from("/usage"),
        }],
        transient_reasoning_status: None,
    };
    let projected = project_response_record(&[profile], raw).expect("profile should project");
    let reparsed: Value =
        serde_json::from_str(&projected.text).expect("projected JSON should parse");
    assert_eq!(projected.value.as_ref(), Some(&reparsed));
    assert_eq!(reparsed["usage"]["input_tokens"], 3);
}

/// Verifies an error profile normalizes vendor error JSON before protocol decoding.
/// 验证错误 Profile 会在协议解码前规范化供应商错误 JSON。
/// Verifies vendor error envelopes are normalized before protocol decoding.
/// 验证供应商错误信封会在协议解码前完成规范化。
#[test]
fn error_profile_projects_vendor_error_before_decode() {
    let profile = crate::provider_manifest::ResponseProjectionProfile {
        profile_id: String::from("vendor-error.v1"),
        revision: 1,
        steps: vec![crate::provider_manifest::MappingStep::RenameJsonPointer {
            from: String::from("/vendor_error"),
            to: String::from("/error"),
        }],
        transient_reasoning_status: None,
    };

    let projected = project_response_record_text(
        &[profile],
        r#"{"vendor_error":{"code":"quota_exceeded","message":"limit"}}"#,
    )
    .expect("error profile should project");
    let value: Value = serde_json::from_str(&projected).expect("projected JSON should parse");
    assert_eq!(value["error"]["code"], "quota_exceeded");
    assert!(value.get("vendor_error").is_none());
}

/// Verifies remote scalar ancestors produce a controlled response-projection failure.
/// 验证远端标量祖先会产生受控响应投影失败。
#[test]
fn response_profile_rejects_non_object_target_ancestor_without_panic() {
    let profile = crate::provider_manifest::ResponseProjectionProfile {
        profile_id: String::from("vendor-response.v1"),
        revision: 1,
        steps: vec![crate::provider_manifest::MappingStep::SetLiteral {
            path: String::from("/error/code"),
            value: Value::String(String::from("mapped")),
        }],
        transient_reasoning_status: None,
    };

    let error = project_response_record_text(&[profile], r#"{"error":"plain"}"#)
        .expect_err("scalar ancestor must fail closed");
    assert!(error.message.contains("non-object"), "{error:?}");
}

/// Verifies runtime request scalars cannot turn a valid profile into a process panic.
/// 验证运行时请求标量不会把有效 Profile 变成进程 panic。
#[test]
fn request_profile_rejects_non_object_target_ancestor_without_panic() {
    // profile expects the protocol body to expose an object-valued reasoning envelope.
    // profile 期望协议正文提供对象类型的 reasoning 信封。
    let profile = crate::provider_manifest::RequestProjectionProfile {
        profile_id: String::from("vendor-request.v1"),
        revision: 1,
        steps: vec![crate::provider_manifest::MappingStep::SetLiteral {
            path: String::from("/reasoning/budget_tokens"),
            value: json!(1024),
        }],
    };

    // error proves a runtime scalar is rejected at the pre-dispatch boundary without panic.
    // error 证明运行时标量会在分发前边界被拒绝且不会触发 panic。
    let error = project_json_request_profiles(
        &[profile],
        "vendor-model",
        json!({"reasoning": "provider-default"}),
    )
    .expect_err("scalar ancestor must fail closed");
    assert!(error.contains("non-object"), "{error}");
}

/// Verifies thinking budgets reserve answer space and reject malformed or impossible integer bounds.
/// 验证思考预算预留答案空间，并拒绝格式错误或无法满足的整数边界。
/// No parameters; checks fitting, overflow extremes, optional values, and pre-dispatch failures.
/// 无参数；检查预算调整、溢出极值、可选值与分派前失败。
#[test]
fn thinking_budget_projection_respects_output_limits_and_integer_bounds() {
    // profile uses the same declarative primitive as the provider's model-specific budget variant.
    // profile 使用与供应商模型预算变体相同的声明式原语。
    let profile = crate::provider_manifest::RequestProjectionProfile {
        profile_id: String::from("thinking-budget.v1"),
        revision: 1,
        steps: vec![crate::provider_manifest::MappingStep::FitThinkingBudget {
            path: String::from("/thinking/budget_tokens"),
            limit_path: String::from("/max_tokens"),
            minimum: 1024,
        }],
    };
    for (budget, limit, expected) in [
        (16000, 64000, 16000),
        (31999, 32768, 16384),
        (16000, 2049, 1024),
        (u64::MAX, u64::MAX, u64::MAX / 2),
    ] {
        // projected comes from the production profile interpreter with no transport involved.
        // projected 来自生产投影解释器，不涉及传输。
        let projected = project_json_request_profiles(
            std::slice::from_ref(&profile),
            "budget-model",
            json!({"max_tokens":limit,"thinking":{"type":"enabled","budget_tokens":budget}}),
        )
        .unwrap();
        assert_eq!(projected["thinking"]["budget_tokens"], expected);
        assert_eq!(projected["max_tokens"], limit);
    }
    for body in [
        json!({"max_tokens":32768,"thinking":{"type":"disabled"}}),
        json!({"thinking":{"type":"enabled","budget_tokens":16000}}),
    ] {
        assert_eq!(
            project_json_request_profiles(
                std::slice::from_ref(&profile),
                "budget-model",
                body.clone()
            )
            .unwrap(),
            body,
        );
    }
    for invalid in [json!(-1), json!(1.5), json!("16000"), Value::Null, json!(0)] {
        assert!(
            project_json_request_profiles(
                std::slice::from_ref(&profile),
                "budget-model",
                json!({"max_tokens":32768,"thinking":{"budget_tokens":invalid}}),
            )
            .is_err()
        );
        assert!(
            project_json_request_profiles(
                std::slice::from_ref(&profile),
                "budget-model",
                json!({"max_tokens":invalid,"thinking":{"budget_tokens":16000}}),
            )
            .is_err()
        );
    }
    assert!(
        project_json_request_profiles(
            &[profile],
            "budget-model",
            json!({"max_tokens":1024,"thinking":{"budget_tokens":16000}}),
        )
        .is_err()
    );
}

/// Verifies enum maps use one deterministic key representation for every supported scalar.
/// 验证枚举映射为每一种受支持标量使用统一且确定的键表示。
#[test]
fn projection_scalar_keys_cover_string_number_and_boolean_values() {
    assert_eq!(
        projection_scalar_key(&json!("32000")),
        Some(String::from("32000"))
    );
    assert_eq!(
        projection_scalar_key(&json!(32000)),
        Some(String::from("32000"))
    );
    assert_eq!(
        projection_scalar_key(&json!(true)),
        Some(String::from("true"))
    );
    assert_eq!(projection_scalar_key(&Value::Null), None);
    assert_eq!(projection_scalar_key(&json!([32000])), None);
}

/// Verifies copied decimal parameters reach the wire as JSON numbers for every accepted spelling.
/// 验证复制的十进制参数对每种可接受写法都以 JSON 数字发送。
#[test]
fn copied_decimal_parameters_are_json_numbers() {
    for (lexeme, expected) in [
        ("0.5", json!(0.5)),
        ("+0.5", json!(0.5)),
        (".5", json!(0.5)),
        ("5.", json!(5)),
        ("-0.25", json!(-0.25)),
        ("007", json!(7)),
    ] {
        assert_eq!(
            model_parameter_to_value(&ModelParameterValue::Decimal(String::from(lexeme))),
            Some(expected),
            "decimal `{lexeme}`"
        );
    }
}
