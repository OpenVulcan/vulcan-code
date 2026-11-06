// Session activity: the subagents and the command jobs of the selected root session, mirroring the
// WebUI activity panel (`session-activity-panel.tsx`), its job log (`shell-job-log-view.tsx`),
// the background-command count of the session bar and the restart guard of app updates.
// 会话活动：所选根会话的子代理与命令任务，对应 WebUI 活动面板（`session-activity-panel.tsx`）、
// 任务日志（`shell-job-log-view.tsx`）、会话栏的后台命令计数以及应用更新的重启保护。
//
// The lists live in the Agents tab of the session panel; ended entries before a clear stay
// reachable through "show all", and nothing is deleted. A job log opens as an overlay that starts
// at the newest lines, follows a running job while the view stays at the bottom, and reads one
// earlier page at a time when scrolled to the top, keeping a bounded number of rows.
// 列表位于会话面板的代理页；清理之前已结束的条目可通过“显示全部”查看，不删除任何内容。任务日志以
// 浮层打开，从最新的行开始；视图停留在底部时跟随运行中的任务，滚动到顶部时每次读取更早的一页，
// 且只保留有界数量的行。

use std::collections::HashMap;
use std::time::{Duration, Instant};

use serde_json::Value;
use vulcan_service_api::{
    SessionActivityScope, SessionActivitySnapshot, ShellJobChangedEvent, ShellJobList,
    ShellJobOutputPage, ShellJobOutputRow, ShellJobView, SubagentActivity,
};

use super::action::{self, UiAction};
use super::overlay::Overlay;
use super::{App, Area, PanelTab};
use crate::backend::{Command, ConnectionState};
use crate::model::AgentInfo;
use crate::service_api::session_activity::{self as api, ShellJobOutputQuery};
use crate::service_api::{CallKind, ServiceCall, Slot};
use crate::tr;

/// Finished command jobs listed before the user asks for all of them.
/// 用户要求查看全部之前列出的已结束命令任务数。
pub const FINISHED_JOB_LIMIT: usize = 5;

/// Re-read interval of a shown list while one of its entries runs.
/// 所显示列表中有条目运行时的重读间隔。
pub const LIVE_REFRESH_INTERVAL: Duration = Duration::from_millis(2000);

/// Log lines one output request asks for.
/// 单次输出请求的日志行数。
pub const LOG_PAGE_LINES: usize = 200;

/// Rows a job log keeps; rows beyond it are dropped from the far end.
/// 任务日志保留的行数；超出部分从远端丢弃。
pub const LOG_RETAINED_ROWS: usize = 2000;

/// Re-read interval of a running job's log while it is shown.
/// 显示期间运行中任务日志的重读间隔。
pub const LOG_FOLLOW_INTERVAL: Duration = Duration::from_millis(1500);

/// One page of the activity view.
/// 活动视图的一页。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ActivityPage {
    #[default]
    Subagents,
    Commands,
}

impl ActivityPage {
    /// The slot whose list the page shows.
    /// 该页所显示列表的槽位。
    fn slot(self) -> Slot {
        match self {
            ActivityPage::Subagents => Slot::SessionActivity,
            ActivityPage::Commands => Slot::ShellJobs,
        }
    }

    /// The other page.
    /// 另一页。
    pub fn other(self) -> Self {
        match self {
            ActivityPage::Subagents => ActivityPage::Commands,
            ActivityPage::Commands => ActivityPage::Subagents,
        }
    }
}

/// User actions of the activity view and the job log.
/// 活动视图与任务日志的用户操作。
#[derive(Debug, Clone, PartialEq)]
pub enum ActivityAction {
    /// Shows the session panel on the activity tab at this page (the session bar segments).
    /// 在活动标签页的该页显示会话面板（会话栏的段）。
    Show(ActivityPage),
    /// Switches the page inside the activity tab.
    /// 在活动标签页内切换页面。
    Page(ActivityPage),
    /// Shows every entry (`true`) or only those after the latest clear on the current page.
    /// 在当前页显示全部条目（`true`）或只显示最近一次清理之后的条目。
    ShowAll(bool),
    /// Clears the ended entries of the current page.
    /// 清理当前页已结束的条目。
    Clear,
    /// Stops one running job.
    /// 停止一个运行中的任务。
    StopJob(String),
    /// Opens the log of one job.
    /// 打开一个任务的日志。
    OpenLog(String),
    /// Reads the page above the first row of the open log.
    /// 读取所打开日志首行之上的一页。
    LogEarlier,
}

