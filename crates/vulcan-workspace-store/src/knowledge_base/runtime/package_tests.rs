//! Current-content package round trips and lifecycle behavior against the real host.
//! 使用真实宿主验证当前内容套件往返与生命周期行为。

use super::super::package_archive::{CONTENT_ENTRY, MANIFEST_ENTRY};
use super::*;
use std::io::Read;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

/// Reads exact JSONL bytes from a small test-created package; never reads a database artifact.
/// 从测试创建的小套件读取精确 JSONL 字节；绝不读取数据库产物。
fn jsonl(path: &Path) -> Vec<u8> {
    // The only content entry is selected by format identity rather than structural position.
    // 唯一内容条目按格式身份选择，而非结构位置。
    let mut archive =
        zip::ZipArchive::new(std::fs::File::open(path).expect("archive")).expect("zip");
    // Only two current-format entries may exist.
    // 仅允许存在两条当前格式条目。
    let names: BTreeSet<_> = archive
        .file_names()
        .map(|name| name.expect("entry name").into_owned())
        .collect();
    assert_eq!(
        names,
        BTreeSet::from([MANIFEST_ENTRY.to_string(), CONTENT_ENTRY.to_string()])
    );
    // Fixture bodies are small and the production reader has independent bound regressions.
    // 夹具正文很小，生产读取器具有独立边界回归。
    let mut bytes = Vec::new();
    archive
        .by_name(CONTENT_ENTRY)
        .expect("content entry")
        .read_to_end(&mut bytes)
        .expect("read content");
    bytes
}

/// Verifies exact current content/config round trips with new IDs and a single initial import history.
/// 验证精确当前内容与配置往返，使用新身份和唯一初始导入历史。
#[tokio::test]
async fn package_round_trip_contains_only_current_content_and_configuration() {
    // Real SQLite and Jieba prove that imported content is rebuilt into fresh derived indexes.
    // 真实 SQLite 和结巴证明导入内容重建到全新派生索引。
    let (root, runtime, library) = fixture(KnowledgeIndexMode::Bm25).await;
    // An older body must remain solely in source history.
    // 旧正文必须仅留在来源历史中。
    let document = add(
        &runtime,
        &library,
        "guide/current.md",
        "historical-only-body",
    )
    .await;
    runtime
        .write_content(
            &library,
            "project",
            KnowledgeWriteRequest {
                action: "update".into(),
                document_id: Some(document.id.clone()),
                expected_revision: Some(document.revision),
                body: Some("# Current\r\ncurrentsearchneedle\r\n".into()),
                keywords: Some(vec!["portable".into()]),
                ..Default::default()
            },
            ai(),
        )
        .await
        .expect("current update");
    runtime
        .write_content(
            &library,
            "project",
            KnowledgeWriteRequest {
                action: "folder".into(),
                path: Some("empty/deep".into()),
                description: Some("Explicit empty folder".into()),
                ..Default::default()
            },
            ai(),
        )
        .await
        .expect("folder");
    runtime
        .ingest_documents(
            &library,
            "project",
            vec![source("pending summary body")],
            ai(),
            "host input",
        )
        .await
        .expect("pending document");
    runtime
        .set_custom_words(
            &library,
            vec![KnowledgeCustomWord {
                word: "currentsearchneedle".into(),
                weight: 25,
            }],
        )
        .await
        .expect("dictionary");
    // Export must not modify the source history or current content.
    // 导出不得修改来源历史或当前内容。
    let before = snapshot(&runtime, &library).await;
    // The source history length is compared dynamically rather than tied to a row position.
    // 动态比较来源历史长度，而非绑定行位置。
    let history_count = runtime
        .list_changes(&library, None, MAX_KNOWLEDGE_PAGE_SIZE)
        .await
        .expect("source history")
        .len();
    // Unique outputs are published without overwriting any existing file.
    // 唯一输出发布时不覆盖任何已有文件。
    let exported = runtime
        .export_package(&library, root.path())
        .await
        .expect("export");
    // The archive is strictly content/config only, not source SQLite or old body bytes.
    // 归档严格只含内容和配置，不含来源 SQLite 或旧正文字节。
    let original_jsonl = jsonl(Path::new(&exported.archive_path));
    assert!(
        !String::from_utf8(original_jsonl.clone())
            .expect("UTF8 JSONL")
            .contains("historical-only-body")
    );
    assert_eq!(
        runtime
            .inspect_package(Path::new(&exported.archive_path))
            .await
            .expect("inspect"),
        exported
    );
    // Import requires the exact identity returned by inspection.
    // 导入要求检查返回的精确身份。
    let imported = runtime
        .import_package_as_new(
            Path::new(&exported.archive_path),
            "Imported",
            &exported.sha256,
        )
        .await
        .expect("import");
    assert_ne!(imported.library.id, library);
    assert_eq!(
        imported.library.settings,
        exported.manifest.settings.clone().into()
    );
    assert_eq!(imported.library.description, exported.manifest.description);
    assert!(
        runtime
            .list_project_bindings(&imported.library.id)
            .await
            .expect("bindings")
            .is_empty()
    );
    // Import preserves content but intentionally starts a fresh history.
    // 导入保留内容，但刻意开始全新历史。
    let changes = runtime
        .list_changes(&imported.library.id, None, MAX_KNOWLEDGE_PAGE_SIZE)
        .await
        .expect("import history");
    assert_eq!(changes.len(), 1);
    assert!(
        changes
            .iter()
            .any(|change| change.seq == imported.change_seq
                && change.operation == "import"
                && change.actor.kind == "user")
    );
    // Current rows receive new stable local IDs and fresh revision tokens.
    // 当前行获得新的稳定本地身份和全新修订令牌。
    let read = runtime
        .read_document(
            &imported.library.id,
            None,
            Some("guide/current.md"),
            None,
            None,
        )
        .await
        .expect("read imported");
    assert_ne!(read.document.id, document.id);
    assert_eq!(read.document.revision, 1);
    assert_eq!(read.document.created_change_seq, imported.change_seq);
    assert_eq!(read.content, "# Current\r\ncurrentsearchneedle\r\n");
    assert_eq!(read.document.summary_state, "stale");
    // Pending summaries remain explicit rather than being guessed on import.
    // 待补充摘要保持显式状态，而非在导入时猜测。
    let pending = runtime
        .read_document(&imported.library.id, None, Some("files/a.txt"), None, None)
        .await
        .expect("pending summary");
    assert_eq!(pending.document.summary_state, "pending");
    assert!(
        runtime
            .list_custom_words(&imported.library.id)
            .await
            .expect("dictionary")
            .iter()
            .any(|entry| entry.word == "currentsearchneedle" && entry.weight == 25)
    );
    assert!(
        runtime
            .index_integrity(&imported.library.id)
            .await
            .expect("rebuilt FTS")
            .healthy
    );
    assert!(
        runtime
            .search(
                std::slice::from_ref(&imported.library.id),
                &["currentsearchneedle".into()],
                20
            )
            .await
            .expect("imported search")
            .hits
            .iter()
            .any(|hit| hit.document_id == read.document.id)
    );
    // Re-export compares semantic current records without UUID or timestamp snapshot assumptions.
    // 重新导出比较语义当前记录，不依赖 UUID 或时间戳快照。
    let reexported = runtime
        .export_package(&imported.library.id, root.path())
        .await
        .expect("re-export");
    assert_ne!(reexported.archive_path, exported.archive_path);
    assert_eq!(jsonl(Path::new(&reexported.archive_path)), original_jsonl);
    assert_eq!(snapshot(&runtime, &library).await, before);
    assert_eq!(
        runtime
            .list_changes(&library, None, MAX_KNOWLEDGE_PAGE_SIZE)
            .await
            .expect("source history")
            .len(),
        history_count
    );
}

