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

/// A project without Git hides the Changes tab; the choice returns with a project that has Git.
/// 没有 Git 的项目隐藏变更页；回到有 Git 的项目时恢复该选择。
#[test]
fn changes_tab_hides_without_git_and_returns_with_it() {
    let (mut app, _) = isolated_app();
    assert_eq!(app.panel_tab, PanelTab::Changes);
    assert_eq!(
        app.active_panel_tab(),
        PanelTab::Changes,
        "shown while loading"
    );
    app.apply(git(Vec::new(), None));
    assert!(!app.panel_tabs().contains(&PanelTab::Changes));
    assert_eq!(app.active_panel_tab(), app.panel_tabs()[0]);
    assert_eq!(
        app.panel_tab,
        PanelTab::Changes,
        "the choice itself is kept"
    );
    app.apply(BackendEvent::GitCleared);
    app.apply(git(vec![repository(".", true, true)], Some(".")));
    assert_eq!(app.active_panel_tab(), PanelTab::Changes);
}

/// A failed diff replaces the loading state, and retrying asks for it again.
/// 失败的 diff 取代加载状态，重试会再次请求。
#[test]
fn failed_diff_shows_and_retries() {
    let (mut app, commands) = isolated_app();
    let mut snapshot = git(vec![repository(".", true, true)], Some("."));
    if let BackendEvent::Git(git) = &mut snapshot {
        git.files.push(GitFile {
            path: "a.rs".into(),
            status: GitFileStatus::Modified,
            staged: false,
            additions: 1,
            deletions: 0,
        });
    }
    app.apply(snapshot);
    input::open_diff(&mut app, 0);
    assert!(commands.try_iter().any(
        |command| matches!(command, Command::LoadWorkspaceDiffFileDetail { path } if path == "a.rs")
    ));
    app.apply(BackendEvent::GitDiffFailed {
        path: "a.rs".into(),
        message: Some("unavailable".into()),
    });
    assert_eq!(
        app.git.diff_failures.get("a.rs"),
        Some(&Some("unavailable".into()))
    );
    action::perform(&mut app, action::UiAction::DiffStep(0));
    assert!(!app.git.diff_failures.contains_key("a.rs"));
    assert!(commands.try_iter().any(
        |command| matches!(command, Command::LoadWorkspaceDiffFileDetail { path } if path == "a.rs")
    ));
}

/// A repository click marks it at once; snapshots still carrying the earlier choice are skipped,
/// so quick clicks never jump back.
/// 点击仓库立即标记；仍带较早选择的快照会被跳过，因此快速点击不会跳回。
#[test]
fn repository_choice_holds_against_earlier_snapshots() {
    let (mut app, commands) = isolated_app();
    let repositories = vec![
        repository(".", true, true),
        repository("lib", false, true),
        repository("app", false, true),
    ];
    app.apply(git(repositories.clone(), Some(".")));
    action::perform(
        &mut app,
        action::UiAction::SelectGitRepository("lib".into()),
    );
    action::perform(
        &mut app,
        action::UiAction::SelectGitRepository("app".into()),
    );
    assert_eq!(app.git.marked().map(String::as_str), Some("app"));
    assert!(
        app.git.repository().is_none(),
        "no details of another repository"
    );
    let sent: Vec<String> = commands
        .try_iter()
        .filter_map(|command| match command {
            Command::SelectGitRepository(key) => Some(key),
            _ => None,
        })
        .collect();
    assert_eq!(sent, vec!["lib".to_string(), "app".to_string()]);
    app.apply(git(repositories.clone(), Some("lib")));
    assert_eq!(app.git.marked().map(String::as_str), Some("app"));
    app.apply(git(repositories, Some("app")));
    assert!(app.git.pending.is_none());
    assert_eq!(
        app.git.repository().and_then(|s| s.selected.as_deref()),
        Some("app")
    );
}
