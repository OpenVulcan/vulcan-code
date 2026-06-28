//! VMM health, project, and user management protocol models.
//! VMM 健康检查、项目和用户管理协议模型。

use serde::{Deserialize, Serialize};

/// Health check response returned by VMM.
/// VMM 返回的健康检查响应。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct HealthzResponse {
    /// Human-readable service status.
    /// 人类可读的服务状态。
    #[serde(default)]
    pub status: String,
}

/// One canonical VMM project row returned by management calls.
/// 管理调用返回的一条标准 VMM 项目记录。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectEntry {
    /// Numeric VMM project identifier.
    /// VMM 数字项目标识。
    pub project_id: u64,
    /// Numeric VMM team identifier.
    /// VMM 数字团队标识。
    pub team_id: u64,
    /// Numeric VMM space identifier.
    /// VMM 数字空间标识。
    pub space_id: u64,
    /// Team name used in the canonical display path.
    /// 标准展示路径中的团队名称。
    #[serde(default)]
    pub team_name: String,
    /// Space name used in the canonical display path.
    /// 标准展示路径中的空间名称。
    #[serde(default)]
    pub space_name: String,
    /// Project name used in the canonical display path.
    /// 标准展示路径中的项目名称。
    #[serde(default)]
    pub project_name: String,
    /// Human-readable Team/Space/Project display path.
    /// 人类可读的 Team/Space/Project 展示路径。
    #[serde(default)]
    pub display_path: String,
}

/// One durable VMM user row returned by management calls.
/// 管理调用返回的一条长期 VMM 用户记录。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct UserEntry {
    /// Numeric VMM user identifier.
    /// VMM 数字用户标识。
    pub user_id: u64,
    /// Durable VMM user name.
    /// 长期 VMM 用户名。
    #[serde(default)]
    pub user_name: String,
}

/// Response body for VMM ListProjects.
/// VMM ListProjects 响应体。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListProjectsResponse {
    /// Project entries returned by VMM.
    /// VMM 返回的项目记录集合。
    #[serde(default)]
    pub projects: Vec<ProjectEntry>,
    /// Trace id returned by VMM.
    /// VMM 返回的追踪标识。
    #[serde(default)]
    pub trace_id: String,
}

/// Request body for VMM ResolveProject.
/// VMM ResolveProject 请求体。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolveProjectRequest {
    /// Numeric id or canonical Team/Space/Project path.
    /// 数字 ID 或标准 Team/Space/Project 路径。
    pub project_ref: String,
}

/// Response body for VMM ResolveProject.
/// VMM ResolveProject 响应体。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolveProjectResponse {
    /// Resolved project entry when present.
    /// 解析成功时返回的项目记录。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<ProjectEntry>,
    /// Stable service message.
    /// 稳定服务消息。
    #[serde(default)]
    pub message: String,
    /// Trace id returned by VMM.
    /// VMM 返回的追踪标识。
    #[serde(default)]
    pub trace_id: String,
}

/// Request body for VMM EnsureProject.
/// VMM EnsureProject 请求体。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnsureProjectRequest {
    /// Canonical Team/Space/Project path to resolve or create.
    /// 需要解析或创建的标准 Team/Space/Project 路径。
    pub project_path: String,
    /// Whether missing nodes may be created.
    /// 是否允许创建缺失节点。
    pub confirm_create: bool,
}

/// Response body for VMM EnsureProject.
/// VMM EnsureProject 响应体。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnsureProjectResponse {
    /// Resolved or created project entry when present.
    /// 解析或创建成功时返回的项目记录。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<ProjectEntry>,
    /// Stable service message.
    /// 稳定服务消息。
    #[serde(default)]
    pub message: String,
    /// Whether the project path already existed.
    /// 项目路径是否已存在。
    pub exists: bool,
    /// Whether another call with confirmation is required.
    /// 是否需要再次确认后调用。
    pub needs_confirm: bool,
    /// Whether the team row was created.
    /// 是否创建了团队记录。
    pub created_team: bool,
    /// Whether the space row was created.
    /// 是否创建了空间记录。
    pub created_space: bool,
    /// Whether the project row was created.
    /// 是否创建了项目记录。
    pub created_project: bool,
    /// Whether the referenced team was missing.
    /// 引用的团队是否缺失。
    pub missing_team: bool,
    /// Whether the referenced space was missing.
    /// 引用的空间是否缺失。
    pub missing_space: bool,
    /// Trace id returned by VMM.
    /// VMM 返回的追踪标识。
    #[serde(default)]
    pub trace_id: String,
}

/// Request body for VMM DeleteProject.
/// VMM DeleteProject 请求体。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeleteProjectRequest {
    /// Canonical Team/Space/Project path to delete.
    /// 需要删除的标准 Team/Space/Project 路径。
    pub project_path: String,
    /// Whether deletion has been explicitly confirmed.
    /// 删除是否已经显式确认。
    pub confirm_delete: bool,
}

