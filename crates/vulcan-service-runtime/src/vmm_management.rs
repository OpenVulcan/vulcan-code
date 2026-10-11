//! VMM management RPC helpers used by settings surfaces.
//! 设置界面使用的 VMM 管理 RPC 辅助逻辑。

use std::sync::Arc;

use vulcan_vmm::{
    ApplyProfileInstructionRequest, ApplyProfileInstructionResponse, DeleteMemoriesRequest,
    DeleteMemoriesResponse, DeleteProjectRequest, DeleteProjectResponse, DeleteUserRequest,
    DeleteUserResponse, EnsureProjectRequest, EnsureProjectResponse, GetProfileBundleRequest,
    GetProfileBundleResponse, GetProfileNodesRequest, GetProfileNodesResponse,
    GetTurnDetailsRequest, GetTurnDetailsResponse, ListProjectsResponse, ListUsersResponse,
    MigrateProjectRequest, MigrateProjectResponse, ProfileBundleMode, ProfileTarget,
    ResolveProjectRequest, ResolveProjectResponse, ResolveUserRequest, ResolveUserResponse,
    SearchMemoryEventsRequest, SearchMemoryEventsResponse, TonicVmmClient, VmmClient,
    VmmConfigLayer, VmmEffectiveConfig, VmmTransportConfig, WriteMemoriesRequest,
    WriteMemoriesResponse, WriteMemoryItem,
};

/// Maximum memory ids accepted by one delete action.
/// 单次删除动作允许接收的最大记忆标识数量。
const MAX_DELETE_MEMORY_IDS: usize = 256;

/// Maximum audit reason characters accepted by one delete action.
/// 单次删除动作允许接收的最大审计原因字符数。
const MAX_DELETE_MEMORY_REASON_CHARS: usize = 1024;

/// Fallback reason used when a manual delete omits one.
/// 手工删除未提供原因时使用的兜底原因。
const DEFAULT_DELETE_MEMORY_REASON: &str = "manual memory delete requested through Vulcan Code";

/// Dedicated VMM management action used by the main memory workbench.
/// 主记忆工作台使用的专用 VMM 管理动作。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VmmWorkbenchAction {
    /// Check the configured VMM gRPC endpoint.
    /// 检查已配置的 VMM gRPC 端点。
    Healthz,
    /// Load durable users and projects.
    /// 读取长期用户与项目列表。
    LoadUsersAndProjects,
    /// Resolve current user/project references.
    /// 解析当前用户与项目引用。
    ResolveUserAndProject,
    /// Ensure current project reference without creation confirmation.
    /// 在不确认创建的前提下预检当前项目引用。
    EnsureProjectPreview,
    /// Load active profile nodes for one target.
    /// 读取某个目标的 active 画像节点。
    LoadProfileNodes {
        /// Profile target to inspect.
        /// 需要查看的画像目标。
        target: ProfileTarget,
    },
    /// Load one full profile bundle.
    /// 读取一份完整画像 bundle。
    LoadProfileBundle,
    /// Apply one manual profile instruction.
    /// 应用一条手工画像指令。
    ApplyProfileInstruction {
        /// Profile target to update.
        /// 需要更新的画像目标。
        target: ProfileTarget,
        /// Natural-language instruction.
        /// 自然语言指令。
        instruction: String,
    },
    /// Resolve or create-preview one user reference.
    /// 解析或创建预检一个用户引用。
    ResolveUser {
        /// User reference to resolve.
        /// 需要解析的用户引用。
        user_ref: String,
        /// Whether user creation is confirmed.
        /// 是否确认创建用户。
        confirm_create: bool,
    },
    /// Delete one user after guarded confirmation.
    /// 在保护性确认后删除一个用户。
    DeleteUser {
        /// User reference to delete.
        /// 需要删除的用户引用。
        user_ref: String,
        /// Confirmation code returned by VMM.
        /// VMM 返回的确认码。
        confirmation_code: String,
    },
    /// Ensure one project path.
    /// 确认一个项目路径。
    EnsureProject {
        /// Canonical project path.
        /// 标准项目路径。
        project_path: String,
        /// Whether project creation is confirmed.
        /// 是否确认创建项目。
        confirm_create: bool,
    },
    /// Delete one project path after confirmation.
    /// 在确认后删除一个项目路径。
    DeleteProject {
        /// Canonical project path to delete.
        /// 需要删除的标准项目路径。
        project_path: String,
        /// Whether deletion is confirmed.
        /// 是否确认删除。
        confirm_delete: bool,
    },
    /// Migrate one project path into another path.
    /// 将一个项目路径迁移到另一个路径。
    MigrateProject {
        /// Source canonical project path.
        /// 源标准项目路径。
        source_project_path: String,
        /// Target canonical project path.
        /// 目标标准项目路径。
        target_project_path: String,
        /// Whether migration is confirmed.
        /// 是否确认迁移。
        confirm_migrate: bool,
    },
    /// Search memory events through the AI tool equivalent.
    /// 通过 AI 工具等价入口检索记忆事件。
    SearchMemoryEvents {
        /// Query list to search.
        /// 需要检索的查询列表。
        queries: Vec<String>,
        /// Maximum hit count per query.
        /// 每条查询的最大命中数量。
        top_k: u32,
    },
    /// Load durable turn details through the AI tool equivalent.
    /// 通过 AI 工具等价入口读取长期 turn 详情。
    GetTurnDetails {
        /// Turn ids to inspect.
        /// 需要查看的 turn 标识。
        turn_ids: Vec<u64>,
    },
    /// Write one memory item through the AI tool equivalent.
    /// 通过 AI 工具等价入口写入一条记忆。
    WriteMemories {
        /// Host session identifier for attribution.
        /// 用于归因的宿主会话标识。
        session_id: String,
        /// Direct memory item to write.
        /// 需要主动写入的记忆项。
        item: WriteMemoryItem,
    },
    /// Delete explicit memory ids through the AI tool equivalent.
    /// 通过 AI 工具等价入口删除明确指定的记忆标识。
    DeleteMemories {
        /// Durable memory ids requested for deletion.
        /// 请求删除的长期记忆标识集合。
        memory_ids: Vec<u64>,
        /// Human or AI supplied deletion reason.
        /// 人类或 AI 提供的删除原因。
        reason: String,
    },
}

