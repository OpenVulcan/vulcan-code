//! Finished stream event orchestration.
//! 流终态事件编排。

use super::super::provider_v1_sink::ProviderV1FinishedDurability;
use super::*;
use crate::session_goal_lifecycle::registered_session_v1_handle_from_state;

/// Outcome of handling one finished stream report.
/// 单个流终态报告的处理结果。
pub(super) enum FinishedStreamEventOutcome {
    /// The active stream owner completed the terminal handling sequence.
    /// 活动流所有者已经完成终态处理序列。
    Handled,
    /// The task no longer owns the session stream slot and skipped terminal side effects.
    /// 当前任务已不再拥有会话流槽位，并跳过终态副作用。
    StaleOwner,
    /// Shared-state ownership could not be inspected, so terminal side effects were skipped.
    /// 无法检查共享状态所有权，因此跳过终态副作用。
    OwnershipUnavailable,
}

/// Decides whether the durable API terminal fully represents the final execution report.
/// 判定可靠 API 终态是否完整表达最终执行报告。
///
/// # Parameters
/// 参数
/// `api_terminal_persisted` states whether the final physical API has a durable terminal receipt.
/// `api_terminal_persisted` 表示最终物理 API 是否具有可靠终态回执。
/// `api_terminal_failure_persisted` states whether that terminal contains a controlled failure.
/// `api_terminal_failure_persisted` 表示该终态是否包含受控失败。
/// `report_failed` states whether the runtime subsequently reported execution failure.
/// `report_failed` 表示运行时随后是否报告执行失败。
///
/// # Returns
/// 返回值
/// Returns `true` only when no additional durable failure boundary is required.
/// 仅在不需要追加可靠失败边界时返回 `true`。
fn v1_api_terminal_satisfies_report(
    api_terminal_persisted: bool,
    api_terminal_failure_persisted: bool,
    report_failed: bool,
) -> bool {
    api_terminal_persisted && (!report_failed || api_terminal_failure_persisted)
}

