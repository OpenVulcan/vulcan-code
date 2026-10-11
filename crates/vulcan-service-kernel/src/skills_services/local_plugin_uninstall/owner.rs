//! The service map owns original physical resources independently from every executor.
//! 服务映射持有原物理资源，独立于每个执行器。

use super::*;

/// A managed removal borrows the existing store protocol, without a parallel ledger.
/// 受管移除借用既有存储协议，不建立并行账本。
pub(super) struct OriginalRemoval {
    /// Existing canonical store performs all history collection.
    /// 既有规范存储执行全部历史回收。
    pub(super) store: VulcanPluginGenerationStore,
    /// Sole namespace File and original complete selected authority.
    /// 唯一命名空间文件及原完整已选权威。
    pub(super) selection: VulcanPluginDeactivationReservation,
    /// Local environment cleanup shares the selection through a weak allocation binding.
    /// 本地环境清理通过弱分配绑定共享选择。
    pub(super) state: VulcanPluginDeactivationStateReservation,
}

/// Actual resource borrowing is separate from short registry and service publication locks.
/// 实际资源借用独立于短注册表锁及服务发布锁。
pub(super) struct UninstallResources {
    /// The same manager is retained, never reconstructed from paths.
    /// 保留相同管理器，绝不从路径重建。
    pub(super) runtime: VulcanPluginRuntimeManager,
    /// First actual issuer is pure manager identity, never a namespace or native resource owner.
    /// 首次实际签发者为纯管理器身份，绝不是命名空间或原生资源所有者。
    pub(super) authority: vulcan_plugin_runtime::VulcanPluginRegisteredNativeMcpHostAuthority,
    /// Only confirmed terminal retirement takes this physical original.
    /// 仅已确认终态退役取出此物理原实例。
    pub(super) original: Option<OriginalRemoval>,
    /// Retain before start; observer loss leaves the same callback available for cleanup.
    /// 在启动之前保留；观测者丢失后仍可清理相同回调。
    pub(super) callback: Option<VulcanPluginRemovalLifecycleCall>,
    /// None means not physically retired, independently from observable business completion.
    /// None 表示尚未物理退役，独立于可观测业务完成。
    pub(super) callback_result: Option<Result<(), String>>,
    /// Explicit acknowledgement applies only to the retained already-retired failed result.
    /// 明确确认仅作用于保留且已退役的失败结果。
    pub(super) effects_acknowledged: bool,
    /// Actual current MCP access supplies the complete union drain until Full removal.
    /// 实际当前 MCP 访问面在完整移除之前提供完整集合排空。
    pub(super) mcp: Option<McpRuntimeAccess>,
    /// Original complete plugin policy membership, captured before lifecycle effects.
    /// 生命周期副作用前捕获的原完整插件策略成员关系。
    pub(super) original_policy: (bool, bool),
    /// True precedes persistence and explicitly admits only original or intended membership on retry.
    /// 真值先于持久化，并在重试时明确仅准入原或期望成员关系。
    pub(super) policy_started: bool,
    /// Adjacent configuration commit confirms the exact intended membership.
    /// 相邻配置提交确认精确期望成员关系。
    pub(super) policy_committed: bool,
    /// The original Full must not be substituted before removal publication.
    /// 移除发布之前不得替代原完整代次。
    pub(super) expected_full: Option<Arc<ExtensionGeneration>>,
    /// Actual infallible neighboring publication callback retains the committed Full.
    /// 实际不可失败相邻发布回调保留已提交完整代次。
    pub(super) committed_full: Option<Arc<ExtensionGeneration>>,
    /// Actual collection/readback completion never derives from UUID or hidden source alone.
    /// 实际回收／回读完成绝不从 UUID 或仅隐藏来源派生。
    pub(super) collection_confirmed: bool,
    /// Exact neighboring workbench projection survives client delivery panic without another revision or Full replacement.
    /// 精确相邻工作台投影跨客户端投递 panic 存续，不再次递增修订或替换完整代次。
    pub(super) projection:
        Option<crate::settings_runtime_projection::PluginCatalogProjectionUpdate>,
}

