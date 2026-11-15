//! Availability of the directory bound to a project.
//! 项目绑定目录的可用性。

use std::path::Path;

/// Stable operation error prefix used to identify an unavailable project directory.
/// 用于识别项目目录不可用的稳定操作错误前缀。
pub const PROJECT_DIRECTORY_UNAVAILABLE: &str = "ERR_PROJECT_DIRECTORY_UNAVAILABLE";

/// Checks that path is a directory and can be opened; returns a diagnostic containing the saved path.
/// 检查 path 是否为可打开的目录，失败时返回包含原保存路径的诊断。
pub fn require_project_directory(path: &Path) -> Result<(), String> {
    std::fs::read_dir(path).map(|_| ()).map_err(|error| {
        format!(
            "{PROJECT_DIRECTORY_UNAVAILABLE}: Project directory `{}` is unavailable: {error}",
            path.display()
        )
    })
}

/// Returns the currently mounted volume identifier of path or a diagnostic containing the saved path.
/// 返回 path 当前挂载卷的标识，或者包含原保存路径的诊断。
pub fn project_volume_identity(path: &Path) -> Result<Option<String>, String> {
    vulcan_mount_table::volume_identity(path).map_err(|error| {
        format!(
            "{PROJECT_DIRECTORY_UNAVAILABLE}: Cannot identify the volume for `{}`: {error}",
            path.display()
        )
    })
}

/// Returns success if path is readable and an expected volume, when supplied, is still mounted there.
/// path 可读取且仍挂载所指定卷（若指定）时返回成功。
pub fn require_bound_project_directory(
    path: &Path,
    expected_volume: Option<&str>,
) -> Result<(), String> {
    require_project_directory(path)?;
    if let Some(expected) = expected_volume {
        validate_project_volume(
            path,
            Some(expected),
            project_volume_identity(path)?.as_deref(),
        )?;
    }
    Ok(())
}

/// Rejects a different or unidentified current volume when a previously selected identity was supplied.
/// 提供先前选择的标识时拒绝当前为不同卷或无法识别卷。
pub fn validate_project_volume(
    path: &Path,
    expected_volume: Option<&str>,
    current_volume: Option<&str>,
) -> Result<(), String> {
    if expected_volume.is_some() && expected_volume != current_volume {
        return Err(format!(
            "{PROJECT_DIRECTORY_UNAVAILABLE}: The mounted volume for `{}` has changed. Reopen the directory after restoring its volume.",
            path.display()
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Simulates an unmount leaving the original path readable on an underlying or replacement volume.
    /// 模拟卸载后原路径在底层卷或替代卷上仍然可读。
    #[test]
    fn readable_mountpoint_rejects_a_replacement_volume() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path();
        require_project_directory(path).unwrap();
        let error = validate_project_volume(path, Some("usb-a"), Some("host")).unwrap_err();
        assert!(error.starts_with(PROJECT_DIRECTORY_UNAVAILABLE));
        assert!(error.contains(&path.display().to_string()));
        assert!(validate_project_volume(path, Some("usb-a"), None).is_err());
        assert!(validate_project_volume(path, Some("usb-a"), Some("usb-b")).is_err());
        assert!(validate_project_volume(path, Some("usb-a"), Some("usb-a")).is_ok());
    }

    /// A removed directory stays unavailable until the same path is restored.
    /// 已移除的目录在原路径恢复后才重新可用。
    #[test]
    fn deletion_and_restore_use_the_saved_path() {
        // Keep the test root alive across deletion and restoration of the project.
        // 在项目目录删除和恢复期间保留测试根目录。
        let temp = tempfile::tempdir().unwrap();
        let project = temp.path().join("project");
        std::fs::create_dir(&project).unwrap();
        assert!(require_project_directory(&project).is_ok());
        std::fs::remove_dir(&project).unwrap();
        let error = require_project_directory(&project).unwrap_err();
        assert!(error.starts_with(PROJECT_DIRECTORY_UNAVAILABLE));
        assert!(error.contains(&project.display().to_string()));
        std::fs::write(&project, b"file").unwrap();
        assert!(require_project_directory(&project).is_err());
        std::fs::remove_file(&project).unwrap();
        std::fs::create_dir(&project).unwrap();
        assert!(require_project_directory(&project).is_ok());
    }
}
