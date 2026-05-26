use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use crate::error::PromptComposerError;
use crate::personality::defaults::{
    DEFAULT_PERSONALITY_ID, FRIENDLY_PERSONALITY_TEXT, PRAGMATIC_PERSONALITY_TEXT,
};
use crate::source::hashing::stable_text_hash;
use crate::types::PersonalitySelection;

/// One resolved personality definition.
/// 一个已解析的人格定义。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersonalityDefinition {
    /// Stable identifier.
    /// 稳定标识。
    pub id: String,
    /// Human-readable label.
    /// 可读标签。
    pub label: String,
    /// Final personality text.
    /// 最终人格文本。
    pub text: String,
    /// Stable text fingerprint.
    /// 稳定文本指纹。
    pub fingerprint: u64,
    /// Whether the definition is built in.
    /// 该定义是否为内置项。
    pub builtin: bool,
    /// Human-readable source label for diagnostics.
    /// 供诊断使用的人类可读来源标签。
    pub source_label: String,
}

impl PersonalityDefinition {
    /// Build one built-in personality definition.
    /// 构建一个内置人格定义。
    pub fn builtin(id: &str, label: &str, text: &str) -> Self {
        Self {
            id: id.to_string(),
            label: label.to_string(),
            text: text.trim().to_string(),
            fingerprint: stable_text_hash(text.trim()),
            builtin: true,
            source_label: format!("Built-in personality `{id}`"),
        }
    }
}

/// Personality catalog used by the composer.
/// 组合器使用的人格目录。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersonalityCatalog {
    /// Default personality identifier.
    /// 默认人格标识。
    pub default_id: String,
    /// All known personality definitions.
    /// 全部已知人格定义。
    pub definitions: BTreeMap<String, PersonalityDefinition>,
}

impl PersonalityCatalog {
    /// Build the built-in default catalog.
    /// 构建内置默认目录。
    pub fn built_in() -> Self {
        let friendly =
            PersonalityDefinition::builtin("friendly", "Friendly", FRIENDLY_PERSONALITY_TEXT);
        let pragmatic =
            PersonalityDefinition::builtin("pragmatic", "Pragmatic", PRAGMATIC_PERSONALITY_TEXT);
        let definitions = BTreeMap::from([
            (friendly.id.clone(), friendly),
            (pragmatic.id.clone(), pragmatic),
        ]);
        Self {
            default_id: DEFAULT_PERSONALITY_ID.to_string(),
            definitions,
        }
    }

    /// Load one catalog by overlaying directory personalities on top of built-ins.
    /// 在内置人格之上叠加目录人格来加载一个目录。
    pub fn load_with_directory(directory: &Path) -> Result<Self, PromptComposerError> {
        let mut catalog = Self::built_in();
        if !directory.exists() {
            return Ok(catalog);
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
            let definition = PersonalityDefinition {
                id: stem.to_string(),
                label: stem.to_string(),
                text: trimmed.to_string(),
                fingerprint: stable_text_hash(trimmed),
                builtin: false,
                source_label: format!("User personality resource `{}`", path.display()),
            };
            catalog
                .definitions
                .insert(definition.id.clone(), definition);
        }
        Ok(catalog)
    }

    /// Resolve one caller-requested personality selection.
    /// 解析调用方请求的人格选择。
    pub fn resolve(
        &self,
        selection: &PersonalitySelection,
    ) -> Result<Option<PersonalityDefinition>, PromptComposerError> {
        match selection {
            PersonalitySelection::Disabled => Ok(None),
            PersonalitySelection::Default => self
                .definitions
                .get(&self.default_id)
                .cloned()
                .map(Some)
                .ok_or_else(|| PromptComposerError::MissingPersonality(self.default_id.clone())),
            PersonalitySelection::Named(id) => self
                .definitions
                .get(id)
                .cloned()
                .map(Some)
                .ok_or_else(|| PromptComposerError::MissingPersonality(id.clone())),
            PersonalitySelection::Inline { id, text } => Ok(Some(PersonalityDefinition {
                id: id.clone(),
                label: id.clone(),
                text: text.trim().to_string(),
                fingerprint: stable_text_hash(text.trim()),
                builtin: false,
                source_label: format!("Inline personality `{id}`"),
            })),
        }
    }
}