/// A non-cloneable physical original stays in one strong registry slot until true completion.
/// 不可克隆物理原实例留在单个强注册表槽内，直至真实完成。
pub(super) struct PendingLocalPluginUninstall {
    /// UUID is immutable lookup identity, not completion authority.
    /// UUID 为不可变查询身份，不是完成权威。
    pub(super) request_id: uuid::Uuid,
    /// Exact namespace from the captured source.
    /// 来自捕获来源的精确命名空间。
    pub(super) plugin_id: String,
    /// The same existing manager handle supports read-only queries without the mutable execution borrow.
    /// 相同既有管理器句柄无需可变执行借用即可支持只读查询。
    /// Cloning its handle creates no second runtime, namespace reservation or execution authority.
    /// 克隆其句柄不创建第二运行时、命名空间预留或执行权威。
    diagnostic_runtime: VulcanPluginRuntimeManager,
    /// Original incarnation is captured from the actual catalog before callbacks or publication.
    /// 原实例在回调或发布之前从实际目录捕获。
    service_epoch: String,
    /// Original disabled descriptor fixes the only permitted callback routing generation.
    /// 原停用描述符固定唯一获准回调路由代次。
    plugin_generation: u64,
    /// Guard binds weak namespace identity and never adds a second physical namespace owner.
    /// 守卫绑定弱命名空间身份，绝不添加第二物理命名空间所有者。
    routing: Mutex<Option<VulcanPluginRoutingReservation>>,
    /// The asynchronous exclusive borrow outlives no actual strong map entry.
    /// 异步排他借用由实际强映射条目支持。
    pub(super) resources: tokio::sync::Mutex<UninstallResources>,
}

/// No expiration, phase-based reconstruction or automatic disappearance is allowed.
/// 不允许过期、根据阶段重建或自动消失。
#[derive(Default)]
pub(crate) struct PendingLocalPluginUninstallRegistry {
    /// Every unfinished original remains strongly owned here.
    /// 每个未完成原实例均在此被强持有。
    owners: Mutex<BTreeMap<uuid::Uuid, Arc<PendingLocalPluginUninstall>>>,
}

impl PendingLocalPluginUninstallRegistry {
    /// Read runtime_id/plugin/request's exact live original without borrowing or advancing mutable removal resources.
    /// 读取 runtime_id/plugin/request 的精确实时原实例，不借用或推进可变移除资源。
    /// Return metadata only while the same owner remains registered; missing or changed ownership is explicit failure.
    /// 仅相同所有者仍注册时返回元数据；所有权缺失或改变时明确失败。
    pub(crate) fn drain_diagnostics(
        &self,
        runtime_id: &str,
        plugin: &str,
        request: uuid::Uuid,
    ) -> Result<LocalPluginUninstallDrainDiagnostics, String> {
        // The short lookup retains the actual owner; no async resource lock joins its running callback.
        // 短查找保留实际所有者；没有异步资源锁联接其正在运行的回调。
        let owner = self.lookup(plugin, request)?;
        crate::plugin_runtime_services::diagnostics::require_runtime_identity(
            &owner.diagnostic_runtime,
            runtime_id,
        )
        .map_err(|error| error.to_string())?;
        // Actual uninstall execution drains the whole namespace, including older retained cleanup owners.
        // 实际卸载执行排空整个命名空间，包含更早保留的清理所有者。
        // The frozen source generation is context, not a filter that could hide those real participants.
        // 冻结来源代次是上下文，不能作为隐藏真实参与者的过滤器。
        let mut resources = owner
            .diagnostic_runtime
            .formal_plugin_resource_diagnostics(plugin, None)
            .map_err(|error| error.to_string())?;
        // Request diagnostics show only live physical participants; general plugin diagnostics retain history separately.
        // 请求诊断仅显示实时物理参与者；通用插件诊断独立保留历史。
        if let Some(sample) = &mut resources {
            sample
                .resources
                .retain(|resource| resource.is_drain_blocking);
        }
        if resources
            .as_ref()
            .is_some_and(|sample| sample.service_epoch != owner.service_epoch)
        {
            return Err(String::from(
                "Plugin uninstall diagnostic manager incarnation changed.",
            ));
        }
        // A completed removal must not donate its original request identity to later selected resources.
        // 已完成移除不能将其原请求身份赠予后续已选资源。
        let current = self.lookup(plugin, request)?;
        if !Arc::ptr_eq(&current, &owner) {
            return Err(String::from("Plugin uninstall diagnostic owner changed."));
        }
        Ok(LocalPluginUninstallDrainDiagnostics {
            request_id: owner.request_id.to_string(),
            plugin_id: owner.plugin_id.clone(),
            service_epoch: owner.service_epoch.clone(),
            plugin_generation: owner.plugin_generation.to_string(),
            resources,
        })
    }

