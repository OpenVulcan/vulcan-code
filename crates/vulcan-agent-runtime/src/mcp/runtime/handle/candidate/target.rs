//! Selected target preparation retains the same actual MCP lease through closed publication and confirmed opening.
//! 已选目标准备在关闭发布及确认开放期间保留同一实际 MCP 租约。

use super::*;
use crate::mcp::PreparedTargetPluginMcpConfigs;
use std::sync::{
    MutexGuard, RwLockReadGuard, RwLockWriteGuard,
    atomic::{AtomicBool, Ordering},
};
use vulcan_plugin_runtime::{
    VulcanPluginActivationTargetCompletion, VulcanPluginActivationTargetHandoffParticipant,
    VulcanPluginActivationTargetRegistration, VulcanPluginActivationTransaction,
    VulcanPluginCatalogSnapshot, VulcanPluginMcpTargetHandoffEvidence,
    VulcanPluginMcpTargetHandoffGuard, VulcanPluginNativeMcpHandoffAdapter,
    VulcanPluginPreparedMcpTarget, VulcanPluginRegisteredNativeMcpHostAuthority,
    VulcanPluginRuntimeManager,
};

#[cfg(test)]
mod gate_tests;

#[cfg(test)]
mod handoff_tests;

/// Controls actual selected preparation; abandonment closes admission and retires the original resources.
/// 控制实际已选准备；放弃关闭入场并退役原资源。
pub struct TargetPluginMcpRuntime {
    /// Explicit registered host authorization remains bound to the original source issuer.
    /// 显式注册宿主授权保持绑定至原来源签发者。
    authority: VulcanPluginRegisteredNativeMcpHostAuthority,
    /// Same actual lease is used by both prepared and published bindings.
    /// 准备及已发布绑定均使用相同实际租约。
    lease: Arc<McpRuntimePartitionLease>,
    /// Control retains exact owner only until completion; observers retain no resource authority.
    /// 控制仅保留精确所有者至完成；观察者不保留资源权威。
    target: VulcanPluginPreparedMcpTarget,
    /// Original registration belongs to this shared access lineage.
    /// 原登记属于此共享访问谱系。
    access: McpRuntimeAccess,
    /// Independent aggregate release evidence survives cancellation.
    /// 独立聚合释放证据跨取消存续。
    observation: CandidatePluginMcpObservation,
    /// Confirmed completion prevents control destruction from shutting down the now-published same lease.
    /// 确认完成避免控制析构关闭现已发布的相同租约。
    published: AtomicBool,
}

/// Short combined handoff participant holds admission, exact target ownership and actual business gate together.
/// 短时组合交接参与者共同持有入场、精确目标所有权及实际业务门。
pub struct TargetPluginMcpHandoffGuard<'a> {
    /// Actual registration borrow prevents escape from the manager's GAT completion scope.
    /// 实际注册借用防止逃逸管理器的 GAT 完成作用域。
    _registration: &'a VulcanPluginActivationTargetRegistration<'a>,
    /// Same original target runtime owns publication state.
    /// 相同原目标运行时拥有发布状态。
    target: &'a TargetPluginMcpRuntime,
    /// Global drain and shutdown cannot replace the actual original while confirmation runs.
    /// 确认期间全局排空及关闭不能替换实际原实例。
    _admission: MutexGuard<'a, OwnerMcpAdmission>,
    /// Pending original observations remain locked until same-instance transfer is confirmed.
    /// 相同实例转交确认前，待处理原观察保持锁定。
    pending: MutexGuard<'a, BTreeMap<String, Vec<McpRuntimeHandle>>>,
    /// Exact original request is released only after durable combined completion confirmation.
    /// 仅持久组合完成确认后释放精确原请求。
    owner: VulcanPluginMcpTargetHandoffGuard<'a>,
    /// Write guard closes transport acquisition through the entire completion boundary.
    /// 写守卫在整个完成边界内关闭传输获取。
    business: RwLockWriteGuard<'a, McpBusinessGate>,
    /// Actual registered transports cannot be removed or replaced across durable completion.
    /// 持久完成期间不能移除或替换实际已登记传输。
    _transports: RwLockReadGuard<'a, BTreeMap<String, Arc<dyn McpTransport>>>,
    /// Same ready snapshot remains fixed until the completion participant is released.
    /// 完成参与者释放前固定相同就绪快照。
    _snapshot: RwLockReadGuard<'a, McpRuntimeSnapshot>,
    /// Unconfirmed outcomes retain Preparing business closure for same-instance recovery.
    /// 未确认结果保留 Preparing 业务关闭，以供相同实例恢复。
    confirmed: bool,
}