/// Client state of the activity view for the selected session.
/// 所选会话活动视图的客户端状态。
#[derive(Debug, Default)]
pub struct ActivityState {
    /// Page shown in the activity tab.
    /// 活动标签页显示的页面。
    pub page: ActivityPage,
    /// Subagents before the latest clear are listed.
    /// 列出最近一次清理之前的子代理。
    pub show_all_subagents: bool,
    /// Every finished job is listed, including those before the latest clear.
    /// 列出全部已结束任务，包括最近一次清理之前的任务。
    pub show_all_commands: bool,
    /// Saved session the lists below belong to.
    /// 下方列表所属的已保存会话。
    pub session_id: Option<String>,
    /// Latest command list of the session.
    /// 该会话最新的命令列表。
    pub jobs: Option<ShellJobList>,
    /// Latest subagent snapshot of the session.
    /// 该会话最新的子代理快照。
    pub subagents: Option<SessionActivitySnapshot>,
    /// Running background jobs per session, from the lists and `shellJobChanged`.
    /// 每个会话运行中的后台任务数，来自列表与 `shellJobChanged`。
    pub background: HashMap<String, usize>,
    /// Parameters of the latest list request per slot; a different wanted request is sent.
    /// 每个槽位最近一次列表请求的参数；所需请求不同时发送。
    requested: HashMap<Slot, Value>,
    /// Page shown in the previous tick, so becoming visible re-reads it.
    /// 上一次计时时显示的页面，使变为可见时重新读取。
    visible: Option<ActivityPage>,
    /// A restart waits for the running background command count.
    /// 一次重启正在等待运行中后台命令的计数。
    restart_pending: bool,
}

impl ActivityState {
    /// Whether the current page lists entries before the latest clear.
    /// 当前页是否列出最近一次清理之前的条目。
    pub fn show_all(&self) -> bool {
        match self.page {
            ActivityPage::Subagents => self.show_all_subagents,
            ActivityPage::Commands => self.show_all_commands,
        }
    }

    /// Running background jobs of one session; 0 until something reported it.
    /// 某会话运行中的后台任务数；在有来源报告之前为 0。
    pub fn background_count(&self, session_id: &str) -> usize {
        self.background.get(session_id).copied().unwrap_or(0)
    }

    /// Subagents of the current snapshot in service order (running first).
    /// 当前快照中按服务顺序排列的子代理（运行中在前）。
    pub fn subagent_entries(&self) -> &[SubagentActivity] {
        self.subagents
            .as_ref()
            .map_or(&[], |snapshot| snapshot.subagents.as_slice())
    }

    /// Jobs of the current list in service order (running first).
    /// 当前列表中按服务顺序排列的任务（运行中在前）。
    pub fn job_entries(&self) -> &[ShellJobView] {
        self.jobs.as_ref().map_or(&[], |list| list.jobs.as_slice())
    }

    /// Selectable rows of the current page.
    /// 当前页可选的行数。
    pub fn item_count(&self) -> usize {
        match self.page {
            ActivityPage::Subagents => self.subagent_entries().len(),
            ActivityPage::Commands => self.job_entries().len(),
        }
    }

    /// Ended entries that a clear would hide now.
    /// 清理现在会隐藏的已结束条目数。
    pub fn clearable(&self) -> usize {
        match self.page {
            ActivityPage::Subagents => self
                .subagent_entries()
                .iter()
                .filter(|entry| !entry.task.is_running && !entry.cleared)
                .count(),
            ActivityPage::Commands => self
                .job_entries()
                .iter()
                .filter(|job| !job.running && !job.cleared)
                .count(),
        }
    }

    /// Entries hidden by the latest clear or the finished limit, offered by "show all".
    /// 被最近一次清理或结束数量上限隐藏、由“显示全部”提供的条目数。
    pub fn hidden(&self) -> usize {
        match self.page {
            ActivityPage::Subagents => self.subagents.as_ref().map_or(0, |s| s.cleared_count),
            ActivityPage::Commands => self.jobs.as_ref().map_or(0, |list| {
                let shown = list.jobs.iter().filter(|job| !job.running).count();
                list.finished_count.saturating_sub(shown)
            }),
        }
    }
}

/// The saved session the activity view belongs to; drafts have no activity.
/// 活动视图所属的已保存会话；草稿没有活动。
pub fn saved_session(app: &App) -> Option<String> {
    app.selected.clone().filter(|id| !id.starts_with("draft:"))
}

