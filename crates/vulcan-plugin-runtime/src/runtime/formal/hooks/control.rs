//! Hook cleanup observes original core identities and resumes after executor interruption.
//! Hook 清理观测原核心身份，并在执行器中断后恢复。

use super::*;

impl ManagedFormalRuntime {
    /// Request closure of every registered Hook, including unfinished preparation; retain all ownership.
    /// 请求关闭每个已注册 Hook，包含未完成准备；保留全部所有权。
    pub(in crate::runtime::formal) fn cancel_hooks(&self) -> Result<(), VulcanPluginRuntimeError> {
        for runtime in self
            .hooks
            .lock()
            .map_err(|_| hook_error("Hook registry lock is poisoned."))?
            .values()
        {
            runtime.closing.store(true, Ordering::Release);
        }
        Ok(())
    }

    /// Return true only when all Hook preparation and physical cleanup have finished.
    /// 仅在全部 Hook 准备及物理清理完成后返回真值。
    pub(in crate::runtime::formal) fn hooks_drained(
        &self,
    ) -> Result<bool, VulcanPluginRuntimeError> {
        for runtime in self
            .hooks
            .lock()
            .map_err(|_| hook_error("Hook registry lock is poisoned."))?
            .values()
        {
            // Keep the exact lifecycle evidence within this synchronous lock scope.
            // 在当前同步锁作用域内保留精确生命周期证据。
            let state = runtime.lock()?;
            if !state.retired || state.preparing {
                return Ok(false);
            }
        }
        Ok(true)
    }
}

impl VulcanPluginRuntimeManager {
    /// Restart cleanup for owner/runtime without recreating a session or replaying business work.
    /// 为 owner/runtime 重启清理，不重建会话或重放业务工作。
    pub(super) fn start_hook_monitor(
        &self,
        owner: &Arc<ManagedFormalRuntime>,
        runtime: &Arc<HookRuntime>,
    ) -> Result<(), VulcanPluginRuntimeError> {
        // Require a current Tokio executor before starting supervision.
        // 启动监督前要求存在当前 Tokio 执行器。
        let executor = tokio::runtime::Handle::try_current()
            .map_err(|_| hook_error("Hook monitoring requires a Tokio executor."))?;
        {
            // Keep the exact lifecycle evidence within this synchronous lock scope.
            // 在当前同步锁作用域内保留精确生命周期证据。
            let mut state = runtime.lock()?;
            if state.monitored || state.retired {
                return Ok(());
            }
            state.monitored = true;
            state.cleanup_error = None;
        }
        // Never retain the manager across an idle sleep, which would prevent manager destruction.
        // 绝不跨空闲等待保留管理器，以免阻止管理器析构。
        let weak = Arc::downgrade(&self.state);
        // Retain this manager's sole formal runtime parent.
        // 保留此管理器唯一正式运行时父级。
        let owner = Arc::clone(owner);
        // Retain the original Hook instance across preparation and observation.
        // 跨准备及观测保留原 Hook 实例。
        let runtime = Arc::clone(runtime);
        // Preserve supervision interruption evidence if this task is dropped.
        // 此任务被丢弃时保留监督中断证据。
        let monitor = HookMonitor(Arc::clone(&runtime));
        executor.spawn(async move {
            // Keep the supervision guard alive for exactly this monitoring task.
            // 仅在此监控任务期间保持监督守卫存活。
            let _monitor = monitor;
            loop {
                // Release strong manager ownership before the next asynchronous wait.
                // 在下一次异步等待之前释放管理器强所有权。
                let done = {
                    let Some(state) = weak.upgrade() else {
                        let _ = owner.host.runtime().request_close();
                        return;
                    };
                    // Keep the manager alive for the bounded preparation or synchronous monitor step.
                    // 为有界准备或同步监控步骤保持管理器存活。
                    let manager = VulcanPluginRuntimeManager { state };
                    match manager.advance_hook(&owner, &runtime) {
                        Ok(done) => done,
                        Err(error) => {
                            if let Ok(mut state) = runtime.lock() {
                                state.cleanup_error = Some(error.to_string());
                            }
                            let _ = manager
                                .record_cleanup_failure(&runtime.plugin.id, error.to_string());
                            return;
                        }
                    }
                };
                if done {
                    return;
                }
                tokio::time::sleep(OBSERVATION_INTERVAL).await;
            }
        });
        Ok(())
    }

