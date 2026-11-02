//! Behavior tests for plan validation, expressions and ordered execution.
//! 计划校验、表达式与顺序执行的行为测试。

use serde_json::{Value, json};
use std::sync::Mutex;

use crate::{
    ChainStatus, MAX_KEPT_VALUE_BYTES, MAX_STEP_ARGUMENT_BYTES, StepExecution, StepExecutor,
    StepOutcome, StepRecord, StepToolPolicy, parse_tool_chain_plan, render_report, run_tool_chain,
};

/// Fake host that answers each tool from a script and records the resolved arguments.
/// 按脚本应答各工具并记录已解析参数的模拟宿主。
struct FakeHost {
    answers: fn(&str, &Value) -> StepExecution,
    calls: Mutex<Vec<(String, Value)>>,
}

impl FakeHost {
    fn new(answers: fn(&str, &Value) -> StepExecution) -> Self {
        Self {
            answers,
            calls: Mutex::new(Vec::new()),
        }
    }

    fn calls(&self) -> Vec<(String, Value)> {
        self.calls
            .lock()
            .map(|calls| calls.clone())
            .unwrap_or_default()
    }
}

impl StepExecutor for FakeHost {
    fn execute(
        &self,
        _index: usize,
        tool: &str,
        arguments: Value,
    ) -> impl Future<Output = StepExecution> + Send {
        let answer = (self.answers)(tool, &arguments);
        if let Ok(mut calls) = self.calls.lock() {
            calls.push((tool.to_string(), arguments));
        }
        std::future::ready(answer)
    }
}

fn ok(output: &str) -> StepExecution {
    StepExecution::Completed {
        output: output.to_string(),
        failed: false,
        exit_code: Some(0),
        streams: None,
        display: Some(json!({ "model_output": output })),
    }
}

fn failed(output: &str) -> StepExecution {
    StepExecution::Completed {
        output: output.to_string(),
        failed: true,
        exit_code: Some(1),
        streams: None,
        display: None,
    }
}

/// A command step whose raw streams are `stdout` and `stderr`.
/// 原始流为 `stdout` 与 `stderr` 的命令步骤。
fn command(stdout: &str, stderr: &str, exit_code: i64) -> StepExecution {
    StepExecution::Completed {
        output: String::from("Exit code: rendered for the model"),
        failed: exit_code != 0,
        exit_code: Some(exit_code),
        streams: Some((stdout.to_string(), stderr.to_string())),
        display: None,
    }
}

/// Every tool may be chained; `sh` runs its `command` argument as shell syntax.
/// 所有工具都可链接；`sh` 将其 `command` 参数作为 shell 语法运行。
fn any_tool(tool: &str) -> Result<StepToolPolicy, String> {
    Ok(if tool == "sh" {
        StepToolPolicy {
            command_arguments: &["command"],
        }
    } else {
        StepToolPolicy::default()
    })
}

async fn run(
    steps: Value,
    answers: fn(&str, &Value) -> StepExecution,
) -> (Vec<StepRecord>, FakeHost) {
    let plan = parse_tool_chain_plan(&json!({ "steps": steps }), 16, any_tool)
        .unwrap_or_else(|error| panic!("plan should be valid: {error}"));
    let host = FakeHost::new(answers);
    let records = run_tool_chain(&plan, &host).await;
    (records, host)
}

fn outcomes(records: &[StepRecord]) -> Vec<StepOutcome> {
    records.iter().map(|record| record.outcome).collect()
}

fn plan_error(arguments: Value) -> String {
    match parse_tool_chain_plan(&arguments, 3, |tool| {
        if tool == "ask" {
            Err(String::from("not chainable"))
        } else {
            Ok(StepToolPolicy::default())
        }
    }) {
        Ok(_) => panic!("plan should be rejected"),
        Err(error) => error,
    }
}

#[test]
fn validation_reports_every_problem_at_once() {
    let error = plan_error(json!({
        "steps": [
            {"id": "a", "tool": "shell_command", "arguments": {}},
            {"id": "a", "tool": "ask", "arguments": "{}", "extra": 1},
            {"tool": "shell_command", "arguments": {"command": "${{ steps.later.output }}"}},
            {"id": "later", "tool": "shell_command", "if": "steps.a.outcome ==", "arguments": {}}
        ]
    }));
    for expected in [
        "at most 3",
        "already used",
        "not chainable",
        "not a string containing JSON",
        "unknown field `extra`",
        "later step",
        "step 4 (`later`): in `if`",
    ] {
        assert!(error.contains(expected), "missing `{expected}` in: {error}");
    }
}

