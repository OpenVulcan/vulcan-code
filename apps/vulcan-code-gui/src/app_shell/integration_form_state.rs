//! Agent and MCP form state extracted from the main workbench module.
//! 从主工作台模块中抽取的 Agent 与 MCP 表单状态。

use super::{join_mcp_server_oauth_scopes, parse_mcp_server_oauth_scopes, trim_to_option};
use crate::agent_profile;
use crate::provider_config::{
    PersistedMcpOAuthConfig, PersistedMcpServerEntry, PersistedMcpTransportKind,
};
use iced::widget::text_editor::Content;
use std::collections::BTreeMap;

/// Editable user-agent form state mirrored into the dedicated user-agent markdown directory.
/// 映射回独立用户 Agent Markdown 目录的可编辑用户 Agent 表单状态。
#[derive(Debug, Clone)]
pub(super) struct AgentProfileFormState {
    /// Stable agent identifier stored on disk and used by the runtime.
    /// 保存在磁盘上并供运行时使用的稳定 Agent 标识。
    pub(super) id: String,
    /// Human-readable agent name displayed in pickers and management lists.
    /// 展示在选择器与管理列表中的人类可读 Agent 名称。
    pub(super) name: String,
    /// Optional concise description shown in the management list and runtime surfaces.
    /// 展示在管理列表与运行时表面中的可选简短描述。
    pub(super) description: String,
    /// Optional `provider/model` override written into the profile frontmatter.
    /// 写入配置前置元数据中的可选 `provider/model` 覆盖值。
    pub(super) model: String,
    /// Optional temperature override shown as editable text.
    /// 以可编辑文本形式展示的可选温度覆盖值。
    pub(super) temperature_text: String,
    /// Optional max-turn override shown as editable text.
    /// 以可编辑文本形式展示的可选最大轮次数覆盖值。
    pub(super) max_turns_text: String,
    /// Comma-separated prompt skill identifiers bound to the profile.
    /// 绑定到该配置的逗号分隔提示词技能标识。
    pub(super) skill_ids_text: String,
    /// Comma-separated native tool identifiers exposed by the profile.
    /// 该配置暴露的逗号分隔原生工具标识。
    pub(super) native_tools_text: String,
    /// Comma-separated MCP tool identifiers exposed by the profile.
    /// 该配置暴露的逗号分隔 MCP 工具标识。
    pub(super) mcp_tools_text: String,
    /// Optional shared tool-policy identifier.
    /// 可选的共享工具策略标识。
    pub(super) tool_policy: String,
    /// Editable YAML runtime-policy override buffer.
    /// 可编辑的 YAML 运行时策略覆盖缓冲区。
    pub(super) runtime_policy_buffer: Content,
    /// Editable primary prompt buffer saved as the Markdown body.
    /// 保存为 Markdown 正文的可编辑主提示词缓冲区。
    pub(super) prompt_buffer: Content,
}

impl AgentProfileFormState {
    /// Builds an editable user-agent form from one persisted user profile.
    /// 根据单条持久化用户 Agent 配置构建可编辑表单。
    pub(super) fn from_profile(profile: &agent_profile::UserAgentProfile) -> Self {
        Self {
            id: profile.id.clone(),
            name: profile.name.clone(),
            description: profile.description.clone(),
            model: profile.model.clone().unwrap_or_default(),
            temperature_text: profile
                .temperature
                .map(|value| format!("{value:.2}"))
                .unwrap_or_default(),
            max_turns_text: profile
                .max_turns
                .map(|value| value.to_string())
                .unwrap_or_default(),
            skill_ids_text: agent_profile::format_agent_profile_list_text(&profile.skill_ids),
            native_tools_text: agent_profile::format_agent_profile_list_text(&profile.native_tools),
            mcp_tools_text: agent_profile::format_agent_profile_list_text(&profile.mcp_tools),
            tool_policy: profile.tool_policy.clone().unwrap_or_default(),
            runtime_policy_buffer: Content::with_text(
                &agent_profile::format_runtime_policy_request_text(profile.runtime_policy),
            ),
            prompt_buffer: Content::with_text(&profile.prompt),
        }
    }
}

impl Default for AgentProfileFormState {
    /// Builds the default editable user-agent draft form.
    /// 构建默认的可编辑用户 Agent 草稿表单。
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            description: String::new(),
            model: String::new(),
            temperature_text: String::new(),
            max_turns_text: String::new(),
            skill_ids_text: String::new(),
            native_tools_text: String::new(),
            mcp_tools_text: String::new(),
            tool_policy: String::new(),
            runtime_policy_buffer: Content::with_text(""),
            prompt_buffer: Content::with_text(""),
        }
    }
}

