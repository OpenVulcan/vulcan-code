//! Original first-install resources remain service-owned before every package, credential and native effect.
//! 原首次安装资源在每次插件包、凭据及原生副作用前保持服务所有。

use super::*;
use vulcan_plugin_runtime::{
    VulcanPluginInstallationTransaction, VulcanPluginLegacyLifecycleCall,
    VulcanPluginLegacyLifecycleEvent, VulcanPluginRuntimeManager,
};

/// Concrete original service requests have no expiry and cannot be replaced by later observations.
/// 具体原服务请求没有过期时间，不能由随后观测替换。
#[derive(Default)]
pub(crate) struct PendingLocalPluginInstallRegistry {
    /// Only confirmed package absence or full commitment permits removing the same Arc.
    /// 仅已确认插件包缺失或完整提交允许移除同一 Arc。
    owners: Mutex<BTreeMap<uuid::Uuid, Arc<PendingLocalPluginInstall>>>,
}

/// Sanitized progress identifies the original request without exposing source paths, secrets or resource handles.
/// 脱敏进度标识原请求，不暴露来源路径、秘密或资源句柄。
#[derive(Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub(crate) enum LocalPluginInstallPhase {
    /// The executor holds the exclusive resources and no effect inference is permitted.
    /// 执行器持有排他资源，不允许推断副作用。
    Executing,
    /// Actual namespace ownership is retained, before package and credential writes.
    /// 实际命名空间所有权已保留，尚未写入插件包及凭据。
    Reserved,
    /// Original package and record publication may be incomplete.
    /// 原插件包及记录发布可能不完整。
    PackagePublication,
    /// Complete original preimages precede all possibly uncertain credential writes.
    /// 完整原前像先于全部可能不确定凭据写入。
    CredentialMutation,
    /// The same original environment or native validation may require retirement.
    /// 同一原环境或原生校验可能需要退役。
    NativeValidation,
    /// The retained original lifecycle call may have effects and is never replayed.
    /// 保留原生命周期调用可能具有副作用，且绝不重放。
    Lifecycle,
    /// Prepared routes have not yet committed the full service pointer.
    /// 已准备路由尚未提交完整服务指针。
    Publishing,
    /// Original compensation is incomplete and retains its exact resources.
    /// 原补偿尚未完成，保留其精确资源。
    Aborting,
    /// Original native cleanup, policy, credential readback and package absence were confirmed.
    /// 已确认原原生清理、策略、凭据回读及插件包缺失。
    Aborted,
    /// The full pointer and this phase were confirmed adjacently before projection.
    /// 完整指针及此阶段在投影之前相邻确认。
    Committed,
}

/// Public recovery identity is the allocated original UUID, never a latest-plugin alias.
/// 公共恢复身份为已分配原 UUID，绝不是最新插件别名。
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LocalPluginInstallStatus {
    /// The core reservation supplies this exact original UUID.
    /// 核心预留提供此精确原 UUID。
    request_id: uuid::Uuid,
    /// The exact namespace remains queryable even before catalog discovery.
    /// 精确命名空间即使在目录发现之前仍可查询。
    plugin_id: String,
    /// Executing conveys only an active exclusive borrow.
    /// 执行中仅表达活动排他借用。
    phase: LocalPluginInstallPhase,
}

/// The same strong owner outlives a cancelled waiter, failed blocking worker or management panic.
/// 同一强所有者跨已取消等待者、失败阻塞工作者或管理 panic 存续。
struct PendingLocalPluginInstall {
    /// Exact immutable core allocation identifies registry removal and recovery.
    /// 精确不可变核心分配标识注册表移除及恢复。
    request_id: uuid::Uuid,
    /// Exact immutable plugin namespace selects no fallback owner.
    /// 精确不可变插件命名空间不选择备用所有者。
    plugin_id: String,
    /// A panic releases only the borrow; package guards, preimages and actual controls remain in this slot.
    /// panic 仅释放借用；插件包屏障、前像及实际控制仍位于此槽。
    resources: tokio::sync::Mutex<LocalPluginInstallResources>,
}

/// Typed original resources never serialize credentials or reconstruct native ownership from status.
/// 类型化原资源绝不序列化凭据，亦不从状态重建原生所有权。
struct LocalPluginInstallResources {
    /// The first actual runtime manager is retained independently from future service replacement.
    /// 首个实际运行时管理器独立于未来服务替换而保留。
    runtime: VulcanPluginRuntimeManager,
    /// The inert actual package owner enters this slot before its sole copy.
    /// 未启动实际插件包所有者在唯一复制之前进入此槽。
    package: VulcanPluginInstallationTransaction,
    /// Complete supplied values derive from exact permission-preview validation before retention.
    /// 完整提交值来自保留之前的精确权限预览校验。
    supplied: BTreeMap<(String, String), String>,
    /// Some contains every actual original account before any write; None means no credential write was admitted.
    /// 非空包含任何写入之前的每个实际原账户；空值表示未准入凭据写入。
    preimages: Option<BTreeMap<(String, String), Option<String>>>,
    /// Exact original override membership is retained before package publication.
    /// 精确原覆盖成员关系在插件包发布之前保留。
    original_policy: (bool, bool),
    /// Exact declared policy intent is stored before a possibly successful uncertain configuration write.
    /// 精确声明策略意图在可能已成功的不确定配置写入之前存储。
    target_policy: Option<(bool, bool)>,
    /// Actual native preparation is retained before dispatch and remains owned after a failed await.
    /// 实际原生准备在派发之前保留，并在等待失败之后仍被拥有。
    native: Option<vulcan_agent_runtime::mcp::LegacyPluginMcpReplacement>,
    /// The original installation call is retained before start, never registered again during recovery.
    /// 原安装调用在启动之前保留，恢复期间绝不重新登记。
    install: Option<VulcanPluginLegacyLifecycleCall>,
    /// Actual installation-call retirement and business outcome survive later publication failure.
    /// 实际安装调用退役及业务结果跨后续发布失败存续。
    install_result: Option<Result<(), String>>,
    /// Starting installation admits its declared compensator, even if business completion is uncertain.
    /// 启动安装准入其声明补偿，即使业务完成不确定。
    install_started: bool,
    /// The same compensating call is retained before its first start and never replayed.
    /// 同一补偿调用在首次启动之前保留，且绝不重放。
    uninstall: Option<VulcanPluginLegacyLifecycleCall>,
    /// Failed but actually retired compensation requires explicit external-effect acknowledgement.
    /// 已失败但实际已退役补偿要求明确确认外部副作用。
    uninstall_result: Option<Result<(), String>>,
    /// Progress advances before uncertain effects and beside the full commit.
    /// 进度在不确定副作用之前及完整提交旁推进。
    phase: LocalPluginInstallPhase,
}

