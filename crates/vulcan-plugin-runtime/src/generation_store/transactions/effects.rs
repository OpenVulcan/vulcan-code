//! Durable candidate effects belong to one immutable activation request.
//! 持久候选副作用归属于单个不可变激活请求。

use super::*;
use crate::generation_store::{
    VulcanPluginStoreError as Error, VulcanPluginStoreErrorCode as Code,
};

mod predecessor;
pub use predecessor::VulcanPluginPredecessorDrainIdentity;

/// Exact sidecar format; original activation records and their index remain unchanged.
/// 精确附属文件格式；原激活记录及其索引保持不变。
const EFFECTS_VERSION: u32 = 1;

/// Durable observation cannot prove that external effects have been undone.
/// 持久观测不能证明外部副作用已被撤销。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VulcanPluginEffectObservation {
    /// Intent precedes execution; actual admission and completion may remain unknown.
    /// 意图先于执行；实际入场及完成状态可能仍未知。
    Started,
    /// Invocation returned success after normal resource retirement.
    /// 调用在正常资源退役后返回成功。
    Succeeded,
    /// Error or timeout was observed; live work and external effects may remain.
    /// 已观测到错误或超时；存活工作及外部副作用可能仍存在。
    ObservationFailed,
}

/// Original-request operation outside installation, which retains its existing authoritative receipt.
/// 安装之外的原请求操作；安装保留其既有权威回执。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VulcanPluginActivationEffect {
    /// Close predecessor Lua admission and retire its exact original execution resources.
    /// 关闭前驱 Lua 入场，并退役其精确原执行资源。
    DrainPredecessorLua,
    /// Execute the frozen old package's disable callback after its previous Lua resources retire.
    /// 先前 Lua 资源退役后执行固定旧包的停用回调。
    DisablePredecessor,
    /// Execute candidate module initialization.
    /// 执行候选模块初始化。
    Probe,
    /// Execute the declared enable callback.
    /// 执行声明的启用回调。
    Enable,
    /// Execute the declared disable callback to compensate candidate enable.
    /// 执行声明的停用回调以补偿候选启用。
    EnableRollback,
    /// Restore the old package's enable callback; any attempt excludes publishing this original target.
    /// 恢复旧包启用回调；任何尝试均排除此原目标发布。
    RestorePredecessor,
}

/// Strict non-secret observations bind all candidate effects to their original selection.
/// 严格无秘密观测将全部候选副作用绑定至原选择。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VulcanPluginActivationEffects {
    /// Explicit format discriminator.
    /// 显式格式标识。
    schema_version: u32,
    /// Original plugin namespace.
    /// 原插件命名空间。
    plugin_id: String,
    /// Canonical original request identity.
    /// 规范原请求身份。
    #[serde(deserialize_with = "records::deserialize_request_id")]
    request_id: Uuid,
    /// Exact successor revision from the original transaction.
    /// 来自原事务的精确后继修订。
    target_revision: u64,
    /// Original host transition identity exists exactly when predecessor Lua drain was attempted.
    /// 仅尝试前驱 Lua 排空时存在原宿主转换身份。
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "preparation::deserialize_present"
    )]
    predecessor_drain_identity: Option<VulcanPluginPredecessorDrainIdentity>,
    /// Drain evidence precedes candidate execution and is distinct from MCP retirement.
    /// 排空证据先于候选执行，并与 MCP 退役区分。
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "preparation::deserialize_present"
    )]
    predecessor_lua_drain: Option<VulcanPluginEffectObservation>,
    /// Old disable belongs to the original request and cannot be inferred from resource retirement.
    /// 旧停用属于原请求，不能从资源退役推断。
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "preparation::deserialize_present"
    )]
    predecessor_disable: Option<VulcanPluginEffectObservation>,
    /// Absent means never attempted; explicit null is invalid.
    /// 缺失表示从未尝试；显式空值无效。
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "preparation::deserialize_present"
    )]
    probe: Option<VulcanPluginEffectObservation>,
    /// One enable observation, without callback text or credentials.
    /// 单次启用观测，不含回调文本或凭据。
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "preparation::deserialize_present"
    )]
    enable: Option<VulcanPluginEffectObservation>,
    /// One compensation observation; any attempt prohibits publishing this candidate.
    /// 单次补偿观测；任何尝试均禁止发布此候选。
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "preparation::deserialize_present"
    )]
    enable_rollback: Option<VulcanPluginEffectObservation>,
    /// Old restore is independent from target compensation and never resets earlier failed evidence.
    /// 旧恢复独立于目标补偿，绝不重置先前失败证据。
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "preparation::deserialize_present"
    )]
    predecessor_restore: Option<VulcanPluginEffectObservation>,
}

