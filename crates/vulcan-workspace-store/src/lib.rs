//! Shared controller-backed workspace storage and bootstrap surface reused by the WebUI shell.
//! 供WebUI壳层复用的共享控制器工作区存储与引导表面。

pub mod data_plane;
pub mod knowledge_base;
pub mod provider_usage;
pub mod workspace_store;

pub(crate) mod conversation_protocol {
    //! Thin crate-local alias for canonical conversation protocol types and rebuild helpers.
    //! 规范会话协议类型与重建辅助函数的 crate 内薄别名。

    pub use vulcan_context::{
        SessionConversationPayload, SessionConversationState,
        conversation_state_from_session_messages, conversation_state_to_session_messages,
    };

    #[cfg(test)]
    pub use vulcan_context::{SessionConversationMessage, SessionProviderContinuationState};
}

pub(crate) use vulcan_provider_config::config as provider_config;
pub(crate) use vulcan_runtime_assets as runtime_assets;
pub(crate) use vulcan_workbench_model as model;
