//! Borrowed first-installation ownership precedes package and state publication.
//! 借用式首次安装所有权先于插件包及状态发布。

use super::*;
use std::sync::{Arc, Weak};

#[cfg(test)]
mod tests;

/// Concrete package progress never substitutes for service or native retirement evidence.
/// 具体插件包进度绝不替代服务或原生退役证据。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InstallationProgress {
    /// No package or state write has been admitted.
    /// 尚未准入插件包或状态写入。
    Reserved,
    /// The unique original copy may be partial and must not be replayed.
    /// 唯一原复制可能不完整，且不得重放。
    Copying,
    /// The exact complete staged package may already have been renamed.
    /// 精确完整暂存插件包可能已被重命名。
    Publishing,
    /// Exact retained authorization bytes may already have been written.
    /// 精确保留授权字节可能已被写入。
    Authorizing,
    /// Exact retained installation bytes may already have been written.
    /// 精确保留安装字节可能已被写入。
    Recording,
    /// All package and record writes returned successfully.
    /// 全部插件包及记录写入已成功返回。
    Installed,
    /// Borrowed compensation is incomplete and retains all original evidence.
    /// 借用式补偿尚未完成，保留全部原证据。
    Aborting,
    /// Actual absence of all owned package and state paths was confirmed.
    /// 已确认全部自有插件包及状态路径实际缺失。
    Aborted,
}

/// Non-serializable first-install owner keeps the same namespace file through uncertain effects.
/// 不可序列化首次安装所有者使同一命名空间文件跨不确定副作用存续。
#[must_use = "retain the installation before publication and explicitly confirm its outcome"]
pub struct VulcanPluginInstallationTransaction {
    /// Original manager identity excludes another manager's cleanup or publication.
    /// 原管理器身份排除其他管理器的清理或发布。
    manager: Weak<super::super::VulcanPluginManagerState>,
    /// Exact original UUID identifies this reservation before its first copy.
    /// 精确原 UUID 在首次复制前标识此预留。
    request_id: Uuid,
    /// Frozen source identity authorizes only the sole initial copy.
    /// 固定来源身份仅授权唯一首次复制。
    source: crate::VulcanPluginDescriptor,
    /// Validated host preview remains independent from later disk inspection.
    /// 已校验宿主预览独立于后续磁盘检查。
    preview: VulcanPluginPermissionPreview,
    /// Unique private staging root is never recovered by searching candidate directories.
    /// 唯一私有暂存根绝不通过搜索候选目录恢复。
    staging_root: PathBuf,
    /// Fixed canonical flat destination belongs only to this original reservation.
    /// 固定规范平铺目标仅属于此原预留。
    destination: PathBuf,
    /// Exact authorization bytes permit readback and conflict-aware compensation.
    /// 精确授权字节支持回读及识别冲突的补偿。
    authorization: Vec<u8>,
    /// Exact installation bytes are allocated before any record write.
    /// 精确安装字节在任何记录写入之前分配。
    installation: Vec<u8>,
    /// Progress advances before every possibly successful uncertain write.
    /// 进度在每次可能已成功的不确定写入前推进。
    progress: InstallationProgress,
    /// Rename admission distinguishes an owned destination from an unrelated pre-copy path.
    /// 重命名准入区分自有目标与复制前的无关路径。
    rename_started: bool,
    /// Authorization admission permits removal only of this request's exact bytes.
    /// 授权准入仅允许删除此请求的精确字节。
    authorization_started: bool,
    /// Installation admission permits removal only of this request's exact bytes.
    /// 安装记录准入仅允许删除此请求的精确字节。
    installation_started: bool,
    /// The actual non-cloneable guard is shared by original calls, never reacquired.
    /// 实际不可克隆屏障由原调用共享，绝不重新获取。
    mutation: Arc<crate::VulcanPluginLegacyMutationGuard>,
}

impl VulcanPluginInstallationTransaction {
    /// Return the allocated original recovery UUID without accessing package or credential storage.
    /// 返回已分配原恢复 UUID，不访问插件包或凭据存储。
    pub fn request_id(&self) -> Uuid {
        self.request_id
    }

    /// Return the frozen plugin identity for this reservation without a mutable catalog lookup.
    /// 返回此预留固定插件身份，不查询可变目录。
    pub fn plugin_id(&self) -> &str {
        &self.source.id
    }

