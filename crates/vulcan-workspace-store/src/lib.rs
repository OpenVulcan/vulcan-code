//! Shared in-process workspace storage and bootstrap surface reused by the WebUI shell.
//! 供 WebUI 壳层复用的共享进程内工作区存储与引导表面。

pub mod data_plane;
pub mod knowledge_base;
pub mod local_database_runtime;
pub mod local_sql;
pub mod luaskills_sql_params;
pub mod provider_usage;
pub mod vmm_data_plane;
pub mod workspace_store;

pub(crate) mod conversation_protocol {
    //! Thin crate-local alias for canonical conversation protocol types and rebuild helpers.
    //! 规范会话协议类型与重建辅助函数的 crate 内薄别名。

    pub use vulcan_context::{SessionConversationPayload, SessionConversationState};

    #[cfg(test)]
    pub use vulcan_context::SessionConversationMessage;
}

pub(crate) use vulcan_runtime_assets as runtime_assets;
pub(crate) use vulcan_workbench_model as model;
