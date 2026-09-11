//! Actual prompt submission, HTTPS streaming and native tools during a suspended index update.
//! 暂停索引更新期间的实际提示词提交、HTTPS 流式输出及原生工具。

use super::vce_configuration_readiness_tests::configured_state;
use super::vce_stream_fixture::{PrivateModelServer, StreamFixtureSecrets};
use super::*;
use futures_util::FutureExt;
use std::time::Duration;
use vulcan_service_api::{ServiceEvent, ServiceEventSink, VceJobAction, VceJobSubmitRequest};
use vulcan_workspace_store::vce_data_plane::{VceDataPlaneFactory, VceJobState};
use vulcan_workspace_store::workspace_store::{load_workspace_index_snapshot, persist_new_project};

/// Own production projection acknowledgement for this isolated, serial integration test process.
/// 为此隔离且串行的集成测试进程持有生产投影确认开关。
struct StreamProjectionGuard;

impl StreamProjectionGuard {
    /// Enable the same durability handshake as service bootstrap; no inputs, returns its cleanup owner.
    /// 启用与服务启动相同的可靠性握手；无输入，返回负责清理的所有者。
    fn enable() -> Self {
        vulcan_agent_runtime::agent_runtime::tool_runtime_bridge::set_runtime_event_projection_barriers_enabled(true)
            .expect("enable production tool durability handshake");
        Self
    }
}

impl Drop for StreamProjectionGuard {
    /// Restore the test process default after its conversation has stopped, including assertion unwinding.
    /// 在测试对话停止后恢复测试进程默认值，包含断言展开清理路径。
    fn drop(&mut self) {
        if let Err(error) = vulcan_agent_runtime::agent_runtime::tool_runtime_bridge::set_runtime_event_projection_barriers_enabled(false) {
            eprintln!("private stream projection cleanup failed: {error}");
        }
    }
}

/// Capture actual emitted UI events, not a mailbox acknowledgement or injected final transcript.
/// 捕获实际发出的界面事件，而不是邮箱应答或注入的最终转录。
#[derive(Default)]
struct StreamEvents {
    /// All events belong to this fixture's single private conversation.
    /// 全部事件属于本夹具唯一私有对话。
    events: Mutex<Vec<ServiceEvent>>,
}

impl ServiceEventSink for StreamEvents {
    /// Retain the exact service event; returns after synchronous observation without I/O.
    /// 保留精确服务事件；无输入输出操作，在同步观察后返回。
    fn emit_service_event(&self, event: ServiceEvent) {
        self.events
            .lock()
            .expect("stream event capture")
            .push(event);
    }
}

impl StreamEvents {
    /// Check actual V1 live deltas for the given fixture text, never unrelated logs or request prompts.
    /// 检查实际 V1 实时增量是否含给定夹具文本，不检查无关日志或请求提示词。
    fn contains_text(&self, text: &str) -> bool {
        self.events.lock().expect("events").iter().any(|event| {
            matches!(event, ServiceEvent::ConversationEventDeltaV1(delta)
                if serde_json::to_string(delta).expect("V1 event JSON").contains(text))
        })
    }

    /// Report whether the actual service successfully settled its only conversation; no parameters.
    /// 返回实际服务是否成功结束唯一对话；无参数。
    fn settled(&self) -> bool {
        self.completed_runs() > 0
    }

    /// Count actual successful settlements so a previous turn cannot satisfy a continuation assertion.
    /// 统计实际成功结算次数，避免前一轮满足继续执行断言。
    /// No inputs; returns only completed run events from this private conversation.
    /// 无输入；仅返回此私有对话的成功运行事件数。
    fn completed_runs(&self) -> usize {
        self.events
            .lock()
            .expect("events")
            .iter()
            .filter(|event| {
                matches!(event, ServiceEvent::ConversationRunSettled(settled)
                    if settled.outcome == vulcan_service_api::ConversationRunOutcome::Completed)
            })
            .count()
    }

