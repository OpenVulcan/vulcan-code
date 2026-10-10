//! Durable child message admission before runtime close and exact post-close receipts.
//! 运行时关闭前可靠封闭子消息准入，并处理精确的关闭后回执。

use super::*;
use vulcan_agent_runtime::agent_runtime::task_runtime::TaskRuntime;
use vulcan_session_core::v1::collaboration::SetRunControlInput;
use vulcan_session_core::{SubagentRegistryEntry, SubagentStatus};
use vulcan_tools_manager::VulcanToolInvocationResult;
use vulcan_workbench_model::subagent_close_agent_projection_from_host_result;

/// Closes a selected runtime child only after its canonical V1 target rejects new messages.
/// 仅在规范 V1 目标拒收新消息后，关闭选中的运行时子会话。
/// `state` owns the canonical handles, `task_runtime` selects exact entries, and `parent_session_id` owns the public target in `args`.
/// `state` 拥有规范句柄，`task_runtime` 选择精确条目，`parent_session_id` 拥有 `args` 中的公开目标。
/// `source_context` is present for model tools and absent for the user-owned panel action.
/// 模型工具提供 `source_context`；用户面板动作不提供模型调用坐标。
/// Returns the runtime result; failed source receipt delivery remains recoverable and is logged.
/// 返回运行时结果；来源回执投递失败仍可恢复并记录日志。
pub(crate) async fn close_child_agent_for_parent(
    state: &SharedServiceBridgeState,
    task_runtime: &Arc<TaskRuntime>,
    parent_session_id: Uuid,
    args: Value,
    source_context: Option<VulcanToolInvocationContext>,
) -> Result<VulcanToolInvocationResult, String> {
    // Resolve the canonical registry before acquiring document lifecycle ownership.
    // 在取得文档生命周期所有权前解析规范注册表。
    let registry = {
        // Release the service lock before any document or runtime lock is acquired.
        // 在取得任何文档或运行时锁之前释放服务锁。
        let guard = state.lock().map_err(|error| error.to_string())?;
        Arc::clone(&guard.session_runtime_registry)
    };
    // Freeze identities while allowing legitimate runtime status changes before admission.
    // 冻结身份，同时允许准入前发生合法运行时状态变化。
    let selected = task_runtime.close_agent_archive_scope(parent_session_id, &args)?;
    // Include storage roots and direct parents to exclude archive and generation replacement.
    // 包含存储根和直接父会话，以排除归档及代次替换。
    let mut handles = Vec::new();
    for entry in &selected {
        // Each runtime child must already have its exact canonical V1 owner.
        // 每个运行时子会话必须已具有精确的规范 V1 所有者。
        let child = registry
            .get(entry.session_id.as_uuid())
            .map_err(|error| error.to_string())?;
        // Root storage identity is supplied by the established usage ledger.
        // 根存储身份由既有用量账本提供。
        let root_id = child
            .lifetime_usage_snapshot()?
            .root_session_id
            .parse::<Uuid>()
            .map_err(|error| error.to_string())?;
        handles.push(registry.get(root_id).map_err(|error| error.to_string())?);
        handles.push(
            registry
                .get(entry.parent_session_id.as_uuid())
                .map_err(|error| error.to_string())?,
        );
        handles.push(child);
    }
    // Keep the selected runtime alive until blocking publication finishes.
    // 在阻塞发布完成前保持选中运行时存活。
    let runtime_for_write = Arc::clone(task_runtime);
    // Reuse the registry whose lifecycle guards protect the callback.
    // 复用以生命周期锁保护回调的注册表。
    let registry_for_write = Arc::clone(&registry);
    // Retain source validation without extending service lifetime.
    // 保留来源校验能力，且不延长服务生命周期。
    let state_weak = Arc::downgrade(state);
    // A cancelled async waiter cannot release these gates before the actual write finishes.
    // 异步等待者取消不能在实际写入完成之前释放这些锁。
    let result = lifecycle_write::with_canonical_handles(&registry, handles, move || {
        if let Some(context) = source_context.as_ref() {
            validate_invocation(&state_weak, context, vulcan_subagent_core::CLOSE_AGENT_TOOL)?;
        }
        runtime_for_write.invoke_close_agent_with_pre_archive(parent_session_id, args, |entries| {
            if entries.len() != selected.len()
                || entries.iter().any(|entry| {
                    !selected.iter().any(|expected| {
                        expected.session_id == entry.session_id
                            && expected.parent_session_id == entry.parent_session_id
                            && expected.agent_id == entry.agent_id
                    })
                })
            {
                return Err(String::from(
                    "close child scope changed before lifecycle admission",
                ));
            }
            // Timestamp the actual stop after all lifecycle and runtime locks are held.
            // 在持有全部生命周期和运行时锁后记录实际停止时间。
            let stopped_at_ms = now_ms()?;
            for entry in entries {
                seal_exact_child_target(&registry_for_write, entry, stopped_at_ms)?;
            }
            Ok(())
        })
    })
    .await?;
    let Some(host_result) = result.host_result.as_ref() else {
        return Ok(result);
    };
    // Only the runtime's structured close result authorizes handle cleanup.
    // 只有运行时结构化关闭结果能够授权句柄清理。
    let projection =
        subagent_close_agent_projection_from_host_result(&host_result.kind, &host_result.payload)?
            .ok_or_else(|| String::from("close_agent returned no close projection"))?;
    for task_id in projection.matched_task_ids {
        match super::reconcile_stopped_question_deliveries_for_session(state, task_id).await {
            Ok(_) => {
                if let Err(error) = release_closed_child_handle(state, task_id).await {
                    tracing::warn!(%task_id, %error, "Closed child V1 handle remains registered");
                }
            }
            Err(error) => {
                // Keep the canonical handle available for the original terminal or recovery attempt.
                // 保留规范句柄，供原终态或恢复尝试继续使用。
                tracing::warn!(%task_id, %error, "Closed child question return remains pending");
            }
        }
    }
    Ok(result)
}

