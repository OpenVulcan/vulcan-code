use super::*;
use crate::embedding::{EmbeddingSpace, SegmentVector};
use crate::management::{VceOperationKind, VceOperationPhase};
use crate::model::{Chunk, ChunkKind, HitKind};
use crate::ports::{
    CommitReceipt, FindMissingResponse, VceAiBinding, VceEmbeddingBatchCapabilities,
    VceEmbeddingBatchCollectedResponse, VceEmbeddingBatchPlan, VceEmbeddingBatchSnapshot,
    VceEmbeddingBatchSubmission, VceEmbeddingBatchTaskPlan, VceEmbeddingBatchUploadedInput,
    VceIndexCounts, VceIndexIntegrity, VceIndexJobSummary, VceIndexStoreStatus, VceRerankResponse,
    VceSearchResponse,
};
use crate::workspace::FileObservation;
use futures::executor::block_on;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

/// Build a deterministic test embedding space shared by fake ports.
/// 构造供伪端口共享的确定性测试向量空间。
fn test_space() -> EmbeddingSpace {
    EmbeddingSpace {
        vector_space_id: "code-test".to_owned(),
        physical_fingerprint: Some("fake-model".to_owned()),
        dimension: 2,
        preprocessing_version: "v1".to_owned(),
        chunker_version: "recursive-v1".to_owned(),
    }
}

/// Build an engine with small deterministic chunking limits for unit tests.
/// 使用小型确定性切块限制构造单元测试引擎。
fn test_engine() -> VceEngine {
    let mut config = VceConfig::new("code-chain", test_space()).unwrap();
    config.indexing.chunker.max_chars = 100;
    config.indexing.chunker.min_chars = 1;
    VceEngine::new(config).unwrap()
}

/// Return a ready future for a fake asynchronous host operation.
/// 为伪异步宿主操作返回立即就绪的 Future。
fn ready<T: Send + 'static>(value: T) -> crate::ports::VceFuture<T> {
    Box::pin(async move { value })
}

/// Fake VCE AI registration that returns deterministic two-dimensional vectors.
/// 返回确定性二维向量的伪 VCE AI 注册。
struct FakeAi {
    /// Fixed registration metadata.
    /// 固定注册元数据。
    binding: VceAiBinding,
    /// Vector space returned by every Embedding request.
    /// 每次向量请求返回的向量空间。
    space: EmbeddingSpace,
    /// Whether optional rewrite calls should fail.
    /// 可选改写调用是否失败。
    fail_rewrite: bool,
    /// Whether optional Rerank calls should fail.
    /// 可选重排调用是否失败。
    fail_rerank: bool,
    /// Whether only path Embedding purposes should fail.
    /// 是否仅让路径向量用途失败。
    fail_path_embedding: bool,
    /// Whether the fake host reports Embedding as available.
    /// 伪宿主是否报告向量能力可用。
    embedding_available: bool,
    /// Whether the fake common registration is active.
    /// 伪共同注册是否处于活动状态。
    registered: bool,
    /// Whether the fake host reports query rewriting as available.
    /// 伪宿主是否报告查询改写能力可用。
    query_rewrite_available: bool,
    /// Whether the fake host reports Rerank as available.
    /// 伪宿主是否报告重排能力可用。
    rerank_available: bool,
    /// Optional provider-declared maximum number of Embedding inputs per request.
    /// 伪宿主声明的单次向量请求最大输入条数。
    embedding_max_inputs: Option<usize>,
    /// Embedding operation identities retained for uniqueness assertions.
    /// 为唯一性断言保留的向量操作标识。
    embedding_operation_ids: Arc<Mutex<Vec<String>>>,
    /// Input counts observed by each fake Embedding request.
    /// 伪向量请求实际观察到的输入条数。
    embedding_batch_sizes: Arc<Mutex<Vec<usize>>>,
    /// Number of optional query-rewrite calls retained for budget assertions.
    /// 为预算断言保留的可选查询改写调用次数。
    rewrite_call_count: Arc<Mutex<usize>>,
    /// Candidate counts received by the fake Rerank capability.
    /// 伪重排能力接收到的候选数量。
    rerank_candidate_counts: Arc<Mutex<Vec<usize>>>,
    /// Number of frozen capability snapshots read by logical engine entries.
    /// 逻辑引擎入口读取冻结能力快照的次数。
    capability_snapshot_call_count: Arc<Mutex<usize>>,
    /// Optional asynchronous Batch limits exposed by the fake provider.
    /// 伪供应商暴露的可选异步 Batch 限制。
    embedding_batch_capabilities: Option<VceEmbeddingBatchCapabilities>,
    /// Ordered remote lifecycle events retained for protocol assertions.
    /// 为协议断言保留的有序远端生命周期事件。
    embedding_batch_events: Arc<Mutex<Vec<String>>>,
    /// Whether the next fake submission should report an indeterminate response.
    /// 下一次伪提交是否应报告不确定响应。
    submission_unknown_once: Arc<Mutex<bool>>,
    /// Optional one-shot status failure used to verify submitted-task recovery.
    /// 用于验证已提交任务恢复的可选单次状态失败。
    batch_status_error_once: Arc<Mutex<Option<VcePortError>>>,
    /// Whether each remote task should remain active for its first status observation.
    /// 每个远端任务是否应在首次状态观测时保持活动态。
    batch_status_in_progress_once_per_task: bool,
    /// Remote task identities whose first status observation has already occurred.
    /// 已经发生首次状态观测的远端任务身份。
    batch_status_seen_tasks: Arc<Mutex<BTreeSet<String>>>,
    /// Settled counts reported by successive first active-state observations.
    /// 连续首次活动态观测所报告的已结算计数。
    batch_status_first_settled_counts: Arc<Mutex<VecDeque<u32>>>,
    /// Optional deterministic collection failure returned after a completed provider task.
    /// 供应商任务完成后返回的可选确定性收集失败。
    batch_collection_error: Option<VcePortError>,
    /// Whether collection should replace one result identity with an unknown value.
    /// 收集是否应将一项结果身份替换为未知值。
    batch_collection_returns_unknown_identity: bool,
}

impl VceAiPort for FakeAi {
    /// Return the fake fixed registration.
    /// 返回伪固定注册。
    fn binding(&self) -> &VceAiBinding {
        &self.binding
    }

    /// Return all fake capabilities as available.
    /// 返回全部可用的伪能力状态。
    fn capability_snapshot(&self) -> VceAiCapabilitySnapshot {
        *self.capability_snapshot_call_count.lock().unwrap() += 1;
        VceAiCapabilitySnapshot {
            registration_id: self.binding.registration_id.clone(),
            consumer_profile_id: self.binding.consumer_profile_id.clone(),
            vector_space_id: self.binding.vector_space_id.clone(),
            registered: self.registered,
            embedding_available: self.embedding_available,
            query_rewrite_available: self.query_rewrite_available,
            rerank_available: self.rerank_available,
        }
    }

    /// Return the optional provider-declared Embedding input bound.
    /// 返回可选的供应商声明向量输入上限。
    fn embedding_max_inputs(&self) -> Option<usize> {
        self.embedding_max_inputs
    }

    /// Return the optional fake asynchronous Batch contract.
    /// 返回可选的伪异步 Batch 契约。
    fn embedding_batch_capabilities(&self) -> Option<VceEmbeddingBatchCapabilities> {
        self.embedding_batch_capabilities.clone()
    }

    /// Split the ordered fake request into deterministic provider-sized tasks.
    /// 将有序伪请求拆分成确定性的供应商大小任务。
    fn plan_embedding_batch(
        &self,
        request: VceEmbeddingBatchRequest,
    ) -> crate::ports::VceFuture<Result<VceEmbeddingBatchPlan, VcePortError>> {
        let Some(capabilities) = self.embedding_batch_capabilities.clone() else {
            return ready(Err(VcePortError::Unavailable {
                operation: VcePortOperation::Embedding,
            }));
        };
        let maximum_requests = usize::try_from(capabilities.maximum_requests)
            .expect("fake maximum requests fits usize");
        let tasks = request
            .batch
            .segments
            .chunks(maximum_requests)
            .enumerate()
            .map(|(task_index, segments)| {
                let first_input_index = task_index * maximum_requests;
                let encoded_payload_bytes = segments
                    .iter()
                    .map(|segment| segment.text.len() as u64 + 1)
                    .sum::<u64>()
                    .max(1);
                VceEmbeddingBatchTaskPlan {
                    task_id: format!("{}:task:{task_index}", request.context.operation_id),
                    first_input_index: u32::try_from(first_input_index)
                        .expect("fake task offset fits u32"),
                    input_count: u32::try_from(segments.len()).expect("fake task count fits u32"),
                    encoded_payload_bytes,
                    payload_sha256: format!("{task_index:064x}"),
                }
            })
            .collect();
        ready(Ok(VceEmbeddingBatchPlan {
            request_id: request.context.operation_id,
            request_fingerprint: String::from(
                "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            ),
            capabilities,
            tasks,
        }))
    }

    /// Record a fake file upload and return its durable provider identity.
    /// 记录一次伪文件上传并返回其持久供应商身份。
    fn upload_embedding_batch_input(
        &self,
        _request: VceEmbeddingBatchRequest,
        _plan: VceEmbeddingBatchPlan,
        task: VceEmbeddingBatchTaskPlan,
    ) -> crate::ports::VceFuture<Result<VceEmbeddingBatchUploadedInput, VcePortError>> {
        self.embedding_batch_events
            .lock()
            .unwrap()
            .push(format!("upload:{}", task.task_id));
        ready(Ok(VceEmbeddingBatchUploadedInput {
            provider_input_file_id: format!("file-{}", task.task_id.replace(':', "-")),
            task_id: task.task_id,
            uploaded_at_unix_ms: 1,
        }))
    }

    /// Record one fake submission and preserve an uploaded file identity when required.
    /// 记录一次伪提交，并在需要时保留已上传文件身份。
    fn submit_embedding_batch(
        &self,
        _request: VceEmbeddingBatchRequest,
        _plan: VceEmbeddingBatchPlan,
        task: VceEmbeddingBatchTaskPlan,
        uploaded_input: Option<VceEmbeddingBatchUploadedInput>,
    ) -> crate::ports::VceFuture<Result<VceEmbeddingBatchSubmission, VcePortError>> {
        self.embedding_batch_events
            .lock()
            .unwrap()
            .push(format!("submit:{}", task.task_id));
        let mut submission_unknown_once = self.submission_unknown_once.lock().unwrap();
        if *submission_unknown_once {
            *submission_unknown_once = false;
            return ready(Err(VcePortError::SubmissionUnknown {
                operation: VcePortOperation::Embedding,
                message: String::from("fake submission response was interrupted"),
            }));
        }
        ready(Ok(VceEmbeddingBatchSubmission {
            provider_batch_id: format!("provider-{}", task.task_id.replace(':', "-")),
            provider_input_file_id: uploaded_input
                .as_ref()
                .map(|uploaded| uploaded.provider_input_file_id.clone()),
            task_id: task.task_id,
            submitted_at_unix_ms: 1,
        }))
    }

    /// Reconcile an indeterminate file-backed fake submission by exact uploaded identity.
    /// 按精确上传身份对账一次不确定的文件型伪提交。
    fn reconcile_embedding_batch_submission(
        &self,
        _request: VceEmbeddingBatchRequest,
        _plan: VceEmbeddingBatchPlan,
        task: VceEmbeddingBatchTaskPlan,
        uploaded_input: VceEmbeddingBatchUploadedInput,
    ) -> crate::ports::VceFuture<Result<Option<VceEmbeddingBatchSubmission>, VcePortError>> {
        self.embedding_batch_events
            .lock()
            .unwrap()
            .push(format!("reconcile:{}", task.task_id));
        ready(Ok(Some(VceEmbeddingBatchSubmission {
            provider_batch_id: format!("provider-{}", task.task_id.replace(':', "-")),
            provider_input_file_id: Some(uploaded_input.provider_input_file_id),
            task_id: task.task_id,
            submitted_at_unix_ms: 1,
        })))
    }

    /// Return one immediately completed fake provider observation.
    /// 返回一份立即完成的伪供应商观测。
    fn embedding_batch_status(
        &self,
        request: VceEmbeddingBatchRequest,
        _plan: VceEmbeddingBatchPlan,
        submission: VceEmbeddingBatchSubmission,
    ) -> crate::ports::VceFuture<Result<VceEmbeddingBatchSnapshot, VcePortError>> {
        self.embedding_batch_events
            .lock()
            .unwrap()
            .push(format!("status:{}", submission.task_id));
        if let Some(error) = self.batch_status_error_once.lock().unwrap().take() {
            return ready(Err(error));
        }
        let total = u32::try_from(request.batch.segments.len()).expect("fake Batch size fits u32");
        let first_observation = self.batch_status_in_progress_once_per_task
            && self
                .batch_status_seen_tasks
                .lock()
                .unwrap()
                .insert(submission.task_id.clone());
        let first_settled = if first_observation {
            self.batch_status_first_settled_counts
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or(0)
                .min(total)
        } else {
            total
        };
        ready(Ok(VceEmbeddingBatchSnapshot {
            submission,
            state: if first_observation {
                VceEmbeddingBatchProviderState::InProgress
            } else {
                VceEmbeddingBatchProviderState::Completed
            },
            total_requests: Some(total),
            completed_requests: Some(first_settled),
            failed_requests: Some(0),
            output_file_id: (!first_observation).then(|| String::from("output-file")),
            error_file_id: None,
            error: None,
        }))
    }

    /// Return deterministic vectors for every segment in one completed fake task.
    /// 为一个已完成伪任务中的每个分段返回确定性向量。
    fn collect_embedding_batch(
        &self,
        request: VceEmbeddingBatchRequest,
        _plan: VceEmbeddingBatchPlan,
        task: VceEmbeddingBatchTaskPlan,
        _snapshot: VceEmbeddingBatchSnapshot,
    ) -> crate::ports::VceFuture<Result<VceEmbeddingBatchCollectedResponse, VcePortError>> {
        self.embedding_batch_events
            .lock()
            .unwrap()
            .push(format!("collect:{}", task.task_id));
        if let Some(error) = &self.batch_collection_error {
            return ready(Err(error.clone()));
        }
        let mut vectors: Vec<SegmentVector> = request
            .batch
            .segments
            .iter()
            .map(|segment| SegmentVector {
                input_id: segment.request_input_id(),
                values: vec![1.0, 1.0],
            })
            .collect();
        if self.batch_collection_returns_unknown_identity
            && let Some(first) = vectors.first_mut()
        {
            first.input_id = String::from("unknown-provider-input");
        }
        ready(Ok(VceEmbeddingBatchCollectedResponse {
            operation_id: request.context.operation_id,
            task_id: task.task_id,
            vectors,
            space: self.space.clone(),
            cleanup_warning: None,
        }))
    }

    /// Record successful cleanup of one fake provider input file.
    /// 记录一次伪供应商输入文件清理成功。
    fn cleanup_embedding_batch_input(
        &self,
        _request: VceEmbeddingBatchRequest,
        _plan: VceEmbeddingBatchPlan,
        submission: VceEmbeddingBatchSubmission,
    ) -> crate::ports::VceFuture<Result<(), VcePortError>> {
        self.embedding_batch_events
            .lock()
            .unwrap()
            .push(format!("cleanup:{}", submission.task_id));
        ready(Ok(()))
    }

    /// Record one fake cancellation and return an exact cancelled provider state.
    /// 记录一次伪取消并返回精确的供应商已取消状态。
    fn cancel_embedding_batch(
        &self,
        request: VceEmbeddingBatchRequest,
        plan: VceEmbeddingBatchPlan,
        submission: VceEmbeddingBatchSubmission,
    ) -> crate::ports::VceFuture<Result<VceEmbeddingBatchSnapshot, VcePortError>> {
        if request.context.operation_id != plan.request_id {
            return ready(Err(VcePortError::Rejected {
                operation: VcePortOperation::Embedding,
                reason: String::from(
                    "fake cancellation request differs from its frozen Batch plan",
                ),
            }));
        }
        self.embedding_batch_events
            .lock()
            .unwrap()
            .push(format!("cancel:{}", submission.task_id));
        let total = u32::try_from(request.batch.segments.len()).expect("fake Batch size fits u32");
        ready(Ok(VceEmbeddingBatchSnapshot {
            submission,
            state: VceEmbeddingBatchProviderState::Cancelled,
            total_requests: Some(total),
            completed_requests: Some(0),
            failed_requests: Some(0),
            output_file_id: None,
            error_file_id: None,
            error: Some(String::from("cancelled by fake provider")),
        }))
    }

    /// Return one deterministic vector for every requested segment.
    /// 为每个请求分段返回一个确定性向量。
    fn embed(
        &self,
        request: VceEmbeddingRequest,
    ) -> crate::ports::VceFuture<Result<VceEmbeddingResponse, VcePortError>> {
        self.embedding_batch_sizes
            .lock()
            .unwrap()
            .push(request.batch.segments.len());
        self.embedding_operation_ids
            .lock()
            .unwrap()
            .push(request.context.operation_id.clone());
        if self.fail_path_embedding
            && matches!(
                request.context.purpose,
                VceAiPurpose::CodePathEmbedding | VceAiPurpose::CodePathQueryEmbedding
            )
        {
            return ready(Err(VcePortError::Failed {
                operation: VcePortOperation::Embedding,
                message: "fake path Embedding failure".to_owned(),
            }));
        }
        let vectors = request
            .batch
            .segments
            .iter()
            .map(|segment| SegmentVector {
                input_id: segment.request_input_id(),
                values: vec![1.0, 1.0],
            })
            .collect();
        ready(Ok(VceEmbeddingResponse {
            operation_id: request.context.operation_id,
            vectors,
            space: self.space.clone(),
        }))
    }

    /// Return no rewrite or an explicit fake failure.
    /// 返回无改写结果或显式伪失败。
    fn rewrite_query(
        &self,
        request: VceQueryRewriteRequest,
    ) -> crate::ports::VceFuture<Result<VceQueryRewriteResponse, VcePortError>> {
        *self.rewrite_call_count.lock().unwrap() += 1;
        if self.fail_rewrite {
            return ready(Err(VcePortError::Failed {
                operation: VcePortOperation::QueryRewrite,
                message: "fake rewrite failure".to_owned(),
            }));
        }
        ready(Ok(VceQueryRewriteResponse {
            operation_id: request.context.operation_id,
            rewrites: vec![format!("{} implementation", request.query)],
        }))
    }

    /// Return the source order or an explicit fake failure.
    /// 返回原顺序或显式伪失败。
    fn rerank(
        &self,
        request: VceRerankRequest,
    ) -> crate::ports::VceFuture<Result<VceRerankResponse, VcePortError>> {
        self.rerank_candidate_counts
            .lock()
            .unwrap()
            .push(request.candidates.len());
        if self.fail_rerank {
            return ready(Err(VcePortError::Failed {
                operation: VcePortOperation::Rerank,
                message: "fake rerank failure".to_owned(),
            }));
        }
        let ordered_candidate_indexes = (0..request.candidates.len()).collect();
        ready(Ok(VceRerankResponse {
            operation_id: request.context.operation_id,
            ordered_candidate_indexes,
        }))
    }
}

/// Test analyzer that returns one duplicate occurrence for a selected source path.
/// 为指定源码路径返回重复出现记录的测试分析器。
#[derive(Debug)]
struct DuplicateSymbolAnalyzer;

impl VceStructureAnalyzer for DuplicateSymbolAnalyzer {
    /// Return the deterministic test analyzer identity.
    /// 返回确定性的测试分析器身份。
    fn analyzer_id(&self) -> &str {
        "duplicate-symbol-test-analyzer"
    }

    /// Return the test vector-space structure version.
    /// 返回测试向量空间使用的结构版本。
    fn version(&self) -> &str {
        "recursive-v1"
    }

