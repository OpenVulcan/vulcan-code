//! One aborted predecessor prepares immutable MCP inputs without replay or target completion authority.
//! 单个已中止前驱准备不可变 MCP 输入，不授予重放或目标完成权威。

use super::*;
use crate::{
    VulcanPluginActivation, VulcanPluginActivationPredecessorCompletion,
    VulcanPluginActivationPredecessorPublication, VulcanPluginActivationPredecessorRegistration,
    VulcanPluginActivationTransaction, VulcanPluginMcpPredecessorHandoffEvidence,
    VulcanPluginPredecessorRestorationSource,
};
use std::sync::{Mutex, MutexGuard, Weak};

/// Sealed physical inputs retain the original restoration source until actual MCP resources retire.
/// 密封物理输入保留原恢复来源，直至实际 MCP 资源退役。
#[derive(Clone)]
pub struct VulcanPluginPreparedMcpPredecessor {
    /// Clones share the same physical preparation rather than opening another recovery owner.
    /// 克隆共享同一物理准备，而非打开另一恢复所有者。
    inner: Arc<PreparedPredecessorMcpResources>,
}

/// Actual source and environment remain separate from historical control and launch profiles.
/// 实际来源与环境独立于历史控制及启动画像。
struct PreparedPredecessorMcpResources {
    /// The original source owns exact old package, configuration, credentials and independent recovery File.
    /// 原来源拥有精确旧包、配置、凭据及独立恢复文件。
    source: Arc<VulcanPluginPredecessorRestorationSource>,
    /// The sole logical control has only a weak link back to these physical resources.
    /// 唯一逻辑控制仅通过弱链接指回这些物理资源。
    control: VulcanPluginMcpPredecessorControl,
    /// Frozen launchers derive only from the source's retained old manifest.
    /// 固定启动器仅从来源保留的旧清单派生。
    stdio_launchers: BTreeMap<String, VulcanPluginMcpStdioLaunchSpec>,
    /// Actual local execution lease lasts until the last physical preparation owner retires.
    /// 实际本地执行租约存续至最后一个物理准备所有者退役。
    _environment: Option<std::fs::File>,
}

/// Original logical state preserves one dispatch fact without retaining recovery files or physical pins.
/// 原逻辑状态保留单次派发事实，不保留恢复文件或物理固定句柄。
#[derive(Clone)]
pub struct VulcanPluginMcpPredecessorControl {
    /// All control clones share this original allocation and its irreversible attempt state.
    /// 全部控制克隆共享此原分配及不可逆尝试状态。
    inner: Arc<PredecessorMcpControlValues>,
}

/// Pure original launch identity grants neither startup nor recovery on its own.
/// 纯原启动身份自身不授予启动或恢复。
#[derive(Clone)]
pub struct VulcanPluginPredecessorMcpProfile {
    /// Immutable values and a weak manager issuer have no reverse link to physical resources.
    /// 不可变值及弱管理器签发者不具有指向物理资源的反向链接。
    inner: Arc<PredecessorMcpProfileValues>,
}

/// Full immutable selection identifies old code rather than the aborted target.
/// 完整不可变选择标识旧代码，而非已中止目标。
struct PredecessorMcpProfileValues {
    /// The first explicitly registered native host remains the only issuer for this source.
    /// 首次显式注册原生宿主保持为此来源唯一签发者。
    authority: VulcanPluginRegisteredNativeMcpHostAuthority,
    /// Old selection contains value revisions only, with no package or credential reader File.
    /// 旧选择仅包含值修订，不含包或凭据读文件。
    old: VulcanPluginActivation,
    /// Manager identity is diagnostic and is checked alongside actual weak allocation identity.
    /// 管理器身份用于诊断，并与实际弱分配身份共同校验。
    manager_id: String,
    /// Original durable identity never substitutes for the independent restoration owner.
    /// 原持久身份绝不替代独立恢复所有者。
    request_id: Uuid,
    /// Actual restoration successor generation differs from the retired old execution generation.
    /// 实际恢复后继代次不同于已退役旧执行代次。
    generation: u64,
}

