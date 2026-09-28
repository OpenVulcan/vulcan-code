//! Admission freezes one Hook instance and a separate authoritative workspace for each call.
//! 入场冻结一个 Hook 实例及每次调用各自的权威工作区。

use super::*;

impl VulcanPluginRuntimeManager {
    /// Prepare or reuse plugin_id at generation within expires; return its retained owner.
    /// 在 expires 内准备或复用 plugin_id 的 generation；返回保留所有者。
    pub(crate) async fn ensure_formal_hook(
        &self,
        plugin_id: &str,
        generation: u64,
        expires: Instant,
    ) -> Result<Arc<HookRuntime>, VulcanPluginRuntimeError> {
        // Initialization ordering consumes the original execution budget.
        // 初始化排序消耗原执行预算。
        let gate =
            self.persistent_runtime_gate(plugin_id, VulcanPluginPersistentRuntimeKind::Hook)?;
        // Keep initialization ordering within the original finite admission budget.
        // 在原有限入场预算内保持初始化顺序。
        let _initialization = tokio::time::timeout(remaining(expires)?, gate.lock_owned())
            .await
            .map_err(|_| hook_error("Hook initialization admission timed out."))?;
        self.ensure_current_persistent_generation(plugin_id, generation)?;
        // Missing formal ownership never selects a legacy fallback.
        // 缺失正式所有权绝不选择旧实现回退。
        let owner = self
            .formal_runtime_owner()?
            .ok_or_else(|| hook_error("Formal plugin runtime is not attached."))?;
        // Reuse only the exact registered plugin identity.
        // 仅复用精确注册的插件身份。
        let existing = owner
            .hooks
            .lock()
            .map_err(|_| hook_error("Hook registry lock is poisoned."))?
            .get(plugin_id)
            .cloned();
        // Retain the original Hook instance across preparation and observation.
        // 跨准备及观测保留原 Hook 实例。
        let runtime = if let Some(runtime) = existing {
            if runtime.plugin.generation != generation {
                runtime.closing.store(true, Ordering::Release);
                self.start_hook_monitor(&owner, &runtime)?;
                self.wait_hook_retired(&runtime, expires).await?;
                return Err(hook_error(
                    "Previous Hook generation must be drained before replacement.",
                ));
            }
            if runtime.closing.load(Ordering::Acquire) {
                return Err(hook_error(
                    "Hook instance is closing; explicit drain is required.",
                ));
            }
            self.start_hook_monitor(&owner, &runtime)?;
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
                    .map_err(|_| hook_error("Plugin publication gate is poisoned."))?;
                self.ensure_current_persistent_generation(plugin_id, generation)?;
                // Freeze the current catalog descriptor before any blocking preparation.
                // 在任何阻塞准备之前冻结当前目录描述符。
                let plugin = self
                    .snapshot()
                    .plugins
                    .get(plugin_id)
                    .cloned()
                    .ok_or_else(|| hook_error("Hook plugin disappeared before registration."))?;
                // Guard the bounded registry while publishing this original identity.
                // 发布此原身份时守护有界注册表。
                let mut hooks = owner
                    .hooks
                    .lock()
                    .map_err(|_| hook_error("Hook registry lock is poisoned."))?;
                if hooks.len() >= owner.settings.parent.max_sessions {
                    return Err(hook_error("Formal Hook ownership capacity is exhausted."));
                }
                // Retain the original Hook instance across preparation and observation.
                // 跨准备及观测保留原 Hook 实例。
                let runtime = Arc::new(HookRuntime {
                    plugin,
                    closing: AtomicBool::new(false),
                    state: Mutex::new(HookState {
                        preparing: true,
                        ..HookState::default()
                    }),
                });
                hooks.insert(plugin_id.to_owned(), Arc::clone(&runtime));
                runtime
            };
            // Create the guard before spawning to cover cancellation before execution.
            // 在派发前创建守卫，以覆盖执行前取消。
            let preparation = HookPreparation(Arc::clone(&runtime));
            self.start_hook_monitor(&owner, &runtime)?;
            // Keep the manager alive for the bounded preparation or synchronous monitor step.
            // 为有界准备或同步监控步骤保持管理器存活。
            let manager = self.clone();
            // Bind blocking preparation to the same formal parent.
            // 将阻塞准备绑定至同一个正式父级。
            let retained_owner = Arc::clone(&owner);
            // Bind preparation results to the already-published Hook record.
            // 将准备结果绑定至已经发布的 Hook 记录。
            let retained_runtime = Arc::clone(&runtime);
            tokio::task::spawn_blocking(move || {
                // The guard records interruption unless this closure publishes its result.
                // 除非此闭包发布结果，否则守卫记录中断。
                let _preparation = preparation;
                // Preserve the exact preparation outcome for both waiter and later cleanup.
                // 为等待者及后续清理保留精确准备结果。
                let result =
                    manager.prepare_formal_hook(&retained_owner, &retained_runtime, expires);
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
                let initialization = initialization
                    .ok_or_else(|| hook_error("Prepared Hook has no initialization operation."))?;
                // Read actual core evidence instead of inferring completion from elapsed time.
                // 读取真实核心证据，不根据经过时间推断完成。
                let snapshot = initialization.snapshot().map_err(core_error)?;
                if snapshot.phase.is_terminal() {
                    if let Some(error) = snapshot.error {
                        runtime.closing.store(true, Ordering::Release);
                        return Err(core_error(error));
                    }
                    if runtime.closing.load(Ordering::Acquire) {
                        return Err(hook_error("Hook closed during initialization."));
                    }
                    self.ensure_current_persistent_generation(plugin_id, generation)?;
                    return Ok(runtime);
                }
            }
            if runtime.closing.load(Ordering::Acquire) {
                return Err(hook_error(
                    "Hook initialization was closed; original ownership remains retained.",
                ));
            }
            if remaining(expires).is_err() {
                runtime.closing.store(true, Ordering::Release);
                return Err(hook_error(
                    "Hook initialization timed out; ownership remains retained.",
                ));
            }
            tokio::time::sleep(OBSERVATION_INTERVAL).await;
        }
    }

