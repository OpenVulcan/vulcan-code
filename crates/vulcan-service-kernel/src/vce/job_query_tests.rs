//! Real VCE storage interleaving with a private, explicitly controlled HostAI test boundary.
//! 使用私有、明确可控的 HostAI 测试边界验证真实 VCE 存储交错。

use super::super::runtime_configuration;
use super::tests::{await_test_job, job_test_capabilities};
use super::{
    VceActiveRefresh, VceJobAdmission, VceJobWork, VceRebuildProjectInput, VceRuntimeManager,
};
use crate::ServiceEventEmitter;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;
use tokio::sync::Semaphore;
use vulcan_ai_contract::test_support::MockHostAiPort;
use vulcan_ai_contract::*;
use vulcan_workbench_model::{ProjectVceSettings, ProjectVceUpdateMode};
use vulcan_workspace_store::vce_data_plane::{VceDataPlaneFactory, VceJobKind, VceJobState};

/// One explicitly armed model-call pause, with independent arrival and release signals.
/// 一个明确启用的模型调用暂停点，具有独立到达及释放信号。
struct ModelCallGate {
    /// Only the next selected call pauses; other calls remain independent.
    /// 仅暂停下一次选中调用，其他调用保持独立。
    armed: AtomicBool,
    /// Arrival proof is buffered, avoiding notification races in the test driver.
    /// 缓存到达证明，避免测试驱动中的通知竞态。
    entered: Semaphore,
    /// Explicit completion permission, unrelated to elapsed wall time.
    /// 明确完成许可，与墙钟经过时间无关。
    released: Semaphore,
}

impl ModelCallGate {
    /// Construct an unarmed gate with no permits; returns its shared ownership.
    /// 构造未启用且没有许可的闸门；返回其共享所有权。
    fn new() -> Arc<Self> {
        Arc::new(Self {
            armed: AtomicBool::new(false),
            entered: Semaphore::new(0),
            released: Semaphore::new(0),
        })
    }

    /// Arm the next call; returns a guard that also releases it if an assertion unwinds.
    /// 启用下一次调用；返回在断言展开时也会释放调用的守卫。
    fn arm(self: &Arc<Self>) -> PausedModelCall {
        assert!(!self.armed.swap(true, Ordering::AcqRel));
        assert_eq!(self.released.available_permits(), 0);
        PausedModelCall {
            gate: Arc::clone(self),
        }
    }

    /// Await actual model entry, returning only after consuming its exact signal.
    /// 等待实际进入模型，仅在消费其精确信号后返回。
    async fn wait_entered(&self) {
        tokio::time::timeout(Duration::from_secs(10), self.entered.acquire())
            .await
            .expect("model call must arrive before deadline")
            .expect("arrival gate stays open")
            .forget();
    }

    /// Pause the selected `context`, returning explicit cancellation or permission to finish.
    /// 暂停选中的 `context`，返回明确取消或完成许可。
    async fn enter(&self, context: &HostAiCallContext) -> Result<(), HostAiError> {
        if self.armed.swap(false, Ordering::AcqRel) {
            self.entered.add_permits(1);
            tokio::select! {
                permit = self.released.acquire() => permit.expect("release gate stays open").forget(),
                () = context.cancellation.cancelled() => return Err(HostAiError::new(
                    HostAiErrorCode::Cancelled, "Controlled fixture call was cancelled",
                )),
            }
        }
        Ok(())
    }
}

/// Test-owned pause lease ensuring assertion failure cannot leave a model call blocked forever.
/// 测试持有的暂停租约，确保断言失败不能使模型调用永久阻塞。
struct PausedModelCall {
    /// Exact selected gate, never a conversation-wide cancellation mechanism.
    /// 精确选中的闸门，绝不是会话级取消机制。
    gate: Arc<ModelCallGate>,
}

impl Drop for PausedModelCall {
    /// Release this pause on normal drop or panic; takes no parameters and returns no value.
    /// 正常释放或异常展开时放开此暂停；无参数及返回值。
    fn drop(&mut self) {
        self.gate.armed.store(false, Ordering::Release);
        self.gate.released.add_permits(1);
    }
}

/// VCE-private adapter retaining shared mock validation while producing deterministic dense vectors.
/// VCE 私有适配器，保留共享模拟校验并生成确定性稠密向量。
struct ControlledHostAi {
    /// Existing authoritative fixture registration and typed-response validation.
    /// 既有权威夹具注册及类型化响应校验。
    mock: MockHostAiPort,
    /// Exact route from the existing capability fixture, not guessed provider metadata.
    /// 既有能力夹具中的精确路由，不猜测供应商元数据。
    route: ExecutionRouteSnapshot,
    /// Only document calls use this pause, leaving queries independently callable.
    /// 仅文档调用使用此暂停，查询可独立执行。
    documents: Arc<ModelCallGate>,
    /// A separate selected query can remain on an old checkpoint across publication.
    /// 单独选中的查询可跨发布继续使用旧检查点。
    queries: Arc<ModelCallGate>,
    /// Exact embedding invocations, including attempts before any test pause or validation.
    /// 精确向量调用次数，包含任何测试暂停或校验之前的尝试。
    embedding_calls: AtomicUsize,
}

impl ControlledHostAi {
    /// Construct local AI from explicit `capabilities`; returns no network-backed provider.
    /// 根据明确的 `capabilities` 构造本地模型边界；不返回网络供应商。
    fn new(capabilities: InferenceCapabilitySnapshot) -> Self {
        // route belongs uniquely to the fixture's general embedding operation.
        // route 唯一属于夹具的通用向量操作。
        let route = capabilities
            .operations
            .iter()
            .find(|operation| {
                operation.operation == AiOperation::EmbeddingCreate
                    && operation.purpose_id.is_none()
            })
            .expect("fixture declares embedding")
            .route
            .clone()
            .expect("fixture declares exact route");
        let mock = MockHostAiPort::new();
        mock.set_capability_snapshot(capabilities)
            .expect("set local capability truth");
        Self {
            mock,
            route,
            documents: ModelCallGate::new(),
            queries: ModelCallGate::new(),
            embedding_calls: AtomicUsize::new(0),
        }
    }
}

