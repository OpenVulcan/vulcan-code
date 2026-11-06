// Mock session activity: background shell jobs with paged logs and the subagent snapshot of the
// demo conversation, answering the same RPCs as the service's session activity routes.
// 模拟会话活动：演示会话的后台 shell 任务（带分页日志）与子代理快照，响应与服务会话活动路由相同的 RPC。

use std::collections::{HashMap, HashSet};

use serde_json::Value;
use vulcan_service_api::{
    ClearSessionActivityRequest, SessionActivityRequest, SessionActivityScope,
    SessionActivitySnapshot, ShellJobChangedEvent, ShellJobList, ShellJobListRequest,
    ShellJobOutputPage, ShellJobOutputRequest, ShellJobOutputRow, ShellJobTargetRequest,
    ShellJobView, SubagentActivity, SubagentActivityTool, SubagentTaskSummary,
};

use super::ConversationLog;
use crate::backend::BackendEvent;
use crate::model::{AgentState, now_unix_ms};
use crate::service_api::ServiceCall;

/// The conversation whose jobs are seeded.
/// 预置任务的会话。
const DEMO_SESSION: &str = "c-tui";

/// Display width at which one log line is split into continuation rows.
/// 一条日志行被拆分为续行的显示宽度。
const ROW_WIDTH: usize = 160;

/// Latest lines a running job reports in the list.
/// 运行中任务在列表中报告的最近行数。
const RECENT_LINES: usize = 3;

/// One simulated job and its whole log.
/// 一个模拟任务及其完整日志。
struct MockJob {
    view: ShellJobView,
    lines: Vec<String>,
}

/// Session activity of the mock backend.
/// 模拟后端的会话活动。
#[derive(Default)]
pub(super) struct MockActivity {
    /// Jobs per session, newest last.
    /// 每个会话的任务，最新的在后。
    jobs: HashMap<String, Vec<MockJob>>,
    /// Sessions whose jobs were seeded.
    /// 已预置任务的会话。
    seeded: HashSet<String>,
    /// Subagent task ids hidden by a clear, per session.
    /// 每个会话中被清理隐藏的子代理任务标识。
    cleared_agents: HashMap<String, HashSet<String>>,
    /// Events produced by the last answer, emitted by the world.
    /// 最近一次回答产生的事件，由模拟世界发出。
    events: Vec<BackendEvent>,
}

/// Deterministic log line `line` (one-based) of a job; line 120 is long enough to wrap.
/// 任务的确定性日志第 `line` 行（从 1 开始）；第 120 行足够长而会被折行。
fn log_line(command: &str, line: usize) -> String {
    if line == 120 {
        return format!("[{line:04}] {command}: {}", "long-output ".repeat(30));
    }
    format!("[{line:04}] {command}: step {line} ok")
}

/// One seeded job.
/// 一个预置任务。
fn job(index: usize, command: &str, purpose: &str, status: &str, lines: usize) -> MockJob {
    let now = now_unix_ms().max(0) as u64;
    let running = status == "running";
    let started = now.saturating_sub(60_000 * (index as u64 + 1));
    let duration_ms = if running {
        now - started
    } else {
        5_000 * index as u64
    };
    let lines: Vec<String> = (1..=lines).map(|line| log_line(command, line)).collect();
    MockJob {
        view: ShellJobView {
            job_id: format!("job-{index}"),
            command: command.into(),
            purpose: purpose.into(),
            shell: "bash".into(),
            cwd: "D:/demo/vulcan-code".into(),
            started_at_ms: started,
            api_call_id: None,
            tool_call_id: None,
            background: index == 1 || running,
            running,
            status: status.into(),
            exit_code: match status {
                "completed" => Some(0),
                "failed" => Some(101),
                _ => None,
            },
            duration_ms,
            finished_at_ms: (!running).then_some(started + duration_ms),
            failure_reason: (status == "timed_out").then(|| "Timed out after 600s".into()),
            total_lines: lines.len() as u64,
            total_bytes: lines.iter().map(|line| line.len() as u64 + 1).sum(),
            log_available: true,
            last_output_at_ms: Some(started + duration_ms),
            recent_lines: Vec::new(),
            cleared: false,
        },
        lines,
    }
}

/// The demo conversation's jobs: one running background server and seven finished jobs, two more
/// than the recent list shows.
/// 演示会话的任务：一个运行中的后台服务与七个已结束任务，比最近列表多出两个。
fn seed_jobs() -> Vec<MockJob> {
    vec![
        job(1, "npm run dev", "Start the dev server", "running", 480),
        job(2, "cargo test -p vulcan-code-tui", "", "completed", 64),
        job(
            3,
            "cargo clippy --all-targets",
            "Lint the TUI",
            "failed",
            40,
        ),
        job(4, "pnpm run build", "", "completed", 25),
        job(5, "npm run e2e", "End-to-end tests", "timed_out", 300),
        job(6, "git fetch --all", "", "completed", 3),
        job(7, "cargo bench", "", "stopped", 12),
        job(8, "make sync", "", "completed", 0),
    ]
}

