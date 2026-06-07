//! Workbench workspace and navigation state extracted from the main workbench module.
//! 从主工作台模块中抽取的工作区与导航状态。

use super::*;

/// Startup lifecycle shown by the splash and bootstrap overlay.
/// 启动闪屏与引导遮罩显示的启动生命周期。
#[derive(Debug, Clone)]
pub(super) enum StartupLifecycle {
    /// The application is ready and no bootstrap overlay should be shown.
    /// 应用已就绪，不应显示引导遮罩。
    Ready,
    /// One startup bootstrap is still in progress.
    /// 一次启动引导仍在进行中。
    Bootstrapping { message: String },
    /// Bootstrap failed and the failure text should be rendered to the user.
    /// 引导失败，应该向用户展示失败文本。
    Failed { message: String },
}

/// Transcript surface mode used by the selected session view.
/// 选中会话视图使用的转录表面模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum AgentTranscriptSurfaceMode {
    /// Legacy segmented rendered-preview stream kept as the primary per-block visual inspection surface.
    /// 作为首要逐块视觉检查表面的旧式分段渲染流。
    DetailCards,
    /// Selectable raw-source projection backed by the unified agent-view document.
    /// 由统一 agent-view 文档支撑的可选原始源码投影。
    RawSource,
    /// Read-only preview of the last full provider request captured before one real API call starts.
    /// 在一次真实 API 调用开始前捕获的最后一次完整 Provider 请求只读预览。
    ApiRequest,
}

/// Context-injection side panel tab used to separate expiring and persistent context.
/// 上下文注入侧栏页签，用于区分时效上下文与静态上下文。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ContextInjectionPanelTab {
    /// Shows turn-TTL dynamic injections such as recalled memories.
    /// 显示按回合 TTL 失效的动态注入，例如召回记忆。
    Dynamic,
    /// Shows persistent context such as the cached profile baseline and profile deltas.
    /// 显示持久上下文，例如缓存画像基点与画像增量。
    Static,
}

impl Default for ContextInjectionPanelTab {
    /// Returns the default tab because most live checks focus on expiring context.
    /// 返回默认页签，因为多数实时检查关注时效上下文。
    fn default() -> Self {
        Self::Dynamic
    }
}

/// Stable scope selection used by the left-panel workspace-history search.
/// 左侧面板工作台历史搜索使用的稳定范围选择。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum WorkspaceHistorySearchScope {
    /// Searches across every visible project and session.
    /// 跨全部可见项目与会话执行搜索。
    Global,
    /// Restricts search to one concrete visible project identifier.
    /// 将搜索限制到某个具体的可见项目标识。
    Project(String),
}

impl Default for WorkspaceHistorySearchScope {
    /// Returns the default global scope used before one session-derived project scope is chosen.
    /// 返回在根据当前会话推导项目范围前使用的默认全局范围。
    fn default() -> Self {
        Self::Global
    }
}

/// One selectable scope option rendered inside the left-panel workspace-history project dropdown.
/// 渲染在左侧工作台历史项目下拉列表中的单条可选范围项。
#[derive(Debug, Clone)]
pub(super) struct WorkspaceHistorySearchScopeOption {
    /// Stable scope value applied when this option is selected.
    /// 选中该项后会应用的稳定范围值。
    pub(super) scope: WorkspaceHistorySearchScope,
    /// Human-readable option label shown to the user.
    /// 展示给用户的人类可读选项标签。
    pub(super) label: String,
    /// Searchable helper text used by the project keyword filter.
    /// 供项目关键字过滤使用的可搜索辅助文本。
    pub(super) filter_text: String,
}

/// Pending transcript navigation target retained while one search result is opening or hydrating.
/// 当某条搜索结果正在打开或补载时保留的待定位转录目标。
#[derive(Debug, Clone)]
pub(super) struct PendingWorkspaceHistorySearchNavigation {
    /// Stable session identifier owning the matched message row.
    /// 拥有该命中消息行的稳定会话标识。
    pub(super) session_id: SessionId,
    /// Stable message identifier that should become visible inside the transcript.
    /// 应在转录中可见的稳定消息标识。
    pub(super) message_id: String,
    /// Stable ordinal used to decide whether older history still needs loading.
    /// 用于判断是否仍需补载更早历史的稳定序号。
    pub(super) ordinal: i64,
    /// Stable turn index used when reporting search-navigation context to the user.
    /// 在向用户回报搜索定位上下文时使用的稳定轮次索引。
    pub(super) turn_index: i64,
}

