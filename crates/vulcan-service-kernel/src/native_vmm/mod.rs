//! Native in-process VMM composition and host adapters.
//! 原生进程内 VMM 组合与宿主适配器。

mod client;
mod driver;
mod llm_contract;
mod maintenance;
mod prompt;

pub(crate) use client::NativeVmmClient;
pub(crate) use driver::NativeVmmCoreRuntime;
pub(crate) use maintenance::{
    NativeVmmClearPreview, NativeVmmClearResult, NativeVmmMaintenance, NativeVmmMigrationPreview,
    NativeVmmMigrationResult, NativeVmmProjectCatalog, NativeVmmProjectContentCounts,
    NativeVmmProjectDeletePreview, NativeVmmSessionRename,
};