    /// Exempt only actual original membership from coordination; request grants no physical authority.
    /// 仅豁免实际原成员的协调；request 不授予物理权威。
    /// Returns rejection before ordinary writers can persist or replace Full.
    /// 在普通写入器能够持久化或替换完整代次之前返回拒绝。
    pub(crate) fn require_full_publication(
        &self,
        request: Option<uuid::Uuid>,
    ) -> Result<(), String> {
        // The short map never joins asynchronous resource borrowers.
        // 短映射绝不联接异步资源借用者。
        let owners = self.owners.lock().map_err(|error| error.to_string())?;
        if let Some(owner) = owners
            .values()
            .find(|owner| Some(owner.request_id) != request)
        {
            return Err(format!(
                "Original uninstall request `{}` still holds actual resources; continue the same request first.",
                owner.request_id
            ));
        }
        Ok(())
    }

    /// Locate the same concrete original using both exact identities; no disk fallback exists.
    /// 使用两个精确身份定位相同具体原实例；不存在磁盘兜底。
    /// Returns its actual Arc or an explicit missing-owner error.
    /// 返回其实际引用，或明确所有者缺失错误。
    pub(super) fn lookup(
        &self,
        plugin: &str,
        request: uuid::Uuid,
    ) -> Result<Arc<PendingLocalPluginUninstall>, String> {
        // Borrow map only long enough to clone an existing strong owner.
        // 仅在克隆既有强所有者所需时间内借用映射。
        let owners = self.owners.lock().map_err(|error| error.to_string())?;
        owners
            .get(&request)
            .filter(|owner| owner.plugin_id == plugin)
            .cloned()
            .ok_or_else(|| String::from("The same actual original uninstall owner was not found; the uninstall cannot be rebuilt."))
    }

    /// Read all sanitized owners; no parameters, no mutation and no waiting on active execution.
    /// 读取全部脱敏所有者；无参数、不变更、不等待活动执行。
    /// Returns executing when the real resource borrow is unavailable.
    /// 无法取得真实资源借用时返回执行中。
    pub(crate) fn statuses(&self) -> Result<Vec<LocalPluginUninstallStatus>, String> {
        // Release the map before trying any resource mutex to preserve the publication lock order.
        // 尝试任何资源互斥锁之前释放映射，以保持发布锁顺序。
        let owners = self
            .owners
            .lock()
            .map_err(|error| error.to_string())?
            .values()
            .cloned()
            .collect::<Vec<_>>();
        Ok(owners
            .into_iter()
            .map(|owner| {
                // Failure to borrow means executing, without guessing a hidden stage or acknowledgement.
                // 借用失败表示执行中，不猜测隐藏阶段或确认。
                let observation = owner.resources.try_lock();
                // Only actual owned fields project a non-executing phase.
                // 仅实际持有字段投影非执行中阶段。
                let (phase, requires_ack) = match observation.as_ref() {
                    Err(_) => (LocalPluginUninstallPhase::Executing, false),
                    Ok(resources) => (
                        if resources.committed_full.is_some() {
                            LocalPluginUninstallPhase::Collection
                        } else if resources.callback_result.is_some() {
                            LocalPluginUninstallPhase::Publishing
                        } else if resources.callback.is_some() {
                            LocalPluginUninstallPhase::Lifecycle
                        } else {
                            LocalPluginUninstallPhase::Reserved
                        },
                        resources
                            .callback_result
                            .as_ref()
                            .is_some_and(Result::is_err)
                            && !resources.effects_acknowledged,
                    ),
                };
                LocalPluginUninstallStatus {
                    request_id: owner.request_id.to_string(),
                    plugin_id: owner.plugin_id.clone(),
                    phase,
                    requires_external_effects_acknowledgement: requires_ack,
                }
            })
            .collect())
    }

