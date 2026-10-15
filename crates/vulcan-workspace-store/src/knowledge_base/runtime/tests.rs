//! Behavioral regressions for exact-version AI-managed content and restore semantics.
//! 精确版本 AI 管理内容与还原语义的行为回归。

use super::*;
#[path = "vector_tests.rs"]
mod vector_tests;
use crate::data_plane::{HostRuntimeMetadata, build_application_schema_script};

/// Creates an isolated real host, application schema and authorized project; returns lifetime root and runtime.
/// 创建隔离真实宿主、应用结构和已授权项目；返回生命周期根目录与运行时。
async fn fixture(mode: KnowledgeIndexMode) -> (tempfile::TempDir, KnowledgeBaseRuntime, String) {
    // Every fixture owns all persistent paths and binding identities.
    // 每个夹具拥有全部持久路径和绑定身份。
    let root = tempfile::tempdir().expect("fixture root");
    // The real local database runtime creates the same SQLite bindings as production.
    // 真实本地数据库运行时创建与生产相同的 SQLite 绑定。
    let database = root.path().join("sqlite/app.sqlite3");
    std::fs::create_dir_all(database.parent().expect("database parent"))
        .expect("create SQLite directory");
    // Shared wrappers later reuse this exact host identity.
    // 后续共享包装复用该精确宿主身份。
    let runtime = KnowledgeBaseRuntime::new(HostRuntimeHandle::new_for_test(HostRuntimeMetadata {
        data_root: root.path().to_path_buf(),
        sqlite_path: database.clone(),
        sqlite_binding_id: format!("kb-test-{}", Uuid::new_v4()),
        system_statistics_sqlite_path: database,
        system_statistics_sqlite_binding_id: format!("kb-stat-{}", Uuid::new_v4()),
    }));
    local_sql::execute_script_async(
        &runtime.host_runtime.local,
        LogicalDatabase::AppState,
        build_application_schema_script(),
        vec![],
    )
    .await
    .expect("application schema");
    local_sql::execute_script_async(&runtime.host_runtime.local,LogicalDatabase::AppState,
        "INSERT INTO projects(id,name,root_path,created_at,updated_at) VALUES('project','fixture','fixture','','');",vec![]).await.expect("project");
    // Storage tests use only free full-text/browse modes; vector worker tests use explicit fake routes later.
    // 存储测试仅使用免费全文或浏览模式；后续向量工作器测试使用显式假路由。
    let library = runtime
        .create_library(
            "Fixture",
            "AI content fixture",
            &["project".into()],
            &KnowledgeLibrarySettings {
                index_mode: mode,
                embedding: None,
            },
        )
        .await
        .expect("create library");
    (root, runtime, library.id)
}

/// Returns a complete AI invocation identity for behavior tests.
/// 为行为测试返回完整 AI 调用身份。
fn ai() -> KnowledgeChangeActor {
    KnowledgeChangeActor {
        kind: "ai".into(),
        session_id: Some("child-session".into()),
        turn_id: Some("turn".into()),
        tool_call_id: Some(Uuid::new_v4().to_string()),
    }
}

/// Returns the only actor permitted to restore content.
/// 返回唯一允许还原内容的执行者。
fn human() -> KnowledgeChangeActor {
    KnowledgeChangeActor {
        kind: "user".into(),
        session_id: None,
        turn_id: None,
        tool_call_id: None,
    }
}

/// Builds an explicit create request; returns a complete required-field payload.
/// 构建显式创建请求；返回完整必填字段载荷。
fn create(path: &str, body: &str) -> KnowledgeWriteRequest {
    KnowledgeWriteRequest {
        action: "create".into(),
        path: Some(path.into()),
        title: Some("Document".into()),
        summary: Some("Useful summary".into()),
        body: Some(body.into()),
        ..Default::default()
    }
}

