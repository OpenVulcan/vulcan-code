use super::*;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// Persisted rollback result written before replaying an edited prompt.
/// 在重放编辑后提示词之前写入的持久化回滚结果。
#[derive(Debug)]
pub struct PersistedUserPromptRollbackResult {
    /// Retained transcript messages before the edited user prompt.
    /// 编辑用户提示词之前保留的转录消息。
    pub retained_messages: Vec<SessionMessageEntry>,
    /// Runtime state rebuilt from the retained canonical conversation state.
    /// 从保留的规范会话状态重建出的运行态。
    pub runtime_state: SessionRuntimeStateEntry,
    /// Transaction backup that restores the original session until the edited submission commits.
    /// 在编辑后提交确认前用于恢复原会话的事务备份。
    rollback_backup: PersistedSessionEditRollbackBackup,
}

impl PersistedUserPromptRollbackResult {
    /// Takes the rewritten transcript and runtime state for one in-memory atomic workspace commit.
    /// 取出重写后的转录与运行态，用于一次内存工作区原子提交。
    ///
    /// # Returns
    /// 返回值
    /// Returns owned rewritten state while retaining the disk rollback transaction guard.
    /// 返回自有重写状态，同时保留磁盘回滚事务守卫。
    pub fn take_rewritten_state(&mut self) -> (Vec<SessionMessageEntry>, SessionRuntimeStateEntry) {
        (
            std::mem::take(&mut self.retained_messages),
            std::mem::take(&mut self.runtime_state),
        )
    }

    /// Commits the destructive edit by deleting its transaction backup.
    /// 通过删除事务备份提交本次破坏性编辑。
    ///
    /// # Returns
    /// 返回值
    /// Returns success after the backup directory is removed.
    /// 在备份目录删除后返回成功。
    pub fn commit(mut self) -> Result<(), String> {
        self.rollback_backup.commit()
    }

    /// Restores the exact persisted session tree captured before destructive editing.
    /// 恢复破坏性编辑前捕获的完整持久化会话目录树。
    ///
    /// # Returns
    /// 返回值
    /// Returns success after the original session directory is restored.
    /// 在原始会话目录恢复后返回成功。
    pub fn restore(mut self) -> Result<(), String> {
        self.rollback_backup.restore()
    }
}

/// Transaction backup for one destructive persisted-session edit.
/// 单次破坏性持久化会话编辑的事务备份。
#[derive(Debug)]
struct PersistedSessionEditRollbackBackup {
    /// Authoritative session directory currently being edited.
    /// 当前正在编辑的权威会话目录。
    session_dir: PathBuf,
    /// Exact pre-edit copy used for rollback.
    /// 用于回滚的编辑前完整副本。
    backup_dir: PathBuf,
    /// Transaction root kept outside the session catalog scan tree.
    /// 位于会话目录扫描树之外的事务根目录。
    backup_root: PathBuf,
    /// Exact relative files that may be mutated by the history rewrite.
    /// 历史重写可能修改的精确相对文件集合。
    mutable_relative_paths: Vec<PathBuf>,
    /// Exact relative files that existed and were copied before the rewrite.
    /// 重写前实际存在并已复制的精确相对文件集合。
    backed_up_relative_paths: Vec<PathBuf>,
    /// Whether drop must still restore the original directory.
    /// Drop 时是否仍需恢复原始目录。
    restore_on_drop: bool,
}

impl PersistedSessionEditRollbackBackup {
    /// Copies the complete session tree before any destructive edit write occurs.
    /// 在任何破坏性编辑写入发生前复制完整会话目录树。
    ///
    /// # Parameters
    /// 参数
    /// `session_dir` is the authoritative persisted session directory.
    /// `session_dir` 是权威持久化会话目录。
    ///
    /// # Returns
    /// 返回值
    /// Returns an armed rollback backup whose drop path restores the original tree.
    /// 返回一个已启用回滚的备份，其 Drop 路径会恢复原目录树。
    fn create(session_dir: &Path) -> Result<Self, String> {
        let sessions_root = session_dir
            .parent()
            .and_then(Path::parent)
            .ok_or_else(|| String::from("ERR_USER_PROMPT_EDIT_BACKUP_SESSIONS_ROOT_MISSING"))?;
        let data_root = sessions_root
            .parent()
            .ok_or_else(|| String::from("ERR_USER_PROMPT_EDIT_BACKUP_DATA_ROOT_MISSING"))?;
        let backup_root = data_root.join(".user-prompt-edit-backups");
        let directory_name = session_dir
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| String::from("ERR_USER_PROMPT_EDIT_BACKUP_NAME_INVALID"))?;
        let backup_dir = backup_root.join(format!("{directory_name}-{}", Uuid::new_v4()));
        let metadata = read_session_metadata(session_dir)?;
        let mutable_relative_paths = history_rewrite_mutable_relative_paths(&metadata);
        let mut backed_up_relative_paths = Vec::new();
        let backup_result = (|| {
            std::fs::create_dir_all(&backup_dir).map_err(to_store_error)?;
            for relative_path in &mutable_relative_paths {
                let source_path = session_dir.join(relative_path);
                let target_path = backup_dir.join(relative_path);
                match std::fs::metadata(&source_path) {
                    Ok(file_metadata) if file_metadata.is_file() => {
                        if let Some(parent) = target_path.parent() {
                            std::fs::create_dir_all(parent).map_err(to_store_error)?;
                        }
                        std::fs::copy(&source_path, &target_path).map_err(to_store_error)?;
                        backed_up_relative_paths.push(relative_path.clone());
                    }
                    Ok(_) => {
                        return Err(format!(
                            "ERR_USER_PROMPT_EDIT_BACKUP_TARGET_NOT_FILE: {}",
                            source_path.display()
                        ));
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => return Err(to_store_error(error)),
                }
            }
            Ok::<(), String>(())
        })();
        if let Err(error) = backup_result {
            let _ = std::fs::remove_dir_all(&backup_dir);
            remove_empty_edit_backup_root(&backup_root);
            return Err(format!(
                "ERR_USER_PROMPT_EDIT_BACKUP_CREATE_FAILED: {error}"
            ));
        }
        Ok(Self {
            session_dir: session_dir.to_path_buf(),
            backup_dir,
            backup_root,
            mutable_relative_paths,
            backed_up_relative_paths,
            restore_on_drop: true,
        })
    }

    /// Commits the edit and disarms automatic restoration.
    /// 提交编辑并解除自动恢复。
    ///
    /// # Returns
    /// 返回值
    /// Returns success after the backup is deleted.
    /// 在备份删除后返回成功。
    fn commit(&mut self) -> Result<(), String> {
        // An accepted edited submission owns the rewritten session even when stale backup cleanup fails.
        // 编辑后的提交一旦被接受，即使陈旧备份清理失败，也应继续拥有已改写会话。
        self.restore_on_drop = false;
        if self.backup_dir.exists() {
            std::fs::remove_dir_all(&self.backup_dir).map_err(to_store_error)?;
        }
        remove_empty_edit_backup_root(&self.backup_root);
        Ok(())
    }

    /// Restores the pre-edit directory with a rename-based swap.
    /// 通过基于重命名的交换恢复编辑前目录。
    ///
    /// # Returns
    /// 返回值
    /// Returns success after the backup becomes the authoritative session directory.
    /// 在备份重新成为权威会话目录后返回成功。
    fn restore(&mut self) -> Result<(), String> {
        if !self.restore_on_drop {
            return Ok(());
        }
        let mut paths_to_remove = self.mutable_relative_paths.clone();
        if let Ok(current_metadata) = read_session_metadata(&self.session_dir) {
            for current_path in history_rewrite_mutable_relative_paths(&current_metadata) {
                if !paths_to_remove.iter().any(|path| path == &current_path) {
                    paths_to_remove.push(current_path);
                }
            }
        }
        for relative_path in paths_to_remove {
            let path = self.session_dir.join(relative_path);
            match std::fs::remove_file(&path) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(to_store_error(error)),
            }
        }
        for relative_path in &self.backed_up_relative_paths {
            let source_path = self.backup_dir.join(relative_path);
            let target_path = self.session_dir.join(relative_path);
            if let Some(parent) = target_path.parent() {
                std::fs::create_dir_all(parent).map_err(to_store_error)?;
            }
            std::fs::copy(&source_path, &target_path).map_err(to_store_error)?;
        }
        self.restore_on_drop = false;
        if self.backup_dir.exists() {
            std::fs::remove_dir_all(&self.backup_dir).map_err(to_store_error)?;
        }
        remove_empty_edit_backup_root(&self.backup_root);
        Ok(())
    }
}