/// Rows of the log lines `from..=to` (one-based); long lines continue on following rows.
/// 日志行 `from..=to`（从 1 开始）的显示行；长行在后续行中续写。
fn rows(lines: &[String], from: usize, to: usize) -> Vec<ShellJobOutputRow> {
    let mut rows = Vec::new();
    for line in from..=to {
        let text = &lines[line - 1];
        let chars: Vec<char> = text.chars().collect();
        for (part, chunk) in chars.chunks(ROW_WIDTH).enumerate() {
            rows.push(ShellJobOutputRow {
                line: line as u64,
                kind: "text".into(),
                continuation: part > 0,
                text: chunk.iter().collect(),
            });
        }
    }
    rows
}

/// The service's run state word of a subagent state.
/// 子代理状态对应的服务运行状态词。
fn run_state(state: AgentState) -> &'static str {
    match state {
        AgentState::Running => "running",
        AgentState::Standby => "completed",
        AgentState::Stopped => "stopped",
        AgentState::Failed => "failed",
        AgentState::Closed => "closed",
    }
}

impl MockActivity {
    /// Events produced since the last call, oldest first.
    /// 自上次调用以来产生的事件，最早的在前。
    pub(super) fn take_events(&mut self) -> Vec<BackendEvent> {
        std::mem::take(&mut self.events)
    }

    /// Jobs of a session, seeding the demo conversation on first use.
    /// 某会话的任务；首次使用时为演示会话预置。
    fn jobs(&mut self, session_id: &str) -> &mut Vec<MockJob> {
        if session_id == DEMO_SESSION && self.seeded.insert(session_id.to_string()) {
            self.jobs.insert(session_id.to_string(), seed_jobs());
        }
        self.jobs.entry(session_id.to_string()).or_default()
    }

    /// Answers a session activity RPC; `None` for every other method.
    /// 响应会话活动 RPC；其他方法返回 `None`。
    pub(super) fn answer(
        &mut self,
        call: &ServiceCall,
        logs: &HashMap<String, ConversationLog>,
    ) -> Option<Result<Value, String>> {
        let result = match call.method {
            "service_list_shell_jobs" => parse(call).map(|request| self.list(request)),
            "service_read_shell_job_output" => parse(call).and_then(|request| self.output(request)),
            "service_stop_shell_job" => parse(call).and_then(|request| self.stop(request)),
            "service_session_activity" => parse(call).map(|request| self.snapshot(request, logs)),
            "service_clear_session_activity" => {
                parse(call).map(|request| self.clear(request, logs))
            }
            "service_running_shell_job_count" => Ok(Value::from(
                self.jobs
                    .values()
                    .flatten()
                    .filter(|job| job.view.running && job.view.background)
                    .count(),
            )),
            _ => return None,
        };
        Some(result)
    }

    fn list(&mut self, request: ShellJobListRequest) -> Value {
        let jobs = self.jobs(&request.session_id);
        let running_count = jobs.iter().filter(|job| job.view.running).count();
        let background_running_count = jobs
            .iter()
            .filter(|job| job.view.running && job.view.background)
            .count();
        let cleared_count = jobs.iter().filter(|job| job.view.cleared).count();
        let visible = |job: &&MockJob| request.include_cleared || !job.view.cleared;
        let mut views: Vec<ShellJobView> = jobs
            .iter()
            .filter(|job| job.view.running)
            .filter(visible)
            .map(|job| {
                let mut view = job.view.clone();
                let start = job.lines.len().saturating_sub(RECENT_LINES);
                view.recent_lines = job.lines[start..].to_vec();
                view
            })
            .collect();
        let finished: Vec<ShellJobView> = jobs
            .iter()
            .rev()
            .filter(|job| !job.view.running)
            .filter(visible)
            .map(|job| job.view.clone())
            .collect();
        let finished_count = finished.len();
        views.extend(
            finished
                .into_iter()
                .take(request.finished_limit.unwrap_or(usize::MAX)),
        );
        to_value(&ShellJobList {
            session_id: request.session_id,
            jobs: views,
            running_count,
            background_running_count,
            finished_count,
            cleared_count,
            recent_line_limit: RECENT_LINES,
        })
    }

    fn output(&mut self, request: ShellJobOutputRequest) -> Result<Value, String> {
        let job = self
            .jobs(&request.session_id)
            .iter()
            .find(|job| job.view.job_id == request.job_id)
            .ok_or_else(|| format!("Unknown shell job {}.", request.job_id))?;
        let total = job.lines.len();
        let max = request.max_lines.unwrap_or(200).max(1);
        let (from, to) = match (request.before_line, request.from_line) {
            (Some(before), _) => {
                let to = (before as usize).saturating_sub(1).min(total);
                (to.saturating_sub(max - 1).max(1), to)
            }
            (None, Some(from)) => {
                let from = (from as usize).max(1);
                (from, (from + max - 1).min(total))
            }
            (None, None) => (total.saturating_sub(max - 1).max(1), total),
        };
        let rows = if total == 0 || from > to {
            Vec::new()
        } else {
            rows(&job.lines, from, to)
        };
        Ok(to_value(&ShellJobOutputPage {
            job_id: request.job_id,
            rows,
            has_earlier: total > 0 && from > 1,
            next_line: (to < total && request.from_line.is_some()).then_some(to as u64 + 1),
            total_lines: total as u64,
            running: job.view.running,
        }))
    }

