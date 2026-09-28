//! Staged user-plugin installation, update, rollback, and uninstallation.
//! 用户插件的暂存安装、更新、回滚与卸载。

use super::authorization::{
    VulcanPluginPermissionPreview, authorization_record_for_descriptor, build_permission_preview,
    plugin_authorization_state_path,
};
use super::configuration::plugin_config_state_path;
use super::environment::{
    remove_obsolete_plugin_execution_environments, remove_plugin_execution_environment_generation,
};
use super::{
    VulcanPluginRuntimeError, VulcanPluginRuntimeManager, copy_plugin_directory_tree,
    write_plugin_json_record,
};
use crate::{
    VulcanPluginDependencyInstallMode, VulcanPluginPackageOrigin, VulcanPluginRuntimeState,
    inspect_user_plugin_package,
};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

mod rollback;

/// Durable installation record for one atomically published user plugin package.
/// 一个原子发布用户插件包的持久安装记录。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
struct VulcanPluginInstallationRecord {
    /// Stable record schema version.
    /// 稳定记录结构版本。
    schema_version: u32,
    /// Exact installed plugin identity.
    /// 精确的已安装插件身份。
    plugin_id: String,
    /// Exact installed package version.
    /// 精确的已安装包版本。
    version: String,
    /// Exact installed content fingerprint.
    /// 精确的已安装内容指纹。
    content_fingerprint: String,
    /// Unix timestamp in milliseconds when publication completed.
    /// 发布完成时的 Unix 毫秒时间戳。
    installed_at_ms: u64,
}

/// Result of one local user-plugin installation transaction.
/// 一次本地用户插件安装事务的结果。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct VulcanPluginInstallationResult {
    /// Exact installed plugin identity.
    /// 精确的已安装插件身份。
    pub plugin_id: String,
    /// Permission preview durably approved for the installed package.
    /// 为已安装包持久批准的权限预览。
    pub permission_preview: VulcanPluginPermissionPreview,
}

/// Pending filesystem transaction for one disabled user-plugin package update.
/// 单个已停用用户插件包更新的待提交文件系统事务。
#[derive(Debug)]
#[must_use = "a plugin update transaction must be committed or rolled back"]
pub struct VulcanPluginUpdateTransaction {
    /// Exact updated plugin identity.
    /// 精确的已更新插件身份。
    plugin_id: String,
    /// Unique private transaction root retaining the previous package until commit.
    /// 提交前保留旧插件包的唯一私有事务根目录。
    transaction_root: PathBuf,
    /// Canonical user-plugin package destination containing the candidate package.
    /// 包含候选插件包的规范化用户插件目标目录。
    destination: PathBuf,
    /// Previous authorization record bytes restored verbatim on rollback.
    /// 回滚时逐字节恢复的旧授权记录内容。
    previous_authorization: Vec<u8>,
    /// Previous installation record bytes restored verbatim on rollback.
    /// 回滚时逐字节恢复的旧安装记录内容。
    previous_installation: Vec<u8>,
    /// Original installed content identity required before any package rollback.
    /// 执行任何插件包回滚前必须匹配的原始已安装内容身份。
    previous_fingerprint: String,
    /// Candidate version used to remove only its unpublished private environment on rollback.
    /// 回滚时仅移除其未发布私有环境的候选版本。
    candidate_version: String,
    /// Candidate fingerprint used to identify the exact unpublished private environment.
    /// 用于识别精确未发布私有环境的候选指纹。
    candidate_fingerprint: String,
    /// Host-derived authorization preview for the candidate package.
    /// 候选插件包的宿主派生授权预览。
    permission_preview: VulcanPluginPermissionPreview,
}

/// Returns the deterministic installation record path for one validated plugin identity.
/// 返回一个已校验插件身份的确定性安装记录路径。
///
/// # Parameters
/// 参数
/// `state_root` is the fixed per-user plugin state root.
/// `state_root` 是固定的用户插件状态根。
/// `plugin_id` is the validated plugin identity selecting its installation record.
/// `plugin_id` 是选择所属安装记录的已校验插件身份。
///
/// # Returns
/// 返回值
/// Returns the deterministic installation.json path without filesystem mutation.
/// 返回不修改文件系统的确定性installation.json路径。
fn plugin_installation_state_path(state_root: &Path, plugin_id: &str) -> PathBuf {
    state_root.join(plugin_id).join("installation.json")
}

