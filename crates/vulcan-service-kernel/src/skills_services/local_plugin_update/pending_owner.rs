//! Service state retains actual original update resources independently from executor and RPC lifetimes.
//! 服务状态保留实际原更新资源，独立于执行器及 RPC 寿命。

use super::*;

mod preparation;
pub(super) use preparation::{
    ApprovedLocalPluginUpdateSource, PendingLocalPluginUpdateStage, ReservedLocalPluginUpdate,
    abort_preparation, list_owned, materialize_preparation, recover_preparation,
};
#[cfg(test)]
use preparation::{
    abort_preparation_with_backend, publish_reserved, recover_preparation_with_backend,
};

#[cfg(test)]
mod early_tests;
#[cfg(test)]
mod tests;

/// Strong original request slots have no expiration and are removed only after confirmed terminal retirement.
/// 强原请求槽没有过期时间，仅在确认终态退役后移除。
#[derive(Default)]
pub(crate) struct PendingLocalPluginUpdateRegistry {
    /// The map owns each physical transaction even when every executor has ended abnormally.
    /// 即使全部执行器异常结束，映射仍拥有每个物理事务。
    owners: Mutex<BTreeMap<uuid::Uuid, Arc<PendingLocalPluginUpdate>>>,
}

/// Immutable lookup identity encloses one asynchronous exclusive borrow of the actual original owner.
/// 不可变查询身份封装对实际原所有者的单个异步排他借用。
pub(super) struct PendingLocalPluginUpdate {
    /// The original durable UUID is also the sole registry key.
    /// 原持久 UUID 同时为唯一注册表键。
    pub(super) request_id: uuid::Uuid,
    /// Namespace identity comes from the original prepared transaction, never public rediscovery.
    /// 命名空间身份来自原准备事务，绝不来自公开重新发现。
    pub(super) plugin_id: String,
    /// Publication exclusion preserves the actual Full witness until this original registry slot retires.
    /// 发布排他保留实际 Full 见证，直至此原注册表槽退役。
    full_publication_retained: std::sync::atomic::AtomicBool,
    /// The actual Core routing reservation survives executor loss until this exact registry slot retires.
    /// 实际核心路由预约跨执行器丢失存续，直至此精确注册表槽退役。
    routing_publication: Mutex<Option<vulcan_plugin_runtime::VulcanPluginRoutingReservation>>,
    /// Borrowing this slot never transfers its non-cloneable transaction to an executor.
    /// 借用此槽绝不将其不可克隆事务转交执行器。
    pub(super) resources: tokio::sync::Mutex<PendingLocalPluginUpdateResources>,
}

/// Typed original resources remain physically owned by the service registry throughout uncertainty.
/// 类型明确的原资源在不确定结果期间始终由服务注册表实际拥有。
pub(super) struct PendingLocalPluginUpdateResources {
    /// The same manager handles every original callback and route recovery.
    /// 相同管理器处理全部原回调及路由恢复。
    pub(super) runtime: VulcanPluginRuntimeManager,
    /// The first registered native issuer fixes manager identity and is retained for subsequent target handoff.
    /// 首次注册原生签发者固定管理器身份，并保留用于后续目标交接。
    pub(super) authority: vulcan_plugin_runtime::VulcanPluginRegisteredNativeMcpHostAuthority,
    /// The concrete original file owner advances by borrowed transfer, without an empty ownership interval.
    /// 具体原文件所有者通过借用移交推进，不存在空所有权间隙。
    pub(super) stage: PendingLocalPluginUpdateStage,
    /// Explicit source and supplied values are retained before the first original journal is written.
    /// 明确来源及提交值在首份原日志写入之前保留。
    pub(super) source: Option<ApprovedLocalPluginUpdateSource>,
    /// Complete configuration is frozen before credential inventory publication and never rediscovered after it.
    /// 完整配置在凭据清单发布之前固定，此后绝不重新发现。
    pub(super) configuration: Option<vulcan_plugin_runtime::VulcanPluginConfigurationSelection>,
    /// Original complete credential values survive backend errors and never derive from later selected-secret reads.
    /// 原完整凭据值跨后端错误存续，绝不从随后所选秘密读取派生。
    pub(super) values: Option<VulcanPluginCredentialValues>,
    /// Actual non-secret inventory survives writer retirement and controls exact terminal cleanup.
    /// 实际非秘密清单跨写入者退役存续，并控制精确终态清理。
    pub(super) inventory: Option<vulcan_plugin_runtime::VulcanPluginCredentialRecord>,
    /// Real readiness control is retained before its first cancellable wait.
    /// 真实就绪控制在首次可取消等待之前保留。
    pub(super) candidate: Option<CandidatePluginMcpRuntime>,
    /// The enabled original retains its real drain, target controller and sealed Full publication together.
    /// 启用原请求同时保留真实排空、目标控制对象及密封 Full 发布。
    pub(super) target: Option<super::target_handoff::RetainedEnabledPluginUpdate>,
}

