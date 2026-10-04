//! Exact selected MCP targets retain actual resources independently of the short original request guard.
//! 精确已选 MCP 目标保留实际资源，独立于短时原请求守卫。

use super::*;
use crate::generation_store::{TargetMcpInitializationAttempt, TargetMcpInitializationOwner};
use crate::runtime::VulcanPluginManagerState;
use crate::{
    VulcanPluginActivation, VulcanPluginActivationTargetCompletion,
    VulcanPluginActivationTargetRegistration, VulcanPluginActivationTransaction,
    VulcanPluginCatalogSnapshot, VulcanPluginGenerationStore, VulcanPluginMcpTargetHandoffEvidence,
    VulcanPluginRuntimeState,
};
use std::sync::{Mutex, MutexGuard, Weak};

/// Explicit native Rust host authorization is separate from ordinary plugin or Lua execution.
/// 显式原生 Rust 宿主授权独立于普通插件或 Lua 执行。
/// Clones refer to one private issuer; registered hosts are trusted to attest their actual external resource facts.
/// 克隆指向单个私有签发者；已注册宿主受信任地证明其实际外部资源事实。
#[derive(Clone)]
pub struct VulcanPluginRegisteredNativeMcpHostAuthority {
    /// One authorization issuer carries no process, lease, counter or runtime registry.
    /// 单个授权签发者不携带进程、租约、计数器或运行时注册表。
    issuer: Arc<NativeMcpHostIssuer>,
}

/// Private issuer is bound to exactly the manager state that explicitly registered the host.
/// 私有签发者绑定至显式注册宿主的精确管理器状态。
struct NativeMcpHostIssuer {
    /// Weak state identity authorizes this manager without retaining unrelated manager resources.
    /// 弱状态身份授权此管理器，不保留无关管理器资源。
    manager: Weak<VulcanPluginManagerState>,
}

impl VulcanPluginRegisteredNativeMcpHostAuthority {
    /// Compare other with this exact private issuer, without exporting manager or resource authority.
    /// 比较 other 是否为此精确私有签发者，不导出管理器或资源权威。
    /// Return true only for clones of the same registration; repeated registration remains distinct.
    /// 仅同次注册的克隆返回真；重复注册仍为不同签发者。
    pub fn same_issuer(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.issuer, &other.issuer)
    }

    /// Compare the actual manager state; return false for another or retired manager without new authorization.
    /// 比较实际管理器状态；其他或已退役管理器返回假值，不产生新授权。
    pub fn matches_manager(&self, manager: &VulcanPluginRuntimeManager) -> bool {
        self.issuer
            .manager
            .upgrade()
            .is_some_and(|state| Arc::ptr_eq(&state, &manager.state))
    }
}

/// Sealed selected target owns immutable inputs until every actual transport resource retires.
/// 密封已选目标拥有不可变输入，直至全部实际传输资源退役。
#[derive(Clone)]
pub struct VulcanPluginPreparedMcpTarget {
    /// Clones share actual package, configuration, credentials, environment and original request retention.
    /// 克隆共享实际包、配置、凭据、环境及原请求保留。
    inner: Arc<PreparedTargetMcpResources>,
}

/// One original target resource owner never reselects inputs from public discovery.
/// 单个原目标资源所有者绝不从公开发现重选输入。
struct PreparedTargetMcpResources {
    /// The same original control coordinates completion without owning these physical resources.
    /// 相同原控制协调完成，不拥有这些物理资源。
    control: VulcanPluginMcpTargetControl,
    /// Exact actual descriptor retains package and credential pins.
    /// 精确实际描述符保留插件包及凭据固定句柄。
    plugin: VulcanPluginDescriptor,
    /// Exact resolved launchers belong to this selected target.
    /// 精确已解析启动器属于此已选目标。
    stdio_launchers: BTreeMap<String, VulcanPluginMcpStdioLaunchSpec>,
    /// Actual OS shared environment lock outlives handoff until transport retirement.
    /// 实际操作系统共享环境锁跨交接存续至传输退役。
    _environment: Option<std::fs::File>,
}

/// Original completion control retains request authority until confirmation, then only historical identity.
/// 原完成控制在确认前保留请求权威，确认后仅保留历史身份。
/// Cloning preserves the unfinished request and closed catalog, but never the physical preparation object or environment File.
/// 克隆保留未完成请求及关闭目录，但绝不保留物理准备对象或环境 File。
#[derive(Clone)]
pub struct VulcanPluginMcpTargetControl {
    /// Shared original state has no link back to physical preparation resources.
    /// 共享原状态没有返回物理准备资源的链接。
    inner: Arc<TargetMcpControlValues>,
}