    /// Borrow the exact inspected preview for validation before the first package or credential write.
    /// 借用精确检查预览，以在首次插件包或凭据写入前校验。
    pub fn permission_preview(&self) -> &VulcanPluginPermissionPreview {
        &self.preview
    }

    /// Borrow the same guard for original credential and lifecycle operations; no new file is opened.
    /// 为原凭据及生命周期操作借用同一屏障；不打开新文件。
    pub fn mutation_guard(&self) -> &Arc<crate::VulcanPluginLegacyMutationGuard> {
        &self.mutation
    }

    /// Check the same manager and canonical original namespace before any borrowed operation.
    /// 在任何借用操作前检查同一管理器及规范原命名空间。
    /// manager must be the reserving instance; success returns unit without reacquiring ownership.
    /// manager 必须为预留实例；成功返回空值，不重新获取所有权。
    fn validate(
        &self,
        manager: &VulcanPluginRuntimeManager,
    ) -> Result<(), VulcanPluginRuntimeError> {
        if !Weak::ptr_eq(&self.manager, &Arc::downgrade(&manager.state)) {
            return Err(VulcanPluginRuntimeError::new(
                "Installation belongs to another manager.",
            ));
        }
        self.mutation
            .validate_user_root(&manager.state.user_plugin_root, self.plugin_id())
            .map_err(|error| VulcanPluginRuntimeError::new(error.to_string()))?;
        validate_state_directories(manager, self.plugin_id())
    }
}

/// Reject link or non-directory state parents without creating or traversing a replacement authority.
/// 拒绝链接或非目录状态父级，不创建或遍历替代权威。
/// manager fixes the state root and plugin_id selects its direct child; returns unit or a path error.
/// manager 固定状态根，plugin_id 选择其直接子级；返回空值或路径错误。
pub(super) fn validate_state_directories(
    manager: &VulcanPluginRuntimeManager,
    plugin_id: &str,
) -> Result<(), VulcanPluginRuntimeError> {
    for path in [
        manager.state.plugin_state_root.clone(),
        manager.state.plugin_state_root.join(plugin_id),
    ] {
        match fs::symlink_metadata(&path) {
            Ok(metadata)
                if !crate::metadata_is_link_or_reparse_point(&metadata) && metadata.is_dir() => {}
            Ok(_) => {
                return Err(VulcanPluginRuntimeError::new(
                    "Installation state parent is not a plain directory.",
                ));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(VulcanPluginRuntimeError::new(format!(
                    "Failed to inspect installation state parent: {error}"
                )));
            }
        }
    }
    Ok(())
}

/// Confirm literal absence using metadata, treating dangling links and access errors as conflicts.
/// 使用元数据确认字面缺失，将悬空链接及访问错误视为冲突。
/// path is the original fixed destination; returns unit only for actual NotFound.
/// path 为原固定目标；仅实际不存在时返回空值。
pub(super) fn require_absent(path: &Path) -> Result<(), VulcanPluginRuntimeError> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(VulcanPluginRuntimeError::new(format!(
            "Failed to inspect installation path: {error}"
        ))),
        Ok(_) => Err(VulcanPluginRuntimeError::new(format!(
            "Installation path already exists: `{}`.",
            path.display()
        ))),
    }
}

/// Remove only exact original record bytes, accepting previously confirmed removal on a retry.
/// 仅移除精确原记录字节，重试时接受此前已确认删除。
/// path and expected belong to the retained original; returns unit or a conflict without removing changed bytes.
/// path 及 expected 属于保留原实例；返回空值或冲突，不删除已改变字节。
fn remove_exact_record(path: &Path, expected: &[u8]) -> Result<(), VulcanPluginRuntimeError> {
    if let Some(actual) = read_optional_plugin_state_file(path)? {
        if actual != expected {
            return Err(VulcanPluginRuntimeError::new(
                "Installation record changed before original compensation.",
            ));
        }
        remove_plugin_state_file(path)?;
    }
    require_absent(path)
}

