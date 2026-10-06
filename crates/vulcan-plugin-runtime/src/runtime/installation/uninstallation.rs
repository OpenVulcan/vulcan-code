//! Managed deactivation retains its local environment cleanup ownership across uncertain effects.
//! 受管停用跨不确定效果保留其本地环境清理所有权。

use super::*;
use std::fs::File;
use std::sync::{Arc, Weak};

mod deactivation;
mod local_state;
pub use deactivation::{
    VulcanPluginDeactivationStateObservation, VulcanPluginDeactivationStateReservation,
};
use local_state::{
    collect_original_environment, confirm_environment_absence, retain_environment_cleanup,
    validate_environment_cleanup,
};

#[cfg(test)]
mod observation;

/// Inspect literal directory presence; path must be an exact declared original and returns only plain presence or actual NotFound.
/// 检查字面目录存在性；path 必须为精确声明原路径，仅返回普通存在或实际不存在。
fn plain_directory_presence(path: &Path) -> Result<bool, VulcanPluginRuntimeError> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(VulcanPluginRuntimeError::new(error.to_string())),
        Ok(metadata)
            if !crate::metadata_is_link_or_reparse_point(&metadata) && metadata.is_dir() =>
        {
            Ok(true)
        }
        Ok(_) => Err(VulcanPluginRuntimeError::new(
            "Original uninstall path is not a plain directory.",
        )),
    }
}

/// Require the exact literal ordinary directory; path is host-derived and returns unit or an explicit absence/type error.
/// 要求精确字面普通目录；path 由宿主推导，返回空值或明确缺失／类型错误。
fn require_plain_directory(path: &Path) -> Result<(), VulcanPluginRuntimeError> {
    if !plain_directory_presence(path)? {
        return Err(VulcanPluginRuntimeError::new(
            "Original uninstall directory is missing.",
        ));
    }
    Ok(())
}

/// Reuse the actual sequencing-only gate for retained-original recovery without repairing data locks.
/// 为保留原实例恢复复用实际仅排序门，不修复数据锁。
/// manager supplies Mutex<()> and validate_original proves its exact retained owner/phase; returns the actual guard or explicit error.
/// manager 提供空元组互斥锁，validate_original 证明精确保留所有者／阶段；返回实际守卫或明确错误。
fn lock_uninstallation_gate(
    manager: &VulcanPluginRuntimeManager,
    validate_original: impl FnOnce() -> Result<(), VulcanPluginRuntimeError>,
) -> Result<std::sync::MutexGuard<'_, ()>, VulcanPluginRuntimeError> {
    match manager.state.session_registration_gate.lock() {
        Ok(guard) => Ok(guard),
        Err(poisoned) => {
            // Retain the actual poisoned guard while checking this exact original; no replacement lock or fresh owner is created.
            // 检查此精确原实例期间保留实际污染守卫；不创建替代锁或新所有者。
            let guard = poisoned.into_inner();
            // The actual data-bearing catalog must still be readable; its poison is not sequencing-only and is never cleared.
            // 实际携带数据目录必须仍可读；其污染并非仅排序污染，绝不被清除。
            drop(current_uninstallation_snapshot(manager)?);
            validate_original()?;
            manager.state.session_registration_gate.clear_poison();
            Ok(guard)
        }
    }
}

/// Read the actual catalog for removal without the ordinary snapshot API's poisoned-lock observation behavior.
/// 为移除读取实际目录，不采用普通快照接口对污染锁的观测行为。
/// manager is the original host instance; returns a stable actual Arc or an explicit data-integrity error, never clears poison.
/// manager 为原宿主实例；返回稳定实际引用计数对象或明确数据完整性错误，绝不清除污染。
fn current_uninstallation_snapshot(
    manager: &VulcanPluginRuntimeManager,
) -> Result<Arc<crate::VulcanPluginCatalogSnapshot>, VulcanPluginRuntimeError> {
    manager
        .state
        .snapshot
        .read()
        .map(|snapshot| Arc::clone(&snapshot))
        .map_err(|_| {
            VulcanPluginRuntimeError::new("Original uninstallation catalog lock is poisoned.")
        })
}
