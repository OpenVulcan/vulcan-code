//! Tonic-based VMM gRPC client implementation.
//! 基于 Tonic 的 VMM gRPC 客户端实现。

mod responses;

use crate::client::{VmmClient, VmmClientError};
use crate::config::VmmEffectiveConfig;
use crate::grpc::vmm_service_client::VmmServiceClient;
use crate::grpc::{
    ApplyProfileInstructionRequest as GrpcApplyProfileInstructionRequest,
    ChatCompactRequest as GrpcChatCompactRequest,
    DeleteMemoriesRequest as GrpcDeleteMemoriesRequest,
    DeleteProjectRequest as GrpcDeleteProjectRequest, DeleteUserRequest as GrpcDeleteUserRequest,
    EnsureProjectRequest as GrpcEnsureProjectRequest,
    GetProfileBundleRequest as GrpcGetProfileBundleRequest,
    GetProfileNodesRequest as GrpcGetProfileNodesRequest,
    GetTurnDetailsRequest as GrpcGetTurnDetailsRequest,
    MigrateProjectRequest as GrpcMigrateProjectRequest, PostActionRequest as GrpcPostActionRequest,
    PostActionTimelineItem as GrpcPostActionTimelineItem,
    PreCheckRecallMode as GrpcPreCheckRecallMode, PreCheckRequest as GrpcPreCheckRequest,
    ProfileBundleMode as GrpcProfileBundleMode, ProfileTarget as GrpcProfileTarget,
    ResolveProjectRequest as GrpcResolveProjectRequest,
    ResolveUserRequest as GrpcResolveUserRequest,
    SearchMemoryEventsRequest as GrpcSearchMemoryEventsRequest,
    WriteMemoriesRequest as GrpcWriteMemoriesRequest, WriteMemoryItem as GrpcWriteMemoryItem,
};
use crate::protocol::{
    ApplyProfileInstructionRequest, ApplyProfileInstructionResponse, ChatCompactRequest,
    ChatCompactResponse, DeleteMemoriesRequest, DeleteMemoriesResponse, DeleteProjectRequest,
    DeleteProjectResponse, DeleteUserRequest, DeleteUserResponse, EnsureProjectRequest,
    EnsureProjectResponse, GetProfileBundleRequest, GetProfileBundleResponse,
    GetProfileNodesRequest, GetProfileNodesResponse, GetTurnDetailsRequest, GetTurnDetailsResponse,
    HealthzResponse, ListProjectsResponse, ListUsersResponse, MigrateProjectRequest,
    MigrateProjectResponse, PostActionRequest, PostActionResponse, PreCheckRecallMode,
    PreCheckRequest, PreCheckResponse, ProfileBundleMode, ProfileTarget, ResolveProjectRequest,
    ResolveProjectResponse, ResolveUserRequest, ResolveUserResponse, SearchMemoryEventsRequest,
    SearchMemoryEventsResponse, WriteMemoriesRequest, WriteMemoriesResponse, WriteMemoryItem,
};
use async_trait::async_trait;
use std::time::Duration;
use tonic::metadata::MetadataValue;
use tonic::transport::{Channel, Endpoint};
use tonic::{Code, Request, Status};

use responses::*;

/// Runtime transport options for the VMM gRPC client.
/// VMM gRPC 客户端的运行时传输选项。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VmmTransportConfig {
    /// gRPC target such as 127.0.0.1:50051.
    /// gRPC 目标，例如 127.0.0.1:50051。
    pub grpc_target: String,
    /// Optional API key reserved for future authenticated gateways.
    /// 为后续认证网关预留的可选 API key。
    pub grpc_api_key: Option<String>,
    /// Connection handshake timeout.
    /// 连接握手超时时间。
    pub handshake_timeout: Duration,
    /// Per-request timeout.
    /// 单次请求超时时间。
    pub request_timeout: Duration,
    /// Optional max receive message bytes.
    /// 可选最大接收消息字节数。
    pub max_receive_message_bytes: Option<usize>,
    /// HTTP/2 keepalive interval.
    /// HTTP/2 keepalive 间隔。
    pub keepalive_time: Duration,
    /// HTTP/2 keepalive acknowledgement timeout.
    /// HTTP/2 keepalive ACK 超时时间。
    pub keepalive_timeout: Duration,
    /// Whether HTTP/2 keepalive can run while no RPC call is active.
    /// 没有活跃 RPC 时是否允许 HTTP/2 keepalive。
    pub keepalive_while_idle: bool,
}

