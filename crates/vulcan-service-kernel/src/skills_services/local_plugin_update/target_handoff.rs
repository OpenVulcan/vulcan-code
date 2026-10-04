//! Enabled updates transfer the same original Lua, native MCP and Full extension owners.
//! 启用更新转交相同原 Lua、原生 MCP 及 Full 扩展所有者。

use super::*;
use vulcan_agent_runtime::mcp::{
    McpRuntimeAccess, TargetPluginMcpRuntime, target_global_mcp_server_configs,
};
use vulcan_plugin_runtime::{
    VulcanPluginActivationTransactionPhase, VulcanPluginCatalogSnapshot,
    VulcanPluginPredecessorDrain, VulcanPluginTargetMcpInitializationPhase,
};

mod predecessor_restoration;
pub(super) use predecessor_restoration::{abort_enabled_update, resume_predecessor_restoration};

/// A service slot owns every original enabled-update participant before its first cancellable observation.
/// 服务槽在首次可取消观察之前拥有全部原启用更新参与者。
pub(super) struct RetainedEnabledPluginUpdate {
    /// The original native observation lineage retains unpublished, current and retiring instances.
    /// 原原生观察谱系保留未发布、当前及退役中实例。
    pub(super) current_mcp: McpRuntimeAccess,
    /// The actual original drain freezes old callback inputs independently from later discovery.
    /// 实际原排空独立于后续发现固定旧回调输入。
    predecessor_drain: Option<VulcanPluginPredecessorDrain>,
    /// The exact closed staging Arc fixes the target's original route generation.
    /// 精确关闭暂存 Arc 固定目标原路由代次。
    staged: Option<Arc<VulcanPluginCatalogSnapshot>>,
    /// Native control owns the same actual preparation lease until combined completion transfers it.
    /// 原生控制在组合完成转交之前拥有相同实际准备租约。
    native: Option<TargetPluginMcpRuntime>,
    /// The sealed publication keeps the actual Full Arc through uncertain durable completion.
    /// 密封发布跨不确定持久完成保留实际 Full Arc。
    publication: Option<vulcan_plugin_lifecycle::TargetPluginExtensionPublication>,
    /// Confirmation comes only from the synchronous policy commit after original selection is Published.
    /// 确认仅来自原选择 Published 后的同步策略提交。
    target_policy_confirmed: bool,
    /// The actual old restoration survives abort, initialization and uncertain joint transfer in this same strong slot.
    /// 实际旧恢复在此同一强槽内跨中止、初始化及不确定联合转交存续。
    restoration: Option<predecessor_restoration::RetainedPredecessorRestoration>,
    /// Only a successful actual combined publisher can confirm terminal ownership transfer.
    /// 仅成功实际组合发布者能确认终态所有权转交。
    pub(super) completed: bool,
}

impl RetainedEnabledPluginUpdate {
    /// Return whether only the observation lineage was retained, before any real predecessor or target transition.
    /// 返回是否仅保留观察谱系，尚无任何真实前驱或目标转换。
    /// No input; durable receipts must independently certify absence before this permits abandoning the request.
    /// 无输入；允许放弃请求之前，持久回执仍须独立证明缺失。
    pub(super) fn has_started_participant(&self) -> bool {
        self.predecessor_drain.is_some()
            || self.staged.is_some()
            || self.native.is_some()
            || self.publication.is_some()
            || self.restoration.is_some()
            || self.completed
    }

    /// Observe whether this original already owns real predecessor restoration; no input or new authority.
    /// 观察此原请求是否已拥有真实前驱恢复；无输入或新权威。
    pub(super) fn has_restoration(&self) -> bool {
        self.restoration.is_some()
    }

    /// Match phase against the actual terminal transfer, never treating durable Aborted alone as resource release.
    /// 将 phase 与实际终态转交匹配，绝不将仅持久 Aborted 当作资源释放。
    /// Untouched enabled owners require the caller's independent absence proof before retirement.
    /// 未触及启用所有者在退休之前要求调用方提供独立缺失证明。
    pub(super) fn confirms_terminal(&self, phase: VulcanPluginActivationTransactionPhase) -> bool {
        match phase {
            VulcanPluginActivationTransactionPhase::Completed => self.completed,
            VulcanPluginActivationTransactionPhase::Aborted => {
                self.restoration.as_ref().map_or_else(
                    || !self.has_started_participant(),
                    |owner| owner.transferred,
                )
            }
            _ => false,
        }
    }
}

