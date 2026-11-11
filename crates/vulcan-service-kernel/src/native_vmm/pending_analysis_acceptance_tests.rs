//! Real native intake, durable queue and extraction acceptance with an inference-only fixture.
//! 使用仅替代推理的夹具验收真实原生入库、持久队列与提炼链路。

use super::*;
use std::sync::Mutex as StdMutex;
use vulcan_ai_contract::test_support::MockHostAiPort;
use vulcan_ai_contract::*;
use vulcan_service_api::VmmNativeHostProjectPolicyOverrides;

#[path = "host_policy_service_acceptance_tests.rs"]
mod policy_service;

#[path = "text_policy_service_acceptance_tests.rs"]
pub(in crate::native_vmm) mod text_policy_service;

#[path = "postaction_budget_policy_acceptance_tests.rs"]
pub(in crate::native_vmm) mod postaction_budget;

#[path = "precheck_policy_acceptance_tests.rs"]
pub(in crate::native_vmm) mod precheck_policy;

#[path = "rerank_policy_acceptance_tests.rs"]
pub(in crate::native_vmm) mod rerank_policy;

#[path = "hybrid_policy_acceptance_tests.rs"]
pub(in crate::native_vmm) mod hybrid_policy;

#[path = "weibull_policy_acceptance_tests.rs"]
pub(in crate::native_vmm) mod weibull_policy;

#[path = "replacement_policy_acceptance_tests.rs"]
pub(in crate::native_vmm) mod replacement_policy;

#[path = "nonempty_analysis_acceptance_tests.rs"]
pub(in crate::native_vmm) mod nonempty;
#[path = "project_profile_acceptance_tests.rs"]
pub(in crate::native_vmm) mod project_profiles;

/// Verifies preview, physical request sizes and durable vector replacement on an isolated controller.
/// 在隔离控制器上验证预检、实际请求条数及持久向量重建。
/// `data_plane` owns the test storage, `capability` is the simulated model, and `memories` are the initial targets.
/// `data_plane` 持有测试存储，`capability` 为模拟模型，`memories` 为初始目标。
pub(in crate::native_vmm) async fn verify_vector_rebuild_batch_limit(
    data_plane: VmmDataPlaneHandle,
    mut capability: InferenceCapabilitySnapshot,
    memories: Vec<vulcan_memorymesh_core::MemoryRecord>,
) {
    // Keep the validating mock port and the deterministic dense-vector response used by native analysis tests.
    // 复用原生分析测试的校验端口及确定性稠密向量响应。
    let port = Arc::new(CapturingAnalysisPort {
        inner: MockHostAiPort::new(),
        requests: StdMutex::new(Vec::new()),
        reject_next_turn: AtomicBool::new(false),
        fixture: AnalysisFixture::Nonempty,
        embedding_requests: StdMutex::new(Vec::new()),
        profile_instruction_pause: StdMutex::new(None),
        postaction_pause: StdMutex::new(None),
    });
    let contract = capability
        .operations
        .iter_mut()
        .find(|operation| {
            operation.operation == AiOperation::EmbeddingCreate && operation.purpose_id.is_none()
        })
        .and_then(|operation| operation.embedding.as_mut())
        .expect("embedding contract");
    contract.maximum_inputs = Some(20);
    let contract = contract.clone();
    port.inner
        .set_capability_snapshot(capability)
        .expect("fixture capability");
    let runtime = Arc::new(
        NativeVmmCoreRuntime::bootstrap(port.clone(), data_plane.clone())
            .await
            .expect("isolated rebuild runtime"),
    );
    let initial = runtime.storage_snapshot().expect("initial snapshot");
    runtime
        .transform_storage_snapshot(
            "rebuild-seed",
            "test",
            initial.storage_version,
            move |snapshot| {
                snapshot.dataset.memories.extend(memories);
                Ok(())
            },
        )
        .await
        .expect("seed legacy memories");
    let maintenance = crate::native_vmm::NativeVmmMaintenance::new(
        runtime,
        data_plane,
        Arc::new(RwLock::new(())),
    );

    // The same confirmation is consumed by the actual maintenance entry point.
    // 由真实维护入口消费同一份确认。
    let preview = maintenance
        .preview_vector_rebuild()
        .await
        .expect("rebuild preview");
    assert_eq!((preview.rebuild_memory_count, preview.batch_count), (39, 2));
    let result = maintenance
        .execute_vector_rebuild(
            "rebuild-capped-fixture",
            preview.storage_version,
            &preview.preview_fingerprint,
            true,
        )
        .await
        .expect("rebuild through production maintenance");
    assert_eq!((result.rebuilt_memory_count, result.batch_count), (39, 2));
    assert_eq!(
        maintenance
            .preview_vector_rebuild()
            .await
            .expect("remaining preview")
            .rebuild_memory_count,
        0
    );

    // Validate each observed request against the same typed limit that rejects the original request.
    // 用会拒绝原始超限请求的同一强类型契约校验每条观测请求。
    let requests = port
        .embedding_requests
        .lock()
        .expect("captured embedding requests");
    assert_eq!(
        requests
            .iter()
            .map(|call| call.request.inputs.len())
            .collect::<Vec<_>>(),
        [20, 19]
    );
    for call in requests.iter() {
        contract
            .validate_request(&call.request)
            .expect("within selected model limit");
    }
}

