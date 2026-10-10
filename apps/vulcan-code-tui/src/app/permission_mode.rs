//! Permission mode picker shared by the keyboard, mouse and command palette.
//! 键盘、鼠标与指令面板共享的权限模式选择器。

use super::App;
use super::overlay::{Overlay, Picker, PickerAction, PickerItem, PickerKind};
use super::pages::PageId;
use crate::{
    backend::{Command, ConnectionState},
    model::*,
    tr,
};

/// Modes in the same order as the WebUI conversation settings menu.
/// 与 WebUI 会话设置菜单相同顺序的模式。
const MODES: [PermissionMode; 3] = [
    PermissionMode::ReadOnly,
    PermissionMode::WorkspaceWrite,
    PermissionMode::FullAccess,
];

/// Translated label of one permission mode.
/// 单个权限模式的翻译标签。
pub fn label(mode: PermissionMode) -> String {
    match mode {
        PermissionMode::ReadOnly => tr!("permissionMode.readOnly"),
        PermissionMode::WorkspaceWrite => tr!("permissionMode.workspaceWrite"),
        PermissionMode::FullAccess => tr!("permissionMode.fullAccess"),
    }
}

/// Translated one-line effect of one permission mode.
/// 单个权限模式的一行效果说明。
fn hint(mode: PermissionMode) -> String {
    match mode {
        PermissionMode::ReadOnly => tr!("permissionMode.readOnlyHint"),
        PermissionMode::WorkspaceWrite => tr!("permissionMode.workspaceWriteHint"),
        PermissionMode::FullAccess => tr!("permissionMode.fullAccessHint"),
    }
}

/// Open the picker for the selected session or draft; nothing opens before its mode loads.
/// 为选中会话或草稿打开选择器；模式加载前不打开。
/// The judge entry asks for setup while unset, shows its state when enabled and hides when off.
/// 判定入口在未设定时提示需要设定，启用时显示其状态，关闭时隐藏。
pub fn open(app: &mut App) {
    let Some(current) = app.session().and_then(|session| session.permission_mode) else {
        return;
    };
    let mut items: Vec<PickerItem> = MODES
        .into_iter()
        .map(|mode| PickerItem {
            label: label(mode),
            detail: hint(mode),
            hint: String::new(),
            current: mode == current,
            action: PickerAction::PermissionMode(mode),
        })
        .collect();
    let judge = match app.permission_judge_state {
        None => Some(tr!("permissionMode.judgeNeedsSetup")),
        Some(JudgeState::Enabled) => Some(tr!("permissionMode.judgeEnabled")),
        Some(JudgeState::Off) => None,
    };
    if let Some(judge) = judge {
        items.push(PickerItem {
            label: judge,
            detail: String::new(),
            hint: String::new(),
            current: false,
            action: PickerAction::Page(PageId::PermissionJudge),
        });
    }
    app.overlay = Some(Overlay::Picker(Picker::new(
        PickerKind::Choice,
        tr!("permissionMode.label"),
        tr!("knowledge.search"),
        items,
    )));
}

/// Request one mode; the displayed mode changes only when the backend confirms it.
/// 请求一个模式；只有后端确认后显示的模式才会改变。
pub fn select(app: &mut App, mode: PermissionMode) {
    let Some(id) = app.selected.clone() else {
        return;
    };
    if app.connection != ConnectionState::Connected
        || app.session().and_then(|session| session.permission_mode) == Some(mode)
    {
        return;
    }
    app.backend.send(Command::SetSessionPermissionMode(
        vulcan_service_api::SessionPermissionModeUpdateRequest {
            session_id: id,
            mode,
        },
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{BackendEvent, BackendHandle};
    use crate::ui::theme::Theme;
    use std::sync::mpsc::{self, Receiver};

    /// Creates an isolated app with one loaded session and returns its command observer.
    /// 创建带一个已加载会话的隔离应用，并返回命令观察端。
    fn loaded_app(judge: Option<JudgeState>) -> (App, Receiver<Command>) {
        let (commands, receiver) = mpsc::channel();
        let (_, events) = mpsc::channel();
        let mut app = App::new(
            BackendHandle {
                commands,
                events,
                label: "test".into(),
                is_mock: true,
            },
            Theme::dark(),
            true,
        );
        app.connection = ConnectionState::Connected;
        app.selected = Some("session-a".into());
        app.permission_judge_state = judge;
        app.apply(BackendEvent::PermissionMode {
            conversation_id: "session-a".into(),
            mode: PermissionMode::WorkspaceWrite,
        });
        (app, receiver)
    }

    /// Returns the open picker's actions.
    /// 返回已打开选择器的动作。
    fn actions(app: &App) -> Vec<PickerAction> {
        match app.overlay.as_ref() {
            Some(Overlay::Picker(picker)) => {
                picker.items.iter().map(|i| i.action.clone()).collect()
            }
            _ => Vec::new(),
        }
    }

    /// The judge entry follows the saved switch, and only the three modes stay when it is off.
    /// 判定入口跟随已保存的开关；关闭时只保留三个模式。
    #[test]
    fn judge_entry_follows_the_saved_switch() {
        for (judge, has_entry) in [
            (None, true),
            (Some(JudgeState::Enabled), true),
            (Some(JudgeState::Off), false),
        ] {
            let (mut app, _) = loaded_app(judge);
            open(&mut app);
            let actions = actions(&app);
            assert_eq!(
                actions
                    .iter()
                    .filter(|a| matches!(a, PickerAction::PermissionMode(_)))
                    .count(),
                3
            );
            assert_eq!(
                actions
                    .iter()
                    .any(|a| matches!(a, PickerAction::Page(PageId::PermissionJudge))),
                has_entry,
                "judge state {judge:?}"
            );
        }
    }

    /// Selecting sends one request and leaves the shown mode until the backend confirms it.
    /// 选择只发送一次请求，后端确认前不改变显示的模式。
    #[test]
    fn selecting_requests_without_optimistic_change() {
        let (mut app, commands) = loaded_app(None);
        select(&mut app, PermissionMode::WorkspaceWrite);
        assert!(commands.try_recv().is_err());
        select(&mut app, PermissionMode::ReadOnly);
        assert!(matches!(
            commands.try_recv(),
            Ok(Command::SetSessionPermissionMode(request))
                if request.mode == PermissionMode::ReadOnly && request.session_id == "session-a"
        ));
        assert_eq!(
            app.session().and_then(|s| s.permission_mode),
            Some(PermissionMode::WorkspaceWrite)
        );
    }
}
