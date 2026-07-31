use crate::client::VmmClientError;
use crate::grpc::{
    PostActionTimelineItem as GrpcPostActionTimelineItem,
    ProfileBundleMode as GrpcProfileBundleMode, ProfileNodeEntry as GrpcProfileNodeEntry,
    ProfileNodeSourceKind as GrpcProfileNodeSourceKind, ProfileTarget as GrpcProfileTarget,
    ProjectEntry as GrpcProjectEntry, RetiredProfileNodeEntry as GrpcRetiredProfileNodeEntry,
    UserEntry as GrpcUserEntry,
};
use crate::protocol::{
    ApplyProfileInstructionResponse, ContextItem, DeleteMemoriesResponse, DeleteProjectResponse,
    DeleteUserResponse, EnsureProjectResponse, GetProfileBundleResponse, GetProfileNodesResponse,
    GetTurnDetailsResponse, ListProjectsResponse, ListUsersResponse, MemorySearchGroupResult,
    MemorySearchHit, MigrateProjectResponse, PostActionTimelineItem, PostActionTimelineType,
    PreCheckResponse, ProfileBundleMode, ProfileNodeEntry, ProfileNodeSourceKind, ProfileTarget,
    ProjectEntry, ResolveProjectResponse, ResolveUserResponse, RetiredProfileNodeEntry,
    SearchMemoryEventsResponse, SessionMemoryStatus, TurnDetailEntry, UserEntry,
    WriteMemoriesResponse, WriteMemoryResultItem,
};

/// Converts a generated protobuf profile-bundle mode into protocol enum.
/// 将生成的 protobuf 画像 bundle 模式转换为协议枚举。
fn protocol_profile_bundle_mode(mode: i32) -> ProfileBundleMode {
    match GrpcProfileBundleMode::try_from(mode).unwrap_or(GrpcProfileBundleMode::Unspecified) {
        GrpcProfileBundleMode::Full => ProfileBundleMode::ProfileBundleModeFull,
        GrpcProfileBundleMode::Split => ProfileBundleMode::ProfileBundleModeSplit,
        GrpcProfileBundleMode::Unspecified => ProfileBundleMode::ProfileBundleModeUnspecified,
    }
}

/// Converts a generated protobuf profile target into protocol enum.
/// 将生成的 protobuf 画像目标转换为协议枚举。
fn protocol_profile_target(target: i32) -> ProfileTarget {
    match GrpcProfileTarget::try_from(target).unwrap_or(GrpcProfileTarget::Unspecified) {
        GrpcProfileTarget::User => ProfileTarget::ProfileTargetUser,
        GrpcProfileTarget::Project => ProfileTarget::ProfileTargetProject,
        GrpcProfileTarget::Team => ProfileTarget::ProfileTargetTeam,
        GrpcProfileTarget::Space => ProfileTarget::ProfileTargetSpace,
        GrpcProfileTarget::All => ProfileTarget::ProfileTargetAll,
        GrpcProfileTarget::Unspecified => ProfileTarget::ProfileTargetUnspecified,
    }
}

/// Converts a generated protobuf profile-node source into protocol enum.
/// 将生成的 protobuf 画像节点来源转换为协议枚举。
fn protocol_profile_node_source_kind(source_kind: i32) -> ProfileNodeSourceKind {
    match GrpcProfileNodeSourceKind::try_from(source_kind)
        .unwrap_or(GrpcProfileNodeSourceKind::Unspecified)
    {
        GrpcProfileNodeSourceKind::TurnExtract => {
            ProfileNodeSourceKind::ProfileNodeSourceKindTurnExtract
        }
        GrpcProfileNodeSourceKind::ManualInstruction => {
            ProfileNodeSourceKind::ProfileNodeSourceKindManualInstruction
        }
        GrpcProfileNodeSourceKind::SystemSeed => {
            ProfileNodeSourceKind::ProfileNodeSourceKindSystemSeed
        }
        GrpcProfileNodeSourceKind::RetainedAfterUserDelete => {
            ProfileNodeSourceKind::ProfileNodeSourceKindRetainedAfterUserDelete
        }
        GrpcProfileNodeSourceKind::Unspecified => {
            ProfileNodeSourceKind::ProfileNodeSourceKindUnspecified
        }
    }
}

/// Converts a generated protobuf project entry into protocol shape.
/// 将生成的 protobuf 项目记录转换为协议结构。
fn protocol_project_entry(entry: GrpcProjectEntry) -> ProjectEntry {
    ProjectEntry {
        project_id: entry.project_id,
        team_id: entry.team_id,
        space_id: entry.space_id,
        team_name: entry.team_name,
        space_name: entry.space_name,
        project_name: entry.project_name,
        display_path: entry.display_path,
    }
}

