//! Original aborted old MCP preparation stays private and retains its same actual native owners.
//! 原中止旧 MCP 准备保持私有，并保留其相同实际原生所有者。

use super::prepared::{
    PreparedPluginMcpLeaseOwnership, PreparedPluginMcpRuntime, PreparedPluginMcpTransferGuard,
};
use super::*;
use crate::mcp::PreparedPredecessorPluginMcpConfigs;
use vulcan_plugin_runtime::{
    VulcanPluginActivationPredecessorCompletion,
    VulcanPluginActivationPredecessorHandoffParticipant,
    VulcanPluginActivationPredecessorPublication, VulcanPluginActivationPredecessorRegistration,
    VulcanPluginActivationTransaction, VulcanPluginMcpPredecessorControl,
    VulcanPluginMcpPredecessorHandoffEvidence, VulcanPluginMcpPredecessorHandoffGuard,
    VulcanPluginNativeMcpPredecessorHandoffAdapter, VulcanPluginPredecessorMcpProfile,
    VulcanPluginPredecessorRestorationSource, VulcanPluginRegisteredNativeMcpHostAuthority,
    VulcanPluginRuntimeManager,
};

/// Prepare one original old restoration and transfer its same actual resources only after exact predecessor completion.
/// 准备单个原旧恢复，并仅在精确前驱完成后转交其相同实际资源。
pub struct PredecessorPluginMcpRuntime {
    /// The first registered native issuer alone may attest this actual original attempt.
    /// 仅首次已注册原生签发者可以证明此实际原尝试。
    authority: VulcanPluginRegisteredNativeMcpHostAuthority,
    /// Mechanical ownership retains the exact registered lease until retirement or confirmed same-instance transfer.
    /// 机械所有权保留精确已登记租约，直至退役或确认相同实例转交。
    prepared: PreparedPluginMcpRuntime,
    /// Historical control retains no physical source or recovery file.
    /// 历史控制不保留物理来源或恢复文件。
    predecessor: VulcanPluginMcpPredecessorControl,
    /// Pure original profile is consumed by each actual native launch input allocation.
    /// 每个实际原生启动输入分配消费纯原画像。
    profile: VulcanPluginPredecessorMcpProfile,
}

/// The predecessor participant retains exact source ownership and native guards within its original registration scope.
/// 前驱参与者在其原注册作用域内保留精确来源所有权及原生守卫。
pub struct PredecessorPluginMcpHandoffGuard<'gate> {
    /// The borrowed predecessor scope prevents this participant from escaping the actual manager's completion gate.
    /// 借用前驱作用域防止此参与者逃逸实际管理器完成门。
    _registration: &'gate VulcanPluginActivationPredecessorRegistration<'gate>,
    /// Drop this original core owner before native guards to preserve source-to-business release order.
    /// 在原生守卫前销毁此原核心所有者，保持来源至业务释放顺序。
    owner: VulcanPluginMcpPredecessorHandoffGuard<'gate>,
    /// Actual shared native locks release before their last retained lease; no independent completion authority exists here.
    /// 实际共享原生锁在其最后保留租约前释放；此处不具有独立完成权威。
    transfer: PreparedPluginMcpTransferGuard<'gate>,
}