/// Verifies changed preview identities and name conflicts leave all existing libraries untouched.
/// 验证变化的预检身份和名称冲突不影响全部已有知识库。
#[tokio::test]
async fn package_import_failure_does_not_replace_existing_libraries() {
    let (root, runtime, library) = fixture(KnowledgeIndexMode::Browse).await;
    add(&runtime, &library, "safe.md", "preserved current body").await;
    // Snapshot every source entity before either rejected lifecycle request.
    // 在两次被拒绝的生命周期请求前快照全部来源实体。
    let before = snapshot(&runtime, &library).await;
    // The approved artifact remains independently selectable after a rejected import.
    // 导入被拒后，已批准产物仍可独立选择。
    let exported = runtime
        .export_package(&library, root.path())
        .await
        .expect("export");
    assert!(
        runtime
            .import_package_as_new(
                Path::new(&exported.archive_path),
                "Fixture",
                &exported.sha256
            )
            .await
            .is_err()
    );
    assert!(
        runtime
            .import_package_as_new(
                Path::new(&exported.archive_path),
                "Rejected",
                &"0".repeat(64)
            )
            .await
            .unwrap_err()
            .starts_with("ERR_KNOWLEDGE_PACKAGE_CHANGED")
    );
    assert_eq!(
        runtime.list_libraries(None).await.expect("libraries").len(),
        1
    );
    assert_eq!(snapshot(&runtime, &library).await, before);
    // Empty packages still establish one import change and never invent a document.
    // 空套件仍建立一条导入变更，绝不虚构文档。
    let empty = runtime
        .create_library(
            "Empty",
            "Empty purpose",
            &[],
            &KnowledgeLibrarySettings {
                index_mode: KnowledgeIndexMode::Browse,
                embedding: None,
            },
        )
        .await
        .expect("empty library");
    let archive = runtime
        .export_package(&empty.id, root.path())
        .await
        .expect("empty export");
    let imported = runtime
        .import_package_as_new(
            Path::new(&archive.archive_path),
            "Imported empty",
            &archive.sha256,
        )
        .await
        .expect("empty import");
    assert_eq!(imported.library.document_count, 0);
    assert_eq!(
        runtime
            .list_changes(&imported.library.id, None, 20)
            .await
            .expect("empty history")
            .len(),
        1
    );
}

