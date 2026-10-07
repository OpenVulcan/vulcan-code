use std::fs;
use std::path::Path;

use crate::error::PromptComposerError;
use vulcan_runtime_assets::text_decode::decode_complete_user_text;

/// Shared text-resource loader used by prompt composition sources.
/// 提示词组合来源共享的文本资源加载器。
#[derive(Debug, Default, Clone, Copy)]
pub struct TextPromptResourceLoader;

impl TextPromptResourceLoader {
    /// Reads one complete user-authored prompt file as UTF-8 or GB18030, including GBK.
    /// 将一个完整的用户提示词文件读取为 UTF-8 或 GB18030（包含 GBK）。
    ///
    /// Parameters: `path` identifies the exact external text resource.
    /// Returns: normalized Unicode text or a resource-load error that includes the source path.
    /// 参数：`path` 标识精确的外部文本资源。
    /// 返回值：规范化后的 Unicode 文本，或包含来源路径的资源加载错误。
    pub(crate) fn read_text_file(path: &Path) -> Result<String, PromptComposerError> {
        let bytes = fs::read(path).map_err(|error| {
            PromptComposerError::ResourceLoad(format!(
                "{}: {error}",
                vulcan_runtime_assets::native_host_path_text_from_path(path)
            ))
        })?;
        decode_complete_user_text(&bytes)
            .map(|decoded| decoded.content)
            .map_err(|error| {
                PromptComposerError::ResourceLoad(format!(
                    "{}: {error}",
                    vulcan_runtime_assets::native_host_path_text_from_path(path)
                ))
            })
    }
}
