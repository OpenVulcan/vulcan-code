//! Plugin-owned MCP server materialization into source-neutral runtime definitions.
//! 将插件自有 MCP 服务器物化为中立运行时定义。

use crate::mcp::config::{
    McpServerConfig, McpServerOAuthConfig, McpServerSource, McpServerTransportConfig,
};
use reqwest::Url;
use serde_json::Value;
use std::collections::BTreeMap;
use vulcan_plugin_runtime::{
    VulcanPluginMcpOAuthManifest, VulcanPluginMcpServerDescriptor,
    VulcanPluginMcpTransportManifest, VulcanPluginRuntimeManager, VulcanPluginValueSourceManifest,
};

/// Host boundary for reading one plugin-scoped secret without exposing storage details to plugins.
/// 用于读取单个插件作用域 Secret 且不向插件暴露存储细节的宿主边界。
pub trait PluginMcpSecretStore: Send + Sync {
    /// Reads one exact secret scoped to a plugin MCP server.
    /// 读取限定到某个插件 MCP 服务器的精确 Secret。
    ///
    /// # Parameters
    /// 参数
    /// Plugin, server, and secret identifiers form the complete host-owned namespace.
    /// 插件、服务器与 Secret 标识共同组成完整的宿主持有命名空间。
    ///
    /// # Returns
    /// 返回值
    /// Returns the secret value or a non-sensitive diagnostic when it is unavailable.
    /// 返回 Secret 值；不可用时返回不含敏感信息的诊断。
    fn read_secret(
        &self,
        plugin_id: &str,
        server_id: &str,
        secret_key: &str,
    ) -> Result<String, String>;
}

/// Materializes every enabled plugin MCP declaration into source-neutral runtime configuration.
/// 将全部已启用插件 MCP 声明物化为中立运行时配置。
///
/// # Parameters
/// 参数
/// `plugins` owns the immutable catalog and managed launchers; `secrets` owns credential retrieval.
/// `plugins` 持有不可变目录与受管启动器；`secrets` 持有凭据读取。
///
/// # Returns
/// 返回值
/// Returns deterministic server definitions or fails without publishing a partial plugin surface.
/// 返回确定性的服务器定义；失败时不发布部分插件表面。
pub fn plugin_mcp_server_configs(
    plugins: &VulcanPluginRuntimeManager,
    secrets: &dyn PluginMcpSecretStore,
) -> Result<Vec<McpServerConfig>, String> {
    let snapshot = plugins.snapshot();
    let servers = snapshot
        .enabled_mcp_servers
        .values()
        .cloned()
        .collect::<Vec<_>>();
    plugin_mcp_server_configs_for_descriptors(plugins, &servers, secrets)
}

/// Materializes an exact validated plugin descriptor set for candidate lifecycle readiness checks.
/// 为候选生命周期就绪检查物化一组精确的已校验插件描述符。
///
/// # Parameters
/// 参数
/// `plugins` owns installed package and launcher state, `servers` belongs to one candidate plugin generation, and `secrets` resolves host credentials.
/// `plugins` 持有已安装包与启动器状态，`servers` 属于单个候选插件代次，`secrets` 解析宿主凭据。
///
/// # Returns
/// 返回值
/// Returns the complete source-neutral candidate list or fails before any runtime partition is published.
/// 返回完整的中立候选列表；失败时不会发布任何运行时分区。
pub fn plugin_mcp_server_configs_for_descriptors(
    plugins: &VulcanPluginRuntimeManager,
    servers: &[VulcanPluginMcpServerDescriptor],
    secrets: &dyn PluginMcpSecretStore,
) -> Result<Vec<McpServerConfig>, String> {
    servers
        .iter()
        .map(|server| plugin_mcp_server_config(plugins, server, secrets))
        .collect()
}