impl TargetPluginMcpRuntime {
    /// Start exact prepared inputs in current's original observation tables before any cancellable wait.
    /// 任何可取消等待前，在 current 原观察表中启动精确准备输入。
    /// prepared carries actual pins and resources; return single-shot control without business admission.
    /// prepared 携带实际固定句柄及资源；返回不带业务入场的单次控制。
    /// authority is explicit native host bootstrap authorization fixed by the original source issuer.
    /// authority 为显式原生宿主引导授权，由原来源签发者固定。
    pub fn start(
        current: &McpRuntimeAccess,
        manager: &VulcanPluginRuntimeManager,
        transaction: &VulcanPluginActivationTransaction,
        prepared: PreparedTargetPluginMcpConfigs,
        authority: &VulcanPluginRegisteredNativeMcpHostAuthority,
    ) -> Result<Self, String> {
        // Transfer owner and definitions together; never accept detached arbitrary configurations.
        // 一起转交所有者及定义；绝不接受脱离所有者的任意配置。
        let (target, servers) = prepared.into_runtime_inputs();
        // Every definition must match the actual nonzero route of the retained target.
        // 每份定义必须匹配保留目标的实际非零路由。
        if servers.iter().any(|server| {
            server.source
                != (crate::mcp::McpServerSource::Plugin {
                    plugin_id: target.plugin_id().to_owned(),
                    generation: target.generation(),
                })
        }) {
            return Err(String::from(
                "Selected target MCP definitions differ from their exact resource owner.",
            ));
        }
        // Actual transport resources retain all package, configuration, credentials and OS environment ownership.
        // 实际传输资源保留全部包、配置、凭据及操作系统环境所有权。
        let (resources, release) = McpTransportResources::new(target.clone());
        // Exact current stage remains fenced until original task registration and dispatch finish.
        // 原任务登记及派发完成前，精确当前暂存保持屏障保护。
        let mut start = target
            .begin_transport_attempt(manager, transaction, authority)
            .map_err(|error| error.to_string())?;
        // Register before spawn waits so old/current access can drain even an unpublished ready target.
        // 在启动等待前登记，使新旧访问均可排空尚未发布的就绪目标。
        let runtime = current.start_preparing_plugin_partition_with_resources(
            target.plugin_id(),
            servers,
            resources,
            true,
        )?;
        // Allocate one actual lease before releasing the short registration fence.
        // 释放短时注册屏障前分配单个实际租约。
        let lease = Arc::new(McpRuntimePartitionLease::new(runtime.clone()));
        // Registered host attests nonces from the actual registered runtime and lease for this exact source and route.
        // 已注册宿主证明来自此精确来源及路由的实际已登记运行时和租约的随机标识。
        if let Err(error) =
            start.bind_actual_instances(runtime.inner.instance_id, lease.instance_id, authority)
        {
            runtime.shutdown();
            return Err(error.to_string());
        }
        drop(start);
        // Observer contains actual release evidence without owning target resources.
        // 观察者包含实际释放证据，不拥有目标资源。
        let observation = CandidatePluginMcpObservation {
            request_id: target.request_id(),
            plugin_id: target.plugin_id().to_owned(),
            runtime: runtime.clone(),
            release,
        };
        Ok(Self {
            authority: authority.clone(),
            lease,
            target,
            access: current.clone(),
            observation,
            published: AtomicBool::new(false),
        })
    }

