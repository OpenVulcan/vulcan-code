//! Prepared participants receive completion evidence only after the exact original transaction is confirmed.
//! 仅在精确原事务已确认后，已准备参与者才接收完成证据。

use super::*;

/// Borrowed registration evidence proves the actual manager gate remains held during participant preparation.
/// 借用注册证据证明参与者准备期间，实际管理器注册门保持持有。
/// Private construction and the guard lifetime prevent safe callers from fabricating or retaining unlocked evidence.
/// 私有构造及守卫寿命防止安全调用方伪造或保留已解锁证据。
pub struct VulcanPluginActivationTargetRegistration<'gate> {
    /// Borrow the exact existing gate guard, without allocating a replacement lock or owner.
    /// 借用精确既有注册门守卫，不分配替代锁或所有者。
    _gate: &'gate std::sync::MutexGuard<'gate, ()>,
    /// Actual immutable manager state includes its identity and excludes foreign manager clones.
    /// 实际不可变管理器状态包含其身份，并排除不同管理器实例。
    manager: &'gate crate::runtime::VulcanPluginManagerState,
}

impl<'gate> VulcanPluginActivationTargetRegistration<'gate> {
    /// Borrow the held original guard and its manager; only the combined completion implementation issues evidence.
    /// 借用已持有原守卫及其管理器；仅组合完成实现签发证据。
    pub(super) fn new(
        gate: &'gate std::sync::MutexGuard<'gate, ()>,
        manager: &'gate crate::runtime::VulcanPluginManagerState,
    ) -> Self {
        Self {
            _gate: gate,
            manager,
        }
    }
    /// Return whether manager shares this exact guarded state; comparison performs no I/O or locking.
    /// 返回 manager 是否共享此精确受守卫状态；比较不执行 I/O 或锁定。
    pub fn matches_manager(&self, manager: &VulcanPluginRuntimeManager) -> bool {
        std::ptr::eq(self.manager, manager.state.as_ref())
    }
}

/// Sealed completion evidence authorizes only prevalidated same-owner memory transfer, never fresh preparation.
/// 封闭完成证据仅授权预校验同所有者内存转交，绝不授权新准备。
pub struct VulcanPluginActivationTargetCompletion {
    /// Original request identity is allocated by the transaction, never by a participant.
    /// 原请求身份由事务分配，绝非由参与者分配。
    request_id: uuid::Uuid,
    /// Complete selected target remains bound to this same original completion.
    /// 完整已选目标保持绑定至此相同原完成。
    target: crate::VulcanPluginActivation,
    /// Actual staged routing generation survives transfer unchanged.
    /// 实际暂存路由代次跨转交保持不变。
    generation: u64,
    /// Actual prevalidated MCP owner UUIDs are present only for the genuine prepared participant.
    /// 仅真实已准备参与者具有实际预校验 MCP 所有者 UUID。
    mcp_instance_ids: Option<(uuid::Uuid, uuid::Uuid)>,
}

impl VulcanPluginActivationTargetCompletion {
    /// Prepare sealed evidence from the original transaction and actual generation before any durable write.
    /// 在任何持久写入之前，从原事务及实际 generation 准备封闭证据。
    pub(super) fn new(
        transaction: &crate::VulcanPluginActivationTransaction,
        generation: u64,
        mcp: Option<&VulcanPluginMcpTargetHandoffEvidence>,
    ) -> Self {
        Self {
            request_id: transaction.record().request_id(),
            target: transaction.record().target().clone(),
            generation,
            mcp_instance_ids: mcp.map(|evidence| {
                (
                    evidence.runtime_instance_id(),
                    evidence.partition_lease_instance_id(),
                )
            }),
        }
    }
    /// Return the confirmed original request UUID; this does not authorize reopening another instance.
    /// 返回已确认原请求 UUID；这不授权重开另一实例。
    pub fn request_id(&self) -> uuid::Uuid {
        self.request_id
    }
    /// Borrow the exact complete target used for prepared participant validation.
    /// 借用用于已准备参与者校验的精确完整目标。
    pub fn target(&self) -> &crate::VulcanPluginActivation {
        &self.target
    }
    /// Return the actual original routing generation; no new generation is allocated during confirmation.
    /// 返回实际原路由代次；确认期间不分配新代次。
    pub fn generation(&self) -> u64 {
        self.generation
    }
    /// Return the exact confirmed runtime and lease UUIDs together; absence cannot open MCP resources.
    /// 共同返回精确已确认运行时及租约 UUID；缺失不能打开 MCP 资源。
    pub fn mcp_instance_ids(&self) -> Option<(uuid::Uuid, uuid::Uuid)> {
        self.mcp_instance_ids
    }
    /// Compare all confirmed original identities against evidence; foreign completion must perform no transfer.
    /// 比较全部已确认原身份与 evidence；不同完成凭据必须不执行转交。
    pub fn matches_mcp_evidence(&self, evidence: &VulcanPluginMcpTargetHandoffEvidence) -> bool {
        self.request_id == evidence.request_id()
            && self.target == *evidence.target()
            && self.generation == evidence.generation()
            && self.mcp_instance_ids
                == Some((
                    evidence.runtime_instance_id(),
                    evidence.partition_lease_instance_id(),
                ))
    }
}

