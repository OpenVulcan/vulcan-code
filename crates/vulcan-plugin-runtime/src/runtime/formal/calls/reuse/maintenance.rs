//! Weakly owned idle cleanup and explicit observation of retained finalization failures.
//! 弱持有空闲清理，以及保留关闭失败的显式观测。

use super::*;
use luaskills::runtime::embedded::OperationContext;

/// Maximum retry interval for unhealthy maintenance; normal observation uses the shared call interval.
/// 不健康维护的最大重试间隔；正常观测使用共用调用间隔。
const MAX_MAINTENANCE_RETRY: Duration = Duration::from_secs(1);

/// Reset observer ownership even if its asynchronous supervisor is aborted.
/// 即使异步监督者被中止，也重置观测者归属。
struct ObserverGuard(std::sync::Weak<ManagedFormalRuntime>);
impl Drop for ObserverGuard {
    /// Release only the observer flag; domain and native ownership remain retained for recovery.
    /// 仅释放观测者标志；域及原生所有权继续保留供恢复。
    fn drop(&mut self) {
        if let Some(owner) = self.0.upgrade() {
            owner.reuse_observer.store(false, Ordering::Release);
        }
    }
}

/// Collect independent terminal shutdowns for domain; return newly retained diagnostics.
/// 收集 domain 的独立终态关闭；返回新保留诊断。
/// Business records belong to controller calls and are never acknowledged by this observer.
/// 业务记录属于控制器调用，此观测者绝不确认它们。
fn collect_finalizations(
    owner: &ManagedFormalRuntime,
    domain: &mut Domain,
    pool: &str,
) -> Result<Vec<(String, String)>, VulcanPluginRuntimeError> {
    // The core's own retained-operation limit bounds the complete page and failure identities.
    // 核心自身保留操作上限约束完整页及失败身份。
    let page = owner
        .host
        .runtime()
        .list_operations(Some(pool), None, owner.settings.parent.max_operations)
        .map_err(core_error)?;
    let mut diagnostics = Vec::new();
    // Completion publication can precede the last physical acknowledgement checkpoint.
    // 完成发布可能早于最后一个物理确认检查点。
    let mut pending = false;
    for id in page.operation_ids {
        // A controller can concurrently acknowledge a listed business record.
        // 控制器可能并发确认已列出的业务记录。
        let operation = match owner.host.runtime().operation(&id) {
            Ok(operation) => operation,
            Err(error) if error.code == EmbeddedErrorCode::NotFound => continue,
            Err(error) => return Err(core_error(error)),
        };
        let snapshot = operation.snapshot().map_err(core_error)?;
        if !matches!(&snapshot.context, OperationContext::Module(context) if context.finalization_instance_id.is_some())
        {
            continue;
        }
        // The original domain owns closing, independently of any last business invocation.
        // 原域拥有关闭，独立于任何最后业务调用。
        crate::runtime::native_identity::observe_snapshot(
            owner.host.runtime().id(),
            &snapshot,
            crate::runtime::native_identity::NativeIdentityOwner {
                plugin_generation: domain.key.generation,
                controller_session_id: Some(&domain.key.controller),
                host_invocation_id: None,
                host_session_id: None,
            },
        );
        if !snapshot.phase.is_terminal() {
            pending = true;
            continue;
        }
        let outcome = snapshot
            .finalization
            .as_ref()
            .and_then(|finalization| finalization.outcome.as_ref())
            .ok_or_else(|| {
                VulcanPluginRuntimeError::new("Terminal reusable finalization has no outcome.")
            })?;
        match outcome.result() {
            Ok(_) => match owner.host.runtime().forget_operation(&id) {
                Ok(()) => {}
                Err(error) if error.code == EmbeddedErrorCode::Busy => pending = true,
                Err(error) => return Err(core_error(error)),
            },
            Err(error) => {
                domain.closing = true;
                // Remember publication separately; a failed diagnostic write must retry the same evidence.
                // 分别记住发布状态；诊断写入失败必须重试同一证据。
                let reported = domain.failures.entry(id.clone()).or_insert(false);
                if !*reported {
                    diagnostics.push((
                        id.clone(),
                        format!("Reusable domain shutdown {id} failed: {error}"),
                    ));
                }
            }
        }
    }
    domain.pending_finalizations = pending;
    Ok(diagnostics)
}

