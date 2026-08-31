//! Credential redaction and bounded provider-tool failure diagnostics.
//! 凭据脱敏与有界供应商工具失败诊断。

use crate::CredentialExecutionSecret;
use crate::provider_task::bound_provider_task_diagnostic;

/// Maximum characters retained from one untrusted provider-tool failure body.
/// 单个不可信供应商工具失败正文允许保留的最大字符数。
const PROVIDER_TOOL_ERROR_DIAGNOSTIC_MAX_CHARACTERS: usize = 6_000;
/// Maximum characters retained from one immutable provider-tool route context.
/// 单个不可变供应商工具路由上下文允许保留的最大字符数。
const PROVIDER_TOOL_ROUTE_DIAGNOSTIC_MAX_CHARACTERS: usize = 2_000;

/// Appends one immutable provider-tool route context without exposing credential material.
/// 为错误追加一份不可变供应商工具路由上下文且不暴露凭据材料。
///
/// # Parameters
/// 参数
/// `error` is a credential-sanitized failure and `context` is the frozen non-secret route identity.
/// `error` 是已经过凭据脱敏的失败，`context` 是冻结的非秘密路由身份。
///
/// # Returns
/// 返回值
/// A bounded diagnostic with bounded failure and route components.
/// 返回失败和路由组成部分均有界的诊断。
pub(super) fn diagnose_provider_tool_failure(error: String, context: &str) -> String {
    let error = truncate_provider_tool_diagnostic_component(
        error.as_str(),
        PROVIDER_TOOL_ERROR_DIAGNOSTIC_MAX_CHARACTERS,
    );
    let context = truncate_provider_tool_diagnostic_component(
        context,
        PROVIDER_TOOL_ROUTE_DIAGNOSTIC_MAX_CHARACTERS,
    );
    bound_provider_task_diagnostic(format!("{error} [provider_tool_route: {context}]").as_str())
}

/// Truncates one provider-tool diagnostic component on a Unicode scalar boundary.
/// 在 Unicode 标量边界截断一个供应商工具诊断组成部分。
///
/// # Parameters
/// 参数
/// `value` is the exact component and `maximum_characters` is its closed public bound.
/// `value` 是精确组成部分，`maximum_characters` 是其封闭公开边界。
///
/// # Returns
/// 返回值
/// The original value within bounds, otherwise a suffix-marked truncated copy.
/// 边界内返回原值，否则返回带截断后缀的副本。
fn truncate_provider_tool_diagnostic_component(value: &str, maximum_characters: usize) -> String {
    if value.chars().count() <= maximum_characters {
        return value.to_string();
    }
    let mut truncated = value
        .chars()
        .take(maximum_characters.saturating_sub(3))
        .collect::<String>();
    truncated.push_str("...");
    truncated
}

/// Redacts the exact execution credential before appending bounded provider-tool route context.
/// 在追加有界供应商工具路由上下文前脱敏精确执行凭据。
///
/// # Parameters
/// 参数
/// `error` is the transport or provider diagnostic, `context` is non-secret route identity, and
/// `secret` is the exact in-memory credential used by the failed physical request.
/// `error` 是传输或供应商诊断，`context` 是非秘密路由身份，`secret` 是失败物理请求使用的
/// 精确内存凭据。
///
/// # Returns
/// 返回值
/// A bounded public diagnostic containing no exact credential value.
/// 返回不含精确凭据值的有界公开诊断。
pub(super) fn diagnose_provider_tool_failure_with_secret(
    error: String,
    context: &str,
    secret: &CredentialExecutionSecret,
) -> String {
    diagnose_provider_tool_failure(redact_provider_tool_secret(error.as_str(), secret), context)
}

/// Removes the exact execution credential and applies the durable diagnostic boundary.
/// 移除精确执行凭据并应用持久诊断边界。
///
/// # Parameters
/// 参数
/// `value` is an untrusted diagnostic and `secret` owns the credential that must not escape.
/// `value` 是不可信诊断，`secret` 持有绝不能外泄的凭据。
///
/// # Returns
/// 返回值
/// A bounded diagnostic with every exact credential occurrence replaced.
/// 返回每个精确凭据出现位置均已替换的有界诊断。
pub(super) fn redact_provider_tool_secret(
    value: &str,
    secret: &CredentialExecutionSecret,
) -> String {
    let sensitive_value = secret.expose_value();
    let redacted = if sensitive_value.is_empty() {
        value.to_string()
    } else {
        value.replace(sensitive_value, "<redacted>")
    };
    bound_provider_task_diagnostic(redacted.as_str())
}

#[cfg(test)]
mod tests;
