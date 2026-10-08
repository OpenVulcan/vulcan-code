//! Base system prompt resolution for the execution runtime.
//! 执行运行时使用的基础系统提示词解析模块。
//!
//! The effective base prompt is the first non-blank source in this order: the project override,
//! the user override, then the built-in default. Product capability guides are attached
//! separately and are never part of the editable base prompt.
//! 生效的基础提示词按以下顺序取第一个非空来源：项目覆盖、用户覆盖、内置默认。
//! 产品功能说明另行附加，不属于可编辑的基础提示词。

use crate::provider_config::{project_system_prompt_file_path, user_system_prompt_file_path};
use crate::runtime_assets;
use std::{
    path::Path,
    sync::{Mutex, OnceLock},
};
use vulcan_prompt_composer::ProjectPromptFileCache;

/// Process-wide metadata-aware cache for user and project system prompt files.
/// 用户与项目系统提示词文件共享的进程级元数据感知缓存。
static SYSTEM_PROMPT_FILE_CACHE: OnceLock<Mutex<ProjectPromptFileCache>> = OnceLock::new();

/// Layer that supplied the effective base system prompt.
/// 提供生效基础系统提示词的层级。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SystemPromptSource {
    /// Compiled-in product default.
    /// 编译进产品的默认值。
    BuiltIn,
    /// User-level override below the user Vulcan Code root.
    /// 用户 Vulcan Code 根目录下的用户级覆盖。
    User,
    /// Project-level override below the workspace `.vulcan/code/` directory.
    /// 工作区 `.vulcan/code/` 目录下的项目级覆盖。
    Project,
}

impl SystemPromptSource {
    /// Returns the stable wire identifier of this source.
    /// 返回该来源的稳定线协议标识。
    pub const fn id(self) -> &'static str {
        match self {
            Self::BuiltIn => "builtIn",
            Self::User => "user",
            Self::Project => "project",
        }
    }
}

/// Loaded base system prompt and its source.
/// 已加载的基础系统提示词及其来源。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadedProviderBaseSystemPrompt {
    /// Trimmed prompt body text.
    /// 已裁剪的提示词正文。
    pub text: String,
    /// Layer that supplied the body.
    /// 提供正文的层级。
    pub source: SystemPromptSource,
    /// Human-readable source label for diagnostics.
    /// 供诊断使用的人类可读来源标签。
    pub source_label: String,
}

/// Returns the trimmed built-in base system prompt.
/// 返回裁剪后的内置基础系统提示词。
pub fn built_in_system_prompt() -> &'static str {
    runtime_assets::DEFAULT_SYSTEM_PROMPT_MD.trim()
}

/// Returns the trimmed product capability guide attached after the base prompt.
/// 返回附加在基础提示词之后的裁剪产品功能说明。
pub fn capability_guide_prompt() -> &'static str {
    runtime_assets::CAPABILITY_GUIDE_MD.trim()
}

/// Resolves the effective base system prompt for one workspace.
/// 解析单个工作区的生效基础系统提示词。
///
/// # Parameters
/// # 参数
/// - `workspace`: Validated workspace directory, or `None` when the request has no workspace.
/// - `workspace`：已验证的工作区目录；请求没有工作区时为 `None`。
///
/// # Returns
/// # 返回值
/// Returns the first non-blank layer; an override that exists but cannot be read is an error.
/// 返回第一个非空层级；存在但无法读取的覆盖文件视为错误。
pub fn load_effective_system_prompt(
    workspace: Option<&Path>,
) -> Result<LoadedProviderBaseSystemPrompt, String> {
    if let Some(workspace) = workspace
        && let Some(loaded) = load_override(
            &project_system_prompt_file_path(workspace),
            SystemPromptSource::Project,
        )?
    {
        return Ok(loaded);
    }
    if let Some(loaded) = load_override(&user_system_prompt_file_path(), SystemPromptSource::User)?
    {
        return Ok(loaded);
    }
    Ok(LoadedProviderBaseSystemPrompt {
        text: built_in_system_prompt().to_string(),
        source: SystemPromptSource::BuiltIn,
        source_label: String::from("Built-in system prompt"),
    })
}