impl VulcanPluginRuntimeManager {
    /// Advance domain cleanup under publication; return the original core or ownership error.
    /// 在发布门下推进域清理；返回原核心或归属错误。
    /// owner retains actual resources and publication orders diagnostics against lifecycle and acknowledgement.
    /// owner 保留真实资源，publication 将诊断与生命周期和确认排序。
    fn maintain_reusable_domains_locked(
        &self,
        owner: &ManagedFormalRuntime,
        publication: &std::sync::MutexGuard<'_, ()>,
    ) -> Result<(), VulcanPluginRuntimeError> {
        let snapshot = self.snapshot();
        // Closed capability authority explicitly retires every domain; cleanup no longer needs a new snapshot.
        // 能力权威关闭时明确退役全部域；清理不再需要新快照。
        let capabilities = match owner.host.runtime().capabilities().snapshot() {
            Ok(snapshot) => Some(snapshot.revision()),
            Err(error) if error.code == EmbeddedErrorCode::Closed => None,
            Err(error) => return Err(core_error(error)),
        };
        let mut diagnostics = BTreeMap::new();
        {
            let mut registry = owner.reusable.lock().map_err(|_| calls_error())?;
            for (domain_id, domain) in &mut registry.domains {
                // Only active domains need current policy; retired failure evidence survives explicit capacity forgetting.
                // 仅活动域需要当前策略；已退役失败证据在显式遗忘容量后仍保留。
                if !domain.closing {
                    // Native policy changes retire old bindings without cancelling their current business borrowers.
                    // 原生策略变化退役旧绑定，不取消其当前业务借用者。
                    let policy = self
                        .formal_capacity_status(
                            &domain.plugin_id,
                            VulcanPluginCapacityPurpose::ToolCall,
                        )?
                        .ok_or_else(|| {
                            VulcanPluginRuntimeError::new(
                                "Active reusable domain lost its capacity owner.",
                            )
                        })?;
                    if policy.snapshot.closing
                        || policy.policy_revision != domain.key.capacity_policy
                    {
                        domain.closing = true;
                    }
                }
                let current = snapshot.plugins.get(&domain.plugin_id);
                if current.is_none_or(|plugin| {
                    plugin.generation != domain.key.generation
                        || plugin.runtime_state != crate::VulcanPluginRuntimeState::Enabled
                }) || capabilities.as_ref() != Some(&domain.key.capabilities)
                    || (domain.borrowers == 0 && domain.idle_since.elapsed() >= domain.idle_ttl)
                {
                    domain.closing = true;
                }
                if domain.preparing {
                    continue;
                }
                if let Some(pool) = domain.pool.clone() {
                    for (operation, message) in collect_finalizations(owner, domain, &pool)? {
                        diagnostics.insert(
                            operation,
                            (domain_id.clone(), domain.plugin_id.clone(), message),
                        );
                    }
                    if domain.closing && !domain.pool_retired && domain.borrowers == 0 {
                        owner.host.runtime().close_pool(&pool).map_err(core_error)?;
                        if domain.borrowers == 0 {
                            match owner.host.runtime().forget_pool(&pool) {
                                Ok(()) => {
                                    domain.pool_retired = true;
                                    // Physical forgetting excludes future shutdown publication; collect the last raced completion.
                                    // 物理遗忘排除后续关闭发布；收集最后一个竞态完成结果。
                                    for (operation, message) in
                                        collect_finalizations(owner, domain, &pool)?
                                    {
                                        diagnostics.insert(
                                            operation,
                                            (domain_id.clone(), domain.plugin_id.clone(), message),
                                        );
                                    }
                                }
                                Err(error) if error.code == EmbeddedErrorCode::Busy => {}
                                Err(error) => return Err(core_error(error)),
                            }
                        }
                    }
                }
            }
            registry.domains.retain(|_, domain| {
                domain.preparing
                    || domain.borrowers > 0
                    || (domain.pool.is_some() && !domain.pool_retired)
                    || !domain.failures.is_empty()
                    || domain.pending_finalizations
            });
        }
        // Keep the original publication guard until diagnostics are visible, while releasing registry ownership first.
        // 保留原发布守卫直至诊断可见，同时先释放注册表归属。
        for (operation, (domain_id, plugin, message)) in diagnostics {
            self.record_domain_cleanup_locked(&plugin, message, &domain_id, true, publication)?;
            // Acknowledgement shares this publication gate, so the original failure cannot disappear here.
            // 确认共用此发布门，因此原失败不能在此消失。
            let mut registry = owner.reusable.lock().map_err(|_| calls_error())?;
            let reported = registry
                .domains
                .get_mut(&domain_id)
                .and_then(|domain| domain.failures.get_mut(&operation))
                .ok_or_else(|| {
                    VulcanPluginRuntimeError::new(
                        "Reusable failure ownership disappeared before diagnostic publication.",
                    )
                })?;
            *reported = true;
        }
        Ok(())
    }

