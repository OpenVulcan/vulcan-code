//! Actual selection hiding and Full publication retain adjacent original completion evidence.
//! 实际选择隐藏及完整发布保留相邻原完成证据。

use super::*;
use owner::{OriginalRemoval, PendingLocalPluginUninstall, UninstallResources};

/// Remove only this plugin's exact captured policy membership, retaining explicit persistence intent across errors.
/// 仅移除此插件精确捕获策略成员关系，跨错误保留明确持久化意图。
/// state/owner/resources identify the actual original; returns only after the adjacent configuration commit.
/// state／owner／resources 标识实际原实例；仅在相邻配置提交之后返回。
pub(super) async fn remove_policy(
    state: &SharedServiceBridgeState,
    owner: &Arc<PendingLocalPluginUninstall>,
    resources: &mut UninstallResources,
) -> Result<(), String> {
    if resources.policy_committed {
        return Ok(());
    }
    // Capture the previously admitted intent before this invocation records its own uncertain write.
    // 在本次调用记录自身不确定写入之前，捕获此前已准入意图。
    let prior_intent = resources.policy_started;
    // Original membership is immutable and independent of the provider's other concurrently edited fields.
    // 原成员关系不可变，独立于供应商其他并行编辑字段。
    let original_membership = resources.original_policy;
    // Intent remains false while awaiting Provider serialization or failing service/member admission.
    // 等待供应商串行化或服务／成员准入失败期间，意图保持为假。
    let policy_started = &mut resources.policy_started;
    // Disjoint mutable confirmation remains available to the adjacent successful commit closure.
    // 互不重叠可变确认保持可用于相邻成功提交闭包。
    let policy_committed = &mut resources.policy_committed;
    crate::service_state_access::mutate_provider_configuration_with_state_validation(
        state,
        "清理原卸载插件状态覆盖失败",
        |service| {
            crate::service_state_access::require_plugin_publication_shutdown_admission(service)?;
            // Prove actual membership before UUID coordination exemption.
            // UUID 协调豁免之前证明实际成员关系。
            let current = service
                .pending_local_plugin_uninstalls
                .lookup(&owner.plugin_id, owner.request_id)?;
            if !Arc::ptr_eq(&current, owner) {
                return Err(String::from("原卸载实际资源槽已经改变。"));
            }
            crate::service_state_access::require_plugin_update_full_publication_admission(
                service,
                Some(owner.request_id),
            )
        },
        |configuration| {
            // Only a previously admitted persistence intent permits reconciling the explicit intended absence.
            // 仅此前已准入持久化意图允许对账明确期望缺失。
            let current = (
                configuration
                    .luaskills
                    .disabled_vulcan_plugins
                    .contains(&owner.plugin_id),
                configuration
                    .luaskills
                    .enabled_vulcan_plugins
                    .contains(&owner.plugin_id),
            );
            if current != original_membership && !(prior_intent && current == (false, false)) {
                return Err(String::from(
                    "原卸载插件策略成员关系已被替代，保留原所有者。",
                ));
            }
            configuration
                .luaskills
                .disabled_vulcan_plugins
                .remove(&owner.plugin_id);
            configuration
                .luaskills
                .enabled_vulcan_plugins
                .remove(&owner.plugin_id);
            // Only this admitted exact mutation can precede an uncertain actual persistence call.
            // 仅此已准入精确变更能够先于不确定实际持久化调用。
            *policy_started = true;
            Ok(())
        },
        |_, _| {
            *policy_committed = true;
        },
    )
    .await
}