impl PendingLocalPluginInstallRegistry {
    /// Retain the original inert package and all explicit inputs before any admitted plugin effect.
    /// 在任何已准入插件副作用之前，保留原未启动插件包及全部明确输入。
    /// runtime, package, supplied and original_policy are frozen originals; returns the same strong owner.
    /// runtime、package、supplied 及 original_policy 为固定原值；返回同一强所有者。
    fn retain(
        &self,
        runtime: VulcanPluginRuntimeManager,
        package: VulcanPluginInstallationTransaction,
        supplied: BTreeMap<(String, String), String>,
        original_policy: (bool, bool),
    ) -> Result<Arc<PendingLocalPluginInstall>, String> {
        // Registry exclusion is short and precedes all package or OS operations.
        // 注册表排他区间短暂，先于全部插件包或操作系统操作。
        let mut owners = self.owners.lock().map_err(|error| error.to_string())?;
        if owners
            .values()
            .any(|owner| owner.plugin_id == package.plugin_id())
            || owners.contains_key(&package.request_id())
        {
            return Err(String::from(
                "Plugin already has an original first-install owner.",
            ));
        }
        // This exact core allocation remains the sole public recovery identity.
        // 此精确核心分配保持为唯一公共恢复身份。
        let owner = Arc::new(PendingLocalPluginInstall {
            request_id: package.request_id(),
            plugin_id: package.plugin_id().to_owned(),
            resources: tokio::sync::Mutex::new(LocalPluginInstallResources {
                runtime,
                package,
                supplied,
                preimages: None,
                original_policy,
                target_policy: None,
                native: None,
                install: None,
                install_result: None,
                install_started: false,
                uninstall: None,
                uninstall_result: None,
                phase: LocalPluginInstallPhase::Reserved,
            }),
        });
        owners.insert(owner.request_id, Arc::clone(&owner));
        Ok(owner)
    }

    /// List exact plugin requests without waiting for active executors or consulting mutable discovery.
    /// 列出精确插件请求，不等待活动执行器，亦不查询可变发现结果。
    /// plugin_id selects only immutable originals; returns sanitized identities and phases.
    /// plugin_id 仅选择不可变原实例；返回脱敏身份及阶段。
    pub(super) fn list(&self, plugin_id: &str) -> Result<Vec<LocalPluginInstallStatus>, String> {
        // Snapshot strong owners before trying their asynchronous resource borrows.
        // 尝试异步资源借用之前，快照强所有者。
        let owners = self
            .owners
            .lock()
            .map_err(|error| error.to_string())?
            .values()
            .filter(|owner| owner.plugin_id == plugin_id)
            .cloned()
            .collect::<Vec<_>>();
        Ok(owners
            .iter()
            .map(|owner| LocalPluginInstallStatus {
                request_id: owner.request_id,
                plugin_id: owner.plugin_id.clone(),
                phase: owner
                    .resources
                    .try_lock()
                    .map(|resources| resources.phase)
                    .unwrap_or(LocalPluginInstallPhase::Executing),
            })
            .collect())
    }

    /// Refuse another same-service management mutation while the exact original installation remains retained.
    /// 精确原安装仍被保留时，拒绝同服务另一管理变更。
    /// plugin_id selects the immutable namespace; returns its actionable UUID on conflict without waiting or reading OS accounts.
    /// plugin_id 选择不可变命名空间；冲突时返回可操作 UUID，不等待或读取操作系统账户。
    /// The caller already holds the sole LuaSkills management gate; this check does not replace cross-process file fencing.
    /// 调用方已持有唯一 LuaSkills 管理门；此检查不替代跨进程文件屏障。
    pub(super) fn require_no_original(&self, plugin_id: &str) -> Result<(), String> {
        // Immutable registry identities remain available even while an executor owns its resource borrow.
        // 即使执行器持有资源借用，不可变注册表身份仍可用。
        let owners = self.owners.lock().map_err(|error| error.to_string())?;
        if let Some(owner) = owners.values().find(|owner| owner.plugin_id == plugin_id) {
            return Err(format!(
                "Plugin has retained original installation request `{}`; recover that exact request first.",
                owner.request_id,
            ));
        }
        Ok(())
    }

    /// Refuse a whole discovery refresh while an original installation still owns a private or unacknowledged committed generation.
    /// 原安装仍拥有私有或未确认已提交代次时，拒绝整个发现刷新。
    /// Returns an actionable original UUID without borrowing asynchronous resources or changing manager revisions.
    /// 返回可操作原 UUID，不借用异步资源或改变管理器修订。
    pub(super) fn require_catalog_refresh(&self) -> Result<(), String> {
        // Discovery cannot invalidate a lifecycle descriptor still owned by an exact original.
        // 发现不能使精确原实例仍拥有的生命周期描述符失效。
        let owners = self.owners.lock().map_err(|error| error.to_string())?;
        if let Some(owner) = owners.values().next() {
            return Err(format!(
                "Catalog refresh requires recovery of original installation request `{}` first.",
                owner.request_id,
            ));
        }
        Ok(())
    }

    /// Constrain ordinary full publication to the actual already-published identity of every retained installation.
    /// 将普通完整发布约束于每个保留安装实际已发布的身份。
    /// current is captured under the actual service publication lock; next is the proposed complete catalog.
    /// current 在实际服务发布锁内捕获；next 为拟发布完整目录。
    /// Returns unit for unchanged originals or an actionable UUID before any route or workbench mutation.
    /// 原实例保持未变时返回空值，否则在任何路由或工作台变更前返回可操作 UUID。
    /// This observation never treats a manager's private snapshot or a resource phase as full commitment authority.
    /// 此观测绝不将管理器私有快照或资源阶段视为完整提交权威。
    pub(crate) fn require_catalog_publication(
        &self,
        current: Option<&vulcan_plugin_runtime::VulcanPluginCatalogSnapshot>,
        next: &vulcan_plugin_runtime::VulcanPluginCatalogSnapshot,
    ) -> Result<(), String> {
        // Immutable original identities can be checked while their executor holds the resource slot.
        // 执行器持有资源槽时，不可变原身份仍可检查。
        let owners = self.owners.lock().map_err(|error| error.to_string())?;
        for owner in owners.values() {
            // Only the actual current full slot determines whether this namespace was published.
            // 仅实际当前完整槽决定此命名空间是否已发布。
            let published = current.and_then(|catalog| catalog.plugins.get(&owner.plugin_id));
            // Exact immutable namespace lookup does not consult latest-owner aliases.
            // 精确不可变命名空间查询不访问最新所有者别名。
            let proposed = next.plugins.get(&owner.plugin_id);
            match (published, proposed) {
                (None, None) => {}
                (Some(published), Some(proposed))
                    if current
                        .is_some_and(|catalog| catalog.service_epoch == next.service_epoch)
                        && published.generation == proposed.generation
                        && published.content_fingerprint == proposed.content_fingerprint
                        && published.package_root == proposed.package_root
                        && published.desired_enabled == proposed.desired_enabled
                        && published.runtime_state == proposed.runtime_state => {}
                _ => {
                    return Err(format!(
                        "Full publication conflicts with original installation request `{}`; recover that exact request first.",
                        owner.request_id,
                    ));
                }
            }
        }
        Ok(())
    }

