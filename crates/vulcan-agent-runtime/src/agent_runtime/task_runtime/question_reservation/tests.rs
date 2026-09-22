use super::*;

/// Closing a child cannot archive either auxiliary owner or mutate its original result before reliable release.
/// 关闭子任务不能在可靠释放前归档任一种辅助所有者或修改其原结果。
#[tokio::test]
async fn close_agent_requires_auxiliary_release_before_archival() {
    for maintenance in [false, true] {
        // The seeded original business remains completed while auxiliary work owns the child.
        // 辅助工作持有子任务时，播种的原业务保持已完成。
        let (runtime, parent, task) = seeded_child(TaskSessionStatus::Completed);
        // Actual public close selection reads this real registry entry.
        // 实际公开关闭选择读取此真实注册表条目。
        let mut entry = SubagentRegistryEntry::new(
            "question-worker",
            task.into(),
            parent.into(),
            SubagentKind::Clone,
            1,
        );
        entry.status = SubagentStatus::Completed;
        runtime
            .subagent_registry
            .write()
            .unwrap()
            .upsert(entry)
            .unwrap();
        // Both auxiliary purposes use the same exclusion checked by close_agent.
        // 两种辅助用途使用 close_agent 检查的同一互斥。
        let owner = if maintenance {
            runtime.try_reserve_child_context_maintenance(task, "retained-business-execution")
        } else {
            runtime.try_reserve_child_question_execution(task)
        }
        .unwrap()
        .unwrap();
        assert!(
            runtime
                .invoke_close_agent(parent, serde_json::json!({"target":"question-worker"}))
                .await
                .unwrap_err()
                .contains("auxiliary execution settles")
        );
        assert_eq!(
            runtime.sessions.read().unwrap()[&task].last_status,
            "completed"
        );
        assert_eq!(
            runtime.sessions.read().unwrap()[&task].last_result_text,
            "retained business result"
        );
        assert_eq!(
            runtime
                .subagent_registry_snapshot()
                .unwrap()
                .get_for_parent(parent.into(), "question-worker")
                .unwrap()
                .status,
            SubagentStatus::Completed
        );
        drop(owner);
        runtime
            .invoke_close_agent(parent, serde_json::json!({"target":"question-worker"}))
            .await
            .unwrap();
        assert_eq!(
            runtime
                .subagent_registry_snapshot()
                .unwrap()
                .get_for_parent(parent.into(), "question-worker")
                .unwrap()
                .status,
            SubagentStatus::Archived
        );
    }
}

/// Maintenance excludes both question execution and business preparation without reopening business state.
/// 维护排除答疑执行及业务准备，不重开业务状态。
/// Returns nothing; exercises both admission orders through production entry points.
/// 不返回值；通过生产入口验证两种准入顺序。
#[test]
fn maintenance_and_questions_share_business_exclusion() {
    for maintenance_first in [false, true] {
        // Retain the exact completed execution as the only maintenance candidate.
        // 保留精确已完成执行作为唯一维护候选。
        let (runtime, parent, task) = seeded_child(TaskSessionStatus::Completed);
        // Both auxiliary purposes must acquire the same transaction and reservation table.
        // 两种辅助用途必须获取同一事务及预留表。
        let guard = if maintenance_first {
            runtime.try_reserve_child_context_maintenance(task, "retained-business-execution")
        } else {
            runtime.try_reserve_child_question_execution(task)
        }
        .unwrap()
        .unwrap();
        assert_eq!(guard.task_id(), task);
        assert_eq!(
            guard.purpose(),
            if maintenance_first {
                ChildAuxiliaryExecutionPurpose::ContextMaintenance
            } else {
                ChildAuxiliaryExecutionPurpose::Question
            }
        );
        assert!(
            runtime
                .try_reserve_child_question_execution(task)
                .unwrap()
                .is_none()
        );
        assert!(
            runtime
                .try_reserve_child_context_maintenance(task, "retained-business-execution")
                .unwrap()
                .is_none()
        );
        assert!(
            prepare_business(&runtime, parent, task)
                .unwrap_err()
                .contains("handling auxiliary execution")
        );
        assert!(!runtime.is_task_running(task).unwrap());
        {
            // Auxiliary ownership never changes the original completed result or execution identity.
            // 辅助归属绝不改变原已完成结果或执行身份。
            let sessions = runtime.sessions.read().unwrap();
            // Read the exact owned record while the maintenance/question guard is still live.
            // 在维护或答疑守卫仍活动时读取精确所属记录。
            let record = sessions.get(&task).unwrap();
            assert_eq!(record.last_status, "completed");
            assert_eq!(record.last_result_text, "retained business result");
            assert_eq!(
                record.execution_id.as_deref(),
                Some("retained-business-execution")
            );
        }
        drop(guard);
        // Release permits new business, whose new execution identity invalidates the old maintenance job.
        // 释放允许新业务，其新执行身份使旧维护任务失效。
        let invocation = prepare_business(&runtime, parent, task).unwrap();
        assert_ne!(invocation.execution_id, "retained-business-execution");
        assert!(
            runtime
                .try_reserve_child_context_maintenance(task, "retained-business-execution")
                .err()
                .unwrap()
                .contains("superseded")
        );
    }
}

