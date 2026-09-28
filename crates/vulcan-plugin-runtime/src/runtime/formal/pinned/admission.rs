//! Pinned session admission, preparation and first business publication.
//! 固定会话入场、准备和首次业务发布。

use super::*;

impl VulcanPluginRuntimeManager {
    /// Open a real pinned session using validated `payload`, original owner/workspace and one absolute business budget.
    /// 使用已校验 `payload`、原所有者／工作区及单个绝对业务预算打开真实固定会话。
    /// Return the unchanged session result envelope, or a failure containing the retained host identity.
    /// 返回未改变的会话结果信封，或包含保留宿主身份的失败。
    pub(crate) async fn open_formal_session(
        &self,
        tool: &VulcanPluginToolDescriptor,
        owner_id: String,
        workspace: PathBuf,
        payload: Value,
        timeout_ms: u64,
    ) -> Result<Value, VulcanPluginRuntimeError> {
        let executor = tokio::runtime::Handle::try_current().map_err(|_| {
            VulcanPluginRuntimeError::new("Formal session admission requires a Tokio executor.")
        })?;
        let owner = self.formal_runtime_owner()?.ok_or_else(|| {
            VulcanPluginRuntimeError::new("Formal plugin runtime is not attached.")
        })?;
        let deadline = deadline(timeout_ms)?;
        json_size(&payload, owner.settings.parent.max_value_bytes).map_err(core_error)?;
        let arguments = self.formal_envelope(payload, timeout_ms)?;
        json_size(&arguments, owner.settings.parent.max_value_bytes).map_err(core_error)?;
        let (session, closing_arguments, closing_ms) = {
            let publication = self
                .state
                .session_registration_gate
                .lock()
                .map_err(|_| publication_error())?;
            self.ensure_current_tool_generation(tool)?;
            let plugin = self
                .snapshot()
                .plugins
                .get(&tool.plugin_id)
                .cloned()
                .ok_or_else(|| {
                    VulcanPluginRuntimeError::new("Plugin disappeared before session admission.")
                })?;
            let manifest = plugin
                .manifest
                .as_ref()
                .ok_or_else(|| VulcanPluginRuntimeError::new("Plugin has no valid manifest."))?;
            let runtime = crate::runtime::lease::required_lua_runtime(manifest)?;
            let close = runtime
                .tools
                .iter()
                .find(|tool| tool.execution.kind == VulcanPluginToolExecutionKind::SessionClose)
                .ok_or_else(|| {
                    VulcanPluginRuntimeError::new("Plugin declares no session close operation.")
                })?;
            let closing_ms = close
                .execution
                .timeout_ms
                .checked_add(close.execution.timeout_grace_ms)
                .ok_or_else(|| {
                    VulcanPluginRuntimeError::new("Session close budget is out of range.")
                })?;
            let closing_arguments = self.formal_envelope(
                plugin_payload(&close.execution.operation, Map::new())?,
                closing_ms,
            )?;
            let input_bytes = json_size(
                &(&arguments, &closing_arguments, &owner_id, &workspace),
                owner.settings.parent.max_queued_bytes,
            )
            .map_err(core_error)?;
            let calls = owner.calls.lock().map_err(|_| registry_error())?;
            let mut pinned = owner.pinned.lock().map_err(|_| registry_error())?;
            let total = calls
                .values()
                .map(|call| call.input_bytes.load(Ordering::Acquire))
                .chain(pinned.values().map(|session| session.input_bytes))
                .chain(std::iter::once(
                    owner
                        .reusable
                        .lock()
                        .map_err(|_| registry_error())?
                        .retained_input_bytes()?,
                ))
                .try_fold(input_bytes, usize::checked_add);
            if pinned.len() >= owner.settings.parent.max_sessions
                || total.is_none_or(|bytes| bytes > owner.settings.parent.max_queued_bytes)
            {
                return Err(VulcanPluginRuntimeError::new(
                    "Formal session retention or input byte capacity is exhausted.",
                ));
            }
            if owner.host.runtime().usage().map_err(core_error)?.closing {
                return Err(VulcanPluginRuntimeError::new(
                    "Formal plugin runtime is closing.",
                ));
            }
            self.reserve_session_slot_locked(
                &plugin.id,
                runtime.max_active_sessions,
                &publication,
            )?;
            let idle = runtime.session_idle_timeout_sec.map(Duration::from_secs);
            let session = Arc::new(PinnedSession {
                id: Uuid::new_v4().to_string(),
                plugin,
                owner: owner_id,
                input_bytes,
                closing: AtomicBool::new(false),
                state: Mutex::new(PinnedState {
                    preparing: true,
                    monitored: false,
                    prepared: None,
                    pool: None,
                    core_session: None,
                    initialization: None,
                    opened: false,
                    activity: Instant::now(),
                    idle,
                    session_forgotten: false,
                    pool_forgotten: false,
                    retired: false,
                    acknowledged: false,
                    idle_closed: false,
                    finalization: None,
                    cleanup_error: None,
                }),
            });
            pinned.insert(session.id.clone(), Arc::clone(&session));
            (session, closing_arguments, closing_ms)
        };
        let mut observation = PinnedObservation {
            session: Arc::clone(&session),
            completed: false,
        };
        let preparation_guard = PinnedPreparationGuard(Arc::clone(&session));
        self.start_pinned_monitor(&owner, &session)?;
        let prepare_manager = self.clone();
        let prepare_owner = Arc::clone(&owner);
        let prepare_session = Arc::clone(&session);
        executor.spawn_blocking(move || {
            let _preparation_guard = preparation_guard;
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                prepare_manager.prepare_pinned(
                    &prepare_owner,
                    &prepare_session,
                    workspace,
                    closing_arguments,
                    closing_ms,
                    deadline,
                )
            }))
            .unwrap_or_else(|_| {
                Err(VulcanPluginRuntimeError::new(
                    "Formal session preparation panicked.",
                ))
            });
            if result.is_err() {
                prepare_session.closing.store(true, Ordering::Release);
            }
            if let Ok(mut state) = prepare_session.lock() {
                state.prepared = Some(result);
                state.preparing = false;
            }
        });
        let result = self
            .finish_pinned_open(&owner, &session, tool, arguments, deadline)
            .await;
        if result.is_err() {
            session.closing.store(true, Ordering::Release);
        }
        observation.completed = true;
        result
            .map(|result| session_result(&session.id, result, false))
            .map_err(|error| with_session(error, &session.id))
    }

    /// Prepare only this registered `session`; publish each real resource before advancing to the next admission.
    /// 仅准备此注册 `session`；推进下一入场前先发布每个真实资源。
    /// The frozen close envelope and deadline are host authority; failures never replay initialization.
    /// 冻结关闭信封及截止时间属于宿主权威；失败绝不重放初始化。
    fn prepare_pinned(
        &self,
        owner: &ManagedFormalRuntime,
        session: &PinnedSession,
        workspace: PathBuf,
        close_arguments: Value,
        close_ms: u64,
        deadline: Instant,
    ) -> Result<(), VulcanPluginRuntimeError> {
        if session.closing.load(Ordering::Acquire) {
            return Err(VulcanPluginRuntimeError::new(
                "Session cancelled before preparation.",
            ));
        }
        let manifest = session
            .plugin
            .manifest
            .as_ref()
            .ok_or_else(|| VulcanPluginRuntimeError::new("Plugin has no valid manifest."))?;
        let runtime = crate::runtime::lease::required_lua_runtime(manifest)?;
        owner
            .settings
            .validate_manifest_requirements(manifest, None)?;
        // Keep the session purpose owner stable until this actual member registration finishes.
        // 在此实际成员注册结束前保持会话用途所有者稳定。
        let capacity = {
            // Serialize the first registration and all later verification with host publication.
            // 将首次注册及全部后续核验与宿主发布排序。
            let publication = self.state.session_registration_gate.lock().map_err(|_| {
                VulcanPluginRuntimeError::new("Plugin publication gate is poisoned.")
            })?;
            self.require_pinned_current(session)?;
            owner.admit_capacity_member(
                &session.plugin.id,
                VulcanPluginCapacityPurpose::ToolSession,
                &publication,
            )?
        };
        let pool = owner
            .host
            .register_bundle_pool_in_capacity(
                &capacity.capacity_id,
                VulcanPluginBundlePoolRequest {
                    plugin_id: session.plugin.id.clone(),
                    plugin_generation: session.plugin.generation,
                    workspace_root: workspace,
                    security_partition: format!("session:{}:{}", session.owner, session.id),
                    scope: VulcanPluginBridgeLeaseScope::Enabled,
                    policy: PluginPoolConfig {
                        kind: capacity.config.resources.kind,
                        min_resident_vms: 0,
                        max_resident_vms: 1,
                        max_running_calls: 1,
                        max_queued_calls: capacity.config.max_queued_calls,
                        reuse: InstanceReuse::Session,
                        serial: true,
                        backend: VULCAN_PLUGIN_EXECUTION_BACKEND,
                        idle_ttl_ms: None,
                        max_uses: None,
                    },
                    finalizer: Some(ModuleFinalizer {
                        export: runtime.shutdown_function.clone(),
                        arguments: close_arguments,
                        timeout_ms: close_ms,
                    }),
                },
            )
            .map_err(core_error)?;
        session.lock()?.pool = Some(pool.clone());
        let _publication = self
            .state
            .session_registration_gate
            .lock()
            .map_err(|_| publication_error())?;
        if let Err(error) = self.require_pinned_current(session) {
            session.closing.store(true, Ordering::Release);
            return Err(error);
        }
        if session.closing.load(Ordering::Acquire) {
            return Err(VulcanPluginRuntimeError::new(
                "Session cancelled before core admission.",
            ));
        }
        let opening = owner
            .host
            .runtime()
            .open_session(&pool, remaining(deadline)?)
            .map_err(core_error)?;
        let mut state = session.lock()?;
        state.core_session = Some(opening.session_id);
        state.initialization = Some(opening.operation);
        Ok(())
    }

    /// Observe exact preparation and initialization, then submit the declared opening export once.
    /// 观测精确准备及初始化，然后仅提交一次声明打开导出。
    /// Return the original opening business value; caller cancellation leaves cleanup with the registered owner.
    /// 返回原打开业务值；调用方取消使清理继续归注册所有者。
    async fn finish_pinned_open(
        &self,
        owner: &Arc<ManagedFormalRuntime>,
        session: &Arc<PinnedSession>,
        tool: &VulcanPluginToolDescriptor,
        arguments: Value,
        deadline: Instant,
    ) -> Result<Value, VulcanPluginRuntimeError> {
        loop {
            let prepared = session.lock()?.prepared.clone();
            if let Some(result) = prepared {
                result?;
                break;
            }
            tokio::time::sleep(OBSERVATION_INTERVAL).await;
        }
        let initialization = session.lock()?.initialization.clone().ok_or_else(|| {
            VulcanPluginRuntimeError::new("Session preparation has no initialization identity.")
        })?;
        let initialized = wait_operation(&initialization).await?;
        core_result(&initialized)?;
        owner
            .host
            .runtime()
            .forget_operation(initialization.id())
            .map_err(core_error)?;
        session.lock()?.initialization = None;
        let operation = self.submit_pinned(owner, session, tool, arguments, deadline, true)?;
        let snapshot = wait_operation(&operation).await?;
        let result = project_business(&snapshot)?;
        let _publication = self
            .state
            .session_registration_gate
            .lock()
            .map_err(|_| publication_error())?;
        self.require_pinned_current(session)?;
        if session.closing.load(Ordering::Acquire) {
            return Err(VulcanPluginRuntimeError::new(
                "Session closed before opening result publication.",
            ));
        }
        let mut state = session.lock()?;
        owner
            .host
            .runtime()
            .forget_operation(operation.id())
            .map_err(core_error)?;
        state.opened = true;
        state.activity = Instant::now();
        Ok(result)
    }

    /// Submit one current-generation operation after checking `opening` versus ready-session admission.
    /// 检查 `opening` 与就绪会话入场后，提交单个当前代次操作。
    /// The publication gate serializes lifecycle changes and submission; core owns the bounded execution queue.
    /// 发布门串行化生命周期变化与提交；核心拥有有界执行队列。
    pub(super) fn submit_pinned(
        &self,
        owner: &ManagedFormalRuntime,
        session: &PinnedSession,
        tool: &VulcanPluginToolDescriptor,
        arguments: Value,
        deadline: Instant,
        opening: bool,
    ) -> Result<OperationHandle, VulcanPluginRuntimeError> {
        let _publication = self
            .state
            .session_registration_gate
            .lock()
            .map_err(|_| publication_error())?;
        if let Err(error) = self.require_pinned_current(session) {
            session.closing.store(true, Ordering::Release);
            return Err(error);
        }
        self.ensure_current_tool_generation(tool)?;
        let mut state = session.lock()?;
        if session.closing.load(Ordering::Acquire) || state.retired || (!opening && !state.opened) {
            return Err(VulcanPluginRuntimeError::new(
                "Formal session is not open for business admission.",
            ));
        }
        let core_session = state
            .core_session
            .as_ref()
            .ok_or_else(|| VulcanPluginRuntimeError::new("Formal session has no core identity."))?;
        let manifest = session
            .plugin
            .manifest
            .as_ref()
            .ok_or_else(|| VulcanPluginRuntimeError::new("Plugin has no valid manifest."))?;
        let runtime = crate::runtime::lease::required_lua_runtime(manifest)?;
        let operation = owner
            .host
            .runtime()
            .submit_session(
                core_session,
                runtime.invoke_function.clone(),
                arguments,
                LuaInvocationContext::default(),
                remaining(deadline)?,
            )
            .map_err(core_error)?;
        state.activity = Instant::now();
        Ok(operation)
    }
}