/// Validate every owned descendant before recursive removal, including a partial staging copy.
/// 递归删除前校验每个自有下级，包括不完整暂存副本。
/// root is an original child of boundary; returns unit after removal or explicit rejection preserving evidence.
/// root 为 boundary 的原子级；删除后返回空值，否则明确拒绝并保留证据。
pub(super) fn remove_plain_owned_tree(
    root: &Path,
    boundary: &Path,
) -> Result<(), VulcanPluginRuntimeError> {
    match fs::symlink_metadata(root) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(VulcanPluginRuntimeError::new(format!(
                "Failed to inspect original installation tree: {error}"
            )));
        }
        Ok(metadata)
            if crate::metadata_is_link_or_reparse_point(&metadata) || !metadata.is_dir() =>
        {
            return Err(VulcanPluginRuntimeError::new(
                "Original installation tree is not a plain directory.",
            ));
        }
        Ok(_) => {}
    }
    // Canonical containment and a full link scan precede the recursive delete.
    // 规范包含关系及完整链接扫描先于递归删除。
    let canonical = root
        .canonicalize()
        .map_err(|error| VulcanPluginRuntimeError::new(error.to_string()))?;
    if canonical == boundary || !canonical.starts_with(boundary) {
        return Err(VulcanPluginRuntimeError::new(
            "Original installation tree escaped its fixed user root.",
        ));
    }
    // The explicit worklist visits the exact original tree only; it never polls candidate paths.
    // 明确工作列表仅访问精确原目录树；绝不轮询候选路径。
    let mut directories = vec![root.to_path_buf()];
    while let Some(directory) = directories.pop() {
        for entry in fs::read_dir(directory)
            .map_err(|error| VulcanPluginRuntimeError::new(error.to_string()))?
        {
            // The exact original path selects one owned file without candidate lookup.
            // 精确原路径选择单个自有文件，不查询候选。
            let path = entry
                .map_err(|error| VulcanPluginRuntimeError::new(error.to_string()))?
                .path();
            // Literal metadata rejects redirected original paths.
            // 字面元数据拒绝重定向原路径。
            let metadata = fs::symlink_metadata(&path)
                .map_err(|error| VulcanPluginRuntimeError::new(error.to_string()))?;
            if crate::metadata_is_link_or_reparse_point(&metadata)
                || !(metadata.is_file() || metadata.is_dir())
            {
                return Err(VulcanPluginRuntimeError::new(
                    "Original installation descendant is not a plain file or directory.",
                ));
            }
            if metadata.is_dir() {
                directories.push(path);
            }
        }
    }
    fs::remove_dir_all(root).map_err(|error| VulcanPluginRuntimeError::new(error.to_string()))?;
    require_absent(root)
}