impl PendingLocalPluginUpdateResources {
    /// Borrow the actual configured owner; early owners cannot grant candidate execution.
    /// 借用实际已配置所有者；早期所有者不能授予候选执行。
    /// Returns the retained Prepared value or an explicit unfinished-preparation error.
    /// 返回保留的 Prepared 值，或明确未完成准备错误。
    pub(super) fn prepared(&self) -> Result<&PreparedLocalPluginUpdate, String> {
        match &self.stage {
            PendingLocalPluginUpdateStage::Prepared(prepared) => Ok(prepared),
            _ => Err(String::from(
                "The original update request has not finished the actual preparation yet.",
            )),
        }
    }

    /// Exclusively borrow the actual configured transaction without moving it out of its strong slot.
    /// 排他借用实际已配置事务，不将其移出强槽。
    /// Returns the same mutable Prepared owner or rejects an early resource type.
    /// 返回相同可变 Prepared 所有者，或拒绝早期资源类型。
    pub(super) fn prepared_mut(&mut self) -> Result<&mut PreparedLocalPluginUpdate, String> {
        match &mut self.stage {
            PendingLocalPluginUpdateStage::Prepared(prepared) => Ok(prepared),
            _ => Err(String::from(
                "The original update request has not finished the actual preparation yet.",
            )),
        }
    }
}

impl PendingLocalPluginUpdateRegistry {
    /// Insert prepared and runtime as the sole original owner; reject duplicate UUIDs without replacement.
    /// 将 prepared 及 runtime 插入为唯一原所有者；拒绝重复 UUID，不进行替换。
    /// Returns the retained slot before the blocking preparation worker can return or panic.
    /// 在阻塞准备工作线程可以返回或 panic 前，返回保留槽。
    #[cfg(test)]
    pub(super) fn retain(
        &self,
        runtime: VulcanPluginRuntimeManager,
        prepared: PreparedLocalPluginUpdate,
    ) -> Result<Arc<PendingLocalPluginUpdate>, String> {
        // Both lookup fields are derived from the already validated original transaction.
        // 两个查询字段均从已经校验的原事务派生。
        let request_id = prepared.transaction.record().request_id();
        // Namespace is independent from a subsequent public catalog refresh.
        // 命名空间独立于后续公开目录刷新。
        let plugin_id = prepared.transaction.record().target().plugin_id.clone();
        // Acquire the registry before allocating the unique physical owner slot.
        // 分配唯一物理所有者槽之前取得注册表。
        let mut owners = self.owners.lock().map_err(|error| error.to_string())?;
        if owners.contains_key(&request_id)
            || owners.values().any(|owner| owner.plugin_id == plugin_id)
        {
            return Err(String::from(
                "插件原更新请求已有服务所有者，不能替换其实际资源。",
            ));
        }
        // A slot contains concrete resources rather than a parallel phase or a type-erased box.
        // 槽包含具体资源，而非并行阶段或类型擦除盒子。
        let owner = Arc::new(PendingLocalPluginUpdate {
            request_id,
            plugin_id,
            full_publication_retained: std::sync::atomic::AtomicBool::new(false),
            routing_publication: Mutex::new(None),
            resources: tokio::sync::Mutex::new(PendingLocalPluginUpdateResources {
                authority: runtime.register_native_mcp_host_adapter(),
                runtime,
                stage: PendingLocalPluginUpdateStage::Prepared(prepared),
                source: None,
                configuration: None,
                values: None,
                inventory: None,
                candidate: None,
                target: None,
            }),
        });
        owners.insert(request_id, Arc::clone(&owner));
        Ok(owner)
    }

