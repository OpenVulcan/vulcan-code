//! Retain one physical removal original until callback, publication and collection all finish.
//! 保留单个物理移除原实例，直至回调、发布及回收全部完成。

use super::*;
use std::collections::BTreeMap;
use std::sync::Mutex;
use vulcan_agent_runtime::agent_runtime::extension_generation::ExtensionGeneration;
use vulcan_agent_runtime::mcp::McpRuntimeAccess;
use vulcan_plugin_runtime::{
    VulcanPluginDeactivationReservation, VulcanPluginDeactivationStateReservation,
    VulcanPluginGenerationStore, VulcanPluginRemovalLifecycleCall, VulcanPluginResourceDiagnostics,
    VulcanPluginRoutingReservation, VulcanPluginRuntimeManager,
};

mod collection;
mod execution;
mod owner;
mod publication;
#[cfg(test)]
mod tests;
pub(crate) use owner::PendingLocalPluginUninstallRegistry;

/// Original removal metadata remains readable while its sole mutable resource borrow is executing.
/// 原移除元数据在其唯一可变资源借用执行时保持可读取。
/// This is a live non-atomic diagnostic sample, never a durable completion or resource-release receipt.
/// 此类型为实时非原子诊断采样，绝不是持久完成或资源释放回执。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LocalPluginUninstallDrainDiagnostics {
    /// Original process-local removal identity.
    /// 原进程内移除身份。
    request_id: String,
    /// Namespace frozen from the actual reserved descriptor.
    /// 从实际预留描述符冻结的命名空间。
    plugin_id: String,
    /// Original manager incarnation is independent from current catalog selection.
    /// 原管理器实例独立于当前目录选择。
    service_epoch: String,
    /// Original disabled routing generation is encoded without JavaScript rounding.
    /// 原停用路由代次编码时避免 JavaScript 舍入。
    plugin_generation: String,
    /// The real plugin-wide drain selects all retained generations in that same original manager.
    /// 真实插件级排空选择相同原管理器中全部保留代次。
    resources: Option<VulcanPluginResourceDiagnostics>,
}

/// A temporary exclusive borrow projects one real retained completed removal without creating another owner or authority.
/// 临时排他借用投影单个真实保留已完成移除，不创建另一所有者或权威。
pub(crate) struct LocalPluginUninstallProjection<'a> {
    /// Actual registry allocation is borrowed, never supplied as copied request metadata.
    /// 借用实际注册表分配，绝不提供复制请求元数据。
    original: &'a Arc<owner::PendingLocalPluginUninstall>,
    /// Same exclusively borrowed resources already contain the actual committed Full receipt.
    /// 相同排他借用资源已包含实际已提交完整回执。
    resources: &'a mut owner::UninstallResources,
}

impl LocalPluginUninstallProjection<'_> {
    /// Validate actual service/map/Full/catalog identities before Task or workbench projection.
    /// 在任务或工作台投影之前校验实际服务／映射／完整代次／目录身份。
    /// service is already locked and catalog is the actual manager snapshot; returns only coordination UUID after proof.
    /// service 已加锁，catalog 为实际管理器快照；仅在证明之后返回协调 UUID。
    pub(crate) fn validate(
        &self,
        service: &ServiceBridgeState,
        catalog: &Arc<vulcan_plugin_runtime::VulcanPluginCatalogSnapshot>,
    ) -> Result<uuid::Uuid, String> {
        // Same allocated owner is proved before any UUID coordination exemption.
        // 任何 UUID 协调豁免之前证明相同分配所有者。
        let current = service
            .pending_local_plugin_uninstalls
            .lookup(&self.original.plugin_id, self.original.request_id)?;
        if !Arc::ptr_eq(&current, self.original)
            || !self.resources.collection_confirmed
            || self.resources.callback.is_some()
        {
            return Err(String::from("原卸载投影必须使用相同实际已回收所有者。"));
        }
        // Genuine Full receipt comes only from its adjacent original production publication.
        // 真实完整回执仅来自相邻原生产发布。
        let committed = self
            .resources
            .committed_full
            .as_ref()
            .ok_or("原卸载完整回执缺失。")?;
        // Actual configured bridge owns both first issuer and the unique current Full pointer.
        // 实际配置桥拥有首次签发者及唯一当前完整指针。
        let bridge = service
            .agent_runtime
            .as_ref()
            .ok_or("Agent 扩展代次尚未初始化。")?;
        // Absence never supplies an alternative catalog authority.
        // 缺失绝不提供替代目录权威。
        let published = committed.plugin_catalog().ok_or("原卸载完整目录缺失。")?;
        if !Arc::ptr_eq(committed, &bridge.extension_generation)
            || !Arc::ptr_eq(&published, catalog)
            || !Arc::ptr_eq(catalog, &self.resources.runtime.snapshot())
            || !self
                .resources
                .authority
                .matches_manager(&bridge.vulcan_plugin_runtime)
            || !self
                .resources
                .authority
                .same_issuer(&bridge.vulcan_plugin_native_mcp_host)
            || catalog.plugins.contains_key(&self.original.plugin_id)
        {
            return Err(String::from(
                "原卸载投影的实际完整、目录或首次签发者已改变。",
            ));
        }
        Ok(self.original.request_id)
    }

    /// Store update adjacent to the real workbench commit without I/O, waits or callback business.
    /// 在真实工作台提交相邻存储 update，不执行输入输出、等待或回调业务。
    /// update is the exact production projection; no return value or new physical owner is created.
    /// update 为精确生产投影；不返回值或创建新物理所有者。
    pub(crate) fn record(
        &mut self,
        update: crate::settings_runtime_projection::PluginCatalogProjectionUpdate,
    ) {
        self.resources.projection = Some(update);
    }
}

