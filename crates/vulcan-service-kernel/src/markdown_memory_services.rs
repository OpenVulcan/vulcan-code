//! Memory mode settings and Markdown memory page services.
//! 记忆模式设置与 Markdown 记忆页面服务。
//!
//! The memory page is a user surface: writes are recorded with `source: user`, and the user may
//! still edit a project whose memory is turned off. Switching the mode never rewrites the saved
//! VMM configuration; VMM is disabled in memory only through the mode projection.
//! 记忆页面是用户界面：写入记录为 `source: user`，用户仍可编辑已关闭记忆的项目。切换模式从不改写
//! 已保存的 VMM 配置；VMM 只通过模式投影在内存中关闭。

use super::*;
use crate::runtime_workspace_helpers::workspace_task_project_for_project_id;
use crate::vmm_hooks::refresh_guard_vmm_hook_snapshot;
use std::path::Path;
use vulcan_markdown_memory::{
    BUDGET_WARNING_PERCENT, InjectionBudget, MarkdownMemorySettings, MarkdownMemoryStore,
    MemoryDocument, MemoryError, MemoryProject, MemoryScope, NodeSource, NodeView, ProjectContext,
    ScopeDocuments, WriteOperation, build_injection_snapshot, now_utc_minute, preset_sections,
};
use vulcan_service_api::{
    MarkdownMemoryFileReplaceRequest, MarkdownMemoryFileRequest, MarkdownMemoryFileView,
    MarkdownMemoryNodeView, MarkdownMemoryOrphanView, MarkdownMemoryOverview,
    MarkdownMemoryProjectAction, MarkdownMemoryProjectView, MarkdownMemoryScopeRequest,
    MarkdownMemorySectionView, MarkdownMemoryUsageView, MarkdownMemoryWriteRequest,
    MemorySettingsSaveRequest, MemorySettingsView,
};

/// A host project as the memory page sees it.
/// 记忆页面所见的宿主项目。
#[derive(Debug, Clone, PartialEq, Eq)]
struct HostProject {
    /// Host project id.
    /// 宿主项目标识。
    id: String,
    /// Project root path.
    /// 项目根路径。
    path: String,
}

/// Serializes one service result.
/// 序列化单个服务结果。
fn to_service_value<T: Serialize>(value: &T) -> Result<Value, String> {
    serde_json::to_value(value)
        .map_err(|error| format!("Failed to serialize the memory result: {error}"))
}

/// Opens the store under the user Vulcan directory.
/// 打开用户 Vulcan 目录下的存储。
fn memory_store() -> Result<MarkdownMemoryStore, String> {
    vulcan_runtime_assets::resolve_user_vulcan_root()
        .map(|root| MarkdownMemoryStore::new(&root))
        .ok_or_else(|| String::from("The user Vulcan directory is unknown."))
}

/// Resolves a host project id to its root through the workspace snapshot.
/// 通过工作区快照将宿主项目标识解析为其根目录。
fn host_project(
    state: &SharedServiceBridgeState,
    project_id: Option<&str>,
) -> Result<Option<HostProject>, String> {
    let Some(project_id) = project_id.map(str::trim).filter(|id| !id.is_empty()) else {
        return Ok(None);
    };
    let guard = state
        .lock()
        .map_err(|error| format!("Failed to lock the service state: {error}"))?;
    let workspace = guard
        .workspace_snapshot
        .as_ref()
        .ok_or_else(|| String::from("The workspace is not loaded."))?;
    workspace_task_project_for_project_id(workspace, project_id)
        .map(|project| {
            Some(HostProject {
                id: project_id.to_string(),
                path: project.path.clone(),
            })
        })
        .ok_or_else(|| format!("Unknown project: {project_id}"))
}

/// Builds the settings view.
/// 构建设置视图。
fn settings_view(settings: &MarkdownMemorySettings) -> MemorySettingsView {
    MemorySettingsView {
        mode: settings.mode,
        budget: settings.budget,
        default_budget: InjectionBudget::default(),
        max_part_tokens: InjectionBudget::MAX_PART_TOKENS,
        warning_percent: BUDGET_WARNING_PERCENT,
    }
}

/// Returns the saved memory settings.
/// 返回已保存的记忆设置。
pub(crate) fn service_get_memory_settings() -> Result<Value, String> {
    let settings = memory_store()?
        .settings()
        .map_err(|error| error.to_string())?;
    to_service_value(&settings_view(&settings))
}

