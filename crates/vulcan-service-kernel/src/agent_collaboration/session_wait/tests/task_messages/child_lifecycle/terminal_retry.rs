//! Real disk publication retries retain terminal evidence without blocking nested admission.
//! 实际磁盘发布重试保留终态凭据，且不阻塞嵌套准入。

use super::*;
use crate::agent_stream::{
    ProviderTelemetryHookGuard, ProviderV1StreamSink, retry, startup_rejection::capture_owner,
};
use crate::session_runtime_registry_v1::{
    SessionHandle, SessionPersistenceBackend, SessionRuntimeRegistry,
};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use vulcan_agent_runtime::agent_runtime::tool_runtime_bridge::attach_test_runtime_event_projection_barrier;
use vulcan_agent_runtime::agent_runtime::{
    events::{ExecutionStreamEvent, RuntimeEvent, RuntimeEventKind},
    task_runtime::DetachedTaskRuntimeEvent,
};
use vulcan_workspace_store::workspace_store::{
    SessionV1FlushResult, SessionV1StorageError, SessionV1StorageLocator, SessionV1StoreError,
    flush_session_v1_candidate, load_session_v1_full,
};

/// Rejects parent writes until nested-event processing explicitly releases the fault.
/// 拒绝父写入，直到嵌套事件处理明确解除故障。
struct RecoveringDiskWriter {
    /// Real parent storage remains authoritative after the injected failure.
    /// 注入失败后，真实父存储仍保持权威。
    locator: SessionV1StorageLocator,
    /// The test controls storage recovery without changing domain state.
    /// 测试控制存储恢复，不修改领域状态。
    unavailable: AtomicBool,
    /// Count failed physical writes before any result can be acknowledged.
    /// 记录结果能够确认之前失败的物理写入次数。
    failures: AtomicUsize,
}

impl SessionPersistenceBackend for RecoveringDiskWriter {
    /// Flushes `candidate` to actual storage after recovery; returns an injected I/O failure while blocked.
    /// 恢复后将 `candidate` 刷入实际存储；阻塞期间返回注入的 I/O 失败。
    fn flush(
        &self,
        candidate: &vulcan_session_core::v1::document::FlushCandidate,
    ) -> Result<SessionV1FlushResult, SessionV1StoreError> {
        if self.unavailable.load(Ordering::SeqCst) {
            self.failures.fetch_add(1, Ordering::SeqCst);
            return Err(SessionV1StoreError::Storage(SessionV1StorageError::io(
                std::path::PathBuf::from("terminal-retry-fixture"),
                "injected parent publication unavailable",
            )));
        }
        flush_session_v1_candidate(&self.locator, candidate)
    }
}