/// Structured outcome returned by one VMM management action.
/// 一次 VMM 管理动作返回的结构化结果。
#[derive(Debug, Clone)]
pub enum VmmManagementOutcome {
    /// Health check result.
    /// 健康检查结果。
    Healthz(String),
    /// User and project lists.
    /// 用户与项目列表。
    UsersAndProjects {
        /// Durable users returned by VMM.
        /// VMM 返回的长期用户列表。
        users: ListUsersResponse,
        /// Durable projects returned by VMM.
        /// VMM 返回的长期项目列表。
        projects: ListProjectsResponse,
    },
    /// Resolved user and project pair.
    /// 已解析用户与项目组合。
    Resolved {
        /// Resolved user response.
        /// 已解析用户响应。
        user: ResolveUserResponse,
        /// Resolved project response.
        /// 已解析项目响应。
        project: ResolveProjectResponse,
    },
    /// Ensure-project preview result.
    /// 项目预检结果。
    EnsureProject(EnsureProjectResponse),
    /// Delete-user result.
    /// 删除用户结果。
    DeleteUser(DeleteUserResponse),
    /// Delete-project result.
    /// 删除项目结果。
    DeleteProject(DeleteProjectResponse),
    /// Migrate-project result.
    /// 迁移项目结果。
    MigrateProject(MigrateProjectResponse),
    /// Active profile nodes.
    /// Active 画像节点。
    ProfileNodes(GetProfileNodesResponse),
    /// Profile bundle result.
    /// 画像 bundle 结果。
    ProfileBundle(GetProfileBundleResponse),
    /// Manual profile instruction result.
    /// 手工画像指令结果。
    ProfileInstruction(ApplyProfileInstructionResponse),
    /// Resolved user result.
    /// 已解析用户结果。
    ResolveUser(ResolveUserResponse),
    /// Memory-search tool result.
    /// 记忆检索工具结果。
    SearchMemoryEvents(SearchMemoryEventsResponse),
    /// Turn-detail tool result.
    /// Turn 详情工具结果。
    GetTurnDetails(GetTurnDetailsResponse),
    /// Direct-write memory tool result.
    /// 主动写入记忆工具结果。
    WriteMemories(WriteMemoriesResponse),
    /// Memory-delete tool result.
    /// 记忆删除工具结果。
    DeleteMemories(DeleteMemoriesResponse),
}

