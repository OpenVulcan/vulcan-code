//! Permission mode picker shared by the keyboard, mouse and command palette.
//! 键盘、鼠标与指令面板共享的权限模式选择器。

use super::App;
use super::action::{PromptDialog, PromptThen};
use super::overlay::{Overlay, Picker, PickerAction, PickerItem, PickerKind};
use super::pages::PageId;
use crate::{
    backend::{Command, ConnectionState},
    model::*,
    tr,
};
use vulcan_service_api::{SessionUnattendedUpdateRequest, UnattendedDefaultResponse};

/// Modes in the same order as the WebUI conversation settings menu.
/// 与 WebUI 会话设置菜单相同顺序的模式。
const MODES: [PermissionMode; 4] = [
    PermissionMode::ReadOnly,
    PermissionMode::WorkspaceWrite,
    PermissionMode::AutoApprove,
    PermissionMode::FullAccess,
];

/// Translated label of one permission mode.
/// 单个权限模式的翻译标签。
pub fn label(mode: PermissionMode) -> String {
    match mode {
        PermissionMode::ReadOnly => tr!("permissionMode.readOnly"),
        PermissionMode::WorkspaceWrite => tr!("permissionMode.workspaceWrite"),
        PermissionMode::AutoApprove => tr!("permissionMode.autoApprove"),
        PermissionMode::FullAccess => tr!("permissionMode.fullAccess"),
    }
}

/// Translated one-line effect of one permission mode.
/// 单个权限模式的一行效果说明。
fn hint(mode: PermissionMode) -> String {
    match mode {
        PermissionMode::ReadOnly => tr!("permissionMode.readOnlyHint"),
        PermissionMode::WorkspaceWrite => tr!("permissionMode.workspaceWriteHint"),
        PermissionMode::AutoApprove => tr!("permissionMode.autoApproveHint"),
        PermissionMode::FullAccess => tr!("permissionMode.fullAccessHint"),
    }
}

/// Open the picker for the selected session or draft; nothing opens before its mode loads.
/// 为选中会话或草稿打开选择器；模式加载前不打开。
/// Auto-approve needs a configured judge: until then it explains why and opens the judge page.
/// 自动审批需要已配置的判定器：在此之前说明原因并打开判定页。
/// The unattended entry follows the modes only under auto-approve, the one mode with a reviewer;
/// a draft shows why it is not available yet.
/// 无人值守入口只在自动审批下位于模式之后，因为只有该模式有复核器；草稿会说明为何尚不可用。
pub fn open(app: &mut App) {
    let Some(session) = app.session() else {
        return;
    };
    let Some(current) = session.permission_mode else {
        return;
    };
    let unattended = session.unattended.clone();
    let Some(session_id) = app.selected.clone() else {
        return;
    };
    let needs_setup = app.permission_judge_backend.is_none();
    let mut items: Vec<PickerItem> = MODES
        .into_iter()
        .map(|mode| {
            let greyed = mode == PermissionMode::AutoApprove && needs_setup;
            PickerItem {
                label: label(mode),
                detail: if greyed {
                    tr!("permissionMode.autoApproveNeedsSetup")
                } else {
                    hint(mode)
                },
                hint: String::new(),
                current: mode == current,
                action: if greyed {
                    PickerAction::Page(PageId::PermissionJudge)
                } else {
                    PickerAction::PermissionMode(session_id.clone(), mode)
                },
            }
        })
        .collect();
    if current != PermissionMode::AutoApprove {
        app.overlay = Some(picker(items));
        return;
    }
    // The review prompt is edited on the review rules page, which shows the session's own prompt.
    // 复核附加提示词在复核规则页编辑，该页显示会话自有提示词。
    items.push(PickerItem {
        label: tr!("permissionMode.reviewPrompt.menu"),
        detail: tr!("permissionMode.reviewPrompt.menuHint"),
        hint: String::new(),
        current: false,
        action: PickerAction::Page(PageId::ReviewRules),
    });
    let on = matches!(unattended, Unattended::On(_));
    items.push(PickerItem {
        label: if on {
            tr!("permissionMode.unattended.menuOn")
        } else {
            tr!("permissionMode.unattended.menuOff")
        },
        detail: if unattended == Unattended::Unavailable {
            tr!("permissionMode.unattended.draftHint")
        } else {
            tr!("permissionMode.unattended.menuHint")
        },
        hint: String::new(),
        current: false,
        action: PickerAction::Unattended,
    });
    if on {
        items.push(PickerItem {
            label: tr!("permissionMode.unattended.disable"),
            detail: String::new(),
            hint: String::new(),
            current: false,
            action: PickerAction::UnattendedOff,
        });
    }
    app.overlay = Some(picker(items));
}

/// Wraps the mode items in the choice picker overlay.
/// 将模式条目包装为选择器浮层。
fn picker(items: Vec<PickerItem>) -> Overlay {
    Overlay::Picker(Picker::new(
        PickerKind::Choice,
        tr!("permissionMode.label"),
        tr!("tui.page.search"),
        items,
    ))
}