/// Reads one optional regular plugin state file for transactional rollback.
/// 读取一个可选普通插件状态文件以供事务回滚。
///
/// # Parameters
/// 参数
/// `path` is the exact state file owned by the selected plugin identity.
/// `path` 是所选插件身份拥有的精确状态文件。
///
/// # Returns
/// 返回值
/// Returns absent state or the exact file bytes after rejecting links and special files.
/// 在拒绝链接与特殊文件后返回缺失状态或精确文件字节。
fn read_optional_plugin_state_file(
    path: &Path,
) -> Result<Option<Vec<u8>>, VulcanPluginRuntimeError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(VulcanPluginRuntimeError::new(format!(
                "Failed to inspect plugin state file: {error}"
            )));
        }
    };
    if crate::metadata_is_link_or_reparse_point(&metadata) || !metadata.is_file() {
        return Err(VulcanPluginRuntimeError::new(format!(
            "Plugin state path `{}` is not a regular non-symbolic-link or reparse-point file.",
            path.display()
        )));
    }
    fs::read(path).map(Some).map_err(|error| {
        VulcanPluginRuntimeError::new(format!(
            "Failed to read plugin state file `{}`: {error}",
            path.display()
        ))
    })
}

/// Removes one optional plugin state file without following links.
/// 在不跟随链接的情况下移除一个可选插件状态文件。
///
/// # Parameters
/// 参数
/// `path` is the exact state file owned by the selected plugin identity.
/// `path` 是所选插件身份拥有的精确状态文件。
///
/// # Returns
/// 返回值
/// Returns success for an absent or removed regular file, or a metadata/type/removal error.
/// 返回缺失或已删除普通文件的成功结果，或元数据/类型/删除错误。
fn remove_plugin_state_file(path: &Path) -> Result<(), VulcanPluginRuntimeError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(VulcanPluginRuntimeError::new(format!(
                "Failed to inspect plugin state file: {error}"
            )));
        }
    };
    if crate::metadata_is_link_or_reparse_point(&metadata) || !metadata.is_file() {
        return Err(VulcanPluginRuntimeError::new(format!(
            "Plugin state path `{}` is not a regular non-symbolic-link or reparse-point file.",
            path.display()
        )));
    }
    fs::remove_file(path).map_err(|error| {
        VulcanPluginRuntimeError::new(format!(
            "Failed to remove plugin state file `{}`: {error}",
            path.display()
        ))
    })
}

/// Returns the current Unix time in milliseconds for installation records.
/// 返回用于安装记录的当前 Unix 毫秒时间。
///
/// # Parameters
/// 参数
///
/// # Returns
/// 返回值
/// Returns the current installation timestamp, using zero before the epoch and u64::MAX on overflow.
/// 返回当前安装时间戳，纪元之前取零、溢出取u64::MAX。
fn current_unix_time_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

impl VulcanPluginUpdateTransaction {
    /// Returns the exact updated plugin identifier.
    /// 返回精确的已更新插件标识。
    ///
    /// # Parameters
    /// 参数
    ///
    /// # Returns
    /// 返回值
    /// Returns the exact plugin identity borrowed from this pending transaction.
    /// 返回从当前待处理事务借用的精确插件身份。
    pub fn plugin_id(&self) -> &str {
        &self.plugin_id
    }

    /// Returns the candidate package authorization preview.
    /// 返回候选插件包的授权预览。
    ///
    /// # Parameters
    /// 参数
    ///
    /// # Returns
    /// 返回值
    /// Returns the candidate permission preview borrowed from this transaction.
    /// 返回从当前事务借用的候选权限预览。
    pub fn permission_preview(&self) -> &VulcanPluginPermissionPreview {
        &self.permission_preview
    }
}

impl VulcanPluginRuntimeManager {
    /// Inspects one local user-plugin package before any filesystem mutation.
    /// 在执行任何文件系统变更前检查一个本地用户插件包。
    ///
    /// # Parameters
    /// 参数
    /// `source_root` is the exact local package directory selected by the user.
    /// `source_root` 是用户选择的精确本地插件包目录。
    ///
    /// # Returns
    /// 返回值
    /// Returns the complete capability, Hook, UI, and process permission preview.
    /// 返回完整能力、Hook、界面及进程权限预览。
    pub fn inspect_local_user_plugin(
        &self,
        source_root: &Path,
    ) -> Result<VulcanPluginPermissionPreview, VulcanPluginRuntimeError> {
        let descriptor =
            inspect_user_plugin_package(source_root).map_err(VulcanPluginRuntimeError::new)?;
        build_permission_preview(&descriptor)
    }