    /// Clone original diagnostics and release observation; no input and no retained resource authority.
    /// 克隆原诊断及释放观察；无输入，不保留资源权威。
    pub fn observer(&self) -> CandidatePluginMcpObservation {
        self.observation.clone()
    }

    /// Copy opaque identities of the actual original runtime and lease; no input or replacement allocation.
    /// 复制实际原运行时及租约的不透明身份；无输入，不分配替代所有者。
    pub fn actual_instance_ids(&self) -> (uuid::Uuid, uuid::Uuid) {
        (self.lease.runtime.inner.instance_id, self.lease.instance_id)
    }

    /// Observe the actual original readiness and persist its result before any completion participant can be prepared.
    /// 观测实际原就绪并持久保存其结果，然后才可准备任何完成参与者。
    /// maximum_wait bounds only this observation; failure closes business and retains explicit cleanup evidence.
    /// maximum_wait 仅限制此次观测；失败关闭业务并保留明确清理证据。
    pub async fn wait_until_ready(&self, maximum_wait: Duration) -> Result<(), String> {
        // Network and initialization complete without any registration or completion lock.
        // 网络及初始化在不持有任何注册或完成锁时结束。
        let ready = self.observation.wait_until_ready(maximum_wait).await;
        // Published observations inspect the same live instance without resurrecting its transferred source writer.
        // 已发布观测检查相同实时实例，不复活其已转交来源写入者。
        if self.published.load(Ordering::Acquire) {
            return ready;
        }
        // Registered host observes the same actual original task; only its real protocol readiness supplies success.
        // 已注册宿主观察相同实际原任务；仅其真实协议就绪提供成功结果。
        let stored = self
            .target
            .finish_transport_observation(&self.authority, ready.is_ok())
            .map_err(|error| error.to_string());
        // A concurrent handoff holds source until it publishes this latch; missing writer after transfer cannot cancel business.
        // 并发交接持有来源直至发布此锁存位；转交后的写入者缺失不能取消业务。
        if self.published.load(Ordering::Acquire) {
            return ready;
        }
        if ready.is_err() || stored.is_err() {
            // Recheck transfer under the same write gate so an observation failure cannot race confirmed opening.
            // 在相同写门内重新检查转交，防止观测失败与已确认开放竞争。
            let mut business = self
                .lease
                .runtime
                .inner
                .business_gate
                .write()
                .expect("mcp business gate poisoned");
            if !self.published.load(Ordering::Acquire) && *business == McpBusinessGate::Preparing {
                *business = McpBusinessGate::Closed;
                request_mcp_runtime_shutdown(&self.lease.runtime.inner.shutdown_tx);
            }
        }
        match (ready, stored) {
            (Ok(()), Ok(())) => Ok(()),
            (Err(error), Ok(())) | (Ok(()), Err(error)) => Err(error),
            (Err(error), Err(storage)) => Err(format!(
                "{error}; target MCP observation persistence failed: {storage}"
            )),
        }
    }

    /// Build handlers from the actual readiness snapshot while its same lease still rejects invocation.
    /// 从实际就绪快照构建处理器，同时其相同租约仍拒绝调用。
    /// allowlist filters exact original views; return bindings for the one full extension publisher.
    /// allowlist 过滤精确原视图；返回供唯一完整扩展发布者使用的绑定。
    pub fn prepared_tool_bindings_filtered(
        &self,
        allowlist: &McpRuntimeToolAllowlist,
    ) -> Result<Vec<McpRuntimeToolBinding>, String> {
        // This read gate permits catalog construction only for an original preparing instance.
        // 此读门仅允许原准备实例构建目录。
        let business = self
            .lease
            .runtime
            .inner
            .business_gate
            .read()
            .map_err(|_| String::from("Target MCP business gate is poisoned."))?;
        if *business != McpBusinessGate::Preparing
            || self.lease.is_revoked()
            || self.lease.runtime.shutdown_requested()
        {
            return Err(String::from("Target MCP preparation is closed."));
        }
        // Derive views from the same actual snapshot; this is no independent shadow catalog.
        // 从相同实际快照派生视图；这不是独立影子目录。
        let views = self.lease.runtime.snapshot().filtered_tool_views(allowlist);
        Ok(views
            .into_iter()
            .map(|view| {
                McpRuntimeToolBinding::new(
                    view,
                    Arc::clone(&self.lease),
                    format!(
                        "plugin:{}:{}",
                        self.target.plugin_id(),
                        self.target.generation()
                    ),
                )
            })
            .collect())
    }

