//! Prepared original predecessor participants transfer only after the independent Transferred record is confirmed.
//! 已准备原前驱参与者仅在独立 Transferred 记录确认后转交。

use super::*;
use crate::{
    VulcanPluginActivationPredecessorPublication, VulcanPluginPredecessorRestorationSource,
};

/// Borrowed registration binds the exact original source and final publication throughout prepared native transfer.
/// 借用注册凭据在已准备原生转交全程绑定精确原来源及最终发布。
pub struct VulcanPluginActivationPredecessorRegistration<'gate> {
    /// The actual existing manager guard excludes new publication and competing registration.
    /// 实际既有管理器守卫排除新发布及竞争注册。
    _gate: &'gate std::sync::MutexGuard<'gate, ()>,
    /// Exact manager allocation cannot be replaced by equal catalog values.
    /// 精确管理器分配不能由相等目录值替代。
    manager: &'gate crate::runtime::VulcanPluginManagerState,
    /// Full Aborted values remain observation, never substitute for physical ownership.
    /// 完整 Aborted 值保持为观测，绝不替代物理所有权。
    transaction: &'gate crate::VulcanPluginActivationTransaction,
    /// The one actual source retains its independent original reservation.
    /// 唯一实际来源保留其独立原预留权。
    source: &'gate Arc<VulcanPluginPredecessorRestorationSource>,
    /// Exact successful Full witness must remain identical throughout preparation.
    /// 精确成功 Full 凭据在准备全程必须保持相同。
    publication: &'gate VulcanPluginActivationPredecessorPublication,
}

impl<'gate> VulcanPluginActivationPredecessorRegistration<'gate> {
    /// Bind the held actual gate to manager/transaction/source/publication after final identity validation.
    /// 最终身份校验后将已持有实际门绑定至 manager／transaction／source／publication。
    /// Return a borrowed scope, without allocating a lock, owner or durable authority.
    /// 返回借用作用域，不分配锁、所有者或持久权威。
    pub(in crate::runtime::formal::instances) fn new(
        gate: &'gate std::sync::MutexGuard<'gate, ()>,
        manager: &'gate crate::runtime::VulcanPluginManagerState,
        transaction: &'gate crate::VulcanPluginActivationTransaction,
        source: &'gate Arc<VulcanPluginPredecessorRestorationSource>,
        publication: &'gate VulcanPluginActivationPredecessorPublication,
    ) -> Self {
        Self {
            _gate: gate,
            manager,
            transaction,
            source,
            publication,
        }
    }

    /// Compare manager's exact guarded allocation; return false without locking or I/O for foreign managers.
    /// 比较 manager 精确受守卫分配；对外来管理器不加锁或执行 I/O 地返回假。
    pub fn matches_manager(&self, manager: &VulcanPluginRuntimeManager) -> bool {
        std::ptr::eq(self.manager, manager.state.as_ref())
    }

    /// Verify the same scoped full original and actual source/publication; no Pending read or startup grant occurs.
    /// 校验相同作用域完整原请求及实际 source／publication；不读取 Pending 或授予启动。
    /// Returns a conflict before native transfer preparation for substituted manager, source or successful seal.
    /// 替换管理器、来源或成功封闭凭据时，在原生转交准备前返回冲突。
    pub(in crate::runtime) fn validate_original(
        &self,
        manager: &VulcanPluginRuntimeManager,
        transaction: &crate::VulcanPluginActivationTransaction,
        source: &Arc<VulcanPluginPredecessorRestorationSource>,
        publication: &VulcanPluginActivationPredecessorPublication,
    ) -> Result<(), VulcanPluginRuntimeError> {
        if !self.matches_manager(manager)
            || self.transaction.record() != transaction.record()
            || !Arc::ptr_eq(self.source, source)
            || !std::ptr::eq(self.publication, publication)
        {
            return Err(VulcanPluginRuntimeError::new(
                "Predecessor handoff registration belongs to another actual original.",
            ));
        }
        publication.validate_current_identity(manager, transaction, source)
    }
}

