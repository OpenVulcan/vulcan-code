//! Prewarm administration shares ordinary tool workspace authority and validates indexed ownership.
//! 预热管理共享普通工具工作区权威，并验证索引归属。

use super::run_plugin_runtime_control;
use crate::project_conversation_services::{
    SessionTurnAdmissionGuard, begin_session_turn_start_exclusion,
};
use crate::{
    SharedServiceBridgeState, session_runtime_registry_v1::SessionLifecycleTransitionGuard,
};
use luaskills::runtime::embedded::{EmbeddedReusablePoolSnapshot, OperationSnapshot};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use vulcan_plugin_runtime::{
    VulcanPluginFormalCallStatus, VulcanPluginPrewarmTargetRequest, VulcanPluginRuntimeError,
    VulcanPluginRuntimeManager,
};

/// Original runtime and controller select retained work even after the session leaves navigation.
/// 原运行时及控制器选择保留工作，即使会话已离开导航。
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PrewarmOwner {
    /// Exact native parent observed before submission.
    /// 提交前观测到的精确原生父级。
    pub(crate) runtime_id: String,
    /// Canonical service session identity; arbitrary strings are rejected during decoding.
    /// 规范服务会话身份；解码时拒绝任意字符串。
    pub(crate) owner_session_id: uuid::Uuid,
}

/// A selected original receipt never means the latest operation for a tool.
/// 选中的原回执绝不表示某工具的最新操作。
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PrewarmReceiptRequest {
    /// Exact native parent retained by the caller.
    /// 调用方保留的精确原生父级。
    pub(crate) runtime_id: String,
    /// Original trusted controller represented as a service UUID.
    /// 以服务 UUID 表示的原可信控制器。
    pub(crate) owner_session_id: uuid::Uuid,
    /// Exact host-issued invocation, including before native operation publication.
    /// 精确宿主签发调用，包含原生操作发布之前。
    pub(crate) invocation_id: String,
}

/// Browser preparation carries identities and intent, never an authoritative filesystem path.
/// 浏览器准备携带身份及意图，绝不携带权威文件系统路径。
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PrewarmPreparation {
    /// Exact native parent retained by the caller.
    /// 调用方保留的精确原生父级。
    pub(crate) runtime_id: String,
    /// Visible indexed session whose retained project owns the ordinary tool workspace.
    /// 保留项目拥有普通工具工作区的可见索引会话。
    pub(crate) owner_session_id: uuid::Uuid,
    /// Exact installed plugin selected from the prewarm catalog.
    /// 从预热目录选中的精确已安装插件。
    pub(crate) plugin_id: String,
    /// Exact enabled reusable tool selected from the same observation.
    /// 从同次观测选中的精确已启用复用工具。
    pub(crate) tool_id: String,
    /// Canonical decimal u64 avoids browser floating-point identity truncation.
    /// 规范十进制 u64 避免浏览器浮点身份截断。
    pub(crate) plugin_generation: String,
    /// Desired confirmed idle count, validated by the original tool declaration.
    /// 由原工具声明校验的期望已确认空闲数量。
    pub(crate) target_idle: usize,
}

/// Exact reusable tool observation; declaration limits remain owned by the plugin manifest.
/// 精确复用工具观测；声明上限仍由插件清单拥有。
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PrewarmTool {
    /// Owning plugin identity from the same enabled snapshot.
    /// 同一已启用快照中的所属插件身份。
    plugin_id: String,
    /// Stable globally unique tool identity.
    /// 全局唯一的稳定工具身份。
    tool_id: String,
    /// Original generation is serialized without numeric precision loss.
    /// 原始代次序列化时不丢失数值精度。
    plugin_generation: String,
    /// Authoritative declared upper bound, not available or reserved capacity.
    /// 权威声明上限，不表示可用或已预留容量。
    max_resident_vms: usize,
}

