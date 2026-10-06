//! Persistent instance cleanup observes original core identities and resumes after executor interruption.
//!持久清理观测原核心身份，并在执行器中断后恢复。

use super::*;

impl ManagedFormalRuntime {
    /// Request closure of every registered persistent instance, including unfinished preparation; retain all ownership.
    /// 请求关闭每个已注册 持久实例，包含未完成准备；保留全部所有权。
    pub(in crate::runtime::formal) fn cancel_instances(
        &self,
    ) -> Result<(), VulcanPluginRuntimeError> {
        for kind in [
            VulcanPluginPersistentRuntimeKind::Hook,
            VulcanPluginPersistentRuntimeKind::UiAction,
        ] {
            for runtime in self
                .instances(kind)
                .lock()
                .map_err(|_| instance_error("Persistent registry lock is poisoned."))?
                .values()
            {
                runtime.closing.store(true, Ordering::Release);
            }
        }
        Ok(())
    }

    /// Return true only when all persistent preparation and physical cleanup have finished.
    /// 仅在全部持久准备及物理清理完成后返回真值。
    pub(in crate::runtime::formal) fn instances_drained(
        &self,
    ) -> Result<bool, VulcanPluginRuntimeError> {
        for kind in [
            VulcanPluginPersistentRuntimeKind::Hook,
            VulcanPluginPersistentRuntimeKind::UiAction,
        ] {
            for runtime in self
                .instances(kind)
                .lock()
                .map_err(|_| instance_error("Persistent registry lock is poisoned."))?
                .values()
            {
                // Keep the exact lifecycle evidence within this synchronous lock scope.
                // 在当前同步锁作用域内保留精确生命周期证据。
                let state = runtime.lock()?;
                if !state.retired || state.preparing {
                    return Ok(false);
                }
            }
        }
        Ok(true)
    }
}