/// Creates one document and returns its exact receipt row by path.
/// 创建一篇文档并按路径返回其精确回执行。
async fn add(
    runtime: &KnowledgeBaseRuntime,
    library: &str,
    path: &str,
    body: &str,
) -> KnowledgeDocumentSummary {
    // A create action must emit exactly one affected surviving row.
    // 创建动作必须产生恰好一条受影响仍存记录。
    let receipt = runtime
        .write_content(library, "project", create(path, body), ai())
        .await
        .expect("create document");
    assert_eq!(receipt.documents.len(), 1);
    receipt
        .documents
        .into_iter()
        .find(|d| d.path == path)
        .expect("created path may have changed")
}

/// Captures complete content and explicit folders while excluding regenerated audit/revision metadata.
/// 捕获完整内容和显式目录，同时排除重新生成的审计与修订元数据。
async fn snapshot(runtime: &KnowledgeBaseRuntime, library: &str) -> serde_json::Value {
    // Comparison reads every entity without depending on directory page positions.
    // 比较读取全部实体，不依赖目录页位置。
    let (_, binding) = runtime
        .ensure_library_binding(library)
        .await
        .expect("binding");
    local_sql::with_read_snapshot_async(&runtime.host_runtime.local,runtime.resolve_library_binding(binding).expect("resolve"),|connection|{
        let mut docs=connection.prepare("SELECT d.id,d.path,d.title,d.summary,d.summary_state,d.keywords_json,b.content,d.embed_policy,d.provenance_json FROM kb_documents d JOIN kb_blobs b ON b.sha256=d.body_sha256 ORDER BY d.id;").map_err(|e|e.to_string())?;
        let documents=docs.query_map([],|row| (0..9).map(|column|row.get::<_,String>(column)).collect::<rusqlite::Result<Vec<_>>>()).map_err(|e|e.to_string())?.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())?;
        let mut dirs=connection.prepare("SELECT path_key,path,description FROM kb_folders ORDER BY path_key;").map_err(|e|e.to_string())?;
        let folders=dirs.query_map([],|row|(0..3).map(|column|row.get::<_,String>(column)).collect::<rusqlite::Result<Vec<_>>>()).map_err(|e|e.to_string())?.collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())?;
        Ok(serde_json::json!({"documents":documents,"folders":folders}))
    }).await.expect("consistent content snapshot")
}

