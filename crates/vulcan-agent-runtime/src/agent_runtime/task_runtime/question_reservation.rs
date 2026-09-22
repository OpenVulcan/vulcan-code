use super::*;

/// Exact purpose retained by an auxiliary reservation, independent from its shared exclusion table.
/// 辅助预留保留的精确用途，独立于共享互斥表。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChildAuxiliaryExecutionPurpose {
    /// Independently authorized question handling may not assume maintenance rights.
    /// 独立授权的答疑处理不能据此取得维护权限。
    Question,
    /// Completed-business maintenance may not answer or settle a question execution.
    /// 完成后业务维护不能回答或结算答疑执行。
    ContextMaintenance,
}

/// Retains exclusive child execution admission without changing its business record.
/// 保持子执行准入互斥，不改变其业务记录。
/// The host retains this guard through preparation, retries, and terminal cleanup.
/// 宿主在准备、重试及终态清理期间持有此守卫。
pub struct ChildAuxiliaryExecutionGuard {
    /// Runtime that owns the auxiliary admission table.
    /// 持有辅助准入表的运行时。
    runtime: Arc<TaskRuntime>,
    /// Stable child session whose business admission is excluded.
    /// 被排除业务准入的稳定子会话。
    task_id: SessionId,
    /// Exact reservation identity protected against stale cleanup.
    /// 防止过期清理的精确预留身份。
    reservation_id: Uuid,
    /// Purpose authorized at reservation creation; sharing a slot does not share capabilities.
    /// 创建预留时授权的用途；共享槽位不表示共享能力。
    purpose: ChildAuxiliaryExecutionPurpose,
}

impl ChildAuxiliaryExecutionGuard {
    /// Returns the exact child protected by this live admission guard.
    /// 返回本活动准入守卫保护的精确子任务。
    pub fn task_id(&self) -> SessionId {
        self.task_id
    }

    /// Returns the immutable reservation purpose for host ownership transfer checks.
    /// 返回不可变预留用途，供宿主归属转交检查。
    pub fn purpose(&self) -> ChildAuxiliaryExecutionPurpose {
        self.purpose
    }
}

impl Drop for ChildAuxiliaryExecutionGuard {
    /// Releases only this reservation; reports lock errors and retains fail-closed exclusion.
    /// 仅释放本次预留；报告锁错误并保留拒绝准入的互斥状态。
    fn drop(&mut self) {
        match self.runtime.auxiliary_execution_reservations.write() {
            Ok(mut reservations) => {
                if reservations.get(&self.task_id) == Some(&self.reservation_id) {
                    reservations.remove(&self.task_id);
                }
            }
            Err(_) => {
                eprintln!(
                    "[task-runtime] child auxiliary execution reservation release failed: registry lock poisoned"
                );
            }
        }
    }
}

impl TaskRuntime {
    /// Reserves auxiliary execution for an already authorized child session.
    /// 为已经过授权的子会话预留辅助执行。
    /// `task_id` is the canonical child ID; the host must separately validate durable control.
    /// `task_id` 是规范子标识；宿主必须另行校验可靠控制状态。
    /// Returns a guard, None for occupied execution, or an explicit state/lock error.
    /// 返回守卫、表示执行占用的 None，或明确的状态与锁错误。
    /// Cold children may have no resident record; this method does not create one.
    /// 冷子会话可以没有常驻记录；本方法不会创建业务记录。
    pub fn try_reserve_child_question_execution(
        self: &Arc<Self>,
        task_id: SessionId,
    ) -> Result<Option<ChildAuxiliaryExecutionGuard>, String> {
        self.try_reserve_child_auxiliary_execution(task_id, None)
    }

    /// Reserves maintenance only for the exact completed resident business execution.
    /// 仅为精确已完成的常驻业务执行预留维护。
    /// `task_id` identifies the child and `business_execution_id` prevents stale background work from claiming a successor.
    /// `task_id` 标识子会话，`business_execution_id` 防止旧后台工作认领后继执行。
    /// Returns the shared exclusion guard, None while occupied, or an explicit stale/status error; durable host admission remains required.
    /// 返回共享互斥守卫、占用时的 None 或明确过期及状态错误；仍须可靠宿主准入。
    pub fn try_reserve_child_context_maintenance(
        self: &Arc<Self>,
        task_id: SessionId,
        business_execution_id: &str,
    ) -> Result<Option<ChildAuxiliaryExecutionGuard>, String> {
        self.try_reserve_child_auxiliary_execution(task_id, Some(business_execution_id))
    }

