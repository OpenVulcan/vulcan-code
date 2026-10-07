//! Session-derived summary skill used by the workbench runtime.
//! 工作台运行时使用的会话派生摘要技能。

use crate::agent_runtime::ExecutionRunReport;
use crate::model::{SessionMessageKind, SessionMessageRecordKind};

/// Aggregate chain bundling the built-in summary step.
/// 聚合摘要步骤的内建会话技能链。
#[derive(Debug, Clone, Default)]
pub struct SessionDerivedSkillChain {
    /// Stateless execution summary skill.
    /// 无状态执行摘要技能。
    summary_skill: SummarySkill,
}

impl SessionDerivedSkillChain {
    /// Runs the summary skill for one completed runtime execution report.
    /// 针对一份已完成的运行时执行报告运行摘要技能。
    pub fn run_summary_skill(&self, input: SummarySkillInput<'_>) -> SessionDerivedRecord {
        self.summary_skill.execute(input)
    }
}

/// Shared hidden-record payload produced by one session-derived skill.
/// 由单个会话派生技能产出的共享隐藏记录载荷。
#[derive(Debug, Clone)]
pub struct SessionDerivedRecord {
    /// Transcript-facing message kind used by the hidden node.
    /// 隐藏节点使用的转录消息类型。
    pub kind: SessionMessageKind,
    /// Durable hidden record kind persisted into the transcript history.
    /// 持久化到转录历史中的隐藏记录类型。
    pub record_kind: SessionMessageRecordKind,
    /// Internal title stored on the hidden node.
    /// 存储在隐藏节点上的内部标题。
    pub title: String,
    /// Internal body stored on the hidden node.
    /// 存储在隐藏节点上的内部正文。
    pub body: String,
    /// Compact preview stored alongside the hidden node for diagnostics and tooltip reuse.
    /// 与隐藏节点一起存储的紧凑预览，供诊断与 tooltip 复用。
    pub preview: String,
}

/// Input consumed by the execution summary skill.
/// 供执行摘要技能消费的输入参数。
pub struct SummarySkillInput<'a> {
    /// Completed execution report returned by the runtime.
    /// 由运行时返回的已完成执行报告。
    pub report: &'a ExecutionRunReport,
}

/// Stateless execution summary skill used after one runtime round finishes.
/// 在一次运行时回合结束后使用的无状态执行摘要技能。
#[derive(Debug, Clone, Default)]
struct SummarySkill;

impl SummarySkill {
    /// Executes the summary step and returns one hidden summary node for the finished run.
    /// 执行摘要步骤，并为已完成回合返回一条隐藏摘要节点。
    fn execute(&self, input: SummarySkillInput<'_>) -> SessionDerivedRecord {
        let body = if input.report.is_success() {
            let output_preview = input
                .report
                .assistant_output
                .as_deref()
                .map(|output| truncate_preview_lines(output, 3, 220))
                .unwrap_or_else(|| {
                    String::from("The model did not return visible text output for this run.")
                });
            format!("The execution finished successfully. Output summary: {output_preview}")
        } else {
            format!(
                "The execution failed. Error summary: {}",
                input
                    .report
                    .error
                    .as_deref()
                    .map(|error| truncate_preview_lines(error, 2, 180))
                    .unwrap_or_else(|| String::from("Unknown error"))
            )
        };

        SessionDerivedRecord {
            kind: SessionMessageKind::Assistant,
            record_kind: SessionMessageRecordKind::SummaryOutput,
            title: String::from("Execution Summary"),
            body,
            preview: if input.report.is_success() {
                input
                    .report
                    .assistant_output
                    .as_deref()
                    .map(|output| truncate_preview_lines(output, 2, 120))
                    .unwrap_or_else(|| {
                        String::from("The model did not return visible text output for this run.")
                    })
            } else {
                input
                    .report
                    .error
                    .as_deref()
                    .map(|error| truncate_preview_lines(error, 1, 120))
                    .unwrap_or_else(|| String::from("Unknown error"))
            },
        }
    }
}

/// Truncates one multi-line preview into a bounded, ellipsis-terminated single string.
/// 将多行预览截断为带省略号且受限的单条字符串。
fn truncate_preview_lines(value: &str, max_lines: usize, max_chars: usize) -> String {
    let flattened = value
        .lines()
        .take(max_lines)
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string();
    let mut truncated = String::new();
    for character in flattened.chars() {
        if truncated.chars().count() >= max_chars {
            truncated.push('…');
            break;
        }
        truncated.push(character);
    }
    if truncated.is_empty() {
        flattened
    } else {
        truncated
    }
}

#[cfg(test)]
mod tests {
    //! Session skill tests covering summary behavior after the compaction split.
    //! 覆盖压缩拆分后摘要行为的会话技能测试。

    use super::*;
    use crate::model::SessionId;

    /// Verifies that success summaries keep only a short output preview and never inline preparation diagnostics.
    /// 验证成功摘要只保留简短输出预览，且绝不会内联准备阶段诊断明细。
    #[test]
    fn summary_skill_success_output_omits_preparation_diagnostics() {
        let skill = SummarySkill;
        let report = ExecutionRunReport {
            session_id: SessionId::default(),
            preparation: None,
            assistant_output: Some(String::from("这是给用户的最终输出。")),
            thinking_output: None,
            conversation_state: None,
            provider_request_count: 0,
            last_successful_provider_usage: None,
            cumulative_successful_provider_usage: None,
            embedded_turn_usage_segments: Vec::new(),
            mcp_discovery_rows: Vec::new(),
            mcp_discovery_errors: Vec::new(),
            warnings: Vec::new(),
            error: None,
            provider_failure_category: None,
            controller_events: Vec::new(),
        };

        let record = skill.execute(SummarySkillInput { report: &report });

        assert_eq!(record.record_kind, SessionMessageRecordKind::SummaryOutput);
        assert!(
            record
                .body
                .contains("Output summary: 这是给用户的最终输出。")
        );
        assert!(!record.body.contains("准备摘要"));
        assert!(!record.body.contains("会话：#"));
    }
}