/// Public status contains only identities and live borrow observations, never original private bytes.
/// 公开状态仅包含身份及实时借用观测，绝不包含原私有字节。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum LocalPluginUninstallPhase {
    /// An actual exclusive resource borrower is running.
    /// 实际排他资源借用者正在运行。
    Executing,
    /// Actual inert originals exist before callback registration.
    /// 实际未启动原实例在回调登记之前存在。
    Reserved,
    /// A same original callback handle remains unretired.
    /// 相同原回调句柄仍未退役。
    Lifecycle,
    /// Callback is retired while actual Full removal remains unconfirmed.
    /// 回调已退役，同时实际完整移除仍未确认。
    Publishing,
    /// Real Full removal is committed while collection/projection remain owned.
    /// 真实完整移除已提交，同时回收／投影仍被持有。
    Collection,
}

/// Sanitized response derives its typed phase from existing original ownership only.
/// 脱敏响应仅从既有原所有权派生类型明确阶段。
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LocalPluginUninstallStatus {
    /// The sole process-local original request UUID.
    /// 唯一进程内原请求 UUID。
    request_id: String,
    /// The namespace captured before any callback.
    /// 任何回调之前捕获的命名空间。
    plugin_id: String,
    /// Executing means the actual exclusive resource borrow could not be acquired.
    /// 执行中表示无法取得实际排他资源借用。
    phase: LocalPluginUninstallPhase,
    /// Only a retained physically retired failed callback requires explicit acknowledgement.
    /// 仅保留且实际退役的失败回调要求明确确认。
    requires_external_effects_acknowledgement: bool,
}

impl LocalPluginUninstallStatus {
    /// Exact original request UUID text used as the pending-operation identity.
    /// 用作待处理操作身份的精确原请求 UUID 文本。
    pub(super) fn request_id(&self) -> &str {
        &self.request_id
    }

    /// Namespace captured before any callback.
    /// 任何回调之前捕获的命名空间。
    pub(super) fn plugin_id(&self) -> &str {
        &self.plugin_id
    }

    /// Offer continuation unless an executor currently borrows the original; acknowledgement follows the retained failure.
    /// 除非执行器正借用原实例，否则提供继续操作；确认要求取决于保留的失败。
    pub(super) fn pending_actions(&self) -> Vec<super::plugin_pending_operations::PluginPendingActionOffer> {
        use super::plugin_pending_operations::{PluginPendingAction, PluginPendingActionOffer};
        if self.phase == LocalPluginUninstallPhase::Executing {
            return Vec::new();
        }
        vec![PluginPendingActionOffer {
            action: PluginPendingAction::Resume,
            requires_acknowledgement: self.requires_external_effects_acknowledgement,
        }]
    }
}

/// Reserve before effects; app/state own publication and plugin_id selects one disabled actual source.
/// 在副作用之前预留；app／state 拥有发布，plugin_id 选择单个停用实际来源。
/// Returns the existing catalog response only after actual terminal collection and projection.
/// 仅在实际终态回收及投影之后返回既有目录响应。
pub(super) async fn uninstall(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    plugin_id: String,
) -> Result<LuaSkillsCatalogUpdate, String> {
    // Blocking capture retains the slot before it can return or suffer a join failure.
    // 阻塞捕获在返回或遭遇联接失败之前保留槽。
    let captured_state = Arc::clone(state);
    // No resource mutex is held by this asynchronous caller during blocking capture.
    // 此异步调用者在阻塞捕获期间不持有资源互斥锁。
    let original =
        run_extension_management_blocking("reserve original plugin removal", move || {
            owner::reserve(&captured_state, &plugin_id)
        })
        .await?;
    execute(app, state, original, false).await
}

