//! Confirm original abort before staging the exact old descriptor with all business admission closed.
//! 确认原中止后暂存精确旧描述符，并保持全部业务入场关闭。

use super::*;
use crate::runtime::update_drain::restoration::{
    VulcanPluginPredecessorRestoration, validate_restoration_receipts,
};
use crate::{VulcanPluginActivationTransaction, VulcanPluginActivationTransactionPhase as Phase};

impl VulcanPluginRuntimeManager {
    /// Abort transaction exclusively, then stage restoration's exact old descriptor without another enable callback.
    /// 独占中止 transaction，再暂存 restoration 的精确旧描述符，不再次调用启用回调。
    /// Policy booleans are only this plugin's current explicit memberships; unrelated provider settings are excluded.
    /// 策略布尔值仅表示此插件当前明确成员关系；不包含无关供应商设置。
    /// Return the closed Enabling Arc; every error retains both mutable owners and leaves business admission closed.
    /// 返回关闭的启用中 Arc；每个错误保留两个可变所有者，并保持业务入场关闭。
    pub fn abort_and_stage_activation_predecessor_restoration(
        &self,
        transaction: &mut VulcanPluginActivationTransaction,
        restoration: &mut VulcanPluginPredecessorRestoration,
        explicitly_enabled: bool,
        explicitly_disabled: bool,
    ) -> Result<Arc<VulcanPluginCatalogSnapshot>, VulcanPluginRuntimeError> {
        // Registration exclusion spans exact identity checks, durable abort and the single closed catalog replacement.
        // 注册互斥跨越精确身份校验、持久中止及单次关闭目录替换。
        let registration =
            self.state.session_registration_gate.lock().map_err(|_| {
                VulcanPluginRuntimeError::new("Plugin registration gate is poisoned.")
            })?;
        self.require_routing_publication_locked(
            &registration,
            Some(crate::runtime::routing::ActivationRoutingOriginal::Transaction(transaction)),
        )?;
        // All three immutable original authorities must match; phase alone never grants old restoration.
        // 三个不可变原权威必须全部匹配；仅阶段不能授予旧恢复。
        let original = restoration.original();
        // Borrow the mutable owner's last confirmed disk observation without cloning readiness ownership.
        // 借用可变所有者最后确认磁盘观测，不克隆就绪所有权。
        let record = transaction.record();
        if !record.matches_predecessor_restoration_original(original) {
            return Err(VulcanPluginRuntimeError::new(
                "Predecessor restoration belongs to another original transaction.",
            ));
        }
        // This primitive implements only prepublication abort; target selection can never be rolled back here.
        // 此原语仅实现发布前中止；此处绝不能回滚目标选择。
        if !matches!(
            record.phase(),
            Phase::Prepared | Phase::Committing | Phase::Aborted
        ) {
            return Err(VulcanPluginRuntimeError::new(
                "Published target cannot restore its predecessor through abort staging.",
            ));
        }
        // Only original plugin membership can authorize restoring its effective enabled state.
        // 仅原插件成员关系能授权恢复其有效启用状态。
        let policy = original.update_policy().ok_or_else(|| {
            VulcanPluginRuntimeError::new(
                "Predecessor restoration requires original plugin-specific policy.",
            )
        })?;
        if !policy.desired_enabled()
            || !policy.matches_overrides(explicitly_enabled, explicitly_disabled)
        {
            return Err(VulcanPluginRuntimeError::new(
                "Original predecessor restoration policy changed.",
            ));
        }
        // The actual configured store and exact selected predecessor are checked before any state change.
        // 在任何状态变化前检查实际配置存储及精确选定前驱。
        let store = crate::VulcanPluginGenerationStore::open_existing(&self.state.user_plugin_root)
            .map_err(|failure| VulcanPluginRuntimeError::new(failure.to_string()))?
            .ok_or_else(|| VulcanPluginRuntimeError::new("Original update store is missing."))?;
        if store
            .activation(&original.target().plugin_id)
            .map_err(|failure| VulcanPluginRuntimeError::new(failure.to_string()))?
            .as_ref()
            != original.predecessor()
        {
            return Err(VulcanPluginRuntimeError::new(
                "Original predecessor selection changed before restoration staging.",
            ));
        }
        // Rechecking actual native retention complements, but does not replace, the store's sole request-lock check.
        // 再次检查实际原生保留补充存储的唯一请求锁检查，但不替代它。
        self.require_restoration_execution_retired(restoration.request_id())?;
        validate_restoration_receipts(&store, &restoration.drain)?;
        // A prior successful closed staging can be returned only while the exact Arc remains current.
        // 仅当精确 Arc 仍为当前目录时，才能返回先前成功关闭暂存。
        if let Some(staged) = restoration.staged.as_ref() {
            if record.phase() == Phase::Aborted && Arc::ptr_eq(staged, &self.snapshot()) {
                return Ok(Arc::clone(staged));
            }
            return Err(VulcanPluginRuntimeError::new(
                "Original restoration staging catalog changed.",
            ));
        }
        // Even an Aborted disk checkpoint is insufficient to recreate a missing live restoration staging credential.
        // 即使磁盘已有 Aborted 检查点，也不足以重建缺失的活动恢复暂存凭据。
        if record.phase() == Phase::Aborted {
            return Err(VulcanPluginRuntimeError::new(
                "Original restoration was aborted without confirmed closed staging.",
            ));
        }
        // Retain the immutable old descriptor rather than rescanning a potentially different package.
        // 保留不可变旧描述符，不重新扫描可能不同的包。
        let predecessor = restoration.predecessor();
        crate::runtime::update_drain::validate_predecessor_callback_current(
            &self.state,
            predecessor,
            restoration.drain.identity(),
        )?;
        if self.has_unretired_plugin_resources_locked(&predecessor.id, &registration)? {
            return Err(VulcanPluginRuntimeError::new(
                "Old plugin resources are still retiring before restoration staging.",
            ));
        }
        self.validate_plugin_requirements(predecessor)?;
        // Prepare every allocation, checked generation and lock before abort can cross the durable boundary.
        // 在中止跨越持久边界之前准备全部分配、受检代次及锁。
        let mut generations =
            self.state.plugin_generations.lock().map_err(|_| {
                VulcanPluginRuntimeError::new("Plugin generation lock is poisoned.")
            })?;
        // Resolve only the existing generation; missing ownership must not allocate a substitute.
        // 仅解析既有代次；缺失所有权不得分配替代者。
        let generation = generations.get_mut(&predecessor.id).ok_or_else(|| {
            VulcanPluginRuntimeError::new("Original predecessor routing generation is missing.")
        })?;
        if *generation != restoration.drain.identity().draining_generation {
            return Err(VulcanPluginRuntimeError::new(
                "Original predecessor routing generation changed.",
            ));
        }
        // New routing identity excludes stale old descriptors without wrapping the sole generation counter.
        // 新路由身份排除陈旧旧描述符，同时避免唯一代次计数器回绕。
        let successor = generation.checked_add(1).ok_or_else(|| {
            VulcanPluginRuntimeError::new("Plugin routing generation is exhausted.")
        })?;
        // Hold the authoritative lifecycle override before crossing durable abort.
        // 跨越持久中止之前持有权威生命周期覆盖。
        let mut overrides = self
            .state
            .lifecycle_overrides
            .lock()
            .map_err(|_| VulcanPluginRuntimeError::new("Plugin lifecycle lock is poisoned."))?;
        // Reuse the existing Draining override so successful abort needs no new key allocation or inferred state.
        // 复用既有排空中覆盖，使成功中止无需分配新键或推断状态。
        let lifecycle = overrides.get_mut(&predecessor.id).ok_or_else(|| {
            VulcanPluginRuntimeError::new("Original predecessor lifecycle override is missing.")
        })?;
        if *lifecycle != VulcanPluginRuntimeState::Draining {
            return Err(VulcanPluginRuntimeError::new(
                "Original predecessor lifecycle override changed.",
            ));
        }
        // Reserve the catalog write lock so successful abort is followed by an infallible pointer replacement.
        // 预留目录写锁，使成功中止后能够无失败替换指针。
        let mut current = self
            .state
            .snapshot
            .write()
            .map_err(|_| VulcanPluginRuntimeError::new("Plugin catalog lock is poisoned."))?;
        // Preserve every other namespace from the exact current catalog.
        // 从精确当前目录保留其他全部命名空间。
        let mut catalog = (**current).clone();
        // Cloning the actual old descriptor shares its immutable package, configuration and credential pins.
        // 克隆实际旧描述符共享其不可变包、配置及凭据固定句柄。
        let mut old = predecessor.clone();
        old.runtime_state = VulcanPluginRuntimeState::Enabling;
        old.generation = successor;
        for tool in &mut old.tools {
            tool.plugin_generation = successor;
        }
        for hook in &mut old.hooks {
            hook.plugin_generation = successor;
        }
        for server in &mut old.mcp_servers {
            server.plugin_generation = successor;
        }
        catalog.plugins.insert(old.id.clone(), old);
        rebuild_enabled_surface(&mut catalog);
        catalog.revision = self
            .state
            .next_revision
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |revision| {
                revision.checked_add(1)
            })
            .map_err(|_| VulcanPluginRuntimeError::new("Plugin catalog revision is exhausted."))?;
        // Allocate the final exact closed Arc before the durable write can complete.
        // 在持久写入可能完成前分配最终精确关闭 Arc。
        let catalog = Arc::new(catalog);
        // Confirm the exact original abort, including a prior published Aborted whose local result was lost.
        // 确认精确原中止，包括先前已发布但局部结果丢失的 Aborted。
        store
            .abort_activation_predecessor_restoration_owned(transaction, original)
            .map_err(|failure| VulcanPluginRuntimeError::new(failure.to_string()))?;
        if transaction.record().phase() != Phase::Aborted {
            return Err(VulcanPluginRuntimeError::new(
                "Original activation abort is not confirmed.",
            ));
        }
        // Confirmation performs no callback or route reopening; the returned exact Arc still requires a future full publisher.
        // 确认不执行回调或重开路由；返回精确 Arc 仍需未来完整发布器。
        *generation = successor;
        *lifecycle = VulcanPluginRuntimeState::Enabling;
        *current = Arc::clone(&catalog);
        restoration.staged = Some(Arc::clone(&catalog));
        Ok(catalog)
    }
}
