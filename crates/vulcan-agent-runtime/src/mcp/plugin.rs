//! Plugin-owned MCP server materialization into source-neutral runtime definitions.
//! 将插件自有 MCP 服务器物化为中立运行时定义。

use crate::mcp::config::{
    McpServerConfig, McpServerOAuthConfig, McpServerSource, McpServerTransportConfig,
};
use reqwest::Url;
use serde_json::Value;
use std::collections::BTreeMap;
use vulcan_plugin_runtime::{
    VulcanPluginCredentialBinding, VulcanPluginMcpOAuthManifest, VulcanPluginMcpScopeManifest,
    VulcanPluginMcpServerDescriptor, VulcanPluginMcpTransportManifest, VulcanPluginRuntimeManager,
    VulcanPluginValueSourceManifest,
};

/// Host boundary for reading one plugin-scoped secret without exposing storage details to plugins.
/// 用于读取单个插件作用域 Secret 且不向插件暴露存储细节的宿主边界。
pub trait PluginMcpSecretStore: Send + Sync {
    /// Reads one exact secret scoped to a plugin MCP server.
    /// 读取限定到某个插件 MCP 服务器的精确 Secret。
    ///
    /// # Parameters
    /// 参数
    /// Plugin, server, secret identifiers and the frozen `binding` form the complete host-owned namespace.
    /// 插件、服务器、Secret 标识及固定 `binding` 共同组成完整的宿主持有命名空间。
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
        binding: &VulcanPluginCredentialBinding,
    ) -> Result<String, String>;
}

/// Bind all values of one server materialization to the credential authority captured in its descriptor.
/// 将单个服务器物化的全部值绑定至描述符捕获的凭据权威。
struct BoundPluginMcpSecrets<'a> {
    /// The caller supplies the exact credential backend without an alternate lookup source.
    /// 调用方提供精确凭据后端，不提供替代查找来源。
    store: &'a dyn PluginMcpSecretStore,
    /// This immutable binding survives changes to the manager's latest catalog.
    /// 此不可变绑定在管理器最新目录变化后仍保持不变。
    binding: &'a VulcanPluginCredentialBinding,
}

impl BoundPluginMcpSecrets<'_> {
    /// Read `secret_key` for the exact `plugin_id` and `server_id` using only this captured binding.
    /// 仅使用此捕获绑定，读取精确 `plugin_id` 及 `server_id` 的 `secret_key`。
    /// Returns the backend value or its original non-sensitive error, with no revision fallback.
    /// 返回后端值或其原始非敏感错误，不回退修订。
    fn read_secret(
        &self,
        plugin_id: &str,
        server_id: &str,
        secret_key: &str,
    ) -> Result<String, String> {
        self.store
            .read_secret(plugin_id, server_id, secret_key, self.binding)
    }
}

/// Materializes every enabled global plugin MCP declaration into source-neutral runtime configuration.
/// 将全部已启用的全局插件 MCP 声明物化为中立运行时配置。
///
/// # Parameters
/// 参数
/// `plugins` owns the immutable catalog and managed launchers; `secrets` owns credential retrieval.
/// `plugins` 持有不可变目录与受管启动器；`secrets` 持有凭据读取。
///
/// # Returns
/// 返回值
/// Returns deterministic global definitions; conversation servers await an owner grant.
/// 返回确定性的全局定义；会话服务器等待所有者授权。
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

/// Materializes the global subset of a validated descriptor set for lifecycle readiness checks.
/// 为生命周期就绪检查物化已校验描述符集合中的全局子集。
///
/// # Parameters
/// 参数
/// `plugins` owns installed package and launcher state, `servers` belongs to one candidate plugin generation, and `secrets` resolves host credentials.
/// `plugins` 持有已安装包与启动器状态，`servers` 属于单个候选插件代次，`secrets` 解析宿主凭据。
///
/// # Returns
/// 返回值
/// Returns the complete global candidate subset or fails before any runtime partition is published.
/// 返回完整的全局候选子集；失败时不会发布任何运行时分区。
pub fn plugin_mcp_server_configs_for_descriptors(
    plugins: &VulcanPluginRuntimeManager,
    servers: &[VulcanPluginMcpServerDescriptor],
    secrets: &dyn PluginMcpSecretStore,
) -> Result<Vec<McpServerConfig>, String> {
    servers
        .iter()
        .filter(|server| server.manifest.scope == VulcanPluginMcpScopeManifest::Global)
        .map(|server| plugin_mcp_server_config(plugins, server, secrets, None))
        .collect()
}