/// Verifies all write actions emit one change, exact edits reject ambiguity and move preserves identity.
/// 验证全部写动作产生一条变更、精确编辑拒绝歧义且移动保留身份。
#[tokio::test]
async fn actions_are_atomic_revision_checked_and_audited() {
    // A real Jieba index participates in the same SQLite transaction.
    // 真实结巴索引参与同一 SQLite 事务。
    let (_root, runtime, library) = fixture(KnowledgeIndexMode::Bm25).await;
    // Capture the sole creation receipt by semantic path.
    // 按语义路径捕获唯一创建回执。
    let original = add(&runtime, &library, "guide/a.md", "# Start\nunique marker\n").await;
    assert_eq!(original.revision, 1);
    // A stale revision cannot alter either content or history.
    // 过期修订不能改变内容或历史。
    let stale = KnowledgeWriteRequest {
        action: "update".into(),
        document_id: Some(original.id.clone()),
        expected_revision: Some(0),
        title: Some("Rejected".into()),
        ..Default::default()
    };
    assert!(
        runtime
            .write_content(&library, "project", stale, ai())
            .await
            .unwrap_err()
            .starts_with("ERR_KNOWLEDGE_REVISION_CONFLICT")
    );
    // Exact replacement has no fuzzy fallback.
    // 精确替换没有模糊兜底。
    let edit = KnowledgeWriteRequest {
        action: "edit".into(),
        document_id: Some(original.id.clone()),
        expected_revision: Some(original.revision),
        edits: Some(vec![KnowledgeTextEdit {
            old_text: "unique".into(),
            new_text: "replacement".into(),
            replace_all: false,
        }]),
        summary: Some("Updated".into()),
        ..Default::default()
    };
    runtime
        .write_content(&library, "project", edit, ai())
        .await
        .expect("edit");
    // Re-read the current token before moving.
    // 移动前重新读取当前令牌。
    let current = runtime
        .read_document(&library, Some(&original.id), None, None, None)
        .await
        .expect("read edited");
    assert!(current.content.contains("replacement"));
    let moved = runtime
        .write_content(
            &library,
            "project",
            KnowledgeWriteRequest {
                action: "move".into(),
                document_id: Some(original.id.clone()),
                expected_revision: Some(current.document.revision),
                new_path: Some("archive/a.md".into()),
                ..Default::default()
            },
            ai(),
        )
        .await
        .expect("move");
    assert!(
        moved
            .documents
            .iter()
            .any(|d| d.id == original.id && d.path == "archive/a.md")
    );
    // Folder updates are content operations, not registry settings mutations.
    // 目录更新是内容操作，而非注册设置变更。
    runtime
        .write_content(
            &library,
            "project",
            KnowledgeWriteRequest {
                action: "folder".into(),
                path: Some("empty".into()),
                description: Some("AI folder".into()),
                ..Default::default()
            },
            ai(),
        )
        .await
        .expect("folder");
    runtime
        .write_content(
            &library,
            "project",
            KnowledgeWriteRequest {
                action: "delete_folder".into(),
                path: Some("empty".into()),
                ..Default::default()
            },
            ai(),
        )
        .await
        .expect("delete folder");
    let current = runtime
        .read_document(&library, Some(&original.id), None, None, None)
        .await
        .expect("read moved");
    runtime
        .write_content(
            &library,
            "project",
            KnowledgeWriteRequest {
                action: "delete".into(),
                document_id: Some(original.id.clone()),
                expected_revision: Some(current.document.revision),
                ..Default::default()
            },
            ai(),
        )
        .await
        .expect("delete");
    let history = runtime
        .list_changes(&library, None, MAX_KNOWLEDGE_PAGE_SIZE)
        .await
        .expect("history");
    assert_eq!(history.len(), 6);
    for change in &history {
        let detail = runtime
            .get_change(&library, change.seq)
            .await
            .expect("detail");
        assert_eq!(detail.items.len(), 1);
        assert_eq!(change.actor.session_id.as_deref(), Some("child-session"));
    }
    assert!(
        runtime
            .read_document(&library, Some(&original.id), None, None, None)
            .await
            .is_err()
    );
}

/// Verifies namespace normalization, collisions, edit limits and invalid fields fail without history writes.
/// 验证命名空间规范、冲突、编辑限制与非法字段均失败且不写历史。
#[tokio::test]
async fn invalid_requests_preserve_content_and_history() {
    let (_root, runtime, library) = fixture(KnowledgeIndexMode::Browse).await;
    let document = add(&runtime, &library, "Café/a.md", "repeat repeat").await;
    let baseline = snapshot(&runtime, &library).await;
    for path in ["../escape", "a//b", "a/./b", "a\\b", "a\n"] {
        assert!(
            runtime
                .write_content(&library, "project", create(path, "text"), ai())
                .await
                .unwrap_err()
                .starts_with("ERR_KNOWLEDGE_PATH_INVALID")
        );
    }
    assert!(
        runtime
            .write_content(
                &library,
                "project",
                create("CAFE\u{301}/A.md", "text"),
                ai()
            )
            .await
            .unwrap_err()
            .starts_with("ERR_KNOWLEDGE_PATH_CONFLICT")
    );
    assert!(
        runtime
            .write_content(&library, "project", create("café", "text"), ai())
            .await
            .unwrap_err()
            .starts_with("ERR_KNOWLEDGE_PATH_CONFLICT")
    );
    assert!(
        runtime
            .write_content(
                &library,
                "project",
                create("too-large", &"x".repeat(MAX_KNOWLEDGE_BODY_BYTES + 1)),
                ai()
            )
            .await
            .unwrap_err()
            .starts_with("ERR_KNOWLEDGE_BODY_TOO_LARGE")
    );
    let ambiguous = KnowledgeWriteRequest {
        action: "edit".into(),
        document_id: Some(document.id.clone()),
        expected_revision: Some(document.revision),
        edits: Some(vec![KnowledgeTextEdit {
            old_text: "repeat".into(),
            new_text: "x".into(),
            replace_all: false,
        }]),
        ..Default::default()
    };
    assert!(
        runtime
            .write_content(&library, "project", ambiguous, ai())
            .await
            .unwrap_err()
            .starts_with("ERR_KNOWLEDGE_EDIT_MATCH_COUNT")
    );
    let unrelated = KnowledgeWriteRequest {
        action: "delete".into(),
        document_id: Some(document.id.clone()),
        expected_revision: Some(document.revision),
        body: Some("ignored?".into()),
        ..Default::default()
    };
    assert!(
        runtime
            .write_content(&library, "project", unrelated, ai())
            .await
            .unwrap_err()
            .starts_with("ERR_KNOWLEDGE_FIELD_UNEXPECTED")
    );
    assert_eq!(snapshot(&runtime, &library).await, baseline);
    assert_eq!(
        runtime
            .list_changes(&library, None, 20)
            .await
            .expect("history")
            .len(),
        1
    );
}

