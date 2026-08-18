use super::*;

/// Persists one session addon document without rewriting transcript or runtime-state documents.
/// 持久化单个会话附属状态文档，且不重写转录或运行态文档。
///
/// # Parameters
/// 参数
/// `controller` is the workspace data-plane controller handle.
/// `controller` 是工作区数据平面控制器句柄。
/// `task_projects` are the in-memory project summaries used to resolve session ownership.
/// `task_projects` 是用于解析会话归属的内存项目摘要。
/// `session` is the session summary that owns the addon state.
/// `session` 是拥有附属状态的会话摘要。
/// `archived` describes whether the session lives in the archived tree.
/// `archived` 描述该会话是否位于归档树。
/// `addon_state` is the full namespace-split addon document to write.
/// `addon_state` 是要写入的完整命名空间拆分附属状态文档。
///
/// # Returns
/// 返回值
/// The merged addon document that was written successfully.
/// 成功写入后的合并附属状态文档。
pub async fn persist_single_session_addon_state_update(
    controller: ControllerRuntimeHandle,
    task_projects: &[TaskProjectSummary],
    session: &SessionSummary,
    archived: bool,
    addon_state: &SessionAddonState,
) -> Result<SessionAddonState, String> {
    if addon_state.session_id != session.id {
        return Err(format!(
            "单会话附属状态保存失败：附属状态会话 `{}` 与目标会话 `{}` 不一致",
            addon_state.session_id, session.id
        ));
    }
    let session_owner_map = session_owner_map_from_projects(task_projects);
    let Some((project_id, _sort_order)) = session_owner_map.get(&session.id) else {
        return Err(format!(
            "单会话附属状态保存失败：未找到会话所属项目：{}",
            session.id
        ));
    };

    let session_id_text = session.id.to_string();
    let locator = segmented_session_locator_from_summary(session, project_id, archived);
    let session_dir =
        ensure_segmented_session_directory_for_locator(&controller, &session_id_text, &locator)?;
    let mut merged_state = read_session_addon_state(&session_dir)?
        .unwrap_or_else(|| SessionAddonState::new(session.id));
    for (namespace, value) in &addon_state.namespaces {
        let _ = merged_state.set_json_namespace(
            *namespace,
            value.value.clone(),
            value.updated_at.clone(),
        );
    }
    write_session_addon_state(&session_dir, &merged_state)?;
    Ok(merged_state)
}

/// Renames one authoritative active session without rebuilding or overwriting its locator identity.
/// 重命名一个权威活动会话，且不重建或覆盖其定位身份。
///
/// # Parameters
/// 参数
/// `controller` owns the authoritative SQLite index and workspace data root.
/// `controller` 持有权威 SQLite 索引与工作区数据根。
/// `session_id` is the exact active session identity selected by the user.
/// `session_id` 是用户选择的精确活动会话身份。
/// `title` is the normalized non-empty manual display title.
/// `title` 是规范化后的非空手动展示标题。
/// `updated_at` is the timestamp committed to metadata and the index.
/// `updated_at` 是提交到元数据与索引的时间戳。
///
/// # Returns
/// 返回值
/// Returns success only after exact-directory metadata and field-scoped SQLite title updates both succeed.
/// 仅在精确目录元数据与限定字段的 SQLite 标题更新均成功后返回成功。
pub async fn rename_single_session(
    controller: ControllerRuntimeHandle,
    session_id: SessionId,
    title: &str,
    updated_at: &str,
) -> Result<(), String> {
    if title.trim().is_empty() {
        return Err(String::from("会话标题不能为空。"));
    }
    // locator provides the only permitted project tag and creation timestamp for directory addressing.
    // locator 提供目录寻址唯一允许使用的项目标签与创建时间戳。
    let locator = query_session_index_locator(&controller, session_id, false)
        .await?
        .ok_or_else(|| format!("活动会话 `{session_id}` 不存在，拒绝重命名。"))?;
    // created_date is strictly derived from the persisted locator and never from a retained summary.
    // created_date 严格从持久化定位信息派生，绝不来自保留摘要。
    let created_date = session_directory_date_from_unix_millis_text(&locator.created_at)?;
    let _ = rename_segmented_session_by_created_date(
        &controller.data_root,
        &locator.project_tag,
        &created_date,
        &session_id.to_string(),
        title,
        updated_at,
    )?;
    update_active_session_index_title(&controller, session_id, title, updated_at).await
}

