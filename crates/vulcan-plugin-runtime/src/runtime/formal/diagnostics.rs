//! Read-only retained ownership sampled from the existing bounded registries.
//! 从既有有界注册表采样的只读保留所有权。

use super::*;
use luaskills::runtime::embedded::{
    EmbeddedErrorCode, EmbeddedSessionPhase, OperationOutcome, OperationPhase, OperationSnapshot,
};

/// Borrow mutex metadata only if immediately available; return fixed Busy or poison diagnostics without waiting.
/// 仅在立即可用时借用 mutex 元数据；不等待，返回固定忙碌或中毒诊断。
/// The returned guard protects the original record for a synchronous read and never crosses await.
/// 返回守卫为同步读取保护原记录，绝不跨越 await。
pub(in crate::runtime::formal) fn diagnostic_lock<T>(
    mutex: &Mutex<T>,
) -> Result<std::sync::MutexGuard<'_, T>, VulcanPluginRuntimeError> {
    mutex.try_lock().map_err(|error| match error {
        std::sync::TryLockError::WouldBlock => {
            VulcanPluginRuntimeError::new("Plugin resource diagnostics are busy.")
        }
        std::sync::TryLockError::Poisoned(_) => {
            VulcanPluginRuntimeError::new("Plugin resource diagnostic evidence is poisoned.")
        }
    })
}

/// Exact host registry category; native sessions used by internal calls remain distinct from pinned tools.
/// 精确宿主注册表类别；内部调用使用的原生会话与固定工具会话保持区分。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VulcanPluginResourceKind {
    /// Public owner-bound fixed tool session.
    /// 公开的所有者绑定固定工具会话。
    PinnedSession,
    /// Ordinary or internal one-shot formal invocation.
    /// 普通或内部单次正式调用。
    FormalCall,
    /// Reusable tool domain retaining its original pool and borrows.
    /// 保留原池及借用的可复用工具域。
    ReusableDomain,
    /// Persistent plugin Hook instance.
    /// 持久插件钩子实例。
    HookRuntime,
    /// Persistent plugin UI Action instance.
    /// 持久插件界面操作实例。
    UiActionRuntime,
}

/// Error existence and source stage without potentially business-bearing messages.
/// 错误存在性及来源阶段，不包含可能携带业务值的文案。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VulcanPluginResourceErrorDiagnostic {
    /// Exact retained failure stage, never parsed from error text.
    /// 精确保留失败阶段，绝不从错误文案解析。
    pub stage: String,
    /// Native structured classification; absent for host errors that only retain a message.
    /// 原生结构化分类；仅保留文案的宿主错误省略此值。
    pub code: Option<EmbeddedErrorCode>,
}

/// Whitelisted operation metadata excludes values, finalization results, context and host effects.
/// 白名单操作元数据排除业务值、关闭结果、上下文及宿主副作用。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VulcanPluginResourceOperationDiagnostic {
    /// Original core-issued operation identity.
    /// 原核心签发操作身份。
    pub operation_id: String,
    /// Actual sampled native phase.
    /// 实际采样的原生阶段。
    pub phase: OperationPhase,
    /// Cooperative cancellation intent does not prove retirement.
    /// 协作取消意图不证明退役。
    pub cancellation_requested: bool,
    /// Failure existence and structured codes from independent native stages.
    /// 独立原生阶段的失败存在性及结构化错误码。
    pub errors: Vec<VulcanPluginResourceErrorDiagnostic>,
}

