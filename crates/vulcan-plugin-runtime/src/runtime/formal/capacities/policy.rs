//! Explicit host transactions preserve native ownership across persistent policy publication.
//! 明确宿主事务在持久策略发布期间保留原生归属。

use super::*;
use luaskills::runtime::embedded::EmbeddedErrorCode;

impl CapacityRegistry {
    /// Reject new preparation for plugin_id/purpose while its original transaction remains unfinished.
    /// 原事务未结束时，拒绝 plugin_id／purpose 的新准备。
    /// Existing native operations and fixed instances keep their original ownership.
    /// 既有原生操作及固定实例保持原归属。
    pub(super) fn require_policy_admission(
        &self,
        plugin_id: &str,
        purpose: VulcanPluginCapacityPurpose,
    ) -> Result<(), VulcanPluginRuntimeError> {
        if self.policy.update.as_ref().is_some_and(|record| {
            record.request.plugin_id == plugin_id
                && record.request.purpose == purpose
                && !record.phase.is_terminal()
        }) {
            return Err(VulcanPluginRuntimeError::new(
                "Capacity policy update requires explicit recovery before new admission.",
            ));
        }
        Ok(())
    }
}

impl ManagedFormalRuntime {
    /// Return effective settings and retained transaction under one lock for a consistent management projection.
    /// 在同一锁下返回生效设置及保留事务，以提供一致管理投影。
    pub(in crate::runtime::formal) fn policy_status(
        &self,
    ) -> Result<
        (
            VulcanPluginRuntimeSettings,
            Option<VulcanPluginCapacityPolicyRecord>,
        ),
        VulcanPluginRuntimeError,
    > {
        // The sole host policy store owns both pieces of this snapshot.
        // 唯一宿主策略存储拥有此快照的两个部分。
        let registry = self.capacities.lock().map_err(|_| capacity_lock_error())?;
        Ok((
            registry.policy.settings.clone(),
            registry.policy.update.clone(),
        ))
    }
    /// Inject one test filesystem failure at phase, optionally after publication; return no value.
    /// 在 phase 注入一次测试文件系统失败，可选择发布之后；无返回值。
    #[cfg(test)]
    pub(in crate::runtime::formal) fn fail_policy_write(
        &self,
        phase: VulcanPluginCapacityPolicyPhase,
        after: bool,
    ) {
        self.capacities
            .lock()
            .expect("policy test registry")
            .policy
            .fault = Some((phase, after));
    }
    /// Clone the sole effective settings projection; frozen parent/plugin budgets are unchanged.
    /// 克隆唯一生效设置投影；冻结父级／插件预算保持不变。
    pub(in crate::runtime) fn effective_settings(
        &self,
    ) -> Result<VulcanPluginRuntimeSettings, VulcanPluginRuntimeError> {
        Ok(self
            .capacities
            .lock()
            .map_err(|_| capacity_lock_error())?
            .policy
            .settings
            .clone())
    }

    /// Read the exact current policy token for plugin_id/purpose and fence unfinished updates.
    /// 读取 plugin_id／purpose 的精确当前策略令牌，并阻止未完成更新。
    /// Missing ownership is an error, never an implicit allocation.
    /// 归属缺失为错误，绝不隐式分配。
    pub(in crate::runtime::formal) fn capacity_revision(
        &self,
        plugin_id: &str,
        purpose: VulcanPluginCapacityPurpose,
    ) -> Result<String, VulcanPluginRuntimeError> {
        // Keep host settings and the queried native registration ordered under the same lock.
        // 在同一锁下保持宿主设置及所查询原生注册的顺序。
        let registry = self.capacities.lock().map_err(|_| capacity_lock_error())?;
        registry.require_policy_admission(plugin_id, purpose)?;
        // Exact semantic ownership must have been registered by the original preparation.
        // 精确语义归属必须已由原准备注册。
        let registration = registry
            .registrations
            .get(&(plugin_id.to_owned(), purpose))
            .ok_or_else(|| {
                VulcanPluginRuntimeError::new("Capacity policy owner is not registered.")
            })?;
        Ok(self
            .verify_capacity(plugin_id, purpose, registration, &registry.policy.settings)?
            .revision)
    }
}

