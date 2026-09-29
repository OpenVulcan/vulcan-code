//! Real managed catalogs feed frozen credential bindings into every MCP transport.
//! 真实受管目录将固定凭据绑定传递至每种 MCP 传输。

use super::*;
use std::sync::Mutex;
use vulcan_plugin_runtime::{VulcanPluginCredentialRevision, VulcanPluginGenerationStore};

/// Isolated backend records exact identities and provides distinct values for each explicit revision.
/// 隔离后端记录精确身份，并为每个显式修订提供不同值。
struct RevisionSecrets {
    /// Known revision labels never consult package state or a latest-selection pointer.
    /// 已知修订标签绝不查询插件包状态或最新选择指针。
    labels: BTreeMap<VulcanPluginCredentialRevision, &'static str>,
    /// All reads retain their declared revision and key for no-fallback assertions.
    /// 全部读取保留声明修订及键，用于不回退断言。
    reads: Mutex<Vec<(String, String, String, VulcanPluginCredentialBinding)>>,
}

impl PluginMcpSecretStore for RevisionSecrets {
    /// Resolve the exact plugin/server/key and captured binding; return a revision-specific fixture value.
    /// 解析精确插件、服务器、键及捕获绑定；返回对应修订的夹具值。
    /// Unknown revisions fail even while other revisions and the legacy namespace remain readable.
    /// 即使其他修订及旧命名空间可读，未知修订仍失败。
    fn read_secret(
        &self,
        plugin_id: &str,
        server_id: &str,
        key: &str,
        binding: &VulcanPluginCredentialBinding,
    ) -> Result<String, String> {
        assert_eq!(plugin_id, "bound-plugin");
        self.reads.lock().expect("credential read log").push((
            plugin_id.into(),
            server_id.into(),
            key.into(),
            binding.clone(),
        ));
        // Legacy remains available as a tripwire for any accidental fallback.
        // 旧命名空间保持可用，以检测任何意外回退。
        let label = match binding {
            VulcanPluginCredentialBinding::Legacy => "legacy",
            VulcanPluginCredentialBinding::Revision(revision) => self
                .labels
                .get(revision)
                .copied()
                .ok_or("Selected credential revision is missing.")?,
        };
        match key {
            "endpoint" => Ok(format!("https://{label}.example.test/mcp")),
            "redirect" => Ok(format!("https://{label}.example.test/callback")),
            "token" | "client" => Ok(format!("{label}-{key}")),
            _ => Err(String::from("Undeclared credential fixture key.")),
        }
    }
}

/// Write a valid all-transport package beneath `root`; return its source path without executing its fake binary.
/// 在 `root` 下写入有效全传输插件包；返回来源路径，不执行其假二进制。
fn package(root: &std::path::Path) -> std::path::PathBuf {
    // The source declares actual supported launcher targets so materialization follows production resolution.
    // 来源声明实际支持的启动器目标，使物化沿生产解析执行。
    let package = root.join("bound-plugin");
    fs::create_dir_all(package.join("bin")).expect("create credential package");
    fs::write(
        package.join("bin/server.bin"),
        b"fixture executable identity only",
    )
    .expect("write declared fixture binary");
    fs::write(
        package.join("plugin.yaml"),
        r#"schema_version: 1
id: bound-plugin
version: 1.0.0
default_enabled: true
display:
  name: Bound Credential Fixture
  capability: Exact revision materialization
permissions: [network_https, secrets_read, process_binary, process_session]
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
    - id: remote
      name: Remote
      transport:
        type: http
        url: {source: secret, key: endpoint}
        headers:
          Authorization: {source: secret, key: token}
        oauth:
          client_id: {source: secret, key: client}
          client_secret: {source: secret, key: token}
          redirect_uri: {source: secret, key: redirect}
    - id: events
      name: Events
      transport:
        type: sse
        url: {source: secret, key: endpoint}
        headers:
          Authorization: {source: secret, key: token}
    - id: local
      name: Local
      transport:
        type: stdio
        launcher: {type: native_binary, binary_id: server}
        args: [{source: secret, key: token}]
        env:
          TOKEN: {source: secret, key: token}
    - id: conversation
      name: Conversation
      scope: conversation
      transport:
        type: stdio
        launcher: {type: native_binary, binary_id: server}
        args: [{source: core_browser_endpoint}, {source: secret, key: token}]
"#,
    )
    .expect("write all-transport credential manifest");
    package
}

