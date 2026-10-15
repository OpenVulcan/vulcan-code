//! Strict archive-format tests using bounded synthetic data only.
//! 仅使用有界合成数据的严格归档格式测试。

use super::*;
use std::io::Cursor;

/// Builds a current empty manifest; all fields are format data rather than historical snapshots.
/// 构建当前空清单；全部字段为格式数据，而非历史快照。
fn manifest() -> KnowledgePackageManifest {
    KnowledgePackageManifest {
        format: KNOWLEDGE_PACKAGE_FORMAT.into(),
        version: KNOWLEDGE_PACKAGE_VERSION,
        name: "Package fixture".into(),
        description: "Portable current content".into(),
        settings: KnowledgeLibrarySettings {
            index_mode: KnowledgeIndexMode::Browse,
            embedding: None,
        },
        custom_words: vec![],
        document_count: 0,
        folder_count: 0,
        content_sha256: super::super::super::sha256_hex(b""),
        exported_at_ms: 1,
    }
}

/// Writes exactly two named entries, allowing explicit hostile entry-name fixtures.
/// 写入恰好两条命名记录，允许显式恶意条目名称夹具。
fn archive(path: &Path, manifest: &serde_json::Value, content: &[u8], content_name: &str) {
    // A stored ZIP makes later corruption fixtures independent of compression internals.
    // 不压缩 ZIP 使后续损坏夹具独立于压缩内部细节。
    let mut writer = ZipWriter::new(File::create(path).expect("create archive"));
    // Entry encoding is deliberately explicit and portable.
    // 条目编码刻意显式且可移植。
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
    writer
        .start_file(MANIFEST_ENTRY, options)
        .expect("manifest entry");
    writer
        .write_all(&serde_json::to_vec(manifest).expect("manifest bytes"))
        .expect("write manifest");
    writer
        .start_file(content_name, options)
        .expect("content entry");
    writer.write_all(content).expect("write content");
    writer.finish().expect("finish archive");
}

/// Verifies exact version acceptance, strict current settings and read-only rejection of old versions.
/// 验证精确版本接受、严格当前设置及只读拒绝旧版本。
#[test]
fn package_versions_and_strict_shapes_are_enforced() {
    // Every rejected file is checked for byte preservation.
    // 每个被拒绝文件都检查字节保留。
    let root = tempfile::tempdir().expect("root");
    // A real bounded ZIP is used rather than a source-string or content golden guard.
    // 使用真实有界 ZIP，而非源码字符串或内容黄金守卫。
    let path = root.path().join("version.vkb");
    for version in [0, 1, 2, KNOWLEDGE_PACKAGE_VERSION + 1] {
        let mut header = serde_json::to_value(manifest()).expect("manifest");
        header["version"] = serde_json::json!(version);
        archive(&path, &header, b"", CONTENT_ENTRY);
        let before = fs::read(&path).expect("before");
        assert!(
            prepare_knowledge_package(&path, None)
                .err()
                .expect("old/future refusal")
                .starts_with("ERR_KNOWLEDGE_PACKAGE_VERSION_UNSUPPORTED")
        );
        assert_eq!(fs::read(&path).expect("after"), before);
    }
    // Only the exact supported version and current field set are accepted.
    // 仅接受精确支持版本与当前字段集合。
    let header = serde_json::to_value(manifest()).expect("manifest");
    archive(&path, &header, b"", CONTENT_ENTRY);
    assert!(prepare_knowledge_package(&path, None).is_ok());
    // Obsolete retrieval fields are never interpreted as a new index mode.
    // 过时检索字段绝不解释为新索引模式。
    let mut invalid = header;
    invalid["settings"]["retrievalMode"] = serde_json::json!("raw_bm25");
    archive(&path, &invalid, b"", CONTENT_ENTRY);
    assert!(prepare_knowledge_package(&path, None).is_err());
}

/// Verifies traversal, legacy artifacts, duplicate entries and inconsistent manifests are rejected.
/// 验证遍历路径、旧产物、重复条目和不一致清单被拒绝。
#[test]
fn package_entry_set_and_content_integrity_are_closed() {
    let root = tempfile::tempdir().expect("root");
    let path = root.path().join("hostile.vkb");
    let header = serde_json::to_value(manifest()).expect("manifest");
    for entry in [
        "../content.jsonl",
        "library/content.sqlite3",
        "history.jsonl",
        "vectors.bin",
    ] {
        archive(&path, &header, b"", entry);
        assert!(
            prepare_knowledge_package(&path, None)
                .err()
                .expect("entry refusal")
                .starts_with("ERR_KNOWLEDGE_PACKAGE_ENTRY_INVALID")
        );
    }
    // Renaming ZIP names preserves payload CRC while creating a true duplicate-name container.
    // 重命名 ZIP 名称保留载荷校验，同时创建真实重复名称容器。
    archive(&path, &header, b"", CONTENT_ENTRY);
    let mut bytes = fs::read(&path).expect("archive bytes");
    assert_eq!(
        CONTENT_ENTRY.len(),
        MANIFEST_ENTRY.len(),
        "ZIP fixture names may have evolved; update the same-length duplicate-name fixture"
    );
    let positions: Vec<_> = bytes
        .windows(CONTENT_ENTRY.len())
        .enumerate()
        .filter_map(|(index, value)| (value == CONTENT_ENTRY.as_bytes()).then_some(index))
        .collect();
    for position in positions {
        bytes[position..position + MANIFEST_ENTRY.len()].copy_from_slice(MANIFEST_ENTRY.as_bytes());
    }
    fs::write(&path, bytes).expect("duplicate names");
    assert!(prepare_knowledge_package(&path, None).is_err());
    // The manifest digest and counts must agree with decoded logical content.
    // 清单摘要和数量必须与解码逻辑内容一致。
    let mut wrong_digest = header.clone();
    wrong_digest["contentSha256"] = serde_json::json!("0".repeat(64));
    archive(&path, &wrong_digest, b"", CONTENT_ENTRY);
    assert!(
        prepare_knowledge_package(&path, None)
            .err()
            .expect("digest")
            .starts_with("ERR_KNOWLEDGE_PACKAGE_CONTENT_DIGEST")
    );
    let mut wrong_count = header;
    wrong_count["documentCount"] = serde_json::json!(1);
    archive(&path, &wrong_count, b"", CONTENT_ENTRY);
    assert!(
        prepare_knowledge_package(&path, None)
            .err()
            .expect("count")
            .starts_with("ERR_KNOWLEDGE_PACKAGE_COUNT_MISMATCH")
    );
}

