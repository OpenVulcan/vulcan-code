//! Single-file persistence and one-time legacy reconstruction for session lifetime usage.
//! 会话生命周期用量的单文件持久化与一次性旧数据补建。

use super::session_v1::documents::write_json_document;
use super::{SessionV1StorageLocator, load_session_v1_full, locate_embedded_task_session_v1};
use std::io::ErrorKind;
use vulcan_session_core::v1::lifetime_usage::{
    ChildSessionUsage, SessionLifetimeUsage, SessionUsageBucket,
};
use vulcan_session_core::v1::{EventBody, SessionDocument};

/// Saves one root-owned aggregate using the existing atomic file publisher.
/// 使用既有原子文件发布器保存一份根会话累计。
/// The caller serializes writes; returns storage errors without altering other session files.
/// 调用方串行写入；返回存储错误，不修改其他会话文件。
pub fn save_session_lifetime_usage(
    locator: &SessionV1StorageLocator,
    usage: &SessionLifetimeUsage,
) -> Result<(), String> {
    if usage.root_session_id != locator.session_id() || usage.version != 1 {
        return Err("Session usage identity or version mismatch".to_owned());
    }
    write_json_document(
        &locator.session_directory().join("usage.json"),
        usage,
        "session usage",
    )
    .map_err(|error| error.to_string())
}

/// Loads the sole root file, or reconstructs it once from persisted own-session API facts.
/// 加载唯一根文件，缺失时从各会话自身持久化 API 事实一次性补建。
/// Only missing files trigger reconstruction; malformed existing JSON remains an error.
/// 仅文件缺失触发补建；既有 JSON 损坏保持明确错误。
pub fn load_session_lifetime_usage(
    locator: &SessionV1StorageLocator,
) -> Result<SessionLifetimeUsage, String> {
    load_session_lifetime_usage_with_goal_baseline(locator).map(|(usage, _)| usage)
}

/// Loads counters and returns a provable legacy Goal baseline only during the first backfill.
/// 加载计数，仅在首次补建时返回可证明的旧 Goal 基线。
/// Child history without a shared sequence boundary remains unknown rather than time-estimated.
/// 没有共同序列边界的子历史保持未知，不按时间估算。
pub fn load_session_lifetime_usage_with_goal_baseline(
    locator: &SessionV1StorageLocator,
) -> Result<
    (
        SessionLifetimeUsage,
        Option<vulcan_session_core::v1::lifetime_usage::GoalUsageSnapshot>,
    ),
    String,
> {
    // Existing aggregates are never replaced by a scan of the current transcript generation.
    // 既有累计绝不被当前转录代际扫描结果覆盖。
    let path = locator.session_directory().join("usage.json");
    match std::fs::read(&path) {
        Ok(bytes) => {
            let usage: SessionLifetimeUsage =
                serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
            if usage.version != 1 || usage.root_session_id != locator.session_id() {
                return Err("Session usage identity or version mismatch".to_owned());
            }
            return Ok((usage, None));
        }
        Err(error) if error.kind() == ErrorKind::NotFound => {}
        Err(error) => return Err(format!("Read {}: {error}", path.display())),
    }
    let root = load_session_v1_full(locator).map_err(|error| error.to_string())?;
    let mut usage =
        SessionLifetimeUsage::new(locator.session_id().to_owned(), root.metadata.updated_at_ms);
    let (main, incomplete) = own_usage(&root.document)?;
    usage.main = main;
    usage.legacy_incomplete = incomplete;
    // Returned identities reveal missing child directories without importing their numeric totals.
    // 回流身份揭示缺失的子目录，但不导入其中的数字总量。
    let mut referenced_children = referenced_child_sessions(&root.document);
    let children = locator.session_directory().join("TASK");
    let entries = match std::fs::read_dir(&children) {
        Ok(entries) => Some(entries),
        Err(error) if error.kind() == ErrorKind::NotFound => None,
        Err(error) => return Err(format!("Read {}: {error}", children.display())),
    };
    if let Some(entries) = entries {
        for entry in entries {
            let entry = entry.map_err(|error| error.to_string())?;
            if !entry
                .file_type()
                .map_err(|error| error.to_string())?
                .is_dir()
            {
                continue;
            }
            let name = entry.file_name();
            let Some(id) = name.to_str().and_then(|name| name.strip_prefix("task-")) else {
                continue;
            };
            let child =
                locate_embedded_task_session_v1(locator, id).map_err(|error| error.to_string())?;
            let loaded = match load_session_v1_full(&child) {
                Ok(loaded) if loaded.metadata.session_id == id => loaded,
                Ok(_) | Err(_) => {
                    usage.legacy_incomplete = true;
                    continue;
                }
            };
            let (bucket, incomplete) = own_usage(&loaded.document)?;
            referenced_children.extend(referenced_child_sessions(&loaded.document));
            usage.children_total.merge(&bucket)?;
            usage.children_by_session_id.insert(
                id.to_owned(),
                ChildSessionUsage {
                    parent_session_id: None,
                    usage: bucket,
                },
            );
            usage.legacy_incomplete |= incomplete;
        }
    }
    usage.legacy_incomplete |= referenced_children
        .iter()
        .any(|id| !usage.children_by_session_id.contains_key(id));
    save_session_lifetime_usage(locator, &usage)?;
    // A child-free exact prefix can prove its baseline without guessing cross-session chronology.
    // 无子会话的精确前缀可以证明基线，无需猜测跨会话时序。
    let baseline = if usage.children_by_session_id.is_empty()
        && !usage.legacy_incomplete
        && root.metadata.mode.goal.enabled
        && root.metadata.mode.goal.usage_baseline.is_none()
    {
        if let Some(origin) = root.metadata.mode.goal.origin_seq {
            let (main, incomplete) = own_usage_before(&root.document, Some(origin))?;
            (!incomplete).then_some(vulcan_session_core::v1::lifetime_usage::GoalUsageSnapshot {
                main,
                children: Default::default(),
            })
        } else {
            None
        }
    } else {
        None
    };
    Ok((usage, baseline))
}

