//! Immutable removal freezes original selection and its sole tombstone before any actual publication.
//! 不可变移除在任何实际发布之前固定原选择及其唯一停用记录。

use super::*;
use super::{
    VulcanPluginStoreError as Error, VulcanPluginStoreErrorCode as Code,
    VulcanPluginStoreResult as Result,
};
use std::sync::{Arc, Weak};
use uuid::Uuid;

mod collection;
pub(in crate::generation_store) mod inventory;
mod progress;

/// An exact original selection observation never implies full host-route publication or native retirement.
/// 精确原选择观测绝不暗示完整宿主路由发布或原生退役。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VulcanPluginDeactivationObservation {
    /// The complete original remains selected; no successful removal is claimed.
    /// 完整原记录仍被选定；不宣称移除成功。
    Original,
    /// The exact originally planned tombstone was read back; all host obligations remain separate.
    /// 精确原计划停用记录已经回读；全部宿主义务仍独立。
    Published,
}

/// A same-file original removal owner is neither cloneable nor reconstructible from UUIDs or serialized records.
/// 同文件原移除所有者既不能克隆，也不能从 UUID 或序列化记录重建。
/// Register it strongly before callbacks, policy changes or publication; this reservation performs none of those effects.
/// 在回调、策略变更或发布之前强保留它；此预留不执行这些效果。
pub struct VulcanPluginDeactivationReservation {
    /// The original canonical store remains retained instead of being reconstructed during recovery.
    /// 原规范存储保持保留，而非在恢复期间重建。
    store: VulcanPluginGenerationStore,
    /// The original nonnil request identifies the host's retained resource slot.
    /// 原非空请求标识宿主保留资源槽。
    request_id: Uuid,
    /// The complete original includes exact approval, credential and configuration authority.
    /// 完整原记录包含精确批准、凭据及配置权威。
    original: VulcanPluginActivation,
    /// The same single successor is fixed before any possibly uncertain atomic write.
    /// 相同单个后继在任何可能不确定原子写入之前固定。
    intended: VulcanPluginActivation,
    /// The actual verified source fixes every declared credential slot before callbacks or cleanup can start.
    /// 实际已验证来源在回调或清理能够启动之前固定每个已声明凭据槽位。
    credential_keys: Vec<VulcanPluginCredentialKey>,
    /// A versioned original retains its exact verified journal; legacy selection explicitly has no revision journal.
    /// 版本化原选择保留精确已验证日志；旧选择明确没有修订日志。
    credential_record: Option<VulcanPluginCredentialRecord>,
    /// Every actual historical package and credential journal is frozen before callbacks without retaining additional physical readers.
    /// 每个实际历史包及凭据日志在回调之前固定，不保留额外物理读者。
    inventory: inventory::RemovalInventory,
    /// An admitted write remains recorded before it can succeed, fail or panic.
    /// 已准入写入在其能够成功、失败或 panic 之前保持记录。
    publication_started: bool,
    /// A host local-state original may be attached once; dropping it cannot authorize a replacement preimage snapshot.
    /// 宿主本地状态原实例仅可关联一次；释放它不能授权替代前像快照。
    local_state_reserved: bool,
    /// An attached local original must finish its explicit post-callback freeze before this owner's publication admission.
    /// 已关联本地原实例必须在此所有者发布准入之前完成明确回调后固定。
    local_state_frozen: bool,
    /// The same actual File may be lent to guarded callbacks; no callback opens a replacement namespace.
    /// 同一实际文件可以借给受保护回调；任何回调都不打开替代命名空间。
    namespace: Arc<VulcanPluginLegacyMutationGuard>,
}

impl std::fmt::Debug for VulcanPluginDeactivationReservation {
    /// Print only public request identity; formatter receives no configuration, source path or credential metadata.
    /// 仅打印公共请求身份；formatter 不接收配置、来源路径或凭据元数据。
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("VulcanPluginDeactivationReservation")
            .field("request_id", &self.request_id)
            .field("plugin_id", &self.original.plugin_id)
            .finish_non_exhaustive()
    }
}

