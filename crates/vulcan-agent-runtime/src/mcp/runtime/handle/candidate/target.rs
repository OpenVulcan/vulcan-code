//! Selected target preparation retains the same actual MCP lease through closed publication and confirmed opening.
//! 已选目标准备在关闭发布及确认开放期间保留同一实际 MCP 租约。

use super::prepared::{
    PreparedPluginMcpLeaseOwnership, PreparedPluginMcpRuntime, PreparedPluginMcpTransferGuard,
};
use super::*;
use crate::mcp::PreparedTargetPluginMcpConfigs;
use vulcan_plugin_runtime::{
    VulcanPluginActivationTargetCompletion, VulcanPluginActivationTargetHandoffParticipant,
    VulcanPluginActivationTargetRegistration, VulcanPluginActivationTransaction,
    VulcanPluginCatalogSnapshot, VulcanPluginMcpTargetControl,
    VulcanPluginMcpTargetHandoffEvidence, VulcanPluginMcpTargetHandoffGuard,
    VulcanPluginNativeMcpHandoffAdapter, VulcanPluginRegisteredNativeMcpHostAuthority,
    VulcanPluginRuntimeManager, VulcanPluginTargetMcpProfile,
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
    /// Shared mechanical preparation owns the same actual lease, observation and access lineage.
    /// 共享机械准备拥有同一实际租约、观察及访问谱系。
    prepared: PreparedPluginMcpRuntime,
    /// Control retains exact owner only until completion; observers retain no resource authority.
    /// 控制仅保留精确所有者至完成；观察者不保留资源权威。
    target: VulcanPluginMcpTargetControl,
    /// Pure source matches the same original profile consumed by every actual launch input Arc.
    /// 纯来源匹配每个实际启动输入 Arc 消费的同一原画像。
    profile: VulcanPluginTargetMcpProfile,
}

