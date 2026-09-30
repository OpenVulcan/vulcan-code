//! Private candidate authority retains real readiness ownership without publishing a catalog generation.
//! 私有候选权威保留实际就绪所有权，不发布目录代次。

use super::*;
use crate::{VulcanPluginActivationCandidate, VulcanPluginDescriptor, VulcanPluginPackageOrigin};

mod mcp;
pub use mcp::VulcanPluginPreparedMcpCandidate;

/// Retains the original request only after confirming its catalog and unpublished package authority.
/// 确认目录及未发布插件包权威后，才保留原请求。
/// The state and inspected candidate must share one user root; returns the original exclusive file owner.
/// 状态与已检查候选必须共用单个用户根；返回原排他文件所有者。
pub(super) fn retain_candidate_readiness(
    state: &VulcanPluginManagerState,
    candidate: &VulcanPluginActivationCandidate<'_>,
) -> Result<Arc<std::fs::File>, VulcanPluginRuntimeError> {
    // Canonical spelling is fixed by the owning store, never inferred from another installed package.
    // 规范拼写由所属存储固定，绝不从另一已安装插件包推断。
    let user_root = state.user_plugin_root.canonicalize().map_err(|error| {
        VulcanPluginRuntimeError::new(format!("Failed to resolve candidate user catalog: {error}"))
    })?;
    if user_root != candidate.catalog_root() {
        return Err(VulcanPluginRuntimeError::new(
            "Activation candidate belongs to another user catalog.",
        ));
    }
    // A prepared view never grants a published generation or enabled execution.
    // 准备视图从不授予已发布代次或启用执行。
    let plugin = candidate.package();
    if plugin.generation != 0 || plugin.runtime_state != VulcanPluginRuntimeState::Disabled {
        return Err(VulcanPluginRuntimeError::new(
            "Activation candidate is not an unpublished disabled package.",
        ));
    }
    validate_candidate_namespace(state, &plugin.id)?;
    Ok(candidate.retain_readiness())
}

/// Rejects a user candidate that would shadow the manager's authoritative bundled namespace.
/// 拒绝会覆盖管理器权威内置命名空间的用户候选。
/// The state supplies current precedence and plugin_id is exact; returns validation without publishing.
/// 状态提供当前优先级，plugin_id 为精确身份；返回校验结果，不执行发布。
fn validate_candidate_namespace(
    state: &VulcanPluginManagerState,
    plugin_id: &str,
) -> Result<(), VulcanPluginRuntimeError> {
    // Recheck current bundled precedence even when an earlier candidate view remains retained.
    // 即使先前候选视图仍被保留，也复核当前内置优先级。
    let catalog = state
        .snapshot
        .read()
        .map_err(|_| VulcanPluginRuntimeError::new("Plugin catalog lock is poisoned."))?;
    if catalog
        .plugins
        .get(plugin_id)
        .is_some_and(|published| published.origin == VulcanPluginPackageOrigin::Bundled)
    {
        return Err(VulcanPluginRuntimeError::new(
            "An activation candidate cannot shadow a bundled plugin.",
        ));
    }
    Ok(())
}

/// The host chooses one exact unpublished operation; permissions cannot be widened by Lua arguments.
/// 宿主选择单个精确未发布操作；Lua 参数不能扩大权限。
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum CandidateOperation {
    /// Initialize without a business export under the enabling-probe capability scope.
    /// 在启用探测能力作用域中初始化，不调用业务导出。
    Probe,
    /// Invoke only the manifest's installation callback under the installation capability scope.
    /// 在安装能力作用域中仅调用清单安装回调。
    Install,
    /// Invoke the declared enable callback before route publication.
    /// 路由发布前调用声明的启用回调。
    Enable,
    /// Compensate candidate enable through its declared disable callback.
    /// 通过声明的停用回调补偿候选启用。
    EnableRollback,
}

impl CandidateOperation {
    /// Return this operation's sole native scope, including explicitly declared enable compensation.
    /// 返回此操作唯一原生作用域，包括显式声明的启用补偿。
    pub(super) fn scope(self) -> VulcanPluginBridgeLeaseScope {
        match self {
            Self::Probe => VulcanPluginBridgeLeaseScope::EnablingProbe,
            Self::Install => VulcanPluginBridgeLeaseScope::LifecycleInstall,
            Self::Enable => VulcanPluginBridgeLeaseScope::LifecycleEnable,
            Self::EnableRollback => VulcanPluginBridgeLeaseScope::LifecycleEnableRollback,
        }
    }
}

/// Host-selected old-package lifecycle operation fixes its native scope and durable effect.
/// 宿主选择的旧包生命周期操作固定其原生作用域及持久副作用。
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum PredecessorOperation {
    /// Disable the old package before any target effect.
    /// 在任何目标副作用之前停用旧包。
    Disable,
    /// Restore only the old package after observed effects and confirmed target compensation.
    /// 在副作用已观测且目标补偿已确认后，仅恢复旧包。
    Restore,
}