impl VulcanPluginRuntimeManager {
    /// Submit request for one existing capacity; return its retained checkpoint even if later persistence needs recovery.
    /// 为一个既有容量提交 request；即使后续持久化需要恢复，也返回保留检查点。
    /// Repeating the exact caller identity observes the original receipt without reapplying or advancing it.
    /// 重复精确调用方身份仅观测原回执，不再次应用或推进它。
    pub fn begin_capacity_policy_update(
        &self,
        request: VulcanPluginCapacityPolicyUpdate,
    ) -> Result<VulcanPluginCapacityPolicyRecord, VulcanPluginRuntimeError> {
        // All management mutations share the original publication order with package and member admission.
        // 全部管理变更与包及成员入场共用原发布顺序。
        let _publication = self
            .state
            .session_registration_gate
            .lock()
            .map_err(|_| capacity_lock_error())?;
        // Explicit control requires an attached owner and never creates a fallback runtime.
        // 明确控制要求已附加所有者，绝不创建回退运行时。
        let owner = self
            .formal_runtime_owner()?
            .ok_or_else(|| VulcanPluginRuntimeError::new("Formal runtime is not attached."))?;
        // One registry protects the transaction and its effective configuration, never physical counters.
        // 单个注册表保护事务及生效配置，绝不保护另建的物理计数。
        let mut registry = owner.capacities.lock().map_err(|_| capacity_lock_error())?;
        if registry.policy.prewarm_update.is_some() {
            return Err(VulcanPluginRuntimeError::new(
                "A prewarm policy receipt is retained; acknowledge it before another policy update.",
            ));
        }
        if let Some(record) = &registry.policy.update {
            if record.request == request {
                return Ok(record.clone());
            }
            return Err(VulcanPluginRuntimeError::new(
                "A capacity policy receipt is retained; observe or acknowledge it before another update.",
            ));
        }
        // Complete candidate validation occurs before a durable intent or any native mutation.
        // 完整候选校验发生在持久意图或任何原生变更之前。
        let candidate = request
            .candidate(&registry.policy.settings)
            .map_err(VulcanPluginRuntimeError::new)?;
        self.validate_capacity_policy_candidate(&request, &candidate)?;
        if request.runtime_id != owner.host.runtime().id() {
            return Err(VulcanPluginRuntimeError::new(
                "Capacity policy predecessor belongs to another runtime.",
            ));
        }
        // The exact existing semantic owner must match both native identity and predecessor.
        // 精确既有语义所有者必须同时匹配原生身份及前驱。
        let registration = registry
            .registrations
            .get(&(request.plugin_id.clone(), request.purpose))
            .ok_or_else(|| {
                VulcanPluginRuntimeError::new("Capacity policy owner is not registered.")
            })?;
        // Keep revision and current configuration in one native observation.
        // 在一次原生观测中保持修订及当前配置。
        let policy = owner.verify_capacity(
            &request.plugin_id,
            request.purpose,
            registration,
            &registry.policy.settings,
        )?;
        if registration.identity != request.capacity_id
            || policy.revision != request.expected_revision
            || policy.capacity.closing
            || registry.retiring_plugins.contains(&request.plugin_id)
        {
            return Err(VulcanPluginRuntimeError::new(
                "Capacity policy predecessor is stale or its owner is closing.",
            ));
        }
        registry
            .policy
            .acquire(self.plugin_state_root())
            .map_err(VulcanPluginRuntimeError::new)?;
        registry.policy.update = Some(VulcanPluginCapacityPolicyRecord {
            request,
            phase: VulcanPluginCapacityPolicyPhase::Prepared,
            applied_revision: None,
            error: None,
            persistence_error: None,
        });
        self.advance_capacity_policy(&owner, &mut registry)
    }

    /// Observe update_id without acquiring a writer, replaying mutations or changing persistent bytes.
    /// 观测 update_id，不取得写入租约、不重放变更、不修改持久字节。
    pub fn capacity_policy_update(
        &self,
        update_id: &str,
    ) -> Result<Option<VulcanPluginCapacityPolicyRecord>, VulcanPluginRuntimeError> {
        // Absence remains read-only; administrative history is explicitly bounded to one record.
        // 缺失保持只读；管理历史明确限制为一个记录。
        let Some(owner) = self.formal_runtime_owner()? else {
            return Ok(None);
        };
        // Returning a clone cannot alter the original transaction or its predecessor.
        // 返回克隆不能改变原事务或前驱。
        let registry = owner.capacities.lock().map_err(|_| capacity_lock_error())?;
        Ok(registry
            .policy
            .update
            .as_ref()
            .filter(|record| record.request.update_id == update_id)
            .cloned())
    }