impl VulcanPluginDeactivationReservation {
    /// Borrow this original request UUID without granting native ownership or a completion receipt.
    /// 借用此原请求 UUID，不授予原生所有权或完成回执。
    /// No parameters; returns the identity allocated by the host before reservation.
    /// 无参数；返回宿主在预留之前分配的身份。
    pub fn request_id(&self) -> Uuid {
        self.request_id
    }

    /// Borrow the complete original; it remains evidence rather than replacement recovery authority.
    /// 借用完整原记录；其为证据，而非替代恢复权威。
    /// No parameters; returns the immutable original captured under this same namespace File.
    /// 无参数；返回在此相同命名空间文件内捕获的不可变原记录。
    pub fn original(&self) -> &VulcanPluginActivation {
        &self.original
    }

    /// Borrow the sole intended tombstone, including its retained exact cleanup bindings.
    /// 借用唯一期望停用记录，包括其保留的精确清理绑定。
    /// No parameters; returns the frozen successor without reading, publishing or incrementing a revision.
    /// 无参数；返回固定后继，不读取、发布或递增修订。
    pub fn intended(&self) -> &VulcanPluginActivation {
        &self.intended
    }

    /// Borrow all original declared credential slots, including unconfigured ones, without exposing values or granting deletion.
    /// 借用全部原已声明凭据槽位，包含未配置槽位，不暴露值或授予删除权限。
    /// No parameters; returns the inventory verified under this same namespace before any callback or publication.
    /// 无参数；返回在任何回调或发布之前于此相同命名空间内验证的清单。
    pub fn credential_keys(&self) -> &[VulcanPluginCredentialKey] {
        &self.credential_keys
    }

    /// Borrow this actual namespace allocation's weak identity without retaining another physical owner or granting mutation.
    /// 借出此实际命名空间分配的弱身份，不保留另一物理所有者或授予变更。
    /// No parameters; only a caller also retaining this original may use the weak identity for exact state-preimage binding.
    /// 无参数；仅同时保留此原实例的调用方可用弱身份绑定精确状态前像。
    pub(crate) fn namespace_identity(&self) -> Weak<VulcanPluginLegacyMutationGuard> {
        Arc::downgrade(&self.namespace)
    }

    /// Compare weak namespace identity with this same actual retained allocation, rather than UUID or serialized evidence.
    /// 对照此相同实际保留分配比较弱命名空间身份，而非 UUID 或序列化证据。
    /// identity is non-authoritative binding metadata; returns true only for the same original File's Arc allocation.
    /// identity 为不具权威绑定元数据；仅相同原文件引用计数分配返回真。
    pub(crate) fn matches_namespace_identity(
        &self,
        identity: &Weak<VulcanPluginLegacyMutationGuard>,
    ) -> bool {
        Weak::ptr_eq(&Arc::downgrade(&self.namespace), identity)
    }

    /// Require the sole original namespace owner after current layout validation, without claiming whole native or MCP retirement.
    /// 当前布局校验之后要求唯一原命名空间所有者，不宣称整个原生或 MCP 退役。
    /// No parameters; returns Busy while the actual original callback or diagnostics retains this File.
    /// 无参数；实际原回调或诊断保留此文件时返回忙碌。
    pub(crate) fn require_no_lifecycle_borrowers(&self) -> Result<()> {
        self.validate(&self.store)?;
        if Arc::strong_count(&self.namespace) != 1 {
            return Err(Error::new(
                Code::Busy,
                "Original removal lifecycle ownership remains retained; selection publication is blocked.",
            ));
        }
        Ok(())
    }

