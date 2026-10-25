//! Configuration tools: `mcp_config` manages the user's configured MCP servers and
//! `provider_config` adds and switches model provider connections. Listing reads the
//! configuration without secrets; every change is shown to the user and needs their approval
//! outside full access. Secrets never pass through the arguments: they are read from Vulcan
//! environment variables by name when the change is saved.
//! 配置工具：`mcp_config` 管理用户已配置的 MCP 服务器，`provider_config` 添加与切换模型供应商连接。
//! 列出时读取不含密钥的配置；每次修改都展示给用户，完全访问以外需要用户批准。密钥绝不经过参数：
//! 保存变更时按名称从 Vulcan 环境变量读取。

use super::*;
use vulcan_provider_adapters::{CustomProviderAuthentication, CustomProviderProtocol};

/// Stable identifier of the tool that manages configured MCP servers.
/// 管理已配置 MCP 服务器的工具的稳定标识。
pub const MCP_CONFIG_TOOL_ID: &str = "mcp_config";

/// Stable identifier of the tool that manages model provider connections.
/// 管理模型供应商连接的工具的稳定标识。
pub const PROVIDER_CONFIG_TOOL_ID: &str = "provider_config";

/// The only action of the configuration tools that reads instead of changing.
/// 配置工具中唯一读取而非修改的动作。
pub const CONFIGURATION_LIST_ACTION: &str = "list";

/// One parsed configuration tool call. The permission preview and the change itself parse the
/// same arguments with this type, so both always describe the same change.
/// 一次已解析的配置工具调用。权限预览与变更本身用此类型解析同一参数，因此二者始终描述同一变更。
#[derive(Debug, Clone, PartialEq)]
pub enum ConfigurationRequest {
    /// A `mcp_config` call.
    /// 一次 `mcp_config` 调用。
    Mcp(McpConfigRequest),
    /// A `provider_config` call.
    /// 一次 `provider_config` 调用。
    Provider(ProviderConfigRequest),
}

impl ConfigurationRequest {
    /// Parses the arguments of the configuration tool `tool_name`.
    /// 解析配置工具 `tool_name` 的参数。
    pub fn parse(tool_name: &str, args: &Value) -> Result<Self, String> {
        let invalid =
            |error: serde_json::Error| format!("Invalid `{tool_name}` arguments: {error}");
        match tool_name {
            MCP_CONFIG_TOOL_ID => serde_json::from_value(args.clone())
                .map(Self::Mcp)
                .map_err(invalid),
            PROVIDER_CONFIG_TOOL_ID => serde_json::from_value(args.clone())
                .map(Self::Provider)
                .map_err(invalid),
            other => Err(format!("`{other}` is not a configuration tool.")),
        }
    }

    /// Whether the call only reads the configuration.
    /// 调用是否只读取配置。
    pub fn is_list(&self) -> bool {
        matches!(
            self,
            Self::Mcp(McpConfigRequest::List) | Self::Provider(ProviderConfigRequest::List)
        )
    }
}

/// Arguments of `mcp_config`.
/// `mcp_config` 的参数。
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum McpConfigRequest {
    /// Lists the configured servers.
    /// 列出已配置的服务器。
    List,
    /// Creates a server, or updates the one named by `server_id`; omitted fields keep their value.
    /// 创建服务器，或更新 `server_id` 指定的服务器；省略的字段保持原值。
    Save {
        /// Server to update; absent creates one.
        /// 要更新的服务器；缺省时创建。
        server_id: Option<String>,
        /// Display name; required when creating.
        /// 显示名称；创建时必填。
        name: Option<String>,
        /// Whether every session loads the server; a new server stays disabled when absent.
        /// 是否所有会话都加载该服务器；新服务器缺省时保持停用。
        enabled: Option<bool>,
        /// `stdio`, `http` or `sse`; required when creating.
        /// `stdio`、`http` 或 `sse`；创建时必填。
        transport: Option<String>,
        /// stdio program.
        /// stdio 程序。
        command: Option<String>,
        /// stdio arguments.
        /// stdio 参数。
        args: Option<Vec<String>>,
        /// stdio working directory; empty clears it.
        /// stdio 工作目录；为空时清除。
        cwd: Option<String>,
        /// Names of environment variables passed to the stdio server.
        /// 传给 stdio 服务器的环境变量名。
        env_passthrough: Option<Vec<String>>,
        /// Remote endpoint URL.
        /// 远端端点 URL。
        url: Option<String>,
        /// Remote request headers, each mapped to the environment variable holding its value.
        /// 远端请求头，每个映射到保存其值的环境变量。
        headers_from_env: Option<BTreeMap<String, String>>,
    },
    /// Removes the server `server_id`.
    /// 移除服务器 `server_id`。
    Remove {
        /// Server to remove.
        /// 要移除的服务器。
        server_id: String,
    },
}