impl VulcanPluginActivationEffects {
    /// Borrow the exact original transition identity; its presence alone does not certify actual retirement.
    /// 借用精确原转换身份；仅其存在不证明实际退役。
    pub fn predecessor_drain_identity(&self) -> Option<&VulcanPluginPredecessorDrainIdentity> {
        self.predecessor_drain_identity.as_ref()
    }

    /// Return attempted operations in execution order, without exposing private revision encoding.
    /// 按执行顺序返回已尝试操作，不暴露私有修订编码。
    pub fn observations(
        &self,
    ) -> Vec<(VulcanPluginActivationEffect, VulcanPluginEffectObservation)> {
        use VulcanPluginActivationEffect as Effect;
        [
            (Effect::DrainPredecessorLua, self.predecessor_lua_drain),
            (Effect::DisablePredecessor, self.predecessor_disable),
            (Effect::Probe, self.probe),
            (Effect::Enable, self.enable),
            (Effect::EnableRollback, self.enable_rollback),
            (Effect::RestorePredecessor, self.predecessor_restore),
        ]
        .into_iter()
        .filter_map(|(operation, phase)| phase.map(|phase| (operation, phase)))
        .collect()
    }

    /// Select the single field for operation; the caller controls and validates its transition.
    /// 选择 operation 的唯一字段；调用方控制并校验其迁移。
    fn observation_mut(
        &mut self,
        operation: VulcanPluginActivationEffect,
    ) -> &mut Option<VulcanPluginEffectObservation> {
        match operation {
            VulcanPluginActivationEffect::DrainPredecessorLua => &mut self.predecessor_lua_drain,
            VulcanPluginActivationEffect::DisablePredecessor => &mut self.predecessor_disable,
            VulcanPluginActivationEffect::Probe => &mut self.probe,
            VulcanPluginActivationEffect::Enable => &mut self.enable,
            VulcanPluginActivationEffect::EnableRollback => &mut self.enable_rollback,
            VulcanPluginActivationEffect::RestorePredecessor => &mut self.predecessor_restore,
        }
    }
}

/// The original observer retains the request lock; dropping it leaves Started durable.
/// 原观察者保留请求锁；销毁它保留持久 Started 状态。
pub(crate) struct EffectAttempt {
    /// Original validated storage root.
    /// 原已校验存储根。
    store: VulcanPluginGenerationStore,
    /// Frozen complete request authority.
    /// 固定完整请求权威。
    transaction: VulcanPluginActivationTransactionRecord,
    /// Exact observation being completed.
    /// 正在完成的精确观测。
    operation: VulcanPluginActivationEffect,
    /// Expected unchanged sidecar before final checkpoint.
    /// 最终检查点前预期未变的附属文件。
    receipt: VulcanPluginActivationEffects,
    /// Ownership survives foreground cancellation until this observer retires.
    /// 所有权跨越前台取消，直至此观察者退役。
    _readiness: Arc<File>,
}