/// A management projection of one original target, including its unchanged native evidence.
/// 单个原目标的管理投影，包含其未改写原生证据。
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PrewarmReceipt {
    /// Original native parent identity.
    /// 原始原生父级身份。
    runtime_id: String,
    /// Host invocation retained through preparation, execution and cleanup.
    /// 在准备、执行及清理期间保留的宿主调用。
    invocation_id: String,
    /// Original plugin identity.
    /// 原始插件身份。
    plugin_id: String,
    /// Original reusable tool identity.
    /// 原始复用工具身份。
    tool_id: String,
    /// Exact original generation in lossless wire form.
    /// 以无损线协议形式表示的精确原始代次。
    plugin_generation: String,
    /// Controller identity frozen at preparation.
    /// 准备时冻结的控制器身份。
    owner_session_id: String,
    /// Workspace resolved by the host and retained by the original request.
    /// 由宿主解析并由原始请求保留的工作区。
    workspace_root: PathBuf,
    /// Original requested confirmed idle count.
    /// 原始请求的已确认空闲数量。
    target_idle: usize,
    /// Whether the original input has been transferred to its supervisor.
    /// 原始输入是否已转移给其监督者。
    started: bool,
    /// Actual request cleanup completion, independent of observer lifetime.
    /// 真实请求清理完成，与观测者寿命无关。
    retired: bool,
    /// Only retained interrupted cleanup may be explicitly recovered.
    /// 仅保留的中断清理可以被显式恢复。
    requires_cleanup_recovery: bool,
    /// A retained successful result is distinct from unfinished work or an error.
    /// 保留的成功结果与未完成工作或错误不同。
    succeeded: bool,
    /// Original host failure, never inferred from an HTTP observation failure.
    /// 原宿主失败，绝不从 HTTP 观测失败推断。
    error: Option<String>,
    /// Current original native operation, when admission has published one.
    /// 入场已发布时的当前原生操作。
    operation: Option<OperationSnapshot>,
    /// Earlier completed original operations retained for exact acknowledgement.
    /// 为精确确认而保留的较早已完成原始操作。
    completed: Vec<OperationSnapshot>,
    /// Scheduler confirmation at target completion, not a future residency guarantee.
    /// 目标完成时的调度器确认，不保证未来常驻。
    satisfied: Option<EmbeddedReusablePoolSnapshot>,
}

/// One parent-scoped read exposes enabled choices and only the selected controller's targets.
/// 单次父级范围读取暴露已启用选项及仅所选控制器的目标。
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PrewarmCatalog {
    /// Exact parent shared by every projected receipt.
    /// 每份投影回执共享的精确父级。
    runtime_id: String,
    /// Controller used for the ownership filter.
    /// 用于归属过滤的控制器。
    owner_session_id: String,
    /// Enabled reusable declarations from one immutable catalog.
    /// 来自单个不可变目录的已启用复用声明。
    tools: Vec<PrewarmTool>,
    /// Original retained prewarm receipts; ordinary business calls are excluded.
    /// 原始保留预热回执；排除普通业务调用。
    targets: Vec<PrewarmReceipt>,
}

/// Project status from the sole call ledger; reject ordinary calls instead of inventing a target.
/// 从唯一调用账本投影 status；拒绝普通调用，而非构造目标。
/// Returns lossless routing identities together with original native evidence.
/// 返回无损路由身份及原生证据。
fn receipt(
    status: VulcanPluginFormalCallStatus,
) -> Result<PrewarmReceipt, VulcanPluginRuntimeError> {
    // The retained prewarm branch is the sole request authority.
    // 保留的预热分支是唯一请求权威。
    let target = status
        .prewarm
        .ok_or_else(|| VulcanPluginRuntimeError::new("Invocation is not a prewarm target."))?;
    Ok(PrewarmReceipt {
        runtime_id: target.request.runtime_id,
        invocation_id: status.invocation_id,
        plugin_id: target.request.plugin_id,
        tool_id: target.request.tool_id,
        plugin_generation: target.request.plugin_generation.to_string(),
        owner_session_id: target.request.owner_session_id,
        workspace_root: target.request.workspace_root,
        target_idle: target.request.target_idle,
        started: target.started,
        retired: status.retired,
        requires_cleanup_recovery: status.requires_cleanup_recovery,
        succeeded: status.value.is_some(),
        error: status.error,
        operation: status.operation,
        completed: target.completed,
        satisfied: target.satisfied,
    })
}