    /// Build valid recursive output except for the selected duplicate-symbol path.
    /// 除指定的重复符号路径外，构建有效的递归结构输出。
    fn analyze(
        &self,
        blob_name: &str,
        path: &str,
        content: &str,
    ) -> Result<crate::structure::StructureAnalysis, StructureError> {
        let chunk = Chunk::new(
            blob_name.to_owned(),
            path,
            content.to_owned(),
            1,
            1,
            ChunkKind::Recursive,
        )
        .map_err(|error| StructureError::AnalysisFailed {
            message: error.to_string(),
        })?;
        let symbol = SymbolOccurrence::from_chunk(
            &chunk,
            "broken_symbol",
            crate::symbol::SymbolKind::Definition,
        );
        let symbols = if path.ends_with("bad.rs") {
            vec![symbol.clone(), symbol]
        } else {
            Vec::new()
        };
        Ok(crate::structure::StructureAnalysis {
            analyzer_id: self.analyzer_id().to_owned(),
            version: self.version().to_owned(),
            mode: StructureMode::RecursiveFallback,
            fallback_reason: None,
            chunks: vec![chunk],
            symbols,
        })
    }
}

/// Fake VLDB boundary that records commits and serves one deterministic hit.
/// 记录提交并返回一个确定性命中的伪 VLDB 边界。
struct FakeIndex {
    /// Fixed code-semantic namespace binding.
    /// 固定的代码语义命名空间绑定。
    binding: VceIndexBinding,
    /// Shared commit records for assertions.
    /// 供断言使用的共享提交记录。
    commits: Arc<Mutex<Vec<IndexCommit>>>,
    /// Prepared staged records retained for assertions.
    /// 为断言保留的已准备暂存记录。
    staged: Arc<Mutex<Vec<IndexedBlob>>>,
    /// Number of bounded staging calls retained for assertions.
    /// 为断言保留的有界暂存调用次数。
    stage_call_count: Arc<Mutex<usize>>,
    /// Number of staging abort calls retained for assertions.
    /// 为断言保留的暂存中止调用次数。
    abort_count: Arc<Mutex<usize>>,
    /// Whether commit should fail after staging.
    /// 提交是否应在暂存后失败。
    fail_commit: bool,
    /// Whether commit should return a mismatched receipt.
    /// 提交是否应返回不匹配的回执。
    invalid_commit_receipt: bool,
    /// Whether staging should return a mismatched receipt.
    /// 暂存是否应返回不匹配的回执。
    invalid_stage_receipt: bool,
    /// Optional content hit returned by dense search.
    /// 内容向量搜索返回的可选命中。
    hit: SearchHit,
    /// Whether each query variant should return a distinct valid occurrence.
    /// 每个查询变体是否应返回不同且有效的出现位置。
    vary_dense_hit_by_query: bool,
    /// Whether path retrieval should add one uncovered first-Chunk fallback.
    /// 路径检索是否应添加一个未覆盖文件的首代码块回填。
    return_path_fallback: bool,
    /// Canonical source path recorded for every staged Blob identity.
    /// 为每个已暂存 Blob 身份记录的规范源码路径。
    blob_paths: Arc<Mutex<BTreeMap<String, String>>>,
    /// Durable fake remote lifecycle records keyed by run and task identity.
    /// 按运行及任务身份索引的持久伪远端生命周期记录。
    remote_batch_records: Arc<Mutex<BTreeMap<(String, String), VceEmbeddingRemoteBatchRecord>>>,
    /// Durable fake logical results keyed by run and logical Batch identity.
    /// 按运行及逻辑 Batch 身份索引的持久伪逻辑结果。
    embedding_batch_results: Arc<Mutex<BTreeMap<(String, String), VceEmbeddingBatchResult>>>,
    /// Number of logical result writes performed through the fake storage port.
    /// 通过伪存储端口执行的逻辑结果写入次数。
    embedding_batch_result_write_count: Arc<Mutex<usize>>,
}

impl VceIndexPort for FakeIndex {
    /// Return the fixed fake index binding.
    /// 返回固定的伪索引绑定。
    fn binding(&self) -> &VceIndexBinding {
        &self.binding
    }

    /// Mark every requested Blob as unknown so the engine prepares it.
    /// 将每个请求 Blob 标记为未知，使引擎准备它。
    fn find_missing(
        &self,
        request: FindMissingRequest,
    ) -> crate::ports::VceFuture<Result<FindMissingResponse, VcePortError>> {
        ready(Ok(FindMissingResponse {
            operation_id: request.request_id,
            unknown: request.blob_names,
            nonindexed: BTreeSet::new(),
        }))
    }

    /// Acknowledge exactly the bounded staged Blob set.
    /// 精确确认有界暂存 Blob 集合。
    fn stage(
        &self,
        request: IndexStageBatch,
    ) -> crate::ports::VceFuture<Result<IndexStageReceipt, VcePortError>> {
        *self.stage_call_count.lock().unwrap() += 1;
        self.blob_paths.lock().unwrap().extend(
            request
                .blobs
                .iter()
                .map(|blob| (blob.blob_name.clone(), blob.path.clone())),
        );
        self.staged
            .lock()
            .unwrap()
            .extend(request.blobs.iter().cloned());
        let operation_id = if self.invalid_stage_receipt {
            format!("{}:invalid", request.operation_id)
        } else {
            request.operation_id
        };
        ready(Ok(IndexStageReceipt {
            operation_id,
            refresh_id: request.refresh_id,
            generation: request.generation,
            staged_blob_names: request
                .blobs
                .into_iter()
                .map(|blob| blob.blob_name)
                .collect(),
        }))
    }

    /// Acknowledge cleanup for an unpublished fake staging transaction.
    /// 确认清理尚未发布的伪暂存事务。
    fn abort_stage(
        &self,
        request: IndexAbortRequest,
    ) -> crate::ports::VceFuture<Result<crate::ports::IndexAbortReceipt, VcePortError>> {
        *self.abort_count.lock().unwrap() += 1;
        ready(Ok(crate::ports::IndexAbortReceipt {
            operation_id: request.operation_id,
            refresh_id: request.refresh_id,
            generation: request.generation,
            removed_blob_count: 0,
        }))
    }

    /// Record the staged publication and return its exact identities.
    /// 记录暂存发布并返回其精确身份。
    fn commit(
        &self,
        request: IndexCommit,
    ) -> crate::ports::VceFuture<Result<CommitReceipt, VcePortError>> {
        if self.fail_commit {
            return ready(Err(VcePortError::Failed {
                operation: VcePortOperation::Commit,
                message: "fake commit failure".to_owned(),
            }));
        }
        let indexed_blob_names = request
            .staged_blob_names
            .iter()
            .cloned()
            .collect::<Vec<_>>();
        let deleted_blob_names = request.deleted_blobs.iter().cloned().collect::<Vec<_>>();
        let receipt = CommitReceipt {
            operation_id: if self.invalid_commit_receipt {
                format!("{}:invalid", request.request_id)
            } else {
                request.request_id.clone()
            },
            refresh_id: request.refresh_id.clone(),
            checkpoint_chain_id: request.checkpoint.chain_id.clone(),
            generation: request.generation,
            checkpoint_version: request.checkpoint.version,
            indexed_count: indexed_blob_names.len(),
            deleted_count: request.deleted_blobs.len(),
            indexed_blob_names,
            deleted_blob_names,
        };
        self.commits.lock().unwrap().push(request);
        ready(Ok(receipt))
    }

    /// Return one dense hit inside the exact requested scope.
    /// 在精确请求范围内返回一个内容向量命中。
    fn search_dense(
        &self,
        request: DenseSearchRequest,
    ) -> crate::ports::VceFuture<Result<VceSearchResponse, VcePortError>> {
        let mut hit = self.hit.clone();
        hit.blob_name = request.allowed_blobs.iter().next().cloned().unwrap();
        hit.path = self
            .blob_paths
            .lock()
            .unwrap()
            .get(&hit.blob_name)
            .cloned()
            .expect("every searchable fake Blob must have a staged source path");
        if self.vary_dense_hit_by_query {
            hit.content = format!("fn candidate_{}() {{}}", request.query_index);
            hit.content_hash = crate::identity::content_hash(&hit.content);
        }
        hit.id = crate::identity::chunk_id_with_offset(
            &hit.blob_name,
            &hit.content_hash,
            hit.range.start_line,
            hit.range.end_line,
            hit.start_char_offset,
        );
        ready(Ok(VceSearchResponse {
            operation_id: request.request_id,
            hits: vec![hit],
        }))
    }

    /// Return no path hits in the generic fake.
    /// 在通用伪实现中不返回路径命中。
    fn search_path(
        &self,
        request: PathSearchRequest,
    ) -> crate::ports::VceFuture<Result<VcePathSearchResponse, VcePortError>> {
        if self.return_path_fallback {
            let blob_name = request.allowed_blobs.iter().next_back().cloned().unwrap();
            let path = "src/path_only.rs";
            let path_hit = crate::path_index::PathDocument::new(blob_name.clone(), path)
                .unwrap()
                .to_hit(0.9)
                .unwrap();
            let fallback = SearchHit::from_chunk(
                &Chunk::new(
                    blob_name,
                    path,
                    "fn path_only() {}",
                    1,
                    1,
                    ChunkKind::Recursive,
                )
                .unwrap(),
                0.9,
                SearchSource::Dense,
                HitKind::Content,
            );
            return ready(Ok(VcePathSearchResponse {
                operation_id: request.request_id,
                hits: vec![path_hit],
                fallback_hits: vec![fallback],
            }));
        }
        ready(Ok(VcePathSearchResponse {
            operation_id: request.request_id,
            hits: Vec::new(),
            fallback_hits: Vec::new(),
        }))
    }

    /// Return no exact hits in the generic fake.
    /// 在通用伪实现中不返回精确命中。
    fn search_exact(
        &self,
        request: ExactSearchRequest,
    ) -> crate::ports::VceFuture<Result<VceSearchResponse, VcePortError>> {
        ready(Ok(VceSearchResponse {
            operation_id: request.request_id,
            hits: Vec::new(),
        }))
    }

    /// Return a ready empty management snapshot.
    /// 返回一个就绪的空管理快照。
    fn management_snapshot(
        &self,
        request: VceManagementRequest,
    ) -> crate::ports::VceFuture<Result<VceIndexManagementSnapshot, VcePortError>> {
        ready(Ok(VceIndexManagementSnapshot {
            operation_id: request.request_id.clone(),
            status: VceIndexStoreStatus::Ready,
            generation: 0,
            checkpoint_version: None,
            counts: VceIndexCounts::default(),
            integrity: VceIndexIntegrity::default(),
            active_job: Some(VceIndexJobSummary {
                job_id: request.request_id,
                kind: VceOperationKind::InitialBuild,
                generation: 0,
                mode: Some(RefreshMode::Full),
                reason: Some(RefreshReason::Initial),
                phase: VceOperationPhase::Completed,
                attempt: 1,
                completed_units: 0,
                total_units: 0,
                unit: None,
                started_at_ns: None,
                finished_at_ns: None,
                coalesced: false,
                error: None,
            }),
            last_error: None,
        }))
    }

    /// Return an empty result for every typed management query variant.
    /// 为每一种类型化管理查询变体返回空结果。
    fn management_query(
        &self,
        request: VceManagementQueryRequest,
    ) -> crate::ports::VceFuture<Result<VceManagementQueryResponse, VcePortError>> {
        let result = match request.query {
            crate::management::VceManagementQuery::Files(_) => {
                crate::management::VceManagementQueryResult::Files(crate::management::VcePage {
                    items: Vec::new(),
                    next_cursor: None,
                    total: Some(0),
                })
            }
            crate::management::VceManagementQuery::BlobDetail { .. } => {
                crate::management::VceManagementQueryResult::BlobDetail(None)
            }
            crate::management::VceManagementQuery::Operations(_) => {
                crate::management::VceManagementQueryResult::Operations(
                    crate::management::VcePage {
                        items: Vec::new(),
                        next_cursor: None,
                        total: Some(0),
                    },
                )
            }
            crate::management::VceManagementQuery::IntegrityIssues(_) => {
                crate::management::VceManagementQueryResult::IntegrityIssues(
                    crate::management::VcePage {
                        items: Vec::new(),
                        next_cursor: None,
                        total: Some(0),
                    },
                )
            }
            crate::management::VceManagementQuery::MaintenancePlan { mode } => {
                crate::management::VceManagementQueryResult::MaintenancePlan(
                    crate::management::VceMaintenancePlan {
                        mode,
                        actions: Vec::new(),
                        affected_blob_count: 0,
                        physical_tables: Vec::new(),
                    },
                )
            }
        };
        ready(Ok(VceManagementQueryResponse {
            operation_id: request.operation_id,
            result,
        }))
    }

    /// Persist one fake remote lifecycle record by its stable task identity.
    /// 按稳定任务身份持久化一条伪远端生命周期记录。
    fn persist_embedding_remote_batch_record(
        &self,
        run_id: String,
        record: VceEmbeddingRemoteBatchRecord,
    ) -> crate::ports::VceFuture<Result<(), VcePortError>> {
        self.remote_batch_records
            .lock()
            .unwrap()
            .insert((run_id, record.remote_task_id.clone()), record);
        ready(Ok(()))
    }

    /// Load one purpose's fake remote records in deterministic logical order.
    /// 按确定性逻辑顺序读取一种用途的伪远端记录。
    fn load_embedding_remote_batch_records(
        &self,
        run_id: String,
        purpose: VceAiPurpose,
    ) -> crate::ports::VceFuture<Result<Vec<VceEmbeddingRemoteBatchRecord>, VcePortError>> {
        let mut records = self
            .remote_batch_records
            .lock()
            .unwrap()
            .iter()
            .filter_map(|((stored_run_id, _), record)| {
                (stored_run_id == &run_id && record.purpose == purpose.as_str())
                    .then_some(record.clone())
            })
            .collect::<Vec<_>>();
        records.sort_by_key(|record| record.first_logical_batch_index);
        ready(Ok(records))
    }

    /// Persist one validated fake logical Batch result.
    /// 持久化一份已校验的伪逻辑 Batch 结果。
    fn persist_embedding_batch_result(
        &self,
        run_id: String,
        result: VceEmbeddingBatchResult,
    ) -> crate::ports::VceFuture<Result<(), VcePortError>> {
        *self.embedding_batch_result_write_count.lock().unwrap() += 1;
        self.embedding_batch_results
            .lock()
            .unwrap()
            .insert((run_id, result.batch_id.clone()), result);
        ready(Ok(()))
    }

    /// Load one exact fake logical Batch result for restart recovery.
    /// 为重启恢复读取一份精确伪逻辑 Batch 结果。
    fn load_embedding_batch_result(
        &self,
        run_id: String,
        batch_id: String,
    ) -> crate::ports::VceFuture<Result<Option<VceEmbeddingBatchResult>, VcePortError>> {
        let result = self
            .embedding_batch_results
            .lock()
            .unwrap()
            .get(&(run_id, batch_id))
            .cloned();
        ready(Ok(result))
    }
}

/// Fake source boundary that fails if a test unexpectedly drops retained source text.
/// 当测试意外丢失已保留源码文本时失败的伪源码边界。
struct FakeSource;

impl VceSourcePort for FakeSource {
    /// Reject unexpected source reads in retained-content tests.
    /// 在保留内容测试中拒绝意外源码读取。
    fn read_sources(
        &self,
        _request: VceSourceReadRequest,
    ) -> crate::ports::VceFuture<Result<VceSourceReadResponse, VcePortError>> {
        ready(Err(VcePortError::Failed {
            operation: VcePortOperation::SourceRead,
            message: "unexpected fake source read".to_owned(),
        }))
    }
}

/// Fake host source boundary that returns immutable text by canonical path.
/// 按规范路径返回不可变文本的伪宿主源码边界。
struct SnapshotSource {
    /// Canonical path-to-content map owned by the fake host.
    /// 伪宿主持有的规范路径到内容映射。
    documents: BTreeMap<String, String>,
}

impl VceSourcePort for SnapshotSource {
    /// Return exactly the source documents requested by VCE.
    /// 精确返回 VCE 请求的源码文档。
    fn read_sources(
        &self,
        request: VceSourceReadRequest,
    ) -> crate::ports::VceFuture<Result<VceSourceReadResponse, VcePortError>> {
        let documents = request
            .expectations
            .iter()
            .map(|expectation| {
                let content = self
                    .documents
                    .get(&expectation.path)
                    .expect("fake source must contain every requested canonical path")
                    .clone();
                crate::ports::VceSourceDocument {
                    path: expectation.path.clone(),
                    size: content.len() as u64,
                    mtime_ns: expectation.mtime_ns,
                    content,
                }
            })
            .collect();
        ready(Ok(VceSourceReadResponse {
            operation_id: request.operation_id,
            documents,
        }))
    }
}

/// Create a fake host pair for functional tests.
/// 为功能测试创建伪宿主端口对。
fn fake_host() -> (FakeAi, FakeIndex, FakeSource, Arc<Mutex<Vec<IndexCommit>>>) {
    let binding = VceAiBinding::new("registration", "code-profile", "code-test").unwrap();
    let chunk = Chunk::new(
        "placeholder",
        "src/lib.rs",
        "fn main() {}",
        1,
        1,
        ChunkKind::Recursive,
    )
    .unwrap();
    let commits = Arc::new(Mutex::new(Vec::new()));
    let staged = Arc::new(Mutex::new(Vec::new()));
    let abort_count = Arc::new(Mutex::new(0));
    (
        FakeAi {
            binding,
            space: test_space(),
            fail_rewrite: false,
            fail_rerank: false,
            fail_path_embedding: false,
            embedding_available: true,
            registered: true,
            query_rewrite_available: true,
            rerank_available: true,
            embedding_max_inputs: None,
            embedding_operation_ids: Arc::new(Mutex::new(Vec::new())),
            embedding_batch_sizes: Arc::new(Mutex::new(Vec::new())),
            rewrite_call_count: Arc::new(Mutex::new(0)),
            rerank_candidate_counts: Arc::new(Mutex::new(Vec::new())),
            capability_snapshot_call_count: Arc::new(Mutex::new(0)),
            embedding_batch_capabilities: None,
            embedding_batch_events: Arc::new(Mutex::new(Vec::new())),
            submission_unknown_once: Arc::new(Mutex::new(false)),
            batch_status_error_once: Arc::new(Mutex::new(None)),
            batch_status_in_progress_once_per_task: false,
            batch_status_seen_tasks: Arc::new(Mutex::new(BTreeSet::new())),
            batch_status_first_settled_counts: Arc::new(Mutex::new(VecDeque::new())),
            batch_collection_error: None,
            batch_collection_returns_unknown_identity: false,
        },
        FakeIndex {
            binding: VceIndexBinding::new("vce:code-chain", "code-chain", 1, test_space()).unwrap(),
            commits: commits.clone(),
            staged,
            stage_call_count: Arc::new(Mutex::new(0)),
            abort_count,
            fail_commit: false,
            invalid_commit_receipt: false,
            invalid_stage_receipt: false,
            hit: SearchHit::from_chunk(&chunk, 1.0, SearchSource::Dense, HitKind::Content),
            vary_dense_hit_by_query: false,
            return_path_fallback: false,
            blob_paths: Arc::new(Mutex::new(BTreeMap::new())),
            remote_batch_records: Arc::new(Mutex::new(BTreeMap::new())),
            embedding_batch_results: Arc::new(Mutex::new(BTreeMap::new())),
            embedding_batch_result_write_count: Arc::new(Mutex::new(0)),
        },
        FakeSource,
        commits,
    )
}

/// Build the bounded file-backed Batch capability used by VCE lifecycle tests.
/// 构建 VCE 生命周期测试使用的有界文件型 Batch 能力。
fn fake_file_batch_capabilities() -> VceEmbeddingBatchCapabilities {
    VceEmbeddingBatchCapabilities {
        transport: VceEmbeddingBatchTransport::JsonlFile,
        provider_maximum_requests: Some(50_000),
        provider_maximum_payload_bytes: Some(500 * 1024 * 1024),
        provider_maximum_item_bytes: Some(1_000_000),
        provider_maximum_parallel_tasks: Some(1_000),
        host_policy_maximum_tasks_per_run: 1_000,
        host_policy_maximum_requests: 20,
        host_policy_maximum_payload_bytes: 500 * 1024 * 1024,
        host_policy_maximum_result_bytes: 64 * 1024 * 1024,
        provider_completion_window_seconds: Some(86_400),
        maximum_requests: 20,
        maximum_payload_bytes: 500 * 1024 * 1024,
        maximum_result_bytes: 64 * 1024 * 1024,
        maximum_tasks_per_run: 1_000,
        provider_maximum_files: Some(10_000),
        provider_maximum_total_file_bytes: Some(100 * 1024 * 1024 * 1024),
        provider_file_upload_qps: Some(3),
        provider_file_operation_qps: Some(10),
        provider_task_create_rpm: Some(1_000),
        provider_task_status_rpm: Some(1_000),
        provider_task_list_rpm: Some(100),
        provider_task_cancel_rpm: Some(1_000),
        minimum_poll_interval_ms: 1,
        supports_cancel: true,
        deletes_input_file_after_ingestion: true,
    }
}

/// Verify provider QPS and RPM declarations produce conservative shared dispatch intervals.
/// 验证供应商 QPS 与 RPM 声明会生成保守的共享分派间隔。
#[test]
fn remote_batch_rate_intervals_apply_the_strictest_declared_limit() {
    let capabilities = fake_file_batch_capabilities();
    assert_eq!(remote_file_operation_interval_ms(&capabilities), 100);
    assert_eq!(remote_file_upload_interval_ms(&capabilities), 334);
    assert_eq!(remote_reconciliation_interval_ms(&capabilities), 600);
    assert_eq!(remote_submission_interval_ms(&capabilities), 334);
    assert_eq!(remote_status_interval_ms(&capabilities), 60);
    assert_eq!(remote_cancel_interval_ms(&capabilities), 60);
}

/// Verify restart recovery retains durable upload and submission clocks for provider throttling.
/// 验证重启恢复会为供应商限流保留持久上传与提交时钟。
#[test]
fn remote_batch_rate_clocks_resume_from_durable_records() {
    let capabilities = fake_file_batch_capabilities();
    let task = VceEmbeddingBatchTaskPlan {
        task_id: String::from("rate-clock-task"),
        first_input_index: 0,
        input_count: 1,
        encoded_payload_bytes: 128,
        payload_sha256: "b".repeat(64),
    };
    let uploaded_input = VceEmbeddingBatchUploadedInput {
        task_id: task.task_id.clone(),
        provider_input_file_id: String::from("file-rate-clock-task"),
        uploaded_at_unix_ms: 700,
    };
    let submission = VceEmbeddingBatchSubmission {
        task_id: task.task_id.clone(),
        provider_batch_id: String::from("provider-rate-clock-task"),
        provider_input_file_id: Some(uploaded_input.provider_input_file_id.clone()),
        submitted_at_unix_ms: 800,
    };
    let record = VceEmbeddingRemoteBatchRecord {
        remote_task_id: task.task_id.clone(),
        purpose: VceAiPurpose::CodeDocumentEmbedding.as_str().to_owned(),
        first_logical_batch_index: 0,
        logical_batch_count: 1,
        plan: VceEmbeddingBatchPlan {
            request_id: String::from("rate-clock-request"),
            request_fingerprint: "a".repeat(64),
            capabilities,
            tasks: vec![task],
        },
        state: VceEmbeddingRemoteBatchState::Submitted,
        uploaded_input: Some(uploaded_input),
        submission: Some(submission),
        snapshot: None,
        poll_attempt: 0,
        next_poll_not_before_unix_ms: Some(1_000),
        error: None,
        cleanup_warning: None,
        updated_at_unix_ms: 900,
    };

    assert_eq!(
        latest_remote_file_operation_unix_ms(std::slice::from_ref(&record)),
        Some(900)
    );
    assert_eq!(
        latest_remote_submission_unix_ms(std::slice::from_ref(&record)),
        Some(900)
    );

    let mut retry = record.clone();
    retry.state = VceEmbeddingRemoteBatchState::SubmissionFailed;
    retry.submission = None;
    retry.snapshot = None;
    retry.next_poll_not_before_unix_ms = None;
    retry.error = Some(String::from("retryable creation failure"));
    retry.updated_at_unix_ms = 1_100;
    assert_eq!(
        latest_remote_submission_unix_ms(&[record, retry]),
        Some(1_100)
    );
}

/// Verify remote request progress exposes the normalized provider state and exact settled count.
/// 验证远端请求进度会暴露归一化供应商状态及精确已结算数量。
#[test]
fn remote_request_progress_exposes_provider_state() {
    let observed = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&observed);
    let control = VceRefreshControl::default().with_observer(Arc::new(move |progress| {
        sink.lock().expect("progress sink").push(progress);
    }));
    let submission = VceEmbeddingBatchSubmission {
        task_id: String::from("task-progress"),
        provider_batch_id: String::from("batch-progress"),
        provider_input_file_id: None,
        submitted_at_unix_ms: 1,
    };
    let snapshot = VceEmbeddingBatchSnapshot {
        submission,
        state: VceEmbeddingBatchProviderState::Finalizing,
        total_requests: Some(10),
        completed_requests: Some(7),
        failed_requests: Some(1),
        output_file_id: None,
        error_file_id: None,
        error: None,
    };

