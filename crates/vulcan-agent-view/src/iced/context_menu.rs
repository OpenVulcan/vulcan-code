//! Lightweight transcript context-menu and shortcut helpers kept separate from the core selectable-surface shell.
//! 与核心可选表面外壳分离的轻量转录右键菜单与快捷键辅助逻辑。

use super::interaction::current_selection_for_state;
use super::{
    RenderedTranscriptDocument, SelectableTranscriptInteraction, SelectableTranscriptState,
    TranscriptContextMenuAction, TranscriptContextMenuState, TranscriptMediaTarget,
};
use crate::iced_theme as theme;
use iced::advanced::layout::{self, Layout};
use iced::advanced::mouse;
use iced::advanced::overlay;
use iced::advanced::renderer;
use iced::advanced::text;
use iced::advanced::text::Renderer as _;
use iced::advanced::{Renderer as _, Shell};
use iced::{
    Background, Border, Color, Event, Pixels, Point, Rectangle, Renderer, Size, Theme,
    keyboard,
};

/// Fixed width used by the lightweight transcript context menu.
/// 转录轻量右键菜单使用的固定宽度。
const TRANSCRIPT_CONTEXT_MENU_WIDTH: f32 = 200.0;

/// Fixed row height used by each transcript context-menu item.
/// 每条转录右键菜单项使用的固定行高。
pub(super) const TRANSCRIPT_CONTEXT_MENU_ITEM_HEIGHT: f32 = 32.0;

/// Outer padding applied around the transcript context-menu text labels.
/// 应用于转录右键菜单文本标签周围的外边距。
pub(super) const TRANSCRIPT_CONTEXT_MENU_ITEM_PADDING_X: f32 = 12.0;

/// Corner radius used by the lightweight transcript context menu.
/// 轻量转录右键菜单使用的圆角半径。
pub(super) const TRANSCRIPT_CONTEXT_MENU_RADIUS: f32 = 2.0;

/// Returns the lightweight transcript context-menu actions visible under the current selection state or hovered media card.
/// 根据当前选区状态或悬停媒体卡片返回轻量转录右键菜单可见的动作集合。
pub(super) fn transcript_context_menu_actions(
    media_target: Option<&TranscriptMediaTarget>,
    has_selection: bool,
) -> Vec<TranscriptContextMenuAction> {
    if let Some(media_target) = media_target {
        return match media_target.kind {
            super::scene::RenderedMediaKind::Image => vec![
                TranscriptContextMenuAction::PreviewImage,
                TranscriptContextMenuAction::CopyImage,
                TranscriptContextMenuAction::CopyMediaPath,
                TranscriptContextMenuAction::RevealMedia,
            ],
            super::scene::RenderedMediaKind::Audio => vec![
                TranscriptContextMenuAction::ToggleAudioPlayback,
                TranscriptContextMenuAction::CopyMediaPath,
                TranscriptContextMenuAction::RevealMedia,
            ],
            super::scene::RenderedMediaKind::Video | super::scene::RenderedMediaKind::Pdf => vec![
                TranscriptContextMenuAction::OpenMedia,
                TranscriptContextMenuAction::CopyMediaPath,
                TranscriptContextMenuAction::RevealMedia,
            ],
        };
    }
    if has_selection {
        vec![
            TranscriptContextMenuAction::CopySelection,
            TranscriptContextMenuAction::SelectAll,
        ]
    } else {
        vec![TranscriptContextMenuAction::SelectAll]
    }
}

/// Returns the localized label rendered for one transcript context-menu action.
/// 返回为某个转录右键菜单动作渲染的本地化标签。
pub(super) fn transcript_context_menu_action_label(
    action: TranscriptContextMenuAction,
    media_target: Option<&TranscriptMediaTarget>,
) -> &'static str {
    match action {
        TranscriptContextMenuAction::CopySelection => "复制",
        TranscriptContextMenuAction::SelectAll => "全选",
        TranscriptContextMenuAction::PreviewImage => "预览图片",
        TranscriptContextMenuAction::ToggleAudioPlayback => {
            if media_target.is_some_and(|target| target.is_active) {
                "停止播放"
            } else {
                "播放音频"
            }
        }
        TranscriptContextMenuAction::OpenMedia => "打开媒体",
        TranscriptContextMenuAction::CopyMediaPath => "复制路径",
        TranscriptContextMenuAction::RevealMedia => "定位文件",
        TranscriptContextMenuAction::CopyImage => "复制图片",
    }
}

