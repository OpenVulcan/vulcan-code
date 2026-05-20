//! Session-derived title, explore, summary, and compaction skills used by the workbench runtime.
//! 工作台运行时使用的会话派生标题、探索、摘要与压缩技能。

use crate::agent_runtime::ExecutionRunReport;
use crate::context_budget::ContextCompactionPolicy;
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
    /// Policy controlling the retained tail and pruning behavior for this compaction pass.
    /// 控制本次压缩保留尾部与剪枝行为的策略。
    pub policy: ContextCompactionPolicy,
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
                .unwrap_or_else(|| String::from("The model did not return visible text output for this run."));
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
        }
    }
}

/// Stateless context compaction skill used when the model-facing history grows too long.
/// 当面向模型的历史过长时使用的无状态上下文压缩技能。
#[derive(Debug, Clone, Default)]
struct CompactionSkill;

/// Transcript-index segment representing one user turn plus following assistant-visible entries.
/// 表示一个用户轮次及其后续助手可见条目的转录索引段。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SessionTailSegment {
    /// Inclusive segment start index.
    /// 段起始索引，包含该位置。
    start: usize,
    /// Exclusive segment end index.
    /// 段结束索引，不包含该位置。
    end: usize,
}

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

        let retained_tail_start = recent_tail_start_index(&context_candidates, input.policy);
        if retained_tail_start == 0 {
            return None;
        }

        let compaction_source = &context_candidates[..retained_tail_start];
        let source_until_message_id = compaction_source.last().map(|message| message.id.clone());
        let summary_body = compaction_source
            .iter()
            .map(|message| compaction_summary_line_for_message(message, input.policy.prune))
            .collect::<Vec<_>>()
            .join("\n");
        let created_at = current_unix_millis_text();

        Some(CompactionSkillResult {
            summary_record: SessionDerivedRecord {
                kind: SessionMessageKind::Assistant,
                record_kind: SessionMessageRecordKind::CompactionSummary,
                title: String::from("Context Compaction Summary"),
                body: format!(
                    "The following summary is retained for future context reconstruction and stays hidden in the default UI.\n\n{}",
                    summary_body
                ),
            },
            source_until_message_id,
            compaction_id: format!("session-{}-compaction-{created_at}", input.session_id),
            created_at,
        })
    }
}

/// Selects the first context message that should remain verbatim after compaction.
/// 选择压缩后应按原样保留的第一条上下文消息。
fn recent_tail_start_index(
    messages: &[SessionMessageEntry],
    policy: ContextCompactionPolicy,
) -> usize {
    if messages.is_empty() {
        return 0;
    }

    let target_turns = policy.tail_turns.max(1usize);
    let segments = session_tail_segments(messages);
    let mut retained_start = messages.len().saturating_sub(1);
    let mut retained_tokens = 0usize;
    let mut retained_turns = 0usize;

    for segment in segments.iter().rev() {
        if retained_turns >= target_turns {
            break;
        }
        let segment_tokens = estimated_session_tail_tokens(&messages[segment.start..segment.end]);
        if retained_turns == 0 || retained_tokens + segment_tokens <= policy.preserve_recent_tokens
        {
            retained_start = segment.start;
            retained_tokens = retained_tokens.saturating_add(segment_tokens);
            retained_turns += 1;
        } else {
            break;
        }
    }

    retained_start
}

/// Builds transcript tail segments at user-turn boundaries.
/// 按用户轮次边界构建转录尾部段。
fn session_tail_segments(messages: &[SessionMessageEntry]) -> Vec<SessionTailSegment> {
    if messages.is_empty() {
        return Vec::new();
    }

    let mut user_starts = messages
        .iter()
        .enumerate()
        .filter_map(|(index, message)| message_is_user_turn_boundary(message).then_some(index))
        .collect::<Vec<_>>();
    if user_starts.is_empty() {
        return vec![SessionTailSegment {
            start: 0,
            end: messages.len(),
        }];
    }
    if user_starts[0] != 0 {
        user_starts.insert(0, 0);
    }

    user_starts
        .iter()
        .enumerate()
        .map(|(position, start)| SessionTailSegment {
            start: *start,
            end: user_starts
                .get(position + 1)
                .copied()
                .unwrap_or(messages.len()),
        })
        .collect()
}

