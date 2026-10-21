use super::*;

/// Maximum number of MCP clients one installation may register.
/// 一个安装可登记的最大 MCP 客户端数。
pub const MAX_MCP_SERVICE_CLIENTS: usize = 32;

/// Maximum characters of one MCP client display name.
/// 一个 MCP 客户端显示名称的最大字符数。
pub const MAX_MCP_CLIENT_NAME_CHARS: usize = 80;

/// Maximum number of projects one MCP client grant may list explicitly.
/// 一个 MCP 客户端授权可显式列出的最大项目数。
pub const MAX_MCP_CLIENT_PROJECTS: usize = 256;

/// Maximum number of media targets one MCP client grant may list.
/// 一个 MCP 客户端授权可列出的最大媒体目标数。
pub const MAX_MCP_CLIENT_MEDIA_TARGETS: usize = 64;

/// Exact length of a lowercase hexadecimal SHA-256 digest.
/// 小写十六进制 SHA-256 摘要的精确长度。
const MCP_TOKEN_SHA256_HEX_LEN: usize = 64;

/// Maximum characters of the non-secret token hint shown in settings.
/// 设置中展示的非秘密令牌提示的最大字符数。
const MAX_MCP_TOKEN_HINT_CHARS: usize = 8;

/// System-level MCP service settings: the master switch and every registered client.
/// 系统级 MCP 服务设置：总开关与每个已登记客户端。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PersistedMcpServiceConfiguration {
    /// Whether the `/mcp` endpoint accepts authenticated requests at all.
    /// `/mcp` 端点是否接受任何已认证请求。
    #[serde(default)]
    pub enabled: bool,
    /// Registered clients in creation order.
    /// 按创建顺序排列的已登记客户端。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub clients: Vec<McpServiceClient>,
}

impl PersistedMcpServiceConfiguration {
    /// Returns whether nothing was configured, so the field stays out of the saved file.
    /// 判断是否未做任何配置，使该字段不写入保存文件。
    pub fn is_default(&self) -> bool {
        !self.enabled && self.clients.is_empty()
    }
}

/// One external AI client allowed to call the MCP endpoint with its own bearer token.
/// 一个持有自有 Bearer 令牌、可调用 MCP 端点的外部 AI 客户端。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct McpServiceClient {
    /// Stable client identifier.
    /// 稳定客户端标识。
    pub id: String,
    /// User-chosen display name.
    /// 用户选择的显示名称。
    pub name: String,
    /// Lowercase hexadecimal SHA-256 of the bearer token; the token itself is never stored.
    /// Bearer 令牌的小写十六进制 SHA-256；令牌本身从不保存。
    pub token_sha256: String,
    /// Last characters of the token, letting the user tell tokens apart.
    /// 令牌末尾几个字符，便于用户区分令牌。
    pub token_hint: String,
    /// Creation time in Unix milliseconds.
    /// 创建时间（Unix 毫秒）。
    pub created_at_ms: u64,
    /// Whether this client may currently authenticate.
    /// 该客户端当前是否可以认证。
    pub enabled: bool,
    /// Session operations granted to this client.
    /// 授予该客户端的会话操作。
    pub sessions: McpSessionGrant,
    /// Exact media-generation targets this client may run.
    /// 该客户端可运行的精确媒体生成目标。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub media_targets: Vec<McpMediaTargetGrant>,
}

/// Session operations one MCP client may perform, limited to a project scope.
/// 一个 MCP 客户端可执行的会话操作，受项目范围限制。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct McpSessionGrant {
    /// Lists projects and sessions, reads messages, and waits for replies.
    /// 列出项目与会话、读取消息并等待回复。
    #[serde(default)]
    pub read: bool,
    /// Sends messages to existing sessions.
    /// 向已有会话发送消息。
    #[serde(default)]
    pub send: bool,
    /// Creates sessions with a first message.
    /// 以首条消息新建会话。
    #[serde(default)]
    pub create: bool,
    /// Stops running sessions.
    /// 停止运行中的会话。
    #[serde(default)]
    pub stop: bool,
    /// Answers or dismisses questions a session asks; never permission approvals.
    /// 回答或驳回会话提出的问题；绝不包括权限审批。
    #[serde(default)]
    pub answer_questions: bool,
    /// Projects whose sessions the grant covers.
    /// 授权覆盖其会话的项目。
    #[serde(default)]
    pub projects: McpProjectScope,
}

impl McpSessionGrant {
    /// Returns whether any session operation is granted.
    /// 判断是否授予了任何会话操作。
    pub fn any(&self) -> bool {
        self.read || self.send || self.create || self.stop || self.answer_questions
    }