/// One explicitly armed test pause after command admission and before the model response is released.
/// 在命令准入后、模型响应释放前显式启用的一次测试暂停。
struct ProfileInstructionPause {
    /// Signals that the actual Host AI instruction boundary was reached.
    /// 通知已到达实际 Host AI 指令边界。
    entered: tokio::sync::oneshot::Sender<()>,
    /// Resumes only this instruction after the test changes the saved configuration.
    /// 测试修改已保存配置后，仅恢复此指令。
    resume: tokio::sync::oneshot::Receiver<()>,
}

/// Selects one closed deterministic inference behavior without changing production command execution.
/// 选择一种封闭确定性推理行为，不改变生产命令执行。
enum AnalysisFixture {
    /// Emits empty extraction for queue and text-admission acceptance.
    /// 为队列及文本准入验收返回空提取。
    Empty,
    /// Emits the established memory/profile extraction and instruction responses.
    /// 返回既有记忆／画像提取及指令响应。
    Nonempty,
    /// Emits affirmative precheck responses with an optional one-shot admission pause.
    /// 返回肯定预检响应，并可在准入后进行一次暂停。
    PrecheckPolicy(StdMutex<Option<precheck_policy::IntentPause>>),
    /// Captures deterministic reranking and an explicitly armed query-admission pause.
    /// 捕获确定性重排以及明确启用的查询准入暂停。
    RerankPolicy(rerank_policy::Capture),
    /// Supplies distinct vector ranks and pauses an explicitly selected hybrid query.
    /// 提供不同向量排名并暂停明确选择的混合查询。
    HybridPolicy(StdMutex<Option<precheck_policy::IntentPause>>),
    /// Uses real precheck adoption and pauses only an explicitly armed decay-search embedding.
    /// 使用真实预检采纳，并仅暂停明确启用的衰减搜索向量请求。
    WeibullPolicy(StdMutex<Option<precheck_policy::IntentPause>>),
    /// Supplies exact replacement vectors and pauses an explicitly armed direct-write query.
    /// 提供准确替代向量并暂停明确启用的主动写入查询。
    ReplacementPolicy(StdMutex<Option<precheck_policy::IntentPause>>),
    /// Supplies declared vectors, precheck responses and explicit extraction probes for live HTTP policy acceptance.
    /// 为真实 HTTP 策略验收提供声明向量、预检响应及明确的提炼探针。
    PolicyHttp,
}

/// Records actual host requests while delegating ownership, validation and execution to the mock port.
/// 记录实际宿主请求，同时将归属、校验及执行委托给模拟端口。
struct CapturingAnalysisPort {
    /// Validating host registry and deterministic response executor.
    /// 执行校验的宿主注册表及确定性响应执行器。
    inner: MockHostAiPort,
    /// Exact ordered L1 and L2 requests observed at the production driver boundary.
    /// 在生产驱动边界观察到的准确有序 L1 和 L2 请求。
    requests: StdMutex<Vec<ResponsesExecutionRequest>>,
    /// Injects one wrong echoed turn without altering production response validation.
    /// 注入一次错误回合回显，不修改生产响应校验。
    reject_next_turn: AtomicBool,
    /// Selects the explicit closed response fixture for this isolated acceptance.
    /// 为本隔离验收选择明确封闭的响应夹具。
    fixture: AnalysisFixture,
    /// Records exact physical embedding requests separately from LLM requests.
    /// 与 LLM 请求分开记录准确物理向量请求。
    embedding_requests: StdMutex<Vec<HostAiEmbeddingRequest>>,
    /// Optional one-shot pause; ordinary acceptance fixtures never arm it.
    /// 可选的一次暂停；普通验收夹具不会启用。
    profile_instruction_pause: StdMutex<Option<ProfileInstructionPause>>,
    /// Optional L1 analysis pause owned by the explicit post-action budget acceptance.
    /// 由明确 PostAction 预算验收持有的可选 L1 分析暂停。
    postaction_pause: StdMutex<Option<postaction_budget::AnalysisPause>>,
}

#[async_trait::async_trait]
impl HostAiPort for CapturingAnalysisPort {
    /// Registers the supplied host identity and returns its validated handle.
    /// 注册传入的宿主身份并返回已校验句柄。
    fn register_capability(
        &self,
        registration: HostAiCapabilityRegistration,
    ) -> Result<HostAiCapabilityHandle, HostAiError> {
        self.inner.register_capability(registration)
    }

    /// Resolves a registered ID without creating another provider.
    /// 解析已注册标识，不创建其他供应商。
    fn capability_handle(
        &self,
        registration_id: &str,
    ) -> Result<HostAiCapabilityHandle, HostAiError> {
        self.inner.capability_handle(registration_id)
    }

    /// Removes the named fixture registration and returns the registry result.
    /// 移除指定夹具注册并返回注册表结果。
    fn unregister_capability(&self, registration_id: &str) -> Result<(), HostAiError> {
        self.inner.unregister_capability(registration_id)
    }

    /// Returns the current registered snapshot for the supplied handle.
    /// 返回传入句柄当前已注册的快照。
    fn capability_snapshot(
        &self,
        handle: &HostAiCapabilityHandle,
    ) -> Result<InferenceCapabilitySnapshot, HostAiError> {
        self.inner.capability_snapshot(handle)
    }