/// Materializes one validated plugin MCP descriptor without admitting alternate field sources.
/// 物化单个已校验插件 MCP 描述符，且不接纳替代字段来源。
fn plugin_mcp_server_config(
    plugins: &VulcanPluginRuntimeManager,
    server: &VulcanPluginMcpServerDescriptor,
    secrets: &dyn PluginMcpSecretStore,
) -> Result<McpServerConfig, String> {
    // config is loaded only when this exact server declaration names a Config source.
    // config 仅在当前精确服务器声明使用 Config 来源时加载。
    let config = if transport_uses_plugin_config(&server.manifest.transport) {
        plugin_server_configuration(plugins, &server.plugin_id)?
    } else {
        None
    };
    let transport = match &server.manifest.transport {
        VulcanPluginMcpTransportManifest::Stdio { args, env, .. } => {
            let launcher = plugins
                .resolve_mcp_stdio_launcher_descriptor(server)
                .map_err(|error| error.to_string())?;
            let mut resolved_args = launcher.base_args;
            resolved_args.extend(
                args.iter()
                    .map(|value| {
                        resolve_plugin_value(
                            value,
                            &server.plugin_id,
                            &server.id,
                            config.as_ref(),
                            secrets,
                        )
                    })
                    .collect::<Result<Vec<_>, _>>()?,
            );
            let mut resolved_env = launcher.env;
            for (name, value) in env {
                resolved_env.insert(
                    name.clone(),
                    resolve_plugin_value(
                        value,
                        &server.plugin_id,
                        &server.id,
                        config.as_ref(),
                        secrets,
                    )?,
                );
            }
            McpServerTransportConfig::Stdio {
                command: vulcan_runtime_assets::native_host_path_text_from_path(&launcher.program),
                args: resolved_args,
                cwd: Some(vulcan_runtime_assets::native_host_path_text_from_path(
                    &launcher.cwd,
                )),
                env: resolved_env,
                env_passthrough: Vec::new(),
                inherit_environment: false,
            }
        }
        VulcanPluginMcpTransportManifest::Http {
            url,
            headers,
            oauth,
        } => McpServerTransportConfig::Http {
            url: resolve_remote_url(url, &server.plugin_id, &server.id, config.as_ref(), secrets)?,
            bearer_token: None,
            headers: resolve_headers(
                headers,
                &server.plugin_id,
                &server.id,
                config.as_ref(),
                secrets,
            )?,
            headers_from_env: BTreeMap::new(),
            oauth: resolve_oauth(
                oauth.as_ref(),
                &server.plugin_id,
                &server.id,
                config.as_ref(),
                secrets,
            )?,
        },
        VulcanPluginMcpTransportManifest::Sse {
            url,
            headers,
            oauth,
        } => McpServerTransportConfig::Sse {
            url: resolve_remote_url(url, &server.plugin_id, &server.id, config.as_ref(), secrets)?,
            bearer_token: None,
            headers: resolve_headers(
                headers,
                &server.plugin_id,
                &server.id,
                config.as_ref(),
                secrets,
            )?,
            headers_from_env: BTreeMap::new(),
            oauth: resolve_oauth(
                oauth.as_ref(),
                &server.plugin_id,
                &server.id,
                config.as_ref(),
                secrets,
            )?,
        },
    };
    Ok(McpServerConfig {
        source: McpServerSource::Plugin {
            plugin_id: server.plugin_id.clone(),
            generation: server.plugin_generation,
        },
        id: format!("{}:{}", server.plugin_id, server.id),
        name: server.name.clone(),
        enabled: true,
        transport,
    })
}

/// Returns whether one MCP transport contains at least one explicit plugin-configuration source.
/// 返回单个 MCP 传输是否包含至少一个显式插件配置来源。
///
/// # Parameters
/// 参数
/// `transport` is the exact validated server declaration under inspection.
/// `transport` 是正在检查的精确已校验服务器声明。
///
/// # Returns
/// 返回值
/// Returns `true` only when materialization must read the plugin configuration document.
/// 仅当物化过程必须读取插件配置文档时返回 `true`。
fn transport_uses_plugin_config(transport: &VulcanPluginMcpTransportManifest) -> bool {
    match transport {
        VulcanPluginMcpTransportManifest::Stdio { args, env, .. } => args
            .iter()
            .chain(env.values())
            .any(value_source_uses_plugin_config),
        VulcanPluginMcpTransportManifest::Http {
            url,
            headers,
            oauth,
        }
        | VulcanPluginMcpTransportManifest::Sse {
            url,
            headers,
            oauth,
        } => {
            value_source_uses_plugin_config(url)
                || headers.values().any(value_source_uses_plugin_config)
                || oauth.as_ref().is_some_and(|oauth| {
                    [
                        oauth.client_id.as_ref(),
                        oauth.client_secret.as_ref(),
                        oauth.redirect_uri.as_ref(),
                    ]
                    .into_iter()
                    .flatten()
                    .any(value_source_uses_plugin_config)
                })
        }
    }
}

/// Returns whether one unambiguous value source reads from the plugin configuration document.
/// 返回单个无歧义值来源是否读取插件配置文档。
///
/// # Parameters
/// 参数
/// `source` is one exact manifest value source.
/// `source` 是一个精确清单值来源。
///
/// # Returns
/// 返回值
/// Returns `true` only for the `Config` variant.
/// 仅对 `Config` 变体返回 `true`。
fn value_source_uses_plugin_config(source: &VulcanPluginValueSourceManifest) -> bool {
    matches!(source, VulcanPluginValueSourceManifest::Config { .. })
}

