//! User-plugin package inspection and managed deactivation local cleanup.
//! 用户插件包检查及受管停用本地清理。

use super::authorization::{VulcanPluginPermissionPreview, build_permission_preview};
use super::{VulcanPluginRuntimeError, VulcanPluginRuntimeManager};
use crate::{VulcanPluginPackageOrigin, VulcanPluginRuntimeState, inspect_user_plugin_package};
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;

mod uninstallation;
pub use uninstallation::{
    VulcanPluginDeactivationStateObservation, VulcanPluginDeactivationStateReservation,
};

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
}
