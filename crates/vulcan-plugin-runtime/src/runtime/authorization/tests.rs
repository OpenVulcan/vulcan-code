//! Sanitized MCP ownership in permission previews.
//! 权限预览中的脱敏MCP归属。

use super::super::tests::{test_manager, write_user_plugin_package};
use super::VulcanPluginMcpPermissionPreview;
use std::fs;

/// Verifies one HTTP MCP declaration projects ownership and Secret key names without values.
/// 验证一个HTTP MCP声明投影归属与Secret键名但不包含值。
#[test]
fn user_plugin_permission_preview_includes_sanitized_mcp_ownership() {
    // temp owns the source package and isolated runtime state for preview construction.
    // temp 持有用于构建预览的来源包与隔离运行时状态。
    let temp = tempfile::tempdir().expect("create MCP permission preview directory");
    // runtime_root provides deterministic manager state without installed user packages.
    // runtime_root 提供不含已安装用户插件的确定性管理器状态。
    let runtime_root = temp.path().join("runtime");
    // vulcan_plugin_root is the empty bundled source required by the test manager.
    // vulcan_plugin_root 是测试管理器要求的空内置来源。
    let vulcan_plugin_root = runtime_root.join("plugins");
    // source_package contains one remote MCP declaration backed by a namespaced Secret.
    // source_package 包含一个由命名空间 Secret 支撑的远程 MCP 声明。
    let source_package = temp.path().join("source").join("local-plugin");
    fs::create_dir_all(&vulcan_plugin_root).expect("create empty Vulcan plugin root");
    write_user_plugin_package(&source_package, "local-plugin");
    // manifest extends the ordinary fixture with one valid remote MCP server.
    // manifest 使用一个有效远程 MCP 服务器扩展普通夹具。
    let mut manifest =
        fs::read_to_string(source_package.join("plugin.yaml")).expect("read local plugin manifest");
    manifest.push_str(
        r#"permissions:
  - network_https
  - secrets_read
mcp:
  servers:
    - id: remote-docs
      name: Remote Docs
      transport:
        type: http
        url:
          source: literal
          value: https://example.test/mcp
        headers:
          Authorization:
            source: secret
            key: access-token
"#,
    );
    fs::write(source_package.join("plugin.yaml"), manifest)
        .expect("write MCP permission preview manifest");
    // manager performs the same strict source inspection used by the authorization UI.
    // manager 执行授权界面使用的同一严格来源检查。
    let manager = test_manager(&runtime_root, vulcan_plugin_root);

    // preview is safe to serialize because it contains only server ownership and Secret key names.
    // preview 可安全序列化，因为它只包含服务器归属与 Secret 键名。
    let preview = manager
        .inspect_local_user_plugin(&source_package)
        .expect("inspect MCP permission preview package");
    assert_eq!(
        preview.mcp_servers,
        vec![VulcanPluginMcpPermissionPreview {
            server_id: String::from("remote-docs"),
            name: String::from("Remote Docs"),
            transport: String::from("http"),
            secret_keys: vec![String::from("access-token")],
        }]
    );
    assert!(preview.install_secret_requirements.is_empty());
}