    observe_remote_request_progress(
        &control,
        "run-progress",
        VceAiPurpose::CodeDocumentEmbedding,
        12,
        20,
        &snapshot,
        Some((6, 30)),
        Some(123_456),
    );

    let observed = observed.lock().expect("progress observations");
    assert_eq!(observed.len(), 1);
    assert_eq!(observed[0].unit, VceRefreshUnit::Requests);
    assert_eq!(observed[0].completed, 12);
    assert_eq!(observed[0].overall_completed, 6);
    assert_eq!(
        observed[0].activity.as_ref().unwrap().next_poll_at_unix_ms,
        Some(123_456)
    );
    assert_eq!(
        observed[0].provider_batch_state,
        Some(VceRefreshProviderBatchState::Finalizing)
    );
}

/// Build a valid multi-file workspace for staging-selection tests without model or storage access.
/// 为暂存选择测试构建有效多文件工作区，不访问模型或存储。
/// No parameters; returns an engine whose unique Blob ownership was checked by normal observation.
/// 无参数；返回通过普通观察校验唯一块归属的引擎。
fn staging_selection_engine() -> VceEngine {
    // Each observation exercises the real path and Blob identity rules.
    // 每次观察执行真实路径及块身份规则。
    let mut engine = test_engine();
    // Distinct paths make a full selection expose repeated scans of the same record table.
    // 不同路径使完整选择能够暴露对同一记录表的重复扫描。
    for index in 0..32 {
        engine
            .observe(format!("src/module_{index:02}.rs"), "fn entry() {}")
            .expect("valid staging source");
    }
    engine
}

/// Require one record traversal for full staging while preserving exact Blob order and borrowed ownership.
/// 要求全量暂存只遍历一次记录，同时保留精确块顺序及借用归属。
/// No parameters or return value; visit counts detect quadratic selection without timing assumptions.
/// 无参数及返回值；访问计数检测平方级选择，不依赖计时假设。
#[test]
fn staging_record_selection_visits_each_record_once() {
    // The engine supplies the same validated record iterator used by the production staging entry.
    // 引擎提供与生产暂存入口相同的已校验记录迭代器。
    let engine = staging_selection_engine();
    // Only present records' actual identities are admitted, without invented scope.
    // 仅准入存在记录的实际身份，不虚构范围。
    let blob_names = engine
        .workspace
        .records
        .values()
        .map(|record| record.blob_name.clone().expect("present Blob"))
        .collect::<BTreeSet<_>>();
    // A cloned iterator shares this counter, so restarting traversal cannot hide duplicate visits.
    // 克隆迭代器共享此计数，重启遍历不能隐藏重复访问。
    let visits = std::cell::Cell::new(0_usize);
    // Selection borrows source records and must never widen the requested set.
    // 选择借用源码记录，绝不能扩大请求集合。
    let selected = staging_records(
        engine
            .workspace
            .records
            .values()
            .inspect(|_| visits.set(visits.get() + 1)),
        &blob_names,
        &VceRefreshControl::default(),
    )
    .expect("select all staging records");
    assert_eq!(visits.get(), engine.workspace.records.len());
    assert_eq!(
        selected
            .iter()
            .map(|record| record.blob_name.as_ref().unwrap())
            .collect::<Vec<_>>(),
        blob_names.iter().collect::<Vec<_>>()
    );
    assert!(
        selected
            .iter()
            .all(|record| std::ptr::eq(*record, &engine.workspace.records[&record.path]))
    );
}

/// Preserve sparse selection and missing-identity errors while empty work performs no traversal.
/// 保留稀疏选择及缺失身份错误，同时空工作不执行遍历。
/// No parameters or return value; assertions use only real workspace identities and a proven absent identity.
/// 无参数及返回值；断言仅使用真实工作区身份及已证明缺失的身份。
#[test]
fn staging_record_selection_preserves_exact_scope_and_missing_error() {
    // The source workspace contains more records than this incremental selection requests.
    // 来源工作区包含的记录多于本次增量选择请求。
    let engine = staging_selection_engine();
    // Every seventh path forms a deterministic sparse selection.
    // 每隔七个路径形成确定性的稀疏选择。
    let blob_names = engine
        .workspace
        .records
        .values()
        .step_by(7)
        .map(|record| record.blob_name.clone().expect("present Blob"))
        .collect::<BTreeSet<_>>();
    // The control is reusable here because selection does not publish or settle a refresh.
    // 选择不发布或结算刷新，因此此处可以复用控制器。
    let control = VceRefreshControl::default();
    // Returned references must retain the exact admitted order and size.
    // 返回引用必须保留精确准入顺序及数量。
    let selected = staging_records(engine.workspace.records.values(), &blob_names, &control)
        .expect("sparse staging selection");
    assert_eq!(
        selected
            .iter()
            .map(|record| record.blob_name.as_ref().unwrap())
            .collect::<Vec<_>>(),
        blob_names.iter().collect::<Vec<_>>()
    );
    assert!(
        staging_records(
            engine
                .workspace
                .records
                .values()
                .inspect(|_| panic!("empty scope must not scan")),
            &BTreeSet::new(),
            &control,
        )
        .expect("empty selection")
        .is_empty()
    );
    // Prove this identity is absent before checking the exact existing missing-record diagnostic.
    // 检查精确既有缺失记录诊断前，先证明此身份不存在。
    let absent = crate::identity::content_hash("absent staging record fixture");
    assert!(
        engine
            .workspace
            .records
            .values()
            .all(|record| record.blob_name.as_ref() != Some(&absent))
    );
    assert!(matches!(staging_records(engine.workspace.records.values(),
            &BTreeSet::from([absent.clone()]), &control),
            Err(VceError::InvalidRequest(message))
                if message == format!("workspace has no present record for added Blob `{absent}`")));
}

/// Honor cancellation both before traversal and inside a sparse scan without returning partial selection.
/// 在遍历前及稀疏扫描中遵守取消，不返回局部选择结果。
/// No parameters or return value; iterator observation provides a deterministic in-scan cancellation point.
/// 无参数及返回值；迭代器观察提供确定性的扫描中取消点。
#[test]
fn staging_record_selection_cancels_before_and_during_scan() {
    // Request the last path so earlier records must be examined before finding the requested identity.
    // 请求最后一个路径，使查找请求身份之前必须检查更早的记录。
    let engine = staging_selection_engine();
    // The target comes from the authoritative path-ordered record table.
    // 目标来自权威的路径有序记录表。
    let blob_names = BTreeSet::from([engine
        .workspace
        .records
        .values()
        .next_back()
        .expect("last source")
        .blob_name
        .clone()
        .expect("present Blob")]);
    // Pre-cancelled work may not read even one source record.
    // 预先取消的工作不能读取哪怕一条源码记录。
    let cancelled = VceRefreshControl::default();
    assert_eq!(cancelled.cancel(), crate::VceCancelOutcome::Accepted);
    assert!(matches!(
        staging_records(
            engine
                .workspace
                .records
                .values()
                .inspect(|_| panic!("cancelled selection must not scan")),
            &blob_names,
            &cancelled,
        ),
        Err(VceError::RefreshCancelled)
    ));
    // This independent job cancels immediately after the first record reaches selection.
    // 此独立作业在第一条记录到达选择阶段后立即取消。
    let during_scan = VceRefreshControl::default();
    // Count exactly how far the input advanced before cancellation was returned.
    // 精确统计返回取消之前输入推进了多远。
    let visits = std::cell::Cell::new(0_usize);
    assert!(matches!(
        staging_records(
            engine.workspace.records.values().inspect(|_| {
                visits.set(visits.get() + 1);
                during_scan.cancel();
            }),
            &blob_names,
            &during_scan,
        ),
        Err(VceError::RefreshCancelled)
    ));
    assert_eq!(visits.get(), 1);
}

/// Verify invalid Embedding planning budgets fail before the engine can call a host.
/// 验证非法向量规划预算会在引擎调用宿主前失败。
#[test]
fn rejects_inconsistent_indexing_budgets() {
    let mut config = VceConfig::new("code-chain", test_space()).unwrap();
    config.indexing.embedding_max_segment_chars = 8;
    config.indexing.embedding_overlap_chars = 8;

    assert!(matches!(
        VceEngine::new(config),
        Err(VceConfigError::InvalidIndexingConfig)
    ));
}

/// Verify a full refresh uses one atomic commit and shared Embeddings.
/// 验证完整刷新使用一次原子提交和共享向量调用。
#[test]
fn refresh_is_a_functional_entry() {
    let mut engine = test_engine();
    let (ai, index, source, commits) = fake_host();
    let result = block_on(engine.refresh(
        RefreshRequest::full(
            "refresh-1",
            RefreshReason::Initial,
            vec![crate::workspace::FileObservation::present(
                "src/lib.rs",
                "fn main() {}",
                Some(1),
                crate::model::FileSource::Filesystem,
            )],
            Some("git-1".to_owned()),
        ),
        &index,
        &ai,
        &source,
    ))
    .unwrap();
    assert_eq!(result.generation, 1);
    assert_eq!(result.indexed_blobs.len(), 1);
    let committed = commits.lock().unwrap();
    assert_eq!(committed.len(), 1);
    assert_eq!(committed[0].state_snapshot, engine.state_snapshot());
    assert_eq!(engine.status().state, VceEngineState::Ready);
}

/// Verify one malformed structural Blob is recorded without aborting sibling batches.
/// 验证单个结构异常 Blob 会被记录且不会中止其他批次。
#[test]
fn structural_analysis_failure_isolated_to_one_blob() {
    let mut engine = test_engine();
    engine.config.index_stage_blob_limit = 1;
    engine.structure_analyzer = Box::new(DuplicateSymbolAnalyzer);
    let (ai, index, source, commits) = fake_host();
    let result = block_on(engine.refresh(
        RefreshRequest::full(
            "refresh-structure-isolation",
            RefreshReason::Initial,
            vec![
                crate::workspace::FileObservation::present(
                    "src/good.rs",
                    "fn good() {}",
                    Some(1),
                    crate::model::FileSource::Filesystem,
                ),
                crate::workspace::FileObservation::present(
                    "src/bad.rs",
                    "fn bad() {}",
                    Some(1),
                    crate::model::FileSource::Filesystem,
                ),
            ],
            None,
        ),
        &index,
        &ai,
        &source,
    ))
    .expect("one structural failure must not fail the refresh");

    assert_eq!(result.indexed_blobs.len(), 2);
    assert_eq!(*index.abort_count.lock().unwrap(), 0);
    assert_eq!(commits.lock().unwrap().len(), 1);
    let staged = index.staged.lock().unwrap();
    assert_eq!(staged.len(), 2);
    assert!(staged.iter().any(|blob| {
        blob.path == "src/bad.rs"
            && matches!(blob.status, IndexedBlobStatus::ReadyWithoutEmbedding { .. })
            && matches!(blob.symbol_status, AuxiliaryIndexStatus::Failed { .. })
    }));
    assert!(staged.iter().any(|blob| {
        blob.path == "src/good.rs" && matches!(blob.status, IndexedBlobStatus::Ready)
    }));
}

/// Verify cancellation after acknowledged staging aborts unpublished work and never commits.
/// 验证暂存确认后的取消中止未发布工作且绝不提交。
///
/// Takes no parameters and returns nothing; the observer deterministically cancels at a real phase.
/// 无参数、无返回值；观察器在真实阶段确定性取消。
#[test]
fn observed_refresh_cancels_after_staging_without_publication() {
    // engine and host are isolated deterministic ports; no external model is called.
    // engine 与 host 为隔离确定性端口；不调用外部模型。
    let mut engine = test_engine();
    let (ai, index, source, commits) = fake_host();
    // cancellation shares state without retaining the observer, avoiding an ownership cycle.
    // cancellation 共享状态但不持有观察器，避免归属循环。
    let cancellation = VceRefreshControl::default();
    // requested is the observer's exact cancellation handle.
    // requested 是观察器的精确取消句柄。
    let requested = cancellation.clone();
    // control cancels only after a stage receipt has been validated.
    // control 仅在暂存回执校验后取消。
    let control = cancellation.with_observer(Arc::new(move |progress: VceRefreshProgress| {
        if progress.phase == VceRefreshPhase::Writing && progress.completed > 0 {
            assert_eq!(requested.cancel(), crate::VceCancelOutcome::Accepted);
        }
    }));
    // result must classify cancellation separately from malformed provider results.
    // result 必须将取消与错误供应商结果分开分类。
    let result = block_on(engine.refresh_observed(
        RefreshRequest::full(
            "cancel-staged",
            RefreshReason::Initial,
            vec![FileObservation::present(
                "src/lib.rs",
                "fn main() {}",
                Some(1),
                crate::FileSource::Filesystem,
            )],
            None,
        ),
        &index,
        &ai,
        &source,
        &control,
    ));
    assert!(matches!(result, Err(VceError::RefreshCancelled)));
    assert!(commits.lock().unwrap().is_empty());
    assert_eq!(*index.abort_count.lock().unwrap(), 1);
    assert!(engine.status().checkpoint_version.is_none());
}

/// Verify every source advances preparation once across staging batches, including empty and oversized files.
/// 验证每个源码跨暂存批次仅推进一次准备计数，包括空文件及超限文件。
/// Takes no input; asserts a frozen denominator and cumulative counts from the real pipeline.
/// 无输入；断言真实流水线中的冻结分母与累计计数。
#[test]
fn observed_preparation_counts_all_files_across_batches() {
    // config forces one source per batch and filters the oversized source before embedding.
    // config 强制每批一个源码，并在向量计算前过滤超限源码。
    let mut config = VceConfig::new("code-chain", test_space()).unwrap();
    config.index_stage_blob_limit = 1;
    config.indexing.max_file_bytes = 64;
    // engine and host exercise real preparation without network or user storage.
    // engine 与 host 执行真实准备，不访问网络或用户存储。
    let mut engine = VceEngine::new(config).unwrap();
    let (ai, index, source, _) = fake_host();
    // events retains actual observations for ordering and completeness assertions.
    // events 保留实际观察，用于顺序与完整性断言。
    let events = Arc::new(Mutex::new(Vec::new()));
    // sink receives only the immutable observations emitted by this refresh.
    // sink 仅接收本次刷新发出的不可变观察。
    let sink = Arc::clone(&events);
    // control records the pipeline without altering cancellation or provider behavior.
    // control 记录流水线，不改变取消或供应商行为。
    let control = VceRefreshControl::default().with_observer(Arc::new(move |progress| {
        sink.lock().unwrap().push(progress);
    }));
    block_on(engine.refresh_observed(
        RefreshRequest::full(
            "prepare-files",
            RefreshReason::Initial,
            vec![
                FileObservation::present(
                    "src/a.rs",
                    "fn alpha() {}",
                    Some(1),
                    crate::FileSource::Filesystem,
                ),
                FileObservation::present(
                    "src/empty.rs",
                    "",
                    Some(1),
                    crate::FileSource::Filesystem,
                ),
                FileObservation::present(
                    "src/large.rs",
                    "x".repeat(65),
                    Some(1),
                    crate::FileSource::Filesystem,
                ),
            ],
            None,
        ),
        &index,
        &ai,
        &source,
        &control,
    ))
    .expect("complete isolated refresh");
    // observed excludes embedding and publication, which use different explicit counting units.
    // observed 排除向量计算与发布，它们使用不同的明确计数单位。
    let observed = events.lock().unwrap();
    // preparation retains only the cumulative source-file stage in emission order.
    // preparation 仅按发出顺序保留累计源码文件阶段。
    let preparation = observed
        .iter()
        .filter(|progress| progress.phase == VceRefreshPhase::Chunking)
        .collect::<Vec<_>>();
    assert_eq!(
        preparation
            .iter()
            .map(|progress| progress.completed)
            .collect::<Vec<_>>(),
        vec![0, 1, 1, 2, 2, 3]
    );
    assert!(preparation.iter().all(|progress| progress.total == Some(3)
        && progress.operation_id == "prepare-files:prepare"
        && progress.current_path.is_some()));
    assert_eq!(
        preparation
            .iter()
            .filter_map(|progress| progress.current_path.as_deref())
            .collect::<BTreeSet<_>>(),
        BTreeSet::from(["src/a.rs", "src/empty.rs", "src/large.rs"])
    );
    assert!(observed.iter().any(|progress| {
        progress
            .activity
            .as_ref()
            .is_some_and(|activity| activity.stage == VceEmbeddingActivityStage::Persisting)
    }));
}

/// Verify cancellation at both chunking boundaries prevents embedding, staging, and publication.
/// 验证在切块前后两个边界取消会阻止向量调用、暂存与发布。
/// Takes no parameters and returns nothing; each boundary uses a fresh engine and isolated ports.
/// 无参数、无返回值；每个边界使用全新引擎和隔离端口。
#[test]
fn observed_refresh_cancels_at_both_chunking_boundaries_without_embedding() {
    // The real pipeline emits zero before preparing a file and one after preparing its chunks.
    // 真实流水线在准备文件前报告零，在准备好切块后报告一。
    for completed_boundary in [0_u64, 1_u64] {
        // Each engine has no published checkpoint that could hide a cancelled initial build.
        // 每个引擎均无已发布检查点，避免掩盖被取消的初始化构建。
        let mut engine = test_engine();
        // All ports record real calls made by this in-memory pipeline without external requests.
        // 全部端口记录此内存流水线实际发出的调用，不发送外部请求。
        let (ai, index, source, commits) = fake_host();
        // Record the matched event so a missing chunking callback cannot pass accidentally.
        // 记录匹配事件，避免缺失切块回调时测试意外通过。
        let matched_events = Arc::new(Mutex::new(0_usize));
        // The observer owns only a clone of the event counter and the observer-free control.
        // 观察器仅持有事件计数器副本以及不含观察器的控制句柄。
        let event_counter = Arc::clone(&matched_events);
        // Cancellation state is shared with the exact refresh being exercised.
        // 取消状态与被测试的精确刷新共享。
        let cancellation = VceRefreshControl::default();
        // The observer-free handle avoids an ownership cycle through its own callback.
        // 不含观察器的句柄避免通过自身回调形成归属循环。
        let requested = cancellation.clone();
        // Cancel only at the requested real file boundary, checking its progress identity.
        // 仅在指定的真实文件边界取消，同时核对进度身份。
        let control = cancellation.with_observer(Arc::new(move |progress: VceRefreshProgress| {
            if progress.phase == VceRefreshPhase::Chunking
                && progress.completed == completed_boundary
            {
                assert_eq!(progress.total, Some(1));
                assert!(progress.current_path.is_some());
                *event_counter.lock().unwrap() += 1;
                assert_eq!(requested.cancel(), crate::VceCancelOutcome::Accepted);
            }
        }));
        // The nonempty source must normally require embedding after chunk preparation.
        // 非空源码在正常完成切块准备后必须需要向量计算。
        let result = block_on(engine.refresh_observed(
            RefreshRequest::full(
                format!("cancel-chunking-{completed_boundary}"),
                RefreshReason::Initial,
                vec![FileObservation::present(
                    "src/lib.rs",
                    "fn main() {}",
                    Some(1),
                    crate::FileSource::Filesystem,
                )],
                None,
            ),
            &index,
            &ai,
            &source,
            &control,
        ));
        assert_eq!(*matched_events.lock().unwrap(), 1);
        assert!(matches!(result, Err(VceError::RefreshCancelled)));
        assert!(ai.embedding_operation_ids.lock().unwrap().is_empty());
        assert_eq!(*index.stage_call_count.lock().unwrap(), 0);
        assert_eq!(*index.abort_count.lock().unwrap(), 1);
        assert!(commits.lock().unwrap().is_empty());
        assert!(engine.status().checkpoint_version.is_none());
    }
}