/// Removes an empty edit-transaction root without disturbing other active backups.
/// 删除空的编辑事务根目录，同时不影响其他活动备份。
///
/// # Parameters
/// 参数
/// `backup_root` is the transaction directory outside the session catalog tree.
/// `backup_root` 是位于会话目录树之外的事务目录。
fn remove_empty_edit_backup_root(backup_root: &Path) {
    if backup_root.exists() {
        let _ = std::fs::remove_dir(backup_root);
    }
}

impl Drop for PersistedSessionEditRollbackBackup {
    /// Restores the original session if submission exits before commit.
    /// 如果提交在确认前退出，则恢复原始会话。
    fn drop(&mut self) {
        if self.restore_on_drop {
            let _ = self.restore();
        }
    }
}

/// Fork result returned after copying one persisted segmented session.
/// 复制单个持久化分段会话后返回的分叉结果。
#[derive(Debug, Clone)]
pub struct PersistedSessionForkResult {
    /// Newly created session summary.
    /// 新创建的会话摘要。
    pub session: SessionSummary,
}

/// Rolls one persisted session back to the point before the selected user prompt.
/// 将一个持久化会话回滚到所选用户提示词之前。
pub async fn rollback_persisted_session_before_user_prompt(
    controller: &ControllerRuntimeHandle,
    session: &SessionSummary,
    project_id: &str,
    message_id: &str,
    runtime_state: SessionRuntimeStateEntry,
) -> Result<PersistedUserPromptRollbackResult, String> {
    let target = locate_persisted_user_prompt_target(
        controller,
        session,
        project_id,
        message_id,
        runtime_state,
    )?;
    let mut rollback_backup = PersistedSessionEditRollbackBackup::create(&target.session_dir)?;
    let rollback_result = (|| {
        truncate_segmented_session_before_message(
            &target.session_dir,
            message_id,
            &target.retained_messages,
        )?;
        let runtime_state = rebuild_runtime_state_for_user_prompt_edit(
            target.runtime_state,
            target.retained_conversation_state,
            &target.retained_record_ids,
            target.edit_cutoff_at_ms,
            target.last_compaction_id,
            target.context_start_message_id,
        )?;
        reset_vmm_state_for_history_rewrite(&target.session_dir)?;
        let state_document =
            state_document_from_runtime_state(&session.id.to_string(), &runtime_state)?;
        write_state(&target.session_dir, &state_document)?;
        rebuild_runtime_log_from_conversation_state(
            &target.session_dir,
            &runtime_state.conversation_state,
            &runtime_state.updated_at,
        )?;
        Ok::<SessionRuntimeStateEntry, String>(runtime_state)
    })();
    match rollback_result {
        Ok(runtime_state) => Ok(PersistedUserPromptRollbackResult {
            retained_messages: target.retained_messages,
            runtime_state,
            rollback_backup,
        }),
        Err(error) => match rollback_backup.restore() {
            Ok(()) => Err(error),
            Err(restore_error) => Err(format!(
                "{error}; ERR_USER_PROMPT_EDIT_ROLLBACK_RESTORE_FAILED: {restore_error}"
            )),
        },
    }
}

/// Rebuilds history-bound runtime fields for an edit while preserving session configuration and durable accounting.
/// 为编辑重建历史绑定的运行态字段，同时保留会话配置与可靠记账事实。
///
/// # Parameters
/// 参数
/// `runtime_state` is the complete persisted state before the destructive edit.
/// `runtime_state` 是破坏性编辑前的完整持久化状态。
/// `retained_conversation_state` is the canonical prefix before the edited user record.
/// `retained_conversation_state` 是被编辑用户记录之前的规范前缀。
/// `retained_record_ids` identifies records that remain on the active branch.
/// `retained_record_ids` 标识仍属于活动分支的记录。
/// `edit_cutoff_at_ms` is the exact edited-record timestamp.
/// `edit_cutoff_at_ms` 是被编辑记录的精确时间戳。
/// `last_compaction_id` and `context_start_message_id` come from the retained replay.
/// `last_compaction_id` 与 `context_start_message_id` 来自保留后的重放结果。
///
/// # Returns
/// 返回值
/// Returns a new edit generation with stale execution ownership removed.
/// 返回移除过期执行所有权的新编辑代际。
fn rebuild_runtime_state_for_user_prompt_edit(
    mut runtime_state: SessionRuntimeStateEntry,
    mut retained_conversation_state: SessionConversationState,
    retained_record_ids: &BTreeSet<String>,
    edit_cutoff_at_ms: i64,
    last_compaction_id: Option<String>,
    context_start_message_id: Option<String>,
) -> Result<SessionRuntimeStateEntry, String> {
    let edit_generation = runtime_state
        .provider_call_ledger
        .active_branch_generation
        .checked_add(1)
        .ok_or_else(|| String::from("ERR_USER_PROMPT_EDIT_BRANCH_GENERATION_EXHAUSTED"))?;
    if runtime_state.provider_call_ledger.revision == u64::MAX {
        return Err(String::from(
            "ERR_USER_PROMPT_EDIT_LEDGER_REVISION_EXHAUSTED",
        ));
    }
    let _ = runtime_state
        .provider_call_ledger
        .supersede_outside_record_boundary(
            retained_record_ids,
            edit_cutoff_at_ms,
            edit_generation,
            format!("edit_context_generation:{edit_generation}"),
        );
    retained_conversation_state.continuation = Default::default();
    runtime_state.conversation_state = retained_conversation_state;
    runtime_state.last_compaction_id = last_compaction_id;
    runtime_state.context_start_message_id = context_start_message_id;
    runtime_state.live_context_budget_fingerprint = None;
    runtime_state.live_context_assembly_fingerprint = None;
    runtime_state.last_provider_request_preview_markdown = None;
    runtime_state.context_injection_state = Default::default();
    runtime_state.todolist = Default::default();
    runtime_state.plan_mode = Default::default();
    runtime_state.vmm_writeback_outbox.clear();
    runtime_state.vmm_profile_sync = Default::default();
    runtime_state.auto_compaction_consecutive_failures = 0;
    runtime_state.tool_call_record_ids.clear();
    runtime_state.tool_request_record_ids.clear();
    runtime_state.active_precheck_turn_id = None;
    runtime_state.session_runtime_kernel = None;
    runtime_state
        .ui_state
        .collapsed_message_ids
        .retain(|record_id| retained_record_ids.contains(record_id));
    runtime_state.ui_state.pending_prompt_queue.clear();
    runtime_state.ui_state.pending_prompt_panel_queue.clear();
    runtime_state.updated_at = current_unix_millis_text();
    Ok(runtime_state)
}

