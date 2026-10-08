//! MCP settings mutation tests.
//! MCP 设置变更测试。

use super::*;

/// Verifies the settings parser preserves exact process arguments without touching real configuration.
/// 验证设置解析器保留精确进程参数，不接触真实配置。
#[test]
fn mcp_settings_preserves_exact_argument_boundaries() {
    // request is entirely synthetic, including whitespace and an intentional empty argument.
    // request 完全由合成数据组成，包含空白与有意传入的空参数。
    let request: McpServerSaveRequest = serde_json::from_value(serde_json::json!({
        "serverId": null, "name": "argv-fixture", "enabled": true, "transport": "stdio",
        "command": "node", "args": [" a b ", "", "--key=a=b", "quoted 'value'"], "url": "",
        "bearerToken": "", "headers": [], "headersFromEnv": [], "oauthClientId": "", "oauthClientSecret": "",
        "oauthScopes": [], "oauthRedirectUri": "", "cwd": "", "env": [{"key":"EXACT_VALUE","value":" a=b "}], "envPassthrough": ["API_TOKEN"]
    })).expect("decode synthetic request");
    // entry is an in-memory parse result only; no process is spawned.
    // entry 仅为内存解析结果，不启动进程。
    let (_, entry) = mcp_server_entry_from_request(request).expect("parse exact argv");
    assert_eq!(entry.args, [" a b ", "", "--key=a=b", "quoted 'value'"]);
    assert_eq!(entry.env_passthrough, ["API_TOKEN"]);
    assert_eq!(entry.env["EXACT_VALUE"], " a=b ");
}

/// Builds one synthetic remote save request with every OAuth field empty.
/// 构建一个全部 OAuth 字段为空的合成远端保存请求。
fn remote_request_without_oauth_fields(oauth_enabled: Option<bool>) -> McpServerSaveRequest {
    let mut value = serde_json::json!({
        "serverId": null, "name": "remote-fixture", "enabled": true, "transport": "http",
        "command": "", "args": [], "url": "https://mcp.example.test/mcp",
        "bearerToken": "", "headers": [], "headersFromEnv": [], "oauthClientId": "", "oauthClientSecret": "",
        "oauthScopes": [], "oauthRedirectUri": "", "cwd": "", "env": [], "envPassthrough": []
    });
    if let Some(enabled) = oauth_enabled {
        value["oauthEnabled"] = serde_json::Value::Bool(enabled);
    }
    serde_json::from_value(value).expect("decode synthetic request")
}

/// Verifies dynamic-registration OAuth persists an empty OAuth block only when explicitly enabled.
/// 验证动态注册 OAuth 仅在显式启用时持久化空 OAuth 配置块。
#[test]
fn oauth_enabled_flag_keeps_an_empty_oauth_block() {
    let (_, enabled) =
        mcp_server_entry_from_request(remote_request_without_oauth_fields(Some(true)))
            .expect("parse enabled request");
    assert!(enabled.oauth.is_some());

    for flag in [Some(false), None] {
        let (_, disabled) =
            mcp_server_entry_from_request(remote_request_without_oauth_fields(flag))
                .expect("parse request without OAuth");
        assert!(disabled.oauth.is_none());
    }
}

/// Builds one minimal stdio MCP server entry for configuration mutation tests.
/// 为配置变更测试构建一个最小 stdio MCP 服务器条目。
///
/// # Parameters
/// 参数
/// `name` is the user-facing MCP server name.
/// `name` 是面向用户显示的 MCP 服务器名称。
/// `command` is the process command persisted for stdio launch.
/// `command` 是持久化用于 stdio 启动的进程命令。
///
/// # Returns
/// 返回值
/// A complete persisted MCP server entry with empty optional fields.
/// 返回一个可选字段为空的完整持久化 MCP 服务器条目。
fn test_stdio_mcp_server_entry(name: &str, command: &str) -> PersistedMcpServerEntry {
    PersistedMcpServerEntry {
        name: name.to_string(),
        enabled: true,
        transport: PersistedMcpTransportKind::Stdio,
        command: command.to_string(),
        args: Vec::new(),
        url: None,
        bearer_token: None,
        headers: BTreeMap::new(),
        headers_from_env: BTreeMap::new(),
        oauth: None,
        cwd: None,
        env: BTreeMap::new(),
        env_passthrough: Vec::new(),
    }
}

