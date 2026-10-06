// Wire DTO → client model mapping for the conversation surface.
// 会话界面的传输 DTO → 客户端模型映射。
//
// Sources: `vulcan-service-api` (workbench, conversation, planning, git DTOs, camelCase) and
// `vulcan-session-core` v1 conversation events (snake_case). Only what the TUI renders is read.
// 来源：`vulcan-service-api`（工作台、会话、规划、Git DTO，camelCase）与 `vulcan-session-core` v1
// 会话事件（snake_case）。只读取 TUI 渲染所需的字段。

use serde_json::{Value, json};

use super::wire::*;
use crate::model::*;

// ---- Workbench ----

/// Projects one service project into the sidebar model using its exact identity.
/// 使用准确身份将一个服务项目投影为侧栏模型。
/// `value` is the protocol value being read or projected.
/// `value` 为正在读取或投影的协议值。
/// Returns `Option<Project>`.
/// 返回 `Option<Project>`。
pub fn project(value: &Value) -> Option<Project> {
    // Exact entity identity.
    // 准确实体身份。
    let id = opt_str(value, "id")?;
    Some(Project {
        name: opt_str(value, "name").unwrap_or_else(|| id.clone()),
        path: str_at(value, "path"),
        pinned: bool_at(value, "pinned"),
        id,
    })
}

/// Maps service project rows and rejects rows lacking required identities.
/// 映射服务项目行，并拒绝缺少必需身份的行。
/// `list` is the protocol row collection.
/// `list` 为协议行集合。
/// Returns `Vec<Project>`.
/// 返回 `Vec<Project>`。
pub fn projects(list: &[Value]) -> Vec<Project> {
    list.iter().filter_map(project).collect()
}

/// Run phase from `runPhase`, falling back to `status` (`conversationIsRunning` in the WebUI).
/// 由 `runPhase` 推导运行阶段，缺省时回退到 `status`（对应 WebUI 的 `conversationIsRunning`）。
/// Returns `RunPhase`.
/// 返回 `RunPhase`。
pub fn run_phase(phase: &str, status: &str) -> RunPhase {
    // Status.
    // 已声明生命周期状态。
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
        _ => RunPhase::Running,
    }
}

/// Projects one service conversation into the sidebar summary.
/// 将一个服务会话投影为侧栏摘要。
/// `value` is the protocol value being read or projected.
/// `value` 为正在读取或投影的协议值。
/// Returns `Option<ConversationSummary>`.
/// 返回 `Option<ConversationSummary>`。
pub fn conversation(value: &Value) -> Option<ConversationSummary> {
    // Exact entity identity.
    // 准确实体身份。
    let id = opt_str(value, "id")?;
    Some(ConversationSummary {
        title: str_at(value, "title"),
        project_id: opt_str(value, "projectId"),
        run_phase: run_phase(&str_at(value, "runPhase"), &str_at(value, "status")),
        last_updated_label: str_at(value, "lastUpdatedLabel"),
        unread: bool_at(value, "unread"),
        attention: !at(value, "attention").is_null(),
        id,
    })
}

