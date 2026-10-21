//! Git repository discovery and safe repository-key resolution for WebUI review flows.
//! WebUI 审查流程使用的 Git 仓库发现与安全仓库键解析。

use std::{
    collections::BTreeSet,
    fs,
    path::{Component, Path, PathBuf},
};

use super::command::map_webui_git_parallel;
use super::diff::{resolve_webui_git_repository_root, resolve_webui_workspace_path_from_text};
use super::model::{WebuiGitRepositoryCatalog, WebuiGitRepositorySummary};
use crate::normalized_workspace_path_text_from_path;

/// Maximum directory depth scanned below one project root.
/// 单个项目根目录下扫描的最大目录深度。
const WEBUI_GIT_DISCOVERY_MAX_DEPTH: usize = 2;

/// Directory names that must never be traversed during fast Git discovery.
/// 快速 Git 发现期间绝不遍历的目录名称。
const WEBUI_GIT_DISCOVERY_SKIP_DIRECTORIES: &[&str] = &[
    ".cache",
    ".git",
    ".next",
    ".vulcan",
    "build",
    "dist",
    "node_modules",
    "target",
    "vendor",
];

/// Discovers Git repositories at the project root and at most two child levels.
/// 在项目根目录及最多两级子目录内发现 Git 仓库。
///
/// The returned order is deterministic: the project root is first, followed by
/// normalized relative repository keys in lexical order.
/// 返回顺序是确定性的：项目根目录优先，其后按规范化相对仓库键字典序排列。
pub(crate) fn discover_webui_git_repository_catalog(
    project_root: &Path,
) -> Result<WebuiGitRepositoryCatalog, String> {
    let project_root = fs::canonicalize(project_root).map_err(|error| {
        format!(
            "Cannot resolve the Git multi-repository project root `{}`: {error}",
            project_root.display()
        )
    })?;
    if !project_root.is_dir() {
        return Err(format!(
            "The Git multi-repository project root is not a directory: {}",
            project_root.display()
        ));
    }

    let mut markers = Vec::new();
    collect_git_repository_candidates(&project_root, &project_root, 0, &mut markers)?;
    // Only directories with a `.git` marker reach Git, and they are confirmed side by side.
    // 只有带 `.git` 标记的目录才交给 Git，并且并行确认。
    let confirmed = map_webui_git_parallel(&markers, |marker| {
        resolve_webui_git_repository_root(marker)
            .map(|root| root.as_deref() == Some(marker.as_path()))
    });
    let mut candidates = Vec::new();
    for (marker, confirmed) in markers.into_iter().zip(confirmed) {
        if confirmed? {
            candidates.push(marker);
        }
    }
    candidates.sort_by_key(|candidate| repository_key_for_root(&project_root, candidate));
    if let Some(root_index) = candidates
        .iter()
        .position(|candidate| candidate == &project_root)
    {
        let root = candidates.remove(root_index);
        candidates.insert(0, root);
    }

    let repositories = candidates
        .into_iter()
        .map(|repository_root| {
            let repository_key = repository_key_for_root(&project_root, &repository_root);
            let name = repository_root
                .file_name()
                .and_then(|value| value.to_str())
                .filter(|value| !value.trim().is_empty())
                .map(ToOwned::to_owned)
                .unwrap_or_else(|| repository_key.clone());
            WebuiGitRepositorySummary {
                relative_path: repository_key.clone(),
                repository_key,
                repository_root: normalized_workspace_path_text_from_path(&repository_root),
                name,
                is_primary: repository_root == project_root,
                monitoring_enabled: true,
            }
        })
        .collect();
    Ok(WebuiGitRepositoryCatalog {
        project_root: normalized_workspace_path_text_from_path(&project_root),
        repositories,
    })
}