/// Verifies the decompression reader enforces actual bytes and exact declared lengths using small bounds.
/// 验证解压读取器通过小边界约束实际字节和精确声明长度。
#[test]
fn decompression_bounds_do_not_trust_declared_sizes() {
    assert!(read_bounded_entry(&mut Cursor::new(b"abc"), 3, 2).is_err());
    assert!(
        read_bounded_entry(&mut Cursor::new(b"abc"), 2, 2)
            .expect_err("actual bound")
            .starts_with("ERR_KNOWLEDGE_PACKAGE_TOO_LARGE")
    );
    assert!(
        read_bounded_entry(&mut Cursor::new(b"abc"), 1, 10)
            .expect_err("declared size")
            .starts_with("ERR_KNOWLEDGE_PACKAGE_SIZE_MISMATCH")
    );
    assert_eq!(
        read_bounded_entry(&mut Cursor::new(b"abc"), 3, 3).expect("exact bound"),
        b"abc"
    );
}

/// Verifies preview checksum binding and manifest bounds before current-content allocation.
/// 验证当前内容分配前的预检校验和绑定及清单边界。
#[test]
fn inspection_digest_and_manifest_limit_precede_import() {
    let root = tempfile::tempdir().expect("root");
    let path = root.path().join("bounded.vkb");
    let header = serde_json::to_value(manifest()).expect("manifest");
    archive(&path, &header, b"", CONTENT_ENTRY);
    let inspected = prepare_knowledge_package(&path, None)
        .expect("inspect")
        .inspection;
    assert!(prepare_knowledge_package(&path, Some(&inspected.sha256)).is_ok());
    let mut changed = header;
    changed["description"] = serde_json::json!("Changed after preview");
    archive(&path, &changed, b"", CONTENT_ENTRY);
    assert!(
        prepare_knowledge_package(&path, Some(&inspected.sha256))
            .err()
            .expect("changed")
            .starts_with("ERR_KNOWLEDGE_PACKAGE_CHANGED")
    );
    // The declared manifest count is checked before allocating logical record objects.
    // 声明清单数量在分配逻辑记录对象前检查。
    changed["folderCount"] = serde_json::json!(MAX_KNOWLEDGE_PACKAGE_ENTRIES + 1);
    archive(&path, &changed, b"", CONTENT_ENTRY);
    assert!(
        prepare_knowledge_package(&path, None)
            .err()
            .expect("entry limit")
            .starts_with("ERR_KNOWLEDGE_PACKAGE_ENTRY_LIMIT")
    );
}

/// Verifies invalid paths, summary states and conflicting document/directory namespaces cannot publish.
/// 验证非法路径、摘要状态和冲突文档目录命名空间不能发布。
#[test]
fn logical_content_contract_is_validated_before_publication() {
    // This valid document is varied only along explicit current-format constraints.
    // 仅沿显式当前格式约束变化该有效文档。
    let document = KnowledgePackageDocument {
        path: "guide/a.md".into(),
        title: "A".into(),
        summary: "".into(),
        summary_state: "pending".into(),
        keywords: vec![],
        body: "# A\nExact body\r\n".into(),
        embed_policy: KnowledgeEmbedPolicy::Summary,
        provenance: vec![],
    };
    for path in ["../a", "a//b", "/a.md", "a\\b"] {
        let mut invalid = document.clone();
        invalid.path = path.into();
        assert!(
            content::prepare_package_content(vec![KnowledgePackageRecord::Document(invalid)])
                .is_err()
        );
    }
    assert!(
        content::prepare_package_content(vec![
            KnowledgePackageRecord::Document(document.clone()),
            KnowledgePackageRecord::Document(document.clone())
        ])
        .is_err()
    );
    assert!(
        content::prepare_package_content(vec![
            KnowledgePackageRecord::Document(document.clone()),
            KnowledgePackageRecord::Folder(KnowledgePackageFolder {
                path: "guide/a.md".into(),
                description: "conflict".into()
            })
        ])
        .is_err()
    );
    let mut invalid = document;
    invalid.summary_state = "unknown".into();
    assert!(
        content::prepare_package_content(vec![KnowledgePackageRecord::Document(invalid)]).is_err()
    );
}