/// Maintenance requires known successful completion and cannot promote stopped, failed, archived or cold records.
/// 维护要求已知成功完成，不能提升停止、失败、归档或冷态记录。
/// Returns nothing; verifies rejection and occupied states leave auxiliary admission available.
/// 不返回值；验证拒绝及占用状态不遗留辅助准入。
#[test]
fn maintenance_requires_exact_completed_business() {
    for status in [
        TaskSessionStatus::Failed,
        TaskSessionStatus::Stopped,
        TaskSessionStatus::Archived,
    ] {
        // Each terminal classification retains the same exact execution for an independent rejection.
        // 每种终态分类为独立拒绝保留相同精确执行。
        let (runtime, _, task) = seeded_child(status);
        assert!(
            runtime
                .try_reserve_child_context_maintenance(task, "retained-business-execution")
                .is_err()
        );
        assert!(
            runtime
                .auxiliary_execution_reservations
                .read()
                .unwrap()
                .is_empty()
        );
    }
    for status in [TaskSessionStatus::Created, TaskSessionStatus::Running] {
        // A still-active resident record cannot be treated as post-completion work even before its lease starts.
        // 即使租约尚未启动，仍活动的常驻记录也不能被当作完成后工作。
        let (runtime, _, task) = seeded_child(status);
        assert!(
            runtime
                .try_reserve_child_context_maintenance(task, "retained-business-execution")
                .unwrap()
                .is_none()
        );
        assert!(
            runtime
                .auxiliary_execution_reservations
                .read()
                .unwrap()
                .is_empty()
        );
    }
    // A background job cannot manufacture a completed identity after its resident owner disappears.
    // 常驻所有者消失后，后台任务不能制造已完成身份。
    let runtime = Arc::new(TaskRuntime::new());
    assert!(
        runtime
            .try_reserve_child_context_maintenance(Uuid::new_v4(), "missing")
            .is_err()
    );
    assert!(
        runtime
            .auxiliary_execution_reservations
            .read()
            .unwrap()
            .is_empty()
    );
}

/// Seeds a child with the supplied status and a retained result; returns runtime, parent, and child IDs.
/// 按指定状态播种带保留结果的子任务；返回运行时、父标识及子标识。
fn seeded_child(status: TaskSessionStatus) -> (Arc<TaskRuntime>, SessionId, SessionId) {
    // Runtime and identities belong only to this test case.
    // 运行时和身份仅归属于本测试。
    let runtime = Arc::new(TaskRuntime::new());
    // Parent identity establishes the prepared request's actual owner.
    // 父身份建立准备请求的实际所有者。
    let parent_id = Uuid::new_v4();
    // Child identity is shared by business and auxiliary admission.
    // 子身份由业务与辅助准入共享。
    let task_id = Uuid::new_v4();
    // The retained record must survive auxiliary reservation unchanged.
    // 保留记录必须在辅助预留期间保持不变。
    let mut record = build_task_session_record(TaskSessionRecordInit {
        task_id,
        public_task_id: String::from("question-worker"),
        origin_session_id: parent_id,
        parent_task_id: None,
        depth: 1,
        description: None,
        subagent: None,
        invocation_config: TaskInvocationConfig::main("business prompt"),
        display_title: String::from("question-worker"),
        controller_config: None,
        snapshot: CoreConversationSnapshot::default(),
        last_prompt: String::from("business prompt"),
        last_status: status_name_for_task_session_status(status).to_string(),
    });
    record.last_result_text = String::from("retained business result");
    record.execution_id = Some(String::from("retained-business-execution"));
    runtime.sessions.write().unwrap().insert(task_id, record);
    (runtime, parent_id, task_id)
}