impl VmmTransportConfig {
    /// Builds transport options from an effective VMM config.
    /// 从最终 VMM 配置构建传输选项。
    pub fn from_effective(config: &VmmEffectiveConfig) -> Self {
        Self {
            grpc_target: config.grpc_target.clone(),
            grpc_api_key: config.grpc_api_key.clone(),
            handshake_timeout: Duration::from_millis(config.handshake_timeout_ms),
            request_timeout: Duration::from_millis(config.receive_timeout_ms),
            max_receive_message_bytes: config.max_receive_message_bytes,
            keepalive_time: Duration::from_millis(config.grpc_keepalive_time_ms),
            keepalive_timeout: Duration::from_millis(config.grpc_keepalive_timeout_ms),
            keepalive_while_idle: config.grpc_keepalive_permit_without_calls,
        }
    }
}

/// Tonic-backed VMM client.
/// 基于 Tonic 的 VMM 客户端。
#[derive(Clone)]
pub struct TonicVmmClient {
    /// Resolved transport configuration.
    /// 已解析的传输配置。
    config: VmmTransportConfig,
    /// Connected tonic service client.
    /// 已连接的 tonic 服务客户端。
    client: VmmServiceClient<Channel>,
}

impl TonicVmmClient {
    /// Connects to a VMM gRPC target using the provided transport config.
    /// 使用给定传输配置连接到 VMM gRPC 目标。
    pub async fn connect(config: VmmTransportConfig) -> Result<Self, VmmClientError> {
        let endpoint = normalize_endpoint(&config.grpc_target)?;
        let channel = Endpoint::from_shared(endpoint)
            .map_err(|error| VmmClientError::new("invalid_endpoint", error.to_string()))?
            .connect_timeout(config.handshake_timeout)
            .timeout(config.request_timeout)
            .http2_keep_alive_interval(config.keepalive_time)
            .keep_alive_timeout(config.keepalive_timeout)
            .keep_alive_while_idle(config.keepalive_while_idle)
            .connect()
            .await
            .map_err(status_like_error("connect_failed"))?;
        let mut client = VmmServiceClient::new(channel);
        if let Some(max_receive_message_bytes) = config.max_receive_message_bytes {
            client = client.max_decoding_message_size(max_receive_message_bytes);
        }
        Ok(Self { config, client })
    }

    /// Builds a request with timeout and trace metadata.
    /// 构建带超时和 trace metadata 的请求。
    fn request<T>(&self, payload: T, trace_method: &str) -> Result<Request<T>, VmmClientError> {
        let mut request = Request::new(payload);
        request.set_timeout(self.config.request_timeout);
        let trace_id = format!("vulcan-code-{trace_method}");
        let trace_value = MetadataValue::try_from(trace_id.as_str())
            .map_err(|error| VmmClientError::new("invalid_trace_id", error.to_string()))?;
        request.metadata_mut().insert("x-trace-id", trace_value);
        if let Some(api_key) = self
            .config
            .grpc_api_key
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        {
            let value = MetadataValue::try_from(api_key)
                .map_err(|error| VmmClientError::new("invalid_api_key", error.to_string()))?;
            request.metadata_mut().insert("authorization", value);
        }
        Ok(request)
    }
}

#[async_trait]
impl VmmClient for TonicVmmClient {
    /// Calls VMM Healthz.
    /// 调用 VMM Healthz。
    async fn healthz(&self) -> Result<HealthzResponse, VmmClientError> {
        let mut client = self.client.clone();
        let response = client
            .healthz(self.request((), "healthz")?)
            .await
            .map_err(status_error)?;
        let response = response.into_inner();
        Ok(HealthzResponse {
            status: response.status,
        })
    }

