//! Immutable user-plugin updates retain the exact original request through candidate validation and publication.
//! 不可变用户插件更新在候选验证及发布期间保留精确原请求。

use super::*;
use crate::plugin_mcp_secret_store::OsPluginMcpSecretStore;
use vulcan_agent_runtime::mcp::{CandidatePluginMcpRuntime, candidate_local_mcp_server_configs};
use vulcan_plugin_runtime::{
    VulcanPluginActivationTransaction, VulcanPluginCredentialKey, VulcanPluginCredentialValues,
    VulcanPluginDescriptor, VulcanPluginGenerationStore, VulcanPluginRuntimeManager,
    VulcanPluginRuntimeState, VulcanPluginUpdatePolicy,
};

pub(super) mod pending_owner;
mod target_handoff;
#[cfg(test)]
pub(super) mod tests;

/// User-visible rejection prevents effects when the old enabled route cannot be restored from exact immutable inputs.
/// 用户可见拒绝在旧启用路由无法从精确不可变输入恢复时阻止副作用。
const ENABLED_PREDECESSOR_RESTORATION_REQUIRED: &str =
    "启用前驱缺少完整的不可变配置或凭据选择，请先停用插件再更新。";

/// Validate predecessor against its authoritative old selection before any enabled drain or callback effect.
/// 在任何启用排空或回调副作用前，按权威旧选择校验 predecessor。
/// Disabled legacy remains supported; selection comes from the actual store or retained original transaction.
/// 停用旧格式继续受支持；selection 来自实际存储或保留原事务。
/// Return an explicit unsupported-state error rather than guessing historical configuration or credentials.
/// 返回明确不支持状态错误，而非猜测历史配置或凭据。
fn validate_enabled_predecessor_restoration_selection(
    predecessor: &VulcanPluginDescriptor,
    selection: Option<&vulcan_plugin_runtime::VulcanPluginActivation>,
) -> Result<(), String> {
    match (
        predecessor.runtime_state,
        predecessor.managed_package(),
        selection,
    ) {
        (VulcanPluginRuntimeState::Disabled, _, _) => Ok(()),
        (VulcanPluginRuntimeState::Enabled, Some(package), Some(old))
            if old.configuration().is_some()
                && old.credential_revision().is_some()
                && package.matches_activation(old) =>
        {
            Ok(())
        }
        _ => Err(String::from(ENABLED_PREDECESSOR_RESTORATION_REQUIRED)),
    }
}

/// Sanitized operation evidence omits private storage metadata and callback values.
/// 脱敏操作证据省略私有存储元数据及回调值。
#[derive(Debug, Serialize)]
struct LocalPluginUpdateEffect {
    /// Exact declared operation.
    /// 精确声明操作。
    operation: vulcan_plugin_runtime::VulcanPluginActivationEffect,
    /// Historical observation does not certify current resource retirement.
    /// 历史观测不证明当前资源退役。
    phase: vulcan_plugin_runtime::VulcanPluginEffectObservation,
}

/// Sanitized durable status exposes stable recovery identity without configuration documents or credential values.
/// 脱敏持久状态公开稳定恢复身份，不包含配置文档或凭据值。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LocalPluginUpdateStatus {
    /// Original request identity, independent from foreground RPC lifetime.
    /// 原请求身份，独立于前台 RPC 寿命。
    request_id: String,
    /// Exact owning plugin namespace.
    /// 精确所属插件命名空间。
    plugin_id: String,
    /// Durable selection phase; it is not a live-resource or readiness result.
    /// 持久选择阶段；它不是存活资源或就绪结果。
    phase: Option<vulcan_plugin_runtime::VulcanPluginActivationTransactionPhase>,
    /// Planned or prepared immutable package identity contains no user configuration or secret values.
    /// 计划中或已准备不可变插件包身份不包含用户配置或秘密值。
    candidate: vulcan_plugin_runtime::VulcanPluginGenerationIdentity,
    /// Decimal revision text preserves exact values in JavaScript clients.
    /// 十进制修订文本在 JavaScript 客户端中保留精确值。
    activation_revision: String,
    /// Optional durable installation evidence is independent from current native call status.
    /// 可选持久安装证据独立于当前原生调用状态。
    installation: Option<vulcan_plugin_runtime::VulcanPluginInstallationPhase>,
    /// Attempted probe and lifecycle operations remain independently inspectable.
    /// 已尝试探测及生命周期操作保持可独立检查。
    effects: Vec<LocalPluginUpdateEffect>,
    /// Current service ownership is a live registry observation, independent from durable phase.
    /// 当前服务所有权为实时注册表观察，独立于持久阶段。
    has_service_owner: bool,
}

/// Project one original record from store into a sanitized status; reject incomplete identities instead of inventing them.
/// 将 store 中单个原记录投影为脱敏状态；拒绝不完整身份，不虚构它们。
fn status(
    store: &VulcanPluginGenerationStore,
    record: &vulcan_plugin_runtime::VulcanPluginActivationRequestRecord,
) -> Result<LocalPluginUpdateStatus, String> {
    // A planned generation identifies resources without claiming that copying or configuration is complete.
    // 计划代次标识资源，不声称复制或配置已经完成。
    let candidate = record.candidate().ok_or("原更新请求缺少候选身份。")?;
    Ok(LocalPluginUpdateStatus {
        request_id: record.request_id().to_string(),
        plugin_id: candidate.plugin_id.clone(),
        phase: Some(record.phase()),
        candidate: candidate.clone(),
        activation_revision: record.activation_revision().to_string(),
        effects: store
            .activation_effects(&candidate.plugin_id, record.request_id())
            .map_err(|error| error.to_string())?
            .map(|receipt| {
                receipt
                    .observations()
                    .into_iter()
                    .map(|(operation, phase)| LocalPluginUpdateEffect { operation, phase })
                    .collect()
            })
            .unwrap_or_default(),
        installation: store
            .installation_receipt(&candidate.plugin_id, record.request_id())
            .map_err(|error| error.to_string())?
            .map(|receipt| receipt.phase()),
        has_service_owner: false,
    })
}

/// List every original request for plugin_id without starting work, creating storage or selecting a recovery candidate.
/// 列出 plugin_id 的全部原请求，不启动工作、不创建存储、不选择恢复候选。
/// Runtime supplies the sole configured user root; returns redacted durable records only.
/// runtime 提供唯一配置用户根；仅返回脱敏持久记录。
pub(super) fn list(
    runtime: &VulcanPluginRuntimeManager,
    plugin_id: &str,
) -> Result<Vec<LocalPluginUpdateStatus>, String> {
    // Existing absence is a read-only empty result.
    // 既有缺失为只读空结果。
    let Some(store) = VulcanPluginGenerationStore::open_existing(runtime.user_plugin_root())
        .map_err(|error| error.to_string())?
    else {
        return Ok(Vec::new());
    };
    store
        .activation_transaction_ids(plugin_id)
        .map_err(|error| error.to_string())?
        .into_iter()
        .map(|request_id| {
            // Each exact ID is validated; disappearing or malformed records are explicit failures.
            // 每个精确标识均经过校验；消失或损坏记录为明确失败。
            let record = store
                .activation_request(plugin_id, request_id)
                .map_err(|error| error.to_string())?
                .ok_or_else(|| String::from("原更新请求在查询期间消失。"))?;
            status(&store, &record)
        })
        .collect()
}

/// Read actual namespace ownership before choosing the sealed early reader or ordinary durable reader.
/// 选择密封早期读取器或普通持久读取器之前，读取实际命名空间所有权。
/// Registry is a short service snapshot; returns sanitized history and only confirmed absent-journal null phases.
/// registry 为短暂服务快照；返回脱敏历史及仅已确认日志缺失的空阶段。
pub(super) fn list_owned(
    runtime: &VulcanPluginRuntimeManager,
    plugin_id: &str,
    registry: &pending_owner::PendingLocalPluginUpdateRegistry,
) -> Result<Vec<LocalPluginUpdateStatus>, String> {
    pending_owner::list_owned(runtime, plugin_id, registry)
}

/// Parse one explicitly supplied original request identity; aliases and nil values are rejected.
/// 解析单个明确提供的原请求身份；拒绝别名及空值。
fn request_identity(value: &str) -> Result<uuid::Uuid, String> {
    // Canonical spelling is part of the durable request contract.
    // 规范拼写属于持久请求契约。
    let identity = uuid::Uuid::parse_str(value).map_err(|_| String::from("更新请求标识无效。"))?;
    if identity.is_nil() || identity.to_string() != value {
        return Err(String::from("更新请求标识必须为规范非空 UUID。"));
    }
    Ok(identity)
}

