//! HTTP and secret-resolution helpers for web-search providers.
//! 网页搜索供应商使用的 HTTP 与密钥解析辅助能力。

use serde_json::Value;

use super::parsing::truncate_error_text;

/// Sends one HTTP request and returns the status code together with the response body text.
/// 发送单个 HTTP 请求，并返回状态码与响应正文文本。
pub(super) async fn send_request(
    builder: reqwest::RequestBuilder,
) -> Result<(reqwest::StatusCode, String), String> {
    let response = builder
        .send()
        .await
        .map_err(|error| format!("HTTP 请求失败：{error}"))?;
    let status = response.status();
    let text = response
        .text()
        .await
        .map_err(|error| format!("读取 HTTP 响应失败：{error}"))?;
    Ok((status, text))
}

/// Sends one HTTP request and parses a successful JSON response.
/// 发送单个 HTTP 请求并解析成功 JSON 响应。
pub(super) async fn fetch_json(builder: reqwest::RequestBuilder) -> Result<Value, String> {
    let (status, text) = send_request(builder).await?;
    if !status.is_success() {
        return Err(format!(
            "HTTP 状态 {status}：{}",
            truncate_error_text(&text, 240)
        ));
    }

    serde_json::from_str(&text).map_err(|error| format!("解析 JSON 响应失败：{error}"))
}

/// Sends one HTTP request and returns the successful response text.
/// 发送单个 HTTP 请求并返回成功响应文本。
pub(super) async fn fetch_text(builder: reqwest::RequestBuilder) -> Result<String, String> {
    let (status, text) = send_request(builder).await?;
    if !status.is_success() {
        return Err(format!(
            "HTTP 状态 {status}：{}",
            truncate_error_text(&text, 240)
        ));
    }
    Ok(text)
}

/// Resolves a secret value that may reference a `{env:NAME}` environment placeholder.
/// 解析可能引用 `{env:NAME}` 环境变量占位符的密钥值。
pub(super) fn resolve_required_secret_value(
    raw_value: &str,
    field_label: &str,
) -> Result<String, String> {
    resolve_optional_secret_value(raw_value, field_label)?
        .ok_or_else(|| format!("{field_label} is empty."))
}

/// Resolves an optional secret value that may reference a `{env:NAME}` environment placeholder.
/// 解析可能引用 `{env:NAME}` 环境变量占位符的可选密钥值。
pub(super) fn resolve_optional_secret_value(
    raw_value: &str,
    field_label: &str,
) -> Result<Option<String>, String> {
    let trimmed = raw_value.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }

    let Some(env_name) = trimmed
        .strip_prefix("{env:")
        .and_then(|value| value.strip_suffix('}'))
    else {
        return Ok(Some(trimmed.to_string()));
    };
    let env_name = env_name.trim();
    if env_name.is_empty() {
        return Err(format!(
            "{field_label} uses an invalid environment-variable placeholder format."
        ));
    }

    std::env::var(env_name)
        .map(|value| value.trim().to_string())
        .map_err(|_| {
            format!(
                "{field_label} references environment variable `{env_name}`, but it is not defined in the current process."
            )
        })
        .and_then(|value| {
            if value.is_empty() {
                Err(format!(
                    "{field_label} references environment variable `{env_name}`, but that variable is empty."
                ))
            } else {
                Ok(Some(value))
            }
        })
}