/// Executes one dedicated VMM workbench management action.
/// 执行一次专用 VMM 工作台管理动作。
///
/// # Parameters
/// 参数
/// `action` is the exact typed workbench operation selected by the caller.
/// `action` 是调用方选择的准确强类型工作台操作。
/// `layer` contains the already resolved backend and behavior configuration.
/// `layer` 包含已经解析的后端与行为配置。
/// `management_user_ref` and `management_project_ref` contain explicit catalog identities.
/// `management_user_ref` 与 `management_project_ref` 包含明确的目录身份。
/// `host_project_id` contains the VulcanCode project identity used only by native binding resolution.
/// `host_project_id` 包含仅供原生绑定解析使用的 VulcanCode 项目标识。
///
/// # Returns
/// 返回值
/// The typed action outcome returned by the selected native or external client.
/// 返回所选原生或外置客户端产生的强类型动作结果。
pub async fn run_vmm_workbench_action(
    action: VmmWorkbenchAction,
    layer: VmmConfigLayer,
    management_user_ref: String,
    management_project_ref: String,
    host_project_id: String,
) -> Result<VmmManagementOutcome, String> {
    let effective = VmmEffectiveConfig::resolve(&layer, None, None);
    // client selects exactly the globally configured backend without leaking transport choices to callers.
    // client 严格选择全局配置的后端，不向调用方泄漏传输选择。
    let client: Arc<dyn VmmClient> = match effective.backend_mode {
        vulcan_vmm::VmmBackendMode::Native => {
            vulcan_vmm::process_native_vmm_client().map_err(|error| error.to_string())?
        }
        vulcan_vmm::VmmBackendMode::ExternalGrpc => {
            if effective.grpc_target.trim().is_empty() {
                return Err(String::from(
                    "The VMM gRPC address is empty; the management call cannot run.",
                ));
            }
            Arc::new(
                TonicVmmClient::connect(VmmTransportConfig::from_effective(&effective))
                    .await
                    .map_err(|error| error.to_string())?,
            )
        }
    };
    match action {
        VmmWorkbenchAction::Healthz => run_vmm_healthz(client.as_ref()).await,
        VmmWorkbenchAction::LoadUsersAndProjects => {
            run_vmm_list_users_and_projects(client.as_ref()).await
        }
        VmmWorkbenchAction::ResolveUserAndProject => {
            run_vmm_resolve_user_and_project(
                client.as_ref(),
                &effective,
                &management_user_ref,
                &management_project_ref,
            )
            .await
        }
        VmmWorkbenchAction::EnsureProjectPreview => {
            run_vmm_ensure_project_preview(client.as_ref(), &management_project_ref).await
        }
        VmmWorkbenchAction::LoadProfileNodes { target } => {
            run_vmm_profile_nodes(
                client.as_ref(),
                &management_user_ref,
                &management_project_ref,
                &host_project_id,
                target,
            )
            .await
        }
        VmmWorkbenchAction::LoadProfileBundle => {
            run_vmm_profile_bundle_preview(
                client.as_ref(),
                &management_user_ref,
                &management_project_ref,
                &host_project_id,
            )
            .await
        }
        VmmWorkbenchAction::ApplyProfileInstruction {
            target,
            instruction,
        } => {
            run_vmm_apply_profile_instruction(
                client.as_ref(),
                &management_user_ref,
                &management_project_ref,
                &host_project_id,
                target,
                instruction,
            )
            .await
        }
        VmmWorkbenchAction::ResolveUser {
            user_ref,
            confirm_create,
        } => run_vmm_resolve_user(client.as_ref(), user_ref, confirm_create).await,
        VmmWorkbenchAction::DeleteUser {
            user_ref,
            confirmation_code,
        } => run_vmm_delete_user(client.as_ref(), user_ref, confirmation_code).await,
        VmmWorkbenchAction::EnsureProject {
            project_path,
            confirm_create,
        } => run_vmm_ensure_project(client.as_ref(), project_path, confirm_create).await,
        VmmWorkbenchAction::DeleteProject {
            project_path,
            confirm_delete,
        } => run_vmm_delete_project(client.as_ref(), project_path, confirm_delete).await,
        VmmWorkbenchAction::MigrateProject {
            source_project_path,
            target_project_path,
            confirm_migrate,
        } => {
            run_vmm_migrate_project(
                client.as_ref(),
                source_project_path,
                target_project_path,
                confirm_migrate,
            )
            .await
        }
        VmmWorkbenchAction::SearchMemoryEvents { queries, top_k } => {
            run_vmm_search_memory_events(
                client.as_ref(),
                &effective,
                &host_project_id,
                queries,
                top_k,
            )
            .await
        }
        VmmWorkbenchAction::GetTurnDetails { turn_ids } => {
            run_vmm_get_turn_details(client.as_ref(), &effective, &host_project_id, turn_ids).await
        }
        VmmWorkbenchAction::WriteMemories { session_id, item } => {
            run_vmm_write_memories(
                client.as_ref(),
                &effective,
                &host_project_id,
                session_id,
                item,
            )
            .await
        }
        VmmWorkbenchAction::DeleteMemories { memory_ids, reason } => {
            run_vmm_delete_memories(
                client.as_ref(),
                &effective,
                &host_project_id,
                memory_ids,
                reason,
            )
            .await
        }
    }
}