/// Sealed native evidence identifies the same successfully observed predecessor runtime and lease.
/// 封闭原生证据标识相同已成功观测前驱运行时及租约。
pub struct VulcanPluginMcpPredecessorHandoffEvidence {
    /// Exact original request is inherited from the actual source, never created by a native participant.
    /// 精确原请求继承自实际来源，绝不由原生参与者创建。
    request_id: uuid::Uuid,
    /// Full immutable old selection includes configuration and credential revision.
    /// 完整不可变旧选择包含配置及凭据修订。
    predecessor: crate::VulcanPluginActivation,
    /// Original recovery successor generation remains unchanged during joint transfer.
    /// 原恢复后继代次在联合转交期间保持不变。
    generation: u64,
    /// Actual native runtime allocated this immutable identity before startup.
    /// 实际原生运行时在启动前分配此不可变身份。
    runtime_instance_id: uuid::Uuid,
    /// Actual native partition lease allocated this immutable identity before startup.
    /// 实际原生分区租约在启动前分配此不可变身份。
    partition_lease_instance_id: uuid::Uuid,
    /// Pure profile fixes the first registered native issuer without retaining recovery files.
    /// 纯画像固定首次注册原生签发者，不保留恢复文件。
    profile: crate::VulcanPluginPredecessorMcpProfile,
    /// Weak actual source allocation rejects equal-value reconstructions without extending physical ownership.
    /// 弱实际来源分配拒绝相等值重建，且不延长物理所有权。
    source: Weak<VulcanPluginPredecessorRestorationSource>,
}

impl VulcanPluginMcpPredecessorHandoffEvidence {
    /// Seal source/profile and actual runtime/lease IDs only while the original successful control guard is held.
    /// 仅原成功控制守卫持有时封闭 source／profile 及实际运行时／租约身份。
    /// Return pure evidence without allocating owners, files, processes or replacement identities.
    /// 返回纯证据，不分配所有者、文件、进程或替代身份。
    pub(in crate::runtime) fn new(
        source: &Arc<VulcanPluginPredecessorRestorationSource>,
        profile: &crate::VulcanPluginPredecessorMcpProfile,
        runtime_instance_id: uuid::Uuid,
        partition_lease_instance_id: uuid::Uuid,
    ) -> Self {
        Self {
            request_id: source.request_id(),
            predecessor: source
                .original()
                .predecessor()
                .expect("validated managed predecessor")
                .clone(),
            generation: source.plugin().generation,
            runtime_instance_id,
            partition_lease_instance_id,
            profile: profile.clone(),
            source: Arc::downgrade(source),
        }
    }

    /// Return the exact original request identity; no inputs or startup permission.
    /// 返回精确原请求身份；无输入或启动权限。
    pub fn request_id(&self) -> uuid::Uuid {
        self.request_id
    }
    /// Borrow the full immutable old selection; no inputs or physical owner acquisition.
    /// 借用完整不可变旧选择；无输入或物理所有者获取。
    pub fn predecessor(&self) -> &crate::VulcanPluginActivation {
        &self.predecessor
    }
    /// Return the actual recovery successor generation; no inputs or allocation.
    /// 返回实际恢复后继代次；无输入或分配。
    pub fn generation(&self) -> u64 {
        self.generation
    }
    /// Return the original actual native runtime identity; no inputs or resource construction.
    /// 返回原实际原生运行时身份；无输入或资源构造。
    pub fn runtime_instance_id(&self) -> uuid::Uuid {
        self.runtime_instance_id
    }
    /// Return the original actual native lease identity; no inputs or resource construction.
    /// 返回原实际原生租约身份；无输入或资源构造。
    pub fn partition_lease_instance_id(&self) -> uuid::Uuid {
        self.partition_lease_instance_id
    }
    /// Check source and authority against sealed actual evidence; return false for foreign allocation or issuer.
    /// 按封闭实际证据检查 source 及 authority；外来分配或签发者返回假。
    pub(in crate::runtime) fn matches_original(
        &self,
        source: &Arc<VulcanPluginPredecessorRestorationSource>,
        authority: &crate::VulcanPluginRegisteredNativeMcpHostAuthority,
    ) -> bool {
        Weak::ptr_eq(&self.source, &Arc::downgrade(source))
            && self.profile.matches_authority(authority)
    }
}

