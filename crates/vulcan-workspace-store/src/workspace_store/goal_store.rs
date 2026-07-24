//! Controller-backed session goal storage aligned with Codex thread goal semantics.
//! 与 Codex 线程目标语义对齐的控制器承载会话目标存储。

use super::{app_sqlite_binding_id, current_unix_millis, decode_query_rows};
use crate::data_plane::{APP_SPACE_ID, ControllerRuntimeHandle};
use crate::model::SessionId;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use vldb_controller_client::ControllerSqliteValue;

/// Maximum accepted Unicode-scalar length for one persisted thread-goal objective.
/// 单个持久化线程目标文本允许的最大 Unicode 标量长度。
pub const MAX_THREAD_GOAL_OBJECTIVE_CHARS: usize = 4_000;

/// Domain validation failure for a persisted thread-goal objective.
/// 持久化线程目标文本的领域校验失败。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ThreadGoalObjectiveError {
    /// The normalized objective is empty.
    /// 规范化后的目标文本为空。
    EmptyObjective,
    /// The normalized objective exceeds the shared maximum length.
    /// 规范化后的目标文本超过共享最大长度。
    ObjectiveTooLong {
        /// Maximum accepted Unicode-scalar count.
        /// 允许的最大 Unicode 标量数量。
        max_chars: usize,
    },
}

impl std::fmt::Display for ThreadGoalObjectiveError {
    /// Formats one domain validation failure for storage diagnostics.
    /// 为存储诊断格式化一条领域校验失败。
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyObjective => formatter.write_str("goal objective must not be empty"),
            Self::ObjectiveTooLong { max_chars } => {
                write!(
                    formatter,
                    "goal objective must be at most {max_chars} characters"
                )
            }
        }
    }
}

impl std::error::Error for ThreadGoalObjectiveError {}

/// Domain validation failure for one explicit thread-goal token budget.
/// 单个显式线程目标 Token 预算的领域校验失败。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ThreadGoalTokenBudgetError;

impl std::fmt::Display for ThreadGoalTokenBudgetError {
    /// Formats the non-positive budget failure for storage diagnostics.
    /// 为存储诊断格式化非正预算失败。
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("goal token budget must be a positive integer")
    }
}

impl std::error::Error for ThreadGoalTokenBudgetError {}

/// Trims and validates one thread-goal objective using the persisted domain contract.
/// 使用持久化领域契约裁剪并校验一个线程目标文本。
///
/// # Parameters
/// 参数
/// `objective` is the untrusted objective text supplied by any mutation entrypoint.
/// `objective` 是任意变更入口提供的不可信目标文本。
///
/// # Returns
/// 返回值
/// The normalized borrowed objective, or the precise domain validation failure.
/// 返回规范化后的借用目标文本，或准确的领域校验失败。
pub fn normalize_thread_goal_objective(objective: &str) -> Result<&str, ThreadGoalObjectiveError> {
    let objective = objective.trim();
    if objective.is_empty() {
        return Err(ThreadGoalObjectiveError::EmptyObjective);
    }
    if objective.chars().count() > MAX_THREAD_GOAL_OBJECTIVE_CHARS {
        return Err(ThreadGoalObjectiveError::ObjectiveTooLong {
            max_chars: MAX_THREAD_GOAL_OBJECTIVE_CHARS,
        });
    }
    Ok(objective)
}

/// Validates one optional explicit token budget using the persisted domain contract.
/// 使用持久化领域契约校验一个可选的显式 Token 预算。
///
/// # Parameters
/// 参数
/// `token_budget` is absent for an unbounded goal or contains its explicit positive budget.
/// `token_budget` 对无预算目标为空，否则包含显式正预算。
///
/// # Returns
/// 返回值
/// Success for absent or positive budgets, otherwise the precise domain validation failure.
/// 预算缺失或为正数时成功，否则返回准确的领域校验失败。
pub fn validate_thread_goal_token_budget(
    token_budget: Option<i64>,
) -> Result<(), ThreadGoalTokenBudgetError> {
    if token_budget.is_some_and(|value| value <= 0) {
        return Err(ThreadGoalTokenBudgetError);
    }
    Ok(())
}

/// Persisted lifecycle state for one long-running session goal.
/// 单个长生命周期会话目标的持久化生命周期状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThreadGoalStatus {
    /// The goal is actively being pursued by automatic continuation.
    /// 目标正在由自动续跑流程推进。
    Active,
    /// The goal is paused and should not auto-continue.
    /// 目标已暂停，不应自动续跑。
    Paused,
    /// The goal is blocked and needs user or external input before resuming.
    /// 目标已受阻，需要用户或外部输入后才能恢复。
    Blocked,
    /// The goal hit a provider or runtime usage limit.
    /// 目标触达 Provider 或运行时用量限制。
    UsageLimited,
    /// The goal consumed its explicit token budget.
    /// 目标已消耗显式 token 预算。
    BudgetLimited,
    /// The goal has been completed.
    /// 目标已经完成。
    Complete,
}

impl ThreadGoalStatus {
    /// Returns the stable persisted snake_case key for this status.
    /// 返回该状态稳定持久化使用的 snake_case 键。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Paused => "paused",
            Self::Blocked => "blocked",
            Self::UsageLimited => "usage_limited",
            Self::BudgetLimited => "budget_limited",
            Self::Complete => "complete",
        }
    }

    /// Returns whether the status represents an actively continuing goal.
    /// 返回该状态是否表示正在继续推进的目标。
    pub fn is_active(self) -> bool {
        self == Self::Active
    }

    /// Returns whether the status is terminal for replacement and continuation decisions.
    /// 返回该状态在替换与续跑决策中是否属于终态。
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::BudgetLimited | Self::Complete)
    }
}