impl VulcanPluginRuntimeManager {
    /// Restart cleanup for owner/runtime without recreating a session or replaying business work.
    /// 为 owner/runtime 重启清理，不重建会话或重放业务工作。
    pub(super) fn start_instance_monitor(
        &self,
        owner: &Arc<ManagedFormalRuntime>,
        runtime: &Arc<PersistentRuntime>,
    ) -> Result<(), VulcanPluginRuntimeError> {
        // Require a current Tokio executor before starting supervision.
        // 启动监督前要求存在当前 Tokio 执行器。
        let executor = self.supervisor_handle()?;
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
        // Retain the original persistent instance across preparation and observation.
        // 跨准备及观测保留原持久实例。
        let runtime = Arc::clone(runtime);
        // Preserve supervision interruption evidence if this task is dropped.
        // 此任务被丢弃时保留监督中断证据。
        let monitor = PersistentMonitor(Arc::clone(&runtime));
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
                    match manager.advance_instance(&owner, &runtime) {
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

    /// Reclaim original terminal operation evidence acknowledged by its observer under the caller's state lock.
    /// 在调用方的 state 锁下回收已由其观察者确认的原终态操作证据。
    /// owner retains the exact Core parent; state is this persistent instance's exclusive evidence record.
    /// owner 保留精确 Core 父级；state 为此持久实例的独占证据记录。
    /// Return success after forgetting eligible records, or propagate the original snapshot or forgetting error.
    /// 遗忘符合条件的记录后返回成功，否则传递原快照或遗忘错误。
    pub(super) fn reclaim_acknowledged_persistent_operations(
        &self,
        owner: &ManagedFormalRuntime,
        state: &mut PersistentState,
    ) -> Result<(), VulcanPluginRuntimeError> {
        // Acknowledgement never substitutes for actual termination or releases still-owned work.
        // 确认绝不替代实际终止，也不释放仍被拥有的工作。
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
        Ok(())
    }

    /// Advance runtime's physical cleanup on owner; return true only after every recorded resource retires.
    /// 在 owner 上推进 runtime 的物理清理；仅在每项记录资源退役后返回真值。
    fn advance_instance(
        &self,
        owner: &ManagedFormalRuntime,
        runtime: &PersistentRuntime,
    ) -> Result<bool, VulcanPluginRuntimeError> {
        // Order this state transition against catalog generation publication.
        // 使当前状态转换与目录代次发布保持顺序。
        let _publication = self
            .state
            .session_registration_gate
            .lock()
            .map_err(|_| instance_error("Plugin publication gate is poisoned."))?;
        // Keep the exact lifecycle evidence within this synchronous lock scope.
        // 在当前同步锁作用域内保留精确生命周期证据。
        let mut state = runtime.lock()?;
        if state.retired {
            return Ok(true);
        }
        if state.preparing {
            return Ok(false);
        }
        // Durable observation failure closes transfer while cleanup continues for the actual original resources.
        // 持久观测失败关闭转交，同时继续清理实际原资源。
        if let Err(error) = self.observe_target_hook_initialization(owner, &mut state) {
            state.target_receipt_error = Some(error.to_string());
            runtime.closing.store(true, Ordering::Release);
        }
        if let Some(initialization) = &state.initialization
            && initialization
                .snapshot()
                .map_err(core_error)?
                .phase
                .is_terminal()
        {
            state.preparation_action.take();
        }
        // Reclaim completed acknowledged calls while the persistent instance remains open.
        // 持久实例仍开启时回收已完成且已确认的调用。
        self.reclaim_acknowledged_persistent_operations(owner, &mut state)?;
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
            if let Some(finalization) = &state.finalization {
                // Borrow existing closing evidence before any successful forget can remove it.
                // 任何成功遗忘可以移除证据之前，借用既有关闭证据。
                crate::runtime::native_identity::observe_snapshot(
                    owner.host.runtime().id(),
                    finalization,
                    runtime.native_identity_owner(),
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
        // Record real failure while publication and this original state still prove retirement ownership.
        // 发布门和此原状态仍证明退役归属时登记真实失败。
        if let Some(snapshot) = &state.finalization
            && let Err(error) = project_closing(snapshot)
        {
            self.record_cleanup_failure_locked(
                &runtime.plugin.id,
                format!("Plugin persistent shutdown failed: {error}"),
                &_publication,
            )?;
        }
        if let Some(Err(error)) = &state.prepared {
            self.record_cleanup_failure_locked(
                &runtime.plugin.id,
                format!("Plugin persistent preparation failed: {error}"),
                &_publication,
            )?;
        }
        state.preparation_action.take();
        state.retired = true;
        // Release private original-request preparation ownership only after the actual core pool is forgotten.
        // 仅在实际核心池已遗忘后释放私有原请求准备所有权。
        state.private_origin.take();
        state.target_attempt.take();
        state.private_handoff.take();
        state.private_catalog.take();
        Ok(true)
    }

    /// Await runtime retirement until expires; interruption and cleanup failures remain explicit.
    /// 等待 runtime 退役至 expires；中断及清理失败保持明确。
    pub(super) async fn wait_instance_retired(
        &self,
        runtime: &PersistentRuntime,
        expires: Instant,
    ) -> Result<(), VulcanPluginRuntimeError> {
        self.observe_instance_retirement(runtime, expires)
            .await?
            .into_result()
    }

    /// Observe runtime until expires; return pending only at the deadline and preserve actual cleanup errors.
    /// 观测 runtime 至 expires；仅在截止时间返回待完成，保留实际清理错误。
    async fn observe_instance_retirement(
        &self,
        runtime: &PersistentRuntime,
        expires: Instant,
    ) -> Result<crate::runtime::drain::RetirementObservation, VulcanPluginRuntimeError> {
        loop {
            {
                // Keep the exact lifecycle evidence within this synchronous lock scope.
                // 在当前同步锁作用域内保留精确生命周期证据。
                let state = runtime.lock()?;
                if state.retired {
                    return Ok(crate::runtime::drain::RetirementObservation::Retired);
                }
                if let Some(error) = &state.cleanup_error {
                    return Err(VulcanPluginRuntimeError::new(error.clone()));
                }
                if !state.monitored {
                    return Err(instance_error(
                        "persistent cleanup supervision was interrupted.",
                    ));
                }
            }
            if remaining(expires).is_err() {
                return Ok(crate::runtime::drain::RetirementObservation::Pending(
                    instance_error(
                        "persistent cleanup timed out; original ownership remains retained.",
                    ),
                ));
            }
            tokio::time::sleep(OBSERVATION_INTERVAL).await;
        }
    }

    /// Observe plugin_id's original instance of kind; return typed cleanup evidence without replaying initialization.
    /// 观测 plugin_id 的原 kind 实例；返回类型化清理证据，不重放初始化。
    pub(in crate::runtime) async fn observe_formal_instance(
        &self,
        plugin_id: &str,
        kind: VulcanPluginPersistentRuntimeKind,
    ) -> Result<crate::runtime::drain::DrainObservation, VulcanPluginRuntimeError> {
        // Accumulate physical-close counts independently from shutdown failures.
        // 独立于关闭失败累计物理关闭计数。
        let mut observation = crate::runtime::drain::DrainObservation::default();
        let Some(owner) = self.formal_runtime_owner()? else {
            return Ok(observation);
        };
        // Freeze one absolute deadline for this complete cleanup attempt.
        // 为本次完整清理尝试冻结一个绝对截止时间。
        let expires = deadline(owner.settings.shutdown_timeout_ms)?;
        // Serialize only this plugin's persistent initialization and explicit drain.
        // 仅串行化此插件的持久初始化及显式排空。
        let gate = self.persistent_runtime_gate(plugin_id, kind)?;
        // Stop retained preparation before waiting for its initialization gate.
        // 等待初始化门前停止已保留准备。
        {
            // Order this state transition against catalog generation publication.
            // 使当前状态转换与目录代次发布保持顺序。
            let _publication = self
                .state
                .session_registration_gate
                .lock()
                .map_err(|_| instance_error("Plugin publication gate is poisoned."))?;
            if let Some(runtime) = owner
                .instances(kind)
                .lock()
                .map_err(|_| instance_error("persistent registry lock is poisoned."))?
                .get(plugin_id)
            {
                runtime.closing.store(true, Ordering::Release);
            }
        }
        // Keep initialization ordering within the original finite admission budget.
        // 在原有限入场预算内保持初始化顺序。
        // Expired admission is still pending on the same owner, not a failed finalizer.
        // 入场到期仍为同所有者待完成，不是关闭函数失败。
        let budget = match remaining(expires) {
            Ok(budget) => budget,
            Err(error) => {
                self.record_cleanup_pending(plugin_id, error.to_string())?;
                observation.pending(error.to_string());
                return Ok(observation);
            }
        };
        // Preserve the original gate ordering and diagnostic while classifying only the proven deadline branch.
        // 保留原门排序及诊断，仅分类已证截止时间分支。
        let _initialization = match tokio::time::timeout(budget, gate.lock_owned()).await {
            Ok(initialization) => initialization,
            Err(_) => {
                // Diagnostic comes from this exact finite admission timeout, not a terminal finalizer.
                // diagnostic 来自此精确有限入场超时，不是终态关闭函数。
                let diagnostic =
                    instance_error("persistent drain admission timed out.").to_string();
                self.record_cleanup_pending(plugin_id, diagnostic.clone())?;
                observation.pending(diagnostic);
                return Ok(observation);
            }
        };
        // Retain the original persistent instance across preparation and observation.
        // 跨准备及观测保留原持久实例。
        let runtime = {
            // Order this state transition against catalog generation publication.
            // 使当前状态转换与目录代次发布保持顺序。
            let _publication = self
                .state
                .session_registration_gate
                .lock()
                .map_err(|_| instance_error("Plugin publication gate is poisoned."))?;
            // Retain the original persistent instance across preparation and observation.
            // 跨准备及观测保留原持久实例。
            let runtime = owner
                .instances(kind)
                .lock()
                .map_err(|_| instance_error("persistent registry lock is poisoned."))?
                .get(plugin_id)
                .cloned();
            if let Some(runtime) = &runtime {
                runtime.closing.store(true, Ordering::Release);
            }
            runtime
        };
        let Some(runtime) = runtime else {
            return Ok(observation);
        };
        self.start_instance_monitor(&owner, &runtime)?;
        match self.observe_instance_retirement(&runtime, expires).await {
            Ok(crate::runtime::drain::RetirementObservation::Retired) => {
                // Keep the exact lifecycle evidence within this synchronous lock scope.
                // 在当前同步锁作用域内保留精确生命周期证据。
                let state = runtime.lock()?;
                if let Some(snapshot) = &state.finalization
                    && let Err(error) = project_closing(snapshot)
                {
                    observation.failed(format!("Plugin persistent shutdown failed: {error}"));
                }
                if let Some(Err(error)) = &state.prepared {
                    observation.failed(format!("Plugin persistent preparation failed: {error}"));
                }
                drop(state);
                owner
                    .instances(kind)
                    .lock()
                    .map_err(|_| instance_error("persistent registry lock is poisoned."))?
                    .retain(|id, current| id != plugin_id || !Arc::ptr_eq(current, &runtime));
                match kind {
                    VulcanPluginPersistentRuntimeKind::Hook => {
                        observation.report.closed_hook_runtimes = 1
                    }
                    VulcanPluginPersistentRuntimeKind::UiAction => {
                        observation.report.closed_ui_action_runtimes = 1
                    }
                }
            }
            Ok(crate::runtime::drain::RetirementObservation::Pending(error)) => {
                self.record_cleanup_pending(plugin_id, error.to_string())?;
                observation.pending(error.to_string());
            }
            Err(error) => {
                self.record_cleanup_failure(plugin_id, error.to_string())?;
                observation.failed(error.to_string());
            }
        }
        Ok(observation)
    }

    /// Report retained ownership for plugin_id and kind before clearing cleanup diagnostics.
    /// 清除清理诊断前报告 plugin_id 及 kind 的保留持久所有权。
    pub(in crate::runtime) fn has_formal_instance(
        &self,
        plugin_id: &str,
        kind: VulcanPluginPersistentRuntimeKind,
    ) -> Result<bool, VulcanPluginRuntimeError> {
        let Some(owner) = self.formal_runtime_owner()? else {
            return Ok(false);
        };
        Ok(owner
            .instances(kind)
            .lock()
            .map_err(|_| instance_error("persistent registry lock is poisoned."))?
            .contains_key(plugin_id))
    }
}