    /// Release files only after actual terminal evidence and exact registry membership, under service exclusion.
    /// 在服务互斥内，仅于实际终态证据及精确注册表成员关系之后释放文件。
    /// owner/resources must be the same exclusively borrowed slot; returns unit after physical release.
    /// owner／resources 必须为相同排他借用槽；物理释放后返回空值。
    pub(super) fn retire(
        &self,
        owner: &Arc<PendingLocalPluginUninstall>,
        resources: &mut UninstallResources,
    ) -> Result<(), String> {
        // Membership is verified before releasing physical ownership even if other executor Arcs remain.
        // 即使其他执行器引用仍保留，也在释放物理所有权之前校验成员关系。
        let mut owners = self.owners.lock().map_err(|error| error.to_string())?;
        if !owners
            .get(&owner.request_id)
            .is_some_and(|current| Arc::ptr_eq(current, owner))
            || !resources.collection_confirmed
            || resources.callback.is_some()
            || resources.committed_full.is_none()
            || !resources.policy_committed
            || resources.projection.is_none()
        {
            return Err(String::from(
                "The resource or terminal-state confirmation of the original uninstall does not match; keeping the physical owner.",
            ));
        }
        // Acquire every fallible coordination lock before releasing any physical original.
        // 释放任何物理原实例之前取得每个可失败协调锁。
        let mut routing = owner.routing.lock().map_err(|error| error.to_string())?;
        resources.original.take();
        resources.mcp.take();
        resources.expected_full.take();
        routing.take();
        owners.remove(&owner.request_id);
        Ok(())
    }

    /// Without an executor runtime, every nonempty physical original must prevent irreversible shutdown.
    /// 没有执行器运行时时，每个非空物理原实例均必须阻止不可逆关闭。
    /// Returns success only for a truly empty strong map.
    /// 仅真实空强映射返回成功。
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
                "The actual original uninstall has not finished; the service runtime must be kept.",
            ))
        }
    }

    /// Drain only existing callbacks/native resources; shutdown never publishes, acknowledges effects or clears originals.
    /// 仅排空既有回调／原生资源；关闭绝不发布、确认副作用或清除原实例。
    /// No parameters; returns unfinished-owner failure after bounded physical drain.
    /// 无参数；有界物理排空之后返回未完成所有者失败。
    pub(crate) async fn drain_for_shutdown(&self) -> Result<(), String> {
        // Snapshot actual owners without retaining their registry lock across waits.
        // 快照实际所有者，不跨等待保留注册表锁。
        let owners = self
            .owners
            .lock()
            .map_err(|error| error.to_string())?
            .values()
            .cloned()
            .collect::<Vec<_>>();
        for owner in owners {
            // Do not wait indefinitely for a foreground executor while shutdown admission is closed.
            // 关闭准入已关闭时，不无限等待前台执行器。
            let mut resources = owner
                .resources
                .try_lock()
                .map_err(|_| String::from("The original uninstall executor still borrows actual resources; shutdown cannot continue."))?;
            if let Some(call) = resources.callback.as_mut() {
                call.drain_cleanup()
                    .await
                    .map_err(|error| error.to_string())?;
                // Record business failure only after outer physical retirement succeeds, without automatic acknowledgement.
                // 仅在外层物理退役成功之后记录业务失败，不自动确认。
                let result = call
                    .confirm_retired()
                    .map_err(|error| error.to_string())?
                    .map_err(|error| error.to_string());
                resources.callback_result = Some(result);
                resources.callback.take();
            }
            execution::drain(&owner.plugin_id, &mut resources).await?;
        }
        self.require_empty_without_runtime()
    }
}

