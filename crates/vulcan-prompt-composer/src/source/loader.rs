use std::fs;
use std::path::Path;

use crate::error::PromptComposerError;
use crate::source::snapshot::{AgentPromptSnapshot, ProjectRulesSnapshot, ProviderPromptSnapshot};

/// Shared text-resource loader used by prompt composition sources.
/// 提示词组合来源共享的文本资源加载器。
#[derive(Debug, Default, Clone, Copy)]
pub struct TextPromptResourceLoader;

impl TextPromptResourceLoader {
    /// Read one optional UTF-8 text file and trim outer blank lines.
    /// 读取一个可选 UTF-8 文本文件并裁剪外层空白行。
    pub fn read_optional_text_file(path: &Path) -> Result<Option<String>, PromptComposerError> {
        if !path.exists() {
            return Ok(None);
        }
        let raw = fs::read_to_string(path).map_err(|error| {
            PromptComposerError::ResourceLoad(format!(
                "{}: {error}",
                vulcan_runtime_assets::native_host_path_text_from_path(path)
            ))
        })?;
        let trimmed = raw.trim().to_string();
        if trimmed.is_empty() {
            return Ok(None);
        }
        Ok(Some(trimmed))
    }

    /// Load one provider prompt snapshot from the given file when available.
    /// 在文件存在时从给定文件加载一个 Provider 提示词快照。
    pub fn load_provider_prompt(
        path: &Path,
    ) -> Result<Option<ProviderPromptSnapshot>, PromptComposerError> {
        Self::read_optional_text_file(path).map(|text| {
            text.map(|text| {
                ProviderPromptSnapshot::new(text).with_source_label(format!(
                    "Provider prompt resource `{}`",
                    vulcan_runtime_assets::native_host_path_text_from_path(path)
                ))
            })
        })
    }

    /// Load one agent prompt snapshot from the given file when available.
    /// 在文件存在时从给定文件加载一个 Agent 提示词快照。
    pub fn load_agent_prompt(
        path: &Path,
    ) -> Result<Option<AgentPromptSnapshot>, PromptComposerError> {
        Self::read_optional_text_file(path).map(|text| {
            text.map(|text| {
                AgentPromptSnapshot::new(text).with_source_label(format!(
                    "Agent prompt resource `{}`",
                    vulcan_runtime_assets::native_host_path_text_from_path(path)
                ))
            })
        })
    }

    /// Load one project-rules snapshot from the given file when available.
    /// 在文件存在时从给定文件加载一个项目规则快照。
    pub fn load_project_rules(
        path: &Path,
    ) -> Result<Option<ProjectRulesSnapshot>, PromptComposerError> {
        Self::read_optional_text_file(path).map(|text| {
            text.map(|text| {
                ProjectRulesSnapshot::new(
                    vulcan_runtime_assets::native_host_path_text_from_path(path),
                    text,
                )
            })
        })
    }
}
