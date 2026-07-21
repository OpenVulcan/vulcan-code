//! VulcanMemoryMesh integration primitives shared by WebUI runtime and future plugins.
//! WebUI 运行时与后续插件共享的 VulcanMemoryMesh 集成原语。

pub mod adapter;
pub mod client;
pub mod config;
pub mod protocol;
pub mod session_state;
pub mod transport;

/// Generated VMM gRPC client and protobuf bindings.
/// 生成的 VMM gRPC 客户端与 protobuf 绑定。
pub mod grpc {
    #![allow(missing_docs)]
    tonic::include_proto!("vmm.v1");
}

pub use adapter::{
    VMM_PRECHECK_MEMORY_RENDERER_ID, VmmAdapterError, VmmCompletedTurnProjection,
    VmmHookTimelineItem, chat_compact_request_from_payload, injection_from_precheck_response,
    post_action_request_from_completed_payload, precheck_request_from_payload,
    render_vmm_context_item_detail, render_vmm_context_item_model, render_vmm_context_item_panel,
};
pub use client::{VmmClient, VmmClientError};
pub use config::{
    DEFAULT_GRPC_KEEPALIVE_PERMIT_WITHOUT_CALLS, DEFAULT_GRPC_KEEPALIVE_TIME_MS,
    DEFAULT_GRPC_KEEPALIVE_TIMEOUT_MS, DEFAULT_HANDSHAKE_TIMEOUT_MS, DEFAULT_IMPLICIT_MEMORY_TURNS,
    DEFAULT_PROFILE_NODE_LIMIT, DEFAULT_PROFILE_REFRESH_TURNS, DEFAULT_RECEIVE_TIMEOUT_MS,
    DEFAULT_VMM_INJECTION_DESCRIPTION, DEFAULT_VMM_INJECTION_TAG, MAX_PROFILE_NODE_LIMIT,
    VMM_OPENAI_APPEND_CONTINUATION_FEATURE_ENV, VMM_SESSION_STATE_FEATURE_ENV,
    VMM_STABLE_ANCHORS_FEATURE_ENV, VmmConfigDiagnostic, VmmConfigLayer, VmmEffectiveConfig,
    VmmEffectiveConfigSource, VmmInstallReservation, VmmRuntimeFeaturePolicy,
};
pub use protocol::{
    ApplyProfileInstructionRequest, ApplyProfileInstructionResponse, ChatCompactRequest,
    ChatCompactResponse, ContextItem, DeleteMemoriesRequest, DeleteMemoriesResponse,
    DeleteProjectRequest, DeleteProjectResponse, DeleteUserRequest, DeleteUserResponse,
    EnsureProjectRequest, EnsureProjectResponse, GetProfileBundleRequest, GetProfileBundleResponse,
    GetProfileNodesRequest, GetProfileNodesResponse, GetTurnDetailsRequest, GetTurnDetailsResponse,
    HealthzResponse, ListProjectsResponse, ListUsersResponse, MemorySearchGroupResult,
    MemorySearchHit, MigrateProjectRequest, MigrateProjectResponse, PostActionRequest,
    PostActionResponse, PostActionTimelineItem, PostActionTimelineType, PreCheckRecallMode,
    PreCheckRequest, PreCheckResponse, ProfileBundleMode, ProfileNodeEntry, ProfileNodeSourceKind,
    ProfileTarget, ProjectEntry, ResolveProjectRequest, ResolveProjectResponse, ResolveUserRequest,
    ResolveUserResponse, RetiredProfileNodeEntry, SearchMemoryEventsRequest,
    SearchMemoryEventsResponse, TurnDetailEntry, UserEntry, WriteMemoriesRequest,
    WriteMemoriesResponse, WriteMemoryItem, WriteMemoryResultItem,
};
pub use session_state::{
    VMM_SESSION_MEMORY_MAX_BYTES, VMM_SESSION_MEMORY_SCHEMA_VERSION, VmmCacheEpoch,
    VmmCachePressure, VmmLeaseTransitionReport, VmmLegacyExtraction, VmmLegacyLeaseItem,
    VmmMemoryLease, VmmPendingPromptEvent, VmmPromptAnchor, VmmPromptAnchorKind, VmmRecallBatch,
    VmmSessionMemoryState, take_legacy_vmm_injection_items,
};
pub use transport::{TonicVmmClient, VmmTransportConfig};
