//! VMM client abstraction used by runtime adapters and tests.
//! 运行时适配器与测试使用的 VMM 客户端抽象。

use crate::protocol::{
    ChatCompactRequest, ChatCompactResponse, EnsureProjectRequest, EnsureProjectResponse,
    GetProfileBundleRequest, GetProfileBundleResponse, GetTurnDetailsRequest,
    GetTurnDetailsResponse, HealthzResponse, ListProjectsResponse, ListUsersResponse,
    PostActionRequest, PostActionResponse, PreCheckRequest, PreCheckResponse,
    ResolveProjectRequest, ResolveProjectResponse, ResolveUserRequest, ResolveUserResponse,
    SearchMemoryEventsRequest, SearchMemoryEventsResponse, WriteMemoriesRequest,
    WriteMemoriesResponse,
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::fmt::{self, Display};

/// Transport or business error returned by a VMM client implementation.
/// VMM 客户端实现返回的传输或业务错误。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmmClientError {
    /// Stable error kind used by diagnostics.
    /// 诊断使用的稳定错误类型。
    pub kind: String,
    /// Human-readable error message.
    /// 人类可读错误信息。
    pub message: String,
    /// Optional gRPC status code.
    /// 可选 gRPC 状态码。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub grpc_code: Option<i32>,
    /// Optional trace id when the remote service returned one.
    /// 远端服务返回时携带的可选追踪标识。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trace_id: Option<String>,
}

impl VmmClientError {
    /// Builds one client error from a stable kind and message.
    /// 根据稳定类型和消息构建一个客户端错误。
    pub fn new(kind: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            kind: kind.into(),
            message: message.into(),
            grpc_code: None,
            trace_id: None,
        }
    }
}

impl Display for VmmClientError {
    /// Formats the client error for hook diagnostics.
    /// 为 hook 诊断格式化客户端错误。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.grpc_code {
            Some(code) => write!(formatter, "{}({code}): {}", self.kind, self.message),
            None => write!(formatter, "{}: {}", self.kind, self.message),
        }
    }
}

impl std::error::Error for VmmClientError {}

/// Async VMM client abstraction.
/// 异步 VMM 客户端抽象。
#[async_trait]
pub trait VmmClient: Send + Sync {
    /// Calls VMM Healthz.
    /// 调用 VMM Healthz。
    async fn healthz(&self) -> Result<HealthzResponse, VmmClientError>;

    /// Calls VMM ListProjects.
    /// 调用 VMM ListProjects。
    async fn list_projects(&self) -> Result<ListProjectsResponse, VmmClientError>;

    /// Calls VMM ResolveProject.
    /// 调用 VMM ResolveProject。
    async fn resolve_project(
        &self,
        request: ResolveProjectRequest,
    ) -> Result<ResolveProjectResponse, VmmClientError>;

    /// Calls VMM EnsureProject.
    /// 调用 VMM EnsureProject。
    async fn ensure_project(
        &self,
        request: EnsureProjectRequest,
    ) -> Result<EnsureProjectResponse, VmmClientError>;

    /// Calls VMM ListUsers.
    /// 调用 VMM ListUsers。
    async fn list_users(&self) -> Result<ListUsersResponse, VmmClientError>;

    /// Calls VMM ResolveUser.
    /// 调用 VMM ResolveUser。
    async fn resolve_user(
        &self,
        request: ResolveUserRequest,
    ) -> Result<ResolveUserResponse, VmmClientError>;

    /// Calls VMM GetProfileBundle.
    /// 调用 VMM GetProfileBundle。
    async fn get_profile_bundle(
        &self,
        request: GetProfileBundleRequest,
    ) -> Result<GetProfileBundleResponse, VmmClientError>;

    /// Calls VMM PreCheck.
    /// 调用 VMM PreCheck。
    async fn pre_check(&self, request: PreCheckRequest)
    -> Result<PreCheckResponse, VmmClientError>;

    /// Calls VMM PostAction.
    /// 调用 VMM PostAction。
    async fn post_action(
        &self,
        request: PostActionRequest,
    ) -> Result<PostActionResponse, VmmClientError>;

    /// Calls VMM ChatCompact.
    /// 调用 VMM ChatCompact。
    async fn chat_compact(
        &self,
        request: ChatCompactRequest,
    ) -> Result<ChatCompactResponse, VmmClientError>;

    /// Calls VMM SearchMemoryEvents.
    /// 调用 VMM SearchMemoryEvents。
    async fn search_memory_events(
        &self,
        request: SearchMemoryEventsRequest,
    ) -> Result<SearchMemoryEventsResponse, VmmClientError>;

    /// Calls VMM GetTurnDetails.
    /// 调用 VMM GetTurnDetails。
    async fn get_turn_details(
        &self,
        request: GetTurnDetailsRequest,
    ) -> Result<GetTurnDetailsResponse, VmmClientError>;

    /// Calls VMM WriteMemories.
    /// 调用 VMM WriteMemories。
    async fn write_memories(
        &self,
        request: WriteMemoriesRequest,
    ) -> Result<WriteMemoriesResponse, VmmClientError>;
}