/// Verify the refreshed catalog contains the complete committed target with disabled business admission.
/// 验证刷新目录包含完整已提交目标，且业务入场仍停用。
/// Runtime supplies live safety counts; catalog and target come from the original request; returns an explicit validation error.
/// runtime 提供实时安全计数；catalog 及 target 来自原请求；返回明确校验错误。
fn validate_published_catalog(
    runtime: &VulcanPluginRuntimeManager,
    catalog: &vulcan_plugin_runtime::VulcanPluginCatalogSnapshot,
    target: &vulcan_plugin_runtime::VulcanPluginActivation,
) -> Result<(), String> {
    // Missing and unavailable descriptors cannot satisfy successful route publication.
    // 缺失及不可用描述符不能满足成功路由发布。
    let plugin = catalog
        .plugins
        .get(&target.plugin_id)
        .ok_or("已提交目标未出现在插件目录中。")?;
    if plugin.availability != vulcan_plugin_runtime::VulcanPluginAvailability::Disabled
        || plugin.runtime_state != VulcanPluginRuntimeState::Disabled
        || plugin.desired_enabled
        || runtime
            .active_session_count(&plugin.id)
            .map_err(|error| error.to_string())?
            != 0
        || runtime
            .has_cleanup_diagnostics(&plugin.id)
            .map_err(|error| error.to_string())?
        || !plugin
            .managed_package()
            .is_some_and(|package| package.matches_activation(target))
    {
        return Err(String::from(
            "已提交目标未能完整投影为停用插件；保留原请求供检查。",
        ));
    }
    Ok(())
}

/// Abandon only the original unpublished request after explicit external-effect acknowledgement when needed.
/// 仅在需要时明确确认外部副作用后，放弃原尚未发布请求。
/// Returns retained evidence; no package, user data, secure entry, or historical receipt is deleted.
/// 返回保留证据；不删除插件包、用户数据、安全条目或历史回执。
pub(super) fn abort(
    runtime: &VulcanPluginRuntimeManager,
    plugin_id: &str,
    request_id: &str,
    acknowledge_external_effects: bool,
) -> Result<LocalPluginUpdateStatus, String> {
    // Exact identity is validated before accessing any durable path.
    // 访问任何持久路径之前校验精确身份。
    let request_id = request_identity(request_id)?;
    // Recovery never creates an empty store and pretends it owns an old request.
    // 恢复绝不创建空存储并假装拥有旧请求。
    let store = VulcanPluginGenerationStore::open_existing(runtime.user_plugin_root())
        .map_err(|error| error.to_string())?
        .ok_or("原更新存储不存在。")?;
    // Inspect the declared stage once; a concurrent transition must fail its exact stage operation, never fall through.
    // 一次检查声明阶段；并发转换必须使精确阶段操作失败，绝不改走另一分支。
    let original = store
        .activation_request(plugin_id, request_id)
        .map_err(|error| error.to_string())?
        .ok_or("原更新请求不存在。")?;
    // An already durable abort is historical status only; reading it cannot reconstruct or retire physical restoration resources.
    // 已持久中止仅为历史状态；读取它不能重建或退役物理恢复资源。
    if matches!(&original, vulcan_plugin_runtime::VulcanPluginActivationRequestRecord::Activation(record) if record.phase() == vulcan_plugin_runtime::VulcanPluginActivationTransactionPhase::Aborted)
    {
        return status(&store, &original);
    }
    // Enabled requests with actual lifecycle attempts require original restoration; untouched preparations remain abandonable.
    // 具有实际生命周期尝试的启用请求要求原恢复；未触及前驱的准备仍可放弃。
    if matches!(&original, vulcan_plugin_runtime::VulcanPluginActivationRequestRecord::Activation(record) if record.update_policy().is_some_and(VulcanPluginUpdatePolicy::desired_enabled))
    {
        target_handoff::require_unstarted_enabled_update(runtime, &store, plugin_id, request_id)?;
    }
    if matches!(
        original,
        vulcan_plugin_runtime::VulcanPluginActivationRequestRecord::Preparation(_)
    ) {
        if !acknowledge_external_effects {
            return Err(String::from(
                "准备阶段可能已复制插件包或暂存凭据；必须明确确认放弃不等于撤销这些副作用。",
            ));
        }
        // The core reacquires original ownership and preserves every physical allocation for explicit cleanup.
        // 核心重新取得原所有权，并保留全部物理分配用于明确清理。
        let abandoned = store
            .abort_activation_preparation_request(plugin_id, request_id)
            .map_err(|error| error.to_string())?;
        return status(&store, &abandoned);
    }
    // Explicit acknowledgement permits abandoning corrupt candidates without pretending to undo external effects.
    // 明确确认允许放弃损坏候选，但不假装撤销外部副作用。
    let record = if acknowledge_external_effects {
        store
            .abort_activation_request(plugin_id, request_id)
            .map_err(|error| error.to_string())?
    } else {
        // Hold actual original ownership while checking absence so a concurrent callback cannot start after the check.
        // 检查缺失期间持有实际原所有权，使并发回调不能在检查后开始。
        let mut transaction = store
            .resume_activation_transaction(plugin_id, request_id)
            .map_err(|error| error.to_string())?;
        if store
            .installation_receipt(plugin_id, request_id)
            .map_err(|error| error.to_string())?
            .is_some()
            || store
                .activation_effects(plugin_id, request_id)
                .map_err(|error| error.to_string())?
                .is_some()
        {
            return Err(String::from(
                "原候选已有副作用尝试；必须先核对外部状态，并明确确认放弃选择不等于撤销副作用。",
            ));
        }
        store
            .abort_activation_transaction(&mut transaction)
            .map_err(|error| error.to_string())?;
        transaction.record().clone()
    };
    status(
        &store,
        &vulcan_plugin_runtime::VulcanPluginActivationRequestRecord::Activation(record),
    )
}

/// Check the disabled publication path against original policy and current explicit overrides under the writer fence.
/// 在写入屏障下按原策略及当前显式覆盖检查停用发布路径。
/// Legacy records declare the existing disabled-only contract; newer requests must also retain a disabled original target.
/// 旧记录声明既有仅停用契约；较新请求还必须保留停用的原目标。
/// Returns a conflict before selection or catalog publication instead of enabling a candidate through a changed policy.
/// 在选择或目录发布前返回冲突，不经已变策略启用候选。
fn validate_disabled_update_publication_policy(
    record: &vulcan_plugin_runtime::VulcanPluginActivationTransactionRecord,
    configuration: &vulcan_provider_config::config::PersistedLuaSkillsConfiguration,
) -> Result<(), String> {
    // Identity and original desired state come from the same validated durable request.
    // 身份及原期望状态来自相同已校验持久请求。
    let plugin_id = &record.target().plugin_id;
    if record
        .update_policy()
        .is_some_and(VulcanPluginUpdatePolicy::desired_enabled)
        || !configuration.disabled_vulcan_plugins.contains(plugin_id)
        || configuration.enabled_vulcan_plugins.contains(plugin_id)
    {
        return Err(String::from(
            "原更新请求的停用目标与当前用户策略不一致，不能发布候选路由。",
        ));
    }
    Ok(())
}