    /// Lookup one original UUID and namespace; absence fails before any storage access.
    /// 查询单个原 UUID 及命名空间；缺失在任何存储访问前失败。
    /// Returns the retained original strong owner, never a reconstructed latest transaction.
    /// 返回保留原强所有者，绝不返回重建最新事务。
    fn get(
        &self,
        plugin_id: &str,
        request_id: uuid::Uuid,
    ) -> Result<Arc<PendingLocalPluginInstall>, String> {
        // Exact lookup excludes other namespaces and later UUIDs.
        // 精确查询排除其他命名空间及后续 UUID。
        let owner = self
            .owners
            .lock()
            .map_err(|error| error.to_string())?
            .get(&request_id)
            .cloned()
            .ok_or("Original installation request is not retained by this service.")?;
        if owner.plugin_id != plugin_id {
            return Err(String::from(
                "Original installation namespace does not match.",
            ));
        }
        Ok(owner)
    }

    /// Remove only the same strong original after confirmed package absence or adjacent full commitment.
    /// 仅已确认插件包缺失或相邻完整提交之后，移除同一强原实例。
    /// owner and resources are the original exclusive borrow; errors retain the registry slot.
    /// owner 及 resources 为原排他借用；错误保留注册表槽。
    fn retire(
        &self,
        owner: &Arc<PendingLocalPluginInstall>,
        resources: &LocalPluginInstallResources,
    ) -> Result<(), String> {
        if !matches!(
            resources.phase,
            LocalPluginInstallPhase::Aborted | LocalPluginInstallPhase::Committed
        ) {
            return Err(String::from(
                "Original installation resources are not confirmed terminal.",
            ));
        }
        // Arc equality prevents a stale executor from removing a later original.
        // Arc 相等性防止旧执行器移除后续原实例。
        let mut owners = self.owners.lock().map_err(|error| error.to_string())?;
        if !owners
            .get(&owner.request_id)
            .is_some_and(|actual| Arc::ptr_eq(actual, owner))
        {
            return Err(String::from(
                "Original installation owner changed before retirement.",
            ));
        }
        owners.remove(&owner.request_id);
        Ok(())
    }

    /// Drain retained originals before irreversible shutdown, using the same physical owners and preimages.
    /// 不可逆关闭之前，使用相同物理所有者及前像排空保留原实例。
    /// state supplies the actual full pointer and policy writer; errors preserve service resources for recovery.
    /// state 提供实际完整指针及策略写入器；错误保留服务资源以供恢复。
    pub(crate) async fn drain_for_shutdown(
        &self,
        state: &SharedServiceBridgeState,
    ) -> Result<(), String> {
        // Admission is already closed; clone owners without awaiting under the registry mutex.
        // 准入已关闭；克隆所有者，不在注册表互斥锁下等待。
        let owners = self
            .owners
            .lock()
            .map_err(|error| error.to_string())?
            .values()
            .cloned()
            .collect::<Vec<_>>();
        for owner in owners {
            // The same original resources are exclusively borrowed without transferring ownership.
            // 排他借用同一原资源，不转移所有权。
            let mut resources = owner.resources.lock().await;
            if !matches!(
                resources.phase,
                LocalPluginInstallPhase::Committed | LocalPluginInstallPhase::Aborted
            ) {
                abort_original(state, &owner.plugin_id, &mut resources, false).await?;
            }
            // A completing executor may already have retired this same confirmed owner.
            // 完成中的执行器可能已退役此相同已确认所有者。
            let retained = self
                .owners
                .lock()
                .map_err(|error| error.to_string())?
                .contains_key(&owner.request_id);
            if retained {
                self.retire(&owner, &resources)?;
            }
        }
        Ok(())
    }

    /// Refuse teardown without a runtime whenever an original physical owner remains.
    /// 存在原物理所有者时，拒绝在无运行时情况下拆卸。
    /// Returns unit only for an actually empty registry, without dropping unfinished guards.
    /// 仅实际注册表为空时返回空值，不销毁未完成屏障。
    pub(crate) fn require_empty_without_runtime(&self) -> Result<(), String> {
        if self
            .owners
            .lock()
            .map_err(|error| error.to_string())?
            .is_empty()
        {
            Ok(())
        } else {
            Err(String::from(
                "Original installation resources require asynchronous recovery before shutdown.",
            ))
        }
    }
}

/// Confirm original call cleanup before recording its immutable retired business outcome.
/// 记录不可变已退役业务结果之前，确认原调用清理。
/// call and result are fields of the same retained slot; unfinished cleanup leaves the actual handle untouched.
/// call 及 result 为同一保留槽字段；未完成清理使实际句柄保持不变。
async fn retire_call(
    call: &mut Option<VulcanPluginLegacyLifecycleCall>,
    result: &mut Option<Result<(), String>>,
) -> Result<(), String> {
    if let Some(original) = call.as_mut() {
        original
            .drain_cleanup()
            .await
            .map_err(|error| error.to_string())?;
        // No fallible or asynchronous operation separates acknowledgement from storing its result.
        // 确认及存储结果之间不插入可能失败或异步操作。
        let retired = original
            .confirm_retired()
            .map_err(|error| error.to_string())?;
        *result = Some(retired.map_err(|error| error.to_string()));
        call.take();
    }
    Ok(())
}

/// Observe exact override membership in enabled/disabled order, without guessing effective defaults.
/// 按启用及停用顺序观测精确覆盖成员关系，不猜测生效默认值。
/// configuration owns persisted membership and plugin_id selects it; returns the exact pair.
/// configuration 持有持久成员关系，plugin_id 选择它；返回精确二元组。
fn policy_membership(
    configuration: &vulcan_provider_config::config::PersistedLuaSkillsConfiguration,
    plugin_id: &str,
) -> (bool, bool) {
    (
        configuration.enabled_vulcan_plugins.contains(plugin_id),
        configuration.disabled_vulcan_plugins.contains(plugin_id),
    )
}