    /// Advance runtime's physical cleanup on owner; return true only after every recorded resource retires.
    /// 在 owner 上推进 runtime 的物理清理；仅在每项记录资源退役后返回真值。
    fn advance_hook(
        &self,
        owner: &ManagedFormalRuntime,
        runtime: &HookRuntime,
    ) -> Result<bool, VulcanPluginRuntimeError> {
        // Order this state transition against catalog generation publication.
        // 使当前状态转换与目录代次发布保持顺序。
        let _publication = self
            .state
            .session_registration_gate
            .lock()
            .map_err(|_| hook_error("Plugin publication gate is poisoned."))?;
        // Keep the exact lifecycle evidence within this synchronous lock scope.
        // 在当前同步锁作用域内保留精确生命周期证据。
        let mut state = runtime.lock()?;
        if state.retired {
            return Ok(true);
        }
        if state.preparing {
            return Ok(false);
        }
        // Reclaim completed acknowledged calls while the persistent instance remains open.
        // 持久实例仍开启时回收已完成且已确认的调用。
        let mut consumed = Vec::new();
        for (id, operation) in &state.operations {
            if operation.acknowledged.load(Ordering::Acquire)
                && operation
                    .handle
                    .snapshot()
                    .map_err(core_error)?
                    .phase
                    .is_terminal()
            {
                consumed.push(id.clone());
            }
        }
        for id in consumed {
            owner
                .host
                .runtime()
                .forget_operation(&id)
                .map_err(core_error)?;
            state.operations.remove(&id);
        }
        owner.host.reap_workspace_requests().map_err(core_error)?;
        if let Some(id) = state.session.clone()
            && !state.session_forgotten
        {
            // Query the original session's physical state and finalization identity.
            // 查询原会话物理状态及关闭身份。
            let core = owner.host.runtime().session(&id).map_err(core_error)?;
            if matches!(
                core.phase,
                EmbeddedSessionPhase::Closing | EmbeddedSessionPhase::Closed
            ) {
                runtime.closing.store(true, Ordering::Release);
            }
            if !runtime.closing.load(Ordering::Acquire) {
                return Ok(false);
            }
            if core.phase != EmbeddedSessionPhase::Closed {
                owner
                    .host
                    .runtime()
                    .close_session(&id)
                    .map_err(core_error)?;
                return Ok(false);
            }
            if !state.operations.is_empty() {
                return Ok(false);
            }
            if let Some(closing) = core.finalization_operation {
                state.finalization = Some(
                    owner
                        .host
                        .runtime()
                        .operation(&closing)
                        .map_err(core_error)?
                        .snapshot()
                        .map_err(core_error)?,
                );
            }
            owner
                .host
                .runtime()
                .forget_session(&id)
                .map_err(core_error)?;
            state.session_forgotten = true;
        } else if !runtime.closing.load(Ordering::Acquire) {
            return Ok(false);
        }
        // Each acknowledgement has its own checkpoint so a failed later step is safely retryable.
        // 每次确认拥有独立检查点，使后续步骤失败后可以安全重试。
        if !state.initialization_forgotten {
            if let Some(operation) = &state.initialization {
                owner
                    .host
                    .runtime()
                    .forget_operation(operation.id())
                    .map_err(core_error)?;
            }
            state.initialization_forgotten = true;
        }
        if !state.finalization_forgotten {
            if let Some(snapshot) = &state.finalization {
                owner
                    .host
                    .runtime()
                    .forget_operation(&snapshot.operation_id)
                    .map_err(core_error)?;
            }
            state.finalization_forgotten = true;
        }
        if let Some(pool) = &state.pool {
            owner.host.runtime().close_pool(pool).map_err(core_error)?;
            match owner.host.runtime().forget_pool(pool) {
                Ok(()) => {}
                Err(error) if error.code == EmbeddedErrorCode::Busy => return Ok(false),
                Err(error) => return Err(core_error(error)),
            }
        }
        state.retired = true;
        Ok(true)
    }

    /// Await runtime retirement until expires; interruption and cleanup failures remain explicit.
    /// 等待 runtime 退役至 expires；中断及清理失败保持明确。
    pub(super) async fn wait_hook_retired(
        &self,
        runtime: &HookRuntime,
        expires: Instant,
    ) -> Result<(), VulcanPluginRuntimeError> {
        loop {
            {
                // Keep the exact lifecycle evidence within this synchronous lock scope.
                // 在当前同步锁作用域内保留精确生命周期证据。
                let state = runtime.lock()?;
                if state.retired {
                    return Ok(());
                }
                if let Some(error) = &state.cleanup_error {
                    return Err(VulcanPluginRuntimeError::new(error.clone()));
                }
                if !state.monitored {
                    return Err(hook_error("Hook cleanup supervision was interrupted."));
                }
            }
            remaining(expires).map_err(|_| {
                hook_error("Hook cleanup timed out; original ownership remains retained.")
            })?;
            tokio::time::sleep(OBSERVATION_INTERVAL).await;
        }
    }

