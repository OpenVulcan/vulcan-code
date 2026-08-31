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