    /// Reject competing Full or core catalog replacement while an original publication witness remains retained.
    /// 原发布见证仍被保留时，拒绝竞争 Full 或核心目录替换。
    /// original_request exempts only that internal request; no file or execution authority is granted by this check.
    /// original_request 仅豁免该内部请求；此检查不授予文件或执行权威。
    /// Inspect only the short map and atomic coordination fact, never the asynchronously borrowed resources.
    /// 仅检查短暂映射及原子协调事实，绝不读取异步借用资源。
    pub(crate) fn require_full_publication(
        &self,
        original_request: Option<uuid::Uuid>,
    ) -> Result<(), String> {
        // Resource borrowers may already hold the service lock, so this check must not acquire their mutex.
        // 资源借用者可能已持有服务锁，因此此检查不得获取其互斥锁。
        let owners = self.owners.lock().map_err(|error| error.to_string())?;
        if let Some(owner) = owners.values().find(|owner| {
            Some(owner.request_id) != original_request
                && owner
                    .full_publication_retained
                    .load(std::sync::atomic::Ordering::Acquire)
        }) {
            return Err(format!(
                "Original plugin request `{}` still holds the actual publication; continue the same request first, then update the runtime settings.",
                owner.request_id
            ));
        }
        Ok(())
    }

    /// Pin only the exact retained owner before its synchronous actual Full publication attempt.
    /// 在同步实际 Full 发布尝试之前，仅固定精确保留所有者。
    /// owner must be the registry's same Arc; caller holds the service lock shared with ordinary publishers.
    /// owner 必须为注册表的相同 Arc；调用方持有与普通发布者共享的服务锁。
    fn retain_full_publication(&self, owner: &Arc<PendingLocalPluginUpdate>) -> Result<(), String> {
        // The map protects both exact slot membership and competing publication observations.
        // 映射同时保护精确槽成员关系及竞争发布观察。
        let owners = self.owners.lock().map_err(|error| error.to_string())?;
        if !owners
            .get(&owner.request_id)
            .is_some_and(|retained| Arc::ptr_eq(retained, owner))
        {
            return Err(String::from(
                "The original publication owner no longer belongs to the actual service registry.",
            ));
        }
        if owners.values().any(|retained| {
            !Arc::ptr_eq(retained, owner)
                && retained
                    .full_publication_retained
                    .load(std::sync::atomic::Ordering::Acquire)
        }) {
            return Err(String::from(
                "Another original request still holds the actual publication; its runtime cannot be replaced.",
            ));
        }
        owner
            .full_publication_retained
            .store(true, std::sync::atomic::Ordering::Release);
        Ok(())
    }

    /// Reject a second update before preparation can replace a terminal journal index still owned by this service.
    /// 此服务仍拥有终态日志索引时，在准备能够替换它之前拒绝第二次更新。
    /// plugin_id comes from the exact inspected source; returns the original UUID for explicit recovery.
    /// plugin_id 来自精确已检查来源；返回原 UUID，用于明确恢复。
    pub(super) fn require_namespace_unowned(&self, plugin_id: &str) -> Result<(), String> {
        // A successful writer with a lost observer remains owned until explicit original confirmation and retirement.
        // 成功写入器的观察者丢失后，在明确原确认及退役之前仍保持所有权。
        let owners = self.owners.lock().map_err(|error| error.to_string())?;
        if let Some(owner) = owners.values().find(|owner| owner.plugin_id == plugin_id) {
            return Err(format!(
                "The plugin still has original update service owner `{}`; inspect and resume or abort the same request first.",
                owner.request_id
            ));
        }
        Ok(())
    }

    /// Lookup one exact canonical request without waiting for its executor or inspecting mutable discovery.
    /// 查询单个精确规范请求，不等待其执行器，也不检查可变发现。
    /// plugin_id must equal the retained original namespace; absence alone permits declared disk recovery.
    /// plugin_id 必须等于保留的原命名空间；仅缺失才允许已声明磁盘恢复。
    pub(super) fn get(
        &self,
        plugin_id: &str,
        request_id: uuid::Uuid,
    ) -> Result<Option<Arc<PendingLocalPluginUpdate>>, String> {
        // Only a short map observation is required, even while actual callbacks are blocked.
        // 即使实际回调阻塞，也仅需短暂观察映射。
        let owners = self.owners.lock().map_err(|error| error.to_string())?;
        let Some(owner) = owners.get(&request_id) else {
            return Ok(None);
        };
        if owner.plugin_id != plugin_id || owner.request_id != request_id {
            return Err(String::from(
                "The original update request does not belong to this plugin namespace.",
            ));
        }
        Ok(Some(Arc::clone(owner)))
    }