/// Forks a persisted segmented session by copying its backing directory and indexing the clone.
/// 通过复制底层目录并写入索引来分叉一个持久化分段会话。
pub async fn fork_persisted_session(
    controller: &ControllerRuntimeHandle,
    source_session: &SessionSummary,
    project_id: &str,
    target_session_id: SessionId,
    target_title: String,
    anchor_message_id: Option<&str>,
) -> Result<PersistedSessionForkResult, String> {
    let now = current_unix_millis_text();
    let target_session_id_text = target_session_id.to_string();
    let anchor_message_id = anchor_message_id
        .map(str::trim)
        .filter(|message_id| !message_id.is_empty());
    let source_dir = locate_segmented_session_dir_for_summary(
        controller,
        source_session,
        project_id,
        "ERR_SESSION_FORK_SOURCE_NOT_FOUND",
    )?;
    let _ = repair_segmented_session_display_if_needed(&source_dir)?;
    let source_replay = replay_segmented_session_context_without_repair(&source_dir)?;
    let source_provider_call_ledger = provider_call_ledger_from_state_document(&source_dir)?;
    let source_anchor_index = anchor_message_id
        .map(|message_id| {
            let replay = source_replay
                .as_ref()
                .ok_or_else(|| String::from("ERR_SESSION_FORK_EMPTY_SESSION"))?;
            fork_anchor_record_index_in_replay(replay, message_id)
        })
        .transpose()?;
    let source_last_visible_index = source_replay
        .as_ref()
        .and_then(last_visible_record_index_in_replay);
    let source_retained_record_count = source_replay
        .as_ref()
        .map(|replay| {
            source_anchor_index
                .map(|anchor_index| fork_retained_record_count(replay, anchor_index))
                .unwrap_or(replay.conversation_state.records.len())
        })
        .unwrap_or(0);
    let truncate_after_anchor = source_replay.as_ref().is_some_and(|replay| {
        source_retained_record_count < replay.conversation_state.records.len()
    });
    let inherited_call_cutoff_at_ms = source_anchor_index
        .and_then(|record_index| {
            source_replay
                .as_ref()
                .and_then(|replay| replay.conversation_state.records.get(record_index))
        })
        .map(|record| {
            record
                .created_at
                .parse::<i64>()
                .map_err(|_| String::from("ERR_SESSION_FORK_ANCHOR_TIMESTAMP_INVALID"))
        })
        .transpose()?
        .unwrap_or(i64::MAX);
    let retained_source_record_ids = source_replay
        .as_ref()
        .map(|replay| {
            replay
                .conversation_state
                .records
                .iter()
                .take(source_retained_record_count)
                .map(|record| record.id.clone())
                .collect::<BTreeSet<_>>()
        })
        .unwrap_or_default();
    let identity_replacements =
        forked_identity_replacements_for_persisted_session(source_session.id, target_session_id);
    let source_anchor_message_id_for_display = anchor_message_id
        .filter(|_| truncate_after_anchor)
        .map(ToOwned::to_owned);
    let target_dir = fork_segmented_session_directory_from_source(
        &source_dir,
        &controller.data_root,
        &SegmentedSessionForkRequest {
            source_archived: false,
            source_project_tag: project_id.to_string(),
            source_session_id: source_session.id.to_string(),
            target_project_tag: project_id.to_string(),
            target_session_id: target_session_id_text.clone(),
            target_created_date: session_directory_date_from_unix_millis_text(&now)?,
            target_created_at: now.clone(),
            target_title: target_title.clone(),
            target_title_source: source_session.title_source.as_persisted_key().to_string(),
            source_anchor_message_id: source_anchor_message_id_for_display,
        },
    )?;
    let target_anchor_message_id = anchor_message_id.map(|message_id| {
        forked_anchor_message_id(message_id, source_session.id, target_session_id)
    });
    let fork_source_anchor_message_id = target_anchor_message_id.clone().or_else(|| {
        source_last_visible_index
            .and_then(|record_index| {
                source_replay
                    .as_ref()
                    .and_then(|replay| replay.conversation_state.records.get(record_index))
            })
            .map(|record| {
                forked_anchor_message_id(&record.id, source_session.id, target_session_id)
            })
    });
    let fork_result = async {
        if truncate_after_anchor && target_anchor_message_id.is_some() {
            let replay = source_replay
                .as_ref()
                .ok_or_else(|| String::from("ERR_SESSION_FORK_EMPTY_SESSION"))?;
            rebuild_forked_persisted_session_runtime_from_source_replay(
                &target_dir,
                &target_session_id_text,
                replay,
                source_retained_record_count,
                &identity_replacements,
            )?;
        }
        persist_forked_provider_call_ledger(
            &target_dir,
            source_provider_call_ledger.clone(),
            &retained_source_record_ids,
            inherited_call_cutoff_at_ms,
            truncate_after_anchor,
            &identity_replacements,
            format!("fork_context_generation:{target_session_id_text}"),
        )?;
        record_persisted_session_fork_source_marker(
            &target_dir,
            source_session,
            project_id,
            fork_source_anchor_message_id,
        )?;
        let forked_session = SessionSummary::new(
            target_session_id,
            Some(project_id.to_string()),
            target_title,
            source_session.focus.clone(),
            SessionState::Completed,
            source_session.changed_files,
            source_session.queued_steps,
            "",
        )
        .with_title_source(source_session.title_source)
        .with_created_at(now.clone())
        .with_updated_at(now.clone());
        upsert_session_index_row(
            controller,
            &forked_session,
            project_id,
            false,
            0,
            Some(&now),
        )
        .await?;
        Ok(PersistedSessionForkResult {
            session: forked_session,
        })
    }
    .await;
    if fork_result.is_err() {
        let _ = std::fs::remove_dir_all(&target_dir);
    }
    fork_result
}

/// Loads the complete Provider ledger from one segmented state document.
/// 从一个分段状态文档加载完整 Provider 账本。
fn provider_call_ledger_from_state_document(
    session_dir: &Path,
) -> Result<vulcan_workbench_model::SessionProviderCallLedger, String> {
    let Some(state_document) = read_state(session_dir)? else {
        return Ok(Default::default());
    };
    if !state_document.runtime_state.is_object() {
        return Ok(Default::default());
    }
    let mut runtime_state = deserialize_session_runtime_state_entry(
        state_document.runtime_state,
        "invalid persisted Provider runtime state: ",
    )?;
    runtime_state
        .migrate_legacy_provider_usage_summaries()
        .map_err(|error| format!("invalid legacy Provider usage summaries: {error}"))?;
    Ok(runtime_state.provider_call_ledger)
}

/// Writes the inherited source-call prefix into a fork without touching public usage statistics.
/// 将继承的源调用前缀写入分叉会话且不改写公共用量统计。
///
/// `session_dir` owns the copied state, `ledger` is the source call history, the retained-record
/// boundary and cutoff select the inherited prefix, `truncate_child_contributions` follows an
/// anchored fork, `identity_replacements` rewrites target ownership, and `context_generation_id`
/// invalidates every source request-budget anchor.
/// `session_dir` 持有已复制状态，`ledger` 是源调用历史，保留记录边界与截止时间选择继承前缀，
/// `truncate_child_contributions` 跟随锚定分叉，`identity_replacements` 改写目标归属，
/// `context_generation_id` 使全部源请求预算锚点失效。
fn persist_forked_provider_call_ledger(
    session_dir: &Path,
    mut ledger: vulcan_workbench_model::SessionProviderCallLedger,
    retained_source_record_ids: &BTreeSet<String>,
    cutoff_at_ms: i64,
    truncate_child_contributions: bool,
    identity_replacements: &BTreeMap<String, String>,
    context_generation_id: String,
) -> Result<(), String> {
    if ledger.revision > u64::MAX - 2 {
        return Err(String::from("ERR_SESSION_FORK_LEDGER_REVISION_EXHAUSTED"));
    }
    if !ledger.retain_inherited_through_record_boundary(
        retained_source_record_ids,
        cutoff_at_ms,
        truncate_child_contributions,
        context_generation_id,
    ) {
        return Err(String::from("ERR_SESSION_FORK_LEDGER_BOUNDARY_INVALID"));
    }
    if !ledger.rewrite_identities_for_fork(identity_replacements) {
        return Err(String::from(
            "ERR_SESSION_FORK_LEDGER_IDENTITY_REWRITE_FAILED",
        ));
    }
    let mut state_document = read_state(session_dir)?
        .ok_or_else(|| String::from("ERR_SESSION_FORK_STATE_DOCUMENT_MISSING"))?;
    let runtime_state = state_document
        .runtime_state
        .as_object_mut()
        .ok_or_else(|| String::from("ERR_SESSION_FORK_RUNTIME_STATE_INVALID"))?;
    runtime_state.insert(
        String::from("provider_call_ledger"),
        serde_json::to_value(ledger)
            .map_err(|error| format!("serialize forked Provider call ledger failed: {error}"))?,
    );
    state_document.updated_at = current_unix_millis_text();
    write_state(session_dir, &state_document)
}

