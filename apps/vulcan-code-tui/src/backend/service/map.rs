// Service DTO → client model projection for the conversation surface.
// 会话界面的服务 DTO → 客户端模型投影。
//
// Inputs are the shared `vulcan-service-api` DTOs and `vulcan-session-core` v1 conversation
// events, already parsed strictly at the transport boundary; only Git snapshots, whose DTO lives
// in the service kernel, are still read from JSON. Only what the TUI renders is projected.
// 输入是已在传输边界严格解析的共享 `vulcan-service-api` DTO 与 `vulcan-session-core` v1 会话事件；
// 只有 DTO 位于服务内核的 Git 快照仍从 JSON 读取。只投影 TUI 渲染所需的内容。

use serde_json::{Value, json};
use vulcan_change_set::{
    ChangeSetFileChangeKind, PersistentChangeSetFile, PersistentChangeSetHunk,
};
use vulcan_service_api as api;
use vulcan_session_core::v1::lifetime_usage::SessionUsageBucket;
use vulcan_session_core::v1::service_dto::ConversationEventSummary;
use vulcan_session_core::v1::{self as core, EventBody as CoreEvent};

use super::wire::*;
use crate::model::*;

// ---- Workbench ----

/// Projects one service project into the sidebar model.
/// 将一个服务项目投影为侧栏模型。
pub fn project(project: &api::ProjectSummary) -> Project {
    Project {
        id: project.id.clone(),
        name: project.name.clone(),
        path: project.path.clone(),
        pinned: project.pinned,
    }
}

/// Projects service project rows.
/// 投影服务项目行。
pub fn projects(list: &[api::ProjectSummary]) -> Vec<Project> {
    list.iter().map(project).collect()
}

/// Run phase from `runPhase`, falling back to `status` (`conversationIsRunning` in the WebUI).
/// 由 `runPhase` 推导运行阶段，缺省时回退到 `status`（对应 WebUI 的 `conversationIsRunning`）。
pub fn run_phase(phase: &str, status: &str) -> RunPhase {
    // Lifecycle status, normalized like the WebUI.
    // 按 WebUI 方式规范化的生命周期状态。
    let status = normalized(status);
    match normalized(phase).as_str() {
        "" => match status.as_str() {
            "running" => RunPhase::Running,
            "queued" => RunPhase::Queued,
            "failed" => RunPhase::Failed,
            "completed" => RunPhase::Completed,
            _ => RunPhase::Idle,
        },
        "idle" | "ready" | "stopped" => {
            if status == "completed" {
                RunPhase::Completed
            } else if status == "failed" {
                RunPhase::Failed
            } else {
                RunPhase::Idle
            }
        }
        "completed" => RunPhase::Completed,
        "failed" => RunPhase::Failed,
        "queued" => RunPhase::Queued,
        "memoryrecall" => RunPhase::MemoryRecall,
        _ => RunPhase::Running,
    }
}

/// Projects one service conversation into the sidebar summary.
/// 将一个服务会话投影为侧栏摘要。
pub fn conversation(conversation: &api::ConversationSummary) -> ConversationSummary {
    ConversationSummary {
        id: conversation.id.clone(),
        title: conversation.title.clone(),
        project_id: conversation.project_id.clone(),
        run_phase: run_phase(&conversation.run_phase, &conversation.status),
        last_updated_label: conversation.last_updated_label.clone(),
        last_updated_at_ms: conversation
            .last_updated_at
            .trim()
            .parse::<i64>()
            .ok()
            .filter(|ms| *ms > 0),
        unread: conversation.unread,
        attention: conversation.attention.is_some(),
        dedicated: conversation.dedicated,
    }
}

/// One selectable model route: the provider, the model and the credential connection.
/// 一条可选模型路线：供应商、模型与凭据连接。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelRoute {
    /// Provider identity from the authoritative selectable route.
    /// 来自权威可选路线的提供方身份。
    pub provider_id: String,
    /// Model identity within the selected provider catalog.
    /// 所选提供方目录内的模型身份。
    pub model_id: String,
    /// Credential connection identity used to route this model request.
    /// 用于路由该模型请求的凭据连接身份。
    pub connection_id: String,
}

impl ModelRoute {
    /// Stable key, the same tuple the WebUI model picker serializes.
    /// 稳定键，与 WebUI 模型选择器序列化的元组相同。
    pub fn key(&self) -> String {
        json!([self.provider_id, self.model_id, self.connection_id]).to_string()
    }

    /// Parses the exact provider/model/connection tuple serialized by the model picker.
    /// 解析模型选择器序列化的准确提供方、模型及连接元组。
    pub fn from_key(key: &str) -> Option<ModelRoute> {
        let parts: Vec<String> = serde_json::from_str(key).ok()?;
        let [provider_id, model_id, connection_id] = <[String; 3]>::try_from(parts).ok()?;
        Some(ModelRoute {
            provider_id,
            model_id,
            connection_id,
        })
    }
}