#[test]
fn validation_rejects_string_encoded_steps_and_unknown_root_fields() {
    assert!(plan_error(json!({"steps": "[]"})).contains("not a string"));
    assert!(plan_error(json!({"steps": [], "mode": "x"})).contains("`mode`"));
    assert!(plan_error(json!({"steps": []})).contains("at least one step"));
}

#[test]
fn validation_rejects_bad_literal_regex_and_unknown_functions() {
    let error = plan_error(json!({"steps": [
        {"id": "a", "tool": "t", "arguments": {}},
        {"tool": "t", "arguments": {"x": "${{ match(steps.a.output, '(') }}"}},
        {"tool": "t", "if": "hashFiles('x')", "arguments": {}}
    ]}));
    assert!(error.contains("invalid regex"), "{error}");
    assert!(error.contains("unknown function `hashFiles()`"), "{error}");
}

#[tokio::test]
async fn default_condition_skips_after_failure_and_failure_condition_runs() {
    let (records, host) = run(
        json!([
            {"id": "build", "tool": "build", "arguments": {}},
            {"id": "test", "tool": "test", "arguments": {}},
            {"tool": "report", "if": "failure()", "arguments": {}},
            {"tool": "cleanup", "if": "${{ always() }}", "arguments": {}}
        ]),
        |tool, _| {
            if tool == "build" {
                failed("error")
            } else {
                ok("done")
            }
        },
    )
    .await;
    assert_eq!(
        outcomes(&records),
        [
            StepOutcome::Failure,
            StepOutcome::Skipped,
            StepOutcome::Success,
            StepOutcome::Success
        ]
    );
    let tools = host
        .calls()
        .into_iter()
        .map(|(tool, _)| tool)
        .collect::<Vec<_>>();
    assert_eq!(tools, ["build", "report", "cleanup"]);
}

#[tokio::test]
async fn outputs_flow_into_later_arguments_with_types_preserved() {
    let (records, host) = run(
        json!([
            {"id": "find", "tool": "search", "arguments": {}},
            {"id": "meta", "tool": "meta", "arguments": {}},
            {"tool": "read", "arguments": {
                "path": "${{ match(steps.find.output, '^(\\S+)') }}",
                "start_line": "${{ fromJSON(steps.meta.output).line }}",
                "label": "line ${{ fromJSON(steps.meta.output).line }} of ${{ steps.find.exit_code }}",
                "nested": ["${{ steps.meta.outcome }}"]
            }}
        ]),
        |tool, _| match tool {
            "search" => ok("  src/config.rs:12: fn parse_config\nsrc/other.rs\n"),
            "meta" => ok("{\"line\": 40}"),
            _ => ok("contents"),
        },
    )
    .await;
    assert_eq!(records[2].outcome, StepOutcome::Success);
    let (_, arguments) = &host.calls()[2];
    assert_eq!(
        arguments,
        &json!({
            "path": "src/config.rs:12:",
            "start_line": 40,
            "label": "line 40 of 0",
            "nested": ["success"]
        })
    );
}

#[tokio::test]
async fn conditions_read_outcomes_outputs_and_exit_codes() {
    let (records, _) = run(
        json!([
            {"id": "check", "tool": "check", "if": "true", "arguments": {}},
            {"id": "a", "tool": "a", "if": "steps.check.exit_code == '1' && contains(steps.check.output, 'warning')", "arguments": {}},
            {"id": "b", "tool": "b", "if": "steps.check.outcome == 'failure' && !startsWith(steps.check.output, 'ok')", "arguments": {}},
            {"tool": "c", "if": "steps.a.outcome != 'skipped' || contains(fromJSON('[1,2]'), 2)", "arguments": {}}
        ]),
        |tool, _| if tool == "check" { failed("found warning") } else { ok("") },
    )
    .await;
    assert_eq!(
        outcomes(&records),
        [
            StepOutcome::Failure,
            StepOutcome::Success,
            StepOutcome::Success,
            StepOutcome::Success
        ]
    );
}