/// Resume only actual registry membership; identities are lookup metadata and never reconstruction authority.
/// 仅继续实际注册表成员；身份为查询元数据，绝非重建权威。
/// app/state publish; plugin_id/request_id identify the owner; acknowledge authorizes continuing after a retired failure.
/// app／state 负责发布；plugin_id／request_id 标识所有者；acknowledge 授权在已退役失败之后继续。
/// Returns the normal catalog projection or a retained-original error.
/// 返回普通目录投影，或保留原实例错误。
pub(super) async fn resume(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    plugin_id: &str,
    request_id: &str,
    acknowledge: bool,
) -> Result<LuaSkillsCatalogUpdate, String> {
    // Parse exactly one supplied UUID; there is no path or disk recovery fallback.
    // 精确解析单个提供 UUID；不存在路径或磁盘恢复兜底。
    let request = uuid::Uuid::parse_str(request_id).map_err(|error| error.to_string())?;
    // Service exclusion coordinates lookup with publication and actual retirement.
    // 服务互斥协调查询、发布及实际退役。
    let original = {
        // No resource borrow is acquired while the service mutex is held.
        // 持有服务互斥锁时不取得资源借用。
        let service = state.lock().map_err(|error| error.to_string())?;
        crate::service_state_access::require_plugin_publication_shutdown_admission(&service)?;
        service
            .pending_local_plugin_uninstalls
            .lookup(plugin_id, request)?
    };
    execute(app, state, original, acknowledge).await
}

/// Complete actual retained work; cancellation never removes the map or moves its original resources out.
/// 完成实际保留工作；取消绝不删除映射或移出其原资源。
/// app/state/original identify real participants; acknowledge is effective only after known callback retirement.
/// app／state／original 标识真实参与者；acknowledge 仅在已知回调退役后生效。
/// Returns the catalog response after projection and physically releasing this original registry slot.
/// 投影并实际释放此原注册表槽后返回目录响应。
async fn execute(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    original: Arc<owner::PendingLocalPluginUninstall>,
    acknowledge: bool,
) -> Result<LuaSkillsCatalogUpdate, String> {
    {
        // One exclusive borrow protects callback creation, retirement and their adjacent durable-in-process evidence.
        // 单个排他借用保护回调创建、退役及其相邻进程内保留证据。
        let mut resources = original.resources.lock().await;
        owner::require_current(state, &original)?;
        execution::retire_callback(&mut resources).await?;
        if resources
            .callback_result
            .as_ref()
            .is_some_and(Result::is_err)
            && !resources.effects_acknowledged
        {
            if !acknowledge {
                return Err(format!(
                    "原卸载请求 `{}` 的回调已经退役但业务失败；请明确确认外部副作用后继续相同请求。",
                    original.request_id
                ));
            }
            resources.effects_acknowledged = true;
        }
        execution::drain(&original.plugin_id, &mut resources).await?;
        publication::remove_policy(state, &original, &mut resources).await?;
    }
    // Blocking work owns an Arc and acquires resources itself, never waits behind our own asynchronous guard.
    // 阻塞工作持有引用并自行取得资源，绝不等待本调用者持有的异步守卫。
    let blocking_owner = Arc::clone(&original);
    // Full publication requires the same service allocation throughout blocking hidden-source work.
    // 阻塞隐藏来源工作期间，完整发布要求相同服务分配。
    let blocking_state = Arc::clone(state);
    run_extension_management_blocking("publish and collect original plugin removal", move || {
        // The service-owned strong slot remains even if this blocking executor panics.
        // 即使此阻塞执行器 panic，服务自有强槽仍保留。
        let mut resources = blocking_owner.resources.blocking_lock();
        owner::require_current(&blocking_state, &blocking_owner)?;
        publication::hide_and_publish(&blocking_state, &blocking_owner, &mut resources)?;
        collection::collect(&mut resources)
    })
    .await?;
    // Confirmation and physical take occur under service exclusion while the executor Arc may remain alive.
    // 确认及物理取出在服务互斥内发生，同时执行器引用可以仍存活。
    let mut resources = original.resources.lock().await;
    publication::confirm_current(state, &resources)?;
    if !resources.collection_confirmed {
        return Err(String::from("原卸载尚未确认实际回收。"));
    }
    if resources.projection.is_none() {
        // A temporary typed borrow carries actual original evidence into the existing shared projector.
        // 临时类型明确借用将实际原证据带入既有共享投影器。
        let mut projection = LocalPluginUninstallProjection {
            original: &original,
            resources: &mut resources,
        };
        crate::settings_runtime_projection::refresh_plugin_catalog_local_state_for_uninstall(
            state,
            &mut projection,
        )
        .await?;
    }
    // Client callback panic retains this same real update and revision for later delivery without another projection.
    // 客户端回调 panic 保留相同真实更新及修订，供稍后投递而不再次投影。
    let projection = resources
        .projection
        .as_ref()
        .ok_or("原卸载真实投影回执缺失。")?;
    emit_plugin_catalog_delta(app, projection);
    // Response and broadcast are derived from the exact same retained neighboring projection receipt.
    // 响应及广播从精确相同保留相邻投影回执派生。
    let update = projection.lua_skills_update();
    // Lock publication before the registry, and drop original files before releasing coordination.
    // 在注册表之前锁定发布，并在释放协调之前释放原文件。
    let service = state.lock().map_err(|error| error.to_string())?;
    service
        .pending_local_plugin_uninstalls
        .retire(&original, &mut resources)?;
    Ok(update)
}
