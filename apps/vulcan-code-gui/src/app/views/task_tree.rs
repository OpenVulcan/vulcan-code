//! Task-tree view composition methods extracted from the main workbench views module.
//! 从主工作台视图模块中抽取的任务树相关界面组合方法。

use super::super::helpers::*;
use super::*;
use iced::widget::column;

impl VulcanWorkbench {
    /// Builds the tree-shaped workspace and session outline shown inside the main task drawer.
    /// 构建展示在主任务抽屉中的树形工作空间与会话大纲。
    pub(super) fn view_main_task_session_tree(&self) -> Element<'_, Message> {
        let mut tree = column![].spacing(1).width(Fill);
        let pinned_projects = self.task_projects.iter().filter(|project| project.pinned);
        let regular_projects = self.task_projects.iter().filter(|project| !project.pinned);

        let has_pinned = self.task_projects.iter().any(|project| project.pinned);

        if has_pinned {
            tree = tree
                .push(self.view_main_task_group_label("置顶", false))
                .push(
                    pinned_projects.fold(column![].spacing(1).width(Fill), |column, project| {
                        column.push(self.view_main_task_project_tree_row(project))
                    }),
                )
                .push(self.view_main_task_project_section_tail(true))
                .push(space::vertical().height(6));
        }

        tree = tree
            .push(self.view_main_task_group_label("项目", true))
            .push(
                regular_projects.fold(column![].spacing(1).width(Fill), |column, project| {
                    column.push(self.view_main_task_project_tree_row(project))
                }),
            );

        tree = tree.push(self.view_main_task_project_section_tail(false));