/// Resolved storage action for one idempotent session archive request.
/// 单次幂等会话归档请求解析出的存储动作。
enum SessionArchiveTransition {
    /// Active locator that still requires a directory and index transition.
    /// 仍需执行目录与索引转换的活动定位行。
    Required(PersistedSessionIndexLocator),
    /// Archived locator proving that an earlier request already committed the target state.
    /// 证明更早请求已经提交目标状态的归档定位行。
    AlreadyCommitted,
}

/// Resolves authoritative active and archived locator query results into one archive action.
/// 将权威活动与归档定位查询结果解析为单个归档动作。
///
/// # Parameters
/// 参数
/// `session_id` is the exact session identity being archived.
/// `session_id` 是正在归档的精确会话身份。
/// `active_locator` is the matching active row when the transition is still required.
/// `active_locator` 是仍需转换时匹配的活动行。
/// `archived_locator` is the matching archived row when the target state is already committed.
/// `archived_locator` 是目标状态已提交时匹配的归档行。
///
/// # Returns
/// 返回值
/// Returns the required transition, an idempotent committed result, or an explicit missing-session error.
/// 返回必需转换、幂等已提交结果或显式会话缺失错误。
fn resolve_session_archive_transition(
    session_id: SessionId,
    active_locator: Option<PersistedSessionIndexLocator>,
    archived_locator: Option<PersistedSessionIndexLocator>,
) -> Result<SessionArchiveTransition, String> {
    if let Some(locator) = active_locator {
        return Ok(SessionArchiveTransition::Required(locator));
    }
    if archived_locator.is_some() {
        return Ok(SessionArchiveTransition::AlreadyCommitted);
    }
    Err(format!("会话 `{session_id}` 不存在，拒绝归档。"))
}

/// Idempotently marks one authoritative session locator as archived without rewriting transcript or runtime-state documents.
/// 以幂等方式将一个权威会话定位行标记为已归档，而不重写转录或运行时状态文档。
///
/// # Parameters
/// 参数
/// `controller` owns the authoritative SQLite index and workspace data root.
/// `controller` 持有权威 SQLite 索引与工作区数据根。
/// `session_id` is the exact session identity to archive or verify as already archived.
/// `session_id` 是需要归档或验证为已归档的精确会话身份。
///
/// # Returns
/// 返回值
/// Returns success after committing an active locator or verifying that a concurrent request already archived it.
/// 活动定位行提交完成，或验证并发请求已将其归档后返回成功。
pub async fn archive_single_session(
    controller: ControllerRuntimeHandle,
    session_id: SessionId,
) -> Result<(), String> {
    // active_locator proves the session still needs its filesystem and index lifecycle transition.
    // active_locator 证明该会话仍需执行文件系统与索引生命周期转换。
    let active_locator = query_session_index_locator(&controller, session_id, false).await?;
    // archived_locator is queried only when no active row exists, distinguishing idempotency from absence.
    // archived_locator 仅在活动行缺失时查询，用于区分幂等完成与完全不存在。
    let archived_locator = if active_locator.is_none() {
        query_session_index_locator(&controller, session_id, true).await?
    } else {
        None
    };
    match resolve_session_archive_transition(session_id, active_locator, archived_locator)? {
        SessionArchiveTransition::Required(locator) => {
            archive_segmented_session_directory_for_locator(&controller, session_id, &locator).await
        }
        SessionArchiveTransition::AlreadyCommitted => Ok(()),
    }
}

/// Restores one archived session after validating its authoritative archived locator.
/// 在校验权威归档定位信息后恢复一个归档会话。
///
/// # Parameters
/// 参数
/// `controller` owns the authoritative SQLite index and workspace data root.
/// `controller` 持有权威 SQLite 索引与工作区数据根目录。
/// `session_id` is the exact archived session identity selected by the user.
/// `session_id` 是用户选中的精确归档会话身份。
///
/// # Returns
/// 返回值
/// Returns success after the archived directory and index state are restored with rollback support.
/// 归档目录与索引状态在支持回滚的流程中恢复后返回成功。
pub async fn restore_archived_session(
    controller: ControllerRuntimeHandle,
    session_id: SessionId,
) -> Result<(), String> {
    let locator = query_session_index_locator(&controller, session_id, true)
        .await?
        .ok_or_else(|| format!("归档会话 `{session_id}` 不存在，拒绝恢复。"))?;
    restore_segmented_session_directory_for_locator(&controller, session_id, &locator).await
}

