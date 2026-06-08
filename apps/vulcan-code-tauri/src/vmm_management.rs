//! VMM management RPC helpers used by settings surfaces.
//! 设置界面使用的 VMM 管理 RPC 辅助逻辑。

use vulcan_vmm::{
    ApplyProfileInstructionRequest, ApplyProfileInstructionResponse, DeleteProjectRequest,
    DeleteProjectResponse, DeleteUserRequest, DeleteUserResponse, EnsureProjectRequest,
    EnsureProjectResponse, GetProfileBundleRequest, GetProfileBundleResponse,
    GetProfileNodesRequest, GetProfileNodesResponse, GetTurnDetailsRequest, GetTurnDetailsResponse,
    ListProjectsResponse, ListUsersResponse, MigrateProjectRequest, MigrateProjectResponse,
    ProfileBundleMode, ProfileTarget, ResolveProjectRequest, ResolveProjectResponse,
    ResolveUserRequest, ResolveUserResponse, SearchMemoryEventsRequest, SearchMemoryEventsResponse,
    TonicVmmClient, VmmClient, VmmConfigLayer, VmmEffectiveConfig, VmmTransportConfig,
    WriteMemoriesRequest, WriteMemoriesResponse, WriteMemoryItem,
};

/// One user-triggered VMM management action.
/// 一种由用户触发的 VMM 管理动作。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum VmmManagementAction {
    /// Check the configured VMM gRPC endpoint.
    /// 检查已配置的 VMM gRPC 端点。
    Healthz,
    /// List available VMM users and projects.
    /// 列出可用的 VMM 用户与项目。
    ListUsersAndProjects,
    /// Resolve the configured user and project references.
    /// 解析当前配置中的用户与项目引用。
    ResolveUserAndProject,
    /// Ensure the configured project path without creation confirmation.
    /// 在不确认创建的前提下预检当前配置中的项目路径。
    EnsureProjectPreview,
    /// Load the configured user/project profile bundle.
    /// 读取当前用户与项目的画像 bundle。
    ProfileBundlePreview,
}

/// Dedicated VMM management action used by the main memory workbench.
/// 主记忆工作台使用的专用 VMM 管理动作。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum VmmWorkbenchAction {
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
}

/// Structured outcome returned by one VMM management action.
/// 一次 VMM 管理动作返回的结构化结果。
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub(crate) enum VmmManagementOutcome {
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
}