/// Converts an optional generated protobuf project entry into protocol shape.
/// 将可选生成 protobuf 项目记录转换为协议结构。
fn protocol_optional_project_entry(entry: Option<GrpcProjectEntry>) -> Option<ProjectEntry> {
    entry.map(protocol_project_entry)
}

/// Converts a generated protobuf user entry into protocol shape.
/// 将生成的 protobuf 用户记录转换为协议结构。
fn protocol_user_entry(entry: GrpcUserEntry) -> UserEntry {
    UserEntry {
        user_id: entry.user_id,
        user_name: entry.user_name,
    }
}

/// Converts an optional generated protobuf user entry into protocol shape.
/// 将可选生成 protobuf 用户记录转换为协议结构。
fn protocol_optional_user_entry(entry: Option<GrpcUserEntry>) -> Option<UserEntry> {
    entry.map(protocol_user_entry)
}

/// Converts generated protobuf ListProjects response into protocol shape.
/// 将生成的 protobuf ListProjects 响应转换为协议结构。
pub(super) fn protocol_list_projects_response(
    response: crate::grpc::ListProjectsResponse,
) -> ListProjectsResponse {
    ListProjectsResponse {
        projects: response
            .projects
            .into_iter()
            .map(protocol_project_entry)
            .collect(),
        trace_id: response.trace_id,
    }
}

/// Converts generated protobuf ResolveProject response into protocol shape.
/// 将生成的 protobuf ResolveProject 响应转换为协议结构。
pub(super) fn protocol_resolve_project_response(
    response: crate::grpc::ResolveProjectResponse,
) -> ResolveProjectResponse {
    ResolveProjectResponse {
        project: protocol_optional_project_entry(response.project),
        message: response.message,
        trace_id: response.trace_id,
    }
}

/// Converts generated protobuf EnsureProject response into protocol shape.
/// 将生成的 protobuf EnsureProject 响应转换为协议结构。
pub(super) fn protocol_ensure_project_response(
    response: crate::grpc::EnsureProjectResponse,
) -> EnsureProjectResponse {
    EnsureProjectResponse {
        project: protocol_optional_project_entry(response.project),
        message: response.message,
        exists: response.exists,
        needs_confirm: response.needs_confirm,
        created_team: response.created_team,
        created_space: response.created_space,
        created_project: response.created_project,
        missing_team: response.missing_team,
        missing_space: response.missing_space,
        trace_id: response.trace_id,
    }
}

/// Converts generated protobuf DeleteProject response into protocol shape.
/// 将生成的 protobuf DeleteProject 响应转换为协议结构。
pub(super) fn protocol_delete_project_response(
    response: crate::grpc::DeleteProjectResponse,
) -> DeleteProjectResponse {
    DeleteProjectResponse {
        project: protocol_optional_project_entry(response.project),
        message: response.message,
        needs_confirm: response.needs_confirm,
        deleted_sessions: response.deleted_sessions,
        deleted_messages: response.deleted_messages,
        deleted_memories: response.deleted_memories,
        deleted_vector_rows: response.deleted_vector_rows,
        trace_id: response.trace_id,
        deleted_projects: response.deleted_projects,
        deleted_spaces: response.deleted_spaces,
        deleted_teams: response.deleted_teams,
        deleted_profiles: response.deleted_profiles,
    }
}

/// Converts generated protobuf MigrateProject response into protocol shape.
/// 将生成的 protobuf MigrateProject 响应转换为协议结构。
pub(super) fn protocol_migrate_project_response(
    response: crate::grpc::MigrateProjectResponse,
) -> MigrateProjectResponse {
    MigrateProjectResponse {
        source_project: protocol_optional_project_entry(response.source_project),
        target_project: protocol_optional_project_entry(response.target_project),
        message: response.message,
        needs_confirm: response.needs_confirm,
        migrated_sessions: response.migrated_sessions,
        migrated_messages: response.migrated_messages,
        migrated_memories: response.migrated_memories,
        rebuilt_vector_rows: response.rebuilt_vector_rows,
        trace_id: response.trace_id,
    }
}

/// Converts generated protobuf ListUsers response into protocol shape.
/// 将生成的 protobuf ListUsers 响应转换为协议结构。
pub(super) fn protocol_list_users_response(
    response: crate::grpc::ListUsersResponse,
) -> ListUsersResponse {
    ListUsersResponse {
        users: response
            .users
            .into_iter()
            .map(protocol_user_entry)
            .collect(),
        trace_id: response.trace_id,
    }
}

