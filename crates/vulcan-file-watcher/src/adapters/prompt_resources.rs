use std::path::{Path, PathBuf};

use crate::live::LiveWatchPath;

/// Workspace-local project configuration file name used by Vulcan Code.
/// Vulcan Code 使用的工作区本地项目配置文件名。
pub const PROJECT_CONFIGURATION_FILE_NAME: &str = "config.json";

/// Workspace-local Vulcan Code directory segments.
/// 工作区本地 Vulcan Code 目录片段。
pub const PROJECT_VULCAN_CODE_DIRECTORY_SEGMENTS: [&str; 2] = [".vulcan", "code"];

/// Prompt-resource watch event category.
/// 提示词资源监听事件类别。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PromptResourcesWatchKind {
    /// Workspace-local project prompt file changed.
    /// 工作区本地项目提示词文件发生变化。
    ProjectPrompt,
    /// Workspace-local project prompt configuration changed.
    /// 工作区本地项目提示词配置发生变化。
    ProjectConfiguration,
    /// User-level prompt resource changed.
    /// 用户级提示词资源发生变化。
    UserPromptResources,
}

/// Watch target descriptor for prompt-composer resources.
/// 提示词组合资源的监听目标描述。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PromptResourcesWatchTarget {
    /// Workspace root directory.
    /// 工作区根目录。
    pub workspace_path: PathBuf,
    /// Workspace-relative project prompt path.
    /// 工作区相对项目提示词路径。
    pub project_prompt_path: String,
    /// Whether the workspace project prompt should be watched.
    /// 是否监听工作区项目提示词。
    pub project_prompt_enabled: bool,
    /// Optional user-level prompt resource root.
    /// 可选用户级提示词资源根目录。
    pub user_prompt_root: Option<PathBuf>,
}

impl PromptResourcesWatchTarget {
    /// Build one prompt-resource target from workspace and user prompt inputs.
    /// 根据工作区与用户提示词输入构建一个提示词资源目标。
    pub fn new(
        workspace_path: impl Into<PathBuf>,
        project_prompt_path: impl Into<String>,
        project_prompt_enabled: bool,
        user_prompt_root: Option<PathBuf>,
    ) -> Self {
        Self {
            workspace_path: workspace_path.into(),
            project_prompt_path: project_prompt_path.into(),
            project_prompt_enabled,
            user_prompt_root,
        }
    }

    /// Return the workspace project prompt file path when project prompt watching is enabled.
    /// 在项目提示词监听启用时返回工作区项目提示词文件路径。
    pub fn project_prompt_full_path(&self) -> Option<PathBuf> {
        let relative = self
            .project_prompt_path
            .trim()
            .trim_start_matches(['/', '\\']);
        if !self.project_prompt_enabled
            || self.workspace_path.as_os_str().is_empty()
            || relative.is_empty()
        {
            return None;
        }

        Some(self.workspace_path.join(relative))
    }

    /// Return the workspace-local project configuration path.
    /// 返回工作区本地项目配置路径。
    pub fn project_configuration_path(&self) -> Option<PathBuf> {
        if self.workspace_path.as_os_str().is_empty() {
            return None;
        }

        Some(
            PROJECT_VULCAN_CODE_DIRECTORY_SEGMENTS
                .iter()
                .fold(self.workspace_path.clone(), |path, segment| {
                    path.join(segment)
                })
                .join(PROJECT_CONFIGURATION_FILE_NAME),
        )
    }

    /// Return live watch paths required by this prompt-resource target.
    /// 返回该提示词资源目标需要的实时监听路径。
    pub fn to_live_watch_paths(&self) -> Vec<LiveWatchPath> {
        let mut watch_paths = Vec::new();
        if let Some(user_prompt_root) = self.user_prompt_root.as_ref() {
            watch_paths.push(LiveWatchPath {
                path: user_prompt_root.clone(),
                recursive: true,
            });
        }
        if let Some(project_prompt_path) = self.project_prompt_full_path() {
            watch_paths.push(LiveWatchPath {
                path: project_prompt_path,
                recursive: false,
            });
        }
        if let Some(project_configuration_path) = self.project_configuration_path() {
            watch_paths.push(LiveWatchPath {
                path: project_configuration_path,
                recursive: false,
            });
        }
        watch_paths
    }

    /// Classify changed paths into the narrowest prompt-resource category.
    /// 将变更路径分类为最窄的提示词资源类别。
    pub fn classify_paths(&self, paths: &[PathBuf]) -> PromptResourcesWatchKind {
        if self
            .project_prompt_full_path()
            .as_deref()
            .is_some_and(|target| paths.iter().any(|path| paths_equal(path, target)))
        {
            return PromptResourcesWatchKind::ProjectPrompt;
        }
        if self
            .project_configuration_path()
            .as_deref()
            .is_some_and(|target| paths.iter().any(|path| paths_equal(path, target)))
        {
            return PromptResourcesWatchKind::ProjectConfiguration;
        }
        PromptResourcesWatchKind::UserPromptResources
    }
}

/// Return whether two paths should be treated as the same watcher path.
/// 返回两个路径是否应被视为同一个监听路径。
fn paths_equal(left: &Path, right: &Path) -> bool {
    left == right
}
