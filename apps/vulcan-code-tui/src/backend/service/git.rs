// Git sidebar over the service's `workspace.*` methods.
// 基于服务 `workspace.*` 方法的 Git 侧栏。
//
// The workspace is the selected conversation's project (the first project when nothing is
// selected) and the repository is the catalog's primary one. The unstaged and staged snapshots are
// merged into one `GitSnapshot`; their wire file identities are kept for file actions and details.
// 工作区为选中会话所属的项目（未选中时取第一个项目），仓库为目录中的主仓库。未暂存与已暂存快照
// 合并为一个 `GitSnapshot`；其传输文件标识被保留，用于文件操作与详情。

use std::collections::{HashMap, HashSet};

use serde_json::{Value, json};

use super::map::{self, GitFileRef};
use super::wire::*;
use super::worker::Worker;
use crate::backend::{BackendEvent, GitBusy, GitNotice, Notice};
use crate::model::*;

/// Keeps the selected service repository and its exact staged/unstaged file identities.
/// 保存所选服务仓库及其准确的暂存、未暂存文件身份。
#[derive(Default)]
pub struct GitState {
    /// Selected absolute workspace path, absent before repository discovery.
    /// 所选绝对工作区路径，在仓库发现前为空。
    workspace: Option<String>,
    /// Host project identity owning the selected workspace.
    /// 持有所选工作区的宿主项目身份。
    project_id: Option<String>,
    /// Selected monitored repository that details and actions target.
    /// 详情与操作所指向的已选受监听仓库。
    repository_key: Option<String>,
    /// Latest repository catalog with its review counts.
    /// 最新的仓库目录及其审查计数。
    repositories: Vec<GitRepository>,
    /// Latest service identities for changes outside the staging area.
    /// 暂存区外变更的最新服务身份。
    unstaged: Vec<GitFileRef>,
    /// Latest service identities for changes inside the staging area.
    /// 暂存区内变更的最新服务身份。
    staged: Vec<GitFileRef>,
    /// Last loaded views of each monitored repository, so switching repositories reads memory.
    /// 每个受监听仓库最近加载的视图，使切换仓库只读内存。
    views: HashMap<String, RepositoryViews>,
    /// The catalog has been read, so memory can be shown before any reread.
    /// 目录已读取，因此可在重读前先显示内存内容。
    catalog_loaded: bool,
    /// Repositories that changed while the project was not shown; they are reread on return.
    /// 项目未显示期间发生变化的仓库；回到该项目时重读。
    dirty: HashSet<String>,
}

/// What a Git reload rereads; repositories without views in memory are always read.
/// Git 重载重读的范围；内存中没有视图的仓库总会读取。
pub(super) enum GitReload<'a> {
    /// The catalog and every monitored repository.
    /// 目录及每个受监听仓库。
    All,
    /// The catalog alone, as after a monitoring switch.
    /// 仅目录，例如监听开关变化之后。
    Catalog,
    /// The named repositories, without the catalog.
    /// 指定的仓库，不读目录。
    Repositories(&'a [String]),
}

/// Unstaged and staged diff snapshots of one repository, as the service returned them.
/// 单个仓库的未暂存与已暂存差异快照，保持服务返回的原样。
#[derive(Clone)]
struct RepositoryViews {
    unstaged: Value,
    staged: Value,
}

impl GitState {
    /// Marks repositories of a project that is not shown as changed: `keys`, or every monitored
    /// repository when the event names none.
    /// 将未显示项目的仓库标记为已变化：`keys`，事件未指明仓库时标记每个受监听仓库。
    fn mark_dirty(&mut self, keys: &[String]) {
        if keys.is_empty() {
            let monitored = self.repositories.iter().filter(|r| r.monitoring);
            self.dirty.extend(monitored.map(|r| r.key.clone()));
        } else {
            self.dirty.extend(keys.iter().cloned());
        }
    }

    /// Whether a `workspaceGitChanged` path concerns the current workspace.
    /// `workspaceGitChanged` 的路径是否属于当前工作区。
    /// `path` is the exact requested path.
    /// `path` 为准确请求路径。
    /// Returns `bool`.
    /// 返回 `bool`。
    pub fn matches(&self, path: &str) -> bool {
        // Norm.
        // 路径规范化函数。
        let norm = |s: &str| {
            s.trim_end_matches(['/', '\\'])
                .replace('\\', "/")
                .to_lowercase()
        };
        self.workspace
            .as_deref()
            .is_some_and(|w| path.is_empty() || norm(w) == norm(path))
    }
}