/// Reads plugin configuration only when the package declares a configuration contract.
/// 仅当插件包声明配置契约时读取插件配置。
fn plugin_server_configuration(
    plugins: &VulcanPluginRuntimeManager,
    plugin_id: &str,
) -> Result<Option<Value>, String> {
    let snapshot = plugins.snapshot();
    let plugin = snapshot
        .plugins
        .get(plugin_id)
        .ok_or_else(|| format!("Vulcan plugin `{plugin_id}` is not installed."))?;
    let manifest = plugin
        .manifest
        .as_ref()
        .ok_or_else(|| format!("Vulcan plugin `{plugin_id}` has no valid manifest."))?;
    manifest.config.as_ref().map_or(Ok(None), |_| {
        plugins
            .plugin_config(plugin_id)
            .map(Some)
            .map_err(|error| error.to_string())
    })
}

/// Resolves one typed plugin value from its single declared source.
/// 从单一声明来源解析一个带类型插件值。
fn resolve_plugin_value(
    source: &VulcanPluginValueSourceManifest,
    plugin_id: &str,
    server_id: &str,
    config: Option<&Value>,
    secrets: &dyn PluginMcpSecretStore,
) -> Result<String, String> {
    match source {
        VulcanPluginValueSourceManifest::Literal { value } => Ok(value.clone()),
        VulcanPluginValueSourceManifest::Config { key } => config
            .and_then(Value::as_object)
            .and_then(|object| object.get(key))
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| {
                format!(
                    "Vulcan plugin MCP server `{plugin_id}:{server_id}` requires string configuration key `{key}`."
                )
            }),
        VulcanPluginValueSourceManifest::Secret { key } => {
            secrets.read_secret(plugin_id, server_id, key)
        }
        VulcanPluginValueSourceManifest::Environment { name } => std::env::var(name).map_err(|_| {
            format!(
                "Vulcan plugin MCP server `{plugin_id}:{server_id}` requires host environment variable `{name}`."
            )
        }),
    }
}

/// Resolves and validates one credential-free HTTPS endpoint.
/// 解析并校验一个不含嵌入凭据的 HTTPS 端点。
fn resolve_remote_url(
    source: &VulcanPluginValueSourceManifest,
    plugin_id: &str,
    server_id: &str,
    config: Option<&Value>,
    secrets: &dyn PluginMcpSecretStore,
) -> Result<String, String> {
    let value = resolve_plugin_value(source, plugin_id, server_id, config, secrets)?;
    let url = Url::parse(&value).map_err(|error| {
        format!("Vulcan plugin MCP server `{plugin_id}:{server_id}` URL is invalid: {error}")
    })?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(format!(
            "Vulcan plugin MCP server `{plugin_id}:{server_id}` URL must be credential-free, fragment-free HTTPS."
        ));
    }
    Ok(value)
}

/// Resolves exact remote request headers without retaining their source metadata.
/// 解析精确远端请求 Header，且不保留其来源元数据。
fn resolve_headers(
    headers: &BTreeMap<String, VulcanPluginValueSourceManifest>,
    plugin_id: &str,
    server_id: &str,
    config: Option<&Value>,
    secrets: &dyn PluginMcpSecretStore,
) -> Result<BTreeMap<String, String>, String> {
    headers
        .iter()
        .map(|(name, source)| {
            resolve_plugin_value(source, plugin_id, server_id, config, secrets)
                .map(|value| (name.clone(), value))
        })
        .collect()
}