/// Verify real embedding counters and cancellation refusal after the commit boundary wins.
/// 验证真实向量计数以及提交边界获胜后的取消拒绝。
///
/// Takes no parameters and returns nothing; all observations are collected from real fake-port receipts.
/// 无参数、无返回值；全部观察来自模拟端口的实际回执。
#[test]
fn observed_refresh_reports_validated_progress_and_protects_commit() {
    // engine and host execute an actual in-memory refresh pipeline.
    // engine 与 host 执行实际内存刷新流水线。
    let mut engine = test_engine();
    let (ai, index, source, commits) = fake_host();
    // events retains progress for assertions after the task has finished.
    // events 保留进度以供任务结束后断言。
    let events = Arc::new(Mutex::new(Vec::new()));
    // sink and cancellation belong only to this test job.
    // sink 与 cancellation 仅属于此测试作业。
    let sink = Arc::clone(&events);
    let cancellation = VceRefreshControl::default();
    let requested = cancellation.clone();
    let control = cancellation.with_observer(Arc::new(move |progress: VceRefreshProgress| {
        if progress.phase == VceRefreshPhase::Committing {
            assert_eq!(requested.cancel(), crate::VceCancelOutcome::Committing);
        }
        if progress.phase == VceRefreshPhase::Complete {
            assert_eq!(requested.cancel(), crate::VceCancelOutcome::Finished);
        }
        sink.lock().unwrap().push(progress);
    }));
    block_on(engine.refresh_observed(
        RefreshRequest::full(
            "observe-success",
            RefreshReason::Initial,
            vec![FileObservation::present(
                "src/lib.rs",
                "fn main() {}",
                Some(1),
                crate::FileSource::Filesystem,
            )],
            None,
        ),
        &index,
        &ai,
        &source,
        &control,
    ))
    .expect("observed refresh");
    assert_eq!(commits.lock().unwrap().len(), 1);
    // observed snapshots must have honest denominators and acknowledged completion counters.
    // observed 快照必须具有真实分母与已确认完成计数。
    let observed = events.lock().unwrap();
    assert!(
        observed
            .iter()
            .any(|progress| progress.phase == VceRefreshPhase::Embedding
                && progress.completed > 0
                && progress.total == Some(progress.completed))
    );
    assert!(observed.iter().all(|progress| {
        progress
            .total
            .is_none_or(|total| progress.completed <= total)
    }));
    assert_eq!(observed.last().unwrap().phase, VceRefreshPhase::Complete);
}

/// Verify no-change refresh keeps its checkpoint, honors cancellation, and reports settled success.
/// 验证无变化刷新保留检查点、遵守取消，并报告已结算成功。
/// Takes no parameters and returns nothing; uses isolated ports without external model requests.
/// 无参数、无返回值；使用隔离端口，不请求外部模型。
#[test]
fn no_change_refresh_preserves_checkpoint_and_settles_control() {
    // engine and host initialize one empty checkpoint without embedding work.
    // engine 与 host 初始化一个空检查点，不产生向量工作。
    let mut engine = test_engine();
    let (ai, index, source, commits) = fake_host();
    block_on(engine.refresh(
        RefreshRequest::full("empty-initial", RefreshReason::Initial, Vec::new(), None),
        &index,
        &ai,
        &source,
    ))
    .expect("initial empty checkpoint");
    // checkpoint is the immutable identity that both subsequent attempts must preserve.
    // checkpoint 是后续两次尝试都必须保留的不可变身份。
    let checkpoint = engine.status().checkpoint_version;

    // cancellation is accepted before reconciliation and must prevent successful completion.
    // cancellation 在核对前受理，必须阻止成功完成。
    let cancellation = VceRefreshControl::default();
    let requested = cancellation.clone();
    let cancelled = cancellation.with_observer(Arc::new(move |progress: VceRefreshProgress| {
        if progress.phase == VceRefreshPhase::Reconciling {
            assert_eq!(requested.cancel(), crate::VceCancelOutcome::Accepted);
        }
    }));
    assert!(matches!(
        block_on(engine.refresh_observed(
            RefreshRequest::full("empty-cancelled", RefreshReason::Manual, Vec::new(), None),
            &index,
            &ai,
            &source,
            &cancelled,
        )),
        Err(VceError::RefreshCancelled)
    ));
    assert_eq!(engine.status().checkpoint_version, checkpoint);

    // success must already be terminal when its complete event becomes observable.
    // success 在完成事件可观察时必须已经处于终态。
    let success = VceRefreshControl::default();
    let late_cancel = success.clone();
    let control = success.with_observer(Arc::new(move |progress: VceRefreshProgress| {
        if progress.phase == VceRefreshPhase::Complete {
            assert_eq!(late_cancel.cancel(), crate::VceCancelOutcome::Finished);
        }
    }));
    // result confirms no extra publication or model work is introduced by the atomic settlement.
    // result 确认原子结算没有引入额外发布或模型工作。
    let result = block_on(engine.refresh_observed(
        RefreshRequest::full("empty-success", RefreshReason::Manual, Vec::new(), None),
        &index,
        &ai,
        &source,
        &control,
    ))
    .expect("successful no-change refresh");
    assert!(result.commit_receipt.is_none());
    assert!(result.indexed_blobs.is_empty());
    assert_eq!(result.checkpoint_version, checkpoint);
    assert_eq!(commits.lock().unwrap().len(), 1);
    assert!(ai.embedding_operation_ids.lock().unwrap().is_empty());
    assert_eq!(engine.status().state, VceEngineState::Ready);
    engine
        .state_snapshot()
        .validate()
        .expect("valid settled snapshot");
}

/// Measure a deterministic offline baseline without asserting machine-specific latency.
/// 测量确定性离线基线，但不对机器相关延迟作断言。
#[test]
#[ignore = "run explicitly when recording the VCE performance baseline"]
fn records_offline_performance_baseline() {
    let mut engine = test_engine();
    let (ai, index, source, _) = fake_host();
    let embedding_operation_ids = Arc::clone(&ai.embedding_operation_ids);
    let rewrite_call_count = Arc::clone(&ai.rewrite_call_count);
    let rerank_candidate_counts = Arc::clone(&ai.rerank_candidate_counts);
    let observations = (0..128)
            .map(|index| {
                let path = format!("src/module_{index:03}.rs");
                let content = (0..12)
                    .map(|function| {
                        format!(
                            "pub fn module_{index:03}_function_{function:02}() -> usize {{ {function} }}\n"
                        )
                    })
                    .collect::<String>();
                crate::workspace::FileObservation::present(
                    path,
                    content,
                    Some(index as u128 + 1),
                    crate::model::FileSource::Filesystem,
                )
            })
            .collect::<Vec<_>>();

    let initial_started = std::time::Instant::now();
    let initial = block_on(engine.refresh(
        RefreshRequest::full(
            "performance-initial",
            RefreshReason::Initial,
            observations,
            Some("baseline-1".to_owned()),
        ),
        &index,
        &ai,
        &source,
    ))
    .unwrap();
    let initial_elapsed = initial_started.elapsed();
    let initial_embedding_calls = embedding_operation_ids.lock().unwrap().len();
    let retained_source_bytes = engine
        .workspace_snapshot()
        .records
        .iter()
        .filter_map(|record| record.content.as_ref())
        .map(String::len)
        .sum::<usize>();
    let state_snapshot_bytes = serde_json::to_vec(&engine.state_snapshot()).unwrap().len();

    let changed_content = (0..12)
        .map(|function| {
            format!(
                "pub fn module_000_function_{function:02}() -> usize {{ {} }}\n",
                function + 1
            )
        })
        .collect::<String>();
    let incremental_started = std::time::Instant::now();
    let incremental = block_on(engine.refresh(
        RefreshRequest::paths(
            "performance-incremental",
            RefreshReason::FileWatcher,
            vec![crate::workspace::FileObservation::present(
                "src/module_000.rs",
                changed_content,
                Some(10_000),
                crate::model::FileSource::Filesystem,
            )],
            Vec::new(),
            Some("baseline-2".to_owned()),
        ),
        &index,
        &ai,
        &source,
    ))
    .unwrap();
    let incremental_elapsed = incremental_started.elapsed();
    let incremental_embedding_calls = embedding_operation_ids
        .lock()
        .unwrap()
        .len()
        .saturating_sub(initial_embedding_calls);

    let retrieval_started = std::time::Instant::now();
    let retrieval = block_on(engine.retrieve(
        RetrieveRequest::current_checkpoint("performance-retrieval", "find module function"),
        &index,
        &ai,
    ))
    .unwrap();
    let retrieval_elapsed = retrieval_started.elapsed();

    assert_eq!(initial.indexed_blobs.len(), 128);
    assert_eq!(incremental.indexed_blobs.len(), 1);
    assert_eq!(incremental.deleted_blobs.len(), 1);
    assert!(incremental_embedding_calls < initial_embedding_calls);
    assert_eq!(retained_source_bytes, 0);
    assert_eq!(*rewrite_call_count.lock().unwrap(), 0);
    assert_eq!(rerank_candidate_counts.lock().unwrap().len(), 1);
    assert!(!retrieval.hits.is_empty());

    println!(
        "VCE_PERF initial_ms={} incremental_ms={} retrieval_us={} initial_embedding_calls={} incremental_embedding_calls={} query_rewrite_calls=0 rerank_calls=1 state_snapshot_bytes={} retained_source_bytes={}",
        initial_elapsed.as_millis(),
        incremental_elapsed.as_millis(),
        retrieval_elapsed.as_micros(),
        initial_embedding_calls,
        incremental_embedding_calls,
        state_snapshot_bytes,
        retained_source_bytes,
    );
}

/// Verify one refresh freezes AI capabilities once across content and path Embeddings.
/// 验证一次刷新会跨内容与路径向量调用仅冻结一次 AI 能力。
#[test]
fn refresh_reads_one_frozen_ai_capability_snapshot() {
    let mut engine = test_engine();
    let (ai, index, source, _) = fake_host();
    let capability_snapshot_call_count = ai.capability_snapshot_call_count.clone();

    block_on(engine.refresh(
        RefreshRequest::full(
            "refresh-frozen-capabilities",
            RefreshReason::Initial,
            vec![crate::workspace::FileObservation::present(
                "src/lib.rs",
                "fn main() {}",
                Some(1),
                crate::model::FileSource::Filesystem,
            )],
            None,
        ),
        &index,
        &ai,
        &source,
    ))
    .unwrap();

    assert_eq!(*capability_snapshot_call_count.lock().unwrap(), 1);
}

/// Verify every Embedding request respects the lower of configuration and model limits.
/// 验证每个向量请求都遵守配置上限与模型上限中较小的值。
#[test]
fn embedding_batches_are_capped_by_model_contract() {
    let mut engine = test_engine();
    let (mut ai, index, source, _) = fake_host();
    ai.embedding_max_inputs = Some(2);
    let batch_sizes = ai.embedding_batch_sizes.clone();
    // progress_events retains both child-batch and whole-refresh counters for unit validation.
    // progress_events 保留子批次与完整刷新计数，用于校验计量单位。
    let progress_events = Arc::new(Mutex::new(Vec::<VceRefreshProgress>::new()));
    // progress_sink is the observer-owned reference copied into the refresh control.
    // progress_sink 是复制进刷新控制器、由观察器持有的引用。
    let progress_sink = Arc::clone(&progress_events);
    // control observes the real durable global batching path exercised below.
    // control 观察下方实际执行的持久化全局分批路径。
    let control = VceRefreshControl::default().with_observer(Arc::new(move |progress| {
        progress_sink.lock().unwrap().push(progress);
    }));
    let content = (0..128)
        .map(|index| format!("fn function_{index:03}() -> usize {{ {index} }}\n"))
        .collect::<String>();

    block_on(engine.refresh_observed(
        RefreshRequest::full(
            "refresh-model-batch-limit",
            RefreshReason::Initial,
            vec![crate::workspace::FileObservation::present(
                "src/batched.rs",
                content.clone(),
                Some(content.len() as u128),
                crate::model::FileSource::Filesystem,
            )],
            None,
        ),
        &index,
        &ai,
        &source,
        &control,
    ))
    .unwrap();

    let observed = batch_sizes.lock().unwrap().clone();
    assert!(
        observed.len() > 1,
        "the fixture must produce multiple requests"
    );
    assert!(observed.iter().all(|size| *size <= 2));
    // embedding_progress isolates provider-batch events from later Blob write acknowledgements.
    // embedding_progress 将供应商批次事件与后续 Blob 落盘确认分离。
    let embedding_progress = progress_events
        .lock()
        .unwrap()
        .iter()
        .filter(|progress| progress.phase == VceRefreshPhase::Embedding)
        .cloned()
        .collect::<Vec<_>>();
    assert!(embedding_progress.len() > 1);
    assert!(
        embedding_progress.iter().all(|progress| {
            progress.overall_completed == 0 && progress.overall_total == Some(1)
        })
    );
    assert!(progress_events.lock().unwrap().iter().all(|progress| {
        progress
            .overall_total
            .is_none_or(|total| progress.overall_completed <= total)
    }));
}

/// Verify separate source files share one provider batch within a bounded staging flush.
/// 验证同一有界暂存批次内的不同源码文件会共享一个供应商向量批次。
#[test]
fn embedding_requests_accumulate_across_files() {
    let mut engine = test_engine();
    let (ai, index, source, _) = fake_host();
    let operation_ids = ai.embedding_operation_ids.clone();
    let batch_sizes = ai.embedding_batch_sizes.clone();
    let first = "fn first() -> usize { 1 }";
    let second = "fn second() -> usize { 2 }";

    block_on(engine.refresh(
        RefreshRequest::full(
            "refresh-cross-file-batch",
            RefreshReason::Initial,
            vec![
                crate::workspace::FileObservation::present(
                    "src/first.rs",
                    first,
                    Some(first.len() as u128),
                    crate::model::FileSource::Filesystem,
                ),
                crate::workspace::FileObservation::present(
                    "src/second.rs",
                    second,
                    Some(second.len() as u128),
                    crate::model::FileSource::Filesystem,
                ),
            ],
            None,
        ),
        &index,
        &ai,
        &source,
    ))
    .unwrap();

    let operations = operation_ids.lock().unwrap().clone();
    let sizes = batch_sizes.lock().unwrap().clone();
    let document_sizes = operations
        .iter()
        .zip(sizes.iter())
        .filter_map(|(operation, size)| {
            // The durable global plan identifies content batches by their stable channel segment.
            // 持久化全局计划使用稳定的通道片段标识内容批次。
            operation.contains(":content:").then_some(*size)
        })
        .collect::<Vec<_>>();
    assert_eq!(document_sizes.len(), 1);
    assert!(document_sizes[0] >= 2);
}

/// Verify a no-op filesystem refresh does not retain committed source text in memory.
/// 验证无变化的文件系统刷新不会在内存中保留已提交源码正文。
#[test]
fn no_op_refresh_releases_committed_filesystem_content() {
    let mut engine = test_engine();
    let (ai, index, source, _) = fake_host();
    let observation = crate::workspace::FileObservation::present(
        "src/lib.rs",
        "fn main() {}",
        Some(1),
        crate::model::FileSource::Filesystem,
    );
    block_on(engine.refresh(
        RefreshRequest::full(
            "refresh-release-initial",
            RefreshReason::Initial,
            vec![observation.clone()],
            None,
        ),
        &index,
        &ai,
        &source,
    ))
    .unwrap();

    let second = block_on(engine.refresh(
        RefreshRequest::full(
            "refresh-release-no-op",
            RefreshReason::Manual,
            vec![observation],
            None,
        ),
        &index,
        &ai,
        &source,
    ))
    .unwrap();

    assert!(second.commit_receipt.is_none());
    assert!(
        engine
            .workspace_snapshot()
            .records
            .iter()
            .all(|record| record.content.is_none())
    );
}

/// Verify an explicit full rebuild reindexes unchanged Blobs and publishes a new checkpoint.
/// 验证明示完整重建会重新索引未变化 Blob 并发布新检查点。
#[test]
fn forced_full_rebuild_reindexes_unchanged_blobs() {
    let mut engine = test_engine();
    let (ai, index, source, commits) = fake_host();
    let stage_call_count = Arc::clone(&index.stage_call_count);
    let observation = crate::workspace::FileObservation::present(
        "src/lib.rs",
        "fn main() {}",
        Some(1),
        crate::model::FileSource::Filesystem,
    );
    block_on(engine.refresh(
        RefreshRequest::full(
            "refresh-before-rebuild",
            RefreshReason::Initial,
            vec![observation.clone()],
            None,
        ),
        &index,
        &ai,
        &source,
    ))
    .unwrap();

    let rebuilt = block_on(engine.refresh(
        RefreshRequest::full_rebuild(
            "refresh-rebuild",
            RefreshReason::Manual,
            vec![observation],
            None,
        ),
        &index,
        &ai,
        &source,
    ))
    .unwrap();

    assert_eq!(rebuilt.indexed_blobs.len(), 1);
    assert_eq!(rebuilt.checkpoint_version, Some(2));
    assert_eq!(*stage_call_count.lock().unwrap(), 2);
    assert_eq!(commits.lock().unwrap().len(), 2);
    // A permissive fake commit must not hide an invalid snapshot rejected by real storage.
    // 宽松的模拟提交不能掩盖真实存储会拒绝的无效快照。
    let state = engine.state_snapshot();
    state
        .validate()
        .expect("unchanged full rebuild has valid state");
    assert_eq!(
        state
            .last_refresh
            .as_ref()
            .expect("rebuild summary")
            .added_blob_count,
        0
    );
    assert_eq!(
        VceStateSnapshot::from_json(&state.to_json().expect("encode rebuilt state"))
            .expect("decode rebuilt state"),
        state
    );
    for commit in commits.lock().unwrap().iter() {
        commit
            .state_snapshot
            .validate()
            .expect("prospective committed state validates");
    }
    // Bounds still reject fabricated work, and a path-only refresh cannot masquerade as a rebuild.
    // 边界仍拒绝虚构工作量，且仅路径刷新不能冒充重建。
    let mut excessive = state.clone();
    excessive
        .last_refresh
        .as_mut()
        .expect("rebuild summary")
        .indexed_blob_count = 2;
    assert!(excessive.validate().is_err());
    let mut invalid_path = state;
    invalid_path
        .last_refresh
        .as_mut()
        .expect("rebuild summary")
        .mode = RefreshMode::Paths;
    assert!(invalid_path.validate().is_err());
}

/// Verify initial VCE construction uses durable remote Batch lifecycle instead of synchronous calls.
/// 验证 VCE 首次构建使用持久远端 Batch 生命周期而不是同步调用。
#[tokio::test]
async fn initial_build_uses_file_backed_remote_batch_lifecycle() {
    let mut engine = test_engine();
    let (mut ai, index, source, commits) = fake_host();
    ai.embedding_batch_capabilities = Some(fake_file_batch_capabilities());
    let events = Arc::clone(&ai.embedding_batch_events);
    let synchronous_sizes = Arc::clone(&ai.embedding_batch_sizes);
    // Record actual lifecycle observations alongside the fake provider's transport calls.
    // 在伪供应商传输调用旁记录真实生命周期观察。
    let progress_events = Arc::new(Mutex::new(Vec::<VceRefreshProgress>::new()));
    // The observer owns only its shared event buffer.
    // 观察器仅持有共享事件缓冲区。
    let observed_events = Arc::clone(&progress_events);
    // Capture each state before the corresponding awaited provider boundary.
    // 在相应供应商等待边界之前捕获每个状态。
    let control = VceRefreshControl::default().with_observer(Arc::new(move |progress| {
        observed_events.lock().unwrap().push(progress);
    }));

    let result = engine
        .refresh_observed(
            RefreshRequest::full(
                "remote-initial-build",
                RefreshReason::Initial,
                vec![FileObservation::present(
                    "src/lib.rs",
                    "fn main() {}",
                    Some(1),
                    crate::model::FileSource::Filesystem,
                )],
                None,
            ),
            &index,
            &ai,
            &source,
            &control,
        )
        .await
        .expect("remote initial build");

    assert_eq!(result.indexed_blobs.len(), 1);
    assert_eq!(commits.lock().unwrap().len(), 1);
    assert!(synchronous_sizes.lock().unwrap().is_empty());
    // Initial Batch work must expose upload, submission, polling and collection without claiming a direct call.
    // 首次 Batch 工作必须展示上传、提交、轮询与取回，不能声称是同步调用。
    let activities = progress_events
        .lock()
        .unwrap()
        .iter()
        .filter_map(|progress| progress.activity.clone())
        .collect::<Vec<_>>();
    for stage in [
        VceEmbeddingActivityStage::Preparing,
        VceEmbeddingActivityStage::PlanningTasks,
        VceEmbeddingActivityStage::Admitting,
        VceEmbeddingActivityStage::Uploading,
        VceEmbeddingActivityStage::Submitting,
        VceEmbeddingActivityStage::Waiting,
        VceEmbeddingActivityStage::Polling,
        VceEmbeddingActivityStage::Collecting,
    ] {
        assert!(
            activities.iter().any(|activity| activity.stage == stage),
            "missing activity {stage:?}"
        );
    }
    assert!(
        !activities
            .iter()
            .any(|activity| activity.stage == VceEmbeddingActivityStage::Requesting)
    );
    // Provider polling cannot claim any completed logical batch before durable results exist.
    // 持久结果尚不存在时，供应商状态查询不得宣称逻辑批次已完成。
    for purpose in ["content", "path"] {
        // The operation isolates the content or path request observed by this assertion.
        // 操作标识限定本断言观察的内容或路径请求。
        let operation = format!("remote-initial-build:{purpose}");
        // The observer buffer is read without modifying durable state or provider behavior.
        // 只读取观察器缓冲，不修改持久状态或供应商行为。
        let progress = progress_events.lock().unwrap();
        // The first provider query precedes any result collection in this isolated lifecycle.
        // 本隔离生命周期中的首次供应商查询早于任何结果收集。
        let first_poll = progress
            .iter()
            .find(|event| {
                event.operation_id == operation
                    && event.activity.as_ref().is_some_and(|activity| {
                        activity.stage == VceEmbeddingActivityStage::Polling
                    })
            })
            .expect("provider status query must be observed");
        assert_eq!(first_poll.completed, 0);
        assert!(first_poll.total.is_some_and(|total| total > 0));
    }
    // Each purpose must finish observable local admission before its first upload begins.
    // 每个用途必须在首次上传前完成可观测的本地准入。
    for purpose in ["content", "path"] {
        // The frozen operation separates independent content and path plans.
        // 冻结操作区分独立的内容及路径计划。
        let operation = format!("remote-initial-build:{purpose}");
        // Events are captured directly at the engine boundary, before UI sampling.
        // 事件直接在引擎边界捕获，尚未经过界面采样。
        let progress = progress_events.lock().unwrap();
        // Locate the first upload without accepting a later purpose's admission.
        // 定位首次上传，不能接受其他用途后续产生的准入事件。
        let upload = progress
            .iter()
            .position(|event| {
                event.operation_id == operation
                    && event.activity.as_ref().is_some_and(|activity| {
                        activity.stage == VceEmbeddingActivityStage::Uploading
                    })
            })
            .unwrap();
        // Every durable admission reports a frozen denominator and confirmed completion.
        // 每次持久准入报告冻结分母及已确认完成数。
        let counts = progress[..upload]
            .iter()
            .filter(|event| {
                event.operation_id == operation
                    && event.activity.as_ref().is_some_and(|activity| {
                        activity.stage == VceEmbeddingActivityStage::Admitting
                    })
            })
            .map(|event| (event.completed, event.total))
            .collect::<Vec<_>>();
        assert_eq!(counts, vec![(0, Some(1)), (1, Some(1))]);
    }
    assert!(
        activities
            .iter()
            .filter(|activity| activity.stage == VceEmbeddingActivityStage::Waiting)
            .all(|activity| activity.provider_batch_id.is_some())
    );
    let events = events.lock().unwrap().clone();
    assert!(!events.is_empty());
    assert_eq!(
        events
            .iter()
            .filter(|event| event.starts_with("upload:"))
            .count(),
        2
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| event.starts_with("submit:"))
            .count(),
        2
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| event.starts_with("status:"))
            .count(),
        2
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| event.starts_with("collect:"))
            .count(),
        2
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| event.starts_with("cleanup:"))
            .count(),
        2
    );
    for record in index.remote_batch_records.lock().unwrap().values() {
        assert_eq!(record.state, VceEmbeddingRemoteBatchState::Succeeded);
        record.validate().expect("durable remote record");
    }
}

