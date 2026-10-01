// Context menus for projects and conversations, mirroring the WebUI sidebar menus.
// 项目与会话的上下文菜单，对应 WebUI 侧边栏菜单。

use super::App;
use super::action::{PromptThen, UiAction, confirm, prompt};
use super::commands::AppCommand;
use super::overlay::{Menu, MenuItem};
use crate::backend::Command;
use crate::tr;

fn entry(label: String, hint: &str, action: UiAction) -> MenuItem {
    MenuItem {
        label,
        hint: hint.to_string(),
        action,
        danger: false,
    }
}

fn danger(label: String, hint: &str, action: UiAction) -> MenuItem {
    MenuItem {
        danger: true,
        ..entry(label, hint, action)
    }
}

pub fn project_menu(app: &App, project_id: &str) -> Option<Menu> {
    let project = app.projects.iter().find(|p| p.id == project_id)?;
    let id = project.id.clone();
    let count = app
        .conversations
        .iter()
        .filter(|c| c.project_id.as_deref() == Some(project_id))
        .count();
    let mut items = vec![
        entry(
            tr!("projectMenu.newConversation"),
            "n",
            UiAction::NewConversation(Some(id.clone())),
        ),
        entry(
            if project.pinned {
                tr!("projectMenu.unpin")
            } else {
                tr!("projectMenu.pin")
            },
            "p",
            UiAction::Send(Command::SetProjectPinned {
                project_id: id.clone(),
                pinned: !project.pinned,
            }),
        ),
        entry(
            tr!("projectMenu.renameProject"),
            "r",
            prompt(
                tr!("projectMenu.renameProject"),
                project.path.clone(),
                project.name.clone(),
                PromptThen::RenameProject(id.clone()),
            ),
        ),
        // Browsing reuses the attachment picker rooted at the project directory.
        // 浏览复用以项目目录为根的附件选择器。
        entry(
            tr!("projectMenu.openDirectory"),
            "",
            UiAction::Browse(project.path.clone()),
        ),
    ];
    if count > 0 {
        items.push(danger(
            tr!("projectMenu.archiveConversations"),
            "",
            confirm(
                tr!("projectMenu.archiveConversations"),
                tr!(
                    "tui.confirm.archiveProjectBody",
                    name = project.name,
                    count = count
                ),
                tr!("conversationMenu.archive"),
                UiAction::Send(Command::ArchiveProjectConversations {
                    project_id: id.clone(),
                    remove_project: false,
                }),
            ),
        ));
    }
    items.push(danger(
        tr!("projectMenu.removeProject"),
        "Del",
        confirm(
            tr!("projectMenu.removeProject"),
            tr!("tui.confirm.removeProjectBody", name = project.name),
            tr!("projectMenu.removeProject"),
            UiAction::Send(Command::ArchiveProjectConversations {
                project_id: id,
                remove_project: true,
            }),
        ),
    ));
    Some(Menu {
        title: project.name.clone(),
        items,
        cursor: 0,
        anchor: None,
    })
}

pub fn conversation_menu(app: &App, conversation_id: &str) -> Option<Menu> {
    let conversation = app.conversations.iter().find(|c| c.id == conversation_id)?;
    let id = conversation.id.clone();
    let mut items = vec![
        entry(
            if conversation.pinned {
                tr!("conversationMenu.unpin")
            } else {
                tr!("conversationMenu.pin")
            },
            "p",
            UiAction::Send(Command::SetConversationPinned {
                conversation_id: id.clone(),
                pinned: !conversation.pinned,
            }),
        ),
        entry(
            tr!("conversationMenu.rename"),
            "r",
            prompt(
                tr!("conversationMenu.rename"),
                String::new(),
                conversation.title.clone(),
                PromptThen::RenameConversation(id.clone()),
            ),
        ),
    ];
    if !conversation.unread {
        items.push(entry(
            tr!("conversationMenu.markUnread"),
            "u",
            UiAction::Send(Command::MarkUnread {
                conversation_id: id.clone(),
            }),
        ));
    }
    if app.selected.as_deref() == Some(conversation_id) {
        items.push(entry(
            tr!("tui.history.title"),
            "Ctrl+F",
            UiAction::App(AppCommand::History),
        ));
    }
    items.push(danger(
        tr!("conversationMenu.archive"),
        "Del",
        confirm(
            tr!("conversationMenu.archive"),
            tr!("tui.confirm.archiveBody", title = conversation.title),
            tr!("conversationMenu.archive"),
            UiAction::Send(Command::ArchiveConversation {
                conversation_id: id,
            }),
        ),
    ));
    Some(Menu {
        title: conversation.title.clone(),
        items,
        cursor: 0,
        anchor: None,
    })
}