/// Resolves optional plugin OAuth settings through the same exact value-source boundary.
/// 通过同一精确值来源边界解析可选插件 OAuth 设置。
fn resolve_oauth(
    oauth: Option<&VulcanPluginMcpOAuthManifest>,
    plugin_id: &str,
    server_id: &str,
    config: Option<&Value>,
    secrets: &dyn PluginMcpSecretStore,
) -> Result<Option<McpServerOAuthConfig>, String> {
    oauth
        .map(|oauth| {
            Ok(McpServerOAuthConfig {
                client_id: oauth
                    .client_id
                    .as_ref()
                    .map(|source| {
                        resolve_plugin_value(source, plugin_id, server_id, config, secrets)
                    })
                    .transpose()?,
                client_secret: oauth
                    .client_secret
                    .as_ref()
                    .map(|source| {
                        resolve_plugin_value(source, plugin_id, server_id, config, secrets)
                    })
                    .transpose()?,
                scopes: oauth.scopes.clone(),
                redirect_uri: oauth
                    .redirect_uri
                    .as_ref()
                    .map(|source| {
                        resolve_plugin_value(source, plugin_id, server_id, config, secrets)
                    })
                    .transpose()?,
            })
        })
        .transpose()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// In-memory Secret Store fixture that exposes only exact namespaced values.
    /// 仅暴露精确命名空间值的内存 Secret Store 夹具。
    struct TestSecretStore {
        /// Values keyed by plugin, server, and declared secret key.
        /// 以插件、服务器及已声明 Secret Key 为键的值集合。
        values: BTreeMap<(String, String, String), String>,
    }

    impl PluginMcpSecretStore for TestSecretStore {
        /// Reads one exact test secret without any alternate namespace lookup.
        /// 读取单个精确测试 Secret，不执行任何替代命名空间查找。
        ///
        /// # Parameters
        /// 参数
        /// The plugin id, server id, and key form the complete lookup identity.
        /// 插件标识、服务器标识与键共同构成完整查找身份。
        ///
        /// # Returns
        /// 返回值
        /// Returns the stored value or a diagnostic that contains identifiers only.
        /// 返回已存值；不存在时返回仅包含标识符的诊断。
        fn read_secret(
            &self,
            plugin_id: &str,
            server_id: &str,
            key: &str,
        ) -> Result<String, String> {
            self.values
                .get(&(
                    plugin_id.to_string(),
                    server_id.to_string(),
                    key.to_string(),
                ))
                .cloned()
                .ok_or_else(|| format!("missing secret `{plugin_id}:{server_id}:{key}`"))
        }
    }

    /// Verifies literal, configuration, and Secret Store sources resolve through only their declared variant.
    /// 验证字面值、配置与 Secret Store 来源仅通过各自声明的变体解析。
    #[test]
    fn plugin_value_sources_resolve_without_cross_source_fallbacks() {
        let secrets = TestSecretStore {
            values: BTreeMap::from([(
                (
                    String::from("plugin-a"),
                    String::from("server-a"),
                    String::from("token"),
                ),
                String::from("secret-value"),
            )]),
        };
        let config = json!({"endpoint": "https://example.test/mcp", "number": 7});
        assert_eq!(
            resolve_plugin_value(
                &VulcanPluginValueSourceManifest::Literal {
                    value: String::from("literal-value"),
                },
                "plugin-a",
                "server-a",
                Some(&config),
                &secrets,
            )
            .expect("literal source"),
            "literal-value"
        );
        assert_eq!(
            resolve_plugin_value(
                &VulcanPluginValueSourceManifest::Config {
                    key: String::from("endpoint"),
                },
                "plugin-a",
                "server-a",
                Some(&config),
                &secrets,
            )
            .expect("config source"),
            "https://example.test/mcp"
        );
        assert_eq!(
            resolve_plugin_value(
                &VulcanPluginValueSourceManifest::Secret {
                    key: String::from("token"),
                },
                "plugin-a",
                "server-a",
                Some(&config),
                &secrets,
            )
            .expect("secret source"),
            "secret-value"
        );
        let non_string_error = resolve_plugin_value(
            &VulcanPluginValueSourceManifest::Config {
                key: String::from("number"),
            },
            "plugin-a",
            "server-a",
            Some(&config),
            &secrets,
        )
        .expect_err("non-string configuration must not fall back");
        assert!(non_string_error.contains("requires string configuration key `number`"));
    }

    /// Verifies remote endpoints accept only credential-free HTTPS and never echo rejected credential values.
    /// 验证远端端点仅接受无凭据 HTTPS，且拒绝时绝不回显凭据值。
    #[test]
    fn remote_url_validation_is_https_only_and_credential_safe() {
        let secrets = TestSecretStore {
            values: BTreeMap::new(),
        };
        let valid = VulcanPluginValueSourceManifest::Literal {
            value: String::from("https://example.test/mcp"),
        };
        assert_eq!(
            resolve_remote_url(&valid, "plugin-a", "server-a", None, &secrets)
                .expect("credential-free HTTPS"),
            "https://example.test/mcp"
        );
        for rejected in [
            "http://example.test/mcp",
            "https://user:embedded-password@example.test/mcp",
        ] {
            let error = resolve_remote_url(
                &VulcanPluginValueSourceManifest::Literal {
                    value: rejected.to_string(),
                },
                "plugin-a",
                "server-a",
                None,
                &secrets,
            )
            .expect_err("insecure endpoint must fail");
            assert!(error.contains("credential-free, fragment-free HTTPS"));
            assert!(!error.contains("embedded-password"));
        }
    }
}
