use super::{
    VCE_SPACE_ID, VceProjectIndexConfig, bootstrap_vce_data_plane, build_vce_schema_script,
    partition_vector_rows_by_data_limit, validate_checkpoint_membership_transition,
    validate_physical_compaction_operation_identity, vector_table_name,
};
use crate::data_plane::{HostRuntimeHandle, HostRuntimeMetadata};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::sync::Arc;
use tempfile::tempdir;
use uuid::Uuid;
use vulcan_context_engine::{
    AuxiliaryIndexStatus, Checkpoint, Chunk, ChunkKind, DenseSearchRequest, EmbeddingSpace,
    EmbeddingVector, FileObservation, FileRecord, FileSource, FileStatus, IndexAbortRequest,
    IndexCommit, IndexStageBatch, IndexedBlob, IndexedBlobStatus, PathDocument, RefreshMode,
    RefreshReason, RefreshRequest, RetrieveRequest, SegmentVector,
    VCE_EMBEDDING_MANIFEST_SCHEMA_VERSION, VceAiBinding, VceAiCapabilitySnapshot, VceAiPort,
    VceAiPurpose, VceCodeNodeSnapshot, VceConfig, VceEmbeddingBatchCapabilities,
    VceEmbeddingBatchPlan, VceEmbeddingBatchProviderState, VceEmbeddingBatchRecord,
    VceEmbeddingBatchSnapshot, VceEmbeddingBatchState, VceEmbeddingBatchSubmission,
    VceEmbeddingBatchTaskPlan, VceEmbeddingBatchTransport, VceEmbeddingBatchUploadedInput,
    VceEmbeddingRemoteBatchRecord, VceEmbeddingRemoteBatchState, VceEmbeddingRequest,
    VceEmbeddingResponse, VceEmbeddingRunContract, VceEmbeddingRunManifest, VceEmbeddingRunState,
    VceEngine, VceFuture, VceIndexBinding, VceIndexPort, VcePhysicalCompactionOperation,
    VcePhysicalCompactionState, VcePhysicalTableRole, VcePortError, VcePortOperation,
    VceQueryRewriteRequest, VceQueryRewriteResponse, VceQueryStatistics, VceRefreshSummary,
    VceRerankRequest, VceRerankResponse, VceSourcePort, VceSourceReadRequest,
    VceSourceReadResponse, VceStateIdentity, VceStateSnapshot, WorkspaceSnapshot, blob_name,
};
use vulcan_lancedb_engine::types::{LanceDbCreateTableInput, LanceDbDropTableInput};
use vulcan_sqlite_engine::sql_exec::SqliteValue;

/// Build the mandatory frozen execution contract used by storage manifest fixtures.
/// 构建存储清单夹具使用的必需冻结执行契约。
fn test_embedding_contract() -> VceEmbeddingRunContract {
    VceEmbeddingRunContract {
        embedding_space: EmbeddingSpace {
            vector_space_id: String::from("test-space"),
            physical_fingerprint: Some(String::from("test-provider:test-model:3")),
            dimension: 3,
            preprocessing_version: String::from("vce-preprocess-v1"),
            chunker_version: String::from("vce-chunker-v1"),
        },
        configured_max_inputs: 20,
        provider_max_inputs: Some(20),
        effective_max_inputs: 20,
        max_chars: 8_192,
        max_segment_chars: 4_096,
        overlap_chars: 128,
        max_file_bytes: 16 * 1024 * 1024,
        purposes: vec![
            VceAiPurpose::CodeDocumentEmbedding.as_str().to_owned(),
            VceAiPurpose::CodePathEmbedding.as_str().to_owned(),
        ],
    }
}

/// Build one bounded planned remote Batch record for durable storage tests.
/// 为持久存储测试构建一条有界的已规划远端 Batch 记录。
pub(super) fn test_remote_embedding_batch_record() -> VceEmbeddingRemoteBatchRecord {
    let remote_task_id = String::from("remote-task-0");
    VceEmbeddingRemoteBatchRecord {
        remote_task_id: remote_task_id.clone(),
        purpose: VceAiPurpose::CodeDocumentEmbedding.as_str().to_owned(),
        first_logical_batch_index: 0,
        logical_batch_count: 1,
        plan: VceEmbeddingBatchPlan {
            request_id: String::from("remote-request-0"),
            request_fingerprint: "a".repeat(64),
            capabilities: VceEmbeddingBatchCapabilities {
                transport: VceEmbeddingBatchTransport::JsonlFile,
                provider_maximum_requests: Some(50_000),
                provider_maximum_payload_bytes: Some(500 * 1024 * 1024),
                provider_maximum_item_bytes: Some(1_000_000),
                provider_maximum_parallel_tasks: Some(1_000),
                host_policy_maximum_tasks_per_run: 1_000,
                host_policy_maximum_requests: 50_000,
                host_policy_maximum_payload_bytes: 500 * 1024 * 1024,
                host_policy_maximum_result_bytes: 256 * 1024 * 1024,
                provider_completion_window_seconds: Some(86_400),
                maximum_requests: 50_000,
                maximum_payload_bytes: 500 * 1024 * 1024,
                maximum_result_bytes: 256 * 1024 * 1024,
                maximum_tasks_per_run: 1_000,
                provider_maximum_files: Some(10_000),
                provider_maximum_total_file_bytes: Some(100 * 1024 * 1024 * 1024),
                provider_file_upload_qps: Some(3),
                provider_file_operation_qps: Some(10),
                provider_task_create_rpm: Some(1_000),
                provider_task_status_rpm: Some(1_000),
                provider_task_list_rpm: Some(100),
                provider_task_cancel_rpm: Some(1_000),
                minimum_poll_interval_ms: 60_000,
                supports_cancel: true,
                deletes_input_file_after_ingestion: true,
            },
            tasks: vec![VceEmbeddingBatchTaskPlan {
                task_id: remote_task_id,
                first_input_index: 0,
                input_count: 1,
                encoded_payload_bytes: 128,
                payload_sha256: "b".repeat(64),
            }],
        },
        state: VceEmbeddingRemoteBatchState::Planned,
        uploaded_input: None,
        submission: None,
        snapshot: None,
        poll_attempt: 0,
        next_poll_not_before_unix_ms: None,
        error: None,
        cleanup_warning: None,
        updated_at_unix_ms: 2,
    }
}

/// Deterministic VCE AI boundary used by the real data-plane test.
/// 真实 VCE 数据面测试使用的确定性 VCE AI 边界。
struct DeterministicAi {
    /// Frozen registration identity.
    /// 冻结的注册身份。
    binding: VceAiBinding,
    /// Physical vector-space contract returned by every Embedding call.
    /// 每次向量调用返回的物理向量空间契约。
    space: EmbeddingSpace,
    /// Ordered request identities used to verify that recovery skips saved vectors.
    /// 用于验证恢复跳过已保存向量的有序请求身份。
    embedding_requests: std::sync::Mutex<Vec<String>>,
    /// Optional one-based request position that stays pending until its future is dropped.
    /// 可选的从一开始计数的请求位置；该请求持续等待直到其 Future 被丢弃。
    pause_embedding_at: Option<usize>,
    /// Buffered signal proving that the selected pending request has been reached.
    /// 证明已经到达选定等待请求的缓冲信号。
    embedding_paused: tokio::sync::Notify,
}

impl VceAiPort for DeterministicAi {
    /// Returns the fixed test registration.
    /// 返回固定测试注册。
    fn binding(&self) -> &VceAiBinding {
        &self.binding
    }

    /// Returns the complete fixed capability snapshot.
    /// 返回完整固定能力快照。
    fn capability_snapshot(&self) -> VceAiCapabilitySnapshot {
        VceAiCapabilitySnapshot {
            registration_id: self.binding.registration_id.clone(),
            consumer_profile_id: self.binding.consumer_profile_id.clone(),
            vector_space_id: self.binding.vector_space_id.clone(),
            registered: true,
            embedding_available: true,
            query_rewrite_available: true,
            rerank_available: true,
        }
    }

    /// Returns deterministic normalized vectors for every requested segment.
    /// 为每个请求分段返回确定性归一化向量。
    fn embed(
        &self,
        request: VceEmbeddingRequest,
    ) -> VceFuture<Result<VceEmbeddingResponse, VcePortError>> {
        // Request counting occurs at admission, including the request interrupted by the crash fixture.
        // 在准入时统计请求，其中包含崩溃夹具中断的请求。
        let request_number = {
            let mut requests = self
                .embedding_requests
                .lock()
                .expect("record embedding request");
            requests.push(request.context.operation_id.clone());
            requests.len()
        };
        if self.pause_embedding_at == Some(request_number) {
            self.embedding_paused.notify_one();
            return Box::pin(std::future::pending());
        }
        let vectors = request
            .batch
            .segments
            .iter()
            .map(|segment| SegmentVector {
                input_id: segment.request_input_id(),
                values: vec![1.0, 0.0, 0.0],
            })
            .collect();
        ready(Ok(VceEmbeddingResponse {
            operation_id: request.context.operation_id,
            vectors,
            space: self.space.clone(),
        }))
    }

    /// Returns no optional query rewrites in the deterministic fixture.
    /// 在确定性夹具中不返回可选查询改写。
    fn rewrite_query(
        &self,
        request: VceQueryRewriteRequest,
    ) -> VceFuture<Result<VceQueryRewriteResponse, VcePortError>> {
        ready(Ok(VceQueryRewriteResponse {
            operation_id: request.context.operation_id,
            rewrites: Vec::new(),
        }))
    }

    /// Preserves the candidate order during deterministic Rerank.
    /// 在确定性重排中保留候选顺序。
    fn rerank(
        &self,
        request: VceRerankRequest,
    ) -> VceFuture<Result<VceRerankResponse, VcePortError>> {
        ready(Ok(VceRerankResponse {
            operation_id: request.context.operation_id,
            ordered_candidate_indexes: (0..request.candidates.len()).collect(),
        }))
    }
}

/// Source boundary that rejects reads because the fixture supplies immutable text observations.
/// 拒绝源码读取的边界，因为夹具会直接提供不可变正文观察。
struct NoReadSource;

impl VceSourcePort for NoReadSource {
    /// Rejects every unexpected source read.
    /// 拒绝每一次非预期源码读取。
    fn read_sources(
        &self,
        _request: VceSourceReadRequest,
    ) -> VceFuture<Result<VceSourceReadResponse, VcePortError>> {
        ready(Err(VcePortError::Failed {
            operation: VcePortOperation::SourceRead,
            message: String::from("unexpected real-data-plane fixture source read"),
        }))
    }
}

/// Returns one immediately ready asynchronous test result.
/// 返回一个立即就绪的异步测试结果。
fn ready<T: Send + 'static>(value: T) -> VceFuture<T> {
    Box::pin(async move { value })
}

/// Verifies compaction snapshots cannot cross an admitted operation or table identity.
/// 验证整理快照不能跨越已准入的操作或表身份。
#[test]
fn physical_compaction_response_identity_is_exact() {
    let operation = VcePhysicalCompactionOperation {
        operation_id: String::from("operation-a"),
        table_name: String::from("content-table"),
        state: VcePhysicalCompactionState::Queued,
        cancel_requested: false,
        message: String::from("queued"),
        error: None,
        submitted_at_unix_ms: 1,
        started_at_unix_ms: None,
        completed_at_unix_ms: None,
        before: None,
        after: None,
        metrics: None,
    };
    assert!(
        validate_physical_compaction_operation_identity(
            "operation-a",
            "content-table",
            operation.clone(),
        )
        .is_ok()
    );
    assert!(
        validate_physical_compaction_operation_identity(
            "operation-b",
            "content-table",
            operation.clone(),
        )
        .is_err()
    );
    assert!(
        validate_physical_compaction_operation_identity("operation-a", "path-table", operation,)
            .is_err()
    );
}

/// Verifies the VCE schema remains isolated and contains atomic-publication tables.
/// 验证 VCE Schema 保持隔离并包含原子发布表。
#[test]
fn schema_is_isolated_and_contains_publication_state() {
    let schema = build_vce_schema_script();
    assert!(schema.contains("CREATE TABLE IF NOT EXISTS vce_project_bindings"));
    assert!(schema.contains("CREATE TABLE IF NOT EXISTS vce_staged_blobs"));
    assert!(schema.contains("CREATE VIEW IF NOT EXISTS vce_checkpoints"));
    assert!(schema.contains("CREATE TABLE IF NOT EXISTS vce_read_leases"));
    assert!(schema.contains("CREATE TABLE IF NOT EXISTS vce_blob_versions"));
    assert!(schema.contains("CREATE TABLE IF NOT EXISTS vce_commit_receipts"));
    assert!(schema.contains("CREATE TABLE IF NOT EXISTS vce_publication_intents"));
    assert!(schema.contains("CREATE TABLE IF NOT EXISTS vce_commit_guards"));
    assert!(schema.contains("CREATE TABLE IF NOT EXISTS vce_embedding_runs"));
    assert!(schema.contains("CREATE TABLE IF NOT EXISTS vce_embedding_run_nodes"));
    assert!(schema.contains("CREATE TABLE IF NOT EXISTS vce_embedding_run_reused_blobs"));
    assert!(schema.contains("CREATE TABLE IF NOT EXISTS vce_embedding_run_batches"));
    assert!(schema.contains("CREATE TABLE IF NOT EXISTS vce_embedding_remote_batches"));
    assert!(schema.contains(VceAiPurpose::CodeDocumentEmbedding.as_str()));
    assert!(schema.contains(VceAiPurpose::CodePathEmbedding.as_str()));
    assert!(schema.contains("CREATE TABLE IF NOT EXISTS vce_embedding_result_refs"));
    assert!(schema.contains("manifest_complete INTEGER NOT NULL DEFAULT 0"));
    assert!(schema.contains("revision INTEGER NOT NULL DEFAULT 1"));
    assert!(schema.contains("trg_vce_embedding_run_batches_touch_parent"));
    assert!(schema.contains("trg_vce_embedding_remote_batches_touch_parent"));
    assert!(!schema.contains("vmm_projects"));
    assert!(!schema.contains("knowledge_documents"));
}

/// Verifies vector table identities are deterministic and channel-separated.
/// 验证向量表身份确定且按通道隔离。
#[test]
fn vector_tables_are_deterministic_and_channel_scoped() {
    let content = vector_table_name(
        "content",
        "code-semantic:project-1",
        "provider:model:1536",
        "pre-v1",
        "chunk-v1",
        1536,
    );
    let repeated = vector_table_name(
        "content",
        "code-semantic:project-1",
        "provider:model:1536",
        "pre-v1",
        "chunk-v1",
        1536,
    );
    let path = vector_table_name(
        "path",
        "code-semantic:project-1",
        "provider:model:1536",
        "pre-v1",
        "chunk-v1",
        1536,
    );
    assert_eq!(content, repeated);
    assert_ne!(content, path);
    assert!(content.starts_with("vce_content_"));
    assert!(content.ends_with("_d1536"));
}

/// Verifies stage rows are fully encoded into exact byte-bounded JSON arrays.
/// 验证暂存行会完整编码为精确字节有界的 JSON 数组。
#[test]
fn vector_rows_partition_at_the_exact_data_field_limit() {
    let rows = vec![
        serde_json::json!({"row_id":"row-a","content":"alpha"}),
        serde_json::json!({"row_id":"row-b","content":"beta"}),
        serde_json::json!({"row_id":"row-c","content":"gamma"}),
    ];
    let first_size = serde_json::to_vec(&rows[0])
        .expect("first row should encode")
        .len();
    let second_size = serde_json::to_vec(&rows[1])
        .expect("second row should encode")
        .len();
    let exact_two_row_limit = 2 + first_size + 1 + second_size;

    let prepared =
        partition_vector_rows_by_data_limit("content_vectors", rows.clone(), exact_two_row_limit)
            .expect("rows should partition within the exact limit");
    assert_eq!(prepared.table_name, "content_vectors");
    assert_eq!(prepared.batches.len(), 2);
    assert_eq!(prepared.batches[0].len(), exact_two_row_limit);
    assert!(
        prepared
            .batches
            .iter()
            .all(|batch| batch.len() <= exact_two_row_limit)
    );

    let decoded = prepared
        .batches
        .iter()
        .flat_map(|batch| {
            serde_json::from_slice::<Vec<serde_json::Value>>(batch)
                .expect("every prepared batch should be a complete JSON array")
        })
        .collect::<Vec<_>>();
    assert_eq!(decoded, rows);
}

/// Verifies byte partitioning preserves UTF-8, escaped text, and heterogeneous vector lengths.
/// 验证字节分批会保留 UTF-8、转义文本及不同向量长度。
#[test]
fn vector_rows_partition_encoded_utf8_escapes_and_dimensions() {
    let rows = vec![
        serde_json::json!({
            "row_id": "路径-甲",
            "content": "引号：\"文本\"\n反斜杠：\\目录",
            "vector": [0.1, 0.2, 0.3]
        }),
        serde_json::json!({
            "row_id": "path-beta",
            "content": "emoji-🧭-终点",
            "vector": [0.4, 0.5, 0.6, 0.7, 0.8]
        }),
    ];
    let standalone_limit = rows
        .iter()
        .map(|row| serde_json::to_vec(row).expect("row should encode").len() + 2)
        .max()
        .expect("fixture should contain rows");

    let prepared =
        partition_vector_rows_by_data_limit("content_vectors", rows.clone(), standalone_limit)
            .expect("encoded rows should fit independently");
    assert_eq!(prepared.batches.len(), 2);
    assert!(
        prepared
            .batches
            .iter()
            .all(|batch| batch.len() <= standalone_limit)
    );
    let decoded = prepared
        .batches
        .iter()
        .flat_map(|batch| {
            serde_json::from_slice::<Vec<serde_json::Value>>(batch)
                .expect("encoded batch should remain valid UTF-8 JSON")
        })
        .collect::<Vec<_>>();
    assert_eq!(decoded, rows);
    assert_eq!(decoded[0]["vector"].as_array().map(Vec::len), Some(3));
    assert_eq!(decoded[1]["vector"].as_array().map(Vec::len), Some(5));
}

/// Verifies an oversized row rejects the complete preparation instead of emitting a partial batch.
/// 验证单行过大会拒绝完整准备过程，而不会产出部分批次。
#[test]
fn vector_row_preparation_rejects_oversized_or_impossible_payloads() {
    let row = serde_json::json!({"row_id":"oversized","content":"payload"});
    let exact_size = serde_json::to_vec(&row)
        .expect("test row should encode")
        .len()
        + 2;
    let oversized =
        partition_vector_rows_by_data_limit("content_vectors", vec![row.clone()], exact_size - 1)
            .expect_err("one oversized row must reject the entire table preparation");
    assert!(oversized.contains("单行 JSON 载荷"));

    let impossible = partition_vector_rows_by_data_limit("content_vectors", vec![row], 1)
        .expect_err("a limit smaller than an empty JSON array must be rejected");
    assert!(impossible.contains("无法容纳 JSON 数组"));

    let empty = partition_vector_rows_by_data_limit("content_vectors", Vec::new(), 0)
        .expect("empty row sets require no transport envelope");
    assert!(empty.batches.is_empty());
}

/// Verifies checkpoint publication cannot omit, fabricate, or delete unknown Blob identities.
/// 验证检查点发布不能遗漏、伪造成员或删除未知 Blob 身份。
#[test]
fn checkpoint_membership_transition_is_exact() {
    // current is the authoritative predecessor membership.
    // current 是权威前驱成员集合。
    let current = BTreeSet::from([String::from("blob-a"), String::from("blob-b")]);
    // deleted removes one existing identity from the next view.
    // deleted 从下一视图移除一个已存在身份。
    let deleted = BTreeSet::from([String::from("blob-a")]);
    // staged publishes one replacement identity.
    // staged 发布一个替换身份。
    let staged = BTreeSet::from([String::from("blob-c")]);
    // expected is the only valid next membership.
    // expected 是唯一有效的下一成员集合。
    let expected = BTreeSet::from([String::from("blob-b"), String::from("blob-c")]);

    assert_eq!(
        validate_checkpoint_membership_transition(&current, &deleted, &staged, &expected),
        Ok(())
    );
    assert!(
        validate_checkpoint_membership_transition(
            &current,
            &deleted,
            &staged,
            &BTreeSet::from([String::from("blob-c")]),
        )
        .is_err()
    );
    assert!(
        validate_checkpoint_membership_transition(
            &current,
            &deleted,
            &staged,
            &BTreeSet::from([
                String::from("blob-b"),
                String::from("blob-c"),
                String::from("blob-fabricated"),
            ]),
        )
        .is_err()
    );
    assert!(
        validate_checkpoint_membership_transition(
            &current,
            &BTreeSet::from([String::from("blob-unknown")]),
            &staged,
            &expected,
        )
        .is_err()
    );
}

/// Exercises project binding, root rotation, and physical-table creation against the real in-process runtime.
/// 使用真实进程内运行时验证项目绑定、根目录轮换和物理表创建。
#[test]
fn in_process_engine_opens_and_rotates_project_binding() {
    // This deliberately large integration fixture creates a correspondingly large async state
    // machine, so give only its private harness thread enough stack to construct and poll it.
    // 该大型集成夹具会生成同样庞大的异步状态机，因此仅为其私有测试线程提供足够栈空间。
    std::thread::Builder::new()
        .name(String::from("vce-real-engine-binding"))
        .stack_size(16 * 1024 * 1024)
        .spawn(|| {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("build VCE real-engine test runtime");
            runtime.block_on(run_in_process_engine_opens_and_rotates_project_binding());
        })
        .expect("spawn VCE real-engine test thread")
        .join()
        .expect("join VCE real-engine test thread");
}