    /// Installs and authorizes a validated user package through staged publication and compensating cleanup.
    /// 通过暂存发布与补偿清理安装并授权已校验的用户插件包。
    ///
    /// # Parameters
    /// 参数
    /// `source_root` selects the package, `expected_content_fingerprint` binds the prior authorization preview,
    /// `source_root` 选择插件包，`expected_content_fingerprint` 绑定此前的授权预览，
    /// `approved_permissions` must match its exact declaration, and `allow_registry_dependencies` records explicit network installation consent.
    /// `approved_permissions` 必须匹配其精确声明，`allow_registry_dependencies` 记录显式网络依赖安装许可。
    ///
    /// # Returns
    /// 返回值
    /// Returns the installed identity and durable permission preview.
    /// 返回已安装身份及持久权限预览。
    pub fn install_local_user_plugin(
        &self,
        source_root: &Path,
        expected_content_fingerprint: &str,
        mut approved_permissions: Vec<String>,
        allow_registry_dependencies: bool,
    ) -> Result<VulcanPluginInstallationResult, VulcanPluginRuntimeError> {
        let _registration = self.state.session_registration_gate.lock().map_err(|_| {
            VulcanPluginRuntimeError::new("Vulcan plugin installation gate is poisoned.")
        })?;
        let source_descriptor =
            inspect_user_plugin_package(source_root).map_err(VulcanPluginRuntimeError::new)?;
        if source_descriptor.content_fingerprint != expected_content_fingerprint {
            return Err(VulcanPluginRuntimeError::new(
                "Local plugin package changed after authorization inspection.",
            ));
        }
        let manifest = source_descriptor.manifest.as_ref().ok_or_else(|| {
            VulcanPluginRuntimeError::new("Local user plugin has no valid manifest.")
        })?;
        if manifest.execution.dependencies.install_mode
            == VulcanPluginDependencyInstallMode::Registry
            && !allow_registry_dependencies
        {
            return Err(VulcanPluginRuntimeError::new(format!(
                "Vulcan plugin `{}` requires explicit registry dependency network authorization.",
                source_descriptor.id
            )));
        }
        let preview = build_permission_preview(&source_descriptor)?;
        approved_permissions.sort();
        if approved_permissions
            .windows(2)
            .any(|pair| pair[0] == pair[1])
            || approved_permissions != preview.permissions
        {
            return Err(VulcanPluginRuntimeError::new(format!(
                "Vulcan plugin `{}` installation must approve the complete current capability set exactly once.",
                source_descriptor.id
            )));
        }
        if self
            .snapshot()
            .plugins
            .get(&source_descriptor.id)
            .is_some_and(|plugin| plugin.origin == VulcanPluginPackageOrigin::Bundled)
        {
            return Err(VulcanPluginRuntimeError::new(format!(
                "Vulcan plugin `{}` is already supplied by the system distribution.",
                source_descriptor.id
            )));
        }
        fs::create_dir_all(&self.state.user_plugin_root).map_err(|error| {
            VulcanPluginRuntimeError::new(format!("Failed to create user plugin root: {error}"))
        })?;
        let canonical_user_root = self
            .state
            .user_plugin_root
            .canonicalize()
            .map_err(|error| {
                VulcanPluginRuntimeError::new(format!(
                    "Failed to resolve user plugin root: {error}"
                ))
            })?;
        let destination = canonical_user_root.join(&source_descriptor.id);
        if destination.exists() {
            return Err(VulcanPluginRuntimeError::new(format!(
                "Vulcan plugin `{}` is already installed for this user.",
                source_descriptor.id
            )));
        }
        // authorization_path, installation_path, and config_path must be absent so rollback never deletes pre-existing user state.
        // authorization_path、installation_path 与 config_path 必须不存在，确保回滚绝不删除既有用户状态。
        let authorization_path =
            plugin_authorization_state_path(&self.state.plugin_state_root, &source_descriptor.id);
        let installation_path =
            plugin_installation_state_path(&self.state.plugin_state_root, &source_descriptor.id);
        let config_path =
            plugin_config_state_path(&self.state.plugin_state_root, &source_descriptor.id);
        for (label, path) in [
            ("authorization", &authorization_path),
            ("installation", &installation_path),
            ("configuration", &config_path),
        ] {
            if path.exists() {
                return Err(VulcanPluginRuntimeError::new(format!(
                    "Vulcan plugin `{}` has stale {label} state at `{}`; remove or recover it before installation.",
                    source_descriptor.id,
                    path.display()
                )));
            }
        }
        let staging_root = canonical_user_root.join(format!(".install-{}", Uuid::new_v4()));
        let staged_package = staging_root.join(&source_descriptor.id);
        let transaction =
            (|| -> Result<VulcanPluginInstallationResult, VulcanPluginRuntimeError> {
                copy_plugin_directory_tree(&source_descriptor.package_root, &staged_package)
                    .map_err(VulcanPluginRuntimeError::new)?;
                let staged_descriptor = inspect_user_plugin_package(&staged_package)
                    .map_err(VulcanPluginRuntimeError::new)?;
                if staged_descriptor.content_fingerprint != source_descriptor.content_fingerprint {
                    return Err(VulcanPluginRuntimeError::new(
                        "Local plugin package changed while it was being installed.",
                    ));
                }
                fs::rename(&staged_package, &destination).map_err(|error| {
                    VulcanPluginRuntimeError::new(format!(
                        "Failed to atomically publish Vulcan plugin `{}`: {error}",
                        source_descriptor.id
                    ))
                })?;
                let authorization = authorization_record_for_descriptor(
                    &staged_descriptor,
                    approved_permissions.clone(),
                )?;
                write_plugin_json_record(&authorization_path, &authorization, "authorization")?;
                let installation = VulcanPluginInstallationRecord {
                    schema_version: 1,
                    plugin_id: staged_descriptor.id.clone(),
                    version: staged_descriptor.version.clone().ok_or_else(|| {
                        VulcanPluginRuntimeError::new("Installed plugin has no validated version.")
                    })?,
                    content_fingerprint: staged_descriptor.content_fingerprint.clone(),
                    installed_at_ms: current_unix_time_ms(),
                };
                write_plugin_json_record(&installation_path, &installation, "installation")?;
                Ok(VulcanPluginInstallationResult {
                    plugin_id: source_descriptor.id.clone(),
                    permission_preview: preview.clone(),
                })
            })();
        match transaction {
            Ok(result) => {
                if staging_root.exists() {
                    // The published package no longer depends on its now-empty unique staging parent.
                    // 已发布插件包不再依赖其当前为空的唯一暂存父目录。
                    let _ = fs::remove_dir(&staging_root);
                }
                Ok(result)
            }
            Err(error) => {
                // rollbackFailures preserves every residual package or state cleanup failure.
                // rollbackFailures 保留每个残余插件包或状态清理失败。
                let mut rollback_failures = Vec::new();
                if destination.exists()
                    && let Err(rollback_error) = fs::remove_dir_all(&destination)
                {
                    rollback_failures.push(format!(
                        "Failed to remove published plugin package during rollback: {rollback_error}"
                    ));
                }
                for (label, path) in [
                    ("authorization", &authorization_path),
                    ("installation", &installation_path),
                ] {
                    if path.exists()
                        && let Err(rollback_error) = fs::remove_file(path)
                    {
                        rollback_failures.push(format!(
                            "Failed to remove plugin {label} state during rollback: {rollback_error}"
                        ));
                    }
                }
                if staging_root.exists()
                    && let Err(rollback_error) = fs::remove_dir_all(&staging_root)
                {
                    rollback_failures.push(format!(
                        "Failed to remove plugin staging directory during rollback: {rollback_error}"
                    ));
                }
                if rollback_failures.is_empty() {
                    Err(error)
                } else {
                    Err(VulcanPluginRuntimeError::new(format!(
                        "{error}; installation rollback failed: {}",
                        rollback_failures.join("; ")
                    )))
                }
            }
        }
    }