/// Prove no original lifecycle effect began before allowing an enabled preparation to be abandoned without restoration.
/// 允许启用准备不经恢复交接放弃之前，证明未开始任何原生命周期副作用。
/// runtime/store identify actual owner and durable authority; plugin_id/request_id identify the exact original.
/// runtime/store 标识实际所有者及持久权威；plugin_id/request_id 标识精确原请求。
/// Returns an explicit restoration requirement when any actual participant or durable attempt exists.
/// 存在任何实际参与者或持久尝试时，返回明确恢复交接要求。
pub(super) fn require_unstarted_enabled_update(
    runtime: &VulcanPluginRuntimeManager,
    store: &VulcanPluginGenerationStore,
    plugin_id: &str,
    request_id: uuid::Uuid,
) -> Result<(), String> {
    if runtime
        .activation_predecessor_drain(request_id)
        .map_err(|error| error.to_string())?
        .is_some()
        || !runtime
            .activation_predecessor_calls(request_id)
            .map_err(|error| error.to_string())?
            .is_empty()
        || !runtime
            .activation_candidate_calls(request_id)
            .map_err(|error| error.to_string())?
            .is_empty()
        || store
            .activation_effects(plugin_id, request_id)
            .map_err(|error| error.to_string())?
            .is_some()
        || store
            .installation_receipt(plugin_id, request_id)
            .map_err(|error| error.to_string())?
            .is_some()
        || store
            .target_hook_initialization_receipt(plugin_id, request_id)
            .map_err(|error| error.to_string())?
            .is_some()
        || store
            .target_mcp_initialization_receipt(plugin_id, request_id)
            .map_err(|error| error.to_string())?
            .is_some()
    {
        return Err(String::from(
            "启用更新已开始实际生命周期；必须恢复原前驱交接，保留原所有者。",
        ));
    }
    Ok(())
}

/// Check the current explicit target policy against the original frozen enabled request.
/// 按原固定启用请求检查当前显式目标策略。
/// record supplies the original target; configuration supplies the serialized current policy; returns conflicts explicitly.
/// record 提供原目标；configuration 提供串行化当前策略；明确返回冲突。
fn require_enabled_policy(
    record: &vulcan_plugin_runtime::VulcanPluginActivationTransactionRecord,
    configuration: &vulcan_provider_config::config::PersistedLuaSkillsConfiguration,
) -> Result<(), String> {
    // One original desired state authorizes the target despite a changed manifest default.
    // 单个原期望状态在清单默认值变化后仍授权目标。
    let plugin_id = &record.target().plugin_id;
    if !record
        .update_policy()
        .is_some_and(VulcanPluginUpdatePolicy::desired_enabled)
        || !configuration.enabled_vulcan_plugins.contains(plugin_id)
        || configuration.disabled_vulcan_plugins.contains(plugin_id)
    {
        return Err(String::from(
            "原启用更新目标与当前显式用户策略不一致；保留原请求。",
        ));
    }
    Ok(())
}

/// Validate configuration against record's exact original enabled baseline before selection or old restoration.
/// 在选择或旧恢复之前，按 record 精确原启用基线校验 configuration。
/// Return a conflict without changing this plugin's overrides or unrelated provider settings.
/// 返回冲突，不改变此插件覆盖或无关供应商设置。
fn require_original_enabled_policy(
    record: &vulcan_plugin_runtime::VulcanPluginActivationTransactionRecord,
    configuration: &vulcan_provider_config::config::PersistedLuaSkillsConfiguration,
) -> Result<(), String> {
    // Exact original policy is retained by the transaction, independently of candidate manifest defaults.
    // 精确原策略由事务保留，独立于候选清单默认值。
    let policy = record.update_policy().ok_or("原更新请求缺少用户策略。")?;
    // Only the original namespace can be normalized after selection or restored before publication.
    // 仅原命名空间可以在选择之后归一化，或在发布之前恢复。
    let plugin_id = &record.target().plugin_id;
    if !policy.desired_enabled()
        || !policy.matches_overrides(
            configuration.enabled_vulcan_plugins.contains(plugin_id),
            configuration.disabled_vulcan_plugins.contains(plugin_id),
        )
    {
        return Err(String::from(
            "此插件原用户策略已改变；保留原请求，不覆盖新设置。",
        ));
    }
    Ok(())
}