/// Assert resolved values belong to `label` for every supplied global transport; no network or child process runs.
/// 断言所供每种全局传输的解析值属于 `label`；不运行网络或子进程。
fn assert_values(configs: &[McpServerConfig], label: &str) {
    for config in configs {
        match &config.transport {
            McpServerTransportConfig::Http {
                url,
                headers,
                oauth,
                ..
            } => {
                assert_eq!(url, &format!("https://{label}.example.test/mcp"));
                assert_eq!(headers["Authorization"], format!("{label}-token"));
                // Explicit OAuth fields are checked independently to catch an unbound optional branch.
                // 独立检查显式 OAuth 字段，以捕获未绑定可选分支。
                let oauth = oauth.as_ref().expect("declared OAuth");
                assert_eq!(
                    oauth.client_id.as_deref(),
                    Some(format!("{label}-client").as_str())
                );
                assert_eq!(
                    oauth.client_secret.as_deref(),
                    Some(format!("{label}-token").as_str())
                );
                assert_eq!(
                    oauth.redirect_uri.as_deref(),
                    Some(format!("https://{label}.example.test/callback").as_str())
                );
            }
            McpServerTransportConfig::Sse { url, headers, .. } => {
                assert_eq!(url, &format!("https://{label}.example.test/mcp"));
                assert_eq!(headers["Authorization"], format!("{label}-token"));
            }
            McpServerTransportConfig::Stdio { args, env, .. } => {
                assert_eq!(args.last(), Some(&format!("{label}-token")));
                assert_eq!(env["TOKEN"], format!("{label}-token"));
            }
        }
    }
}