    /// Begins a gated candidate swap while retaining the disabled and drained plugin's previous package.
    /// 在同步门内开始候选替换，并保留已停用且排空插件的旧包。
    ///
    /// # Parameters
    /// 参数
    /// `source_root` selects the replacement package, `expected_content_fingerprint` binds the prior authorization preview,
    /// `source_root` 选择替换插件包，`expected_content_fingerprint` 绑定此前的授权预览，
    /// `approved_permissions` must match its exact declaration, and `allow_registry_dependencies` records explicit network installation consent.
    /// `approved_permissions` 必须与其精确声明匹配，`allow_registry_dependencies` 记录显式网络依赖安装许可。
    ///
    /// # Returns
    /// 返回值
    /// Returns a pending transaction retaining the previous package until service-level validation commits it.
    /// 返回一份待提交事务，在 Service 层校验提交前保留旧插件包。
    pub fn begin_local_user_plugin_update(
        &self,
        source_root: &Path,
        expected_content_fingerprint: &str,
        mut approved_permissions: Vec<String>,
        allow_registry_dependencies: bool,
    ) -> Result<VulcanPluginUpdateTransaction, VulcanPluginRuntimeError> {
        let _registration =
            self.state.session_registration_gate.lock().map_err(|_| {
                VulcanPluginRuntimeError::new("Vulcan plugin update gate is poisoned.")
            })?;
        // source_descriptor is independently inspected before any installed package mutation.
        // source_descriptor 在任何已安装插件包变更前独立完成检查。
        let source_descriptor =
            inspect_user_plugin_package(source_root).map_err(VulcanPluginRuntimeError::new)?;
        if source_descriptor.content_fingerprint != expected_content_fingerprint {
            return Err(VulcanPluginRuntimeError::new(
                "Local plugin package changed after update authorization inspection.",
            ));
        }
        let manifest = source_descriptor.manifest.as_ref().ok_or_else(|| {
            VulcanPluginRuntimeError::new("Local user plugin update has no valid manifest.")
        })?;
        if manifest.execution.dependencies.install_mode
            == VulcanPluginDependencyInstallMode::Registry
            && !allow_registry_dependencies
        {
            return Err(VulcanPluginRuntimeError::new(format!(
                "Vulcan plugin `{}` update requires explicit registry dependency network authorization.",
                source_descriptor.id
            )));
        }
        let preview = build_permission_preview(&source_descriptor)?;
        approved_permissions.sort();
        if approved_permissions
            .windows(2)
            .any(|pair| pair[0] == pair[1])
            || approved_permissions != preview.permissions
        {
            return Err(VulcanPluginRuntimeError::new(format!(
                "Vulcan plugin `{}` update must approve the complete current capability set exactly once.",
                source_descriptor.id
            )));
        }
        // installed_snapshot pins the exact old generation while its package is moved into the transaction root.
        // installed_snapshot 在旧插件包移入事务根期间固定精确旧代次。
        let installed_snapshot = self.snapshot();
        let installed = installed_snapshot
            .plugins
            .get(&source_descriptor.id)
            .ok_or_else(|| {
                VulcanPluginRuntimeError::new(format!(
                    "Vulcan plugin `{}` is not installed for this user.",
                    source_descriptor.id
                ))
            })?;
        if installed.origin != VulcanPluginPackageOrigin::UserInstalled {
            return Err(VulcanPluginRuntimeError::new(format!(
                "Bundled Vulcan plugin `{}` cannot be updated from a local user package.",
                source_descriptor.id
            )));
        }
        if installed.runtime_state != VulcanPluginRuntimeState::Disabled
            || installed.active_session_count != 0
        {
            return Err(VulcanPluginRuntimeError::new(format!(
                "Vulcan plugin `{}` must be disabled and fully drained before update.",
                source_descriptor.id
            )));
        }
        if installed.content_fingerprint == source_descriptor.content_fingerprint {
            return Err(VulcanPluginRuntimeError::new(format!(
                "Vulcan plugin `{}` update package is identical to the installed package.",
                source_descriptor.id
            )));
        }
        let canonical_user_root = self
            .state
            .user_plugin_root
            .canonicalize()
            .map_err(|error| {
                VulcanPluginRuntimeError::new(format!(
                    "Failed to resolve user plugin root: {error}"
                ))
            })?;
        let destination = installed.package_root.canonicalize().map_err(|error| {
            VulcanPluginRuntimeError::new(format!(
                "Failed to resolve installed user plugin package: {error}"
            ))
        })?;
        if destination.parent() != Some(canonical_user_root.as_path()) {
            return Err(VulcanPluginRuntimeError::new(
                "Installed user plugin package is outside the exact user plugin root.",
            ));
        }
        // authorization_path and installation_path are snapshotted verbatim for exact rollback.
        // authorization_path 与 installation_path 被逐字节快照以供精确回滚。
        let authorization_path =
            plugin_authorization_state_path(&self.state.plugin_state_root, &source_descriptor.id);
        let installation_path =
            plugin_installation_state_path(&self.state.plugin_state_root, &source_descriptor.id);
        let previous_authorization = fs::read(&authorization_path).map_err(|error| {
            VulcanPluginRuntimeError::new(format!(
                "Failed to snapshot existing plugin authorization before update: {error}"
            ))
        })?;
        let previous_installation = fs::read(&installation_path).map_err(|error| {
            VulcanPluginRuntimeError::new(format!(
                "Failed to snapshot existing plugin installation record before update: {error}"
            ))
        })?;
        let transaction_root = canonical_user_root.join(format!(".update-{}", Uuid::new_v4()));
        let staged_package = transaction_root.join(&source_descriptor.id);
        let previous_package = transaction_root.join("previous");
        fs::create_dir(&transaction_root).map_err(|error| {
            VulcanPluginRuntimeError::new(format!(
                "Failed to create Vulcan plugin update transaction root: {error}"
            ))
        })?;
        let candidate_version = source_descriptor.version.clone().ok_or_else(|| {
            VulcanPluginRuntimeError::new("Updated plugin has no validated version.")
        })?;
        let candidate_fingerprint = source_descriptor.content_fingerprint.clone();
        let update_result = (|| -> Result<(), VulcanPluginRuntimeError> {
            // Persist exact old records before the first package or authorization mutation.
            // 首次插件包或授权变更前持久保存精确旧记录。
            rollback::preserve_rollback_records(
                &transaction_root,
                &source_descriptor.id,
                &previous_authorization,
                &previous_installation,
            )?;
            copy_plugin_directory_tree(&source_descriptor.package_root, &staged_package)
                .map_err(VulcanPluginRuntimeError::new)?;
            let staged_descriptor = inspect_user_plugin_package(&staged_package)
                .map_err(VulcanPluginRuntimeError::new)?;
            if staged_descriptor.content_fingerprint != source_descriptor.content_fingerprint {
                return Err(VulcanPluginRuntimeError::new(
                    "Local plugin package changed while its update was being staged.",
                ));
            }
            fs::rename(&destination, &previous_package).map_err(|error| {
                VulcanPluginRuntimeError::new(format!(
                    "Failed to retain previous Vulcan plugin package for update rollback: {error}"
                ))
            })?;
            fs::rename(&staged_package, &destination).map_err(|error| {
                VulcanPluginRuntimeError::new(format!(
                    "Failed to publish candidate Vulcan plugin package for validation: {error}"
                ))
            })?;
            let authorization = authorization_record_for_descriptor(
                &staged_descriptor,
                approved_permissions.clone(),
            )?;
            write_plugin_json_record(&authorization_path, &authorization, "authorization")?;
            let installation = VulcanPluginInstallationRecord {
                schema_version: 1,
                plugin_id: staged_descriptor.id.clone(),
                version: candidate_version.clone(),
                content_fingerprint: candidate_fingerprint.clone(),
                installed_at_ms: current_unix_time_ms(),
            };
            write_plugin_json_record(&installation_path, &installation, "installation")?;
            Ok(())
        })();
        if let Err(error) = update_result {
            // Keep the transaction directory and old record bytes whenever restoration fails.
            // 任一恢复失败时保留事务目录及旧记录字节。
            let rollback_failures = rollback::rollback_failed_preparation(
                &transaction_root,
                &destination,
                &authorization_path,
                &installation_path,
                &previous_authorization,
                &previous_installation,
                &installed.content_fingerprint,
            );
            return if rollback_failures.is_empty() {
                Err(error)
            } else {
                Err(VulcanPluginRuntimeError::new(format!(
                    "{error}; update rollback failed: {}; recovery files retained at `{}`",
                    rollback_failures.join("; "),
                    transaction_root.display()
                )))
            };
        }
        Ok(VulcanPluginUpdateTransaction {
            plugin_id: source_descriptor.id.clone(),
            transaction_root,
            destination,
            previous_authorization,
            previous_installation,
            previous_fingerprint: installed.content_fingerprint.clone(),
            candidate_version,
            candidate_fingerprint,
            permission_preview: preview,
        })
    }

