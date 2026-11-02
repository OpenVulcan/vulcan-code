//! Model-facing text and host payload for a finished chain.
//! 已结束链条的模型可见文本与宿主载荷。

use serde_json::{Value, json};

use crate::run::{StepOutcome, StepRecord};

/// Rendered chain result.
/// 渲染后的链条结果。
#[derive(Debug, Clone)]
pub struct ToolChainReport {
    /// Text returned to the model.
    /// 返回给模型的文本。
    pub model_output: String,
    /// Per-step summary for the host card, nesting each step's own display data.
    /// 供宿主卡片使用的逐步摘要，内嵌每个步骤自身的展示数据。
    pub host_payload: Value,
}

/// Renders the records of one chain run.
/// 渲染一次链条运行的记录。
pub fn render_report(records: &[StepRecord]) -> ToolChainReport {
    let count = |outcome: StepOutcome| {
        records
            .iter()
            .filter(|record| record.outcome == outcome)
            .count()
    };
    let ran = count(StepOutcome::Success) + count(StepOutcome::Failure);
    let mut summary = vec![
        format!("{} succeeded", count(StepOutcome::Success)),
        format!("{} failed", count(StepOutcome::Failure)),
    ];
    for (outcome, label) in [
        (StepOutcome::Skipped, "skipped"),
        (StepOutcome::Denied, "denied"),
        (StepOutcome::NotRun, "not run"),
    ] {
        let value = count(outcome);
        if value > 0 {
            summary.push(format!("{value} {label}"));
        }
    }
    let mut text = format!(
        "tool_chain ran {ran} of {} steps: {}.",
        records.len(),
        summary.join(", ")
    );

    for (index, record) in records.iter().enumerate() {
        let id = record
            .id
            .as_deref()
            .map(|id| format!(" `{id}`"))
            .unwrap_or_default();
        text.push_str(&format!(
            "\n\n## Step {}{id} ({}): {}",
            index + 1,
            record.tool,
            record.outcome.as_str().replace('_', " ")
        ));
        if let Some(code) = record.exit_code {
            text.push_str(&format!(" (exit code {code})"));
        }
        if let Some(detail) = &record.detail {
            text.push('\n');
            text.push_str(detail);
        }
        if record.outcome == StepOutcome::Denied {
            text.push_str(&format!(
                "\nThe chain stopped here and later steps were not run. If this step is still needed, call `{}` on its own as a normal tool call.",
                record.tool
            ));
        }
        if record.outcome.ran() {
            text.push('\n');
            text.push_str(if record.output.trim().is_empty() {
                "(no output)"
            } else {
                record.output.trim_end()
            });
        }
    }

    let steps = records
        .iter()
        .enumerate()
        .map(|(index, record)| {
            json!({
                "index": index,
                "id": record.id,
                "tool": record.tool,
                "outcome": record.outcome.as_str(),
                "exit_code": record.exit_code,
                "detail": record.detail,
                "arguments": record.arguments,
                "result": record.display,
            })
        })
        .collect::<Vec<_>>();
    ToolChainReport {
        model_output: text,
        host_payload: json!({ "steps": steps }),
    }
}