/// Restore exact original policy only from the declared original-or-intended outcomes of our uncertain write.
/// 仅从自身不确定写入声明的原结果或期望结果恢复精确原策略。
/// state owns the serialized writer; original and target are frozen memberships; conflicts preserve newer user policy.
/// state 持有串行写入器；original 及 target 为固定成员关系；冲突保留较新用户策略。
async fn restore_policy(
    state: &SharedServiceBridgeState,
    plugin_id: &str,
    original: (bool, bool),
    target: Option<(bool, bool)>,
) -> Result<(), String> {
    let Some(target) = target else {
        return Ok(());
    };
    // Move the exact original identity into the existing configuration transaction.
    // 将精确原身份移入既有配置事务。
    let plugin_id = plugin_id.to_owned();
    mutate_provider_configuration(
        state,
        "恢复原安装策略失败",
        move |configuration| {
            // Uncertain atomic publication permits precisely its original and intended images.
            // 不确定原子发布仅允许精确原像及期望像。
            let current = policy_membership(&configuration.luaskills, &plugin_id);
            if current != original && current != target {
                return Err(String::from(
                    "安装后插件策略已改变；保留原请求，不覆盖新设置。",
                ));
            }
            if original.0 {
                configuration
                    .luaskills
                    .enabled_vulcan_plugins
                    .insert(plugin_id.clone());
            } else {
                configuration
                    .luaskills
                    .enabled_vulcan_plugins
                    .remove(&plugin_id);
            }
            if original.1 {
                configuration
                    .luaskills
                    .disabled_vulcan_plugins
                    .insert(plugin_id.clone());
            } else {
                configuration
                    .luaskills
                    .disabled_vulcan_plugins
                    .remove(&plugin_id);
            }
            Ok(())
        },
        |_, _| (),
    )
    .await
}

/// Require actual unpublished routing before any original package or credential compensation.
/// 任何原插件包或凭据补偿之前，要求实际路由未发布。
/// state owns the full slot; plugin_id selects the original; returns unit or a conflict preserving all resources.
/// state 持有完整槽；plugin_id 选择原实例；返回空值或冲突并保留全部资源。
fn require_unpublished(state: &SharedServiceBridgeState, plugin_id: &str) -> Result<(), String> {
    // The actual service lock excludes publication and shutdown races.
    // 实际服务锁排除发布及关闭竞争。
    let guard = state.lock().map_err(|error| error.to_string())?;
    // The actual full slot is authoritative for published visibility.
    // 实际完整槽为已发布可见性的权威。
    let full = &guard
        .agent_runtime
        .as_ref()
        .ok_or("Agent 扩展代次尚未初始化。")?
        .extension_generation;
    if full
        .plugin_catalog()
        .is_some_and(|catalog| catalog.plugins.contains_key(plugin_id))
    {
        return Err(String::from(
            "Original installation is already visible in the actual full slot; package rollback requires publication reconciliation.",
        ));
    }
    Ok(())
}

/// Abort the retained original without copying, native replay, or re-reading replacement credential preimages.
/// 中止保留原实例，不复制、不重放原生工作、亦不重读替代凭据前像。
/// acknowledgement applies only to retired failed external compensation; physical cleanup is always mandatory.
/// acknowledgement 仅适用于已退役失败外部补偿；物理清理始终必需。
async fn abort_original(
    state: &SharedServiceBridgeState,
    plugin_id: &str,
    resources: &mut LocalPluginInstallResources,
    acknowledge_external_effects: bool,
) -> Result<(), String> {
    {
        // Installation recovery cannot borrow an update UUID to replace its retained Full or core catalog.
        // 安装恢复不能借用更新 UUID 替换其保留 Full 或核心目录。
        let guard = state.lock().map_err(|error| error.to_string())?;
        crate::service_state_access::require_plugin_update_full_publication_admission(
            &guard, None,
        )?;
    }
    if resources.phase == LocalPluginInstallPhase::Committed {
        return Err(String::from(
            "Committed installation cannot be rolled back.",
        ));
    }
    if resources.phase == LocalPluginInstallPhase::Aborted {
        return Ok(());
    }
    require_unpublished(state, plugin_id)?;
    resources.phase = LocalPluginInstallPhase::Aborting;
    if let Some(native) = resources.native.as_ref() {
        native.abort_and_drain().await?;
    }
    retire_call(&mut resources.install, &mut resources.install_result).await?;
    if resources.install_started && resources.uninstall_result.is_none() {
        if resources.uninstall.is_none() {
            // Reproject the exact original package while borrowing its original namespace owner.
            // 借用原命名空间所有者期间，重新投影精确原插件包。
            let configuration = provider_configuration_from_state(state)?;
            // Only this original candidate is forced into disabled validation.
            // 仅此原候选被置入停用校验。
            let mut disabled = configuration.luaskills.disabled_vulcan_plugins.clone();
            disabled.insert(plugin_id.to_owned());
            // The same retained guard supplies the actual package catalog.
            // 同一保留屏障提供实际插件包目录。
            {
                // Recheck before the compensation refresh replaces core identity, independently of the later Full publisher.
                // 补偿刷新替换核心身份前重新检查，独立于后续 Full 发布器。
                let guard = state.lock().map_err(|error| error.to_string())?;
                crate::service_state_access::require_plugin_update_full_publication_admission(
                    &guard, None,
                )?;
            }
            let catalog = resources
                .runtime
                .refresh_catalog_with_legacy_mutation(
                    &disabled,
                    &configuration.luaskills.enabled_vulcan_plugins,
                    plugin_id,
                    resources.package.mutation_guard(),
                )
                .map_err(|error| error.to_string())?;
            validate_candidate(&resources.runtime, &catalog, &resources.package)?;
            resources.uninstall = Some(
                resources
                    .runtime
                    .prepare_legacy_lifecycle_call(
                        plugin_id,
                        VulcanPluginLegacyLifecycleEvent::Uninstall,
                        Arc::clone(resources.package.mutation_guard()),
                    )
                    .map_err(|error| error.to_string())?,
            );
        }
        // Start is idempotent for this actual registered call; no new callback is registered on retry.
        // 对此实际已登记调用，启动具有幂等性；重试不登记新回调。
        let uninstall = resources
            .uninstall
            .as_ref()
            .ok_or("Original uninstall call is missing.")?;
        uninstall.start().map_err(|error| error.to_string())?;
        // Observed original callback result, whose retired outcome is recorded separately.
        // 观测到的原回调结果，其已退役结果独立记录。
        let _business = uninstall.observe().await;
        retire_call(&mut resources.uninstall, &mut resources.uninstall_result).await?;
    }
    if resources
        .uninstall_result
        .as_ref()
        .is_some_and(Result::is_err)
        && !acknowledge_external_effects
    {
        return Err(String::from(
            "Original uninstall compensation failed after physical retirement; explicitly acknowledge its external effects before package removal.",
        ));
    }
    restore_policy(
        state,
        plugin_id,
        resources.original_policy,
        resources.target_policy,
    )
    .await?;
    if let Some(preimages) = resources.preimages.as_ref() {
        // Restore every declared account, including the uncertain failing write and not-yet-written accounts.
        // 恢复每个已声明账户，包括不确定失败写入及尚未写入账户。
        let store = crate::plugin_mcp_secret_store::OsPluginMcpSecretStore;
        for ((server_id, secret_key), previous) in preimages.iter().rev() {
            restore_guarded_plugin_mcp_secret(
                &store,
                resources.package.mutation_guard(),
                plugin_id,
                server_id,
                secret_key,
                previous.as_deref(),
            )?;
            // Actual original storage evidence is required before confirmation.
            // 确认之前需要实际原存储证据。
            let actual = store.read_optional_secret(plugin_id, server_id, secret_key)?;
            if &actual != previous {
                return Err(String::from(
                    "Original installation credential restoration readback did not match.",
                ));
            }
        }
    }
    require_unpublished(state, plugin_id)?;
    // The original package guard remains in the service slot through actual absence confirmation.
    // 原插件包屏障跨实际缺失确认保持位于服务槽。
    let runtime = resources.runtime.clone();
    runtime
        .abort_local_user_plugin_installation(&mut resources.package)
        .map_err(|error| error.to_string())?;
    // Actual configuration supplies persisted policy without inferred defaults.
    // 实际配置提供持久策略，不推断默认值。
    let configuration = provider_configuration_from_state(state)?;
    {
        // Final installation compensation also changes the manager Arc and must preserve an update's exact publication.
        // 最终安装补偿也改变管理器 Arc，必须保留更新精确发布。
        let guard = state.lock().map_err(|error| error.to_string())?;
        crate::service_state_access::require_plugin_update_full_publication_admission(
            &guard, None,
        )?;
    }
    runtime
        .refresh_catalog_with_legacy_mutation(
            &configuration.luaskills.disabled_vulcan_plugins,
            &configuration.luaskills.enabled_vulcan_plugins,
            plugin_id,
            resources.package.mutation_guard(),
        )
        .map_err(|error| error.to_string())?;
    resources.phase = LocalPluginInstallPhase::Aborted;
    Ok(())
}

