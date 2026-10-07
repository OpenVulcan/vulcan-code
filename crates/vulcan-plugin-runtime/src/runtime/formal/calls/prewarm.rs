//! Automatic reusable-tool initialization targets under existing controller and module ownership.
//! 既有控制器及模块归属下的自动可复用工具初始化目标。

use super::*;
use luaskills::runtime::embedded::EmbeddedReusablePoolSnapshot;
use serde::Deserialize;

pub(super) mod automatic;
mod target;
pub use automatic::{
    VulcanPluginAutomaticPrewarmOrigin, VulcanPluginAutomaticPrewarmPermit,
    VulcanPluginAutomaticPrewarmTrigger,
};
pub(super) use target::TargetProgress;

/// Trusted explicit target selection; every field is frozen before any initialization can run.
/// 可信明确目标选择；任何初始化运行前冻结全部字段。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VulcanPluginPrewarmTargetRequest {
    /// Observed original parent; a replacement runtime cannot accept this request.
    /// 已观测原父级；替代运行时不能接受此请求。
    pub runtime_id: String,
    /// Exact enabled plugin owning the selected tool.
    /// 拥有所选工具的精确已启用插件。
    pub plugin_id: String,
    /// Exact enabled catalog tool identifier.
    /// 精确已启用目录工具标识。
    pub tool_id: String,
    /// Observed plugin-local routing generation.
    /// 已观测插件内路由代次。
    pub plugin_generation: u64,
    /// Trusted controller owning this receipt and its later business partition.
    /// 拥有此回执及其后续业务分区的可信控制器。
    pub owner_session_id: String,
    /// Workspace authorized by the host, never selected by plugin arguments.
    /// 宿主授权工作区，绝不由插件参数选择。
    pub workspace_root: PathBuf,
    /// Desired confirmed idle count at one observation; zero never initializes a VM.
    /// 单次观测期望的已确认空闲数；零绝不初始化 VM。
    pub target_idle: usize,
}

/// Original target and bounded operation history retained until explicit acknowledgement.
/// 原目标及有界操作历史，保留至显式确认。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VulcanPluginPrewarmTargetStatus {
    /// Original trusted request; no live routing values are substituted.
    /// 原可信请求；不替换为活动路由值。
    pub request: VulcanPluginPrewarmTargetRequest,
    /// Trusted automatic trigger provenance that admitted this target.
    /// 接纳此目标的可信自动触发来源。
    pub automatic: VulcanPluginAutomaticPrewarmOrigin,
    /// True only after the original input was transferred to its one supervisor.
    /// 仅在原输入转移至唯一监督者后为真。
    pub started: bool,
    /// Earlier confirmed operations; the current operation stays in the enclosing call status.
    /// 先前已确认操作；当前操作继续位于外层调用状态。
    pub completed: Vec<OperationSnapshot>,
    /// Scheduler observation that satisfied the target, not a permanent residency guarantee.
    /// 满足目标的调度器观测，不是永久驻留保证。
    pub satisfied: Option<EmbeddedReusablePoolSnapshot>,
}

impl VulcanPluginRuntimeManager {
    /// Start the existing id owned by owner_session_id exactly once; return its current evidence.
    /// 将 owner_session_id 拥有的既有 id 启动恰好一次；返回当前证据。
    /// Unknown or forgotten IDs fail and never recreate initialization.
    /// 未知或已遗忘标识失败，绝不重新创建初始化。
    pub fn start_reusable_tool_prewarm(
        &self,
        id: &str,
        owner_session_id: &str,
    ) -> Result<VulcanPluginFormalCallStatus, VulcanPluginRuntimeError> {
        // Exact registry ownership prevents a retry from changing the target or execution domain.
        // 精确注册表归属防止重试改变目标或执行域。
        let owner = self.required_formal_owner()?;
        // Resolve only the retained invocation owned by the trusted controller.
        // 仅解析可信控制器拥有的保留调用。
        let call = owned_call(&owner, id, owner_session_id)?;
        if call.lock()?.prewarm.is_none() {
            return Err(VulcanPluginRuntimeError::new(
                "Formal invocation is not a prewarm target.",
            ));
        }
        self.start_registered_formal_call(&owner, &call)?;
        call.status()
    }