/// Handles one terminal execution report after stream generation validation succeeds.
/// 在流代际校验成功后处理单个终态执行报告。
///
/// # Parameters
/// # 参数
/// `app` emits terminal workbench deltas and completion-hook updates.
/// `app` 发送终态工作台增量与完成 Hook 更新。
/// `task_state` owns the active-stream registry and retained workspace state.
/// `task_state` 持有活动流注册表与保留工作区状态。
/// `controller_runtime` persists checkpoints, lifecycle boundaries, goals, and sessions.
/// `controller_runtime` 持久化检查点、生命周期边界、目标与会话。
/// `agent_runtime` provides completion hooks and turn-chaining runtimes.
/// `agent_runtime` 提供完成 Hook 与回合链式运行时。
/// `active_stream_task_id` identifies the exact task allowed to commit terminal effects.
/// `active_stream_task_id` 标识获准提交终态副作用的精确任务。
/// `stream_task_started_at` is the task-entry timestamp used for goal elapsed accounting.
/// `stream_task_started_at` 是用于目标耗时计量的任务入口时间戳。
/// `execution_goal_id` is the latest Goal identity bound at a physical Provider boundary.
/// `execution_goal_id` 是在物理 Provider 边界绑定的最新目标身份。
/// `retry_evidence` proves either a pre-dispatch failure or the latest exact physical Provider call bound to this stream owner.
/// `retry_evidence` 证明分派前失败，或标识绑定到当前流所有者的最近一个精确物理 Provider 调用。
/// `v1_finished_durability` carries the exact durable physical-API terminal and optional logical-Turn completion.
/// `v1_finished_durability` 携带精确可靠的物理 API 终态与可选逻辑回合完成事实。
/// `report` is the authoritative terminal report emitted by the Agent runtime.
/// `report` 是 Agent 运行时发送的权威终态报告。
///
/// # Returns
/// # 返回值
/// `Handled` for the active owner, `StaleOwner` for an identity mismatch, or `OwnershipUnavailable` when state locking fails.
/// 活动所有者返回 `Handled`，身份不匹配返回 `StaleOwner`，状态加锁失败返回 `OwnershipUnavailable`。
pub(super) async fn handle_finished_stream_event(
    app: &ServiceEventEmitter,
    task_state: &SharedServiceBridgeState,
    controller_runtime: &ControllerRuntimeHandle,
    agent_runtime: &AgentRuntimeBridge,
    active_stream_task_id: Uuid,
    stream_task_started_at: i128,
    execution_goal_id: Option<&str>,
    v1_finished_durability: ProviderV1FinishedDurability,
    report: ExecutionRunReport,
) -> FinishedStreamEventOutcome {
    let ProviderV1FinishedDurability {
        api_terminal: v1_api_terminal_receipt,
        api_terminal_failure_persisted,
        turn_completion: turn_completion_receipt,
        ..
    } = v1_finished_durability;
    // A nonempty promoted-input receipt keeps the logical Turn open and authorizes one exact physical continuation.
    // 非空晋升输入回执会保持逻辑回合开放，并授权一次精确物理续跑。
    let pending_in_turn_receipt = v1_api_terminal_receipt
        .as_ref()
        .filter(|receipt| !receipt.value.promoted_input_seqs.is_empty())
        .map(|receipt| receipt.value.clone());
    // Only the durable V1 Turn-completion receipt authorizes settlement and completed-Turn hooks.
    // 只有可靠 V1 回合完成回执可以授权结束事件与已完成回合 Hook。
    let logical_turn_completed = turn_completion_receipt.is_some();
    // The report session is the sole owner key for all terminal mutations.
    // 报告会话是全部终态变更的唯一所有权键。
    let finished_session_id = report.session_id;
    // Preserve the runtime's original terminal error before the memory-only report projection consumes it.
    // 在仅内存报告投影消费报告前保留运行时原始终态错误，避免 V1 历史只剩通用收尾标记。
    let terminal_error = report.error.clone();
    webui_trace(
        "stream_finished",
        format!(
            "session={finished_session_id} provider_requests={} error={} output_chars={}",
            report.provider_request_count,
            report.error.is_some(),
            report
                .assistant_output
                .as_deref()
                .map(str::len)
                .unwrap_or(0)
        ),
    );

    // Removing the exact active task and installing its terminal-persistence marker form one mandatory ownership gate.
    // 移除精确活动任务并安装其终态持久化标记，共同构成强制所有权闸门。
    // terminal_persistence_owner_id identifies this exact stream boundary in the shared admission table.
    // terminal_persistence_owner_id 在共享准入表中标识当前精确流边界。
    let terminal_persistence_owner_id =
        SessionTerminalPersistenceOwner::StreamTask(active_stream_task_id);
    let removed_active_stream_task = match task_state.lock() {
        Ok(mut guard) => {
            // Identity equality prevents a replaced task from terminalizing the replacement.
            // 身份相等校验防止已被替换的任务终结替代任务。
            if !active_stream_task_matches_id(&guard, finished_session_id, active_stream_task_id) {
                return FinishedStreamEventOutcome::StaleOwner;
            }
            if let Err(error) = register_session_terminal_persistence_owner(
                &mut guard,
                finished_session_id,
                terminal_persistence_owner_id,
            ) {
                eprintln!(
                    "[webui-service] finish stream terminal-persistence registration failed: session={finished_session_id} task_id={active_stream_task_id} error={error}"
                );
                return FinishedStreamEventOutcome::OwnershipUnavailable;
            }
            let removed_entry = remove_active_stream_task_if_id(
                &mut guard,
                finished_session_id,
                active_stream_task_id,
            );
            if removed_entry.is_some() {
                finish_webui_session_kernel_turn(
                    &mut guard,
                    finished_session_id,
                    finished_turn_terminal_status(report.error.is_some()),
                );
            } else {
                // rollback keeps an impossible registry disappearance from leaking a permanent admission block.
                // rollback 避免不可能的注册表消失泄漏永久准入阻塞。
                let _ = rollback_session_terminal_persistence_owner(
                    &mut guard,
                    finished_session_id,
                    terminal_persistence_owner_id,
                );
            }
            removed_entry
        }
        Err(error) => {
            eprintln!("[webui-service] finish stream ownership check failed: {error}");
            webui_trace(
                "stream_finished_skipped_stale_owner",
                format!("session={finished_session_id} task_id={active_stream_task_id}"),
            );
            return FinishedStreamEventOutcome::OwnershipUnavailable;
        }
    };
    let Some(removed_active_stream_task) = removed_active_stream_task else {
        webui_trace(
            "stream_finished_skipped_stale_owner",
            format!("session={finished_session_id} task_id={active_stream_task_id}"),
        );
        return FinishedStreamEventOutcome::StaleOwner;
    };
    // Retain the producer-safe error in the console as well, even if terminal persistence subsequently fails.
    // 控制台也保留生产端安全处理后的错误，即使随后终态持久化失败仍可定位原始原因。
    if let Some(error) = terminal_error.as_deref() {
        eprintln!(
            "[webui-service] execution failed: session={finished_session_id} provider_requests={} error={error}",
            report.provider_request_count
        );
    }
    // terminalPersistenceOwner blocks every replacement Continue, prompt, Stop, and kernel turn until persistence settles.
    // terminalPersistenceOwner 在持久化结算前阻止全部替代 Continue、提示词、Stop 与 kernel 回合。
    let terminal_persistence_owner = match SessionTerminalPersistenceOwnerGuard::adopt_registered(
        task_state.clone(),
        finished_session_id,
        terminal_persistence_owner_id,
    ) {
        Ok(owner) => owner,
        Err(error) => {
            eprintln!(
                "[webui-service] finish stream terminal-persistence adoption failed: session={finished_session_id} task_id={active_stream_task_id} error={error}"
            );
            return FinishedStreamEventOutcome::OwnershipUnavailable;
        }
    };
    // retainedGoalId closes the interval between an AI-created Goal tool result and the next Provider boundary.
    // retainedGoalId 闭合 AI 创建目标工具结果与下一次 Provider 边界之间的时间窗口。
    let retained_goal_id = removed_active_stream_task
        .session_goal_id()
        .map(str::to_owned);
    // terminalGoalId prefers the stream-local physical-request binding and falls back to the atomically retained task binding.
    // terminalGoalId 优先使用流内物理请求绑定，并回退到原子保留的任务绑定。
    let terminal_goal_id = execution_goal_id.or(retained_goal_id.as_deref());

    // Goal elapsed time uses whole seconds and saturates invalid platform conversions to zero.
    // 目标耗时使用整秒，并将无效的平台转换饱和为零。
    let goal_elapsed_seconds = current_unix_millis_value()
        .saturating_sub(stream_task_started_at)
        .checked_div(1_000)
        .and_then(|value| i64::try_from(value).ok())
        .unwrap_or_default();
    webui_trace(
        "stream_goal_accounting_start",
        format!(
            "session={finished_session_id} has_goal={} elapsed_seconds={goal_elapsed_seconds}",
            terminal_goal_id.is_some()
        ),
    );
    match account_session_goal_from_report(
        task_state,
        controller_runtime,
        &report,
        terminal_goal_id,
        goal_elapsed_seconds,
    )
    .await
    {
        Ok(true) => {
            if let Err(error) =
                emit_session_addons_changed_delta(app, task_state, finished_session_id)
            {
                eprintln!("[webui-service] terminal session goal delta emit failed: {error}");
            }
        }
        Ok(false) => {}
        Err(error) => {
            eprintln!("[webui-service] session goal accounting failed: {error}");
        }
    }
    webui_trace(
        "stream_goal_accounting_finished",
        format!("session={finished_session_id}"),
    );

    // Measures terminal report projection, including turn stats and final session state.
    // 衡量终态报告投影耗时，包括统计行与最终会话状态。
    let final_report_apply_started_at = current_unix_millis_value();
    webui_trace(
        "stream_final_report_apply_start",
        format!("session={finished_session_id}"),
    );
    // The optional apply result owns only process-local UI state and completion-hook inputs.
    // 可选应用结果只负责进程内 UI 状态与完成 Hook 输入。
    let final_report_apply = match apply_execution_report_to_memory(
        app,
        task_state,
        agent_runtime,
        report,
    ) {
        Ok(apply) => Some(apply),
        Err(error) => {
            eprintln!("[webui-service] apply final report failed: {error}");
            // A terminal report projection error must never leave the visible conversation running after ownership is released.
            // 终态报告投影错误绝不能在所有权释放后仍让可见会话保持运行中。
            if let Err(projection_error) = apply_session_phase(
                app,
                task_state,
                finished_session_id,
                SessionRunPhase::Failed,
            ) {
                eprintln!(
                    "[webui-service] apply final report fallback phase failed: {projection_error}"
                );
            }
            None
        }
    };
    webui_trace_if_slow(
        "stream_final_report_apply_slow",
        final_report_apply_started_at,
        format!(
            "session={finished_session_id} applied={}",
            final_report_apply.is_some()
        ),
    );
    // A successful ApiStats cannot hide a later host projection failure; that failure needs its own durable boundary.
    // 成功 ApiStats 不能掩盖随后发生的宿主投影失败；该失败需要独立的可靠边界。
    let v1_terminal_authority_persisted = v1_api_terminal_satisfies_report(
        v1_api_terminal_receipt.is_some(),
        api_terminal_failure_persisted,
        terminal_error.is_some(),
    );
    // failure_reason_code distinguishes a missing terminal from a failure observed after successful API persistence.
    // failure_reason_code 区分缺失终态与成功 API 持久化后才观察到的失败。
    let failure_reason_code = if v1_api_terminal_receipt.is_some() {
        "stream_failed_after_v1_api_terminal"
    } else {
        "stream_finished_without_v1_api_terminal"
    };
    // A sufficient ApiStats or an explicit fallback is required before the exact owner may reopen admission.
    // 只有充分的 ApiStats 或明确回退落盘后，精确 owner 才允许重新开放准入。
    let terminal_authority_settled = settle_stream_terminal_authority(
        task_state,
        finished_session_id,
        v1_terminal_authority_persisted,
        failure_reason_code,
        terminal_error,
        terminal_persistence_owner,
    )
    .await;
    webui_trace(
        "stream_terminal_authority",
        format!(
            "session={finished_session_id} terminal_authority_settled={terminal_authority_settled}"
        ),
    );

    // The durable V1 Turn receipt alone selects and authorizes a successor; legacy persistence cannot veto it.
    // 可靠 V1 回合回执独自选择并授权后继；旧持久化不得否决该事实。
    // Re-read only after releasing terminal admission: a child can finish after the sink snapshot.
    // 释放终态准入后重新读取：子任务可能在 sink 快照之后完成。
    let pending_subagent_return = if terminal_authority_settled {
        match registered_session_v1_handle_from_state(task_state, finished_session_id).and_then(
            |handle| {
                handle
                    .read(|document| document.pending_subagent_return_dispatch())
                    .map_err(|error| error.to_string())?
                    .map_err(|error| error.to_string())
            },
        ) {
            Ok(receipt) => receipt,
            Err(error) => {
                eprintln!(
                    "[webui-service] read terminal subagent return failed: session={finished_session_id} error={error}"
                );
                None
            }
        }
    } else {
        None
    };
    let started_next_prompt = if let Some(receipt) = pending_subagent_return {
        match start_exact_v1_subagent_return_turn(
            app,
            task_state,
            controller_runtime,
            agent_runtime,
            finished_session_id,
            receipt,
        )
        .await
        {
            Ok(started) => started,
            Err(error) => {
                eprintln!(
                    "[webui-service] dispatch exact V1 subagent return failed: session={finished_session_id} error={error}"
                );
                false
            }
        }
    } else if let Some(receipt) = turn_completion_receipt {
        maybe_start_webui_turn_chain_after_finished(
            app,
            task_state,
            controller_runtime,
            agent_runtime,
            finished_session_id,
            receipt,
        )
        .await
    } else if let Some(receipt) = pending_in_turn_receipt {
        match start_exact_v1_pending_in_turn(
            app,
            task_state,
            controller_runtime,
            agent_runtime,
            finished_session_id,
            receipt,
        )
        .await
        {
            Ok(started) => started,
            Err(error) => {
                eprintln!(
                    "[webui-service] dispatch exact V1 in-turn continuation failed: session={finished_session_id} error={error}"
                );
                false
            }
        }
    } else {
        false
    };
    if terminal_authority_settled
        && logical_turn_completed
        && let Some(apply) = final_report_apply.as_ref()
        && let Some(event) = conversation_run_settled_event_from_terminal_facts(
            finished_session_id,
            apply.project_id.clone(),
            active_stream_task_id,
            apply.conversation_run_outcome,
            started_next_prompt,
            current_unix_millis_text(),
        )
    {
        app.emit(ServiceEvent::ConversationRunSettled(Box::new(event)));
    }
    // A completion request exists only when terminal memory projection opened a completed-turn hook.
    // 仅当终态内存投影打开完成回合 Hook 时才存在完成请求。
    if terminal_authority_settled
        && logical_turn_completed
        && !started_next_prompt
        && let Some(completion_request) = final_report_apply
            .as_ref()
            .and_then(|apply| apply.completion_request.clone())
    {
        spawn_conversation_completion_hook_task(
            app.clone(),
            task_state.clone(),
            agent_runtime.clone(),
            completion_request,
        );
    }
    FinishedStreamEventOutcome::Handled
}