/// Verify one slow remote task cannot monopolize polling ahead of its submitted peers.
/// 验证一个缓慢远端任务不能在已提交兄弟任务之前独占轮询。
#[tokio::test]
async fn remote_batch_status_polling_visits_every_active_task_per_round() {
    let mut engine = test_engine();
    engine.config.indexing.embedding_max_inputs = 1;
    let (mut ai, index, source, _) = fake_host();
    let mut capabilities = fake_file_batch_capabilities();
    capabilities.host_policy_maximum_requests = 1;
    capabilities.maximum_requests = 1;
    ai.embedding_batch_capabilities = Some(capabilities);
    ai.batch_status_in_progress_once_per_task = true;
    *ai.batch_status_first_settled_counts.lock().unwrap() = VecDeque::from([1, 0]);
    let events = Arc::clone(&ai.embedding_batch_events);
    let progress_events = Arc::new(Mutex::new(Vec::new()));
    let observed_progress = Arc::clone(&progress_events);
    let control = VceRefreshControl::default().with_observer(Arc::new(move |progress| {
        observed_progress.lock().unwrap().push(progress);
    }));

    engine
        .refresh_observed(
            RefreshRequest::full(
                "round-robin-initial-build",
                RefreshReason::Initial,
                vec![
                    FileObservation::present(
                        "src/first.rs",
                        "fn first() {}",
                        Some(1),
                        crate::model::FileSource::Filesystem,
                    ),
                    FileObservation::present(
                        "src/second.rs",
                        "fn second() {}",
                        Some(1),
                        crate::model::FileSource::Filesystem,
                    ),
                ],
                None,
            ),
            &index,
            &ai,
            &source,
            &control,
        )
        .await
        .expect("round-robin remote initial build");

    let status_events = events
        .lock()
        .unwrap()
        .iter()
        .filter(|event| event.starts_with("status:"))
        .cloned()
        .collect::<Vec<_>>();
    assert!(status_events.len() >= 4);
    assert_ne!(status_events[0], status_events[1]);
    assert_eq!(status_events[0], status_events[2]);
    assert_eq!(status_events[1], status_events[3]);
    let content_request_progress = progress_events
        .lock()
        .unwrap()
        .iter()
        .filter(|progress| {
            progress.provider_batch_state.is_some() && progress.operation_id.contains(":content:")
        })
        .map(|progress| progress.completed)
        .collect::<Vec<_>>();
    assert!(content_request_progress.len() >= 4);
    assert_eq!(&content_request_progress[..2], &[1, 1]);
    assert!(
        content_request_progress
            .windows(2)
            .all(|counts| counts[1] >= counts[0])
    );
    // Every emitted batch count must remain within its frozen denominator across repeated polls.
    // 多轮查询期间，每次发出的批次数都必须处于冻结分母以内。
    let observed = progress_events.lock().unwrap();
    assert!(observed.iter().all(|progress| {
        progress
            .total
            .is_none_or(|total| progress.completed <= total)
    }));
    // Waiting and querying never claim logical completion before result collection begins.
    // 结果收集开始前，等待与查询不得宣称逻辑批次已完成。
    let first_collection = observed
        .iter()
        .position(|progress| {
            progress.operation_id == "round-robin-initial-build:content"
                && progress
                    .activity
                    .as_ref()
                    .is_some_and(|activity| activity.stage == VceEmbeddingActivityStage::Collecting)
        })
        .expect("content collection must begin after provider completion");
    assert!(
        observed[..first_collection]
            .iter()
            .filter(
                |progress| progress.operation_id == "round-robin-initial-build:content"
                    && progress.activity.as_ref().is_some_and(|activity| matches!(
                        activity.stage,
                        VceEmbeddingActivityStage::Waiting | VceEmbeddingActivityStage::Polling
                    ))
            )
            .all(|progress| progress.completed == 0)
    );
}

/// Verify malformed provider identities are rejected before any logical result is published.
/// 验证格式错误的供应商身份会在发布任何逻辑结果前被拒绝。
#[tokio::test]
async fn remote_batch_identity_mismatch_publishes_no_logical_result() {
    let mut engine = test_engine();
    let (mut ai, index, source, _) = fake_host();
    ai.embedding_batch_capabilities = Some(fake_file_batch_capabilities());
    ai.batch_collection_returns_unknown_identity = true;
    let write_count = Arc::clone(&index.embedding_batch_result_write_count);

    let error = engine
        .refresh(
            RefreshRequest::full(
                "remote-invalid-result-identity",
                RefreshReason::Initial,
                vec![FileObservation::present(
                    "src/lib.rs",
                    "fn main() {}",
                    Some(1),
                    crate::model::FileSource::Filesystem,
                )],
                None,
            ),
            &index,
            &ai,
            &source,
        )
        .await
        .expect_err("unknown result identity must fail");

    assert!(matches!(error, VceError::InvalidHostResponse { .. }));
    assert_eq!(*write_count.lock().unwrap(), 0);
    assert!(index.embedding_batch_results.lock().unwrap().is_empty());
    let records = index.remote_batch_records.lock().unwrap();
    assert!(!records.is_empty());
    assert!(records.values().any(|record| {
        record.state == VceEmbeddingRemoteBatchState::Failed
            && record
                .error
                .as_deref()
                .is_some_and(|diagnostic| diagnostic.contains("duplicate, missing, or unknown"))
    }));
}

/// Verify an initial build without declared Batch capability retains the synchronous path.
/// 验证未声明 Batch 能力的首次构建保留同步路径。
#[tokio::test]
async fn initial_build_without_batch_capability_uses_synchronous_embedding() {
    let mut engine = test_engine();
    let (ai, index, source, _) = fake_host();
    let events = Arc::clone(&ai.embedding_batch_events);
    let synchronous_sizes = Arc::clone(&ai.embedding_batch_sizes);

    engine
        .refresh(
            RefreshRequest::full(
                "synchronous-initial-build",
                RefreshReason::Initial,
                vec![FileObservation::present(
                    "src/lib.rs",
                    "fn main() {}",
                    Some(1),
                    crate::model::FileSource::Filesystem,
                )],
                None,
            ),
            &index,
            &ai,
            &source,
        )
        .await
        .expect("synchronous initial build");

    assert!(events.lock().unwrap().is_empty());
    assert!(!synchronous_sizes.lock().unwrap().is_empty());
}

/// Verify ordinary incremental refreshes remain synchronous even when Batch is available.
/// 验证即使 Batch 可用，普通增量刷新仍保持同步执行。
#[tokio::test]
async fn incremental_refresh_with_batch_capability_remains_synchronous() {
    let mut engine = test_engine();
    let (mut ai, index, source, _) = fake_host();
    ai.embedding_batch_capabilities = Some(fake_file_batch_capabilities());
    let events = Arc::clone(&ai.embedding_batch_events);
    let synchronous_sizes = Arc::clone(&ai.embedding_batch_sizes);
    let initial = FileObservation::present(
        "src/lib.rs",
        "fn first() {}",
        Some(1),
        crate::model::FileSource::Filesystem,
    );
    engine
        .refresh(
            RefreshRequest::full(
                "batch-initial-before-incremental",
                RefreshReason::Initial,
                vec![initial],
                None,
            ),
            &index,
            &ai,
            &source,
        )
        .await
        .expect("remote initial build");
    events.lock().unwrap().clear();
    synchronous_sizes.lock().unwrap().clear();

    engine
        .refresh(
            RefreshRequest::full(
                "ordinary-incremental-refresh",
                RefreshReason::Manual,
                vec![FileObservation::present(
                    "src/lib.rs",
                    "fn second() {}",
                    Some(2),
                    crate::model::FileSource::Filesystem,
                )],
                None,
            ),
            &index,
            &ai,
            &source,
        )
        .await
        .expect("synchronous incremental refresh");

    assert!(events.lock().unwrap().is_empty());
    assert!(!synchronous_sizes.lock().unwrap().is_empty());
}

/// Verify a deterministic result-contract failure becomes a durable failed remote task.
/// 验证确定性结果契约失败会成为持久的远端任务失败状态。
#[tokio::test]
async fn remote_batch_collection_contract_failure_is_durable() {
    let mut engine = test_engine();
    engine.config.indexing.embedding_max_inputs = 1;
    let (mut ai, index, source, _) = fake_host();
    let mut capabilities = fake_file_batch_capabilities();
    capabilities.host_policy_maximum_requests = 1;
    capabilities.maximum_requests = 1;
    ai.embedding_batch_capabilities = Some(capabilities);
    let events = Arc::clone(&ai.embedding_batch_events);
    ai.batch_collection_error = Some(VcePortError::InvalidResponse {
        operation: VcePortOperation::Embedding,
        reason: String::from("fake completed Batch omitted one input"),
    });

    let error = engine
        .refresh(
            RefreshRequest::full(
                "remote-collection-failure",
                RefreshReason::Initial,
                vec![
                    FileObservation::present(
                        "src/lib.rs",
                        "fn main() {}",
                        Some(1),
                        crate::model::FileSource::Filesystem,
                    ),
                    FileObservation::present(
                        "src/peer.rs",
                        "fn peer() {}",
                        Some(1),
                        crate::model::FileSource::Filesystem,
                    ),
                ],
                None,
            ),
            &index,
            &ai,
            &source,
        )
        .await
        .expect_err("invalid provider result must fail the refresh");
    assert!(
        matches!(&error, VceError::Port(VcePortError::InvalidResponse { .. })),
        "unexpected remote collection failure: {error:?}"
    );
    let records = index.remote_batch_records.lock().unwrap();
    assert_eq!(
        records
            .values()
            .filter(|record| record.state == VceEmbeddingRemoteBatchState::Failed)
            .count(),
        1
    );
    assert_eq!(
        records
            .values()
            .filter(|record| record.state == VceEmbeddingRemoteBatchState::Cancelled)
            .count(),
        1
    );
    let failed = records
        .values()
        .find(|record| record.state == VceEmbeddingRemoteBatchState::Failed)
        .expect("durable failed remote task");
    assert!(
        failed
            .error
            .as_deref()
            .is_some_and(|message| message.contains("omitted one input"))
    );
    failed.validate().expect("valid failed remote record");
    drop(records);
    assert_eq!(
        events
            .lock()
            .unwrap()
            .iter()
            .filter(|event| event.starts_with("cancel:"))
            .count(),
        1
    );
}

/// Cached logical batches become explicit gaps without changing result order or resubmitting them.
/// 缓存逻辑批次形成显式间隙，不改变结果顺序，也不重复提交。
#[tokio::test]
async fn remote_batch_skips_cached_ranges_and_preserves_order() {
    let engine = test_engine();
    let (mut ai, index, _, _) = fake_host();
    ai.embedding_batch_capabilities = Some(fake_file_batch_capabilities());
    let plan = EmbeddingPlan::new(
        (0..3)
            .map(|ordinal| {
                EmbeddingInput::new(format!("input-{ordinal}"), format!("text {ordinal}")).unwrap()
            })
            .collect(),
        1,
        8192,
        4096,
        128,
    )
    .unwrap();
    let mut manifest = VceEmbeddingRunManifest {
        schema_version: crate::manifest::VCE_EMBEDDING_MANIFEST_SCHEMA_VERSION,
        run_id: "cached-gaps".into(),
        namespace_id: "vce:code-chain".into(),
        generation: 1,
        source_revision: None,
        state: VceEmbeddingRunState::Running,
        contract: VceEmbeddingRunContract {
            embedding_space: test_space(),
            configured_max_inputs: 1,
            provider_max_inputs: Some(20),
            effective_max_inputs: 1,
            max_chars: 8192,
            max_segment_chars: 4096,
            overlap_chars: 128,
            max_file_bytes: 16777216,
            purposes: vec![VceAiPurpose::CodeDocumentEmbedding.as_str().into()],
        },
        base_checkpoint_version: None,
        reused_blob_names: vec![],
        nodes: vec![],
        batches: plan
            .batches
            .iter()
            .enumerate()
            .map(|(index, batch)| {
                VceEmbeddingBatchRecord::pending(
                    format!("cached-gaps:content:{index}"),
                    VceAiPurpose::CodeDocumentEmbedding.as_str(),
                    batch,
                )
            })
            .collect(),
        created_at_unix_ms: 1,
        updated_at_unix_ms: 1,
    };
    // First and last batches already have immutable results; only the middle range needs a task.
    // 首尾批次已持有不可变结果；仅中间范围需要任务。
    for ordinal in [0, 2] {
        manifest.batches[ordinal].mark_succeeded();
        let id = manifest.batches[ordinal].batch_id.clone();
        index.embedding_batch_results.lock().unwrap().insert(
            (manifest.run_id.clone(), id.clone()),
            VceEmbeddingBatchResult::from_vectors(
                id,
                &[SegmentVector {
                    input_id: plan.batches[ordinal].request_input_ids()[0].clone(),
                    values: vec![1.0, 1.0],
                }],
            ),
        );
    }
    let capability = ai.capability_snapshot();
    let context = EmbeddingExecutionContext {
        request_id: "cached-gaps:content",
        purpose: VceAiPurpose::CodeDocumentEmbedding,
        ai: &ai,
        capability: &capability,
        use_remote_batch: true,
        overall: None,
    };
    let result = engine
        .execute_embedding_plan_remote_durable(
            &plan,
            &context,
            &VceRefreshControl::default(),
            &index,
            &mut manifest,
        )
        .await
        .unwrap();
    assert_eq!(
        result
            .iter()
            .map(|vector| vector.input_id.clone())
            .collect::<Vec<_>>(),
        plan.batches
            .iter()
            .flat_map(|batch| batch.request_input_ids())
            .collect::<Vec<_>>()
    );
    {
        let records = index.remote_batch_records.lock().unwrap();
        assert_eq!(records.len(), 1);
        assert_eq!(
            records.values().next().unwrap().first_logical_batch_index,
            1
        );
        assert_eq!(records.values().next().unwrap().logical_batch_count, 1);
    }
    // No remote records are required when every result is already durable.
    // 全部结果已持久化时无需任何远端记录。
    index.remote_batch_records.lock().unwrap().clear();
    let events_before = ai.embedding_batch_events.lock().unwrap().len();
    let cached = engine
        .execute_embedding_plan_remote_durable(
            &plan,
            &context,
            &VceRefreshControl::default(),
            &index,
            &mut manifest,
        )
        .await
        .unwrap();
    assert_eq!(cached, result);
    assert_eq!(
        ai.embedding_batch_events.lock().unwrap().len(),
        events_before
    );
}

/// Verify a retryable result download keeps the completed remote task ready for recovery.
/// 验证可重试结果下载失败会保留已完成远端任务供恢复收集。
#[tokio::test]
async fn remote_batch_collection_transport_failure_remains_recoverable() {
    let mut engine = test_engine();
    let (mut ai, index, source, _) = fake_host();
    ai.embedding_batch_capabilities = Some(fake_file_batch_capabilities());
    ai.batch_collection_error = Some(VcePortError::Retryable {
        operation: VcePortOperation::Embedding,
        message: String::from("fake result download timed out"),
        retry_after_ms: Some(1),
    });

    let error = engine
        .refresh(
            RefreshRequest::full(
                "remote-collection-retry",
                RefreshReason::Initial,
                vec![FileObservation::present(
                    "src/lib.rs",
                    "fn main() {}",
                    Some(1),
                    crate::model::FileSource::Filesystem,
                )],
                None,
            ),
            &index,
            &ai,
            &source,
        )
        .await
        .expect_err("transient result download must surface to the job runner");
    assert!(matches!(
        error,
        VceError::Port(VcePortError::Retryable { .. })
    ));
    let records = index.remote_batch_records.lock().unwrap();
    assert!(records.values().any(|record| {
        record.purpose == VceAiPurpose::CodeDocumentEmbedding.as_str()
            && record.state == VceEmbeddingRemoteBatchState::Ready
            && record.error.is_none()
    }));
}

/// Verify an interrupted Batch creation is reconciled without re-uploading or blind resubmission.
/// 验证中断的 Batch 创建会被对账，且不会重复上传或盲目重新提交。
#[tokio::test]
async fn file_backed_submission_unknown_resumes_by_exact_reconciliation() {
    let engine = test_engine();
    let (mut ai, index, _, _) = fake_host();
    ai.embedding_batch_capabilities = Some(fake_file_batch_capabilities());
    *ai.submission_unknown_once.lock().unwrap() = true;
    let events = Arc::clone(&ai.embedding_batch_events);
    let plan = EmbeddingPlan::new(
        vec![crate::embedding::EmbeddingInput::new("blob-a", "alpha").expect("embedding input")],
        20,
        8_192,
        4_096,
        128,
    )
    .expect("embedding plan");
    let mut manifest = VceEmbeddingRunManifest {
        schema_version: crate::manifest::VCE_EMBEDDING_MANIFEST_SCHEMA_VERSION,
        run_id: String::from("resume-run"),
        namespace_id: String::from("vce:code-chain"),
        generation: 1,
        source_revision: None,
        state: VceEmbeddingRunState::Running,
        contract: VceEmbeddingRunContract {
            embedding_space: test_space(),
            configured_max_inputs: 20,
            provider_max_inputs: Some(20),
            effective_max_inputs: 20,
            max_chars: 8_192,
            max_segment_chars: 4_096,
            overlap_chars: 128,
            max_file_bytes: 16 * 1024 * 1024,
            purposes: vec![VceAiPurpose::CodeDocumentEmbedding.as_str().to_owned()],
        },
        base_checkpoint_version: None,
        reused_blob_names: Vec::new(),
        nodes: Vec::new(),
        batches: vec![VceEmbeddingBatchRecord::pending(
            "resume-run:content:0",
            VceAiPurpose::CodeDocumentEmbedding.as_str(),
            &plan.batches[0],
        )],
        created_at_unix_ms: 1,
        updated_at_unix_ms: 1,
    };
    let control = VceRefreshControl::default();
    let capability = ai.capability_snapshot();
    let execution_context = EmbeddingExecutionContext {
        request_id: "resume-request",
        purpose: VceAiPurpose::CodeDocumentEmbedding,
        ai: &ai,
        capability: &capability,
        use_remote_batch: true,
        overall: None,
    };

    let first = engine
        .execute_embedding_plan_remote_durable(
            &plan,
            &execution_context,
            &control,
            &index,
            &mut manifest,
        )
        .await;
    assert!(matches!(
        first,
        Err(VceError::Port(VcePortError::SubmissionUnknown { .. }))
    ));
    let persisted = index
        .remote_batch_records
        .lock()
        .unwrap()
        .values()
        .next()
        .cloned()
        .expect("unknown submission record");
    assert_eq!(
        persisted.state,
        VceEmbeddingRemoteBatchState::SubmissionUnknown
    );
    assert!(persisted.uploaded_input.is_some());
    assert!(persisted.submission.is_none());

    let synchronous_context = EmbeddingExecutionContext {
        request_id: "resume-request",
        purpose: VceAiPurpose::CodeDocumentEmbedding,
        ai: &ai,
        capability: &capability,
        use_remote_batch: false,
        overall: None,
    };
    let downgrade = engine
        .execute_embedding_plan_durable(&plan, synchronous_context, &control, &index, &mut manifest)
        .await
        .expect_err("durable remote work must not fall back to synchronous execution");
    assert!(matches!(downgrade, VceError::InvalidRequest(_)));

    let vectors = engine
        .execute_embedding_plan_remote_durable(
            &plan,
            &execution_context,
            &control,
            &index,
            &mut manifest,
        )
        .await
        .expect("reconciled resume");
    assert_eq!(vectors.len(), 1);
    assert_eq!(manifest.batches[0].state, VceEmbeddingBatchState::Succeeded);
    assert_eq!(manifest.batches[0].attempt, 1);
    let events = events.lock().unwrap();
    assert_eq!(
        events
            .iter()
            .filter(|event| event.starts_with("upload:"))
            .count(),
        1
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| event.starts_with("submit:"))
            .count(),
        1
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| event.starts_with("reconcile:"))
            .count(),
        1
    );
}

