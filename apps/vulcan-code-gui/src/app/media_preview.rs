//! Generated media interaction and image preview helpers extracted from the main workbench module.
//! 从主工作台模块中抽取的生成媒体交互与图片预览辅助实现。

use super::*;

impl VulcanWorkbench {
    /// Opens one staged image attachment in the blocking preview modal after verifying the managed file still exists.
    /// 在确认受管文件仍然存在后，将某个已暂存图片附件打开到阻塞式预览弹窗中。
    pub(super) fn open_composer_image_attachment_preview(
        &mut self,
        file_name: String,
        local_path: String,
    ) -> Task<Message> {
        if !Path::new(&local_path).is_file() {
            let notice = format!("图片附件 `{file_name}` 的本地缓存文件已不存在，无法打开预览。");
            self.show_workbench_notice_modal("图片附件不可预览", notice.clone());
            self.status_message = notice;
            return Task::none();
        }

        self.workbench_image_preview_modal = Some(WorkbenchImagePreviewModalState {
            file_name,
            local_path,
            zoom: WORKBENCH_IMAGE_PREVIEW_DEFAULT_ZOOM,
            pan_pointer: None,
            preview_pointer: None,
            scroll_offset: scrollable::AbsoluteOffset::default(),
        });
        Task::none()
    }

    /// Opens one generated media file through the host operating system default application and updates the user-visible status banner.
    /// 通过宿主操作系统默认应用打开某个生成媒体文件，并更新面向用户的状态横幅。
    pub(super) fn open_generated_attachment_with_system_default_application(
        &mut self,
        local_path: &str,
    ) -> Task<Message> {
        match open_path_with_system_default_application(local_path) {
            Ok(()) => {
                self.status_message = String::from("已使用系统默认应用打开生成媒体。");
            }
            Err(error) => {
                self.show_workbench_notice_modal("无法打开生成媒体", error.clone());
                self.status_message = format!("打开生成媒体失败：{error}");
            }
        }
        Task::none()
    }

    /// Toggles host-side playback for one generated audio file while ensuring at most one generated clip plays at a time.
    /// 为某个生成音频文件切换宿主侧播放状态，并确保任意时刻最多只播放一条生成音频。
    pub(super) fn toggle_generated_audio_playback(
        &mut self,
        file_name: String,
        local_path: String,
    ) -> Task<Message> {
        if let Some(playback) = self.generated_audio_playback.as_ref() {
            if playback.local_path == local_path && !playback.sink.empty() {
                self.stop_generated_audio_playback();
                self.status_message = format!("已停止播放 `{file_name}`。");
                if let Some(session_id) = self.selected_session().map(|session| session.id) {
                    self.refresh_session_agent_view_buffers(session_id);
                }
                return Task::none();
            }
        }

        if !Path::new(&local_path).is_file() {
            let notice = format!("音频文件 `{file_name}` 的本地缓存已不存在，无法播放。");
            self.show_workbench_notice_modal("音频无法播放", notice.clone());
            self.status_message = notice;
            return Task::none();
        }

        self.stop_generated_audio_playback();
        match Self::build_generated_audio_playback_state(&local_path) {
            Ok(playback) => {
                self.generated_audio_playback = Some(playback);
                self.status_message = format!("正在播放 `{file_name}`。");
            }
            Err(error) => {
                self.show_workbench_notice_modal("音频无法播放", error.clone());
                self.status_message = format!("播放音频失败：{error}");
            }
        }
        if let Some(session_id) = self.selected_session().map(|session| session.id) {
            self.refresh_session_agent_view_buffers(session_id);
        }
        Task::none()
    }