impl VmmManagementOutcome {
    /// Builds one compact user-facing summary.
    /// 构建一条紧凑的用户可见摘要。
    pub(crate) fn summary(&self) -> String {
        match self {
            Self::Healthz(status) => format!("VMM 连接正常：{status}"),
            Self::UsersAndProjects { users, projects } => format!(
                "用户 {} 个；项目 {} 个。",
                users.users.len(),
                projects.projects.len()
            ),
            Self::Resolved { user, project } => {
                let user_text = user
                    .user
                    .as_ref()
                    .map(|entry| format!("{}:{}", entry.user_id, entry.user_name))
                    .unwrap_or_else(|| user.message.clone());
                let project_text = project
                    .project
                    .as_ref()
                    .map(|entry| format!("{}:{}", entry.project_id, entry.display_path))
                    .unwrap_or_else(|| project.message.clone());
                format!("用户 {user_text}；项目 {project_text}。")
            }
            Self::EnsureProject(response) => {
                let project_text = response
                    .project
                    .as_ref()
                    .map(|entry| format!("{}:{}", entry.project_id, entry.display_path))
                    .unwrap_or_else(|| response.message.clone());
                format!(
                    "项目预检：{project_text}；exists={}；needs_confirm={}。",
                    response.exists, response.needs_confirm
                )
            }
            Self::DeleteUser(response) => format!(
                "删除用户：{}；requires_confirmation={}；deleted_users={}；deleted_memories={}；deleted_profiles={}。",
                response.message,
                response.requires_confirmation,
                response.deleted_users,
                response.deleted_memories,
                response.deleted_profiles
            ),
            Self::DeleteProject(response) => format!(
                "删除项目：{}；needs_confirm={}；deleted_projects={}；deleted_memories={}；deleted_profiles={}。",
                response.message,
                response.needs_confirm,
                response.deleted_projects,
                response.deleted_memories,
                response.deleted_profiles
            ),
            Self::MigrateProject(response) => format!(
                "迁移项目：{}；needs_confirm={}；migrated_sessions={}；migrated_memories={}；rebuilt_vectors={}。",
                response.message,
                response.needs_confirm,
                response.migrated_sessions,
                response.migrated_memories,
                response.rebuilt_vector_rows
            ),
            Self::ProfileNodes(response) => {
                format!("已读取 {} 个 active 画像节点。", response.nodes.len())
            }
            Self::ProfileBundle(response) => format!(
                "画像 bundle 已读取：combined={} 字符，team={}，space={}，project={}，user={}。",
                response.combined_text.chars().count(),
                response.team_profile.chars().count(),
                response.space_profile.chars().count(),
                response.project_profile.chars().count(),
                response.user_profile.chars().count()
            ),
            Self::ProfileInstruction(response) => format!(
                "画像指令已应用：instruction_id={}；accepted={}；retired={}。",
                response.instruction_id,
                response.accepted_nodes.len(),
                response.retired_nodes.len()
            ),
            Self::ResolveUser(response) => {
                let user_text = response
                    .user
                    .as_ref()
                    .map(|entry| format!("{}:{}", entry.user_id, entry.user_name))
                    .unwrap_or_else(|| response.message.clone());
                format!(
                    "用户解析：{user_text}；created={}；exists={}。",
                    response.created, response.exists
                )
            }
            Self::SearchMemoryEvents(response) => {
                let hit_count = response
                    .results
                    .iter()
                    .map(|group| group.hits.len())
                    .sum::<usize>();
                format!(
                    "记忆检索完成：queries={}；hits={}；trace={}。",
                    response.results.len(),
                    hit_count,
                    response.trace_id
                )
            }
            Self::GetTurnDetails(response) => format!(
                "Turn 详情读取完成：turns={}；trace={}。",
                response.turns.len(),
                response.trace_id
            ),
            Self::WriteMemories(response) => format!(
                "主动写入完成：items={}；trace={}。",
                response.items.len(),
                response.trace_id
            ),
        }
    }
}

impl VmmManagementAction {
    /// Returns the visible action label.
    /// 返回可见动作标签。
    #[allow(dead_code)]
    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::Healthz => "测试连接",
            Self::ListUsersAndProjects => "列出用户/项目",
            Self::ResolveUserAndProject => "解析用户/项目",
            Self::EnsureProjectPreview => "预检项目",
            Self::ProfileBundlePreview => "读取画像",
        }
    }
}

/// Executes one VMM management action from a draft config layer.
/// 基于草稿配置层执行一个 VMM 管理动作。
#[allow(dead_code)]
pub(crate) async fn run_vmm_management_action(
    action: VmmManagementAction,
    layer: VmmConfigLayer,
    management_user_ref: String,
    management_project_ref: String,
) -> Result<String, String> {
    let outcome = run_vmm_management_action_structured(
        action,
        layer,
        management_user_ref,
        management_project_ref,
    )
    .await?;
    Ok(outcome.summary())
}

/// Executes one VMM management action and returns structured data.
/// 执行一次 VMM 管理动作并返回结构化数据。
#[allow(dead_code)]
pub(crate) async fn run_vmm_management_action_structured(
    action: VmmManagementAction,
    layer: VmmConfigLayer,
    management_user_ref: String,
    management_project_ref: String,
) -> Result<VmmManagementOutcome, String> {
    let effective = VmmEffectiveConfig::resolve(&layer, None, None);
    if effective.grpc_target.trim().is_empty() {
        return Err(String::from("VMM gRPC 地址为空，无法执行管理调用。"));
    }
    let client = TonicVmmClient::connect(VmmTransportConfig::from_effective(&effective))
        .await
        .map_err(|error| error.to_string())?;
    match action {
        VmmManagementAction::Healthz => run_vmm_healthz(&client).await,
        VmmManagementAction::ListUsersAndProjects => run_vmm_list_users_and_projects(&client).await,
        VmmManagementAction::ResolveUserAndProject => {
            run_vmm_resolve_user_and_project(
                &client,
                &effective,
                &management_user_ref,
                &management_project_ref,
            )
            .await
        }
        VmmManagementAction::EnsureProjectPreview => {
            run_vmm_ensure_project_preview(&client, &management_project_ref).await
        }
        VmmManagementAction::ProfileBundlePreview => {
            run_vmm_profile_bundle_preview(&client, &effective).await
        }
    }
}