/// Picker rows from `providers` (`modelPickerChoicesFromProviders`), plus the active route.
/// 由 `providers` 得到的选择器行（对应 `modelPickerChoicesFromProviders`），以及当前激活路线。
pub fn models(providers: &[api::ProviderSummary]) -> (Vec<ModelInfo>, Option<String>) {
    let mut rows = Vec::new();
    // Route key of the active model; a preferred connection wins over the first active one.
    // 激活模型的路线键；首选连接优先于第一个激活项。
    let mut active = None;
    for provider in providers {
        let provider_id = provider
            .source_provider_id
            .clone()
            .unwrap_or_else(|| provider.id.clone());
        let connection_id = provider.connection_id.clone().unwrap_or_default();
        let provider_name = provider
            .provider_display_name
            .clone()
            .unwrap_or_else(|| provider.name.clone());
        let label = match &provider.account_name {
            Some(account) => format!("{provider_name} · {account}"),
            None => provider_name,
        };
        for model in provider.models.iter().filter(|model| model.enabled) {
            let route = ModelRoute {
                provider_id: provider_id.clone(),
                model_id: model.id.clone(),
                connection_id: connection_id.clone(),
            };
            let key = route.key();
            if model.active && (active.is_none() || model.preferred_connection) {
                active = Some(key.clone());
            }
            rows.push(ModelInfo {
                label: if model.name.is_empty() {
                    model.id.clone()
                } else {
                    model.name.clone()
                },
                provider: label.clone(),
                provider_id: provider_id.clone(),
                model_id: model.id.clone(),
                context_window: model.context_limit.unwrap_or(0) as u64,
                id: key,
            });
        }
    }
    rows.sort_by(|a, b| a.label.cmp(&b.label).then(a.provider.cmp(&b.provider)));
    (rows, active)
}

/// Route key of one session request configuration.
/// 一份会话请求配置的路线键。
pub fn session_route(request: &api::ConversationRequestConfigV1) -> Option<String> {
    Some(
        ModelRoute {
            provider_id: request.provider.clone()?,
            model_id: request.model.clone()?,
            connection_id: request.connection_id.clone().unwrap_or_default(),
        }
        .key(),
    )
}

/// Maps a declared service reasoning-effort value to the local selector enum.
/// 将声明的服务推理强度值映射为本地选择器枚举。
pub fn effort(value: &str) -> Option<ReasoningEffort> {
    Some(match normalized(value).as_str() {
        "off" | "none" => ReasoningEffort::Off,
        "adaptive" => ReasoningEffort::Adaptive,
        "minimal" => ReasoningEffort::Minimal,
        "low" => ReasoningEffort::Low,
        "medium" => ReasoningEffort::Medium,
        "high" => ReasoningEffort::High,
        "xhigh" => ReasoningEffort::XHigh,
        "max" => ReasoningEffort::Max,
        "ultra" => ReasoningEffort::Ultra,
        _ => return None,
    })
}

// ---- Planning state ----

/// Maps one pending question and its exact request identity.
/// 映射一个待回复提问及其准确请求身份。
/// `requester` names the child agent when the request is surfaced in its root conversation.
/// `requester` 在请求展示于根会话时标明发起的子代理。
pub fn pending_question(
    question: &api::PendingQuestionSnapshot,
    requester: Option<String>,
) -> PendingQuestion {
    PendingQuestion {
        id: question.id.clone(),
        questions: question
            .questions
            .iter()
            .map(|q| Question {
                header: q.header.clone(),
                question: q.question.clone(),
                options: q
                    .options
                    .iter()
                    .map(|o| QuestionOption {
                        label: o.label.clone(),
                        description: o.description.clone(),
                    })
                    .collect(),
                multiple: q.multiple,
            })
            .collect(),
        approval: question.approval.as_ref().map(pending_approval),
        environment: question
            .environment
            .as_ref()
            .map(|request| PendingEnvironmentRequest {
                purpose: request.purpose.clone(),
                names: request
                    .variables
                    .iter()
                    .map(|variable| variable.name.clone())
                    .collect(),
            }),
        requester,
    }
}

/// Maps the permission facts of one tool-call approval.
/// 映射一次工具调用审批的权限事实。
fn pending_approval(approval: &api::PendingApprovalSnapshot) -> PendingApproval {
    PendingApproval {
        tool_name: approval.tool_name.clone(),
        permission_mode: approval.permission_mode,
        subject: match &approval.subject {
            api::PendingApprovalSubject::Command { text } => ApprovalSubject::Command(text.clone()),
            api::PendingApprovalSubject::FileWrite { paths } => {
                ApprovalSubject::FileWrite(paths.clone())
            }
            api::PendingApprovalSubject::Opaque => ApprovalSubject::Opaque,
            api::PendingApprovalSubject::Configuration {
                target_kind,
                target,
                action,
                changes,
                notices,
            } => ApprovalSubject::Configuration(Box::new(crate::model::ApprovalConfiguration {
                target_kind: target_kind.clone(),
                target: target.clone(),
                action: action.clone(),
                changes: changes
                    .iter()
                    .map(|change| crate::model::ApprovalConfigurationField {
                        field: change.field.clone(),
                        before: change.before.clone(),
                        after: change.after.clone(),
                    })
                    .collect(),
                notices: notices.clone(),
            })),
        },
        high_risk: approval.high_risk.clone(),
        judge: approval.judge.as_ref().map(|judge| match judge {
            api::PendingApprovalJudge::Scored { label } => ApprovalJudge::Scored(label.clone()),
            api::PendingApprovalJudge::Unavailable { reason } => {
                ApprovalJudge::Unavailable(reason.clone())
            }
        }),
        review: approval.review.as_ref().map(|review| match review {
            api::PendingApprovalReview::Reviewed { verdict, reason } => ApprovalReview::Reviewed {
                verdict: verdict.clone(),
                reason: reason.clone(),
            },
            api::PendingApprovalReview::Unavailable { reason } => {
                ApprovalReview::Unavailable(reason.clone())
            }
        }),
        deadline_unix_ms: approval.deadline_unix_ms,
        allow_label: approval.allow_label.clone(),
        deny_label: approval.deny_label.clone(),
    }
}