/// Runs the VMM health check management call.
/// 执行 VMM 健康检查管理调用。
async fn run_vmm_healthz(client: &dyn VmmClient) -> Result<VmmManagementOutcome, String> {
    let response = client.healthz().await.map_err(|error| error.to_string())?;
    Ok(VmmManagementOutcome::Healthz(response.status))
}

/// Lists users and projects in a compact settings notice.
/// 以紧凑设置提示列出用户与项目。
async fn run_vmm_list_users_and_projects(
    client: &dyn VmmClient,
) -> Result<VmmManagementOutcome, String> {
    let users = client
        .list_users()
        .await
        .map_err(|error| error.to_string())?;
    let projects = client
        .list_projects()
        .await
        .map_err(|error| error.to_string())?;
    Ok(VmmManagementOutcome::UsersAndProjects { users, projects })
}

/// Resolves configured user and project references.
/// 解析当前配置中的用户与项目引用。
async fn run_vmm_resolve_user_and_project(
    client: &dyn VmmClient,
    effective: &VmmEffectiveConfig,
    management_user_ref: &str,
    management_project_ref: &str,
) -> Result<VmmManagementOutcome, String> {
    let user_ref = management_user_ref.trim().to_string();
    let user_ref = if user_ref.is_empty() {
        effective
            .user_id
            .map(|value| value.to_string())
            .ok_or_else(|| String::from("Both the VMM management user reference and user ID are empty; the user cannot be resolved."))?
    } else {
        user_ref
    };
    let project_ref = management_project_ref.trim().to_string();
    let project_ref = if project_ref.is_empty() {
        effective
            .project_id
            .map(|value| value.to_string())
            .ok_or_else(|| String::from("Both the VMM management project reference and project ID are empty; the project cannot be resolved."))?
    } else {
        project_ref
    };
    let user = client
        .resolve_user(ResolveUserRequest {
            user_ref,
            confirm_create: false,
        })
        .await
        .map_err(|error| error.to_string())?;
    let project = client
        .resolve_project(ResolveProjectRequest { project_ref })
        .await
        .map_err(|error| error.to_string())?;
    Ok(VmmManagementOutcome::Resolved { user, project })
}

/// Ensures the configured project path without allowing creation.
/// 在不允许创建的前提下确认当前项目路径。
async fn run_vmm_ensure_project_preview(
    client: &dyn VmmClient,
    management_project_ref: &str,
) -> Result<VmmManagementOutcome, String> {
    let project_path = management_project_ref.trim();
    if project_path.is_empty() {
        return Err(String::from(
            "The VMM management project reference is empty; the Team/Space/Project path cannot be prechecked.",
        ));
    }
    let response = client
        .ensure_project(EnsureProjectRequest {
            project_path: project_path.to_string(),
            confirm_create: false,
        })
        .await
        .map_err(|error| error.to_string())?;
    Ok(VmmManagementOutcome::EnsureProject(response))
}

/// Resolves one user reference with an explicit creation flag.
/// 使用显式创建标记解析一个用户引用。
async fn run_vmm_resolve_user(
    client: &dyn VmmClient,
    user_ref: String,
    confirm_create: bool,
) -> Result<VmmManagementOutcome, String> {
    let user_ref = user_ref.trim().to_string();
    if user_ref.is_empty() {
        return Err(String::from("The user reference must not be empty."));
    }
    let response = client
        .resolve_user(ResolveUserRequest {
            user_ref,
            confirm_create,
        })
        .await
        .map_err(|error| error.to_string())?;
    Ok(VmmManagementOutcome::ResolveUser(response))
}

