//! Private resource mechanics retain one actual prepared MCP lease without granting source-specific completion.
//! 私有资源机制保留单个实际已准备 MCP 租约，不授予特定来源的完成权限。

use super::*;
use std::sync::Weak;
use std::sync::{MutexGuard, RwLockReadGuard, RwLockWriteGuard};

/// The sole ownership state transfers the same actual lease once after source-specific confirmation.
/// 唯一所有权状态在特定来源确认后将相同实际租约转交一次。
pub(super) enum PreparedPluginMcpLeaseOwnership {
    /// Original preparation owns its actual lease until the authorized completion accepts it.
    /// 原准备拥有其实际租约，直至获授权的完成接纳它。
    Preparing(Arc<McpRuntimePartitionLease>),
    /// Historical control identifies the transferred lease without retaining execution authority.
    /// 历史控制标识已转交租约，不保留执行权威。
    Published {
        /// Weak original identity cannot keep the last execution owner alive.
        /// 弱原身份不能使最后一个执行所有者继续存活。
        _lease: Weak<McpRuntimePartitionLease>,
    },
}

/// Own the actual preparation resources and observations shared by source-specific adapters.
/// 拥有特定来源适配器共用的实际准备资源及观察。
/// This type supplies no request authorization, initialization permission or durable completion writer.
/// 此类型不提供请求授权、初始化权限或持久完成写入者。
pub(super) struct PreparedPluginMcpRuntime {
    /// Same actual lease is used by both prepared and published bindings.
    /// 准备及已发布绑定均使用相同实际租约。
    pub(super) lease: Mutex<PreparedPluginMcpLeaseOwnership>,
    /// Original registration belongs to this shared access lineage.
    /// 原登记属于此共享访问谱系。
    pub(super) access: McpRuntimeObservationAccess,
    /// Independent aggregate release evidence survives cancellation.
    /// 独立聚合释放证据跨取消存续。
    pub(super) observation: CandidatePluginMcpObservation,
    /// Diagnostic copy comes from the allocated actual lease and grants no completion authority.
    /// 诊断副本来自已分配实际租约，不授予完成权威。
    pub(super) partition_lease_instance_id: uuid::Uuid,
}

/// Mechanical transfer retains actual native guards without source-specific registration or completion authority.
/// 机械转交保留实际原生守卫，不具有特定来源注册或完成权威。
pub(super) struct PreparedPluginMcpTransferGuard<'a> {
    /// The caller's validated literal namespace selects only this original pending observation.
    /// 调用方已校验字面命名空间仅选择此原待处理观察。
    plugin_id: &'a str,
    /// Global drain and shutdown cannot replace the actual original during confirmation.
    /// 确认期间全局排空及关闭不能替换实际原实例。
    _admission: MutexGuard<'a, OwnerMcpAdmission>,
    /// Pending originals remain fixed until the source-specific owner confirms transfer.
    /// 特定来源所有者确认转交前，原待处理实例保持固定。
    pending: MutexGuard<'a, BTreeMap<String, Vec<McpRuntimeHandle>>>,
    /// The same actual lease ownership serializes readiness observations and transfer.
    /// 相同实际租约所有权串行化就绪观察及转交。
    ownership: MutexGuard<'a, PreparedPluginMcpLeaseOwnership>,
    /// Transport acquisition remains closed across the durable source-specific confirmation.
    /// 跨特定来源持久确认期间，传输获取保持关闭。
    business: RwLockWriteGuard<'a, McpBusinessGate>,
    /// Actual transports cannot be replaced between validation and confirmation.
    /// 校验及确认之间不能替换实际传输。
    transports: RwLockReadGuard<'a, BTreeMap<String, Arc<dyn McpTransport>>>,
    /// Protected original input allocations remain bound to their actual transport.
    /// 受保护原输入分配保持绑定至其实际传输。
    inputs: RwLockReadGuard<'a, BTreeMap<String, crate::mcp::registry::McpActualLaunchInputLease>>,
    /// The actual Connected snapshot remains held throughout source-specific confirmation.
    /// 特定来源确认全程保持持有实际 Connected 快照。
    _snapshot: RwLockReadGuard<'a, McpRuntimeSnapshot>,
    /// Unconfirmed abandonment retains Preparing instead of granting native business.
    /// 未确认放弃保留 Preparing，而非授予原生业务。
    confirmed: bool,
    /// Drop last so final-lease shutdown cannot reacquire this guard's still-held business write lock.
    /// 最后销毁，避免最后租约关闭重新获取此守卫仍持有的业务写锁。
    /// This actual owner supplies instance IDs independently of diagnostic copies.
    /// 此实际所有者独立于诊断副本提供实例身份。
    lease: Arc<McpRuntimePartitionLease>,
}

