//! Original early file owners remain in the service slot through publication, copying and configured promotion.
//! 原早期文件所有者在发布、复制及已配置晋级期间保持于服务槽内。

use super::*;
use vulcan_plugin_runtime::{
    PinnedVulcanPluginGeneration, VulcanPluginActivationPreparation,
    VulcanPluginActivationPreparationInspection, VulcanPluginActivationPreparationReservation,
    VulcanPluginActivationRequestRecord, VulcanPluginActivationTransactionPhase as Phase,
    VulcanPluginCredentialPhase, VulcanPluginCredentialStagingReservation,
};

/// Explicit approved source inputs are retained independently from later filesystem or secret observations.
/// 明确批准来源输入独立于随后文件系统或秘密观察而保留。
pub(in crate::skills_services::local_plugin_update) struct ApprovedLocalPluginUpdateSource {
    /// Original requested package path is used only by the initial copy, never recovery copying.
    /// 原请求插件包路径仅用于首次复制，绝不用于恢复复制。
    pub(in crate::skills_services::local_plugin_update) path: PathBuf,
    /// Exact validated local secret inputs are not serialized or printed.
    /// 精确已校验本地秘密输入不被序列化或打印。
    pub(in crate::skills_services::local_plugin_update) supplied:
        BTreeMap<(String, String), String>,
}

/// Actual reservation holds both request and namespace files before its first journal exists.
/// 实际预留在首份日志存在之前持有请求及命名空间文件。
pub(in crate::skills_services::local_plugin_update) struct ReservedLocalPluginUpdate {
    /// The original canonical store remains the sole durable authority.
    /// 原规范存储保持为唯一持久权威。
    pub(in crate::skills_services::local_plugin_update) store: VulcanPluginGenerationStore,
    /// Frozen original descriptor supplies exact configuration and credential declarations; None is a first install.
    /// 固定原描述符提供精确配置及凭据声明；空值表示首次安装。
    pub(in crate::skills_services::local_plugin_update) predecessor: Option<VulcanPluginDescriptor>,
    /// Non-cloneable actual files remain retained before and after uncertain first writes.
    /// 不可克隆实际文件在不确定首次写入之前及之后均保持保留。
    pub(in crate::skills_services::local_plugin_update) reservation:
        VulcanPluginActivationPreparationReservation,
    /// Original explicit inputs accompany the same reservation into the service registry.
    /// 原明确输入与相同预留一同进入服务注册表。
    pub(in crate::skills_services::local_plugin_update) source: ApprovedLocalPluginUpdateSource,
}

/// Actual preparation owns immutable allocation and optional materialized resources, without a shadow durable phase.
/// 实际准备拥有不可变分配及可选已物化资源，不设影子持久阶段。
pub(in crate::skills_services::local_plugin_update) struct PreparingLocalPluginUpdate {
    /// Store binding is copied as access only; physical file authority stays in preparation.
    /// 存储绑定仅复制访问；物理文件权威保持于 preparation。
    pub(in crate::skills_services::local_plugin_update) store: VulcanPluginGenerationStore,
    /// The first descriptor remains the unique old authority; None is a first install.
    /// 首个描述符保持为唯一旧权威；空值表示首次安装。
    pub(in crate::skills_services::local_plugin_update) predecessor: Option<VulcanPluginDescriptor>,
    /// This original request File is never moved into a worker or replaced through disk resumption.
    /// 此原请求 File 绝不移入工作者或通过磁盘恢复替换。
    pub(in crate::skills_services::local_plugin_update) preparation:
        VulcanPluginActivationPreparation,
    /// A real immutable package pin exists only after verified completion, never from a copied path.
    /// 真实不可变插件包固定仅在已验证完成后存在，绝不来自复制路径。
    pub(in crate::skills_services::local_plugin_update) generation:
        Option<PinnedVulcanPluginGeneration>,
    /// The original revision writer is retained before its first inventory or backend write.
    /// 原修订写入者在首次清单或后端写入之前保留。
    pub(in crate::skills_services::local_plugin_update) credentials:
        Option<VulcanPluginCredentialStagingReservation>,
    /// Only the actual borrowed abort returns this full terminal confirmation.
    /// 仅实际借用式中止返回此完整终态确认。
    pub(in crate::skills_services::local_plugin_update) abandoned:
        Option<VulcanPluginActivationRequestRecord>,
}

/// Each variant owns the concrete original resources; public phase always comes from actual durable observation.
/// 每个变体拥有具体原资源；公开阶段始终来自实际持久观察。
#[allow(
    clippy::large_enum_variant,
    reason = "every stage owns kilobytes of resources and exactly one value exists per update"
)]
pub(in crate::skills_services::local_plugin_update) enum PendingLocalPluginUpdateStage {
    /// First journal publication still uses the actual retained namespace owner.
    /// 首份日志发布仍使用实际保留命名空间所有者。
    Reserved(ReservedLocalPluginUpdate),
    /// Actual early request survives package and credential work plus uncertain promotion.
    /// 实际早期请求跨插件包及凭据工作和不确定晋级存续。
    Preparing(PreparingLocalPluginUpdate),
    /// Actual configured transaction grants the existing original candidate and commit operations.
    /// 实际已配置事务授予既有原候选及提交操作。
    Prepared(PreparedLocalPluginUpdate),
}

