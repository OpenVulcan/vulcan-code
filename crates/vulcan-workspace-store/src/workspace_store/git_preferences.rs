//! Persisted Git review monitoring preferences for project repositories.
//! 项目仓库的 Git 审查监听偏好持久化。

use super::*;

/// One persisted repository-monitoring preference row.
/// 一条已持久化的仓库监听偏好行。
#[derive(Debug, Clone, Deserialize)]
struct GitRepositoryMonitoringPreferenceRow {
    /// Stable project identifier owning this preference.
    /// 拥有该偏好的稳定项目标识。
    project_id: String,
    /// Stable project-relative repository key.
    /// 稳定的项目相对仓库键。
    repository_key: String,
    /// SQLite integer representation of the monitoring flag.
    /// 监听标记的 SQLite 整数表示。
    enabled: i64,
}

/// Loads persisted monitoring flags for one project.
/// 加载单个项目已持久化的监听标记。
pub async fn load_git_repository_monitoring_preferences(
    host_runtime: HostRuntimeHandle,
    project_id: &str,
) -> Result<std::collections::BTreeMap<String, bool>, String> {
    let project_id = validate_git_preference_text(project_id, "项目标识")?;
    // The asynchronous twin is the entry every async caller uses, so this call leaves the Tokio core threads.
    // 异步孪生是每个异步调用方使用的入口，因此本次调用会离开 Tokio 核心线程。
    let result = local_sql::query_json_async(
        &host_runtime.local,
        LogicalDatabase::AppState,
        r#"
SELECT project_id, repository_key, enabled
FROM project_git_review_preferences
WHERE project_id = ?1
ORDER BY repository_key ASC;
"#,
        vec![SqliteValue::Text(project_id.to_string())],
    )
    .await
    .map_err(|error| format!("查询 Git 审查监听偏好失败：{error}"))?;
    let rows = decode_query_rows::<GitRepositoryMonitoringPreferenceRow>(&result.json_data)?;
    let mut preferences = std::collections::BTreeMap::new();
    for row in rows {
        if row.project_id != project_id {
            return Err(format!(
                "Git 审查监听偏好返回了错误项目 `{}`。",
                row.project_id
            ));
        }
        preferences.insert(row.repository_key, row.enabled != 0);
    }
    Ok(preferences)
}

/// Persists one repository monitoring flag using an idempotent upsert.
/// 使用幂等 upsert 持久化单个仓库监听标记。
pub async fn persist_git_repository_monitoring_preference(
    host_runtime: HostRuntimeHandle,
    project_id: &str,
    repository_key: &str,
    enabled: bool,
) -> Result<(), String> {
    let project_id = validate_git_preference_text(project_id, "项目标识")?;
    let repository_key = validate_git_preference_text(repository_key, "仓库键")?;
    local_sql::execute_batch_async(
        &host_runtime.local,
        LogicalDatabase::AppState,
        r#"
INSERT INTO project_git_review_preferences (
    project_id,
    repository_key,
    enabled,
    created_at,
    updated_at
) VALUES (?1, ?2, ?3, ?4, ?4)
ON CONFLICT(project_id, repository_key) DO UPDATE SET
    enabled = excluded.enabled,
    updated_at = excluded.updated_at;
"#,
        vec![vec![
            SqliteValue::Text(project_id.to_string()),
            SqliteValue::Text(repository_key.to_string()),
            SqliteValue::Integer(i64::from(enabled)),
            SqliteValue::Text(current_unix_millis_text()),
        ]],
    )
    .await
    .map_err(|error| format!("保存 Git 审查监听偏好失败：{error}"))?;
    Ok(())
}

/// Validates one persisted Git preference text field.
/// 校验一项需要持久化的 Git 偏好文本字段。
fn validate_git_preference_text<'a>(value: &'a str, label: &str) -> Result<&'a str, String> {
    let value = value.trim();
    if value.is_empty() {
        return Err(format!("{label}不能为空。"));
    }
    Ok(value)
}