/// Runs the oversized in-process integration fixture on its dedicated harness thread.
/// 在专用测试线程上运行超大型进程内集成夹具。
async fn run_in_process_engine_opens_and_rotates_project_binding() {
    let temporary = tempdir().expect("create VCE integration root");
    let data_root = temporary.path().join("data");
    let first_root = temporary.path().join("project-one");
    let second_root = temporary.path().join("project-two");
    fs::create_dir_all(&first_root).expect("create first VCE project root");
    fs::create_dir_all(&second_root).expect("create second VCE project root");
    let metadata_path = data_root.join("host-metadata.sqlite3");
    let host_runtime = HostRuntimeHandle::new_for_test(HostRuntimeMetadata {
        data_root: data_root.clone(),
        sqlite_path: metadata_path.clone(),
        sqlite_binding_id: format!("vce-e2e-app-{}", Uuid::new_v4()),
        system_statistics_sqlite_path: metadata_path,
        system_statistics_sqlite_binding_id: format!("vce-e2e-statistics-{}", Uuid::new_v4()),
    });
    // catalog is the new lightweight location boundary; bootstrapping it must not open projects.
    // catalog 是新的轻量位置边界；引导它不得打开项目。
    fs::create_dir_all(&data_root).expect("create isolated application data root");
    let catalog = crate::vce_storage_catalog::VceStorageCatalog::bootstrap(&host_runtime)
        .await
        .expect("bootstrap lightweight catalog");
    assert!(catalog.root().join("catalog.sqlite3").is_file());
    assert!(!catalog.root().join("projects").exists());
    assert!(!catalog.root().join("lancedb").exists());
    for number in 0..100 {
        assert!(
            catalog
                .project_location(&format!("inactive-project-{number}"))
                .await
                .expect("read cold location")
                .is_none()
        );
    }
    assert!(!catalog.root().join("projects").exists());
    // allocated is location ownership only: neither SQLite nor vector tables are initialized.
    // allocated 仅代表位置归属：尚未初始化 SQLite 或向量表。
    let allocated = catalog
        .allocate_project("catalog-project-a")
        .await
        .expect("allocate exact project");
    assert!(allocated.root.is_dir());
    assert!(!allocated.sqlite_path.exists());
    assert!(
        catalog
            .project_location("catalog-project-b")
            .await
            .expect("peer location")
            .is_none()
    );
    // original_location must survive repeated allocation and catalog reopening unchanged.
    // original_location 必须在重复分配与目录重开后保持不变。
    let original_location = catalog
        .project_location("catalog-project-a")
        .await
        .expect("read location");
    assert_eq!(
        catalog
            .allocate_project("catalog-project-a")
            .await
            .expect("repeat allocation"),
        allocated
    );
    catalog.close().await.expect("close catalog binding only");
    // reopened verifies the durable catalog version and ownership through the same host runtime.
    // reopened 通过相同宿主运行时验证持久目录版本与归属。
    let reopened = crate::vce_storage_catalog::VceStorageCatalog::bootstrap(&host_runtime)
        .await
        .expect("reopen catalog");
    assert_eq!(
        reopened
            .project_location("catalog-project-a")
            .await
            .expect("reread location"),
        original_location
    );
    reopened.close().await.expect("close reopened catalog");
    // factory starts with no content bindings, even when the workspace contains many projects.
    // factory 起初不含内容绑定，即使工作区包含很多项目。
    let factory = super::VceDataPlaneFactory::bootstrap(&host_runtime)
        .await
        .expect("bootstrap isolated factory");
    for number in 0..100 {
        assert!(
            factory
                .load_active_project_config(&format!("cold-{number}"))
                .await
                .expect("read cold project")
                .is_none()
        );
    }
    assert!(!data_root.join("vmm").exists());
    // isolated_a and isolated_b must use separate physical files and host binding identities.
    // isolated_a 与 isolated_b 必须使用分离的物理文件和宿主绑定身份。
    let isolated_a = factory
        .create_project_index(test_project_config("isolated-a", &first_root, 1))
        .await
        .expect("create isolated a");
    let isolated_b = factory
        .create_project_index(test_project_config("isolated-b", &second_root, 1))
        .await
        .expect("create isolated b");
    assert_ne!(
        isolated_a.data_plane.sqlite_path,
        isolated_b.data_plane.sqlite_path
    );
    assert_ne!(
        isolated_a.data_plane.lancedb_path,
        isolated_b.data_plane.lancedb_path
    );
    assert_ne!(
        isolated_a.data_plane.sqlite_binding_id,
        isolated_b.data_plane.sqlite_binding_id
    );
    assert!(isolated_a.data_plane.sqlite_path.is_file());
    assert!(isolated_b.data_plane.sqlite_path.is_file());
    assert!(
        isolated_a.data_plane.sqlite_path.starts_with(
            fs::canonicalize(&data_root)
                .expect("resolve fixture data root")
                .join("vce")
                .join("projects")
        )
    );
    assert!(!isolated_a.is_read_only());
    // Every child lookup used by file-level cascades must include the complete Blob foreign key.
    // 文件级级联使用的每个子表查询必须覆盖完整 Blob 外键。
    for (table, index) in [
        ("vce_staged_chunks", "idx_vce_staged_chunk_blob"),
        ("vce_staged_symbols", "idx_vce_staged_symbol_blob"),
        ("vce_chunk_versions", "idx_vce_version_chunk_blob"),
        ("vce_symbol_versions", "idx_vce_version_symbol_blob"),
    ] {
        // plan is produced by the real host SQLite planner, not inferred from index declarations.
        // plan 来自真实宿主 SQLite 查询规划器，不从索引声明推断。
        let plan: Vec<serde_json::Value> = isolated_a.data_plane.query_rows(
                &format!("EXPLAIN QUERY PLAN SELECT rowid FROM {table} WHERE namespace_id = ? AND refresh_id = ? AND generation = ? AND blob_name = ?;"),
                vec![super::sqlite_text("namespace"), super::sqlite_text("refresh"), SqliteValue::Integer(1), super::sqlite_text("blob")],
            ).await.expect("inspect exact file cascade query");
        assert!(
            plan.iter().any(|row| {
                row["detail"]
                    .as_str()
                    .is_some_and(|detail| detail.contains(index) && detail.contains("blob_name=?"))
            }),
            "{table}: {plan:?}"
        );
    }
    // Inspect the production paging statement so a logically correct full-prefix scan cannot regress unnoticed.
    // 检查生产分页语句，防止逻辑正确但反复扫描整个前缀的查询悄然回归。
    let page_plan: Vec<serde_json::Value> = isolated_a
        .data_plane
        .query_rows(
            &format!(
                "EXPLAIN QUERY PLAN {}",
                super::embedding_manifest_store::VCE_EMBEDDING_NODE_PAGE_SQL
            ),
            vec![
                super::sqlite_text("namespace"),
                super::sqlite_text("run"),
                SqliteValue::Integer(100),
            ],
        )
        .await
        .expect("inspect manifest keyset seek");
    assert!(
        page_plan.iter().any(|row| row["detail"]
            .as_str()
            .is_some_and(|detail| detail.contains("node_index>?"))),
        "{page_plan:?}"
    );
    // Recovery uses a distinct namespace so all following synthetic storage fixtures stay independent.
    // 恢复使用独立命名空间，使后续合成存储夹具保持独立。
    let recovery_index = factory
        .create_project_index(test_project_config("manifest-recovery", &first_root, 1))
        .await
        .expect("create manifest recovery index");
    Box::pin(verify_manifest_crash_recovery(&recovery_index)).await;
    drop(recovery_index);
    Box::pin(async {
            // A project-sized plan is written and reconstructed in bounded shards before provider work.
            // 项目级计划在供应商工作前通过有界分片写入并完整重建。
            let large_source = "x".repeat(64 * 1024);
            let large_nodes = (0..140)
                .map(|index| VceCodeNodeSnapshot {
                    blob_name: format!("manifest-blob-{index:04}"),
                    path: format!("src/manifest-{index:04}.rs"),
                    generation: 1,
                    source_size: large_source.len() as u64,
                    mtime_ns: None,
                    source_content: Some(large_source.clone()),
                    prepared_index: None,
                    structure_analyzer_id: None,
                    structure_mode: None,
                    structure_fallback_reason: None,
                    symbols: Vec::new(),
                    ready_without_embedding_reason: Some(String::from("fixture skip")),
                    chunk_ids: Vec::new(),
                    content_input_ids: Vec::new(),
                    path_input_id: None,
                })
                .collect::<Vec<_>>();
            let mut large_manifest = VceEmbeddingRunManifest {
                schema_version: VCE_EMBEDDING_MANIFEST_SCHEMA_VERSION,
                run_id: String::from("large-manifest-run"),
                namespace_id: isolated_b.binding().namespace_id.clone(),
                generation: 1,
                source_revision: None,
                state: VceEmbeddingRunState::Planned,
                contract: test_embedding_contract(),
                base_checkpoint_version: None,
                reused_blob_names: vec![String::from("reused-manifest-blob")],
                nodes: large_nodes,
                batches: vec![VceEmbeddingBatchRecord {
                    batch_id: String::from("large-manifest-run:content:0"),
                    purpose: VceAiPurpose::CodeDocumentEmbedding.as_str().to_owned(),
                    input_ids: vec![String::from("manifest-input-0")],
                    total_chars: 16,
                    state: VceEmbeddingBatchState::Pending,
                    attempt: 0,
                    error: None,
                }],
                created_at_unix_ms: 1,
                updated_at_unix_ms: 1,
            };
            assert!(
                serde_json::to_vec(&large_manifest)
                    .expect("encode large manifest fixture")
                    .len()
                    > 8 * 1024 * 1024
            );
            // receipts captures real bounded writes, including files larger than one fragment page.
            // receipts 捕获真实有界写入，包括大于一页分片的文件。
            let receipts = std::sync::Arc::new(std::sync::Mutex::new(Vec::<vulcan_context_engine::VceRefreshProgress>::new()));
            // sink shares only progress records with the exact refresh observer.
            // sink 仅与精确刷新观察器共享进度记录。
            let sink = receipts.clone();
            // control reports progress without changing index publication or provider behavior.
            // control 报告进度，不改变索引发布或供应商行为。
            let control = vulcan_context_engine::VceRefreshControl::default().with_observer(std::sync::Arc::new(move |progress| {
                sink.lock().unwrap().push(progress);
            }));
            VceIndexPort::persist_embedding_manifest_observed(&isolated_b, large_manifest.clone(), control)
                .await
                .expect("persist large sharded manifest");
            {
                // observed must contain incremental whole-file completions and the exact relative paths.
                // observed 必须包含递增的完整文件完成量与精确相对路径。
                let observed = receipts.lock().unwrap();
                assert!(observed.len() > 2);
                assert!(observed.iter().all(|progress| progress.total == Some(large_manifest.nodes.len() as u64)
                    && progress.overall_completed == 0
                    && progress.unit == vulcan_context_engine::VceRefreshUnit::Files));
                assert!(observed.windows(2).all(|pair| pair[0].completed <= pair[1].completed));
                assert_eq!(observed.last().unwrap().completed, large_manifest.nodes.len() as u64);
                assert!(observed.iter().any(|progress| progress.completed > 0 && progress.completed < large_manifest.nodes.len() as u64));
                assert_eq!(observed.iter().filter_map(|progress| progress.current_path.as_deref()).collect::<BTreeSet<_>>(),
                    large_manifest.nodes.iter().map(|node| node.path.as_str()).collect::<BTreeSet<_>>());
            }
            assert_eq!(
                isolated_b
                    .load_embedding_manifest(large_manifest.run_id.clone())
                    .await
                    .expect("load large sharded manifest"),
                Some(large_manifest.clone())
            );
            // Node guards make accidental full-plan rewrites fail during lifecycle updates.
            // 节点保护使生命周期更新期间意外重写完整计划的行为直接失败。
            isolated_b.data_plane.execute(
                "CREATE TRIGGER reject_manifest_node_insert BEFORE INSERT ON vce_embedding_run_nodes WHEN NEW.run_id = 'large-manifest-run' BEGIN SELECT RAISE(ABORT, 'node rewrite'); END;
                 CREATE TRIGGER reject_manifest_node_delete BEFORE DELETE ON vce_embedding_run_nodes WHEN OLD.run_id = 'large-manifest-run' BEGIN SELECT RAISE(ABORT, 'node rewrite'); END;
                 CREATE TRIGGER reject_manifest_node_update BEFORE UPDATE ON vce_embedding_run_nodes WHEN OLD.run_id = 'large-manifest-run' BEGIN SELECT RAISE(ABORT, 'node rewrite'); END;",
                Vec::new(), "guard immutable node shards",
            ).await.expect("guard immutable node shards");
            large_manifest
                .mark_running(2)
                .expect("mark large run running");
            isolated_b
                .persist_embedding_manifest(large_manifest.clone())
                .await
                .expect("update large manifest without rewriting nodes");
            // Remote provider state is stored independently from the large immutable source plan.
            // 远端供应商状态独立于大型不可变源码计划持久化。
            let mut remote_batch = test_remote_embedding_batch_record();
            isolated_b
                .persist_embedding_remote_batch_record(
                    large_manifest.run_id.clone(),
                    remote_batch.clone(),
                )
                .await
                .expect("persist planned remote Batch record");
            assert_eq!(
                isolated_b
                    .load_embedding_remote_batch_records(
                        large_manifest.run_id.clone(),
                        VceAiPurpose::CodeDocumentEmbedding,
                    )
                    .await
                    .expect("load planned remote Batch record"),
                vec![remote_batch.clone()]
            );
            // Reject a later remote task that skips a logical-batch range before any provider I/O can use it.
            // 在任何供应商 I/O 使用记录前，拒绝跳过逻辑批次范围的后续远端任务。
            let mut gapped_remote_batch = remote_batch.clone();
            gapped_remote_batch.remote_task_id = String::from("remote-task-gap");
            gapped_remote_batch.first_logical_batch_index = 2;
            gapped_remote_batch.plan.request_id = String::from("remote-request-gap");
            gapped_remote_batch.plan.tasks[0].task_id = gapped_remote_batch.remote_task_id.clone();
            assert!(
                isolated_b
                    .persist_embedding_remote_batch_record(
                        large_manifest.run_id.clone(),
                        gapped_remote_batch,
                    )
                    .await
                    .is_err()
            );
            // Exercise every durable file-backed transition used around upload and exact reconciliation.
            // 覆盖文件上传与精确对账周围使用的每一条持久状态转换。
            remote_batch.state = VceEmbeddingRemoteBatchState::Uploading;
            remote_batch.updated_at_unix_ms = 3;
            isolated_b
                .persist_embedding_remote_batch_record(
                    large_manifest.run_id.clone(),
                    remote_batch.clone(),
                )
                .await
                .expect("persist uploading remote Batch");
            let uploaded_input = VceEmbeddingBatchUploadedInput {
                task_id: remote_batch.remote_task_id.clone(),
                provider_input_file_id: String::from("file-remote-task-0"),
                uploaded_at_unix_ms: 3,
            };
            remote_batch.uploaded_input = Some(uploaded_input.clone());
            remote_batch.state = VceEmbeddingRemoteBatchState::Uploaded;
            remote_batch.updated_at_unix_ms = 4;
            isolated_b
                .persist_embedding_remote_batch_record(
                    large_manifest.run_id.clone(),
                    remote_batch.clone(),
                )
                .await
                .expect("persist uploaded remote Batch");
            remote_batch.state = VceEmbeddingRemoteBatchState::Submitting;
            remote_batch.updated_at_unix_ms = 5;
            isolated_b
                .persist_embedding_remote_batch_record(
                    large_manifest.run_id.clone(),
                    remote_batch.clone(),
                )
                .await
                .expect("persist submitting remote Batch");
            remote_batch.state = VceEmbeddingRemoteBatchState::SubmissionUnknown;
            remote_batch.error = Some(String::from("submission response interrupted"));
            remote_batch.updated_at_unix_ms = 6;
            isolated_b
                .persist_embedding_remote_batch_record(
                    large_manifest.run_id.clone(),
                    remote_batch.clone(),
                )
                .await
                .expect("persist indeterminate remote Batch submission");
            let submission = VceEmbeddingBatchSubmission {
                task_id: remote_batch.remote_task_id.clone(),
                provider_batch_id: String::from("batch-remote-task-0"),
                provider_input_file_id: Some(uploaded_input.provider_input_file_id.clone()),
                submitted_at_unix_ms: 7,
            };
            remote_batch.submission = Some(submission.clone());
            remote_batch.state = VceEmbeddingRemoteBatchState::Submitted;
            remote_batch.error = None;
            remote_batch.next_poll_not_before_unix_ms = Some(60_007);
            remote_batch.updated_at_unix_ms = 7;
            isolated_b
                .persist_embedding_remote_batch_record(
                    large_manifest.run_id.clone(),
                    remote_batch.clone(),
                )
                .await
                .expect("persist reconciled remote Batch submission");
            remote_batch.snapshot = Some(VceEmbeddingBatchSnapshot {
                submission,
                state: VceEmbeddingBatchProviderState::Cancelled,
                total_requests: Some(1),
                completed_requests: Some(0),
                failed_requests: Some(0),
                output_file_id: None,
                error_file_id: None,
                error: Some(String::from("cancelled by fixture")),
            });
            remote_batch.state = VceEmbeddingRemoteBatchState::Cancelled;
            remote_batch.poll_attempt = 1;
            remote_batch.next_poll_not_before_unix_ms = None;
            remote_batch.error = Some(String::from("cancelled by fixture"));
            remote_batch.updated_at_unix_ms = 8;
            isolated_b
                .persist_embedding_remote_batch_record(
                    large_manifest.run_id.clone(),
                    remote_batch.clone(),
                )
                .await
                .expect("persist cancelled remote Batch");
            assert_eq!(
                isolated_b
                    .load_embedding_remote_batch_records(
                        large_manifest.run_id.clone(),
                        VceAiPurpose::CodeDocumentEmbedding,
                    )
                    .await
                    .expect("load terminal remote Batch record"),
                vec![remote_batch]
            );
            large_manifest.batches[0]
                .mark_running()
                .expect("mark large batch running");
            large_manifest.updated_at_unix_ms = 9;
            isolated_b
                .persist_embedding_batch_record(
                    large_manifest.run_id.clone(),
                    large_manifest.batches[0].clone(),
                    large_manifest.updated_at_unix_ms,
                )
                .await
                .expect("persist one large-manifest batch state");
            // Provider results are immutable and exact replay reconciles an ambiguous first response.
            // 供应商结果不可变，精确重放用于收敛首次响应不明确的情况。
            let large_result = vulcan_context_engine::VceEmbeddingBatchResult::from_vectors(
                &large_manifest.batches[0].batch_id,
                &[vulcan_context_engine::SegmentVector {
                    input_id: String::from("manifest-input-0"),
                    values: vec![0.25, 0.75, 0.0],
                }],
            );
            isolated_b
                .persist_embedding_batch_result(
                    large_manifest.run_id.clone(),
                    large_result.clone(),
                )
                .await
                .expect("persist provider result");
            isolated_b
                .persist_embedding_batch_result(
                    large_manifest.run_id.clone(),
                    large_result.clone(),
                )
                .await
                .expect("replay exact provider result");
            let conflicting_result = vulcan_context_engine::VceEmbeddingBatchResult::from_vectors(
                &large_manifest.batches[0].batch_id,
                &[vulcan_context_engine::SegmentVector {
                    input_id: String::from("manifest-input-0"),
                    values: vec![0.5, 0.5, 0.0],
                }],
            );
            assert!(
                isolated_b
                    .persist_embedding_batch_result(
                        large_manifest.run_id.clone(),
                        conflicting_result,
                    )
                    .await
                    .is_err()
            );

            // A parent-update failure must roll back its triggering batch change in the same statement.
            // 父记录更新失败必须在同一条语句内回滚触发它的批次变更。
            isolated_b.data_plane.execute(
                "CREATE TRIGGER reject_manifest_parent_update BEFORE UPDATE ON vce_embedding_runs WHEN OLD.run_id = 'large-manifest-run' BEGIN SELECT RAISE(ABORT, 'injected parent failure'); END;",
                Vec::new(), "inject atomic batch update failure",
            ).await.expect("inject atomic batch update failure");
            // The proposed success remains uncommitted when the linked revision cannot advance.
            // 当关联修订无法推进时，拟议成功状态保持未提交。
            let mut rejected_batch = large_manifest.batches[0].clone();
            rejected_batch.mark_succeeded();
            assert!(isolated_b.persist_embedding_batch_record(large_manifest.run_id.clone(), rejected_batch, 10).await.is_err());
            isolated_b.data_plane.execute(
                "DROP TRIGGER reject_manifest_parent_update;",
                Vec::new(), "remove atomic batch failure fixture",
            ).await.expect("remove atomic batch failure fixture");
            assert_eq!(isolated_b.load_embedding_manifest(large_manifest.run_id.clone()).await.expect("read rolled-back batch"), Some(large_manifest.clone()));
            large_manifest.state = VceEmbeddingRunState::Cancelled;
            large_manifest.updated_at_unix_ms = 11;
            isolated_b
                .persist_embedding_run_state(
                    large_manifest.run_id.clone(),
                    large_manifest.state,
                    large_manifest.updated_at_unix_ms,
                )
                .await
                .expect("persist one large-manifest run state");
            assert_eq!(
                isolated_b
                    .load_embedding_manifest(large_manifest.run_id.clone())
                    .await
                    .expect("reload granular manifest state"),
                Some(large_manifest.clone())
            );
            let final_revision: Vec<super::CountRow> = isolated_b
                .data_plane
                .query_rows(
                    "SELECT revision AS count FROM vce_embedding_runs WHERE namespace_id = ? AND run_id = ?;",
                    vec![
                        super::sqlite_text(&isolated_b.binding().namespace_id),
                        super::sqlite_text(&large_manifest.run_id),
                    ],
                )
                .await
                .expect("read granular manifest revision");
            assert_eq!(final_revision[0].count, 11);
            isolated_b.data_plane.execute(
                "DROP TRIGGER reject_manifest_node_insert; DROP TRIGGER reject_manifest_node_delete; DROP TRIGGER reject_manifest_node_update;",
                Vec::new(), "remove immutable node guards",
            ).await.expect("remove immutable node guards");

            // A partially persisted first plan can be replaced before any provider work was admitted.
            // 在任何供应商工作获准前，首次持久化只完成部分的计划可以重新准备并替换。
            let mut interrupted_manifest = VceEmbeddingRunManifest {
                run_id: String::from("interrupted-manifest-run"),
                nodes: large_manifest.nodes[..1].to_vec(),
                batches: Vec::new(), reused_blob_names: Vec::new(),
                state: VceEmbeddingRunState::Planned,
                created_at_unix_ms: 10, updated_at_unix_ms: 10,
                ..large_manifest.clone()
            };
            isolated_b.data_plane.execute(
                "CREATE TRIGGER reject_manifest_chunk BEFORE INSERT ON vce_embedding_run_nodes WHEN NEW.run_id = 'interrupted-manifest-run' BEGIN SELECT RAISE(ABORT, 'injected partial plan failure'); END;",
                Vec::new(), "inject first-plan interruption",
            ).await.expect("inject first-plan interruption");
            assert!(isolated_b.persist_embedding_manifest(interrupted_manifest.clone()).await.is_err());
            assert!(isolated_b.load_embedding_manifest(interrupted_manifest.run_id.clone()).await.expect("hide interrupted plan").is_none());
            isolated_b.data_plane.execute("DROP TRIGGER reject_manifest_chunk;", Vec::new(), "remove partial-plan failure").await.expect("remove partial-plan failure");
            interrupted_manifest.created_at_unix_ms = 20;
            interrupted_manifest.updated_at_unix_ms = 20;
            interrupted_manifest.nodes[0].path = String::from("src/reprepared.rs");
            isolated_b.persist_embedding_manifest(interrupted_manifest.clone()).await.expect("retry incomplete plan with a new creation time");
            assert_eq!(isolated_b.load_embedding_manifest(interrupted_manifest.run_id.clone()).await.expect("read retried plan"), Some(interrupted_manifest));

            // An interrupted first write has no readable envelope and therefore cannot admit provider I/O.
            // 中断的首次写入没有可读概要，因此不能准入供应商调用。
            isolated_b
                .data_plane
                .execute(
                    r#"INSERT INTO vce_embedding_runs(
                       namespace_id, run_id, generation, state, manifest_json,
                       layout_version, manifest_complete, plan_fingerprint,
                       created_at_unix_ms, updated_at_unix_ms
                   ) VALUES (?, 'partial-manifest-run', 1, 'planned', '{}', 2, 0, ?, 1, 1);"#,
                    vec![
                        super::sqlite_text(&isolated_b.binding().namespace_id),
                        SqliteValue::Text("f".repeat(64)),
                    ],
                    "create partial manifest fixture",
                )
                .await
                .expect("create partial manifest fixture");
            assert!(
                isolated_b
                    .load_embedding_manifest(String::from("partial-manifest-run"))
                    .await
                    .expect("hide partial manifest")
                    .is_none()
            );
            // The v1 schema itself forbids every layout but the pinned one, so a manifest row in an earlier
            // layout cannot exist in a database this build created. This is the assertion that replaced the
            // dual-read path: without it, "there is only one layout" would be a convention that the loader
            // happens to follow, and the first re-introduced decode branch would silently start reading
            // storage the schema was supposed to make impossible.
            // v1 结构自身禁止除固定布局外的任何布局，因此本构建创建的数据库中不可能存在更早布局的清单记录。
            // 这就是取代双读路径的那条断言：若没有它，"只有一种布局"会沦为加载器恰好遵守的约定，而第一个被重新
            // 引入的解码分支就会开始静默读取本该被结构排除在外的存储。
            let refused = isolated_b
                .data_plane
                .execute(
                    r#"INSERT INTO vce_embedding_runs(
                           namespace_id, run_id, generation, state, manifest_json,
                           layout_version, manifest_complete, plan_fingerprint,
                           created_at_unix_ms, updated_at_unix_ms
                       ) VALUES (?, 'pre-v1-layout-run', 1, 'running', '{}', 1, 1, NULL, 10, 10);"#,
                    vec![super::sqlite_text(&isolated_b.binding().namespace_id)],
                    "probe a pre-v1 manifest layout",
                )
                .await;
            assert!(
                refused.is_err(),
                "the v1 schema must reject a manifest row in any layout but the pinned one"
            );
        })
        .await;
    // jobs is the exact physical project repository; a lookup cannot create its receipt table.
    // jobs 是精确物理项目仓库；查找不能创建其回执表。
    let jobs = isolated_a.job_store();
    assert!(
        jobs.latest()
            .await
            .expect("read absent latest job")
            .is_none()
    );
    assert!(
        jobs.unfinished()
            .await
            .expect("read absent job owner")
            .is_none()
    );
    assert!(
        jobs.get("job-a")
            .await
            .expect("read absent job table")
            .is_none()
    );
    // queued freezes the original command separately from mutable project update policy.
    // queued 将原始命令与可变项目更新策略分开冻结。
    let queued = super::VceStoredJob {
        schema_version: super::VCE_JOB_RECEIPT_SCHEMA_VERSION,
        replacement: None,
        compaction: None,
        project_id: String::from("isolated-a"),
        job_id: String::from("job-a"),
        owner_run_id: String::from("host-run-a"),
        idempotency_key: String::from("request-a"),
        request_fingerprint: "a".repeat(64),
        contract_fingerprint: "b".repeat(64),
        kind: super::VceJobKind::Initialize,
        state: super::VceJobState::Queued,
        sequence: 1,
        created_at_unix_ms: 1000,
        updated_at_unix_ms: 1000,
        admitted_settings_revision: 1,
        source_checkpoint_version: None,
        retry_of_job_id: None,
        result_checkpoint_version: None,
        progress: None,
        followup_requested: false,
        completed_update: None,
        error: None,
    };
    assert_eq!(
        jobs.register(&queued)
            .await
            .expect("durably register before admission response"),
        queued
    );
    assert_eq!(
        jobs.latest().await.expect("read queued receipt"),
        Some(queued.clone())
    );
    assert_eq!(
        jobs.unfinished().await.expect("read active owner"),
        Some(queued.clone())
    );
    assert_eq!(
        jobs.register(&queued)
            .await
            .expect("replay exact admission"),
        queued
    );
    assert!(
        isolated_b
            .job_store()
            .get("job-a")
            .await
            .expect("peer job lookup")
            .is_none()
    );
    assert!(isolated_b.job_store().register(&queued).await.is_err());
    // conflict reuses the key for a different command and must preserve the first receipt.
    // conflict 为不同命令复用键，必须保留首个回执。
    let mut conflict = queued.clone();
    conflict.request_fingerprint = "c".repeat(64);
    assert!(jobs.register(&conflict).await.is_err());
    // running advances only the acknowledged prior sequence.
    // running 仅推进已确认的先前序号。
    let mut running = queued.clone();
    running.state = super::VceJobState::Running;
    running.sequence = 2;
    running.updated_at_unix_ms = 1001;
    assert_eq!(
        jobs.update(&queued, &running)
            .await
            .expect("start durable job"),
        running
    );
    assert!(jobs.update(&queued, &running).await.is_err());
    // failed remains a durable terminal receipt; retry must be a distinct job identity.
    // failed 保持持久终态回执；重试必须使用不同作业身份。
    let mut failed = running.clone();
    failed.state = super::VceJobState::Failed;
    failed.sequence = 3;
    failed.error = Some(String::from("fixture failure"));
    assert_eq!(
        jobs.update(&running, &failed)
            .await
            .expect("record terminal failure"),
        failed
    );
    assert_eq!(
        jobs.register(&queued)
            .await
            .expect("replay terminal admission without running again"),
        failed
    );
    running.sequence = 4;
    assert!(jobs.update(&failed, &running).await.is_err());
    assert_eq!(
        jobs.latest().await.expect("read latest terminal receipt"),
        Some(failed)
    );
    assert!(
        jobs.unfinished()
            .await
            .expect("terminal job releases owner")
            .is_none()
    );
    // A synchronization may retain one later batch and immutable aliases without admitting another writer.
    // 同步可保留一个后续批次和不可变别名，而不准入另一写入者。
    let mut sync_queued = queued.clone();
    sync_queued.job_id = String::from("job-sync");
    sync_queued.idempotency_key = String::from("request-sync");
    sync_queued.kind = super::VceJobKind::Synchronize;
    sync_queued.source_checkpoint_version = Some(1);
    sync_queued.created_at_unix_ms = 1100;
    sync_queued.updated_at_unix_ms = 1100;
    jobs.register(&sync_queued)
        .await
        .expect("register synchronization fixture");
    let mut sync_running = sync_queued.clone();
    sync_running.state = super::VceJobState::Running;
    sync_running.sequence += 1;
    jobs.update(&sync_queued, &sync_running)
        .await
        .expect("start synchronization fixture");
    let followup_key = "request-followup'quoted";
    let coalesced = jobs
        .coalesce(&sync_running, followup_key, &"c".repeat(64), 1110, true)
        .await
        .expect("atomically register follow-up");
    assert!(coalesced.followup_requested);
    assert_eq!(
        jobs.coalesce(&sync_running, followup_key, &"c".repeat(64), 1120, true)
            .await
            .expect("same request never adds work"),
        coalesced
    );
    assert!(
        jobs.find_submission(followup_key, &"d".repeat(64))
            .await
            .is_err()
    );
    assert!(
        isolated_b
            .job_store()
            .find_submission(followup_key, &"c".repeat(64))
            .await
            .expect("peer cannot see alias")
            .is_none()
    );
    // Stale CAS must roll back the alias too; a retry cannot masquerade as an already accepted request.
    // 过期比较交换也必须回滚别名；重试不能伪装为已受理请求。
    assert!(
        jobs.coalesce(&sync_running, "stale-followup", &"d".repeat(64), 1120, true)
            .await
            .is_err()
    );
    assert!(
        jobs.find_submission("stale-followup", &"d".repeat(64))
            .await
            .expect("query after transactional rollback")
            .is_none()
    );
    let mut premature = coalesced.clone();
    premature.sequence += 1;
    premature.followup_requested = false;
    premature.state = super::VceJobState::Queued;
    assert!(jobs.update(&coalesced, &premature).await.is_err());
    // A later failed batch must preserve the exact prior batch success timestamp and checkpoint.
    // 后续失败批次必须保留精确的先前批次成功时间及检查点。
    let mut batch_complete = coalesced.clone();
    batch_complete.sequence += 1;
    batch_complete.updated_at_unix_ms = 1200;
    batch_complete.completed_update = Some(super::VceCompletedJobUpdate {
        checkpoint_version: 1,
        at_unix_ms: 1200,
    });
    jobs.update(&coalesced, &batch_complete)
        .await
        .expect("record successful batch");
    let mut erased_success = batch_complete.clone();
    erased_success.sequence += 1;
    erased_success.completed_update = None;
    assert!(jobs.update(&batch_complete, &erased_success).await.is_err());
    // The sole receipt format requires explicit batch evidence instead of a legacy timestamp fallback.
    // 唯一回执格式要求明确批次证据，而不是旧时间戳兜底。
    let mut incomplete_success = erased_success.clone();
    incomplete_success.state = super::VceJobState::Succeeded;
    incomplete_success.result_checkpoint_version = Some(1);
    incomplete_success.followup_requested = false;
    assert!(incomplete_success.validate().is_err());
    assert!(serde_json::from_str::<super::VceJobKind>("\"migrate\"").is_err());
    let mut following = batch_complete.clone();
    following.sequence += 1;
    following.state = super::VceJobState::Queued;
    following.followup_requested = false;
    jobs.update(&batch_complete, &following)
        .await
        .expect("queue acknowledged successor");
    let mut failed_followup = following.clone();
    failed_followup.sequence += 1;
    failed_followup.state = super::VceJobState::Failed;
    failed_followup.error = Some(String::from("follow-up fixture failure"));
    failed_followup.updated_at_unix_ms = 1300;
    jobs.update(&following, &failed_followup)
        .await
        .expect("preserve partial successful publication");
    assert_eq!(
        jobs.latest_successful_update()
            .await
            .expect("read last successful batch"),
        Some(failed_followup.clone())
    );
    assert_eq!(
        jobs.find_submission(followup_key, &"c".repeat(64))
            .await
            .expect("terminal alias does not create a new job"),
        Some(failed_followup)
    );
    drop(jobs);
    assert!(
        isolated_a
            .load_state_snapshot()
            .await
            .expect("new physical store has no content checkpoint")
            .is_none()
    );
    assert!(
        isolated_a
            .data_plane
            .load_active_project_config("isolated-b")
            .await
            .is_err()
    );
    assert!(
        factory
            .create_project_index(test_project_config("isolated-a", &second_root, 2))
            .await
            .is_err()
    );
    assert!(
        factory
            .close_project_bindings()
            .await
            .expect_err("live leases protect stores")
            .contains("live project leases")
    );
    assert!(isolated_a.load_state_snapshot().await.is_ok());
    // held leases fill the physical resource budget; no live project can be evicted.
    // held 租约填满物理资源预算；任何活动项目均不得被淘汰。
    let mut held = Vec::new();
    for number in 0..6 {
        held.push(
            factory
                .create_project_index(test_project_config(
                    &format!("budget-{number}"),
                    &first_root,
                    1,
                ))
                .await
                .expect("fill bounded cache"),
        );
    }
    assert!(
        factory
            .create_project_index(test_project_config("budget-overflow", &first_root, 1))
            .await
            .expect_err("live lease budget cannot be exceeded")
            .contains("budget")
    );
    assert!(isolated_b.load_state_snapshot().await.is_ok());
    drop(held);
    // retried proves an explicitly requested creation can reuse capacity after leases drain.
    // retried 证明租约排空后明确请求的创建能够复用容量。
    let retried = factory
        .create_project_index(test_project_config("budget-overflow", &first_root, 1))
        .await
        .expect("retry after idle lease capacity returns");
    drop(retried);
    drop(isolated_a);
    drop(isolated_b);
    factory
        .close_project_bindings()
        .await
        .expect("release idle project bindings");
    assert!(
        factory
            .load_active_project_config("isolated-a")
            .await
            .expect("lazy reopen after drained shutdown")
            .is_some()
    );
    factory.shutdown().await.expect("close isolated factory");
    // Verify first-allocation failure separately before the ordinary publication fixture starts.
    // 在普通发布夹具开始之前单独验证首次分配失败。
    verify_initial_allocation_retry(&host_runtime, &first_root).await;
    // Exercise registration rollback and cleanup failure using isolated real-runtime ownership.
    // 使用隔离的真实运行时归属验证登记回滚及清理失败。
    for (project_id, interrupt_cleanup) in [
        ("initial-registration-failure", false),
        ("initial-cleanup-failure", true),
    ] {
        // Await each owned task separately so its deep operation polling does not nest in the large parent fixture.
        // 分别等待每个自有任务，避免深层操作轮询嵌入大型父夹具栈。
        let failure_host_runtime = host_runtime.clone();
        // The same exact project target is moved into the sequential current-thread task.
        // 同一精确项目目标移入串行的当前线程任务。
        let failure_config = test_project_config(project_id, &first_root, 1);
        tokio::spawn(async move {
            super::initial_allocation::verify_initial_allocation_failures(
                &failure_host_runtime,
                failure_config,
                interrupt_cleanup,
            )
            .await;
        })
        .await
        .expect("complete isolated initial allocation failure assertions");
    }
    // Use the same project-isolated factory as production; no legacy shared-store fixture.
    // 使用与生产相同的项目隔离工厂，不使用旧共享库夹具。
    let current_factory = bootstrap_vce_data_plane(&host_runtime)
        .await
        .expect("bootstrap current VCE factory");
    let first = test_project_config("project-1", &first_root, 1);
    assert!(
        current_factory
            .open_existing_project_index(&first)
            .await
            .expect("read never-created project")
            .is_none()
    );
    assert!(
        current_factory
            .load_active_project_config("project-1")
            .await
            .expect("read missing origin")
            .is_none()
    );
    let first_index = current_factory
        .create_project_index(first)
        .await
        .expect("open first VCE project binding");
    // Inspect only this project's host-owned metadata through its actual binding.
    // 仅通过此项目的实际绑定检查由宿主存储支持的元数据。
    let data_plane = first_index.data_plane.clone();
    assert!(
        first_index
            .binding()
            .namespace_id
            .starts_with("vce:project:project-1:root:")
    );
    let second = test_project_config("project-1", &second_root, 2);
    let second_index = data_plane
        .open_project_index(second.clone())
        .await
        .expect("rotate VCE project binding");
    let source_content = "pub fn semantic_entry() {}";
    let blob_name = blob_name("src/lib.rs", source_content)
        .expect("build canonical VCE integration Blob identity");
    let chunk = Chunk::new(
        blob_name.clone(),
        "src/lib.rs",
        source_content,
        1,
        1,
        ChunkKind::Recursive,
    )
    .expect("build VCE integration Chunk");
    let vector = EmbeddingVector {
        input_id: chunk.chunk_id.clone(),
        values: vec![1.0, 0.0, 0.0],
    };
    let indexed_blob = IndexedBlob {
        blob_name: blob_name.clone(),
        path: String::from("src/lib.rs"),
        generation: 1,
        structure_analyzer_id: None,
        structure_mode: None,
        structure_fallback_reason: None,
        chunks: vec![chunk.clone()],
        embedding_vectors: vec![vector.clone()],
        path_document: None,
        path_embedding: None,
        symbols: Vec::new(),
        path_status: AuxiliaryIndexStatus::NotApplicable,
        symbol_status: AuxiliaryIndexStatus::NotApplicable,
        status: IndexedBlobStatus::Ready,
    };
    // Heap-box this isolated fault sequence so the already-large parent async test stays bounded.
    // 将隔离故障序列装箱到堆上，使原本已很大的父异步测试保持有界。
    Box::pin(async {
        // Two complete Blobs prove stage-local aggregation and exercise both physical tables.
        // 两个完整 Blob 用于证明阶段内聚合，并覆盖两张物理表。
        let primary_path_document = PathDocument::new(blob_name.clone(), "src/lib.rs")
            .expect("build primary path document");
        let mut primary_batched_blob = indexed_blob.clone();
        primary_batched_blob.path_embedding = Some(EmbeddingVector {
            input_id: primary_path_document.path_id.clone(),
            values: vec![0.0, 1.0, 0.0],
        });
        primary_batched_blob.path_document = Some(primary_path_document);
        primary_batched_blob.path_status = AuxiliaryIndexStatus::Ready;
        let secondary_path = "src/secondary.rs";
        let secondary_source = "pub fn secondary_entry() {}";
        let secondary_blob_name =
            vulcan_context_engine::blob_name(secondary_path, secondary_source)
                .expect("build secondary canonical Blob identity");
        let secondary_chunk = Chunk::new(
            secondary_blob_name.clone(),
            secondary_path,
            secondary_source,
            1,
            1,
            ChunkKind::Recursive,
        )
        .expect("build secondary VCE integration Chunk");
        let secondary_path_document =
            PathDocument::new(secondary_blob_name.clone(), secondary_path)
                .expect("build secondary path document");
        let secondary_batched_blob = IndexedBlob {
            blob_name: secondary_blob_name.clone(),
            path: secondary_path.to_owned(),
            generation: 1,
            structure_analyzer_id: None,
            structure_mode: None,
            structure_fallback_reason: None,
            chunks: vec![secondary_chunk.clone()],
            embedding_vectors: vec![EmbeddingVector {
                input_id: secondary_chunk.chunk_id,
                values: vec![0.0, 0.0, 1.0],
            }],
            path_embedding: Some(EmbeddingVector {
                input_id: secondary_path_document.path_id.clone(),
                values: vec![0.0, 1.0, 0.0],
            }),
            path_document: Some(secondary_path_document),
            symbols: Vec::new(),
            path_status: AuxiliaryIndexStatus::Ready,
            symbol_status: AuxiliaryIndexStatus::NotApplicable,
            status: IndexedBlobStatus::Ready,
        };
        let batched_blobs = vec![primary_batched_blob, secondary_batched_blob];
        let expected_batched_names = BTreeSet::from([blob_name.clone(), secondary_blob_name]);

        // Replace only the empty temporary path table with an incompatible schema so content succeeds first.
        // 仅将空的临时路径表替换为不兼容结构，使内容表先成功而路径表随后失败。
        let content_baseline = second_index
            .physical_table_statistics(VcePhysicalTableRole::Content)
            .await
            .expect("read content baseline before child-batch fault");
        let path_baseline = second_index
            .physical_table_statistics(VcePhysicalTableRole::Path)
            .await
            .expect("read path baseline before child-batch fault");
        assert_eq!(content_baseline.row_count, 0);
        assert_eq!(path_baseline.row_count, 0);
        data_plane
            .vector_engine()
            .await
            .expect("open fixture vector engine")
            .drop_table(LanceDbDropTableInput {
                table_name: second_index.path_table.to_string(),
            })
            .await
            .expect("replace empty path table for child-batch fault");
        data_plane
            .vector_engine()
            .await
            .expect("open fixture vector engine")
            .create_table(LanceDbCreateTableInput {
                table_name: second_index.path_table.clone(),
                columns: super::vector_table_columns(4),
                overwrite_if_exists: false,
            })
            .await
            .expect("create incompatible path table fixture");
        let child_failure_request = IndexStageBatch {
            operation_id: String::from("stage-child-batch-retry"),
            refresh_id: String::from("refresh-child-batch-retry"),
            generation: 1,
            blobs: batched_blobs.clone(),
        };
        let child_failure = second_index
            .stage(child_failure_request.clone())
            .await
            .expect_err("incompatible second table must reject the stage after content succeeds");
        assert!(child_failure.to_string().contains(&second_index.path_table));
        let content_after_child_failure = second_index
            .physical_table_statistics(VcePhysicalTableRole::Content)
            .await
            .expect("read content rows after child-batch fault");
        assert_eq!(content_after_child_failure.row_count, 2);
        assert_eq!(content_after_child_failure.fragment_count, 1);
        assert!(
            second_index
                .staged_blob_names("refresh-child-batch-retry", 1)
                .await
                .expect("read absent metadata after child-batch fault")
                .is_empty()
        );

        // Restore the correct table and replay the exact stage; stable row IDs prevent content duplication.
        // 恢复正确表并重放同一阶段；稳定行标识会阻止内容向量重复。
        data_plane
            .vector_engine()
            .await
            .expect("open fixture vector engine")
            .drop_table(LanceDbDropTableInput {
                table_name: second_index.path_table.to_string(),
            })
            .await
            .expect("drop incompatible path table fixture");
        data_plane
            .ensure_vector_table(&second_index.path_table, second_index.dimension)
            .await
            .expect("restore compatible path table");
        second_index
            .stage(child_failure_request)
            .await
            .expect("resume exact stage after path-table repair");
        let content_after_child_retry = second_index
            .physical_table_statistics(VcePhysicalTableRole::Content)
            .await
            .expect("read content rows after child-batch retry");
        let path_after_child_retry = second_index
            .physical_table_statistics(VcePhysicalTableRole::Path)
            .await
            .expect("read path rows after child-batch retry");
        assert_eq!(content_after_child_retry.row_count, 2);
        assert_eq!(path_after_child_retry.row_count, 2);
        assert_eq!(path_after_child_retry.fragment_count, 1);
        assert_eq!(
            second_index
                .staged_blob_names("refresh-child-batch-retry", 1)
                .await
                .expect("read resumed child-batch metadata")
                .into_iter()
                .collect::<BTreeSet<_>>(),
            expected_batched_names
        );
        second_index
            .abort_stage(IndexAbortRequest {
                operation_id: String::from("abort-child-batch-retry"),
                refresh_id: String::from("refresh-child-batch-retry"),
                generation: 1,
            })
            .await
            .expect("clean child-batch retry fixture");
        assert_eq!(
            second_index
                .physical_table_statistics(VcePhysicalTableRole::Content)
                .await
                .expect("read content rows after child-batch cleanup")
                .row_count,
            0
        );
        assert_eq!(
            second_index
                .physical_table_statistics(VcePhysicalTableRole::Path)
                .await
                .expect("read path rows after child-batch cleanup")
                .row_count,
            0
        );

        // Reject SQLite metadata after both vector writes, then prove an exact retry publishes no duplicates.
        // 在两张向量表均写入后拒绝 SQLite 元数据，再证明精确重试不会发布重复行。
        data_plane
            .execute(
                "CREATE TRIGGER inject_stage_metadata_failure BEFORE INSERT ON vce_staged_blobs
                 WHEN NEW.refresh_id = 'refresh-metadata-retry'
                 BEGIN SELECT RAISE(ABORT, 'injected stage metadata failure'); END;",
                Vec::new(),
                "install stage metadata failure fixture",
            )
            .await
            .expect("install stage metadata failure fixture");
        let metadata_failure_request = IndexStageBatch {
            operation_id: String::from("stage-metadata-retry"),
            refresh_id: String::from("refresh-metadata-retry"),
            generation: 1,
            blobs: batched_blobs,
        };
        let metadata_failure = second_index
            .stage(metadata_failure_request.clone())
            .await
            .expect_err("metadata trigger must reject stage after both vector writes");
        assert!(
            metadata_failure
                .to_string()
                .contains("injected stage metadata failure")
        );
        assert_eq!(
            second_index
                .physical_table_statistics(VcePhysicalTableRole::Content)
                .await
                .expect("read content rows after metadata fault")
                .row_count,
            2
        );
        assert_eq!(
            second_index
                .physical_table_statistics(VcePhysicalTableRole::Path)
                .await
                .expect("read path rows after metadata fault")
                .row_count,
            2
        );
        assert!(
            second_index
                .staged_blob_names("refresh-metadata-retry", 1)
                .await
                .expect("read absent staged metadata after injected fault")
                .is_empty()
        );
        data_plane
            .execute(
                "DROP TRIGGER inject_stage_metadata_failure;",
                Vec::new(),
                "remove stage metadata failure fixture",
            )
            .await
            .expect("remove stage metadata failure fixture");
        second_index
            .stage(metadata_failure_request)
            .await
            .expect("resume exact stage after metadata failure");
        assert_eq!(
            second_index
                .physical_table_statistics(VcePhysicalTableRole::Content)
                .await
                .expect("read content rows after metadata retry")
                .row_count,
            2
        );
        assert_eq!(
            second_index
                .physical_table_statistics(VcePhysicalTableRole::Path)
                .await
                .expect("read path rows after metadata retry")
                .row_count,
            2
        );
        assert_eq!(
            second_index
                .staged_blob_names("refresh-metadata-retry", 1)
                .await
                .expect("read staged metadata after exact retry")
                .into_iter()
                .collect::<BTreeSet<_>>(),
            expected_batched_names
        );
        second_index
            .abort_stage(IndexAbortRequest {
                operation_id: String::from("abort-metadata-retry"),
                refresh_id: String::from("refresh-metadata-retry"),
                generation: 1,
            })
            .await
            .expect("clean metadata retry fixture");
    })
    .await;
    let stage_request = IndexStageBatch {
        operation_id: String::from("stage-one"),
        refresh_id: String::from("refresh-one"),
        generation: 1,
        blobs: vec![indexed_blob.clone()],
    };
    let stage = second_index
        .stage(stage_request.clone())
        .await
        .expect("stage VCE integration Blob");
    assert_eq!(stage.staged_blob_names, vec![blob_name.clone()]);
    // Identical content under distinct refresh IDs must still have independent cleanup ownership.
    // 不同刷新编号下的相同内容仍必须具有独立清理归属。
    for refresh_id in ["owned-panic", "owned-panic-suffix"] {
        second_index
            .stage(IndexStageBatch {
                operation_id: format!("{refresh_id}:stage"),
                refresh_id: refresh_id.to_owned(),
                generation: 1,
                blobs: vec![indexed_blob.clone()],
            })
            .await
            .expect("stage independently owned attempts");
    }
    // Only the host-observed exact attempt is in-flight, not an orphan; similar names stay visible.
    // 仅宿主观察到的精确尝试属于在途而非孤立；相似名称仍须显示。
    let observed_issues = second_index
        .management_read_for_refresh(Some("owned-panic"))
        .integrity_issues()
        .await
        .expect("inspect observed staging");
    assert_eq!(observed_issues.len(), 2);
    assert!(
        observed_issues
            .iter()
            .any(|issue| issue.message.contains("owned-panic-suffix"))
    );
    assert!(
        observed_issues
            .iter()
            .any(|issue| issue.message.contains("refresh-one"))
    );
    assert_eq!(
        observed_issues
            .iter()
            .map(|issue| &issue.issue_id)
            .collect::<BTreeSet<_>>()
            .len(),
        2
    );
    // The read-local exclusion must not change the original handle or hide unconfirmed producers.
    // 读取本地排除不能改变原句柄，也不能隐藏未确认生产者。
    let unobserved_issues = second_index
        .integrity_issues()
        .await
        .expect("inspect unconfirmed staging");
    assert_eq!(unobserved_issues.len(), 3);
    assert_eq!(
        unobserved_issues
            .iter()
            .map(|issue| &issue.issue_id)
            .collect::<BTreeSet<_>>()
            .len(),
        3
    );
    second_index
        .retry_pending_cleanup()
        .await
        .expect("retry only authorized cleanup");
    // Reopening the binding cannot infer that earlier producers have stopped.
    // 重新打开绑定不能推断先前生产者已经停止。
    let reopened = data_plane
        .open_project_index(second)
        .await
        .expect("reopen without claiming staging");
    assert!(
        reopened
            .has_unresolved_staging()
            .await
            .expect("inspect retained staging")
    );
    for refresh_id in ["refresh-one", "owned-panic", "owned-panic-suffix"] {
        assert_eq!(
            reopened
                .staged_blob_names(refresh_id, 1)
                .await
                .expect("opening preserved exact attempt"),
            BTreeSet::from([blob_name.clone()])
        );
    }
    reopened
        .abort_refresh_staging("owned-panic")
        .await
        .expect("abort only the stopped owned attempt");
    assert!(
        reopened
            .staged_blob_names("owned-panic", 1)
            .await
            .expect("owned staging cleared")
            .is_empty()
    );
    assert_eq!(
        reopened
            .staged_blob_names("owned-panic-suffix", 1)
            .await
            .expect("similar foreign identity survives"),
        BTreeSet::from([blob_name.clone()])
    );
    assert_eq!(
        reopened
            .staged_blob_names("refresh-one", 1)
            .await
            .expect("primary writer survives"),
        BTreeSet::from([blob_name.clone()])
    );
    assert!(reopened.abort_refresh_staging(" ").await.is_err());
    // The test explicitly owns the second fixture too, so it may finish that exact attempt separately.
    // 测试也明确持有第二个夹具，因此可单独结束该精确尝试。
    reopened
        .abort_refresh_staging("owned-panic-suffix")
        .await
        .expect("fixture owner finishes its own attempt");
    drop(reopened);
    let checkpoint = Checkpoint::new(
        second_index.binding().checkpoint_chain_id.clone(),
        1,
        [blob_name.clone()],
    );
    let state_snapshot = VceStateSnapshot::new(
        VceStateIdentity::new(
            second_index.binding().namespace_id.clone(),
            second_index.binding().checkpoint_chain_id.clone(),
            second_index.binding().schema_version,
            second_index.binding().embedding_space.clone(),
        ),
        WorkspaceSnapshot {
            generation: 1,
            checkpoint: Some(checkpoint.clone()),
            records: vec![FileRecord {
                path: String::from("src/lib.rs"),
                blob_name: Some(blob_name.clone()),
                committed_blob_name: Some(blob_name.clone()),
                status: FileStatus::Present,
                size: source_content.len() as u64,
                mtime_ns: None,
                source: FileSource::Filesystem,
                content: None,
                generation: 1,
                skip_reason: None,
            }],
        },
        VceQueryStatistics {
            refresh_attempts: 1,
            refresh_successes: 1,
            ..VceQueryStatistics::default()
        },
        Some(VceRefreshSummary {
            request_id: String::from("refresh-one"),
            mode: RefreshMode::Full,
            reason: RefreshReason::Initial,
            generation: 1,
            added_blob_count: 1,
            deleted_blob_count: 0,
            indexed_blob_count: 1,
            skipped_path_count: 0,
            checkpoint_version: Some(1),
            source_revision: Some(String::from("source-one")),
        }),
        None,
        None,
    );
    let commit_request = IndexCommit {
        request_id: String::from("commit-one"),
        generation: 1,
        refresh_id: String::from("refresh-one"),
        source_revision: Some(String::from("source-one")),
        deleted_blobs: BTreeSet::new(),
        staged_blob_names: BTreeSet::from([blob_name.clone()]),
        expected_checkpoint_version: None,
        checkpoint,
        state_snapshot: state_snapshot.clone(),
        finalize_embedding_manifest: false,
    };
    // A missing host owner is rejected before any checkpoint or lease becomes durable.
    // 缺少宿主所有者时，在任何检查点或租约持久化之前拒绝。
    assert!(
        second_index
            .commit_pinned(commit_request.clone(), " ")
            .await
            .is_err()
    );
    // Inject a failure at the transaction's final lease insertion, after all checkpoint writes.
    // 在检查点全部写入之后、事务最终租约插入处注入失败。
    data_plane.execute(
            "CREATE TRIGGER vce_fixture_reject_read_lease BEFORE INSERT ON vce_read_leases BEGIN SELECT RAISE(ROLLBACK, 'fixture rejects publication lease'); END;",
            Vec::new(), "install fixture lease failure",
        ).await.expect("install lease rejection");
    assert!(
        second_index
            .commit_pinned(commit_request.clone(), "binding-fixture-run")
            .await
            .is_err()
    );
    assert!(
        second_index
            .current_checkpoint_row()
            .await
            .expect("checkpoint after rollback")
            .is_none()
    );
    // Counts prove this is atomic rollback, not a committed checkpoint reported as a failed job.
    // 计数证明这是原子回滚，而不是已提交检查点却报告作业失败。
    let rolled_back: Vec<super::CountRow> = data_plane.query_rows(
            "SELECT (SELECT COUNT(*) FROM vce_read_leases) + (SELECT COUNT(*) FROM vce_read_checkpoints) + (SELECT COUNT(*) FROM vce_blob_versions) + (SELECT COUNT(*) FROM vce_commit_receipts) AS count;",
            Vec::new(),
        ).await.expect("count rolled-back publication writes");
    assert_eq!(rolled_back[0].count, 0);
    assert_eq!(
        second_index
            .staged_blob_names("refresh-one", 1)
            .await
            .expect("retained staging"),
        commit_request.staged_blob_names
    );
    data_plane
        .execute(
            "DROP TRIGGER vce_fixture_reject_read_lease;",
            Vec::new(),
            "remove fixture lease failure",
        )
        .await
        .expect("remove lease rejection");
    // Retry the identical request after rollback; commit returns an already retained query view.
    // 回滚后重试完全相同的请求；提交直接返回已保留的查询视图。
    let (first_commit, committed_read_index) = second_index
        .commit_pinned(commit_request.clone(), "binding-fixture-run")
        .await
        .expect("commit VCE integration Blob");
    assert_eq!(
        committed_read_index
            .embedding_provenance()
            .await
            .expect("retained provenance"),
        second_index
            .embedding_provenance()
            .await
            .expect("durable provenance")
    );
    // committed_leases counts exact host ownership, including failed-attempt cleanup residue.
    // committed_leases 按精确宿主归属计数，包含失败尝试的清理残留。
    let committed_leases: Vec<super::CountRow> = data_plane
        .query_rows(
            "SELECT COUNT(*) AS count FROM vce_read_leases WHERE owner_run_id = ?;",
            vec![super::sqlite_text("binding-fixture-run")],
        )
        .await
        .expect("count atomic publication lease");
    assert_eq!(committed_leases[0].count, 1);
    let replayed_commit = second_index
        .commit(commit_request.clone())
        .await
        .expect("replay exact VCE commit idempotently");
    assert_eq!(replayed_commit, first_commit);
    second_index
        .abort_refresh_staging("refresh-one")
        .await
        .expect("owned abort never rolls back a committed checkpoint");
    assert_eq!(
        second_index
            .current_checkpoint_row()
            .await
            .expect("committed checkpoint survives")
            .expect("checkpoint exists")
            .checkpoint_version,
        1
    );
    // fabricatedSource is never staged and therefore has no authoritative storage row.
    // fabricatedSource 从未暂存，因此不存在权威存储行。
    let fabricated_source = "pub fn fabricated() {}";
    // fabricatedBlob represents a canonical identity that has no staged or committed row.
    // fabricatedBlob 表示一个既无暂存行也无已提交行的规范身份。
    let fabricated_blob = vulcan_context_engine::blob_name("src/fabricated.rs", fabricated_source)
        .expect("build fabricated Blob identity");
    second_index
        .stage(IndexStageBatch {
            operation_id: String::from("stage-invalid-membership"),
            refresh_id: String::from("refresh-invalid-membership"),
            generation: 2,
            blobs: Vec::new(),
        })
        .await
        .expect("stage empty invalid-membership generation");
    // invalidCheckpoint attempts to add fabricatedBlob without staging it.
    // invalidCheckpoint 尝试在未暂存 fabricatedBlob 的情况下将其加入成员集合。
    let invalid_checkpoint = Checkpoint::new(
        second_index.binding().checkpoint_chain_id.clone(),
        2,
        [blob_name.clone(), fabricated_blob.clone()],
    );
    // invalidState remains internally coherent so the storage transition check is decisive.
    // invalidState 保持内部一致，使存储转换校验成为决定性拒绝条件。
    let mut invalid_state = state_snapshot.clone();
    invalid_state.workspace.generation = 2;
    invalid_state.workspace.checkpoint = Some(invalid_checkpoint.clone());
    invalid_state.workspace.records.push(FileRecord {
        path: String::from("src/fabricated.rs"),
        blob_name: Some(fabricated_blob.clone()),
        committed_blob_name: Some(fabricated_blob),
        status: FileStatus::Present,
        size: fabricated_source.len() as u64,
        mtime_ns: None,
        source: FileSource::Filesystem,
        content: None,
        generation: 2,
        skip_reason: None,
    });
    invalid_state.last_refresh = Some(VceRefreshSummary {
        request_id: String::from("refresh-invalid-membership"),
        mode: RefreshMode::Paths,
        reason: RefreshReason::Manual,
        generation: 2,
        added_blob_count: 0,
        deleted_blob_count: 0,
        indexed_blob_count: 0,
        skipped_path_count: 0,
        checkpoint_version: Some(2),
        source_revision: Some(String::from("source-invalid-membership")),
    });
    second_index
        .commit(IndexCommit {
            request_id: String::from("commit-invalid-membership"),
            generation: 2,
            refresh_id: String::from("refresh-invalid-membership"),
            source_revision: Some(String::from("source-invalid-membership")),
            deleted_blobs: BTreeSet::new(),
            staged_blob_names: BTreeSet::new(),
            expected_checkpoint_version: Some(1),
            checkpoint: invalid_checkpoint,
            state_snapshot: invalid_state,
            finalize_embedding_manifest: false,
        })
        .await
        .expect_err("fabricated checkpoint membership must be rejected");
    second_index
        .abort_stage(IndexAbortRequest {
            operation_id: String::from("abort-invalid-membership"),
            refresh_id: String::from("refresh-invalid-membership"),
            generation: 2,
        })
        .await
        .expect("abort rejected membership generation");
    second_index
        .stage(stage_request)
        .await
        .expect_err("committed VCE generation must reject staging replay");
    second_index
        .abort_stage(IndexAbortRequest {
            operation_id: String::from("abort-committed"),
            refresh_id: String::from("refresh-one"),
            generation: 1,
        })
        .await
        .expect_err("committed VCE generation must reject abort");
    let persisted_state = data_plane
        .load_state_snapshot(&second_index.binding().namespace_id)
        .await
        .expect("load atomically committed VCE state")
        .expect("committed VCE state should exist");
    assert_eq!(persisted_state, state_snapshot);
    // original is read from persisted origin rather than fabricated from the current model.
    // original 来自持久化原始来源，而不是根据当前模型编造。
    let original = data_plane
        .load_active_project_config("project-1")
        .await
        .expect("read original contract")
        .expect("existing project");
    // current_settings changes only the host settings revision; read-only restore must not save it.
    // current_settings 仅改变宿主设置修订；只读恢复不得保存它。
    let mut current_settings = original.clone();
    current_settings.config_revision += 1;
    // restored is a read-only restoration of the existing binding, without cleanup or replacement.
    // restored 是现有绑定的只读恢复，不执行清理或替换。
    let restored = data_plane
        .open_existing_project_index(&current_settings)
        .await
        .expect("restore read-only binding")
        .expect("existing index");
    assert_eq!(restored.config_revision, original.config_revision);
    assert_eq!(
        data_plane
            .load_active_project_config("project-1")
            .await
            .expect("reread original"),
        Some(original.clone())
    );
    // incompatible must fail without deleting the previous state or changing active provenance.
    // incompatible 必须失败，且不能删除之前状态或改变活动来源。
    let mut incompatible = original.clone();
    incompatible.dimension += 1;
    assert!(
        data_plane
            .open_existing_project_index(&incompatible)
            .await
            .is_err()
    );
    assert_eq!(
        data_plane
            .load_state_snapshot(&original.binding.namespace_id)
            .await
            .expect("state after rejected restore"),
        Some(state_snapshot.clone())
    );
    assert_eq!(
        data_plane
            .load_active_project_config("project-1")
            .await
            .expect("origin after rejected restore"),
        Some(original)
    );
    // Pin the same exact checkpoint for vector selection and subsequent body materialization.
    // 为向量选择及后续正文物化固定同一个精确检查点。
    let read_index = second_index
        .pin_checkpoint(
            state_snapshot
                .workspace
                .checkpoint
                .clone()
                .expect("committed checkpoint"),
            "binding-fixture-run",
        )
        .await
        .expect("pin committed storage");
    let search = read_index
        .search_dense(DenseSearchRequest {
            request_id: String::from("search-one"),
            query_index: 0,
            query_vector: EmbeddingVector {
                input_id: String::from("query-one"),
                values: vec![1.0, 0.0, 0.0],
            },
            allowed_blobs: BTreeSet::from([blob_name.clone()]),
            limit: 4,
        })
        .await
        .expect("search committed VCE vector");
    assert_eq!(search.hits.len(), 1);
    assert_eq!(
        committed_read_index
            .search_dense(DenseSearchRequest {
                request_id: String::from("search-atomic-publication"),
                query_index: 0,
                query_vector: EmbeddingVector {
                    input_id: String::from("query-atomic"),
                    values: vec![1.0, 0.0, 0.0]
                },
                allowed_blobs: BTreeSet::from([blob_name.clone()]),
                limit: 4,
            })
            .await
            .expect("query lease returned directly by commit")
            .hits
            .len(),
        1
    );
    println!(
        "VCE_STORAGE sqlite_bytes={} lancedb_bytes={} total_bytes={}",
        file_or_directory_size(&data_plane.sqlite_path),
        file_or_directory_size(&data_plane.lancedb_path),
        file_or_directory_size(&data_plane.data_root),
    );
    let mut aborted_blob = indexed_blob.clone();
    aborted_blob.generation = 2;
    second_index
        .stage(IndexStageBatch {
            operation_id: String::from("stage-two"),
            refresh_id: String::from("refresh-two"),
            generation: 2,
            blobs: vec![aborted_blob],
        })
        .await
        .expect("stage abortable VCE Blob");
    second_index
        .abort_stage(IndexAbortRequest {
            operation_id: String::from("abort-two"),
            refresh_id: String::from("refresh-two"),
            generation: 2,
        })
        .await
        .expect("abort VCE staging generation");
    #[derive(serde::Deserialize)]
    struct BindingCountRow {
        /// Number of matching rows.
        /// 匹配行数量。
        count: i64,
    }
    let active: Vec<BindingCountRow> = data_plane
            .query_rows(
                "SELECT COUNT(*) AS count FROM vce_project_bindings WHERE project_id = ? AND active = 1;",
                vec![super::sqlite_text("project-1")],
            )
            .await
            .expect("count active VCE bindings");
    let orphaned: Vec<BindingCountRow> = data_plane
            .query_rows(
                "SELECT COUNT(*) AS count FROM vce_project_bindings WHERE project_id = ? AND lifecycle_state = 'orphaned';",
                vec![super::sqlite_text("project-1")],
            )
            .await
            .expect("count orphaned VCE bindings");
    let pending_cleanup: Vec<BindingCountRow> = data_plane
            .query_rows(
                "SELECT COUNT(*) AS count FROM vce_vector_cleanup WHERE namespace_id = ? AND state = 'pending';",
                vec![super::sqlite_text(&second_index.binding().namespace_id)],
            )
            .await
            .expect("count pending VCE vector cleanup rows");
    assert_eq!(active[0].count, 1);
    assert_eq!(orphaned[0].count, 1);
    assert_eq!(pending_cleanup[0].count, 0);
    // Exercise unchanged, replaced and removed producers through the atomic publication API.
    // 通过原子发布接口验证未变化、已替换及已移除的生产来源。
    let mut approved_space = second_index.binding.embedding_space.clone();
    approved_space.physical_fingerprint = Some(String::from("fixture-approved-producer-b"));
    // approved_writer changes actual production only under the explicit compatibility grant.
    // approved_writer 仅在明确兼容授权下改变实际生产来源。
    let approved_writer = second_index
        .clone()
        .with_embedding_execution(
            vulcan_context_engine::VceEmbeddingExecutionContract::new(
                second_index.binding.clone(),
                approved_space,
                "fixture-approved-space-switch",
            )
            .expect("construct explicit fixture grant"),
        )
        .expect("bind approved writer");
    // Keep every publication alive to test source provenance independently of later commits.
    // 保持各次发布存活，以验证来源信息独立于后续提交。
    let mut retained_publications = Vec::new();
    for generation in 2..=4 {
        // Only the middle update replaces vectors; the last update removes the member entirely.
        // 仅中间一次更新替换向量；最后一次更新彻底移除成员。
        let replaced = generation == 3;
        // removed selects the initialized empty checkpoint at the final publication.
        // removed 在最终发布时选择已初始化的空检查点。
        let removed = generation == 4;
        // refresh_id distinguishes this exact attempt from previous staging batches.
        // refresh_id 将本次精确尝试与先前暂存批次区分开。
        let refresh_id = format!("atomic-publication-{generation}");
        // staged_blob preserves content identity while assigning the new producer attempt.
        // staged_blob 保持内容身份，并分配新的生产尝试。
        let mut staged_blob = indexed_blob.clone();
        staged_blob.generation = generation;
        approved_writer
            .stage(IndexStageBatch {
                operation_id: format!("stage-{refresh_id}"),
                refresh_id: refresh_id.clone(),
                generation,
                blobs: if replaced {
                    vec![staged_blob]
                } else {
                    Vec::new()
                },
            })
            .await
            .expect("stage exact publication attempt");
        // members is the exact surviving file set at this publication.
        // members 是本次发布精确存活的文件集合。
        let members = if removed {
            BTreeSet::new()
        } else {
            BTreeSet::from([blob_name.clone()])
        };
        // checkpoint remains in the original physical chain under the approved producer.
        // checkpoint 在已批准生产者下仍保留在原始物理链中。
        let checkpoint = Checkpoint::new(
            second_index.binding.checkpoint_chain_id.clone(),
            generation,
            members,
        );
        // next_snapshot describes the same full-refresh result as the exact commit below.
        // next_snapshot 描述与下方精确提交相同的全量刷新结果。
        let mut next_snapshot = state_snapshot.clone();
        next_snapshot.workspace.generation = generation;
        next_snapshot.workspace.checkpoint = Some(checkpoint.clone());
        if removed {
            next_snapshot.workspace.records.clear();
        }
        next_snapshot.statistics.refresh_attempts = generation;
        next_snapshot.statistics.refresh_successes = generation;
        next_snapshot.last_refresh = Some(VceRefreshSummary {
            request_id: refresh_id.clone(),
            mode: RefreshMode::Full,
            reason: RefreshReason::Manual,
            generation,
            added_blob_count: 0,
            deleted_blob_count: usize::from(removed),
            indexed_blob_count: usize::from(replaced),
            skipped_path_count: 0,
            checkpoint_version: Some(generation),
            source_revision: None,
        });
        if generation == 2 {
            // guarded_commit is valid before a binding changes between preflight and publication.
            // guarded_commit 在预检与发布之间发生绑定变化前是有效提交。
            let guarded_commit = IndexCommit {
                request_id: format!("commit-{refresh_id}"),
                refresh_id: refresh_id.clone(),
                generation,
                source_revision: None,
                deleted_blobs: BTreeSet::new(),
                staged_blob_names: BTreeSet::new(),
                expected_checkpoint_version: Some(1),
                checkpoint: checkpoint.clone(),
                state_snapshot: next_snapshot.clone(),
                finalize_embedding_manifest: false,
            };
            // guarded_lease freezes the valid predecessor before simulating an external binding change.
            // guarded_lease 在模拟外部绑定变化前固定有效前驱。
            let guarded_lease = approved_writer
                .prepare_publication_lease(&guarded_commit, "binding-fixture-run")
                .await
                .expect("prepare guarded publication");
            // guarded_script uses the real fingerprint so rejection cannot come from a malformed receipt.
            // guarded_script 使用真实指纹，使拒绝不可能来自格式错误的回执。
            let guarded_script = approved_writer
                .build_commit_script(
                    &guarded_commit,
                    &super::commit_request_fingerprint(&guarded_commit)
                        .expect("fingerprint guarded commit"),
                    &[],
                    Some(&guarded_lease),
                )
                .expect("build guarded publication");
            // Each exact contract field must be fenced independently under the transaction's write lock.
            // 每个精确契约字段均须在事务写锁内独立受到保护。
            for (change, restore) in [
                (String::from("active = 0"), String::from("active = 1")),
                (
                    String::from("config_revision = config_revision + 1"),
                    String::from("config_revision = config_revision - 1"),
                ),
                (
                    String::from("physical_fingerprint = 'changed-fixture'"),
                    format!(
                        "physical_fingerprint = '{}'",
                        super::sqlite_string_literal(&approved_writer.physical_fingerprint)
                    ),
                ),
                (
                    String::from("dimension = dimension + 1"),
                    String::from("dimension = dimension - 1"),
                ),
                (
                    String::from("content_table = 'changed_content_fixture'"),
                    format!(
                        "content_table = '{}'",
                        super::sqlite_string_literal(&approved_writer.content_table)
                    ),
                ),
                (
                    String::from("path_table = 'changed_path_fixture'"),
                    format!(
                        "path_table = '{}'",
                        super::sqlite_string_literal(&approved_writer.path_table)
                    ),
                ),
            ] {
                approved_writer
                    .data_plane
                    .execute(
                        &format!(
                            "UPDATE vce_project_bindings SET {change} WHERE namespace_id = ?;"
                        ),
                        vec![super::sqlite_text(&approved_writer.binding.namespace_id)],
                        "change fixture binding after admission",
                    )
                    .await
                    .expect("change fixture binding");
                assert!(
                    approved_writer
                        .data_plane
                        .execute(&guarded_script, Vec::new(), "reject stale fixture writer")
                        .await
                        .is_err(),
                    "publication must reject changed binding: {change}"
                );
                // effects proves the rejected transaction persisted neither checkpoint, receipt nor lease.
                // effects 证明被拒绝事务既未持久化检查点，也未持久化回执或租约。
                let effects: Vec<BindingCountRow> = approved_writer.data_plane.query_rows(
                        "SELECT (SELECT COUNT(*) FROM vce_read_checkpoints WHERE namespace_id = ? AND checkpoint_version = 2) + (SELECT COUNT(*) FROM vce_commit_receipts WHERE namespace_id = ? AND operation_id = ?) + (SELECT COUNT(*) FROM vce_read_leases WHERE namespace_id = ? AND checkpoint_version = 2) AS count;",
                        vec![super::sqlite_text(&approved_writer.binding.namespace_id), super::sqlite_text(&approved_writer.binding.namespace_id), super::sqlite_text(&guarded_commit.request_id), super::sqlite_text(&approved_writer.binding.namespace_id)],
                    ).await.expect("inspect rejected publication effects");
                assert_eq!(
                    effects[0].count, 0,
                    "stale publication must roll back completely"
                );
                assert_eq!(
                    approved_writer
                        .current_checkpoint_row()
                        .await
                        .expect("read retained predecessor")
                        .expect("predecessor remains")
                        .checkpoint_version,
                    1
                );
                approved_writer
                    .data_plane
                    .execute(
                        &format!(
                            "UPDATE vce_project_bindings SET {restore} WHERE namespace_id = ?;"
                        ),
                        vec![super::sqlite_text(&approved_writer.binding.namespace_id)],
                        "restore exact fixture binding",
                    )
                    .await
                    .expect("restore fixture binding");
            }
            drop(guarded_lease);
            // An explicitly aborted empty attempt must not become publishable just because no blobs remain.
            // 明确中止的空尝试不能仅因没有剩余块对象就重新变为可发布。
            // stage_first separates empty staged cancellation from cancellation before any staging call.
            // stage_first 区分空暂存后的取消与任何暂存调用之前的取消。
            for (cancelled_refresh, stage_first) in [
                ("cancelled-empty-publication", true),
                ("cancelled-before-staging", false),
            ] {
                if stage_first {
                    approved_writer
                        .stage(IndexStageBatch {
                            operation_id: format!("stage-{cancelled_refresh}"),
                            refresh_id: cancelled_refresh.to_owned(),
                            generation,
                            blobs: Vec::new(),
                        })
                        .await
                        .expect("stage cancelled empty attempt");
                }
                approved_writer
                    .abort_stage(IndexAbortRequest {
                        operation_id: format!("abort-{cancelled_refresh}"),
                        refresh_id: cancelled_refresh.to_owned(),
                        generation,
                    })
                    .await
                    .expect("cancel empty attempt");
                // cancelled_snapshot is otherwise a valid successor so the attempt-state guard is decisive.
                // cancelled_snapshot 在其他方面均是有效后继，使尝试状态保护成为决定性条件。
                let mut cancelled_snapshot = next_snapshot.clone();
                cancelled_snapshot
                    .last_refresh
                    .as_mut()
                    .expect("fixture refresh summary")
                    .request_id = cancelled_refresh.to_owned();
                // cancelled_commit retains valid checkpoint data but names the terminal aborted attempt.
                // cancelled_commit 保留有效检查点数据，但指向已终止的中止尝试。
                let cancelled_commit = IndexCommit {
                    request_id: format!("commit-{cancelled_refresh}"),
                    refresh_id: cancelled_refresh.to_owned(),
                    generation,
                    source_revision: None,
                    deleted_blobs: BTreeSet::new(),
                    staged_blob_names: BTreeSet::new(),
                    expected_checkpoint_version: Some(1),
                    checkpoint: checkpoint.clone(),
                    state_snapshot: cancelled_snapshot,
                    finalize_embedding_manifest: false,
                };
                // cancelled_script bypasses preflight to verify the transaction's independent abort fence.
                // cancelled_script 绕过预检，验证事务自身独立的中止保护。
                let cancelled_script = approved_writer
                    .build_commit_script(
                        &cancelled_commit,
                        &super::commit_request_fingerprint(&cancelled_commit)
                            .expect("fingerprint cancelled fixture"),
                        &[],
                        None,
                    )
                    .expect("build cancelled transaction fixture");
                assert!(
                    approved_writer
                        .data_plane
                        .execute(
                            &cancelled_script,
                            Vec::new(),
                            "cancelled transaction fixture"
                        )
                        .await
                        .is_err(),
                    "transaction must reject an aborted empty attempt"
                );
                assert!(
                    approved_writer
                        .commit_pinned(cancelled_commit, "binding-fixture-run")
                        .await
                        .is_err(),
                    "aborted empty attempt must never commit"
                );
                assert_eq!(
                    approved_writer
                        .generation_state(cancelled_refresh, generation, VcePortOperation::Commit)
                        .await
                        .expect("read cancelled generation")
                        .as_deref(),
                    Some("aborted")
                );
                assert_eq!(
                    approved_writer
                        .current_checkpoint_row()
                        .await
                        .expect("read unchanged checkpoint")
                        .expect("original checkpoint")
                        .checkpoint_version,
                    1
                );
            }
        }
        // published_read must be usable immediately without another lease admission.
        // published_read 必须无需再次准入租约即可立即使用。
        let (_, published_read) = approved_writer
            .commit_pinned(
                IndexCommit {
                    request_id: format!("commit-{refresh_id}"),
                    refresh_id: refresh_id.clone(),
                    generation,
                    source_revision: None,
                    deleted_blobs: if removed {
                        BTreeSet::from([blob_name.clone()])
                    } else {
                        BTreeSet::new()
                    },
                    staged_blob_names: if replaced {
                        BTreeSet::from([blob_name.clone()])
                    } else {
                        BTreeSet::new()
                    },
                    expected_checkpoint_version: Some(generation - 1),
                    checkpoint,
                    state_snapshot: next_snapshot,
                    finalize_embedding_manifest: false,
                },
                "binding-fixture-run",
            )
            .await
            .expect("commit and retain exact producers");
        // Even unstaged publications retain a terminal generation identity against later reuse.
        // 即使未经暂存的发布也保留终态代次身份，阻止后续复用。
        assert_eq!(
            approved_writer
                .generation_state(&refresh_id, generation, VcePortOperation::Commit)
                .await
                .expect("read committed generation")
                .as_deref(),
            Some("committed")
        );
        // provenance is frozen in memory and must exactly match the durable checkpoint.
        // provenance 在内存中固定，必须与持久检查点精确一致。
        let provenance = published_read
            .embedding_provenance()
            .await
            .expect("frozen producer sources");
        assert_eq!(
            provenance,
            approved_writer
                .embedding_provenance()
                .await
                .expect("durable producer sources")
        );
        assert_eq!(
            provenance
                .source_fingerprints
                .contains("fixture-approved-producer-b"),
            replaced
        );
        assert_eq!(provenance.checkpoint_version, Some(generation));
        retained_publications.push(published_read);
    }
    assert_eq!(
        committed_read_index
            .embedding_provenance()
            .await
            .expect("original frozen sources")
            .checkpoint_version,
        Some(1)
    );
    assert!(
        retained_publications[1]
            .embedding_provenance()
            .await
            .expect("retained B sources")
            .source_fingerprints
            .contains("fixture-approved-producer-b")
    );
    assert_eq!(
        retained_publications[1]
            .search_dense(DenseSearchRequest {
                request_id: String::from("search-retained-atomic-rebuild"),
                query_index: 0,
                query_vector: EmbeddingVector {
                    input_id: String::from("query-retained"),
                    values: vec![1.0, 0.0, 0.0]
                },
                allowed_blobs: BTreeSet::from([blob_name.clone()]),
                limit: 4,
            })
            .await
            .expect("query replaced producer after empty publication")
            .hits
            .len(),
        1
    );
    // candidate_fixture retains exact inputs for a separately polled stage on this same runtime.
    // candidate_fixture 在同一运行时上为单独轮询的阶段保留精确输入。
    let candidate_fixture = (
        current_factory.clone(),
        data_plane.clone(),
        approved_writer.clone(),
        retained_publications.clone(),
        indexed_blob.clone(),
        commit_request.clone(),
        blob_name.clone(),
    );
    tokio::spawn(async move {
        // Separate polling bounds nested debug-stack use; this awaited stage adds no threads or concurrent indexing.
        // 单独轮询限制嵌套调试栈使用；此被等待的阶段不增加线程或并发索引。
        let (factory, data, writer, reads, blob, commit, name) = candidate_fixture;
        verify_candidate_publication(&factory, &data, &writer, &reads, &blob, &commit, &name).await;
    })
    .await
    .expect("candidate publication stage completed");
    drop(retained_publications);
    drop(approved_writer);
    // Drain the published read view before the production factory closes its physical binding.
    // 在生产工厂关闭物理绑定之前排空已发布读取视图。
    drop(read_index);
    drop(committed_read_index);
    drop(restored);
    drop(first_index);
    drop(second_index);
    drop(data_plane);
    current_factory
        .shutdown()
        .await
        .expect("close current-format fixture");
    assert_eq!(VCE_SPACE_ID, "vce");
}