impl EffectAttempt {
    /// Record succeeded for this owned attempt; identical durable completion may be acknowledged again after an I/O error.
    /// 为此自有尝试记录 succeeded；I/O 错误后可再次确认相同的持久完成。
    /// Return the write result without releasing ownership or repeating any plugin operation.
    /// 返回写入结果，不释放所有权或重复任何插件操作。
    pub(crate) fn record_observation(&self, succeeded: bool) -> VulcanPluginStoreResult<()> {
        // Freeze the requested terminal evidence before comparing either possible atomic-write outcome.
        // 比较两种可能的原子写入结果之前固定所请求的终态证据。
        let mut completed = self.receipt.clone();
        *completed.observation_mut(self.operation) = Some(if succeeded {
            VulcanPluginEffectObservation::Succeeded
        } else {
            VulcanPluginEffectObservation::ObservationFailed
        });
        // The request lock and namespace together exclude another observer or recovery writer.
        // 请求锁与命名空间共同排除另一观察者或恢复写入方。
        let _namespace = self.store.namespace_lock(&self.receipt.plugin_id, false)?;
        if self
            .store
            .read_transaction_locked(&self.receipt.plugin_id, self.receipt.request_id)?
            .as_ref()
            != Some(&self.transaction)
        {
            return Err(Error::new(
                Code::Conflict,
                "Original candidate effect identity changed.",
            ));
        }
        // Atomic replacement may have completed before a flush error reached its caller.
        // 原子替换可能在刷新错误到达调用方之前已经完成。
        let current = self.store.read_effects_locked(&self.transaction)?;
        if current.as_ref() == Some(&completed) {
            return self
                .store
                .write_effects_locked(&self.transaction, &completed);
        }
        if current.as_ref() != Some(&self.receipt) {
            return Err(Error::new(
                Code::Conflict,
                "Original candidate effect evidence changed.",
            ));
        }
        self.store
            .write_effects_locked(&self.transaction, &completed)
    }

    /// Persist succeeded for this exact attempt; return an error if identity or evidence changed.
    /// 为此精确尝试持久保存 succeeded；身份或证据改变时返回错误。
    /// No diagnostics or callback results enter the durable non-secret record.
    /// 诊断或回调结果不进入持久无秘密记录。
    pub(crate) fn finish(self, succeeded: bool) -> VulcanPluginStoreResult<()> {
        self.record_observation(succeeded)
    }
}

