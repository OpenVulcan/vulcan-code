//! Finished stream event orchestration.
//! 流终态事件编排。

use super::*;

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
    report: ExecutionRunReport,
) -> FinishedStreamEventOutcome {
    // The report session is the sole owner key for all terminal mutations.
    // 报告会话是全部终态变更的唯一所有权键。
    let finished_session_id = report.session_id;
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

    // Removing the exact active task is the mandatory gate before every terminal side effect.
    // 移除精确活动任务是执行任何终态副作用前的强制闸门。
    let owns_active_stream_entry = match task_state.lock() {
        Ok(mut guard) => {
            // Identity equality prevents a replaced task from terminalizing the replacement.
            // 身份相等校验防止已被替换的任务终结替代任务。
            let owns_entry = remove_active_stream_task_if_id(
                &mut guard,
                finished_session_id,
                active_stream_task_id,
            )
            .is_some();
            if owns_entry {
                finish_webui_session_kernel_turn(
                    &mut guard,
                    finished_session_id,
                    finished_turn_terminal_status(report.error.is_some()),
                );
            }
            owns_entry
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
    if !owns_active_stream_entry {
        webui_trace(
            "stream_finished_skipped_stale_owner",
            format!("session={finished_session_id} task_id={active_stream_task_id}"),
        );
        return FinishedStreamEventOutcome::StaleOwner;
    }

    if let Err(error) = record_webui_provider_response_checkpoint(controller_runtime, &report) {
        eprintln!("[webui-service] provider response checkpoint write failed: {error}");
    }
    // Goal elapsed time uses whole seconds and saturates invalid platform conversions to zero.
    // 目标耗时使用整秒，并将无效的平台转换饱和为零。
    let goal_elapsed_seconds = current_unix_millis_value()
        .saturating_sub(stream_task_started_at)
        .checked_div(1_000)
        .and_then(|value| i64::try_from(value).ok())
        .unwrap_or_default();
    if let Err(error) = account_session_goal_from_report(
        task_state,
        controller_runtime,
        &report,
        goal_elapsed_seconds,
    )
    .await
    {
        eprintln!("[webui-service] session goal accounting failed: {error}");
    }

    // Boundary events are captured before report ownership moves into the in-memory projection.
    // 边界事件在报告所有权移入内存投影前完成捕获。
    let terminal_lifecycle = report
        .turn_completed_boundary_event(
            u64::try_from(current_unix_millis_value()).unwrap_or_default(),
        )
        .map(|terminal_boundary_event| {
            // Final assistant boundaries precede the hidden terminal boundary in lifecycle storage.
            // 最终助手边界在生命周期存储中位于隐藏终态边界之前。
            let final_boundary_events = report
                .final_assistant_boundary_event(
                    u64::try_from(current_unix_millis_value()).unwrap_or_default(),
                )
                .into_iter()
                .collect::<Vec<_>>();
            // The preparation title is authoritative; missing failure preparation uses the established placeholder.
            // 准备阶段标题是权威来源；失败报告缺失准备信息时使用既定占位标题。
            let session_title = report
                .preparation
                .as_ref()
                .map(|preparation| preparation.session_title.clone())
                .unwrap_or_else(|| String::from("Untitled"));
            (
                final_boundary_events,
                terminal_boundary_event,
                session_title,
            )
        });
    // Measures terminal report projection, including turn stats and final session state.
    // 衡量终态报告投影耗时，包括统计行与最终会话状态。
    let final_report_apply_started_at = current_unix_millis_value();
    // The optional apply result gates lifecycle, hook, and persistence work after projection failure.
    // 可选应用结果在投影失败后阻止生命周期、Hook 与持久化工作。
    let final_report_apply =
        match apply_execution_report_to_memory(app, task_state, agent_runtime, report) {
            Ok(apply) => Some(apply),
            Err(error) => {
                eprintln!("[webui-service] apply final report failed: {error}");
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

    if let (Some(apply), Some((final_boundary_events, terminal_boundary_event, session_title))) =
        (final_report_apply.as_ref(), terminal_lifecycle)
        && let Some(project_id) =
            owning_project_id_for_session(&apply.workspace_snapshot, finished_session_id)
    {
        // The cloned runtime moves into the non-blocking external lifecycle task.
        // 克隆后的运行时移入非阻塞外部生命周期任务。
        let controller_runtime_for_lifecycle = controller_runtime.clone();
        spawn_service_task(async move {
            // Lifecycle latency is measured inside the spawned task to exclude scheduling delay ambiguity.
            // 生命周期延迟在生成任务内部测量，以排除调度延迟歧义。
            let lifecycle_started_at = current_unix_millis_value();
            if let Err(error) = finish_session_external_turn_lifecycle(
                &controller_runtime_for_lifecycle,
                &project_id,
                finished_session_id,
                &session_title,
                final_boundary_events,
                terminal_boundary_event,
            ) {
                eprintln!("[webui-service] append session terminal boundary failed: {error}");
            }
            webui_trace_if_slow(
                "stream_terminal_lifecycle_finish_slow",
                lifecycle_started_at,
                format!("session={finished_session_id}"),
            );
        });
    }

    // Chained turns receive priority over direct final persistence for the same session.
    // 链式回合优先于同一会话的直接最终持久化。
    let started_next_prompt = maybe_start_webui_turn_chain_after_finished(
        app,
        task_state,
        controller_runtime,
        agent_runtime,
        finished_session_id,
    )
    .await;
    // A completion request exists only when terminal memory projection opened a completed-turn hook.
    // 仅当终态内存投影打开完成回合 Hook 时才存在完成请求。
    if let Some(completion_request) = final_report_apply
        .as_ref()
        .and_then(|apply| apply.completion_request.clone())
    {
        spawn_conversation_completion_hook_task(
            app.clone(),
            task_state.clone(),
            controller_runtime.clone(),
            agent_runtime.clone(),
            completion_request,
        );
    }
    if !started_next_prompt && let Some(apply) = final_report_apply {
        // Measures final session persistence only when no chained turn starts immediately.
        // 仅在没有立即链式启动下一回合时衡量最终会话持久化耗时。
        let final_persist_started_at = current_unix_millis_value();
        if let Err(error) = persist_workspace_session_update(
            Some(task_state),
            controller_runtime.clone(),
            &apply.workspace_snapshot,
            finished_session_id,
        )
        .await
        {
            eprintln!("[webui-service] persist final report failed: {error}");
        }
        webui_trace_if_slow(
            "stream_final_persist_slow",
            final_persist_started_at,
            format!("session={finished_session_id}"),
        );
    }
    FinishedStreamEventOutcome::Handled
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
}