/// Validate this actual disabled candidate against the immutable original package preview.
/// 根据不可变原插件包预览校验此实际停用候选。
/// runtime supplies live safety counts; catalog and package are actual originals; returns their exact descriptor or a conflict before effects.
/// runtime 提供实时安全计数；catalog 及 package 为实际原实例；返回精确描述符，或副作用前的冲突。
fn validate_candidate<'a>(
    runtime: &VulcanPluginRuntimeManager,
    catalog: &'a vulcan_plugin_runtime::VulcanPluginCatalogSnapshot,
    package: &VulcanPluginInstallationTransaction,
) -> Result<&'a vulcan_plugin_runtime::VulcanPluginDescriptor, String> {
    // The immutable original identity selects the exact descriptor.
    // 不可变原身份选择精确描述符。
    let plugin = catalog
        .plugins
        .get(package.plugin_id())
        .ok_or("Original installed package is not in the guarded catalog.")?;
    if plugin.content_fingerprint != package.permission_preview().content_fingerprint
        || plugin.managed_package().is_some()
        || plugin.origin != vulcan_plugin_runtime::VulcanPluginPackageOrigin::UserInstalled
        || plugin.desired_enabled
        || plugin.runtime_state != vulcan_plugin_runtime::VulcanPluginRuntimeState::Disabled
        || runtime
            .active_session_count(&plugin.id)
            .map_err(|error| error.to_string())?
            != 0
        || runtime
            .has_cleanup_diagnostics(&plugin.id)
            .map_err(|error| error.to_string())?
    {
        return Err(String::from(
            "Original installation does not match its exact disabled candidate.",
        ));
    }
    Ok(plugin)
}

