//! Task-tree and archive management methods extracted from the main workbench module.
//! 从主工作台模块中抽取的任务树与归档管理方法。

use super::helpers::*;
use super::*;
use crate::workspace_path::{
    normalize_workspace_path_text, normalized_workspace_path_text_from_path,
};

/// Icon kinds used by the hover action buttons inside the task-drawer tree.
/// 任务抽屉树悬停动作按钮使用的图标类型。
#[derive(Debug, Clone, Copy)]
pub(super) enum TaskTreeActionIcon {
    /// Adds one new project node by selecting a folder from disk.
    /// 通过从磁盘选择文件夹来新增一个项目节点。
    AddProjectFromFolder,
    /// Creates a new child session under the current project node.
    /// 在当前项目节点下创建新的子会话。
    AddSession,
    /// Archives the current session row.
    /// 归档当前会话行。
    Archive,
}

impl VulcanWorkbench {
    /// Returns the session summary matching one identifier, if it still exists.
    /// 返回与某个标识匹配的会话摘要（如果仍然存在）。
    pub(super) fn session_by_id(&self, session_id: SessionId) -> Option<&SessionSummary> {
        self.sessions
            .iter()
            .find(|session| session.id == session_id)
    }

    /// Resolves the effective workspace path owned by one session through its backing project node.
    /// 通过所属项目节点解析单个会话实际拥有的工作区路径。
    pub(super) fn workspace_path_for_session_id(&self, session_id: SessionId) -> Option<String> {
        let project_id = self.session_by_id(session_id)?.project_id.as_deref()?;
        self.task_project_by_id(project_id)
            .map(|project| normalize_workspace_path_text(&project.path))
    }

    /// Synchronizes the visible workspace-path field from one session when the session belongs to a project node.
    /// 当某个会话归属于项目节点时，根据该会话同步界面上可见的工作区路径字段。
    pub(super) fn sync_workspace_path_with_session(&mut self, session_id: SessionId) {
        if let Some(project_path) = self.workspace_path_for_session_id(session_id) {
            self.workspace_path = project_path;
        }
    }

    /// Synchronizes the visible workspace-path field from the currently selected session.
    /// 根据当前选中会话同步界面上可见的工作区路径字段。
    pub(super) fn sync_workspace_path_with_selected_session(&mut self) {
        self.sync_workspace_path_with_session(self.selected_session);
    }

    /// Returns the project node matching one identifier, if it still exists.
    /// 返回与某个标识匹配的项目节点（如果仍然存在）。
    pub(super) fn task_project_by_id(&self, project_id: &str) -> Option<&TaskProjectSummary> {
        self.task_projects
            .iter()
            .find(|project| project.id == project_id)
    }

    /// Returns the mutable project node matching one identifier, if it still exists.
    /// 返回与某个标识匹配的可变项目节点（如果仍然存在）。
    pub(super) fn task_project_by_id_mut(
        &mut self,
        project_id: &str,
    ) -> Option<&mut TaskProjectSummary> {
        self.task_projects
            .iter_mut()
            .find(|project| project.id == project_id)
    }

    /// Resolves which project currently owns the selected session so generic actions can target the right branch.
    /// 解析当前哪一个项目持有已选会话，以便通用动作能够命中正确分支。
    pub(super) fn selected_task_project_id(&self) -> Option<String> {
        self.task_projects
            .iter()
            .find(|project| project.session_ids.contains(&self.selected_session))
            .map(|project| project.id.clone())
    }

    /// Schedule one background cleanup pass for every currently known project temp directory.
    /// 为当前已知的全部项目临时目录安排一次后台清理。
    pub(super) fn schedule_project_temp_cleanup_for_all_projects(&self) -> Task<Message> {
        let mut project_paths = self
            .task_projects
            .iter()
            .chain(self.archived_task_projects.iter())
            .map(|project| PathBuf::from(project.path.clone()))
            .collect::<Vec<_>>();
        project_paths.sort();
        project_paths.dedup();
        self.schedule_project_temp_cleanup_for_paths(project_paths)
    }

    /// Schedule one background cleanup pass for the provided project-root list.
    /// 为给定项目根目录列表安排一次后台清理。
    pub(super) fn schedule_project_temp_cleanup_for_paths(
        &self,
        project_paths: Vec<PathBuf>,
    ) -> Task<Message> {
        Task::perform(
            async move { project_temp_maintenance::maintain_project_temp_dirs(&project_paths) },
            Message::ProjectTempMaintenanceFinished,
        )
    }

