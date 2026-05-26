use std::path::{Path, PathBuf};

use crate::adapters::routing::FileWatchRoute;
use crate::event::FileWatchEvent;
use crate::live::LiveWatchPath;
use crate::traits::FileWatchTarget;

/// Default workspace project-rules file name.
/// 默认工作区项目规则文件名。
pub const DEFAULT_PROJECT_RULES_FILE_NAME: &str = "AGENTS.md";

/// Stable route prefix used by project-rules watch adapters.
/// 项目规则监听适配器使用的稳定路由前缀。
pub const PROJECT_RULES_ROUTE_PREFIX: &str = "project-rules";

/// Watch target descriptor for one workspace project-rules file.
/// 单个工作区项目规则文件的监听目标描述。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectRulesWatchTarget {
    /// Workspace root directory.
    /// 工作区根目录。
    pub workspace_path: PathBuf,
    /// Project-rules file name inside the workspace root.
    /// 工作区根目录内的项目规则文件名。
    pub file_name: String,
}

impl ProjectRulesWatchTarget {
    /// Build one target using the default `AGENTS.md` file name.
    /// 使用默认 `AGENTS.md` 文件名构建一个目标。
    pub fn workspace_default(workspace_path: impl Into<PathBuf>) -> Self {
        Self {
            workspace_path: workspace_path.into(),
            file_name: String::from(DEFAULT_PROJECT_RULES_FILE_NAME),
        }
    }

    /// Return the concrete project-rules file path.
    /// 返回具体项目规则文件路径。
    pub fn rules_path(&self) -> PathBuf {
        self.workspace_path.join(&self.file_name)
    }

    /// Return the stable route id for this workspace target.
    /// 返回该工作区目标的稳定路由标识。
    pub fn route_id(&self) -> String {
        format!(
            "{}:{}",
            PROJECT_RULES_ROUTE_PREFIX,
            self.rules_path().display()
        )
    }

    /// Convert this target into the generic synchronous watch target.
    /// 将该目标转换为通用同步监听目标。
    pub fn to_file_watch_target(&self) -> FileWatchTarget {
        FileWatchTarget {
            path: self.rules_path(),
            recursive: false,
        }
    }

    /// Convert this target into the live watcher path descriptor.
    /// 将该目标转换为实时监听器路径描述。
    pub fn to_live_watch_path(&self) -> LiveWatchPath {
        LiveWatchPath {
            path: self.rules_path(),
            recursive: false,
        }
    }

    /// Convert this target into a reusable route.
    /// 将该目标转换为可复用路由。
    pub fn to_route(&self) -> FileWatchRoute {
        FileWatchRoute {
            id: self.route_id(),
            root: self.rules_path(),
            recursive: false,
        }
    }

    /// Return whether one changed path affects this project-rules file.
    /// 返回某个变更路径是否影响该项目规则文件。
    pub fn matches_path(&self, path: &Path) -> bool {
        path == self.rules_path()
    }

    /// Return whether one file-watch event affects this project-rules file.
    /// 返回某个文件监听事件是否影响该项目规则文件。
    pub fn matches_event(&self, event: &FileWatchEvent) -> bool {
        self.matches_path(&event.path)
            || event
                .previous_path
                .as_deref()
                .is_some_and(|path| self.matches_path(path))
    }
}

/// Build the default project-rules route for one workspace.
/// 为单个工作区构建默认项目规则路由。
pub fn default_project_rules_route(workspace_path: impl Into<PathBuf>) -> FileWatchRoute {
    ProjectRulesWatchTarget::workspace_default(workspace_path).to_route()
}

/// Build the default live watch path for one workspace project-rules file.
/// 为单个工作区项目规则文件构建默认实时监听路径。
pub fn default_project_rules_live_watch_path(workspace_path: impl Into<PathBuf>) -> LiveWatchPath {
    ProjectRulesWatchTarget::workspace_default(workspace_path).to_live_watch_path()
}