impl PredecessorOperation {
    /// Return the sole native scope; restore uses the original enable declaration.
    /// 返回唯一原生作用域；恢复使用原启用声明。
    pub(super) fn scope(self) -> VulcanPluginBridgeLeaseScope {
        match self {
            Self::Disable => VulcanPluginBridgeLeaseScope::LifecycleDisable,
            Self::Restore => VulcanPluginBridgeLeaseScope::LifecycleEnable,
        }
    }

    /// Return this operation's original-request receipt field, independently of the Lua event spelling.
    /// 返回此操作的原请求回执字段，独立于 Lua 事件拼写。
    pub(super) fn effect(self) -> crate::VulcanPluginActivationEffect {
        match self {
            Self::Disable => crate::VulcanPluginActivationEffect::DisablePredecessor,
            Self::Restore => crate::VulcanPluginActivationEffect::RestorePredecessor,
        }
    }
}

/// Explicit provenance fixes one private target operation or one exact old-package callback.
/// 显式来源固定单个私有目标操作或单个精确旧包回调。
#[derive(Clone)]
pub(super) enum ActivationOperation {
    /// The prepared target never acquires a public routing generation.
    /// 准备目标绝不取得公开路由代次。
    Candidate(CandidateOperation),
    /// Old execution uses one typed operation and its exact completed drain identity.
    /// 旧执行使用单个类型化操作及其精确已完成排空身份。
    Predecessor {
        /// Only the operation chooses native lifecycle scope.
        /// 仅操作选择原生生命周期作用域。
        operation: PredecessorOperation,
        /// The actual old package and current closed routing must match this identity.
        /// 实际旧包及当前关闭路由必须匹配此身份。
        identity: crate::VulcanPluginPredecessorDrainIdentity,
    },
}

impl ActivationOperation {
    /// Return the sole native scope for this explicit operation.
    /// 返回此显式操作唯一的原生作用域。
    pub(super) fn scope(&self) -> VulcanPluginBridgeLeaseScope {
        match self {
            Self::Candidate(operation) => operation.scope(),
            Self::Predecessor { operation, .. } => operation.scope(),
        }
    }

    /// Return true only for candidate work, preserving original candidate-query boundaries.
    /// 仅对候选工作返回 true，保留原候选查询边界。
    pub(super) fn is_candidate(&self) -> bool {
        matches!(self, Self::Candidate(_))
    }

    /// Return true only for private initialization that may share the original probe admission.
    /// 仅对可共享原探测入场的私有初始化返回 true。
    pub(super) fn is_candidate_probe(&self) -> bool {
        matches!(self, Self::Candidate(CandidateOperation::Probe))
    }
}

/// Exact original-request execution authority shared by tracked calls and actual native pools.
/// 由受跟踪调用及实际原生池共享的精确原请求执行权威。
pub(super) struct ActivationExecution {
    /// Frozen operation package carries its configuration, credentials, permissions and real lease.
    /// 固定操作插件包携带其配置、凭据、权限及实际租约。
    pub(super) plugin: VulcanPluginDescriptor,
    /// Original durable request selects the isolation partition without becoming an execution receipt.
    /// 原持久请求选择隔离分区，但不充当执行回执。
    pub(super) request_id: Uuid,
    /// The operation fixes its native scope for the entire original call and actual pool lifetime.
    /// 操作在整个原调用及实际池寿命内固定原生作用域。
    pub(super) operation: ActivationOperation,
    /// Only this manager may register or invoke native services for the original operation.
    /// 仅此管理器可以为原操作注册或调用原生服务。
    manager_id: String,
    /// The original exclusive OS request lock survives until actual work and retained calls are gone.
    /// 原排他系统请求锁保留至实际工作与保留调用全部退出。
    _readiness: Arc<std::fs::File>,
    /// Lazily acquired environment ownership survives every actual call and native pool reference.
    /// 延迟取得的环境所有权在每个实际调用及原生池引用期间存续。
    /// None means filesystem preparation never began; no-callback operations create no lease files.
    /// None 表示文件系统准备从未开始；无回调操作不创建租约文件。
    environment: std::sync::Mutex<Option<std::fs::File>>,
}

impl ActivationExecution {
    /// Retain candidate's original lock while selecting drain's frozen old package for operation.
    /// 保留 candidate 原锁，同时选择 drain 固定旧包用于 operation。
    /// state owns both identities; return authority only after confirmed old Lua retirement.
    /// state 拥有两个身份；仅在确认旧 Lua 退役后返回权威。
    pub(super) fn predecessor(
        state: &VulcanPluginManagerState,
        candidate: &VulcanPluginActivationCandidate<'_>,
        drain: &crate::VulcanPluginPredecessorDrain,
        operation: PredecessorOperation,
    ) -> Result<Arc<Self>, VulcanPluginRuntimeError> {
        // The same actual original lease protects both old and target package operations.
        // 相同实际原租约保护旧包及目标包操作。
        let readiness = retain_candidate_readiness(state, candidate)?;
        // A completed handle supplies exact old identity, not a new public catalog selection.
        // 已完成句柄提供精确旧身份，而非新的公开目录选择。
        let plugin = drain.predecessor_for_callback(state, candidate)?;
        // Operation identity cannot be chosen from Lua arguments or an arbitrary scope.
        // 操作身份不能从 Lua 参数或任意作用域选择。
        let authority = Arc::new(Self {
            plugin,
            request_id: candidate.transaction_record().request_id(),
            operation: ActivationOperation::Predecessor {
                operation,
                identity: drain.identity().clone(),
            },
            manager_id: state.manager_id.clone(),
            _readiness: readiness,
            environment: std::sync::Mutex::new(None),
        });
        authority.validate(state, &authority.plugin, authority.operation.scope())?;
        Ok(authority)
    }