    /// Snapshot the sole plugin owner without waiting for its executor or retaining the registry lock during filesystem reads.
    /// 快照唯一插件所有者，不等待其执行器，也不在文件系统读取期间保留注册表锁。
    /// Plugin namespace is exact; returned Arc grants access to the already retained concrete slot only.
    /// 插件命名空间为精确值；返回 Arc 仅授予访问已经保留具体槽。
    pub(super) fn plugin_owner(
        &self,
        plugin_id: &str,
    ) -> Result<Option<Arc<PendingLocalPluginUpdate>>, String> {
        let owners = self.owners.lock().map_err(|error| error.to_string())?;
        Ok(owners
            .values()
            .find(|owner| owner.plugin_id == plugin_id)
            .cloned())
    }

    /// Snapshot every namespace with a live original owner, including one whose journal does not exist yet.
    /// 快照每个具有存活原所有者的命名空间，包括尚无日志的命名空间。
    pub(super) fn plugin_ids(&self) -> Result<std::collections::BTreeSet<String>, String> {
        let owners = self.owners.lock().map_err(|error| error.to_string())?;
        Ok(owners
            .values()
            .map(|owner| owner.plugin_id.clone())
            .collect())
    }

    /// Remove only this exact confirmed terminal owner after the caller has awaited actual resource retirement.
    /// 调用方等待实际资源退役后，仅移除此精确已确认终态所有者。
    /// The resource guard proves executor exclusion; failures preserve the slot unchanged.
    /// 资源守卫证明执行器排他；失败保持槽不变。
    pub(super) fn retire(
        &self,
        owner: &Arc<PendingLocalPluginUpdate>,
        resources: &PendingLocalPluginUpdateResources,
    ) -> Result<(), String> {
        // Lock the same coordination slot before validation; poison must not remove the retained original.
        // 校验前锁定相同协调槽；锁污染不得移除保留原请求。
        let mut routing_publication = owner
            .routing_publication
            .lock()
            .map_err(|error| error.to_string())?;
        // Successful local observation and canonical durable readback must describe the same complete record.
        // 成功本地观察及规范持久回读必须描述相同完整记录。
        if resources.candidate.is_some() {
            return Err(String::from(
                "The original candidate control object is not actually retired yet.",
            ));
        }
        // Durable terminal phase alone cannot retire unconfirmed target or original predecessor transfers.
        // 仅持久终态阶段不能退役未确认目标或原前驱转交。
        if resources.target.as_ref().is_some_and(|target| {
            resources.prepared().map_or(true, |prepared| {
                !target.confirms_terminal(prepared.transaction.record().phase())
            })
        }) {
            return Err(String::from(
                "The enable target or original predecessor has not confirmed the actual composition handoff; keeping the original service owner.",
            ));
        }
        match &resources.stage {
            PendingLocalPluginUpdateStage::Prepared(prepared) => {
                let record = prepared.transaction.record();
                if !matches!(
                    record.phase(),
                    vulcan_plugin_runtime::VulcanPluginActivationTransactionPhase::Completed
                        | vulcan_plugin_runtime::VulcanPluginActivationTransactionPhase::Aborted
                ) || prepared
                    .store
                    .activation_request(&owner.plugin_id, owner.request_id)
                    .map_err(|error| error.to_string())?
                    != Some(
                        vulcan_plugin_runtime::VulcanPluginActivationRequestRecord::Activation(
                            record.clone(),
                        ),
                    )
                {
                    return Err(String::from(
                        "The original update request has not confirmed the terminal state and actual resource retirement; keeping the service owner.",
                    ));
                }
            }
            PendingLocalPluginUpdateStage::Preparing(preparing) => {
                preparation::confirm_abandoned_retirement(resources.inventory.as_ref(), preparing)?;
            }
            PendingLocalPluginUpdateStage::Reserved(_) => {
                return Err(String::from(
                    "The original reservation was not published and explicitly aborted yet; keeping the service owner.",
                ));
            }
        }
        // Pointer equality prevents an executor from removing a different slot with copied identifiers.
        // 指针相等防止执行器使用复制身份移除不同槽。
        let mut owners = self.owners.lock().map_err(|error| error.to_string())?;
        if !owners
            .get(&owner.request_id)
            .is_some_and(|retained| Arc::ptr_eq(retained, owner))
        {
            return Err(String::from(
                "The original update service owner changed; other actual resources cannot be released.",
            ));
        }
        owners.remove(&owner.request_id);
        // Release the Core reservation only after confirmed removal, even when the executor still holds this Arc.
        // 仅在确认移除后释放核心预约，即使执行器仍持有此 Arc。
        drop(owners);
        routing_publication.take();
        owner
            .full_publication_retained
            .store(false, std::sync::atomic::Ordering::Release);
        Ok(())
    }