/// Verifies every point of a deterministic randomized operation sequence, including restoring restores.
/// 验证确定性随机操作序列的每个历史点，包括还原还原操作。
#[tokio::test]
async fn restore_reproduces_every_content_snapshot_and_retains_history() {
    let (_root, runtime, library) = fixture(KnowledgeIndexMode::Browse).await;
    let mut states = BTreeMap::new();
    states.insert(0, snapshot(&runtime, &library).await);
    // This deterministic generator varies action order without an extra test dependency.
    // 该确定性生成器变化动作顺序，无需额外测试依赖。
    let mut seed = 17u64;
    for turn in 0..24 {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        let page = runtime
            .list_content(&library, "", 3, None, MAX_KNOWLEDGE_PAGE_SIZE)
            .await
            .expect("list");
        let documents: Vec<_> = page
            .entries
            .iter()
            .filter_map(|e| e.document.as_ref())
            .collect();
        let receipt = if documents.is_empty() || seed.is_multiple_of(4) {
            runtime
                .write_content(
                    &library,
                    "project",
                    create(&format!("docs/{turn}.md"), &format!("# Item\n{seed}\n")),
                    ai(),
                )
                .await
                .expect("random create")
        } else {
            let chosen = documents
                .iter()
                .find(|d| d.id == documents.iter().min_by_key(|d| &d.id).expect("nonempty").id)
                .expect("semantic document");
            let request = match seed % 4 {
                1 => KnowledgeWriteRequest {
                    action: "update".into(),
                    document_id: Some(chosen.id.clone()),
                    expected_revision: Some(chosen.revision),
                    body: Some(format!("new {seed}\n")),
                    ..Default::default()
                },
                2 => KnowledgeWriteRequest {
                    action: "move".into(),
                    document_id: Some(chosen.id.clone()),
                    expected_revision: Some(chosen.revision),
                    new_path: Some(format!("moved/{turn}.md")),
                    ..Default::default()
                },
                _ => KnowledgeWriteRequest {
                    action: "delete".into(),
                    document_id: Some(chosen.id.clone()),
                    expected_revision: Some(chosen.revision),
                    ..Default::default()
                },
            };
            runtime
                .write_content(&library, "project", request, ai())
                .await
                .expect("random mutation")
        };
        states.insert(
            receipt.change_seq.expect("one change"),
            snapshot(&runtime, &library).await,
        );
    }
    // Restore points remain stable even as the restore operations append new history.
    // 还原操作追加新历史时，历史还原点保持稳定。
    let history_before = runtime
        .list_changes(&library, None, MAX_KNOWLEDGE_PAGE_SIZE)
        .await
        .expect("history")
        .len();
    for (target, expected) in &states {
        runtime
            .restore_content(&library, *target, human(), "verify historical state")
            .await
            .expect("restore target");
        assert_eq!(
            &snapshot(&runtime, &library).await,
            expected,
            "restore point {target}"
        );
    }
    assert_eq!(
        runtime
            .list_changes(&library, None, MAX_KNOWLEDGE_PAGE_SIZE)
            .await
            .expect("history")
            .len(),
        history_before + states.len()
    );
    // A restore itself is an ordinary retained change and can be undone to a previous restore.
    // 还原本身也是普通保留变更，可以撤销到之前的还原。
    let latest = runtime
        .list_changes(&library, None, MAX_KNOWLEDGE_PAGE_SIZE)
        .await
        .expect("history");
    let restore = latest
        .iter()
        .filter(|c| c.operation == "restore")
        .min_by_key(|c| c.seq)
        .expect("first restore");
    runtime
        .restore_content(&library, restore.seq, human(), "restore a restore")
        .await
        .expect("restore restore");
    assert_eq!(
        snapshot(&runtime, &library).await,
        *states.get(&0).expect("empty snapshot")
    );
}

