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

#[cfg(test)]
pub(super) mod tests;

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
    phase: vulcan_plugin_runtime::VulcanPluginActivationTransactionPhase,
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
        phase: record.phase(),
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
/// Catalog and target come from the original request; returns an explicit error instead of completing a diagnostic row.
/// catalog 及 target 来自原请求；返回明确错误，而不把诊断条目标记为完成。
fn validate_published_catalog(
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
        || plugin.active_session_count != 0
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
    // Reuse only the service's original manager and original immutable request.
    // 仅复用服务原管理器及原不可变请求。
    let runtime = service_vulcan_plugin_runtime(state)?;
    // Canonical identity never comes from directory ordering or latest-version inference.
    // 规范身份绝不来自目录顺序或最新版本推断。
    let request_id = request_identity(request_id)?;
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
    // Capture the one service-owned MCP access before publishing its disabled replacement.
    // 发布停用替换之前，捕获唯一服务持有的 MCP 访问。
    let current_mcp = state
        .lock()
        .map_err(|error| format!("锁定服务状态失败：{error}"))?
        .agent_runtime
        .as_ref()
        .map(|bridge| bridge.extension_generation.mcp_runtime())
        .ok_or_else(|| String::from("Agent 扩展代次尚未初始化。"))?;
    // No candidate callback is run during this readback-and-route operation.
    // 此回读及路由操作期间不运行候选回调。
    let catalog = runtime
        .refresh_catalog_for_activation_recovery(
            &transaction,
            &configuration.luaskills.disabled_vulcan_plugins,
            &configuration.luaskills.enabled_vulcan_plugins,
        )
        .map_err(|error| error.to_string())?;
    validate_published_catalog(&catalog, transaction.record().target())?;
    synchronize_service_vulcan_plugin_hooks(state, runtime.clone(), &catalog)?;
    vulcan_plugin_lifecycle::publish_extension_generation(
        state,
        plugin_id,
        catalog,
        remove_plugin_mcp_runtime_partition(&current_mcp, plugin_id),
    )?;
    store
        .complete_activation_transaction(&mut transaction)
        .map_err(|error| error.to_string())?;
    drop(policy_publication);
    refresh_plugin_catalog_local_state_and_emit(app, state).await
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
fn prepare_update(
    runtime: &VulcanPluginRuntimeManager,
    configuration: &vulcan_provider_config::config::PersistedLuaSkillsConfiguration,
    source: &Path,
    fingerprint: &str,
    mut permissions: Vec<String>,
    allow_registry_dependencies: bool,
    initial_secrets: Vec<VulcanPluginInitialSecretInput>,
    request_id: uuid::Uuid,
) -> Result<PreparedLocalPluginUpdate, String> {
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
    if predecessor.runtime_state != VulcanPluginRuntimeState::Disabled
        || predecessor.active_session_count != 0
    {
        return Err(String::from(
            "用户插件必须先停用并完全排空，再执行此更新入口。",
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
    // Persist the original request and both physical resource identities before copying or secure-store writes.
    // 复制或安全存储写入之前，持久保存原请求及两个物理资源身份。
    let preparation = store
        .begin_activation_update_preparation(
            request_id,
            source,
            fingerprint,
            selected.as_ref(),
            permissions,
            update_policy,
        )
        .map_err(|error| error.to_string())?;
    // This handle owns the copied package and is used by every subsequent target decision.
    // 此句柄拥有复制包，并用于后续每个目标决定。
    let generation = store
        .prepare_activation_package(&preparation, source)
        .map_err(|error| error.to_string())?;
    // Complete configuration is selected before any callback can observe the candidate.
    // 在任何回调可以观测候选之前，选择完整配置。
    let configuration = runtime
        .configuration_for_update(&generation, &predecessor)
        .map_err(|error| error.to_string())?;
    // Credential slots are derived only from the verified immutable copy.
    // 凭据槽位仅从已验证不可变副本派生。
    let candidate = vulcan_plugin_runtime::inspect_user_plugin_package(generation.package_root())?;
    // Secure reads never probe another version when an exact selected slot is absent.
    // 精确选定槽位缺失时，安全读取绝不探测另一版本。
    let secrets = OsPluginMcpSecretStore;
    // Every declaration has an explicit configured-or-absent value; no undeclared values are copied.
    // 每个声明都有明确的已配置或缺失值；不复制未声明值。
    let mut values = VulcanPluginCredentialValues::new();
    for server in &candidate.mcp_servers {
        for secret_key in server.manifest.declared_secret_keys() {
            // Exact declared identity is shared by staging inventory and backend account selection.
            // 暂存清单及后端账户选择共用精确声明身份。
            let key = VulcanPluginCredentialKey {
                server_id: server.id.clone(),
                secret_key,
            };
            // New local values are explicit; preserved remote slots must be declared in the predecessor.
            // 新本地值为明确输入；保留的远程槽位必须在前驱中声明。
            let value = if let Some(value) =
                supplied.get(&(key.server_id.clone(), key.secret_key.clone()))
            {
                Some(value.clone())
            } else if let Some(previous_server) = predecessor.mcp_servers.iter().find(|previous| {
                previous.id == server.id
                    && previous
                        .manifest
                        .declared_secret_keys()
                        .contains(&key.secret_key)
            }) {
                secrets.read_selected_optional_secret(
                    &predecessor.id,
                    &key.server_id,
                    &key.secret_key,
                    &previous_server.credential_binding,
                )?
            } else {
                None
            };
            values.insert(key, value);
        }
    }
    // Staging owns its original revision until every secure write has been verified.
    // 暂存在每次安全写入均已验证之前拥有原修订。
    let mut staging = store
        .begin_activation_credential_staging(&preparation, &generation, &values)
        .map_err(|error| error.to_string())?;
    staging
        .write_and_finish(&secrets, &values)
        .map_err(|error| error.to_string())?;
    drop(staging);
    // Bind the complete configured target after validating the immutable copy and exact staged credentials.
    // 校验不可变副本及精确已暂存凭据之后，绑定完整已配置目标。
    let transaction = store
        .finish_activation_preparation(preparation, configuration)
        .map_err(|error| error.to_string())?;
    if transaction.record().predecessor() != selected.as_ref() {
        return Err(format!(
            "插件原选择在准备期间发生变化；保留请求 `{request_id}`，不得继续候选执行。"
        ));
    }
    Ok(PreparedLocalPluginUpdate {
        store,
        transaction,
        predecessor,
    })
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

/// Execute the existing disabled-plugin update through immutable preparation and the original durable selection.
/// 经不可变准备及原持久选择，执行既有已停用插件更新。
/// App/state own the sole service publisher; inputs are the explicitly approved local package request.
/// app/state 拥有唯一服务发布器；输入为已明确批准的本地插件包请求。
/// Returns the committed catalog projection; any failure retains its request and never renames old package directories.
/// 返回已提交目录投影；任何失败均保留请求，绝不重命名旧插件包目录。
pub(super) async fn update(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    source_path: String,
    fingerprint: String,
    permissions: Vec<String>,
    allow_registry_dependencies: bool,
    initial_secrets: Vec<VulcanPluginInitialSecretInput>,
) -> Result<LuaSkillsCatalogUpdate, String> {
    // Allocate once before any package, credential or callback side effect.
    // 在任何插件包、凭据或回调副作用之前仅分配一次。
    let request_id = uuid::Uuid::new_v4();
    // A failed long-running RPC can still identify the same durable request.
    // 失败的长时间 RPC 仍能标识相同持久请求。
    let result = async {
        // The existing service manager remains the only runtime owner.
        // 既有服务管理器仍为唯一运行时所有者。
        let runtime = service_vulcan_plugin_runtime(state)?;
        // Clone only access to the original manager before moving filesystem preparation to its existing worker.
        // 仅克隆原管理器访问，再将文件系统准备移入既有工作线程。
        let preparing_runtime = runtime.clone();
        // Capture the authoritative configuration before the original request is created or copying starts.
        // 原请求创建或复制开始前捕获权威配置。
        let original_configuration = provider_configuration_from_state(state)?;
        // Preparation completes inside the service-owned management task even if its external waiter disappears.
        // 即使外部等待者消失，准备也在服务自有管理任务内完成。
        let mut prepared =
            run_extension_management_blocking("prepare immutable plugin update", move || {
                prepare_update(
                    &preparing_runtime,
                    &original_configuration.luaskills,
                    Path::new(&source_path),
                    &fingerprint,
                    permissions,
                    allow_registry_dependencies,
                    initial_secrets,
                    request_id,
                )
            })
            .await?;
        // The exact predecessor, not a later candidate scan, identifies the plugin being updated.
        // 精确前驱而非后续候选扫描，标识正在更新的插件。
        let plugin_id = prepared.predecessor.id.clone();
        // Capture existing access once so the final service publication cannot mix runtime generations.
        // 仅捕获既有访问一次，避免最终服务发布混用运行时代次。
        let current_mcp = state
            .lock()
            .map_err(|error| format!("锁定服务状态失败：{error}"))?
            .agent_runtime
            .as_ref()
            .map(|bridge| bridge.extension_generation.mcp_runtime())
            .ok_or_else(|| String::from("Agent 扩展代次尚未初始化。"))?;
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
        persist_update_policy(state, &prepared).await?;
        // Preserve the returned original authority before examining the normal validation outcome.
        // 检查正常验证结果前，保留返回的原权威。
        let (validated, validation) = validate_and_install(&runtime, prepared).await?;
        prepared = validated;
        validation?;
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
        let catalog = runtime
            .refresh_catalog(
                &configuration.luaskills.disabled_vulcan_plugins,
                &configuration.luaskills.enabled_vulcan_plugins,
            )
            .map_err(|error| error.to_string())?;
        validate_published_catalog(&catalog, prepared.transaction.record().target())?;
        synchronize_service_vulcan_plugin_hooks(state, runtime.clone(), &catalog)?;
        vulcan_plugin_lifecycle::publish_extension_generation(
            state,
            &plugin_id,
            catalog,
            remove_plugin_mcp_runtime_partition(&current_mcp, &plugin_id),
        )?;
        prepared
            .store
            .complete_activation_transaction(&mut prepared.transaction)
            .map_err(|error| error.to_string())?;
        drop(policy_publication);
        refresh_plugin_catalog_local_state_and_emit(app, state).await
    }
    .await;
    result.map_err(|error| format!("插件更新请求 `{request_id}` 返回错误：{error}。请检查原请求；不会自动重放安装或回滚已提交选择。"))
}