/// Converts generated protobuf DeleteUser response into protocol shape.
/// 将生成的 protobuf DeleteUser 响应转换为协议结构。
pub(super) fn protocol_delete_user_response(
    response: crate::grpc::DeleteUserResponse,
) -> DeleteUserResponse {
    DeleteUserResponse {
        user: protocol_optional_user_entry(response.user),
        message: response.message,
        requires_confirmation: response.requires_confirmation,
        confirmation_code: response.confirmation_code,
        deleted_sessions: response.deleted_sessions,
        deleted_messages: response.deleted_messages,
        deleted_memories: response.deleted_memories,
        deleted_vector_rows: response.deleted_vector_rows,
        trace_id: response.trace_id,
        deleted_users: response.deleted_users,
        deleted_profiles: response.deleted_profiles,
    }
}

/// Converts generated protobuf GetProfileNodes response into protocol shape.
/// 将生成的 protobuf GetProfileNodes 响应转换为协议结构。
pub(super) fn protocol_get_profile_nodes_response(
    response: crate::grpc::GetProfileNodesResponse,
) -> GetProfileNodesResponse {
    GetProfileNodesResponse {
        nodes: response
            .nodes
            .into_iter()
            .map(protocol_profile_node_entry)
            .collect(),
        trace_id: response.trace_id,
    }
}

/// Converts generated protobuf ResolveUser response into protocol shape.
/// 将生成的 protobuf ResolveUser 响应转换为协议结构。
pub(super) fn protocol_resolve_user_response(
    response: crate::grpc::ResolveUserResponse,
) -> ResolveUserResponse {
    ResolveUserResponse {
        user: protocol_optional_user_entry(response.user),
        message: response.message,
        created: response.created,
        exists: response.exists,
        trace_id: response.trace_id,
    }
}

/// Converts generated protobuf ApplyProfileInstruction response into protocol shape.
/// 将生成的 protobuf ApplyProfileInstruction 响应转换为协议结构。
pub(super) fn protocol_apply_profile_instruction_response(
    response: crate::grpc::ApplyProfileInstructionResponse,
) -> ApplyProfileInstructionResponse {
    ApplyProfileInstructionResponse {
        instruction_id: response.instruction_id,
        accepted_nodes: response
            .accepted_nodes
            .into_iter()
            .map(protocol_profile_node_entry)
            .collect(),
        retired_nodes: response
            .retired_nodes
            .into_iter()
            .map(protocol_retired_profile_node_entry)
            .collect(),
        review_reason: response.review_reason,
        trace_id: response.trace_id,
    }
}

/// Converts a generated protobuf profile node into protocol shape.
/// 将生成的 protobuf 画像节点转换为协议结构。
fn protocol_profile_node_entry(entry: GrpcProfileNodeEntry) -> ProfileNodeEntry {
    ProfileNodeEntry {
        profile_node_id: entry.profile_node_id,
        target: protocol_profile_target(entry.target),
        bind_id: entry.bind_id,
        content: entry.content,
        priority: entry.priority,
        level: entry.level,
        refresh_weight: entry.refresh_weight,
        profile_date: entry.profile_date,
        expires_timestamp: entry.expires_timestamp,
        level_reason: entry.level_reason,
        source_kind: protocol_profile_node_source_kind(entry.source_kind),
        source_id: entry.source_id,
    }
}

/// Converts a generated protobuf retired profile node into protocol shape.
/// 将生成的 protobuf 退役画像节点转换为协议结构。
fn protocol_retired_profile_node_entry(
    entry: GrpcRetiredProfileNodeEntry,
) -> RetiredProfileNodeEntry {
    RetiredProfileNodeEntry {
        profile_node_id: entry.profile_node_id,
        reason: entry.reason,
    }
}

/// Converts generated protobuf GetProfileBundle response into protocol shape.
/// 将生成的 protobuf GetProfileBundle 响应转换为协议结构。
pub(super) fn protocol_get_profile_bundle_response(
    response: crate::grpc::GetProfileBundleResponse,
) -> GetProfileBundleResponse {
    GetProfileBundleResponse {
        mode: protocol_profile_bundle_mode(response.mode),
        include_explanation: response.include_explanation,
        explanation_text: response.explanation_text,
        environment_priority_text: response.environment_priority_text,
        combined_text: response.combined_text,
        team_profile: response.team_profile,
        space_profile: response.space_profile,
        project_profile: response.project_profile,
        user_profile: response.user_profile,
        trace_id: response.trace_id,
    }
}

/// Converts generated protobuf PreCheck response into protocol shape.
/// 将生成的 protobuf PreCheck 响应转换为协议结构。
pub(super) fn protocol_precheck_response(
    response: crate::grpc::PreCheckResponse,
) -> Result<PreCheckResponse, VmmClientError> {
    Ok(PreCheckResponse {
        should_inject: response.should_inject,
        context_items: response
            .context_items
            .into_iter()
            .map(|item| ContextItem {
                text: item.text,
                score: item.score,
                turn_id: item.turn_id,
                has_dialogue: item.has_dialogue,
                created_datetime: item.created_datetime,
                memory_id: item.memory_id,
            })
            .collect(),
        degraded: response.degraded,
        trace_id: response.trace_id,
        session_memory_status: protocol_session_memory_status(response.session_memory_status)?,
    })
}