    /// Start one weak observer using the caller's live Tokio runtime; no manager is retained across waits.
    /// 使用调用方活跃 Tokio 运行时启动单个弱观测者；等待期间不保留管理器。
    /// Existing domain identities remain recoverable if the executor later stops.
    /// 执行器随后停止时，既有域身份仍可恢复。
    pub(in crate::runtime::formal) fn ensure_reuse_observer(
        &self,
        owner: &Arc<ManagedFormalRuntime>,
    ) {
        if owner.reuse_observer.swap(true, Ordering::AcqRel) {
            return;
        }
        // The guard lives in the future before spawn, so cancellation before its first poll is covered.
        // 守卫在 spawn 前位于 future 中，因此覆盖首次轮询前取消。
        let guard = ObserverGuard(Arc::downgrade(owner));
        let manager = Arc::downgrade(&self.state);
        // A supervisor that cannot start drops the future, and its guard clears the claim for a retry.
        // 监督器无法启动时会丢弃 future，其守卫清除占用以便重试。
        let Ok(supervisor) = self.supervisor_handle() else {
            return;
        };
        supervisor.spawn(async move {
            let _guard = guard;
            // Bound unhealthy retries without relinquishing ownership or repeatedly spawning supervisors.
            // 限制不健康重试，不放弃归属，也不反复派生监督者。
            let mut retry_interval = OBSERVATION_INTERVAL;
            loop {
                {
                    let (Some(state), Some(owner)) = (manager.upgrade(), _guard.0.upgrade()) else {
                        break;
                    };
                    let manager = VulcanPluginRuntimeManager { state };
                    retry_interval = match manager.maintain_reusable_domains(&owner) {
                        Ok(()) => OBSERVATION_INTERVAL,
                        Err(_) => retry_interval.saturating_mul(2).min(MAX_MAINTENANCE_RETRY),
                    };
                    if owner
                        .reusable
                        .lock()
                        .is_ok_and(|registry| registry.closing && registry.domains.is_empty())
                    {
                        break;
                    }
                }
                tokio::time::sleep(retry_interval).await;
            }
        });
    }

    /// Advance actual retirement and collect independent shutdown evidence; no Lua runs under host locks.
    /// 推进实际退役并收集独立关闭证据；宿主锁内不执行 Lua。
    /// Returns precise errors while leaving every unfinished identity available for retry.
    /// 返回精确错误，并保留每个未完成身份以供重试。
    pub(in crate::runtime::formal) fn maintain_reusable_domains(
        &self,
        owner: &ManagedFormalRuntime,
    ) -> Result<(), VulcanPluginRuntimeError> {
        // Publish health and cleanup evidence under the same gate used by final business admission.
        // 在最终业务入场使用的同一门控下发布健康状态及清理证据。
        let publication =
            self.state.session_registration_gate.lock().map_err(|_| {
                VulcanPluginRuntimeError::new("Plugin publication gate is poisoned.")
            })?;
        // A failed pass preserves exact ownership and cannot be mistaken for healthy admission.
        // 失败轮次保留精确归属，不能被误认为健康入场。
        let result = self.maintain_reusable_domains_locked(owner, &publication);
        // Store only the latest health error; physical failure records remain in their original core operations.
        // 仅保存最新健康错误；物理失败记录仍位于原核心操作中。
        let mut registry = owner.reusable.lock().map_err(|_| {
            VulcanPluginRuntimeError::new("Reusable domain registry lock is poisoned.")
        })?;
        registry.observer_error = result.as_ref().err().map(ToString::to_string);
        result
    }