    /// Validates the supplied handle, call context and embedding request through the declared fixture mode.
    /// 通过声明的夹具模式校验传入句柄、调用上下文及向量请求。
    /// Returns captured deterministic embeddings for nonempty modes; unconfigured empty-mode requests fail.
    /// 非空模式返回已捕获的确定性向量；空模式未配置请求失败。
    async fn create_embeddings(
        &self,
        handle: &HostAiCapabilityHandle,
        context: HostAiCallContext,
        request: HostAiEmbeddingRequest,
    ) -> Result<HostAiEmbeddingResponse, HostAiError> {
        if !matches!(&self.fixture, AnalysisFixture::Empty) {
            // The precheck fixture uses distinct document vectors so hard dedupe remains production behavior.
            // 预检夹具使用不同文档向量，使硬去重保持生产行为。
            let response = match &self.fixture {
                AnalysisFixture::PrecheckPolicy(_) => precheck_policy::embedding_response(&request),
                AnalysisFixture::RerankPolicy(_) => precheck_policy::embedding_response(&request),
                AnalysisFixture::HybridPolicy(pause) => {
                    hybrid_policy::wait_for_query(&request, pause).await;
                    hybrid_policy::embedding_response(&request)
                }
                AnalysisFixture::WeibullPolicy(pause) => {
                    weibull_policy::wait_for_query(&request, pause).await;
                    precheck_policy::embedding_response(&request)
                }
                AnalysisFixture::ReplacementPolicy(pause) => {
                    replacement_policy::wait_for_query(&request, pause).await;
                    replacement_policy::embedding_response(&request)
                }
                AnalysisFixture::Nonempty => nonempty::embedding_response(&request),
                AnalysisFixture::PolicyHttp => replacement_policy::embedding_response(&request),
                AnalysisFixture::Empty => unreachable!("empty embedding fixture was excluded"),
            };
            self.inner.set_embedding_response(response)?;
            self.embedding_requests
                .lock()
                .expect("embedding capture")
                .push(request.clone());
        }
        self.inner.create_embeddings(handle, context, request).await
    }

    /// Delegates the exact handle, context and rerank request to the validating host port.
    /// 将准确句柄、上下文及重排请求委托给执行校验的宿主端口。
    /// Returns controlled reranking only in its explicit mode; other modes reject unconfigured calls.
    /// 仅在明确模式下返回受控重排；其他模式拒绝未配置调用。
    async fn rerank_documents(
        &self,
        handle: &HostAiCapabilityHandle,
        context: HostAiCallContext,
        request: RerankRequest,
    ) -> Result<RerankResponse, HostAiError> {
        if let AnalysisFixture::RerankPolicy(capture) = &self.fixture {
            capture
                .requests
                .lock()
                .expect("rerank capture")
                .push(request.clone());
            self.inner
                .set_rerank_response(rerank_policy::response(&request))?;
            precheck_policy::wait_for_intent(&capture.pause).await;
        }
        self.inner.rerank_documents(handle, context, request).await
    }