/// Verifies restoring a deleted document invalidates every token issued before deletion.
/// 验证还原已删除文档使删除前签发的全部令牌失效。
#[tokio::test]
async fn restore_preserves_identity_without_revision_aba() {
    let (_root, runtime, library) = fixture(KnowledgeIndexMode::Browse).await;
    let document = add(&runtime, &library, "a.md", "original").await;
    let creation = runtime
        .list_changes(&library, None, 20)
        .await
        .expect("creation")
        .into_iter()
        .find(|c| c.operation == "create")
        .expect("create change")
        .seq;
    runtime
        .write_content(
            &library,
            "project",
            KnowledgeWriteRequest {
                action: "delete".into(),
                document_id: Some(document.id.clone()),
                expected_revision: Some(document.revision),
                ..Default::default()
            },
            ai(),
        )
        .await
        .expect("delete");
    runtime
        .restore_content(&library, creation, human(), "recover")
        .await
        .expect("recover");
    let restored = runtime
        .read_document(&library, Some(&document.id), None, None, None)
        .await
        .expect("restored");
    assert!(restored.document.revision > document.revision);
    assert_eq!(restored.document.id, document.id);
    assert!(
        runtime
            .write_content(
                &library,
                "project",
                KnowledgeWriteRequest {
                    action: "update".into(),
                    document_id: Some(document.id),
                    expected_revision: Some(document.revision),
                    body: Some("stale".into()),
                    ..Default::default()
                },
                ai()
            )
            .await
            .unwrap_err()
            .starts_with("ERR_KNOWLEDGE_REVISION_CONFLICT")
    );
}

/// Creates a host-decoded file candidate with a real source-content digest.
/// 创建具有真实来源内容摘要的宿主解码文件候选。
fn source(body: &str) -> KnowledgeIngestDocument {
    KnowledgeIngestDocument {
        path: "files/a.txt".into(),
        title: "Imported".into(),
        body: body.into(),
        summary: None,
        provenance: KnowledgeProvenance {
            kind: "file".into(),
            reference: "D:/fixture/a.txt".into(),
            sha256: Some(super::super::sha256_hex(body.as_bytes())),
            observed_at_ms: 1,
        },
        embed: KnowledgeEmbedPolicy::None,
    }
}