/// Computes the clamped transcript context-menu bounds for the provided anchor point and item count.
/// 根据给定锚点与菜单项数量计算经过钳制的转录右键菜单边界。
pub(super) fn transcript_context_menu_bounds(
    widget_bounds: Rectangle,
    menu_state: TranscriptContextMenuState,
    item_count: usize,
) -> Rectangle {
    let width = TRANSCRIPT_CONTEXT_MENU_WIDTH.min(widget_bounds.width.max(1.0));
    let height = (TRANSCRIPT_CONTEXT_MENU_ITEM_HEIGHT * item_count as f32).max(1.0);
    let max_x = (widget_bounds.x + widget_bounds.width - width).max(widget_bounds.x);
    let max_y = (widget_bounds.y + widget_bounds.height - height).max(widget_bounds.y);

    Rectangle::new(
        Point::new(
            menu_state.anchor_position.x.clamp(widget_bounds.x, max_x),
            menu_state.anchor_position.y.clamp(widget_bounds.y, max_y),
        ),
        Size::new(width, height),
    )
}

/// Returns the row bounds occupied by the indexed transcript context-menu item.
/// 返回给定索引的转录右键菜单项所占据的行边界。
pub(super) fn transcript_context_menu_item_bounds(
    menu_bounds: Rectangle,
    index: usize,
) -> Rectangle {
    Rectangle::new(
        Point::new(
            menu_bounds.x,
            menu_bounds.y + TRANSCRIPT_CONTEXT_MENU_ITEM_HEIGHT * index as f32,
        ),
        Size::new(menu_bounds.width, TRANSCRIPT_CONTEXT_MENU_ITEM_HEIGHT),
    )
}

/// Hit-tests the transcript context menu and returns the clicked action, if any.
/// 对转录右键菜单执行命中测试，并在命中时返回对应动作。
pub(super) fn hit_test_transcript_context_menu_action(
    menu_bounds: Rectangle,
    actions: &[TranscriptContextMenuAction],
    position: Point,
) -> Option<TranscriptContextMenuAction> {
    if !menu_bounds.contains(position) {
        return None;
    }

    actions.iter().enumerate().find_map(|(index, action)| {
        transcript_context_menu_item_bounds(menu_bounds, index)
            .contains(position)
            .then_some(*action)
    })
}