/// Require the original parent of manager before reading or controlling retained work.
/// 在读取或控制保留工作前要求 manager 的原始父级。
/// Returns an explicit error for absent or changed ownership without attaching another parent.
/// 对缺失或改变的归属返回明确错误，不附加其他父级。
fn require_parent(
    manager: &VulcanPluginRuntimeManager,
    runtime_id: &str,
) -> Result<(), VulcanPluginRuntimeError> {
    if manager
        .formal_runtime_status()?
        .is_some_and(|status| status.runtime_id == runtime_id)
    {
        Ok(())
    } else {
        Err(VulcanPluginRuntimeError::new(
            "Prewarm runtime identity is unavailable or changed.",
        ))
    }
}

/// Retain both service admission and storage lifecycle ownership through actual submission.
/// 跨真实提交保留服务准入及存储生命周期归属。
struct PrewarmWorkspace {
    /// Ordinary tool workspace frozen before entering native control.
    /// 进入原生控制前冻结的普通工具工作区。
    path: PathBuf,
    /// Storage gate serializes exact index lookup and physical session deletion.
    /// 存储门将精确索引查询与物理会话删除串行化。
    _lifecycle: SessionLifecycleTransitionGuard,
    /// Outer admission excludes the whole deletion transaction, including its earlier plugin drain.
    /// 外层准入排除整个删除事务，包括其较早的插件排空。
    _admission: SessionTurnAdmissionGuard,
}

/// Resolve the exact indexed session under its existing lifecycle gate.
/// 在既有生命周期门下解析精确索引会话。
/// Returns the ordinary tool execution workspace and guards retained through actual submission.
/// 返回普通工具执行工作区及保留至真实提交完成的守卫。
async fn workspace(
    state: &SharedServiceBridgeState,
    owner: uuid::Uuid,
) -> Result<PrewarmWorkspace, String> {
    // Deletion drains plugins before taking the storage gate, so its outer admission must also be excluded.
    // 删除在取得存储门前先排空插件，因此还必须排除其外层准入。
    let admission = begin_session_turn_start_exclusion(state, [owner], "插件预热提交")?;
    // Capture existing owners without holding the bridge lock across asynchronous work.
    // 捕获既有所有者，不跨异步工作持有桥锁。
    let (controller, registry) = {
        let state = state.lock().map_err(|error| error.to_string())?;
        (
            state
                .controller_runtime
                .clone()
                .ok_or("Controller runtime is not ready.")?,
            state.session_runtime_registry.clone(),
        )
    };
    // Serialize index resolution and submission against session deletion or archival.
    // 将索引解析及提交与会话删除或归档串行化。
    let lifecycle = registry
        .acquire_lifecycle_transition(owner)
        .await
        .map_err(|error| error.to_string())?;
    // SQLite proves session existence; its storage directory is not the execution workspace.
    // SQLite 证明会话存在；其存储目录不是执行工作区。
    let locator = vulcan_workspace_store::workspace_store::locate_session_v1_storage(
        &controller,
        &owner.to_string(),
    )
    .await
    .map_err(|error| error.to_string())?;
    // Use the same retained session-to-project resolver that prepares ordinary plugin tools.
    // 使用准备普通插件工具时相同的保留会话到项目解析器。
    let path = {
        // Freeze the exact projection under the service lock while lifecycle admission remains held.
        // 生命周期准入仍被持有时，在服务锁下冻结精确投影。
        let state = state.lock().map_err(|error| error.to_string())?;
        // Visibility is required for new initialization, while old receipt cleanup remains independent.
        // 新初始化要求可见性，旧回执清理仍保持独立。
        let workspace = state
            .workspace_snapshot
            .as_ref()
            .ok_or("Workspace is not ready.")?;
        // A missing owner must not activate the ordinary projectless workspace policy.
        // 缺失所有者不得激活普通无项目工作区策略。
        let session = crate::workspace_session_summary_for_session(workspace, owner)
            .ok_or("Prewarm session is not active in the workspace.")?;
        if let Some(project_id) = crate::session_summary_project_id(session) {
            if project_id != locator.project_id() {
                return Err("Prewarm session project disagrees with its indexed owner.".into());
            }
            if crate::workspace_task_project_for_project_id(workspace, project_id).is_none() {
                return Err("Prewarm session's declared project is unavailable.".into());
            }
        }
        // Legitimately projectless sessions inherit only the existing ordinary tool policy.
        // 合法无项目会话仅继承既有普通工具策略。
        PathBuf::from(crate::workspace_path_for_session(workspace, owner))
    };
    Ok(PrewarmWorkspace {
        path,
        _lifecycle: lifecycle,
        _admission: admission,
    })
}