    /// Attach one actual local preimage owner after every inert state capture and exact binding check has succeeded.
    /// 每个未启动状态捕获及精确绑定检查成功之后，关联单个实际本地前像所有者。
    /// No parameters; returns unit for first pre-publication admission and rejects any replacement even after local-owner drop.
    /// 无参数；首次发布前准入返回空值，即使本地所有者释放仍拒绝任何替代。
    pub(crate) fn reserve_local_state_owner(&mut self) -> Result<()> {
        self.require_no_lifecycle_borrowers()?;
        if self.local_state_reserved
            || self.publication_started
            || self.observe()? != VulcanPluginDeactivationObservation::Original
        {
            return Err(Error::new(
                Code::Conflict,
                "Original removal local state ownership cannot be replaced or reserved after publication admission.",
            ));
        }
        self.local_state_reserved = true;
        Ok(())
    }

    /// Require this actual attached original's first pre-publication cleanup freeze before any state snapshot assignment.
    /// 任何状态快照赋值之前，要求此实际关联原实例首次发布前清理固定。
    /// No parameters; returns unit without changing phase or admitting publication.
    /// 无参数；返回空值，不改变阶段或准入发布。
    pub(crate) fn validate_local_state_freeze(&self) -> Result<()> {
        self.require_no_lifecycle_borrowers()?;
        if !self.local_state_reserved
            || self.local_state_frozen
            || self.publication_started
            || self.observe()? != VulcanPluginDeactivationObservation::Original
        {
            return Err(Error::new(
                Code::Conflict,
                "Original removal local freeze requires its one attached pre-publication owner.",
            ));
        }
        Ok(())
    }

    /// Record true local freeze adjacent to the same original's successful snapshot assignment, with no fallible gap.
    /// 与相同原实例成功快照赋值相邻记录真实本地固定，无可失败间隙。
    /// No parameters or return value; the manager must first validate this same pair and successfully freeze every file.
    /// 无参数或返回值；管理器必须先验证相同配对并成功固定每个文件。
    pub(crate) fn record_local_state_freeze(&mut self) {
        self.local_state_frozen = true;
    }

    /// Lend only the actual original namespace to an inert managed-removal callback before publication admission.
    /// 仅在发布准入之前，将实际原命名空间借给未启动受管移除回调。
    /// No parameters; returns the same guard Arc after exact original validation, never a reconstructed file owner.
    /// 无参数；精确原校验后返回同一守卫 Arc，绝不返回重建文件所有者。
    pub(crate) fn lifecycle_guard(&self) -> Result<Arc<VulcanPluginLegacyMutationGuard>> {
        self.validate(&self.store)?;
        if self.publication_started {
            return Err(Error::new(
                Code::Conflict,
                "Original removal lifecycle cannot be admitted after selection publication starts.",
            ));
        }
        self.namespace
            .validate_owner(&self.store, &self.original.plugin_id)?;
        Ok(Arc::clone(&self.namespace))
    }

    /// Require the exact admitted tombstone before a manager may project removal through this same namespace.
    /// 管理器通过此相同命名空间投影移除之前，要求精确已准入停用记录。
    /// store must match the retained canonical root; returns unit for the actual published successor without a new lock.
    /// store 必须匹配保留规范根；实际已发布后继返回空值，不取得新锁。
    pub(crate) fn validate_published_for_catalog(
        &self,
        store: &VulcanPluginGenerationStore,
    ) -> Result<()> {
        self.validate(store)?;
        if self.observe()? != VulcanPluginDeactivationObservation::Published {
            return Err(Error::new(
                Code::Conflict,
                "Original removal catalog requires its exact admitted tombstone.",
            ));
        }
        Ok(())
    }

