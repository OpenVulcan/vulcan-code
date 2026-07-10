use super::*;

/// Builds a segmented-session locator directly from the in-memory session summary.
/// 直接从内存会话摘要构建分段会话定位信息。
pub(super) fn segmented_session_locator_from_summary(
    session: &SessionSummary,
    project_id: &str,
    archived: bool,
) -> SegmentedSessionLocator {
    SegmentedSessionLocator {
        project_id: project_id.to_string(),
        title: session.title.clone(),
        title_source: session.title_source.as_persisted_key().to_string(),
        archived,
        created_at: normalized_session_created_at(session),
    }
}

/// Resolves or creates the segmented directory described by one persisted session locator.
/// 解析或创建一条持久化会话定位信息描述的分段目录。
pub(super) fn ensure_segmented_session_directory_for_locator(
    controller: &ControllerRuntimeHandle,
    session_id_text: &str,
    locator: &SegmentedSessionLocator,
) -> Result<std::path::PathBuf, String> {
    match find_segmented_session_directory(
        &controller.data_root,
        locator.archived,
        &locator.project_id,
        session_id_text,
    )? {
        Some(session_dir) => Ok(session_dir),
        None => create_segmented_session_in_location(
            &controller.data_root,
            locator.archived,
            SegmentedSessionCreateRequest {
                session_id: session_id_text.to_string(),
                project_tag: locator.project_id.clone(),
                created_date: session_directory_date_from_unix_millis_text(&locator.created_at)?,
                created_at: locator.created_at.clone(),
                project_id: Some(locator.project_id.clone()),
                title: Some(locator.title.clone()),
                title_source: Some(locator.title_source.clone()),
                segment_policy: SegmentPolicy::default(),
            },
        ),
    }
}

/// Stores normalized locator data after SQLite scalar decoding.
/// 保存 SQLite 标量解码后的规范化定位数据。
pub(super) struct SegmentedSessionLocator {
    /// Stable owning project identifier used as the project tag.
    /// 作为项目标签使用的稳定所属项目标识。
    project_id: String,
    /// Visible session title mirrored into metadata when the segmented directory is first created.
    /// 首次创建分段目录时镜像到 metadata 的可见会话标题。
    title: String,
    /// Persisted title-source key mirrored into metadata.
    /// 镜像到 metadata 的持久化标题来源键。
    title_source: String,
    /// Whether the segmented directory belongs under archived_sessions.
    /// 分段目录是否属于 archived_sessions。
    archived: bool,
    /// Creation timestamp stored as Unix milliseconds text.
    /// 以 Unix 毫秒文本存储的创建时间。
    pub(super) created_at: String,
}
