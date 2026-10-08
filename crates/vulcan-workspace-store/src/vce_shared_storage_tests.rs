//! Isolated reuse and bounded-retention regression fixtures.
//! 隔离的复用与有界保留回归夹具。

use super::{artifact_retention, shared_artifacts, shared_manifest_store, shared_result_store};
use rusqlite::{Connection, params};
use vulcan_context_engine::{
    EmbeddingSpace, VceCodeNodeSnapshot, VceEmbeddingBatchRecord, VceEmbeddingBatchResult,
    VceEmbeddingBatchState, VceEmbeddingRunContract, VceEmbeddingVectorResult,
};

/// Open an in-memory database with the real project schema and one valid binding.
/// 使用真实项目结构和一个合法绑定打开内存数据库。
/// No parameters or user resources; returns the isolated connection.
/// 无参数且不使用用户资源；返回隔离连接。
fn database() -> Connection {
    let connection = Connection::open_in_memory().unwrap();
    connection.execute_batch("PRAGMA foreign_keys=ON").unwrap();
    connection
        .execute_batch(&super::build_vce_schema_script())
        .unwrap();
    connection.execute("INSERT INTO vce_project_bindings VALUES ('n','p','root',?,'chain',1,'space','producer','prep','chunks',3,'content','paths',1,1,'ready',1,1)",["0".repeat(64)]).unwrap();
    connection
}