    /// Create publication access from the current service access while preserving the exact prepared lease.
    /// 从当前服务访问创建发布访问，并保留精确已准备租约。
    /// current must share original admission lineage; return only the changed target partition.
    /// current 必须共享原入场谱系；仅返回变更目标分区的访问。
    pub fn publication_access(
        &self,
        current: &McpRuntimeAccess,
    ) -> Result<McpRuntimeAccess, String> {
        if !Arc::ptr_eq(
            &self.access.blocked_owner_plugins,
            &current.blocked_owner_plugins,
        ) {
            return Err(String::from(
                "Target MCP publication belongs to another access lineage.",
            ));
        }
        Ok(current.with_prepared_target_lease(
            self.target.plugin_id(),
            self.target.generation(),
            Arc::clone(&self.lease),
        ))
    }

    /// Validate every fallible original owner, ready state and actual lease check before the combined Completed write.
    /// 组合 Completed 写入前校验全部可失败原所有者、就绪状态及实际租约。
    /// manager, transaction and final_catalog are exact original authorities; return a closed short transfer guard.
    /// manager、transaction 及 final_catalog 为精确原权威；返回关闭的短时转交守卫。
    /// registration borrows that manager's existing publication exclusion; a foreign token is explicitly rejected.
    /// registration 借用此管理器的既有发布互斥；明确拒绝外来令牌。
    /// ```compile_fail
    /// use vulcan_agent_runtime::mcp::{TargetPluginMcpRuntime, TargetPluginMcpHandoffGuard};
    /// use vulcan_plugin_runtime::{VulcanPluginRuntimeManager, VulcanPluginActivationTransaction, VulcanPluginCatalogSnapshot, VulcanPluginActivationTargetRegistration};
    /// fn escape<'outer, 'gate>(target: &'outer TargetPluginMcpRuntime, manager: &VulcanPluginRuntimeManager, transaction: &VulcanPluginActivationTransaction, catalog: &std::sync::Arc<VulcanPluginCatalogSnapshot>, registration: &'gate VulcanPluginActivationTargetRegistration<'gate>) -> TargetPluginMcpHandoffGuard<'outer> {
    ///     target.prepare_handoff(manager, transaction, catalog, registration).unwrap()
    /// }
    /// ```
    pub fn prepare_handoff<'a>(
        &'a self,
        manager: &VulcanPluginRuntimeManager,
        transaction: &VulcanPluginActivationTransaction,
        final_catalog: &Arc<VulcanPluginCatalogSnapshot>,
        registration: &'a VulcanPluginActivationTargetRegistration<'a>,
    ) -> Result<TargetPluginMcpHandoffGuard<'a>, String> {
        // The unforgeable borrowed token proves the same manager's already-held registration gate.
        // 不可伪造的借用令牌证明相同管理器的注册门已被持有。
        if !registration.matches_manager(manager) {
            return Err(String::from(
                "Target MCP completion registration belongs to another manager.",
            ));
        }
        // Original observations and shutdown share this same admission lock.
        // 原观察及关闭共享此相同入场锁。
        let admission = self
            .access
            .blocked_owner_plugins
            .lock()
            .map_err(|_| String::from("Target MCP admission is poisoned."))?;
        if admission.closed
            || !admission
                .managed_globals
                .get(self.target.plugin_id())
                .is_some_and(|instances| {
                    instances
                        .iter()
                        .any(|runtime| runtime.same_instance(&self.lease.runtime))
                })
        {
            return Err(String::from(
                "Target MCP original startup registration changed.",
            ));
        }
        // Lock original pending observations before the business gate in the same order as plugin drain.
        // 按插件排空相同顺序，在业务门前锁定原待处理观察。
        let pending = self
            .access
            .retired_plugin_partitions
            .lock()
            .map_err(|_| String::from("Target MCP pending observations are poisoned."))?;
        if !pending
            .get(self.target.plugin_id())
            .is_some_and(|instances| {
                instances
                    .iter()
                    .any(|runtime| runtime.same_instance(&self.lease.runtime))
            })
        {
            return Err(String::from(
                "Target MCP original pending observation changed.",
            ));
        }
        // Same lease's actual gate cannot be revoked or shut down between validation and confirmation.
        // 校验与确认之间，相同租约实际业务门不能被撤销或关闭。
        let business = self
            .lease
            .runtime
            .inner
            .business_gate
            .write()
            .map_err(|_| String::from("Target MCP business gate is poisoned."))?;
        // Lock actual transport ownership before diagnostics, matching lifecycle map removal and status publication.
        // 在诊断前锁定实际传输所有权，与生命周期映射移除及状态发布顺序一致。
        let transports = self
            .lease
            .runtime
            .inner
            .transports
            .read()
            .map_err(|_| String::from("Target MCP actual transports are poisoned."))?;
        // Readiness cannot turn into a stale observation while the Completed writer holds this participant.
        // Completed 写入者持有此参与者期间，就绪不能转为过期观测。
        let snapshot = self
            .lease
            .runtime
            .inner
            .snapshot
            .read()
            .map_err(|_| String::from("Target MCP readiness snapshot is poisoned."))?;
        if *business != McpBusinessGate::Preparing
            || self.lease.is_revoked()
            || self.lease.runtime.shutdown_requested()
            || self
                .lease
                .runtime
                .inner
                .registry
                .enabled_servers()
                .into_iter()
                .any(|server| {
                    !transports.contains_key(&server.id)
                        || snapshot
                            .servers
                            .get(&server.id)
                            .is_none_or(|state| state.status != McpRuntimeServerStatus::Connected)
                })
        {
            return Err(String::from(
                "Target MCP original lease is not ready for handoff.",
            ));
        }
        // The same manager registration token and actual admission, pending, business, transport and Ready guards
        // 相同管理器注册令牌及实际入场、待处理、业务、传输和 Ready 守卫
        // remain held across this source guard, with UUIDs from these exact original actual runtime and lease owners.
        // 跨此来源守卫保持持有，UUID 来自这些精确原实际运行时及租约所有者。
        let owner = self
            .target
            .prepare_handoff(
                manager,
                transaction,
                final_catalog,
                self.lease.runtime.inner.instance_id,
                self.lease.instance_id,
                &self.authority,
                registration,
            )
            .map_err(|error| error.to_string())?;
        Ok(TargetPluginMcpHandoffGuard {
            _registration: registration,
            target: self,
            _admission: admission,
            pending,
            owner,
            business,
            _transports: transports,
            _snapshot: snapshot,
            confirmed: false,
        })
    }
}