/// Materializes one conversation-scoped stdio server with its exact Core-issued browser endpoint.
/// 使用 Core 颁发的精确浏览器端点物化一个会话作用域的 stdio 服务器。
///
/// # Parameters
/// 参数
/// `plugins` resolves the enabled descriptor and launcher; `plugin_id` and `server_id` select it;
/// `secrets` resolves declared keys; `owner_session_id` is the trusted conversation;
/// `core_browser_endpoint` is the one-use grant and must never be logged or persisted.
/// `plugins` 解析已启用描述符与启动器；`plugin_id` 和 `server_id` 选择它；`secrets` 解析声明的键；
/// `owner_session_id` 是可信会话；`core_browser_endpoint` 是绝不可记录或持久化的一次性授权。
///
/// # Returns
/// 返回值
/// Returns one runtime config for an owner-isolated MCP instance, not for the global partition.
/// 返回用于所有者隔离 MCP 实例的一份运行时配置，不能用于全局分区。
pub fn plugin_conversation_mcp_server_config(
    plugins: &VulcanPluginRuntimeManager,
    plugin_id: &str,
    server_id: &str,
    secrets: &dyn PluginMcpSecretStore,
    owner_session_id: &str,
    core_browser_endpoint: &str,
) -> Result<McpServerConfig, String> {
    if owner_session_id.trim().is_empty() {
        return Err(String::from(
            "Conversation owner is required for a scoped plugin MCP server.",
        ));
    }
    let snapshot = plugins.snapshot();
    let server = snapshot
        .enabled_mcp_servers
        .get(&format!("{plugin_id}:{server_id}"))
        .ok_or_else(|| format!("Plugin MCP server `{plugin_id}:{server_id}` is not enabled."))?;
    if server.manifest.scope != VulcanPluginMcpScopeManifest::Conversation {
        return Err(format!(
            "Plugin MCP server `{}:{}` is not conversation-scoped.",
            server.plugin_id, server.id
        ));
    }
    validate_core_browser_endpoint(core_browser_endpoint)?;
    plugin_mcp_server_config(plugins, server, secrets, Some(core_browser_endpoint))
}

/// Accepts only the exact loopback browser-level WebSocket capability returned by Core.
/// 仅接受 Core 返回的精确回环浏览器级 WebSocket 授权地址。
///
/// # Parameters
/// 参数
/// `endpoint` is the grant to validate without echoing its bearer capability on failure.
/// `endpoint` 是待校验授权；失败时不得回显承载能力。
///
/// # Returns
/// 返回值
/// Returns success for a local one-use browser endpoint, otherwise a redacted diagnostic.
/// 本地一次性浏览器端点返回成功，否则返回脱敏诊断。
fn validate_core_browser_endpoint(endpoint: &str) -> Result<(), String> {
    let valid = Url::parse(endpoint).is_ok_and(|url| {
        url.scheme() == "ws"
            && url.host_str() == Some("127.0.0.1")
            && url.port().is_some_and(|port| port > 0)
            && url.username().is_empty()
            && url.password().is_none()
            && url.query().is_none()
            && url.fragment().is_none()
            && url
                .path()
                .strip_prefix("/devtools/browser/")
                .is_some_and(|capability| {
                    !capability.is_empty()
                        && capability.bytes().all(|byte| {
                            byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_'
                        })
                })
    });
    if valid {
        Ok(())
    } else {
        Err(String::from(
            "Core browser endpoint must be a loopback browser-level grant.",
        ))
    }
}