/// Pure target source has no request File, physical pin, execution owner or reverse link to completion control.
/// 纯目标来源不具有请求 File、物理固定、执行所有者或指向完成控制的反向链接。
/// Native producers must bind it before dispatch and independently prove the same actual input and transport.
/// 原生生产者必须在派发前绑定它，并独立证明同一实际输入及传输。
#[derive(Clone)]
pub struct VulcanPluginTargetMcpProfile {
    /// Shared original source identity contains only immutable values and the weak manager issuer.
    /// 共享原来源身份仅包含不可变值及弱管理器签发者。
    inner: Arc<TargetMcpProfileValues>,
}

/// Frozen source values are neither serialized nor formatted, and grant no startup or restoration on their own.
/// 固定来源值不序列化或格式化，自身不授予启动或恢复。
struct TargetMcpProfileValues {
    /// Original source fixes the explicitly registered native host issuer, not arbitrary raw proof values.
    /// 原来源固定显式注册原生宿主签发者，而非任意裸证明值。
    issuer: Arc<NativeMcpHostIssuer>,
    /// Full original selection includes configuration, permission and credential revisions.
    /// 完整原选择包含配置、权限及凭据修订。
    target: VulcanPluginActivation,
    /// Original non-secret plugin identity does not retain a full descriptor.
    /// 原非秘密插件身份不保留完整描述符。
    plugin_id: String,
    /// Original route generation is diagnostic and never replaces source evidence.
    /// 原路由代次用于诊断，绝不替代来源证据。
    generation: u64,
    /// Original durable request identity is diagnostic and does not replace its file lock.
    /// 原持久请求身份用于诊断，不替代其文件锁。
    request_id: Uuid,
    /// Only the creating manager may validate this target authority.
    /// 仅创建管理器可校验此目标权威。
    manager_id: String,
}

impl VulcanPluginTargetMcpProfile {
    /// Compare other with this original opaque allocation; equal request or configuration values are insufficient.
    /// 比较 other 是否为此原不透明分配；相等请求或配置值不足以成立。
    /// Return true only for clones of this same source without exposing its protected values.
    /// 仅此同一来源的克隆返回真，不暴露其受保护值。
    pub fn same_source(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.inner, &other.inner)
    }
}

/// Logical completion control holds pure identity separately from unfinished strong request authority.
/// 逻辑完成控制将纯身份与未完成强请求权威分开持有。
struct TargetMcpControlValues {
    /// This same source is fixed once before native dispatch and never points back to the control.
    /// 此同一来源在原生派发前仅固定一次，绝不指回控制。
    profile: VulcanPluginTargetMcpProfile,
    /// Same original lock Arc can be released at confirmed handoff while transport resources remain owned.
    /// 确认交接时可释放同一原锁 Arc，传输资源仍保持自有。
    source: Mutex<TargetMcpSourceState>,
}

/// The sole source state holds original request authority and persistent actual-attempt evidence together.
/// 唯一来源状态共同持有原请求权威及持久实际尝试证据。
struct TargetMcpSourceState {
    /// Original control retains the exact closed catalog until confirmed transfer or its last source owner drops.
    /// 原控制保留精确关闭目录，直至确认转交或最后一个来源所有者销毁。
    staged: Option<Arc<VulcanPluginCatalogSnapshot>>,
    /// Same actual original request retention can end at confirmed handoff.
    /// 相同实际原请求保留可在确认交接时结束。
    request: Option<Arc<std::fs::File>>,
    /// This same-request owner is consumed only by transport attempt preparation.
    /// 此同请求所有者仅由传输尝试准备消费。
    initialization: Option<TargetMcpInitializationOwner>,
    /// Sole persistent writer records Started, actual owner UUIDs and terminal readiness observation.
    /// 唯一持久写入者记录 Started、实际所有者 UUID 及终态就绪观测。
    attempt: Option<TargetMcpInitializationAttempt>,
}

/// Short transfer guard retains the exact original request after all fallible owner checks.
/// 短时转交守卫在全部可失败所有者检查后保留精确原请求。
pub struct VulcanPluginMcpTargetHandoffGuard<'a> {
    /// Borrowed actual registration scope cannot escape its manager's completion operation.
    /// 借用的实际注册作用域不能逃逸其管理器的完成操作。
    _registration: &'a VulcanPluginActivationTargetRegistration<'a>,
    /// Releasing this field at confirmed completion preserves every long-lived resource pin.
    /// 确认完成时释放此字段，保留全部长生命周期资源固定句柄。
    source: MutexGuard<'a, TargetMcpSourceState>,
    /// Sealed evidence is borrowed only while this same original source remains held.
    /// 仅此相同原来源保持持有时借用封闭证据。
    evidence: VulcanPluginMcpTargetHandoffEvidence,
}