impl PreparedPluginMcpTransferGuard<'_> {
    /// Copy actual original runtime and lease IDs; no inputs or authorization is returned.
    /// 复制实际原运行时及租约身份；无输入，不返回授权。
    pub(super) fn actual_instance_ids(&self) -> (uuid::Uuid, uuid::Uuid) {
        (self.lease.runtime.inner.instance_id, self.lease.instance_id)
    }

    /// Validate every enabled actual input with the caller's source-specific verifier under retained native locks.
    /// 在保留原生锁下，用调用方特定来源校验器校验每个启用实际输入。
    /// verify borrows the same transport and input allocation; return its explicit rejection without exposing ownership.
    /// verify 借用相同传输及输入分配；返回其明确拒绝，不暴露所有权。
    pub(super) fn validate_actual_inputs(
        &self,
        mut verify: impl FnMut(
            &Arc<dyn McpTransport>,
            &crate::mcp::registry::McpActualLaunchInputLease,
        ) -> Result<(), String>,
    ) -> Result<(), String> {
        for server in self.lease.runtime.inner.registry.enabled_servers() {
            // Construction already checked every declaration while these same maps remained locked.
            // 构造已在这些相同映射保持锁定期间检查每个声明。
            let transport = self
                .transports
                .get(&server.id)
                .expect("validated original actual transport");
            // The original protected input is never reconstructed from configuration values.
            // 原受保护输入绝不从配置值重建。
            let actual = self
                .inputs
                .get(&server.id)
                .expect("validated original actual launch inputs");
            verify(transport, actual)?;
        }
        Ok(())
    }

    /// Transfer the same actual original only after the caller confirms its source-specific durable owner.
    /// 仅调用方确认其特定来源持久所有者之后，才转交相同实际原实例。
    /// No input or result; this mechanical method grants no independent completion authority and performs no I/O.
    /// 无输入及结果；此机械方法不授予独立完成权威，且不执行输入输出。
    pub(super) fn confirm_after_source(&mut self) {
        // Remove only this actual original; managed lifecycle observations remain available after transfer.
        // 仅移除此实际原实例；转交后受管生命周期观察仍可使用。
        self.pending
            .get_mut(self.plugin_id)
            .expect("validated original pending observation")
            .retain(|runtime| !runtime.same_instance(&self.lease.runtime));
        *self.business = McpBusinessGate::Open;
        *self.ownership = PreparedPluginMcpLeaseOwnership::Published {
            _lease: Arc::downgrade(&self.lease),
        };
        self.confirmed = true;
    }
}

impl Drop for PreparedPluginMcpTransferGuard<'_> {
    /// Keep unconfirmed actual work preparing without shutdown or replay; no inputs or return value.
    /// 保持未确认实际工作准备中，不关闭或重放；无输入及返回值。
    fn drop(&mut self) {
        if !self.confirmed {
            *self.business = McpBusinessGate::Preparing;
        }
    }
}