    /// Resume update_id from its original checkpoint; native application is never repeated after acknowledged success.
    /// 从原检查点恢复 update_id；确认成功后绝不重复原生应用。
    /// A lost runtime becomes Interrupted only after its exclusive writer lease can be acquired.
    /// 仅在能够取得其排他写入租约后，失去的运行时才成为 Interrupted。
    pub fn resume_capacity_policy_update(
        &self,
        update_id: &str,
    ) -> Result<VulcanPluginCapacityPolicyRecord, VulcanPluginRuntimeError> {
        // Serialize recovery with every other host publication operation.
        // 将恢复与全部其他宿主发布操作串行化。
        let _publication = self
            .state
            .session_registration_gate
            .lock()
            .map_err(|_| capacity_lock_error())?;
        // The current owner retains the exact persisted receipt even when its original runtime ended.
        // 即使原运行时结束，当前所有者仍保留精确持久回执。
        let owner = self
            .formal_runtime_owner()?
            .ok_or_else(|| VulcanPluginRuntimeError::new("Formal runtime is not attached."))?;
        // Recovery cannot mint another identity when its receipt is missing.
        // 回执缺失时，恢复不能签发另一个身份。
        let mut registry = owner.capacities.lock().map_err(|_| capacity_lock_error())?;
        require_update(&registry, update_id)?;
        self.advance_capacity_policy(&owner, &mut registry)
    }

    /// Cancel a not-yet-applied update_id; retain explicit rejection without reverting acknowledged native changes.
    /// 取消尚未应用的 update_id；保留明确拒绝，不撤销已确认原生变化。
    pub fn cancel_capacity_policy_update(
        &self,
        update_id: &str,
    ) -> Result<VulcanPluginCapacityPolicyRecord, VulcanPluginRuntimeError> {
        // The same gate excludes a concurrent native application while cancellation checks its checkpoint.
        // 同一门控在取消检查其检查点时排除并发原生应用。
        let _publication = self
            .state
            .session_registration_gate
            .lock()
            .map_err(|_| capacity_lock_error())?;
        // No runtime means no transaction available for cancellation.
        // 没有运行时即没有可取消事务。
        let owner = self
            .formal_runtime_owner()?
            .ok_or_else(|| VulcanPluginRuntimeError::new("Formal runtime is not attached."))?;
        // Retain original ownership through cancellation acknowledgement.
        // 在取消确认期间保留原归属。
        let mut registry = owner.capacities.lock().map_err(|_| capacity_lock_error())?;
        // Clone only the immutable receipt, never its native resources.
        // 仅克隆不可变回执，绝不克隆其原生资源。
        let mut record = require_update(&registry, update_id)?.clone();
        if record.phase != VulcanPluginCapacityPolicyPhase::Prepared {
            return Err(VulcanPluginRuntimeError::new(
                "Only a prepared capacity policy update can be cancelled.",
            ));
        }
        registry
            .policy
            .acquire(self.plugin_state_root())
            .map_err(VulcanPluginRuntimeError::new)?;
        if record.request.runtime_id == owner.host.runtime().id() {
            record.phase = VulcanPluginCapacityPolicyPhase::Rejected;
            record.error =
                Some("Capacity policy update cancelled before native application.".into());
        } else {
            record.phase = VulcanPluginCapacityPolicyPhase::Interrupted;
            record.error = Some("Original runtime ended before durable policy completion; its business effects are not replayed.".into());
        }
        registry.policy.update = Some(record);
        self.persist_capacity_policy_terminal(&mut registry)
    }

    /// Acknowledge terminal update_id and durably release its receipt; unfinished ownership is never forgotten.
    /// 确认终态 update_id 并持久释放其回执；绝不遗忘未结束归属。
    pub fn forget_capacity_policy_update(
        &self,
        update_id: &str,
    ) -> Result<(), VulcanPluginRuntimeError> {
        // Release ordering is shared with new submissions so no receipt can be replaced during acknowledgement.
        // 释放与新提交共用顺序，避免确认期间替换回执。
        let _publication = self
            .state
            .session_registration_gate
            .lock()
            .map_err(|_| capacity_lock_error())?;
        // Require the exact current retained owner.
        // 要求精确当前保留所有者。
        let owner = self
            .formal_runtime_owner()?
            .ok_or_else(|| VulcanPluginRuntimeError::new("Formal runtime is not attached."))?;
        // The receipt stays in memory if durable removal fails.
        // 持久移除失败时，回执继续保留在内存。
        let mut registry = owner.capacities.lock().map_err(|_| capacity_lock_error())?;
        if !require_update(&registry, update_id)?.phase.is_terminal() {
            return Err(VulcanPluginRuntimeError::new(
                "Capacity policy update is not terminal.",
            ));
        }
        // One document replacement removes history while retaining the exact effective configuration.
        // 单次文档替换移除历史，同时保留精确生效配置。
        let document = PolicyDocument::new(registry.policy.settings.clone(), None);
        registry
            .policy
            .persist(self.plugin_state_root(), &document)
            .map_err(VulcanPluginRuntimeError::new)?;
        registry.policy.update = None;
        Ok(())
    }