    /// Resolve request against one enabled catalog and owner for bounded convergence admitted by automatic.
    /// 按单个已启用目录及 owner 解析 request，用于由 automatic 接纳的有界收敛。
    /// Return the original descriptor and deadline-bearing preparation, without creating a VM.
    /// 返回原描述符及携带截止时间的准备输入，不创建 VM。
    fn reusable_prewarm_input(
        &self,
        owner: &ManagedFormalRuntime,
        request: VulcanPluginPrewarmTargetRequest,
        automatic: automatic::AutomaticAdmission,
    ) -> Result<(VulcanPluginDescriptor, Preparation), VulcanPluginRuntimeError> {
        if owner.host.runtime().id() != request.runtime_id {
            return Err(VulcanPluginRuntimeError::new(
                "Prewarm runtime identity changed before admission.",
            ));
        }
        if request.owner_session_id.trim().is_empty() {
            return Err(VulcanPluginRuntimeError::new(
                "Prewarming requires a controller owner.",
            ));
        }
        // This snapshot owns both the declaration and plugin-local generation.
        // 此快照同时拥有声明及插件内代次。
        let snapshot = self.snapshot();
        // Resolve the declared tool from the same immutable enabled catalog.
        // 从同一不可变已启用目录解析声明工具。
        let tool = snapshot
            .enabled_tools
            .get(&request.tool_id)
            .ok_or_else(|| {
                VulcanPluginRuntimeError::new("Reusable prewarm tool is not enabled.")
            })?;
        if tool.plugin_id != request.plugin_id
            || tool.plugin_generation != request.plugin_generation
        {
            return Err(VulcanPluginRuntimeError::new(
                "Reusable prewarm plugin identity or generation changed before admission.",
            ));
        }
        // The validated manifest is the sole source of the tool's resident ceiling.
        // 已校验清单是工具常驻上限的唯一来源。
        let reuse = tool.manifest.execution.reusable.as_ref().ok_or_else(|| {
            VulcanPluginRuntimeError::new("Prewarming requires a declared reusable tool.")
        })?;
        if request.target_idle > reuse.max_resident_vms {
            return Err(VulcanPluginRuntimeError::new(
                "Prewarm target exceeds the declared resident limit.",
            ));
        }
        // The ordinary tool resolver remains the sole default and grace authority.
        // 普通工具解析器仍是唯一默认值及宽限权威。
        let timeout_ms = crate::runtime::sessions::resolve_host_timeout(tool, &Map::new())?;
        // Retain one absolute deadline across preparation, queueing and all initialization attempts.
        // 在准备、排队及全部初始化尝试期间保留一个绝对截止时间。
        let deadline = super::super::pinned::deadline(timeout_ms)?;
        // Pin the descriptor belonging to this exact enabled tool observation.
        // 固定属于此精确已启用工具观测的描述符。
        let plugin = snapshot
            .plugins
            .get(&tool.plugin_id)
            .cloned()
            .ok_or_else(|| VulcanPluginRuntimeError::new("Reusable prewarm plugin is missing."))?;
        // Keep the original package's validated finalizer declaration.
        // 保留原包已校验的关闭器声明。
        let manifest = plugin.manifest.as_ref().ok_or_else(|| {
            VulcanPluginRuntimeError::new("Reusable prewarm manifest is missing.")
        })?;
        // Use the sole Lua runtime declaration without probing substitute exports.
        // 使用唯一 Lua 运行时声明，不探测替代导出。
        let runtime = crate::runtime::lease::required_lua_runtime(manifest)?;
        // The original finalizer is required even when no business export ever runs.
        // 即使业务导出从未运行，仍要求原关闭器。
        let input = Preparation {
            workspace: CallWorkspace::Provided(request.workspace_root.clone()),
            arguments: Value::Null,
            finalizer: Some(ModuleFinalizer {
                export: runtime.shutdown_function.clone(),
                arguments: self.formal_envelope(
                    plugin_payload("shutdown", Map::new())?,
                    owner.settings.shutdown_timeout_ms,
                )?,
                timeout_ms: owner.settings.shutdown_timeout_ms,
            }),
            security_partition: format!("tool:{}", request.owner_session_id),
            admission: CallAdmission::PrewarmTarget { request, automatic },
            scope: VulcanPluginBridgeLeaseScope::Enabled,
            deadline,
        };
        Ok((plugin, input))
    }
}