impl PredecessorPluginMcpRuntime {
    /// Consume prepared under manager's exact aborted transaction and registered authority.
    /// 在 manager 精确中止事务及已注册 authority 下消费 prepared。
    /// current registers actual old work before dispatch; return private control or an explicit error.
    /// current 在派发前登记实际旧工作；返回私有控制或明确错误。
    pub fn start(
        current: &McpRuntimeAccess,
        manager: &VulcanPluginRuntimeManager,
        transaction: &VulcanPluginActivationTransaction,
        prepared: PreparedPredecessorPluginMcpConfigs,
        authority: &VulcanPluginRegisteredNativeMcpHostAuthority,
    ) -> Result<Self, String> {
        // The sealed pair prevents arbitrary definitions from replacing their physical old owner.
        // 密封组合防止任意定义替换其物理旧所有者。
        let (prepared, servers) = prepared.into_runtime_inputs();
        // Logical state and pure profile preserve the first issuer without retaining physical files.
        // 逻辑状态及纯画像保留首次签发者，不保留物理文件。
        let predecessor = prepared.control();
        // Reject foreign authorization before irreversible dispatch or native effects.
        // 在不可逆派发或原生副作用前拒绝外来授权。
        let profile = predecessor
            .host_profile(authority)
            .map_err(|error| error.to_string())?;
        if servers.iter().any(|server| {
            server.source
                != (crate::mcp::McpServerSource::Plugin {
                    plugin_id: predecessor.plugin_id().to_owned(),
                    generation: predecessor.generation(),
                })
        }) {
            return Err(String::from(
                "Predecessor MCP definitions differ from their original resource owner.",
            ));
        }
        // Actual tasks and transports own old pins and environment, independently of control history.
        // 实际任务及传输拥有旧固定句柄及环境，独立于控制历史。
        let (resources, release) = McpTransportResources::new(prepared);
        // Consume once before native admission; cancellation or failure never resets this source.
        // 在原生入场前消费一次；取消或失败绝不重置此来源。
        let mut start = predecessor
            .begin_transport_attempt(manager, transaction, authority)
            .map_err(|error| error.to_string())?;
        // Local plugin drain remains closed while the sole actual original is registered and dispatched.
        // 登记及派发唯一实际原实例期间，本地插件排空保持关闭。
        let lease = current.start_predecessor_plugin_partition(
            predecessor.plugin_id(),
            servers,
            resources,
            profile.clone(),
        )?;
        // IDs originate from the same actual registered runtime and lease, never diagnostic reconstruction.
        // 身份源自相同实际已登记运行时及租约，绝不从诊断重建。
        let runtime = lease.runtime.clone();
        if let Err(error) =
            start.bind_actual_instances(runtime.inner.instance_id, lease.instance_id, authority)
        {
            runtime.shutdown();
            return Err(error.to_string());
        }
        drop(start);
        // Observations retain only actual release evidence and lifecycle diagnostics.
        // 观察仅保留实际释放证据及生命周期诊断。
        let observation = CandidatePluginMcpObservation {
            request_id: predecessor.request_id(),
            plugin_id: predecessor.plugin_id().to_owned(),
            runtime,
            release,
        };
        Ok(Self {
            authority: authority.clone(),
            prepared: PreparedPluginMcpRuntime::new(current, lease, observation),
            predecessor,
            profile,
        })
    }

    /// Clone the same pure source allocation; no inputs, startup rights or physical resource retention.
    /// 克隆相同纯来源分配；无输入、启动权限或物理资源保留。
    pub fn profile(&self) -> VulcanPluginPredecessorMcpProfile {
        self.profile.clone()
    }

    /// Clone historical logical control; no inputs and no additional physical resource owner.
    /// 克隆历史逻辑控制；无输入，不增加物理资源所有者。
    pub fn control(&self) -> VulcanPluginMcpPredecessorControl {
        self.predecessor.clone()
    }

    /// Clone original release and lifecycle diagnostics; no inputs or startup permission.
    /// 克隆原释放及生命周期诊断；无输入或启动权限。
    pub fn observer(&self) -> CandidatePluginMcpObservation {
        self.prepared.observation.clone()
    }

    /// Copy actual original runtime and lease identities; no inputs or replacement allocation.
    /// 复制实际原运行时及租约身份；无输入，不分配替代实例。
    pub fn actual_instance_ids(&self) -> (uuid::Uuid, uuid::Uuid) {
        (
            self.prepared.observation.runtime.inner.instance_id,
            self.prepared.partition_lease_instance_id,
        )
    }

    /// Verify server_id consumed expected's same profile before actual launch under native eligibility checks.
    /// 在原生资格检查下，校验 server_id 在实际启动前消费 expected 相同画像。
    /// Return success for the same live actual input allocation, or a value-free source rejection.
    /// 对同一存活实际输入分配返回成功，否则返回不含值的来源拒绝。
    pub fn verify_source(
        &self,
        server_id: &str,
        expected: &VulcanPluginPredecessorMcpProfile,
    ) -> Result<(), String> {
        self.prepared
            .observation
            .runtime
            .with_connected_predecessor_launch_inputs(server_id, expected, |_, _| ())
            .map_err(|error| format!("Predecessor MCP launch source is unavailable: {error:?}."))
    }

    /// Wait using the sole existing native readiness budget; no inputs or business opening.
    /// 使用唯一既有原生就绪预算等待；无输入，不开放业务。
    pub async fn wait_until_ready_default(&self) -> Result<(), String> {
        self.wait_until_ready(MCP_PARTITION_READINESS_TIMEOUT).await
    }

