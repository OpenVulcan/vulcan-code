//! Credential-scoped service-origin overrides for provider execution.
//! 供应商执行使用的凭据作用域服务源地址覆盖。

use crate::{CredentialConnection, ProtocolRoute, ProviderToolTarget, ServiceEntry};

/// Normalizes one credential-scoped service origin after enforcing the provider contract.
/// 在强制执行供应商契约后规范化一个凭据作用域服务源地址。
///
/// # Parameters
/// 参数
/// `provider_id` and `service_entry_id` identify the exact product, while `value` may be either
/// the workspace origin or one of its supported OpenAI/Anthropic compatibility URLs.
/// `provider_id` 与 `service_entry_id` 标识精确产品，`value` 可以是工作空间源地址，
/// 也可以是其支持的 OpenAI 或 Anthropic 兼容地址。
///
/// # Returns
/// 返回值
/// A canonical origin without a protocol path, `None` for an empty value, or an explicit error.
/// 返回不含协议路径的规范源地址、空值对应的 `None`，或明确错误。
pub fn normalize_connection_service_origin_override(
    provider_id: &str,
    service_entry_id: &str,
    value: Option<&str>,
) -> Result<Option<String>, String> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    if provider_id != "alibaba"
        || !matches!(
            service_entry_id,
            "model-studio-cn" | "model-studio-singapore"
        )
    {
        return Err(String::from(
            "A credential-scoped service origin is supported only for Alibaba Cloud Model Studio entries.",
        ));
    }
    let parsed = reqwest::Url::parse(value)
        .map_err(|_| String::from("The Model Studio service origin is not a valid URL."))?;
    if parsed.scheme() != "https"
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.port().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
    {
        return Err(String::from(
            "The Model Studio service origin must be an HTTPS URL without credentials, port, query, or fragment.",
        ));
    }
    let expected_region = match service_entry_id {
        "model-studio-cn" => "cn-beijing",
        "model-studio-singapore" => "ap-southeast-1",
        _ => unreachable!("the service entry was checked above"),
    };
    let expected_suffix = format!(".{expected_region}.maas.aliyuncs.com");
    let host = parsed
        .host_str()
        .ok_or_else(|| String::from("The Model Studio service origin must include a host."))?;
    let workspace_id = host.strip_suffix(expected_suffix.as_str()).ok_or_else(|| {
        format!(
            "The Model Studio service origin must belong to the `{expected_region}.maas.aliyuncs.com` region."
        )
    })?;
    if workspace_id.is_empty() || workspace_id.contains('.') {
        return Err(String::from(
            "The Model Studio service origin must use one exact workspace-dedicated or trial domain.",
        ));
    }
    let path = parsed.path().trim_end_matches('/');
    if !matches!(path, "" | "/compatible-mode/v1" | "/apps/anthropic") {
        return Err(String::from(
            "The Model Studio service origin path must be empty, `/compatible-mode/v1`, or `/apps/anthropic`.",
        ));
    }
    Ok(Some(format!("https://{host}")))
}

/// Resolves the protocol base URL for one frozen credential route.
/// 为一个冻结凭据路由解析协议基础地址。
///
/// # Parameters
/// 参数
/// `connection`, `entry`, and `route` are the exact immutable execution snapshots.
/// `connection`、`entry` 与 `route` 是精确且不可变的执行快照。
///
/// # Returns
/// 返回凭据覆盖后的协议基础地址，或服务入口声明的默认地址。
pub(crate) fn resolved_connection_protocol_base_url(
    connection: &CredentialConnection,
    entry: &ServiceEntry,
    route: &ProtocolRoute,
) -> String {
    let Some(origin) = connection.service_origin_override.as_deref() else {
        return route.resolved_base_url(entry).to_string();
    };
    match route.protocol_profile_id.as_str() {
        vulcan_ai_protocols::ANTHROPIC_MESSAGES_PROFILE_ID => {
            format!("{origin}/apps/anthropic")
        }
        vulcan_ai_protocols::OPENAI_CHAT_PROFILE_ID
        | vulcan_ai_protocols::OPENAI_RESPONSES_PROFILE_ID => {
            format!("{origin}/compatible-mode/v1")
        }
        _ => route.resolved_base_url(entry).to_string(),
    }
}