/// One logical state has no strong path to its source or prepared environment.
/// 单个逻辑状态不具有通往其来源或准备环境的强路径。
struct PredecessorMcpControlValues {
    /// Native launch input retention may outlive physical recovery without retaining its files.
    /// 原生启动输入保留可以超过物理恢复寿命，且不保留其文件。
    profile: VulcanPluginPredecessorMcpProfile,
    /// The original source must still exist when authorizing first dispatch.
    /// 授权首次派发时原来源必须仍然存在。
    source: Weak<VulcanPluginPredecessorRestorationSource>,
    /// The original attempt and weak preparation identity are serialized together.
    /// 原尝试及弱准备身份共同串行化。
    state: Mutex<PredecessorMcpState>,
}

/// Memory-only attempt facts never reconstruct an owner from the durable Pending marker.
/// 仅内存尝试事实绝不从持久 Pending 标记重建所有者。
struct PredecessorMcpState {
    /// Weak physical identity allows retry to observe this same live resource object only.
    /// 弱物理身份只允许重试观察此同一存活资源对象。
    prepared: Weak<PreparedPredecessorMcpResources>,
    /// A consumed attempt cannot reset after failure, cancellation or transport retirement.
    /// 已消费尝试在失败、取消或传输退役后不能重置。
    attempt: PredecessorMcpAttempt,
}

/// Explicit branches distinguish unused authority from one actually admitted dispatch.
/// 显式分支区分未使用权威与一次实际已接纳派发。
enum PredecessorMcpAttempt {
    /// No native work has yet been admitted under this original source.
    /// 尚无原生工作在此原来源下被接纳。
    Unattempted,
    /// Dispatch authority was consumed before actual allocation and transport startup.
    /// 实际分配及传输启动之前已消费派发权威。
    Started {
        /// Actual runtime and lease IDs come only from the registered native producer.
        /// 实际运行时及租约身份仅来自已注册原生生产者。
        instances: Option<(Uuid, Uuid)>,
        /// A terminal observation belongs to the same bound actual attempt.
        /// 终态观测属于同一已绑定实际尝试。
        succeeded: Option<bool>,
    },
    /// Confirmed same-owner transfer is terminal and cannot authorize another preparation or startup.
    /// 已确认同所有者转交为终态，不能授权另一准备或启动。
    Transferred,
}

/// Short actual predecessor control guard retains successful original evidence throughout joint durable transfer.
/// 短实际前驱控制守卫跨联合持久转交保留成功原证据。
pub struct VulcanPluginMcpPredecessorHandoffGuard<'gate> {
    /// Borrow this control's actual logical lock, without borrowing through a temporary upgraded Arc.
    /// 借用此控制实际逻辑锁，不通过临时升级 Arc 借用。
    state: MutexGuard<'gate, PredecessorMcpState>,
    /// Evidence seals the original actual native identities and pure first-issuer profile.
    /// 证据封闭原实际原生身份及纯首次签发者画像。
    evidence: VulcanPluginMcpPredecessorHandoffEvidence,
    /// Same physical preparation stays alive through the existing native guarded transfer.
    /// 相同物理准备跨既有原生受守卫转交保持存活。
    _prepared: Arc<PreparedPredecessorMcpResources>,
    /// Genuine manager registration excludes publication for this guard's entire lifetime.
    /// 真实管理器注册在此守卫整个寿命内排除发布。
    _registration: &'gate VulcanPluginActivationPredecessorRegistration<'gate>,
}

/// First dispatch keeps manager registration stable without holding the logical state lock during native admission.
/// 首次派发保持管理器注册稳定，原生入场期间不持有逻辑状态锁。
pub struct VulcanPluginMcpPredecessorStartGuard<'a> {
    /// The original first issuer alone can bind constructed actual native owners.
    /// 仅原首次签发者可以绑定已构造实际原生所有者。
    authority: &'a VulcanPluginRegisteredNativeMcpHostAuthority,
    /// Source and routing cannot change before work is actually registered and dispatched.
    /// 实际登记及派发工作前，来源和路由不能改变。
    _registration: MutexGuard<'a, ()>,
    /// Relock only for actual ID binding, never while entering backend admission.
    /// 仅为实际身份绑定重新加锁，绝不在进入后端入场期间持锁。
    state: &'a Mutex<PredecessorMcpState>,
    /// Actual preparation stays alive across registration and ID binding, then drops after registration unlocks.
    /// 实际准备跨注册及身份绑定保持存活，然后在注册解锁后销毁。
    _prepared: Arc<PreparedPredecessorMcpResources>,
}