/// Returns whether one transcript entry starts a user-authored turn.
/// 返回一条转录项是否标记一个用户发起轮次的开始。
fn message_is_user_turn_boundary(message: &SessionMessageEntry) -> bool {
    message.kind == SessionMessageKind::User
        && message.record_kind == SessionMessageRecordKind::Transcript
}

/// Estimates the approximate token footprint of retained transcript entries.
/// 估算保留转录项的近似 Token 占用。
fn estimated_session_tail_tokens(messages: &[SessionMessageEntry]) -> usize {
    messages.iter().map(estimate_session_message_tokens).sum()
}

/// Estimates one transcript entry with a lightweight text heuristic.
/// 使用轻量文本启发式估算一条转录项。
fn estimate_session_message_tokens(message: &SessionMessageEntry) -> usize {
    estimate_text_tokens(&message.title)
        + estimate_text_tokens(&message.body)
        + message
            .metadata
            .as_ref()
            .and_then(|metadata| serde_json::to_string(metadata).ok())
            .map(|metadata| estimate_text_tokens(&metadata))
            .unwrap_or_default()
        + 16usize
}

/// Estimates plain text using the same character heuristic as the context budget path.
/// 使用与上下文预算路径一致的字符启发式估算纯文本。
fn estimate_text_tokens(text: &str) -> usize {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return 0;
    }
    ((trimmed.chars().count() + 3usize) / 4usize).max(1usize)
}

/// Builds one compaction summary line and optionally prunes old tool-output bodies.
/// 构建单条压缩摘要行，并可选剪枝旧工具输出正文。
fn compaction_summary_line_for_message(message: &SessionMessageEntry, prune_tools: bool) -> String {
    let label = canonical_compaction_message_label(message);
    let title = canonical_compaction_message_title(message);
    let preview = if prune_tools
        && matches!(
            message.record_kind,
            SessionMessageRecordKind::ToolCallOutput | SessionMessageRecordKind::ToolResultOutput
        ) {
        String::from("Tool output was compacted into this placeholder.")
    } else {
        truncate_preview_lines(&message.body, 1, 88)
    };

    if title == label {
        format!("{label}: {preview}")
    } else {
        format!("{label} · {title}: {preview}")
    }
}

/// Resolves the stable English label used for model-facing compaction summaries.
/// 解析供面向模型的压缩摘要使用的稳定英文标签。
fn canonical_compaction_message_label(message: &SessionMessageEntry) -> &'static str {
    match message.record_kind {
        SessionMessageRecordKind::Transcript => message.kind.label(),
        SessionMessageRecordKind::ThinkingOutput => "THINKING",
        SessionMessageRecordKind::AssistantOutput => "OUTPUT",
        SessionMessageRecordKind::ToolCallOutput => "TOOL_CALL",
        SessionMessageRecordKind::ToolResultOutput => "TOOL_RESULT",
        SessionMessageRecordKind::SummaryOutput => "SUMMARY",
        SessionMessageRecordKind::ExploreOutput => "EXPLORE",
        SessionMessageRecordKind::CompactionSummary => "COMPACTION",
        SessionMessageRecordKind::TitleOutput => "TITLE",
    }
}