    /// Copies one generated image into the system clipboard as bitmap data after validating the managed file still exists.
    /// 在确认受管文件仍然存在后，将某个生成图片以位图数据形式复制到系统剪贴板。
    pub(super) fn copy_generated_image_to_clipboard(
        &mut self,
        file_name: String,
        local_path: String,
    ) -> Task<Message> {
        if !Path::new(&local_path).is_file() {
            let notice = format!("图片文件 `{file_name}` 的本地缓存已不存在，无法复制。");
            self.show_workbench_notice_modal("图片无法复制", notice.clone());
            self.status_message = notice;
            return Task::none();
        }

        match copy_image_file_to_system_clipboard(&local_path) {
            Ok(()) => {
                self.status_message = format!("已复制图片 `{file_name}`。");
            }
            Err(error) => {
                self.show_workbench_notice_modal("图片无法复制", error.clone());
                self.status_message = format!("复制图片失败：{error}");
            }
        }
        Task::none()
    }

    /// Stops the active generated-audio playback sink, if any, and clears the retained playback state.
    /// 停止当前激活的生成音频播放 sink（如果存在），并清空保留的播放状态。
    pub(super) fn stop_generated_audio_playback(&mut self) {
        if let Some(playback) = self.generated_audio_playback.take() {
            playback.sink.stop();
        }
        if let Some(session_id) = self.selected_session().map(|session| session.id) {
            self.refresh_session_agent_view_buffers(session_id);
        }
    }

    /// Builds one host-side generated-audio playback state by opening, decoding, and enqueueing the target audio file.
    /// 通过打开、解码并入队目标音频文件来构建一份宿主侧生成音频播放状态。
    pub(super) fn build_generated_audio_playback_state(
        local_path: &str,
    ) -> Result<GeneratedAudioPlaybackState, String> {
        let file = fs::File::open(local_path)
            .map_err(|error| format!("无法打开音频文件 `{local_path}`：{error}"))?;
        let reader = BufReader::new(file);
        let source = Decoder::new(reader)
            .map_err(|error| format!("无法解码音频文件 `{local_path}`：{error}"))?;
        let (stream, stream_handle) = OutputStream::try_default()
            .map_err(|error| format!("无法初始化音频输出设备：{error}"))?;
        let sink = Sink::try_new(&stream_handle)
            .map_err(|error| format!("无法创建音频播放通道：{error}"))?;
        sink.append(source);
        sink.play();

        Ok(GeneratedAudioPlaybackState {
            local_path: local_path.to_string(),
            _stream: stream,
            sink,
        })
    }

    /// Computes the modal card size used by the current image preview overlay from the live main-window size.
    /// 根据实时主窗口尺寸计算当前图片预览遮罩使用的弹窗卡片尺寸。
    pub(super) fn workbench_image_preview_modal_size(&self) -> iced::Size {
        iced::Size::new(
            (self.main_window_size.width * WORKBENCH_IMAGE_PREVIEW_MODAL_RATIO)
                .max(WORKBENCH_IMAGE_PREVIEW_MODAL_MIN_WIDTH),
            (self.main_window_size.height * WORKBENCH_IMAGE_PREVIEW_MODAL_RATIO)
                .max(WORKBENCH_IMAGE_PREVIEW_MODAL_MIN_HEIGHT),
        )
    }

    /// Computes the visible image viewport size inside the current image preview modal.
    /// 计算当前图片预览弹窗内部可见图片视口尺寸。
    pub(super) fn workbench_image_preview_viewport_size(&self) -> iced::Size {
        let modal_size = self.workbench_image_preview_modal_size();
        iced::Size::new(
            (modal_size.width - WORKBENCH_IMAGE_PREVIEW_CARD_PADDING_X * 2.0).max(160.0),
            (modal_size.height
                - WORKBENCH_IMAGE_PREVIEW_CARD_PADDING_Y * 2.0
                - WORKBENCH_IMAGE_PREVIEW_HEADER_HEIGHT
                - WORKBENCH_IMAGE_PREVIEW_BODY_SPACING)
                .max(120.0),
        )
    }

    /// Adjusts the currently open image preview zoom while preserving the hovered image point when possible.
    /// 调整当前已打开图片预览的缩放倍率，并尽量保持鼠标悬停的图片点不漂移。
    pub(super) fn adjust_workbench_image_preview_zoom(&mut self, delta: f32) -> Task<Message> {
        self.zoom_workbench_image_preview(delta, None)
    }