/// Execute the sole original package, credential, validation and lifecycle sequence before full commitment.
/// 在完整提交之前，执行唯一原插件包、凭据、校验及生命周期序列。
/// app/state own projection and publication; owner already resides in the strong service registry.
/// app 及 state 持有投影及发布；owner 已位于强服务注册表。
async fn execute(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    owner: &Arc<PendingLocalPluginInstall>,
) -> Result<LuaSkillsCatalogUpdate, String> {
    {
        // Internal installation execution shares the public management entry's ordinary Full exclusion before effects.
        // 内部安装执行在副作用前共享公开管理入口的普通 Full 排他。
        let guard = state.lock().map_err(|error| error.to_string())?;
        crate::service_state_access::require_plugin_update_full_publication_admission(
            &guard, None,
        )?;
    }
    // All actual account preimages enter the slot before the first package or credential mutation.
    // 全部实际账户前像在首次插件包或凭据变更之前进入槽。
    {
        // The same original resources are exclusively borrowed without transferring ownership.
        // 排他借用同一原资源，不转移所有权。
        let mut resources = owner.resources.lock().await;
        // The actual configured storage boundary performs original operations.
        // 实际已配置存储边界执行原操作。
        let store = crate::plugin_mcp_secret_store::OsPluginMcpSecretStore;
        // Every actual Some or None account is captured before credential writes.
        // 凭据写入之前捕获每个实际存在或缺失账户。
        let mut preimages = BTreeMap::new();
        for (server_id, secret_key) in resources.supplied.keys() {
            resources
                .package
                .mutation_guard()
                .validate_plugin(&owner.plugin_id)
                .map_err(|error| error.to_string())?;
            preimages.insert(
                (server_id.clone(), secret_key.clone()),
                store.read_optional_secret(&owner.plugin_id, server_id, secret_key)?,
            );
        }
        resources.preimages = Some(preimages);
    }
    // The blocking worker borrows the same service slot, so cancellation cannot strand its package authority.
    // 阻塞工作者借用同一服务槽，因此取消不能遗失其插件包权威。
    let copying_owner = Arc::clone(owner);
    run_extension_management_blocking("publish original local installation", move || {
        // The same original resources are exclusively borrowed without transferring ownership.
        // 排他借用同一原资源，不转移所有权。
        let mut resources = copying_owner.resources.blocking_lock();
        resources.phase = LocalPluginInstallPhase::PackagePublication;
        // The original manager remains retained independently from later discovery.
        // 原管理器独立于随后发现而保持保留。
        let runtime = resources.runtime.clone();
        runtime
            .publish_local_user_plugin_installation(&mut resources.package)
            .map(|_| ())
            .map_err(|error| error.to_string())
    })
    .await?;
    // The same original resources are exclusively borrowed without transferring ownership.
    // 排他借用同一原资源，不转移所有权。
    let mut resources = owner.resources.lock().await;
    resources.phase = LocalPluginInstallPhase::CredentialMutation;
    // The actual configured storage boundary performs original operations.
    // 实际已配置存储边界执行原操作。
    let store = crate::plugin_mcp_secret_store::OsPluginMcpSecretStore;
    for ((server_id, secret_key), value) in &resources.supplied {
        resources
            .package
            .mutation_guard()
            .validate_plugin(&owner.plugin_id)
            .map_err(|error| error.to_string())?;
        store.write_secret(&owner.plugin_id, server_id, secret_key, value)?;
        if store
            .read_optional_secret(&owner.plugin_id, server_id, secret_key)?
            .as_deref()
            != Some(value.as_str())
        {
            return Err(String::from(
                "Original installation credential write readback did not match.",
            ));
        }
    }
    // Actual configuration supplies persisted policy without inferred defaults.
    // 实际配置提供持久策略，不推断默认值。
    let configuration = provider_configuration_from_state(state)?;
    if policy_membership(&configuration.luaskills, &owner.plugin_id) != resources.original_policy {
        return Err(String::from(
            "插件原安装策略已改变；保留原请求，不覆盖新设置。",
        ));
    }
    // Only this original candidate is forced into disabled validation.
    // 仅此原候选被置入停用校验。
    let mut disabled = configuration.luaskills.disabled_vulcan_plugins.clone();
    disabled.insert(owner.plugin_id.clone());
    // The same retained guard supplies the actual package catalog.
    // 同一保留屏障提供实际插件包目录。
    {
        // Disabled candidate refresh changes core catalog allocation before any Full commit, so guard that earlier boundary.
        // 停用候选刷新在任何 Full 提交前改变核心目录分配，因此保护此更早边界。
        let guard = state.lock().map_err(|error| error.to_string())?;
        crate::service_state_access::require_plugin_update_full_publication_admission(
            &guard, None,
        )?;
    }
    let catalog = resources
        .runtime
        .refresh_catalog_with_legacy_mutation(
            &disabled,
            &configuration.luaskills.enabled_vulcan_plugins,
            &owner.plugin_id,
            resources.package.mutation_guard(),
        )
        .map_err(|error| error.to_string())?;
    // The exact original disabled candidate is validated before further effects.
    // 后续副作用之前校验精确原停用候选。
    let candidate = validate_candidate(&resources.runtime, &catalog, &resources.package)?;
    // The actual candidate manifest fixes policy normalization.
    // 实际候选清单固定策略规范化。
    let default_enabled = candidate
        .manifest
        .as_ref()
        .ok_or("原安装候选缺少清单。")?
        .default_enabled;
    // The declared policy default normalizes only this plugin.
    // 声明策略默认值仅规范化此插件。
    let mut target = configuration.luaskills.clone();
    target.set_vulcan_plugin_enabled_with_default(&owner.plugin_id, false, default_enabled);
    resources.target_policy = Some(policy_membership(&target, &owner.plugin_id));
    // Original authority or outcome is retained rather than reconstructed.
    // 原权威或结果被保留，而非重建。
    let original = resources.original_policy;
    // The same exact namespace is retained across the transaction.
    // 事务全程保留同一精确命名空间。
    let plugin_id = owner.plugin_id.clone();
    mutate_provider_configuration(
        state,
        "保存原安装停用策略失败",
        move |configuration| {
            if policy_membership(&configuration.luaskills, &plugin_id) != original {
                return Err(String::from(
                    "插件原安装策略已改变；保留原请求，不覆盖新设置。",
                ));
            }
            configuration
                .luaskills
                .set_vulcan_plugin_enabled_with_default(&plugin_id, false, default_enabled);
            Ok(())
        },
        |_, _| (),
    )
    .await?;
    resources.phase = LocalPluginInstallPhase::NativeValidation;
    drop(resources);
    // The same exclusive slot also covers the actual environment worker through cancellation and shutdown.
    // 同一排他槽亦跨取消及关闭覆盖实际环境工作者。
    let environment_owner = Arc::clone(owner);
    run_extension_management_blocking("prepare original installed environment", move || {
        // The same original resources are exclusively borrowed without transferring ownership.
        // 排他借用同一原资源，不转移所有权。
        let resources = environment_owner.resources.blocking_lock();
        resources
            .runtime
            .prepare_plugin_environment(&environment_owner.plugin_id)
            .map(|_| ())
            .map_err(|error| error.to_string())
    })
    .await?;
    // The same original resources are exclusively borrowed without transferring ownership.
    // 排他借用同一原资源，不转移所有权。
    let mut resources = owner.resources.lock().await;
    // Only actual declared stdio servers require initial native validation.
    // 仅实际已声明标准输入输出服务器需要初始原生校验。
    let local = candidate
        .mcp_servers
        .iter()
        .filter(|server| {
            matches!(
                server.manifest.transport,
                vulcan_plugin_runtime::VulcanPluginMcpTransportManifest::Stdio { .. }
            )
        })
        .cloned()
        .collect::<Vec<_>>();
    // Production materialization supplies exact native inputs.
    // 生产物化提供精确原生输入。
    let servers = plugin_mcp_server_configs_for_descriptors(&resources.runtime, &local, &store)?;
    if !servers.is_empty() {
        // Actual current authority is observed without replacing its original owner.
        // 观测实际当前权威，不替换其原所有者。
        let current = state
            .lock()
            .map_err(|error| error.to_string())?
            .agent_runtime
            .as_ref()
            .ok_or("Agent 扩展代次尚未初始化。")?
            .extension_generation
            .mcp_runtime();
        resources.native = Some(
            vulcan_agent_runtime::mcp::LegacyPluginMcpReplacement::prepare(
                &current,
                &owner.plugin_id,
                candidate.generation,
                servers,
            )?,
        );
        // The actual original controller was retained before dispatch.
        // 实际原控制器在派发之前已被保留。
        let native = resources
            .native
            .as_ref()
            .ok_or("Original installation native owner is missing.")?;
        native.dispatch()?;
        native.wait_until_ready().await?;
        #[cfg(test)]
        super::legacy_native_observation::observe(
            &owner.plugin_id,
            super::legacy_native_observation::Milestone::Ready,
        );
        native.abort_and_drain().await?;
    }
    resources.phase = LocalPluginInstallPhase::Lifecycle;
    resources.install = Some(
        resources
            .runtime
            .prepare_legacy_lifecycle_call(
                &owner.plugin_id,
                VulcanPluginLegacyLifecycleEvent::Install,
                Arc::clone(resources.package.mutation_guard()),
            )
            .map_err(|error| error.to_string())?,
    );
    resources.install_started = true;
    // Only the retained original installation call is observed.
    // 仅观测保留原安装调用。
    let install = resources
        .install
        .as_ref()
        .ok_or("Original installation call is missing.")?;
    install.start().map_err(|error| error.to_string())?;
    // Actual callback completion is distinct from physical retirement.
    // 实际回调完成区别于物理退役。
    let business = install.observe().await.map_err(|error| error.to_string());
    let LocalPluginInstallResources {
        install,
        install_result,
        ..
    } = &mut *resources;
    retire_call(install, install_result).await?;
    business?;
    // Callback configuration writes can issue a newer manager revision; publish only its actual guarded successor.
    // 回调配置写入可能签发较新管理器修订；仅发布其实际已屏障后继。
    let effective = provider_configuration_from_state(state)?;
    if Some(policy_membership(&effective.luaskills, &owner.plugin_id)) != resources.target_policy {
        return Err(String::from(
            "原安装目标策略在发布前已改变；保留原请求，不覆盖新设置。",
        ));
    }
    // The final scan uses the same original namespace rather than the pre-callback catalog.
    // 最终扫描使用同一原命名空间，而非回调之前的目录。
    {
        // Keep a retained update's actual core catalog intact through this final installation refresh.
        // 在此最终安装刷新期间保持保留更新的实际核心目录完整。
        let guard = state.lock().map_err(|error| error.to_string())?;
        crate::service_state_access::require_plugin_update_full_publication_admission(
            &guard, None,
        )?;
    }
    let publication_catalog = resources
        .runtime
        .refresh_catalog_with_legacy_mutation(
            &effective.luaskills.disabled_vulcan_plugins,
            &effective.luaskills.enabled_vulcan_plugins,
            &owner.plugin_id,
            resources.package.mutation_guard(),
        )
        .map_err(|error| error.to_string())?;
    resources.phase = LocalPluginInstallPhase::Publishing;
    publish_original(state, &publication_catalog, &mut resources)?;
    #[cfg(test)]
    super::legacy_native_observation::observe(
        &owner.plugin_id,
        super::legacy_native_observation::Milestone::Published,
    );
    refresh_plugin_catalog_local_state_and_emit(app, state).await
}

