//! Session-derived title, explore, summary, and compaction skills used by the workbench runtime.
//! 工作台运行时使用的会话派生标题、探索、摘要与压缩技能。

use crate::agent_runtime::ExecutionRunReport;
use crate::model::{
    SessionCompactionEntry, SessionId, SessionMessageEntry, SessionMessageKind,
    SessionMessageRecordKind, SessionRuntimeStateEntry, SessionTitleSource, TaskProjectSummary,
};
use std::time::{SystemTime, UNIX_EPOCH};

/// Aggregate chain bundling the built-in title, explore, summary, and compaction steps.
/// 聚合标题、探索、摘要与压缩步骤的内建会话技能链。
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
    /// Stateless context compaction skill.
    /// 无状态上下文压缩技能。
    compaction_skill: CompactionSkill,
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

    /// Runs the compaction skill and produces a hidden summary plus the next checkpoint metadata.
    /// 运行压缩技能，并产出隐藏摘要与下一枚检查点元数据。
    pub fn run_compaction_skill(
        &self,
        input: CompactionSkillInput<'_>,
    ) -> Option<CompactionSkillResult> {
        self.compaction_skill.execute(input)
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

/// Input consumed by the context compaction skill.
/// 供上下文压缩技能消费的输入参数。
pub struct CompactionSkillInput<'a> {
    /// Owning session identifier.
    /// 所属会话标识。
    pub session_id: SessionId,
    /// Full transcript history for the session.
    /// 该会话的完整转录历史。
    pub messages: &'a [SessionMessageEntry],
    /// Runtime-state snapshot used to resolve the current context start point.
    /// 用于解析当前上下文起点的运行时状态快照。
    pub runtime_state: &'a SessionRuntimeStateEntry,
}

/// Output produced by the context compaction skill before the UI assigns the final summary message id.
/// 由上下文压缩技能产出、且在界面分配最终摘要消息标识前的结果。
#[derive(Debug, Clone)]
pub struct CompactionSkillResult {
    /// Hidden summary record that should be inserted into the transcript first.
    /// 应先插入转录历史的隐藏摘要记录。
    pub summary_record: SessionDerivedRecord,
    /// Optional source tail message identifier covered by the new summary.
    /// 新摘要覆盖的可选源历史尾消息标识。
    pub source_until_message_id: Option<String>,
    /// Stable compaction identifier prepared by the skill.
    /// 技能预先生成的稳定压缩标识。
    pub compaction_id: String,
    /// Created-at timestamp for the new checkpoint.
    /// 新检查点的创建时间戳。
    pub created_at: String,
}

impl CompactionSkillResult {
    /// Materializes the final compaction checkpoint once the hidden summary message has been persisted.
    /// 当隐藏摘要消息已经持久化后，物化最终压缩检查点。
    pub fn materialize_checkpoint(&self, summary_message_id: String) -> SessionCompactionEntry {
        SessionCompactionEntry {
            id: self.compaction_id.clone(),
            source_until_message_id: self.source_until_message_id.clone(),
            context_start_message_id: Some(summary_message_id.clone()),
            summary_message_id: Some(summary_message_id),
            created_at: self.created_at.clone(),
        }
    }
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
                "当前会话绑定到项目 `{}`，根目录为 `{}`，后续 explore/triage 将优先围绕该目录展开。",
                project.title, project.path
            )
        } else if let Some(project_id) = input.project_id {
            format!(
                "当前会话绑定到项目标识 `{project_id}`，但项目详情暂未加载，后续 explore/triage 将在工作区内兜底。"
            )
        } else {
            format!(
                "当前会话未绑定到显式项目目录，后续 explore/triage 将围绕工作区 `{}` 展开。",
                input.workspace_path
            )
        };

        SessionDerivedRecord {
            kind: SessionMessageKind::Assistant,
            record_kind: SessionMessageRecordKind::ExploreOutput,
            title: String::from("Explore 输出"),
            body,
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
                .unwrap_or_else(|| String::from("模型本轮未返回可见文本输出。"));
            format!(
                "本轮执行成功完成。准备摘要：{}。输出摘要：{}",
                input
                    .report
                    .preparation
                    .as_ref()
                    .map(|preparation| preparation.summary_rows().join("；"))
                    .unwrap_or_else(|| String::from("无")),
                output_preview
            )
        } else {
            format!(
                "本轮执行失败。错误摘要：{}",
                input
                    .report
                    .error
                    .as_deref()
                    .map(|error| truncate_preview_lines(error, 2, 180))
                    .unwrap_or_else(|| String::from("未知错误"))
            )
        };

        SessionDerivedRecord {
            kind: SessionMessageKind::Assistant,
            record_kind: SessionMessageRecordKind::SummaryOutput,
            title: String::from("Summary 输出"),
            body,
        }
    }
}

