//! Atomic public-AI configuration publication and exact activation compensation.
//! 公共 AI 配置原子发布与精确激活补偿。

use std::collections::BTreeSet;
use vulcan_provider_adapters::{PersistedAiConfiguration, PublicAiRuntime};
use vulcan_service_api::{
    ContextCompactionWindowNodeSummary, ServiceEventEmitter, WorkbenchSnapshot,
};

use super::{
    plan_reconciled_session_model_selections, public_ai_session_media_understanding_bridge_kinds,
};
use crate::{
    SharedServiceBridgeState, emit_workbench_event, mutate_public_ai_configuration_in_transaction,
    public_ai_provider_rows_for_selection, public_ai_workbench_selection,
    public_ai_workbench_selection_after_route_revocation,
};

#[cfg(test)]
#[path = "publication/vce_compensation_tests.rs"]
mod vce_compensation_tests;

/// Combines two independent rollback results without hiding either failure.
/// 合并两个独立回滚结果且不隐藏任何失败。
///
/// # Parameters
/// 参数
/// `first` and `second` are independent cleanup or rollback results.
/// `first` 与 `second` 是独立的清理或回滚结果。
///
/// # Returns
/// 返回值
/// Success only when both succeed, otherwise every failure in stable order.
/// 仅两项均成功时返回成功，否则按稳定顺序保留全部失败。
pub(super) fn combine_results(
    first: Result<(), String>,
    second: Result<(), String>,
) -> Result<(), String> {
    match (first, second) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(error), Ok(())) | (Ok(()), Err(error)) => Err(error),
        (Err(first), Err(second)) => Err(format!("{first}; {second}")),
    }
}

/// Preserves a primary failure while exposing any failed compensation step.
/// 保留主失败，并显式暴露任何失败的补偿步骤。
///
/// # Parameters
/// 参数
/// `primary` describes the original operation failure, while `first` and `second`
/// are independent cleanup or state-restoration results.
/// `primary` 描述原始操作失败，`first` 与 `second` 是彼此独立的清理或状态恢复结果。
///
/// # Returns
/// 返回值
/// The primary error with deterministic compensation diagnostics appended.
/// 返回追加确定性补偿诊断后的主错误。
pub(super) fn compensated_operation_error(
    primary: String,
    first: Result<(), String>,
    second: Result<(), String>,
) -> String {
    let mut errors = Vec::new();
    if let Err(error) = first {
        errors.push(format!("first compensation failed: {error}"));
    }
    if let Err(error) = second {
        errors.push(format!("second compensation failed: {error}"));
    }
    if errors.is_empty() {
        primary
    } else {
        format!("{primary} Compensation diagnostics: {}.", errors.join("; "))
    }
}

/// Restores one exact persisted/runtime configuration image after pre-publication failure.
/// 在发布前失败后恢复一份精确的持久化/运行时配置镜像。
///
/// # Parameters
/// 参数
/// `state` resolves persistence, `runtime` owns the current activation, `before_configuration`
/// and `before_runtime` are the exact paired before-images, and `restore_error_context` labels
/// a failed durable compensation.
/// `state` 解析持久化，`runtime` 持有当前激活；`before_configuration` 与 `before_runtime`
/// 是精确配对的前镜像；`restore_error_context` 标记持久补偿失败。
///
/// # Returns
/// 返回值
/// Success only when both the durable configuration and every runtime catalog field return to
/// their exact before-image; otherwise one deterministic uncertainty diagnostic.
/// 仅当持久配置与全部运行时目录字段均返回精确前镜像时成功；否则返回确定性不确定状态诊断。
pub(super) fn restore_public_ai_configuration_activation_in_transaction(
    state: &SharedServiceBridgeState,
    runtime: &PublicAiRuntime,
    before_configuration: &PersistedAiConfiguration,
    before_runtime: &vulcan_provider_adapters::ProviderRuntimeActivationSnapshot,
    restore_error_context: &str,
) -> Result<(), String> {
    // configurationRestore returns the standalone custom manifests and central document to the
    // exact durable before-image before the runtime maps are restored exactly.
    // configurationRestore 在精确恢复运行时映射前，把独立自定义清单与中央文档恢复到
    // 精确持久前镜像。
    let configuration_restore = mutate_public_ai_configuration_in_transaction(
        state,
        restore_error_context,
        |configuration| {
            *configuration = before_configuration.clone();
            Ok(())
        },
    )
    .map(|_| ());
    // runtimeRestore uses the post-compensation image as the CAS compare side so an unrelated
    // mutation is surfaced instead of overwritten.
    // runtimeRestore 使用补偿后的镜像作为 CAS 比较端，使无关变更被显式报告而不是遭到覆盖。
    let runtime_restore = runtime
        .capture_activation_snapshot()
        .and_then(|current_runtime| {
            runtime.restore_activation_snapshot(before_runtime, &current_runtime)
        });
    match (configuration_restore, runtime_restore) {
        (Ok(()), Ok(())) => Ok(()),
        (configuration, runtime) => Err(format!(
            "Public AI configuration activation restoration was incomplete: configuration={configuration:?}; runtime={runtime:?}"
        )),
    }
}