#[tokio::test]
async fn expression_errors_fail_the_step_without_calling_the_tool() {
    let (records, host) = run(
        json!([
            {"id": "find", "tool": "search", "arguments": {}},
            {"tool": "read", "arguments": {"path": "${{ match(steps.find.output, 'zzz') }}"}},
            {"tool": "after", "if": "always()", "arguments": {"x": "${{ steps.find.output }}"}}
        ]),
        |_, _| ok("no match here"),
    )
    .await;
    assert_eq!(
        outcomes(&records),
        [
            StepOutcome::Success,
            StepOutcome::Failure,
            StepOutcome::Success
        ]
    );
    assert!(records[1].detail.is_some());
    assert_eq!(host.calls().len(), 2);
}

#[tokio::test]
async fn reading_output_of_a_skipped_step_is_an_error() {
    let (records, _) = run(
        json!([
            {"id": "a", "tool": "a", "if": "false", "arguments": {}},
            {"tool": "b", "arguments": {"x": "${{ steps.a.output }}"}}
        ]),
        |_, _| ok(""),
    )
    .await;
    assert_eq!(
        outcomes(&records),
        [StepOutcome::Skipped, StepOutcome::Failure]
    );
}

#[tokio::test]
async fn denial_halts_the_chain() {
    let (records, host) = run(
        json!([
            {"tool": "a", "arguments": {}},
            {"tool": "b", "arguments": {}},
            {"tool": "c", "if": "always()", "arguments": {}}
        ]),
        |tool, _| {
            if tool == "b" {
                StepExecution::Denied {
                    message: String::from("denied"),
                }
            } else {
                ok("")
            }
        },
    )
    .await;
    assert_eq!(
        outcomes(&records),
        [
            StepOutcome::Success,
            StepOutcome::Denied,
            StepOutcome::NotRun
        ]
    );
    assert_eq!(host.calls().len(), 2);
    let report = render_report(&records);
    let steps = report.host_payload["steps"].as_array().map(Vec::len);
    assert_eq!(steps, Some(3));
    assert_eq!(report.host_payload["steps"][1]["outcome"], "denied");
    assert_eq!(report.host_payload["steps"][2]["outcome"], "not_run");
}

#[tokio::test]
async fn report_nests_each_step_display_in_order() {
    let (records, _) = run(
        json!([
            {"id": "a", "tool": "a", "arguments": {"k": 1}},
            {"tool": "b", "if": "false", "arguments": {}}
        ]),
        |_, _| ok("unique-output-marker"),
    )
    .await;
    let report = render_report(&records);
    assert!(report.model_output.contains("unique-output-marker"));
    let steps = &report.host_payload["steps"];
    assert_eq!(steps[0]["index"], 0);
    assert_eq!(steps[0]["arguments"], json!({"k": 1}));
    assert_eq!(
        steps[0]["result"],
        json!({"model_output": "unique-output-marker"})
    );
    assert_eq!(steps[1]["outcome"], "skipped");
    assert_eq!(steps[1]["result"], Value::Null);
}

#[tokio::test]
async fn command_arguments_take_only_numbers_and_booleans_from_expressions() {
    let (records, host) = run(
        json!([
            {"id": "find", "tool": "sh", "arguments": {"command": "find"}},
            {"tool": "sh", "if": "always()", "arguments": {"command": "cat ${{ steps.find.stdout }}"}},
            {"tool": "sh", "if": "always()", "arguments": {"command": "${{ steps.find.stdout }}"}},
            {"tool": "sh", "if": "always()", "arguments": {"command": "exit ${{ steps.find.exit_code }} ${{ test(steps.find.stdout, 'a') }}"}},
            {"tool": "sh", "if": "always()", "arguments": {"command": "xargs cat", "stdin": "${{ steps.find.stdout }}"}},
            {"tool": "read", "if": "always()", "arguments": {"command": "${{ steps.find.stdout }}"}}
        ]),
        |_, _| command("a.txt; rm -rf /", "", 0),
    )
    .await;
    assert_eq!(
        outcomes(&records),
        [
            StepOutcome::Success,
            StepOutcome::Failure,
            StepOutcome::Failure,
            StepOutcome::Success,
            StepOutcome::Success,
            StepOutcome::Success
        ]
    );
    let calls = host.calls();
    assert_eq!(
        calls.len(),
        4,
        "rejected command steps must not call the tool"
    );
    assert_eq!(calls[1].1["command"], "exit 0 true");
    assert_eq!(calls[2].1["stdin"], "a.txt; rm -rf /");
    assert_eq!(calls[3].1["command"], "a.txt; rm -rf /");
}