/// Resolves the stable English headline used for model-facing compaction summaries without depending on GUI locale.
/// 解析供面向模型的压缩摘要使用的稳定英文标题，并且不依赖图形界面语言。
fn canonical_compaction_message_title(message: &SessionMessageEntry) -> String {
    match message.record_kind {
        SessionMessageRecordKind::Transcript => match message.kind {
            SessionMessageKind::User => String::from("User Prompt"),
            SessionMessageKind::Assistant => String::from("Assistant Message"),
            SessionMessageKind::Error => String::from("System Error"),
        },
        SessionMessageRecordKind::ThinkingOutput => String::from("Thinking"),
        SessionMessageRecordKind::AssistantOutput => String::from("Output"),
        SessionMessageRecordKind::ToolCallOutput => canonical_tool_summary_title(message, "Tool Call"),
        SessionMessageRecordKind::ToolResultOutput => {
            canonical_tool_summary_title(message, "Tool Result")
        }
        SessionMessageRecordKind::SummaryOutput => String::from("Execution Summary"),
        SessionMessageRecordKind::ExploreOutput => String::from("Explore Context"),
        SessionMessageRecordKind::CompactionSummary => String::from("Context Compaction Summary"),
        SessionMessageRecordKind::TitleOutput => String::from("Title Update"),
    }
}

/// Builds a stable English tool summary title from normalized metadata first and legacy persisted titles second.
/// 优先根据规范化元数据、其次根据旧版持久化标题构建稳定英文工具摘要标题。
fn canonical_tool_summary_title(message: &SessionMessageEntry, prefix: &str) -> String {
    let tool_name = message
        .metadata
        .as_ref()
        .and_then(|metadata| metadata.get("tool_name"))
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| legacy_tool_name_from_title(&message.title));

    tool_name
        .map(|tool_name| format!("{prefix} {tool_name}"))
        .unwrap_or_else(|| prefix.to_string())
}

