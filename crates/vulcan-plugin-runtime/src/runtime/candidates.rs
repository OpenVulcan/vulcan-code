//! Private candidate authority retains real readiness ownership without publishing a catalog generation.
//! 私有候选权威保留实际就绪所有权，不发布目录代次。

use super::*;
use crate::{VulcanPluginActivationCandidate, VulcanPluginDescriptor, VulcanPluginPackageOrigin};

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
}

impl CandidateOperation {
    /// Return this operation's sole native scope; no published business or teardown scope is admitted.
    /// 返回此操作唯一原生作用域；不接纳已发布业务或拆除作用域。
    pub(super) fn scope(self) -> VulcanPluginBridgeLeaseScope {
        match self {
            Self::Probe => VulcanPluginBridgeLeaseScope::EnablingProbe,
            Self::Install => VulcanPluginBridgeLeaseScope::LifecycleInstall,
        }
    }
}

/// Exact host-only candidate authority shared by its tracked call and actual native pool.
/// 由受跟踪调用及实际原生池共享的精确宿主专属候选权威。
pub(super) struct CandidateExecution {
    /// Frozen target carries its selected configuration, credentials, permissions and real package lease.
    /// 固定目标携带选定配置、凭据、权限及实际插件包租约。
    pub(super) plugin: VulcanPluginDescriptor,
    /// Original durable request selects the isolation partition without becoming an execution receipt.
    /// 原持久请求选择隔离分区，但不充当执行回执。
    pub(super) request_id: Uuid,
    /// The operation fixes its native scope for the entire original call and actual pool lifetime.
    /// 操作在整个原调用及实际池寿命内固定原生作用域。
    pub(super) operation: CandidateOperation,
    /// Only this manager may register or invoke the candidate's native services.
    /// 仅此管理器可以注册或调用候选原生服务。
    manager_id: String,
    /// The original exclusive OS request lock survives until actual work and retained calls are gone.
    /// 原排他系统请求锁保留至实际工作与保留调用全部退出。
    _readiness: Arc<std::fs::File>,
}

impl CandidateExecution {
    /// Bind candidate's exact target and operation to state and return private readiness authority.
    /// 将 candidate 精确目标及 operation 绑定至 state，并返回私有就绪权威。
    /// Reject another user catalog or a bundled namespace; no runtime or public descriptor is created.
    /// 拒绝其他用户目录或内置命名空间；不创建运行时或公开描述符。
    pub(super) fn new(
        state: &VulcanPluginManagerState,
        candidate: &VulcanPluginActivationCandidate<'_>,
        operation: CandidateOperation,
    ) -> Result<Arc<Self>, VulcanPluginRuntimeError> {
        // Canonical spelling is fixed by the owning store and compared with the actual configured user root.
        // 规范拼写由所属存储固定，并与实际配置的用户根目录比较。
        let user_root = state.user_plugin_root.canonicalize().map_err(|error| {
            VulcanPluginRuntimeError::new(format!(
                "Failed to resolve candidate user catalog: {error}"
            ))
        })?;
        if user_root != candidate.catalog_root() {
            return Err(VulcanPluginRuntimeError::new(
                "Activation candidate belongs to another user catalog.",
            ));
        }
        // Ownership of the original exclusive lease travels with private execution, including detached cleanup.
        // 原排他租约所有权随私有执行传递，包含分离清理。
        let authority = Arc::new(Self {
            plugin: candidate.package().clone(),
            request_id: candidate.transaction_record().request_id(),
            operation,
            manager_id: state.manager_id.clone(),
            _readiness: candidate.retain_readiness(),
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
            || plugin.generation != 0
            || plugin.runtime_state != VulcanPluginRuntimeState::Disabled
        {
            return Err(VulcanPluginRuntimeError::new(
                "Candidate authority does not match this exact operation.",
            ));
        }
        // Bundled precedence remains authoritative even if a catalog refresh occurs during preparation.
        // 即使准备期间目录刷新，内置优先级仍具有权威。
        let catalog = state
            .snapshot
            .read()
            .map_err(|_| VulcanPluginRuntimeError::new("Plugin catalog lock is poisoned."))?;
        if catalog
            .plugins
            .get(&plugin.id)
            .is_some_and(|published| published.origin == VulcanPluginPackageOrigin::Bundled)
        {
            return Err(VulcanPluginRuntimeError::new(
                "An activation candidate cannot shadow a bundled plugin.",
            ));
        }
        Ok(())
    }
}