/// Restores one exact runtime-only activation image through compare-and-swap.
/// 通过比较交换恢复一份精确的仅运行时激活镜像。
///
/// # Parameters
/// 参数
/// `runtime` owns the current generation and `before_runtime` is the exact image captured before a
/// validation, discovery, or usage side effect that did not yet persist configuration.
/// `runtime` 持有当前代际，`before_runtime` 是在尚未持久化配置的校验、发现或用量副作用前
/// 捕获的精确镜像。
///
/// # Returns
/// 返回值
/// Success after exact restoration, or an explicit snapshot/CAS failure without overwriting
/// unrelated state.
/// 精确恢复后返回成功；若快照或比较交换失败则显式报错，且不覆盖无关状态。
pub(super) fn restore_public_ai_runtime_activation(
    runtime: &PublicAiRuntime,
    before_runtime: &vulcan_provider_adapters::ProviderRuntimeActivationSnapshot,
) -> Result<(), String> {
    // currentRuntime is the exact compare side that prevents blind restoration.
    // currentRuntime 是防止盲目恢复的精确比较端。
    let current_runtime = runtime.capture_activation_snapshot()?;
    runtime
        .restore_activation_snapshot(before_runtime, &current_runtime)
        .map(|_| ())
}

/// Mutates configuration and atomically publishes its Workbench/response projection or restores it.
/// 变更配置并原子发布配套工作台/响应投影，失败时恢复原状。
///
/// # Parameters
/// 参数
/// `app`, `state`, and `runtime` own the publication boundary; `before_configuration` is the
/// exact durable baseline; `save_error_context` labels the forward write; `mutate` builds the
/// candidate; `prepare_output` completes every fallible response projection before Workbench
/// state changes.
/// `app`、`state` 与 `runtime` 持有发布边界；`before_configuration` 是精确持久基线；
/// `save_error_context` 标记正向写入；`mutate` 构建候选；`prepare_output` 在工作台状态改变前
/// 完成全部可失败响应投影。
///
/// # Returns
/// 返回值
/// The installed configuration and prepared response only after Workbench publication succeeds;
/// any later failure restores the paired durable/runtime before-image.
/// 仅在工作台发布成功后返回已安装配置与预备响应；任何后续失败都会恢复配对的持久/运行时前镜像。
pub(super) fn mutate_public_ai_configuration_and_publish_in_transaction<
    Mutation,
    PrepareOutput,
    Output,
>(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    runtime: &PublicAiRuntime,
    before_configuration: &PersistedAiConfiguration,
    save_error_context: &str,
    mutate: Mutation,
    prepare_output: PrepareOutput,
) -> Result<(PersistedAiConfiguration, Output), String>
where
    Mutation: FnOnce(&mut PersistedAiConfiguration) -> Result<(), String>,
    PrepareOutput: FnOnce(&PersistedAiConfiguration) -> Result<Output, String>,
{
    mutate_public_ai_configuration_and_publish_using_in_transaction(
        state,
        runtime,
        before_configuration,
        save_error_context,
        mutate,
        prepare_output,
        |configuration| publish_public_ai_workbench(app, state, runtime, configuration).map(|_| ()),
    )
}

/// Mutates provider-tool configuration without reconciling unrelated session model routes.
/// 修改供应商工具配置时不协调无关的会话模型路由。
///
/// # Parameters
/// 参数
/// `app`, `state`, and `runtime` own the publication boundary; `before_configuration` is the
/// durable before-image; `save_error_context` labels persistence; `mutate` builds the candidate;
/// `prepare_output` completes the response projection.
/// `app`、`state` 与 `runtime` 持有发布边界；`before_configuration` 是持久化前镜像；
/// `save_error_context` 标记持久化；`mutate` 构建候选；`prepare_output` 完成响应投影。
///
/// # Returns
/// 返回值
/// The installed configuration and output after provider-tool publication, or a compensated error.
/// 供应商工具发布后的已安装配置与输出，或已完成补偿的错误。
pub(super) fn mutate_public_ai_configuration_and_publish_without_session_model_reconciliation_in_transaction<
    Mutation,
    PrepareOutput,
    Output,
>(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    runtime: &PublicAiRuntime,
    before_configuration: &PersistedAiConfiguration,
    save_error_context: &str,
    mutate: Mutation,
    prepare_output: PrepareOutput,
) -> Result<(PersistedAiConfiguration, Output), String>
where
    Mutation: FnOnce(&mut PersistedAiConfiguration) -> Result<(), String>,
    PrepareOutput: FnOnce(&PersistedAiConfiguration) -> Result<Output, String>,
{
    mutate_public_ai_configuration_and_publish_using_in_transaction(
        state,
        runtime,
        before_configuration,
        save_error_context,
        mutate,
        prepare_output,
        |configuration| {
            publish_public_ai_workbench_without_session_model_reconciliation(
                app,
                state,
                runtime,
                configuration,
            )
            .map(|_| ())
        },
    )
}

