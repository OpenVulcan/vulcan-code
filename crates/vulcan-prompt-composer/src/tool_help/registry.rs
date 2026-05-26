use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use crate::error::PromptComposerError;
use crate::source::hashing::stable_text_hash;
use crate::tool_help::builtin::built_in_tool_help_entries;

/// One registered tool-help entry.
/// 一个已注册的工具帮助条目。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ToolHelpEntry {
    /// Stable tool identifier.
    /// 稳定工具标识。
    pub tool_id: String,
    /// Help body text.
    /// 帮助正文文本。
    pub body: String,
    /// Stable entry fingerprint.
    /// 稳定条目指纹。
    pub fingerprint: u64,
    /// Human-readable source label for diagnostics.
    /// 供诊断使用的人类可读来源标签。
    pub source_label: String,
}

impl ToolHelpEntry {
    /// Build one tool-help entry.
    /// 构建一个工具帮助条目。
    pub fn new(tool_id: impl Into<String>, body: impl Into<String>) -> Self {
        let tool_id = tool_id.into();
        let body = body.into();
        let fingerprint = stable_text_hash(&format!("{tool_id}|{body}"));
        Self {
            tool_id,
            body,
            fingerprint,
            source_label: String::from("Built-in tool help"),
        }
    }

    /// Attach one human-readable source label to this tool-help entry.
    /// 为该工具帮助条目附加一个人类可读来源标签。
    pub fn with_source_label(mut self, source_label: impl Into<String>) -> Self {
        self.source_label = source_label.into();
        self
    }
}

/// Registry of tool-help definitions.
/// 工具帮助定义注册表。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ToolHelpRegistry {
    /// Registered help entries by tool id.
    /// 按工具标识注册的帮助条目。
    pub entries: BTreeMap<String, ToolHelpEntry>,
}

impl ToolHelpRegistry {
    /// Build the built-in registry.
    /// 构建内置注册表。
    pub fn built_in() -> Self {
        Self::from_entries(built_in_tool_help_entries())
    }

    /// Build a registry from entries.
    /// 从条目集合构建注册表。
    pub fn from_entries(entries: impl IntoIterator<Item = ToolHelpEntry>) -> Self {
        let entries = entries
            .into_iter()
            .map(|entry| (entry.tool_id.clone(), entry))
            .collect();
        Self { entries }
    }

    /// Load one registry by overlaying directory help files on top of built-ins.
    /// 在内置帮助之上叠加目录帮助文件来加载一个注册表。
    pub fn load_with_directory(directory: &Path) -> Result<Self, PromptComposerError> {
        let mut registry = Self::built_in();
        registry.overlay_directory(directory)?;
        Ok(registry)
    }

    /// Overlay one directory of Markdown or text help files.
    /// 叠加一个 Markdown 或文本帮助文件目录。
    pub fn overlay_directory(&mut self, directory: &Path) -> Result<(), PromptComposerError> {
        if !directory.exists() {
            return Ok(());
        }
        let read_dir = fs::read_dir(directory).map_err(|error| {
            PromptComposerError::ResourceLoad(format!("{}: {error}", directory.display()))
        })?;
        for entry in read_dir {
            let entry = entry.map_err(|error| {
                PromptComposerError::ResourceLoad(format!("{}: {error}", directory.display()))
            })?;
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            let Some(stem) = path.file_stem().and_then(|value| value.to_str()) else {
                continue;
            };
            let Some(extension) = path.extension().and_then(|value| value.to_str()) else {
                continue;
            };
            if !matches!(extension, "md" | "txt") {
                continue;
            }
            let text = fs::read_to_string(&path).map_err(|error| {
                PromptComposerError::ResourceLoad(format!("{}: {error}", path.display()))
            })?;
            let trimmed = text.trim();
            if trimmed.is_empty() {
                continue;
            }
            self.entries.insert(
                stem.to_string(),
                ToolHelpEntry::new(stem.to_string(), trimmed.to_string())
                    .with_source_label(format!("User tool-help resource `{}`", path.display())),
            );
        }
        Ok(())
    }

    /// Resolve one tool-help entry by tool id.
    /// 按工具标识解析一个工具帮助条目。
    pub fn get(&self, tool_id: &str) -> Option<&ToolHelpEntry> {
        self.entries.get(tool_id)
    }
}
