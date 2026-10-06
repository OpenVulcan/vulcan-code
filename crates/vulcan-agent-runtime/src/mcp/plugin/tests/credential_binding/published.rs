//! Real immutable selections verify sealed published materialization before any native launch.
//! 真实不可变选择在任何原生启动前验证密封已发布物化。

use super::*;
use crate::mcp::published_global_mcp_server_configs;
use vulcan_plugin_runtime::VulcanPluginConfigurationSelection;

/// Original immutable package, selected revision and actual backend used by both published tests.
/// 两个已发布测试使用的原不可变插件包、选定修订及实际后端。
struct Fixture {
    /// The actual manager issues source authority from its real published snapshot.
    /// 实际管理器从其真实已发布快照签发来源权威。
    manager: VulcanPluginRuntimeManager,
    /// This original store can publish a different complete selection without changing frozen inputs.
    /// 此原存储可发布不同完整选择，不改变固定输入。
    store: VulcanPluginGenerationStore,
    /// Physical package identity is retained for a genuine successor activation.
    /// 保留物理插件包身份，供真实后继激活使用。
    generation: PinnedVulcanPluginGeneration,
    /// Complete actual approval remains fixed across the configuration-only successor.
    /// 仅配置变化的后继期间固定完整实际批准。
    permissions: Vec<String>,
    /// The successor has a distinct genuinely staged credential namespace.
    /// 后继拥有不同且真实暂存的凭据命名空间。
    successor_credentials: VulcanPluginCredentialRevision,
    /// Reads record the exact descriptor-owned revision rather than inferring current state.
    /// 读取记录精确描述符自有修订，不推断当前状态。
    secrets: RevisionSecrets,
    /// Temporary paths outlive every real resource owner above.
    /// 临时路径跨上方每个真实资源所有者存续。
    _temp: tempfile::TempDir,
}