    /// Close matching domains under caller-held publication; return without waiting for native callbacks.
    /// 在调用方持有发布门时关闭匹配域；不等待原生回调即返回。
    /// None selects all owners; a parent close additionally prevents future preparation.
    /// None 选择全部所有者；父级关闭还阻止后续准备。
    pub(in crate::runtime) fn close_reusable_domains(
        &self,
        plugin: Option<&str>,
        controller: Option<&str>,
        parent: bool,
    ) -> Result<(), VulcanPluginRuntimeError> {
        let Some(owner) = self.formal_runtime_owner()? else {
            return Ok(());
        };
        let mut registry = owner.reusable.lock().map_err(|_| calls_error())?;
        registry.closing |= parent;
        for domain in registry.domains.values_mut().filter(|domain| {
            plugin.is_none_or(|id| domain.plugin_id == id)
                && controller.is_none_or(|id| domain.key.controller == id)
        }) {
            domain.closing = true;
            if !domain.pool_retired
                && let Some(pool) = &domain.pool
            {
                owner.host.runtime().close_pool(pool).map_err(core_error)?;
            }
        }
        Ok(())
    }

    /// Return whether plugin still owns a pending, live, or unacknowledged reusable domain.
    /// 返回 plugin 是否仍拥有准备中、活跃或未确认的可复用域。
    /// This read cannot hide finalization failures after physical VM retirement.
    /// 此读取不能在物理 VM 退役后隐藏关闭失败。
    pub(in crate::runtime) fn has_reusable_domains(
        &self,
        plugin: &str,
    ) -> Result<bool, VulcanPluginRuntimeError> {
        let Some(owner) = self.formal_runtime_owner()? else {
            return Ok(false);
        };
        Ok(owner
            .reusable
            .lock()
            .map_err(|_| calls_error())?
            .domains
            .values()
            .any(|domain| domain.plugin_id == plugin))
    }