impl VulcanPluginPredecessorMcpProfile {
    /// Compare authority with this pure profile's original first issuer without retaining physical owners.
    /// 将 authority 与此纯画像原首次签发者比较，不保留物理所有者。
    /// Return false for repeated registrations or foreign managers even when all copied values match.
    /// 即使全部复制值匹配，重复注册或外来管理器也返回假。
    pub(in crate::runtime) fn matches_authority(
        &self,
        authority: &VulcanPluginRegisteredNativeMcpHostAuthority,
    ) -> bool {
        authority.same_issuer(&self.inner.authority)
    }

    /// Compare other with this exact opaque source allocation; returns false for equal values in another source.
    /// 比较 other 是否为此精确不透明来源分配；另一来源的相等值返回假。
    pub fn same_source(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.inner, &other.inner)
    }
}

impl fmt::Debug for VulcanPluginPreparedMcpPredecessor {
    /// Format redacted request and route identities for formatter; returns its formatting result.
    /// 为 formatter 格式化脱敏请求及路由身份；返回格式化结果。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("VulcanPluginPreparedMcpPredecessor")
            .field("request_id", &self.request_id())
            .field("generation", &self.generation())
            .finish_non_exhaustive()
    }
}

impl VulcanPluginPreparedMcpPredecessor {
    /// Clone the same logical control; no inputs, and physical ownership is not extended by the result.
    /// 克隆同一逻辑控制；无输入，结果不延长物理所有权。
    pub fn control(&self) -> VulcanPluginMcpPredecessorControl {
        self.inner.control.clone()
    }

    /// Borrow exact old server declarations; no inputs or latest configuration lookup.
    /// 借用精确旧服务器声明；无输入，不查询最新配置。
    pub fn servers(&self) -> &[VulcanPluginMcpServerDescriptor] {
        &self.inner.source.plugin().mcp_servers
    }

    /// Borrow server_id's frozen launcher; undeclared or remote IDs return an explicit error.
    /// 借用 server_id 固定启动器；未声明或远端身份返回明确错误。
    pub fn stdio_launcher(
        &self,
        server_id: &str,
    ) -> Result<&VulcanPluginMcpStdioLaunchSpec, VulcanPluginRuntimeError> {
        self.inner.stdio_launchers.get(server_id).ok_or_else(|| {
            VulcanPluginRuntimeError::new("Predecessor has no prepared stdio launcher.")
        })
    }
}

impl std::ops::Deref for VulcanPluginPreparedMcpPredecessor {
    /// The sole logical control supplies observation methods without duplicating attempt state.
    /// 唯一逻辑控制提供观察方法，不复制尝试状态。
    type Target = VulcanPluginMcpPredecessorControl;

    /// Borrow the preparation's control; no inputs or resource transfer.
    /// 借用准备对象控制；无输入或资源转交。
    fn deref(&self) -> &Self::Target {
        &self.inner.control
    }
}

impl VulcanPluginMcpPredecessorControl {
    /// Observe whether source is this exact original weak allocation, without upgrading or granting any execution authority.
    /// 观察 source 是否为此精确原弱分配，不升级或授予任何执行权威。
    /// Return pointer identity only; no preparation, reader, File or source-state validation is performed.
    /// 仅返回指针身份；不执行准备、读者、文件或来源状态校验。
    pub fn matches_source(&self, source: &Arc<VulcanPluginPredecessorRestorationSource>) -> bool {
        Weak::ptr_eq(&self.inner.source, &Arc::downgrade(source))
    }