/// A prepared participant holds its actual admission and resource guards until confirmed transfer or rollback.
/// 已准备参与者持有其实际入场及资源守卫，直至已确认转交或回退。
/// Implementations must prevalidate all fallible work; confirmation performs only infallible same-owner memory changes.
/// 实现必须预校验全部可失败工作；确认仅执行不可失败同所有者内存变更。
/// Evidence must belong to actual ready backend/source owners whose admission, lifecycle and resource guards remain held.
/// 证据必须属于实际就绪后端／来源所有者，且其入场、生命周期及资源守卫持续持有。
/// Confirmation may transfer only those same owners after matching every witness identity; it performs no fallible or external work.
/// 确认仅可在匹配全部凭据身份后转交这些相同所有者；不执行可失败或外部工作。
/// Implementations cannot release actual guards before confirmation or open resources on a foreign completion witness.
/// 实现不可在确认之前释放实际守卫，或使用不同完成凭据打开资源。
pub trait VulcanPluginActivationTargetHandoffParticipant {
    /// Borrow sealed actual MCP evidence while this participant holds its original source and backend guards.
    /// 此参与者持有原来源及后端守卫时，借用封闭实际 MCP 证据。
    /// None is valid only when the actual target has no global MCP declaration or initialization receipt.
    /// 仅实际目标没有全局 MCP 声明或初始化回执时，None 才有效。
    fn mcp_evidence(&self) -> Option<&VulcanPluginMcpTargetHandoffEvidence>;
    /// Transfer prevalidated original resources while their guards remain held; confirmation is sealed completed evidence.
    /// 原守卫仍持有时转交预校验原资源；confirmation 为封闭已完成凭据。
    /// This method performs no I/O, registration, transport startup, VM creation or fallible validation.
    /// 此方法不执行 I/O、注册、传输启动、VM 创建或可失败校验。
    fn confirm_completed(&mut self, confirmation: &VulcanPluginActivationTargetCompletion);
}

/// An explicitly registered native host prepares lifetime-bound original backend guards inside the manager gate.
/// 显式注册原生宿主在管理器注册门内准备受寿命约束的原后端守卫。
/// The host remains responsible for truthful native readiness; safe lifetime bounds prevent registration evidence escaping.
/// 宿主仍负责真实原生就绪；安全寿命约束防止注册证据逃逸。
pub trait VulcanPluginNativeMcpHandoffAdapter {
    /// Prepared guards borrow the actual registration scope and this registered adapter until they are dropped.
    /// 已准备守卫借用实际注册作用域及此已注册适配器，直至其销毁。
    type Guard<'gate>: VulcanPluginActivationTargetHandoffParticipant
    where
        Self: 'gate;
    /// Borrow the explicit manager-bound native issuer; absence is allowed only for the empty no-MCP adapter.
    /// 借用显式管理器绑定原生签发者；仅空无 MCP 适配器允许缺失。
    fn authority(&self) -> Option<&crate::VulcanPluginRegisteredNativeMcpHostAuthority>;
    /// Prepare original backend/source guards within registration; no guard may outlive registration.
    /// 在注册门内准备原后端／来源守卫；任何守卫不可超出注册寿命。
    /// manager/transaction/catalog identify the actual original selection; errors retain closed actual owners.
    /// manager／transaction／catalog 标识实际原选择；错误保留已关闭实际所有者。
    fn prepare<'gate>(
        &'gate self,
        manager: &VulcanPluginRuntimeManager,
        transaction: &crate::VulcanPluginActivationTransaction,
        catalog: &Arc<crate::VulcanPluginCatalogSnapshot>,
        registration: &'gate VulcanPluginActivationTargetRegistration<'gate>,
    ) -> Result<Self::Guard<'gate>, VulcanPluginRuntimeError>;
}

/// The actual absence of global MCP declarations needs no external native authority or resource owner.
/// 全局 MCP 声明实际缺失时，不需要外部原生权威或资源所有者。
pub(super) struct NoTargetHandoffAdapter;

/// The empty prepared participant borrows the same real registration scope until completion returns.
/// 空已准备参与者借用相同真实注册作用域，直至完成返回。
pub(super) struct NoTargetHandoffParticipant<'gate> {
    /// Actual scope is retained without a new lock or runtime resource.
    /// 保留实际作用域，不新建锁或运行时资源。
    _registration: &'gate VulcanPluginActivationTargetRegistration<'gate>,
}

