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

#[cfg(test)]
mod tests {
    use super::*;

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