/// Hide through actual original APIs and publish one complete service generation under its sole mutex.
/// 通过实际原接口隐藏，并在唯一互斥锁内发布单个完整服务代次。
/// state/owner/resources are the same retained originals; returns without collecting or releasing originals.
/// state／owner／resources 为相同保留原实例；返回时不回收或释放原实例。
pub(super) fn hide_and_publish(
    state: &SharedServiceBridgeState,
    owner: &Arc<PendingLocalPluginUninstall>,
    resources: &mut UninstallResources,
) -> Result<(), String> {
    if resources.committed_full.is_some() {
        confirm_current(state, resources)?;
        // A retained real receipt retires this owner's obsolete pins before any cleanup retry.
        // 保留真实回执在任何清理重试之前退役此所有者过期固定。
        resources.expected_full.take();
        resources.mcp.take();
        // A post-publication Hook projection failure retries only this exact committed catalog.
        // 发布后 Hook 投影失败仅重试此精确已提交目录。
        let catalog = resources
            .committed_full
            .as_ref()
            .ok_or("原卸载完整回执缺失。")?
            .plugin_catalog()
            .ok_or("原卸载完整目录缺失。")?;
        synchronize_service_vulcan_plugin_hooks(state, resources.runtime.clone(), &catalog)?;
        return Ok(());
    }
    if resources.callback.is_some()
        || resources.callback_result.is_none()
        || !resources.policy_committed
    {
        return Err(String::from("原卸载回调退役及策略提交尚未确认。"));
    }
    // Read latest policy only after this original's committed removal and retain no old descriptor pins.
    // 仅在此原实例已提交移除之后读取最新策略，不保留旧描述符固定。
    let configuration = provider_configuration_from_state(state)?;
    // Existing typed protocols own write intents; no service phase guesses initial versus recovery.
    // 既有类型明确协议拥有写入意图；服务阶段不猜测首次或恢复。
    let catalog = match resources
        .original
        .as_mut()
        .ok_or("原卸载物理所有者缺失。")?
    {
        OriginalRemoval::Managed {
            store,
            selection,
            state: local,
        } => {
            match resources
                .runtime
                .observe_deactivation_state(local, selection)
                .map_err(|error| error.to_string())?
            {
                vulcan_plugin_runtime::VulcanPluginDeactivationStateObservation::Reserved => {
                    resources
                        .runtime
                        .freeze_deactivation_state(local, selection)
                        .map_err(|error| error.to_string())?;
                }
                vulcan_plugin_runtime::VulcanPluginDeactivationStateObservation::Frozen => {}
                _ => {
                    return Err(String::from(
                        "完整发布之前原本地卸载状态已进入回收，保留原实例。",
                    ));
                }
            }
            store
                .continue_deactivation_retained(selection)
                .map_err(|error| error.to_string())?;
            resources
                .runtime
                .refresh_catalog_with_deactivation(
                    &configuration.luaskills.disabled_vulcan_plugins,
                    &configuration.luaskills.enabled_vulcan_plugins,
                    selection,
                )
                .map_err(|error| error.to_string())?
        }
        OriginalRemoval::Flat(flat) => {
            resources
                .runtime
                .continue_legacy_user_plugin_uninstallation(flat)
                .map_err(|error| error.to_string())?;
            resources
                .runtime
                .refresh_catalog_with_legacy_mutation(
                    &configuration.luaskills.disabled_vulcan_plugins,
                    &configuration.luaskills.enabled_vulcan_plugins,
                    &owner.plugin_id,
                    flat.mutation_guard(),
                )
                .map_err(|error| error.to_string())?
        }
    };
    if catalog.plugins.contains_key(&owner.plugin_id) {
        return Err(String::from("原卸载选择已隐藏，但实际核心目录仍暴露插件。"));
    }
    // No asynchronous wait or plugin business is permitted while holding the publication mutex.
    // 持有发布互斥锁时，不允许异步等待或插件业务。
    let mut service = state.lock().map_err(|error| error.to_string())?;
    crate::service_state_access::require_plugin_publication_shutdown_admission(&service)?;
    // The map is consulted before actual Full replacement, not trusted from a supplied request ID alone.
    // 实际完整替换之前检查映射，不仅信任提供请求标识。
    let current = service
        .pending_local_plugin_uninstalls
        .lookup(&owner.plugin_id, owner.request_id)?;
    if !Arc::ptr_eq(&current, owner) {
        return Err(String::from("原卸载发布所有者已改变。"));
    }
    crate::service_state_access::require_plugin_update_full_publication_admission(
        &service,
        Some(owner.request_id),
    )?;
    // Competing first-install originals retain their real catalog visibility.
    // 竞争首次安装原实例保留真实目录可见性。
    let current_catalog = service
        .agent_runtime
        .as_ref()
        .and_then(|bridge| bridge.extension_generation.plugin_catalog());
    service
        .pending_local_plugin_installs
        .require_catalog_publication(current_catalog.as_deref(), &catalog)?;
    // The actual bridge is the unique production publisher.
    // 实际桥为唯一生产发布器。
    let bridge = service
        .agent_runtime
        .as_mut()
        .ok_or("Agent 扩展代次尚未初始化。")?;
    // Original Full retains predecessor resources until neighboring commit succeeds.
    // 原完整代次保留前驱资源，直至相邻提交成功。
    let expected = resources
        .expected_full
        .as_ref()
        .ok_or("原卸载缺少实际前置完整代次。")?;
    if !Arc::ptr_eq(expected, &bridge.extension_generation) {
        return Err(String::from("原卸载实际前置完整代次被替代，不能重新准备。"));
    }
    // Remove only the original plugin partition; every unrelated actual partition is preserved.
    // 仅移除原插件分区；保留每个无关实际分区。
    let next_mcp = remove_plugin_mcp_runtime_partition(&expected.mcp_runtime(), &owner.plugin_id);
    // Existing preparation validates the complete Hook/MCP/catalog surface before pointer replacement.
    // 既有准备在指针替换之前校验完整 Hook／MCP／目录暴露面。
    let prepared = bridge.prepare_plugin_extension_generation(Arc::clone(&catalog), next_mcp)?;
    ExtensionGeneration::publish_prepared(
        &mut bridge.extension_generation,
        expected,
        prepared,
        |committed| {
            // Infallible adjacent receipt survives errors after pointer replacement without another publication.
            // 不可失败相邻回执跨指针替换之后的错误存续，不再次发布。
            resources.committed_full = Some(Arc::clone(committed));
        },
    )?;
    drop(current_catalog);
    drop(service);
    // Successful complete removal releases this owner's predecessor pins before any physical GC.
    // 成功完整移除在任何物理回收之前释放此所有者前驱固定。
    resources.expected_full.take();
    resources.mcp.take();
    // Legacy Hook dispatcher is projected from this actual new complete catalog, with no old descriptor adoption.
    // 旧 Hook 调度器从此实际新完整目录投影，不接纳旧描述符。
    synchronize_service_vulcan_plugin_hooks(state, resources.runtime.clone(), &catalog)?;
    confirm_current(state, resources)
}