    /// Returns whether `project_id` lies inside the granted scope.
    /// 判断 `project_id` 是否位于授权范围内。
    pub fn covers_project(&self, project_id: &str) -> bool {
        match &self.projects {
            McpProjectScope::All => true,
            McpProjectScope::Selected { project_ids } => {
                project_ids.iter().any(|candidate| candidate == project_id)
            }
        }
    }
}

/// Projects covered by one session grant.
/// 一份会话授权覆盖的项目。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "mode",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum McpProjectScope {
    /// Every project, including ones added later.
    /// 全部项目，包括之后新增的项目。
    #[default]
    All,
    /// Only the listed projects.
    /// 仅列出的项目。
    Selected {
        /// Exact project identifiers.
        /// 精确项目标识。
        project_ids: Vec<String>,
    },
}

/// One exact provider-tool target an MCP client may run.
/// MCP 客户端可运行的一个精确供应商工具目标。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct McpMediaTargetGrant {
    /// Credential connection that pays for the call.
    /// 为调用付费的凭据连接。
    pub connection_id: String,
    /// Exact manifest target identifier.
    /// 精确 Manifest 目标标识。
    pub target_id: String,
}

/// Validates and normalizes the MCP service settings before they are saved.
/// 在保存前校验并规范化 MCP 服务设置。
///
/// # Parameters
/// 参数
/// `configuration` is the complete candidate settings value.
/// `configuration` 是完整的候选设置值。
///
/// # Returns
/// 返回值
/// The value with trimmed names and identifiers, or an English error naming the first problem.
/// 名称与标识去除首尾空白后的值，或指出首个问题的英文错误。
pub fn normalize_mcp_service_configuration(
    configuration: PersistedMcpServiceConfiguration,
) -> Result<PersistedMcpServiceConfiguration, String> {
    if configuration.clients.len() > MAX_MCP_SERVICE_CLIENTS {
        return Err(format!(
            "The MCP service holds at most {MAX_MCP_SERVICE_CLIENTS} clients."
        ));
    }
    let mut clients: Vec<McpServiceClient> = Vec::with_capacity(configuration.clients.len());
    for client in configuration.clients {
        let client = normalize_mcp_service_client(client)?;
        if clients.iter().any(|existing| existing.id == client.id) {
            return Err(format!(
                "MCP client `{}` appears more than once.",
                client.id
            ));
        }
        if clients
            .iter()
            .any(|existing| existing.token_sha256 == client.token_sha256)
        {
            return Err(format!(
                "MCP client `{}` reuses another client's token.",
                client.id
            ));
        }
        clients.push(client);
    }
    Ok(PersistedMcpServiceConfiguration {
        enabled: configuration.enabled,
        clients,
    })
}

/// Validates one client entry.
/// 校验一个客户端条目。
fn normalize_mcp_service_client(client: McpServiceClient) -> Result<McpServiceClient, String> {
    let id = client.id.trim().to_string();
    if id.is_empty() {
        return Err(String::from("An MCP client needs an identifier."));
    }
    let name = client.name.trim().to_string();
    if name.is_empty() {
        return Err(format!("MCP client `{id}` needs a name."));
    }
    if name.chars().count() > MAX_MCP_CLIENT_NAME_CHARS {
        return Err(format!(
            "The name of MCP client `{id}` exceeds {MAX_MCP_CLIENT_NAME_CHARS} characters."
        ));
    }
    if client.token_sha256.len() != MCP_TOKEN_SHA256_HEX_LEN
        || !client
            .token_sha256
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(format!(
            "MCP client `{id}` must store a lowercase hexadecimal SHA-256 token digest."
        ));
    }
    if client.token_hint.chars().count() > MAX_MCP_TOKEN_HINT_CHARS {
        return Err(format!(
            "The token hint of MCP client `{id}` exceeds {MAX_MCP_TOKEN_HINT_CHARS} characters."
        ));
    }
    let sessions = normalize_mcp_session_grant(&id, client.sessions)?;
    let media_targets = normalize_mcp_media_targets(&id, client.media_targets)?;
    Ok(McpServiceClient {
        id,
        name,
        token_sha256: client.token_sha256,
        token_hint: client.token_hint,
        created_at_ms: client.created_at_ms,
        enabled: client.enabled,
        sessions,
        media_targets,
    })
}

