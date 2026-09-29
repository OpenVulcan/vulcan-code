//! Public policy controls share the capacity publication gate and the single settings store.
//! 公开策略控制共用容量发布门与单一设置存储。

use super::*;

impl VulcanPluginRuntimeManager {
    /// Observe this native owner's effective automatic policy and retained receipt without changing either.
    /// 观测此原生所有者的生效自动策略及保留回执，不改变任一对象。
    pub fn prewarm_policy_status(
        &self,
    ) -> Result<VulcanPluginPrewarmPolicyStatus, VulcanPluginRuntimeError> {
        self.with_prewarm_policy(|store, runtime_id, _| Ok(store.prewarm_status(runtime_id)))
    }

    /// Begin the exact explicit request; return its durable checkpoint without preparing or starting a VM.
    /// 开始精确明确请求；返回持久检查点，不准备或启动 VM。
    pub fn begin_prewarm_policy_update(
        &self,
        request: VulcanPluginPrewarmPolicyUpdate,
    ) -> Result<VulcanPluginPrewarmPolicyRecord, VulcanPluginRuntimeError> {
        self.with_prewarm_policy(|store, runtime_id, root| {
            store.begin_prewarm(root, runtime_id, request)
        })
    }

    /// Resume update_id's original publication only; another or missing receipt is an explicit error.
    /// 仅恢复 update_id 的原发布；其他或缺失回执明确报错。
    pub fn resume_prewarm_policy_update(
        &self,
        update_id: &str,
    ) -> Result<VulcanPluginPrewarmPolicyRecord, VulcanPluginRuntimeError> {
        self.with_prewarm_policy(|store, runtime_id, root| {
            require_prewarm_update(store, update_id)?;
            store.resume_prewarm(root, runtime_id)
        })
    }

    /// Cancel update_id before final publication; an uncertain final replacement must first be reconciled.
    /// 在最终发布前取消 update_id；最终替换不确定时必须先对账。
    pub fn cancel_prewarm_policy_update(
        &self,
        update_id: &str,
    ) -> Result<VulcanPluginPrewarmPolicyRecord, VulcanPluginRuntimeError> {
        self.with_prewarm_policy(|store, _, root| {
            require_prewarm_update(store, update_id)?;
            store.cancel_prewarm(root)
        })
    }

    /// Forget exactly update_id after durable terminal acknowledgement; this does not cancel execution targets.
    /// 持久终态确认后精确遗忘 update_id；这不取消执行目标。
    pub fn forget_prewarm_policy_update(
        &self,
        update_id: &str,
    ) -> Result<(), VulcanPluginRuntimeError> {
        self.with_prewarm_policy(|store, _, root| {
            require_prewarm_update(store, update_id)?;
            store.forget_prewarm(root)
        })
    }

    /// Run control against the existing owner under publication then settings locks; propagate original errors.
    /// 按发布门、设置锁顺序在既有所有者上执行 control；传播原错误。
    /// The closure may publish settings but must never dispatch Lua while these metadata locks are held.
    /// 闭包可发布设置，但持有这些元数据锁时绝不能派发 Lua。
    fn with_prewarm_policy<T>(
        &self,
        control: impl FnOnce(&mut PolicyStore, &str, &std::path::Path) -> Result<T, String>,
    ) -> Result<T, VulcanPluginRuntimeError> {
        // Match capacity updates and package publication so no policy write can erase another live intent.
        // 与容量更新及包发布保持一致，防止策略写入抹除其他活动意图。
        let _publication = self
            .state
            .session_registration_gate
            .lock()
            .map_err(|_| capacity_lock_error())?;
        let owner = self
            .formal_runtime_owner()?
            .ok_or_else(|| VulcanPluginRuntimeError::new("Formal runtime is not attached."))?;
        let mut registry = owner.capacities.lock().map_err(|_| capacity_lock_error())?;
        control(
            &mut registry.policy,
            owner.host.runtime().id(),
            self.plugin_state_root(),
        )
        .map_err(VulcanPluginRuntimeError::new)
    }
}

/// Require an exact caller-owned update_id before any recovery or removal and return no substituted receipt.
/// 任何恢复或移除前要求精确调用方 update_id，绝不返回替代回执。
fn require_prewarm_update(store: &PolicyStore, update_id: &str) -> Result<(), String> {
    if store
        .prewarm_update
        .as_ref()
        .is_some_and(|record| record.request.update_id == update_id)
    {
        Ok(())
    } else {
        Err("Original prewarm policy update is not retained.".into())
    }
}
