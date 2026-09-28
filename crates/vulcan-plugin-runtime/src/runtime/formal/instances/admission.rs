//! Admission freezes one purpose-isolated persistent instance and a separate authoritative workspace for each call.
//! 入场冻结一个按用途隔离的持久实例及每次调用各自的权威工作区。

use super::*;

impl VulcanPluginRuntimeManager {
    /// Prepare the exact Hook generation before expires; return its shared physical owner.
    /// 在 expires 前准备精确 Hook 代次；返回共享物理所有者。
    pub(in crate::runtime) async fn ensure_formal_hook(
        &self,
        plugin_id: &str,
        generation: u64,
        expires: Instant,
    ) -> Result<Arc<PersistentRuntime>, VulcanPluginRuntimeError> {
        self.ensure_formal_instance(
            plugin_id,
            generation,
            VulcanPluginPersistentRuntimeKind::Hook,
            expires,
        )
        .await
    }
    /// Prepare or reuse plugin_id at generation within expires; return its retained owner.
    /// 在 expires 内准备或复用 plugin_id 的 generation；返回保留所有者。
    pub(in crate::runtime) async fn ensure_formal_instance(
        &self,
        plugin_id: &str,
        generation: u64,
        kind: VulcanPluginPersistentRuntimeKind,
        expires: Instant,
    ) -> Result<Arc<PersistentRuntime>, VulcanPluginRuntimeError> {
        // Initialization ordering consumes the original execution budget.
        // 初始化排序消耗原执行预算。
        let gate = self.persistent_runtime_gate(plugin_id, kind)?;
        // Keep initialization ordering within the original finite admission budget.
        // 在原有限入场预算内保持初始化顺序。
        let _initialization = tokio::time::timeout(remaining(expires)?, gate.lock_owned())
            .await
            .map_err(|_| instance_error("persistent initialization admission timed out."))?;
        self.ensure_current_persistent_generation(plugin_id, generation)?;
        // Only the admitted initializer advances its owning action's stage.
        // 仅已入场初始化方推进所属 Action 阶段。
        let _ = crate::runtime::action_status::ACTION_RECORD
            .try_with(|record| record.phase("initializing"));
        // Missing formal ownership never selects a legacy fallback.
        // 缺失正式所有权绝不选择旧实现回退。
        let owner = self
            .formal_runtime_owner()?
            .ok_or_else(|| instance_error("Formal plugin runtime is not attached."))?;
        // Reuse only the exact registered plugin identity.
        // 仅复用精确注册的插件身份。
        let existing = owner
            .instances(kind)
            .lock()
            .map_err(|_| instance_error("persistent registry lock is poisoned."))?
            .get(plugin_id)
            .cloned();
        // Retain the original persistent instance across preparation and observation.
        // 跨准备及观测保留原持久实例。
        let runtime = if let Some(runtime) = existing {
            if runtime.plugin.generation != generation {
                runtime.closing.store(true, Ordering::Release);
                self.start_instance_monitor(&owner, &runtime)?;
                self.wait_instance_retired(&runtime, expires).await?;
                return Err(instance_error(
                    "Previous persistent generation must be drained before replacement.",
                ));
            }
            if runtime.closing.load(Ordering::Acquire) {
                return Err(instance_error(
                    "persistent instance is closing; explicit drain is required.",
                ));
            }
            self.start_instance_monitor(&owner, &runtime)?;
            runtime
        } else {
            // Publish identity before any preparation that can outlive this waiter.
            // 在任何可能比等待者存活更久的准备之前发布身份。
            let runtime = {
                // Order this state transition against catalog generation publication.
                // 使当前状态转换与目录代次发布保持顺序。
                let _publication = self
                    .state
                    .session_registration_gate
                    .lock()
                    .map_err(|_| instance_error("Plugin publication gate is poisoned."))?;
                self.ensure_current_persistent_generation(plugin_id, generation)?;
                // Freeze the current catalog descriptor before any blocking preparation.
                // 在任何阻塞准备之前冻结当前目录描述符。
                let plugin = self
                    .snapshot()
                    .plugins
                    .get(plugin_id)
                    .cloned()
                    .ok_or_else(|| {
                        instance_error("persistent plugin disappeared before registration.")
                    })?;
                // Guard the bounded registry while publishing this original identity.
                // 发布此原身份时守护有界注册表。
                let mut hooks = owner
                    .instances(kind)
                    .lock()
                    .map_err(|_| instance_error("persistent registry lock is poisoned."))?;
                if hooks.len() >= owner.settings.parent.max_sessions {
                    return Err(instance_error(
                        "Formal persistent ownership capacity is exhausted.",
                    ));
                }
                // Retain the original persistent instance across preparation and observation.
                // 跨准备及观测保留原持久实例。
                let runtime = Arc::new(PersistentRuntime {
                    kind,
                    operation_gate: Arc::new(tokio::sync::Mutex::new(())),
                    plugin,
                    closing: AtomicBool::new(false),
                    state: Mutex::new(PersistentState {
                        preparing: true,
                        preparation_action: crate::runtime::action_status::ACTION_RECORD
                            .try_with(|record| {
                                crate::runtime::action_status::ActionWorkGuard::new(
                                    Arc::clone(record),
                                    None,
                                )
                            })
                            .ok()
                            .transpose()?,
                        ..PersistentState::default()
                    }),
                });
                hooks.insert(plugin_id.to_owned(), Arc::clone(&runtime));
                runtime
            };
            // Create the guard before spawning to cover cancellation before execution.
            // 在派发前创建守卫，以覆盖执行前取消。
            let preparation = PersistentPreparation(Arc::clone(&runtime));
            self.start_instance_monitor(&owner, &runtime)?;
            // Keep the manager alive for the bounded preparation or synchronous monitor step.
            // 为有界准备或同步监控步骤保持管理器存活。
            let manager = self.clone();
            // Bind blocking preparation to the same formal parent.
            // 将阻塞准备绑定至同一个正式父级。
            let retained_owner = Arc::clone(&owner);
            // Bind preparation results to the already-published persistent record.
            // 将准备结果绑定至已经发布的持久记录。
            let retained_runtime = Arc::clone(&runtime);
            tokio::task::spawn_blocking(move || {
                // The guard records interruption unless this closure publishes its result.
                // 除非此闭包发布结果，否则守卫记录中断。
                let _preparation = preparation;
                // Preserve the exact preparation outcome for both waiter and later cleanup.
                // 为等待者及后续清理保留精确准备结果。
                let result =
                    manager.prepare_formal_instance(&retained_owner, &retained_runtime, expires);
                if result.is_err() {
                    retained_runtime.closing.store(true, Ordering::Release);
                }
                if let Ok(mut state) = retained_runtime.lock() {
                    state.prepared = Some(result);
                    state.preparing = false;
                }
            });
            runtime
        };
        loop {
            // Read consistent preparation evidence without holding locks across suspension.
            // 读取一致准备证据，不跨暂停持锁。
            let (prepared, initialization) = {
                // Keep the exact lifecycle evidence within this synchronous lock scope.
                // 在当前同步锁作用域内保留精确生命周期证据。
                let state = runtime.lock()?;
                (state.prepared.clone(), state.initialization.clone())
            };
            if let Some(result) = prepared {
                result?;
                // Observe the original core initialization without submitting a second operation.
                // 观测原核心初始化，不提交第二次操作。
                let initialization = initialization.ok_or_else(|| {
                    instance_error("Prepared persistent has no initialization operation.")
                })?;
                // Read actual core evidence instead of inferring completion from elapsed time.
                // 读取真实核心证据，不根据经过时间推断完成。
                let snapshot = initialization.snapshot().map_err(core_error)?;
                if snapshot.phase.is_terminal() {
                    if let Some(error) = snapshot.error {
                        runtime.closing.store(true, Ordering::Release);
                        return Err(core_error(error));
                    }
                    if runtime.closing.load(Ordering::Acquire) {
                        return Err(instance_error("persistent closed during initialization."));
                    }
                    self.ensure_current_persistent_generation(plugin_id, generation)?;
                    return Ok(runtime);
                }
            }
            if runtime.closing.load(Ordering::Acquire) {
                return Err(instance_error(
                    "persistent initialization was closed; original ownership remains retained.",
                ));
            }
            if remaining(expires).is_err() {
                runtime.closing.store(true, Ordering::Release);
                return Err(instance_error(
                    "persistent initialization timed out; ownership remains retained.",
                ));
            }
            tokio::time::sleep(OBSERVATION_INTERVAL).await;
        }
    }

