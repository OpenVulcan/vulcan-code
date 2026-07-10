//! File-backed helpers for session addon state documents.
//! 会话附属状态文档的文件持久化辅助。

use crate::model::SessionAddonState;
use serde::Deserialize;
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};

use super::segmented_session_store::write_json_file_atomic;

/// Durable filename used for session addon state beside segmented transcript files.
/// 分段转录文件旁用于保存会话附属状态的持久化文件名。
pub(super) const SESSION_ADDON_STATE_FILE: &str = "addons.json";

/// Returns the addon document path for one segmented session directory.
/// 返回一个分段会话目录对应的附属状态文档路径。
///
/// # Parameters
/// 参数
/// `session_dir` is the resolved segmented session directory.
/// `session_dir` 是已解析的分段会话目录。
///
/// # Returns
/// 返回值
/// The absolute or relative addon document path under the session directory.
/// 会话目录下的绝对或相对附属状态文档路径。
pub(super) fn session_addon_state_path(session_dir: &Path) -> PathBuf {
    session_dir.join(SESSION_ADDON_STATE_FILE)
}

/// Reads one session addon state document when it already exists.
/// 在文档已存在时读取一个会话附属状态文档。
///
/// # Parameters
/// 参数
/// `session_dir` is the resolved segmented session directory.
/// `session_dir` 是已解析的分段会话目录。
///
/// # Returns
/// 返回值
/// The addon state when the file exists, or `None` when the session has no addon document yet.
/// 文件存在时返回附属状态；会话尚无附属文档时返回 `None`。
pub(super) fn read_session_addon_state(
    session_dir: &Path,
) -> Result<Option<SessionAddonState>, String> {
    let path = session_addon_state_path(session_dir);
    if !path.exists() {
        return Ok(None);
    }
    read_required_json(&path).map(Some)
}

/// Writes one session addon state document atomically without touching transcript files.
/// 原子写入一个会话附属状态文档，且不触碰转录文件。
///
/// # Parameters
/// 参数
/// `session_dir` is the resolved segmented session directory.
/// `session_dir` 是已解析的分段会话目录。
/// `state` is the complete addon document to store.
/// `state` 是要保存的完整附属状态文档。
///
/// # Returns
/// 返回值
/// `Ok(())` when the addon document has been written.
/// 附属状态文档写入成功时返回 `Ok(())`。
pub(super) fn write_session_addon_state(
    session_dir: &Path,
    state: &SessionAddonState,
) -> Result<(), String> {
    write_json_file_atomic(&session_addon_state_path(session_dir), state)
}

/// Reads a required JSON document from disk.
/// 从磁盘读取一个必需的 JSON 文档。
///
/// # Parameters
/// 参数
/// `path` is the JSON document path.
/// `path` 是 JSON 文档路径。
///
/// # Returns
/// 返回值
/// The decoded JSON document.
/// 解码后的 JSON 文档。
fn read_required_json<T>(path: &Path) -> Result<T, String>
where
    T: for<'de> Deserialize<'de>,
{
    let file = File::open(path)
        .map_err(|error| format!("打开 JSON 文件失败 `{}`：{error}", path.display()))?;
    let reader = BufReader::new(file);
    serde_json::from_reader(reader)
        .map_err(|error| format!("解析 JSON 文件失败 `{}`：{error}", path.display()))
}