    /// Retains the exact manager's execution environment before original-operation filesystem preparation.
    /// 在原操作文件系统准备之前，保留精确管理器的执行环境。
    /// State must own this authority; returns after acquiring or retaining the same actual shared file lock.
    /// state 必须拥有此权威；取得或保留同一实际共享文件锁后返回。
    /// Acquisition is lazy and monotonic so absent callbacks have no filesystem effects and clones share ownership.
    /// 获取为延迟且单向，使缺失回调没有文件系统影响，克隆共享所有权。
    pub(super) fn retain_execution_environment(
        &self,
        state: &VulcanPluginManagerState,
    ) -> Result<(), VulcanPluginRuntimeError> {
        self.validate(state, &self.plugin, self.operation.scope())?;
        // Serialize first acquisition; no observer can replace or release an established environment lease.
        // 串行化首次获取；任何观察者均不能替换或释放已建立的环境租约。
        let mut environment = self.environment.lock().map_err(|_| {
            VulcanPluginRuntimeError::new("Activation environment ownership lock is poisoned.")
        })?;
        if environment.is_none() {
            *environment = Some(
                super::environment::retain_plugin_execution_environment(state, &self.plugin.id)
                    .map_err(VulcanPluginRuntimeError::new)?,
            );
        }
        Ok(())
    }

    /// Bind candidate's exact target and operation to state and return private readiness authority.
    /// 将 candidate 精确目标及 operation 绑定至 state，并返回私有就绪权威。
    /// Reject another user catalog or a bundled namespace; no runtime or public descriptor is created.
    /// 拒绝其他用户目录或内置命名空间；不创建运行时或公开描述符。
    pub(super) fn new(
        state: &VulcanPluginManagerState,
        candidate: &VulcanPluginActivationCandidate<'_>,
        operation: CandidateOperation,
    ) -> Result<Arc<Self>, VulcanPluginRuntimeError> {
        // Native calls and MCP preparation share the same original request and catalog checks.
        // 原生调用与 MCP 准备共用相同原请求及目录校验。
        let readiness = retain_candidate_readiness(state, candidate)?;
        // Operation-specific authority still belongs exclusively to this formal runtime manager.
        // 操作专属权威仍仅属于此正式运行时管理器。
        let authority = Arc::new(Self {
            plugin: candidate.package().clone(),
            request_id: candidate.transaction_record().request_id(),
            operation: ActivationOperation::Candidate(operation),
            manager_id: state.manager_id.clone(),
            _readiness: readiness,
            environment: std::sync::Mutex::new(None),
        });
        authority.validate(state, &authority.plugin, operation.scope())?;
        Ok(authority)
    }

    /// Validate this manager, exact plugin and operation-specific scope before registration or native admission.
    /// 在注册或原生入场前校验此管理器、精确 plugin 及操作专属 scope。
    /// The OS lease prevents cooperating transaction writers from advancing this prepared authority.
    /// 系统租约阻止协作事务写入者推进此准备权威。
    pub(super) fn validate(
        &self,
        state: &VulcanPluginManagerState,
        plugin: &VulcanPluginDescriptor,
        scope: VulcanPluginBridgeLeaseScope,
    ) -> Result<(), VulcanPluginRuntimeError> {
        if self.manager_id != state.manager_id
            || scope != self.operation.scope()
            || plugin.id != self.plugin.id
            || plugin.package_root != self.plugin.package_root
            || plugin.content_fingerprint != self.plugin.content_fingerprint
            || plugin.version != self.plugin.version
            || plugin.generation != self.plugin.generation
            || plugin.runtime_state != self.plugin.runtime_state
        {
            return Err(VulcanPluginRuntimeError::new(
                "Activation authority does not match this exact operation.",
            ));
        }
        match &self.operation {
            ActivationOperation::Candidate(_) => {
                if plugin.generation != 0
                    || plugin.runtime_state != VulcanPluginRuntimeState::Disabled
                {
                    return Err(VulcanPluginRuntimeError::new(
                        "Candidate authority is not unpublished.",
                    ));
                }
                validate_candidate_namespace(state, &plugin.id)
            }
            ActivationOperation::Predecessor { identity, .. } => {
                super::update_drain::validate_predecessor_callback_current(state, plugin, identity)
            }
        }
    }
}
