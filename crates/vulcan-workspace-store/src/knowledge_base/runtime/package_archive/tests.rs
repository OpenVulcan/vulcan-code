//! Portable archive boundary checks and shared binary package fixtures.
//! 可移植归档边界检查与共享二进制包夹具。

use super::*;
use crate::knowledge_base::models::KnowledgeLibrarySettings;
use tempfile::tempdir;

/// Writes one compact package fixture with caller-controlled metadata entry and optional extra entry.
/// 写入一个紧凑知识库包夹具，允许调用方控制元数据条目及可选额外条目。
fn write_package_fixture(
    path: &Path,
    package: &KnowledgePackageManifest,
    manifest: &KnowledgeManifest,
    database: &[u8],
    metadata_entry: &str,
    extra_entry: Option<&str>,
) {
    let file = File::create(path).expect("create package fixture");
    let mut writer = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    writer
        .start_file(metadata_entry, options)
        .expect("start package metadata");
    writer
        .write_all(&serde_json::to_vec(package).expect("serialize package metadata"))
        .expect("write package metadata");
    writer
        .start_file(KNOWLEDGE_PACKAGE_LIBRARY_MANIFEST_ENTRY, options)
        .expect("start package manifest");
    writer
        .write_all(&serde_json::to_vec(manifest).expect("serialize package manifest"))
        .expect("write package manifest");
    writer
        .start_file(KNOWLEDGE_PACKAGE_DATABASE_ENTRY, options)
        .expect("start package database");
    writer.write_all(database).expect("write package database");
    if let Some(extra_entry) = extra_entry {
        writer
            .start_file(extra_entry, options)
            .expect("start extra package entry");
        writer
            .write_all(b"extra")
            .expect("write extra package entry");
    }
    writer.finish().expect("finish package fixture");
}

/// Builds matching metadata for one compact package fixture.
/// 为一个紧凑知识库包夹具构建相互匹配的元数据。
fn package_fixture_metadata(database: &[u8]) -> (KnowledgePackageManifest, KnowledgeManifest) {
    let source_library_id = String::from("550e8400-e29b-41d4-a716-446655440000");
    let package = KnowledgePackageManifest {
        format: String::from(KNOWLEDGE_PACKAGE_FORMAT),
        package_version: KNOWLEDGE_PACKAGE_VERSION,
        source_library_id: source_library_id.clone(),
        name: String::from("测试知识库"),
        description: String::from("fixture"),
        settings: Some(KnowledgeLibrarySettings::default()),
        schema_version: KNOWLEDGE_SCHEMA_VERSION,
        document_count: 0,
        chunk_count: 0,
        index_status: String::from("ready"),
        exported_at_ms: 1,
        content_sha256: hex::encode(Sha256::digest(database)),
        content_bytes: database.len() as u64,
        derived_artifacts: Vec::new(),
    };
    let manifest = KnowledgeManifest {
        library_id: source_library_id,
        name: package.name.clone(),
        schema_version: KNOWLEDGE_SCHEMA_VERSION,
    };
    (package, manifest)
}

/// Rewrites only the declared source identity while retaining the original SQLite bytes and digest.
/// 仅重写声明的来源身份，同时保留原始 SQLite 字节与摘要。
pub(in super::super) fn write_identity_mismatch_package(source: &Path, destination: &Path) {
    // archive reads the three exact entries from a valid exported package.
    // archive 从一个有效导出包中读取三个精确条目。
    let input = File::open(source).expect("open valid source package");
    let mut archive = ZipArchive::new(input).expect("parse valid source package");
    // package changes provenance metadata without changing the database content.
    // package 修改来源元数据，但不改变数据库内容。
    let mut package: KnowledgePackageManifest = {
        let mut entry = archive
            .by_name(KNOWLEDGE_PACKAGE_METADATA_ENTRY)
            .expect("read source package metadata");
        serde_json::from_reader(&mut entry).expect("decode source package metadata")
    };
    // manifest mirrors the forged identity so the outer package checks still pass.
    // manifest 镜像伪造身份，使知识库包外层检查仍能通过。
    let mut manifest: KnowledgeManifest = {
        let mut entry = archive
            .by_name(KNOWLEDGE_PACKAGE_LIBRARY_MANIFEST_ENTRY)
            .expect("read source package manifest");
        serde_json::from_reader(&mut entry).expect("decode source package manifest")
    };
    // database retains the real embedded identity and therefore must fail full inspection.
    // database 保留真实内嵌身份，因此必须无法通过完整检查。
    let mut database = Vec::new();
    archive
        .by_name(KNOWLEDGE_PACKAGE_DATABASE_ENTRY)
        .expect("read source package database")
        .read_to_end(&mut database)
        .expect("decode source package database");
    // forgedIdentity is canonical but intentionally differs from the SQLite metadata row.
    // forgedIdentity 是规范标识，但刻意不同于 SQLite 元数据记录。
    let forged_identity = String::from("6ba7b810-9dad-11d1-80b4-00c04fd430c8");
    package.source_library_id.clone_from(&forged_identity);
    manifest.library_id = forged_identity;
    write_package_fixture(
        destination,
        &package,
        &manifest,
        &database,
        KNOWLEDGE_PACKAGE_METADATA_ENTRY,
        None,
    );
}