    /// Observe only domains owned by controller; return actual core shutdown failures without secret configuration.
    /// 仅观测 controller 所有域；返回真实核心关闭失败，不含私密配置。
    pub fn reusable_tool_domains(
        &self,
        controller: &str,
    ) -> Result<Vec<VulcanPluginReusableDomainStatus>, VulcanPluginRuntimeError> {
        let Some(owner) = self.formal_runtime_owner()? else {
            return Ok(Vec::new());
        };
        let registry = owner.reusable.lock().map_err(|_| calls_error())?;
        registry
            .domains
            .iter()
            .filter(|(_, domain)| domain.key.controller == controller)
            .map(|(id, domain)| {
                let failures = domain
                    .failures
                    .keys()
                    .map(|id| {
                        owner
                            .host
                            .runtime()
                            .operation(id)
                            .and_then(|operation| operation.snapshot())
                            .map_err(core_error)
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(VulcanPluginReusableDomainStatus {
                    domain_id: id.clone(),
                    plugin_id: domain.plugin_id.clone(),
                    plugin_generation: domain.key.generation,
                    tool_id: domain.key.tool.clone(),
                    retained_input_bytes: domain.input_bytes,
                    pool_id: domain.pool.clone(),
                    pool_retired: domain.pool_retired,
                    pending_finalizations: domain.pending_finalizations,
                    preparing: domain.preparing,
                    borrowers: domain.borrowers,
                    closing: domain.closing,
                    finalization_failures: failures,
                    observer_error: registry.observer_error.clone(),
                })
            })
            .collect()
    }

    /// Explicitly acknowledge a physically retired domain owned by controller; return errors without erasing active work.
    /// 明确确认 controller 所有且已物理退役的域；活动工作存在时返回错误，不抹除。
    /// Failed core shutdown records are forgotten only through this exact owner-selected operation.
    /// 仅通过此精确所有者选择操作遗忘失败核心关闭记录。
    pub fn forget_reusable_tool_domain(
        &self,
        domain_id: &str,
        controller: &str,
    ) -> Result<(), VulcanPluginRuntimeError> {
        let publication = self
            .state
            .session_registration_gate
            .lock()
            .map_err(|_| calls_error())?;
        let owner = self.required_formal_owner()?;
        let plugin = {
            let mut registry = owner.reusable.lock().map_err(|_| calls_error())?;
            let domain = registry
                .domains
                .get_mut(domain_id)
                .filter(|domain| domain.key.controller == controller)
                .ok_or_else(|| {
                    VulcanPluginRuntimeError::new(
                        "Reusable domain is not owned by this controller.",
                    )
                })?;
            if domain.preparing
                || domain.borrowers > 0
                || (domain.pool.is_some() && !domain.pool_retired)
                || domain.pending_finalizations
            {
                return Err(VulcanPluginRuntimeError::new(
                    "Reusable domain has not physically retired.",
                ));
            }
            // Remove evidence one acknowledged core record at a time so a partial failure remains recoverable.
            // 逐个移除已确认核心记录，使局部失败仍可恢复。
            for id in domain.failures.keys().cloned().collect::<Vec<_>>() {
                owner
                    .host
                    .runtime()
                    .forget_operation(&id)
                    .map_err(core_error)?;
                domain.failures.remove(&id);
            }
            let plugin = domain.plugin_id.clone();
            registry.domains.remove(domain_id);
            plugin
        };
        self.acknowledge_domain_cleanup_locked(&plugin, domain_id, &publication)
    }

    /// Drain exact plugin/controller domains within the frozen shutdown budget; return retained failures and counts.
    /// 在冻结关闭预算内排空精确插件／控制器域；返回保留失败及计数。
    pub(in crate::runtime) async fn drain_reusable_domains(
        &self,
        plugin: Option<&str>,
        controller: Option<&str>,
    ) -> Result<VulcanPluginDrainReport, VulcanPluginRuntimeError> {
        Ok(self
            .observe_reusable_domains(plugin, controller)
            .await?
            .report)
    }

    /// Observe plugin/controller's exact domains; return physical pending separately from retained shutdown failures.
    /// 观测 plugin/controller 的精确域；分别返回物理待完成与保留关闭失败。
    pub(in crate::runtime) async fn observe_reusable_domains(
        &self,
        plugin: Option<&str>,
        controller: Option<&str>,
    ) -> Result<crate::runtime::drain::DrainObservation, VulcanPluginRuntimeError> {
        let Some(owner) = self.formal_runtime_owner()? else {
            return Ok(crate::runtime::drain::DrainObservation::default());
        };
        self.close_reusable_domains(plugin, controller, false)?;
        let deadline = Instant::now()
            .checked_add(Duration::from_millis(owner.settings.shutdown_timeout_ms))
            .ok_or_else(|| {
                VulcanPluginRuntimeError::new("Reuse drain deadline is out of range.")
            })?;
        // Preserve real shutdown failures independently from still-live borrowers and pools.
        // 独立于仍存活借用者及池保留真实关闭失败。
        let mut observation = crate::runtime::drain::DrainObservation::default();
        loop {
            self.maintain_reusable_domains(&owner)?;
            // Completed is decided synchronously; the publication guard never crosses an await.
            // completed 同步决定；发布守卫绝不跨越 await。
            let completed = {
                // Publication orders the actual snapshot and its diagnostic write against explicit domain ACK.
                // publication 将实际快照及其诊断写入与显式域确认排序。
                let publication = self
                    .state
                    .session_registration_gate
                    .lock()
                    .map_err(|_| calls_error())?;
                // Pending freezes the actual domain records while ACK is excluded, releasing the registry before accounting.
                // pending 在排除确认时固定实际域记录，记账之前释放注册表。
                let pending = {
                    let registry = owner.reusable.lock().map_err(|_| calls_error())?;
                    registry
                        .domains
                        .iter()
                        .filter(|(_, domain)| {
                            plugin.is_none_or(|id| domain.plugin_id == id)
                                && controller.is_none_or(|id| domain.key.controller == id)
                        })
                        .map(|(id, domain)| {
                            (
                                id.clone(),
                                domain.plugin_id.clone(),
                                domain.preparing
                                    || domain.borrowers > 0
                                    || (domain.pool.is_some() && !domain.pool_retired)
                                    || domain.pending_finalizations,
                                !domain.failures.is_empty(),
                            )
                        })
                        .collect::<Vec<_>>()
                };
                if pending.iter().all(|(_, _, physical, _)| !physical) || Instant::now() >= deadline
                {
                    for (id, plugin, physical, failed) in pending {
                        if physical || failed {
                            // Message preserves the original public bytes while typed source selects accounting.
                            // message 保留原公开字节，由类型化来源选择记账。
                            let message = format!(
                                "Reusable domain {id} retains unfinished cleanup or unacknowledged shutdown failure."
                            );
                            self.record_domain_cleanup_locked(
                                &plugin,
                                message.clone(),
                                &id,
                                failed,
                                &publication,
                            )?;
                            if failed {
                                observation.failed(message);
                            } else {
                                // This branch is reached at the finite deadline with physical work still retained.
                                // 此分支在有限截止时间到达且物理工作仍保留时进入。
                                observation.pending(message);
                            }
                        }
                    }
                    true
                } else {
                    false
                }
            };
            if completed {
                return Ok(observation);
            }
            tokio::time::sleep(OBSERVATION_INTERVAL).await;
        }
    }
}
