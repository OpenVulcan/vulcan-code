//! Explicit reusable-tool initialization under existing controller and module ownership.
//! 既有控制器及模块归属下的明确可复用工具初始化。

use super::*;
use luaskills::runtime::embedded::EmbeddedReusablePoolSnapshot;
use serde::Deserialize;

pub(super) mod automatic;
mod target;
pub use automatic::VulcanPluginAutomaticPrewarmOrigin;
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
    /// Trusted automatic trigger provenance; manual targets omit this metadata.
    /// 可信自动触发来源；手动目标省略此元数据。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub automatic: Option<VulcanPluginAutomaticPrewarmOrigin>,
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
    /// Register request and input budgets without running Lua or spawning work; return a queryable receipt.
    /// 登记 request 及输入预算，不运行 Lua 或启动工作；返回可查询回执。
    /// Repeating preparation creates another inert receipt; start only the selected original invocation ID.
    /// 重复准备会创建另一份未启动回执；仅启动选定的原调用标识。
    pub fn prepare_reusable_tool_prewarm(
        &self,
        request: VulcanPluginPrewarmTargetRequest,
    ) -> Result<VulcanPluginFormalCallStatus, VulcanPluginRuntimeError> {
        // Parent identity must be checked before retaining any controller work.
        // 保留任何控制器工作前必须检查父级身份。
        let owner = self.required_formal_owner()?;
        let (plugin, input) = self.reusable_prewarm_input(&owner, request, true)?;
        self.register_owned_formal_call(&owner, plugin, input)?
            .status()
    }

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

    /// Resume only retained cleanup for id under its original owner_session_id; never replay initialization.
    /// 仅在原 owner_session_id 下恢复 id 的保留清理；绝不重放初始化。
    /// Returns current original evidence; active or already retired receipts remain unchanged.
    /// 返回当前原始证据；活动或已经退役的回执保持不变。
    pub fn recover_reusable_tool_prewarm_cleanup(
        &self,
        id: &str,
        owner_session_id: &str,
    ) -> Result<VulcanPluginFormalCallStatus, VulcanPluginRuntimeError> {
        // Resolve the existing owner and exact controller receipt before inspecting its target kind.
        // 在检查目标类型前解析既有所有者及精确控制器回执。
        let owner = self.required_formal_owner()?;
        // Recovery cannot create or replace the retained invocation.
        // 恢复不能创建或替换保留调用。
        let call = owned_call(&owner, id, owner_session_id)?;
        // Capture only original supervision evidence, not inferred VM idleness.
        // 仅捕获原监督证据，不推断 VM 空闲状态。
        let status = call.status()?;
        if status.prewarm.is_none() {
            return Err(VulcanPluginRuntimeError::new(
                "Formal invocation is not a prewarm target.",
            ));
        }
        if status.requires_cleanup_recovery {
            // Reject missing executor before the cleanup helper transfers supervision ownership.
            // 在清理帮助函数转移监督归属前拒绝缺失执行器。
            tokio::runtime::Handle::try_current().map_err(|error| {
                VulcanPluginRuntimeError::new(format!(
                    "Prewarm cleanup requires an executor: {error}"
                ))
            })?;
            self.resume_formal_cleanup(&owner, &call)?;
        }
        call.status()
    }

    /// Resolve request against one enabled catalog and owner; target selects bounded convergence or one addition.
    /// 按单个已启用目录及 owner 解析 request；target 选择有界收敛或单次增加。
    /// Return the original descriptor and deadline-bearing preparation, without creating a VM.
    /// 返回原描述符及携带截止时间的准备输入，不创建 VM。
    fn reusable_prewarm_input(
        &self,
        owner: &ManagedFormalRuntime,
        request: VulcanPluginPrewarmTargetRequest,
        target: bool,
    ) -> Result<(VulcanPluginDescriptor, Preparation), VulcanPluginRuntimeError> {
        if owner.host.runtime().id() != request.runtime_id {
            return Err(VulcanPluginRuntimeError::new(
                "Prewarm runtime identity changed before admission.",
            ));
        }
        if request.owner_session_id.trim().is_empty() {
            return Err(VulcanPluginRuntimeError::new(
                "Explicit prewarming requires a controller owner.",
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
            VulcanPluginRuntimeError::new("Explicit prewarming requires a declared reusable tool.")
        })?;
        if target && request.target_idle > reuse.max_resident_vms {
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
            admission: if target {
                CallAdmission::PrewarmTarget {
                    request,
                    automatic: None,
                }
            } else {
                CallAdmission::Prewarm {
                    tool_id: request.tool_id,
                    owner_session_id: request.owner_session_id,
                }
            },
            scope: VulcanPluginBridgeLeaseScope::Enabled,
            deadline,
        };
        Ok((plugin, input))
    }
}

impl VulcanPluginRuntimeManager {
    /// Initialize one additional VM for plugin_id and its published tool_id at exactly plugin_generation.
    /// 为 plugin_id 及其已发布 tool_id 在精确 plugin_generation 下初始化一个额外 VM。
    /// The trusted caller supplies owner_session_id and workspace_root, authorizing normal module initialization capabilities.
    /// 可信调用方提供 owner_session_id 及 workspace_root，并授权普通模块初始化能力。
    /// Return the core instance_id object after confirmed initialization; admitted failures retain queryable formal call evidence.
    /// 确认初始化后返回核心 instance_id 对象；已入场失败保留可查询正式调用证据。
    /// This explicit operation may produce initialization effects, never invokes business, and never replenishes a target automatically.
    /// 此明确操作可能产生初始化副作用，绝不调用业务，也不自动补齐目标数量。
    pub async fn prewarm_reusable_tool_instance(
        &self,
        plugin_id: &str,
        tool_id: &str,
        plugin_generation: u64,
        owner_session_id: String,
        workspace_root: PathBuf,
    ) -> Result<Value, VulcanPluginRuntimeError> {
        // Share exact admission validation with retained targets without changing one-addition semantics.
        // 与保留目标共享精确入场校验，不改变单次增加语义。
        let owner = self.required_formal_owner()?;
        // This request selects the original attached parent and trusted controller workspace.
        // 此请求选择原附加父级及可信控制器工作区。
        let request = VulcanPluginPrewarmTargetRequest {
            runtime_id: owner.host.runtime().id().to_owned(),
            plugin_id: plugin_id.to_owned(),
            tool_id: tool_id.to_owned(),
            plugin_generation,
            owner_session_id,
            workspace_root,
            target_idle: 1,
        };
        let (plugin, input) = self.reusable_prewarm_input(&owner, request, false)?;
        self.invoke_owned_formal_call(owner, plugin, input).await
    }
}
