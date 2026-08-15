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
    entry: &ServiceEntry,
    value: Option<&str>,
) -> Result<Option<String>, String> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    let Some(policy) = entry.service_origin_override_policy.as_ref() else {
        return Err(String::from(
            "The selected service entry does not support a credential-scoped service origin.",
        ));
    };
    let parsed = reqwest::Url::parse(value)
        .map_err(|_| String::from("The service origin is not a valid URL."))?;
    if parsed.scheme() != "https"
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.port().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
    {
        return Err(String::from(
            "The service origin must be an HTTPS URL without credentials, port, query, or fragment.",
        ));
    }
    let host = parsed
        .host_str()
        .ok_or_else(|| String::from("The service origin must include a host."))?;
    let workspace_id = host
        .strip_suffix(policy.host_suffix.as_str())
        .ok_or_else(|| {
            format!(
                "The service origin host must end with `{}`.",
                policy.host_suffix
            )
        })?;
    if workspace_id.is_empty() || workspace_id.contains('.') {
        return Err(String::from(
            "The service origin must use one exact workspace-scoped domain label.",
        ));
    }
    let path = parsed.path().trim_end_matches('/');
    if !policy.allowed_input_paths.contains(path) {
        return Err(format!(
            "The service origin path `{path}` is not allowed by service entry `{}`.",
            entry.id
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
    entry
        .service_origin_override_policy
        .as_ref()
        .and_then(|policy| policy.protocol_base_paths.get(&route.protocol_profile_id))
        .map(|path| format!("{origin}{path}"))
        .unwrap_or_else(|| route.resolved_base_url(entry).to_string())
}

/// Materializes the exact effective origin into a provider-tool target before it is frozen.
/// 在供应商工具目标冻结前，将精确的最终有效源地址实体化到目标中。
///
/// # Parameters
/// 参数
/// `connection` owns the optional credential override, `entry` owns the service default, and
/// `target` is the exact native tool contract that may declare its own endpoint.
/// `connection` 持有可选凭据覆盖地址，`entry` 持有服务默认地址，`target` 是可声明自身端点的
/// 精确原生工具契约。
///
/// # Returns
/// Returns the unchanged contract with one self-contained effective execution origin.
/// 返回其他契约字段不变且包含自足最终有效执行源地址的目标。
pub(crate) fn apply_connection_provider_tool_origin(
    connection: &CredentialConnection,
    entry: &ServiceEntry,
    mut target: ProviderToolTarget,
) -> ProviderToolTarget {
    // The effective origin follows the generic contract precedence and is always persisted into
    // the target so an asynchronous frozen snapshot never depends on a later service-entry value.
    // 最终有效源地址遵循通用契约优先级并始终写入目标，使异步冻结快照绝不依赖之后的服务入口值。
    let resolved_origin = connection
        .service_origin_override
        .as_ref()
        .or(target.execution.endpoint_base_url.as_ref())
        .unwrap_or(&entry.base_url)
        .clone();
    target.execution.endpoint_base_url = Some(resolved_origin);
    target
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CredentialMode, ProviderRegistry};
    use std::collections::BTreeMap;

    /// Resolves one bundled service entry used by origin-policy tests.
    /// 解析源地址策略测试使用的一个内置服务入口。
    fn bundled_entry(provider_id: &str, service_entry_id: &str) -> ServiceEntry {
        ProviderRegistry::first_batch()
            .expect("bundled registry")
            .provider(provider_id)
            .expect("bundled provider")
            .service_entries
            .iter()
            .find(|entry| entry.id == service_entry_id)
            .expect("bundled service entry")
            .clone()
    }

    /// Accepts both full compatibility URLs and canonicalizes them to one origin.
    /// 接受完整兼容地址并将其规范化为同一个源地址。
    #[test]
    fn normalizes_model_studio_workspace_urls() {
        let singapore = bundled_entry("alibaba", "model-studio-singapore");
        let host = "https://ws-example.ap-southeast-1.maas.aliyuncs.com";
        assert_eq!(
            normalize_connection_service_origin_override(
                &singapore,
                Some(format!("{host}/compatible-mode/v1").as_str()),
            )
            .expect("OpenAI compatibility URL"),
            Some(host.to_string())
        );
        assert_eq!(
            normalize_connection_service_origin_override(
                &singapore,
                Some(format!("{host}/apps/anthropic").as_str()),
            )
            .expect("Anthropic compatibility URL"),
            Some(host.to_string())
        );
        let china = bundled_entry("alibaba", "model-studio-cn");
        let china_host = "https://ws-example.cn-beijing.maas.aliyuncs.com";
        assert_eq!(
            normalize_connection_service_origin_override(
                &china,
                Some(format!("{china_host}/compatible-api/v1").as_str()),
            )
            .expect("China compatible API URL"),
            Some(china_host.to_string())
        );
    }

    /// Rejects cross-region and unsupported-provider origins before persistence.
    /// 在持久化前拒绝跨区域及不受支持供应商的源地址。
    #[test]
    fn rejects_invalid_service_origin_scope() {
        let china = bundled_entry("alibaba", "model-studio-cn");
        assert!(
            normalize_connection_service_origin_override(
                &china,
                Some("https://ws-example.ap-southeast-1.maas.aliyuncs.com"),
            )
            .is_err()
        );
        let openrouter = bundled_entry("openrouter", "public-api");
        assert!(
            normalize_connection_service_origin_override(
                &openrouter,
                Some("https://ws-example.ap-southeast-1.maas.aliyuncs.com"),
            )
            .is_err()
        );
    }

    /// Verifies provider-native tools use the same credential workspace as conversation calls.
    /// 验证供应商原生工具与会话调用使用同一凭据工作空间。
    #[test]
    fn applies_workspace_origin_to_provider_tools() {
        // This connection supplies the highest-priority credential-scoped origin.
        // 此连接提供最高优先级的凭据作用域源地址。
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
            prompt_cache_by_protocol: BTreeMap::new(),
        };
        // The selected entry owns the fallback origin for targets without an explicit endpoint.
        // 所选服务入口为未显式声明端点的目标持有回退源地址。
        let entry = bundled_entry("alibaba", "model-studio-singapore");
        // The manifest target represents the shared contract used by both precedence assertions.
        // Manifest 目标表示两项优先级断言共同使用的契约。
        let target = crate::provider_manifest::load_bundled_resolved_catalog()
            .manifest("alibaba")
            .expect("Alibaba manifest")
            .tool_targets
            .clone()
            .into_iter()
            .find(|target| {
                target.service_entry_id == "model-studio-singapore"
                    && target.contract.capability == crate::ProviderToolCapabilityKind::WebSearch
            })
            .expect("Singapore WebSearch target");
        // A connection override must win over both target and service-entry values.
        // 连接覆盖地址必须优先于目标值与服务入口值。
        let target_with_connection_origin =
            apply_connection_provider_tool_origin(&connection, &entry, target.clone());
        assert_eq!(
            target_with_connection_origin
                .execution
                .endpoint_base_url
                .as_deref(),
            Some("https://ws-example.ap-southeast-1.maas.aliyuncs.com")
        );

        // Removing both higher-priority values proves the service-entry fallback is materialized.
        // 移除两个更高优先级的值，证明服务入口回退地址会被实体化。
        let mut default_connection = connection;
        default_connection.service_origin_override = None;
        // This target supplies the middle-priority endpoint independently from the service entry.
        // 此目标独立于服务入口提供中间优先级的端点地址。
        let mut target_with_explicit_endpoint = target.clone();
        target_with_explicit_endpoint.execution.endpoint_base_url =
            Some(String::from("https://target-explicit.example"));
        // Without a connection override, the explicit target endpoint must remain authoritative.
        // 没有连接覆盖地址时，目标显式端点必须保持权威。
        let target_with_explicit_origin = apply_connection_provider_tool_origin(
            &default_connection,
            &entry,
            target_with_explicit_endpoint,
        );
        assert_eq!(
            target_with_explicit_origin
                .execution
                .endpoint_base_url
                .as_deref(),
            Some("https://target-explicit.example")
        );

        // This target intentionally omits its endpoint to exercise the final fallback branch.
        // 此目标特意省略自身端点，以覆盖最后一级回退分支。
        let mut target_without_endpoint = target;
        target_without_endpoint.execution.endpoint_base_url = None;
        // The resolved snapshot must carry the entry URL instead of retaining an implicit lookup.
        // 解析后的快照必须携带入口地址，而不是保留隐式查询。
        let target_with_entry_origin = apply_connection_provider_tool_origin(
            &default_connection,
            &entry,
            target_without_endpoint,
        );
        assert_eq!(
            target_with_entry_origin
                .execution
                .endpoint_base_url
                .as_deref(),
            Some(entry.base_url.as_str())
        );
    }
}