impl TryFrom<&str> for ThreadGoalStatus {
    type Error = String;

    /// Parses one persisted goal status key.
    /// 解析一个已持久化的目标状态键。
    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "active" => Ok(Self::Active),
            "paused" => Ok(Self::Paused),
            "blocked" => Ok(Self::Blocked),
            "usage_limited" => Ok(Self::UsageLimited),
            "budget_limited" => Ok(Self::BudgetLimited),
            "complete" => Ok(Self::Complete),
            other => Err(format!("unknown thread goal status `{other}`")),
        }
    }
}

/// Full persisted goal record loaded from the controller SQLite store.
/// 从控制器 SQLite 存储加载的完整持久化目标记录。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadGoal {
    /// Session identifier that owns this goal.
    /// 拥有该目标的会话标识。
    pub thread_id: SessionId,
    /// Stable goal version identifier used for stale update protection.
    /// 用于防止过期更新的稳定目标版本标识。
    pub goal_id: String,
    /// Human-authored goal objective.
    /// 用户编写的目标内容。
    pub objective: String,
    /// Current lifecycle status.
    /// 当前生命周期状态。
    pub status: ThreadGoalStatus,
    /// Optional explicit token budget.
    /// 可选的显式 token 预算。
    pub token_budget: Option<i64>,
    /// Accounted token usage for this goal.
    /// 当前目标已记账的 token 用量。
    pub tokens_used: i64,
    /// Accounted input tokens that were not served from cache.
    /// 当前目标已记账的非缓存输入 Token 数。
    pub uncached_input_tokens: i64,
    /// Accounted input tokens served from cache.
    /// 当前目标已记账的缓存输入 Token 数。
    pub cached_input_tokens: i64,
    /// Accounted complete output tokens.
    /// 当前目标已记账的完整输出 Token 数。
    pub output_tokens: i64,
    /// Accounted reasoning tokens reported as an output detail.
    /// 当前目标已记账的思考 Token 输出明细。
    pub reasoning_tokens: i64,
    /// Accounted provider-reported total tokens.
    /// 当前目标已记账的 Provider 总 Token 数。
    pub total_tokens: i64,
    /// Accounted wall-clock time in seconds.
    /// 当前目标已记账的墙钟时间秒数。
    pub time_used_seconds: i64,
    /// Creation timestamp in Unix epoch milliseconds.
    /// 创建时间的 Unix 纪元毫秒值。
    pub created_at_ms: i64,
    /// Last update timestamp in Unix epoch milliseconds.
    /// 最近更新时间的 Unix 纪元毫秒值。
    pub updated_at_ms: i64,
}

/// Partial goal update accepted by the Codex-compatible goal store.
/// 与 Codex 兼容的目标存储接受的局部目标更新。
#[derive(Debug, Clone, Default)]
pub struct GoalUpdate {
    /// Optional replacement objective.
    /// 可选的替换目标内容。
    pub objective: Option<String>,
    /// Optional replacement status.
    /// 可选的替换状态。
    pub status: Option<ThreadGoalStatus>,
    /// Optional replacement token budget, where `Some(None)` clears it.
    /// 可选的替换 token 预算，其中 `Some(None)` 表示清除预算。
    pub token_budget: Option<Option<i64>>,
    /// Expected goal id used to ignore stale writes.
    /// 用于忽略过期写入的预期目标标识。
    pub expected_goal_id: Option<String>,
    /// Expected persisted status used to reject lifecycle races.
    /// 用于拒绝生命周期竞态的预期持久化状态。
    pub expected_status: Option<ThreadGoalStatus>,
}

/// Strict mutation payload for starting a fresh budget stage from one stopped goal.
/// 从已停止目标启动新预算阶段的严格变更载荷。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoalResume {
    /// Objective carried into the new stage.
    /// 带入新阶段的目标文本。
    pub objective: String,
    /// Explicit token budget for the new stage, or no limit.
    /// 新阶段的显式 Token 预算，或不设限制。
    pub token_budget: Option<i64>,
    /// Goal identity of the stopped stage being replaced.
    /// 被替换停止阶段的目标身份。
    pub expected_goal_id: String,
    /// Lifecycle status required for the stopped stage.
    /// 停止阶段必须匹配的生命周期状态。
    pub expected_status: ThreadGoalStatus,
}

/// Result of applying usage accounting to one goal.
/// 对单个目标应用用量记账后的结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GoalAccountingOutcome {
    /// No row changed; includes the latest goal if it still exists.
    /// 没有行被修改；如果目标仍存在则携带最新目标。
    Unchanged(Option<ThreadGoal>),
    /// The goal row was updated.
    /// 目标行已经更新。
    Updated(ThreadGoal),
}

/// Exact additive usage reported by one successful physical provider request.
/// 单次成功物理 Provider 请求报告的精确累加用量。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThreadGoalProviderUsage {
    /// Input tokens that were not served from cache.
    /// 未由缓存提供的输入 Token 数。
    pub uncached_input_tokens: i64,
    /// Input tokens served from cache.
    /// 由缓存提供的输入 Token 数。
    pub cached_input_tokens: i64,
    /// Complete output tokens.
    /// 完整输出 Token 数。
    pub output_tokens: i64,
    /// Reasoning tokens reported as an output detail.
    /// 作为输出明细报告的思考 Token 数。
    pub reasoning_tokens: i64,
    /// Provider-reported total tokens.
    /// Provider 报告的总 Token 数。
    pub total_tokens: i64,
}

