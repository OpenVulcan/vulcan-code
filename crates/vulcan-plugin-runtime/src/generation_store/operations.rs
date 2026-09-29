//! Explicit package preparation, activation CAS, leases, and resumable byte reclamation.
//! 显式插件准备、激活 CAS、租约及可恢复字节回收。

use super::{
    AUTHORIZED_ACTIVATION_VERSION, CONFIGURED_ACTIVATION_VERSION, CREDENTIAL_ACTIVATION_VERSION,
    GenerationLease, GenerationRecord, GenerationState, PinnedVulcanPluginGeneration,
    RECORD_VERSION, STORE_DIRECTORY, VulcanPluginActivation, VulcanPluginActiveGeneration,
    VulcanPluginConfigurationSelection, VulcanPluginCredentialRevision,
    VulcanPluginGenerationIdentity, VulcanPluginGenerationStore, VulcanPluginStoreError as Error,
    VulcanPluginStoreErrorCode as Code, VulcanPluginStoreResult as Result, files,
};
use std::fs::{self, File};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use uuid::Uuid;

impl VulcanPluginGenerationStore {
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
        // A generation lease is acquired before publishing any completion marker.
        // 在发布任何完成标记前取得代次租约。
        let lease = self.generation_lock(&identity, true)?;
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
            let verified_metadata = self.verify_package(&package_root, &identity)?;
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
            identity,
            package_root,
            required_permissions: verified_metadata.required_permissions,
            credential_keys: verified_metadata.credential_keys,
            configuration: verified_metadata.configuration,
            _lease: Arc::new(GenerationLease { _lock: lease }),
        })
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
        // The exact on-disk choice is the sole source of generation selection.
        // 精确磁盘选择是代次选择的唯一来源。
        let activation = self.read_activation(plugin_id)?;
        // Optionality represents absence or a declared tombstone, never guessed recovery.
        // 可选性表示缺失或声明的停用记录，不表示猜测恢复。
        let generation = activation
            .as_ref()
            .and_then(|record| record.generation.as_ref())
            .map(|identity| self.pin_locked(identity))
            .transpose()?;
        if let (Some(approval), Some(package)) = (
            activation
                .as_ref()
                .and_then(VulcanPluginActivation::approved_permissions),
            generation.as_ref(),
        ) && approval != package.required_permissions
        {
            return Err(Error::invalid(
                "Selected approval differs from the exact package capability set.",
            ));
        }
        // Ownership must be acquired before the same selection fence is released.
        // 必须在相同选择屏障释放前取得所有权。
        let credentials = if let (Some(revision), Some(package)) = (
            activation
                .as_ref()
                .and_then(VulcanPluginActivation::credential_revision),
            generation.as_ref(),
        ) {
            Some(self.pin_credentials_locked(package, revision)?)
        } else {
            None
        };
        if let (Some(configuration), Some(package)) = (
            activation
                .as_ref()
                .and_then(VulcanPluginActivation::configuration),
            generation.as_ref(),
        ) {
            package.validate_configuration(configuration)?;
        }
        Ok(VulcanPluginActiveGeneration {
            activation,
            generation,
            credentials,
        })
    }

    /// Select `generation` only when `expected_revision` is current; returns the exact successor record.
    /// 仅 `expected_revision` 为当前修订时选择 `generation`；返回精确后继记录。
    /// The host must first authorize, validate readiness, drain old writers, and prepare its route transaction.
    /// 宿主必须先授权、验证就绪、排空旧写入者并准备路由事务。
    pub fn activate(
        &self,
        generation: &PinnedVulcanPluginGeneration,
        expected_revision: u64,
    ) -> Result<VulcanPluginActivation> {
        if generation.store_root != self.root {
            return Err(Error::invalid(
                "Generation handle belongs to a different store.",
            ));
        }
        // Only the target plugin is fenced; copying and unrelated plugin work do not hold this lock.
        // 仅对目标插件建立屏障；复制及其他插件工作不持有此锁。
        let _namespace = self.namespace_lock(&generation.identity.plugin_id, false)?;
        // Revalidate physical contents before allowing any activation mutation.
        // 允许任何激活变更前重新校验物理内容。
        let _verified = self.pin_locked(&generation.identity)?;
        self.replace_activation(
            &generation.identity.plugin_id,
            expected_revision,
            Some(generation.identity.clone()),
            None,
            None,
            None,
        )
    }

    /// Atomically select `generation` and its complete `approved_permissions` at the exact predecessor revision.
    /// 在精确前驱修订原子选定 `generation` 及其完整 `approved_permissions`。
    /// Returns a version-two selection; callers must still coordinate credentials, readiness and route publication.
    /// 返回第二版选择；调用方仍须协调凭据、就绪及路由发布。
    pub fn activate_authorized(
        &self,
        generation: &PinnedVulcanPluginGeneration,
        expected_revision: u64,
        approved_permissions: Vec<String>,
    ) -> Result<VulcanPluginActivation> {
        self.activate_approved_selection(
            generation,
            expected_revision,
            approved_permissions,
            None,
            None,
        )
    }

    /// Publish one package, complete approval and exact `credential_revision` in the same successor record.
    /// 在同一后继记录发布单个插件包、完整批准及精确 `credential_revision`。
    /// `generation` owns verified bytes and `expected_revision` freezes the predecessor; returns the version-three selection.
    /// `generation` 持有已验证字节，`expected_revision` 固定前驱；返回第三版选择。
    /// The host must first finish secure staging and readiness; this primitive never copies or reads secret values.
    /// 宿主必须先完成安全暂存及就绪；此基础组件绝不复制或读取秘密值。
    pub fn activate_with_credentials(
        &self,
        generation: &PinnedVulcanPluginGeneration,
        expected_revision: u64,
        approved_permissions: Vec<String>,
        credential_revision: VulcanPluginCredentialRevision,
    ) -> Result<VulcanPluginActivation> {
        self.activate_approved_selection(
            generation,
            expected_revision,
            approved_permissions,
            Some(credential_revision),
            None,
        )
    }

    /// Validate explicit approval and declared credential/configuration authorities under the namespace fence before publication.
    /// 在命名空间屏障下校验显式批准及声明的凭据／配置权威，再进行发布。
    /// Inputs name the pinned package, exact predecessor and selected authority; returns the original write outcome.
    /// 输入指定固定插件包、精确前驱及选定权威；返回原写入结果。
    pub(super) fn activate_approved_selection(
        &self,
        generation: &PinnedVulcanPluginGeneration,
        expected_revision: u64,
        mut approved_permissions: Vec<String>,
        credential_revision: Option<VulcanPluginCredentialRevision>,
        configuration: Option<VulcanPluginConfigurationSelection>,
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
        if let Some(revision) = &credential_revision {
            self.validate_staged_credentials_locked(&verified, revision)?;
        }
        if let Some(selection) = &configuration {
            verified.validate_configuration(selection)?;
        }
        self.replace_activation(
            &generation.identity.plugin_id,
            expected_revision,
            Some(generation.identity.clone()),
            Some(approved_permissions),
            credential_revision,
            configuration,
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
        self.replace_activation(plugin_id, expected_revision, None, None, None, None)
    }

    /// Collect unselected, unpinned package bytes for `identity`; returns false when already collected.
    /// 回收 `identity` 未被选定且未被固定的插件包字节；已回收时返回假。
    /// Cleanup intent is durable before deletion; failed cleanup retains its identity for explicit retry.
    /// 删除前持久保存清理意图；清理失败保留身份供显式重试。
    pub fn collect(&self, identity: &VulcanPluginGenerationIdentity) -> Result<bool> {
        // Namespace first, then generation lock is the common ordering for all ownership changes.
        // 先命名空间、后代次锁，是所有所有权变更的共同顺序。
        let _namespace = self.namespace_lock(&identity.plugin_id, false)?;
        files::validate_identity(identity)?;
        self.check_transaction_package_retention(identity)?;
        // An active generation cannot be collected even when no current process holds a package lease.
        // 即使当前没有进程持有插件租约，也不能回收激活代次。
        let active = self.read_activation(&identity.plugin_id)?;
        if active
            .as_ref()
            .and_then(|record| record.generation.as_ref())
            .is_some_and(|selected| selected.generation_id == identity.generation_id)
        {
            return Err(Error::new(
                Code::Busy,
                "Selected plugin generation cannot be collected.",
            ));
        }
        // Every process and cloned pin must release before this real exclusive lock can succeed.
        // 此实际排他锁成功前，每个进程及克隆固定句柄均须释放。
        let _generation = self.generation_lock(identity, false)?;
        // Keep the identity marker even after content reclamation, including interrupted cleanup.
        // 内容回收后仍保留身份标记，包括被中断的清理。
        let (generation_root, mut record) = self.read_generation(identity)?;
        // Only this identity-derived package child may be removed.
        // 仅可删除由此身份派生的插件包子目录。
        let package_root = generation_root.join(&identity.plugin_id);
        if record.state == GenerationState::Collected {
            if files::exists(&package_root, true)? {
                return Err(Error::invalid(
                    "Collected generation unexpectedly contains package bytes.",
                ));
            }
            return Ok(false);
        }
        if record.state != GenerationState::Collecting {
            if record.state == GenerationState::Prepared {
                self.verify_package(&package_root, identity)?;
            }
            record.state = GenerationState::Collecting;
            files::write_record(&generation_root.join("generation.json"), &record)?;
        }
        if files::exists(&package_root, true)? {
            fs::remove_dir_all(&package_root).map_err(|error| {
                Error::io(
                    "Collect retired package bytes; explicit retry is required",
                    error,
                )
            })?;
        }
        files::sync_directory(&generation_root)?;
        record.state = GenerationState::Collected;
        files::write_record(&generation_root.join("generation.json"), &record)?;
        Ok(true)
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
        if record.schema_version != CONFIGURED_ACTIVATION_VERSION
            && encoded_size > super::MAX_RECORD_BYTES
        {
            return Err(Error::invalid(
                "Legacy activation metadata exceeds its encoded byte limit.",
            ));
        }
        match (
            record.schema_version,
            &record.approved_permissions,
            &record.credential_revision,
            &record.configuration,
        ) {
            (RECORD_VERSION, None, None, None) => {}
            (AUTHORIZED_ACTIVATION_VERSION, Some(permissions), None, None)
            | (CREDENTIAL_ACTIVATION_VERSION, Some(permissions), Some(_), None)
            | (CONFIGURED_ACTIVATION_VERSION, Some(permissions), Some(_), Some(_)) => {
                if permissions.windows(2).any(|pair| pair[0] >= pair[1])
                    || permissions
                        .iter()
                        .any(|key| key.is_empty() || key.contains('\0'))
                    || (record.generation.is_none() && !permissions.is_empty())
                {
                    return Err(Error::invalid("Activation approval is not canonical."));
                }
            }
            _ => {
                return Err(Error::invalid(
                    "Activation version, approval, credentials and configuration do not agree.",
                ));
            }
        }
        if let Some(selection) = &record.configuration {
            selection.validate_size()?;
        }
        if let Some(identity) = &record.generation {
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
        generation: Option<VulcanPluginGenerationIdentity>,
        approved_permissions: Option<Vec<String>>,
        credential_revision: Option<VulcanPluginCredentialRevision>,
        configuration: Option<VulcanPluginConfigurationSelection>,
    ) -> Result<VulcanPluginActivation> {
        // Direct legacy and configured writers cannot bypass an unfinished durable handoff.
        // 旧格式及配置直接写入者不能绕过尚未完成的持久交接。
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
        // Check overflow before writing; revision zero is never recycled.
        // 写入前检查溢出；从不复用修订零。
        let revision = expected_revision
            .checked_add(1)
            .ok_or_else(|| Error::invalid("Plugin activation revision is exhausted."))?;
        // Once promoted, a namespace cannot regain external authorization through legacy selection or removal.
        // 命名空间提升后，不能通过旧选择或移除重新取得外部授权。
        let approved_permissions = if current
            .as_ref()
            .is_some_and(|record| record.approved_permissions.is_some())
            && approved_permissions.is_none()
        {
            if generation.is_some() {
                return Err(Error::invalid(
                    "Authorized activation cannot downgrade to legacy approval.",
                ));
            }
            Some(Vec::new())
        } else {
            approved_permissions
        };
        // Removal retains the last credential identity for cleanup; an active successor must name its own binding.
        // 移除保留最后凭据身份供清理；活动后继必须指定自身绑定。
        let credential_revision = match (&current, credential_revision) {
            (Some(original), None) if original.credential_revision.is_some() => {
                if generation.is_some() {
                    return Err(Error::invalid(
                        "Credential-bound activation cannot downgrade its credential authority.",
                    ));
                }
                original.credential_revision.clone()
            }
            (_, declared) => declared,
        };
        // Removal retains the original configuration, while an active successor must explicitly name its complete value.
        // 移除保留原配置，而有效后继必须显式指定其完整值。
        let configuration = match (&current, configuration) {
            (Some(original), None) if original.configuration.is_some() => {
                if generation.is_some() {
                    return Err(Error::invalid(
                        "Configuration-bound activation cannot downgrade its configuration authority.",
                    ));
                }
                original.configuration.clone()
            }
            (_, declared) => declared,
        };
        // This single record is the storage-level selection authority, not a claim of route readiness.
        // 此单一记录为存储层选择权威，不表示路由已就绪。
        let next = VulcanPluginActivation {
            schema_version: if configuration.is_some() {
                CONFIGURED_ACTIVATION_VERSION
            } else if credential_revision.is_some() {
                CREDENTIAL_ACTIVATION_VERSION
            } else if approved_permissions.is_some() {
                AUTHORIZED_ACTIVATION_VERSION
            } else {
                RECORD_VERSION
            },
            plugin_id: plugin_id.to_owned(),
            revision,
            generation,
            approved_permissions,
            credential_revision,
            configuration,
        };
        self.write_activation(&next)?;
        Ok(next)
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
        if next.configuration.is_some() {
            let bytes = serde_json::to_vec(next)
                .map_err(|_| Error::invalid("Cannot encode configured plugin activation."))?;
            files::write_encoded_record(&path, &bytes, super::MAX_ACTIVATION_BYTES)?;
        } else {
            files::write_record(&path, next)?;
        }
        Ok(())
    }

    /// Read a generation state record and compare every identity field before returning its root.
    /// 读取代次状态记录，返回其根目录前比较全部身份字段。
    fn read_generation(
        &self,
        identity: &VulcanPluginGenerationIdentity,
    ) -> Result<(PathBuf, GenerationRecord)> {
        files::validate_identity(identity)?;
        // Inspect each identity-derived component before opening metadata or package contents.
        // 打开元数据或插件内容前检查每个身份派生组件。
        let plugin_root = self.root.join("packages").join(&identity.plugin_id);
        if !files::exists(&plugin_root, true)? {
            return Err(Error::invalid("Plugin generation namespace is missing."));
        }
        // The generation root contains metadata beside, never inside, distributable package bytes.
        // 代次根目录在可分发插件字节旁保存元数据，不将其写入插件包。
        let generation_root = plugin_root.join(&identity.generation_id);
        if !files::exists(&generation_root, true)? {
            return Err(Error::invalid("Plugin generation directory is missing."));
        }
        // A missing or malformed state record cannot be inferred from the presence of copied package files.
        // 不得从复制插件文件的存在推断缺失或损坏的状态记录。
        let record: GenerationRecord =
            files::read_record(&generation_root.join("generation.json"))?.ok_or_else(|| {
                Error::invalid("Package generation has no valid identity state record.")
            })?;
        if record.schema_version != RECORD_VERSION || record.identity != *identity {
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
