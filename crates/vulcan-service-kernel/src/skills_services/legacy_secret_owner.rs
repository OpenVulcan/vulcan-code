//! Retain exact Legacy credential ownership before the first native write.
//! 在首次原生写入之前保留精确旧凭据所有权。

use super::*;

/// Strong request ownership survives management executor cancellation and panic.
/// 强请求所有权跨管理执行器取消及 panic 存续。
#[derive(Default)]
pub(crate) struct LegacySecretMutationRegistry {
    /// Only the service removes confirmed terminal owners; no timeout expires their physical locks.
    /// 仅服务移除已确认终态所有者；超时不使其物理锁过期。
    owners: Mutex<BTreeMap<uuid::Uuid, Arc<LegacySecretMutationOwner>>>,
}

/// Public observations contain exact lookup identity and phase, without credential values or resources.
/// 公共观测包含精确查询身份及阶段，不含凭据值或资源。
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LegacySecretMutationStatus {
    /// This allocated original UUID is the only recovery key.
    /// 此已分配原 UUID 为唯一恢复键。
    pub(crate) request_id: uuid::Uuid,
    /// Exact plugin namespace retained before writing.
    /// 写入之前保留的精确插件命名空间。
    pub(crate) plugin_id: String,
    /// Exact declared server retained before writing.
    /// 写入之前保留的精确已声明服务器。
    pub(crate) server_id: String,
    /// Manifest key identity is public; neither its original nor next value is exposed.
    /// 清单键身份为公开值；不暴露其原值或下一值。
    pub(crate) secret_key: String,
    /// An executing borrow does not claim that native effects have or have not started.
    /// 执行中的借用不声称原生副作用已开始或尚未开始。
    pub(crate) phase: LegacySecretMutationPhase,
}

/// Typed progress records which recovery proof is still required.
/// 类型化进度记录仍需哪种恢复证明。
#[derive(Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum LegacySecretMutationPhase {
    /// Public query could not exclusively borrow an active executor's resources.
    /// 公共查询无法排他借用活动执行器的资源。
    Executing,
    /// The first write may have completed, but no native partition mutation has been admitted.
    /// 首次写入可能已完成，但尚未准入原生分区变更。
    CredentialMutation,
    /// Native partition effects may exist and require actual source retirement proof.
    /// 原生分区副作用可能存在，需要实际来源退役证明。
    NativeRefresh,
    /// Actual OS readback confirmed the original preimage without any admitted native refresh.
    /// 实际操作系统回读确认原前像，且没有已准入原生刷新。
    Restored,
    /// The exact mutation and any required full publication succeeded.
    /// 精确变更及任何所需完整发布已成功。
    Committed,
}

/// Immutable account identity encloses the unique asynchronous resource borrow.
/// 不可变账户身份封装唯一异步资源借用。
pub(super) struct LegacySecretMutationOwner {
    /// UUID is allocated before the first OS write, never reconstructed after failure.
    /// UUID 在首次操作系统写入前分配，绝不在失败后重建。
    pub(super) request_id: uuid::Uuid,
    /// Exact original account namespace.
    /// 精确原账户命名空间。
    plugin_id: String,
    /// Exact original declared server.
    /// 精确原已声明服务器。
    server_id: String,
    /// Exact original declared key.
    /// 精确原已声明键。
    secret_key: String,
    /// Panic releases only the borrow, leaving the actual guard and preimage owned by this slot.
    /// panic 仅释放借用，使实际屏障及前像仍由此槽拥有。
    pub(super) resources: tokio::sync::Mutex<LegacySecretMutationResources>,
}

/// Non-serializable resources retain the exact non-cloneable file guard and original OS value.
/// 不可序列化资源保留精确不可克隆文件屏障及原操作系统值。
pub(super) struct LegacySecretMutationResources {
    /// The same cross-process file remains locked across writes, awaits and recovery.
    /// 相同跨进程文件跨写入、等待及恢复保持锁定。
    pub(super) mutation: vulcan_plugin_runtime::VulcanPluginLegacyMutationGuard,
    /// None means actual original absence; a later read can never replace this preimage.
    /// 空值表示实际原缺失；随后读取绝不能替换此前像。
    previous_value: Option<String>,
    /// Phase advances before its first potentially uncertain effect.
    /// 阶段在其首次可能不确定副作用之前推进。
    pub(super) phase: LegacySecretMutationPhase,
    /// An enabled request retains its exact undispatched replacement before the first native effect.
    /// 启用请求在首次原生副作用前保留其精确未派发替换。
    pub(super) native: Option<vulcan_agent_runtime::mcp::LegacyPluginMcpReplacement>,
}

