//! Explicit prepublication abort restores the same retained old owner without replaying prior work.
//! 明确发布前中止恢复相同保留旧所有者，不重放先前工作。

use super::*;
use vulcan_agent_runtime::mcp::{
    PluginMcpSecretStore, PredecessorPluginMcpRuntime, PreparedPredecessorPluginMcpConfigs,
    predecessor_global_mcp_server_configs,
};
use vulcan_plugin_runtime::{
    VulcanPluginActivationEffect as Effect, VulcanPluginEffectObservation as Observation,
    VulcanPluginPredecessorRestoration, VulcanPluginPredecessorRestorationSource,
    VulcanPluginPreparedMcpPredecessor,
};

#[cfg(test)]
mod tests;

/// One actual restoration stays inside the original service slot across every fallible checkpoint.
/// 单个实际恢复在每个可失败检查点期间始终位于原服务槽内。
pub(super) struct RetainedPredecessorRestoration {
    /// The consumed actual drain retains exact old package, configuration and credential ownership.
    /// 已消费实际排空保留精确旧包、配置及凭据所有权。
    restoration: VulcanPluginPredecessorRestoration,
    /// This memory acknowledgement follows the actual Pending writer and prevents replay after an uncertain abort.
    /// 此内存确认跟随实际 Pending 写入器，防止不确定中止后的重放。
    private_owner_prepared: bool,
    /// Only the unique original source may initialize old Hook or native resources.
    /// 仅唯一原来源可以初始化旧 Hook 或原生资源。
    source: Option<Arc<VulcanPluginPredecessorRestorationSource>>,
    /// The first physical preparation survives fallible secret reads until actual native ownership is retained.
    /// 首次物理准备跨可失败秘密读取存续，直至实际原生所有权被保留。
    prepared_mcp: Option<VulcanPluginPreparedMcpPredecessor>,
    /// The same actual native preparation is retained before its first readiness await.
    /// 相同实际原生准备在首次就绪 await 之前保留。
    native: Option<PredecessorPluginMcpRuntime>,
    /// Genuine service Full and core seal survive uncertain Transferred writes together.
    /// 真实服务 Full 与核心封闭凭据共同跨不确定 Transferred 写入存续。
    publication: Option<vulcan_plugin_lifecycle::PredecessorPluginExtensionPublication>,
    /// Only actual successful joint transfer permits cleanup or service registry retirement.
    /// 仅实际成功联合转交允许清理或服务注册表退休。
    pub(super) transferred: bool,
}

/// Restore owner under explicit abort acknowledgement and return the exact original durable status after retirement.
/// 在明确中止确认下恢复 owner，并在退休后返回精确原持久状态。
/// state/registry identify the actual service; acknowledgement authorizes compensation, never replay or external-effect erasure.
/// state／registry 标识实际服务；acknowledgement 授权补偿，绝不授权重放或外部副作用抹除。
pub(in crate::skills_services::local_plugin_update) async fn abort_enabled_update(
    state: &SharedServiceBridgeState,
    registry: &pending_owner::PendingLocalPluginUpdateRegistry,
    owner: &Arc<pending_owner::PendingLocalPluginUpdate>,
    acknowledgement: bool,
) -> Result<LocalPluginUpdateStatus, String> {
    restore_retained_predecessor(state, registry, owner, Some(acknowledgement)).await
}

/// Resume only owner's already retained real restoration, then refresh actual service catalog and emit its result.
/// 仅恢复 owner 已保留真实恢复，再刷新实际服务目录并发送结果。
/// app/state/registry are the same publisher and owner registry; absence of restoration rejects new compensation.
/// app／state／registry 为同发布器及所有者注册表；恢复缺失时拒绝新补偿。
pub(in crate::skills_services::local_plugin_update) async fn resume_predecessor_restoration(
    app: &ServiceEventEmitter,
    state: &SharedServiceBridgeState,
    registry: &pending_owner::PendingLocalPluginUpdateRegistry,
    owner: &Arc<pending_owner::PendingLocalPluginUpdate>,
) -> Result<LuaSkillsCatalogUpdate, String> {
    restore_retained_predecessor(state, registry, owner, None).await?;
    refresh_plugin_catalog_local_state_and_emit(app, state).await
}