/// Publish only the original disabled candidate, preserving unrelated actual current routes and MCP owners.
/// 仅发布原停用候选，保留实际当前无关路由及 MCP 所有者。
/// The service phase changes beside its actual full pointer before any event or projection can fail.
/// 服务阶段在实际完整指针旁改变，先于任何可能失败的事件或投影。
fn publish_original(
    state: &SharedServiceBridgeState,
    catalog: &Arc<vulcan_plugin_runtime::VulcanPluginCatalogSnapshot>,
    resources: &mut LocalPluginInstallResources,
) -> Result<(), String> {
    // The exact original disabled candidate is validated before further effects.
    // 后续副作用之前校验精确原停用候选。
    let candidate = validate_candidate(&resources.runtime, catalog, &resources.package)?.clone();
    resources
        .package
        .mutation_guard()
        .validate_plugin(&candidate.id)
        .map_err(|error| error.to_string())?;
    // The actual service lock excludes publication and shutdown races.
    // 实际服务锁排除发布及关闭竞争。
    let mut guard = state.lock().map_err(|error| error.to_string())?;
    crate::service_state_access::require_plugin_publication_shutdown_admission(&guard)?;
    crate::service_state_access::require_plugin_update_full_publication_admission(&guard, None)?;
    // The actual service bridge owns the sole complete extension pointer.
    // 实际服务桥持有唯一完整扩展指针。
    let bridge = guard
        .agent_runtime
        .as_mut()
        .ok_or("Agent 扩展代次尚未初始化。")?;
    // The actual current pointer is captured under its publication lock.
    // 实际当前指针在发布锁下捕获。
    let expected = Arc::clone(&bridge.extension_generation);
    // Routing comes from the actual current full pointer.
    // 路由来自实际当前完整指针。
    let current_catalog = expected
        .plugin_catalog()
        .ok_or("Actual full generation has no plugin catalog.")?;
    if current_catalog.service_epoch != catalog.service_epoch
        || current_catalog.plugins.contains_key(&candidate.id)
    {
        return Err(String::from(
            "Original installation full publication conflicts with the actual current catalog.",
        ));
    }
    // Unrelated current routes are preserved while adding the disabled original.
    // 增加停用原实例时保留当前无关路由。
    let mut merged = (*current_catalog).clone();
    merged.revision = catalog.revision;
    merged.user_root = catalog.user_root.clone();
    merged.plugins.insert(candidate.id.clone(), candidate);
    // Actual current authority is observed without replacing its original owner.
    // 观测实际当前权威，不替换其原所有者。
    let current = expected.mcp_runtime();
    // All fallible preparations precede the adjacent full commit.
    // 全部可能失败准备先于相邻完整提交。
    let next = bridge.prepare_plugin_extension_generation(Arc::new(merged), current)?;
    vulcan_agent_runtime::agent_runtime::extension_generation::ExtensionGeneration::publish_prepared(&mut bridge.extension_generation, &expected, next, |_| { resources.phase = LocalPluginInstallPhase::Committed; })?;
    Ok(())
}