/// Builds a settled lifecycle event only from terminal facts that close the visible conversation run.
/// 仅根据闭合可见会话运行的终态事实构建最终结束生命周期事件。
///
/// # Parameters
/// # 参数
/// `conversation_id` identifies the regular conversation whose report was accepted.
/// `conversation_id` 标识终态报告已被接纳的普通会话。
/// `project_id` is the owning project resolved from the accepted workspace projection.
/// `project_id` 是从已接纳工作区投影解析出的所属项目。
/// `active_stream_task_id` is the exact stream identity that owned terminal side effects.
/// `active_stream_task_id` 是持有终态副作用的精确流身份。
/// `outcome` exists only when the completion gate declared the logical turn final.
/// `outcome` 仅在完成闸门声明逻辑回合最终结束时存在。
/// `started_next_prompt` indicates whether an exact logical successor or same-Turn physical continuation started.
/// `started_next_prompt` 表示精确逻辑后继或同一回合的物理续跑是否已启动。
/// `occurred_at` is the service timestamp captured for the emitted event.
/// `occurred_at` 是为发出事件捕获的服务时间戳。
///
/// # Returns
/// # 返回值
/// A deterministic lifecycle event for final idle settlement, otherwise `None`.
/// 最终空闲结束时返回确定性生命周期事件，否则返回 `None`。
fn conversation_run_settled_event_from_terminal_facts(
    conversation_id: SessionId,
    project_id: Option<String>,
    active_stream_task_id: Uuid,
    outcome: Option<vulcan_service_api::ConversationRunOutcome>,
    started_next_prompt: bool,
    occurred_at: String,
) -> Option<ConversationRunSettledEvent> {
    if started_next_prompt {
        return None;
    }
    // outcome is the finality proof produced by the authoritative completion gate.
    // outcome 是权威完成闸门产生的最终性证明。
    let outcome = outcome?;
    // run_id is the exact stream UUID registered before execution began.
    // run_id 是执行开始前注册的精确流 UUID。
    let run_id = active_stream_task_id.to_string();
    Some(ConversationRunSettledEvent {
        event_id: format!("{conversation_id}:{run_id}:settled"),
        conversation_id: conversation_id.to_string(),
        project_id,
        run_id,
        outcome,
        occurred_at,
    })
}