/// Mutates configuration and atomically publishes explicit session-route revocations.
/// 修改配置并原子发布显式会话路由撤销。
///
/// # Parameters
/// 参数
/// `app`, `state`, `runtime`, and `before_configuration` own the activation boundary;
/// `save_error_context` labels persistence; `mutate` builds the candidate; `prepare_output`
/// completes the response; `reconcile_sessions` updates a detached workspace and returns exact
/// lease owners to revoke; `persistence_reason` labels session-state persistence.
/// `app`、`state`、`runtime` 与 `before_configuration` 持有激活边界；`save_error_context` 标记
/// 持久化；`mutate` 构建候选；`prepare_output` 完成响应；`reconcile_sessions` 更新独立工作区并
/// 返回要撤销租约的精确所有者；`persistence_reason` 标记会话状态持久化。
///
/// # Returns
/// 返回值
/// The installed configuration and prepared output only after configuration, leases, workspace,
/// and Workbench commit together; any failure restores configuration/runtime and leases.
/// 仅当配置、租约、工作区与工作台共同提交后返回已安装配置及预备输出；任何失败都会恢复
/// 配置、运行时与租约。
// Transactional publication separates mutation, session reconciliation and durable commit callbacks.
// 事务发布分别保留变更、会话协调与持久化提交回调。
#[expect(
    clippy::too_many_arguments,
    reason = "Transactional publication separates mutation, session reconciliation and durable commit callbacks."
)]
pub(super) fn mutate_public_ai_configuration_and_publish_sessions_in_transaction<
    Mutation,
    PrepareOutput,
    ReconcileSessions,
    Output,
>(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    runtime: &PublicAiRuntime,
    before_configuration: &PersistedAiConfiguration,
    save_error_context: &str,
    mutate: Mutation,
    prepare_output: PrepareOutput,
    reconcile_sessions: ReconcileSessions,
    persistence_reason: &'static str,
) -> Result<(PersistedAiConfiguration, Output), String>
where
    Mutation: FnOnce(&mut PersistedAiConfiguration) -> Result<(), String>,
    PrepareOutput: FnOnce(&PersistedAiConfiguration) -> Result<Output, String>,
    ReconcileSessions: FnOnce(
        &mut vulcan_workspace_store::workspace_store::WorkspaceSnapshot,
    ) -> Result<BTreeSet<crate::SessionId>, String>,
{
    mutate_public_ai_configuration_and_publish_using_in_transaction(
        state,
        runtime,
        before_configuration,
        save_error_context,
        mutate,
        prepare_output,
        |configuration| {
            publish_public_ai_workbench_with_session_reconciliation_and_commit(
                app,
                state,
                runtime,
                configuration,
                reconcile_sessions,
                persistence_reason,
                || Ok(()),
            )
            .map(|_| ())
        },
    )
}

/// Executes one reversible configuration activation with a caller-selected publication boundary.
/// 使用调用方选择的发布边界执行一次可逆配置激活。
///
/// # Parameters
/// 参数
/// `state`, `runtime`, and `before_configuration` identify the exact before-image;
/// `save_error_context` labels persistence; `mutate` builds the candidate; `prepare_output`
/// completes the response; `publish` atomically commits the browser/session projection.
/// `state`、`runtime` 与 `before_configuration` 标识精确前镜像；`save_error_context` 标记持久化；
/// `mutate` 构建候选；`prepare_output` 完成响应；`publish` 原子提交浏览器与会话投影。
///
/// # Returns
/// 返回值
/// The installed configuration and output after publication, or a compensated error.
/// 发布后返回已安装配置与输出，失败则返回已补偿错误。
pub(super) fn mutate_public_ai_configuration_and_publish_using_in_transaction<
    Mutation,
    PrepareOutput,
    Publish,
    Output,
>(
    state: &SharedServiceBridgeState,
    runtime: &PublicAiRuntime,
    before_configuration: &PersistedAiConfiguration,
    save_error_context: &str,
    mutate: Mutation,
    prepare_output: PrepareOutput,
    publish: Publish,
) -> Result<(PersistedAiConfiguration, Output), String>
where
    Mutation: FnOnce(&mut PersistedAiConfiguration) -> Result<(), String>,
    PrepareOutput: FnOnce(&PersistedAiConfiguration) -> Result<Output, String>,
    Publish: FnOnce(&PersistedAiConfiguration) -> Result<(), String>,
{
    // runtimeBeforeImage pairs every catalog/status/cache field with the durable baseline.
    // runtimeBeforeImage 把每个目录、状态与缓存字段同持久基线配对。
    let runtime_before_image = runtime.capture_activation_snapshot()?;
    // configuration crosses the durable/runtime activation boundary before any browser state.
    // configuration 在任何浏览器状态前越过持久/运行时激活边界。
    let configuration =
        mutate_public_ai_configuration_in_transaction(state, save_error_context, mutate)?;
    // publicationResult prepares the complete RPC result before the atomic Workbench swap.
    // publicationResult 在原子工作台交换前准备完整 RPC 结果。
    let publication_result =
        prepare_output(&configuration).and_then(|output| publish(&configuration).map(|_| output));
    match publication_result {
        Ok(output) => Ok((configuration, output)),
        Err(primary) => {
            // restoration owns every configuration/runtime field changed after the before-image.
            // restoration 持有前镜像后发生变更的全部配置/运行时字段。
            let restoration = restore_public_ai_configuration_activation_in_transaction(
                state,
                runtime,
                before_configuration,
                &runtime_before_image,
                "Failed to restore the state from before the public AI configuration publication",
            );
            Err(compensated_operation_error(primary, restoration, Ok(())))
        }
    }
}

