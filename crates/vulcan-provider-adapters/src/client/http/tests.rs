//! Regression tests for http.
//! http 的回归测试。

use super::*;

/// Verifies decoded response chunks are rejected before crossing their memory boundary.
/// 验证解码响应块会在越过内存边界前被拒绝。
#[test]
fn bounded_model_discovery_chunk_rejects_overflow_before_append() {
    let mut body = vec![b'a'; 4];

    let error = append_bounded_model_discovery_chunk(&mut body, b"b", 4, "model discovery")
        .expect_err("overflowing chunk must fail");

    assert_eq!(body, b"aaaa");
    assert_eq!(error.kind, ModelDiscoveryFailureKind::InvalidResponse);
    assert!(error.message.contains("4-byte catalog boundary"));
}

/// Verifies public provider diagnostics expose only registered fields and redact the exact key.
/// 验证公开供应商诊断只暴露已登记字段并遮盖精确 Key。
#[test]
fn controlled_error_summary_is_structured_and_secret_safe() {
    // secret is deliberately echoed by the fixture to prove exact redaction.
    // secret 会被夹具有意回显，用于证明精确遮盖有效。
    let secret = "sk-private-fixture";
    // summary must retain actionable fields but omit every unregistered payload field.
    // summary 必须保留可操作字段，同时省略每个未登记载荷字段。
    let summary = controlled_error_summary(
        r#"{"error":{"code":"invalid_key","message":"rejected sk-private-fixture"},"debug":{"raw":"private-upstream-body"}}"#,
        secret,
    );

    assert!(summary.contains("error.code=invalid_key"));
    assert!(summary.contains("error.message=rejected [REDACTED]"));
    assert!(!summary.contains(secret));
    assert!(!summary.contains("private-upstream-body"));
}

/// Verifies non-JSON provider bodies never enter the public diagnostic channel.
/// 验证非 JSON 供应商正文绝不会进入公开诊断通道。
#[test]
fn controlled_error_summary_omits_unstructured_body() {
    // summary is fixed text and must not reproduce the raw upstream HTML.
    // summary 是固定文本，不能复现原始上游 HTML。
    let summary = controlled_error_summary("<html>private diagnostic</html>", "secret");

    assert_eq!(
        summary,
        "Provider returned an unstructured error response; body omitted."
    );
    assert!(!summary.contains("private diagnostic"));
}