    /// Validates and captures the supplied analysis request under the exact host handle and call context.
    /// 在准确宿主句柄及调用上下文下校验并捕获传入分析请求。
    /// Returns an execution handle for the explicitly selected extraction, profile or precheck fixture response.
    /// 为明确选择的提炼、画像或预检夹具响应返回执行句柄。
    async fn start_response_execution(
        &self,
        handle: &HostAiCapabilityHandle,
        context: HostAiCallContext,
        request: ResponsesExecutionRequest,
    ) -> Result<HostAiExecutionHandle, HostAiError> {
        assert!(matches!(
            request.purpose_id.as_str(),
            "postaction_l1"
                | "postaction_l2"
                | "precheck_l1"
                | "precheck_l2"
                | "profile_instruction"
        ));
        assert_eq!(request.reasoning_effort, Some(ReasoningEffort::None));
        assert_eq!(request.reasoning_output, ReasoningOutputPolicy::Hidden);
        // Structured output is a typed host contract; provider JSON-object projection is upstream-owned.
        // 结构化输出属于强类型宿主契约；供应商 JSON-object 投影由上游负责。
        assert!(matches!(
            request.structured_output,
            StructuredOutputPolicy::JsonSchema { strict: true, .. }
        ));
        let input = analysis_input(&request);
        // Pause only explicitly armed analysis calls after the production command snapshot has been selected.
        // 仅在生产命令快照选定后，暂停明确启用的分析调用。
        if request.purpose_id == "postaction_l1" {
            postaction_budget::wait_for_analysis(&self.postaction_pause).await;
        }
        // Pause only the explicitly armed precheck intent after its command snapshot has been admitted.
        // 仅在命令快照准入后，暂停明确启用的预检意图。
        if request.purpose_id == "precheck_l1"
            && let AnalysisFixture::PrecheckPolicy(pause) = &self.fixture
        {
            precheck_policy::wait_for_intent(pause).await;
        }
        // Release the synchronous fixture lock before waiting so configuration work cannot deadlock it.
        // 等待前释放同步夹具锁，避免配置操作与其死锁。
        if request.purpose_id == "profile_instruction" {
            let pause = self
                .profile_instruction_pause
                .lock()
                .expect("instruction pause lock")
                .take();
            if let Some(pause) = pause {
                pause.entered.send(()).expect("instruction observer");
                tokio::time::timeout(Duration::from_secs(10), pause.resume)
                    .await
                    .expect("bounded instruction resume")
                    .expect("resume sender");
            }
        }
        let mut result = match &self.fixture {
            AnalysisFixture::PolicyHttp
                if request.purpose_id == "postaction_l1"
                    && input["target_turn"]["raw_turn"]["user"]
                        .as_str()
                        .expect("HTTP L1 source")
                        .starts_with("HTTP profile ") =>
            {
                replacement_policy::network::profiles::queued_response(&request.purpose_id, &input)
            }
            AnalysisFixture::PolicyHttp
                if request.purpose_id == "postaction_l2"
                    && (input.get("user").is_some() || input.get("project").is_some()) =>
            {
                replacement_policy::network::profiles::queued_response(&request.purpose_id, &input)
            }
            AnalysisFixture::PolicyHttp if request.purpose_id == "profile_instruction" => {
                replacement_policy::network::profiles::response(&input)
            }
            AnalysisFixture::PolicyHttp
                if matches!(request.purpose_id.as_str(), "precheck_l1" | "precheck_l2") =>
            {
                replacement_policy::network::recall::response(&request.purpose_id, &input)
            }
            AnalysisFixture::PolicyHttp
                if request.purpose_id == "postaction_l1"
                    && input["target_turn"]["raw_turn"]["user"]
                        .as_str()
                        .expect("HTTP L1 user")
                        .starts_with("replacement-probe") =>
            {
                replacement_policy::response(&request.purpose_id, &input)
            }
            AnalysisFixture::PolicyHttp if request.purpose_id == "postaction_l2" => {
                replacement_policy::response(&request.purpose_id, &input)
            }
            AnalysisFixture::Nonempty => nonempty::response(&request.purpose_id, &input),
            AnalysisFixture::ReplacementPolicy(_) => {
                replacement_policy::response(&request.purpose_id, &input)
            }
            AnalysisFixture::RerankPolicy(_) | AnalysisFixture::HybridPolicy(_) => {
                assert_eq!(request.purpose_id, "postaction_l2");
                nonempty::response(&request.purpose_id, &input)
            }
            AnalysisFixture::PrecheckPolicy(_) => {
                precheck_policy::response(&request.purpose_id, &input)
            }
            AnalysisFixture::WeibullPolicy(_) => {
                assert!(matches!(
                    request.purpose_id.as_str(),
                    "postaction_l2" | "precheck_l1" | "precheck_l2"
                ));
                precheck_policy::response(&request.purpose_id, &input)
            }
            AnalysisFixture::Empty | AnalysisFixture::PolicyHttp => {
                assert_eq!(request.purpose_id, "postaction_l1");
                serde_json::json!({
                "user_input_kind": "statement",
                "turn_id": input["target_turn"]["turn_id"],
                "details": input["target_turn"]["raw_turn"]["user"],
                "memory_nodes": [],
                "profile_nodes": []
                })
            }
        };
        if self.reject_next_turn.swap(false, Ordering::SeqCst) {
            result["turn_id"] = serde_json::json!("vmm-turn-999999999");
        }
        self.inner.set_response(
            ResponsesExecutionResult {
                request_id: request.request_id.clone(),
                route: ExecutionRouteSnapshot {
                    provider_id: "fixture".to_owned(),
                    service_entry_id: "fixture".to_owned(),
                    connection_id: "fixture".to_owned(),
                    model_id: "fixture".to_owned(),
                    protocol_binding_id: None,
                    action_binding_id: None,
                    secondary_route_id: None,
                    secondary_provider_name: None,
                    secondary_endpoint_name: None,
                    capability_revision: "fixture-v1".to_owned(),
                    catalog_revision: "fixture-v1".to_owned(),
                    route_revision: "fixture-v1".to_owned(),
                    usage_scope: InferenceUsageScope::InternalBackground,
                },
                output_text: result.to_string(),
                reasoning_text: None,
                include_reasoning_encrypted_content: false,
                structured_output: Some(result),
                output_items: Vec::new(),
                termination_reason: "stop".to_owned(),
                usage: RequestUsage::default(),
                duration_ms: 0,
            },
            Vec::new(),
        )?;
        self.requests
            .lock()
            .expect("request capture lock")
            .push(request.clone());
        self.inner
            .start_response_execution(handle, context, request)
            .await
    }

    /// Returns the fixture execution state for the exact execution handle.
    /// 返回准确执行句柄对应的夹具执行状态。
    fn execution_status(
        &self,
        handle: &HostAiExecutionHandle,
    ) -> Result<HostAiExecutionSnapshot, HostAiError> {
        self.inner.execution_status(handle)
    }

    /// Returns the registered execution stream without manufacturing provider events.
    /// 返回已注册执行流，不制造供应商事件。
    fn stream_response(
        &self,
        handle: &HostAiExecutionHandle,
    ) -> Result<HostAiEventStream, HostAiError> {
        self.inner.stream_response(handle)
    }