/// Validates one session grant: every write operation also needs read access.
/// 校验一份会话授权：每项写入操作都必须同时具备读取权限。
fn normalize_mcp_session_grant(
    client_id: &str,
    grant: McpSessionGrant,
) -> Result<McpSessionGrant, String> {
    if (grant.send || grant.create || grant.stop || grant.answer_questions) && !grant.read {
        return Err(format!(
            "MCP client `{client_id}` must have read access to send, create, stop, or answer."
        ));
    }
    let projects = match grant.projects {
        McpProjectScope::All => McpProjectScope::All,
        McpProjectScope::Selected { project_ids } => {
            if project_ids.len() > MAX_MCP_CLIENT_PROJECTS {
                return Err(format!(
                    "MCP client `{client_id}` lists more than {MAX_MCP_CLIENT_PROJECTS} projects."
                ));
            }
            let mut normalized: Vec<String> = Vec::with_capacity(project_ids.len());
            for project_id in project_ids {
                let project_id = project_id.trim().to_string();
                if project_id.is_empty() {
                    return Err(format!(
                        "MCP client `{client_id}` lists an empty project identifier."
                    ));
                }
                if normalized.contains(&project_id) {
                    return Err(format!(
                        "MCP client `{client_id}` lists project `{project_id}` more than once."
                    ));
                }
                normalized.push(project_id);
            }
            McpProjectScope::Selected {
                project_ids: normalized,
            }
        }
    };
    Ok(McpSessionGrant { projects, ..grant })
}

/// Validates the media target list of one client.
/// 校验一个客户端的媒体目标清单。
fn normalize_mcp_media_targets(
    client_id: &str,
    targets: Vec<McpMediaTargetGrant>,
) -> Result<Vec<McpMediaTargetGrant>, String> {
    if targets.len() > MAX_MCP_CLIENT_MEDIA_TARGETS {
        return Err(format!(
            "MCP client `{client_id}` lists more than {MAX_MCP_CLIENT_MEDIA_TARGETS} media targets."
        ));
    }
    let mut normalized: Vec<McpMediaTargetGrant> = Vec::with_capacity(targets.len());
    for target in targets {
        let target = McpMediaTargetGrant {
            connection_id: target.connection_id.trim().to_string(),
            target_id: target.target_id.trim().to_string(),
        };
        if target.connection_id.is_empty() || target.target_id.is_empty() {
            return Err(format!(
                "A media target of MCP client `{client_id}` needs both an account and a target."
            ));
        }
        if normalized.contains(&target) {
            return Err(format!(
                "MCP client `{client_id}` lists media target `{}` more than once.",
                target.target_id
            ));
        }
        normalized.push(target);
    }
    Ok(normalized)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds one valid client for normalization tests.
    /// 为规范化测试构建一个有效客户端。
    fn client(id: &str, digest_byte: char) -> McpServiceClient {
        McpServiceClient {
            id: id.to_string(),
            name: String::from(" Claude Code "),
            token_sha256: digest_byte.to_string().repeat(64),
            token_hint: String::from("ab12"),
            created_at_ms: 1,
            enabled: true,
            sessions: McpSessionGrant {
                read: true,
                send: true,
                ..McpSessionGrant::default()
            },
            media_targets: Vec::new(),
        }
    }

    #[test]
    fn normalization_trims_names_and_keeps_valid_clients() {
        let normalized = normalize_mcp_service_configuration(PersistedMcpServiceConfiguration {
            enabled: true,
            clients: vec![client("a", 'a'), client("b", 'b')],
        })
        .expect("valid configuration");
        assert_eq!(normalized.clients.len(), 2);
        assert_eq!(normalized.clients[0].name, "Claude Code");
    }

    #[test]
    fn write_operations_require_read_access() {
        let mut candidate = client("a", 'a');
        candidate.sessions.read = false;
        assert!(
            normalize_mcp_service_configuration(PersistedMcpServiceConfiguration {
                enabled: true,
                clients: vec![candidate],
            })
            .is_err()
        );
    }

    #[test]
    fn duplicate_tokens_and_bad_digests_are_rejected() {
        let duplicate = PersistedMcpServiceConfiguration {
            enabled: true,
            clients: vec![client("a", 'a'), client("b", 'a')],
        };
        assert!(normalize_mcp_service_configuration(duplicate).is_err());
        let mut uppercase = client("a", 'a');
        uppercase.token_sha256 = "A".repeat(64);
        assert!(
            normalize_mcp_service_configuration(PersistedMcpServiceConfiguration {
                enabled: false,
                clients: vec![uppercase],
            })
            .is_err()
        );
    }

    #[test]
    fn project_scope_round_trips_through_the_tagged_wire_shape() {
        let mut candidate = client("a", 'a');
        candidate.sessions.projects = McpProjectScope::Selected {
            project_ids: vec![String::from("p1")],
        };
        let value = serde_json::to_value(&candidate).expect("serialize");
        assert_eq!(value["sessions"]["projects"]["mode"], "selected");
        assert_eq!(value["sessions"]["projects"]["projectIds"][0], "p1");
        let decoded: McpServiceClient = serde_json::from_value(value).expect("deserialize");
        assert!(decoded.sessions.covers_project("p1"));
        assert!(!decoded.sessions.covers_project("p2"));
    }
}