impl VulcanPluginActivationCandidate<'_> {
    /// Persist operation before any preparation or Lua execution; return its sole durable observer.
    /// 在任何准备或 Lua 执行之前持久保存 operation；返回其唯一持久观察者。
    /// Existing attempts cannot be replayed, even after successful execution or a process restart.
    /// 即使执行成功或进程重启，既有尝试也不能重放。
    pub(crate) fn begin_effect_attempt(
        &self,
        operation: VulcanPluginActivationEffect,
    ) -> VulcanPluginStoreResult<EffectAttempt> {
        if operation == VulcanPluginActivationEffect::DrainPredecessorLua {
            return Err(Error::invalid(
                "Predecessor drain requires exact host transition authority.",
            ));
        }
        // All paths derive from the borrowed original, never from user-controlled callback input.
        // 全部路径从借用原请求派生，绝不来自用户控制的回调输入。
        let store = VulcanPluginGenerationStore {
            root: self.transaction.store_root.clone(),
        };
        // One namespace fence orders different operation kinds as well as duplicate callers.
        // 单个命名空间屏障同时排序不同操作种类及重复调用者。
        let _namespace = store.namespace_lock(&self.transaction.record.target.plugin_id, false)?;
        store.check_transaction_owner(self.transaction)?;
        if self.transaction.record.phase != VulcanPluginActivationTransactionPhase::Prepared
            || store.read_activation(&self.transaction.record.target.plugin_id)?
                != self.transaction.record.predecessor
        {
            return Err(Error::new(
                Code::Conflict,
                "Candidate effects require the unchanged prepared request.",
            ));
        }
        // Actual absence is the only reason to allocate a first sidecar.
        // 实际缺失是分配首个附属文件的唯一原因。
        let mut receipt = store
            .read_effects_locked(&self.transaction.record)?
            .unwrap_or_else(|| VulcanPluginActivationEffects {
                schema_version: EFFECTS_VERSION,
                plugin_id: self.transaction.record.target.plugin_id.clone(),
                request_id: self.transaction.record.request_id,
                target_revision: self.transaction.record.target.revision,
                predecessor_drain_identity: None,
                predecessor_lua_drain: None,
                predecessor_disable: None,
                probe: None,
                enable: None,
                enable_rollback: None,
                predecessor_restore: None,
            });
        if receipt.observation_mut(operation).is_some() {
            return Err(Error::new(
                Code::Conflict,
                "Candidate effect was already attempted; inspect the original request instead of replaying it.",
            ));
        }
        if receipt
            .predecessor_lua_drain
            .is_some_and(|phase| phase != VulcanPluginEffectObservation::Succeeded)
        {
            return Err(Error::new(
                Code::Conflict,
                "Predecessor Lua resources have not confirmed retirement.",
            ));
        }
        // Any old restore attempt permanently excludes this original target from further effects.
        // 任何旧恢复尝试均永久排除此原目标的后续副作用。
        if receipt.predecessor_restore.is_some() {
            return Err(Error::new(
                Code::Conflict,
                "Original predecessor restore was attempted; this target is no longer admissible.",
            ));
        }
        // Installation remains authoritative in its existing receipt, not duplicated here.
        // 安装在既有回执中保持权威，不在此复制。
        if operation != VulcanPluginActivationEffect::RestorePredecessor
            && receipt
                .predecessor_disable
                .is_some_and(|phase| phase != VulcanPluginEffectObservation::Succeeded)
        {
            return Err(Error::new(
                Code::Conflict,
                "Original predecessor disable has not confirmed success.",
            ));
        }
        let installation = store.read_installation_locked(&self.transaction.record)?;
        use VulcanPluginActivationEffect as Effect;
        use VulcanPluginEffectObservation as Observation;
        match operation {
            Effect::RestorePredecessor => {
                receipt.require_observed_predecessor_restore()?;
                if installation
                    .as_ref()
                    .is_some_and(|receipt| receipt.phase() == Observation::Started)
                {
                    return Err(Error::new(
                        Code::Conflict,
                        "Predecessor restore cannot follow an unobserved installation attempt.",
                    ));
                }
            }
            Effect::DisablePredecessor => {
                if receipt.predecessor_lua_drain != Some(Observation::Succeeded)
                    || installation.is_some()
                    || receipt.probe.is_some()
                    || receipt.enable.is_some()
                    || receipt.enable_rollback.is_some()
                {
                    return Err(Error::new(
                        Code::Conflict,
                        "Predecessor disable requires confirmed original Lua retirement before candidate effects.",
                    ));
                }
            }
            Effect::DrainPredecessorLua => {
                return Err(Error::invalid(
                    "Predecessor drain is not candidate execution.",
                ));
            }
            Effect::Probe => {
                if installation.is_some()
                    || receipt.enable.is_some()
                    || receipt.enable_rollback.is_some()
                {
                    return Err(Error::new(
                        Code::Conflict,
                        "Candidate probe must precede lifecycle effects.",
                    ));
                }
            }
            Effect::Enable => {
                store.require_effects_before_install(&self.transaction.record)?;
                store.require_installation_succeeded(&self.transaction.record)?;
                // A declared installation callback is a real prerequisite, even when no receipt exists yet.
                // 声明的安装回调属于实际前置条件，即使尚无回执。
                let requires_installation = self
                    .package()
                    .manifest
                    .as_ref()
                    .and_then(|manifest| manifest.lua_runtime.as_ref())
                    .is_some_and(|runtime| runtime.lifecycle.install.is_some());
                if requires_installation && installation.is_none() {
                    return Err(Error::new(
                        Code::Conflict,
                        "Candidate enable requires its declared installation to succeed.",
                    ));
                }
            }
            Effect::EnableRollback => {
                if !matches!(
                    receipt.enable,
                    Some(Observation::Succeeded | Observation::ObservationFailed)
                ) {
                    return Err(Error::new(
                        Code::Conflict,
                        "Candidate compensation requires an observed original enable attempt.",
                    ));
                }
            }
        }
        *receipt.observation_mut(operation) = Some(Observation::Started);
        store.write_effects_locked(&self.transaction.record, &receipt)?;
        drop(_namespace);
        Ok(EffectAttempt {
            store,
            transaction: self.transaction.record.clone(),
            operation,
            receipt,
            _readiness: self.retain_readiness(),
        })
    }
}