    /// Verifies that a staged update candidate still matches the exact package inspected at transaction start.
    /// 验证暂存更新候选仍与事务开始时检查的精确插件包一致。
    ///
    /// # Parameters
    /// 参数
    /// `transaction` identifies the candidate that is about to enter an extension generation.
    /// `transaction` 标识即将进入扩展代次的候选插件包。
    ///
    /// # Returns
    /// 返回值
    /// Returns success only when the complete package fingerprint remains unchanged.
    /// 仅当完整插件包指纹保持不变时返回成功。
    pub fn verify_local_user_plugin_update_candidate(
        &self,
        transaction: &VulcanPluginUpdateTransaction,
    ) -> Result<(), VulcanPluginRuntimeError> {
        let _registration = self.state.session_registration_gate.lock().map_err(|_| {
            VulcanPluginRuntimeError::new("Vulcan plugin update verification gate is poisoned.")
        })?;
        let fingerprint = crate::catalog::compute_package_fingerprint(&transaction.destination)
            .map_err(VulcanPluginRuntimeError::new)?;
        if fingerprint != transaction.candidate_fingerprint {
            return Err(VulcanPluginRuntimeError::new(format!(
                "Vulcan plugin `{}` candidate changed before update publication.",
                transaction.plugin_id
            )));
        }
        Ok(())
    }