    /// Read the sole exact published tombstone under the retained original File, excluding non-cooperating record replacement.
    /// 在保留原文件内读取唯一精确已发布停用记录，排除非协作记录替换。
    /// store is the caller's existing canonical store; returns actual full selection and no package/native ownership for removal.
    /// store 为调用方既有规范存储；返回实际完整选择，移除不携带插件包／原生所有权。
    pub(crate) fn pin_active_for_catalog(
        &self,
        store: &VulcanPluginGenerationStore,
    ) -> Result<VulcanPluginActiveGeneration> {
        self.validate_published_for_catalog(store)?;
        #[cfg(test)]
        self.namespace
            .observe_catalog_selection_after_validation_for_test()?;
        // Reuse the sole physical selection reader under the same exclusive File and verify its second actual read.
        // 在相同排他文件内复用唯一物理选择读取器，并核验其第二次实际读取。
        let selected = store.pin_active_locked(&self.original.plugin_id)?;
        if selected.activation.as_ref() != Some(&self.intended) {
            return Err(Error::new(
                Code::Conflict,
                "Original removal catalog read an unowned replacement selection.",
            ));
        }
        Ok(selected)
    }

    /// Arm the shared exact-owner observer with actual foreign producer bytes after removal validation.
    /// 在移除验证之后，使用实际外来生产者字节启用共享精确所有者观察器。
    /// bytes must originate from a real isolated selection writer; returns scoped test-only cleanup and no production authority.
    /// bytes 必须来自真实隔离选择写入器；返回作用域仅测试清理，不携带生产权威。
    #[cfg(test)]
    pub(crate) fn replace_catalog_selection_after_validation_for_test(
        &self,
        bytes: Vec<u8>,
    ) -> impl Drop {
        self.namespace
            .replace_catalog_selection_after_validation_for_test(bytes)
    }

    /// Validate the same literal store and retained owner before any write or reconciliation.
    /// 在任何写入或核对之前校验相同字面存储及保留所有者。
    /// store is the actual caller; returns unit for unchanged strict layout and no unrelated pending activation.
    /// store 为实际调用方；严格布局未变且无无关待处理激活时返回空值。
    fn validate(&self, store: &VulcanPluginGenerationStore) -> Result<()> {
        if self.store.root != store.root {
            return Err(Error::new(
                Code::Conflict,
                "Original deactivation belongs to another store.",
            ));
        }
        self.store.validate_layout()?;
        self.store
            .ensure_no_pending_activation_locked(&self.original.plugin_id)
    }

    /// Observe only the exact original or this owner's intended tombstone under its original namespace File.
    /// 在原命名空间文件内，仅观测精确原记录或此所有者期望停用记录。
    /// No parameters; third-party, missing and corrupt records fail while retaining the same owner.
    /// 无参数；第三方、缺失及损坏记录失败，同时保留相同所有者。
    fn observe(&self) -> Result<VulcanPluginDeactivationObservation> {
        match self.store.read_activation(&self.original.plugin_id)? {
            Some(current) if current == self.original => {
                Ok(VulcanPluginDeactivationObservation::Original)
            }
            Some(current) if self.publication_started && current == self.intended => {
                Ok(VulcanPluginDeactivationObservation::Published)
            }
            _ => Err(Error::new(
                Code::Conflict,
                "Original deactivation selection has an unowned current record.",
            )),
        }
    }

    /// Publish the single frozen successor after admitting its original write intent.
    /// 准入原写入意图之后发布单个固定后继。
    /// No parameters; errors retain this owner and require exact original readback rather than a new removal request.
    /// 无参数；错误保留此所有者，要求精确原回读，而非新移除请求。
    fn write_intended(&mut self) -> Result<VulcanPluginDeactivationObservation> {
        // This is only a callback-borrow fence, not proof that every MCP/native owner or host route retired.
        // 此处仅为回调借用屏障，不证明所有 MCP／原生所有者或宿主路由均已退役。
        self.require_no_lifecycle_borrowers()?;
        if self.local_state_reserved && !self.local_state_frozen {
            return Err(Error::new(
                Code::Busy,
                "Original removal local cleanup state must be frozen before selection publication.",
            ));
        }
        self.publication_started = true;
        self.store.write_activation(&self.intended)?;
        #[cfg(test)]
        observation::after_write(&self.store.root, self.request_id)?;
        self.observe()
    }
}

