use std::path::PathBuf;

/// Resource directories used to build one prompt composer engine.
/// 用于构建提示词组合引擎的资源目录。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PromptComposerResourceConfig {
    /// Optional custom personality directory.
    /// 可选自定义人格目录。
    pub personality_directory: Option<PathBuf>,
    /// Optional custom tool-help directory.
    /// 可选自定义工具帮助目录。
    pub tool_help_directory: Option<PathBuf>,
}