/// Executes one dedicated VMM workbench management action.
/// 执行一次专用 VMM 工作台管理动作。
pub(crate) async fn run_vmm_workbench_action(
    action: VmmWorkbenchAction,
    layer: VmmConfigLayer,
    management_user_ref: String,
    management_project_ref: String,
) -> Result<VmmManagementOutcome, String> {
    let effective = VmmEffectiveConfig::resolve(&layer, None, None);
    if effective.grpc_target.trim().is_empty() {
        return Err(String::from("VMM gRPC 地址为空，无法执行管理调用。"));
    }
    let client = TonicVmmClient::connect(VmmTransportConfig::from_effective(&effective))
        .await
        .map_err(|error| error.to_string())?;
    match action {
        VmmWorkbenchAction::Healthz => run_vmm_healthz(&client).await,
        VmmWorkbenchAction::LoadUsersAndProjects => run_vmm_list_users_and_projects(&client).await,
        VmmWorkbenchAction::ResolveUserAndProject => {
            run_vmm_resolve_user_and_project(
                &client,
                &effective,
                &management_user_ref,
                &management_project_ref,
            )
            .await
        }
        VmmWorkbenchAction::EnsureProjectPreview => {
            run_vmm_ensure_project_preview(&client, &management_project_ref).await
        }
        VmmWorkbenchAction::LoadProfileNodes { target } => {
            run_vmm_profile_nodes(&client, &effective, target).await
        }
        VmmWorkbenchAction::LoadProfileBundle => {
            run_vmm_profile_bundle_preview(&client, &effective).await
        }
        VmmWorkbenchAction::ApplyProfileInstruction {
            target,
            instruction,
        } => run_vmm_apply_profile_instruction(&client, &effective, target, instruction).await,
        VmmWorkbenchAction::ResolveUser {
            user_ref,
            confirm_create,
        } => run_vmm_resolve_user(&client, user_ref, confirm_create).await,
        VmmWorkbenchAction::DeleteUser {
            user_ref,
            confirmation_code,
        } => run_vmm_delete_user(&client, user_ref, confirmation_code).await,
        VmmWorkbenchAction::EnsureProject {
            project_path,
            confirm_create,
        } => run_vmm_ensure_project(&client, project_path, confirm_create).await,
        VmmWorkbenchAction::DeleteProject {
            project_path,
            confirm_delete,
        } => run_vmm_delete_project(&client, project_path, confirm_delete).await,
        VmmWorkbenchAction::MigrateProject {
            source_project_path,
            target_project_path,
            confirm_migrate,
        } => {
            run_vmm_migrate_project(
                &client,
                source_project_path,
                target_project_path,
                confirm_migrate,
            )
            .await
        }
        VmmWorkbenchAction::SearchMemoryEvents { queries, top_k } => {
            run_vmm_search_memory_events(&client, &effective, queries, top_k).await
        }
        VmmWorkbenchAction::GetTurnDetails { turn_ids } => {
            run_vmm_get_turn_details(&client, turn_ids).await
        }
        VmmWorkbenchAction::WriteMemories { session_id, item } => {
            run_vmm_write_memories(&client, &effective, session_id, item).await
        }
    }
}

/// Runs the VMM health check management call.
/// 执行 VMM 健康检查管理调用。
async fn run_vmm_healthz(client: &TonicVmmClient) -> Result<VmmManagementOutcome, String> {
    let response = client.healthz().await.map_err(|error| error.to_string())?;
    Ok(VmmManagementOutcome::Healthz(response.status))
}