/// The page shown on screen, if the activity tab is open.
/// 活动标签页打开时屏幕上显示的页面。
pub fn visible_page(app: &App) -> Option<ActivityPage> {
    (app.area == Area::Chat
        && app.overlay.is_none()
        && app.panel_visible
        && app.active_panel_tab() == PanelTab::Agents)
        .then_some(app.activity.page)
}

/// The subagent row of the navigation snapshot matching one activity entry, which carries the
/// stop, end and detail controls.
/// 与一个活动条目匹配的导航快照子代理行，其带有停止、结束与详情控制。
pub fn agent_for<'a>(app: &'a App, entry: &SubagentActivity) -> Option<&'a AgentInfo> {
    app.session()?
        .agents
        .iter()
        .find(|agent| agent.id == entry.task.task_id)
}

/// Sends a list read when the wanted request differs from the one last sent.
/// 所需请求与最近一次发送的请求不同时发送列表读取。
fn send_if_new(app: &mut App, call: ServiceCall) {
    let CallKind::Load(slot) = call.kind else {
        return;
    };
    if app.activity.requested.get(&slot) == Some(&call.params) {
        return;
    }
    app.activity.requested.insert(slot, call.params.clone());
    action::perform(app, UiAction::Send(Command::Call(call)));
}

/// Keeps the lists of the selected session loaded; called every tick. The command list is always
/// read for the session bar count, the subagent snapshot only while its page is shown.
/// 保持所选会话的列表已加载；每次计时调用。命令列表始终读取以供会话栏计数，子代理快照仅在其页面
/// 显示时读取。
pub fn ensure_loaded(app: &mut App) {
    if app.connection != ConnectionState::Connected {
        return;
    }
    let session = saved_session(app);
    if app.activity.session_id != session {
        app.activity.session_id = session.clone();
        app.activity.jobs = None;
        app.activity.subagents = None;
        app.activity.requested.clear();
        for slot in [Slot::ShellJobs, Slot::SessionActivity] {
            app.live_poll_due.remove(&slot);
            app.slot_errors.remove(&slot);
        }
    }
    let Some(session) = session else {
        return;
    };
    let visible = visible_page(app);
    if visible != app.activity.visible {
        // A page that becomes visible is read again, as the WebUI effect re-runs on visibility.
        // 变为可见的页面重新读取，与 WebUI 的副作用随可见性重新运行一致。
        if let Some(page) = visible {
            app.activity.requested.remove(&page.slot());
        }
        app.activity.visible = visible;
    }
    let all = app.activity.show_all_commands;
    send_if_new(
        app,
        api::list_shell_jobs(&session, all, (!all).then_some(FINISHED_JOB_LIMIT)),
    );
    if visible == Some(ActivityPage::Subagents) {
        let all = app.activity.show_all_subagents;
        send_if_new(app, api::load_session_activity(&session, all));
    }
}

/// Whether a due live re-read of `slot` should be sent: its page is on screen.
/// `slot` 到期的实时重读是否应发送：其页面正在显示。
pub fn live_slot_visible(app: &App, slot: Slot) -> bool {
    visible_page(app).is_some_and(|page| page.slot() == slot)
}

/// Re-read interval of an activity list that just arrived, while one of its entries runs.
/// 刚到达的活动列表中有条目运行时的重读间隔。
pub fn live_interval(app: &App, slot: Slot) -> Option<Duration> {
    let running = match slot {
        Slot::ShellJobs => app
            .activity
            .jobs
            .as_ref()
            .is_some_and(|l| l.running_count > 0),
        Slot::SessionActivity => app
            .activity
            .subagent_entries()
            .iter()
            .any(|entry| entry.task.is_running),
        _ => false,
    };
    running.then_some(LIVE_REFRESH_INTERVAL)
}

/// Whether a result for an activity slot still belongs to what is shown; `None` for other slots.
/// 活动槽位的结果是否仍属于当前显示的内容；其他槽位返回 `None`。
pub fn slot_is_current(app: &App, slot: Slot, call: &ServiceCall) -> Option<bool> {
    let log_matches = || match &app.overlay {
        Some(Overlay::ShellJobLog(log)) => {
            call.param("/sessionId").as_str() == Some(log.session_id.as_str())
                && call.param("/jobId").as_str() == Some(log.job_id.as_str())
        }
        _ => false,
    };
    Some(match slot {
        Slot::ShellJobs | Slot::SessionActivity => {
            call.param("/sessionId").as_str() == app.activity.session_id.as_deref()
        }
        Slot::ShellJobOutput | Slot::ShellJobOutputEarlier => log_matches(),
        Slot::RunningShellJobCount => app.activity.restart_pending,
        _ => return None,
    })
}