    /// Commits one validated update by deleting its retained previous package and obsolete environments.
    /// 通过删除保留的旧插件包与过期环境提交一次已验证更新。
    ///
    /// # Parameters
    /// 参数
    /// `transaction` is the exact pending update previously returned by this manager.
    /// `transaction` 是此前由当前管理器返回的精确待提交更新。
    ///
    /// # Returns
    /// 返回值
    /// Returns success after fingerprint verification and retained-data cleanup, or the exact failure diagnostic.
    /// 返回指纹复查和保留数据清理后的成功结果，或精确失败诊断。
    pub fn commit_local_user_plugin_update(
        &self,
        transaction: VulcanPluginUpdateTransaction,
    ) -> Result<(), VulcanPluginRuntimeError> {
        let _registration = self.state.session_registration_gate.lock().map_err(|_| {
            VulcanPluginRuntimeError::new("Vulcan plugin update commit gate is poisoned.")
        })?;
        // The second fingerprint check closes the gap between pre-publication verification and post-publication cleanup.
        // 第二次指纹检查封闭发布前复核与发布后清理之间的间隙。
        let fingerprint = crate::catalog::compute_package_fingerprint(&transaction.destination)
            .map_err(VulcanPluginRuntimeError::new)?;
        if fingerprint != transaction.candidate_fingerprint {
            return Err(VulcanPluginRuntimeError::new(format!(
                "Vulcan plugin `{}` candidate changed before update cleanup.",
                transaction.plugin_id
            )));
        }
        fs::remove_dir_all(&transaction.transaction_root).map_err(|error| {
            VulcanPluginRuntimeError::new(format!(
                "Failed to remove retained previous package for Vulcan plugin `{}`: {error}",
                transaction.plugin_id
            ))
        })?;
        remove_obsolete_plugin_execution_environments(
            &self.state,
            &transaction.plugin_id,
            &transaction.candidate_version,
            &transaction.candidate_fingerprint,
        )
        .map_err(VulcanPluginRuntimeError::new)
    }

