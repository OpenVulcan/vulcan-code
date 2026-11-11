//! Attached-folder repository discovery and request scoping.
//! 附属目录仓库发现与请求作用域解析。

use std::{collections::BTreeSet, fs, path::Path};

use super::{
    WebuiGitRepositoryCatalog, discover_webui_git_repository_catalog,
    webui_diff_project_id_from_state, webui_diff_workspace_path_from_state,
};
use crate::workspace_files::workspace_path_is_inside_root;
use crate::{
    SharedServiceBridgeState, controller_runtime_from_state,
    normalized_workspace_path_text_from_path, spawn_service_blocking_task,
};
use vulcan_workspace_store::workspace_store::load_project_attached_folders;

/// Prefix disjoint from existing project-relative repository keys.
/// 与现有项目相对仓库键不重叠的前缀。
const ATTACHED_REPOSITORY_PREFIX: &str = "/attached/";

/// Discovers the primary `workspace_path` and each saved attached path as independent roots.
/// 将主目录 `workspace_path` 与各已登记路径作为独立根目录发现仓库。
/// Returns one project catalog, deduplicating by canonical repository root.
/// 返回按规范化仓库根目录去重的项目目录。
pub(super) fn discover_project_git_catalog(
    workspace_path: &str,
    attached_paths: &[String],
) -> Result<WebuiGitRepositoryCatalog, String> {
    // The primary project keeps its existing repository keys and priority.
    // 主项目保留原有仓库键和优先级。
    let mut catalog = discover_webui_git_repository_catalog(Path::new(workspace_path))?;
    // A repository may be present under overlapping roots or discovered by the primary scan.
    // 重叠根目录或主目录扫描可能发现同一个仓库。
    let mut seen: BTreeSet<_> = catalog
        .repositories
        .iter()
        .map(|repository| repository.repository_root.clone())
        .collect();
    for attached in attached_paths {
        // A missing or unreadable attachment is reported on its project-home row and cannot block other roots.
        // 缺失或不可读的附属目录在项目首页对应行显示错误，其他目录仍可正常使用。
        let Ok(attached_catalog) = discover_webui_git_repository_catalog(Path::new(attached))
        else {
            continue;
        };
        for mut repository in attached_catalog.repositories {
            if seen.insert(repository.repository_root.clone()) {
                repository.repository_key =
                    format!("{ATTACHED_REPOSITORY_PREFIX}{}", repository.repository_root);
                repository.relative_path = repository.repository_root.clone();
                repository.is_primary = false;
                catalog.repositories.push(repository);
            }
        }
    }
    Ok(catalog)
}

/// Resolves the registered workspace and translates an attached repository key for the inner Git operations.
/// 解析已登记的工作区，并为内部 Git 操作翻译附属仓库键。
/// `state` resolves `project_id` or `conversation_id`; `workspace_path` retains existing standalone behavior.
/// `state` 解析 `project_id` 或 `conversation_id`；`workspace_path` 保留现有独立目录行为。
/// On success, returns the workspace path and rewrites an attached `repository_key` to its own root key.
/// 成功时返回工作区路径，并将附属 `repository_key` 改写为该仓库自身的根键。
pub(super) async fn resolve_git_request_scope(
    state: &SharedServiceBridgeState,
    workspace_path: Option<&str>,
    project_id: Option<&str>,
    conversation_id: Option<&str>,
    repository_key: Option<&mut Option<String>>,
) -> Result<String, String> {
    if let Some(repository_key) = repository_key
        && let Some(path) = repository_key
            .as_deref()
            .and_then(|key| key.strip_prefix(ATTACHED_REPOSITORY_PREFIX))
    {
        // Authorize an attached target exclusively through this project's saved associations.
        // 附属目标只能通过当前项目保存的目录关联获得授权。
        let owner = webui_diff_project_id_from_state(state, project_id, conversation_id)?
            .ok_or_else(|| {
                String::from("An attached Git repository requires an active project.")
            })?;
        let roots =
            load_project_attached_folders(controller_runtime_from_state(state)?, &owner).await?;
        let path = path.to_owned();
        let resolved =
            spawn_service_blocking_task(move || canonical_attached_repository_path(&path, &roots))
                .await
                .map_err(|error| format!("Attached Git repository resolution failed: {error}"))??;
        *repository_key = Some(String::from("."));
        return Ok(resolved);
    }
    webui_diff_workspace_path_from_state(state, project_id, conversation_id, workspace_path)
}

