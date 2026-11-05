//! Metadata access and automatic upgrades through production storage readers.
//! 通过生产存储读取入口验证元数据访问与自动升级。

use super::*;
use vulcan_session_core::v1::config::{METADATA_SCHEMA_VERSION, METADATA_UPGRADE_BASELINE_VERSION};

/// Publishes one ordinary session and returns its storage plus the expected metadata.
/// 发布一个普通会话，返回其存储及预期元数据。
fn fixture() -> (tempfile::TempDir, SessionV1Layout, MetadataDocument) {
    let (root, layout) = fresh_layout("metadata-access");
    let mut document = SessionDocument::new(&layout.session_id, "project-1", "access", 1);
    submit_test_root(&mut document, "human", "Human work", 2);
    let metadata = publish_initial(&layout, &document);
    (root, layout, metadata)
}

/// Rewrites the published metadata with `schema` as its version and returns the exact bytes.
/// 将已发布元数据的版本改写为 `schema`，并返回精确字节。
fn write_schema(layout: &SessionV1Layout, metadata: &MetadataDocument, schema: u32) -> Vec<u8> {
    let mut raw = serde_json::to_value(metadata).unwrap();
    raw["schema_version"] = schema.into();
    let bytes = serde_json::to_vec(&raw).unwrap();
    std::fs::write(layout.metadata_path(), &bytes).unwrap();
    bytes
}

/// Lists the file names of one session directory for side-effect checks.
/// 列出一个会话目录中的文件名，用于检查副作用。
fn session_files(layout: &SessionV1Layout) -> BTreeSet<String> {
    std::fs::read_dir(layout.session_dir())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect()
}

/// The current schema reads directly, matches full loading, and creates no extra files.
/// 当前模式直接读取，与完整加载一致，且不产生额外文件。
#[test]
fn metadata_access_reads_current_schema_without_side_effects() {
    let (_root, layout, metadata) = fixture();
    let before = session_files(&layout);
    assert_eq!(read_metadata(&layout.metadata_path()).unwrap(), metadata);
    assert_eq!(load_session(&layout).unwrap().metadata, metadata);
    assert_eq!(session_files(&layout), before);
}

/// Schemas below the upgrade baseline and future schemas are rejected unchanged.
/// 低于升级基线的模式与未来模式原样拒绝。
#[test]
fn metadata_access_rejects_unsupported_schemas_unchanged() {
    for schema in [
        1,
        METADATA_UPGRADE_BASELINE_VERSION - 1,
        METADATA_SCHEMA_VERSION + 1,
    ] {
        let (_root, layout, metadata) = fixture();
        let original = write_schema(&layout, &metadata, schema);
        let files = session_files(&layout);
        let error = read_metadata(&layout.metadata_path()).unwrap_err();
        assert_eq!(
            error.code,
            ErrorCode::UnsupportedDevelopmentFormat,
            "{schema}"
        );
        assert!(load_session(&layout).is_err(), "{schema}");
        assert_eq!(
            std::fs::read(layout.metadata_path()).unwrap(),
            original,
            "{schema}"
        );
        assert_eq!(session_files(&layout), files, "{schema}");
    }
}

/// Returns the backup path the upgrade uses for one original schema.
/// 返回升级为某个原始模式使用的备份路径。
fn backup_path(layout: &SessionV1Layout, schema: u32) -> PathBuf {
    layout
        .session_dir()
        .join(format!("metadata.v{schema}.backup.json"))
}

/// Every supported older schema upgrades once on first read and keeps its original bytes.
/// 每个受支持的旧模式在首次读取时升级一次，并保留原字节。
#[test]
fn metadata_access_upgrades_supported_schemas_once_with_backup() {
    for schema in METADATA_UPGRADE_BASELINE_VERSION..METADATA_SCHEMA_VERSION {
        let (_root, layout, metadata) = fixture();
        let original = write_schema(&layout, &metadata, schema);

        assert_eq!(read_metadata(&layout.metadata_path()).unwrap(), metadata);
        assert_eq!(
            std::fs::read(backup_path(&layout, schema)).unwrap(),
            original
        );
        let upgraded = std::fs::read(layout.metadata_path()).unwrap();
        let raw: serde_json::Value = serde_json::from_slice(&upgraded).unwrap();
        assert_eq!(raw["schema_version"], METADATA_SCHEMA_VERSION);

        let files = session_files(&layout);
        assert_eq!(load_session(&layout).unwrap().metadata, metadata);
        assert_eq!(std::fs::read(layout.metadata_path()).unwrap(), upgraded);
        assert_eq!(
            session_files(&layout),
            files,
            "a second read never upgrades"
        );
    }
}