/// Rebuilds forked runtime artifacts from the already replayed source state.
/// 从已重放的源会话状态重建分叉运行态产物。
fn rebuild_forked_persisted_session_runtime_from_source_replay(
    session_dir: &Path,
    target_session_id_text: &str,
    source_replay: &SegmentedSessionReplayResult,
    source_retained_record_count: usize,
    replacements: &BTreeMap<String, String>,
) -> Result<(), String> {
    let mut runtime_state = read_state(session_dir)?
        .filter(|state| !state.runtime_state.is_null())
        .map(|state| {
            deserialize_session_runtime_state_entry(
                state.runtime_state,
                "ERR_SESSION_FORK_RUNTIME_STATE_INVALID: ",
            )
        })
        .transpose()?
        .unwrap_or_default();
    runtime_state
        .migrate_legacy_provider_usage_summaries()
        .map_err(|error| format!("ERR_SESSION_FORK_RUNTIME_STATE_INVALID: {error}"))?;
    let mut retained_conversation_state = source_replay.conversation_state.clone();
    retained_conversation_state
        .records
        .truncate(source_retained_record_count);
    retained_conversation_state.continuation = Default::default();
    rewrite_forked_conversation_state_identity(&mut retained_conversation_state, replacements)?;
    let original_runtime_messages = retained_conversation_state.messages.clone();
    let retained_messages = conversation_state_to_session_messages(&retained_conversation_state);
    let retained_context_start_message_id = retained_context_anchor(
        source_replay
            .context_start_message_id
            .clone()
            .map(|message_id| rewrite_forked_identity_text(message_id, replacements)),
        &retained_messages,
    );
    retained_conversation_state = fork_conversation_state_from_retained_messages(
        &retained_messages,
        &original_runtime_messages,
    )?;
    let retained_record_ids = retained_conversation_state
        .records
        .iter()
        .map(|record| record.id.clone())
        .collect::<BTreeSet<_>>();
    let mut metadata = read_session_metadata(session_dir)?;
    update_segmented_message_checkpoint(&mut metadata, &retained_messages)?;
    write_metadata(session_dir, &metadata)?;
    let runtime_state = rebuild_runtime_state_for_persisted_fork(
        runtime_state,
        retained_conversation_state,
        source_replay.last_compaction_id.clone(),
        retained_context_start_message_id,
        &retained_record_ids,
        replacements,
        current_unix_millis_text(),
    )?;
    reset_vmm_state_for_history_rewrite(session_dir)?;
    let state_document = state_document_from_runtime_state(target_session_id_text, &runtime_state)?;
    write_state(session_dir, &state_document)?;
    rebuild_runtime_log_from_conversation_state(
        session_dir,
        &runtime_state.conversation_state,
        &runtime_state.updated_at,
    )?;
    Ok(())
}

/// Rebuilds one forked runtime state while preserving session configuration and rewriting owned identities.
/// 重建一份分叉运行态，同时保留会话配置并改写其拥有的身份标识。
///
/// `runtime_state` is the complete copied source state and `retained_conversation_state` is the canonical fork prefix.
/// `runtime_state` 是完整复制的源状态，`retained_conversation_state` 是规范分叉前缀。
/// `last_compaction_id` and `context_start_message_id` retain the surviving context boundary.
/// `last_compaction_id` 与 `context_start_message_id` 保留仍有效的上下文边界。
/// `retained_record_ids` limits UI references and `replacements` rewrites exact source identities.
/// `retained_record_ids` 限制 UI 引用，`replacements` 精确改写源身份。
/// `updated_at` is the deterministic persisted update timestamp.
/// `updated_at` 是确定性的持久化更新时间戳。
///
/// # Returns
/// 返回值
/// Returns a target-owned runtime state with history-bound execution state cleared.
/// 返回一份清除历史绑定执行状态且归目标所有的运行态。
fn rebuild_runtime_state_for_persisted_fork(
    mut runtime_state: SessionRuntimeStateEntry,
    mut retained_conversation_state: SessionConversationState,
    last_compaction_id: Option<String>,
    context_start_message_id: Option<String>,
    retained_record_ids: &BTreeSet<String>,
    replacements: &BTreeMap<String, String>,
    updated_at: String,
) -> Result<SessionRuntimeStateEntry, String> {
    retained_conversation_state.continuation = Default::default();
    runtime_state.conversation_state = retained_conversation_state;
    runtime_state.updated_at = updated_at;
    runtime_state.last_compaction_id = last_compaction_id;
    runtime_state.context_start_message_id = context_start_message_id;
    runtime_state.live_context_budget_fingerprint = None;
    runtime_state.live_context_assembly_fingerprint = None;
    runtime_state.last_provider_request_preview_markdown = None;
    runtime_state.context_injection_state = Default::default();
    runtime_state.todolist = Default::default();
    runtime_state.plan_mode = Default::default();
    runtime_state.vmm_writeback_outbox.clear();
    runtime_state.vmm_profile_sync = Default::default();
    runtime_state.auto_compaction_consecutive_failures = 0;
    runtime_state.tool_call_record_ids.clear();
    runtime_state.tool_request_record_ids.clear();
    runtime_state.active_precheck_turn_id = None;
    runtime_state.session_runtime_kernel = None;
    runtime_state.ui_state.pending_prompt_queue.clear();
    runtime_state.ui_state.pending_prompt_panel_queue.clear();
    let mut runtime_state_value = serialize_session_runtime_state_entry(
        &runtime_state,
        "serialize forked runtime state failed: ",
    )?;
    rewrite_json_value_identity_tokens(&mut runtime_state_value, replacements);
    let mut runtime_state = deserialize_session_runtime_state_entry(
        runtime_state_value,
        "deserialize forked runtime state failed: ",
    )?;
    runtime_state
        .ui_state
        .collapsed_message_ids
        .retain(|record_id| retained_record_ids.contains(record_id));
    Ok(runtime_state)
}

/// Builds the exact session-id replacement table used by persisted fork reconstruction.
/// 构建持久化分叉重建使用的精确会话标识替换表。
fn forked_identity_replacements_for_persisted_session(
    source_session_id: SessionId,
    target_session_id: SessionId,
) -> BTreeMap<String, String> {
    let mut replacements = BTreeMap::new();
    replacements.insert(source_session_id.to_string(), target_session_id.to_string());
    replacements
}

/// Rewrites session identity tokens inside an already replayed conversation state.
/// 改写已重放会话状态内部的会话身份 token。
fn rewrite_forked_conversation_state_identity(
    conversation_state: &mut SessionConversationState,
    replacements: &BTreeMap<String, String>,
) -> Result<(), String> {
    let mut state_value = serde_json::to_value(&conversation_state).map_err(to_store_error)?;
    rewrite_json_value_identity_tokens(&mut state_value, replacements);
    *conversation_state = serde_json::from_value(state_value).map_err(to_store_error)?;
    Ok(())
}

/// Rewrites one text value with the fork identity replacement table.
/// 使用分叉身份替换表改写单个文本值。
fn rewrite_forked_identity_text(
    mut text: String,
    replacements: &BTreeMap<String, String>,
) -> String {
    rewrite_string_identity_tokens(&mut text, replacements);
    text
}

