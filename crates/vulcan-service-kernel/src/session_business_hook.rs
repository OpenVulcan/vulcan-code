//! Durable admission of the main-session Hook before physical execution starts.
//! 在物理执行开始前可靠准入主会话 Hook。

use crate::session_runtime_registry_v1::{
    SessionFlushReason, SessionHandle, SessionMutationAdmission, SessionMutationChange,
};
use crate::{AgentRuntimeBridge, SessionId};

mod continuation;
pub(crate) use continuation::continuation_hook_authority;

/// Publishes `start`'s actual Hook before allowing its deferred user observer to run.
/// 允许延迟用户观察器运行之前，发布 `start` 的实际 Hook。
/// `handle` owns durable storage and `runtime` owns the prepared Hook; any publication failure leaves observers inert.
/// `handle` 持有可靠存储，`runtime` 持有已准备 Hook；发布失败时观察器保持不执行。
/// Returns only after publication and process-local observation admission; handler execution remains asynchronous.
/// 仅在发布及进程内观察准入后返回；处理器执行仍是异步的。
pub(crate) fn publish_prepared_business_hook(
    handle: &SessionHandle,
    runtime: &AgentRuntimeBridge,
    start: &vulcan_agent_runtime::workbench_prompt_turn::SharedPromptTurnStart,
) -> Result<(), String> {
    capture_business_hook_before_dispatch(
        handle,
        runtime,
        start.session_id,
        &start.preparation_turn_id,
    )?;
    if let Some(observation) = &start.deferred_user_event {
        observation.dispatch_after_publication(&start.preparation_turn_id)?;
    }
    Ok(())
}

/// Reads published Hook facts only for the current open business turn.
/// 仅读取当前开放业务回合已发布的 Hook 事实。
/// `handle` owns the canonical document; absent preparation or a closed turn returns no live owner.
/// `handle` 持有规范文档；准备缺失或回合已闭合时返回无活动所有者。
pub(crate) fn live_business_hook_state(
    handle: &SessionHandle,
) -> Result<Option<vulcan_session_core::v1::business_hook::BusinessHookMetadata>, String> {
    handle
        .read(|document| {
            if document.memory_revision() != document.durable_revision() {
                return Err(String::from(
                    "Business Hook projection requires published canonical facts.",
                ));
            }
            let Some(turn) = document.active_turn().filter(|turn| !turn.closed) else {
                return Ok(None);
            };
            let Some(binding) =
                document
                    .metadata
                    .business_hook
                    .binding
                    .as_ref()
                    .filter(|binding| {
                        binding.turn_id == turn.turn_id && binding.start_seq == turn.start_seq
                    })
            else {
                return Ok(None);
            };
            if binding.session_id != handle.session_id().to_string()
                || binding.segment_generation != document.segment_generation()
                || document
                    .metadata
                    .business_hook
                    .observed_inputs
                    .iter()
                    .any(|entry| entry.turn_id != turn.turn_id)
            {
                return Err(String::from(
                    "Business Hook projection has inconsistent canonical ownership.",
                ));
            }
            Ok(Some(document.metadata.business_hook.clone()))
        })
        .map_err(|error| error.to_string())?
}

/// Synchronizes canonical input history into an already live Hook before further preparation.
/// 在进一步准备前将规范输入历史同步到已活动 Hook。
/// `handle` and `runtime` identify the same session; no missing owner is fabricated and no event is dispatched.
/// `handle` 与 `runtime` 标识同一会话；不伪造缺失所有者，也不派发事件。
pub(crate) fn synchronize_live_business_hook_inputs(
    handle: &SessionHandle,
    runtime: &AgentRuntimeBridge,
) -> Result<(), String> {
    // Read storage authority before taking the Hook lock; this projection performs no disk writes.
    // 取得 Hook 锁前读取存储权威；本投影不执行磁盘写入。
    let Some(captured) = live_business_hook_state(handle)? else {
        return Ok(());
    };
    let mut hook = runtime
        .lock_conversation_hooks()
        .map_err(|error| format!("Failed to lock business Hook runtime: {error}"))?;
    if hook.has_active_turn(handle.session_id()) {
        hook.restore_business_hook_state(&captured)?;
    }
    Ok(())
}