impl VulcanPluginNativeMcpHandoffAdapter for NoTargetHandoffAdapter {
    type Guard<'gate> = NoTargetHandoffParticipant<'gate>;
    /// Empty native targets cannot manufacture registered host authority.
    /// 空原生目标不能虚构已注册宿主权威。
    fn authority(&self) -> Option<&crate::VulcanPluginRegisteredNativeMcpHostAuthority> {
        None
    }
    /// Borrow registration for the no-resource path; manager has already rejected any actual global MCP declaration.
    /// 为无资源路径借用注册门；管理器已拒绝任何实际全局 MCP 声明。
    fn prepare<'gate>(
        &'gate self,
        _manager: &VulcanPluginRuntimeManager,
        _transaction: &crate::VulcanPluginActivationTransaction,
        _catalog: &Arc<crate::VulcanPluginCatalogSnapshot>,
        registration: &'gate VulcanPluginActivationTargetRegistration<'gate>,
    ) -> Result<Self::Guard<'gate>, VulcanPluginRuntimeError> {
        Ok(NoTargetHandoffParticipant {
            _registration: registration,
        })
    }
}

impl VulcanPluginActivationTargetHandoffParticipant for NoTargetHandoffParticipant<'_> {
    /// The empty participant owns no actual MCP resources and cannot furnish sealed evidence.
    /// 空参与者不拥有实际 MCP 资源，无法提供封闭证据。
    fn mcp_evidence(&self) -> Option<&VulcanPluginMcpTargetHandoffEvidence> {
        None
    }
    /// Confirm the empty participant without allocating or opening any resources.
    /// 确认空参与者，不分配或打开任何资源。
    fn confirm_completed(&mut self, _confirmation: &VulcanPluginActivationTargetCompletion) {}
}

/// A genuine retained MCP source seals original request and actual same-process runtime/lease identities.
/// 真实保留 MCP 来源封闭原请求及实际同进程运行时／租约身份。
/// Private construction prevents a no-owner wrapper from fabricating preparation evidence.
/// 私有构造防止无所有者包装器伪造准备证据。
pub struct VulcanPluginMcpTargetHandoffEvidence {
    /// Original activation UUID is retained without creating a transport-specific request UUID.
    /// 保留原激活 UUID，不创建传输专属请求 UUID。
    request_id: uuid::Uuid,
    /// Full selected target binds configuration, credentials, package and permissions together.
    /// 完整已选目标共同绑定配置、凭据、包及权限。
    target: crate::VulcanPluginActivation,
    /// Actual original route generation remains immutable throughout preparation and transfer.
    /// 实际原路由代次在准备及转交全程保持不可变。
    generation: u64,
    /// Immutable opaque UUID is allocated once by the actual McpRuntimeInner owner.
    /// 不可变不透明 UUID 由实际 McpRuntimeInner 所有者仅分配一次。
    runtime_instance_id: uuid::Uuid,
    /// Immutable opaque UUID is allocated once by the actual McpRuntimePartitionLease owner.
    /// 不可变不透明 UUID 由实际 McpRuntimePartitionLease 所有者仅分配一次。
    partition_lease_instance_id: uuid::Uuid,
    /// Original preparation catalog keeps source ownership alive only during this short native transfer guard.
    /// 原准备目录仅在此短时原生转交守卫期间保持来源所有权存活。
    _staged: Arc<crate::VulcanPluginCatalogSnapshot>,
}

impl VulcanPluginMcpTargetHandoffEvidence {
    /// Seal actual ready source evidence for transaction/generation and the two actual immutable owner UUIDs.
    /// 为 transaction／generation 及两个实际不可变所有者 UUID封闭实际就绪来源证据。
    /// Only this crate's genuine source guard constructs evidence after actual readiness and receipt validation.
    /// 仅此 crate 的真实来源守卫在实际就绪及回执校验后构造证据。
    pub(crate) fn new(
        transaction: &crate::VulcanPluginActivationTransaction,
        generation: u64,
        runtime_instance_id: uuid::Uuid,
        partition_lease_instance_id: uuid::Uuid,
        staged: &Arc<crate::VulcanPluginCatalogSnapshot>,
    ) -> Self {
        Self {
            request_id: transaction.record().request_id(),
            target: transaction.record().target().clone(),
            generation,
            runtime_instance_id,
            partition_lease_instance_id,
            _staged: Arc::clone(staged),
        }
    }
    /// Return the exact original activation UUID without granting another transport startup.
    /// 返回精确原激活 UUID，不授予另一次传输启动。
    pub fn request_id(&self) -> uuid::Uuid {
        self.request_id
    }
    /// Borrow the exact full selected target validated by the actual source holder.
    /// 借用实际来源持有者校验的精确完整已选目标。
    pub fn target(&self) -> &crate::VulcanPluginActivation {
        &self.target
    }
    /// Return the actual immutable original route generation.
    /// 返回实际不可变原路由代次。
    pub fn generation(&self) -> u64 {
        self.generation
    }
    /// Return the actual original runtime owner UUID, valid only while its genuine guard retains that instance.
    /// 返回实际原运行时所有者 UUID，仅在真实守卫保留该实例时有效。
    pub fn runtime_instance_id(&self) -> uuid::Uuid {
        self.runtime_instance_id
    }
    /// Return the actual original partition lease owner UUID, never a recreated or persistent process identity.
    /// 返回实际原分区租约所有者 UUID，绝非重建或持久进程身份。
    pub fn partition_lease_instance_id(&self) -> uuid::Uuid {
        self.partition_lease_instance_id
    }
}