/// Actual failed publications recover after EOF, preserve mailbox identity and keep nested starts responsive.
/// 实际失败发布在流结束后恢复，保留邮箱身份，并保持嵌套启动可响应。
/// Returns no value; verifies both startup abort and natural failure through the production driver.
/// 不返回值；通过生产驱动器验证启动中止与自然失败。
#[tokio::test]
async fn terminal_retry_recovers_disk_failure_after_eof_without_blocking_stream() {
    for startup in [false, true] {
        // The fixture owns real parent storage, authentication and an open parent tool API.
        // 夹具拥有真实父存储、认证及开放父工具 API。
        let (_root, invocation, _target, _context) = message_fixture(false).await;
        // One immutable physical child and execution are reused across all retry attempts.
        // 所有重试共用同一个不可变物理子任务及执行。
        let child_id = Uuid::new_v4();
        // The root action and reservation use the same execution ID.
        // 根动作与预留使用相同执行标识。
        let execution = CollaborationExecutionId::try_new("retry-child-result").unwrap();
        // Production admission establishes the embedded child before parent ownership.
        // 生产准入先建立嵌入子任务，再建立父归属。
        let child = crate::session_v1_lifecycle::create_or_load_embedded_task_session_v1(
            &invocation.registry,
            TaskV1ExecutionFacts {
                root_session_id: invocation.source_session_id,
                parent_session_id: invocation.source_session_id,
                task_id: child_id,
                execution_id: execution.as_str().into(),
                prompt: "child work".into(),
                display_title: "child".into(),
                is_resumed: false,
                admitted_at_ms: 5,
            },
        )
        .unwrap();
        // Capture the actual admission owner before any terminal closes its turn.
        // 在任何终态关闭回合前捕获实际准入所有者。
        let owner = capture_owner(&child, execution.as_str()).unwrap();
        // Participant facts are resolved from production locators.
        // 参与者事实由生产定位器解析。
        let participant = participants::participant_for_handle(&invocation, child_id, &child, None)
            .await
            .unwrap();
        invocation
            .source_handle
            .mutate_durable(
                SessionMutationAdmission::ControlPlane,
                SessionFlushReason::MetadataChanged,
                |doc| {
                    doc.reserve_child_task(ReserveChildTaskInput {
                        source_tool_name: invocation.source_tool_id.clone(),
                        operation_id: CollaborationOperationId::try_new("retry-reserve").unwrap(),
                        execution_id: execution.clone(),
                        task_id: participant.task_id.clone().unwrap(),
                        parent_turn_id: TurnId::try_new(invocation.source_turn_id).unwrap(),
                        child: participant,
                        reserved_at_ms: 5,
                    })
                },
            )
            .unwrap();
        // Replace only the test fixture's writer before registering the driver-visible owners.
        // 在注册驱动器可见所有者之前，仅替换测试夹具写入器。
        let writer = Arc::new(RecoveringDiskWriter {
            locator: invocation.source_handle.storage_locator().unwrap(),
            unavailable: AtomicBool::new(true),
            failures: AtomicUsize::new(0),
        });
        // Preserve real disk loading while faulting parent publication only.
        // 保留真实磁盘加载，仅使父发布发生故障。
        let parent = SessionHandle::with_storage_backend(
            invocation.source_session_id,
            writer.locator.clone(),
            invocation.source_handle.read(Clone::clone).unwrap(),
            writer.clone(),
        );
        // All subsequent production lookup uses this sole fixture registry.
        // 后续所有生产查询均使用此唯一夹具注册表。
        let registry = Arc::new(SessionRuntimeRegistry::default());
        registry.register(parent.clone()).unwrap();
        registry.register(child.clone()).unwrap();
        // Runtime feedback is gated until the original parent result is durable.
        // 原父结果可靠落盘之前，运行时反馈受门禁限制。
        let runtime = crate::tests::test_support::test_agent_runtime_bridge();
        runtime
            .task_runtime
            .expect_durable_task_return(child_id, execution.as_str())
            .unwrap();
        {
            // Retain exact listener ownership and suppress real Provider continuation in this offline test.
            // 保留精确监听器归属，并在离线测试中抑制真实 Provider 续跑。
            let mut state = invocation.state.lock().unwrap();
            state.session_runtime_registry = registry;
            state.agent_runtime = Some(runtime.clone());
            state
                .detached_subagent_task_listeners
                .insert(child_id, execution.as_str().into());
            state
                .workspace_snapshot
                .as_mut()
                .unwrap()
                .session_run_phases
                .insert(
                    invocation.source_session_id,
                    crate::SessionRunPhase::Stopped,
                );
        }
        // Preserve exact host payload across failed write, duplicate event and retry.
        // 在失败写入、重复事件与重试间保留精确宿主载荷。
        let mailbox = vulcan_session_boundary::SubagentMailboxEvent {
            parent_thread_id: invocation.source_session_id.into(),
            parent_turn_id: None,
            child_thread_id: child_id.into(),
            kind: vulcan_session_boundary::SubagentMailboxEventKind::Failed,
            timestamp_ms: 7,
            payload: json!({"status":"failed", "execution_id":execution.as_str(), "error_text":"original failure"}),
        };
        // The immutable event uses the same metadata contract as production.
        // 不可变事件使用与生产相同的元数据契约。
        let event = RuntimeEvent::new(RuntimeEventKind::DelegateFinished, "Failed", "original failure").with_metadata(Some(json!({"kind":"subagent_task", "payload":{"subagent_task_id":child_id.to_string(), "execution_id":execution.as_str(), "subagent_mailbox_event":mailbox}})));
        // Explicit startup control and ordinary terminal delivery share the retry scheduler.
        // 明确启动控制与普通终态投递共用重试调度器。
        let terminal = if startup {
            DetachedTaskRuntimeEvent::StartupAborted {
                parent_session_id: invocation.source_session_id,
                event,
            }
        } else {
            DetachedTaskRuntimeEvent::ParentRuntime {
                parent_session_id: invocation.source_session_id,
                event,
            }
        };
        // A foreign nested start must be rejected even while the terminal write remains unavailable.
        // 终态写入仍不可用时，也必须拒绝外来嵌套启动。
        let (nested, rejected) = attach_test_runtime_event_projection_barrier(RuntimeEvent::new(
            RuntimeEventKind::DelegateStarted,
            "Nested",
            "wrong parent",
        ))
        .unwrap();
        // EOF immediately follows ordinary delivery and duplicate feedback.
        // 普通投递与重复反馈之后立即结束事件流。
        let stream = futures_util::stream::iter(vec![
            terminal.clone(),
            DetachedTaskRuntimeEvent::ChildExecution(ExecutionStreamEvent::RuntimeEvent {
                session_id: invocation.source_session_id,
                event: nested,
            }),
            terminal,
        ]);
        // Use real sinks and retained service ownership, with no live Provider.
        // 使用真实接收器与保留服务归属，不使用实时 Provider。
        let mut sink = ProviderV1StreamSink::new(
            child.clone(),
            invocation
                .state
                .lock()
                .unwrap()
                .project_resource_catalog_registry
                .clone(),
        );
        // Telemetry owns the same scope as the production stream listener.
        // 遥测与生产流监听器持有相同作用域。
        let mut telemetry =
            ProviderTelemetryHookGuard::new_with_persistence(invocation.state.clone(), None);
        // Empty emission destinations preserve production state mutations without external effects.
        // 空发射目标保留生产状态修改，不产生外部影响。
        let emitter = vulcan_service_api::ServiceEventEmitter::new(vec![]);
        // Retry and storage repair progress concurrently in the same deterministic test runtime.
        // 重试与存储修复在同一确定性测试运行时并发推进。
        let recovery = retry::drive(
            &emitter,
            &invocation.state,
            &invocation.controller_runtime,
            &runtime,
            invocation.source_session_id,
            child_id,
            execution.as_str(),
            &child,
            owner,
            stream,
            &mut telemetry,
            &mut sink,
        );
        // Only observed ordinary-event handling may repair storage; blocked stream processing would time out.
        // 仅已观测普通事件处理才能修复存储；流处理阻塞时会超时。
        let repair = async {
            assert!(
                tokio::time::timeout(std::time::Duration::from_secs(2), rejected)
                    .await
                    .unwrap()
                    .unwrap_err()
                    .contains("does not belong")
            );
            assert!(writer.failures.load(Ordering::SeqCst) > 0);
            assert!(
                runtime
                    .task_runtime
                    .take_subagent_events_for_parent(invocation.source_session_id.into())
                    .unwrap()
                    .is_empty()
            );
            writer.unavailable.store(false, Ordering::SeqCst);
        };
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            tokio::join!(recovery, repair);
        })
        .await
        .unwrap();
        // Exactly one original feedback is published after real disk repair and EOF.
        // 真实磁盘修复与流结束后，精确发布一次原反馈。
        assert_eq!(
            runtime
                .task_runtime
                .take_subagent_events_for_parent(invocation.source_session_id.into())
                .unwrap(),
            vec![mailbox]
        );
        assert!(
            !invocation
                .state
                .lock()
                .unwrap()
                .detached_subagent_task_listeners
                .contains_key(&child_id)
        );
        // Cold loading proves publication survived beyond in-memory retry state.
        // 冷加载证明发布不依赖内存重试状态而存续。
        let loaded = load_session_v1_full(&writer.locator).unwrap();
        assert!(
            loaded.document.collaboration_state().child_tasks[&execution]
                .non_success_result
                .is_some()
        );
        assert_eq!(
            loaded.document.memory_revision(),
            parent.read(|doc| doc.memory_revision()).unwrap()
        );
    }
}
