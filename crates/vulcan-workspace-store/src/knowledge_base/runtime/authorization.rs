//! Explicit project authorization and inherited-default binding queries.
//! 显式项目授权与继承默认绑定查询。

use super::super::models::KnowledgeProjectBinding;
use super::{KnowledgeBaseRuntime, unix_epoch_millis};
use super::{int, text};
use crate::local_database_runtime::LogicalDatabase;
use crate::local_sql;
use serde::Deserialize;
use vulcan_sqlite_engine::sql_exec::SqliteValue;

/// One project binding row decoded from the application database.
/// 从应用数据库解码的一条项目绑定记录。
#[derive(Debug, Deserialize)]
struct ProjectBindingRow {
    /// Immutable library identifier.
    /// 不可变知识库标识符。
    library_id: String,
    /// Immutable project identifier.
    /// 不可变项目标识符。
    project_id: String,
    /// SQLite integer boolean for default enablement.
    /// 默认启用状态对应的 SQLite 整数布尔值。
    default_enabled: i64,
}

impl KnowledgeBaseRuntime {
    /// Creates, updates, or removes one explicit project authorization binding.
    /// 创建、更新或移除一条显式项目授权绑定。
    ///
    /// # Parameters
    /// 参数
    /// `library_id` is the canonical immutable library UUID.
    /// `library_id` 是规范且不可变的知识库 UUID。
    /// `project_id` is the exact project identity owning this authorization.
    /// `project_id` 是拥有该授权的精确项目身份。
    /// `authorized` is whether to upsert the explicit binding or revoke it.
    /// `authorized` 是是否更新显式绑定，否则撤销绑定。
    /// `default_enabled` is whether an authorized binding is selected by inherited defaults.
    /// `default_enabled` 是已授权绑定是否被继承默认选择。
    ///
    /// # Returns
    /// 返回值
    /// Returns success after the exact binding update or revocation, or a registry error.
    /// 返回精确绑定更新或撤销后的成功，或注册表错误。
    pub async fn set_project_binding(
        &self,
        library_id: &str,
        project_id: &str,
        authorized: bool,
        default_enabled: bool,
    ) -> Result<(), String> {
        let _guard = self.library_mutation_guard(library_id).await?;
        self.require_active_library(library_id).await?;
        let now = unix_epoch_millis()?;
        if authorized {
            local_sql::execute_script_async(
                &self.host_runtime.local,
                LogicalDatabase::AppState,
                r#"INSERT INTO knowledge_library_project_bindings(
                            library_id, project_id, default_enabled, created_at_ms, updated_at_ms
                        ) VALUES (?, ?, ?, ?, ?)
                        ON CONFLICT(library_id, project_id) DO UPDATE SET
                            default_enabled = excluded.default_enabled,
                            updated_at_ms = excluded.updated_at_ms;"#,
                vec![
                    text(library_id),
                    text(project_id),
                    bool_value(default_enabled),
                    int(now),
                    int(now),
                ],
            )
            .await
            .map_err(|error| format!("保存知识库项目授权失败：{error}"))?;
        } else {
            local_sql::execute_script_async(
                &self.host_runtime.local,
                LogicalDatabase::AppState,
                    "DELETE FROM knowledge_library_project_bindings WHERE library_id = ? AND project_id = ?;",
                    vec![text(library_id), text(project_id)],
                ).await
                .map_err(|error| format!("移除知识库项目授权失败：{error}"))?;
        }
        Ok(())
    }

    /// Lists explicit project bindings for one active library.
    /// 列出一个活动知识库的显式项目绑定。
    ///
    /// # Parameters
    /// 参数
    /// `library_id` is the canonical immutable library UUID.
    /// `library_id` 是规范且不可变的知识库 UUID。
    ///
    /// # Returns
    /// 返回值
    /// Returns the exact library bindings ordered by project ID, or an error.
    /// 返回按项目身份排序的精确知识库绑定，或错误。
    pub async fn list_project_bindings(
        &self,
        library_id: &str,
    ) -> Result<Vec<KnowledgeProjectBinding>, String> {
        self.require_active_library(library_id).await?;
        let rows: Vec<ProjectBindingRow> = self
            .query_app_rows(
                "SELECT library_id, project_id, default_enabled FROM knowledge_library_project_bindings WHERE library_id = ? ORDER BY project_id;",
                &[text(library_id)],
            )
            .await?;
        Ok(rows
            .into_iter()
            .map(|row| KnowledgeProjectBinding {
                library_id: row.library_id,
                project_id: row.project_id,
                default_enabled: row.default_enabled != 0,
            })
            .collect())
    }

    /// Lists every active-library authorization row owned by one exact project.
    /// 列出一个精确项目拥有的全部活动知识库授权记录。
    ///
    /// # Parameters
    /// 参数
    /// `project_id` is the exact project identity owning this authorization.
    /// `project_id` 是拥有该授权的精确项目身份。
    ///
    /// # Returns
    /// 返回值
    /// Returns the active-library bindings for the project ordered by library name, or an error.
    /// 返回按库名排序的该项目活动知识库绑定，或错误。
    pub async fn list_bindings_for_project(
        &self,
        project_id: &str,
    ) -> Result<Vec<KnowledgeProjectBinding>, String> {
        let rows: Vec<ProjectBindingRow> = self
            .query_app_rows(
                r#"SELECT b.library_id, b.project_id, b.default_enabled
                   FROM knowledge_library_project_bindings b
                   INNER JOIN knowledge_libraries l ON l.id = b.library_id
                   WHERE b.project_id = ? AND l.lifecycle_state = 'active'
                   ORDER BY l.name_key ASC;"#,
                &[text(project_id)],
            )
            .await?;
        Ok(rows
            .into_iter()
            .map(|row| KnowledgeProjectBinding {
                library_id: row.library_id,
                project_id: row.project_id,
                default_enabled: row.default_enabled != 0,
            })
            .collect())
    }

    /// Resolves the authorized library set for one project, optionally limiting it to defaults.
    /// 解析一个项目的授权知识库集合，并可选择仅保留默认启用项。
    ///
    /// # Parameters
    /// 参数
    /// `project_id` is the exact project identity owning this authorization.
    /// `project_id` 是拥有该授权的精确项目身份。
    /// `defaults_only` is whether to restrict the active authorized set to default-enabled bindings.
    /// `defaults_only` 是是否将活动授权集合限制为默认启用项。
    ///
    /// # Returns
    /// 返回值
    /// Returns the sorted active authorized library IDs for this project and requested default filter, or an error.
    /// 返回该项目及请求默认过滤下已排序的活动授权知识库身份，或错误。
    pub async fn authorized_library_ids(
        &self,
        project_id: &str,
        defaults_only: bool,
    ) -> Result<Vec<String>, String> {
        let predicate = if defaults_only {
            " AND b.default_enabled = 1"
        } else {
            ""
        };
        let sql = format!(
            "SELECT b.library_id, b.project_id, b.default_enabled FROM knowledge_library_project_bindings b INNER JOIN knowledge_libraries l ON l.id = b.library_id WHERE b.project_id = ? AND l.lifecycle_state = 'active'{predicate} ORDER BY b.library_id;"
        );
        let rows: Vec<ProjectBindingRow> = self.query_app_rows(&sql, &[text(project_id)]).await?;
        Ok(rows.into_iter().map(|row| row.library_id).collect())
    }
}

/// Returns a typed SQLite boolean value.
/// 返回类型化 SQLite 布尔值。
///
/// # Parameters
/// 参数
/// `value` is the boolean represented by the engine-typed SQL value.
/// `value` 是待表示为引擎类型化 SQL 值的布尔值。
///
/// # Returns
/// 返回值
/// Returns the typed SQLite boolean preserving the exact input.
/// 返回保留精确输入的类型化 SQLite 布尔值。
fn bool_value(value: bool) -> SqliteValue {
    SqliteValue::Integer(i64::from(value))
}