/// Verifies a vector package imports no ready vectors and rebuilds through the fake host executor.
/// 验证向量套件不导入就绪向量，并通过假宿主执行器重建。
#[tokio::test]
async fn vector_package_rebuilds_instead_of_copying_vector_or_history_state() {
    let (root, runtime, library) = fixture(KnowledgeIndexMode::Browse).await;
    // A deterministic gate separates source readiness from target background regeneration.
    // 确定门控将来源就绪与目标后台重新生成分开。
    let block = Arc::new(AtomicBool::new(false));
    // Completion waits on a real accepted fake request rather than a timing guess.
    // 完成等待真实接受的假请求，而非时间猜测。
    let started = Arc::new(tokio::sync::Notify::new());
    // The release permit affects only the one bounded imported document batch.
    // 释放许可仅影响单个有界导入文档批次。
    let release = Arc::new(tokio::sync::Notify::new());
    // Count actual background requests, excluding query inference.
    // 计数实际后台请求，排除查询推理。
    let calls = Arc::new(AtomicUsize::new(0));
    runtime
        .set_embedding_executor({
            let block = block.clone();
            let started = started.clone();
            let release = release.clone();
            let calls = calls.clone();
            Arc::new(move |request| {
                let block = block.clone();
                let started = started.clone();
                let release = release.clone();
                let calls = calls.clone();
                Box::pin(async move {
                    if !request.is_query {
                        calls.fetch_add(1, Ordering::SeqCst);
                        if block.load(Ordering::SeqCst) {
                            started.notify_one();
                            release.notified().await;
                        }
                    }
                    Ok(request.inputs.iter().map(|_| vec![1.0, 0.0]).collect())
                })
            })
        })
        .await
        .expect("gated fake");
    add(&runtime, &library, "vector.md", "current vector content").await;
    runtime
        .set_index_mode(
            &library,
            super::vector_tests::vector_settings(KnowledgeIndexMode::Vector, "portable-space"),
        )
        .await
        .expect("mode");
    let source_state = super::vector_tests::settled(&runtime, &library).await;
    assert!(source_state.ready > 0);
    let exported = runtime
        .export_package(&library, root.path())
        .await
        .expect("vector export");
    let before = calls.load(Ordering::SeqCst);
    block.store(true, Ordering::SeqCst);
    let imported = runtime
        .import_package_as_new(
            Path::new(&exported.archive_path),
            "Imported vectors",
            &exported.sha256,
        )
        .await
        .expect("vector import");
    tokio::time::timeout(Duration::from_secs(30), started.notified())
        .await
        .expect("new vector request");
    // Pending target work proves that no source-ready marker or Lance row was transported.
    // 待处理目标工作证明没有传输来源就绪标记或 Lance 行。
    let pending = runtime
        .embedding_status(&imported.library.id, 20, 0)
        .await
        .expect("target progress");
    assert!(pending.pending > 0);
    assert_eq!(pending.ready, 0);
    assert!(calls.load(Ordering::SeqCst) > before);
    release.notify_one();
    let complete = super::vector_tests::settled(&runtime, &imported.library.id).await;
    assert_eq!((complete.pending, complete.failed), (0, 0));
    assert!(complete.ready > 0);
    assert_eq!(
        runtime
            .list_changes(&imported.library.id, None, 20)
            .await
            .expect("target history")
            .len(),
        1
    );
}

/// Verifies failed-install cleanup cannot discard content that acquired the same library lock first.
/// 验证失败安装清理不能丢弃先获取相同知识库锁的内容。
#[tokio::test]
async fn failed_install_cleanup_preserves_concurrent_content() {
    // The fixture library represents a newly allocated installation identity.
    // 夹具知识库代表新分配安装身份。
    let (_root, runtime, library) = fixture(KnowledgeIndexMode::Browse).await;
    add(&runtime, &library, "concurrent.md", "must survive cleanup").await;
    assert!(
        runtime
            .discard_empty_library(&library)
            .await
            .unwrap_err()
            .starts_with("ERR_KNOWLEDGE_IMPORT_CLEANUP_CONFLICT")
    );
    assert_eq!(
        runtime
            .read_document(&library, None, Some("concurrent.md"), None, None)
            .await
            .expect("preserved concurrent document")
            .content,
        "must survive cleanup"
    );
    // An actually empty fresh identity remains safe to remove.
    // 实际为空的新身份仍可安全移除。
    let empty = runtime
        .create_library(
            "Failed empty",
            "",
            &[],
            &KnowledgeLibrarySettings {
                index_mode: KnowledgeIndexMode::Browse,
                embedding: None,
            },
        )
        .await
        .expect("fresh empty library");
    runtime
        .discard_empty_library(&empty.id)
        .await
        .expect("remove unused installation");
    assert!(runtime.get_library(&empty.id).await.is_err());
}