impl PendingLocalPluginUpdateRegistry {
    /// Register the actual reservation before any first journal write; reject namespace or UUID replacement.
    /// 在任何首份日志写入之前注册实际预留；拒绝命名空间或 UUID 替换。
    /// runtime and authority are the service's original pair; reserved carries real files and exact explicit inputs.
    /// runtime 及 authority 为服务原配对；reserved 携带真实文件及精确明确输入。
    pub(in crate::skills_services::local_plugin_update) fn retain_reserved(
        &self,
        runtime: VulcanPluginRuntimeManager,
        authority: vulcan_plugin_runtime::VulcanPluginRegisteredNativeMcpHostAuthority,
        reserved: ReservedLocalPluginUpdate,
    ) -> Result<Arc<PendingLocalPluginUpdate>, String> {
        if !authority.matches_manager(&runtime) {
            return Err(String::from("原更新预留的管理器与原生宿主签发者不一致。"));
        }
        // Stable lookup identity is derived directly from the actual first reservation.
        // 稳定查询身份直接从实际首次预留派生。
        let request_id = reserved.reservation.request_id();
        let plugin_id = reserved.reservation.intent().generation().plugin_id.clone();
        // One short map exclusion precedes creation of the sole retained slot.
        // 单次短暂映射互斥先于唯一保留槽创建。
        let mut owners = self.owners.lock().map_err(|error| error.to_string())?;
        if owners.contains_key(&request_id)
            || owners.values().any(|owner| owner.plugin_id == plugin_id)
        {
            return Err(String::from(
                "插件原更新请求已有服务所有者，不能替换其实际资源。",
            ));
        }
        // Inputs remain outside the replaceable resource variant throughout every borrowed transfer.
        // 输入在每次借用移交期间保持于可替换资源变体之外。
        let source = ApprovedLocalPluginUpdateSource {
            path: reserved.source.path.clone(),
            supplied: reserved.source.supplied.clone(),
        };
        let owner = Arc::new(PendingLocalPluginUpdate {
            request_id,
            plugin_id,
            full_publication_retained: std::sync::atomic::AtomicBool::new(false),
            routing_publication: Mutex::new(None),
            resources: tokio::sync::Mutex::new(PendingLocalPluginUpdateResources {
                authority,
                runtime,
                stage: PendingLocalPluginUpdateStage::Reserved(reserved),
                source: Some(source),
                configuration: None,
                values: None,
                inventory: None,
                candidate: None,
                target: None,
            }),
        });
        owners.insert(request_id, Arc::clone(&owner));
        // Registration is complete before observers run; an unwind must not poison the short map mutex.
        // 观察器运行之前注册已经完成；展开不得污染短暂映射互斥量。
        drop(owners);
        #[cfg(test)]
        observation::after(request_id, observation::Boundary::Reserved)?;
        Ok(owner)
    }
}

/// Publish the retained original then synchronously replace its concrete resource type before any copying.
/// 发布保留原请求，再在任何复制之前同步替换其具体资源类型。
/// Errors leave the same reservation and its namespace alive; success drops only the transferred predecessor type.
/// 错误让相同预留及其命名空间存活；成功仅销毁已移交前驱类型。
pub(in crate::skills_services::local_plugin_update) fn publish_reserved(
    resources: &mut PendingLocalPluginUpdateResources,
) -> Result<(), String> {
    let PendingLocalPluginUpdateStage::Reserved(reserved) = &mut resources.stage else {
        return Ok(());
    };
    // The returned owner shares the same original File; the reservation remains in the strong slot during this call.
    // 返回所有者共用相同原 File；调用期间预留保持于强槽。
    let preparation = reserved
        .store
        .publish_activation_preparation_retained(&mut reserved.reservation)
        .map_err(|error| error.to_string())?;
    #[cfg(test)]
    observation::after(preparation.request_id(), observation::Boundary::Journal)?;
    // Replacement drops the actual reserved namespace synchronously, without an empty slot or fallible intervening work.
    // 替换同步销毁实际预留命名空间，不出现空槽或插入可失败工作。
    resources.stage = PendingLocalPluginUpdateStage::Preparing(PreparingLocalPluginUpdate {
        store: reserved.store.clone(),
        predecessor: reserved.predecessor.clone(),
        preparation,
        generation: None,
        credentials: None,
        abandoned: None,
    });
    Ok(())
}