#[tokio::test]
async fn streams_are_readable_but_never_reported() {
    let (records, host) = run(
        json!([
            {"id": "run", "tool": "sh", "arguments": {"command": "x"}},
            {"id": "plain", "tool": "read", "arguments": {}},
            {"tool": "use", "arguments": {
                "out": "${{ steps.run.stdout }}",
                "err": "${{ steps.run.stderr }}"
            }},
            {"tool": "use", "if": "always()", "arguments": {"x": "${{ steps.plain.stdout }}"}}
        ]),
        |tool, _| match tool {
            "sh" => command("  stdout-marker\n\n", "stderr-marker\n", 0),
            _ => ok("plain text"),
        },
    )
    .await;
    assert_eq!(records[2].outcome, StepOutcome::Success);
    assert_eq!(
        host.calls()[2].1,
        json!({"out": "  stdout-marker", "err": "stderr-marker"})
    );
    assert_eq!(
        records[3].outcome,
        StepOutcome::Failure,
        "a non-command step has no stdout"
    );

    // Kept values reach the report only through the arguments of the step that referenced them.
    // 保留的取值只通过引用它们的步骤参数进入报告。
    let report = render_report(&records[..2]);
    let rendered = format!("{}{}", report.model_output, report.host_payload);
    assert!(!rendered.contains("stdout-marker") && !rendered.contains("stderr-marker"));
    let report = render_report(&records);
    assert!(report.host_payload["steps"][2]["arguments"]["out"] == "  stdout-marker");
}

#[tokio::test]
async fn kept_values_share_one_chain_budget() {
    let (records, _) = run(
        json!([
            {"id": "a", "tool": "sh", "arguments": {"command": "x"}},
            {"id": "b", "tool": "sh", "arguments": {"command": "x"}},
            {"tool": "use", "arguments": {"x": "${{ steps.a.exit_code }}"}},
            {"tool": "use", "arguments": {"x": "${{ steps.b.stderr }}"}}
        ]),
        |_, _| command(&"x".repeat(MAX_KEPT_VALUE_BYTES / 2 + 1), "", 0),
    )
    .await;
    assert_eq!(
        outcomes(&records),
        [
            StepOutcome::Success,
            StepOutcome::Success,
            StepOutcome::Success,
            StepOutcome::Failure
        ]
    );
}

#[tokio::test]
async fn resolved_arguments_over_the_step_limit_fail_the_step() {
    let (records, host) = run(
        json!([
            {"id": "big", "tool": "sh", "arguments": {"command": "x"}},
            {"tool": "sh", "arguments": {"command": "wc -c", "stdin": "${{ steps.big.stdout }}"}}
        ]),
        |_, _| command(&"x".repeat(MAX_STEP_ARGUMENT_BYTES + 1), "", 0),
    )
    .await;
    assert_eq!(
        outcomes(&records),
        [StepOutcome::Success, StepOutcome::Failure]
    );
    assert_eq!(host.calls().len(), 1);
}

#[tokio::test]
async fn continue_on_error_keeps_the_failure_but_not_its_effects() {
    let (records, host) = run(
        json!([
            {"id": "lint", "tool": "lint", "continue_on_error": true, "arguments": {}},
            {"tool": "build", "arguments": {}},
            {"tool": "report", "if": "failure()", "arguments": {}},
            {"tool": "note", "if": "steps.lint.outcome == 'failure'", "arguments": {}}
        ]),
        |tool, _| {
            if tool == "lint" {
                failed("warnings")
            } else {
                ok("")
            }
        },
    )
    .await;
    assert_eq!(
        outcomes(&records),
        [
            StepOutcome::Failure,
            StepOutcome::Success,
            StepOutcome::Skipped,
            StepOutcome::Success
        ]
    );
    assert_eq!(host.calls().len(), 3);
    let report = render_report(&records);
    assert_eq!(ChainStatus::of(&records), ChainStatus::Succeeded);
    assert_eq!(report.host_payload["status"], "succeeded");
    assert_eq!(report.host_payload["steps"][0]["continue_on_error"], true);
    assert!(report.model_output.starts_with("tool_chain SUCCEEDED"));
}

