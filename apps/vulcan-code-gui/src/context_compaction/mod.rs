//! Thin GUI adapter for shared context-compaction planning helpers.
//! 面向图形界面的共享上下文压缩规划辅助薄适配层。

use crate::skills::SessionDerivedRecord;

pub use vulcan_context_compaction::{
    CompactionPlan, CompactionPlanInput as CompactionSkillInput,
    CompactionResult as CompactionSkillResult, CompactionSummaryRecord,
    extract_compaction_previous_summary,
};

/// Plans one context-compaction window from the current transcript and runtime anchor.
/// 根据当前转录历史与运行时锚点规划一份上下文压缩窗口。
pub fn plan_compaction(input: CompactionSkillInput<'_>) -> Option<CompactionPlan> {
    vulcan_context_compaction::plan_context_compaction(input)
}

/// Converts one shared compaction summary record into the GUI-owned hidden session-derived record shape.
/// 将一条共享压缩摘要记录转换为图形界面持有的隐藏会话派生记录形态。
pub fn into_session_derived_record(record: CompactionSummaryRecord) -> SessionDerivedRecord {
    SessionDerivedRecord {
        kind: record.kind,
        record_kind: record.record_kind,
        title: record.title,
        body: record.body,
        preview: record.preview,
    }
}