/// Saves the memory settings and republishes the VMM projection for the new mode.
/// 保存记忆设置，并按新模式重新发布 VMM 投影。
pub(crate) fn service_save_memory_settings(
    state: SharedServiceBridgeState,
    request: MemorySettingsSaveRequest,
) -> Result<Value, String> {
    request
        .budget
        .validate()
        .map_err(|error| error.to_string())?;
    let settings = MarkdownMemorySettings {
        mode: request.mode,
        budget: request.budget,
    };
    memory_store()?
        .save_settings(&settings)
        .map_err(|error| error.to_string())?;
    let configuration = provider_configuration_from_state(&state)?;
    let mode_vmm_layer = vulcan_agent_runtime::tooling::project_vmm_layer_for_mode(
        &configuration.vmm,
        settings.mode,
    );
    let effective = vulcan_vmm::VmmEffectiveConfig::resolve(&mode_vmm_layer, None, None);
    let mut guard = state
        .lock()
        .map_err(|error| format!("Failed to lock the service state: {error}"))?;
    if let Some(maintenance) = guard.native_vmm_maintenance.as_ref() {
        maintenance.set_native_backend_enabled(
            effective.enabled && effective.backend_mode == vulcan_vmm::VmmBackendMode::Native,
        );
    }
    refresh_guard_vmm_hook_snapshot(&mut guard, &configuration)?;
    to_service_value(&settings_view(&settings))
}

/// Loads everything the memory page shows.
/// 加载记忆页面显示的全部内容。
pub(crate) fn service_load_markdown_memory(
    state: SharedServiceBridgeState,
    request: MarkdownMemoryScopeRequest,
) -> Result<Value, String> {
    let host = host_project(&state, request.project_id.as_deref())?;
    let overview = load_overview(&memory_store()?, host.as_ref(), request.memory_project_id)
        .map_err(|error| error.to_string())?;
    to_service_value(&overview)
}

/// Applies node operations written by the user and returns the refreshed page.
/// 应用用户写入的节点操作并返回刷新后的页面。
pub(crate) fn service_write_markdown_memory(
    state: SharedServiceBridgeState,
    request: MarkdownMemoryWriteRequest,
) -> Result<Value, String> {
    let host = host_project(&state, request.project_id.as_deref())?;
    let overview = write_as_user(
        &memory_store()?,
        host.as_ref(),
        request.memory_project_id,
        request.operations,
    )?;
    to_service_value(&overview)
}

/// Reads one memory file for whole-file editing.
/// 读取单个记忆文件用于整文件编辑。
pub(crate) fn service_read_markdown_memory_file(
    request: MarkdownMemoryFileRequest,
) -> Result<Value, String> {
    let store = memory_store()?;
    let scope = parse_scope(&request.scope)?;
    let memory_project_id = request.memory_project_id.as_deref();
    let path = store
        .scope_path(scope, memory_project_id)
        .map_err(|error| error.to_string())?;
    let text = store
        .read_file_text(scope, memory_project_id)
        .map_err(|error| error.to_string())?;
    to_service_value(&MarkdownMemoryFileView {
        path: path.display().to_string(),
        text,
    })
}

/// Replaces one memory file if nobody changed it since the editor loaded it.
/// 若编辑器加载后无人修改，则替换单个记忆文件。
pub(crate) fn service_replace_markdown_memory_file(
    request: MarkdownMemoryFileReplaceRequest,
) -> Result<Value, String> {
    let store = memory_store()?;
    let scope = parse_scope(&request.scope)?;
    let memory_project_id = request.memory_project_id.as_deref();
    store
        .replace_file_text(scope, memory_project_id, &request.expected, &request.text)
        .map_err(|error| error.to_string())?;
    let path = store
        .scope_path(scope, memory_project_id)
        .map_err(|error| error.to_string())?;
    to_service_value(&MarkdownMemoryFileView {
        path: path.display().to_string(),
        text: request.text,
    })
}

