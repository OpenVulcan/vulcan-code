//! Ordinary historical tasks require user consent independently of collaboration startup repair.
//! 普通历史任务需要用户许可，不能由协作启动修复自行执行。

use super::*;

/// Cold root and promoted queue sources remain idle until a new explicit user action.
/// 冷根输入与已提升队列来源保持空闲，直到新的显式用户操作。
/// Uses isolated storage and the production worker stack; returns no value.
/// 使用隔离存储与生产工作线程栈；无返回值。
#[test]
fn initial_human_business_startup_does_not_dispatch_root_or_queued_inputs() {
    // Use the production worker stack to expose unintended preparation.
    // 使用生产工作线程栈暴露意外准备行为。
    let executor = crate::service_bridge_io::webui_service_runtime_builder()
        .worker_threads(2)
        .build()
        .unwrap();
    for queued in [false, true] {
        executor
            .block_on(executor.spawn(cold_main_business_scenario(false, queued, None)))
            .unwrap();
    }
}

/// A queued successor distinguishes auxiliary occupancy from actual business acknowledgement.
/// 排队后继区分辅助占用与真实业务确认。
/// Exercises a real Goal replacement, active and interrupted APIs, and busy preparation without a provider.
/// 覆盖真实目标替换、活动及中断 API 和繁忙准备，不调用供应商。
#[tokio::test]
async fn main_business_queued_dispatch_rejects_auxiliary_acknowledgement() {
    use vulcan_session_core::v1::document::EnqueueInputDisposition;
    for purpose in [
        ApiPurposeKind::Compaction,
        ApiPurposeKind::QuestionExecution,
        ApiPurposeKind::MainGeneration,
    ] {
        // All canonical files and the registered target belong to this isolated fixture.
        // 全部规范文件与已登记目标归此隔离夹具持有。
        let (_root, invocation, target) = fixture(false).await;
        target
            .mutate_durable(
                SessionMutationAdmission::MetadataOnly,
                SessionFlushReason::MetadataChanged,
                |document| document.create_goal("Queued goal", None, "Continue work", 2),
            )
            .unwrap();
        // The normal enqueue transition replaces the undispatched Goal with a scheduled user root.
        // 正常排队转换以已调度用户根替代尚未分发目标。
        let queued = target
            .enqueue_input(
                ClientInputSubmission::new(
                    ClientActionId::try_new("queued-direction").unwrap(),
                    "Queued work",
                    vec![],
                ),
                3,
            )
            .unwrap()
            .value;
        // Read the source coordinates from the domain receipt, without inventing events.
        // 从领域回执读取来源坐标，不臆造事件。
        let EnqueueInputDisposition::SupersededGoalAndScheduled {
            scheduled_turn_id,
            input_seq,
            ..
        } = queued.disposition
        else {
            panic!("expected canonical queued successor");
        };
        // This retained authority remains the same while physical execution changes.
        // 物理执行变化时，此保留权威保持不变。
        let authority = crate::webui_prompt_lifecycle::queued_dispatch_authority_from_v1_handle(
            &target,
            scheduled_turn_id,
            queued.input_id,
            &queued.client_action_id,
            input_seq,
        )
        .unwrap();
        start_api_with_purpose(&target, purpose, 4);
        // Hold normal preparation admission so the terminal case cannot accidentally call a provider.
        // 持有正常准备准入，使终态场景不能意外调用供应商。
        let _admission =
            crate::project_conversation_services::try_begin_pending_prompt_dispatch_exclusion(
                &invocation.state,
                target.session_id(),
            )
            .unwrap()
            .unwrap();
        // No source Hook should be created merely to classify request history.
        // 仅分类请求历史不应创建来源 Hook。
        let runtime = crate::tests::test_support::test_agent_runtime_bridge();
        for active in [true, false] {
            if !active {
                target
                    .mutate_durable(
                        SessionMutationAdmission::ExistingTurn,
                        SessionFlushReason::ApiCompleted,
                        |document| document.interrupt_active_api_for_process(5),
                    )
                    .unwrap();
            }
            // Both active and terminal checks must preserve complete canonical history.
            // 活动与终态检查都必须保留完整规范历史。
            let before = target
                .read(|document| (document.metadata.clone(), document.events().to_vec()))
                .unwrap();
            assert_eq!(
                crate::webui_turn_chaining::start_exact_v1_queued_turn(
                    &crate::ServiceEventEmitter::new(vec![]),
                    &invocation.state,
                    &invocation.controller_runtime,
                    &runtime,
                    target.session_id(),
                    authority.clone(),
                )
                .await
                .unwrap(),
                purpose == ApiPurposeKind::MainGeneration
            );
            assert_eq!(
                target
                    .read(|document| (document.metadata.clone(), document.events().to_vec()))
                    .unwrap(),
                before
            );
        }
        assert!(
            !runtime
                .conversation_hook_runtime
                .lock()
                .unwrap()
                .has_active_turn(target.session_id())
        );
    }
}