/// Request one mode; the displayed mode changes only when the backend confirms it. Auto-approve
/// without a configured judge opens the judge page instead.
/// 请求一个模式；只有后端确认后显示的模式才会改变。判定器未配置时，自动审批改为打开判定页。
/// A choice made for `session_id` is dropped once another session is selected.
/// 为 `session_id` 做出的选择在选中其他会话后被丢弃。
pub fn select(app: &mut App, session_id: &str, mode: PermissionMode) {
    if app.selected.as_deref() != Some(session_id) {
        return;
    }
    if mode == PermissionMode::AutoApprove && app.permission_judge_backend.is_none() {
        super::action::perform(
            app,
            super::action::UiAction::GoPage(PageId::PermissionJudge),
        );
        return;
    }
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

/// Open the policy prompt: an unattended session edits its own text, otherwise the layered default
/// is read first. Nothing is applied until the user confirms.
/// 打开策略输入框：无人值守会话编辑其自身文本，否则先读取分层默认值。用户确认前不应用任何内容。
pub fn edit_unattended(app: &mut App) {
    let Some(id) = app.selected.clone() else {
        return;
    };
    let Some(state) = app.session().map(|session| session.unattended.clone()) else {
        return;
    };
    match state {
        Unattended::Unavailable => app.toast(
            NoticeSeverity::Info,
            tr!("permissionMode.unattended.draftHint"),
        ),
        Unattended::On(policy) => open_prompt(
            app,
            id,
            tr!("permissionMode.unattended.editTitle"),
            policy,
            None,
        ),
        Unattended::Off if app.connection == ConnectionState::Connected => {
            app.backend.send(Command::LoadSessionUnattendedDefault(
                vulcan_service_api::UnattendedDefaultRequest { session_id: id },
            ));
        }
        Unattended::Off => {}
    }
}

/// Open the enable prompt once its prefill arrives, only while that session is still selected and
/// still attended; a failed read leaves the field empty for the user to fill.
/// 预填值到达后打开开启输入框，仅当该会话仍被选中且仍未开启无人值守；读取失败时留空供用户填写。
pub fn open_unattended_prompt(
    app: &mut App,
    conversation_id: &str,
    default: Option<UnattendedDefaultResponse>,
) {
    if app.selected.as_deref() != Some(conversation_id)
        || app.session().map(|session| &session.unattended) != Some(&Unattended::Off)
    {
        return;
    }
    let (input, source) = match default {
        Some(default) => {
            let source = match default.source.as_str() {
                "project" => tr!("permissionMode.unattended.sourceProject"),
                "system" => tr!("permissionMode.unattended.sourceSystem"),
                _ => tr!("permissionMode.unattended.sourceNone"),
            };
            (default.deny_policy, source)
        }
        None => (String::new(), tr!("permissionMode.unattended.loadFailed")),
    };
    open_prompt(
        app,
        conversation_id.to_string(),
        tr!("permissionMode.unattended.enableTitle"),
        input,
        Some(source),
    );
}

/// Shows the policy prompt with the mode's effect and where the text came from.
/// 显示策略输入框，附带该模式的效果与文本来源。
fn open_prompt(app: &mut App, id: String, title: String, input: String, source: Option<String>) {
    let mut description = vec![tr!("permissionMode.unattended.body")];
    description.extend(source);
    app.overlay = Some(Overlay::Prompt(Box::new(PromptDialog::new(
        title,
        description.join("\n\n"),
        input,
        PromptThen::Unattended(id),
        false,
    ))));
}

/// Save the confirmed policy, or turn the mode off with `None`; the shown state changes only when
/// the backend confirms it.
/// 保存确认的策略，或以 `None` 关闭该模式；只有后端确认后显示的状态才会改变。
pub fn apply_unattended(app: &mut App, unattended: Option<UnattendedConfig>) {
    let Some(id) = app.selected.clone() else {
        return;
    };
    if app.connection != ConnectionState::Connected {
        return;
    }
    app.backend.send(Command::SetSessionUnattended(
        SessionUnattendedUpdateRequest {
            session_id: id,
            unattended,
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
    fn loaded_app(judge: Option<JudgeBackend>) -> (App, Receiver<Command>) {
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
        app.permission_judge_backend = judge;
        app.apply(BackendEvent::PermissionMode {
            conversation_id: "session-a".into(),
            mode: PermissionMode::AutoApprove,
            unattended: Unattended::Off,
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

    /// Auto-approve opens the judge page until a judge is configured, then becomes a mode choice.
    /// 判定器配置前自动审批打开判定页，配置后成为普通模式选项。
    #[test]
    fn auto_approve_follows_the_configured_judge() {
        for (judge, choosable) in [(None, false), (Some(JudgeBackend::Local), true)] {
            let (mut app, _) = loaded_app(judge);
            open(&mut app);
            let actions = actions(&app);
            assert_eq!(
                actions.iter().any(|a| matches!(
                    a,
                    PickerAction::PermissionMode(_, PermissionMode::AutoApprove)
                )),
                choosable,
                "judge backend {judge:?}"
            );
            assert_eq!(
                actions
                    .iter()
                    .any(|a| matches!(a, PickerAction::Page(PageId::PermissionJudge))),
                !choosable,
                "judge backend {judge:?}"
            );
        }
    }

    /// Unattended mode is offered only under auto-approve.
    /// 无人值守只在自动审批下提供。
    #[test]
    fn unattended_entry_is_offered_only_under_auto_approve() {
        let (mut app, _) = loaded_app(Some(JudgeBackend::Local));
        for mode in [
            PermissionMode::ReadOnly,
            PermissionMode::WorkspaceWrite,
            PermissionMode::AutoApprove,
            PermissionMode::FullAccess,
        ] {
            app.apply(BackendEvent::PermissionMode {
                conversation_id: "session-a".into(),
                mode,
                unattended: Unattended::Off,
            });
            open(&mut app);
            assert_eq!(
                actions(&app)
                    .iter()
                    .any(|a| matches!(a, PickerAction::Unattended)),
                mode == PermissionMode::AutoApprove,
                "mode {mode:?}"
            );
            app.overlay = None;
        }
    }

    /// Selecting sends one request and leaves the shown mode until the backend confirms it.
    /// 选择只发送一次请求，后端确认前不改变显示的模式。
    #[test]
    fn selecting_requests_without_optimistic_change() {
        let (mut app, commands) = loaded_app(Some(JudgeBackend::Local));
        select(&mut app, "session-a", PermissionMode::AutoApprove);
        assert!(commands.try_recv().is_err());
        select(&mut app, "session-b", PermissionMode::ReadOnly);
        assert!(commands.try_recv().is_err());
        select(&mut app, "session-a", PermissionMode::ReadOnly);
        assert!(matches!(
            commands.try_recv(),
            Ok(Command::SetSessionPermissionMode(request))
                if request.mode == PermissionMode::ReadOnly && request.session_id == "session-a"
        ));
        assert_eq!(
            app.session().and_then(|s| s.permission_mode),
            Some(PermissionMode::AutoApprove)
        );
    }

    /// Turning unattended mode on reads the default first and saves only the confirmed text.
    /// 开启无人值守模式先读取默认值，且只保存确认的文本。
    #[test]
    fn unattended_mode_saves_only_the_confirmed_policy() {
        let (mut app, commands) = loaded_app(Some(JudgeBackend::Local));
        open(&mut app);
        assert!(
            actions(&app)
                .iter()
                .any(|a| matches!(a, PickerAction::Unattended))
        );
        assert!(
            !actions(&app)
                .iter()
                .any(|a| matches!(a, PickerAction::UnattendedOff))
        );
        edit_unattended(&mut app);
        assert!(matches!(
            commands.try_recv(),
            Ok(Command::LoadSessionUnattendedDefault(request)) if request.session_id == "session-a"
        ));
        app.overlay = None;
        app.apply(BackendEvent::UnattendedDefault {
            conversation_id: "session-a".into(),
            default: Some(UnattendedDefaultResponse {
                deny_policy: "never push".into(),
                source: "project".into(),
                max_chars: 4000,
            }),
        });
        let Some(Overlay::Prompt(dialog)) = &app.overlay else {
            panic!("the prefill opens the policy prompt");
        };
        assert_eq!(dialog.input, "never push");
        assert!(matches!(&dialog.then, PromptThen::Unattended(id) if id == "session-a"));
        assert!(commands.try_recv().is_err());
        crate::app::action::perform(&mut app, crate::app::action::UiAction::PromptAccept);
        assert!(matches!(
            commands.try_recv(),
            Ok(Command::SetSessionUnattended(request))
                if request.session_id == "session-a"
                    && request.unattended.as_ref().map(|u| u.deny_policy.as_str()) == Some("never push")
        ));
        assert_eq!(
            app.session().map(|s| s.unattended.clone()),
            Some(Unattended::Off)
        );
    }

    /// An unattended session offers turning it off; a draft only explains why it is unavailable.
    /// 无人值守会话提供关闭入口；草稿只说明为何不可用。
    #[test]
    fn unattended_entries_follow_the_session_state() {
        let (mut app, commands) = loaded_app(Some(JudgeBackend::Local));
        app.apply(BackendEvent::PermissionMode {
            conversation_id: "session-a".into(),
            mode: PermissionMode::AutoApprove,
            unattended: Unattended::On("keep main".into()),
        });
        open(&mut app);
        assert!(
            actions(&app)
                .iter()
                .any(|a| matches!(a, PickerAction::UnattendedOff))
        );
        app.overlay = None;
        apply_unattended(&mut app, None);
        assert!(matches!(
            commands.try_recv(),
            Ok(Command::SetSessionUnattended(request)) if request.unattended.is_none()
        ));
        app.apply(BackendEvent::PermissionMode {
            conversation_id: "session-a".into(),
            mode: PermissionMode::AutoApprove,
            unattended: Unattended::Unavailable,
        });
        edit_unattended(&mut app);
        assert!(commands.try_recv().is_err());
        assert!(app.overlay.is_none());
    }
}