/// Deletes one user with the confirmation code supplied by VMM.
/// 使用 VMM 提供的确认码删除一个用户。
async fn run_vmm_delete_user(
    client: &dyn VmmClient,
    user_ref: String,
    confirmation_code: String,
) -> Result<VmmManagementOutcome, String> {
    let user_ref = user_ref.trim().to_string();
    if user_ref.is_empty() {
        return Err(String::from("The user reference must not be empty."));
    }
    let response = client
        .delete_user(DeleteUserRequest {
            user_ref,
            confirmation_code: confirmation_code.trim().to_string(),
        })
        .await
        .map_err(|error| error.to_string())?;
    Ok(VmmManagementOutcome::DeleteUser(response))
}

/// Ensures one project path with an explicit creation flag.
/// 使用显式创建标记确认一个项目路径。
async fn run_vmm_ensure_project(
    client: &dyn VmmClient,
    project_path: String,
    confirm_create: bool,
) -> Result<VmmManagementOutcome, String> {
    let project_path = project_path.trim().to_string();
    if project_path.is_empty() {
        return Err(String::from("The project path must not be empty."));
    }
    let response = client
        .ensure_project(EnsureProjectRequest {
            project_path,
            confirm_create,
        })
        .await
        .map_err(|error| error.to_string())?;
    Ok(VmmManagementOutcome::EnsureProject(response))
}

/// Deletes one project path with an explicit confirmation flag.
/// 使用显式确认标记删除一个项目路径。
async fn run_vmm_delete_project(
    client: &dyn VmmClient,
    project_path: String,
    confirm_delete: bool,
) -> Result<VmmManagementOutcome, String> {
    let project_path = project_path.trim().to_string();
    if project_path.is_empty() {
        return Err(String::from("The project path must not be empty."));
    }
    let response = client
        .delete_project(DeleteProjectRequest {
            project_path,
            confirm_delete,
        })
        .await
        .map_err(|error| error.to_string())?;
    Ok(VmmManagementOutcome::DeleteProject(response))
}

/// Migrates one project path into another path with an explicit confirmation flag.
/// 使用显式确认标记将一个项目路径迁移到另一个路径。
async fn run_vmm_migrate_project(
    client: &dyn VmmClient,
    source_project_path: String,
    target_project_path: String,
    confirm_migrate: bool,
) -> Result<VmmManagementOutcome, String> {
    let source_project_path = source_project_path.trim().to_string();
    let target_project_path = target_project_path.trim().to_string();
    if source_project_path.is_empty() || target_project_path.is_empty() {
        return Err(String::from(
            "Neither the source project path nor the target project path may be empty.",
        ));
    }
    let response = client
        .migrate_project(MigrateProjectRequest {
            source_project_path,
            target_project_path,
            confirm_migrate,
        })
        .await
        .map_err(|error| error.to_string())?;
    Ok(VmmManagementOutcome::MigrateProject(response))
}

/// Loads one explicitly selected profile bundle in combined mode.
/// 按组合模式读取一份明确选择的画像 bundle。
///
/// # Parameters
/// 参数
/// `client` is the connected VMM transport client.
/// `client` 是已连接的 VMM 传输客户端。
/// `management_user_ref` is the exact numeric catalog user key selected by the operator.
/// `management_user_ref` 是操作人员明确选择的数字目录用户键。
/// `management_project_ref` is the exact numeric catalog project key selected by the operator.
/// `management_project_ref` 是操作人员明确选择的数字目录项目键。
/// `host_project_id` identifies the VulcanCode project when native binding resolution is required.
/// `host_project_id` 在需要原生绑定解析时标识 VulcanCode 项目。
///
/// # Returns
/// 返回值
/// The complete profile bundle for the selected user and project pair.
/// 返回所选用户与项目组合的完整画像 bundle。
async fn run_vmm_profile_bundle_preview(
    client: &dyn VmmClient,
    management_user_ref: &str,
    management_project_ref: &str,
    host_project_id: &str,
) -> Result<VmmManagementOutcome, String> {
    let (user_id, project_id) =
        parse_vmm_profile_identity_ids(management_user_ref, management_project_ref)?;
    let response = client
        .get_profile_bundle(GetProfileBundleRequest {
            user_id,
            project_id,
            host_project_id: host_project_id.to_owned(),
            mode: ProfileBundleMode::ProfileBundleModeFull,
            include_explanation: Some(true),
        })
        .await
        .map_err(|error| error.to_string())?;
    Ok(VmmManagementOutcome::ProfileBundle(response))
}