/// Returns the retained display-record index for one message-scoped fork anchor.
/// 返回消息级分叉锚点对应的保留展示记录下标。
fn fork_anchor_record_index_in_replay(
    replay: &SegmentedSessionReplayResult,
    anchor_message_id: &str,
) -> Result<usize, String> {
    replay
        .conversation_state
        .records
        .iter()
        .position(|record| record.id == anchor_message_id && record.visible)
        .ok_or_else(|| String::from("ERR_SESSION_FORK_ANCHOR_NOT_FOUND"))
        .and_then(|record_index| {
            ensure_fork_anchor_after_context_start(replay, record_index)?;
            Ok(record_index)
        })
}

/// Returns the last visible display-record index in one replayed segmented session.
/// 返回一份分段会话重放结果中的最后一条可见展示记录下标。
fn last_visible_record_index_in_replay(replay: &SegmentedSessionReplayResult) -> Option<usize> {
    replay
        .conversation_state
        .records
        .iter()
        .rposition(|record| record.visible)
}

/// Returns the semantic fork boundary after hidden records owned by the selected visible anchor.
/// 返回选中可见锚点所属隐藏记录之后的语义分叉边界。
///
/// # Parameters
/// 参数
/// `replay` is the canonical source replay and `anchor_index` is the validated visible record index.
/// `replay` 是规范源回放，`anchor_index` 是已校验的可见记录索引。
///
/// # Returns
/// 返回值
/// Returns the exclusive retained record count, ending immediately before the next visible record.
/// 返回保留记录的排他数量，边界位于下一条可见记录之前。
fn fork_retained_record_count(replay: &SegmentedSessionReplayResult, anchor_index: usize) -> usize {
    replay
        .conversation_state
        .records
        .iter()
        .enumerate()
        .skip(anchor_index.saturating_add(1))
        .find(|(_, record)| record.visible)
        .map(|(index, _)| index)
        .unwrap_or(replay.conversation_state.records.len())
}

/// Rejects anchors that belong to display history before the active runtime context boundary.
/// 拒绝位于活动运行时上下文边界之前的展示历史锚点。
fn ensure_fork_anchor_after_context_start(
    replay: &SegmentedSessionReplayResult,
    record_index: usize,
) -> Result<(), String> {
    let Some(context_start_message_id) = replay
        .context_start_message_id
        .as_deref()
        .map(str::trim)
        .filter(|message_id| !message_id.is_empty())
    else {
        return Ok(());
    };
    let context_start_index = replay
        .conversation_state
        .records
        .iter()
        .position(|record| record.id == context_start_message_id)
        .ok_or_else(|| String::from("ERR_SESSION_FORK_CONTEXT_START_NOT_FOUND"))?;
    if record_index < context_start_index {
        return Err(String::from("ERR_SESSION_FORK_ANCHOR_NOT_FOUND"));
    }
    Ok(())
}

/// Rebuilds fork runtime state from retained display records while preserving exact original runtime payloads when ids still match.
/// 从保留展示记录重建分叉运行态，并在标识仍匹配时保留原始运行时载荷。
fn fork_conversation_state_from_retained_messages(
    retained_messages: &[SessionMessageEntry],
    original_runtime_messages: &[SessionConversationMessage],
) -> Result<SessionConversationState, String> {
    let mut conversation_state = conversation_state_from_session_messages(retained_messages)?;
    for message in &mut conversation_state.messages {
        if let Some(original_message) = original_runtime_messages
            .iter()
            .find(|original_message| original_message.id == message.id)
        {
            *message = original_message.clone();
        }
    }
    Ok(conversation_state)
}

/// Preserves the context-start anchor only when it is still present after fork truncation.
/// 仅当上下文起点锚点在分叉截断后仍然存在时保留它。
fn retained_context_anchor(
    context_start_message_id: Option<String>,
    retained_messages: &[SessionMessageEntry],
) -> Option<String> {
    context_start_message_id.filter(|message_id| {
        retained_messages
            .iter()
            .any(|message| message.id == message_id.as_str() && message.visible)
    })
}

/// Records a display-only source marker for a forked persisted session.
/// 为已分叉的持久化会话记录纯展示用来源标记。
fn record_persisted_session_fork_source_marker(
    session_dir: &Path,
    source_session: &SessionSummary,
    source_project_id: &str,
    anchor_message_id: Option<String>,
) -> Result<(), String> {
    write_segmented_session_fork_source(
        session_dir,
        SegmentedSessionForkSource {
            source_project_tag: source_project_id.to_string(),
            source_session_id: source_session.id.to_string(),
            source_title: source_session.title.clone(),
            anchor_message_id,
            created_at: current_unix_millis_text(),
        },
    )
}

/// Locates one known session directory by its project and creation timestamp instead of scanning all sessions.
/// 通过项目与创建时间定位一条已知会话目录，而不是扫描全部会话。
fn locate_segmented_session_dir_for_summary(
    controller: &ControllerRuntimeHandle,
    session: &SessionSummary,
    project_id: &str,
    missing_error: &str,
) -> Result<PathBuf, String> {
    let created_at = normalized_session_created_at(session);
    find_segmented_session_directory_for_task_store_created_at(
        &controller.data_root,
        project_id,
        &created_at,
        &session.id.to_string(),
    )
    .ok_or_else(|| missing_error.to_string())
}

/// Rewrites a source message anchor the same way the forked directory identity rewrite does.
/// 按分叉目录身份改写的同一规则改写源消息锚点。
fn forked_anchor_message_id(
    anchor_message_id: &str,
    source_session_id: SessionId,
    target_session_id: SessionId,
) -> String {
    anchor_message_id.replace(
        &source_session_id.to_string(),
        &target_session_id.to_string(),
    )
}

/// Located persisted user prompt and its retained prefix.
/// 已定位的持久化用户提示词及其保留前缀。
struct PersistedUserPromptTarget {
    /// Segmented session directory that owns the target message.
    /// 拥有目标消息的分段会话目录。
    session_dir: std::path::PathBuf,
    /// Retained canonical conversation state before the target user prompt.
    /// 目标用户提示词之前保留的规范会话状态。
    retained_conversation_state: SessionConversationState,
    /// Retained transcript messages before the target user prompt.
    /// 目标用户提示词之前保留的转录消息。
    retained_messages: Vec<SessionMessageEntry>,
    /// Last compaction id preserved from segmented replay.
    /// 从分段重放中保留的最近压缩标识。
    last_compaction_id: Option<String>,
    /// Context-start message id preserved from segmented replay.
    /// 从分段重放中保留的上下文起点消息标识。
    context_start_message_id: Option<String>,
    /// Complete persisted runtime state retained across destructive transcript edits.
    /// 在破坏性转录编辑中完整保留的持久化运行态。
    runtime_state: SessionRuntimeStateEntry,
    /// Canonical record identities that remain before the edited user prompt.
    /// 编辑用户提示词之前仍保留的规范记录身份。
    retained_record_ids: BTreeSet<String>,
    /// Target user-message timestamp used to map calls onto the retained branch prefix.
    /// 用于把调用映射到保留分支前缀的目标用户消息时间戳。
    edit_cutoff_at_ms: i64,
}