/// Invalidates node-dependent budget fingerprints for every locked active or archived session.
/// 使所有已锁定活动或归档会话中依赖节点的预算指纹失效。
///
/// # Parameters
/// 参数
/// `workspace` is the detached candidate whose session node ids remain unchanged.
/// `workspace` 是会话节点 ID 保持不变的独立候选工作区。
///
/// # Returns
/// 返回值
/// Exact session ids whose in-memory live budget snapshots must also be removed at commit.
/// 返回提交时还必须移除内存 live 预算快照的精确会话 ID。
fn invalidate_context_compaction_window_budget_fingerprints(
    workspace: &mut vulcan_workspace_store::workspace_store::WorkspaceSnapshot,
) -> BTreeSet<crate::SessionId> {
    // invalidatedSessionIds identifies only sessions whose selected node resolves through global values.
    // invalidatedSessionIds 仅标识所选节点通过全局数值解析的会话。
    let mut invalidated_session_ids = BTreeSet::new();
    for (session_id, runtime_state) in workspace
        .session_runtime_state
        .iter_mut()
        .chain(workspace.archived_session_runtime_state.iter_mut())
    {
        if runtime_state.context_compaction_window_node_id.is_none() {
            continue;
        }
        runtime_state.live_context_budget_fingerprint = None;
        runtime_state.anchor_context_budget_fingerprint = None;
        invalidated_session_ids.insert(*session_id);
    }
    invalidated_session_ids
}

/// Publishes changed global node values with atomic session-budget invalidation.
/// 以原子会话预算失效方式发布已变化的全局节点数值。
///
/// # Parameters
/// 参数
/// `app`, `state`, `runtime`, and `configuration` identify the exact committed generation;
/// `nodes_changed` distinguishes a real value change from an idempotent save.
/// `app`、`state`、`runtime` 与 `configuration` 标识精确提交代际；`nodes_changed`
/// 区分真实数值变化与幂等保存。
///
/// # Returns
/// 返回值
/// The atomically installed Workbench snapshot, or one explicit publication error.
/// 返回原子安装的工作台快照，或一条明确的发布错误。
pub(super) fn publish_public_ai_workbench_after_context_compaction_window_nodes_change(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    runtime: &PublicAiRuntime,
    configuration: &PersistedAiConfiguration,
    nodes_changed: bool,
) -> Result<WorkbenchSnapshot, String> {
    publish_public_ai_workbench_with_workspace_reconciliation_and_commit(
        app,
        state,
        runtime,
        configuration,
        |workspace| {
            // invalidatedSessionIds is empty for an idempotent save and preserves every route lease.
            // invalidatedSessionIds 在幂等保存时为空，并始终保留全部路由租约。
            let invalidated_session_ids = if nodes_changed {
                invalidate_context_compaction_window_budget_fingerprints(workspace)
            } else {
                BTreeSet::new()
            };
            Ok(
                PublicAiWorkspaceReconciliation::context_budget_invalidations(
                    invalidated_session_ids,
                ),
            )
        },
        true,
        "context_compaction_window_nodes",
        || Ok(()),
    )
}

/// Publishes public AI model rows through the existing shared workbench event.
/// 通过既有共享工作台事件发布公共 AI 模型行。
///
/// # Parameters
/// 参数
/// `app` emits the snapshot, `state` owns workbench state, and runtime/configuration are exact.
/// `app` 发出快照，`state` 持有工作台状态，运行时与配置均为精确快照。
///
/// # Returns
/// 返回值
/// The updated workbench snapshot after an atomic state replacement.
/// 原子替换状态后返回更新的工作台快照。
pub(crate) fn publish_public_ai_workbench(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    runtime: &PublicAiRuntime,
    configuration: &PersistedAiConfiguration,
) -> Result<WorkbenchSnapshot, String> {
    publish_public_ai_workbench_with_commit(app, state, runtime, configuration, || Ok(()))
}

/// Publishes provider-tool changes while preserving every session model route lease.
/// 发布供应商工具变更，同时保留全部会话模型路由租约。
///
/// # Parameters
/// 参数
/// `app` emits the snapshot, `state` owns Workbench state, and `runtime` plus `configuration`
/// identify the exact provider projection.
/// `app` 发出快照，`state` 持有工作台状态，`runtime` 与 `configuration` 标识精确供应商投影。
///
/// # Returns
/// 返回值
/// The updated Workbench snapshot after an atomic provider-tool-only state replacement.
/// 在原子替换仅供应商工具状态后返回更新的工作台快照。
fn publish_public_ai_workbench_without_session_model_reconciliation(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    runtime: &PublicAiRuntime,
    configuration: &PersistedAiConfiguration,
) -> Result<WorkbenchSnapshot, String> {
    publish_public_ai_workbench_with_workspace_reconciliation_and_commit(
        app,
        state,
        runtime,
        configuration,
        |_workspace| Ok(PublicAiWorkspaceReconciliation::default()),
        false,
        "provider_tool_settings",
        || Ok(()),
    )
}

