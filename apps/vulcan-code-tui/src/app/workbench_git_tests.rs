// Workbench selection and Git repository regressions over inert channels and synthetic state.
// 基于静默通道与合成状态的工作台选择与 Git 仓库回归测试。

use super::*;
use std::sync::mpsc::{self, Receiver};

/// Creates an isolated app and returns its command observer without a backend worker.
/// 创建隔离应用并返回命令观察端，不运行后端工作器。
fn isolated_app() -> (App, Receiver<Command>) {
    // No external I/O is connected to either channel.
    // 两条通道都没有连接外部输入输出。
    let (commands, receiver) = mpsc::channel();
    let (_, events) = mpsc::channel();
    let app = App::new(
        BackendHandle {
            commands,
            events,
            label: "test".into(),
            is_mock: true,
        },
        Theme::dark(),
        true,
    );
    (app, receiver)
}

/// A workbench snapshot listing `ids` whose service-side selection is `selected`.
/// 列出 `ids` 且服务端选择为 `selected` 的工作台快照。
fn workbench(ids: &[&str], selected: Option<&str>) -> BackendEvent {
    BackendEvent::Workbench(WorkbenchSnapshot {
        projects: Vec::new(),
        conversations: ids
            .iter()
            .map(|id| ConversationSummary {
                id: (*id).into(),
                title: (*id).into(),
                project_id: None,
                run_phase: RunPhase::Idle,
                last_updated_label: String::new(),
                unread: false,
                attention: false,
            })
            .collect(),
        selected_conversation_id: selected.map(Into::into),
        models: Vec::new(),
        selected_model_id: String::new(),
        ui_locale: None,
        base_url: String::new(),
    })
}

/// A repository entry of the Git catalog.
/// Git 目录中的一个仓库条目。
fn repository(key: &str, primary: bool, monitoring: bool) -> GitRepository {
    GitRepository {
        key: key.into(),
        name: key.into(),
        relative_path: key.into(),
        primary,
        monitoring,
        changes: monitoring.then_some(0),
        ahead: 0,
    }
}

/// A Git snapshot over `repositories` with `selected` details.
/// 包含 `repositories` 且详情属于 `selected` 的 Git 快照。
fn git(repositories: Vec<GitRepository>, selected: Option<&str>) -> BackendEvent {
    BackendEvent::Git(GitSnapshot {
        repositories,
        selected: selected.map(Into::into),
        branch: "main".into(),
        upstream: None,
        ahead: 0,
        behind: 0,
        files: Vec::new(),
        unpushed: Vec::new(),
    })
}

/// A snapshot still carrying an earlier selection does not pull the user back to it.
/// 仍带有较早选择的快照不会把用户拉回该选择。
#[test]
fn stale_workbench_selection_does_not_bounce_back() {
    let (mut app, _) = isolated_app();
    app.apply(workbench(&["a", "b"], Some("a")));
    assert_eq!(app.selected.as_deref(), Some("a"));
    app.select_conversation("b".into());
    app.apply(workbench(&["a", "b"], Some("a")));
    assert_eq!(app.selected.as_deref(), Some("b"));
    assert_eq!(
        app.sidebar_cursor,
        Some(SidebarRow::Conversation("b".into()))
    );
}

/// A selection that left the list is replaced by the service's selection.
/// 已离开列表的选择由服务端选择取代。
#[test]
fn removed_selection_adopts_the_workbench_selection() {
    let (mut app, _) = isolated_app();
    app.apply(workbench(&["a", "b"], Some("a")));
    app.select_conversation("b".into());
    app.apply(workbench(&["a"], Some("a")));
    assert_eq!(app.selected.as_deref(), Some("a"));
    assert_eq!(
        app.sidebar_cursor,
        Some(SidebarRow::Conversation("a".into()))
    );
}

/// Clearing Git drops the previous project's repositories and details.
/// 清除 Git 时丢弃上一项目的仓库与详情。
#[test]
fn git_cleared_drops_the_previous_project() {
    let (mut app, _) = isolated_app();
    app.apply(git(vec![repository(".", true, true)], Some(".")));
    assert!(app.git.repository().is_some());
    app.apply(BackendEvent::GitCleared);
    assert!(app.git.snapshot.is_none());
    app.apply(git(Vec::new(), None));
    assert!(app.git.repository().is_none());
    assert!(app.git.monitoring_menu().is_none());
}

/// Cycling visits monitored repositories only and wraps around.
/// 循环只经过受监听仓库，并回绕。
#[test]
fn next_repository_cycles_monitored_repositories() {
    let (mut app, _) = isolated_app();
    let repositories = vec![
        repository(".", true, true),
        repository("off", false, false),
        repository("lib", false, true),
    ];
    app.apply(git(repositories.clone(), Some(".")));
    assert_eq!(app.git.next_repository().as_deref(), Some("lib"));
    app.apply(git(repositories, Some("lib")));
    assert_eq!(app.git.next_repository().as_deref(), Some("."));
    app.apply(git(
        vec![repository(".", true, true), repository("off", false, false)],
        Some("."),
    ));
    assert_eq!(app.git.next_repository(), None);
}

/// Each monitoring switch sends the inverted state for its exact repository.
/// 每个监听开关为其准确仓库发送相反状态。
#[test]
fn monitoring_menu_sends_inverted_switches() {
    let (mut app, _) = isolated_app();
    app.apply(git(
        vec![repository(".", true, true), repository("off", false, false)],
        Some("."),
    ));
    let menu = app
        .git
        .monitoring_menu()
        .expect("repositories have switches");
    let sent: Vec<(String, bool)> = menu
        .items
        .iter()
        .filter_map(|item| match &item.action {
            action::UiAction::Send(Command::SetGitMonitoring {
                repository_key,
                enabled,
            }) => Some((repository_key.clone(), *enabled)),
            _ => None,
        })
        .collect();
    assert_eq!(sent, vec![(".".into(), false), ("off".into(), true)]);
}