    /// Waits for the exact validated fixture execution and returns its final response.
    /// 等待准确的已校验夹具执行并返回其最终响应。
    async fn wait_response(
        &self,
        handle: &HostAiExecutionHandle,
    ) -> Result<ResponsesExecutionResult, HostAiError> {
        self.inner.wait_response(handle).await
    }

    /// Forwards cancellation using the original host execution handle.
    /// 使用原始宿主执行句柄转发取消请求。
    fn cancel_execution(&self, handle: &HostAiExecutionHandle) -> Result<(), HostAiError> {
        self.inner.cancel_execution(handle)
    }
}

/// Reads the sole business JSON message from the production two-message L1 contract.
/// 从生产双消息 L1 契约读取唯一业务 JSON 消息。
/// `request` is captured unchanged; the returned value is parsed only from its user text.
/// `request` 保持捕获时原样；返回值仅从其用户文本解析。
fn analysis_input(request: &ResponsesExecutionRequest) -> serde_json::Value {
    assert_eq!(request.messages.len(), 2);
    assert_eq!(request.messages[0].role, MessageRole::System);
    assert_eq!(request.messages[1].role, MessageRole::User);
    let [ContentItem::Text { text }] = request.messages[1].content.as_slice() else {
        panic!("L1 must carry exactly one business JSON text");
    };
    serde_json::from_str(text).expect("actual L1 business JSON")
}

