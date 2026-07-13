//! Workspace-path normalization helpers shared by the service and runtime prompt builders.
//! 服务与运行时提示词构建器共享的工作区路径规范化辅助模块。

use std::fs;
use std::path::{Path, PathBuf};

/// Resolves one workspace path to its canonical filesystem location when the host platform can do so.
/// 当宿主平台能够解析时，将一个工作区路径解析到其规范文件系统位置。
pub(crate) fn canonicalize_workspace_path(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

/// Converts one filesystem path buffer into the normalized workspace text shown to users and providers.
/// 将一个文件系统路径缓冲区转换为展示给用户与 Provider 的规范工作区文本。
pub(crate) fn normalized_workspace_path_text_from_path(path: &Path) -> String {
    normalize_workspace_path_text(&path.to_string_lossy())
}

/// Converts one filesystem path buffer into the native host path text used for host-managed runtime injections.
/// 将一个文件系统路径缓冲区转换为用于宿主管理运行时注入的宿主原生路径文本。
pub(crate) fn native_workspace_path_text_from_path(path: &Path) -> String {
    native_workspace_path_text(&canonicalize_workspace_path(path).to_string_lossy())
}

/// Converts one raw workspace path string into a stable human-readable text form without Windows verbatim prefixes.
/// 将一个原始工作区路径字符串转换为稳定且可读的文本形式，并去除 Windows verbatim 前缀。
pub(crate) fn normalize_workspace_path_text(path: &str) -> String {
    native_workspace_path_text(path)
}

/// Converts one raw workspace path string into a stable native host path form without Windows verbatim prefixes.
/// 将一个原始工作区路径字符串转换为稳定的宿主原生路径形式，并去除 Windows verbatim 前缀。
fn native_workspace_path_text(path: &str) -> String {
    vulcan_runtime_assets::native_host_path_text(path)
}

#[cfg(test)]
mod tests {
    //! Focused workspace-path normalization tests.
    //! 聚焦工作区路径规范化的定向测试。

    use super::{native_workspace_path_text_from_path, normalize_workspace_path_text};
    use std::path::Path;

    /// Verifies Windows verbatim drive paths are rewritten into the standard backslash drive form used by host-visible path text.
    /// 验证 Windows verbatim 盘符路径会被改写成宿主可见路径文本使用的标准反斜杠盘符形式。
    #[test]
    fn normalizes_windows_verbatim_drive_path() {
        assert_eq!(
            normalize_workspace_path_text(r"\\?\D:\openvulcan\vulcan-code"),
            r"D:\openvulcan\vulcan-code"
        );
    }

    /// Verifies slash-normalized Windows verbatim drive paths also lose the generic root prefix and return standard Windows separators.
    /// 验证已经使用斜杠表示的 Windows verbatim 盘符路径同样会移除通用根前缀并返回标准 Windows 分隔符。
    #[test]
    fn normalizes_slash_windows_verbatim_drive_path() {
        assert_eq!(
            normalize_workspace_path_text("//?/D:/openvulcan/vulcan-code"),
            r"D:\openvulcan\vulcan-code"
        );
    }

    /// Verifies Windows verbatim UNC paths are rewritten into standard UNC text with backslashes.
    /// 验证 Windows verbatim UNC 路径会被改写为带反斜杠的标准 UNC 文本。
    #[test]
    fn normalizes_windows_verbatim_unc_path() {
        assert_eq!(
            normalize_workspace_path_text(r"\\?\UNC\server\share\vulcan-code"),
            r"\\server\share\vulcan-code"
        );
    }

    /// Verifies Unix-style paths remain unchanged after normalization.
    /// 验证 Unix 风格路径在规范化后保持不变。
    #[test]
    fn preserves_unix_style_workspace_path() {
        assert_eq!(
            normalize_workspace_path_text("/Users/demo/openvulcan/vulcan-code"),
            "/Users/demo/openvulcan/vulcan-code"
        );
    }

    /// Verifies Unix double-slash paths are not mistaken for Windows UNC paths on non-Windows hosts.
    /// 验证类 Unix 宿主上的双斜杠路径不会被误判成 Windows UNC 路径。
    #[cfg(not(windows))]
    #[test]
    fn preserves_unix_double_slash_workspace_path() {
        assert_eq!(
            normalize_workspace_path_text("//server/share/openvulcan"),
            "//server/share/openvulcan"
        );
    }

    /// Verifies native workspace-path formatting keeps slash paths unchanged on Unix-like hosts.
    /// 验证宿主原生工作区路径格式化在类 Unix 宿主上会保持斜杠路径不变。
    #[cfg(not(windows))]
    #[test]
    fn native_workspace_path_preserves_unix_style_text() {
        assert_eq!(
            native_workspace_path_text_from_path(Path::new("/Users/demo/openvulcan/vulcan-code")),
            "/Users/demo/openvulcan/vulcan-code"
        );
    }

    /// Verifies native workspace-path formatting restores Windows separators for host-managed runtime injections.
    /// 验证宿主原生工作区路径格式化会为宿主管理运行时注入恢复 Windows 分隔符。
    #[cfg(windows)]
    #[test]
    fn native_workspace_path_restores_windows_separators() {
        assert_eq!(
            native_workspace_path_text_from_path(Path::new("C:/openvulcan/vulcan-code")),
            r"C:\openvulcan\vulcan-code"
        );
    }
}
