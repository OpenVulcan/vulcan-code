//! Global memory mode and injection budget, stored in `memory/settings.json`.
//! 全局记忆模式与注入预算，存放于 `memory/settings.json`。

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::{MemoryError, SETTINGS_FORMAT_VERSION, store::write_atomically};

/// Which long-term memory system is active for every project.
/// 对所有项目生效的长期记忆系统。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryMode {
    /// No long-term memory.
    /// 不使用长期记忆。
    Off,
    /// VulcanMemoryMesh; the runtime default when no settings file exists.
    /// VulcanMemoryMesh；不存在设置文件时的运行时默认值。
    #[default]
    Vmm,
    /// Markdown memory files written by the AI.
    /// 由 AI 写入的 Markdown 记忆文件。
    Markdown,
}

/// Token budget of each injected part.
/// 各注入部分的 Token 预算。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InjectionBudget {
    /// User profile, injected in full.
    /// 用户画像，全文注入。
    pub user: usize,
    /// Space base section, injected in full.
    /// 空间基础分区，全文注入。
    pub space_base: usize,
    /// Project base section, injected in full.
    /// 项目基础分区，全文注入。
    pub project_base: usize,
    /// Space catalog, injected as an outline.
    /// 空间目录，以大纲注入。
    pub space_catalog: usize,
    /// Project catalog, injected as an outline.
    /// 项目目录，以大纲注入。
    pub project_catalog: usize,
}

impl InjectionBudget {
    /// Largest budget accepted for one part.
    /// 单个分项可接受的最大预算。
    pub const MAX_PART_TOKENS: usize = 16_000;

    /// Returns the sum of every part.
    /// 返回全部分项之和。
    pub const fn total(&self) -> usize {
        self.user + self.space_base + self.project_base + self.space_catalog + self.project_catalog
    }

    /// Rejects parts above [`Self::MAX_PART_TOKENS`].
    /// 拒绝超过 [`Self::MAX_PART_TOKENS`] 的分项。
    pub fn validate(&self) -> Result<(), MemoryError> {
        let parts = [
            self.user,
            self.space_base,
            self.project_base,
            self.space_catalog,
            self.project_catalog,
        ];
        if parts.iter().any(|part| *part > Self::MAX_PART_TOKENS) {
            return Err(MemoryError::invalid(format!(
                "Each memory injection budget part must be at most {} tokens.",
                Self::MAX_PART_TOKENS
            )));
        }
        Ok(())
    }
}

impl Default for InjectionBudget {
    fn default() -> Self {
        Self {
            user: 500,
            space_base: 400,
            project_base: 600,
            space_catalog: 1_000,
            project_catalog: 1_500,
        }
    }
}

/// Persisted memory settings.
/// 持久化的记忆设置。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MarkdownMemorySettings {
    /// Active memory mode.
    /// 当前记忆模式。
    pub mode: MemoryMode,
    /// Injection budget used in Markdown mode.
    /// Markdown 模式使用的注入预算。
    pub budget: InjectionBudget,
}

/// On-disk envelope carrying the format version.
/// 携带格式版本的磁盘封装。
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SettingsFile {
    /// Format version; only [`SETTINGS_FORMAT_VERSION`] is read.
    /// 格式版本；只读取 [`SETTINGS_FORMAT_VERSION`]。
    version: u32,
    /// Active memory mode.
    /// 当前记忆模式。
    mode: MemoryMode,
    /// Injection budget.
    /// 注入预算。
    budget: InjectionBudget,
}

impl MarkdownMemorySettings {
    /// Settings file name inside the memory directory.
    /// 记忆目录中的设置文件名。
    pub const FILE_NAME: &'static str = "settings.json";

    /// Loads settings; a missing file yields the in-memory defaults and is not written back.
    /// 加载设置；文件不存在时返回内存默认值，且不回写。
    pub fn load(memory_directory: &Path) -> Result<Self, MemoryError> {
        let path = memory_directory.join(Self::FILE_NAME);
        let text = match std::fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default());
            }
            Err(error) => return Err(MemoryError::io(&path, "read", error)),
        };
        let version = serde_json::from_str::<serde_json::Value>(&text)
            .ok()
            .and_then(|value| value.get("version").and_then(serde_json::Value::as_u64));
        if version != Some(u64::from(SETTINGS_FORMAT_VERSION)) {
            return Err(MemoryError::UnsupportedFormat(format!(
                "Memory settings `{}` must declare format version {SETTINGS_FORMAT_VERSION}.",
                path.display()
            )));
        }
        let file: SettingsFile =
            serde_json::from_str(&text).map_err(|error| MemoryError::Json {
                path: path.display().to_string(),
                message: error.to_string(),
            })?;
        file.budget.validate()?;
        Ok(Self {
            mode: file.mode,
            budget: file.budget,
        })
    }

    /// Saves settings atomically after validating the budget.
    /// 校验预算后原子保存设置。
    pub fn save(&self, memory_directory: &Path) -> Result<(), MemoryError> {
        self.budget.validate()?;
        let path = memory_directory.join(Self::FILE_NAME);
        let file = SettingsFile {
            version: SETTINGS_FORMAT_VERSION,
            mode: self.mode,
            budget: self.budget,
        };
        let mut text = serde_json::to_string_pretty(&file).map_err(|error| MemoryError::Json {
            path: path.display().to_string(),
            message: error.to_string(),
        })?;
        text.push('\n');
        write_atomically(&path, &text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A missing file yields defaults without creating it; saved settings load back; other versions are rejected.
    /// 缺失文件返回默认值且不创建；保存的设置可读回；其他版本被拒绝。
    #[test]
    fn load_defaults_round_trip_and_version_gate() {
        let root = tempfile::tempdir().expect("temp dir");
        let loaded = MarkdownMemorySettings::load(root.path()).expect("defaults");
        assert_eq!(loaded.mode, MemoryMode::Vmm);
        assert!(!root.path().join(MarkdownMemorySettings::FILE_NAME).exists());

        let settings = MarkdownMemorySettings {
            mode: MemoryMode::Markdown,
            budget: InjectionBudget {
                user: 100,
                ..InjectionBudget::default()
            },
        };
        settings.save(root.path()).expect("save");
        assert_eq!(
            MarkdownMemorySettings::load(root.path()).expect("load"),
            settings
        );

        std::fs::write(
            root.path().join(MarkdownMemorySettings::FILE_NAME),
            r#"{"version":2,"mode":"off","budget":{"user":1,"spaceBase":1,"projectBase":1,"spaceCatalog":1,"projectCatalog":1}}"#,
        )
        .expect("write");
        assert!(matches!(
            MarkdownMemorySettings::load(root.path()),
            Err(MemoryError::UnsupportedFormat(_))
        ));
    }
}