/// Folds an arrived activity result into the typed state; `call` is its request.
/// 将到达的活动结果折叠进类型化状态；`call` 为其请求。
pub fn slot_arrived(app: &mut App, slot: Slot, call: &ServiceCall) {
    let value = app.slots.value(slot).clone();
    match slot {
        Slot::ShellJobs => match serde_json::from_value::<ShellJobList>(value) {
            Ok(list) => {
                app.activity
                    .background
                    .insert(list.session_id.clone(), list.background_running_count);
                app.activity.jobs = Some(list);
            }
            Err(error) => {
                app.slot_errors.insert(slot, error.to_string());
            }
        },
        Slot::SessionActivity => match serde_json::from_value::<SessionActivitySnapshot>(value) {
            Ok(snapshot) => app.activity.subagents = Some(snapshot),
            Err(error) => {
                app.slot_errors.insert(slot, error.to_string());
            }
        },
        Slot::ShellJobOutput | Slot::ShellJobOutputEarlier => {
            let now = app.now;
            let Some(Overlay::ShellJobLog(log)) = app.overlay.as_mut() else {
                return;
            };
            match serde_json::from_value::<ShellJobOutputPage>(value) {
                Ok(page) if slot == Slot::ShellJobOutput => {
                    log.follow_pending = false;
                    log.tail_arrived(call.param("/fromLine").as_u64(), page, now);
                }
                Ok(page) => {
                    log.earlier_pending = false;
                    log.earlier_arrived(call.param("/beforeLine").as_u64(), page);
                }
                Err(error) => {
                    log.follow_pending = false;
                    log.earlier_pending = false;
                    log.error = Some(error.to_string());
                }
            }
        }
        Slot::RunningShellJobCount => {
            app.activity.restart_pending = false;
            restart_after_count(app, value.as_u64().unwrap_or(0));
        }
        _ => {}
    }
}

/// Records a failed activity read; `message` is the service error.
/// 记录失败的活动读取；`message` 为服务错误。
pub fn slot_failed(app: &mut App, slot: Slot, message: &str) {
    match slot {
        Slot::ShellJobOutput | Slot::ShellJobOutputEarlier => {
            if let Some(Overlay::ShellJobLog(log)) = app.overlay.as_mut() {
                log.follow_pending = false;
                log.earlier_pending = false;
                log.error = Some(message.to_string());
            }
        }
        // The count is a courtesy; a failed count must not block an update.
        // 该计数只是提醒；计数失败不得阻止更新。
        Slot::RunningShellJobCount => {
            app.activity.restart_pending = false;
            restart_after_count(app, 0);
        }
        _ => {}
    }
}

/// A `shellJobChanged` notice: updates the count and re-reads what shows the job.
/// `shellJobChanged` 通知：更新计数并重读显示该任务的内容。
pub fn job_changed(app: &mut App, changed: ShellJobChangedEvent) {
    app.activity
        .background
        .insert(changed.session_id.clone(), changed.background_running_count);
    if app
        .activity
        .session_id
        .as_deref()
        .is_some_and(|id| id.eq_ignore_ascii_case(&changed.session_id))
    {
        app.activity.requested.remove(&Slot::ShellJobs);
    }
    let now = app.now;
    if let Some(Overlay::ShellJobLog(log)) = app.overlay.as_mut()
        && log.session_id.eq_ignore_ascii_case(&changed.session_id)
        && log.job_id == changed.job_id
        && !log.follow_pending
    {
        log.follow_due = Some(now);
    }
}

/// The subagent navigation of a session changed; its snapshot is read again when shown.
/// 某会话的子代理导航已变化；显示时重新读取其快照。
pub fn subagents_changed(app: &mut App, conversation_id: &str) {
    if app.activity.session_id.as_deref() == Some(conversation_id) {
        app.activity.requested.remove(&Slot::SessionActivity);
    }
}

/// The connection came back; every list is read again.
/// 连接已恢复；重新读取所有列表。
pub fn reconnected(app: &mut App) {
    app.activity.requested.clear();
}