/// Serializes the service-issued Git file identity for actions and detail requests.
/// 为动作及详情请求序列化服务签发的 Git 文件身份。
/// `file` is the service-issued Git file identity.
/// `file` 为服务签发的 Git 文件身份。
/// Returns `Value`.
/// 返回 `Value`。
fn file_ref(file: &GitFileRef) -> Value {
    json!({ "oldPath": file.old_path, "newPath": file.new_path, "status": file.status })
}

/// Uses a Git file new path or its declared deleted-file old path.
/// 使用 Git 文件的新路径，或协议规定的已删除文件旧路径。
/// `file` is the service-issued Git file identity.
/// `file` 为服务签发的 Git 文件身份。
/// Returns `&str`.
/// 返回 `&str`。
fn path_of(file: &GitFileRef) -> &str {
    if file.new_path.is_empty() {
        &file.old_path
    } else {
        &file.new_path
    }
}

impl Worker {
    /// Common `{workspacePath, projectId, conversationId}` envelope plus extra fields.
    /// 公共 `{workspacePath, projectId, conversationId}` 信封及附加字段。
    /// Returns `Value`.
    /// 返回 `Value`。
    fn workspace_params(&self, extra: Value) -> Value {
        // Exact RPC parameters.
        // 准确 RPC 参数。
        let mut params = json!({
            "workspacePath": self.git.workspace,
            "projectId": self.git.project_id,
            "conversationId": self.selected_real(),
        });
        if let (Some(target), Value::Object(extra)) = (params.as_object_mut(), extra) {
            target.extend(extra);
        }
        params
    }

    /// Rereads the catalog and every monitored repository of the selected project, as
    /// `refreshProjectGitSummary`; an explicit refresh and a new connection use it.
    /// 重读所选项目的目录及每个受监听仓库，对应 `refreshProjectGitSummary`；显式刷新与新连接使用。
    pub(super) fn git_refresh(&mut self) {
        self.git_reload(GitReload::All);
    }

    /// Shows the selected conversation's project from memory, rereading only the repositories that
    /// changed while it was not shown; a project never read loads in full.
    /// 从内存显示所选会话的项目，只重读其未显示期间发生变化的仓库；从未读取的项目完整加载。
    pub(super) fn git_show(&mut self) {
        if !self.git_enter_project() {
            return;
        }
        if !self.git.catalog_loaded {
            self.git_reload(GitReload::All);
            return;
        }
        self.emit_git_selected();
        if !self.git.dirty.is_empty() {
            let keys: Vec<String> = self.git.dirty.iter().cloned().collect();
            self.git_reload(GitReload::Repositories(&keys));
        }
    }

    /// Rereads the selected repository after an action changed it.
    /// 在操作改变所选仓库后重读该仓库。
    pub(super) fn git_reload_selected(&mut self) {
        let keys: Vec<String> = self.git.repository_key.iter().cloned().collect();
        self.git_reload(GitReload::Repositories(&keys));
    }

    /// Applies a `workspaceGitChanged` event: the shown project rereads the named repositories,
    /// every monitored one when the event names none; a project kept in memory marks them for its
    /// return.
    /// 应用 `workspaceGitChanged` 事件：显示中的项目重读指明的仓库，未指明时重读每个受监听仓库；
    /// 内存中保留的项目将其标记，待回到该项目时重读。
    /// `path` is the event's workspace path; `keys` its repository keys.
    /// `path` 为事件的工作区路径；`keys` 为其仓库键。
    pub(super) fn git_changed(&mut self, path: &str, keys: &[String]) {
        if self.git.matches(path) {
            let keys: Vec<String> = if keys.is_empty() {
                let monitored = self.git.repositories.iter().filter(|r| r.monitoring);
                monitored.map(|r| r.key.clone()).collect()
            } else {
                keys.to_vec()
            };
            self.git_reload(GitReload::Repositories(&keys));
            return;
        }
        for state in self.git_parked.values_mut() {
            if state.matches(path) {
                state.mark_dirty(keys);
            }
        }
    }

    /// A new connection may have lost the service's watches and missed events, so every project
    /// kept in memory reads its catalog again on return, and the shown one rereads now.
    /// 新连接可能丢失服务端监听并错过事件，因此内存中的每个项目回到时重读目录，显示中的项目立即重读。
    pub(super) fn git_reconnected(&mut self) {
        for state in self.git_parked.values_mut() {
            state.catalog_loaded = false;
        }
        self.git_refresh();
    }

