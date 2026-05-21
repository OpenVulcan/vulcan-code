//! Event-routing helpers that keep transcript input handling outside the main widget shell.
//! 将转录输入处理保留在主组件外壳之外的事件分发辅助逻辑。

use super::context_menu::{
    shortcut_matches_copy, shortcut_matches_select_all, transcript_context_menu_actions,
    transcript_context_menu_bounds,
};
use super::interaction::{
    background_action_link_at_position, current_selection_for_state,
    drag_auto_scroll_request_for_position, drag_selection_head_char, effective_selection,
    hit_test_rendered_document, media_target_at_position, scroll_collapsible_body_at_position,
};
use super::layout_engine::{localize_point, localize_rectangle};
use super::scene::RenderedMediaKind;
use super::{
    RenderedTranscriptDocument, RenderedTranscriptLinkAction,
    SelectableTranscriptContextMenuMediaKind, SelectableTranscriptContextMenuMediaTarget,
    SelectableTranscriptContextMenuRequest, SelectableTranscriptDragAutoScroll,
    SelectableTranscriptInteraction, SelectableTranscriptState, SelectableTranscriptSurfaceId,
    TranscriptContextMenuState, TranscriptHoverTooltipState, TranscriptMediaTarget,
};
use iced::advanced::Shell;
use iced::advanced::mouse;
use iced::{Event, Rectangle, keyboard};
use std::time::Instant;

/// Routes one runtime event through the selectable transcript interaction state machine without changing rendering behavior.
/// 在不改变渲染行为的前提下，将一个运行时事件分发到可选择转录交互状态机中。
pub(super) fn handle_transcript_widget_event<Message>(
    document: &RenderedTranscriptDocument,
    on_drag_auto_scroll: &dyn Fn(Option<SelectableTranscriptDragAutoScroll>) -> Message,
    on_interaction: &dyn Fn(SelectableTranscriptInteraction) -> Message,
    context_menu_surface_id: Option<&SelectableTranscriptSurfaceId>,
    state: &mut SelectableTranscriptState,
    event: &Event,
    widget_bounds: Rectangle,
    cursor: mouse::Cursor,
    shell: &mut Shell<'_, Message>,
    viewport: Rectangle,
) {
    let local_viewport = localize_rectangle(viewport, widget_bounds.position());

    match event {
        Event::Mouse(mouse::Event::WheelScrolled { delta }) => {
            state.context_menu = None;
            shell.request_redraw();
            if let Some(position) = cursor.position_over(widget_bounds) {
                let local_position = localize_point(position, widget_bounds.position());
                let delta_y = match delta {
                    mouse::ScrollDelta::Lines { y, .. } => -y * 36.0,
                    mouse::ScrollDelta::Pixels { y, .. } => -y,
                };

                if scroll_collapsible_body_at_position(document, state, local_position, delta_y) {
                    shell.capture_event();
                }
            }
        }
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
            state.hover_tooltip = None;
            handle_left_button_pressed(
                document,
                on_drag_auto_scroll,
                state,
                widget_bounds,
                cursor,
                shell,
            );
        }
        Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Right)) => {
            state.hover_tooltip = None;
            handle_right_button_pressed(
                document,
                on_interaction,
                context_menu_surface_id,
                state,
                widget_bounds,
                cursor,
                shell,
            );
        }
        Event::Mouse(mouse::Event::CursorMoved { .. }) if state.dragging => {
            state.hover_tooltip = None;
            handle_drag_cursor_moved(
                document,
                on_drag_auto_scroll,
                state,
                widget_bounds,
                local_viewport,
                cursor,
                shell,
            );
        }
        Event::Mouse(mouse::Event::CursorMoved { .. }) => {
            update_hover_tooltip_state(document, state, widget_bounds, cursor, shell);
        }
        Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) if state.dragging => {
            handle_left_button_released(
                document,
                on_drag_auto_scroll,
                on_interaction,
                state,
                widget_bounds,
                local_viewport,
                cursor,
                shell,
            );
        }
        Event::Keyboard(keyboard::Event::KeyPressed {
            key,
            modifiers,
            text,
            ..
        }) if state.focused && shortcut_matches_copy(key, *modifiers, text.as_deref()) => {
            state.context_menu = None;
            state.hover_tooltip = None;
            if let Some((selection_start, selection_end)) = effective_selection(
                state,
                document,
                local_viewport,
                cursor
                    .position_including_levitation()
                    .map(|position| localize_point(position, widget_bounds.position())),
            ) {
                shell.write_clipboard(
                    document
                        .selected_text(selection_start, selection_end)
                        .into(),
                );
                shell.request_redraw();
                shell.capture_event();
            }
        }
        Event::Keyboard(keyboard::Event::KeyPressed {
            key,
            modifiers,
            text,
            ..
        }) if state.focused && shortcut_matches_select_all(key, *modifiers, text.as_deref()) => {
            state.context_menu = None;
            state.hover_tooltip = None;
            state.anchor_char = 0;
            state.head_char = document.len_chars();
            shell.request_redraw();
            shell.capture_event();
        }
        Event::Keyboard(keyboard::Event::KeyPressed { .. }) => {
            state.context_menu = None;
            state.hover_tooltip = None;
            shell.request_redraw();
        }
        _ => {}
    }
}

