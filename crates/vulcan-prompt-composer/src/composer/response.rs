use crate::composer::diagnostics::PromptComposerDiagnostics;
use crate::types::PromptSegment;

/// Final prompt assembly output grouped by target.
/// 按目标层分组的最终提示词装配输出。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PromptAssemblyOutput {
    /// System-targeted segments.
    /// 面向系统层的片段。
    pub system_segments: Vec<PromptSegment>,
    /// Developer-targeted segments.
    /// 面向开发者层的片段。
    pub developer_segments: Vec<PromptSegment>,
    /// User-targeted segments.
    /// 面向用户层的片段。
    pub user_segments: Vec<PromptSegment>,
    /// Tool metadata segments.
    /// 工具元数据片段。
    pub tool_metadata_segments: Vec<PromptSegment>,
    /// Diagnostics payload.
    /// 诊断载荷。
    pub diagnostics: PromptComposerDiagnostics,
}

impl PromptAssemblyOutput {
    /// Join all system bodies into one stable text block.
    /// 将全部系统正文拼接为稳定文本块。
    pub fn render_system_text(&self) -> String {
        self.system_segments
            .iter()
            .map(|segment| segment.body.as_str())
            .filter(|body| !body.trim().is_empty())
            .collect::<Vec<_>>()
            .join("\n\n")
    }

    /// Join all developer bodies into one stable text block.
    /// 将全部开发者正文拼接为稳定文本块。
    pub fn render_developer_text(&self) -> String {
        self.developer_segments
            .iter()
            .map(|segment| segment.body.as_str())
            .filter(|body| !body.trim().is_empty())
            .collect::<Vec<_>>()
            .join("\n\n")
    }

    /// Join all user bodies into one stable text block.
    /// 将全部用户正文拼接为稳定文本块。
    pub fn render_user_text(&self) -> String {
        self.user_segments
            .iter()
            .map(|segment| segment.body.as_str())
            .filter(|body| !body.trim().is_empty())
            .collect::<Vec<_>>()
            .join("\n\n")
    }
}