/// Converts one generated lifecycle integer and rejects unspecified or unknown protocol values.
/// 将生成的生命周期整数转换为协议枚举，并拒绝未指定或未知的协议值。
pub(super) fn protocol_session_memory_status(
    status: i32,
) -> Result<SessionMemoryStatus, VmmClientError> {
    match crate::grpc::SessionMemoryStatus::try_from(status) {
        Ok(crate::grpc::SessionMemoryStatus::Active) => Ok(SessionMemoryStatus::Active),
        Ok(crate::grpc::SessionMemoryStatus::Archived) => Ok(SessionMemoryStatus::Archived),
        Ok(crate::grpc::SessionMemoryStatus::Recycled) => Ok(SessionMemoryStatus::Recycled),
        Ok(crate::grpc::SessionMemoryStatus::Forgotten) => Ok(SessionMemoryStatus::Forgotten),
        Ok(crate::grpc::SessionMemoryStatus::Unspecified) | Err(_) => Err(VmmClientError::new(
            "incompatible_session_memory_status",
            format!("VMM returned unsupported session memory status {status}"),
        )),
    }
}

/// Converts generated protobuf SearchMemoryEvents response into protocol shape.
/// 将生成的 protobuf SearchMemoryEvents 响应转换为协议结构。
pub(super) fn protocol_search_memory_events_response(
    response: crate::grpc::SearchMemoryEventsResponse,
) -> SearchMemoryEventsResponse {
    SearchMemoryEventsResponse {
        results: response
            .results
            .into_iter()
            .map(|group| MemorySearchGroupResult {
                query_index: group.query_index,
                query: group.query,
                hits: group
                    .hits
                    .into_iter()
                    .map(|hit| MemorySearchHit {
                        memory_id: hit.memory_id,
                        source_turn_id: hit.source_turn_id,
                        abstract_text: hit.r#abstract,
                        details_preview: hit.details_preview,
                        category: hit.category,
                        created_datetime: hit.created_datetime,
                    })
                    .collect(),
            })
            .collect(),
        trace_id: response.trace_id,
    }
}

/// Converts generated protobuf GetTurnDetails response into protocol shape.
/// 将生成的 protobuf GetTurnDetails 响应转换为协议结构。
pub(super) fn protocol_get_turn_details_response(
    response: crate::grpc::GetTurnDetailsResponse,
) -> GetTurnDetailsResponse {
    GetTurnDetailsResponse {
        turns: response
            .turns
            .into_iter()
            .map(|turn| TurnDetailEntry {
                turn_id: turn.turn_id,
                user_question: turn.user_question,
                timeline: turn
                    .timeline
                    .into_iter()
                    .map(protocol_timeline_item)
                    .collect(),
                assistant_answer: turn.assistant_answer,
                detail: turn.detail,
                previous_turn_ids: turn.previous_turn_ids,
                next_turn_ids: turn.next_turn_ids,
            })
            .collect(),
        trace_id: response.trace_id,
    }
}

/// Converts generated protobuf WriteMemories response into protocol shape.
/// 将生成的 protobuf WriteMemories 响应转换为协议结构。
pub(super) fn protocol_write_memories_response(
    response: crate::grpc::WriteMemoriesResponse,
) -> WriteMemoriesResponse {
    WriteMemoriesResponse {
        items: response
            .items
            .into_iter()
            .map(|item| WriteMemoryResultItem {
                memory_id: item.memory_id,
                deduped: item.deduped,
            })
            .collect(),
        trace_id: response.trace_id,
    }
}

/// Converts generated protobuf DeleteMemories response into protocol shape.
/// 将生成的 protobuf DeleteMemories 响应转换为协议结构。
pub(super) fn protocol_delete_memories_response(
    response: crate::grpc::DeleteMemoriesResponse,
) -> DeleteMemoriesResponse {
    DeleteMemoriesResponse {
        deleted_memory_ids: response.deleted_memory_ids,
        not_found_memory_ids: response.not_found_memory_ids,
        deleted_vector_rows: response.deleted_vector_rows,
        trace_id: response.trace_id,
    }
}

/// Converts one generated protobuf timeline item into protocol shape.
/// 将单条生成的 protobuf timeline 项转换为协议结构。
fn protocol_timeline_item(item: GrpcPostActionTimelineItem) -> PostActionTimelineItem {
    let kind = if item.r#type.eq_ignore_ascii_case("assistant") {
        PostActionTimelineType::Assistant
    } else {
        PostActionTimelineType::User
    };
    PostActionTimelineItem {
        kind,
        content: item.content,
    }
}