/// Confirmed independent Transferred evidence authorizes only prevalidated original predecessor memory transfer.
/// 已确认独立 Transferred 证据仅授权已预校验原前驱内存转交。
pub struct VulcanPluginActivationPredecessorCompletion {
    /// Exact original request remains Aborted in the activation ledger.
    /// 精确原请求在激活账本中保持 Aborted。
    request_id: uuid::Uuid,
    /// Full old selection stays immutable across durable and memory transfer.
    /// 完整旧选择跨持久及内存转交保持不可变。
    predecessor: crate::VulcanPluginActivation,
    /// Actual recovery successor generation does not change at completion.
    /// 实际恢复后继代次在完成时不变。
    generation: u64,
    /// Only an actual prepared global native participant contributes runtime/lease identities.
    /// 仅实际已准备全局原生参与者贡献运行时／租约身份。
    mcp_instance_ids: Option<(uuid::Uuid, uuid::Uuid)>,
}

impl VulcanPluginActivationPredecessorCompletion {
    /// Capture source and mcp before durable writing; caller issues this witness only after confirmed Transferred.
    /// 持久写入前捕获 source 及 mcp；调用方仅在已确认 Transferred 后签发此凭据。
    /// Return an original identity witness, without changing any actual resources or admission.
    /// 返回原身份凭据，不改变任何实际资源或入场。
    pub(in crate::runtime::formal::instances) fn new(
        source: &Arc<VulcanPluginPredecessorRestorationSource>,
        mcp: Option<&VulcanPluginMcpPredecessorHandoffEvidence>,
    ) -> Self {
        Self {
            request_id: source.request_id(),
            predecessor: source
                .original()
                .predecessor()
                .expect("validated managed predecessor")
                .clone(),
            generation: source.plugin().generation,
            mcp_instance_ids: mcp.map(|evidence| {
                (
                    evidence.runtime_instance_id(),
                    evidence.partition_lease_instance_id(),
                )
            }),
        }
    }
    /// Return the confirmed original request; no inputs or reopening authority.
    /// 返回已确认原请求；无输入或重新开放权威。
    pub fn request_id(&self) -> uuid::Uuid {
        self.request_id
    }
    /// Borrow the confirmed full old selection; no inputs or owner allocation.
    /// 借用已确认完整旧选择；无输入或所有者分配。
    pub fn predecessor(&self) -> &crate::VulcanPluginActivation {
        &self.predecessor
    }
    /// Return the actual recovery successor generation; no inputs or new generation allocation.
    /// 返回实际恢复后继代次；无输入或新代次分配。
    pub fn generation(&self) -> u64 {
        self.generation
    }
    /// Return confirmed actual runtime/lease identities; no inputs, and absence cannot authorize MCP admission.
    /// 返回已确认实际运行时／租约身份；无输入，缺失不能授权 MCP 入场。
    pub fn mcp_instance_ids(&self) -> Option<(uuid::Uuid, uuid::Uuid)> {
        self.mcp_instance_ids
    }
    /// Match every original identity with evidence; foreign witnesses must not transfer or open actual owners.
    /// 匹配 evidence 的每个原身份；外来凭据不得转交或开放实际所有者。
    pub fn matches_mcp_evidence(
        &self,
        evidence: &VulcanPluginMcpPredecessorHandoffEvidence,
    ) -> bool {
        self.request_id == evidence.request_id()
            && self.predecessor == *evidence.predecessor()
            && self.generation == evidence.generation()
            && self.mcp_instance_ids
                == Some((
                    evidence.runtime_instance_id(),
                    evidence.partition_lease_instance_id(),
                ))
    }
}

/// Actual predecessor guards retain admission, transport, source and lease through durable completion.
/// 实际前驱守卫跨持久完成保留入场、传输、来源及租约。
/// Implementations must finish all fallible validation before the writer; confirmation only changes those same owners in memory.
/// 实现必须在写入器前完成全部可失败校验；确认仅在内存中改变这些相同所有者。
pub trait VulcanPluginActivationPredecessorHandoffParticipant {
    /// Borrow actual sealed native evidence; None is valid only for a genuinely absent global declaration.
    /// 借用实际封闭原生证据；仅确实缺失全局声明时 None 有效。
    fn mcp_evidence(&self) -> Option<&VulcanPluginMcpPredecessorHandoffEvidence>;
    /// Transfer the same original owners after matching confirmation; perform no I/O, creation or fallible work.
    /// 匹配 confirmation 后转交相同原所有者；不执行 I/O、创建或可失败工作。
    fn confirm_transferred(&mut self, confirmation: &VulcanPluginActivationPredecessorCompletion);
}