    /// Return the last six conversation events for failed assertions; no parameters or credential headers.
    /// 返回最近六个对话事件用于失败断言；无参数，不包含凭据头。
    fn diagnostic_tail(&self) -> String {
        // Scope evidence to this private conversation, excluding workbench and unrelated service snapshots.
        // 将证据限定到此私有对话，排除工作台及无关服务快照。
        let events = self.events.lock().expect("diagnostic events");
        // Reverse order makes the terminal failure appear before earlier text deltas.
        // 倒序使终态失败出现在较早文本增量之前。
        let tail: Vec<_> = events
            .iter()
            .rev()
            .filter(|event| {
                matches!(
                    event,
                    ServiceEvent::ConversationEventDeltaV1(_)
                        | ServiceEvent::ConversationRunSettled(_)
                )
            })
            .take(6)
            .collect();
        serde_json::to_string(&tail).expect("private diagnostic JSON")
    }
}

/// Observe the exact durable job until terminal; return its state within a bounded timeout.
/// 观察精确持久作业直到终态，在有界超时内返回其状态。
/// Manager, project and job identify private storage; no polling call creates work.
/// 管理器、项目及作业标识私有存储，轮询不创建工作。
async fn finish_index(manager: &vce::VceRuntimeManager, project: &str, job: &str) -> VceJobState {
    tokio::time::timeout(Duration::from_secs(30), async {
        loop {
            // Read the real job rather than trusting a cached UI flag.
            // 读取真实作业，不信任缓存界面标志。
            let receipt = manager
                .get_job(project, Some(job))
                .await
                .expect("index progress")
                .expect("job exists");
            if receipt.state.is_terminal() {
                assert_eq!(
                    receipt.state,
                    VceJobState::Succeeded,
                    "index receipt: {receipt:?}"
                );
                return receipt.state;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("bounded index completion")
}

/// Prove that an in-flight embedding update cannot block real conversation streaming or native tools.
/// 证明执行中的向量更新不能阻塞真实对话流或原生工具。
/// No input; all model HTTP traffic and storage use isolated local fixtures, without user credentials.
/// 无输入；全部模型 HTTP 流量及存储使用隔离本地夹具，不使用用户凭据。
#[test]
fn real_host_storage_stream_and_tools_continue_during_index_update() {
    run_stream_interleaving_fixture(true);
}

/// Verify ordinary conversation streaming and tools independently of the additional Continue boundary.
/// 独立验证普通对话流及工具，不以附加的继续执行边界替代该验收。
/// No inputs; uses only private local model and storage fixtures.
/// 无输入；仅使用私有本地模型和存储夹具。
#[test]
fn real_host_storage_stream_and_tools_complete_during_index_update() {
    run_stream_interleaving_fixture(false);
}

/// Execute the real index/conversation fixture, optionally exercising actual Stop and Continue services.
/// 执行真实索引／对话夹具，可按用例要求额外执行实际停止和继续服务。
/// `exercise_continue` selects the acceptance boundary; assertion failures propagate to the caller.
/// `exercise_continue` 选择验收边界；断言失败传播给调用方。
fn run_stream_interleaving_fixture(exercise_continue: bool) {
    // Execute only this test in a child so process-wide startup switches cannot affect sibling tests.
    // 仅在子进程执行此测试，防止进程级启动开关影响同一批次的其他测试。
    const CHILD_MARKER: &str = "VULCAN_VCE_STREAM_ISOLATED_CHILD";
    if std::env::var_os(CHILD_MARKER).is_none() {
        // Pass the marker only to the child; never mutate the parent's process environment.
        // 仅向子进程传递标记，不修改父进程环境。
        let mut command = std::process::Command::new(
            std::env::current_exe().expect("current kernel test executable"),
        );
        // Select the exact matching test entry, never a broad filter that might recurse into sibling tests.
        // 选择精确匹配的测试入口，不使用可能递归进入其他用例的宽泛筛选。
        let test_name = if exercise_continue {
            "tests::vce_stream_interleaving_tests::real_host_storage_stream_and_tools_continue_during_index_update"
        } else {
            "tests::vce_stream_interleaving_tests::real_host_storage_stream_and_tools_complete_during_index_update"
        };
        command
            .args([
                "--exact",
                test_name,
                "--ignored",
                "--test-threads=1",
                "--nocapture",
            ])
            .env(CHILD_MARKER, "1");
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
        }
        // Preserve the actual child result and diagnostics instead of manufacturing a parent pass.
        // 保留子进程实际结果和诊断，不能在父进程伪造通过。
        let output = command.output().expect("run isolated VCE stream test");
        assert!(
            output.status.success(),
            "isolated VCE stream test failed: {}\n{}\n{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
        return;
    }
    // Only this private process owns the production startup switch and its cleanup.
    // 只有此私有进程持有生产启动开关及其清理责任。
    let _projection = StreamProjectionGuard::enable();
    // Reuse the real chat entrypoint's runtime configuration, limiting only the fixture worker count.
    // 复用实际聊天入口的运行时配置，仅限制夹具工作线程数量。
    let executor = Arc::new(
        crate::service_bridge_io::webui_service_runtime_builder()
            .worker_threads(2)
            .build()
            .expect("private stream executor"),
    );
    // Reuse actual bundled model declarations and exact private account identities.
    // 复用实际内置模型声明和精确私有账号身份。
    let (state, _, mut configuration) = configured_state();
    // Full conversation dispatch also needs the host's active chat selection, not only VCE consumer routes.
    // 完整对话派发还需要宿主活动聊天选择，而不只是 VCE 消费者路由。
    configuration.active = Some(vulcan_provider_adapters::ActiveAiModelSelection {
        provider_id: "alibaba".into(),
        model_id: "qwen3.5-flash".into(),
        connection_id: "vce-readiness-account".into(),
    });
    // Preserve the session identity across its actual indexed V1 storage and workspace.
    // 在实际索引式 V1 存储与工作区之间保留会话身份。
    let session_id = state
        .lock()
        .expect("state")
        .workspace_snapshot
        .as_ref()
        .expect("workspace")
        .selected_session;
    // This object owns the exact private host-storage data root.
    // 此对象持有精确私有宿主存储数据根。
    let storage = TestHostRuntimeRoot::new(session_id);
    // Keep generated Lance filenames below the platform path limit without changing production paths.
    // 保持生成的 Lance 文件名低于平台路径限制，不改变生产路径。
    let index_root = tempfile::tempdir().expect("short private index root");
    executor.block_on(async {
        // The HTTPS server is created before any configured inference can run.
        // 在任何已配置推理运行之前创建 HTTPS 服务器。
        let server = PrivateModelServer::start()
            .await
            .expect("private HTTPS inference");
        // Real protocol execution uses only a fake immutable secret store and this HTTP client.
        // 真实协议执行只使用假不可变秘密存储及此 HTTP 客户端。
        let public_ai = Arc::new(
            vulcan_provider_adapters::PublicAiRuntime::first_batch_with_http_client_for_testing(
                Arc::new(StreamFixtureSecrets),
                server.http.clone(),
            )
            .expect("registered HTTP test runtime"),
        );
        public_ai
            .replace_configuration(configuration.clone())
            .expect("register exact private routes");
        // Use the normal host registration boundary shared by indexing and conversation dispatch.
        // 使用索引与对话派发共享的普通宿主注册边界。
        let inference = Arc::new(
            crate::configured_inference::ConfiguredInferenceService::new(public_ai.clone()),
        );
        // This port remains owned by the host, not by the semantic engine.
        // 此端口仍由宿主持有，而非语义引擎持有。
        let ai: Arc<dyn vulcan_ai_contract::HostAiPort> = Arc::new(
            crate::internal_ai::ConfiguredInferenceHostAiPort::new(inference.clone()),
        );
        // Attach only the fixture's application binding.
        // 只附着夹具应用绑定。
        let mut controller = storage.attached_host_runtime_handle().await;
        // Cleanup covers both initialization and stream assertions.
        // 清理同时覆盖初始化及流断言。
        let outer = std::panic::AssertUnwindSafe(async {
            // Actual chat telemetry requires its own attached space, unlike the application-only fixture.
            // 与仅应用夹具不同，实际聊天遥测需要自己的已附着空间。
            let statistics_root = storage.data_root().join("stream-statistics");
            fs::create_dir_all(&statistics_root).expect("private statistics directory");
            controller.system_statistics_sqlite_path = statistics_root.join("usage.sqlite3");
            controller.system_statistics_sqlite_binding_id = "stream-statistics".into();
            vulcan_workspace_store::data_plane::prepare_application_schema_for_test(&controller)
                .await
                .expect("private application tables");
            // One real source file is enough to distinguish old indexed content from current file content.
            // 一个真实源码文件足以区分旧索引内容与当前文件内容。
            let source = storage.data_root().join("stream-source");
            fs::create_dir_all(&source).expect("private project root");
            fs::write(
                source.join("lib.rs"),
                "pub fn generation_marker() -> &'static str { \"vce_old_generation\" }\n",
            )
            .expect("tiny original source");
            // Auxiliary inference stays configured but is intentionally disabled for this project query policy.
            // 辅助推理保持已配置，但本项目查询策略明确禁用它。
            let settings = vulcan_workbench_model::ProjectVceSettings {
                enabled: true,
                allow_query_rewrite: false,
                allow_rerank: false,
                ..Default::default()
            };
            // Actual V1 fixture metadata belongs to this stable project identifier.
            // 实际 V1 夹具元数据属于该稳定项目标识。
            let project = vulcan_workbench_model::TaskProjectSummary::new(
                "v1-test-project",
                "VCE stream fixture",
                source.display().to_string(),
                false,
                0,
                None,
                None,
                vec![session_id],
            )
            .with_vce_settings(settings.clone());
            persist_new_project(controller.clone(), &project)
                .await
                .expect("actual project row");
            // The production loader supplies all session/project indexes for real prompt submission.
            // 生产加载器为真实提示词提交提供全部会话／项目索引。
            let workspace = load_workspace_index_snapshot(controller.clone())
                .await
                .expect("actual workspace");
            // Events retain real streaming, lifecycle and progress outputs.
            // 事件保留真实流式、生命周期及进度输出。
            let events = Arc::new(StreamEvents::default());
            // The same emitter is passed to conversation and index jobs.
            // 同一个发射器传给对话及索引作业。
            let emitter = ServiceEventEmitter::new(vec![events.clone()]);
            // Mirror desktop startup registration so V1 publishes real text deltas to this observer.
            // 对齐桌面启动注册，使 V1 向此观察者发布真实文本增量。
            crate::conversation_events_v1::service_install_conversation_event_v1_emitter(
                &state,
                emitter.clone(),
            )
            .expect("install actual V1 event publisher");
            // Only VCE receives a short private root; the same real controller still owns all bindings.
            // 只有 VCE 接收短私有根；同一个真实控制器仍持有全部绑定。
            let mut index_controller = controller.clone();
            index_controller.data_root = index_root.path().to_path_buf();
            // Index storage is private but uses the production data-plane factory.
            // 索引存储私有，但使用生产数据平面工厂。
            let factory = VceDataPlaneFactory::bootstrap(&index_controller)
                .await
                .expect("VCE storage");
            // Publication checks are the real live host checks, never a test readiness boolean.
            // 发布检查为真实实时宿主检查，绝非测试就绪布尔值。
            let manager = vce::VceRuntimeManager::new(
                ai.clone(),
                factory.clone(),
                emitter.clone(),
                vce::vce_publication_authority(&state),
            );
            {
                // Install the exact registered services without saving any user configuration.
                // 安装精确已注册服务，不保存任何用户配置。
                let mut guard = state.lock().expect("state");
                guard.controller_runtime = Some(controller.clone());
                guard.workspace_snapshot = Some(workspace);
                guard.service_runtime = Some(executor.clone());
                guard.vce_runtime = Some(manager.clone());
                // Only the private bridge's inference objects are replaced.
                // 只替换私有桥接的推理对象。
                let bridge = guard.agent_runtime.as_mut().expect("bridge");
                bridge.public_ai_runtime = public_ai.clone();
                bridge.configured_inference_service = inference;
                bridge.internal_ai_port = ai;
            }
            // Release barriers and stop the actual conversation on every assertion path.
            // 在每条断言路径上释放屏障并停止实际对话。
            let outcome = std::panic::AssertUnwindSafe(async {
                assert!(vce::vce_configuration_readiness(&state).status.ready);
                // Explicit initialization is the only first-build trigger.
                // 明确初始化是唯一首次建库触发点。
                let initial = vce::service_submit_vce_job(
                    state.clone(),
                    VceJobSubmitRequest {
                        retry_of_job_id: None,
                        project_id: project.id.clone(),
                        action: VceJobAction::Initialize,
                        idempotency_key: "stream-initialize".into(),
                        expected_settings_revision: settings.revision,
                        expected_checkpoint_version: None,
                    },
                )
                .await
                .expect("explicit initialization");
                assert_eq!(
                    finish_index(&manager, &project.id, &initial.job_id).await,
                    VceJobState::Succeeded
                );
                // Query the actual initial checkpoint through the real registered model/HTTP stack.
                // 通过真实已注册模型／HTTP 链查询实际初始检查点。
                let baseline = manager
                    .retrieve(&project.id, "generation_marker")
                    .await
                    .expect("initial query");
                assert!(
                    baseline
                        .result
                        .formatted_context
                        .contains("vce_old_generation")
                );
                fs::write(
                    source.join("lib.rs"),
                    "pub fn generation_marker() -> &'static str { \"vce_new_generation\" }\n",
                )
                .expect("tiny changed source");
                server
                    .signals
                    .pause_embedding
                    .store(true, Ordering::Release);
                // Suspend the real next vector HTTP response, not the whole session executor.
                // 暂停真实下一向量 HTTP 响应，不暂停整个会话执行器。
                let update = vce::service_submit_vce_job(
                    state.clone(),
                    VceJobSubmitRequest {
                        retry_of_job_id: None,
                        project_id: project.id.clone(),
                        action: VceJobAction::Synchronize,
                        idempotency_key: "stream-update".into(),
                        expected_settings_revision: settings.revision,
                        expected_checkpoint_version: Some(baseline.checkpoint_version),
                    },
                )
                .await
                .expect("explicit update");
                tokio::time::timeout(
                    Duration::from_secs(15),
                    server.signals.embedding_entered.acquire(),
                )
                .await
                .expect("update actually reaches HTTP")
                .expect("barrier")
                .forget();
                // Verify the real inference and read-view path independently before adding tool dispatch.
                // 在加入工具派发前，独立验证真实推理与读取视图路径。
                let concurrent_query = tokio::time::timeout(
                    Duration::from_secs(5),
                    manager.retrieve(&project.id, "generation_marker"),
                )
                .await;
                assert!(
                    concurrent_query.is_ok(),
                    "direct concurrent query stalled; embedding HTTP requests: {}",
                    server.signals.embedding_count.load(Ordering::SeqCst)
                );
                assert!(concurrent_query.expect("bounded concurrent query")
                    .expect("concurrent query succeeds").result.formatted_context.contains("vce_old_generation"));
                // This is the real user submit service, including V1 admission, request preparation and stream dispatch.
                // 这是真实用户提交服务，包含 V1 准入、请求准备及流派发。
                let ack = crate::conversation_submission::service_submit_conversation_prompt(
                    emitter.clone(),
                    state.clone(),
                    "vce-stream-prompt".into(),
                    Some(project.id.clone()),
                    Some(session_id.to_string()),
                    None,
                    "Read generation_marker using semantic search and the file tool.".into(),
                    None,
                    None,
                    None,
                    Some(crate::conversation_submission::SubmissionModelSelection {
                        provider_id: "alibaba".into(),
                        model_id: "qwen3.5-flash".into(),
                        connection_id: "vce-readiness-account".into(),
                    }),
                    None,
                    None,
                    None,
                    crate::conversation_submission::ExistingSessionHydrationPolicy::HydrateIfEmpty,
                    crate::conversation_submission::ConversationSubmissionMode::UserPrompt,
                )
                .await
                .expect("normal conversation starts while update waits");
                assert!(!ack.queued);
                assert_eq!(ack.conversation_id, session_id.to_string());
                // The server withholds tool chunks until the actual host publishes its first text delta.
                // 服务器保留工具分块，直到实际宿主发布首个文本增量。
                assert!(
                    tokio::time::timeout(Duration::from_secs(20), async {
                        while !events.contains_text("Inspecting the indexed source.") {
                            tokio::time::sleep(Duration::from_millis(20)).await;
                        }
                    })
                    .await
                    .is_ok(),
                    "actual streamed text while indexing: {}",
                    events.diagnostic_tail()
                );
                assert_eq!(
                    manager
                        .get_job(&project.id, Some(&update.job_id))
                        .await
                        .expect("update")
                        .expect("receipt")
                        .state,
                    VceJobState::Running
                );
                server.signals.release_tools.add_permits(1);
                // The actual tool executor must return old semantic content and new physical file content to the next HTTP request.
                // 实际工具执行器必须将旧语义内容和新物理文件内容返回下一 HTTP 请求。
                assert!(
                    tokio::time::timeout(Duration::from_secs(25), async {
                        while !events.settled()
                            || !events
                                .contains_text("Search and file read completed while indexing.")
                        {
                            tokio::time::sleep(Duration::from_millis(20)).await;
                        }
                    })
                    .await
                    .is_ok(),
                    "conversation and tools finish before index update; embedding HTTP requests: {}; events: {}",
                    server.signals.embedding_count.load(Ordering::SeqCst),
                    events.diagnostic_tail()
                );
                assert_eq!(server.signals.chat_count.load(Ordering::SeqCst), 2);
                // Continue is a separate explicit acceptance case; both cases keep the update suspended.
                // 继续执行是独立明确的验收用例；两个用例均保持更新暂停。
                if exercise_continue {
                // Start another real user turn, stop its live text stream, then invoke the actual Continue service.
                // 开始另一轮真实用户输入，停止其存活文本流，然后调用实际继续服务。
                let interrupted = crate::conversation_submission::service_submit_conversation_prompt(
                    emitter.clone(), state.clone(), "vce-stream-before-continue".into(),
                    Some(project.id.clone()), Some(session_id.to_string()), None,
                    "Check generation_marker again after I continue this response.".into(),
                    None, None, None, None, None, None, None,
                    crate::conversation_submission::ExistingSessionHydrationPolicy::HydrateIfEmpty,
                    crate::conversation_submission::ConversationSubmissionMode::UserPrompt,
                ).await.expect("second real user turn");
                assert!(!interrupted.queued);
                assert!(tokio::time::timeout(Duration::from_secs(15), async {
                    while !events.contains_text("Awaiting user stop before continuation.") {
                        tokio::time::sleep(Duration::from_millis(20)).await;
                    }
                }).await.is_ok(), "second turn streams before stop: {}", events.diagnostic_tail());
                service_stop_conversation_run(emitter.clone(), state.clone(), session_id.to_string(),
                    Some(StopConversationRunMode::MainAndSubagents))
                    .await.expect("real user stop completes durable interruption");
                // Stop has joined the old stream and durably interrupted its API before returning.
                // 停止服务在返回前已等待旧流退出并可靠记录 API 中断。
                assert!(!state.lock().expect("stopped state").active_stream_tasks.contains_key(&session_id));
                // No supplemental prompt or route override may substitute a new user turn for Continue.
                // 不得通过补充提示词或路由覆盖将新用户回合冒充继续动作。
                let completions_before_continue = events.completed_runs();
                let continued = crate::conversation_submission::service_continue_conversation(
                    emitter.clone(), state.clone(), session_id.to_string(), "vce-stream-continue".into(),
                    None, None, None, None, None, None,
                ).await.expect("actual V1 Continue dispatch during indexing");
                assert!(!continued.queued);
                assert_eq!(continued.conversation_id, session_id.to_string());
                assert!(tokio::time::timeout(Duration::from_secs(20), async {
                        while events.completed_runs() <= completions_before_continue
                        || !events.contains_text("Continued search and file read completed while indexing.") {
                        tokio::time::sleep(Duration::from_millis(20)).await;
                    }
                }).await.is_ok(), "continued tools and text settle before update: {}", events.diagnostic_tail());
                assert_eq!(server.signals.chat_count.load(Ordering::SeqCst), 5);
                }
                assert_eq!(
                    manager
                        .get_job(&project.id, Some(&update.job_id))
                        .await
                        .expect("still updating")
                        .expect("update receipt")
                        .state,
                    VceJobState::Running
                );
                server.signals.release_embedding.add_permits(1);
                assert_eq!(
                    finish_index(&manager, &project.id, &update.job_id).await,
                    VceJobState::Succeeded
                );
                // Only after explicit release may the newly indexed generation become searchable.
                // 只有明确释放后，新索引代次才可检索。
                let after = manager
                    .retrieve(&project.id, "generation_marker")
                    .await
                    .expect("published update query");
                assert!(after.checkpoint_version > baseline.checkpoint_version);
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
                // Revoke an actual configured route while another nonempty vector request is in flight.
                // 在另一条非空向量请求执行期间撤销实际已配置路由。
                fs::write(
                    source.join("lib.rs"),
                    "pub fn generation_marker() -> &'static str { \"vce_revoked_generation\" }\n",
                ).expect("source change for revoked update");
                server.signals.pause_embedding.store(true, Ordering::Release);
                // This command uses the latest committed checkpoint and the unchanged project revision.
                // 此命令使用最新已提交检查点及未改变的项目修订号。
                let revoked_update = vce::service_submit_vce_job(
                    state.clone(), VceJobSubmitRequest {
                        retry_of_job_id: None,
                        project_id: project.id.clone(),
                        action: VceJobAction::Synchronize,
                        idempotency_key: "stream-revoked-update".into(),
                        expected_settings_revision: settings.revision,
                        expected_checkpoint_version: Some(after.checkpoint_version),
                    },
                ).await.expect("submit nonempty revocation update");
                tokio::time::timeout(Duration::from_secs(15), server.signals.embedding_entered.acquire())
                    .await.expect("revoked update reaches actual vector HTTP")
                    .expect("revocation barrier").forget();
                // Removing the rewrite route invalidates engine readiness even when rewrite is disabled for this query.
                // 即使本项目查询关闭改写，删除改写路由仍使引擎准备度失效。
                let retained_handler = vce::vce_retrieval_handler(state.clone(), manager.clone());
                let mut revoked_configuration = configuration.clone();
                revoked_configuration.inference_profiles.get_mut(vce::VCE_CONSUMER_PROFILE_ID)
                    .expect("configured VCE profile").llm_routes.remove(vce::VCE_QUERY_REWRITE_PURPOSE_ID)
                    .expect("previously configured rewrite route");
                public_ai.replace_configuration(revoked_configuration).expect("revoke actual host route");
                assert!(!vce::vce_configuration_readiness(&state).status.ready);
                assert!(!vce::vce_project_tool_enabled(&state, &project.id));
                // A retained callback must respect current authority, not its construction-time readiness.
                // 保留回调必须遵守当前权威，不使用构造时准备度。
                assert!(retained_handler.retrieve(project.id.clone(), "generation_marker".into()).await.is_err());
                server.signals.release_embedding.add_permits(1);
                // Wait for the precise durable failed job; a successful old-contract publication is forbidden.
                // 等待精确持久作业失败；禁止旧契约成功发布。
                let revoked_receipt = tokio::time::timeout(Duration::from_secs(30), async {
                    loop {
                        let receipt = manager.get_job(&project.id, Some(&revoked_update.job_id))
                            .await.expect("revoked job status").expect("durable revoked job");
                        if receipt.state.is_terminal() { break receipt; }
                        tokio::time::sleep(Duration::from_millis(20)).await;
                    }
                }).await.expect("revoked job reaches terminal");
                assert_eq!(revoked_receipt.state, VceJobState::Failed, "revoked job: {revoked_receipt:?}");
                public_ai.replace_configuration(configuration.clone()).expect("restore exact host configuration");
                assert!(vce::vce_project_tool_enabled(&state, &project.id));
                // Restoring the exact route reuses the last good index; it must not make the rejected content searchable.
                // 恢复精确路由复用最后有效索引，不能使被拒绝的内容变得可查询。
                let recovered = manager.retrieve(&project.id, "generation_marker").await.expect("recover existing index");
                assert_eq!(recovered.checkpoint_version, after.checkpoint_version);
                assert!(recovered.result.formatted_context.contains("vce_new_generation"));
                assert!(!recovered.result.formatted_context.contains("vce_revoked_generation"));
                assert_eq!(manager.get_job(&project.id, None).await.expect("latest job")
                    .expect("latest durable job").job_id, revoked_update.job_id,
                    "configuration restoration must not submit an implicit index job");
                assert!(
                    state
                        .lock()
                        .expect("state")
                        .vmm_controller_registry
                        .snapshots()
                        .expect("VMM registry")
                        .is_empty()
                );
                assert_eq!(
                    public_ai
                        .configuration_snapshot()
                        .expect("private config unchanged"),
                    configuration
                );
            })
            .catch_unwind()
            .await;
            server.signals.release_embedding.add_permits(1);
            server.signals.release_tools.add_permits(1);
            let _ = service_stop_conversation_run(
                emitter,
                state.clone(),
                session_id.to_string(),
                Some(StopConversationRunMode::MainAndSubagents),
            )
            .await;
            manager.shutdown().await.expect("drain private VCE");
            factory.shutdown().await.expect("release VCE storage");
            if let Err(panic) = outcome {
                std::panic::resume_unwind(panic);
            }
        })
        .catch_unwind()
        .await;
        server.shutdown().await;
        if let Err(panic) = outer {
            std::panic::resume_unwind(panic);
        }
    });
}
