//! Lightweight transcript context-menu and shortcut helpers kept separate from the core selectable-surface shell.
//! 与核心可选表面外壳分离的轻量转录右键菜单与快捷键辅助逻辑。

use super::interaction::current_selection_for_state;
use super::{
    RenderedTranscriptDocument, SelectableTranscriptState, TranscriptContextMenuAction,
    TranscriptContextMenuState,
};
use iced::advanced::Shell;
use iced::{Point, Rectangle, Size, keyboard};

/// Fixed width used by the lightweight transcript context menu.
/// 转录轻量右键菜单使用的固定宽度。
const TRANSCRIPT_CONTEXT_MENU_WIDTH: f32 = 136.0;

/// Fixed row height used by each transcript context-menu item.
/// 每条转录右键菜单项使用的固定行高。
pub(super) const TRANSCRIPT_CONTEXT_MENU_ITEM_HEIGHT: f32 = 30.0;

/// Outer padding applied around the transcript context-menu text labels.
/// 应用于转录右键菜单文本标签周围的外边距。
pub(super) const TRANSCRIPT_CONTEXT_MENU_ITEM_PADDING_X: f32 = 12.0;

/// Corner radius used by the lightweight transcript context menu.
/// 轻量转录右键菜单使用的圆角半径。
pub(super) const TRANSCRIPT_CONTEXT_MENU_RADIUS: f32 = 10.0;

/// Returns the lightweight transcript context-menu actions visible under the current selection state.
/// 根据当前选区状态返回轻量转录右键菜单可见的动作集合。
pub(super) fn transcript_context_menu_actions(
    has_selection: bool,
) -> Vec<TranscriptContextMenuAction> {
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
) -> &'static str {
    match action {
        TranscriptContextMenuAction::CopySelection => "复制",
        TranscriptContextMenuAction::SelectAll => "全选",
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