/// Updates the lightweight hover tooltip state using the currently hovered transcript chip, requesting redraw only when the visible payload changes.
/// 基于当前悬停的转录标签更新轻量提示状态，仅在可见载荷发生变化时请求重绘。
fn update_hover_tooltip_state<Message>(
    document: &RenderedTranscriptDocument,
    state: &mut SelectableTranscriptState,
    widget_bounds: Rectangle,
    cursor: mouse::Cursor,
    shell: &mut Shell<'_, Message>,
) {
    let next_tooltip = cursor
        .position_over(widget_bounds)
        .and_then(|position| hovered_turn_stats_tooltip(document, state, widget_bounds, position));
    if state.hover_tooltip == next_tooltip {
        return;
    }
    state.hover_tooltip = next_tooltip;
    shell.request_redraw();
}

/// Resolves the hovered turn-statistics tooltip payload under the current pointer position, if any.
/// 解析当前指针位置下方命中的按轮次统计提示载荷（如果存在）。
fn hovered_turn_stats_tooltip(
    document: &RenderedTranscriptDocument,
    state: &SelectableTranscriptState,
    widget_bounds: Rectangle,
    position: iced::Point,
) -> Option<TranscriptHoverTooltipState> {
    let local_position = localize_point(position, widget_bounds.position());
    let link_target = background_action_link_at_position(document, state, local_position)?;
    match document.link_action_for_target(&link_target) {
        Some(RenderedTranscriptLinkAction::ShowTurnStatsTooltip { title, body }) => {
            Some(TranscriptHoverTooltipState {
                anchor_position: position,
                title: title.clone(),
                body: body.clone(),
            })
        }
        _ => None,
    }
}

/// Resolves the mouse interaction that should be shown for the current transcript pointer location.
/// 解析当前转录指针位置应显示的鼠标交互样式。
pub(super) fn transcript_mouse_interaction(
    document: &RenderedTranscriptDocument,
    state: &SelectableTranscriptState,
    widget_bounds: Rectangle,
    cursor: mouse::Cursor,
) -> mouse::Interaction {
    let Some(position) = cursor.position_over(widget_bounds) else {
        return mouse::Interaction::None;
    };
    let local_position = localize_point(position, widget_bounds.position());

    if let Some(menu_state) = state.context_menu.as_ref() {
        let menu_actions = transcript_context_menu_actions(
            menu_state.media_target.as_ref(),
            current_selection_for_state(state).is_some(),
        );
        let menu_bounds =
            transcript_context_menu_bounds(widget_bounds, menu_state.clone(), menu_actions.len());
        if menu_bounds.contains(position) {
            return mouse::Interaction::Pointer;
        }
    }

    if hit_test_rendered_document(document, state, local_position)
        .and_then(|hit_char| document.link_at_char_offset(hit_char))
        .or_else(|| background_action_link_at_position(document, state, local_position))
        .is_some()
    {
        mouse::Interaction::Pointer
    } else {
        mouse::Interaction::Text
    }
}