/// Verify a durable submitted task resumes by polling and never uploads or submits again.
/// 验证持久已提交任务通过轮询恢复，且绝不再次上传或提交。
#[tokio::test]
async fn submitted_remote_batch_resumes_without_resubmission() {
    let engine = test_engine();
    let (mut ai, index, _, _) = fake_host();
    ai.embedding_batch_capabilities = Some(fake_file_batch_capabilities());
    *ai.batch_status_error_once.lock().unwrap() = Some(VcePortError::Retryable {
        operation: VcePortOperation::Embedding,
        message: String::from("temporary status timeout"),
        retry_after_ms: Some(1),
    });
    let events = Arc::clone(&ai.embedding_batch_events);
    let plan = EmbeddingPlan::new(
        vec![crate::embedding::EmbeddingInput::new("blob-a", "alpha").expect("embedding input")],
        20,
        8_192,
        4_096,
        128,
    )
    .expect("embedding plan");
    let mut manifest = VceEmbeddingRunManifest {
        schema_version: crate::manifest::VCE_EMBEDDING_MANIFEST_SCHEMA_VERSION,
        run_id: String::from("submitted-resume-run"),
        namespace_id: String::from("vce:code-chain"),
        generation: 1,
        source_revision: None,
        state: VceEmbeddingRunState::Running,
        contract: VceEmbeddingRunContract {
            embedding_space: test_space(),
            configured_max_inputs: 20,
            provider_max_inputs: Some(20),
            effective_max_inputs: 20,
            max_chars: 8_192,
            max_segment_chars: 4_096,
            overlap_chars: 128,
            max_file_bytes: 16 * 1024 * 1024,
            purposes: vec![VceAiPurpose::CodeDocumentEmbedding.as_str().to_owned()],
        },
        base_checkpoint_version: None,
        reused_blob_names: Vec::new(),
        nodes: Vec::new(),
        batches: vec![VceEmbeddingBatchRecord::pending(
            "submitted-resume-run:content:0",
            VceAiPurpose::CodeDocumentEmbedding.as_str(),
            &plan.batches[0],
        )],
        created_at_unix_ms: 1,
        updated_at_unix_ms: 1,
    };
    let control = VceRefreshControl::default();
    let capability = ai.capability_snapshot();
    let execution_context = EmbeddingExecutionContext {
        request_id: "submitted-resume-request",
        purpose: VceAiPurpose::CodeDocumentEmbedding,
        ai: &ai,
        capability: &capability,
        use_remote_batch: true,
        overall: None,
    };

    let first = engine
        .execute_embedding_plan_remote_durable(
            &plan,
            &execution_context,
            &control,
            &index,
            &mut manifest,
        )
        .await;
    assert!(matches!(
        first,
        Err(VceError::Port(VcePortError::Retryable { .. }))
    ));
    let persisted = index
        .remote_batch_records
        .lock()
        .unwrap()
        .values()
        .next()
        .cloned()
        .expect("submitted remote record");
    assert_eq!(persisted.state, VceEmbeddingRemoteBatchState::Submitted);
    assert!(persisted.submission.is_some());

    let vectors = engine
        .execute_embedding_plan_remote_durable(
            &plan,
            &execution_context,
            &control,
            &index,
            &mut manifest,
        )
        .await
        .expect("polling resume");
    assert_eq!(vectors.len(), 1);
    let events = events.lock().unwrap();
    assert_eq!(
        events
            .iter()
            .filter(|event| event.starts_with("upload:"))
            .count(),
        1
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| event.starts_with("submit:"))
            .count(),
        1
    );
    assert_eq!(
        events
            .iter()
            .filter(|event| event.starts_with("status:"))
            .count(),
        2
    );
}

/// Verify recovery preserves durable logical results and writes only missing results before cleanup.
/// 验证恢复会保留持久逻辑结果，并仅在清理前写入缺失结果。
#[tokio::test]
async fn ready_remote_batch_recovery_writes_only_missing_logical_results() {
    let engine = test_engine();
    let (mut ai, index, _, _) = fake_host();
    ai.embedding_batch_capabilities = Some(fake_file_batch_capabilities());
    let events = Arc::clone(&ai.embedding_batch_events);
    let write_count = Arc::clone(&index.embedding_batch_result_write_count);
    let plan = EmbeddingPlan::new(
        vec![
            crate::embedding::EmbeddingInput::new("blob-a", "alpha")
                .expect("first embedding input"),
            crate::embedding::EmbeddingInput::new("blob-b", "beta")
                .expect("second embedding input"),
        ],
        1,
        8_192,
        4_096,
        128,
    )
    .expect("two logical embedding batches");
    let grouped = merge_embedding_batches(&plan.batches).expect("grouped request");
    let provider_plan = ai
        .plan_embedding_batch(VceEmbeddingBatchRequest {
            context: VceAiRequestContext::new(
                "partial-resume-request:remote:0",
                VceAiPurpose::CodeDocumentEmbedding,
                ai.binding(),
            ),
            batch: grouped,
        })
        .await
        .expect("provider plan");
    let task = provider_plan.tasks[0].clone();
    let uploaded_input = VceEmbeddingBatchUploadedInput {
        task_id: task.task_id.clone(),
        provider_input_file_id: String::from("file-partial-resume"),
        uploaded_at_unix_ms: 1,
    };
    let submission = VceEmbeddingBatchSubmission {
        task_id: task.task_id.clone(),
        provider_batch_id: String::from("provider-partial-resume"),
        provider_input_file_id: Some(uploaded_input.provider_input_file_id.clone()),
        submitted_at_unix_ms: 1,
    };
    let snapshot = VceEmbeddingBatchSnapshot {
        submission: submission.clone(),
        state: VceEmbeddingBatchProviderState::Completed,
        total_requests: Some(2),
        completed_requests: Some(2),
        failed_requests: Some(0),
        output_file_id: Some(String::from("output-partial-resume")),
        error_file_id: None,
        error: None,
    };
    let record = VceEmbeddingRemoteBatchRecord {
        remote_task_id: task.task_id.clone(),
        purpose: VceAiPurpose::CodeDocumentEmbedding.as_str().to_owned(),
        first_logical_batch_index: 0,
        logical_batch_count: 2,
        plan: provider_plan,
        state: VceEmbeddingRemoteBatchState::Ready,
        uploaded_input: Some(uploaded_input),
        submission: Some(submission),
        snapshot: Some(snapshot),
        poll_attempt: 1,
        next_poll_not_before_unix_ms: None,
        error: None,
        cleanup_warning: None,
        updated_at_unix_ms: 1,
    };
    index.remote_batch_records.lock().unwrap().insert(
        (String::from("partial-resume-run"), task.task_id.clone()),
        record,
    );
    let first_batch_id = String::from("partial-resume-run:content:0");
    let first_vectors = vec![SegmentVector {
        input_id: plan.batches[0].request_input_ids()[0].clone(),
        values: vec![1.0, 1.0],
    }];
    index.embedding_batch_results.lock().unwrap().insert(
        (String::from("partial-resume-run"), first_batch_id.clone()),
        VceEmbeddingBatchResult::from_vectors(first_batch_id.clone(), &first_vectors),
    );
    let mut manifest = VceEmbeddingRunManifest {
        schema_version: crate::manifest::VCE_EMBEDDING_MANIFEST_SCHEMA_VERSION,
        run_id: String::from("partial-resume-run"),
        namespace_id: String::from("vce:code-chain"),
        generation: 1,
        source_revision: None,
        state: VceEmbeddingRunState::Running,
        contract: VceEmbeddingRunContract {
            embedding_space: test_space(),
            configured_max_inputs: 1,
            provider_max_inputs: Some(20),
            effective_max_inputs: 1,
            max_chars: 8_192,
            max_segment_chars: 4_096,
            overlap_chars: 128,
            max_file_bytes: 16 * 1024 * 1024,
            purposes: vec![VceAiPurpose::CodeDocumentEmbedding.as_str().to_owned()],
        },
        base_checkpoint_version: None,
        reused_blob_names: Vec::new(),
        nodes: Vec::new(),
        batches: plan
            .batches
            .iter()
            .enumerate()
            .map(|(index, batch)| {
                VceEmbeddingBatchRecord::pending(
                    format!("partial-resume-run:content:{index}"),
                    VceAiPurpose::CodeDocumentEmbedding.as_str(),
                    batch,
                )
            })
            .collect(),
        created_at_unix_ms: 1,
        updated_at_unix_ms: 1,
    };
    let capability = ai.capability_snapshot();
    let execution_context = EmbeddingExecutionContext {
        request_id: "partial-resume-request",
        purpose: VceAiPurpose::CodeDocumentEmbedding,
        ai: &ai,
        capability: &capability,
        use_remote_batch: true,
        overall: None,
    };
    let control = VceRefreshControl::default();

    let vectors = engine
        .execute_embedding_plan_remote_durable(
            &plan,
            &execution_context,
            &control,
            &index,
            &mut manifest,
        )
        .await
        .expect("partial durable recovery");

    assert_eq!(vectors.len(), 2);
    assert_eq!(*write_count.lock().unwrap(), 1);
    assert!(
        index
            .embedding_batch_results
            .lock()
            .unwrap()
            .contains_key(&(String::from("partial-resume-run"), first_batch_id.clone()))
    );
    {
        let events = events.lock().unwrap();
        assert_eq!(
            events
                .iter()
                .filter(|event| event.starts_with("collect:"))
                .count(),
            1
        );
        assert_eq!(
            events
                .iter()
                .filter(|event| event.starts_with("cleanup:"))
                .count(),
            1
        );
    }

    // Recreate the precise crash window after every logical result was durable but before cleanup state persisted.
    // 重建全部逻辑结果已持久化但清理状态尚未落盘的精确崩溃窗口。
    let record_key = (String::from("partial-resume-run"), task.task_id);
    {
        let mut records = index.remote_batch_records.lock().unwrap();
        let record = records
            .get_mut(&record_key)
            .expect("completed remote record");
        record.state = VceEmbeddingRemoteBatchState::Ready;
        record.updated_at_unix_ms = record.updated_at_unix_ms.saturating_add(1);
    }
    events.lock().unwrap().clear();
    *write_count.lock().unwrap() = 0;
    let recovered = engine
        .execute_embedding_plan_remote_durable(
            &plan,
            &execution_context,
            &control,
            &index,
            &mut manifest,
        )
        .await
        .expect("all-durable cleanup recovery");
    assert_eq!(recovered.len(), 2);
    assert_eq!(*write_count.lock().unwrap(), 0);
    {
        let events = events.lock().unwrap();
        assert_eq!(
            events
                .iter()
                .filter(|event| event.starts_with("collect:"))
                .count(),
            0
        );
        assert_eq!(
            events
                .iter()
                .filter(|event| event.starts_with("cleanup:"))
                .count(),
            1
        );
    }

    // Corrupt one durable vector after success to prove restart recovery revalidates dimensions.
    // 在成功后破坏一条持久向量，以证明重启恢复会重新校验维度。
    index
        .embedding_batch_results
        .lock()
        .unwrap()
        .get_mut(&(String::from("partial-resume-run"), first_batch_id))
        .expect("first durable logical result")
        .vectors[0]
        .values = vec![1.0];
    let error = engine
        .execute_embedding_plan_remote_durable(
            &plan,
            &execution_context,
            &control,
            &index,
            &mut manifest,
        )
        .await
        .expect_err("corrupt durable remote result must fail recovery");
    assert!(matches!(error, VceError::InvalidHostResponse { .. }));
}

/// Verify host cancellation reaches the provider and persists the exact terminal state.
/// 验证宿主取消会到达供应商并持久化精确终态。
#[test]
fn remote_batch_cancellation_persists_provider_terminal_state() {
    let engine = test_engine();
    let (mut ai, index, _, _) = fake_host();
    let capabilities = fake_file_batch_capabilities();
    ai.embedding_batch_capabilities = Some(capabilities.clone());
    let events = Arc::clone(&ai.embedding_batch_events);
    let logical_plan = EmbeddingPlan::new(
        vec![crate::embedding::EmbeddingInput::new("blob-a", "alpha").expect("embedding input")],
        20,
        8_192,
        4_096,
        128,
    )
    .expect("embedding plan");
    let task = VceEmbeddingBatchTaskPlan {
        task_id: String::from("cancel-task"),
        first_input_index: 0,
        input_count: 1,
        encoded_payload_bytes: 128,
        payload_sha256: "b".repeat(64),
    };
    let provider_plan = VceEmbeddingBatchPlan {
        request_id: String::from("cancel-request"),
        request_fingerprint: "a".repeat(64),
        capabilities,
        tasks: vec![task.clone()],
    };
    let uploaded_input = VceEmbeddingBatchUploadedInput {
        task_id: task.task_id.clone(),
        provider_input_file_id: String::from("file-cancel-task"),
        uploaded_at_unix_ms: 1,
    };
    let submission = VceEmbeddingBatchSubmission {
        task_id: task.task_id.clone(),
        provider_batch_id: String::from("provider-cancel-task"),
        provider_input_file_id: Some(uploaded_input.provider_input_file_id.clone()),
        submitted_at_unix_ms: 1,
    };
    let mut record = VceEmbeddingRemoteBatchRecord {
        remote_task_id: task.task_id,
        purpose: VceAiPurpose::CodeDocumentEmbedding.as_str().to_owned(),
        first_logical_batch_index: 0,
        logical_batch_count: 1,
        plan: provider_plan,
        state: VceEmbeddingRemoteBatchState::Submitted,
        uploaded_input: Some(uploaded_input),
        submission: Some(submission),
        snapshot: None,
        poll_attempt: 0,
        next_poll_not_before_unix_ms: None,
        error: None,
        cleanup_warning: None,
        updated_at_unix_ms: 1,
    };
    let request = VceEmbeddingBatchRequest {
        context: VceAiRequestContext::new(
            "cancel-request",
            VceAiPurpose::CodeDocumentEmbedding,
            ai.binding(),
        ),
        batch: logical_plan.batches[0].clone(),
    };
    assert!(remote_embedding_batch_can_cancel(&record));
    assert!(!remote_embedding_batch_run_has_terminal_failure(
        std::slice::from_ref(&record)
    ));

    block_on(engine.cancel_remote_embedding_batch(
        request.clone(),
        &ai,
        &index,
        "cancel-run",
        &mut record,
    ));

    assert_eq!(record.state, VceEmbeddingRemoteBatchState::Cancelled);
    assert!(remote_embedding_batch_run_has_terminal_failure(
        std::slice::from_ref(&record)
    ));
    assert_eq!(record.poll_attempt, 1);
    record.validate().expect("cancelled remote record");
    assert_eq!(events.lock().unwrap().as_slice(), ["cancel:cancel-task"]);
    assert_eq!(
        index
            .remote_batch_records
            .lock()
            .unwrap()
            .get(&(String::from("cancel-run"), String::from("cancel-task")))
            .map(|stored| stored.state),
        Some(VceEmbeddingRemoteBatchState::Cancelled)
    );

    events.lock().unwrap().clear();
    record.state = VceEmbeddingRemoteBatchState::Succeeded;
    record.snapshot = Some(VceEmbeddingBatchSnapshot {
        submission: record.submission.clone().expect("submitted identity"),
        state: VceEmbeddingBatchProviderState::Completed,
        total_requests: Some(1),
        completed_requests: Some(1),
        failed_requests: Some(0),
        output_file_id: Some(String::from("output-file")),
        error_file_id: None,
        error: None,
    });
    record.error = None;
    let settled_poll_attempt = record.poll_attempt;
    assert!(!remote_embedding_batch_can_cancel(&record));
    assert!(!remote_embedding_batch_run_has_terminal_failure(
        std::slice::from_ref(&record)
    ));

    block_on(engine.cancel_remote_embedding_batch(request, &ai, &index, "cancel-run", &mut record));

    assert_eq!(record.state, VceEmbeddingRemoteBatchState::Succeeded);
    assert_eq!(record.poll_attempt, settled_poll_attempt);
    assert!(events.lock().unwrap().is_empty());
}

/// Verify an empty committed index remains queryable without model calls or fabricated scope.
/// 验证已提交空索引仍可查询，不产生模型调用或虚构范围。
///
/// Takes no parameters and returns nothing; distinguishes initialization from explicit invalid scope.
/// 无参数、无返回值；区分初始化与明确无效范围。
#[test]
fn initialized_empty_checkpoint_returns_without_model_calls() {
    // The same ports serve initialization and retrieval so unexpected model activity is observable.
    // 同一组端口服务初始化与检索，使非预期模型活动可被观察。
    let mut engine = test_engine();
    let (ai, index, source, _) = fake_host();
    assert!(matches!(
        block_on(engine.retrieve_read_view(
            RetrieveRequest::current_checkpoint("before-initialization", "find code"),
            &index,
            &ai,
        )),
        Err(VceError::MissingCheckpoint)
    ));
    block_on(engine.refresh(
        RefreshRequest::full("initialize-empty", RefreshReason::Initial, Vec::new(), None),
        &index,
        &ai,
        &source,
    ))
    .expect("initialize empty checkpoint");
    let result = block_on(engine.retrieve(
        RetrieveRequest::current_checkpoint("query-empty", "find code"),
        &index,
        &ai,
    ))
    .expect("empty committed search succeeds");
    assert!(result.hits.is_empty() && result.scope.is_empty());
    assert!(ai.embedding_operation_ids.lock().unwrap().is_empty());
    assert_eq!(*ai.rewrite_call_count.lock().unwrap(), 0);
    assert!(ai.rerank_candidate_counts.lock().unwrap().is_empty());
    engine
        .state_snapshot()
        .validate()
        .expect("empty search statistics remain valid");
    let mut invalid = RetrieveRequest::current_checkpoint("explicit-empty", "find code");
    invalid.scope = RetrievalScope::Blobs(BTreeSet::new());
    assert!(matches!(
        block_on(engine.retrieve_read_view(invalid, &index, &ai)),
        Err(VceError::EmptyScope)
    ));
}

/// Verify editor overlay changes are visible as pending work before publication.
/// 验证编辑器覆盖层变更在发布前会显示为待刷新状态。
#[test]
fn overlay_is_a_functional_entry() {
    let mut engine = test_engine();
    let (ai, index, source, _) = fake_host();
    block_on(engine.refresh(
        RefreshRequest::full(
            "refresh-1",
            RefreshReason::Initial,
            vec![crate::workspace::FileObservation::present(
                "src/lib.rs",
                "fn main() {}",
                Some(1),
                crate::model::FileSource::Filesystem,
            )],
            None,
        ),
        &index,
        &ai,
        &source,
    ))
    .unwrap();

    let snapshot = engine
        .observe("src/lib.rs", "fn main() { println!(\"overlay\"); }")
        .unwrap();
    assert_eq!(snapshot.generation, 2);
    assert_eq!(engine.status().state, VceEngineState::PendingRefresh);
    assert_eq!(engine.workspace_statistics().present_blobs, 1);
}

/// Verify retrieval invokes the functional pipeline and returns rendered context.
/// 验证检索调用功能流水线并返回渲染后的上下文。
#[test]
fn retrieval_is_a_functional_entry() {
    let mut engine = test_engine();
    let (ai, index, source, _) = fake_host();
    block_on(engine.refresh(
        RefreshRequest::full(
            "refresh-1",
            RefreshReason::Initial,
            vec![crate::workspace::FileObservation::present(
                "src/lib.rs",
                "fn main() {}",
                Some(1),
                crate::model::FileSource::Filesystem,
            )],
            None,
        ),
        &index,
        &ai,
        &source,
    ))
    .unwrap();
    let result = block_on(engine.retrieve(
        RetrieveRequest::current_checkpoint("query-1", "find main function"),
        &index,
        &ai,
    ))
    .unwrap();
    assert!(!result.hits.is_empty());
    assert!(result.formatted_context.contains("Path:"));
    assert_eq!(engine.statistics().retrieval_successes, 1);
}

/// Verify two immutable query entries preserve the published engine and keep distinct summaries.
/// 验证两个不可变查询入口保留已发布引擎，并保持独立摘要。
///
/// Uses isolated port substitutes. Returns no value; storage snapshot isolation is tested by the host.
/// 使用隔离端口替身。无返回值；存储快照隔离由宿主测试。
#[test]
fn immutable_read_views_preserve_published_state_and_query_identity() {
    // engine is populated once before being shared immutably with both queries.
    // engine 在被两个查询不可变共享之前仅填充一次。
    let mut engine = test_engine();
    // ai/index/source are existing deterministic ports, not a replacement pipeline.
    // ai/index/source 为既有确定性端口，而非替代流水线。
    let (ai, index, source, _) = fake_host();
    block_on(engine.refresh(
        RefreshRequest::full(
            "read-view-initial",
            RefreshReason::Initial,
            vec![crate::workspace::FileObservation::present(
                "src/lib.rs",
                "fn main() {}",
                Some(1),
                crate::model::FileSource::Filesystem,
            )],
            None,
        ),
        &index,
        &ai,
        &source,
    ))
    .unwrap();
    // published includes counters and summaries, not merely a checkpoint number.
    // published 包含计数与摘要，不仅仅是检查点编号。
    let published = engine.state_snapshot();
    // results borrow the same immutable engine for overlapping query lifetimes.
    // results 在重叠查询生命周期内借用同一不可变引擎。
    let (first, second) = block_on(futures::future::join(
        engine.retrieve_read_view(
            RetrieveRequest::current_checkpoint("view-query-1", "find main function"),
            &index,
            &ai,
        ),
        engine.retrieve_read_view(
            RetrieveRequest::current_checkpoint("view-query-2", "find main function"),
            &index,
            &ai,
        ),
    ));
    // first/second preserve their operation identities even when their selected content matches.
    // first/second 即使所选内容相同也保留各自操作身份。
    let first = first.unwrap();
    let second = second.unwrap();
    assert_eq!(first.summary.request_id, "view-query-1");
    assert_eq!(second.summary.request_id, "view-query-2");
    assert!(!first.hits.is_empty());
    assert_eq!(first.formatted_context, second.formatted_context);
    assert_eq!(engine.state_snapshot(), published);

    // A failed query does not poison a still-valid published checkpoint's lifecycle.
    // 失败查询不会污染仍有效的已发布检查点生命周期。
    assert!(
        block_on(engine.retrieve_read_view(
            RetrieveRequest::current_checkpoint("view-invalid", ""),
            &index,
            &ai,
        ))
        .is_err()
    );
    assert_eq!(engine.state_snapshot(), published);
    assert_eq!(engine.status().state, VceEngineState::Ready);
}