/// Response body for VMM DeleteProject.
/// VMM DeleteProject 响应体。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeleteProjectResponse {
    /// Deleted project entry when the target was resolved.
    /// 目标解析成功时对应的被删除项目记录。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<ProjectEntry>,
    /// Stable service message.
    /// 稳定服务消息。
    #[serde(default)]
    pub message: String,
    /// Whether a confirmed retry is required.
    /// 是否需要确认后再次调用。
    pub needs_confirm: bool,
    /// Deleted session row count.
    /// 已删除会话行数。
    pub deleted_sessions: i64,
    /// Deleted message row count.
    /// 已删除消息行数。
    pub deleted_messages: i64,
    /// Deleted memory row count.
    /// 已删除记忆行数。
    pub deleted_memories: i64,
    /// Deleted vector row count.
    /// 已删除向量行数。
    pub deleted_vector_rows: u64,
    /// Trace id returned by VMM.
    /// VMM 返回的追踪标识。
    #[serde(default)]
    pub trace_id: String,
    /// Deleted project row count.
    /// 已删除项目行数。
    pub deleted_projects: i64,
    /// Deleted space row count.
    /// 已删除空间行数。
    pub deleted_spaces: i64,
    /// Deleted team row count.
    /// 已删除团队行数。
    pub deleted_teams: i64,
    /// Deleted profile row count.
    /// 已删除画像行数。
    pub deleted_profiles: i64,
}

/// Request body for VMM MigrateProject.
/// VMM MigrateProject 请求体。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MigrateProjectRequest {
    /// Source canonical Team/Space/Project path.
    /// 源标准 Team/Space/Project 路径。
    pub source_project_path: String,
    /// Target canonical Team/Space/Project path.
    /// 目标标准 Team/Space/Project 路径。
    pub target_project_path: String,
    /// Whether migration has been explicitly confirmed.
    /// 迁移是否已经显式确认。
    pub confirm_migrate: bool,
}

/// Response body for VMM MigrateProject.
/// VMM MigrateProject 响应体。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MigrateProjectResponse {
    /// Source project entry when resolved.
    /// 解析成功时的源项目记录。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_project: Option<ProjectEntry>,
    /// Target project entry when resolved.
    /// 解析成功时的目标项目记录。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_project: Option<ProjectEntry>,
    /// Stable service message.
    /// 稳定服务消息。
    #[serde(default)]
    pub message: String,
    /// Whether a confirmed retry is required.
    /// 是否需要确认后再次调用。
    pub needs_confirm: bool,
    /// Migrated session row count.
    /// 已迁移会话行数。
    pub migrated_sessions: i64,
    /// Migrated message row count.
    /// 已迁移消息行数。
    pub migrated_messages: i64,
    /// Migrated memory row count.
    /// 已迁移记忆行数。
    pub migrated_memories: i64,
    /// Rebuilt vector row count.
    /// 已重建向量行数。
    pub rebuilt_vector_rows: i64,
    /// Trace id returned by VMM.
    /// VMM 返回的追踪标识。
    #[serde(default)]
    pub trace_id: String,
}

/// Request body for VMM ResolveUser.
/// VMM ResolveUser 请求体。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolveUserRequest {
    /// Numeric id or durable user name.
    /// 数字 ID 或长期用户名。
    pub user_ref: String,
    /// Whether a missing user may be created.
    /// 是否允许创建缺失用户。
    pub confirm_create: bool,
}

/// Response body for VMM ResolveUser.
/// VMM ResolveUser 响应体。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolveUserResponse {
    /// Resolved or created user entry when present.
    /// 解析或创建成功时返回的用户记录。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user: Option<UserEntry>,
    /// Stable service message.
    /// 稳定服务消息。
    #[serde(default)]
    pub message: String,
    /// Whether the user row was created.
    /// 是否创建了用户记录。
    pub created: bool,
    /// Whether the user row already existed.
    /// 用户记录是否已存在。
    pub exists: bool,
    /// Trace id returned by VMM.
    /// VMM 返回的追踪标识。
    #[serde(default)]
    pub trace_id: String,
}

/// Response body for VMM ListUsers.
/// VMM ListUsers 响应体。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListUsersResponse {
    /// User entries returned by VMM.
    /// VMM 返回的用户记录集合。
    #[serde(default)]
    pub users: Vec<UserEntry>,
    /// Trace id returned by VMM.
    /// VMM 返回的追踪标识。
    #[serde(default)]
    pub trace_id: String,
}

/// Request body for VMM DeleteUser.
/// VMM DeleteUser 请求体。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeleteUserRequest {
    /// Numeric id or durable user name.
    /// 数字 ID 或长期用户名。
    pub user_ref: String,
    /// Confirmation code returned by the first guarded call.
    /// 首次受保护调用返回的确认码。
    #[serde(default)]
    pub confirmation_code: String,
}

/// Response body for VMM DeleteUser.
/// VMM DeleteUser 响应体。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeleteUserResponse {
    /// Deleted user entry when the target was resolved.
    /// 目标解析成功时对应的被删除用户记录。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user: Option<UserEntry>,
    /// Stable service message.
    /// 稳定服务消息。
    #[serde(default)]
    pub message: String,
    /// Whether another call with the returned code is required.
    /// 是否需要携带返回确认码再次调用。
    pub requires_confirmation: bool,
    /// Confirmation code generated by VMM.
    /// VMM 生成的确认码。
    #[serde(default)]
    pub confirmation_code: String,
    /// Deleted session row count.
    /// 已删除会话行数。
    pub deleted_sessions: i64,
    /// Deleted message row count.
    /// 已删除消息行数。
    pub deleted_messages: i64,
    /// Deleted memory row count.
    /// 已删除记忆行数。
    pub deleted_memories: i64,
    /// Deleted vector row count.
    /// 已删除向量行数。
    pub deleted_vector_rows: u64,
    /// Trace id returned by VMM.
    /// VMM 返回的追踪标识。
    #[serde(default)]
    pub trace_id: String,
    /// Deleted user row count.
    /// 已删除用户行数。
    pub deleted_users: i64,
    /// Deleted profile row count.
    /// 已删除画像行数。
    pub deleted_profiles: i64,
}