/// Resolves one discovered repository key beneath a canonical project root.
/// 在规范化项目根目录下解析一个已发现的仓库键。
pub(crate) fn resolve_webui_repository_root_for_key(
    project_root: &Path,
    repository_key: Option<&str>,
) -> Result<PathBuf, String> {
    let project_root = fs::canonicalize(project_root).map_err(|error| {
        format!(
            "Cannot resolve the Git project root `{}`: {error}",
            project_root.display()
        )
    })?;
    let repository_key = repository_key
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(".");
    let candidate = if repository_key == "." {
        project_root.clone()
    } else {
        let key_path = Path::new(repository_key);
        if key_path.is_absolute()
            || key_path.components().any(|component| {
                matches!(
                    component,
                    Component::ParentDir | Component::RootDir | Component::Prefix(_)
                )
            })
        {
            return Err(format!(
                "The Git repository key contains an invalid path: `{repository_key}`."
            ));
        }
        project_root.join(key_path)
    };
    let candidate = fs::canonicalize(&candidate).map_err(|error| {
        format!("Cannot resolve Git repository key `{repository_key}`: {error}")
    })?;
    if !candidate.starts_with(&project_root) {
        return Err(format!(
            "The Git repository key escapes the project root: `{repository_key}`."
        ));
    }
    if resolve_webui_git_repository_root(&candidate)?.as_deref() != Some(candidate.as_path()) {
        return Err(format!(
            "The path is not a Git repository inside the project: `{repository_key}`."
        ));
    }
    let canonical_repository_key = repository_key_for_root(&project_root, &candidate);
    if canonical_repository_key != repository_key {
        return Err(format!(
            "The Git repository key is not a canonical key discovered in the project directory: `{repository_key}`."
        ));
    }
    Ok(candidate)
}

/// Resolves an optional repository key while preserving legacy repository-root behavior.
/// 解析可选仓库键，同时保持旧请求的仓库根目录行为。
pub(crate) fn resolve_webui_repository_root_for_request(
    project_root: &Path,
    repository_key: Option<&str>,
    legacy_repository_root: Option<PathBuf>,
) -> Result<Option<PathBuf>, String> {
    if let Some(repository_key) = repository_key {
        return resolve_webui_repository_root_for_key(project_root, Some(repository_key)).map(Some);
    }
    let project_root = fs::canonicalize(project_root).map_err(|error| {
        format!(
            "Cannot resolve the Git project root `{}`: {error}",
            project_root.display()
        )
    })?;
    if legacy_repository_root.as_ref() == Some(&project_root) {
        return Ok(legacy_repository_root);
    }
    let catalog = discover_webui_git_repository_catalog(&project_root)?;
    if let Some(repository) = catalog.repositories.first() {
        return Ok(Some(PathBuf::from(&repository.repository_root)));
    }
    Ok(legacy_repository_root)
}

/// Resolves a workspace text and optional repository key into one Git root.
/// 将工作区文本与可选仓库键解析为单个 Git 根目录。
pub(crate) fn resolve_webui_repository_root_from_workspace_text_with_key(
    workspace_path: &str,
    repository_key: Option<&str>,
) -> Result<PathBuf, String> {
    let workspace_path = resolve_webui_workspace_path_from_text(workspace_path)?;
    if repository_key.is_none() {
        return resolve_webui_repository_root_for_request(
            &workspace_path,
            None,
            resolve_webui_git_repository_root(&workspace_path)?,
        )?
        .ok_or_else(|| {
            String::from(
                "The current workspace is not a Git repository; the Git operation cannot run.",
            )
        });
    }
    resolve_webui_repository_root_for_key(&workspace_path, repository_key)
}

/// Recursively collects repository roots without following symlinks.
/// 不跟随符号链接递归收集仓库根目录。
fn collect_git_repository_candidates(
    project_root: &Path,
    current: &Path,
    depth: usize,
    candidates: &mut Vec<PathBuf>,
) -> Result<(), String> {
    // Repository confirmation needs a Git process, so the walk only records marker directories.
    // 仓库确认需要 Git 进程，因此遍历只记录带标记的目录。
    if git_marker_exists(current)? {
        candidates.push(current.to_path_buf());
    }
    if depth >= WEBUI_GIT_DISCOVERY_MAX_DEPTH {
        return Ok(());
    }
    let mut entries = fs::read_dir(current)
        .map_err(|error| {
            format!(
                "Cannot read the Git discovery directory `{}`: {error}",
                current.display()
            )
        })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("Failed to read a Git discovery directory entry: {error}"))?;
    entries.sort_by_key(|entry| entry.file_name());
    let mut seen_paths = BTreeSet::new();
    for entry in entries {
        let path = entry.path();
        let file_type = entry.file_type().map_err(|error| {
            format!(
                "Failed to read the type of Git discovery entry `{}`: {error}",
                path.display()
            )
        })?;
        if !file_type.is_dir() || file_type.is_symlink() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
            continue;
        };
        if WEBUI_GIT_DISCOVERY_SKIP_DIRECTORIES
            .iter()
            .any(|skip| name.eq_ignore_ascii_case(skip))
        {
            continue;
        }
        let canonical_path = fs::canonicalize(&path).map_err(|error| {
            format!(
                "Cannot resolve the Git discovery path `{}`: {error}",
                path.display()
            )
        })?;
        if !canonical_path.starts_with(project_root) || !seen_paths.insert(canonical_path.clone()) {
            continue;
        }
        collect_git_repository_candidates(project_root, &canonical_path, depth + 1, candidates)?;
    }
    Ok(())
}