/// Actual selected descriptors retain their own credential revision after rotation, including all transport branches.
/// 实际选定描述符在轮换后保留自身凭据修订，覆盖全部传输分支。
#[test]
fn credential_binding_mcp_materialization_uses_frozen_selection_without_fallback() {
    // Real package, activation and catalog state remain isolated from the user's installation.
    // 实际插件包、激活及目录状态与用户安装隔离。
    let temp = tempfile::tempdir().expect("create MCP credential binding fixture");
    // Empty bundled and existing user roots follow the production immutable-store contract.
    // 空内置根及既有用户根遵循生产不可变存储契约。
    let bundled = temp.path().join("bundled");
    fs::create_dir(&bundled).expect("create bundled root");
    // The user root is the only selected package authority.
    // 用户根是唯一选定插件包权威。
    let user = temp.path().join("user");
    fs::create_dir(&user).expect("create user root");
    // A Lua-free production manager still resolves native launchers and MCP descriptors.
    // 无 Lua 的生产管理器仍解析原生启动器及 MCP 描述符。
    let plugins = VulcanPluginRuntimeManager::without_runtime(
        bundled,
        user.clone(),
        temp.path().join("bridge"),
        temp.path().join("managed"),
        temp.path().join("state"),
        temp.path().join("runtime"),
    );
    // Inspect actual distributable contents before any selected authority changes.
    // 任何选定权威改变前检查实际可分发内容。
    let source = package(&temp.path().join("source"));
    // The same preview supplies exact permissions to all publications.
    // 同一预览为全部发布提供精确权限。
    let preview = plugins
        .inspect_local_user_plugin(&source)
        .expect("inspect MCP fixture");
    // Generation staging is independent of credential revision selection.
    // 代次暂存独立于凭据修订选择。
    let store = VulcanPluginGenerationStore::open(&user).expect("open MCP selection store");
    // Retain one physical package while rotating credentials twice.
    // 两次轮换凭据期间保留单个物理插件包。
    let generation = store
        .prepare(&source, &preview.content_fingerprint)
        .expect("prepare MCP fixture");
    // Explicit revision A owns the initial configured values.
    // 显式修订 A 持有初始配置值。
    let first = VulcanPluginCredentialRevision::allocate();
    // Explicit revision B must never rewrite or relabel A's reads.
    // 显式修订 B 绝不能重写 A 的读取或重新标记其归属。
    let second = VulcanPluginCredentialRevision::allocate();
    // This fixture exposes both revisions and records every lookup tuple.
    // 此夹具暴露两个修订并记录每个查找元组。
    let mut secrets = RevisionSecrets {
        labels: BTreeMap::from([(first.clone(), "first"), (second.clone(), "second")]),
        reads: Mutex::new(Vec::new()),
    };
    store
        .activate_with_credentials(&generation, 0, preview.permissions.clone(), first.clone())
        .expect("publish first credentials");
    // The first immutable descriptor set retains its binding after subsequent manager refreshes.
    // 第一次不可变描述符集合在后续管理器刷新后仍保留绑定。
    let original = plugins
        .refresh_catalog(&Default::default(), &Default::default())
        .expect("load first credential selection");
    // Resolve every global branch through the same public adapter used by startup and lifecycle readiness.
    // 通过启动及生命周期就绪使用的相同公开适配器解析全部全局分支。
    let original_configs =
        plugin_mcp_server_configs(&plugins, &secrets).expect("materialize original credentials");
    assert_eq!(original_configs.len(), 3);
    assert_values(&original_configs, "first");
    store
        .activate_with_credentials(&generation, 1, preview.permissions, second.clone())
        .expect("rotate selected credentials");
    // Routing generation changes even though the package identity is unchanged.
    // 即使插件包身份未变，路由代次仍变化。
    let current = plugins
        .refresh_catalog(&Default::default(), &Default::default())
        .expect("load second credential selection");
    assert!(
        current.plugins["bound-plugin"].generation > original.plugins["bound-plugin"].generation
    );
    assert_values(
        &plugin_mcp_server_configs(&plugins, &secrets).expect("materialize current credentials"),
        "second",
    );
    assert_values(&original_configs, "first");
    // Remote descriptors need no current launcher lookup; retained descriptors must read their own original revision.
    // 远端描述符无需查询当前启动器；保留描述符必须读取自身原修订。
    let old_remote = original.plugins["bound-plugin"]
        .mcp_servers
        .iter()
        .filter(|server| {
            matches!(
                server.manifest.transport,
                VulcanPluginMcpTransportManifest::Http { .. }
                    | VulcanPluginMcpTransportManifest::Sse { .. }
            )
        })
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(old_remote.len(), 2);
    assert_values(
        &plugin_mcp_server_configs_for_descriptors(&plugins, &old_remote, &secrets)
            .expect("materialize retained remote binding"),
        "first",
    );
    // Conversation-scoped materialization uses the current descriptor binding and the separately supplied owner grant.
    // 会话作用域物化使用当前描述符绑定及独立提供的所有者授权。
    let conversation = plugin_conversation_mcp_server_config(
        &plugins,
        "bound-plugin",
        "conversation",
        &secrets,
        "owner",
        "ws://127.0.0.1:35123/devtools/browser/Fixture_123",
    )
    .expect("materialize bound conversation");
    let McpServerTransportConfig::Stdio { args, .. } = conversation.transport else {
        panic!("conversation transport");
    };
    assert_eq!(args.last().map(String::as_str), Some("second-token"));
    assert!(
        secrets
            .reads
            .lock()
            .expect("read bindings")
            .iter()
            .all(|(_, _, _, binding)| matches!(
                binding,
                VulcanPluginCredentialBinding::Revision(_)
            ))
    );
    secrets.labels.remove(&second);
    secrets.reads.lock().expect("reset reads").clear();
    assert!(
        plugin_mcp_server_configs(&plugins, &secrets)
            .expect_err("missing selected revision")
            .contains("Selected credential revision is missing")
    );
    // A missing selected value causes one failing lookup, despite readable old and legacy values.
    // 即使旧值及旧命名空间可读，缺失选中值也仅产生一次失败查找。
    let reads = secrets.reads.lock().expect("read failure lookup");
    assert_eq!(reads.len(), 1);
    assert!(
        reads.iter().all(|(_, _, _, binding)| *binding
            == VulcanPluginCredentialBinding::Revision(second.clone()))
    );
}