/// Verify a rejected initial table allocation preserves foreign tables and permits explicit retry.
/// 验证被拒绝的首次表分配保留外部表，并允许明确重试。
/// `host_runtime` owns temporary fixture storage; `root` is the existing temporary source root.
/// `host_runtime` 持有临时夹具存储；`root` 是已存在的临时源码根目录。
/// Returns boxed assertions, closing every additional binding before reporting a retry failure.
/// 返回装箱断言，在报告重试失败前关闭全部额外绑定。
fn verify_initial_allocation_retry<'a>(
    host_runtime: &'a HostRuntimeHandle,
    root: &'a std::path::Path,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + 'a>> {
    Box::pin(async move {
        // Each collision owns a separate stable project so no attempt can borrow another table.
        // 每种碰撞持有独立稳定项目，使尝试不能借用其他项目的表。
        for channel in ["content", "path"] {
            // Config is the same deterministic identity used by both explicit creation attempts.
            // config 是两次明确创建尝试共同使用的确定性身份。
            let config = test_project_config(&format!("initial-collision-{channel}"), root, 1);
            // The catalog allocates directories only; it does not publish an initialized index.
            // catalog 仅分配目录，不发布已初始化索引。
            let catalog = crate::vce_storage_catalog::VceStorageCatalog::bootstrap(host_runtime)
                .await
                .expect("open initial collision catalog");
            // Paths come from the real catalog rather than a guessed project directory.
            // paths 来自真实目录服务，而不是猜测的项目目录。
            let paths = catalog
                .allocate_project(&config.project_id)
                .await
                .expect("allocate isolated initial collision paths");
            // The fixture's binding installs only schema and its own deliberate collision.
            // 夹具绑定仅安装结构及其自有的有意碰撞。
            let store = super::prepare_vce_storage_binding(
                host_runtime,
                paths.root,
                config.project_id.clone(),
                true,
            )
            .expect("prepare initial collision binding");
            store
                .activate_bindings(true)
                .await
                .expect("activate isolated initial collision schema");
            catalog
                .close()
                .await
                .expect("close collision catalog binding");
            // The colliding table belongs to this fixture, never to the rejected allocator.
            // 冲突表属于本夹具，绝不属于被拒绝的分配器。
            let foreign_table = vector_table_name(
                channel,
                &config.vector_space_id,
                &config.physical_fingerprint,
                &config.preprocessing_version,
                &config.chunker_version,
                config.dimension,
            );
            store
                .ensure_vector_table(&foreign_table, config.dimension)
                .await
                .expect("create fixture-owned initial collision table");
            // Factory is the public production creation boundary with the real project writer lock.
            // factory 是使用真实项目写入锁的公开生产创建边界。
            let factory = bootstrap_vce_data_plane(host_runtime)
                .await
                .expect("open initial collision factory");
            assert!(factory.create_project_index(config.clone()).await.is_err());
            assert!(
                store
                    .load_active_project_config(&config.project_id)
                    .await
                    .expect("inspect rejected initial allocation")
                    .is_none(),
                "failed allocation must not publish an initial binding"
            );
            assert!(
                store
                    .ensure_vector_table(&foreign_table, config.dimension)
                    .await
                    .is_err(),
                "failed allocation must preserve the fixture-owned collision"
            );
            // Only the fixture removes its exact collision after proving the allocator preserved it.
            // 仅夹具在证明分配器保留冲突表后移除其精确自有冲突。
            store
                .vector_engine()
                .await
                .expect("open fixture vector engine")
                .drop_table(LanceDbDropTableInput {
                    table_name: foreign_table.to_string(),
                })
                .await
                .expect("remove only the fixture-owned collision");
            // Capture failure before closing bindings, so a red regression leaves no live fixture owner.
            // 在关闭绑定前捕获失败，使红色回归不遗留存活夹具所有者。
            let retry_error = factory.create_project_index(config).await.err();
            factory
                .shutdown()
                .await
                .expect("close initial retry factory");
            store
                .close_bindings()
                .await
                .expect("close initial collision binding");
            assert!(
                retry_error.is_none(),
                "explicit initial retry after removing {channel} collision must succeed: {retry_error:?}"
            );
        }
    })
}

