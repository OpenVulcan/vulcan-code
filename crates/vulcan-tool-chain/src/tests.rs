//! Behavior tests for plan validation, expressions and ordered execution.
//! 计划校验、表达式与顺序执行的行为测试。

use serde_json::{Value, json};
use std::sync::Mutex;

use crate::{
    StepExecution, StepExecutor, StepOutcome, StepRecord, parse_tool_chain_plan, render_report,
    run_tool_chain,
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
        display: Some(json!({ "model_output": output })),
    }
}

fn failed(output: &str) -> StepExecution {
    StepExecution::Completed {
        output: output.to_string(),
        failed: true,
        exit_code: Some(1),
        display: None,
    }
}

fn any_tool(_: &str) -> Result<(), String> {
    Ok(())
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
            Ok(())
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
