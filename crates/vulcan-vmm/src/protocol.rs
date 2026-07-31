//! Typed VMM request and response models mirroring the current gRPC contract.
//! 与当前 gRPC 契约保持一致的 VMM 请求与响应类型。

/// VMM health, project, and user management protocol models.
/// VMM 健康检查、项目和用户管理协议模型。
mod management;
/// VMM memory and turn protocol models.
/// VMM 记忆与回合协议模型。
mod memory;
/// VMM profile protocol models.
/// VMM 画像协议模型。
mod profile;

pub use management::{
    DeleteProjectRequest, DeleteProjectResponse, DeleteUserRequest, DeleteUserResponse,
    EnsureProjectRequest, EnsureProjectResponse, HealthzResponse, ListProjectsResponse,
    ListUsersResponse, MigrateProjectRequest, MigrateProjectResponse, ProjectEntry,
    ResolveProjectRequest, ResolveProjectResponse, ResolveUserRequest, ResolveUserResponse,
    UserEntry,
};
pub use memory::{
    ChatCompactRequest, ChatCompactResponse, ContextItem, DeleteMemoriesRequest,
    DeleteMemoriesResponse, GetTurnDetailsRequest, GetTurnDetailsResponse, MemorySearchGroupResult,
    MemorySearchHit, PostActionRequest, PostActionResponse, PostActionTimelineItem,
    PostActionTimelineType, PreCheckRecallMode, PreCheckRequest, PreCheckResponse,
    SearchMemoryEventsRequest, SearchMemoryEventsResponse, SessionMemoryStatus, TurnDetailEntry,
    WriteMemoriesRequest, WriteMemoriesResponse, WriteMemoryItem, WriteMemoryResultItem,
};
pub use profile::{
    ApplyProfileInstructionRequest, ApplyProfileInstructionResponse, GetProfileBundleRequest,
    GetProfileBundleResponse, GetProfileNodesRequest, GetProfileNodesResponse, ProfileBundleMode,
    ProfileNodeEntry, ProfileNodeSourceKind, ProfileTarget, RetiredProfileNodeEntry,
};
