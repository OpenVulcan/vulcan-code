//! One pending-operation registry projects every interrupted plugin management operation and routes its continuation.
//! 单一待处理操作登记表投影所有中断的插件管理操作，并路由其继续处理。
//!
//! Each kind keeps its own durable owner (activation journals, live uninstall slots, policy receipts); this module is
//! the only listing and continuation surface, and each kind's offered actions come from one rule beside its record.
//! 每类操作保留各自的持久所有者（激活日志、存活卸载槽、策略回执）；本模块是唯一的列表与继续入口，
//! 每类可用动作均来自其记录旁的唯一规则。

use super::*;
use crate::plugin_runtime_services::run_plugin_runtime_control;
use vulcan_plugin_runtime::{
    VulcanPluginCapacityPolicyPhase, VulcanPluginCapacityPolicyRecord,
    VulcanPluginPrewarmPolicyPhase, VulcanPluginPrewarmPolicyRecord,
};

/// Explicit continuation of a retained operation; none of them replays already acknowledged work.
/// 保留操作的明确继续方式；均不重放已确认的工作。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum PluginPendingAction {
    /// Continue the original operation from its retained checkpoint.
    /// 从保留检查点继续原操作。
    Resume,
    /// Abandon an unpublished update while preserving external-effect history.
    /// 放弃尚未发布的更新，同时保留外部副作用历史。
    Abort,
    /// Cancel a policy change before it was applied.
    /// 在策略变更应用前取消它。
    Cancel,
    /// Release a terminal policy receipt after the user has seen its outcome.
    /// 用户看到结果后释放终态策略回执。
    Forget,
}

/// One offered action and whether its caller must acknowledge that external effects are not undone.
/// 单个可用动作，以及调用方是否必须确认外部副作用不会被撤销。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PluginPendingActionOffer {
    /// Exact action accepted by the continuation entry.
    /// 继续入口接受的精确动作。
    pub(crate) action: PluginPendingAction,
    /// The continuation must carry `acknowledgeExternalEffects: true`.
    /// 继续请求必须携带 `acknowledgeExternalEffects: true`。
    pub(crate) requires_acknowledgement: bool,
}

impl PluginPendingActionOffer {
    /// Offer action without an acknowledgement requirement.
    /// 提供不要求确认的动作。
    pub(crate) fn plain(action: PluginPendingAction) -> Self {
        Self {
            action,
            requires_acknowledgement: false,
        }
    }

    /// Offer action that requires the external-effect acknowledgement.
    /// 提供要求外部副作用确认的动作。
    pub(crate) fn acknowledged(action: PluginPendingAction) -> Self {
        Self {
            action,
            requires_acknowledgement: true,
        }
    }
}

/// Kind-specific sanitized evidence, tagged with the operation kind on the wire.
/// 各类别的脱敏证据，在线协议上以操作类别标记。
#[derive(Debug, Serialize)]
#[serde(tag = "kind", content = "detail", rename_all = "camelCase")]
pub(crate) enum PluginPendingOperationDetail {
    /// Installation or update journal row.
    /// 安装或更新日志行。
    Update(LocalPluginUpdateStatus),
    /// Live original removal.
    /// 存活原移除。
    Uninstall(local_plugin_uninstall::LocalPluginUninstallStatus),
    /// Capacity policy receipt.
    /// 容量策略回执。
    CapacityPolicy(VulcanPluginCapacityPolicyRecord),
    /// Automatic prewarm policy receipt.
    /// 自动预热策略回执。
    PrewarmPolicy(VulcanPluginPrewarmPolicyRecord),
}

/// One retained operation with its stable identity, owning plugin and offered continuations.
/// 单个保留操作，含稳定身份、所属插件及可用继续方式。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PluginPendingOperation {
    /// Original request or update UUID; recovery never allocates a replacement.
    /// 原请求或更新 UUID；恢复绝不分配替代标识。
    operation_id: String,
    /// Owning plugin namespace; a prewarm policy spans plugins and has none.
    /// 所属插件命名空间；预热策略跨插件，因此为空。
    plugin_id: Option<String>,
    /// Continuations the owner currently accepts; empty while an executor holds the operation.
    /// 所有者当前接受的继续方式；执行器持有操作期间为空。
    actions: Vec<PluginPendingActionOffer>,
    /// Kind tag and its sanitized evidence.
    /// 类别标记及其脱敏证据。
    #[serde(flatten)]
    detail: PluginPendingOperationDetail,
}

impl PluginPendingOperation {
    /// Project one update journal row; finished history without a continuation is not pending.
    /// 投影单个更新日志行；没有继续方式的已结束历史不属于待处理。
    fn update(status: LocalPluginUpdateStatus) -> Option<Self> {
        let actions = status.pending_actions();
        (!actions.is_empty()).then(|| Self {
            operation_id: status.request_id().to_owned(),
            plugin_id: Some(status.plugin_id().to_owned()),
            actions,
            detail: PluginPendingOperationDetail::Update(status),
        })
    }

