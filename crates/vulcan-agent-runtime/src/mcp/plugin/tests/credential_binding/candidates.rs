//! Candidate configuration materialization uses real request, credential and package ownership.
//! 候选配置物化使用真实请求、凭据及插件包所有权。

use super::*;
use crate::mcp::candidate_global_mcp_server_configs;
use vulcan_plugin_runtime::{VulcanPluginConfigurationSelection, VulcanPluginStoreErrorCode};

pub(in crate::mcp) mod runtime;

/// Every candidate transport reads its selected values, keeps the request lock, and fails without revision fallback.
/// 每个候选传输读取选定值、保留请求锁，并在失败时禁止修订回退。
/// No inputs or return value; uses production preparation and all three global transport materializers.
/// 无输入与返回值；使用生产准备及三种全局传输物化器。
#[test]
fn candidate_mcp_configs_keep_frozen_values_and_original_request() {
    // Both the published and candidate selections belong to this isolated actual store.
    // 已发布与候选选择均属于此隔离实际存储。
    let temp = tempfile::tempdir().expect("candidate materialization fixture");
    let bundled = temp.path().join("bundled");
    let user = temp.path().join("user");
    fs::create_dir(&bundled).expect("bundled root");
    fs::create_dir(&user).expect("user root");
    let plugins = VulcanPluginRuntimeManager::without_runtime(
        bundled,
        user.clone(),
        temp.path().join("bridge"),
        temp.path().join("managed"),
        temp.path().join("state"),
        temp.path().join("runtime"),
    );
    // Extend the authoritative all-transport fixture with one declared configuration value.
    // 为权威全传输夹具增加一个已声明配置值。
    let source = package(&temp.path().join("source"));
    let manifest_path = source.join("plugin.yaml");
    let manifest = fs::read_to_string(&manifest_path).expect("manifest")
        .replace("permissions: [network_https,", "permissions: [config, network_https,")
        .replace("mcp:\n", "config:\n  schema_file: schema.json\n  defaults_file: defaults.json\nmcp:\n")
        .replace("args: [{source: secret, key: token}]", "args: [{source: config, key: label}, {source: secret, key: token}]")
        .replace("Authorization: {source: secret, key: token}", "Authorization: {source: secret, key: token}\n          X-Candidate-Label: {source: config, key: label}");
    fs::write(manifest_path, manifest).expect("configured manifest");
    fs::write(source.join("schema.json"),
        r#"{"type":"object","properties":{"label":{"type":"string"}},"required":["label"],"additionalProperties":false}"#,
    ).expect("schema");
    fs::write(
        source.join("defaults.json"),
        r#"{"label":"package-default"}"#,
    )
    .expect("defaults");
    let preview = plugins
        .inspect_local_user_plugin(&source)
        .expect("exact preview");
    let store = VulcanPluginGenerationStore::open(&user).expect("store");
    let generation = store
        .prepare(&source, &preview.content_fingerprint)
        .expect("immutable package");
    // Separate staged revisions make accidental latest-state lookups observable.
    // 独立暂存修订使意外读取最新状态可被观察。
    let first = VulcanPluginCredentialRevision::allocate();
    let second = VulcanPluginCredentialRevision::allocate();
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
            first,
            VulcanPluginConfigurationSelection::Document {
                value: serde_json::json!({"label":"published-value"}),
            },
        )
        .expect("publish predecessor");
    plugins
        .refresh_catalog(&Default::default(), &Default::default())
        .expect("public catalog");
    let original = store
        .activation("bound-plugin")
        .expect("original selection");
    let mut transaction = store
        .begin_activation_transaction(
            uuid::Uuid::new_v4(),
            &generation,
            1,
            preview.permissions,
            second.clone(),
            VulcanPluginConfigurationSelection::Document {
                value: serde_json::json!({"label":"private-candidate-value"}),
            },
        )
        .expect("candidate request");
    // Preparation consumes the inspected target without altering ordinary public routing.
    // 准备消费已检查目标，不改变普通公开路由。
    let prepared = {
        let candidate = store
            .inspect_activation_candidate(&transaction)
            .expect("inspect");
        plugins
            .prepare_mcp_activation_candidate(&candidate)
            .expect("candidate preparation")
    };
    let configs = candidate_global_mcp_server_configs(prepared.clone(), &secrets)
        .expect("materialize candidate values");
    assert_eq!(
        configs.candidate().request_id(),
        transaction.record().request_id()
    );
    assert_eq!(configs.servers().len(), 3);
    // The declared conversation browser endpoint still requires its actual owner grant during local validation.
    // 声明的会话浏览器端点在本地校验期间仍要求实际所有者授权。
    assert!(
        crate::mcp::candidate_local_mcp_server_configs(prepared.clone(), &secrets)
            .expect_err("local checks must not invent conversation authorization")
            .contains("Core browser endpoint requires a conversation grant.")
    );
    assert_values(configs.servers(), "second");
    for config in configs.servers() {
        assert!(
            matches!(&config.source, McpServerSource::Plugin { plugin_id, generation }
            if plugin_id == "bound-plugin" && *generation == 0)
        );
        match &config.transport {
            McpServerTransportConfig::Stdio { args, .. } => {
                assert_eq!(
                    args.first().map(String::as_str),
                    Some("private-candidate-value")
                );
            }
            McpServerTransportConfig::Http { headers, .. }
            | McpServerTransportConfig::Sse { headers, .. } => {
                assert_eq!(
                    headers.get("X-Candidate-Label").map(String::as_str),
                    Some("private-candidate-value")
                );
            }
        }
    }
    // Diagnostics cannot expose the prepared arguments, secret values or selected configuration.
    // 诊断不能暴露准备参数、秘密值或选定配置。
    let diagnostic = format!("{configs:?}");
    assert!(!diagnostic.contains("private-candidate-value"));
    assert!(!diagnostic.contains("second-token"));
    assert_values(
        &plugin_mcp_server_configs(&plugins, &secrets).expect("unchanged public values"),
        "first",
    );
    assert_eq!(
        store
            .activation("bound-plugin")
            .expect("unchanged selection"),
        original
    );
    assert_eq!(
        store
            .commit_activation_transaction(&mut transaction)
            .expect_err("prepared configs retain the actual request lock")
            .code,
        VulcanPluginStoreErrorCode::Busy
    );
    // A readable predecessor must not rescue a missing candidate credential revision.
    // 可读前驱不得补救缺失的候选凭据修订。
    secrets.labels.remove(&second);
    secrets.reads.lock().expect("reset lookups").clear();
    assert!(
        candidate_global_mcp_server_configs(prepared, &secrets)
            .expect_err("missing candidate revision")
            .contains("Selected credential revision is missing")
    );
    {
        let reads = secrets.reads.lock().expect("exact failed lookup");
        assert_eq!(reads.len(), 1);
        assert!(reads.iter().all(|(_, _, _, binding)| *binding
            == VulcanPluginCredentialBinding::Revision(second.clone())));
    }
    drop(configs);
    store
        .abort_activation_transaction(&mut transaction)
        .expect("failed materialization and released configs leak no request owner");
}
