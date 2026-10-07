use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use crate::error::PromptComposerError;
use crate::source::loader::TextPromptResourceLoader;
use crate::source::snapshot::ProjectPromptSnapshot;

/// Metadata marker used to detect whether one project-prompt file changed.
/// 用于检测单个项目提示词文件是否变化的元数据标记。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ProjectPromptFileMarker {
    /// File length in bytes.
    /// 文件字节长度。
    len: u64,
    /// Modified timestamp in nanoseconds since Unix epoch when available.
    /// 可用时的 Unix 纪元后纳秒级修改时间戳。
    modified_nanos: u128,
}

/// One cached project-prompt file entry.
/// 单个项目提示词文件缓存条目。
#[derive(Debug, Clone, PartialEq, Eq)]
struct CachedProjectPromptEntry {
    /// Last observed metadata marker; `None` means the file was missing.
    /// 最近观测到的元数据标记；`None` 表示文件缺失。
    marker: Option<ProjectPromptFileMarker>,
    /// Cached prompt snapshot; `None` means missing or blank prompt.
    /// 已缓存提示词快照；`None` 表示缺失或空白提示词。
    snapshot: Option<ProjectPromptSnapshot>,
}

/// Cached project-prompt source loader.
/// 带缓存的项目提示词来源加载器。
#[derive(Debug, Default)]
pub struct ProjectPromptFileCache {
    /// Cached entries keyed by absolute or caller-provided prompt path.
    /// 按绝对路径或调用方提供的提示词路径索引的缓存条目。
    entries: HashMap<PathBuf, CachedProjectPromptEntry>,
}

impl ProjectPromptFileCache {
    /// Build an empty project-prompt file cache.
    /// 构建一个空的项目提示词文件缓存。
    pub fn new() -> Self {
        Self::default()
    }

    /// Load one project-prompt file with cache reuse when metadata is unchanged.
    /// 加载单个项目提示词文件，并在元数据未变化时复用缓存。
    pub fn load_file(
        &mut self,
        path: impl AsRef<Path>,
    ) -> Result<Option<ProjectPromptSnapshot>, PromptComposerError> {
        let path = path.as_ref().to_path_buf();
        let marker = project_prompt_file_marker(&path)?;
        if let Some(entry) = self.entries.get(&path)
            && entry.marker == marker
        {
            return Ok(entry.snapshot.clone());
        }

        let snapshot = match marker {
            Some(_) => read_project_prompt_snapshot(&path)?,
            None => None,
        };
        self.entries.insert(
            path,
            CachedProjectPromptEntry {
                marker,
                snapshot: snapshot.clone(),
            },
        );
        Ok(snapshot)
    }

    /// Return the current number of cached entries.
    /// 返回当前缓存条目数量。
    pub fn entry_count(&self) -> usize {
        self.entries.len()
    }
}

/// Return one file metadata marker when the prompt file exists.
/// 当提示词文件存在时返回一个文件元数据标记。
///
/// # Parameters
/// # 参数
/// - `path`: Exact project-prompt file whose metadata identifies the cached snapshot.
/// - `path`：其元数据用于标识缓存快照的精确项目提示词文件。
///
/// # Returns
/// # 返回值
/// Returns `None` only for a confirmed missing or non-file path, and preserves all metadata failures.
/// 仅在确认路径缺失或不是文件时返回 `None`，并保留全部元数据失败。
fn project_prompt_file_marker(
    path: &Path,
) -> Result<Option<ProjectPromptFileMarker>, PromptComposerError> {
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
    // modified_at is required to distinguish cached content from a changed source file.
    // modified_at 是区分缓存内容与已变化来源文件所必需的时间戳。
    let modified_at = metadata.modified().map_err(|error| {
        PromptComposerError::ResourceLoad(format!(
            "{}: failed to read project-prompt modification time: {error}",
            vulcan_runtime_assets::native_host_path_text_from_path(path)
        ))
    })?;
    // modified_nanos treats pre-epoch filesystem timestamps as the oldest stable marker.
    // modified_nanos 将早于纪元的文件系统时间戳视为最早的稳定标记。
    let modified_nanos = modified_at
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    Ok(Some(ProjectPromptFileMarker {
        len: metadata.len(),
        modified_nanos,
    }))
}

/// Read one project-prompt snapshot from disk.
/// 从磁盘读取单个项目提示词快照。
fn read_project_prompt_snapshot(
    path: &Path,
) -> Result<Option<ProjectPromptSnapshot>, PromptComposerError> {
    let text = TextPromptResourceLoader::read_text_file(path)?;
    let text = text.trim();
    if text.is_empty() {
        return Ok(None);
    }
    Ok(Some(ProjectPromptSnapshot::new(
        vulcan_runtime_assets::native_host_path_text_from_path(path),
        text.to_string(),
    )))
}

#[cfg(test)]
mod tests {
    //! Project-prompt cache tests.
    //! 项目提示词缓存测试。

    use super::*;

    /// Verifies the project-prompt file cache reuses an unchanged snapshot.
    /// 验证项目提示词文件缓存会复用未变化的快照。
    #[test]
    fn project_prompt_file_cache_reuses_unchanged_snapshot() {
        let root = unique_temp_dir("vulcan-project-prompt-cache-reuse");
        fs::create_dir_all(&root).expect("create workspace");
        let prompt_path = root.join("prompt.md");
        fs::write(&prompt_path, "Project prompt v1\n").expect("write prompt");

        let mut cache = ProjectPromptFileCache::new();
        let first = cache
            .load_file(&prompt_path)
            .expect("first load should succeed")
            .expect("prompt should exist");
        let second = cache
            .load_file(&prompt_path)
            .expect("second load should succeed")
            .expect("prompt should still exist");

        assert_eq!(first, second);
        assert_eq!(cache.entry_count(), 1);

        let _ = fs::remove_dir_all(root);
    }

    /// Verifies the project-prompt file cache refreshes after content changes.
    /// 验证项目提示词文件缓存会在内容变化后刷新。
    #[test]
    fn project_prompt_file_cache_refreshes_changed_snapshot() {
        let root = unique_temp_dir("vulcan-project-prompt-cache-refresh");
        fs::create_dir_all(&root).expect("create workspace");
        let prompt_path = root.join("prompt.md");
        fs::write(&prompt_path, "Project prompt v1\n").expect("write first prompt");

        let mut cache = ProjectPromptFileCache::new();
        let first = cache
            .load_file(&prompt_path)
            .expect("first load should succeed")
            .expect("prompt should exist");
        fs::write(&prompt_path, "Project prompt v2 changed\n").expect("write second prompt");
        let second = cache
            .load_file(&prompt_path)
            .expect("second load should succeed")
            .expect("prompt should still exist");

        assert_ne!(first.content_hash, second.content_hash);
        assert_eq!(second.text, "Project prompt v2 changed");

        let _ = fs::remove_dir_all(root);
    }

    /// Builds one unique temporary directory for cache tests.
    /// 为缓存测试构建一个唯一临时目录。
    fn unique_temp_dir(prefix: &str) -> std::path::PathBuf {
        let suffix = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system time should be after epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("{prefix}-{suffix}"))
    }
}
