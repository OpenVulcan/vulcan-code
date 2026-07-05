//! MCP settings mutation tests.
//! MCP 设置变更测试。

use super::*;

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