/// An explicitly registered original native host prepares lifetime-bound actual guards under core registration.
/// 显式注册原原生宿主在核心注册门下准备受寿命约束实际守卫。
pub trait VulcanPluginNativeMcpPredecessorHandoffAdapter {
    /// Actual prepared guards cannot outlive this adapter or borrowed original registration.
    /// 实际已准备守卫不能超过此适配器或借用原注册寿命。
    type Guard<'gate>: VulcanPluginActivationPredecessorHandoffParticipant
    where
        Self: 'gate;
    /// Borrow the first explicit native issuer; only the genuinely empty adapter returns None.
    /// 借用首次显式原生签发者；仅确实为空的适配器返回 None。
    fn authority(&self) -> Option<&crate::VulcanPluginRegisteredNativeMcpHostAuthority>;
    /// Prepare actual guarded native resources for the exact manager/transaction/source/publication within registration.
    /// 在注册门内为精确 manager／transaction／source／publication 准备实际受守卫原生资源。
    /// Return only a participant whose actual guards remain held through confirmation; failures preserve original private owners.
    /// 仅返回实际守卫跨确认保持持有的参与者；失败保留原私有所有者。
    fn prepare<'gate>(
        &'gate self,
        manager: &VulcanPluginRuntimeManager,
        transaction: &crate::VulcanPluginActivationTransaction,
        source: &Arc<VulcanPluginPredecessorRestorationSource>,
        publication: &VulcanPluginActivationPredecessorPublication,
        registration: &'gate VulcanPluginActivationPredecessorRegistration<'gate>,
    ) -> Result<Self::Guard<'gate>, VulcanPluginRuntimeError>;
}

/// An actually absent global declaration has no native preparation or resource transfer.
/// 实际缺失全局声明不具有原生准备或资源转交。
pub(in crate::runtime::formal::instances) struct NoPredecessorHandoffAdapter;
/// An empty participant retains the genuine manager scope without pretending to own native resources.
/// 空参与者保留真实管理器作用域，不假称拥有原生资源。
pub struct NoPredecessorHandoffParticipant<'gate> {
    /// Retain the real borrowed scope until completion returns.
    /// 保留真实借用作用域，直至完成返回。
    _registration: &'gate VulcanPluginActivationPredecessorRegistration<'gate>,
}
impl VulcanPluginNativeMcpPredecessorHandoffAdapter for NoPredecessorHandoffAdapter {
    /// The empty participant remains bounded by the actual original registration lifetime.
    /// 空参与者保持受实际原注册生命周期约束。
    type Guard<'gate> = NoPredecessorHandoffParticipant<'gate>;
    /// The empty adapter has no issuer; no inputs and no manufactured authority.
    /// 空适配器无签发者；无输入且不制造权威。
    fn authority(&self) -> Option<&crate::VulcanPluginRegisteredNativeMcpHostAuthority> {
        None
    }
    /// Retain actual registration for the verified empty path; return no fabricated native evidence or owners.
    /// 为已验证空路径保留实际 registration；不返回伪造原生证据或所有者。
    fn prepare<'gate>(
        &'gate self,
        _manager: &VulcanPluginRuntimeManager,
        _transaction: &crate::VulcanPluginActivationTransaction,
        _source: &Arc<VulcanPluginPredecessorRestorationSource>,
        _publication: &VulcanPluginActivationPredecessorPublication,
        registration: &'gate VulcanPluginActivationPredecessorRegistration<'gate>,
    ) -> Result<Self::Guard<'gate>, VulcanPluginRuntimeError> {
        Ok(NoPredecessorHandoffParticipant {
            _registration: registration,
        })
    }
}
impl VulcanPluginActivationPredecessorHandoffParticipant for NoPredecessorHandoffParticipant<'_> {
    /// Empty resources provide no evidence; no inputs or authorization.
    /// 空资源不提供证据；无输入或授权。
    fn mcp_evidence(&self) -> Option<&VulcanPluginMcpPredecessorHandoffEvidence> {
        None
    }
    /// Confirm the verified empty participant without changing or allocating any resources.
    /// 确认已验证空参与者，不改变或分配任何资源。
    fn confirm_transferred(&mut self, _confirmation: &VulcanPluginActivationPredecessorCompletion) {
    }
}