impl ThreadGoalProviderUsage {
    /// Returns the budgeted sum of uncached input and complete output.
    /// 返回由非缓存输入与完整输出组成的预算记账总量。
    pub fn budget_tokens(self) -> i64 {
        self.uncached_input_tokens
            .saturating_add(self.output_tokens)
    }
}

/// Controller-backed implementation of the Codex-compatible goal store API.
/// Codex 兼容目标存储 API 的控制器承载实现。
#[derive(Debug, Clone)]
pub struct VulcanGoalStore {
    /// Shared controller runtime used to execute SQLite statements.
    /// 用于执行 SQLite 语句的共享控制器运行时。
    controller: ControllerRuntimeHandle,
}

#[derive(Debug, Deserialize)]
struct ThreadGoalRow {
    /// Persisted session identifier.
    /// 持久化会话标识。
    thread_id: String,
    /// Persisted goal version identifier.
    /// 持久化目标版本标识。
    goal_id: String,
    /// Persisted objective text.
    /// 持久化目标文本。
    objective: String,
    /// Persisted lifecycle status key.
    /// 持久化生命周期状态键。
    status: String,
    /// Persisted optional token budget.
    /// 持久化的可选 token 预算。
    token_budget: Option<i64>,
    /// Persisted token usage.
    /// 持久化 token 用量。
    tokens_used: i64,
    /// Persisted non-cached input usage.
    /// 持久化非缓存输入用量。
    uncached_input_tokens: i64,
    /// Persisted cached input usage.
    /// 持久化缓存输入用量。
    cached_input_tokens: i64,
    /// Persisted complete output usage.
    /// 持久化完整输出用量。
    output_tokens: i64,
    /// Persisted reasoning output detail.
    /// 持久化思考输出明细。
    reasoning_tokens: i64,
    /// Persisted provider total usage.
    /// 持久化 Provider 总用量。
    total_tokens: i64,
    /// Persisted wall-clock usage in seconds.
    /// 持久化墙钟用量秒数。
    time_used_seconds: i64,
    /// Persisted creation timestamp.
    /// 持久化创建时间戳。
    created_at_ms: i64,
    /// Persisted update timestamp.
    /// 持久化更新时间戳。
    updated_at_ms: i64,
}

impl VulcanGoalStore {
    /// Creates one goal store bound to the shared controller runtime.
    /// 创建一个绑定到共享控制器运行时的目标存储。
    pub fn new(controller: ControllerRuntimeHandle) -> Self {
        Self { controller }
    }

    /// Reads the current goal for one session.
    /// 读取单个会话的当前目标。
    pub async fn get_thread_goal(
        &self,
        thread_id: SessionId,
    ) -> Result<Option<ThreadGoal>, String> {
        let rows = self
            .query_goal_rows(
                r#"
SELECT
    thread_id,
    goal_id,
    objective,
    status,
    token_budget,
    tokens_used,
    uncached_input_tokens,
    cached_input_tokens,
    output_tokens,
    reasoning_tokens,
    total_tokens,
    time_used_seconds,
    created_at_ms,
    updated_at_ms
FROM thread_goals
WHERE thread_id = ?1;
"#,
                vec![ControllerSqliteValue::String(thread_id.to_string())],
            )
            .await?;
        rows.into_iter()
            .next()
            .map(thread_goal_from_row)
            .transpose()
    }

    /// Replaces any existing goal with a new goal version.
    /// 用新的目标版本替换任何现有目标。
    pub async fn replace_thread_goal(
        &self,
        thread_id: SessionId,
        objective: &str,
        status: ThreadGoalStatus,
        token_budget: Option<i64>,
    ) -> Result<ThreadGoal, String> {
        let objective =
            normalize_thread_goal_objective(objective).map_err(|error| error.to_string())?;
        validate_thread_goal_token_budget(token_budget).map_err(|error| error.to_string())?;
        let goal_id = Uuid::new_v4().to_string();
        let now_ms = current_unix_millis();
        let status = status_after_budget_limit(status, 0, token_budget);
        let rows = self
            .query_goal_rows(
                r#"
INSERT INTO thread_goals (
    thread_id,
    goal_id,
    objective,
    status,
    token_budget,
    tokens_used,
    uncached_input_tokens,
    cached_input_tokens,
    output_tokens,
    reasoning_tokens,
    total_tokens,
    time_used_seconds,
    created_at_ms,
    updated_at_ms
) VALUES (?1, ?2, ?3, ?4, ?5, 0, 0, 0, 0, 0, 0, 0, ?6, ?7)
ON CONFLICT(thread_id) DO UPDATE SET
    goal_id = excluded.goal_id,
    objective = excluded.objective,
    status = excluded.status,
    token_budget = excluded.token_budget,
    tokens_used = 0,
    uncached_input_tokens = 0,
    cached_input_tokens = 0,
    output_tokens = 0,
    reasoning_tokens = 0,
    total_tokens = 0,
    time_used_seconds = 0,
    created_at_ms = excluded.created_at_ms,
    updated_at_ms = excluded.updated_at_ms
RETURNING
    thread_id,
    goal_id,
    objective,
    status,
    token_budget,
    tokens_used,
    uncached_input_tokens,
    cached_input_tokens,
    output_tokens,
    reasoning_tokens,
    total_tokens,
    time_used_seconds,
    created_at_ms,
    updated_at_ms;
"#,
                vec![
                    ControllerSqliteValue::String(thread_id.to_string()),
                    ControllerSqliteValue::String(goal_id),
                    ControllerSqliteValue::String(objective.to_string()),
                    ControllerSqliteValue::String(status.as_str().to_string()),
                    optional_i64(token_budget),
                    ControllerSqliteValue::Int64(now_ms),
                    ControllerSqliteValue::Int64(now_ms),
                ],
            )
            .await?;
        one_goal_row(rows, "replace thread goal")
    }

