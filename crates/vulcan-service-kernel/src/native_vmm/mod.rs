//! Native in-process VMM composition and host adapters.
//! 原生进程内 VMM 组合与宿主适配器。

mod analysis_queue;
mod client;
mod dialogue;
mod driver;
mod lexical;
mod llm_contract;
mod maintenance;
mod profile_bundle;
mod prompt;
mod resources;
mod retention_policy;
mod retrieval_origin;
mod turn_details;

pub(crate) use client::NativeVmmClient;
pub(crate) use driver::{NativeVmmCoreRuntime, NativeVmmHostAiStatus};
pub(crate) use maintenance::{
    NativeVmmClearPreview, NativeVmmClearResult, NativeVmmMaintenance, NativeVmmMemoryLevelCounts,
    NativeVmmMemoryRecords, NativeVmmMemorySessionSummary, NativeVmmMigrationPreview,
    NativeVmmMigrationRecordSummary, NativeVmmMigrationResult, NativeVmmProfileRecordSummary,
    NativeVmmProfileRecords, NativeVmmProfileTarget, NativeVmmProjectCatalog,
    NativeVmmProjectContentCounts, NativeVmmProjectDeletePreview, NativeVmmRecallPublication,
    NativeVmmRecycleBatchSummary, NativeVmmRecycleBin, NativeVmmRetentionApplyResult,
    NativeVmmRetentionStatus, NativeVmmSessionRename, NativeVmmTurnStatusCounts,
    NativeVmmVectorRebuildPreview, NativeVmmVectorRebuildResult, NativeVmmVectorStatus,
};
