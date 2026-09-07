//! VMM client abstraction used by runtime adapters and tests.
//! 运行时适配器与测试使用的 VMM 客户端抽象。

use crate::protocol::{
    ApplyProfileInstructionRequest, ApplyProfileInstructionResponse, ChatCompactRequest,
    ChatCompactResponse, DeleteMemoriesRequest, DeleteMemoriesResponse, DeleteProjectRequest,
    DeleteProjectResponse, DeleteUserRequest, DeleteUserResponse, EnsureProjectRequest,
    EnsureProjectResponse, GetProfileBundleRequest, GetProfileBundleResponse,
    GetProfileNodesRequest, GetProfileNodesResponse, GetTurnDetailsRequest, GetTurnDetailsResponse,
    HealthzResponse, ListProjectsResponse, ListUsersResponse, MigrateProjectRequest,
    MigrateProjectResponse, PostActionRequest, PostActionResponse, PreCheckRequest,
    PreCheckResponse, ResolveProjectRequest, ResolveProjectResponse, ResolveUserRequest,
    ResolveUserResponse, SearchMemoryEventsRequest, SearchMemoryEventsResponse,
    WriteMemoriesRequest, WriteMemoriesResponse,
};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::fmt::{self, Display};
use std::sync::{Arc, OnceLock, RwLock};

/// Process-local native VMM client registry installed by the VulcanCode composition root.
/// 由 VulcanCode 组合根安装的进程内原生 VMM 客户端注册表。
static PROCESS_NATIVE_VMM_CLIENT: OnceLock<RwLock<Option<Arc<dyn VmmClient>>>> = OnceLock::new();

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

/// Installs or replaces the process-local native VMM client without exposing a network endpoint.
/// 安装或替换进程内原生 VMM 客户端，且不暴露网络端点。
///
/// # Parameters
/// 参数
/// `client` is the host-owned direct Rust adapter over the copied Core.
/// `client` 是宿主持有并直接覆盖复制 Core 的 Rust 适配器。
///
/// # Returns
/// 返回值
/// Success after atomic replacement, or a lock-poison diagnostic.
/// 原子替换后返回成功，锁中毒时返回诊断。
pub fn install_process_native_vmm_client(client: Arc<dyn VmmClient>) -> Result<(), String> {
    let registry = PROCESS_NATIVE_VMM_CLIENT.get_or_init(|| RwLock::new(None));
    let mut slot = registry
        .write()
        .map_err(|_| String::from("The process-local native VMM client registry is locked."))?;
    *slot = Some(client);
    Ok(())
}

/// Resolves the process-local native VMM client installed by the service host.
/// 解析由服务宿主安装的进程内原生 VMM 客户端。
///
/// # Returns
/// 返回值
/// The shared direct Rust client, or an explicit readiness error.
/// 返回共享的直接 Rust 客户端，未就绪时返回显式错误。
pub fn process_native_vmm_client() -> Result<Arc<dyn VmmClient>, VmmClientError> {
    PROCESS_NATIVE_VMM_CLIENT
        .get_or_init(|| RwLock::new(None))
        .read()
        .map_err(|_| {
            VmmClientError::new(
                "native_registry_locked",
                "The process-local native VMM client registry cannot be read.",
            )
        })?
        .clone()
        .ok_or_else(|| {
            VmmClientError::new(
                "native_not_ready",
                "The process-local native VMM client is not installed.",
            )
        })
}

/// Async VMM client abstraction.
/// 异步 VMM 客户端抽象。
#[async_trait]
pub trait VmmClient: Send + Sync {
    /// Resolves the actual memory owner for cache admission without running AI or remote management.
    /// 解析缓存准入的实际记忆归属，不运行 AI 或远端管理。
    /// Takes the host binding identity and request envelope IDs; returns an exact source identity.
    /// 接收宿主绑定身份及请求封套 ID；返回准确来源身份。
    async fn memory_source(
        &self,
        host_project_id: &str,
        user_id: u64,
        project_id: u64,
    ) -> Result<crate::VmmMemorySource, VmmClientError>;

    /// Resolves read ownership and native linked visibility without AI or remote management calls.
    /// 不调用 AI 或远端管理，解析读取归属及原生连带可见范围。
    /// Takes exact host and envelope identities; returns the complete local read source.
    /// 接收准确宿主及封套身份；返回完整本地读取来源。
    async fn memory_read_source(
        &self,
        host_project_id: &str,
        user_id: u64,
        project_id: u64,
    ) -> Result<crate::VmmMemoryReadSource, VmmClientError>;

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

    /// Calls VMM DeleteProject.
    /// 调用 VMM DeleteProject。
    async fn delete_project(
        &self,
        request: DeleteProjectRequest,
    ) -> Result<DeleteProjectResponse, VmmClientError>;

    /// Calls VMM MigrateProject.
    /// 调用 VMM MigrateProject。
    async fn migrate_project(
        &self,
        request: MigrateProjectRequest,
    ) -> Result<MigrateProjectResponse, VmmClientError>;

    /// Calls VMM ListUsers.
    /// 调用 VMM ListUsers。
    async fn list_users(&self) -> Result<ListUsersResponse, VmmClientError>;

    /// Calls VMM ResolveUser.
    /// 调用 VMM ResolveUser。
    async fn resolve_user(
        &self,
        request: ResolveUserRequest,
    ) -> Result<ResolveUserResponse, VmmClientError>;

    /// Calls VMM DeleteUser.
    /// 调用 VMM DeleteUser。
    async fn delete_user(
        &self,
        request: DeleteUserRequest,
    ) -> Result<DeleteUserResponse, VmmClientError>;

    /// Calls VMM GetProfileNodes.
    /// 调用 VMM GetProfileNodes。
    async fn get_profile_nodes(
        &self,
        request: GetProfileNodesRequest,
    ) -> Result<GetProfileNodesResponse, VmmClientError>;

    /// Calls VMM GetProfileBundle.
    /// 调用 VMM GetProfileBundle。
    async fn get_profile_bundle(
        &self,
        request: GetProfileBundleRequest,
    ) -> Result<GetProfileBundleResponse, VmmClientError>;

    /// Calls VMM ApplyProfileInstruction.
    /// 调用 VMM ApplyProfileInstruction。
    async fn apply_profile_instruction(
        &self,
        request: ApplyProfileInstructionRequest,
    ) -> Result<ApplyProfileInstructionResponse, VmmClientError>;

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

    /// Delivers a queued host request under its revocable process-local admission generation.
    /// 在可撤销进程内准入代次下投递排队宿主请求。
    /// Takes a source-frozen request and live permit; returns acceptance or explicit terminal cancellation.
    /// 接收来源冻结请求及活动许可；返回接受结果或明确终态取消。
    async fn post_action_with_permit(
        &self,
        request: PostActionRequest,
        permit: &crate::VmmWritebackPermit,
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

    /// Calls VMM DeleteMemories.
    /// 调用 VMM DeleteMemories。
    async fn delete_memories(
        &self,
        request: DeleteMemoriesRequest,
    ) -> Result<DeleteMemoriesResponse, VmmClientError>;
}
