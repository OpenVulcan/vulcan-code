//! Clear markers of the session activity panel: ended subagents and finished commands before a marker are
//! hidden by default. Nothing else is deleted; the sessions, job logs and indexes stay as they are.
//! 会话活动面板的清理标记：标记时间之前已结束的子代理与已结束的命令默认隐藏。不删除其他任何内容；会话、
//! 任务日志与索引保持原样。

use super::*;
use rusqlite::OptionalExtension;

/// Clear markers of one session, as Unix milliseconds of the latest clear of each list.
/// 一个会话的清理标记，即每个列表最近一次清理的 Unix 毫秒时间。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SessionActivityMarks {
    /// Latest clear of the subagent list.
    /// 子代理列表最近一次清理的时间。
    pub subagents_cleared_at_ms: Option<u64>,
    /// Latest clear of the command list.
    /// 命令列表最近一次清理的时间。
    pub commands_cleared_at_ms: Option<u64>,
}

/// List of the activity panel a marker belongs to.
/// 标记所属的活动面板列表。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionActivityMarkScope {
    /// Ended subagents.
    /// 已结束的子代理。
    Subagents,
    /// Finished commands.
    /// 已结束的命令。
    Commands,
}

/// Reads the clear markers of one session; a session never cleared has none.
/// 读取一个会话的清理标记；从未清理过的会话没有标记。
pub async fn load_session_activity_marks(
    host_runtime: &HostRuntimeHandle,
    session_id: &str,
) -> Result<SessionActivityMarks, String> {
    let session_id = session_id.to_string();
    local_sql::with_connection_async(
        &host_runtime.local,
        LogicalDatabase::AppState,
        move |connection| {
            connection
                .query_row(
                    "SELECT subagents_cleared_at_ms, commands_cleared_at_ms
                     FROM session_activity_marks_v1 WHERE session_id = ?1",
                    [&session_id],
                    |row| {
                        Ok(SessionActivityMarks {
                            subagents_cleared_at_ms: row
                                .get::<_, Option<i64>>(0)?
                                .map(|value| value.max(0) as u64),
                            commands_cleared_at_ms: row
                                .get::<_, Option<i64>>(1)?
                                .map(|value| value.max(0) as u64),
                        })
                    },
                )
                .optional()
                .map(Option::unwrap_or_default)
                .map_err(|error| format!("Failed to read the session activity markers: {error}"))
        },
    )
    .await
}

/// Records that the user cleared one list of a session at `cleared_at_ms`.
/// 记录用户在 `cleared_at_ms` 清理了某会话的一个列表。
pub async fn mark_session_activity_cleared(
    host_runtime: &HostRuntimeHandle,
    session_id: &str,
    scope: SessionActivityMarkScope,
    cleared_at_ms: u64,
) -> Result<SessionActivityMarks, String> {
    let sql = match scope {
        SessionActivityMarkScope::Subagents => {
            "INSERT INTO session_activity_marks_v1 (session_id, subagents_cleared_at_ms)
             VALUES (?1, ?2)
             ON CONFLICT(session_id) DO UPDATE SET subagents_cleared_at_ms = excluded.subagents_cleared_at_ms"
        }
        SessionActivityMarkScope::Commands => {
            "INSERT INTO session_activity_marks_v1 (session_id, commands_cleared_at_ms)
             VALUES (?1, ?2)
             ON CONFLICT(session_id) DO UPDATE SET commands_cleared_at_ms = excluded.commands_cleared_at_ms"
        }
    };
    let owned_session_id = session_id.to_string();
    let cleared_at = i64::try_from(cleared_at_ms)
        .map_err(|_| String::from("The clear time is out of range."))?;
    local_sql::with_connection_async(
        &host_runtime.local,
        LogicalDatabase::AppState,
        move |connection| {
            connection
                .execute(sql, rusqlite::params![owned_session_id, cleared_at])
                .map(|_| ())
                .map_err(|error| format!("Failed to save the session activity marker: {error}"))
        },
    )
    .await?;
    load_session_activity_marks(host_runtime, session_id).await
}