/// Loads active profile nodes for one explicitly selected catalog target.
/// 读取一个明确选择的目录目标的 active 画像节点。
///
/// # Parameters
/// 参数
/// `client` is the connected VMM transport client.
/// `client` 是已连接的 VMM 传输客户端。
/// `management_user_ref` is the exact numeric catalog user key selected by the operator.
/// `management_user_ref` 是操作人员明确选择的数字目录用户键。
/// `management_project_ref` is the exact numeric catalog project key selected by the operator.
/// `management_project_ref` 是操作人员明确选择的数字目录项目键。
/// `host_project_id` identifies the VulcanCode project when native binding resolution is required.
/// `host_project_id` 在需要原生绑定解析时标识 VulcanCode 项目。
/// `target` selects whether VMM returns user or project profile nodes.
/// `target` 选择 VMM 返回用户画像节点还是项目画像节点。
///
/// # Returns
/// 返回值
/// Active nodes for the selected profile target.
/// 返回所选画像目标的 active 节点。
async fn run_vmm_profile_nodes(
    client: &dyn VmmClient,
    management_user_ref: &str,
    management_project_ref: &str,
    host_project_id: &str,
    target: ProfileTarget,
) -> Result<VmmManagementOutcome, String> {
    let (user_id, project_id) =
        parse_vmm_profile_identity_ids(management_user_ref, management_project_ref)?;
    let response = client
        .get_profile_nodes(GetProfileNodesRequest {
            expected_read_source: None,
            target,
            user_id,
            project_id,
            host_project_id: host_project_id.to_owned(),
            limit: 200,
        })
        .await
        .map_err(|error| error.to_string())?;
    Ok(VmmManagementOutcome::ProfileNodes(response))
}

/// Applies one manual profile instruction to an explicitly selected catalog target.
/// 向一个明确选择的目录目标应用一条手工画像指令。
///
/// # Parameters
/// 参数
/// `client` is the connected VMM transport client.
/// `client` 是已连接的 VMM 传输客户端。
/// `management_user_ref` is the exact numeric catalog user key selected by the operator.
/// `management_user_ref` 是操作人员明确选择的数字目录用户键。
/// `management_project_ref` is the exact numeric catalog project key selected by the operator.
/// `management_project_ref` 是操作人员明确选择的数字目录项目键。
/// `host_project_id` identifies the VulcanCode project when native binding resolution is required.
/// `host_project_id` 在需要原生绑定解析时标识 VulcanCode 项目。
/// `target` selects the user or project profile namespace.
/// `target` 选择用户或项目画像命名空间。
/// `instruction` is the natural-language correction to apply.
/// `instruction` 是需要应用的自然语言修正。
///
/// # Returns
/// 返回值
/// The durable instruction result returned by VMM.
/// 返回 VMM 生成的长期指令结果。
async fn run_vmm_apply_profile_instruction(
    client: &dyn VmmClient,
    management_user_ref: &str,
    management_project_ref: &str,
    host_project_id: &str,
    target: ProfileTarget,
    instruction: String,
) -> Result<VmmManagementOutcome, String> {
    let instruction = instruction.trim().to_string();
    if instruction.is_empty() {
        return Err(String::from(
            "The manual profile instruction must not be empty.",
        ));
    }
    let (user_id, project_id) =
        parse_vmm_profile_identity_ids(management_user_ref, management_project_ref)?;
    let response = client
        .apply_profile_instruction(ApplyProfileInstructionRequest {
            target,
            user_id,
            project_id,
            host_project_id: host_project_id.to_owned(),
            instruction,
        })
        .await
        .map_err(|error| error.to_string())?;
    Ok(VmmManagementOutcome::ProfileInstruction(response))
}