/// Active transcript highlight target retained after one workspace-history search hit has been opened successfully.
/// 某条工作台历史搜索命中成功打开后保留的活动转录高亮目标。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct WorkspaceHistorySearchHighlightTarget {
    /// Stable session identifier owning the highlighted visible message row.
    /// 拥有当前高亮可见消息行的稳定会话标识。
    pub(super) session_id: SessionId,
    /// Stable visible message identifier that should stay highlighted inside the transcript.
    /// 应在转录中保持高亮的稳定可见消息标识。
    pub(super) message_id: String,
}

/// Aggregated state retained by the left-panel workspace-history search UI.
/// 左侧面板工作台历史搜索界面保留的聚合状态。
#[derive(Debug, Clone, Default)]
pub(super) struct WorkspaceHistorySearchPanelState {
    /// Whether the dedicated search panel is currently expanded.
    /// 当前专用搜索面板是否处于展开状态。
    pub(super) panel_open: bool,
    /// Current query text shown inside the search input.
    /// 显示在搜索输入框中的当前查询文本。
    pub(super) query: String,
    /// Latest search result page returned by the BM25 workspace query.
    /// BM25 工作区查询返回的最近一页搜索结果。
    pub(super) results: Option<WorkspaceHistorySearchPage>,
    /// Whether one workspace-history search request is currently in flight.
    /// 当前是否有一条工作台历史搜索请求正在执行。
    pub(super) loading: bool,
    /// Whether the current in-flight search request is loading an additional result page.
    /// 当前正在执行中的搜索请求是否属于继续加载下一页结果。
    pub(super) loading_more: bool,
    /// Latest search error shown inline to the user, if any.
    /// 若存在，则为当前内联展示给用户的最近一次搜索错误。
    pub(super) error: Option<String>,
    /// Current visible search scope.
    /// 当前可见搜索范围。
    pub(super) scope: WorkspaceHistorySearchScope,
    /// Whether the project selector dropdown is currently expanded.
    /// 当前项目选择下拉区是否处于展开状态。
    pub(super) project_picker_open: bool,
    /// Current project keyword filter text.
    /// 当前项目关键字过滤文本。
    pub(super) project_filter: String,
    /// Optional pending transcript navigation target created from one clicked search result.
    /// 由某条已点击搜索结果创建的可选待定位转录目标。
    pub(super) pending_navigation: Option<PendingWorkspaceHistorySearchNavigation>,
    /// Optional active transcript highlight target retained after one search result has been resolved into the visible transcript.
    /// 某条搜索结果已经成功解析到可见转录后保留的可选活动高亮目标。
    pub(super) active_highlight: Option<WorkspaceHistorySearchHighlightTarget>,
}

/// Shared prepared inline transcript document reused by the main session surface without recloning the full block tree on every GUI pass.
/// 主会话表面复用的共享内联转录文档快照，避免每次界面刷新都重新深拷贝整棵区块树。
#[derive(Debug, Clone)]
pub(super) struct SessionInlineAgentViewDocumentCache {
    /// Shared prepared inline transcript document reused by the main session surface without recloning the full block tree on every GUI pass.
    /// 主会话表面复用的共享内联转录文档快照，避免每次界面刷新都重新深拷贝整棵区块树。
    pub(super) document: Rc<vulcan_agent_view::AgentViewDocument>,
}

/// Prompt-resource watch event category used to avoid broad cache invalidation.
/// 提示词资源监听事件类别，用于避免过宽的缓存失效。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PromptResourceWatchKind {
    /// Workspace-local project prompt file changed.
    /// 工作区本地项目提示词文件发生变化。
    ProjectPrompt,
    /// Workspace-local project prompt configuration changed.
    /// 工作区本地项目提示词配置发生变化。
    ProjectPromptConfig,
    /// User-level prompt composer resources changed.
    /// 用户级提示词组合资源发生变化。
    UserPromptResources,
}