/// Finish the exact already-published request without invoking preparation, readiness, or installation again.
/// 终结精确已发布请求，不再次调用准备、就绪或安装。
/// App/state own route publication; request_id is explicit and target must still be the actual durable selection.
/// app/state 拥有路由发布；request_id 为明确输入，目标必须仍为实际持久选择。
pub(super) async fn resume_published(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    plugin_id: &str,
    request_id: &str,
) -> Result<LuaSkillsCatalogUpdate, String> {
    // Canonical identity comes from the existing contract before any owner, disk or catalog recovery work.
    // 任何所有者、磁盘或目录恢复工作前，规范身份来自既有契约。
    let request_id = request_identity(request_id)?;
    {
        // Only this original request may retain its own actual Full; another request cannot replace that catalog.
        // 仅此原请求可保留其自身实际 Full；另一请求不能替换该目录。
        let guard = state.lock().map_err(|error| error.to_string())?;
        crate::service_state_access::require_plugin_update_full_publication_admission(
            &guard,
            Some(request_id),
        )?;
    }
    // An active original owner must be dispatched directly; disk recovery may never compete for its real lock.
    // 活动原所有者必须直接分派；磁盘恢复绝不能争用其真实锁。
    let registry = pending_owner::admitted_registry(state)?;
    if let Some(owner) = registry.get(plugin_id, request_id)? {
        return resume_retained(app, state, &registry, &owner).await;
    }
    // Reuse only the service's original manager and original immutable request.
    // 仅复用服务原管理器及原不可变请求。
    let runtime = service_vulcan_plugin_runtime(state)?;
    // The existing store is authoritative and is not recreated by recovery.
    // 既有存储具有权威，恢复不重新创建它。
    let store = VulcanPluginGenerationStore::open_existing(runtime.user_plugin_root())
        .map_err(|error| error.to_string())?
        .ok_or("原更新存储不存在。")?;
    // Original ownership remains exclusive against real background candidate resources.
    // 原所有权相对真实后台候选资源保持排他。
    let mut transaction = store
        .resume_activation_transaction(plugin_id, request_id)
        .map_err(|error| error.to_string())?;
    if !matches!(
        transaction.record().phase(),
        vulcan_plugin_runtime::VulcanPluginActivationTransactionPhase::Committing
            | vulcan_plugin_runtime::VulcanPluginActivationTransactionPhase::Published
    ) || store
        .activation(plugin_id)
        .map_err(|error| error.to_string())?
        .as_ref()
        != Some(transaction.record().target())
    {
        return Err(String::from(
            "原请求尚无确定的已提交目标；此恢复入口只补齐已提交路由，不重放验证或安装。",
        ));
    }
    // The same serialized configuration authority fences readback and every route publication.
    // 相同串行配置权威对回读及全部路由发布设立屏障。
    let policy_publication =
        crate::service_state_access::begin_provider_configuration_transaction().await;
    // Read the latest retained settings only after taking that authority.
    // 仅在取得该权威后读取最新保留设置。
    let configuration = provider_configuration_from_state(state)?;
    validate_disabled_update_publication_policy(transaction.record(), &configuration.luaskills)?;
    store
        .commit_activation_transaction(&mut transaction)
        .map_err(|error| error.to_string())?;
    // No candidate callback is run during this readback-and-route operation.
    // 此回读及路由操作期间不运行候选回调。
    {
        // Validate before core refresh replaces the exact manager catalog Arc, not merely at the later Full write.
        // 在核心刷新替换精确管理器目录 Arc 前校验，而非仅在后续 Full 写入时校验。
        let guard = state.lock().map_err(|error| error.to_string())?;
        crate::service_state_access::require_plugin_update_full_publication_admission(
            &guard,
            Some(transaction.record().request_id()),
        )?;
    }
    let catalog = runtime
        .refresh_catalog_for_activation_recovery(
            &transaction,
            &configuration.luaskills.disabled_vulcan_plugins,
            &configuration.luaskills.enabled_vulcan_plugins,
        )
        .map_err(|error| error.to_string())?;
    validate_published_catalog(&runtime, &catalog, transaction.record().target())?;
    synchronize_service_vulcan_plugin_hooks(state, runtime.clone(), &catalog)?;
    vulcan_plugin_lifecycle::publish_disabled_plugin_extension_generation(
        state, plugin_id, catalog,
    )?;
    store
        .complete_activation_transaction(&mut transaction)
        .map_err(|error| error.to_string())?;
    drop(policy_publication);
    refresh_plugin_catalog_local_state_and_emit(app, state).await
}

/// Resume exact selected route publication through the same strongly retained original transaction.
/// 通过相同强保留原事务恢复精确已选路由发布。
/// Inputs identify the service publisher and actual slot; no candidate callback or new readiness attempt is permitted.
/// 输入标识服务发布器及实际槽；不允许候选回调或新就绪尝试。
/// Returns a confirmed catalog or preserves the same physical owner and its complete original evidence.
/// 返回已确认目录，或保留相同物理所有者及其完整原证据。
async fn resume_retained(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    registry: &pending_owner::PendingLocalPluginUpdateRegistry,
    owner: &Arc<pending_owner::PendingLocalPluginUpdate>,
) -> Result<LuaSkillsCatalogUpdate, String> {
    // Early recovery borrows the retained concrete owner inside the same blocking executor.
    // 早期恢复在相同阻塞执行器内借用保留具体所有者。
    let early = {
        let resources = owner.resources.lock().await;
        pending_owner::require_current_manager(state, &resources)?;
        !matches!(
            resources.stage,
            pending_owner::PendingLocalPluginUpdateStage::Prepared(_)
        )
    };
    if early {
        let recovering = Arc::clone(owner);
        run_extension_management_blocking("recover retained original preparation", move || {
            pending_owner::recover_preparation(&mut recovering.resources.blocking_lock())
        })
        .await?;
        return continue_prepared_update(app, state, registry, owner).await;
    }
    // The service-owned management executor already holds the unique management fence.
    // 服务自有管理执行器已经持有唯一管理屏障。
    let mut resources = owner.resources.lock().await;
    pending_owner::require_current_manager(state, &resources)?;
    // Clone only manager access; the original transaction stays in its retained slot.
    // 仅克隆管理器访问；原事务保持在保留槽中。
    let runtime = resources.runtime.clone();
    // Enabled originals must retain their Published record until actual combined transfer confirms disk completion.
    // 启用原请求必须保留 Published 正文，直到实际组合转交确认磁盘完成。
    if resources
        .prepared()?
        .transaction
        .record()
        .update_policy()
        .is_some_and(VulcanPluginUpdatePolicy::desired_enabled)
    {
        // An existing actual restoration must resume before target reconciliation can rewrite its original phase.
        // 既有实际恢复必须先于目标对账恢复，避免对账改写其原阶段。
        let restoring = resources
            .target
            .as_ref()
            .is_some_and(target_handoff::RetainedEnabledPluginUpdate::has_restoration);
        drop(resources);
        if restoring {
            return target_handoff::resume_predecessor_restoration(app, state, registry, owner)
                .await;
        }
        return target_handoff::resume_enabled_update(app, state, registry, owner).await;
    }
    // Original full-record readback updates this same owner without creating a request lease or replaying work.
    // 原完整记录回读更新此相同所有者，不创建请求租约，也不重放工作。
    let prepared = resources.prepared_mut()?;
    let phase = prepared
        .store
        .reconcile_activation_transaction_owned(&mut prepared.transaction)
        .map_err(|error| error.to_string())?;
    if phase == vulcan_plugin_runtime::VulcanPluginActivationTransactionPhase::Completed {
        // Same-attempt release is observed without restarting readiness or discarding its retained diagnostics early.
        // 观察相同尝试释放，不重启就绪，也不提前丢弃保留诊断。
        retire_candidate_control(&mut resources).await?;
        registry.retire(owner, &resources)?;
        drop(resources);
        return refresh_plugin_catalog_local_state_and_emit(app, state).await;
    }
    if !matches!(
        phase,
        vulcan_plugin_runtime::VulcanPluginActivationTransactionPhase::Committing
            | vulcan_plugin_runtime::VulcanPluginActivationTransactionPhase::Published
    ) || prepared
        .store
        .activation(&owner.plugin_id)
        .map_err(|error| error.to_string())?
        .as_ref()
        != Some(prepared.transaction.record().target())
    {
        return Err(String::from(
            "原请求尚无确定的已提交目标；保留原所有者，不重放验证或安装。",
        ));
    }
    // A selected original may keep only its actual observer; require actual same-attempt release before publishing.
    // 已选原请求可仅保留其实际观察对象；发布之前要求相同尝试实际释放。
    if let Some(candidate) = resources.candidate.as_ref() {
        candidate
            .observer()
            .wait_until_released(AgentRuntimeBridge::RETIRED_MCP_PARTITION_DRAIN_TIMEOUT)
            .await?;
    }
    // Reborrow after observation while keeping the concrete original controller strongly retained through completion.
    // 观察后重新借用，同时在完成之前强保留具体原控制对象。
    let prepared = resources.prepared_mut()?;
    // Latest policy is observed under the existing writer fence before any route publication.
    // 任一路由发布之前，在既有写入屏障下观察最新策略。
    let policy_publication =
        crate::service_state_access::begin_provider_configuration_transaction().await;
    let configuration = provider_configuration_from_state(state)?;
    validate_disabled_update_publication_policy(
        prepared.transaction.record(),
        &configuration.luaskills,
    )?;
    if phase != vulcan_plugin_runtime::VulcanPluginActivationTransactionPhase::Published {
        prepared
            .store
            .commit_activation_transaction(&mut prepared.transaction)
            .map_err(|error| error.to_string())?;
    }
    // Recovery changes only original selected routes; callbacks and native startup are never invoked here.
    // 恢复仅改变原已选路由；此处绝不调用回调或原生启动。
    {
        // Exclude another retained Full before the original recovery refresh changes core catalog identity.
        // 原恢复刷新改变核心目录身份前，排除另一保留 Full。
        let guard = state.lock().map_err(|error| error.to_string())?;
        crate::service_state_access::require_plugin_update_full_publication_admission(
            &guard,
            Some(prepared.transaction.record().request_id()),
        )?;
    }
    let catalog = runtime
        .refresh_catalog_for_activation_recovery(
            &prepared.transaction,
            &configuration.luaskills.disabled_vulcan_plugins,
            &configuration.luaskills.enabled_vulcan_plugins,
        )
        .map_err(|error| error.to_string())?;
    validate_published_catalog(&runtime, &catalog, prepared.transaction.record().target())?;
    synchronize_service_vulcan_plugin_hooks(state, runtime, &catalog)?;
    vulcan_plugin_lifecycle::publish_disabled_plugin_extension_generation(
        state,
        &owner.plugin_id,
        catalog,
    )?;
    prepared
        .store
        .complete_activation_transaction(&mut prepared.transaction)
        .map_err(|error| error.to_string())?;
    drop(policy_publication);
    retire_candidate_control(&mut resources).await?;
    registry.retire(owner, &resources)?;
    drop(resources);
    refresh_plugin_catalog_local_state_and_emit(app, state).await
}

