//! Bounded response metadata and exact protocol/Retry-After interpretation.
//! 有界响应元数据与精确的协议及 Retry-After 解析。

use super::failure::{ProviderExecutionFailure, response_headers_protocol_failure};
use reqwest::header::HeaderMap;
use std::collections::BTreeMap;
use std::time::SystemTime;
use vulcan_ai_contract::{RetryAfterHint, RetryAfterSource};
use vulcan_llm_core::ProtocolTransportProfile;

/// Maximum bytes retained from one provider-controlled diagnostic response-header value.
/// 单个供应商控制的诊断响应头值允许保留的最大字节数。
const PROVIDER_DIAGNOSTIC_HEADER_VALUE_MAX_BYTES: usize = 1_024;

/// Explicit suffix appended when a provider diagnostic header value is truncated.
/// 供应商诊断响应头值被截断时附加的显式后缀。
const PROVIDER_DIAGNOSTIC_HEADER_TRUNCATION_SUFFIX: &str = "...[truncated]";

/// Copies only the fixed non-secret rate-limit response-header allowlist with bounded values.
/// 仅复制固定且不含秘密、值有界的限流响应头允许列表。
pub(super) fn allowlisted_rate_limit_headers(headers: &HeaderMap) -> BTreeMap<String, String> {
    const NAMES: &[&str] = &[
        "retry-after",
        "ratelimit-limit",
        "ratelimit-policy",
        "ratelimit-remaining",
        "ratelimit-reset",
        "x-ratelimit-limit-requests",
        "x-ratelimit-limit-tokens",
        "x-ratelimit-remaining-requests",
        "x-ratelimit-remaining-tokens",
        "x-ratelimit-reset-requests",
        "x-ratelimit-reset-tokens",
    ];
    NAMES
        .iter()
        .filter_map(|name| {
            headers
                .get(*name)
                .and_then(|value| value.to_str().ok())
                .map(|value| {
                    (
                        (*name).to_string(),
                        bounded_provider_diagnostic_header_value(value),
                    )
                })
        })
        .collect()
}

/// Copies one provider-controlled diagnostic header value into a fixed byte budget.
/// 将单个供应商控制的诊断响应头值复制到固定字节预算内。
///
/// # Parameters
/// 参数
/// `value` is already decoded response-header text and may still be adversarially large.
/// `value` 是已解码的响应头文本，但仍可能被对端恶意放大。
///
/// # Returns
/// 返回值
/// The original value when bounded, otherwise a UTF-8-safe prefix plus an explicit marker whose
/// total length never exceeds `PROVIDER_DIAGNOSTIC_HEADER_VALUE_MAX_BYTES`.
/// 值已受限时原样返回，否则返回 UTF-8 安全前缀与显式标记，且总长度绝不超过
/// `PROVIDER_DIAGNOSTIC_HEADER_VALUE_MAX_BYTES`。
fn bounded_provider_diagnostic_header_value(value: &str) -> String {
    if value.len() <= PROVIDER_DIAGNOSTIC_HEADER_VALUE_MAX_BYTES {
        return value.to_string();
    }
    // prefix_budget reserves the marker inside the same hard byte ceiling.
    // prefix_budget 在相同硬字节上限内为标记预留空间。
    let prefix_budget = PROVIDER_DIAGNOSTIC_HEADER_VALUE_MAX_BYTES
        .saturating_sub(PROVIDER_DIAGNOSTIC_HEADER_TRUNCATION_SUFFIX.len());
    // boundary walks backward only when the byte budget splits one UTF-8 scalar.
    // boundary 仅在字节预算切开 UTF-8 标量时向前回退。
    let mut boundary = prefix_budget.min(value.len());
    while !value.is_char_boundary(boundary) {
        boundary = boundary.saturating_sub(1);
    }
    // bounded owns the exact retained prefix and explicit truncation evidence.
    // bounded 持有精确保留前缀与显式截断证据。
    let mut bounded = String::with_capacity(PROVIDER_DIAGNOSTIC_HEADER_VALUE_MAX_BYTES);
    bounded.push_str(&value[..boundary]);
    bounded.push_str(PROVIDER_DIAGNOSTIC_HEADER_TRUNCATION_SUFFIX);
    bounded
}

/// Extracts the upstream request identifier using only the route-declared ordered headers.
/// 仅使用路由声明的有序响应头提取上游请求标识。
///
/// # Parameters
/// 参数
/// `headers` are the exact upstream response headers and `declared_names` is the route-owned order.
/// `headers` 是精确上游响应头，`declared_names` 是路由自有的查找顺序。
///
/// # Returns
/// 返回值
/// The first declared non-empty bounded identifier, absence when none is present, or a protocol failure for invalid text.
/// 返回首个已声明、非空且有界的标识；均不存在时返回空；文本无效时返回协议失败。
pub(super) fn provider_request_id_from_headers(
    headers: &HeaderMap,
    declared_names: &[String],
) -> Result<Option<String>, ProviderExecutionFailure> {
    for name in declared_names {
        let Some(value) = headers.get(name) else {
            continue;
        };
        let value = value.to_str().map_err(|error| {
            response_headers_protocol_failure(format!(
                "Provider request-id header `{name}` is not valid text: {error}"
            ))
        })?;
        if !value.is_empty() {
            return Ok(Some(bounded_provider_diagnostic_header_value(value)));
        }
    }
    Ok(None)
}

