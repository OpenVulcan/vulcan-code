//! Shared WebUI Git diff text parsing helpers.
//! WebUI Git diff 文本解析共享辅助逻辑。

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::Path,
};

use super::command::{
    format_webui_git_error, run_webui_git_command, run_webui_git_command_with_stdin,
};
use super::diff::{ensure_webui_git_ref_text, is_path_within_root};
use super::model::{WebuiWorkspaceDiffFileSummary, WebuiWorkspaceDiffMode};

/// Maximum per-file changed lines transmitted to the WebUI diff renderer.
/// 允许传输给 WebUI diff 渲染器的单文件最大变更行数。
const WEBUI_DIFF_FILE_TRANSMIT_CHANGED_LINES_LIMIT: usize = 20_000;

/// Maximum single changed-line bytes transmitted to the WebUI diff renderer.
/// 允许传输给 WebUI diff 渲染器的单条变更行最大字节数。
const WEBUI_DIFF_FILE_TRANSMIT_MAX_CHANGED_LINE_BYTES_LIMIT: usize = 1024 * 1024;

mod hunk_scoping;
mod large_filter;
mod line_counts;
mod pathspec;
mod stats;
mod summary_parser;

pub(crate) use hunk_scoping::*;
pub(crate) use large_filter::*;
pub(crate) use line_counts::*;
pub(crate) use pathspec::*;
pub(crate) use stats::*;
pub(crate) use summary_parser::*;