/// Applies one registry change and returns every memory project.
/// 应用一项注册表变更并返回全部记忆项目。
pub(crate) fn service_apply_markdown_memory_project_action(
    state: SharedServiceBridgeState,
    action: MarkdownMemoryProjectAction,
) -> Result<Value, String> {
    let host_id = match &action {
        MarkdownMemoryProjectAction::Link { project_id, .. }
        | MarkdownMemoryProjectAction::Create { project_id }
        | MarkdownMemoryProjectAction::Ignore { project_id } => Some(project_id.as_str()),
        MarkdownMemoryProjectAction::SetEnabled { .. }
        | MarkdownMemoryProjectAction::Rename { .. } => None,
    };
    let host = host_project(&state, host_id)?;
    let projects = apply_project_action(&memory_store()?, host.as_ref(), action)
        .map_err(|error| error.to_string())?;
    to_service_value(&projects)
}

/// Parses a stable scope key.
/// 解析稳定层键。
fn parse_scope(key: &str) -> Result<MemoryScope, String> {
    MemoryScope::parse(key).ok_or_else(|| format!("`{key}` is not a memory scope."))
}

/// Builds the page overview for a host project and an optional explicit memory project.
/// 为宿主项目及可选的显式记忆项目构建页面概览。
fn load_overview(
    store: &MarkdownMemoryStore,
    host: Option<&HostProject>,
    memory_project_id: Option<String>,
) -> Result<MarkdownMemoryOverview, MemoryError> {
    let settings = store.settings()?;
    let registry = store.registry()?;
    let bound_memory_project_id = host
        .and_then(|host| registry.project_for_host(&host.id))
        .map(|project| project.id.clone());
    let shown_id = memory_project_id.or_else(|| bound_memory_project_id.clone());
    let shown_project = match shown_id.as_deref() {
        Some(id) => Some(registry.project(id).cloned().ok_or_else(|| {
            MemoryError::ProjectUnavailable(format!("Memory project `{id}` does not exist."))
        })?),
        None => None,
    };
    let documents = ScopeDocuments {
        user: store.load(MemoryScope::User, None)?,
        space: store.load(MemoryScope::Space, None)?,
        project: match shown_project {
            Some(project) => {
                let document = store.load(MemoryScope::Project, Some(&project.id))?;
                Some((project, document))
            }
            None => None,
        },
    };
    let usage = build_injection_snapshot(&documents, &settings.budget)
        .usage
        .into_iter()
        .map(|usage| MarkdownMemoryUsageView {
            part: serde_json::to_value(usage.part)
                .ok()
                .and_then(|part| part.as_str().map(str::to_string))
                .unwrap_or_default(),
            tokens: usage.tokens,
            budget: usage.budget,
            omitted: usage.omitted,
        })
        .collect();
    let orphan_candidates = host
        .map(|host| {
            registry
                .orphan_candidates(&host.id, &host.path, None, path_exists)
                .into_iter()
                .map(|candidate| MarkdownMemoryOrphanView {
                    memory_project_id: candidate.project.id,
                    reasons: candidate.reasons.into_iter().map(str::to_string).collect(),
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(MarkdownMemoryOverview {
        settings: settings_view(&settings),
        user: node_views(&documents.user, MemoryScope::User),
        space: node_views(&documents.space, MemoryScope::Space),
        project: documents
            .project
            .as_ref()
            .map(|(_, document)| node_views(document, MemoryScope::Project))
            .unwrap_or_default(),
        memory_project_id: shown_id,
        bound_memory_project_id,
        host_project_id: host.map(|host| host.id.clone()),
        orphan_candidates,
        projects: registry.projects.iter().map(project_view).collect(),
        usage,
        preset_sections: [MemoryScope::User, MemoryScope::Space, MemoryScope::Project]
            .into_iter()
            .flat_map(|scope| {
                preset_sections(scope)
                    .iter()
                    .map(move |(key, title)| MarkdownMemorySectionView {
                        scope: scope.key().to_string(),
                        key: (*key).to_string(),
                        title: (*title).to_string(),
                    })
            })
            .collect(),
    })
}

/// Writes operations as the user; the first project write may create and bind a memory project.
/// 以用户身份写入操作；首次项目写入可能新建并绑定记忆项目。
fn write_as_user(
    store: &MarkdownMemoryStore,
    host: Option<&HostProject>,
    memory_project_id: Option<String>,
    operations: Vec<Value>,
) -> Result<MarkdownMemoryOverview, String> {
    let operations = operations
        .into_iter()
        .enumerate()
        .map(|(index, operation)| {
            serde_json::from_value::<WriteOperation>(operation)
                .map_err(|error| format!("Operation {index} is invalid: {error}"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let settings = store.settings().map_err(|error| error.to_string())?;
    let context = ProjectContext {
        host_project_id: host.map(|host| host.id.clone()),
        root_path: host.map(|host| host.path.clone()),
        memory_project_id: memory_project_id.clone(),
    };
    let receipt = store
        .write(
            &context,
            None,
            NodeSource::User,
            &settings.budget,
            operations,
        )
        .map_err(|error| error.to_string())?;
    load_overview(store, host, memory_project_id.or(receipt.created_project))
        .map_err(|error| error.to_string())
}

/// Applies one registry change.
/// 应用一项注册表变更。
fn apply_project_action(
    store: &MarkdownMemoryStore,
    host: Option<&HostProject>,
    action: MarkdownMemoryProjectAction,
) -> Result<Vec<MarkdownMemoryProjectView>, MemoryError> {
    let now = now_utc_minute();
    let host_path = || {
        host.map(|host| host.path.clone()).ok_or_else(|| {
            MemoryError::ProjectUnavailable(String::from("The host project is unknown."))
        })
    };
    store.update_registry(|registry| {
        match action {
            MarkdownMemoryProjectAction::Link {
                project_id,
                memory_project_id,
            } => registry.link(&memory_project_id, &project_id, &host_path()?, &now)?,
            MarkdownMemoryProjectAction::Create { project_id } => {
                registry.create_for_host(&project_id, &host_path()?, None, &now);
            }
            MarkdownMemoryProjectAction::Ignore { project_id } => registry.ignore_host(&project_id),
            MarkdownMemoryProjectAction::SetEnabled {
                memory_project_id,
                enabled,
            } => registry.set_enabled(&memory_project_id, enabled, &now)?,
            MarkdownMemoryProjectAction::Rename {
                memory_project_id,
                name,
            } => registry.rename(&memory_project_id, &name, &now)?,
        }
        Ok(registry.projects.iter().map(project_view).collect())
    })
}

/// Returns whether a recorded project path still exists.
/// 返回记录的项目路径是否仍存在。
fn path_exists(path: &str) -> bool {
    Path::new(path).exists()
}

/// Builds the view of one memory project.
/// 构建单个记忆项目的视图。
fn project_view(project: &MemoryProject) -> MarkdownMemoryProjectView {
    MarkdownMemoryProjectView {
        id: project.id.clone(),
        name: project.name.clone(),
        host_project_ids: project.host_project_ids.clone(),
        paths: project.paths.clone(),
        memory_enabled: project.memory_enabled,
        orphaned: !project.paths.is_empty() && !project.paths.iter().any(|path| path_exists(path)),
        updated: project.updated.clone(),
    }
}

/// Builds the node views of one document.
/// 构建单个文档的节点视图。
fn node_views(document: &MemoryDocument, scope: MemoryScope) -> Vec<MarkdownMemoryNodeView> {
    document.views(scope).into_iter().map(node_view).collect()
}

/// Builds the view of one node.
/// 构建单个节点的视图。
fn node_view(view: NodeView) -> MarkdownMemoryNodeView {
    let meta = view.meta.as_ref();
    MarkdownMemoryNodeView {
        id: view.id,
        level: view.level,
        path: view.path,
        section: view.section,
        revision: view.revision,
        body: view.body,
        created: meta.map(|meta| meta.created.clone()),
        updated: meta.map(|meta| meta.updated.clone()),
        session: meta.and_then(|meta| meta.session.clone()),
        source: meta.map(|meta| meta.source.key().to_string()),
        descendants: view.descendants,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Opens a store in a fresh temporary user root.
    /// 在新的临时用户根目录中打开存储。
    fn temporary_store() -> (tempfile::TempDir, MarkdownMemoryStore) {
        let root = tempfile::tempdir().expect("temp root");
        let store = MarkdownMemoryStore::new(root.path());
        (root, store)
    }

    /// A host project rooted in an existing directory.
    /// 根目录真实存在的宿主项目。
    fn existing_host(root: &tempfile::TempDir) -> HostProject {
        HostProject {
            id: String::from("host-1"),
            path: root.path().display().to_string(),
        }
    }

    /// The user's first project write creates and binds a memory project and the page shows it.
    /// 用户首次项目写入会新建并绑定记忆项目，页面随即显示它。
    #[test]
    fn user_project_write_binds_and_shows_the_project() {
        let (root, store) = temporary_store();
        let host = existing_host(&root);
        let overview = write_as_user(
            &store,
            Some(&host),
            None,
            vec![json!({
                "action": "create", "scope": "project", "section": "base",
                "title": "Build with make", "body": "Use make.ps1.",
            })],
        )
        .expect("write");
        let bound = overview.bound_memory_project_id.clone().expect("bound");
        assert_eq!(overview.memory_project_id.as_deref(), Some(bound.as_str()));
        assert!(
            overview
                .project
                .iter()
                .any(|node| node.body == "Use make.ps1." && node.source.as_deref() == Some("user"))
        );
        assert!(
            overview
                .projects
                .iter()
                .any(|project| project.id == bound && !project.orphaned)
        );
    }

    /// The user may still edit a project whose memory is off.
    /// 用户仍可编辑已关闭记忆的项目。
    #[test]
    fn user_writes_reach_a_disabled_project() {
        let (root, store) = temporary_store();
        let host = existing_host(&root);
        apply_project_action(
            &store,
            Some(&host),
            MarkdownMemoryProjectAction::Create {
                project_id: host.id.clone(),
            },
        )
        .expect("create");
        let id = store
            .registry()
            .expect("registry")
            .project_for_host(&host.id)
            .expect("bound")
            .id
            .clone();
        apply_project_action(
            &store,
            None,
            MarkdownMemoryProjectAction::SetEnabled {
                memory_project_id: id.clone(),
                enabled: false,
            },
        )
        .expect("disable");
        let overview = write_as_user(
            &store,
            Some(&host),
            None,
            vec![json!({
                "action": "create", "scope": "project", "section": "base",
                "title": "Keep notes", "body": "Still editable.",
            })],
        )
        .expect("user write");
        assert!(
            overview
                .project
                .iter()
                .any(|node| node.body == "Still editable.")
        );
    }

    /// An orphaned project is suggested by directory name and can be shown explicitly.
    /// 孤立项目按目录名被建议，并可被显式显示。
    #[test]
    fn orphans_are_suggested_and_shown_on_request() {
        let (root, store) = temporary_store();
        let gone = root.path().join("gone").join("vulcan-code");
        let old_host = HostProject {
            id: String::from("old-host"),
            path: gone.display().to_string(),
        };
        apply_project_action(
            &store,
            Some(&old_host),
            MarkdownMemoryProjectAction::Create {
                project_id: old_host.id.clone(),
            },
        )
        .expect("create");
        let orphan = store.registry().expect("registry").projects[0].id.clone();
        let current = root.path().join("vulcan-code");
        std::fs::create_dir_all(&current).expect("current dir");
        let host = HostProject {
            id: String::from("new-host"),
            path: current.display().to_string(),
        };
        let overview = load_overview(&store, Some(&host), None).expect("overview");
        assert!(overview.bound_memory_project_id.is_none());
        let candidate = overview
            .orphan_candidates
            .iter()
            .find(|candidate| candidate.memory_project_id == orphan)
            .expect("orphan suggested");
        assert!(
            candidate
                .reasons
                .iter()
                .any(|reason| reason == "same_directory_name")
        );
        assert!(
            overview
                .projects
                .iter()
                .any(|project| project.id == orphan && project.orphaned)
        );

        let shown = load_overview(&store, Some(&host), Some(orphan.clone())).expect("explicit");
        assert_eq!(shown.memory_project_id.as_deref(), Some(orphan.as_str()));

        apply_project_action(
            &store,
            Some(&host),
            MarkdownMemoryProjectAction::Link {
                project_id: host.id.clone(),
                memory_project_id: orphan.clone(),
            },
        )
        .expect("link");
        let linked = load_overview(&store, Some(&host), None).expect("linked");
        assert_eq!(
            linked.bound_memory_project_id.as_deref(),
            Some(orphan.as_str())
        );
        assert!(linked.orphan_candidates.is_empty());
    }

    /// An explicit memory project that does not exist is refused.
    /// 不存在的显式记忆项目会被拒绝。
    #[test]
    fn unknown_explicit_memory_project_is_refused() {
        let (_root, store) = temporary_store();
        let error = load_overview(&store, None, Some(String::from("p-missing")))
            .expect_err("missing project");
        assert!(matches!(error, MemoryError::ProjectUnavailable(_)));
    }
}