/// Lightweight selection payload representing one configurable MCP server entry.
/// 表示单个可配置 MCP 服务器条目的轻量选择载荷。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct McpServerChoice {
    /// Stable MCP server identifier.
    /// 稳定 MCP 服务器标识。
    pub(super) id: String,
    /// Human-readable display label.
    /// 人类可读显示标签。
    pub(super) label: String,
}

impl std::fmt::Display for McpServerChoice {
    /// Renders the user-facing label of one MCP server picker item.
    /// 渲染单个 MCP 服务器选择项的面向用户标签。
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.label)
    }
}

/// Default working directory seeded into new stdio MCP drafts.
/// 新建 stdio MCP 草稿时预填的默认工作目录。
pub(super) const DEFAULT_MCP_WORKING_DIRECTORY: &str = "~/.vulcan/code/mcp";

/// One editable single-value row used by MCP argument and passthrough-environment lists.
/// MCP 参数与透传环境变量列表使用的单值可编辑行。
#[derive(Debug, Clone, Default)]
pub(super) struct McpTextRowState {
    /// Current row value buffer.
    /// 当前行的值缓冲区。
    pub(super) value: String,
}

impl McpTextRowState {
    /// Builds one single-value MCP row from an optional initial value.
    /// 根据可选初始值构建一条单值 MCP 行。
    fn new(value: impl Into<String>) -> Self {
        Self {
            value: value.into(),
        }
    }
}

/// One editable key-value row used by MCP environment and header groups.
/// MCP 环境变量与请求头分组使用的键值可编辑行。
#[derive(Debug, Clone, Default)]
pub(super) struct McpKeyValueRowState {
    /// Current row key buffer.
    /// 当前行的键缓冲区。
    pub(super) key: String,
    /// Current row value buffer.
    /// 当前行的值缓冲区。
    pub(super) value: String,
}

impl McpKeyValueRowState {
    /// Builds one key-value MCP row from optional initial key and value buffers.
    /// 根据可选初始键和值缓冲区构建一条 MCP 键值行。
    fn new(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            value: value.into(),
        }
    }
}

/// Editable MCP server form state mirrored into the shared configuration file.
/// 映射回共享配置文件的可编辑 MCP 服务器表单状态。
#[derive(Debug, Clone)]
pub(super) struct McpServerFormState {
    /// Human-readable MCP server name.
    /// 人类可读的 MCP 服务器名称。
    pub(super) name: String,
    /// Whether the MCP server is enabled.
    /// 当前 MCP 服务器是否启用。
    pub(super) enabled: bool,
    /// Selected transport family.
    /// 当前选中的传输类型。
    pub(super) transport: PersistedMcpTransportKind,
    /// Command used to start the MCP server process.
    /// 用于启动 MCP 服务器进程的命令。
    pub(super) command: String,
    /// Editable argument rows shown in the settings form.
    /// 展示在设置表单中的可编辑参数行集合。
    pub(super) args_rows: Vec<McpTextRowState>,
    /// Remote endpoint URL used by HTTP transports.
    /// HTTP 传输使用的远端端点 URL。
    pub(super) url: String,
    /// Optional bearer token used by HTTP transports.
    /// HTTP 传输使用的可选 Bearer Token。
    pub(super) bearer_token: String,
    /// Editable explicit HTTP header rows.
    /// 可编辑的显式 HTTP 请求头行集合。
    pub(super) header_rows: Vec<McpKeyValueRowState>,
    /// Editable request-header rows that resolve values from process environment variables.
    /// 从当前进程环境变量解析值的可编辑请求头行集合。
    pub(super) header_from_env_rows: Vec<McpKeyValueRowState>,
    /// Optional OAuth client identifier used by remote MCP transports.
    /// 远端 MCP 传输使用的可选 OAuth 客户端标识。
    pub(super) oauth_client_id: String,
    /// Optional OAuth client secret used by remote MCP transports.
    /// 远端 MCP 传输使用的可选 OAuth 客户端密钥。
    pub(super) oauth_client_secret: String,
    /// Delimited OAuth scope declarations shown in the settings form.
    /// 展示在设置表单中的分隔 OAuth scope 声明文本。
    pub(super) oauth_scopes_text: String,
    /// Optional OAuth redirect URI used by remote MCP transports.
    /// 远端 MCP 传输使用的可选 OAuth 回调 URI。
    pub(super) oauth_redirect_uri: String,
    /// Optional working directory override.
    /// 可选的工作目录覆盖值。
    pub(super) cwd: String,
    /// Editable explicit environment-variable rows injected into stdio child processes.
    /// 注入到 stdio 子进程中的可编辑显式环境变量行集合。
    pub(super) env_rows: Vec<McpKeyValueRowState>,
    /// Editable environment-variable names that should be passed through from the current host process.
    /// 需要从当前宿主进程透传的可编辑环境变量名称行集合。
    pub(super) env_passthrough_rows: Vec<McpTextRowState>,
}

