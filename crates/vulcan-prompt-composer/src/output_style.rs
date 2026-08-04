use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use crate::error::PromptComposerError;
use crate::source::hashing::stable_text_hash;
use crate::source::loader::TextPromptResourceLoader;

/// Stable identifier used when no output-style instruction is injected.
/// 不注入输出风格指令时使用的稳定标识。
pub const OUTPUT_STYLE_UNRESTRICTED_ID: &str = "unrestricted";
/// Stable identifier for the concise general-purpose output style.
/// 简洁通用输出风格使用的稳定标识。
pub const OUTPUT_STYLE_BALANCED_ID: &str = "balanced";
/// Stable identifier for the structured-report output style.
/// 分层报告输出风格使用的稳定标识。
pub const OUTPUT_STYLE_STRUCTURED_ID: &str = "structured";
/// Stable identifier for the engineering-rigorous output style.
/// 工程严谨输出风格使用的稳定标识。
pub const OUTPUT_STYLE_RIGOROUS_ID: &str = "rigorous";

/// Immutable output-style snapshot carried by one prompt boundary.
/// 由一个提示词边界携带的不可变输出风格快照。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptOutputStyleSnapshot {
    /// Stable style identifier.
    /// 稳定风格标识。
    pub id: String,
    /// Human-readable style label.
    /// 面向用户的风格名称。
    pub label: String,
    /// Exact instruction text captured at the safe boundary.
    /// 在安全边界捕获的精确指令正文。
    pub text: String,
    /// Stable fingerprint of the identifier and exact text.
    /// 标识与精确正文共同计算的稳定指纹。
    pub fingerprint: u64,
    /// Human-readable resource source label.
    /// 面向诊断的人类可读资源来源。
    pub source_label: String,
}

impl PromptOutputStyleSnapshot {
    /// Builds an immutable output-style snapshot.
    /// 构建不可变输出风格快照。
    pub fn new(
        id: impl Into<String>,
        label: impl Into<String>,
        text: impl Into<String>,
        source_label: impl Into<String>,
    ) -> Self {
        let id = id.into();
        let label = label.into();
        let text = text.into().trim().to_string();
        let fingerprint = stable_text_hash(&format!("{id}\n{text}"));
        Self {
            id,
            label,
            text,
            fingerprint,
            source_label: source_label.into(),
        }
    }

    /// Returns whether this snapshot contributes a prompt node.
    /// 返回该快照是否会产生提示词节点。
    pub fn is_injected(&self) -> bool {
        !self.text.trim().is_empty() && self.id != OUTPUT_STYLE_UNRESTRICTED_ID
    }
}

/// One catalog entry available to global and session selectors.
/// 可供全局与会话选择器使用的一个目录项。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputStyleDefinition {
    /// Stable style identifier.
    /// 稳定风格标识。
    pub id: String,
    /// Chinese display label.
    /// 中文显示名称。
    pub label_zh: String,
    /// English display label.
    /// 英文显示名称。
    pub label_en: String,
    /// Constraint level shown in the selector.
    /// 选择器中展示的约束等级。
    pub constraint_level: String,
    /// Exact instruction body, empty for unrestricted mode.
    /// 精确指令正文；不限定模式为空。
    pub text: String,
    /// Stable identifier-plus-body fingerprint.
    /// 标识与正文共同计算的稳定指纹。
    pub fingerprint: u64,
    /// Whether this entry is built into the application.
    /// 该项是否为应用内置项。
    pub builtin: bool,
    /// Human-readable resource source label.
    /// 面向诊断的人类可读资源来源。
    pub source_label: String,
}

impl OutputStyleDefinition {
    /// Builds an unrestricted catalog entry without a resource file.
    /// 构建不需要资源文件的不限定目录项。
    pub fn unrestricted() -> Self {
        Self {
            id: OUTPUT_STYLE_UNRESTRICTED_ID.to_string(),
            label_zh: String::from("不限定"),
            label_en: String::from("Unrestricted"),
            constraint_level: String::from("无"),
            text: String::new(),
            fingerprint: stable_text_hash(OUTPUT_STYLE_UNRESTRICTED_ID),
            builtin: true,
            source_label: String::from("Built-in output style unrestricted"),
        }
    }

    /// Converts a catalog entry into an immutable prompt-boundary snapshot.
    /// 将目录项转换为不可变提示词边界快照。
    pub fn snapshot(&self) -> PromptOutputStyleSnapshot {
        PromptOutputStyleSnapshot {
            id: self.id.clone(),
            label: self.label_zh.clone(),
            text: self.text.clone(),
            fingerprint: self.fingerprint,
            source_label: self.source_label.clone(),
        }
    }
}

/// Built-in and optional user-resource output-style catalog.
/// 内置及可选用户资源输出风格目录。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputStyleCatalog {
    /// Default style identifier.
    /// 默认风格标识。
    pub default_id: String,
    /// All known style definitions keyed by stable identifier.
    /// 按稳定标识索引的全部已知风格定义。
    pub definitions: BTreeMap<String, OutputStyleDefinition>,
}

impl OutputStyleCatalog {
    /// Builds the four built-in output styles.
    /// 构建四个内置输出风格。
    pub fn built_in() -> Self {
        let unrestricted = OutputStyleDefinition::unrestricted();
        let definitions = BTreeMap::from([
            (unrestricted.id.clone(), unrestricted),
            (include_style(OUTPUT_STYLE_BALANCED_ID)),
            (include_style(OUTPUT_STYLE_STRUCTURED_ID)),
            (include_style(OUTPUT_STYLE_RIGOROUS_ID)),
        ]);
        Self {
            default_id: OUTPUT_STYLE_BALANCED_ID.to_string(),
            definitions,
        }
    }

