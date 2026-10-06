//! Bundled configuration lives in a settings record created by the first edit; reset is the explicit recovery.
//! 内建配置位于首次编辑创建的设置记录中；重置是显式恢复手段。

use crate::{
    VulcanPluginGenerationStore, VulcanPluginPackageSelection, VulcanPluginRuntimeManager,
};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

/// Write one bundled package with a required-label schema and defaults beneath `root/bundled`.
/// 在 `root/bundled` 下写入一个带必需标签 Schema 及默认值的内建插件包。
fn write_bundled_package(root: &Path) {
    let source = root.join("bundled/bundled-plugin");
    fs::create_dir_all(&source).expect("create bundled package");
    fs::write(source.join("plugin.yaml"), "schema_version: 1\nid: bundled-plugin\nversion: 1.0.0\ndefault_enabled: true\ndisplay:\n  name: Bundled configuration fixture\n  capability: Bundled settings\npermissions: [config, network_https]\nconfig:\n  schema_file: schema.json\n  defaults_file: defaults.json\nmcp:\n  servers:\n    - id: remote\n      name: Remote\n      transport:\n        type: http\n        url: {source: literal, value: 'https://example.test/mcp'}\n")
        .expect("write bundled manifest");
    fs::write(source.join("schema.json"), r#"{"type":"object","properties":{"label":{"type":"string"}},"required":["label"],"additionalProperties":false}"#)
        .expect("write bundled schema");
    fs::write(source.join("defaults.json"), r#"{"label":"default"}"#)
        .expect("write bundled defaults");
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
        .expect("refresh bundled catalog");
    manager
}

/// Path of the bundled plugin's settings record inside the user root.
/// 用户根目录内内建插件设置记录的路径。
fn record_path(root: &Path) -> PathBuf {
    root.join("user/.vulcan-generations/active/bundled-plugin.json")
}

/// Reads return the package defaults without creating anything; the first edit creates the record and reset restores defaults.
/// 读取返回插件包默认值且不创建任何内容；首次编辑创建记录，重置恢复默认值。
#[test]
fn bundled_configuration_reads_defaults_until_edited_and_reset_restores_them() {
    let root = tempfile::tempdir().expect("create bundled fixture");
    write_bundled_package(root.path());
    let runtime = manager(root.path());

    assert_eq!(
        runtime
            .plugin_config("bundled-plugin")
            .expect("read defaults"),
        json!({"label": "default"})
    );
    assert!(
        !root.path().join("user").exists(),
        "reading configuration must not create the user root"
    );

    runtime
        .update_plugin_config("bundled-plugin", json!({"label": "edited"}))
        .expect("edit bundled configuration");
    assert_eq!(
        runtime.plugin_config("bundled-plugin").expect("read edit"),
        json!({"label": "edited"})
    );
    let store = VulcanPluginGenerationStore::open_existing(&root.path().join("user"))
        .expect("open created store")
        .expect("edit created the store");
    let created = store
        .activation("bundled-plugin")
        .expect("read settings record")
        .expect("edit created the settings record");
    assert_eq!(created.package(), &VulcanPluginPackageSelection::Bundled {});

    assert_eq!(
        runtime
            .reset_plugin_config("bundled-plugin")
            .expect("reset to defaults"),
        json!({"label": "default"})
    );
    let reset = store
        .activation("bundled-plugin")
        .expect("read reset record")
        .expect("reset keeps the settings record");
    assert_eq!(reset.settings().revision, created.settings().revision + 1);
    assert_eq!(
        runtime.plugin_config("bundled-plugin").expect("read reset"),
        json!({"label": "default"})
    );
    assert!(
        !root
            .path()
            .join("state/bundled-plugin/config.json")
            .exists()
    );
}

/// A stored value the schema no longer accepts fails every read, without being rewritten, until an explicit reset.
/// Schema 不再接受的已存值在显式重置前使每次读取失败，且不被改写。
#[test]
fn invalid_stored_bundled_configuration_fails_until_reset() {
    let root = tempfile::tempdir().expect("create invalid bundled fixture");
    write_bundled_package(root.path());
    let runtime = manager(root.path());
    runtime
        .update_plugin_config("bundled-plugin", json!({"label": "edited"}))
        .expect("create settings record");

    // Simulate an upgraded schema rejecting the stored document.
    // 模拟升级后 Schema 拒绝已存文档。
    let path = record_path(root.path());
    let mut record: Value = serde_json::from_slice(&fs::read(&path).expect("read settings record"))
        .expect("decode settings record");
    assert!(
        record["settings"]["configuration"]["value"].is_object(),
        "settings record format changed; the configuration value may have moved"
    );
    record["settings"]["configuration"]["value"] = json!({"label": 9});
    let invalid = serde_json::to_vec(&record).expect("encode invalid record");
    fs::write(&path, &invalid).expect("write invalid settings");

    runtime
        .plugin_config("bundled-plugin")
        .expect_err("invalid stored configuration");
    assert_eq!(
        fs::read(&path).expect("reread settings record"),
        invalid,
        "a failed read must not rewrite the stored settings"
    );

    runtime
        .reset_plugin_config("bundled-plugin")
        .expect("reset recovers invalid settings");
    assert_eq!(
        runtime
            .plugin_config("bundled-plugin")
            .expect("read recovered defaults"),
        json!({"label": "default"})
    );
}
