//! Explicit package preparation, activation CAS, leases, and resumable byte reclamation.
//! 显式插件准备、激活 CAS、租约及可恢复字节回收。

use super::{
    ACTIVATION_RECORD_VERSION, GenerationLease, GenerationRecord, GenerationState,
    PinnedVulcanPluginGeneration, RECORD_VERSION, STORE_DIRECTORY, VulcanPluginActivation,
    VulcanPluginActiveGeneration, VulcanPluginConfigurationSelection,
    VulcanPluginCredentialRevision, VulcanPluginGenerationIdentity, VulcanPluginGenerationStore,
    VulcanPluginPackageSelection, VulcanPluginSettings, VulcanPluginStoreError as Error,
    VulcanPluginStoreErrorCode as Code, VulcanPluginStoreResult as Result, files,
};
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use uuid::Uuid;

/// Actual outcomes from the sole marked-generation collection primitive.
/// 唯一有标记代际回收原语的实际结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum GenerationCollectionOutcome {
    /// This invocation completed deletion, sync and the actual Collected writer.
    /// 此次调用完成删除、同步及实际 Collected 写入。
    CollectedNow,
    /// Strict metadata already states Collected and actual package bytes are absent.
    /// 严格元数据已声明 Collected，且实际插件包字节缺失。
    AlreadyCollected,
}

/// The complete next selection, or a removal tombstone that retains the original cleanup identities.
/// 完整下一选择，或保留原清理身份的移除停用记录。
pub(in crate::generation_store) enum ActivationSuccessor {
    /// Select one verified package with its complete approval, staged credentials and configuration.
    /// 选择单个已验证插件包及其完整批准、已暂存凭据和配置。
    Select {
        /// Exact package identity.
        /// 精确插件包身份。
        generation: VulcanPluginGenerationIdentity,
        /// Complete sorted approval.
        /// 完整有序批准。
        approved_permissions: Vec<String>,
        /// Staged credential revision.
        /// 已暂存凭据修订。
        credential_revision: VulcanPluginCredentialRevision,
        /// Complete configuration selection.
        /// 完整配置选择。
        configuration: VulcanPluginConfigurationSelection,
    },
    /// Publish a tombstone after the current selection.
    /// 在当前选择之后发布停用记录。
    Deactivate,
}

/// Reject a package transaction over a bundled settings record; a bundled package is never selected by a transaction.
/// 拒绝针对内建设置记录的插件包交易；内建插件包从不由交易选择。
/// `original` is the current record of the namespace; returns unit when it may be succeeded.
/// `original` 为命名空间当前记录；可被后继时返回空值。
pub(in crate::generation_store) fn reject_bundled_predecessor(
    original: Option<&VulcanPluginActivation>,
) -> Result<()> {
    if original.is_some_and(|record| record.package == VulcanPluginPackageSelection::Bundled {}) {
        return Err(Error::new(
            Code::Conflict,
            "A bundled plugin namespace cannot be selected by a package transaction.",
        ));
    }
    Ok(())
}

impl VulcanPluginGenerationStore {
    /// Inspect `source` and approved digest, returning a fresh identity without copying or allocating directories.
    /// 检查 `source` 及已批准摘要，返回新身份，不复制或分配目录。
    pub(in crate::generation_store) fn plan_generation(
        &self,
        source: &Path,
        expected_content_fingerprint: &str,
    ) -> Result<VulcanPluginGenerationIdentity> {
        self.validate_layout()?;
        if !files::exists(source, true)? {
            return Err(Error::invalid("Generation source is missing."));
        }
        // Use the authoritative package validator before allocating a private generation.
        // 分配私有代次前使用权威插件包校验器。
        let original = crate::inspect_user_plugin_package(source).map_err(Error::invalid)?;
        if original.content_fingerprint != expected_content_fingerprint {
            return Err(Error::invalid(
                "Source changed after generation authorization inspection.",
            ));
        }
        // Freeze every identity field before copying; the copied package must match all of them.
        // 复制前固定全部身份字段；复制后的插件包必须全部匹配。
        let identity = VulcanPluginGenerationIdentity {
            plugin_id: original.id.clone(),
            generation_id: Uuid::new_v4().to_string(),
            version: original
                .version
                .clone()
                .ok_or_else(|| Error::invalid("Validated generation has no version."))?,
            content_fingerprint: original.content_fingerprint.clone(),
        };
        files::validate_identity(&identity)?;
        Ok(identity)
    }