/// Verify isolated candidate publication while preserving the predecessor's pinned queries.
/// 验证隔离候选发布，同时保留前驱固定查询。
/// Inputs identify real storage, its writer, retained reads and the original blob/commit. Returns boxed assertions.
/// 参数标识真实存储、写入方、保留读取及原始代码块／提交。返回装箱断言任务。
fn verify_candidate_publication<'a>(
    current_factory: &'a super::VceDataPlaneFactory,
    data_plane: &'a super::VceDataPlaneHandle,
    approved_writer: &'a super::VceProjectIndex,
    retained_publications: &'a [super::VceProjectIndex],
    indexed_blob: &'a IndexedBlob,
    commit_request: &'a IndexCommit,
    blob_name: &'a str,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + 'a>> {
    Box::pin(async move {
        // original is the published identity that candidate allocation must leave unchanged.
        // original 是候选分配必须保持不变的已发布身份。
        let original = data_plane
            .load_active_project_config(&approved_writer.project_id)
            .await
            .expect("load rebuild predecessor")
            .expect("published predecessor exists");
        // candidate_config intentionally selects distinct physical tables and another explicit dimension.
        // candidate_config 明确选择独立物理表及另一明确维度。
        let mut candidate_config = original.clone();
        candidate_config.binding.namespace_id.push_str(":candidate");
        candidate_config
            .binding
            .checkpoint_chain_id
            .push_str(":candidate");
        candidate_config.vector_space_id.push_str(":candidate");
        candidate_config.binding.embedding_space.vector_space_id =
            candidate_config.vector_space_id.clone();
        candidate_config.dimension = 4;
        candidate_config.binding.embedding_space.dimension = 4;
        candidate_config.physical_fingerprint.push_str(":candidate");
        candidate_config
            .binding
            .embedding_space
            .physical_fingerprint = Some(candidate_config.physical_fingerprint.clone());
        // candidate_base captures the exact last publication rather than guessing from settings revision.
        // candidate_base 捕获精确最近发布，而不是从设置修订猜测。
        let candidate_base = super::VceRebuildCandidateBase {
            namespace_id: original.binding.namespace_id.clone(),
            checkpoint_version: 4,
            job_id: String::from("candidate-rebuild-job"),
        };
        // Isolate this partial-allocation phase on an awaited task without increasing the default thread stack.
        // 将部分分配阶段隔离到被等待的任务，不增加默认线程栈。
        tokio::spawn(
            super::candidate_allocation::verify_partial_candidate_allocation(
                current_factory.clone(),
                data_plane.clone(),
                approved_writer.clone(),
                candidate_config.clone(),
                candidate_base.clone(),
            ),
        )
        .await
        .expect("partial candidate allocation fixture completed");
        assert!(
            current_factory
                .prepare_rebuild_candidate(candidate_config.clone(), candidate_base.clone())
                .await
                .expect_err("candidate must not allocate before durable admission")
                .contains("durable replacement job")
        );
        // candidate_jobs freezes the physical target before allocation and keeps it immutable during progress updates.
        // candidate_jobs 在分配前固定物理目标，并在进度更新期间保持其不可变。
        let candidate_jobs = approved_writer.job_store();
        let queued_candidate =
            super::jobs::replacement_test_receipt(candidate_config.clone(), candidate_base.clone());
        candidate_jobs
            .register(&queued_candidate)
            .await
            .expect("persist exact replacement before allocation");
        assert_eq!(
            candidate_jobs
                .get(&candidate_base.job_id)
                .await
                .expect("read durable replacement"),
            Some(queued_candidate.clone())
        );
        // cancelled_before_allocation owns a separate receipt and must never create the same pending target.
        // cancelled_before_allocation 持有独立回执，绝不能创建相同待分配目标。
        let mut cancelled_before_allocation = queued_candidate.clone();
        cancelled_before_allocation
            .job_id
            .push_str(":cancel-before-allocation");
        cancelled_before_allocation.idempotency_key = cancelled_before_allocation.job_id.clone();
        cancelled_before_allocation
            .replacement
            .as_mut()
            .expect("replacement target")
            .predecessor
            .job_id = cancelled_before_allocation.job_id.clone();
        candidate_jobs
            .register(&cancelled_before_allocation)
            .await
            .expect("register cancelled allocation fixture");
        let mut allocation_cancelled = cancelled_before_allocation.clone();
        allocation_cancelled.sequence += 1;
        allocation_cancelled.state = super::VceJobState::Cancelled;
        candidate_jobs
            .update(&cancelled_before_allocation, &allocation_cancelled)
            .await
            .expect("cancel before allocating");
        assert!(
            current_factory
                .prepare_rebuild_candidate(
                    candidate_config.clone(),
                    allocation_cancelled
                        .replacement
                        .expect("cancelled target")
                        .predecessor,
                )
                .await
                .expect_err("cancelled receipt cannot allocate")
                .contains("admitted replacement receipt")
        );
        let unallocated: Vec<super::CountRow> = data_plane
            .query_rows(
                "SELECT COUNT(*) AS count FROM vce_project_bindings WHERE namespace_id = ?;",
                vec![super::sqlite_text(&candidate_config.binding.namespace_id)],
            )
            .await
            .expect("inspect absent cancelled allocation");
        assert_eq!(unallocated[0].count, 0);
        // changed_target remains structurally valid but cannot retarget an admitted job.
        // changed_target 结构仍有效，但不能改写已准入作业目标。
        let mut changed_target = queued_candidate.clone();
        changed_target.sequence += 1;
        changed_target.state = super::VceJobState::Running;
        changed_target
            .replacement
            .as_mut()
            .expect("replacement exists")
            .target
            .binding
            .namespace_id
            .push_str(":changed");
        changed_target
            .validate()
            .expect("changed identity is structurally valid");
        assert!(
            candidate_jobs
                .update(&queued_candidate, &changed_target)
                .await
                .is_err()
        );
        let wrong_target = changed_target
            .replacement
            .expect("changed replacement")
            .target;
        assert!(
            current_factory
                .prepare_rebuild_candidate(wrong_target, candidate_base.clone())
                .await
                .is_err()
        );
        // stale_base must be rejected before any new physical table is created.
        // stale_base 必须在创建任何新物理表之前被拒绝。
        let mut stale_base = candidate_base.clone();
        stale_base.checkpoint_version = 3;
        assert!(
            current_factory
                .prepare_rebuild_candidate(candidate_config.clone(), stale_base)
                .await
                .is_err()
        );
        // candidate remains invisible to active-project discovery and cannot bypass the ordinary port gate.
        // candidate 对活动项目发现不可见，且不能绕过普通端口门禁。
        let candidate = current_factory
            .prepare_rebuild_candidate(candidate_config.clone(), candidate_base.clone())
            .await
            .expect("prepare inactive different-dimension candidate");
        assert_eq!(
            current_factory
                .load_active_project_config(&original.project_id)
                .await
                .expect("read published pointer after candidate creation"),
            Some(original.clone())
        );
        assert!(
            data_plane
                .open_project_index(candidate_config.clone())
                .await
                .is_err(),
            "ordinary opening must not activate a pending candidate"
        );
        assert!(
            candidate
                .find_missing(vulcan_context_engine::FindMissingRequest {
                    request_id: String::from("candidate-ordinary-read"),
                    blob_names: BTreeSet::new(),
                })
                .await
                .is_err(),
            "candidate cannot enter the ordinary active index pipeline"
        );
        // candidate_rows verifies durable exact predecessor/job ownership without changing the active pointer.
        // candidate_rows 验证持久精确前驱／作业归属，且不改变活动指针。
        let candidate_rows: Vec<super::CountRow> = data_plane.query_rows(
                "SELECT COUNT(*) AS count FROM vce_rebuild_candidates c JOIN vce_project_bindings b ON b.namespace_id = c.namespace_id WHERE c.namespace_id = ? AND c.base_namespace_id = ? AND c.base_checkpoint_version = 4 AND c.job_id = 'candidate-rebuild-job' AND c.state = 'building' AND b.active = 0;",
                vec![super::sqlite_text(&candidate_config.binding.namespace_id), super::sqlite_text(&original.binding.namespace_id)],
            ).await.expect("read candidate ownership");
        assert!(matches!(candidate_rows.as_slice(), [row] if row.count == 1));
        // foreign_base cannot borrow a candidate allocated to another durable host job.
        // foreign_base 不能借用分配给另一持久宿主作业的候选。
        let mut foreign_base = candidate_base.clone();
        foreign_base.job_id.push_str("-foreign");
        assert!(
            candidate
                .admit_rebuild_candidate_writer(foreign_base)
                .await
                .is_err()
        );
        // candidate_writer is explicitly admitted; the original inactive handle remains unauthorized.
        // candidate_writer 已显式准入；原始非活动句柄仍未获授权。
        let candidate_writer = candidate
            .admit_rebuild_candidate_writer(candidate_base)
            .await
            .expect("admit exact candidate writer");
        // candidate_blob reuses content while writing vectors only into the new four-dimensional tables.
        // candidate_blob 复用内容，但仅向新的四维表写入向量。
        let mut candidate_blob = indexed_blob.clone();
        candidate_blob.generation = 1;
        for vector in &mut candidate_blob.embedding_vectors {
            vector.values.push(0.0);
        }
        if let Some(vector) = &mut candidate_blob.path_embedding {
            vector.values.push(0.0);
        }
        candidate_writer
            .stage(IndexStageBatch {
                operation_id: String::from("stage-candidate"),
                refresh_id: String::from("candidate-refresh"),
                generation: 1,
                blobs: vec![candidate_blob],
            })
            .await
            .expect("stage four-dimensional candidate");
        // candidate_commit publishes the same content under the new physical binding and checkpoint chain.
        // candidate_commit 在新物理绑定及检查点链下发布相同内容。
        let mut candidate_commit = commit_request.clone();
        candidate_commit.request_id = String::from("commit-candidate");
        candidate_commit.refresh_id = String::from("candidate-refresh");
        candidate_commit.checkpoint.chain_id = candidate_config.binding.checkpoint_chain_id.clone();
        candidate_commit.state_snapshot.identity = VceStateIdentity::new(
            candidate_config.binding.namespace_id.clone(),
            candidate_config.binding.checkpoint_chain_id.clone(),
            candidate_config.binding.schema_version,
            candidate_config.binding.embedding_space.clone(),
        );
        candidate_commit.state_snapshot.workspace.checkpoint =
            Some(candidate_commit.checkpoint.clone());
        candidate_commit
            .state_snapshot
            .last_refresh
            .as_mut()
            .expect("candidate summary")
            .request_id = candidate_commit.refresh_id.clone();
        assert!(
            candidate_writer
                .commit(candidate_commit.clone())
                .await
                .is_err(),
            "candidate requires atomic host publication lease"
        );
        // candidate_lease and script are prepared before faults to exercise independent transactional fencing.
        // candidate_lease 及脚本在故障前准备，用于验证独立事务保护。
        let candidate_lease = candidate_writer
            .prepare_publication_lease(&candidate_commit, "candidate-run")
            .await
            .expect("prepare candidate origin and lease");
        let candidate_script = candidate_writer
            .build_commit_script(
                &candidate_commit,
                &super::commit_request_fingerprint(&candidate_commit)
                    .expect("candidate fingerprint"),
                &[],
                Some(&candidate_lease),
            )
            .expect("candidate publication script");
        // Advance the predecessor after admission, then restore it to test the cancellation ledger separately.
        // 在准入后改变前驱，再恢复，以单独测试取消台账。
        for (change, restore) in [
            (
                format!(
                    "UPDATE vce_active_checkpoints SET checkpoint_version = 3 WHERE namespace_id = '{}';",
                    super::sqlite_string_literal(&original.binding.namespace_id)
                ),
                format!(
                    "UPDATE vce_active_checkpoints SET checkpoint_version = 4 WHERE namespace_id = '{}';",
                    super::sqlite_string_literal(&original.binding.namespace_id)
                ),
            ),
            (
                format!(
                    "UPDATE vce_rebuild_candidates SET state = 'aborted' WHERE namespace_id = '{}';",
                    super::sqlite_string_literal(&candidate_config.binding.namespace_id)
                ),
                format!(
                    "UPDATE vce_rebuild_candidates SET state = 'building' WHERE namespace_id = '{}';",
                    super::sqlite_string_literal(&candidate_config.binding.namespace_id)
                ),
            ),
        ] {
            data_plane
                .execute(&change, Vec::new(), "inject candidate admission race")
                .await
                .expect("inject candidate race");
            assert!(
                data_plane
                    .execute(
                        &candidate_script,
                        Vec::new(),
                        "reject stale candidate transaction"
                    )
                    .await
                    .is_err()
            );
            assert!(
                candidate_writer
                    .current_checkpoint_row()
                    .await
                    .expect("candidate checkpoint after rejection")
                    .is_none()
            );
            data_plane
                .execute(&restore, Vec::new(), "restore exact candidate fixture")
                .await
                .expect("restore candidate fixture");
        }
        drop(candidate_lease);
        data_plane.execute(
                "CREATE TRIGGER vce_fixture_reject_candidate_publish BEFORE UPDATE OF active ON vce_project_bindings WHEN NEW.active = 1 BEGIN SELECT RAISE(ROLLBACK, 'fixture rejects candidate pointer'); END;",
                Vec::new(), "inject candidate switch failure",
            ).await.expect("install candidate switch fault");
        assert!(
            candidate_writer
                .commit_pinned(candidate_commit.clone(), "candidate-run")
                .await
                .is_err()
        );
        assert_eq!(
            data_plane
                .load_active_project_config(&original.project_id)
                .await
                .expect("original remains after failed switch"),
            Some(original.clone())
        );
        // rolled_back proves even the preceding lease and receipt insertions were rolled back by switch failure.
        // rolled_back 证明切换失败时连此前插入的租约及回执也一起回滚。
        let rolled_back: Vec<super::CountRow> = data_plane.query_rows(
                "SELECT (SELECT COUNT(*) FROM vce_read_checkpoints WHERE namespace_id = ?) + (SELECT COUNT(*) FROM vce_read_leases WHERE namespace_id = ?) + (SELECT COUNT(*) FROM vce_commit_receipts WHERE namespace_id = ?) AS count;",
                vec![super::sqlite_text(&candidate_config.binding.namespace_id); 3],
            ).await.expect("candidate rollback effects");
        assert_eq!(rolled_back[0].count, 0);
        assert_eq!(
            candidate_writer
                .staged_blob_names("candidate-refresh", 1)
                .await
                .expect("retained candidate staging"),
            BTreeSet::from([blob_name.to_owned()])
        );
        data_plane
            .execute(
                "DROP TRIGGER vce_fixture_reject_candidate_publish;",
                Vec::new(),
                "remove candidate switch fault",
            )
            .await
            .expect("remove candidate fault");
        // published_candidate becomes queryable only after the new pointer and its lease commit together.
        // published_candidate 仅在新指针与租约一同提交后才可查询。
        let (_, published_candidate) = candidate_writer
            .commit_pinned(candidate_commit.clone(), "candidate-run")
            .await
            .expect("publish candidate atomically");
        assert_eq!(
            data_plane
                .load_active_project_config(&original.project_id)
                .await
                .expect("new active candidate"),
            Some(candidate_config.clone())
        );
        assert_eq!(
            published_candidate
                .embedding_provenance()
                .await
                .expect("candidate frozen provenance")
                .original,
            candidate_config
        );
        assert_eq!(
            published_candidate
                .search_dense(DenseSearchRequest {
                    request_id: String::from("search-published-candidate"),
                    query_index: 0,
                    query_vector: EmbeddingVector {
                        input_id: String::from("new-query"),
                        values: vec![1.0, 0.0, 0.0, 0.0]
                    },
                    allowed_blobs: BTreeSet::from([blob_name.to_owned()]),
                    limit: 4,
                })
                .await
                .expect("new four-dimensional query")
                .hits
                .len(),
            1
        );
        assert!(
            approved_writer
                .find_missing(vulcan_context_engine::FindMissingRequest {
                    request_id: String::from("old-writer-after-switch"),
                    blob_names: BTreeSet::new(),
                })
                .await
                .is_err(),
            "old writer must lose active authority"
        );
        assert!(
            candidate_writer
                .stage(IndexStageBatch {
                    operation_id: String::from("reuse-published-candidate"),
                    refresh_id: String::from("reuse-candidate"),
                    generation: 2,
                    blobs: Vec::new(),
                })
                .await
                .is_err(),
            "candidate write grant ends on publication"
        );
        // replay proves an acknowledged candidate publication is not interpreted as another rebuild.
        // replay 证明已确认候选发布不会被解释为另一次重建。
        let (_, replay) = candidate_writer
            .commit_pinned(candidate_commit.clone(), "candidate-run")
            .await
            .expect("replay candidate publication");
        drop(replay);
        assert!(
            candidate_writer
                .cancel_rebuild_candidate(
                    candidate_writer
                        .rebuild_candidate
                        .as_ref()
                        .expect("published candidate authority")
                )
                .await
                .is_err(),
            "published candidate cannot be cancelled"
        );
        assert!(
            candidate_writer
                .cleanup_cancelled_rebuild_candidate(
                    candidate_writer
                        .rebuild_candidate
                        .as_ref()
                        .expect("published candidate identity")
                )
                .await
                .is_err(),
            "published candidate tables cannot be cleaned as cancelled work"
        );
        verify_candidate_cancellation(
            current_factory.clone(),
            data_plane.clone(),
            candidate_config.clone(),
            candidate_commit.clone(),
        )
        .await
        .expect("candidate cancellation verification task");
        drop(published_candidate);
        drop(candidate_writer);
        // A retained three-dimensional read must still work after publishing the four-dimensional replacement.
        // 发布四维替换库后，已保留的三维读取必须仍然可用。
        assert_eq!(
            retained_publications[1]
                .search_dense(DenseSearchRequest {
                    request_id: String::from("old-query-after-candidate-publication"),
                    query_index: 0,
                    query_vector: EmbeddingVector {
                        input_id: String::from("old-query"),
                        values: vec![1.0, 0.0, 0.0]
                    },
                    allowed_blobs: BTreeSet::from([blob_name.to_owned()]),
                    limit: 4,
                })
                .await
                .expect("old retained vectors survive candidate publication")
                .hits
                .len(),
            1
        );
        drop(candidate);
    })
}