/// Sends the due follow read of the open log; called every tick.
/// 发送所打开日志到期的跟随读取；每次计时调用。
pub fn tick(app: &mut App) {
    ensure_loaded(app);
    let now = app.now;
    let Some(Overlay::ShellJobLog(log)) = app.overlay.as_mut() else {
        return;
    };
    if log.follow_pending || log.follow_due.is_none_or(|due| due > now) {
        return;
    }
    log.follow_due = None;
    log.follow_pending = true;
    let call = api::read_shell_job_output(
        &log.session_id,
        &log.job_id,
        ShellJobOutputQuery {
            from_line: last_text_line(&log.rows),
            max_lines: Some(LOG_PAGE_LINES),
            ..ShellJobOutputQuery::default()
        },
    );
    action::perform(app, UiAction::Send(Command::Call(call)));
}

/// Runs one activity action.
/// 执行一个活动操作。
pub fn perform(app: &mut App, activity: ActivityAction) {
    match activity {
        ActivityAction::Show(page) => {
            action::perform(app, UiAction::ShowPanel(PanelTab::Agents));
            perform(app, ActivityAction::Page(page));
        }
        ActivityAction::Page(page) => {
            if app.activity.page != page {
                app.activity.page = page;
                app.panel.cursor.insert(PanelTab::Agents, 0);
                app.panel.scroll.insert(PanelTab::Agents, 0);
            }
        }
        ActivityAction::ShowAll(all) => {
            match app.activity.page {
                ActivityPage::Subagents => app.activity.show_all_subagents = all,
                ActivityPage::Commands => app.activity.show_all_commands = all,
            }
            ensure_loaded(app);
        }
        ActivityAction::Clear => {
            let Some(session) = app.activity.session_id.clone() else {
                return;
            };
            if app.activity.clearable() == 0 {
                return;
            }
            let scope = match app.activity.page {
                ActivityPage::Subagents => SessionActivityScope::Subagents,
                ActivityPage::Commands => SessionActivityScope::Commands,
            };
            action::perform(
                app,
                UiAction::Send(Command::Call(api::clear_session_activity(&session, scope))),
            );
        }
        ActivityAction::StopJob(job_id) => {
            let Some(session) = app.activity.session_id.clone() else {
                return;
            };
            action::perform(
                app,
                UiAction::Send(Command::Call(api::stop_shell_job(&session, &job_id))),
            );
        }
        ActivityAction::OpenLog(job_id) => open_log(app, &job_id),
        ActivityAction::LogEarlier => {
            let Some(Overlay::ShellJobLog(log)) = app.overlay.as_mut() else {
                return;
            };
            let Some(before_line) = log.earlier_boundary() else {
                return;
            };
            if log.earlier_pending || !log.has_earlier {
                return;
            }
            // Reading upward leaves the newest lines, as scrolling to the top does.
            // 向上读取会离开最新的行，与滚动到顶部一致。
            log.follow = false;
            log.earlier_pending = true;
            let call = api::read_shell_job_output(
                &log.session_id,
                &log.job_id,
                ShellJobOutputQuery {
                    before_line: Some(before_line),
                    max_lines: Some(LOG_PAGE_LINES),
                    ..ShellJobOutputQuery::default()
                },
            );
            action::perform(app, UiAction::Send(Command::Call(call)));
        }
    }
}

/// Opens the log of a listed job, starting from its newest lines.
/// 打开一个已列出任务的日志，从其最新的行开始。
fn open_log(app: &mut App, job_id: &str) {
    let (Some(session_id), Some(job)) = (
        app.activity.session_id.clone(),
        app.activity
            .job_entries()
            .iter()
            .find(|job| job.job_id == job_id)
            .cloned(),
    ) else {
        return;
    };
    if !job.log_available {
        return;
    }
    let call = api::read_shell_job_output(
        &session_id,
        job_id,
        ShellJobOutputQuery {
            max_lines: Some(LOG_PAGE_LINES),
            ..ShellJobOutputQuery::default()
        },
    );
    app.overlay = Some(Overlay::ShellJobLog(Box::new(ShellJobLog::new(
        session_id, &job,
    ))));
    action::perform(app, UiAction::Send(Command::Call(call)));
}

/// Restarts to update, first counting the running background commands that a restart stops.
/// 重启以更新，先统计重启会停止的运行中后台命令。
pub fn restart_to_update(app: &mut App) {
    if app.activity.restart_pending {
        return;
    }
    app.activity.restart_pending = true;
    action::perform(
        app,
        UiAction::Send(Command::Call(api::count_running_shell_jobs())),
    );
}