        container(tree).width(Fill).into()
    }

    /// Builds the inline destructive second-confirmation card shown before one archived item is permanently deleted.
    /// 构建在某个归档项被永久删除前展示的内联破坏性二次确认卡片。
    pub(super) fn view_archived_delete_confirmation_card(
        &self,
        confirmation: &ArchivedDeleteConfirmationState,
    ) -> Element<'_, Message> {
        let subject_label = match &confirmation.target {
            ArchivedDeleteTarget::Project(_) => "归档项目",
            ArchivedDeleteTarget::Session(_) => "归档会话",
        };

        container(
            column![
                text(format!(
                    "将彻底删除{} `{}`，此操作不可恢复。",
                    subject_label, confirmation.title
                ))
                .size(13),
                row![
                    button("取消")
                        .padding([8, 12])
                        .style(button::secondary)
                        .on_press(Message::CancelPermanentDeleteArchivedItem),
                    button("确认删除")
                        .padding([8, 12])
                        .style(button::danger)
                        .on_press(Message::ConfirmPermanentDeleteArchivedItem),
                ]
                .spacing(8)
                .align_y(Center),
            ]
            .spacing(10),
        )
        .padding(12)
        .width(Fill)
        .style(container::rounded_box)
        .into()
    }

    /// Builds one compact section label used inside the project tree.
    /// 构建展示在项目树内部的紧凑分区标签。
    pub(super) fn view_main_task_group_label(
        &self,
        label: &'static str,
        show_add_project_button: bool,
    ) -> Element<'_, Message> {
        let trailing_action: Element<'_, Message> = if show_add_project_button {
            tooltip(
                Self::view_main_task_tree_icon_action_button(
                    TaskTreeActionIcon::AddProjectFromFolder,
                    Message::AddTaskProjectFromFolder,
                    true,
                ),
                text("从文件夹添加项目"),
                tooltip::Position::Bottom,
            )
            .style(container::rounded_box)
            .into()
        } else {
            space::horizontal()
                .width(MAIN_TASK_TREE_ACTION_BUTTON_SIZE)
                .into()
        };

        container(
            row![
                text(label).size(15).style(|theme: &Theme| {
                    iced::widget::text::Style {
                        color: Some(theme.extended_palette().background.base.text),
                    }
                }),
                space::horizontal(),
                trailing_action,
            ]
            .align_y(Center),
        )
        .padding(iced::Padding {
            top: 3.0,
            right: MAIN_TASK_TREE_SECTION_LEFT_PADDING,
            bottom: 3.0,
            left: MAIN_TASK_TREE_SECTION_LEFT_PADDING,
        })
        .height(MAIN_TASK_PROJECT_ROW_HEIGHT)
        .width(Fill)
        .style(|theme| style::main_task_tree_project_row_style(theme, false, false))
        .into()
    }

    /// Builds the insertion indicator line shown before one project row while reordering.
    /// 构建在项目重排期间显示在某个项目行之前的插入指示线。
    pub(super) fn view_main_task_project_drop_indicator(
        &self,
        is_visible: bool,
    ) -> Element<'_, Message> {
        let height = if is_visible { 8.0 } else { 0.0 };
        let padding = if is_visible { 2.0 } else { 0.0 };

        container(
            row![
                container(space::horizontal())
                    .width(7.0)
                    .height(7.0)
                    .style(style::main_task_tree_drop_anchor_style),
                container(space::horizontal())
                    .height(3.0)
                    .width(Fill)
                    .style(style::main_task_tree_drop_indicator_style),
            ]
            .spacing(4)
            .align_y(Center),
        )
        .padding(iced::Padding {
            top: 0.0,
            right: 6.0,
            bottom: padding,
            left: 6.0,
        })
        .height(height)
        .width(Fill)
        .into()
    }

    /// Builds the tail drop zone shown after one project section so dragged cards can land at the end.
    /// 构建显示在项目分区末尾的尾部落点，以便拖拽卡片落到分区最后。
    pub(super) fn view_main_task_project_section_tail(
        &self,
        section_is_pinned: bool,
    ) -> Element<'_, Message> {
        let dragging_matches_section = self
            .dragging_task_project
            .as_ref()
            .and_then(|project_id| self.task_project_by_id(project_id))
            .map(|project| project.pinned == section_is_pinned)
            .unwrap_or(false);

        if !dragging_matches_section {
            return space::vertical().height(0.0).into();
        }

        let shows_drop_tail = matches!(
            self.active_task_project_drop_target.as_ref(),
            Some(TaskProjectDropTarget::EndOfPinned) if section_is_pinned
        ) || matches!(
            self.active_task_project_drop_target.as_ref(),
            Some(TaskProjectDropTarget::EndOfRegular) if !section_is_pinned
        );

        mouse_area(if shows_drop_tail {
            container(
                row![
                    container(space::horizontal())
                        .width(7.0)
                        .height(7.0)
                        .style(style::main_task_tree_drop_anchor_style),
                    container(space::horizontal())
                        .height(3.0)
                        .width(Fill)
                        .style(style::main_task_tree_drop_indicator_style),
                ]
                .spacing(4)
                .align_y(Center),
            )
            .padding(iced::Padding {
                top: 2.0,
                right: 6.0,
                bottom: 0.0,
                left: 6.0,
            })
            .height(10.0)
            .width(Fill)
        } else {
            container(space::horizontal()).height(10.0).width(Fill)
        })
        .interaction(if self.dragging_task_project.is_some() {
            iced::mouse::Interaction::Move
        } else {
            iced::mouse::Interaction::default()
        })
        .on_enter(Message::TaskProjectSectionTailHovered(Some(
            section_is_pinned,
        )))
        .on_exit(Message::TaskProjectSectionTailHovered(None))
        .into()
    }

    /// Builds one project branch shown inside the main task drawer tree.
    /// 构建显示在主任务抽屉树中的单个项目分支。
    pub(super) fn view_main_task_project_tree_row<'a>(
        &'a self,
        project: &'a TaskProjectSummary,
    ) -> Element<'a, Message> {
        let is_expanded = self.expanded_task_projects.contains(&project.id);
        let shows_drop_before = matches!(
            self.active_task_project_drop_target.as_ref(),
            Some(TaskProjectDropTarget::Before(target_project_id)) if target_project_id == &project.id
        );
        let mut section = column![]
            .spacing(0)
            .width(Fill)
            .push(self.view_main_task_project_drop_indicator(shows_drop_before))
            .push(self.view_main_task_project_row(project, is_expanded));

        if is_expanded {
            section = section.push(
                container(self.view_main_task_project_children(project))
                    .padding(iced::Padding {
                        top: 2.0,
                        right: 2.0,
                        bottom: 8.0,
                        left: 1.0,
                    })
                    .width(Fill),
            );
        }

        container(section).width(Fill).into()
    }

    /// Builds the clickable project row displayed at the top of one tree branch.
    /// 构建显示在树分支顶部的可点击项目行。
    pub(super) fn view_main_task_project_row<'a>(
        &'a self,
        project: &'a TaskProjectSummary,
        is_expanded: bool,
    ) -> Element<'a, Message> {
        let is_active = self.dragging_task_project.is_none()
            && self.hovered_task_project.as_deref() == Some(project.id.as_str());
        let is_drag_source = self.dragging_task_project.as_deref() == Some(project.id.as_str());
        let source_alpha = if is_drag_source { 0.46 } else { 1.0 };
        let project_title =
            truncate_with_ellipsis(&project.title, self.main_task_project_title_limit_width());
        let project_label = text(project_title)
            .size(15)
            .width(Fill)
            .wrapping(iced::advanced::text::Wrapping::None)
            .style(move |theme: &Theme| iced::widget::text::Style {
                color: Some(
                    theme
                        .extended_palette()
                        .background
                        .base
                        .text
                        .scale_alpha(source_alpha),
                ),
            });
        let folder_icon = if is_expanded {
            codicon::folder_opened()
        } else {
            codicon::folder()
        }
        .size(16)
        .line_height(1.0)
        .style(move |theme: &Theme| iced::widget::text::Style {
            color: Some(
                theme
                    .extended_palette()
                    .background
                    .base
                    .text
                    .scale_alpha(0.86 * source_alpha),
            ),
        });
        let state_arrow: Element<'a, Message> = if is_expanded {
            codicon::chevron_down()
        } else {
            codicon::chevron_right()
        }
        .size(14)
        .line_height(1.0)
        .style(move |theme: &Theme| iced::widget::text::Style {
            color: Some(
                theme
                    .extended_palette()
                    .background
                    .base
                    .text
                    .scale_alpha(0.72 * source_alpha),
            ),
        })
        .into();

        let header = container(
            row![
                folder_icon,
                project_label,
                container(Self::view_main_task_tree_icon_action_button(
                    TaskTreeActionIcon::AddSession,
                    Message::CreateTaskProjectSession(project.id.clone()),
                    true,
                ))
                .width(MAIN_TASK_PROJECT_ACTIONS_WIDTH)
                .center_y(Fill),
                container(state_arrow)
                    .width(MAIN_TASK_PROJECT_ARROW_WIDTH)
                    .padding(iced::Padding {
                        top: 0.0,
                        right: 2.0,
                        bottom: 0.0,
                        left: 0.0,
                    })
                    .center_y(Fill),
            ]
            .spacing(8)
            .align_y(Center),
        )
        .padding(iced::Padding {
            top: 3.0,
            right: MAIN_TASK_TREE_SECTION_LEFT_PADDING,
            bottom: 3.0,
            left: MAIN_TASK_TREE_SECTION_LEFT_PADDING,
        })
        .height(MAIN_TASK_PROJECT_ROW_HEIGHT)
        .width(Fill)
        .style(move |theme| {
            style::main_task_tree_project_row_style(theme, is_active, is_drag_source)
        });

        mouse_area(header)
            .interaction(if self.dragging_task_project.is_some() {
                iced::mouse::Interaction::Move
            } else {
                iced::mouse::Interaction::default()
            })
            .on_press(Message::StartTaskProjectInteraction(project.id.clone()))
            .on_right_press(Message::OpenTaskProjectContextMenu(project.id.clone()))
            .on_enter(Message::TaskProjectHovered(Some(project.id.clone())))
            .on_move({
                let project_id = project.id.clone();
                move |position| Message::TaskProjectDropPreviewMoved(project_id.clone(), position)
            })
            .on_exit(Message::TaskProjectHovered(None))
            .into()
    }

    /// Builds the expanded child rows shown beneath one project header in the task drawer tree.
    /// 构建显示在任务抽屉树某个项目头部下方的展开子行区域。
    pub(super) fn view_main_task_project_children<'a>(
        &'a self,
        project: &'a TaskProjectSummary,
    ) -> Element<'a, Message> {
        let has_visible_sessions = !project.session_ids.is_empty();

        if !has_visible_sessions {
            return container(text("暂无对话").size(13).style(|theme: &Theme| {
                iced::widget::text::Style {
                    color: Some(
                        theme
                            .extended_palette()
                            .background
                            .base
                            .text
                            .scale_alpha(0.45),
                    ),
                }
            }))
            .padding(iced::Padding {
                top: 2.0,
                right: 2.0,
                bottom: 8.0,
                left: 34.0,
            })
            .width(Fill)
            .into();
        }

        let content = project.session_ids.iter().fold(
            column![].spacing(4).width(Fill),
            |column, session_id| {
                if let Some(session) = self.session_by_id(*session_id) {
                    column.push(self.view_main_task_session_tree_row(session))
                } else {
                    column
                }
            },
        );

        content.into()
    }

    /// Builds one tree row shown under a project node inside the main task drawer.
    /// 构建显示在主任务抽屉某个项目节点下方的单条会话树节点。
    pub(super) fn view_main_task_session_tree_row<'a>(
        &'a self,
        session: &'a SessionSummary,
    ) -> Element<'a, Message> {
        let is_selected =
            self.dragging_task_project.is_none() && session.id == self.selected_session;
        let is_hovered =
            self.dragging_task_project.is_none() && self.hovered_task_session == Some(session.id);
        let compact_meta = compact_relative_label(&session.last_updated_label);
        let session_title =
            truncate_with_ellipsis(&session.title, self.main_task_session_title_limit_width());
        let session_label = text(session_title)
            .size(14)
            .width(Fill)
            .wrapping(iced::advanced::text::Wrapping::None)
            .style(|theme: &Theme| iced::widget::text::Style {
                color: Some(theme.extended_palette().background.base.text),
            });
        let session_meta: Element<'a, Message> =
            container(text(compact_meta).size(12).style(|theme: &Theme| {
                iced::widget::text::Style {
                    color: Some(
                        theme
                            .extended_palette()
                            .background
                            .base
                            .text
                            .scale_alpha(0.55),
                    ),
                }
            }))
            .width(MAIN_TASK_SESSION_TRAILING_WIDTH)
            .align_right(MAIN_TASK_SESSION_TRAILING_WIDTH)
            .into();

        let archive_action = Self::view_main_task_tree_icon_action_button(
            TaskTreeActionIcon::Archive,
            Message::ArchiveTaskSession(session.id),
            is_hovered,
        );
        let trailing_slot: Element<'a, Message> = if is_hovered {
            container(archive_action)
                .width(MAIN_TASK_SESSION_TRAILING_WIDTH)
                .align_right(MAIN_TASK_SESSION_TRAILING_WIDTH)
                .center_y(Fill)
                .into()
        } else {
            session_meta
        };

        let session_button = button(
            container(
                row![
                    Self::view_main_task_session_status_icon(session.status),
                    session_label,
                    space::horizontal(),
                    trailing_slot,
                ]
                .spacing(8)
                .align_y(Center),
            )
            .padding(iced::Padding {
                top: 3.0,
                right: 10.0,
                bottom: 3.0,
                left: 35.0,
            })
            .width(Fill),
        )
        .padding(0)
        .width(Fill)
        .style(move |theme, status| {
            style::main_task_tree_session_button_style(theme, status, is_selected, is_hovered)
        })
        .on_press(Message::SessionSelected(session.id));

        mouse_area(
            container(session_button)
                .height(MAIN_TASK_SESSION_ROW_HEIGHT)
                .width(Fill),
        )
        .on_right_press(Message::OpenTaskSessionContextMenu(session.id))
        .on_enter(Message::TaskSessionHovered(Some(session.id)))
        .on_exit(Message::TaskSessionHovered(None))
        .into()
    }

    /// Builds one archived project row that can be restored back into the visible task tree.
    /// 构建一条可恢复回可见任务树的归档项目行。
    pub(super) fn view_archived_task_project_row<'a>(
        &'a self,
        project: &'a TaskProjectSummary,
    ) -> Element<'a, Message> {
        let is_hovered = self.hovered_task_project.as_deref() == Some(project.id.as_str());
        let folder_icon = codicon::folder()
            .size(16)
            .line_height(1.0)
            .style(|theme: &Theme| iced::widget::text::Style {
                color: Some(
                    theme
                        .extended_palette()
                        .background
                        .base
                        .text
                        .scale_alpha(0.7),
                ),
            });
        let project_label = text(truncate_with_ellipsis(
            &project.title,
            self.main_task_project_title_limit_width() + 26.0,
        ))
        .size(15)
        .width(Fill)
        .wrapping(iced::advanced::text::Wrapping::None)
        .style(|theme: &Theme| iced::widget::text::Style {
            color: Some(
                theme
                    .extended_palette()
                    .background
                    .base
                    .text
                    .scale_alpha(0.74),
            ),
        });

        mouse_area(
            container(
                row![
                    folder_icon,
                    project_label,
                    text("已归档").size(12).style(|theme: &Theme| {
                        iced::widget::text::Style {
                            color: Some(
                                theme
                                    .extended_palette()
                                    .background
                                    .base
                                    .text
                                    .scale_alpha(0.45),
                            ),
                        }
                    }),
                ]
                .spacing(8)
                .align_y(Center),
            )
            .padding(iced::Padding {
                top: 3.0,
                right: MAIN_TASK_TREE_SECTION_LEFT_PADDING,
                bottom: 3.0,
                left: MAIN_TASK_TREE_SECTION_LEFT_PADDING,
            })
            .height(MAIN_TASK_PROJECT_ROW_HEIGHT)
            .width(Fill)
            .style(move |theme| style::main_task_tree_project_row_style(theme, is_hovered, false)),
        )
        .on_press(Message::RestoreTaskProject(project.id.clone()))
        .on_right_press(Message::OpenArchivedTaskProjectContextMenu(
            project.id.clone(),
        ))
        .on_enter(Message::TaskProjectHovered(Some(project.id.clone())))
        .on_exit(Message::TaskProjectHovered(None))
        .into()
    }

    /// Builds one archived session row that can be restored back into its visible project.
    /// 构建一条可恢复回其可见项目中的归档会话行。
    pub(super) fn view_archived_task_session_tree_row<'a>(
        &'a self,
        session: &'a SessionSummary,
    ) -> Element<'a, Message> {
        let is_hovered =
            self.dragging_task_project.is_none() && self.hovered_task_session == Some(session.id);
        let session_label = text(truncate_with_ellipsis(
            &session.title,
            self.main_task_session_title_limit_width() + 40.0,
        ))
        .size(14)
        .width(Fill)
        .wrapping(iced::advanced::text::Wrapping::None)
        .style(|theme: &Theme| iced::widget::text::Style {
            color: Some(
                theme
                    .extended_palette()
                    .background
                    .base
                    .text
                    .scale_alpha(0.72),
            ),
        });

        let session_button = button(
            container(
                row![
                    Self::view_main_task_session_status_icon(session.status),
                    session_label,
                    space::horizontal(),
                    text("已归档").size(12).style(|theme: &Theme| {
                        iced::widget::text::Style {
                            color: Some(
                                theme
                                    .extended_palette()
                                    .background
                                    .base
                                    .text
                                    .scale_alpha(0.45),
                            ),
                        }
                    }),
                ]
                .spacing(8)
                .align_y(Center),
            )
            .padding(iced::Padding {
                top: 3.0,
                right: 10.0,
                bottom: 3.0,
                left: 35.0,
            })
            .width(Fill),
        )
        .padding(0)
        .width(Fill)
        .style(move |theme, status| {
            style::main_task_tree_session_button_style(theme, status, false, is_hovered)
        })
        .on_press(Message::RestoreTaskSession(session.id));

        mouse_area(
            container(session_button)
                .height(MAIN_TASK_SESSION_ROW_HEIGHT)
                .width(Fill),
        )
        .on_right_press(Message::OpenArchivedTaskSessionContextMenu(session.id))
        .on_enter(Message::TaskSessionHovered(Some(session.id)))
        .on_exit(Message::TaskSessionHovered(None))
        .into()
    }

    /// Builds the small state icon shown before one session row inside the main task drawer tree.
    /// 构建显示在主任务抽屉树节点会话行前方的小型状态图标。
    pub(super) fn view_main_task_session_status_icon(
        status: SessionState,
    ) -> Element<'static, Message> {
        let icon = match status {
            SessionState::Draft => codicon::history(),
            SessionState::Running => codicon::sync(),
            SessionState::Blocked => codicon::warning(),
            SessionState::Completed => codicon::check(),
        }
        .size(14)
        .line_height(1.0);

        container(icon).width(16).center_y(Fill).into()
    }

    /// Builds one compact icon-only action button used by the task-drawer tree actions.
    /// 构建任务抽屉树动作使用的紧凑纯图标按钮。
    pub(super) fn view_main_task_tree_icon_action_button(
        icon: TaskTreeActionIcon,
        message: Message,
        is_visible: bool,
    ) -> Element<'static, Message> {
        let icon_alpha = if is_visible { 0.88 } else { 0.0 };
        let icon_widget = match icon {
            TaskTreeActionIcon::AddProjectFromFolder => lucide::folder_plus(),
            TaskTreeActionIcon::AddSession => lucide::square_pen(),
            TaskTreeActionIcon::Archive => codicon::archive(),
        }
        .size(14.5)
        .line_height(1.0)
        .style(move |theme: &Theme| iced::widget::text::Style {
            color: Some(
                theme
                    .extended_palette()
                    .background
                    .base
                    .text
                    .scale_alpha(icon_alpha),
            ),
        });

        if is_visible {
            button(
                container(icon_widget)
                    .width(MAIN_TASK_TREE_ACTION_BUTTON_SIZE)
                    .height(MAIN_TASK_TREE_ACTION_BUTTON_SIZE)
                    .center(Fill),
            )
            .padding(0)
            .width(MAIN_TASK_TREE_ACTION_BUTTON_SIZE)
            .height(MAIN_TASK_TREE_ACTION_BUTTON_SIZE)
            .style(style::subtle_panel_button_style)
            .on_press(message)
            .into()
        } else {
            container(icon_widget)
                .width(MAIN_TASK_TREE_ACTION_BUTTON_SIZE)
                .height(MAIN_TASK_TREE_ACTION_BUTTON_SIZE)
                .center(Fill)
                .into()
        }
    }
}