/// Verify exact candidate reopening, durable cancellation and post-drain cleanup on real storage.
/// 在真实存储上验证精确候选重开、持久取消及排空后清理。
/// Owned fixture handles/config/commit retain the same store. Returns a task that the caller immediately joins.
/// 已拥有的夹具句柄／配置／提交保留同一存储。返回由调用方立即等待完成的任务。
fn verify_candidate_cancellation(
    current_factory: super::VceDataPlaneFactory,
    data_plane: super::VceDataPlaneHandle,
    candidate_config: super::VceProjectIndexConfig,
    candidate_commit: IndexCommit,
) -> tokio::task::JoinHandle<()> {
    // A separate task unwinds predecessor poll frames without adding a thread or concurrent indexing work.
    // 独立任务使前驱轮询帧退出，不增加线程，也不并发执行索引工作。
    tokio::spawn(async move {
        // cancelled_config allocates a separate attempt so a durable cancellation is never revived in the fixture.
        // cancelled_config 分配独立尝试，使夹具绝不复活持久取消状态。
        let mut cancelled_config = candidate_config.clone();
        cancelled_config.binding.namespace_id.push_str(":cancelled");
        cancelled_config.vector_space_id.push_str(":cancelled");
        cancelled_config.binding.embedding_space.vector_space_id =
            cancelled_config.vector_space_id.clone();
        cancelled_config
            .binding
            .checkpoint_chain_id
            .push_str(":cancelled");
        cancelled_config.physical_fingerprint.push_str(":cancelled");
        cancelled_config
            .binding
            .embedding_space
            .physical_fingerprint = Some(cancelled_config.physical_fingerprint.clone());
        // cancelled_base belongs to the newly published four-dimensional predecessor.
        // cancelled_base 属于刚发布的四维前驱。
        let cancelled_base = super::VceRebuildCandidateBase {
            namespace_id: candidate_config.binding.namespace_id.clone(),
            checkpoint_version: 1,
            job_id: String::from("cancelled-candidate-job"),
        };
        // Record the second independent target before its physical allocation, without reviving the first attempt.
        // 在物理分配前记录第二个独立目标，不复活首次尝试。
        super::VceProjectJobStore::new(data_plane.clone(), cancelled_config.project_id.clone())
            .register(&super::jobs::replacement_test_receipt(
                cancelled_config.clone(),
                cancelled_base.clone(),
            ))
            .await
            .expect("persist cancellable candidate target");
        drop(
            current_factory
                .prepare_rebuild_candidate(cancelled_config.clone(), cancelled_base.clone())
                .await
                .expect("allocate cancellable candidate"),
        );
        // reopened restores only physical ownership; it does not admit writes or create any missing tables.
        // reopened 仅恢复物理归属，不准入写入，也不创建任何缺失表。
        let reopened = current_factory
            .reopen_rebuild_candidate(&cancelled_config, &cancelled_base)
            .await
            .expect("reopen exact candidate");
        assert!(
            reopened
                .find_missing(vulcan_context_engine::FindMissingRequest {
                    request_id: String::from("unadmitted-recovery"),
                    blob_names: BTreeSet::new(),
                })
                .await
                .is_err()
        );
        // wrong_job demonstrates that neither reopening nor cancellation accepts another job's identity.
        // wrong_job 证明重新打开和取消都不接受其他作业身份。
        let mut wrong_job = cancelled_base.clone();
        wrong_job.job_id.push_str("-wrong");
        assert!(
            current_factory
                .reopen_rebuild_candidate(&cancelled_config, &wrong_job)
                .await
                .is_err()
        );
        assert!(reopened.cancel_rebuild_candidate(&wrong_job).await.is_err());
        // wrong_config must not silently replace the candidate's allocated revision during recovery.
        // wrong_config 在恢复时不得静默替换候选分配修订。
        let mut wrong_config = cancelled_config.clone();
        wrong_config.config_revision += 1;
        assert!(
            current_factory
                .reopen_rebuild_candidate(&wrong_config, &cancelled_base)
                .await
                .is_err()
        );
        // recovered_writer receives the separate explicit grant after exact read-only reopening.
        // recovered_writer 在精确只读重新打开之后另行取得明确授权。
        let recovered_writer = reopened
            .admit_rebuild_candidate_writer(cancelled_base.clone())
            .await
            .expect("readmit exact pending candidate");
        recovered_writer
            .stage(IndexStageBatch {
                operation_id: String::from("stage-cancellable-candidate"),
                refresh_id: String::from("cancellable-refresh"),
                generation: 1,
                blobs: Vec::new(),
            })
            .await
            .expect("stage cancellable empty candidate");
        // cancelled_commit is otherwise a valid first empty publication for the new candidate chain.
        // cancelled_commit 在其他方面是新候选链合法的首次空发布。
        let mut cancelled_commit = candidate_commit.clone();
        cancelled_commit.request_id = String::from("commit-cancelled-candidate");
        cancelled_commit.refresh_id = String::from("cancellable-refresh");
        cancelled_commit.staged_blob_names.clear();
        cancelled_commit.checkpoint.chain_id = cancelled_config.binding.checkpoint_chain_id.clone();
        cancelled_commit.checkpoint.members.clear();
        cancelled_commit.state_snapshot.identity = VceStateIdentity::new(
            cancelled_config.binding.namespace_id.clone(),
            cancelled_config.binding.checkpoint_chain_id.clone(),
            cancelled_config.binding.schema_version,
            cancelled_config.binding.embedding_space.clone(),
        );
        cancelled_commit.state_snapshot.workspace.checkpoint =
            Some(cancelled_commit.checkpoint.clone());
        cancelled_commit.state_snapshot.workspace.records.clear();
        // summary mirrors the explicitly empty candidate, not the populated predecessor.
        // summary 对应明确为空的候选，而不是有内容的前驱。
        let summary = cancelled_commit
            .state_snapshot
            .last_refresh
            .as_mut()
            .expect("cancelled candidate summary");
        summary.request_id = cancelled_commit.refresh_id.clone();
        summary.added_blob_count = 0;
        summary.indexed_blob_count = 0;
        recovered_writer
            .validate_commit(&cancelled_commit)
            .expect("valid candidate before cancellation");
        // lease and script are prepared before revocation so the SQL guard itself must enforce cancellation.
        // lease 与脚本在撤销前准备，因此必须由 SQL 守卫自身执行取消保护。
        let lease = recovered_writer
            .prepare_publication_lease(&cancelled_commit, "cancelled-run")
            .await
            .expect("prepare cancellable publication");
        let script = recovered_writer
            .build_commit_script(
                &cancelled_commit,
                &super::commit_request_fingerprint(&cancelled_commit)
                    .expect("cancelled candidate fingerprint"),
                &[],
                Some(&lease),
            )
            .expect("build cancellable publication");
        // Retire the predecessor after admission: cancellation must remain possible when write admission is stale.
        // 在准入后退役前驱：写入准入已过期时仍必须可以取消。
        data_plane
            .execute(
                "UPDATE vce_project_bindings SET active = 0 WHERE namespace_id = ?;",
                vec![super::sqlite_text(&cancelled_base.namespace_id)],
                "retire fixture predecessor temporarily",
            )
            .await
            .expect("retire cancellation predecessor");
        reopened
            .cancel_rebuild_candidate(&cancelled_base)
            .await
            .expect("revoke exact candidate");
        data_plane
            .execute(
                "UPDATE vce_project_bindings SET active = 1 WHERE namespace_id = ?;",
                vec![super::sqlite_text(&cancelled_base.namespace_id)],
                "restore fixture predecessor",
            )
            .await
            .expect("restore cancellation predecessor");
        assert!(
            data_plane
                .execute(&script, Vec::new(), "reject revoked candidate publication")
                .await
                .is_err()
        );
        assert!(
            recovered_writer
                .commit_pinned(cancelled_commit, "cancelled-run")
                .await
                .is_err()
        );
        assert!(
            data_plane
                .open_project_index(cancelled_config.clone())
                .await
                .is_err(),
            "ordinary open cannot revive cancelled candidate"
        );
        assert!(
            reopened
                .admit_rebuild_candidate_writer(cancelled_base.clone())
                .await
                .is_err()
        );
        assert_eq!(
            recovered_writer
                .generation_state("cancellable-refresh", 1, VcePortOperation::Stage)
                .await
                .expect("cancel retains in-flight staging")
                .as_deref(),
            Some("staging")
        );
        // cancelled_reopened permits exact cancellation replay after lost handles, but never write readmission.
        // cancelled_reopened 允许丢失句柄后精确重放取消，但绝不重新准入写入。
        let cancelled_reopened = current_factory
            .reopen_rebuild_candidate(&cancelled_config, &cancelled_base)
            .await
            .expect("reopen revoked candidate");
        cancelled_reopened
            .cancel_rebuild_candidate(&cancelled_base)
            .await
            .expect("replay durable cancellation");
        assert!(
            cancelled_reopened
                .admit_rebuild_candidate_writer(cancelled_base.clone())
                .await
                .is_err()
        );
        assert_eq!(
            data_plane
                .load_active_project_config(&candidate_config.project_id)
                .await
                .expect("cancellation preserves published index"),
            Some(candidate_config.clone())
        );
        // The fixture has no running stage task, so its exact refresh may now drain through existing cleanup.
        // 夹具不存在运行中的暂存任务，因此现在可通过既有清理排空精确刷新。
        recovered_writer
            .abort_refresh_staging("cancellable-refresh")
            .await
            .expect("drain owned cancelled staging");
        assert!(
            !recovered_writer
                .has_unresolved_staging()
                .await
                .expect("cancelled staging drained")
        );
        drop(lease);
        drop(cancelled_reopened);
        drop(recovered_writer);
        drop(reopened);
    })
}

