//! Shared workbench context-menu helpers extracted from the main workbench module.
//! 从主工作台模块中抽取的共享右键菜单辅助实现。

use super::*;

impl VulcanWorkbench {
/// Resolves the best root-window anchor from the tracked pointer and a fallback event position.
    /// 通过已跟踪指针与备用事件坐标解析最佳根窗口锚点。
    pub(super) fn workbench_context_menu_anchor(&self, fallback: iced::Point) -> iced::Point {
        self.main_window_cursor_position.unwrap_or(fallback)
    }

    /// Opens the shared workbench context menu at the provided window-relative anchor.
    /// 在给定窗口相对锚点处打开共享工作台右键菜单。
    pub(super) fn open_workbench_context_menu(
        &mut self,
        source: WorkbenchContextMenuSource,
        anchor: iced::Point,
        width: f32,
        items: Vec<WorkbenchContextMenuItem>,
    ) {
        self.workbench_context_menu = Some(WorkbenchContextMenuState {
            source,
            anchor,
            width,
            items,
        });
    }

    /// Dismisses the shared workbench context menu and clears any composer-only clipboard snapshot.
    /// 关闭共享工作台右键菜单，并清理仅供提示词菜单使用的剪贴板快照。
    pub(super) fn dismiss_workbench_context_menu(&mut self) {
        self.workbench_context_menu = None;
        self.composer_context_clipboard_text = None;
    }

    /// Invokes one enabled item from the current shared workbench context menu.
    /// 触发当前共享工作台右键菜单中的一个启用菜单项。
    pub(super) fn invoke_workbench_context_menu_item(&mut self, index: usize) -> Task<Message> {
        let Some(menu) = self.workbench_context_menu.take() else {
            return Task::none();
        };
        let Some(message) = menu.items.get(index).and_then(|item| item.message.clone()) else {
            return Task::none();
        };

        self.update(message)
    }

    /// Builds the current composer menu items from editor state and clipboard availability.
    /// 根据编辑器状态与剪贴板可用性构建当前提示词菜单项。
    pub(super) fn composer_context_menu_items(&self) -> Vec<WorkbenchContextMenuItem> {
        let has_any_text = !self.composer_buffer.is_empty();
        let has_selection =
            composer_selection_has_content(self.composer_buffer.selection().as_deref());
        let can_paste =
            composer_clipboard_text_is_pastable(self.composer_context_clipboard_text.as_deref());
        let supports_attachments = self.active_workbench_supports_any_attachments();

        vec![
            if supports_attachments {
                WorkbenchContextMenuItem::enabled("添加附件", Message::AddComposerAttachments)
            } else {
                WorkbenchContextMenuItem::disabled("添加附件")
            },
            if supports_attachments {
                WorkbenchContextMenuItem::enabled(
                    "粘贴附件",
                    Message::PasteComposerClipboardAttachments,
                )
            } else {
                WorkbenchContextMenuItem::disabled("粘贴附件")
            },
            if has_any_text {
                WorkbenchContextMenuItem::enabled("全选", Message::ComposerContextMenuSelectAll)
            } else {
                WorkbenchContextMenuItem::disabled("全选")
            },
            if has_selection {
                WorkbenchContextMenuItem::enabled("复制", Message::ComposerContextMenuCopy)
            } else {
                WorkbenchContextMenuItem::disabled("复制")
            },
            if has_selection {
                WorkbenchContextMenuItem::enabled("剪切", Message::ComposerContextMenuCut)
            } else {
                WorkbenchContextMenuItem::disabled("剪切")
            },
            if can_paste {
                WorkbenchContextMenuItem::enabled("粘贴", Message::ComposerContextMenuPaste)
            } else {
                WorkbenchContextMenuItem::disabled("粘贴")
            },
        ]
    }

    /// Opens the composer context menu through the shared workbench menu renderer.
    /// 通过共享工作台菜单渲染器打开提示词右键菜单。
    pub(super) fn open_composer_context_menu(&mut self) -> Task<Message> {
        self.composer_context_clipboard_text = None;
        self.open_workbench_context_menu(
            WorkbenchContextMenuSource::Composer,
            self.workbench_context_menu_anchor(iced::Point::ORIGIN),
            WORKBENCH_CONTEXT_MENU_COMPACT_WIDTH,
            self.composer_context_menu_items(),
        );

        clipboard::read_text().map(Message::ComposerContextClipboardLoaded)
    }