/// One retained original resource identity, including physical ownership and safe lifecycle metadata.
/// 单个保留原资源身份，包含物理所有权及安全生命周期元数据。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VulcanPluginResourceDiagnostic {
    /// The actual registry determines this category.
    /// 实际注册表决定此类别。
    pub resource_kind: VulcanPluginResourceKind,
    /// Registry-issued identity; persistent registries use their actual plugin key.
    /// 注册表签发身份；持久注册表使用其实际插件键。
    pub resource_id: String,
    /// Original descriptor or domain-key generation, encoded without JavaScript rounding.
    /// 原描述符或域键代次，编码时避免 JavaScript 舍入。
    pub plugin_generation: String,
    /// Host controller identity; internal calls and persistent instances have no conversation owner.
    /// 宿主控制器身份；内部调用及持久实例没有会话所有者。
    pub owner_session_id: Option<String>,
    /// Original registered pool identity survives physical forgetting when its host record retains it.
    /// 宿主记录保留时，原注册池身份在物理遗忘后存活。
    pub original_pool_id: Option<String>,
    /// Original core session identity; never substituted with a pool or VM identity.
    /// 原核心会话身份；绝不以池或 VM 身份替代。
    pub core_session_id: Option<String>,
    /// Native session phase is absent when no live native session record remains.
    /// 不再存在实时原生会话记录时省略原生阶段。
    pub core_phase: Option<EmbeddedSessionPhase>,
    /// Current native session operation, absent for idle sessions or non-session resources.
    /// 当前原生会话操作；闲置会话或非会话资源省略。
    pub active_operation_id: Option<String>,
    /// Independent closing operation or retained closing receipt identity.
    /// 独立关闭操作或保留关闭回执身份。
    pub finalization_operation_id: Option<String>,
    /// Actual host preparation ownership.
    /// 实际宿主准备所有权。
    pub preparing: bool,
    /// Host closure or cancellation intent.
    /// 宿主关闭或取消意图。
    pub closing: bool,
    /// Actual recorded physical retirement checkpoint.
    /// 实际记录的物理退役检查点。
    pub retired: bool,
    /// Physical work remains selected by plugin drain; this does not imply active business execution.
    /// 物理工作仍被插件排空选中；此值不表示正在执行业务。
    pub is_drain_blocking: bool,
    /// Original supervision ended while physical retirement remains incomplete.
    /// 物理退役尚未完成时原监督已经结束。
    pub cleanup_recovery: bool,
    /// Native session queue size; absent for resources without a live session snapshot.
    /// 原生会话队列长度；无实时会话快照的资源省略。
    pub queued_calls: Option<usize>,
    /// Actual reusable-domain host borrows, absent for other registry categories.
    /// 实际复用域宿主借用数；其他注册表类别省略。
    pub borrowers: Option<usize>,
    /// Independent reusable finalizations remain pending.
    /// 独立复用关闭仍待完成。
    pub pending_finalizations: bool,
    /// Original retained independent shutdown operation identities for reusable-domain failures.
    /// 复用域失败的原保留独立关闭操作身份。
    pub finalization_operation_ids: Vec<String>,
    /// Safe original operation observations without business payloads.
    /// 不包含业务载荷的安全原操作观测。
    pub operations: Vec<VulcanPluginResourceOperationDiagnostic>,
    /// Host or session failure metadata excludes all raw messages.
    /// 宿主或会话失败元数据排除所有原始文案。
    pub errors: Vec<VulcanPluginResourceErrorDiagnostic>,
}

/// Plugin ownership observations are bounded by existing admission and retained-record limits.
/// 插件所有权观测由既有准入及保留记录上限约束。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VulcanPluginResourceDiagnostics {
    /// Exact attached native parent identity.
    /// 精确附加原生父级身份。
    pub runtime_id: String,
    /// Manager incarnation owning all sampled registries.
    /// 拥有全部采样注册表的管理器实例。
    pub service_epoch: String,
    /// Exact requested plugin namespace.
    /// 精确请求的插件命名空间。
    pub plugin_id: String,
    /// Explicit generation filter; null observes all retained generations of this plugin.
    /// 明确代次过滤器；空值观测此插件全部保留代次。
    pub plugin_generation: Option<String>,
    /// Registry and core observations are individually synchronized, not one atomic multi-map snapshot.
    /// 注册表及核心观测各自同步，并非单个跨映射原子快照。
    pub non_atomic_snapshot: bool,
    /// Only physically unretired entries from the host pinned-tool registry contribute.
    /// 仅宿主固定工具注册表中尚未物理退役的条目参与计数。
    pub pinned_session_count: usize,
    /// Actual retained records, including clearly marked nonblocking terminal history.
    /// 实际保留记录，包含明确标注不阻塞的终态历史。
    pub resources: Vec<VulcanPluginResourceDiagnostic>,
}

impl VulcanPluginRuntimeManager {
    /// Observe plugin_id, optionally restricted to its exact generation, without allocating runtime or execution.
    /// 观测 plugin_id，可选限定其精确 generation，不分配运行时或执行。
    /// Return None before parent attachment, otherwise existing retained ownership or an explicit observation error.
    /// 父级附加前返回空值，否则返回既有保留所有权或明确观测错误。
    pub fn formal_plugin_resource_diagnostics(
        &self,
        plugin_id: &str,
        plugin_generation: Option<u64>,
    ) -> Result<Option<VulcanPluginResourceDiagnostics>, VulcanPluginRuntimeError> {
        // Clone the existing owner; this read never initializes a native parent.
        // 克隆既有所有者；此读取绝不初始化原生父级。
        let Some(owner) = self.formal_runtime_owner()? else {
            return Ok(None);
        };
        // Each helper releases its registry before observing individual lifecycle or native records.
        // 每个辅助方法在观测独立生命周期或原生记录前释放其注册表。
        let mut resources = pinned::diagnostics::sample(&owner, plugin_id, plugin_generation)?;
        resources.extend(calls::diagnostics::sample(
            &owner,
            plugin_id,
            plugin_generation,
        )?);
        resources.extend(calls::reuse::diagnostics::sample(
            &owner,
            plugin_id,
            plugin_generation,
        )?);
        resources.extend(instances::diagnostics::sample(
            &owner,
            plugin_id,
            plugin_generation,
        )?);
        // A session's retired checkpoint, never mixed active-session accounting, defines this count.
        // 会话退役检查点定义此计数，绝不使用混合活动会话记账。
        let pinned_session_count = resources
            .iter()
            .filter(|resource| {
                resource.resource_kind == VulcanPluginResourceKind::PinnedSession
                    && !resource.retired
            })
            .count();
        Ok(Some(VulcanPluginResourceDiagnostics {
            runtime_id: owner.host.runtime().id().to_string(),
            service_epoch: self.state.manager_id.clone(),
            plugin_id: plugin_id.to_owned(),
            plugin_generation: plugin_generation.map(|generation| generation.to_string()),
            non_atomic_snapshot: true,
            pinned_session_count,
            resources,
        }))
    }
}