/// Short combined handoff participant holds admission, exact target ownership and actual business gate together.
/// 短时组合交接参与者共同持有入场、精确目标所有权及实际业务门。
pub struct TargetPluginMcpHandoffGuard<'a> {
    /// Actual registration borrow prevents escape from the manager's GAT completion scope.
    /// 实际注册借用防止逃逸管理器的 GAT 完成作用域。
    _registration: &'a VulcanPluginActivationTargetRegistration<'a>,
    /// Release this exact source owner before native mechanical guards, preserving source-to-business drop order.
    /// 在原生机械守卫前释放此精确来源所有者，保持来源至业务的销毁顺序。
    owner: VulcanPluginMcpTargetHandoffGuard<'a>,
    /// Common native guards retain the same actual lease last and confer no target completion authorization.
    /// 共用原生守卫最后保留相同实际租约，不授予目标完成授权。
    transfer: PreparedPluginMcpTransferGuard<'a>,
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
        let (prepared, servers) = prepared.into_runtime_inputs();
        // Logical completion control has no reverse link to the actual physical preparation.
        // 逻辑完成控制没有返回实际物理准备的反向链接。
        let target = prepared.control();
        // Bind the original pure source before any persistent attempt or actual native dispatch.
        // 在任何持久尝试或实际原生派发前绑定原纯来源。
        let profile = target
            .host_profile(authority)
            .map_err(|error| error.to_string())?;
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
        let (resources, release) = McpTransportResources::new(prepared);
        // Exact current stage remains fenced until original task registration and dispatch finish.
        // 原任务登记及派发完成前，精确当前暂存保持屏障保护。
        let mut start = target
            .begin_transport_attempt(manager, transaction, authority)
            .map_err(|error| error.to_string())?;
        // Register before spawn waits so old/current access can drain even an unpublished ready target.
        // 在启动等待前登记，使新旧访问均可排空尚未发布的就绪目标。
        let lease = current.start_target_plugin_partition(
            target.plugin_id(),
            servers,
            resources,
            profile.clone(),
        )?;
        // Both original controls and this sole actual lease were registered before native dispatch.
        // 原控制及此唯一实际租约均在原生派发前登记。
        let runtime = lease.runtime.clone();
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
            runtime,
            release,
        };
        Ok(Self {
            authority: authority.clone(),
            prepared: PreparedPluginMcpRuntime::new(current, lease, observation),
            target,
            profile,
        })
    }

    /// Clone original pure source identity without retaining unfinished request or physical resources.
    /// 克隆原纯来源身份，不保留未完成请求或物理资源。
    /// No inputs; return the source fixed before this actual target's native dispatch, not an execution receipt.
    /// 无输入；返回此实际目标原生派发前固定的来源，不返回执行回执。
    pub fn profile(&self) -> VulcanPluginTargetMcpProfile {
        self.profile.clone()
    }

    /// Verify server_id's actual connected target consumed expected before launch without exposing its inputs.
    /// 不暴露输入，校验 server_id 实际已连接目标在启动前消费了 expected。
    /// Return success only for the same original input Arc, native transport, eligible inputs and live release.
    /// 仅相同原输入 Arc、原生传输、合格输入及存续释放均成立时返回成功。
    pub fn verify_source(
        &self,
        server_id: &str,
        expected: &VulcanPluginTargetMcpProfile,
    ) -> Result<(), String> {
        self.prepared
            .observation
            .runtime
            .with_connected_target_launch_inputs(server_id, expected, |_, _| ())
            .map_err(|error| format!("Target MCP launch source is unavailable: {error:?}."))
    }

    /// Clone original diagnostics and release observation; no input and no retained resource authority.
    /// 克隆原诊断及释放观察；无输入，不保留资源权威。
    pub fn observer(&self) -> CandidatePluginMcpObservation {
        self.prepared.observation.clone()
    }

    /// Copy opaque identities of the actual original runtime and lease; no input or replacement allocation.
    /// 复制实际原运行时及租约的不透明身份；无输入，不分配替代所有者。
    pub fn actual_instance_ids(&self) -> (uuid::Uuid, uuid::Uuid) {
        (
            self.prepared.observation.runtime.inner.instance_id,
            self.prepared.partition_lease_instance_id,
        )
    }

    /// Observe this original target using the sole native readiness budget without copying a host-side timeout.
    /// 使用唯一原生就绪预算观测此原目标，不复制宿主侧超时。
    /// No inputs; returns actual persisted readiness or the same explicit failure as wait_until_ready.
    /// 无输入；返回实际持久就绪，或与 wait_until_ready 相同的明确失败。
    pub async fn wait_until_ready_default(&self) -> Result<(), String> {
        self.wait_until_ready(MCP_PARTITION_READINESS_TIMEOUT).await
    }

    /// Observe the actual original readiness and persist its result before any completion participant can be prepared.
    /// 观测实际原就绪并持久保存其结果，然后才可准备任何完成参与者。
    /// maximum_wait bounds only this observation; failure closes business and retains explicit cleanup evidence.
    /// maximum_wait 仅限制此次观测；失败关闭业务并保留明确清理证据。
    pub async fn wait_until_ready(&self, maximum_wait: Duration) -> Result<(), String> {
        // Network and initialization complete without any registration or completion lock.
        // 网络及初始化在不持有任何注册或完成锁时结束。
        let ready = self
            .prepared
            .observation
            .wait_until_ready(maximum_wait)
            .await;
        // Published observations inspect the same live instance without resurrecting its transferred source writer.
        // 已发布观测检查相同实时实例，不复活其已转交来源写入者。
        // Never hold ownership across the preceding await; completion and source writes serialize from here.
        // 绝不跨前述 await 持有所有权；完成及来源写入从此处开始串行化。
        let ownership = self
            .prepared
            .lease
            .lock()
            .map_err(|_| String::from("Target MCP ownership is poisoned."))?;
        if matches!(
            *ownership,
            PreparedPluginMcpLeaseOwnership::Published { .. }
        ) {
            return ready;
        }
        // Registered host observes the same actual original task; only its real protocol readiness supplies success.
        // 已注册宿主观察相同实际原任务；仅其真实协议就绪提供成功结果。
        let stored = self
            .target
            .finish_transport_observation(&self.authority, ready.is_ok())
            .map_err(|error| error.to_string());
        if ready.is_err() || stored.is_err() {
            // Ownership remains Preparing throughout this guarded failure handling; confirmed opening cannot race it.
            // 此受保护失败处理全程所有权保持 Preparing；确认开放不能与其竞争。
            let mut business = self
                .prepared
                .observation
                .runtime
                .inner
                .business_gate
                .write()
                .expect("mcp business gate poisoned");
            if *business == McpBusinessGate::Preparing {
                *business = McpBusinessGate::Closed;
                request_mcp_runtime_shutdown(&self.prepared.observation.runtime.inner.shutdown_tx);
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

    /// Build handlers from the same preparing lease; allowlist filters views and the target supplies its exact route.
    /// 从同一准备租约构建处理器；allowlist 过滤视图，目标提供其精确路由。
    /// Return closed bindings for the sole extension publisher without granting completion authority.
    /// 返回供唯一扩展发布者使用的关闭绑定，不授予完成权限。
    pub fn prepared_tool_bindings_filtered(
        &self,
        allowlist: &McpRuntimeToolAllowlist,
    ) -> Result<Vec<McpRuntimeToolBinding>, String> {
        self.prepared.prepared_tool_bindings_filtered(
            allowlist,
            self.target.plugin_id(),
            self.target.generation(),
        )
    }

    /// Create publication access preserving the same prepared lease and original access lineage.
    /// 创建发布访问，保留同一已准备租约及原访问谱系。
    /// current must share the original lineage; return only the exact target partition change or an error.
    /// current 必须共享原谱系；仅返回精确目标分区变更或错误。
    pub fn publication_access(
        &self,
        current: &McpRuntimeAccess,
    ) -> Result<McpRuntimeAccess, String> {
        self.prepared
            .publication_access(current, self.target.plugin_id(), self.target.generation())
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
        // Acquire the original native locks mechanically after proving this exact manager's registration.
        // 证明此精确管理器注册之后，机械获取原生原锁。
        let transfer = self.prepared.prepare_transfer(self.target.plugin_id())?;
        // Target source validation remains explicit here and receives the same guarded actual transport/input pair.
        // 目标来源校验在此保持显式，并接收相同受守卫保护的实际传输／输入对。
        transfer.validate_actual_inputs(|transport, actual| {
            actual
                .with_target_source(transport, &self.profile, |_, _| ())
                .map_err(|error| {
                    format!("Target MCP original launch source is unavailable during handoff: {error:?}.")
                })
        })?;
        // Read both nonces from retained actual native owners, never from diagnostic copies.
        // 从保留的实际原生所有者读取两个随机标识，绝不从诊断副本读取。
        let (runtime_instance_id, partition_lease_instance_id) = transfer.actual_instance_ids();
        // The same registration and all native locks remain held while the exact Target authority validates completion.
        // 精确目标权威校验完成期间，相同注册及全部原生锁保持持有。
        let owner = self
            .target
            .prepare_handoff(
                manager,
                transaction,
                final_catalog,
                runtime_instance_id,
                partition_lease_instance_id,
                &self.authority,
                registration,
            )
            .map_err(|error| error.to_string())?;
        Ok(TargetPluginMcpHandoffGuard {
            _registration: registration,
            owner,
            transfer,
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
        // Only this source-specific sealed witness permits the shared infallible actual transfer.
        // 仅此特定来源封闭见证允许共享无失败实际转交。
        self.transfer.confirm_after_source();
    }
}