impl VulcanPluginRuntimeManager {
    /// Reserve first-install package and state ownership before copying or writing any plugin state.
    /// 在复制或写入任何插件状态之前预留首次安装插件包及状态所有权。
    /// source_root and expected_content_fingerprint bind inspection; approved_permissions and registry consent bind exact authorization.
    /// source_root 及 expected_content_fingerprint 绑定检查；approved_permissions 及注册表许可绑定精确授权。
    /// Returns the inert original transaction; the service must retain it before calling publication.
    /// 返回未启动原事务；服务必须在调用发布之前保留它。
    pub fn reserve_local_user_plugin_installation(
        &self,
        source_root: &Path,
        expected_content_fingerprint: &str,
        mut approved_permissions: Vec<String>,
        allow_registry_dependencies: bool,
    ) -> Result<VulcanPluginInstallationTransaction, VulcanPluginRuntimeError> {
        // Freeze actual validated source bytes and authorization before allocating namespace ownership.
        // 在分配命名空间所有权之前固定实际已校验来源字节及授权。
        let source =
            inspect_user_plugin_package(source_root).map_err(VulcanPluginRuntimeError::new)?;
        if source.content_fingerprint != expected_content_fingerprint {
            return Err(VulcanPluginRuntimeError::new(
                "Local plugin package changed after authorization inspection.",
            ));
        }
        // The validated actual manifest is the sole declaration source.
        // 已校验实际清单为唯一声明来源。
        let manifest = source.manifest.as_ref().ok_or_else(|| {
            VulcanPluginRuntimeError::new("Local user plugin has no valid manifest.")
        })?;
        if manifest.execution.dependencies.install_mode
            == VulcanPluginDependencyInstallMode::Registry
            && !allow_registry_dependencies
        {
            return Err(VulcanPluginRuntimeError::new(
                "Local plugin requires explicit registry dependency network authorization.",
            ));
        }
        // Actual inspection fixes complete authorization and content identity.
        // 实际检查固定完整授权及内容身份。
        let preview = build_permission_preview(&source)?;
        approved_permissions.sort();
        if approved_permissions
            .windows(2)
            .any(|pair| pair[0] == pair[1])
            || approved_permissions != preview.permissions
        {
            return Err(VulcanPluginRuntimeError::new(
                "Local installation must approve the complete current capability set exactly once.",
            ));
        }
        // Exclude catalog publication while the real namespace owner and immutable paths are allocated.
        // 分配真实命名空间所有者及不可变路径期间，排除目录发布。
        let _registration = self.state.session_registration_gate.lock().map_err(|_| {
            VulcanPluginRuntimeError::new("Vulcan plugin installation gate is poisoned.")
        })?;
        self.require_routing_publication_locked(&_registration, None)?;
        if self
            .snapshot()
            .plugins
            .get(&source.id)
            .is_some_and(|plugin| plugin.origin == VulcanPluginPackageOrigin::Bundled)
        {
            return Err(VulcanPluginRuntimeError::new(
                "Local plugin is already supplied by the system distribution.",
            ));
        }
        // Store opening validates the literal root before its canonical root becomes authoritative.
        // 存储打开在规范根成为权威之前校验字面根。
        fs::create_dir_all(&self.state.user_plugin_root).map_err(|error| {
            VulcanPluginRuntimeError::new(format!(
                "Failed to create host user plugin root: {error}"
            ))
        })?;
        // The actual configured storage boundary performs original operations.
        // 实际已配置存储边界执行原操作。
        let store = crate::VulcanPluginGenerationStore::open(&self.state.user_plugin_root)
            .map_err(|error| VulcanPluginRuntimeError::new(error.to_string()))?;
        // Complete actual activation distinguishes absence from managed state.
        // 完整实际激活区分缺失及受管状态。
        let selected = store
            .activation(&source.id)
            .map_err(|error| VulcanPluginRuntimeError::new(error.to_string()))?;
        if selected.is_some() {
            return Err(VulcanPluginRuntimeError::new(format!(
                "Vulcan plugin `{}` has managed generation state; legacy installation cannot replace it.",
                source.id
            )));
        }
        // The same actual physical guard survives every original effect.
        // 同一实际物理屏障跨每个原副作用存续。
        let mutation = Arc::new(
            store
                .acquire_legacy_mutation_guard(&source.id, None)
                .map_err(|error| VulcanPluginRuntimeError::new(error.to_string()))?,
        );
        mutation
            .validate_user_root(&self.state.user_plugin_root, &source.id)
            .map_err(|error| VulcanPluginRuntimeError::new(error.to_string()))?;
        // The validated canonical host root fixes owned destinations.
        // 已校验规范宿主根固定自有目标。
        let user_root = self
            .state
            .user_plugin_root
            .canonicalize()
            .map_err(|error| VulcanPluginRuntimeError::new(error.to_string()))?;
        // The exact original identity selects its sole flat destination.
        // 精确原身份选择其唯一平铺目标。
        let destination = user_root.join(&source.id);
        require_absent(&destination)?;
        validate_state_directories(self, &source.id)?;
        for path in [
            plugin_authorization_state_path(&self.state.plugin_state_root, &source.id),
            plugin_installation_state_path(&self.state.plugin_state_root, &source.id),
            plugin_config_state_path(&self.state.plugin_state_root, &source.id),
        ] {
            require_absent(&path)?;
        }
        // Bind callback configuration before any package or lifecycle effect can create it.
        // 在任何插件包或生命周期副作用能够创建配置之前，绑定回调配置。
        mutation
            .bind_initial_installation_configuration(&plugin_config_state_path(
                &self.state.plugin_state_root,
                &source.id,
            ))
            .map_err(|error| VulcanPluginRuntimeError::new(error.to_string()))?;
        // The original recovery UUID is allocated before package effects.
        // 原恢复 UUID 在插件包副作用之前分配。
        let request_id = Uuid::new_v4();
        // The unique original staging path cannot alias another request.
        // 唯一原暂存路径不能与其他请求共享。
        let staging_root = user_root.join(format!(".install-{request_id}"));
        require_absent(&staging_root)?;
        // Record bytes are frozen before any uncertain atomic publication, including directory sync failure.
        // 记录字节在任何不确定原子发布之前固定，包括目录同步失败。
        let authorization = serde_json::to_vec_pretty(&authorization_record_for_descriptor(
            &source,
            approved_permissions,
        )?)
        .map_err(|error| VulcanPluginRuntimeError::new(error.to_string()))?;
        // Exact installation bytes are frozen before uncertain publication.
        // 精确安装字节在不确定发布之前固定。
        let installation = serde_json::to_vec_pretty(&VulcanPluginInstallationRecord {
            schema_version: 1,
            plugin_id: source.id.clone(),
            version: source.version.clone().ok_or_else(|| {
                VulcanPluginRuntimeError::new("Installed plugin has no validated version.")
            })?,
            content_fingerprint: source.content_fingerprint.clone(),
            installed_at_ms: current_unix_time_ms(),
        })
        .map_err(|error| VulcanPluginRuntimeError::new(error.to_string()))?;
        Ok(VulcanPluginInstallationTransaction {
            manager: Arc::downgrade(&self.state),
            request_id,
            source,
            preview,
            staging_root,
            destination,
            authorization,
            installation,
            progress: InstallationProgress::Reserved,
            rename_started: false,
            authorization_started: false,
            installation_started: false,
            mutation,
        })
    }