    /// Prepare the same successful actual control under registration without rereading Pending or taking its File lock.
    /// 在注册门下准备相同成功实际控制，不重读 Pending 或取得其文件锁。
    /// manager/transaction/source/publication identify the sealed original; authority and both IDs must match its first dispatch.
    /// manager／transaction／source／publication 标识封闭原请求；authority 及两身份必须匹配其首次派发。
    /// Return a borrowed logical guard with a strong physical pin; actual native guards must precede this call.
    /// 返回携带强物理固定句柄的借用逻辑守卫；实际原生守卫必须先于此调用。
    #[allow(clippy::too_many_arguments)]
    pub fn prepare_handoff_with_registration<'gate>(
        &'gate self,
        manager: &VulcanPluginRuntimeManager,
        transaction: &VulcanPluginActivationTransaction,
        source: &Arc<VulcanPluginPredecessorRestorationSource>,
        publication: &VulcanPluginActivationPredecessorPublication,
        authority: &VulcanPluginRegisteredNativeMcpHostAuthority,
        runtime_instance_id: Uuid,
        partition_lease_instance_id: Uuid,
        registration: &'gate VulcanPluginActivationPredecessorRegistration<'gate>,
    ) -> Result<VulcanPluginMcpPredecessorHandoffGuard<'gate>, VulcanPluginRuntimeError> {
        registration.validate_original(manager, transaction, source, publication)?;
        self.require_issuer(authority)?;
        if !authority.matches_manager(manager)
            || !Weak::ptr_eq(&self.inner.source, &Arc::downgrade(source))
            || self.inner.profile.inner.manager_id != manager.state.manager_id
            || self.request_id() != source.request_id()
            || transaction.record().predecessor() != Some(&self.inner.profile.inner.old)
            || self.generation() != source.plugin().generation
        {
            return Err(VulcanPluginRuntimeError::new(
                "Predecessor MCP transfer requires its exact original source and issuer.",
            ));
        }
        // Borrow only this control's lock after actual native guards; the reservation is locked centrally by the manager later.
        // 在实际原生守卫后仅借用此控制锁；预留权稍后由管理器集中锁定。
        let state = self
            .inner
            .state
            .lock()
            .map_err(|_| VulcanPluginRuntimeError::new("Predecessor MCP state is poisoned."))?;
        if !matches!(&state.attempt, PredecessorMcpAttempt::Started {
            instances: Some(instances), succeeded: Some(true),
        } if *instances == (runtime_instance_id, partition_lease_instance_id))
        {
            return Err(VulcanPluginRuntimeError::new(
                "Predecessor MCP transfer requires its same observed successful runtime and lease.",
            ));
        }
        // Retired physical ownership cannot be repaired from pure launch profiles or historical IDs.
        // 已退役物理所有权不能从纯启动画像或历史身份修复。
        let prepared = state.prepared.upgrade().ok_or_else(|| {
            VulcanPluginRuntimeError::new("Original predecessor MCP preparation was released.")
        })?;
        if !Arc::ptr_eq(&prepared.source, source)
            || !Arc::ptr_eq(&prepared.control.inner, &self.inner)
        {
            return Err(VulcanPluginRuntimeError::new(
                "Predecessor MCP physical preparation belongs to another control.",
            ));
        }
        // Seal before the writer while all actual native/control/physical guards remain live.
        // 写入器前封闭证据，同时全部实际原生／控制／物理守卫保持存活。
        let evidence = VulcanPluginMcpPredecessorHandoffEvidence::new(
            source,
            &self.inner.profile,
            runtime_instance_id,
            partition_lease_instance_id,
        );
        Ok(VulcanPluginMcpPredecessorHandoffGuard {
            state,
            evidence,
            _prepared: prepared,
            _registration: registration,
        })
    }

    /// Clone the same pure profile for authority; foreign first issuers are explicitly rejected.
    /// 为 authority 克隆同一纯画像；明确拒绝外来首次签发者。
    /// The result retains no source, environment or recovery File and grants no dispatch.
    /// 结果不保留来源、环境或恢复文件，且不授予派发。
    pub fn host_profile(
        &self,
        authority: &VulcanPluginRegisteredNativeMcpHostAuthority,
    ) -> Result<VulcanPluginPredecessorMcpProfile, VulcanPluginRuntimeError> {
        self.require_issuer(authority)?;
        Ok(self.inner.profile.clone())
    }

    /// Borrow the exact old plugin identity; no inputs or resource transfer.
    /// 借用精确旧插件身份；无输入或资源转交。
    pub fn plugin_id(&self) -> &str {
        &self.inner.profile.inner.old.plugin_id
    }

    /// Copy the original aborted request identity; no inputs or recovery-owner acquisition.
    /// 复制原中止请求身份；无输入，不获取恢复所有者。
    pub fn request_id(&self) -> Uuid {
        self.inner.profile.inner.request_id
    }

    /// Return the actual restoration successor generation; no inputs or route mutation.
    /// 返回实际恢复后继代次；无输入，不变更路由。
    pub fn generation(&self) -> u64 {
        self.inner.profile.inner.generation
    }

    /// Validate manager, full aborted transaction and authority before consuming the one original dispatch.
    /// 消费唯一原派发前校验 manager、完整中止 transaction 及 authority。
    /// Returns a short registration guard; it must end before network or readiness waiting.
    /// 返回短时注册守卫；它必须在网络或就绪等待前结束。
    pub fn begin_transport_attempt<'a>(
        &'a self,
        manager: &'a VulcanPluginRuntimeManager,
        transaction: &VulcanPluginActivationTransaction,
        authority: &VulcanPluginRegisteredNativeMcpHostAuthority,
    ) -> Result<VulcanPluginMcpPredecessorStartGuard<'a>, VulcanPluginRuntimeError> {
        self.require_issuer(authority)?;
        if !authority.matches_manager(manager)
            || self.inner.profile.inner.manager_id != manager.state.manager_id
        {
            return Err(VulcanPluginRuntimeError::new(
                "Predecessor MCP startup requires its original manager.",
            ));
        }
        // Registration precedes logical source and native admission in both startup and handoff.
        // 启动及交接中，注册门都先于逻辑来源与原生入场。
        let registration = manager
            .state
            .session_registration_gate
            .lock()
            .map_err(|_| {
                VulcanPluginRuntimeError::new("Predecessor MCP registration gate is poisoned.")
            })?;
        // Upgrade actual ownership rather than reconstructing it from equal request or selection values.
        // 升级实际所有权，而非从相等请求或选择值重建它。
        let source = self.inner.source.upgrade().ok_or_else(|| {
            VulcanPluginRuntimeError::new("Original predecessor MCP source was released.")
        })?;
        if transaction.record() != source.original()
            || transaction.record().predecessor() != Some(&self.inner.profile.inner.old)
            || source.plugin().generation != self.generation()
        {
            return Err(VulcanPluginRuntimeError::new(
                "Predecessor MCP transaction differs from its original aborted source.",
            ));
        }
        source.validate(&manager.state)?;
        // The same physical preparation must still exist before any external work is admitted.
        // 接纳任何外部工作之前，同一物理准备必须仍然存在。
        let mut state = self
            .inner
            .state
            .lock()
            .map_err(|_| VulcanPluginRuntimeError::new("Predecessor MCP state is poisoned."))?;
        // The returned short guard retains this actual owner until native work takes its own resource pin.
        // 返回的短时守卫保留此实际所有者，直到原生工作取得自己的资源固定句柄。
        let prepared = state.prepared.upgrade().ok_or_else(|| {
            VulcanPluginRuntimeError::new("Original predecessor MCP preparation was released.")
        })?;
        if !matches!(state.attempt, PredecessorMcpAttempt::Unattempted) {
            return Err(VulcanPluginRuntimeError::new(
                "Original predecessor MCP startup was already attempted.",
            ));
        }
        // Pending already fences restart recovery; this memory fact never mints a second durable ledger.
        // Pending 已隔离重启恢复；此内存事实绝不签发第二持久账本。
        state.attempt = PredecessorMcpAttempt::Started {
            instances: None,
            succeeded: None,
        };
        drop(state);
        Ok(VulcanPluginMcpPredecessorStartGuard {
            authority: &self.inner.profile.inner.authority,
            _registration: registration,
            state: &self.inner.state,
            _prepared: prepared,
        })
    }

    /// Record succeeded for the sole bound actual attempt under authority; differing terminal results are rejected.
    /// 在 authority 下记录唯一已绑定实际尝试的 succeeded；拒绝不同终态结果。
    /// Reconfirming the same result is read-only and never authorizes another dispatch.
    /// 再确认同一结果只读进行，绝不授权另一派发。
    pub fn finish_transport_observation(
        &self,
        authority: &VulcanPluginRegisteredNativeMcpHostAuthority,
        succeeded: bool,
    ) -> Result<(), VulcanPluginRuntimeError> {
        self.require_issuer(authority)?;
        // Observation serializes only the original logical state, not backend admission or business locks.
        // 观测仅串行化原逻辑状态，不串行化后端入场或业务锁。
        let mut state = self
            .inner
            .state
            .lock()
            .map_err(|_| VulcanPluginRuntimeError::new("Predecessor MCP state is poisoned."))?;
        match &mut state.attempt {
            PredecessorMcpAttempt::Started {
                instances: Some(_),
                succeeded: observed,
            } => match observed {
                Some(previous) if *previous == succeeded => Ok(()),
                Some(_) => Err(VulcanPluginRuntimeError::new(
                    "Predecessor MCP terminal observation conflicts with the original result.",
                )),
                None => {
                    *observed = Some(succeeded);
                    Ok(())
                }
            },
            _ => Err(VulcanPluginRuntimeError::new(
                "Predecessor MCP observation requires its bound actual attempt.",
            )),
        }
    }

    /// Validate authority against this original first issuer; returns no additional permission or physical ownership.
    /// 按此原首次签发者校验 authority；不返回额外权限或物理所有权。
    fn require_issuer(
        &self,
        authority: &VulcanPluginRegisteredNativeMcpHostAuthority,
    ) -> Result<(), VulcanPluginRuntimeError> {
        if !authority.same_issuer(&self.inner.profile.inner.authority) {
            return Err(VulcanPluginRuntimeError::new(
                "Predecessor MCP requires its original registered native host.",
            ));
        }
        Ok(())
    }
}

