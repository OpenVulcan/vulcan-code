//! Shared controller-backed workspace storage and bootstrap surface reused by the GUI shell.
//! 供图形界面壳层复用的共享控制器工作区存储与引导表面。

pub mod data_plane;
pub mod provider_usage;
pub mod workspace_store;

pub(crate) mod conversation_protocol {
    //! Thin crate-local alias for canonical conversation protocol types and rebuild helpers.
    //! 规范会话协议类型与重建辅助函数的 crate 内薄别名。

    pub use vulcan_context::{
        SessionConversationPayload, SessionConversationState, SessionProviderContinuationState,
        conversation_state_from_session_messages, conversation_state_to_session_messages,
    };

    #[cfg(test)]
    pub use vulcan_context::{
        SessionConversationMessage, SessionConversationRecord, append_assistant_text_message,
        append_user_text_message,
    };
}

pub(crate) use vulcan_provider_config::config as provider_config;
pub(crate) use vulcan_runtime_assets as runtime_assets;
pub(crate) use vulcan_workbench_model as model;

pub(crate) mod tooling {
    //! Minimal crate-local compatibility facade for reused `change_set` helpers.
    //! 复用 `change_set` 辅助逻辑使用的最小 crate 内兼容门面。

    pub mod change_set {
        //! Re-export of compact `change_set` persistence helpers.
        //! 紧凑 `change_set` 持久化辅助逻辑的转发导出。

        pub use vulcan_change_set::compact_change_set_host_result;
    }
}
