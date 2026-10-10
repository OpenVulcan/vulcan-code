// Service DTO → client model projection for the conversation surface.
// 会话界面的服务 DTO → 客户端模型投影。
//
// Inputs are the shared `vulcan-service-api` DTOs and `vulcan-session-core` v1 conversation
// events, already parsed strictly at the transport boundary; only Git snapshots, whose DTO lives
// in the service kernel, are still read from JSON. Only what the TUI renders is projected.
// 输入是已在传输边界严格解析的共享 `vulcan-service-api` DTO 与 `vulcan-session-core` v1 会话事件；
// 只有 DTO 位于服务内核的 Git 快照仍从 JSON 读取。只投影 TUI 渲染所需的内容。

use serde_json::{Value, json};
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
        unread: conversation.unread,
        attention: conversation.attention.is_some(),
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
        "minimal" | "low" => ReasoningEffort::Low,
        "medium" | "adaptive" => ReasoningEffort::Medium,
        "high" => ReasoningEffort::High,
        "xhigh" | "max" | "ultra" => ReasoningEffort::Max,
        _ => return None,
    })
}

// ---- Planning state ----

/// Maps one pending question and its exact request identity.
/// 映射一个待回复提问及其准确请求身份。
pub fn pending_question(question: &api::PendingQuestionSnapshot) -> PendingQuestion {
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
            state: match normalized(&task.run_state).as_str() {
                "completed" => AgentState::Standby,
                "stopped" => AgentState::Stopped,
                "failed" => AgentState::Failed,
                "closed" => AgentState::Closed,
                _ => AgentState::Running,
            },
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
    label.trim().parse().unwrap_or_default()
}

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
    let dynamic = snapshot
        .dynamic_cards
        .iter()
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
    // The service drops expired injections from `dynamicCards`, so there is no expired list.
    // 服务会从 `dynamicCards` 中移除已过期的注入，因此没有过期列表。
    ContextInjection {
        dynamic,
        expired: Vec::new(),
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
            additions: file.hunks.iter().map(|h| h.inserted.len() as u32).sum(),
            deletions: file.hunks.iter().map(|h| h.deleted.len() as u32).sum(),
        })
        .collect()
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
        },
        CoreEvent::ToolCall(call) => EventBody::ToolCall {
            tool_name: call.tool_name.clone(),
            input: serde_json::to_value(&call.input).unwrap_or(Value::Null),
            output: call.output.as_ref().map(tool_output),
            changeset: changeset(call),
            status: call.status,
            started_at_ms: call.started_at_ms.map(millis),
            finished_at_ms: call.finished_at_ms.map(millis),
        },
        CoreEvent::DisplayNote(note) => EventBody::DisplayNote {
            severity: note.severity,
            message: display_note_message(note),
        },
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