impl McpServerFormState {
    /// Returns one default stdio working directory used by new MCP drafts and empty persisted entries.
    /// 返回新 MCP 草稿与空持久化条目使用的默认 stdio 工作目录。
    fn default_working_directory() -> String {
        String::from(DEFAULT_MCP_WORKING_DIRECTORY)
    }

    /// Converts one persisted string vector into editable MCP single-value rows while preserving at least one row.
    /// 将持久化字符串向量转换为可编辑 MCP 单值行，并保证至少保留一行。
    fn text_rows_from_values(values: &[String]) -> Vec<McpTextRowState> {
        if values.is_empty() {
            vec![McpTextRowState::default()]
        } else {
            values
                .iter()
                .map(|value| McpTextRowState::new(value.clone()))
                .collect()
        }
    }

    /// Converts one persisted key-value map into editable MCP key-value rows while preserving at least one row.
    /// 将持久化键值映射转换为可编辑 MCP 键值行，并保证至少保留一行。
    fn key_value_rows_from_map(values: &BTreeMap<String, String>) -> Vec<McpKeyValueRowState> {
        if values.is_empty() {
            vec![McpKeyValueRowState::default()]
        } else {
            values
                .iter()
                .map(|(key, value)| McpKeyValueRowState::new(key.clone(), value.clone()))
                .collect()
        }
    }

    /// Resolves one ordered argument vector from the current editable row set, trimming blank entries.
    /// 从当前可编辑行集合解析出有序参数向量，并裁剪空白项。
    fn resolved_argument_rows(&self) -> Vec<String> {
        self.args_rows
            .iter()
            .map(|row| row.value.trim())
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .collect()
    }

    /// Resolves one ordered passthrough-environment list from the current editable row set, trimming blanks and removing duplicates.
    /// 从当前可编辑行集合解析出有序透传环境变量列表，并裁剪空白与移除重复项。
    fn resolved_env_passthrough_rows(&self) -> Vec<String> {
        let mut ordered_values = Vec::new();
        for value in self
            .env_passthrough_rows
            .iter()
            .map(|row| row.value.trim())
            .filter(|value| !value.is_empty())
        {
            if !ordered_values.iter().any(|existing| existing == value) {
                ordered_values.push(value.to_string());
            }
        }
        ordered_values
    }

    /// Resolves one validated key-value map from editable rows while ignoring fully blank rows and rejecting partial or duplicate keys.
    /// 从可编辑行解析出一份经过校验的键值映射，并忽略完全空白行且拒绝半填写或重复键。
    fn resolved_key_value_rows(
        rows: &[McpKeyValueRowState],
        subject_label: &str,
        value_label: &str,
    ) -> Result<BTreeMap<String, String>, String> {
        let mut map = BTreeMap::new();
        for row in rows {
            let key = row.key.trim();
            let value = row.value.trim();
            if key.is_empty() && value.is_empty() {
                continue;
            }
            if key.is_empty() {
                return Err(format!("{subject_label} 的键不能为空。"));
            }
            if value.is_empty() {
                return Err(format!("{subject_label} 的{value_label}不能为空。"));
            }
            if map.insert(key.to_string(), value.to_string()).is_some() {
                return Err(format!("{subject_label} `{key}` 重复，请保留一条即可。"));
            }
        }
        Ok(map)
    }

    /// Builds an editable form state from one persisted MCP server entry.
    /// 根据单个持久化 MCP 服务器条目构建可编辑表单状态。
    pub(super) fn from_entry(entry: &PersistedMcpServerEntry) -> Self {
        Self {
            name: entry.name.clone(),
            enabled: entry.enabled,
            transport: entry.transport,
            command: entry.command.clone(),
            args_rows: Self::text_rows_from_values(&entry.args),
            url: entry.url.clone().unwrap_or_default(),
            bearer_token: entry.bearer_token.clone().unwrap_or_default(),
            header_rows: Self::key_value_rows_from_map(&entry.headers),
            header_from_env_rows: Self::key_value_rows_from_map(&entry.headers_from_env),
            oauth_client_id: entry
                .oauth
                .as_ref()
                .and_then(|oauth| oauth.client_id.clone())
                .unwrap_or_default(),
            oauth_client_secret: entry
                .oauth
                .as_ref()
                .and_then(|oauth| oauth.client_secret.clone())
                .unwrap_or_default(),
            oauth_scopes_text: entry
                .oauth
                .as_ref()
                .map(|oauth| join_mcp_server_oauth_scopes(&oauth.scopes))
                .unwrap_or_default(),
            oauth_redirect_uri: entry
                .oauth
                .as_ref()
                .and_then(|oauth| oauth.redirect_uri.clone())
                .unwrap_or_default(),
            cwd: entry
                .cwd
                .clone()
                .unwrap_or_else(Self::default_working_directory),
            env_rows: Self::key_value_rows_from_map(&entry.env),
            env_passthrough_rows: Self::text_rows_from_values(&entry.env_passthrough),
        }
    }

