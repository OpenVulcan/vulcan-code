// Git sidebar over the service's `workspace.*` methods.
// 基于服务 `workspace.*` 方法的 Git 侧栏。
//
// The workspace is the selected conversation's project (the first project when nothing is
// selected) and the repository is the catalog's primary one. The unstaged and staged snapshots are
// merged into one `GitSnapshot`; their wire file identities are kept for file actions and details.
// 工作区为选中会话所属的项目（未选中时取第一个项目），仓库为目录中的主仓库。未暂存与已暂存快照
// 合并为一个 `GitSnapshot`；其传输文件标识被保留，用于文件操作与详情。

use serde_json::{Value, json};

use super::map::{self, GitFileRef};
use super::wire::*;
use super::worker::Worker;
use crate::backend::{BackendEvent, GitBusy, GitNotice};
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
    /// Exact repository identity returned by the service repository catalog.
    /// 服务仓库目录返回的准确仓库身份。
    repository_key: Option<String>,
    /// Latest service identities for changes outside the staging area.
    /// 暂存区外变更的最新服务身份。
    unstaged: Vec<GitFileRef>,
    /// Latest service identities for changes inside the staging area.
    /// 暂存区内变更的最新服务身份。
    staged: Vec<GitFileRef>,
}

impl GitState {
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

    /// Loads repository catalogs and both staged and unstaged views for the selected workspace.
    /// 为所选工作区加载仓库目录及已暂存、未暂存视图。
    pub(super) fn git_refresh(&mut self) {
        // Project.
        // 所选项目数据。
        let Some(project) = self.selected_project() else {
            return;
        };
        // Workspace; Project id.
        // 所选 Git 工作区；准确宿主项目身份。
        let (workspace, project_id) = (project.path.clone(), project.id.clone());
        if self.git.workspace.as_deref() != Some(workspace.as_str()) {
            self.git = GitState {
                workspace: Some(workspace),
                project_id: Some(project_id),
                ..GitState::default()
            };
        }
        if self.git.repository_key.is_none() {
            // Catalog.
            // 服务发布的目录。
            let catalog = self.call("workspace.git.catalog", self.workspace_params(json!({})));
            // Catalog.
            // 服务发布的目录。
            let Some(catalog) = catalog else {
                return;
            };
            // Repositories.
            // 服务发布的仓库目录。
            let repositories = arr(&catalog, "repositories");
            self.git.repository_key = repositories
                .iter()
                .find(|r| bool_at(r, "isPrimary"))
                .or_else(|| repositories.first())
                .and_then(|r| opt_str(r, "repositoryKey"));
            if self.git.repository_key.is_none() {
                return;
            }
        }
        // Snapshot.
        // 权威快照。
        let snapshot = |worker: &mut Worker, mode: &str| {
            // Exact RPC parameters.
            // 准确 RPC 参数。
            let params = worker.workspace_params(json!({
                "request": { "repositoryKey": worker.git.repository_key, "mode": mode },
            }));
            worker.call("workspace.diff.snapshot", params)
        };
        // Unstaged.
        // 未暂存差异。
        let Some(unstaged) = snapshot(self, "unstaged") else {
            return;
        };
        // Staged.
        // 已暂存差异。
        let Some(staged) = snapshot(self, "staged") else {
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
            branch: str_at(&unstaged, "currentBranch"),
            upstream: opt_str(&unstaged, "defaultBranch"),
            ahead: ahead as u32,
            behind: 0,
            files,
            unpushed: map::git_commits(&unstaged, ahead),
        }));
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
            self.git_refresh();
        }
    }

    /// Loads one file's diff, preferring its unstaged changes.
    /// 加载单个文件的 diff，优先取未暂存的变更。
    /// `path` is the exact requested path.
    /// `path` 为准确请求路径。
    pub(super) fn git_diff(&mut self, path: String) {
        // Found.
        // 是否找到有效结果。
        let found = [
            ("unstaged", &self.git.unstaged),
            ("staged", &self.git.staged),
        ]
        .into_iter()
        .find_map(|(mode, refs)| {
            refs.iter()
                .find(|f| path_of(f) == path)
                .map(|f| (mode, f.clone()))
        });
        // Mode; File.
        // 已声明操作模式；当前文件数据。
        let Some((mode, file)) = found else {
            return;
        };
        // Exact RPC parameters.
        // 准确 RPC 参数。
        let params = self.workspace_params(json!({
            "request": { "repositoryKey": self.git.repository_key, "mode": mode },
            "file": { "key": file.key, "oldPath": file.old_path, "newPath": file.new_path },
        }));
        if let Some(detail) = self.call("workspace.diff.fileDetail", params) {
            self.emit(BackendEvent::GitDiff(map::unified_diff(
                &path,
                &str_at(&detail, "unifiedDiff"),
            )));
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
            self.git_refresh();
        }
    }

    /// Asynchronously requests a commit message for the selected workspace.
    /// 异步请求所选工作区的提交信息。
    pub(super) fn git_generate_message(&mut self) {
        // Exact protocol request.
        // 协议请求文档。
        let request = json!({
            "request": {
                "conversationId": self.selected_real(),
                "workspacePath": self.git.workspace,
                "repositoryKey": self.git.repository_key,
                "includeUnstaged": self.git.staged.is_empty(),
            },
        });
        self.emit(BackendEvent::GitBusy(Some(GitBusy::Generating)));
        self.spawn_call(
            "service_generate_git_commit_message",
            request,
            |worker, result| {
                worker.emit(BackendEvent::GitBusy(None));
                match result {
                    Ok(value) => worker.emit(BackendEvent::CommitMessage {
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
                        worker.git_refresh();
                    }
                    Err(error) => worker.failed(error),
                }
            },
        );
    }
}