impl VulcanPluginGenerationStore {
    /// Reserve expected's actual active selection and its sole monotonic tombstone without publishing or deleting resources.
    /// 预留 expected 实际有效选择及其唯一单调停用记录，不发布或删除资源。
    /// request_id is nonnil and expected is the complete original active record; returns actual exclusive namespace ownership.
    /// request_id 非空，expected 为完整原有效记录；返回实际排他命名空间所有权。
    pub fn reserve_deactivation(
        &self,
        request_id: Uuid,
        expected: &VulcanPluginActivation,
    ) -> Result<VulcanPluginDeactivationReservation> {
        if request_id.is_nil() || expected.generation.is_none() {
            return Err(Error::invalid(
                "Original removal requires a nonnil request and an explicitly active selection.",
            ));
        }
        self.validate_layout()?;
        // Capture the complete selection under the one retained exclusive File, before allocating a successor intent.
        // 在单个保留排他文件内捕获完整选择，先于分配后继意图。
        let namespace =
            Arc::new(self.acquire_legacy_mutation_guard(&expected.plugin_id, Some(expected))?);
        // Validate actual package bytes, complete approval and versioned journal under the same File before retaining cleanup evidence.
        // 在保留清理证据之前，于相同文件内验证实际包字节、完整批准及版本化日志。
        let source = self.pin_active_locked(&expected.plugin_id)?;
        if source.activation.as_ref() != Some(expected) {
            return Err(Error::new(
                Code::Conflict,
                "Original removal source changed during actual verification.",
            ));
        }
        // The constructor requires an active source; absence cannot be interpreted as an empty cleanup inventory.
        // 构造器要求有效来源；缺失不能被解释为空清理清单。
        let package = source
            .generation
            .as_ref()
            .ok_or_else(|| Error::invalid("Original removal has no verified active package."))?;
        // These original names are evidence; the temporary actual pins are released while the namespace remains held.
        // 此些原名称为证据；临时实际固定被释放，同时命名空间保持持有。
        let credential_keys = package.credential_keys.clone();
        // Only the existing verified selection reader may supply a versioned cleanup record.
        // 仅既有已验证选择读取器可以提供版本化清理记录。
        let credential_record = source
            .credentials
            .as_ref()
            .map(|credentials| credentials.record().clone());
        // Freeze all real namespace histories while the same original File excludes cooperating writers, before any effect admission.
        // 相同原文件排除协作写入者期间固定全部真实命名空间历史，先于任何效果准入。
        let inventory = self.capture_removal_inventory_locked(&expected.plugin_id)?;
        if !inventory.packages.iter().any(|record| {
            record.identity == package.identity && record.state == GenerationState::Prepared
        }) || credential_record
            .as_ref()
            .is_some_and(|selected| !inventory.credentials.contains(selected))
        {
            return Err(Error::new(
                Code::Conflict,
                "Original removal inventory does not contain its complete verified selected source.",
            ));
        }
        // The existing sole successor producer retains credentials and configuration without downgrading approval.
        // 既有唯一后继生产者保留凭据及配置，不降级批准。
        let intended = self.plan_activation_successor(
            &expected.plugin_id,
            Some(expected),
            None,
            None,
            None,
            None,
        )?;
        Ok(VulcanPluginDeactivationReservation {
            store: self.clone(),
            request_id,
            original: expected.clone(),
            intended,
            credential_keys,
            credential_record,
            inventory,
            publication_started: false,
            local_state_reserved: false,
            local_state_frozen: false,
            namespace,
        })
    }