/// The v10 child maintenance identity survives the nullable-owner upgrade without being reclassified as a root.
/// v10 子维护身份在可空所有者升级后保持不变，不会被重新归类为主会话。
/// Returns nothing; verifies the production reader and the exact original-byte backup.
/// 不返回值；验证生产读取器及精确原字节备份。
#[test]
fn metadata_access_upgrade_preserves_child_maintenance_identity() {
    // This storage-only fixture isolates the metadata wire contract from transcript validation.
    // 此存储专用夹具将元数据线协议与转录校验隔离。
    let (_root, layout, metadata) = fixture();
    // Version 10 always encoded a concrete child execution identity.
    // 版本 10 始终编码具体子执行身份。
    let mut raw = serde_json::to_value(metadata).unwrap();
    raw["schema_version"] = 10.into();
    raw["context_maintenance"] = serde_json::json!({
        "execution_id": "maintenance-owner", "business_execution_id": "child-owner",
        "business_turn_id": 1, "turn_id": 2, "segment_generation": 1, "source_revision": 1,
        "source_compaction_id": null, "started_seq": 4, "started_at_ms": 3, "terminal": null
    });
    // Retain exact pre-upgrade bytes, including the concrete source identity.
    // 保留精确升级前字节，包含具体来源身份。
    let original = serde_json::to_vec(&raw).unwrap();
    std::fs::write(layout.metadata_path(), &original).unwrap();
    // The normal read path performs the same atomic upgrade used by Release.
    // 正常读取路径执行 Release 使用的同一原子升级。
    let upgraded = read_metadata(&layout.metadata_path()).unwrap();
    assert_eq!(upgraded.schema_version, METADATA_SCHEMA_VERSION);
    assert_eq!(
        upgraded
            .context_maintenance
            .unwrap()
            .business_execution_id
            .unwrap()
            .as_str(),
        "child-owner"
    );
    assert_eq!(std::fs::read(backup_path(&layout, 10)).unwrap(), original);
}

/// The v13 window node id string becomes the v14 tagged selection; a malformed one is corrupt.
/// v13 窗口节点标识字符串转为 v14 带标签选择；格式错误的值视为损坏。
#[test]
fn metadata_access_upgrade_moves_window_node_into_selection() {
    let (_root, layout, metadata) = fixture();
    let mut raw = serde_json::to_value(&metadata).unwrap();
    raw["schema_version"] = 13.into();
    raw["config"]["context_compaction_window_node_id"] = "compression-window-4".into();
    let original = serde_json::to_vec(&raw).unwrap();
    std::fs::write(layout.metadata_path(), &original).unwrap();

    let upgraded = read_metadata(&layout.metadata_path()).unwrap();
    assert_eq!(
        upgraded.config.context_window_selection,
        Some(vulcan_ai_contract::ContextWindowSelection::Node(
            vulcan_ai_contract::ContextCompactionWindowNodeId::CompressionWindow4
        ))
    );
    assert_eq!(std::fs::read(backup_path(&layout, 13)).unwrap(), original);
    let written: serde_json::Value =
        serde_json::from_slice(&std::fs::read(layout.metadata_path()).unwrap()).unwrap();
    assert!(
        written["config"]
            .get("context_compaction_window_node_id")
            .is_none()
    );

    let (_root, layout, metadata) = fixture();
    let mut raw = serde_json::to_value(&metadata).unwrap();
    raw["schema_version"] = 13.into();
    raw["config"]["context_compaction_window_node_id"] = "compression-window-9".into();
    let original = serde_json::to_vec(&raw).unwrap();
    std::fs::write(layout.metadata_path(), &original).unwrap();
    assert!(read_metadata(&layout.metadata_path()).is_err());
    assert_eq!(std::fs::read(layout.metadata_path()).unwrap(), original);
}

/// A different existing backup stops the upgrade; an identical one is reused.
/// 已存在的不同备份会中止升级；相同备份则复用。
#[test]
fn metadata_access_upgrade_never_overwrites_a_different_backup() {
    let (_root, layout, metadata) = fixture();
    let original = write_schema(&layout, &metadata, METADATA_UPGRADE_BASELINE_VERSION);
    let backup = backup_path(&layout, METADATA_UPGRADE_BASELINE_VERSION);
    std::fs::write(&backup, b"unrelated").unwrap();

    assert!(read_metadata(&layout.metadata_path()).is_err());
    assert_eq!(std::fs::read(layout.metadata_path()).unwrap(), original);
    assert_eq!(std::fs::read(&backup).unwrap(), b"unrelated");

    std::fs::write(&backup, &original).unwrap();
    assert_eq!(read_metadata(&layout.metadata_path()).unwrap(), metadata);
    assert_eq!(std::fs::read(&backup).unwrap(), original);
}

