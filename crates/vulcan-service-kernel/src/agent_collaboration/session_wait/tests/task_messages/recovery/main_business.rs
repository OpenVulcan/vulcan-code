//! Actual startup selection of open main business with no collaboration outbox.
//! 没有协作出站待办时对开放主业务的实际启动筛选。

use super::*;

/// Cold root and queued sources without a published Hook execute their first preparation exactly once.
/// 没有已发布 Hook 的冷根及队列来源各执行一次首次准备。
/// Uses the production worker stack and local missing-provider failure, preserving canonical history.
/// 使用生产工作线程栈和本地缺供应商失败，保留规范历史。
#[test]
fn initial_human_business_startup_preserves_root_and_queued_inputs() {
    // Both cases require the real preparation future's production stack size.
    // 两种场景都要求真实准备 Future 的生产栈大小。
    let executor = crate::service_bridge_io::webui_service_runtime_builder()
        .worker_threads(2)
        .build()
        .unwrap();
    for queued in [false, true] {
        executor
            .block_on(executor.spawn(cold_main_business_scenario(false, queued)))
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

/// Uses the production service stack for the real cold hydration and asynchronous preparation path.
/// 使用生产服务栈执行真实冷水合及异步准备路径。
/// No provider is configured; preparation must fail locally after restoring original ownership.
/// 未配置供应商；准备必须在恢复原归属后于本地失败。
#[test]
fn main_business_recovery_startup_preserves_original_task() {
    let executor = crate::service_bridge_io::webui_service_runtime_builder()
        .worker_threads(2)
        .build()
        .unwrap();
    executor
        .block_on(executor.spawn(cold_main_business_scenario(true, false)))
        .unwrap();
}

/// Recovers published ownership or first preparation; `queued` selects the authentic scheduled source.
/// 恢复已发布归属或首次准备；`queued` 选择真实已调度来源。
/// Returns nothing and keeps all writes inside the isolated host fixture.
/// 无返回值，全部写入限制在隔离宿主夹具内。
async fn cold_main_business_scenario(published: bool, queued: bool) {
    let (root, invocation, target) = fixture(false).await;
    vulcan_workspace_store::data_plane::prepare_application_schema_for_test(
        &invocation.controller_runtime,
    )
    .await
    .unwrap();
    let project_path = root.data_root().join("main-recovery-project");
    std::fs::create_dir_all(&project_path).unwrap();
    let project = vulcan_workbench_model::TaskProjectSummary::new(
        target.storage_locator().unwrap().project_id(),
        "Main recovery project",
        project_path.display().to_string(),
        false,
        0,
        None,
        None,
        Vec::new(),
    );
    vulcan_workspace_store::workspace_store::persist_new_project(
        invocation.controller_runtime.clone(),
        &project,
    )
    .await
    .unwrap();
    target.initialize_root_usage(true, 1);
    // Select only real root or queued admission; both retain their original canonical input.
    // 只选择真实根或队列准入；两者都保留原规范输入。
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
    let runtime = crate::tests::test_support::test_agent_runtime_bridge();
    let prepared = runtime
        .conversation_hook_runtime
        .lock()
        .unwrap()
        .prepare_user_prompt_hook_requests(
            target.session_id(),
            "original-main",
            None,
            project_path.display().to_string(),
            vulcan_workbench_model::PendingSessionPromptSource::UserFollowup,
            vulcan_hooks::SystemPromptHookPhase::FirstSession,
            "Original main task",
            &Default::default(),
            false,
        );
    if published {
        crate::session_business_hook::capture_business_hook_before_dispatch(
            &target,
            &runtime,
            target.session_id(),
            &prepared.turn_id,
        )
        .unwrap();
    }
    let old_events = target.read(|document| document.events().to_vec()).unwrap();
    let registry = Arc::new(SessionRuntimeRegistry::default());
    let cold_runtime = crate::tests::test_support::test_agent_runtime_bridge();
    // Count actual precheck execution, not just the plan's selected source flag.
    // 统计实际预检执行，而不只检查计划选择的来源标记。
    let precheck_calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    // The isolated observer has no external effect and remains available when VMM is disabled.
    // 隔离观察器没有外部效果，且在 VMM 禁用时仍然可用。
    let precheck_counter = precheck_calls.clone();
    cold_runtime.conversation_hook_runtime.lock().unwrap().dispatcher_mut().register(
        vulcan_hooks::HookRegistration::new(
            "initial-human-precheck",
            vulcan_agent_runtime::agent_runtime::conversation_hooks::HOOK_CONVERSATION_TURN_PRECHECK,
            vulcan_hooks::HookMode::Effect,
        ),
        Arc::new(vulcan_hooks::FnHookHandler::new(move |_invocation: vulcan_hooks::HookInvocation| {
            let counter = precheck_counter.clone();
            async move {
                counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                Ok(vulcan_hooks::HookResult::default())
            }
        })),
    ).unwrap();
    {
        let mut state = invocation.state.lock().unwrap();
        state.session_runtime_registry = registry.clone();
        state.agent_runtime = Some(cold_runtime.clone());
    }
    // A concurrent submitter must defer recovery without falsely completing the startup worker's duty.
    // 并发提交者必须使恢复延后，不能错误完成启动任务义务。
    let admission =
        crate::project_conversation_services::try_begin_pending_prompt_dispatch_exclusion(
            &invocation.state,
            target.session_id(),
        )
        .unwrap()
        .unwrap();
    let deferred = startup_recovery::recover_indexed_collaboration(
        &crate::ServiceEventEmitter::new(vec![]),
        &invocation.state,
        [target.session_id()].into_iter().collect(),
    )
    .await
    .unwrap_err();
    assert!(
        deferred.contains("Main business recovery remains pending behind preparation admission.")
    );
    assert!(
        !cold_runtime
            .conversation_hook_runtime
            .lock()
            .unwrap()
            .has_active_turn(target.session_id())
    );
    drop(admission);
    // Simulate a process-local preparation that never crossed the durable publication boundary.
    // 模拟从未跨越可靠发布边界的进程内准备。
    let unpublished_id = if !published {
        Some(
            cold_runtime
                .conversation_hook_runtime
                .lock()
                .unwrap()
                .prepare_user_prompt_hook_requests(
                    target.session_id(),
                    "unpublished",
                    None,
                    project_path.display().to_string(),
                    vulcan_workbench_model::PendingSessionPromptSource::UserFollowup,
                    vulcan_hooks::SystemPromptHookPhase::FirstSession,
                    "Unpublished draft",
                    &Default::default(),
                    false,
                )
                .turn_id,
        )
    } else {
        None
    };

    startup_recovery::recover_indexed_collaboration(
        &crate::ServiceEventEmitter::new(vec![]),
        &invocation.state,
        [target.session_id()].into_iter().collect(),
    )
    .await
    .unwrap();
    let recovered = registry.get(target.session_id()).unwrap();
    assert!(!Arc::ptr_eq(&recovered, &target));
    // Published recovery preserves identity; first preparation publishes only the original human source.
    // 已发布恢复保留身份；首次准备只发布原人类来源。
    let captured = recovered
        .read(|document| document.metadata.business_hook.binding.clone().unwrap())
        .unwrap();
    assert_eq!(captured.turn_id, turn_id);
    assert_eq!(captured.user_content, "Original main task");
    if published {
        assert_eq!(captured.hook_turn_id, prepared.turn_id);
    } else {
        assert_ne!(Some(captured.hook_turn_id), unpublished_id);
    }
    // With no provider, the ordinary preparation error must settle only this original turn.
    // 无供应商时，常规准备错误必须只结算此原回合。
    tokio::time::timeout(std::time::Duration::from_secs(20), async {
        loop {
            if recovered
                .read(|document| document.active_turn().is_none_or(|turn| turn.closed))
                .unwrap()
            {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("missing provider must settle original recovered preparation");
    assert_eq!(
        precheck_calls.load(std::sync::atomic::Ordering::SeqCst),
        usize::from(!published)
    );
    // Pin the canonical writer while reading disk; a closed in-memory turn may still be publishing.
    // 读磁盘时固定规范写入器；内存中的已闭合回合仍可能正在发布。
    let read_handle = recovered.clone();
    // Full durable loading belongs on the blocking executor and must propagate storage failures.
    // 完整可靠加载归阻塞执行器处理，必须传播存储失败。
    let stored =
        crate::spawn_service_blocking_task(move || read_handle.read_full_durable(Clone::clone))
            .await
            .unwrap()
            .unwrap();
    assert_eq!(&stored.events()[..old_events.len()], old_events.as_slice());
    assert_eq!(
        stored
            .events()
            .iter()
            .filter(|event| matches!(event.event, EventBody::UserInput(_)))
            .count(),
        1
    );
    assert!(
        stored
            .events()
            .iter()
            .skip(old_events.len())
            .filter_map(|event| event.turn_id)
            .all(|id| id == turn_id)
    );
    let state = invocation.state.lock().unwrap();
    assert_eq!(
        state.workspace_snapshot.as_ref().unwrap().selected_session,
        invocation.source_session_id
    );
    assert!(!state.active_stream_tasks.contains_key(&target.session_id()));
}