    /// Start this retained owner's one removal publication; repeated foreground starts are refused.
    /// 启动此保留所有者的一次移除发布；拒绝重复前台启动。
    /// original is borrowed in place; returns exact storage observation, not a full host-uninstall receipt.
    /// 原地借用 original；返回精确存储观测，而非完整宿主卸载回执。
    pub fn publish_deactivation_retained(
        &self,
        original: &mut VulcanPluginDeactivationReservation,
    ) -> Result<VulcanPluginDeactivationObservation> {
        original.validate(self)?;
        if original.publication_started {
            return Err(Error::new(
                Code::Conflict,
                "Original deactivation publication was already admitted; inspect the same owner.",
            ));
        }
        if original.observe()? != VulcanPluginDeactivationObservation::Original {
            return Err(Error::new(
                Code::Conflict,
                "Original selection is not available for removal.",
            ));
        }
        original.write_intended()
    }

    /// Continue this actual retained `original` using its sole existing publication intent.
    /// 使用此实际保留 `original` 唯一既有发布意图继续处理。
    /// Returns the existing initial/resume observation; no host boolean or error text can select replay.
    /// 返回既有初始／恢复观测；宿主布尔值或错误文本不能选择重放。
    pub fn continue_deactivation_retained(
        &self,
        original: &mut VulcanPluginDeactivationReservation,
    ) -> Result<VulcanPluginDeactivationObservation> {
        if original.publication_started {
            self.resume_deactivation_retained(original)
        } else {
            self.publish_deactivation_retained(original)
        }
    }

    /// Reconcile the same retained original without writing, deleting, dropping its File or inferring route readiness.
    /// 核对相同保留原实例，不写入、删除、释放其文件或推断路由就绪。
    /// original supplies actual namespace ownership; returns exact original/published observation or a retained conflict.
    /// original 提供实际命名空间所有权；返回精确原记录／已发布观测，或保留冲突。
    pub fn reconcile_deactivation_retained(
        &self,
        original: &VulcanPluginDeactivationReservation,
    ) -> Result<VulcanPluginDeactivationObservation> {
        original.validate(self)?;
        original.observe()
    }

    /// Explicitly finish the admitted original selection write from exact readback, without replaying callbacks or incrementing revision.
    /// 按精确回读明确完成已准入原选择写入，不重放回调或递增修订。
    /// original must have admitted its sole write; a matching published target is a read-only success.
    /// original 必须已准入唯一写入；匹配已发布目标为只读成功。
    pub fn resume_deactivation_retained(
        &self,
        original: &mut VulcanPluginDeactivationReservation,
    ) -> Result<VulcanPluginDeactivationObservation> {
        original.validate(self)?;
        if !original.publication_started {
            return Err(Error::new(
                Code::Conflict,
                "Unstarted original removal must use its initial publication entry.",
            ));
        }
        match original.observe()? {
            VulcanPluginDeactivationObservation::Published => {
                Ok(VulcanPluginDeactivationObservation::Published)
            }
            VulcanPluginDeactivationObservation::Original => original.write_intended(),
        }
    }
}

#[cfg(test)]
pub(in crate::generation_store) mod observation {
    use super::*;
    use std::cell::RefCell;

    /// Exactly one scoped observer acts after a real successful atomic selection publication.
    /// 精确单个作用域观察器在真实成功原子选择发布之后作用。
    struct Fault {
        /// The actual canonical store and original UUID isolate this observer from unrelated tests.
        /// 实际规范存储及原 UUID 将此观察器与无关测试隔离。
        key: (PathBuf, Uuid),
        /// Panic versus lost return occurs only after the actual write; neither fabricates filesystem success.
        /// panic 与返回丢失均仅发生在实际写入之后；两者都不虚构文件系统成功。
        panic: bool,
        /// One fault is consumed without replaying it on original recovery.
        /// 单个故障被消费，不在原恢复时重放。
        armed: bool,
        /// The count records actual successful writes by this same producer.
        /// 次数记录此相同生产者的实际成功写入。
        writes: usize,
    }
    thread_local! {
        /// Thread-local ownership cannot inject faults into an independent original worker.
        /// 线程局部所有权不能向独立原工作者注入故障。
        static FAULT: RefCell<Option<Fault>> = const { RefCell::new(None) };
    }