/// First dispatch holds the existing registration fence only until actual work is registered and spawned.
/// 首次派发仅在实际工作登记及启动前持有既有注册屏障。
pub struct VulcanPluginMcpTargetStartGuard<'a> {
    /// Binding actual instances requires the same explicitly registered native host issuer.
    /// 绑定实际实例要求相同显式注册原生宿主签发者。
    issuer: &'a Arc<NativeMcpHostIssuer>,
    /// Prevent source staging from changing between exact current-Arc validation and actual dispatch.
    /// 防止源暂存在精确当前 Arc 校验与实际派发之间改变。
    _registration: MutexGuard<'a, ()>,
    /// Same source is relocked only for receipt writes, never while waiting for backend admission.
    /// 相同来源仅在写回执时重新加锁，绝不在等待后端入场期间持锁。
    source: &'a Mutex<TargetMcpSourceState>,
}

impl fmt::Debug for VulcanPluginPreparedMcpTarget {
    /// Write only stable identities; formatter returns the redacted formatting result.
    /// 仅写入稳定身份；formatter 返回脱敏格式化结果。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("VulcanPluginPreparedMcpTarget")
            .field("plugin_id", &self.plugin_id())
            .field("request_id", &self.request_id())
            .field("generation", &self.generation())
            .finish_non_exhaustive()
    }
}

impl VulcanPluginPreparedMcpTarget {
    /// Clone this exact original control without extending physical preparation ownership.
    /// 克隆此精确原控制，不延长物理准备所有权。
    /// No inputs; the returned control retains the original request and closed stage only until confirmed completion.
    /// 无输入；返回控制仅在确认完成前保留原请求及关闭暂存。
    pub fn control(&self) -> VulcanPluginMcpTargetControl {
        self.inner.control.clone()
    }

    /// Borrow original server declarations; no input and no latest-selection lookup.
    /// 借用原服务器声明；无输入，不查询最新选择。
    pub fn servers(&self) -> &[VulcanPluginMcpServerDescriptor] {
        &self.inner.plugin.mcp_servers
    }

    /// Borrow the resolved launcher for server_id; return an explicit error for undeclared stdio IDs.
    /// 借用 server_id 已解析启动器；未声明的标准输入输出标识返回明确错误。
    pub fn stdio_launcher(
        &self,
        server_id: &str,
    ) -> Result<&VulcanPluginMcpStdioLaunchSpec, VulcanPluginRuntimeError> {
        self.inner.stdio_launchers.get(server_id).ok_or_else(|| {
            VulcanPluginRuntimeError::new("Selected target has no prepared stdio launcher.")
        })
    }
}

impl std::ops::Deref for VulcanPluginPreparedMcpTarget {
    /// Preserve original control operations without a second state or reverse physical ownership link.
    /// 保留原控制操作，不增加第二份状态或反向物理所有权链接。
    type Target = VulcanPluginMcpTargetControl;

    /// Borrow this preparation's sole original control; no inputs or resource transfer.
    /// 借用此准备的唯一原控制；无输入或资源转交。
    fn deref(&self) -> &Self::Target {
        &self.inner.control
    }
}

impl VulcanPluginMcpTargetControl {
    /// Clone the original pure source for authority without sharing unfinished request or physical ownership.
    /// 为 authority 克隆原纯来源，不共享未完成请求或物理所有权。
    /// Return the same profile only for its actual registered issuer; foreign or retired issuers are rejected.
    /// 仅其实际注册签发者返回同一画像；拒绝外来或已退役签发者。
    pub fn host_profile(
        &self,
        authority: &VulcanPluginRegisteredNativeMcpHostAuthority,
    ) -> Result<VulcanPluginTargetMcpProfile, VulcanPluginRuntimeError> {
        if !Arc::ptr_eq(&authority.issuer, &self.inner.profile.inner.issuer)
            || authority.issuer.manager.upgrade().is_none()
        {
            return Err(VulcanPluginRuntimeError::new(
                "Target MCP source requires its original live registered host.",
            ));
        }
        Ok(self.inner.profile.clone())
    }

