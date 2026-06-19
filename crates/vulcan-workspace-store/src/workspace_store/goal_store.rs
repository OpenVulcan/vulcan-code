//! Controller-backed session goal storage aligned with Codex thread goal semantics.
//! 与 Codex 线程目标语义对齐的控制器承载会话目标存储。

use super::{app_sqlite_binding_id, current_unix_millis, decode_query_rows};
use crate::data_plane::{APP_SPACE_ID, ControllerRuntimeHandle};
use crate::model::SessionId;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use vldb_controller_client::ControllerSqliteValue;

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

/// Usage-accounting status filter copied from Codex goal store semantics.
/// 从 Codex 目标存储语义复制的用量记账状态过滤器。
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GoalAccountingMode {
    /// Only active-status goals may be accounted.
    /// 只有 active 状态目标可被记账。
    ActiveStatusOnly,
    /// Only active goals may be accounted.
    /// 只有活动目标可被记账。
    ActiveOnly,
    /// Active or complete goals may be accounted for final completion turns.
    /// active 或 complete 目标可用于完成回合的最终记账。
    ActiveOrComplete,
    /// Active or stopped resumable goals may be accounted for in-flight turns.
    /// active 或已停止可恢复目标可用于飞行中回合的记账。
    ActiveOrStopped,
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
        let goal_id = Uuid::new_v4().to_string();
        let now_ms = current_unix_millis() as i64;
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
    time_used_seconds,
    created_at_ms,
    updated_at_ms
) VALUES (?1, ?2, ?3, ?4, ?5, 0, 0, ?6, ?7)
ON CONFLICT(thread_id) DO UPDATE SET
    goal_id = excluded.goal_id,
    objective = excluded.objective,
    status = excluded.status,
    token_budget = excluded.token_budget,
    tokens_used = 0,
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
        let goal_id = Uuid::new_v4().to_string();
        let now_ms = current_unix_millis() as i64;
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
    time_used_seconds,
    created_at_ms,
    updated_at_ms
) VALUES (?1, ?2, ?3, ?4, ?5, 0, 0, ?6, ?7)
ON CONFLICT(thread_id) DO UPDATE SET
    goal_id = excluded.goal_id,
    objective = excluded.objective,
    status = excluded.status,
    token_budget = excluded.token_budget,
    tokens_used = 0,
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
            return Ok(Some(existing));
        }

        let objective = update.objective.unwrap_or(existing.objective);
        let token_budget = update.token_budget.unwrap_or(existing.token_budget);
        let status = update.status.unwrap_or(existing.status);
        let status = status_after_budget_limit(status, existing.tokens_used, token_budget);
        let now_ms = current_unix_millis() as i64;
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
RETURNING
    thread_id,
    goal_id,
    objective,
    status,
    token_budget,
    tokens_used,
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
    pub async fn resume_thread_goal(
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
            return Ok(Some(existing));
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

        let objective = update.objective.unwrap_or(existing.objective);
        let token_budget = update.token_budget.unwrap_or(existing.token_budget);
        let status = status_after_budget_limit(ThreadGoalStatus::Active, 0, token_budget);
        let now_ms = current_unix_millis() as i64;
        let rows = self
            .query_goal_rows(
                r#"
UPDATE thread_goals
SET
    objective = ?1,
    status = ?2,
    token_budget = ?3,
    tokens_used = 0,
    time_used_seconds = 0,
    updated_at_ms = ?4
WHERE thread_id = ?5
  AND status IN ('paused', 'blocked', 'usage_limited', 'budget_limited')
  AND (?6 IS NULL OR goal_id = ?6)
RETURNING
    thread_id,
    goal_id,
    objective,
    status,
    token_budget,
    tokens_used,
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
                ],
            )
            .await?;
        rows.into_iter()
            .next()
            .map(thread_goal_from_row)
            .transpose()
    }

    /// Marks one active goal as paused without clobbering terminal statuses.
    /// 将一个活动目标标记为暂停，同时不覆盖终态。
    pub async fn pause_active_thread_goal(
        &self,
        thread_id: SessionId,
    ) -> Result<Option<ThreadGoal>, String> {
        self.update_active_thread_goal_status(thread_id, ThreadGoalStatus::Paused)
            .await
    }

    /// Marks one active or budget-limited goal as usage-limited.
    /// 将一个 active 或 budget_limited 目标标记为用量受限。
    pub async fn usage_limit_active_thread_goal(
        &self,
        thread_id: SessionId,
    ) -> Result<Option<ThreadGoal>, String> {
        self.update_active_thread_goal_status(thread_id, ThreadGoalStatus::UsageLimited)
            .await
    }

    /// Deletes the current goal for one session and returns the removed row.
    /// 删除单个会话的当前目标并返回被删除的行。
    pub async fn delete_thread_goal(
        &self,
        thread_id: SessionId,
    ) -> Result<Option<ThreadGoal>, String> {
        let rows = self
            .query_goal_rows(
                r#"
DELETE FROM thread_goals
WHERE thread_id = ?1
RETURNING
    thread_id,
    goal_id,
    objective,
    status,
    token_budget,
    tokens_used,
    time_used_seconds,
    created_at_ms,
    updated_at_ms;
"#,
                vec![ControllerSqliteValue::String(thread_id.to_string())],
            )
            .await?;
        rows.into_iter()
            .next()
            .map(thread_goal_from_row)
            .transpose()
    }

    /// Accounts token and time usage for one goal according to Codex status filters.
    /// 按 Codex 状态过滤规则为一个目标记账 token 与时间用量。
    pub async fn account_thread_goal_usage(
        &self,
        thread_id: SessionId,
        tokens_delta: i64,
        time_delta_seconds: i64,
        mode: GoalAccountingMode,
        expected_goal_id: Option<&str>,
    ) -> Result<GoalAccountingOutcome, String> {
        let existing = self.get_thread_goal(thread_id).await?;
        let Some(existing) = existing else {
            return Ok(GoalAccountingOutcome::Unchanged(None));
        };
        if let Some(expected_goal_id) = expected_goal_id
            && existing.goal_id != expected_goal_id
        {
            return Ok(GoalAccountingOutcome::Unchanged(Some(existing)));
        }
        if !status_is_accountable(existing.status, mode) {
            return Ok(GoalAccountingOutcome::Unchanged(Some(existing)));
        }

        let tokens_used = existing.tokens_used.saturating_add(tokens_delta.max(0));
        let time_used_seconds = existing
            .time_used_seconds
            .saturating_add(time_delta_seconds.max(0));
        let status = status_after_budget_limit(existing.status, tokens_used, existing.token_budget);
        let now_ms = current_unix_millis() as i64;
        let rows = self
            .query_goal_rows(
                r#"
UPDATE thread_goals
SET
    tokens_used = ?1,
    time_used_seconds = ?2,
    status = ?3,
    updated_at_ms = ?4
WHERE thread_id = ?5
  AND (?6 IS NULL OR goal_id = ?6)
RETURNING
    thread_id,
    goal_id,
    objective,
    status,
    token_budget,
    tokens_used,
    time_used_seconds,
    created_at_ms,
    updated_at_ms;
"#,
                vec![
                    ControllerSqliteValue::Int64(tokens_used),
                    ControllerSqliteValue::Int64(time_used_seconds),
                    ControllerSqliteValue::String(status.as_str().to_string()),
                    ControllerSqliteValue::Int64(now_ms),
                    ControllerSqliteValue::String(thread_id.to_string()),
                    expected_goal_id
                        .map(|value| ControllerSqliteValue::String(value.to_string()))
                        .unwrap_or(ControllerSqliteValue::Null),
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

    /// Updates active goal status using Codex terminal-status protection.
    /// 使用 Codex 终态保护规则更新活动目标状态。
    async fn update_active_thread_goal_status(
        &self,
        thread_id: SessionId,
        status: ThreadGoalStatus,
    ) -> Result<Option<ThreadGoal>, String> {
        let rows = self
            .query_goal_rows(
                r#"
UPDATE thread_goals
SET
    status = ?1,
    updated_at_ms = ?2
WHERE thread_id = ?3
  AND status IN ('active', 'budget_limited')
RETURNING
    thread_id,
    goal_id,
    objective,
    status,
    token_budget,
    tokens_used,
    time_used_seconds,
    created_at_ms,
    updated_at_ms;
"#,
                vec![
                    ControllerSqliteValue::String(status.as_str().to_string()),
                    ControllerSqliteValue::Int64(current_unix_millis() as i64),
                    ControllerSqliteValue::String(thread_id.to_string()),
                ],
            )
            .await?;
        rows.into_iter()
            .next()
            .map(thread_goal_from_row)
            .transpose()
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
fn status_is_accountable(status: ThreadGoalStatus, mode: GoalAccountingMode) -> bool {
    match mode {
        GoalAccountingMode::ActiveStatusOnly | GoalAccountingMode::ActiveOnly => {
            status == ThreadGoalStatus::Active
        }
        GoalAccountingMode::ActiveOrComplete => {
            matches!(
                status,
                ThreadGoalStatus::Active | ThreadGoalStatus::Complete
            )
        }
        GoalAccountingMode::ActiveOrStopped => matches!(
            status,
            ThreadGoalStatus::Active
                | ThreadGoalStatus::Paused
                | ThreadGoalStatus::Blocked
                | ThreadGoalStatus::UsageLimited
                | ThreadGoalStatus::BudgetLimited
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    /// Verifies accounting modes preserve Codex status filters.
    /// 验证记账模式保留 Codex 的状态过滤规则。
    #[test]
    fn accounting_mode_filters_statuses() {
        assert!(status_is_accountable(
            ThreadGoalStatus::Active,
            GoalAccountingMode::ActiveOnly
        ));
        assert!(!status_is_accountable(
            ThreadGoalStatus::Paused,
            GoalAccountingMode::ActiveOnly
        ));
        assert!(status_is_accountable(
            ThreadGoalStatus::Paused,
            GoalAccountingMode::ActiveOrStopped
        ));
        assert!(status_is_accountable(
            ThreadGoalStatus::Complete,
            GoalAccountingMode::ActiveOrComplete
        ));
    }
}