    /// Makes the selected conversation's project the shown Git state, keeping the previous
    /// project's state in memory.
    /// 将所选会话的项目设为显示中的 Git 状态，并在内存中保留上一项目的状态。
    /// Returns `false` without a selected project.
    /// 没有所选项目时返回 `false`。
    fn git_enter_project(&mut self) -> bool {
        // Project.
        // 所选项目数据。
        let Some(project) = self.selected_project() else {
            return false;
        };
        // Workspace; Project id.
        // 所选 Git 工作区；准确宿主项目身份。
        let (workspace, project_id) = (project.path.clone(), project.id.clone());
        if self.git.workspace.as_deref() == Some(workspace.as_str()) {
            return true;
        }
        let entering = self
            .git_parked
            .remove(&workspace)
            .unwrap_or_else(|| GitState {
                workspace: Some(workspace),
                project_id: Some(project_id),
                ..GitState::default()
            });
        let leaving = std::mem::replace(&mut self.git, entering);
        if let Some(path) = leaving.workspace.clone() {
            self.git_parked.insert(path, leaving);
        }
        // The previous project's repositories must not stay on screen.
        // 不得继续显示上一项目的仓库。
        self.emit(BackendEvent::GitCleared);
        true
    }

    /// Rereads what `scope` names for the selected project, then publishes from memory; a project
    /// whose catalog was never read loads in full.
    /// 为所选项目重读 `scope` 指定的内容，随后从内存发布；从未读取目录的项目完整加载。
    pub(super) fn git_reload(&mut self, scope: GitReload) {
        if !self.git_enter_project() {
            return;
        }
        let scope = if self.git.catalog_loaded {
            scope
        } else {
            GitReload::All
        };
        // Catalog read; Repositories reread whatever their memory.
        // 是否读取目录；无论内存如何都重读的仓库。
        let (read_catalog, only): (bool, Option<Vec<String>>) = match scope {
            GitReload::All => (true, None),
            GitReload::Catalog => (true, Some(Vec::new())),
            GitReload::Repositories(keys) => (false, Some(keys.to_vec())),
        };
        if read_catalog {
            // Catalog.
            // 服务发布的目录。
            let Some(catalog) =
                self.call("workspace.git.catalog", self.workspace_params(json!({})))
            else {
                // The screen keeps its last state, and a project read for the first time still
                // leaves the loading state.
                // 画面保留最近状态，首次读取的项目仍需离开加载状态。
                self.emit_git_selected();
                return;
            };
            self.git.repositories = arr(&catalog, "repositories")
                .iter()
                .map(|repository| GitRepository {
                    key: str_at(repository, "repositoryKey"),
                    name: str_at(repository, "name"),
                    relative_path: str_at(repository, "relativePath"),
                    primary: bool_at(repository, "isPrimary"),
                    monitoring: bool_at(repository, "monitoringEnabled"),
                    changes: None,
                    ahead: 0,
                })
                .collect();
            self.git.catalog_loaded = true;
        }
        let repositories = &self.git.repositories;
        // The current choice stays while monitored; otherwise the primary, then the first
        // monitored repository.
        // 当前选择在仍受监听时保留；否则依次取主仓库、第一个受监听仓库。
        let monitored = |key: &str| repositories.iter().any(|r| r.monitoring && r.key == key);
        let primary = repositories
            .iter()
            .find(|r| r.primary)
            .or(repositories.first())
            .map(|r| r.key.clone());
        let selected = self
            .git
            .repository_key
            .clone()
            .filter(|key| monitored(key))
            .or(primary.filter(|key| monitored(key)))
            .or_else(|| {
                repositories
                    .iter()
                    .find(|r| r.monitoring)
                    .map(|r| r.key.clone())
            });
        let keys: Vec<String> = repositories
            .iter()
            .filter(|r| r.monitoring)
            .map(|r| r.key.clone())
            .collect();
        self.git.repository_key = selected;
        self.git.views.retain(|key, _| keys.contains(key));
        self.git.dirty.retain(|key| keys.contains(key));
        // Snapshot.
        // 权威快照。
        let snapshot = |worker: &mut Worker, key: &str, mode: &str| {
            // Exact RPC parameters.
            // 准确 RPC 参数。
            let params = worker.workspace_params(json!({
                "request": { "repositoryKey": key, "mode": mode },
            }));
            worker.call("workspace.diff.snapshot", params)
        };
        for key in keys {
            let requested = only.as_ref().is_none_or(|only| only.contains(&key));
            if !requested && self.git.views.contains_key(&key) {
                continue;
            }
            // A failed read keeps the repository's previous views and its mark.
            // 读取失败时保留该仓库先前的视图及其标记。
            self.git.dirty.insert(key.clone());
            let views = snapshot(self, &key, "unstaged")
                .and_then(|unstaged| Some((unstaged, snapshot(self, &key, "staged")?)));
            if let Some((unstaged, staged)) = views {
                self.git.dirty.remove(&key);
                self.git
                    .views
                    .insert(key, RepositoryViews { unstaged, staged });
            }
        }
        let views = &self.git.views;
        for repository in &mut self.git.repositories {
            let view = views.get(&repository.key).filter(|_| repository.monitoring);
            repository.changes = view.map(|view| {
                map::git_file_refs(&view.unstaged).len() + map::git_file_refs(&view.staged).len()
            });
            repository.ahead = view.map_or(0, |view| {
                u64_at(&view.unstaged, "currentBranchUnpushedCommits") as u32
            });
        }
        self.emit_git_selected();
    }