/// Verifies a live lease rejects shutdown before any idle binding enters partial closure.
/// 验证活动租约会在任何空闲绑定进入部分关闭前拒绝关闭。
#[tokio::test(flavor = "current_thread")]
async fn shutdown_live_lease_preflight_preserves_all_cached_bindings() {
    let temporary = tempdir().expect("create VCE shutdown preflight root");
    let data_root = temporary.path().join("data");
    let first_root = temporary.path().join("project-one");
    let second_root = temporary.path().join("project-two");
    fs::create_dir_all(&first_root).expect("create first shutdown project root");
    fs::create_dir_all(&second_root).expect("create second shutdown project root");
    fs::create_dir_all(&data_root).expect("create shutdown application root");
    let metadata_path = data_root.join("host-metadata.sqlite3");
    let host_runtime = HostRuntimeHandle::new_for_test(HostRuntimeMetadata {
        data_root,
        sqlite_path: metadata_path.clone(),
        sqlite_binding_id: format!("vce-shutdown-app-{}", Uuid::new_v4()),
        system_statistics_sqlite_path: metadata_path,
        system_statistics_sqlite_binding_id: format!("vce-shutdown-statistics-{}", Uuid::new_v4()),
    });
    let factory = bootstrap_vce_data_plane(&host_runtime)
        .await
        .expect("bootstrap VCE shutdown preflight factory");
    let first = factory
        .create_project_index(test_project_config("shutdown-first", &first_root, 1))
        .await
        .expect("create first shutdown project");
    let second = factory
        .create_project_index(test_project_config("shutdown-second", &second_root, 1))
        .await
        .expect("create second shutdown project");
    let ordered = factory.cached_store_states().await;
    assert_eq!(ordered.len(), 2);
    // Make the first cache entry idle and retain the second entry's external lease. The old
    // one-pass loop closed the first binding before discovering the second live lease.
    // 让首个缓存条目空闲并保留第二个条目的外部租约。旧的单遍循环会先关闭首个绑定，
    // 然后才发现第二个活动租约。
    let (idle, live) = if first.data_plane.project_owner_for_test() == ordered[0].0 {
        (first, second)
    } else {
        (second, first)
    };
    drop(idle);
    assert!(
        factory
            .close_project_bindings()
            .await
            .expect_err("live lease rejects shutdown preflight")
            .contains("live project leases")
    );
    assert!(
        factory
            .cached_store_states()
            .await
            .iter()
            .all(|(_, closing)| !closing),
        "rejected shutdown must not partially close an idle binding"
    );
    assert!(live.load_state_snapshot().await.is_ok());
    drop(live);
    factory
        .shutdown()
        .await
        .expect("close shutdown preflight factory");
}