/// Verifies creating a same-id MCP server cannot silently overwrite the existing entry.
/// 验证新增同标识 MCP 服务器时不能静默覆盖已有条目。
#[test]
fn create_mcp_server_entry_rejects_existing_id() {
    let mut configuration = ProviderConfigurationFile::default();
    configuration.mcp.servers.insert(
        String::from("local-search"),
        test_stdio_mcp_server_entry("Local Search", "old-command"),
    );

    let error = apply_mcp_server_entry_to_configuration(
        &mut configuration,
        String::from("local-search"),
        test_stdio_mcp_server_entry("Local Search", "new-command"),
        true,
    )
    .expect_err("duplicate create must fail");

    assert!(error.contains("已存在"), "unexpected error: {error}");
    assert_eq!(
        configuration.mcp.servers["local-search"].command,
        "old-command"
    );
}

/// Verifies editing a missing MCP server id does not create a new shadow entry.
/// 验证编辑缺失 MCP 服务器标识时不会创建新的影子条目。
#[test]
fn update_mcp_server_entry_rejects_missing_id() {
    let mut configuration = ProviderConfigurationFile::default();

    let error = apply_mcp_server_entry_to_configuration(
        &mut configuration,
        String::from("missing-server"),
        test_stdio_mcp_server_entry("Missing Server", "new-command"),
        false,
    )
    .expect_err("missing update target must fail");

    assert!(error.contains("未找到"), "unexpected error: {error}");
    assert!(configuration.mcp.servers.is_empty());
}

/// Verifies MCP secrets leave the service only as placeholders and an untouched form keeps them.
/// 验证 MCP 秘密仅以占位符离开服务，且未改动的表单会保留原秘密。
#[test]
fn mcp_secrets_round_trip_through_placeholders() {
    let mut configuration = ProviderConfigurationFile::default();
    let mut remote = test_stdio_mcp_server_entry("Remote", "");
    remote.transport = PersistedMcpTransportKind::Http;
    remote.url = Some(String::from("https://mcp.example.test/mcp"));
    remote.bearer_token = Some(String::from("synthetic-bearer"));
    remote.oauth = Some(PersistedMcpOAuthConfig {
        client_id: Some(String::from("client")),
        client_secret: Some(String::from("synthetic-client-secret")),
        scopes: Vec::new(),
        redirect_uri: None,
    });
    configuration
        .mcp
        .servers
        .insert(String::from("remote"), remote);

    let summary = crate::settings_provider_projection::mcp_servers(&configuration)
        .pop()
        .expect("projected server");
    assert_eq!(
        summary.bearer_token,
        vulcan_service_api::WEBUI_REDACTED_SECRET_PLACEHOLDER
    );
    assert_eq!(
        summary.oauth_client_secret,
        vulcan_service_api::WEBUI_REDACTED_SECRET_PLACEHOLDER
    );

    // The form echoes the snapshot unchanged, placeholders included.
    // 表单原样回传快照，包括占位符。
    let request: McpServerSaveRequest = serde_json::from_value(serde_json::json!({
        "serverId": "remote", "name": "Remote", "enabled": true, "transport": "http",
        "command": "", "args": [], "url": "https://mcp.example.test/mcp",
        "bearerToken": summary.bearer_token, "headers": [], "headersFromEnv": [],
        "oauthEnabled": true, "oauthClientId": "client", "oauthClientSecret": summary.oauth_client_secret,
        "oauthScopes": [], "oauthRedirectUri": "", "cwd": "", "env": [], "envPassthrough": []
    }))
    .expect("decode synthetic request");
    let (server_id, entry) = mcp_server_entry_from_request(request).expect("parse edit");
    apply_mcp_server_entry_to_configuration(&mut configuration, server_id, entry, false)
        .expect("apply edit");

    let saved = &configuration.mcp.servers["remote"];
    assert_eq!(saved.bearer_token.as_deref(), Some("synthetic-bearer"));
    assert_eq!(
        saved
            .oauth
            .as_ref()
            .and_then(|oauth| oauth.client_secret.as_deref()),
        Some("synthetic-client-secret")
    );
}