/// Dispatch an explicit abort to a live original slot, or use declared disk recovery only when no slot exists.
/// 将明确中止分派给存活原槽，或仅在槽不存在时使用已声明磁盘恢复。
/// State selects the sole registry; inputs retain exact namespace, UUID, and acknowledgement semantics.
/// state 选择唯一注册表；输入保留精确命名空间、UUID 及确认语义。
/// Returns retained evidence after real retirement, never a claim that external effects were undone.
/// 在真实退役后返回保留证据，绝不声称外部副作用已经撤销。
pub(super) async fn abort_owned(
    state: &SharedServiceBridgeState,
    plugin_id: &str,
    request_id: &str,
    acknowledge_external_effects: bool,
) -> Result<LocalPluginUpdateStatus, String> {
    // Parse the same authoritative original UUID before permitting restoration to alter actual catalogs.
    // 允许恢复改变实际目录之前，解析相同权威原 UUID。
    let identity = request_identity(request_id)?;
    {
        // Own-request compensation may continue; another original's retained publication remains excluded.
        // 自有请求补偿可继续；另一原请求保留发布仍被排除。
        let guard = state.lock().map_err(|error| error.to_string())?;
        crate::service_state_access::require_plugin_update_full_publication_admission(
            &guard,
            Some(identity),
        )?;
    }
    // Exact lookup is outside the slot lock and does not wait for a diagnostic-only status query.
    // 精确查询位于槽锁之外，不让仅诊断状态查询等待。
    let registry = pending_owner::admitted_registry(state)?;
    let Some(owner) = registry.get(plugin_id, identity)? else {
        // No physical owner exists in this service; preserve the existing explicit durable recovery boundary.
        // 此服务不存在物理所有者；保留既有明确持久恢复边界。
        let runtime = service_vulcan_plugin_runtime(state)?;
        let plugin_id = plugin_id.to_owned();
        let request_id = request_id.to_owned();
        return run_extension_management_blocking("abandon immutable plugin update", move || {
            abort(
                &runtime,
                &plugin_id,
                &request_id,
                acknowledge_external_effects,
            )
        })
        .await;
    };
    // A failed executor releases its borrow, allowing this exact service owner to retire its original resources.
    // 失败执行器释放其借用，允许此精确服务所有者退役其原资源。
    let mut resources = owner.resources.lock().await;
    // Preparation alone cannot grant permission to drain an enabled old plugin while abandoning a candidate.
    // 仅准备候选不能授权在放弃时排空已启用旧插件。
    let preserve_enabled_predecessor = match &resources.stage {
        pending_owner::PendingLocalPluginUpdateStage::Reserved(reserved) => {
            reserved.predecessor.runtime_state == VulcanPluginRuntimeState::Enabled
        }
        pending_owner::PendingLocalPluginUpdateStage::Preparing(preparing) => {
            preparing.predecessor.runtime_state == VulcanPluginRuntimeState::Enabled
        }
        pending_owner::PendingLocalPluginUpdateStage::Prepared(prepared) => prepared
            .transaction
            .record()
            .update_policy()
            .is_some_and(VulcanPluginUpdatePolicy::desired_enabled),
    };
    if preserve_enabled_predecessor {
        if resources.candidate.is_some()
            || resources
                .target
                .as_ref()
                .is_some_and(target_handoff::RetainedEnabledPluginUpdate::has_started_participant)
        {
            // Route the real retained drain/restoration before generic abort reconciliation changes its local checkpoint.
            // 通用中止对账改变局部检查点之前，分派真实保留排空／恢复。
            drop(resources);
            return target_handoff::abort_enabled_update(
                state,
                &registry,
                &owner,
                acknowledge_external_effects,
            )
            .await;
        }
        if let pending_owner::PendingLocalPluginUpdateStage::Prepared(prepared) = &resources.stage {
            target_handoff::require_unstarted_enabled_update(
                &resources.runtime,
                &prepared.store,
                plugin_id,
                identity,
            )?;
        }
    }
    if let Some(candidate) = resources.candidate.as_ref() {
        candidate.shutdown();
        candidate
            .observer()
            .wait_until_released(AgentRuntimeBridge::RETIRED_MCP_PARTITION_DRAIN_TIMEOUT)
            .await?;
    }
    // Abort requires actual original Lua retirement before any durable terminal acknowledgement.
    // 任一持久终态确认之前，中止要求实际原 Lua 退役。
    if !preserve_enabled_predecessor {
        // Only the disabled path owns old Lua retirement; an untouched enabled predecessor keeps its sessions.
        // 仅停用路径拥有旧 Lua 退役；未触及的启用前驱保留会话。
        let drained = resources
            .runtime
            .drain_plugin_sessions(plugin_id)
            .await
            .map_err(|error| error.to_string())?;
        if !drained.failures.is_empty() {
            return Err(format!(
                "原请求 Lua 资源尚未退役：{}",
                drained.failures.join("；")
            ));
        }
    }
    if !matches!(
        resources.stage,
        pending_owner::PendingLocalPluginUpdateStage::Prepared(_)
    ) {
        // Release the async borrow before the blocking worker borrows the same retained slot.
        // 阻塞工作线程借用相同保留槽之前，释放异步借用。
        drop(resources);
        let abandoning = Arc::clone(&owner);
        let cleaned =
            run_extension_management_blocking("clean retained original preparation", move || {
                pending_owner::abort_preparation(
                    &mut abandoning.resources.blocking_lock(),
                    acknowledge_external_effects,
                )
            })
            .await?;
        resources = owner.resources.lock().await;
        if cleaned {
            let pending_owner::PendingLocalPluginUpdateStage::Preparing(preparing) =
                &resources.stage
            else {
                return Err(String::from("原准备清理确认后的资源类型发生改变。"));
            };
            let result = status(
                &preparing.store,
                preparing.abandoned.as_ref().ok_or("原中止正文缺失。")?,
            )?;
            retire_candidate_control(&mut resources).await?;
            registry.retire(&owner, &resources)?;
            return Ok(result);
        }
        // Actual already-written promotion now uses the same transaction's existing abort protocol.
        // 实际已经写入的晋级现在使用相同事务的既有中止协议。
    }
    // Reconcile the exact terminal decision before requiring acknowledgement for a new abort mutation.
    // 要求新中止变更确认之前，先核对精确终态决定。
    let phase = {
        // This borrow updates only the same original observation and does not release any physical authority.
        // 此借用仅更新相同原观察，不释放任何物理权威。
        let prepared = resources.prepared_mut()?;
        prepared
            .store
            .reconcile_activation_transaction_owned(&mut prepared.transaction)
            .map_err(|error| error.to_string())?
    };
    if phase != vulcan_plugin_runtime::VulcanPluginActivationTransactionPhase::Aborted
        && (!matches!(
            phase,
            vulcan_plugin_runtime::VulcanPluginActivationTransactionPhase::Prepared
                | vulcan_plugin_runtime::VulcanPluginActivationTransactionPhase::Committing
        ) || resources
            .prepared()?
            .store
            .activation(plugin_id)
            .map_err(|error| error.to_string())?
            .as_ref()
            != resources.prepared()?.transaction.record().predecessor())
    {
        return Err(String::from(
            "原请求的选择已提交或改变，不能中止；请恢复相同原请求的已选路由。",
        ));
    }
    if phase != vulcan_plugin_runtime::VulcanPluginActivationTransactionPhase::Aborted
        && !acknowledge_external_effects
        && resources
            .candidate
            .as_ref()
            .is_some_and(|candidate| !candidate.observer().server_states().is_empty())
    {
        // Actual nonempty states certify a startup attempt; an already confirmed Aborted decision needs no second consent.
        // 实际非空状态证明启动尝试；已经确认 Aborted 的决定不需要第二次同意。
        return Err(String::from(
            "原候选已有实际 MCP 启动尝试；必须明确确认放弃选择不等于撤销外部副作用。",
        ));
    }
    // Continue with the same retained transaction after its independent exact readback.
    // 独立精确回读后，继续使用相同保留事务。
    let prepared = resources.prepared_mut()?;
    if phase != vulcan_plugin_runtime::VulcanPluginActivationTransactionPhase::Aborted {
        if !acknowledge_external_effects
            && (prepared
                .store
                .installation_receipt(plugin_id, identity)
                .map_err(|error| error.to_string())?
                .is_some()
                || prepared
                    .store
                    .activation_effects(plugin_id, identity)
                    .map_err(|error| error.to_string())?
                    .is_some())
        {
            return Err(String::from(
                "原候选已有副作用尝试；必须先核对外部状态，并明确确认放弃选择不等于撤销副作用。",
            ));
        }
        prepared
            .store
            .abort_activation_transaction(&mut prepared.transaction)
            .map_err(|error| error.to_string())?;
    }
    let result = status(
        &prepared.store,
        &vulcan_plugin_runtime::VulcanPluginActivationRequestRecord::Activation(
            prepared.transaction.record().clone(),
        ),
    )?;
    // Only the explicit confirmed terminal decision releases the same real controller and its diagnostics.
    // 仅明确已确认终态决定释放相同真实控制对象及其诊断。
    // An unstarted target contains only existing observations; confirmed abandonment can discard that empty participant slot.
    // 未开始目标仅包含既有观察；确认放弃后可以丢弃该空参与者槽。
    if preserve_enabled_predecessor {
        resources.target = None;
    }
    retire_candidate_control(&mut resources).await?;
    registry.retire(&owner, &resources)?;
    Ok(result)
}