/// Permanently deletes one archived session with a filesystem-first rollback boundary.
/// 以文件系统优先且可回滚的边界彻底删除一个归档会话。
///
/// # Parameters
/// 参数
/// `controller` owns the authoritative SQLite index and workspace data root.
/// `controller` 持有权威 SQLite 索引与工作区数据根目录。
/// `session_id` is the exact archived session identity selected by the user.
/// `session_id` 是用户选中的精确归档会话身份。
///
/// # Returns
/// 返回值
/// Returns success only after the exact archived directory and locator row are both permanently removed.
/// 仅在精确归档目录与定位索引行均被彻底移除后返回成功。
pub async fn delete_archived_session(
    controller: ControllerRuntimeHandle,
    session_id: SessionId,
) -> Result<(), String> {
    let locator = query_session_index_locator(&controller, session_id, true)
        .await?
        .ok_or_else(|| format!("归档会话 `{session_id}` 不存在，拒绝彻底删除。"))?;
    let created_date = session_directory_date_from_unix_millis_text(&locator.created_at)?;
    let archived_dir = segmented_session_directory(
        &controller.data_root,
        true,
        &locator.project_tag,
        &created_date,
        &session_id.to_string(),
    );
    let staged_dir = stage_archived_segmented_session_deletion(
        &controller.data_root,
        &locator.project_tag,
        &created_date,
        &session_id.to_string(),
    )?;
    if let Err(index_error) =
        delete_archived_session_index_row(&controller, session_id, &locator).await
    {
        let rollback_result = rollback_staged_archived_session_deletion(&staged_dir, &archived_dir);
        return match rollback_result {
            Ok(()) => Err(format!(
                "会话 `{session_id}` 的彻底删除索引提交失败，目录已回滚：{index_error}"
            )),
            Err(rollback_error) => Err(format!(
                "会话 `{session_id}` 的彻底删除索引提交失败：{index_error}；目录回滚也失败：{rollback_error}"
            )),
        };
    }
    finalize_staged_archived_session_deletion(&staged_dir)
        .map_err(|error| format!("会话 `{session_id}` 的索引已删除，但归档目录清理失败：{error}"))
}

/// Removes one stale session locator when both lifecycle directories are physically absent.
/// 当活动与归档生命周期目录均被物理删除时移除一条陈旧会话定位行。
///
/// # Parameters
/// 参数
/// `controller` owns the authoritative session index and workspace data root.
/// `controller` 持有权威会话索引与工作区数据根目录。
/// `session_id` is the exact session identity being checked.
/// `session_id` 是正在检查的精确会话身份。
///
/// # Returns
/// 返回值
/// Returns `true` only when a physically missing session row was deleted, and `false` when the row is present and its directory is valid or no row remains.
/// 仅当物理缺失会话的索引行被删除时返回 `true`；索引行有效或已不存在时返回 `false`。
pub async fn cleanup_missing_session_index(
    controller: ControllerRuntimeHandle,
    session_id: SessionId,
) -> Result<bool, String> {
    let active_locator = query_session_index_locator(&controller, session_id, false).await?;
    let archived_locator = query_session_index_locator(&controller, session_id, true).await?;
    let locator = match (active_locator, archived_locator) {
        (Some(_), Some(_)) => {
            return Err(format!(
                "会话 `{session_id}` 同时存在活动与归档定位索引，无法清理。"
            ));
        }
        (Some(locator), None) | (None, Some(locator)) => locator,
        (None, None) => return Ok(false),
    };
    let created_date = session_directory_date_from_unix_millis_text(&locator.created_at)?;
    // expected_dir is the only directory consistent with the authoritative lifecycle flag.
    // expected_dir 是唯一符合权威生命周期标记的目录。
    let expected_dir = segmented_session_directory(
        &controller.data_root,
        locator.archived,
        &locator.project_tag,
        &created_date,
        &session_id.to_string(),
    );
    // opposite_dir distinguishes a missing session from an interrupted lifecycle move.
    // opposite_dir 用于区分物理缺失会话与生命周期移动中断。
    let opposite_dir = segmented_session_directory(
        &controller.data_root,
        !locator.archived,
        &locator.project_tag,
        &created_date,
        &session_id.to_string(),
    );
    let expected_exists = segmented_session_directory_exists(&expected_dir)?;
    let opposite_exists = segmented_session_directory_exists(&opposite_dir)?;
    match (expected_exists, opposite_exists) {
        (true, false) => Ok(false),
        (false, true) => Err(format!(
            "会话 `{session_id}` 的权威索引与生命周期目录不一致，拒绝将中断移动视为物理删除。"
        )),
        (true, true) => Err(format!(
            "会话 `{session_id}` 同时存在活动与归档目录，拒绝清理。"
        )),
        (false, false) => {
            delete_session_index_row_exact(&controller, session_id, &locator).await?;
            Ok(true)
        }
    }
}

