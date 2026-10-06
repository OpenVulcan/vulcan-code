//! Credential edits stage a complete revision, select it through settings and republish the bound descriptor.
//! 凭据编辑暂存完整修订，经设置选定，并重新发布已绑定描述符。

use crate::{
    VulcanPluginConfigurationSelection, VulcanPluginCredentialBackend,
    VulcanPluginCredentialBinding, VulcanPluginCredentialKey, VulcanPluginCredentialRevision,
    VulcanPluginCredentialValues, VulcanPluginGenerationStore, VulcanPluginPackageSelection,
    VulcanPluginRuntimeManager,
};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;
use std::sync::Mutex;

/// In-memory secure backend keyed by the exact plugin, revision and slot; one slot may be made to fail writes.
/// 以精确插件、修订及槽位为键的内存安全后端；可令单个槽位写入失败。
#[derive(Default)]
struct MemoryBackend {
    /// Stored values per exact tuple.
    /// 每个精确元组的已存值。
    values: Mutex<
        BTreeMap<
            (
                String,
                VulcanPluginCredentialRevision,
                VulcanPluginCredentialKey,
            ),
            String,
        >,
    >,
    /// Secret key whose writes fail, if any.
    /// 写入失败的秘密键（如有）。
    fail_write: Option<&'static str>,
}

impl MemoryBackend {
    /// Return the stored value of one exact tuple.
    /// 返回单个精确元组的已存值。
    fn value(
        &self,
        plugin_id: &str,
        revision: &VulcanPluginCredentialRevision,
        key: &VulcanPluginCredentialKey,
    ) -> Option<String> {
        self.values
            .lock()
            .expect("backend lock")
            .get(&(plugin_id.to_owned(), revision.clone(), key.clone()))
            .cloned()
    }

    /// Return every revision with at least one stored value.
    /// 返回至少存有一个值的每个修订。
    fn revisions(&self) -> BTreeSet<VulcanPluginCredentialRevision> {
        self.values
            .lock()
            .expect("backend lock")
            .keys()
            .map(|(_, revision, _)| revision.clone())
            .collect()
    }
}

impl VulcanPluginCredentialBackend for MemoryBackend {
    fn read(
        &self,
        plugin_id: &str,
        revision: &VulcanPluginCredentialRevision,
        key: &VulcanPluginCredentialKey,
    ) -> Result<Option<String>, String> {
        Ok(self.value(plugin_id, revision, key))
    }

    fn write(
        &self,
        plugin_id: &str,
        revision: &VulcanPluginCredentialRevision,
        key: &VulcanPluginCredentialKey,
        value: &str,
    ) -> Result<(), String> {
        if self.fail_write == Some(key.secret_key.as_str()) {
            return Err(String::from("fixture write failure"));
        }
        self.values.lock().expect("backend lock").insert(
            (plugin_id.to_owned(), revision.clone(), key.clone()),
            value.to_owned(),
        );
        Ok(())
    }

    fn delete(
        &self,
        plugin_id: &str,
        revision: &VulcanPluginCredentialRevision,
        key: &VulcanPluginCredentialKey,
    ) -> Result<(), String> {
        self.values.lock().expect("backend lock").remove(&(
            plugin_id.to_owned(),
            revision.clone(),
            key.clone(),
        ));
        Ok(())
    }
}

/// MCP declaration with two secret headers, shared by the bundled and managed fixtures.
/// 带两个秘密请求头的 MCP 声明，由内建及受管夹具共用。
const MCP_SERVERS: &str = "mcp:\n  servers:\n    - id: remote\n      name: Remote\n      transport:\n        type: http\n        url: {source: literal, value: 'https://example.test/mcp'}\n        headers:\n          Authorization: {source: secret, key: token}\n          X-Region: {source: secret, key: region}\n";