/// Reads only the supplied document's local API and tool facts; returned child totals are excluded.
/// 仅读取给定文档的本地 API 与工具事实；排除回流子代理总量。
/// Returns fixed-size counters and whether unmatched historical requests remain.
/// 返回固定大小计数及是否存在无法配对的历史请求。
fn own_usage(document: &SessionDocument) -> Result<(SessionUsageBucket, bool), String> {
    own_usage_before(document, None)
}

/// Returns historical child identities within this session's own generation for coverage checks.
/// 返回当前会话自身代际内的历史子身份，用于覆盖检查。
/// Fork-inherited references belong to the source root and are excluded.
/// 分叉继承的引用属于来源根，因此排除。
fn referenced_child_sessions(document: &SessionDocument) -> std::collections::BTreeSet<String> {
    let inherited_until = document
        .metadata
        .fork_origin
        .as_ref()
        .map(|origin| origin.source_target_seq);
    document
        .events()
        .iter()
        .filter(|event| inherited_until.is_none_or(|seq| event.seq > seq))
        .filter_map(|event| match &event.event {
            EventBody::SubagentReturn(returned) => Some(returned.child_session_id.clone()),
            _ => None,
        })
        .collect()
}

/// Aggregates an exact local prefix; unmatched requests prevent a known Goal baseline.
/// 汇总精确本地前缀；无法配对的请求阻止基线被标为已知。
fn own_usage_before(
    document: &SessionDocument,
    before: Option<vulcan_session_core::v1::ids::Seq>,
) -> Result<(SessionUsageBucket, bool), String> {
    let mut bucket = SessionUsageBucket::default();
    let mut started = 0u64;
    // A fork's retained source prefix belongs to its source session, not the newly created root.
    // 分叉保留的来源前缀属于源会话，不属于新建根会话。
    let inherited_until = document
        .metadata
        .fork_origin
        .as_ref()
        .map(|origin| origin.source_target_seq);
    for event in document.events().iter().filter(|event| {
        inherited_until.is_none_or(|seq| event.seq > seq)
            && before.is_none_or(|seq| event.seq < seq)
    }) {
        match &event.event {
            EventBody::ApiStats(api) => {
                bucket.merge(&SessionUsageBucket::from_api(&api.usage, &api.timing)?)?
            }
            EventBody::ApiStarted(_) => {
                started = started
                    .checked_add(1)
                    .ok_or("Session request count overflow")?
            }
            EventBody::ToolCall(_) => {
                bucket.tool_call_count = bucket
                    .tool_call_count
                    .checked_add(1)
                    .ok_or("Session tool count overflow")?
            }
            _ => {}
        }
    }
    let incomplete = started != bucket.request_count;
    Ok((bucket, incomplete))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace_store::session_v1_store::create_session_v1_storage_fixture;

    /// A missing file is created once; subsequent loads do not depend on session event files.
    /// 缺失文件只补建一次；后续加载不依赖会话事件文件。
    #[test]
    fn usage_backfill_is_saved_and_existing_totals_are_preserved() {
        let temporary = tempfile::tempdir().unwrap();
        let locator = create_session_v1_storage_fixture(temporary.path(), "usage-root", 1).unwrap();
        let mut usage = load_session_lifetime_usage(&locator).unwrap();
        assert!(locator.session_directory().join("usage.json").exists());
        usage.main.tool_call_count = 12;
        save_session_lifetime_usage(&locator, &usage).unwrap();
        assert_eq!(load_session_lifetime_usage(&locator).unwrap(), usage);
    }

    /// Malformed or mismatched files remain explicit errors instead of silently rebuilding.
    /// 损坏或身份不符的文件保持明确错误，不静默重建。
    #[test]
    fn existing_invalid_usage_never_triggers_backfill() {
        let temporary = tempfile::tempdir().unwrap();
        let locator = create_session_v1_storage_fixture(temporary.path(), "usage-root", 1).unwrap();
        let path = locator.session_directory().join("usage.json");
        std::fs::write(&path, "invalid").unwrap();
        assert!(load_session_lifetime_usage(&locator).is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "invalid");
        let wrong = SessionLifetimeUsage::new("another-root".into(), 0);
        std::fs::write(&path, serde_json::to_vec(&wrong).unwrap()).unwrap();
        assert!(load_session_lifetime_usage(&locator).is_err());
    }
}
