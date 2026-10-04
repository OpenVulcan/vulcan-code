//! Flat and managed removal share one original-state and actual environment cleanup implementation.
//! 平铺及受管移除共用单个原状态及实际环境清理实现。

use super::*;

/// Capture all three actual ordinary state preimages under the caller's actual namespace before callbacks.
/// 在调用方实际命名空间内，于回调之前捕获全部三个实际普通状态前像。
/// manager fixes roots and plugin_id is the checked original identity; returns exact bytes/absence without effects.
/// manager 固定根，plugin_id 为已检查原身份；返回精确字节／缺失，不产生效果。
pub(super) fn capture_original_states(
    manager: &VulcanPluginRuntimeManager,
    plugin_id: &str,
) -> Result<Vec<UninstallationStateFile>, VulcanPluginRuntimeError> {
    validate_state_directories(manager, plugin_id)?;
    [
        plugin_authorization_state_path(&manager.state.plugin_state_root, plugin_id),
        plugin_installation_state_path(&manager.state.plugin_state_root, plugin_id),
        plugin_config_state_path(&manager.state.plugin_state_root, plugin_id),
    ]
    .into_iter()
    .map(|path| {
        read_optional_plugin_state_file(&path).map(|original| UninstallationStateFile {
            path,
            original,
            cleanup: None,
            deletion_started: false,
        })
    })
    .collect()
}

/// Freeze every post-callback cleanup file once while retaining separate pre-callback bytes.
/// 保留独立回调前字节期间，一次性固定每个回调后清理文件。
/// states belong to the same actual original; returns unit only after all reads succeed, without partially adopting new snapshots.
/// states 属于相同实际原实例；仅全部读取成功后返回空值，不部分接纳新快照。
pub(super) fn freeze_cleanup_states(
    states: &mut [UninstallationStateFile],
) -> Result<(), VulcanPluginRuntimeError> {
    if states.iter().any(|state| state.cleanup.is_some()) {
        return Err(VulcanPluginRuntimeError::new(
            "Original uninstall cleanup state was already frozen.",
        ));
    }
    // Allocation and every actual read precede assignment, so an error leaves all original freeze slots unchanged.
    // 分配及每个实际读取先于赋值，使错误保持全部原固定槽未变。
    let cleanup = states
        .iter()
        .map(|state| read_optional_plugin_state_file(&state.path))
        .collect::<Result<Vec<_>, _>>()?;
    // Every assignment uses only the fully read snapshot set, keeping pre-callback preimages separate.
    // 每次赋值仅使用完整已读快照集合，保持回调前前像独立。
    for (state, bytes) in states.iter_mut().zip(cleanup) {
        state.cleanup = Some(bytes);
    }
    Ok(())
}

/// Validate one exact cleanup file against its explicit frozen bytes or this same original's admitted absence.
/// 对照明确固定字节或此相同原实例已准入缺失，验证单个精确清理文件。
/// state retains both preimage and cleanup intent; returns actual presence or a conflict preserving foreign bytes.
/// state 保留前像及清理意图；返回实际存在性或保留外来字节的冲突。
pub(super) fn validate_cleanup_state(
    state: &UninstallationStateFile,
) -> Result<bool, VulcanPluginRuntimeError> {
    // Outer absence means no freeze authority, while inner absence is an explicitly recorded missing file.
    // 外层缺失表示没有固定权威，而内层缺失为明确已记录缺失文件。
    let expected = state.cleanup.as_ref().ok_or_else(|| {
        VulcanPluginRuntimeError::new("Original uninstall cleanup state was not frozen.")
    })?;
    // Only actual bytes or actual absence can match the declared original cleanup protocol.
    // 仅实际字节或实际缺失能够匹配声明原清理协议。
    let current = read_optional_plugin_state_file(&state.path)?;
    if &current == expected || (state.deletion_started && current.is_none()) {
        return Ok(current.is_some());
    }
    Err(VulcanPluginRuntimeError::new(
        "Original uninstall state changed outside its exact cleanup intent.",
    ))
}

/// Remove only exact frozen files, retaining each intent before its first possibly uncertain effect.
/// 仅移除精确固定文件，在首次可能不确定效果之前保留每个意图。
/// states are the actual original and validate rechecks its retained authority before effects/readback; returns unit or a retained error.
/// states 为实际原实例，validate 在效果／回读之前重新检查保留权威；返回空值或保留错误。
pub(super) fn remove_frozen_states(
    states: &mut [UninstallationStateFile],
    validate: impl Fn() -> Result<(), VulcanPluginRuntimeError>,
) -> Result<(), VulcanPluginRuntimeError> {
    for state in states {
        validate()?;
        if validate_cleanup_state(state)? {
            state.deletion_started = true;
            remove_plugin_state_file(&state.path)?;
        }
        validate()?;
        require_absent(&state.path)?;
    }
    Ok(())
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

/// Confirm exact state and environment absence while preserving the actual environment File for terminal recovery.
/// 保留实际环境文件以供终态恢复期间，确认精确状态及环境缺失。
/// states/plugin_id/owner belong to the same original; returns unit without deleting or publishing another completion.
/// states／plugin_id／owner 属于相同原实例；返回空值，不删除或发布另一完成。
pub(super) fn confirm_local_absence(
    states: &[UninstallationStateFile],
    plugin_id: &str,
    owner: &Option<(File, PathBuf)>,
) -> Result<(), VulcanPluginRuntimeError> {
    for state in states {
        require_absent(&state.path)?;
    }
    // A missing resource wrapper is never accepted as physical environment retirement.
    // 缺失资源包装对象绝不被接受为物理环境退役。
    let (_, root) = owner.as_ref().ok_or_else(|| {
        VulcanPluginRuntimeError::new("Original environment cleanup ownership is missing.")
    })?;
    require_plain_directory(root)?;
    require_absent(&root.join(plugin_id))
}
