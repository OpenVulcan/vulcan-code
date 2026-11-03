use serde::{Deserialize, Serialize};

/// How the WebUI opens a file outside the session's workspace, as a system default or a session
/// override. The prompt guards against accidental opens; it is not a security boundary.
/// WebUI 如何打开会话工作区之外的文件，可作为系统默认值或会话覆盖。提示用于防止误开，并非安全边界。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OutsideWorkspaceFileAccess {
    /// Outside files are not opened.
    /// 不打开工作区之外的文件。
    Deny,
    /// Each outside file asks before opening.
    /// 每个工作区之外的文件在打开前询问。
    #[default]
    Prompt,
    /// Outside files open without asking.
    /// 工作区之外的文件直接打开。
    Allow,
}