/// Builds a real resumed business request for the supplied runtime and parent/child IDs.
/// 根据给定运行时及父子标识构造真实业务续跑请求。
/// Returns the production preparation result, including auxiliary admission errors.
/// 返回生产准备结果，包括辅助准入错误。
fn prepare_business(
    runtime: &Arc<TaskRuntime>,
    parent_id: SessionId,
    task_id: SessionId,
) -> Result<TaskExecutionRequest, String> {
    prepare_task_execution_request(
        runtime,
        parent_id,
        ResolvedTaskSessionIdentifier {
            task_id,
            public_task_id: String::from("question-worker"),
            is_resumed: true,
        },
        parse_task_tool_arguments(json!({"task_id": "question-worker", "prompt": "new work"}))?,
    )
}

/// Verifies reservation excludes business preparation and preserves its terminal result.
/// 验证预留排除业务准备并保留其终态结果。
#[test]
fn child_question_reservation_preserves_business_and_releases_admission() {
    // Completed child provides the production resume target.
    // 已完成子任务提供生产续跑目标。
    let (runtime, parent_id, task_id) = seeded_child(TaskSessionStatus::Completed);
    // Guard owns auxiliary admission until dropped.
    // 守卫在释放前持有辅助准入。
    let guard = runtime
        .try_reserve_child_question_execution(task_id)
        .unwrap()
        .unwrap();
    assert!(!runtime.is_task_running(task_id).unwrap());
    assert!(
        runtime
            .try_reserve_child_question_execution(task_id)
            .unwrap()
            .is_none()
    );
    // Real preparation must reject before mutating any retained business field.
    // 真实准备必须在修改任何保留业务字段之前拒绝。
    let error = prepare_business(&runtime, parent_id, task_id)
        .err()
        .unwrap();
    assert!(error.contains("handling auxiliary execution"));
    {
        // Read the business authority while the reservation is still held.
        // 在预留仍然持有时读取业务权威记录。
        let sessions = runtime.sessions.read().unwrap();
        // Exact child record is retained throughout the auxiliary scope.
        // 精确子记录在整个辅助作用域内保持。
        let record = sessions.get(&task_id).unwrap();
        assert_eq!(record.last_status, "completed");
        assert_eq!(record.last_prompt, "business prompt");
        assert_eq!(record.last_result_text, "retained business result");
        assert_eq!(
            record.execution_id.as_deref(),
            Some("retained-business-execution")
        );
    }
    drop(guard);
    // Business preparation owns admission after auxiliary release, even before Provider launch.
    // 辅助释放后业务准备即持有准入，即使 Provider 尚未启动。
    let invocation = prepare_business(&runtime, parent_id, task_id).unwrap();
    assert!(
        runtime
            .try_reserve_child_question_execution(task_id)
            .unwrap()
            .is_none()
    );
    // Claim uses the same exclusion transaction as preparation.
    // 认领与准备使用相同的互斥事务。
    let business_guard = runtime
        .claim_and_reserve_active_task_run(&invocation)
        .unwrap();
    assert!(
        runtime
            .try_reserve_child_question_execution(task_id)
            .unwrap()
            .is_none()
    );
    drop(business_guard);
}

/// Verifies all active statuses defer auxiliary work and stopped/archived records reject it.
/// 验证所有活动状态延后辅助工作，停止与归档记录拒绝辅助工作。
#[test]
fn child_question_reservation_respects_business_status_contract() {
    for status in [
        TaskSessionStatus::Created,
        TaskSessionStatus::Running,
        TaskSessionStatus::WaitingChildren,
        TaskSessionStatus::WaitingFollowup,
        TaskSessionStatus::Compacting,
    ] {
        // Each typed active status must prevent admission during a Provider gap.
        // 每种强类型活动状态都必须在 Provider 空隙期间阻止准入。
        let (runtime, _, task_id) = seeded_child(status);
        assert!(
            runtime
                .try_reserve_child_question_execution(task_id)
                .unwrap()
                .is_none()
        );
    }
    for status in [TaskSessionStatus::Stopped, TaskSessionStatus::Archived] {
        // Explicitly stopped children cannot be silently awakened.
        // 明确停止的子任务不能被静默唤醒。
        let (runtime, _, task_id) = seeded_child(status);
        assert!(
            runtime
                .try_reserve_child_question_execution(task_id)
                .is_err()
        );
    }
    // A failed business task is settled and may answer without replacing its failure.
    // 失败业务任务已经落定，可以答疑而不替换其失败结果。
    let (runtime, _, task_id) = seeded_child(TaskSessionStatus::Failed);
    assert!(
        runtime
            .try_reserve_child_question_execution(task_id)
            .unwrap()
            .is_some()
    );
}