    /// Converts the editable form state into one persisted MCP server entry.
    /// 将可编辑表单状态转换为单个持久化 MCP 服务器条目。
    pub(super) fn to_entry(&self) -> Result<PersistedMcpServerEntry, String> {
        let name = self.name.trim();
        if name.is_empty() {
            return Err(String::from("请先填写 MCP 服务器名称。"));
        }
        let oauth_config = self.oauth_config();

        match self.transport {
            PersistedMcpTransportKind::Stdio => {
                let command = self.command.trim();
                if command.is_empty() {
                    return Err(String::from("请先填写 MCP 服务器启动命令。"));
                }

                Ok(PersistedMcpServerEntry {
                    name: name.to_string(),
                    enabled: self.enabled,
                    transport: PersistedMcpTransportKind::Stdio,
                    command: command.to_string(),
                    args: self.resolved_argument_rows(),
                    url: None,
                    bearer_token: None,
                    headers: BTreeMap::new(),
                    headers_from_env: BTreeMap::new(),
                    oauth: None,
                    cwd: trim_to_option(&self.cwd),
                    env: Self::resolved_key_value_rows(&self.env_rows, "环境变量", "值")?,
                    env_passthrough: self.resolved_env_passthrough_rows(),
                })
            }
            PersistedMcpTransportKind::Http | PersistedMcpTransportKind::Sse => {
                let url = self.url.trim();
                if url.is_empty() {
                    return Err(String::from("请先填写 MCP 远端服务器 URL。"));
                }
                if !(url.starts_with("http://") || url.starts_with("https://")) {
                    return Err(String::from(
                        "MCP 远端服务器 URL 必须以 `http://` 或 `https://` 开头。",
                    ));
                }

                Ok(PersistedMcpServerEntry {
                    name: name.to_string(),
                    enabled: self.enabled,
                    transport: self.transport,
                    command: String::new(),
                    args: Vec::new(),
                    url: Some(url.to_string()),
                    bearer_token: trim_to_option(&self.bearer_token),
                    headers: Self::resolved_key_value_rows(&self.header_rows, "请求头", "值")?,
                    headers_from_env: Self::resolved_key_value_rows(
                        &self.header_from_env_rows,
                        "来自环境变量的请求头",
                        "环境变量名",
                    )?,
                    oauth: oauth_config,
                    cwd: None,
                    env: BTreeMap::new(),
                    env_passthrough: Vec::new(),
                })
            }
        }
    }

    /// Resolves one persisted OAuth configuration block from the editable MCP server form when any OAuth field is currently populated.
    /// 当任一 OAuth 字段已填写时，从可编辑 MCP 服务器表单解析出一份可持久化 OAuth 配置块。
    fn oauth_config(&self) -> Option<PersistedMcpOAuthConfig> {
        let client_id = trim_to_option(&self.oauth_client_id);
        let client_secret = trim_to_option(&self.oauth_client_secret);
        let redirect_uri = trim_to_option(&self.oauth_redirect_uri);
        let scopes = parse_mcp_server_oauth_scopes(&self.oauth_scopes_text);

        if client_id.is_none()
            && client_secret.is_none()
            && redirect_uri.is_none()
            && scopes.is_empty()
        {
            return None;
        }

        Some(PersistedMcpOAuthConfig {
            client_id,
            client_secret,
            scopes,
            redirect_uri,
        })
    }
}

impl Default for McpServerFormState {
    /// Builds the default editable MCP server form.
    /// 构建默认的可编辑 MCP 服务器表单。
    fn default() -> Self {
        Self {
            name: String::new(),
            enabled: false,
            transport: PersistedMcpTransportKind::Stdio,
            command: String::new(),
            args_rows: vec![McpTextRowState::default()],
            url: String::new(),
            bearer_token: String::new(),
            header_rows: vec![McpKeyValueRowState::default()],
            header_from_env_rows: vec![McpKeyValueRowState::default()],
            oauth_client_id: String::new(),
            oauth_client_secret: String::new(),
            oauth_scopes_text: String::new(),
            oauth_redirect_uri: String::new(),
            cwd: Self::default_working_directory(),
            env_rows: vec![McpKeyValueRowState::default()],
            env_passthrough_rows: vec![McpTextRowState::default()],
        }
    }
}