/// Write a package `id` with a required-label schema, defaults and secret-bearing MCP server into `dir`.
/// 将带必需标签 Schema、默认值及携带秘密 MCP 服务器的插件包 `id` 写入 `dir`。
fn write_package(dir: &Path, id: &str) {
    fs::create_dir_all(dir).expect("create package");
    fs::write(dir.join("plugin.yaml"), format!("schema_version: 1\nid: {id}\nversion: 1.0.0\ndefault_enabled: true\ndisplay:\n  name: Credential fixture\n  capability: Credential edits\npermissions: [config, network_https, secrets_read]\nconfig:\n  schema_file: schema.json\n  defaults_file: defaults.json\n{MCP_SERVERS}"))
        .expect("write manifest");
    fs::write(dir.join("schema.json"), r#"{"type":"object","properties":{"label":{"type":"string"}},"required":["label"],"additionalProperties":false}"#)
        .expect("write schema");
    fs::write(dir.join("defaults.json"), r#"{"label":"default"}"#).expect("write defaults");
}

/// Create and refresh a runtime-free manager over `root`.
/// 在 `root` 上创建并刷新不含运行时的管理器。
fn manager(root: &Path) -> VulcanPluginRuntimeManager {
    let manager = VulcanPluginRuntimeManager::without_runtime(
        root.join("bundled"),
        root.join("user"),
        root.join("bridge"),
        root.join("distributions"),
        root.join("state"),
        root.join("runtime"),
    );
    manager
        .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
        .expect("refresh catalog");
    manager
}

/// Declared slot `secret_key` of the fixture server.
/// 夹具服务器的声明槽位 `secret_key`。
fn key(secret_key: &str) -> VulcanPluginCredentialKey {
    VulcanPluginCredentialKey {
        server_id: String::from("remote"),
        secret_key: secret_key.to_owned(),
    }
}

/// Partial edit naming the given slots.
/// 命名给定槽位的部分编辑。
fn changes(entries: &[(&str, Option<&str>)]) -> VulcanPluginCredentialValues {
    entries
        .iter()
        .map(|(secret_key, value)| (key(secret_key), value.map(str::to_owned)))
        .collect()
}

/// Binding of the published `plugin_id:remote` server.
/// 已发布 `plugin_id:remote` 服务器的绑定。
fn server_binding(
    runtime: &VulcanPluginRuntimeManager,
    plugin_id: &str,
) -> VulcanPluginCredentialBinding {
    runtime.snapshot().plugins[plugin_id].mcp_servers[0]
        .credential_binding
        .clone()
}

/// Prepare and commit one edit; return the committed revision.
/// 准备并提交单个编辑；返回已提交修订。
fn set_credentials(
    runtime: &VulcanPluginRuntimeManager,
    plugin_id: &str,
    changes: &VulcanPluginCredentialValues,
    backend: &MemoryBackend,
) -> Result<VulcanPluginCredentialRevision, crate::VulcanPluginRuntimeError> {
    let edit = runtime.prepare_plugin_credentials(plugin_id, changes, backend)?;
    let revision = edit.revision().clone();
    runtime.commit_plugin_credentials(edit, backend)?;
    Ok(revision)
}

/// A bundled plugin starts unconfigured; its first secret edit creates a record that still follows the defaults.
/// 内建插件起初未配置；首次秘密编辑创建仍跟随默认值的记录。
#[test]
fn bundled_secret_edit_creates_record_following_package_defaults() {
    let root = tempfile::tempdir().expect("create bundled fixture");
    write_package(
        &root.path().join("bundled/bundled-plugin"),
        "bundled-plugin",
    );
    let runtime = manager(root.path());
    let backend = MemoryBackend::default();
    assert_eq!(
        server_binding(&runtime, "bundled-plugin"),
        VulcanPluginCredentialBinding::Unconfigured
    );

    let revision = set_credentials(
        &runtime,
        "bundled-plugin",
        &changes(&[("token", Some("t1"))]),
        &backend,
    )
    .expect("set bundled secret");

    let record = VulcanPluginGenerationStore::open_existing(&root.path().join("user"))
        .expect("open store")
        .expect("edit created the store")
        .activation("bundled-plugin")
        .expect("read record")
        .expect("edit created the record");
    assert_eq!(record.package(), &VulcanPluginPackageSelection::Bundled {});
    assert_eq!(
        record.configuration(),
        &VulcanPluginConfigurationSelection::PackageDefaults {}
    );
    assert_eq!(record.credential_revision(), Some(&revision));
    assert_eq!(
        server_binding(&runtime, "bundled-plugin"),
        VulcanPluginCredentialBinding::Revision(revision.clone())
    );
    assert_eq!(
        backend
            .value("bundled-plugin", &revision, &key("token"))
            .as_deref(),
        Some("t1")
    );
    assert_eq!(
        backend.value("bundled-plugin", &revision, &key("region")),
        None
    );
    assert_eq!(
        runtime
            .plugin_config("bundled-plugin")
            .expect("read defaults"),
        json!({"label": "default"})
    );

    // A fresh catalog binds the same revision from the record.
    // 新目录从记录绑定同一修订。
    runtime
        .refresh_catalog(&BTreeSet::new(), &BTreeSet::new())
        .expect("refresh catalog");
    assert_eq!(
        server_binding(&runtime, "bundled-plugin"),
        VulcanPluginCredentialBinding::Revision(revision)
    );
}

/// Unnamed slots keep their values, None clears a slot, configuration edits keep the credentials,
/// and superseded revisions are collected once unreachable.
/// 未命名槽位保留其值，None 清除槽位，配置编辑保留凭据，被取代修订在不可达后被回收。
#[test]
fn secret_edits_merge_slots_and_collect_superseded_revisions() {
    let root = tempfile::tempdir().expect("create merge fixture");
    write_package(
        &root.path().join("bundled/bundled-plugin"),
        "bundled-plugin",
    );
    let runtime = manager(root.path());
    let backend = MemoryBackend::default();

    let first = set_credentials(
        &runtime,
        "bundled-plugin",
        &changes(&[("token", Some("t1")), ("region", Some("eu"))]),
        &backend,
    )
    .expect("set both secrets");
    let second = set_credentials(
        &runtime,
        "bundled-plugin",
        &changes(&[("token", None)]),
        &backend,
    )
    .expect("clear token");
    assert_eq!(
        backend.value("bundled-plugin", &second, &key("token")),
        None
    );
    assert_eq!(
        backend
            .value("bundled-plugin", &second, &key("region"))
            .as_deref(),
        Some("eu")
    );

    runtime
        .update_plugin_config("bundled-plugin", json!({"label": "edited"}))
        .expect("edit configuration");
    assert_eq!(
        server_binding(&runtime, "bundled-plugin"),
        VulcanPluginCredentialBinding::Revision(second.clone())
    );

    assert_eq!(
        runtime
            .collect_unreachable_plugin_credentials("bundled-plugin", &backend)
            .expect("collect superseded"),
        vec![first]
    );
    assert_eq!(backend.revisions(), BTreeSet::from([second]));
}

/// Undeclared slots, empty values and failed secure writes leave the settings and the backend unchanged.
/// 未声明槽位、空值及安全写入失败使设置及后端保持不变。
#[test]
fn rejected_secret_edits_change_nothing() {
    let root = tempfile::tempdir().expect("create rejection fixture");
    write_package(
        &root.path().join("bundled/bundled-plugin"),
        "bundled-plugin",
    );
    let runtime = manager(root.path());
    let failing = MemoryBackend {
        fail_write: Some("region"),
        ..MemoryBackend::default()
    };

    for edit in [
        changes(&[("undeclared", Some("x"))]),
        changes(&[("token", Some(""))]),
    ] {
        set_credentials(&runtime, "bundled-plugin", &edit, &failing).expect_err("invalid edit");
    }
    set_credentials(
        &runtime,
        "bundled-plugin",
        &changes(&[("token", Some("t1")), ("region", Some("eu"))]),
        &failing,
    )
    .expect_err("failed secure write");

    let store =
        VulcanPluginGenerationStore::open_existing(&root.path().join("user")).expect("open store");
    assert!(
        store
            .as_ref()
            .map(|store| store.activation("bundled-plugin").expect("read record"))
            .is_none_or(|record| record.is_none()),
        "rejected edits must not create settings"
    );
    assert!(
        failing.revisions().is_empty(),
        "the candidate must be discarded"
    );
    assert_eq!(
        server_binding(&runtime, "bundled-plugin"),
        VulcanPluginCredentialBinding::Unconfigured
    );
}

/// A managed plugin's edit commits through its settings and republishes the descriptor and enabled servers.
/// 受管插件的编辑经其设置提交，并重新发布描述符及已启用服务器。
#[test]
fn managed_secret_edit_republishes_descriptor_and_keeps_configuration() {
    let root = tempfile::tempdir().expect("create managed fixture");
    let source = root.path().join("source/managed-plugin");
    write_package(&source, "managed-plugin");
    fs::create_dir_all(root.path().join("bundled")).expect("create bundled root");
    let store = VulcanPluginGenerationStore::open(&{
        fs::create_dir_all(root.path().join("user")).expect("create user root");
        root.path().join("user")
    })
    .expect("open store");
    let inspected = crate::inspect_user_plugin_package(&source).expect("inspect package");
    let approval = crate::runtime::plugin_required_permissions(&inspected).expect("approval");
    let package = store
        .prepare(&source, &inspected.content_fingerprint)
        .expect("prepare package");
    let backend = MemoryBackend::default();
    let initial = VulcanPluginCredentialRevision::allocate();
    let values = changes(&[("token", Some("t0")), ("region", None)]);
    store
        .begin_credential_staging(&package, initial.clone(), &values)
        .expect("stage initial")
        .write_and_finish(&backend, &values)
        .expect("finish initial");
    store
        .activate_configured(
            &package,
            0,
            approval,
            initial.clone(),
            VulcanPluginConfigurationSelection::Document {
                value: json!({"label": "original"}),
            },
        )
        .expect("activate package");
    let runtime = manager(root.path());

    let revision = set_credentials(
        &runtime,
        "managed-plugin",
        &changes(&[("region", Some("us"))]),
        &backend,
    )
    .expect("set managed secret");

    let snapshot = runtime.snapshot();
    let plugin = &snapshot.plugins["managed-plugin"];
    let bound = VulcanPluginCredentialBinding::Revision(revision.clone());
    assert_eq!(
        plugin
            .managed_package()
            .expect("managed descriptor")
            .credential_binding(),
        &bound
    );
    assert_eq!(plugin.mcp_servers[0].credential_binding, bound);
    if let Some(enabled) = snapshot.enabled_mcp_servers.get("managed-plugin:remote") {
        assert_eq!(enabled.credential_binding, bound);
    }
    assert_eq!(
        backend
            .value("managed-plugin", &revision, &key("token"))
            .as_deref(),
        Some("t0")
    );
    assert_eq!(
        backend
            .value("managed-plugin", &revision, &key("region"))
            .as_deref(),
        Some("us")
    );
    assert_eq!(
        runtime
            .plugin_config("managed-plugin")
            .expect("read configuration"),
        json!({"label": "original"})
    );
    let record = store
        .activation("managed-plugin")
        .expect("read record")
        .expect("managed record");
    assert_eq!(record.credential_revision(), Some(&revision));
    drop(snapshot);

    assert_eq!(
        runtime
            .collect_unreachable_plugin_credentials("managed-plugin", &backend)
            .expect("collect superseded"),
        vec![initial]
    );
}

/// A prepared edit selects nothing, refuses a second edit of the plugin, keeps collection away and is discarded cleanly.
/// 已准备编辑不选定任何内容，拒绝该插件的第二个编辑，使回收远离，并可被干净地丢弃。
#[test]
fn prepared_secret_edit_is_exclusive_until_discarded() {
    let root = tempfile::tempdir().expect("create prepared fixture");
    write_package(
        &root.path().join("bundled/bundled-plugin"),
        "bundled-plugin",
    );
    let runtime = manager(root.path());
    let backend = MemoryBackend::default();
    let selected = set_credentials(
        &runtime,
        "bundled-plugin",
        &changes(&[("token", Some("t1"))]),
        &backend,
    )
    .expect("select first revision");

    let edit = runtime
        .prepare_plugin_credentials(
            "bundled-plugin",
            &changes(&[("token", Some("t2"))]),
            &backend,
        )
        .expect("prepare edit");
    let candidate = edit.revision().clone();
    assert_eq!(
        edit.candidate().mcp_servers[0].credential_binding,
        VulcanPluginCredentialBinding::Revision(candidate.clone())
    );
    assert_eq!(
        server_binding(&runtime, "bundled-plugin"),
        VulcanPluginCredentialBinding::Revision(selected.clone()),
        "a prepared edit must not change the published binding"
    );
    runtime
        .prepare_plugin_credentials("bundled-plugin", &changes(&[("token", None)]), &backend)
        .err()
        .expect("a second edit of the same plugin is refused");
    assert!(
        runtime
            .collect_unreachable_plugin_credentials("bundled-plugin", &backend)
            .expect("collect during edit")
            .is_empty()
    );

    runtime
        .discard_plugin_credentials(edit, &backend)
        .expect("discard edit");
    assert_eq!(backend.revisions(), BTreeSet::from([selected.clone()]));
    assert_eq!(
        server_binding(&runtime, "bundled-plugin"),
        VulcanPluginCredentialBinding::Revision(selected)
    );
    runtime
        .prepare_plugin_credentials("bundled-plugin", &changes(&[("token", None)]), &backend)
        .expect("the plugin can be edited again after discard");
}