    /// Refreshes the menu rows if the currently open shared menu belongs to the composer.
    /// 当当前已打开共享菜单属于提示词输入区时刷新菜单行。
    pub(super) fn refresh_open_composer_context_menu(&mut self) {
        let Some(menu) = self.workbench_context_menu.as_ref() else {
            return;
        };
        if menu.source != WorkbenchContextMenuSource::Composer {
            return;
        }
        let anchor = menu.anchor;
        self.open_workbench_context_menu(
            WorkbenchContextMenuSource::Composer,
            anchor,
            WORKBENCH_CONTEXT_MENU_COMPACT_WIDTH,
            self.composer_context_menu_items(),
        );
    }

    /// Opens the shared context menu for one visible project row.
    /// 为一条可见项目行打开共享右键菜单。
    pub(super) fn open_task_project_context_menu(&mut self, project_id: String) -> Task<Message> {
        let Some(project) = self
            .task_projects
            .iter()
            .find(|project| project.id == project_id)
        else {
            return Task::none();
        };
        let pin_label = if project.pinned {
            "取消置顶"
        } else {
            "置顶"
        };
        let items = vec![
            WorkbenchContextMenuItem::enabled(
                "从资源管理器中打开",
                Message::OpenTaskProjectDirectory(project.id.clone()),
            ),
            WorkbenchContextMenuItem::enabled(
                pin_label,
                Message::PinTaskProject(project.id.clone()),
            ),
            WorkbenchContextMenuItem::enabled(
                "重命名",
                Message::RenameTaskProject(project.id.clone()),
            ),
            WorkbenchContextMenuItem::enabled(
                "项目设置",
                Message::OpenTaskProjectSettings(project.id.clone()),
            ),
            WorkbenchContextMenuItem::enabled(
                "归档",
                Message::ArchiveTaskProject(project.id.clone()),
            ),
        ];
        self.open_workbench_context_menu(
            WorkbenchContextMenuSource::TaskTree,
            self.workbench_context_menu_anchor(iced::Point::ORIGIN),
            WORKBENCH_CONTEXT_MENU_DEFAULT_WIDTH,
            items,
        );
        Task::none()
    }

    /// Opens the shared context menu for one visible session row.
    /// 为一条可见会话行打开共享右键菜单。
    pub(super) fn open_task_session_context_menu(&mut self, session_id: SessionId) -> Task<Message> {
        if !self.sessions.iter().any(|session| session.id == session_id) {
            return Task::none();
        }
        self.open_workbench_context_menu(
            WorkbenchContextMenuSource::TaskTree,
            self.workbench_context_menu_anchor(iced::Point::ORIGIN),
            WORKBENCH_CONTEXT_MENU_DEFAULT_WIDTH,
            vec![
                WorkbenchContextMenuItem::enabled("重命名", Message::RenameTaskSession(session_id)),
                WorkbenchContextMenuItem::enabled("归档", Message::ArchiveTaskSession(session_id)),
            ],
        );
        Task::none()
    }

    /// Opens the shared context menu for one archived project row.
    /// 为一条已归档项目行打开共享右键菜单。
    pub(super) fn open_archived_task_project_context_menu(&mut self, project_id: String) -> Task<Message> {
        let Some(project) = self
            .archived_task_projects
            .iter()
            .find(|project| project.id == project_id)
        else {
            return Task::none();
        };
        self.open_workbench_context_menu(
            WorkbenchContextMenuSource::TaskTree,
            self.workbench_context_menu_anchor(iced::Point::ORIGIN),
            WORKBENCH_CONTEXT_MENU_DEFAULT_WIDTH,
            vec![
                WorkbenchContextMenuItem::enabled(
                    "恢复",
                    Message::RestoreTaskProject(project.id.clone()),
                ),
                WorkbenchContextMenuItem::enabled(
                    "彻底删除",
                    Message::RequestPermanentDeleteArchivedTaskProject(project.id.clone()),
                ),
                WorkbenchContextMenuItem::enabled(
                    "从资源管理器中打开",
                    Message::OpenTaskProjectDirectory(project.id.clone()),
                ),
            ],
        );
        Task::none()
    }

    /// Opens the shared context menu for one archived session row.
    /// 为一条已归档会话行打开共享右键菜单。
    pub(super) fn open_archived_task_session_context_menu(&mut self, session_id: SessionId) -> Task<Message> {
        if !self
            .archived_sessions
            .iter()
            .any(|session| session.id == session_id)
        {
            return Task::none();
        }
        self.open_workbench_context_menu(
            WorkbenchContextMenuSource::TaskTree,
            self.workbench_context_menu_anchor(iced::Point::ORIGIN),
            WORKBENCH_CONTEXT_MENU_DEFAULT_WIDTH,
            vec![
                WorkbenchContextMenuItem::enabled("恢复", Message::RestoreTaskSession(session_id)),
                WorkbenchContextMenuItem::enabled(
                    "彻底删除",
                    Message::RequestPermanentDeleteArchivedTaskSession(session_id),
                ),
            ],
        );
        Task::none()
    }