/// Releases an already closed child after its original turn and listener have both settled.
/// 原始回合与监听器均结算后，释放已经关闭的子会话。
/// `state` owns the registry and listener map; `task_id` comes from a committed runtime archive.
/// `state` 拥有注册表和监听器映射；`task_id` 来自已提交运行时归档。
/// Returns without unloading active terminal work; repeated cleanup is harmless under the lifecycle gate.
/// 活动终态工作尚存时保留句柄；生命周期门使重复清理安全。
pub(crate) async fn release_closed_child_handle(
    state: &SharedServiceBridgeState,
    task_id: Uuid,
) -> Result<(), String> {
    // Capture the canonical registry without retaining the service lock.
    // 捕获规范注册表，不保留服务锁。
    let registry = state
        .lock()
        .map_err(|error| error.to_string())?
        .session_runtime_registry
        .clone();
    // Serialize unloading with resumption and durable document changes.
    // 将卸载与恢复和可靠文档变更串行化。
    let _lifecycle = registry
        .acquire_lifecycle_transition(task_id)
        .await
        .map_err(|error| error.to_string())?;
    // A previous terminal callback may already have completed cleanup.
    // 先前终态回调可能已经完成清理。
    let child = match registry.get(task_id) {
        Ok(child) => child,
        Err(SessionRuntimeError::SessionNotRegistered(id)) if id == task_id => return Ok(()),
        Err(error) => return Err(error.to_string()),
    };
    // A prior interrupt can finish the runtime record before the original V1 terminal is published.
    // 先前中断可能在原始 V1 终态发布前结束运行时记录。
    let listener_pending = state
        .lock()
        .map_err(|error| error.to_string())?
        .detached_subagent_task_listeners
        .contains_key(&task_id);
    // An open business turn retains its source owner until terminal publication.
    // 开放业务回合在终态发布前保留来源所有者。
    let turn_pending = child
        .read(|document| document.active_turn().is_some_and(|turn| !turn.closed))
        .map_err(|error| error.to_string())?;
    // Durable usage ownership identifies the direct parent after runtime archival.
    // 可靠用量归属在运行时归档后识别直接父会话。
    let usage = child.lifetime_usage_snapshot()?;
    // The child ledger entry is the authoritative parent binding.
    // 子账本条目是父归属的权威绑定。
    let parent_id = usage
        .children_by_session_id
        .get(&task_id.to_string())
        .map(|child| child.parent_session_id.as_str())
        .ok_or_else(|| String::from("closed child has no verified parent ownership"))?
        .parse::<Uuid>()
        .map_err(|error| error.to_string())?;
    // Parent obligations can remain pending after source closure.
    // 来源闭合之后父义务仍可能尚未结算。
    let parent = registry.get(parent_id).map_err(|error| error.to_string())?;
    // Keep source history resident until all owned executions have terminal receipts.
    // 在全部所属执行具有终态回执之前保持来源历史驻留。
    let return_pending = parent
        .read(|document| {
            document.collaboration_state().child_tasks.values().any(|record| {
                record.child.session_id == task_id.to_string()
                    && record.state != vulcan_session_core::v1::collaboration::CollaborationChildTaskState::Terminal
            })
        })
        .map_err(|error| error.to_string())?;
    if !listener_pending && !turn_pending && !return_pending {
        registry
            .unregister_lifecycle_session(task_id)
            .map_err(|error| format!("failed to unload closed child V1 session: {error}"))?;
    }
    Ok(())
}