/// Install with strong service retention before publication; failures use only the same original for compensation.
/// 在发布之前通过强服务保留安装；失败仅使用同一原实例补偿。
/// Inputs bind exact inspection and declared secrets; returns the committed projection or a recoverable original UUID.
/// 输入绑定精确检查及已声明秘密；返回已提交投影，或可恢复原 UUID。
pub(super) async fn install(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    source_path: String,
    fingerprint: String,
    permissions: Vec<String>,
    allow_registry_dependencies: bool,
    initial_secrets: Vec<VulcanPluginInitialSecretInput>,
) -> Result<LuaSkillsCatalogUpdate, String> {
    {
        // Reject before reservation, package, credential or callback work can invalidate another retained Full.
        // 在预留、插件包、凭据或回调工作能够使另一保留 Full 失效之前拒绝。
        let guard = state.lock().map_err(|error| error.to_string())?;
        crate::service_state_access::require_plugin_update_full_publication_admission(
            &guard, None,
        )?;
    }
    // The original manager remains retained independently from later discovery.
    // 原管理器独立于随后发现而保持保留。
    let runtime = service_vulcan_plugin_runtime(state)?;
    // Actual configuration supplies persisted policy without inferred defaults.
    // 实际配置提供持久策略，不推断默认值。
    let configuration = provider_configuration_from_state(state)?;
    // Registration and shutdown exclusion use the same actual service state.
    // 登记及关闭排他使用同一实际服务状态。
    let retaining_state = Arc::clone(state);
    // Only the explicitly authorized original supplies package input.
    // 仅明确已授权原实例提供插件包输入。
    let source = PathBuf::from(source_path);
    // The same retained strong original is used throughout recovery.
    // 恢复全程使用同一保留强原实例。
    let owner =
        run_extension_management_blocking("reserve original local installation", move || {
            // Actual inspection fixes complete authorization and content identity.
            // 实际检查固定完整授权及内容身份。
            let preview = runtime
                .inspect_local_user_plugin(&source)
                .map_err(|error| error.to_string())?;
            if preview.content_fingerprint != fingerprint {
                return Err(String::from("本地插件包在原安装秘密校验之前已发生变化。"));
            }
            // Exact declaration validation precedes all credential effects.
            // 精确声明校验先于全部凭据副作用。
            let supplied = validate_initial_plugin_mcp_secrets(&preview, initial_secrets)?;
            // The inert actual package reservation carries its original physical file.
            // 未启动实际插件包预留携带其原物理文件。
            let package = runtime
                .reserve_local_user_plugin_installation(
                    &source,
                    &fingerprint,
                    permissions,
                    allow_registry_dependencies,
                )
                .map_err(|error| error.to_string())?;
            // Exact original membership precedes package effects.
            // 精确原成员关系先于插件包副作用。
            let original_policy = policy_membership(&configuration.luaskills, package.plugin_id());
            // The actual service lock excludes publication and shutdown races.
            // 实际服务锁排除发布及关闭竞争。
            let guard = retaining_state.lock().map_err(|error| error.to_string())?;
            crate::service_state_access::require_plugin_publication_shutdown_admission(&guard)?;
            require_installation_absence(&guard, package.plugin_id())?;
            guard
                .pending_local_plugin_installs
                .retain(runtime, package, supplied, original_policy)
        })
        .await?;
    // The actual operation result selects compensation or committed reprojection.
    // 实际操作结果选择补偿或已提交重新投影。
    let result = execute(app, state, &owner).await;
    // The actual strong service registry supplies original ownership.
    // 实际强服务注册表提供原所有权。
    let registry = Arc::clone(
        &state
            .lock()
            .map_err(|error| error.to_string())?
            .pending_local_plugin_installs,
    );
    // The same original resources are exclusively borrowed without transferring ownership.
    // 排他借用同一原资源，不转移所有权。
    let mut resources = owner.resources.lock().await;
    match result {
        Ok(update) => {
            registry.retire(&owner, &resources)?;
            Ok(update)
        }
        Err(error) if resources.phase == LocalPluginInstallPhase::Committed => Err(format!(
            "{error}；原安装请求 `{}` 已提交，保留原请求以重试投影。",
            owner.request_id
        )),
        Err(error) => match abort_original(state, &owner.plugin_id, &mut resources, false).await {
            Ok(()) => {
                registry.retire(&owner, &resources)?;
                Err(error)
            }
            Err(cleanup) => Err(format!(
                "{error}；原安装请求 `{}` 恢复未完成：{cleanup}",
                owner.request_id
            )),
        },
    }
}

/// Reject an already published identity under the same shutdown admission lock as owner insertion.
/// 在与所有者插入相同的关闭准入锁下，拒绝已发布身份。
/// guard is actual service ownership; plugin_id is the original inspected identity; returns unit or a pre-effect conflict.
/// guard 为实际服务所有权；plugin_id 为原检查身份；返回空值或副作用前冲突。
fn require_installation_absence(guard: &ServiceBridgeState, plugin_id: &str) -> Result<(), String> {
    // The actual full slot is authoritative for published visibility.
    // 实际完整槽为已发布可见性的权威。
    let full = &guard
        .agent_runtime
        .as_ref()
        .ok_or("Agent 扩展代次尚未初始化。")?
        .extension_generation;
    if full
        .plugin_catalog()
        .is_some_and(|catalog| catalog.plugins.contains_key(plugin_id))
    {
        return Err(String::from(
            "Plugin identity is already published in the actual service generation.",
        ));
    }
    Ok(())
}

/// Recover only the actual retained request, without restarting installation or overwriting committed credentials.
/// 仅恢复实际保留请求，不重新启动安装，亦不覆盖已提交凭据。
/// app/state supply projection and original lookup; acknowledgement cannot bypass actual native or callback retirement.
/// app 及 state 提供投影及原查询；确认不能绕过实际原生或回调退役。
pub(super) async fn recover(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    plugin_id: &str,
    request_id: uuid::Uuid,
    acknowledge_external_effects: bool,
) -> Result<(), String> {
    {
        // This UUID belongs to the independent installation registry and grants no exemption from update Full exclusion.
        // 此 UUID 属于独立安装注册表，不授予更新 Full 排他的豁免。
        let guard = state.lock().map_err(|error| error.to_string())?;
        crate::service_state_access::require_plugin_update_full_publication_admission(
            &guard, None,
        )?;
    }
    // The actual strong service registry supplies original ownership.
    // 实际强服务注册表提供原所有权。
    let registry = Arc::clone(
        &state
            .lock()
            .map_err(|error| error.to_string())?
            .pending_local_plugin_installs,
    );
    // The same retained strong original is used throughout recovery.
    // 恢复全程使用同一保留强原实例。
    let owner = registry.get(plugin_id, request_id)?;
    // The same original resources are exclusively borrowed without transferring ownership.
    // 排他借用同一原资源，不转移所有权。
    let mut resources = owner.resources.lock().await;
    if resources.phase == LocalPluginInstallPhase::Committed {
        refresh_plugin_catalog_local_state_and_emit(app, state).await?;
    } else {
        abort_original(
            state,
            plugin_id,
            &mut resources,
            acknowledge_external_effects,
        )
        .await?;
    }
    registry.retire(&owner, &resources)
}
