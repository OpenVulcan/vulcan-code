//! Workspace file file kind helpers.
//! 工作区文件 file kind 辅助逻辑。

use super::*;

/// Classifies one workspace file path for preview eligibility.
/// 对单个工作区文件路径进行预览资格分类。
///
/// # Parameters
/// 参数
/// `path` is the file path to classify.
/// `path` 是需要分类的文件路径。
///
/// # Returns
/// 返回值
/// The file entry kind used by the backend and WebUI.
/// 返回后端与 WebUI 使用的文件条目类别。
pub(crate) fn workspace_file_kind_from_path(path: &Path) -> WorkspaceFileEntryKind {
    workspace_file_path_metadata(path).kind
}