/// Validate actual registry membership and shutdown admission; state/owner borrow no replacement authority.
/// 校验实际注册表成员关系及关闭准入；state／owner 不借用替代权威。
/// Returns unit only for the exact registered Arc before effects or publication.
/// 仅副作用或发布之前的精确登记引用返回空值。
pub(super) fn require_current(
    state: &SharedServiceBridgeState,
    owner: &Arc<PendingLocalPluginUninstall>,
) -> Result<(), String> {
    // Publication exclusion fixes map membership and the sole service bridge.
    // 发布互斥固定映射成员关系及唯一服务桥。
    let service = state.lock().map_err(|error| error.to_string())?;
    crate::service_state_access::require_plugin_publication_shutdown_admission(&service)?;
    // Equal UUID metadata cannot substitute for this actual retained allocation.
    // 相等 UUID 元数据不能替代此实际保留分配。
    let current = service
        .pending_local_plugin_uninstalls
        .lookup(&owner.plugin_id, owner.request_id)?;
    if !Arc::ptr_eq(&current, owner) {
        return Err(String::from(
            "The actual service slot of the original uninstall changed.",
        ));
    }
    crate::service_state_access::require_plugin_update_full_publication_admission(
        &service,
        Some(owner.request_id),
    )
}

/// Capture inert concrete originals under service exclusion before callbacks, policy writes or package hiding.
/// 在服务互斥内捕获未启动具体原实例，先于回调、策略写入或插件包隐藏。
/// state/plugin select an actual disabled user descriptor; returns its retained sole strong slot.
/// state／plugin 选择实际停用用户描述符；返回其保留唯一强槽。
pub(super) fn reserve(
    state: &SharedServiceBridgeState,
    plugin: &str,
) -> Result<Arc<PendingLocalPluginUninstall>, String> {
    // Lock order matches the Full publisher: service, registry and actual Core registration.
    // 锁顺序匹配完整发布器：服务、注册表及实际核心登记。
    let service = state.lock().map_err(|error| error.to_string())?;
    crate::service_state_access::require_plugin_publication_shutdown_admission(&service)?;
    crate::service_state_access::require_plugin_update_full_publication_admission(&service, None)?;
    // The actual bridge supplies manager, original policy and original Full together.
    // 实际桥共同提供管理器、原策略及原完整代次。
    let bridge = service
        .agent_runtime
        .as_ref()
        .ok_or("The agent extension generation is not initialized yet.")?;
    // The snapshot is temporary and is dropped before any physical collection.
    // 快照为临时对象，在任何物理回收之前释放。
    let catalog = bridge.vulcan_plugin_runtime.snapshot();
    // Absence is an explicit error, never guessed from the user plugin root.
    // 缺失为明确错误，绝不从用户插件根目录猜测。
    let descriptor = catalog
        .plugins
        .get(plugin)
        .ok_or("The user plugin is not installed yet.")?;
    if descriptor.origin != vulcan_plugin_runtime::VulcanPluginPackageOrigin::UserInstalled
        || descriptor.runtime_state != vulcan_plugin_runtime::VulcanPluginRuntimeState::Disabled
        || bridge
            .vulcan_plugin_runtime
            .active_session_count(plugin)
            .map_err(|error| error.to_string())?
            != 0
        || bridge
            .vulcan_plugin_runtime
            .has_cleanup_diagnostics(plugin)
            .map_err(|error| error.to_string())?
    {
        return Err(String::from(
            "The user plugin must be disabled and fully drained before it can be uninstalled.",
        ));
    }
    // Allocate only lookup metadata; concrete originals independently establish physical ownership.
    // 仅分配查询元数据；具体原实例独立建立物理所有权。
    let request_id = uuid::Uuid::new_v4();
    // Every user plugin is a managed selection that must exist in its actual configured store.
    // 每个用户插件都是受管选择，必须存在于其实际配置存储。
    if descriptor.managed_package().is_none() {
        return Err(String::from("The user plugin has no managed selection."));
    }
    // Existing stores are opened without creation.
    // 打开既有存储而不创建。
    let store =
        VulcanPluginGenerationStore::open_existing(bridge.vulcan_plugin_runtime.user_plugin_root())
            .map_err(|error| error.to_string())?
            .ok_or("The managed plugin storage is missing.")?;
    // Read the complete authoritative selection before reserving its same File.
    // 在预留相同文件之前读取完整权威选择。
    let selected = store
        .activation(plugin)
        .map_err(|error| error.to_string())?
        .ok_or("The complete managed plugin selection is missing.")?;
    // Sole physical reservation captures actual inventories before all effects.
    // 唯一物理预留在全部副作用之前捕获实际清单。
    let mut selection = store
        .reserve_deactivation(request_id, &selected)
        .map_err(|error| error.to_string())?;
    // Local state is an actual separate original sharing only weak namespace identity.
    // 本地状态为实际独立原实例，仅共享弱命名空间身份。
    let local = bridge
        .vulcan_plugin_runtime
        .reserve_deactivation_state(&mut selection)
        .map_err(|error| error.to_string())?;
    // Coordination holds no second strong namespace borrower.
    // 协调不持有第二强命名空间借用者。
    let routing = bridge
        .vulcan_plugin_runtime
        .reserve_deactivation_routing(&selection)
        .map_err(|error| error.to_string())?;
    let original = OriginalRemoval {
        store,
        selection,
        state: local,
    };
    // This exact slot captures policy before callback effects and is retained before leaving service exclusion.
    // 此精确槽在回调副作用之前捕获策略，并在离开服务互斥之前保留。
    let owner = Arc::new(PendingLocalPluginUninstall {
        request_id,
        plugin_id: plugin.to_owned(),
        diagnostic_runtime: bridge.vulcan_plugin_runtime.clone(),
        service_epoch: catalog.service_epoch.clone(),
        plugin_generation: descriptor.generation,
        routing: Mutex::new(Some(routing)),
        resources: tokio::sync::Mutex::new(UninstallResources {
            runtime: bridge.vulcan_plugin_runtime.clone(),
            authority: bridge.vulcan_plugin_native_mcp_host.clone(),
            original: Some(original),
            callback: None,
            callback_result: None,
            effects_acknowledged: false,
            mcp: Some(bridge.extension_generation.mcp_runtime()),
            original_policy: (
                bridge
                    .provider_configuration
                    .luaskills
                    .disabled_vulcan_plugins
                    .contains(plugin),
                bridge
                    .provider_configuration
                    .luaskills
                    .enabled_vulcan_plugins
                    .contains(plugin),
            ),
            policy_started: false,
            policy_committed: false,
            expected_full: Some(Arc::clone(&bridge.extension_generation)),
            committed_full: None,
            collection_confirmed: false,
            projection: None,
        }),
    });
    // Admission already excludes competing owners; insert never replaces an existing physical slot.
    // 准入已经排除竞争所有者；插入绝不替代既有物理槽。
    let mut owners = service
        .pending_local_plugin_uninstalls
        .owners
        .lock()
        .map_err(|error| error.to_string())?;
    if owners.contains_key(&request_id) || owners.values().any(|owner| owner.plugin_id == plugin) {
        return Err(String::from(
            "The same plugin already has an actual original uninstall.",
        ));
    }
    owners.insert(request_id, Arc::clone(&owner));
    Ok(owner)
}
