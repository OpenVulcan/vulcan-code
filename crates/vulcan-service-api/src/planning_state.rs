use super::*;

/// Describes one pending ask-tool request mirrored into the WebUI.
/// 描述镜像到 WebUI 的单个待处理 ask 工具请求。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PendingQuestionSnapshot {
    /// Stable request identifier used by reply and reject commands.
    /// 回复与拒绝命令使用的稳定请求标识。
    pub id: String,
    /// Controller session that owns the blocked ask tool call.
    /// 拥有阻塞 ask 工具调用的控制器会话。
    pub session_id: String,
    /// Monotonic runtime sequence used to order concurrent asks.
    /// 用于排序并发 ask 请求的运行时单调序号。
    pub sequence: u64,
    /// Ordered prompts shown in the modal.
    /// 在弹窗中按顺序展示的问题提示。
    pub questions: Vec<PendingQuestionPromptSummary>,
    /// Permission facts when the request approves a gated tool call; the prompts are then a
    /// default-language fallback and replies use the canonical approval labels.
    /// 请求审批受管控工具调用时的权限事实；此时问题提示只是默认语言后备，回复使用规范审批标签。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approval: Option<PendingApprovalSnapshot>,
}

/// Permission facts of one pending tool-call approval, rendered by clients in their own language.
/// 一条待处理工具调用审批的权限事实，由客户端以自身语言渲染。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PendingApprovalSnapshot {
    /// Public name of the gated tool.
    /// 受管控工具的公开名称。
    pub tool_name: String,
    /// Session permission mode that escalated the call.
    /// 使调用升级的会话权限模式。
    pub permission_mode: vulcan_session_core::v1::config::PermissionMode,
    /// What the call would affect.
    /// 调用会影响的对象。
    pub subject: PendingApprovalSubject,
    /// Matched high-risk category key, such as `recursive_delete`.
    /// 命中的高风险类别键，例如 `recursive_delete`。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub high_risk: Option<String>,
    /// Permission judge finding, when a judge is configured for the call.
    /// 权限判定结论（当调用配置了判定器时）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub judge: Option<PendingApprovalJudge>,
    /// Permission reviewer finding, when a reviewing LLM is configured for the call.
    /// 权限复核结论（当调用配置了复核 LLM 时）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review: Option<PendingApprovalReview>,
    /// Reply label that runs the call once.
    /// 执行一次调用的回复标签。
    pub allow_label: String,
    /// Reply label that refuses the call.
    /// 拒绝调用的回复标签。
    pub deny_label: String,
}

/// What the permission judge found for a call it did not clear.
/// 权限判定对未放行调用的结论。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PendingApprovalJudge {
    /// The judge scored the call; `allow` appears when a high-risk rule still needs the user.
    /// 判定器已评分；高风险规则仍需用户确认时会出现 `allow`。
    Scored {
        /// Label key: `allow`, `review` or `block`.
        /// 标签键：`allow`、`review` 或 `block`。
        label: String,
    },
    /// The judge is configured but could not score the call.
    /// 判定器已配置但无法为调用评分。
    Unavailable {
        /// English reason.
        /// 英文原因。
        reason: String,
    },
}

/// What the permission reviewer found for a call it did not decide.
/// 权限复核对未决定调用的结论。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PendingApprovalReview {
    /// The reviewer answered; `allow` appears when it may not clear a blocked or high-risk call.
    /// 复核器已回答；不允许其放行被拦截或高风险的调用时会出现 `allow`。
    Reviewed {
        /// Verdict key: `allow`, `ask_user` or `deny`.
        /// 决定键：`allow`、`ask_user` 或 `deny`。
        verdict: String,
        /// Reason in the user's language.
        /// 以用户语言写出的理由。
        reason: String,
    },
    /// The reviewer is configured but could not review the call.
    /// 复核器已配置但无法复核调用。
    Unavailable {
        /// English reason.
        /// 英文原因。
        reason: String,
    },
}

/// What one gated tool call would affect.
/// 一次受管控工具调用会影响的对象。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PendingApprovalSubject {
    /// Shell text; empty starts an interactive shell.
    /// shell 文本；为空表示启动交互式 shell。
    Command {
        /// Command text exactly as the model wrote it.
        /// 与模型书写完全一致的命令文本。
        text: String,
    },
    /// File writes to these raw paths.
    /// 写入这些原始路径。
    FileWrite {
        /// Raw paths exactly as the model wrote them.
        /// 与模型书写完全一致的原始路径。
        paths: Vec<String>,
    },
    /// An extension tool not known to be read-only.
    /// 未知是否只读的扩展工具。
    Opaque,
}