    /// Returns the next project identifier in the same pinned or regular section.
    /// 返回同一置顶或普通分区中的下一个项目标识。
    pub(super) fn next_task_project_in_section(&self, project_id: &str) -> Option<String> {
        let current_index = self
            .task_projects
            .iter()
            .position(|project| project.id == project_id)?;
        let section_is_pinned = self.task_projects.get(current_index)?.pinned;

        self.task_projects
            .iter()
            .skip(current_index + 1)
            .find(|project| project.pinned == section_is_pinned)
            .map(|project| project.id.clone())
    }

    /// Toggles the expanded state of one project node inside the main task tree.
    /// 切换主任务树中某个项目节点的展开状态。
    pub(super) fn toggle_task_project_expanded(&mut self, project_id: &str) {
        if !self.expanded_task_projects.insert(project_id.to_string()) {
            self.expanded_task_projects.remove(project_id);
        }
    }

    /// Sorts projects into pinned and regular sections while preserving each section order value.
    /// 按置顶区与普通区对项目进行排序，并保留各自分区内部的顺序值。
    pub(super) fn sort_task_projects(&mut self) {
        self.task_projects
            .sort_by_key(|project| (!project.pinned, project.order));
    }

    /// Reorders one dragged project before the hovered target when both belong to the same section.
    /// 当拖拽项目与悬停目标同属一个分区时，将前者移动到后者之前。
    pub(super) fn reorder_task_project_before(
        &mut self,
        dragged_project_id: &str,
        target_project_id: &str,
    ) {
        if dragged_project_id == target_project_id {
            return;
        }

        let Some(dragged_index) = self
            .task_projects
            .iter()
            .position(|project| project.id == dragged_project_id)
        else {
            return;
        };
        let Some(target_index) = self
            .task_projects
            .iter()
            .position(|project| project.id == target_project_id)
        else {
            return;
        };

        if self.task_projects[dragged_index].pinned != self.task_projects[target_index].pinned {
            return;
        }

        let dragged_project = self.task_projects.remove(dragged_index);
        let insert_index = if dragged_index < target_index {
            target_index.saturating_sub(1)
        } else {
            target_index
        };
        self.task_projects.insert(insert_index, dragged_project);
        self.normalize_task_project_orders();
    }

    /// Reorders one dragged project to the end of its current pinned or regular section.
    /// 将某个被拖拽的项目移动到其当前置顶或普通分区的末尾位置。
    pub(super) fn reorder_task_project_to_section_end(
        &mut self,
        dragged_project_id: &str,
        section_is_pinned: bool,
    ) {
        let Some(dragged_index) = self
            .task_projects
            .iter()
            .position(|project| project.id == dragged_project_id)
        else {
            return;
        };

        if self.task_projects[dragged_index].pinned != section_is_pinned {
            return;
        }

        let dragged_project = self.task_projects.remove(dragged_index);
        let insert_index = if section_is_pinned {
            self.task_projects
                .iter()
                .take_while(|project| project.pinned)
                .count()
        } else {
            self.task_projects.len()
        };

        self.task_projects.insert(insert_index, dragged_project);
        self.normalize_task_project_orders();
    }

    /// Applies one pending drop-target preview when a dragged project card is released.
    /// 在拖拽项目卡片释放时应用当前记录的插入目标预览。
    pub(super) fn apply_task_project_drop_target(
        &mut self,
        dragged_project_id: &str,
        drop_target: &TaskProjectDropTarget,
    ) {
        match drop_target {
            TaskProjectDropTarget::Before(target_project_id) => {
                self.reorder_task_project_before(dragged_project_id, target_project_id);
            }
            TaskProjectDropTarget::EndOfPinned => {
                self.reorder_task_project_to_section_end(dragged_project_id, true);
            }
            TaskProjectDropTarget::EndOfRegular => {
                self.reorder_task_project_to_section_end(dragged_project_id, false);
            }
        }
    }

    /// Rebuilds stable order numbers after one visible project reorder.
    /// 一次可见项目重排后重新生成稳定顺序编号。
    pub(super) fn normalize_task_project_orders(&mut self) {
        let mut pinned_order = 0usize;
        let mut regular_order = 0usize;

        for project in &mut self.task_projects {
            if project.pinned {
                project.order = pinned_order;
                pinned_order += 1;
            } else {
                project.order = regular_order;
                regular_order += 1;
            }
        }

        self.sort_task_projects();
    }