/// Materializes one validated plugin MCP descriptor without admitting alternate field sources.
/// 物化单个已校验插件 MCP 描述符，且不接纳替代字段来源。
fn plugin_mcp_server_config(
    plugins: &VulcanPluginRuntimeManager,
    server: &VulcanPluginMcpServerDescriptor,
    secrets: &dyn PluginMcpSecretStore,
    core_browser_endpoint: Option<&str>,
) -> Result<McpServerConfig, String> {
    // Every transport and OAuth field uses one descriptor-owned revision, never a fresh global selection.
    // 每种传输及 OAuth 字段使用单个描述符自有修订，绝不重新全局选择。
    let secrets = &BoundPluginMcpSecrets {
        store: secrets,
        binding: &server.credential_binding,
    };
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
                        resolve_plugin_stdio_value(
                            value,
                            &server.plugin_id,
                            &server.id,
                            config.as_ref(),
                            secrets,
                            core_browser_endpoint,
                        )
                    })
                    .collect::<Result<Vec<_>, _>>()?,
            );
            let mut resolved_env = launcher.env;
            for (name, value) in env {
                resolved_env.insert(
                    name.clone(),
                    resolve_plugin_stdio_value(
                        value,
                        &server.plugin_id,
                        &server.id,
                        config.as_ref(),
                        secrets,
                        core_browser_endpoint,
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
                // Managed Node aborts on Windows without SystemRoot in an isolated child.
                // Windows 下受管 Node 在隔离子进程缺少 SystemRoot 时会中止。
                // Pass through that one OS runtime prerequisite without inheriting other values.
                // 仅透传这一项系统运行前提，不继承其他宿主环境变量。
                env_passthrough: if cfg!(windows) {
                    vec![String::from("SystemRoot")]
                } else {
                    Vec::new()
                },
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

/// Resolves a stdio value, admitting a Core browser grant only at the scoped launch boundary.
/// 解析 stdio 值，仅在作用域启动边界接纳 Core 浏览器授权。
///
/// # Parameters
/// 参数
/// `source` is the declared value, identifiers select the plugin namespace, `config` and
/// `secrets` resolve existing sources, and `core_browser_endpoint` is the trusted owner grant.
/// `source` 是声明值，标识符选择插件命名空间，`config` 与 `secrets` 解析既有来源，
/// `core_browser_endpoint` 是可信所有者授权。
///
/// # Returns
/// 返回值
/// Returns the exact resolved value or a diagnostic without the grant contents.
/// 返回精确解析值，或不含授权内容的诊断。
fn resolve_plugin_stdio_value(
    source: &VulcanPluginValueSourceManifest,
    plugin_id: &str,
    server_id: &str,
    config: Option<&Value>,
    secrets: &BoundPluginMcpSecrets<'_>,
    core_browser_endpoint: Option<&str>,
) -> Result<String, String> {
    match source {
        VulcanPluginValueSourceManifest::CoreBrowserEndpoint => core_browser_endpoint
            .map(str::to_owned)
            .ok_or_else(|| String::from("Core browser endpoint requires a conversation grant.")),
        _ => resolve_plugin_value(source, plugin_id, server_id, config, secrets),
    }
}

/// Resolves one typed plugin value from its single declared source.
/// 从单一声明来源解析一个带类型插件值。
fn resolve_plugin_value(
    source: &VulcanPluginValueSourceManifest,
    plugin_id: &str,
    server_id: &str,
    config: Option<&Value>,
    secrets: &BoundPluginMcpSecrets<'_>,
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
        VulcanPluginValueSourceManifest::CoreBrowserEndpoint => Err(String::from(
            "Core browser endpoint requires a conversation-scoped stdio launch.",
        )),
    }
}

/// Resolves and validates one credential-free HTTPS endpoint.
/// 解析并校验一个不含嵌入凭据的 HTTPS 端点。
fn resolve_remote_url(
    source: &VulcanPluginValueSourceManifest,
    plugin_id: &str,
    server_id: &str,
    config: Option<&Value>,
    secrets: &BoundPluginMcpSecrets<'_>,
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
    secrets: &BoundPluginMcpSecrets<'_>,
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
    secrets: &BoundPluginMcpSecrets<'_>,
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
    use std::fs;

    mod credential_binding;

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
            binding: &VulcanPluginCredentialBinding,
        ) -> Result<String, String> {
            if *binding != VulcanPluginCredentialBinding::Legacy {
                return Err(String::from(
                    "This fixture declares only legacy credentials.",
                ));
            }
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
        // These existing source tests explicitly select the legacy fixture namespace.
        // 这些既有来源测试明确选择旧夹具命名空间。
        let secrets = BoundPluginMcpSecrets {
            store: &secrets,
            binding: &VulcanPluginCredentialBinding::Legacy,
        };
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
        // Literal endpoint validation has no credential revision to infer.
        // 字面端点校验没有可推断的凭据修订。
        let secrets = BoundPluginMcpSecrets {
            store: &secrets,
            binding: &VulcanPluginCredentialBinding::Legacy,
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

    /// Verifies a live plugin declaration enters only its owner runtime and receives the exact grant.
    /// 验证有效插件声明只进入所有者运行时，并收到精确授权。
    #[test]
    fn conversation_mcp_config_is_owner_granted_and_excluded_from_global_startup() {
        let temp = tempfile::tempdir().expect("create scoped MCP fixture");
        let bundled = temp.path().join("bundled");
        let package = bundled.join("scoped-browser");
        fs::create_dir_all(package.join("bin")).expect("create test package binary directory");
        fs::write(package.join("bin/server.bin"), b"test-only binary identity")
            .expect("write test binary identity");
        fs::write(
            package.join("plugin.yaml"),
            r#"schema_version: 1
id: scoped-browser
version: 1.0.0
default_enabled: true
display:
  name: Scoped Browser
  capability: Owner-scoped MCP fixture
permissions:
  - process_binary
  - process_session
execution:
  native_binaries:
    - id: server
      targets:
        x86_64-pc-windows-msvc: bin/server.bin
        aarch64-pc-windows-msvc: bin/server.bin
        x86_64-unknown-linux-gnu: bin/server.bin
        aarch64-unknown-linux-gnu: bin/server.bin
        x86_64-apple-darwin: bin/server.bin
        aarch64-apple-darwin: bin/server.bin
mcp:
  servers:
    - id: playwright
      name: Playwright
      scope: conversation
      transport:
        type: stdio
        launcher:
          type: native_binary
          binary_id: server
        args:
          - source: literal
            value: --endpoint
          - source: core_browser_endpoint
"#,
        )
        .expect("write scoped MCP manifest");
        let plugins = VulcanPluginRuntimeManager::without_runtime(
            bundled,
            temp.path().join("user"),
            temp.path().join("bridge"),
            temp.path().join("managed"),
            temp.path().join("state"),
            temp.path().join("runtime"),
        );
        let snapshot = plugins
            .refresh_catalog(&Default::default(), &Default::default())
            .expect("publish scoped plugin catalog");
        assert!(
            snapshot
                .enabled_mcp_servers
                .contains_key("scoped-browser:playwright")
        );
        let secrets = TestSecretStore {
            values: BTreeMap::new(),
        };
        assert!(
            plugin_mcp_server_configs(&plugins, &secrets)
                .expect("materialize global servers")
                .is_empty()
        );
        let endpoint = "ws://127.0.0.1:35123/devtools/browser/OwnerGrant_123";
        let config = plugin_conversation_mcp_server_config(
            &plugins,
            "scoped-browser",
            "playwright",
            &secrets,
            "conversation-a",
            endpoint,
        )
        .expect("materialize owner server");
        assert_eq!(config.id, "scoped-browser:playwright");
        let McpServerTransportConfig::Stdio {
            args,
            env_passthrough,
            inherit_environment,
            ..
        } = config.transport
        else {
            panic!("conversation server must remain stdio");
        };
        assert_eq!(args, ["--endpoint", endpoint]);
        assert!(!inherit_environment);
        assert_eq!(
            env_passthrough,
            if cfg!(windows) {
                vec![String::from("SystemRoot")]
            } else {
                Vec::new()
            }
        );
        let missing_owner = plugin_conversation_mcp_server_config(
            &plugins,
            "scoped-browser",
            "playwright",
            &secrets,
            "",
            endpoint,
        )
        .expect_err("a scoped server requires a trusted owner");
        assert!(missing_owner.contains("Conversation owner is required"));
        let invalid_endpoint = "ws://192.168.1.8:35123/devtools/browser/secret-value";
        let rejected = plugin_conversation_mcp_server_config(
            &plugins,
            "scoped-browser",
            "playwright",
            &secrets,
            "conversation-a",
            invalid_endpoint,
        )
        .expect_err("remote endpoint must be rejected");
        assert!(!rejected.contains("secret-value"));
        plugins
            .refresh_catalog(
                &[String::from("scoped-browser")].into(),
                &Default::default(),
            )
            .expect("disable scoped plugin");
        assert!(
            plugin_conversation_mcp_server_config(
                &plugins,
                "scoped-browser",
                "playwright",
                &secrets,
                "conversation-a",
                endpoint,
            )
            .is_err()
        );
    }
}