    /// Wait for original executors and retire actual readiness and Lua resources during closed shutdown admission.
    /// 在关闭准入期间等待原执行器，并退役实际就绪及 Lua 资源。
    /// Does not discard unresolved transactions or fabricate completion; returns every real drain failure.
    /// 不丢弃未决事务，也不虚构完成；返回全部真实排空失败。
    pub(crate) async fn drain_for_shutdown(&self) -> Result<(), String> {
        // This is the same existing management fence: preparation must finish retaining its owner before enumeration.
        // 此为相同既有管理屏障：准备必须完成所有者保留后才能枚举。
        let _management = lock_luaskills_management_mutation().await;
        // Clone only retained slots under the short map lock; no service or map lock crosses an await.
        // 在短暂映射锁下仅克隆保留槽；服务锁及映射锁均不跨越 await。
        let owners = self
            .owners
            .lock()
            .map_err(|error| error.to_string())?
            .values()
            .cloned()
            .collect::<Vec<_>>();
        for owner in owners {
            // An active executor must actually finish borrowing before shutdown can inspect its resources.
            // 关闭检查资源之前，活动执行器必须实际结束借用。
            let resources = owner.resources.lock().await;
            // Drain the same observation lineage without taking or dropping unresolved target controls.
            // 排空相同观察谱系，不取走或销毁未决目标控制对象。
            if let Some(target) = resources.target.as_ref() {
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
            }
            if let Some(candidate) = resources.candidate.as_ref() {
                candidate.shutdown();
                candidate
                    .observer()
                    .wait_until_released(AgentRuntimeBridge::RETIRED_MCP_PARTITION_DRAIN_TIMEOUT)
                    .await?;
                // Real release is confirmed, while unresolved startup diagnostics remain attached to this original owner.
                // 确认真实释放，同时让未决启动诊断继续附着于此原所有者。
            }
            // Lua cleanup uses the actual manager registry instead of a copied active-count diagnostic.
            // Lua 清理使用实际管理器注册表，而非复制的活动数量诊断。
            let drained = resources
                .runtime
                .drain_plugin_sessions(&owner.plugin_id)
                .await
                .map_err(|error| error.to_string())?;
            if !drained.failures.is_empty() {
                return Err(format!(
                    "The Lua resources of original update request `{}` are not retired: {}",
                    owner.request_id,
                    drained.failures.join("; ")
                ));
            }
        }
        Ok(())
    }

    /// Prove no original owner or management executor exists when no retained async runtime is available.
    /// 没有保留异步运行时时，证明不存在原所有者或管理执行器。
    /// Returns a synchronous empty proof or rejects shutdown before process resource teardown.
    /// 返回同步空证明，或在进程资源拆卸前拒绝关闭。
    pub(crate) fn require_empty_without_runtime(&self) -> Result<(), String> {
        // The same management fence prevents a preparing worker from inserting after this observation.
        // 相同管理屏障防止准备工作线程在此观察之后插入。
        let _management = LUASKILLS_MANAGEMENT_MUTATION_LOCK.try_lock().map_err(|_| String::from("Plugin update shutdown requires the retained service runtime while management work is active."))?;
        if !self
            .owners
            .lock()
            .map_err(|error| error.to_string())?
            .is_empty()
        {
            return Err(String::from(
                "Plugin update shutdown requires the retained service runtime while original owners remain.",
            ));
        }
        Ok(())
    }
}

impl PendingLocalPluginUpdate {
    /// Reserve true routing mutations before the enabled original closes its first predecessor admission.
    /// 在启用原请求关闭首次前驱入场之前，预约实际路由变更。
    /// state supplies publisher exclusion; resources borrow the exact first issuer and physical Prepared transaction.
    /// state 提供发布者排他；resources 借用精确首次签发者及实际 Prepared 事务。
    /// Returns success after strong retention; errors preserve the original resources and any earlier reservation.
    /// 强保留后返回成功；错误保留原资源及此前任何预约。
    pub(super) fn retain_routing_publication(
        self: &Arc<Self>,
        state: &SharedServiceBridgeState,
        resources: &PendingLocalPluginUpdateResources,
    ) -> Result<(), String> {
        // Service publishers cannot pass between Core reservation and the existing registry admission pin.
        // 服务发布者不得穿过核心预约与既有注册表准入固定之间。
        let service = state.lock().map_err(|error| error.to_string())?;
        crate::service_state_access::require_plugin_publication_shutdown_admission(&service)?;
        // This synchronous slot never waits for an async resource borrower or calls back into the service.
        // 此同步槽绝不等待异步资源借用者，也不回调服务。
        let mut routing_publication = self
            .routing_publication
            .lock()
            .map_err(|error| error.to_string())?;
        if routing_publication.is_none() {
            // The Core checks the actual store, physical original lease and exact manager issuer before reserving.
            // 核心在预约前校验实际存储、物理原租约及精确管理器签发者。
            let reservation = resources
                .runtime
                .reserve_activation_routing(
                    &resources.authority,
                    &resources.prepared()?.transaction,
                )
                .map_err(|error| error.to_string())?;
            service
                .pending_local_plugin_updates
                .retain_full_publication(self)?;
            *routing_publication = Some(reservation);
        } else {
            // Same-slot retry keeps the original Core object and restores no authority from copied identifiers.
            // 相同槽重试保留原核心对象，不从复制身份恢复任何权威。
            service
                .pending_local_plugin_updates
                .retain_full_publication(self)?;
        }
        Ok(())
    }