    /// Inserts a new goal only when no unfinished goal exists.
    /// 仅在不存在未完成目标时插入新目标。
    pub async fn insert_thread_goal(
        &self,
        thread_id: SessionId,
        objective: &str,
        status: ThreadGoalStatus,
        token_budget: Option<i64>,
    ) -> Result<Option<ThreadGoal>, String> {
        let objective =
            normalize_thread_goal_objective(objective).map_err(|error| error.to_string())?;
        validate_thread_goal_token_budget(token_budget).map_err(|error| error.to_string())?;
        let goal_id = Uuid::new_v4().to_string();
        let now_ms = current_unix_millis();
        let status = status_after_budget_limit(status, 0, token_budget);
        let rows = self
            .query_goal_rows(
                r#"
INSERT INTO thread_goals (
    thread_id,
    goal_id,
    objective,
    status,
    token_budget,
    tokens_used,
    uncached_input_tokens,
    cached_input_tokens,
    output_tokens,
    reasoning_tokens,
    total_tokens,
    time_used_seconds,
    created_at_ms,
    updated_at_ms
) VALUES (?1, ?2, ?3, ?4, ?5, 0, 0, 0, 0, 0, 0, 0, ?6, ?7)
ON CONFLICT(thread_id) DO UPDATE SET
    goal_id = excluded.goal_id,
    objective = excluded.objective,
    status = excluded.status,
    token_budget = excluded.token_budget,
    tokens_used = 0,
    uncached_input_tokens = 0,
    cached_input_tokens = 0,
    output_tokens = 0,
    reasoning_tokens = 0,
    total_tokens = 0,
    time_used_seconds = 0,
    created_at_ms = excluded.created_at_ms,
    updated_at_ms = excluded.updated_at_ms
WHERE thread_goals.status = 'complete'
RETURNING
    thread_id,
    goal_id,
    objective,
    status,
    token_budget,
    tokens_used,
    uncached_input_tokens,
    cached_input_tokens,
    output_tokens,
    reasoning_tokens,
    total_tokens,
    time_used_seconds,
    created_at_ms,
    updated_at_ms;
"#,
                vec![
                    ControllerSqliteValue::String(thread_id.to_string()),
                    ControllerSqliteValue::String(goal_id),
                    ControllerSqliteValue::String(objective.to_string()),
                    ControllerSqliteValue::String(status.as_str().to_string()),
                    optional_i64(token_budget),
                    ControllerSqliteValue::Int64(now_ms),
                    ControllerSqliteValue::Int64(now_ms),
                ],
            )
            .await?;
        rows.into_iter()
            .next()
            .map(thread_goal_from_row)
            .transpose()
    }

    /// Updates an existing goal while preserving Codex budget and stale-id semantics.
    /// 在保留 Codex 预算与过期标识语义的前提下更新现有目标。
    pub async fn update_thread_goal(
        &self,
        thread_id: SessionId,
        update: GoalUpdate,
    ) -> Result<Option<ThreadGoal>, String> {
        let existing = self.get_thread_goal(thread_id).await?;
        let Some(existing) = existing else {
            return Ok(None);
        };
        if let Some(expected_goal_id) = update.expected_goal_id.as_deref()
            && existing.goal_id != expected_goal_id
        {
            return Ok(None);
        }
        if let Some(expected_status) = update.expected_status
            && existing.status != expected_status
        {
            return Ok(None);
        }

        let objective = update.objective.unwrap_or(existing.objective);
        let objective = normalize_thread_goal_objective(&objective)
            .map_err(|error| error.to_string())?
            .to_string();
        let token_budget = update.token_budget.unwrap_or(existing.token_budget);
        validate_thread_goal_token_budget(token_budget).map_err(|error| error.to_string())?;
        let status = update.status.unwrap_or(existing.status);
        let status = status_after_budget_limit(status, existing.tokens_used, token_budget);
        let now_ms = current_unix_millis();
        let rows = self
            .query_goal_rows(
                r#"
UPDATE thread_goals
SET
    objective = ?1,
    status = ?2,
    token_budget = ?3,
    updated_at_ms = ?4
WHERE thread_id = ?5
  AND (?6 IS NULL OR goal_id = ?6)
  AND (?7 IS NULL OR status = ?7)
RETURNING
    thread_id,
    goal_id,
    objective,
    status,
    token_budget,
    tokens_used,
    uncached_input_tokens,
    cached_input_tokens,
    output_tokens,
    reasoning_tokens,
    total_tokens,
    time_used_seconds,
    created_at_ms,
    updated_at_ms;
"#,
                vec![
                    ControllerSqliteValue::String(objective),
                    ControllerSqliteValue::String(status.as_str().to_string()),
                    optional_i64(token_budget),
                    ControllerSqliteValue::Int64(now_ms),
                    ControllerSqliteValue::String(thread_id.to_string()),
                    optional_string(update.expected_goal_id),
                    optional_thread_goal_status(update.expected_status),
                ],
            )
            .await?;
        rows.into_iter()
            .next()
            .map(thread_goal_from_row)
            .transpose()
    }

