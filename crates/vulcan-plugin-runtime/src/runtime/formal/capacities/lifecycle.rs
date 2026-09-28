//! Lifecycle retirement and reactivation preserve physical ownership and whole configured guarantees.
//! 生命周期回收及重新启用保留物理归属和完整配置保证。

use super::*;

impl ManagedFormalRuntime {
    /// Retire plugin_id's capacities under the caller-held publication gate; return only after all identities are removed.
    /// 在调用方持有的发布门下回收 plugin_id 的容量；仅在全部身份移除后成功返回。
    /// Callers must finish lifecycle callbacks and fence business admission before requesting retirement.
    /// 调用方必须先完成生命周期回调并阻止业务入场，再请求回收。
    pub(in crate::runtime) fn retire_plugin_capacities(
        &self,
        plugin_id: &str,
        _publication: &std::sync::MutexGuard<'_, ()>,
    ) -> Result<(), VulcanPluginRuntimeError> {
        // One lock orders lifecycle intent, unfinished host preparation and native member metadata.
        // 同一锁排序生命周期意图、未完成宿主准备及原生成员元数据。
        let mut registry = self.capacities.lock().map_err(|_| capacity_lock_error())?;
        registry.retiring_plugins.insert(plugin_id.to_owned());
        self.retire_registered_capacities(plugin_id, &mut registry)
    }

    /// Observe whether plugin_id retains explicit unfinished lifecycle retirement; return a lock diagnostic on uncertainty.
    /// 观测 plugin_id 是否保留明确未完成生命周期回收；归属不确定时返回锁诊断。
    pub(in crate::runtime) fn has_retiring_capacities(
        &self,
        plugin_id: &str,
    ) -> Result<bool, VulcanPluginRuntimeError> {
        Ok(self
            .capacities
            .lock()
            .map_err(|_| capacity_lock_error())?
            .retiring_plugins
            .contains(plugin_id))
    }

    /// Restore every explicit guarantee before plugin_id enters Enabling; publication excludes competing host preparation.
    /// 在 plugin_id 进入启用中状态前恢复全部显式保证；发布门排除竞争宿主准备。
    /// Return an error without publishing activation; newly created empty capacities are rolled back on partial failure.
    /// 失败不发布启用；局部失败时回滚新建空容量。
    pub(in crate::runtime) fn prepare_plugin_capacities(
        &self,
        plugin_id: &str,
        publication: &std::sync::MutexGuard<'_, ()>,
    ) -> Result<(), VulcanPluginRuntimeError> {
        // A failed prior lifecycle release is retried explicitly by reactivation, never by ordinary admission.
        // 重新启用显式重试此前失败的生命周期释放；普通入场绝不重试。
        let mut registry = self.capacities.lock().map_err(|_| capacity_lock_error())?;
        if registry.retiring_plugins.contains(plugin_id) {
            self.retire_registered_capacities(plugin_id, &mut registry)?;
        }
        // Unconfigured purposes remain explicitly lazy and do not allocate metadata during activation.
        // 未配置用途保持明确延迟创建，不在启用期间分配元数据。
        let Some(configured) = self.settings.capacity_budgets.get(plugin_id) else {
            return Ok(());
        };
        if configured.is_empty() {
            return Ok(());
        }
        self.ensure_plugin_budget(plugin_id, publication)?;
        for purpose in configured.keys() {
            if let Some(registration) = registry
                .registrations
                .get(&(plugin_id.to_owned(), *purpose))
                && self
                    .verify_capacity(plugin_id, *purpose, registration)?
                    .closing
            {
                return Err(VulcanPluginRuntimeError::new(
                    "Plugin purpose capacity admission is permanently closed.",
                ));
            }
        }
        // Only identities created by this activation may be removed by its compensation.
        // 此次启用的补偿仅可移除此启用创建的身份。
        let mut created = Vec::new();
        for (purpose, config) in configured {
            // The exact semantic owner remains independent of package generation.
            // 精确语义所有者保持独立于包代次。
            let key = (plugin_id.to_owned(), *purpose);
            if registry.registrations.contains_key(&key) {
                continue;
            }
            match self
                .host
                .runtime()
                .register_capacity(plugin_id, config.clone())
            {
                Ok(identity) => {
                    registry.registrations.insert(
                        key.clone(),
                        CapacityRegistration {
                            identity,
                            preparing_members: 0,
                        },
                    );
                    created.push(key);
                }
                Err(error) => {
                    // Preserve every failed rollback identity instead of hiding incomplete release.
                    // 保留每个回滚失败身份，不隐藏未完成释放。
                    let mut failures = Vec::new();
                    for created_key in created {
                        // This activation has not admitted any member to its unpublished capacity.
                        // 此次启用尚未向未发布容量接纳任何成员。
                        let registration = registry
                            .registrations
                            .get(&created_key)
                            .expect("activation retains its created capacity");
                        match self
                            .host
                            .runtime()
                            .close_capacity(&registration.identity)
                            .and_then(|()| {
                                self.host.runtime().forget_capacity(&registration.identity)
                            }) {
                            Ok(()) => {
                                registry.registrations.remove(&created_key);
                            }
                            Err(cleanup) => failures.push(cleanup.to_string()),
                        }
                    }
                    if failures.is_empty() {
                        return Err(calls::core_error(error));
                    }
                    registry.retiring_plugins.insert(plugin_id.to_owned());
                    return Err(VulcanPluginRuntimeError::new(format!(
                        "{error}; capacity activation rollback remains unfinished: {}",
                        failures.join("; ")
                    )));
                }
            }
        }
        Ok(())
    }

