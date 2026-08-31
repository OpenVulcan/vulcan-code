//! Tests for preserving bounded route evidence without exposing execution credentials.
//! 保留有界路由证据且不暴露执行凭据的测试。

use super::*;
use crate::CredentialSecretEnvelope;

/// Verifies provider diagnostics redact credentials while preserving bounded route identity.
/// 验证供应商诊断会脱敏凭据并保留有界路由身份。
#[test]
fn provider_tool_diagnostics_redact_secret_and_preserve_route_context() {
    let secret = CredentialSecretEnvelope::new_api_key(String::from("secret-provider-key"))
        .expect("test credential should validate")
        .execution_secret()
        .expect("test execution secret should resolve");
    let diagnostic = diagnose_provider_tool_failure_with_secret(
        format!(
            "secret-provider-key {} secret-provider-key",
            "provider diagnostic ".repeat(1_000)
        ),
        "provider=test, target=exact-target",
        &secret,
    );

    assert!(!diagnostic.contains("secret-provider-key"));
    assert!(diagnostic.contains("<redacted>"));
    assert!(diagnostic.contains("provider_tool_route"));
    assert!(diagnostic.contains("target=exact-target"));
    assert!(
        diagnostic.chars().count() <= crate::provider_task::PROVIDER_TASK_DIAGNOSTIC_MAX_CHARACTERS
    );
}
