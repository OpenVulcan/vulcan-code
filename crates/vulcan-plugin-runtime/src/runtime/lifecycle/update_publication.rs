//! Original target staging and synchronous extension handoff preserve closed business admission.
//! 原目标暂存及同步扩展交接保持业务入场关闭。

use super::*;

mod publication;
pub use publication::VulcanPluginActivationTargetPublication;

impl VulcanPluginRuntimeManager {
    /// Reserve a monotonic handoff revision from the sole manager catalog counter without wrapping.
    /// 从唯一管理器目录计数器预留单调交接修订，不发生回绕。
    /// No inputs; returns the reserved revision or exhaustion before any publisher executes.
    /// 无输入；返回预留修订，或在任何发布器执行前报告耗尽。
    fn reserve_activation_catalog_revision(&self) -> Result<u64, VulcanPluginRuntimeError> {
        self.state
            .next_revision
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |revision| {
                revision.checked_add(1)
            })
            .map_err(|_| VulcanPluginRuntimeError::new("Plugin catalog revision is exhausted."))
    }

    /// Stage the exact published target with closed business surfaces and its final routing generation.
    /// 暂存精确已发布目标，保持业务暴露面关闭，并固定最终路由代次。
    /// transaction owns the original request; policy sets are the caller's serialized current configuration.
    /// transaction 拥有原请求；策略集合是调用方串行化的当前配置。
    /// Returns an Enabling catalog for private MCP and extension preparation; never runs callbacks or completes the request.
    /// 返回启用中目录，供私有 MCP 与扩展准备；绝不运行回调或完成请求。
    pub fn stage_activation_target_handoff(
        &self,
        transaction: &crate::VulcanPluginActivationTransaction,
        disabled_plugin_ids: &BTreeSet<String>,
        enabled_plugin_ids: &BTreeSet<String>,
    ) -> Result<Arc<VulcanPluginCatalogSnapshot>, VulcanPluginRuntimeError> {
        // Exclude registration and every ordinary lifecycle writer across ownership and projection.
        // 在所有权及投影全程排除注册与全部普通生命周期写入器。
        let registration =
            self.state.session_registration_gate.lock().map_err(|_| {
                VulcanPluginRuntimeError::new("Plugin publication gate is poisoned.")
            })?;
        self.enabled_activation_publication_store(transaction)?;
        // The original target is the sole package and namespace authority.
        // 原目标是唯一插件包及命名空间权威。
        let target = transaction.record().target();
        if !enabled_plugin_ids.contains(&target.plugin_id)
            || disabled_plugin_ids.contains(&target.plugin_id)
        {
            return Err(VulcanPluginRuntimeError::new(
                "Enabled target handoff requires explicit enabled policy.",
            ));
        }
        // Repeated staging of the same closed target preserves its actual route generation.
        // 重复暂存同一关闭目标保留其实际路由代次。
        let current = self.snapshot();
        if current
            .plugins
            .get(&target.plugin_id)
            .is_some_and(|plugin| {
                plugin.runtime_state == VulcanPluginRuntimeState::Enabling
                    && plugin.availability == VulcanPluginAvailability::Enabled
                    && plugin.runtime_execution_available
                    && plugin.desired_enabled
                    && plugin
                        .managed_package()
                        .is_some_and(|package| package.matches_activation(target))
            })
        {
            self.validate_plugin_requirements(&current.plugins[&target.plugin_id])?;
            return Ok(current);
        }
        if self.has_unretired_plugin_resources_locked(&target.plugin_id, &registration)? {
            return Err(VulcanPluginRuntimeError::new(
                "Original plugin resources must retire before target handoff staging.",
            ));
        }
        // Discovery reads the selected immutable package; it cannot substitute an unselected candidate.
        // 发现读取已选不可变插件包；不能替换为未选候选。
        let mut snapshot =
            self.scan_runtime_catalog(disabled_plugin_ids, enabled_plugin_ids, &registration);
        // The exact target must be valid before any capacity or lifecycle mutation.
        // 精确目标必须在任何容量或生命周期变更前有效。
        let plugin = snapshot.plugins.get(&target.plugin_id).ok_or_else(|| {
            VulcanPluginRuntimeError::new("Selected target is absent from discovery.")
        })?;
        if plugin.availability != VulcanPluginAvailability::Enabled
            || !plugin.desired_enabled
            || !plugin
                .managed_package()
                .is_some_and(|package| package.matches_activation(target))
        {
            return Err(VulcanPluginRuntimeError::new(
                "Selected target differs from the original enabled activation.",
            ));
        }
        self.validate_plugin_requirements(plugin)?;
        // Reserve one checked catalog revision before changing capacities or projection state.
        // 在改变容量或投影状态之前预留单个经过范围检查的目录修订。
        let revision = self.reserve_activation_catalog_revision()?;
        if let Some(owner) = self.formal_runtime_owner()? {
            owner.prepare_plugin_capacities(&target.plugin_id, &registration)?;
        }
        // Project real generation and other namespaces without suppressing unrelated recovery obligations.
        // 投影真实代次及其他命名空间，不压制无关恢复义务。
        self.project_runtime_state(&mut snapshot, Some(transaction.record()))?;
        // Recheck the resulting authority before replacing any public pointer.
        // 在替换任何公开指针前重新核对所得权威。
        let plugin = snapshot.plugins.get_mut(&target.plugin_id).ok_or_else(|| {
            VulcanPluginRuntimeError::new("Selected target disappeared during projection.")
        })?;
        if plugin.runtime_state == VulcanPluginRuntimeState::Unavailable
            || !plugin.runtime_execution_available
        {
            return Err(VulcanPluginRuntimeError::new(
                "Selected target cannot satisfy runtime publication requirements.",
            ));
        }
        plugin.runtime_state = VulcanPluginRuntimeState::Enabling;
        rebuild_enabled_surface(&mut snapshot);
        // Preserve the single lifecycle source so ordinary refresh also keeps this target closed.
        // 保留唯一生命周期来源，使普通刷新也保持此目标关闭。
        self.state
            .lifecycle_overrides
            .lock()
            .map_err(|_| VulcanPluginRuntimeError::new("Plugin lifecycle lock is poisoned."))?
            .insert(target.plugin_id.clone(), VulcanPluginRuntimeState::Enabling);
        snapshot.revision = revision;
        // Readers retain the exact closed catalog used by all private handoff preparation.
        // 读取方保留全部私有交接准备使用的精确关闭目录。
        let snapshot = Arc::new(snapshot);
        *self
            .state
            .snapshot
            .write()
            .map_err(|_| VulcanPluginRuntimeError::new("Plugin catalog lock is poisoned."))? =
            Arc::clone(&snapshot);
        Ok(snapshot)
    }

    /// Publish matching enabled runtime and extension catalogs under the existing registration exclusion.
    /// 在既有注册互斥下发布匹配的启用运行时及扩展目录。
    /// transaction owns the original request; staged is the exact current closed catalog, including later namespace refreshes.
    /// transaction 拥有原请求；staged 是精确当前关闭目录，包含随后其他命名空间的刷新。
    /// publisher synchronously replaces the complete extension pointer; it must not reenter runtime admission.
    /// publisher 同步替换完整扩展指针；不得重新进入运行时入场。
    /// Returns sealed full publication evidence with its exact closed/final Arcs; original completion still gates business.
    /// 返回具有精确关闭／最终 Arc 的封闭完整发布凭据；原请求完成仍门控业务。
    pub fn publish_activation_target_handoff(
        &self,
        transaction: &crate::VulcanPluginActivationTransaction,
        staged: &Arc<VulcanPluginCatalogSnapshot>,
        publisher: impl FnOnce(Arc<VulcanPluginCatalogSnapshot>) -> Result<(), String>,
    ) -> Result<VulcanPluginActivationTargetPublication, VulcanPluginRuntimeError> {
        // No business registration can cross the synchronous complete-extension handoff.
        // 任何业务注册均不能越过同步完整扩展交接。
        let _registration =
            self.state.session_registration_gate.lock().map_err(|_| {
                VulcanPluginRuntimeError::new("Plugin publication gate is poisoned.")
            })?;
        self.enabled_activation_publication_store(transaction)?;
        // Arc identity rejects stale preparation even if revision fields were copied.
        // 即使复制修订字段，Arc 身份仍拒绝陈旧准备。
        let current = self.snapshot();
        if !Arc::ptr_eq(&current, staged) {
            return Err(VulcanPluginRuntimeError::new(
                "Target handoff catalog changed after private preparation.",
            ));
        }
        // The actual original target must remain closed until this exact publication.
        // 实际原目标必须保持关闭，直至此次精确发布。
        let target = transaction.record().target();
        let plugin = staged
            .plugins
            .get(&target.plugin_id)
            .ok_or_else(|| VulcanPluginRuntimeError::new("Staged target is missing."))?;
        if plugin.runtime_state != VulcanPluginRuntimeState::Enabling
            || plugin.availability != VulcanPluginAvailability::Enabled
            || !plugin.desired_enabled
            || !plugin.runtime_execution_available
            || !plugin
                .managed_package()
                .is_some_and(|package| package.matches_activation(target))
        {
            return Err(VulcanPluginRuntimeError::new(
                "Staged target does not match the original closed activation.",
            ));
        }
        self.validate_plugin_requirements(plugin)?;
        // Build the final complete surface without issuing another routing generation.
        // 构建最终完整暴露面，不签发另一运行代次。
        // Reserve the original persistent publication evidence before invoking any external publisher.
        // 在调用任何外部发布器前预留原持久发布证据。
        let private_hook = self.activation_target_hook_for_publication(transaction)?;
        let private_publication = private_hook
            .as_ref()
            .map(|runtime| runtime.target_publication_guard())
            .transpose()?;
        let mut next = (**staged).clone();
        next.plugins
            .get_mut(&target.plugin_id)
            .expect("validated staged target")
            .runtime_state = VulcanPluginRuntimeState::Enabled;
        rebuild_enabled_surface(&mut next);
        next.revision = self.reserve_activation_catalog_revision()?;
        // The publisher receives exactly the provisional catalog installed into the manager.
        // 发布器接收已临时安装到管理器的精确目录。
        let next = Arc::new(next);
        // Prepare the exact original publication witness before invoking the synchronous full publisher.
        // 调用同步完整发布器之前，准备精确原发布凭据。
        let publication =
            VulcanPluginActivationTargetPublication::new(self, transaction, staged, &next);
        // Host Hook validation reads the current manager; publish its exact provisional catalog while business stays fenced.
        // 宿主 Hook 校验读取当前管理器；业务仍被隔离时发布其精确临时目录。
        self.state
            .lifecycle_overrides
            .lock()
            .map_err(|_| VulcanPluginRuntimeError::new("Plugin lifecycle lock is poisoned."))?
            .remove(&target.plugin_id);
        *self
            .state
            .snapshot
            .write()
            .map_err(|_| VulcanPluginRuntimeError::new("Plugin catalog lock is poisoned."))? =
            Arc::clone(&next);
        if let Err(publication_error) = publisher(Arc::clone(&next)) {
            // A normal publisher failure restores the same closed pointer under the still-held registration gate.
            // 正常发布器失败在仍持有的注册门内恢复同一关闭指针。
            self.state
                .lifecycle_overrides
                .lock()
                .map_err(|_| {
                    VulcanPluginRuntimeError::new(format!(
                        "{publication_error}; closed target lifecycle restoration failed."
                    ))
                })?
                .insert(target.plugin_id.clone(), VulcanPluginRuntimeState::Enabling);
            *self.state.snapshot.write().map_err(|_| {
                VulcanPluginRuntimeError::new(format!(
                    "{publication_error}; closed target catalog restoration failed."
                ))
            })? = Arc::clone(staged);
            return Err(VulcanPluginRuntimeError::new(publication_error));
        }
        if let Some(publication) = private_publication {
            publication.published(&next);
        }
        // The selected-target disk fence still denies business until this original request completes.
        // 已选目标磁盘屏障仍拒绝业务，直至此原请求完成。
        Ok(publication)
    }

    /// Validate actual ownership, complete selected target and the frozen enabled update intent.
    /// 校验实际所有权、完整已选目标及固定启用更新意图。
    /// transaction supplies the sole original request; returns its existing validated store without allocating a new owner.
    /// transaction 提供唯一原请求；返回既有已校验存储，不分配新所有者。
    fn enabled_activation_publication_store(
        &self,
        transaction: &crate::VulcanPluginActivationTransaction,
    ) -> Result<crate::VulcanPluginGenerationStore, VulcanPluginRuntimeError> {
        // Opening absence is an error after a published original was declared.
        // 声明已发布原请求后，打开时缺失即为错误。
        let store = crate::VulcanPluginGenerationStore::open_existing(&self.state.user_plugin_root)
            .map_err(|error| VulcanPluginRuntimeError::new(error.to_string()))?
            .ok_or_else(|| {
                VulcanPluginRuntimeError::new("Original activation store is missing.")
            })?;
        // A retained initialized original permits exact Completed readback recovery without a new VM or init.
        // 保留的已初始化原实例允许精确 Completed 回读恢复，不创建新 VM 或初始化。
        // An explicitly empty Hook declaration has no private VM owner to transfer or replay.
        // 显式空 Hook 声明没有需要转交或重放的私有 VM 所有者。
        let no_hooks = self
            .snapshot()
            .plugins
            .get(&transaction.record().target().plugin_id)
            .is_some_and(|plugin| {
                plugin.hooks.is_empty()
                    && plugin.managed_package().is_some_and(|package| {
                        package.matches_activation(transaction.record().target())
                    })
            });
        if no_hooks
            || self
                .activation_target_hook_for_publication(transaction)?
                .is_some()
        {
            store.validate_published_target_handoff_owner(transaction)
        } else {
            store.validate_published_activation_owner(transaction)
        }
        .map_err(|error| VulcanPluginRuntimeError::new(error.to_string()))?;
        if !transaction
            .record()
            .update_policy()
            .is_some_and(|policy| policy.desired_enabled())
        {
            return Err(VulcanPluginRuntimeError::new(
                "Target handoff requires the original enabled update policy.",
            ));
        }
        Ok(store)
    }
}