/// Lists users and projects in a compact settings notice.
/// 以紧凑设置提示列出用户与项目。
async fn run_vmm_list_users_and_projects(
    client: &TonicVmmClient,
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
    client: &TonicVmmClient,
    effective: &VmmEffectiveConfig,
    management_user_ref: &str,
    management_project_ref: &str,
) -> Result<VmmManagementOutcome, String> {
    let user_ref = management_user_ref.trim().to_string();
    let user_ref = if user_ref.is_empty() {
        effective
            .user_id
            .map(|value| value.to_string())
            .ok_or_else(|| String::from("VMM 管理用户引用和用户 ID 都为空，无法解析用户。"))?
    } else {
        user_ref
    };
    let project_ref = management_project_ref.trim().to_string();
    let project_ref = if project_ref.is_empty() {
        effective
            .project_id
            .map(|value| value.to_string())
            .ok_or_else(|| String::from("VMM 管理项目引用和项目 ID 都为空，无法解析项目。"))?
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
    client: &TonicVmmClient,
    management_project_ref: &str,
) -> Result<VmmManagementOutcome, String> {
    let project_path = management_project_ref.trim();
    if project_path.is_empty() {
        return Err(String::from(
            "VMM 管理项目引用为空，无法预检 Team/Space/Project 路径。",
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
    client: &TonicVmmClient,
    user_ref: String,
    confirm_create: bool,
) -> Result<VmmManagementOutcome, String> {
    let user_ref = user_ref.trim().to_string();
    if user_ref.is_empty() {
        return Err(String::from("用户引用不能为空。"));
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
    client: &TonicVmmClient,
    user_ref: String,
    confirmation_code: String,
) -> Result<VmmManagementOutcome, String> {
    let user_ref = user_ref.trim().to_string();
    if user_ref.is_empty() {
        return Err(String::from("用户引用不能为空。"));
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
    client: &TonicVmmClient,
    project_path: String,
    confirm_create: bool,
) -> Result<VmmManagementOutcome, String> {
    let project_path = project_path.trim().to_string();
    if project_path.is_empty() {
        return Err(String::from("项目路径不能为空。"));
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
    client: &TonicVmmClient,
    project_path: String,
    confirm_delete: bool,
) -> Result<VmmManagementOutcome, String> {
    let project_path = project_path.trim().to_string();
    if project_path.is_empty() {
        return Err(String::from("项目路径不能为空。"));
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
    client: &TonicVmmClient,
    source_project_path: String,
    target_project_path: String,
    confirm_migrate: bool,
) -> Result<VmmManagementOutcome, String> {
    let source_project_path = source_project_path.trim().to_string();
    let target_project_path = target_project_path.trim().to_string();
    if source_project_path.is_empty() || target_project_path.is_empty() {
        return Err(String::from("源项目路径和目标项目路径都不能为空。"));
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

/// Loads the configured profile bundle in combined mode.
/// 按组合模式读取当前配置的画像 bundle。
async fn run_vmm_profile_bundle_preview(
    client: &TonicVmmClient,
    effective: &VmmEffectiveConfig,
) -> Result<VmmManagementOutcome, String> {
    let user_id = effective
        .user_id
        .ok_or_else(|| String::from("VMM 用户 ID 为空，无法读取画像。"))?;
    let project_id = effective
        .project_id
        .ok_or_else(|| String::from("VMM 项目 ID 为空，无法读取画像。"))?;
    let response = client
        .get_profile_bundle(GetProfileBundleRequest {
            user_id,
            project_id,
            mode: ProfileBundleMode::ProfileBundleModeFull,
            include_explanation: Some(true),
        })
        .await
        .map_err(|error| error.to_string())?;
    Ok(VmmManagementOutcome::ProfileBundle(response))
}

/// Loads active profile nodes for one configured scope.
/// 读取一个已配置作用域的 active 画像节点。
async fn run_vmm_profile_nodes(
    client: &TonicVmmClient,
    effective: &VmmEffectiveConfig,
    target: ProfileTarget,
) -> Result<VmmManagementOutcome, String> {
    let user_id = effective
        .user_id
        .ok_or_else(|| String::from("VMM 用户 ID 为空，无法读取画像节点。"))?;
    let project_id = effective
        .project_id
        .ok_or_else(|| String::from("VMM 项目 ID 为空，无法读取画像节点。"))?;
    let response = client
        .get_profile_nodes(GetProfileNodesRequest {
            target,
            user_id,
            project_id,
            limit: 200,
        })
        .await
        .map_err(|error| error.to_string())?;
    Ok(VmmManagementOutcome::ProfileNodes(response))
}

/// Applies one manual profile instruction for one configured scope.
/// 为一个已配置作用域应用一条手工画像指令。
async fn run_vmm_apply_profile_instruction(
    client: &TonicVmmClient,
    effective: &VmmEffectiveConfig,
    target: ProfileTarget,
    instruction: String,
) -> Result<VmmManagementOutcome, String> {
    let instruction = instruction.trim().to_string();
    if instruction.is_empty() {
        return Err(String::from("手工画像指令不能为空。"));
    }
    let user_id = effective
        .user_id
        .ok_or_else(|| String::from("VMM 用户 ID 为空，无法应用画像指令。"))?;
    let project_id = effective
        .project_id
        .ok_or_else(|| String::from("VMM 项目 ID 为空，无法应用画像指令。"))?;
    let response = client
        .apply_profile_instruction(ApplyProfileInstructionRequest {
            target,
            user_id,
            project_id,
            instruction,
        })
        .await
        .map_err(|error| error.to_string())?;
    Ok(VmmManagementOutcome::ProfileInstruction(response))
}

/// Runs the memory-search AI-tool equivalent against the configured user/project.
/// 针对当前配置的用户与项目执行记忆检索 AI 工具等价调用。
async fn run_vmm_search_memory_events(
    client: &TonicVmmClient,
    effective: &VmmEffectiveConfig,
    queries: Vec<String>,
    top_k: u32,
) -> Result<VmmManagementOutcome, String> {
    let queries = queries
        .into_iter()
        .map(|query| query.trim().to_string())
        .filter(|query| !query.is_empty())
        .collect::<Vec<_>>();
    if queries.is_empty() {
        return Err(String::from("记忆检索查询不能为空。"));
    }
    let user_id = effective
        .user_id
        .ok_or_else(|| String::from("VMM 用户 ID 为空，无法执行记忆检索。"))?;
    let project_id = effective
        .project_id
        .ok_or_else(|| String::from("VMM 项目 ID 为空，无法执行记忆检索。"))?;
    let response = client
        .search_memory_events(SearchMemoryEventsRequest {
            user_id,
            project_id,
            queries,
            top_k: top_k.max(1),
        })
        .await
        .map_err(|error| error.to_string())?;
    Ok(VmmManagementOutcome::SearchMemoryEvents(response))
}

/// Runs the turn-detail AI-tool equivalent.
/// 执行 Turn 详情 AI 工具等价调用。
async fn run_vmm_get_turn_details(
    client: &TonicVmmClient,
    turn_ids: Vec<u64>,
) -> Result<VmmManagementOutcome, String> {
    if turn_ids.is_empty() {
        return Err(String::from("Turn ID 列表不能为空。"));
    }
    let response = client
        .get_turn_details(GetTurnDetailsRequest { turn_ids })
        .await
        .map_err(|error| error.to_string())?;
    Ok(VmmManagementOutcome::GetTurnDetails(response))
}

/// Runs the direct-write AI-tool equivalent against the configured user/project.
/// 针对当前配置的用户与项目执行主动写入 AI 工具等价调用。
async fn run_vmm_write_memories(
    client: &TonicVmmClient,
    effective: &VmmEffectiveConfig,
    session_id: String,
    item: WriteMemoryItem,
) -> Result<VmmManagementOutcome, String> {
    if item.abstract_text.trim().is_empty() && item.details.trim().is_empty() {
        return Err(String::from("主动写入摘要和详情不能同时为空。"));
    }
    let user_id = effective
        .user_id
        .ok_or_else(|| String::from("VMM 用户 ID 为空，无法主动写入记忆。"))?;
    let project_id = effective
        .project_id
        .ok_or_else(|| String::from("VMM 项目 ID 为空，无法主动写入记忆。"))?;
    let response = client
        .write_memories(WriteMemoriesRequest {
            session_id,
            user_id,
            project_id,
            items: vec![item],
        })
        .await
        .map_err(|error| error.to_string())?;
    Ok(VmmManagementOutcome::WriteMemories(response))
}