/// Read current strong original ownership without awaiting long-running mutation or callback work.
/// 不等待长时间变更或回调工作，读取当前强原所有权。
/// State selects the actual registry; rows keep their independent durable phases and sanitized evidence.
/// state 选择实际注册表；各行保持独立持久阶段及脱敏证据。
pub(super) fn project_service_owners(
    state: &SharedServiceBridgeState,
    rows: &mut [LocalPluginUpdateStatus],
) -> Result<(), String> {
    // Status remains readable while shutdown has closed mutation admission.
    // 关闭停止变更准入期间，状态仍可读取。
    let registry = Arc::clone(
        &state
            .lock()
            .map_err(|error| error.to_string())?
            .pending_local_plugin_updates,
    );
    for row in rows {
        row.has_service_owner = registry
            .get(&row.plugin_id, request_identity(&row.request_id)?)?
            .is_some();
    }
    Ok(())
}

/// Owns one prepared request and its exact predecessor; public discovery remains unchanged until explicit commit.
/// 拥有单个准备请求及其精确前驱；在明确提交前，公开发现保持不变。
struct PreparedLocalPluginUpdate {
    /// Original store is the only activation authority for this update.
    /// 原存储是此更新的唯一激活权威。
    store: VulcanPluginGenerationStore,
    /// Non-cloneable request retains actual package and credential resources.
    /// 不可克隆请求保留实际插件包及凭据资源。
    transaction: VulcanPluginActivationTransaction,
    /// Frozen predecessor is retained through preparation and final route handoff.
    /// 固定前驱在准备及最终路由交接期间保持保留。
    predecessor: VulcanPluginDescriptor,
}

/// Prepare source under exact approval and request_id without publishing a catalog or overwriting predecessor bytes.
/// 按精确批准及 request_id 准备 source，不发布目录或覆盖前驱字节。
/// Runtime owns the catalog, submitted secrets satisfy its inspected requirements; returns the durable request owner.
/// runtime 拥有目录，提交秘密满足其已检查要求；返回持久请求所有者。
/// Configuration supplies the original retained user policy and must agree with the predecessor catalog.
/// configuration 提供原保留用户策略，必须与前驱目录一致。
#[allow(clippy::too_many_arguments)]
fn reserve_update(
    runtime: &VulcanPluginRuntimeManager,
    configuration: &vulcan_provider_config::config::PersistedLuaSkillsConfiguration,
    source: &Path,
    fingerprint: &str,
    mut permissions: Vec<String>,
    allow_registry_dependencies: bool,
    initial_secrets: Vec<VulcanPluginInitialSecretInput>,
    request_id: uuid::Uuid,
) -> Result<pending_owner::ReservedLocalPluginUpdate, String> {
    // This exact preview binds authorization and secret input validation before any package preparation.
    // 此精确预览在任何插件包准备前绑定授权及秘密输入校验。
    let preview = runtime
        .inspect_local_user_plugin(source)
        .map_err(|error| error.to_string())?;
    if preview.content_fingerprint != fingerprint {
        return Err(String::from("本地插件包在更新授权检查后发生了变化。"));
    }
    permissions.sort();
    if permissions.windows(2).any(|pair| pair[0] == pair[1]) || permissions != preview.permissions {
        return Err(String::from("更新必须精确批准候选声明的完整权限集合。"));
    }
    // Snapshot ownership keeps the predecessor independent from later directory observations.
    // 快照所有权使前驱独立于后续目录观测。
    let predecessor = runtime
        .snapshot()
        .plugins
        .get(&preview.plugin_id)
        .cloned()
        .ok_or_else(|| format!("用户插件 `{}` 尚未安装。", preview.plugin_id))?;
    if predecessor.origin != vulcan_plugin_runtime::VulcanPluginPackageOrigin::UserInstalled {
        return Err(String::from("内置插件不能通过本地用户更新入口替换。"));
    }
    if !matches!(
        predecessor.runtime_state,
        VulcanPluginRuntimeState::Enabled | VulcanPluginRuntimeState::Disabled
    ) || (predecessor.runtime_state == VulcanPluginRuntimeState::Disabled
        && (runtime
            .active_session_count(&predecessor.id)
            .map_err(|error| error.to_string())?
            != 0
            || runtime
                .has_cleanup_diagnostics(&predecessor.id)
                .map_err(|error| error.to_string())?))
        || (predecessor.runtime_state == VulcanPluginRuntimeState::Enabled
            && !predecessor.desired_enabled)
    {
        return Err(String::from(
            "更新前驱必须为稳定启用状态，或已完全排空的停用状态。",
        ));
    }
    // Freeze only this plugin's exact original override membership and manifest baseline.
    // 仅固定此插件的精确原覆盖成员关系及清单基线。
    let update_policy = VulcanPluginUpdatePolicy {
        explicitly_enabled: configuration
            .enabled_vulcan_plugins
            .contains(&predecessor.id),
        explicitly_disabled: configuration
            .disabled_vulcan_plugins
            .contains(&predecessor.id),
        predecessor_default_enabled: predecessor
            .manifest
            .as_ref()
            .ok_or("原插件缺少有效清单。")?
            .default_enabled,
    };
    if update_policy.desired_enabled() != predecessor.desired_enabled {
        return Err(String::from(
            "插件用户策略与当前运行时快照不一致，请先刷新后再更新。",
        ));
    }
    // Legacy enabled packages lack immutable old selection authority; reject before creating any store or intent.
    // 旧格式启用包缺少不可变旧选择权威；在创建任何存储或意图前拒绝。
    if predecessor.runtime_state == VulcanPluginRuntimeState::Enabled
        && predecessor.managed_package().is_none()
    {
        return Err(String::from(ENABLED_PREDECESSOR_RESTORATION_REQUIRED));
    }
    if predecessor.content_fingerprint == fingerprint {
        return Err(String::from("更新候选与已安装插件内容相同。"));
    }
    // Explicit local installation inputs are checked before secure-store mutation.
    // 在安全存储变更之前检查明确的本地安装输入。
    let supplied = validate_initial_plugin_mcp_secrets(&preview, initial_secrets)?;
    // The inspected manifest decides whether network dependency installation was explicitly approved.
    // 已检查清单决定是否已明确批准网络依赖安装。
    let source_package = vulcan_plugin_runtime::inspect_user_plugin_package(source)?;
    let manifest = source_package
        .manifest
        .as_ref()
        .ok_or("更新候选缺少有效清单。")?;
    if source_package.content_fingerprint != fingerprint {
        return Err(String::from("更新候选在权限校验期间发生了变化。"));
    }
    if manifest.execution.dependencies.install_mode
        == vulcan_plugin_runtime::VulcanPluginDependencyInstallMode::Registry
        && !allow_registry_dependencies
    {
        return Err(String::from("更新候选需要明确批准网络依赖安装。"));
    }
    // Store preparation uses immutable unique paths and cannot replace the currently selected package.
    // 存储准备使用不可变唯一路径，不能替换当前所选插件包。
    let store = VulcanPluginGenerationStore::open(runtime.user_plugin_root())
        .map_err(|error| error.to_string())?;
    // Refuse a second preparation before allocating more candidate packages or secure entries.
    // 在分配更多候选插件包或安全条目之前，拒绝第二次准备。
    for pending_id in store
        .activation_transaction_ids(&predecessor.id)
        .map_err(|error| error.to_string())?
    {
        // Inspection never selects a replacement authority or starts recovery.
        // 检查绝不选择替代权威或启动恢复。
        let record = store
            .activation_request(&predecessor.id, pending_id)
            .map_err(|error| error.to_string())?
            .ok_or_else(|| String::from("原更新记录在检查期间消失。"))?;
        if !matches!(
            record.phase(),
            vulcan_plugin_runtime::VulcanPluginActivationTransactionPhase::Completed
                | vulcan_plugin_runtime::VulcanPluginActivationTransactionPhase::Aborted
        ) {
            return Err(format!(
                "插件已有未完成更新请求 `{pending_id}`，请先检查该原请求。"
            ));
        }
    }
    // Compare the full original selection before allocating or copying any new credential authority.
    // 分配或复制任何新凭据权威之前，比较完整原选择。
    let selected = store
        .activation(&predecessor.id)
        .map_err(|error| error.to_string())?;
    if !match (predecessor.managed_package(), selected.as_ref()) {
        (None, None) => true,
        (Some(package), Some(activation)) => package.matches_activation(activation),
        _ => false,
    } {
        return Err(String::from(
            "插件持久选择与当前目录不一致，请先刷新并核对原更新请求。",
        ));
    }
    // Historical managed selections may predate full configuration or credential revisions.
    // 历史受管选择可能早于完整配置或凭据修订。
    validate_enabled_predecessor_restoration_selection(&predecessor, selected.as_ref())?;
    // Persist the original request and both physical resource identities before copying or secure-store writes.
    // 复制或安全存储写入之前，持久保存原请求及两个物理资源身份。
    let reservation = store
        .reserve_activation_update_preparation(
            request_id,
            source,
            fingerprint,
            selected.as_ref(),
            permissions,
            update_policy,
        )
        .map_err(|error| error.to_string())?;
    Ok(pending_owner::ReservedLocalPluginUpdate {
        store,
        predecessor,
        reservation,
        source: pending_owner::ApprovedLocalPluginUpdateSource {
            path: source.to_path_buf(),
            supplied,
        },
    })
}