/// Validates a successful response content type against the immutable protocol profile.
/// 根据不可变协议 Profile 校验成功响应的内容类型。
///
/// # Parameters
/// 参数
/// `headers` are the exact upstream headers, `profile` is the registered transport declaration,
/// and `url` is used only in the controlled diagnostic.
/// `headers` 是精确上游响应头，`profile` 是已注册传输声明，`url` 仅用于受控诊断。
///
/// # Returns
/// 返回值
/// Success for an exact accepted media type, otherwise a dispatched protocol failure.
/// 精确匹配允许媒体类型时返回成功，否则返回已分派协议失败。
pub(super) fn validate_response_content_type(
    headers: &HeaderMap,
    profile: &ProtocolTransportProfile,
    url: &str,
) -> Result<(), ProviderExecutionFailure> {
    let value = headers
        .get(reqwest::header::CONTENT_TYPE)
        .ok_or_else(|| {
            response_headers_protocol_failure(format!(
                "Provider response from `{url}` omitted the required Content-Type header."
            ))
        })?
        .to_str()
        .map_err(|_| {
            response_headers_protocol_failure(format!(
                "Provider response from `{url}` contained a non-ASCII Content-Type header."
            ))
        })?;
    let media_type = value.split(';').next().map(str::trim).unwrap_or_default();
    if profile
        .accepted_response_content_types
        .iter()
        .any(|accepted| media_type.eq_ignore_ascii_case(accepted))
    {
        return Ok(());
    }
    Err(response_headers_protocol_failure(format!(
        "Provider response from `{url}` used unregistered Content-Type `{media_type}`; expected one of [{}].",
        profile.accepted_response_content_types.join(", ")
    )))
}

/// Parses the standard Retry-After header without discarding fractional precision or provenance.
/// 解析标准 Retry-After 响应头，且不丢弃小数精度或来源。
///
/// # Parameters
/// 参数
/// `headers` is the exact upstream response header map and `now` is the comparison instant for
/// HTTP-date values.
/// `headers` 是精确上游响应头集合，`now` 是 HTTP 日期值使用的比较时刻。
///
/// # Returns
/// 返回值
/// A normalized retry hint for valid seconds, fractional seconds, or HTTP dates.
/// 对合法秒数、小数秒或 HTTP 日期返回归一化重试提示。
pub(crate) fn parse_retry_after_headers(
    headers: &HeaderMap,
    now: SystemTime,
) -> Option<RetryAfterHint> {
    let value = headers
        .get(reqwest::header::RETRY_AFTER)?
        .to_str()
        .ok()?
        .trim();
    if let Some(delay_milliseconds) = parse_decimal_seconds_milliseconds(value) {
        return Some(RetryAfterHint {
            delay_milliseconds,
            source: RetryAfterSource::HeaderSeconds,
        });
    }
    let retry_at = httpdate::parse_http_date(value).ok()?;
    let delay_milliseconds = retry_at
        .duration_since(now)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX);
    Some(RetryAfterHint {
        delay_milliseconds,
        source: RetryAfterSource::HeaderHttpDate,
    })
}

/// Converts a non-negative decimal seconds string into milliseconds without floating-point rounding.
/// 将非负十进制秒数字符串转换为毫秒，且不进行浮点舍入。
///
/// # Parameters
/// 参数
/// `value` is the trimmed Retry-After header value.
/// `value` 是去除首尾空白后的 Retry-After 响应头值。
///
/// # Returns
/// 返回值
/// Exact milliseconds rounded up to the next millisecond for sub-millisecond fractions.
/// 对亚毫秒小数向上取整后的精确毫秒数。
fn parse_decimal_seconds_milliseconds(value: &str) -> Option<u64> {
    let (whole, fraction) = value.split_once('.').unwrap_or((value, ""));
    if whole.is_empty()
        || !whole.bytes().all(|byte| byte.is_ascii_digit())
        || !fraction.bytes().all(|byte| byte.is_ascii_digit())
    {
        return None;
    }
    let whole_milliseconds = whole.parse::<u64>().ok()?.checked_mul(1_000)?;
    if fraction.is_empty() {
        return Some(whole_milliseconds);
    }
    let mut digits = fraction.bytes();
    let hundreds = u64::from(digits.next().unwrap_or(b'0') - b'0');
    let tens = u64::from(digits.next().unwrap_or(b'0') - b'0');
    let ones = u64::from(digits.next().unwrap_or(b'0') - b'0');
    let rounded_fraction =
        hundreds * 100 + tens * 10 + ones + u64::from(digits.any(|digit| digit != b'0'));
    whole_milliseconds.checked_add(rounded_fraction)
}

#[cfg(test)]
mod tests;