/// Restarts now with nothing running, else asks first how many commands will stop.
/// 没有运行中的命令时立即重启，否则先告知将停止的命令数并请求确认。
fn restart_after_count(app: &mut App, running: u64) {
    let restart = UiAction::Send(Command::Call(
        crate::service_api::app_update::restart_to_update(),
    ));
    if running == 0 {
        action::perform(app, restart);
        return;
    }
    action::perform(
        app,
        action::confirm(
            tr!("settings.appUpdate.restart"),
            tr!("appUpdate.toast.backgroundJobs", count = running),
            tr!("appUpdate.toast.restartAnyway"),
            restart,
        ),
    );
}

/// Label of a job status word; unknown words read as lost, as in the WebUI.
/// 任务状态词的标签；未知取值按丢失显示，与 WebUI 一致。
pub fn job_status_label(status: &str) -> String {
    match status {
        "running" => tr!("sessionActivity.status.running"),
        "completed" => tr!("sessionActivity.status.completed"),
        "failed" => tr!("sessionActivity.status.failed"),
        "timed_out" => tr!("sessionActivity.status.timedOut"),
        "stopped" => tr!("sessionActivity.status.stopped"),
        _ => tr!("sessionActivity.status.lost"),
    }
}

/// The line number of the last output row, or `None` while no row is shown.
/// 最后一条输出行的行号；没有显示行时为 `None`。
pub fn last_text_line(rows: &[ShellJobOutputRow]) -> Option<u64> {
    rows.iter()
        .rev()
        .find(|row| row.kind == "text")
        .map(|row| row.line)
}

/// The open log of one job, bounded to `LOG_RETAINED_ROWS` rows.
/// 一个任务的已打开日志，以 `LOG_RETAINED_ROWS` 行为界。
#[derive(Debug, Clone)]
pub struct ShellJobLog {
    pub session_id: String,
    pub job_id: String,
    /// Purpose of the job, else its command.
    /// 任务用途，否则为其命令。
    pub title: String,
    pub command: String,
    pub rows: Vec<ShellJobOutputRow>,
    /// The service holds lines above the first row kept.
    /// 服务在保留的首行之上还有更多行。
    pub has_earlier: bool,
    pub running: bool,
    /// The first page arrived.
    /// 第一页已到达。
    pub loaded: bool,
    pub error: Option<String>,
    /// First row shown.
    /// 显示的第一行。
    pub top: usize,
    /// Largest `top` that still fills the view; the renderer keeps it current.
    /// 仍能填满视图的最大 `top`；由渲染器保持最新。
    pub max_top: usize,
    /// The view stays on the newest rows.
    /// 视图停留在最新的行。
    pub follow: bool,
    /// When the next follow read is due.
    /// 下一次跟随读取的到期时间。
    pub follow_due: Option<Instant>,
    follow_pending: bool,
    earlier_pending: bool,
}

impl ShellJobLog {
    /// An empty log of `job`; the first page is requested by the opener.
    /// `job` 的空日志；第一页由打开方请求。
    pub fn new(session_id: String, job: &ShellJobView) -> Self {
        let title = match job.purpose.trim() {
            "" => job.command.clone(),
            purpose => purpose.to_string(),
        };
        ShellJobLog {
            session_id,
            job_id: job.job_id.clone(),
            title,
            command: job.command.clone(),
            rows: Vec::new(),
            has_earlier: false,
            running: job.running,
            loaded: false,
            error: None,
            top: 0,
            max_top: 0,
            follow: true,
            follow_due: None,
            follow_pending: true,
            earlier_pending: false,
        }
    }

    /// Whether an earlier page is being read.
    /// 是否正在读取更早的一页。
    pub fn earlier_pending(&self) -> bool {
        self.earlier_pending
    }

    /// `beforeLine` of the page above the first row: a first row that continues a line restarts
    /// from that line, so the line is shown whole.
    /// 首行之上一页的 `beforeLine`：首行是某行的续行时从该行重新开始，使该行完整显示。
    fn earlier_boundary(&self) -> Option<u64> {
        let first = self.rows.first()?;
        Some(if first.continuation {
            first.line + 1
        } else {
            first.line
        })
    }