/// Persist the selected enabled policy under transaction and synchronously confirm the original strong owner.
/// 在 transaction 下持久保存所选启用策略，并同步确认原强所有者。
/// state owns the actual bridge; record must already be Published or Completed; confirmed is its original memory checkpoint.
/// state 拥有实际桥；record 必须已经 Published 或 Completed；confirmed 为其原内存检查点。
/// Return installed configuration; failed persistence keeps original in-memory policy and confirmation unchanged.
/// 返回已安装配置；持久化失败保持原内存策略及确认不变。
fn persist_selected_enabled_policy(
    state: &SharedServiceBridgeState,
    transaction: &crate::service_state_access::ProviderConfigurationTransaction,
    record: &vulcan_plugin_runtime::VulcanPluginActivationTransactionRecord,
    confirmed: &mut bool,
) -> Result<vulcan_provider_config::config::ProviderConfigurationFile, String> {
    if !matches!(
        record.phase(),
        VulcanPluginActivationTransactionPhase::Published
            | VulcanPluginActivationTransactionPhase::Completed
    ) {
        return Err(String::from("原选择尚未确认，不能归一化启用策略。"));
    }
    // The existing provider transaction already precedes service exclusion; never reacquire its asynchronous mutex.
    // 既有供应商事务已经先于服务互斥；绝不重新取得其异步互斥锁。
    let mut guard = state.lock().map_err(|error| error.to_string())?;
    crate::service_state_access::require_plugin_publication_shutdown_admission(&guard)?;
    // Retain the actual bridge through persistence and the final memory checkpoint, matching the existing writer contract.
    // 跨持久化及最终内存检查点保留实际桥，与既有写入器契约一致。
    let bridge = guard
        .agent_runtime
        .as_mut()
        .ok_or("Agent 运行时尚未初始化。")?;
    // Clone only the latest actual configuration, preserving changes to every other plugin and provider.
    // 仅克隆最新实际配置，保留其他全部插件及供应商的变更。
    let mut configuration = bridge.provider_configuration.clone();
    if *confirmed {
        require_enabled_policy(record, &configuration.luaskills)?;
        return Ok(configuration);
    }
    require_original_enabled_policy(record, &configuration.luaskills)?;
    // This exact selected target changes only its own membership; no reverse-policy inference is needed.
    // 此精确所选目标仅改变其自身成员关系；无需逆向推断策略。
    let plugin_id = &record.target().plugin_id;
    configuration
        .luaskills
        .enabled_vulcan_plugins
        .insert(plugin_id.clone());
    configuration
        .luaskills
        .disabled_vulcan_plugins
        .remove(plugin_id);
    transaction.persist(&configuration, "保存原已选更新的插件策略失败")?;
    bridge.provider_configuration = configuration.clone();
    *confirmed = true;
    Ok(configuration)
}