#[tokio::test]
async fn denial_stops_the_chain_even_with_continue_on_error() {
    let (records, host) = run(
        json!([
            {"tool": "write", "continue_on_error": true, "arguments": {}},
            {"tool": "after", "if": "always()", "continue_on_error": true, "arguments": {}}
        ]),
        |tool, _| {
            if tool == "write" {
                StepExecution::Denied {
                    message: String::from("denied"),
                }
            } else {
                ok("")
            }
        },
    )
    .await;
    assert_eq!(
        outcomes(&records),
        [StepOutcome::Denied, StepOutcome::NotRun]
    );
    assert_eq!(host.calls().len(), 1);
    let report = render_report(&records);
    assert_eq!(report.host_payload["status"], "stopped");
    assert!(report.model_output.starts_with("tool_chain STOPPED"));
}

#[tokio::test]
async fn a_recovered_failure_still_reports_the_chain_as_failed() {
    let (records, _) = run(
        json!([
            {"id": "test", "tool": "test", "arguments": {}},
            {"tool": "fix", "if": "failure()", "arguments": {}}
        ]),
        |tool, _| {
            if tool == "test" {
                failed("1 failed")
            } else {
                ok("")
            }
        },
    )
    .await;
    assert_eq!(
        outcomes(&records),
        [StepOutcome::Failure, StepOutcome::Success]
    );
    let report = render_report(&records);
    assert_eq!(report.host_payload["status"], "failed");
    assert!(
        report
            .model_output
            .starts_with("tool_chain FAILED: step 1 `test` failed")
    );
}

#[tokio::test]
async fn test_returns_a_boolean_where_match_fails_the_step() {
    let (records, _) = run(
        json!([
            {"id": "a", "tool": "a", "arguments": {}},
            {"tool": "hit", "if": "test(steps.a.output, '^v[0-9]+')", "arguments": {}},
            {"tool": "miss", "if": "test(steps.a.output, 'zzz')", "arguments": {}},
            {"tool": "strict", "if": "always()", "arguments": {"x": "${{ match(steps.a.output, 'zzz') }}"}}
        ]),
        |_, _| ok("v12 ready"),
    )
    .await;
    assert_eq!(
        outcomes(&records),
        [
            StepOutcome::Success,
            StepOutcome::Success,
            StepOutcome::Skipped,
            StepOutcome::Failure
        ]
    );
}

#[test]
fn validation_checks_continue_on_error_and_test_regex() {
    let error = plan_error(json!({"steps": [
        {"id": "a", "tool": "t", "continue_on_error": "yes", "arguments": {}},
        {"tool": "t", "if": "test(steps.a.output, '(')", "arguments": {}}
    ]}));
    assert!(
        error.contains("`continue_on_error` must be true or false"),
        "{error}"
    );
    assert!(error.contains("invalid regex in test()"), "{error}");
}

/// Host whose `slow` tool never finishes, standing in for a step the user stops.
/// `slow` 工具永不结束的宿主，代表被用户停止的步骤。
struct StoppedHost {
    calls: Mutex<Vec<String>>,
}

impl StepExecutor for StoppedHost {
    fn execute(
        &self,
        _index: usize,
        tool: &str,
        _arguments: Value,
    ) -> impl Future<Output = StepExecution> + Send {
        if let Ok(mut calls) = self.calls.lock() {
            calls.push(tool.to_string());
        }
        let slow = tool == "slow";
        async move {
            if slow {
                std::future::pending::<()>().await;
            }
            ok("")
        }
    }
}

#[test]
fn dropping_a_running_chain_starts_no_later_step() {
    let plan = parse_tool_chain_plan(
        &json!({"steps": [
            {"tool": "fast", "arguments": {}},
            {"tool": "slow", "continue_on_error": true, "arguments": {}},
            {"tool": "after", "if": "always()", "arguments": {}}
        ]}),
        16,
        any_tool,
    )
    .unwrap_or_else(|error| panic!("plan should be valid: {error}"));
    let host = StoppedHost {
        calls: Mutex::new(Vec::new()),
    };
    {
        let mut chain = std::pin::pin!(run_tool_chain(&plan, &host));
        let mut context = std::task::Context::from_waker(std::task::Waker::noop());
        assert!(chain.as_mut().poll(&mut context).is_pending());
    }
    let calls = host
        .calls
        .lock()
        .map(|calls| calls.clone())
        .unwrap_or_default();
    assert_eq!(calls, ["fast", "slow"]);
}
