// Mirror of `service/session-activity-service.ts`: background shell jobs, their logs and the
// subagent list of one root session.
// `service/session-activity-service.ts` 的镜像：一个根会话的后台命令任务、任务日志与子代理列表。
//
// `backgroundShellCommand` is not mirrored: the WebUI offers it from the live output of a running
// `shell_command` card, and the TUI transcript has neither that live view nor the tool call ids.
// 未镜像 `backgroundShellCommand`：WebUI 在运行中 `shell_command` 卡片的实时输出中提供它，而 TUI
// 会话记录既没有该实时视图，也没有工具调用标识。

use serde_json::{Value, json};
use vulcan_service_api::{
    ClearSessionActivityRequest, SessionActivityRequest, SessionActivityScope, ShellJobListRequest,
    ShellJobOutputRequest, ShellJobTargetRequest,
};

use super::{ServiceCall, Slot};

/// Serializes one shared request type into the exact RPC parameter document.
/// 将一个共享请求类型序列化为准确的 RPC 参数文档。
fn params<T: serde::Serialize>(request: &T) -> Value {
    serde_json::to_value(request).expect("session activity requests serialize")
}

/// Builds the request for `service_list_shell_jobs`; `finished_limit` caps the finished jobs.
/// 构建 `service_list_shell_jobs` 的请求；`finished_limit` 限制已结束任务的数量。
pub fn list_shell_jobs(
    session_id: &str,
    include_cleared: bool,
    finished_limit: Option<usize>,
) -> ServiceCall {
    ServiceCall::load(
        "list_shell_jobs",
        "service_list_shell_jobs",
        params(&ShellJobListRequest {
            session_id: session_id.to_string(),
            include_cleared,
            finished_limit,
        }),
        Slot::ShellJobs,
    )
}

/// Selects one page of a job log: the tail by default, rows ending before `before_line`, or rows
/// from `from_line`.
/// 选择任务日志的一页：默认为末尾，或 `before_line` 之前的行，或从 `from_line` 开始的行。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ShellJobOutputQuery {
    /// Read rows that end before this one-based line.
    /// 读取结束于该从 1 开始行号之前的行。
    pub before_line: Option<u64>,
    /// Read rows starting at this one-based line.
    /// 读取从该从 1 开始行号开始的行。
    pub from_line: Option<u64>,
    /// Maximum log lines of the page.
    /// 本页最多的日志行数。
    pub max_lines: Option<usize>,
}

/// Builds the request for `service_read_shell_job_output`. Pages read upward land in their own
/// slot, so a page above never cancels the tail that follows a running job.
/// 构建 `service_read_shell_job_output` 的请求。向上读取的页进入独立槽位，因此读取上方的页不会取消
/// 跟随运行中任务的末尾读取。
pub fn read_shell_job_output(
    session_id: &str,
    job_id: &str,
    query: ShellJobOutputQuery,
) -> ServiceCall {
    let slot = if query.before_line.is_some() {
        Slot::ShellJobOutputEarlier
    } else {
        Slot::ShellJobOutput
    };
    ServiceCall::load(
        "read_shell_job_output",
        "service_read_shell_job_output",
        params(&ShellJobOutputRequest {
            session_id: session_id.to_string(),
            job_id: job_id.to_string(),
            before_line: query.before_line,
            from_line: query.from_line,
            max_lines: query.max_lines,
        }),
        slot,
    )
}

/// Builds the request for `service_stop_shell_job`; the command list reloads afterwards.
/// 构建 `service_stop_shell_job` 的请求；之后重新读取命令列表。
pub fn stop_shell_job(session_id: &str, job_id: &str) -> ServiceCall {
    ServiceCall::mutate(
        "stop_shell_job",
        "service_stop_shell_job",
        params(&ShellJobTargetRequest {
            session_id: session_id.to_string(),
            job_id: job_id.to_string(),
        }),
        &[Slot::ShellJobs],
    )
}

/// Builds the request for `service_session_activity`.
/// 构建 `service_session_activity` 的请求。
pub fn load_session_activity(session_id: &str, include_cleared: bool) -> ServiceCall {
    ServiceCall::load(
        "load_session_activity",
        "service_session_activity",
        params(&SessionActivityRequest {
            session_id: session_id.to_string(),
            include_cleared,
        }),
        Slot::SessionActivity,
    )
}