/// Promoted direction remains unacknowledged while a later auxiliary API occupies its turn.
/// 后续辅助 API 占用回合时，已提升方向仍保持未确认。
/// Uses a real API terminal receipt and the production continuation entry; no provider is contacted.
/// 使用真实 API 终态回执及生产续跑入口；不联系供应商。
#[tokio::test]
async fn main_business_pending_dispatch_rejects_auxiliary_acknowledgement() {
    use vulcan_session_core::v1::config::QueuedInputIdentity;
    use vulcan_session_core::v1::document::ApiCompletion;
    use vulcan_session_core::v1::event::{
        ApiCallStatus, ApiEndReason, ApiTimings, ContinuationEligibility,
    };
    use vulcan_session_core::v1::stats::UsageSnapshot;
    for purpose in [
        ApiPurposeKind::Compaction,
        ApiPurposeKind::QuestionExecution,
        ApiPurposeKind::RetryGeneration,
    ] {
        // Real indexed storage supplies the unique target handle used by dispatch.
        // 真实索引存储提供分发使用的唯一目标句柄。
        let (_root, invocation, target) = fixture(false).await;
        target
            .submit_root_input(
                ClientInputSubmission::new(
                    ClientActionId::try_new("pending-root").unwrap(),
                    "Original work",
                    vec![],
                ),
                2,
            )
            .unwrap();
        // The first business request precedes the direction and must not acknowledge it.
        // 首个业务请求早于方向，不能确认该方向。
        let original_api = start_api_with_purpose(&target, ApiPurposeKind::MainGeneration, 3);
        // Queue admission and promotion preserve the actual input identity in the terminal receipt.
        // 排队准入及提升在终态回执中保留实际输入身份。
        let queued = target
            .enqueue_input(
                ClientInputSubmission::new(
                    ClientActionId::try_new("pending-direction").unwrap(),
                    "New direction",
                    vec![],
                ),
                4,
            )
            .unwrap()
            .value;
        target
            .promote_queued_input_to_pending_turn(
                QueuedInputIdentity {
                    client_action_id: queued.client_action_id,
                    input_id: queued.input_id,
                },
                5,
            )
            .unwrap();
        // Complete through the real durable writer, which allocates the promoted input positions.
        // 通过真实可靠写入器完成，由其分配已提升输入位置。
        let receipt = target
            .mutate_durable(
                SessionMutationAdmission::ExistingTurn,
                SessionFlushReason::ApiCompleted,
                |document| {
                    document.complete_api_group(
                        original_api,
                        ApiCompletion {
                            request: ApiRequestSnapshot::new("openai", "model"),
                            usage: UsageSnapshot::default(),
                            status: ApiCallStatus::Completed,
                            end_reason: Some(ApiEndReason::CompletedSuccessfully),
                            error: None,
                            continuation_eligibility: ContinuationEligibility::Forbidden,
                            timing: ApiTimings::default(),
                            provider_source: None,
                            completed_at_ms: 6,
                        },
                    )
                },
            )
            .unwrap()
            .value;
        assert_eq!(receipt.promoted_input_seqs.len(), 1);
        start_api_with_purpose(&target, purpose, 7);
        // No advisory acknowledgement may append history, release physical ownership or create a Hook.
        // 提示性确认不能追加历史、释放物理归属或创建 Hook。
        let before = target
            .read(|document| (document.metadata.clone(), document.events().to_vec()))
            .unwrap();
        // A separate runtime makes unintended Hook creation observable.
        // 独立运行时让意外 Hook 创建可被观察。
        let runtime = crate::tests::test_support::test_agent_runtime_bridge();
        assert_eq!(
            crate::webui_turn_chaining::start_exact_v1_pending_in_turn(
                &crate::ServiceEventEmitter::new(vec![]),
                &invocation.state,
                &invocation.controller_runtime,
                &runtime,
                target.session_id(),
                receipt,
            )
            .await
            .unwrap(),
            purpose == ApiPurposeKind::RetryGeneration
        );
        assert_eq!(
            target
                .read(|document| (document.metadata.clone(), document.events().to_vec()))
                .unwrap(),
            before
        );
        assert!(
            !runtime
                .conversation_hook_runtime
                .lock()
                .unwrap()
                .has_active_turn(target.session_id())
        );
    }
}