/// Archives every active session locator owned by one project while keeping the project active.
/// 归档单个项目拥有的全部活动会话定位行，同时保持项目为活动状态。
///
/// # Parameters
/// 参数
/// `controller` owns the authoritative SQLite index and workspace data root.
/// `controller` 持有权威 SQLite 索引与工作区数据根。
/// `project_id` is the exact active project identity whose sessions must be archived.
/// `project_id` 是必须归档其会话的精确活动项目身份。
///
/// # Returns
/// 返回值
/// Returns success only after the project is verified active and no active child locator remains.
/// 仅在项目验证为活动状态且不存在任何活动子定位行后返回成功。
pub async fn archive_project_sessions(
    controller: ControllerRuntimeHandle,
    project_id: &str,
) -> Result<(), String> {
    if project_id.trim().is_empty() {
        return Err(String::from("活动项目标识不能为空。"));
    }
    if !project_archive_state_exists(&controller, project_id, false).await? {
        return Err(format!(
            "活动项目 `{project_id}` 不存在，拒绝归档项目会话。"
        ));
    }

    // session_entries are the complete authoritative active child set from SQLite.
    // session_entries 是来自 SQLite 的完整权威活动子会话集合。
    let session_entries =
        query_session_index_locators_for_project(&controller, project_id, false).await?;
    for entry in session_entries {
        archive_segmented_session_directory_for_locator(
            &controller,
            entry.session_id,
            &entry.locator,
        )
        .await?;
    }

    // remaining_entries detect a concurrent or partial writer before the caller mutates the project row.
    // remaining_entries 在调用方变更项目行前检测并发或局部写入者。
    let remaining_entries =
        query_session_index_locators_for_project(&controller, project_id, false).await?;
    if !remaining_entries.is_empty() {
        return Err(format!(
            "项目 `{project_id}` 归档后仍包含 {} 个活动会话定位行。",
            remaining_entries.len()
        ));
    }
    Ok(())
}

/// Archives every authoritative project session and removes the active project row without deleting archived data.
/// 归档项目的全部权威会话并移除活动项目行，但不删除归档数据。
///
/// # Parameters
/// 参数
/// `controller` owns the authoritative SQLite index and workspace data root.
/// `controller` 持有权威 SQLite 索引与工作区数据根。
/// `project_id` is the exact active project identity to remove from the visible tree.
/// `project_id` 是需要从可见树移除的精确活动项目身份。
///
/// # Returns
/// 返回值
/// Returns success only after child locators are archived, their VMM documents are deleted, and the active project row is deleted.
/// 仅在子定位行归档、其 VMM 文档删除且活动项目行删除完成后返回成功。
pub async fn remove_project_after_archiving_sessions(
    controller: ControllerRuntimeHandle,
    project_id: &str,
) -> Result<(), String> {
    archive_project_sessions(controller.clone(), project_id).await?;
    // archived_entries are queried after archival so retries can finish VMM deletion after a partial prior attempt.
    // archived_entries 在归档后查询，使重试能够完成先前局部尝试遗留的 VMM 删除。
    let archived_entries =
        query_session_index_locators_for_project(&controller, project_id, true).await?;
    for entry in archived_entries {
        // removed is intentionally ignored because an already-absent VMM document is an idempotent deletion.
        // removed 被刻意忽略，因为已经缺失的 VMM 文档属于幂等删除。
        let _removed = crate::workspace_store::delete_core_session_vmm_state(
            &controller.data_root,
            entry.session_id,
        )?;
    }
    delete_active_project_row(&controller, project_id).await?;
    Ok(())
}