/// Handles a primary-button press by starting one transcript selection drag or clearing focus when the press misses the surface.
/// 处理一次主按钮按下事件，在命中表面时启动转录选区拖拽，在未命中时清除焦点。
fn handle_left_button_pressed<Message>(
    document: &RenderedTranscriptDocument,
    on_drag_auto_scroll: &dyn Fn(Option<SelectableTranscriptDragAutoScroll>) -> Message,
    state: &mut SelectableTranscriptState,
    widget_bounds: Rectangle,
    cursor: mouse::Cursor,
    shell: &mut Shell<'_, Message>,
) {
    state.context_menu = None;
    if let Some(position) = cursor.position_over(widget_bounds) {
        let local_position = localize_point(position, widget_bounds.position());
        if let Some(hit_char) = hit_test_rendered_document(document, state, local_position) {
            state.focused = true;
            state.dragging = true;
            state.drag_moved = false;
            state.anchor_char = hit_char;
            state.head_char = hit_char;
            shell.request_redraw();
            publish_drag_auto_scroll_request(state, None, on_drag_auto_scroll, shell);
            shell.capture_event();
        } else if background_action_link_at_position(document, state, local_position).is_some() {
            state.focused = true;
            state.dragging = true;
            state.drag_moved = false;
            state.anchor_char = 0;
            state.head_char = 0;
            shell.request_redraw();
            publish_drag_auto_scroll_request(state, None, on_drag_auto_scroll, shell);
            shell.capture_event();
        } else {
            state.focused = false;
            shell.request_redraw();
            publish_drag_auto_scroll_request(state, None, on_drag_auto_scroll, shell);
        }
    } else {
        state.focused = false;
        shell.request_redraw();
        publish_drag_auto_scroll_request(state, None, on_drag_auto_scroll, shell);
    }
}

/// Handles a secondary-button press by opening or dismissing the transcript context menu.
/// 处理一次次按钮按下事件，以打开或关闭转录右键菜单。
fn handle_right_button_pressed<Message>(
    document: &RenderedTranscriptDocument,
    on_interaction: &dyn Fn(SelectableTranscriptInteraction) -> Message,
    context_menu_surface_id: Option<&SelectableTranscriptSurfaceId>,
    state: &mut SelectableTranscriptState,
    widget_bounds: Rectangle,
    cursor: mouse::Cursor,
    shell: &mut Shell<'_, Message>,
) {
    if let Some(position) = cursor.position_over(widget_bounds) {
        state.focused = true;
        let local_position = localize_point(position, widget_bounds.position());
        let media_target = media_target_at_position(document, state, local_position);
        if let Some(surface_id) = context_menu_surface_id {
            let selected_text =
                current_selection_for_state(state).map(|(selection_start, selection_end)| {
                    document.selected_text(selection_start, selection_end)
                });
            shell.publish(on_interaction(
                SelectableTranscriptInteraction::OpenContextMenu(
                    SelectableTranscriptContextMenuRequest {
                        surface_id: surface_id.clone(),
                        anchor_position: position,
                        selected_text,
                        has_document_text: document.len_chars() > 0,
                        media_target: media_target.map(selectable_context_menu_media_target),
                    },
                ),
            ));
            state.context_menu = None;
            shell.request_redraw();
            shell.capture_event();
            return;
        }

        state.context_menu = Some(TranscriptContextMenuState {
            anchor_position: position,
            media_target,
        });
        shell.request_redraw();
        shell.capture_event();
    } else {
        state.context_menu = None;
        shell.request_redraw();
    }
}

/// Converts one rendered media kind into the host-visible context-menu media kind.
/// 将一种已渲染媒体类型转换为宿主可见的右键菜单媒体类型。
fn selectable_context_menu_media_kind(
    kind: RenderedMediaKind,
) -> SelectableTranscriptContextMenuMediaKind {
    match kind {
        RenderedMediaKind::Image => SelectableTranscriptContextMenuMediaKind::Image,
        RenderedMediaKind::Audio => SelectableTranscriptContextMenuMediaKind::Audio,
        RenderedMediaKind::Video => SelectableTranscriptContextMenuMediaKind::Video,
        RenderedMediaKind::Pdf => SelectableTranscriptContextMenuMediaKind::Pdf,
    }
}