    /// Copy `source` into its original reserved `identity`; return actual ownership or retained failure evidence.
    /// 将 `source` 复制到原预留 `identity`；返回实际所有权或保留失败证据。
    /// Existing directories are never reused, repaired, or replaced by this operation.
    /// 此操作绝不复用、修复或替换既有目录。
    pub(in crate::generation_store) fn prepare_planned_generation(
        &self,
        source: &Path,
        identity: &VulcanPluginGenerationIdentity,
    ) -> Result<PinnedVulcanPluginGeneration> {
        self.validate_layout()?;
        files::validate_identity(identity)?;
        if !files::exists(source, true)? {
            return Err(Error::invalid("Generation source is missing."));
        }
        // Reinspect this exact approved source before allocating the originally reserved directory.
        // 分配原预留目录前，重新检查此精确已批准来源。
        let original = crate::inspect_user_plugin_package(source).map_err(Error::invalid)?;
        if original.id != identity.plugin_id
            || original.version.as_deref() != Some(identity.version.as_str())
            || original.content_fingerprint != identity.content_fingerprint
        {
            return Err(Error::invalid(
                "Source differs from the original reserved generation.",
            ));
        }
        // A generation lease is acquired before publishing any completion marker.
        // 在发布任何完成标记前取得代次租约。
        let lease = self.generation_lock(identity, true)?;
        // Only unique generation directories are allocated; existing package bytes are never overwritten.
        // 仅分配唯一代次目录；从不覆盖既有插件包字节。
        let plugin_root = files::ensure_child(&self.root.join("packages"), &identity.plugin_id)?;
        // Exclusive directory creation rejects UUID collisions without reusing an existing candidate.
        // 排他创建目录，在 UUID 冲突时拒绝，不复用既有候选。
        let generation_root = plugin_root.join(&identity.generation_id);
        fs::create_dir(&generation_root)
            .map_err(|error| Error::io("Create unique package generation", error))?;
        // The final package directory already has its immutable location and validated package basename.
        // 最终插件包目录已具有不可变位置和已校验包目录名。
        let package_root = generation_root.join(&identity.plugin_id);
        // Retain incomplete files after failure so repair cannot confuse them with a prepared generation.
        // 失败后保留不完整文件，避免修复将其与已准备代次混淆。
        let prepared = (|| {
            #[cfg(test)]
            self.observe_package_preparation_checkpoint_for_test(
                identity,
                super::VulcanPluginGenerationPhase::Preparing,
            )?;
            // Persist identity before any package copy so interrupted preparation has an explicit cleanup owner.
            // 复制任何插件内容前持久保存身份，使被中断准备具有显式清理所有者。
            files::write_record(
                &generation_root.join("generation.json"),
                &GenerationRecord {
                    schema_version: RECORD_VERSION,
                    identity: identity.clone(),
                    state: GenerationState::Preparing,
                },
            )?;
            files::copy_synced(&original.package_root, &package_root)?;
            // Permission and credential derivation use the verified copied package, not the earlier source observation.
            // 权限及凭据派生使用已验证复制包，不使用较早来源观察。
            let verified_metadata = self.verify_package(&package_root, identity)?;
            files::sync_directory(&generation_root)?;
            files::sync_directory(&plugin_root)?;
            files::sync_directory(&self.root.join("packages"))?;
            files::write_record(
                &generation_root.join("generation.json"),
                &GenerationRecord {
                    schema_version: RECORD_VERSION,
                    identity: identity.clone(),
                    state: GenerationState::Prepared,
                },
            )?;
            #[cfg(test)]
            self.observe_package_preparation_checkpoint_for_test(
                identity,
                super::VulcanPluginGenerationPhase::Prepared,
            )?;
            Ok(verified_metadata)
        })();
        // Keep the verified permission and credential metadata only after complete preparation durably succeeds.
        // 仅完整准备持久成功后保留已验证权限及凭据元数据。
        let verified_metadata = prepared.map_err(|error: Error| {
            // Return the original candidate identity instead of forcing the host to parse a path from text.
            // 返回原候选身份，避免宿主从文本解析路径。
            let mut retained = Error::new(
                error.code,
                format!(
                    "{}; generation evidence retained at `{}`",
                    error.message,
                    generation_root.display()
                ),
            );
            retained.retained_generation = Some(Box::new(identity.clone()));
            retained
        })?;
        Ok(PinnedVulcanPluginGeneration {
            store_root: self.root.clone(),
            identity: identity.clone(),
            package_root,
            required_permissions: verified_metadata.required_permissions,
            credential_keys: verified_metadata.credential_keys,
            configuration: verified_metadata.configuration,
            _lease: Arc::new(GenerationLease { _lock: lease }),
        })
    }

    /// Read the persisted phase for the exact `identity` without treating it as a live package lease.
    /// 读取精确 `identity` 的持久阶段，不将其视为存活插件包租约。
    /// Returns declared state even for an incomplete or collected generation; malformed identities remain errors.
    /// 即使代次尚未完整或已回收，也返回声明状态；损坏身份仍返回错误。
    pub fn generation_phase(
        &self,
        identity: &VulcanPluginGenerationIdentity,
    ) -> Result<super::VulcanPluginGenerationPhase> {
        // The namespace fence coordinates metadata reads with activation and collection.
        // 命名空间屏障使元数据读取与激活及回收协调。
        let _namespace = self.namespace_lock(&identity.plugin_id, true)?;
        self.read_generation(identity)
            .map(|(_, record)| record.state)
    }