    /// Validate first dispatch against the original transaction and exact current staged Arc under registration.
    /// 在注册门内按原事务及精确当前暂存 Arc 校验首次派发。
    /// manager and transaction are original owners; returned guard must end before network/readiness waiting.
    /// manager 及 transaction 为原所有者；返回守卫必须在网络／就绪等待前结束。
    /// authority is the original explicitly registered native host issuer, shared by authorization clones.
    /// authority 为原显式注册原生宿主签发者，由授权克隆共享。
    pub fn begin_transport_attempt<'a>(
        &'a self,
        manager: &'a VulcanPluginRuntimeManager,
        transaction: &VulcanPluginActivationTransaction,
        authority: &VulcanPluginRegisteredNativeMcpHostAuthority,
    ) -> Result<VulcanPluginMcpTargetStartGuard<'a>, VulcanPluginRuntimeError> {
        if !authority.matches_manager(manager)
            || !Arc::ptr_eq(&authority.issuer, &self.inner.profile.inner.issuer)
        {
            return Err(VulcanPluginRuntimeError::new(
                "Target MCP startup requires its original registered native host.",
            ));
        }
        // Registration precedes source and MCP admission locks, matching final completion ordering.
        // 注册门先于来源及 MCP 入场锁，与最终完成排序一致。
        let registration = manager
            .state
            .session_registration_gate
            .lock()
            .map_err(|_| {
                VulcanPluginRuntimeError::new("Target MCP registration gate is poisoned.")
            })?;
        // Current Arc must still be the actual closed stage that issued this resource owner.
        // 当前 Arc 必须仍为签发此资源所有者的实际关闭暂存。
        let staged = self
            .inner
            .source
            .lock()
            .map_err(|_| VulcanPluginRuntimeError::new("Target MCP source is poisoned."))?
            .staged
            .clone()
            .ok_or_else(|| {
                VulcanPluginRuntimeError::new("Original MCP target stage was released.")
            })?;
        let current = manager
            .state
            .snapshot
            .read()
            .map_err(|_| VulcanPluginRuntimeError::new("Target MCP catalog is poisoned."))?;
        if !Arc::ptr_eq(&current, &staged)
            || manager.state.manager_id != self.inner.profile.inner.manager_id
            || transaction.record().request_id() != self.request_id()
            || transaction.record().target() != &self.inner.profile.inner.target
        {
            return Err(VulcanPluginRuntimeError::new(
                "Original MCP target stage changed before dispatch.",
            ));
        }
        drop(current);
        // Durable Started precedes actual work; source clones cannot authorize a second attempt.
        // 持久 Started 先于实际工作；来源克隆不能授权第二次尝试。
        let mut source = self
            .inner
            .source
            .lock()
            .map_err(|_| VulcanPluginRuntimeError::new("Target MCP source is poisoned."))?;
        if source.attempt.is_some() || source.request.is_none() {
            return Err(VulcanPluginRuntimeError::new(
                "Original target MCP startup was already attempted or transferred.",
            ));
        }
        // A missing owner means the original source was already consumed, never a fallback startup grant.
        // 缺失所有者表示原来源已消费，绝不是回退启动授权。
        let initialization = source.initialization.as_ref().ok_or_else(|| {
            VulcanPluginRuntimeError::new("Original target MCP initialization owner is missing.")
        })?;
        // Persist before any task or transport dispatch; errors keep the original source and prevent startup.
        // 在任何任务或传输派发前持久保存；错误保留原来源并阻止启动。
        let attempt = initialization
            .begin()
            .map_err(|error| VulcanPluginRuntimeError::new(error.to_string()))?;
        source.attempt = Some(attempt);
        source.initialization.take();
        // Dispatch acquires backend admission after this source lock ends, eliminating inverse handoff waits.
        // 此来源锁结束后派发才取得后端入场，消除与交接的反向等待。
        drop(source);
        Ok(VulcanPluginMcpTargetStartGuard {
            issuer: &self.inner.profile.inner.issuer,
            _registration: registration,
            source: &self.inner.source,
        })
    }
    /// Borrow the exact plugin identity; no input and no resource transfer.
    /// 借用精确插件身份；无输入，不转交资源。
    pub fn plugin_id(&self) -> &str {
        &self.inner.profile.inner.plugin_id
    }
    /// Copy the original request identity; no input and no new lease acquisition.
    /// 复制原请求身份；无输入，不获取新租约。
    pub fn request_id(&self) -> Uuid {
        self.inner.profile.inner.request_id
    }
    /// Return the actual nonzero route generation; no input and no route mutation.
    /// 返回实际非零路由代次；无输入，不变更路由。
    pub fn generation(&self) -> u64 {
        self.inner.profile.inner.generation
    }
    /// Validate original transaction, full selected manifest and exact current final Arc before retaining its transfer guard.
    /// 保留转交守卫前校验原事务、完整已选清单及精确当前最终 Arc。
    /// manager and transaction supply original authority; final_catalog must be the current published exact target.
    /// manager 及 transaction 提供原权威；final_catalog 必须为当前已发布精确目标。
    /// authority must match the original registered issuer; registration binds the returned guard to actual manager exclusion.
    /// authority 必须匹配原注册签发者；registration 将返回守卫绑定至实际管理器互斥。
    /// Returns a short request guard; no business gate is opened and no network I/O occurs.
    /// 返回短时请求守卫；不打开业务门，不执行网络输入输出。
    /// # Registered native host contract
    /// 已注册原生宿主契约
    /// The native host must verify both UUIDs belong to this original actual runtime and lease, and hold its admission,
    /// 原生宿主必须验证两个 UUID 属于此原实际运行时及租约，并持有其入场、
    /// business, actual transport and Ready snapshot guards throughout this returned source guard's lifetime.
    /// 业务、实际传输及 Ready 快照守卫，覆盖此返回来源守卫的完整生命周期。
    /// The complete original selection and route must refer to the same guarded owners; core cannot inspect external resources.
    /// 完整原选择及路由必须指向相同受保护所有者；core 无法检查外部资源。
    /// The same manager's existing registration gate must already be retained through source validation and confirmation.
    /// 相同管理器的既有注册门必须在来源校验及确认全程已被保留。
    /// ```compile_fail
    /// use vulcan_plugin_runtime::{VulcanPluginPreparedMcpTarget, VulcanPluginRuntimeManager, VulcanPluginActivationTransaction, VulcanPluginCatalogSnapshot};
    /// fn forge(target: &VulcanPluginPreparedMcpTarget, manager: &VulcanPluginRuntimeManager, transaction: &VulcanPluginActivationTransaction, catalog: &std::sync::Arc<VulcanPluginCatalogSnapshot>, runtime: uuid::Uuid, lease: uuid::Uuid) {
    ///     target.prepare_handoff(manager, transaction, catalog, runtime, lease).unwrap();
    /// }
    /// ```
    // All existing inputs bind one atomic handoff authorization, including the original registration guard's lifetime.
    // 全部既有输入绑定单个原子转交授权，包括原注册守卫的生命周期。
    #[expect(
        clippy::too_many_arguments,
        reason = "The public handoff validates the original manager, transaction, catalog, two instance identities, issuer and registration guard together."
    )]
    pub fn prepare_handoff<'a>(
        &'a self,
        manager: &VulcanPluginRuntimeManager,
        transaction: &VulcanPluginActivationTransaction,
        final_catalog: &Arc<VulcanPluginCatalogSnapshot>,
        runtime_instance_id: Uuid,
        partition_lease_instance_id: Uuid,
        authority: &VulcanPluginRegisteredNativeMcpHostAuthority,
        registration: &'a VulcanPluginActivationTargetRegistration<'a>,
    ) -> Result<VulcanPluginMcpTargetHandoffGuard<'a>, VulcanPluginRuntimeError> {
        if !authority.matches_manager(manager)
            || !Arc::ptr_eq(&authority.issuer, &self.inner.profile.inner.issuer)
            || !registration.matches_manager(manager)
        {
            return Err(VulcanPluginRuntimeError::new(
                "Target MCP handoff requires its original registered native host and registration scope.",
            ));
        }
        // Full disk validation uses the original store authority rather than a copied selection token.
        // 完整磁盘校验使用原存储权威，而非复制的选择令牌。
        let store = VulcanPluginGenerationStore::open_existing(&manager.state.user_plugin_root)
            .map_err(|error| VulcanPluginRuntimeError::new(error.to_string()))?
            .ok_or_else(|| {
                VulcanPluginRuntimeError::new("Original activation store is missing.")
            })?;
        store
            .validate_published_target_handoff_owner(transaction)
            .map_err(|error| VulcanPluginRuntimeError::new(error.to_string()))?;
        // Keep the real current catalog through identity validation.
        // 身份校验期间保留真实当前目录。
        let current = manager
            .state
            .snapshot
            .read()
            .map_err(|_| VulcanPluginRuntimeError::new("Plugin catalog lock is poisoned."))?;
        let plugin = final_catalog
            .plugins
            .get(self.plugin_id())
            .ok_or_else(|| VulcanPluginRuntimeError::new("Final MCP target is missing."))?;
        if self.inner.profile.inner.manager_id != manager.state.manager_id
            || !Arc::ptr_eq(&current, final_catalog)
            || transaction.record().request_id() != self.request_id()
            || transaction.record().target() != &self.inner.profile.inner.target
            || plugin.generation != self.generation()
            || plugin.runtime_state != VulcanPluginRuntimeState::Enabled
            || !plugin
                .managed_package()
                .is_some_and(|package| package.matches_activation(&self.inner.profile.inner.target))
        {
            return Err(VulcanPluginRuntimeError::new(
                "Original MCP target handoff authority changed.",
            ));
        }
        // Hold original ownership until the combined Completed boundary confirms every participant.
        // 保留原所有权，直至组合 Completed 边界确认全部参与者。
        let source =
            self.inner.source.lock().map_err(|_| {
                VulcanPluginRuntimeError::new("Target MCP request owner is poisoned.")
            })?;
        if source.request.is_none() {
            return Err(VulcanPluginRuntimeError::new(
                "Target MCP request was already transferred.",
            ));
        }
        if !source.attempt.as_ref().is_some_and(|attempt| {
            attempt.ready_for(runtime_instance_id, partition_lease_instance_id)
        }) {
            return Err(VulcanPluginRuntimeError::new(
                "Original target MCP ready evidence does not match its actual runtime and lease.",
            ));
        }
        // Only genuine source readiness under this same guard creates completion participant evidence.
        // 仅此相同守卫内的真实来源就绪创建完成参与者证据。
        let staged = source.staged.as_ref().ok_or_else(|| {
            VulcanPluginRuntimeError::new("Original closed target MCP catalog was released.")
        })?;
        // The exact original closed catalog is retained only across this short confirmation or rollback boundary.
        // 精确原关闭目录仅在此短时确认或回退边界内保留。
        let evidence = VulcanPluginMcpTargetHandoffEvidence::new(
            transaction,
            self.generation(),
            runtime_instance_id,
            partition_lease_instance_id,
            staged,
        );
        Ok(VulcanPluginMcpTargetHandoffGuard {
            _registration: registration,
            source,
            evidence,
        })
    }

    /// Persist an actual backend readiness observation for its exact immutable runtime and lease UUIDs.
    /// 为实际后端就绪观测的精确不可变运行时及租约 UUID 持久保存。
    /// succeeded records observed readiness or failure; return explicit storage errors without authorizing replay.
    /// succeeded 记录已观测就绪或失败；返回明确存储错误，不授权重放。
    /// authority explicitly identifies the trusted native host that owns this original attempt.
    /// authority 显式标识拥有此原尝试的受信任原生宿主。
    /// # Registered native host contract
    /// 已注册原生宿主契约
    /// The native host must observe the same actual original runtime and lease previously bound to this attempt.
    /// 原生宿主必须观察先前绑定至此尝试的相同实际原运行时及租约。
    /// True requires real protocol/tool readiness; false records a genuine failed observation and never grants replay.
    /// 真值要求真实协议／工具就绪；假值记录真正观测失败，并且绝不授予重放。
    /// ```compile_fail
    /// use vulcan_plugin_runtime::VulcanPluginPreparedMcpTarget;
    /// fn forge(target: &VulcanPluginPreparedMcpTarget) {
    ///     target.finish_transport_observation(true).unwrap();
    /// }
    /// ```
    pub fn finish_transport_observation(
        &self,
        authority: &VulcanPluginRegisteredNativeMcpHostAuthority,
        succeeded: bool,
    ) -> Result<(), VulcanPluginRuntimeError> {
        if !Arc::ptr_eq(&authority.issuer, &self.inner.profile.inner.issuer)
            || authority.issuer.manager.upgrade().is_none()
        {
            return Err(VulcanPluginRuntimeError::new(
                "Target MCP observation requires its original registered native host.",
            ));
        }
        // Observation serialization preserves the sole original startup writer.
        // 观测串行化保留唯一原启动写入者。
        let mut source = self
            .inner
            .source
            .lock()
            .map_err(|_| VulcanPluginRuntimeError::new("Target MCP source is poisoned."))?;
        source
            .attempt
            .as_mut()
            .ok_or_else(|| {
                VulcanPluginRuntimeError::new("Original target MCP attempt is missing.")
            })?
            .finish(succeeded)
            .map_err(|error| VulcanPluginRuntimeError::new(error.to_string()))
    }
}

