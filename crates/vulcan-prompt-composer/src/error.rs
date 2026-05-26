use std::error::Error;
use std::fmt::{Display, Formatter};

/// Prompt composer error kind.
/// 提示词组合器错误类型。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PromptComposerError {
    /// Missing required personality definition.
    /// 缺少必需的人格定义。
    MissingPersonality(String),
    /// Invalid caller-provided input.
    /// 调用方提供的输入无效。
    InvalidInput(String),
    /// Resource loading failed.
    /// 资源加载失败。
    ResourceLoad(String),
}

impl Display for PromptComposerError {
    /// Render the error as one readable message.
    /// 将错误渲染为可读消息。
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingPersonality(id) => {
                write!(f, "missing prompt personality definition: {id}")
            }
            Self::InvalidInput(message) => write!(f, "invalid prompt composer input: {message}"),
            Self::ResourceLoad(message) => {
                write!(f, "failed to load prompt composer resource: {message}")
            }
        }
    }
}

impl Error for PromptComposerError {}