/// Extracts a best-effort tool name from legacy persisted transcript headlines.
/// 从旧版持久化转录标题中提取最佳努力工具名称。
fn legacy_tool_name_from_title(title: &str) -> Option<String> {
    let trimmed = title.trim();
    if let Some(tool_name) = trimmed
        .strip_prefix("调用 ")
        .and_then(|value| value.strip_suffix(" 工具"))
    {
        return (!tool_name.trim().is_empty()).then(|| tool_name.trim().to_string());
    }
    if let Some(tool_name) = trimmed.strip_prefix("工具返回 ") {
        return (!tool_name.trim().is_empty()).then(|| tool_name.trim().to_string());
    }
    if let Some(tool_name) = trimmed.strip_prefix("Tool Call ") {
        return (!tool_name.trim().is_empty()).then(|| tool_name.trim().to_string());
    }
    if let Some(tool_name) = trimmed.strip_prefix("Tool Result ") {
        return (!tool_name.trim().is_empty()).then(|| tool_name.trim().to_string());
    }

    None
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

#[cfg(test)]
mod tests {
    //! Session skill tests for compaction policy and pruning behavior.
    //! 会话技能测试，覆盖压缩策略与剪枝行为。

    use super::*;

    /// Builds one compact transcript entry for compaction helper tests.
    /// 为压缩辅助函数测试构建一条紧凑转录项。
    fn test_message(
        id: &str,
        kind: SessionMessageKind,
        record_kind: SessionMessageRecordKind,
        body: &str,
    ) -> SessionMessageEntry {
        SessionMessageEntry {
            id: id.to_string(),
            kind,
            record_kind,
            visible: true,
            title: id.to_string(),
            body: body.to_string(),
            metadata: None,
            created_at: String::from("1"),
        }
    }

    /// Verifies tail-turn retention keeps the requested recent user turn window.
    /// 验证尾部轮次保留会保留指定的最近用户轮次窗口。
    #[test]
    fn recent_tail_start_index_respects_tail_turns() {
        let messages = vec![
            test_message(
                "u1",
                SessionMessageKind::User,
                SessionMessageRecordKind::Transcript,
                "one",
            ),
            test_message(
                "a1",
                SessionMessageKind::Assistant,
                SessionMessageRecordKind::AssistantOutput,
                "two",
            ),
            test_message(
                "u2",
                SessionMessageKind::User,
                SessionMessageRecordKind::Transcript,
                "three",
            ),
            test_message(
                "a2",
                SessionMessageKind::Assistant,
                SessionMessageRecordKind::AssistantOutput,
                "four",
            ),
        ];
        let policy = ContextCompactionPolicy {
            tail_turns: 1,
            preserve_recent_tokens: 10_000,
            ..ContextCompactionPolicy::default()
        };

        assert_eq!(recent_tail_start_index(&messages, policy), 2);
    }

    /// Verifies the retained transcript tail starts at a user turn instead of an assistant tool-result record.
    /// 验证保留的转录尾部会从用户轮次开始，而不是从助手工具结果记录中间开始。
    #[test]
    fn recent_tail_start_index_does_not_split_tool_turn() {
        let messages = vec![
            test_message(
                "u1",
                SessionMessageKind::User,
                SessionMessageRecordKind::Transcript,
                "旧问题",
            ),
            test_message(
                "a1",
                SessionMessageKind::Assistant,
                SessionMessageRecordKind::AssistantOutput,
                "旧回答",
            ),
            test_message(
                "u2",
                SessionMessageKind::User,
                SessionMessageRecordKind::Transcript,
                "读取文件",
            ),
            test_message(
                "call",
                SessionMessageKind::Assistant,
                SessionMessageRecordKind::ToolCallOutput,
                "调用 read_file",
            ),
            test_message(
                "result",
                SessionMessageKind::Assistant,
                SessionMessageRecordKind::ToolResultOutput,
                &"很大的工具输出".repeat(120),
            ),
        ];
        let policy = ContextCompactionPolicy {
            tail_turns: 1,
            preserve_recent_tokens: 8,
            ..ContextCompactionPolicy::default()
        };

        assert_eq!(recent_tail_start_index(&messages, policy), 2);
    }

    /// Verifies old tool output bodies can be pruned into compact placeholders.
    /// 验证旧工具输出正文可被剪枝为紧凑占位内容。
    #[test]
    fn compaction_summary_line_prunes_tool_outputs() {
        let message = test_message(
            "tool",
            SessionMessageKind::Assistant,
            SessionMessageRecordKind::ToolResultOutput,
            &"large tool output ".repeat(40),
        );

        let line = compaction_summary_line_for_message(&message, true);

        assert!(line.contains("Tool output was compacted"));
        assert!(!line.contains("large tool output large tool output"));
    }

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
            mcp_discovery_rows: Vec::new(),
            mcp_discovery_errors: Vec::new(),
            warnings: Vec::new(),
            error: None,
            controller_events: Vec::new(),
        };

        let record = skill.execute(SummarySkillInput { report: &report });

        assert_eq!(record.record_kind, SessionMessageRecordKind::SummaryOutput);
        assert!(record.body.contains("Output summary: 这是给用户的最终输出。"));
        assert!(!record.body.contains("准备摘要"));
        assert!(!record.body.contains("会话：#"));
    }

    /// Verifies compaction summaries always use stable English structure even when source transcript titles are localized for the GUI.
    /// 验证即使源转录标题为了图形界面而本地化，压缩摘要仍会始终使用稳定英文结构。
    #[test]
    fn compaction_summary_line_uses_canonical_english_titles() {
        let message = SessionMessageEntry {
            id: String::from("tool-1"),
            kind: SessionMessageKind::Assistant,
            record_kind: SessionMessageRecordKind::ToolResultOutput,
            visible: true,
            title: String::from("工具返回"),
            body: String::from("## Tool Result\n\nRead completed."),
            metadata: Some(serde_json::json!({
                "tool_name": "read_file"
            })),
            created_at: String::from("1"),
        };

        let line = compaction_summary_line_for_message(&message, false);

        assert!(line.starts_with("TOOL_RESULT · Tool Result read_file:"));
        assert!(!line.contains("工具返回"));
    }
}