/// Parses the opaque catalog keys used by profile RPCs into their protocol identifiers.
/// 将画像 RPC 使用的不透明目录键解析为协议数字标识。
///
/// # Parameters
/// 参数
/// `management_user_ref` is the exact `stable_key` returned by the VMM user catalog.
/// `management_user_ref` 是 VMM 用户目录返回的准确 `stable_key`。
/// `management_project_ref` is the exact `stable_key` returned by the VMM project catalog.
/// `management_project_ref` 是 VMM 项目目录返回的准确 `stable_key`。
///
/// # Returns
/// 返回值
/// The numeric `(user_id, project_id)` pair required by profile RPCs.
/// 返回画像 RPC 所需的数字 `(user_id, project_id)` 组合。
fn parse_vmm_profile_identity_ids(
    management_user_ref: &str,
    management_project_ref: &str,
) -> Result<(u64, u64), String> {
    // user_id is the exact numeric key emitted by `vmm_management_catalog_from_outcome`.
    // user_id 是 `vmm_management_catalog_from_outcome` 产生的准确数字键。
    let user_id = management_user_ref
        .trim()
        .parse::<u64>()
        .map_err(|_| String::from("Select a valid VMM user before reading the profile."))?;
    // project_id is the exact numeric key emitted by `vmm_management_catalog_from_outcome`.
    // project_id 是 `vmm_management_catalog_from_outcome` 产生的准确数字键。
    let project_id = management_project_ref
        .trim()
        .parse::<u64>()
        .map_err(|_| String::from("Select a valid VMM project before reading the profile."))?;
    Ok((user_id, project_id))
}

/// Runs the memory-search AI-tool equivalent against the configured user/project.
/// 针对当前配置的用户与项目执行记忆检索 AI 工具等价调用。
/// `host_project_id` selects the native write owner and recall links; it is empty for external gRPC.
/// `host_project_id` 选择原生写入归属与连带召回项目；外置 gRPC 下为空。
async fn run_vmm_search_memory_events(
    client: &dyn VmmClient,
    effective: &VmmEffectiveConfig,
    host_project_id: &str,
    queries: Vec<String>,
    top_k: u32,
) -> Result<VmmManagementOutcome, String> {
    let queries = queries
        .into_iter()
        .map(|query| query.trim().to_string())
        .filter(|query| !query.is_empty())
        .collect::<Vec<_>>();
    if queries.is_empty() {
        return Err(String::from("The memory search query must not be empty."));
    }
    let user_id = effective
        .user_id
        .ok_or_else(|| String::from("The VMM user ID is empty; memory search cannot run."))?;
    let project_id = effective
        .project_id
        .ok_or_else(|| String::from("The VMM project ID is empty; memory search cannot run."))?;
    let response = client
        .search_memory_events(SearchMemoryEventsRequest {
            runtime_session_id: None,
            user_id,
            project_id,
            host_project_id: host_project_id.to_owned(),
            queries,
            top_k: top_k.max(1),
        })
        .await
        .map_err(|error| error.to_string())?;
    Ok(VmmManagementOutcome::SearchMemoryEvents(response))
}

/// Runs the turn-detail AI-tool equivalent.
/// 执行 Turn 详情 AI 工具等价调用。
/// `host_project_id` selects the native project binding; external gRPC retains its legacy request identity.
/// `host_project_id` 选择原生项目绑定；外置 gRPC 保留既有请求身份语义。
async fn run_vmm_get_turn_details(
    client: &dyn VmmClient,
    effective: &VmmEffectiveConfig,
    host_project_id: &str,
    turn_ids: Vec<u64>,
) -> Result<VmmManagementOutcome, String> {
    if turn_ids.is_empty() {
        return Err(String::from("The turn ID list must not be empty."));
    }
    // request carries native ownership only for the in-process backend; external gRPC keeps its legacy zero identity.
    // request 仅在进程内后端携带原生归属；外置 gRPC 保持原有零身份语义。
    let request = if effective.backend_mode == vulcan_vmm::VmmBackendMode::Native {
        GetTurnDetailsRequest {
            user_id: effective
                .user_id
                .ok_or_else(|| String::from("The native VMM user identity is not initialized."))?,
            project_id: effective.project_id.ok_or_else(|| {
                String::from("The native VMM project identity is not initialized.")
            })?,
            host_project_id: host_project_id.to_owned(),
            turn_ids,
            runtime_session_id: None,
        }
    } else {
        GetTurnDetailsRequest {
            turn_ids,
            ..GetTurnDetailsRequest::default()
        }
    };
    let response = client
        .get_turn_details(request)
        .await
        .map_err(|error| error.to_string())?;
    Ok(VmmManagementOutcome::GetTurnDetails(response))
}