/// Publishes one public AI Workbench candidate after a caller-owned pre-publication commit.
/// 在调用方持有的发布前提交成功后发布一份公共 AI 工作台候选。
///
/// # Parameters
/// 参数
/// `app`, `state`, `runtime`, and `configuration` identify the exact projection; `commit` performs
/// any final fallible durable transition while browser-visible state is still unchanged.
/// `app`、`state`、`runtime` 与 `configuration` 标识精确投影；`commit` 在浏览器可见状态仍未
/// 改变时执行最终可失败持久转换。
///
/// # Returns
/// 返回值
/// The atomically installed Workbench snapshot, or an error that leaves Service state unchanged.
/// 返回原子安装的工作台快照，或使 Service 状态保持不变的错误。
pub(super) fn publish_public_ai_workbench_with_commit<Commit>(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    runtime: &PublicAiRuntime,
    configuration: &PersistedAiConfiguration,
    commit: Commit,
) -> Result<WorkbenchSnapshot, String>
where
    Commit: FnOnce() -> Result<(), String>,
{
    publish_public_ai_workbench_with_session_reconciliation_and_commit(
        app,
        state,
        runtime,
        configuration,
        |_workspace| Ok(BTreeSet::new()),
        "missing_model_fallback",
        commit,
    )
}

/// One detached workspace reconciliation split by persistence and route-lease ownership.
/// 一份按持久化与路由租约所有权拆分的独立工作区协调结果。
#[derive(Default)]
pub(super) struct PublicAiWorkspaceReconciliation {
    /// Every session runtime row changed by the detached plan.
    /// 独立规划修改的每个会话运行态行。
    pub(super) changed_session_ids: BTreeSet<crate::SessionId>,
    /// Changed sessions whose process-local model route leases must be revoked.
    /// 已修改且必须撤销进程内模型路由租约的会话。
    pub(super) revoked_session_ids: BTreeSet<crate::SessionId>,
    /// Sessions whose node-dependent in-memory context budgets must be discarded at commit.
    /// 提交时必须丢弃依赖节点的内存上下文预算的会话。
    invalidated_context_budget_session_ids: BTreeSet<crate::SessionId>,
}

impl PublicAiWorkspaceReconciliation {
    /// Creates a lease-only reconciliation for explicit route revocation.
    /// 为显式路由撤销创建一份仅租约协调结果。
    ///
    /// # Parameters
    /// 参数
    /// `session_ids` is the exact revoked owner set; durable session configuration remains unchanged.
    /// `session_ids` 是精确的已撤销所有者集合；可靠会话配置保持不变。
    ///
    /// # Returns
    /// 返回值
    /// A reconciliation with no durable row changes and the exact revoked set.
    /// 返回不包含可靠行变化且包含精确撤销集合的协调结果。
    fn route_revocations(session_ids: BTreeSet<crate::SessionId>) -> Self {
        Self {
            changed_session_ids: BTreeSet::new(),
            revoked_session_ids: session_ids,
            invalidated_context_budget_session_ids: BTreeSet::new(),
        }
    }

    /// Creates a workspace-only reconciliation that preserves model route leases.
    /// 创建一份保留模型路由租约的仅工作区协调结果。
    ///
    /// # Parameters
    /// 参数
    /// `session_ids` identifies runtime rows that require persistence only.
    /// `session_ids` 标识仅需持久化的运行态行。
    ///
    /// # Returns
    /// 返回值
    /// A reconciliation with changed rows and no revoked route owners.
    /// 返回包含已修改行但不包含已撤销路由所有者的协调结果。
    pub(super) fn workspace_only(session_ids: BTreeSet<crate::SessionId>) -> Self {
        Self {
            changed_session_ids: session_ids,
            revoked_session_ids: BTreeSet::new(),
            invalidated_context_budget_session_ids: BTreeSet::new(),
        }
    }

    /// Creates an in-memory budget invalidation that preserves durable controls and route leases.
    /// 创建一份保留持久控制项与路由租约的内存预算失效结果。
    ///
    /// # Parameters
    /// 参数
    /// `session_ids` identifies locked active or archived sessions affected by changed node values.
    /// `session_ids` 标识受节点数值变化影响的已锁定活动或归档会话。
    ///
    /// # Returns
    /// 返回值
    /// A reconciliation that invalidates context budgets without scheduling redundant addon writes.
    /// 返回一份使上下文预算失效且不安排冗余 addon 写入的协调结果。
    fn context_budget_invalidations(session_ids: BTreeSet<crate::SessionId>) -> Self {
        Self {
            changed_session_ids: BTreeSet::new(),
            revoked_session_ids: BTreeSet::new(),
            invalidated_context_budget_session_ids: session_ids,
        }
    }
}

/// Publishes one Workbench candidate with explicit session-route reconciliation.
/// 通过显式会话路由协调发布一份工作台候选。
///
/// # Parameters
/// 参数
/// `app`, `state`, `runtime`, and `configuration` identify the exact projection;
/// `reconcile_sessions` changes a detached workspace and returns leases that must be revoked;
/// `persistence_reason` labels durable session-state persistence; `commit` performs the final
/// fallible external transition before browser-visible state changes.
/// `app`、`state`、`runtime` 与 `configuration` 标识精确投影；`reconcile_sessions` 修改独立
/// 工作区并返回必须撤销租约的会话；`persistence_reason` 标记持久会话状态写入；`commit`
/// 在浏览器可见状态改变前执行最终可失败外部转换。
///
/// # Returns
/// 返回值
/// The atomically installed Workbench snapshot, or an error that restores revoked leases and
/// leaves Service state unchanged.
/// 返回原子安装的工作台快照；失败时恢复已撤销租约并保持 Service 状态不变。
pub(super) fn publish_public_ai_workbench_with_session_reconciliation_and_commit<
    ReconcileSessions,
    Commit,
