//! Cross-domain restart-state and real-controller knowledge runtime regressions.
//! 跨领域重启状态与真实控制器知识库运行时回归。

use super::package_lifecycle::{
    KNOWLEDGE_REPLACEMENT_JOURNAL_VERSION, KNOWLEDGE_REPLACEMENT_PHASE_COMMITTED,
    KNOWLEDGE_REPLACEMENT_PHASE_PREPARED, KnowledgeReplacementJournal,
};

use super::package_archive::tests::write_identity_mismatch_package;
use super::*;
use crate::data_plane::{
    ControllerRuntimeMetadata, build_application_schema_script,
    ensure_application_schema_migrations,
};
use crate::knowledge_base::models::{
    KnowledgeDisplayIndexStatus, KnowledgeOperationKind, KnowledgeVectorIndexStatus,
};
use crate::runtime_assets;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use tempfile::tempdir;
use vldb_controller_client::{
    ClientRegistration, ControllerClient, ControllerClientConfig, ControllerProcessMode,
    ControllerSqliteEnableRequest, SpaceKind, SpaceRegistration,
};

/// Verifies restart recovery preserves published generations and releases abandoned building flags.
/// 验证重启恢复会保留已发布代次并释放遗留构建中标记。
#[test]
fn interrupted_derived_index_statuses_become_actionable() {
    assert_eq!(
        recovered_display_index_status(KnowledgeDisplayIndexStatus::Building, true),
        Some(KnowledgeDisplayIndexStatus::Stale)
    );
    assert_eq!(
        recovered_display_index_status(KnowledgeDisplayIndexStatus::Building, false),
        Some(KnowledgeDisplayIndexStatus::Failed)
    );
    assert_eq!(
        recovered_display_index_status(KnowledgeDisplayIndexStatus::Ready, true),
        None
    );
    assert_eq!(
        recovered_vector_index_status(KnowledgeVectorIndexStatus::Building, true),
        Some(KnowledgeVectorIndexStatus::Stale)
    );
    assert_eq!(
        recovered_vector_index_status(KnowledgeVectorIndexStatus::Building, false),
        Some(KnowledgeVectorIndexStatus::Failed)
    );
}