/// Executes one transcript context-menu action against the current selection state and clipboard.
/// 针对当前选区状态与剪贴板执行一次转录右键菜单动作。
pub(super) fn execute_transcript_context_menu_action<Message>(
    action: TranscriptContextMenuAction,
    state: &mut SelectableTranscriptState,
    document: &RenderedTranscriptDocument,
    on_interaction: &dyn Fn(SelectableTranscriptInteraction) -> Message,
    shell: &mut Shell<'_, Message>,
) {
    match action {
        TranscriptContextMenuAction::CopySelection => {
            if let Some((selection_start, selection_end)) = current_selection_for_state(state) {
                shell.write_clipboard(
                    document
                        .selected_text(selection_start, selection_end)
                        .into(),
                );
            }
        }
        TranscriptContextMenuAction::SelectAll => {
            state.anchor_char = 0;
            state.head_char = document.len_chars();
        }
        TranscriptContextMenuAction::PreviewImage => {
            if let Some(media_target) = state
                .context_menu
                .as_ref()
                .and_then(|menu_state| menu_state.media_target.as_ref())
            {
                shell.publish(on_interaction(
                    SelectableTranscriptInteraction::OpenImagePreview {
                        file_name: media_target.file_name.clone(),
                        local_path: media_target.local_path.clone(),
                    },
                ));
            }
        }
        TranscriptContextMenuAction::ToggleAudioPlayback => {
            if let Some(media_target) = state
                .context_menu
                .as_ref()
                .and_then(|menu_state| menu_state.media_target.as_ref())
            {
                shell.publish(on_interaction(
                    SelectableTranscriptInteraction::ToggleAudioPlayback {
                        file_name: media_target.file_name.clone(),
                        local_path: media_target.local_path.clone(),
                    },
                ));
            }
        }
        TranscriptContextMenuAction::OpenMedia => {
            if let Some(media_target) = state
                .context_menu
                .as_ref()
                .and_then(|menu_state| menu_state.media_target.as_ref())
            {
                shell.publish(on_interaction(
                    SelectableTranscriptInteraction::OpenMediaWithDefaultApp(
                        media_target.local_path.clone(),
                    ),
                ));
            }
        }
        TranscriptContextMenuAction::CopyMediaPath => {
            if let Some(media_target) = state
                .context_menu
                .as_ref()
                .and_then(|menu_state| menu_state.media_target.as_ref())
            {
                shell.publish(on_interaction(
                    SelectableTranscriptInteraction::CopyMediaPath(media_target.local_path.clone()),
                ));
            }
        }
        TranscriptContextMenuAction::RevealMedia => {
            if let Some(media_target) = state
                .context_menu
                .as_ref()
                .and_then(|menu_state| menu_state.media_target.as_ref())
            {
                shell.publish(on_interaction(
                    SelectableTranscriptInteraction::RevealMediaInFolder(
                        media_target.local_path.clone(),
                    ),
                ));
            }
        }
        TranscriptContextMenuAction::CopyImage => {
            if let Some(media_target) = state
                .context_menu
                .as_ref()
                .and_then(|menu_state| menu_state.media_target.as_ref())
            {
                shell.publish(on_interaction(
                    SelectableTranscriptInteraction::CopyImageToClipboard {
                        file_name: media_target.file_name.clone(),
                        local_path: media_target.local_path.clone(),
                    },
                ));
            }
        }
    }
}

/// Returns whether the provided key event matches the copy shortcut on the current platform.
/// 返回给定按键事件是否匹配当前平台上的复制快捷键。
pub(super) fn shortcut_matches_copy(
    key: &keyboard::Key,
    modifiers: keyboard::Modifiers,
    text: Option<&str>,
) -> bool {
    modifiers.command()
        && !modifiers.alt()
        && matches!(
            key,
            keyboard::Key::Character(character) if character.eq_ignore_ascii_case("c")
        )
        || modifiers.command()
            && !modifiers.alt()
            && text.is_some_and(|value| value.eq_ignore_ascii_case("c"))
}

/// Returns whether the provided key event matches the select-all shortcut on the current platform.
/// 返回给定按键事件是否匹配当前平台上的全选快捷键。
pub(super) fn shortcut_matches_select_all(
    key: &keyboard::Key,
    modifiers: keyboard::Modifiers,
    text: Option<&str>,
) -> bool {
    modifiers.command()
        && !modifiers.alt()
        && matches!(
            key,
            keyboard::Key::Character(character) if character.eq_ignore_ascii_case("a")
        )
        || modifiers.command()
            && !modifiers.alt()
            && text.is_some_and(|value| value.eq_ignore_ascii_case("a"))
}

/// Builds one floating context-menu overlay element anchored to the transcript widget.
/// 构建一个锚定到转录组件的浮动右键菜单覆盖层元素。
pub(super) fn build_context_menu_overlay<'a, Message>(
    state: &'a mut SelectableTranscriptState,
    document: &'a RenderedTranscriptDocument,
    widget_bounds: Rectangle,
    menu_state: TranscriptContextMenuState,
    on_interaction: &'a dyn Fn(SelectableTranscriptInteraction) -> Message,
) -> overlay::Element<'a, Message, Theme, Renderer> {
    overlay::Element::new(Box::new(SelectableTranscriptContextMenuOverlay {
        state,
        document,
        widget_bounds,
        menu_state,
        on_interaction,
    }))
}

