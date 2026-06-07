//! Subagent panel state and runtime coordination helpers for the main workbench.
//! 主工作台的子代理面板状态与运行时协调辅助逻辑。

use super::*;
use vulcan_task_session::SegmentedTaskSessionStore;
use vulcan_workbench_model::subagent_navigation_surface_is_visible;

impl VulcanWorkbench {
    /// Rebuilds one session-scoped subagent task cache from segmented task metadata while preserving in-flight running entries.
    /// 根据分段 task 元数据重建单个会话的子代理任务缓存，同时保留仍在运行中的活动条目。
    pub(super) fn rebuild_subagent_task_cache_for_session(&mut self, session_id: SessionId) {
        let segmented_states = self.segmented_subagent_task_panel_states_for_session(session_id);
        let previous_states = self
            .session_subagent_tasks
            .get(&session_id)
            .cloned()
            .unwrap_or_default();
        let task_runtime = self.runtime_foundation.task_runtime();
        let merged_states =
            merge_subagent_task_panel_cache(&previous_states, segmented_states, |task_id| {
                task_runtime.is_task_running(task_id).unwrap_or(false)
            });
        if merged_states.is_empty() {
            self.session_subagent_tasks.remove(&session_id);
        } else {
            self.session_subagent_tasks
                .insert(session_id, merged_states);
        }

        if self
            .active_subagent_panel
            .is_some_and(|selection| selection.parent_session_id == session_id)
            && self.selected_subagent_task_panel_state().is_none()
        {
            self.active_subagent_panel = None;
        }
    }
    /// Loads all direct child TaskSession panel states from the shared segmented store for one direct parent session.
    /// 从共享分段存储加载某个直接父会话下的所有直接子 TaskSession 面板状态。
    fn segmented_subagent_task_panel_states_for_session(
        &self,
        parent_session_id: SessionId,
    ) -> Vec<SubagentTaskPanelState> {
        let Some(store) = self.task_session_store_for_parent_session(parent_session_id) else {
            return Vec::new();
        };
        subagent_task_panel_states_from_segmented_task_session_store(&store, parent_session_id)
    }
    /// Builds the shared segmented TaskSession store for a direct parent session when the root session belongs to a project.
    /// 当根会话属于项目时，为某个直接父会话构建共享分段 TaskSession 存储。
    fn task_session_store_for_parent_session(
        &self,
        parent_session_id: SessionId,
    ) -> Option<SegmentedTaskSessionStore> {
        let (root_session_id, project_id) = if let Some(project_id) = self
            .session_by_id(parent_session_id)
            .and_then(|session| session.project_id.clone())
        {
            (parent_session_id, project_id)
        } else {
            let root_session_id = self.selected_session().map(|session| session.id)?;
            let project_id = self
                .session_by_id(root_session_id)
                .and_then(|session| session.project_id.clone())?;
            (root_session_id, project_id)
        };
        let controller_runtime = self.controller_runtime.as_ref()?;
        let session_dir = crate::workspace_store::find_segmented_session_directory_for_task_store(
            &controller_runtime.data_root,
            &project_id,
            &root_session_id.to_string(),
        )
        .ok()
        .flatten()?;
        Some(SegmentedTaskSessionStore::new(session_dir))
    }
    /// Inserts or updates one cached subagent task panel state under its owning parent session.
    /// 将一条缓存子代理任务面板状态写入或更新到所属父会话之下。
    pub(super) fn upsert_subagent_task_panel_state(&mut self, state: SubagentTaskPanelState) {
        let state = self.normalized_subagent_task_panel_state(&state);
        self.session_subagent_tasks
            .entry(state.parent_session_id)
            .or_default()
            .insert(state.task_id, state);
    }