/// Runs real intake and queue drains on the isolated controller owned by the parent acceptance.
/// 在父级验收拥有的隔离控制器上运行真实入库与队列排空。
/// `data_plane` is temporary real storage; `capability` declares the fixture's exact embedding route.
/// `data_plane` 为临时真实存储；`capability` 声明夹具准确的向量路由。
/// Verifies five successful jobs, then one rejected response followed by a real successful retry.
/// 验证五个成功作业，再验证一次响应拒绝及其后的真实成功重试。
pub(in crate::native_vmm) async fn verify_caller_policy_in_real_queue(
    data_plane: VmmDataPlaneHandle,
    capability: InferenceCapabilitySnapshot,
) {
    // Replace only inference. Bootstrap, resources, intake, queue, driver, Core and persistence remain real.
    // 仅替换推理；引导、资源、入库、队列、驱动、Core 与持久化保持真实。
    let port = Arc::new(CapturingAnalysisPort {
        inner: MockHostAiPort::new(),
        requests: StdMutex::new(Vec::new()),
        reject_next_turn: AtomicBool::new(false),
        fixture: AnalysisFixture::Empty,
        embedding_requests: StdMutex::new(Vec::new()),
        profile_instruction_pause: StdMutex::new(None),
        postaction_pause: StdMutex::new(None),
    });
    port.inner
        .set_capability_snapshot(capability)
        .expect("fixture capability");
    let runtime = Arc::new(
        NativeVmmCoreRuntime::bootstrap(port.clone(), data_plane.clone())
            .await
            .expect("native analysis runtime"),
    );
    let mut settings = runtime
        .resource_catalog()
        .await
        .expect("resource catalog")
        .settings;
    settings.noise_semantic_enabled = false;
    runtime
        .update_resource_settings(settings)
        .await
        .expect("disable unrelated semantic noise fixture");
    let memory_project = data_plane
        .create_project("Queue acceptance", "queue-acceptance")
        .await
        .expect("memory project");
    for caller in ["queue-host-a", "queue-host-b"] {
        data_plane
            .bind_vulcan_project(caller, memory_project.id)
            .await
            .expect("caller binding");
    }
    let mut global = runtime
        .runtime_policy()
        .await
        .expect("global policy")
        .recall;
    global.postaction_history_turn_limit = 3;
    global.postaction_max_input_tokens = 16_384;
    // Management and the client share the same production operation gate.
    // 管理与客户端共享同一个生产操作锁。
    let operation_gate = Arc::new(RwLock::new(()));
    // Save through the exact public actions used by the UI before reopening the runtime.
    // 在重新打开运行时之前，通过界面使用的准确公开动作保存。
    let service =
        policy_service::service(runtime.clone(), data_plane.clone(), operation_gate.clone());
    // Keep the actual service response for exact value comparison.
    // 保留实际服务响应，用于准确值比较。
    let saved = policy_service::action(
        &service,
        "native_runtime_policy_update",
        serde_json::to_value(&global).expect("global JSON"),
    )
    .await
    .expect("global initial service policy");
    // Accept only the documented global-policy response variant.
    // 仅接受已定义的全局策略响应变体。
    let vulcan_service_api::VmmMemoryActionResponse::NativeRuntimePolicy(saved) = saved else {
        panic!("expected global policy response");
    };
    assert_eq!(saved.recall, global);
    let mut a = global.clone();
    a.postaction_history_turn_limit = 1;
    // Freeze the full expected override group, including inherited siblings.
    // 冻结完整预期覆盖组，包括继承的其他组。
    let overrides = VmmNativeHostProjectPolicyOverrides {
        recall: Some(a.clone()),
        ..Default::default()
    };
    // Submit the same closed host-policy payload as the project settings control.
    // 提交与项目设置控件相同的封闭宿主策略参数。
    let saved = policy_service::action(
        &service,
        "native_host_project_policy_update",
        serde_json::json!({"hostProjectId": "queue-host-a", "overrides": overrides}),
    )
    .await
    .expect("caller A service override");
    // Preserve the full typed reply as the disk-reload expectation.
    // 保留完整类型化响应，作为磁盘重载预期。
    let vulcan_service_api::VmmMemoryActionResponse::NativeHostProjectPolicy(saved) = saved else {
        panic!("expected host policy response");
    };
    assert_eq!(saved.host_project_id, "queue-host-a");
    assert_eq!(saved.overrides, overrides);
    assert_eq!(saved.effective.recall, a);
    policy_service::verify_rejections(&service, &data_plane, &global).await;
    drop(service);
    drop(runtime);
    // Rebuild Core and resource ownership from the same private disk before admitting real queued commands.
    // 准入真实队列命令前，从同一私有磁盘重建 Core 与资源归属。
    let runtime = Arc::new(
        NativeVmmCoreRuntime::bootstrap(port.clone(), data_plane.clone())
            .await
            .expect("reload saved policies"),
    );
    // Replace the old service owner with the independently rehydrated runtime.
    // 将旧服务归属方替换为独立重新水合的运行时。
    let service =
        policy_service::service(runtime.clone(), data_plane.clone(), operation_gate.clone());
    // Read through the product service again instead of inspecting only a constructed bundle.
    // 再次通过产品服务读取，而不只检查构造的配置包。
    let reloaded = policy_service::action(
        &service,
        "native_host_project_policy",
        serde_json::json!({"hostProjectId": "queue-host-a"}),
    )
    .await
    .expect("reloaded service policy");
    // Require the same response kind before comparing every saved and effective field.
    // 比较每个已保存及有效字段前，要求响应类型保持一致。
    let vulcan_service_api::VmmMemoryActionResponse::NativeHostProjectPolicy(reloaded) = reloaded
    else {
        panic!("expected reloaded host policy response");
    };
    assert_eq!(*reloaded, *saved);
    let client = NativeVmmClient::new(
        runtime.clone(),
        data_plane.clone(),
        operation_gate,
        Arc::new(AtomicBool::new(true)),
        Arc::new(Notify::new()),
    );
    // Shared memory/session identity makes the original caller the only discriminator for these policies.
    // 共享记忆及会话身份，使原始调用方成为这些策略的唯一判别依据。
    let callers = [
        "queue-host-a",
        "queue-host-a",
        "queue-host-a",
        "queue-host-b",
        "queue-host-a",
    ];
    let mut jobs = Vec::new();
    for (index, caller) in callers.iter().enumerate() {
        let operation = format!("queue-policy-{index}");
        client
            .post_action(PostActionRequest {
                host_project_id: (*caller).to_owned(),
                project_id: memory_project.id,
                user_id: 1,
                session_id: "queue-policy-session".to_owned(),
                operation_id: operation.clone(),
                user_content: format!(
                    "项目使用 PostgreSQL 存储业务数据，这是本次验收记录 {index}。"
                ),
                assistant_content: "已记录本轮讨论内容。".to_owned(),
                ..Default::default()
            })
            .await
            .expect("real postaction intake");
        let job = data_plane
            .load_pending_analysis_job(&operation)
            .await
            .expect("durable job")
            .expect("job exists");
        assert_eq!(job.state, VMM_PENDING_ANALYSIS_READY);
        let payload: NativePendingPostActionPayload =
            serde_json::from_str(&job.payload_json).expect("exact queue payload");
        assert_eq!(payload.host_project_id, *caller);
        jobs.push(job);
    }
    assert!(
        port.requests.lock().expect("capture lock").is_empty(),
        "intake must not start extraction"
    );
    recycle_queue_acceptance_tests::verify(&client, &port, &jobs).await;
    // Change inherited defaults after enqueue, and admit B last: A must still use its own explicit group.
    // 入队后修改继承默认值并最后准入 B：A 仍必须使用自己的明确分组。
    global.postaction_history_turn_limit = 2;
    // Capture the formal save that will affect the following inherited analysis commands.
    // 捕获会影响后续继承分析命令的正式保存。
    let updated = policy_service::action(
        &service,
        "native_runtime_policy_update",
        serde_json::to_value(&global).expect("updated global JSON"),
    )
    .await
    .expect("global service edit after enqueue");
    // A successful unrelated action cannot satisfy this policy-save assertion.
    // 无关动作成功不能满足此策略保存断言。
    let vulcan_service_api::VmmMemoryActionResponse::NativeRuntimePolicy(updated) = updated else {
        panic!("expected updated global policy response");
    };
    assert_eq!(updated.recall, global);
    runtime
        .active_memory_context("queue-host-b")
        .await
        .expect("activate other caller");
    for job in &jobs {
        assert!(
            client
                .drain_due_pending_analysis_jobs()
                .await
                .expect("production queue drain")
        );
        let completed = data_plane
            .load_pending_analysis_job(&job.operation_id)
            .await
            .expect("completed job")
            .expect("audit job");
        assert_eq!(
            completed.state,
            vulcan_workspace_store::vmm_data_plane::VMM_PENDING_ANALYSIS_COMPLETED,
            "job failed: {:?}",
            completed.last_error
        );
        assert_eq!(completed.attempt_count, 0);
    }
    assert!(
        !client
            .drain_due_pending_analysis_jobs()
            .await
            .expect("empty queue")
    );
    // A stale failure report after successful extraction must report the committed successful state.
    // 成功提取之后的迟到失败报告必须返回已提交成功状态。
    assert_eq!(
        client
            .finalize_failed_pending_analysis_job(
                &jobs[0],
                TurnStatus::Corrupted,
                "stale failure after success",
            )
            .await
            .expect("completed result remains authoritative"),
        Some(TurnStatus::Completed)
    );
    // Own the completed captures so no synchronous guard survives into the following async retry test.
    // 独立持有已完成捕获，避免同步锁守卫存续至后续异步重试测试。
    let captured = port.requests.lock().expect("capture lock").clone();
    assert_eq!(captured.len(), jobs.len());
    for (index, (request, expected_count)) in captured.iter().zip([0, 1, 1, 2, 1]).enumerate() {
        let input = analysis_input(request);
        assert_eq!(
            input["target_turn"]["turn_id"].as_str(),
            jobs[index].turn_id.as_deref()
        );
        let references = input["reference_turns"]
            .as_array()
            .expect("actual references");
        assert_eq!(
            references.len(),
            expected_count,
            "caller policy at turn {index}"
        );
        for reference in references {
            assert!(
                jobs[index - expected_count..index]
                    .iter()
                    .any(|job| reference["turn_id"].as_str() == job.turn_id.as_deref()),
                "history must contain only the expected preceding completed turns"
            );
        }
        assert_eq!(
            input["target_turn"]["raw_turn"]
                .as_object()
                .expect("narrow dialogue")
                .len(),
            3
        );
    }
    drop(captured);
    let snapshot = runtime
        .storage_snapshot()
        .expect("persisted extraction state");
    let turns = snapshot
        .dataset
        .turns
        .iter()
        .filter(|turn| turn.session_source_id.as_str() == "queue-policy-session")
        .collect::<Vec<_>>();
    assert_eq!(turns.len(), 5);
    assert!(turns.iter().all(|turn| turn.status == TurnStatus::Completed
        && turn.scope_id == project_scope_id(memory_project.id).expect("owner scope")));
    let session = snapshot
        .memory_sessions
        .iter()
        .find(|session| session.session_source_id.as_str() == "queue-policy-session")
        .expect("authoritative session");
    assert!(session.last_extraction_observed_at_ns > 0);
    // Keep the last committed boundary to verify the following failed attempt cannot advance it.
    // 保留最后提交边界，验证后续失败尝试不能推进它。
    let previous_boundary = session.last_extraction_observed_at_ns;
    // Empty extraction is intentional here; this test must not masquerade as real-model quality acceptance.
    // 此处空提取是有意设置；本测试不能冒充真实模型质量验收。
    assert!(
        snapshot
            .dataset
            .memories
            .iter()
            .all(|memory| memory.scope_id
                != project_scope_id(memory_project.id).expect("owner scope"))
    );
    drop(snapshot);
    verify_failed_response_retry(
        &client,
        &port,
        memory_project.id,
        previous_boundary,
        &jobs[4],
    )
    .await;
    recycle_queue_acceptance_tests::verify_permanent_removal(&client, &port, memory_project.id)
        .await;
}

