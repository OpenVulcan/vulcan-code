//! Managed removal owns one actual environment cleanup lease and its fixed canonical root.
//! 受管移除拥有单个实际环境清理租约及其固定规范根。

use super::*;

/// Confirm literal absence using metadata, treating dangling links and access errors as conflicts.
/// 使用元数据确认字面缺失，将悬空链接及访问错误视为冲突。
/// path is the original fixed destination; returns unit only for actual NotFound.
/// path 为原固定目标；仅实际不存在时返回空值。
fn require_absent(path: &Path) -> Result<(), VulcanPluginRuntimeError> {
    match fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(VulcanPluginRuntimeError::new(format!(
            "Failed to inspect original environment path: {error}"
        ))),
        Ok(_) => Err(VulcanPluginRuntimeError::new(format!(
            "Original environment path still exists: `{}`.",
            path.display()
        ))),
    }
}

/// Validate every owned descendant before recursive removal.
/// 递归删除前校验每个自有下级。
/// root is an original child of boundary; returns unit after removal or explicit rejection preserving evidence.
/// root 为 boundary 的原子级；删除后返回空值，否则明确拒绝并保留证据。
fn remove_plain_owned_tree(root: &Path, boundary: &Path) -> Result<(), VulcanPluginRuntimeError> {
    match fs::symlink_metadata(root) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => {
            return Err(VulcanPluginRuntimeError::new(format!(
                "Failed to inspect original environment tree: {error}"
            )));
        }
        Ok(metadata)
            if crate::metadata_is_link_or_reparse_point(&metadata) || !metadata.is_dir() =>
        {
            return Err(VulcanPluginRuntimeError::new(
                "Original environment tree is not a plain directory.",
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
            "Original environment tree escaped its fixed runtime root.",
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
                    "Original environment descendant is not a plain file or directory.",
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

/// Retain the original actual exclusive environment File and canonical root before any irreversible deletion.
/// 在任何不可逆删除之前保留原实际排他环境文件及规范根。
/// manager/plugin_id select the same physical lease as readers; owner is kept in place across failures and returns unit after validation.
/// manager／plugin_id 选择与读者相同物理租约；owner 跨失败原地保留，验证后返回空值。
pub(super) fn retain_environment_cleanup(
    manager: &VulcanPluginRuntimeManager,
    plugin_id: &str,
    owner: &mut Option<(File, PathBuf)>,
) -> Result<(), VulcanPluginRuntimeError> {
    if owner.is_none() {
        // Original callbacks must retire first because they may themselves retain the same shared environment File.
        // 原回调必须先退役，因为其自身可能保留相同共享环境文件。
        let lease = crate::runtime::environment::lock_plugin_execution_environment_cleanup(
            &manager.state,
            plugin_id,
        )
        .map_err(VulcanPluginRuntimeError::new)?;
        // Store canonical authority with the actual lease before any effect can be admitted.
        // 在任何效果能够准入之前，将规范权威与实际租约共同存储。
        let root = manager
            .state
            .plugin_runtime_root
            .canonicalize()
            .map_err(|error| VulcanPluginRuntimeError::new(error.to_string()))?;
        *owner = Some((lease, root));
    }
    validate_environment_cleanup(manager, owner)
}

/// Validate the same literal host root and retained actual environment File without acquiring replacement ownership.
/// 验证相同字面宿主根及保留实际环境文件，不获取替代所有权。
/// manager and owner are actual originals; returns unit or a redirection/missing-owner error.
/// manager 及 owner 为实际原实例；返回空值或重定向／缺失所有者错误。
pub(super) fn validate_environment_cleanup(
    manager: &VulcanPluginRuntimeManager,
    owner: &Option<(File, PathBuf)>,
) -> Result<(), VulcanPluginRuntimeError> {
    // A canonical path alone cannot substitute for the actual exclusive File.
    // 仅规范路径不能替代实际排他文件。
    let (_, root) = owner.as_ref().ok_or_else(|| {
        VulcanPluginRuntimeError::new("Original environment cleanup owner is missing.")
    })?;
    require_plain_directory(&manager.state.plugin_runtime_root)?;
    if manager
        .state
        .plugin_runtime_root
        .canonicalize()
        .map_err(|error| VulcanPluginRuntimeError::new(error.to_string()))?
        != *root
    {
        return Err(VulcanPluginRuntimeError::new(
            "Original environment cleanup boundary changed.",
        ));
    }
    Ok(())
}

/// Collect only the original plugin child under its retained actual environment File and fixed canonical root.
/// 仅在保留实际环境文件及固定规范根内回收原插件子级。
/// manager/plugin_id/owner select actual originals and validate rechecks source authority before deletion; returns unit or retained failure.
/// manager／plugin_id／owner 选择实际原实例，validate 在删除前重新检查来源权威；返回空值或保留失败。
pub(super) fn collect_original_environment(
    manager: &VulcanPluginRuntimeManager,
    plugin_id: &str,
    owner: &Option<(File, PathBuf)>,
    validate: impl Fn() -> Result<(), VulcanPluginRuntimeError>,
) -> Result<(), VulcanPluginRuntimeError> {
    validate_environment_cleanup(manager, owner)?;
    validate()?;
    // The same actual retained tuple derives the only allowed deletion child.
    // 相同实际保留元组推导唯一允许删除子级。
    let (_, root) = owner.as_ref().ok_or_else(|| {
        VulcanPluginRuntimeError::new("Original environment cleanup owner is missing.")
    })?;
    remove_plain_owned_tree(&root.join(plugin_id), root)
}

/// Confirm the original environment child is absent while preserving the actual environment File for terminal recovery.
/// 保留实际环境文件以供终态恢复期间，确认原环境子级缺失。
/// plugin_id/owner belong to the same original; returns unit without deleting or publishing another completion.
/// plugin_id／owner 属于相同原实例；返回空值，不删除或发布另一完成。
pub(super) fn confirm_environment_absence(
    plugin_id: &str,
    owner: &Option<(File, PathBuf)>,
) -> Result<(), VulcanPluginRuntimeError> {
    // A missing resource wrapper is never accepted as physical environment retirement.
    // 缺失资源包装对象绝不被接受为物理环境退役。
    let (_, root) = owner.as_ref().ok_or_else(|| {
        VulcanPluginRuntimeError::new("Original environment cleanup ownership is missing.")
    })?;
    require_plain_directory(root)?;
    require_absent(&root.join(plugin_id))
}