>(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    runtime: &PublicAiRuntime,
    configuration: &PersistedAiConfiguration,
    reconcile_sessions: ReconcileSessions,
    persistence_reason: &'static str,
    commit: Commit,
) -> Result<WorkbenchSnapshot, String>
where
    ReconcileSessions: FnOnce(
        &mut vulcan_workspace_store::workspace_store::WorkspaceSnapshot,
    ) -> Result<BTreeSet<crate::SessionId>, String>,
    Commit: FnOnce() -> Result<(), String>,
{
    publish_public_ai_workbench_with_workspace_reconciliation_and_commit(
        app,
        state,
        runtime,
        configuration,
        |workspace| {
            reconcile_sessions(workspace).map(PublicAiWorkspaceReconciliation::route_revocations)
        },
        true,
        persistence_reason,
        commit,
    )
}

/// Publishes one Workbench candidate from a detached general workspace reconciliation.
/// 从一份独立的通用工作区协调结果发布工作台候选。
///
/// # Parameters
/// 参数
/// `app`, `state`, `runtime`, and `configuration` identify the projection; `reconcile_workspace`
/// returns separately tracked changed and route-revoked sessions; `reconcile_session_models`
/// controls whether unrelated catalog-only model reconciliation runs; `persistence_reason` labels
/// durable session rows; `commit` performs the final fallible external transition.
/// `app`、`state`、`runtime` 与 `configuration` 标识投影；`reconcile_workspace` 分别返回已修改
/// 与路由已撤销会话；`reconcile_session_models` 控制是否运行无关的仅目录模型协调；
/// `persistence_reason` 标记持久会话行；`commit` 执行最终可失败外部转换。
///
/// # Returns
/// 返回值
/// The atomically installed Workbench snapshot, or an error with Service state and revoked leases
/// restored.
/// 返回原子安装的工作台快照；失败时恢复 Service 状态与已撤销租约。
// Transactional publication separates mutation, session reconciliation and durable commit callbacks.
// 事务发布分别保留变更、会话协调与持久化提交回调。
#[expect(
    clippy::too_many_arguments,
    reason = "Transactional publication separates mutation, session reconciliation and durable commit callbacks."
)]
pub(super) fn publish_public_ai_workbench_with_workspace_reconciliation_and_commit<
    ReconcileWorkspace,
    Commit,