/// Verifies the final backend claim rejects an auxiliary owner before assigning business ownership.
/// 验证最终后端认领在分配业务所有权前拒绝已有辅助所有者。
#[test]
fn child_question_reservation_excludes_backend_claim_before_record_mutation() {
    // Real preparation supplies every required field of the backend request.
    // 真实准备提供后端请求的全部必需字段。
    let (runtime, parent_id, task_id) = seeded_child(TaskSessionStatus::Completed);
    // Retain a request while returning its fixture record to a settled, unclaimed state.
    // 保留请求，同时将夹具记录恢复为已落定且尚未认领的状态。
    let invocation = prepare_business(&runtime, parent_id, task_id).unwrap();
    {
        // This fixture makes an accidental early claim observable on execution_id.
        // 此夹具通过 execution_id 使意外的提前认领可观测。
        let mut sessions = runtime.sessions.write().unwrap();
        // Only the known child is adjusted; auxiliary reservation never writes this record.
        // 仅调整已知子记录；辅助预留本身绝不写入此记录。
        let record = sessions.get_mut(&task_id).unwrap();
        record.last_status = String::from("completed");
        record.execution_id = None;
    }
    // The auxiliary owner must exclude both preparation and final backend claim.
    // 辅助所有者必须同时排除准备及最终后端认领。
    let guard = runtime
        .try_reserve_child_question_execution(task_id)
        .unwrap()
        .unwrap();
    // Rejection must happen before claim_task_execution_record_owner can fill the identity.
    // 拒绝必须发生在 claim_task_execution_record_owner 填入身份之前。
    let error = runtime
        .claim_and_reserve_active_task_run(&invocation)
        .err()
        .unwrap();
    assert!(error.contains("handling auxiliary execution"));
    assert!(
        runtime
            .sessions
            .read()
            .unwrap()
            .get(&task_id)
            .unwrap()
            .execution_id
            .is_none()
    );
    assert!(!runtime.is_task_running(task_id).unwrap());
    drop(guard);
}

/// Verifies simultaneous cold-child reservations have one owner and create no business record.
/// 验证冷子会话的并发预留只有一个所有者且不会创建业务记录。
#[test]
fn child_question_reservation_concurrent_cold_admission_has_one_owner() {
    // Cold runtime deliberately has no resident child business record.
    // 冷运行时刻意不持有常驻子业务记录。
    let runtime = Arc::new(TaskRuntime::new());
    // Canonical child identity would be resolved and authorized by the host.
    // 规范子身份由宿主负责解析及授权。
    let task_id = Uuid::new_v4();
    // Barrier aligns both callers without timing sleeps.
    // 屏障对齐两个调用者，不依赖定时休眠。
    let barrier = Arc::new(std::sync::Barrier::new(2));
    // Completed thread results retain winning ownership until both joins finish.
    // 已完成线程的返回值保留获胜所有权，直到两个连接都结束。
    let handles: Vec<_> = (0..2)
        .map(|_| {
            // Shared runtime supplies the same task transaction to both callers.
            // 共享运行时向两个调用者提供相同任务事务。
            let runtime = Arc::clone(&runtime);
            // Shared barrier starts both reservations together.
            // 共享屏障同时启动两个预留。
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                barrier.wait();
                runtime
                    .try_reserve_child_question_execution(task_id)
                    .unwrap()
            })
        })
        .collect();
    // Keep returned guards alive so the second caller cannot acquire after early release.
    // 保持返回守卫存活，避免第二调用者在提前释放后获得准入。
    let guards: Vec<_> = handles
        .into_iter()
        .map(|handle| handle.join().unwrap())
        .collect();
    assert_eq!(guards.iter().filter(|guard| guard.is_some()).count(), 1);
    assert!(runtime.sessions.read().unwrap().is_empty());
    assert!(!runtime.is_task_running(task_id).unwrap());
    drop(guards);
    assert!(
        runtime
            .try_reserve_child_question_execution(task_id)
            .unwrap()
            .is_some()
    );
}