    /// Publish the same borrowed original once; errors retain its guard, paths and intended record bytes.
    /// 仅发布相同借用原实例一次；错误保留其屏障、路径及期望记录字节。
    /// transaction must be this manager's Reserved original; returns installed identity or a recoverable error.
    /// transaction 必须为此管理器的已预留原实例；返回已安装身份或可恢复错误。
    pub fn publish_local_user_plugin_installation(
        &self,
        transaction: &mut VulcanPluginInstallationTransaction,
    ) -> Result<VulcanPluginInstallationResult, VulcanPluginRuntimeError> {
        // The existing publication gate excludes concurrent catalog and lifecycle changes.
        // 既有发布门排除并发目录及生命周期变更。
        let _registration = self.state.session_registration_gate.lock().map_err(|_| {
            VulcanPluginRuntimeError::new("Vulcan plugin installation gate is poisoned.")
        })?;
        self.require_routing_publication_locked(&_registration, None)?;
        transaction.validate(self)?;
        if transaction.progress != InstallationProgress::Reserved {
            return Err(VulcanPluginRuntimeError::new(
                "Original installation copy cannot be replayed.",
            ));
        }
        require_absent(&transaction.destination)?;
        require_absent(&transaction.staging_root)?;
        // This original staging child receives the sole approved copy.
        // 此原暂存子级接收唯一已批准复制。
        let staged = transaction.staging_root.join(transaction.plugin_id());
        transaction.progress = InstallationProgress::Copying;
        copy_plugin_directory_tree(&transaction.source.package_root, &staged)
            .map_err(VulcanPluginRuntimeError::new)?;
        // Actual copied bytes must match the originally approved identity.
        // 实际已复制字节必须匹配原已批准身份。
        let inspected =
            inspect_user_plugin_package(&staged).map_err(VulcanPluginRuntimeError::new)?;
        if inspected.content_fingerprint != transaction.source.content_fingerprint {
            return Err(VulcanPluginRuntimeError::new(
                "Local plugin package changed while it was being installed.",
            ));
        }
        transaction
            .mutation
            .validate_plugin(transaction.plugin_id())
            .map_err(|error| VulcanPluginRuntimeError::new(error.to_string()))?;
        transaction.progress = InstallationProgress::Publishing;
        transaction.rename_started = true;
        fs::rename(&staged, &transaction.destination).map_err(|error| {
            VulcanPluginRuntimeError::new(format!(
                "Failed to publish original plugin package: {error}"
            ))
        })?;
        // The fixed host state root selects the exact authorization file.
        // 固定宿主状态根选择精确授权文件。
        let authorization_path =
            plugin_authorization_state_path(&self.state.plugin_state_root, transaction.plugin_id());
        // The fixed host state root selects the exact installation file.
        // 固定宿主状态根选择精确安装文件。
        let installation_path =
            plugin_installation_state_path(&self.state.plugin_state_root, transaction.plugin_id());
        validate_state_directories(self, transaction.plugin_id())?;
        require_absent(&authorization_path)?;
        transaction.progress = InstallationProgress::Authorizing;
        transaction.authorization_started = true;
        crate::atomic_file::write_file_atomically(&authorization_path, &transaction.authorization)
            .map_err(VulcanPluginRuntimeError::new)?;
        validate_state_directories(self, transaction.plugin_id())?;
        require_absent(&installation_path)?;
        transaction.progress = InstallationProgress::Recording;
        transaction.installation_started = true;
        crate::atomic_file::write_file_atomically(&installation_path, &transaction.installation)
            .map_err(VulcanPluginRuntimeError::new)?;
        // Confirmation comes from exact readback, not just a possibly uncertain writer return.
        // 确认来自精确回读，而非仅可能不确定的写入返回。
        if read_optional_plugin_state_file(&authorization_path)?.as_deref()
            != Some(transaction.authorization.as_slice())
            || read_optional_plugin_state_file(&installation_path)?.as_deref()
                != Some(transaction.installation.as_slice())
        {
            return Err(VulcanPluginRuntimeError::new(
                "Original installation record readback did not match.",
            ));
        }
        fs::remove_dir(&transaction.staging_root).map_err(|error| {
            VulcanPluginRuntimeError::new(format!(
                "Failed to remove original empty staging directory: {error}"
            ))
        })?;
        transaction.progress = InstallationProgress::Installed;
        Ok(VulcanPluginInstallationResult {
            plugin_id: transaction.source.id.clone(),
            permission_preview: transaction.preview.clone(),
        })
    }