#[path = "recycle_queue_acceptance_tests.rs"]
mod recycle_queue_acceptance_tests;

/// Verifies a malformed model response preserves the watermark and retries through the actual durable scheduler.
/// 验证模型响应错误会保留水位，并通过实际耐久调度器重试。
/// The client and port belong to this isolated test; owner, boundary and prior job define the exact assertions.
/// 客户端及端口属于本隔离测试；归属、边界及前一作业定义准确断言。
/// Returns after one rejected response and one successful real-time retry, without editing queue state.
/// 在一次响应拒绝及一次真实时间重试成功后返回，不修改队列状态。
async fn verify_failed_response_retry(
    client: &NativeVmmClient,
    port: &CapturingAnalysisPort,
    project_id: u64,
    previous_boundary: i64,
    previous_job: &NativeVmmPendingAnalysisJob,
) {
    client
        .post_action(PostActionRequest {
            host_project_id: "queue-host-a".to_owned(),
            project_id,
            user_id: 1,
            session_id: "queue-policy-session".to_owned(),
            operation_id: "queue-policy-retry".to_owned(),
            user_content: "数据库部署仍使用 PostgreSQL，请继续记录本次方案讨论。".to_owned(),
            assistant_content: "本轮方案讨论已经结束。".to_owned(),
            ..Default::default()
        })
        .await
        .expect("retry intake");
    port.reject_next_turn.store(true, Ordering::SeqCst);
    assert!(
        client
            .drain_due_pending_analysis_jobs()
            .await
            .expect("failed analysis drain")
    );
    // Read the real failure receipt and retry timestamp; the fixture never changes them.
    // 读取真实失败收据及重试时间；夹具绝不修改二者。
    let failed = client
        .data_plane
        .load_pending_analysis_job("queue-policy-retry")
        .await
        .expect("failed row")
        .expect("queued retry");
    assert_eq!(
        failed.state, VMM_PENDING_ANALYSIS_READY,
        "{:?}",
        failed.last_error
    );
    assert_eq!(failed.attempt_count, 1);
    assert!(
        failed
            .last_error
            .as_deref()
            .expect("failure diagnosis")
            .contains("turn_id does not echo")
    );
    // Failed extraction must not change either the session watermark or the turn's pending state.
    // 提炼失败不得改变会话水位，也不能改变回合待处理状态。
    let after_failure = client.runtime.storage_snapshot().expect("failure state");
    assert_eq!(
        after_failure
            .memory_sessions
            .iter()
            .find(|session| session.session_source_id.as_str() == "queue-policy-session")
            .expect("failed session")
            .last_extraction_observed_at_ns,
        previous_boundary
    );
    assert_eq!(
        after_failure
            .dataset
            .turns
            .iter()
            .find(|turn| Some(turn.id.as_str()) == failed.turn_id.as_deref())
            .expect("failed turn")
            .status,
        TurnStatus::Pending
    );
    drop(after_failure);
    // Await only the actual first retry window; do not bypass backoff or forge a due timestamp.
    // 仅等待实际首次重试窗口；不绕过退避，也不伪造到期时间。
    let wait_ms =
        (failed.next_attempt_at_unix_ms - now_unix_ms().expect("retry clock")).max(0) as u64;
    assert!(wait_ms <= 1_000);
    tokio::time::sleep(std::time::Duration::from_millis(wait_ms + 1)).await;
    assert!(
        client
            .drain_due_pending_analysis_jobs()
            .await
            .expect("actual due retry")
    );
    // The acknowledged row retains the failure count but has a successful final state.
    // 已确认记录保留失败次数，但最终状态成功。
    let completed = client
        .data_plane
        .load_pending_analysis_job("queue-policy-retry")
        .await
        .expect("retry result")
        .expect("retry audit");
    assert_eq!(
        completed.state,
        vulcan_workspace_store::vmm_data_plane::VMM_PENDING_ANALYSIS_COMPLETED,
        "{:?}",
        completed.last_error
    );
    assert_eq!(completed.attempt_count, 1);
    assert!(
        client
            .runtime
            .storage_snapshot()
            .expect("success state")
            .memory_sessions
            .iter()
            .find(|session| session.session_source_id.as_str() == "queue-policy-session")
            .expect("successful session")
            .last_extraction_observed_at_ns
            > previous_boundary
    );
    // Read committed SQLite through VLDB, not only the runtime's in-memory projection.
    // 通过 VLDB 读取已提交 SQLite，而不仅检查运行时内存投影。
    let mut persisted: vulcan_memorymesh_core::StorageSnapshot = serde_json::from_str(
        &client
            .data_plane
            .load_core_snapshot_json()
            .await
            .expect("persisted Core snapshot")
            .expect("snapshot exists"),
    )
    .expect("typed persisted state");
    // Compare each authoritative result group and retain both attempts for the exact retried turn.
    // 比较每组权威结果，并保留准确重试回合的两次尝试。
    let mut live = client
        .runtime
        .storage_snapshot()
        .expect("final live projection");
    assert_eq!(persisted.dataset.turns, live.dataset.turns);
    assert_eq!(persisted.memory_sessions, live.memory_sessions);
    // StorageSnapshot projects analysis_attempts from a HashMap; compare all records by their actual composite key.
    // StorageSnapshot 从 HashMap 投影 analysis_attempts；按实际复合键比较全部记录。
    // Sorting retains duplicates and every field, unlike comparing only counts or a deduplicated set.
    // 排序保留重复项及所有字段，不仅比较数量，也不使用去重集合。
    for snapshot in [&mut persisted, &mut live] {
        snapshot.analysis_attempts.sort_by(|left, right| {
            left.turn_id
                .as_str()
                .cmp(right.turn_id.as_str())
                .then(left.analysis_generation.cmp(&right.analysis_generation))
        });
    }
    assert_eq!(persisted.analysis_attempts, live.analysis_attempts);
    let attempts = persisted
        .analysis_attempts
        .iter()
        .filter(|attempt| Some(attempt.turn_id.as_str()) == failed.turn_id.as_deref())
        .collect::<Vec<_>>();
    assert_eq!(attempts.len(), 2);
    assert_eq!(
        attempts
            .iter()
            .map(|attempt| attempt.analysis_generation)
            .collect::<Vec<_>>(),
        vec![1, 2]
    );
    assert_ne!(
        attempts[0].mutation_identity.operation_id,
        attempts[1].mutation_identity.operation_id
    );
    assert_ne!(
        attempts[0].mutation_identity.idempotency_key,
        attempts[1].mutation_identity.idempotency_key
    );
    // Both L1 attempts keep the exact target and caller-owned one-turn context.
    // 两次 L1 尝试保留准确目标及调用方的一轮上下文。
    let captured = port.requests.lock().expect("retry captures");
    assert_eq!(captured.len(), 7);
    for request in &captured[5..] {
        // Parse the request actually received by the host boundary, not a separately constructed expectation.
        // 解析宿主边界实际收到的请求，而不是另行构造的期望输入。
        let input = analysis_input(request);
        assert_eq!(
            input["target_turn"]["turn_id"].as_str(),
            failed.turn_id.as_deref()
        );
        assert_eq!(
            input["reference_turns"]
                .as_array()
                .expect("retry history")
                .len(),
            1
        );
        assert_eq!(
            input["reference_turns"][0]["turn_id"].as_str(),
            previous_job.turn_id.as_deref()
        );
    }
}