/// Verifies exact decoded bytes, unchanged no-op ingestion, stale summaries and one batch change.
/// 验证精确解码字节、无变化搬运、过期摘要与单批变更。
#[tokio::test]
async fn ingest_preserves_bytes_and_summary_lifecycle() {
    let (_root, runtime, library) = fixture(KnowledgeIndexMode::Browse).await;
    let first = runtime
        .ingest_documents(
            &library,
            "project",
            vec![source("# Source\r\n原文\r\n")],
            ai(),
            "ingest",
        )
        .await
        .expect("ingest");
    let document = first
        .documents
        .iter()
        .find(|d| d.path == "files/a.txt")
        .expect("imported document");
    assert_eq!(document.summary_state, "pending");
    assert_eq!(
        runtime
            .read_document(&library, Some(&document.id), None, None, None)
            .await
            .expect("read")
            .content,
        "# Source\r\n原文\r\n"
    );
    let no_op = runtime
        .ingest_documents(
            &library,
            "project",
            vec![source("# Source\r\n原文\r\n")],
            ai(),
            "unchanged",
        )
        .await
        .expect("no-op");
    assert_eq!(no_op.change_seq, None);
    runtime
        .write_content(
            &library,
            "project",
            KnowledgeWriteRequest {
                action: "update".into(),
                document_id: Some(document.id.clone()),
                expected_revision: Some(document.revision),
                summary: Some("AI supplied".into()),
                ..Default::default()
            },
            ai(),
        )
        .await
        .expect("summary");
    let changed = runtime
        .ingest_documents(
            &library,
            "project",
            vec![source("# Source\r\nchanged\r\n")],
            ai(),
            "source changed",
        )
        .await
        .expect("changed ingest");
    let updated = changed
        .documents
        .iter()
        .find(|d| d.id == document.id)
        .expect("stable document");
    assert_eq!(updated.summary_state, "stale");
    assert!(updated.revision > document.revision);
    assert_eq!(
        runtime
            .list_changes(&library, None, 20)
            .await
            .expect("history")
            .len(),
        3
    );
}

/// Verifies a cursor cannot cross a mutation and line windows preserve boundaries.
/// 验证游标不能跨越变更，且行窗口保留边界。
#[tokio::test]
async fn listing_and_read_windows_are_bounded_and_consistent() {
    let (_root, runtime, library) = fixture(KnowledgeIndexMode::Browse).await;
    add(&runtime, &library, "a.md", "one\ntwo\nthree\n").await;
    add(&runtime, &library, "b.md", "body").await;
    let page = runtime
        .list_content(&library, "", 1, None, 1)
        .await
        .expect("first page");
    let cursor = page.next_cursor.expect("continuation");
    let window = runtime
        .read_document(&library, None, Some("a.md"), Some(2), Some(2))
        .await
        .expect("middle window");
    assert_eq!(window.content, "two\n");
    assert_eq!(window.next_line, Some(3));
    add(&runtime, &library, "c.md", "body").await;
    assert!(
        runtime
            .list_content(&library, "", 1, Some(&cursor), 1)
            .await
            .unwrap_err()
            .starts_with("ERR_KNOWLEDGE_CURSOR_STALE")
    );
    assert!(
        runtime
            .read_document(&library, None, Some("a.md"), Some(0), None)
            .await
            .is_err()
    );
}

/// Verifies old format refusal is read-only and unsupported libraries can still be deleted.
/// 验证旧格式拒绝为只读，且不支持格式的库仍可删除。
#[tokio::test]
async fn unsupported_format_is_explicit_and_deletable() {
    let (_root, runtime, library) = fixture(KnowledgeIndexMode::Browse).await;
    let manifest_path = runtime
        .library_root(&library)
        .expect("root")
        .join("manifest.json");
    let old = serde_json::json!({"library_id":library,"name":"Fixture","schema_version":1});
    std::fs::write(
        &manifest_path,
        serde_json::to_vec(&old).expect("old manifest"),
    )
    .expect("write old manifest");
    let before = std::fs::read(&manifest_path).expect("before");
    let summary = runtime
        .get_library(&library)
        .await
        .expect("error projection");
    assert_eq!(summary.lifecycle_state, "error");
    assert!(summary.settings.is_none());
    assert!(summary.error_message.is_some());
    assert!(
        runtime
            .write_content(&library, "project", create("a.md", "text"), ai())
            .await
            .is_err()
    );
    assert_eq!(std::fs::read(&manifest_path).expect("after"), before);
    runtime
        .delete_library(&library)
        .await
        .expect("delete unsupported library");
    assert!(runtime.get_library(&library).await.is_err());
}