/// Stateless context compaction skill used when the model-facing history grows too long.
/// 当面向模型的历史过长时使用的无状态上下文压缩技能。
#[derive(Debug, Clone, Default)]
struct CompactionSkill;

impl CompactionSkill {
    /// Executes the compaction step and returns a hidden summary plus checkpoint metadata when compaction is needed.
    /// 执行压缩步骤，并在需要压缩时返回隐藏摘要与检查点元数据。
    fn execute(&self, input: CompactionSkillInput<'_>) -> Option<CompactionSkillResult> {
        let context_start_index = input
            .runtime_state
            .context_start_message_id
            .as_deref()
            .and_then(|context_start_message_id| {
                input
                    .messages
                    .iter()
                    .position(|message| message.id == context_start_message_id)
            })
            .unwrap_or(0);
        let context_candidates = input.messages[context_start_index..]
            .iter()
            .filter(|message| message.contributes_to_context())
            .cloned()
            .collect::<Vec<_>>();

        if context_candidates.len() < 10 {
            return None;
        }

        let retained_tail_count = 4usize;
        if context_candidates.len() <= retained_tail_count {
            return None;
        }

        let compaction_source =
            &context_candidates[..context_candidates.len() - retained_tail_count];
        let source_until_message_id = compaction_source.last().map(|message| message.id.clone());
        let summary_body = compaction_source
            .iter()
            .map(|message| {
                let label = match message.record_kind {
                    SessionMessageRecordKind::Transcript => message.kind.label(),
                    SessionMessageRecordKind::ThinkingOutput => "THINKING",
                    SessionMessageRecordKind::AssistantOutput => "OUTPUT",
                    SessionMessageRecordKind::ToolCallOutput => "TOOL_CALL",
                    SessionMessageRecordKind::ToolResultOutput => "TOOL_RESULT",
                    SessionMessageRecordKind::SummaryOutput => "SUMMARY",
                    SessionMessageRecordKind::ExploreOutput => "EXPLORE",
                    SessionMessageRecordKind::CompactionSummary => "COMPACTION",
                    SessionMessageRecordKind::TitleOutput => "TITLE",
                };
                format!(
                    "{} · {}：{}",
                    label,
                    message.title,
                    truncate_preview_lines(&message.body, 1, 88)
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        let created_at = current_unix_millis_text();

        Some(CompactionSkillResult {
            summary_record: SessionDerivedRecord {
                kind: SessionMessageKind::Assistant,
                record_kind: SessionMessageRecordKind::CompactionSummary,
                title: String::from("上下文压缩摘要"),
                body: format!(
                    "以下内容用于后续上下文重建，用户界面默认隐藏。\n\n{}",
                    summary_body
                ),
            },
            source_until_message_id,
            compaction_id: format!("session-{}-compaction-{created_at}", input.session_id),
            created_at,
        })
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

/// Returns the current Unix-milliseconds timestamp as a stable decimal string.
/// 以稳定十进制字符串形式返回当前 Unix 毫秒时间戳。
fn current_unix_millis_text() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .to_string()
}