/// A failed publish leaves the original file readable and the next read finishes the upgrade.
/// 发布失败时原文件保持可读，下一次读取完成升级。
#[test]
fn metadata_access_upgrade_survives_a_failed_publish() {
    let (_root, layout, metadata) = fixture();
    let original = write_schema(&layout, &metadata, METADATA_UPGRADE_BASELINE_VERSION);

    test_hooks::fail_next_artifact(WriteStage::Rename, "metadata");
    let error = read_metadata(&layout.metadata_path()).unwrap_err();
    test_hooks::clear_failures();
    assert_eq!(error.code, ErrorCode::AtomicWriteFailed);
    assert_eq!(std::fs::read(layout.metadata_path()).unwrap(), original);

    assert_eq!(read_metadata(&layout.metadata_path()).unwrap(), metadata);
    assert_eq!(
        std::fs::read(backup_path(&layout, METADATA_UPGRADE_BASELINE_VERSION)).unwrap(),
        original
    );
}

/// Concurrent first reads all return the upgraded document and leave one backup.
/// 并发首次读取都返回升级后的文档，并只留下一个备份。
#[test]
fn metadata_access_concurrent_first_reads_upgrade_once() {
    let (_root, layout, metadata) = fixture();
    let original = write_schema(&layout, &metadata, METADATA_UPGRADE_BASELINE_VERSION);
    let path = layout.metadata_path();
    let readers = (0..4)
        .map(|_| {
            let path = path.clone();
            std::thread::spawn(move || read_metadata(&path).unwrap())
        })
        .collect::<Vec<_>>();
    for reader in readers {
        assert_eq!(reader.join().unwrap(), metadata);
    }
    assert_eq!(
        std::fs::read(backup_path(&layout, METADATA_UPGRADE_BASELINE_VERSION)).unwrap(),
        original
    );
    let backups = session_files(&layout)
        .into_iter()
        .filter(|name| name.ends_with(".backup.json"))
        .count();
    assert_eq!(backups, 1);
}

/// A capability rebuild replaces only its subtree on the current schema and never publishes.
/// 能力重建只在当前模式上替换其子树，且绝不自行发布。
#[test]
fn metadata_access_capability_rebuild_requires_current_schema() {
    let (_root, layout, metadata) = fixture();
    let original = std::fs::read(layout.metadata_path()).unwrap();
    let candidate = super::super::documents::read_metadata_for_provider_tool_rebuild(
        &layout.metadata_path(),
        &Default::default(),
    )
    .unwrap();
    assert_eq!(candidate.schema_version, METADATA_SCHEMA_VERSION);
    assert_eq!(candidate.session_id, metadata.session_id);
    assert_eq!(std::fs::read(layout.metadata_path()).unwrap(), original);

    let old = write_schema(&layout, &metadata, METADATA_UPGRADE_BASELINE_VERSION);
    let error = super::super::documents::read_metadata_for_provider_tool_rebuild(
        &layout.metadata_path(),
        &Default::default(),
    )
    .unwrap_err();
    assert_eq!(error.code, ErrorCode::UnsupportedDevelopmentFormat);
    assert_eq!(std::fs::read(layout.metadata_path()).unwrap(), old);
}

/// The production configuration reader rejects schemas below the upgrade baseline unchanged.
/// 生产配置读取入口原样拒绝低于升级基线的模式。
#[test]
fn metadata_access_configuration_read_rejects_pre_baseline_schema() {
    let root = tempfile::tempdir().unwrap();
    let locator = crate::workspace_store::session_v1_store::create_session_v1_storage_fixture(
        root.path(),
        "access-model",
        1,
    )
    .unwrap();
    locator.read_request_configuration().unwrap();
    let path = locator.session_directory().join("metadata.json");
    let mut raw: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    raw["schema_version"] = (METADATA_UPGRADE_BASELINE_VERSION - 1).into();
    let original = serde_json::to_vec(&raw).unwrap();
    std::fs::write(&path, &original).unwrap();
    assert!(locator.read_request_configuration().is_err());
    assert_eq!(std::fs::read(&path).unwrap(), original);
}