/// Verify the exact committed Full remains current and excludes the original namespace.
/// 校验精确已提交完整代次仍为当前代次，并排除原命名空间。
/// state/resources are actual originals; returns no new publication or cleanup authority.
/// state／resources 为实际原实例；不返回新发布或清理权威。
pub(super) fn confirm_current(
    state: &SharedServiceBridgeState,
    resources: &UninstallResources,
) -> Result<(), String> {
    // The committed pointer is a true receipt from the production publisher.
    // 已提交指针为生产发布器真实回执。
    let committed = resources
        .committed_full
        .as_ref()
        .ok_or("原卸载尚未确认完整发布。")?;
    // Service exclusion makes current pointer comparison coherent with publication.
    // 服务互斥使当前指针比较与发布保持一致。
    let service = state.lock().map_err(|error| error.to_string())?;
    // No fallback bridge or reconstructed catalog is acceptable.
    // 不接受备用桥或重建目录。
    let bridge = service
        .agent_runtime
        .as_ref()
        .ok_or("Agent 扩展代次尚未初始化。")?;
    // Both Core and Full must retain the same actually published catalog allocation.
    // 核心及完整代次必须保留相同实际已发布目录分配。
    let catalog = committed.plugin_catalog().ok_or("原卸载完整目录缺失。")?;
    if !Arc::ptr_eq(committed, &bridge.extension_generation)
        || !Arc::ptr_eq(&catalog, &resources.runtime.snapshot())
    {
        return Err(String::from("原卸载实际完整回执或核心目录已经改变。"));
    }
    Ok(())
}