    /// Open storage below the existing host-selected `user_root`; creates only its private controlled children.
    /// 在现有宿主选定 `user_root` 下打开存储；仅创建其私有受控子目录。
    /// Returns a canonical store, rejecting redirected or missing roots without searching alternate paths.
    /// 返回规范存储；拒绝重定向或缺失根目录，不搜索替代路径。
    pub fn open(user_root: &Path) -> Result<Self> {
        if !files::exists(user_root, true)? {
            return Err(Error::invalid("User plugin root must already exist."));
        }
        // All later handles bind this one canonical storage location.
        // 后续所有句柄均绑定此唯一规范存储位置。
        let user_root = user_root
            .canonicalize()
            .map_err(|error| Error::io("Resolve user plugin root", error))?;
        // Private layout is independent of distributable package names and files.
        // 私有布局独立于可分发插件包名称及文件。
        let root = files::ensure_child(&user_root, STORE_DIRECTORY)?;
        for directory in ["packages", "active", "locks"] {
            files::ensure_child(&root, directory)?;
        }
        files::sync_directory(&root)?;
        files::sync_directory(&user_root)?;
        Ok(Self { root })
    }

    /// Prepare `source` as a new immutable generation matching the explicitly approved content digest.
    /// 将 `source` 准备为与明确批准内容摘要匹配的新不可变代次。
    /// Returns a real shared lease without changing activation, authorization, configuration, or runtime routes.
    /// 返回实际共享租约，不改变激活、授权、配置或运行时路由。
    pub fn prepare(
        &self,
        source: &Path,
        expected_content_fingerprint: &str,
    ) -> Result<PinnedVulcanPluginGeneration> {
        // Legacy callers allocate one fresh identity and use the same immutable copy implementation.
        // 旧调用方分配一个新身份并使用相同不可变复制实现。
        let identity = self.plan_generation(source, expected_content_fingerprint)?;
        self.prepare_planned_generation(source, &identity)
    }

    /// Pin a previously prepared `identity`; returns a verified root protected from collection.
    /// 固定先前已准备的 `identity`；返回受回收保护的已验证根目录。
    pub fn pin(
        &self,
        identity: &VulcanPluginGenerationIdentity,
    ) -> Result<PinnedVulcanPluginGeneration> {
        // Namespace ownership closes the race between selecting a generation and acquiring its real lease.
        // 命名空间所有权封闭选择代次与取得实际租约之间的竞态。
        let _namespace = self.namespace_lock(&identity.plugin_id, true)?;
        self.pin_locked(identity)
    }

    /// Read the exact current activation record for `plugin_id`; absence means revision zero only.
    /// 读取 `plugin_id` 的精确当前激活记录；缺失仅表示修订零。
    pub fn activation(&self, plugin_id: &str) -> Result<Option<VulcanPluginActivation>> {
        // Readers coordinate with writers through the same real namespace lock.
        // 读取者经相同实际命名空间锁与写入者协调。
        let _namespace = self.namespace_lock(plugin_id, true)?;
        self.read_activation(plugin_id)
    }

    /// Read activation and acquire its package lease atomically with respect to store writers.
    /// 相对于存储写入者，原子读取激活记录并取得其插件包租约。
    /// Returns the record and optional generation; an explicit tombstone never falls back to an older package.
    /// 返回记录及可选代次；显式停用记录从不回退到旧插件包。
    pub fn pin_active(&self, plugin_id: &str) -> Result<VulcanPluginActiveGeneration> {
        // The namespace fence remains held through acquiring the selected shared generation lease.
        // 命名空间屏障持续持有至所选共享代次租约取得。
        let _namespace = self.namespace_lock(plugin_id, true)?;
        self.pin_active_locked(plugin_id)
    }

    /// Pin the exact selected activation for `plugin_id` under the caller's already retained namespace fence.
    /// 在调用方已保留命名空间屏障内，固定 `plugin_id` 的精确已选激活。
    /// Return full activation and actual package/credential leases after all existing approval and configuration checks.
    /// 全部既有批准及配置检查后，返回完整激活及实际插件包、凭据租约。
    /// A guard caller must validate its complete original before entering; this helper never opens another namespace File.
    /// 守卫调用方进入前须校验完整原请求；此辅助方法绝不另开命名空间 File。
    pub(super) fn pin_active_locked(
        &self,
        plugin_id: &str,
    ) -> Result<VulcanPluginActiveGeneration> {
        // The exact on-disk choice is the sole source of generation selection.
        // 精确磁盘选择是代次选择的唯一来源。
        let activation = self.read_activation(plugin_id)?;
        // Optionality represents absence or a declared tombstone, never guessed recovery.
        // 可选性表示缺失或声明的停用记录，不表示猜测恢复。
        let generation = activation
            .as_ref()
            .and_then(|record| record.generation())
            .map(|identity| self.pin_locked(identity))
            .transpose()?;
        // A selected package must agree with every authority frozen beside it.
        // 所选插件包必须与其旁固定的每个权威一致。
        let credentials = match (activation.as_ref(), generation.as_ref()) {
            (Some(record), Some(package)) => {
                if record.approved_permissions() != package.required_permissions {
                    return Err(Error::invalid(
                        "Selected approval differs from the exact package capability set.",
                    ));
                }
                // Ownership must be acquired before the same selection fence is released.
                // 必须在相同选择屏障释放前取得所有权。
                let credentials =
                    self.pin_credentials_locked(package, record.generation_credential_revision()?)?;
                package.validate_configuration(record.configuration())?;
                Some(credentials)
            }
            _ => None,
        };
        Ok(VulcanPluginActiveGeneration {
            activation,
            generation,
            credentials,
        })
    }