/// Returns the canonical target `path` only when it is inside one currently registered root.
/// 仅当目标 `path` 位于一个当前已登记根目录内时才返回其规范化路径。
fn canonical_attached_repository_path(path: &str, roots: &[String]) -> Result<String, String> {
    // The key carries a canonical absolute repository root emitted by the catalog.
    // 键携带由目录发出的规范化绝对仓库根路径。
    if !Path::new(path).is_absolute() {
        return Err(String::from(
            "An attached Git repository key must name an absolute path.",
        ));
    }
    // Resolve the real path before checking containment to reject traversals and symlink escapes.
    // 先解析实际路径再校验包含关系，拒绝路径穿越和符号链接越界。
    let canonical = fs::canonicalize(path)
        .map_err(|error| format!("Cannot resolve attached Git repository `{path}`: {error}"))?;
    let canonical_text = normalized_workspace_path_text_from_path(&canonical);
    if canonical_text != path
        || !roots.iter().any(|root| {
            fs::canonicalize(root)
                .is_ok_and(|root| workspace_path_is_inside_root(&canonical, &root))
        })
    {
        return Err(format!(
            "Git repository `{path}` is not inside an attached folder of the project."
        ));
    }
    Ok(canonical_text)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Creates an independent Git repository in the supplied synthetic directory.
    /// 在给定的合成目录中创建独立 Git 仓库。
    fn create_repository(path: &Path) {
        fs::create_dir_all(path).expect("create repository directory");
        assert!(
            std::process::Command::new("git")
                .args(["init", "--quiet"])
                .current_dir(path)
                .status()
                .expect("start git init")
                .success()
        );
    }

    /// Discovers only registered sibling repositories and deduplicates overlapping directory roots.
    /// 仅发现已登记的同级仓库，并对重叠的目录根进行去重。
    #[test]
    fn detects_attached_repositories_and_checks_their_project_boundary() {
        // The fixture includes a primary repo, a registered sibling, and a sibling with a similar name.
        // 测试夹具含主仓库、已登记的同级仓库及名称相近的同级仓库。
        let temp = tempfile::tempdir().expect("create synthetic workspace");
        let primary = temp.path().join("primary");
        let attached = temp.path().join("shared");
        let nested = attached.join("nested");
        let outside = temp.path().join("shared-old");
        for repo in [&primary, &attached, &nested, &outside] {
            create_repository(repo);
        }
        // Register the nested repo twice through overlapping roots and include one missing directory.
        // 通过重叠的根目录重复登记嵌套仓库，并加入一个已缺失目录。
        let roots = [attached, nested.clone(), temp.path().join("missing")]
            .map(|path| normalized_workspace_path_text_from_path(&path));
        let catalog = discover_project_git_catalog(primary.to_str().expect("primary path"), &roots)
            .expect("discover the project repositories");
        assert_eq!(catalog.repositories.len(), 3);
        assert!(
            catalog
                .repositories
                .iter()
                .any(|repo| repo.is_primary && repo.repository_key == ".")
        );
        for repo in catalog.repositories.iter().filter(|repo| !repo.is_primary) {
            let path = repo
                .repository_key
                .strip_prefix(ATTACHED_REPOSITORY_PREFIX)
                .expect("attached repository key");
            assert_eq!(
                canonical_attached_repository_path(path, &roots)
                    .expect("resolve registered repository"),
                repo.repository_root
            );
            assert!(canonical_attached_repository_path(path, &[]).is_err());
        }
        let outside = normalized_workspace_path_text_from_path(&outside);
        assert!(canonical_attached_repository_path(&outside, &roots).is_err());
    }
}