/// Arguments of `provider_config`.
/// `provider_config` 的参数。
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum ProviderConfigRequest {
    /// Lists providers, their API-key entries and the configured connections.
    /// 列出供应商、其 API Key 入口与已配置连接。
    List,
    /// Adds an API-key connection to a built-in provider.
    /// 为内置供应商添加 API Key 连接。
    AddConnection {
        /// Provider to connect.
        /// 要连接的供应商。
        provider_id: String,
        /// Service entry; may be omitted when the provider has one API-key entry.
        /// 服务入口；供应商只有一个 API Key 入口时可省略。
        service_entry_id: Option<String>,
        /// Credential profile; may be omitted when the entry has one API-key profile.
        /// 凭据配置档；入口只有一个 API Key 配置档时可省略。
        credential_profile_id: Option<String>,
        /// Connection name; defaults to the provider name.
        /// 连接名称；默认使用供应商名称。
        display_name: Option<String>,
        /// Environment variable holding the API key.
        /// 保存 API Key 的环境变量。
        api_key_env: String,
    },
    /// Adds an OpenAI-, Anthropic- or Gemini-compatible provider with its API key.
    /// 添加兼容 OpenAI、Anthropic 或 Gemini 的供应商及其 API Key。
    AddCustomProvider {
        /// Provider name.
        /// 供应商名称。
        display_name: String,
        /// API base URL.
        /// API 基础 URL。
        base_url: String,
        /// Protocols the endpoint speaks.
        /// 端点支持的协议。
        protocols: Vec<CustomProviderProtocol>,
        /// How the API key is sent; defaults to a bearer `Authorization` header.
        /// API Key 的发送方式；默认使用 Bearer `Authorization` 请求头。
        authentication: Option<CustomProviderAuthentication>,
        /// Environment variable holding the API key.
        /// 保存 API Key 的环境变量。
        api_key_env: String,
    },
    /// Enables or disables one connection.
    /// 启用或停用一个连接。
    SetConnectionEnabled {
        /// Connection to change.
        /// 要修改的连接。
        connection_id: String,
        /// New state.
        /// 新状态。
        enabled: bool,
    },
    /// Makes one connection the default of its provider.
    /// 将一个连接设为其供应商的默认连接。
    SetDefaultConnection {
        /// Connection to make the default.
        /// 要设为默认的连接。
        connection_id: String,
    },
}

/// Text and host payload produced by one configuration call.
/// 一次配置调用产生的文本与宿主载荷。
#[derive(Debug, Clone, PartialEq)]
pub struct ConfigurationOutcome {
    /// Result read by the model.
    /// 模型读取的结果。
    pub text: String,
    /// Structured result for clients.
    /// 供客户端使用的结构化结果。
    pub payload: Value,
}

/// Host-injected configuration port. The host owns the configuration and the settings services,
/// so listing and changing go through the same code as the settings pages.
/// 宿主注入的配置端口。宿主拥有配置与设置服务，因此列出与修改都走与设置页相同的代码。
#[async_trait::async_trait]
pub trait NativeConfigurationHost: fmt::Debug + Send + Sync {
    /// Lists or changes configuration for `session_id`. A change runs only after the permission
    /// gate admitted it; the host still refuses changes from spawned agents.
    /// 为 `session_id` 列出或修改配置。修改只在权限闸门准入后运行；宿主仍拒绝派生代理的修改。
    async fn run(
        &self,
        session_id: SessionId,
        request: ConfigurationRequest,
    ) -> Result<ConfigurationOutcome, String>;
}