/// Stable archived-item deletion target retained while the user is going through the destructive second confirmation.
/// 当用户正在经过破坏性二次确认流程时保留的稳定归档项删除目标。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ArchivedDeleteTarget {
    /// Permanently deletes one archived project together with all child archived sessions.
    /// 彻底删除一个归档项目及其全部子归档会话。
    Project(String),
    /// Permanently deletes one archived session row.
    /// 彻底删除一条归档会话行。
    Session(SessionId),
}

/// Pending destructive archived-item deletion confirmation shown inline inside the archived area.
/// 显示在归档区域内部的待确认破坏性归档项删除状态。
#[derive(Debug, Clone)]
pub(super) struct ArchivedDeleteConfirmationState {
    /// Stable deletion target that will be removed permanently after the second confirmation.
    /// 完成二次确认后将被永久移除的稳定删除目标。
    pub(super) target: ArchivedDeleteTarget,
    /// Human-readable title echoed back to the user in the destructive confirmation copy.
    /// 在破坏性确认文案中回显给用户的人类可读标题。
    pub(super) title: String,
}

/// Result payload returned after one archived-item hard-delete request finishes.
/// 单次归档项硬删除请求完成后返回的结果载荷。
#[derive(Debug, Clone)]
pub(super) struct ArchivedDeleteOutcome {
    /// Stable deletion target that has already been removed from persistence.
    /// 已经从持久化层移除的稳定删除目标。
    pub(super) target: ArchivedDeleteTarget,
    /// Human-readable title used for status and confirmation messages.
    /// 用于状态与确认文案的人类可读标题。
    pub(super) title: String,
}

/// Drop-target previews used while the task-tree project cards are being reordered.
/// 任务树项目卡片重排期间使用的插入目标预览类型。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum TaskProjectDropTarget {
    /// Inserts the dragged project before one concrete project row.
    /// 将正在拖拽的项目插入到某个具体项目行之前。
    Before(String),
    /// Inserts the dragged pinned project at the end of the pinned section.
    /// 将正在拖拽的置顶项目插入到置顶分区末尾。
    EndOfPinned,
    /// Inserts the dragged regular project at the end of the regular section.
    /// 将正在拖拽的普通项目插入到普通分区末尾。
    EndOfRegular,
}

/// Drop-target previews used while pending-prompt queue rows are being reordered.
/// 待发送队列行重排期间使用的插入目标预览类型。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum PendingPromptDropTarget {
    /// Inserts the dragged queue entry before one concrete queue row.
    /// 将正在拖拽的队列项插入到某个具体队列行之前。
    Before(String),
    /// Inserts the dragged queue entry at the tail of the current queue.
    /// 将正在拖拽的队列项插入到当前队列尾部。
    End,
}

/// Drop-target previews used while enabled AI capability targets are being reordered.
/// 已启用 AI 能力目标重排期间使用的插入目标预览类型。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum AiCapabilityRouteDropTarget {
    /// Inserts the dragged capability target before one concrete enabled row.
    /// 将正在拖拽的能力目标插入到某个具体已启用行之前。
    Before(String),
    /// Inserts the dragged capability target at the end of the current capability route.
    /// 将正在拖拽的能力目标插入到当前能力路由尾部。
    End,
}

impl VulcanWorkbench {
    /// Returns the default search scope derived from the currently selected session, or global when no visible project is active.
    /// 返回根据当前选中会话推导出的默认搜索范围；若当前没有可见项目会话则回退到全局。
    pub(super) fn default_workspace_history_search_scope(&self) -> WorkspaceHistorySearchScope {
        self.selected_session()
            .and_then(|session| session.project_id.clone())
            .filter(|project_id| self.task_project_by_id(project_id).is_some())
            .map(WorkspaceHistorySearchScope::Project)
            .unwrap_or(WorkspaceHistorySearchScope::Global)
    }