/// Applies one credential-scoped origin to a frozen provider-tool target.
/// 将一个凭据作用域源地址应用到冻结的供应商工具目标。
///
/// # Parameters
/// 参数
/// `connection` owns the optional origin and `target` is the exact native tool contract.
/// `connection` 持有可选源地址，`target` 是精确原生工具契约。
///
/// # Returns
/// 返回保持其他契约字段不变且带有效执行源地址的目标。
pub(crate) fn apply_connection_provider_tool_origin(
    connection: &CredentialConnection,
    mut target: ProviderToolTarget,
) -> ProviderToolTarget {
    if connection.provider_id == "alibaba"
        && matches!(
            connection.service_entry_id.as_str(),
            "model-studio-cn" | "model-studio-singapore"
        )
        && let Some(origin) = connection.service_origin_override.as_ref()
    {
        target.execution.endpoint_base_url = Some(origin.clone());
    }
    target
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CredentialMode, built_in_provider_tool_targets};
    use std::collections::{BTreeMap, BTreeSet};

    /// Accepts both full compatibility URLs and canonicalizes them to one origin.
    /// 接受完整兼容地址并将其规范化为同一个源地址。
    #[test]
    fn normalizes_model_studio_workspace_urls() {
        let host = "https://ws-example.ap-southeast-1.maas.aliyuncs.com";
        assert_eq!(
            normalize_connection_service_origin_override(
                "alibaba",
                "model-studio-singapore",
                Some(format!("{host}/compatible-mode/v1").as_str()),
            )
            .expect("OpenAI compatibility URL"),
            Some(host.to_string())
        );
        assert_eq!(
            normalize_connection_service_origin_override(
                "alibaba",
                "model-studio-singapore",
                Some(format!("{host}/apps/anthropic").as_str()),
            )
            .expect("Anthropic compatibility URL"),
            Some(host.to_string())
        );
    }

    /// Rejects cross-region and unsupported-provider origins before persistence.
    /// 在持久化前拒绝跨区域及不受支持供应商的源地址。
    #[test]
    fn rejects_invalid_service_origin_scope() {
        assert!(
            normalize_connection_service_origin_override(
                "alibaba",
                "model-studio-cn",
                Some("https://ws-example.ap-southeast-1.maas.aliyuncs.com"),
            )
            .is_err()
        );
        assert!(
            normalize_connection_service_origin_override(
                "openrouter",
                "public-api",
                Some("https://ws-example.ap-southeast-1.maas.aliyuncs.com"),
            )
            .is_err()
        );
    }

    /// Verifies provider-native tools use the same credential workspace as conversation calls.
    /// 验证供应商原生工具与会话调用使用同一凭据工作空间。
    #[test]
    fn applies_workspace_origin_to_provider_tools() {
        let connection = CredentialConnection {
            id: String::from("connection"),
            provider_id: String::from("alibaba"),
            service_entry_id: String::from("model-studio-singapore"),
            credential_profile_id: String::from("api-key"),
            credential_mode: CredentialMode::Token,
            credential_id: String::from("credential"),
            display_name: String::from("Workspace"),
            secret_reference: String::from("secret"),
            management_secret_reference: None,
            service_origin_override: Some(String::from(
                "https://ws-example.ap-southeast-1.maas.aliyuncs.com",
            )),
            verified_protocols: BTreeSet::new(),
            selected_protocol: String::new(),
            prompt_cache_by_protocol: BTreeMap::new(),
            usage_scope_override: None,
        };
        let target = built_in_provider_tool_targets("alibaba")
            .into_iter()
            .find(|target| {
                target.service_entry_id == "model-studio-singapore"
                    && target.contract.capability == crate::ProviderToolCapabilityKind::WebSearch
            })
            .expect("Singapore WebSearch target");
        let target = apply_connection_provider_tool_origin(&connection, target);
        assert_eq!(
            target.execution.endpoint_base_url.as_deref(),
            Some("https://ws-example.ap-southeast-1.maas.aliyuncs.com")
        );
    }
}