/// Build an actual configured version-four publication with independent original and successor credentials.
/// 构建实际已配置版本四发布，使用独立原凭据及后继凭据。
/// global selects the real manifest declaration set; return source ownership without executing its test binary.
/// global 选择真实清单声明集合；返回来源所有权，不执行其测试二进制。
fn fixture(global: bool) -> Fixture {
    // Each fixture owns distinct real canonical roots and no external user state.
    // 每个夹具拥有不同真实规范根，不接触外部用户状态。
    let temp = tempfile::tempdir().expect("published materialization fixture");
    // The empty bundled root preserves the actual user namespace selection rule.
    // 空内置根保持实际用户命名空间选择规则。
    let bundled = temp.path().join("bundled");
    // The user root holds real durable activations and physical generation pins.
    // 用户根持有真实持久激活及物理代次固定。
    let user = temp.path().join("user");
    fs::create_dir(&bundled).expect("bundled root");
    fs::create_dir(&user).expect("user root");
    // Production discovery works without a Lua engine for these native declarations.
    // 对这些原生声明，生产发现在无 Lua 引擎时工作。
    let manager = VulcanPluginRuntimeManager::without_runtime(
        bundled,
        user.clone(),
        temp.path().join("bridge"),
        temp.path().join("managed"),
        temp.path().join("state"),
        temp.path().join("runtime"),
    );
    // Extend only known declared fields of the existing all-transport source before inspection.
    // 检查前，仅扩展既有全传输来源的已知声明字段。
    let source = package(&temp.path().join("source"));
    // This manifest text comes from the unique package fixture above, not a current runtime catalog.
    // 此清单文本来自上方唯一插件包夹具，不来自当前运行时目录。
    let manifest = fs::read_to_string(source.join("plugin.yaml"))
        .expect("actual package manifest")
        .replace("permissions: [network_https,", "permissions: [config, network_https,")
        .replace("mcp:\n", "config:\n  schema_file: schema.json\n  defaults_file: defaults.json\nmcp:\n")
        .replace("args: [{source: secret, key: token}]", "args: [{source: config, key: label}, {source: secret, key: token}]")
        .replace("Authorization: {source: secret, key: token}", "Authorization: {source: secret, key: token}\n          X-Published-Label: {source: config, key: label}");
    // Parsed fixture fields have their real manifest shape before any immutable copy or approval.
    // 任何不可变复制或批准之前，已解析夹具字段具有真实清单形状。
    let mut manifest: Value = serde_yaml::from_str(&manifest).expect("real declared manifest");
    if !global {
        // This known unique conversation declaration remains real even when global materialization is empty.
        // 即使全局物化为空，此已知唯一会话声明仍保持真实。
        let servers = manifest["mcp"]["servers"]
            .as_array_mut()
            .expect("actual declared servers");
        servers.retain(|server| server["id"].as_str() == Some("conversation"));
        assert_eq!(
            servers.len(),
            1,
            "the expected conversation fixture may have changed"
        );
    }
    fs::write(
        source.join("plugin.yaml"),
        serde_json::to_vec_pretty(&manifest).expect("actual manifest encoding"),
    )
    .expect("write exact manifest");
    fs::write(
        source.join("schema.json"),
        r#"{"type":"object","properties":{"label":{"type":"string"}},"required":["label"],"additionalProperties":false}"#,
    )
    .expect("actual configuration schema");
    fs::write(
        source.join("defaults.json"),
        r#"{"label":"package-default"}"#,
    )
    .expect("actual package defaults");
    // Inspection freezes the complete actual permission list and package fingerprint.
    // 检查固定完整实际权限列表及插件包指纹。
    let preview = manager
        .inspect_local_user_plugin(&source)
        .expect("actual inspection");
    // This production store creates the only package and credential ownership namespaces.
    // 此生产存储创建唯一插件包及凭据所有权命名空间。
    let store = VulcanPluginGenerationStore::open(&user).expect("actual store");
    // Immutable package preparation uses the inspected exact bytes.
    // 不可变插件包准备使用已检查精确字节。
    let generation = store
        .prepare(&source, &preview.content_fingerprint)
        .expect("actual package");
    // The original and successor namespaces are created independently before their selections.
    // 原命名空间及后继命名空间在其选择之前独立创建。
    let original_credentials = VulcanPluginCredentialRevision::allocate();
    // A distinct real revision reveals accidental newest-credential reads.
    // 不同真实修订揭示意外读取最新凭据。
    let successor_credentials = VulcanPluginCredentialRevision::allocate();
    // The revision-aware backend supplies real staging and records later materialization reads.
    // 识别修订的后端提供真实暂存，并记录后续物化读取。
    let secrets = RevisionSecrets {
        labels: BTreeMap::from([
            (original_credentials.clone(), "original"),
            (successor_credentials.clone(), "successor"),
        ]),
        reads: Mutex::new(Vec::new()),
    };
    stage_fixture_credentials(
        &store,
        &generation,
        &preview,
        &original_credentials,
        &secrets,
    );
    stage_fixture_credentials(
        &store,
        &generation,
        &preview,
        &successor_credentials,
        &secrets,
    );
    store
        .activate_configured(
            &generation,
            0,
            preview.permissions.clone(),
            original_credentials,
            VulcanPluginConfigurationSelection::Document {
                value: serde_json::json!({"label":"original-document"}),
            },
        )
        .expect("actual original selection");
    manager
        .refresh_catalog(&Default::default(), &Default::default())
        .expect("actual published catalog");
    secrets.reads.lock().expect("actual backend reads").clear();
    Fixture {
        manager,
        store,
        generation,
        permissions: preview.permissions,
        successor_credentials,
        secrets,
        _temp: temp,
    }
}