    /// Returns the human-readable label shown by the current workspace-history scope selector.
    /// 返回当前工作台历史范围选择器显示的人类可读标签。
    pub(super) fn workspace_history_search_scope_label(&self) -> String {
        match &self.workspace_history_search.scope {
            WorkspaceHistorySearchScope::Global => String::from("全部项目"),
            WorkspaceHistorySearchScope::Project(project_id) => self
                .task_project_by_id(project_id)
                .map(|project| project.title.clone())
                .unwrap_or_else(|| String::from("全部项目")),
        }
    }

    /// Builds the full visible scope-option list used by the left-panel workspace-history search picker.
    /// 构建左侧工作台历史搜索选择器使用的完整可见范围选项列表。
    pub(super) fn workspace_history_search_scope_options(
        &self,
    ) -> Vec<WorkspaceHistorySearchScopeOption> {
        let mut options = vec![WorkspaceHistorySearchScopeOption {
            scope: WorkspaceHistorySearchScope::Global,
            label: String::from("全部项目"),
            filter_text: String::from("全部项目 global all"),
        }];

        options.extend(self.task_projects.iter().map(|project| {
            WorkspaceHistorySearchScopeOption {
                scope: WorkspaceHistorySearchScope::Project(project.id.clone()),
                label: project.title.clone(),
                filter_text: format!("{} {}", project.title, project.path).to_lowercase(),
            }
        }));

        options
    }

    /// Returns the scope options filtered by the current project keyword text inside the left search panel.
    /// 返回根据左侧搜索面板当前项目关键字文本过滤后的范围选项集合。
    pub(super) fn filtered_workspace_history_search_scope_options(
        &self,
    ) -> Vec<WorkspaceHistorySearchScopeOption> {
        let filter = self
            .workspace_history_search
            .project_filter
            .trim()
            .to_lowercase();
        let options = self.workspace_history_search_scope_options();
        if filter.is_empty() {
            return options;
        }

        options
            .into_iter()
            .filter(|option| option.filter_text.contains(&filter))
            .collect()
    }

    /// Opens the dedicated left-panel workspace-history search surface and focuses the main query input.
    /// 打开左侧专用工作台历史搜索表面，并聚焦主查询输入框。
    pub(super) fn open_workspace_history_search_panel(&mut self) -> Task<Message> {
        if !WORKSPACE_HISTORY_SEARCH_ENABLED {
            self.workspace_history_search = WorkspaceHistorySearchPanelState::default();
            self.status_message = String::from("历史搜索已下线，后续将按新全文搜索方案重新接入。");
            return Task::none();
        }
        let should_reset_scope = !self.workspace_history_search.panel_open
            || matches!(
                &self.workspace_history_search.scope,
                WorkspaceHistorySearchScope::Project(project_id)
                    if self.task_project_by_id(project_id).is_none()
            );
        if should_reset_scope {
            self.workspace_history_search.scope = self.default_workspace_history_search_scope();
        }
        self.workspace_history_search.panel_open = true;
        self.workspace_history_search.project_picker_open = false;
        self.workspace_history_search.project_filter.clear();

        operation::focus(WORKSPACE_HISTORY_SEARCH_INPUT_ID)
    }