    /// Opens the shared context menu requested by the rendered transcript surface.
    /// 打开渲染转录表面请求的共享右键菜单。
    pub(super) fn open_transcript_context_menu(
        &mut self,
        request: SelectableTranscriptContextMenuRequest,
    ) -> Task<Message> {
        let anchor = request.anchor_position;
        let items = if let Some(media_target) = request.media_target {
            self.transcript_media_context_menu_items(media_target)
        } else {
            self.transcript_text_context_menu_items(request)
        };
        self.open_workbench_context_menu(
            WorkbenchContextMenuSource::Transcript,
            self.workbench_context_menu_anchor(anchor),
            WORKBENCH_CONTEXT_MENU_DEFAULT_WIDTH,
            items,
        );
        Task::none()
    }

    /// Builds text-selection actions for a rendered transcript context menu request.
    /// 为一条渲染转录右键菜单请求构建文本选择动作。
    pub(super) fn transcript_text_context_menu_items(
        &self,
        request: SelectableTranscriptContextMenuRequest,
    ) -> Vec<WorkbenchContextMenuItem> {
        let mut items = Vec::new();
        if let Some(selected_text) = request
            .selected_text
            .filter(|selected_text| !selected_text.is_empty())
        {
            items.push(WorkbenchContextMenuItem::enabled(
                "复制",
                Message::TranscriptContextMenuCopySelection(selected_text),
            ));
        }
        if request.has_document_text {
            items.push(WorkbenchContextMenuItem::enabled(
                "全选",
                Message::TranscriptContextMenuSelectAll(request.surface_id),
            ));
        } else {
            items.push(WorkbenchContextMenuItem::disabled("全选"));
        }
        items
    }

    /// Builds generated-media actions for a rendered transcript context menu target.
    /// 为一个渲染转录右键菜单媒体目标构建生成媒体动作。
    pub(super) fn transcript_media_context_menu_items(
        &self,
        media_target: crate::selectable_transcript::SelectableTranscriptContextMenuMediaTarget,
    ) -> Vec<WorkbenchContextMenuItem> {
        match media_target.kind {
            SelectableTranscriptContextMenuMediaKind::Image => vec![
                WorkbenchContextMenuItem::enabled(
                    "预览图片",
                    Message::OpenComposerImageAttachmentPreview {
                        file_name: media_target.file_name.clone(),
                        local_path: media_target.local_path.clone(),
                    },
                ),
                WorkbenchContextMenuItem::enabled(
                    "复制图片",
                    Message::CopyGeneratedImageToClipboard {
                        file_name: media_target.file_name,
                        local_path: media_target.local_path.clone(),
                    },
                ),
                WorkbenchContextMenuItem::enabled(
                    "复制路径",
                    Message::CopyGeneratedAttachmentPath(media_target.local_path.clone()),
                ),
                WorkbenchContextMenuItem::enabled(
                    "定位文件",
                    Message::RevealGeneratedAttachmentInFolder(media_target.local_path),
                ),
            ],
            SelectableTranscriptContextMenuMediaKind::Audio => vec![
                WorkbenchContextMenuItem::enabled(
                    if media_target.is_active {
                        "停止播放"
                    } else {
                        "播放音频"
                    },
                    Message::ToggleGeneratedAudioPlayback {
                        file_name: media_target.file_name,
                        local_path: media_target.local_path.clone(),
                    },
                ),
                WorkbenchContextMenuItem::enabled(
                    "复制路径",
                    Message::CopyGeneratedAttachmentPath(media_target.local_path.clone()),
                ),
                WorkbenchContextMenuItem::enabled(
                    "定位文件",
                    Message::RevealGeneratedAttachmentInFolder(media_target.local_path),
                ),
            ],
            SelectableTranscriptContextMenuMediaKind::Video
            | SelectableTranscriptContextMenuMediaKind::Pdf => vec![
                WorkbenchContextMenuItem::enabled(
                    "打开媒体",
                    Message::OpenGeneratedAttachmentWithDefaultApp(media_target.local_path.clone()),
                ),
                WorkbenchContextMenuItem::enabled(
                    "复制路径",
                    Message::CopyGeneratedAttachmentPath(media_target.local_path.clone()),
                ),
                WorkbenchContextMenuItem::enabled(
                    "定位文件",
                    Message::RevealGeneratedAttachmentInFolder(media_target.local_path),
                ),
            ],
        }
    }
}