/// Builds the model-visible `mcp_config` tool.
/// 构建模型可见的 `mcp_config` 工具。
pub(crate) fn build_mcp_config_tool() -> Arc<NativeToolDefinition> {
    build_configuration_tool(
        MCP_CONFIG_TOOL_ID,
        String::from(
            "Manage the user's configured MCP servers, which load in every session of every project. Use it when the user asks to add, change, enable or remove a server; for a one-off use, open a temporary connection with `mcp_connect` instead.\n\
- `list`: the configured servers and their state, without secrets.\n\
- `save`: create a server (`name`, `transport`, and `command`/`args`/`cwd`/`env_passthrough` for stdio or `url`/`headers_from_env` for http/sse), or update the one named by `server_id`; omitted fields keep their value. A new server stays disabled unless `enabled` is true.\n\
- `remove`: delete the server `server_id`.\n\
The user sees every change and approves it unless the session has full access. Secrets never go in the arguments: name environment variables with `env_passthrough` or `headers_from_env`, and ask the user for missing ones with `env` action `request` (scope `global`, since configured servers only see global variables). Tokens, literal headers and OAuth settings are kept as the user set them and can only be changed in the settings page. Tools of a newly enabled server appear from the next turn.",
        ),
        mcp_config_args_schema(),
    )
}

/// Builds the model-visible `provider_config` tool.
/// 构建模型可见的 `provider_config` 工具。
pub(crate) fn build_provider_config_tool() -> Arc<NativeToolDefinition> {
    build_configuration_tool(
        PROVIDER_CONFIG_TOOL_ID,
        String::from(
            "Manage the user's model provider connections. Use it when the user asks to add a provider or switch connections.\n\
- `list`: providers with their API-key service entries and credential profiles, and the configured connections, without secrets.\n\
- `add_connection`: connect a built-in provider with an API key (`provider_id`, optional `service_entry_id`/`credential_profile_id` when the provider has several, `display_name`).\n\
- `add_custom_provider`: add a compatible endpoint (`display_name`, `base_url`, `protocols`, optional `authentication`).\n\
- `set_connection_enabled`, `set_default_connection`: switch connections by `connection_id`.\n\
The API key is read from the Vulcan environment variable named by `api_key_env` when the change is saved and stored with the connection; never put a key in the arguments. If the user has not provided it, ask with `env` action `request` first. Saving checks the key against the provider. The user sees every change and approves it unless the session has full access. Accounts that sign in through a browser are added in the settings page.",
        ),
        provider_config_args_schema(),
    )
}

/// Builds one configuration tool; only `list` runs in Plan mode, through its action policy.
/// 构建一个配置工具；计划模式中只有 `list` 可以经由其动作策略运行。
fn build_configuration_tool(
    id: &'static str,
    description: String,
    args_schema: Value,
) -> Arc<NativeToolDefinition> {
    build_native_tool_definition(
        NativeToolDescriptor {
            id: id.to_string(),
            name: id.to_string(),
            description,
            help_text: None,
            ai_visible: true,
            available_in_plan: true,
        },
        args_schema,
        move |request| Box::pin(async move { execute_configuration_tool(id, request).await }),
    )
}

/// JSON Schema of the `mcp_config` arguments.
/// `mcp_config` 参数的 JSON Schema。
fn mcp_config_args_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "action": {
                "type": "string",
                "enum": [CONFIGURATION_LIST_ACTION, "save", "remove"],
                "description": "Operation to perform."
            },
            "server_id": { "type": "string", "description": "save: server to update (omit to create); remove: server to delete." },
            "name": { "type": "string", "description": "save: display name; required when creating." },
            "enabled": { "type": "boolean", "description": "save: whether every session loads the server." },
            "transport": {
                "type": "string",
                "enum": ["stdio", "http", "sse"],
                "description": "save: how to reach the server; required when creating."
            },
            "command": { "type": "string", "description": "save stdio: program to run." },
            "args": { "type": "array", "items": { "type": "string" }, "description": "save stdio: program arguments." },
            "cwd": { "type": "string", "description": "save stdio: working directory; an empty string clears it." },
            "env_passthrough": {
                "type": "array",
                "items": { "type": "string" },
                "description": "save stdio: names of environment variables passed to the server."
            },
            "url": { "type": "string", "description": "save http/sse: endpoint URL." },
            "headers_from_env": {
                "type": "object",
                "additionalProperties": { "type": "string" },
                "description": "save http/sse: request headers, each mapped to the name of the environment variable holding its value."
            }
        },
        "required": ["action"],
        "additionalProperties": false
    })
}