impl VulcanPluginNativeMcpHandoffAdapter for TargetPluginMcpRuntime {
    /// Every returned guard borrows actual registration and resource owners for exactly this completion scope.
    /// 每个返回守卫在此精确完成作用域内借用实际注册及资源所有者。
    type Guard<'gate>
        = TargetPluginMcpHandoffGuard<'gate>
    where
        Self: 'gate;

    /// Borrow explicit original native host authorization; no new issuer or runtime owner is created.
    /// 借用显式原原生宿主授权；不创建新签发者或运行时所有者。
    fn authority(&self) -> Option<&VulcanPluginRegisteredNativeMcpHostAuthority> {
        Some(&self.authority)
    }

    /// Build same-original native guards inside the borrowed registration scope; return explicit validation errors.
    /// 在借用注册作用域内构建同原原生守卫；返回明确校验错误。
    /// manager/transaction/catalog identify the complete original, and registration bounds the returned lifetime.
    /// manager／transaction／catalog 标识完整原请求，registration 约束返回生命周期。
    fn prepare<'gate>(
        &'gate self,
        manager: &VulcanPluginRuntimeManager,
        transaction: &VulcanPluginActivationTransaction,
        catalog: &Arc<VulcanPluginCatalogSnapshot>,
        registration: &'gate VulcanPluginActivationTargetRegistration<'gate>,
    ) -> Result<Self::Guard<'gate>, vulcan_plugin_runtime::VulcanPluginRuntimeError> {
        self.prepare_handoff(manager, transaction, catalog, registration)
            .map_err(vulcan_plugin_runtime::VulcanPluginRuntimeError::new)
    }
}