impl VulcanPluginGenerationStore {
    /// Read exact request observations without admitting work; unconfigured originals must have no sidecar.
    /// 读取精确请求观测，不接纳工作；未配置原请求不得拥有附属文件。
    pub fn activation_effects(
        &self,
        plugin_id: &str,
        request_id: Uuid,
    ) -> VulcanPluginStoreResult<Option<VulcanPluginActivationEffects>> {
        // Query and original identity share one consistent namespace observation.
        // 查询与原身份共享一次一致命名空间观测。
        let _namespace = self.namespace_lock(plugin_id, true)?;
        // Missing originals cannot acquire historical effects by filename alone.
        // 缺失原请求不能仅凭文件名取得历史副作用。
        let request = self
            .read_request_locked(plugin_id, request_id)?
            .ok_or_else(|| Error::invalid("Original candidate effect request is missing."))?;
        match request {
            VulcanPluginActivationRequestRecord::Activation(transaction) => {
                self.read_effects_locked(&transaction)
            }
            VulcanPluginActivationRequestRecord::Preparation(_) => {
                // Validate the exact sidecar path before deciding actual absence.
                // 判断实际缺失前校验精确附属文件路径。
                let directory = self
                    .transaction_directory(plugin_id, false)?
                    .ok_or_else(|| Error::invalid("Original preparation directory disappeared."))?;
                if files::exists(&directory.join(format!("{request_id}.effects")), false)? {
                    return Err(Error::invalid(
                        "Early preparation cannot own candidate effect receipts.",
                    ));
                }
                Ok(None)
            }
        }
    }

    /// Reject incomplete probe or already enabled/compensated candidates before install or enable.
    /// 安装或启用前拒绝未完成探测或已启用／已补偿候选。
    /// Caller holds the namespace fence; absent probe preserves explicit direct-install support.
    /// 调用方持有命名空间屏障；缺失探测保留显式直接安装支持。
    pub(super) fn require_effects_before_install(
        &self,
        transaction: &VulcanPluginActivationTransactionRecord,
    ) -> VulcanPluginStoreResult<()> {
        if let Some(receipt) = self.read_effects_locked(transaction)?
            && (receipt
                .predecessor_lua_drain
                .is_some_and(|phase| phase != VulcanPluginEffectObservation::Succeeded)
                || receipt
                    .predecessor_disable
                    .is_some_and(|phase| phase != VulcanPluginEffectObservation::Succeeded)
                || receipt
                    .probe
                    .is_some_and(|phase| phase != VulcanPluginEffectObservation::Succeeded)
                || receipt.predecessor_restore.is_some()
                || receipt.enable.is_some()
                || receipt.enable_rollback.is_some())
        {
            return Err(Error::new(
                Code::Conflict,
                "Candidate lifecycle prerequisites have no confirmed success or were already advanced.",
            ));
        }
        Ok(())
    }

    /// Permit publication only when all attempted readiness effects succeeded and none was compensated.
    /// 仅全部已尝试就绪副作用成功且均未被补偿时允许发布。
    /// Missing receipts preserve historical and content-only transactions, without claiming readiness.
    /// 缺失回执保留历史及纯内容事务，不宣称已就绪。
    pub(super) fn require_effects_succeeded(
        &self,
        transaction: &VulcanPluginActivationTransactionRecord,
    ) -> VulcanPluginStoreResult<()> {
        if let Some(receipt) = self.read_effects_locked(transaction)?
            && (receipt.predecessor_restore.is_some()
                || receipt.enable_rollback.is_some()
                || receipt
                    .observations()
                    .iter()
                    .any(|(_, phase)| *phase != VulcanPluginEffectObservation::Succeeded))
        {
            return Err(Error::new(
                Code::Conflict,
                "Candidate effects have no confirmed success or compensation was attempted; inspect the original request.",
            ));
        }
        Ok(())
    }