/// Builds the request for `service_clear_session_activity`; the cleared list reloads afterwards.
/// 构建 `service_clear_session_activity` 的请求；之后重新读取被清理的列表。
pub fn clear_session_activity(session_id: &str, scope: SessionActivityScope) -> ServiceCall {
    ServiceCall::mutate(
        "clear_session_activity",
        "service_clear_session_activity",
        params(&ClearSessionActivityRequest {
            session_id: session_id.to_string(),
            scope,
        }),
        match scope {
            SessionActivityScope::Subagents => &[Slot::SessionActivity],
            SessionActivityScope::Commands => &[Slot::ShellJobs],
        },
    )
}

/// Builds the request for `service_running_shell_job_count`: running background jobs of every
/// session.
/// 构建 `service_running_shell_job_count` 的请求：所有会话中运行中的后台任务数。
pub fn count_running_shell_jobs() -> ServiceCall {
    ServiceCall::load(
        "count_running_shell_jobs",
        "service_running_shell_job_count",
        json!({}),
        Slot::RunningShellJobCount,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::de::DeserializeOwned;

    /// Parses the parameters back as the strict shared request the service router accepts.
    /// 将参数按服务路由接受的严格共享请求类型解析回来。
    fn strict<T: DeserializeOwned>(call: &ServiceCall) -> T {
        serde_json::from_value(call.params.clone()).expect("the router accepts the request")
    }

    /// Every builder sends a document the service router parses strictly, with the WebUI's nulls.
    /// 每个构建器发送的文档都能被服务路由严格解析，并保留 WebUI 的空值。
    #[test]
    fn requests_parse_as_the_strict_service_types() {
        let list = list_shell_jobs("s1", true, None);
        assert_eq!(list.method, "service_list_shell_jobs");
        assert!(list.param("/finishedLimit").is_null());
        let parsed: ShellJobListRequest = strict(&list);
        assert!(parsed.include_cleared && parsed.finished_limit.is_none());
        let limited: ShellJobListRequest = strict(&list_shell_jobs("s1", false, Some(5)));
        assert_eq!(limited.finished_limit, Some(5));

        let tail = read_shell_job_output("s1", "job-1", ShellJobOutputQuery::default());
        assert!(tail.param("/beforeLine").is_null() && tail.param("/fromLine").is_null());
        let _: ShellJobOutputRequest = strict(&tail);
        let _: ShellJobTargetRequest = strict(&stop_shell_job("s1", "job-1"));
        let _: SessionActivityRequest = strict(&load_session_activity("s1", false));
        for scope in [
            SessionActivityScope::Subagents,
            SessionActivityScope::Commands,
        ] {
            let request: ClearSessionActivityRequest = strict(&clear_session_activity("s1", scope));
            assert_eq!(request.scope, scope);
        }
        assert_eq!(
            count_running_shell_jobs().method,
            "service_running_shell_job_count"
        );
    }

    /// Upward pages and the following tail use separate slots; writes reload the list they change.
    /// 向上翻页与跟随末尾使用不同槽位；写入重读其改变的列表。
    #[test]
    fn output_pages_and_writes_route_to_their_slots() {
        use crate::service_api::CallKind;
        let earlier = read_shell_job_output(
            "s1",
            "job-1",
            ShellJobOutputQuery {
                before_line: Some(40),
                ..ShellJobOutputQuery::default()
            },
        );
        assert_eq!(earlier.kind, CallKind::Load(Slot::ShellJobOutputEarlier));
        let follow = read_shell_job_output(
            "s1",
            "job-1",
            ShellJobOutputQuery {
                from_line: Some(40),
                ..ShellJobOutputQuery::default()
            },
        );
        assert_eq!(follow.kind, CallKind::Load(Slot::ShellJobOutput));
        let reloads = |call: ServiceCall| match call.kind {
            CallKind::Mutate(slots) => slots.to_vec(),
            other => panic!("expected a write, got {other:?}"),
        };
        assert_eq!(
            reloads(stop_shell_job("s1", "job-1")),
            vec![Slot::ShellJobs]
        );
        assert_eq!(
            reloads(clear_session_activity(
                "s1",
                SessionActivityScope::Subagents
            )),
            vec![Slot::SessionActivity]
        );
    }
}