    /// Atomically select `generation`, exact permissions, staged credentials and `configuration` after `expected_revision`.
    /// 在 `expected_revision` 之后原子选择 `generation`、精确权限、已暂存凭据及 `configuration`。
    /// The host must first authorize, validate readiness, drain old writers, and prepare its route transaction.
    /// 宿主必须先授权、验证就绪、排空旧写入者并准备路由事务。
    /// Return the committed selection; errors require original-record inspection and never imply application readiness.
    /// 返回已提交选择；错误要求检查原记录，且绝不表示应用就绪。
    pub fn activate_configured(
        &self,
        generation: &PinnedVulcanPluginGeneration,
        expected_revision: u64,
        mut approved_permissions: Vec<String>,
        credential_revision: VulcanPluginCredentialRevision,
        configuration: VulcanPluginConfigurationSelection,
    ) -> Result<VulcanPluginActivation> {
        if generation.store_root != self.root {
            return Err(Error::invalid(
                "Generation handle belongs to a different store.",
            ));
        }
        // The original namespace fence orders package verification, approval and selection together.
        // 原命名空间屏障共同排序插件包校验、批准及选择。
        let _namespace = self.namespace_lock(&generation.identity.plugin_id, false)?;
        // Reinspection binds approval to the physical contents still owned by this exact lease.
        // 重新检查将批准绑定至此精确租约仍持有的物理内容。
        let verified = self.pin_locked(&generation.identity)?;
        approved_permissions.sort();
        if approved_permissions != verified.required_permissions {
            return Err(Error::invalid(
                "Activation must approve the complete exact capability set once, without duplicates.",
            ));
        }
        self.validate_staged_credentials_locked(&verified, &credential_revision)?;
        verified.validate_configuration(&configuration)?;
        self.replace_activation(
            &generation.identity.plugin_id,
            expected_revision,
            ActivationSuccessor::Select {
                generation: generation.identity.clone(),
                approved_permissions,
                credential_revision,
                configuration,
            },
        )
    }

    /// Publish an explicit deactivation tombstone at `expected_revision`, retaining monotonic history.
    /// 在 `expected_revision` 发布显式停用记录，保留单调历史。
    /// Returns the successor without deleting package bytes or releasing another owner's lease.
    /// 返回后继，不删除插件包字节，也不释放其他所有者的租约。
    pub fn deactivate(
        &self,
        plugin_id: &str,
        expected_revision: u64,
    ) -> Result<VulcanPluginActivation> {
        // Use the same fence and revision semantics as activation.
        // 使用与激活相同的屏障及修订语义。
        let _namespace = self.namespace_lock(plugin_id, false)?;
        self.replace_activation(
            plugin_id,
            expected_revision,
            ActivationSuccessor::Deactivate,
        )
    }

    /// Collect unselected, unpinned package bytes for `identity`; returns false when already collected.
    /// 回收 `identity` 未被选定且未被固定的插件包字节；已回收时返回假。
    /// Cleanup intent is durable before deletion; failed cleanup retains its identity for explicit retry.
    /// 删除前持久保存清理意图；清理失败保留身份供显式重试。
    pub fn collect(&self, identity: &VulcanPluginGenerationIdentity) -> Result<bool> {
        // Namespace first, then generation lock is the common ordering for all ownership changes.
        // 先命名空间、后代次锁，是所有所有权变更的共同顺序。
        let _namespace = self.namespace_lock(&identity.plugin_id, false)?;
        // The same exclusive generation guard spans the sole deletion primitive.
        // 同一排他代际守卫覆盖唯一删除原语。
        let _generation = self.lock_generation_for_collection_locked(identity)?;
        Ok(self.collect_generation_locked(identity)? == GenerationCollectionOutcome::CollectedNow)
    }

    /// Validate exact identity retention and selected-package exclusion under the caller-held namespace.
    /// 在调用方持有命名空间内校验精确身份保留及已选插件包排除。
    /// Return unit without allocating a generation lock, permitting strictly read-only missing-directory observation.
    /// 不分配代际锁便返回空值，允许严格只读缺失目录观察。
    pub(in crate::generation_store) fn check_generation_collection_admission_locked(
        &self,
        identity: &VulcanPluginGenerationIdentity,
    ) -> Result<()> {
        files::validate_identity(identity)?;
        self.check_transaction_package_retention(identity)?;
        // An active generation cannot be collected even when no current process holds a package lease.
        // 即使当前没有进程持有插件租约，也不能回收激活代次。
        let active = self.read_activation(&identity.plugin_id)?;
        if active
            .as_ref()
            .and_then(|record| record.generation())
            .is_some_and(|selected| selected.generation_id == identity.generation_id)
        {
            return Err(Error::new(
                Code::Busy,
                "Selected plugin generation cannot be collected.",
            ));
        }
        Ok(())
    }

