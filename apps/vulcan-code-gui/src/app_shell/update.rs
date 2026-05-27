//! Workbench message dispatch loop extracted from the main application module.
//! 从主应用模块中抽取的工作台消息分发循环。

use super::*;

impl VulcanWorkbench {
    /// Handles widget messages and mutates the application state.
    /// 处理控件消息并更新应用状态。
    pub(super) fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Noop => Task::none(),
            Message::BeginStartupBootstrap => {
                mark_startup_trace("message:BeginStartupBootstrap");
                self.startup_lifecycle = StartupLifecycle::Bootstrapping {
                    message: self.t(I18nKey::StatusInitializingDataPlane),
                };
                self.status_message = self.t(I18nKey::StatusInitializingDataPlane);
                self.controller_runtime = None;
                self.luaskills_runtime = None;
                if let Some(runtime_access) = self.mcp_runtime.take() {
                    runtime_access.shutdown();
                }
                self.luaskills_catalog = None;
                self.luaskills_notice = String::from("LuaSkills 面板尚未加载技能目录。");
                self.luaskills_install_source_choice = LuaSkillsInstallSourceChoice::Github;
                self.luaskills_install_skill_id.clear();
                self.luaskills_install_source.clear();
                self.luaskills_operation_in_flight = false;
                self.luaskills_operation_progress = None;
                self.task_projects.clear();
                self.archived_task_projects.clear();
                self.sessions.clear();
                self.session_messages.clear();
                self.session_tool_detail_messages.clear();
                self.archived_sessions.clear();
                self.archived_session_messages.clear();
                self.session_compactions.clear();
                self.archived_session_compactions.clear();
                self.session_runtime_state.clear();
                self.archived_session_runtime_state.clear();
                self.hydrated_session_ids.clear();
                self.dirty_session_payload_ids.clear();
                self.dirty_session_message_ids.clear();
                self.loading_session_ids.clear();
                self.session_history_earliest_ordinals.clear();
                self.session_history_has_more.clear();
                self.loading_older_session_history_ids.clear();
                self.workspace_history_search = WorkspaceHistorySearchPanelState::default();
                self.background_legacy_maintenance_queue.clear();
                self.background_legacy_maintenance_in_flight = None;
                self.background_legacy_maintenance_completed_count = 0;
                self.background_legacy_maintenance_failed_count = 0;
                self.pending_archived_delete_confirmation = None;
                self.session_runtime_events.clear();
                self.collapsed_runtime_event_ids.clear();
                self.markdown_session_message_ids.clear();
                self.markdown_runtime_event_ids.clear();
                self.session_message_raw_buffers.clear();
                self.session_message_markdown.clear();
                self.session_message_markdown_buffers.clear();
                self.streaming_thinking_message_ids.clear();
                self.floating_streaming_thinking_sessions.clear();
                self.session_streaming_thinking_follow_tail.clear();
                self.active_thinking_modal = None;
                self.active_tool_modal = None;
                self.session_subagent_tasks.clear();
                self.session_subagent_navigation_rounds.clear();
                self.active_subagent_panel = None;
                self.subagent_navigation_expanded = false;
                self.detached_subagent_task_listeners.clear();
                self.streaming_output_message_ids.clear();
                self.runtime_event_raw_buffers.clear();
                self.runtime_event_markdown.clear();
                self.runtime_event_markdown_buffers.clear();
                self.session_agent_view_buffers.clear();
                self.session_agent_view_raw_buffers.clear();
                self.session_last_api_request_buffers.clear();
                self.session_last_api_request_markdowns.clear();
                self.session_agent_view_selectable_documents.clear();
                self.session_agent_view_documents.clear();
                self.session_inline_agent_view_documents.clear();
                self.session_agent_view_block_collapsed.clear();
                self.session_agent_view_follow_tail.clear();
                self.session_agent_view_viewports.clear();
                self.pending_transcript_follow_tail_session_ids.clear();
                self.session_streaming_thinking_viewports.clear();
                self.pending_thinking_overlay_follow_tail_session_ids
                    .clear();
                self.active_transcript_drag_auto_scroll = None;
                self.active_thinking_overlay_drag_auto_scroll = None;
                self.agent_transcript_surface_mode = AgentTranscriptSurfaceMode::DetailCards;
                self.session_run_phases.clear();
                self.active_runs.clear();
                self.active_session_compactions.clear();
                self.pending_provider_response_sessions.clear();
                self.runtime_progress_animation_frame = 0;
                self.expanded_task_projects.clear();
                self.selected_session = SessionId::nil();

                Task::perform(
                    data_plane::bootstrap_application_data_plane(
                        self.controller_configuration.clone(),
                    ),
                    Message::StartupBootstrapFinished,
                )
            }
            Message::StartupBootstrapFinished(result) => {
                mark_startup_trace("message:StartupBootstrapFinished");
                match result {
                    Ok(report) => {
                        self.status_message = report.summary_text();
                        self.activities.push(ActivityEntry {
                            level: ActivityLevel::Success,
                            title: String::from("vldb 数据平面就绪"),
                            detail: format!(
                                "控制器端点 {} 已连接，主 SQLite 空间已初始化到 {}。",
                                report.handle.endpoint,
                                report.handle.sqlite_path.display()
                            ),
                        });
                        self.controller_runtime = Some(report.handle);
                        self.startup_lifecycle = StartupLifecycle::Bootstrapping {
                            message: self.t(I18nKey::StatusInitializingLuaSkills),
                        };
                        self.status_message = self.t(I18nKey::StatusInitializingLuaSkills);

                        return Task::perform(
                            luaskills_runtime::bootstrap_luaskills_runtime(
                                self.controller_configuration.clone(),
                            ),
                            Message::LuaSkillsBootstrapFinished,
                        );
                    }
                    Err(error) => {
                        self.status_message = format!("vldb 数据平面初始化失败：{error}");
                        self.activities.push(ActivityEntry {
                            level: ActivityLevel::Warning,
                            title: String::from("vldb 数据平面初始化失败"),
                            detail: error.clone(),
                        });
                        self.controller_runtime = None;
                        self.startup_lifecycle = StartupLifecycle::Failed { message: error };
                    }
                }

                Task::none()
            }
            Message::LuaSkillsBootstrapFinished(result) => {
                mark_startup_trace("message:LuaSkillsBootstrapFinished");
                match result {
                    Ok(report) => {
                        self.status_message = report.summary_text();
                        self.activities.push(ActivityEntry {
                            level: ActivityLevel::Success,
                            title: String::from("LuaSkills 运行时就绪"),
                            detail: format!(
                                "运行时根目录 {} 已准备完成，已挂载 {} 个技能根并发现 {} 个入口。",
                                report.handle.runtime_root.display(),
                                report.handle.skill_roots.len(),
                                report.handle.entry_count
                            ),
                        });
                        self.luaskills_runtime = Some(report.handle);
                        self.refresh_runtime_foundation();
                        self.luaskills_operation_in_flight = true;
                        self.luaskills_notice =
                            String::from("LuaSkills 运行时已就绪，正在载入技能目录。");
                        self.startup_lifecycle = StartupLifecycle::Bootstrapping {
                            message: String::from("正在初始化 Token 预算运行时。"),
                        };
                        self.status_message =
                            String::from("LuaSkills 运行时已就绪，正在初始化 Token 预算运行时。");

                        return Task::perform(
                            TokenBudgetRuntime::initialize_on_background_thread(),
                            Message::TokenBudgetRuntimeInitialized,
                        );
                    }
                    Err(error) => {
                        self.status_message = format!("LuaSkills 运行时初始化失败：{error}");
                        self.activities.push(ActivityEntry {
                            level: ActivityLevel::Warning,
                            title: String::from("LuaSkills 运行时初始化失败"),
                            detail: error.clone(),
                        });
                        self.luaskills_runtime = None;
                        self.luaskills_catalog = None;
                        self.luaskills_operation_in_flight = false;
                        self.startup_lifecycle = StartupLifecycle::Failed { message: error };
                    }
                }

                Task::none()
            }
            Message::TokenBudgetRuntimeInitialized(token_budget_runtime) => {
                mark_startup_trace("message:TokenBudgetRuntimeInitialized");
                let status = token_budget_runtime.status_row();
                let level = if token_budget_runtime.is_available() {
                    ActivityLevel::Success
                } else {
                    ActivityLevel::Warning
                };
                let title = if token_budget_runtime.is_available() {
                    "Token 预算运行时就绪"
                } else {
                    "Token 预算运行时不可用"
                };
                self.refresh_runtime_foundation_with_token_budget_runtime(
                    token_budget_runtime,
                    false,
                );
                self.status_message = status.clone();
                self.push_activity(level, title, status);

                self.startup_lifecycle = StartupLifecycle::Ready;
                self.status_message =
                    String::from("Token 预算运行时已处理，正在后台加载工作台数据。");

                let Some(controller_runtime) = self.controller_runtime.clone() else {
                    self.startup_lifecycle = StartupLifecycle::Failed {
                        message: String::from("控制器运行时缺失，无法加载工作台数据。"),
                    };
                    return Task::none();
                };
                let Some(luaskills_handle) = self.luaskills_runtime.clone() else {
                    self.startup_lifecycle = StartupLifecycle::Failed {
                        message: String::from("LuaSkills 运行时缺失，无法加载技能目录。"),
                    };
                    return Task::none();
                };

                Task::batch([
                    Task::perform(
                        workspace_store::load_or_seed_workspace(controller_runtime),
                        Message::WorkspaceStateLoaded,
                    ),
                    Task::perform(
                        bootstrap_mcp_runtime_access(self.provider_configuration.clone()),
                        Message::McpRuntimeBootstrapped,
                    ),
                    Task::perform(
                        luaskills_runtime::load_luaskills_catalog(luaskills_handle),
                        Message::LuaSkillsCatalogLoaded,
                    ),
                ])
            }
            Message::WorkspaceStateLoaded(result) => {
                mark_startup_trace("message:WorkspaceStateLoaded");
                match result {
                    Ok(snapshot) => {
                        self.apply_workspace_snapshot(snapshot);
                        self.startup_lifecycle = StartupLifecycle::Ready;
                        self.status_message =
                            String::from("工作台列表已加载完成，当前会话正文将按需载入。");
                        self.push_activity(
                            ActivityLevel::Success,
                            "工作台数据已加载",
                            format!(
                                "已恢复 {} 个项目与 {} 个会话；会话正文按需加载，避免启动阶段拉取超大历史。",
                                self.task_projects.len(),
                                self.sessions.len()
                            ),
                        );
                        let hydration_task = self
                            .selected_session()
                            .map(|session| session.id)
                            .map(|session_id| self.schedule_session_hydration(session_id))
                            .unwrap_or_else(Task::none);
                        let legacy_maintenance_task =
                            self.schedule_next_background_legacy_session_maintenance();
                        return Task::batch([
                            self.schedule_project_temp_cleanup_for_all_projects(),
                            hydration_task,
                            legacy_maintenance_task,
                        ]);
                    }
                    Err(error) => {
                        self.status_message = format!("工作台数据加载失败：{error}");
                        self.push_activity(ActivityLevel::Warning, "工作台数据加载失败", error);
                    }
                }

                Task::none()
            }
            Message::SessionHydrationLoaded(session_id, result) => {
                self.loading_session_ids.remove(&session_id);
                match result {
                    Ok(snapshot) => {
                        self.apply_session_hydration_snapshot(snapshot);
                        if self
                            .selected_session()
                            .is_some_and(|session| session.id == session_id)
                        {
                            let continue_search_navigation =
                                self.maybe_continue_workspace_history_search_navigation(session_id);
                            self.status_message = self.t(I18nKey::StatusSessionHydrated);
                            return Task::batch([
                                self.schedule_session_agent_view_projection_refresh_if_needed(
                                    session_id,
                                ),
                                self.maybe_follow_selected_session_transcript_tail(session_id),
                                self.maybe_follow_selected_session_thinking_overlay_tail(
                                    session_id,
                                ),
                                continue_search_navigation,
                            ]);
                        }

                        return Task::batch([self
                            .schedule_session_agent_view_projection_refresh_if_needed(
                                session_id,
                            )]);
                    }
                    Err(error) => {
                        if session_event_store_missing_error(&error) {
                            let session_title = self
                                .sessions
                                .iter()
                                .chain(self.archived_sessions.iter())
                                .find(|session| session.id == session_id)
                                .map(|session| session.title.clone())
                                .unwrap_or_else(|| format!("会话 {session_id}"));
                            self.show_workbench_confirmation_modal(
                                "会话正文已丢失",
                                format!(
                                    "会话 `{session_title}` 的正文文件已经丢失，当前无法继续加载。\n\n是否按“新会话”状态重建当前会话？重建后会保留原标题，但原有正文与工具记录将不会恢复。"
                                ),
                                "重建会话",
                                "暂不处理",
                                WorkbenchNoticeConfirmAction::RebuildMissingSession {
                                    session_id,
                                },
                            );
                        }
                        self.status_message = format!("当前会话正文加载失败：{error}");
                        self.push_activity(ActivityLevel::Warning, "当前会话正文加载失败", error);
                    }
                }

                Task::none()
            }
            Message::SessionAgentViewProjectionLoaded(session_id, result) => {
                if !self
                    .loading_session_agent_view_projection_ids
                    .remove(&session_id)
                {
                    return Task::none();
                }

                match result {
                    Ok(snapshot) => {
                        self.apply_session_agent_view_projection_snapshot(snapshot);
                        if self
                            .selected_session()
                            .is_some_and(|session| session.id == session_id)
                        {
                            return Task::batch([
                                self.maybe_follow_selected_session_transcript_tail(session_id),
                                self.maybe_follow_selected_session_thinking_overlay_tail(
                                    session_id,
                                ),
                                self.maybe_continue_workspace_history_search_navigation(session_id),
                            ]);
                        }
                    }
                    Err(error) => {
                        self.status_message = format!("当前会话视图构建失败：{error}");
                        self.push_activity(ActivityLevel::Warning, "当前会话视图构建失败", error);
                    }
                }

                Task::none()
            }
            Message::OlderSessionHistoryLoaded(session_id, result) => {
                self.loading_older_session_history_ids.remove(&session_id);
                match result {
                    Ok(page) => {
                        self.apply_older_session_history_page(page);
                        self.status_message = self.t(I18nKey::StatusOlderHistoryLoaded);
                        return Task::batch([
                            self.maybe_continue_workspace_history_search_navigation(session_id)
                        ]);
                    }
                    Err(error) => {
                        self.status_message = format!("加载更早会话历史失败：{error}");
                        self.push_activity(ActivityLevel::Warning, "加载更早会话历史失败", error);
                    }
                }

                Task::none()
            }
            Message::BackgroundLegacySessionMaintenanceFinished(session_id, result) => {
                if self.background_legacy_maintenance_in_flight == Some(session_id) {
                    self.background_legacy_maintenance_in_flight = None;
                }
                match result {
                    Ok(report) => {
                        self.background_legacy_maintenance_completed_count = self
                            .background_legacy_maintenance_completed_count
                            .saturating_add(1);
                        self.record_session_legacy_chunk_migration_activity(
                            "后台旧版超大消息已迁移",
                            &report,
                        );
                    }
                    Err(error) => {
                        self.background_legacy_maintenance_failed_count = self
                            .background_legacy_maintenance_failed_count
                            .saturating_add(1);
                        self.push_activity(
                            ActivityLevel::Warning,
                            "后台旧会话维护失败",
                            format!("session_id={session_id}，{error}"),
                        );
                    }
                }

                let next_task = self.schedule_next_background_legacy_session_maintenance();
                if self.background_legacy_maintenance_in_flight.is_none()
                    && self.background_legacy_maintenance_queue.is_empty()
                    && (self.background_legacy_maintenance_completed_count > 0
                        || self.background_legacy_maintenance_failed_count > 0)
                {
                    let level = if self.background_legacy_maintenance_failed_count == 0 {
                        ActivityLevel::Success
                    } else {
                        ActivityLevel::Warning
                    };
                    self.push_activity(
                        level,
                        "后台旧会话维护完成",
                        format!(
                            "已处理 {} 个会话，失败 {} 个。后台维护只执行轻量迁移与旧状态清理，未水合完整正文。",
                            self.background_legacy_maintenance_completed_count,
                            self.background_legacy_maintenance_failed_count
                        ),
                    );
                }

                next_task
            }
            Message::ProjectTempMaintenanceFinished(result) => {
                if let Err(error) = result {
                    self.push_activity(
                        ActivityLevel::Warning,
                        "项目临时目录清理失败",
                        error.clone(),
                    );
                    self.status_message = format!("项目临时目录清理失败：{error}");
                }

                Task::none()
            }
            Message::WorkspaceStatePersisted(result) => {
                self.workspace_snapshot_persist_in_flight = false;
                let next_persist_task = if self.workspace_snapshot_persist_pending {
                    self.workspace_snapshot_persist_pending = false;
                    self.persist_workspace_snapshot_task()
                } else {
                    Task::none()
                };
                match result {
                    Ok(outcome) if !outcome.is_clean() => {
                        self.status_message =
                            String::from("工作台状态已保存，但历史搜索索引同步被延后。");
                        for warning in outcome.warnings {
                            self.push_activity(
                                ActivityLevel::Warning,
                                "工作台历史搜索延后同步",
                                warning,
                            );
                        }
                    }
                    Ok(_) => {}
                    Err(error) => {
                        self.status_message = format!("工作台状态保存失败：{error}");
                        self.push_activity(ActivityLevel::Warning, "工作台状态保存失败", error);
                    }
                }

                next_persist_task
            }
            luaskills_message @ (Message::LuaSkillsCatalogLoaded(_)
            | Message::PollLuaSkillsOperationProgressTick
            | Message::RefreshLuaSkillsCatalog
            | Message::LuaSkillsInstallSourceChoiceSelected(_)
            | Message::LuaSkillsInstallSkillIdChanged(_)
            | Message::LuaSkillsInstallSourceChanged(_)
            | Message::InstallLuaSkill
            | Message::UpdateLuaSkill(_)
            | Message::EnableLuaSkill(_)
            | Message::DisableLuaSkill(_)
            | Message::UninstallLuaSkill(_)
            | Message::SetLuaSkillToolVisibility { .. }
            | Message::LuaSkillsManagementFinished(_)) => {
                self.update_luaskills_message(luaskills_message)
            }
            Message::McpRuntimeBootstrapped(result) => {
                mark_startup_trace("message:McpRuntimeBootstrapped");
                match result {
                    Ok(runtime_access) => {
                        self.mcp_runtime = Some(runtime_access);
                        self.refresh_runtime_foundation();
                        self.mcp_notice =
                            String::from("MCP 运行时已就绪，后续执行将直接注册实时可用工具。");
                    }
                    Err(error) => {
                        self.mcp_runtime = None;
                        self.refresh_runtime_foundation();
                        self.mcp_notice = format!("MCP 运行时初始化失败：{error}");
                        self.push_activity(ActivityLevel::Warning, "MCP 运行时初始化失败", error);
                    }
                }

                self.load_selected_mcp_server_oauth_status_task()
            }
            Message::MainWindowOpened(window_id) => {
                mark_startup_trace("message:MainWindowOpened");
                self.main_window_id = Some(window_id);
                window::is_maximized(window_id).map(move |maximized| {
                    Message::WindowMaximizedStateChanged(window_id, maximized)
                })
            }
            Message::DiffWindowOpened(window_id) => {
                self.diff_window_id = Some(window_id);
                self.refresh_visible_diff_buffers();
                self.status_message = String::from("Diff 预览窗口已打开，可独立调整面板结构。");
                window::is_maximized(window_id).map(move |maximized| {
                    Message::WindowMaximizedStateChanged(window_id, maximized)
                })
            }
            Message::OpenDiffWindow => {
                // Reuse the existing diff preview window when it is already alive to avoid duplicate reviewers.
                // 当 Diff 预览窗口已存在时复用该窗口，避免产生重复审查窗口。
                if let Some(window_id) = self.diff_window_id {
                    window::gain_focus(window_id)
                } else {
                    let (window_id, open_diff_window) = window::open(diff_window_settings());
                    self.diff_window_id = Some(window_id);
                    self.status_message = String::from("正在打开独立 Diff 预览窗口。");

                    open_diff_window.map(Message::DiffWindowOpened)
                }
            }
            Message::OpenAskDebugModal => {
                self.active_question_modal = Some(Self::build_ask_debug_question_modal_state());
                self.status_message =
                    String::from("已打开 ask 调试弹窗，可直接验证不同问题类型与样式。");
                self.sync_active_question_modal_focus()
            }
            Message::WindowTitleBarPressed(window_id) => {
                self.pending_title_bar_drag = self
                    .title_bar_pointer_position
                    .filter(|pointer| pointer.window_id == window_id)
                    .map(|pointer| PendingWindowTitleBarDrag {
                        window_id,
                        origin: pointer.position,
                    });
                Task::none()
            }
            Message::WindowTitleBarPointerMoved(window_id, position) => {
                self.title_bar_pointer_position = Some(WindowTitleBarPointer {
                    window_id,
                    position,
                });

                if let Some(pending_drag) = self.pending_title_bar_drag {
                    if pending_drag.window_id == window_id {
                        let delta_x = position.x - pending_drag.origin.x;
                        let delta_y = position.y - pending_drag.origin.y;
                        let moved_far_enough = delta_x.abs() >= WINDOW_TITLE_BAR_DRAG_THRESHOLD
                            || delta_y.abs() >= WINDOW_TITLE_BAR_DRAG_THRESHOLD;

                        if moved_far_enough {
                            self.pending_title_bar_drag = None;
                            return window::drag(window_id);
                        }
                    }
                }

                Task::none()
            }
            Message::WindowTitleBarReleased(window_id) => {
                if self
                    .pending_title_bar_drag
                    .is_some_and(|pending| pending.window_id == window_id)
                {
                    self.pending_title_bar_drag = None;
                }
                Task::none()
            }
            Message::WindowTitleBarExited(window_id) => {
                if self
                    .title_bar_pointer_position
                    .is_some_and(|pointer| pointer.window_id == window_id)
                {
                    self.title_bar_pointer_position = None;
                }

                if self
                    .pending_title_bar_drag
                    .is_some_and(|pending| pending.window_id == window_id)
                {
                    self.pending_title_bar_drag = None;
                }

                Task::none()
            }
            Message::MinimizeWindow(window_id) => window::minimize(window_id, true),
            Message::StartWindowResize(window_id, direction) => {
                window::drag_resize(window_id, direction)
            }
            Message::ToggleMaximizeWindow(window_id) => {
                if self
                    .pending_title_bar_drag
                    .is_some_and(|pending| pending.window_id == window_id)
                {
                    self.pending_title_bar_drag = None;
                }

                window::toggle_maximize(window_id)
            }
            Message::CloseWindow(window_id) => window::close(window_id),
            Message::ToggleMainTaskPanel => {
                self.main_task_panel_expanded = !self.main_task_panel_expanded;
                self.pending_main_task_panel_resize = None;
                self.main_task_panel_resize_handle_hovered = false;
                Task::none()
            }
            Message::StartMainTaskPanelResize => {
                if let Some(position) = self.main_window_cursor_position {
                    self.pending_main_task_panel_resize = Some(PendingMainTaskPanelResize {
                        origin_x: position.x,
                        initial_width: self.main_task_panel_width,
                    });
                    self.main_task_panel_resize_handle_hovered = true;
                }
                Task::none()
            }
            Message::MainTaskPanelResizeHandleHovered(is_hovered) => {
                self.main_task_panel_resize_handle_hovered = is_hovered;
                Task::none()
            }
            Message::MainDisplayPanelSelected(panel) => {
                self.active_main_panel = panel;
                self.composer_drag_hover_labels.clear();
                self.status_message = self.tf(
                    I18nKey::StatusMainPanelSwitched,
                    &[I18nArg::named(
                        "panel_label",
                        panel.localized_label(self.current_locale()),
                    )],
                );

                if panel == MainDisplayPanel::LuaSkills
                    && self.luaskills_catalog.is_none()
                    && !self.luaskills_operation_in_flight
                {
                    if let Some(handle) = self.luaskills_runtime.clone() {
                        self.luaskills_operation_in_flight = true;
                        self.luaskills_notice = String::from("正在载入 LuaSkills 技能目录。");
                        return Task::perform(
                            luaskills_runtime::load_luaskills_catalog(handle),
                            Message::LuaSkillsCatalogLoaded,
                        );
                    }
                }

                Task::none()
            }
            Message::WindowClosed(window_id) => {
                if self.main_window_id == Some(window_id) {
                    self.main_window_id = None;
                }

                if self.diff_window_id == Some(window_id) {
                    self.diff_window_id = None;
                    self.status_message = String::from("Diff 预览窗口已关闭。");
                }

                if self.main_window_id.is_some() {
                    self.refresh_visible_diff_buffers();
                }

                if self.main_window_id.is_none() && self.diff_window_id.is_none() {
                    iced::exit()
                } else {
                    Task::none()
                }
            }
            Message::WindowResized(window_id, size) => {
                if self.main_window_id == Some(window_id) {
                    self.main_window_size = size;
                    self.dismiss_workbench_context_menu();
                }

                if self.diff_window_id == Some(window_id) {
                    self.diff_window_size = size;
                }

                if self.main_window_id == Some(window_id) || self.diff_window_id == Some(window_id)
                {
                    self.refresh_visible_diff_buffers();
                }

                window::is_maximized(window_id).map(move |maximized| {
                    Message::WindowMaximizedStateChanged(window_id, maximized)
                })
            }
            Message::WindowPointerMoved(window_id, position) => {
                if self.main_window_id == Some(window_id) {
                    self.main_window_cursor_position = Some(position);
                    if self
                        .workbench_image_preview_modal
                        .as_ref()
                        .is_some_and(|modal| modal.pan_pointer.is_some())
                    {
                        return self.apply_workbench_image_preview_pan(position);
                    }

                    if let Some(pending_resize) = self.pending_main_task_panel_resize {
                        let new_width = (pending_resize.initial_width
                            + (position.x - pending_resize.origin_x))
                            .clamp(MAIN_TASK_PANEL_MIN_WIDTH, MAIN_TASK_PANEL_MAX_WIDTH);
                        self.main_task_panel_width = new_width;
                    }

                    if let Some(pending_project_interaction) =
                        self.pending_task_project_interaction.as_ref()
                    {
                        if self.dragging_task_project.is_none()
                            && ((position.x - pending_project_interaction.origin.x).abs()
                                >= WINDOW_TITLE_BAR_DRAG_THRESHOLD
                                || (position.y - pending_project_interaction.origin.y).abs()
                                    >= WINDOW_TITLE_BAR_DRAG_THRESHOLD)
                        {
                            self.dragging_task_project =
                                Some(pending_project_interaction.project_id.clone());
                            self.hovered_task_project = None;
                            self.hovered_task_session = None;
                        }
                    }
                    if let Some(pending_queue_drag) = self.pending_pending_prompt_drag.as_ref() {
                        if self.dragging_pending_prompt_entry_id.is_none()
                            && ((position.x - pending_queue_drag.origin.x).abs()
                                >= WINDOW_TITLE_BAR_DRAG_THRESHOLD
                                || (position.y - pending_queue_drag.origin.y).abs()
                                    >= WINDOW_TITLE_BAR_DRAG_THRESHOLD)
                        {
                            self.dragging_pending_prompt_entry_id =
                                Some(pending_queue_drag.entry_id.clone());
                            self.active_pending_prompt_drop_target =
                                Some(PendingPromptDropTarget::Before(
                                    pending_queue_drag.entry_id.clone(),
                                ));
                        }
                    }
                    if let Some(pending_route_drag) = self.pending_ai_capability_route_drag.as_ref()
                    {
                        if self.dragging_ai_capability_route_target.is_none()
                            && ((position.x - pending_route_drag.origin.x).abs()
                                >= WINDOW_TITLE_BAR_DRAG_THRESHOLD
                                || (position.y - pending_route_drag.origin.y).abs()
                                    >= WINDOW_TITLE_BAR_DRAG_THRESHOLD)
                        {
                            self.dragging_ai_capability_route_target =
                                Some(pending_route_drag.target_selector.clone());
                            self.active_ai_capability_route_drop_target =
                                Some(AiCapabilityRouteDropTarget::Before(
                                    pending_route_drag.target_selector.clone(),
                                ));
                        }
                    }
                }
                Task::none()
            }
            Message::WorkbenchKeyboardModifiersChanged(modifiers) => {
                self.workbench_keyboard_modifiers = modifiers;
                Task::none()
            }
            Message::WindowLeftButtonReleased(window_id) => {
                self.clear_transcript_drag_auto_scrolls();
                self.composer_scrollbar_drag_grab_offset = None;
                let mut did_commit_drop = false;
                let mut did_commit_pending_prompt_drop = false;
                if self.main_window_id == Some(window_id) {
                    if let Some(modal) = self.workbench_image_preview_modal.as_mut() {
                        modal.pan_pointer = None;
                    }
                    self.pending_main_task_panel_resize = None;

                    let toggled_project_id = self
                        .pending_task_project_interaction
                        .as_ref()
                        .filter(|_| self.dragging_task_project.is_none())
                        .map(|interaction| interaction.project_id.clone());

                    let committed_drop = self
                        .dragging_task_project
                        .clone()
                        .zip(self.active_task_project_drop_target.clone());

                    self.pending_task_project_interaction = None;

                    if let Some((dragged_project_id, drop_target)) = committed_drop {
                        self.apply_task_project_drop_target(&dragged_project_id, &drop_target);
                        did_commit_drop = true;
                    }

                    self.dragging_task_project = None;
                    self.active_task_project_drop_target = None;

                    let committed_pending_prompt_drop = self
                        .selected_session()
                        .map(|session| session.id)
                        .zip(self.dragging_pending_prompt_entry_id.clone())
                        .zip(self.active_pending_prompt_drop_target.clone())
                        .map(|((session_id, dragged_entry_id), drop_target)| {
                            (session_id, dragged_entry_id, drop_target)
                        });
                    self.pending_pending_prompt_drag = None;
                    if let Some((session_id, dragged_entry_id, drop_target)) =
                        committed_pending_prompt_drop
                    {
                        did_commit_pending_prompt_drop = self.apply_pending_prompt_drop_target(
                            session_id,
                            &dragged_entry_id,
                            &drop_target,
                        );
                    }
                    self.dragging_pending_prompt_entry_id = None;
                    self.active_pending_prompt_drop_target = None;

                    let committed_ai_capability_route_drop = self
                        .pending_ai_capability_route_drag
                        .as_ref()
                        .map(|drag| drag.capability_kind)
                        .zip(self.dragging_ai_capability_route_target.clone())
                        .zip(self.active_ai_capability_route_drop_target.clone())
                        .map(
                            |((capability_kind, dragged_target_selector), drop_target)| {
                                (capability_kind, dragged_target_selector, drop_target)
                            },
                        );
                    self.pending_ai_capability_route_drag = None;
                    if let Some((capability_kind, dragged_target_selector, drop_target)) =
                        committed_ai_capability_route_drop
                    {
                        let _ = self.apply_ai_capability_route_drop_target(
                            capability_kind,
                            &dragged_target_selector,
                            &drop_target,
                        );
                    }
                    self.dragging_ai_capability_route_target = None;
                    self.active_ai_capability_route_drop_target = None;

                    if let Some(project_id) = toggled_project_id {
                        self.toggle_task_project_expanded(&project_id);
                    }
                }
                if did_commit_drop || did_commit_pending_prompt_drop {
                    self.persist_workspace_snapshot_task()
                } else {
                    Task::none()
                }
            }
            Message::WindowMaximizedStateChanged(window_id, maximized) => {
                if self.main_window_id == Some(window_id) {
                    self.main_window_maximized = maximized;
                }

                if self.diff_window_id == Some(window_id) {
                    self.diff_window_maximized = maximized;
                }

                Task::none()
            }
            Message::WorkspacePathChanged(value) => {
                self.workspace_path = value;
                Task::none()
            }
            Message::ProjectRulesWatchChanged { workspace_path } => {
                crate::agent_runtime::provider_runtime_context::invalidate_workspace_project_rules_prompt_cache(
                    &workspace_path,
                );
                crate::agent_runtime::prompt_composer_runtime::invalidate_runtime_prompt_composer_cache_for_reason(
                    vulcan_prompt_composer::PromptInvalidationReason::ProjectRuleFileChanged,
                );
                Task::none()
            }
            Message::PromptResourcesWatchChanged {
                workspace_path,
                kind,
            } => {
                match kind {
                    PromptResourceWatchKind::ProjectPrompt => {
                        crate::agent_runtime::provider_runtime_context::invalidate_workspace_project_user_prompt_cache(
                            &workspace_path,
                            &self.provider_configuration.prompt.project_prompt_path,
                        );
                        crate::agent_runtime::prompt_composer_runtime::invalidate_runtime_prompt_composer_cache_for_reason(
                            vulcan_prompt_composer::PromptInvalidationReason::ProjectPromptFileChanged,
                        );
                        self.refresh_prompt_composer_inspector_buffer();
                        self.status_message =
                            String::from("项目提示词已变化，提示词用户上下文缓存已刷新。");
                    }
                    PromptResourceWatchKind::ProjectPromptConfig => {
                        crate::agent_runtime::provider_runtime_context::invalidate_workspace_project_user_prompt_cache(
                            &workspace_path,
                            &self.provider_configuration.prompt.project_prompt_path,
                        );
                        if self.project_prompt_settings_draft.workspace_path == workspace_path {
                            self.reload_project_prompt_settings_draft();
                        }
                        crate::agent_runtime::prompt_composer_runtime::invalidate_runtime_prompt_composer_cache_for_reason(
                            vulcan_prompt_composer::PromptInvalidationReason::ProjectPromptFileChanged,
                        );
                        self.refresh_prompt_composer_inspector_buffer();
                        self.status_message =
                            String::from("项目提示词配置已变化，提示词用户上下文缓存已刷新。");
                    }
                    PromptResourceWatchKind::UserPromptResources => {
                        crate::agent_runtime::prompt_composer_runtime::invalidate_runtime_prompt_composer_cache_for_reason(
                            vulcan_prompt_composer::PromptInvalidationReason::PersonalityChanged,
                        );
                        crate::agent_runtime::prompt_composer_runtime::invalidate_runtime_prompt_composer_cache_for_reason(
                            vulcan_prompt_composer::PromptInvalidationReason::ToolSchemaChanged,
                        );
                        match crate::agent_runtime::prompt_composer_runtime::reload_default_runtime_prompt_composer_engine() {
                            Ok(()) => {
                                self.refresh_prompt_composer_inspector_buffer();
                                self.status_message = String::from("用户级提示词资源已变化，运行时提示词组合器已刷新。");
                            }
                            Err(error) => {
                                self.status_message = format!("用户级提示词资源变化后刷新组合器失败：{error}");
                            }
                        }
                    }
                }
                Task::none()
            }
            Message::ComposerChanged(value) => {
                self.composer = value;
                self.refresh_composer_slash_command_state();
                self.save_selected_session_composer_draft();
                self.persist_workspace_snapshot_task()
            }
            Message::ComposerEditorAction(action) => {
                let task = self.apply_composer_editor_action(action);
                self.refresh_composer_slash_command_state();
                task
            }
            Message::ComposerSlashCommandMoveSelectionUp => {
                self.move_composer_slash_command_highlight(false);
                Task::none()
            }
            Message::ComposerSlashCommandMoveSelectionDown => {
                self.move_composer_slash_command_highlight(true);
                Task::none()
            }
            Message::ActivateComposerSlashCommand(command) => {
                self.activate_composer_slash_command(command)
            }
            Message::DismissComposerSlashCommandPanel => {
                self.dismiss_composer_slash_command_panel();
                Task::none()
            }
            Message::ComposerUndo => {
                let task = self.restore_latest_composer_undo_snapshot();
                self.refresh_composer_slash_command_state();
                task
            }
            Message::AddComposerAttachments => self.add_composer_attachments(),
            Message::PasteComposerClipboardContents => self.paste_composer_clipboard(false),
            Message::PasteComposerClipboardAttachments => self.paste_composer_clipboard(true),
            Message::RemoveComposerAttachment(local_path) => {
                self.remove_composer_attachment(&local_path)
            }
            Message::OpenComposerImageAttachmentPreview {
                file_name,
                local_path,
            } => self.open_composer_image_attachment_preview(file_name, local_path),
            Message::OpenGeneratedAttachmentWithDefaultApp(local_path) => {
                self.open_generated_attachment_with_system_default_application(&local_path)
            }
            Message::OpenAboutLicensesDirectory => {
                match self
                    .about_license_catalog
                    .preferred_directory_to_open()
                    .map(Path::to_path_buf)
                {
                    Some(directory) => {
                        match open_path_in_system_file_manager(&directory.display().to_string()) {
                            Ok(()) => {
                                self.status_message = String::from("已打开许可证资源目录。");
                            }
                            Err(error) => {
                                self.show_workbench_notice_modal(
                                    "无法打开许可证资源目录",
                                    error.clone(),
                                );
                                self.status_message = format!("打开许可证资源目录失败：{error}");
                            }
                        }
                    }
                    None => {
                        self.status_message =
                            String::from("当前运行环境还没有可打开的许可证资源目录。");
                    }
                }
                Task::none()
            }
            Message::OpenAboutLicenseDocumentPath(document_id) => {
                match self
                    .about_license_catalog
                    .document(&document_id)
                    .and_then(|document| document.external_path.as_ref())
                    .map(PathBuf::from)
                {
                    Some(path) => {
                        match open_path_with_system_default_application(&path.display().to_string())
                        {
                            Ok(()) => {
                                self.status_message = String::from("已打开许可证原始文件。");
                            }
                            Err(error) => {
                                self.show_workbench_notice_modal(
                                    "无法打开许可证原始文件",
                                    error.clone(),
                                );
                                self.status_message = format!("打开许可证原始文件失败：{error}");
                            }
                        }
                    }
                    None => {
                        self.status_message =
                            String::from("当前许可证文档没有可直接打开的本地文件。");
                    }
                }
                Task::none()
            }
            Message::RevealGeneratedAttachmentInFolder(local_path) => {
                match reveal_path_in_system_file_manager(&local_path) {
                    Ok(()) => {
                        self.status_message = String::from("已在文件管理器中定位生成媒体。");
                    }
                    Err(error) => {
                        self.show_workbench_notice_modal("无法定位生成媒体", error.clone());
                        self.status_message = format!("定位生成媒体失败：{error}");
                    }
                }
                Task::none()
            }
            Message::CopyGeneratedAttachmentPath(local_path) => {
                self.status_message = String::from("已复制生成媒体路径。");
                clipboard::write(local_path).discard()
            }
            Message::ToggleGeneratedAudioPlayback {
                file_name,
                local_path,
            } => self.toggle_generated_audio_playback(file_name, local_path),
            Message::CopyGeneratedImageToClipboard {
                file_name,
                local_path,
            } => self.copy_generated_image_to_clipboard(file_name, local_path),
            Message::CloseWorkbenchImagePreviewModal => {
                self.workbench_image_preview_modal = None;
                Task::none()
            }
            Message::AdjustWorkbenchImagePreviewZoom(delta) => {
                self.adjust_workbench_image_preview_zoom(delta)
            }
            Message::WorkbenchImagePreviewWheelScrolled(delta) => {
                self.handle_workbench_image_preview_wheel(delta)
            }
            Message::WorkbenchImagePreviewLeftPressed => self.begin_workbench_image_preview_pan(),
            Message::WorkbenchImagePreviewLeftReleased => self.end_workbench_image_preview_pan(),
            Message::WorkbenchImagePreviewPointerMoved(position) => {
                if let Some(modal) = self.workbench_image_preview_modal.as_mut() {
                    modal.preview_pointer = Some(position);
                }
                Task::none()
            }
            Message::WorkbenchImagePreviewScrolled(viewport) => {
                if let Some(modal) = self.workbench_image_preview_modal.as_mut() {
                    modal.scroll_offset = viewport.absolute_offset();
                }
                Task::none()
            }
            Message::ResetWorkbenchImagePreviewZoom => self.reset_workbench_image_preview_zoom(),
            Message::ConfirmWorkbenchNoticeModal => self.confirm_workbench_notice_modal(),
            Message::CloseWorkbenchNoticeModal => {
                self.workbench_notice_modal = None;
                if self.active_question_modal.is_some() {
                    self.sync_active_question_modal_focus()
                } else {
                    Task::none()
                }
            }
            Message::QuestionRuntimePollTick => {
                if self.sync_active_question_modal_from_runtime() {
                    self.sync_active_question_modal_focus()
                } else {
                    Task::none()
                }
            }
            Message::QuestionModalOptionPressed {
                question_index,
                option_index,
            } => {
                self.toggle_active_question_modal_option(question_index, option_index);
                self.sync_active_question_modal_focus()
            }
            Message::QuestionModalCustomTextChanged {
                question_index,
                value,
            } => {
                self.update_active_question_modal_custom_answer(question_index, value);
                Task::none()
            }
            Message::QuestionModalSelectQuestionTab(question_index) => {
                self.select_active_question_modal_question(question_index);
                self.sync_active_question_modal_focus()
            }
            Message::QuestionModalSelectConfirmTab => {
                self.open_active_question_modal_confirm_stage();
                self.sync_active_question_modal_focus()
            }
            Message::QuestionModalMoveToPreviousQuestion => {
                self.move_active_question_modal_question(false);
                self.sync_active_question_modal_focus()
            }
            Message::QuestionModalMoveToNextQuestion => {
                self.move_active_question_modal_question(true);
                self.sync_active_question_modal_focus()
            }
            Message::QuestionModalToggleHighlightedOption => {
                if self.active_question_modal_stage()
                    == Some(WorkbenchQuestionModalStage::Confirming)
                {
                    self.confirm_active_question_modal_page()
                } else {
                    self.toggle_active_question_modal_highlighted_option();
                    self.sync_active_question_modal_focus()
                }
            }
            Message::SubmitQuestionModal => self.submit_active_question_modal(),
            Message::DismissTopLevelOverlay => {
                if self.composer_slash_command_panel_state().is_some() {
                    self.dismiss_composer_slash_command_panel();
                    return Task::none();
                }
                if self.workbench_context_menu.take().is_some() {
                    self.composer_context_clipboard_text = None;
                    return Task::none();
                }
                if self.workbench_image_preview_modal.take().is_some() {
                    return Task::none();
                }
                if self.workbench_notice_modal.take().is_some() {
                    if self.active_question_modal.is_some() {
                        return self.sync_active_question_modal_focus();
                    }
                    return Task::none();
                }
                if self.active_question_modal.is_some() {
                    return self.request_active_question_modal_rejection();
                }
                if self.about_license_active_document_id.take().is_some() {
                    self.about_license_document_buffer_loading = false;
                    self.about_license_document_buffer_load_error = None;
                    return Task::none();
                }
                if self.ai_capability_vendor_editor.take().is_some() {
                    return Task::none();
                }
                if self.ai_capability_vendor_picker.take().is_some() {
                    return Task::none();
                }
                if self.ai_capability_target_picker.take().is_some() {
                    return Task::none();
                }
                if self.ai_capability_routing_picker.take().is_some() {
                    return Task::none();
                }
                if self.provider_model_override_editor.take().is_some() {
                    return Task::none();
                }
                if self.provider_model_picker.take().is_some() {
                    return Task::none();
                }
                if self.provider_editor.take().is_some() {
                    return Task::none();
                }
                if self.mcp_server_editor_open {
                    self.mcp_server_editor_open = false;
                    return Task::none();
                }
                if self.workbench_ai_capability_panel_open {
                    self.workbench_ai_capability_panel_open = false;
                    return Task::none();
                }
                if self.workbench_reasoning_picker_open {
                    self.workbench_reasoning_picker_open = false;
                    return Task::none();
                }
                if self.workbench_model_picker_open {
                    self.workbench_model_picker_open = false;
                    self.workbench_model_picker_filter.clear();
                    self.workbench_model_picker_highlighted_index = None;
                    self.workbench_model_picker_scroll_offset = 0.0;
                    self.workbench_model_picker_viewport_height =
                        WORKBENCH_MODEL_PICKER_LIST_HEIGHT;
                    return Task::none();
                }
                self.begin_close_thinking_modal();
                self.begin_close_tool_modal();
                Task::none()
            }
            Message::ConfirmTopLevelOverlay => {
                if self.workbench_image_preview_modal.is_some() {
                    return Task::none();
                }
                if self.ai_capability_vendor_editor.is_some() {
                    return self.save_ai_capability_vendor_editor();
                }
                if self.ai_capability_vendor_picker.is_some() {
                    return Task::none();
                }
                if self.ai_capability_target_picker.is_some() {
                    return self.apply_ai_capability_target_picker_selections();
                }
                if self.ai_capability_routing_picker.is_some() {
                    return Task::none();
                }
                if self.provider_model_override_editor.is_some() {
                    return Task::done(Message::SaveProviderModelOverrideEditor);
                }
                if self.provider_model_picker.is_some() {
                    return Task::done(Message::SaveProviderModelPicker);
                }
                if self.provider_editor.is_some() {
                    return Task::done(Message::SaveProviderConfiguration);
                }
                if self.mcp_server_editor_open {
                    return Task::done(Message::SaveMcpServerConfiguration);
                }
                if self.workbench_ai_capability_panel_open {
                    self.workbench_ai_capability_panel_open = false;
                    return Task::none();
                }
                if self.workbench_reasoning_picker_open {
                    self.workbench_reasoning_picker_open = false;
                    return Task::none();
                }
                if self.workbench_notice_modal.is_some() {
                    return self.confirm_workbench_notice_modal();
                }
                if self.active_question_modal.is_some() {
                    return self.confirm_active_question_modal_page();
                }
                if !self.workbench_model_picker_open {
                    return Task::none();
                }

                let Some(choice) = self.highlighted_workbench_model_choice() else {
                    return Task::none();
                };

                self.request_workbench_model_choice(choice)
            }
            Message::WindowFileHovered(window_id, path) => {
                if self.main_window_id != Some(window_id)
                    || !self.active_workbench_supports_any_attachments()
                {
                    return Task::none();
                }
                self.push_composer_drag_hover_label(&path);
                Task::none()
            }
            Message::WindowFilesHoverLeft(window_id) => {
                if self.main_window_id == Some(window_id) {
                    self.composer_drag_hover_labels.clear();
                }
                Task::none()
            }
            Message::WindowFileDropped(window_id, path) => {
                if self.main_window_id != Some(window_id) {
                    return Task::none();
                }
                self.composer_drag_hover_labels.clear();
                if !self.active_workbench_supports_any_attachments() {
                    let notice = String::from(
                        "当前模型未声明可用附件输入模态，请切换到支持音频、图片、视频或 PDF 的模型后再试。",
                    );
                    self.show_workbench_notice_modal("当前模型不支持附件", notice.clone());
                    self.status_message = notice;
                    return operation::focus(COMPOSER_EDITOR_ID);
                }
                if self.composer_attachments.len() >= MAX_COMPOSER_ATTACHMENTS {
                    self.show_composer_attachment_limit_notice("拖拽文件");
                    return operation::focus(COMPOSER_EDITOR_ID);
                }
                let attachment_workspace_path = self.selected_attachment_workspace_path();
                match stage_composer_attachment_paths(
                    &attachment_workspace_path,
                    std::slice::from_ref(&path),
                ) {
                    Ok(attachments) => {
                        self.append_imported_composer_attachments(attachments, "拖拽")
                    }
                    Err(error) => {
                        self.show_workbench_notice_modal(
                            Self::composer_attachment_error_title(&error),
                            error.clone(),
                        );
                        self.status_message = format!("处理拖拽附件失败：{error}");
                        operation::focus(COMPOSER_EDITOR_ID)
                    }
                }
            }
            Message::OpenComposerContextMenu => self.open_composer_context_menu(),
            Message::OpenTaskProjectContextMenu(project_id) => {
                self.open_task_project_context_menu(project_id)
            }
            Message::OpenTaskSessionContextMenu(session_id) => {
                self.open_task_session_context_menu(session_id)
            }
            Message::OpenArchivedTaskProjectContextMenu(project_id) => {
                self.open_archived_task_project_context_menu(project_id)
            }
            Message::OpenArchivedTaskSessionContextMenu(session_id) => {
                self.open_archived_task_session_context_menu(session_id)
            }
            Message::OpenTranscriptContextMenu(request) => {
                self.open_transcript_context_menu(request)
            }
            Message::CloseWorkbenchContextMenu => {
                self.dismiss_workbench_context_menu();
                Task::none()
            }
            Message::InvokeWorkbenchContextMenuItem(index) => {
                self.invoke_workbench_context_menu_item(index)
            }
            Message::ComposerContextClipboardLoaded(result) => {
                self.composer_context_clipboard_text = result
                    .ok()
                    .map(|text| text.as_ref().clone())
                    .filter(|text| composer_clipboard_text_is_pastable(Some(text)));
                self.refresh_open_composer_context_menu();
                Task::none()
            }
            Message::ComposerScrollbarPointerMoved(pointer_y) => {
                self.composer_scrollbar_pointer_y = Some(pointer_y);
                self.apply_composer_scrollbar_pointer_position(pointer_y);
                Task::none()
            }
            Message::StartComposerScrollbarDrag => {
                let Some(layout) = self.composer_scrollbar_layout() else {
                    return Task::none();
                };
                let pointer_y = self.composer_scrollbar_pointer_y.unwrap_or(8.0);
                let local_track_y = (pointer_y - 8.0).clamp(0.0, layout.track_height);
                let thumb_bottom = layout.thumb_top + layout.thumb_height;
                let grab_offset =
                    if local_track_y >= layout.thumb_top && local_track_y <= thumb_bottom {
                        local_track_y - layout.thumb_top
                    } else {
                        (layout.thumb_height / 2.0).clamp(0.0, layout.thumb_height)
                    };

                self.composer_scrollbar_drag_grab_offset = Some(grab_offset);
                self.apply_composer_scrollbar_pointer_position(pointer_y);
                Task::none()
            }
            Message::ComposerContextMenuSelectAll => {
                self.workbench_context_menu = None;
                self.composer_context_clipboard_text = None;
                let task = self.apply_composer_editor_action(text_editor::Action::SelectAll);
                Task::batch([task, operation::focus(COMPOSER_EDITOR_ID)])
            }
            Message::ComposerContextMenuCopy => {
                self.workbench_context_menu = None;
                self.composer_context_clipboard_text = None;
                if let Some(selection) = self
                    .composer_buffer
                    .selection()
                    .filter(|text| composer_selection_has_content(Some(text)))
                {
                    Task::batch([
                        clipboard::write(selection).discard(),
                        operation::focus(COMPOSER_EDITOR_ID),
                    ])
                } else {
                    operation::focus(COMPOSER_EDITOR_ID)
                }
            }
            Message::ComposerContextMenuCut => {
                self.workbench_context_menu = None;
                self.composer_context_clipboard_text = None;
                if let Some(selection) = self
                    .composer_buffer
                    .selection()
                    .filter(|text| composer_selection_has_content(Some(text)))
                {
                    let action_task = self.apply_composer_editor_action(text_editor::Action::Edit(
                        text_editor::Edit::Backspace,
                    ));
                    Task::batch([
                        clipboard::write(selection).discard(),
                        action_task,
                        operation::focus(COMPOSER_EDITOR_ID),
                    ])
                } else {
                    operation::focus(COMPOSER_EDITOR_ID)
                }
            }
            Message::ComposerContextMenuPaste => {
                self.workbench_context_menu = None;
                let clipboard_text = self.composer_context_clipboard_text.take();
                if let Some(text) = clipboard_text {
                    let action_task = self.apply_composer_editor_action(text_editor::Action::Edit(
                        text_editor::Edit::Paste(std::sync::Arc::new(text)),
                    ));
                    Task::batch([action_task, operation::focus(COMPOSER_EDITOR_ID)])
                } else {
                    operation::focus(COMPOSER_EDITOR_ID)
                }
            }
            Message::TranscriptContextMenuCopySelection(selected_text) => {
                self.workbench_context_menu = None;
                self.status_message = String::from("已复制转录选中文本。");
                clipboard::write(selected_text).discard()
            }
            Message::TranscriptContextMenuSelectAll(surface_id) => {
                self.workbench_context_menu = None;
                self.selectable_transcript_command_revision = self
                    .selectable_transcript_command_revision
                    .saturating_add(1);
                self.selectable_transcript_command = Some(SelectableTranscriptCommand::SelectAll {
                    surface_id,
                    revision: self.selectable_transcript_command_revision,
                });
                Task::none()
            }
            Message::EditorActionPerformed(action) => {
                self.editor_content.perform(action);
                Task::none()
            }
            Message::ExamplePreviewMarkdownAction(action) => {
                if !action.is_edit() {
                    self.example_preview_markdown_buffer.perform(action);
                }
                Task::none()
            }
            Message::SettingsSectionSelected(section) => {
                let section_changed = self.selected_settings_section != section;
                self.selected_settings_section = section;
                if section.belongs_to_ai_capabilities() {
                    self.settings_ai_capability_tree_expanded = true;
                }
                if section.belongs_to_model_settings() {
                    self.settings_model_tree_expanded = true;
                }
                if section == SettingsSection::RuntimeDiagnostics {
                    self.refresh_prompt_composer_inspector_buffer();
                }
                if section == SettingsSection::PromptComposer {
                    self.reload_project_prompt_settings_draft();
                }
                self.status_message = self.tf(
                    I18nKey::StatusSettingsSectionSwitched,
                    &[I18nArg::named(
                        "section_title",
                        section.localized_title(self.current_locale()),
                    )],
                );
                if section == SettingsSection::AboutLicenses
                    && self.should_auto_start_about_license_catalog_reload(section_changed)
                {
                    self.start_about_license_catalog_reload(
                        AboutLicenseCatalogLoadTrigger::OpenSection,
                    )
                } else {
                    Task::none()
                }
            }
            Message::ToggleModelSettingsTree => {
                self.settings_model_tree_expanded = !self.settings_model_tree_expanded;
                Task::none()
            }
            Message::ToggleAiCapabilitySettingsTree => {
                self.settings_ai_capability_tree_expanded =
                    !self.settings_ai_capability_tree_expanded;
                Task::none()
            }
            Message::ProviderCatalogSelected(provider) => {
                self.selected_provider = provider;
                if self.provider_editor.is_some() {
                    self.reload_selected_provider_form();
                    self.provider_notice = format!(
                        "已切换到供应商：{}。",
                        provider_config::provider_summary(&self.selected_provider.id)
                    );
                }
                Task::none()
            }
            Message::ProviderFieldChanged(field, value) => {
                self.provider_form.set(field, value);
                Task::none()
            }
            Message::OpenProviderEditorForCreate => {
                self.open_provider_editor_for_create();
                Task::none()
            }
            Message::OpenProviderEditorForEdit(provider_id) => {
                if let Err(error) = self.open_provider_editor_for_edit(&provider_id) {
                    self.provider_notice = error.clone();
                    self.status_message = error;
                }
                Task::none()
            }
            Message::CloseProviderEditor => {
                self.provider_editor = None;
                Task::none()
            }
            Message::SaveProviderConfiguration => {
                let provider_id = self.selected_provider.id.clone();
                let provider_name = provider_config::provider_summary(&provider_id);
                let should_open_model_picker = self
                    .provider_editor
                    .as_ref()
                    .is_some_and(|editor| editor.is_new);
                if let Err(error) = self.save_selected_provider_configuration() {
                    self.provider_notice = error.clone();
                    self.status_message = error;
                    return Task::none();
                }
                self.refresh_runtime_foundation();
                self.provider_notice = format!("已保存 `{provider_name}` 供应商配置。");
                self.status_message = format!("供应商 `{provider_name}` 配置已写入本地文件。");
                self.provider_editor = None;
                if should_open_model_picker {
                    if let Err(error) = self.open_provider_model_picker(&provider_id) {
                        self.provider_notice = error.clone();
                        self.status_message = error;
                    } else if let Some(state) = self.provider_model_picker.as_mut() {
                        let default_enabled_ids =
                            provider_config::supported_provider_by_id(&provider_id)
                                .map(|definition| {
                                    provider_config::default_enabled_model_ids(&definition.models)
                                        .into_iter()
                                        .collect::<BTreeSet<_>>()
                                })
                                .unwrap_or_default();
                        state
                            .draft_enabled_model_ids_by_provider
                            .insert(provider_id.clone(), default_enabled_ids);
                    }
                }
                Task::none()
            }
            Message::OpenProviderModelPicker(provider_id) => {
                if let Err(error) = self.open_provider_model_picker(&provider_id) {
                    self.provider_notice = error.clone();
                    self.status_message = error;
                }
                Task::none()
            }
            Message::CloseProviderModelPicker => {
                self.provider_model_picker = None;
                Task::none()
            }
            Message::ProviderModelPickerProviderSelected(provider_id) => {
                if let Some(state) = self.provider_model_picker.as_mut() {
                    state.selected_provider_id = provider_id;
                    state.filter.clear();
                }
                Task::none()
            }
            Message::ProviderModelPickerFilterChanged(value) => {
                if let Some(state) = self.provider_model_picker.as_mut() {
                    state.filter = value;
                }
                Task::none()
            }
            Message::ProviderModelPickerModelToggled {
                provider_id,
                model_id,
                enabled,
            } => {
                if let Some(state) = self.provider_model_picker.as_mut() {
                    if state.selected_provider_id == provider_id {
                        let draft_ids = state
                            .draft_enabled_model_ids_by_provider
                            .entry(provider_id)
                            .or_insert_with(BTreeSet::new);
                        if enabled {
                            draft_ids.insert(model_id);
                        } else {
                            draft_ids.remove(&model_id);
                        }
                    }
                }
                Task::none()
            }
            Message::ProviderModelPickerSelectAllVisible => {
                let visible_ids = self
                    .filtered_provider_model_picker_catalog()
                    .into_iter()
                    .map(|model| model.id)
                    .collect::<Vec<_>>();
                if let Some(state) = self.provider_model_picker.as_mut() {
                    let draft_ids = state
                        .draft_enabled_model_ids_by_provider
                        .entry(state.selected_provider_id.clone())
                        .or_insert_with(BTreeSet::new);
                    for model_id in visible_ids {
                        draft_ids.insert(model_id);
                    }
                }
                Task::none()
            }
            Message::ProviderModelPickerClearAllVisible => {
                let visible_ids = self
                    .filtered_provider_model_picker_catalog()
                    .into_iter()
                    .map(|model| model.id)
                    .collect::<Vec<_>>();
                if let Some(state) = self.provider_model_picker.as_mut() {
                    let draft_ids = state
                        .draft_enabled_model_ids_by_provider
                        .entry(state.selected_provider_id.clone())
                        .or_insert_with(BTreeSet::new);
                    for model_id in visible_ids {
                        draft_ids.remove(&model_id);
                    }
                }
                Task::none()
            }
            Message::SaveProviderModelPicker => {
                let provider_id = self
                    .provider_model_picker
                    .as_ref()
                    .map(|state| state.selected_provider_id.clone())
                    .unwrap_or_default();
                if let Err(error) = self.save_provider_model_picker() {
                    self.provider_notice = error.clone();
                    self.status_message = error;
                    return Task::none();
                }
                self.provider_model_picker = None;
                self.refresh_runtime_foundation();
                self.provider_notice = format!(
                    "已保存 `{}` 的启用模型配置。",
                    provider_config::provider_summary(&provider_id)
                );
                self.status_message = self.provider_notice.clone();
                Task::none()
            }
            Message::RemoveConfiguredProvider(provider_id) => {
                let provider_name = provider_config::provider_summary(&provider_id);
                self.show_workbench_confirmation_modal(
                    format!("确认移除模型供应商 `{provider_name}`"),
                    "移除后，该供应商的接入配置与已启用模型都会一并删除；如果它正作为当前模型来源，也会同步清空当前选择。",
                    "确认移除",
                    "取消",
                    WorkbenchNoticeConfirmAction::RemoveConfiguredProvider { provider_id },
                );
                Task::none()
            }
            Message::OpenProviderModelOverrideEditor {
                provider_id,
                model_id,
            } => {
                if let Err(error) =
                    self.open_provider_model_override_editor(&provider_id, &model_id)
                {
                    self.provider_notice = error.clone();
                    self.status_message = error;
                }
                Task::none()
            }
            Message::CloseProviderModelOverrideEditor => {
                self.provider_model_override_editor = None;
                Task::none()
            }
            Message::ProviderModelOverrideReasoningChanged(value) => {
                if let Some(editor) = self.provider_model_override_editor.as_mut() {
                    editor.reasoning = value;
                }
                Task::none()
            }
            Message::ProviderModelOverrideAttachmentChanged(value) => {
                if let Some(editor) = self.provider_model_override_editor.as_mut() {
                    editor.attachment = value;
                }
                Task::none()
            }
            Message::ProviderModelOverrideInputModalityToggled { modality, enabled } => {
                if let Some(editor) = self.provider_model_override_editor.as_mut() {
                    match modality.as_str() {
                        "text" => editor.supports_text = enabled,
                        "image" => editor.supports_image = enabled,
                        "audio" => editor.supports_audio = enabled,
                        "video" => editor.supports_video = enabled,
                        "pdf" => editor.supports_pdf = enabled,
                        _ => {}
                    }
                }
                Task::none()
            }
            Message::ProviderModelOverrideContextLimitChanged(value) => {
                if let Some(editor) = self.provider_model_override_editor.as_mut() {
                    editor.context_limit_text = value;
                }
                Task::none()
            }
            Message::ProviderModelOverrideInputLimitChanged(value) => {
                if let Some(editor) = self.provider_model_override_editor.as_mut() {
                    editor.input_limit_text = value;
                }
                Task::none()
            }
            Message::ProviderModelOverrideOutputLimitChanged(value) => {
                if let Some(editor) = self.provider_model_override_editor.as_mut() {
                    editor.output_limit_text = value;
                }
                Task::none()
            }
            Message::ProviderModelOverrideReasoningProfileAction(action) => {
                if let Some(editor) = self.provider_model_override_editor.as_mut() {
                    editor.reasoning_profile_buffer.perform(action);
                }
                Task::none()
            }
            Message::SaveProviderModelOverrideEditor => {
                let provider_id = self
                    .provider_model_override_editor
                    .as_ref()
                    .map(|editor| editor.provider_id.clone())
                    .unwrap_or_default();
                let model_name = self
                    .provider_model_override_editor
                    .as_ref()
                    .map(|editor| editor.model_name.clone())
                    .unwrap_or_default();
                if let Err(error) = self.save_provider_model_override_editor() {
                    self.provider_notice = error.clone();
                    self.status_message = error;
                    return Task::none();
                }
                self.provider_model_override_editor = None;
                self.refresh_runtime_foundation();
                let reasoning_note = self.reconcile_workbench_reasoning_effort_for_active_model();
                let temperature_note = self.reconcile_workbench_temperature_for_active_model();
                let mut notes = Vec::new();
                if let Some(note) = reasoning_note {
                    notes.push(note);
                }
                if let Some(note) = temperature_note {
                    notes.push(note);
                }
                self.provider_notice = format!(
                    "已保存 `{}` 下模型 `{model_name}` 的覆盖配置。",
                    provider_config::provider_summary(&provider_id)
                );
                self.status_message = if notes.is_empty() {
                    self.provider_notice.clone()
                } else {
                    format!("{} {}", self.provider_notice, notes.join(" "))
                };
                Task::none()
            }
            Message::WebSearchRoutingModeSelected(mode) => {
                self.ai_capability_routing_picker = None;
                self.ai_tool_configuration.web_search.mode = mode;
                self.persist_ai_tool_configuration_change(format!(
                    "web-search 路由已切换为「{}」并自动保存。",
                    mode.label()
                ));
                Task::none()
            }
            Message::UnderstandImageRoutingModeSelected(mode) => {
                self.ai_capability_routing_picker = None;
                self.ai_tool_configuration.understand_image.mode = mode;
                self.persist_ai_tool_configuration_change(format!(
                    "图像理解路由已切换为「{}」并自动保存。",
                    mode.label()
                ));
                Task::none()
            }
            Message::OpenAiCapabilityRoutingPicker(capability_kind) => {
                self.ai_capability_routing_picker =
                    Some(AiCapabilityRoutingPickerState { capability_kind });
                Task::none()
            }
            Message::CloseAiCapabilityRoutingPicker => {
                self.ai_capability_routing_picker = None;
                Task::none()
            }
            Message::AiCapabilityTargetEnabledToggled {
                capability_kind,
                vendor_id,
                target_alias,
                enabled,
            } => {
                match self.set_ai_capability_target_enabled(
                    capability_kind,
                    &vendor_id,
                    &target_alias,
                    enabled,
                ) {
                    Ok(true) => {
                        let state_label = if enabled { "启用" } else { "停用" };
                        self.persist_ai_tool_configuration_change(format!(
                            "已{state_label}“{}”中的 `{}/{}` 并自动保存。",
                            capability_kind.label(),
                            vendor_id,
                            target_alias
                        ));
                    }
                    Ok(false) => {}
                    Err(error) => {
                        self.ai_tool_notice = error.clone();
                        self.status_message = error;
                    }
                }
                Task::none()
            }
            Message::RemoveAiCapabilityVendor(vendor_id) => {
                let vendor_name = self
                    .ai_tool_configuration
                    .vendor_by_id(&vendor_id)
                    .map(|vendor| vendor.name.clone())
                    .unwrap_or_else(|| {
                        provider_config::ai_capability_vendor_display_name(&vendor_id)
                    });
                self.show_workbench_confirmation_modal(
                    format!("确认移除共享能力供应商 `{vendor_name}`"),
                    "移除后，这个供应商下所有已配置接口都会一起删除，并立即写回当前能力扩展配置。",
                    "确认移除",
                    "取消",
                    WorkbenchNoticeConfirmAction::RemoveAiCapabilityVendor { vendor_id },
                );
                Task::none()
            }
            Message::OpenAiCapabilityVendorPicker => {
                self.ai_capability_vendor_picker = Some(AiCapabilityVendorPickerState::default());
                Task::none()
            }
            Message::CloseAiCapabilityVendorPicker => {
                self.ai_capability_vendor_picker = None;
                Task::none()
            }
            Message::AiCapabilityVendorPickerFilterChanged(value) => {
                if let Some(state) = self.ai_capability_vendor_picker.as_mut() {
                    state.filter = value;
                }
                Task::none()
            }
            Message::AiCapabilityVendorPickerVendorSelected(vendor_id) => {
                self.ai_capability_vendor_picker = None;
                self.open_ai_capability_vendor_editor(&vendor_id, true);
                Task::none()
            }
            Message::OpenAiCapabilityVendorEditor(vendor_id) => {
                self.open_ai_capability_vendor_editor(&vendor_id, false);
                Task::none()
            }
            Message::CloseAiCapabilityVendorEditor => {
                self.ai_capability_vendor_editor = None;
                Task::none()
            }
            Message::AiCapabilityVendorEditorApiKeyChanged(value) => {
                if let Some(editor) = self.ai_capability_vendor_editor.as_mut() {
                    editor.api_key = value;
                }
                Task::none()
            }
            Message::AiCapabilityVendorEditorConnectionModeChanged(value) => {
                if let Some(editor) = self.ai_capability_vendor_editor.as_mut() {
                    editor.connection_mode = value;
                }
                Task::none()
            }
            Message::AiCapabilityVendorEditorBaseUrlChanged(value) => {
                if let Some(editor) = self.ai_capability_vendor_editor.as_mut() {
                    editor.base_url = value;
                }
                Task::none()
            }
            Message::AiCapabilityVendorEditorRegionChanged(value) => {
                if let Some(editor) = self.ai_capability_vendor_editor.as_mut() {
                    editor.region = value;
                }
                Task::none()
            }
            Message::SaveAiCapabilityVendorEditor => self.save_ai_capability_vendor_editor(),
            Message::OpenAiCapabilityTargetPicker(capability_kind) => {
                self.open_ai_capability_target_picker(capability_kind)
            }
            Message::CloseAiCapabilityTargetPicker => {
                self.ai_capability_target_picker = None;
                Task::none()
            }
            Message::AiCapabilityTargetPickerVendorSelected(vendor_id) => {
                if let Some(picker) = self.ai_capability_target_picker.as_mut() {
                    picker.selected_vendor_id = Some(vendor_id);
                }
                Task::none()
            }
            Message::AiCapabilityTargetPickerTargetToggled {
                vendor_id,
                target_alias,
                enabled,
            } => {
                self.stage_ai_capability_target_picker_toggle(&vendor_id, &target_alias, enabled);
                Task::none()
            }
            Message::AiCapabilityTargetPickerTargetModelChanged {
                vendor_id,
                target_alias,
                model_id,
            } => {
                self.stage_ai_capability_target_picker_model(&vendor_id, &target_alias, model_id);
                Task::none()
            }
            Message::SaveAiCapabilityTargetPicker => {
                self.apply_ai_capability_target_picker_selections()
            }
            Message::StartAiCapabilityRouteDrag {
                capability_kind,
                target_selector,
            } => {
                if let Some(position) = self.main_window_cursor_position {
                    self.pending_ai_capability_route_drag = Some(PendingAiCapabilityRouteDrag {
                        capability_kind,
                        target_selector,
                        origin: position,
                    });
                }
                Task::none()
            }
            Message::AiCapabilityRouteDropPreviewMoved {
                capability_kind,
                target_selector,
                position,
            } => {
                let Some(dragged_target_selector) =
                    self.dragging_ai_capability_route_target.as_ref()
                else {
                    return Task::none();
                };
                let Some(pending_drag) = self.pending_ai_capability_route_drag.as_ref() else {
                    return Task::none();
                };
                if pending_drag.capability_kind != capability_kind {
                    return Task::none();
                }
                if dragged_target_selector == &target_selector {
                    self.active_ai_capability_route_drop_target =
                        Some(AiCapabilityRouteDropTarget::Before(target_selector));
                    return Task::none();
                }
                let route_entries = self
                    .ai_tool_configuration
                    .resolved_configured_targets_for_kind(capability_kind);
                self.active_ai_capability_route_drop_target = Some(
                    if position.y <= AI_CAPABILITY_ROUTE_TARGET_ROW_HEIGHT / 2.0 {
                        AiCapabilityRouteDropTarget::Before(target_selector)
                    } else {
                        let target_index = route_entries
                            .iter()
                            .position(|entry| entry.target_selector == target_selector)
                            .unwrap_or(route_entries.len());
                        let next_target_selector = route_entries
                            .get(target_index.saturating_add(1))
                            .map(|entry| entry.target_selector.clone());
                        if let Some(next_target_selector) = next_target_selector {
                            if next_target_selector == *dragged_target_selector {
                                AiCapabilityRouteDropTarget::Before(target_selector)
                            } else {
                                AiCapabilityRouteDropTarget::Before(next_target_selector)
                            }
                        } else {
                            AiCapabilityRouteDropTarget::End
                        }
                    },
                );
                Task::none()
            }
            Message::AiCapabilityRouteTailHovered {
                capability_kind,
                is_hovered,
            } => {
                if !is_hovered {
                    if self.dragging_ai_capability_route_target.is_none() {
                        self.active_ai_capability_route_drop_target = None;
                    }
                    return Task::none();
                }
                if self.dragging_ai_capability_route_target.is_some()
                    && self
                        .pending_ai_capability_route_drag
                        .as_ref()
                        .is_some_and(|drag| drag.capability_kind == capability_kind)
                {
                    self.active_ai_capability_route_drop_target =
                        Some(AiCapabilityRouteDropTarget::End);
                }
                Task::none()
            }
            Message::RemoveAiCapabilityRouteTarget {
                capability_kind,
                target_selector,
            } => {
                if self.remove_ai_capability_route_target(capability_kind, &target_selector) {
                    self.persist_ai_tool_configuration_change(format!(
                        "已从“{}”中移除 `{}` 并自动保存。",
                        capability_kind.label(),
                        target_selector
                    ));
                }
                Task::none()
            }
            Message::OpenWorkbenchAiCapabilityPanel => {
                self.workbench_ai_capability_panel_open = true;
                Task::none()
            }
            Message::CloseWorkbenchAiCapabilityPanel => {
                self.workbench_ai_capability_panel_open = false;
                Task::none()
            }
            Message::WorkbenchAiCapabilitySectionSelected(section) => {
                self.workbench_ai_capability_section = section;
                Task::none()
            }
            Message::WorkbenchAiCapabilitySectionEnabledToggled { section, enabled } => {
                match section.capability_kind() {
                    Some(capability_kind) => {
                        let usable_target_count = self
                            .ai_tool_configuration
                            .usable_targets_for_kind(capability_kind)
                            .len();
                        if usable_target_count == 0 {
                            self.set_workbench_ai_capability_enabled(section, false);
                            self.status_message = format!(
                                "“{}”当前没有已启用的可用目标，请先在右侧内容区至少勾选一个模型后再开启。",
                                section.label()
                            );
                            return Task::none();
                        }
                        self.set_workbench_ai_capability_enabled(section, enabled);
                        let configured_target_count = self
                            .ai_tool_configuration
                            .resolved_configured_targets_for_kind(capability_kind)
                            .len();
                        if configured_target_count > 0 {
                            let state_label = if enabled { "启用" } else { "关闭" };
                            self.persist_ai_tool_configuration_change(format!(
                                "已{state_label}“{}”并自动保存。已保留既有目标勾选与顺序偏好。",
                                capability_kind.label()
                            ));
                            return Task::none();
                        } else {
                            self.status_message = format!(
                                "“{}”当前还没有可快速切换的已配置接口，请先在左侧能力页中完成配置。",
                                section.label()
                            );
                        }
                    }
                    None => {
                        self.status_message = format!(
                            "“{}”入口已预留，当前还没有可快速切换的真实能力目标。",
                            section.label()
                        );
                    }
                }
                Task::none()
            }
            Message::WorkbenchModelSelected(choice) => self.request_workbench_model_choice(choice),
            Message::OpenWorkbenchOpenRouterEndpointPicker => {
                self.open_workbench_openrouter_endpoint_picker()
            }
            Message::CloseWorkbenchOpenRouterEndpointPicker => {
                self.workbench_openrouter_endpoint_picker =
                    WorkbenchOpenRouterEndpointPickerState::default();
                Task::none()
            }
            Message::WorkbenchOpenRouterEndpointPickerFilterChanged(value) => {
                self.workbench_openrouter_endpoint_picker.filter = value;
                Task::none()
            }
            Message::WorkbenchOpenRouterEndpointSelected(selection) => {
                let result = self.set_active_workbench_openrouter_endpoint_route(selection.clone());
                self.workbench_openrouter_endpoint_picker =
                    WorkbenchOpenRouterEndpointPickerState::default();
                match result {
                    Ok(()) => {
                        self.status_message = selection
                            .as_ref()
                            .map(|route| {
                                format!("已切换 OpenRouter 二级供应商：{}", route.provider_name)
                            })
                            .unwrap_or_else(|| String::from("已切换为 OpenRouter 自动路由。"));
                    }
                    Err(error) => {
                        self.status_message = error.clone();
                        self.show_workbench_notice_modal("保存二级供应商失败", error);
                    }
                }
                Task::none()
            }
            Message::WorkbenchOpenRouterEndpointsLoaded { model_id, result } => {
                if self.workbench_openrouter_endpoint_picker.model_id != model_id {
                    return Task::none();
                }
                self.workbench_openrouter_endpoint_picker.loading = false;
                match result {
                    Ok(choices) => {
                        self.workbench_openrouter_endpoint_picker.error = None;
                        self.workbench_openrouter_endpoint_picker.choices = choices;
                    }
                    Err(error) => {
                        self.workbench_openrouter_endpoint_picker.choices.clear();
                        self.workbench_openrouter_endpoint_picker.error = Some(error.clone());
                        self.status_message = error;
                    }
                }
                Task::none()
            }
            Message::OpenWorkbenchModelPicker => {
                self.dismiss_composer_slash_command_panel();
                self.open_workbench_model_picker()
            }
            Message::CloseWorkbenchModelPicker => {
                self.workbench_openrouter_endpoint_picker =
                    WorkbenchOpenRouterEndpointPickerState::default();
                self.workbench_model_picker_open = false;
                self.workbench_model_picker_filter.clear();
                self.workbench_model_picker_highlighted_index = None;
                self.workbench_model_picker_scroll_offset = 0.0;
                self.workbench_model_picker_viewport_height = WORKBENCH_MODEL_PICKER_LIST_HEIGHT;
                Task::none()
            }
            Message::OpenWorkbenchReasoningPicker => {
                self.dismiss_composer_slash_command_panel();
                self.workbench_model_picker_open = false;
                self.workbench_openrouter_endpoint_picker =
                    WorkbenchOpenRouterEndpointPickerState::default();
                self.workbench_model_picker_filter.clear();
                self.workbench_model_picker_highlighted_index = None;
                self.workbench_reasoning_picker_open = true;
                Task::none()
            }
            Message::CloseWorkbenchReasoningPicker => {
                self.workbench_reasoning_picker_open = false;
                Task::none()
            }
            Message::WorkbenchModelPickerScrolled(viewport) => {
                self.workbench_model_picker_scroll_offset = viewport.absolute_offset().y.max(0.0);
                self.workbench_model_picker_viewport_height = viewport.bounds().height.max(0.0);
                Task::none()
            }
            Message::WorkbenchModelPickerFilterChanged(value) => {
                self.workbench_model_picker_filter = value;
                self.workbench_model_picker_highlighted_index =
                    self.preferred_workbench_model_picker_index();
                self.clamp_workbench_model_picker_highlight();
                self.workbench_model_picker_scroll_offset = 0.0;
                self.workbench_model_picker_highlighted_index
                    .map(|index| self.scroll_workbench_model_picker_to_index(index))
                    .unwrap_or_else(Task::none)
            }
            Message::WorkbenchModelPickerMoveSelectionUp => {
                if self.active_question_modal.is_some() {
                    self.move_active_question_modal_option(false);
                    return Task::none();
                }
                if !self.workbench_model_picker_open {
                    return Task::none();
                }

                let filtered_len = self.filtered_workbench_model_choices().len();
                self.workbench_model_picker_highlighted_index = if filtered_len == 0 {
                    None
                } else {
                    Some(
                        self.workbench_model_picker_highlighted_index
                            .unwrap_or(0)
                            .checked_sub(1)
                            .unwrap_or(filtered_len.saturating_sub(1)),
                    )
                };
                self.workbench_model_picker_highlighted_index
                    .map(|index| self.scroll_workbench_model_picker_to_index(index))
                    .unwrap_or_else(Task::none)
            }
            Message::WorkbenchModelPickerMoveSelectionDown => {
                if self.active_question_modal.is_some() {
                    self.move_active_question_modal_option(true);
                    return Task::none();
                }
                if !self.workbench_model_picker_open {
                    return Task::none();
                }

                let filtered_len = self.filtered_workbench_model_choices().len();
                self.workbench_model_picker_highlighted_index = if filtered_len == 0 {
                    None
                } else {
                    Some(
                        self.workbench_model_picker_highlighted_index
                            .unwrap_or(0)
                            .saturating_add(1)
                            % filtered_len,
                    )
                };
                self.workbench_model_picker_highlighted_index
                    .map(|index| self.scroll_workbench_model_picker_to_index(index))
                    .unwrap_or_else(Task::none)
            }
            Message::ActiveAgentSelected(choice) => {
                let selected_agent_id = if choice.id == SYSTEM_DEFAULT_AGENT_CHOICE_ID {
                    None
                } else if choice.id == PLAN_AGENT_ID || choice.id == EXPLORE_AGENT_ID {
                    self.execution_mode = ExecutionMode::Plan;
                    Some(choice.id.clone())
                } else if self.agent_profile_catalog.agents.contains_key(&choice.id) {
                    Some(choice.id.clone())
                } else {
                    self.status_message = format!(
                        "未找到可切换的代理 `{}`，无法切换当前激活代理。",
                        choice.label
                    );
                    return Task::none();
                };
                self.agent_profile_catalog.state.selected_agent_id = selected_agent_id;

                if let Err(error) =
                    agent_profile::save_agent_profile_state(&self.agent_profile_catalog.state)
                {
                    self.mcp_notice = error.clone();
                    self.status_message = error;
                    return Task::none();
                }

                self.refresh_runtime_foundation();
                self.status_message = if choice.id == PLAN_AGENT_ID || choice.id == EXPLORE_AGENT_ID
                {
                    format!(
                        "当前激活代理已切换为 `{}`，执行模式已固定为 Plan。",
                        choice.label
                    )
                } else {
                    format!("当前激活代理已切换为 `{}`。", choice.label)
                };
                Task::none()
            }
            Message::UserAgentProfileSelected(agent_id) => {
                self.selected_user_agent_profile_id = self
                    .agent_profile_catalog
                    .agents
                    .contains_key(&agent_id)
                    .then_some(agent_id);
                self.reload_selected_user_agent_profile_form();
                self.agent_profile_notice = self
                    .selected_user_agent_profile_id
                    .as_deref()
                    .and_then(|selected_id| self.agent_profile_catalog.agents.get(selected_id))
                    .map(|profile| format!("已切换到用户 Agent `{}`。", profile.name))
                    .unwrap_or_else(|| String::from("未找到目标用户 Agent，已回退到新建草稿。"));
                self.status_message = self.agent_profile_notice.clone();
                Task::none()
            }
            Message::CreateUserAgentProfileDraft => {
                self.selected_user_agent_profile_id = None;
                self.reload_selected_user_agent_profile_form();
                self.agent_profile_notice =
                    String::from("已创建新的用户 Agent 草稿，请填写名称与主提示词。");
                self.status_message = self.agent_profile_notice.clone();
                Task::none()
            }
            Message::UserAgentProfileIdChanged(value) => {
                self.agent_profile_form.id = value;
                Task::none()
            }
            Message::UserAgentProfileNameChanged(value) => {
                self.agent_profile_form.name = value;
                Task::none()
            }
            Message::UserAgentProfileDescriptionChanged(value) => {
                self.agent_profile_form.description = value;
                Task::none()
            }
            Message::UserAgentProfileModelChanged(value) => {
                self.agent_profile_form.model = value;
                Task::none()
            }
            Message::UserAgentProfileTemperatureChanged(value) => {
                self.agent_profile_form.temperature_text = value;
                Task::none()
            }
            Message::UserAgentProfileMaxTurnsChanged(value) => {
                self.agent_profile_form.max_turns_text = value;
                Task::none()
            }
            Message::UserAgentProfilePromptAction(action) => {
                self.agent_profile_form.prompt_buffer.perform(action);
                Task::none()
            }
            Message::SaveUserAgentProfile => {
                match self.save_selected_user_agent_profile() {
                    Ok(saved_name) => {
                        self.agent_profile_notice =
                            format!("用户 Agent `{saved_name}` 已保存并刷新运行时。");
                        self.status_message = self.agent_profile_notice.clone();
                    }
                    Err(error) => {
                        self.agent_profile_notice = error.clone();
                        self.status_message = error;
                    }
                }
                Task::none()
            }
            Message::DeleteUserAgentProfile => {
                match self.remove_selected_user_agent_profile() {
                    Ok(removed_name) => {
                        self.agent_profile_notice =
                            format!("用户 Agent `{removed_name}` 已删除并刷新运行时。");
                        self.status_message = self.agent_profile_notice.clone();
                    }
                    Err(error) => {
                        self.agent_profile_notice = error.clone();
                        self.status_message = error;
                    }
                }
                Task::none()
            }
            mcp_message @ (Message::McpServerSelected(_)
            | Message::CloseMcpServerEditor
            | Message::McpServerNameChanged(_)
            | Message::McpServerTransportSelected(_)
            | Message::McpServerCommandChanged(_)
            | Message::McpServerArgRowChanged(_, _)
            | Message::AddMcpServerArgRow
            | Message::RemoveMcpServerArgRow(_)
            | Message::McpServerUrlChanged(_)
            | Message::McpServerBearerTokenChanged(_)
            | Message::McpServerHeaderKeyChanged(_, _)
            | Message::McpServerHeaderValueChanged(_, _)
            | Message::AddMcpServerHeaderRow
            | Message::RemoveMcpServerHeaderRow(_)
            | Message::McpServerHeaderFromEnvKeyChanged(_, _)
            | Message::McpServerHeaderFromEnvValueChanged(_, _)
            | Message::AddMcpServerHeaderFromEnvRow
            | Message::RemoveMcpServerHeaderFromEnvRow(_)
            | Message::McpServerOAuthClientIdChanged(_)
            | Message::McpServerOAuthClientSecretChanged(_)
            | Message::McpServerOAuthScopesChanged(_)
            | Message::McpServerOAuthRedirectUriChanged(_)
            | Message::McpServerCwdChanged(_)
            | Message::McpServerEnvKeyChanged(_, _)
            | Message::McpServerEnvValueChanged(_, _)
            | Message::AddMcpServerEnvRow
            | Message::RemoveMcpServerEnvRow(_)
            | Message::McpServerEnvPassthroughChanged(_, _)
            | Message::AddMcpServerEnvPassthroughRow
            | Message::RemoveMcpServerEnvPassthroughRow(_)
            | Message::McpServerOAuthCallbackInputChanged(_)
            | Message::McpServerEnabledToggled(_)
            | Message::CreateMcpServer
            | Message::SaveMcpServerConfiguration
            | Message::RequestRemoveMcpServerConfiguration(_)
            | Message::StartSelectedMcpServerOAuth
            | Message::SelectedMcpServerOAuthChallengeReady(_, _)
            | Message::McpServerOAuthBrowserOpened(_)
            | Message::SelectedMcpServerOAuthCallbackCaptured(_, _)
            | Message::CompleteSelectedMcpServerOAuthFromInput
            | Message::SelectedMcpServerOAuthFinished(_, _)
            | Message::RemoveSelectedMcpServerOAuthAuthorization
            | Message::SelectedMcpServerOAuthAuthorizationRemoved(_, _)
            | Message::SelectedMcpServerOAuthStatusLoaded(_, _)) => {
                self.update_mcp_message(mcp_message)
            }
            project_settings_message @ (Message::OpenTaskProjectSettings(_)
            | Message::CloseTaskProjectSettings
            | Message::OpenProjectAttachmentCleanupDatePicker
            | Message::CancelProjectAttachmentCleanupDatePicker
            | Message::ProjectAttachmentCleanupDateSelected(_)
            | Message::ProjectAttachmentCleanupKindToggled(_, _)
            | Message::PreviewProjectAttachmentCleanup
            | Message::ProjectAttachmentCleanupPreviewFinished(_, _)
            | Message::ExecuteProjectAttachmentCleanup
            | Message::ProjectAttachmentCleanupExecutionFinished(
                _,
                _,
            )) => self.update_project_settings_message(project_settings_message),
            Message::ThemeSelected(theme) => {
                self.app_theme = theme.clone();
                self.rebuild_quick_guide_markdown_buffer();
                self.status_message = self.tf(
                    I18nKey::StatusThemeChanged,
                    &[I18nArg::named("theme_name", theme.to_string())],
                );
                Task::none()
            }
            Message::LocaleSelected(locale) => {
                self.language_manager.set_locale(locale);
                clear_selectable_transcript_render_cache_stash();
                self.rebuild_quick_guide_markdown_buffer();
                self.refresh_all_session_agent_view_localizations();
                match self.persist_locale_preference() {
                    Ok(()) => {
                        self.status_message = self.tf(
                            I18nKey::StatusLocaleChanged,
                            &[I18nArg::named("locale_name", locale.native_name())],
                        );
                    }
                    Err(error) => {
                        self.status_message = format!(
                            "{} {}",
                            self.tf(
                                I18nKey::StatusLocaleChanged,
                                &[I18nArg::named("locale_name", locale.native_name())],
                            ),
                            error
                        );
                    }
                }
                Task::none()
            }
            Message::SessionTranscriptTurnPageSizeSelected(turn_limit) => {
                if !SESSION_TRANSCRIPT_TURN_PAGE_SIZE_OPTIONS.contains(&turn_limit)
                    || self.session_transcript_turn_page_size == turn_limit
                {
                    return Task::none();
                }

                self.session_transcript_turn_page_size = turn_limit;
                match self.persist_session_transcript_turn_page_size_preference() {
                    Ok(()) => {
                        self.status_message =
                            format!("启动会话分页已切换为每页 {turn_limit} 个完整轮次。");
                    }
                    Err(error) => {
                        self.status_message = format!(
                            "启动会话分页已切换为每页 {turn_limit} 个完整轮次，但保存配置失败：{error}"
                        );
                    }
                }
                Task::none()
            }
            Message::CommandShellPreferenceSelected(choice) => {
                let current_raw = self
                    .provider_configuration
                    .default_command_shell
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(|value| value.to_ascii_lowercase());
                let next_raw = choice.shell.map(|shell| shell.id().to_string());
                if current_raw == next_raw {
                    return Task::none();
                }

                match self.persist_default_command_shell_preference(choice.shell) {
                    Ok(status_message) => {
                        self.status_message = status_message;
                    }
                    Err(error) => {
                        self.status_message = error;
                    }
                }
                Task::none()
            }
            Message::PromptPersonalitySelected(choice) => {
                if self.provider_configuration.prompt.personality == choice.selection {
                    return Task::none();
                }
                self.provider_configuration.prompt.personality = choice.selection;
                match self.persist_prompt_configuration() {
                    Ok(()) => {
                        self.refresh_runtime_foundation();
                        self.refresh_prompt_composer_inspector_buffer();
                        self.status_message = format!("提示词人格已切换为 {}。", choice.label);
                    }
                    Err(error) => {
                        self.status_message = error;
                    }
                }
                Task::none()
            }
            Message::ProjectPromptInjectionToggled(enabled) => {
                if self.provider_configuration.prompt.project_prompt_enabled == enabled {
                    return Task::none();
                }
                self.provider_configuration.prompt.project_prompt_enabled = enabled;
                match self.persist_prompt_configuration() {
                    Ok(()) => {
                        self.refresh_runtime_foundation();
                        self.refresh_prompt_composer_inspector_buffer();
                        self.status_message = if enabled {
                            String::from("已启用项目级用户提示词注入。")
                        } else {
                            String::from("已禁用项目级用户提示词注入。")
                        };
                    }
                    Err(error) => {
                        self.status_message = error;
                    }
                }
                Task::none()
            }
            Message::ProjectPromptOverrideToggled(enabled) => {
                self.ensure_project_prompt_settings_draft_current();
                self.project_prompt_settings_draft.enabled = enabled;
                self.project_prompt_settings_draft.notice =
                    String::from("项目提示词覆盖已修改，点击保存后写入当前项目配置。");
                Task::none()
            }
            Message::ProjectPromptPathChanged(value) => {
                self.ensure_project_prompt_settings_draft_current();
                self.project_prompt_settings_draft.path = value;
                self.project_prompt_settings_draft.notice =
                    String::from("项目提示词路径已修改，点击保存后写入当前项目配置。");
                Task::none()
            }
            Message::ReloadProjectPromptConfiguration => {
                self.reload_project_prompt_settings_draft();
                self.status_message = self.project_prompt_settings_draft.notice.clone();
                Task::none()
            }
            Message::SaveProjectPromptConfiguration => {
                match self.save_project_prompt_settings_draft() {
                    Ok(()) => {
                        self.status_message = self.project_prompt_settings_draft.notice.clone();
                    }
                    Err(error) => {
                        self.project_prompt_settings_draft.notice = error.clone();
                        self.status_message = error;
                    }
                }
                Task::none()
            }
            Message::QuickGuideMarkdownAction(action) => {
                if !action.is_edit() {
                    self.quick_guide_markdown_buffer.perform(action);
                }
                Task::none()
            }
            Message::TogglePromptComposerInspectorLayer(layer) => {
                if !self
                    .prompt_composer_inspector_expanded_layers
                    .remove(&layer)
                {
                    self.prompt_composer_inspector_expanded_layers.insert(layer);
                    self.ensure_prompt_composer_inspector_layer_buffer(layer);
                }
                Task::none()
            }
            Message::PromptComposerInspectorLayerAction(layer, action) => {
                if !action.is_edit() {
                    if let Some(content) =
                        self.prompt_composer_inspector_layer_buffers.get_mut(&layer)
                    {
                        content.perform(action);
                    }
                }
                Task::none()
            }
            Message::TogglePromptComposerInspectorSegment(segment_key) => {
                if !self
                    .prompt_composer_inspector_expanded_segments
                    .remove(&segment_key)
                {
                    let opened_segment_key = segment_key.clone();
                    self.prompt_composer_inspector_expanded_segments
                        .insert(segment_key);
                    self.ensure_prompt_composer_inspector_segment_buffer(&opened_segment_key);
                }
                Task::none()
            }
            Message::PromptComposerInspectorSegmentAction(segment_key, action) => {
                if !action.is_edit() {
                    if let Some(content) = self
                        .prompt_composer_inspector_segment_buffers
                        .get_mut(&segment_key)
                    {
                        content.perform(action);
                    }
                }
                Task::none()
            }
            Message::CopyPromptComposerInspectorReport => {
                if self.prompt_composer_inspector_markdown.trim().is_empty() {
                    self.refresh_prompt_composer_inspector_buffer();
                }
                if self.prompt_composer_inspector_markdown.trim().is_empty() {
                    self.status_message = String::from("暂无可复制的提示词组合报告。");
                    Task::none()
                } else {
                    self.status_message = String::from("已复制完整提示词组合报告。");
                    clipboard::write(self.prompt_composer_inspector_markdown.clone()).discard()
                }
            }
            Message::CopyPromptComposerInspectorLayer(layer) => {
                self.copy_prompt_composer_inspector_layer(layer)
            }
            Message::CopyPromptComposerInspectorSegment(segment_key) => {
                self.copy_prompt_composer_inspector_segment(segment_key)
            }
            Message::AboutLicenseCatalogLoaded(revision, result) => {
                self.finish_about_license_catalog_reload(revision, result)
            }
            Message::ToggleAboutLicenseDocumentBody(document_id) => {
                if self.about_license_active_document_id.as_deref() == Some(document_id.as_str()) {
                    self.about_license_active_document_id = None;
                    self.about_license_document_buffer_loading = false;
                    self.about_license_document_buffer_load_error = None;
                    Task::none()
                } else {
                    self.start_about_license_document_buffer_load(document_id)
                }
            }
            Message::AboutLicenseDocumentBufferLoaded(revision, document_id, result) => {
                self.finish_about_license_document_buffer_load(revision, document_id, result)
            }
            Message::AboutLicenseDocumentRawAction(document_id, action) => {
                if !action.is_edit() {
                    if let Some(content) = self.about_license_document_buffers.get_mut(&document_id)
                    {
                        content.perform(action);
                    }
                }
                Task::none()
            }
            Message::ExecutionModeSelected(mode) => {
                if self.selected_agent_requires_plan_mode() {
                    self.execution_mode = ExecutionMode::Plan;
                    self.status_message =
                        String::from("当前选中的是分析代理，执行模式已固定为 Plan。");
                } else {
                    self.execution_mode = mode;
                    self.status_message = format!("执行模式已切换为 {mode}。");
                }
                Task::none()
            }
            Message::WorkbenchReasoningEffortSelected(option) => {
                self.workbench_reasoning_picker_open = false;
                self.workbench_reasoning_effort_preference = option.effort;
                self.workbench_reasoning_effort = option.effort;
                match self.persist_workbench_reasoning_effort_preference() {
                    Ok(()) => {
                        self.status_message = format!("思考模式已切换为 {}。", option.label);
                    }
                    Err(error) => {
                        self.provider_notice = error.clone();
                        self.status_message = format!(
                            "思考模式已切换为 {}，但保存到配置文件失败：{}",
                            option.label, error
                        );
                    }
                }
                Task::none()
            }
            Message::CycleExecutionMode => {
                if self.selected_agent_requires_plan_mode() {
                    self.execution_mode = ExecutionMode::Plan;
                    self.status_message =
                        String::from("当前选中的是分析代理，执行模式已固定为 Plan。");
                } else {
                    self.execution_mode = match self.execution_mode {
                        ExecutionMode::Build => ExecutionMode::Plan,
                        ExecutionMode::Plan => ExecutionMode::Build,
                    };
                    self.status_message = format!("执行模式已切换为 {}。", self.execution_mode);
                }
                Task::none()
            }
            Message::TemperatureChanged(value) => {
                self.temperature = value;
                Task::none()
            }
            Message::TemperatureEnabledToggled(enabled) => {
                self.temperature_enabled = enabled;
                self.status_message = if enabled {
                    format!("已开启显式温度参数，当前值为 {:.2}。", self.temperature)
                } else {
                    String::from("已关闭显式温度参数，后续执行将使用模型默认值。")
                };
                Task::none()
            }
            Message::GeneratePlanToggled(enabled) => {
                self.generate_plan = enabled;
                Task::none()
            }
            Message::RunCargoCheckToggled(enabled) => {
                self.run_cargo_check = enabled;
                Task::none()
            }
            Message::PersistMemoryToggled(enabled) => {
                self.persist_memory = enabled;
                Task::none()
            }
            Message::AutoApproveToggled(enabled) => {
                self.auto_approve = enabled;
                Task::none()
            }
            Message::SessionSelected(session_id) => {
                let previous_workspace_path = self.selected_attachment_workspace_path();
                let previous_scope = self.composer_attachment_ref_scope();
                self.save_selected_session_composer_draft();
                self.prune_unmaterialized_sessions(Some(session_id));
                if self
                    .workspace_history_search
                    .pending_navigation
                    .as_ref()
                    .is_some_and(|pending| pending.session_id != session_id)
                {
                    self.workspace_history_search.pending_navigation = None;
                }
                let has_pending_search_navigation = self
                    .workspace_history_search
                    .pending_navigation
                    .as_ref()
                    .is_some_and(|pending| pending.session_id == session_id);
                self.selected_session = session_id;
                self.hovered_task_session = Some(session_id);
                self.clear_pending_prompt_drag_state();
                self.sync_workspace_path_with_selected_session();
                self.reload_project_prompt_settings_draft();
                self.restore_session_composer_draft(&previous_workspace_path, &previous_scope);
                self.clear_transcript_drag_auto_scrolls();
                self.active_subagent_panel = None;
                self.rebuild_subagent_task_cache_for_session(session_id);
                self.sync_context_usage_from_session_budget(session_id);
                self.session_agent_view_follow_tail
                    .insert(session_id, !has_pending_search_navigation);
                self.session_streaming_thinking_follow_tail
                    .insert(session_id, true);
                // Capture the selected session title to keep footer feedback specific.
                // 记录被选中的会话标题，保证底部反馈足够具体。
                let selected_title = self
                    .selected_session()
                    .map(|session| session.title.clone())
                    .unwrap_or_else(|| String::from("未知会话"));

                self.status_message = self.tf(
                    I18nKey::StatusSessionSwitched,
                    &[I18nArg::named("selected_title", selected_title.as_str())],
                );
                let projection_task =
                    self.schedule_session_agent_view_projection_refresh_if_needed(session_id);
                Task::batch([
                    self.persist_workspace_snapshot_task(),
                    self.schedule_session_hydration(session_id),
                    projection_task,
                    if has_pending_search_navigation {
                        Task::none()
                    } else {
                        self.maybe_follow_selected_session_transcript_tail(session_id)
                    },
                    self.maybe_follow_selected_session_thinking_overlay_tail(session_id),
                    self.maybe_continue_workspace_history_search_navigation(session_id),
                ])
            }
            Message::ShowSelectedSessionDocumentRawSource => {
                self.agent_transcript_surface_mode = AgentTranscriptSurfaceMode::RawSource;
                self.clear_transcript_drag_auto_scrolls();
                self.active_thinking_modal = None;
                self.active_tool_modal = None;
                let Some(session_id) = self.selected_session().map(|session| session.id) else {
                    return Task::none();
                };
                Task::batch([
                    self.schedule_session_agent_view_projection_refresh_if_needed(session_id),
                    self.maybe_follow_selected_session_transcript_tail(session_id),
                    self.maybe_follow_selected_session_thinking_overlay_tail(session_id),
                ])
            }
            Message::ShowSelectedSessionDocumentApiRequest => {
                self.agent_transcript_surface_mode = AgentTranscriptSurfaceMode::ApiRequest;
                self.clear_transcript_drag_auto_scrolls();
                self.active_thinking_modal = None;
                self.active_tool_modal = None;
                let Some(session_id) = self.selected_session().map(|session| session.id) else {
                    return Task::none();
                };
                self.ensure_session_last_api_request_preview_buffer(session_id);
                Task::batch([
                    self.schedule_session_agent_view_projection_refresh_if_needed(session_id),
                    self.maybe_follow_selected_session_transcript_tail(session_id),
                    self.maybe_follow_selected_session_thinking_overlay_tail(session_id),
                ])
            }
            Message::ShowSelectedSessionDocumentDetailCards => {
                self.agent_transcript_surface_mode = AgentTranscriptSurfaceMode::DetailCards;
                self.clear_transcript_drag_auto_scrolls();
                self.active_thinking_modal = None;
                self.active_tool_modal = None;
                let Some(session_id) = self.selected_session().map(|session| session.id) else {
                    return Task::none();
                };
                Task::batch([
                    self.schedule_session_agent_view_projection_refresh_if_needed(session_id),
                    self.maybe_follow_selected_session_transcript_tail(session_id),
                    self.maybe_follow_selected_session_thinking_overlay_tail(session_id),
                ])
            }
            Message::OpenSelectedSessionThinkingModal(block_id) => {
                let Some(session_id) = self.active_transcript_surface_session_id() else {
                    return Task::none();
                };
                self.open_thinking_modal(session_id, block_id);
                Task::none()
            }
            Message::OpenSelectedSessionToolModal(block_id) => {
                let Some(session_id) = self.active_transcript_surface_session_id() else {
                    return Task::none();
                };
                let selected_root_session_id = self.selected_session().map(|session| session.id);
                if selected_root_session_id == Some(session_id)
                    && let Some(task_id) =
                        self.resolve_task_session_id_from_rendered_tool_block(session_id, &block_id)
                    && self.should_open_subagent_panel_from_tool_card(session_id, task_id)
                    && self.open_subagent_task_panel(session_id, task_id)
                {
                    return Task::none();
                }
                self.open_tool_modal(session_id, block_id);
                Task::none()
            }
            Message::OpenSelectedSessionSubagentPanel(task_id) => {
                let Some(session_id) = self.selected_session().map(|session| session.id) else {
                    return Task::none();
                };
                if !self.open_subagent_task_panel(session_id, task_id) {
                    self.status_message =
                        String::from("当前子代理详情已不可恢复，请从 task 工具卡片查看返回信息。");
                }
                Task::none()
            }
            Message::SubagentNavigationHoverChanged(is_expanded) => {
                self.subagent_navigation_expanded = is_expanded;
                Task::none()
            }
            Message::StopSelectedSubagentTask => {
                let Some(state) = self.selected_subagent_task_panel_state().cloned() else {
                    self.status_message = String::from("当前没有可停止的子代理面板。");
                    return Task::none();
                };
                if !self.subagent_task_is_actively_running(state.task_id) {
                    self.status_message = String::from("当前子代理已经结束，无需再次停止。");
                    return Task::none();
                }
                let stop_summary = subagent_task_stopped_summary_text();
                match self
                    .runtime_foundation
                    .task_runtime()
                    .stop_task(state.task_id)
                {
                    Ok(true) => {
                        self.mark_subagent_task_stopped(
                            state.parent_session_id,
                            state.task_id,
                            stop_summary,
                        );
                        self.refresh_subagent_navigation_round_visibility(state.parent_session_id);
                        self.status_message = String::from("已停止当前子代理调用。");
                        Task::batch([
                            self.persist_workspace_snapshot_task(),
                            self.maybe_follow_selected_session_transcript_tail(state.task_id),
                        ])
                    }
                    Ok(false) => {
                        self.status_message =
                            String::from("当前子代理已不在执行中，可直接查看已有结果。");
                        Task::none()
                    }
                    Err(error) => {
                        self.status_message = format!("停止子代理失败：{error}");
                        Task::none()
                    }
                }
            }
            Message::StopSelectedCommandSession => {
                let Some((owner_session_id, command_session_id)) =
                    self.active_command_session_modal_stop_target()
                else {
                    self.status_message = String::from("当前没有可停止的命令会话弹层。");
                    return Task::none();
                };
                let Some(modal_state) = self.active_tool_modal.as_mut() else {
                    self.status_message = String::from("当前没有可停止的命令会话弹层。");
                    return Task::none();
                };
                if modal_state.command_session_stop_in_flight {
                    self.status_message = String::from("命令会话停止请求已经发出，请稍候。");
                    return Task::none();
                }
                modal_state.command_session_stop_in_flight = true;
                let command_session_runtime = self.runtime_foundation.command_session_runtime();
                Task::perform(
                    async move {
                        command_session_runtime
                            .stop_session(owner_session_id, command_session_id, None)
                            .await
                    },
                    Message::SelectedCommandSessionStopFinished,
                )
            }
            Message::SelectedCommandSessionStopFinished(result) => {
                let active_modal_session_id = self
                    .active_tool_modal
                    .as_ref()
                    .map(|modal_state| modal_state.session_id);
                let command_tool_call_id = self.active_command_session_modal_tool_call_id();
                if let Some(modal_state) = self.active_tool_modal.as_mut() {
                    modal_state.command_session_stop_in_flight = false;
                    if let Ok(snapshot) = result.as_ref() {
                        modal_state
                            .payload
                            .tool_modal_presentation
                            .apply_command_session_snapshot(snapshot);
                    }
                }
                match result {
                    Ok(snapshot) => {
                        let tool_call_id = command_tool_call_id.unwrap_or_else(|| {
                            format!("gui-command-session-{}", snapshot.command_session_id)
                        });
                        let transcript_session_id =
                            active_modal_session_id.unwrap_or(snapshot.owner_session_id);
                        self.append_command_session_stop_result_message(
                            transcript_session_id,
                            &tool_call_id,
                            &snapshot,
                        );
                        self.status_message =
                            format!("命令会话 `{}` 已停止。", snapshot.command_session_id);
                    }
                    Err(error) => {
                        self.status_message = format!("停止命令会话失败：{error}");
                    }
                }
                Task::none()
            }
            Message::ReturnToSelectedSessionTranscript => {
                let parent_session_id = self.selected_session().map(|session| session.id);
                self.active_subagent_panel = None;
                if let Some(parent_session_id) = parent_session_id {
                    self.refresh_subagent_navigation_round_visibility(parent_session_id);
                }
                Task::none()
            }
            Message::CloseSelectedSessionThinkingModal => {
                self.workbench_model_picker_open = false;
                self.workbench_model_picker_filter.clear();
                self.workbench_model_picker_highlighted_index = None;
                self.begin_close_thinking_modal();
                self.begin_close_tool_modal();
                Task::none()
            }
            Message::CloseSelectedSessionToolModal => {
                self.begin_close_tool_modal();
                Task::none()
            }
            Message::SelectedSessionDocumentStructuredAction(action) => {
                if action.is_edit() {
                    return Task::none();
                }

                let Some(session_id) = self.selected_session().map(|session| session.id) else {
                    return Task::none();
                };

                if let Some(content) = self.session_agent_view_buffers.get_mut(&session_id) {
                    content.perform(action);
                }

                Task::none()
            }
            Message::SelectedSessionDocumentRawSourceAction(action) => {
                if action.is_edit() {
                    return Task::none();
                }

                let Some(session_id) = self.selected_session().map(|session| session.id) else {
                    return Task::none();
                };

                if let Some(content) = self.session_agent_view_raw_buffers.get_mut(&session_id) {
                    content.perform(action);
                }

                Task::none()
            }
            Message::SelectedSessionDocumentApiRequestAction(action) => {
                if action.is_edit() {
                    return Task::none();
                }

                let Some(session_id) = self.selected_session().map(|session| session.id) else {
                    return Task::none();
                };

                self.ensure_session_last_api_request_preview_buffer(session_id);
                if let Some(content) = self.session_last_api_request_buffers.get_mut(&session_id) {
                    content.perform(action);
                }

                Task::none()
            }
            Message::JumpSelectedSessionTranscriptToBottom => {
                let Some(session_id) = self.active_transcript_surface_session_id() else {
                    return Task::none();
                };

                self.session_agent_view_follow_tail.insert(session_id, true);
                operation::snap_to_end(transcript_scrollable_id(session_id))
            }
            Message::LoadOlderSessionHistoryRequested(session_id) => {
                if self.selected_session().map(|session| session.id) != Some(session_id) {
                    return Task::none();
                }
                if self.loading_older_session_history_ids.contains(&session_id) {
                    self.status_message = self.t(I18nKey::StatusOlderHistoryLoadingInProgress);
                    return Task::none();
                }
                if !self.session_history_has_more.contains(&session_id) {
                    self.status_message = self.t(I18nKey::StatusNoMoreOlderHistory);
                    return Task::none();
                }

                self.status_message = self.t(I18nKey::StatusOlderHistoryLoading);
                self.schedule_older_session_history_load(session_id)
            }
            Message::OpenWorkspaceHistorySearchPanel => self.open_workspace_history_search_panel(),
            Message::CloseWorkspaceHistorySearchPanel => {
                self.workspace_history_search.panel_open = false;
                self.workspace_history_search.project_picker_open = false;
                self.workspace_history_search.project_filter.clear();
                Task::none()
            }
            Message::WorkspaceHistorySearchQueryChanged(query) => {
                self.workspace_history_search.query =
                    sanitize_workspace_history_search_query_input(&query);
                self.workspace_history_search.error = None;
                if self.workspace_history_search.query.trim().is_empty() {
                    self.workspace_history_search.results = None;
                    self.workspace_history_search.pending_navigation = None;
                }
                Task::none()
            }
            Message::SubmitWorkspaceHistorySearch => {
                if self.workspace_history_search.loading {
                    self.status_message = self.t(I18nKey::StatusHistorySearchInProgress);
                    return Task::none();
                }
                let query = self.workspace_history_search.query.trim();
                if query.is_empty() {
                    self.workspace_history_search.results = None;
                    self.workspace_history_search.error = None;
                    self.status_message = self.t(I18nKey::StatusHistorySearchEmptyQuery);
                    return self.open_workspace_history_search_panel();
                }

                self.workspace_history_search.panel_open = true;
                self.workspace_history_search.project_picker_open = false;
                self.workspace_history_search.project_filter.clear();
                self.workspace_history_search.error = None;
                self.status_message = self.t(I18nKey::StatusHistorySearchStarted);
                self.schedule_workspace_history_search(false)
            }
            Message::LoadMoreWorkspaceHistorySearchResults => {
                if self.workspace_history_search.loading {
                    self.status_message = self.t(I18nKey::StatusHistorySearchInProgress);
                    return Task::none();
                }
                let Some(current_page) = self.workspace_history_search.results.as_ref() else {
                    self.status_message = self.t(I18nKey::StatusHistorySearchNoMoreResults);
                    return Task::none();
                };
                if !current_page.has_more {
                    self.status_message = self.t(I18nKey::StatusHistorySearchAllLoaded);
                    return Task::none();
                }
                if self.workspace_history_search.query.trim() != current_page.query {
                    self.status_message = self.t(I18nKey::StatusHistorySearchQueryChanged);
                    return Task::none();
                }

                self.workspace_history_search.error = None;
                self.status_message = self.t(I18nKey::StatusHistorySearchLoadMoreStarted);
                self.schedule_workspace_history_search(true)
            }
            Message::WorkspaceHistorySearchLoaded { append, result } => {
                self.workspace_history_search.loading = false;
                self.workspace_history_search.loading_more = false;
                match result {
                    Ok(page) => {
                        let WorkspaceHistorySearchPage {
                            query,
                            hits,
                            limit_reached,
                            total_hits,
                            next_offset,
                            has_more,
                        } = page;
                        let hit_count = hits.len();
                        self.workspace_history_search.error = None;
                        if append {
                            if let Some(existing_page) =
                                self.workspace_history_search.results.as_mut()
                            {
                                if existing_page.query == query {
                                    existing_page.hits.extend(hits);
                                    existing_page.limit_reached = limit_reached;
                                    existing_page.total_hits = total_hits;
                                    existing_page.next_offset = next_offset;
                                    existing_page.has_more = has_more;
                                } else {
                                    self.workspace_history_search.results =
                                        Some(WorkspaceHistorySearchPage {
                                            query,
                                            hits,
                                            limit_reached,
                                            total_hits,
                                            next_offset,
                                            has_more,
                                        });
                                }
                            } else {
                                self.workspace_history_search.results =
                                    Some(WorkspaceHistorySearchPage {
                                        query,
                                        hits,
                                        limit_reached,
                                        total_hits,
                                        next_offset,
                                        has_more,
                                    });
                            }
                            let loaded_count = self
                                .workspace_history_search
                                .results
                                .as_ref()
                                .map(|merged_page| merged_page.hits.len())
                                .unwrap_or(0);
                            let total_hits = self
                                .workspace_history_search
                                .results
                                .as_ref()
                                .map(|merged_page| merged_page.total_hits)
                                .unwrap_or(loaded_count);
                            self.status_message = self.tf(
                                I18nKey::StatusHistorySearchLoadMoreCompleted,
                                &[
                                    I18nArg::named("hit_count", hit_count.to_string()),
                                    I18nArg::named("loaded_count", loaded_count.to_string()),
                                    I18nArg::named("total_hits", total_hits.to_string()),
                                ],
                            );
                        } else {
                            self.workspace_history_search.results =
                                Some(WorkspaceHistorySearchPage {
                                    query,
                                    hits,
                                    limit_reached,
                                    total_hits,
                                    next_offset,
                                    has_more,
                                });
                            self.status_message = self.tf(
                                I18nKey::StatusHistorySearchCompleted,
                                &[I18nArg::named("total_hits", total_hits.to_string())],
                            );
                        }
                    }
                    Err(error) => {
                        self.workspace_history_search.error = Some(error.clone());
                        self.status_message = self.tf(
                            I18nKey::StatusHistorySearchFailed,
                            &[I18nArg::named("error", error.as_str())],
                        );
                        self.push_activity(ActivityLevel::Warning, "工作台历史搜索失败", error);
                    }
                }

                Task::none()
            }
            Message::ToggleWorkspaceHistorySearchProjectPicker => {
                self.workspace_history_search.panel_open = true;
                let next_open = !self.workspace_history_search.project_picker_open;
                self.workspace_history_search.project_picker_open = next_open;
                if next_open {
                    self.workspace_history_search.project_filter.clear();
                    operation::focus(WORKSPACE_HISTORY_SEARCH_PROJECT_FILTER_INPUT_ID)
                } else {
                    self.workspace_history_search.project_filter.clear();
                    operation::focus(WORKSPACE_HISTORY_SEARCH_INPUT_ID)
                }
            }
            Message::WorkspaceHistorySearchProjectFilterChanged(value) => {
                self.workspace_history_search.project_filter = value;
                Task::none()
            }
            Message::WorkspaceHistorySearchScopeSelected(scope) => {
                self.workspace_history_search.scope = scope;
                self.workspace_history_search.project_picker_open = false;
                self.workspace_history_search.project_filter.clear();
                self.workspace_history_search.error = None;
                operation::focus(WORKSPACE_HISTORY_SEARCH_INPUT_ID)
            }
            Message::ConfirmWorkspaceHistorySearchScopeSelection => {
                let Some(option) = self
                    .filtered_workspace_history_search_scope_options()
                    .into_iter()
                    .next()
                else {
                    self.status_message = String::from("没有匹配的项目范围可供选择。");
                    return operation::focus(WORKSPACE_HISTORY_SEARCH_PROJECT_FILTER_INPUT_ID);
                };

                self.workspace_history_search.scope = option.scope;
                self.workspace_history_search.project_picker_open = false;
                self.workspace_history_search.project_filter.clear();
                self.workspace_history_search.error = None;
                operation::focus(WORKSPACE_HISTORY_SEARCH_INPUT_ID)
            }
            Message::OpenWorkspaceHistorySearchHit {
                session_id,
                message_id,
                ordinal,
                turn_index,
            } => {
                self.workspace_history_search.active_highlight = None;
                self.workspace_history_search.pending_navigation =
                    Some(PendingWorkspaceHistorySearchNavigation {
                        session_id,
                        message_id,
                        ordinal,
                        turn_index,
                    });
                self.workspace_history_search.project_picker_open = false;
                self.workspace_history_search.project_filter.clear();
                self.workspace_history_search.panel_open = false;
                self.status_message = String::from("正在打开搜索命中并定位对应历史位置。");

                if self.selected_session().map(|session| session.id) == Some(session_id) {
                    self.maybe_continue_workspace_history_search_navigation(session_id)
                } else {
                    Task::done(Message::SessionSelected(session_id))
                }
            }
            Message::SelectedSessionTranscriptScrolled {
                session_id,
                viewport,
            } => {
                self.session_agent_view_viewports
                    .insert(session_id, viewport);
                self.update_session_agent_view_follow_tail(session_id, viewport);
                Task::none()
            }
            Message::SelectedSessionTranscriptDragAutoScrollChanged {
                session_id,
                request,
            } => {
                self.update_transcript_drag_auto_scroll_state(session_id, request);
                Task::none()
            }
            Message::SelectedSessionTranscriptDragAutoScrollTick => {
                self.perform_transcript_drag_auto_scroll_tick()
            }
            Message::SelectedSessionTranscriptFollowTailTick => {
                self.perform_pending_transcript_tail_follow_tick()
            }
            Message::SelectedSessionThinkingOverlayScrolled {
                session_id,
                viewport,
            } => {
                self.session_streaming_thinking_viewports
                    .insert(session_id, viewport);
                self.update_session_streaming_thinking_follow_tail(session_id, viewport);
                Task::none()
            }
            Message::SelectedSessionThinkingOverlayDragAutoScrollChanged {
                session_id,
                request,
            } => {
                self.update_thinking_overlay_drag_auto_scroll_state(session_id, request);
                Task::none()
            }
            Message::SelectedSessionThinkingOverlayDragAutoScrollTick => {
                self.perform_thinking_overlay_drag_auto_scroll_tick()
            }
            Message::SelectedSessionThinkingOverlayFollowTailTick => {
                self.perform_pending_thinking_overlay_tail_follow_tick()
            }
            Message::ThinkingModalAnimationTick => {
                self.advance_thinking_modal_animation();
                self.advance_tool_modal_animation();
                Task::none()
            }
            Message::ActivityLogRawAction(index, action) => {
                if !action.is_edit() {
                    if let Some(content) = self.activity_raw_buffers.get_mut(index) {
                        content.perform(action);
                    }
                }
                Task::none()
            }
            Message::CreateSession => {
                let project_id = self
                    .selected_task_project_id()
                    .or_else(|| self.task_projects.first().map(|project| project.id.clone()));

                if let Some(project_id) = project_id {
                    self.create_task_project_session(&project_id);
                    return self.persist_workspace_snapshot_task();
                } else {
                    self.status_message = String::from("当前没有可承载会话的项目节点。");
                }
                Task::none()
            }
            Message::TaskProjectHovered(project_id) => {
                if project_id.is_none() && self.dragging_task_project.is_none() {
                    self.active_task_project_drop_target = None;
                }
                self.hovered_task_project = if self.dragging_task_project.is_some() {
                    None
                } else {
                    project_id
                };
                Task::none()
            }
            Message::TaskProjectDropPreviewMoved(project_id, position) => {
                if let Some(dragged_project_id) = self.dragging_task_project.as_ref() {
                    if dragged_project_id == &project_id {
                        self.active_task_project_drop_target = None;
                        return Task::none();
                    }

                    let dragging_section_is_pinned = self
                        .task_project_by_id(dragged_project_id)
                        .map(|project| project.pinned);
                    let hovered_section_is_pinned = self
                        .task_project_by_id(&project_id)
                        .map(|project| project.pinned);

                    if dragging_section_is_pinned != hovered_section_is_pinned {
                        self.active_task_project_drop_target = None;
                        return Task::none();
                    }

                    let drop_target = if position.y <= MAIN_TASK_PROJECT_ROW_HEIGHT / 2.0 {
                        Some(TaskProjectDropTarget::Before(project_id.clone()))
                    } else if let Some(next_project_id) =
                        self.next_task_project_in_section(&project_id)
                    {
                        if next_project_id == *dragged_project_id {
                            None
                        } else {
                            Some(TaskProjectDropTarget::Before(next_project_id))
                        }
                    } else {
                        self.task_project_by_id(&project_id).map(|project| {
                            if project.pinned {
                                TaskProjectDropTarget::EndOfPinned
                            } else {
                                TaskProjectDropTarget::EndOfRegular
                            }
                        })
                    };

                    self.active_task_project_drop_target = drop_target;
                }
                Task::none()
            }
            Message::TaskProjectSectionTailHovered(section_is_pinned) => {
                if let (Some(section_is_pinned), Some(dragged_project_id)) =
                    (section_is_pinned, self.dragging_task_project.clone())
                {
                    let dragging_section_is_pinned = self
                        .task_project_by_id(&dragged_project_id)
                        .map(|project| project.pinned);

                    if dragging_section_is_pinned == Some(section_is_pinned) {
                        self.active_task_project_drop_target = Some(if section_is_pinned {
                            TaskProjectDropTarget::EndOfPinned
                        } else {
                            TaskProjectDropTarget::EndOfRegular
                        });
                    }
                } else if self.dragging_task_project.is_none() {
                    self.active_task_project_drop_target = None;
                }
                Task::none()
            }
            Message::TaskSessionHovered(session_id) => {
                if self.dragging_task_project.is_some() {
                    self.hovered_task_session = None;
                    return Task::none();
                }

                if session_id.is_none() {
                    self.suppress_task_session_hover = false;
                    self.hovered_task_session = None;
                    return Task::none();
                }

                if self.suppress_task_session_hover {
                    return Task::none();
                }
                self.hovered_task_session = session_id;
                Task::none()
            }
            Message::StartTaskProjectInteraction(project_id) => {
                if let Some(position) = self.main_window_cursor_position {
                    self.pending_task_project_interaction = Some(PendingTaskProjectInteraction {
                        project_id,
                        origin: position,
                    });
                }
                Task::none()
            }
            Message::OpenTaskProjectDirectory(project_id) => {
                if let Some(project) = self.task_project_by_id(&project_id) {
                    match open_path_in_system_file_manager(&project.path) {
                        Ok(()) => {
                            self.status_message =
                                format!("已请求打开项目目录：{}。", project.title);
                        }
                        Err(error) => {
                            self.status_message =
                                format!("打开项目目录 `{}` 失败：{error}", project.title);
                        }
                    }
                }
                Task::none()
            }
            Message::CreateTaskProjectSession(project_id) => {
                self.create_task_project_session(&project_id);
                self.persist_workspace_snapshot_task()
            }
            Message::AddTaskProjectFromFolder => {
                match pick_task_project_folder(self.workspace_path.trim()) {
                    Ok(Some(selected_path)) => {
                        self.add_task_project_from_folder(&selected_path);
                        return Task::batch([
                            self.persist_workspace_snapshot_task(),
                            self.schedule_project_temp_cleanup_for_paths(vec![
                                selected_path.clone(),
                            ]),
                        ]);
                    }
                    Ok(None) => {
                        self.status_message = String::from("已取消从文件夹添加项目。");
                    }
                    Err(error) => {
                        self.status_message = format!("从文件夹添加项目失败：{error}");
                    }
                }
                Task::none()
            }
            Message::PinTaskProject(project_id) => {
                if let Some(project) = self.task_project_by_id_mut(&project_id) {
                    project.pinned = !project.pinned;
                    let title = project.title.clone();
                    let pinned = project.pinned;
                    self.sort_task_projects();
                    self.status_message = if pinned {
                        format!("已将项目 `{title}` 置顶。")
                    } else {
                        format!("已取消项目 `{title}` 的置顶状态。")
                    };
                }
                self.persist_workspace_snapshot_task()
            }
            Message::RenameTaskProject(project_id) => {
                if let Some(project) = self.task_project_by_id_mut(&project_id) {
                    let original_title = project.title.clone();
                    if project.title.ends_with("（重命名）") {
                        project.title = project.title.trim_end_matches("（重命名）").to_string();
                    } else {
                        project.title = format!("{}（重命名）", project.title);
                    }
                    let renamed_title = project.title.clone();
                    self.status_message =
                        format!("已示例性重命名项目：`{original_title}` -> `{renamed_title}`。");
                }
                self.persist_workspace_snapshot_task()
            }
            Message::ArchiveTaskProject(project_id) => {
                if let Some(index) = self
                    .task_projects
                    .iter()
                    .position(|project| project.id == project_id)
                {
                    let project = self.task_projects.remove(index);
                    self.drop_unmaterialized_sessions_for_project(&project.id);
                    let archived_session_ids = project.session_ids.clone();
                    for session_id in &archived_session_ids {
                        if let Some(session_index) = self
                            .sessions
                            .iter()
                            .position(|session| session.id == *session_id)
                        {
                            let session = self.sessions.remove(session_index);
                            self.archived_sessions.push(session);
                        }
                        if let Some(messages) = self.session_messages.remove(session_id) {
                            self.archived_session_messages.insert(*session_id, messages);
                        }
                        if let Some(compactions) = self.session_compactions.remove(session_id) {
                            self.archived_session_compactions
                                .insert(*session_id, compactions);
                        }
                        if let Some(runtime_state) = self.session_runtime_state.remove(session_id) {
                            self.archived_session_runtime_state
                                .insert(*session_id, runtime_state);
                        }
                        self.session_runtime_events.remove(session_id);
                        self.session_run_phases.remove(session_id);
                    }
                    self.archived_task_projects.push(project.clone());
                    self.expanded_task_projects.remove(&project.id);
                    self.hovered_task_project = None;
                    self.hovered_task_session = None;
                    self.active_task_project_drop_target = None;
                    self.pending_task_project_interaction = None;
                    self.dragging_task_project = None;
                    self.ensure_valid_selected_session();
                    self.status_message = format!("已归档项目 `{}`。", project.title);
                }
                self.persist_workspace_snapshot_task()
            }
            Message::RestoreTaskProject(project_id) => {
                self.pending_archived_delete_confirmation = None;
                let _ = self.restore_archived_task_project_by_id(&project_id);
                let mut tasks = vec![self.persist_workspace_snapshot_task()];
                if let Some(project_path) = self
                    .task_project_by_id(&project_id)
                    .map(|project| PathBuf::from(project.path.clone()))
                {
                    tasks.push(self.schedule_project_temp_cleanup_for_paths(vec![project_path]));
                }
                Task::batch(tasks)
            }
            Message::RenameTaskSession(session_id) => {
                if let Some(session) = self
                    .sessions
                    .iter_mut()
                    .find(|session| session.id == session_id)
                {
                    let original_title = session.title.clone();
                    if session.title.ends_with("（重命名）") {
                        session.title = session.title.trim_end_matches("（重命名）").to_string();
                    } else {
                        session.title = format!("{}（重命名）", session.title);
                    }
                    let renamed_title = session.title.clone();
                    self.status_message =
                        format!("已示例性重命名会话：`{original_title}` -> `{renamed_title}`。");
                }
                self.persist_workspace_snapshot_task()
            }
            Message::ArchiveTaskSession(session_id) => {
                let archived_owner_snapshot = self.task_projects.iter().find_map(|project| {
                    project
                        .session_ids
                        .iter()
                        .position(|id| *id == session_id)
                        .map(|session_index| (project.id.clone(), session_index))
                });
                let archived_title = self
                    .session_by_id(session_id)
                    .map(|session| session.title.clone())
                    .unwrap_or_else(|| format!("会话 #{session_id}"));
                if let Some(index) = self
                    .sessions
                    .iter()
                    .position(|session| session.id == session_id)
                {
                    let session = self.sessions.remove(index);
                    self.archived_sessions.push(session);
                }
                if let Some(messages) = self.session_messages.remove(&session_id) {
                    self.archived_session_messages.insert(session_id, messages);
                }
                if let Some(compactions) = self.session_compactions.remove(&session_id) {
                    self.archived_session_compactions
                        .insert(session_id, compactions);
                }
                if let Some(runtime_state) = self.session_runtime_state.remove(&session_id) {
                    self.archived_session_runtime_state
                        .insert(session_id, runtime_state);
                }
                self.session_runtime_events.remove(&session_id);
                self.session_run_phases.remove(&session_id);
                for project in &mut self.task_projects {
                    project.session_ids.retain(|id| *id != session_id);
                }
                self.hovered_task_session =
                    archived_owner_snapshot
                        .as_ref()
                        .and_then(|(project_id, removed_index)| {
                            self.task_project_by_id(project_id).and_then(|project| {
                                project
                                    .session_ids
                                    .get(*removed_index)
                                    .copied()
                                    .or_else(|| project.session_ids.last().copied())
                            })
                        });
                self.suppress_task_session_hover = false;
                self.ensure_valid_selected_session();
                self.status_message = format!("已归档会话 `{archived_title}`。");
                self.persist_workspace_snapshot_task()
            }
            Message::RestoreTaskSession(session_id) => {
                self.pending_archived_delete_confirmation = None;
                let _ = self.restore_archived_task_session(session_id);
                self.persist_workspace_snapshot_task()
            }
            Message::RequestPermanentDeleteArchivedTaskProject(project_id) => {
                self.pending_archived_delete_confirmation = self
                    .archived_task_projects
                    .iter()
                    .find(|project| project.id == project_id)
                    .map(|project| ArchivedDeleteConfirmationState {
                        target: ArchivedDeleteTarget::Project(project.id.clone()),
                        title: project.title.clone(),
                    });
                if self.pending_archived_delete_confirmation.is_some() {
                    self.status_message = String::from("请再次确认是否彻底删除该归档项目。");
                }
                Task::none()
            }
            Message::RequestPermanentDeleteArchivedTaskSession(session_id) => {
                self.pending_archived_delete_confirmation = self
                    .archived_sessions
                    .iter()
                    .find(|session| session.id == session_id)
                    .map(|session| ArchivedDeleteConfirmationState {
                        target: ArchivedDeleteTarget::Session(session.id),
                        title: session.title.clone(),
                    });
                if self.pending_archived_delete_confirmation.is_some() {
                    self.status_message = String::from("请再次确认是否彻底删除该归档会话。");
                }
                Task::none()
            }
            Message::CancelPermanentDeleteArchivedItem => {
                self.pending_archived_delete_confirmation = None;
                self.status_message = String::from("已取消归档项彻底删除。");
                Task::none()
            }
            Message::ConfirmPermanentDeleteArchivedItem => {
                let Some(confirmation) = self.pending_archived_delete_confirmation.clone() else {
                    return Task::none();
                };
                self.pending_archived_delete_confirmation = None;
                self.status_message = format!("正在彻底删除 `{}`。", confirmation.title);
                self.permanently_delete_archived_item_task(confirmation)
            }
            Message::PermanentDeleteArchivedItemFinished(result) => {
                match result {
                    Ok(outcome) => {
                        self.apply_archived_delete_outcome(&outcome);
                        self.status_message = format!("已彻底删除 `{}`。", outcome.title);
                    }
                    Err(error) => {
                        self.status_message = format!("彻底删除归档项失败：{error}");
                    }
                }
                Task::none()
            }
            Message::ScanWorkspace => {
                // Load the current workspace diff synchronously for the first testing milestone.
                // 为首个测试里程碑同步载入当前工作区的 diff 结果。
                let diff_preview = load_workspace_diff_preview(&self.workspace_path);

                self.diff_summary = diff_preview.summary.clone();
                self.diff_panel = diff_preview.panel.clone();
                self.diff_scroll_offset = 0.0;
                self.hovered_diff_line = None;
                let available_diff_file_keys = self
                    .diff_panel
                    .files
                    .iter()
                    .map(DiffFile::key)
                    .collect::<BTreeSet<_>>();
                self.collapsed_diff_files
                    .retain(|key| available_diff_file_keys.contains(key));
                self.rebuild_diff_code_buffers();
                self.status_message = diff_preview.status_message.clone();

                // Reflect the detected change count into the selected session card so the left queue stays informative.
                // 将检测到的变更数量回写到当前会话卡片，保证左侧队列信息同步更新。
                if let Some(session) = self.selected_session_mut() {
                    session.changed_files = diff_preview.changed_files.min(u8::MAX as usize) as u8;
                }

                self.push_activity(
                    diff_preview.activity_level,
                    "Git Diff 扫描完成",
                    format!(
                        "目标路径：{}；摘要：{}。",
                        self.workspace_path, diff_preview.summary
                    ),
                );
                Task::none()
            }
            Message::ToggleDiffFile(file_key) => {
                self.toggle_diff_file_collapsed(&file_key);
                Task::none()
            }
            Message::DiffPanelScrolled(viewport) => {
                self.diff_scroll_offset = viewport.absolute_offset().y;
                self.diff_viewport_height = viewport.bounds().height.max(0.0);
                self.refresh_visible_diff_buffers();
                Task::none()
            }
            Message::ToggleAndFocusDiffFile(file_key) => {
                let target_offset = self
                    .diff_file_scroll_offset(&file_key)
                    .unwrap_or(self.diff_scroll_offset);
                self.toggle_diff_file_collapsed(&file_key);
                self.diff_scroll_offset = target_offset;
                self.refresh_visible_diff_buffers();

                operation::scroll_to(
                    DIFF_PANEL_SCROLLABLE_ID,
                    scrollable::AbsoluteOffset {
                        x: 0.0,
                        y: target_offset,
                    },
                )
            }
            Message::DiffCodeAction(buffer_key, action) => {
                if matches!(action, text_editor::Action::Scroll { .. }) {
                    return Task::none();
                }

                if !action.is_edit() {
                    if let Some(content) = self.diff_code_buffers.get_mut(&buffer_key) {
                        content.perform(action);
                    }
                }
                Task::none()
            }
            Message::DiffCodeHoverMoved(file_key, line_index) => {
                self.hovered_diff_line = Some(HoveredDiffLine {
                    file_key,
                    line_index,
                });
                Task::none()
            }
            Message::DiffCodeHoverExited(file_key) => {
                if self
                    .hovered_diff_line
                    .as_ref()
                    .is_some_and(|hovered| hovered.file_key == file_key)
                {
                    self.hovered_diff_line = None;
                }
                Task::none()
            }
            Message::StartRun => self.submit_selected_session_prompt(false),
            Message::PendingPromptQueueInsertNow(entry_id) => {
                self.dispatch_pending_prompt_entry_now(&entry_id)
            }
            Message::PendingPromptQueueEdit(entry_id) => self.edit_pending_prompt_entry(&entry_id),
            Message::PendingPromptQueueRemove(entry_id) => {
                self.remove_pending_prompt_entry(&entry_id)
            }
            Message::StartPendingPromptQueueDrag(entry_id) => {
                let Some(session_id) = self.selected_session().map(|session| session.id) else {
                    return Task::none();
                };
                if let Some(position) = self.main_window_cursor_position {
                    self.pending_pending_prompt_drag = Some(PendingPendingPromptDrag {
                        session_id,
                        entry_id,
                        origin: position,
                    });
                }
                Task::none()
            }
            Message::PendingPromptQueueDropPreviewMoved(target_entry_id, position) => {
                let Some(session_id) = self.selected_session().map(|session| session.id) else {
                    return Task::none();
                };
                let Some(dragged_entry_id) = self.dragging_pending_prompt_entry_id.as_ref() else {
                    return Task::none();
                };
                if let Some(pending_drag) = self.pending_pending_prompt_drag.as_ref() {
                    if pending_drag.session_id != session_id {
                        return Task::none();
                    }
                }
                if dragged_entry_id == &target_entry_id {
                    self.active_pending_prompt_drop_target =
                        Some(PendingPromptDropTarget::Before(target_entry_id));
                    return Task::none();
                }
                self.active_pending_prompt_drop_target =
                    Some(if position.y <= COMPOSER_PENDING_QUEUE_ROW_HEIGHT / 2.0 {
                        PendingPromptDropTarget::Before(target_entry_id)
                    } else {
                        let queue_entries = self.session_pending_prompt_panel_queue(session_id);
                        let target_index = queue_entries
                            .iter()
                            .position(|entry| entry.id == target_entry_id)
                            .unwrap_or(queue_entries.len());
                        let next_entry_id = queue_entries
                            .get(target_index.saturating_add(1))
                            .map(|entry| entry.id.clone());
                        if let Some(next_entry_id) = next_entry_id {
                            if next_entry_id == *dragged_entry_id {
                                PendingPromptDropTarget::Before(target_entry_id)
                            } else {
                                PendingPromptDropTarget::Before(next_entry_id)
                            }
                        } else {
                            PendingPromptDropTarget::End
                        }
                    });
                Task::none()
            }
            Message::PendingPromptQueueTailHovered(is_hovered) => {
                if !is_hovered {
                    if self.dragging_pending_prompt_entry_id.is_none() {
                        self.active_pending_prompt_drop_target = None;
                    }
                    return Task::none();
                }
                if self.dragging_pending_prompt_entry_id.is_some() {
                    self.active_pending_prompt_drop_target = Some(PendingPromptDropTarget::End);
                }
                Task::none()
            }
            Message::SessionRuntimeProgressTick => {
                self.runtime_progress_animation_frame =
                    self.runtime_progress_animation_frame.wrapping_add(1);
                Task::none()
            }
            Message::RunExecutionStreamEvent { run_id, event } => {
                let Some(session_id) = self.active_run_session_id_for_run_id(run_id) else {
                    return Task::none();
                };

                match event {
                    ExecutionStreamEvent::Prepared(preparation) => {
                        let detail = preparation.summary_rows().join(" | ");
                        self.replace_session_last_api_request_preview(
                            preparation.session_id,
                            preparation.provider_request_preview_markdown.clone(),
                        );
                        self.refresh_prompt_composer_inspector_buffer();
                        self.sync_context_usage_from_preparation(Some(&preparation));
                        self.update_session_runtime_identity_from_preparation(
                            preparation.session_id,
                            &preparation,
                        );
                        self.last_execution_preparation = Some(preparation);
                        self.status_message =
                            String::from("真实执行已开始，正在分离思考与输出并持续写入工作台。");
                        self.push_activity(ActivityLevel::Success, "执行准备已完成", detail);
                        Task::none()
                    }
                    ExecutionStreamEvent::RequestStarted { session_id } => {
                        self.mark_session_pending_provider_response(session_id);
                        Task::none()
                    }
                    ExecutionStreamEvent::ThinkingDelta { session_id, delta } => {
                        self.clear_session_pending_provider_response(session_id);
                        self.append_streaming_thinking_delta(session_id, &delta);
                        self.request_selected_session_transcript_tail_follow(session_id);
                        self.request_selected_session_thinking_overlay_tail_follow(session_id);
                        Task::none()
                    }
                    ExecutionStreamEvent::ThinkingCompleted { session_id } => {
                        self.dock_streaming_thinking_message(session_id);
                        self.maybe_follow_selected_session_transcript_tail(session_id)
                    }
                    ExecutionStreamEvent::OutputDelta { session_id, delta } => {
                        self.clear_session_pending_provider_response(session_id);
                        self.append_streaming_output_delta(session_id, &delta);
                        self.request_selected_session_transcript_tail_follow(session_id);
                        Task::none()
                    }
                    ExecutionStreamEvent::RuntimeEvent { session_id, event } => {
                        self.clear_session_pending_provider_response(session_id);
                        if is_provider_tool_boundary_runtime_event(&event) {
                            self.dock_streaming_thinking_message(session_id);
                        }
                        let detached_listener_task =
                            self.maybe_start_detached_subagent_task_listener(session_id, &event);
                        self.push_session_runtime_event(session_id, event);
                        Task::batch([
                            detached_listener_task,
                            self.maybe_follow_selected_session_transcript_tail(session_id),
                            self.maybe_start_next_pending_prompt(session_id, false),
                        ])
                    }
                    ExecutionStreamEvent::Finished(report) => {
                        self.clear_active_run_tracking(session_id);
                        self.apply_execution_run_report(report)
                    }
                }
            }
            Message::SessionPromptCompactionFinished {
                run_id,
                session_id,
                trigger,
                budget_note,
                plan,
                result,
            } => {
                let Some(active_session_id) = self.active_compaction_session_id_for_run_id(run_id)
                else {
                    return Task::none();
                };
                if active_session_id != session_id {
                    return Task::none();
                }
                let Some(active_state) = self.take_active_compaction_state(session_id) else {
                    return Task::none();
                };

                match result {
                    Ok(execution) => self.finish_session_prompt_compaction(
                        session_id,
                        trigger,
                        budget_note,
                        active_state.previous_phase,
                        execution,
                    ),
                    Err(error) => {
                        self.push_activity(
                            ActivityLevel::Warning,
                            "提示词压缩回退",
                            format!(
                                "会话 #{session_id} 的提示词式压缩未返回可用总结，已回退到宿主侧摘要：{error}"
                            ),
                        );
                        self.finish_session_prompt_compaction(
                            session_id,
                            trigger,
                            budget_note,
                            active_state.previous_phase,
                            PromptCompactionExecution {
                                compaction_result: CompactionSkillResult::from_legacy_plan(&plan),
                                fallback_used: true,
                                provider_request_count: 0,
                            },
                        )
                    }
                }
            }
            Message::DetachedSubagentTaskEvent { task_id, event } => match event {
                DetachedTaskRuntimeEvent::ChildExecution(event) => match event {
                    ExecutionStreamEvent::Prepared(_) => Task::none(),
                    ExecutionStreamEvent::RequestStarted { session_id } => {
                        self.mark_session_pending_provider_response(session_id);
                        Task::none()
                    }
                    ExecutionStreamEvent::ThinkingDelta { session_id, delta } => {
                        self.clear_session_pending_provider_response(session_id);
                        self.append_streaming_thinking_delta(session_id, &delta);
                        self.request_selected_session_transcript_tail_follow(session_id);
                        self.request_selected_session_thinking_overlay_tail_follow(session_id);
                        Task::none()
                    }
                    ExecutionStreamEvent::ThinkingCompleted { session_id } => {
                        self.dock_streaming_thinking_message(session_id);
                        self.maybe_follow_selected_session_transcript_tail(session_id)
                    }
                    ExecutionStreamEvent::OutputDelta { session_id, delta } => {
                        self.clear_session_pending_provider_response(session_id);
                        self.append_streaming_output_delta(session_id, &delta);
                        self.request_selected_session_transcript_tail_follow(session_id);
                        Task::none()
                    }
                    ExecutionStreamEvent::RuntimeEvent { session_id, event } => {
                        self.clear_session_pending_provider_response(session_id);
                        if is_provider_tool_boundary_runtime_event(&event) {
                            self.dock_streaming_thinking_message(session_id);
                        }
                        self.push_session_runtime_event(session_id, event);
                        self.maybe_follow_selected_session_transcript_tail(session_id)
                    }
                    ExecutionStreamEvent::Finished(report) => {
                        self.apply_execution_run_report(report)
                    }
                },
                DetachedTaskRuntimeEvent::ParentRuntime {
                    parent_session_id,
                    event,
                } => {
                    let is_finished = event.kind == RuntimeEventKind::DelegateFinished;
                    self.push_session_runtime_event(parent_session_id, event);
                    if is_finished {
                        self.detached_subagent_task_listeners.remove(&task_id);
                        self.refresh_subagent_navigation_round_visibility(parent_session_id);
                    }
                    Task::batch([
                        self.persist_workspace_snapshot_task(),
                        self.maybe_follow_selected_session_transcript_tail(parent_session_id),
                        self.maybe_start_next_pending_prompt(parent_session_id, false),
                    ])
                }
            },
            Message::StopRun => {
                let Some(session_id) = self.selected_session().map(|session| session.id) else {
                    self.status_message = String::from("当前没有正在执行的 AI 调用。");
                    return Task::none();
                };
                self.stop_all_session_work(session_id)
            }
            Message::PaneFocused(pane) => {
                self.focus = Some(pane);
                Task::none()
            }
            Message::PaneResized(event) => {
                self.panes.resize(event.split, event.ratio);
                self.refresh_visible_diff_buffers();
                Task::none()
            }
        }
    }
}