/// Restores one archived project and every authoritative archived child locator into the visible tree.
/// 将单个归档项目及其全部权威归档子定位行恢复到可见树。
///
/// # Parameters
/// 参数
/// `controller` owns the authoritative SQLite index and workspace data root.
/// `controller` 持有权威 SQLite 索引与工作区数据根。
/// `project_id` is the exact archived project identity to restore.
/// `project_id` 是需要恢复的精确归档项目身份。
///
/// # Returns
/// 返回值
/// Returns success only after no archived child locator remains and the existing project row is reactivated.
/// 仅在不存在任何归档子定位行且既有项目行重新激活后返回成功。
pub async fn restore_project(
    controller: ControllerRuntimeHandle,
    project_id: &str,
) -> Result<(), String> {
    if project_id.trim().is_empty() {
        return Err(String::from("归档项目标识不能为空。"));
    }
    if !project_archive_state_exists(&controller, project_id, true).await? {
        return Err(format!("归档项目 `{project_id}` 不存在，拒绝恢复。"));
    }

    // session_entries are the complete authoritative archived child set from SQLite.
    // session_entries 是来自 SQLite 的完整权威归档子会话集合。
    let session_entries =
        query_session_index_locators_for_project(&controller, project_id, true).await?;
    for entry in session_entries {
        restore_segmented_session_directory_for_locator(
            &controller,
            entry.session_id,
            &entry.locator,
        )
        .await?;
    }

    // remaining_entries stop project reactivation when a concurrent or partial writer leaves archived children.
    // remaining_entries 在并发或局部写入者留下归档子会话时阻止项目重新激活。
    let remaining_entries =
        query_session_index_locators_for_project(&controller, project_id, true).await?;
    if !remaining_entries.is_empty() {
        return Err(format!(
            "项目 `{project_id}` 恢复后仍包含 {} 个归档会话定位行。",
            remaining_entries.len()
        ));
    }
    update_project_archive_flag(&controller, project_id, true, false).await?;
    Ok(())
}

/// Archives one session directory and index row from an authoritative active locator.
/// 根据权威活动定位行归档一个会话目录与索引行。
///
/// # Parameters
/// 参数
/// `controller` owns the workspace data root and SQLite index.
/// `controller` 持有工作区数据根与 SQLite 索引。
/// `session_id` is the stable session identity parsed from SQLite.
/// `session_id` 是从 SQLite 解析出的稳定会话身份。
/// `locator` is the exact active filesystem locator returned by SQLite.
/// `locator` 是 SQLite 返回的精确活动文件系统定位信息。
///
/// # Returns
/// 返回值
/// Returns success after directory publication and the archive-state index update both succeed.
/// 在目录发布与索引归档状态更新均成功后返回成功。
async fn archive_segmented_session_directory_for_locator(
    controller: &ControllerRuntimeHandle,
    session_id: SessionId,
    locator: &PersistedSessionIndexLocator,
) -> Result<(), String> {
    if locator.archived {
        return Err(format!(
            "会话 `{session_id}` 的定位行已归档，拒绝重复执行活动目录归档。"
        ));
    }
    let archived_at = current_unix_millis_text();
    let created_date = session_directory_date_from_unix_millis_text(&locator.created_at)?;
    let _ = archive_segmented_session_by_created_date(
        &controller.data_root,
        &locator.project_tag,
        &created_date,
        &session_id.to_string(),
        &archived_at,
    )?;
    if let Err(commit_error) =
        update_session_index_archive_state(controller, session_id, locator, true, &archived_at)
            .await
    {
        let rollback_result = restore_segmented_session_by_created_date(
            &controller.data_root,
            &locator.project_tag,
            &created_date,
            &session_id.to_string(),
            &locator.updated_at,
        );
        return match rollback_result {
            Ok(_) => Err(format!(
                "会话 `{session_id}` 的归档索引提交失败，目录已回滚：{commit_error}"
            )),
            Err(rollback_error) => Err(format!(
                "会话 `{session_id}` 的归档索引提交失败：{commit_error}；目录回滚也失败：{rollback_error}"
            )),
        };
    }
    Ok(())
}