/// Runs the direct-write AI-tool equivalent against the configured user/project.
/// 针对当前配置的用户与项目执行主动写入 AI 工具等价调用。
/// `host_project_id` selects the sole native write owner; it is empty for external gRPC.
/// `host_project_id` 选择唯一的原生写入归属；外置 gRPC 下为空。
async fn run_vmm_write_memories(
    client: &dyn VmmClient,
    effective: &VmmEffectiveConfig,
    host_project_id: &str,
    session_id: String,
    item: WriteMemoryItem,
) -> Result<VmmManagementOutcome, String> {
    if item.abstract_text.trim().is_empty() && item.details.trim().is_empty() {
        return Err(String::from(
            "The summary and details of a manual memory write must not both be empty.",
        ));
    }
    let user_id = effective
        .user_id
        .ok_or_else(|| String::from("The VMM user ID is empty; memories cannot be written."))?;
    let project_id = effective
        .project_id
        .ok_or_else(|| String::from("The VMM project ID is empty; memories cannot be written."))?;
    let response = client
        .write_memories(WriteMemoriesRequest {
            session_id,
            user_id,
            project_id,
            host_project_id: host_project_id.to_owned(),
            items: vec![item],
        })
        .await
        .map_err(|error| error.to_string())?;
    Ok(VmmManagementOutcome::WriteMemories(response))
}

/// Runs the memory-delete AI-tool equivalent against the configured user/project.
/// 针对当前配置的用户与项目执行记忆删除 AI 工具等价调用。
/// `host_project_id` selects the native project binding; it is empty for external gRPC.
/// `host_project_id` 选择原生项目绑定；外置 gRPC 下为空。
async fn run_vmm_delete_memories(
    client: &dyn VmmClient,
    effective: &VmmEffectiveConfig,
    host_project_id: &str,
    memory_ids: Vec<u64>,
    reason: String,
) -> Result<VmmManagementOutcome, String> {
    let user_id = effective
        .user_id
        .ok_or_else(|| String::from("The VMM user ID is empty; memories cannot be deleted."))?;
    let project_id = effective
        .project_id
        .ok_or_else(|| String::from("The VMM project ID is empty; memories cannot be deleted."))?;
    let memory_ids = normalize_delete_memory_ids(memory_ids)?;
    let reason = normalize_delete_memory_reason(reason);
    let response = client
        .delete_memories(DeleteMemoriesRequest {
            runtime_session_id: None,
            user_id,
            project_id,
            host_project_id: host_project_id.to_owned(),
            memory_ids,
            reason,
        })
        .await
        .map_err(format_delete_memory_error)?;
    Ok(VmmManagementOutcome::DeleteMemories(response))
}

/// Normalizes delete-memory ids by dropping zeroes and duplicates.
/// 通过移除零值与重复项归一化待删除记忆标识。
fn normalize_delete_memory_ids(memory_ids: Vec<u64>) -> Result<Vec<u64>, String> {
    let mut normalized = Vec::new();
    for memory_id in memory_ids {
        if memory_id == 0 || normalized.contains(&memory_id) {
            continue;
        }
        normalized.push(memory_id);
        if normalized.len() > MAX_DELETE_MEMORY_IDS {
            return Err(format!(
                "At most {MAX_DELETE_MEMORY_IDS} memories can be deleted at once; split the request and retry."
            ));
        }
    }
    if normalized.is_empty() {
        return Err(String::from("The memory ID list must not be empty."));
    }
    Ok(normalized)
}

/// Normalizes delete-memory reason with a stable fallback.
/// 使用稳定兜底文本归一化删除记忆原因。
fn normalize_delete_memory_reason(reason: String) -> String {
    let reason = reason.trim();
    let reason = if reason.is_empty() {
        DEFAULT_DELETE_MEMORY_REASON
    } else {
        reason
    };
    reason
        .chars()
        .take(MAX_DELETE_MEMORY_REASON_CHARS)
        .collect()
}

/// Formats VMM delete-memory transport errors with upgrade guidance.
/// 将 VMM 删除记忆传输错误格式化为带升级指引的错误。
fn format_delete_memory_error(error: vulcan_vmm::VmmClientError) -> String {
    if error.is_unimplemented() {
        String::from(
            "The current VMM service version does not support deleting memories; update VulcanMemoryMesh and retry.",
        )
    } else {
        error.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies profile actions use the selected catalog ids instead of configured defaults.
    /// 验证画像动作使用所选目录标识，而不是退回配置默认值。
    #[test]
    fn profile_identity_parser_uses_explicit_catalog_keys() {
        assert_eq!(
            parse_vmm_profile_identity_ids("17", "29").expect("valid catalog keys"),
            (17, 29)
        );
        assert!(parse_vmm_profile_identity_ids("user-name", "29").is_err());
        assert!(parse_vmm_profile_identity_ids("17", "project-path").is_err());
    }
}