/// Exercises engine publication, retrieval, mutations, restart, and project isolation on real storage.
/// 在真实存储上验证引擎发布、检索、变更、重启与项目隔离。
#[tokio::test(flavor = "current_thread")]
async fn in_process_engine_end_to_end_preserves_checkpoint_isolation() {
    let temporary = tempdir().expect("create VCE engine integration root");
    let data_root = temporary.path().join("data");
    let first_root = temporary.path().join("project-one");
    let second_root = temporary.path().join("project-two");
    fs::create_dir_all(&first_root).expect("create first VCE engine project root");
    fs::create_dir_all(&second_root).expect("create second VCE engine project root");
    let metadata_path = data_root.join("host-metadata.sqlite3");
    let host_runtime = HostRuntimeHandle::new_for_test(HostRuntimeMetadata {
        data_root: data_root.clone(),
        sqlite_path: metadata_path.clone(),
        sqlite_binding_id: format!("vce-engine-app-{}", Uuid::new_v4()),
        system_statistics_sqlite_path: metadata_path,
        system_statistics_sqlite_binding_id: format!("vce-engine-statistics-{}", Uuid::new_v4()),
    });
    fs::create_dir_all(&data_root).expect("create test application root");
    let factory = bootstrap_vce_data_plane(&host_runtime)
        .await
        .expect("bootstrap project-isolated VCE engine factory");
    let first_index = factory
        .create_project_index(test_project_config("project-1", &first_root, 1))
        .await
        .expect("open first VCE engine project index");
    // Metadata inspection uses this project's actual physical binding, not a shared fixture.
    // 元数据检查使用此项目的实际物理绑定，而不是共享夹具。
    let data_plane = first_index.data_plane.clone();
    let first_ai = deterministic_ai_for(&first_index);
    // The sole new layout is initialized together; passive reads never fabricate a checkpoint.
    // 唯一新布局统一初始化；被动读取绝不虚构检查点。
    let before_provenance = data_plane
        .embedding_provenance("project-1")
        .await
        .expect("read pre-initialization provenance")
        .expect("existing original binding");
    assert_eq!(before_provenance.checkpoint_version, None);
    assert_eq!(
        before_provenance.source_fingerprints,
        BTreeSet::from([first_index.physical_fingerprint.clone()])
    );
    // Origin tables belong to base schema, not a lazily installed compatibility extension.
    // 来源表属于基础结构，不是惰性安装的兼容扩展。
    let base_tables: Vec<super::CountRow> = data_plane.query_rows(
            "SELECT COUNT(*) AS count FROM sqlite_master WHERE type = 'table' AND name IN ('vce_embedding_stage_origins', 'vce_embedding_generation_origins', 'vce_read_leases');",
            Vec::new(),
        ).await.expect("inspect sole schema");
    assert_eq!(base_tables[0].count, 3);
    let first_config = VceConfig::new(
        first_index.binding().checkpoint_chain_id.clone(),
        first_index.binding().embedding_space.clone(),
    )
    .expect("build first VCE engine configuration");
    let mut first_engine = VceEngine::new(first_config.clone()).expect("create first VCE engine");
    let initial_lib = "pub fn semantic_entry() -> usize { 1 }\n";
    let initial_other = "pub fn secondary_entry() -> usize { 2 }\n";
    let initial_result = first_engine
        .refresh(
            RefreshRequest::full(
                "e2e-initial",
                RefreshReason::Initial,
                vec![
                    FileObservation::present(
                        "src/lib.rs",
                        initial_lib,
                        Some(1),
                        FileSource::Filesystem,
                    ),
                    FileObservation::present(
                        "src/other.rs",
                        initial_other,
                        Some(1),
                        FileSource::Filesystem,
                    ),
                ],
                Some(String::from("source-1")),
            ),
            &first_index,
            &first_ai,
            &NoReadSource,
        )
        .await
        .expect("publish initial VCE engine checkpoint");
    assert_eq!(initial_result.indexed_blobs.len(), 2);
    // The first commit records actual sources in the already-initialized versioned layout.
    // 首次提交在已初始化的版本化布局中记录实际来源。
    let initial_provenance = data_plane
        .embedding_provenance("project-1")
        .await
        .expect("read committed provenance")
        .expect("committed binding");
    assert_eq!(
        initial_provenance.checkpoint_version,
        first_engine.status().checkpoint_version
    );
    assert_eq!(initial_provenance.original, before_provenance.original);
    assert_eq!(
        initial_provenance.source_fingerprints,
        before_provenance.source_fingerprints
    );
    // Blob ownership follows committed rows, not every historical source or pending stage.
    // 块对象归属跟随已提交记录，而不是每个历史来源或待提交暂存。
    let origin_rows: Vec<super::CountRow> = data_plane
        .query_rows(
            "SELECT COUNT(*) AS count FROM vce_blob_embedding_origins WHERE namespace_id = ?;",
            vec![super::sqlite_text(&first_index.binding.namespace_id)],
        )
        .await
        .expect("count initial vector origins");
    assert_eq!(origin_rows[0].count, 2);
    // Keep the original publication alive across several later commits and garbage collection.
    // 在后续多次提交和垃圾回收期间保留原始发布对象。
    let initial_read = first_index
        .pin_checkpoint(
            first_engine
                .workspace_snapshot()
                .checkpoint
                .expect("initial checkpoint"),
            "engine-fixture-run",
        )
        .await
        .expect("pin initial checkpoint");
    let initial_retrieval = first_engine
        .retrieve(
            RetrieveRequest::current_checkpoint("e2e-query-1", "find semantic entry"),
            &initial_read,
            &first_ai,
        )
        .await
        .expect("retrieve initial VCE context");
    assert!(!initial_retrieval.hits.is_empty());

    // Rebind only execution after explicit host approval; stored tables and snapshot identity stay original.
    // 明确宿主批准后仅重新绑定执行；存储表和快照身份保持原始值。
    let mut target_space = first_ai.space.clone();
    target_space.physical_fingerprint = Some(String::from("provider-b:same-model:3"));
    let grant = vulcan_context_engine::VceEmbeddingExecutionContract::new(
        first_index.binding().clone(),
        target_space.clone(),
        "fixture-direct-project-approval",
    )
    .expect("approve exact original-to-target space");
    let original_writer = first_index.clone();
    let first_index = first_index
        .with_embedding_execution(grant.clone())
        .expect("retain original index with a different actual producer");
    let mut first_config = first_config;
    first_config.embedding_execution = Some(grant);
    first_engine =
        VceEngine::from_state_snapshot(first_config.clone(), first_engine.state_snapshot())
            .expect("restore original checkpoint under approved target execution");
    let mut first_ai = deterministic_ai_for(&first_index);
    first_ai.space = target_space.clone();
    assert!(
        first_index
            .validate_embedding_stage_origin("e2e-initial", initial_result.generation)
            .await
            .is_err(),
        "approved B cannot relabel A's previously staged vectors"
    );
    assert_ne!(
        first_ai.space.physical_fingerprint,
        first_index.binding().embedding_space.physical_fingerprint
    );
    // A newly authorized execution still reads the same immutable original checkpoint.
    // 新授权的执行仍然读取同一个不可变原始检查点。
    let approved_read = first_index
        .pin_checkpoint(
            first_engine
                .workspace_snapshot()
                .checkpoint
                .expect("approved checkpoint"),
            "engine-fixture-run",
        )
        .await
        .expect("pin approved original checkpoint");
    let old_engine =
        VceEngine::from_state_snapshot(first_config.clone(), first_engine.state_snapshot())
            .expect("retain old core publication");
    // Pause after actual vector recall; update will happen before body materialization.
    // 在实际向量召回之后暂停；更新发生在正文物化之前。
    let recalled_old_rows = approved_read
        .search_vectors(
            &approved_read.content_table,
            &[1.0, 0.0, 0.0],
            &old_engine
                .workspace_snapshot()
                .checkpoint
                .expect("old members")
                .members,
            8,
            VcePortOperation::DenseSearch,
        )
        .await
        .expect("recall original vectors");
    assert!(!recalled_old_rows.is_empty());
    assert!(
        !first_engine
            .retrieve(
                RetrieveRequest::current_checkpoint("e2e-approved-old-index", "semantic entry"),
                &approved_read,
                &first_ai,
            )
            .await
            .expect("query original vectors using approved target")
            .hits
            .is_empty()
    );

    let old_lib_blob = blob_name("src/lib.rs", initial_lib).expect("build old lib Blob");
    let changed_lib = "pub fn semantic_entry() -> usize { 3 }\n";
    let changed_result = first_engine
        .refresh(
            RefreshRequest::paths(
                "e2e-change",
                RefreshReason::FileWatcher,
                vec![FileObservation::present(
                    "src/lib.rs",
                    changed_lib,
                    Some(2),
                    FileSource::Filesystem,
                )],
                Vec::new(),
                Some(String::from("source-2")),
            ),
            &first_index,
            &first_ai,
            &NoReadSource,
        )
        .await
        .expect("publish changed VCE Blob");
    assert_eq!(changed_result.indexed_blobs.len(), 1);
    // Materialize vectors recalled before the commit; body and line ranges remain original.
    // 物化提交前召回的向量；正文和行号仍保持原始值。
    let retained_hits = approved_read
        .materialize_chunk_hits(
            recalled_old_rows,
            &[1.0, 0.0, 0.0],
            super::SearchSource::Dense,
        )
        .await
        .expect("materialize old vectors after new commit");
    assert!(
        retained_hits
            .iter()
            .any(|hit| hit.path == "src/lib.rs" && hit.content == initial_lib)
    );
    assert!(
        retained_hits
            .iter()
            .all(|hit| !hit.content.contains("{ 3 }"))
    );
    let old_result = old_engine
        .retrieve_read_view(
            RetrieveRequest::current_checkpoint("old-query-after-publish", "semantic_entry"),
            &approved_read,
            &first_ai,
        )
        .await
        .expect("all channels query retained publication");
    assert!(
        old_result
            .hits
            .iter()
            .any(|hit| hit.path == "src/lib.rs" && hit.content == initial_lib)
    );
    let changed_read = first_index
        .pin_checkpoint(
            first_engine
                .workspace_snapshot()
                .checkpoint
                .expect("changed checkpoint"),
            "engine-fixture-run",
        )
        .await
        .expect("pin changed checkpoint");
    // Each auxiliary channel must read old membership instead of current management views.
    // 每个辅助通道必须读取旧成员关系，而不是当前管理视图。
    let old_members = old_engine
        .workspace_snapshot()
        .checkpoint
        .expect("old channel scope")
        .members;
    let old_symbols = approved_read
        .search_exact(super::ExactSearchRequest {
            request_id: String::from("old-symbols-after-publish"),
            identifiers: vec![String::from("semantic_entry")],
            allowed_blobs: old_members.clone(),
            limit: 8,
        })
        .await
        .expect("read old exact symbols");
    assert!(!old_symbols.hits.is_empty());
    assert!(
        old_symbols
            .hits
            .iter()
            .all(|hit| hit.content.contains("{ 1 }") && hit.range.start_line == 1)
    );
    let old_paths = approved_read
        .search_path(super::PathSearchRequest {
            request_id: String::from("old-paths-after-publish"),
            query_index: 0,
            query_vector: EmbeddingVector {
                input_id: String::from("old-path-vector"),
                values: vec![1.0, 0.0, 0.0],
            },
            allowed_blobs: old_members,
            limit: 8,
        })
        .await
        .expect("read old path documents");
    assert_eq!(old_paths.hits.len(), 2);
    assert!(old_paths.hits.iter().any(|hit| hit.path == "src/other.rs"));
    assert!(approved_read.require_write_access().is_err());
    assert!(
        first_index
            .integrity_issues()
            .await
            .expect("inspect healthy retained index")
            .is_empty(),
        "a leased predecessor awaiting reclamation is not corrupt staging"
    );
    assert!(
        first_index
            .pin_checkpoint(
                old_engine
                    .workspace_snapshot()
                    .checkpoint
                    .expect("stale checkpoint"),
                "engine-fixture-run"
            )
            .await
            .is_err(),
        "new admissions cannot resurrect a retired checkpoint"
    );
    // Untouched A vectors and newly generated B vectors coexist without laundering their provenance.
    // 未改动的 A 向量与新生成的 B 向量共存，不混淆其来源。
    let mixed_provenance = first_index
        .embedding_provenance()
        .await
        .expect("read mixed actual sources");
    assert!(
        original_writer
            .validate_embedding_stage_origin("e2e-change", changed_result.generation)
            .await
            .is_err(),
        "old A handle cannot publish B's staged vectors"
    );
    assert_eq!(mixed_provenance.original, initial_provenance.original);
    assert_eq!(
        mixed_provenance.source_fingerprints,
        BTreeSet::from([
            initial_provenance.original.physical_fingerprint.clone(),
            target_space
                .physical_fingerprint
                .clone()
                .expect("explicit B identity"),
        ])
    );
    // Corrupt only isolated fixture metadata, prove fail-closed inspection, and restore the exact row owner.
    // 仅破坏隔离夹具元数据，验证读取失败关闭，再恢复精确记录所有者。
    data_plane.execute(
            "UPDATE vce_embedding_generation_origins SET physical_fingerprint = 'fixture-invalid-producer' WHERE namespace_id = ? AND refresh_id = 'e2e-change';",
            vec![super::sqlite_text(&first_index.binding.namespace_id)], "inject fixture origin mismatch",
        ).await.expect("inject isolated origin mismatch");
    assert!(first_index.embedding_provenance().await.is_err());
    data_plane.execute(
            "UPDATE vce_embedding_generation_origins SET physical_fingerprint = 'provider-b:same-model:3' WHERE namespace_id = ? AND refresh_id = 'e2e-change';",
            vec![super::sqlite_text(&first_index.binding.namespace_id)], "restore exact fixture origin owner",
        ).await.expect("restore isolated origin metadata");
    assert_eq!(
        first_index
            .embedding_provenance()
            .await
            .expect("recheck exact restoration"),
        mixed_provenance
    );
    assert_eq!(
        changed_result.deleted_blobs,
        BTreeSet::from([old_lib_blob.clone()])
    );
    assert!(
        changed_read
            .search_dense(DenseSearchRequest {
                request_id: String::from("e2e-search-old-lib"),
                query_index: 0,
                query_vector: EmbeddingVector {
                    input_id: String::from("e2e-old-lib-vector"),
                    values: vec![1.0, 0.0, 0.0],
                },
                allowed_blobs: BTreeSet::from([old_lib_blob]),
                limit: 4,
            })
            .await
            .is_err()
    );

    let old_other_blob = blob_name("src/other.rs", initial_other).expect("build old other Blob");
    let renamed_blob = blob_name("src/renamed.rs", initial_other).expect("build renamed Blob");
    let rename_result = first_engine
        .refresh(
            RefreshRequest::paths(
                "e2e-rename",
                RefreshReason::FileWatcher,
                vec![FileObservation::present(
                    "src/renamed.rs",
                    initial_other,
                    Some(3),
                    FileSource::Filesystem,
                )],
                vec![String::from("src/other.rs")],
                Some(String::from("source-3")),
            ),
            &first_index,
            &first_ai,
            &NoReadSource,
        )
        .await
        .expect("publish renamed VCE Blob");
    assert_eq!(rename_result.indexed_blobs, vec![renamed_blob.clone()]);
    assert_eq!(
        rename_result.deleted_blobs,
        BTreeSet::from([old_other_blob])
    );
    let delete_result = first_engine
        .refresh(
            RefreshRequest::paths(
                "e2e-delete",
                RefreshReason::FileWatcher,
                Vec::new(),
                vec![String::from("src/renamed.rs")],
                Some(String::from("source-4")),
            ),
            &first_index,
            &first_ai,
            &NoReadSource,
        )
        .await
        .expect("publish deleted VCE path");
    assert!(delete_result.indexed_blobs.is_empty());
    assert_eq!(delete_result.deleted_blobs, BTreeSet::from([renamed_blob]));
    // Deleted and superseded blobs lose current origin membership, while generation audit remains.
    // 删除及替换的块对象移出当前来源成员集合，但代次审计记录保留。
    let remaining_origins: Vec<super::CountRow> = data_plane
        .query_rows(
            "SELECT COUNT(*) AS count FROM vce_blob_embedding_origins WHERE namespace_id = ?;",
            vec![super::sqlite_text(&first_index.binding.namespace_id)],
        )
        .await
        .expect("count remaining vector origins");
    assert_eq!(remaining_origins[0].count, 1);
    // Deletion-only publication has no new embedding source and must not claim a model call.
    // 仅删除发布没有新向量来源，不得声称发生了模型调用。
    let written_generations: Vec<super::CountRow> = data_plane.query_rows(
            "SELECT COUNT(*) AS count FROM vce_embedding_generation_origins WHERE namespace_id = ?;",
            vec![super::sqlite_text(&first_index.binding.namespace_id)],
        ).await.expect("count actual embedding generations");
    assert_eq!(written_generations[0].count, 3);

    // Management content must retain the old checkpoint after replacement, rename and deletion.
    // 替换、重命名与删除之后，管理内容仍须保留旧检查点。
    let writer_guard = first_index.mutation_lock.lock().await;
    // Holding the writer lock models an in-flight write without delaying published management reads.
    // 持有写锁模拟在途写入，不能延迟已发布管理读取。
    let old_management = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        approved_read.management_snapshot(super::VceManagementRequest {
            request_id: String::from("old-management-after-delete"),
        }),
    )
    .await
    .expect("pinned management cannot wait for the writer lock")
    .expect("read pinned old management snapshot");
    drop(writer_guard);
    assert_eq!(old_management.checkpoint_version, Some(1));
    assert_eq!(old_management.counts.files, 2);
    assert_eq!(old_management.integrity.checkpoint_blob_count, 2);
    assert_eq!(old_management.integrity.missing_blob_count, 0);
    // Aggregate statistics still include the separate ready path channel.
    // 聚合统计仍包含独立的就绪路径通道。
    assert_eq!(
        old_management.counts.vectors,
        old_management.counts.chunks + old_management.counts.path_documents
    );
    // The unpinned management handle reports the latest committed inventory.
    // 未固定管理句柄报告最新已提交清单。
    let current_management = first_index
        .management_snapshot(super::VceManagementRequest {
            request_id: String::from("current-management-after-delete"),
        })
        .await
        .expect("read current management snapshot");
    assert_eq!(current_management.counts.files, 1);
    assert_ne!(
        current_management.checkpoint_version,
        old_management.checkpoint_version
    );
    // File rows and ordered previews must share the lease, not consult active SQL views.
    // 文件行与有序预览必须共享租约，不能读取活动 SQL 视图。
    let old_files = approved_read
        .file_page(vulcan_context_engine::VceFileQuery {
            path_prefix: None,
            status: None,
            auxiliary_failures_only: false,
            page: super::VcePageRequest::new(None, 10).expect("bounded fixture page"),
        })
        .await
        .expect("read old file inventory");
    assert_eq!(old_files.total, Some(2));
    assert!(
        old_files
            .items
            .iter()
            .any(|file| file.path == "src/other.rs")
    );
    assert!(
        !old_files
            .items
            .iter()
            .any(|file| file.path == "src/renamed.rs")
    );
    // Prefixes are literal and case-sensitive, including SQL wildcard characters.
    // 前缀按字面且区分大小写匹配，包括 SQL 通配字符。
    for (prefix, expected_count) in [
        ("src", 2),
        ("src/lib.rs", 1),
        ("s_c", 0),
        ("s%c", 0),
        ("SRC", 0),
    ] {
        // query uses the same retained publication while the active index has already changed.
        // query 使用同一个保留发布，此时活动索引已经变化。
        let query = vulcan_context_engine::VceFileQuery {
            path_prefix: Some(prefix.to_owned()),
            status: None,
            auxiliary_failures_only: false,
            page: super::VcePageRequest::new(None, 10).expect("bounded prefix page"),
        };
        // page must satisfy the core's exact filter contract rather than SQLite LIKE semantics.
        // page 必须满足核心精确过滤契约，而不是 SQLite LIKE 语义。
        let page = approved_read
            .file_page(query.clone())
            .await
            .expect("read literal prefix");
        assert_eq!(page.items.len(), expected_count, "literal prefix {prefix}");
        vulcan_context_engine::validate_management_response(
            &vulcan_context_engine::VceManagementQuery::Files(query),
            &vulcan_context_engine::VceManagementQueryResult::Files(page),
        )
        .expect("host prefix result matches core contract");
    }
    // The original lib identity is no longer active but its leased chunks are still available.
    // 原始 lib 身份已不再活动，但其租约代码块仍可用。
    let old_detail = approved_read
        .blob_detail(&blob_name("src/lib.rs", initial_lib).expect("original lib identity"))
        .await
        .expect("read old blob detail")
        .expect("old blob retained");
    assert!(!old_detail.chunks.is_empty());
    assert!(
        old_detail
            .chunks
            .iter()
            .any(|chunk| chunk.preview.as_deref() == Some(initial_lib))
    );
    assert!(
        approved_read
            .integrity_issues()
            .await
            .expect("old integrity")
            .is_empty()
    );

    let persisted = data_plane
        .load_state_snapshot(&first_index.binding().namespace_id)
        .await
        .expect("load VCE restart state")
        .expect("VCE restart state exists");
    let mut restored_engine = VceEngine::from_state_snapshot(first_config, persisted)
        .expect("restore VCE engine from checkpoint state");
    let restored_read = first_index
        .pin_checkpoint(
            restored_engine
                .workspace_snapshot()
                .checkpoint
                .expect("restored checkpoint"),
            "engine-fixture-run",
        )
        .await
        .expect("pin restored publication");
    let restored_retrieval = restored_engine
        .retrieve(
            RetrieveRequest::current_checkpoint("e2e-query-restored", "semantic entry"),
            &restored_read,
            &first_ai,
        )
        .await
        .expect("retrieve VCE context after restart");
    assert!(!restored_retrieval.hits.is_empty());

    let second_index = factory
        .create_project_index(test_project_config("project-2", &second_root, 1))
        .await
        .expect("open second VCE engine project index");
    let second_ai = deterministic_ai_for(&second_index);
    let mut second_engine = VceEngine::new(
        VceConfig::new(
            second_index.binding().checkpoint_chain_id.clone(),
            second_index.binding().embedding_space.clone(),
        )
        .expect("build second VCE engine configuration"),
    )
    .expect("create second VCE engine");
    let second_content = "pub fn project_two_only() -> usize { 20 }\n";
    let second_blob = blob_name("src/two.rs", second_content).expect("build project-two Blob");
    second_engine
        .refresh(
            RefreshRequest::full(
                "e2e-project-two",
                RefreshReason::Initial,
                vec![FileObservation::present(
                    "src/two.rs",
                    second_content,
                    Some(1),
                    FileSource::Filesystem,
                )],
                Some(String::from("source-project-two")),
            ),
            &second_index,
            &second_ai,
            &NoReadSource,
        )
        .await
        .expect("publish second project checkpoint");
    let current_first_blob =
        blob_name("src/lib.rs", changed_lib).expect("build current first-project Blob");
    let second_read = second_index
        .pin_checkpoint(
            second_engine
                .workspace_snapshot()
                .checkpoint
                .expect("second checkpoint"),
            "engine-fixture-run",
        )
        .await
        .expect("pin second project");
    assert!(
        restored_read
            .search_dense(DenseSearchRequest {
                request_id: String::from("e2e-cross-search-first"),
                query_index: 0,
                query_vector: EmbeddingVector {
                    input_id: String::from("e2e-cross-first-vector"),
                    values: vec![1.0, 0.0, 0.0],
                },
                allowed_blobs: BTreeSet::from([second_blob.clone()]),
                limit: 4,
            })
            .await
            .is_err()
    );
    assert!(
        second_read
            .search_dense(DenseSearchRequest {
                request_id: String::from("e2e-cross-search-second"),
                query_index: 0,
                query_vector: EmbeddingVector {
                    input_id: String::from("e2e-cross-second-vector"),
                    values: vec![1.0, 0.0, 0.0],
                },
                allowed_blobs: BTreeSet::from([current_first_blob]),
                limit: 4,
            })
            .await
            .is_err()
    );
    let second_retrieval = second_engine
        .retrieve(
            RetrieveRequest::current_checkpoint("e2e-query-project-two", "project two only"),
            &second_read,
            &second_ai,
        )
        .await
        .expect("retrieve isolated second-project context");
    assert!(
        second_retrieval
            .hits
            .iter()
            .all(|hit| hit.path == "src/two.rs")
    );
    // Identical content rebuilds create distinct producer rows while older readers remain valid.
    // 相同内容重建生成不同生产记录，同时旧读取方仍然有效。
    let rebuild_result = restored_engine
        .refresh(
            RefreshRequest::full_rebuild(
                "e2e-same-blob-rebuild",
                RefreshReason::Manual,
                vec![FileObservation::present(
                    "src/lib.rs",
                    changed_lib,
                    Some(2),
                    FileSource::Filesystem,
                )],
                None,
            ),
            &first_index,
            &first_ai,
            &NoReadSource,
        )
        .await
        .expect("rebuild identical blob into a new attempt");
    assert_eq!(rebuild_result.indexed_blobs.len(), 1);
    let rebuilt_read = first_index
        .pin_checkpoint(
            restored_engine
                .workspace_snapshot()
                .checkpoint
                .expect("rebuilt checkpoint"),
            "engine-fixture-run",
        )
        .await
        .expect("pin rebuilt publication");
    let held_attempts: Vec<super::CountRow> = data_plane.query_rows(
            "SELECT COUNT(*) AS count FROM vce_blob_versions WHERE namespace_id = ? AND blob_name = ?;",
            vec![super::sqlite_text(&first_index.binding.namespace_id), super::sqlite_text(blob_name("src/lib.rs", changed_lib).expect("unchanged blob identity"))],
        ).await.expect("count same-blob immutable attempts");
    assert_eq!(held_attempts[0].count, 2);
    // A valid empty checkpoint is published normally without erasing leased old versions.
    // 合法空检查点正常发布，不抹除已被租约保留的旧版本。
    restored_engine
        .refresh(
            RefreshRequest::full("e2e-empty-project", RefreshReason::Manual, Vec::new(), None),
            &first_index,
            &first_ai,
            &NoReadSource,
        )
        .await
        .expect("publish empty current checkpoint");
    let empty_read = first_index
        .pin_checkpoint(
            restored_engine
                .workspace_snapshot()
                .checkpoint
                .expect("empty checkpoint"),
            "engine-fixture-run",
        )
        .await
        .expect("pin initialized empty publication");
    let empty_result = restored_engine
        .retrieve_read_view(
            RetrieveRequest::current_checkpoint("e2e-empty-query", "semantic entry"),
            &empty_read,
            &first_ai,
        )
        .await
        .expect("query initialized empty checkpoint");
    assert!(empty_result.hits.is_empty());
    assert!(
        !rebuilt_read
            .search_dense(DenseSearchRequest {
                request_id: String::from("query-rebuilt-after-empty"),
                query_index: 0,
                query_vector: EmbeddingVector {
                    input_id: String::from("retained-rebuild-vector"),
                    values: vec![1.0, 0.0, 0.0]
                },
                allowed_blobs: BTreeSet::from([
                    blob_name("src/lib.rs", changed_lib).expect("rebuilt blob")
                ]),
                limit: 4,
            })
            .await
            .expect("query old rebuild after empty publication")
            .hits
            .is_empty()
    );
    // An old timestamp does not authorize deleting another host's read protection.
    // 较旧的时间戳不代表获准删除另一个宿主的读取保护。
    data_plane.execute(
            "INSERT INTO vce_read_leases(namespace_id, lease_id, owner_run_id, checkpoint_version, created_at_unix_ms) VALUES (?, 'foreign-fixture-lease', 'another-host-run', 1, 0);",
            vec![super::sqlite_text(&first_index.binding.namespace_id)], "insert foreign fixture lease",
        ).await.expect("retain independently owned reader");
    // Last-reference release permits precise retired-vector and metadata cleanup, not active deletion.
    // 最后引用释放后允许精确清理退役向量和元数据，而不是删除活动内容。
    drop(initial_read);
    drop(approved_read);
    drop(changed_read);
    drop(restored_read);
    drop(second_read);
    drop(rebuilt_read);
    drop(empty_read);
    first_index
        .recover_pending_vector_cleanup()
        .await
        .expect("collect unleased old versions");
    second_index
        .recover_pending_vector_cleanup()
        .await
        .expect("flush second-project leases");
    let foreign_versions: Vec<super::CountRow> = data_plane
        .query_rows(
            "SELECT COUNT(*) AS count FROM vce_blob_versions WHERE namespace_id = ?;",
            vec![super::sqlite_text(&first_index.binding.namespace_id)],
        )
        .await
        .expect("count foreign-reader protected versions");
    assert_eq!(foreign_versions[0].count, 2);
    // Only explicit release by the known fixture owner permits collection of the remaining checkpoint.
    // 仅已知夹具所有者的明确释放才允许回收剩余检查点。
    data_plane.execute(
            "DELETE FROM vce_read_leases WHERE namespace_id = ? AND lease_id = 'foreign-fixture-lease' AND owner_run_id = 'another-host-run';",
            vec![super::sqlite_text(&first_index.binding.namespace_id)], "release exact foreign fixture lease",
        ).await.expect("release known fixture owner");
    first_index
        .recover_pending_vector_cleanup()
        .await
        .expect("collect released foreign checkpoint");
    let remaining_versions: Vec<super::CountRow> = data_plane
        .query_rows(
            "SELECT COUNT(*) AS count FROM vce_blob_versions WHERE namespace_id = ?;",
            vec![super::sqlite_text(&first_index.binding.namespace_id)],
        )
        .await
        .expect("count retained physical versions");
    assert_eq!(remaining_versions[0].count, 0);
    let remaining_leases: Vec<super::CountRow> = data_plane
        .query_rows("SELECT COUNT(*) AS count FROM vce_read_leases;", Vec::new())
        .await
        .expect("count released read leases");
    assert_eq!(remaining_leases[0].count, 0);
    drop(original_writer);
    drop(first_index);
    drop(second_index);
    drop(data_plane);
    factory
        .shutdown()
        .await
        .expect("close project-isolated test factory");
}