>(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    runtime: &PublicAiRuntime,
    configuration: &PersistedAiConfiguration,
    reconcile_workspace: ReconcileWorkspace,
    reconcile_session_models: bool,
    _persistence_reason: &'static str,
    commit: Commit,
) -> Result<WorkbenchSnapshot, String>
where
    ReconcileWorkspace: FnOnce(
        &mut vulcan_workspace_store::workspace_store::WorkspaceSnapshot,
    ) -> Result<PublicAiWorkspaceReconciliation, String>,
    Commit: FnOnce() -> Result<(), String>,
{
    // catalogs freezes the exact runtime model image used by the whole candidate projection.
    // catalogs 冻结整个候选投影使用的精确运行时模型镜像。
    let catalogs = runtime.catalog_snapshot()?;
    let workbench = {
        // guard excludes every concurrent Service state writer while the detached candidate is
        // prepared and committed.
        // guard 在独立候选准备与提交期间排除全部并发 Service 状态写入方。
        let mut guard = state
            .lock()
            .map_err(|error| format!("Failed to lock service state: {error}"))?;
        // plannedWorkspace and changedSessionIds remain detached until every fallible projection
        // and lease-exclusion check has succeeded.
        // plannedWorkspace 与 changedSessionIds 在全部可失败投影及租约排除检查成功前保持独立。
        let (
            planned_workspace,
            revoked_session_ids,
            changed_session_ids,
            invalidated_context_budget_session_ids,
        ) = if let Some(workspace) = guard.workspace_snapshot.as_ref() {
            // forcedWorkspace applies explicit access revocation before catalog-only fallback.
            // forcedWorkspace 在仅目录回退前应用显式访问撤销。
            let mut forced_workspace = workspace.clone();
            let PublicAiWorkspaceReconciliation {
                mut changed_session_ids,
                revoked_session_ids,
                invalidated_context_budget_session_ids,
            } = reconcile_workspace(&mut forced_workspace)?;
            let (planned_workspace, catalog_changed_session_ids) = if reconcile_session_models {
                plan_reconciled_session_model_selections(
                    forced_workspace,
                    runtime,
                    configuration,
                    &catalogs,
                )?
            } else {
                // Provider-tool changes do not change the model route owned by this session.
                // 供应商工具变更不会改变该会话拥有的模型路由。
                (forced_workspace, BTreeSet::new())
            };
            changed_session_ids.extend(catalog_changed_session_ids);
            (
                Some(planned_workspace),
                revoked_session_ids,
                changed_session_ids,
                invalidated_context_budget_session_ids,
            )
        } else {
            (None, BTreeSet::new(), BTreeSet::new(), BTreeSet::new())
        };
        // selectedSessionId and selectedRuntimeState identify the same candidate workbench owner
        // for the shared lease-aware projection helper.
        // selectedSessionId 与 selectedRuntimeState 为共享租约感知投影辅助函数标识同一候选工作台所有者。
        let selected_session_id = planned_workspace
            .as_ref()
            .map(|workspace| workspace.selected_session);
        let selected_runtime_state = planned_workspace.as_ref().and_then(|workspace| {
            workspace
                .session_runtime_state
                .get(&workspace.selected_session)
        });
        // plannedLiveContextBudgets removes stale node-derived snapshots without touching other sessions.
        // plannedLiveContextBudgets 移除依赖旧节点值的快照，同时不影响其他会话。
        let planned_live_context_budgets = if invalidated_context_budget_session_ids.is_empty() {
            None
        } else {
            let mut live_context_budgets = guard.live_context_budgets.clone();
            live_context_budgets.retain(|session_id, _| {
                !invalidated_context_budget_session_ids.contains(session_id)
            });
            Some(live_context_budgets)
        };
        // effectiveSelection prefers the immutable process-local session lease before current catalogs.
        // effectiveSelection 在当前目录之前优先采用进程内不可变会话租约。
        let effective_selection = if selected_session_id
            .is_some_and(|session_id| revoked_session_ids.contains(&session_id))
        {
            public_ai_workbench_selection_after_route_revocation(
                configuration,
                runtime,
                selected_runtime_state,
            )?
        } else {
            public_ai_workbench_selection(
                configuration,
                runtime,
                selected_session_id,
                selected_runtime_state,
            )?
        };
        // bridgedMediaKinds freezes every exact attachment bridge result for this candidate.
        // bridgedMediaKinds 冻结当前候选的每一项精确附件桥接结果。
        let bridged_media_kinds = public_ai_session_media_understanding_bridge_kinds(
            runtime,
            configuration,
            selected_runtime_state,
        )?;
        // rows is the complete provider projection derived from the same immutable selection.
        // rows 是从同一不可变选择派生的完整供应商投影。
        let rows = public_ai_provider_rows_for_selection(
            configuration,
            runtime,
            effective_selection.as_ref(),
        )?;
        // plannedWorkbench isolates all browser-visible mutations until the final commit boundary.
        // plannedWorkbench 把全部浏览器可见变更隔离到最终提交边界。
        let mut planned_workbench = guard.workbench.clone();
        planned_workbench.providers = rows;
        // contextCompactionWindowNodes publishes the same validated configuration generation as providers.
        // contextCompactionWindowNodes 与供应商一起发布同一份已校验配置代际。
        planned_workbench.context_compaction_window_nodes = configuration
            .context_compaction_window_nodes
            .nodes
            .iter()
            .map(|node| ContextCompactionWindowNodeSummary {
                id: node.id.as_str().to_string(),
                tokens: node.tokens,
            })
            .collect();
        // selectedNodeId remains session-owned and is never replaced by the global token values.
        // selectedNodeId 始终由会话持有，绝不会被全局 Token 数值替代。
        planned_workbench.session_context_compaction_window_node_id = selected_runtime_state
            .and_then(|runtime_state| runtime_state.context_compaction_window_node_id)
            .map(|node_id| node_id.as_str().to_string());
        if selected_session_id
            .is_some_and(|session_id| invalidated_context_budget_session_ids.contains(&session_id))
        {
            // contextUsage is rebuilt from historical usage and the new node value in the same generation.
            // contextUsage 在同一代际中根据历史用量与新节点值重新构建。
            let live_context_budgets = planned_live_context_budgets
                .as_ref()
                .unwrap_or(&guard.live_context_budgets);
            let compression_window_limit =
                crate::context_compaction_window_limit_from_configuration(
                    selected_runtime_state,
                    Some(configuration),
                );
            let selected_context_usage_facts = selected_session_id
                .map(|session_id| {
                    crate::session_context_usage_facts_from_v1_registry(
                        guard.session_runtime_registry.as_ref(),
                        session_id,
                    )
                })
                .transpose()?
                .flatten();
            planned_workbench.context_usage = selected_session_id.and_then(|session_id| {
                crate::context_usage_snapshot_from_runtime_state(
                    session_id,
                    selected_runtime_state,
                    selected_context_usage_facts.as_ref(),
                    live_context_budgets,
                    effective_selection
                        .as_ref()
                        .map(|selection| &selection.model.capabilities),
                    compression_window_limit,
                )
            });
        }
        if let Some(provider_configuration) = guard
            .agent_runtime
            .as_ref()
            .map(|agent_runtime| agent_runtime.provider_configuration.clone())
        {
            planned_workbench.reasoning_selection = crate::reasoning_selection(
                &provider_configuration,
                effective_selection
                    .as_ref()
                    .map(|selection| &selection.model.capabilities),
            );
            planned_workbench.composer_attachment_capabilities =
                crate::composer_attachment_capabilities(
                    effective_selection.as_ref().map(|selection| {
                        selection.route.protocol_route.protocol_profile_id.as_str()
                    }),
                    effective_selection
                        .as_ref()
                        .map(|selection| &selection.model.capabilities),
                    &bridged_media_kinds,
                );
        }
        // leaseScopeIds materializes the exact sessions proven unleased during planning.
        // leaseScopeIds 物化规划期间被证明未租用的精确会话。
        let revoked_lease_scope_ids = revoked_session_ids
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        // requiredAbsentLeaseScopeIds contains catalog-only changes whose leases must remain absent.
        // requiredAbsentLeaseScopeIds 包含租约必须保持缺失的仅目录变更。
        let required_absent_lease_scope_ids = if reconcile_session_models {
            changed_session_ids
                .difference(&revoked_session_ids)
                .map(ToString::to_string)
                .collect::<Vec<_>>()
        } else {
            // Provider-tool persistence is independent of model-route lease ownership.
            // 供应商工具持久化独立于模型路由租约所有权。
            Vec::new()
        };
        // leaseGuard atomically removes explicit revocations and excludes every concurrent writer.
        // leaseGuard 原子移除显式撤销，并排除全部并发写入方。
        let lease_guard =
            if revoked_lease_scope_ids.is_empty() && required_absent_lease_scope_ids.is_empty() {
                None
            } else {
                Some(
                    runtime.acquire_session_selection_lease_reconciliation_guard(
                        &revoked_lease_scope_ids,
                        &required_absent_lease_scope_ids,
                    )?,
                )
            };
        // commit is the last fallible external transition and runs while Service state and lease
        // admission remain excluded. A failure therefore leaves the browser before-image intact.
        // commit 是最后一个可失败外部转换，并在 Service 状态与租约准入均被排除时运行；
        // 因此失败会让浏览器前镜像保持完整。
        commit()?;
        // A published provider projection is a new authoritative snapshot and must outrank the
        // previous WebUI revision. No fallible operation remains after these assignments.
        // 已发布的供应商投影是新的权威快照，必须以更高版本覆盖 WebUI 中的旧快照；
        // 以下赋值之后不再存在可失败操作。
        guard.workspace_snapshot = planned_workspace;
        if let Some(planned_live_context_budgets) = planned_live_context_budgets {
            guard.live_context_budgets = planned_live_context_budgets;
        }
        guard.workbench_generation = guard.workbench_generation.wrapping_add(1);
        planned_workbench.revision = guard.workbench_generation;
        guard.workbench = planned_workbench;
        // Committing after both assignments makes lease deletion inseparable from state publication.
        // 在两项赋值之后提交，使租约删除与状态发布不可分割。
        if let Some(lease_guard) = lease_guard {
            lease_guard.commit();
        }
        // Durable session configuration is committed by the caller's exact V1 mutation before
        // browser publication. Route revocation only removes process-local leases and never
        // rewrites session-owned metadata, so no legacy SessionAddon payload remains here.
        // 可靠会话配置由调用方在浏览器发布前通过精确 V1 修改提交。路由撤销只移除
        // 进程内租约且绝不改写会话自有 metadata，因此此处不再存在旧 SessionAddon 载荷。
        guard.workbench.clone()
    };
    emit_workbench_event(app, &workbench);
    // VCE consumes the committed public-AI generation and closes every project runtime when a
    // required route disappears or becomes incompatible.
    // VCE 消费已提交的公共 AI 代际；任一必需路由消失或不兼容时关闭全部项目运行时。
    crate::vce::schedule_vce_reconciliation(app.clone(), state.clone());
    schedule_native_memory_revalidation(app.clone(), state.clone());
    Ok(workbench)
}