    /// Observe the original attempt within maximum_wait; only Preparing confirms its same terminal logical result.
    /// 在 maximum_wait 内观察原尝试；仅 Preparing 确认其相同终态逻辑结果。
    /// Preparing success validates every actual old source with closed admission; Published returns only its actual live observation.
    /// Preparing 成功在关闭入场下校验每个实际旧来源；Published 仅返回其实际实时观察。
    pub async fn wait_until_ready(&self, maximum_wait: Duration) -> Result<(), String> {
        // Await without source, registration, native ownership or business locks.
        // 等待时不持有来源、注册、原生所有权或业务锁。
        let ready = self
            .prepared
            .observation
            .wait_until_ready(maximum_wait)
            .await;
        // Serialize source observation and failure closure with transfer only after the asynchronous wait has finished.
        // 仅异步等待结束后，将来源观察及失败关闭与转交串行化。
        let ownership = self
            .prepared
            .lease
            .lock()
            .map_err(|_| String::from("Predecessor MCP ownership is poisoned."))?;
        if matches!(
            *ownership,
            PreparedPluginMcpLeaseOwnership::Published { .. }
        ) {
            return ready;
        }
        // Preparing alone may still attest this original source; a published observation never rewrites transferred control.
        // 仅 Preparing 仍可证明此原来源；已发布观察绝不改写已转交控制。
        let ready = ready.and_then(|()| {
            self.prepared
                .observation
                .runtime
                .inner
                .registry
                .enabled_servers()
                .iter()
                .try_for_each(|server| self.verify_source(&server.id, &self.profile))
        });
        // Observation never grants transfer permission; core records only this same bound attempt.
        // 观察绝不授予转交权限；核心仅记录此同一已绑定尝试。
        let stored = self
            .predecessor
            .finish_transport_observation(&self.authority, ready.is_ok())
            .map_err(|error| error.to_string());
        if ready.is_err() || stored.is_err() {
            // Ownership stays Preparing while the same actual business gate closes, so confirmed transfer cannot race failure.
            // 相同实际业务门关闭期间，所有权保持 Preparing，因此已确认转交不能与失败竞争。
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
            (Err(error), Err(observation)) => Err(format!(
                "{error}; predecessor MCP observation failed: {observation}"
            )),
        }
    }

    /// Build closed original bindings from the same preparing lease; allowlist filters views without granting transfer.
    /// 从相同准备租约构建关闭原绑定；allowlist 过滤视图，不授予转交权限。
    /// Return bindings labeled with this source's exact old generation, or an explicit unavailable error.
    /// 返回以此来源精确旧代次标记的绑定，或明确不可用错误。
    pub fn prepared_tool_bindings_filtered(
        &self,
        allowlist: &McpRuntimeToolAllowlist,
    ) -> Result<Vec<McpRuntimeToolBinding>, String> {
        self.prepared.prepared_tool_bindings_filtered(
            allowlist,
            self.predecessor.plugin_id(),
            self.predecessor.generation(),
        )
    }

    /// Create publication access from current while retaining this same prepared old lease and original access lineage.
    /// 从 current 创建发布访问，同时保留此相同已准备旧租约及原访问谱系。
    /// Return only the exact old partition change; this operation creates no runtime or completion authorization.
    /// 仅返回精确旧分区变更；此操作不创建运行时或完成授权。
    pub fn publication_access(
        &self,
        current: &McpRuntimeAccess,
    ) -> Result<McpRuntimeAccess, String> {
        self.prepared.publication_access(
            current,
            self.predecessor.plugin_id(),
            self.predecessor.generation(),
        )
    }