    /// Remove plugin_id's original registrations from registry after proving every member and preparation ended.
    /// 证明全部成员及准备结束后，从 registry 移除 plugin_id 的原注册。
    /// Failed preflight closes nothing; failed native removal retains the exact remaining identities and retirement intent.
    /// 前置检查失败不关闭任何容量；原生移除失败保留精确剩余身份及回收意图。
    fn retire_registered_capacities(
        &self,
        plugin_id: &str,
        registry: &mut CapacityRegistry,
    ) -> Result<(), VulcanPluginRuntimeError> {
        // Collect only this exact plugin's semantic owners; other plugin guarantees remain untouched.
        // 仅收集此精确插件的语义所有者；其他插件保证保持不变。
        let keys: Vec<_> = registry
            .registrations
            .keys()
            .filter(|(owner, _)| owner == plugin_id)
            .cloned()
            .collect();
        for key in &keys {
            // Native state remains the sole source for retained members and physical ownership.
            // 原生状态继续作为保留成员及物理归属的唯一来源。
            let registration = registry.registrations.get(key).expect("retained capacity");
            if registration.preparing_members != 0 {
                return Err(VulcanPluginRuntimeError::new(
                    "Plugin capacity retirement still owns unfinished member preparation.",
                ));
            }
            if self
                .verify_capacity(plugin_id, key.1, registration)?
                .retained_pools
                != 0
            {
                return Err(VulcanPluginRuntimeError::new(
                    "Plugin capacity retirement still owns retained member pools.",
                ));
            }
        }
        for key in keys {
            // Empty native owners can be closed without running Lua or waiting for asynchronous cleanup under this lock.
            // 空原生所有者可以关闭，无需在此锁下执行 Lua 或等待异步清理。
            let registration = registry
                .registrations
                .get(&key)
                .expect("preflight retained capacity");
            self.host
                .runtime()
                .close_capacity(&registration.identity)
                .map_err(calls::core_error)?;
            self.host
                .runtime()
                .forget_capacity(&registration.identity)
                .map_err(calls::core_error)?;
            // Acknowledge each completed native removal before attempting another fallible step.
            // 尝试另一个可能失败的步骤前，确认每次完成的原生移除。
            registry.registrations.remove(&key);
        }
        registry.retiring_plugins.remove(plugin_id);
        Ok(())
    }
}
