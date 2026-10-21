//! Compact conversation age labels, mirroring the WebUI `project-sidebar-time.ts`.
//! 紧凑的会话时长标签，镜像 WebUI 的 `project-sidebar-time.ts`。

use crate::model::ConversationSummary;
use crate::tr;

/// One minute in milliseconds, the smallest unit an age label counts.
/// 一分钟的毫秒数，即时长标签计数的最小单位。
pub const MINUTE_MS: i64 = 60_000;
const HOUR_MS: i64 = 60 * MINUTE_MS;
const DAY_MS: i64 = 24 * HOUR_MS;
const WEEK_MS: i64 = 7 * DAY_MS;
const MONTH_MS: i64 = 30 * DAY_MS;
const YEAR_MS: i64 = 365 * DAY_MS;

/// Returns the compact age of a conversation's last update, falling back to the persisted label without a timestamp.
/// 返回会话最后更新的紧凑时长；没有时间戳时回退为持久化标签。
pub fn conversation_age(conversation: &ConversationSummary, now_ms: i64) -> String {
    match conversation.last_updated_at_ms {
        Some(updated_at_ms) => compact_age(now_ms - updated_at_ms),
        None => compact_persisted_label(&conversation.last_updated_label),
    }
}

/// Converts one elapsed duration into the compact age label the WebUI sidebar shares.
/// 将经过时长转换为与 WebUI 侧栏共用的紧凑时长标签。
fn compact_age(elapsed_ms: i64) -> String {
    let elapsed = elapsed_ms.max(0);
    let count = |unit: i64| (elapsed / unit).max(1);
    if elapsed < MINUTE_MS {
        tr!("projectSidebar.time.justNow")
    } else if elapsed < HOUR_MS {
        tr!("projectSidebar.time.minutes", value = count(MINUTE_MS))
    } else if elapsed < DAY_MS {
        tr!("projectSidebar.time.hours", value = count(HOUR_MS))
    } else if elapsed < WEEK_MS {
        tr!("projectSidebar.time.days", value = count(DAY_MS))
    } else if elapsed < MONTH_MS {
        tr!("projectSidebar.time.weeks", value = count(WEEK_MS))
    } else if elapsed < YEAR_MS {
        tr!("projectSidebar.time.months", value = count(MONTH_MS))
    } else {
        tr!("projectSidebar.time.years", value = count(YEAR_MS))
    }
}

/// Compresses the backend relative label (`just now`, `5 min ago`, `2 mo ago`); other text is kept as is.
/// 压缩后端相对时间标签（`just now`、`5 min ago`、`2 mo ago`）；其他文本原样保留。
fn compact_persisted_label(label: &str) -> String {
    let trimmed = label.trim();
    if trimmed.eq_ignore_ascii_case("just now") {
        return tr!("projectSidebar.time.justNow");
    }
    let parsed = trimmed.strip_suffix(" ago").and_then(|rest| {
        let unit_at = rest.find(|c: char| !c.is_ascii_digit())?;
        let value = rest[..unit_at].parse::<u64>().ok()?;
        Some((value, rest[unit_at..].trim().to_ascii_lowercase()))
    });
    match parsed {
        Some((value, unit)) => match unit.as_str() {
            "min" => tr!("projectSidebar.time.minutes", value = value),
            "h" => tr!("projectSidebar.time.hours", value = value),
            "d" => tr!("projectSidebar.time.days", value = value),
            "w" => tr!("projectSidebar.time.weeks", value = value),
            "mo" => tr!("projectSidebar.time.months", value = value),
            _ => trimmed.to_string(),
        },
        None => trimmed.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::RunPhase;

    fn summary(label: &str, at: Option<i64>) -> ConversationSummary {
        ConversationSummary {
            id: "c".into(),
            title: "t".into(),
            project_id: None,
            run_phase: RunPhase::Idle,
            last_updated_label: label.into(),
            last_updated_at_ms: at,
            unread: false,
            attention: false,
            dedicated: false,
        }
    }

    /// The timestamp decides the unit, so the label ages with the clock instead of the snapshot.
    /// 由时间戳决定单位，标签随时钟而非快照变化。
    #[test]
    fn timestamp_picks_the_largest_whole_unit() {
        let now = 10 * YEAR_MS;
        let age = |elapsed: i64| conversation_age(&summary("5 min ago", Some(now - elapsed)), now);
        assert_eq!(age(30_000), tr!("projectSidebar.time.justNow"));
        assert_eq!(
            age(5 * MINUTE_MS),
            tr!("projectSidebar.time.minutes", value = 5)
        );
        assert_eq!(
            age(22 * HOUR_MS),
            tr!("projectSidebar.time.hours", value = 22)
        );
        assert_eq!(age(8 * DAY_MS), tr!("projectSidebar.time.weeks", value = 1));
        assert_eq!(
            age(2 * YEAR_MS),
            tr!("projectSidebar.time.years", value = 2)
        );
        assert_eq!(age(-MINUTE_MS), tr!("projectSidebar.time.justNow"));
    }

    /// Without a timestamp the backend label is compressed to the same templates, and unknown text passes through.
    /// 没有时间戳时后端标签压缩为相同模板，未知文本原样通过。
    #[test]
    fn persisted_label_falls_back_to_the_same_templates() {
        let age = |label: &str| conversation_age(&summary(label, None), 0);
        assert_eq!(age("just now"), tr!("projectSidebar.time.justNow"));
        assert_eq!(
            age("5 min ago"),
            tr!("projectSidebar.time.minutes", value = 5)
        );
        assert_eq!(
            age("2 mo ago"),
            tr!("projectSidebar.time.months", value = 2)
        );
        assert_eq!(age("Completed"), "Completed");
        assert_eq!(age(""), "");
    }
}
