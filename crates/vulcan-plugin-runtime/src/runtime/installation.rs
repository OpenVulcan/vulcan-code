//! Retained user-plugin installation and legacy uninstallation.
//! 保留原所有权的用户插件安装及旧卸载。

use super::authorization::{
    VulcanPluginPermissionPreview, authorization_record_for_descriptor, build_permission_preview,
    plugin_authorization_state_path,
};
use super::configuration::plugin_config_state_path;
use super::environment::remove_plugin_execution_environment_generation;
use super::{VulcanPluginRuntimeError, VulcanPluginRuntimeManager, copy_plugin_directory_tree};
use crate::{
    VulcanPluginDependencyInstallMode, VulcanPluginPackageOrigin, VulcanPluginRuntimeState,
    inspect_user_plugin_package,
};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

mod retained;
pub use retained::VulcanPluginInstallationTransaction;
mod uninstallation;
pub use uninstallation::{
    VulcanPluginDeactivationStateObservation, VulcanPluginDeactivationStateReservation,
    VulcanPluginLegacyUninstallationObservation, VulcanPluginLegacyUninstallationReservation,
};

/// Per-file opaque state snapshot budget bounds original/cleanup preimages without borrowing another protocol's JSON metadata limit.
/// 每文件不透明状态快照额度约束原／清理前像，不借用其他协议的 JSON 元数据上限。
/// Oversized or concurrently growing files fail before mutation; all snapshot callers derive their read bound from this sole value.
/// 超限或并发增长文件在变更之前失败；全部快照调用方从此唯一值派生读取边界。
const MAX_PLUGIN_STATE_SNAPSHOT_BYTES: u64 = 16 * 1024 * 1024;

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
/// The sole per-file snapshot budget bounds allocation; oversized or concurrently growing files return an error without truncation.
/// 唯一每文件快照额度约束分配；超限或并发增长文件返回错误，不截断。
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
    if metadata.len() > MAX_PLUGIN_STATE_SNAPSHOT_BYTES {
        return Err(VulcanPluginRuntimeError::new(
            "Plugin state file exceeds the original snapshot budget.",
        ));
    }
    // The actual opened file is also checked; its bounded read rejects growth rather than allocating an unbounded snapshot.
    // 实际已打开文件也被检查；有界读取拒绝增长，而非分配无上限快照。
    let file = fs::File::open(path).map_err(|error| {
        VulcanPluginRuntimeError::new(format!(
            "Failed to open plugin state file `{}`: {error}",
            path.display()
        ))
    })?;
    // File-handle metadata is authoritative for type and current size, independently of the earlier path observation.
    // 文件句柄元数据为类型及当前大小的权威，独立于此前路径观测。
    let actual = file
        .metadata()
        .map_err(|error| VulcanPluginRuntimeError::new(error.to_string()))?;
    if !actual.is_file()
        || crate::metadata_is_link_or_reparse_point(&actual)
        || actual.len() > MAX_PLUGIN_STATE_SNAPSHOT_BYTES
    {
        return Err(VulcanPluginRuntimeError::new(
            "Opened plugin state file is not a plain file within the original snapshot budget.",
        ));
    }
    // Retain at most the sole allowed snapshot plus one overflow byte, even if the actual file grows during readback.
    // 即使实际文件在回读期间增长，最多保留唯一允许快照及单个溢出字节。
    let mut bytes = Vec::new();
    file.take(MAX_PLUGIN_STATE_SNAPSHOT_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| {
            VulcanPluginRuntimeError::new(format!(
                "Failed to read plugin state file `{}`: {error}",
                path.display()
            ))
        })?;
    if bytes.len() as u64 > MAX_PLUGIN_STATE_SNAPSHOT_BYTES {
        return Err(VulcanPluginRuntimeError::new(
            "Plugin state file grew beyond the original snapshot budget.",
        ));
    }
    Ok(Some(bytes))
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
        approved_permissions: Vec<String>,
        allow_registry_dependencies: bool,
    ) -> Result<VulcanPluginInstallationResult, VulcanPluginRuntimeError> {
        // The synchronous compatibility entry uses the same original borrowed transaction.
        // 同步兼容入口使用相同原借用事务。
        let mut transaction = self.reserve_local_user_plugin_installation(
            source_root,
            expected_content_fingerprint,
            approved_permissions,
            allow_registry_dependencies,
        )?;
        match self.publish_local_user_plugin_installation(&mut transaction) {
            Ok(result) => Ok(result),
            Err(error) => match self.abort_local_user_plugin_installation(&mut transaction) {
                Ok(()) => Err(error),
                Err(cleanup) => Err(VulcanPluginRuntimeError::new(format!(
                    "{error}; original installation compensation failed: {cleanup}"
                ))),
            },
        }
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
        self.require_routing_publication_locked(&_registration, None)?;
        let snapshot = self.snapshot();
        let plugin = snapshot.plugins.get(plugin_id).ok_or_else(|| {
            VulcanPluginRuntimeError::new(format!("Vulcan plugin `{plugin_id}` is not installed."))
        })?;
        if plugin.origin != VulcanPluginPackageOrigin::UserInstalled {
            return Err(VulcanPluginRuntimeError::new(format!(
                "Bundled Vulcan plugin `{plugin_id}` cannot be uninstalled."
            )));
        }
        if self.active_session_count(plugin_id)? != 0
            || self.has_cleanup_diagnostics(plugin_id)?
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
        // Every flat removal cooperates with retained install, configuration and credential owners.
        // 每次平铺移除均与保留安装、配置及凭据所有者协作。
        let store = crate::VulcanPluginGenerationStore::open(&self.state.user_plugin_root)
            .map_err(|error| VulcanPluginRuntimeError::new(error.to_string()))?;
        // This actual File stays held through capacity retirement, package removal and compensation.
        // 此实际文件跨容量退役、插件包移除及补偿持续持有。
        let _mutation = store
            .acquire_legacy_mutation_guard(plugin_id, None)
            .map_err(|error| VulcanPluginRuntimeError::new(error.to_string()))?;
        _mutation
            .validate_user_root(&self.state.user_plugin_root, plugin_id)
            .map_err(|error| VulcanPluginRuntimeError::new(error.to_string()))?;
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
        // Keep uninstall outside all live environment use before any package or configuration mutation.
        // 在任何插件包或配置变更前，使卸载避开全部存续环境使用。
        let _environment_cleanup = if self.state.plugin_runtime_root.exists() {
            Some(
                super::environment::lock_plugin_execution_environment_cleanup(
                    &self.state,
                    plugin_id,
                )
                .map_err(VulcanPluginRuntimeError::new)?,
            )
        } else {
            None
        };
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
        // The caller completes uninstall callbacks first; refuse removal while any formal capacity member still owns the package.
        // 调用方先完成卸载回调；任何正式容量成员仍持有包时拒绝移除。
        if let Some(owner) = self.formal_runtime_owner()?
            && let Err(error) = owner.retire_plugin_capacities_classified(plugin_id, &_registration)
        {
            // Only proven unfinished preparation or native members may be reconciled after physical retirement.
            // 只有已证未完成准备或原生成员可在物理退役后对账。
            let pending = error.is_pending();
            // Return the original rejection without proceeding to package removal.
            // 返回原拒绝，不继续移除包。
            let error = error.into_runtime_error();
            if pending {
                self.record_cleanup_pending_locked(plugin_id, error.to_string(), &_registration)?;
            } else {
                self.record_cleanup_failure_locked(plugin_id, error.to_string(), &_registration)?;
            }
            return Err(error);
        }
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