/// Initial execution performs the sole approved copy while borrowing the original service-owned File.
/// 首次执行在借用服务自有原 File 时执行唯一批准复制。
/// Successful physical resources enter the slot before configuration, inventory or backend work can fail.
/// 成功物理资源在配置、清单或后端工作可能失败之前进入槽。
pub(in crate::skills_services::local_plugin_update) fn materialize_preparation(
    resources: &mut PendingLocalPluginUpdateResources,
) -> Result<(), String> {
    publish_reserved(resources)?;
    let PendingLocalPluginUpdateStage::Preparing(preparing) = &mut resources.stage else {
        return Err(String::from("首次准备必须使用原早期所有者。"));
    };
    // No recovery caller reaches this original-source copy primitive.
    // 任何恢复调用者均不会到达此原来源复制原语。
    let source = resources.source.as_ref().ok_or("原更新来源输入缺失。")?;
    preparing.generation = Some(
        preparing
            .store
            .prepare_activation_package(&preparing.preparation, &source.path)
            .map_err(|error| error.to_string())?,
    );
    #[cfg(test)]
    observation::after(
        preparing.preparation.request_id(),
        observation::Boundary::Package,
    )?;
    complete_preparation(resources)
}

/// Recover only verified original package completion or exact already-written configured promotion; never recopy source.
/// 仅恢复已验证原插件包完成或精确已写已配置晋级；绝不重新复制来源。
/// Missing and partial packages remain explicit failures requiring original abandonment.
/// 缺失及部分插件包保持为要求原请求放弃的明确失败。
pub(in crate::skills_services::local_plugin_update) fn recover_preparation(
    resources: &mut PendingLocalPluginUpdateResources,
) -> Result<(), String> {
    recover_preparation_with_backend(resources, &OsPluginMcpSecretStore)
}

/// Drive the original recovery with the exact declared credential backend; production supplies the OS store.
/// 使用精确声明凭据后端驱动原恢复；生产提供操作系统存储。
/// Resources retain frozen expected values and every actual file owner; backend selection supplies no phase authority.
/// 资源保留固定预期值及每个实际文件所有者；后端选择不提供阶段权威。
pub(in crate::skills_services::local_plugin_update) fn recover_preparation_with_backend(
    resources: &mut PendingLocalPluginUpdateResources,
    backend: &dyn vulcan_plugin_runtime::VulcanPluginCredentialBackend,
) -> Result<(), String> {
    publish_reserved(resources)?;
    let PendingLocalPluginUpdateStage::Preparing(preparing) = &mut resources.stage else {
        return Ok(());
    };
    // A held first-inventory namespace already proves the original Preparing parent through its sealed publisher.
    // 持有首次清单命名空间时，密封发布器已证明原 Preparing 父级。
    if preparing
        .credentials
        .as_ref()
        .is_some_and(|credentials| credentials.holds_publication_namespace())
    {
        return complete_preparation_with_backend(resources, backend);
    }
    // Read the exact original body, including a real Prepared write whose result was lost before local transfer.
    // 读取精确原正文，包括结果在本地移交前丢失的真实 Prepared 写入。
    let record = preparing
        .store
        .activation_request(
            &preparing.preparation.intent().generation().plugin_id,
            preparing.preparation.request_id(),
        )
        .map_err(|error| error.to_string())?
        .ok_or("原准备日志缺失。")?;
    match record {
        VulcanPluginActivationRequestRecord::Activation(record)
            if record.phase() == Phase::Prepared =>
        {
            promote_preparation(resources)
        }
        VulcanPluginActivationRequestRecord::Preparation(record) => {
            if VulcanPluginActivationRequestRecord::Preparation(record).phase() != Phase::Preparing
            {
                return Err(String::from("原准备已放弃；必须继续同原请求的清理。"));
            }
            // The strict marker reader accepts no source and performs no repair or directory deletion.
            // 严格标记读取器不接受来源，不执行修复或目录删除。
            preparing.generation = Some(
                preparing
                    .store
                    .inspect_activation_preparation_package(&preparing.preparation)
                    .map_err(|error| {
                        format!("原插件包尚无可验证的完整准备，不能重复制；请中止相同请求：{error}")
                    })?,
            );
            complete_preparation_with_backend(resources, backend)
        }
        _ => Err(String::from("原准备日志已改变，不能重建或重放候选工作。")),
    }
}