/// Materialize the same borrowed early pipeline for existing internal fixtures.
/// 为既有内部夹具物化相同借用式早期流水线。
/// Production registers before publication; this helper supplies no production owner.
/// 生产在发布之前注册；此辅助方法不提供生产所有者。
#[cfg(test)]
#[allow(clippy::too_many_arguments)]
fn prepare_update(
    runtime: &VulcanPluginRuntimeManager,
    configuration: &vulcan_provider_config::config::PersistedLuaSkillsConfiguration,
    source: &Path,
    fingerprint: &str,
    permissions: Vec<String>,
    allow_registry_dependencies: bool,
    initial_secrets: Vec<VulcanPluginInitialSecretInput>,
    request_id: uuid::Uuid,
) -> Result<PreparedLocalPluginUpdate, String> {
    // Fixture uses actual resource types and the exact production transfer operations.
    // 夹具使用实际资源类型及精确生产移交操作。
    let reserved = reserve_update(
        runtime,
        configuration,
        source,
        fingerprint,
        permissions,
        allow_registry_dependencies,
        initial_secrets,
        request_id,
    )?;
    let source = pending_owner::ApprovedLocalPluginUpdateSource {
        path: reserved.source.path.clone(),
        supplied: reserved.source.supplied.clone(),
    };
    let mut resources = pending_owner::PendingLocalPluginUpdateResources {
        runtime: runtime.clone(),
        authority: runtime.register_native_mcp_host_adapter(),
        stage: pending_owner::PendingLocalPluginUpdateStage::Reserved(reserved),
        source: Some(source),
        configuration: None,
        values: None,
        inventory: None,
        candidate: None,
        target: None,
    };
    pending_owner::materialize_preparation(&mut resources)?;
    match resources.stage {
        pending_owner::PendingLocalPluginUpdateStage::Prepared(prepared) => Ok(prepared),
        _ => Err(String::from("测试准备未完成实际晋级。")),
    }
}

/// Persist the frozen target policy only if this plugin's current overrides still equal the original snapshot.
/// 仅此插件当前覆盖仍等于原快照时，持久保存固定目标策略。
/// State owns the serialized configuration writer; prepared owns the original request. Other plugin changes are preserved.
/// state 拥有串行配置写入器；prepared 拥有原请求。保留其他插件的变更。
/// Returns confirmation or a conflict before configuration writes and candidate callbacks.
/// 返回确认，或在配置写入及候选回调前返回冲突。
async fn persist_update_policy(
    state: &SharedServiceBridgeState,
    prepared: &PreparedLocalPluginUpdate,
) -> Result<(), String> {
    // Newly prepared updates must carry their policy; legacy recovery has a separate declared path.
    // 新准备更新必须携带策略；旧恢复具有独立声明路径。
    let policy = *prepared
        .transaction
        .record()
        .update_policy()
        .ok_or("原更新请求缺少用户策略，不能继续候选执行。")?;
    // Use the durable target namespace instead of mutable catalog discovery.
    // 使用持久目标命名空间，而非可变目录发现。
    let plugin_id = prepared.transaction.record().target().plugin_id.clone();
    mutate_provider_configuration(
        state,
        "保存原更新请求的插件策略失败",
        move |configuration| {
            if !policy.matches_overrides(
                configuration
                    .luaskills
                    .enabled_vulcan_plugins
                    .contains(&plugin_id),
                configuration
                    .luaskills
                    .disabled_vulcan_plugins
                    .contains(&plugin_id),
            ) {
                return Err(String::from(
                    "更新准备期间此插件用户策略已改变；保留原请求，不覆盖新设置。",
                ));
            }
            // The explicit update target remains stable across either manifest baseline and process restart.
            // 明确更新目标跨任一清单基线及进程重启保持稳定。
            if policy.desired_enabled() {
                configuration
                    .luaskills
                    .enabled_vulcan_plugins
                    .insert(plugin_id.clone());
                configuration
                    .luaskills
                    .disabled_vulcan_plugins
                    .remove(&plugin_id);
            } else {
                configuration
                    .luaskills
                    .disabled_vulcan_plugins
                    .insert(plugin_id.clone());
                configuration
                    .luaskills
                    .enabled_vulcan_plugins
                    .remove(&plugin_id);
            }
            Ok(())
        },
        |_, _| (),
    )
    .await
}