    /// Retain the actual exclusive generation File after common retention and selection checks.
    /// 在共同保留及选择检查后保留实际排他代际 File。
    /// Return the guard to retain through deletion, synchronization and final readback.
    /// 返回须跨删除、同步及最终回读保留的守卫。
    pub(in crate::generation_store) fn lock_generation_for_collection_locked(
        &self,
        identity: &VulcanPluginGenerationIdentity,
    ) -> Result<File> {
        self.check_generation_collection_admission_locked(identity)?;
        // Every process and cloned pin must release before this real exclusive lock can succeed.
        // 此实际排他锁成功前，每个进程及克隆固定句柄均须释放。
        self.generation_lock(identity, false)
    }

    /// Collect the exact generation using strict existing metadata under caller-held namespace and generation guards.
    /// 在调用方持有命名空间及代际守卫下，使用严格既有元数据回收精确代际。
    /// Return a completed Collected transition or already Collected evidence; missing or malformed markers remain errors.
    /// 返回已完成 Collected 转换或已 Collected 证据；缺失、损坏标记仍属于错误。
    pub(in crate::generation_store) fn collect_generation_locked(
        &self,
        identity: &VulcanPluginGenerationIdentity,
    ) -> Result<GenerationCollectionOutcome> {
        self.collect_generation_observed_locked(identity, |_| Ok(()), |_| {})
    }

    /// Collect through the sole producer while validating full actual records and recording acknowledged phase checkpoints.
    /// 通过唯一生产者回收，同时验证完整实际记录并记录已确认阶段检查点。
    /// identity fixes the physical namespace; validate rejects regressions and observe retains nonfallible same-original evidence.
    /// identity 固定物理命名空间；validate 拒绝回退，observe 保留不可失败同原实例证据。
    /// Returns actual current/prior collection; caller keeps the same physical namespace/generation exclusion throughout.
    /// 返回实际当前／先前回收；调用方全程保持相同物理命名空间／代次互斥。
    pub(in crate::generation_store) fn collect_generation_observed_locked(
        &self,
        identity: &VulcanPluginGenerationIdentity,
        validate: impl Fn(&GenerationRecord) -> Result<()>,
        observe: impl Fn(GenerationState),
    ) -> Result<GenerationCollectionOutcome> {
        // Keep the identity marker even after content reclamation, including interrupted cleanup.
        // 内容回收后仍保留身份标记，包括被中断的清理。
        let (generation_root, mut record) = self.read_generation(identity)?;
        validate(&record)?;
        observe(record.state);
        // Only this identity-derived package child may be removed.
        // 仅可删除由此身份派生的插件包子目录。
        let package_root = generation_root.join(&identity.plugin_id);
        if record.state == GenerationState::Collected {
            if files::exists(&package_root, true)? {
                return Err(Error::invalid(
                    "Collected generation unexpectedly contains package bytes.",
                ));
            }
            return Ok(GenerationCollectionOutcome::AlreadyCollected);
        }
        if record.state != GenerationState::Collecting {
            if record.state == GenerationState::Prepared {
                self.verify_package(&package_root, identity)?;
            }
            record.state = GenerationState::Collecting;
            files::write_record(&generation_root.join("generation.json"), &record)?;
            observe(record.state);
        }
        if files::exists(&package_root, true)? {
            fs::remove_dir_all(&package_root).map_err(|error| {
                Error::io(
                    "Collect retired package bytes; explicit retry is required",
                    error,
                )
            })?;
            #[cfg(test)]
            self.observe_package_collection_checkpoint_for_test(
                identity,
                GenerationState::Collecting,
            )?;
        }
        files::sync_directory(&generation_root)?;
        record.state = GenerationState::Collected;
        files::write_record(&generation_root.join("generation.json"), &record)?;
        observe(record.state);
        #[cfg(test)]
        self.observe_package_collection_checkpoint_for_test(identity, GenerationState::Collected)?;
        Ok(GenerationCollectionOutcome::CollectedNow)
    }

    /// Validate every fixed layout directory before dereferencing controlled descendants.
    /// 解引用受控后代前校验每个固定布局目录。
    pub(super) fn validate_layout(&self) -> Result<()> {
        for path in [
            &self.root,
            &self.root.join("packages"),
            &self.root.join("active"),
            &self.root.join("locks"),
        ] {
            if !files::exists(path, true)? {
                return Err(Error::invalid("Generation store layout is incomplete."));
            }
        }
        Ok(())
    }

    /// Acquire a namespace fence after validating `plugin_id`; `shared` selects readers instead of mutations.
    /// 校验 `plugin_id` 后取得命名空间屏障；`shared` 为真时选择读取锁，否则选择变更锁。
    /// Returns an owning file handle; shared readers coexist while activation and collection remain exclusive.
    /// 返回拥有锁的文件句柄；共享读取者可以共存，激活及回收继续保持排他。
    pub(super) fn namespace_lock(&self, plugin_id: &str, shared: bool) -> Result<File> {
        crate::catalog::validate_identifier(plugin_id, "generation plugin id")
            .map_err(Error::invalid)?;
        self.validate_layout()?;
        files::lock(
            &self.root.join("locks").join(format!("{plugin_id}.lock")),
            shared,
        )
    }