    /// Refreshes the cached TaskSession tree from the shared segmented store before execution-chain decisions inspect running children.
    /// 在执行链条决策检查运行中子节点前，从共享分段存储刷新缓存的 TaskSession 树。
    pub(super) fn refresh_subagent_task_tree_cache_for_session(
        &mut self,
        root_parent_session_id: SessionId,
    ) {
        let mut seen_parent_ids = BTreeSet::new();
        let mut pending_parent_ids = vec![root_parent_session_id];
        while let Some(parent_session_id) = pending_parent_ids.pop() {
            if !seen_parent_ids.insert(parent_session_id) {
                continue;
            }

            self.rebuild_subagent_task_cache_for_session(parent_session_id);
            let child_task_ids = self
                .session_subagent_tasks
                .get(&parent_session_id)
                .map(|entries| {
                    entries
                        .values()
                        .map(|state| state.task_id)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            pending_parent_ids.extend(child_task_ids);
        }
    }

    /// Returns running subagent tasks after first refreshing the shared segmented TaskSession tree.
    /// 先刷新共享分段 TaskSession 树，再返回运行中的子代理任务。
    pub(super) fn running_subagent_task_states_for_tree_after_refresh(
        &mut self,
        root_parent_session_id: SessionId,
    ) -> Vec<(SessionId, SubagentTaskPanelState)> {
        self.refresh_subagent_task_tree_cache_for_session(root_parent_session_id);
        self.running_subagent_task_states_for_tree(root_parent_session_id)
    }

    /// Returns whether one cached child task is still truly running according to the shared runtime registry rather than the possibly stale UI metadata snapshot.
    /// 根据共享运行时注册表而非可能陈旧的界面元数据快照，返回某个缓存子任务当前是否仍在真实执行中。
    pub(super) fn subagent_task_is_actively_running(&self, task_id: SessionId) -> bool {
        self.runtime_foundation
            .task_runtime()
            .is_task_running(task_id)
            .unwrap_or(false)
    }
    /// Normalizes one cached subagent panel state against the live task runtime without repeated disk scans.
    /// 将一条缓存子代理面板状态与实时任务运行时对齐，且不重复扫描磁盘。
    pub(super) fn normalized_subagent_task_panel_state(
        &self,
        state: &SubagentTaskPanelState,
    ) -> SubagentTaskPanelState {
        normalize_subagent_task_panel_state(
            state,
            self.subagent_task_is_actively_running(state.task_id),
            None,
        )
    }
    /// Seeds one live child-session transcript with its initial task prompt so the subagent panel has the same conversation opening shape as the main panel.
    /// 使用初始任务提示词为实时子会话转录播种，确保子代理面板拥有与主面板一致的对话开场形态。
    pub(super) fn seed_subagent_task_prompt_if_needed(&mut self, task_id: SessionId, prompt: &str) {
        if prompt.trim().is_empty()
            || self
                .session_messages
                .get(&task_id)
                .is_some_and(|messages| !messages.is_empty())
        {
            return;
        }

        self.push_session_message(task_id, SessionMessageKind::User, "用户指令", prompt);
    }
    /// Replaces one child-session transcript projection with the final canonical conversation state returned by the finished subagent task.
    /// 使用已完成子代理任务返回的最终规范会话状态替换对应子会话转录投影。
    pub(super) fn replace_subagent_task_conversation_state(
        &mut self,
        task_id: SessionId,
        conversation_state: SessionConversationState,
    ) {
        self.ensure_session_conversation_state_loaded(task_id);
        let runtime_state = self.ensure_session_runtime_state_mut(task_id);
        let preserved_records = runtime_state.conversation_state.records.clone();
        runtime_state.conversation_state = conversation_state;
        if !preserved_records.is_empty() {
            runtime_state.conversation_state.records = preserved_records;
        }
        runtime_state.updated_at = current_unix_millis_text();
        self.sync_session_messages_from_conversation_state(task_id);
        self.refresh_session_agent_view_buffers(task_id);
    }
    /// Returns one cached subagent task panel state for the given parent session and task identifier when available.
    /// 当存在时，返回给定父会话与任务标识对应的缓存子代理任务面板状态。
    pub(super) fn subagent_task_panel_state(
        &self,
        parent_session_id: SessionId,
        task_id: SessionId,
    ) -> Option<&SubagentTaskPanelState> {
        self.session_subagent_tasks
            .get(&parent_session_id)?
            .get(&task_id)
    }
    /// Returns the currently opened subagent panel state for the selected session when the selection remains valid.
    /// 当当前选择仍然有效时，返回当前选中会话已打开的子代理面板状态。
    pub(super) fn selected_subagent_task_panel_state(&self) -> Option<&SubagentTaskPanelState> {
        let selection = self.active_subagent_panel?;
        self.subagent_task_panel_state(selection.parent_session_id, selection.task_id)
    }
    /// Returns whether the target subagent panel still owns recoverable transcript detail after persistence reloads or memory loss.
    /// 返回目标子代理面板在持久化重载或内存丢失之后是否仍保有可恢复的转录详情。
    pub(super) fn subagent_task_panel_has_recoverable_detail(
        &self,
        parent_session_id: SessionId,
        task_id: SessionId,
    ) -> bool {
        let Some(state) = self.subagent_task_panel_state(parent_session_id, task_id) else {
            return false;
        };

        self.rendered_session_agent_view_document(task_id)
            .is_some_and(|document| !document.blocks.is_empty())
            || subagent_task_panel_state_has_recoverable_conversation(state)
    }
    /// Returns whether the target subagent task still owns live in-process transcript detail instead of only persisted task-card snapshots.
    /// 返回目标子代理任务当前是否仍持有进程内实时转录详情，而不是仅剩持久化 task 卡片快照。
    pub(super) fn subagent_task_panel_has_live_detail(&self, task_id: SessionId) -> bool {
        self.session_messages.contains_key(&task_id)
            || self.session_runtime_events.contains_key(&task_id)
            || self.session_agent_view_documents.contains_key(&task_id)
    }
    /// Finds the most suitable parent task-tool call message for one delegate payload so the visible transcript card can inherit task-session metadata.
    /// 为某份委派 payload 找到最合适的父 task 工具调用消息，使可见转录卡片能够继承 task-session 元数据。
    pub(super) fn find_task_session_tool_message_id_for_payload(
        &self,
        session_id: SessionId,
        payload: &Value,
    ) -> Option<String> {
        let messages = self
            .session_messages
            .get(&session_id)
            .or_else(|| self.archived_session_messages.get(&session_id))
            .into_iter()
            .flat_map(|messages| messages.iter().rev());
        let records = self
            .session_runtime_state(session_id)
            .into_iter()
            .flat_map(|runtime_state| runtime_state.conversation_state.records.iter().rev());
        vulcan_workbench_model::find_task_session_tool_message_id_for_payload(
            messages, records, payload,
        )
    }
    /// Merges the provided metadata into one canonical session record and refreshes all derived transcript projections if the target message exists.
    /// 若目标消息存在，则把给定元数据合并进对应规范会话记录，并刷新全部派生转录投影。
    pub(super) fn merge_metadata_into_session_record(
        &mut self,
        session_id: SessionId,
        message_id: &str,
        metadata_update: Value,
    ) -> bool {
        self.ensure_session_conversation_state_loaded(session_id);
        let updated = {
            let runtime_state = self.ensure_session_runtime_state_mut(session_id);
            let mut updated = false;
            for record in &mut runtime_state.conversation_state.records {
                if record.id != message_id {
                    continue;
                }
                record.metadata = merge_tool_display_metadata(
                    record.metadata.take(),
                    Some(metadata_update.clone()),
                );
                updated = true;
                break;
            }
            if updated {
                runtime_state.updated_at = current_unix_millis_text();
            }
            updated
        };
        if !updated {
            return false;
        }

        self.sync_session_messages_from_conversation_state(session_id);
        self.refresh_session_agent_view_buffers(session_id);
        true
    }
    /// Links one delegate task-session payload back onto the visible parent task card so running cards gain the live child-session identifier needed for panel navigation.
    /// 把一份委派 task-session payload 反向挂回可见父 task 卡片，让运行中卡片获得进入子面板所需的实时子会话标识。
    pub(super) fn attach_task_session_metadata_to_parent_tool_message(
        &mut self,
        session_id: SessionId,
        metadata: &Value,
    ) {
        let Some(payload) = task_session_payload_from_tool_metadata(metadata) else {
            return;
        };
        let Some(message_id) =
            self.find_task_session_tool_message_id_for_payload(session_id, payload)
        else {
            return;
        };

        let execution_status = task_session_execution_status_from_tool_metadata(metadata)
            .unwrap_or("running")
            .to_string();
        let merged_metadata =
            attach_tool_display_state_metadata(Some(metadata.clone()), execution_status.as_str())
                .unwrap_or_else(|| metadata.clone());
        let _ = self.merge_metadata_into_session_record(session_id, &message_id, merged_metadata);
    }
    /// Marks one cached subagent task as stopped both in the panel cache and in the visible parent tool card so interrupted delegates stop looking in-flight.
    /// 同时在面板缓存与可见父工具卡片中把某个缓存子代理任务标记为已停止，避免被中断的委派继续显示为进行中。
    pub(super) fn mark_subagent_task_stopped(
        &mut self,
        parent_session_id: SessionId,
        task_id: SessionId,
        stop_summary: &str,
    ) {
        let mut state_snapshot = self
            .subagent_task_panel_state(parent_session_id, task_id)
            .cloned();
        if let Some(entries) = self.session_subagent_tasks.get_mut(&parent_session_id)
            && let Some(state) = entries.get_mut(&task_id)
        {
            state.run_state = SubagentTaskRunState::Stopped;
            if state.result_text.trim().is_empty() {
                state.result_text = stop_summary.to_string();
            }
            state_snapshot = Some(state.clone());
        }

        let stop_notice_exists = self.session_messages.get(&task_id).is_some_and(|messages| {
            messages.iter().any(|message| {
                message.title == "执行已停止" && message.body.trim() == stop_summary.trim()
            })
        });
        if !stop_notice_exists {
            self.push_session_message(
                task_id,
                SessionMessageKind::Assistant,
                "执行已停止",
                stop_summary,
            );
        }

        let Some(message_id) = self.find_task_session_tool_message_id_for_payload(
            parent_session_id,
            &json!({ "task_id": task_id.to_string() }),
        ) else {
            return;
        };
        let Some(message) = self
            .session_message_entry(parent_session_id, &message_id)
            .cloned()
        else {
            return;
        };

        let updated_at = current_unix_millis_text();
        let updated_metadata = vulcan_workbench_model::build_stopped_task_session_metadata(
            message.metadata.as_ref(),
            state_snapshot.as_ref(),
            task_id,
            stop_summary,
            &updated_at,
        );
        let _ = self.merge_metadata_into_session_record(
            parent_session_id,
            &message_id,
            updated_metadata,
        );
    }

    /// Returns every running subagent task under one parent tree with the direct parent that owns each task card.
    /// 返回某个父级树下所有运行中的子代理任务，并携带每个任务卡片所属的直接父级。
    pub(super) fn running_subagent_task_states_for_tree(
        &self,
        root_parent_session_id: SessionId,
    ) -> Vec<(SessionId, SubagentTaskPanelState)> {
        collect_running_subagent_task_states_for_tree(root_parent_session_id, |parent_session_id| {
            self.session_subagent_tasks
                .get(&parent_session_id)
                .map(|entries| {
                    entries
                        .values()
                        .map(|state| self.normalized_subagent_task_panel_state(state))
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        })
    }

    /// Marks every currently running subagent task under the provided parent session as stopped after the user aborts the parent execution.
    /// 当用户中止父执行时，将该父会话下所有仍在运行的子代理任务统一标记为已停止。
    pub(super) fn mark_running_subagent_tasks_stopped(&mut self, parent_session_id: SessionId) {
        let running_states =
            self.running_subagent_task_states_for_tree_after_refresh(parent_session_id);
        if running_states.is_empty() {
            return;
        }

        let stop_summary = subagent_task_stopped_summary_text();
        for (_, state) in &running_states {
            let _ = self
                .runtime_foundation
                .task_runtime()
                .stop_task_tree(state.task_id);
        }
        for (direct_parent_session_id, state) in running_states {
            self.mark_subagent_task_stopped(direct_parent_session_id, state.task_id, stop_summary);
        }
    }
    /// Returns the subagent task entries that should stay visible inside the selected session navigation panel for the current coordination round.
    /// 返回当前协调轮次中应继续显示在选中会话导航面板里的子代理任务条目。
    pub(super) fn selected_session_active_subagent_tasks(&self) -> Vec<SubagentTaskPanelState> {
        let Some(session_id) = self.selected_session().map(|session| session.id) else {
            return Vec::new();
        };
        let has_visible_round = self
            .session_subagent_navigation_rounds
            .contains(&session_id);
        let has_running_subagents = self.session_has_running_subagent_tasks(session_id);
        let has_active_panel = self
            .active_subagent_panel
            .is_some_and(|selection| selection.parent_session_id == session_id);
        if !subagent_navigation_surface_is_visible(
            has_visible_round,
            has_running_subagents,
            has_active_panel,
        ) {
            return Vec::new();
        }
        let normalized_states: Vec<SubagentTaskPanelState> = self
            .session_subagent_tasks
            .get(&session_id)
            .map(|entries| {
                entries
                    .values()
                    .map(|state| self.normalized_subagent_task_panel_state(state))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let requested_active_task_id = self.active_subagent_panel.and_then(|selection| {
            (selection.parent_session_id == session_id).then_some(selection.task_id)
        });
        select_subagent_navigation_panel_states(
            &normalized_states,
            requested_active_task_id,
            |state| self.subagent_task_panel_has_recoverable_detail(session_id, state.task_id),
        )
        .0
    }
    /// Starts one detached child-task listener stream exactly once so background subagent output can keep flowing after the parent run has already completed.
    /// 仅启动一次某条分离子任务监听流，使父运行结束后后台子代理输出仍可继续流入界面。
    pub(super) fn maybe_start_detached_subagent_task_listener(
        &mut self,
        parent_session_id: SessionId,
        event: &RuntimeEvent,
    ) -> Task<Message> {
        if event.kind != RuntimeEventKind::DelegateStarted {
            return Task::none();
        }
        let Some(metadata) = event.metadata.as_ref() else {
            return Task::none();
        };
        let Some(state) = parse_subagent_task_panel_metadata(parent_session_id, metadata) else {
            return Task::none();
        };
        if !self.detached_subagent_task_listeners.insert(state.task_id) {
            return Task::none();
        }
        let task_id = state.task_id;
        Task::run(
            self.runtime_foundation
                .task_runtime()
                .subscribe_detached_task_events(task_id),
            move |event| Message::DetachedSubagentTaskEvent { task_id, event },
        )
    }
    /// Opens one cached subagent task panel inside the main transcript area when the task state is available.
    /// 当任务状态可用时，在主转录区域内部打开一条缓存子代理任务面板。
    pub(super) fn open_subagent_task_panel(
        &mut self,
        parent_session_id: SessionId,
        task_id: SessionId,
    ) -> bool {
        self.rebuild_subagent_task_cache_for_session(parent_session_id);
        let Some(state) = self.subagent_task_panel_state(parent_session_id, task_id) else {
            return false;
        };
        let state = self.normalized_subagent_task_panel_state(state);
        if !subagent_task_panel_is_openable(
            &state,
            self.subagent_task_panel_has_recoverable_detail(parent_session_id, task_id),
        ) {
            return false;
        }

        self.clear_transcript_drag_auto_scrolls();
        self.active_thinking_modal = None;
        self.active_tool_modal = None;
        self.active_subagent_panel = Some(SelectedSubagentTaskPanel {
            parent_session_id,
            task_id,
        });
        true
    }
    /// Resolves the child task identifier attached to one rendered tool block when that block represents a cached task-session card.
    /// 当某个渲染后的工具区块代表已缓存 task 会话卡片时，解析其附着的子任务标识。
    pub(super) fn resolve_task_session_id_from_rendered_tool_block(
        &self,
        session_id: SessionId,
        block_id: &str,
    ) -> Option<SessionId> {
        let document = self.session_agent_view_documents.get(&session_id)?;
        for (block_index, block) in document.blocks.iter().enumerate() {
            let candidate_block_id = agent_view_block_id(session_id, block_index, block);
            if candidate_block_id != block_id {
                continue;
            }

            let vulcan_agent_view::AgentViewBlock::Directive(directive) = block else {
                return None;
            };
            return parse_session_id_text(
                directive
                    .attributes
                    .get("task-session-id")
                    .map(String::as_str),
            );
        }

        None
    }
    /// Resolves the canonical session message entry attached to one rendered tool block when the block still points back to a persisted transcript row.
    /// 当区块仍能回指到持久化转录行时，解析其对应的渲染工具区块所附着的规范会话消息条目。
    pub(super) fn resolve_rendered_tool_message_entry(
        &self,
        session_id: SessionId,
        block_id: &str,
    ) -> Option<&SessionMessageEntry> {
        let document = self.session_agent_view_documents.get(&session_id)?;
        for (block_index, block) in document.blocks.iter().enumerate() {
            let candidate_block_id = agent_view_block_id(session_id, block_index, block);
            if candidate_block_id != block_id {
                continue;
            }

            let vulcan_agent_view::AgentViewBlock::Directive(directive) = block else {
                return None;
            };
            let message_id = directive.attributes.get("message-id")?;
            return self
                .full_tool_detail_message_entry(session_id, message_id)
                .or_else(|| self.session_message_entry(session_id, message_id));
        }

        None
    }

    /// Resolves the full modal-facing tool message for one rendered tool block.
    /// 解析单个已渲染工具区块对应的完整弹层工具消息。
    pub(super) fn resolve_rendered_tool_modal_message_entry(
        &self,
        session_id: SessionId,
        block_id: &str,
    ) -> Option<SessionMessageEntry> {
        let message = self.resolve_rendered_tool_message_entry(session_id, block_id)?;
        Some(self.full_tool_modal_message_entry(session_id, message))
    }

    /// Resolves the rendered attribute map for one visible tool block so richer modal surfaces can reuse routing metadata without reparsing persisted JSON.
    /// 解析单个可见工具区块的已渲染属性映射，使更丰富的弹层表面能够复用路由元数据，而无需重新解析持久化 JSON。
    pub(super) fn resolve_rendered_tool_block_attributes(
        &self,
        session_id: SessionId,
        block_id: &str,
    ) -> Option<vulcan_agent_view::AgentViewAttributes> {
        let document = self.session_agent_view_documents.get(&session_id)?;
        for (block_index, block) in document.blocks.iter().enumerate() {
            let candidate_block_id = agent_view_block_id(session_id, block_index, block);
            if candidate_block_id != block_id {
                continue;
            }

            let vulcan_agent_view::AgentViewBlock::Directive(directive) = block else {
                return None;
            };
            return Some(directive.attributes.clone());
        }

        None
    }
}