/// The real root entry never acknowledges an auxiliary physical request as business dispatch.
/// 真实根入口绝不把辅助物理请求确认为业务分发。
/// Uses registered durable sessions and returns without provider I/O or history changes.
/// 使用已登记可靠会话，不执行供应商 I/O，也不改变历史。
#[tokio::test]
async fn main_business_root_dispatch_rejects_auxiliary_acknowledgement() {
    for purpose in [
        ApiPurposeKind::Compaction,
        ApiPurposeKind::QuestionExecution,
        ApiPurposeKind::MainGeneration,
    ] {
        // Keep this scenario's actual indexed storage and registry alive through the entry call.
        // 在入口调用期间保持本场景真实索引存储和注册表存活。
        let (_root, invocation, target) = fixture(false).await;
        target
            .submit_root_input(
                ClientInputSubmission::new(
                    ClientActionId::try_new("original-root").unwrap(),
                    "Original work",
                    vec![],
                ),
                2,
            )
            .unwrap();
        // The source position is allocated by root admission, never inferred from display ordering.
        // 来源位置由根准入分配，绝不从显示顺序推断。
        let input_seq = target
            .read(|document| document.active_turn().unwrap().start_seq)
            .unwrap();
        start_api_with_purpose(&target, purpose, 3);
        // Preserve every published event and metadata field across this advisory entry check.
        // 此提示性入口检查前后保留每条已发布事件及元数据字段。
        let before = target
            .read(|document| (document.metadata.clone(), document.events().to_vec()))
            .unwrap();
        // The runtime is never dispatched while the exact physical API owns the slot.
        // 精确物理 API 占用执行位置期间，运行时不会被分发。
        let runtime = crate::tests::test_support::test_agent_runtime_bridge();
        assert_eq!(
            crate::webui_turn_chaining::start_exact_v1_user_root_turn(
                &crate::ServiceEventEmitter::new(vec![]),
                &invocation.state,
                &invocation.controller_runtime,
                &runtime,
                target.session_id(),
                input_seq,
            )
            .await
            .unwrap(),
            purpose == ApiPurposeKind::MainGeneration
        );
        assert_eq!(
            target
                .read(|document| (document.metadata.clone(), document.events().to_vec()))
                .unwrap(),
            before
        );
        assert!(
            !runtime
                .conversation_hook_runtime
                .lock()
                .unwrap()
                .has_active_turn(target.session_id())
        );
    }
}

/// Published ordinary work must not restart merely because its historical turn remains open.
/// 已发布普通任务不能仅因历史回合仍开放而重新执行。
/// Covers failed and interrupted requests, explicit continuation and new input without a provider.
/// 覆盖失败、中断请求、显式继续与新输入，不调用供应商。
#[test]
fn main_business_startup_does_not_resume_failed_or_interrupted_tasks() {
    // Cover absent, forbidden, retryable and physically interrupted API histories.
    // 覆盖尚无 API、禁止继续、允许继续以及物理中断的 API 历史。
    let executor = crate::service_bridge_io::webui_service_runtime_builder()
        .worker_threads(2)
        .build()
        .unwrap();
    for terminal in [
        None,
        Some(vulcan_session_core::v1::event::ApiEndReason::ProviderError),
        Some(vulcan_session_core::v1::event::ApiEndReason::NetworkError),
        Some(vulcan_session_core::v1::event::ApiEndReason::ProcessInterrupted),
    ] {
        executor
            .block_on(executor.spawn(cold_main_business_scenario(true, false, terminal)))
            .unwrap();
    }
}