impl VulcanPluginMcpPredecessorHandoffGuard<'_> {
    /// Borrow exact sealed evidence while the original control, physical pin and registration remain held.
    /// 原控制、物理固定及注册门仍持有时，借用精确封闭证据。
    /// No inputs; evidence alone never authorizes native admission or another initialization.
    /// 无输入；仅凭证据绝不授权原生入场或另一初始化。
    pub fn evidence(&self) -> &VulcanPluginMcpPredecessorHandoffEvidence {
        &self.evidence
    }

    /// Mark this same original control transferred only for exact confirmed evidence; no I/O or fallible operations occur.
    /// 仅精确已确认证据可将此相同原控制标记为已转交；不执行 I/O 或可失败操作。
    /// Return true only after same-evidence transfer; foreign confirmation returns false without admission or mutation.
    /// 仅同证据转交后返回真；外来 confirmation 返回假，不授予入场或变更。
    pub fn confirm_transferred(
        &mut self,
        confirmation: &VulcanPluginActivationPredecessorCompletion,
    ) -> bool {
        if confirmation.matches_mcp_evidence(&self.evidence) {
            self.state.attempt = PredecessorMcpAttempt::Transferred;
            true
        } else {
            false
        }
    }
}

impl VulcanPluginMcpPredecessorStartGuard<'_> {
    /// Bind the producer's actual runtime_instance_id and partition_lease_instance_id under original authority.
    /// 在原 authority 下绑定生产者实际 runtime_instance_id 及 partition_lease_instance_id。
    /// Nonzero IDs must identify the constructed owners retained by this same registered native host.
    /// 非零身份必须标识此同一已注册原生宿主持有的已构造所有者。
    /// Rebinding equal IDs only confirms them; replacements and post-observation binding return errors.
    /// 再绑定相等身份仅确认它们；替代及观测后绑定返回错误。
    pub fn bind_actual_instances(
        &mut self,
        runtime_instance_id: Uuid,
        partition_lease_instance_id: Uuid,
        authority: &VulcanPluginRegisteredNativeMcpHostAuthority,
    ) -> Result<(), VulcanPluginRuntimeError> {
        if !authority.same_issuer(self.authority)
            || runtime_instance_id.is_nil()
            || partition_lease_instance_id.is_nil()
        {
            return Err(VulcanPluginRuntimeError::new(
                "Predecessor MCP binding requires its original host and nonzero actual owner IDs.",
            ));
        }
        // Actual binding does not enter native locks or open a business gate.
        // 实际绑定不进入原生锁或打开业务门。
        let mut state = self
            .state
            .lock()
            .map_err(|_| VulcanPluginRuntimeError::new("Predecessor MCP state is poisoned."))?;
        match &mut state.attempt {
            PredecessorMcpAttempt::Started {
                instances,
                succeeded: None,
            } => match instances {
                Some(previous)
                    if *previous == (runtime_instance_id, partition_lease_instance_id) =>
                {
                    Ok(())
                }
                Some(_) => Err(VulcanPluginRuntimeError::new(
                    "Predecessor MCP actual owner IDs cannot be replaced.",
                )),
                None => {
                    *instances = Some((runtime_instance_id, partition_lease_instance_id));
                    Ok(())
                }
            },
            _ => Err(VulcanPluginRuntimeError::new(
                "Predecessor MCP binding requires its original unfinished attempt.",
            )),
        }
    }
}

