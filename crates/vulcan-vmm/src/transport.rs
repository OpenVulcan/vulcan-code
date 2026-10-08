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
#[derive(Clone, PartialEq, Eq)]
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
    /// Workspace and management RPC timeout.
    /// 工作区与管理类 RPC 超时时间。
    pub workspace_timeout: Duration,
    /// Precheck RPC timeout.
    /// Precheck RPC 超时时间。
    pub precheck_timeout: Duration,
    /// PostAction intake RPC timeout.
    /// PostAction 接收 RPC 超时时间。
    pub post_action_timeout: Duration,
    /// Background profile and compaction RPC timeout.
    /// 后台画像与压缩 RPC 超时时间。
    pub background_timeout: Duration,
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
        let configured_timeout = Duration::from_millis(config.receive_timeout_ms);
        Self {
            grpc_target: config.grpc_target.clone(),
            grpc_api_key: config.grpc_api_key.clone(),
            handshake_timeout: Duration::from_millis(config.handshake_timeout_ms),
            workspace_timeout: configured_timeout,
            precheck_timeout: configured_timeout.min(Duration::from_secs(30)),
            post_action_timeout: configured_timeout,
            background_timeout: configured_timeout.max(Duration::from_secs(120)),
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

    /// Builds a request with the supplied stage timeout and trace metadata.
    /// 使用给定阶段超时构建带 trace metadata 的请求。
    ///
    /// # Parameters
    /// 参数
    /// `payload` is the exact RPC body, `trace_method` identifies the operation, and `timeout`
    /// is the already-selected stage budget.
    /// `payload` 是精确 RPC 正文，`trace_method` 标识操作，`timeout` 是已选定的阶段预算。
    ///
    /// # Returns
    /// 返回值
    /// A Tonic request carrying timeout, trace, and optional authorization metadata.
    /// 返回携带超时、追踪与可选授权元数据的 Tonic 请求。
    fn request_with_timeout<T>(
        &self,
        payload: T,
        trace_method: &str,
        timeout: Duration,
    ) -> Result<Request<T>, VmmClientError> {
        let mut request = Request::new(payload);
        request.set_timeout(timeout);
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

    /// Builds a workspace or management request.
    /// 构建工作区或管理类请求。
    ///
    /// # Parameters
    /// 参数
    /// `payload` is the exact RPC body and `trace_method` identifies the management operation.
    /// `payload` 是精确 RPC 正文，`trace_method` 标识管理操作。
    ///
    /// # Returns
    /// 返回值
    /// A request governed by the configured workspace timeout.
    /// 返回受已配置工作区超时约束的请求。
    fn request<T>(&self, payload: T, trace_method: &str) -> Result<Request<T>, VmmClientError> {
        self.request_with_timeout(payload, trace_method, self.config.workspace_timeout)
    }

    /// Builds a precheck request protected by the short interactive budget.
    /// 使用短交互预算构建 Precheck 请求。
    ///
    /// # Parameters
    /// 参数
    /// `payload` is the exact RPC body and `trace_method` identifies the Precheck operation.
    /// `payload` 是精确 RPC 正文，`trace_method` 标识 Precheck 操作。
    ///
    /// # Returns
    /// 返回值
    /// A request governed by the short interactive Precheck timeout.
    /// 返回受短交互 Precheck 超时约束的请求。
    fn precheck_request<T>(
        &self,
        payload: T,
        trace_method: &str,
    ) -> Result<Request<T>, VmmClientError> {
        self.request_with_timeout(payload, trace_method, self.config.precheck_timeout)
    }

    /// Builds a PostAction intake request without extending background analysis semantics.
    /// 构建 PostAction 接收请求，不混淆后台分析超时语义。
    ///
    /// # Parameters
    /// 参数
    /// `payload` is the exact RPC body and `trace_method` identifies the PostAction intake.
    /// `payload` 是精确 RPC 正文，`trace_method` 标识 PostAction 接收操作。
    ///
    /// # Returns
    /// 返回值
    /// A request governed only by the PostAction intake timeout.
    /// 返回仅受 PostAction 接收超时约束的请求。
    fn post_action_request<T>(
        &self,
        payload: T,
        trace_method: &str,
    ) -> Result<Request<T>, VmmClientError> {
        self.request_with_timeout(payload, trace_method, self.config.post_action_timeout)
    }

    /// Builds a background profile or compaction request with the long budget.
    /// 使用长预算构建后台画像或压缩请求。
    ///
    /// # Parameters
    /// 参数
    /// `payload` is the exact RPC body and `trace_method` identifies the background operation.
    /// `payload` 是精确 RPC 正文，`trace_method` 标识后台操作。
    ///
    /// # Returns
    /// 返回值
    /// A request governed by the long-running background timeout.
    /// 返回受长时后台超时约束的请求。
    fn background_request<T>(
        &self,
        payload: T,
        trace_method: &str,
    ) -> Result<Request<T>, VmmClientError> {
        self.request_with_timeout(payload, trace_method, self.config.background_timeout)
    }
}

#[async_trait]
impl VmmClient for TonicVmmClient {
    /// Resolves cached-memory ownership locally from this connection and the exact remote IDs.
    /// 根据此连接及准确远端 ID 在本地解析缓存记忆归属。
    /// Host project is intentionally unused externally; no management RPC or credential output occurs.
    /// 外部模式明确不使用宿主项目；不发起管理 RPC 或输出凭据。
    async fn memory_source(
        &self,
        _host_project_id: &str,
        user_id: u64,
        project_id: u64,
    ) -> Result<crate::VmmMemorySource, VmmClientError> {
        crate::VmmMemorySource::external(&self.config, user_id, project_id)
    }

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

    /// Resolves an exact remote read identity without probing externally managed visibility.
    /// 解析准确远端读取身份，不探测外部自行管理的可见范围。
    /// Takes request user/project IDs and returns the configured transport identity with no local scopes.
    /// 接收请求用户/项目 ID，返回不包含本地范围的已配置传输身份。
    async fn memory_read_source(
        &self,
        _host_project_id: &str,
        user_id: u64,
        project_id: u64,
    ) -> Result<crate::VmmMemoryReadSource, VmmClientError> {
        // External query scope is owned by VMM, not inferred or configured by the host.
        // 外置查询范围由 VMM 拥有，不由宿主推断或配置。
        crate::VmmMemoryReadSource::new(
            crate::VmmMemorySource::external(&self.config, user_id, project_id)?,
            Vec::new(),
        )
    }

    /// Calls VMM GetProfileNodes after verifying the optional local read expectation.
    /// 核对可选本地读取预期后调用 VMM GetProfileNodes。
    async fn get_profile_nodes(
        &self,
        request: GetProfileNodesRequest,
    ) -> Result<GetProfileNodesResponse, VmmClientError> {
        // Direct management requests retain their target-specific ID semantics without a cache expectation.
        // 未提供缓存预期时，直接管理请求保留其目标专属 ID 语义。
        if let Some(expected) = request.expected_read_source.as_ref() {
            self.memory_read_source(
                &request.host_project_id,
                request.user_id,
                request.project_id,
            )
            .await?
            .verify_expected(Some(expected))?;
        }
        let mut client = self.client.clone();
        let response = client
            .get_profile_nodes(
                self.background_request(grpc_get_profile_nodes_request(request), "profilenodes")?,
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
                self.background_request(grpc_get_profile_bundle_request(request), "profilebundle")?,
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
            .apply_profile_instruction(self.background_request(
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
        // Freeze this immutable connection and exact request IDs before consuming the request.
        // 在消费请求前冻结本不可变连接及请求的准确身份。
        let read_source = self
            .memory_read_source(
                &request.host_project_id,
                request.user_id,
                request.project_id,
            )
            .await?;
        let mut client = self.client.clone();
        let response = client
            .pre_check(self.precheck_request(grpc_precheck_request(request), "precheck")?)
            .await
            .map_err(status_error)?;
        protocol_precheck_response(response.into_inner(), read_source)
    }

    /// Calls VMM PostAction.
    /// 调用 VMM PostAction。
    async fn post_action(
        &self,
        request: PostActionRequest,
    ) -> Result<PostActionResponse, VmmClientError> {
        // Verify against the frozen connected route before creating or sending the wire request.
        // 创建或发送线上请求前，根据冻结的已连接路由校验。
        crate::VmmMemorySource::external(&self.config, request.user_id, request.project_id)?
            .verify_expected(request.expected_source.as_ref())?;
        let mut client = self.client.clone();
        let response = client
            .post_action(self.post_action_request(grpc_post_action_request(request), "postaction")?)
            .await
            .map_err(status_error)?;
        let response = response.into_inner();
        Ok(PostActionResponse {
            accepted: response.accepted,
            trace_id: response.trace_id,
            session_memory_status: protocol_session_memory_status(response.session_memory_status)?,
        })
    }

    /// Checks revocation immediately before dispatch on the immutable connected remote route.
    /// 在不可变已连接远端路由上派发之前立即检查撤销状态。
    async fn post_action_with_permit(
        &self,
        request: PostActionRequest,
        permit: &crate::VmmWritebackPermit,
    ) -> Result<PostActionResponse, VmmClientError> {
        permit.validate_request(&request)?;
        self.post_action(request).await
    }

    /// Calls VMM ChatCompact.
    /// 调用 VMM ChatCompact。
    async fn chat_compact(
        &self,
        request: ChatCompactRequest,
    ) -> Result<ChatCompactResponse, VmmClientError> {
        let mut client = self.client.clone();
        let response = client
            .chat_compact(
                self.background_request(grpc_chat_compact_request(request), "chatcompact")?,
            )
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
pub(crate) fn normalize_endpoint(target: &str) -> Result<String, VmmClientError> {
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

    /// Verifies exact local source retention for empty and non-empty responses and rejects missing provenance.
    /// 验证空与非空响应均保留准确本地来源，并拒绝缺失来源的响应。
    /// Uses synthetic transport configuration only; returns after conversion and serialization assertions.
    /// 仅使用合成传输配置；完成转换与序列化断言后返回。
    #[test]
    fn precheck_response_preserves_frozen_source_without_remote_scope_inference() {
        // This synthetic credential must remain only in local configuration, never in the response.
        // 此合成凭据只能留在本地配置中，不能进入响应。
        let config = VmmTransportConfig::from_effective(&VmmEffectiveConfig {
            grpc_target: "127.0.0.1:50051".to_owned(),
            grpc_api_key: Some("synthetic-precheck-secret".to_owned()),
            ..Default::default()
        });
        // The host owns the route identity; remote protobuf has no authority to supply it.
        // 宿主拥有路由身份；远端 protobuf 无权提供该身份。
        let source = crate::VmmMemoryReadSource::new(
            crate::VmmMemorySource::external(&config, 3, 7).expect("owner"),
            Vec::new(),
        )
        .expect("source");
        for has_content in [false, true] {
            // Exercise both an empty lifecycle result and actual memory content through the production converter.
            // 通过生产转换器验证空生命周期结果与真实格式记忆内容。
            let response = protocol_precheck_response(
                crate::grpc::PreCheckResponse {
                    should_inject: has_content,
                    context_items: if has_content {
                        vec![crate::grpc::ContextItem {
                            memory_id: 11,
                            text: "frozen-source-content".to_owned(),
                            ..Default::default()
                        }]
                    } else {
                        Vec::new()
                    },
                    session_memory_status: crate::grpc::SessionMemoryStatus::Active as i32,
                    ..Default::default()
                },
                source.clone(),
            )
            .expect("response");
            assert_eq!(response.read_source, source);
            assert!(response.read_source.visible_project_ids.is_empty());
            assert_eq!(response.context_items.len(), usize::from(has_content));
            // A serialized local response must not silently recreate a missing read identity.
            // 序列化本地响应不能静默重建缺失的读取身份。
            let mut serialized = serde_json::to_value(&response).expect("serialize");
            assert!(!serialized.to_string().contains("synthetic-precheck-secret"));
            assert_eq!(
                serde_json::from_value::<PreCheckResponse>(serialized.clone()).expect("decode"),
                response
            );
            serialized
                .as_object_mut()
                .expect("object")
                .remove("read_source");
            assert!(serde_json::from_value::<PreCheckResponse>(serialized).is_err());
        }
    }

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

    /// Verifies interactive, intake, and background RPC budgets are derived independently instead of inheriting one global endpoint timeout.
    /// 验证交互、接收与后台 RPC 预算会独立派生，而不是继承同一个 Endpoint 全局超时。
    #[test]
    fn transport_config_derives_stage_specific_rpc_timeouts() {
        let config = VmmEffectiveConfig {
            receive_timeout_ms: 45_000,
            ..VmmEffectiveConfig::default()
        };

        let transport = VmmTransportConfig::from_effective(&config);

        assert_eq!(transport.workspace_timeout, Duration::from_secs(45));
        assert_eq!(transport.precheck_timeout, Duration::from_secs(30));
        assert_eq!(transport.post_action_timeout, Duration::from_secs(45));
        assert_eq!(transport.background_timeout, Duration::from_secs(120));
    }

    #[test]
    fn post_action_conversion_keeps_wire_timeline_types() {
        let request = grpc_post_action_request(PostActionRequest {
            expected_source: Some(crate::VmmMemorySource::native(2).expect("local metadata only")),
            operation_id: String::from("operation"),
            session_id: String::from("s"),
            user_id: 1,
            project_id: 2,
            host_project_id: String::from("host-project"),
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

    /// Rejects revoked, unowned and wrong-route deliveries before reaching a listening test endpoint.
    /// 在到达监听中的测试端点之前拒绝已撤销、无来源及路由不匹配的投递。
    #[tokio::test]
    async fn guarded_post_action_rejects_before_any_remote_connection() {
        // A private loopback listener detects accidental dispatch without a real VMM service or paid model.
        // 私有回环监听器检测意外派发，不需要真实 VMM 服务或付费模型。
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("test listener");
        listener
            .set_nonblocking(true)
            .expect("nonblocking listener");
        let config = VmmTransportConfig::from_effective(&VmmEffectiveConfig {
            grpc_target: format!(
                "http://{}",
                listener.local_addr().expect("listener address")
            ),
            ..Default::default()
        });
        let source = crate::VmmMemorySource::external(&config, 1, 7).expect("remote source");
        let client = TonicVmmClient {
            client: VmmServiceClient::new(
                Endpoint::from_shared(config.grpc_target.clone())
                    .expect("endpoint")
                    .connect_lazy(),
            ),
            config,
        };
        let request = PostActionRequest {
            expected_source: Some(source),
            user_id: 1,
            project_id: 7,
            ..Default::default()
        };
        let revoked = crate::VmmWritebackPermit::new().expect("old permit");
        revoked.revoke();
        assert_eq!(
            client
                .post_action_with_permit(request.clone(), &revoked)
                .await
                .expect_err("cancelled")
                .kind,
            "writeback_cancelled"
        );
        let current = crate::VmmWritebackPermit::new().expect("current permit");
        assert_eq!(
            client
                .post_action_with_permit(PostActionRequest::default(), &current)
                .await
                .expect_err("missing provenance")
                .kind,
            "missing_memory_source"
        );
        let wrong_route = PostActionRequest {
            expected_source: Some(crate::VmmMemorySource::native(7).expect("native")),
            ..request
        };
        assert_eq!(
            client
                .post_action_with_permit(wrong_route, &current)
                .await
                .expect_err("wrong route")
                .kind,
            "memory_source_changed"
        );
        // Profile preparation must reject a wrong backend locally before any gRPC dispatch too.
        // 画像准备也必须在任何 gRPC 派发前于本地拒绝错误后端。
        assert_eq!(
            client
                .get_profile_nodes(GetProfileNodesRequest {
                    expected_read_source: Some(
                        crate::VmmMemoryReadSource::new(
                            crate::VmmMemorySource::native(7).expect("native owner"),
                            vec![7],
                        )
                        .expect("native read scope")
                    ),
                    user_id: 1,
                    project_id: 7,
                    target: ProfileTarget::ProfileTargetProject,
                    limit: 50,
                    ..Default::default()
                })
                .await
                .expect_err("wrong profile source")
                .kind,
            "memory_read_source_changed"
        );
        assert_eq!(
            listener
                .accept()
                .expect_err("no connection dispatched")
                .kind(),
            std::io::ErrorKind::WouldBlock
        );
    }

    #[test]
    fn profile_management_conversion_keeps_targets_and_nodes() {
        let request = grpc_get_profile_nodes_request(GetProfileNodesRequest {
            expected_read_source: None,
            target: ProfileTarget::ProfileTargetTeam,
            user_id: 7,
            project_id: 11,
            host_project_id: String::from("host-project"),
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

    /// Only the gRPC Unimplemented status marks an RPC as unsupported; labels and text never do.
    /// 只有 gRPC Unimplemented 状态表示 RPC 不受支持；类型标签与正文绝不起作用。
    #[test]
    fn unimplemented_is_decided_by_the_grpc_status_code_alone() {
        assert!(status_error(Status::unimplemented("DeleteMemories")).is_unimplemented());
        assert!(!status_error(Status::unavailable("unimplemented")).is_unimplemented());
        assert!(!VmmClientError::new("unimplemented", "unimplemented").is_unimplemented());
    }
}
