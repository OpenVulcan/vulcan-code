//! Session-derived title, explore, and summary skills used by the workbench runtime.
//! 工作台运行时使用的会话派生标题、探索与摘要技能。

use crate::agent_runtime::ExecutionRunReport;
use crate::model::{
    SessionMessageKind, SessionMessageRecordKind, SessionTitleSource, TaskProjectSummary,
};

/// Aggregate chain bundling the built-in title, explore, and summary steps.
/// 聚合标题、探索与摘要步骤的内建会话技能链。
#[derive(Debug, Clone, Default)]
pub struct SessionDerivedSkillChain {
    /// Stateless title-generation agent.
    /// 无状态标题生成代理。
    title_agent: TitleAgent,
    /// Stateless project explore skill.
    /// 无状态项目探索技能。
    explore_skill: ExploreSkill,
    /// Stateless execution summary skill.
    /// 无状态执行摘要技能。
    summary_skill: SummarySkill,
}

impl SessionDerivedSkillChain {
    /// Runs the title agent for one fresh user prompt when the session title is still auto-managed.
    /// 当会话标题仍由系统托管时，针对一条新的用户提示运行标题代理。
    pub fn run_title_agent(&self, input: TitleAgentInput<'_>) -> Option<TitleAgentResult> {
        self.title_agent.execute(input)
    }

    /// Runs the explore skill for one newly materialized session bound to the current project.
    /// 针对一个刚完成物化且绑定到当前项目的会话运行探索技能。
    pub fn run_explore_skill(&self, input: ExploreSkillInput<'_>) -> SessionDerivedRecord {
        self.explore_skill.execute(input)
    }

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

/// Input consumed by the title-generation agent.
/// 供标题生成代理消费的输入参数。
pub struct TitleAgentInput<'a> {
    /// Current user-managed or generated title.
    /// 当前用户或系统管理的会话标题。
    pub current_title: &'a str,
    /// Current title ownership mode.
    /// 当前标题所有权模式。
    pub title_source: SessionTitleSource,
    /// Latest user prompt.
    /// 最新用户提示词。
    pub prompt: &'a str,
}

/// Output produced by the title-generation agent.
/// 由标题生成代理产出的结果。
#[derive(Debug, Clone)]
pub struct TitleAgentResult {
    /// Generated visible session title.
    /// 生成出的可见会话标题。
    pub generated_title: String,
    /// Hidden trace record persisted for auditability.
    /// 为可审计性保留的隐藏追踪记录。
    pub hidden_record: SessionDerivedRecord,
}

/// Input consumed by the project explore skill.
/// 供项目探索技能消费的输入参数。
pub struct ExploreSkillInput<'a> {
    /// Current normalized workspace path.
    /// 当前规范化后的工作区路径。
    pub workspace_path: &'a str,
    /// Optional project identifier already bound to the session.
    /// 已绑定到会话上的可选项目标识。
    pub project_id: Option<&'a str>,
    /// Optional visible project snapshot.
    /// 可选的可见项目快照。
    pub project: Option<&'a TaskProjectSummary>,
}

/// Input consumed by the execution summary skill.
/// 供执行摘要技能消费的输入参数。
pub struct SummarySkillInput<'a> {
    /// Completed execution report returned by the runtime.
    /// 由运行时返回的已完成执行报告。
    pub report: &'a ExecutionRunReport,
}

/// Stateless title-generation agent used by fresh draft sessions.
/// 供新生草稿会话使用的无状态标题生成代理。
#[derive(Debug, Clone, Default)]
struct TitleAgent;

impl TitleAgent {
    /// Executes the title-generation step and returns nothing when the title should stay unchanged.
    /// 执行标题生成步骤，并在标题应保持不变时返回空值。
    fn execute(&self, input: TitleAgentInput<'_>) -> Option<TitleAgentResult> {
        if input.title_source == SessionTitleSource::Manual {
            return None;
        }

        let generated_title = derive_session_title_from_prompt(input.prompt)?;
        if generated_title == input.current_title {
            return None;
        }

        Some(TitleAgentResult {
            hidden_record: SessionDerivedRecord {
                kind: SessionMessageKind::Assistant,
                record_kind: SessionMessageRecordKind::TitleOutput,
                title: String::from("标题生成"),
                body: format!("系统已根据最新用户输入将当前会话标题更新为：{generated_title}"),
                preview: generated_title.clone(),
            },
            generated_title,
        })
    }
}

/// Stateless project explore skill used when one draft session is first materialized.
/// 在草稿会话首次物化时使用的无状态项目探索技能。
#[derive(Debug, Clone, Default)]
struct ExploreSkill;

impl ExploreSkill {
    /// Executes the explore step and returns one hidden context node describing the bound project scope.
    /// 执行探索步骤，并返回一条描述绑定项目范围的隐藏上下文节点。
    fn execute(&self, input: ExploreSkillInput<'_>) -> SessionDerivedRecord {
        let body = if let Some(project) = input.project {
            format!(
                "This session is bound to project `{}` with root directory `{}`. Future explore and triage steps should prioritize that scope.",
                project.title, project.path
            )
        } else if let Some(project_id) = input.project_id {
            format!(
                "This session is bound to project id `{project_id}`, but the project details are not loaded yet. Future explore and triage steps should fall back to the workspace scope."
            )
        } else {
            format!(
                "This session is not bound to an explicit project directory. Future explore and triage steps should operate within workspace `{}`.",
                input.workspace_path
            )
        };

        SessionDerivedRecord {
            kind: SessionMessageKind::Assistant,
            record_kind: SessionMessageRecordKind::ExploreOutput,
            title: String::from("Explore Context"),
            body,
            preview: input
                .project
                .map(|project| format!("{} · {}", project.title, project.path))
                .or_else(|| {
                    input
                        .project_id
                        .map(|project_id| format!("project id {project_id}"))
                })
                .unwrap_or_else(|| input.workspace_path.to_string()),
        }
    }
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

/// Builds a compact user-facing title from the latest prompt and drops surrounding decoration.
/// 根据最新提示词构建紧凑的用户可见标题，并裁去外围装饰。
fn derive_session_title_from_prompt(prompt: &str) -> Option<String> {
    let collapsed = prompt
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string();
    if collapsed.is_empty() {
        return None;
    }

    let compact = truncate_preview_lines(&collapsed, 1, 30).replace('\n', " ");
    let trimmed = compact.trim_matches(|character: char| {
        character.is_ascii_punctuation() || matches!(character, '，' | '。' | '：' | '；')
    });

    Some(if trimmed.is_empty() {
        String::from("新对话")
    } else {
        trimmed.to_string()
    })
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
            embedded_turn_usage_segments: Vec::new(),
            mcp_discovery_rows: Vec::new(),
            mcp_discovery_errors: Vec::new(),
            warnings: Vec::new(),
            error: None,
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