/// Exercises create, Markdown import, Jieba BM25 search, incremental deletion, and safe removal against a real controller.
/// 使用真实控制器验证创建、Markdown 导入、Jieba BM25 检索、增量删除与安全移除。
#[tokio::test(flavor = "current_thread")]
#[ignore = "requires the bundled output/bin/vldb-controller.exe"]
async fn real_controller_markdown_bm25_end_to_end() {
    let temporary = tempdir().expect("create knowledge end-to-end root");
    let data_root = temporary.path().join("data");
    let source_root = temporary.path().join("source");
    fs::create_dir_all(source_root.join("guide")).expect("create source tree");
    fs::write(
        source_root.join("guide").join("search.md"),
        "# 检索设计\n\n火山知识库使用结巴分词和 BM25 检索。\n\ngetskilllevelup(actor, 10)\n",
    )
    .expect("write Chinese Markdown fixture");
    fs::write(source_root.join("ignored.txt"), "must not import").expect("write ignored fixture");

    let listener =
        std::net::TcpListener::bind("127.0.0.1:0").expect("reserve an ephemeral controller port");
    let port = listener.local_addr().expect("read controller port").port();
    drop(listener);
    let executable = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("output")
        .join("bin")
        .join("vldb-controller.exe");
    assert!(executable.is_file(), "bundled controller must exist");
    let endpoint = format!("http://127.0.0.1:{port}");
    let client = ControllerClient::new(
        ControllerClientConfig {
            endpoint: endpoint.clone(),
            auto_spawn: true,
            spawn_executable: Some(executable.display().to_string()),
            spawn_process_mode: ControllerProcessMode::Managed,
            minimum_uptime_secs: 0,
            idle_timeout_secs: 1,
            default_lease_ttl_secs: 30,
            connect_timeout_secs: 30,
            startup_timeout_secs: 15,
            startup_retry_interval_ms: 100,
            lease_renew_interval_secs: 10,
        },
        ClientRegistration {
            client_name: format!("knowledge-e2e-{}", Uuid::new_v4()),
            host_kind: String::from("test"),
            process_id: std::process::id(),
            process_name: String::from("vulcan-workspace-store-test"),
            lease_ttl_secs: Some(30),
        },
    );
    client.connect().await.expect("connect real controller");
    let app_path = data_root.join("sqlite").join("app-workspace.sqlite3");
    fs::create_dir_all(app_path.parent().expect("app database parent"))
        .expect("create app database parent");
    let app_binding_id = format!("knowledge-e2e-app-{}", Uuid::new_v4());
    client
        .attach_space(SpaceRegistration {
            space_id: String::from(APP_SPACE_ID),
            space_label: String::from("APP"),
            space_kind: SpaceKind::User,
            space_root: runtime_assets::native_host_path_text_from_path(&data_root),
        })
        .await
        .expect("attach app space");
    client
        .enable_sqlite(ControllerSqliteEnableRequest {
            space_id: String::from(APP_SPACE_ID),
            binding_id: app_binding_id.clone(),
            db_path: runtime_assets::native_host_path_text_from_path(&app_path),
            ..ControllerSqliteEnableRequest::default()
        })
        .await
        .expect("enable app database");
    client
        .execute_sqlite_script_typed(
            APP_SPACE_ID,
            &app_binding_id,
            build_application_schema_script(),
            Vec::new(),
        )
        .await
        .expect("initialize app schema");
    ensure_application_schema_migrations(&client, &app_binding_id)
        .await
        .expect("apply app schema migrations");
    let runtime = KnowledgeBaseRuntime::new(ControllerRuntimeHandle::new(
        client.clone(),
        ControllerRuntimeMetadata {
            endpoint,
            executable: executable.display().to_string(),
            data_root,
            sqlite_path: app_path.clone(),
            sqlite_binding_id: app_binding_id.clone(),
            system_statistics_sqlite_path: app_path,
            system_statistics_sqlite_binding_id: app_binding_id,
        },
    ));

    // abandonedExtraction simulates a package TempDir left behind by process termination.
    // abandonedExtraction 模拟进程终止后遗留的知识库包临时解压目录。
    let abandoned_extraction = runtime
        .knowledge_root()
        .join(".package-inspect-crash-fixture");
    fs::create_dir_all(&abandoned_extraction).expect("create abandoned package extraction fixture");
    fs::write(abandoned_extraction.join("content.sqlite3"), b"abandoned")
        .expect("write abandoned package extraction fixture");
    runtime
        .recover_interrupted_jobs()
        .await
        .expect("clean abandoned package extraction at startup");
    assert!(!abandoned_extraction.exists());

    let library = runtime
        .create_library("端到端知识库", "", &[])
        .await
        .expect("create isolated library");
    let library = runtime
        .rename_library(&library.id, "端到端知识库已重命名", "")
        .await
        .expect("replace existing manifest during rename");
    assert_eq!(library.name, "端到端知识库已重命名");
    let first_import = runtime
        .import_markdown_directory(&library.id, &source_root)
        .await
        .expect("import Markdown through Jieba FTS");
    assert_eq!(first_import.scanned_files, 1);
    assert_eq!(first_import.changed_files, 1);
    let completed_job = runtime
        .latest_import_job(&library.id)
        .await
        .expect("load completed import job")
        .expect("completed import job exists");
    assert_eq!(completed_job.operation_kind, KnowledgeOperationKind::Import);
    assert_eq!(completed_job.total_files, 1);
    assert_eq!(completed_job.processed_files, 1);
    assert_eq!(completed_job.current_file, None);
    assert_eq!(
        runtime
            .list_documents(&library.id, None, 20, 0)
            .await
            .expect("list imported documents")
            .len(),
        1
    );
    // SourceSummary separates the authoritative document total from its searchable chunk total.
    // SourceSummary 将权威文档总数与其可检索切片总数分开。
    let source_summary = runtime
        .list_sources(&library.id)
        .await
        .expect("list durable source summary")
        .into_iter()
        .next()
        .expect("source summary exists");
    assert_eq!(source_summary.document_count, 1);
    assert_eq!(source_summary.chunk_count, first_import.chunk_count);
    let initial_integrity = runtime
        .inspect_index_integrity(&library.id)
        .await
        .expect("inspect ready index integrity");
    assert!(initial_integrity.searchable);
    assert_eq!(
        initial_integrity.authoritative_chunk_count,
        first_import.chunk_count
    );
    assert_eq!(
        initial_integrity.indexed_chunk_count,
        first_import.chunk_count
    );
    let search = runtime
        .search(std::slice::from_ref(&library.id), "火山知识库", 10)
        .await
        .expect("search Chinese content");
    assert!(!search.hits.is_empty());
    assert!(!search.fts_queries[0].expression.is_empty());
    // independentRuntime reproduces the native AI tool path that reconstructs the runtime from the same controller handle.
    // independentRuntime 复现原生 AI 工具使用同一控制器句柄重新构造运行时的路径。
    let independent_runtime = KnowledgeBaseRuntime::new(runtime.controller.clone());
    // independentSearch must reuse the controller binding instead of reopening the locked SQLite file.
    // independentSearch 必须复用控制器绑定，而不是重新打开已经锁定的 SQLite 文件。
    let independent_search = independent_runtime
        .search(std::slice::from_ref(&library.id), "火山知识库", 10)
        .await
        .expect("search Chinese content through independent runtime");
    assert!(!independent_search.hits.is_empty());
    // thirdRuntime exposes controller state corruption after one same-binding re-enable.
    // thirdRuntime 用于暴露同一绑定重复启用一次后的控制器状态损坏。
    let third_runtime = KnowledgeBaseRuntime::new(runtime.controller.clone());
    // thirdSearch must remain idempotent even after the binding has already been enabled twice.
    // thirdSearch 必须在绑定已经启用两次后仍保持幂等。
    let third_search = third_runtime
        .search(std::slice::from_ref(&library.id), "火山知识库", 10)
        .await
        .expect("search Chinese content through third runtime");
    assert!(
        third_search.failures.is_empty(),
        "third runtime search failures: {:?}",
        third_search.failures
    );
    assert!(!third_search.hits.is_empty());
    let substring_search = runtime
        .search(std::slice::from_ref(&library.id), "levelup", 10)
        .await
        .expect("supplement Jieba BM25 with identifier substring matches");
    assert_eq!(substring_search.hits.len(), 1);
    assert_eq!(substring_search.hits[0].match_kind, "substring");

    // LegacyEncodingLibrary isolates GBK ingestion from the UTF-8 incremental fixtures below.
    // LegacyEncodingLibrary 将 GBK 导入与下方 UTF-8 增量夹具隔离。
    let legacy_encoding_root = temporary.path().join("legacy-encoding-source");
    fs::create_dir_all(&legacy_encoding_root).expect("create legacy encoding source root");
    let legacy_markdown = "# 服务端等级\n\n服务端用户等级获取规则使用 GBK 文档。\n";
    let (legacy_bytes, _, legacy_encode_had_errors) = encoding_rs::GBK.encode(legacy_markdown);
    assert!(!legacy_encode_had_errors);
    fs::write(
        legacy_encoding_root.join("server-level.md"),
        legacy_bytes.as_ref(),
    )
    .expect("write GBK Markdown fixture");
    let legacy_encoding_library = runtime
        .create_library("GBK 知识库", "", &[])
        .await
        .expect("create GBK library");
    let legacy_encoding_import = runtime
        .import_markdown_directory(&legacy_encoding_library.id, &legacy_encoding_root)
        .await
        .expect("import GBK Markdown through the shared decoder");
    assert_eq!(legacy_encoding_import.changed_files, 1);
    let legacy_encoding_search = runtime
        .search(
            std::slice::from_ref(&legacy_encoding_library.id),
            "用户等级",
            10,
        )
        .await
        .expect("search GBK Markdown through Jieba BM25");
    assert!(!legacy_encoding_search.hits.is_empty());
    runtime
        .delete_library(&legacy_encoding_library.id)
        .await
        .expect("delete GBK library");

    // ImportTimestamp must describe content ingestion rather than later derived-index maintenance.
    // ImportTimestamp 必须描述内容导入，而不是后续派生索引维护。
    let import_timestamp = runtime
        .get_library(&library.id)
        .await
        .expect("load successful import timestamp")
        .last_completed_import_at_ms;
    assert!(import_timestamp.is_some());
    let rebuilt_rows = runtime
        .rebuild_index(&library.id)
        .await
        .expect("manually rebuild Jieba index through parameter-safe statements");
    assert_eq!(rebuilt_rows, first_import.chunk_count);
    let rebuilt_library = runtime
        .get_library(&library.id)
        .await
        .expect("load registry counts after manual rebuild");
    assert_eq!(rebuilt_library.document_count, 1);
    assert_eq!(rebuilt_library.chunk_count, first_import.chunk_count);
    assert_eq!(
        rebuilt_library.last_completed_import_at_ms,
        import_timestamp
    );

    // KnowledgeBinding identifies the exact physical database used by the recovery fixture.
    // KnowledgeBinding 标识恢复夹具使用的准确物理数据库。
    let (knowledge_space_id, knowledge_binding_id) = runtime
        .ensure_library_binding(&library.id)
        .await
        .expect("resolve library binding for recovery fixture");
    client
        .execute_sqlite_script_typed(
            &knowledge_space_id,
            &knowledge_binding_id,
            "UPDATE knowledge_metadata SET value = 'dirty' WHERE key = 'index_status'; UPDATE knowledge_metadata SET value = '0' WHERE key = 'indexed_content_revision';",
            Vec::new(),
        )
        .await
        .expect("mark index dirty for recovery import");
    // RecoveryImport must detect unchanged documents while repairing only derived FTS state.
    // RecoveryImport 必须识别未变化文档，同时仅修复派生 FTS 状态。
    let recovery_import = runtime
        .import_markdown_directory(&library.id, &source_root)
        .await
        .expect("recover dirty FTS through cancellable batches");
    assert_eq!(recovery_import.changed_files, 0);
    // RecoveredJob exposes the completed batched index counters.
    // RecoveredJob 暴露已完成的批量索引计数。
    let recovered_job = runtime
        .latest_import_job(&library.id)
        .await
        .expect("load recovered import job")
        .expect("recovered import job exists");
    assert!(recovered_job.total_index_items > 0);
    assert_eq!(
        recovered_job.processed_index_items,
        recovered_job.total_index_items
    );
    assert!(
        !runtime
            .search(std::slice::from_ref(&library.id), "火山知识库", 10)
            .await
            .expect("search recovered index")
            .hits
            .is_empty()
    );

    let package_directory = temporary.path().join("packages");
    fs::create_dir(&package_directory).expect("create package export directory");
    let export = runtime
        .export_package(&library.id, &package_directory)
        .await
        .expect("export complete ready library");
    // tamperedIndexBinding keeps ready metadata while removing every physical FTS row.
    // tamperedIndexBinding 在保留 ready 元数据的同时删除全部物理 FTS 记录。
    let (tampered_index_space_id, tampered_index_binding_id) = runtime
        .ensure_library_binding(&library.id)
        .await
        .expect("activate library for FTS integrity fixture");
    client
        .execute_sqlite_script_typed(
            &tampered_index_space_id,
            &tampered_index_binding_id,
            format!("DELETE FROM {KNOWLEDGE_FTS_INDEX};"),
            Vec::new(),
        )
        .await
        .expect("remove FTS rows without changing ready metadata");
    assert!(
        runtime
            .export_package(&library.id, &package_directory)
            .await
            .is_err(),
        "export must reject ready metadata without exact FTS rows"
    );
    let broken_integrity = runtime
        .inspect_index_integrity(&library.id)
        .await
        .expect("inspect physically incomplete index");
    assert!(!broken_integrity.searchable);
    assert_eq!(broken_integrity.indexed_chunk_count, 0);
    assert_eq!(
        broken_integrity.missing_or_mismatched_count,
        first_import.chunk_count
    );
    runtime
        .repair_index(&library.id)
        .await
        .expect("repair missing FTS rows after integrity fixture");
    assert!(
        runtime
            .inspect_index_integrity(&library.id)
            .await
            .expect("inspect repaired index")
            .searchable
    );
    // DuplicateFixture proves integrity accounting and repair operate on physical row cardinality.
    // DuplicateFixture 证明完整性计数与补全会处理物理记录基数。
    client
        .execute_sqlite_script_typed(
            &tampered_index_space_id,
            &tampered_index_binding_id,
            format!(
                "INSERT INTO {KNOWLEDGE_FTS_INDEX}(id, file_path, title, content) SELECT id, file_path, title, content FROM {KNOWLEDGE_FTS_INDEX} LIMIT 1;"
            ),
            Vec::new(),
        )
        .await
        .expect("insert one duplicate physical FTS row");
    let duplicate_integrity = runtime
        .inspect_index_integrity(&library.id)
        .await
        .expect("inspect duplicate physical FTS row");
    assert!(!duplicate_integrity.searchable);
    assert_eq!(
        duplicate_integrity.indexed_chunk_count,
        first_import.chunk_count + 1
    );
    assert_eq!(duplicate_integrity.orphan_or_mismatched_count, 1);
    runtime
        .repair_index(&library.id)
        .await
        .expect("remove duplicate physical FTS row during repair");
    assert!(
        runtime
            .inspect_index_integrity(&library.id)
            .await
            .expect("inspect index repaired from duplicate row")
            .searchable
    );
    // DetachedRebuild proves the operation continues after the initiating call returns.
    // DetachedRebuild 证明操作在发起调用返回后仍会继续执行。
    let detached_rebuild = runtime
        .start_rebuild_index(&library.id)
        .await
        .expect("start detached rebuild");
    assert_eq!(
        detached_rebuild.operation_kind,
        KnowledgeOperationKind::Rebuild
    );
    assert_eq!(detached_rebuild.state, "queued");
    // TerminalJob is observed only through durable polling, mirroring a refreshed WebUI.
    // TerminalJob 仅通过持久化轮询观察，与刷新后的 WebUI 行为一致。
    let terminal_rebuild = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let job = runtime
                .latest_import_job(&library.id)
                .await
                .expect("poll detached rebuild")
                .expect("detached rebuild job exists");
            if job.id == detached_rebuild.id && job.state == "completed" {
                break job;
            }
            assert_ne!(job.state, "failed", "detached rebuild must not fail");
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .expect("detached rebuild reaches terminal state");
    assert_eq!(
        terminal_rebuild.processed_index_items,
        first_import.chunk_count
    );
    // identityMismatchPath contains valid hashes but forged package/manifest provenance.
    // identityMismatchPath 包含有效摘要，但伪造了包与清单来源身份。
    let identity_mismatch_path = package_directory.join("identity-mismatch.vkb");
    write_identity_mismatch_package(Path::new(&export.archive_path), &identity_mismatch_path);
    assert!(
        runtime
            .inspect_package(&identity_mismatch_path)
            .await
            .is_err(),
        "full package inspection must reject a database identity mismatch"
    );
    let inspection = runtime
        .inspect_package(Path::new(&export.archive_path))
        .await
        .expect("inspect exported package through real SQLite validation");
    assert_eq!(inspection.package.document_count, 1);
    assert_eq!(
        inspection.conflicting_library.as_ref().map(|row| &row.id),
        Some(&library.id)
    );

    let imported = runtime
        .import_package_as_new(Path::new(&export.archive_path), "端到端知识库包副本")
        .await
        .expect("import package under a unique name");
    assert_ne!(imported.library.id, library.id);
    assert_eq!(
        imported.library.document_count,
        export.package.document_count
    );
    assert!(
        !runtime
            .search(std::slice::from_ref(&imported.library.id), "火山知识库", 10)
            .await
            .expect("search imported package without rebuilding")
            .hits
            .is_empty()
    );
    let replacement_target = runtime
        .create_library("覆盖目标知识库", "", &[])
        .await
        .expect("create replacement target");
    client
        .execute_sqlite_script_typed(
            APP_SPACE_ID,
            &runtime.controller.sqlite_binding_id,
            r#"INSERT INTO projects(
                    id, name, root_path, pinned, archived, sort_order, created_at, updated_at
                ) VALUES (?, ?, ?, 0, 0, 0, ?, ?);"#,
            vec![
                text("package-replacement-project"),
                text("Package replacement project"),
                text(
                    runtime
                        .controller
                        .data_root
                        .join("replacement-project")
                        .display()
                        .to_string(),
                ),
                text("2026-07-21T00:00:00Z"),
                text("2026-07-21T00:00:00Z"),
            ],
        )
        .await
        .expect("insert replacement project fixture");
    runtime
        .set_project_binding(
            &replacement_target.id,
            "package-replacement-project",
            true,
            true,
        )
        .await
        .expect("bind replacement target to project");
    let replaced = runtime
        .replace_library_from_package(
            Path::new(&export.archive_path),
            &replacement_target.id,
            "覆盖目标知识库",
        )
        .await
        .expect("explicitly replace target from package");
    assert_eq!(replaced.library.id, replacement_target.id);
    assert_eq!(
        runtime
            .list_bindings_for_project("package-replacement-project")
            .await
            .expect("load preserved replacement binding")
            .len(),
        1
    );
    assert!(
        !runtime
            .search(
                std::slice::from_ref(&replacement_target.id),
                "火山知识库",
                10,
            )
            .await
            .expect("search replaced package without rebuilding")
            .hits
            .is_empty()
    );
    // preparedCrash simulates process termination after the original directory becomes a backup.
    // preparedCrash 模拟原目录已成为备份后发生进程终止。
    let prepared_crash_previous = runtime
        .get_library(&replacement_target.id)
        .await
        .expect("load replacement target before prepared crash");
    let prepared_crash_operation_id = Uuid::new_v4().to_string();
    let (prepared_crash_backup, prepared_crash_journal_path, prepared_crash_backup_name) = runtime
        .replacement_operation_artifacts(&replacement_target.id, &prepared_crash_operation_id)
        .expect("derive prepared crash artifacts");
    let prepared_crash_journal = KnowledgeReplacementJournal {
        version: KNOWLEDGE_REPLACEMENT_JOURNAL_VERSION,
        target_library_id: replacement_target.id.clone(),
        backup_directory_name: prepared_crash_backup_name,
        phase: String::from(KNOWLEDGE_REPLACEMENT_PHASE_PREPARED),
        previous: prepared_crash_previous,
    };
    runtime
        .write_replacement_journal_atomic(&prepared_crash_journal_path, &prepared_crash_journal)
        .expect("persist prepared crash journal");
    runtime
        .release_library_binding(&replacement_target.id)
        .await
        .expect("release replacement target before prepared crash");
    let prepared_crash_root = runtime
        .library_root(&replacement_target.id)
        .expect("resolve prepared crash target root");
    fs::rename(&prepared_crash_root, &prepared_crash_backup)
        .expect("simulate prepared crash after backup rename");
    // preparedRecovery exercises durable recovery through a new wrapper sharing the same controller binding state.
    // preparedRecovery 通过共享同一控制器绑定状态的新包装实例验证持久恢复。
    let prepared_recovery = KnowledgeBaseRuntime::new(runtime.controller.clone());
    prepared_recovery
        .recover_interrupted_jobs()
        .await
        .expect("recover prepared replacement crash");
    assert!(prepared_crash_root.is_dir());
    assert!(!prepared_crash_backup.exists());
    assert!(!prepared_crash_journal_path.exists());
    assert!(
        !prepared_recovery
            .search(
                std::slice::from_ref(&replacement_target.id),
                "火山知识库",
                10,
            )
            .await
            .expect("search target after prepared crash recovery")
            .hits
            .is_empty()
    );
    prepared_recovery
        .release_library_binding(&replacement_target.id)
        .await
        .expect("release prepared recovery target binding");

    // committedCrash simulates termination after registry commit but before backup cleanup.
    // committedCrash 模拟注册表提交后、备份清理前发生进程终止。
    let committed_crash_previous = runtime
        .get_library(&replacement_target.id)
        .await
        .expect("load replacement target before committed crash");
    let (committed_space_id, committed_binding_id) = runtime
        .ensure_library_binding(&replacement_target.id)
        .await
        .expect("activate replacement target before committed crash");
    client
        .execute_sqlite_script_typed(
            &committed_space_id,
            &committed_binding_id,
            "PRAGMA wal_checkpoint(TRUNCATE);",
            Vec::new(),
        )
        .await
        .expect("checkpoint replacement target before committed crash");
    runtime
        .release_library_binding(&replacement_target.id)
        .await
        .expect("release replacement target before committed crash");
    let committed_crash_operation_id = Uuid::new_v4().to_string();
    let (committed_crash_backup, committed_crash_journal_path, committed_crash_backup_name) =
        runtime
            .replacement_operation_artifacts(&replacement_target.id, &committed_crash_operation_id)
            .expect("derive committed crash artifacts");
    fs::create_dir(&committed_crash_backup).expect("create committed crash backup");
    fs::copy(
        prepared_crash_root.join("manifest.json"),
        committed_crash_backup.join("manifest.json"),
    )
    .expect("copy committed crash manifest backup");
    fs::copy(
        prepared_crash_root.join("content.sqlite3"),
        committed_crash_backup.join("content.sqlite3"),
    )
    .expect("copy committed crash database backup");
    let committed_crash_journal = KnowledgeReplacementJournal {
        version: KNOWLEDGE_REPLACEMENT_JOURNAL_VERSION,
        target_library_id: replacement_target.id.clone(),
        backup_directory_name: committed_crash_backup_name,
        phase: String::from(KNOWLEDGE_REPLACEMENT_PHASE_COMMITTED),
        previous: committed_crash_previous,
    };
    runtime
        .write_replacement_journal_atomic(&committed_crash_journal_path, &committed_crash_journal)
        .expect("persist committed crash journal");
    let committed_recovery = KnowledgeBaseRuntime::new(runtime.controller.clone());
    committed_recovery
        .recover_interrupted_jobs()
        .await
        .expect("recover committed replacement crash");
    assert!(!committed_crash_backup.exists());
    assert!(!committed_crash_journal_path.exists());
    assert_eq!(
        committed_recovery
            .list_bindings_for_project("package-replacement-project")
            .await
            .expect("load binding after committed crash recovery")
            .len(),
        1
    );
    committed_recovery
        .release_library_binding(&replacement_target.id)
        .await
        .expect("release committed recovery target binding");

    // failedReplacement injects a database-identity failure while validating the new staging copy.
    // failedReplacement 在校验新的暂存副本期间注入数据库身份失败。
    let failed_replacement = runtime
        .replace_library_from_package(
            &identity_mismatch_path,
            &replacement_target.id,
            "覆盖目标知识库",
        )
        .await;
    assert!(failed_replacement.is_err());
    assert!(
        !runtime
            .search(
                std::slice::from_ref(&replacement_target.id),
                "火山知识库",
                10,
            )
            .await
            .expect("search original target after failed replacement rollback")
            .hits
            .is_empty()
    );
    assert_eq!(
        runtime
            .list_bindings_for_project("package-replacement-project")
            .await
            .expect("load binding after failed replacement rollback")
            .len(),
        1
    );

    // OptionalPerfSource measures the production-size host path only when explicitly requested.
    // OptionalPerfSource 仅在显式请求时测量生产规模宿主路径。
    if let Some(performance_source) = std::env::var_os("VULCAN_KNOWLEDGE_PERF_SOURCE") {
        // PerformanceLibrary remains isolated from deterministic fixtures and is deleted before teardown.
        // PerformanceLibrary 与确定性夹具隔离，并在清理前删除。
        let performance_library = runtime
            .create_library("知识库集合式索引性能测试", "", &[])
            .await
            .expect("create performance library");
        let performance_started_at = Instant::now();
        let performance_report = runtime
            .import_markdown_directory(&performance_library.id, Path::new(&performance_source))
            .await
            .expect("import production-size performance source");
        let performance_elapsed = performance_started_at.elapsed();
        eprintln!(
            "knowledge performance: files={}, chunks={}, elapsed_ms={}",
            performance_report.scanned_files,
            performance_report.chunk_count,
            performance_elapsed.as_millis()
        );
        assert!(performance_report.scanned_files > 0);
        // SearchTiming verifies the formerly slow production keyword path after tokenizer caching.
        // SearchTiming 在 tokenizer 缓存后验证此前缓慢的真实关键词路径。
        let search_started_at = Instant::now();
        let performance_search = runtime
            .search(std::slice::from_ref(&performance_library.id), "等级", 20)
            .await
            .expect("search production keyword through Jieba BM25");
        let search_elapsed = search_started_at.elapsed();
        eprintln!(
            "knowledge search performance: query=等级, hits={}, elapsed_ms={}",
            performance_search.hits.len(),
            search_elapsed.as_millis()
        );
        assert!(!performance_search.hits.is_empty());
        assert!(
            search_elapsed < Duration::from_secs(3),
            "production keyword search exceeded three seconds: {search_elapsed:?}"
        );
        // RebuildTiming covers the same authoritative corpus through the progress-aware replay path.
        // RebuildTiming 通过带进度的回放路径覆盖同一份权威语料。
        let rebuild_started_at = Instant::now();
        let rebuilt_chunks = runtime
            .rebuild_index(&performance_library.id)
            .await
            .expect("rebuild production-size Jieba index");
        let rebuild_elapsed = rebuild_started_at.elapsed();
        eprintln!(
            "knowledge rebuild performance: chunks={}, elapsed_ms={}",
            rebuilt_chunks,
            rebuild_elapsed.as_millis()
        );
        assert_eq!(rebuilt_chunks, performance_report.chunk_count);
        assert!(
            runtime
                .inspect_index_integrity(&performance_library.id)
                .await
                .expect("inspect performance library")
                .searchable
        );
        runtime
            .delete_library(&performance_library.id)
            .await
            .expect("delete performance library");
    }

    runtime
        .delete_library(&imported.library.id)
        .await
        .expect("delete imported package copy");
    runtime
        .delete_library(&replacement_target.id)
        .await
        .expect("delete replacement target");

    fs::remove_file(source_root.join("guide").join("search.md")).expect("remove imported source");
    let second_import = runtime
        .import_markdown_directory(&library.id, &source_root)
        .await
        .expect("synchronize source deletion");
    assert_eq!(second_import.deleted_files, 1);
    assert!(
        runtime
            .search(std::slice::from_ref(&library.id), "火山知识库", 10)
            .await
            .expect("search rebuilt empty index")
            .hits
            .is_empty()
    );
    runtime
        .delete_library(&library.id)
        .await
        .expect("delete isolated library");

    // multiSourceLibrary isolates multi-root and source-remapping assertions from package fixtures.
    // multiSourceLibrary 将多根目录与来源重新映射断言和知识库包夹具隔离。
    let multi_source_library = runtime
        .create_library("多来源增量知识库", "", &[])
        .await
        .expect("create multi-source library");
    // sourceARoot is the first independently tracked Markdown root.
    // sourceARoot 是第一个独立跟踪的 Markdown 根目录。
    let source_a_root = temporary.path().join("source-a");
    // sourceBRoot is the second independently tracked Markdown root.
    // sourceBRoot 是第二个独立跟踪的 Markdown 根目录。
    let source_b_root = temporary.path().join("source-b");
    // relocatedSourceARoot simulates the first source being moved to another disk location.
    // relocatedSourceARoot 模拟第一个来源移动到另一个磁盘位置。
    let relocated_source_a_root = temporary.path().join("source-a-relocated");
    fs::create_dir_all(&source_a_root).expect("create first source root");
    fs::create_dir_all(&source_b_root).expect("create second source root");
    fs::create_dir_all(&relocated_source_a_root).expect("create relocated source root");
    fs::write(source_a_root.join("shared.md"), "# 来源 A\n\n稳定内容。\n")
        .expect("write first source Markdown");
    fs::write(source_b_root.join("shared.md"), "# 来源 B\n\n独立内容。\n")
        .expect("write second source Markdown");
    runtime
        .import_markdown_directory(&multi_source_library.id, &source_a_root)
        .await
        .expect("import first source root");
    runtime
        .import_markdown_directory(&multi_source_library.id, &source_b_root)
        .await
        .expect("import second source root");
    // sourceARootText is the canonical host representation persisted in knowledge_sources.
    // sourceARootText 是持久化到 knowledge_sources 的规范宿主表示。
    let source_a_root_text = runtime_assets::native_host_path_text_from_path(
        &fs::canonicalize(&source_a_root).expect("canonicalize first source root"),
    );
    // initialSources proves two independent roots coexist even with the same relative file path.
    // initialSources 证明即使相对文件路径相同，两个独立根目录仍可共存。
    let initial_sources = runtime
        .list_sources(&multi_source_library.id)
        .await
        .expect("list initial multi-source roots");
    assert_eq!(initial_sources.len(), 2);
    assert!(
        initial_sources
            .iter()
            .all(|source| source.document_count == 1)
    );
    // sourceA retains the durable identity that must survive a physical path remap.
    // sourceA 保留在物理路径重新映射后仍必须不变的持久化身份。
    let source_a = initial_sources
        .iter()
        .find(|source| source.canonical_root == source_a_root_text)
        .expect("find first durable source")
        .clone();
    fs::copy(
        source_a_root.join("shared.md"),
        relocated_source_a_root.join("shared.md"),
    )
    .expect("copy unchanged Markdown into relocated root");
    // relocationReport proves byte-identical content is skipped by SHA-256 after remapping.
    // relocationReport 证明重新映射后字节完全相同的内容会被 SHA-256 跳过。
    let relocation_report = runtime
        .import_markdown_directory_for_source(
            &multi_source_library.id,
            Some(&source_a.id),
            &relocated_source_a_root,
        )
        .await
        .expect("remap first source and run incremental synchronization");
    assert_eq!(relocation_report.changed_files, 0);
    assert_eq!(relocation_report.unchanged_files, 1);
    assert_eq!(relocation_report.deleted_files, 0);
    // relocatedRootText is the new canonical path expected in the durable source row.
    // relocatedRootText 是持久化来源记录中预期的新规范路径。
    let relocated_root_text = runtime_assets::native_host_path_text_from_path(
        &fs::canonicalize(&relocated_source_a_root).expect("canonicalize relocated source root"),
    );
    // remappedSources verifies only the selected source identity changed its physical root.
    // remappedSources 验证只有选中的来源身份修改了物理根目录。
    let remapped_sources = runtime
        .list_sources(&multi_source_library.id)
        .await
        .expect("list remapped source roots");
    assert_eq!(remapped_sources.len(), 2);
    assert_eq!(
        remapped_sources
            .iter()
            .find(|source| source.id == source_a.id)
            .expect("find remapped durable source")
            .canonical_root,
        relocated_root_text
    );
    // conflictingRemap proves one physical root cannot be assigned to two durable sources.
    // conflictingRemap 证明一个物理根目录不能分配给两个持久化来源。
    let conflicting_remap = runtime
        .import_markdown_directory_for_source(
            &multi_source_library.id,
            Some(&source_a.id),
            &source_b_root,
        )
        .await;
    assert!(conflicting_remap.is_err());
    fs::write(
        relocated_source_a_root.join("shared.md"),
        "# 来源 A\n\n内容已经变化。\n",
    )
    .expect("change relocated Markdown content");
    // changedHashReport proves only the modified source document is reparsed and reindexed.
    // changedHashReport 证明只有内容变化的来源文档会重新解析并建立索引。
    let changed_hash_report = runtime
        .import_markdown_directory_for_source(
            &multi_source_library.id,
            Some(&source_a.id),
            &relocated_source_a_root,
        )
        .await
        .expect("synchronize changed relocated source");
    assert_eq!(changed_hash_report.changed_files, 1);
    assert_eq!(changed_hash_report.unchanged_files, 0);
    runtime
        .delete_library(&multi_source_library.id)
        .await
        .expect("delete multi-source library");
    client.shutdown().await.expect("shutdown controller client");
}