    /// Prepare runtime on owner within expires; return after recording core identities, without waiting for Lua.
    /// 在 expires 内于 owner 上准备 runtime；记录核心身份后返回，不等待 Lua。
    fn prepare_formal_instance(
        &self,
        owner: &ManagedFormalRuntime,
        runtime: &PersistentRuntime,
        expires: Instant,
    ) -> Result<(), VulcanPluginRuntimeError> {
        remaining(expires)?;
        if runtime.closing.load(Ordering::Acquire) {
            return Err(instance_error("persistent cancelled before preparation."));
        }
        // Freeze the existing manifest and its authoritative execution package.
        // 冻结现有清单及其权威执行包。
        let manifest = runtime
            .plugin
            .manifest
            .as_ref()
            .ok_or_else(|| instance_error("persistent plugin has no valid manifest."))?;
        // Read the validated Lua declarations, including the optional Hook finalizer.
        // 读取已校验 Lua 声明，包含可选 Hook 关闭器。
        let lua = crate::runtime::lease::required_lua_runtime(manifest)?;
        owner
            .effective_settings()?
            .validate_manifest_requirements(manifest, None)?;
        // Resolve the host-authorized workspace for this precise execution scope.
        // 为此精确执行作用域解析宿主授权工作区。
        let workspace = crate::runtime::environment::plugin_execution_package_root(
            &self.state,
            &runtime.plugin,
        )
        .map_err(VulcanPluginRuntimeError::new)?;
        // Preserve distinct Hook and UI capacity owners without changing their fixed-state semantics.
        // 保留不同 Hook 及 UI 容量所有者，不改变固定状态语义。
        let capacity = {
            // Serialize the first registration and all later verification with host publication.
            // 将首次注册及全部后续核验与宿主发布排序。
            let publication = self.state.session_registration_gate.lock().map_err(|_| {
                VulcanPluginRuntimeError::new("Plugin publication gate is poisoned.")
            })?;
            self.ensure_current_persistent_generation(
                &runtime.plugin.id,
                runtime.plugin.generation,
            )?;
            owner.admit_capacity_member(
                &runtime.plugin.id,
                match runtime.kind {
                    VulcanPluginPersistentRuntimeKind::Hook => VulcanPluginCapacityPurpose::Hook,
                    VulcanPluginPersistentRuntimeKind::UiAction => {
                        VulcanPluginCapacityPurpose::UiAction
                    }
                },
                &publication,
            )?
        };
        // Optional Hook shutdown differs from the mandatory tool shutdown export.
        // 可选 Hook 关闭不同于必填工具关闭导出。
        let finalizer = match runtime.kind {
            VulcanPluginPersistentRuntimeKind::UiAction => None,
            VulcanPluginPersistentRuntimeKind::Hook => lua.hook_shutdown_function.as_ref().map(|export| ModuleFinalizer {
            export: export.clone(),
            arguments: json!({
                "payload_json": json!({"event":"hook_runtime_shutdown","plugin_id":runtime.plugin.id}).to_string(),
                "host_timeout_ms": owner.settings.shutdown_timeout_ms,
            }),
            timeout_ms: owner.settings.shutdown_timeout_ms,
        }),
        };
        // Register the original execution domain before admitting its fixed session.
        // 接纳固定会话之前注册原执行域。
        let pool = owner
            .host
            .register_bundle_pool_in_capacity(
                &capacity.capacity_id,
                VulcanPluginBundlePoolRequest {
                    plugin_id: runtime.plugin.id.clone(),
                    plugin_generation: runtime.plugin.generation,
                    workspace_root: workspace.clone(),
                    security_partition: format!(
                        "{}:{}:{}",
                        runtime.kind.label(),
                        runtime.plugin.id,
                        runtime.plugin.generation
                    ),
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
                    finalizer,
                },
            )
            .map_err(core_error)?;
        {
            // Keep the exact lifecycle evidence within this synchronous lock scope.
            // 在当前同步锁作用域内保留精确生命周期证据。
            let mut state = runtime.lock()?;
            state.pool = Some(pool.clone());
            state.workspace = Some(workspace);
        }
        // Order this state transition against catalog generation publication.
        // 使当前状态转换与目录代次发布保持顺序。
        let _publication = self
            .state
            .session_registration_gate
            .lock()
            .map_err(|_| instance_error("Plugin publication gate is poisoned."))?;
        self.ensure_current_persistent_generation(&runtime.plugin.id, runtime.plugin.generation)?;
        if runtime.closing.load(Ordering::Acquire) {
            return Err(instance_error(
                "persistent cancelled before core admission.",
            ));
        }
        // Admit one initialization operation with the original remaining budget.
        // 使用原剩余预算接纳一次初始化操作。
        let opening = owner
            .host
            .runtime()
            .open_session(&pool, remaining(expires)?)
            .map_err(core_error)?;
        // Keep the exact lifecycle evidence within this synchronous lock scope.
        // 在当前同步锁作用域内保留精确生命周期证据。
        let mut state = runtime.lock()?;
        state.session = Some(opening.session_id);
        state.initialization = Some(opening.operation);
        Ok(())
    }