/// Validate and install the original prepared target, preserving public state and durable callback evidence.
/// 校验并安装原准备目标，保留公开状态及持久回调证据。
/// Runtime and prepared supply the exact manager and owned original request; normal results return both owner and outcome.
/// runtime 及 prepared 提供精确管理器与自有原请求；正常结果同时返回所有者与执行结果。
/// Only a lost blocking worker returns an outer error; callers must inspect evidence before any compensation.
/// 仅阻塞工作线程丢失返回外层错误；调用方必须先检查证据，再决定任何补偿。
#[cfg(test)]
async fn validate_and_install(
    runtime: &VulcanPluginRuntimeManager,
    prepared: PreparedLocalPluginUpdate,
) -> Result<(PreparedLocalPluginUpdate, Result<(), String>), String> {
    // Return the original owner even when borrowed inspection or the Lua probe reports a normal error.
    // 即使借用检查或 Lua 探测返回正常错误，也返回原所有者。
    let probe = async {
        // The exact borrowed view fences publication during the original Lua probe.
        // 精确借用视图在原 Lua 探测期间阻止发布。
        let candidate = prepared
            .store
            .inspect_activation_candidate(&prepared.transaction)
            .map_err(|error| error.to_string())?;
        runtime
            .probe_activation_candidate(&candidate)
            .await
            .map_err(|error| error.to_string())
    }
    .await;
    if let Err(error) = probe {
        return Ok((prepared, Err(error)));
    }
    // Move actual ownership, not a borrowed path, into the blocking preparation worker.
    // 将实际所有权而非借用路径移入阻塞准备工作线程。
    let preparing_runtime = runtime.clone();
    // Even observer cancellation cannot release the request while dependencies or secure reads are still running.
    // 即使观察者取消，依赖准备或安全读取仍运行时也不能释放请求。
    let (prepared, configs) =
        run_extension_management_blocking("prepare immutable candidate MCP", move || {
            // Normal preparation failures return alongside the owner; worker loss remains an outer error.
            // 正常准备失败与所有者一起返回；工作线程丢失仍为外层错误。
            let configs = (|| {
                // Reinspect the same owned transaction; never select a newer public catalog entry.
                // 重新检查同一自有事务；绝不选择更新的公开目录条目。
                let candidate = prepared
                    .store
                    .inspect_activation_candidate(&prepared.transaction)
                    .map_err(|error| error.to_string())?;
                // This original owner retains package, environment and credentials across blocking work.
                // 此原所有者在阻塞工作期间保留插件包、环境及凭据。
                let mcp_candidate = preparing_runtime
                    .prepare_mcp_activation_candidate(&candidate)
                    .map_err(|error| error.to_string())?;
                // Secure-store materialization also runs outside the asynchronous executor thread.
                // 安全存储物化也在异步执行器线程之外运行。
                candidate_local_mcp_server_configs(mcp_candidate, &OsPluginMcpSecretStore)
            })();
            Ok((prepared, configs))
        })
        .await?;
    // Readiness and installation failures retain the exact owner for the caller's controlled compensation.
    // 就绪及安装失败为调用方受控补偿保留精确所有者。
    let validation = async {
        CandidatePluginMcpRuntime::start(configs?)?
            .validate_and_release()
            .await?;
        // Installation consumes the same original authority after actual MCP shutdown and release.
        // 实际 MCP 关闭及释放后，安装消费同一原权威。
        let candidate = prepared
            .store
            .inspect_activation_candidate(&prepared.transaction)
            .map_err(|error| error.to_string())?;
        runtime
            .install_activation_candidate(&candidate)
            .await
            .map_err(|error| error.to_string())
    }
    .await;
    Ok((prepared, validation))
}

/// Validate and install by borrowing the retained original owner rather than moving it into either executor.
/// 通过借用保留原所有者校验及安装，而非将其移入任一执行器。
/// owner retains the transaction and real MCP controller through cancellation, panic, and join failure.
/// owner 在取消、panic 及联接故障期间保留事务与真实 MCP 控制对象。
/// Returns the original validation result without replaying an existing readiness attempt.
/// 返回原验证结果，不重放既有就绪尝试。
async fn validate_and_install_retained(
    owner: &Arc<pending_owner::PendingLocalPluginUpdate>,
) -> Result<(), String> {
    {
        // The async exclusive borrow can cross the Lua await; the owning slot itself remains in service state.
        // 异步排他借用可跨越 Lua await；所有者槽本身仍在服务状态中。
        let resources = owner.resources.lock().await;
        if resources.candidate.is_some() {
            return Err(String::from("原更新已有就绪尝试；不能重新探测或启动候选。"));
        }
        // Inspect the same transaction and never borrow a candidate from latest public discovery.
        // 检查相同事务，绝不从最新公开发现借用候选。
        let candidate = resources
            .prepared()?
            .store
            .inspect_activation_candidate(&resources.prepared()?.transaction)
            .map_err(|error| error.to_string())?;
        resources
            .runtime
            .probe_activation_candidate(&candidate)
            .await
            .map_err(|error| error.to_string())?;
    }
    // Release the async borrow before the blocking worker acquires this same slot; the registry still owns it.
    // 阻塞工作线程取得相同槽之前释放异步借用；注册表仍拥有它。
    let preparing_owner = Arc::clone(owner);
    // A worker panic releases only its borrow, never the actual original transaction or request lock.
    // 工作线程 panic 仅释放其借用，绝不释放实际原事务或请求锁。
    let configs =
        run_extension_management_blocking("prepare retained immutable candidate MCP", move || {
            // No entire Prepared value moves out of this slot, including on secure-store materialization failure.
            // 包括安全存储物化失败在内，整个 Prepared 值均不移出此槽。
            let resources = preparing_owner.resources.blocking_lock();
            let candidate = resources
                .prepared()?
                .store
                .inspect_activation_candidate(&resources.prepared()?.transaction)
                .map_err(|error| error.to_string())?;
            let mcp_candidate = resources
                .runtime
                .prepare_mcp_activation_candidate(&candidate)
                .map_err(|error| error.to_string())?;
            candidate_local_mcp_server_configs(mcp_candidate, &OsPluginMcpSecretStore)
        })
        .await?;
    // Retain real control synchronously before its first await, so executor cancellation cannot discard it.
    // 首次 await 之前同步保留真实控制，使执行器取消不能丢弃它。
    let mut resources = owner.resources.lock().await;
    resources.candidate = Some(CandidatePluginMcpRuntime::start(configs)?);
    resources
        .candidate
        .as_ref()
        .expect("retained original candidate")
        .validate_and_release_retained()
        .await?;
    // Successful validation awaited actual release; its non-owning controller still retains exact startup diagnostics.
    // 成功校验已等待实际释放；非拥有资源的控制对象仍保留精确启动诊断。
    // Keep that real attempt until confirmed completion or acknowledged abort instead of inventing a shadow phase.
    // 保留此真实尝试直到已确认完成或已确认中止，而非虚构影子阶段。
    // Installation borrows exactly the same request after actual candidate transport retirement.
    // 实际候选传输退役后，安装借用精确相同请求。
    let candidate = resources
        .prepared()?
        .store
        .inspect_activation_candidate(&resources.prepared()?.transaction)
        .map_err(|error| error.to_string())?;
    resources
        .runtime
        .install_activation_candidate(&candidate)
        .await
        .map_err(|error| error.to_string())
}

/// Execute one approved local plugin update through immutable preparation and the original durable selection.
/// 经不可变准备及原持久选择，执行单个已批准本地插件更新。
/// App/state own the sole service publisher; inputs are the explicitly approved local package request.
/// app/state 拥有唯一服务发布器；输入为已明确批准的本地插件包请求。
/// Return the committed catalog projection; after reservation, failures retain the original owner and request.
/// 返回已提交目录投影；预留成功后的失败保留原所有者及请求。
/// Earlier admission failures create no reservation; old package directories are never renamed.
/// 更早的入场失败不创建预留；绝不重命名旧插件包目录。
pub(super) async fn update(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    source_path: String,
    fingerprint: String,
    permissions: Vec<String>,
    allow_registry_dependencies: bool,
    initial_secrets: Vec<VulcanPluginInitialSecretInput>,
) -> Result<LuaSkillsCatalogUpdate, String> {
    {
        // A new update is an ordinary writer until it owns its explicit request, so reject pinned Full before effects.
        // 新更新在拥有其明确请求前属于普通写入器，因此副作用前拒绝固定 Full。
        let guard = state.lock().map_err(|error| error.to_string())?;
        crate::service_state_access::require_plugin_update_full_publication_admission(
            &guard, None,
        )?;
    }
    // Allocate once before any package, credential or callback side effect.
    // 在任何插件包、凭据或回调副作用之前仅分配一次。
    let request_id = uuid::Uuid::new_v4();
    // A failed long-running RPC can still identify the same durable request.
    // 失败的长时间 RPC 仍能标识相同持久请求。
    let result = async {
        // One actual service observation captures the current manager together with its original native issuer.
        // 单次实际服务观察一起捕获当前管理器及其原原生签发者。
        let (runtime, authority) = service_vulcan_plugin_native_mcp_host(state)?;
        // Admission and the registry share one authoritative service-state observation.
        // 准入与注册表共享单次权威服务状态观察。
        let registry = pending_owner::admitted_registry(state)?;
        // Clone only access to the original manager before moving filesystem preparation to its existing worker.
        // 仅克隆原管理器访问，再将文件系统准备移入既有工作线程。
        let preparing_runtime = runtime.clone();
        // Capture the authoritative configuration before the original request is created or copying starts.
        // 原请求创建或复制开始前捕获权威配置。
        let original_configuration = provider_configuration_from_state(state)?;
        // Preparation completes inside the service-owned management task even if its external waiter disappears.
        // 即使外部等待者消失，准备也在服务自有管理任务内完成。
        let retaining_registry = Arc::clone(&registry);
        // The worker retains the actual owner before its return can be lost or its observer can disappear.
        // 工作线程在其返回可能丢失或观察者可能消失之前，保留实际所有者。
        let owner =
            run_extension_management_blocking("prepare immutable plugin update", move || {
                // Do not replace an original terminal index whose successful writer still lacks observer confirmation.
                // 原终态索引的成功写入器仍缺少观察确认时，不替换该索引。
                let preview = preparing_runtime
                    .inspect_local_user_plugin(Path::new(&source_path))
                    .map_err(|error| error.to_string())?;
                retaining_registry.require_namespace_unowned(&preview.plugin_id)?;
                let reserved = reserve_update(
                    &preparing_runtime,
                    &original_configuration.luaskills,
                    Path::new(&source_path),
                    &fingerprint,
                    permissions,
                    allow_registry_dependencies,
                    initial_secrets,
                    request_id,
                )?;
                // Insert the actual reservation before its first journal or package effect.
                // 在首次日志或插件包副作用之前插入实际预留。
                let owner =
                    retaining_registry.retain_reserved(preparing_runtime, authority, reserved)?;
                pending_owner::materialize_preparation(&mut owner.resources.blocking_lock())?;
                Ok(owner)
            })
            .await?;
        continue_prepared_update(app, state, &registry, &owner).await
    }
    .await;
    result.map_err(|error| format!("插件更新请求 `{request_id}` 返回错误：{error}。请检查原请求；不会自动重放安装或回滚已提交选择。"))
}

