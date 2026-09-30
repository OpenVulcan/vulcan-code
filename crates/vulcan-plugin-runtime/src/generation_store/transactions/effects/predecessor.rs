//! Durable predecessor Lua drain identity binds the original host transition before admission closes.
//! 持久前驱 Lua 排空身份在关闭入场前绑定原宿主转换。

use super::*;

/// Exact predecessor package and runtime transition, independent from subsequent public catalog scans.
/// 精确前驱插件包及运行时转换，独立于后续公开目录扫描。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VulcanPluginPredecessorDrainIdentity {
    /// Original manager incarnation; a restarted manager cannot silently reuse its transition grant.
    /// 原管理器实例；重启管理器不能静默复用其转换授权。
    pub service_epoch: String,
    /// Original public runtime generation before closing business admission.
    /// 关闭业务入场前的原公开运行代次。
    pub predecessor_generation: u64,
    /// Exact newer draining generation issued by the same manager.
    /// 相同管理器签发的精确较新排空代次。
    pub draining_generation: u64,
    /// Frozen predecessor manifest version, including legacy flat installations.
    /// 固定前驱清单版本，包含旧平铺安装。
    pub version: String,
    /// Frozen complete predecessor package digest, including legacy flat installations.
    /// 固定完整前驱插件包摘要，包含旧平铺安装。
    pub content_fingerprint: String,
}

impl VulcanPluginActivationEffects {
    /// Check predecessor identity and causal ordering against the original transaction; return no execution grant.
    /// 按原事务检查前驱身份及因果顺序；不授予执行权。
    pub(super) fn validate_predecessor_drain(
        &self,
        transaction: &VulcanPluginActivationTransactionRecord,
    ) -> VulcanPluginStoreResult<()> {
        // Identity and phase are one declaration; neither may silently disappear independently.
        // 身份及阶段属于同一声明；二者均不得独立静默消失。
        let identity = match (&self.predecessor_drain_identity, self.predecessor_lua_drain) {
            (None, None) if self.predecessor_disable.is_none() => return Ok(()),
            (Some(identity), Some(_)) => identity,
            _ => {
                return Err(Error::invalid(
                    "Predecessor drain identity and observation must coexist.",
                ));
            }
        };
        // The original user policy is required before a host can initiate an enabled predecessor transition.
        // 宿主开始启用前驱转换前必须具有原用户策略。
        if !transaction
            .update_policy()
            .is_some_and(|policy| policy.desired_enabled())
            || identity.predecessor_generation == 0
            || identity.draining_generation <= identity.predecessor_generation
            || identity.version.is_empty()
        {
            return Err(Error::invalid("Predecessor drain transition is invalid."));
        }
        // UUID spelling follows the same canonical non-nil host identity contract.
        // UUID 拼写遵循相同规范非空宿主身份契约。
        let epoch = Uuid::parse_str(&identity.service_epoch)
            .map_err(|_| Error::invalid("Invalid predecessor manager identity."))?;
        if epoch.is_nil() || epoch.to_string() != identity.service_epoch {
            return Err(Error::invalid("Noncanonical predecessor manager identity."));
        }
        files::validate_content_fingerprint(&identity.content_fingerprint)?;
        if let Some(predecessor) = transaction.predecessor() {
            // Managed originals have complete durable package authority; tombstones cannot represent enabled predecessors.
            // 受管原请求具有完整持久插件包权威；停用记录不能表示启用前驱。
            let generation = predecessor
                .generation
                .as_ref()
                .ok_or_else(|| Error::invalid("Draining predecessor has no selected package."))?;
            if generation.version != identity.version
                || generation.content_fingerprint != identity.content_fingerprint
            {
                return Err(Error::invalid(
                    "Predecessor drain differs from its original package selection.",
                ));
            }
        }
        if self.predecessor_lua_drain != Some(VulcanPluginEffectObservation::Succeeded)
            && (self.predecessor_disable.is_some()
                || self.probe.is_some()
                || self.enable.is_some()
                || self.enable_rollback.is_some())
        {
            return Err(Error::invalid(
                "Candidate execution precedes predecessor Lua retirement.",
            ));
        }
        if self
            .predecessor_disable
            .is_some_and(|phase| phase != VulcanPluginEffectObservation::Succeeded)
            && (self.probe.is_some() || self.enable.is_some() || self.enable_rollback.is_some())
        {
            return Err(Error::invalid(
                "Candidate execution precedes confirmed predecessor disable.",
            ));
        }
        Ok(())
    }
}