    /// A same-thread scope preserves the original observer count through panic unwinding.
    /// 同线程作用域跨 panic 展开保留原观察器次数。
    pub(in crate::generation_store) struct Scope {
        /// Same original identity protects independent scopes from stale retirement.
        /// 相同原身份使独立作用域免受陈旧退役影响。
        key: (PathBuf, Uuid),
        /// The observer must remain on its original thread.
        /// 观察器必须留在原线程。
        _thread: std::marker::PhantomData<std::rc::Rc<()>>,
    }
    impl Scope {
        /// Read the actual successful original publication count; returns no fabricated default when scope is missing.
        /// 读取实际成功原发布次数；作用域缺失时不返回虚构默认值。
        pub(in crate::generation_store) fn writes(&self) -> usize {
            FAULT.with(|fault| {
                // The same actual observer is required to report producer evidence.
                // 报告生产者证据需要相同实际观察器。
                let fault = fault.borrow();
                // Keep original evidence strict rather than inferring a count after disposal.
                // 保持原证据严格，而非在销毁之后推断次数。
                let fault = fault.as_ref().expect("original observer remains retained");
                assert_eq!(fault.key, self.key);
                fault.writes
            })
        }
    }
    impl Drop for Scope {
        /// Drop only the same thread's observer; never modify production selection or release another owner's namespace.
        /// 仅丢弃相同线程观察器；绝不修改生产选择或释放其他所有者命名空间。
        fn drop(&mut self) {
            FAULT.with(|fault| {
                // Clear only this exact observation scope, including during test unwinding.
                // 仅清除此精确观测作用域，包括测试展开期间。
                let mut fault = fault.borrow_mut();
                if fault.as_ref().is_some_and(|fault| fault.key == self.key) {
                    fault.take();
                }
            });
        }
    }
    /// Arm a real postwrite fault for root/request; panic chooses worker loss, otherwise an explicit lost return.
    /// 为 root／request 启用真实写后故障；panic 选择工作者丢失，否则选择明确返回丢失。
    /// Returns the same-thread scope without granting production recovery authority.
    /// 返回同线程作用域，不授予生产恢复权威。
    pub(in crate::generation_store) fn arm(root: &Path, request: Uuid, panic: bool) -> Scope {
        // The exact key is allocated before the observer can affect a real writer.
        // 精确键在观察器能够影响真实写入者之前分配。
        let key = (root.to_owned(), request);
        FAULT.with(|fault| {
            let mut fault = fault.borrow_mut();
            assert!(fault.is_none());
            *fault = Some(Fault {
                key: key.clone(),
                panic,
                armed: true,
                writes: 0,
            });
        });
        Scope {
            key,
            _thread: std::marker::PhantomData,
        }
    }
    /// Observe actual successful publication for this original; a fault occurs only after its atomic write.
    /// 观测此原实例实际成功发布；故障仅在其原子写入之后发生。
    /// root/request select no alternate source; returns a one-shot lost-return error or panics after preserving count.
    /// root／request 不选择替代来源；在保留次数之后返回一次性返回丢失错误或 panic。
    pub(super) fn after_write(root: &Path, request: Uuid) -> Result<()> {
        // Capture the fault outside the RefCell borrow so unwind retains accessible original evidence.
        // 在 RefCell 借用之外捕获故障，使展开保留可访问原证据。
        let action = FAULT.with(|fault| {
            let mut fault = fault.borrow_mut();
            match fault.as_mut() {
                Some(fault) if fault.key.0 == root && fault.key.1 == request => {
                    fault.writes += 1;
                    if fault.armed {
                        fault.armed = false;
                        Some(fault.panic)
                    } else {
                        None
                    }
                }
                _ => None,
            }
        });
        match action {
            Some(true) => {
                panic!("actual original deactivation write completed before worker panic")
            }
            Some(false) => Err(Error::new(
                Code::Storage,
                "Actual original deactivation write completed before return loss.",
            )),
            None => Ok(()),
        }
    }
}
