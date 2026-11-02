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

/// Overall result of a chain.
/// 链条的总体结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChainStatus {
    /// No step failed, apart from failures allowed by `continue_on_error`.
    /// 除 `continue_on_error` 允许的失败外，没有步骤失败。
    Succeeded,
    /// A step failed without `continue_on_error`; a later recovery step does not undo it.
    /// 有步骤在未设置 `continue_on_error` 时失败；后续恢复步骤不会撤销它。
    Failed,
    /// A step was denied and the chain stopped.
    /// 有步骤被拒绝，链条已停止。
    Stopped,
}

impl ChainStatus {
    /// Derives the status from the step records.
    /// 根据步骤记录推导状态。
    pub fn of(records: &[StepRecord]) -> Self {
        if records
            .iter()
            .any(|record| record.outcome == StepOutcome::Denied)
        {
            Self::Stopped
        } else if records.iter().any(StepRecord::counts_as_failure) {
            Self::Failed
        } else {
            Self::Succeeded
        }
    }

    /// Stable wire name used in the host payload.
    /// 宿主载荷中使用的稳定线上名称。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Stopped => "stopped",
        }
    }
}

/// Names a step for the report's first line.
/// 为报告首行命名步骤。
fn step_name(index: usize, record: &StepRecord) -> String {
    match &record.id {
        Some(id) => format!("step {} `{id}`", index + 1),
        None => format!("step {}", index + 1),
    }
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
    let allowed = records
        .iter()
        .filter(|record| record.outcome == StepOutcome::Failure && record.continue_on_error)
        .count();
    let mut summary = vec![
        format!("{} succeeded", count(StepOutcome::Success)),
        format!("{} failed", count(StepOutcome::Failure) - allowed),
    ];
    for (value, label) in [
        (allowed, "failed but allowed by continue_on_error"),
        (count(StepOutcome::Skipped), "skipped"),
        (count(StepOutcome::Denied), "denied"),
        (count(StepOutcome::NotRun), "not run"),
    ] {
        if value > 0 {
            summary.push(format!("{value} {label}"));
        }
    }
    let status = ChainStatus::of(records);
    let headline = match status {
        ChainStatus::Succeeded => String::from("tool_chain SUCCEEDED"),
        ChainStatus::Failed => format!(
            "tool_chain FAILED: {} failed",
            records
                .iter()
                .enumerate()
                .filter(|(_, record)| record.counts_as_failure())
                .map(|(index, record)| step_name(index, record))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        ChainStatus::Stopped => format!(
            "tool_chain STOPPED: {} was denied",
            records
                .iter()
                .enumerate()
                .find(|(_, record)| record.outcome == StepOutcome::Denied)
                .map(|(index, record)| step_name(index, record))
                .unwrap_or_default()
        ),
    };
    let mut text = format!(
        "{headline}. Ran {ran} of {} steps: {}.",
        records.len(),
        summary.join(", ")
    );

    for (index, record) in records.iter().enumerate() {
        let id = record
            .id
            .as_deref()
            .map(|id| format!(" `{id}`"))
            .unwrap_or_default();
        let allowed = if record.outcome == StepOutcome::Failure && record.continue_on_error {
            ", allowed by continue_on_error"
        } else {
            ""
        };
        text.push_str(&format!(
            "\n\n## Step {}{id} ({}): {}{allowed}",
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
                "continue_on_error": record.continue_on_error,
                "exit_code": record.exit_code,
                "detail": record.detail,
                "arguments": record.arguments,
                "result": record.display,
            })
        })
        .collect::<Vec<_>>();
    ToolChainReport {
        model_output: text,
        host_payload: json!({ "status": status.as_str(), "steps": steps }),
    }
}