/// Publishes the actual Hook seed against the currently admitted V1 business turn.
/// 对照当前准入的 V1 业务回合发布实际 Hook 种子。
/// `handle` owns canonical storage; `runtime`, `session_id` and `expected_hook_id` identify preparation.
/// `handle` 持有规范存储；`runtime`、`session_id` 和 `expected_hook_id` 标识准备过程。
/// Returns only after durable publication; no service or Hook lock may be held by the caller.
/// 仅在可靠发布后返回；调用方不得持有服务或 Hook 锁。
pub(crate) fn capture_business_hook_before_dispatch(
    handle: &SessionHandle,
    runtime: &AgentRuntimeBridge,
    session_id: SessionId,
    expected_hook_id: &str,
) -> Result<(), String> {
    // Read canonical coordinates without holding the Hook lock across storage or observers.
    // 读取规范坐标，避免跨存储或观察器操作持有 Hook 锁。
    let (generation, turn, captured) = handle
        .read(|document| {
            if document.metadata.session_id != session_id.to_string() {
                return Err(String::from(
                    "Business Hook admission has a foreign session owner.",
                ));
            }
            let turn = document
                .active_turn()
                .filter(|turn| !turn.closed)
                .cloned()
                .ok_or_else(|| {
                    String::from("Business Hook admission requires an open canonical turn.")
                })?;
            Ok((
                document.segment_generation(),
                turn,
                document.metadata.business_hook.binding.clone(),
            ))
        })
        .map_err(|error| error.to_string())??;
    // Capture the real owner; a continuation string must never be used as original user content.
    // 捕获实际所有者；绝不能把续跑字符串用作原始用户正文。
    let mut binding = runtime
        .lock_conversation_hooks()
        .map_err(|error| format!("Failed to lock business Hook runtime: {error}"))?
        .capture_business_hook_binding(session_id, generation, turn.turn_id, turn.start_seq)?;
    if binding.hook_turn_id != expected_hook_id {
        return Err(String::from(
            "Business Hook admission no longer owns the prepared correlation.",
        ));
    }
    if let Some(original) = captured.filter(|value| value.turn_id == turn.turn_id) {
        // Steering can change live readiness. Immutable admission retains its original readiness.
        // 引导可以改变活动就绪状态；不可变准入保留原始就绪状态。
        if original.session_id != binding.session_id
            || original.segment_generation != binding.segment_generation
            || original.start_seq != binding.start_seq
            || original.hook_turn_id != binding.hook_turn_id
            || original.user_content != binding.user_content
            || original.observes_user != binding.observes_user
        {
            return Err(String::from(
                "Business Hook admission conflicts with its original durable owner.",
            ));
        }
        binding = original;
    }
    // Core rechecks generation and turn under the publication lock; storage follows outside outer locks.
    // Core 在发布锁内重验代次及回合；存储在外层锁之外执行。
    let now_ms = crate::current_v1_unix_millis()?;
    handle
        .mutate_durable_if_changed(
            SessionMutationAdmission::MetadataOnly,
            SessionFlushReason::MetadataChanged,
            move |document| {
                if document.capture_business_hook_binding(binding, now_ms)? {
                    Ok(SessionMutationChange::Changed(()))
                } else {
                    Ok(SessionMutationChange::Unchanged(()))
                }
            },
        )
        .map_err(|error| format!("Failed to publish business Hook admission: {error}"))?;
    synchronize_live_business_hook_inputs(handle, runtime)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    //! Actual file publication tests for prepared business Hook ownership.
    //! 已准备业务 Hook 归属的真实文件发布测试。

    use super::*;
    use vulcan_agent_runtime::agent_runtime::conversation_hooks::ConversationHookRuntime;
    use vulcan_session_core::v1::{ClientActionId, ClientInputSubmission};
    use vulcan_workspace_store::workspace_store::{
        create_session_v1_storage_fixture, load_session_v1_full, load_session_v1_tail,
    };

    /// An actual observer sees the original Hook on disk, and failed publication cannot notify it.
    /// 实际观察器能够在磁盘看到原 Hook，发布失败则不能通知它。
    /// Uses the production shared reducer and service publication helper with an isolated file store.
    /// 使用生产共享归约器、服务发布帮助函数及隔离文件存储。
    #[test]
    fn business_hook_observer_runs_only_after_successful_publication() {
        use std::sync::{Arc, mpsc};
        use std::time::Duration;
        use vulcan_agent_runtime::agent_runtime::conversation_hooks::HOOK_CONVERSATION_USER_EVENT;
        use vulcan_agent_runtime::workbench_prompt_turn::{
            SharedPromptTurnBeginInput, begin_shared_prompt_turn,
        };
        use vulcan_hooks::{FnHookHandler, HookInvocation, HookMode, HookRegistration, HookResult};
        // Empty durable storage deliberately rejects the first attempt to publish business ownership.
        // 空可靠存储故意拒绝首次业务归属发布。
        let temporary = tempfile::tempdir().unwrap();
        let session_id = SessionId::from_u128(0x62d3);
        let locator =
            create_session_v1_storage_fixture(temporary.path(), &session_id.to_string(), 1)
                .unwrap();
        let handle = SessionHandle::new(
            session_id,
            locator.clone(),
            load_session_v1_tail(&locator).unwrap().document,
        );
        let runtime = crate::tests::test_support::test_agent_runtime_bridge();
        // The handler reads actual storage when invoked; a resident-only binding cannot satisfy it.
        // 处理器被调用时读取实际存储；仅驻留的绑定不能满足检查。
        let (sender, receiver) = mpsc::channel();
        let observer_locator = locator;
        runtime
            .conversation_hook_runtime
            .lock()
            .unwrap()
            .dispatcher_mut()
            .register(
                HookRegistration::new(
                    "durable-user-observer",
                    HOOK_CONVERSATION_USER_EVENT,
                    HookMode::Effect,
                ),
                Arc::new(FnHookHandler::new(move |invocation: HookInvocation| {
                    let sender = sender.clone();
                    let locator = observer_locator.clone();
                    async move {
                        let stored = load_session_v1_full(&locator).unwrap().document;
                        sender
                            .send((
                                stored.metadata.business_hook.binding,
                                invocation.payload.value,
                            ))
                            .unwrap();
                        Ok(HookResult::default())
                    }
                })),
            )
            .unwrap();
        // The real shared reducer must return before dispatching the observation.
        // 真实共享归约器必须在派发观察之前返回。
        let mut workspace =
            crate::tests::test_support::test_service_state_with_parent_session(session_id)
                .workspace_snapshot
                .clone()
                .unwrap();
        let start = begin_shared_prompt_turn(
            &mut workspace,
            &mut runtime.conversation_hook_runtime.lock().unwrap(),
            SharedPromptTurnBeginInput {
                session_id,
                session_title: "Original task",
                workspace_path: "D:/durable-observer-test",
                prompt_entry_id: "original-root".into(),
                continuation_action_id: None,
                prompt: "Original task",
                hidden_model_guidance: None,
                provider_visible_agent_messages: Vec::new(),
                attachments: Vec::new(),
                provider_attachments: Some(Vec::new()),
                existing_canonical_user_message_id: None,
                append_canonical_user_message: false,
                append_visible_user_prompt_record: false,
                source: vulcan_workbench_model::PendingSessionPromptSource::UserFollowup,
                vmm_runtime_ready: false,
                vmm_precheck_ready: false,
                execution_mode: "build".into(),
                reasoning_effort: None,
            },
        )
        .unwrap();
        assert!(matches!(
            receiver.recv_timeout(Duration::from_millis(50)),
            Err(mpsc::RecvTimeoutError::Timeout)
        ));
        assert!(publish_prepared_business_hook(&handle, &runtime, &start).is_err());
        assert!(matches!(
            receiver.recv_timeout(Duration::from_millis(50)),
            Err(mpsc::RecvTimeoutError::Timeout)
        ));
        // Once the canonical input exists, publication and the same prepared observation can be retried.
        // 规范输入存在后，可以重试发布及同一个已准备观察。
        handle
            .submit_root_input(
                ClientInputSubmission::new(
                    ClientActionId::try_new("original-root").unwrap(),
                    "Original task",
                    vec![],
                ),
                2,
            )
            .unwrap();
        publish_prepared_business_hook(&handle, &runtime, &start).unwrap();
        let revision = handle.read(|document| document.durable_revision()).unwrap();
        publish_prepared_business_hook(&handle, &runtime, &start).unwrap();
        let (binding, payload) = receiver.recv_timeout(Duration::from_secs(2)).unwrap();
        let binding = binding.unwrap();
        assert_eq!(binding.hook_turn_id, start.preparation_turn_id);
        assert_eq!(payload["turn_id"], binding.hook_turn_id);
        assert_eq!(payload["user_content"], binding.user_content);
        assert_eq!(
            handle.read(|document| document.durable_revision()).unwrap(),
            revision
        );
        assert!(matches!(
            receiver.recv_timeout(Duration::from_millis(50)),
            Err(mpsc::RecvTimeoutError::Timeout)
        ));
    }

    /// Saves the real prepared correlation before dispatch and proves cold identity and exact retry.
    /// 在分发前保存真实准备关联，证明冷身份及精确重试。
    #[tokio::test]
    async fn business_hook_publication_survives_cold_load_and_preserves_initial_readiness() {
        // Use the actual file backend so a resident-only seed cannot satisfy this assertion.
        // 使用真实文件后端，防止仅常驻种子满足断言。
        let temporary = tempfile::tempdir().unwrap();
        let session_id = SessionId::from_u128(0x62d1);
        let locator =
            create_session_v1_storage_fixture(temporary.path(), &session_id.to_string(), 1)
                .unwrap();
        let handle = SessionHandle::new(
            session_id,
            locator.clone(),
            load_session_v1_tail(&locator).unwrap().document,
        );
        handle
            .submit_root_input(
                ClientInputSubmission::new(
                    ClientActionId::try_new("root").unwrap(),
                    "Original task",
                    vec![],
                ),
                2,
            )
            .unwrap();
        // The production runtime supplies the immutable ID and sanitized original input.
        // 生产运行时提供不可变标识和清理后的原始输入。
        let runtime = crate::tests::test_support::test_agent_runtime_bridge();
        let prepared = runtime
            .conversation_hook_runtime
            .lock()
            .unwrap()
            .prepare_user_prompt_hook_requests(
                session_id,
                "root",
                None,
                "D:/business-hook-test".into(),
                vulcan_workbench_model::PendingSessionPromptSource::UserFollowup,
                vulcan_hooks::SystemPromptHookPhase::FirstSession,
                "Sanitized original task",
                &Default::default(),
                false,
            );
        capture_business_hook_before_dispatch(&handle, &runtime, session_id, &prepared.turn_id)
            .unwrap();
        let loaded = load_session_v1_full(&locator).unwrap();
        let binding = loaded
            .document
            .business_hook_recovery_binding()
            .unwrap()
            .clone();
        assert_eq!(binding.hook_turn_id, prepared.turn_id);
        assert_eq!(binding.user_content, "Sanitized original task");
        assert!(!binding.vmm_runtime_ready);
        let revision = loaded.document.memory_revision();
        // A later in-turn prompt changes live readiness without rewriting original admission facts.
        // 后续回合内提示改变活动就绪状态，不改写原始准入事实。
        runtime
            .conversation_hook_runtime
            .lock()
            .unwrap()
            .prepare_user_prompt_hook_requests(
                session_id,
                "steer",
                None,
                "D:/business-hook-test".into(),
                vulcan_workbench_model::PendingSessionPromptSource::UserFollowup,
                vulcan_hooks::SystemPromptHookPhase::SubsequentSession,
                "Later direction",
                &Default::default(),
                true,
            );
        capture_business_hook_before_dispatch(&handle, &runtime, session_id, &prepared.turn_id)
            .unwrap();
        assert_eq!(
            handle.read(|document| document.memory_revision()).unwrap(),
            revision
        );
        assert!(
            capture_business_hook_before_dispatch(&handle, &runtime, session_id, "wrong-hook")
                .is_err()
        );
        // Simulate a crash after canonical queue publication but before either runtime notification.
        // 模拟规范队列发布之后、两条运行时通知之前崩溃。
        for (action, content) in [("queue-first", " first "), ("queue-second", "second")] {
            handle
                .enqueue_input(
                    ClientInputSubmission::new(
                        ClientActionId::try_new(action).unwrap(),
                        content,
                        vec![],
                    ),
                    10,
                )
                .unwrap();
            if action == "queue-first" {
                // The direct input is durably accepted between queue admissions, with no Hook callback.
                // 直接输入在队列准入之间可靠接纳，没有 Hook 回调。
                handle
                    .mutate_durable_if_changed(
                        SessionMutationAdmission::ExistingTurn,
                        SessionFlushReason::InputAccepted,
                        |document| {
                            document
                                .add_pending_in_turn_input(
                                    ClientInputSubmission::new(
                                        ClientActionId::try_new("direct-between").unwrap(),
                                        "@inject 2 rule\nDirect direction",
                                        vec![],
                                    ),
                                    10,
                                )
                                .map(|result| {
                                    if result.changed() {
                                        SessionMutationChange::Changed(result.into_value())
                                    } else {
                                        SessionMutationChange::Unchanged(result.into_value())
                                    }
                                })
                        },
                    )
                    .unwrap();
            }
        }
        let reloaded = load_session_v1_full(&locator).unwrap();
        let recovery = reloaded.document.business_hook_recovery_state().unwrap();
        assert_eq!(recovery.observed_inputs.len(), 3);
        let queued_revision = reloaded.document.memory_revision();
        // Deliver callbacks in reverse order; production notification must project canonical order first.
        // 以相反顺序交付回调；生产通知必须先投影规范顺序。
        let mut notification_state =
            crate::tests::test_support::test_service_state_with_parent_session(session_id);
        for (action, content) in [
            ("queue-second", "second"),
            ("direct-between", "@inject 2 rule\nDirect direction"),
            ("queue-first", " first "),
        ] {
            let event = crate::pending_prompts::enqueue_webui_pending_prompt_from_submission(
                &mut notification_state,
                &runtime,
                session_id,
                "D:/business-hook-test",
                action,
                content.into(),
                None,
                Vec::new(),
                None,
                None,
            )
            .unwrap();
            crate::pending_prompts::dispatch_queued_pending_prompt_hook(&handle, &runtime, event)
                .unwrap();
        }
        assert_eq!(
            handle.read(|document| document.memory_revision()).unwrap(),
            queued_revision
        );
        // Install into a distinct process-local owner using only validated durable binding and queue history.
        // 仅使用已验证可靠绑定与队列历史，安装到独立进程内所有者。
        let mut cold_hook = ConversationHookRuntime::new();
        assert!(cold_hook.restore_business_hook_state(&recovery).unwrap());
        assert_eq!(
            cold_hook.active_turn_id(session_id),
            Some(prepared.turn_id.as_str())
        );
        let queued = cold_hook.prepare_user_prompt_hook_requests(
            session_id,
            "queue-first",
            None,
            "D:/business-hook-test".into(),
            vulcan_workbench_model::PendingSessionPromptSource::UserFollowup,
            vulcan_hooks::SystemPromptHookPhase::SubsequentSession,
            "first",
            &Default::default(),
            false,
        );
        assert_eq!(
            queued.precheck.skip().report.unwrap().payload.value["was_recorded_while_queued"],
            true
        );
        // The actual completion consumer sees the cold-restored direct input at its original admission position.
        // 实际完成消费方在原准入位置看到冷恢复的直接输入。
        let (_, completion) = cold_hook.prepare_turn_completion_dispatch(
            session_id,
            None,
            "done",
            1,
            &Default::default(),
        );
        let report = completion.run().await.report.unwrap();
        let timeline = report.payload.value["timeline"].as_array().unwrap();
        assert_eq!(timeline.len(), 3);
        assert_eq!(timeline[0]["source_id"], "queue-first");
        assert_eq!(timeline[1]["source_id"], "direct-between");
        assert_eq!(timeline[1]["kind"], "user_interrupt");
        assert_eq!(timeline[1]["content"], "Direct direction");
        assert_eq!(timeline[2]["source_id"], "queue-second");
        assert_eq!(
            load_session_v1_full(&locator)
                .unwrap()
                .document
                .memory_revision(),
            queued_revision
        );
    }
}