/// Read original parent and owner with no session loading, settings writes or Lua execution.
/// 读取原父级及所有者，不加载会话、不写设置、不执行 Lua。
/// Returns current reusable choices and retained original target receipts.
/// 返回当前复用选项及保留的原始目标回执。
pub(crate) async fn catalog(
    state: SharedServiceBridgeState,
    request: PrewarmOwner,
) -> Result<PrewarmCatalog, String> {
    run_plugin_runtime_control(state, move |manager| {
        require_parent(manager, &request.runtime_id)?;
        // One immutable catalog owns each tool's identity and declared limit.
        // 单个不可变目录拥有每个工具的身份及声明上限。
        let snapshot = manager.snapshot();
        // Project only explicitly reusable enabled tools, without inventing a pool.
        // 仅投影显式复用的已启用工具，不构造池。
        let tools = snapshot
            .enabled_tools
            .values()
            .filter_map(|tool| {
                tool.manifest
                    .execution
                    .reusable
                    .as_ref()
                    .map(|reuse| PrewarmTool {
                        plugin_id: tool.plugin_id.clone(),
                        tool_id: tool.id.clone(),
                        plugin_generation: tool.plugin_generation.to_string(),
                        max_resident_vms: reuse.max_resident_vms,
                    })
            })
            .collect();
        // The call registry already performs controller ownership filtering.
        // 调用注册表已经执行控制器归属过滤。
        let targets = manager
            .formal_calls(&request.owner_session_id.to_string())?
            .into_iter()
            .filter(|status| status.prewarm.is_some())
            .map(receipt)
            .collect::<Result<_, _>>()?;
        Ok(PrewarmCatalog {
            runtime_id: request.runtime_id,
            owner_session_id: request.owner_session_id.to_string(),
            tools,
            targets,
        })
    })
    .await
}

/// Prepare request under authoritative session ownership without executing Lua.
/// 在权威会话归属下准备 request，不执行 Lua。
/// Returns an inert receipt; uncertain HTTP completion must be recovered by listing original receipts.
/// 返回未启动回执；HTTP 完成情况未知时必须通过列出原回执恢复。
pub(crate) async fn prepare(
    state: SharedServiceBridgeState,
    request: PrewarmPreparation,
) -> Result<PrewarmReceipt, String> {
    // Exact decimal spelling prevents alternate browser representations from changing identity.
    // 精确十进制拼写防止浏览器其他表示改变身份。
    let generation = request
        .plugin_generation
        .parse::<u64>()
        .map_err(|error| error.to_string())?;
    if generation.to_string() != request.plugin_generation {
        return Err("Plugin generation must be a canonical decimal u64 string.".into());
    }
    // Transfer this lifecycle guard into admitted blocking work, even if its HTTP observer leaves.
    // 将此生命周期守卫转移给已接纳阻塞工作，即使 HTTP 观测者离开也保留。
    let workspace = workspace(&state, request.owner_session_id).await?;
    run_plugin_runtime_control(state, move |manager| {
        // Move the full guard into admitted work; retain it even after the observer is cancelled.
        // 将完整守卫移入已接纳工作；观测者取消后仍保留。
        let PrewarmWorkspace {
            path: workspace_root,
            _lifecycle,
            _admission,
        } = workspace;
        receipt(
            manager.prepare_reusable_tool_prewarm(VulcanPluginPrewarmTargetRequest {
                runtime_id: request.runtime_id,
                plugin_id: request.plugin_id,
                tool_id: request.tool_id,
                plugin_generation: generation,
                owner_session_id: request.owner_session_id.to_string(),
                workspace_root,
                target_idle: request.target_idle,
            })?,
        )
    })
    .await
}

