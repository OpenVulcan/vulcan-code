//! Deterministic project-scoped temporary-directory locators used by the desktop host.
//! 桌面宿主使用的确定性项目级临时目录定位器。

use std::path::{Path, PathBuf};

/// Shared hidden Vulcan metadata directory name stored below one project root.
/// 存放在项目根目录下的共享隐藏 Vulcan 元数据目录名称。
const PROJECT_VULCAN_DIRECTORY_NAME: &str = ".vulcan";

/// Shared temp directory name stored below one project-scoped Vulcan metadata directory.
/// 存放在项目级 Vulcan 元数据目录下的共享临时目录名称。
const PROJECT_TEMP_DIRECTORY_NAME: &str = "temp";

/// Resolves the project-scoped temp directory used by host-managed oversized tool output files.
/// 解析宿主管理超长工具输出文件使用的项目级临时目录。
///
/// # Parameters
/// 参数
/// `project_root` is the exact project root selected by the current session.
/// `project_root` 是当前会话选择的精确项目根目录。
///
/// # Returns
/// 返回值
/// The deterministic `.vulcan/temp` path without probing or enumerating the filesystem.
/// 返回无需探测或枚举文件系统即可确定的 `.vulcan/temp` 路径。
pub fn resolve_project_temp_dir(project_root: &Path) -> PathBuf {
    project_root
        .join(PROJECT_VULCAN_DIRECTORY_NAME)
        .join(PROJECT_TEMP_DIRECTORY_NAME)
}

#[cfg(test)]
mod tests {
    //! Tests for deterministic project temp location without maintenance scans.
    //! 无维护扫描的确定性项目临时目录定位测试。

    use super::*;

    /// Verifies the locator uses the fixed hidden Vulcan subtree without creating it.
    /// 验证定位器使用固定隐藏 Vulcan 子树且不会创建该目录。
    #[test]
    fn resolve_project_temp_dir_uses_hidden_vulcan_subtree_without_io() {
        let project_root = PathBuf::from("project-a");
        let expected = project_root.join(".vulcan").join("temp");

        assert_eq!(resolve_project_temp_dir(&project_root), expected);
    }
}