/// JSON Schema of the `provider_config` arguments.
/// `provider_config` 参数的 JSON Schema。
fn provider_config_args_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "action": {
                "type": "string",
                "enum": [
                    CONFIGURATION_LIST_ACTION,
                    "add_connection",
                    "add_custom_provider",
                    "set_connection_enabled",
                    "set_default_connection"
                ],
                "description": "Operation to perform."
            },
            "provider_id": { "type": "string", "description": "add_connection: built-in provider to connect." },
            "service_entry_id": { "type": "string", "description": "add_connection: service entry, when the provider has several." },
            "credential_profile_id": { "type": "string", "description": "add_connection: credential profile, when the entry has several." },
            "display_name": { "type": "string", "description": "add_connection: connection name; add_custom_provider: provider name." },
            "api_key_env": { "type": "string", "description": "add_connection/add_custom_provider: name of the Vulcan environment variable holding the API key." },
            "base_url": { "type": "string", "description": "add_custom_provider: API base URL." },
            "protocols": {
                "type": "array",
                "items": {
                    "type": "string",
                    "enum": [
                        "anthropic_messages",
                        "open_ai_chat",
                        "open_ai_responses",
                        "open_ai_codex_responses",
                        "gemini_generate_content"
                    ]
                },
                "description": "add_custom_provider: protocols the endpoint speaks."
            },
            "authentication": {
                "type": "string",
                "enum": ["bearer_authorization", "x_api_key", "google_api_key"],
                "description": "add_custom_provider: how the key is sent. Defaults to bearer_authorization."
            },
            "connection_id": { "type": "string", "description": "set_connection_enabled/set_default_connection: the connection." },
            "enabled": { "type": "boolean", "description": "set_connection_enabled: new state." }
        },
        "required": ["action"],
        "additionalProperties": false
    })
}

/// Executes one configuration tool call through the host.
/// 经由宿主执行一次配置工具调用。
async fn execute_configuration_tool(
    tool_id: &str,
    request: NativeToolExecutionRequest,
) -> Result<VulcanToolInvocationResult, NativeToolError> {
    let parsed =
        ConfigurationRequest::parse(tool_id, &request.arguments).map_err(NativeToolError::new)?;
    let host = request.configuration.clone().ok_or_else(|| {
        NativeToolError::new("Configuration changes are not available in this run.")
    })?;
    let session_id = request
        .session_id
        .ok_or_else(|| NativeToolError::new(format!("`{tool_id}` needs a session.")))?;
    let outcome = host
        .run(session_id, parsed)
        .await
        .map_err(NativeToolError::new)?;
    Ok(
        VulcanToolInvocationResult::from_model_output(Value::String(outcome.text))
            .with_host_result(VulcanToolHostResult {
                kind: tool_id.to_string(),
                payload: outcome.payload,
            }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The preview and the change parse one call identically, and unknown fields are refused.
    /// 预览与变更对同一调用的解析完全一致，未知字段被拒绝。
    #[test]
    fn requests_parse_strictly_and_detect_listing() {
        let list = ConfigurationRequest::parse(MCP_CONFIG_TOOL_ID, &json!({"action": "list"}))
            .expect("list parses");
        assert!(list.is_list());
        let save = ConfigurationRequest::parse(
            MCP_CONFIG_TOOL_ID,
            &json!({"action": "save", "name": "docs", "transport": "stdio", "command": "npx"}),
        )
        .expect("save parses");
        assert!(!save.is_list());
        assert!(
            ConfigurationRequest::parse(
                MCP_CONFIG_TOOL_ID,
                &json!({"action": "save", "bearer_token": "secret"})
            )
            .is_err(),
            "secret fields are not part of the tool"
        );
        let provider = ConfigurationRequest::parse(
            PROVIDER_CONFIG_TOOL_ID,
            &json!({
                "action": "add_custom_provider",
                "display_name": "Local",
                "base_url": "http://127.0.0.1:8080/v1",
                "protocols": ["open_ai_chat"],
                "api_key_env": "LOCAL_KEY"
            }),
        )
        .expect("custom provider parses");
        assert!(matches!(
            provider,
            ConfigurationRequest::Provider(ProviderConfigRequest::AddCustomProvider { .. })
        ));
        assert!(
            ConfigurationRequest::parse(
                PROVIDER_CONFIG_TOOL_ID,
                &json!({"action": "add_connection", "provider_id": "deepseek", "secret": "sk"})
            )
            .is_err()
        );
        assert!(ConfigurationRequest::parse("file", &json!({"action": "list"})).is_err());
    }
}