/// Maps queued service prompts; a prompt without a preview shows its text.
/// 映射排队中的服务提示；没有预览的提示显示其正文。
pub fn pending_prompts(list: &[api::PendingPromptSummary]) -> Vec<PendingPrompt> {
    list.iter()
        .map(|prompt| PendingPrompt {
            preview: if prompt.preview.is_empty() {
                prompt.prompt.clone()
            } else {
                prompt.preview.clone()
            },
        })
        .collect()
}

/// Maps published planning entries into the session progress panel.
/// 将发布的规划条目映射到会话进度面板。
pub fn todo(list: &api::SessionTodolistSummary) -> Vec<TodoItem> {
    list.items
        .iter()
        .map(|item| TodoItem {
            content: item.step.clone(),
            status: match normalized(&item.status).as_str() {
                "completed" | "done" => TodoStatus::Completed,
                "inprogress" | "running" => TodoStatus::InProgress,
                _ => TodoStatus::Pending,
            },
        })
        .collect()
}

/// Maps the published plan state into the local execution mode.
/// 将发布的计划状态映射为本地执行模式。
pub fn plan_mode(plan: &api::SessionPlanModeSummary) -> ExecutionMode {
    if normalized(&plan.execution_mode) == "plan" {
        ExecutionMode::Plan
    } else {
        ExecutionMode::Build
    }
}

/// Projects service token usage and its budget into the context meter.
/// 将服务令牌用量及预算投影为上下文计量表。
pub fn context_usage(usage: &api::ContextUsageSnapshot) -> ContextUsage {
    ContextUsage {
        used_tokens: usage.used_tokens as u64,
        max_tokens: usage.max_tokens.unwrap_or(0) as u64,
    }
}

/// Maps service command sessions into terminal panel rows.
/// 将服务命令会话映射为终端面板行。
pub fn command_sessions(list: &[api::CommandSessionSummary]) -> Vec<CommandSession> {
    list.iter()
        .map(|session| {
            let status = normalized(&session.status);
            let screen = session.screen_text.as_deref().unwrap_or(&session.preview);
            CommandSession {
                id: session.id.clone(),
                title: session
                    .name
                    .clone()
                    .unwrap_or_else(|| session.command.clone()),
                command: session.command.clone(),
                running: matches!(status.as_str(), "running" | "starting"),
                exit_code: session.exit_code,
                screen: screen.lines().map(str::to_string).collect(),
            }
        })
        .collect()
}

/// Subagent navigation tasks → subagent rows.
/// 子代理导航任务 → 子代理行。
pub fn agents(navigation: &api::SubagentNavigationSnapshot) -> Vec<AgentInfo> {
    navigation
        .tasks
        .iter()
        .map(|task| AgentInfo {
            id: task.task_id.clone(),
            name: if task.label.is_empty() {
                task.public_task_id.clone()
            } else {
                task.label.clone()
            },
            profile: task.agent_profile_id.clone().unwrap_or_default(),
            task: task.description.clone().unwrap_or_default(),
            state: AgentState::from_run_state(&normalized(&task.run_state)),
            prompt: String::new(),
            pending: task
                .pending_messages
                .iter()
                .map(|message| AgentMessage {
                    steering: message.steering,
                    text: message.message.clone(),
                })
                .collect(),
            activity: Vec::new(),
            result: None,
            usage: UsageBucket::default(),
        })
        .collect()
}

// ---- Insight ----

/// Maps one published usage bucket without starting any model request.
/// 映射一份已发布用量，不发起任何模型请求。
fn usage_bucket(bucket: &SessionUsageBucket) -> UsageBucket {
    UsageBucket {
        requests: bucket.request_count,
        unknown_usage: bucket.unknown_usage_count,
        tool_calls: bucket.tool_call_count,
        input: bucket.input_tokens.sum,
        uncached_input: bucket.uncached_input_tokens.sum,
        cache_read: bucket.cache_read_tokens.sum,
        cache_write: bucket.cache_write_tokens.sum,
        output: bucket.output_tokens.sum,
        reasoning: bucket.reasoning_tokens.sum,
        first_token_ms: bucket.first_token_ms.sum,
        first_token_samples: bucket.first_token_ms.samples,
        duration_ms: bucket.request_duration_ms.sum,
        duration_samples: bucket.request_duration_ms.samples,
        measured_output: bucket.measured_output_tokens,
        measured_output_ms: bucket.output_duration_ms.sum,
    }
}

/// Session lifetime usage; children are keyed by child session id until the worker relabels them
/// with subagent names.
/// 会话生命周期用量；子项以子会话 ID 为键，之后由工作线程替换为子代理名。
pub fn lifetime(usage: &api::SessionProviderUsageSummary) -> LifetimeUsage {
    let Some(lifetime) = &usage.lifetime else {
        return LifetimeUsage::default();
    };
    LifetimeUsage {
        main: usage_bucket(&lifetime.main),
        children: lifetime
            .children_by_session_id
            .iter()
            .map(|(id, child)| (id.clone(), usage_bucket(&child.usage)))
            .collect(),
    }
}