    /// Resumes one stopped goal as a fresh active budget stage.
    /// 将一个已停止目标作为新的活动预算阶段恢复。
    ///
    /// # Parameters
    /// 参数
    /// `thread_id` identifies the session whose stopped stage is being replaced.
    /// `thread_id` 标识正在替换停止阶段的会话。
    /// `resume` carries the new stage content and strict previous-stage preconditions.
    /// `resume` 携带新阶段内容与严格的上一阶段前置条件。
    ///
    /// # Returns
    /// 返回值
    /// A fresh active stage with a new Goal identity, or `None` when the previous stage changed.
    /// 返回具有新 Goal 身份的活动阶段；上一阶段已变化时返回 `None`。
    pub async fn resume_thread_goal(
        &self,
        thread_id: SessionId,
        resume: GoalResume,
    ) -> Result<Option<ThreadGoal>, String> {
        let existing = self.get_thread_goal(thread_id).await?;
        let Some(existing) = existing else {
            return Ok(None);
        };
        if existing.goal_id != resume.expected_goal_id {
            return Ok(None);
        }
        if existing.status != resume.expected_status {
            return Ok(None);
        }
        if !matches!(
            existing.status,
            ThreadGoalStatus::Paused
                | ThreadGoalStatus::Blocked
                | ThreadGoalStatus::UsageLimited
                | ThreadGoalStatus::BudgetLimited
        ) {
            return Ok(Some(existing));
        }

        let objective = normalize_thread_goal_objective(&resume.objective)
            .map_err(|error| error.to_string())?
            .to_string();
        let token_budget = resume.token_budget;
        validate_thread_goal_token_budget(token_budget).map_err(|error| error.to_string())?;
        let resumed_goal_id = Uuid::new_v4().to_string();
        let now_ms = current_unix_millis();
        let rows = self
            .query_goal_rows(
                r#"
UPDATE thread_goals
SET
    goal_id = ?1,
    objective = ?2,
    status = 'active',
    token_budget = ?3,
    tokens_used = 0,
    uncached_input_tokens = 0,
    cached_input_tokens = 0,
    output_tokens = 0,
    reasoning_tokens = 0,
    total_tokens = 0,
    time_used_seconds = 0,
    created_at_ms = ?4,
    updated_at_ms = ?4
WHERE thread_id = ?5
  AND status IN ('paused', 'blocked', 'usage_limited', 'budget_limited')
  AND goal_id = ?6
  AND status = ?7
RETURNING
    thread_id,
    goal_id,
    objective,
    status,
    token_budget,
    tokens_used,
    uncached_input_tokens,
    cached_input_tokens,
    output_tokens,
    reasoning_tokens,
    total_tokens,
    time_used_seconds,
    created_at_ms,
    updated_at_ms;
"#,
                vec![
                    ControllerSqliteValue::String(resumed_goal_id),
                    ControllerSqliteValue::String(objective),
                    optional_i64(token_budget),
                    ControllerSqliteValue::Int64(now_ms),
                    ControllerSqliteValue::String(thread_id.to_string()),
                    ControllerSqliteValue::String(resume.expected_goal_id),
                    ControllerSqliteValue::String(resume.expected_status.as_str().to_string()),
                ],
            )
            .await?;
        rows.into_iter()
            .next()
            .map(thread_goal_from_row)
            .transpose()
    }

    /// Pauses one exact active or budget-limited goal captured by its owning run.
    /// 暂停由所属运行捕获的精确 active 或 budget_limited 目标。
    ///
    /// # Parameters
    /// 参数
    /// `thread_id` identifies the session that owns the stopped run.
    /// `thread_id` 标识拥有已停止运行的会话。
    /// `expected_goal_id` binds the pause to the Goal captured when that run started.
    /// `expected_goal_id` 将暂停绑定到该运行启动时捕获的目标。
    ///
    /// # Returns
    /// 返回值
    /// The paused goal, or `None` when its identity or pausable status no longer matches.
    /// 返回已暂停目标；身份或可暂停状态不再匹配时返回 `None`。
    pub async fn pause_thread_goal(
        &self,
        thread_id: SessionId,
        expected_goal_id: &str,
    ) -> Result<Option<ThreadGoal>, String> {
        let rows = self
            .query_goal_rows(
                r#"
UPDATE thread_goals
SET
    status = 'paused',
    updated_at_ms = ?1
WHERE thread_id = ?2
  AND goal_id = ?3
  AND status IN ('active', 'budget_limited')
RETURNING
    thread_id,
    goal_id,
    objective,
    status,
    token_budget,
    tokens_used,
    uncached_input_tokens,
    cached_input_tokens,
    output_tokens,
    reasoning_tokens,
    total_tokens,
    time_used_seconds,
    created_at_ms,
    updated_at_ms;
"#,
                vec![
                    ControllerSqliteValue::Int64(current_unix_millis()),
                    ControllerSqliteValue::String(thread_id.to_string()),
                    ControllerSqliteValue::String(expected_goal_id.to_string()),
                ],
            )
            .await?;
        rows.into_iter()
            .next()
            .map(thread_goal_from_row)
            .transpose()
    }