    /// Publishes the selected repository from memory; without a selected or readable repository
    /// the panel shows the catalog alone.
    /// 从内存发布所选仓库；没有已选或可读仓库时面板只显示目录。
    fn emit_git_selected(&mut self) {
        // An empty worker has no workspace and cannot publish a repository snapshot.
        // 空工作器没有工作区，不能发布仓库快照。
        let Some(workspace) = self.git.workspace.clone() else {
            return;
        };
        let repositories = self.git.repositories.clone();
        let views = self
            .git
            .repository_key
            .as_ref()
            .and_then(|key| self.git.views.get(key))
            .cloned();
        let Some(RepositoryViews { unstaged, staged }) = views else {
            self.git.unstaged.clear();
            self.git.staged.clear();
            self.emit(BackendEvent::Git(GitSnapshot {
                workspace,
                repositories,
                selected: None,
                branch: String::new(),
                upstream: None,
                ahead: 0,
                behind: 0,
                files: Vec::new(),
                unpushed: Vec::new(),
            }));
            return;
        };
        self.git.unstaged = map::git_file_refs(&unstaged);
        self.git.staged = map::git_file_refs(&staged);
        // Files.
        // 文件集合。
        let mut files = map::git_files(&staged, true);
        files.extend(map::git_files(&unstaged, false));
        // Ahead.
        // 未推送提交数量。
        let ahead = u64_at(&unstaged, "currentBranchUnpushedCommits") as usize;
        self.emit(BackendEvent::Git(GitSnapshot {
            workspace,
            repositories,
            selected: self.git.repository_key.clone(),
            branch: str_at(&unstaged, "currentBranch"),
            upstream: opt_str(&unstaged, "defaultBranch"),
            ahead: ahead as u32,
            behind: 0,
            files,
            unpushed: map::git_commits(&unstaged, ahead),
        }));
    }

    /// Points details and actions at one monitored repository. Loaded repositories switch from
    /// memory without a request; one never read is loaded alone. Every call publishes, so the
    /// panel always learns where the selection landed.
    /// 将详情与操作指向一个受监听仓库。已加载的仓库从内存切换，不发请求；从未读取的仓库单独加载。
    /// 每次调用都会发布，使面板总能得知选择的落点。
    /// `key` is the exact repository key from the catalog.
    /// `key` 为目录中的准确仓库键。
    pub(super) fn git_select_repository(&mut self, key: String) {
        let monitored = self
            .git
            .repositories
            .iter()
            .any(|r| r.monitoring && r.key == key);
        if monitored {
            self.git.repository_key = Some(key.clone());
        }
        if monitored && (!self.git.views.contains_key(&key) || self.git.dirty.contains(&key)) {
            self.git_reload(GitReload::Repositories(&[key]));
        } else {
            self.emit_git_selected();
        }
    }

    /// Persists one repository's monitoring switch, then rereads the project.
    /// 持久化单个仓库的监听开关，随后重读项目。
    /// `key` is the exact repository key; `enabled` the new switch state.
    /// `key` 为准确仓库键；`enabled` 为新的开关状态。
    pub(super) fn git_set_monitoring(&mut self, key: String, enabled: bool) {
        let params = self.workspace_params(json!({
            "request": { "repositoryKey": key, "enabled": enabled },
        }));
        if self.call("workspace.git.setMonitoring", params).is_some() {
            self.git_reload(GitReload::Catalog);
        }
    }