    /// Check current catalog requirements against candidate for request's exact plugin; return conflicts without clipping.
    /// 针对 request 的精确插件，以 candidate 检查当前目录要求；返回冲突，不裁剪。
    fn validate_capacity_policy_candidate(
        &self,
        request: &VulcanPluginCapacityPolicyUpdate,
        candidate: &VulcanPluginRuntimeSettings,
    ) -> Result<(), VulcanPluginRuntimeError> {
        // Uninstalled configured owners have no current manifest; future activation still validates the same policy.
        // 未安装配置所有者没有当前清单；未来激活仍校验同一策略。
        if let Some(manifest) = self
            .snapshot()
            .plugins
            .get(&request.plugin_id)
            .and_then(|plugin| plugin.manifest.as_ref())
        {
            candidate.validate_manifest_requirements(manifest, None)?;
        }
        Ok(())
    }

    /// Advance registry's original receipt under publication; return its exact live checkpoint on recoverable failures.
    /// 在发布门下推进 registry 的原回执；可恢复失败时返回精确活动检查点。
    /// owner supplies the sole native runtime; no pool or fixed session is rebound here.
    /// owner 提供唯一原生运行时；此处不重新绑定池或固定会话。
    fn advance_capacity_policy(
        &self,
        owner: &ManagedFormalRuntime,
        registry: &mut CapacityRegistry,
    ) -> Result<VulcanPluginCapacityPolicyRecord, VulcanPluginRuntimeError> {
        registry
            .policy
            .acquire(self.plugin_state_root())
            .map_err(VulcanPluginRuntimeError::new)?;
        // Clone the one retained checkpoint so each completed step can be published before the next failure.
        // 克隆单个保留检查点，使每个完成步骤可在下一次失败前发布。
        let mut record = registry
            .policy
            .update
            .clone()
            .expect("retained policy update");
        if record.phase == VulcanPluginCapacityPolicyPhase::Applied || record.phase.is_terminal() {
            return self.persist_capacity_policy_terminal(registry);
        }
        if record.request.runtime_id != owner.host.runtime().id() {
            record.phase = VulcanPluginCapacityPolicyPhase::Interrupted;
            record.error = Some("Original runtime ended before durable policy completion; no mutation was replayed into this runtime.".into());
            registry.policy.update = Some(record);
            return self.persist_capacity_policy_terminal(registry);
        }
        record.persistence_error = None;
        // Persist exact intent before calling the core, including when an earlier publication was ambiguous.
        // 调用核心前持久化精确意图，包括此前发布不确定的情况。
        let prepared = PolicyDocument::new(registry.policy.settings.clone(), Some(record.clone()));
        if let Err(error) = registry.policy.persist(self.plugin_state_root(), &prepared) {
            record.persistence_error = Some(format!(
                "Capacity policy preparation persistence failed: {error}"
            ));
            registry.policy.update = Some(record.clone());
            return Ok(record);
        }
        // Recheck the current manifest under the same publication gate used by package transitions.
        // 在包转换使用的同一发布门下复核当前清单。
        let candidate = match record
            .request
            .candidate(&registry.policy.settings)
            .map_err(VulcanPluginRuntimeError::new)
            .and_then(|candidate| {
                self.validate_capacity_policy_candidate(&record.request, &candidate)?;
                Ok(candidate)
            }) {
            Ok(candidate) => candidate,
            Err(error) => {
                record.phase = VulcanPluginCapacityPolicyPhase::Rejected;
                record.error = Some(error.to_string());
                registry.policy.update = Some(record);
                return self.persist_capacity_policy_terminal(registry);
            }
        };
        // A retained intent prevents explicit forgetting, but native shutdown can still be requested.
        // 保留意图阻止显式遗忘，但仍可请求原生关闭。
        let registration = registry
            .registrations
            .get(&(record.request.plugin_id.clone(), record.request.purpose))
            .ok_or_else(|| {
                VulcanPluginRuntimeError::new(
                    "Capacity policy registration disappeared during its transaction.",
                )
            })?;
        if registration.preparing_members != 0 {
            record.error =
                Some("Capacity policy waits for original member preparations to return.".into());
            registry.policy.update = Some(record.clone());
            return Ok(record);
        }
        // Never refresh the predecessor after another writer, even if it restored the same configuration.
        // 即使另一写入者恢复了相同配置，也绝不刷新前驱。
        let before = owner.verify_capacity(
            &record.request.plugin_id,
            record.request.purpose,
            registration,
            &registry.policy.settings,
        )?;
        if registration.identity != record.request.capacity_id
            || before.revision != record.request.expected_revision
        {
            record.phase = VulcanPluginCapacityPolicyPhase::Rejected;
            record.error = Some(
                "Capacity policy predecessor changed; submit a new explicit transaction.".into(),
            );
            registry.policy.update = Some(record);
            return self.persist_capacity_policy_terminal(registry);
        }
        // The same candidate resolver owns both persisted policy and actual native limits.
        // 同一候选解析器同时拥有持久策略及实际原生上限。
        let target = candidate
            .capacity_budget(&record.request.plugin_id, record.request.purpose)
            .0;
        match owner.host.runtime().revise_capacity(
            &record.request.capacity_id,
            &record.request.expected_revision,
            target,
        ) {
            Ok(revision) => {
                // Publish native success in memory before any fallible disk confirmation; never invoke the core again.
                // 在任何可能失败的磁盘确认前，于内存发布原生成功；绝不再次调用核心。
                registry.policy.settings = candidate;
                record.phase = VulcanPluginCapacityPolicyPhase::Applied;
                record.applied_revision = Some(revision);
                record.error = None;
            }
            Err(error) => {
                if error.code != EmbeddedErrorCode::Busy {
                    record.phase = VulcanPluginCapacityPolicyPhase::Rejected;
                }
                record.error = Some(error.to_string());
            }
        }
        registry.policy.update = Some(record.clone());
        if record.phase == VulcanPluginCapacityPolicyPhase::Prepared {
            return Ok(record);
        }
        self.persist_capacity_policy_terminal(registry)
    }

