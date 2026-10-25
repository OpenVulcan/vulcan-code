//! Tests for planning `mcp_config` saves against a configuration.
//! 针对配置规划 `mcp_config` 保存的测试。

use super::*;

/// A configuration holding one enabled remote server with every kind of hidden value.
/// 持有一个已启用远端服务器且包含各类隐藏值的配置。
fn configuration_with_remote_server() -> ProviderConfigurationFile {
    let mut configuration = ProviderConfigurationFile::default();
    configuration.mcp.servers.insert(
        String::from("docs"),
        PersistedMcpServerEntry {
            name: String::from("Docs"),
            enabled: true,
            transport: PersistedMcpTransportKind::Http,
            command: String::new(),
            args: Vec::new(),
            url: Some(String::from("https://docs.example.test/mcp")),
            bearer_token: Some(String::from("token-value")),
            headers: BTreeMap::from([(String::from("X-Team"), String::from("literal"))]),
            headers_from_env: BTreeMap::new(),
            oauth: Some(PersistedMcpOAuthConfig {
                client_id: Some(String::from("client")),
                client_secret: Some(String::from("client-secret")),
                scopes: Vec::new(),
                redirect_uri: None,
            }),
            cwd: None,
            env: BTreeMap::new(),
            env_passthrough: Vec::new(),
        },
    );
    configuration
}

/// Builds a save request from JSON arguments, as the tool receives them.
/// 从 JSON 参数构建保存请求，与工具收到的形式一致。
fn save(args: serde_json::Value) -> McpConfigRequest {
    match ConfigurationRequest::parse(
        vulcan_agent_runtime::tooling::native::MCP_CONFIG_TOOL_ID,
        &args,
    )
    .expect("save arguments should parse")
    {
        ConfigurationRequest::Mcp(request) => request,
        other => panic!("expected an MCP request, got {other:?}"),
    }
}

/// The preview field keys of one plan.
/// 一个计划的预览字段键。
fn changed_fields(plan: &McpSavePlan) -> Vec<String> {
    plan.preview()
        .changes
        .into_iter()
        .map(|change| change.field)
        .collect()
}

/// New servers stay disabled unless enabled explicitly, and enabling one is announced.
/// 新服务器除非显式启用否则保持停用，启用时会给出提示。
#[test]
fn new_servers_stay_disabled_unless_enabled() {
    let configuration = ProviderConfigurationFile::default();
    let plan = plan_mcp_save(
        &configuration,
        &save(json!({
            "action": "save", "name": "Local Files", "transport": "stdio",
            "command": "npx", "args": ["-y", "files-server"],
        })),
    )
    .expect("a complete stdio server should plan");
    assert_eq!(plan.server_id, "local-files");
    assert!(!plan.after.enabled);
    let preview = plan.preview();
    assert_eq!(preview.action, "create");
    assert!(preview.notices.is_empty());
    assert!(preview.changes.iter().all(|change| change.before.is_none()));

    let enabled = plan_mcp_save(
        &configuration,
        &save(json!({
            "action": "save", "name": "Local Files", "transport": "stdio",
            "command": "npx", "enabled": true,
        })),
    )
    .expect("an enabled stdio server should plan");
    assert_eq!(enabled.preview().notices, ["activates_for_all_sessions"]);
}

/// Updates keep omitted fields and hidden values, and list only the fields that change.
/// 更新保留省略的字段与隐藏值，并只列出发生变化的字段。
#[test]
fn updates_keep_secrets_and_show_only_changes() {
    let configuration = configuration_with_remote_server();
    let plan = plan_mcp_save(
        &configuration,
        &save(json!({
            "action": "save", "server_id": "docs",
            "url": "https://docs.example.test/v2/mcp",
            "headers_from_env": { "X-Key": "DOCS_KEY" },
        })),
    )
    .expect("an update of a known server should plan");
    assert_eq!(plan.after.bearer_token.as_deref(), Some("token-value"));
    assert_eq!(plan.after.headers.len(), 1);
    assert!(plan.after.oauth.is_some());
    assert!(plan.after.enabled);
    assert_eq!(changed_fields(&plan), ["url", "headers_from_env"]);
    let preview = plan.preview();
    assert_eq!(preview.action, "update");
    assert!(preview.notices.is_empty());
    let rendered = format!("{:?}", preview);
    assert!(!rendered.contains("token-value") && !rendered.contains("client-secret"));
}

/// Switching between remote and stdio drops the hidden values and says so.
/// 在远端与 stdio 之间切换会丢弃隐藏值并给出提示。
#[test]
fn transport_switch_drops_hidden_values_with_notice() {
    let configuration = configuration_with_remote_server();
    let plan = plan_mcp_save(
        &configuration,
        &save(json!({
            "action": "save", "server_id": "docs", "transport": "stdio", "command": "docs-mcp",
        })),
    )
    .expect("switching to stdio with a command should plan");
    assert!(plan.after.bearer_token.is_none());
    assert!(plan.after.headers.is_empty());
    assert!(plan.after.oauth.is_none());
    assert!(plan.after.url.is_none());
    assert!(
        plan.preview()
            .notices
            .contains(&String::from("drops_secret_fields"))
    );
}

/// Invalid saves are refused before anything is shown to the user.
/// 无效的保存在展示给用户之前就被拒绝。
#[test]
fn invalid_saves_are_refused() {
    let configuration = configuration_with_remote_server();
    for args in [
        json!({ "action": "save", "server_id": "missing", "enabled": true }),
        json!({ "action": "save", "name": "Docs", "transport": "http", "url": "https://x.test" }),
        json!({ "action": "save", "name": "Tool", "transport": "stdio" }),
        json!({ "action": "save", "name": "Remote", "transport": "http", "url": "ftp://x.test" }),
        json!({ "action": "save", "name": "Unknown", "transport": "pipe", "command": "x" }),
    ] {
        assert!(
            plan_mcp_save(&configuration, &save(args.clone())).is_err(),
            "{args} should be refused"
        );
    }
}

/// The settings requests the tool builds are accepted by the settings request types.
/// 工具构建的设置请求被设置请求类型接受。
#[test]
fn settings_requests_match_the_settings_types() {
    add_connection_request("deepseek", "api", "api-key", "DeepSeek", "key")
        .expect("the add-connection request should parse");
    custom_provider_request(
        "Relay",
        "https://relay.example.test/v1",
        CustomProviderAuthentication::BearerAuthorization,
        &[vulcan_provider_adapters::CustomProviderProtocol::OpenAiChat],
        "key",
    )
    .expect("the custom-provider request should parse");
    connection_enabled_request("c", false).expect("the enabled request should parse");
    default_connection_request("p", "c").expect("the default-connection request should parse");
}