    /// Invoke hook with version-one arguments, optional trusted workspace and total expires; return its business frame.
    /// 使用版本一 arguments、可选可信 workspace 及总截止时间 expires 调用 hook；返回业务帧。
    pub(crate) async fn invoke_formal_hook(
        &self,
        hook: &VulcanPluginHookDescriptor,
        arguments: Value,
        workspace: Option<PathBuf>,
        expires: Instant,
    ) -> Result<Value, VulcanPluginRuntimeError> {
        // Retain the original Hook instance across preparation and observation.
        // 跨准备及观测保留原 Hook 实例。
        let runtime = self
            .ensure_formal_hook(&hook.plugin_id, hook.plugin_generation, expires)
            .await?;
        // Retain this manager's sole formal runtime parent.
        // 保留此管理器唯一正式运行时父级。
        let owner = self
            .formal_runtime_owner()?
            .ok_or_else(|| instance_error("Formal plugin runtime is not attached."))?;
        // Admission shares catalog ordering and publishes actual core ownership before releasing the state lock.
        // 入场共享目录排序，并在释放状态锁前发布真实核心所有权。
        let mut observation = {
            // Order this state transition against catalog generation publication.
            // 使当前状态转换与目录代次发布保持顺序。
            let _publication = self
                .state
                .session_registration_gate
                .lock()
                .map_err(|_| instance_error("Plugin publication gate is poisoned."))?;
            self.ensure_current_hook_generation(hook)?;
            // Keep the exact lifecycle evidence within this synchronous lock scope.
            // 在当前同步锁作用域内保留精确生命周期证据。
            let mut state = runtime.lock()?;
            if runtime.closing.load(Ordering::Acquire) || state.retired {
                return Err(instance_error("Hook instance is closing."));
            }
            // Issue a checked monotonic correlation that Lua cannot replace.
            // 签发 Lua 不能替换的受检单调关联。
            let request = self
                .state
                .next_request_id
                .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                    value.checked_add(1)
                })
                .map_err(|_| instance_error("Plugin request identity is exhausted."))?;
            // Absence explicitly means package scope; invalid supplied metadata cannot fall back.
            // 缺省明确表示包作用域；无效已提供元数据不能回退。
            let workspace = match workspace {
                Some(workspace) => workspace,
                None => state
                    .workspace
                    .clone()
                    .ok_or_else(|| instance_error("Prepared Hook has no workspace."))?,
            };
            // Freeze the declared export, arguments and host request authority together.
            // 共同冻结已声明导出、参数及宿主请求权威。
            let call = EmbeddedCall {
                pool_id: state
                    .pool
                    .clone()
                    .ok_or_else(|| instance_error("Prepared Hook has no pool."))?,
                export: hook.manifest.function.clone(),
                arguments,
                context: LuaInvocationContext {
                    request_context: Some(RuntimeRequestContext {
                        request_id: Some(request.to_string()),
                        ..RuntimeRequestContext::default()
                    }),
                    ..LuaInvocationContext::default()
                },
            };
            // Select the recorded core session without any alternate identity lookup.
            // 选择已记录核心会话，不查询任何替代身份。
            let session = state
                .session
                .as_ref()
                .ok_or_else(|| instance_error("Prepared Hook has no session."))?;
            // Record the actual operation returned by core admission.
            // 记录核心入场返回的实际操作。
            let handle = owner
                .host
                .submit_session_in_workspace(session, call, &workspace, remaining(expires)?)
                .map_err(core_error)?;
            // Separate observer completion from the operation's physical termination.
            // 区分观测者完成与操作物理终止。
            let acknowledged = Arc::new(AtomicBool::new(false));
            state.operations.insert(
                handle.id().to_owned(),
                PersistentOperation {
                    handle: handle.clone(),
                    acknowledged: Arc::clone(&acknowledged),
                },
            );
            PersistentObservation {
                handle,
                acknowledged,
                delivered: false,
            }
        };
        loop {
            // Read actual core evidence instead of inferring completion from elapsed time.
            // 读取真实核心证据，不根据经过时间推断完成。
            let snapshot = observation.handle.snapshot().map_err(core_error)?;
            if snapshot.phase.is_terminal() {
                observation.delivered = true;
                return project_business(&snapshot);
            }
            if remaining(expires).is_err() {
                return Err(instance_error(
                    "Hook execution timed out; actual work remains retained.",
                ));
            }
            tokio::time::sleep(OBSERVATION_INTERVAL).await;
        }
    }
}