impl VulcanPluginRuntimeManager {
    /// Prepare exact source inputs for authority without opening business or granting Target completion.
    /// 为 authority 准备精确 source 输入，不打开业务或授予目标完成权威。
    /// Retries return only the same live physical object; released preparations are never rebuilt.
    /// 重试仅返回同一存活物理对象；绝不重建已释放准备。
    pub fn prepare_predecessor_mcp_resources(
        &self,
        source: &Arc<VulcanPluginPredecessorRestorationSource>,
        authority: &VulcanPluginRegisteredNativeMcpHostAuthority,
    ) -> Result<VulcanPluginPreparedMcpPredecessor, VulcanPluginRuntimeError> {
        if !authority.matches_manager(self) {
            return Err(VulcanPluginRuntimeError::new(
                "Predecessor MCP preparation requires a host registered by its original manager.",
            ));
        }
        // Serialize this source's factory before briefly entering global registration.
        // 串行化此来源工厂，然后短暂进入全局注册。
        // Dispatch and handoff never acquire this factory-only lock under registration.
        // 派发及交接绝不在注册门内获取此工厂专属锁。
        let _preparation = source.mcp_preparation.lock().map_err(|_| {
            VulcanPluginRuntimeError::new("Predecessor MCP preparation gate is poisoned.")
        })?;
        {
            // Exact closed route and durable Pending precede any filesystem preparation.
            // 精确关闭路由及持久 Pending 校验先于任何文件系统准备。
            let _registration = self.state.session_registration_gate.lock().map_err(|_| {
                VulcanPluginRuntimeError::new("Predecessor MCP registration gate is poisoned.")
            })?;
            source.validate(&self.state)?;
            // This short slot lock never crosses filesystem or dependency installation work.
            // 此短时槽锁绝不跨文件系统或依赖安装工作存活。
            let slot = source.mcp.lock().map_err(|_| {
                VulcanPluginRuntimeError::new("Predecessor MCP source slot is poisoned.")
            })?;
            if let Some(control) = slot.as_ref() {
                control.require_issuer(authority)?;
                return control.inner.state.lock().map_err(|_| {
                    VulcanPluginRuntimeError::new("Predecessor MCP state is poisoned.")
                })?.prepared.upgrade().map(|inner| VulcanPluginPreparedMcpPredecessor { inner })
                    .ok_or_else(|| VulcanPluginRuntimeError::new(
                        "Original predecessor MCP preparation was released and cannot be rebuilt.",
                    ));
            }
        }
        // All old revisions are already pinned by the validated actual source, never latest discovery.
        // 全部旧修订已由已校验实际来源固定，绝不取自最新发现。
        let plugin = source.plugin();
        // Local launchers need the existing shared execution-copy lease before filesystem preparation.
        // 本地启动器在文件系统准备前需要既有共享执行副本租约。
        let environment = if plugin.mcp_servers.iter().any(|server| {
            matches!(
                server.manifest.transport,
                VulcanPluginMcpTransportManifest::Stdio { .. }
            )
        }) {
            Some(
                crate::runtime::environment::retain_plugin_execution_environment(
                    &self.state,
                    &plugin.id,
                )
                .map_err(VulcanPluginRuntimeError::new)?,
            )
        } else {
            None
        };
        // The manifest is the actual retained old declaration, not the aborted target's declaration.
        // 清单为实际保留旧声明，而非已中止目标声明。
        let manifest = plugin
            .manifest
            .as_ref()
            .ok_or_else(|| VulcanPluginRuntimeError::new("Predecessor MCP manifest is missing."))?;
        // One map fixes all stdio inputs before any native task can start.
        // 单个映射在任何原生任务可以启动前固定全部标准输入输出输入。
        let mut stdio_launchers = BTreeMap::new();
        for server in &plugin.mcp_servers {
            if let VulcanPluginMcpTransportManifest::Stdio { launcher, .. } =
                &server.manifest.transport
            {
                // Reuse actual launcher production without Target receipts or Hook initialization grants.
                // 复用实际启动器生产，不使用目标回执或 Hook 初始化授权。
                let resolved = crate::runtime::process::resolve_plugin_mcp_stdio_launcher(
                    &self.state,
                    plugin,
                    manifest,
                    launcher,
                )
                .map_err(VulcanPluginRuntimeError::new)?;
                stdio_launchers.insert(server.id.clone(), resolved);
            }
        }
        // Reenter only after potentially blocking installation and revalidate the actual source.
        // 可能阻塞的安装结束后才重新进入，并重新校验实际来源。
        let _registration = self.state.session_registration_gate.lock().map_err(|_| {
            VulcanPluginRuntimeError::new("Predecessor MCP registration gate is poisoned.")
        })?;
        source.validate(&self.state)?;
        // The factory-only gate kept this slot untouched while global registration remained available.
        // 工厂专属门让此槽保持未修改，同时全局注册始终可用。
        let mut slot = source.mcp.lock().map_err(|_| {
            VulcanPluginRuntimeError::new("Predecessor MCP source slot is poisoned.")
        })?;
        if slot.is_some() {
            return Err(VulcanPluginRuntimeError::new(
                "Original predecessor MCP preparation slot changed unexpectedly.",
            ));
        }
        // Profile values contain no File or reverse physical resource link.
        // 画像值不含文件或反向物理资源链接。
        let profile = VulcanPluginPredecessorMcpProfile {
            inner: Arc::new(PredecessorMcpProfileValues {
                authority: authority.clone(),
                old: source
                    .original()
                    .predecessor()
                    .expect("validated managed old selection")
                    .clone(),
                manager_id: self.state.manager_id.clone(),
                request_id: source.request_id(),
                generation: plugin.generation,
            }),
        };
        // Logical state owns no actual resource; its weak preparation is fixed once below.
        // 逻辑状态不拥有实际资源；其弱准备在下方仅固定一次。
        let control = VulcanPluginMcpPredecessorControl {
            inner: Arc::new(PredecessorMcpControlValues {
                profile,
                source: Arc::downgrade(source),
                state: Mutex::new(PredecessorMcpState {
                    prepared: Weak::new(),
                    attempt: PredecessorMcpAttempt::Unattempted,
                }),
            }),
        };
        // Physical retention precedes signing the same source's only preparation slot.
        // 物理保留先于签发相同来源唯一准备槽。
        let inner = Arc::new(PreparedPredecessorMcpResources {
            source: Arc::clone(source),
            control: control.clone(),
            stdio_launchers,
            _environment: environment,
        });
        control
            .inner
            .state
            .lock()
            .map_err(|_| VulcanPluginRuntimeError::new("Predecessor MCP state is poisoned."))?
            .prepared = Arc::downgrade(&inner);
        *slot = Some(control);
        Ok(VulcanPluginPreparedMcpPredecessor { inner })
    }
}