impl PreparedPluginMcpRuntime {
    /// Acquire the same actual native transfer locks for plugin_id, without accepting any source authorization.
    /// 为 plugin_id 取得相同实际原生转交锁，不接收任何来源授权。
    /// Caller validates its own registration first and its source-specific original before confirmation.
    /// 调用方先校验自身注册，并在确认前校验其特定来源原请求。
    /// Return only retained mechanical evidence or an explicit error; this method never accepts authorization tokens.
    /// 仅返回保留机械证据或明确错误；此方法绝不接收授权令牌。
    pub(super) fn prepare_transfer<'a>(
        &'a self,
        plugin_id: &'a str,
    ) -> Result<PreparedPluginMcpTransferGuard<'a>, String> {
        // Preserve the original admission -> pending -> ownership -> business -> transports -> inputs -> Ready order.
        // 保留原入场 → 待处理 → 所有权 → 业务 → 传输 → 输入 → Ready 顺序。
        let admission = self
            .access
            .blocked_owner_plugins
            .lock()
            .map_err(|_| String::from("Target MCP admission is poisoned."))?;
        if admission.closed
            || !admission
                .managed_globals
                .get(plugin_id)
                .is_some_and(|instances| {
                    instances
                        .iter()
                        .any(|runtime| runtime.same_instance(&self.observation.runtime))
                })
        {
            return Err(String::from(
                "Target MCP original startup registration changed.",
            ));
        }
        // Original pending evidence shares the same actual access lineage as startup.
        // 原待处理证据与启动共享相同实际访问谱系。
        let pending = self
            .access
            .retired_plugin_partitions
            .lock()
            .map_err(|_| String::from("Target MCP pending observations are poisoned."))?;
        if !pending.get(plugin_id).is_some_and(|instances| {
            instances
                .iter()
                .any(|runtime| runtime.same_instance(&self.observation.runtime))
        }) {
            return Err(String::from(
                "Target MCP original pending observation changed.",
            ));
        }
        // Only the existing Preparing state owns the genuine lease to transfer.
        // 仅既有 Preparing 状态拥有要转交的真正租约。
        let ownership = self
            .lease
            .lock()
            .map_err(|_| String::from("Target MCP ownership is poisoned."))?;
        let PreparedPluginMcpLeaseOwnership::Preparing(lease) = &*ownership else {
            return Err(String::from(
                "Target MCP ownership was already transferred.",
            ));
        };
        // Local reverse drop order releases every later guard before this actual lease on pre-construction errors.
        // 构造前错误时，局部逆序销毁在此实际租约前释放每个后续守卫。
        let lease = Arc::clone(lease);
        if !lease.runtime.same_instance(&self.observation.runtime) {
            return Err(String::from("Target MCP actual lease identity changed."));
        }
        // The actual same runtime's admission cannot close while its write gate remains held.
        // 实际相同运行时写门保持持有期间，其入场不能关闭。
        let business = self
            .observation
            .runtime
            .inner
            .business_gate
            .write()
            .map_err(|_| String::from("Target MCP business gate is poisoned."))?;
        // Match actual lifecycle removal order before freezing input and diagnostic maps.
        // 固定输入及诊断映射前匹配实际生命周期移除顺序。
        let transports = self
            .observation
            .runtime
            .inner
            .transports
            .read()
            .map_err(|_| String::from("Target MCP actual transports are poisoned."))?;
        // Keep the exact protected input allocation bound throughout confirmation.
        // 确认全程保持精确受保护输入分配绑定。
        let inputs = self
            .observation
            .runtime
            .inner
            .launch_inputs
            .read()
            .map_err(|_| String::from("Target MCP actual launch inputs are poisoned."))?;
        // Hold the actual current Ready snapshot rather than trusting a historical copy.
        // 持有实际当前 Ready 快照，而非信任历史副本。
        let snapshot = self
            .observation
            .runtime
            .inner
            .snapshot
            .read()
            .map_err(|_| String::from("Target MCP readiness snapshot is poisoned."))?;
        if *business != McpBusinessGate::Preparing
            || lease.is_revoked()
            || lease.runtime.shutdown_requested()
        {
            return Err(String::from(
                "Target MCP original lease is not ready for handoff.",
            ));
        }
        for server in self.observation.runtime.inner.registry.enabled_servers() {
            if !transports.contains_key(&server.id) {
                return Err(String::from(
                    "Target MCP actual transport is missing during handoff.",
                ));
            }
            if !inputs.contains_key(&server.id) {
                return Err(String::from(
                    "Target MCP actual launch inputs are missing during handoff.",
                ));
            }
            if snapshot
                .servers
                .get(&server.id)
                .is_none_or(|state| state.status != McpRuntimeServerStatus::Connected)
            {
                return Err(String::from(
                    "Target MCP original discovery is not ready for handoff.",
                ));
            }
        }
        Ok(PreparedPluginMcpTransferGuard {
            plugin_id,
            _admission: admission,
            pending,
            ownership,
            business,
            transports,
            inputs,
            _snapshot: snapshot,
            confirmed: false,
            lease,
        })
    }

    /// Retain the already registered actual lease and observation without starting or authorizing work.
    /// 保留已登记的实际租约及观察，不启动工作或授予工作权限。
    /// current identifies the original access lineage; lease and observation come from the same actual startup.
    /// current 标识原访问谱系；lease 及 observation 来自同一次实际启动。
    /// Return sole mechanical ownership; the caller keeps all source-specific authorization separate.
    /// 返回唯一机械所有权；调用方单独保留全部特定来源授权。
    pub(super) fn new(
        current: &McpRuntimeAccess,
        lease: Arc<McpRuntimePartitionLease>,
        observation: CandidatePluginMcpObservation,
    ) -> Self {
        Self {
            partition_lease_instance_id: lease.instance_id,
            lease: Mutex::new(PreparedPluginMcpLeaseOwnership::Preparing(lease)),
            access: current.observation_access(),
            observation,
        }
    }

    /// Build handlers from the actual readiness snapshot while its same lease still rejects invocation.
    /// 从实际就绪快照构建处理器，同时其相同租约仍拒绝调用。
    /// allowlist filters exact original views; return bindings for the one full extension publisher.
    /// allowlist 过滤精确原视图；返回供唯一完整扩展发布者使用的绑定。
    /// plugin_id and generation are the source adapter's validated route, used only to label these same lease bindings.
    /// plugin_id 及 generation 为来源适配器已校验路由，仅用于标记这些相同租约绑定。
    pub(super) fn prepared_tool_bindings_filtered(
        &self,
        allowlist: &McpRuntimeToolAllowlist,
        plugin_id: &str,
        generation: u64,
    ) -> Result<Vec<McpRuntimeToolBinding>, String> {
        // Bindings may acquire the original lease only before its single transfer.
        // 绑定仅在单次转交前可以获取原租约。
        let ownership = self
            .lease
            .lock()
            .map_err(|_| String::from("Target MCP ownership is poisoned."))?;
        let PreparedPluginMcpLeaseOwnership::Preparing(lease) = &*ownership else {
            return Err(String::from(
                "Target MCP ownership was already transferred.",
            ));
        };
        // This read gate permits catalog construction only for an original preparing instance.
        // 此读门仅允许原准备实例构建目录。
        let business = lease
            .runtime
            .inner
            .business_gate
            .read()
            .map_err(|_| String::from("Target MCP business gate is poisoned."))?;
        if *business != McpBusinessGate::Preparing
            || lease.is_revoked()
            || lease.runtime.shutdown_requested()
        {
            return Err(String::from("Target MCP preparation is closed."));
        }
        // Derive views from the same actual snapshot; this is no independent shadow catalog.
        // 从相同实际快照派生视图；这不是独立影子目录。
        let views = lease.runtime.snapshot().filtered_tool_views(allowlist);
        Ok(views
            .into_iter()
            .map(|view| {
                McpRuntimeToolBinding::new(
                    view,
                    Arc::clone(lease),
                    format!("plugin:{}:{}", plugin_id, generation),
                )
            })
            .collect())
    }

    /// Create publication access from the current service access while preserving the exact prepared lease.
    /// 从当前服务访问创建发布访问，并保留精确已准备租约。
    /// current must share original admission lineage; return only the changed plugin partition.
    /// current 必须共享原入场谱系；仅返回变更插件分区的访问。
    /// plugin_id and generation are the source adapter's validated route; neither value grants completion authority.
    /// plugin_id 及 generation 为来源适配器已校验路由；任何值均不授予完成权限。
    pub(super) fn publication_access(
        &self,
        current: &McpRuntimeAccess,
        plugin_id: &str,
        generation: u64,
    ) -> Result<McpRuntimeAccess, String> {
        if !self.access.same_lineage(current) {
            return Err(String::from(
                "Target MCP publication belongs to another access lineage.",
            ));
        }
        // Release ownership before acquiring shared publication registries in their existing order.
        // 按既有顺序获取共享发布注册表前释放所有权。
        let lease = {
            // Only the same original unpublished owner grants a new execution binding.
            // 仅相同原未发布所有者授予新的执行绑定。
            let ownership = self
                .lease
                .lock()
                .map_err(|_| String::from("Target MCP ownership is poisoned."))?;
            let PreparedPluginMcpLeaseOwnership::Preparing(lease) = &*ownership else {
                return Err(String::from(
                    "Target MCP ownership was already transferred.",
                ));
            };
            Arc::clone(lease)
        };
        Ok(current.with_prepared_plugin_lease(plugin_id, generation, lease))
    }
}

impl Drop for PreparedPluginMcpRuntime {
    /// Abandoning unconfirmed preparation stops the same original; published ownership follows its actual lease.
    /// 放弃未确认准备停止相同原实例；已发布所有权跟随其实际租约。
    /// No input or result; asynchronous retirement remains observable separately.
    /// 无输入及结果；异步退役仍可单独观察。
    fn drop(&mut self) {
        if matches!(
            *self.lease.get_mut().expect("target MCP ownership poisoned"),
            PreparedPluginMcpLeaseOwnership::Preparing(_)
        ) {
            self.observation.runtime.shutdown();
        }
    }
}