    /// Calls VMM ListProjects.
    /// 调用 VMM ListProjects。
    async fn list_projects(&self) -> Result<ListProjectsResponse, VmmClientError> {
        let mut client = self.client.clone();
        let response = client
            .list_projects(self.request((), "listprojects")?)
            .await
            .map_err(status_error)?;
        Ok(protocol_list_projects_response(response.into_inner()))
    }

    /// Calls VMM ResolveProject.
    /// 调用 VMM ResolveProject。
    async fn resolve_project(
        &self,
        request: ResolveProjectRequest,
    ) -> Result<ResolveProjectResponse, VmmClientError> {
        let mut client = self.client.clone();
        let response = client
            .resolve_project(self.request(grpc_resolve_project_request(request), "resolveproject")?)
            .await
            .map_err(status_error)?;
        Ok(protocol_resolve_project_response(response.into_inner()))
    }

    /// Calls VMM EnsureProject.
    /// 调用 VMM EnsureProject。
    async fn ensure_project(
        &self,
        request: EnsureProjectRequest,
    ) -> Result<EnsureProjectResponse, VmmClientError> {
        let mut client = self.client.clone();
        let response = client
            .ensure_project(self.request(grpc_ensure_project_request(request), "ensureproject")?)
            .await
            .map_err(status_error)?;
        Ok(protocol_ensure_project_response(response.into_inner()))
    }

    /// Calls VMM DeleteProject.
    /// 调用 VMM DeleteProject。
    async fn delete_project(
        &self,
        request: DeleteProjectRequest,
    ) -> Result<DeleteProjectResponse, VmmClientError> {
        let mut client = self.client.clone();
        let response = client
            .delete_project(self.request(grpc_delete_project_request(request), "deleteproject")?)
            .await
            .map_err(status_error)?;
        Ok(protocol_delete_project_response(response.into_inner()))
    }

    /// Calls VMM MigrateProject.
    /// 调用 VMM MigrateProject。
    async fn migrate_project(
        &self,
        request: MigrateProjectRequest,
    ) -> Result<MigrateProjectResponse, VmmClientError> {
        let mut client = self.client.clone();
        let response = client
            .migrate_project(self.request(grpc_migrate_project_request(request), "migrateproject")?)
            .await
            .map_err(status_error)?;
        Ok(protocol_migrate_project_response(response.into_inner()))
    }

    /// Calls VMM ListUsers.
    /// 调用 VMM ListUsers。
    async fn list_users(&self) -> Result<ListUsersResponse, VmmClientError> {
        let mut client = self.client.clone();
        let response = client
            .list_users(self.request((), "listusers")?)
            .await
            .map_err(status_error)?;
        Ok(protocol_list_users_response(response.into_inner()))
    }

    /// Calls VMM ResolveUser.
    /// 调用 VMM ResolveUser。
    async fn resolve_user(
        &self,
        request: ResolveUserRequest,
    ) -> Result<ResolveUserResponse, VmmClientError> {
        let mut client = self.client.clone();
        let response = client
            .resolve_user(self.request(grpc_resolve_user_request(request), "resolveuser")?)
            .await
            .map_err(status_error)?;
        Ok(protocol_resolve_user_response(response.into_inner()))
    }

    /// Calls VMM DeleteUser.
    /// 调用 VMM DeleteUser。
    async fn delete_user(
        &self,
        request: DeleteUserRequest,
    ) -> Result<DeleteUserResponse, VmmClientError> {
        let mut client = self.client.clone();
        let response = client
            .delete_user(self.request(grpc_delete_user_request(request), "deleteuser")?)
            .await
            .map_err(status_error)?;
        Ok(protocol_delete_user_response(response.into_inner()))
    }

    /// Calls VMM GetProfileNodes.
    /// 调用 VMM GetProfileNodes。
    async fn get_profile_nodes(
        &self,
        request: GetProfileNodesRequest,
    ) -> Result<GetProfileNodesResponse, VmmClientError> {
        let mut client = self.client.clone();
        let response = client
            .get_profile_nodes(
                self.request(grpc_get_profile_nodes_request(request), "profilenodes")?,
            )
            .await
            .map_err(status_error)?;
        Ok(protocol_get_profile_nodes_response(response.into_inner()))
    }