/// Maps context usage sections into the panel composition breakdown.
/// 将上下文用量分区映射为面板组成明细。
pub fn breakdown(usage: &api::ContextUsageSnapshot) -> Vec<ContextSlice> {
    let mut slices: Vec<ContextSlice> = Vec::new();
    for item in &usage.breakdown {
        let part = match normalized(&item.kind).as_str() {
            "systemprompt" => ContextPart::SystemPrompt,
            "systemtools" | "mcptools" | "skills" | "tools" => ContextPart::Tools,
            "memory" | "vmm" => ContextPart::Memory,
            "attachments" | "media" => ContextPart::Attachments,
            _ => ContextPart::Conversation,
        };
        let tokens = item.tokens as u64;
        match slices.iter_mut().find(|slice| slice.part == part) {
            Some(slice) => slice.tokens += tokens,
            None => slices.push(ContextSlice { part, tokens }),
        }
    }
    slices
}

/// Projects the published goal status, budget and usage into the session panel.
/// 将发布的目标状态、预算及用量投影为会话面板。
pub fn goal(goal: &api::SessionGoalSummary) -> Goal {
    let count = |value: i64| value.max(0) as u64;
    Goal {
        objective: goal.objective.clone(),
        status: goal.status,
        elapsed_ms: count(goal.time_used_seconds) * 1000,
        usage: UsageBucket {
            requests: goal.api_call_count as u64,
            tool_calls: goal.tool_call_count as u64,
            input: count(goal.input_tokens),
            cache_read: count(goal.cached_input_tokens),
            output: count(goal.output_tokens),
            reasoning: count(goal.reasoning_tokens),
            ..UsageBucket::default()
        },
        budget: goal.token_budget.map(count).filter(|budget| *budget > 0),
    }
}

/// Leading number of a display label such as a score or a profile priority; 0 when absent.
/// 分数或画像优先级等显示标签开头的数字；缺失时为 0。
fn label_number<T: std::str::FromStr + Default>(label: &str) -> T {
    // Profile markers arrive as `P1`, `L2` or `W3`; only the number is kept.
    // 画像标记以 `P1`、`L2` 或 `W3` 下发，只保留数字部分。
    label
        .trim()
        .trim_start_matches(|c: char| c.is_ascii_alphabetic())
        .parse()
        .unwrap_or_default()
}

/// Dynamic card tag the service uses for VMM memories past their TTL.
/// 服务用于已超过 TTL 的 VMM 记忆的动态卡片标签。
const EXPIRED_MEMORY_CARD_TAG: &str = "VMM_MEMORY_EXPIRED";

/// Context injection → dynamic memory cards and the static profile.
/// 上下文注入 → 动态记忆卡片与静态画像。
pub fn context_injection(snapshot: &api::ContextInjectionSnapshot) -> ContextInjection {
    let memory = |item: &api::ContextInjectionItemSummary| InjectedMemory {
        // Injections that are not stored memories have no `memoryId`; their dedupe key names them.
        // 非存储记忆的注入没有 `memoryId`；以其去重键标识。
        memory_id: item
            .memory_id
            .map(|id| id.to_string())
            .or_else(|| item.dedupe_key.clone())
            .unwrap_or_default(),
        content: item.content.clone(),
        turn_id: item
            .turn_label
            .clone()
            .or_else(|| item.turn_id.map(|id| id.to_string()))
            .unwrap_or_default(),
        turns_left: item
            .ttl_label
            .chars()
            .filter(char::is_ascii_digit)
            .collect::<String>()
            .parse()
            .unwrap_or(0),
        score: item.score_label.as_deref().map_or(0.0, label_number),
        source: item.source_display.clone().unwrap_or_default(),
        dialogue: None,
    };
    let (expired_cards, active_cards): (Vec<_>, Vec<_>) = snapshot
        .dynamic_cards
        .iter()
        .partition(|card| card.tag == EXPIRED_MEMORY_CARD_TAG);
    let expired = expired_cards
        .into_iter()
        .flat_map(|card| card.items.iter().map(memory))
        .collect();
    let dynamic = active_cards
        .into_iter()
        .map(|card| MemoryCard {
            tag: card.tag.clone(),
            description: card.description.clone(),
            items: card.items.iter().map(memory).collect(),
        })
        .collect();
    let profile = snapshot
        .static_profile
        .iter()
        .flat_map(|profile| &profile.sections)
        .map(|section| ProfileSection {
            user: normalized(&section.target) == "user",
            name: section.label.clone(),
            nodes: section
                .dates
                .iter()
                .flat_map(|date| {
                    let day = date.date.get(..10).unwrap_or(&date.date).to_string();
                    date.nodes.iter().map(move |node| ProfileNode {
                        id: node.id.to_string(),
                        date: day.clone(),
                        priority: label_number(&node.priority),
                        level: label_number(&node.level),
                        weight: label_number(&node.refresh_weight),
                        content: node.content.clone(),
                        source: match normalized(&node.source_kind).as_str() {
                            "systemseed" | "seed" => ProfileSource::SystemSeed,
                            "manualinstruction" | "manual" => ProfileSource::ManualInstruction,
                            _ => ProfileSource::TurnExtract,
                        },
                    })
                })
                .collect(),
        })
        .collect();
    ContextInjection {
        dynamic,
        expired,
        profile,
    }
}

// ---- Conversation events (vulcan-session-core v1) ----

/// Tool output as the model saw it; structured payloads fall back to compact JSON.
/// 模型所见的工具输出；结构化载荷回退为紧凑 JSON。
fn tool_output(output: &core::ToolPayload) -> String {
    let value = serde_json::to_value(output).unwrap_or(Value::Null);
    match value.get("model_output") {
        Some(Value::String(text)) => text.clone(),
        _ => value.to_string(),
    }
}

