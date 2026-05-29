//! VMM management RPC helpers used by settings surfaces.
//! 设置界面使用的 VMM 管理 RPC 辅助逻辑。

use vulcan_vmm::{
    EnsureProjectRequest, GetProfileBundleRequest, ProfileBundleMode, ResolveProjectRequest,
    ResolveUserRequest, TonicVmmClient, VmmClient, VmmConfigLayer, VmmEffectiveConfig,
    VmmTransportConfig,
};

/// One user-triggered VMM management action.
/// 一种由用户触发的 VMM 管理动作。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum VmmManagementAction {
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

impl VmmManagementAction {
    /// Returns the visible action label.
    /// 返回可见动作标签。
    pub(super) const fn label(self) -> &'static str {
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
pub(super) async fn run_vmm_management_action(
    action: VmmManagementAction,
    layer: VmmConfigLayer,
    management_user_ref: String,
    management_project_ref: String,
) -> Result<String, String> {
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

/// Runs the VMM health check management call.
/// 执行 VMM 健康检查管理调用。
async fn run_vmm_healthz(client: &TonicVmmClient) -> Result<String, String> {
    let response = client.healthz().await.map_err(|error| error.to_string())?;
    Ok(format!("VMM 连接正常：{}", response.status))
}

/// Lists users and projects in a compact settings notice.
/// 以紧凑设置提示列出用户与项目。
async fn run_vmm_list_users_and_projects(client: &TonicVmmClient) -> Result<String, String> {
    let users = client
        .list_users()
        .await
        .map_err(|error| error.to_string())?;
    let projects = client
        .list_projects()
        .await
        .map_err(|error| error.to_string())?;
    let user_preview = users
        .users
        .iter()
        .take(5)
        .map(|user| format!("{}:{}", user.user_id, user.user_name))
        .collect::<Vec<_>>()
        .join(", ");
    let project_preview = projects
        .projects
        .iter()
        .take(5)
        .map(|project| format!("{}:{}", project.project_id, project.display_path))
        .collect::<Vec<_>>()
        .join(", ");
    Ok(format!(
        "用户 {} 个 [{}]；项目 {} 个 [{}]。",
        users.users.len(),
        user_preview,
        projects.projects.len(),
        project_preview
    ))
}

/// Resolves configured user and project references.
/// 解析当前配置中的用户与项目引用。
async fn run_vmm_resolve_user_and_project(
    client: &TonicVmmClient,
    effective: &VmmEffectiveConfig,
    management_user_ref: &str,
    management_project_ref: &str,
) -> Result<String, String> {
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
    let user_text = user
        .user
        .map(|entry| format!("{}:{}", entry.user_id, entry.user_name))
        .unwrap_or_else(|| user.message);
    let project_text = project
        .project
        .map(|entry| format!("{}:{}", entry.project_id, entry.display_path))
        .unwrap_or_else(|| project.message);
    Ok(format!("用户 {user_text}；项目 {project_text}。"))
}

/// Ensures the configured project path without allowing creation.
/// 在不允许创建的前提下确认当前项目路径。
async fn run_vmm_ensure_project_preview(
    client: &TonicVmmClient,
    management_project_ref: &str,
) -> Result<String, String> {
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
    let project_text = response
        .project
        .map(|entry| format!("{}:{}", entry.project_id, entry.display_path))
        .unwrap_or_else(|| response.message.clone());
    Ok(format!(
        "项目预检：{project_text}；exists={}；needs_confirm={}。",
        response.exists, response.needs_confirm
    ))
}

/// Loads the configured profile bundle in combined mode.
/// 按组合模式读取当前配置的画像 bundle。
async fn run_vmm_profile_bundle_preview(
    client: &TonicVmmClient,
    effective: &VmmEffectiveConfig,
) -> Result<String, String> {
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
    Ok(format!(
        "画像 bundle 已读取：combined={} 字符，team={}，space={}，project={}，user={}。",
        response.combined_text.chars().count(),
        response.team_profile.chars().count(),
        response.space_profile.chars().count(),
        response.project_profile.chars().count(),
        response.user_profile.chars().count()
    ))
}