    /// Rolls back one uncommitted update to the exact previous package and durable records.
    /// 将一次未提交更新回滚到精确的旧插件包与持久记录。
    ///
    /// # Parameters
    /// 参数
    /// `transaction` is the pending update whose candidate has not entered a published generation.
    /// `transaction` 是候选尚未进入已发布代次的待处理更新。
    ///
    /// # Returns
    /// 返回值
    /// Returns success after restoring the old package/records and removing the candidate environment, or a restoration/cleanup error.
    /// 返回恢复旧包/记录并移除候选环境后的成功结果，或恢复/清理错误。
    pub fn rollback_local_user_plugin_update(
        &self,
        transaction: VulcanPluginUpdateTransaction,
    ) -> Result<(), VulcanPluginRuntimeError> {
        let _registration = self.state.session_registration_gate.lock().map_err(|_| {
            VulcanPluginRuntimeError::new("Vulcan plugin update rollback gate is poisoned.")
        })?;
        let previous_package = transaction.transaction_root.join("previous");
        let failed_candidate = transaction.transaction_root.join("failed-candidate");
        let authorization_path =
            plugin_authorization_state_path(&self.state.plugin_state_root, &transaction.plugin_id);
        let installation_path =
            plugin_installation_state_path(&self.state.plugin_state_root, &transaction.plugin_id);
        // A conflicting retained package must not replace the candidate or erase recovery evidence.
        // 存在冲突的保留插件包不得替换候选或擦除恢复证据。
        rollback::verify_original_package(&previous_package, &transaction.previous_fingerprint)
            .map_err(VulcanPluginRuntimeError::new)?;
        fs::rename(&transaction.destination, &failed_candidate).map_err(|error| {
            VulcanPluginRuntimeError::new(format!(
                "Failed to hide candidate package during update rollback: {error}"
            ))
        })?;
        if let Err(error) = fs::rename(&previous_package, &transaction.destination) {
            let restore_candidate_result = fs::rename(&failed_candidate, &transaction.destination);
            return Err(VulcanPluginRuntimeError::new(
                match restore_candidate_result {
                    Ok(()) => format!("Failed to restore previous plugin package: {error}"),
                    Err(candidate_error) => format!(
                        "Failed to restore previous plugin package: {error}; candidate restoration also failed: {candidate_error}"
                    ),
                },
            ));
        }
        let rollback_result = (|| -> Result<(), String> {
            rollback::verify_original_package(
                &transaction.destination,
                &transaction.previous_fingerprint,
            )?;
            crate::atomic_file::write_file_atomically(
                &authorization_path,
                &transaction.previous_authorization,
            )?;
            crate::atomic_file::write_file_atomically(
                &installation_path,
                &transaction.previous_installation,
            )?;
            if failed_candidate.exists() {
                fs::remove_dir_all(&failed_candidate).map_err(|error| {
                    format!("Failed to remove rolled-back candidate package: {error}")
                })?;
            }
            // Both original records are restored before their independent recovery copy is removed.
            // 两份原始记录均已恢复后，才删除其独立恢复副本。
            fs::remove_file(
                transaction
                    .transaction_root
                    .join(rollback::ROLLBACK_RECORDS_FILE),
            )
            .map_err(|error| format!("Failed to remove completed rollback snapshot: {error}"))?;
            fs::remove_dir(&transaction.transaction_root).map_err(|error| {
                format!("Failed to remove completed update rollback root: {error}")
            })?;
            remove_plugin_execution_environment_generation(
                &self.state,
                &transaction.plugin_id,
                &transaction.candidate_version,
                &transaction.candidate_fingerprint,
            )
        })();
        rollback_result.map_err(VulcanPluginRuntimeError::new)
    }