/// Checks the direct `.git` marker before spawning a Git process.
/// 在启动 Git 进程前检查目录直属的 `.git` 标记。
fn git_marker_exists(path: &Path) -> Result<bool, String> {
    let marker_path = path.join(".git");
    match fs::symlink_metadata(&marker_path) {
        Ok(metadata) => Ok(metadata.file_type().is_dir() || metadata.file_type().is_file()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(format!(
            "Failed to read Git marker `{}`: {error}",
            marker_path.display()
        )),
    }
}

/// Converts one repository root into its stable project-relative key.
/// 将一个仓库根目录转换为稳定的项目相对键。
fn repository_key_for_root(project_root: &Path, repository_root: &Path) -> String {
    repository_root
        .strip_prefix(project_root)
        .ok()
        .filter(|relative| !relative.as_os_str().is_empty())
        .map(|relative| {
            relative
                .components()
                .filter_map(|component| match component {
                    Component::Normal(value) => value.to_str().map(ToOwned::to_owned),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("/")
        })
        .filter(|relative| !relative.is_empty())
        .unwrap_or_else(|| String::from("."))
}

#[cfg(test)]
mod tests {
    use std::{fs, process::Command};

    use super::*;

    /// Creates a marker Git repository for discovery tests.
    /// 为发现测试创建带标记的 Git 仓库。
    fn create_repository(path: &Path) {
        fs::create_dir_all(path).expect("repository directory should be created");
        let status = Command::new("git")
            .args(["init", "--quiet"])
            .current_dir(path)
            .status()
            .expect("git should be available for discovery tests");
        assert!(status.success(), "git init should succeed");
    }

    #[test]
    fn discovers_root_and_two_levels_with_stable_keys() {
        let temporary = tempfile::tempdir().expect("temporary directory should be created");
        create_repository(temporary.path());
        create_repository(&temporary.path().join("nested").join("service"));
        create_repository(&temporary.path().join("nested").join("service").join("deep"));
        create_repository(
            &temporary
                .path()
                .join("nested")
                .join("service")
                .join("deep")
                .join("too-deep"),
        );
        create_repository(&temporary.path().join("node_modules").join("ignored"));

        let catalog = discover_webui_git_repository_catalog(temporary.path())
            .expect("discovery should succeed");
        let keys = catalog
            .repositories
            .iter()
            .map(|repository| repository.repository_key.as_str())
            .collect::<Vec<_>>();
        assert_eq!(keys, vec![".", "nested/service"]);
    }

    #[test]
    fn rejects_parent_repository_keys() {
        let temporary = tempfile::tempdir().expect("temporary directory should be created");
        create_repository(temporary.path());
        let error = resolve_webui_repository_root_for_key(temporary.path(), Some("../outside"))
            .expect_err("parent key must be rejected");
        assert!(error.contains("invalid path"));
    }

    #[test]
    fn rejects_noncanonical_repository_keys() {
        let temporary = tempfile::tempdir().expect("temporary directory should be created");
        create_repository(temporary.path());
        create_repository(&temporary.path().join("nested").join("service"));
        let error =
            resolve_webui_repository_root_for_key(temporary.path(), Some("nested/./service"))
                .expect_err("noncanonical repository key must be rejected");
        assert!(error.contains("canonical key"));
    }

    #[test]
    fn resolves_primary_nested_repository_when_project_root_is_not_git() {
        let temporary = tempfile::tempdir().expect("temporary directory should be created");
        create_repository(&temporary.path().join("services").join("alpha"));
        create_repository(&temporary.path().join("services").join("beta"));

        let repository_root = resolve_webui_repository_root_from_workspace_text_with_key(
            temporary
                .path()
                .to_str()
                .expect("temporary path should be text"),
            None,
        )
        .expect("nested primary repository should resolve");
        assert_eq!(
            repository_root,
            temporary.path().join("services").join("alpha")
        );
    }
}