impl LegacySecretMutationOwner {
    /// Retire only this request's unpublished original before restoring its exact OS preimage.
    /// 恢复精确操作系统前像之前，仅退役此请求的未发布原实例。
    /// Resources are the service's exclusive borrow; timeout preserves the actual owner for retry.
    /// resources 为服务排他借用；超时保留实际所有者，以供重试。
    pub(super) async fn rollback(
        &self,
        resources: &mut LegacySecretMutationResources,
    ) -> Result<(), String> {
        if resources.phase == LegacySecretMutationPhase::NativeRefresh {
            // Neither status nor a current partition can substitute for the retained original controller.
            // 状态及当前分区均不能替代保留原控制器。
            let native = resources.native.as_ref().ok_or_else(|| {
                String::from(
                    "Original credential request has no retained native replacement owner.",
                )
            })?;
            native.abort_and_drain().await?;
            // Positive original retirement precedes credential compensation.
            // 正向原退役先于凭据补偿。
            resources.phase = LegacySecretMutationPhase::CredentialMutation;
        }
        self.restore_preimage(resources)
    }

    /// Restore this original account under its retained guard and verify actual OS readback.
    /// 在保留屏障下恢复此原账户，并验证实际操作系统回读。
    /// Resources are the exclusive original borrow; errors retain ownership and unfinished phase.
    /// resources 为排他原借用；错误保留所有权及未完成阶段。
    fn restore_preimage(
        &self,
        resources: &mut LegacySecretMutationResources,
    ) -> Result<(), String> {
        if !matches!(
            resources.phase,
            LegacySecretMutationPhase::CredentialMutation | LegacySecretMutationPhase::Restored
        ) {
            return Err(String::from(
                "Original credential request cannot be rolled back in this phase.",
            ));
        }
        // Do not re-read selection or acquire a different namespace owner during compensation.
        // 补偿期间不重新读取选择或获取不同命名空间所有者。
        let store = crate::plugin_mcp_secret_store::OsPluginMcpSecretStore;
        restore_guarded_plugin_mcp_secret(
            &store,
            &resources.mutation,
            &self.plugin_id,
            &self.server_id,
            &self.secret_key,
            resources.previous_value.as_deref(),
        )?;
        // A successful adapter return alone does not certify the exact preimage.
        // 仅适配器成功返回不足以证明精确前像。
        let actual =
            store.read_optional_secret(&self.plugin_id, &self.server_id, &self.secret_key)?;
        if actual != resources.previous_value {
            return Err(String::from(
                "Original credential restoration readback did not match.",
            ));
        }
        // Readback cannot certify retirement if the retained original selection ceased to be valid.
        // 保留原选择不再有效时，回读不能证明退役。
        resources
            .mutation
            .validate_plugin(&self.plugin_id)
            .map_err(|error| error.to_string())?;
        resources.phase = LegacySecretMutationPhase::Restored;
        Ok(())
    }
}

impl LegacySecretMutationRegistry {
    /// Retain the actual guard and preimage before any OS mutation; identities select one declared account.
    /// 在任何操作系统变更前保留实际屏障及前像；标识选择单个已声明账户。
    /// Returns the same strong slot, rejecting a second owner of its plugin namespace.
    /// 返回相同强槽，拒绝其插件命名空间的第二个所有者。
    pub(super) fn retain(
        &self,
        plugin_id: String,
        server_id: String,
        secret_key: String,
        mutation: vulcan_plugin_runtime::VulcanPluginLegacyMutationGuard,
        previous_value: Option<String>,
    ) -> Result<Arc<LegacySecretMutationOwner>, String> {
        // Registry lookup never borrows secret resources or holds a lock across native effects.
        // 注册表查询绝不借用秘密资源或跨原生副作用持锁。
        let mut owners = self.owners.lock().map_err(|error| error.to_string())?;
        if owners.values().any(|owner| owner.plugin_id == plugin_id) {
            return Err(String::from(
                "Plugin still has an original credential mutation owner.",
            ));
        }
        // Allocate once under the registry lock; no fallible backend operation precedes insertion.
        // 在注册表锁内仅分配一次；插入之前没有可能失败的后端操作。
        let request_id = uuid::Uuid::new_v4();
        let owner = Arc::new(LegacySecretMutationOwner {
            request_id,
            plugin_id,
            server_id,
            secret_key,
            resources: tokio::sync::Mutex::new(LegacySecretMutationResources {
                mutation,
                previous_value,
                phase: LegacySecretMutationPhase::CredentialMutation,
                native: None,
            }),
        });
        owners.insert(request_id, Arc::clone(&owner));
        Ok(owner)
    }

