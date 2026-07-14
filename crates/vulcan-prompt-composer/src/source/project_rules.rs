use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use crate::error::PromptComposerError;
use crate::source::snapshot::ProjectRulesSnapshot;

/// Default project-rules file name used by workspace-level prompt composition.
/// 工作区级提示词组合使用的默认项目规则文件名。
pub const DEFAULT_PROJECT_RULES_FILE_NAME: &str = "AGENTS.md";

/// Metadata marker used to detect whether one rules file changed.
/// 用于检测单个规则文件是否变化的元数据标记。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ProjectRulesFileMarker {
    /// File length in bytes.
    /// 文件字节长度。
    len: u64,
    /// Modified timestamp in nanoseconds since Unix epoch when available.
    /// 可用时的 Unix 纪元后纳秒级修改时间戳。
    modified_nanos: u128,
}

/// One cached project-rules file entry.
/// 单个项目规则文件缓存条目。
#[derive(Debug, Clone, PartialEq, Eq)]
struct CachedProjectRulesEntry {
    /// Last observed metadata marker; `None` means the file was missing.
    /// 最近观测到的元数据标记；`None` 表示文件缺失。
    marker: Option<ProjectRulesFileMarker>,
    /// Cached rules snapshot; `None` means missing or blank rules.
    /// 已缓存规则快照；`None` 表示缺失或空白规则。
    snapshot: Option<ProjectRulesSnapshot>,
}

/// Cached project-rules source loader.
/// 带缓存的项目规则来源加载器。
#[derive(Debug, Default)]
pub struct ProjectRulesFileCache {
    /// Cached entries keyed by absolute or caller-provided rules path.
    /// 按绝对路径或调用方提供的规则路径索引的缓存条目。
    entries: HashMap<PathBuf, CachedProjectRulesEntry>,
}

impl ProjectRulesFileCache {
    /// Build an empty project-rules file cache.
    /// 构建一个空的项目规则文件缓存。
    pub fn new() -> Self {
        Self::default()
    }

    /// Load the default `AGENTS.md` snapshot from one workspace directory.
    /// 从单个工作区目录加载默认 `AGENTS.md` 快照。
    pub fn load_workspace_default(
        &mut self,
        workspace_path: impl AsRef<Path>,
    ) -> Result<Option<ProjectRulesSnapshot>, PromptComposerError> {
        let Some(path) = workspace_default_project_rules_path(workspace_path.as_ref()) else {
            return Ok(None);
        };
        self.load_file(path)
    }

    /// Load one project-rules file with cache reuse when metadata is unchanged.
    /// 加载单个项目规则文件，并在元数据未变化时复用缓存。
    pub fn load_file(
        &mut self,
        path: impl AsRef<Path>,
    ) -> Result<Option<ProjectRulesSnapshot>, PromptComposerError> {
        let path = path.as_ref().to_path_buf();
        let marker = project_rules_file_marker(&path)?;
        if let Some(entry) = self.entries.get(&path)
            && entry.marker == marker
        {
            return Ok(entry.snapshot.clone());
        }

        let snapshot = match marker {
            Some(_) => read_project_rules_snapshot(&path)?,
            None => None,
        };
        self.entries.insert(
            path,
            CachedProjectRulesEntry {
                marker,
                snapshot: snapshot.clone(),
            },
        );
        Ok(snapshot)
    }

    /// Invalidate one concrete rules file path.
    /// 使单个具体规则文件路径失效。
    pub fn invalidate_file(&mut self, path: impl AsRef<Path>) {
        self.entries.remove(path.as_ref());
    }

    /// Invalidate the default `AGENTS.md` entry for one workspace directory.
    /// 使单个工作区目录的默认 `AGENTS.md` 条目失效。
    pub fn invalidate_workspace_default(&mut self, workspace_path: impl AsRef<Path>) {
        if let Some(path) = workspace_default_project_rules_path(workspace_path.as_ref()) {
            self.invalidate_file(path);
        }
    }

    /// Return the current number of cached entries.
    /// 返回当前缓存条目数量。
    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }
}

/// Resolve the default project-rules path for one workspace.
/// 解析单个工作区的默认项目规则路径。
pub fn workspace_default_project_rules_path(workspace_path: &Path) -> Option<PathBuf> {
    if !workspace_path.is_dir() {
        return None;
    }
    Some(workspace_path.join(DEFAULT_PROJECT_RULES_FILE_NAME))
}

/// Return one file metadata marker when the rules file exists.
/// 当规则文件存在时返回一个文件元数据标记。
fn project_rules_file_marker(
    path: &Path,
) -> Result<Option<ProjectRulesFileMarker>, PromptComposerError> {
    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(PromptComposerError::ResourceLoad(format!(
                "{}: {error}",
                vulcan_runtime_assets::native_host_path_text_from_path(path)
            )));
        }
    };
    if !metadata.is_file() {
        return Ok(None);
    }
    Ok(Some(ProjectRulesFileMarker {
        len: metadata.len(),
        modified_nanos: metadata
            .modified()
            .ok()
            .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
            .map(|duration| duration.as_nanos())
            .unwrap_or_default(),
    }))
}

/// Read one project-rules snapshot from disk.
/// 从磁盘读取单个项目规则快照。
fn read_project_rules_snapshot(
    path: &Path,
) -> Result<Option<ProjectRulesSnapshot>, PromptComposerError> {
    let text = fs::read_to_string(path).map_err(|error| {
        PromptComposerError::ResourceLoad(format!(
            "{}: {error}",
            vulcan_runtime_assets::native_host_path_text_from_path(path)
        ))
    })?;
    let text = text.trim();
    if text.is_empty() {
        return Ok(None);
    }
    Ok(Some(ProjectRulesSnapshot::new(
        vulcan_runtime_assets::native_host_path_text_from_path(path),
        text.to_string(),
    )))
}