/// Start the enabled path once with the exact prepared predecessor and the original native lineage.
/// 使用精确准备前驱及原原生谱系单次启动启用路径。
/// app/state publish service results; registry/owner retain resources; current_mcp is the captured actual access.
/// app/state 发布服务结果；registry/owner 保留资源；current_mcp 为已捕获实际访问。
/// Returns the final catalog only after real predecessor retirement, candidate effects and combined target transfer.
/// 仅在真实前驱退役、候选副作用及组合目标转交之后返回最终目录。
pub(super) async fn continue_enabled_update(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    registry: &pending_owner::PendingLocalPluginUpdateRegistry,
    owner: &Arc<pending_owner::PendingLocalPluginUpdate>,
    current_mcp: McpRuntimeAccess,
) -> Result<LuaSkillsCatalogUpdate, String> {
    // The executor borrows actual originals while the service registry retains the physical owner.
    // 服务注册表保留物理所有者时，执行器借用实际原资源。
    let mut resources = owner.resources.lock().await;
    pending_owner::require_current_manager(state, &resources)?;
    owner.retain_routing_publication(state, &resources)?;
    if resources.target.is_some() {
        return Err(String::from(
            "原启用更新已经开始；不能重放前驱排空或候选副作用。",
        ));
    }
    // Retained historical Prepared requests must satisfy the same support boundary before their first real drain.
    // 保留历史 Prepared 请求在首次真实排空前必须满足相同支持边界。
    {
        // The actual original pairs its frozen descriptor with the complete immutable old selection.
        // 实际原请求将固定描述符与完整不可变旧选择配对。
        let prepared = resources.prepared()?;
        validate_enabled_predecessor_restoration_selection(
            &prepared.predecessor,
            prepared.transaction.record().predecessor(),
        )?;
    }
    // Strong retention precedes old admission closure and every native or callback effect.
    // 强保留先于旧入场关闭及全部原生或回调副作用。
    resources.target = Some(RetainedEnabledPluginUpdate {
        current_mcp,
        predecessor_drain: None,
        staged: None,
        native: None,
        publication: None,
        target_policy_confirmed: false,
        restoration: None,
        completed: false,
    });
    // A manager clone preserves the actual formal parent and drain registry.
    // 管理器克隆保留实际正式父级及排空注册表。
    let runtime = resources.runtime.clone();
    // Freeze the real current Arc before the core compares it with the original candidate selection.
    // 在核心与原候选选择比较之前，固定真实当前 Arc。
    let expected = runtime.snapshot();
    // The first preparation's descriptor must still identify this exact old route.
    // 首次准备描述符必须仍标识此精确旧路由。
    let predecessor = &resources.prepared()?.predecessor;
    if !expected
        .plugins
        .get(&owner.plugin_id)
        .is_some_and(|current| {
            current.generation == predecessor.generation
                && current.package_root == predecessor.package_root
                && current.content_fingerprint == predecessor.content_fingerprint
                && current.runtime_state == VulcanPluginRuntimeState::Enabled
        })
    {
        return Err(String::from(
            "准备后的实际启用前驱已经变化；不能排空替代代次。",
        ));
    }
    // The candidate borrows the same non-cloneable transaction instead of mutable public discovery.
    // 候选借用相同不可克隆事务，而非可变公开发现。
    let candidate = resources
        .prepared()?
        .store
        .inspect_activation_candidate(&resources.prepared()?.transaction)
        .map_err(|error| error.to_string())?;
    // The core registers and closes this exact old route before returning its opaque original drain.
    // 核心在返回不透明原排空之前注册并关闭此精确旧路由。
    let drain = runtime
        .begin_activation_predecessor_drain(&candidate, &expected)
        .map_err(|error| error.to_string())?;
    // Inspection owns a real readiness Arc; release this completed borrowed view before eventual mutable selection commit.
    // 检查拥有真实就绪 Arc；最终可变选择提交之前，释放此已完成借用视图。
    drop(candidate);
    resources
        .target
        .as_mut()
        .expect("retained enabled original")
        .predecessor_drain = Some(drain);
    // Native drain covers the same original lineage, including unpublished and retiring owners.
    // 原生排空覆盖相同原谱系，包括未发布及退役中所有者。
    let target = resources
        .target
        .as_ref()
        .expect("retained enabled original");
    target
        .current_mcp
        .begin_plugin_mcp_drain(&owner.plugin_id)?;
    target
        .current_mcp
        .wait_plugin_mcp_drain(
            &owner.plugin_id,
            AgentRuntimeBridge::RETIRED_MCP_PARTITION_DRAIN_TIMEOUT,
        )
        .await?;
    // The actual drain remains retained after its registry entry checkpoints physical retirement.
    // 注册表条目保存物理退役检查点后，实际排空仍保持保留。
    let drain = target
        .predecessor_drain
        .as_ref()
        .expect("retained original drain");
    // No candidate initialization is admitted before every real old Lua resource has retired.
    // 全部真实旧 Lua 资源退役之前，不接纳任何候选初始化。
    let drained = runtime
        .drain_activation_predecessor(drain)
        .await
        .map_err(|error| error.to_string())?;
    if !drained.failures.is_empty() {
        return Err(format!(
            "启用前驱资源尚未退役：{}",
            drained.failures.join("；")
        ));
    }
    // The old disable callback consumes the frozen descriptor and its own single-attempt receipt.
    // 旧停用回调消费固定描述符及各自单次尝试回执。
    let candidate = resources
        .prepared()?
        .store
        .inspect_activation_candidate(&resources.prepared()?.transaction)
        .map_err(|error| error.to_string())?;
    runtime
        .disable_activation_predecessor(&candidate, drain)
        .await
        .map_err(|error| error.to_string())?;
    // The core supervises actual callback retirement; this local inspection lease must not survive into commit.
    // 核心监督实际回调退役；此局部检查租约不能跨越到提交。
    drop(candidate);
    // Observe actual old callback cleanup separately from its historical successful receipt.
    // 独立于历史成功回执观察实际旧回调清理。
    let retired = runtime
        .drain_activation_predecessor_calls(owner.request_id)
        .await
        .map_err(|error| error.to_string())?;
    if !retired.failures.is_empty() {
        return Err(format!(
            "原停用回调资源尚未退役：{}",
            retired.failures.join("；")
        ));
    }
    // Release only the borrow so the existing validation worker can borrow this same retained slot.
    // 仅释放借用，让既有校验工作线程借用此相同保留槽。
    drop(resources);
    validate_and_install_retained(owner).await?;
    // The actual enable operation is private and never admits normal target business.
    // 实际启用操作保持私有，绝不接纳普通目标业务。
    let resources = owner.resources.lock().await;
    let candidate = resources
        .prepared()?
        .store
        .inspect_activation_candidate(&resources.prepared()?.transaction)
        .map_err(|error| error.to_string())?;
    resources
        .runtime
        .enable_activation_candidate(&candidate)
        .await
        .map_err(|error| error.to_string())?;
    // Shadowing an earlier candidate does not drop its readiness Arc; explicitly end each successful inspection scope.
    // 遮蔽先前候选不会销毁其就绪 Arc；明确结束每次成功检查作用域。
    drop(candidate);
    // Every candidate native call must retire before selection can be committed.
    // 选择提交之前，全部候选原生调用必须退役。
    let retired = resources
        .runtime
        .drain_activation_candidate_calls(owner.request_id)
        .await
        .map_err(|error| error.to_string())?;
    if !retired.failures.is_empty() {
        return Err(format!(
            "原候选回调资源尚未退役：{}",
            retired.failures.join("；")
        ));
    }
    drop(resources);
    select_and_publish_enabled_update(app, state, registry, owner, false).await
}