    /// List exact plugin requests without waiting for executors; observations never authorize recovery.
    /// 列出精确插件请求，不等待执行器；观测绝不授权恢复。
    /// Returns non-secret phases and original UUIDs only.
    /// 仅返回无秘密阶段及原 UUID。
    pub(super) fn list(&self, plugin_id: &str) -> Result<Vec<LegacySecretMutationStatus>, String> {
        // Clone only strong owner handles before observing asynchronous borrows.
        // 观察异步借用前，仅克隆强所有者句柄。
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
            .map(|owner| LegacySecretMutationStatus {
                request_id: owner.request_id,
                plugin_id: owner.plugin_id.clone(),
                server_id: owner.server_id.clone(),
                secret_key: owner.secret_key.clone(),
                phase: owner
                    .resources
                    .try_lock()
                    .map(|resources| resources.phase)
                    .unwrap_or(LegacySecretMutationPhase::Executing),
            })
            .collect())
    }

    /// Lookup only the retained original plugin and UUID; absence rejects, without latest-account recovery.
    /// 仅查询保留的原插件及 UUID；缺失即拒绝，不进行最新账户恢复。
    /// Returns the exact original strong owner or a non-secret identity error.
    /// 返回精确原强所有者，或无秘密身份错误。
    pub(super) fn get(
        &self,
        plugin_id: &str,
        request_id: uuid::Uuid,
    ) -> Result<Arc<LegacySecretMutationOwner>, String> {
        // Never reconstruct a guard or preimage from a public status row.
        // 绝不从公共状态行重建屏障或前像。
        let owners = self.owners.lock().map_err(|error| error.to_string())?;
        owners
            .get(&request_id)
            .filter(|owner| owner.plugin_id == plugin_id)
            .cloned()
            .ok_or_else(|| {
                String::from("Original credential mutation request was not found for this plugin.")
            })
    }

    /// Retire this exact owner only under its exclusive confirmed terminal resource borrow.
    /// 仅在此精确所有者的排他已确认终态资源借用下退役。
    /// Returns an error without removing uncertain or substituted original ownership.
    /// 返回错误时不移除不确定或被替换的原所有权。
    pub(super) fn retire(
        &self,
        owner: &Arc<LegacySecretMutationOwner>,
        resources: &LegacySecretMutationResources,
    ) -> Result<(), String> {
        if !matches!(
            resources.phase,
            LegacySecretMutationPhase::Restored | LegacySecretMutationPhase::Committed
        ) {
            return Err(String::from(
                "Original native credential mutation resources remain unconfirmed.",
            ));
        }
        // Exact Arc identity prevents a stale executor from deleting any later request.
        // 精确 Arc 身份防止旧执行器删除任何后续请求。
        let mut owners = self.owners.lock().map_err(|error| error.to_string())?;
        if !owners
            .get(&owner.request_id)
            .is_some_and(|current| Arc::ptr_eq(current, owner))
        {
            return Err(String::from(
                "Original credential mutation owner changed before retirement.",
            ));
        }
        owners.remove(&owner.request_id);
        Ok(())
    }

    /// Drain each retained original before service teardown, including actual unpublished native cleanup.
    /// 服务拆卸前排空每个保留原实例，包括实际未发布原生清理。
    /// Returns success only after all retained original owners have actually reached a terminal phase.
    /// 仅在全部保留原所有者实际达到终态后返回成功。
    pub(crate) async fn drain_for_shutdown(&self) -> Result<(), String> {
        // Shutdown admission is already closed, so this snapshot cannot gain a new writer.
        // 关闭准入已关闭，因此此快照不能新增写入者。
        let owners = self
            .owners
            .lock()
            .map_err(|error| error.to_string())?
            .values()
            .cloned()
            .collect::<Vec<_>>();
        for owner in owners {
            // Await the actual executor's exclusive borrow without holding service or registry locks.
            // 等待实际执行器的排他借用，不持有服务或注册表锁。
            let mut resources = owner.resources.lock().await;
            if matches!(
                resources.phase,
                LegacySecretMutationPhase::CredentialMutation
                    | LegacySecretMutationPhase::NativeRefresh
            ) {
                owner.rollback(&mut resources).await?;
            }
            if !matches!(
                resources.phase,
                LegacySecretMutationPhase::Restored | LegacySecretMutationPhase::Committed
            ) {
                return Err(format!(
                    "Original credential request `{}` still requires actual native partition recovery.",
                    owner.request_id
                ));
            }
            // A completing executor may already have removed this same terminal slot.
            // 完成中的执行器可能已经移除此相同终态槽。
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

    /// Without an asynchronous runtime, require no unfinished physical owner before teardown.
    /// 无异步运行时时，在拆卸之前要求没有未完成物理所有者。
    /// Returns explicit refusal without dropping any retained file guard or preimage.
    /// 返回明确拒绝，不丢弃任何保留文件屏障或前像。
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
                "Original credential mutations require asynchronous recovery before shutdown.",
            ))
        }
    }
}