/// Borrow actual originals through compensation, staging, initialization and joint transfer, retaining them on every error.
/// 跨补偿、暂存、初始化及联合转交借用实际原资源，每次错误均保留它们。
/// acknowledgement is present only for explicit abort; recovery without it requires an already retained restoration.
/// 仅明确中止时存在 acknowledgement；无此值的恢复要求已经保留恢复。
/// Return terminal status after actual retirement; no disk identity can recreate a missing physical owner.
/// 实际退休后返回终态状态；任何磁盘身份均不能重建缺失物理所有者。
async fn restore_retained_predecessor(
    state: &SharedServiceBridgeState,
    registry: &pending_owner::PendingLocalPluginUpdateRegistry,
    owner: &Arc<pending_owner::PendingLocalPluginUpdate>,
    acknowledgement: Option<bool>,
) -> Result<LocalPluginUpdateStatus, String> {
    // The service registry retains this slot independently from this executor's exclusive borrow.
    // 服务注册表独立于此执行器排他借用保留此槽。
    let mut resources = owner.resources.lock().await;
    pending_owner::require_current_manager(state, &resources)?;
    if resources.target.is_none() {
        return Err(String::from("原启用更新缺少实际前驱排空所有者。"));
    }
    if acknowledgement.is_none()
        && !resources
            .target
            .as_ref()
            .expect("retained enabled original")
            .has_restoration()
    {
        return Err(String::from("原请求缺少实际恢复槽；恢复不能新执行补偿。"));
    }
    // A successful transfer may only validate its real Full and continue cleanup; core completion is not replayable.
    // 成功转交仅可校验真实 Full 并继续清理；核心完成不可重放。
    let transferred = resources
        .target
        .as_ref()
        .expect("retained enabled original")
        .restoration
        .as_ref()
        .is_some_and(|restoration| restoration.transferred);
    if !transferred {
        // Preserve original plugin memberships under the existing serialized writer through old restoration and publication.
        // 在既有串行写入器下跨旧恢复及发布保留原插件成员关系。
        let policy = crate::service_state_access::begin_provider_configuration_transaction().await;
        // Current actual configuration must still equal the original baseline; never infer who changed an override.
        // 当前实际配置必须仍等于原基线；绝不推断谁改变了覆盖。
        let configuration = provider_configuration_from_state(state)?;
        require_original_enabled_policy(
            resources.prepared()?.transaction.record(),
            &configuration.luaskills,
        )?;
        if !resources
            .target
            .as_ref()
            .expect("retained enabled original")
            .has_restoration()
        {
            if acknowledgement != Some(true) {
                return Err(String::from(
                    "启用更新已有实际副作用；恢复前必须明确确认补偿不等于撤销外部副作用。",
                ));
            }
            prepare_original_restoration(&mut resources, owner).await?;
        }
        // Split only concrete fields; the actual transaction and non-cloneable restoration stay in the strong service slot.
        // 仅分别借用具体字段；实际事务及不可克隆恢复保持在强服务槽中。
        let runtime = resources.runtime.clone();
        {
            // Separate original transaction and restoration borrows across the existing synchronous core writers.
            // 跨既有同步核心写入器分别借用原事务及恢复。
            let pending_owner::PendingLocalPluginUpdateResources { stage, target, .. } =
                &mut *resources;
            // A configured owner must remain the same throughout every restoration checkpoint.
            // 每个恢复检查点期间已配置所有者必须保持相同。
            let pending_owner::PendingLocalPluginUpdateStage::Prepared(prepared) = stage else {
                return Err(String::from("原前驱恢复的准备类型已经变化。"));
            };
            // This is the actual retained restoration, not a credential reconstructed from Aborted bytes.
            // 此为实际保留恢复，而非从 Aborted 字节重建的凭据。
            let restoration = target
                .as_mut()
                .expect("retained enabled original")
                .restoration
                .as_mut()
                .expect("actual predecessor restoration is retained");
            if restoration.restoration.staged_catalog().is_none() {
                if !restoration.private_owner_prepared {
                    runtime
                        .prepare_activation_predecessor_private_owner(
                            &mut prepared.transaction,
                            &mut restoration.restoration,
                            configuration
                                .luaskills
                                .enabled_vulcan_plugins
                                .contains(&owner.plugin_id),
                            configuration
                                .luaskills
                                .disabled_vulcan_plugins
                                .contains(&owner.plugin_id),
                        )
                        .map_err(|error| error.to_string())?;
                    restoration.private_owner_prepared = true;
                }
                // Unknown abort must retry this same primitive before generic reconciliation changes its local original phase.
                // 未知中止必须先重试同一原语，再由通用对账改变其局部原阶段。
                runtime
                    .abort_and_stage_activation_predecessor_restoration(
                        &mut prepared.transaction,
                        &mut restoration.restoration,
                        configuration
                            .luaskills
                            .enabled_vulcan_plugins
                            .contains(&owner.plugin_id),
                        configuration
                            .luaskills
                            .disabled_vulcan_plugins
                            .contains(&owner.plugin_id),
                    )
                    .map_err(|error| error.to_string())?;
            }
            if restoration.source.is_none() {
                restoration.source = Some(
                    runtime
                        .prepare_activation_predecessor_source(
                            &prepared.transaction,
                            &mut restoration.restoration,
                        )
                        .map_err(|error| error.to_string())?,
                );
            }
        }
        // A prior successful Full permits only its same sealed completion, never another initialization or source eligibility check.
        // 先前成功 Full 仅允许相同封闭完成，绝不允许另一初始化或来源资格检查。
        let unpublished = resources
            .target
            .as_ref()
            .expect("retained enabled original")
            .restoration
            .as_ref()
            .expect("actual predecessor restoration is retained")
            .publication
            .is_none();
        if unpublished {
            // Clone the same source allocation without opening another File or issuing another recovery authority.
            // 克隆同一来源分配，不打开另一文件或签发另一恢复权威。
            let source = Arc::clone(
                resources
                    .target
                    .as_ref()
                    .expect("retained enabled original")
                    .restoration
                    .as_ref()
                    .expect("actual predecessor restoration is retained")
                    .source
                    .as_ref()
                    .expect("actual predecessor source is retained"),
            );
            runtime
                .prewarm_activation_predecessor_hook(&source)
                .await
                .map_err(|error| error.to_string())?;
            // Exact retained old declarations decide whether a real global native participant is required.
            // 精确保留旧声明决定是否要求真实全局原生参与者。
            let has_global = resources
                .target
                .as_ref()
                .expect("retained enabled original")
                .restoration
                .as_ref()
                .expect("actual predecessor restoration is retained")
                .restoration
                .staged_catalog()
                .ok_or("原前驱关闭目录缺失。")?
                .plugins
                .get(&owner.plugin_id)
                .ok_or("原前驱关闭描述符缺失。")?
                .mcp_servers
                .iter()
                .any(|server| {
                    server.manifest.scope
                        == vulcan_plugin_runtime::VulcanPluginMcpScopeManifest::Global
                });
            if has_global {
                if resources
                    .target
                    .as_ref()
                    .expect("retained enabled original")
                    .restoration
                    .as_ref()
                    .expect("actual predecessor restoration is retained")
                    .native
                    .is_none()
                {
                    // Release only the asynchronous borrow; the actual source and File remain in the service-owned strong slot.
                    // 仅释放异步借用；实际来源及文件保持在服务所有强槽中。
                    drop(resources);
                    // The blocking materializer borrows that same original rather than retaining detached latest configuration.
                    // 阻塞物化器借用同一原请求，不保留脱离所有者的最新配置。
                    let preparing = Arc::clone(owner);
                    // Return sealed actual old inputs together with their physical resource owner.
                    // 将封闭实际旧输入与其物理资源所有者共同返回。
                    let inputs = run_extension_management_blocking(
                        "prepare retained predecessor MCP",
                        move || {
                            // Keep the actual slot owned through ordinary errors and blocking executor failure.
                            // 跨普通错误及阻塞执行器故障保持实际槽所有权。
                            let mut resources = preparing.resources.blocking_lock();
                            materialize_retained_predecessor_mcp(
                                &mut resources,
                                &OsPluginMcpSecretStore,
                            )
                        },
                    )
                    .await?;
                    resources = owner.resources.lock().await;
                    pending_owner::require_current_manager(state, &resources)?;
                    // Real dispatch and retention are synchronous and precede the first readiness await.
                    // 真实派发及保留同步完成，并先于首次就绪 await。
                    let native = PredecessorPluginMcpRuntime::start(
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
                    // The actual transport now owns this same physical preparation before the service clears its extra pin.
                    // 服务清除额外固定句柄之前，实际传输已拥有此相同物理准备。
                    let restoration = resources
                        .target
                        .as_mut()
                        .expect("retained enabled original")
                        .restoration
                        .as_mut()
                        .expect("actual predecessor restoration is retained");
                    restoration.native = Some(native);
                    restoration.prepared_mcp = None;
                }
                resources
                    .target
                    .as_ref()
                    .expect("retained enabled original")
                    .restoration
                    .as_ref()
                    .expect("actual predecessor restoration is retained")
                    .native
                    .as_ref()
                    .expect("actual predecessor native is retained")
                    .wait_until_ready_default()
                    .await?;
            }
        }
        {
            // Borrow original transaction and actual restored participants separately for one synchronous real Full publisher.
            // 为单次同步真实 Full 发布器分别借用原事务及实际恢复参与者。
            let pending_owner::PendingLocalPluginUpdateResources { stage, target, .. } =
                &mut *resources;
            // Only the already retained actual configured transaction can authorize this same-source transfer.
            // 仅已保留实际已配置事务可以授权此次同来源转交。
            let pending_owner::PendingLocalPluginUpdateStage::Prepared(prepared) = stage else {
                return Err(String::from("原前驱组合交接的准备类型已经变化。"));
            };
            // This strong restoration retains its real pair before fallible Transferred confirmation.
            // 此强恢复在可失败 Transferred 确认之前保留真实配对。
            let restoration = target
                .as_mut()
                .expect("retained enabled original")
                .restoration
                .as_mut()
                .expect("actual predecessor restoration is retained");
            owner.retain_full_publication(state)?;
            // Preserve exclusion whenever actual Full committed, including an uncertain subsequent joint transfer.
            // 实际 Full 提交后保留排他，包括后续不确定联合转交。
            let publication =
                vulcan_plugin_lifecycle::publish_activation_predecessor_plugin_extension_generation(
                    state,
                    &prepared.transaction,
                    restoration
                        .source
                        .as_ref()
                        .expect("actual predecessor source is retained"),
                    restoration
                        .restoration
                        .staged_catalog()
                        .ok_or("原前驱关闭目录缺失。")?,
                    restoration.native.as_ref(),
                    &mut restoration.publication,
                );
            if publication.is_err() && restoration.publication.is_none() {
                owner.release_unpublished_full_publication()?;
            }
            publication?;
            restoration.transferred = true;
        }
        drop(policy);
    }
    // Confirm exact actual Full before opening the original owner-drain fence, including retries after cleanup errors.
    // 开放原所有者排空屏障之前确认精确实际 Full，包括清理错误后的重试。
    let target = resources
        .target
        .as_ref()
        .expect("retained enabled original");
    target
        .restoration
        .as_ref()
        .expect("actual predecessor restoration is retained")
        .publication
        .as_ref()
        .ok_or("已转交前驱缺少实际 Full 凭据。")?
        .validate_current(state, &resources.runtime)?;
    target
        .current_mcp
        .finish_plugin_owner_drain(&owner.plugin_id)?;
    retire_candidate_control(&mut resources).await?;
    // Build the exact original terminal result while the non-cloneable transaction remains owned.
    // 不可克隆事务仍被拥有时构建精确原终态结果。
    let prepared = resources.prepared()?;
    // Durable Aborted is observable status only; actual transferred confirmation independently authorizes registry retirement.
    // 持久 Aborted 仅为可观测状态；实际转交确认独立授权注册表退休。
    let result = status(
        &prepared.store,
        &vulcan_plugin_runtime::VulcanPluginActivationRequestRecord::Activation(
            prepared.transaction.record().clone(),
        ),
    )?;
    registry.retire(owner, &resources)?;
    Ok(result)
}

/// Retire actual original work, compensate only unattempted declared steps and retain a typed old restoration.
/// 退役实际原工作，仅补偿未尝试声明步骤，并保留类型化旧恢复。
/// resources owns the Prepared request and actual drain; owner supplies immutable request identity.
/// resources 拥有 Prepared 请求及实际排空；owner 提供不可变请求身份。
/// Errors preserve the same original drain and transaction; Started or failed compensation is never replayed.
/// 错误保留同原排空及事务；Started 或失败补偿绝不重放。
async fn prepare_original_restoration(
    resources: &mut pending_owner::PendingLocalPluginUpdateResources,
    owner: &pending_owner::PendingLocalPluginUpdate,
) -> Result<(), String> {
    // Candidate inspection permits only an unchanged actual Prepared original, never guessed Committing or Aborted ownership.
    // 候选检查仅允许未变实际 Prepared 原请求，绝不允许猜测 Committing 或 Aborted 所有权。
    let prepared = resources.prepared()?;
    if prepared.transaction.record().phase() != VulcanPluginActivationTransactionPhase::Prepared
        || prepared
            .store
            .activation(&owner.plugin_id)
            .map_err(|error| error.to_string())?
            .as_ref()
            != prepared.transaction.record().predecessor()
    {
        return Err(String::from(
            "原请求尚无可执行的 Prepared 前驱恢复权威；已选目标须继续目标恢复。",
        ));
    }
    // The actual old observation lineage includes all original transports and unfinished preparations.
    // 实际旧观察谱系包含全部原传输及未完成准备。
    let target = resources.target.as_ref().ok_or("原启用更新槽缺失。")?;
    if target.predecessor_drain.is_none()
        || target.staged.is_some()
        || target.native.is_some()
        || target.publication.is_some()
        || target.completed
    {
        return Err(String::from(
            "原请求缺少实际旧排空或已经开始目标发布，不能重建前驱恢复。",
        ));
    }
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
    if let Some(candidate) = resources.candidate.as_ref() {
        candidate.shutdown();
        candidate
            .observer()
            .wait_until_released(AgentRuntimeBridge::RETIRED_MCP_PARTITION_DRAIN_TIMEOUT)
            .await?;
    }
    // Clone only the original manager; all physical transaction and drain values remain in their actual strong slot.
    // 仅克隆原管理器；全部物理事务及排空值保持在实际强槽中。
    let runtime = resources.runtime.clone();
    // Reconcile only this actual live drain's existing supervisor, without creating a second durable attempt.
    // 仅对账此实际存活排空的既有监督器，不创建第二次持久尝试。
    match effect_observation(resources.prepared()?, Effect::DrainPredecessorLua)? {
        Some(Observation::Succeeded) => {}
        Some(Observation::Started) => {
            // The opaque actual drain, not its durable UUID, authorizes observing original physical cleanup.
            // 不透明实际排空授权观察原物理清理，而非其持久 UUID。
            let drain = resources
                .target
                .as_ref()
                .expect("retained enabled original")
                .predecessor_drain
                .as_ref()
                .expect("actual predecessor drain is retained");
            // Existing finalizers and the retained original effect observer are supervised by the same core manager.
            // 既有关闭函数及保留原副作用观察者由同核心管理器监督。
            let report = runtime
                .drain_activation_predecessor(drain)
                .await
                .map_err(|error| error.to_string())?;
            require_retired(report, "原前驱 Lua")?;
        }
        _ => {
            return Err(String::from(
                "原前驱排空未确认成功；保留实际所有者，不能重放失败排空。",
            ));
        }
    }
    require_retired(
        runtime
            .drain_activation_candidate_calls(owner.request_id)
            .await
            .map_err(|error| error.to_string())?,
        "原候选调用",
    )?;
    require_retired(
        runtime
            .drain_activation_predecessor_calls(owner.request_id)
            .await
            .map_err(|error| error.to_string())?,
        "原前驱调用",
    )?;
    // Read fresh original effects only after actual retirement; uncertain work cannot become an authorization to replay.
    // 仅实际退役后读取新鲜原副作用；不确定工作不能成为重放授权。
    let effects = resources
        .prepared()?
        .store
        .activation_effects(&owner.plugin_id, owner.request_id)
        .map_err(|error| error.to_string())?
        .ok_or("原前驱排空回执缺失。")?;
    if effects
        .observations()
        .iter()
        .any(|(_, observation)| *observation == Observation::Started)
        || resources
            .prepared()?
            .store
            .installation_receipt(&owner.plugin_id, owner.request_id)
            .map_err(|error| error.to_string())?
            .is_some_and(|receipt| receipt.phase() == Observation::Started)
    {
        return Err(String::from(
            "原副作用仍为未确认 Started；请核对原请求，不能重放补偿或旧恢复。",
        ));
    }
    if effect_observation(resources.prepared()?, Effect::Enable)?.is_some() {
        match effect_observation(resources.prepared()?, Effect::EnableRollback)? {
            Some(Observation::Succeeded) => {}
            None => {
                // This exact actual inspection owns readiness only through its one unattempted compensation.
                // 此精确实际检查仅跨单次未尝试补偿拥有就绪权。
                let candidate = resources
                    .prepared()?
                    .store
                    .inspect_activation_candidate(&resources.prepared()?.transaction)
                    .map_err(|error| error.to_string())?;
                runtime
                    .rollback_activation_candidate_enable(&candidate)
                    .await
                    .map_err(|error| error.to_string())?;
            }
            _ => return Err(String::from("原候选补偿未确认成功；不能重复执行。")),
        }
        require_retired(
            runtime
                .drain_activation_candidate_calls(owner.request_id)
                .await
                .map_err(|error| error.to_string())?,
            "原候选补偿",
        )?;
    }
    match effect_observation(resources.prepared()?, Effect::RestorePredecessor)? {
        Some(Observation::Succeeded) => {}
        None => {
            // The actual drain fixes the old declaration; absent callbacks remain a core no-op without a fabricated receipt.
            // 实际排空固定旧声明；缺失回调保持核心无操作，不伪造回执。
            let drain = resources
                .target
                .as_ref()
                .expect("retained enabled original")
                .predecessor_drain
                .as_ref()
                .expect("actual predecessor drain is retained");
            // Candidate borrows the same unchanged actual transaction, independently of public discovery.
            // 候选借用同一未变实际事务，独立于公开发现。
            let candidate = resources
                .prepared()?
                .store
                .inspect_activation_candidate(&resources.prepared()?.transaction)
                .map_err(|error| error.to_string())?;
            runtime
                .restore_activation_predecessor(&candidate, drain)
                .await
                .map_err(|error| error.to_string())?;
        }
        _ => return Err(String::from("原前驱恢复回调未确认成功；不能重复执行。")),
    }
    require_retired(
        runtime
            .drain_activation_predecessor_calls(owner.request_id)
            .await
            .map_err(|error| error.to_string())?,
        "原前驱恢复",
    )?;
    // Split the configured transaction borrow from its actual drain, which the factory returns on ordinary failure.
    // 将已配置事务借用与实际排空分开；工厂在普通失败时返回该排空。
    let pending_owner::PendingLocalPluginUpdateResources { stage, target, .. } = resources;
    // No record or file owner is moved out of the original strong service slot.
    // 不将任何记录或文件所有者移出原强服务槽。
    let pending_owner::PendingLocalPluginUpdateStage::Prepared(prepared) = stage else {
        return Err(String::from("原前驱恢复准备类型已经变化。"));
    };
    // This temporary inspection must end before any mutable transaction writer can run.
    // 任一可变事务写入器运行之前，此临时检查必须结束。
    let candidate = prepared
        .store
        .inspect_activation_candidate(&prepared.transaction)
        .map_err(|error| error.to_string())?;
    // The actual target slot is still owned during synchronous consuming preparation.
    // 同步消费准备期间，实际目标槽仍被拥有。
    let target = target.as_mut().expect("retained enabled original");
    // The factory consumes this exact physical drain and returns it on every ordinary error; there is no cancellable await here.
    // 工厂消费此精确物理排空，每次普通错误均返回它；此处没有可取消 await。
    let drain = target
        .predecessor_drain
        .take()
        .ok_or("原实际前驱排空缺失。")?;
    match runtime.prepare_activation_predecessor_restoration(&candidate, drain) {
        Ok(restoration) => {
            target.restoration = Some(RetainedPredecessorRestoration {
                restoration,
                private_owner_prepared: false,
                source: None,
                prepared_mcp: None,
                native: None,
                publication: None,
                transferred: false,
            });
            Ok(())
        }
        Err(failure) => {
            target.predecessor_drain = Some(failure.drain);
            Err(failure.error.to_string())
        }
    }
}

/// Materialize exact old inputs while retaining the first physical preparation before every fallible secret read.
/// 在每次可失败秘密读取之前保留首次物理准备，同时物化精确旧输入。
/// resources owns the actual source and first issuer; secrets reads only that source's frozen credential revision.
/// resources 拥有实际来源及首次签发者；secrets 仅读取该来源的固定凭据修订。
/// Return sealed inputs sharing the same preparation, or an error that leaves that preparation available for retry.
/// 返回共享相同准备的密封输入，或让该准备保持可重试的错误。
fn materialize_retained_predecessor_mcp(
    resources: &mut pending_owner::PendingLocalPluginUpdateResources,
    secrets: &dyn PluginMcpSecretStore,
) -> Result<PreparedPredecessorPluginMcpConfigs, String> {
    // Borrow distinct concrete fields so the original physical owner is stored without a detached latest configuration.
    // 分别借用具体字段，使原物理所有者被保存而不使用脱离来源的最新配置。
    let pending_owner::PendingLocalPluginUpdateResources {
        runtime,
        authority,
        target,
        ..
    } = resources;
    // The configured source was already retained by the actual restoration factory before this blocking materializer.
    // 实际恢复工厂已在此阻塞物化器之前保留已配置来源。
    let restoration = target
        .as_mut()
        .ok_or("原启用更新槽缺失。")?
        .restoration
        .as_mut()
        .ok_or("实际前驱恢复缺失。")?;
    if restoration.prepared_mcp.is_none() {
        // Store the sole strong physical preparation before conversion; the core source retains only its weak control link.
        // 转换之前保存唯一强物理准备；核心来源仅保留其弱控制链接。
        let source = restoration.source.as_ref().ok_or("实际前驱来源缺失。")?;
        restoration.prepared_mcp = Some(
            runtime
                .prepare_predecessor_mcp_resources(source, authority)
                .map_err(|error| error.to_string())?,
        );
    }
    predecessor_global_mcp_server_configs(
        restoration
            .prepared_mcp
            .as_ref()
            .expect("actual predecessor physical preparation is retained")
            .clone(),
        secrets,
    )
}

/// Read only operation's exact original effect from prepared's configured store; absence does not grant execution itself.
/// 仅从 prepared 已配置存储读取 operation 精确原副作用；缺失自身不授予执行。
/// Return the durable observation or explicit I/O/identity failure, without mutating any receipt.
/// 返回持久观测或明确 I/O／身份失败，不修改任何回执。
fn effect_observation(
    prepared: &PreparedLocalPluginUpdate,
    operation: Effect,
) -> Result<Option<Observation>, String> {
    // All paths derive from the same actual immutable original transaction.
    // 全部路径从同一实际不可变原事务派生。
    let record = prepared.transaction.record();
    Ok(prepared
        .store
        .activation_effects(&record.target().plugin_id, record.request_id())
        .map_err(|error| error.to_string())?
        .and_then(|effects| {
            effects
                .observations()
                .into_iter()
                .find_map(|(effect, observation)| (effect == operation).then_some(observation))
        }))
}

/// Require report's real cleanup success before context may advance; label identifies retained resource failures.
/// 要求 report 真实清理成功后才推进上下文；label 标识保留资源故障。
/// Return a combined explicit error without discarding any actual owner.
/// 返回组合明确错误，不丢弃任何实际所有者。
fn require_retired(
    report: vulcan_plugin_runtime::VulcanPluginDrainReport,
    label: &str,
) -> Result<(), String> {
    if report.failures.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "{label}资源尚未退役：{}",
            report.failures.join("；")
        ))
    }
}