    /// Estimates a stable single-line pixel budget for one project title inside the task drawer header.
    /// 估算任务抽屉头部中单行项目标题可使用的稳定像素宽度预算。
    pub(super) fn main_task_project_title_limit_width(&self) -> f32 {
        let horizontal_padding = 22.0;
        let leading_folder_width = 16.0;
        let inter_item_spacing = 24.0;
        let trailing_width = MAIN_TASK_PROJECT_ACTIONS_WIDTH + MAIN_TASK_PROJECT_ARROW_WIDTH;
        let title_compensation = 26.0;
        (self.main_task_panel_width
            - horizontal_padding
            - leading_folder_width
            - inter_item_spacing
            - trailing_width
            - title_compensation)
            .max(144.0)
    }

    /// Estimates a stable single-line pixel budget for one session title inside the task drawer.
    /// 估算任务抽屉内部单条会话标题可使用的稳定单行像素宽度预算。
    pub(super) fn main_task_session_title_limit_width(&self) -> f32 {
        let trailing_slot_width = MAIN_TASK_SESSION_TRAILING_WIDTH;
        let structural_overhead = 84.0;
        (self.main_task_panel_width - trailing_slot_width - structural_overhead).max(112.0)
    }

    /// Creates one new session under the target project and updates all related prototype state.
    /// 在目标项目下创建一个新会话，并同步更新相关原型状态。
    pub(super) fn create_task_project_session(&mut self, project_id: &str) {
        self.prune_unmaterialized_sessions(None);

        let session_id = Uuid::new_v4();

        let project_title = self
            .task_project_by_id(project_id)
            .map(|project| project.title.clone())
            .unwrap_or_else(|| String::from("未命名项目"));

        self.sessions.insert(
            0,
            SessionSummary::new(
                session_id,
                Some(project_id.to_string()),
                "新对话",
                "等待首条真实用户输入后再正式生成会话标题与上下文。",
                SessionState::Draft,
                0,
                0,
                "刚刚",
            )
            .with_title_source(SessionTitleSource::Generated),
        );

        self.session_messages.entry(session_id).or_default();
        self.session_compactions.entry(session_id).or_default();
        self.session_runtime_state
            .entry(session_id)
            .or_insert_with(|| SessionRuntimeStateEntry {
                updated_at: current_unix_millis_text(),
                ..SessionRuntimeStateEntry::default()
            });
        self.session_runtime_events.entry(session_id).or_default();
        self.session_run_phases
            .insert(session_id, SessionRunPhase::Idle);
        // Fresh draft sessions are already fully materialized in memory, so the first submit
        // should not be blocked by an extra lazy-hydration round-trip.
        self.hydrated_session_ids.insert(session_id);
        self.expanded_task_projects.insert(project_id.to_string());
        self.selected_session = session_id;
        self.active_subagent_panel = None;
        self.hovered_task_project = Some(project_id.to_string());
        self.hovered_task_session = Some(session_id);
        self.suppress_task_session_hover = false;
        self.sync_workspace_path_with_session(session_id);
        self.status_message =
            format!("已在项目 `{project_title}` 下创建新的空白对话草稿，等待首条真实输入。");
        self.push_activity(
            ActivityLevel::Info,
            format!("创建新对话草稿 #{session_id}"),
            format!("项目 `{project_title}` 已创建一个未入列的新生会话，首条真实对话后才会正式加入列表。"),
        );
    }