    /// Sends stage or unstage actions for exact displayed service file identities.
    /// 为显示的准确服务文件身份发送暂存或取消暂存动作。
    /// `paths` is the exact displayed Git file paths.
    /// `paths` 为显示的准确 Git 文件路径。
    /// `stage` is whether to stage or unstage the files.
    /// `stage` 为是否暂存或取消暂存文件。
    pub(super) fn git_stage(&mut self, paths: Vec<String>, stage: bool) {
        // Source.
        // 当前来源。
        let source = if stage {
            &self.git.unstaged
        } else {
            &self.git.staged
        };
        // Files.
        // 文件集合。
        let files: Vec<Value> = source
            .iter()
            .filter(|f| paths.iter().any(|p| p == path_of(f)))
            .map(file_ref)
            .collect();
        if files.is_empty() {
            return;
        }
        // Exact RPC parameters.
        // 准确 RPC 参数。
        let params = self.workspace_params(json!({
            "request": {
                "repositoryKey": self.git.repository_key,
                "action": if stage { "stageFile" } else { "unstageFile" },
                "files": files,
            },
        }));
        if self.call("workspace.git.fileAction", params).is_some() {
            self.git_reload_selected();
        }
    }

    /// Loads the exact diff identified by `key`; stale owners never send a service request.
    /// 加载 `key` 指定的准确差异；过期归属绝不发送服务请求。
    pub(super) fn git_diff(&mut self, key: GitDiffKey) {
        if self.git.workspace.as_ref() != Some(&key.workspace)
            || self.git.repository_key.as_ref() != Some(&key.repository_key)
        {
            self.emit(BackendEvent::GitDiffFailed { key, message: None });
            return;
        }
        // The selected row, rather than a path lookup order, determines the comparison mode.
        // 由选中行决定比较模式，而非路径查询顺序。
        let (mode, files) = if key.staged {
            ("staged", &self.git.staged)
        } else {
            ("unstaged", &self.git.unstaged)
        };
        // Resolve only inside the declared mode's service-issued file identities.
        // 仅在声明模式中由服务签发的文件身份内查找。
        let Some(file) = files.iter().find(|file| path_of(file) == key.path) else {
            self.emit(BackendEvent::GitDiffFailed { key, message: None });
            return;
        };
        // Exact RPC parameters.
        // 准确 RPC 参数。
        let params = self.workspace_params(json!({
            "request": { "repositoryKey": self.git.repository_key, "mode": mode },
            "file": { "key": file.key, "oldPath": file.old_path, "newPath": file.new_path },
        }));
        let Some(client) = self.client.clone() else {
            self.emit(BackendEvent::Notice(Notice::NotConnected));
            self.emit(BackendEvent::GitDiffFailed { key, message: None });
            return;
        };
        match client.call("workspace.diff.fileDetail", params) {
            Ok(detail) => self.emit(BackendEvent::GitDiff {
                diff: map::unified_diff(&str_at(&detail, "unifiedDiff")),
                key,
            }),
            // The diff view shows the reason in place of its loading state.
            // diff 视图以失败原因取代其加载状态。
            Err(error) => {
                let message = Some(error.message.clone());
                self.failed(error);
                self.emit(BackendEvent::GitDiffFailed { key, message });
            }
        }
    }

    /// Loads the branch catalog of the selected workspace repository.
    /// 加载所选工作区仓库的分支目录。
    pub(super) fn git_branches(&mut self) {
        // Exact RPC parameters.
        // 准确 RPC 参数。
        let params = self.workspace_params(json!({
            "request": { "repositoryKey": self.git.repository_key },
        }));
        if let Some(list) = self.call("workspace.git.branches", params) {
            // Current.
            // 当前选中或已读取值。
            let current = str_at(&list, "currentBranch");
            // Branches.
            // 服务发布的分支目录。
            let branches = arr(&list, "branches")
                .iter()
                .filter_map(text)
                .map(|name| GitBranch {
                    current: name == current,
                    remote: false,
                    last_commit: String::new(),
                    name,
                })
                .collect();
            self.emit(BackendEvent::GitBranches(branches));
        }
    }