impl VulcanPluginMcpTargetHandoffGuard<'_> {
    /// Release the original request only inside the host's confirmed combined Completed boundary.
    /// 仅在宿主确认的组合 Completed 边界内释放原请求。
    /// confirmation must match this sealed evidence; returns false without transfer for a foreign witness, with no fallible I/O.
    /// confirmation 必须匹配此封闭证据；外来见证返回假值且不转交，不执行可失败输入输出。
    pub fn confirm_completed(
        &mut self,
        confirmation: &VulcanPluginActivationTargetCompletion,
    ) -> bool {
        if !confirmation.matches_mcp_evidence(&self.evidence) {
            return false;
        }
        self.source.request.take();
        self.source.attempt.take();
        self.source.initialization.take();
        self.source.staged.take();
        true
    }
    /// Borrow sealed actual readiness evidence; no input and no resource transfer.
    /// 借用封闭实际就绪证据；无输入，不转交资源。
    pub fn evidence(&self) -> &VulcanPluginMcpTargetHandoffEvidence {
        &self.evidence
    }
}

impl VulcanPluginMcpTargetStartGuard<'_> {
    /// Bind actual owner UUIDs before releasing the first-dispatch fence; return explicit persistence failures.
    /// 释放首次派发屏障前绑定实际所有者 UUID；返回明确持久化失败。
    /// Both inputs come from the constructed actual runtime and lease; no substitute IDs are allocated.
    /// 两个输入来自已构造实际运行时及租约；不分配替代身份。
    /// authority must be the same registered issuer retained by first dispatch.
    /// authority 必须为首次派发保留的相同注册签发者。
    /// # Registered native host contract
    /// 已注册原生宿主契约
    /// Both UUIDs must come from the constructed original actual runtime and lease retained by this source's native host.
    /// 两个 UUID 必须来自已构造原实际运行时及租约，并由此来源的原生宿主持有。
    /// They must identify owners registered under the complete original target and route, never synthetic proof tokens.
    /// 它们必须标识完整原目标及路由下登记的所有者，绝不能是合成证明令牌。
    /// ```compile_fail
    /// use vulcan_plugin_runtime::VulcanPluginMcpTargetStartGuard;
    /// fn forge(start: &mut VulcanPluginMcpTargetStartGuard<'_>, runtime: uuid::Uuid, lease: uuid::Uuid) {
    ///     start.bind_actual_instances(runtime, lease).unwrap();
    /// }
    /// ```
    pub fn bind_actual_instances(
        &mut self,
        runtime_instance_id: Uuid,
        partition_lease_instance_id: Uuid,
        authority: &VulcanPluginRegisteredNativeMcpHostAuthority,
    ) -> Result<(), VulcanPluginRuntimeError> {
        if !Arc::ptr_eq(&authority.issuer, self.issuer) {
            return Err(VulcanPluginRuntimeError::new(
                "Target MCP instance binding requires its original registered native host.",
            ));
        }
        // Receipt writes serialize independently from backend locks while the registration fence remains retained.
        // 注册屏障保持持有时，回执写入独立于后端锁串行化。
        let mut source = self
            .source
            .lock()
            .map_err(|_| VulcanPluginRuntimeError::new("Target MCP source is poisoned."))?;
        source
            .attempt
            .as_mut()
            .expect("validated original target MCP attempt")
            .bind_instance(runtime_instance_id, partition_lease_instance_id)
            .map_err(|error| VulcanPluginRuntimeError::new(error.to_string()))
    }
}