/// Verifies independent wrappers serialize concurrent optimistic updates and honor revocation.
/// 验证独立包装串行化并发乐观更新并遵守撤销。
#[tokio::test]
async fn shared_host_serializes_writers_and_rechecks_authorization() {
    let (_root, runtime, library) = fixture(KnowledgeIndexMode::Browse).await;
    let document = add(&runtime, &library, "a.md", "original").await;
    let other = KnowledgeBaseRuntime::new(runtime.host_runtime.clone());
    let first = KnowledgeWriteRequest {
        action: "update".into(),
        document_id: Some(document.id.clone()),
        expected_revision: Some(document.revision),
        body: Some("first".into()),
        ..Default::default()
    };
    let second = KnowledgeWriteRequest {
        body: Some("second".into()),
        ..first.clone()
    };
    let (first, second) = tokio::join!(
        runtime.write_content(&library, "project", first, ai()),
        other.write_content(&library, "project", second, ai())
    );
    assert_ne!(first.is_ok(), second.is_ok());
    runtime
        .set_project_binding(&library, "project", false, false)
        .await
        .expect("revoke");
    assert!(
        other
            .write_content(&library, "project", create("denied.md", "text"), ai())
            .await
            .unwrap_err()
            .starts_with("ERR_KNOWLEDGE_NOT_AUTHORIZED")
    );
}

/// Verifies an index write failure rolls back entities, blobs and history together.
/// 验证索引写入失败使实体、正文和历史一起回滚。
#[tokio::test]
async fn derived_index_failure_rolls_back_the_entire_change() {
    let (_root, runtime, library) = fixture(KnowledgeIndexMode::Bm25).await;
    add(&runtime, &library, "existing.md", "existing").await;
    let baseline = snapshot(&runtime, &library).await;
    let (_, binding) = runtime
        .ensure_library_binding(&library)
        .await
        .expect("binding");
    local_sql::with_connection_async(&runtime.host_runtime.local,runtime.resolve_library_binding(binding).expect("resolve"),|connection|{
        connection.execute_batch("CREATE TRIGGER reject_chunk BEFORE INSERT ON kb_chunks BEGIN SELECT RAISE(ABORT,'fixture failure'); END;").map_err(|e|e.to_string())
    }).await.expect("inject derived failure");
    assert!(
        runtime
            .write_content(&library, "project", create("failed.md", "body"), ai())
            .await
            .is_err()
    );
    assert_eq!(snapshot(&runtime, &library).await, baseline);
    assert_eq!(
        runtime
            .list_changes(&library, None, 20)
            .await
            .expect("history")
            .len(),
        1
    );
}

/// Verifies FTS corruption is detected and repaired without altering content history.
/// 验证全文损坏被检测并修复，且不改变内容历史。
#[tokio::test]
async fn fts_integrity_and_dictionary_rebuild_preserve_content_history() {
    let (_root, runtime, library) = fixture(KnowledgeIndexMode::Bm25).await;
    assert!(
        runtime
            .index_integrity(&library)
            .await
            .expect("empty index")
            .healthy
    );
    add(
        &runtime,
        &library,
        "terms.md",
        "# Terms\n火山知识库 customterm\n",
    )
    .await;
    let baseline = snapshot(&runtime, &library).await;
    let (_, binding) = runtime
        .ensure_library_binding(&library)
        .await
        .expect("binding");
    local_sql::with_connection_async(
        &runtime.host_runtime.local,
        runtime.resolve_library_binding(binding).expect("resolve"),
        |connection| {
            connection
                .execute(&format!("DELETE FROM {KNOWLEDGE_FTS_INDEX};"), [])
                .map_err(|e| e.to_string())?;
            Ok(())
        },
    )
    .await
    .expect("inject missing derived rows");
    let damaged = runtime
        .index_integrity(&library)
        .await
        .expect("damaged integrity");
    assert!(!damaged.healthy);
    assert!(damaged.mismatched_rows > 0);
    assert!(
        runtime
            .rebuild_index(&library)
            .await
            .expect("rebuild")
            .healthy
    );
    runtime
        .set_custom_words(
            &library,
            vec![KnowledgeCustomWord {
                word: "customterm".into(),
                weight: 100,
            }],
        )
        .await
        .expect("replace terms");
    assert!(
        runtime
            .list_custom_words(&library)
            .await
            .expect("terms")
            .iter()
            .any(|term| term.word == "customterm")
    );
    let rebuilt = runtime
        .index_integrity(&library)
        .await
        .expect("rebuilt integrity");
    assert!(rebuilt.healthy);
    assert_eq!(
        rebuilt.dictionary_revision,
        rebuilt.indexed_dictionary_revision
    );
    assert_eq!(snapshot(&runtime, &library).await, baseline);
    assert_eq!(
        runtime
            .list_changes(&library, None, 20)
            .await
            .expect("history")
            .len(),
        1
    );
}