/// Continue the original configured request once, using the first retained runtime and actual candidate gates.
/// 使用首次保留运行时及实际候选门控，继续原已配置请求一次。
/// Early recovery reaches this path only before any candidate attempt exists.
/// 早期恢复仅在任何候选尝试存在之前到达此路径。
async fn continue_prepared_update(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    registry: &pending_owner::PendingLocalPluginUpdateRegistry,
    owner: &Arc<pending_owner::PendingLocalPluginUpdate>,
) -> Result<LuaSkillsCatalogUpdate, String> {
    let runtime = owner.resources.lock().await.runtime.clone();
    // The exact predecessor, not a later candidate scan, identifies the plugin being updated.
    // 精确前驱而非后续候选扫描，标识正在更新的插件。
    let plugin_id = owner.plugin_id.clone();
    // An executor borrows this strong slot; panic does not drop the original prepared request.
    // 执行器借用此强槽；panic 不会销毁原准备请求。
    let resources = owner.resources.lock().await;
    // Capture existing access once so the final service publication cannot mix runtime generations.
    // 仅捕获既有访问一次，避免最终服务发布混用运行时代次。
    let current_mcp = state
        .lock()
        .map_err(|error| format!("锁定服务状态失败：{error}"))?
        .agent_runtime
        .as_ref()
        .map(|bridge| bridge.extension_generation.mcp_runtime())
        .ok_or_else(|| String::from("Agent 扩展代次尚未初始化。"))?;
    // The enabled path closes the exact original generation and runs its owned lifecycle receipts.
    // 启用路径关闭精确原代次，并运行其自有生命周期回执。
    if resources
        .prepared()?
        .transaction
        .record()
        .update_policy()
        .is_some_and(VulcanPluginUpdatePolicy::desired_enabled)
    {
        drop(resources);
        return target_handoff::continue_enabled_update(app, state, registry, owner, current_mcp)
            .await;
    }
    // Close starts before observing originals, including unpublished or removed owner instances.
    // 观察原实例前关闭启动，包括尚未发布或已经移除的所有者实例。
    current_mcp.begin_plugin_mcp_drain(&plugin_id)?;
    current_mcp
        .wait_plugin_mcp_drain(
            &plugin_id,
            AgentRuntimeBridge::RETIRED_MCP_PARTITION_DRAIN_TIMEOUT,
        )
        .await?;
    // Prove original Lua calls and sessions retired before admitting candidate installation effects.
    // 在接纳候选安装副作用之前，证明原 Lua 调用及会话已经退役。
    let drained = runtime
        .drain_plugin_sessions(&plugin_id)
        .await
        .map_err(|error| error.to_string())?;
    if !drained.failures.is_empty() {
        return Err(format!(
            "原插件仍有未退役资源：{}",
            drained.failures.join("；")
        ));
    }
    persist_update_policy(state, resources.prepared()?).await?;
    // The retained blocking worker needs the same slot, so release only this borrow before waiting.
    // 保留阻塞工作线程需要相同槽，因此在等待前仅释放此借用。
    drop(resources);
    validate_and_install_retained(owner).await?;
    // Validate lifecycle admission again after actual readiness and installation have finished.
    // 实际就绪及安装结束后，再次校验生命周期准入。
    pending_owner::admitted_registry(state)?;
    // Original ownership remains strongly retained across every commit and publisher failure.
    // 每次提交及发布器失败期间，原所有权均保持强保留。
    let mut resources = owner.resources.lock().await;
    pending_owner::require_current_manager(state, &resources)?;
    let prepared = resources.prepared_mut()?;
    // Keep the existing configuration writer excluded through selection commit and route publication.
    // 在选择提交及路由发布期间，持续排除既有配置写入器。
    let policy_publication =
        crate::service_state_access::begin_provider_configuration_transaction().await;
    // Recheck after readiness, which may have waited while other user edits completed.
    // 就绪可能等待其他用户编辑完成，因此在其后重新检查。
    let configuration = provider_configuration_from_state(state)?;
    validate_disabled_update_publication_policy(
        prepared.transaction.record(),
        &configuration.luaskills,
    )?;
    prepared
        .store
        .commit_activation_transaction(&mut prepared.transaction)
        .map_err(|error| error.to_string())?;
    // Durable selection is committed; failures below must resume publication rather than restore old bytes.
    // 持久选择已提交；以下失败必须恢复发布，而不是恢复旧字节。
    // The persisted disabled policy prevents the candidate from acquiring business admission during publication.
    // 持久停用策略防止候选在发布期间取得业务入场。
    {
        // The original transaction supplies the only exemption before a disabled update changes core catalog identity.
        // 停用更新改变核心目录身份前，原事务提供唯一豁免。
        let guard = state.lock().map_err(|error| error.to_string())?;
        crate::service_state_access::require_plugin_update_full_publication_admission(
            &guard,
            Some(prepared.transaction.record().request_id()),
        )?;
    }
    let catalog = runtime
        .refresh_catalog(
            &configuration.luaskills.disabled_vulcan_plugins,
            &configuration.luaskills.enabled_vulcan_plugins,
        )
        .map_err(|error| error.to_string())?;
    validate_published_catalog(&runtime, &catalog, prepared.transaction.record().target())?;
    synchronize_service_vulcan_plugin_hooks(state, runtime.clone(), &catalog)?;
    vulcan_plugin_lifecycle::publish_disabled_plugin_extension_generation(
        state, &plugin_id, catalog,
    )?;
    prepared
        .store
        .complete_activation_transaction(&mut prepared.transaction)
        .map_err(|error| error.to_string())?;
    drop(policy_publication);
    retire_candidate_control(&mut resources).await?;
    // Remove only this original confirmed terminal slot, after all candidate resources really retired.
    // 全部候选资源实际退役后，仅移除此原已确认终态槽。
    registry.retire(owner, &resources)?;
    drop(resources);
    refresh_plugin_catalog_local_state_and_emit(app, state).await
}

/// Drop the retained original readiness controller only after actual release and an explicit terminal decision.
/// 仅在实际释放及明确终态决定之后销毁保留原就绪控制对象。
/// resources is the same concrete slot; no shutdown, retry, new attempt or readiness success is fabricated.
/// resources 为相同具体槽；不虚构关闭、重试、新尝试或就绪成功。
async fn retire_candidate_control(
    resources: &mut pending_owner::PendingLocalPluginUpdateResources,
) -> Result<(), String> {
    if let Some(candidate) = resources.candidate.as_ref() {
        candidate
            .observer()
            .wait_until_released(AgentRuntimeBridge::RETIRED_MCP_PARTITION_DRAIN_TIMEOUT)
            .await?;
        resources.candidate = None;
    }
    Ok(())
}
