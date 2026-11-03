//! Validated provider authentication, application identity, and handshake headers.
//! 已校验的供应商认证、应用身份与握手请求头。

use super::failure::{ProviderExecutionFailure, pre_dispatch_failure};
use super::route::{ProviderExecutionRoute, route_uses_native_codex_responses};
use crate::{
    AuthenticationProjection, CredentialExecutionSecret,
    provider_manifest::{CodexWireDialect, is_host_owned_session_header},
};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use std::collections::BTreeMap;
pub(crate) use vulcan_llm_core::vulcan_code_user_agent;
use vulcan_llm_core::{VULCAN_CODE_CLIENT_NAME, vulcan_code_version};

/// Host-owned marker that selects the Responses Lite wire contract.
/// 由宿主持有、用于选择 Responses Lite 线路契约的标记。
pub(super) const CODEX_RESPONSES_LITE_HEADER_NAME: &str = "x-openai-internal-codex-responses-lite";

/// Builds provider authentication and public application headers.
/// 构建供应商鉴权与公共应用请求头。
///
/// # Parameters
/// 参数
/// `route` identifies the exact provider, `protocol_headers` contains non-secret Profile
/// requirements, `secret` is the selected credential material, and `session_id` is the current
/// public session identifier when one is available.
/// `route` 标识精确供应商，`protocol_headers` 包含非秘密 Profile 要求，
/// `secret` 是选中的凭据材料，`session_id` 是当前可用的公共会话标识。
///
/// # Returns
/// 返回值
/// A validated merged header map or a controlled pre-dispatch failure.
/// 返回已校验的合并请求头，或受控的分派前失败。
pub(crate) fn execution_headers(
    route: &ProviderExecutionRoute,
    protocol_headers: &std::collections::BTreeMap<String, String>,
    secret: &CredentialExecutionSecret,
    session_id: Option<&str>,
) -> Result<HeaderMap, ProviderExecutionFailure> {
    let mut headers = HeaderMap::new();
    headers.insert(
        reqwest::header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    // Streaming requests require byte-transparent SSE framing; compressed error responses
    // are still decoded by the configured reqwest content decoders if an upstream ignores this.
    // 流式请求要求 SSE 分帧保持字节透明；若上游忽略该设置，已配置的 reqwest 内容
    // 解码器仍会解码压缩错误响应。
    headers.insert(
        reqwest::header::ACCEPT_ENCODING,
        HeaderValue::from_static("identity"),
    );
    for (name, value) in protocol_headers {
        let name = HeaderName::from_bytes(name.as_bytes()).map_err(|_| {
            pre_dispatch_failure(format!(
                "Protocol profile produced invalid header name `{name}`."
            ))
        })?;
        if name == reqwest::header::AUTHORIZATION
            || name == HeaderName::from_static("x-api-key")
            || name == HeaderName::from_static("x-goog-api-key")
            || name == HeaderName::from_static(CODEX_RESPONSES_LITE_HEADER_NAME)
        {
            return Err(pre_dispatch_failure(format!(
                "Protocol profile must not provide host-owned header `{name}`."
            )));
        }
        let value = HeaderValue::from_str(value).map_err(|_| {
            pre_dispatch_failure(format!(
                "Protocol profile produced invalid value for header `{name}`."
            ))
        })?;
        headers.insert(name, value);
    }
    for (name, value) in &route.entry.request_headers {
        let name = HeaderName::from_bytes(name.as_bytes()).map_err(|_| {
            pre_dispatch_failure(format!(
                "Service entry produced invalid header name `{name}`."
            ))
        })?;
        if name == reqwest::header::AUTHORIZATION
            || name == HeaderName::from_static("x-api-key")
            || name == HeaderName::from_static("x-goog-api-key")
            || name == HeaderName::from_static("chatgpt-account-id")
            || name == HeaderName::from_static("x-goog-user-project")
            || name == reqwest::header::ACCEPT_ENCODING
            || name == reqwest::header::CONTENT_TYPE
            || name == HeaderName::from_static(CODEX_RESPONSES_LITE_HEADER_NAME)
        {
            return Err(pre_dispatch_failure(format!(
                "Service entry must not override reserved header `{name}`."
            )));
        }
        let value = HeaderValue::from_str(value).map_err(|_| {
            pre_dispatch_failure(format!(
                "Service entry produced invalid value for header `{name}`."
            ))
        })?;
        headers.insert(name, value);
    }
    for (name, value) in &route.protocol_route.request_headers {
        let name = HeaderName::from_bytes(name.as_bytes()).map_err(|_| {
            pre_dispatch_failure(format!(
                "Protocol route produced invalid header name `{name}`."
            ))
        })?;
        if name == reqwest::header::AUTHORIZATION
            || name == HeaderName::from_static("x-api-key")
            || name == HeaderName::from_static("x-goog-api-key")
            || name == HeaderName::from_static("chatgpt-account-id")
            || name == HeaderName::from_static("x-goog-user-project")
            || name == reqwest::header::ACCEPT_ENCODING
            || name == reqwest::header::CONTENT_TYPE
            || name == HeaderName::from_static(CODEX_RESPONSES_LITE_HEADER_NAME)
        {
            return Err(pre_dispatch_failure(format!(
                "Protocol route must not override reserved header `{name}`."
            )));
        }
        let value = HeaderValue::from_str(value).map_err(|_| {
            pre_dispatch_failure(format!(
                "Protocol route produced invalid value for header `{name}`."
            ))
        })?;
        headers.insert(name, value);
    }
    let credential_profile = route
        .entry
        .credential_profile(route.connection.credential_profile_id.as_str())
        .ok_or_else(|| {
            pre_dispatch_failure(format!(
                "Credential profile `{}` is not registered for service entry `{}`.",
                route.connection.credential_profile_id, route.entry.id
            ))
        })?;
    let authentication = route.protocol_route.resolved_authentication(
        route.entry.authentication,
        credential_profile.authentication,
    );
    match authentication {
        AuthenticationProjection::XApiKey => {
            headers.insert(
                HeaderName::from_static("x-api-key"),
                HeaderValue::from_str(secret.expose_value()).map_err(|_| {
                    pre_dispatch_failure("API key contains invalid header characters.")
                })?,
            );
        }
        AuthenticationProjection::OpenVulcanAuth => {
            // base_url uses the frozen connection, so an override cannot exfiltrate native credentials.
            // base_url 使用冻结连接，避免覆盖地址导出原生凭据。
            let base_url = crate::connection_endpoint::resolved_connection_protocol_base_url(
                &route.connection,
                &route.entry,
                &route.protocol_route,
            );
            let value = crate::openvulcan::authorization_header(secret, &base_url)
                .map_err(pre_dispatch_failure)?;
            headers.insert(
                reqwest::header::AUTHORIZATION,
                HeaderValue::from_str(&value).map_err(|_| {
                    pre_dispatch_failure("Invalid OpenVulcan authorization header.")
                })?,
            );
        }
        AuthenticationProjection::BearerAuthorization
        | AuthenticationProjection::CodingDeviceBearer => {
            headers.insert(
                reqwest::header::AUTHORIZATION,
                HeaderValue::from_str(format!("Bearer {}", secret.expose_value()).as_str())
                    .map_err(|_| {
                        pre_dispatch_failure(
                            "Bearer credential contains invalid header characters.",
                        )
                    })?,
            );
        }
        AuthenticationProjection::OpenAiCodexBearer => {
            headers.insert(
                reqwest::header::AUTHORIZATION,
                HeaderValue::from_str(format!("Bearer {}", secret.expose_value()).as_str())
                    .map_err(|_| {
                        pre_dispatch_failure(
                            "Codex bearer credential contains invalid header characters.",
                        )
                    })?,
            );
            let account_id = secret.account_id().ok_or_else(|| {
                pre_dispatch_failure("Codex credential does not contain an account identity.")
            })?;
            headers.insert(
                HeaderName::from_static("chatgpt-account-id"),
                HeaderValue::from_str(account_id).map_err(|_| {
                    pre_dispatch_failure(
                        "Codex account identity contains invalid header characters.",
                    )
                })?,
            );
        }
        AuthenticationProjection::GoogleCodeAssistBearer => {
            headers.insert(
                reqwest::header::AUTHORIZATION,
                HeaderValue::from_str(format!("Bearer {}", secret.expose_value()).as_str())
                    .map_err(|_| {
                        pre_dispatch_failure(
                            "Google bearer credential contains invalid header characters.",
                        )
                    })?,
            );
            if let Some(quota_project_id) = secret.quota_project_id() {
                headers.insert(
                    HeaderName::from_static("x-goog-user-project"),
                    HeaderValue::from_str(quota_project_id).map_err(|_| {
                        pre_dispatch_failure(
                            "Google quota project contains invalid header characters.",
                        )
                    })?,
                );
            }
        }
        AuthenticationProjection::GoogleGeminiBearer => {
            headers.insert(
                reqwest::header::AUTHORIZATION,
                HeaderValue::from_str(format!("Bearer {}", secret.expose_value()).as_str())
                    .map_err(|_| {
                        pre_dispatch_failure(
                            "Google Gemini bearer credential contains invalid header characters.",
                        )
                    })?,
            );
            let quota_project_id = secret.quota_project_id().ok_or_else(|| {
                pre_dispatch_failure(
                    "Public Gemini OAuth credential does not contain a Google Cloud project id.",
                )
            })?;
            headers.insert(
                HeaderName::from_static("x-goog-user-project"),
                HeaderValue::from_str(quota_project_id).map_err(|_| {
                    pre_dispatch_failure(
                        "Google Gemini quota project contains invalid header characters.",
                    )
                })?,
            );
        }
        AuthenticationProjection::GoogleApiKey => {
            headers.insert(
                HeaderName::from_static("x-goog-api-key"),
                HeaderValue::from_str(secret.expose_value()).map_err(|_| {
                    pre_dispatch_failure("Google API key contains invalid header characters.")
                })?,
            );
        }
    }
    if authentication == AuthenticationProjection::CodingDeviceBearer {
        insert_coding_device_identity_headers(&mut headers, secret)
            .map_err(pre_dispatch_failure)?;
    }
    if let Some(organization_id) = secret.opencode_organization_id() {
        // A Console token is only valid on its declared inference authority and workspace.
        // Console 令牌仅可用于其声明的推理授权端与工作区。
        let base_url = crate::connection_endpoint::resolved_connection_protocol_base_url(
            &route.connection,
            &route.entry,
            &route.protocol_route,
        );
        if base_url != crate::opencode::INFERENCE_URL {
            return Err(pre_dispatch_failure(
                "OpenCode OAuth requires the Console inference authority.",
            ));
        }
        headers.insert(
            HeaderName::from_static(crate::opencode::ORGANIZATION_HEADER),
            HeaderValue::from_str(organization_id).map_err(|_| {
                pre_dispatch_failure("OpenCode workspace contains invalid header characters.")
            })?,
        );
    }
    // Every protocol identifies VulcanCode itself; no route borrows another client's identity.
    // 所有协议都使用 VulcanCode 自身身份，任何路由都不借用其他客户端身份。
    headers.insert(
        reqwest::header::USER_AGENT,
        HeaderValue::from_str(vulcan_code_user_agent()).map_err(|_| {
            pre_dispatch_failure("The VulcanCode user agent contains invalid header characters.")
        })?,
    );
    // Lite is an exact frozen native-model behavior. The host emits its selector after all
    // provider declarations have been validated so no manifest or profile may spoof it.
    // Lite 是精确冻结的原生模型行为。宿主在全部供应商声明校验后写入选择头，
    // 因而任何清单或 Profile 都不能伪造该标记。
    if route_uses_native_codex_responses(route)
        && route
            .codex_behavior
            .as_ref()
            .is_some_and(|behavior| behavior.wire_dialect == CodexWireDialect::Lite)
    {
        headers.insert(
            HeaderName::from_static(CODEX_RESPONSES_LITE_HEADER_NAME),
            HeaderValue::from_static("true"),
        );
    }
    // Session projection is last so every already materialized host or provider header can win.
    // 会话投影最后执行，确保所有已经生成的宿主或供应商请求头优先保留。
    inject_session_header(&mut headers, route, session_id)?;
    Ok(headers)
}

/// Injects the configured session identifier into the final provider header map.
/// 将已配置的会话标识注入最终供应商请求头映射。
///
/// # Parameters
/// 参数
/// `headers` is the common final header map, `route` supplies the optional header declaration,
/// and `session_id` supplies the current public session scope.
/// `headers` 是公共最终请求头映射，`route` 提供可选请求头声明，`session_id` 提供当前公共会话作用域。
///
/// # Returns
/// 返回值
/// Success when the declaration is absent or the session header is safely projected.
/// 当未声明配置，或会话请求头已安全投影时返回成功。
fn inject_session_header(
    headers: &mut HeaderMap,
    route: &ProviderExecutionRoute,
    session_id: Option<&str>,
) -> Result<(), ProviderExecutionFailure> {
    // Session headers are a conversation-only contract; keep the runtime guard even if a malformed route bypasses manifest validation.
    // 会话请求头只属于常规会话契约；即使异常路由绕过 Manifest 校验，也保留运行时防线。
    if route.entry.kind != crate::ServiceEntryKind::Conversation {
        return Ok(());
    }
    let Some(header_name) = route.entry.session_header.as_deref() else {
        return Ok(());
    };
    let name = HeaderName::from_bytes(header_name.as_bytes()).map_err(|_| {
        pre_dispatch_failure(format!(
            "Service entry produced invalid session header name `{header_name}`."
        ))
    })?;
    let Some(session_id) = session_id else {
        return Ok(());
    };
    // A session declaration never replaces an existing or host-owned header.
    // 会话声明绝不替换已经存在或由宿主持有的请求头。
    if headers.contains_key(&name) || is_host_owned_session_header(name.as_str()) {
        return Ok(());
    }
    let value = HeaderValue::from_str(session_id).map_err(|_| {
        pre_dispatch_failure("Session identifier contains invalid header characters.")
    })?;
    headers.insert(name, value);
    Ok(())
}

/// Inserts the Kimi Coding device identity shared by conversation, tool, and upload requests.
/// 插入由会话、工具与上传请求共用的 Kimi Coding 设备身份。
///
/// # Parameters
/// 参数
/// `headers` receives the exact identity fields and `secret` supplies the frozen provider device id.
/// `headers` 接收精确身份字段，`secret` 提供冻结的供应商设备标识。
///
/// # Returns
/// 返回值
/// Success after every bounded header value is inserted, or a controlled validation error.
/// 全部有界请求头值插入后返回成功，否则返回受控校验错误。
pub(crate) fn insert_coding_device_identity_headers(
    headers: &mut HeaderMap,
    secret: &CredentialExecutionSecret,
) -> Result<(), String> {
    headers.extend(coding_device_identity_headers(secret.provider_device_id())?);
    Ok(())
}

/// Builds VulcanCode's own Kimi Coding device identity for authorization, usage, and execution.
/// 为授权、用量与执行构建 VulcanCode 自身的 Kimi Coding 设备身份。
///
/// # Parameters
/// 参数
/// `device_id` is the frozen provider device id; `None` sends the shared VulcanCode placeholder.
/// `device_id` 是冻结的供应商设备标识；为 `None` 时发送共享的 VulcanCode 占位值。
///
/// # Returns
/// 返回值
/// The complete identity header set, or a controlled validation error.
/// 完整身份请求头集合，或受控校验错误。
pub(crate) fn coding_device_identity_headers(device_id: Option<&str>) -> Result<HeaderMap, String> {
    let device_name = std::env::var("COMPUTERNAME")
        .or_else(|_| std::env::var("HOSTNAME"))
        .unwrap_or_else(|_| String::from("unknown"));
    let device_model = coding_device_model();
    let mut headers = HeaderMap::new();
    for (name, value) in [
        ("user-agent", vulcan_code_user_agent()),
        ("x-msh-platform", VULCAN_CODE_CLIENT_NAME),
        ("x-msh-version", vulcan_code_version()),
        ("x-msh-device-name", device_name.as_str()),
        ("x-msh-device-model", device_model.as_str()),
        ("x-msh-device-id", device_id.unwrap_or("vulcan-code-device")),
    ] {
        headers.insert(
            HeaderName::from_static(name),
            HeaderValue::from_str(value)
                .map_err(|_| String::from("Coding-device identity is invalid."))?,
        );
    }
    Ok(headers)
}

/// Returns the provider-facing operating-system label and architecture of this device.
/// 返回本设备面向供应商的操作系统标签与架构。
pub(crate) fn coding_device_model() -> String {
    let operating_system = match std::env::consts::OS {
        "darwin" => "macOS",
        "windows" => "Windows",
        "linux" => "Linux",
        other => other,
    };
    format!("{operating_system} {}", std::env::consts::ARCH)
}

/// Copies validated HTTP headers into the secret-safe WebSocket handshake container.
/// 将已校验 HTTP Header 复制到不实现调试输出的 WebSocket 握手容器。
pub(super) fn websocket_headers(
    headers: &HeaderMap,
) -> Result<BTreeMap<String, String>, ProviderExecutionFailure> {
    headers
        .iter()
        .map(|(name, value)| {
            value
                .to_str()
                .map(|value| (name.as_str().to_string(), value.to_string()))
                .map_err(|error| {
                    pre_dispatch_failure(format!(
                        "Responses WebSocket header `{name}` is not valid UTF-8: {error}"
                    ))
                })
        })
        .collect()
}

#[cfg(test)]
mod tests;