    /// Acquire a shared package lease or exclusive reclamation lease for the exact validated identity.
    /// 为精确已校验身份取得共享插件租约或排他回收租约。
    fn generation_lock(
        &self,
        identity: &VulcanPluginGenerationIdentity,
        shared: bool,
    ) -> Result<File> {
        files::validate_identity(identity)?;
        self.validate_layout()?;
        files::lock(
            &self.root.join("locks").join(format!(
                "{}-{}.lease",
                identity.plugin_id, identity.generation_id
            )),
            shared,
        )
    }

    /// Read and validate one record against its caller-selected namespace; returns declared absence only.
    /// 根据调用方选定命名空间读取并校验记录；仅返回声明的缺失。
    pub(super) fn read_activation(
        &self,
        plugin_id: &str,
    ) -> Result<Option<VulcanPluginActivation>> {
        // Bound the actual input before decoding; older versions retain their original smaller envelope.
        // 解码前限制实际输入；旧版本保留原来较小的记录封套。
        let Some(bytes) = files::read_bytes(
            &self.root.join("active").join(format!("{plugin_id}.json")),
            super::MAX_ACTIVATION_BYTES,
        )?
        else {
            return Ok(None);
        };
        // Decoder diagnostics cannot echo potentially sensitive configuration values or unknown property names.
        // 解码诊断不能回显可能敏感的配置值或未知属性名称。
        let record: VulcanPluginActivation = serde_json::from_slice(&bytes)
            .map_err(|_| Error::invalid("Invalid plugin activation metadata."))?;
        Self::validate_activation_record(&record, plugin_id, bytes.len() as u64)?;
        Ok(Some(record))
    }

    /// Validate `record` for `plugin_id` and its actual `encoded_size`; journals reuse the same format authority.
    /// 按 `plugin_id` 及实际 `encoded_size` 校验 `record`；事务日志复用相同格式权威。
    pub(super) fn validate_activation_record(
        record: &VulcanPluginActivation,
        plugin_id: &str,
        encoded_size: u64,
    ) -> Result<()> {
        if encoded_size > super::MAX_ACTIVATION_BYTES {
            return Err(Error::invalid(
                "Activation metadata exceeds its encoded byte limit.",
            ));
        }
        if record.plugin_id != plugin_id || record.revision == 0 {
            return Err(Error::invalid(
                "Activation metadata has an unknown version or mismatched identity.",
            ));
        }
        if record.schema_version != ACTIVATION_RECORD_VERSION {
            return Err(Error::invalid(
                "Activation metadata has an unknown version or mismatched identity.",
            ));
        }
        let permissions = &record.approved_permissions;
        if permissions.windows(2).any(|pair| pair[0] >= pair[1])
            || permissions
                .iter()
                .any(|key| key.is_empty() || key.contains('\0'))
            || (record.generation().is_none() && !permissions.is_empty())
        {
            return Err(Error::invalid("Activation approval is not canonical."));
        }
        if record.settings.revision == 0 {
            return Err(Error::invalid("Activation settings have no revision."));
        }
        // Managed generations always carry staged credentials; bundled packages have none in this store.
        // 受管代次始终携带已暂存凭据；内建插件包在此存储中没有凭据。
        let credentials_match_package = match &record.package {
            VulcanPluginPackageSelection::Generation { .. } => {
                record.credential_revision().is_some()
            }
            VulcanPluginPackageSelection::Bundled {} => record.credential_revision().is_none(),
            VulcanPluginPackageSelection::Uninstalled {} => true,
        };
        if !credentials_match_package {
            return Err(Error::invalid(
                "Activation credential revision does not match its package kind.",
            ));
        }
        record.configuration().validate_size()?;
        if let Some(identity) = record.generation() {
            files::validate_identity(identity)?;
            if identity.plugin_id != plugin_id {
                return Err(Error::invalid(
                    "Activation points to a different plugin namespace.",
                ));
            }
        }
        Ok(())
    }

    /// Replace activation under the existing namespace lease; returns a checked monotonic successor.
    /// 在现有命名空间租约下替换激活记录；返回经检查的单调后继。
    fn replace_activation(
        &self,
        plugin_id: &str,
        expected_revision: u64,
        successor: ActivationSuccessor,
    ) -> Result<VulcanPluginActivation> {
        // Direct writers cannot bypass an unfinished durable handoff.
        // 直接写入者不能绕过尚未完成的持久交接。
        self.ensure_no_pending_activation_locked(plugin_id)?;
        // Absence is explicitly the never-published predecessor; tombstones are always retained records.
        // 缺失显式表示从未发布的前驱；停用状态始终保留为记录。
        let current = self.read_activation(plugin_id)?;
        if current.as_ref().map_or(0, |record| record.revision) != expected_revision {
            return Err(Error::new(
                Code::Conflict,
                "Plugin activation revision changed; inspect the current record.",
            ));
        }
        // Freeze the same complete successor before its sole actual publication.
        // 在唯一实际发布之前固定相同完整后继。
        let next = self.plan_activation_successor(plugin_id, current.as_ref(), successor)?;
        self.write_activation(&next)?;
        Ok(next)
    }

