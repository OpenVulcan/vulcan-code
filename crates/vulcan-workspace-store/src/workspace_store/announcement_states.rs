//! Local state of OpenVulcan announcements: which ones the user has seen in the message center and
//! which banners they closed. The announcements themselves stay on the user center; only these marks
//! are stored, per region, and marks of announcements that left the region's document are pruned.
//! OpenVulcan 公告的本地状态：用户在消息中心看过哪些，以及关闭了哪些横幅。公告本身保留在用户中心；
//! 这里只按区域保存这些标记，并清理已离开该区域文档的公告标记。

use super::*;

/// Local marks of one announcement, as Unix milliseconds.
/// 一条公告的本地标记，单位为 Unix 毫秒。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AnnouncementState {
    /// When the user first saw it in the message center.
    /// 用户首次在消息中心看到它的时间。
    pub seen_at_ms: Option<u64>,
    /// When the user closed its banner.
    /// 用户关闭其横幅的时间。
    pub dismissed_at_ms: Option<u64>,
}

/// Which mark one update sets.
/// 一次更新设置的标记。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnnouncementMark {
    /// Seen in the message center.
    /// 已在消息中心看到。
    Seen,
    /// Banner closed.
    /// 横幅已关闭。
    Dismissed,
}

/// Reads the marks of every announcement of `region` that has any.
/// 读取 `region` 中所有带标记公告的标记。
pub async fn load_announcement_states(
    host_runtime: &HostRuntimeHandle,
    region: &str,
) -> Result<BTreeMap<String, AnnouncementState>, String> {
    let region = region.to_string();
    local_sql::with_connection_async(
        &host_runtime.local,
        LogicalDatabase::AppState,
        move |connection| {
            let mut statement = connection
                .prepare(
                    "SELECT announcement_id, seen_at_ms, dismissed_at_ms
                     FROM announcement_states_v1 WHERE region = ?1",
                )
                .map_err(|error| format!("Failed to read the announcement marks: {error}"))?;
            let rows = statement
                .query_map([&region], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        AnnouncementState {
                            seen_at_ms: row
                                .get::<_, Option<i64>>(1)?
                                .map(|value| value.max(0) as u64),
                            dismissed_at_ms: row
                                .get::<_, Option<i64>>(2)?
                                .map(|value| value.max(0) as u64),
                        },
                    ))
                })
                .map_err(|error| format!("Failed to read the announcement marks: {error}"))?;
            rows.collect::<Result<BTreeMap<_, _>, _>>()
                .map_err(|error| format!("Failed to read the announcement marks: {error}"))
        },
    )
    .await
}

/// Sets one mark on each of `announcement_ids`, keeping an earlier time when one is already set.
/// 为 `announcement_ids` 中每一项设置一个标记；已设置时保留较早的时间。
pub async fn mark_announcements(
    host_runtime: &HostRuntimeHandle,
    region: &str,
    announcement_ids: &[String],
    mark: AnnouncementMark,
    at_ms: u64,
) -> Result<(), String> {
    let sql = match mark {
        AnnouncementMark::Seen => {
            "INSERT INTO announcement_states_v1 (region, announcement_id, seen_at_ms)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(region, announcement_id)
             DO UPDATE SET seen_at_ms = COALESCE(seen_at_ms, excluded.seen_at_ms)"
        }
        AnnouncementMark::Dismissed => {
            "INSERT INTO announcement_states_v1 (region, announcement_id, dismissed_at_ms)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(region, announcement_id)
             DO UPDATE SET dismissed_at_ms = COALESCE(dismissed_at_ms, excluded.dismissed_at_ms)"
        }
    };
    let region = region.to_string();
    let ids = announcement_ids.to_vec();
    let at = i64::try_from(at_ms).map_err(|_| String::from("The mark time is out of range."))?;
    local_sql::with_connection_async(
        &host_runtime.local,
        LogicalDatabase::AppState,
        move |connection| {
            let transaction = connection
                .transaction()
                .map_err(|error| format!("Failed to save the announcement marks: {error}"))?;
            for id in &ids {
                transaction
                    .execute(sql, rusqlite::params![region, id, at])
                    .map_err(|error| format!("Failed to save the announcement marks: {error}"))?;
            }
            transaction
                .commit()
                .map_err(|error| format!("Failed to save the announcement marks: {error}"))
        },
    )
    .await
}

/// Deletes the marks of `region` whose announcement is no longer in `current_ids`.
/// 删除 `region` 中公告已不在 `current_ids` 内的标记。
pub async fn prune_announcement_states(
    host_runtime: &HostRuntimeHandle,
    region: &str,
    current_ids: &[String],
) -> Result<(), String> {
    let region = region.to_string();
    let keep: std::collections::BTreeSet<String> = current_ids.iter().cloned().collect();
    local_sql::with_connection_async(
        &host_runtime.local,
        LogicalDatabase::AppState,
        move |connection| {
            let stored: Vec<String> = {
                let mut statement = connection
                    .prepare("SELECT announcement_id FROM announcement_states_v1 WHERE region = ?1")
                    .map_err(|error| format!("Failed to prune the announcement marks: {error}"))?;
                let rows = statement
                    .query_map([&region], |row| row.get::<_, String>(0))
                    .map_err(|error| format!("Failed to prune the announcement marks: {error}"))?;
                rows.collect::<Result<Vec<_>, _>>()
                    .map_err(|error| format!("Failed to prune the announcement marks: {error}"))?
            };
            let transaction = connection
                .transaction()
                .map_err(|error| format!("Failed to prune the announcement marks: {error}"))?;
            for id in stored.iter().filter(|id| !keep.contains(*id)) {
                transaction
                    .execute(
                        "DELETE FROM announcement_states_v1 WHERE region = ?1 AND announcement_id = ?2",
                        rusqlite::params![region, id],
                    )
                    .map_err(|error| format!("Failed to prune the announcement marks: {error}"))?;
            }
            transaction
                .commit()
                .map_err(|error| format!("Failed to prune the announcement marks: {error}"))
        },
    )
    .await
}