/// Builds a deterministic AI registration for one project index.
/// 为一个项目索引构建确定性 AI 注册。
fn deterministic_ai_for(index: &super::VceProjectIndex) -> DeterministicAi {
    DeterministicAi {
        binding: VceAiBinding::new(
            format!("registration:{}", index.binding().namespace_id),
            "vce-code-retrieval",
            index.binding().embedding_space.vector_space_id.clone(),
        )
        .expect("build deterministic VCE AI binding"),
        space: index.binding().embedding_space.clone(),
        embedding_requests: std::sync::Mutex::new(Vec::new()),
        pause_embedding_at: None,
        embedding_paused: tokio::sync::Notify::new(),
    }
}

/// Measure real storage for 3,398 synthetic files and a repeated rebuild without network calls.
/// 对 3,398 个合成文件及重复重建测量真实存储，不发起网络调用。
/// Explicit opt-in keeps the scale fixture out of routine correctness tests.
/// 显式选择执行，避免规模夹具进入常规正确性测试。
#[tokio::test(flavor = "current_thread")]
#[ignore = "run explicitly for shared storage scale measurements"]
async fn shared_storage_scale_baseline() {
    // Every physical store belongs to this disposable fixture.
    // 全部物理存储归属于此可丢弃夹具。
    let temporary = tempdir().unwrap();
    let data_root = temporary.path().join("data");
    let project_root = temporary.path().join("project");
    fs::create_dir_all(&data_root).unwrap();
    fs::create_dir_all(&project_root).unwrap();
    let metadata_path = data_root.join("host.sqlite3");
    let host = HostRuntimeHandle::new_for_test(HostRuntimeMetadata {
        data_root,
        sqlite_path: metadata_path.clone(),
        sqlite_binding_id: format!("vce-scale-{}", Uuid::new_v4()),
        system_statistics_sqlite_path: metadata_path,
        system_statistics_sqlite_binding_id: format!("vce-scale-statistics-{}", Uuid::new_v4()),
    });
    let factory = bootstrap_vce_data_plane(&host).await.unwrap();
    let index = factory
        .create_project_index(test_project_config("scale", &project_root, 1))
        .await
        .unwrap();
    let config = VceConfig::new(
        index.binding().checkpoint_chain_id.clone(),
        index.binding().embedding_space.clone(),
    )
    .unwrap();
    let mut engine = VceEngine::new(config).unwrap();
    // Source size and vector dimensions are synthetic; timings exclude real provider latency.
    // 源码大小和向量维度为合成数据；耗时不包含真实供应商延迟。
    let files = (0..3398)
        .map(|number| {
            FileObservation::present(
                format!("src/file_{number}.rs"),
                format!("pub fn value_{number}() -> usize {{ {number} }}\n"),
                Some(1),
                FileSource::Filesystem,
            )
        })
        .collect::<Vec<_>>();
    for round in 0..2 {
        let ai = deterministic_ai_for(&index);
        let mut request = RefreshRequest::full(
            format!("scale-{round}"),
            RefreshReason::Initial,
            files.clone(),
            Some("scale-source".into()),
        );
        request.force_reindex = true;
        // Attribute elapsed wall time to the last observed phase without modifying production state.
        // 将墙钟耗时归于最后观察的阶段，不修改生产状态。
        let phases = Arc::new(std::sync::Mutex::new((
            std::time::Instant::now(),
            String::from("admission"),
            BTreeMap::<String, u128>::new(),
        )));
        let observed = phases.clone();
        let control = vulcan_context_engine::VceRefreshControl::default().with_observer(Arc::new(
            move |progress: vulcan_context_engine::VceRefreshProgress| {
                let mut measurements = observed.lock().unwrap();
                let key = format!(
                    "{:?}/{:?}",
                    progress.phase,
                    progress.activity.as_ref().map(|activity| activity.stage)
                );
                if key != measurements.1 {
                    let elapsed = measurements.0.elapsed().as_micros();
                    let previous = measurements.1.clone();
                    *measurements.2.entry(previous).or_default() += elapsed;
                    measurements.0 = std::time::Instant::now();
                    measurements.1 = key;
                }
            },
        ));
        let started = std::time::Instant::now();
        let result = engine
            .refresh_observed(request, &index, &ai, &NoReadSource, &control)
            .await
            .unwrap();
        assert_eq!(result.indexed_blobs.len(), 3398);
        let calls = ai.embedding_requests.lock().unwrap().len();
        if round == 1 {
            assert_eq!(
                calls, 0,
                "unchanged rebuild must reuse all provider results"
            );
        }
        let plane = index.data_plane.clone();
        let sizes = crate::local_sql::with_connection_async(
            &plane.local_runtime,
            &plane.sqlite_binding,
            |connection| {
                let payload: i64 = connection
                    .query_row(
                        "SELECT sum(byte_size) FROM vce_shared_payloads",
                        [],
                        |row| row.get(0),
                    )
                    .map_err(|error| error.to_string())?;
                let pages: i64 = connection
                    .query_row("PRAGMA page_count", [], |row| row.get(0))
                    .map_err(|error| error.to_string())?;
                let free: i64 = connection
                    .query_row("PRAGMA freelist_count", [], |row| row.get(0))
                    .map_err(|error| error.to_string())?;
                Ok((payload, pages, free))
            },
        )
        .await
        .unwrap();
        println!(
            "VCE_SHARED_SCALE round={round} files=3398 total_ms={} provider_calls={calls} shared_payload_bytes={} sqlite_pages={} free_pages={} phases_us={:?}",
            started.elapsed().as_millis(),
            sizes.0,
            sizes.1,
            sizes.2,
            phases.lock().unwrap().2
        );
    }
}

/// Verify a fresh engine resumes a persisted plan after a future is dropped mid-provider call.
/// 验证供应商调用期间 Future 被丢弃后，新引擎能够恢复持久化计划。
///
/// `index` owns an empty fixture namespace; returns after one checkpoint is published without
/// repeating the saved batch, or panics with the failed recovery invariant.
/// `index` 拥有空夹具命名空间；成功时完成一次检查点发布且不重复已保存批次，失败时携带恢复不变量触发断言。
async fn verify_manifest_crash_recovery(index: &super::VceProjectIndex) {
    // The small input budget guarantees more than one content request for this fixture.
    // 较小的输入预算确保该夹具存在多个内容请求。
    let mut config = VceConfig::new(
        index.binding().checkpoint_chain_id.clone(),
        index.binding().embedding_space.clone(),
    )
    .expect("recovery engine config");
    config.indexing.embedding_max_inputs = 1;
    let mut engine = VceEngine::new(config.clone()).expect("initial recovery engine");
    let mut interrupted_ai = deterministic_ai_for(index);
    interrupted_ai.pause_embedding_at = Some(2);
    let request = RefreshRequest::full(
        "crash-recovery-run",
        RefreshReason::Initial,
        (0..3)
            .map(|number| {
                FileObservation::present(
                    format!("src/recovery-{number}.rs"),
                    format!("pub fn recovery_{number}() -> usize {{ {number} }}\n"),
                    Some(1),
                    FileSource::Filesystem,
                )
            })
            .collect(),
        Some(String::from("recovery-source")),
    );
    // Dropping a pending future reproduces process loss without invoking explicit cancellation cleanup.
    // 丢弃等待中的 Future 模拟进程丢失，不执行显式取消清理。
    let mut interrupted =
        Box::pin(engine.refresh(request.clone(), index, &interrupted_ai, &NoReadSource));
    tokio::time::timeout(std::time::Duration::from_secs(15), async {
            tokio::select! {
                result = &mut interrupted => panic!("fixture finished before its crash boundary: {result:?}"),
                () = interrupted_ai.embedding_paused.notified() => {}
            }
        }).await.expect("reach actual second embedding request");
    drop(interrupted);
    drop(engine);
    // Only the first successful result is durable; the second request must be retried.
    // 只有第一份成功结果已持久化；第二个请求必须重试。
    let pending = index
        .load_embedding_manifest(request.request_id.clone())
        .await
        .expect("read interrupted manifest")
        .expect("complete plan published before provider I/O");
    assert_eq!(pending.state, VceEmbeddingRunState::Running);
    assert_eq!(pending.batches[0].state, VceEmbeddingBatchState::Succeeded);
    assert_eq!(pending.batches[1].state, VceEmbeddingBatchState::Running);
    assert!(
        index
            .load_embedding_batch_result(
                pending.run_id.clone(),
                pending.batches[0].batch_id.clone()
            )
            .await
            .expect("load saved first result")
            .is_some()
    );
    // A fresh engine has no in-memory preparation or checkpoint state to reuse.
    // 新引擎没有可复用的内存准备产物或检查点状态。
    let mut resumed = VceEngine::new(config).expect("fresh recovery engine");
    let resumed_ai = deterministic_ai_for(index);
    let result = resumed
        .refresh(request.clone(), index, &resumed_ai, &NoReadSource)
        .await
        .expect("resume durable manifest and publish");
    assert_eq!(result.indexed_blobs.len(), 3);
    let completed = index
        .load_embedding_manifest(pending.run_id)
        .await
        .expect("load recovered terminal manifest")
        .expect("retain recovered manifest");
    assert_eq!(completed.state, VceEmbeddingRunState::Succeeded);
    assert!(
        completed
            .batches
            .iter()
            .all(|batch| batch.state == VceEmbeddingBatchState::Succeeded)
    );
    {
        let requests = resumed_ai
            .embedding_requests
            .lock()
            .expect("read resumed requests");
        assert_eq!(requests.len(), completed.batches.len() - 1);
        assert!(
            !requests.contains(
                &interrupted_ai
                    .embedding_requests
                    .lock()
                    .expect("read original requests")[0]
            )
        );
    }
    // A new forced run must reuse exact successful vectors instead of repeating provider requests.
    // 新的强制运行必须复用精确成功向量，而非重复供应商请求。
    let mut retry = request;
    retry.request_id = String::from("cross-run-reuse");
    retry.force_reindex = true;
    let cached_ai = deterministic_ai_for(index);
    resumed
        .refresh(retry, index, &cached_ai, &NoReadSource)
        .await
        .expect("rebuild through shared preparation and results");
    assert!(
        cached_ai.embedding_requests.lock().unwrap().is_empty(),
        "a repeated unchanged rebuild must make no embedding calls"
    );
}

/// Returns the recursive on-disk byte size of one test artifact path.
/// 返回一个测试产物路径在磁盘上的递归字节大小。
fn file_or_directory_size(path: &std::path::Path) -> u64 {
    let metadata = fs::metadata(path).expect("read VCE performance artifact metadata");
    if metadata.is_file() {
        return metadata.len();
    }
    fs::read_dir(path)
        .expect("read VCE performance artifact directory")
        .map(|entry| {
            let entry = entry.expect("read VCE performance artifact entry");
            file_or_directory_size(&entry.path())
        })
        .sum()
}

/// Builds one exact test configuration from a canonical project root.
/// 从规范项目根目录构建一个精确测试配置。
fn test_project_config(
    project_id: &str,
    root: &std::path::Path,
    revision: u64,
) -> VceProjectIndexConfig {
    let canonical = fs::canonicalize(root).expect("canonicalize VCE test root");
    let root_path = canonical.display().to_string();
    let root_hash = hex::encode(Sha256::digest(root_path.as_bytes()));
    let chain = format!("project:{project_id}:root:{root_hash}");
    let namespace = format!("vce:{chain}");
    let vector_space_id = format!("code-semantic:{project_id}:{root_hash}");
    let physical_fingerprint = String::from("test-provider:test-model:3");
    let preprocessing_version = String::from("vce-preprocess-v1");
    let chunker_version = String::from("vce-chunker-v1");
    let embedding_space = EmbeddingSpace {
        vector_space_id: vector_space_id.clone(),
        physical_fingerprint: Some(physical_fingerprint.clone()),
        dimension: 3,
        preprocessing_version: preprocessing_version.clone(),
        chunker_version: chunker_version.clone(),
    };
    VceProjectIndexConfig {
        project_id: project_id.to_owned(),
        root_path,
        root_hash,
        binding: VceIndexBinding::new(namespace, chain, 1, embedding_space)
            .expect("build VCE test binding"),
        vector_space_id,
        physical_fingerprint,
        preprocessing_version,
        chunker_version,
        dimension: 3,
        config_revision: revision,
    }
}
