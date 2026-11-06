// Session activity behavior on an isolated app: inert channels, synthetic results, no service.
// 隔离应用上的会话活动行为：静默通道、合成结果，不连接服务。

use std::sync::mpsc::{self, Receiver};

use serde_json::{Value, json};

use super::action::{UiAction, perform};
use super::activity::{ActivityAction, ActivityPage, FINISHED_JOB_LIMIT};
use super::*;
use crate::service_api::{ServiceCall, Slot};

/// Creates a connected app on session `s1` and returns its command observer.
/// 创建连接到会话 `s1` 的应用并返回命令观察端。
fn connected_app() -> (App, Receiver<Command>) {
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
    app.selected = Some("s1".into());
    (app, receiver)
}

/// Service calls sent since the last drain.
/// 自上次读取以来发送的服务调用。
fn sent(observer: &Receiver<Command>) -> Vec<ServiceCall> {
    observer
        .try_iter()
        .filter_map(|command| match command {
            Command::Call(call) => Some(call),
            _ => None,
        })
        .collect()
}

/// A synthetic job list of `session` with `running` background jobs.
/// `session` 的合成任务列表，含 `running` 个运行中的后台任务。
fn job_list(session: &str, running: usize) -> Value {
    json!({
        "sessionId": session,
        "jobs": [],
        "runningCount": running,
        "backgroundRunningCount": running,
        "finishedCount": 0,
        "clearedCount": 0,
        "recentLineLimit": 3
    })
}

/// The command list is always read for the selected saved session, its count follows the list
/// and `shellJobChanged`, and a notice for the session reads the list again.
/// 命令列表始终为所选已保存会话读取，计数跟随列表与 `shellJobChanged`，该会话的通知会重新读取列表。
#[test]
fn background_count_follows_the_list_and_job_notices() {
    let (mut app, observer) = connected_app();
    app.tick(Instant::now());
    let calls = sent(&observer);
    assert_eq!(calls.len(), 1, "only the command list is read off screen");
    let list = calls[0].clone();
    assert_eq!(list.method, "service_list_shell_jobs");
    assert_eq!(list.param("/finishedLimit"), &json!(FINISHED_JOB_LIMIT));
    app.tick(Instant::now());
    assert!(
        sent(&observer).is_empty(),
        "an unchanged request is not repeated"
    );

    app.apply(BackendEvent::Slot {
        slot: Slot::ShellJobs,
        call: list,
        value: job_list("s1", 2),
    });
    assert_eq!(app.activity.background_count("s1"), 2);

    app.apply(BackendEvent::ShellJobChanged(
        vulcan_service_api::ShellJobChangedEvent {
            session_id: "s1".into(),
            job_id: "job-1".into(),
            change: "finished".into(),
            running_count: 1,
            background_running_count: 1,
        },
    ));
    assert_eq!(app.activity.background_count("s1"), 1);
    app.tick(Instant::now());
    let reread = sent(&observer);
    assert_eq!(reread.len(), 1);
    assert_eq!(reread[0].method, "service_list_shell_jobs");

    // A draft has no activity and reads nothing.
    // 草稿没有活动，也不读取任何内容。
    app.selected = Some("draft:1".into());
    app.tick(Instant::now());
    assert!(sent(&observer).is_empty());
    assert!(app.activity.jobs.is_none());
}

/// Results for a session that is no longer selected are dropped.
/// 已不再选中会话的结果被丢弃。
#[test]
fn results_of_another_session_are_rejected() {
    let (mut app, observer) = connected_app();
    app.tick(Instant::now());
    let old = sent(&observer).remove(0);
    app.selected = Some("s2".into());
    app.tick(Instant::now());
    assert_eq!(sent(&observer)[0].param("/sessionId"), "s2");
    app.apply(BackendEvent::Slot {
        slot: Slot::ShellJobs,
        call: old,
        value: job_list("s1", 3),
    });
    assert!(app.activity.jobs.is_none());
    assert_eq!(app.activity.background_count("s1"), 0);
}

/// The subagent snapshot is read only while its page is shown; "show all" changes the request
/// and a clear writes the scope of the page shown.
/// 子代理快照仅在其页面显示时读取；“显示全部”改变请求，清理写入所显示页面的范围。
#[test]
fn shown_pages_load_and_show_all_changes_the_request() {
    let (mut app, observer) = connected_app();
    perform(
        &mut app,
        UiAction::SessionActivity(ActivityAction::Show(ActivityPage::Subagents)),
    );
    app.tick(Instant::now());
    let methods: Vec<&str> = sent(&observer).iter().map(|call| call.method).collect();
    assert!(methods.contains(&"service_session_activity"));

    perform(
        &mut app,
        UiAction::SessionActivity(ActivityAction::Page(ActivityPage::Commands)),
    );
    perform(
        &mut app,
        UiAction::SessionActivity(ActivityAction::ShowAll(true)),
    );
    let calls = sent(&observer);
    let all = calls
        .iter()
        .find(|call| call.method == "service_list_shell_jobs")
        .expect("show all reads the list again");
    assert_eq!(all.param("/includeCleared"), &json!(true));
    assert!(all.param("/finishedLimit").is_null());

    app.apply(BackendEvent::Slot {
        slot: Slot::ShellJobs,
        call: all.clone(),
        value: json!({
            "sessionId": "s1",
            "jobs": [{
                "jobId": "job-1", "command": "make", "purpose": "", "shell": "bash", "cwd": ".",
                "startedAtMs": 0, "background": false, "running": false, "status": "completed",
                "durationMs": 10, "totalLines": 1, "totalBytes": 2, "logAvailable": true
            }],
            "runningCount": 0, "backgroundRunningCount": 0, "finishedCount": 1,
            "clearedCount": 0, "recentLineLimit": 3
        }),
    });
    perform(&mut app, UiAction::SessionActivity(ActivityAction::Clear));
    let clear = sent(&observer);
    assert_eq!(clear[0].method, "service_clear_session_activity");
    assert_eq!(clear[0].param("/scope"), "commands");
}

/// A restart counts the running background commands first: none restarts at once, some ask
/// first, and a failed count never blocks the update.
/// 重启前先统计运行中的后台命令：没有时立即重启，有时先询问，计数失败绝不阻止更新。
#[test]
fn restart_waits_for_the_background_command_count() {
    let restart = "service_restart_to_update";
    for (result, asks) in [
        (Ok(json!(0)), false),
        (Ok(json!(2)), true),
        (Err(()), false),
    ] {
        let (mut app, observer) = connected_app();
        perform(&mut app, UiAction::RestartToUpdate);
        let count = sent(&observer).remove(0);
        assert_eq!(count.method, "service_running_shell_job_count");
        perform(&mut app, UiAction::RestartToUpdate);
        assert!(sent(&observer).is_empty(), "one count at a time");
        match result {
            Ok(value) => app.apply(BackendEvent::Slot {
                slot: Slot::RunningShellJobCount,
                call: count,
                value,
            }),
            Err(()) => app.apply(BackendEvent::SlotFailed {
                slot: Slot::RunningShellJobCount,
                call: count,
                message: "unavailable".into(),
            }),
        }
        if asks {
            assert!(matches!(app.overlay, Some(overlay::Overlay::Confirm(_))));
            assert!(
                sent(&observer).is_empty(),
                "nothing restarts before the answer"
            );
            perform(&mut app, UiAction::ConfirmAccept);
        }
        let calls = sent(&observer);
        assert!(
            calls.iter().any(|call| call.method == restart),
            "the restart is sent"
        );
    }
}