/// File-level summaries of a tool call's changeset.
/// 工具调用变更集的文件级摘要。
fn changeset(call: &core::ToolCallEvent) -> Vec<ChangesetFile> {
    call.changeset
        .iter()
        .flat_map(|changeset| &changeset.files)
        .map(|file| ChangesetFile {
            path: file
                .new_path
                .clone()
                .or_else(|| file.path.clone())
                .or_else(|| file.old_path.clone())
                .unwrap_or_default(),
            additions: changeset_line_count(file, ChangeSetFileChangeKind::Create, |hunk| {
                hunk.inserted.len()
            }),
            deletions: changeset_line_count(file, ChangeSetFileChangeKind::Delete, |hunk| {
                hunk.deleted.len()
            }),
        })
        .collect()
}

/// Lines one file record adds or removes, matching the WebUI change summary.
/// 一条文件记录新增或删除的行数，与 WebUI 变更摘要一致。
/// `whole` is the kind whose lifecycle text counts entirely; the opposite lifecycle kind counts none.
/// `whole` 为生命周期正文整体计入的类型；相反的生命周期类型不计入。
fn changeset_line_count(
    file: &PersistentChangeSetFile,
    whole: ChangeSetFileChangeKind,
    hunk_lines: impl Fn(&PersistentChangeSetHunk) -> usize,
) -> u32 {
    let lines = match file.change {
        kind if kind == whole => file
            .lifecycle_content
            .as_ref()
            .and_then(|content| content.total_line_count())
            .unwrap_or(0),
        ChangeSetFileChangeKind::Create | ChangeSetFileChangeKind::Delete => 0,
        ChangeSetFileChangeKind::Modify | ChangeSetFileChangeKind::Rename => {
            file.hunks.iter().map(hunk_lines).sum()
        }
    };
    u32::try_from(lines).unwrap_or(u32::MAX)
}

/// Milliseconds since the epoch as a display count; pre-epoch values clamp to zero.
/// 以显示计数表示的纪元毫秒；纪元前的值截为零。
fn millis(value: i64) -> u64 {
    value.max(0) as u64
}

/// One conversation event; events the transcript does not show map to `None`.
/// 一条会话事件；记录区不显示的事件映射为 `None`。
pub fn event(summary: &ConversationEventSummary) -> Option<ConversationEvent> {
    let body = match &summary.event {
        CoreEvent::UserInput(input) => EventBody::UserInput {
            text: input.text.clone(),
            input_source: input.input_source,
            attachments: Vec::new(),
        },
        CoreEvent::AssistantReasoning(reasoning) => EventBody::AssistantReasoning {
            text: reasoning.text.clone(),
            status: reasoning.status,
        },
        CoreEvent::AssistantText(text) => EventBody::AssistantText {
            text: text.text.clone(),
            status: text.status,
            // Captures attach with the sealing revision, so only a streaming text lacks them.
            // 抓取结果随封口修订附着，因此只有流式正文缺少它们。
            code_snapshots: (text.status != AssistantSemanticStatus::Streaming)
                .then(|| text.code_snapshots.clone()),
        },
        CoreEvent::ToolCall(call) => EventBody::ToolCall {
            tool_name: call.tool_name.clone(),
            input: serde_json::to_value(&call.input).unwrap_or(Value::Null),
            output: call.output.as_ref().map(tool_output),
            changeset: changeset(call),
            status: call.status,
            started_at_ms: call.started_at_ms.map(millis),
            finished_at_ms: call.finished_at_ms.map(millis),
            user_wait_ms: call.user_wait_ms.map(millis).unwrap_or(0),
        },
        CoreEvent::DisplayNote(note) => EventBody::DisplayNote {
            severity: note.severity,
            message: display_note_message(note),
        },
        CoreEvent::MemoryRecall(recall) => EventBody::DisplayNote {
            severity: match recall.outcome {
                core::MemoryRecallOutcome::Failed => NoticeSeverity::Warning,
                core::MemoryRecallOutcome::Matched | core::MemoryRecallOutcome::NoMatch => {
                    NoticeSeverity::Info
                }
            },
            message: memory_recall_message(recall),
        },
        // A host wake after background shell jobs finished marks its turn like the WebUI card.
        // 后台 shell 任务结束后的宿主唤醒像 WebUI 卡片一样标记其回合。
        CoreEvent::SystemContext(context)
            if context.context_key == core::document::SHELL_JOB_CONTINUATION_CONTEXT_KEY
                && context.operation == core::SystemContextOperation::Upsert =>
        {
            EventBody::DisplayNote {
                severity: NoticeSeverity::Info,
                message: crate::tr!(
                    "conversation.contextInjection.title",
                    category = crate::tr!("conversation.contextInjection.category.shellJob")
                ),
            }
        }
        CoreEvent::TurnStats(stats) => EventBody::TurnStats {
            outcome: stats.outcome,
            duration_ms: stats.duration_ms.map_or(0, millis),
            total_tokens: stats.buckets.total.usage.total_tokens.unwrap_or(0),
            requests: stats.buckets.total.api_call_count as u32,
            tool_calls: stats.buckets.total.tool_call_count as u32,
        },
        _ => return None,
    };
    Some(ConversationEvent {
        seq: summary.seq.value(),
        created_at_ms: millis(summary.created_at_ms),
        updated_at_ms: millis(summary.updated_at_ms),
        body,
    })
}

/// Text of one display note; dedicated memory lock changes use the shared timeline wording.
/// 一条展示通知的文本；专职记忆锁定变化使用共享时间线文案。
fn display_note_message(note: &core::DisplayNoteEvent) -> String {
    match note.notice_kind {
        core::DisplayNoticeKind::MemoryLocked => crate::tr!("conversation.dedicatedNotice.locked"),
        core::DisplayNoticeKind::MemoryUnlocked => {
            crate::tr!("conversation.dedicatedNotice.unlocked")
        }
        _ => note
            .redacted_detail
            .clone()
            .unwrap_or_else(|| note.reason_code.clone()),
    }
}