    /// Derive one complete successor from the exact original without filesystem effects or allocating another revision later.
    /// 从精确原记录派生单个完整后继，不产生文件系统效果或随后分配另一修订。
    /// plugin_id and original identify the namespace; successor declares the exact next selection or removal.
    /// plugin_id 及 original 标识命名空间；successor 声明精确下一选择或移除。
    /// Returns the sole checked successor; removal retains the original credential and configuration for cleanup.
    /// 返回唯一经检查后继；移除保留原凭据及配置以供清理。
    pub(in crate::generation_store) fn plan_activation_successor(
        &self,
        plugin_id: &str,
        original: Option<&VulcanPluginActivation>,
        successor: ActivationSuccessor,
    ) -> Result<VulcanPluginActivation> {
        reject_bundled_predecessor(original)?;
        // The original validated selection is the only source of the next monotonic revision.
        // 原已校验选择是下一单调修订的唯一来源。
        let expected_revision = original.map_or(0, |record| record.revision);
        // Check overflow before writing; revision zero is never recycled.
        // 写入前检查溢出；从不复用修订零。
        let revision = expected_revision
            .checked_add(1)
            .ok_or_else(|| Error::invalid("Plugin activation revision is exhausted."))?;
        // A commit is also a settings change, so both halves advance together.
        // 提交同时也是设置变更，因此两部分一同递增。
        let settings_revision = VulcanPluginActivation::next_settings_revision(original)?;
        // This single record is the storage-level selection authority, not a claim of route readiness.
        // 此单一记录为存储层选择权威，不表示路由已就绪。
        match successor {
            ActivationSuccessor::Select {
                generation,
                approved_permissions,
                credential_revision,
                configuration,
            } => Ok(VulcanPluginActivation::generation_selection(
                plugin_id.to_owned(),
                revision,
                generation,
                approved_permissions,
                settings_revision,
                configuration,
                credential_revision,
            )),
            ActivationSuccessor::Deactivate => {
                // A tombstone needs a published original whose cleanup identities it retains.
                // 停用记录需要已发布原记录，并保留其清理身份。
                let original = original.ok_or_else(|| {
                    Error::new(
                        Code::Conflict,
                        "Plugin deactivation requires a published selection.",
                    )
                })?;
                Ok(VulcanPluginActivation {
                    schema_version: ACTIVATION_RECORD_VERSION,
                    plugin_id: plugin_id.to_owned(),
                    revision,
                    package: VulcanPluginPackageSelection::Uninstalled {},
                    approved_permissions: Vec::new(),
                    settings: VulcanPluginSettings {
                        revision: settings_revision,
                        ..original.settings.clone()
                    },
                })
            }
        }
    }

    /// Publish the prevalidated `next` selection while its caller retains the namespace fence.
    /// 调用方保留命名空间屏障时发布已预先校验的 `next` 选择。
    /// Return only after the common atomic writer succeeds; errors require exact record readback.
    /// 仅在共同原子写入器成功后返回；错误需要精确回读记录。
    pub(super) fn write_activation(&self, next: &VulcanPluginActivation) -> Result<()> {
        // The selected document shares exactly one atomic publication with all other activation authorities.
        // 所选文档与其他全部激活权威共用唯一原子发布。
        let path = self
            .root
            .join("active")
            .join(format!("{}.json", next.plugin_id));
        let bytes = serde_json::to_vec(next)
            .map_err(|_| Error::invalid("Cannot encode plugin activation."))?;
        files::write_encoded_record(&path, &bytes, super::MAX_ACTIVATION_BYTES)
    }

    /// Read a generation state record and compare every identity field before returning its root.
    /// 读取代次状态记录，返回其根目录前比较全部身份字段。
    pub(in crate::generation_store) fn read_generation(
        &self,
        identity: &VulcanPluginGenerationIdentity,
    ) -> Result<(PathBuf, GenerationRecord)> {
        files::validate_identity(identity)?;
        // Existing selected readers and removal enumeration share the sole canonical marker reader.
        // 既有已选读取者及移除枚举共享唯一规范标记读取器。
        let (generation_root, record) =
            self.read_generation_directory_locked(&identity.plugin_id, &identity.generation_id)?;
        if record.identity != *identity {
            return Err(Error::invalid(
                "Package generation record does not match the exact requested identity.",
            ));
        }
        Ok((generation_root, record))
    }