    /// Confirm the retained terminal/native-applied checkpoint; preserve Applied on failed durable publication.
    /// 确认保留终态／原生已应用检查点；持久发布失败时保留 Applied。
    /// registry owns exact current settings and no native mutation is performed by this step.
    /// registry 拥有精确当前设置，此步骤不执行任何原生变更。
    fn persist_capacity_policy_terminal(
        &self,
        registry: &mut CapacityRegistry,
    ) -> Result<VulcanPluginCapacityPolicyRecord, VulcanPluginRuntimeError> {
        // Preserve the original checkpoint until filesystem acknowledgement actually succeeds.
        // 在文件系统确认实际成功前保留原检查点。
        let mut record = registry
            .policy
            .update
            .clone()
            .expect("retained policy update");
        // The persistent document atomically couples committed settings and their native acknowledgement.
        // 持久文档将已提交设置及其原生确认原子关联。
        let mut committed = record.clone();
        committed.persistence_error = None;
        if committed.phase == VulcanPluginCapacityPolicyPhase::Applied {
            committed.phase = VulcanPluginCapacityPolicyPhase::Committed;
            committed.error = None;
        }
        // A rejected or interrupted update keeps its actual diagnostic in durable history.
        // 被拒绝或中断的更新在持久历史中保留其实际诊断。
        let document =
            PolicyDocument::new(registry.policy.settings.clone(), Some(committed.clone()));
        match registry.policy.persist(self.plugin_state_root(), &document) {
            Ok(()) => {
                registry.policy.update = Some(committed.clone());
                Ok(committed)
            }
            Err(error) => {
                record.persistence_error = Some(format!(
                    "Capacity policy durable confirmation failed: {error}"
                ));
                registry.policy.update = Some(record.clone());
                Ok(record)
            }
        }
    }
}

/// Resolve exact update_id in registry; return original receipt or a precise missing-identity error.
/// 在 registry 解析精确 update_id；返回原回执或精确身份缺失错误。
fn require_update<'a>(
    registry: &'a CapacityRegistry,
    update_id: &str,
) -> Result<&'a VulcanPluginCapacityPolicyRecord, VulcanPluginRuntimeError> {
    registry
        .policy
        .update
        .as_ref()
        .filter(|record| record.request.update_id == update_id)
        .ok_or_else(|| {
            VulcanPluginRuntimeError::new("Capacity policy update identity was not found.")
        })
}