/// Revalidates native memory against the latest committed AI generation and publishes its readiness.
/// 基于最新已提交 AI 代际重验原生记忆，并发布就绪状态。
/// `app` delivers the refreshed snapshot; `state` supplies the process-local runtime. Returns nothing.
/// `app` 投递刷新快照；`state` 提供进程内运行时。无返回值。
fn schedule_native_memory_revalidation(app: ServiceEventEmitter, state: SharedServiceBridgeState) {
    // Runtime ownership is copied without retaining the service lock across an asynchronous wait.
    // 复制运行时所有权，不跨异步等待持有服务锁。
    let runtime = state
        .lock()
        .ok()
        .and_then(|guard| guard.service_runtime.clone());
    let Some(runtime) = runtime else { return };
    runtime.spawn(async move {
        // Serialize with every AI writer and read current capabilities after admission, never a queued before-image.
        // 与所有 AI 写入串行，准入后读取当前能力，绝不使用排队时的旧镜像。
        let _transaction = crate::begin_public_ai_configuration_transaction().await;
        // Native validation does not perform inference or modify user memory; it refreshes capability/config caches.
        // 原生校验不执行推理或修改用户记忆，仅刷新能力及配置缓存。
        let maintenance = state.lock().ok().and_then(|guard| {
            // Only the active native backend owns this cache; external mode must not activate native resources.
            // 只有活动原生后端拥有该缓存；外链模式不得激活原生资源。
            let configuration = &guard.agent_runtime.as_ref()?.provider_configuration.vmm;
            let effective = vulcan_vmm::VmmEffectiveConfig::resolve(configuration, None, None);
            if !effective.enabled || effective.backend_mode != vulcan_vmm::VmmBackendMode::Native {
                return None;
            }
            guard.native_vmm_maintenance.clone()
        });
        let Some(maintenance) = maintenance else {
            return;
        };
        if let Err(error) = maintenance.validate_host_ai().await {
            tracing::warn!("[webui-service] Native memory capability revalidation failed: {error}");
        }
        // Failed readiness must also reach the browser; the transaction prevents stale completion after a newer save.
        // 失败就绪状态也必须送达浏览器；事务阻止旧完成结果落在更新保存之后。
        match crate::refresh_workbench_snapshot(state).await {
            Ok(workbench) => emit_workbench_event(&app, &workbench),
            Err(error) => {
                tracing::warn!(
                    "[webui-service] Native memory readiness publication failed: {error}"
                )
            }
        }
    });
}

#[cfg(test)]
mod tests;