/// Describes one prompt inside a pending ask-tool request.
/// 描述待处理 ask 工具请求中的单条问题提示。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PendingQuestionPromptSummary {
    /// Full question text shown to the user.
    /// 展示给用户的完整问题文本。
    pub question: String,
    /// Short field header shown above the question.
    /// 展示在问题上方的简短字段标题。
    pub header: String,
    /// Selectable choices offered by the prompt.
    /// 该问题提供的可选答案。
    #[serde(default)]
    pub options: Vec<PendingQuestionOptionSummary>,
    /// Whether multiple structured choices may be selected together.
    /// 是否允许同时选择多个结构化选项。
    pub multiple: bool,
}

/// Describes one selectable choice inside an ask-tool prompt.
/// 描述 ask 工具问题中的单个可选答案。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct PendingQuestionOptionSummary {
    /// Choice label returned to the model when selected.
    /// 被选中时回传给模型的选项标签。
    pub label: String,
    /// Helper text explaining the choice.
    /// 解释该选项的辅助说明。
    pub description: String,
}

/// Describes the current model-authored todolist for one selected session.
/// 描述当前选中会话的模型编写 todolist。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SessionTodolistSummary {
    /// Optional explanation attached to the latest todolist update.
    /// 最近一次 todolist 更新附带的可选说明。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub explanation: Option<String>,
    /// Ordered todolist items shown in the session tool panel.
    /// 会话工具面板中展示的有序 todolist 条目。
    #[serde(default)]
    pub items: Vec<SessionTodolistItemSummary>,
    /// Completed item count.
    /// 已完成条目数量。
    pub completed_count: usize,
    /// Total item count.
    /// 条目总数量。
    pub total_count: usize,
    /// Unix-millis update timestamp text.
    /// Unix 毫秒格式的更新时间文本。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

/// Describes one todolist item for WebUI rendering.
/// 描述供 WebUI 渲染的单个 todolist条目。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SessionTodolistItemSummary {
    /// Human-readable step text supplied by the model.
    /// 模型提供的人类可读步骤文本。
    pub step: String,
    /// Stable status key: pending, in_progress, or completed.
    /// 稳定状态键：pending、in_progress 或 completed。
    pub status: String,
}

/// Describes the current session Plan mode state for WebUI rendering.
/// 描述供 WebUI 渲染的当前会话计划模式状态。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SessionPlanModeSummary {
    /// Stable execution mode key such as `build` or `plan`.
    /// 稳定执行模式键，例如 `build` 或 `plan`。
    pub execution_mode: String,
    /// Whether Plan mode steering has already been injected for this activation.
    /// 当前计划模式激活期间是否已经注入过计划模式引导。
    pub prompt_injected: bool,
    /// Latest generated plan artifact, if one exists.
    /// 最近生成的计划产物，如果存在。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact: Option<SessionPlanArtifactSummary>,
}

/// Describes one generated Plan mode artifact for WebUI rendering.
/// 描述供 WebUI 渲染的单个计划模式产物。
#[derive(Debug, Clone, Default, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SessionPlanArtifactSummary {
    /// Human-readable plan name supplied by the model.
    /// 模型提供的人类可读计划名称。
    pub name: String,
    /// Absolute filesystem path of the generated Markdown plan file.
    /// 生成的 Markdown 计划文件绝对路径。
    pub path: String,
    /// Full Markdown plan content captured at submit time.
    /// 提交时捕获的完整 Markdown 计划内容。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    /// Unix-millis text capturing when the artifact was written.
    /// 捕获产物写入时间的 Unix 毫秒文本。
    pub updated_at: String,
}

/// Stable lifecycle status exposed for one session goal.
/// 单个会话目标对外暴露的稳定生命周期状态。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum SessionGoalStatus {
    /// The goal is actively pursued by continuation.
    /// 目标正在由续跑流程推进。
    Active,
    /// The goal is paused by the user.
    /// 目标已被用户暂停。
    Paused,
    /// The goal is blocked on user or external input.
    /// 目标正等待用户或外部输入。
    Blocked,
    /// The goal reached a provider or runtime usage limit.
    /// 目标已触达 Provider 或运行时用量限制。
    UsageLimited,
    /// The goal consumed its explicit token budget.
    /// 目标已消耗显式 Token 预算。
    BudgetLimited,
    /// The goal completed successfully.
    /// 目标已成功完成。
    Complete,
}

impl SessionGoalStatus {
    /// Returns the stable snake_case protocol key for this status.
    /// 返回该状态稳定的 snake_case 协议键。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Paused => "paused",
            Self::Blocked => "blocked",
            Self::UsageLimited => "usage_limited",
            Self::BudgetLimited => "budget_limited",
            Self::Complete => "complete",
        }
    }
}