/// Verify request-local budgets change real nonempty retrieval without mutating the shared checkpoint.
/// 验证请求本地预算改变真实非空检索结果，不修改共享检查点。
/// No parameters or output; existing deterministic ports provide source, embedding and candidate results.
/// 无参数及输出；既有确定性端口提供源码、向量及候选结果。
#[test]
fn immutable_queries_freeze_independent_execution_limits() {
    // engine and ports establish an actual committed nonempty workspace before testing query policy.
    // engine 与 ports 在测试查询策略前建立实际已提交的非空工作区。
    let mut engine = test_engine();
    let (ai, index, source, _) = fake_host();
    block_on(engine.refresh(
        RefreshRequest::full(
            "limits-initial",
            RefreshReason::Initial,
            vec![crate::workspace::FileObservation::present(
                "src/lib.rs",
                "fn main() {}",
                Some(1),
                crate::model::FileSource::Filesystem,
            )],
            None,
        ),
        &index,
        &ai,
        &source,
    ))
    .expect("initialize limit fixture");
    // published and defaults must survive both concurrent policies and invalid requests unchanged.
    // published 与 defaults 必须在两种并发策略及无效请求后保持不变。
    let published = engine.state_snapshot();
    let defaults = VceRetrievalLimits::from_config(engine.config());
    let narrow = VceRetrievalLimits {
        query_max_concurrency: 1,
        max_results: 1,
        max_context_chars: 1,
    };
    let (wide_result, narrow_result) = block_on(futures::future::join(
        engine.retrieve_read_view_with_limits(
            RetrieveRequest::current_checkpoint("limits-wide", "find main function"),
            &index,
            &ai,
            defaults,
        ),
        engine.retrieve_read_view_with_limits(
            RetrieveRequest::current_checkpoint("limits-narrow", "find main function"),
            &index,
            &ai,
            narrow,
        ),
    ));
    // Results retain independently frozen budgets, including all final formatting overhead.
    // 结果保持独立冻结的预算，包含最终格式化的全部开销。
    let wide_result = wide_result.expect("wide result");
    let narrow_result = narrow_result.expect("narrow result");
    assert!(!wide_result.hits.is_empty());
    assert!(wide_result.formatted_context.chars().count() <= defaults.max_context_chars);
    assert!(narrow_result.hits.is_empty());
    assert!(narrow_result.formatted_context.chars().count() <= narrow.max_context_chars);
    assert_eq!(engine.state_snapshot(), published);
    assert_eq!(VceRetrievalLimits::from_config(engine.config()), defaults);
    for invalid in [
        VceRetrievalLimits {
            query_max_concurrency: 0,
            ..defaults
        },
        VceRetrievalLimits {
            max_results: 0,
            ..defaults
        },
        VceRetrievalLimits {
            max_context_chars: 0,
            ..defaults
        },
    ] {
        assert!(matches!(
            block_on(engine.retrieve_read_view_with_limits(
                RetrieveRequest::current_checkpoint("limits-invalid", "find main function"),
                &index,
                &ai,
                invalid,
            )),
            Err(VceError::InvalidRequest(_))
        ));
    }
    assert_eq!(engine.state_snapshot(), published);
}

/// Verify explicit retrieval scopes reject invalid Blob identities before host search.
/// 验证明确定义的检索范围会在宿主搜索前拒绝非法 Blob 身份。
#[test]
fn retrieval_rejects_noncanonical_explicit_blob_scope() {
    let mut engine = test_engine();
    let (ai, index, _, _) = fake_host();
    let error = block_on(engine.retrieve(
        RetrieveRequest::in_blobs(
            "query-invalid-scope",
            "find the implementation",
            BTreeSet::from(["not-a-blob".to_owned()]),
        ),
        &index,
        &ai,
    ))
    .unwrap_err();

    assert!(matches!(error, VceError::InvalidRequest(_)));
}

/// Verify the configured Rerank limit bounds the payload sent to the shared AI capability.
/// 验证配置的重排限制会约束发送给共享 AI 能力的载荷。
#[test]
fn rerank_candidate_limit_bounds_host_payload() {
    let mut config = VceConfig::new("code-chain", test_space()).unwrap();
    config.indexing.chunker.max_chars = 100;
    config.indexing.chunker.min_chars = 1;
    config.rerank_candidate_limit = 1;
    let mut engine = VceEngine::new(config).unwrap();
    let (ai, mut index, source, _) = fake_host();
    index.vary_dense_hit_by_query = true;
    let rerank_candidate_counts = ai.rerank_candidate_counts.clone();
    block_on(engine.refresh(
        RefreshRequest::full(
            "refresh-rerank-limit",
            RefreshReason::Initial,
            vec![crate::workspace::FileObservation::present(
                "src/lib.rs",
                "fn main() {}",
                Some(1),
                crate::model::FileSource::Filesystem,
            )],
            None,
        ),
        &index,
        &ai,
        &source,
    ))
    .unwrap();

    // This test opts into rewrite so the fake host produces multiple fusion candidates.
    // 本测试显式启用改写，使伪宿主产生多个融合候选。
    let mut request =
        RetrieveRequest::current_checkpoint("query-rerank-limit", "find main function");
    request.allow_query_rewrite = true;
    let result = block_on(engine.retrieve(request, &index, &ai)).unwrap();

    assert_eq!(rerank_candidate_counts.lock().unwrap().as_slice(), &[1]);
    assert!(result.summary.candidate_count > 1);
}

/// Verify a planner with no rewrite capacity never spends an optional LLM call.
/// 验证没有改写容量的规划器绝不会消耗可选 LLM 调用。
#[test]
fn single_query_budget_skips_query_rewrite_call() {
    let mut config = VceConfig::new("code-chain", test_space()).unwrap();
    config.query.max_queries = 1;
    let mut engine = VceEngine::new(config).unwrap();
    let (ai, index, source, _) = fake_host();
    let rewrite_call_count = ai.rewrite_call_count.clone();
    block_on(engine.refresh(
        RefreshRequest::full(
            "refresh-single-query",
            RefreshReason::Initial,
            vec![crate::workspace::FileObservation::present(
                "src/lib.rs",
                "fn main() {}",
                Some(1),
                crate::model::FileSource::Filesystem,
            )],
            None,
        ),
        &index,
        &ai,
        &source,
    ))
    .unwrap();

    let mut request = RetrieveRequest::current_checkpoint("query-single", "find main function");
    request.allow_query_rewrite = true;
    let result = block_on(engine.retrieve(request, &index, &ai)).unwrap();

    assert_eq!(*rewrite_call_count.lock().unwrap(), 0);
    assert_eq!(result.query_plan.queries.len(), 1);
    assert!(result.degradations.is_empty());
}

/// Verify path fallbacks cannot expand the post-fusion candidate budget.
/// 验证路径回填不能扩大融合后的候选预算。
#[test]
fn path_fallbacks_obey_fusion_candidate_limit() {
    let mut config = VceConfig::new("code-chain", test_space()).unwrap();
    config.fusion_candidate_limit = 1;
    let mut engine = VceEngine::new(config).unwrap();
    let (ai, mut index, source, _) = fake_host();
    index.return_path_fallback = true;
    block_on(engine.refresh(
        RefreshRequest::full(
            "refresh-fusion-limit",
            RefreshReason::Initial,
            vec![
                crate::workspace::FileObservation::present(
                    "src/a.rs",
                    "fn a() {}",
                    Some(1),
                    crate::model::FileSource::Filesystem,
                ),
                crate::workspace::FileObservation::present(
                    "src/z.rs",
                    "fn z() {}",
                    Some(1),
                    crate::model::FileSource::Filesystem,
                ),
            ],
            None,
        ),
        &index,
        &ai,
        &source,
    ))
    .unwrap();

    let result = block_on(engine.retrieve(
        RetrieveRequest::current_checkpoint("query-fusion-limit", "find config.json"),
        &index,
        &ai,
    ))
    .unwrap();

    assert_eq!(result.summary.candidate_count, 1);
    assert!(result.hits.len() <= 1);
}

/// Verify rewrite and Rerank failures remain visible as safe degradation.
/// 验证改写和重排失败会以安全降级方式保持可见。
#[test]
fn optional_ai_stages_degrade_without_losing_retrieval() {
    let mut engine = test_engine();
    let (mut ai, index, source, _) = fake_host();
    ai.fail_rewrite = true;
    ai.fail_rerank = true;
    block_on(engine.refresh(
        RefreshRequest::full(
            "refresh-1",
            RefreshReason::Initial,
            vec![crate::workspace::FileObservation::present(
                "src/lib.rs",
                "fn main() {}",
                Some(1),
                crate::model::FileSource::Filesystem,
            )],
            None,
        ),
        &index,
        &ai,
        &source,
    ))
    .unwrap();
    // This test explicitly exercises both optional AI stages.
    // 本测试显式覆盖两个可选 AI 阶段。
    let mut request = RetrieveRequest::current_checkpoint("query-1", "find main function");
    request.allow_query_rewrite = true;
    let result = block_on(engine.retrieve(request, &index, &ai)).unwrap();
    assert!(
        result
            .degraded_features
            .contains(&VceDegradedFeature::QueryRewrite)
    );
    assert!(
        result
            .degraded_features
            .contains(&VceDegradedFeature::Rerank)
    );
    assert_eq!(result.degradations.len(), 2);
    assert!(
        result
            .degradations
            .iter()
            .all(|degradation| !degradation.reason.is_empty())
    );
    assert!(!result.hits.is_empty());
}

/// Verify metadata-only observations are read through the host and not retained after commit.
/// 验证仅元数据观察通过宿主读取，且提交后不再保留源码正文。
#[test]
fn identified_observation_uses_bounded_source_port() {
    let mut engine = test_engine();
    let (ai, index, _, _) = fake_host();
    let staged = index.staged.clone();
    let content = "pub fn answer() -> u32 { 42 }";
    let blob = crate::identity::blob_name("src/lib.rs", content).unwrap();
    let source = SnapshotSource {
        documents: BTreeMap::from([("src/lib.rs".to_owned(), content.to_owned())]),
    };
    let result = block_on(engine.refresh(
        RefreshRequest::full(
            "refresh-identified",
            RefreshReason::Initial,
            vec![crate::workspace::FileObservation::identified(
                "src/lib.rs",
                blob.clone(),
                content.len() as u64,
                Some(9),
                crate::model::FileSource::Filesystem,
            )],
            None,
        ),
        &index,
        &ai,
        &source,
    ))
    .unwrap();
    assert_eq!(result.indexed_blobs, vec![blob]);
    assert_eq!(staged.lock().unwrap().len(), 1);
    assert!(engine.workspace_snapshot().records[0].content.is_none());
}

/// Verify metadata-only admission does not require an unused Embedding capability.
/// 验证仅元数据准入不会要求未被使用的向量能力。
#[test]
fn over_limit_blob_refresh_succeeds_without_embedding_capability() {
    let mut config = VceConfig::new("code-chain", test_space()).unwrap();
    config.indexing.max_file_bytes = 1;
    let mut engine = VceEngine::new(config).unwrap();
    let (mut ai, index, source, _) = fake_host();
    ai.embedding_available = false;

    let result = block_on(engine.refresh(
        RefreshRequest::full(
            "refresh-without-embedding",
            RefreshReason::Initial,
            vec![crate::workspace::FileObservation::present(
                "assets/large.txt",
                "larger than one byte",
                Some(1),
                crate::model::FileSource::Filesystem,
            )],
            None,
        ),
        &index,
        &ai,
        &source,
    ))
    .unwrap();

    assert_eq!(result.indexed_blobs.len(), 1);
    assert!(ai.embedding_operation_ids.lock().unwrap().is_empty());
}

/// Verify an over-limit metadata observation is staged without loading its source.
/// 验证超限的仅元数据观察不读取源码即可暂存。
#[test]
fn over_limit_metadata_blob_bypasses_source_staging_bytes() {
    let mut config = VceConfig::new("code-chain", test_space()).unwrap();
    config.indexing.max_file_bytes = 1;
    let mut engine = VceEngine::new(config).unwrap();
    let (mut ai, index, source, _) = fake_host();
    ai.embedding_available = false;
    let staged = index.staged.clone();
    let blob = crate::identity::blob_name("assets/large.txt", "ab").unwrap();

    let result = block_on(engine.refresh(
        RefreshRequest::full(
            "refresh-large-metadata",
            RefreshReason::Initial,
            vec![crate::workspace::FileObservation::identified(
                "assets/large.txt",
                blob.clone(),
                2,
                Some(1),
                crate::model::FileSource::Filesystem,
            )],
            None,
        ),
        &index,
        &ai,
        &source,
    ))
    .unwrap();

    assert_eq!(result.indexed_blobs, vec![blob]);
    assert!(ai.embedding_operation_ids.lock().unwrap().is_empty());
    let staged = staged.lock().unwrap();
    assert_eq!(staged.len(), 1);
    assert!(matches!(
        staged[0].status,
        IndexedBlobStatus::ReadyWithoutEmbedding { .. }
    ));
}

/// Verify path-vector failure does not invalidate primary content indexing.
/// 验证路径向量失败不会使主要内容索引失效。
#[test]
fn path_embedding_failure_is_persisted_as_auxiliary_degradation() {
    let mut engine = test_engine();
    let (mut ai, index, source, _) = fake_host();
    ai.fail_path_embedding = true;
    let staged = index.staged.clone();
    block_on(engine.refresh(
        RefreshRequest::full(
            "refresh-path-failure",
            RefreshReason::Initial,
            vec![crate::workspace::FileObservation::present(
                "src/lib.rs",
                "fn main() {}",
                Some(1),
                crate::model::FileSource::Filesystem,
            )],
            None,
        ),
        &index,
        &ai,
        &source,
    ))
    .unwrap();
    let staged = staged.lock().unwrap();
    assert_eq!(staged.len(), 1);
    assert!(matches!(staged[0].status, IndexedBlobStatus::Ready));
    assert!(matches!(
        staged[0].path_status,
        AuxiliaryIndexStatus::Failed { .. }
    ));
}

/// Verify a mismatched staging receipt is rejected and its transaction is aborted.
/// 验证不匹配的暂存回执会被拒绝，并中止对应事务。
#[test]
fn invalid_stage_receipt_aborts_without_checkpoint() {
    let mut engine = test_engine();
    let (ai, mut index, source, _) = fake_host();
    index.invalid_stage_receipt = true;
    let abort_count = index.abort_count.clone();
    let error = block_on(engine.refresh(
        RefreshRequest::full(
            "refresh-invalid-stage",
            RefreshReason::Initial,
            vec![crate::workspace::FileObservation::present(
                "src/lib.rs",
                "fn main() {}",
                Some(1),
                crate::model::FileSource::Filesystem,
            )],
            None,
        ),
        &index,
        &ai,
        &source,
    ))
    .unwrap_err();
    assert!(matches!(error, VceError::ResponseIdentity { .. }));
    assert_eq!(*abort_count.lock().unwrap(), 1);
    assert!(engine.workspace_snapshot().checkpoint.is_none());
}

/// Verify a commit failure cleans unpublished staging and never advances local state.
/// 验证提交失败会清理未发布暂存，且绝不推进本地状态。
#[test]
fn commit_failure_aborts_without_checkpoint() {
    let mut engine = test_engine();
    let (ai, mut index, source, _) = fake_host();
    index.fail_commit = true;
    let abort_count = index.abort_count.clone();
    let error = block_on(engine.refresh(
        RefreshRequest::full(
            "refresh-commit-failure",
            RefreshReason::Initial,
            vec![crate::workspace::FileObservation::present(
                "src/lib.rs",
                "fn main() {}",
                Some(1),
                crate::model::FileSource::Filesystem,
            )],
            None,
        ),
        &index,
        &ai,
        &source,
    ))
    .unwrap_err();
    assert!(matches!(error, VceError::Port(_)));
    assert_eq!(*abort_count.lock().unwrap(), 1);
    assert!(engine.workspace_snapshot().checkpoint.is_none());
}

/// Verify an invalid commit receipt is rejected before local checkpoint publication.
/// 验证无效提交回执会在本地检查点发布前被拒绝。
#[test]
fn invalid_commit_receipt_aborts_without_checkpoint() {
    let mut engine = test_engine();
    let (ai, mut index, source, _) = fake_host();
    index.invalid_commit_receipt = true;
    let abort_count = index.abort_count.clone();
    let error = block_on(engine.refresh(
        RefreshRequest::full(
            "refresh-invalid-commit",
            RefreshReason::Initial,
            vec![crate::workspace::FileObservation::present(
                "src/lib.rs",
                "fn main() {}",
                Some(1),
                crate::model::FileSource::Filesystem,
            )],
            None,
        ),
        &index,
        &ai,
        &source,
    ))
    .unwrap_err();
    assert!(matches!(error, VceError::ResponseIdentity { .. }));
    assert_eq!(*abort_count.lock().unwrap(), 1);
    assert!(engine.workspace_snapshot().checkpoint.is_none());
}

/// Verify an abort acknowledgement cannot refer to another workspace generation.
/// 验证暂存中止确认不能指向另一工作区代数。
#[test]
fn rejects_abort_receipt_for_another_generation() {
    let error = validate_abort_result(
        "abort-op",
        "refresh-op",
        7,
        Ok(crate::ports::IndexAbortReceipt {
            operation_id: "abort-op".to_owned(),
            refresh_id: "refresh-op".to_owned(),
            generation: 6,
            removed_blob_count: 0,
        }),
    )
    .unwrap_err();

    assert!(matches!(
        error,
        VceError::InvalidHostResponse {
            operation: VcePortOperation::AbortStage,
            ..
        }
    ));
}

/// Verify versioned state survives JSON and remains bound to its exact namespace.
/// 验证版本化状态可通过 JSON 恢复，并始终绑定其精确命名空间。
#[test]
fn state_snapshot_round_trip_rejects_namespace_drift() {
    let mut engine = test_engine();
    let (ai, index, source, _) = fake_host();
    block_on(engine.refresh(
        RefreshRequest::full(
            "refresh-state",
            RefreshReason::Initial,
            vec![crate::workspace::FileObservation::present(
                "src/lib.rs",
                "fn main() {}",
                Some(1),
                crate::model::FileSource::Filesystem,
            )],
            None,
        ),
        &index,
        &ai,
        &source,
    ))
    .unwrap();
    let json = engine.state_snapshot().to_json().unwrap();
    let snapshot = VceStateSnapshot::from_json(&json).unwrap();
    let restored = VceEngine::from_state_snapshot(engine.config().clone(), snapshot).unwrap();
    assert_eq!(restored.status().state, VceEngineState::Ready);
    assert_eq!(restored.workspace_snapshot(), engine.workspace_snapshot());

    let mut invalid = engine.state_snapshot();
    invalid.identity.index_namespace_id = "vce:other".to_owned();
    assert!(matches!(
        VceEngine::from_state_snapshot(engine.config().clone(), invalid),
        Err(VceConfigError::StateSnapshot(_))
    ));
}

/// Verify restart recovery preserves an observable last-operation failure.
/// 验证重启恢复会保留可观测的最近操作失败状态。
#[test]
fn state_snapshot_restores_failed_lifecycle_state() {
    let mut engine = test_engine();
    let (ai, index, source, _) = fake_host();
    block_on(engine.refresh(
        RefreshRequest::full(
            "refresh-before-failure",
            RefreshReason::Initial,
            vec![crate::workspace::FileObservation::present(
                "src/lib.rs",
                "fn main() {}",
                Some(1),
                crate::model::FileSource::Filesystem,
            )],
            None,
        ),
        &index,
        &ai,
        &source,
    ))
    .unwrap();
    let error = block_on(engine.retrieve(
        RetrieveRequest::in_blobs("failed-retrieval", "find main", BTreeSet::new()),
        &index,
        &ai,
    ));
    assert!(error.is_err());
    assert_eq!(engine.status().state, VceEngineState::Failed);

    let restored =
        VceEngine::from_state_snapshot(engine.config().clone(), engine.state_snapshot()).unwrap();

    assert_eq!(restored.status().state, VceEngineState::Failed);
    assert!(restored.status().last_error.is_some());
}

