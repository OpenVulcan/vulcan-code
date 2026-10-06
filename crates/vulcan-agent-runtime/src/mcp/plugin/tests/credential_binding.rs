//! Real managed catalogs feed frozen credential bindings into every MCP transport.
//! 真实受管目录将固定凭据绑定传递至每种 MCP 传输。

use super::*;
use std::sync::Mutex;
use vulcan_plugin_runtime::{
    PinnedVulcanPluginGeneration, VulcanPluginCredentialBackend, VulcanPluginCredentialKey,
    VulcanPluginCredentialValues, VulcanPluginPermissionPreview,
};

pub(in crate::mcp) mod candidates;
mod published;

/// Verify managed configuration never mixes a retained remote declaration with a newer selection.
/// 验证受管配置绝不将保留的远端声明与更新选择混用。
#[test]
fn configuration_selection_mcp_retains_exact_document_after_same_package_update() {
    // The real manager and immutable store are isolated in this temporary root.
    // 实际管理器及不可变存储隔离在此临时根目录中。
    let temp = tempfile::tempdir().expect("create MCP configuration selection fixture");
    // Production discovery needs separate bundled and user roots.
    // 生产发现需要独立的内置及用户根目录。
    let bundled = temp.path().join("bundled");
    fs::create_dir(&bundled).expect("create empty configuration bundled root");
    // The real user root owns the activation record.
    // 实际用户根目录持有激活记录。
    let user = temp.path().join("user");
    fs::create_dir(&user).expect("create configuration user root");
    // A Lua-free production manager still performs normal configuration and MCP materialization.
    // 无 Lua 的生产管理器仍执行正常配置及 MCP 物化。
    let plugins = VulcanPluginRuntimeManager::without_runtime(
        bundled,
        user.clone(),
        temp.path().join("bridge"),
        temp.path().join("managed"),
        temp.path().join("state"),
        temp.path().join("runtime"),
    );
    // This actual package declares configuration-backed HTTP and SSE fields without secrets.
    // 此实际插件包声明基于配置的 HTTP 及 SSE 字段，不使用秘密。
    let source = temp.path().join("source/configured-mcp");
    fs::create_dir_all(&source).expect("create configured MCP source");
    fs::write(
        source.join("plugin.yaml"),
        r#"schema_version: 1
id: configured-mcp
version: 1.0.0
default_enabled: true
display:
  name: Configured MCP
  capability: Exact selected configuration
permissions: [network_https, config]
config:
  schema_file: schema.json
  defaults_file: defaults.json
mcp:
  servers:
    - id: http
      name: HTTP
      transport:
        type: http
        url: {source: config, key: endpoint}
        headers:
          X-Label: {source: config, key: label}
    - id: sse
      name: SSE
      transport:
        type: sse
        url: {source: config, key: endpoint}
        headers:
          X-Label: {source: config, key: label}
"#,
    )
    .expect("write configured MCP manifest");
    fs::write(source.join("schema.json"), r#"{"type":"object","properties":{"endpoint":{"type":"string"},"label":{"type":"string"}},"required":["endpoint","label"],"additionalProperties":false}"#)
        .expect("write configured MCP schema");
    fs::write(
        source.join("defaults.json"),
        r#"{"endpoint":"https://default.test/mcp","label":"default"}"#,
    )
    .expect("write configured MCP defaults");
    // Preview supplies the exact production fingerprint and complete capability approval.
    // 预览提供精确生产指纹及完整能力批准。
    let preview = plugins
        .inspect_local_user_plugin(&source)
        .expect("inspect configured MCP package");
    // Package and credentials remain constant while the selected configuration changes.
    // 选定配置变化时，插件包及凭据保持不变。
    let store = VulcanPluginGenerationStore::open(&user).expect("open configured MCP store");
    // Physical package identity is shared by both selections.
    // 两次选择共用物理插件包身份。
    let generation = store
        .prepare(&source, &preview.content_fingerprint)
        .expect("prepare configured MCP package");
    // A genuinely empty secret inventory still has an exact staged revision.
    // 确实为空的秘密清单仍具有精确已暂存修订。
    let revision = VulcanPluginCredentialRevision::allocate();
    // No secure lookup should occur because both servers use configuration-only sources.
    // 两个服务器仅使用配置来源，因此不应发生任何安全查找。
    let secrets = RevisionSecrets {
        labels: BTreeMap::new(),
        reads: Mutex::new(Vec::new()),
    };
    stage_fixture_credentials(&store, &generation, &preview, &revision, &secrets);
    // The original document is independent of package defaults.
    // 原文档独立于包默认值。
    let first = serde_json::json!({"endpoint":"https://first.test/mcp","label":"first"});
    store
        .activate_configured(
            &generation,
            0,
            preview.permissions.clone(),
            revision.clone(),
            vulcan_plugin_runtime::VulcanPluginConfigurationSelection::Document {
                value: first.clone(),
            },
        )
        .expect("select first MCP configuration");
    // Detached descriptors must retain the original configuration after their catalog is dropped.
    // 所属目录释放后，独立描述符必须保留原配置。
    let original = plugins
        .refresh_catalog(&Default::default(), &Default::default())
        .expect("publish first configured catalog");
    // Only the server declarations are retained across the update.
    // 更新期间仅保留服务器声明。
    let servers = original.plugins["configured-mcp"].mcp_servers.clone();
    assert_eq!(
        plugins
            .plugin_config("configured-mcp")
            .expect("read selected manager config"),
        first
    );
    // The successor selection changes configuration and activation revision atomically.
    // 后继选择原子改变配置及激活修订。
    let second = serde_json::json!({"endpoint":"https://second.test/mcp","label":"second"});
    store
        .activate_configured(
            &generation,
            1,
            preview.permissions,
            revision,
            vulcan_plugin_runtime::VulcanPluginConfigurationSelection::Document {
                value: second.clone(),
            },
        )
        .expect("select second MCP configuration");
    plugins
        .refresh_catalog(&Default::default(), &Default::default())
        .expect("publish second configured catalog");
    drop(original);
    assert_eq!(
        plugins
            .plugin_config("configured-mcp")
            .expect("read second manager config"),
        second
    );
    for (configs, expected) in [
        (
            plugin_mcp_server_configs_for_descriptors(&plugins, &servers, &secrets)
                .expect("materialize detached old config"),
            "first",
        ),
        (
            plugin_mcp_server_configs(&plugins, &secrets).expect("materialize current config"),
            "second",
        ),
    ] {
        assert_eq!(configs.len(), 2);
        for config in configs {
            match config.transport {
                McpServerTransportConfig::Http { url, headers, .. }
                | McpServerTransportConfig::Sse { url, headers, .. } => {
                    assert_eq!(url, format!("https://{expected}.test/mcp"));
                    assert_eq!(headers.get("X-Label").map(String::as_str), Some(expected));
                }
                McpServerTransportConfig::Stdio { .. } => {
                    panic!("expected configured remote transports")
                }
            }
        }
    }
    assert!(
        secrets
            .reads
            .lock()
            .expect("inspect configuration-only lookups")
            .is_empty()
    );
}
use vulcan_plugin_runtime::{VulcanPluginCredentialRevision, VulcanPluginGenerationStore};

impl VulcanPluginCredentialBackend for RevisionSecrets {
    /// Read this fixture's already-populated revision through the same exact namespace used by MCP.
    /// 经 MCP 使用的相同精确命名空间，读取此夹具已填充修订。
    fn read(
        &self,
        plugin: &str,
        revision: &VulcanPluginCredentialRevision,
        key: &VulcanPluginCredentialKey,
    ) -> Result<Option<String>, String> {
        self.read_secret(
            plugin,
            &key.server_id,
            &key.secret_key,
            &VulcanPluginCredentialBinding::Revision(revision.clone()),
        )
        .map(Some)
    }
    /// Reject unexpected writes; this fixture deliberately starts with complete matching values.
    /// 拒绝非预期写入；此夹具刻意以完整匹配值开始。
    fn write(
        &self,
        _: &str,
        _: &VulcanPluginCredentialRevision,
        _: &VulcanPluginCredentialKey,
        _: &str,
    ) -> Result<(), String> {
        Err("Unexpected credential fixture overwrite.".into())
    }
    /// Reject deletion because these selected fixture revisions remain retained throughout the test.
    /// 拒绝删除，因为这些选定夹具修订在测试全程保留。
    fn delete(
        &self,
        _: &str,
        _: &VulcanPluginCredentialRevision,
        _: &VulcanPluginCredentialKey,
    ) -> Result<(), String> {
        Err("Unexpected credential fixture deletion.".into())
    }
}

/// Verify the fixture's populated values and persist the real staging checkpoint for the exact preview and package.
/// 验证夹具已填充值，并为精确预览及插件包持久保存真实暂存检查点。
fn stage_fixture_credentials(
    store: &VulcanPluginGenerationStore,
    generation: &PinnedVulcanPluginGeneration,
    preview: &VulcanPluginPermissionPreview,
    revision: &VulcanPluginCredentialRevision,
    backend: &RevisionSecrets,
) {
    // Every input key comes from the production permission preview rather than a parallel fixture inventory.
    // 每个输入键均来自生产权限预览，而非平行夹具清单。
    let values: VulcanPluginCredentialValues = preview
        .mcp_servers
        .iter()
        .flat_map(|server| {
            server
                .secret_keys
                .iter()
                .map(|secret_key| VulcanPluginCredentialKey {
                    server_id: server.server_id.clone(),
                    secret_key: secret_key.clone(),
                })
        })
        .map(|key| {
            // The same backend provides the expected fixture value and the subsequent staging readback.
            // 相同后端提供预期夹具值及后续暂存回读。
            let value =
                VulcanPluginCredentialBackend::read(backend, &preview.plugin_id, revision, &key)
                    .expect("read prepopulated revision fixture value");
            (key, value)
        })
        .collect();
    store
        .begin_credential_staging(generation, revision.clone(), &values)
        .expect("prepare real MCP credential inventory")
        .write_and_finish(backend, &values)
        .expect("verify prepopulated MCP credential revision");
}

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
    /// Unknown revisions fail even while other revisions remain readable.
    /// 即使其他修订可读，未知修订仍失败。
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
        let label = match binding {
            VulcanPluginCredentialBinding::Revision(revision) => self
                .labels
                .get(revision)
                .copied()
                .ok_or("Selected credential revision is missing.")?,
            VulcanPluginCredentialBinding::Unconfigured => {
                return Err(String::from("Plugin credentials are not configured."));
            }
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
    stage_fixture_credentials(&store, &generation, &preview, &first, &secrets);
    stage_fixture_credentials(&store, &generation, &preview, &second, &secrets);
    store
        .activate_configured(
            &generation,
            0,
            preview.permissions.clone(),
            first.clone(),
            vulcan_plugin_runtime::VulcanPluginConfigurationSelection::NotDeclared {},
        )
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
        .activate_configured(
            &generation,
            1,
            preview.permissions,
            second.clone(),
            vulcan_plugin_runtime::VulcanPluginConfigurationSelection::NotDeclared {},
        )
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
    // A missing selected value causes one failing lookup, despite readable old values.
    // 即使旧值可读，缺失选中值也仅产生一次失败查找。
    let reads = secrets.reads.lock().expect("read failure lookup");
    assert_eq!(reads.len(), 1);
    assert!(
        reads.iter().all(|(_, _, _, binding)| *binding
            == VulcanPluginCredentialBinding::Revision(second.clone()))
    );
}
