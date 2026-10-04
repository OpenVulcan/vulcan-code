//! Physical session retirement, interrupted supervision and bounded drain.
//! 会话物理退役、中断监督及有界排空。

use super::*;

impl VulcanPluginRuntimeManager {
    /// Start or resume this record's monitor; the task holds only a weak manager across idle waits.
    /// 启动或恢复此记录监控；任务跨空闲等待仅持有弱管理器引用。
    /// Existing supervision is reused; executor interruption never creates a second core owner.
    /// 复用既有监督；执行器中断绝不创建第二核心所有者。
    pub(super) fn start_pinned_monitor(
        &self,
        owner: &Arc<ManagedFormalRuntime>,
        session: &Arc<PinnedSession>,
    ) -> Result<(), VulcanPluginRuntimeError> {
        let executor = tokio::runtime::Handle::try_current().map_err(|_| {
            VulcanPluginRuntimeError::new("Formal session monitoring requires a Tokio executor.")
        })?;
        {
            let mut state = session.lock()?;
            if state.monitored || state.retired {
                return Ok(());
            }
            state.monitored = true;
            state.cleanup_error = None;
        }
        let weak = Arc::downgrade(&self.state);
        let owner = Arc::clone(owner);
        let session = Arc::clone(session);
        let guard = PinnedMonitorGuard(Arc::clone(&session));
        executor.spawn(async move {
            let _guard = guard;
            loop {
                let done = {
                    let Some(state) = weak.upgrade() else {
                        let _ = owner.host.runtime().request_close();
                        return;
                    };
                    let manager = VulcanPluginRuntimeManager { state };
                    let progress = manager.advance_pinned(&owner, &session).and_then(|done| {
                        if done {
                            manager.acknowledge_clean_pinned(&owner, &session, true)?;
                        }
                        Ok(done)
                    });
                    match progress {
                        Ok(done) => done,
                        Err(error) => {
                            if let Ok(mut state) = session.lock() {
                                state.cleanup_error = Some(error.to_string());
                            }
                            let _ = manager.record_cleanup_failure(
                                &session.plugin.id,
                                with_session(error, &session.id).to_string(),
                            );
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

    /// Advance one nonblocking ownership observation under publication order; never run or replay Lua here.
    /// 在发布顺序下推进一次非阻塞所有权观测；绝不在此运行或重放 Lua。
    /// Return true only when physical retirement and host accounting are both complete.
    /// 仅在物理退役与宿主记账均完成后返回真值。
    fn advance_pinned(
        &self,
        owner: &ManagedFormalRuntime,
        session: &PinnedSession,
    ) -> Result<bool, VulcanPluginRuntimeError> {
        let publication = self
            .state
            .session_registration_gate
            .lock()
            .map_err(|_| publication_error())?;
        let mut state = session.lock()?;
        if state.retired {
            return Ok(true);
        }
        if state.preparing {
            return Ok(false);
        }
        if let Some(id) = state.core_session.clone()
            && !state.session_forgotten
        {
            let core = owner.host.runtime().session(&id).map_err(core_error)?;
            if state.opened
                && core.phase == EmbeddedSessionPhase::Ready
                && core.queued_calls == 0
                && state
                    .idle
                    .is_some_and(|idle| state.activity.elapsed() >= idle)
            {
                session.closing.store(true, Ordering::Release);
                state.idle_closed = true;
            }
            if matches!(
                core.phase,
                EmbeddedSessionPhase::Closing | EmbeddedSessionPhase::Closed
            ) {
                session.closing.store(true, Ordering::Release);
            }
            if !session.closing.load(Ordering::Acquire) {
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
                    session.native_identity_owner(),
                );
            }
            owner
                .host
                .runtime()
                .forget_session(&id)
                .map_err(core_error)?;
            state.session_forgotten = true;
        } else if !session.closing.load(Ordering::Acquire) {
            return Ok(false);
        }
        if !state.pool_forgotten
            && let Some(pool) = &state.pool
        {
            owner.host.runtime().close_pool(pool).map_err(core_error)?;
            match owner.host.runtime().forget_pool(pool) {
                Ok(()) => state.pool_forgotten = true,
                Err(error) if error.code == EmbeddedErrorCode::Busy => return Ok(false),
                Err(error) => return Err(core_error(error)),
            }
        }
        // Publish the actual finalizer failure before releasing the last live capacity proof.
        // 在释放最后活动容量证明前发布实际关闭失败。
        if let Some(snapshot) = &state.finalization
            && let Err(error) = project_closing(snapshot)
        {
            self.record_cleanup_failure_locked(
                &session.plugin.id,
                with_session(error, &session.id).to_string(),
                &publication,
            )?;
        }
        self.release_session_slot_with_publication(&session.plugin.id, &publication)?;
        state.retired = true;
        Ok(true)
    }

    /// Wait only until `deadline` for real retirement; timeout keeps the session and its capacity queryable.
    /// 仅等待至 `deadline` 以确认真实退役；超时使会话及容量继续可查询。
    pub(super) async fn wait_pinned_retired(
        &self,
        session: &PinnedSession,
        deadline: Instant,
    ) -> Result<(), VulcanPluginRuntimeError> {
        self.observe_pinned_retirement(session, deadline)
            .await?
            .into_result()
    }

    /// Observe session until deadline; return typed pending only for expiration and Err for actual cleanup faults.
    /// 观测 session 至 deadline；仅到期返回类型化待完成，实际清理故障返回 Err。
    async fn observe_pinned_retirement(
        &self,
        session: &PinnedSession,
        deadline: Instant,
    ) -> Result<crate::runtime::drain::RetirementObservation, VulcanPluginRuntimeError> {
        loop {
            {
                let state = session.lock()?;
                if state.retired {
                    return Ok(crate::runtime::drain::RetirementObservation::Retired);
                }
                if let Some(error) = &state.cleanup_error {
                    return Err(with_session(
                        VulcanPluginRuntimeError::new(error),
                        &session.id,
                    ));
                }
            }
            if Instant::now() >= deadline {
                return Ok(crate::runtime::drain::RetirementObservation::Pending(
                    VulcanPluginRuntimeError::new(format!(
                        "Formal session {} has not physically retired; ownership remains retained.",
                        session.id
                    )),
                ));
            }
            tokio::time::sleep(OBSERVATION_INTERVAL).await;
        }
    }

    /// Cancel and drain exactly the sessions matching explicit plugin/controller filters; retain failed evidence.
    /// 取消并排空精确匹配插件／控制器过滤条件的会话；保留失败证据。
    pub(crate) async fn drain_pinned(
        &self,
        plugin_id: Option<&str>,
        owner_id: Option<&str>,
    ) -> Result<VulcanPluginDrainReport, VulcanPluginRuntimeError> {
        Ok(self.observe_pinned(plugin_id, owner_id).await?.report)
    }

    /// Observe plugin_id/owner_id's original sessions; return pending retirement independently from closing failures.
    /// 观测 plugin_id/owner_id 的原会话；分别返回待完成退役与关闭失败。
    pub(in crate::runtime) async fn observe_pinned(
        &self,
        plugin_id: Option<&str>,
        owner_id: Option<&str>,
    ) -> Result<crate::runtime::drain::DrainObservation, VulcanPluginRuntimeError> {
        let Some(owner) = self.formal_runtime_owner()? else {
            return Ok(crate::runtime::drain::DrainObservation::default());
        };
        let sessions: Vec<_> = owner
            .pinned
            .lock()
            .map_err(|_| registry_error())?
            .values()
            .filter(|session| {
                plugin_id.is_none_or(|plugin| session.plugin.id == plugin)
                    && owner_id.is_none_or(|id| session.owner == id)
            })
            .cloned()
            .collect();
        let deadline = deadline(owner.settings.shutdown_timeout_ms)?;
        for session in &sessions {
            session.closing.store(true, Ordering::Release);
            self.start_pinned_monitor(&owner, session)?;
        }
        // The typed aggregate retains a real finalizer failure after later registry cleanup.
        // 类型化聚合在后续注册表清理之后保留真实关闭函数失败。
        let mut observation = crate::runtime::drain::DrainObservation::default();
        for session in sessions {
            match self.observe_pinned_retirement(&session, deadline).await {
                Ok(crate::runtime::drain::RetirementObservation::Retired) => {
                    observation.report.closed_sessions += 1;
                    if let Some(finalization) = &session.lock()?.finalization
                        && let Err(error) = project_closing(finalization)
                    {
                        observation.failed(with_session(error, &session.id).to_string());
                    }
                }
                Ok(crate::runtime::drain::RetirementObservation::Pending(error)) => {
                    self.record_cleanup_pending(&session.plugin.id, error.to_string())?;
                    observation.pending(error.to_string());
                }
                Err(error) => {
                    self.record_cleanup_failure(&session.plugin.id, error.to_string())?;
                    observation.failed(error.to_string());
                }
            }
        }
        Ok(observation)
    }
}
