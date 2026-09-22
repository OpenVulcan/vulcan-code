//! Retained service ownership of independently admitted child context maintenance.
//! 独立准入子上下文维护的保留服务所有权。

use super::*;
use std::collections::BTreeSet;
use vulcan_agent_runtime::agent_runtime::task_runtime::{
    ChildAuxiliaryExecutionGuard, ChildAuxiliaryExecutionPurpose,
};
use vulcan_session_core::v1::context_maintenance::ContextMaintenanceExecution;
use vulcan_session_core::v1::document::ChildContextMaintenanceSource;

mod settlement;
mod tasks;
pub(super) use tasks::run;
pub(crate) use tasks::{ChildMaintenanceTask, stop_all, stop_tree};

/// Immutable storage ownership used to exclude root and project lifecycle transitions.
/// 用于排除根及项目生命周期转换的不可变存储归属。
#[derive(Clone)]
struct MaintenanceStorageScope {
    /// Indexed root that owns the embedded child's directory.
    /// 拥有嵌入子目录的索引根。
    root_session_id: Uuid,
    /// Exact project identity from the indexed root storage locator, never its display tag.
    /// 索引根存储定位器提供的精确项目身份，绝不使用展示分区标识。
    project_id: String,
}

impl MaintenanceStorageScope {
    /// Captures profile root and locator project ownership from canonical handles; returns explicit ownership errors.
    /// 从规范句柄捕获档案根及定位器项目归属；返回明确归属错误。
    /// `handle` owns the child and `registry` selects its root; registration retains both gates and later work retains the registered storage exclusion.
    /// `handle` 持有子任务，`registry` 选择其根；登记持有两者门，后续工作持有已登记存储互斥。
    fn capture(handle: &SessionHandle, registry: &SessionRuntimeRegistry) -> Result<Self, String> {
        // Read the child first, then release its document lock before inspecting the root.
        // 先读取子任务，再在检查根之前释放其文档锁。
        let (root_session_id, project_tag) = handle
            .read(|document| {
                let profile = document
                    .metadata
                    .child_execution_profile
                    .as_ref()
                    .ok_or_else(|| {
                        String::from("Child maintenance has no storage ownership profile.")
                    })?;
                Ok::<_, String>((
                    profile
                        .root_session_id
                        .parse::<Uuid>()
                        .map_err(|error| error.to_string())?,
                    document.metadata.project_tag.clone(),
                ))
            })
            .map_err(|error| error.to_string())??;
        // A missing or archived root cannot authorize writes into its embedded directory.
        // 缺失或已归档的根不能授权对其嵌入目录写入。
        let root = registry
            .get(root_session_id)
            .map_err(|error| error.to_string())?;
        root.read(|document| {
            if document.metadata.archived_at_ms.is_some()
                || document.metadata.child_execution_profile.is_some()
                || document.metadata.project_tag != project_tag
            {
                return Err(String::from(
                    "Child maintenance root storage is archived or changed.",
                ));
            }
            Ok(())
        })
        .map_err(|error| error.to_string())??;
        Ok(Self {
            root_session_id,
            project_id: root
                .storage_locator()
                .map_err(|error| error.to_string())?
                .project_id()
                .to_owned(),
        })
    }
}

/// Finds a registered or retained maintenance owner affected by exact session/project lifecycle targets.
/// 查找受精确会话或项目生命周期目标影响的已登记或保留维护所有者。
/// `state` must be locked by the caller; returns the conflicting child identity without loading documents.
/// 调用者必须持有 `state` 锁；返回冲突子身份，不加载文档。
pub(crate) fn lifecycle_conflict(
    state: &ServiceBridgeState,
    session_ids: &BTreeSet<Uuid>,
    project_scopes: &BTreeSet<crate::state::ProjectAdmissionScope>,
) -> Option<Uuid> {
    state
        .child_maintenance_tasks
        .iter()
        .map(|(id, task)| (id, &task.storage_scope))
        .chain(
            state
                .child_maintenance_owners
                .iter()
                .map(|(id, owner)| (id, &owner.storage_scope)),
        )
        .find_map(|(id, scope)| {
            (session_ids.contains(id)
                || session_ids.contains(&scope.root_session_id)
                || project_scopes.contains(&crate::state::ProjectAdmissionScope::Exact(
                    scope.project_id.clone(),
                )))
            .then_some(*id)
        })
}