    /// Drain plugin_id's original Hook; return counts and independent shutdown diagnostics.
    /// 排空 plugin_id 的原 Hook；返回计数及独立关闭诊断。
    pub(crate) async fn drain_formal_hook(
        &self,
        plugin_id: &str,
    ) -> Result<VulcanPluginDrainReport, VulcanPluginRuntimeError> {
        // Accumulate physical-close counts independently from shutdown failures.
        // 独立于关闭失败累计物理关闭计数。
        let mut report = VulcanPluginDrainReport::default();
        let Some(owner) = self.formal_runtime_owner()? else {
            return Ok(report);
        };
        // Freeze one absolute deadline for this complete cleanup attempt.
        // 为本次完整清理尝试冻结一个绝对截止时间。
        let expires = deadline(owner.settings.shutdown_timeout_ms)?;
        // Serialize only this plugin's Hook initialization and explicit drain.
        // 仅串行化此插件的 Hook 初始化及显式排空。
        let gate =
            self.persistent_runtime_gate(plugin_id, VulcanPluginPersistentRuntimeKind::Hook)?;
        // Stop retained preparation before waiting for its initialization gate.
        // 等待初始化门前停止已保留准备。
        {
            // Order this state transition against catalog generation publication.
            // 使当前状态转换与目录代次发布保持顺序。
            let _publication = self
                .state
                .session_registration_gate
                .lock()
                .map_err(|_| hook_error("Plugin publication gate is poisoned."))?;
            if let Some(runtime) = owner
                .hooks
                .lock()
                .map_err(|_| hook_error("Hook registry lock is poisoned."))?
                .get(plugin_id)
            {
                runtime.closing.store(true, Ordering::Release);
            }
        }
        // Keep initialization ordering within the original finite admission budget.
        // 在原有限入场预算内保持初始化顺序。
        let _initialization = tokio::time::timeout(remaining(expires)?, gate.lock_owned())
            .await
            .map_err(|_| hook_error("Hook drain admission timed out."))?;
        // Retain the original Hook instance across preparation and observation.
        // 跨准备及观测保留原 Hook 实例。
        let runtime = {
            // Order this state transition against catalog generation publication.
            // 使当前状态转换与目录代次发布保持顺序。
            let _publication = self
                .state
                .session_registration_gate
                .lock()
                .map_err(|_| hook_error("Plugin publication gate is poisoned."))?;
            // Retain the original Hook instance across preparation and observation.
            // 跨准备及观测保留原 Hook 实例。
            let runtime = owner
                .hooks
                .lock()
                .map_err(|_| hook_error("Hook registry lock is poisoned."))?
                .get(plugin_id)
                .cloned();
            if let Some(runtime) = &runtime {
                runtime.closing.store(true, Ordering::Release);
            }
            runtime
        };
        let Some(runtime) = runtime else {
            return Ok(report);
        };
        self.start_hook_monitor(&owner, &runtime)?;
        match self.wait_hook_retired(&runtime, expires).await {
            Ok(()) => {
                // Keep the exact lifecycle evidence within this synchronous lock scope.
                // 在当前同步锁作用域内保留精确生命周期证据。
                let state = runtime.lock()?;
                if let Some(snapshot) = &state.finalization
                    && let Err(error) = project_closing(snapshot)
                {
                    report
                        .failures
                        .push(format!("Plugin Hook shutdown failed: {error}"));
                }
                if let Some(Err(error)) = &state.prepared {
                    report
                        .failures
                        .push(format!("Plugin Hook preparation failed: {error}"));
                }
                drop(state);
                owner
                    .hooks
                    .lock()
                    .map_err(|_| hook_error("Hook registry lock is poisoned."))?
                    .retain(|id, current| id != plugin_id || !Arc::ptr_eq(current, &runtime));
                report.closed_hook_runtimes = 1;
            }
            Err(error) => {
                self.record_cleanup_failure(plugin_id, error.to_string())?;
                report.failures.push(error.to_string());
            }
        }
        Ok(report)
    }

    /// Report retained Hook ownership for plugin_id before clearing cleanup diagnostics.
    /// 清除清理诊断前报告 plugin_id 的保留 Hook 所有权。
    pub(crate) fn has_formal_hook(
        &self,
        plugin_id: &str,
    ) -> Result<bool, VulcanPluginRuntimeError> {
        let Some(owner) = self.formal_runtime_owner()? else {
            return Ok(false);
        };
        Ok(owner
            .hooks
            .lock()
            .map_err(|_| hook_error("Hook registry lock is poisoned."))?
            .contains_key(plugin_id))
    }
}