    /// Deletes and returns one exact goal captured by its caller.
    /// 删除并返回由调用方捕获的精确目标。
    ///
    /// # Parameters
    /// 参数
    /// `thread_id` identifies the session that owns the goal.
    /// `thread_id` 标识拥有目标的会话。
    /// `expected_goal_id` prevents a stale request from deleting a replacement goal.
    /// `expected_goal_id` 防止过期请求删除替换后的目标。
    ///
    /// # Returns
    /// 返回值
    /// The deleted goal, or `None` when the captured goal no longer exists.
    /// 返回已删除目标；捕获的目标已不存在时返回 `None`。
    pub async fn delete_thread_goal(
        &self,
        thread_id: SessionId,
        expected_goal_id: &str,
    ) -> Result<Option<ThreadGoal>, String> {
        let rows = self
            .query_goal_rows(
                r#"
DELETE FROM thread_goals
WHERE thread_id = ?1
  AND goal_id = ?2
RETURNING
    thread_id,
    goal_id,
    objective,
    status,
    token_budget,
    tokens_used,
    uncached_input_tokens,
    cached_input_tokens,
    output_tokens,
    reasoning_tokens,
    total_tokens,
    time_used_seconds,
    created_at_ms,
    updated_at_ms;
"#,
                vec![
                    ControllerSqliteValue::String(thread_id.to_string()),
                    ControllerSqliteValue::String(expected_goal_id.to_string()),
                ],
            )
            .await?;
        rows.into_iter()
            .next()
            .map(thread_goal_from_row)
            .transpose()
    }

    /// Accounts one successful physical provider request for an exact request-bound goal.
    /// 为精确请求绑定目标记账一次成功的物理 Provider 请求。
    ///
    /// # Parameters
    /// 参数
    /// `thread_id` identifies the session that owns the goal.
    /// `thread_id` 标识拥有目标的会话。
    /// `usage` carries the exact normalized additive token facts.
    /// `usage` 携带精确标准化的可累加 Token 事实。
    /// `expected_goal_id` binds the request to the goal that was active when it started.
    /// `expected_goal_id` 将请求绑定到其启动时处于活动状态的目标。
    ///
    /// # Returns
    /// 返回值
    /// The atomically updated goal without reviving stopped states, or the latest unchanged goal when the binding no longer matches.
    /// 返回原子更新且不复活停止状态的目标；绑定不再匹配时返回最新的未变更目标。
    pub async fn account_thread_goal_provider_usage(
        &self,
        thread_id: SessionId,
        usage: ThreadGoalProviderUsage,
        expected_goal_id: &str,
    ) -> Result<GoalAccountingOutcome, String> {
        let usage = ThreadGoalProviderUsage {
            uncached_input_tokens: usage.uncached_input_tokens.max(0),
            cached_input_tokens: usage.cached_input_tokens.max(0),
            output_tokens: usage.output_tokens.max(0),
            reasoning_tokens: usage.reasoning_tokens.max(0),
            total_tokens: usage.total_tokens.max(0),
        };
        let budget_tokens = usage.budget_tokens();
        let now_ms = current_unix_millis();
        let rows = self
            .query_goal_rows(
                r#"
UPDATE thread_goals
SET
    tokens_used = CASE
        WHEN tokens_used > 9223372036854775807 - ?1 THEN 9223372036854775807
        ELSE tokens_used + ?1
    END,
    uncached_input_tokens = CASE
        WHEN uncached_input_tokens > 9223372036854775807 - ?2 THEN 9223372036854775807
        ELSE uncached_input_tokens + ?2
    END,
    cached_input_tokens = CASE
        WHEN cached_input_tokens > 9223372036854775807 - ?3 THEN 9223372036854775807
        ELSE cached_input_tokens + ?3
    END,
    output_tokens = CASE
        WHEN output_tokens > 9223372036854775807 - ?4 THEN 9223372036854775807
        ELSE output_tokens + ?4
    END,
    reasoning_tokens = CASE
        WHEN reasoning_tokens > 9223372036854775807 - ?5 THEN 9223372036854775807
        ELSE reasoning_tokens + ?5
    END,
    total_tokens = CASE
        WHEN total_tokens > 9223372036854775807 - ?6 THEN 9223372036854775807
        ELSE total_tokens + ?6
    END,
    status = CASE
        WHEN status = 'active'
         AND token_budget IS NOT NULL
         AND CASE
                WHEN tokens_used > 9223372036854775807 - ?1
                    THEN 9223372036854775807
                ELSE tokens_used + ?1
             END >= token_budget
            THEN 'budget_limited'
        ELSE status
    END,
    updated_at_ms = ?7
WHERE thread_id = ?8
  AND goal_id = ?9
RETURNING
    thread_id,
    goal_id,
    objective,
    status,
    token_budget,
    tokens_used,
    uncached_input_tokens,
    cached_input_tokens,
    output_tokens,
    reasoning_tokens,
    total_tokens,
    time_used_seconds,
    created_at_ms,
    updated_at_ms;
"#,
                vec![
                    ControllerSqliteValue::Int64(budget_tokens),
                    ControllerSqliteValue::Int64(usage.uncached_input_tokens),
                    ControllerSqliteValue::Int64(usage.cached_input_tokens),
                    ControllerSqliteValue::Int64(usage.output_tokens),
                    ControllerSqliteValue::Int64(usage.reasoning_tokens),
                    ControllerSqliteValue::Int64(usage.total_tokens),
                    ControllerSqliteValue::Int64(now_ms),
                    ControllerSqliteValue::String(thread_id.to_string()),
                    ControllerSqliteValue::String(expected_goal_id.to_string()),
                ],
            )
            .await?;
        let Some(row) = rows.into_iter().next() else {
            return Ok(GoalAccountingOutcome::Unchanged(
                self.get_thread_goal(thread_id).await?,
            ));
        };
        Ok(GoalAccountingOutcome::Updated(thread_goal_from_row(row)?))
    }