/// Converts one internal media target into a host-visible context-menu media target.
/// 将一个内部媒体目标转换为宿主可见的右键菜单媒体目标。
fn selectable_context_menu_media_target(
    target: TranscriptMediaTarget,
) -> SelectableTranscriptContextMenuMediaTarget {
    SelectableTranscriptContextMenuMediaTarget {
        kind: selectable_context_menu_media_kind(target.kind),
        file_name: target.file_name,
        local_path: target.local_path,
        is_active: target.is_active,
    }
}

/// Handles one drag move by projecting the live selection head and publishing any needed parent auto-scroll request.
/// 处理一次拖拽移动事件，通过投影实时选区头部并发布所需的父级自动滚动请求。
fn handle_drag_cursor_moved<Message>(
    document: &RenderedTranscriptDocument,
    on_drag_auto_scroll: &dyn Fn(Option<SelectableTranscriptDragAutoScroll>) -> Message,
    state: &mut SelectableTranscriptState,
    widget_bounds: Rectangle,
    local_viewport: Rectangle,
    cursor: mouse::Cursor,
    shell: &mut Shell<'_, Message>,
) {
    if let Some(position) = cursor.position_including_levitation() {
        let local_position = localize_point(position, widget_bounds.position());
        if let Some(hit_char) =
            drag_selection_head_char(document, state, local_viewport, local_position)
        {
            if hit_char != state.head_char {
                state.drag_moved = true;
            }
            state.head_char = hit_char;
            shell.request_redraw();
            shell.capture_event();
        }

        publish_drag_auto_scroll_request(
            state,
            drag_auto_scroll_request_for_position(local_viewport, local_position),
            on_drag_auto_scroll,
            shell,
        );
    } else {
        publish_drag_auto_scroll_request(state, None, on_drag_auto_scroll, shell);
    }
}

/// Handles one primary-button release by finalizing selection, dispatching clicked link actions, and stopping drag auto-scroll.
/// 处理一次主按钮释放事件，以完成选区、分发被点击的链接动作并停止拖拽自动滚动。
fn handle_left_button_released<Message>(
    document: &RenderedTranscriptDocument,
    on_drag_auto_scroll: &dyn Fn(Option<SelectableTranscriptDragAutoScroll>) -> Message,
    on_interaction: &dyn Fn(SelectableTranscriptInteraction) -> Message,
    state: &mut SelectableTranscriptState,
    widget_bounds: Rectangle,
    local_viewport: Rectangle,
    cursor: mouse::Cursor,
    shell: &mut Shell<'_, Message>,
) {
    if let Some(position) = cursor.position_including_levitation() {
        let local_position = localize_point(position, widget_bounds.position());
        if let Some(hit_char) =
            drag_selection_head_char(document, state, local_viewport, local_position)
        {
            if hit_char != state.head_char {
                state.drag_moved = true;
            }
            state.head_char = hit_char;
            shell.request_redraw();
        }
    }
    if !state.drag_moved
        && state.anchor_char == state.head_char
        && let Some(link_target) =
            clicked_transcript_link_target(document, state, widget_bounds, cursor)
    {
        dispatch_clicked_link_target(document, on_interaction, state, shell, link_target);
    }

    state.dragging = false;
    state.drag_moved = false;
    shell.request_redraw();
    publish_drag_auto_scroll_request(state, None, on_drag_auto_scroll, shell);
    shell.capture_event();
}

/// Resolves the clicked synthetic link target under a non-drag primary click, including background chrome actions.
/// 解析一次非拖拽主按钮点击下命中的合成链接目标，包括背景外框动作。
fn clicked_transcript_link_target(
    document: &RenderedTranscriptDocument,
    state: &SelectableTranscriptState,
    widget_bounds: Rectangle,
    cursor: mouse::Cursor,
) -> Option<String> {
    let position = cursor.position_over(widget_bounds)?;
    let local_position = localize_point(position, widget_bounds.position());

    hit_test_rendered_document(document, state, local_position)
        .and_then(|hit_char| document.link_at_char_offset(hit_char))
        .or_else(|| background_action_link_at_position(document, state, local_position))
}