/// Verifies implicit directory ancestors exist even for an explicitly empty nested folder.
/// 验证显式空嵌套目录也会产生隐式祖先目录。
#[tokio::test]
async fn explicit_empty_nested_folders_have_browsable_ancestors() {
    let (_root, runtime, library) = fixture(KnowledgeIndexMode::Browse).await;
    runtime
        .write_content(
            &library,
            "project",
            KnowledgeWriteRequest {
                action: "folder".into(),
                path: Some("first/second/third".into()),
                description: Some("Deep purpose".into()),
                ..Default::default()
            },
            ai(),
        )
        .await
        .expect("nested empty folder");
    let root = runtime
        .list_content(&library, "", 1, None, 20)
        .await
        .expect("root");
    assert!(
        root.entries
            .iter()
            .any(|entry| entry.path == "first" && entry.kind == "folder")
    );
    let nested = runtime
        .list_content(&library, "first/second", 1, None, 20)
        .await
        .expect("nested");
    assert!(
        nested
            .entries
            .iter()
            .any(|entry| entry.path == "first/second/third" && entry.description == "Deep purpose")
    );
    let before = snapshot(&runtime, &library).await;
    let point = runtime
        .list_changes(&library, None, 20)
        .await
        .expect("history")
        .into_iter()
        .find(|c| c.operation == "folder")
        .expect("folder change")
        .seq;
    runtime
        .restore_content(&library, 0, human(), "empty")
        .await
        .expect("empty restore");
    runtime
        .restore_content(&library, point, human(), "folder restore")
        .await
        .expect("restore nested folders");
    assert_eq!(snapshot(&runtime, &library).await, before);
}

/// Verifies line references agree with Markdown normalization while original separators remain unchanged.
/// 验证行引用与 Markdown 规范化一致，同时原始分隔符保持不变。
#[tokio::test]
async fn exact_body_windows_preserve_all_supported_newline_forms() {
    // Mixed separators must retain their byte identity through storage and line windows.
    // 混合分隔符经过存储和行窗口后必须保留字节身份。
    let (_root, runtime, library) = fixture(KnowledgeIndexMode::Browse).await;
    for (name, text) in [
        ("cr", "one\rtwo\rthree\r"),
        ("crlf", "one\r\ntwo\r\nthree\r\n"),
        ("mixed", "one\rtwo\nthree\r\n"),
    ] {
        // The path is selected semantically so added fixtures cannot shift row positions.
        // 路径按语义选择，使新增夹具不能改变行位置。
        let document = add(&runtime, &library, &format!("{name}.md"), text).await;
        assert_eq!(document.line_count, 3);
        assert_eq!(
            runtime
                .read_document(&library, Some(&document.id), None, None, None)
                .await
                .expect("full read")
                .content,
            text
        );
        // Window two corresponds to the same logical line regardless of separator encoding.
        // 无论分隔符编码如何，第二行窗口对应相同逻辑行。
        let middle = runtime
            .read_document(&library, Some(&document.id), None, Some(2), Some(2))
            .await
            .expect("middle line");
        assert_eq!(middle.content.trim_end_matches(['\r', '\n']), "two");
        assert_eq!(middle.next_line, Some(3));
    }
}