impl VulcanPluginRuntimeManager {
    /// Invoke the declared UI export on runtime, preserving arguments and the original ActionExecution budget.
    /// 在 runtime 调用已声明 UI 导出，保留 arguments 及原 ActionExecution 预算。
    /// Return the exact business result; observers never release physical operation ownership.
    /// 返回精确业务结果；观察者绝不释放物理操作所有权。
    pub(in crate::runtime) async fn invoke_formal_ui_action(
        &self,
        runtime: &Arc<PersistentRuntime>,
        export: &str,
        arguments: Value,
        execution: &mut crate::runtime::execution::ActionExecution,
    ) -> Result<Value, VulcanPluginRuntimeError> {
        execution.record.phase("operation_wait");
        // The cancellable gate is admission policy, while the core still owns VM scheduling.
        // 可取消门属于入场策略，VM 调度仍由核心所有。
        let gate = execution
            .wait(Arc::clone(&runtime.operation_gate).lock_owned())
            .await?;
        execution.remaining_ms()?;
        // Retain the manager's single formal parent without creating another scheduler.
        // 保留管理器唯一正式父级，不创建另一个调度器。
        let owner = self
            .formal_runtime_owner()?
            .ok_or_else(|| instance_error("Formal plugin runtime is not attached."))?;
        // Retain the original core observer independently from physical request ownership.
        // 独立于物理请求所有权保留原核心观察者。
        let mut observation = {
            // Publication and generation validation must order together with core admission.
            // 发布及代次校验必须与核心入场共同排序。
            let _publication = self
                .state
                .session_registration_gate
                .lock()
                .map_err(|_| instance_error("Plugin publication gate is poisoned."))?;
            self.ensure_current_persistent_generation(
                &runtime.plugin.id,
                runtime.plugin.generation,
            )?;
            // Read or update one exact ownership record under its short evidence lock.
            // 在短证据锁下读取或更新一条精确所有权记录。
            let mut state = runtime.lock()?;
            if runtime.kind != VulcanPluginPersistentRuntimeKind::UiAction
                || runtime.closing.load(Ordering::Acquire)
                || state.retired
            {
                return Err(instance_error(
                    "UI Action instance is closing or has a different purpose.",
                ));
            }
            // Cancellation closes atomically before submission can produce any business effects.
            // 提交可能产生任何业务副作用之前，原子关闭取消。
            execution.record.start()?;
            // Transfer capacity and serial admission to the real submitted operation.
            // 将容量及串行入场转移至真实提交操作。
            let work = crate::runtime::action_status::ActionWorkGuard::new(
                Arc::clone(&execution.record),
                Some(gate),
            )?;
            // Freeze host-selected request authority before asynchronous execution.
            // 在异步执行前冻结宿主选择的请求权威。
            let request = self
                .state
                .next_request_id
                .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                    value.checked_add(1)
                })
                .map_err(|_| instance_error("Plugin request identity is exhausted."))?;
            // Persistent UI retains its original package workspace; request arguments cannot expand it.
            // 持久 UI 保留原包工作区；请求参数不能扩展它。
            let workspace = state
                .workspace
                .as_ref()
                .ok_or_else(|| instance_error("Prepared UI instance has no workspace."))?;
            // Freeze the declared export, arguments and host correlation together.
            // 共同冻结已声明导出、参数及宿主关联。
            let call = EmbeddedCall {
                pool_id: state
                    .pool
                    .clone()
                    .ok_or_else(|| instance_error("Prepared UI instance has no pool."))?,
                export: export.to_owned(),
                arguments,
                context: LuaInvocationContext {
                    request_context: Some(RuntimeRequestContext {
                        request_id: Some(request.to_string()),
                        ..RuntimeRequestContext::default()
                    }),
                    ..LuaInvocationContext::default()
                },
            };
            // Borrow the recorded fixed session instead of resolving another instance.
            // 借用已记录固定会话，不解析另一个实例。
            let session = state
                .session
                .as_ref()
                .ok_or_else(|| instance_error("Prepared UI instance has no session."))?;
            // Keep the actual core operation returned by admission.
            // 保留入场返回的真实核心操作。
            let handle = owner
                .host
                .submit_action(
                    session,
                    call,
                    workspace,
                    execution.deadline.into_std(),
                    work,
                )
                .map_err(core_error)?;
            // Record observer acknowledgement separately from real termination.
            // 将观察者确认与真实终止分开记录。
            let acknowledged = Arc::new(AtomicBool::new(false));
            state.operations.insert(
                handle.id().to_owned(),
                PersistentOperation {
                    handle: handle.clone(),
                    acknowledged: Arc::clone(&acknowledged),
                },
            );
            PersistentObservation {
                handle,
                acknowledged,
                delivered: false,
            }
        };
        loop {
            // Read actual core or catalog evidence, without inferring a replacement identity.
            // 读取真实核心或目录证据，不推断替代身份。
            let snapshot = observation.handle.snapshot().map_err(core_error)?;
            if snapshot.phase.is_terminal() {
                observation.delivered = true;
                owner.host.reap_workspace_requests().map_err(core_error)?;
                return project_business(&snapshot);
            }
            if remaining(execution.deadline.into_std()).is_err() {
                return Err(instance_error(
                    "Plugin action timed out; actual work and capacity remain retained. Do not replay mutations.",
                ));
            }
            tokio::time::sleep(OBSERVATION_INTERVAL).await;
        }
    }
}