/// One-line memory recall note mirroring the WebUI work-process item: title, outcome and queries.
/// 镜像 WebUI 工作过程条目的单行记忆召回通知：标题、结果与检索词。
fn memory_recall_message(recall: &core::MemoryRecallEvent) -> String {
    let outcome = match recall.outcome {
        core::MemoryRecallOutcome::Matched => {
            crate::tr!(
                "conversation.memoryRecall.matched",
                count = recall.memories.len()
            )
        }
        core::MemoryRecallOutcome::NoMatch => crate::tr!("conversation.memoryRecall.noMatch"),
        core::MemoryRecallOutcome::Failed => crate::tr!("conversation.memoryRecall.failed"),
    };
    let mut message = format!(
        "{} · {outcome}",
        crate::tr!("conversation.memoryRecall.title")
    );
    if !recall.queries.is_empty() {
        message.push_str(&format!(
            " · {}: {}",
            crate::tr!("conversation.memoryRecall.queries"),
            recall.queries.join(" · ")
        ));
    }
    message
}

// ---- Git ----

/// Maps the declared Git status code into the file display enum.
/// 将声明的 Git 状态码映射为文件显示枚举。
/// `value` is the protocol value being read or projected.
/// `value` 为正在读取或投影的协议值。
/// Returns `GitFileStatus`.
/// 返回 `GitFileStatus`。
fn git_status(value: &str) -> GitFileStatus {
    match normalized(value).as_str() {
        "added" => GitFileStatus::Added,
        "deleted" => GitFileStatus::Deleted,
        "renamed" => GitFileStatus::Renamed,
        "untracked" => GitFileStatus::Untracked,
        _ => GitFileStatus::Modified,
    }
}

/// Files of one `WorkspaceDiffSnapshotResponse`.
/// 一份 `WorkspaceDiffSnapshotResponse` 中的文件。
/// `snapshot` is the authoritative service snapshot.
/// `snapshot` 为权威服务快照。
/// Returns `Vec<GitFile>`.
/// 返回 `Vec<GitFile>`。
pub fn git_files(snapshot: &Value, staged: bool) -> Vec<GitFile> {
    arr(snapshot, "files")
        .iter()
        .map(|file| GitFile {
            path: opt_str(file, "newPath")
                .or_else(|| opt_str(file, "oldPath"))
                .unwrap_or_default(),
            status: git_status(&str_at(file, "status")),
            staged,
            additions: u64_at(file, "additions") as u32,
            deletions: u64_at(file, "deletions") as u32,
        })
        .collect()
}

/// Wire identity of a changed file, needed by file actions and file details.
/// 变更文件的传输标识，文件操作与文件详情需要它。
#[derive(Debug, Clone)]
pub struct GitFileRef {
    /// Stable file identity supplied by the service diff snapshot.
    /// 服务差异快照提供的稳定文件身份。
    pub key: String,
    /// Original repository-relative path, including rename source paths.
    /// 原始仓库相对路径，包含重命名来源路径。
    pub old_path: String,
    /// Current repository-relative path used for file operations.
    /// 用于文件操作的当前仓库相对路径。
    pub new_path: String,
    /// Service-declared change status used by action payloads.
    /// 动作载荷使用的服务声明变更状态。
    pub status: String,
}

/// Keeps exact old/new paths and status from the service diff snapshot for later actions.
/// 保留服务差异快照中的准确旧新路径及状态，用于后续动作。
/// `snapshot` is the authoritative service snapshot.
/// `snapshot` 为权威服务快照。
/// Returns `Vec<GitFileRef>`.
/// 返回 `Vec<GitFileRef>`。
pub fn git_file_refs(snapshot: &Value) -> Vec<GitFileRef> {
    arr(snapshot, "files")
        .iter()
        .map(|file| GitFileRef {
            key: str_at(file, "key"),
            old_path: str_at(file, "oldPath"),
            new_path: str_at(file, "newPath"),
            status: str_at(file, "status"),
        })
        .collect()
}

/// Maps the bounded published commit history into Git panel entries.
/// 将有界的已发布提交历史映射为 Git 面板条目。
/// `snapshot` is the authoritative service snapshot.
/// `snapshot` 为权威服务快照。
/// `limit` is the maximum rows requested.
/// `limit` 为请求的最大行数。
/// Returns `Vec<GitCommit>`.
/// 返回 `Vec<GitCommit>`。
pub fn git_commits(snapshot: &Value, limit: usize) -> Vec<GitCommit> {
    arr(snapshot, "recentCommits")
        .iter()
        .take(limit)
        .map(|c| GitCommit {
            short_sha: str_at(c, "shortHash"),
            subject: str_at(c, "subject"),
        })
        .collect()
}