    /// Calls VMM GetProfileBundle.
    /// 调用 VMM GetProfileBundle。
    async fn get_profile_bundle(
        &self,
        request: GetProfileBundleRequest,
    ) -> Result<GetProfileBundleResponse, VmmClientError> {
        let mut client = self.client.clone();
        let response = client
            .get_profile_bundle(
                self.request(grpc_get_profile_bundle_request(request), "profilebundle")?,
            )
            .await
            .map_err(status_error)?;
        Ok(protocol_get_profile_bundle_response(response.into_inner()))
    }

    /// Calls VMM ApplyProfileInstruction.
    /// 调用 VMM ApplyProfileInstruction。
    async fn apply_profile_instruction(
        &self,
        request: ApplyProfileInstructionRequest,
    ) -> Result<ApplyProfileInstructionResponse, VmmClientError> {
        let mut client = self.client.clone();
        let response = client
            .apply_profile_instruction(self.request(
                grpc_apply_profile_instruction_request(request),
                "profileinstruction",
            )?)
            .await
            .map_err(status_error)?;
        Ok(protocol_apply_profile_instruction_response(
            response.into_inner(),
        ))
    }

    /// Calls VMM PreCheck.
    /// 调用 VMM PreCheck。
    async fn pre_check(
        &self,
        request: PreCheckRequest,
    ) -> Result<PreCheckResponse, VmmClientError> {
        let mut client = self.client.clone();
        let response = client
            .pre_check(self.request(grpc_precheck_request(request), "precheck")?)
            .await
            .map_err(status_error)?;
        protocol_precheck_response(response.into_inner())
    }

    /// Calls VMM PostAction.
    /// 调用 VMM PostAction。
    async fn post_action(
        &self,
        request: PostActionRequest,
    ) -> Result<PostActionResponse, VmmClientError> {
        let mut client = self.client.clone();
        let response = client
            .post_action(self.request(grpc_post_action_request(request), "postaction")?)
            .await
            .map_err(status_error)?;
        let response = response.into_inner();
        Ok(PostActionResponse {
            accepted: response.accepted,
            trace_id: response.trace_id,
            session_memory_status: protocol_session_memory_status(response.session_memory_status)?,
        })
    }

    /// Calls VMM ChatCompact.
    /// 调用 VMM ChatCompact。
    async fn chat_compact(
        &self,
        request: ChatCompactRequest,
    ) -> Result<ChatCompactResponse, VmmClientError> {
        let mut client = self.client.clone();
        let response = client
            .chat_compact(self.request(grpc_chat_compact_request(request), "chatcompact")?)
            .await
            .map_err(status_error)?;
        let response = response.into_inner();
        Ok(ChatCompactResponse {
            accepted: response.accepted,
            updated: response.updated,
            compacted_turn_id: response.compacted_turn_id,
            trace_id: response.trace_id,
        })
    }

    /// Calls VMM SearchMemoryEvents.
    /// 调用 VMM SearchMemoryEvents。
    async fn search_memory_events(
        &self,
        request: SearchMemoryEventsRequest,
    ) -> Result<SearchMemoryEventsResponse, VmmClientError> {
        let mut client = self.client.clone();
        let response = client
            .search_memory_events(self.request(
                grpc_search_memory_events_request(request),
                "searchmemoryevents",
            )?)
            .await
            .map_err(status_error)?;
        Ok(protocol_search_memory_events_response(
            response.into_inner(),
        ))
    }

    /// Calls VMM GetTurnDetails.
    /// 调用 VMM GetTurnDetails。
    async fn get_turn_details(
        &self,
        request: GetTurnDetailsRequest,
    ) -> Result<GetTurnDetailsResponse, VmmClientError> {
        let mut client = self.client.clone();
        let response = client
            .get_turn_details(self.request(grpc_get_turn_details_request(request), "turndetails")?)
            .await
            .map_err(status_error)?;
        Ok(protocol_get_turn_details_response(response.into_inner()))
    }