    /// Accounts token and time usage for one exact budget stage without changing stopped states.
    /// 为一个精确预算阶段记账 Token 与时间用量，同时不改变已停止状态。
    ///
    /// # Parameters
    /// 参数
    /// `thread_id` identifies the session that owns the goal.
    /// `thread_id` 标识拥有目标的会话。
    /// `tokens_delta` is the non-negative token increment applied with saturation.
    /// `tokens_delta` 是以饱和方式应用的非负 Token 增量。
    /// `time_delta_seconds` is the non-negative elapsed-time increment applied with saturation.
    /// `time_delta_seconds` 是以饱和方式应用的非负耗时增量。
    /// `expected_goal_id` binds accounting to the goal captured when execution started.
    /// `expected_goal_id` 将记账绑定到执行开始时捕获的目标。
    ///
    /// # Returns
    /// 返回值
    /// The atomically updated goal, or the latest unchanged goal when the binding no longer matches.
    /// 返回原子更新后的目标；绑定不再匹配时返回最新的未变更目标。
    pub async fn account_thread_goal_usage(
        &self,
        thread_id: SessionId,
        tokens_delta: i64,
        time_delta_seconds: i64,
        expected_goal_id: &str,
    ) -> Result<GoalAccountingOutcome, String> {
        let tokens_delta = tokens_delta.max(0);
        let time_delta_seconds = time_delta_seconds.max(0);
        let now_ms = current_unix_millis();
        // SQLite evaluates every right-hand expression against the pre-update row. Keeping the
        // SQLite 会针对更新前的行计算所有右值表达式。将增量、预算判定与状态保留放进
        // complete mutation in one statement prevents stale reports from changing stopped states,
        // 同一条语句，可防止过期报告改变已停止状态，
        // charging replacement stages, losing concurrent increments, or using an obsolete budget.
        // 误记替换后的阶段、丢失并发增量或使用过期预算。
        let rows = self
            .query_goal_rows(
                r#"
    UPDATE thread_goals
    SET
        tokens_used = CASE
            WHEN tokens_used > 9223372036854775807 - ?1 THEN 9223372036854775807
            ELSE tokens_used + ?1
        END,
        time_used_seconds = CASE
            WHEN time_used_seconds > 9223372036854775807 - ?2 THEN 9223372036854775807
            ELSE time_used_seconds + ?2
        END,
        status = CASE
            WHEN status = 'active'
             AND token_budget IS NOT NULL
             AND CASE
                    WHEN tokens_used > 9223372036854775807 - ?1
                        THEN 9223372036854775807
                    ELSE tokens_used + ?1
                 END >= token_budget
                THEN 'budget_limited'
            ELSE status
        END,
        updated_at_ms = ?3
WHERE thread_id = ?4
  AND goal_id = ?5
RETURNING
        thread_id,
        goal_id,
        objective,
        status,
        token_budget,
        tokens_used,
        uncached_input_tokens,
        cached_input_tokens,
        output_tokens,
        reasoning_tokens,
        total_tokens,
        time_used_seconds,
        created_at_ms,
        updated_at_ms;
    "#,
                vec![
                    ControllerSqliteValue::Int64(tokens_delta),
                    ControllerSqliteValue::Int64(time_delta_seconds),
                    ControllerSqliteValue::Int64(now_ms),
                    ControllerSqliteValue::String(thread_id.to_string()),
                    ControllerSqliteValue::String(expected_goal_id.to_string()),
                ],
            )
            .await?;
        let Some(row) = rows.into_iter().next() else {
            return Ok(GoalAccountingOutcome::Unchanged(
                self.get_thread_goal(thread_id).await?,
            ));
        };
        Ok(GoalAccountingOutcome::Updated(thread_goal_from_row(row)?))
    }

    /// Executes one SQLite statement that returns thread goal rows.
    /// 执行一条会返回目标行的 SQLite 语句。
    async fn query_goal_rows(
        &self,
        sql: &str,
        params: Vec<ControllerSqliteValue>,
    ) -> Result<Vec<ThreadGoalRow>, String> {
        let result = self
            .controller
            .client
            .query_sqlite_json_typed(
                APP_SPACE_ID,
                app_sqlite_binding_id(&self.controller),
                sql,
                params,
            )
            .await
            .map_err(|error| format!("执行目标 SQLite 查询失败：{error}"))?;
        decode_query_rows::<ThreadGoalRow>(&result.json_data)
    }
}