/// Parses a unified diff into hunks; `diff --git`/`---`/`+++` headers are skipped.
/// 将统一 diff 解析为块；跳过 `diff --git`/`---`/`+++` 头部。
/// `text` is the service-returned unified diff body.
/// `text` 为服务返回的统一差异正文。
/// Returns `FileDiff`.
/// 返回 `FileDiff`。
pub fn unified_diff(text: &str) -> FileDiff {
    // Hunks.
    // 差异片段集合。
    let mut hunks: Vec<DiffHunk> = Vec::new();
    // Old no; New no.
    // 原始行号；更新后行号。
    let (mut old_no, mut new_no) = (0u32, 0u32);
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("@@") {
            // Numbers.
            // 差异片段行号。
            let numbers = rest.split("@@").next().unwrap_or("");
            // Start.
            // 起始位置。
            let start = |sign: char| {
                numbers
                    .split_whitespace()
                    .find_map(|part| part.strip_prefix(sign))
                    .and_then(|part| part.split(',').next())
                    .and_then(|n| n.parse::<u32>().ok())
                    .unwrap_or(1)
            };
            old_no = start('-');
            new_no = start('+');
            hunks.push(DiffHunk {
                header: line.to_string(),
                lines: Vec::new(),
            });
            continue;
        }
        // Hunk.
        // 当前差异片段。
        let Some(hunk) = hunks.last_mut() else {
            continue;
        };
        // Kind; Text.
        // 已声明的操作类别；当前正文。
        let (kind, text) = match line.chars().next() {
            Some('+') => (DiffLineKind::Add, &line[1..]),
            Some('-') => (DiffLineKind::Delete, &line[1..]),
            Some(' ') => (DiffLineKind::Context, &line[1..]),
            Some('\\') => continue,
            _ => (DiffLineKind::Context, line),
        };
        // Old; New.
        // 原始值；更新值。
        let (old, new) = match kind {
            DiffLineKind::Add => (None, Some(new_no)),
            DiffLineKind::Delete => (Some(old_no), None),
            DiffLineKind::Context => (Some(old_no), Some(new_no)),
        };
        if old.is_some() {
            old_no += 1;
        }
        if new.is_some() {
            new_no += 1;
        }
        hunk.lines.push(DiffLine {
            kind,
            old_no: old,
            new_no: new,
            text: text.to_string(),
        });
    }
    FileDiff { hunks }
}

// ---- Directories ----