/// Resume only the selected original enabled target without replaying predecessor or candidate effects.
/// 仅恢复已选原启用目标，不重放前驱或候选副作用。
/// app/state/registry/owner identify the same retained service request; returns the actual combined result.
/// app/state/registry/owner 标识相同保留服务请求；返回实际组合结果。
pub(super) async fn resume_enabled_update(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    registry: &pending_owner::PendingLocalPluginUpdateRegistry,
    owner: &Arc<pending_owner::PendingLocalPluginUpdate>,
) -> Result<LuaSkillsCatalogUpdate, String> {
    select_and_publish_enabled_update(app, state, registry, owner, true).await
}

/// Commit or observe the exact selected original, then prepare real target owners before one combined publisher.
/// 提交或观察精确已选原请求，然后在单个组合发布者之前准备真实目标所有者。
/// recovering forbids replay of prepared effects; all other inputs retain the service's actual original request.
/// recovering 禁止重放准备副作用；其余输入保留服务实际原请求。
/// Returns only after true Full publication and durable completion have transferred the same Hook and MCP owners.
/// 仅在真实 Full 发布及持久完成转交相同 Hook 与 MCP 所有者之后返回。
async fn select_and_publish_enabled_update(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    registry: &pending_owner::PendingLocalPluginUpdateRegistry,
    owner: &Arc<pending_owner::PendingLocalPluginUpdate>,
    recovering: bool,
) -> Result<LuaSkillsCatalogUpdate, String> {
    pending_owner::admitted_registry(state)?;
    // Keep the original phase unchanged until the combined core path confirms unknown Completed outcomes.
    // 组合核心路径确认未知 Completed 结果之前，保持原阶段不变。
    let mut resources = owner.resources.lock().await;
    pending_owner::require_current_manager(state, &resources)?;
    if resources.target.is_none() {
        return Err(String::from(
            "原启用更新缺少实际服务参与者；不能由历史回执重建资源。",
        ));
    }
    // A previous successful transfer only revalidates its actual current Full and observes candidate release.
    // 先前成功转交仅重新校验实际当前 Full，并观察候选释放。
    if resources
        .target
        .as_ref()
        .expect("retained enabled original")
        .completed
    {
        resources
            .target
            .as_ref()
            .expect("retained enabled original")
            .publication
            .as_ref()
            .ok_or("已确认目标缺少实际 Full 见证。")?
            .validate_current(state, &resources.runtime)?;
        // Reopen owned MCP admission only after the actual completed Full witness still matches the service.
        // 仅在实际已完成 Full 见证仍匹配服务之后，重新开放自有 MCP 入场。
        resources
            .target
            .as_ref()
            .expect("retained enabled original")
            .current_mcp
            .finish_plugin_owner_drain(&owner.plugin_id)?;
        retire_candidate_control(&mut resources).await?;
        registry.retire(owner, &resources)?;
        drop(resources);
        return refresh_plugin_catalog_local_state_and_emit(app, state).await;
    }
    // The serialized policy guard covers exact selection and every Full publication attempt.
    // 串行化策略守卫覆盖精确选择及全部 Full 发布尝试。
    let policy = crate::service_state_access::begin_provider_configuration_transaction().await;
    // Before selected-target policy confirmation, preserve the original baseline so prepublication abort never guesses a rollback.
    // 在所选目标策略确认之前保留原基线，避免发布前中止猜测回退。
    let configuration = provider_configuration_from_state(state)?;
    if resources
        .target
        .as_ref()
        .expect("retained enabled original")
        .target_policy_confirmed
    {
        require_enabled_policy(
            resources.prepared()?.transaction.record(),
            &configuration.luaskills,
        )?;
    } else {
        require_original_enabled_policy(
            resources.prepared()?.transaction.record(),
            &configuration.luaskills,
        )?;
    }
    // Read the retained original phase without changing its sealed publication identity.
    // 读取保留原阶段，不改变其密封发布身份。
    let phase = resources.prepared()?.transaction.record().phase();
    if recovering
        && !matches!(
            phase,
            VulcanPluginActivationTransactionPhase::Committing
                | VulcanPluginActivationTransactionPhase::Published
                | VulcanPluginActivationTransactionPhase::Completed
        )
    {
        return Err(String::from(
            "原启用更新尚未进入选择提交；必须检查原副作用，不能重放验证或安装。",
        ));
    }
    if !matches!(
        phase,
        VulcanPluginActivationTransactionPhase::Published
            | VulcanPluginActivationTransactionPhase::Completed
    ) {
        // Commit reconciles only the same original selection and cannot restart any lifecycle callback.
        // 提交仅核对相同原选择，不能重启任何生命周期回调。
        let prepared = resources.prepared_mut()?;
        prepared
            .store
            .commit_activation_transaction(&mut prepared.transaction)
            .map_err(|error| error.to_string())?;
    }
    // Only a confirmed selected target may normalize its plugin policy; an uncertain selection keeps the old baseline.
    // 仅确认已选目标才能归一化其插件策略；不确定选择保持旧基线。
    let configuration = {
        // Split the actual transaction and confirmation without moving either out of its original owner.
        // 分别借用实际事务与确认，不将任一移出原所有者。
        let pending_owner::PendingLocalPluginUpdateResources { stage, target, .. } =
            &mut *resources;
        // A retained configured owner is mandatory after actual selection.
        // 实际选择之后必须存在保留已配置所有者。
        let pending_owner::PendingLocalPluginUpdateStage::Prepared(prepared) = stage else {
            return Err(String::from("原目标策略提交的准备类型已经变化。"));
        };
        // The confirmation is written in the same infallible tail as the actual in-memory configuration replacement.
        // 确认在实际内存配置替换的同一不可失败尾段写入。
        persist_selected_enabled_policy(
            state,
            &policy,
            prepared.transaction.record(),
            &mut target
                .as_mut()
                .expect("retained enabled original")
                .target_policy_confirmed,
        )?
    };
    // Freeze the actual closed target Arc once; later failure retains it and every initialized owner.
    // 单次固定实际关闭目标 Arc；后续失败保留它及全部已初始化所有者。
    if resources
        .target
        .as_ref()
        .expect("retained enabled original")
        .staged
        .is_none()
    {
        let staged = resources
            .runtime
            .stage_activation_target_handoff(
                &resources.prepared()?.transaction,
                &configuration.luaskills.disabled_vulcan_plugins,
                &configuration.luaskills.enabled_vulcan_plugins,
            )
            .map_err(|error| error.to_string())?;
        resources
            .target
            .as_mut()
            .expect("retained enabled original")
            .staged = Some(staged);
    }
    // An already published Full witness resumes only confirmation; it cannot initialize another Hook or native target.
    // 已发布 Full 见证仅恢复确认；不能初始化另一 Hook 或原生目标。
    if resources
        .target
        .as_ref()
        .expect("retained enabled original")
        .publication
        .is_none()
    {
        // Cloning the staging Arc preserves exact pointer identity and the original route.
        // 克隆暂存 Arc 保留精确指针身份及原路由。
        let staged = Arc::clone(
            resources
                .target
                .as_ref()
                .expect("retained enabled original")
                .staged
                .as_ref()
                .expect("staged enabled original"),
        );
        resources
            .runtime
            .prewarm_activation_target_hook(&resources.prepared()?.transaction, &staged)
            .await
            .map_err(|error| error.to_string())?;
        // Only actual global declarations require a native transfer participant.
        // 仅实际全局声明要求原生转交参与者。
        let has_global = staged
            .plugins
            .get(&owner.plugin_id)
            .ok_or("实际暂存目标缺失。")?
            .mcp_servers
            .iter()
            .any(|server| {
                server.manifest.scope == vulcan_plugin_runtime::VulcanPluginMcpScopeManifest::Global
            });
        if has_global {
            if resources
                .target
                .as_ref()
                .expect("retained enabled original")
                .native
                .is_none()
            {
                // The blocking materializer borrows this same retained transaction rather than moving it out.
                // 阻塞物化器借用此相同保留事务，而非将其移出。
                drop(resources);
                // Only the owning Arc crosses the worker boundary; the actual source remains in the service slot.
                // 仅所属 Arc 跨工作线程边界；实际来源保持在服务槽中。
                let preparing = Arc::clone(owner);
                // Physical pins and exact values return together without a detached server configuration.
                // 物理固定句柄及精确值共同返回，不产生脱离所有者的服务器配置。
                let inputs = run_extension_management_blocking(
                    "prepare retained selected target MCP",
                    move || {
                        // The exclusive borrow keeps the original owner alive through materialization errors and panic.
                        // 排他借用跨物化错误及 panic 保持原所有者存活。
                        let resources = preparing.resources.blocking_lock();
                        // The stored staging pointer is the sole target routing authority.
                        // 已存暂存指针为唯一目标路由权威。
                        let staged = resources
                            .target
                            .as_ref()
                            .ok_or("原启用更新资源缺失。")?
                            .staged
                            .as_ref()
                            .ok_or("原目标暂存身份缺失。")?;
                        // Sealed preparation uses the original registered issuer and actual immutable target.
                        // 密封准备使用原注册签发者及实际不可变目标。
                        let prepared = resources
                            .runtime
                            .prepare_mcp_activation_target(
                                &resources.prepared()?.transaction,
                                staged,
                                &resources.authority,
                            )
                            .map_err(|error| error.to_string())?;
                        target_global_mcp_server_configs(prepared, &OsPluginMcpSecretStore)
                    },
                )
                .await?;
                resources = owner.resources.lock().await;
                pending_owner::require_current_manager(state, &resources)?;
                // Start and retain real native control synchronously before the first readiness await.
                // 首次就绪 await 之前，同步启动并保留真实原生控制。
                let native = TargetPluginMcpRuntime::start(
                    &resources
                        .target
                        .as_ref()
                        .expect("retained enabled original")
                        .current_mcp,
                    &resources.runtime,
                    &resources.prepared()?.transaction,
                    inputs,
                    &resources.authority,
                )?;
                resources
                    .target
                    .as_mut()
                    .expect("retained enabled original")
                    .native = Some(native);
            }
            // Historical phase selects same-controller observation; the actual transfer guard still proves live readiness.
            // 历史阶段选择相同控制对象观察；实际转交守卫仍证明实时就绪。
            let target = resources
                .target
                .as_ref()
                .expect("retained enabled original");
            // No diagnostic UUID may substitute for the actual retained native instance pair.
            // 任何诊断 UUID 都不能替代实际保留原生实例对。
            let native = target
                .native
                .as_ref()
                .expect("retained original native target");
            // Strict original receipt validates full selection and recorded actual startup identities.
            // 严格原回执校验完整选择及记录的实际启动身份。
            let receipt = resources
                .prepared()?
                .store
                .target_mcp_initialization_receipt(&owner.plugin_id, owner.request_id)
                .map_err(|error| error.to_string())?
                .ok_or("实际原目标 MCP 启动回执缺失。")?;
            if receipt.request_id() != owner.request_id
                || receipt.target() != resources.prepared()?.transaction.record().target()
                || receipt.routing_generation()
                    != staged
                        .plugins
                        .get(&owner.plugin_id)
                        .ok_or("实际暂存目标缺失。")?
                        .generation
                || receipt.instance_ids() != Some(native.actual_instance_ids())
            {
                return Err(String::from(
                    "原目标 MCP 回执与实际保留实例不一致；不能替换资源。",
                ));
            }
            match receipt.phase() {
                // Only an unfinished same-owner observation can finish the original readiness write.
                // 仅未完成同所有者观察可以完成原就绪写入。
                VulcanPluginTargetMcpInitializationPhase::Started => {
                    native.wait_until_ready_default().await?
                }
                // Actual GAT guards revalidate this same backend without recreating or rewriting historical Ready.
                // 实际 GAT 守卫重新校验相同后端，不重建或改写历史 Ready。
                VulcanPluginTargetMcpInitializationPhase::Ready => {}
                // A failed startup stays non-replayable and retains its real control for inspection.
                // 失败启动保持不可重放，并保留真实控制用于检查。
                VulcanPluginTargetMcpInitializationPhase::ObservationFailed => {
                    return Err(String::from(
                        "原目标 MCP 启动已观测失败；保留原实例，不重启或重放。",
                    ));
                }
            }
        }
    }
    // Split concrete fields without moving any original participant out of its strong service slot.
    // 分别借用具体字段，不将任何原参与者移出强服务槽。
    let pending_owner::PendingLocalPluginUpdateResources { stage, target, .. } = &mut *resources;
    // Prepared is the already retained non-cloneable original request, never a reconstructed disk handle.
    // Prepared 为已保留不可克隆原请求，绝非重建磁盘句柄。
    let pending_owner::PendingLocalPluginUpdateStage::Prepared(prepared) = stage else {
        return Err(String::from("原目标交接的准备类型已经变化。"));
    };
    // The same physical target slot survives all publisher and unknown completion failures.
    // 相同物理目标槽跨全部发布者及未知完成失败存活。
    let target = target.as_mut().expect("retained enabled original");
    owner.retain_full_publication(state)?;
    // A failed precommit attempt may release exclusion; an actual retained pair must survive through registry retirement.
    // 失败的提交前尝试可以释放排他；实际保留配对必须存续至注册表退役。
    let publication =
        vulcan_plugin_lifecycle::publish_activation_target_plugin_extension_generation(
            state,
            &mut prepared.transaction,
            target.staged.as_ref().expect("staged enabled original"),
            target.native.as_ref(),
            &mut target.publication,
        );
    if publication.is_err() && target.publication.is_none() {
        owner.release_unpublished_full_publication()?;
    }
    publication?;
    target.completed = true;
    // This separate owned-MCP fence opens only after true durable completion transferred all target participants.
    // 仅在真实持久完成转交全部目标参与者之后，才开放此独立自有 MCP 屏障。
    target
        .current_mcp
        .finish_plugin_owner_drain(&owner.plugin_id)?;
    drop(policy);
    retire_candidate_control(&mut resources).await?;
    registry.retire(owner, &resources)?;
    drop(resources);
    refresh_plugin_catalog_local_state_and_emit(app, state).await
}