    /// Removes one fully drained user-installed plugin package and its private state.
    /// 移除一个已完全排空的用户安装插件包及其私有状态。
    ///
    /// # Parameters
    /// 参数
    /// `plugin_id` is the exact effective user plugin identity.
    /// `plugin_id` 是精确的生效用户插件身份。
    ///
    /// # Returns
    /// 返回值
    /// Returns success after removal, or the original failure with any compensating restoration diagnostics.
    /// 返回移除后的成功结果，或原始失败与补偿恢复诊断。
    pub fn uninstall_user_plugin(&self, plugin_id: &str) -> Result<(), VulcanPluginRuntimeError> {
        let _registration = self.state.session_registration_gate.lock().map_err(|_| {
            VulcanPluginRuntimeError::new("Vulcan plugin uninstallation gate is poisoned.")
        })?;
        let snapshot = self.snapshot();
        let plugin = snapshot.plugins.get(plugin_id).ok_or_else(|| {
            VulcanPluginRuntimeError::new(format!("Vulcan plugin `{plugin_id}` is not installed."))
        })?;
        if plugin.origin != VulcanPluginPackageOrigin::UserInstalled {
            return Err(VulcanPluginRuntimeError::new(format!(
                "Bundled Vulcan plugin `{plugin_id}` cannot be uninstalled."
            )));
        }
        if plugin.active_session_count != 0
            || plugin.runtime_state != VulcanPluginRuntimeState::Disabled
        {
            return Err(VulcanPluginRuntimeError::new(format!(
                "Vulcan plugin `{plugin_id}` must be disabled and fully drained before uninstall."
            )));
        }
        let canonical_user_root = self
            .state
            .user_plugin_root
            .canonicalize()
            .map_err(|error| {
                VulcanPluginRuntimeError::new(format!(
                    "Failed to resolve user plugin root: {error}"
                ))
            })?;
        let canonical_package = plugin.package_root.canonicalize().map_err(|error| {
            VulcanPluginRuntimeError::new(format!("Failed to resolve user plugin package: {error}"))
        })?;
        if canonical_package.parent() != Some(canonical_user_root.as_path()) {
            return Err(VulcanPluginRuntimeError::new(
                "User plugin package is outside the exact user plugin root.",
            ));
        }
        // state_paths are validated and snapshotted before the package leaves its discoverable name.
        // state_paths 在插件包离开可发现名称前完成校验与快照。
        let state_paths = [
            plugin_authorization_state_path(&self.state.plugin_state_root, plugin_id),
            plugin_installation_state_path(&self.state.plugin_state_root, plugin_id),
            plugin_config_state_path(&self.state.plugin_state_root, plugin_id),
        ];
        let state_snapshots = state_paths
            .iter()
            .map(|path| read_optional_plugin_state_file(path).map(|bytes| (path.clone(), bytes)))
            .collect::<Result<Vec<_>, _>>()?;
        // runtime_directory is canonicalized before any recursive removal authority is granted.
        // runtime_directory 在授予递归移除权限前完成规范化。
        let runtime_directory = self.state.plugin_runtime_root.join(plugin_id);
        let canonical_runtime_directory = if runtime_directory.exists() {
            let canonical_runtime_root =
                self.state
                    .plugin_runtime_root
                    .canonicalize()
                    .map_err(|error| {
                        VulcanPluginRuntimeError::new(format!(
                            "Failed to resolve Vulcan plugin runtime root: {error}"
                        ))
                    })?;
            let canonical = runtime_directory.canonicalize().map_err(|error| {
                VulcanPluginRuntimeError::new(format!(
                    "Failed to resolve Vulcan plugin runtime directory: {error}"
                ))
            })?;
            if canonical.parent() != Some(canonical_runtime_root.as_path()) {
                return Err(VulcanPluginRuntimeError::new(
                    "User plugin runtime directory is outside the exact plugin runtime root.",
                ));
            }
            Some(canonical)
        } else {
            None
        };
        let tombstone =
            canonical_user_root.join(format!(".uninstall-{}-{}", plugin_id, Uuid::new_v4()));
        fs::rename(&canonical_package, &tombstone).map_err(|error| {
            VulcanPluginRuntimeError::new(format!(
                "Failed to hide Vulcan plugin `{plugin_id}` before uninstall: {error}"
            ))
        })?;
        let uninstall_result = (|| -> Result<(), VulcanPluginRuntimeError> {
            for path in &state_paths {
                remove_plugin_state_file(path)?;
            }
            if let Some(runtime_directory) = &canonical_runtime_directory {
                fs::remove_dir_all(runtime_directory).map_err(|error| {
                    VulcanPluginRuntimeError::new(format!(
                        "Failed to remove Vulcan plugin `{plugin_id}` runtime environments: {error}"
                    ))
                })?;
            }
            fs::remove_dir_all(&tombstone).map_err(|error| {
                VulcanPluginRuntimeError::new(format!(
                    "Failed to remove Vulcan plugin `{plugin_id}` package tombstone: {error}"
                ))
            })?;
            Ok(())
        })();
        if let Err(error) = uninstall_result {
            // rollback_failures preserves every restoration diagnostic instead of masking the original failure.
            // rollback_failures 保留每条恢复诊断，避免掩盖原始失败。
            let mut rollback_failures = Vec::new();
            for (path, bytes) in &state_snapshots {
                if let Some(bytes) = bytes
                    && !path.exists()
                    && let Err(restore_error) =
                        crate::atomic_file::write_file_atomically(path, bytes)
                {
                    rollback_failures.push(restore_error);
                }
            }
            if tombstone.exists()
                && let Err(restore_error) = fs::rename(&tombstone, &canonical_package)
            {
                rollback_failures.push(format!(
                    "Failed to restore user plugin package after uninstall error: {restore_error}"
                ));
            }
            return if rollback_failures.is_empty() {
                Err(error)
            } else {
                Err(VulcanPluginRuntimeError::new(format!(
                    "{error}; uninstall rollback failed: {}",
                    rollback_failures.join("; ")
                )))
            };
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