    fn stop(&mut self, request: ShellJobTargetRequest) -> Result<Value, String> {
        let jobs = self.jobs(&request.session_id);
        let job = jobs
            .iter_mut()
            .find(|job| job.view.job_id == request.job_id)
            .ok_or_else(|| format!("Unknown shell job {}.", request.job_id))?;
        if job.view.running {
            job.view.running = false;
            job.view.status = "stopped".into();
            job.view.finished_at_ms = Some(now_unix_ms().max(0) as u64);
        }
        let running_count = jobs.iter().filter(|job| job.view.running).count();
        let background_running_count = jobs
            .iter()
            .filter(|job| job.view.running && job.view.background)
            .count();
        self.events
            .push(BackendEvent::ShellJobChanged(ShellJobChangedEvent {
                session_id: request.session_id,
                job_id: request.job_id,
                change: "finished".into(),
                running_count,
                background_running_count,
            }));
        Ok(Value::Null)
    }

    fn snapshot(
        &mut self,
        request: SessionActivityRequest,
        logs: &HashMap<String, ConversationLog>,
    ) -> Value {
        let cleared = self
            .cleared_agents
            .get(&request.session_id)
            .cloned()
            .unwrap_or_default();
        let agents = logs
            .get(&request.session_id)
            .map(|log| log.agents.as_slice())
            .unwrap_or_default();
        let mut subagents = Vec::new();
        let mut cleared_count = 0;
        for (index, agent) in agents.iter().enumerate() {
            let is_running = agent.state == AgentState::Running;
            let entry_cleared = !is_running && cleared.contains(&agent.id);
            if entry_cleared {
                cleared_count += 1;
                if !request.include_cleared {
                    continue;
                }
            }
            let start = agent.activity.len().saturating_sub(3);
            let recent_tools = agent.activity[start..]
                .iter()
                .enumerate()
                .map(|(step, activity)| {
                    let (name, summary) = activity.split_once(' ').unwrap_or((activity, ""));
                    let last = start + step + 1 == agent.activity.len();
                    SubagentActivityTool {
                        tool_name: name.into(),
                        summary: (!summary.is_empty()).then(|| summary.into()),
                        status: if last && is_running {
                            "running".into()
                        } else {
                            "completed".into()
                        },
                    }
                })
                .collect();
            subagents.push(SubagentActivity {
                task: SubagentTaskSummary {
                    pending_messages: Vec::new(),
                    task_id: agent.id.clone(),
                    public_task_id: format!("task-{}", index + 1),
                    label: agent.name.clone(),
                    description: (!agent.task.is_empty()).then(|| agent.task.clone()),
                    agent_profile_id: (!agent.profile.is_empty()).then(|| agent.profile.clone()),
                    run_state: run_state(agent.state).into(),
                    is_running,
                    is_active: agent.state != AgentState::Closed,
                },
                created_at_ms: 0,
                updated_at_ms: index as u64,
                cleared: entry_cleared,
                recent_tools,
                recent_output: agent
                    .result
                    .iter()
                    .flat_map(|result| result.lines())
                    .take(2)
                    .map(str::to_string)
                    .collect(),
            });
        }
        subagents.sort_by_key(|entry| !entry.task.is_running);
        to_value(&SessionActivitySnapshot {
            session_id: request.session_id,
            subagents,
            cleared_count,
        })
    }

    fn clear(
        &mut self,
        request: ClearSessionActivityRequest,
        logs: &HashMap<String, ConversationLog>,
    ) -> Value {
        match request.scope {
            SessionActivityScope::Commands => {
                for job in self.jobs(&request.session_id) {
                    job.view.cleared |= !job.view.running;
                }
            }
            SessionActivityScope::Subagents => {
                let ended = logs
                    .get(&request.session_id)
                    .map(|log| log.agents.as_slice())
                    .unwrap_or_default()
                    .iter()
                    .filter(|agent| agent.state != AgentState::Running)
                    .map(|agent| agent.id.clone());
                self.cleared_agents
                    .entry(request.session_id)
                    .or_default()
                    .extend(ended);
            }
        }
        Value::Null
    }
}

/// Parses the parameters as the strict request type the service router accepts.
/// 将参数按服务路由接受的严格请求类型解析。
fn parse<T: serde::de::DeserializeOwned>(call: &ServiceCall) -> Result<T, String> {
    serde_json::from_value(call.params.clone())
        .map_err(|error| format!("Invalid {} request: {error}", call.method))
}

fn to_value<T: serde::Serialize>(value: &T) -> Value {
    serde_json::to_value(value).expect("session activity results serialize")
}