/// Owns the exact auxiliary reservation independently from its caller and any Provider future.
/// 独立于调用者和供应商 Future 持有精确辅助预留。
pub(crate) struct ChildMaintenanceOwner {
    /// Storage ownership remains reserved even when failed cleanup removes the worker entry.
    /// 即使失败清理移除工作条目，存储归属仍保持预留。
    storage_scope: MaintenanceStorageScope,
    /// Immutable start record, never replaced by a business or question execution.
    /// 不可变起始记录，绝不被业务或答疑执行替换。
    execution: ContextMaintenanceExecution,
    /// Dispatch remains disabled until the admitted start has actually been published.
    /// 直到已准入起始实际发布后才启用分发。
    dispatch_ready: bool,
    /// Shared auxiliary exclusion retained on persistence or cancellation failures.
    /// 持久化或取消失败时保留的共享辅助互斥。
    _admission: ChildAuxiliaryExecutionGuard,
}

/// Publishes maintenance for `source` under the canonical lifecycle gate and transfers `admission` before returning.
/// 在规范生命周期门内发布 `source` 的维护，并在返回前转交 `admission`。
/// `state`, `registry`, and `handle` select one canonical owner; `now_ms` timestamps the new execution.
/// `state`、`registry` 与 `handle` 选择唯一规范所有者；`now_ms` 为新执行标记时间。
/// Returns its durable start, or an error with any admitted execution still excluded for exact cleanup.
/// 返回可靠起始，或返回错误并为已准入执行保持互斥以供精确清理。
pub(super) async fn prepare(
    state: &SharedServiceBridgeState,
    registry: &Arc<SessionRuntimeRegistry>,
    handle: &Arc<SessionHandle>,
    admission: ChildAuxiliaryExecutionGuard,
    source: ChildContextMaintenanceSource,
    now_ms: i64,
) -> Result<ContextMaintenanceExecution, String> {
    // The typed process guard must agree with the source and actual registered child before any mutation.
    // 在任何修改前，强类型进程守卫必须与来源及实际已登记子任务一致。
    let session_id = handle.session_id();
    if admission.task_id() != session_id
        || admission.purpose() != ChildAuxiliaryExecutionPurpose::ContextMaintenance
        || source.profile().task_id.as_str() != session_id.to_string()
    {
        return Err(String::from(
            "Child maintenance admission does not match its canonical source.",
        ));
    }
    // Preallocate the exact identity so failure cleanup never guesses which execution was started.
    // 预分配精确身份，使失败清理绝不猜测启动了哪个执行。
    let execution_id = CollaborationExecutionId::try_new(Uuid::new_v4().to_string())
        .map_err(|error| error.to_string())?;
    // These owners move into blocking publication together with the lifecycle lock.
    // 这些所有者与生命周期锁一起移动到阻塞发布中。
    let state = Arc::clone(state);
    // The canonical child handle survives cancellation of the async waiter.
    // 规范子句柄在异步等待者取消后仍存活。
    let handle = Arc::clone(handle);
    // Keep canonical lookup available inside the protected blocking callback.
    // 在受保护阻塞回调中保持规范查找可用。
    let scope_registry = registry.clone();
    lifecycle_write::with_canonical_handles(registry, vec![Arc::clone(&handle)], move || {
        // Retained repair must keep root/project exclusion after the asynchronous worker exits.
        // 异步工作任务退出后，保留修复必须继续排除根及项目生命周期操作。
        let storage_scope = MaintenanceStorageScope::capture(&handle, &scope_registry)?;
        // Reject duplicate service ownership before creating any new domain facts.
        // 在创建任何新领域事实前拒绝重复服务所有权。
        if state
            .lock()
            .map_err(|error| error.to_string())?
            .child_maintenance_owners
            .contains_key(&session_id)
        {
            return Err(String::from(
                "Child maintenance already has a retained service owner.",
            ));
        }
        // Capture only the actual Core receipt, even if the subsequent storage publication fails.
        // 即使后续存储发布失败，也只捕获实际 Core 回执。
        let mut started = None;
        // A new auxiliary turn uses new-inference admission and must be rejected during shutdown.
        // 新辅助回合使用新推理准入，必须在关闭过程中被拒绝。
        let publication = handle.mutate_durable_if_changed(
            SessionMutationAdmission::RootTurn,
            SessionFlushReason::MetadataChanged,
            |document| {
                let result =
                    document.begin_child_context_maintenance(&source, execution_id, now_ms)?;
                started = Some(match &result {
                    IdempotentMutation::Committed(record)
                    | IdempotentMutation::Idempotent(record) => record.clone(),
                });
                Ok(session_change(result))
            },
        );
        // No successful domain mutation means there is no newly admitted owner to retain.
        // 没有成功领域修改意味着没有需要保留的新准入所有者。
        let Some(execution) = started else {
            return Err(match publication {
                Err(error) => format!("Child maintenance admission failed: {error}"),
                Ok(_) => String::from("Child maintenance admission produced no execution."),
            });
        };
        // Transfer before inspecting fallible publication; errors cannot drop the only exclusion guard.
        // 在检查可能失败的发布之前转交；错误不能丢弃唯一互斥守卫。
        state
            .lock()
            .map_err(|error| error.to_string())?
            .child_maintenance_owners
            .insert(
                session_id,
                ChildMaintenanceOwner {
                    storage_scope,
                    execution: execution.clone(),
                    dispatch_ready: false,
                    _admission: admission,
                },
            );
        publication.map_err(|error| {
            format!("Child maintenance publication failed; admission retained: {error}")
        })?;
        handle
            .read(|document| {
                if document.memory_revision() != document.durable_revision()
                    || document.metadata.context_maintenance.as_ref() != Some(&execution)
                {
                    Err(String::from(
                        "Child maintenance start is not the exact durable execution.",
                    ))
                } else {
                    Ok(())
                }
            })
            .map_err(|error| error.to_string())??;
        // Activate only the exact transferred owner after publication is proven.
        // 在证明发布后，仅激活精确已转交所有者。
        let mut service = state.lock().map_err(|error| error.to_string())?;
        let owner = service
            .child_maintenance_owners
            .get_mut(&session_id)
            .filter(|owner| owner.execution == execution)
            .ok_or_else(|| {
                String::from("Child maintenance ownership changed during publication.")
            })?;
        owner.dispatch_ready = true;
        Ok(execution)
    })
    .await
}