#[async_trait::async_trait]
impl HostAiPort for ControlledHostAi {
    /// Delegate `registration`; returns the shared mock's validated capability handle or error.
    /// 委托 `registration`；返回共享模拟器校验的能力句柄或错误。
    fn register_capability(
        &self,
        registration: HostAiCapabilityRegistration,
    ) -> Result<HostAiCapabilityHandle, HostAiError> {
        self.mock.register_capability(registration)
    }

    /// Resolve `registration_id`; returns its existing handle or an explicit missing error.
    /// 解析 `registration_id`；返回既有句柄或明确缺失错误。
    fn capability_handle(
        &self,
        registration_id: &str,
    ) -> Result<HostAiCapabilityHandle, HostAiError> {
        self.mock.capability_handle(registration_id)
    }

    /// Unregister exact `registration_id`; returns the delegated cleanup result.
    /// 注销精确 `registration_id`；返回委托清理结果。
    fn unregister_capability(&self, registration_id: &str) -> Result<(), HostAiError> {
        self.mock.unregister_capability(registration_id)
    }

    /// Refresh `handle`; returns existing declared capability truth without external calls.
    /// 刷新 `handle`；返回既有声明能力事实，不进行外部调用。
    fn capability_snapshot(
        &self,
        handle: &HostAiCapabilityHandle,
    ) -> Result<InferenceCapabilitySnapshot, HostAiError> {
        self.mock.capability_snapshot(handle)
    }

    /// Execute typed `request` under `handle` and `context`; returns exact matching three-dimensional vectors.
    /// 在 `handle` 与 `context` 下执行类型化 `request`；返回精确匹配的三维向量。
    async fn create_embeddings(
        &self,
        handle: &HostAiCapabilityHandle,
        context: HostAiCallContext,
        request: HostAiEmbeddingRequest,
    ) -> Result<HostAiEmbeddingResponse, HostAiError> {
        self.embedding_calls.fetch_add(1, Ordering::Relaxed);
        match request.request.task {
            EmbeddingTask::Document => self.documents.enter(&context).await?,
            EmbeddingTask::Query => self.queries.enter(&context).await?,
            _ => {
                return Err(HostAiError::new(
                    HostAiErrorCode::InvalidRequest,
                    "Fixture only declares document and query tasks",
                ));
            }
        }
        // response matches each actual input ID; shared validation still checks counts, dimensions and permissions.
        // response 匹配每个实际输入编号；共享校验仍检查数量、维度及权限。
        let response = EmbeddingResponse {
            request_id: request.request.request_id.clone(),
            route: self.route.clone(),
            fingerprint: EmbeddingSpaceFingerprint {
                provider_id: self.route.provider_id.clone(),
                service_entry_id: self.route.service_entry_id.clone(),
                deployment_id: Some(self.route.connection_id.clone()),
                secondary_route_id: None,
                model_id: self.route.model_id.clone(),
                binding_id: self
                    .route
                    .protocol_binding_id
                    .clone()
                    .expect("fixture protocol identity"),
                capability_revision: self.route.capability_revision.clone(),
                task: request.request.task,
                dimensions: 3,
                output_kind: EmbeddingOutputKind::Dense,
                encoding: EmbeddingEncoding::Float,
                normalization: EmbeddingNormalization::Unknown,
                distance_metric: DistanceMetric::Unknown,
            },
            results: request
                .request
                .inputs
                .iter()
                .enumerate()
                .map(|(index, input)| EmbeddingResult {
                    input_id: input.input_id.clone(),
                    input_index: u32::try_from(index).expect("fixture input count fits protocol"),
                    embedding: EmbeddingPayload::Dense {
                        values: vec![1.0, 0.0, 0.0],
                    },
                })
                .collect(),
            dropped: Vec::new(),
            usage: RequestUsage::default(),
            duration_ms: 0,
        };
        self.mock.set_embedding_response(response)?;
        self.mock.create_embeddings(handle, context, request).await
    }

    /// Delegate typed reranking inputs; returns the fixture result or explicit missing-fixture error.
    /// 委托类型化重排输入；返回夹具结果或明确缺失夹具错误。
    async fn rerank_documents(
        &self,
        handle: &HostAiCapabilityHandle,
        context: HostAiCallContext,
        request: RerankRequest,
    ) -> Result<RerankResponse, HostAiError> {
        self.mock.rerank_documents(handle, context, request).await
    }

    /// Delegate typed response inputs; returns the original execution handle without alternate routing.
    /// 委托类型化响应输入；返回原执行句柄，不进行替代路由。
    async fn start_response_execution(
        &self,
        handle: &HostAiCapabilityHandle,
        context: HostAiCallContext,
        request: ResponsesExecutionRequest,
    ) -> Result<HostAiExecutionHandle, HostAiError> {
        self.mock
            .start_response_execution(handle, context, request)
            .await
    }

    /// Read exact execution `handle`; returns its delegated live status.
    /// 读取精确执行 `handle`；返回委托的实时状态。
    fn execution_status(
        &self,
        handle: &HostAiExecutionHandle,
    ) -> Result<HostAiExecutionSnapshot, HostAiError> {
        self.mock.execution_status(handle)
    }

    /// Open exact execution `handle`; returns the delegated normalized event stream.
    /// 打开精确执行 `handle`；返回委托的归一化事件流。
    fn stream_response(
        &self,
        handle: &HostAiExecutionHandle,
    ) -> Result<HostAiEventStream, HostAiError> {
        self.mock.stream_response(handle)
    }

    /// Await exact execution `handle`; returns the delegated terminal response or cancellation.
    /// 等待精确执行 `handle`；返回委托的终态响应或取消。
    async fn wait_response(
        &self,
        handle: &HostAiExecutionHandle,
    ) -> Result<ResponsesExecutionResult, HostAiError> {
        self.mock.wait_response(handle).await
    }

    /// Cancel only execution `handle`; returns the delegated cooperative cancellation result.
    /// 仅取消执行 `handle`；返回委托的协作取消结果。
    fn cancel_execution(&self, handle: &HostAiExecutionHandle) -> Result<(), HostAiError> {
        self.mock.cancel_execution(handle)
    }
}