impl VulcanPluginRuntimeManager {
    /// Explicitly register a trusted native Rust host adapter, separate from all Lua/plugin execution permissions.
    /// 显式注册受信任原生 Rust 宿主适配器，独立于全部 Lua／插件执行权限。
    /// No inputs; return authorization referencing one private issuer bound to this actual manager state.
    /// 无输入；返回引用单个私有签发者的授权，该签发者绑定至此实际管理器状态。
    /// This bootstrap operation trusts the registered host's external resource attestations, like installed native host services.
    /// 此引导操作信任已注册宿主的外部资源证明，与已安装原生宿主服务相同。
    pub fn register_native_mcp_host_adapter(&self) -> VulcanPluginRegisteredNativeMcpHostAuthority {
        VulcanPluginRegisteredNativeMcpHostAuthority {
            issuer: Arc::new(NativeMcpHostIssuer {
                manager: Arc::downgrade(&self.state),
            }),
        }
    }

    /// Prepare the exact staged published target without granting ordinary enable or probe admission.
    /// 准备精确暂存已发布目标，不授予普通启用或探测入场。
    /// transaction is the original Published owner and staged is its actual current Enabling Arc.
    /// transaction 为原 Published 所有者，staged 为其实际当前 Enabling Arc。
    /// authority explicitly authorizes one trusted native host to observe and attest this original target.
    /// authority 显式授权单个受信任原生宿主观察并证明此原目标。
    /// Returns sealed resource ownership and exact launchers; failures occur before transport creation.
    /// 返回密封资源所有权及精确启动器；失败发生在传输创建前。
    pub fn prepare_mcp_activation_target(
        &self,
        transaction: &VulcanPluginActivationTransaction,
        staged: &Arc<VulcanPluginCatalogSnapshot>,
        authority: &VulcanPluginRegisteredNativeMcpHostAuthority,
    ) -> Result<VulcanPluginPreparedMcpTarget, VulcanPluginRuntimeError> {
        if !authority.matches_manager(self) {
            return Err(VulcanPluginRuntimeError::new(
                "MCP target preparation requires this manager's registered native host.",
            ));
        }
        // Revalidate actual immutable contents and complete selection before accessing its launcher.
        // 访问启动器前重新校验实际不可变内容及完整选择。
        let store = VulcanPluginGenerationStore::open_existing(&self.state.user_plugin_root)
            .map_err(|error| VulcanPluginRuntimeError::new(error.to_string()))?
            .ok_or_else(|| {
                VulcanPluginRuntimeError::new("Original activation store is missing.")
            })?;
        store
            .validate_published_activation_owner(transaction)
            .map_err(|error| VulcanPluginRuntimeError::new(error.to_string()))?;
        // Exact staged Arc, selected managed target and route are all authoritative.
        // 精确暂存 Arc、已选受管目标及路由均为权威。
        let current = self
            .state
            .snapshot
            .read()
            .map_err(|_| VulcanPluginRuntimeError::new("Plugin catalog lock is poisoned."))?;
        let target = transaction.record().target().clone();
        let plugin = staged
            .plugins
            .get(&target.plugin_id)
            .cloned()
            .ok_or_else(|| VulcanPluginRuntimeError::new("Staged MCP target is missing."))?;
        if !Arc::ptr_eq(&current, staged)
            || plugin.generation == 0
            || plugin.runtime_state != VulcanPluginRuntimeState::Enabling
            || !transaction
                .record()
                .update_policy()
                .is_some_and(|policy| policy.desired_enabled())
            || !plugin
                .managed_package()
                .is_some_and(|package| package.matches_activation(&target))
        {
            return Err(VulcanPluginRuntimeError::new(
                "MCP target requires the original published closed routing generation.",
            ));
        }
        drop(current);
        // Retain original startup authority and reject every historical attempt before preparing launch files.
        // 保留原启动权威，并在准备启动文件前拒绝每个历史尝试。
        let initialization = transaction
            .target_mcp_initialization_owner(plugin.generation)
            .map_err(|error| VulcanPluginRuntimeError::new(error.to_string()))?;
        initialization
            .require_unattempted()
            .map_err(|error| VulcanPluginRuntimeError::new(error.to_string()))?;
        // Local targets retain the same permanent OS lock before any filesystem preparation.
        // 本地目标在任何文件系统准备前保留同一永久操作系统锁。
        let environment = if plugin.mcp_servers.iter().any(|server| {
            matches!(
                server.manifest.transport,
                VulcanPluginMcpTransportManifest::Stdio { .. }
            )
        }) {
            Some(
                super::super::super::environment::retain_plugin_execution_environment(
                    &self.state,
                    &plugin.id,
                )
                .map_err(VulcanPluginRuntimeError::new)?,
            )
        } else {
            None
        };
        // Launchers derive solely from the retained target manifest and descriptor.
        // 启动器仅从保留的目标清单及描述符派生。
        let manifest = plugin.manifest.as_ref().ok_or_else(|| {
            VulcanPluginRuntimeError::new("Selected MCP target manifest is missing.")
        })?;
        let mut stdio_launchers = BTreeMap::new();
        for server in &plugin.mcp_servers {
            if let VulcanPluginMcpTransportManifest::Stdio { launcher, .. } =
                &server.manifest.transport
            {
                let resolved = super::super::super::process::resolve_plugin_mcp_stdio_launcher(
                    &self.state,
                    &plugin,
                    manifest,
                    launcher,
                )
                .map_err(VulcanPluginRuntimeError::new)?;
                stdio_launchers.insert(server.id.clone(), resolved);
            }
        }
        // Pure source is allocated once and never holds the unfinished request, catalog or physical preparation.
        // 纯来源仅分配一次，绝不持有未完成请求、目录或物理准备。
        let profile = VulcanPluginTargetMcpProfile {
            inner: Arc::new(TargetMcpProfileValues {
                issuer: Arc::clone(&authority.issuer),
                target,
                plugin_id: plugin.id.clone(),
                generation: plugin.generation,
                request_id: transaction.record().request_id(),
                manager_id: self.state.manager_id.clone(),
            }),
        };
        // Control owns only the original unfinished source; native preparation owns package and environment pins.
        // 控制仅拥有原未完成来源；原生准备拥有插件包及环境固定句柄。
        let control = VulcanPluginMcpTargetControl {
            inner: Arc::new(TargetMcpControlValues {
                profile,
                source: Mutex::new(TargetMcpSourceState {
                    staged: Some(Arc::clone(staged)),
                    request: Some(transaction.retain_published_target_owner()),
                    initialization: Some(initialization),
                    attempt: None,
                }),
            }),
        };
        Ok(VulcanPluginPreparedMcpTarget {
            inner: Arc::new(PreparedTargetMcpResources {
                control,
                plugin,
                stdio_launchers,
                _environment: environment,
            }),
        })
    }
}
