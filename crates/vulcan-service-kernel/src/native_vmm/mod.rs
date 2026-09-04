//! Native in-process VMM composition and host adapters.
//! 原生进程内 VMM 组合与宿主适配器。

mod client;
mod driver;
mod llm_contract;
mod maintenance;
mod prompt;
mod resources;

pub(crate) use client::NativeVmmClient;
pub(crate) use driver::{NativeVmmCoreRuntime, NativeVmmHostAiStatus};
pub(crate) use maintenance::{
    NativeVmmClearPreview, NativeVmmClearResult, NativeVmmMaintenance, NativeVmmMemoryLevelCounts,
    NativeVmmMemoryRecords, NativeVmmMigrationPreview, NativeVmmMigrationRecordSummary,
    NativeVmmMigrationResult, NativeVmmProfileRecordSummary, NativeVmmProfileRecords,
    NativeVmmProfileTarget, NativeVmmProjectCatalog, NativeVmmProjectContentCounts,
    NativeVmmProjectDeletePreview, NativeVmmRecycleBatchSummary, NativeVmmRecycleBin,
    NativeVmmRetentionApplyResult, NativeVmmRetentionStatus, NativeVmmSessionRename,
    NativeVmmTurnStatusCounts, NativeVmmVectorRebuildPreview, NativeVmmVectorRebuildResult,
    NativeVmmVectorStatus,
};