/// Requires the exact published `execution` to retain dispatch admission in `state` for `session_id`.
/// 要求 `session_id` 对应精确已发布 `execution` 在 `state` 中保持分发准入。
/// Returns an explicit error for missing, unready or replaced ownership; it does not grant durable document authority.
/// 缺失、未就绪或被替换的归属返回明确错误；本函数不授予可靠文档权威。
pub(super) fn require_owner(
    state: &SharedServiceBridgeState,
    session_id: Uuid,
    execution: &ContextMaintenanceExecution,
) -> Result<(), String> {
    if state
        .lock()
        .map_err(|error| error.to_string())?
        .child_maintenance_owners
        .get(&session_id)
        .is_some_and(|owner| owner.dispatch_ready && &owner.execution == execution)
    {
        Ok(())
    } else {
        Err(String::from(
            "Child maintenance has no exact published service owner.",
        ))
    }
}

/// Releases `execution_id` only after its own terminal and every current mutation are durably published.
/// 仅在 `execution_id` 自身终态及所有当前修改可靠发布后释放。
/// Callers retain canonical lifecycle ownership; errors preserve the reservation for retry.
/// 调用者保持规范生命周期所有权；错误保留预留以供重试。
pub(super) fn release_owner(
    state: &SharedServiceBridgeState,
    handle: &SessionHandle,
    execution_id: &CollaborationExecutionId,
) -> Result<(), String> {
    // Copy the immutable start without holding service state across document reads.
    // 复制不可变起始，避免在文档读取期间持有服务状态锁。
    let expected = {
        let service = state.lock().map_err(|error| error.to_string())?;
        let Some(owner) = service.child_maintenance_owners.get(&handle.session_id()) else {
            return Ok(());
        };
        if &owner.execution.execution_id != execution_id {
            return Err(String::from(
                "Child maintenance release belongs to a different execution.",
            ));
        }
        owner.execution.clone()
    };
    handle
        .read(|document| {
            let mut current = document
                .metadata
                .context_maintenance
                .clone()
                .ok_or_else(|| String::from("Child maintenance release has no domain record."))?;
            if current.terminal.take().is_none()
                || current != expected
                || document.memory_revision() != document.durable_revision()
                || document.active_api().is_some()
                || document
                    .active_turn()
                    .is_some_and(|turn| !turn.closed && turn.turn_id == expected.turn_id)
            {
                return Err(String::from(
                    "Child maintenance release requires its exact durable terminal.",
                ));
            }
            Ok(())
        })
        .map_err(|error| error.to_string())??;
    // A stale cleanup can never remove a replacement owner.
    // 过期清理绝不能移除替代所有者。
    let mut service = state.lock().map_err(|error| error.to_string())?;
    if service
        .child_maintenance_owners
        .get(&handle.session_id())
        .is_some_and(|owner| owner.execution == expected)
    {
        service
            .child_maintenance_owners
            .remove(&handle.session_id());
    }
    Ok(())
}

#[cfg(test)]
mod tests;