/// Verify nonempty initialization and overlapping old/new queries on `factory` within isolated `project_root`.
/// 在隔离的 `project_root` 内通过 `factory` 验证非空初始化及新旧查询交错。
/// `executor` owns all workers; returns after cancelling no chat, completing writes and draining this manager.
/// `executor` 持有全部工作线程；不取消任何聊天，完成写入并排空本管理器后返回。
pub(super) async fn verify_nonempty_query_interleaving(
    factory: VceDataPlaneFactory,
    project_root: PathBuf,
    executor: Arc<tokio::runtime::Runtime>,
) {
    // Capabilities and identity are the same explicit fixture used by the existing real-engine suite.
    // 能力与身份使用既有真实引擎测试组的相同明确夹具。
    let capabilities = job_test_capabilities();
    let fingerprint = crate::vce::vce_embedding_physical_fingerprint(&capabilities, 3)
        .expect("fixture fingerprint");
    let ai = Arc::new(ControlledHostAi::new(capabilities));
    // Retain a storage factory handle for the cold-owner recovery checks below.
    // 为下面的冷态所有者恢复检查保留一个存储工厂句柄。
    let cold_factory = factory.clone();
    let expected_fingerprint = fingerprint.clone();
    let manager = VceRuntimeManager::new(
        ai.clone(),
        factory,
        ServiceEventEmitter::new(Vec::new()),
        Arc::new(move |contract| {
            if matches!(
                contract.project_id.as_str(),
                "nonempty-project" | "cold-cancel-project"
            ) && contract.dimension == 3
                && contract.embedding_fingerprint == expected_fingerprint
            {
                Ok(())
            } else {
                Err(String::from("Nonempty fixture publication contract changed").into())
            }
        }),
    );
    let settings = ProjectVceSettings {
        enabled: true,
        update_mode: ProjectVceUpdateMode::Manual,
        allow_query_rewrite: false,
        allow_rerank: false,
        ..ProjectVceSettings::default()
    };
    std::fs::create_dir_all(&project_root).expect("create isolated nonempty source");
    let source_path = project_root.join("lib.rs");
    std::fs::write(
        &source_path,
        "pub fn generation_marker() -> &'static str { \"vce_old_generation\" }\n",
    )
    .expect("write initial source");
    manager
        .enable_project(
            "nonempty-project",
            &project_root,
            settings.clone(),
            3,
            &fingerprint,
            true,
        )
        .await
        .expect("prepare explicit project without scanning");
    // Initial indexing must expose progress but no query admission until the first committed checkpoint.
    // 初次索引必须暴露进度，但首次检查点提交前不得准入查询。
    let initial_pause = ai.documents.arm();
    let initial = manager
        .submit_job(
            "nonempty-project",
            VceJobAdmission {
                retry_of_job_id: None,
                kind: VceJobKind::Initialize,
                idempotency_key: String::from("nonempty-initial"),
                expected_settings_revision: settings.revision,
                expected_checkpoint_version: None,
                contract_fingerprint: fingerprint.clone(),
            },
            &executor,
        )
        .await
        .expect("queue nonempty initialization");
    ai.documents.wait_entered().await;
    assert!(!manager.project_can_serve_now("nonempty-project", settings.revision, &fingerprint));
    assert!(
        manager
            .retrieve("nonempty-project", "generation_marker")
            .await
            .is_err()
    );
    assert_eq!(
        manager
            .get_job("nonempty-project", Some(&initial.job_id))
            .await
            .expect("initial receipt")
            .expect("receipt exists")
            .state,
        VceJobState::Running
    );
    drop(initial_pause);
    let initialized = await_test_job(&manager, "nonempty-project", &initial.job_id).await;
    assert_eq!(
        initialized.state,
        VceJobState::Succeeded,
        "initial diagnostic: {initialized:?}"
    );
    let baseline = manager
        .retrieve("nonempty-project", "generation_marker")
        .await
        .expect("query initial nonempty index");
    assert!(
        baseline
            .result
            .formatted_context
            .contains("vce_old_generation")
    );
    assert!(
        baseline.result.degraded_features.is_empty(),
        "baseline: {:?}",
        baseline.result.degraded_features
    );
    // A durable initialize receipt must not retire a binding after a checkpoint is published.
    // 检查点发布后，持久化的初始化回执不得再退役绑定。
    let published_runtime = manager
        .require_project("nonempty-project")
        .await
        .expect("published project runtime");
    let mut published_attempt = initialized.clone();
    published_attempt.job_id = String::from("published-cancel-guard");
    published_attempt.owner_run_id = String::from("published-cancel-guard-owner");
    published_attempt.idempotency_key = String::from("published-cancel-guard-request");
    // Durable admission always registers a fresh receipt as queued before advancing it.
    // 持久准入始终先以排队状态注册新回执，再推进到运行状态。
    published_attempt.state = VceJobState::Queued;
    published_attempt.sequence = 1;
    published_attempt.created_at_unix_ms = initialized.created_at_unix_ms.saturating_add(1);
    published_attempt.updated_at_unix_ms = published_attempt.created_at_unix_ms;
    published_attempt.source_checkpoint_version = None;
    published_attempt.replacement = None;
    published_attempt.result_checkpoint_version = None;
    published_attempt.progress = None;
    published_attempt.followup_requested = false;
    published_attempt.completed_update = None;
    published_attempt.error = None;
    published_runtime
        .index
        .job_store()
        .register(&published_attempt)
        .await
        .expect("register published checkpoint cancellation guard");
    let published_queued = published_attempt.clone();
    published_attempt.state = VceJobState::Running;
    published_attempt.sequence = published_queued.sequence.saturating_add(1);
    published_attempt.updated_at_unix_ms = published_queued.updated_at_unix_ms.saturating_add(1);
    published_runtime
        .index
        .job_store()
        .update(&published_queued, &published_attempt)
        .await
        .expect("start published checkpoint cancellation guard");
    // A stopped executor can now be settled without retiring an already published binding.
    // 现在可以收敛已停止执行者，而不退役已经发布的绑定。
    let settled = manager
        .cancel_job("nonempty-project", &published_attempt.job_id)
        .await
        .expect("settle cold attempt without deleting published content");
    assert_eq!(settled.state, VceJobState::Interrupted);
    assert!(settled.result_checkpoint_version.is_none());
    // Exact committed query content must survive recovery of the unrelated unfinished attempt.
    // 恢复不相关的未完成尝试后，精确已提交查询内容必须保留。
    let preserved = manager
        .retrieve("nonempty-project", "generation_marker")
        .await
        .expect("query preserved checkpoint");
    assert_eq!(preserved.checkpoint_version, baseline.checkpoint_version);
    assert!(
        preserved
            .result
            .formatted_context
            .contains("vce_old_generation")
    );
    // A suspended old query already owns its revision, engine and read-generation lease.
    // 暂停的旧查询已持有自身修订、引擎及读取代次租约。
    let query_pause = ai.queries.arm();
    let querying_manager = manager.clone();
    let old_query = executor.spawn(async move {
        querying_manager
            .retrieve("nonempty-project", "generation_marker")
            .await
    });
    ai.queries.wait_entered().await;
    std::fs::write(
        &source_path,
        "pub fn generation_marker() -> &'static str { \"vce_new_generation\" }\n",
    )
    .expect("write next source generation");
    let update_pause = ai.documents.arm();
    let update = manager
        .submit_job(
            "nonempty-project",
            VceJobAdmission {
                retry_of_job_id: None,
                kind: VceJobKind::Synchronize,
                idempotency_key: String::from("nonempty-update"),
                expected_settings_revision: settings.revision,
                expected_checkpoint_version: Some(baseline.checkpoint_version),
                contract_fingerprint: fingerprint.clone(),
            },
            &executor,
        )
        .await
        .expect("queue update alongside old query");
    ai.documents.wait_entered().await;
    // Changed budgets affect new queries and future batches without cancelling either in-flight operation.
    // 改变的预算影响新查询及未来批次，不取消任何一个执行中操作。
    let mut updated_settings = settings.clone();
    updated_settings.revision += 1;
    updated_settings.max_results = 1;
    updated_settings.max_context_chars = 2_048;
    updated_settings.embedding_max_concurrency = 2;
    assert!(
        manager
            .reconcile_policy_revision(
                "nonempty-project",
                &project_root,
                &updated_settings,
                &fingerprint
            )
            .await
            .expect("reconcile during real model waits")
    );
    let during = tokio::time::timeout(
        Duration::from_secs(10),
        manager.retrieve("nonempty-project", "generation_marker"),
    )
    .await
    .expect("query must not wait for indexing or another query")
    .expect("read old checkpoint during update");
    assert_eq!(during.checkpoint_version, baseline.checkpoint_version);
    assert_eq!(during.settings_revision, updated_settings.revision);
    assert_eq!(during.result.hits.len(), 1);
    assert!(
        during
            .result
            .formatted_context
            .contains("vce_old_generation")
    );
    assert!(during.result.formatted_context.chars().count() <= updated_settings.max_context_chars);
    assert!(during.result.degraded_features.is_empty());
    assert!(manager.project_can_serve_now(
        "nonempty-project",
        updated_settings.revision,
        &fingerprint
    ));
    drop(update_pause);
    let completed = await_test_job(&manager, "nonempty-project", &update.job_id).await;
    assert_eq!(
        completed.state,
        VceJobState::Succeeded,
        "update diagnostic: {completed:?}"
    );
    let after = manager
        .retrieve("nonempty-project", "generation_marker")
        .await
        .expect("query new publication");
    assert!(after.checkpoint_version > baseline.checkpoint_version);
    assert_eq!(after.settings_revision, updated_settings.revision);
    assert!(
        after
            .result
            .formatted_context
            .contains("vce_new_generation")
    );
    assert!(
        !after
            .result
            .formatted_context
            .contains("vce_old_generation")
    );
    assert!(after.result.degraded_features.is_empty());
    // Only now release the old model result; its old source and policy must remain internally consistent.
    // 仅在此时释放旧模型结果；其旧源码及策略仍必须保持内部一致。
    drop(query_pause);
    let old = tokio::time::timeout(Duration::from_secs(10), old_query)
        .await
        .expect("old query completion deadline")
        .expect("old query worker joined")
        .expect("old query succeeds after new publication");
    assert_eq!(old.checkpoint_version, baseline.checkpoint_version);
    assert_eq!(old.settings_revision, settings.revision);
    assert!(old.result.formatted_context.contains("vce_old_generation"));
    assert!(!old.result.formatted_context.contains("vce_new_generation"));
    assert!(old.result.degraded_features.is_empty());
    // successful_update is authoritative freshness; cancelling a later attempt must not overwrite it.
    // successful_update 是权威新鲜度；取消后续尝试不能覆盖它。
    let successful_update = manager
        .last_successful_update("nonempty-project")
        .await
        .expect("read confirmed update time")
        .expect("published update has freshness");
    // survivor_pause retains a query independently from the next write job's cancellation token.
    // survivor_pause 保留独立查询，与下一写入任务的取消令牌分离。
    let survivor_pause = ai.queries.arm();
    let survivor_manager = manager.clone();
    let survivor = executor.spawn(async move {
        survivor_manager
            .retrieve("nonempty-project", "generation_marker")
            .await
    });
    ai.queries.wait_entered().await;
    std::fs::write(
        &source_path,
        "pub fn generation_marker() -> &'static str { \"vce_cancelled_generation\" }\n",
    )
    .expect("write source for cancelled attempt");
    // cancellation_pause represents an already dispatched request that has not acknowledged completion.
    // cancellation_pause 表示已经发出、尚未确认完成的请求。
    let cancellation_pause = ai.documents.arm();
    let cancellation_job = manager
        .submit_job(
            "nonempty-project",
            VceJobAdmission {
                retry_of_job_id: None,
                kind: VceJobKind::Synchronize,
                idempotency_key: String::from("nonempty-cancel"),
                expected_settings_revision: updated_settings.revision,
                expected_checkpoint_version: Some(after.checkpoint_version),
                contract_fingerprint: fingerprint.clone(),
            },
            &executor,
        )
        .await
        .expect("queue cancellable nonempty update");
    ai.documents.wait_entered().await;
    // Exercise real user stop/cancel services while this exact model request remains suspended.
    // 在此精确模型请求保持暂停时，验证真实用户停止与取消服务。
    crate::tests::vce_conversation_isolation_tests::verify_stop_and_index_cancellation(
        manager.clone(),
        project_root.clone(),
        updated_settings.clone(),
        cancellation_job.job_id.clone(),
    )
    .await;
    let cancelling = manager
        .get_job("nonempty-project", Some(&cancellation_job.job_id))
        .await
        .expect("read exact model-waiting job")
        .expect("durable cancelling receipt");
    assert_eq!(cancelling.state, VceJobState::Cancelling);
    assert!(manager.project_can_serve_now(
        "nonempty-project",
        updated_settings.revision,
        &fingerprint
    ));
    // during_cancel proves a pending external response cannot block unrelated committed-index queries.
    // during_cancel 证明等待外部响应不能阻塞无关的已提交索引查询。
    let during_cancel = tokio::time::timeout(
        Duration::from_secs(10),
        manager.retrieve("nonempty-project", "generation_marker"),
    )
    .await
    .expect("cancelling update must not block queries")
    .expect("query during cancellation");
    assert_eq!(during_cancel.checkpoint_version, after.checkpoint_version);
    assert!(
        during_cancel
            .result
            .formatted_context
            .contains("vce_new_generation")
    );
    assert!(
        !during_cancel
            .result
            .formatted_context
            .contains("vce_cancelled_generation")
    );
    assert!(during_cancel.result.degraded_features.is_empty());
    // The real runner drains this acknowledged response before marking cancellation terminal and clearing staging.
    // 真实运行器在把取消标为终态及清理暂存前，排空这个已确认的响应。
    drop(cancellation_pause);
    let cancelled = await_test_job(&manager, "nonempty-project", &cancellation_job.job_id).await;
    assert_eq!(
        cancelled.state,
        VceJobState::Cancelled,
        "cancel diagnostic: {cancelled:?}"
    );
    assert_eq!(
        manager
            .last_successful_update("nonempty-project")
            .await
            .expect("freshness after cancellation"),
        Some(successful_update)
    );
    drop(survivor_pause);
    let survived = tokio::time::timeout(Duration::from_secs(10), survivor)
        .await
        .expect("unrelated query must finish")
        .expect("join surviving query")
        .expect("index cancellation cannot cancel a query");
    assert_eq!(survived.checkpoint_version, after.checkpoint_version);
    assert_eq!(survived.settings_revision, updated_settings.revision);
    assert!(
        survived
            .result
            .formatted_context
            .contains("vce_new_generation")
    );
    assert!(
        !survived
            .result
            .formatted_context
            .contains("vce_cancelled_generation")
    );
    assert!(survived.result.degraded_features.is_empty());
    // A host restart may leave an uninitialized receipt without an in-memory job owner.
    // 宿主重启可能留下没有内存作业所有者的未初始化持久回执。
    let cold_root = project_root
        .parent()
        .expect("nonempty fixture has a temporary parent")
        .join("cold-cancel-source");
    std::fs::create_dir_all(&cold_root).expect("create cold recovery source");
    manager
        .enable_project(
            "cold-cancel-project",
            &cold_root,
            settings.clone(),
            3,
            &fingerprint,
            true,
        )
        .await
        .expect("prepare cold recovery project");
    let previous_config = cold_factory
        .load_active_project_config("cold-cancel-project")
        .await
        .expect("read cold active binding")
        .expect("cold project binding exists");
    let cold_runtime = manager
        .require_project("cold-cancel-project")
        .await
        .expect("cold recovery runtime");
    let mut first_receipt = vulcan_workspace_store::vce_data_plane::VceStoredJob {
        schema_version: vulcan_workspace_store::vce_data_plane::VCE_JOB_RECEIPT_SCHEMA_VERSION,
        project_id: String::from("cold-cancel-project"),
        job_id: String::from("cold-cancel-first"),
        owner_run_id: String::from("cold-recovery-owner"),
        idempotency_key: String::from("cold-cancel-first-request"),
        retry_of_job_id: None,
        request_fingerprint: "a".repeat(64),
        // Durable receipts store the raw indexing contract identity, not the tagged physical route fingerprint.
        // 持久回执保存原始索引契约身份，而不是带类型前缀的物理路由指纹。
        contract_fingerprint: super::job_index_contract_fingerprint(
            &cold_runtime.config,
            &cold_runtime.settings,
        )
        .expect("compute cold job contract fingerprint"),
        kind: VceJobKind::Initialize,
        // Registering through the durable admission contract requires a fresh queued receipt.
        // 按持久准入契约注册时必须使用全新的排队回执。
        state: VceJobState::Queued,
        sequence: 1,
        created_at_unix_ms: 1_000,
        updated_at_unix_ms: 1_000,
        admitted_settings_revision: settings.revision,
        source_checkpoint_version: None,
        replacement: None,
        compaction: None,
        result_checkpoint_version: None,
        progress: None,
        followup_requested: false,
        completed_update: None,
        error: None,
    };
    cold_runtime
        .index
        .job_store()
        .register(&first_receipt)
        .await
        .expect("register orphaned first initialization");
    let first_queued = first_receipt.clone();
    first_receipt.state = VceJobState::Running;
    first_receipt.sequence = first_queued.sequence.saturating_add(1);
    first_receipt.updated_at_unix_ms = first_queued.updated_at_unix_ms.saturating_add(1);
    cold_runtime
        .index
        .job_store()
        .update(&first_queued, &first_receipt)
        .await
        .expect("start orphaned first initialization");
    // A competing writer must block recovery before any receipt transition or binding retirement.
    // 竞争写者必须在任何回执转换或绑定退役之前阻止恢复。
    let competing_writer = cold_factory
        .try_acquire_project_writer("cold-cancel-project")
        .expect("acquire competing cold writer")
        .expect("cold writer is initially available");
    assert!(
        manager
            .cancel_job("cold-cancel-project", &first_receipt.job_id)
            .await
            .is_err()
    );
    drop(competing_writer);
    let first_cancelled = manager
        .cancel_job("cold-cancel-project", &first_receipt.job_id)
        .await
        .expect("cancel orphaned first initialization");
    assert_eq!(first_cancelled.state, VceJobState::Cancelled);
    assert!(
        cold_factory
            .load_active_project_config("cold-cancel-project")
            .await
            .expect("read fenced cold binding")
            .is_none()
    );
    assert_eq!(
        cold_factory
            .load_uninitialized_recovery_config("cold-cancel-project")
            .await
            .expect("read first cold recovery candidate")
            .expect("first orphan remains recoverable")
            .binding
            .namespace_id,
        previous_config.binding.namespace_id
    );
    // A second failed initialization must be independently cancellable and must use a fresh namespace.
    // 第二次失败的初始化也必须可以独立取消，并且必须使用新的命名空间。
    let mut same_physical_config = previous_config.clone();
    same_physical_config.binding.namespace_id = String::from("cold-retry-same-physical");
    same_physical_config.binding.checkpoint_chain_id = String::from("cold-retry-same-chain");
    assert!(
        cold_factory
            .reinitialize_uninitialized_project_index(&previous_config, same_physical_config,)
            .await
            .is_err()
    );
    let second_job_id = "cold-cancel-second";
    let replacement_config = runtime_configuration::prepare_runtime_configuration(
        "cold-cancel-project",
        &cold_root,
        &settings,
        3,
        &fingerprint,
        Some(second_job_id),
    )
    .expect("prepare fresh replacement configuration")
    .index;
    assert_ne!(
        previous_config.vector_space_id,
        replacement_config.vector_space_id,
    );
    let (replacement_index, replacement_writer) = cold_factory
        .reinitialize_uninitialized_project_index(&previous_config, replacement_config.clone())
        .await
        .expect("reinitialize from first orphan");
    let mut second_receipt = vulcan_workspace_store::vce_data_plane::VceStoredJob {
        job_id: String::from(second_job_id),
        owner_run_id: String::from("cold-recovery-owner-2"),
        idempotency_key: String::from("cold-cancel-second-request"),
        created_at_unix_ms: 2_000,
        updated_at_unix_ms: 2_000,
        ..first_receipt.clone()
    };
    second_receipt.state = VceJobState::Queued;
    second_receipt.sequence = 1;
    second_receipt.updated_at_unix_ms = second_receipt.created_at_unix_ms;
    let second_queued = second_receipt.clone();
    replacement_index
        .job_store()
        .register(&second_receipt)
        .await
        .expect("register orphaned second initialization");
    second_receipt.state = VceJobState::Running;
    second_receipt.sequence = second_queued.sequence.saturating_add(1);
    second_receipt.updated_at_unix_ms = second_queued.updated_at_unix_ms.saturating_add(1);
    replacement_index
        .job_store()
        .update(&second_queued, &second_receipt)
        .await
        .expect("start orphaned second initialization");
    drop(replacement_writer);
    let second_cancelled = manager
        .cancel_job("cold-cancel-project", &second_receipt.job_id)
        .await
        .expect("cancel orphaned second initialization");
    assert_eq!(second_cancelled.state, VceJobState::Cancelled);
    assert!(
        cold_factory
            .load_active_project_config("cold-cancel-project")
            .await
            .expect("read second fenced cold binding")
            .is_none()
    );
    assert_eq!(
        cold_factory
            .load_uninitialized_recovery_config("cold-cancel-project")
            .await
            .expect("read newest cold recovery candidate")
            .expect("second orphan remains recoverable")
            .binding
            .namespace_id,
        replacement_config.binding.namespace_id
    );
    std::fs::write(
        cold_root.join("recovery.rs"),
        "pub fn recovery_marker() -> &'static str { \"cold_recovery\" }\n",
    )
    .expect("write cold recovery source");
    // A cold-cancelled orphan must expose the same explicit maintenance plan used by rebuild admission.
    // 冷取消留下的孤立绑定必须暴露与重建准入相同的明确维护计划。
    let recovery_plan = manager
        .stored_maintenance_plan(
            "cold-cancel-project",
            vulcan_context_engine::VceMaintenanceMode::Rebuild,
        )
        .await
        .expect("read orphaned recovery maintenance plan");
    let recovery_actions = recovery_plan
        .actions
        .into_iter()
        .map(|action| action.action_id)
        .collect::<BTreeSet<_>>();
    assert!(
        !recovery_actions.is_empty(),
        "orphaned uninitialized binding must keep an explicit rebuild entry"
    );
    let compact_error = manager
        .stored_maintenance_plan(
            "cold-cancel-project",
            vulcan_context_engine::VceMaintenanceMode::Compact,
        )
        .await
        .expect_err("orphaned uninitialized binding must not offer physical compaction");
    assert!(
        compact_error.contains("active published index"),
        "compact preview should identify its active-index requirement: {compact_error}"
    );
    let recovery_input = VceRebuildProjectInput {
        project_id: String::from("cold-cancel-project"),
        project_root: cold_root.clone(),
        settings: settings.clone(),
        dimension: 3,
        fingerprint: fingerprint.clone(),
    };
    // Rebuild admission is recreated with identical fields for each idempotent replay.
    // 重建准入在每次幂等重放时都用完全相同的字段重新构造。
    let recovery_command = || VceJobAdmission {
        retry_of_job_id: None,
        kind: VceJobKind::Rebuild,
        idempotency_key: String::from("cold-recovery-rebuild"),
        expected_settings_revision: settings.revision,
        expected_checkpoint_version: None,
        contract_fingerprint: fingerprint.clone(),
    };
    // Hold one actual embedding call so the in-flight idempotency replay is exercised before completion.
    // 暂停一次真实向量调用，确保在完成前验证进行中的幂等重放。
    let recovery_pause = ai.documents.arm();
    let recovery_job = manager
        .submit_project_rebuild(
            recovery_input.clone(),
            recovery_command(),
            false,
            recovery_actions.clone(),
            &executor,
        )
        .await
        .expect("admit rebuild from orphaned uninitialized binding");
    assert_eq!(recovery_job.kind, VceJobKind::Rebuild);
    assert_eq!(recovery_job.source_checkpoint_version, None);
    assert!(recovery_job.replacement.is_none());
    ai.documents.wait_entered().await;
    assert_eq!(
        manager
            .submit_project_rebuild(
                recovery_input.clone(),
                recovery_command(),
                false,
                recovery_actions.clone(),
                &executor,
            )
            .await
            .expect("replay active null-checkpoint rebuild")
            .job_id,
        recovery_job.job_id
    );
    drop(recovery_pause);
    let recovered = await_test_job(&manager, "cold-cancel-project", &recovery_job.job_id).await;
    assert_eq!(
        recovered.state,
        VceJobState::Succeeded,
        "null-checkpoint recovery diagnostic: {recovered:?}"
    );
    assert!(
        cold_factory
            .load_active_project_config("cold-cancel-project")
            .await
            .expect("read rebuilt active binding")
            .is_some()
    );
    assert!(manager.project_can_serve_now("cold-cancel-project", settings.revision, &fingerprint));
    assert_eq!(
        manager
            .submit_project_rebuild(
                recovery_input,
                recovery_command(),
                false,
                recovery_actions,
                &executor,
            )
            .await
            .expect("replay completed null-checkpoint rebuild")
            .job_id,
        recovery_job.job_id
    );
    // Release test-only storage handles before shutdown so no external project lease survives the drain.
    // 在关闭管理器前释放测试专用存储句柄，确保排空时不会残留外部项目租约。
    drop(replacement_index);
    drop(cold_runtime);
    drop(published_runtime);
    drop(cold_factory);
    // Keep the additional acceptance future boxed so its filesystem state does not enlarge this frame.
    // 将追加验收 Future 装箱，避免其文件系统状态扩大当前栈帧。
    Box::pin(verify_manual_admission_and_watcher_source_events(
        &manager,
        &project_root,
        &updated_settings,
        &fingerprint,
        &ai,
        &executor,
    ))
    .await;
    manager
        .shutdown()
        .await
        .expect("drain nonempty fixture runtime and read leases");
}