    /// Calls VMM WriteMemories.
    /// 调用 VMM WriteMemories。
    async fn write_memories(
        &self,
        request: WriteMemoriesRequest,
    ) -> Result<WriteMemoriesResponse, VmmClientError> {
        let mut client = self.client.clone();
        let response = client
            .write_memories(self.request(grpc_write_memories_request(request), "writememories")?)
            .await
            .map_err(status_error)?;
        Ok(protocol_write_memories_response(response.into_inner()))
    }

    /// Calls VMM DeleteMemories.
    /// 调用 VMM DeleteMemories。
    async fn delete_memories(
        &self,
        request: DeleteMemoriesRequest,
    ) -> Result<DeleteMemoriesResponse, VmmClientError> {
        let mut client = self.client.clone();
        let response = client
            .delete_memories(self.request(grpc_delete_memories_request(request), "deletememories")?)
            .await
            .map_err(status_error)?;
        Ok(protocol_delete_memories_response(response.into_inner()))
    }
}

/// Normalizes a host:port target into a tonic endpoint URL.
/// 将 host:port 目标规范化为 tonic endpoint URL。
fn normalize_endpoint(target: &str) -> Result<String, VmmClientError> {
    let target = target.trim();
    if target.is_empty() {
        return Err(VmmClientError::new(
            "missing_grpc_target",
            "VMM gRPC target is empty",
        ));
    }
    if target.starts_with("http://") || target.starts_with("https://") {
        Ok(target.to_string())
    } else {
        Ok(format!("http://{target}"))
    }
}

/// Converts a protocol PreCheck request into generated protobuf shape.
/// 将协议 PreCheck 请求转换为生成的 protobuf 结构。
fn grpc_precheck_request(request: PreCheckRequest) -> GrpcPreCheckRequest {
    GrpcPreCheckRequest {
        session_id: request.session_id,
        user_id: request.user_id,
        project_id: request.project_id,
        user_content: request.user_content,
        recall_mode: grpc_recall_mode(request.recall_mode) as i32,
    }
}

/// Converts a protocol ResolveProject request into generated protobuf shape.
/// 将协议 ResolveProject 请求转换为生成的 protobuf 结构。
fn grpc_resolve_project_request(request: ResolveProjectRequest) -> GrpcResolveProjectRequest {
    GrpcResolveProjectRequest {
        project_ref: request.project_ref,
    }
}

/// Converts a protocol EnsureProject request into generated protobuf shape.
/// 将协议 EnsureProject 请求转换为生成的 protobuf 结构。
fn grpc_ensure_project_request(request: EnsureProjectRequest) -> GrpcEnsureProjectRequest {
    GrpcEnsureProjectRequest {
        project_path: request.project_path,
        confirm_create: request.confirm_create,
    }
}

/// Converts a protocol DeleteProject request into generated protobuf shape.
/// 将协议 DeleteProject 请求转换为生成的 protobuf 结构。
fn grpc_delete_project_request(request: DeleteProjectRequest) -> GrpcDeleteProjectRequest {
    GrpcDeleteProjectRequest {
        project_path: request.project_path,
        confirm_delete: request.confirm_delete,
    }
}

/// Converts a protocol MigrateProject request into generated protobuf shape.
/// 将协议 MigrateProject 请求转换为生成的 protobuf 结构。
fn grpc_migrate_project_request(request: MigrateProjectRequest) -> GrpcMigrateProjectRequest {
    GrpcMigrateProjectRequest {
        source_project_path: request.source_project_path,
        target_project_path: request.target_project_path,
        confirm_migrate: request.confirm_migrate,
    }
}

/// Converts a protocol ResolveUser request into generated protobuf shape.
/// 将协议 ResolveUser 请求转换为生成的 protobuf 结构。
fn grpc_resolve_user_request(request: ResolveUserRequest) -> GrpcResolveUserRequest {
    GrpcResolveUserRequest {
        user_ref: request.user_ref,
        confirm_create: request.confirm_create,
    }
}

