//! Git workspace service modules.
//! Git 工作区服务模块。

mod actions;
mod command;
mod commit_action;
mod commit_summary;
mod detail;
mod diff;
mod diff_text;
mod history;
mod model;
mod pull_action;
mod repository;
mod search;
mod selection;
mod settings;
mod snapshot;

pub(super) use actions::run_webui_workspace_git_file_action;
#[cfg(test)]
pub(super) use command::run_webui_git_command;
pub(super) use commit_action::service_run_git_commit_action;
pub(super) use commit_summary::service_generate_git_commit_message;
pub(super) use detail::load_webui_workspace_diff_file_detail;
#[cfg(test)]
pub(super) use diff_text::{
    webui_diff_hunk_scoped_unified_diff, webui_unified_diff_file_summaries,
    webui_unified_diff_line_count, webui_unified_diff_stats,
};
pub(super) use history::{
    load_webui_git_history_page, load_webui_git_local_branches, switch_webui_git_branch,
};
#[cfg(test)]
pub(super) use model::WebuiWorkspaceDiffMode;
pub(super) use model::{
    WebuiGitHistoryPage, WebuiGitHistoryRequest, WebuiGitLocalBranchList,
    WebuiGitRepositoryTargetRequest, WebuiGitSwitchBranchRequest, WebuiGitSwitchBranchResult,
};
pub(super) use model::{WebuiGitRepositoryCatalog, WebuiGitRepositoryMonitoringRequest};
pub(super) use model::{
    WebuiWorkspaceDiffFileDetail, WebuiWorkspaceDiffFileDetailRequest, WebuiWorkspaceDiffRequest,
    WebuiWorkspaceDiffSearchRequest, WebuiWorkspaceDiffSearchResponse, WebuiWorkspaceDiffSnapshot,
    WebuiWorkspaceGitFileActionRequest, WebuiWorkspaceGitFileActionResult,
};
pub(super) use pull_action::service_run_git_pull_action;
pub(super) use repository::{
    discover_webui_git_repository_catalog,
    resolve_webui_repository_root_from_workspace_text_with_key,
};
pub(super) use search::load_webui_workspace_diff_search;
pub(super) use settings::{git_commit_settings, service_save_git_commit_settings};
pub(super) use snapshot::load_webui_workspace_diff_snapshot;