/// Derived working-time checkpoint used for client-only clock interpolation.
/// 供客户端本地时钟插值使用的派生工作时长检查点。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SessionGoalWorkTime {
    /// Active elapsed milliseconds, excluding paused intervals.
    /// 活动累计毫秒数，不含暂停区间。
    pub elapsed_ms: u64,
    /// Durable observation timestamp in Unix milliseconds, not browser receipt time.
    /// Unix 毫秒格式的持久化观察时间，不是浏览器收包时间。
    pub observed_at_ms: i64,
}

/// Describes the current long-running goal for one selected session.
/// 描述当前选中会话的长生命周期目标。
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SessionGoalSummary {
    /// Whether Goal budget usage is known from the lifetime baseline difference.
    /// Goal 预算用量是否能由生命周期基线差值确定。
    #[serde(default)]
    pub usage_known: bool,
    /// Stable session identifier that owns this goal.
    /// 拥有该目标的稳定会话标识。
    pub session_id: String,
    /// Stable goal version identifier used for stale update protection.
    /// 用于过期更新保护的稳定目标版本标识。
    pub goal_id: String,
    /// Human-readable goal objective.
    /// 人类可读的目标内容。
    pub objective: String,
    /// Stable status key: active, paused, blocked, usage_limited, budget_limited, or complete.
    /// 稳定状态键：active、paused、blocked、usage_limited、budget_limited 或 complete。
    pub status: SessionGoalStatus,
    /// Optional explicit token budget.
    /// 可选的显式 token 预算。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_budget: Option<i64>,
    /// Accounted token usage.
    /// 已记账 token 用量。
    pub tokens_used: i64,
    /// Live usage total displayed inside the goal details hover card.
    /// 目标详情悬浮卡展示的实时用量总数。
    pub usage_tokens: i64,
    /// Provider API calls observed for the current goal turn.
    /// 当前目标轮次观测到的 Provider API 调用次数。
    pub api_call_count: usize,
    /// Tool calls observed for the current goal turn.
    /// 当前目标轮次观测到的工具调用次数。
    pub tool_call_count: usize,
    /// Cumulative uncached input tokens observed for the current goal.
    /// 当前目标累计观测到的新增输入 Token 数。
    pub input_tokens: i64,
    /// Cumulative cache-hit input tokens observed for the current goal.
    /// 当前目标累计观测到的缓存命中输入 Token 数。
    pub cached_input_tokens: i64,
    /// Cumulative output tokens observed for the current goal.
    /// 当前目标累计观测到的输出 Token 数。
    pub output_tokens: i64,
    /// Cumulative reasoning tokens observed inside the current goal output.
    /// 当前目标输出中累计观测到的思考 Token 数。
    #[serde(default)]
    pub reasoning_tokens: i64,
    /// Cumulative provider tokens observed for the current goal.
    /// 当前目标累计观测到的 Provider 总 Token 数。
    pub total_tokens: i64,
    /// Accounted wall-clock usage in seconds.
    /// 已记账墙钟用量秒数。
    pub time_used_seconds: i64,
    /// Exact event-derived clock; absent only in drafts or older snapshots.
    /// 精确事件派生时钟；仅草稿或旧快照缺失。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub work_time: Option<SessionGoalWorkTime>,
    /// Creation timestamp in Unix epoch milliseconds.
    /// Unix 纪元毫秒格式的创建时间。
    pub created_at_ms: i64,
    /// Last update timestamp in Unix epoch milliseconds.
    /// Unix 纪元毫秒格式的最近更新时间。
    pub updated_at_ms: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies the strict status enum preserves the existing snake_case JSON protocol.
    /// 验证严格状态枚举保留既有 snake_case JSON 协议。
    #[test]
    fn session_goal_status_serializes_to_stable_protocol_key() {
        assert_eq!(
            serde_json::to_value(SessionGoalStatus::BudgetLimited)
                .expect("goal status should serialize"),
            serde_json::json!("budget_limited")
        );
    }

    /// Verifies unknown goal status keys are rejected at the service API boundary.
    /// 验证未知目标状态键会在 service API 边界被拒绝。
    #[test]
    fn session_goal_status_rejects_unknown_protocol_key() {
        let error = serde_json::from_value::<SessionGoalStatus>(serde_json::json!("unknown"))
            .expect_err("unknown goal status must not deserialize");

        assert!(error.to_string().contains("unknown variant"));
    }

    /// Verifies every service status exposes the same key emitted through Serde.
    /// 验证每个 service 状态暴露的键与 Serde 发出的键一致。
    #[test]
    fn session_goal_status_as_str_matches_serde() {
        let statuses = [
            SessionGoalStatus::Active,
            SessionGoalStatus::Paused,
            SessionGoalStatus::Blocked,
            SessionGoalStatus::UsageLimited,
            SessionGoalStatus::BudgetLimited,
            SessionGoalStatus::Complete,
        ];
        for status in statuses {
            assert_eq!(
                serde_json::to_value(status).expect("goal status should serialize"),
                serde_json::json!(status.as_str())
            );
        }
    }
}