/// Converts a protocol DeleteUser request into generated protobuf shape.
/// 将协议 DeleteUser 请求转换为生成的 protobuf 结构。
fn grpc_delete_user_request(request: DeleteUserRequest) -> GrpcDeleteUserRequest {
    GrpcDeleteUserRequest {
        user_ref: request.user_ref,
        confirmation_code: request.confirmation_code,
    }
}

/// Converts a protocol GetProfileNodes request into generated protobuf shape.
/// 将协议 GetProfileNodes 请求转换为生成的 protobuf 结构。
fn grpc_get_profile_nodes_request(request: GetProfileNodesRequest) -> GrpcGetProfileNodesRequest {
    GrpcGetProfileNodesRequest {
        target: grpc_profile_target(request.target) as i32,
        user_id: request.user_id,
        project_id: request.project_id,
        limit: request.limit,
    }
}

/// Converts a protocol GetProfileBundle request into generated protobuf shape.
/// 将协议 GetProfileBundle 请求转换为生成的 protobuf 结构。
fn grpc_get_profile_bundle_request(
    request: GetProfileBundleRequest,
) -> GrpcGetProfileBundleRequest {
    GrpcGetProfileBundleRequest {
        user_id: request.user_id,
        project_id: request.project_id,
        mode: grpc_profile_bundle_mode(request.mode) as i32,
        include_explanation: request.include_explanation,
    }
}

/// Converts a protocol ApplyProfileInstruction request into generated protobuf shape.
/// 将协议 ApplyProfileInstruction 请求转换为生成的 protobuf 结构。
fn grpc_apply_profile_instruction_request(
    request: ApplyProfileInstructionRequest,
) -> GrpcApplyProfileInstructionRequest {
    GrpcApplyProfileInstructionRequest {
        target: grpc_profile_target(request.target) as i32,
        user_id: request.user_id,
        project_id: request.project_id,
        instruction: request.instruction,
    }
}

/// Converts a protocol PostAction request into generated protobuf shape.
/// 将协议 PostAction 请求转换为生成的 protobuf 结构。
fn grpc_post_action_request(request: PostActionRequest) -> GrpcPostActionRequest {
    GrpcPostActionRequest {
        session_id: request.session_id,
        user_id: request.user_id,
        project_id: request.project_id,
        user_content: request.user_content,
        assistant_content: request.assistant_content,
        timeline: request
            .timeline
            .into_iter()
            .map(|item| GrpcPostActionTimelineItem {
                r#type: item.kind.as_wire_type().to_string(),
                content: item.content,
            })
            .collect(),
    }
}

/// Converts a protocol ChatCompact request into generated protobuf shape.
/// 将协议 ChatCompact 请求转换为生成的 protobuf 结构。
fn grpc_chat_compact_request(request: ChatCompactRequest) -> GrpcChatCompactRequest {
    GrpcChatCompactRequest {
        session_id: request.session_id,
        user_id: request.user_id,
        project_id: request.project_id,
    }
}

/// Converts a protocol SearchMemoryEvents request into generated protobuf shape.
/// 将协议 SearchMemoryEvents 请求转换为生成的 protobuf 结构。
fn grpc_search_memory_events_request(
    request: SearchMemoryEventsRequest,
) -> GrpcSearchMemoryEventsRequest {
    GrpcSearchMemoryEventsRequest {
        user_id: request.user_id,
        project_id: request.project_id,
        queries: request.queries,
        top_k: request.top_k,
    }
}

/// Converts a protocol GetTurnDetails request into generated protobuf shape.
/// 将协议 GetTurnDetails 请求转换为生成的 protobuf 结构。
fn grpc_get_turn_details_request(request: GetTurnDetailsRequest) -> GrpcGetTurnDetailsRequest {
    GrpcGetTurnDetailsRequest {
        turn_ids: request.turn_ids,
    }
}