    /// Handles one intercepted wheel movement, using Ctrl for pointer-centered zoom and plain wheel for preview scrolling.
    /// 处理一次被拦截的滚轮移动，按住 Ctrl 时围绕指针缩放，普通滚轮则滚动预览区域。
    pub(super) fn handle_workbench_image_preview_wheel(
        &mut self,
        delta: iced::mouse::ScrollDelta,
    ) -> Task<Message> {
        if self.workbench_image_preview_modal.is_none() {
            return Task::none();
        }

        if !self.workbench_keyboard_modifiers.control() {
            let scroll_offset = Self::workbench_image_preview_wheel_scroll_offset(delta);
            if scroll_offset.x.abs() <= f32::EPSILON && scroll_offset.y.abs() <= f32::EPSILON {
                return Task::none();
            }
            let viewport_size = self.workbench_image_preview_viewport_size();
            let Some(modal) = self.workbench_image_preview_modal.as_mut() else {
                return Task::none();
            };
            let next_offset = Self::clamp_workbench_image_preview_scroll_offset(
                viewport_size,
                modal.zoom,
                scrollable::AbsoluteOffset {
                    x: modal.scroll_offset.x + scroll_offset.x,
                    y: modal.scroll_offset.y + scroll_offset.y,
                },
            );
            modal.scroll_offset = next_offset;
            return operation::scroll_to(workbench_image_preview_scrollable_id(), next_offset);
        }

        let raw_delta = match delta {
            iced::mouse::ScrollDelta::Lines { y, .. } => y,
            iced::mouse::ScrollDelta::Pixels { y, .. } => y.signum(),
        };
        if raw_delta.abs() <= f32::EPSILON {
            return Task::none();
        }

        self.zoom_workbench_image_preview(
            raw_delta.signum() * WORKBENCH_IMAGE_PREVIEW_WHEEL_ZOOM_STEP,
            None,
        )
    }

    /// Converts one wheel delta into the same absolute preview-scroll offset used by iced scrollables.
    /// 将一次滚轮增量转换为与 iced 滚动容器一致的绝对预览滚动偏移。
    pub(super) fn workbench_image_preview_wheel_scroll_offset(
        delta: iced::mouse::ScrollDelta,
    ) -> scrollable::AbsoluteOffset {
        match delta {
            iced::mouse::ScrollDelta::Lines { x, y } => scrollable::AbsoluteOffset {
                x: -x * WORKBENCH_IMAGE_PREVIEW_WHEEL_SCROLL_LINE_STEP,
                y: -y * WORKBENCH_IMAGE_PREVIEW_WHEEL_SCROLL_LINE_STEP,
            },
            iced::mouse::ScrollDelta::Pixels { x, y } => {
                scrollable::AbsoluteOffset { x: -x, y: -y }
            }
        }
    }

    /// Clamps one requested preview scroll offset to the currently reachable zoomed image content bounds.
    /// 将请求的预览滚动偏移限制到当前缩放图片内容可达的边界内。
    pub(super) fn clamp_workbench_image_preview_scroll_offset(
        viewport_size: iced::Size,
        zoom: f32,
        offset: scrollable::AbsoluteOffset,
    ) -> scrollable::AbsoluteOffset {
        let max_x = (viewport_size.width * zoom.max(WORKBENCH_IMAGE_PREVIEW_DEFAULT_ZOOM)
            - viewport_size.width)
            .max(0.0);
        let max_y = (viewport_size.height * zoom.max(WORKBENCH_IMAGE_PREVIEW_DEFAULT_ZOOM)
            - viewport_size.height)
            .max(0.0);

        scrollable::AbsoluteOffset {
            x: offset.x.clamp(0.0, max_x),
            y: offset.y.clamp(0.0, max_y),
        }
    }

    /// Starts left-button panning for the image preview from the current window cursor position.
    /// 从当前窗口光标位置开始图片预览左键平移。
    pub(super) fn begin_workbench_image_preview_pan(&mut self) -> Task<Message> {
        if let Some(modal) = self.workbench_image_preview_modal.as_mut() {
            modal.pan_pointer = self.main_window_cursor_position;
        }
        Task::none()
    }