    /// Validate exact source and actual native owners before durable predecessor transfer, retaining all guards until confirmation.
    /// 持久前驱转交前校验精确来源及实际原生所有者，保留全部守卫直至确认。
    /// manager/transaction/source/publication identify the same aborted original and its actual sealed Full publication.
    /// manager／transaction／source／publication 标识同一中止原请求及其实际封闭 Full 发布。
    /// registration must borrow this manager's predecessor gate; return a closed participant or an explicit rejection.
    /// registration 必须借用此管理器前驱门；返回关闭参与者或明确拒绝。
    pub fn prepare_handoff<'gate>(
        &'gate self,
        manager: &VulcanPluginRuntimeManager,
        transaction: &VulcanPluginActivationTransaction,
        source: &Arc<VulcanPluginPredecessorRestorationSource>,
        publication: &VulcanPluginActivationPredecessorPublication,
        registration: &'gate VulcanPluginActivationPredecessorRegistration<'gate>,
    ) -> Result<PredecessorPluginMcpHandoffGuard<'gate>, String> {
        if !registration.matches_manager(manager) {
            return Err(String::from(
                "Predecessor MCP completion registration belongs to another manager.",
            ));
        }
        // The private mechanical guard accepts no source or completion token and preserves all original native locks.
        // 私有机械守卫不接收来源或完成令牌，并保留全部原原生锁。
        let transfer = self
            .prepared
            .prepare_transfer(self.predecessor.plugin_id())?;
        // Validate predecessor profile allocation explicitly against each same actual guarded launch input and transport.
        // 根据每个相同实际受守卫启动输入及传输，显式校验前驱画像分配。
        transfer.validate_actual_inputs(|transport, actual| {
            actual
                .with_predecessor_source(transport, &self.profile, |_, _| ())
                .map_err(|error| {
                    format!("Predecessor MCP original launch source is unavailable during handoff: {error:?}.")
                })
        })?;
        // The exact original runtime and lease owners supply IDs independently of historical diagnostics.
        // 精确原运行时及租约所有者独立于历史诊断提供身份。
        let (runtime_instance_id, partition_lease_instance_id) = transfer.actual_instance_ids();
        // Core alone checks this same source, sealed publication, first issuer and dedicated borrowed registration.
        // 仅核心检查此同一来源、封闭发布、首次签发者及专属借用注册。
        let owner = self
            .predecessor
            .prepare_handoff_with_registration(
                manager,
                transaction,
                source,
                publication,
                &self.authority,
                runtime_instance_id,
                partition_lease_instance_id,
                registration,
            )
            .map_err(|error| error.to_string())?;
        Ok(PredecessorPluginMcpHandoffGuard {
            _registration: registration,
            owner,
            transfer,
        })
    }

    /// Request permanent retirement of this same original runtime; no input or replay authorization.
    /// 请求此同一原运行时永久退役；无输入或重放授权。
    /// Actual protocol shutdown and physical resource release remain separately observable.
    /// 实际协议关闭及物理资源释放保持独立可观察。
    pub fn shutdown(&self) {
        self.prepared.observation.runtime.shutdown();
    }
}

impl VulcanPluginNativeMcpPredecessorHandoffAdapter for PredecessorPluginMcpRuntime {
    /// Each actual native participant borrows the same adapter and dedicated registration for exactly one completion scope.
    /// 每个实际原生参与者为精确单个完成作用域借用相同适配器及专属注册。
    type Guard<'gate>
        = PredecessorPluginMcpHandoffGuard<'gate>
    where
        Self: 'gate;

    /// Borrow the original explicit native issuer; no input, replacement authority or physical owner is created.
    /// 借用原显式原生签发者；无输入，不创建替代权威或物理所有者。
    fn authority(&self) -> Option<&VulcanPluginRegisteredNativeMcpHostAuthority> {
        Some(&self.authority)
    }

    /// Prepare same-original actual guards under registration and return an explicit source or native validation error.
    /// 在 registration 下准备同原实际守卫，并返回明确来源或原生校验错误。
    /// manager/transaction/source/publication identify the original Full boundary; registration limits the returned lifetime.
    /// manager／transaction／source／publication 标识原 Full 边界；registration 限制返回生命周期。
    fn prepare<'gate>(
        &'gate self,
        manager: &VulcanPluginRuntimeManager,
        transaction: &VulcanPluginActivationTransaction,
        source: &Arc<VulcanPluginPredecessorRestorationSource>,
        publication: &VulcanPluginActivationPredecessorPublication,
        registration: &'gate VulcanPluginActivationPredecessorRegistration<'gate>,
    ) -> Result<Self::Guard<'gate>, vulcan_plugin_runtime::VulcanPluginRuntimeError> {
        self.prepare_handoff(manager, transaction, source, publication, registration)
            .map_err(vulcan_plugin_runtime::VulcanPluginRuntimeError::new)
    }
}

impl VulcanPluginActivationPredecessorHandoffParticipant for PredecessorPluginMcpHandoffGuard<'_> {
    /// Borrow exact sealed predecessor evidence while all original core and actual native owners remain held.
    /// 全部原核心及实际原生所有者仍保持持有时，借用精确封闭前驱证据。
    /// No inputs; return the real successful runtime/lease evidence accepted before durable transfer.
    /// 无输入；返回持久转交前接纳的真实成功运行时／租约证据。
    fn mcp_evidence(&self) -> Option<&VulcanPluginMcpPredecessorHandoffEvidence> {
        Some(self.owner.evidence())
    }

    /// Open the same original native lease only after its dedicated core owner accepts exact transferred confirmation.
    /// 仅其专属核心所有者接纳精确已转交确认后，开放相同原原生租约。
    /// Foreign confirmation grants no admission; matching confirmation performs no I/O or fallible work.
    /// 外来 confirmation 不授予入场；匹配 confirmation 不执行 I/O 或可失败工作。
    fn confirm_transferred(&mut self, confirmation: &VulcanPluginActivationPredecessorCompletion) {
        if !self.owner.confirm_transferred(confirmation) {
            return;
        }
        self.transfer.confirm_after_source();
    }
}