    /// Folds a tail page in. Without `from_line` it is the newest page and replaces the rows;
    /// with it the rows from that line on are replaced, since the last line may have grown.
    /// 折叠一页末尾内容。没有 `from_line` 时为最新的一页并替换全部行；有时替换从该行开始的行，
    /// 因为最后一行可能仍在增长。
    pub fn tail_arrived(&mut self, from_line: Option<u64>, page: ShellJobOutputPage, now: Instant) {
        match from_line {
            None => {
                self.rows = page.rows;
                self.has_earlier = page.has_earlier;
                self.top = 0;
            }
            Some(from_line) => {
                if last_text_line(&self.rows) != Some(from_line) {
                    // The rows moved since the request (an earlier page); read again from the
                    // current last line.
                    // 请求之后行已变化（读取了更早的一页）；从当前最后一行重新读取。
                    self.follow_due = Some(now);
                    return;
                }
                self.rows.retain(|row| row.line < from_line);
                self.rows.extend(page.rows);
                if self.rows.len() > LOG_RETAINED_ROWS {
                    let dropped = self.rows.len() - LOG_RETAINED_ROWS;
                    self.rows.drain(..dropped);
                    self.top = self.top.saturating_sub(dropped);
                    self.has_earlier = true;
                }
            }
        }
        self.loaded = true;
        self.error = None;
        self.running = page.running;
        self.follow_due = if page.next_line.is_some() {
            Some(now)
        } else if page.running {
            Some(now + LOG_FOLLOW_INTERVAL)
        } else {
            None
        };
    }

    /// Prepends the page above the first row, keeping the rows in view still. Rows dropped from
    /// the bottom are read again by the follower, which stops following meanwhile.
    /// 在首行之上插入一页，并保持视图中的行不动。从底部丢弃的行会由跟随读取再次读取，其间停止跟随。
    pub fn earlier_arrived(&mut self, before_line: Option<u64>, page: ShellJobOutputPage) {
        if before_line.is_none() || before_line != self.earlier_boundary() {
            return;
        }
        let first = self.rows[0].clone();
        let removed = if first.continuation {
            let removed = self
                .rows
                .iter()
                .take_while(|row| row.line == first.line)
                .count();
            self.rows.retain(|row| row.line != first.line);
            removed
        } else {
            0
        };
        let added = page.rows.len();
        let mut merged = page.rows;
        merged.append(&mut self.rows);
        if merged.len() > LOG_RETAINED_ROWS {
            merged.truncate(LOG_RETAINED_ROWS);
            self.follow = false;
        }
        self.rows = merged;
        self.has_earlier = page.has_earlier;
        self.top =
            (added + self.top.saturating_sub(removed)).min(self.rows.len().saturating_sub(1));
    }

    /// Moves the view by `delta` rows; leaving the bottom stops following, reaching it resumes.
    /// Returns whether scrolling up reached the top, where an earlier page is read.
    /// 将视图移动 `delta` 行；离开底部停止跟随，回到底部恢复跟随。返回向上滚动是否到达顶部，此时
    /// 读取更早的一页。
    pub fn scroll_by(&mut self, delta: isize) -> bool {
        let top = if self.follow { self.max_top } else { self.top };
        self.top = top.saturating_add_signed(delta).min(self.max_top);
        self.follow = self.top >= self.max_top;
        self.top == 0 && delta < 0
    }