/// Overlay responsible for presenting and executing the selectable transcript context menu.
/// 负责展示并执行可选择转录右键菜单的浮层。
struct SelectableTranscriptContextMenuOverlay<'a, Message> {
    /// Mutable transcript runtime state so menu actions can update selection and dismiss themselves.
    /// 可变转录运行时状态，用于让菜单动作更新选区并自行关闭。
    state: &'a mut SelectableTranscriptState,
    /// Rendered transcript document used to copy selected text and compute the full character span.
    /// 用于复制选中文本并计算全文字符跨度的渲染转录文档。
    document: &'a RenderedTranscriptDocument,
    /// Absolute widget bounds used to clamp the menu overlay inside the transcript surface.
    /// 用于将菜单浮层钳制在转录表面内部的绝对组件边界。
    widget_bounds: Rectangle,
    /// Open menu anchor captured from the right-click gesture.
    /// 从右键手势捕获的已打开菜单锚点。
    menu_state: TranscriptContextMenuState,
    /// High-level transcript interaction dispatcher reused by media-card context-menu actions.
    /// 供媒体卡片右键菜单动作复用的高层转录交互分发器。
    on_interaction: &'a dyn Fn(SelectableTranscriptInteraction) -> Message,
}

impl<Message> overlay::Overlay<Message, Theme, Renderer>
    for SelectableTranscriptContextMenuOverlay<'_, Message>
{
    /// Lays out the transcript context menu using one clamped fixed-size floating rectangle.
    /// 使用一个经过钳制的固定尺寸浮动矩形对转录右键菜单进行布局。
    fn layout(&mut self, _renderer: &Renderer, bounds: Size) -> layout::Node {
        layout::Node::new(Size::new(bounds.width.max(1.0), bounds.height.max(1.0)))
    }

    /// Draws the floating transcript context menu above the transcript surface.
    /// 在转录表面之上绘制浮动转录右键菜单。
    fn draw(
        &self,
        renderer: &mut Renderer,
        theme_palette_owner: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
    ) {
        let actions = transcript_context_menu_actions(
            self.menu_state.media_target.as_ref(),
            current_selection_for_state(self.state).is_some(),
        );
        let menu_bounds = transcript_context_menu_bounds(
            self.widget_bounds,
            self.menu_state.clone(),
            actions.len(),
        );
        let actions = transcript_context_menu_actions(
            self.menu_state.media_target.as_ref(),
            current_selection_for_state(self.state).is_some(),
        );
        let palette = theme_palette_owner.palette();
        let hovered_action = cursor.position().and_then(|position| {
            hit_test_transcript_context_menu_action(menu_bounds, actions.as_slice(), position)
        });

        renderer.with_layer(layout.bounds(), |renderer| {
            let local_menu_bounds = Rectangle::new(
                Point::new(
                    menu_bounds.x - layout.bounds().x,
                    menu_bounds.y - layout.bounds().y,
                ),
                menu_bounds.size(),
            );

            renderer.fill_quad(
                renderer::Quad {
                    bounds: local_menu_bounds,
                    border: Border {
                        color: palette.background.strong.color.scale_alpha(0.22),
                        width: 1.0,
                        radius: TRANSCRIPT_CONTEXT_MENU_RADIUS.into(),
                    },
                    shadow: iced::Shadow {
                        color: Color::BLACK.scale_alpha(0.12),
                        offset: iced::Vector::new(0.0, 4.0),
                        blur_radius: 12.0,
                    },
                    ..renderer::Quad::default()
                },
                Background::Color(palette.background.base.color),
            );

            for (index, action) in actions.iter().enumerate() {
                let item_bounds = transcript_context_menu_item_bounds(local_menu_bounds, index);
                if hovered_action == Some(*action) {
                    renderer.fill_quad(
                        renderer::Quad {
                            bounds: item_bounds,
                            border: Border {
                                color: Color::TRANSPARENT,
                                width: 0.0,
                                radius: TRANSCRIPT_CONTEXT_MENU_RADIUS.into(),
                            },
                            ..renderer::Quad::default()
                        },
                        Background::Color(palette.background.strong.color.scale_alpha(0.12)),
                    );
                }

                renderer.fill_text(
                    text::Text {
                        content: transcript_context_menu_action_label(
                            *action,
                            self.menu_state.media_target.as_ref(),
                        )
                        .to_string(),
                        bounds: Size::new(
                            (item_bounds.width - TRANSCRIPT_CONTEXT_MENU_ITEM_PADDING_X * 2.0)
                                .max(1.0),
                            item_bounds.height.max(1.0),
                        ),
                        size: Pixels(14.0),
                        line_height: text::LineHeight::default(),
                        font: theme::ui_font(),
                        align_x: text::Alignment::Left,
                        align_y: iced::alignment::Vertical::Center,
                        shaping: text::Shaping::Advanced,
                        wrapping: text::Wrapping::None,
                        ellipsis: text::Ellipsis::None,
                        hint_factor: None,
                    },
                    Point::new(
                        item_bounds.x + TRANSCRIPT_CONTEXT_MENU_ITEM_PADDING_X,
                        item_bounds.y,
                    ),
                    palette.background.base.text.scale_alpha(0.90),
                    layout.bounds(),
                );
            }
        });
    }

    /// Updates the floating transcript context menu, executing menu actions or dismissing itself on outside clicks.
    /// 更新浮动转录右键菜单，在点击菜单动作或外部区域时执行动作或自行关闭。
    fn update(
        &mut self,
        event: &Event,
        _layout: Layout<'_>,
        cursor: mouse::Cursor,
        _renderer: &Renderer,
        shell: &mut Shell<'_, Message>,
    ) {
        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                let actions = transcript_context_menu_actions(
                    self.menu_state.media_target.as_ref(),
                    current_selection_for_state(self.state).is_some(),
                );
                let menu_bounds = transcript_context_menu_bounds(
                    self.widget_bounds,
                    self.menu_state.clone(),
                    actions.len(),
                );

                if let Some(position) = cursor.position() {
                    if let Some(action) = hit_test_transcript_context_menu_action(
                        menu_bounds,
                        actions.as_slice(),
                        position,
                    ) {
                        execute_transcript_context_menu_action(
                            action,
                            self.state,
                            self.document,
                            self.on_interaction,
                            shell,
                        );
                        self.state.context_menu = None;
                        shell.request_redraw();
                        shell.capture_event();
                        return;
                    }
                }

                self.state.context_menu = None;
                shell.request_redraw();
            }
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right))
            | Event::Keyboard(keyboard::Event::KeyPressed { .. })
            | Event::Mouse(mouse::Event::WheelScrolled { .. }) => {
                self.state.context_menu = None;
                shell.request_redraw();
            }
            _ => {}
        }
    }

    /// Returns pointer interaction while hovering the context-menu overlay.
    /// 返回悬停在右键菜单浮层上时的指针交互。
    fn mouse_interaction(
        &self,
        _layout: Layout<'_>,
        cursor: mouse::Cursor,
        _renderer: &Renderer,
    ) -> mouse::Interaction {
        let actions = transcript_context_menu_actions(
            self.menu_state.media_target.as_ref(),
            current_selection_for_state(self.state).is_some(),
        );
        let menu_bounds = transcript_context_menu_bounds(
            self.widget_bounds,
            self.menu_state.clone(),
            actions.len(),
        );
        cursor
            .position()
            .filter(|position| menu_bounds.contains(*position))
            .map(|_| mouse::Interaction::Pointer)
            .unwrap_or(mouse::Interaction::default())
    }
}

#[cfg(test)]
mod tests {
    use super::transcript_context_menu_action_label;
    use crate::iced::scene::RenderedMediaKind;
    use crate::iced::surface_state::{TranscriptContextMenuAction, TranscriptMediaTarget};

    /// Verifies the audio context-menu verb switches between play and stop according to the active transcript media target state.
    /// 验证音频右键菜单动词会根据当前转录媒体目标的激活状态在播放与停止之间切换。
    #[test]
    fn audio_context_menu_label_tracks_active_playback_state() {
        let stopped_target = TranscriptMediaTarget {
            kind: RenderedMediaKind::Audio,
            file_name: String::from("demo.mp3"),
            local_path: String::from("D:/demo.mp3"),
            is_active: false,
        };
        let active_target = TranscriptMediaTarget {
            is_active: true,
            ..stopped_target.clone()
        };

        assert_eq!(
            transcript_context_menu_action_label(
                TranscriptContextMenuAction::ToggleAudioPlayback,
                Some(&stopped_target),
            ),
            "播放音频"
        );
        assert_eq!(
            transcript_context_menu_action_label(
                TranscriptContextMenuAction::ToggleAudioPlayback,
                Some(&active_target),
            ),
            "停止播放"
        );
    }
}