/// Maps report error presence to the session-kernel terminal status.
/// 将报告错误存在性映射到会话内核终态状态。
///
/// # Parameters
/// # 参数
/// `has_error` indicates whether the terminal execution report carries an error.
/// `has_error` 表示终态执行报告是否携带错误。
///
/// # Returns
/// # 返回值
/// `Failed` for error reports and `Completed` for successful reports.
/// 错误报告返回 `Failed`，成功报告返回 `Completed`。
fn finished_turn_terminal_status(has_error: bool) -> TurnTerminalStatus {
    if has_error {
        TurnTerminalStatus::Failed
    } else {
        TurnTerminalStatus::Completed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies that report success and failure map to exact kernel terminal states.
    /// 验证报告成功与失败会映射到精确的内核终态。
    ///
    /// # Parameters
    /// # 参数
    /// This test has no parameters.
    /// 此测试没有参数。
    ///
    /// # Returns
    /// # 返回值
    /// This test returns no value and fails through assertions.
    /// 此测试无返回值，并通过断言报告失败。
    #[test]
    fn report_error_presence_defines_kernel_terminal_status() {
        assert!(matches!(
            finished_turn_terminal_status(false),
            TurnTerminalStatus::Completed
        ));
        assert!(matches!(
            finished_turn_terminal_status(true),
            TurnTerminalStatus::Failed
        ));
    }

    /// Verifies a successful API terminal cannot satisfy a later failed execution report.
    /// 验证成功 API 终态不能满足随后出现的失败执行报告。
    ///
    /// # Parameters
    /// 参数
    /// This test has no parameters.
    /// 此测试没有参数。
    ///
    /// # Returns
    /// 返回值
    /// This test returns no value and fails through assertions.
    /// 此测试无返回值，并通过断言报告失败。
    #[test]
    fn api_terminal_must_contain_failure_when_report_failed() {
        assert!(!v1_api_terminal_satisfies_report(false, false, false));
        assert!(!v1_api_terminal_satisfies_report(false, false, true));
        assert!(!v1_api_terminal_satisfies_report(false, true, true));
        assert!(v1_api_terminal_satisfies_report(true, false, false));
        assert!(!v1_api_terminal_satisfies_report(true, false, true));
        assert!(v1_api_terminal_satisfies_report(true, true, true));
    }

    /// Verifies completed and failed outcomes produce distinct deterministic lifecycle events.
    /// 验证完成与失败结果会产生不同且确定的生命周期事件。
    ///
    /// # Parameters
    /// # 参数
    /// This test has no parameters.
    /// 此测试没有参数。
    ///
    /// # Returns
    /// # 返回值
    /// This test returns no value and fails through assertions.
    /// 此测试无返回值，并通过断言报告失败。
    #[test]
    fn final_idle_terminal_facts_build_deterministic_settlement_events() {
        // conversation_id identifies the exact regular session under test.
        // conversation_id 标识本测试中的精确普通会话。
        let conversation_id = SessionId::new_v4();
        // run_id identifies one exact stream generation for deterministic event ids.
        // run_id 标识一个精确流代次，用于生成确定性事件标识。
        let run_id = Uuid::new_v4();
        // completed is the event derived from a final successful report.
        // completed 是从最终成功报告派生的事件。
        let completed = conversation_run_settled_event_from_terminal_facts(
            conversation_id,
            Some(String::from("project-a")),
            run_id,
            Some(vulcan_service_api::ConversationRunOutcome::Completed),
            false,
            String::from("1000"),
        )
        .expect("final successful run should settle");
        // failed is the event derived from the same exact run identity with a failure result.
        // failed 是使用相同精确运行身份与失败结果派生的事件。
        let failed = conversation_run_settled_event_from_terminal_facts(
            conversation_id,
            Some(String::from("project-a")),
            run_id,
            Some(vulcan_service_api::ConversationRunOutcome::Failed),
            false,
            String::from("1000"),
        )
        .expect("final failed run should settle");
        assert_eq!(completed.event_id, failed.event_id);
        assert_eq!(completed.run_id, run_id.to_string());
        assert_eq!(completed.conversation_id, conversation_id.to_string());
        assert_eq!(completed.project_id.as_deref(), Some("project-a"));
        assert_eq!(
            completed.outcome,
            vulcan_service_api::ConversationRunOutcome::Completed
        );
        assert_eq!(
            failed.outcome,
            vulcan_service_api::ConversationRunOutcome::Failed
        );
    }

    /// Verifies pending child work and immediate chained turns suppress settlement events.
    /// 验证待处理子工作与立即链式回合会抑制最终结束事件。
    ///
    /// # Parameters
    /// # 参数
    /// This test has no parameters.
    /// 此测试没有参数。
    ///
    /// # Returns
    /// # 返回值
    /// This test returns no value and fails through assertions.
    /// 此测试无返回值，并通过断言报告失败。
    #[test]
    fn unfinished_or_chained_terminal_facts_do_not_build_settlement_events() {
        // conversation_id identifies the exact regular session under test.
        // conversation_id 标识本测试中的精确普通会话。
        let conversation_id = SessionId::new_v4();
        // run_id identifies the exact owned stream generation.
        // run_id 标识精确持有的流代次。
        let run_id = Uuid::new_v4();
        assert!(
            conversation_run_settled_event_from_terminal_facts(
                conversation_id,
                None,
                run_id,
                None,
                false,
                String::from("1000"),
            )
            .is_none()
        );
        assert!(
            conversation_run_settled_event_from_terminal_facts(
                conversation_id,
                None,
                run_id,
                Some(vulcan_service_api::ConversationRunOutcome::Completed),
                true,
                String::from("1000"),
            )
            .is_none()
        );
    }
}