/// Seals one child only when the selected runtime entry still matches V1 parent and usage ownership.
/// 仅在选中运行时条目仍匹配 V1 父任务与用量归属时封闭子会话。
/// `registry` supplies canonical handles; `entry` is protected by the runtime close transaction; `stopped_at_ms` dates the durable stop.
/// `registry` 提供规范句柄；`entry` 受运行时关闭事务保护；`stopped_at_ms` 记录可靠停止时间。
/// Returns an explicit identity or persistence error before the runtime aborts and archives work.
/// 在运行时中止并归档工作之前返回明确的身份或持久化错误。
fn seal_exact_child_target(
    registry: &SessionRuntimeRegistry,
    entry: &SubagentRegistryEntry,
    stopped_at_ms: i64,
) -> Result<(), String> {
    if entry.status != SubagentStatus::Archived {
        return Err(String::from(
            "selected close entry is not an archived projection",
        ));
    }
    // Physical identities come from the frozen runtime selection.
    // 物理身份来自冻结的运行时选择。
    let task_id = entry.session_id.as_uuid();
    // The selection carries the direct parent, not merely the storage root.
    // 选择携带直接父会话，而非仅存储根。
    let parent_id = entry.parent_session_id.as_uuid();
    // The canonical parent must own the selected child reservation.
    // 规范父会话必须拥有选中子预留。
    let parent = registry
        .get(parent_id)
        .map_err(|error| format!("close parent V1 session is unavailable: {error}"))?;
    // The canonical child is sealed before runtime visibility is removed.
    // 在移除运行时可见性前封闭规范子会话。
    let child = registry
        .get(task_id)
        .map_err(|error| format!("close child V1 session is unavailable: {error}"))?;
    // Convert the physical ID into the domain's exact task identity.
    // 将物理 ID 转换为领域精确任务身份。
    let child_task_id = SubagentTaskId::try_new(task_id.to_string())
        .map_err(|error| format!("invalid close child task identity: {error}"))?;
    // Require durable ownership in addition to the runtime registry.
    // 除运行时注册表外还要求可靠归属。
    let parent_owned = parent
        .read(|document| {
            document
                .collaboration_state()
                .child_tasks
                .values()
                .any(|record| {
                    record.task_id == child_task_id
                        && record.child.session_id == task_id.to_string()
                        && record.child.task_id.as_ref() == Some(&child_task_id)
                })
        })
        .map_err(|error| error.to_string())?;
    // Independently verify the child storage owner against the parent's reservation.
    // 将子存储所有者与父预留独立核对。
    let usage = child.lifetime_usage_snapshot()?;
    // Optional parent ownership is rejected for a selected spawned child.
    // 对选中的派生子会话拒绝缺失父归属。
    let usage_parent = usage
        .children_by_session_id
        .get(&task_id.to_string())
        .map(|child| child.parent_session_id.as_str());
    if !parent_owned || usage_parent != Some(parent_id.to_string().as_str()) {
        return Err(String::from(
            "close child V1 ownership does not match the runtime entry",
        ));
    }
    child
        .set_collaboration_run_control_durable(SetRunControlInput {
            operation_id: CollaborationOperationId::try_new(format!(
                "close-agent:{task_id}:{}",
                Uuid::new_v4()
            ))
            .map_err(|error| error.to_string())?,
            state: CollaborationRunControl::Stopped,
            changed_at_ms: stopped_at_ms,
        })
        .map_err(|error| format!("failed to persist closed child admission: {error}"))?;
    Ok(())
}
