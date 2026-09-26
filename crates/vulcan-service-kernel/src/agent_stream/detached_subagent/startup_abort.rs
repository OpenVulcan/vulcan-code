//! Explicit startup-abort handling under the original listener admission identity.
//! 在原监听器准入身份下处理明确启动中止。

use super::*;
use startup_rejection::DetachedStartupOwner;
use vulcan_session_boundary::SubagentMailboxEventKind;
use vulcan_session_core::v1::collaboration::CollaborationExecutionId;

/// Settles an explicit startup abort after its runtime lease has been released.
/// 在运行时租约释放后结算明确启动中止。
/// Captured owner and listener coordinates authorize cleanup; host event supplies the original failure.
/// 捕获的所有者及监听器坐标授权清理；宿主事件提供原始失败。
/// Returns after source/parent durability and exact gate cleanup, preserving errors for recovery.
/// 来源与父状态可靠落盘并完成精确门清理后返回，保留错误以便恢复。
// Stream ownership, generation guards and independently borrowed telemetry remain explicit at this event boundary.
// 事件边界显式保留流所有权、代际守卫和独立借用的遥测状态。
#[expect(
    clippy::too_many_arguments,
    reason = "Stream ownership, generation guards and independently borrowed telemetry remain explicit at this event boundary."
)]
pub(crate) fn handle_startup_abort(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    runtime: &AgentRuntimeBridge,
    parent_id: SessionId,
    task_id: SessionId,
    execution_id: &str,
    child: &Arc<SessionHandle>,
    owner: DetachedStartupOwner,
    event: &RuntimeEvent,
) -> Result<(), String> {
    // Freeze the canonical registry without holding the UI lock across any storage operation.
    // 冻结规范注册表，不让界面锁跨越任何存储操作。
    let registry = {
        // A superseded listener cannot clean up a newer execution's child or return gate.
        // 被替代的监听器不能清理更新执行的子任务或回流门。
        let guard = state.lock().map_err(|error| error.to_string())?;
        if guard
            .detached_subagent_task_listeners
            .get(&task_id)
            .is_none_or(|id| id != execution_id)
        {
            return Ok(());
        }
        guard.session_runtime_registry.clone()
    };
    // Startup abort is distinct from natural failure only in its trusted lifecycle stage.
    // 启动中止与自然失败的差别仅在于其可信生命周期阶段。
    let mailbox = non_success::terminal_mailbox(event, parent_id, task_id, execution_id)?;
    if event.kind != RuntimeEventKind::DelegateFinished
        || mailbox.kind != SubagentMailboxEventKind::Failed
    {
        return Err(String::from(
            "Startup abort requires explicit host failure feedback.",
        ));
    }
    // Keep the real error intact for the pre-reservation child closure as well.
    // 对预留前子任务闭合也完整保留实际错误。
    let reason = mailbox
        .payload
        .get("error_text")
        .and_then(serde_json::Value::as_str)
        .filter(|text| !text.trim().is_empty())
        .ok_or("Startup abort is missing its original cause.")?;
    // Both existing and absent reservations are checked only after the runtime released its lease.
    // 只有运行时释放租约后，才检查预留存在或缺失两种状态。
    let parent = registry.get(parent_id).map_err(|error| error.to_string())?;
    // The execution key is never inferred from the physical task or the currently open turn.
    // 执行键绝不由物理任务或当前开放回合推断。
    let execution =
        CollaborationExecutionId::try_new(execution_id).map_err(|error| error.to_string())?;
    // Read the committed reservation, including a write that completed just before lease release.
    // 读取已提交预留，包括恰好在租约释放前完成的写入。
    let reservation = parent
        .read_full_durable(|document| {
            document
                .collaboration_state()
                .child_tasks
                .get(&execution)
                .cloned()
        })
        .map_err(|error| error.to_string())?;
    if let Some(reservation) = reservation {
        if reservation.child.segment_generation != owner.generation
            || reservation
                .child
                .turn_id
                .is_some_and(|turn| turn != owner.turn_id)
        {
            return Err(String::from(
                "Startup abort reservation differs from its captured child owner.",
            ));
        }
        // Validate the captured root before the shared publisher resolves any terminal facts.
        // 在共享发布器解析终态事实之前，验证捕获的根输入。
        child
            .read_full_durable(|document| {
                if document.segment_generation() != owner.generation
                    || !document.events().iter().any(|entry| {
                        entry.turn_id == Some(owner.turn_id)
                            && matches!(&entry.event, EventBody::UserInput(input)
                    if input.client_action_id.as_str() == execution_id)
                    })
                {
                    return Err(String::from(
                        "Startup abort source differs from its admission owner.",
                    ));
                }
                Ok(())
            })
            .map_err(|error| error.to_string())??;
        non_success::publish_non_success(
            &parent,
            child,
            execution_id,
            &mailbox,
            current_v1_unix_millis()?,
        )?;
        runtime
            .task_runtime
            .publish_durable_task_return(execution_id, mailbox)?;
    } else {
        // No owned task was accepted: close the admitted child and withdraw the unpublished gate.
        // 尚未接纳所属任务：关闭已准入子文档，并撤销未发布门。
        startup_rejection::close_rejected_child(child, owner, reason)?;
        runtime
            .task_runtime
            .cancel_rejected_task_return(task_id, execution_id)?;
    }
    // Only durable success permits listener retirement; an error keeps recovery ownership visible.
    // 只有可靠成功才允许退休监听器；错误使恢复归属继续可见。
    let mut guard = state.lock().map_err(|error| error.to_string())?;
    clear_webui_detached_listener_execution_if_current(&mut guard, task_id, execution_id);
    drop(guard);
    emit_webui_subagent_navigation_changed_delta_for_parent(app, state, parent_id)?;
    Ok(())
}