/// Dispatches one clicked synthetic link target into either transcript-level actions or direct clipboard output.
/// 将一个已点击的合成链接目标分发为转录级动作，或直接输出到剪贴板。
fn dispatch_clicked_link_target<Message>(
    document: &RenderedTranscriptDocument,
    on_interaction: &dyn Fn(SelectableTranscriptInteraction) -> Message,
    state: &mut SelectableTranscriptState,
    shell: &mut Shell<'_, Message>,
    link_target: String,
) {
    if let Some(link_action) = document.link_action_for_target(&link_target) {
        match link_action {
            RenderedTranscriptLinkAction::ToggleBlockInline(block_id) => {
                shell.publish(on_interaction(
                    SelectableTranscriptInteraction::ToggleBlockInline(block_id.clone()),
                ));
                record_link_click(state, link_target);
            }
            RenderedTranscriptLinkAction::CopyText(copy_payload) => {
                shell.write_clipboard(copy_payload.clone().into());
            }
            RenderedTranscriptLinkAction::OpenThinkingModal(block_id) => {
                shell.publish(on_interaction(
                    SelectableTranscriptInteraction::OpenThinkingModal(block_id.clone()),
                ));
                record_link_click(state, link_target);
            }
            RenderedTranscriptLinkAction::OpenToolModal(block_id) => {
                shell.publish(on_interaction(
                    SelectableTranscriptInteraction::OpenToolModal(block_id.clone()),
                ));
                record_link_click(state, link_target);
            }
            RenderedTranscriptLinkAction::OpenImagePreview {
                file_name,
                local_path,
            } => {
                shell.publish(on_interaction(
                    SelectableTranscriptInteraction::OpenImagePreview {
                        file_name: file_name.clone(),
                        local_path: local_path.clone(),
                    },
                ));
                record_link_click(state, link_target);
            }
            RenderedTranscriptLinkAction::ToggleAudioPlayback {
                file_name,
                local_path,
            } => {
                shell.publish(on_interaction(
                    SelectableTranscriptInteraction::ToggleAudioPlayback {
                        file_name: file_name.clone(),
                        local_path: local_path.clone(),
                    },
                ));
                record_link_click(state, link_target);
            }
            RenderedTranscriptLinkAction::OpenMediaWithDefaultApp(local_path) => {
                shell.publish(on_interaction(
                    SelectableTranscriptInteraction::OpenMediaWithDefaultApp(local_path.clone()),
                ));
                record_link_click(state, link_target);
            }
            RenderedTranscriptLinkAction::ShowTurnStatsTooltip { .. } => {}
        }
    } else {
        shell.write_clipboard(link_target.into());
    }
}

/// Records the latest clicked synthetic link target and timestamp so future multi-click behaviors can keep stable intent.
/// 记录最近一次点击的合成链接目标及时间戳，以便未来多击行为保持稳定语义。
fn record_link_click(state: &mut SelectableTranscriptState, link_target: String) {
    state.last_click_link_target = Some(link_target);
    state.last_click_instant = Some(Instant::now());
}

/// Publishes one changed drag auto-scroll request to the parent scroll container while suppressing duplicates.
/// 在抑制重复消息的前提下向父级滚动容器发布一次变更后的拖拽自动滚动请求。
fn publish_drag_auto_scroll_request<Message>(
    state: &mut SelectableTranscriptState,
    request: Option<SelectableTranscriptDragAutoScroll>,
    on_drag_auto_scroll: &dyn Fn(Option<SelectableTranscriptDragAutoScroll>) -> Message,
    shell: &mut Shell<'_, Message>,
) {
    if state.drag_auto_scroll_request == request {
        return;
    }

    state.drag_auto_scroll_request = request;
    shell.publish(on_drag_auto_scroll(request));
}