    /// Serializes question and maintenance admission with business preparation and preserves all business fields.
    /// 将答疑及维护准入与业务准备串行化，并保留全部业务字段。
    /// An execution ID selects completed maintenance; absence selects independently host-authorized question execution.
    /// 执行 ID 选择完成后维护；不存在则选择由宿主独立授权的答疑执行。
    /// Returns one shared exclusion owner or an explicit rejection without changing the record.
    /// 返回一个共享互斥所有者或明确拒绝，不修改记录。
    fn try_reserve_child_auxiliary_execution(
        self: &Arc<Self>,
        task_id: SessionId,
        maintenance_execution_id: Option<&str>,
    ) -> Result<Option<ChildAuxiliaryExecutionGuard>, String> {
        // Share the business preparation/claim lock to cover the pre-provider gap.
        // 共享业务准备与认领锁，覆盖 Provider 启动前的空隙。
        let transaction_lock = self.task_execution_state_lock(task_id)?;
        // Keep all admission checks and insertion in one task transaction.
        // 将全部准入检查与插入保持在同一个任务事务内。
        let _transaction = transaction_lock.lock().map_err(|_| {
            String::from("The task execution transaction is locked and cannot reserve child auxiliary execution.")
        })?;
        if self.task_has_active_or_starting_run(task_id)? {
            return Ok(None);
        }
        {
            // Inspect preparation state without changing the last business result.
            // 检查准备状态，不改变上次业务结果。
            let sessions = self.sessions.read().map_err(|_| {
                String::from("The task session registry is locked and cannot admit child auxiliary execution.")
            })?;
            if let Some(record) = sessions.get(&task_id) {
                // Parse the canonical status contract instead of guessing terminal text.
                // 解析规范状态契约，不猜测终态文本。
                let status = task_session_status_from_record_status(&record.last_status)?;
                if let Some(execution_id) = maintenance_execution_id {
                    if record.execution_id.as_deref() != Some(execution_id) {
                        return Err(format!(
                            "Task `{task_id}` maintenance belongs to a superseded business execution."
                        ));
                    }
                    if !status.is_active() && status != TaskSessionStatus::Completed {
                        return Err(format!(
                            "Task `{task_id}` maintenance requires a successfully completed business execution."
                        ));
                    }
                }
                if status.is_active() {
                    return Ok(None);
                }
                if matches!(
                    status,
                    TaskSessionStatus::Stopped | TaskSessionStatus::Archived
                ) {
                    return Err(format!(
                        "Task `{task_id}` is stopped or archived and cannot start auxiliary execution."
                    ));
                }
            } else if maintenance_execution_id.is_some() {
                return Err(format!(
                    "Task `{task_id}` has no completed resident execution for maintenance."
                ));
            }
        }
        // Auxiliary ownership is independent from the business active-run registry.
        // 辅助所有权独立于业务活动运行注册表。
        let mut reservations = self.auxiliary_execution_reservations.write().map_err(|_| {
            String::from(
                "The child auxiliary execution registry is locked and cannot reserve execution.",
            )
        })?;
        if reservations.contains_key(&task_id) {
            return Ok(None);
        }
        // A unique owner prevents late cleanup from releasing another execution.
        // 唯一所有者防止延迟清理释放其他执行。
        let reservation_id = Uuid::new_v4();
        reservations.insert(task_id, reservation_id);
        Ok(Some(ChildAuxiliaryExecutionGuard {
            runtime: Arc::clone(self),
            task_id,
            reservation_id,
            purpose: if maintenance_execution_id.is_some() {
                ChildAuxiliaryExecutionPurpose::ContextMaintenance
            } else {
                ChildAuxiliaryExecutionPurpose::Question
            },
        }))
    }

    /// Rejects business admission while auxiliary execution owns this child.
    /// 当辅助执行持有此子会话时拒绝业务准入。
    /// `task_id` must be protected by its execution transaction lock at the caller.
    /// 调用者必须持有 `task_id` 对应的执行事务锁。
    /// Returns success only when no auxiliary owner exists, or a state/lock error.
    /// 仅在没有辅助所有者时返回成功，否则返回状态与锁错误。
    pub(super) fn ensure_no_auxiliary_execution_with_transaction_lock_held(
        &self,
        task_id: SessionId,
    ) -> Result<(), String> {
        if self.auxiliary_execution_reservations.read().map_err(|_| {
            String::from("The child auxiliary execution registry is locked and cannot admit business execution.")
        })?.contains_key(&task_id) {
            return Err(format!("Task `{task_id}` is handling auxiliary execution; retry business execution after it finishes."));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