/// Locates one editable user prompt from persisted segmented replay output.
/// 从持久化分段重放结果中定位一个可编辑用户提示词。
fn locate_persisted_user_prompt_target(
    controller: &ControllerRuntimeHandle,
    session: &SessionSummary,
    project_id: &str,
    message_id: &str,
    mut runtime_state: SessionRuntimeStateEntry,
) -> Result<PersistedUserPromptTarget, String> {
    let session_dir = locate_segmented_session_dir_for_summary(
        controller,
        session,
        project_id,
        "ERR_USER_PROMPT_EDIT_SESSION_NOT_FOUND",
    )?;
    runtime_state
        .migrate_legacy_provider_usage_summaries()
        .map_err(|error| format!("ERR_USER_PROMPT_EDIT_RUNTIME_STATE_INVALID: {error}"))?;
    let record_index = runtime_state
        .conversation_state
        .records
        .iter()
        .position(|record| {
            record.id == message_id
                && record.visible
                && record.kind == SessionMessageKind::User
                && record.record_kind == SessionMessageRecordKind::Transcript
        })
        .ok_or_else(|| String::from("ERR_USER_PROMPT_EDIT_NOT_FOUND"))?;
    let message_index = runtime_state
        .conversation_state
        .messages
        .iter()
        .position(|message| message.id == message_id)
        .ok_or_else(|| String::from("ERR_USER_PROMPT_EDIT_PROTOCOL_MISMATCH"))?;
    if !matches!(
        &runtime_state.conversation_state.messages[message_index].payload,
        SessionConversationPayload::UserText { .. }
    ) {
        return Err(String::from("ERR_USER_PROMPT_EDIT_PROTOCOL_MISMATCH"));
    }
    let edit_cutoff_at_ms = runtime_state.conversation_state.records[record_index]
        .created_at
        .parse::<i64>()
        .map_err(|_| String::from("ERR_USER_PROMPT_EDIT_TIMESTAMP_INVALID"))?;
    let retained_record_ids = runtime_state
        .conversation_state
        .records
        .iter()
        .take(record_index)
        .map(|record| record.id.clone())
        .collect::<BTreeSet<_>>();
    let last_compaction_id = runtime_state.last_compaction_id.clone();
    let context_start_message_id = runtime_state.context_start_message_id.clone();
    let mut retained_conversation_state = std::mem::take(&mut runtime_state.conversation_state);
    retained_conversation_state.records.truncate(record_index);
    retained_conversation_state.messages.truncate(message_index);
    retained_conversation_state.continuation = Default::default();
    let retained_messages = conversation_state_to_session_messages(&retained_conversation_state);
    Ok(PersistedUserPromptTarget {
        session_dir,
        retained_conversation_state,
        retained_messages,
        last_compaction_id,
        context_start_message_id,
        runtime_state,
        retained_record_ids,
        edit_cutoff_at_ms,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Creates an isolated persisted-session directory for edit rollback tests.
    /// 为编辑回滚测试创建隔离的持久化会话目录。
    ///
    /// # Returns
    /// 返回值
    /// Returns the test root and its authoritative session directory.
    /// 返回测试根目录及其权威会话目录。
    fn edit_backup_fixture() -> (PathBuf, PathBuf) {
        let root =
            std::env::temp_dir().join(format!("vulcan-user-prompt-edit-backup-{}", Uuid::new_v4()));
        // session_id gives the fixture a valid exact owner accepted by metadata validation.
        // session_id 为夹具提供元数据校验接受的有效精确所有者。
        let session_id = Uuid::new_v4().to_string();
        // session_dir is created by the production constructor so no test-only directory guessing is involved.
        // session_dir 由生产构造器创建，因此不涉及测试专用的目录猜测。
        let session_dir = create_segmented_session_in_location(
            &root,
            false,
            SegmentedSessionCreateRequest {
                session_id,
                project_tag: String::from("project-test"),
                created_date: String::from("2026-08-22"),
                created_at: String::from("100"),
                project_id: Some(String::from("project-test")),
                title: Some(String::from("original metadata")),
                title_source: Some(String::from("test")),
                segment_policy: SegmentPolicy::default(),
            },
        )
        .expect("create edit backup fixture");
        std::fs::create_dir_all(session_dir.join("runtime"))
            .expect("create edit backup runtime directory");
        std::fs::write(
            session_dir.join("runtime").join("r001.jsonl"),
            "original runtime",
        )
        .expect("write original runtime");
        (root, session_dir)
    }

    /// Verifies failed prompt editing restores only the declared mutable files without scanning unknown files.
    /// 验证提示词编辑失败时仅恢复已声明的可变文件，并且不会扫描未知文件。
    #[test]
    fn persisted_edit_backup_restores_declared_files_without_scanning_unknown_files() {
        let (root, session_dir) = edit_backup_fixture();
        // original_metadata is the exact byte baseline restored after the failed rewrite.
        // original_metadata 是失败重写后要恢复的精确字节基线。
        let original_metadata =
            std::fs::read(session_dir.join("metadata.json")).expect("read original metadata");
        let mut backup =
            PersistedSessionEditRollbackBackup::create(&session_dir).expect("create edit backup");
        let sessions_root = session_dir
            .parent()
            .and_then(Path::parent)
            .expect("sessions root");
        assert!(!backup.backup_dir.starts_with(sessions_root));
        // rewritten_metadata remains structurally valid so rollback can enumerate its declared segments directly.
        // rewritten_metadata 保持结构有效，使回滚能够直接枚举其声明的分段。
        let mut rewritten_metadata = read_session_metadata(&session_dir).expect("read metadata");
        rewritten_metadata.title = Some(String::from("rewritten metadata"));
        let mut encoded_rewritten_metadata =
            serde_json::to_vec_pretty(&rewritten_metadata).expect("encode rewritten metadata");
        encoded_rewritten_metadata.push(b'\n');
        std::fs::write(
            session_dir.join("metadata.json"),
            encoded_rewritten_metadata,
        )
        .expect("rewrite metadata");
        std::fs::write(
            session_dir.join("runtime").join("r001.jsonl"),
            "rewritten runtime",
        )
        .expect("rewrite runtime");
        std::fs::write(session_dir.join("unexpected.jsonl"), "partial edit")
            .expect("write partial edit artifact");

        backup.restore().expect("restore original session");

        assert_eq!(
            std::fs::read(session_dir.join("metadata.json")).expect("read restored metadata"),
            original_metadata
        );
        assert_eq!(
            std::fs::read_to_string(session_dir.join("runtime").join("r001.jsonl"))
                .expect("read restored runtime"),
            "original runtime"
        );
        assert_eq!(
            std::fs::read_to_string(session_dir.join("unexpected.jsonl"))
                .expect("read untouched unknown file"),
            "partial edit"
        );
        std::fs::remove_dir_all(root).expect("remove edit backup fixture");
    }

    /// Verifies committed prompt editing keeps the rewritten tree and deletes its backup.
    /// 验证提示词编辑提交后会保留改写目录并删除事务备份。
    #[test]
    fn persisted_edit_backup_commit_keeps_rewritten_tree() {
        let (root, session_dir) = edit_backup_fixture();
        let mut backup =
            PersistedSessionEditRollbackBackup::create(&session_dir).expect("create edit backup");
        let backup_dir = backup.backup_dir.clone();
        // rewritten_metadata is a valid committed replacement of the declared metadata file.
        // rewritten_metadata 是声明元数据文件的有效提交替代内容。
        let mut rewritten_metadata = read_session_metadata(&session_dir).expect("read metadata");
        rewritten_metadata.title = Some(String::from("rewritten metadata"));
        let mut encoded_rewritten_metadata =
            serde_json::to_vec_pretty(&rewritten_metadata).expect("encode rewritten metadata");
        encoded_rewritten_metadata.push(b'\n');
        std::fs::write(
            session_dir.join("metadata.json"),
            encoded_rewritten_metadata,
        )
        .expect("rewrite metadata");

        backup.commit().expect("commit edit backup");

        assert_eq!(
            read_session_metadata(&session_dir)
                .expect("read committed metadata")
                .title
                .as_deref(),
            Some("rewritten metadata")
        );
        assert!(!backup_dir.exists());
        std::fs::remove_dir_all(root).expect("remove edit backup fixture");
    }

    /// Builds one persisted transcript message for fork-anchor lookup tests.
    /// 为分叉锚点查找测试构建一条持久化转录消息。
    fn test_message(
        id: &str,
        kind: SessionMessageKind,
        record_kind: SessionMessageRecordKind,
        body: &str,
    ) -> SessionMessageEntry {
        let mut message = SessionMessageEntry::new(kind, "Title", body)
            .with_runtime_metadata(id, format!("2026-07-09T10:00:0{}Z", id.len()));
        message.record_kind = record_kind;
        message
    }

    /// Builds one replay result from the current runtime-context messages.
    /// 根据当前运行时上下文消息构建一份重放结果。
    fn replay_from_messages(messages: Vec<SessionMessageEntry>) -> SegmentedSessionReplayResult {
        SegmentedSessionReplayResult {
            conversation_state: conversation_state_from_session_messages(&messages)
                .expect("valid replayed conversation identities"),
            last_compaction_id: Some(String::from("segmented-compaction-test")),
            context_start_message_id: messages.first().map(|message| message.id.clone()),
            updated_at: Some(String::from("2026-07-09T10:00:00Z")),
        }
    }

    /// Verifies an edit retains session configuration while clearing only history-bound execution state.
    /// 验证编辑会保留会话配置，同时只清理与历史绑定的执行状态。
    #[test]
    fn user_prompt_edit_rebuild_preserves_session_configuration() {
        let retained_message = test_message(
            "retained-user",
            SessionMessageKind::User,
            SessionMessageRecordKind::Transcript,
            "retained prompt",
        );
        let retained_conversation_state =
            conversation_state_from_session_messages(std::slice::from_ref(&retained_message))
                .expect("retained conversation state should build");
        let retained_record_ids = BTreeSet::from([retained_message.id.clone()]);
        let mut runtime_state = SessionRuntimeStateEntry {
            active_provider_id: Some(String::from("provider-1")),
            active_connection_id: Some(String::from("connection-1")),
            active_model_id: Some(String::from("model-1")),
            active_work_mode: Some(String::from("code")),
            provider_tool_settings_initialized: true,
            live_context_budget_fingerprint: Some(String::from("stale-budget")),
            live_context_assembly_fingerprint: Some(String::from("stale-assembly")),
            last_provider_request_preview_markdown: Some(String::from("stale preview")),
            active_precheck_turn_id: Some(String::from("stale-turn")),
            ..SessionRuntimeStateEntry::default()
        };
        runtime_state.output_style.override_style_id = Some(String::from("concise"));
        runtime_state
            .tool_call_record_ids
            .insert(String::from("tool-call"), String::from("record-after"));
        runtime_state
            .tool_request_record_ids
            .insert(String::from("tool-request"), String::from("record-after"));
        runtime_state
            .ui_state
            .collapsed_message_ids
            .extend([String::from("retained-user"), String::from("record-after")]);

        let rebuilt = rebuild_runtime_state_for_user_prompt_edit(
            runtime_state,
            retained_conversation_state,
            &retained_record_ids,
            200,
            Some(String::from("compaction-1")),
            Some(String::from("retained-user")),
        )
        .expect("edit runtime state should rebuild");

        assert_eq!(rebuilt.active_provider_id.as_deref(), Some("provider-1"));
        assert_eq!(
            rebuilt.active_connection_id.as_deref(),
            Some("connection-1")
        );
        assert_eq!(rebuilt.active_model_id.as_deref(), Some("model-1"));
        assert_eq!(rebuilt.active_work_mode.as_deref(), Some("code"));
        assert!(rebuilt.provider_tool_settings_initialized);
        assert_eq!(
            rebuilt.output_style.override_style_id.as_deref(),
            Some("concise")
        );
        assert_eq!(rebuilt.provider_call_ledger.active_branch_generation, 1);
        assert_eq!(
            rebuilt
                .provider_call_ledger
                .active_context_generation_id
                .as_deref(),
            Some("edit_context_generation:1")
        );
        assert!(
            rebuilt
                .provider_call_ledger
                .last_stable_context_baseline
                .is_none()
        );
        assert_eq!(rebuilt.last_compaction_id.as_deref(), Some("compaction-1"));
        assert_eq!(
            rebuilt.context_start_message_id.as_deref(),
            Some("retained-user")
        );
        assert!(rebuilt.live_context_budget_fingerprint.is_none());
        assert!(rebuilt.live_context_assembly_fingerprint.is_none());
        assert!(rebuilt.last_provider_request_preview_markdown.is_none());
        assert!(rebuilt.active_precheck_turn_id.is_none());
        assert!(rebuilt.tool_call_record_ids.is_empty());
        assert!(rebuilt.tool_request_record_ids.is_empty());
        assert_eq!(
            rebuilt.ui_state.collapsed_message_ids,
            BTreeSet::from([String::from("retained-user")])
        );
        assert_eq!(rebuilt.conversation_state.records.len(), 1);
    }

    /// Verifies an anchored fork preserves session configuration while rewriting identities and clearing stale execution state.
    /// 验证锚定分叉会保留会话配置，同时改写身份并清理过期执行状态。
    #[test]
    fn anchored_fork_rebuild_preserves_session_configuration() {
        let retained_message = test_message(
            "record-target-session",
            SessionMessageKind::User,
            SessionMessageRecordKind::Transcript,
            "retained fork prompt",
        );
        let retained_conversation_state =
            conversation_state_from_session_messages(std::slice::from_ref(&retained_message))
                .expect("retained fork conversation state should build");
        let retained_record_ids = BTreeSet::from([retained_message.id.clone()]);
        let replacements = BTreeMap::from([(
            String::from("source-session"),
            String::from("target-session"),
        )]);
        let mut runtime_state = SessionRuntimeStateEntry {
            active_provider_id: Some(String::from("provider-1")),
            active_connection_id: Some(String::from("connection-1")),
            active_model_id: Some(String::from("model-1")),
            active_work_mode: Some(String::from("code")),
            provider_tool_settings_initialized: true,
            live_context_budget_fingerprint: Some(String::from("stale-budget")),
            live_context_assembly_fingerprint: Some(String::from("stale-assembly")),
            last_provider_request_preview_markdown: Some(String::from("stale preview")),
            active_precheck_turn_id: Some(String::from("stale-turn")),
            ..SessionRuntimeStateEntry::default()
        };
        runtime_state.output_style.override_style_id = Some(String::from("concise"));
        runtime_state.tool_call_record_ids.insert(
            String::from("tool-call"),
            String::from("record-source-session"),
        );
        runtime_state.ui_state.collapsed_message_ids.extend([
            String::from("record-source-session"),
            String::from("discarded-record"),
        ]);

        let rebuilt = rebuild_runtime_state_for_persisted_fork(
            runtime_state,
            retained_conversation_state,
            Some(String::from("compaction-source-session")),
            Some(String::from("record-target-session")),
            &retained_record_ids,
            &replacements,
            String::from("2026-08-10T00:00:00Z"),
        )
        .expect("fork runtime state should rebuild");

        assert_eq!(rebuilt.active_provider_id.as_deref(), Some("provider-1"));
        assert_eq!(
            rebuilt.active_connection_id.as_deref(),
            Some("connection-1")
        );
        assert_eq!(rebuilt.active_model_id.as_deref(), Some("model-1"));
        assert_eq!(rebuilt.active_work_mode.as_deref(), Some("code"));
        assert!(rebuilt.provider_tool_settings_initialized);
        assert_eq!(
            rebuilt.output_style.override_style_id.as_deref(),
            Some("concise")
        );
        assert_eq!(
            rebuilt.last_compaction_id.as_deref(),
            Some("compaction-target-session")
        );
        assert_eq!(
            rebuilt.context_start_message_id.as_deref(),
            Some("record-target-session")
        );
        assert_eq!(
            rebuilt.ui_state.collapsed_message_ids,
            BTreeSet::from([String::from("record-target-session")])
        );
        assert!(rebuilt.live_context_budget_fingerprint.is_none());
        assert!(rebuilt.live_context_assembly_fingerprint.is_none());
        assert!(rebuilt.last_provider_request_preview_markdown.is_none());
        assert!(rebuilt.active_precheck_turn_id.is_none());
        assert!(rebuilt.tool_call_record_ids.is_empty());
        assert_eq!(rebuilt.conversation_state.records.len(), 1);
    }

    /// Verifies a fork anchor is accepted when it belongs to the visible transcript records.
    /// 验证分叉锚点属于可见转录记录时会被接受。
    #[test]
    fn fork_anchor_record_index_accepts_current_display_anchor() {
        let replay = replay_from_messages(vec![
            test_message(
                "summary-user",
                SessionMessageKind::User,
                SessionMessageRecordKind::Transcript,
                "compressed prompt",
            ),
            test_message(
                "summary-assistant",
                SessionMessageKind::Assistant,
                SessionMessageRecordKind::AssistantOutput,
                "compressed answer",
            ),
            test_message(
                "live-assistant",
                SessionMessageKind::Assistant,
                SessionMessageRecordKind::AssistantOutput,
                "new answer",
            ),
        ]);

        let record_index = fork_anchor_record_index_in_replay(&replay, "live-assistant")
            .expect("current display anchor should be accepted");

        assert_eq!(record_index, 2);
    }

    /// Verifies a visible assistant anchor retains its hidden terminal records but a user anchor excludes the later response.
    /// 验证可见 Assistant 锚点保留所属隐藏终态记录，而用户锚点排除后续响应。
    #[test]
    fn fork_semantic_boundary_retains_hidden_records_owned_by_visible_anchor() {
        let mut replay = replay_from_messages(vec![
            test_message(
                "user",
                SessionMessageKind::User,
                SessionMessageRecordKind::Transcript,
                "prompt",
            ),
            test_message(
                "assistant",
                SessionMessageKind::Assistant,
                SessionMessageRecordKind::AssistantOutput,
                "answer",
            ),
            test_message(
                "next-user",
                SessionMessageKind::User,
                SessionMessageRecordKind::Transcript,
                "next prompt",
            ),
        ]);
        let mut hidden_summary = replay.conversation_state.records[1].clone();
        hidden_summary.id = String::from("hidden-summary");
        hidden_summary.visible = false;
        hidden_summary.record_kind = SessionMessageRecordKind::SummaryOutput;
        let mut hidden_stats = hidden_summary.clone();
        hidden_stats.id = String::from("hidden-stats");
        hidden_stats.record_kind = SessionMessageRecordKind::TurnStats;
        replay.conversation_state.records.insert(2, hidden_summary);
        replay.conversation_state.records.insert(3, hidden_stats);

        assert_eq!(fork_retained_record_count(&replay, 0), 1);
        assert_eq!(fork_retained_record_count(&replay, 1), 4);
        assert_eq!(fork_retained_record_count(&replay, 4), 5);
    }

    /// Verifies fork validation does not require a matching provider-runtime message id.
    /// 验证分叉校验不要求存在同名 Provider 运行时消息标识。
    #[test]
    fn fork_anchor_record_index_accepts_display_anchor_without_runtime_message() {
        let mut replay = replay_from_messages(vec![
            test_message(
                "summary-user",
                SessionMessageKind::User,
                SessionMessageRecordKind::Transcript,
                "compressed prompt",
            ),
            test_message(
                "display-assistant",
                SessionMessageKind::Assistant,
                SessionMessageRecordKind::AssistantOutput,
                "visible answer",
            ),
        ]);
        replay.conversation_state.messages.clear();

        let record_index = fork_anchor_record_index_in_replay(&replay, "display-assistant")
            .expect("display anchor should not depend on provider messages");

        assert_eq!(record_index, 1);
    }

    /// Verifies visible records before the active context-start anchor cannot be forked.
    /// 验证活动上下文起点之前的可见记录不能作为分叉锚点。
    #[test]
    fn fork_anchor_record_index_rejects_anchor_before_context_start() {
        let mut replay = replay_from_messages(vec![
            test_message(
                "old-assistant",
                SessionMessageKind::Assistant,
                SessionMessageRecordKind::AssistantOutput,
                "old answer",
            ),
            test_message(
                "compaction-summary",
                SessionMessageKind::Assistant,
                SessionMessageRecordKind::CompactionSummary,
                "compressed context",
            ),
            test_message(
                "live-assistant",
                SessionMessageKind::Assistant,
                SessionMessageRecordKind::AssistantOutput,
                "new answer",
            ),
        ]);
        replay.context_start_message_id = Some(String::from("compaction-summary"));

        let error = fork_anchor_record_index_in_replay(&replay, "old-assistant")
            .expect_err("pre-context display anchor should be rejected");

        assert_eq!(error, "ERR_SESSION_FORK_ANCHOR_NOT_FOUND");
        let record_index = fork_anchor_record_index_in_replay(&replay, "live-assistant")
            .expect("post-context display anchor should be accepted");
        assert_eq!(record_index, 2);
    }

    /// Verifies a fork anchor from pruned display history is rejected by runtime replay validation.
    /// 验证来自已裁剪展示历史的分叉锚点会被运行时重放校验拒绝。
    #[test]
    fn fork_anchor_record_index_rejects_pruned_display_anchor() {
        let replay = replay_from_messages(vec![
            test_message(
                "summary-user",
                SessionMessageKind::User,
                SessionMessageRecordKind::Transcript,
                "compressed prompt",
            ),
            test_message(
                "summary-assistant",
                SessionMessageKind::Assistant,
                SessionMessageRecordKind::AssistantOutput,
                "compressed answer",
            ),
        ]);

        let error = fork_anchor_record_index_in_replay(&replay, "old-assistant")
            .expect_err("pruned display anchor should be rejected");

        assert_eq!(error, "ERR_SESSION_FORK_ANCHOR_NOT_FOUND");
    }

    /// Verifies fork truncation uses the target-side message id after session identity rewriting.
    /// 验证分叉截断在会话身份改写后使用目标侧消息标识。
    #[test]
    fn forked_anchor_message_id_matches_session_identity_rewrite() {
        let source_session_id = SessionId::from_u128(610);
        let target_session_id = SessionId::from_u128(611);
        let source_anchor_message_id = format!("{source_session_id}-1783590054811-255");

        let target_anchor_message_id = forked_anchor_message_id(
            &source_anchor_message_id,
            source_session_id,
            target_session_id,
        );

        assert_eq!(
            target_anchor_message_id,
            format!("{target_session_id}-1783590054811-255")
        );
    }

    /// Verifies source replay reconstruction rewrites both visible and runtime message identities.
    /// 验证源重放重建会同时改写展示消息与运行时消息身份。
    #[test]
    fn forked_source_replay_identity_rewrite_updates_retained_state_ids() {
        let source_session_id = SessionId::from_u128(710);
        let target_session_id = SessionId::from_u128(711);
        let source_message_id = format!("{source_session_id}-source-message");
        let target_message_id = format!("{target_session_id}-source-message");
        let mut replay = replay_from_messages(vec![test_message(
            &source_message_id,
            SessionMessageKind::User,
            SessionMessageRecordKind::Transcript,
            "source prompt",
        )]);
        replay.context_start_message_id = Some(source_message_id.clone());
        let replacements = forked_identity_replacements_for_persisted_session(
            source_session_id,
            target_session_id,
        );
        let mut retained_conversation_state = replay.conversation_state.clone();

        rewrite_forked_conversation_state_identity(&mut retained_conversation_state, &replacements)
            .expect("rewrite source replay identity");

        let retained_messages =
            conversation_state_to_session_messages(&retained_conversation_state);
        let retained_context_start_message_id = retained_context_anchor(
            replay
                .context_start_message_id
                .map(|message_id| rewrite_forked_identity_text(message_id, &replacements)),
            &retained_messages,
        );
        let rewritten_state =
            serde_json::to_string(&retained_conversation_state).expect("serialize state");
        assert!(rewritten_state.contains(&target_message_id));
        assert!(!rewritten_state.contains(&source_message_id));
        assert_eq!(retained_context_start_message_id, Some(target_message_id));
    }
}