    /// Switches or creates the requested branch through the existing workspace service.
    /// 通过既有工作区服务切换或创建请求的分支。
    /// `name` is the exact requested catalog or branch name.
    /// `name` 为准确请求的目录或分支名称。
    /// `create` is whether to create the requested branch.
    /// `create` 为是否创建请求的分支。
    pub(super) fn git_switch(&mut self, name: String, create: bool) {
        // Exact RPC parameters.
        // 准确 RPC 参数。
        let params = self.workspace_params(json!({
            "request": {
                "repositoryKey": self.git.repository_key,
                "branchName": name,
                "create": create,
            },
        }));
        if let Some(result) = self.call("workspace.git.switchBranch", params) {
            // Branch.
            // 请求的分支。
            let branch = opt_str(&result, "branchName").unwrap_or(name);
            self.emit(BackendEvent::GitNotice(if bool_at(&result, "created") {
                GitNotice::Created(branch)
            } else {
                GitNotice::Switched(branch)
            }));
            self.git_reload_selected();
        }
    }

    /// Asynchronously requests a commit message for the selected workspace.
    /// 异步请求所选工作区的提交信息。
    /// `request` is echoed in the result; `include_unstaged` is the dialog's own choice.
    /// `request` 随结果回传；`include_unstaged` 为对话框自身的选择。
    pub(super) fn git_generate_message(&mut self, request: u64, include_unstaged: bool) {
        // Exact protocol request.
        // 协议请求文档。
        let params = json!({
            "request": {
                "conversationId": self.selected_real(),
                "workspacePath": self.git.workspace,
                "repositoryKey": self.git.repository_key,
                "includeUnstaged": include_unstaged,
            },
        });
        self.emit(BackendEvent::GitBusy(Some(GitBusy::Generating)));
        self.spawn_call(
            "service_generate_git_commit_message",
            params,
            move |worker, result| {
                worker.emit(BackendEvent::GitBusy(None));
                match result {
                    Ok(value) => worker.emit(BackendEvent::CommitMessage {
                        request,
                        text: str_at(&value, "message"),
                        done: true,
                    }),
                    Err(error) => worker.failed(error),
                }
            },
        );
    }

    /// Commits and/or pushes; an empty message with `push` only pushes.
    /// 提交和/或推送；消息为空且 `push` 时只推送。
    /// `message` is the displayed or sanitized error text.
    /// `message` 为显示文案或脱敏错误文本。
    pub(super) fn git_commit(
        &mut self,
        message: String,
        new_branch: Option<String>,
        include_unstaged: bool,
        push: bool,
    ) {
        // Target.
        // 已声明目标。
        let target = match new_branch {
            Some(branch) => json!({ "mode": "new", "branchName": branch }),
            None => json!({ "mode": "current" }),
        };
        // Exact protocol request.
        // 协议请求文档。
        let request = json!({
            "conversationId": self.selected_real(),
            "workspacePath": self.git.workspace,
            "repositoryKey": self.git.repository_key,
            "target": target,
            "includeUnstaged": include_unstaged,
            "message": message,
            "push": push,
        });
        // Busy.
        // Git 操作等待状态。
        let busy = if message.is_empty() {
            GitBusy::Pushing
        } else {
            GitBusy::Committing
        };
        self.emit(BackendEvent::GitBusy(Some(busy)));
        self.spawn_call(
            "service_run_git_commit_action",
            json!({ "request": request }),
            |worker, result| {
                worker.emit(BackendEvent::GitBusy(None));
                match result {
                    Ok(value) => {
                        if bool_at(&value, "committed") {
                            worker.emit(BackendEvent::GitNotice(GitNotice::Committed));
                        }
                        if bool_at(&value, "pushed") {
                            worker.emit(BackendEvent::GitNotice(GitNotice::Pushed));
                        }
                        worker.git_reload_selected();
                    }
                    Err(error) => worker.failed(error),
                }
            },
        );
    }

    /// Fast-forwards the selected repository's current branch from its upstream.
    /// 从上游快进所选仓库的当前分支。
    pub(super) fn git_pull(&mut self) {
        let request = json!({
            "conversationId": self.selected_real(),
            "workspacePath": self.git.workspace,
            "repositoryKey": self.git.repository_key,
        });
        self.emit(BackendEvent::GitBusy(Some(GitBusy::Pulling)));
        self.spawn_call(
            "service_run_git_pull_action",
            json!({ "request": request }),
            |worker, result| {
                worker.emit(BackendEvent::GitBusy(None));
                match result {
                    Ok(value) => {
                        worker.emit(BackendEvent::GitNotice(GitNotice::Pulled {
                            updated: bool_at(&value, "updated"),
                        }));
                        worker.git_reload_selected();
                    }
                    Err(error) => worker.failed(error),
                }
            },
        );
    }
}