impl VulcanPluginActivationCandidate<'_> {
    /// Persist one original predecessor drain before the host closes admission; return its exclusive observation.
    /// 宿主关闭入场前持久保存单次原前驱排空；返回其排他观测。
    /// Predecessor is the current frozen descriptor and identity is the manager's exact proposed transition.
    /// predecessor 为当前固定描述符，identity 为管理器精确拟议转换。
    pub(crate) fn begin_predecessor_drain_attempt(
        &self,
        predecessor: &crate::VulcanPluginDescriptor,
        identity: VulcanPluginPredecessorDrainIdentity,
    ) -> VulcanPluginStoreResult<EffectAttempt> {
        // Store and original request come exclusively from the inspected borrowed candidate.
        // 存储及原请求仅来自已检查借用候选。
        let store = VulcanPluginGenerationStore {
            root: self.transaction.store_root.clone(),
        };
        // Serialize receipt creation with exact original selection and transaction validation.
        // 将回执创建与精确原选择及事务校验排序。
        let namespace = store.namespace_lock(&self.transaction.record.target.plugin_id, false)?;
        store.check_transaction_owner(self.transaction)?;
        if self.transaction.record.phase != VulcanPluginActivationTransactionPhase::Prepared
            || store.read_activation(&self.transaction.record.target.plugin_id)?
                != self.transaction.record.predecessor
            || predecessor.id != self.transaction.record.target.plugin_id
            || predecessor.origin != crate::VulcanPluginPackageOrigin::UserInstalled
            || predecessor.runtime_state != crate::VulcanPluginRuntimeState::Enabled
            || predecessor.generation != identity.predecessor_generation
            || predecessor.version.as_deref() != Some(identity.version.as_str())
            || predecessor.content_fingerprint != identity.content_fingerprint
        {
            return Err(Error::new(
                Code::Conflict,
                "Predecessor drain does not own the unchanged enabled original.",
            ));
        }
        // Exact managed and explicitly legacy predecessors remain separate authorities.
        // 精确受管前驱及明确旧格式前驱保持不同权威。
        if !match (
            predecessor.managed_package(),
            self.transaction.record.predecessor(),
        ) {
            (Some(package), Some(selected)) => package.matches_activation(selected),
            (None, None) => true,
            _ => false,
        } {
            return Err(Error::new(
                Code::Conflict,
                "Predecessor package selection changed.",
            ));
        }
        // A frozen baseline must still match the actual original descriptor.
        // 固定基线仍须匹配实际原描述符。
        let policy =
            self.transaction.record.update_policy().ok_or_else(|| {
                Error::invalid("Predecessor drain requires original update policy.")
            })?;
        if !policy.desired_enabled()
            || !predecessor.desired_enabled
            || predecessor
                .manifest
                .as_ref()
                .map(|manifest| manifest.default_enabled)
                != Some(policy.predecessor_default_enabled)
        {
            return Err(Error::new(
                Code::Conflict,
                "Predecessor policy changed before drain.",
            ));
        }
        if store
            .read_effects_locked(&self.transaction.record)?
            .is_some()
            || store
                .read_installation_locked(&self.transaction.record)?
                .is_some()
        {
            return Err(Error::new(
                Code::Conflict,
                "Predecessor drain must begin once before candidate effects.",
            ));
        }
        // Started records intent only; actual resources remain protected by the same request lock.
        // Started 仅记录意图；实际资源继续由相同请求锁保护。
        let receipt = VulcanPluginActivationEffects {
            schema_version: EFFECTS_VERSION,
            plugin_id: predecessor.id.clone(),
            request_id: self.transaction.record.request_id,
            target_revision: self.transaction.record.target.revision,
            predecessor_drain_identity: Some(identity),
            predecessor_lua_drain: Some(VulcanPluginEffectObservation::Started),
            predecessor_disable: None,
            probe: None,
            enable: None,
            enable_rollback: None,
        };
        store.write_effects_locked(&self.transaction.record, &receipt)?;
        drop(namespace);
        Ok(EffectAttempt {
            store,
            transaction: self.transaction.record.clone(),
            operation: VulcanPluginActivationEffect::DrainPredecessorLua,
            receipt,
            _readiness: self.retain_readiness(),
        })
    }
}