    /// Read one canonical generation directory's actual marker under the caller's retained namespace without guessing identity fields.
    /// 在调用方保留命名空间内读取单个规范代次目录的实际标记，不猜测身份字段。
    /// plugin_id/generation_id are exact directory names; returns the validated complete producer record and ordinary root.
    /// plugin_id／generation_id 为精确目录名；返回已验证完整生产者记录及普通根。
    pub(in crate::generation_store) fn read_generation_directory_locked(
        &self,
        plugin_id: &str,
        generation_id: &str,
    ) -> Result<(PathBuf, GenerationRecord)> {
        crate::catalog::validate_identifier(plugin_id, "generation plugin id")
            .map_err(Error::invalid)?;
        // Canonical UUID text is checked before any directory is dereferenced.
        // 解引用任何目录之前检查规范 UUID 文本。
        let uuid = uuid::Uuid::parse_str(generation_id)
            .map_err(|_| Error::invalid("Invalid package generation UUID."))?;
        if uuid.to_string() != generation_id {
            return Err(Error::invalid(
                "Package generation directory name is not canonical.",
            ));
        }
        self.validate_layout()?;
        // Inspect each identity-derived component before opening metadata or package contents.
        // 打开元数据或插件内容前检查每个身份派生组件。
        let plugin_root = self.root.join("packages").join(plugin_id);
        if !files::exists(&plugin_root, true)? {
            return Err(Error::invalid("Plugin generation namespace is missing."));
        }
        // The generation root contains metadata beside, never inside, distributable package bytes.
        // 代次根目录在可分发插件字节旁保存元数据，不将其写入插件包。
        let generation_root = plugin_root.join(generation_id);
        if !files::exists(&generation_root, true)? {
            return Err(Error::invalid("Plugin generation directory is missing."));
        }
        // A missing or malformed state record cannot be inferred from the presence of copied package files.
        // 不得从复制插件文件的存在推断缺失或损坏的状态记录。
        let record: GenerationRecord =
            files::read_record(&generation_root.join("generation.json"))?.ok_or_else(|| {
                Error::invalid("Package generation has no valid identity state record.")
            })?;
        files::validate_identity(&record.identity)?;
        if record.schema_version != RECORD_VERSION
            || record.identity.plugin_id != plugin_id
            || record.identity.generation_id != generation_id
        {
            return Err(Error::invalid(
                "Package generation record does not match the exact requested identity.",
            ));
        }
        Ok((generation_root, record))
    }

    /// Acquire and verify a generation while the caller retains its namespace lease.
    /// 调用方保持命名空间租约期间取得并验证代次。
    pub(super) fn pin_locked(
        &self,
        identity: &VulcanPluginGenerationIdentity,
    ) -> Result<PinnedVulcanPluginGeneration> {
        // Hold the actual shared lock through marker and package validation and transfer it to the result.
        // 在标记及插件包校验期间持有实际共享锁，并将其转移至结果。
        let lease = self.generation_lock(identity, true)?;
        // Prepared is the only state eligible for new use; collection never rolls back to prepared.
        // 仅已准备状态可以重新使用；回收从不退回已准备状态。
        let (generation_root, record) = self.read_generation(identity)?;
        if record.state != GenerationState::Prepared {
            return Err(Error::invalid(
                "Plugin generation is not prepared, is retiring, or is already collected.",
            ));
        }
        // The physical package remains fixed for the complete lifetime of the returned lease.
        // 在返回租约的整个生命周期内，物理插件包保持固定。
        let package_root = generation_root.join(&identity.plugin_id);
        // The same verified manifest supplies approval and credential keys to publication and subsequent reads.
        // 同一已验证清单向发布及后续读取提供批准及凭据键。
        let verified_metadata = self.verify_package(&package_root, identity)?;
        Ok(PinnedVulcanPluginGeneration {
            store_root: self.root.clone(),
            identity: identity.clone(),
            package_root,
            required_permissions: verified_metadata.required_permissions,
            credential_keys: verified_metadata.credential_keys,
            configuration: verified_metadata.configuration,
            _lease: Arc::new(GenerationLease { _lock: lease }),
        })
    }

    /// Validate `package_root` using the authoritative manifest loader and compare exact immutable contents.
    /// 使用权威清单加载器校验 `package_root`，并比较精确不可变内容。
    /// Returns canonical permission and credential metadata bound to the same verified `identity`.
    /// 返回绑定至相同已验证 `identity` 的规范权限及凭据元数据。
    fn verify_package(
        &self,
        package_root: &Path,
        identity: &VulcanPluginGenerationIdentity,
    ) -> Result<super::VerifiedPackageMetadata> {
        if !files::exists(package_root, true)? {
            return Err(Error::invalid(
                "Prepared generation package bytes are missing.",
            ));
        }
        // One authoritative inspection supplies byte identity, permissions and complete declared credential keys.
        // 单次权威检查提供字节身份、权限及完整声明凭据键。
        let descriptor =
            crate::inspect_user_plugin_package(package_root).map_err(Error::invalid)?;
        if descriptor.id != identity.plugin_id
            || descriptor.version.as_deref() != Some(identity.version.as_str())
            || descriptor.content_fingerprint != identity.content_fingerprint
        {
            return Err(Error::invalid(
                "Prepared generation contents changed or do not match their frozen identity.",
            ));
        }
        Ok(super::VerifiedPackageMetadata {
            required_permissions: crate::runtime::plugin_required_permissions(&descriptor)
                .map_err(|error| Error::invalid(error.to_string()))?,
            credential_keys: super::credential_staging::declared_keys(&descriptor),
            configuration: descriptor
                .manifest
                .as_ref()
                .ok_or_else(|| Error::invalid("Verified package has no manifest."))?
                .config
                .clone(),
        })
    }
}
