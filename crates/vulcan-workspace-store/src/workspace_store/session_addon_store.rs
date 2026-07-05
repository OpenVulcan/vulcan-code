//! File-backed helpers for session addon state documents.
//! 会话附属状态文档的文件持久化辅助。

use crate::model::SessionAddonState;
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::{BufReader, Write};
use std::path::{Path, PathBuf};
use uuid::Uuid;

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

/// Writes one JSON document through a temporary file and rename.
/// 通过临时文件与重命名写入一个 JSON 文档。
///
/// # Parameters
/// 参数
/// `path` is the target JSON document path.
/// `path` 是目标 JSON 文档路径。
/// `value` is the serializable payload to persist.
/// `value` 是要持久化的可序列化载荷。
///
/// # Returns
/// 返回值
/// `Ok(())` when the JSON file has been replaced atomically.
/// JSON 文件被原子替换成功时返回 `Ok(())`。
fn write_json_file_atomic<T>(path: &Path, value: &T) -> Result<(), String>
where
    T: Serialize,
{
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(to_store_error)?;
    }
    let temp_path = unique_json_temp_path(path);
    let mut file = File::create(&temp_path).map_err(to_store_error)?;
    serde_json::to_writer_pretty(&mut file, value).map_err(|error| error.to_string())?;
    file.write_all(b"\n").map_err(to_store_error)?;
    file.flush().map_err(to_store_error)?;
    fs::rename(&temp_path, path).map_err(|error| {
        let _ = fs::remove_file(&temp_path);
        to_store_error(error)
    })
}

/// Builds a unique temporary JSON path next to the target file.
/// 在目标文件旁构建唯一临时 JSON 路径。
///
/// # Parameters
/// 参数
/// `path` is the target path that will eventually be replaced.
/// `path` 是最终会被替换的目标路径。
///
/// # Returns
/// 返回值
/// A unique sibling temporary file path.
/// 唯一的同级临时文件路径。
fn unique_json_temp_path(path: &Path) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("addons.json");
    path.with_file_name(format!(".{file_name}.{}.tmp", Uuid::new_v4()))
}

/// Converts a filesystem error into the workspace-store error text convention.
/// 将文件系统错误转换为 workspace-store 的错误文本约定。
///
/// # Parameters
/// 参数
/// `error` is the source filesystem error.
/// `error` 是源文件系统错误。
///
/// # Returns
/// 返回值
/// A human-readable store error string.
/// 可读的存储错误字符串。
fn to_store_error(error: std::io::Error) -> String {
    error.to_string()
}