/// Converts a protocol WriteMemories request into generated protobuf shape.
/// 将协议 WriteMemories 请求转换为生成的 protobuf 结构。
fn grpc_write_memories_request(request: WriteMemoriesRequest) -> GrpcWriteMemoriesRequest {
    GrpcWriteMemoriesRequest {
        session_id: request.session_id,
        user_id: request.user_id,
        project_id: request.project_id,
        items: request
            .items
            .into_iter()
            .map(grpc_write_memory_item)
            .collect(),
    }
}

/// Converts a protocol DeleteMemories request into generated protobuf shape.
/// 将协议 DeleteMemories 请求转换为生成的 protobuf 结构。
fn grpc_delete_memories_request(request: DeleteMemoriesRequest) -> GrpcDeleteMemoriesRequest {
    GrpcDeleteMemoriesRequest {
        user_id: request.user_id,
        project_id: request.project_id,
        memory_ids: request.memory_ids,
        reason: request.reason,
    }
}

/// Converts one protocol write-memory item into generated protobuf shape.
/// 将单条协议写记忆项转换为生成的 protobuf 结构。
fn grpc_write_memory_item(item: WriteMemoryItem) -> GrpcWriteMemoryItem {
    GrpcWriteMemoryItem {
        scope_level: item.scope_level,
        r#abstract: item.abstract_text,
        details: item.details,
        category: item.category,
        priority: item.priority,
        memory_level: item.memory_level,
    }
}

/// Converts a protocol recall mode into generated protobuf enum.
/// 将协议召回模式转换为生成的 protobuf 枚举。
fn grpc_recall_mode(recall_mode: PreCheckRecallMode) -> GrpcPreCheckRecallMode {
    match recall_mode {
        PreCheckRecallMode::PreCheckRecallModeUnspecified => GrpcPreCheckRecallMode::Unspecified,
        PreCheckRecallMode::PreCheckRecallModeSessionCompact => {
            GrpcPreCheckRecallMode::SessionCompact
        }
        PreCheckRecallMode::PreCheckRecallModeFull => GrpcPreCheckRecallMode::Full,
    }
}

/// Converts a protocol profile-bundle mode into generated protobuf enum.
/// 将协议画像 bundle 模式转换为生成的 protobuf 枚举。
fn grpc_profile_bundle_mode(mode: ProfileBundleMode) -> GrpcProfileBundleMode {
    match mode {
        ProfileBundleMode::ProfileBundleModeUnspecified => GrpcProfileBundleMode::Unspecified,
        ProfileBundleMode::ProfileBundleModeFull => GrpcProfileBundleMode::Full,
        ProfileBundleMode::ProfileBundleModeSplit => GrpcProfileBundleMode::Split,
    }
}

/// Converts a protocol profile target into generated protobuf enum.
/// 将协议画像目标转换为生成的 protobuf 枚举。
fn grpc_profile_target(target: ProfileTarget) -> GrpcProfileTarget {
    match target {
        ProfileTarget::ProfileTargetUnspecified => GrpcProfileTarget::Unspecified,
        ProfileTarget::ProfileTargetUser => GrpcProfileTarget::User,
        ProfileTarget::ProfileTargetProject => GrpcProfileTarget::Project,
        ProfileTarget::ProfileTargetTeam => GrpcProfileTarget::Team,
        ProfileTarget::ProfileTargetSpace => GrpcProfileTarget::Space,
        ProfileTarget::ProfileTargetAll => GrpcProfileTarget::All,
    }
}

/// Converts tonic Status into a stable client error.
/// 将 tonic Status 转换为稳定客户端错误。
fn status_error(status: Status) -> VmmClientError {
    VmmClientError {
        kind: status_kind(status.code()).to_string(),
        message: status.message().to_string(),
        grpc_code: Some(status.code() as i32),
        trace_id: None,
    }
}

/// Builds one transport error mapper.
/// 构建一个传输错误映射器。
fn status_like_error(kind: &'static str) -> impl Fn(tonic::transport::Error) -> VmmClientError {
    move |error| VmmClientError::new(kind, error.to_string())
}