    /// Adds one new project node from the selected folder path or focuses the existing node when it already exists.
    /// 从选中的文件夹路径新增一个项目节点，若已存在则聚焦已有节点。
    pub(super) fn add_task_project_from_folder(&mut self, selected_path: &Path) {
        let normalized_path = normalize_task_project_path(selected_path);
        let normalized_path_text = normalized_workspace_path_text_from_path(&normalized_path);

        if let Some(existing_project) = self.task_projects.iter().find(|project| {
            normalized_workspace_path_text_from_path(&normalize_task_project_path(Path::new(
                &project.path,
            ))) == normalized_path_text
        }) {
            self.expanded_task_projects
                .insert(existing_project.id.clone());
            self.hovered_task_project = Some(existing_project.id.clone());
            self.status_message = format!(
                "项目 `{}` 已存在，已定位到当前节点。",
                existing_project.title
            );
            return;
        }

        if let Some(archived_index) = self.archived_task_projects.iter().position(|project| {
            normalized_workspace_path_text_from_path(&normalize_task_project_path(Path::new(
                &project.path,
            ))) == normalized_path_text
        }) {
            let restored_project_id = self.archived_task_projects[archived_index].id.clone();
            let _ = self.restore_archived_task_project_by_id(&restored_project_id);
            return;
        }

        let title = normalized_path
            .file_name()
            .and_then(|name| name.to_str())
            .filter(|name| !name.trim().is_empty())
            .map(|name| name.to_string())
            .unwrap_or_else(|| normalized_path_text.clone());
        let order = self
            .task_projects
            .iter()
            .filter(|project| !project.pinned)
            .map(|project| project.order)
            .max()
            .map(|order| order + 1)
            .unwrap_or(0);
        let project_id = self.build_task_project_id(&title, &normalized_path_text);

        self.task_projects.push(TaskProjectSummary::new(
            project_id.clone(),
            title.clone(),
            normalized_path_text.clone(),
            false,
            order,
            None,
            None,
            vec![],
        ));
        self.normalize_task_project_orders();
        self.expanded_task_projects.insert(project_id.clone());
        self.hovered_task_project = Some(project_id);
        self.status_message = format!("已从文件夹新增项目 `{title}`。");
        self.push_activity(
            ActivityLevel::Info,
            format!("新增项目 `{title}`"),
            format!("已将目录 `{normalized_path_text}` 添加到项目树。"),
        );
    }

    /// Ensures the selected session still points to a valid session after tree-level archive operations.
    /// 在树级归档操作后确保当前选中会话仍然指向有效节点。
    pub(super) fn ensure_valid_selected_session(&mut self) {
        if self
            .sessions
            .iter()
            .all(|session| session.id != self.selected_session)
        {
            self.selected_session = self
                .sessions
                .first()
                .map(|session| session.id)
                .unwrap_or_default();
            self.active_subagent_panel = None;
        }
        self.sync_workspace_path_with_selected_session();
    }

    /// Builds one stable project identifier from a title and path while avoiding collisions in the current tree.
    /// 基于标题与路径构建稳定项目标识，并避免与当前树中的已有节点冲突。
    pub(super) fn build_task_project_id(&self, title: &str, normalized_path_text: &str) -> String {
        let base_slug = slugify_task_project_title(title);
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        normalized_path_text.hash(&mut hasher);
        let path_suffix = format!("{:x}", hasher.finish());
        let path_suffix = &path_suffix[..path_suffix.len().min(8)];

        let candidate = if base_slug.is_empty() {
            format!("project-{path_suffix}")
        } else {
            format!("{base_slug}-{path_suffix}")
        };

        if self.task_project_by_id(&candidate).is_none() {
            return candidate;
        }

        let mut counter = 2usize;
        loop {
            let next_candidate = format!("{candidate}-{counter}");
            if self.task_project_by_id(&next_candidate).is_none() {
                return next_candidate;
            }
            counter += 1;
        }
    }

    /// Restores one archived project node together with all persisted session artifacts back into the visible tree.
    /// 将一个归档项目节点连同其全部持久化会话工件恢复回可见树。
    pub(super) fn restore_archived_task_project_by_id(&mut self, project_id: &str) -> bool {
        let Some(archived_index) = self
            .archived_task_projects
            .iter()
            .position(|project| project.id == project_id)
        else {
            return false;
        };

        let restored_project = self.archived_task_projects.remove(archived_index);
        let restored_session_ids = restored_project.session_ids.clone();
        let restored_title = restored_project.title.clone();
        let restored_project_id = restored_project.id.clone();

        for session_id in &restored_session_ids {
            if let Some(index) = self
                .archived_sessions
                .iter()
                .position(|session| session.id == *session_id)
            {
                let restored_session = self.archived_sessions.remove(index);
                self.sessions.push(restored_session);
            }

            if let Some(messages) = self.archived_session_messages.remove(session_id) {
                self.session_messages.insert(*session_id, messages);
            }
            if let Some(compactions) = self.archived_session_compactions.remove(session_id) {
                self.session_compactions.insert(*session_id, compactions);
            }
            if let Some(runtime_state) = self.archived_session_runtime_state.remove(session_id) {
                self.session_runtime_state
                    .insert(*session_id, runtime_state);
            }
            self.session_run_phases
                .insert(*session_id, SessionRunPhase::Idle);
            self.session_runtime_events.remove(session_id);
            self.refresh_session_agent_view_buffers(*session_id);
        }

        self.task_projects.push(restored_project);
        self.normalize_task_project_orders();
        self.expanded_task_projects
            .insert(restored_project_id.clone());
        self.hovered_task_project = Some(restored_project_id);
        if let Some(restored_session_id) = restored_session_ids.first().copied() {
            self.selected_session = restored_session_id;
            self.active_subagent_panel = None;
            self.hovered_task_session = Some(restored_session_id);
            self.sync_workspace_path_with_session(restored_session_id);
        }
        self.ensure_valid_selected_session();
        self.status_message = format!("已恢复归档项目 `{restored_title}`。");
        self.push_activity(
            ActivityLevel::Info,
            format!("恢复项目 `{restored_title}`"),
            format!("项目 `{restored_title}` 及其会话已恢复到可见工作台树。"),
        );
        true
    }