    /// Continues one pending search-result navigation by hydrating older history until the target message becomes visible.
    /// 继续推进一条待完成的搜索结果定位流程，按需补载更早历史直到目标消息可见。
    pub(super) fn maybe_continue_workspace_history_search_navigation(
        &mut self,
        session_id: SessionId,
    ) -> Task<Message> {
        let Some(pending_navigation) = self
            .workspace_history_search
            .pending_navigation
            .clone()
            .filter(|pending| pending.session_id == session_id)
        else {
            return Task::none();
        };

        if self.selected_session().map(|session| session.id) != Some(session_id) {
            return Task::none();
        }

        self.session_agent_view_follow_tail
            .insert(session_id, false);

        if self
            .session_message_entry(session_id, &pending_navigation.message_id)
            .is_some()
        {
            self.workspace_history_search.active_highlight =
                Some(WorkspaceHistorySearchHighlightTarget {
                    session_id,
                    message_id: pending_navigation.message_id.clone(),
                });
            self.workspace_history_search.pending_navigation = None;
            self.refresh_session_inline_agent_view_document_cache(session_id);
            self.status_message = format!(
                "已定位并高亮搜索命中：轮次 {} · 会话 {}。",
                pending_navigation.turn_index,
                self.selected_session()
                    .map(|session| session.title.clone())
                    .unwrap_or_else(|| String::from("未知会话"))
            );
            return operation::scroll_to(
                transcript_scrollable_id(session_id),
                scrollable::AbsoluteOffset {
                    x: 0.0,
                    y: self.estimated_workspace_history_search_scroll_offset(
                        session_id,
                        &pending_navigation.message_id,
                    ),
                },
            );
        }

        if self.loading_session_ids.contains(&session_id)
            || self.loading_older_session_history_ids.contains(&session_id)
        {
            return Task::none();
        }

        if !self.hydrated_session_ids.contains(&session_id) {
            self.status_message = String::from("正在载入目标会话正文以定位搜索命中。");
            return self.schedule_session_hydration(session_id);
        }

        if self
            .session_history_earliest_ordinals
            .get(&session_id)
            .copied()
            .is_some_and(|earliest_ordinal| pending_navigation.ordinal >= earliest_ordinal)
        {
            self.workspace_history_search.active_highlight = None;
            self.workspace_history_search.pending_navigation = None;
            self.refresh_session_inline_agent_view_document_cache(session_id);
            self.status_message = String::from("已打开目标会话，但当前窗口中未恢复出该搜索命中。");
            return Task::none();
        }

        if self.session_history_has_more.contains(&session_id) {
            self.status_message = String::from("正在补载更早会话历史以定位搜索命中。");
            return self.schedule_older_session_history_load(session_id);
        }

        self.workspace_history_search.active_highlight = None;
        self.workspace_history_search.pending_navigation = None;
        self.refresh_session_inline_agent_view_document_cache(session_id);
        self.status_message = String::from("已加载完可见历史，但仍未定位到该搜索命中。");
        Task::none()
    }

    /// Estimates the transcript scroll offset needed to bring one searched message close to the viewport center.
    /// 估算将某条搜索命中消息带到视口附近所需的转录滚动偏移量。
    pub(super) fn estimated_workspace_history_search_scroll_offset(
        &self,
        session_id: SessionId,
        message_id: &str,
    ) -> f32 {
        let message_index = self
            .visible_session_messages_for_agent_view(session_id)
            .iter()
            .position(|message| message.id == message_id)
            .or_else(|| {
                self.session_messages.get(&session_id).and_then(|messages| {
                    messages.iter().position(|message| message.id == message_id)
                })
            })
            .unwrap_or(0);

        (message_index as f32 * WORKSPACE_HISTORY_SEARCH_SCROLL_ESTIMATED_ROW_HEIGHT - 48.0)
            .max(0.0)
    }

    /// Schedules one workspace-scoped BM25 history search for the left task panel.
    /// 为左侧任务面板调度一次工作区级 BM25 历史搜索。
    pub(super) fn schedule_workspace_history_search(&mut self, append: bool) -> Task<Message> {
        if !WORKSPACE_HISTORY_SEARCH_ENABLED {
            self.workspace_history_search = WorkspaceHistorySearchPanelState::default();
            self.status_message = String::from("历史搜索已下线，后续将按新全文搜索方案重新接入。");
            return Task::none();
        }
        if self.workspace_history_search.loading {
            return Task::none();
        }
        let Some(controller_runtime) = self.controller_runtime.clone() else {
            return Task::none();
        };

        self.workspace_history_search.loading = true;
        self.workspace_history_search.loading_more = append;
        let query = self.workspace_history_search.query.clone();
        let project_scope = match &self.workspace_history_search.scope {
            WorkspaceHistorySearchScope::Global => None,
            WorkspaceHistorySearchScope::Project(project_id) => Some(project_id.clone()),
        };
        let offset = if append {
            self.workspace_history_search
                .results
                .as_ref()
                .map(|page| page.next_offset)
                .unwrap_or(0)
        } else {
            0
        };

        Task::perform(
            async move {
                workspace_store::search_workspace_history(
                    controller_runtime,
                    project_scope,
                    query,
                    SESSION_HISTORY_SEARCH_RESULT_LIMIT,
                    offset,
                )
                .await
            },
            move |result| Message::WorkspaceHistorySearchLoaded { result, append },
        )
    }
}