/// Converts one controller row into the public goal record.
/// 将一条控制器行转换为公开目标记录。
fn thread_goal_from_row(row: ThreadGoalRow) -> Result<ThreadGoal, String> {
    Ok(ThreadGoal {
        thread_id: Uuid::parse_str(&row.thread_id)
            .map_err(|error| format!("解析目标会话标识失败：{error}"))?,
        goal_id: row.goal_id,
        objective: row.objective,
        status: ThreadGoalStatus::try_from(row.status.as_str())?,
        token_budget: row.token_budget,
        tokens_used: row.tokens_used,
        uncached_input_tokens: row.uncached_input_tokens,
        cached_input_tokens: row.cached_input_tokens,
        output_tokens: row.output_tokens,
        reasoning_tokens: row.reasoning_tokens,
        total_tokens: row.total_tokens,
        time_used_seconds: row.time_used_seconds,
        created_at_ms: row.created_at_ms,
        updated_at_ms: row.updated_at_ms,
    })
}

/// Returns one goal row or a diagnostic when a returning write unexpectedly yields none.
/// 返回单个目标行；当写入返回意外为空时给出诊断。
fn one_goal_row(rows: Vec<ThreadGoalRow>, action: &str) -> Result<ThreadGoal, String> {
    rows.into_iter()
        .next()
        .ok_or_else(|| format!("{action} did not return a goal row"))
        .and_then(thread_goal_from_row)
}

/// Converts an optional integer into a controller SQLite value.
/// 将可选整数转换为控制器 SQLite 值。
fn optional_i64(value: Option<i64>) -> ControllerSqliteValue {
    value
        .map(ControllerSqliteValue::Int64)
        .unwrap_or(ControllerSqliteValue::Null)
}

/// Converts an optional string into a controller SQLite value.
/// 将可选字符串转换为控制器 SQLite 值。
fn optional_string(value: Option<String>) -> ControllerSqliteValue {
    value
        .map(ControllerSqliteValue::String)
        .unwrap_or(ControllerSqliteValue::Null)
}

/// Converts an optional goal status into a controller SQLite value.
/// 将可选目标状态转换为控制器 SQLite 值。
fn optional_thread_goal_status(value: Option<ThreadGoalStatus>) -> ControllerSqliteValue {
    value
        .map(|status| ControllerSqliteValue::String(status.as_str().to_string()))
        .unwrap_or(ControllerSqliteValue::Null)
}

/// Applies Codex immediate budget limiting to the requested status.
/// 将 Codex 即时预算限制规则应用到请求状态。
fn status_after_budget_limit(
    status: ThreadGoalStatus,
    tokens_used: i64,
    token_budget: Option<i64>,
) -> ThreadGoalStatus {
    if status == ThreadGoalStatus::Active
        && token_budget.is_some_and(|budget| tokens_used >= budget)
    {
        ThreadGoalStatus::BudgetLimited
    } else {
        status
    }
}

/// Returns whether one status can be accounted for the requested mode.
/// 返回某个状态是否可按请求模式进行记账。
#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies the shared objective contract trims valid text and rejects empty or oversized input.
    /// 验证共享目标文本契约会裁剪有效文本并拒绝空文本或超长输入。
    #[test]
    fn thread_goal_objective_contract_normalizes_and_validates() {
        assert_eq!(
            normalize_thread_goal_objective("  pursue the goal  "),
            Ok("pursue the goal")
        );
        assert_eq!(
            normalize_thread_goal_objective("  "),
            Err(ThreadGoalObjectiveError::EmptyObjective)
        );
        assert_eq!(
            normalize_thread_goal_objective(
                "x".repeat(MAX_THREAD_GOAL_OBJECTIVE_CHARS + 1).as_str()
            ),
            Err(ThreadGoalObjectiveError::ObjectiveTooLong {
                max_chars: MAX_THREAD_GOAL_OBJECTIVE_CHARS,
            })
        );
    }

    /// Verifies the shared budget contract accepts absent or positive values and rejects non-positive input.
    /// 验证共享预算契约接受缺失或正数预算，并拒绝非正输入。
    #[test]
    fn thread_goal_budget_contract_rejects_non_positive_values() {
        assert_eq!(validate_thread_goal_token_budget(None), Ok(()));
        assert_eq!(validate_thread_goal_token_budget(Some(1)), Ok(()));
        assert_eq!(
            validate_thread_goal_token_budget(Some(0)),
            Err(ThreadGoalTokenBudgetError)
        );
        assert_eq!(
            validate_thread_goal_token_budget(Some(-1)),
            Err(ThreadGoalTokenBudgetError)
        );
    }

    /// Verifies active goals are immediately budget-limited when already over budget.
    /// 验证 active 目标在已经超出预算时会立即转为 budget_limited。
    #[test]
    fn status_after_budget_limit_limits_active_goal() {
        assert_eq!(
            status_after_budget_limit(ThreadGoalStatus::Active, 10, Some(10)),
            ThreadGoalStatus::BudgetLimited
        );
        assert_eq!(
            status_after_budget_limit(ThreadGoalStatus::Active, 0, Some(10)),
            ThreadGoalStatus::Active
        );
        assert_eq!(
            status_after_budget_limit(ThreadGoalStatus::Paused, 10, Some(10)),
            ThreadGoalStatus::Paused
        );
    }

    /// Verifies goal budgets include uncached input and complete output without adding cache or reasoning twice.
    /// 验证目标预算包含非缓存输入与完整输出，且不会重复加入缓存或思考。
    #[test]
    fn provider_usage_budget_counts_uncached_input_plus_complete_output() {
        let usage = ThreadGoalProviderUsage {
            uncached_input_tokens: 17_593,
            cached_input_tokens: 27_904,
            output_tokens: 176,
            reasoning_tokens: 118,
            total_tokens: 45_673,
        };

        assert_eq!(usage.budget_tokens(), 17_769);
    }
}