/// Maps conversation rows into the displayed sidebar catalog.
/// 将会话行映射为显示的侧栏目录。
/// `list` is the protocol row collection.
/// `list` 为协议行集合。
/// Returns `Vec<ConversationSummary>`.
/// 返回 `Vec<ConversationSummary>`。
pub fn conversations(list: &[Value]) -> Vec<ConversationSummary> {
    list.iter().filter_map(conversation).collect()
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
    /// Returns `String`.
    /// 返回 `String`。
    pub fn key(&self) -> String {
        json!([self.provider_id, self.model_id, self.connection_id]).to_string()
    }

    /// Parses the exact provider/model/connection tuple serialized by the model picker.
    /// 解析模型选择器序列化的准确提供方、模型及连接元组。
    /// Returns `Option<ModelRoute>`.
    /// 返回 `Option<ModelRoute>`。
    pub fn from_key(key: &str) -> Option<ModelRoute> {
        // Parts.
        // 通过解析的路线元组。
        let parts: Vec<String> = serde_json::from_str(key).ok()?;
        // Provider id; Model id; Connection id.
        // 准确提供方身份；准确模型身份；准确连接身份。
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
/// Returns `(Vec<ModelInfo>, Option<String>)`.
/// 返回 `(Vec<ModelInfo>, Option<String>)`。
pub fn models(providers: &[Value]) -> (Vec<ModelInfo>, Option<String>) {
    // Current operation rows.
    // 当前操作的行集合。
    let mut rows = Vec::new();
    // Active.
    // 是否处于活动状态。
    let mut active = None;
    for provider in providers {
        // Provider id.
        // 准确提供方身份。
        let provider_id =
            opt_str(provider, "sourceProviderId").unwrap_or_else(|| str_at(provider, "id"));
        // Connection id.
        // 准确连接身份。
        let connection_id = str_at(provider, "connectionId");
        // Provider name.
        // 提供方显示名称。
        let provider_name =
            opt_str(provider, "providerDisplayName").unwrap_or_else(|| str_at(provider, "name"));
        // Account.
        // 公开账号显示信息。
        let account = opt_str(provider, "accountName");
        for model in arr(provider, "models") {
            if at(model, "enabled").as_bool() == Some(false) {
                continue;
            }
            // Route.
            // 目录中的推理路由。
            let route = ModelRoute {
                provider_id: provider_id.clone(),
                model_id: str_at(model, "id"),
                connection_id: connection_id.clone(),
            };
            // Key.
            // 当前结构持有的键。
            let key = route.key();
            if bool_at(model, "active")
                && (active.is_none() || bool_at(model, "preferredConnection"))
            {
                active = Some(key.clone());
            }
            rows.push(ModelInfo {
                label: opt_str(model, "name").unwrap_or_else(|| route.model_id.clone()),
                provider: match &account {
                    Some(account) => format!("{provider_name} · {account}"),
                    None => provider_name.clone(),
                },
                context_window: u64_at(model, "contextLimit"),
                id: key,
            });
        }
    }
    rows.sort_by(|a, b| a.label.cmp(&b.label).then(a.provider.cmp(&b.provider)));
    (rows, active)
}

/// Route key of a session's current request (`session_config.current_request`).
/// 会话当前请求（`session_config.current_request`）的路线键。
/// Returns `Option<String>`.
/// 返回 `Option<String>`。
pub fn session_route(config: &Value) -> Option<String> {
    // Exact protocol request.
    // 协议请求文档。
    let request = at(config, "current_request");
    Some(
        ModelRoute {
            provider_id: opt_str(request, "provider")?,
            model_id: opt_str(request, "model")?,
            connection_id: str_at(request, "connection_id"),
        }
        .key(),
    )
}

/// Maps a declared service reasoning-effort value to the local selector enum.
/// 将声明的服务推理强度值映射为本地选择器枚举。
/// `value` is the protocol value being read or projected.
/// `value` 为正在读取或投影的协议值。
/// Returns `Option<ReasoningEffort>`.
/// 返回 `Option<ReasoningEffort>`。
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

/// Maps the pending-question fields and their exact request identity.
/// 映射待回复提问字段及其准确请求身份。
/// `value` is the protocol value being read or projected.
/// `value` 为正在读取或投影的协议值。
/// Returns `Option<PendingQuestion>`.
/// 返回 `Option<PendingQuestion>`。
pub fn pending_question(value: &Value) -> Option<PendingQuestion> {
    Some(PendingQuestion {
        id: opt_str(value, "id")?,
        questions: arr(value, "questions")
            .iter()
            .map(|q| Question {
                header: str_at(q, "header"),
                question: str_at(q, "question"),
                options: arr(q, "options")
                    .iter()
                    .map(|o| QuestionOption {
                        label: str_at(o, "label"),
                        description: str_at(o, "description"),
                    })
                    .collect(),
                multiple: bool_at(q, "multiple"),
            })
            .collect(),
    })
}

/// Maps queued service prompts for the current conversation.
/// 映射当前会话排队中的服务提示。
/// `list` is the protocol row collection.
/// `list` 为协议行集合。
/// Returns `Vec<PendingPrompt>`.
/// 返回 `Vec<PendingPrompt>`。
pub fn pending_prompts(list: &[Value]) -> Vec<PendingPrompt> {
    list.iter()
        .filter_map(|p| {
            Some(PendingPrompt {
                id: opt_str(p, "id")?,
                preview: opt_str(p, "preview").unwrap_or_else(|| str_at(p, "prompt")),
            })
        })
        .collect()
}

/// Maps published planning entries into the session progress panel.
/// 将发布的规划条目映射到会话进度面板。
/// `value` is the protocol value being read or projected.
/// `value` 为正在读取或投影的协议值。
/// Returns `Vec<TodoItem>`.
/// 返回 `Vec<TodoItem>`。
pub fn todo(value: &Value) -> Vec<TodoItem> {
    arr(value, "items")
        .iter()
        .map(|item| TodoItem {
            content: str_at(item, "step"),
            status: match normalized(&str_at(item, "status")).as_str() {
                "completed" | "done" => TodoStatus::Completed,
                "inprogress" | "running" => TodoStatus::InProgress,
                _ => TodoStatus::Pending,
            },
        })
        .collect()
}

/// Maps the published plan state into the local execution mode.
/// 将发布的计划状态映射为本地执行模式。
/// `value` is the protocol value being read or projected.
/// `value` 为正在读取或投影的协议值。
/// Returns `ExecutionMode`.
/// 返回 `ExecutionMode`。
pub fn plan_mode(value: &Value) -> ExecutionMode {
    if normalized(&str_at(value, "executionMode")) == "plan" {
        ExecutionMode::Plan
    } else {
        ExecutionMode::Build
    }
}

/// Projects service token usage and its budget into the context meter.
/// 将服务令牌用量及预算投影为上下文计量表。
/// `value` is the protocol value being read or projected.
/// `value` 为正在读取或投影的协议值。
/// Returns `ContextUsage`.
/// 返回 `ContextUsage`。
pub fn context_usage(value: &Value) -> ContextUsage {
    ContextUsage {
        used_tokens: u64_at(value, "usedTokens"),
        max_tokens: u64_at(value, "maxTokens"),
    }
}

/// Maps service command-session output into terminal panel rows.
/// 将服务命令会话输出映射为终端面板行。
/// `list` is the protocol row collection.
/// `list` 为协议行集合。
/// Returns `Vec<CommandSession>`.
/// 返回 `Vec<CommandSession>`。
pub fn command_sessions(list: &[Value]) -> Vec<CommandSession> {
    list.iter()
        .filter_map(|s| {
            // Exact entity identity.
            // 准确实体身份。
            let id = opt_str(s, "id")?;
            // Status.
            // 已声明生命周期状态。
            let status = normalized(&str_at(s, "status"));
            // Screen.
            // 显示来源窗口。
            let screen = opt_str(s, "screenText")
                .or_else(|| opt_str(s, "preview"))
                .unwrap_or_default();
            Some(CommandSession {
                title: opt_str(s, "name").unwrap_or_else(|| str_at(s, "command")),
                command: str_at(s, "command"),
                running: matches!(status.as_str(), "running" | "starting"),
                exit_code: at(s, "exitCode").as_i64().map(|c| c as i32),
                screen: screen.lines().map(str::to_string).collect(),
                id,
            })
        })
        .collect()
}

/// `SubagentNavigationSnapshot.tasks` → subagent rows.
/// `SubagentNavigationSnapshot.tasks` → 子代理行。
/// Returns `Vec<AgentInfo>`.
/// 返回 `Vec<AgentInfo>`。
pub fn agents(navigation: &Value) -> Vec<AgentInfo> {
    arr(navigation, "tasks")
        .iter()
        .filter_map(|task| {
            // Exact entity identity.
            // 准确实体身份。
            let id = opt_str(task, "taskId")?;
            // State.
            // 当前生命周期状态。
            let state = match normalized(&str_at(task, "runState")).as_str() {
                "completed" => AgentState::Standby,
                "stopped" => AgentState::Stopped,
                "failed" => AgentState::Failed,
                "closed" => AgentState::Closed,
                _ => AgentState::Running,
            };
            Some(AgentInfo {
                name: opt_str(task, "label").unwrap_or_else(|| str_at(task, "publicTaskId")),
                profile: str_at(task, "agentProfileId"),
                task: str_at(task, "description"),
                state,
                prompt: String::new(),
                pending: arr(task, "pendingMessages")
                    .iter()
                    .map(|m| AgentMessage {
                        steering: bool_at(m, "steering"),
                        text: str_at(m, "message"),
                    })
                    .collect(),
                activity: Vec::new(),
                result: None,
                usage: UsageBucket::default(),
                id,
            })
        })
        .collect()
}

// ---- Insight ----

/// Maps one published usage bucket without starting any model request.
/// 映射一份已发布用量，不发起任何模型请求。
/// `value` is the protocol value being read or projected.
/// `value` 为正在读取或投影的协议值。
/// Returns `UsageBucket`.
/// 返回 `UsageBucket`。
fn usage_bucket(value: &Value) -> UsageBucket {
    UsageBucket {
        requests: u64_at(value, "requestCount"),
        unknown_usage: u64_at(value, "unknownUsageCount"),
        tool_calls: u64_at(value, "toolCallCount"),
        input: u64_at(value, "inputTokens.sum"),
        uncached_input: u64_at(value, "uncachedInputTokens.sum"),
        cache_read: u64_at(value, "cacheReadTokens.sum"),
        cache_write: u64_at(value, "cacheWriteTokens.sum"),
        output: u64_at(value, "outputTokens.sum"),
        reasoning: u64_at(value, "reasoningTokens.sum"),
        first_token_ms: u64_at(value, "firstTokenMs.sum"),
        first_token_samples: u64_at(value, "firstTokenMs.samples"),
        duration_ms: u64_at(value, "requestDurationMs.sum"),
        duration_samples: u64_at(value, "requestDurationMs.samples"),
        measured_output: u64_at(value, "measuredOutputTokens"),
        measured_output_ms: u64_at(value, "outputDurationMs.sum"),
    }
}

/// `sessionProviderUsage.lifetime` → lifetime usage; children are keyed by child session id until
/// the worker relabels them with subagent names.
/// `sessionProviderUsage.lifetime` → 生命周期用量；子项以子会话 ID 为键，之后由工作线程替换为子代理名。
/// Returns `LifetimeUsage`.
/// 返回 `LifetimeUsage`。
pub fn lifetime(usage: &Value) -> LifetimeUsage {
    // Lifetime.
    // 会话累计用量。
    let lifetime = at(usage, "lifetime");
    // Children.
    // 子代理条目。
    let children = at(lifetime, "childrenBySessionId")
        .as_object()
        .map(|map| {
            map.iter()
                .map(|(id, child)| (id.clone(), usage_bucket(at(child, "usage"))))
                .collect()
        })
        .unwrap_or_default();
    LifetimeUsage {
        main: usage_bucket(at(lifetime, "main")),
        children,
        history_incomplete: bool_at(lifetime, "legacyIncomplete"),
    }
}

/// Maps context usage sections into the panel composition breakdown.
/// 将上下文用量分区映射为面板组成明细。
/// Returns `Vec<ContextSlice>`.
/// 返回 `Vec<ContextSlice>`。
pub fn breakdown(context_usage: &Value) -> Vec<ContextSlice> {
    // Slices.
    // 上下文组成分区。
    let mut slices: Vec<ContextSlice> = Vec::new();
    for item in arr(context_usage, "breakdown") {
        // Part.
        // 当前组成项。
        let part = match normalized(&str_at(item, "kind")).as_str() {
            "systemprompt" => ContextPart::SystemPrompt,
            "systemtools" | "mcptools" | "skills" | "tools" => ContextPart::Tools,
            "memory" | "vmm" => ContextPart::Memory,
            "attachments" | "media" => ContextPart::Attachments,
            _ => ContextPart::Conversation,
        };
        // Tokens.
        // 当前令牌数量。
        let tokens = u64_at(item, "tokens");
        match slices.iter_mut().find(|s| s.part == part) {
            Some(slice) => slice.tokens += tokens,
            None => slices.push(ContextSlice { part, tokens }),
        }
    }
    slices
}

/// Projects the published goal status, budget and usage into the session panel.
/// 将发布的目标状态、预算及用量投影为会话面板。
/// `value` is the protocol value being read or projected.
/// `value` 为正在读取或投影的协议值。
/// Returns `Option<Goal>`.
/// 返回 `Option<Goal>`。
pub fn goal(value: &Value) -> Option<Goal> {
    // Objective.
    // 目标描述。
    let objective = opt_str(value, "objective")?;
    Some(Goal {
        objective,
        status: match normalized(&str_at(value, "status")).as_str() {
            "paused" => GoalStatus::Paused,
            "blocked" => GoalStatus::Blocked,
            "usagelimited" => GoalStatus::UsageLimited,
            "budgetlimited" => GoalStatus::BudgetLimited,
            "complete" | "completed" => GoalStatus::Completed,
            _ => GoalStatus::Active,
        },
        elapsed_ms: u64_at(value, "timeUsedSeconds") * 1000,
        usage: UsageBucket {
            requests: u64_at(value, "apiCallCount"),
            tool_calls: u64_at(value, "toolCallCount"),
            input: u64_at(value, "inputTokens"),
            cache_read: u64_at(value, "cachedInputTokens"),
            output: u64_at(value, "outputTokens"),
            reasoning: u64_at(value, "reasoningTokens"),
            ..UsageBucket::default()
        },
        budget: opt_u64(value, "tokenBudget").filter(|b| *b > 0),
    })
}

/// `ContextInjectionSnapshot` → dynamic memory cards and the static profile.
/// `ContextInjectionSnapshot` → 动态记忆卡片与静态画像。
/// `value` is the protocol value being read or projected.
/// `value` 为正在读取或投影的协议值。
/// Returns `ContextInjection`.
/// 返回 `ContextInjection`。
pub fn context_injection(value: &Value) -> ContextInjection {
    // Memory.
    // 会话记忆状态。
    let memory = |item: &Value| InjectedMemory {
        // Injections that are not stored memories have no `memoryId`; their dedupe key names them.
        // 非存储记忆的注入没有 `memoryId`；以其去重键标识。
        memory_id: opt_str(item, "memoryId").unwrap_or_else(|| str_at(item, "dedupeKey")),
        content: str_at(item, "content"),
        turn_id: opt_str(item, "turnLabel").unwrap_or_else(|| str_at(item, "turnId")),
        turns_left: str_at(item, "ttlLabel")
            .chars()
            .filter(char::is_ascii_digit)
            .collect::<String>()
            .parse()
            .unwrap_or(0),
        score: f32_at(item, "scoreLabel"),
        source: str_at(item, "sourceDisplay"),
        dialogue: None,
    };
    // Dynamic.
    // 动态记忆条目。
    let dynamic = arr(value, "dynamicCards")
        .iter()
        .map(|card| MemoryCard {
            tag: str_at(card, "tag"),
            description: str_at(card, "description"),
            items: arr(card, "items").iter().map(memory).collect(),
        })
        .collect();
    // Profile.
    // 声明的消费者或凭据配置。
    let profile = arr(value, "staticProfile.sections")
        .iter()
        .map(|section| ProfileSection {
            user: normalized(&str_at(section, "target")) == "user",
            name: str_at(section, "label"),
            nodes: arr(section, "dates")
                .iter()
                .flat_map(|date| {
                    // Day.
                    // 日期显示文本。
                    let day = str_at(date, "date");
                    arr(date, "nodes").iter().map(move |node| ProfileNode {
                        id: str_at(node, "id"),
                        date: day.get(..10).unwrap_or(&day).to_string(),
                        priority: u64_at(node, "priority").min(u8::MAX as u64) as u8,
                        level: u64_at(node, "level").min(u8::MAX as u64) as u8,
                        weight: f32_at(node, "refreshWeight"),
                        content: str_at(node, "content"),
                        source: match normalized(&str_at(node, "sourceKind")).as_str() {
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

/// Maps the declared event stream status to local rendering state.
/// 将声明的事件流状态映射为本地渲染状态。
/// `value` is the protocol value being read or projected.
/// `value` 为正在读取或投影的协议值。
/// Returns `StreamStatus`.
/// 返回 `StreamStatus`。
fn stream_status(value: &Value) -> StreamStatus {
    match normalized(&str_at(value, "status")).as_str() {
        "streaming" => StreamStatus::Streaming,
        "interrupted" => StreamStatus::Interrupted,
        _ => StreamStatus::Completed,
    }
}

/// Maps the declared tool execution status to the display enum.
/// 将声明的工具执行状态映射为显示枚举。
/// `value` is the protocol value being read or projected.
/// `value` 为正在读取或投影的协议值。
/// Returns `ToolStatus`.
/// 返回 `ToolStatus`。
fn tool_status(value: &str) -> ToolStatus {
    match normalized(value).as_str() {
        "prepared" => ToolStatus::Prepared,
        "running" => ToolStatus::Running,
        "waitinguser" => ToolStatus::WaitingUser,
        "failed" => ToolStatus::Failed,
        "cancelled" => ToolStatus::Cancelled,
        "stopped" => ToolStatus::Stopped,
        "interrupted" => ToolStatus::Interrupted,
        _ => ToolStatus::Completed,
    }
}

/// Tool output as the model saw it; structured payloads fall back to compact JSON.
/// 模型所见的工具输出；结构化载荷回退为紧凑 JSON。
/// Returns `Option<String>`.
/// 返回 `Option<String>`。
fn tool_output(output: &Value) -> Option<String> {
    if output.is_null() {
        return None;
    }
    if let Some(text) = opt_str(output, "model_output") {
        return Some(text);
    }
    match output {
        Value::String(s) => Some(s.clone()),
        other => Some(other.to_string()),
    }
}

/// Maps a service changeset into file-operation detail rows.
/// 将服务变更集映射为文件操作详情行。
/// `value` is the protocol value being read or projected.
/// `value` 为正在读取或投影的协议值。
/// Returns `Vec<ChangesetFile>`.
/// 返回 `Vec<ChangesetFile>`。
fn changeset(value: &Value) -> Vec<ChangesetFile> {
    arr(value, "files")
        .iter()
        .map(|file| {
            // Additions; Deletions.
            // 新增行数量；删除行数量。
            let (mut additions, mut deletions) = (0, 0);
            for hunk in arr(file, "hunks") {
                additions += arr(hunk, "inserted").len() as u32;
                deletions += arr(hunk, "deleted").len() as u32;
            }
            ChangesetFile {
                path: opt_str(file, "new_path")
                    .or_else(|| opt_str(file, "path"))
                    .or_else(|| opt_str(file, "old_path"))
                    .unwrap_or_default(),
                additions,
                deletions,
            }
        })
        .collect()
}

/// One `ConversationEventSummary`; events the transcript does not show map to `None`.
/// 一条 `ConversationEventSummary`；记录区不显示的事件映射为 `None`。
/// Returns `Option<ConversationEvent>`.
/// 返回 `Option<ConversationEvent>`。
pub fn event(summary: &Value) -> Option<ConversationEvent> {
    // Seq.
    // 事件序号。
    let seq = opt_u64(summary, "seq")?;
    // Body.
    // 准确消息正文。
    let body = at(summary, "event");
    // Body.
    // 准确消息正文。
    let body = match str_at(body, "kind").as_str() {
        "user_input" => EventBody::UserInput {
            text: str_at(body, "text"),
            input_source: match normalized(&str_at(body, "input_source")).as_str() {
                "pendinginturn" => InputSource::PendingInTurn,
                "queuedscheduled" => InputSource::QueuedScheduled,
                "agentmessage" => InputSource::AgentMessage,
                _ => InputSource::Root,
            },
            attachments: Vec::new(),
        },
        "assistant_reasoning" => EventBody::AssistantReasoning {
            api_call_id: str_at(body, "api_call_id"),
            text: str_at(body, "text"),
            status: stream_status(body),
        },
        "assistant_text" => EventBody::AssistantText {
            api_call_id: str_at(body, "api_call_id"),
            text: str_at(body, "text"),
            status: stream_status(body),
        },
        "tool_call" => EventBody::ToolCall {
            tool_call_id: str_at(body, "tool_call_id"),
            tool_name: str_at(body, "tool_name"),
            input: at(body, "input").clone(),
            output: tool_output(at(body, "output")),
            changeset: changeset(at(body, "changeset")),
            status: tool_status(&str_at(body, "status")),
            started_at_ms: opt_u64(body, "started_at_ms"),
            finished_at_ms: opt_u64(body, "finished_at_ms"),
        },
        "display_note" => EventBody::DisplayNote {
            severity: match normalized(&str_at(body, "severity")).as_str() {
                "error" => Severity::Error,
                "warning" => Severity::Warning,
                _ => Severity::Info,
            },
            message: opt_str(body, "redacted_detail")
                .unwrap_or_else(|| str_at(body, "reason_code")),
        },
        "turn_stats" => EventBody::TurnStats {
            outcome: match normalized(&str_at(body, "outcome")).as_str() {
                "abandoned" => TurnOutcome::Abandoned,
                "stopped" | "interrupted" | "cancelled" => TurnOutcome::Stopped,
                _ => TurnOutcome::Completed,
            },
            duration_ms: u64_at(body, "duration_ms"),
            total_tokens: u64_at(body, "buckets.total.usage.total_tokens"),
            requests: u64_at(body, "buckets.total.api_call_count") as u32,
            tool_calls: u64_at(body, "buckets.total.tool_call_count") as u32,
        },
        _ => return None,
    };
    Some(ConversationEvent {
        seq,
        turn_id: text(at(summary, "turn_id")),
        created_at_ms: u64_at(summary, "created_at_ms"),
        updated_at_ms: u64_at(summary, "updated_at_ms"),
        body,
    })
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
/// `path` is the exact requested path.
/// `path` 为准确请求路径。
/// Returns `FileDiff`.
/// 返回 `FileDiff`。
pub fn unified_diff(path: &str, text: &str) -> FileDiff {
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
    FileDiff {
        path: path.to_string(),
        hunks,
    }
}

// ---- Directories ----

/// `ProjectDirectoryListing` → directory listing (directories only).
/// `ProjectDirectoryListing` → 目录列表（仅目录）。
/// `value` is the protocol value being read or projected.
/// `value` 为正在读取或投影的协议值。
/// Returns `DirectoryListing`.
/// 返回 `DirectoryListing`。
pub fn directory(value: &Value) -> DirectoryListing {
    DirectoryListing {
        path: str_at(value, "path"),
        parent: opt_str(value, "parentPath"),
        roots: arr(value, "roots")
            .iter()
            .map(|r| str_at(r, "path"))
            .filter(|p| !p.is_empty())
            .collect(),
        entries: arr(value, "entries")
            .iter()
            .map(|e| DirEntry {
                name: str_at(e, "name"),
                is_dir: true,
                is_repo: false,
                hidden: bool_at(e, "hidden"),
                size: None,
            })
            .collect(),
        error: opt_str(value, "notice"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    /// Verifies maps run phases with synthetic data and assertions.
    /// 使用合成数据及断言验证 maps_run_phases。
    fn maps_run_phases() {
        assert_eq!(run_phase("Ready", "Completed"), RunPhase::Completed);
        assert_eq!(run_phase("Ready", ""), RunPhase::Idle);
        assert_eq!(run_phase("tool_running", "Running"), RunPhase::Running);
        assert_eq!(run_phase("", "running"), RunPhase::Running);
        assert_eq!(run_phase("failed", ""), RunPhase::Failed);
    }

    #[test]
    /// Verifies model route key round trips with synthetic data and assertions.
    /// 使用合成数据及断言验证 model_route_key_round_trips。
    fn model_route_key_round_trips() {
        // Route.
        // 目录中的推理路由。
        let route = ModelRoute {
            provider_id: "p".into(),
            model_id: "a/b".into(),
            connection_id: "c".into(),
        };
        assert_eq!(ModelRoute::from_key(&route.key()), Some(route));
    }

    #[test]
    /// Verifies maps wire events with synthetic data and assertions.
    /// 使用合成数据及断言验证 maps_wire_events。
    fn maps_wire_events() {
        // Tool.
        // 工具数据。
        let tool = json!({"seq": 4, "turn_id": 2, "created_at_ms": 1, "updated_at_ms": 2,
            "event": {"kind": "tool_call", "api_call_id": 3, "tool_call_id": 4,
                "tool_name": "read", "input": {"path": "a"}, "status": "completed",
                "output": {"model_output": "ok", "is_error": false},
                "changeset": {"mode": "x", "files": [{"change": "modify", "path": "a.rs",
                    "hunks": [{"before": "", "after": "", "deleted": [{"line": 1, "content": "x"}],
                        "inserted": [{"line": 1, "content": "y"}, {"line": 2, "content": "z"}]}]}]}}});
        // Event.
        // 后端或会话事件。
        let event = event(&tool).expect("tool call");
        assert_eq!(event.turn_id.as_deref(), Some("2"));
        let EventBody::ToolCall {
            output, changeset, ..
        } = event.body
        else {
            panic!("expected a tool call");
        };
        assert_eq!(output.as_deref(), Some("ok"));
        assert_eq!((changeset[0].additions, changeset[0].deletions), (2, 1));
        // Hidden.
        // 是否为隐藏项。
        let hidden = json!({"seq": 5, "event": {"kind": "api_started"}});
        assert!(super::event(&hidden).is_none());
    }

    #[test]
    /// Verifies parses unified diff with synthetic data and assertions.
    /// 使用合成数据及断言验证 parses_unified_diff。
    fn parses_unified_diff() {
        // Diff.
        // 差异结果。
        let diff = unified_diff(
            "a.rs",
            "diff --git a/a.rs b/a.rs\n--- a/a.rs\n+++ b/a.rs\n@@ -3,2 +3,3 @@ fn x\n a\n-b\n+c\n+d\n",
        );
        assert_eq!(diff.hunks.len(), 1);
        // Lines.
        // 当前文本行。
        let lines = &diff.hunks[0].lines;
        assert_eq!(lines.len(), 4);
        assert_eq!((lines[0].old_no, lines[0].new_no), (Some(3), Some(3)));
        assert_eq!(lines[1].kind, DiffLineKind::Delete);
        assert_eq!(lines[3].new_no, Some(5));
    }
}