/// `published` controls Hook capture, `queued` selects user queue promotion, and `terminal` selects API history.
/// `published` 控制 Hook 捕获，`queued` 选择用户队列提升，`terminal` 选择 API 历史。
/// Returns nothing and keeps all writes inside the isolated host fixture.
/// 无返回值，全部写入限制在隔离宿主夹具内。
async fn cold_main_business_scenario(
    published: bool,
    queued: bool,
    terminal: Option<vulcan_session_core::v1::event::ApiEndReason>,
) {
    use vulcan_session_core::v1::document::{ApiCompletion, ContinuationRequest};
    use vulcan_session_core::v1::event::{
        ApiCallStatus, ApiEndReason, ApiErrorCategory, ApiTimings, ContinuationEligibility,
        RedactedApiError,
    };
    use vulcan_session_core::v1::stats::UsageSnapshot;
    // Isolated indexed storage and a fresh runtime model service restart without a provider.
    // 隔离索引存储与全新运行时模拟服务重启，不调用供应商。
    let (_root, invocation, target) = fixture(false).await;
    // Retain the authentic root or promoted queue identity across restart.
    // 在重启前后保留真实根输入或已提升队列的身份。
    let submission = ClientInputSubmission::new(
        ClientActionId::try_new("original-main").unwrap(),
        "Original main task",
        vec![],
    );
    let turn_id = if queued {
        target
            .mutate_durable(
                SessionMutationAdmission::MetadataOnly,
                SessionFlushReason::MetadataChanged,
                |document| document.create_goal("Cold goal", None, "Continue", 2),
            )
            .unwrap();
        target.enqueue_input(submission, 3).unwrap();
        target
            .read(|document| document.active_turn().unwrap().turn_id)
            .unwrap()
    } else {
        target.submit_root_input(submission, 2).unwrap().value
    };
    // Published Hook ownership is history, not permission for a new dispatch.
    // 已发布 Hook 归属属于历史，不代表新的派发许可。
    let original_runtime = crate::tests::test_support::test_agent_runtime_bridge();
    if published {
        // Publish through the real Hook admission entry.
        // 通过真实 Hook 准入入口发布。
        let prepared = original_runtime
            .conversation_hook_runtime
            .lock()
            .unwrap()
            .prepare_user_prompt_hook_requests(
                target.session_id(),
                "original-main",
                None,
                "D:/startup-consent-test".into(),
                vulcan_workbench_model::PendingSessionPromptSource::UserFollowup,
                vulcan_hooks::SystemPromptHookPhase::FirstSession,
                "Original main task",
                &Default::default(),
                false,
            );
        crate::session_business_hook::capture_business_hook_before_dispatch(
            &target,
            &original_runtime,
            target.session_id(),
            &prepared.turn_id,
        )
        .unwrap();
    }
    if let Some(reason) = terminal {
        // Keep the process-interrupted API open until production cold loading reconciles it.
        // 进程中断 API 保持开放，直到生产冷加载将其结算。
        let api = start_api_with_purpose(&target, ApiPurposeKind::MainGeneration, 4);
        if reason != ApiEndReason::ProcessInterrupted {
            target
                .mutate_durable(
                    SessionMutationAdmission::ExistingTurn,
                    SessionFlushReason::ApiCompleted,
                    |document| {
                        document.complete_api_group(
                            api,
                            ApiCompletion {
                                request: ApiRequestSnapshot::new("openai", "model"),
                                usage: UsageSnapshot::default(),
                                status: ApiCallStatus::Failed,
                                end_reason: Some(reason),
                                error: Some(RedactedApiError {
                                    category: if reason == ApiEndReason::ProviderError {
                                        ApiErrorCategory::Provider
                                    } else {
                                        ApiErrorCategory::Network
                                    },
                                    error_code: Some("provider_error".into()),
                                    message:
                                        "Usage update arrived after a different final observation."
                                            .into(),
                                }),
                                continuation_eligibility: if reason == ApiEndReason::ProviderError {
                                    ContinuationEligibility::Forbidden
                                } else {
                                    ContinuationEligibility::Allowed
                                },
                                timing: ApiTimings::default(),
                                provider_source: None,
                                completed_at_ms: 5,
                            },
                        )
                    },
                )
                .unwrap();
        }
    }
    // Capture durable contents before startup can inspect the session.
    // 在启动扫描检查会话之前捕获可靠内容。
    let before = target
        .read(|document| (document.metadata.clone(), document.events().to_vec()))
        .unwrap();
    // Restart discards all process-local owners.
    // 重启丢弃所有进程内所有者。
    let registry = Arc::new(SessionRuntimeRegistry::default());
    let cold_runtime = crate::tests::test_support::test_agent_runtime_bridge();
    {
        let mut state = invocation.state.lock().unwrap();
        state.session_runtime_registry = registry.clone();
        state.agent_runtime = Some(cold_runtime.clone());
    }
    startup_recovery::recover_indexed_collaboration(
        &crate::ServiceEventEmitter::new(vec![]),
        &invocation.state,
        [target.session_id()].into_iter().collect(),
    )
    .await
    .unwrap();
    assert!(
        registry.get(target.session_id()).is_err(),
        "ordinary history must remain cold during collaboration startup repair"
    );
    // Preserve history and forbidden-failure policy exactly.
    // 精确保留历史及禁止继续的失败策略。
    let stored = vulcan_workspace_store::workspace_store::load_session_v1_full(
        &target.storage_locator().unwrap(),
    )
    .unwrap()
    .document;
    assert_eq!(stored.metadata, before.0);
    assert_eq!(stored.events(), before.1.as_slice());
    // Viewing history may project interruption, but cannot dispatch an API.
    // 查看历史可以投影中断，但不能派发 API。
    let loaded = crate::session_v1_lifecycle::get_or_load_session_v1(
        &invocation.controller_runtime,
        &registry,
        target.session_id(),
    )
    .await
    .unwrap();
    assert!(!Arc::ptr_eq(&loaded, &target));
    let after_load = loaded
        .read(|document| (document.metadata.clone(), document.events().to_vec()))
        .unwrap();
    assert_eq!(
        after_load
            .1
            .iter()
            .filter(|event| matches!(event.event, EventBody::ApiStarted(_)))
            .count(),
        usize::from(terminal.is_some())
    );
    assert!(
        loaded
            .read(|document| document.active_api().is_none())
            .unwrap()
    );
    // Delayed startup repair must leave an already-opened conversation idle too.
    // 延迟启动修复也必须让已经打开的会话保持空闲。
    startup_recovery::recover_indexed_collaboration(
        &crate::ServiceEventEmitter::new(vec![]),
        &invocation.state,
        [target.session_id()].into_iter().collect(),
    )
    .await
    .unwrap();
    assert_eq!(
        loaded
            .read(|document| (document.metadata.clone(), document.events().to_vec(),))
            .unwrap(),
        after_load
    );
    assert!(
        !cold_runtime
            .conversation_hook_runtime
            .lock()
            .unwrap()
            .has_active_turn(target.session_id())
    );
    {
        let state = invocation.state.lock().unwrap();
        assert!(!state.active_stream_tasks.contains_key(&target.session_id()));
        assert!(!crate::webui_session_has_active_run(
            &state,
            target.session_id()
        ));
        assert!(!crate::webui_session_kernel_has_active_turn(
            &state,
            target.session_id()
        ));
    }
    // Only a subsequent user action may request execution, respecting the saved API policy.
    // 只有后续用户操作可请求执行，并遵守保存的 API 策略。
    if terminal.is_some() {
        let continuation = loaded.continue_turn(
            ContinuationRequest::new(
                ClientActionId::try_new("explicit-user-continue").unwrap(),
                "",
                Vec::new(),
            ),
            10,
        );
        if terminal == Some(ApiEndReason::ProviderError) {
            assert!(
                continuation.is_err(),
                "forbidden failure cannot become implicitly retryable"
            );
        } else {
            assert!(
                continuation.is_ok(),
                "explicit continuation must remain available: {continuation:?}"
            );
        }
    }
    // A new task can replace an inactive turn even when its final marker was never persisted.
    // 即便最终标记从未持久化，新任务也能替换没有执行者的旧回合。
    let _admission =
        crate::project_conversation_services::try_begin_pending_prompt_dispatch_exclusion(
            &invocation.state,
            target.session_id(),
        )
        .unwrap()
        .unwrap();
    let new_turn = loaded
        .submit_root_input_after_execution_ended(
            ClientInputSubmission::new(
                ClientActionId::try_new("new-user-task").unwrap(),
                "New task",
                vec![],
            ),
            11,
        )
        .unwrap()
        .value;
    assert_ne!(new_turn, turn_id);
}