/// A bounded cache reclaims unowned payloads while retaining even over-budget active artifacts.
/// 有界缓存回收无归属负载，同时保留即使超过预算的活动产物。
#[test]
fn shared_storage_cache_budget_evicts_only_unpinned_objects() {
    // Use real ownership tables with a tiny budget, avoiding large or user-owned test files.
    // 使用真实归属表和微小预算，避免大型测试文件及用户文件。
    let mut connection = database();
    run(&connection, "active", 1, "running", &contract());
    let transaction = connection.transaction().unwrap();
    shared_manifest_store::store_node(&transaction, "n", "active", 0, &node(1), &contract(), 1)
        .unwrap();
    // Cache-only objects have no surviving run references and may be discarded.
    // 仅缓存对象没有存活运行引用，可以丢弃。
    let unpinned = shared_artifacts::store_artifact(
        &transaction,
        serde_json::json!({"body":"unused".repeat(1000)}),
    )
    .unwrap();
    transaction
        .execute(
            "INSERT INTO vce_prepared_artifact_cache VALUES ('contract','unused',?,1)",
            [&unpinned],
        )
        .unwrap();
    transaction.commit().unwrap();
    artifact_retention::collect_with_budget(&mut connection, "active", 0).unwrap();
    assert_eq!(
        connection
            .query_row(
                "SELECT count(*) FROM vce_shared_artifacts WHERE artifact_hash=?",
                [&unpinned],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
    assert_eq!(
        connection
            .query_row("SELECT count(*) FROM vce_embedding_run_nodes", [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap(),
        1
    );
    let hash: String = connection
        .query_row(
            "SELECT artifact_hash FROM vce_embedding_run_nodes",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(
        shared_manifest_store::restore_node(&connection, &hash, "blob", 1).unwrap(),
        node(1)
    );
}

/// Unknown provider uploads block overlapping retries but allow unrelated inputs and original recovery.
/// 未知供应商上传阻止重叠重试，但允许无关输入和原运行恢复。
#[test]
fn shared_storage_unknown_remote_work_requires_reconciliation() {
    let connection = database();
    let contract = contract();
    run(&connection, "original", 1, "failed", &contract);
    let batch = batch("old-batch");
    connection
        .execute(
            "INSERT INTO vce_embedding_run_batches VALUES ('n','original',0,?,?,?,1)",
            params![
                batch.batch_id,
                batch.identity_fingerprint().unwrap(),
                serde_json::to_string(&batch).unwrap()
            ],
        )
        .unwrap();
    // This fixture is the same typed remote contract used by full data-plane tests.
    // 此夹具与完整数据面测试使用相同的类型化远端契约。
    let mut remote = super::tests::test_remote_embedding_batch_record();
    remote.state = vulcan_context_engine::VceEmbeddingRemoteBatchState::UploadUnknown;
    remote.error = Some("upload acknowledgement lost".into());
    remote.validate().unwrap();
    connection
        .execute(
            "INSERT INTO vce_embedding_remote_batches VALUES ('n','original',?,?,0,1,?,1)",
            params![
                remote.remote_task_id,
                remote.purpose,
                serde_json::to_string(&remote).unwrap()
            ],
        )
        .unwrap();
    let error = shared_result_store::require_resolved_predecessors(
        &connection,
        "n",
        "retry",
        std::slice::from_ref(&batch),
    )
    .unwrap_err();
    assert!(error.contains("original") && error.contains(&remote.remote_task_id));
    shared_result_store::require_resolved_predecessors(
        &connection,
        "n",
        "original",
        std::slice::from_ref(&batch),
    )
    .unwrap();
    let mut unrelated = batch;
    unrelated.input_ids = vec!["other::segment:0".into()];
    shared_result_store::require_resolved_predecessors(&connection, "n", "retry", &[unrelated])
        .unwrap();
}

/// Retire ended candidates only after publication ambiguity is resolved; active jobs remain protected.
/// 仅在发布歧义解决后回收已结束候选；活动作业仍受保护。
#[test]
fn shared_storage_retention_respects_host_and_publication_lifecycle() {
    let mut connection = database();
    connection
        .execute_batch(
            "CREATE TABLE vce_host_jobs(job_id TEXT PRIMARY KEY,snapshot_json TEXT NOT NULL);",
        )
        .unwrap();
    for (number, id, state) in [
        (1, "ended", "failed"),
        (2, "publishing", "failed"),
        (3, "active", "running"),
    ] {
        run(&connection, id, number, "candidate_ready", &contract());
        connection
            .execute(
                "INSERT INTO vce_host_jobs VALUES (?,?)",
                params![id, serde_json::json!({"state":state}).to_string()],
            )
            .unwrap();
    }
    connection.execute("INSERT INTO vce_publication_intents VALUES ('n','commit','publishing',2,2,?,'committed',NULL,1,1,1)",["a".repeat(64)]).unwrap();
    for number in 4..10 {
        let id = format!("settled-{number}");
        run(&connection, &id, number, "failed", &contract());
        artifact_retention::collect(&mut connection, &id).unwrap();
    }
    assert_eq!(
        connection
            .query_row(
                "SELECT count(*) FROM vce_embedding_runs WHERE run_id='ended'",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
    assert_eq!(
        connection
            .query_row(
                "SELECT count(*) FROM vce_embedding_runs WHERE run_id IN ('publishing','active')",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
        2
    );
    connection.execute_batch("UPDATE vce_publication_intents SET stage='reconciled'; UPDATE vce_host_jobs SET snapshot_json='{\"state\":\"failed\"}' WHERE job_id='active';").unwrap();
    artifact_retention::collect(&mut connection, "settled-9").unwrap();
    assert_eq!(
        connection
            .query_row(
                "SELECT count(*) FROM vce_embedding_runs WHERE run_id IN ('publishing','active')",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
}

/// Build a stable, physically identified embedding contract for synthetic requests.
/// 为合成请求构建稳定且带物理身份的向量契约。
/// Returns one three-dimensional producer contract.
/// 返回一个三维生产方契约。
fn contract() -> VceEmbeddingRunContract {
    VceEmbeddingRunContract {
        embedding_space: EmbeddingSpace {
            vector_space_id: "space".into(),
            physical_fingerprint: Some("producer".into()),
            dimension: 3,
            preprocessing_version: "prep".into(),
            chunker_version: "chunks".into(),
        },
        configured_max_inputs: 20,
        provider_max_inputs: Some(20),
        effective_max_inputs: 20,
        max_chars: 8192,
        max_segment_chars: 4096,
        overlap_chars: 128,
        max_file_bytes: 1048576,
        purposes: vec![
            vulcan_context_engine::VceAiPurpose::CodeDocumentEmbedding
                .as_str()
                .into(),
        ],
    }
}

/// Insert one minimal parent carrying the same real frozen contract used by production readers.
/// 插入一个携带生产读取者所用真实冻结契约的最小父记录。
/// Inputs select run identity, generation and lifecycle; returns after insertion.
/// 输入选择运行身份、代次和生命周期；插入后返回。
fn run(
    connection: &Connection,
    id: &str,
    generation: i64,
    state: &str,
    contract: &VceEmbeddingRunContract,
) {
    connection.execute("INSERT INTO vce_embedding_runs(namespace_id,run_id,generation,state,manifest_json,manifest_complete,created_at_unix_ms,updated_at_unix_ms) VALUES ('n',?,?,?,?,1,?,?)",params![id,generation,state,serde_json::json!({"contract":contract}).to_string(),generation,generation]).unwrap();
}

/// Build one large source snapshot whose bytes stay unchanged across run generations.
/// 构建一个正文跨运行代次不变的大型源码快照。
/// Generation is execution-local; returns a source-only node fixture.
/// 代次局限于执行；返回仅源码的节点夹具。
fn node(generation: u64) -> VceCodeNodeSnapshot {
    let body = "中文 source body\n".repeat(5000);
    VceCodeNodeSnapshot {
        blob_name: "blob".into(),
        path: "src/lib.rs".into(),
        generation,
        source_size: body.len() as u64,
        mtime_ns: None,
        source_content: Some(body),
        prepared_index: None,
        structure_analyzer_id: None,
        structure_mode: None,
        structure_fallback_reason: None,
        symbols: vec![],
        ready_without_embedding_reason: Some("fixture".into()),
        chunk_ids: vec![],
        content_input_ids: vec![],
        path_input_id: None,
    }
}

/// Build one immutable request record with only its execution identity varying.
/// 构建仅执行身份变化的不可变请求记录。
/// Returns a pending logical batch for exact reuse checks.
/// 返回用于精确复用校验的待执行逻辑批次。
fn batch(id: &str) -> VceEmbeddingBatchRecord {
    VceEmbeddingBatchRecord {
        batch_id: id.into(),
        purpose: vulcan_context_engine::VceAiPurpose::CodeDocumentEmbedding
            .as_str()
            .into(),
        input_ids: vec!["input::segment:0".into()],
        total_chars: 16,
        state: VceEmbeddingBatchState::Pending,
        attempt: 0,
        error: None,
    }
}

/// Repeated completed attempts share material and leave only bounded run references.
/// 反复结束的尝试共享材料，仅留下有界运行引用。
#[test]
fn shared_storage_retries_are_bounded_and_results_are_reusable() {
    let mut connection = database();
    let contract = contract();
    let result = VceEmbeddingBatchResult {
        batch_id: "batch-0".into(),
        vectors: vec![VceEmbeddingVectorResult {
            input_id: "input::segment:0".into(),
            values: vec![1.0, 0.0, 0.0],
        }],
    };
    let mut baseline = 0_i64;
    for generation in 1..=30 {
        let id = format!("run-{generation}");
        run(&connection, &id, generation, "running", &contract);
        let transaction = connection.transaction().unwrap();
        shared_manifest_store::store_node(
            &transaction,
            "n",
            &id,
            0,
            &node(generation as u64),
            &contract,
            generation,
        )
        .unwrap();
        let batch = batch(&format!("batch-{generation}"));
        if generation == 1 {
            let mut result = result.clone();
            result.batch_id = batch.batch_id.clone();
            shared_result_store::store(&transaction, "n", &id, &batch, &result, generation)
                .unwrap();
        } else {
            let recovered = shared_result_store::reuse(&transaction, "n", &id, &batch, generation)
                .unwrap()
                .expect("reuse without provider");
            assert_eq!(recovered.vectors, result.vectors);
            assert_eq!(recovered.batch_id, batch.batch_id);
        }
        transaction.commit().unwrap();
        connection
            .execute(
                "UPDATE vce_embedding_runs SET state='failed' WHERE run_id=?",
                [&id],
            )
            .unwrap();
        artifact_retention::collect(&mut connection, &id).unwrap();
        let bytes: i64 = connection
            .query_row(
                "SELECT sum(byte_size) FROM vce_shared_payloads",
                [],
                |row| row.get(0),
            )
            .unwrap();
        if generation == 1 {
            baseline = bytes;
        } else {
            assert_eq!(
                bytes, baseline,
                "retry must not accumulate source or vector copies"
            );
        }
        let runs: i64 = connection
            .query_row("SELECT count(*) FROM vce_embedding_runs", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert!(runs <= 3);
    }
    // Different physical producers and segment contracts cannot adopt old vectors.
    // 不同物理生产方和分段契约不能接纳旧向量。
    let mut changed = contract;
    changed.embedding_space.physical_fingerprint = Some("other-producer".into());
    run(&connection, "different", 31, "running", &changed);
    let transaction = connection.transaction().unwrap();
    assert!(
        shared_result_store::reuse(
            &transaction,
            "n",
            "different",
            &batch("different-batch"),
            31
        )
        .unwrap()
        .is_none()
    );
    transaction.rollback().unwrap();
    // Unreferenced writes are swept after a lost owner transaction, without touching cached results.
    // 丢失归属后的无引用写入会被清扫，不触碰缓存结果。
    let transaction = connection.transaction().unwrap();
    shared_artifacts::store_artifact(
        &transaction,
        serde_json::json!({"abandoned":"z".repeat(80000)}),
    )
    .unwrap();
    transaction.commit().unwrap();
    artifact_retention::collect(&mut connection, "different").unwrap();
    assert_eq!(
        connection
            .query_row(
                "SELECT sum(byte_size) FROM vce_shared_payloads",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
        baseline
    );
}

/// Unknown remote ownership remains a root while old settled attempts are reclaimed.
/// 回收已结束的旧尝试时，未知远端归属仍然作为根引用保留。
#[test]
fn shared_storage_retention_pins_unknown_remote_work() {
    let mut connection = database();
    let contract = contract();
    run(&connection, "unknown", 1, "failed", &contract);
    let transaction = connection.transaction().unwrap();
    shared_manifest_store::store_node(&transaction, "n", "unknown", 0, &node(1), &contract, 1)
        .unwrap();
    transaction.commit().unwrap();
    // The retention query consumes only the typed lifecycle field, not provider-specific task payloads.
    // 保留查询仅使用类型化生命周期字段，不使用供应商特定任务负载。
    connection
        .execute(
            "INSERT INTO vce_embedding_remote_batches VALUES ('n','unknown','remote',?,0,1,?,1)",
            params![
                vulcan_context_engine::VceAiPurpose::CodeDocumentEmbedding.as_str(),
                serde_json::json!({"state":"submission_unknown","cleanup_warning":null})
                    .to_string()
            ],
        )
        .unwrap();
    for number in 2..=12 {
        let id = format!("settled-{number}");
        run(&connection, &id, number, "failed", &contract);
        artifact_retention::collect(&mut connection, &id).unwrap();
    }
    assert_eq!(
        connection
            .query_row(
                "SELECT count(*) FROM vce_embedding_run_nodes WHERE run_id='unknown'",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
        1
    );
    assert_eq!(
        connection
            .query_row("SELECT count(*) FROM vce_embedding_runs", [], |row| row
                .get::<_, i64>(0))
            .unwrap(),
        4
    );
    connection
        .execute(
            "UPDATE vce_embedding_remote_batches SET record_json=? WHERE run_id='unknown'",
            [serde_json::json!({"state":"cancelled","cleanup_warning":null}).to_string()],
        )
        .unwrap();
    artifact_retention::collect(&mut connection, "settled-12").unwrap();
    assert_eq!(
        connection
            .query_row(
                "SELECT count(*) FROM vce_embedding_run_nodes WHERE run_id='unknown'",
                [],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
    assert_eq!(
        connection
            .query_row("SELECT count(*) FROM vce_shared_payloads", [], |row| row
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
}