    /// Restores one archived session row back into its still-visible owning project.
    /// 将一个归档会话行恢复回其仍然可见的所属项目。
    pub(super) fn restore_archived_task_session(&mut self, session_id: SessionId) -> bool {
        let Some(archived_index) = self
            .archived_sessions
            .iter()
            .position(|session| session.id == session_id)
        else {
            return false;
        };

        let restored_session = self.archived_sessions.remove(archived_index);
        let restored_title = restored_session.title.clone();
        let Some(project_id) = restored_session.project_id.clone() else {
            self.archived_sessions
                .insert(archived_index, restored_session);
            return false;
        };

        {
            let Some(project) = self.task_project_by_id_mut(&project_id) else {
                self.archived_sessions
                    .insert(archived_index, restored_session);
                self.status_message =
                    format!("无法恢复会话 `{restored_title}`，因为所属项目当前不可见。");
                return false;
            };

            project.session_ids.insert(0, session_id);
        }
        self.sessions.insert(0, restored_session);
        if let Some(messages) = self.archived_session_messages.remove(&session_id) {
            self.session_messages.insert(session_id, messages);
        }
        if let Some(compactions) = self.archived_session_compactions.remove(&session_id) {
            self.session_compactions.insert(session_id, compactions);
        }
        if let Some(runtime_state) = self.archived_session_runtime_state.remove(&session_id) {
            self.session_runtime_state.insert(session_id, runtime_state);
        }
        self.session_run_phases
            .insert(session_id, SessionRunPhase::Idle);
        self.session_runtime_events.remove(&session_id);
        self.refresh_session_agent_view_buffers(session_id);
        self.expanded_task_projects.insert(project_id);
        self.selected_session = session_id;
        self.active_subagent_panel = None;
        self.hovered_task_session = Some(session_id);
        self.sync_workspace_path_with_session(session_id);
        self.ensure_valid_selected_session();
        self.status_message = format!("已恢复归档会话 `{restored_title}`。");
        self.push_activity(
            ActivityLevel::Info,
            format!("恢复会话 `{restored_title}`"),
            format!("归档会话 `{restored_title}` 已恢复到所属项目列表。"),
        );
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies creating a session under a later project also switches the effective workspace path to that project.
    /// 验证在后续项目下创建会话时，会同步把生效工作区路径切换到该项目路径。
    #[test]
    fn create_task_project_session_switches_workspace_path_to_project() {
        let (mut workbench, _task) = VulcanWorkbench::new();
        workbench.sessions.clear();
        workbench.task_projects.clear();
        workbench.workspace_path = String::from(r"D:\workspace\project-a");
        workbench.task_projects.push(TaskProjectSummary::new(
            "project-a",
            "项目 A",
            r"D:\workspace\project-a",
            false,
            0,
            None,
            None,
            Vec::new(),
        ));
        workbench.task_projects.push(TaskProjectSummary::new(
            "project-b",
            "项目 B",
            r"D:\workspace\project-b",
            false,
            1,
            None,
            None,
            Vec::new(),
        ));

        workbench.create_task_project_session("project-b");

        assert_eq!(workbench.workspace_path, r"D:\workspace\project-b");
        assert_eq!(
            workbench
                .session_by_id(workbench.selected_session)
                .and_then(|session| session.project_id.as_deref()),
            Some("project-b")
        );
        assert!(
            workbench
                .hydrated_session_ids
                .contains(&workbench.selected_session)
        );
    }
}