/// Exact receipt actions separate observation, execution, cancellation and acknowledgement.
/// 精确回执动作区分观测、执行、取消及确认。
#[derive(Clone, Copy)]
pub(crate) enum PrewarmAction {
    /// Read retained evidence without advancing execution.
    /// 读取保留证据，不推进执行。
    Status,
    /// Transfer original input to its sole supervisor at most once.
    /// 至多一次将原始输入转移给其唯一监督者。
    Start,
    /// Request cooperative cancellation while retaining actual work.
    /// 请求协作取消，同时保留真实工作。
    Cancel,
    /// Resume only interrupted cleanup of the original target.
    /// 仅恢复原目标已中断的清理。
    Recover,
    /// Acknowledge a fully retired original receipt.
    /// 确认完整退役的原始回执。
    Forget,
}

/// Apply action to the original owned receipt; only start requires current indexed workspace authority.
/// 将 action 应用于原有归属回执；仅启动要求当前索引工作区权威。
/// Returns current evidence, or null only after successful explicit forgetting.
/// 返回当前证据，仅在明确遗忘成功后返回空值。
pub(crate) async fn control(
    state: SharedServiceBridgeState,
    request: PrewarmReceiptRequest,
    action: PrewarmAction,
) -> Result<Option<PrewarmReceipt>, String> {
    // Cleanup remains available after a deleted session, but new initialization needs live index authority.
    // 删除会话后仍可清理，但新初始化需要实时索引权威。
    let workspace = if matches!(action, PrewarmAction::Start) {
        Some(workspace(&state, request.owner_session_id).await?)
    } else {
        None
    };
    run_plugin_runtime_control(state, move |manager| {
        require_parent(manager, &request.runtime_id)?;
        // Canonical controller identity is identical across every original receipt operation.
        // 每项原回执操作使用相同的规范控制器身份。
        let owner = request.owner_session_id.to_string();
        // Validate target kind and its original parent before any destructive action.
        // 在任何破坏性动作前校验目标类型及其原父级。
        let original = receipt(manager.formal_call_status(&request.invocation_id, &owner)?)?;
        if original.runtime_id != request.runtime_id {
            return Err(VulcanPluginRuntimeError::new(
                "Prewarm receipt belongs to another runtime.",
            ));
        }
        // The captured gate is retained through actual start; a changed workspace never redirects old work.
        // 捕获的门保留至实际启动完成；变化的工作区绝不重定向旧工作。
        if let Some(workspace) = &workspace
            && workspace.path != original.workspace_root
        {
            return Err(VulcanPluginRuntimeError::new(
                "Prewarm workspace changed after preparation.",
            ));
        }
        match action {
            PrewarmAction::Status => return Ok(Some(original)),
            PrewarmAction::Start => {
                manager.start_reusable_tool_prewarm(&request.invocation_id, &owner)?;
            }
            PrewarmAction::Cancel => manager.cancel_formal_call(&request.invocation_id, &owner)?,
            PrewarmAction::Recover => {
                manager.recover_reusable_tool_prewarm_cleanup(&request.invocation_id, &owner)?;
            }
            PrewarmAction::Forget => {
                manager.forget_formal_call(&request.invocation_id, &owner)?;
                return Ok(None);
            }
        }
        receipt(manager.formal_call_status(&request.invocation_id, &owner)?).map(Some)
    })
    .await
}