    /// Project one live removal; an executing removal stays visible without actions.
    /// 投影单个存活移除；执行中的移除保持可见但无动作。
    fn uninstall(status: local_plugin_uninstall::LocalPluginUninstallStatus) -> Self {
        Self {
            operation_id: status.request_id().to_owned(),
            plugin_id: Some(status.plugin_id().to_owned()),
            actions: status.pending_actions(),
            detail: PluginPendingOperationDetail::Uninstall(status),
        }
    }

    /// Project the capacity policy receipt; terminal receipts await acknowledgement, others continuation.
    /// 投影容量策略回执；终态回执等待确认，其余等待继续。
    fn capacity_policy(record: VulcanPluginCapacityPolicyRecord) -> Self {
        let actions = policy_actions(
            record.is_acknowledgeable(),
            record.phase == VulcanPluginCapacityPolicyPhase::Prepared,
        );
        Self {
            operation_id: record.request.update_id.clone(),
            plugin_id: Some(record.request.plugin_id.clone()),
            actions,
            detail: PluginPendingOperationDetail::CapacityPolicy(record),
        }
    }

    /// Project the prewarm policy receipt; only a durably terminal receipt may be acknowledged.
    /// 投影预热策略回执；仅持久终态回执可被确认。
    fn prewarm_policy(record: VulcanPluginPrewarmPolicyRecord) -> Self {
        let actions = policy_actions(
            record.is_acknowledgeable(),
            record.phase == VulcanPluginPrewarmPolicyPhase::Prepared,
        );
        Self {
            operation_id: record.request.update_id.clone(),
            plugin_id: None,
            actions,
            detail: PluginPendingOperationDetail::PrewarmPolicy(record),
        }
    }
}

/// Offer forget for an acknowledgeable receipt, otherwise resume plus cancel while still prepared.
/// 可确认回执提供遗忘，否则提供继续，并在仍处于准备阶段时提供取消。
fn policy_actions(acknowledgeable: bool, prepared: bool) -> Vec<PluginPendingActionOffer> {
    if acknowledgeable {
        return vec![PluginPendingActionOffer::plain(PluginPendingAction::Forget)];
    }
    let mut actions = Vec::new();
    if prepared {
        actions.push(PluginPendingActionOffer::plain(PluginPendingAction::Cancel));
    }
    actions.push(PluginPendingActionOffer::plain(PluginPendingAction::Resume));
    actions
}

/// List every retained plugin operation without starting work, creating storage or attaching a runtime.
/// 列出所有保留的插件操作，不启动工作、不创建存储、不附加运行时。
/// Read-only status remains available while the management owner is busy; returns kinds in a stable order.
/// 管理所有者忙碌时只读状态仍可用；按稳定顺序返回各类别。
pub(crate) async fn service_plugin_pending_operations(
    state: SharedServiceBridgeState,
) -> Result<Vec<PluginPendingOperation>, String> {
    let mut operations: Vec<PluginPendingOperation> =
        local_plugin_update_records(Arc::clone(&state))
            .await?
            .into_iter()
            .filter_map(PluginPendingOperation::update)
            .collect();
    operations.extend(
        local_plugin_uninstall_records(Arc::clone(&state))?
            .into_iter()
            .map(PluginPendingOperation::uninstall),
    );
    // Policy receipts exist only while a formal runtime is attached; absence is an empty result.
    // 策略回执仅在附加正式运行时期间存在；缺失即为空结果。
    let (capacity, prewarm) = run_plugin_runtime_control(state, |runtime| {
        let Some(status) = runtime.formal_runtime_status()? else {
            return Ok((None, None));
        };
        Ok((
            status.policy_update,
            runtime.prewarm_policy_status()?.update,
        ))
    })
    .await?;
    operations.extend(capacity.map(PluginPendingOperation::capacity_policy));
    operations.extend(prewarm.map(PluginPendingOperation::prewarm_policy));
    Ok(operations)
}

/// Update continuation; installation shares the same journal and actions.
/// 更新继续方式；安装共用相同日志与动作。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum PluginUpdateContinuation {
    /// Finish the request from its retained checkpoint.
    /// 从保留检查点完成请求。
    Resume,
    /// Abandon the unpublished request.
    /// 放弃尚未发布的请求。
    Abort,
}

/// Removal continuation; a retained removal can only be carried forward.
/// 移除继续方式；保留的移除只能向前推进。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum PluginUninstallContinuation {
    /// Continue the original removal.
    /// 继续原移除。
    Resume,
}

/// Policy receipt continuation shared by capacity and prewarm policies.
/// 容量与预热策略共用的策略回执继续方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum PluginPolicyContinuation {
    /// Continue publication from the retained checkpoint.
    /// 从保留检查点继续发布。
    Resume,
    /// Cancel before application.
    /// 应用前取消。
    Cancel,
    /// Release a terminal receipt.
    /// 释放终态回执。
    Forget,
}