/// Verify manual rejection and exact watcher scan branches on the existing committed fixture.
/// 在既有已提交夹具上验证手动模式拒绝及精确监听扫描分支。
/// `manager` and `project_root` select the private project; `settings` and `fingerprint` freeze its policy.
/// `manager` 与 `project_root` 选择私有项目；`settings` 与 `fingerprint` 固定其策略。
/// `ai` counts actual calls and `executor` owns the explicit seed job; returns after read-only scan checks.
/// `ai` 计数实际调用，`executor` 持有明确的播种作业；完成只读扫描检查后返回。
async fn verify_manual_admission_and_watcher_source_events(
    manager: &VceRuntimeManager,
    project_root: &Path,
    settings: &ProjectVceSettings,
    fingerprint: &str,
    ai: &ControlledHostAi,
    executor: &tokio::runtime::Runtime,
) {
    assert_eq!(settings.update_mode, ProjectVceUpdateMode::Manual);
    // Use the same canonical identity that the production source adapter admits.
    // 使用生产源码适配器准入的相同规范身份。
    let canonical_root = std::fs::canonicalize(project_root).expect("canonical source fixture");
    // The previous terminal receipt must remain the latest after rejected automatic work.
    // 自动工作被拒绝后，先前终态回执必须仍为最近回执。
    let previous_job = manager
        .get_job("nonempty-project", None)
        .await
        .expect("read previous fixture job");
    // Successful freshness is independent from the most recent cancelled attempt.
    // 成功新鲜度独立于最近一次已取消尝试。
    let previous_success = manager
        .last_successful_update("nonempty-project")
        .await
        .expect("read previous successful update");
    // Capture actual model invocations before admission, not only successful responses.
    // 准入之前捕获实际模型调用次数，而不只统计成功响应。
    let calls_before = ai.embedding_calls.load(Ordering::Relaxed);
    // This is the actual durable watcher admission entry, not the policy calculator alone.
    // 这是实际持久监听准入入口，而不只是策略计算器。
    let rejected = manager
        .submit_watcher_job(
            "nonempty-project",
            &canonical_root,
            Some(BTreeSet::from([String::from("lib.rs")])),
            None,
            executor,
        )
        .await
        .expect_err("manual mode must reject background admission");
    assert!(rejected.contains("automatic updates are disabled"));
    assert_eq!(ai.embedding_calls.load(Ordering::Relaxed), calls_before);
    assert_eq!(
        manager
            .get_job("nonempty-project", None)
            .await
            .expect("latest after rejection"),
        previous_job
    );
    assert_eq!(
        manager
            .last_successful_update("nonempty-project")
            .await
            .expect("freshness after rejection"),
        previous_success
    );
    // This runtime already owns the published nonempty checkpoint used by the preceding assertions.
    // 此运行时已持有前述断言使用的非空已发布检查点。
    let runtime = manager
        .require_project("nonempty-project")
        .await
        .expect("fixture runtime");
    // Freeze the exact checkpoint expected by the one additional explicit human synchronization.
    // 固定追加的唯一明确人为同步所要求的精确检查点。
    let checkpoint = runtime
        .current_engine
        .read()
        .await
        .engine
        .status()
        .checkpoint_version;
    // Seed a nested record through the real writer so deleted-directory detection has published evidence.
    // 通过真实写入者播种嵌套记录，使删除目录检测具有已发布证据。
    let removed_directory = canonical_root.join("watcher-deleted");
    std::fs::create_dir(&removed_directory).expect("create owned directory fixture");
    std::fs::write(
        removed_directory.join("child.rs"),
        "pub fn deleted_child() {}\n",
    )
    .expect("write nested source fixture");
    // This unchanged source must survive directory and ignore reconciliation when not explicitly excluded.
    // 此未变源码在未明确排除时必须在目录和忽略规则核对中保留。
    let retained_file = canonical_root.join("watcher-kept.rs");
    std::fs::write(&retained_file, "pub fn retained_source() {}\n")
        .expect("write retained fixture");
    // A local Git metadata directory activates Git ignore semantics without creating a real repository.
    // 本地 Git 元数据目录启用 Git 忽略语义，不创建真实仓库。
    std::fs::create_dir_all(canonical_root.join(".git/info"))
        .expect("local ignore fixture metadata");
    // Only this explicitly requested human job may publish the newly seeded source records.
    // 只有此明确请求的人为作业可以发布新播种的源码记录。
    let seed = manager
        .submit_job(
            "nonempty-project",
            VceJobAdmission {
                retry_of_job_id: None,
                kind: VceJobKind::Synchronize,
                idempotency_key: String::from("watcher-source-evidence-seed"),
                expected_settings_revision: settings.revision,
                expected_checkpoint_version: checkpoint,
                contract_fingerprint: fingerprint.to_owned(),
            },
            executor,
        )
        .await
        .expect("explicit source seed synchronization");
    // Wait for both the terminal receipt and worker lease release before inspecting scan-only branches.
    // 检查纯扫描分支前等待终态回执和工作线程租约释放。
    let seeded = await_test_job(manager, "nonempty-project", &seed.job_id).await;
    assert_eq!(seeded.state, VceJobState::Succeeded, "seed: {seeded:?}");
    // All following checks construct input only and must never invoke embedding or publish a new job.
    // 后续检查仅构造输入，绝不得调用向量服务或发布新作业。
    let seeded_calls = ai.embedding_calls.load(Ordering::Relaxed);
    assert!(seeded_calls > calls_before);
    // The exact control belongs to isolated scans and does not simulate durable writer admission.
    // 精确控制信号属于隔离扫描，不模拟持久写入者准入。
    let active = VceActiveRefresh {
        request_id: String::from("watcher-source-evidence"),
        embedding_max_concurrency: settings.embedding_max_concurrency,
        cancellation: vulcan_context_engine::VceRefreshControl::default(),
    };
    // Build exact dirty-path work; direct scanning avoids the separate missing-continuity promotion.
    // 构造精确脏路径工作；直接扫描避免另一个缺失连续性凭据的提升分支。
    let work = |paths: Option<BTreeSet<String>>| VceJobWork::FileChanges {
        observation: None,
        project_root: canonical_root.clone(),
        paths,
    };
    // Remove only the two known fixture objects, preserving their old published record for the branch.
    // 仅删除两个已知夹具对象，为分支保留其旧的已发布记录。
    std::fs::remove_file(removed_directory.join("child.rs")).expect("remove exact source child");
    std::fs::remove_dir(&removed_directory).expect("remove empty fixture directory");
    // A deleted directory must remove all its old descendants by ordinary full reconciliation.
    // 删除目录必须通过普通全量核对移除其全部旧后代。
    let removed = manager
        .scan_job_request(
            &runtime,
            &active,
            work(Some(BTreeSet::from([String::from("watcher-deleted")]))),
        )
        .await
        .expect("scan deleted directory event");
    assert_eq!(removed.mode, vulcan_context_engine::RefreshMode::Full);
    assert_eq!(
        removed.reason,
        vulcan_context_engine::RefreshReason::FileWatcher
    );
    assert!(!removed.force_reindex);
    assert!(
        !removed
            .observations
            .iter()
            .any(|record| record.path.starts_with("watcher-deleted/"))
    );
    assert!(
        removed
            .observations
            .iter()
            .any(|record| record.path == "watcher-kept.rs")
    );
    // A new directory event must discover its descendants even when only the directory was delivered.
    // 即使只投递目录本身，新目录事件也必须发现其后代。
    let added_directory = canonical_root.join("watcher-added");
    std::fs::create_dir(&added_directory).expect("create new directory fixture");
    std::fs::write(
        added_directory.join("child.rs"),
        "pub fn added_child() {}\n",
    )
    .expect("write new nested source");
    // Directory observation is a different branch from missing-directory detection above.
    // 目录观察是与上述缺失目录检测不同的分支。
    let added = manager
        .scan_job_request(
            &runtime,
            &active,
            work(Some(BTreeSet::from([String::from("watcher-added")]))),
        )
        .await
        .expect("scan new directory event");
    assert_eq!(added.mode, vulcan_context_engine::RefreshMode::Full);
    assert!(!added.force_reindex);
    assert!(
        added
            .observations
            .iter()
            .any(|record| record.path == "watcher-added/child.rs")
    );
    // Each supported control file must independently rescan unaffected paths under its new rule.
    // 每种支持的控制文件都必须独立按新规则重新扫描未直接变化的路径。
    for control_path in [".gitignore", ".ignore", ".oceignore", ".git/info/exclude"] {
        std::fs::write(canonical_root.join(control_path), "watcher-kept.rs\n")
            .expect("write one local ignore control");
        // The input event names only the rule file, never the source it now excludes.
        // 输入事件仅包含规则文件，绝不包含它现在排除的源码。
        let ignored = manager
            .scan_job_request(
                &runtime,
                &active,
                work(Some(BTreeSet::from([control_path.to_owned()]))),
            )
            .await
            .expect("scan ignore control event");
        std::fs::remove_file(canonical_root.join(control_path)).expect("remove exact rule fixture");
        assert_eq!(
            ignored.mode,
            vulcan_context_engine::RefreshMode::Full,
            "{control_path}"
        );
        assert!(!ignored.force_reindex, "{control_path}");
        assert!(
            !ignored
                .observations
                .iter()
                .any(|record| record.path == "watcher-kept.rs"),
            "{control_path}"
        );
        assert!(
            ignored
                .observations
                .iter()
                .any(|record| record.path == "watcher-added/child.rs"),
            "{control_path}"
        );
    }
    // Rename supplies both exact paths and should remain incremental rather than forcing all vectors.
    // 重命名提供两个精确路径，应保持增量而不是强制处理全部向量。
    std::fs::rename(&retained_file, canonical_root.join("watcher-renamed.rs"))
        .expect("rename exact retained fixture");
    // Observe the real filesystem rename through the production request builder.
    // 通过生产请求构建器观察真实文件系统重命名。
    let renamed = manager
        .scan_job_request(
            &runtime,
            &active,
            work(Some(BTreeSet::from([
                String::from("watcher-kept.rs"),
                String::from("watcher-renamed.rs"),
            ]))),
        )
        .await
        .expect("scan rename event");
    assert_eq!(renamed.mode, vulcan_context_engine::RefreshMode::Paths);
    assert!(!renamed.force_reindex);
    assert_eq!(renamed.removed_paths, vec![String::from("watcher-kept.rs")]);
    assert_eq!(renamed.observations.len(), 1);
    assert_eq!(renamed.observations[0].path, "watcher-renamed.rs");
    // An explicit root-event marker also produces ordinary full reconciliation, never a rebuild.
    // 明确根事件标记也产生普通全量核对，而不是重建。
    let root_event = manager
        .scan_job_request(&runtime, &active, work(None))
        .await
        .expect("root event");
    assert_eq!(root_event.mode, vulcan_context_engine::RefreshMode::Full);
    assert!(!root_event.force_reindex);
    assert!(
        root_event
            .observations
            .iter()
            .any(|record| record.path == "watcher-renamed.rs")
    );
    assert_eq!(ai.embedding_calls.load(Ordering::Relaxed), seeded_calls);
    assert_eq!(
        manager
            .get_job("nonempty-project", None)
            .await
            .expect("scan-only receipt"),
        Some(seeded.clone())
    );
    assert_eq!(
        runtime
            .current_engine
            .read()
            .await
            .engine
            .status()
            .checkpoint_version,
        seeded.result_checkpoint_version
    );
}
