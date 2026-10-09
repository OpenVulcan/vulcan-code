//! Exact-version metadata access through production storage readers.
//! 通过生产存储读取入口验证精确版本的元数据访问。

use super::*;
use vulcan_session_core::v1::config::METADATA_SCHEMA_VERSION;

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

/// Older and future schemas are rejected as unsupported, leaving bytes and directory unchanged.
/// 旧模式与未来模式都以不受支持拒绝，文件字节与目录保持不变。
#[test]
fn metadata_access_rejects_other_schemas_unchanged() {
    for schema in [1, METADATA_SCHEMA_VERSION - 1, METADATA_SCHEMA_VERSION + 1] {
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

    let old = write_schema(&layout, &metadata, METADATA_SCHEMA_VERSION - 1);
    let error = super::super::documents::read_metadata_for_provider_tool_rebuild(
        &layout.metadata_path(),
        &Default::default(),
    )
    .unwrap_err();
    assert_eq!(error.code, ErrorCode::UnsupportedDevelopmentFormat);
    assert_eq!(std::fs::read(layout.metadata_path()).unwrap(), old);
}

/// The production configuration reader surfaces the rejection instead of upgrading.
/// 生产配置读取入口直接暴露拒绝，而不是升级。
#[test]
fn metadata_access_configuration_read_rejects_old_schema() {
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
    raw["schema_version"] = (METADATA_SCHEMA_VERSION - 1).into();
    let original = serde_json::to_vec(&raw).unwrap();
    std::fs::write(&path, &original).unwrap();
    assert!(locator.read_request_configuration().is_err());
    assert_eq!(std::fs::read(&path).unwrap(), original);
}