/// Verify malformed host hits are rejected even when their Blob remains in scope.
/// 验证即使命中 Blob 仍在范围内，畸形宿主命中也会被拒绝。
#[test]
fn search_hit_validation_rejects_nonfinite_scores() {
    let content = "fn main() {}";
    let blob = crate::identity::blob_name("src/lib.rs", content).unwrap();
    let chunk = Chunk::new(
        blob.clone(),
        "src/lib.rs",
        content,
        1,
        1,
        ChunkKind::Recursive,
    )
    .unwrap();
    let mut hit = SearchHit::from_chunk(&chunk, f32::NAN, SearchSource::Dense, HitKind::Content);
    hit.blob_name = blob.clone();
    let error = validate_hits(
        VcePortOperation::DenseSearch,
        &[hit],
        &BTreeSet::from([blob]),
        1,
        None,
    )
    .unwrap_err();
    assert!(matches!(error, VceError::InvalidHostResponse { .. }));
}

/// Verify every AI adapter must return finite vectors in the configured physical dimension.
/// 验证每个 AI 适配器都必须按配置物理维度返回有限向量。
#[test]
fn embedding_response_rejects_invalid_vector_values() {
    let engine = test_engine();
    let input = EmbeddingInput::new("query", "find code").unwrap();
    let plan = EmbeddingPlan::new(vec![input], 1, 100, 100, 0).unwrap();
    let batch = &plan.batches[0];
    let input_id = batch.segments[0].request_input_id();
    for values in [vec![1.0], vec![f32::NAN, 1.0]] {
        let response = VceEmbeddingResponse {
            operation_id: "embedding-op".to_owned(),
            space: test_space(),
            vectors: vec![SegmentVector {
                input_id: input_id.clone(),
                values,
            }],
        };

        assert!(matches!(
            validate_embedding_response(&engine, "embedding-op", response, batch),
            Err(VceError::InvalidHostResponse {
                operation: VcePortOperation::Embedding,
                ..
            })
        ));
    }
}

/// Verify an absent configured fingerprint is not a wildcard for provider model identity.
/// 验证配置中缺省的指纹不会成为供应商模型身份的通配符。
#[test]
fn embedding_response_requires_exact_optional_fingerprint() {
    let mut expected = test_space();
    expected.physical_fingerprint = None;
    let config = VceConfig::new("code-chain", expected).unwrap();
    let engine = VceEngine::new(config).unwrap();
    let mut actual = engine.config.embedding_space.clone();
    actual.physical_fingerprint = Some("unexpected-model".to_owned());

    assert!(matches!(
        validate_embedding_space(&engine, &actual),
        Err(VceError::EmbeddingSpaceMismatch { .. })
    ));
}

/// An exact host grant changes the live response contract but preserves original persisted identity.
/// 精确宿主授权改变实时响应契约，但保留原始持久身份。
/// No inputs or outputs; validates binding, snapshot restoration, strict target checks and scope rejection.
/// 无输入输出；验证绑定、快照恢复、严格目标检查及作用域拒绝。
#[test]
fn approved_embedding_execution_preserves_original_identity() {
    // Original storage remains bound to the old producer, even after approving a new provider.
    // 即使批准新供应商，原始存储仍绑定旧生产来源。
    let mut config = VceConfig::new("code-chain", test_space()).unwrap();
    let original = VceIndexBinding::new(
        config.index_namespace_id.clone(),
        config.checkpoint_chain_id.clone(),
        config.index_schema_version,
        config.embedding_space.clone(),
    )
    .unwrap();
    let mut target = test_space();
    target.physical_fingerprint = Some(String::from("new-provider-same-model"));
    let original_engine = VceEngine::new(config.clone()).unwrap();
    let original_snapshot = original_engine.state_snapshot();
    config.embedding_execution = Some(
        crate::VceEmbeddingExecutionContract::new(
            original.clone(),
            target.clone(),
            "host-checked-direct-project-approval",
        )
        .unwrap(),
    );
    let engine = VceEngine::from_state_snapshot(config.clone(), original_snapshot.clone()).unwrap();
    assert_eq!(engine.state_snapshot().identity, original_snapshot.identity);
    assert!(engine.validate_index_binding(&original).is_ok());
    assert!(validate_embedding_space(&engine, &target).is_ok());
    assert!(validate_embedding_space(&engine, &test_space()).is_err());
    let mut rewritten = original.clone();
    rewritten.embedding_space = target.clone();
    assert!(engine.validate_index_binding(&rewritten).is_err());
    config.index_namespace_id.push_str("-other-project");
    assert_eq!(
        config.validate(),
        Err(VceConfigError::InvalidEmbeddingExecution)
    );
    for mismatch in 0..5 {
        let mut invalid = target.clone();
        match mismatch {
            0 => invalid.dimension += 1,
            1 => invalid.vector_space_id.push_str("-other-root"),
            2 => invalid.preprocessing_version.push_str("-changed"),
            3 => invalid.chunker_version.push_str("-changed"),
            _ => invalid.physical_fingerprint = None,
        }
        assert!(
            crate::VceEmbeddingExecutionContract::new(original.clone(), invalid, "approval")
                .is_err()
        );
    }
    assert!(crate::VceEmbeddingExecutionContract::new(original, target, "  ").is_err());
}

/// Verify the path channel can return only the canonical generated path document.
/// 验证路径通道只能返回规范生成的路径文档。
#[test]
fn path_search_rejects_arbitrary_self_consistent_chunks() {
    let content = "fn main() {}";
    let blob = crate::identity::blob_name("src/lib.rs", content).unwrap();
    let chunk = Chunk::new(
        blob.clone(),
        "src/lib.rs",
        content,
        1,
        1,
        ChunkKind::Recursive,
    )
    .unwrap();
    let hit = SearchHit::from_chunk(&chunk, 0.9, SearchSource::Path, HitKind::Content);

    let error = validate_path_response(
        "path-search",
        VcePathSearchResponse {
            operation_id: "path-search".to_owned(),
            hits: vec![hit],
            fallback_hits: Vec::new(),
        },
        &BTreeSet::from([blob]),
        1,
    )
    .unwrap_err();

    assert!(matches!(error, VceError::InvalidHostResponse { .. }));
}

/// Verify one Blob cannot claim multiple canonical path documents in one response.
/// 验证一个 Blob 不能在同一响应中声明多个规范路径文档。
#[test]
fn path_search_rejects_multiple_paths_for_one_blob() {
    let blob = crate::identity::content_hash("shared blob");
    let first = crate::path_index::PathDocument::new(blob.clone(), "src/a.rs")
        .unwrap()
        .to_hit(1.0)
        .unwrap();
    let second = crate::path_index::PathDocument::new(blob.clone(), "src/b.rs")
        .unwrap()
        .to_hit(0.9)
        .unwrap();

    let error = validate_path_response(
        "path-search",
        VcePathSearchResponse {
            operation_id: "path-search".to_owned(),
            hits: vec![first, second],
            fallback_hits: Vec::new(),
        },
        &BTreeSet::from([blob]),
        2,
    )
    .unwrap_err();

    assert!(matches!(error, VceError::InvalidHostResponse { .. }));
}

/// Verify optional host responses and source revisions obey their declared bounds.
/// 验证可选宿主响应与源码版本遵守其声明边界。
#[test]
fn optional_response_and_revision_bounds_are_enforced() {
    assert!(
        validate_rewrite_response(
            "rewrite",
            1,
            &VceQueryRewriteResponse {
                operation_id: "rewrite".to_owned(),
                rewrites: vec!["one".to_owned(), "two".to_owned()],
            },
        )
        .is_err()
    );
    let request = RefreshRequest::full(
        "refresh",
        RefreshReason::Manual,
        Vec::new(),
        Some("   ".to_owned()),
    );
    assert!(validate_refresh_request(&request).is_err());
}

/// Verify host search channels cannot return an ascending score sequence.
/// 验证宿主搜索通道不能返回分数升序序列。
#[test]
fn search_hit_validation_requires_descending_scores() {
    let source = "fn first() {}\nfn second() {}";
    let blob = crate::identity::blob_name("src/lib.rs", source).unwrap();
    let first = Chunk::new(
        blob.clone(),
        "src/lib.rs",
        "fn first() {}",
        1,
        1,
        ChunkKind::Recursive,
    )
    .unwrap();
    let second = Chunk::new(
        blob.clone(),
        "src/lib.rs",
        "fn second() {}",
        2,
        2,
        ChunkKind::Recursive,
    )
    .unwrap();
    let error = validate_hits(
        VcePortOperation::DenseSearch,
        &[
            SearchHit::from_chunk(&first, 0.25, SearchSource::Dense, HitKind::Content),
            SearchHit::from_chunk(&second, 0.75, SearchSource::Dense, HitKind::Content),
        ],
        &BTreeSet::from([blob]),
        2,
        None,
    )
    .unwrap_err();
    assert!(matches!(error, VceError::InvalidHostResponse { .. }));
}

/// Verify exact search accepts stable symbol occurrence identities instead of Chunk IDs.
/// 验证精确搜索接受稳定符号出现身份，而不是错误地要求 Chunk 身份。
#[test]
fn exact_search_accepts_symbol_occurrence_identity() {
    let content = "fn run() {}";
    let blob = crate::identity::blob_name("src/lib.rs", content).unwrap();
    let chunk = Chunk::new(
        blob.clone(),
        "src/lib.rs",
        content,
        1,
        1,
        ChunkKind::Recursive,
    )
    .unwrap();
    let hit = crate::symbol::SymbolOccurrence::from_chunk(
        &chunk,
        "run",
        crate::symbol::SymbolKind::Definition,
    )
    .to_hit(0.9);

    assert!(
        validate_hits(
            VcePortOperation::ExactSearch,
            std::slice::from_ref(&hit),
            &BTreeSet::from([blob.clone()]),
            1,
            Some(&["run".to_owned()]),
        )
        .is_ok()
    );
    assert!(
        validate_hits(
            VcePortOperation::DenseSearch,
            &[hit],
            &BTreeSet::from([blob]),
            1,
            None,
        )
        .is_err()
    );
}

/// Verify exact search cannot substitute an unrelated SHA for a requested symbol occurrence.
/// 验证精确搜索不能为请求的符号出现位置替换无关 SHA 身份。
#[test]
fn exact_search_rejects_forged_occurrence_identity() {
    let content = "fn run() {}";
    let blob = crate::identity::blob_name("src/lib.rs", content).unwrap();
    let chunk = Chunk::new(
        blob.clone(),
        "src/lib.rs",
        content,
        1,
        1,
        ChunkKind::Recursive,
    )
    .unwrap();
    let mut hit = crate::symbol::SymbolOccurrence::from_chunk(
        &chunk,
        "run",
        crate::symbol::SymbolKind::Definition,
    )
    .to_hit(0.9);
    hit.id = crate::identity::content_hash("forged occurrence");

    assert!(
        validate_hits(
            VcePortOperation::ExactSearch,
            &[hit],
            &BTreeSet::from([blob]),
            1,
            Some(&["run".to_owned()]),
        )
        .is_err()
    );
}

/// Verify host search hits cannot substitute an unrelated SHA-256 value as their Chunk ID.
/// 验证宿主搜索命中不能使用无关 SHA-256 值冒充 Chunk 身份。
#[test]
fn search_hit_validation_rejects_mismatched_chunk_identity() {
    let blob_name = crate::identity::blob_name("src/lib.rs", "fn main() {}").unwrap();
    let chunk = Chunk::new(
        blob_name.clone(),
        "src/lib.rs",
        "fn main() {}",
        1,
        1,
        ChunkKind::Recursive,
    )
    .unwrap();
    let mut hit = SearchHit::from_chunk(&chunk, 1.0, SearchSource::Dense, HitKind::Content);
    hit.id = crate::identity::content_hash("unrelated identity");

    let error = validate_hits(
        VcePortOperation::DenseSearch,
        &[hit],
        &BTreeSet::from([blob_name]),
        1,
        None,
    )
    .unwrap_err();

    assert!(matches!(
        error,
        VceError::InvalidHostResponse {
            operation: VcePortOperation::DenseSearch,
            ..
        }
    ));
}

/// Verify semantic kinds cannot cross from exact symbols into content-only channels.
/// 验证精确符号的语义类型不能越界进入仅内容通道。
#[test]
fn content_channels_reject_symbol_semantic_kinds() {
    let blob_name = crate::identity::blob_name("src/lib.rs", "fn main() {}").unwrap();
    let chunk = Chunk::new(
        blob_name.clone(),
        "src/lib.rs",
        "fn main() {}",
        1,
        1,
        ChunkKind::Recursive,
    )
    .unwrap();
    let scope = BTreeSet::from([blob_name.clone()]);
    let endpoint = SearchHit::from_chunk(&chunk, 1.0, SearchSource::Dense, HitKind::Endpoint);
    let error = validate_search_response(
        VcePortOperation::DenseSearch,
        "dense-1",
        VceSearchResponse {
            operation_id: "dense-1".to_owned(),
            hits: vec![endpoint.clone()],
        },
        SearchSource::Dense,
        &scope,
        1,
        None,
    )
    .unwrap_err();
    assert!(matches!(error, VceError::InvalidHostResponse { .. }));

    let path_hit = crate::path_index::PathDocument::new(blob_name, "src/lib.rs")
        .unwrap()
        .to_hit(1.0)
        .unwrap();
    let error = validate_path_response(
        "path-1",
        VcePathSearchResponse {
            operation_id: "path-1".to_owned(),
            hits: vec![path_hit],
            fallback_hits: vec![endpoint],
        },
        &scope,
        1,
    )
    .unwrap_err();
    assert!(matches!(error, VceError::InvalidHostResponse { .. }));
}

/// Verify management snapshots reject impossible lifecycle and checkpoint states.
/// 验证管理快照拒绝不可能的生命周期与检查点状态。
#[test]
fn management_snapshot_rejects_impossible_lifecycle_state() {
    let mut snapshot = VceIndexManagementSnapshot {
        operation_id: "management-test".to_owned(),
        status: VceIndexStoreStatus::Refreshing,
        generation: 1,
        checkpoint_version: None,
        counts: VceIndexCounts::default(),
        integrity: VceIndexIntegrity::default(),
        active_job: Some(VceIndexJobSummary {
            job_id: "job-test".to_owned(),
            kind: VceOperationKind::InitialBuild,
            generation: 1,
            mode: Some(RefreshMode::Full),
            reason: None,
            phase: VceOperationPhase::Embedding,
            attempt: 1,
            completed_units: 0,
            total_units: 1,
            unit: None,
            started_at_ns: None,
            finished_at_ns: None,
            coalesced: false,
            error: None,
        }),
        last_error: None,
    };
    let error = validate_index_management_snapshot(&snapshot).unwrap_err();
    assert!(matches!(error, VceError::InvalidHostResponse { .. }));

    snapshot.active_job.as_mut().unwrap().reason = Some(RefreshReason::Initial);
    assert!(validate_index_management_snapshot(&snapshot).is_ok());

    snapshot.active_job.as_mut().unwrap().phase = VceOperationPhase::Completed;
    let error = validate_index_management_snapshot(&snapshot).unwrap_err();
    assert!(matches!(error, VceError::InvalidHostResponse { .. }));

    snapshot.active_job.as_mut().unwrap().completed_units = 1;
    snapshot.active_job.as_mut().unwrap().error = Some("stale failure".to_owned());
    let error = validate_index_management_snapshot(&snapshot).unwrap_err();
    assert!(matches!(error, VceError::InvalidHostResponse { .. }));

    snapshot.active_job.as_mut().unwrap().phase = VceOperationPhase::Embedding;
    snapshot.active_job.as_mut().unwrap().completed_units = 0;
    snapshot.active_job.as_mut().unwrap().error = None;

    snapshot.status = VceIndexStoreStatus::Error;
    let error = validate_index_management_snapshot(&snapshot).unwrap_err();
    assert!(matches!(error, VceError::InvalidHostResponse { .. }));

    snapshot.status = VceIndexStoreStatus::Refreshing;
    snapshot.active_job.as_mut().unwrap().phase = VceOperationPhase::Failed;
    let error = validate_index_management_snapshot(&snapshot).unwrap_err();
    assert!(matches!(error, VceError::InvalidHostResponse { .. }));

    snapshot.active_job.as_mut().unwrap().phase = VceOperationPhase::Embedding;
    snapshot.checkpoint_version = Some(0);
    let error = validate_index_management_snapshot(&snapshot).unwrap_err();
    assert!(matches!(error, VceError::InvalidHostResponse { .. }));

    snapshot.checkpoint_version = Some(2);
    let error = validate_index_management_snapshot(&snapshot).unwrap_err();
    assert!(matches!(error, VceError::InvalidHostResponse { .. }));

    snapshot.checkpoint_version = None;
    snapshot.integrity = VceIndexIntegrity {
        index_exists: true,
        searchable: false,
        checkpoint_blob_count: 2,
        indexed_blob_count: 2,
        missing_blob_count: 1,
        orphan_blob_count: 0,
        detail: None,
    };
    let error = validate_index_management_snapshot(&snapshot).unwrap_err();
    assert!(matches!(error, VceError::InvalidHostResponse { .. }));

    snapshot.integrity = VceIndexIntegrity {
        index_exists: false,
        searchable: false,
        checkpoint_blob_count: 1,
        indexed_blob_count: 1,
        missing_blob_count: 0,
        orphan_blob_count: 0,
        detail: None,
    };
    let error = validate_index_management_snapshot(&snapshot).unwrap_err();
    assert!(matches!(error, VceError::InvalidHostResponse { .. }));
}

/// Verify local diagnostics reject capabilities exposed by an inactive registration.
/// 验证本地诊断会拒绝由未激活注册暴露的能力。
#[test]
fn local_management_snapshot_validates_ai_registration_state() {
    let engine = test_engine();
    let (mut ai, _, _, _) = fake_host();
    ai.registered = false;

    assert!(matches!(
        engine.local_management_snapshot(&ai),
        Err(VceError::AiBindingMismatch { .. })
    ));

    ai.embedding_available = false;
    ai.query_rewrite_available = false;
    ai.rerank_available = false;
    let snapshot = engine.local_management_snapshot(&ai).unwrap();
    assert!(!snapshot.ai.registered);
    assert!(snapshot.index.is_none());
}

/// Verify initial indexing groups admitted files without a separate aggregate source-byte ceiling.
/// 验证初次索引合并已准入文件，不再受独立的源码总字节上限限制。
#[test]
fn staging_batches_group_admitted_files_by_count() {
    let mut config = VceConfig::new("code-chain", test_space()).unwrap();
    config.indexing.chunker.max_chars = 100;
    config.indexing.chunker.min_chars = 1;
    config.indexing.max_file_bytes = 16;
    config.index_stage_blob_limit = 8;
    let mut engine = VceEngine::new(config).unwrap();
    let (ai, index, source, _) = fake_host();
    let staged = index.staged.clone();
    let stage_call_count = index.stage_call_count.clone();
    block_on(engine.refresh(
        RefreshRequest::full(
            "refresh-byte-batches",
            RefreshReason::Initial,
            vec![
                crate::workspace::FileObservation::present(
                    "src/a.rs",
                    "fn first() {}",
                    Some(1),
                    crate::model::FileSource::Filesystem,
                ),
                crate::workspace::FileObservation::present(
                    "src/b.rs",
                    "fn second() {}",
                    Some(1),
                    crate::model::FileSource::Filesystem,
                ),
            ],
            None,
        ),
        &index,
        &ai,
        &source,
    ))
    .unwrap();
    let staged = staged.lock().unwrap();
    assert_eq!(*stage_call_count.lock().unwrap(), 1);
    assert_eq!(staged.len(), 2);
}

/// Verify every Blob and query variant receives distinct host Embedding operation identities.
/// 验证每个 Blob 与查询变体都会获得不同的宿主向量操作标识。
#[test]
fn embedding_operation_identities_are_unique_across_blobs() {
    let mut engine = test_engine();
    let (ai, index, source, _) = fake_host();
    let operation_ids = ai.embedding_operation_ids.clone();
    block_on(engine.refresh(
        RefreshRequest::full(
            "refresh-identities",
            RefreshReason::Initial,
            vec![
                crate::workspace::FileObservation::present(
                    "src/lib.rs",
                    "fn first() {}",
                    Some(1),
                    crate::model::FileSource::Filesystem,
                ),
                crate::workspace::FileObservation::present(
                    "src/main.rs",
                    "fn second() {}",
                    Some(1),
                    crate::model::FileSource::Filesystem,
                ),
            ],
            None,
        ),
        &index,
        &ai,
        &source,
    ))
    .unwrap();
    let operation_ids = operation_ids.lock().unwrap();
    assert_eq!(
        operation_ids.iter().collect::<BTreeSet<_>>().len(),
        operation_ids.len()
    );
}

/// Verify public AI binding fields are revalidated before any host operation starts.
/// 验证任何宿主操作开始前都会重新校验公开 AI 绑定字段。
#[test]
fn empty_ai_binding_identity_is_rejected_at_the_engine_boundary() {
    let mut engine = test_engine();
    let (mut ai, index, source, _) = fake_host();
    ai.binding.registration_id.clear();

    let error = block_on(engine.refresh(
        RefreshRequest::full(
            "refresh-empty-ai-binding",
            RefreshReason::Manual,
            Vec::new(),
            None,
        ),
        &index,
        &ai,
        &source,
    ))
    .unwrap_err();

    assert!(matches!(error, VceError::AiBindingMismatch { .. }));
}

/// Verify an index namespace mismatch is rejected before any staging mutation.
/// 验证索引命名空间不匹配会在任何暂存修改前被拒绝。
#[test]
fn index_binding_mismatch_fails_before_staging() {
    let mut engine = test_engine();
    let (ai, mut index, source, _) = fake_host();
    index.binding.namespace_id = "vce:other".to_owned();
    let staged = index.staged.clone();
    let error = block_on(engine.refresh(
        RefreshRequest::full(
            "refresh-wrong-index",
            RefreshReason::Initial,
            Vec::new(),
            None,
        ),
        &index,
        &ai,
        &source,
    ))
    .unwrap_err();
    assert!(matches!(error, VceError::IndexBindingMismatch { .. }));
    assert!(staged.lock().unwrap().is_empty());
}