    /// Ends any active left-button image preview panning gesture.
    /// 结束当前激活的任意左键图片预览平移手势。
    pub(super) fn end_workbench_image_preview_pan(&mut self) -> Task<Message> {
        if let Some(modal) = self.workbench_image_preview_modal.as_mut() {
            modal.pan_pointer = None;
        }
        Task::none()
    }

    /// Applies one cursor movement as a left-button image preview pan operation.
    /// 将一次光标移动应用为左键图片预览平移操作。
    pub(super) fn apply_workbench_image_preview_pan(
        &mut self,
        position: iced::Point,
    ) -> Task<Message> {
        let viewport_size = self.workbench_image_preview_viewport_size();
        let Some(modal) = self.workbench_image_preview_modal.as_mut() else {
            return Task::none();
        };
        let Some(previous_position) = modal.pan_pointer.replace(position) else {
            return Task::none();
        };
        let pan_delta_x = previous_position.x - position.x;
        let pan_delta_y = previous_position.y - position.y;
        if pan_delta_x.abs() <= WORKBENCH_IMAGE_PREVIEW_PAN_EPSILON
            && pan_delta_y.abs() <= WORKBENCH_IMAGE_PREVIEW_PAN_EPSILON
        {
            return Task::none();
        }

        let next_offset = Self::clamp_workbench_image_preview_scroll_offset(
            viewport_size,
            modal.zoom,
            scrollable::AbsoluteOffset {
                x: modal.scroll_offset.x + pan_delta_x,
                y: modal.scroll_offset.y + pan_delta_y,
            },
        );
        modal.scroll_offset = next_offset;
        operation::scroll_to(workbench_image_preview_scrollable_id(), next_offset)
    }

    /// Applies one zoom delta and scrolls the preview so the selected local anchor stays under the cursor.
    /// 应用一次缩放增量，并滚动预览以尽量让选定的局部锚点停留在鼠标下方。
    pub(super) fn zoom_workbench_image_preview(
        &mut self,
        delta: f32,
        anchor_override: Option<iced::Point>,
    ) -> Task<Message> {
        let viewport_size = self.workbench_image_preview_viewport_size();
        let Some(modal) = self.workbench_image_preview_modal.as_mut() else {
            return Task::none();
        };
        let previous_zoom = modal.zoom;
        let next_zoom = (previous_zoom + delta).clamp(
            WORKBENCH_IMAGE_PREVIEW_MIN_ZOOM,
            WORKBENCH_IMAGE_PREVIEW_MAX_ZOOM,
        );
        if (next_zoom - previous_zoom).abs() <= f32::EPSILON {
            return Task::none();
        }

        let anchor = anchor_override
            .or(modal.preview_pointer)
            .unwrap_or_else(|| {
                iced::Point::new(viewport_size.width / 2.0, viewport_size.height / 2.0)
            });
        let zoom_ratio = next_zoom / previous_zoom.max(f32::EPSILON);
        let next_offset = Self::clamp_workbench_image_preview_scroll_offset(
            viewport_size,
            next_zoom,
            scrollable::AbsoluteOffset {
                x: (modal.scroll_offset.x + anchor.x) * zoom_ratio - anchor.x,
                y: (modal.scroll_offset.y + anchor.y) * zoom_ratio - anchor.y,
            },
        );

        modal.zoom = next_zoom;
        modal.scroll_offset = next_offset;
        operation::scroll_to(workbench_image_preview_scrollable_id(), next_offset)
    }

    /// Resets the currently open image preview zoom to the default fit value.
    /// 将当前已打开图片预览的缩放倍率还原到默认适配值。
    pub(super) fn reset_workbench_image_preview_zoom(&mut self) -> Task<Message> {
        if let Some(modal) = self.workbench_image_preview_modal.as_mut() {
            modal.zoom = WORKBENCH_IMAGE_PREVIEW_DEFAULT_ZOOM;
            modal.scroll_offset = scrollable::AbsoluteOffset::default();
        }
        operation::scroll_to(
            workbench_image_preview_scrollable_id(),
            scrollable::AbsoluteOffset { x: 0.0, y: 0.0 },
        )
    }
}