/// One explicit continuation naming the original operation; invalid kind/action pairs fail to decode.
/// 精确指名原操作的单个明确继续请求；无效的类别与动作组合无法解码。
#[derive(Debug, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub(crate) enum PluginPendingContinuation {
    /// Installation or update journal request.
    /// 安装或更新日志请求。
    Update {
        /// Original request UUID.
        /// 原请求 UUID。
        operation_id: String,
        /// Exact owning namespace.
        /// 精确所属命名空间。
        plugin_id: String,
        /// Explicit continuation.
        /// 明确继续方式。
        action: PluginUpdateContinuation,
        /// Explicit acknowledgement that abandoning does not undo attempted effects.
        /// 明确确认放弃不会撤销已尝试的副作用。
        #[serde(default)]
        acknowledge_external_effects: bool,
    },
    /// Live original removal.
    /// 存活原移除。
    Uninstall {
        /// Original request UUID.
        /// 原请求 UUID。
        operation_id: String,
        /// Namespace captured before any callback.
        /// 任何回调之前捕获的命名空间。
        plugin_id: String,
        /// Explicit continuation.
        /// 明确继续方式。
        action: PluginUninstallContinuation,
        /// Explicit acknowledgement accepting a retired failed callback.
        /// 明确确认接受已退役的失败回调。
        #[serde(default)]
        acknowledge_external_effects: bool,
    },
    /// Capacity policy receipt.
    /// 容量策略回执。
    CapacityPolicy {
        /// Original update UUID.
        /// 原更新 UUID。
        operation_id: String,
        /// Explicit continuation.
        /// 明确继续方式。
        action: PluginPolicyContinuation,
    },
    /// Automatic prewarm policy receipt.
    /// 自动预热策略回执。
    PrewarmPolicy {
        /// Original update UUID.
        /// 原更新 UUID。
        operation_id: String,
        /// Explicit continuation.
        /// 明确继续方式。
        action: PluginPolicyContinuation,
    },
}

/// Continuation result; catalog is present when the operation republished the plugin catalog.
/// 继续结果；操作重新发布插件目录时携带目录。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PluginPendingContinuationOutcome {
    /// Revisioned catalog to apply through the ordinary publisher.
    /// 通过普通发布器应用的带修订目录。
    catalog: Option<LuaSkillsCatalogUpdate>,
}

/// Continue one original operation through its sole owner; app/state publish catalog changes.
/// 通过唯一所有者继续单个原操作；app／state 发布目录变化。
/// Returns the republished catalog when there is one; the caller re-reads the list for the new state.
/// 有重新发布目录时返回该目录；调用方重新读取列表获取新状态。
pub(crate) async fn service_continue_plugin_pending_operation(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    request: PluginPendingContinuation,
) -> Result<PluginPendingContinuationOutcome, String> {
    let catalog = match request {
        PluginPendingContinuation::Update {
            operation_id,
            plugin_id,
            action: PluginUpdateContinuation::Resume,
            ..
        } => Some(service_resume_local_plugin_update(app, state, plugin_id, operation_id).await?),
        PluginPendingContinuation::Update {
            operation_id,
            plugin_id,
            action: PluginUpdateContinuation::Abort,
            acknowledge_external_effects,
        } => {
            let status = service_abort_local_plugin_update(
                state,
                plugin_id,
                operation_id,
                acknowledge_external_effects,
            )
            .await?;
            if !status.is_aborted() {
                return Err(String::from(
                    "The plugin update abandonment was not durably confirmed.",
                ));
            }
            None
        }
        PluginPendingContinuation::Uninstall {
            operation_id,
            plugin_id,
            action: PluginUninstallContinuation::Resume,
            acknowledge_external_effects,
        } => Some(
            service_resume_local_plugin_uninstall(
                app,
                state,
                plugin_id,
                operation_id,
                acknowledge_external_effects,
            )
            .await?,
        ),
        PluginPendingContinuation::CapacityPolicy {
            operation_id,
            action,
        } => {
            run_plugin_runtime_control(state, move |runtime| match action {
                PluginPolicyContinuation::Resume => runtime
                    .resume_capacity_policy_update(&operation_id)
                    .map(drop),
                PluginPolicyContinuation::Cancel => runtime
                    .cancel_capacity_policy_update(&operation_id)
                    .map(drop),
                PluginPolicyContinuation::Forget => {
                    runtime.forget_capacity_policy_update(&operation_id)
                }
            })
            .await?;
            None
        }
        PluginPendingContinuation::PrewarmPolicy {
            operation_id,
            action,
        } => {
            run_plugin_runtime_control(state, move |runtime| match action {
                PluginPolicyContinuation::Resume => runtime
                    .resume_prewarm_policy_update(&operation_id)
                    .map(drop),
                PluginPolicyContinuation::Cancel => runtime
                    .cancel_prewarm_policy_update(&operation_id)
                    .map(drop),
                PluginPolicyContinuation::Forget => {
                    runtime.forget_prewarm_policy_update(&operation_id)
                }
            })
            .await?;
            None
        }
    };
    Ok(PluginPendingContinuationOutcome { catalog })
}

#[cfg(test)]
mod tests;