    /// Resolve transaction's sidecar under its existing validated directory without creating storage.
    /// 在既有已校验目录下解析 transaction 的附属文件，不创建存储。
    fn effects_path(
        &self,
        transaction: &VulcanPluginActivationTransactionRecord,
    ) -> VulcanPluginStoreResult<PathBuf> {
        // A suffix adds observations without adding another activation pointer.
        // 后缀增加观测，不增加另一个激活指针。
        let directory = self
            .transaction_directory(&transaction.target.plugin_id, false)?
            .ok_or_else(|| Error::invalid("Original candidate effect directory is missing."))?;
        Ok(directory.join(format!("{}.effects", transaction.request_id)))
    }

    /// Validate identity and causal shape of receipt against transaction; never normalize invalid evidence.
    /// 按 transaction 校验 receipt 的身份及因果结构；绝不规范化无效证据。
    fn validate_effects(
        transaction: &VulcanPluginActivationTransactionRecord,
        receipt: &VulcanPluginActivationEffects,
    ) -> VulcanPluginStoreResult<()> {
        use VulcanPluginEffectObservation as Observation;
        if receipt.schema_version != EFFECTS_VERSION
            || receipt.plugin_id != transaction.target.plugin_id
            || receipt.request_id != transaction.request_id
            || receipt.target_revision != transaction.target.revision
            || receipt.observations().is_empty()
            || (receipt.enable.is_some()
                && receipt
                    .probe
                    .is_some_and(|phase| phase != Observation::Succeeded))
            || (receipt.enable_rollback.is_some()
                && !matches!(
                    receipt.enable,
                    Some(Observation::Succeeded | Observation::ObservationFailed)
                ))
        {
            return Err(Error::invalid(
                "Candidate effect identity, format or ordering is invalid.",
            ));
        }
        receipt.validate_predecessor_drain(transaction)?;
        Ok(())
    }

    /// Read bounded strict evidence for transaction while the caller holds its namespace fence.
    /// 调用方持有命名空间屏障时，为 transaction 读取有界严格证据。
    pub(super) fn read_effects_locked(
        &self,
        transaction: &VulcanPluginActivationTransactionRecord,
    ) -> VulcanPluginStoreResult<Option<VulcanPluginActivationEffects>> {
        // Reuse the sole metadata byte budget and reject symlink-like paths through the existing reader.
        // 复用唯一元数据字节预算，并通过既有读取器拒绝类符号链接路径。
        let Some(bytes) = files::read_bytes(&self.effects_path(transaction)?, MAX_RECORD_BYTES)?
        else {
            return Ok(None);
        };
        // Only typed observations can enter recovery; raw corrupt bytes are never logged.
        // 仅类型化观测可进入恢复；绝不记录原始损坏字节。
        let receipt = serde_json::from_slice(&bytes)
            .map_err(|_| Error::invalid("Invalid candidate effect receipt."))?;
        Self::validate_effects(transaction, &receipt)?;
        // Cross-sidecar ordering is checked on every read, including terminal history and startup inspection.
        // 每次读取均检查跨附属文件顺序，包括终态历史及启动检查。
        if receipt.predecessor_restore.is_some()
            && self
                .read_installation_locked(transaction)?
                .as_ref()
                .is_some_and(|installation| {
                    installation.phase() == VulcanPluginEffectObservation::Started
                })
        {
            return Err(Error::invalid(
                "Predecessor restore follows an unobserved installation attempt.",
            ));
        }
        Ok(Some(receipt))
    }

    /// Atomically persist validated receipt for transaction; caller owns the namespace fence.
    /// 为 transaction 原子持久保存已校验 receipt；调用方拥有命名空间屏障。
    fn write_effects_locked(
        &self,
        transaction: &VulcanPluginActivationTransactionRecord,
        receipt: &VulcanPluginActivationEffects,
    ) -> VulcanPluginStoreResult<()> {
        Self::validate_effects(transaction, receipt)?;
        // Serialize no user arguments, callback output or credentials.
        // 不序列化用户参数、回调输出或凭据。
        let bytes = serde_json::to_vec(receipt)
            .map_err(|_| Error::invalid("Cannot encode candidate effect receipt."))?;
        files::write_encoded_record(&self.effects_path(transaction)?, &bytes, MAX_RECORD_BYTES)
    }
}