/// Project directory listing → directory picker rows (directories only).
/// 项目目录列表 → 目录选择器行（仅目录）。
pub fn directory(listing: &api::ProjectDirectoryListing) -> DirectoryListing {
    DirectoryListing {
        path: listing.path.clone(),
        parent: listing.parent_path.clone(),
        roots: listing
            .roots
            .iter()
            .map(|root| root.path.clone())
            .filter(|path| !path.is_empty())
            .collect(),
        entries: listing
            .entries
            .iter()
            .map(|entry| DirEntry {
                name: entry.name.clone(),
                is_dir: true,
                is_repo: false,
                hidden: entry.hidden,
                size: None,
            })
            .collect(),
        error: listing.notice.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Run phases follow the WebUI normalization, including the PreCheck memory recall phase.
    /// 运行阶段遵循 WebUI 的规范化规则，包括 PreCheck 记忆召回阶段。
    #[test]
    fn maps_run_phases() {
        assert_eq!(run_phase("Ready", "Completed"), RunPhase::Completed);
        assert_eq!(run_phase("Ready", ""), RunPhase::Idle);
        assert_eq!(run_phase("tool_running", "Running"), RunPhase::Running);
        assert_eq!(run_phase("MemoryRecall", "Running"), RunPhase::MemoryRecall);
        assert_eq!(run_phase("", "running"), RunPhase::Running);
        assert_eq!(run_phase("failed", ""), RunPhase::Failed);
    }

    /// Model route keys survive a round trip through the picker key.
    /// 模型路线键可经选择器键往返还原。
    #[test]
    fn model_route_key_round_trips() {
        let route = ModelRoute {
            provider_id: "p".into(),
            model_id: "a/b".into(),
            connection_id: "c".into(),
        };
        assert_eq!(ModelRoute::from_key(&route.key()), Some(route));
    }

    /// A strictly parsed tool call projects its model output and changeset size; event kinds the
    /// transcript does not show are skipped.
    /// 严格解析的工具调用投影其模型输出与变更规模；记录区不显示的事件种类被跳过。
    #[test]
    fn maps_conversation_events() {
        let tool: ConversationEventSummary = serde_json::from_value(json!({
            "seq": 4, "turn_id": 2, "created_at_ms": 1, "updated_at_ms": 2, "event_revision": 1,
            "visibility": "provider",
            "event": {"kind": "tool_call", "api_call_id": 3, "tool_call_id": 4,
                "request_stream_id": "stream-1", "tool_name": "read", "input": {"path": "a"},
                "status": "completed", "output": {"model_output": "ok", "is_error": false},
                "changeset": {"mode": "applied", "files": [{"change": "modify", "path": "a.rs",
                    "hunks": [{"before": "", "after": "",
                        "deleted": [{"line": 1, "content": "x"}],
                        "inserted": [{"line": 1, "content": "y"}, {"line": 2, "content": "z"}]}]}]}}
        }))
        .expect("tool call summary matches the shared DTO");
        let event = event(&tool).expect("tool calls are shown");
        assert_eq!(event.seq, 4);
        let EventBody::ToolCall {
            output, changeset, ..
        } = event.body
        else {
            panic!("expected a tool call");
        };
        assert_eq!(output.as_deref(), Some("ok"));
        assert_eq!((changeset[0].additions, changeset[0].deletions), (2, 1));

        let boundary: ConversationEventSummary = serde_json::from_value(json!({
            "seq": 5, "turn_id": 2, "created_at_ms": 1, "updated_at_ms": 1, "event_revision": 1,
            "visibility": "host_only",
            "event": {"kind": "turn_boundary", "reason": "completed", "outcome": "completed",
                "purpose": "business"}
        }))
        .expect("turn boundary summary matches the shared DTO");
        assert!(super::event(&boundary).is_none());
    }

    /// Created and deleted files count their lifecycle text, as the WebUI summary does.
    /// 创建与删除的文件按生命周期正文计数，与 WebUI 摘要一致。
    #[test]
    fn lifecycle_files_count_their_whole_text() {
        let file = |change: &str| {
            let tool: ConversationEventSummary = serde_json::from_value(json!({
                "seq": 4, "turn_id": 2, "created_at_ms": 1, "updated_at_ms": 2, "event_revision": 1,
                "visibility": "provider",
                "event": {"kind": "tool_call", "api_call_id": 3, "tool_call_id": 4,
                    "request_stream_id": "stream-1", "tool_name": "write", "input": {"path": "a"},
                    "status": "completed", "output": {"model_output": "ok", "is_error": false},
                    "changeset": {"mode": "applied", "files": [{"change": change, "path": "a.rs",
                        "hunks": [], "lifecycle_content": {"mode": "full",
                            "content": "a\nb\nc\n", "total_line_count": 3}}]}}
            }))
            .expect("tool call summary matches the shared DTO");
            let EventBody::ToolCall { changeset, .. } = event(&tool).unwrap().body else {
                panic!("expected a tool call");
            };
            (changeset[0].additions, changeset[0].deletions)
        };
        assert_eq!(file("create"), (3, 0));
        assert_eq!(file("delete"), (0, 3));
    }

    /// Profile markers keep their numbers, and expired memories leave the active cards.
    /// 画像标记保留数字，过期记忆不再出现在活动卡片中。
    #[test]
    fn context_injection_keeps_profile_numbers_and_expired_memories() {
        let item = |content: &str| {
            json!({"content": content, "detail": "", "turnLabel": null, "ttlLabel": "",
                "sourceDisplay": null})
        };
        let snapshot: api::ContextInjectionSnapshot = serde_json::from_value(json!({
            "dynamicCards": [
                {"tag": "VMM_MEMORY", "description": "", "items": [item("active")]},
                {"tag": "VMM_MEMORY_EXPIRED", "description": "", "items": [item("old")]}
            ],
            "staticCards": []
        }))
        .expect("snapshot matches the shared DTO");
        let mapped = context_injection(&snapshot);
        assert_eq!(mapped.dynamic.len(), 1);
        assert_eq!(mapped.dynamic[0].items.len(), 1);
        assert_eq!(mapped.expired.len(), 1);
        assert_eq!(label_number::<u32>("P1"), 1);
        assert_eq!(label_number::<u32>("W3"), 3);
    }

    /// A memory recall record becomes a note carrying its queries; a failed recall warns.
    /// 记忆召回记录成为携带检索词的通知；失败的召回以警告呈现。
    #[test]
    fn maps_memory_recall_to_a_note() {
        let recall = |outcome: &str| {
            let summary: ConversationEventSummary = serde_json::from_value(json!({
                "seq": 3, "turn_id": 2, "created_at_ms": 1, "updated_at_ms": 1, "event_revision": 1,
                "visibility": "ui_only",
                "event": {"kind": "memory_recall", "outcome": outcome, "queries": ["deploy steps"]}
            }))
            .expect("memory recall summary matches the shared DTO");
            match event(&summary).expect("memory recall is shown").body {
                EventBody::DisplayNote { severity, message } => (severity, message),
                _ => panic!("expected a note"),
            }
        };
        let (severity, message) = recall("no_match");
        assert_eq!(severity, NoticeSeverity::Info);
        assert!(message.contains("deploy steps"));
        assert_eq!(recall("failed").0, NoticeSeverity::Warning);
    }

    /// Only the start of a background command wake turn becomes a note; its expiry and other
    /// system contexts stay out of the transcript.
    /// 只有后台命令唤醒回合的起点成为通知；其失效与其他系统上下文不进入记录区。
    #[test]
    fn maps_background_command_wake_to_a_note() {
        let context = |key: &str, operation: &str| {
            let summary: ConversationEventSummary = serde_json::from_value(json!({
                "seq": 7, "turn_id": 3, "created_at_ms": 1, "updated_at_ms": 1, "event_revision": 1,
                "visibility": "provider",
                "event": {"kind": "system_context", "context_key": key, "operation": operation,
                    "scope": "turn", "ttl_turns": 1, "content": "Background shell jobs finished."}
            }))
            .expect("system context summary matches the shared DTO");
            event(&summary).map(|event| event.body)
        };
        assert!(matches!(
            context("shell_job_continuation", "upsert"),
            Some(EventBody::DisplayNote {
                severity: NoticeSeverity::Info,
                ..
            })
        ));
        assert!(context("shell_job_continuation", "remove").is_none());
        assert!(context("plan_mode", "upsert").is_none());
    }

    /// Unified diffs parse into numbered hunks without their file headers.
    /// 统一 diff 解析为带行号的块，不含文件头。
    #[test]
    fn parses_unified_diff() {
        let diff = unified_diff(
            "diff --git a/a.rs b/a.rs\n--- a/a.rs\n+++ b/a.rs\n@@ -3,2 +3,3 @@ fn x\n a\n-b\n+c\n+d\n",
        );
        assert_eq!(diff.hunks.len(), 1);
        let lines = &diff.hunks[0].lines;
        assert_eq!(lines.len(), 4);
        assert_eq!((lines[0].old_no, lines[0].new_no), (Some(3), Some(3)));
        assert_eq!(lines[1].kind, DiffLineKind::Delete);
        assert_eq!(lines[3].new_no, Some(5));
    }
}