/// Verifies the minimal three-entry fixture obeys exact metadata, entry and digest validation.
/// 验证最小三条目夹具遵守精确元数据、条目与摘要校验。
#[test]
fn package_preparation_enforces_exact_entries_and_digest() {
    let temporary = tempdir().expect("create package fixture root");
    let database = b"SQLite format 3\0fixture";
    let (package, manifest) = package_fixture_metadata(database);
    let valid_path = temporary.path().join("valid.vkb");
    write_package_fixture(
        &valid_path,
        &package,
        &manifest,
        database,
        KNOWLEDGE_PACKAGE_METADATA_ENTRY,
        None,
    );
    let prepared =
        prepare_knowledge_package(&valid_path, temporary.path()).expect("prepare matching package");
    assert_eq!(prepared.package, package);

    let mut corrupt_package = package.clone();
    corrupt_package.content_sha256 = "0".repeat(64);
    let corrupt_path = temporary.path().join("corrupt.vkb");
    write_package_fixture(
        &corrupt_path,
        &corrupt_package,
        &manifest,
        database,
        KNOWLEDGE_PACKAGE_METADATA_ENTRY,
        None,
    );
    assert!(prepare_knowledge_package(&corrupt_path, temporary.path()).is_err());

    let traversal_path = temporary.path().join("traversal.vkb");
    write_package_fixture(
        &traversal_path,
        &package,
        &manifest,
        database,
        "../package.json",
        None,
    );
    assert!(prepare_knowledge_package(&traversal_path, temporary.path()).is_err());

    let extra_path = temporary.path().join("extra.vkb");
    write_package_fixture(
        &extra_path,
        &package,
        &manifest,
        database,
        KNOWLEDGE_PACKAGE_METADATA_ENTRY,
        Some("unexpected.txt"),
    );
    assert!(prepare_knowledge_package(&extra_path, temporary.path()).is_err());

    // unsupportedPackage proves the format protocol field is enforced.
    // unsupportedPackage 证明格式协议字段会被强制校验。
    let mut unsupported_package = package.clone();
    unsupported_package.format = String::from("unsupported-format");
    let unsupported_path = temporary.path().join("unsupported.vkb");
    write_package_fixture(
        &unsupported_path,
        &unsupported_package,
        &manifest,
        database,
        KNOWLEDGE_PACKAGE_METADATA_ENTRY,
        None,
    );
    assert!(prepare_knowledge_package(&unsupported_path, temporary.path()).is_err());

    // incompatiblePackage proves the SQLite schema version is checked independently.
    // incompatiblePackage 证明 SQLite 结构版本会被独立校验。
    let mut incompatible_package = package.clone();
    incompatible_package.schema_version += 1;
    let incompatible_path = temporary.path().join("incompatible.vkb");
    write_package_fixture(
        &incompatible_path,
        &incompatible_package,
        &manifest,
        database,
        KNOWLEDGE_PACKAGE_METADATA_ENTRY,
        None,
    );
    assert!(prepare_knowledge_package(&incompatible_path, temporary.path()).is_err());
}

/// Verifies bounded ZIP readers reject decompressed metadata beyond the configured limit.
/// 验证有界 ZIP 读取器拒绝超过配置上限的解压元数据。
#[test]
fn bounded_package_entry_rejects_expansion_over_limit() {
    // oversized creates one byte beyond the exact test limit.
    // oversized 创建比精确测试上限多一个字节的数据。
    let mut oversized = std::io::Cursor::new(vec![0_u8; 17]);
    assert!(read_bounded_zip_entry(&mut oversized, 16, "fixture").is_err());
}

/// Verifies long or filesystem-reserved names remain bounded in exported archive names.
/// 验证过长或文件系统保留字符名称生成的导出归档名保持有界。
#[test]
fn portable_archive_name_is_safe_and_bounded() {
    let archive_name = portable_knowledge_archive_name(
        &format!("CON:<>?*{}", "界".repeat(200)),
        123456789,
        "550e8400-e29b-41d4-a716-446655440000",
    );
    assert!(archive_name.ends_with("-123456789-550e8400-e29b-41d4-a716-446655440000.vkb"));
    assert!(
        archive_name
            .chars()
            .all(|character| !matches!(character, ':' | '<' | '>' | '?' | '*'))
    );
    assert!(
        archive_name.chars().count()
            <= 48
                + "-123456789-550e8400-e29b-41d4-a716-446655440000.vkb"
                    .chars()
                    .count()
    );
}