/// Frozen materialization survives a real successor publication and rejects another issuer before secret reads.
/// 固定物化跨真实后继发布存续，并在秘密读取之前拒绝另一签发者。
/// No inputs or return value; actual selections and backend reads are the evidence.
/// 无输入及返回值；实际选择及后端读取构成证据。
#[test]
fn published_materialization_keeps_original_config_credentials_and_route() {
    // Retain the exact original actual catalog independently of the later current selection.
    // 独立于后续当前选择，保留精确原实际目录。
    let fixture = fixture(true);
    // This actual registered host alone may borrow the original inputs.
    // 仅此实际已注册宿主可借用原输入。
    let authority = fixture.manager.register_native_mcp_host_adapter();
    // Retaining the original snapshot also makes later retrospective dispatch rejection observable.
    // 保留原快照也使后续追认派发拒绝可被观察。
    let snapshot = fixture.manager.snapshot();
    // Full owner contains real original package, credential and execution-environment files.
    // 完整所有者包含真实原插件包、凭据及执行环境文件。
    let prepared = fixture
        .manager
        .prepare_published_mcp_resources(&snapshot, "bound-plugin", &authority)
        .expect("original published owner");
    // A separate registration on the same manager must not be accepted as the original issuer.
    // 同一管理器的独立注册不得被接受为原签发者。
    let foreign = fixture.manager.register_native_mcp_host_adapter();
    assert!(
        published_global_mcp_server_configs(prepared.clone(), &foreign, &fixture.secrets).is_err()
    );
    assert!(
        fixture
            .secrets
            .reads
            .lock()
            .expect("pre-effect backend reads")
            .is_empty()
    );
    fixture
        .store
        .activate_configured(
            &fixture.generation,
            1,
            fixture.permissions.clone(),
            fixture.successor_credentials.clone(),
            VulcanPluginConfigurationSelection::Document {
                value: serde_json::json!({"label":"successor-document"}),
            },
        )
        .expect("real successor selection");
    fixture
        .manager
        .refresh_catalog(&Default::default(), &Default::default())
        .expect("real successor catalog");
    assert!(
        prepared
            .begin_dispatch(&fixture.manager, &authority)
            .is_err()
    );
    // Materialization is permitted to read frozen values; that does not certify a stale owner for new dispatch.
    // 物化可读取固定值；这不为过期所有者认证新派发。
    let configs = published_global_mcp_server_configs(prepared, &authority, &fixture.secrets)
        .expect("original frozen materialization");
    assert_eq!(
        configs.identity(),
        ("bound-plugin", snapshot.plugins["bound-plugin"].generation)
    );
    assert_eq!(configs.servers().len(), 3);
    assert_values(configs.servers(), "original");
    for server in configs.servers() {
        assert_eq!(
            server.source,
            McpServerSource::Plugin {
                plugin_id: String::from("bound-plugin"),
                generation: configs.identity().1
            }
        );
        match &server.transport {
            McpServerTransportConfig::Stdio { args, .. } => {
                assert_eq!(args.first().map(String::as_str), Some("original-document"))
            }
            McpServerTransportConfig::Http { headers, .. }
            | McpServerTransportConfig::Sse { headers, .. } => assert_eq!(
                headers.get("X-Published-Label").map(String::as_str),
                Some("original-document")
            ),
        }
    }
}

/// Empty global declarations still retain exact original route metadata without inventing a first server.
/// 空全局声明仍保留精确原路由元数据，不虚构首个服务器。
/// No inputs or return value; the real conversation-only manifest and actual owner supply the evidence.
/// 无输入及返回值；真实仅会话清单及实际所有者提供证据。
#[test]
fn published_materialization_empty_globals_use_original_owner_identity() {
    // This actual manifest declares only a conversation server, which global startup must not launch.
    // 此实际清单仅声明会话服务器，全局启动不得启动它。
    let fixture = fixture(false);
    // Source authority is registered once for this actual manager incarnation.
    // 为此实际管理器实例仅注册一次来源权威。
    let authority = fixture.manager.register_native_mcp_host_adapter();
    // The same actual immutable snapshot defines its route even without a global definition.
    // 即使没有全局定义，同一实际不可变快照也定义其路由。
    let snapshot = fixture.manager.snapshot();
    // Prepare through full production validation, including all declared original resources.
    // 通过完整生产校验准备，包括全部已声明原资源。
    let prepared = fixture
        .manager
        .prepare_published_mcp_resources(&snapshot, "bound-plugin", &authority)
        .expect("conversation-only published owner");
    // The sealed result must not read conversation credentials or substitute a browser grant.
    // 密封结果不得读取会话凭据，也不得替代浏览器授权。
    let configs = published_global_mcp_server_configs(prepared, &authority, &fixture.secrets)
        .expect("empty global materialization");
    assert!(configs.servers().is_empty());
    assert_eq!(
        configs.identity(),
        ("bound-plugin", snapshot.plugins["bound-plugin"].generation)
    );
    assert!(
        fixture
            .secrets
            .reads
            .lock()
            .expect("empty global reads")
            .is_empty()
    );
}