/// Restores one session directory and index row from an authoritative archived locator.
/// 根据权威归档定位行恢复一个会话目录与索引行。
///
/// # Parameters
/// 参数
/// `controller` owns the workspace data root and SQLite index.
/// `controller` 持有工作区数据根与 SQLite 索引。
/// `session_id` is the stable session identity parsed from SQLite.
/// `session_id` 是从 SQLite 解析出的稳定会话身份。
/// `locator` is the exact archived filesystem locator returned by SQLite.
/// `locator` 是 SQLite 返回的精确归档文件系统定位信息。
///
/// # Returns
/// 返回值
/// Returns success after active-directory publication and the index-state update both succeed.
/// 在活动目录发布与索引状态更新均成功后返回成功。
async fn restore_segmented_session_directory_for_locator(
    controller: &ControllerRuntimeHandle,
    session_id: SessionId,
    locator: &PersistedSessionIndexLocator,
) -> Result<(), String> {
    if !locator.archived {
        return Err(format!(
            "会话 `{session_id}` 的定位行未归档，拒绝重复执行归档目录恢复。"
        ));
    }
    let restored_at = current_unix_millis_text();
    let created_date = session_directory_date_from_unix_millis_text(&locator.created_at)?;
    let _ = restore_segmented_session_by_created_date(
        &controller.data_root,
        &locator.project_tag,
        &created_date,
        &session_id.to_string(),
        &restored_at,
    )?;
    if let Err(commit_error) =
        update_session_index_archive_state(controller, session_id, locator, false, &restored_at)
            .await
    {
        let rollback_result = archive_segmented_session_by_created_date(
            &controller.data_root,
            &locator.project_tag,
            &created_date,
            &session_id.to_string(),
            &locator.updated_at,
        );
        return match rollback_result {
            Ok(_) => Err(format!(
                "会话 `{session_id}` 的恢复索引提交失败，目录已回滚：{commit_error}"
            )),
            Err(rollback_error) => Err(format!(
                "会话 `{session_id}` 的恢复索引提交失败：{commit_error}；目录回滚也失败：{rollback_error}"
            )),
        };
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds one exact lifecycle locator for archive-transition resolution tests.
    /// 为归档转换解析测试构建一个精确生命周期定位行。
    ///
    /// # Parameters
    /// 参数
    /// `archived` is the persisted lifecycle state represented by the locator.
    /// `archived` 是定位行表示的持久化生命周期状态。
    ///
    /// # Returns
    /// 返回值
    /// Returns a complete deterministic locator with the requested archive state.
    /// 返回具有所需归档状态的完整确定性定位行。
    fn archive_transition_test_locator(archived: bool) -> PersistedSessionIndexLocator {
        PersistedSessionIndexLocator {
            project_id: String::from("project-a"),
            project_tag: String::from("project-a"),
            created_at: String::from("1783669000000"),
            updated_at: String::from("1783669001000"),
            archived,
        }
    }

    /// Verifies duplicate archive requests accept an already committed locator while missing sessions still fail.
    /// 验证重复归档请求接受已提交定位行，而完全缺失的会话仍然失败。
    #[test]
    fn session_archive_transition_is_idempotent_after_commit() {
        // session_id is shared by every lifecycle lookup outcome under test.
        // session_id 由测试中的每种生命周期查询结果共享。
        let session_id = SessionId::from_u128(0x51);
        // required proves an active row still selects the physical transition path.
        // required 证明活动行仍会选择物理转换路径。
        let required = resolve_session_archive_transition(
            session_id,
            Some(archive_transition_test_locator(false)),
            None,
        )
        .expect("active locator should require archive transition");
        // committed proves a prior archive is accepted as the idempotent target state.
        // committed 证明先前归档会被接受为幂等目标状态。
        let committed = resolve_session_archive_transition(
            session_id,
            None,
            Some(archive_transition_test_locator(true)),
        )
        .expect("archived locator should be idempotent success");
        // missing_error proves idempotency does not hide an unknown session identity.
        // missing_error 证明幂等语义不会掩盖未知会话身份。
        let missing_error = resolve_session_archive_transition(session_id, None, None)
            .err()
            .expect("missing session should fail");

        assert!(matches!(required, SessionArchiveTransition::Required(_)));
        assert!(matches!(
            committed,
            SessionArchiveTransition::AlreadyCommitted
        ));
        assert!(missing_error.contains(&session_id.to_string()));
    }
}