impl Drop for TargetPluginMcpRuntime {
    /// Abandoning unconfirmed control stops the same original; published ownership follows its actual lease.
    /// 放弃未确认控制停止相同原实例；已发布所有权跟随其实际租约。
    /// No input or result; asynchronous retirement remains observable separately.
    /// 无输入及结果；异步退役仍可单独观察。
    fn drop(&mut self) {
        if !self.published.load(Ordering::Acquire) {
            self.lease.runtime.shutdown();
        }
    }
}

// This participant retains actual source, admission, transport and Ready guards inside its borrowed registration scope.
// 此参与者在其借用注册作用域内保留实际来源、入场、传输及 Ready 守卫。
impl VulcanPluginActivationTargetHandoffParticipant for TargetPluginMcpHandoffGuard<'_> {
    /// Borrow sealed source evidence while the same actual runtime, lease and source guards remain held.
    /// 相同实际运行时、租约及来源守卫仍保持持有时，借用封闭来源证据。
    /// No inputs; returns the exact Ready evidence validated before durable completion.
    /// 无输入；返回持久完成前已校验的精确 Ready 证据。
    fn mcp_evidence(&self) -> Option<&VulcanPluginMcpTargetHandoffEvidence> {
        Some(self.owner.evidence())
    }
    /// Open the same prepared lease only inside the host's confirmed combined Completed boundary.
    /// 仅在宿主确认的组合 Completed 边界内开放相同已准备租约。
    /// confirmation is an exact sealed same-owner witness; foreign witnesses do nothing and no fallible I/O occurs.
    /// confirmation 为精确封闭同所有者见证；外来见证不执行操作，且不执行可失败输入输出。
    fn confirm_completed(&mut self, confirmation: &VulcanPluginActivationTargetCompletion) {
        if !self.owner.confirm_completed(confirmation) {
            return;
        }
        // The same observed original leaves pending cleanup history while managed lifecycle observation survives.
        // 相同已观察原实例离开待处理清理历史，受管生命周期观察仍存续。
        self.pending
            .get_mut(self.target.target.plugin_id())
            .expect("validated original pending observation")
            .retain(|runtime| !runtime.same_instance(&self.target.lease.runtime));
        *self.business = McpBusinessGate::Open;
        self.target.published.store(true, Ordering::Release);
        self.confirmed = true;
    }
}

impl Drop for TargetPluginMcpHandoffGuard<'_> {
    /// Unconfirmed completion keeps this same original preparing and retains source/task authority for exact recovery.
    /// 未确认完成保持相同原实例准备中，并保留来源／任务权威以供精确恢复。
    /// No input or result; explicit control cancellation or drain alone requests permanent shutdown.
    /// 无输入及结果；仅显式控制取消或排空请求永久关闭。
    fn drop(&mut self) {
        if !self.confirmed {
            *self.business = McpBusinessGate::Preparing;
        }
    }
}