    /// Jumps to the oldest kept row (`true`) or to the newest and follows it.
    /// 跳到最早保留的行（`true`），或跳到最新的行并跟随。
    pub fn jump(&mut self, oldest: bool) {
        if oldest {
            self.top = 0;
            self.follow = self.max_top == 0;
        } else {
            self.top = self.max_top;
            self.follow = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(line: u64, continuation: bool) -> ShellJobOutputRow {
        ShellJobOutputRow {
            line,
            kind: "text".into(),
            continuation,
            text: format!("line {line}"),
        }
    }

    fn page(rows: Vec<ShellJobOutputRow>, has_earlier: bool, running: bool) -> ShellJobOutputPage {
        ShellJobOutputPage {
            job_id: "job-1".into(),
            total_lines: rows.last().map_or(0, |r| r.line),
            rows,
            has_earlier,
            next_line: None,
            running,
        }
    }

    fn log() -> ShellJobLog {
        let job: ShellJobView = serde_json::from_value(serde_json::json!({
            "jobId": "job-1", "command": "npm run dev", "purpose": "", "shell": "bash", "cwd": ".",
            "startedAtMs": 0, "background": true, "running": true, "status": "running",
            "durationMs": 0, "totalLines": 0, "totalBytes": 0, "logAvailable": true
        }))
        .expect("job view");
        ShellJobLog::new("s1".into(), &job)
    }

    /// The follower replaces the possibly growing last line, keeps the bound and keeps the view.
    /// 跟随读取替换可能仍在增长的最后一行，保持行数上限并保持视图位置。
    #[test]
    fn follow_replaces_the_last_line_and_keeps_the_bound() {
        let now = Instant::now();
        let mut log = log();
        log.tail_arrived(
            None,
            page((1..=3).map(|l| row(l, false)).collect(), false, true),
            now,
        );
        assert_eq!(log.follow_due, Some(now + LOG_FOLLOW_INTERVAL));
        let mut grown = page((3..=5).map(|l| row(l, false)).collect(), false, true);
        grown.next_line = Some(6);
        log.tail_arrived(Some(3), grown, now);
        let lines: Vec<u64> = log.rows.iter().map(|r| r.line).collect();
        assert_eq!(
            lines,
            vec![1, 2, 3, 4, 5],
            "line 3 is replaced, not repeated"
        );
        assert_eq!(log.follow_due, Some(now), "more lines are read at once");

        log.top = 3;
        let flood = (5..5 + LOG_RETAINED_ROWS as u64)
            .map(|l| row(l, false))
            .collect();
        log.tail_arrived(Some(5), page(flood, false, false), now);
        assert_eq!(log.rows.len(), LOG_RETAINED_ROWS);
        assert!(log.has_earlier, "dropped rows are offered again above");
        assert_eq!(log.top, 0, "the view follows the dropped rows");
        assert_eq!(log.follow_due, None, "a finished job is no longer followed");

        // A follow page requested before the rows changed is not merged.
        // 在行变化之前请求的跟随页不会被合并。
        let before = log.rows.clone();
        log.tail_arrived(Some(2), page(vec![row(9, false)], false, true), now);
        assert_eq!(log.rows, before);
        assert_eq!(log.follow_due, Some(now));
    }

    /// Earlier pages restart a continued first line, keep the view still and stay bounded.
    /// 更早的页从续行首行所在行重新开始，保持视图不动并保持有界。
    #[test]
    fn earlier_pages_restart_split_lines_and_stay_bounded() {
        let now = Instant::now();
        let mut log = log();
        log.tail_arrived(
            None,
            page(
                vec![row(10, true), row(11, false), row(12, false)],
                true,
                false,
            ),
            now,
        );
        assert_eq!(
            log.earlier_boundary(),
            Some(11),
            "a continued line is read whole"
        );
        log.top = 1;
        log.earlier_arrived(
            Some(11),
            page(
                vec![row(9, false), row(10, false), row(10, true)],
                true,
                false,
            ),
        );
        let lines: Vec<u64> = log.rows.iter().map(|r| r.line).collect();
        assert_eq!(lines, vec![9, 10, 10, 11, 12]);
        assert_eq!(log.rows[log.top].line, 11, "the row in view stays in view");

        // A stale boundary is ignored.
        // 过期的边界被忽略。
        let before = log.rows.clone();
        log.earlier_arrived(Some(3), page(vec![row(1, false)], false, false));
        assert_eq!(log.rows, before);

        let huge = (1..=LOG_RETAINED_ROWS as u64)
            .map(|l| row(l, false))
            .collect();
        log.earlier_arrived(Some(9), page(huge, false, false));
        assert_eq!(log.rows.len(), LOG_RETAINED_ROWS);
        assert!(!log.follow, "rows dropped from the bottom stop following");
        assert!(!log.has_earlier);
    }

    /// Scrolling up leaves the bottom and reports the top; scrolling back down follows again.
    /// 向上滚动离开底部并报告到达顶部；向下滚回底部后再次跟随。
    #[test]
    fn scrolling_toggles_following_and_reports_the_top() {
        let mut log = log();
        log.max_top = 2;
        assert!(log.follow);
        assert!(!log.scroll_by(-1));
        assert!(!log.follow);
        assert!(log.scroll_by(-1), "reaching the top asks for earlier rows");
        assert!(log.scroll_by(-1), "so does scrolling up at the top");
        assert!(!log.scroll_by(1));
        log.scroll_by(5);
        assert!(log.follow);
        log.jump(true);
        assert_eq!((log.top, log.follow), (0, false));
        log.jump(false);
        assert_eq!((log.top, log.follow), (2, true));
    }
}