impl VulcanPluginResourceDiagnostic {
    /// Initialize safe absence for kind/id/generation/owner; caller fills only facts from that registry.
    /// 为 kind/id/generation/owner 初始化安全缺失值；调用方仅填充该注册表事实。
    /// Return metadata without probing another identity, inferring a VM or executing a lifecycle action.
    /// 返回元数据，不探测其他身份、不推断 VM，也不执行生命周期动作。
    pub(in crate::runtime::formal) fn new(
        resource_kind: VulcanPluginResourceKind,
        resource_id: String,
        generation: u64,
        owner_session_id: Option<String>,
    ) -> Self {
        Self {
            resource_kind,
            resource_id,
            plugin_generation: generation.to_string(),
            owner_session_id,
            original_pool_id: None,
            core_session_id: None,
            core_phase: None,
            active_operation_id: None,
            finalization_operation_id: None,
            preparing: false,
            closing: false,
            retired: false,
            is_drain_blocking: false,
            cleanup_recovery: false,
            queued_calls: None,
            borrowers: None,
            pending_finalizations: false,
            finalization_operation_ids: Vec::new(),
            operations: Vec::new(),
            errors: Vec::new(),
        }
    }

    /// Copy session metadata from core into this row, preserving identity and structured errors only.
    /// 将 core 会话元数据复制至此行，仅保留身份及结构化错误。
    /// Return no value; callers retain their host evidence lock while acquiring this synchronous core snapshot.
    /// 不返回值；调用方获取同步核心快照期间保留其宿主证据锁。
    pub(in crate::runtime::formal) fn session(
        &mut self,
        core: luaskills::runtime::embedded::EmbeddedSessionSnapshot,
    ) {
        self.core_phase = Some(core.phase);
        self.active_operation_id = core.active_operation;
        self.finalization_operation_id = core.finalization_operation;
        self.queued_calls = Some(core.queued_calls);
        if let Some(error) = core.error {
            self.errors.push(VulcanPluginResourceErrorDiagnostic::new(
                "session",
                Some(error.code),
            ));
        }
    }
}

impl VulcanPluginResourceErrorDiagnostic {
    /// Retain stage and optional native code, returning a safe marker without the original error message.
    /// 保留 stage 及可选原生 code，返回不含原错误文案的安全标记。
    pub(in crate::runtime::formal) fn new(stage: &str, code: Option<EmbeddedErrorCode>) -> Self {
        Self {
            stage: stage.to_owned(),
            code,
        }
    }
}

/// Project snapshot's exact operation identity and phase; return safe errors from each independent stage.
/// 投影 snapshot 精确操作身份及阶段；返回每个独立阶段的安全错误。
/// Raw values, messages, context and host effects are deliberately never serialized by this diagnostic type.
/// 此诊断类型明确绝不序列化原始值、文案、上下文及宿主副作用。
pub(in crate::runtime::formal) fn operation(
    snapshot: &OperationSnapshot,
) -> VulcanPluginResourceOperationDiagnostic {
    // Errors preserve their original source stage without cloning application values.
    // 错误保留原来源阶段，不克隆应用值。
    let mut errors = Vec::new();
    if let Some(error) = &snapshot.error {
        errors.push(VulcanPluginResourceErrorDiagnostic::new(
            "operation",
            Some(error.code),
        ));
    }
    if let Some(finalization) = &snapshot.finalization {
        if let OperationOutcome::Failed { error } = &finalization.business {
            errors.push(VulcanPluginResourceErrorDiagnostic::new(
                "business",
                Some(error.code),
            ));
        }
        if let Some(OperationOutcome::Failed { error }) = &finalization.outcome {
            errors.push(VulcanPluginResourceErrorDiagnostic::new(
                "finalization",
                Some(error.code),
            ));
        }
    }
    VulcanPluginResourceOperationDiagnostic {
        operation_id: snapshot.operation_id.clone(),
        phase: snapshot.phase,
        cancellation_requested: snapshot.cancellation_requested,
        errors,
    }
}