    /// Prepare runtime on owner within expires; return after recording core identities, without waiting for Lua.
    /// 在 expires 内于 owner 上准备 runtime；记录核心身份后返回，不等待 Lua。
    fn prepare_formal_hook(
        &self,
        owner: &ManagedFormalRuntime,
        runtime: &HookRuntime,
        expires: Instant,
    ) -> Result<(), VulcanPluginRuntimeError> {
        remaining(expires)?;
        if runtime.closing.load(Ordering::Acquire) {
            return Err(hook_error("Hook cancelled before preparation."));
        }
        // Freeze the existing manifest and its authoritative execution package.
        // 冻结现有清单及其权威执行包。
        let manifest = runtime
            .plugin
            .manifest
            .as_ref()
            .ok_or_else(|| hook_error("Hook plugin has no valid manifest."))?;
        // Read the validated Lua declarations, including the optional Hook finalizer.
        // 读取已校验 Lua 声明，包含可选 Hook 关闭器。
        let lua = crate::runtime::lease::required_lua_runtime(manifest)?;
        // Resolve the host-authorized workspace for this precise execution scope.
        // 为此精确执行作用域解析宿主授权工作区。
        let workspace = crate::runtime::environment::plugin_execution_package_root(
            &self.state,
            &runtime.plugin,
        )
        .map_err(VulcanPluginRuntimeError::new)?;
        {
            // Order this state transition against catalog generation publication.
            // 使当前状态转换与目录代次发布保持顺序。
            let _publication = self
                .state
                .session_registration_gate
                .lock()
                .map_err(|_| hook_error("Plugin publication gate is poisoned."))?;
            self.ensure_current_persistent_generation(
                &runtime.plugin.id,
                runtime.plugin.generation,
            )?;
            match owner.host.runtime().plugin(&runtime.plugin.id) {
                Ok(_) => {}
                Err(error) if error.code == EmbeddedErrorCode::NotFound => {
                    // Derive plugin aggregate limits from the one parent configuration.
                    // 从唯一父级配置派生插件聚合上限。
                    let parent = &owner.settings.parent;
                    owner
                        .host
                        .runtime()
                        .register_plugin(
                            runtime.plugin.id.clone(),
                            EmbeddedPluginConfig {
                                max_registered_pools: parent.max_registered_pools,
                                max_sessions: parent.max_sessions,
                                max_resident_vms: parent.max_resident_vms,
                                max_running_calls: parent.max_running_calls,
                                max_queued_calls: parent.max_queued_calls,
                                max_queued_bytes: parent.max_queued_bytes,
                                max_operations: parent.max_operations,
                            },
                        )
                        .map_err(core_error)?;
                }
                Err(error) => return Err(core_error(error)),
            }
        }
        // Optional Hook shutdown differs from the mandatory tool shutdown export.
        // 可选 Hook 关闭不同于必填工具关闭导出。
        let finalizer = lua.hook_shutdown_function.as_ref().map(|export| ModuleFinalizer {
            export: export.clone(),
            arguments: json!({
                "payload_json": json!({"event":"hook_runtime_shutdown","plugin_id":runtime.plugin.id}).to_string(),
                "host_timeout_ms": owner.settings.shutdown_timeout_ms,
            }),
            timeout_ms: owner.settings.shutdown_timeout_ms,
        });
        // Register the original execution domain before admitting its fixed session.
        // 接纳固定会话之前注册原执行域。
        let pool = owner
            .host
            .register_bundle_pool(VulcanPluginBundlePoolRequest {
                plugin_id: runtime.plugin.id.clone(),
                plugin_generation: runtime.plugin.generation,
                workspace_root: workspace.clone(),
                security_partition: format!(
                    "hook:{}:{}",
                    runtime.plugin.id, runtime.plugin.generation
                ),
                scope: VulcanPluginBridgeLeaseScope::Enabled,
                policy: PluginPoolConfig {
                    kind: PoolKind::Shared,
                    min_resident_vms: 0,
                    max_resident_vms: 1,
                    max_running_calls: 1,
                    max_queued_calls: owner.settings.parent.max_queued_calls,
                    reuse: InstanceReuse::Session,
                    serial: true,
                    backend: ExecutionBackend::InProcess,
                    idle_ttl_ms: None,
                    max_uses: None,
                },
                finalizer,
            })
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
            .map_err(|_| hook_error("Plugin publication gate is poisoned."))?;
        self.ensure_current_persistent_generation(&runtime.plugin.id, runtime.plugin.generation)?;
        if runtime.closing.load(Ordering::Acquire) {
            return Err(hook_error("Hook cancelled before core admission."));
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
            .ok_or_else(|| hook_error("Formal plugin runtime is not attached."))?;
        // Admission shares catalog ordering and publishes actual core ownership before releasing the state lock.
        // 入场共享目录排序，并在释放状态锁前发布真实核心所有权。
        let mut observation = {
            // Order this state transition against catalog generation publication.
            // 使当前状态转换与目录代次发布保持顺序。
            let _publication = self
                .state
                .session_registration_gate
                .lock()
                .map_err(|_| hook_error("Plugin publication gate is poisoned."))?;
            self.ensure_current_hook_generation(hook)?;
            // Keep the exact lifecycle evidence within this synchronous lock scope.
            // 在当前同步锁作用域内保留精确生命周期证据。
            let mut state = runtime.lock()?;
            if runtime.closing.load(Ordering::Acquire) || state.retired {
                return Err(hook_error("Hook instance is closing."));
            }
            // Issue a checked monotonic correlation that Lua cannot replace.
            // 签发 Lua 不能替换的受检单调关联。
            let request = self
                .state
                .next_request_id
                .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                    value.checked_add(1)
                })
                .map_err(|_| hook_error("Plugin request identity is exhausted."))?;
            // Absence explicitly means package scope; invalid supplied metadata cannot fall back.
            // 缺省明确表示包作用域；无效已提供元数据不能回退。
            let workspace = match workspace {
                Some(workspace) => workspace,
                None => state
                    .workspace
                    .clone()
                    .ok_or_else(|| hook_error("Prepared Hook has no workspace."))?,
            };
            // Freeze the declared export, arguments and host request authority together.
            // 共同冻结已声明导出、参数及宿主请求权威。
            let call = EmbeddedCall {
                pool_id: state
                    .pool
                    .clone()
                    .ok_or_else(|| hook_error("Prepared Hook has no pool."))?,
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
                .ok_or_else(|| hook_error("Prepared Hook has no session."))?;
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
                HookOperation {
                    handle: handle.clone(),
                    acknowledged: Arc::clone(&acknowledged),
                },
            );
            HookObservation {
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
                return Err(hook_error(
                    "Hook execution timed out; actual work remains retained.",
                ));
            }
            tokio::time::sleep(OBSERVATION_INTERVAL).await;
        }
    }
}