/// Returns a stable error kind for one gRPC status code.
/// 返回一个 gRPC 状态码对应的稳定错误类型。
fn status_kind(code: Code) -> &'static str {
    match code {
        Code::InvalidArgument => "invalid_argument",
        Code::NotFound => "not_found",
        Code::DeadlineExceeded => "deadline_exceeded",
        Code::Unavailable => "unavailable",
        Code::ResourceExhausted => "resource_exhausted",
        Code::Unauthenticated => "unauthenticated",
        Code::PermissionDenied => "permission_denied",
        _ => "grpc_error",
    }
}

#[cfg(test)]
mod tests {
    //! Tests for VMM tonic transport conversion helpers.
    //! VMM tonic 传输转换辅助逻辑测试。

    use super::*;
    use crate::grpc::{
        ProfileNodeEntry as GrpcProfileNodeEntry,
        ProfileNodeSourceKind as GrpcProfileNodeSourceKind,
    };
    use crate::protocol::{PostActionTimelineType, ProfileNodeSourceKind};

    /// Verifies that protocol recall modes retain the updated gRPC wire values, especially the active compact-aware value 1.
    /// 验证协议召回模式保持更新后的 gRPC 线协议数值，尤其锁定当前启用的压缩感知模式值 1。
    #[test]
    fn grpc_recall_modes_match_updated_wire_contract() {
        assert_eq!(
            grpc_recall_mode(PreCheckRecallMode::PreCheckRecallModeUnspecified) as i32,
            0
        );
        assert_eq!(
            grpc_recall_mode(PreCheckRecallMode::PreCheckRecallModeSessionCompact) as i32,
            1
        );
        assert_eq!(
            grpc_recall_mode(PreCheckRecallMode::PreCheckRecallModeFull) as i32,
            2
        );
    }

    #[test]
    fn normalize_endpoint_adds_http_scheme_to_bare_target() {
        assert_eq!(
            normalize_endpoint("127.0.0.1:50051").expect("endpoint"),
            "http://127.0.0.1:50051"
        );
    }

    #[test]
    fn post_action_conversion_keeps_wire_timeline_types() {
        let request = grpc_post_action_request(PostActionRequest {
            session_id: String::from("s"),
            user_id: 1,
            project_id: 2,
            user_content: String::from("u"),
            assistant_content: String::from("a"),
            timeline: vec![crate::protocol::PostActionTimelineItem {
                kind: PostActionTimelineType::Assistant,
                content: String::from("ask"),
            }],
        });

        assert_eq!(request.timeline[0].r#type, "assistant");
        assert_eq!(request.timeline[0].content, "ask");
    }

    #[test]
    fn profile_management_conversion_keeps_targets_and_nodes() {
        let request = grpc_get_profile_nodes_request(GetProfileNodesRequest {
            target: ProfileTarget::ProfileTargetTeam,
            user_id: 7,
            project_id: 11,
            limit: 50,
        });
        assert_eq!(request.target, GrpcProfileTarget::Team as i32);
        assert_eq!(request.user_id, 7);
        assert_eq!(request.project_id, 11);
        assert_eq!(request.limit, 50);

        let response = protocol_get_profile_nodes_response(crate::grpc::GetProfileNodesResponse {
            nodes: vec![GrpcProfileNodeEntry {
                profile_node_id: 9,
                target: GrpcProfileTarget::Project as i32,
                bind_id: 11,
                content: String::from("Project uses Rust."),
                priority: String::from("P1"),
                level: String::from("L2"),
                refresh_weight: 3,
                profile_date: String::from("2026-05-30"),
                expires_timestamp: 0,
                level_reason: String::from("stable project fact"),
                source_kind: GrpcProfileNodeSourceKind::ManualInstruction as i32,
                source_id: 77,
            }],
            trace_id: String::from("trace"),
        });

        assert_eq!(response.nodes.len(), 1);
        assert_eq!(
            response.nodes[0].target,
            ProfileTarget::ProfileTargetProject
        );
        assert_eq!(
            response.nodes[0].source_kind,
            ProfileNodeSourceKind::ProfileNodeSourceKindManualInstruction
        );
        assert_eq!(response.trace_id, "trace");
    }
}