    /// Compensate only the borrowed original package after the service confirms actual native and callback retirement.
    /// 服务确认实际原生及回调退役后，仅补偿借用原插件包。
    /// transaction retains every failed cleanup stage; success confirms literal absence and never consumes its guard.
    /// transaction 保留每个失败清理阶段；成功确认字面缺失，绝不消费其屏障。
    pub fn abort_local_user_plugin_installation(
        &self,
        transaction: &mut VulcanPluginInstallationTransaction,
    ) -> Result<(), VulcanPluginRuntimeError> {
        // The existing publication gate excludes concurrent catalog and lifecycle changes.
        // 既有发布门排除并发目录及生命周期变更。
        let _registration = self.state.session_registration_gate.lock().map_err(|_| {
            VulcanPluginRuntimeError::new("Vulcan plugin installation gate is poisoned.")
        })?;
        self.require_routing_publication_locked(&_registration, None)?;
        transaction.validate(self)?;
        if transaction.progress == InstallationProgress::Aborted {
            return Ok(());
        }
        transaction.progress = InstallationProgress::Aborting;
        // The same original guard restores only configuration images admitted by its actual callback writer.
        // 同一原屏障仅恢复其实际回调写入器准入的配置像。
        transaction
            .mutation
            .restore_initial_installation_configuration(&plugin_config_state_path(
                &self.state.plugin_state_root,
                transaction.plugin_id(),
            ))
            .map_err(|error| VulcanPluginRuntimeError::new(error.to_string()))?;
        if transaction.rename_started {
            match fs::symlink_metadata(&transaction.destination) {
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(VulcanPluginRuntimeError::new(error.to_string())),
                Ok(_) => {
                    // Actual original storage evidence is required before confirmation.
                    // 确认之前需要实际原存储证据。
                    let actual = inspect_user_plugin_package(&transaction.destination)
                        .map_err(VulcanPluginRuntimeError::new)?;
                    if actual.content_fingerprint != transaction.source.content_fingerprint {
                        return Err(VulcanPluginRuntimeError::new(
                            "Published installation package changed before compensation.",
                        ));
                    }
                }
            }
            remove_plugin_execution_environment_generation(
                &self.state,
                transaction.plugin_id(),
                transaction.source.version.as_deref().ok_or_else(|| {
                    VulcanPluginRuntimeError::new("Original installation version is absent.")
                })?,
                &transaction.source.content_fingerprint,
            )
            .map_err(VulcanPluginRuntimeError::new)?;
        }
        if transaction.installation_started {
            remove_exact_record(
                &plugin_installation_state_path(
                    &self.state.plugin_state_root,
                    transaction.plugin_id(),
                ),
                &transaction.installation,
            )?;
        }
        if transaction.authorization_started {
            remove_exact_record(
                &plugin_authorization_state_path(
                    &self.state.plugin_state_root,
                    transaction.plugin_id(),
                ),
                &transaction.authorization,
            )?;
        }
        // The fixed original parent constrains recursive cleanup.
        // 固定原父级约束递归清理。
        let boundary = transaction.destination.parent().ok_or_else(|| {
            VulcanPluginRuntimeError::new("Original installation destination has no parent.")
        })?;
        if transaction.rename_started {
            remove_plain_owned_tree(&transaction.destination, boundary)?;
        }
        remove_plain_owned_tree(&transaction.staging_root, boundary)?;
        require_absent(&transaction.destination)?;
        for path in [
            plugin_authorization_state_path(&self.state.plugin_state_root, transaction.plugin_id()),
            plugin_installation_state_path(&self.state.plugin_state_root, transaction.plugin_id()),
        ] {
            require_absent(&path)?;
        }
        transaction.progress = InstallationProgress::Aborted;
        Ok(())
    }
}