/// Loads one override file through the shared cache.
/// 通过共享缓存加载一个覆盖文件。
///
/// # Returns
/// # 返回值
/// `None` when the file is missing or blank.
/// 文件缺失或为空白时返回 `None`。
fn load_override(
    path: &Path,
    source: SystemPromptSource,
) -> Result<Option<LoadedProviderBaseSystemPrompt>, String> {
    let cache = SYSTEM_PROMPT_FILE_CACHE.get_or_init(|| Mutex::new(ProjectPromptFileCache::new()));
    let mut cache = cache
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let snapshot = cache.load_file(path).map_err(|error| {
        format!(
            "Failed to load system prompt from `{}`: {error}",
            path.display()
        )
    })?;
    Ok(snapshot.map(|snapshot| LoadedProviderBaseSystemPrompt {
        text: snapshot.text,
        source,
        source_label: format!("System prompt `{}`", snapshot.source),
    }))
}

#[cfg(test)]
mod tests {
    //! Base system prompt resolution tests.
    //! 基础系统提示词解析测试。

    use super::*;
    use crate::provider_config::save_system_prompt_file;
    use std::fs;
    use std::path::PathBuf;

    /// Builds one absent, process-unique workspace directory below the host temp directory.
    /// 在宿主临时目录下构建一个不存在且进程唯一的工作区目录。
    fn unique_workspace(label: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "vulcan-system-prompt-loader-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("test clock should follow the Unix epoch")
                .as_nanos()
        ))
    }

    /// Verifies the built-in and capability resources are non-empty and stay separate.
    /// 验证内置提示词与功能说明资源非空且相互独立。
    #[test]
    fn built_in_prompt_and_capability_guide_are_separate_resources() {
        assert!(!built_in_system_prompt().is_empty());
        assert!(!capability_guide_prompt().is_empty());
        assert_ne!(built_in_system_prompt(), capability_guide_prompt());
    }

    /// Verifies a project override takes precedence over every lower layer.
    /// 验证项目覆盖优先于所有较低层级。
    #[test]
    fn project_override_takes_precedence() {
        let workspace = unique_workspace("project");
        save_system_prompt_file(&project_system_prompt_file_path(&workspace), "Project base")
            .expect("save project override");

        let loaded = load_effective_system_prompt(Some(&workspace)).expect("load prompt");

        assert_eq!(loaded.source, SystemPromptSource::Project);
        assert_eq!(loaded.text, "Project base");
        let _ = fs::remove_dir_all(workspace);
    }

    /// Verifies a blank project override does not shadow the lower layers.
    /// 验证空白项目覆盖不会遮蔽较低层级。
    #[test]
    fn blank_project_override_falls_through() {
        let workspace = unique_workspace("blank");
        let path = project_system_prompt_file_path(&workspace);
        fs::create_dir_all(path.parent().expect("override should have a parent"))
            .expect("create override directory");
        fs::write(&path, " \n").expect("write blank override");

        let loaded = load_effective_system_prompt(Some(&workspace)).expect("load prompt");

        assert_ne!(loaded.source, SystemPromptSource::Project);
        let _ = fs::remove_dir_all(workspace);
    }

    /// Verifies an unreadable project override fails instead of silently falling back.
    /// 验证无法读取的项目覆盖会失败，而不是静默回退。
    #[test]
    fn invalid_utf8_project_override_is_an_error() {
        let workspace = unique_workspace("invalid");
        let path = project_system_prompt_file_path(&workspace);
        fs::create_dir_all(path.parent().expect("override should have a parent"))
            .expect("create override directory");
        fs::write(&path, [0xff, 0xfe, 0xfd]).expect("write invalid override");

        let error = load_effective_system_prompt(Some(&workspace))
            .expect_err("invalid override should fail");

        assert!(error.contains(&path.display().to_string()));
        let _ = fs::remove_dir_all(workspace);
    }
}
