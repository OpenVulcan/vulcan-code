//! Device-local folder associations owned by registered projects.
//! 已注册项目持有的本机目录关联。

use super::*;

/// One selected folder path returned by SQLite.
/// SQLite 返回的一条目录路径。
#[derive(Deserialize)]
struct AttachedFolderRow {
    /// Canonical native display path.
    /// 规范化的本机显示路径。
    path: String,
}

/// Loads canonical attached paths for the given project in deterministic order.
/// 按确定顺序加载指定项目的规范化附属路径。
/// `runtime` owns the application database and `project_id` scopes the returned paths.
/// `runtime` 持有应用数据库，`project_id` 限定返回路径的范围。
pub async fn load_project_attached_folders(
    runtime: HostRuntimeHandle,
    project_id: &str,
) -> Result<Vec<String>, String> {
    // Query only the associations of the requested project.
    // 只查询指定项目的关联。
    let result = local_sql::query_json_async(
        &runtime.local,
        LogicalDatabase::AppState,
        "SELECT path FROM project_attached_folders WHERE project_id = ?1 ORDER BY path_key;",
        vec![SqliteValue::Text(project_id.to_owned())],
    )
    .await
    .map_err(|error| format!("Failed to load project attached folders: {error}"))?;
    Ok(decode_query_rows::<AttachedFolderRow>(&result.json_data)?
        .into_iter()
        .map(|row| row.path)
        .collect())
}

/// Adds or removes a folder association without touching the filesystem.
/// 添加或移除目录关联，操作范围仅限数据库。
/// `runtime` owns the database, `project_id` owns the association, and `path` is the native path.
/// `runtime` 持有数据库，`project_id` 持有关联，`path` 为本机路径。
/// Returns success after the idempotent update specified by `attached`.
/// 在完成 `attached` 指定的幂等更新后返回成功。
pub async fn set_project_attached_folder(
    runtime: HostRuntimeHandle,
    project_id: &str,
    path: &str,
    attached: bool,
) -> Result<(), String> {
    // Use a case-insensitive identity on Windows, preserving the canonical display path.
    // Windows 使用不区分大小写的标识，同时保留规范化显示路径。
    let path_key = if cfg!(windows) {
        path.to_lowercase()
    } else {
        path.to_owned()
    };
    // Each mutation updates one association so concurrent edits to other paths are retained.
    // 每次变更只更新一条关联，以保留其他路径的并发编辑。
    let (sql, params) = if attached {
        (
            "INSERT INTO project_attached_folders (project_id, path_key, path) VALUES (?1, ?2, ?3) ON CONFLICT(project_id, path_key) DO NOTHING;",
            vec![
                SqliteValue::Text(project_id.to_owned()),
                SqliteValue::Text(path_key),
                SqliteValue::Text(path.to_owned()),
            ],
        )
    } else {
        (
            "DELETE FROM project_attached_folders WHERE project_id = ?1 AND path_key = ?2;",
            vec![
                SqliteValue::Text(project_id.to_owned()),
                SqliteValue::Text(path_key),
            ],
        )
    };
    local_sql::execute_batch_async(&runtime.local, LogicalDatabase::AppState, sql, vec![params])
        .await
        .map_err(|error| format!("Failed to update the project attached folder: {error}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data_plane::{HostRuntimeMetadata, build_application_schema_script};

    /// Verifies reopens, duplicate registrations and removal isolation with a real application database.
    /// 使用真实应用数据库验证重新打开、重复登记和移除隔离。
    #[tokio::test]
    async fn persists_project_attached_folders_across_reopens() {
        // Separate database bindings keep this fixture isolated from user data.
        // 独立数据库绑定使夹具与用户数据隔离。
        let directory = tempfile::tempdir().expect("temporary application data");
        let metadata = HostRuntimeMetadata {
            data_root: directory.path().to_path_buf(),
            sqlite_path: directory.path().join("app.sqlite3"),
            sqlite_binding_id: "attached-folders-app".into(),
            system_statistics_sqlite_path: directory.path().join("statistics.sqlite3"),
            system_statistics_sqlite_binding_id: "attached-folders-statistics".into(),
        };
        let runtime = HostRuntimeHandle::new_for_test(metadata.clone());
        local_sql::execute_script(
            &runtime.local,
            LogicalDatabase::AppState,
            build_application_schema_script(),
            &[],
        )
        .expect("initialize the application schema");
        local_sql::execute_script(
            &runtime.local,
            LogicalDatabase::AppState,
            String::from("INSERT INTO projects (id, name, root_path, created_at, updated_at) VALUES ('one', 'One', '/one', '1', '1'), ('two', 'Two', '/two', '1', '1');"),
            &[],
        ).expect("seed two project owners");
        // The same folder can be registered independently to more than one project.
        // 同一目录可以分别登记到多个项目。
        let path = directory.path().to_str().expect("native directory text");
        for project_id in ["one", "one", "two"] {
            set_project_attached_folder(runtime.clone(), project_id, path, true)
                .await
                .expect("register folder");
        }
        drop(runtime);

        let reopened = HostRuntimeHandle::new_for_test(metadata);
        assert_eq!(
            load_project_attached_folders(reopened.clone(), "one")
                .await
                .expect("load first project"),
            [path]
        );
        set_project_attached_folder(reopened.clone(), "one", path, false)
            .await
            .expect("remove first association");
        assert!(
            load_project_attached_folders(reopened.clone(), "one")
                .await
                .expect("reload first project")
                .is_empty()
        );
        assert_eq!(
            load_project_attached_folders(reopened, "two")
                .await
                .expect("load second project"),
            [path]
        );
        assert!(
            directory.path().is_dir(),
            "removal cannot delete the attached directory"
        );
    }
}