/// Freeze original configuration and complete values before reserving or publishing credential inventory.
/// 在预留或发布凭据清单之前固定原配置及完整值。
/// Inputs derive only from the pinned immutable candidate and the originally retained descriptor and supplied values.
/// 输入仅从已固定不可变候选及原保留描述符和提交值派生。
fn freeze_inputs(resources: &mut PendingLocalPluginUpdateResources) -> Result<(), String> {
    let PendingLocalPluginUpdateStage::Preparing(preparing) = &resources.stage else {
        return Err(String::from("固定输入要求原实际准备所有者。"));
    };
    let generation = preparing.generation.as_ref().ok_or("原插件包固定缺失。")?;
    if resources.configuration.is_none() {
        resources.configuration = Some(
            resources
                .runtime
                .configuration_for_update(generation, preparing.predecessor.as_ref())
                .map_err(|error| error.to_string())?,
        );
    }
    if resources.values.is_none() {
        // This derivation occurs before any inventory/backend operation and is never repeated after values enter the slot.
        // 此派生在任何清单或后端操作之前发生；值进入槽后绝不重复。
        let source = resources.source.as_ref().ok_or("原秘密输入缺失。")?;
        let candidate =
            vulcan_plugin_runtime::inspect_user_plugin_package(generation.package_root())?;
        let secrets = OsPluginMcpSecretStore;
        let mut values = VulcanPluginCredentialValues::new();
        for server in &candidate.mcp_servers {
            for secret_key in server.manifest.declared_secret_keys() {
                let key = VulcanPluginCredentialKey {
                    server_id: server.id.clone(),
                    secret_key,
                };
                let value = if let Some(value) = source
                    .supplied
                    .get(&(key.server_id.clone(), key.secret_key.clone()))
                {
                    Some(value.clone())
                } else if let Some(previous_server) = preparing
                    .predecessor
                    .iter()
                    .flat_map(|predecessor| &predecessor.mcp_servers)
                    .find(|previous| {
                        previous.id == server.id
                            && previous
                                .manifest
                                .declared_secret_keys()
                                .contains(&key.secret_key)
                    })
                {
                    secrets.read_selected_optional_secret(
                        &preparing.preparation.intent().generation().plugin_id,
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
        resources.values = Some(values);
    }
    Ok(())
}

/// Finish actual original credentials and promote the same retained File, preserving frozen values across every error.
/// 完成实际原凭据并晋级相同保留 File，在每次错误期间保留固定值。
fn complete_preparation(resources: &mut PendingLocalPluginUpdateResources) -> Result<(), String> {
    complete_preparation_with_backend(resources, &OsPluginMcpSecretStore)
}

/// Complete real inventory and promotion using frozen values and one explicit backend.
/// 使用固定值及单个明确后端完成真实清单与晋级。
/// Test file backends exercise actual core calls without changing the production OS implementation.
/// 测试文件后端使用实际核心调用，不改变生产操作系统实现。
fn complete_preparation_with_backend(
    resources: &mut PendingLocalPluginUpdateResources,
    backend: &dyn vulcan_plugin_runtime::VulcanPluginCredentialBackend,
) -> Result<(), String> {
    freeze_inputs(resources)?;
    let PendingLocalPluginUpdateStage::Preparing(preparing) = &mut resources.stage else {
        return Err(String::from("完成准备要求原早期所有者。"));
    };
    let values = resources.values.as_ref().ok_or("原固定凭据值缺失。")?;
    if preparing.credentials.is_none() && resources.inventory.is_none() {
        // Retain the actual revision writer before any first inventory or secure-store effect.
        // 在任何首份清单或安全存储副作用之前保留实际修订写入者。
        preparing.credentials = Some(
            preparing
                .store
                .reserve_activation_credential_staging(
                    &preparing.preparation,
                    preparing.generation.as_ref().ok_or("原插件包固定缺失。")?,
                    values,
                )
                .map_err(|error| error.to_string())?,
        );
        #[cfg(test)]
        observation::after(
            preparing.preparation.request_id(),
            observation::Boundary::CredentialReservation,
        )?;
    }
    if let Some(credentials) = preparing.credentials.as_mut() {
        // Namespace presence is the actual File fact, never a service shadow progress flag.
        // 命名空间存在性是实际 File 事实，绝不是服务影子进度标记。
        let record = if credentials.holds_publication_namespace() {
            preparing
                .store
                .publish_activation_credential_staging_retained(credentials, &preparing.preparation)
                .map_err(|error| error.to_string())?
                .record()
                .clone()
        } else {
            credentials
                .reconcile_owned()
                .map_err(|error| error.to_string())?
        };
        resources.inventory = Some(record.clone());
        #[cfg(test)]
        observation::after(
            preparing.preparation.request_id(),
            observation::Boundary::Inventory,
        )?;
        if record.phase() == VulcanPluginCredentialPhase::Preparing {
            // The exact original writer reads/writes its target slots using the already frozen expected values.
            // 精确原写入者使用已固定预期值读写其目标槽位。
            let staged = preparing
                .store
                .publish_activation_credential_staging_retained(credentials, &preparing.preparation)
                .map_err(|error| error.to_string())?
                .write_and_finish(backend, values)
                .map_err(|error| error.to_string())?;
            resources.inventory = Some(staged);
            #[cfg(test)]
            observation::after(
                preparing.preparation.request_id(),
                observation::Boundary::Staged,
            )?;
        }
        if resources.inventory.as_ref().map(|record| record.phase())
            != Some(VulcanPluginCredentialPhase::Staged)
        {
            return Err(String::from("原凭据尚未确认实际暂存完成。"));
        }
        // True Staged confirmation retires the actual exclusive revision and shared parent owner before promotion.
        // 真实 Staged 确认在晋级之前退役实际排他修订及共享父所有者。
        drop(preparing.credentials.take());
    }
    if resources.inventory.as_ref().map(|record| record.phase())
        != Some(VulcanPluginCredentialPhase::Staged)
    {
        return Err(String::from("原准备缺少真实完整 Staged 凭据确认。"));
    }
    promote_preparation(resources)
}

/// Confirm the original configured target with frozen configuration, then synchronously transfer the same File to Prepared.
/// 使用固定配置确认原已配置目标，再同步将相同 File 移交至 Prepared。
fn promote_preparation(resources: &mut PendingLocalPluginUpdateResources) -> Result<(), String> {
    let PendingLocalPluginUpdateStage::Preparing(preparing) = &mut resources.stage else {
        return Ok(());
    };
    let configuration = resources
        .configuration
        .as_ref()
        .ok_or("原固定配置缺失，不能认领已写晋级。")?
        .clone();
    // This borrowed API keeps the original preparation alive on writer failure, readback loss or unwind.
    // 此借用接口在写入者失败、回读丢失或展开时让原准备存活。
    let transaction = preparing
        .store
        .finish_activation_preparation_retained(&mut preparing.preparation, configuration)
        .map_err(|error| error.to_string())?;
    #[cfg(test)]
    observation::after(
        transaction.record().request_id(),
        observation::Boundary::Promotion,
    )?;
    resources.stage = PendingLocalPluginUpdateStage::Prepared(PreparedLocalPluginUpdate {
        store: preparing.store.clone(),
        predecessor: preparing.predecessor.clone(),
        transaction,
    });
    Ok(())
}

/// Compare every immutable inventory field; cleanup may advance only its explicitly declared checkpoint.
/// 比较全部不可变清单字段；清理仅可推进其明确声明检查点。
/// Original and observed records contain no secret values and must name the same original request and complete keys.
/// 原记录及观察记录均不含秘密值，且必须指向相同原请求及完整键集合。
fn same_inventory(
    original: &vulcan_plugin_runtime::VulcanPluginCredentialRecord,
    observed: &vulcan_plugin_runtime::VulcanPluginCredentialRecord,
) -> bool {
    original.activation_request() == observed.activation_request()
        && original.package() == observed.package()
        && original.revision() == observed.revision()
        && original.keys() == observed.keys()
        && original.configured_keys() == observed.configured_keys()
}

/// Permit only exact original or forward cleanup checkpoints, never a substituted backward staging state.
/// 仅允许精确原检查点或前进清理检查点，绝不允许被替换的后退暂存状态。
fn cleanup_checkpoint(
    original: VulcanPluginCredentialPhase,
    actual: VulcanPluginCredentialPhase,
) -> bool {
    use VulcanPluginCredentialPhase::*;
    matches!(
        (original, actual),
        (Preparing, Preparing | Discarding | Discarded)
            | (Staged, Staged | Discarding | Discarded)
            | (Discarding, Discarding | Discarded)
            | (Discarded, Discarded)
    )
}

/// Abandon and clean the actual early owner; an already promoted target transfers to the existing transaction abort.
/// 放弃并清理实际早期所有者；已经晋级的目标移交给既有事务中止。
/// A returned true means full original terminal cleanup is confirmed; failures keep all remaining original resources.
/// 返回 true 表示完整原终态清理已经确认；失败保留全部剩余原资源。
pub(in crate::skills_services::local_plugin_update) fn abort_preparation(
    resources: &mut PendingLocalPluginUpdateResources,
    acknowledge: bool,
) -> Result<bool, String> {
    abort_preparation_with_backend(resources, acknowledge, &OsPluginMcpSecretStore)
}

/// Clean the original explicit backend only after the same actual parent confirms Aborted.
/// 仅在相同实际父级确认 Aborted 后清理原明确后端。
/// Backend calls cannot replace request or revision ownership; all cleanup errors retain the exact original slot.
/// 后端调用不能替换请求或修订所有权；全部清理错误保留精确原槽。
pub(in crate::skills_services::local_plugin_update) fn abort_preparation_with_backend(
    resources: &mut PendingLocalPluginUpdateResources,
    acknowledge: bool,
    backend: &dyn vulcan_plugin_runtime::VulcanPluginCredentialBackend,
) -> Result<bool, String> {
    // Reserved has not admitted a backend; publication creates the exact history needed by the borrowed abort.
    // Reserved 尚未接纳后端；发布创建借用式中止所需精确历史。
    let reserved = matches!(resources.stage, PendingLocalPluginUpdateStage::Reserved(_));
    publish_reserved(resources)?;
    let PendingLocalPluginUpdateStage::Preparing(preparing) = &mut resources.stage else {
        return Ok(false);
    };
    if preparing.abandoned.is_none() {
        // Namespace release is a physical fact: only the ordinary reader may run when no original namespace is held.
        // 命名空间释放是物理事实：仅原命名空间未持有时才可运行普通读取器。
        let held = preparing
            .credentials
            .as_ref()
            .is_some_and(|credentials| credentials.holds_publication_namespace());
        let mut actual_aborted = false;
        if !held {
            let record = preparing
                .store
                .activation_request(
                    &preparing.preparation.intent().generation().plugin_id,
                    preparing.preparation.request_id(),
                )
                .map_err(|error| error.to_string())?
                .ok_or("原准备日志缺失。")?;
            if matches!(record, VulcanPluginActivationRequestRecord::Activation(_)) {
                // Frozen configuration is the only authority for claiming an actual promotion with a lost result.
                // 固定配置是认领结果丢失的实际晋级的唯一权威。
                promote_preparation(resources)?;
                return Ok(false);
            }
            actual_aborted = record.phase() == Phase::Aborted;
        }
        if !reserved && !actual_aborted && !acknowledge {
            return Err(String::from(
                "原准备可能已有实际插件包或凭据副作用；必须明确确认原请求放弃。",
            ));
        }
        if let Some(credentials) = preparing.credentials.as_mut() {
            // Confirm the original inventory without replaying any secure backend write.
            // 确认原清单，不重放任何安全后端写入。
            let record = if credentials.holds_publication_namespace() {
                preparing
                    .store
                    .publish_activation_credential_staging_retained(
                        credentials,
                        &preparing.preparation,
                    )
                    .map_err(|error| error.to_string())?
                    .record()
                    .clone()
            } else {
                credentials
                    .reconcile_owned()
                    .map_err(|error| error.to_string())?
            };
            resources.inventory = Some(record);
            // Retire the original revision writer and shared parent Arc before the borrowed terminal writer.
            // 在借用式终态写入器之前退役原修订写入者及共享父 Arc。
            drop(preparing.credentials.take());
        }
        // No executor can perform backend work while this exclusive concrete resource borrow is held.
        // 此排他具体资源借用持有期间，任何执行器均不能执行后端工作。
        let record = preparing
            .store
            .abort_activation_preparation_retained(&mut preparing.preparation)
            .map_err(|error| error.to_string())?;
        preparing.abandoned = Some(record);
        #[cfg(test)]
        observation::after(
            preparing.preparation.request_id(),
            observation::Boundary::Aborted,
        )?;
    }
    // Parent terminal history, original request File and the real revision lock jointly fence cleanup retry.
    // 父终态历史、原请求 File 及真实修订锁共同约束清理重试。
    let plugin_id = preparing
        .preparation
        .intent()
        .generation()
        .plugin_id
        .clone();
    // Reconfirm the complete actual terminal owner before any subsequent destructive backend or package operation.
    // 任何随后破坏性后端或插件包操作之前，重新确认完整实际终态所有者。
    let original = preparing
        .store
        .abort_activation_preparation_retained(&mut preparing.preparation)
        .map_err(|error| error.to_string())?;
    if preparing.abandoned.as_ref() != Some(&original) {
        return Err(String::from("原 Aborted 完整正文发生变化，不能继续清理。"));
    }
    let revision = preparing.preparation.intent().credential_revision();
    let actual = preparing
        .store
        .credential_record(&plugin_id, revision)
        .map_err(|error| error.to_string())?;
    match (&resources.inventory, actual) {
        (None, None) => {
            // No writer was ever created, so backend admission was impossible; this is actual canonical absence.
            // 从未创建写入者，因此后端准入不可能；此为实际规范缺失。
        }
        (Some(original), Some(actual))
            if same_inventory(original, &actual)
                && cleanup_checkpoint(original.phase(), actual.phase()) =>
        {
            match actual.phase() {
                VulcanPluginCredentialPhase::Preparing
                | VulcanPluginCredentialPhase::Discarding => {
                    preparing
                        .store
                        .discard_credential_staging(&plugin_id, revision, backend)
                        .map_err(|error| error.to_string())?;
                }
                VulcanPluginCredentialPhase::Staged => {
                    preparing
                        .store
                        .collect_credentials(&plugin_id, revision, backend)
                        .map_err(|error| error.to_string())?;
                }
                VulcanPluginCredentialPhase::Discarded => {}
            }
            let discarded = preparing
                .store
                .credential_record(&plugin_id, revision)
                .map_err(|error| error.to_string())?
                .ok_or("原清理清单消失。")?;
            if !same_inventory(original, &discarded)
                || discarded.phase() != VulcanPluginCredentialPhase::Discarded
            {
                return Err(String::from("原凭据清理尚未得到完整 Discarded 回读。"));
            }
            resources.inventory = Some(discarded);
        }
        _ => return Err(String::from("原凭据清单缺失或被替换，保留同原请求供检查。")),
    }
    #[cfg(test)]
    observation::after(
        preparing.preparation.request_id(),
        observation::Boundary::Discarded,
    )?;
    // Actual package readers must retire before the original exclusive package collection primitive can run.
    // 实际插件包读者必须在原排他插件包回收原语运行之前退役。
    drop(preparing.generation.take());
    preparing
        .store
        .collect_activation_preparation_package_retained(&preparing.preparation)
        .map_err(|error| error.to_string())?;
    #[cfg(test)]
    observation::after(
        preparing.preparation.request_id(),
        observation::Boundary::Collected,
    )?;
    confirm_abandoned_retirement(resources.inventory.as_ref(), preparing)?;
    Ok(true)
}

/// Require actual full Aborted history, terminal inventory and byte absence before removing the original strong slot.
/// 移除原强槽之前，要求实际完整 Aborted 历史、终态清单及字节缺失。
/// This readback borrows the same original owner and creates no new request, preparation or credential backend work.
/// 此回读借用相同原所有者，不创建新请求、准备或凭据后端工作。
pub(super) fn confirm_abandoned_retirement(
    inventory: Option<&vulcan_plugin_runtime::VulcanPluginCredentialRecord>,
    preparing: &PreparingLocalPluginUpdate,
) -> Result<(), String> {
    let original = preparing
        .abandoned
        .as_ref()
        .ok_or("原准备尚未确认 Aborted。")?;
    if preparing.credentials.is_some()
        || preparing.generation.is_some()
        || original.phase() != Phase::Aborted
        || preparing
            .store
            .activation_request(
                &preparing.preparation.intent().generation().plugin_id,
                preparing.preparation.request_id(),
            )
            .map_err(|error| error.to_string())?
            .as_ref()
            != Some(original)
    {
        return Err(String::from("原准备终态或实际资源退役尚未得到完整确认。"));
    }
    let actual = preparing
        .store
        .credential_record(
            &preparing.preparation.intent().generation().plugin_id,
            preparing.preparation.intent().credential_revision(),
        )
        .map_err(|error| error.to_string())?;
    match (inventory, actual) {
        (None, None) => {}
        (Some(original), Some(actual))
            if original == &actual && actual.phase() == VulcanPluginCredentialPhase::Discarded => {}
        _ => return Err(String::from("原终态凭据清单尚未完整确认。")),
    }
    // The sealed original collector verifies its full request, index, selection and exact marker/byte observation.
    // 密封原回收器校验完整请求、索引、选择及精确标记与字节观察。
    preparing
        .store
        .collect_activation_preparation_package_retained(&preparing.preparation)
        .map_err(|error| error.to_string())?;
    Ok(())
}

/// Project complete sealed history through actual retained namespace resources, or use the ordinary reader when they are unheld.
/// 通过实际保留命名空间资源投影完整密封历史，或在其未持有时使用普通读取器。
/// An occupied async borrow takes the declared ordinary-read path; Busy and Conflict remain errors without fallback.
/// 异步借用被占用时采用声明的普通读取路径；Busy 及 Conflict 保持错误，不回退。
pub(in crate::skills_services::local_plugin_update) fn list_owned(
    runtime: &VulcanPluginRuntimeManager,
    plugin_id: &str,
    registry: &PendingLocalPluginUpdateRegistry,
) -> Result<Vec<LocalPluginUpdateStatus>, String> {
    let Some(owner) = registry.plugin_owner(plugin_id)? else {
        return list(runtime, plugin_id);
    };
    // Only a nonblocking borrow can inspect which concrete owner currently retains the real namespace File.
    // 仅非阻塞借用可检查当前哪种具体所有者保留真实命名空间 File。
    let sealed = if let Ok(resources) = owner.resources.try_lock() {
        match &resources.stage {
            PendingLocalPluginUpdateStage::Reserved(reserved) => {
                let inspection = reserved
                    .store
                    .inspect_activation_preparation_reservation(&reserved.reservation)
                    .map_err(|error| error.to_string())?;
                let mut rows = inspection_rows(&owner, &inspection)?;
                if inspection.original().is_none() {
                    // True absence is confirmed by the original held-namespace reader, not an error or missing public row.
                    // 真实缺失由原持有命名空间读取器确认，不来自错误或缺失公开行。
                    rows.push(LocalPluginUpdateStatus {
                        request_id: owner.request_id.to_string(),
                        plugin_id: owner.plugin_id.clone(),
                        phase: None,
                        candidate: reserved.reservation.intent().generation().clone(),
                        activation_revision: reserved
                            .reservation
                            .intent()
                            .activation_revision()
                            .to_string(),
                        installation: None,
                        effects: Vec::new(),
                        has_service_owner: true,
                    });
                }
                Some(rows)
            }
            PendingLocalPluginUpdateStage::Preparing(preparing) => {
                if let Some(credentials) = preparing
                    .credentials
                    .as_ref()
                    .filter(|credentials| credentials.holds_publication_namespace())
                {
                    let inspection = preparing
                        .store
                        .inspect_activation_preparation_credential_staging_reservation(
                            &preparing.preparation,
                            credentials,
                        )
                        .map_err(|error| error.to_string())?;
                    Some(inspection_rows(&owner, &inspection)?)
                } else {
                    None
                }
            }
            PendingLocalPluginUpdateStage::Prepared(_) => None,
        }
    } else {
        None
    };
    if let Some(rows) = sealed {
        return Ok(rows);
    }
    // No namespace grant was used above; this sole ordinary reader must expose any actual lock contention.
    // 上述未使用命名空间授权；此唯一普通读取器必须暴露任何实际锁争用。
    let mut rows = list(runtime, plugin_id)?;
    for row in &mut rows {
        row.has_service_owner = row.request_id == owner.request_id.to_string();
    }
    Ok(rows)
}

/// Build sanitized rows exclusively from complete sealed request and sidecar observations.
/// 仅从完整密封请求及附属观察构建脱敏行。
/// Missing sidecar entries are protocol errors; physical None entries are confirmed absence.
/// 缺失附属映射项是协议错误；物理 None 项是已确认缺失。
fn inspection_rows(
    owner: &PendingLocalPluginUpdate,
    inspection: &VulcanPluginActivationPreparationInspection,
) -> Result<Vec<LocalPluginUpdateStatus>, String> {
    inspection
        .requests()
        .iter()
        .map(|(request_id, record)| {
            let candidate = record.candidate().ok_or("原历史请求缺少候选身份。")?;
            let effects = inspection
                .effects()
                .get(request_id)
                .ok_or("原历史请求缺少完整副作用观察。")?;
            let installation = inspection
                .installations()
                .get(request_id)
                .ok_or("原历史请求缺少完整安装观察。")?;
            Ok(LocalPluginUpdateStatus {
                request_id: request_id.to_string(),
                plugin_id: candidate.plugin_id.clone(),
                phase: Some(record.phase()),
                candidate: candidate.clone(),
                activation_revision: record.activation_revision().to_string(),
                effects: effects
                    .as_ref()
                    .map(|receipt| {
                        receipt
                            .observations()
                            .into_iter()
                            .map(|(operation, phase)| LocalPluginUpdateEffect { operation, phase })
                            .collect()
                    })
                    .unwrap_or_default(),
                installation: installation.as_ref().map(|receipt| receipt.phase()),
                has_service_owner: *request_id == owner.request_id,
            })
        })
        .collect()
}

/// Exact original post-success observation hooks exercise real file writers without replacing production authority.
/// 精确原请求成功后观察钩子覆盖真实文件写入器，不替换生产权威。
#[cfg(test)]
pub(in crate::skills_services::local_plugin_update) mod observation {
    /// Each boundary follows an actual successful write or resource materialization.
    /// 每个边界均跟随实际成功写入或资源物化。
    #[derive(Clone, Copy, PartialEq, Eq)]
    pub(in crate::skills_services::local_plugin_update) enum Boundary {
        /// Map registration precedes the first actual journal.
        /// 映射注册先于首份实际日志。
        Reserved,
        Journal,
        Package,
        /// Actual revision and namespace files enter the slot before inventory publication.
        /// 实际修订及命名空间文件在清单发布之前进入槽。
        CredentialReservation,
        Inventory,
        Staged,
        Promotion,
        Aborted,
        Discarded,
        Collected,
    }
    std::thread_local! {
        /// One thread-bound exact UUID fault cannot alter unrelated requests or production execution.
        /// 单个线程绑定精确 UUID 故障不能改变无关请求或生产执行。
        static FAULT: std::cell::RefCell<Option<(uuid::Uuid, Boundary, bool)>> = const { std::cell::RefCell::new(None) };
    }
    /// Scoped test guard retires only the observer registration and never any original physical owner.
    /// 作用域测试守卫仅退役观察注册，绝不退役任何原物理所有者。
    pub(in crate::skills_services::local_plugin_update) struct Scope;
    impl Drop for Scope {
        /// Clear the thread's exact observation registration during ordinary exit or unwinding.
        /// 在正常退出或展开期间清除线程精确观察注册。
        fn drop(&mut self) {
            FAULT.with(|fault| {
                fault.borrow_mut().take();
            });
        }
    }
    /// Arm a single actual post-success loss for the supplied original UUID and boundary.
    /// 为提供原 UUID 及边界启用单次实际成功后丢失。
    /// Panic selects unwind; false returns an explicit observer error after the real writer succeeded.
    /// panic 选择展开；false 在真实写入器成功后返回明确观察错误。
    pub(in crate::skills_services::local_plugin_update) fn arm(
        request: uuid::Uuid,
        boundary: Boundary,
        panic: bool,
    ) -> Scope {
        FAULT.with(|fault| {
            let mut fault = fault.borrow_mut();
            assert!(fault.is_none(), "original observation scopes overlap");
            *fault = Some((request, boundary, panic));
        });
        Scope
    }
    /// Consume only the exact original boundary's observation loss; absence leaves actual production behavior unchanged.
    /// 仅消费精确原边界观察丢失；缺失时保持实际生产行为。
    pub(super) fn after(request: uuid::Uuid, boundary: Boundary) -> Result<(), String> {
        let loss = FAULT.with(|fault| {
            let mut fault = fault.borrow_mut();
            if fault
                .as_ref()
                .is_some_and(|(original, armed, _)| *original == request && *armed == boundary)
            {
                fault.take().map(|(_, _, panic)| panic)
            } else {
                None
            }
        });
        match loss {
            Some(true) => panic!("original preparation observer lost after actual success"),
            Some(false) => Err(String::from("原实际成功后的观察返回丢失。")),
            None => Ok(()),
        }
    }
}