    /// Retain this actual publication exclusion under the same service mutex used by all competing publishers.
    /// 在所有竞争发布者共用的相同服务互斥锁下，保留此实际发布排他。
    /// state supplies the actual registry and shutdown authority; errors leave all physical resources owned.
    /// state 提供实际注册表及关闭权威；错误让全部物理资源保持被拥有。
    pub(super) fn retain_full_publication(
        self: &Arc<Self>,
        state: &SharedServiceBridgeState,
    ) -> Result<(), String> {
        // No async resource borrow is acquired while observing the actual service and short registry map.
        // 观察实际服务及短暂注册表映射时，不获取异步资源借用。
        let service = state.lock().map_err(|error| error.to_string())?;
        crate::service_state_access::require_plugin_publication_shutdown_admission(&service)?;
        service
            .pending_local_plugin_updates
            .retain_full_publication(self)
    }

    /// Release a publication-only pin only when neither an actual Full nor a Core routing reservation remains.
    /// 仅当实际 Full 与核心路由预约均未保留时，释放单独发布固定状态。
    /// Caller owns the original resource mutex; committed or uncertain publications must keep this pin until retirement.
    /// 调用方拥有原资源互斥锁；已提交或不确定发布必须保留固定状态直至退役。
    pub(super) fn release_unpublished_full_publication(&self) -> Result<(), String> {
        // A closed Source must remain protected even before the first successful Full publication.
        // 即使尚无首次成功 Full 发布，关闭来源也必须持续受保护。
        let routing_publication = self
            .routing_publication
            .lock()
            .map_err(|error| error.to_string())?;
        if routing_publication.is_none() {
            self.full_publication_retained
                .store(false, std::sync::atomic::Ordering::Release);
        }
        Ok(())
    }
}

/// Verify the retained first issuer still belongs to the service's exact current manager before route recovery.
/// 路由恢复之前，验证保留首次签发者仍属于服务精确当前管理器。
/// state supplies current authority; resources supply the original identity, without registering a replacement issuer.
/// state 提供当前权威；resources 提供原身份，不注册替代签发者。
pub(super) fn require_current_manager(
    state: &SharedServiceBridgeState,
    resources: &PendingLocalPluginUpdateResources,
) -> Result<(), String> {
    // A manager clone preserves state identity; equal roots or equal public catalog values cannot replace it.
    // 管理器克隆保留状态身份；相等根路径或相等公开目录值不能替代它。
    let current = service_vulcan_plugin_runtime(state)?;
    if !resources.authority.matches_manager(&current) {
        return Err(String::from(
            "The service plugin manager changed; keeping the original update owner, mixed-generation publication routes are not allowed.",
        ));
    }
    Ok(())
}

/// Clone the sole service registry after checking its authoritative lifecycle admission.
/// 检查权威生命周期准入后，克隆唯一服务注册表。
/// state owns both fields; shutdown rejects new work without changing durable request evidence.
/// state 拥有两个字段；关闭拒绝新工作，不改变持久请求证据。
pub(super) fn admitted_registry(
    state: &SharedServiceBridgeState,
) -> Result<Arc<PendingLocalPluginUpdateRegistry>, String> {
    // The same state observation couples admission to the registry selected for this operation.
    // 同一次状态观察将准入与本操作选定注册表关联。
    let service = state.lock().map_err(|error| error.to_string())?;
    crate::service_state_access::require_plugin_publication_shutdown_admission(&service)?;
    Ok(Arc::clone(&service.pending_local_plugin_updates))
}