    /// Loads optional Markdown or text resources over the built-in catalog.
    /// 在内置目录之上加载可选 Markdown 或文本资源。
    pub fn load_with_directory(directory: &Path) -> Result<Self, PromptComposerError> {
        let mut catalog = Self::built_in();
        if !directory.exists() {
            return Ok(catalog);
        }
        for entry in fs::read_dir(directory).map_err(|error| {
            PromptComposerError::ResourceLoad(format!("{}: {error}", directory.display()))
        })? {
            let path = entry
                .map_err(|error| PromptComposerError::ResourceLoad(error.to_string()))?
                .path();
            if !path.is_file() {
                continue;
            }
            let Some(stem) = path.file_stem().and_then(|value| value.to_str()) else {
                continue;
            };
            let Some(extension) = path.extension().and_then(|value| value.to_str()) else {
                continue;
            };
            if !matches!(extension, "md" | "txt") || stem == OUTPUT_STYLE_UNRESTRICTED_ID {
                continue;
            }
            let text = TextPromptResourceLoader::read_text_file(&path)?;
            let trimmed = text.trim();
            if trimmed.is_empty() {
                continue;
            }
            let definition = OutputStyleDefinition {
                id: stem.to_string(),
                label_zh: stem.to_string(),
                label_en: stem.to_string(),
                constraint_level: String::from("自定义"),
                text: trimmed.to_string(),
                fingerprint: stable_text_hash(&format!("{stem}\n{trimmed}")),
                builtin: false,
                source_label: format!("User output style resource `{}`", path.display()),
            };
            catalog
                .definitions
                .insert(definition.id.clone(), definition);
        }
        Ok(catalog)
    }

    /// Resolves one style id into a boundary snapshot.
    /// 将一个风格标识解析为边界快照。
    pub fn resolve(&self, id: &str) -> Result<PromptOutputStyleSnapshot, PromptComposerError> {
        self.definitions
            .get(id)
            .map(OutputStyleDefinition::snapshot)
            .ok_or_else(|| PromptComposerError::InvalidInput(format!("unknown output style: {id}")))
    }

    /// Returns all catalog entries in stable identifier order.
    /// 按稳定标识顺序返回全部目录项。
    pub fn entries(&self) -> Vec<OutputStyleDefinition> {
        self.definitions.values().cloned().collect()
    }
}

/// Builds one built-in style from its exact packaged resource body.
/// 从精确打包资源正文构建一个内置风格。
fn include_style(id: &str) -> (String, OutputStyleDefinition) {
    let (label_zh, label_en, constraint_level, text) = match id {
        OUTPUT_STYLE_BALANCED_ID => (
            "清晰均衡",
            "Clear and Balanced",
            "基础",
            include_str!("../resources/output-styles/balanced.md"),
        ),
        OUTPUT_STYLE_STRUCTURED_ID => (
            "分层报告",
            "Structured Report",
            "增强",
            include_str!("../resources/output-styles/structured.md"),
        ),
        OUTPUT_STYLE_RIGOROUS_ID => (
            "工程严谨",
            "Engineering Rigorous",
            "严格",
            include_str!("../resources/output-styles/rigorous.md"),
        ),
        _ => unreachable!("unsupported built-in output style id"),
    };
    let text = text.trim().to_string();
    let definition = OutputStyleDefinition {
        id: id.to_string(),
        label_zh: label_zh.to_string(),
        label_en: label_en.to_string(),
        constraint_level: constraint_level.to_string(),
        fingerprint: stable_text_hash(&format!("{id}\n{text}")),
        text,
        builtin: true,
        source_label: format!("Built-in output style `{id}`"),
    };
    (id.to_string(), definition)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies the built-in catalog exposes the four stable selector choices.
    /// 验证内置目录暴露四个稳定的选择项。
    #[test]
    fn built_in_catalog_has_expected_choices_and_default() {
        let catalog = OutputStyleCatalog::built_in();
        assert_eq!(catalog.default_id, OUTPUT_STYLE_BALANCED_ID);
        assert_eq!(catalog.definitions.len(), 4);
        assert!(
            !catalog
                .resolve(OUTPUT_STYLE_UNRESTRICTED_ID)
                .expect("unrestricted style should resolve")
                .is_injected()
        );
        for id in [
            OUTPUT_STYLE_BALANCED_ID,
            OUTPUT_STYLE_STRUCTURED_ID,
            OUTPUT_STYLE_RIGOROUS_ID,
        ] {
            assert!(
                catalog
                    .resolve(id)
                    .expect("built-in style should resolve")
                    .is_injected()
            );
        }
    }

    /// Verifies a snapshot preserves the exact prompt text and stable fingerprint.
    /// 验证风格快照保留精确正文与稳定指纹。
    #[test]
    fn snapshot_fingerprint_is_stable_for_exact_text() {
        let catalog = OutputStyleCatalog::built_in();
        let first = catalog
            .resolve(OUTPUT_STYLE_STRUCTURED_ID)
            .expect("structured style should resolve");
        let second = catalog
            .resolve(OUTPUT_STYLE_STRUCTURED_ID)
            .expect("structured style should resolve");
        assert_eq!(first, second);
        assert!(!first.text.is_empty());
        assert_ne!(first.fingerprint, 0);
    }
}
