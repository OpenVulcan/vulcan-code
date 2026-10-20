//! Project groups: user-named, ordered groups of projects shown in the workbench project tree.
//! 项目分组：在工作台项目树中显示的、由用户命名并排序的项目分组。
//!
//! The groups are one versioned JSON value in `app_metadata`. Membership may name projects that are
//! currently archived; readers project the groups onto the visible projects, and a restored project
//! returns to its group. An unknown format version is rejected rather than reinterpreted.
//! 分组以一个带版本的 JSON 值保存在 `app_metadata` 中。成员可以引用当前已归档的项目；读取方把分组
//! 投影到可见项目上，恢复的项目会回到原分组。未知格式版本会被拒绝，不做重新解释。

use super::*;
use serde::Serialize;

/// Metadata key holding the serialized project groups.
/// 保存序列化项目分组的元数据键。
const PROJECT_GROUPS_METADATA_KEY: &str = "workspace.project_groups";

/// Current serialized project-group format version.
/// 当前项目分组序列化格式版本。
pub const PROJECT_GROUPS_FORMAT_VERSION: u32 = 1;

/// Longest accepted group name, counted in characters after trimming.
/// 允许的最长分组名称，按去除首尾空白后的字符数计算。
pub const PROJECT_GROUP_NAME_MAX_CHARS: usize = 80;

/// One user-named group and its projects in display order.
/// 一个由用户命名的分组及其按显示顺序排列的项目。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectGroup {
    /// Stable group identifier.
    /// 稳定的分组标识。
    pub id: String,
    /// User-facing group name.
    /// 面向用户的分组名称。
    pub name: String,
    /// Member project identifiers in display order.
    /// 按显示顺序排列的成员项目标识。
    pub project_ids: Vec<String>,
}

/// Versioned envelope written to `app_metadata`.
/// 写入 `app_metadata` 的带版本封装。
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredProjectGroups {
    version: u32,
    groups: Vec<ProjectGroup>,
}

/// Trims a requested group name and checks that it is non-empty and within the length limit.
/// 去除请求分组名称的首尾空白，并检查其非空且未超出长度上限。
///
/// # Parameters
/// 参数
/// `name` is the name as entered by the user.
/// `name` 是用户输入的名称。
///
/// # Returns
/// 返回值
/// Returns the trimmed name, or an error naming the violated rule.
/// 返回去除空白后的名称，或说明违反规则的错误。
pub fn normalize_project_group_name(name: &str) -> Result<String, String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(String::from("Project group name must not be empty"));
    }
    if trimmed.chars().count() > PROJECT_GROUP_NAME_MAX_CHARS {
        return Err(format!(
            "Project group name must be at most {PROJECT_GROUP_NAME_MAX_CHARS} characters"
        ));
    }
    Ok(trimmed.to_string())
}

/// Checks that group ids are unique and non-empty, names are valid, and no project is in two groups.
/// 检查分组标识唯一且非空、名称有效，且没有项目同时属于两个分组。
///
/// # Parameters
/// 参数
/// `groups` is the complete group list about to be stored or that was just read.
/// `groups` 是即将保存或刚刚读取的完整分组列表。
///
/// # Returns
/// 返回值
/// Returns `Ok(())` when the list is consistent, otherwise the first violation.
/// 列表一致时返回 `Ok(())`，否则返回第一个违规项。
pub fn validate_project_groups(groups: &[ProjectGroup]) -> Result<(), String> {
    let mut group_ids = BTreeSet::new();
    let mut project_ids = BTreeSet::new();
    for group in groups {
        if group.id.trim().is_empty() {
            return Err(String::from("Project group id must not be empty"));
        }
        if !group_ids.insert(group.id.as_str()) {
            return Err(format!("Duplicate project group id: {}", group.id));
        }
        if normalize_project_group_name(&group.name)? != group.name {
            return Err(format!("Project group name is not normalized: {}", group.id));
        }
        for project_id in &group.project_ids {
            if !project_ids.insert(project_id.as_str()) {
                return Err(format!("Project {project_id} belongs to more than one group"));
            }
        }
    }
    Ok(())
}

/// Serializes the complete group list into the versioned metadata value.
/// 将完整分组列表序列化为带版本的元数据值。
fn encode_project_groups(groups: &[ProjectGroup]) -> Result<String, String> {
    validate_project_groups(groups)?;
    serde_json::to_string(&StoredProjectGroups {
        version: PROJECT_GROUPS_FORMAT_VERSION,
        groups: groups.to_vec(),
    })
    .map_err(|error| format!("Failed to serialize the project groups: {error}"))
}

/// Parses the versioned metadata value, rejecting other versions and inconsistent content.
/// 解析带版本的元数据值，拒绝其他版本与不一致的内容。
fn decode_project_groups(value: &str) -> Result<Vec<ProjectGroup>, String> {
    let stored = serde_json::from_str::<StoredProjectGroups>(value)
        .map_err(|error| format!("Failed to parse the project groups: {error}"))?;
    if stored.version != PROJECT_GROUPS_FORMAT_VERSION {
        return Err(format!(
            "Unsupported project group format version {} (expected {PROJECT_GROUPS_FORMAT_VERSION})",
            stored.version
        ));
    }
    validate_project_groups(&stored.groups)?;
    Ok(stored.groups)
}

/// Loads the stored project groups; a workspace that never saved groups has none.
/// 读取已保存的项目分组；从未保存过分组的工作区没有分组。
///
/// # Parameters
/// 参数
/// `host_runtime` owns the application database.
/// `host_runtime` 持有应用数据库。
///
/// # Returns
/// 返回值
/// Returns the groups in display order, or an error when the stored value cannot be read.
/// 返回按显示顺序排列的分组；无法读取已保存值时返回错误。
pub fn load_project_groups(host_runtime: &HostRuntimeHandle) -> Result<Vec<ProjectGroup>, String> {
    let result = local_sql::query_json(
        host_runtime.local.as_ref(),
        LogicalDatabase::AppState,
        "SELECT value FROM app_metadata WHERE key = ?1;",
        &[SqliteValue::Text(PROJECT_GROUPS_METADATA_KEY.to_string())],
    )
    .map_err(|error| format!("Failed to query the project groups: {error}"))?;
    let rows = decode_query_rows::<StoredAppMetadataRow>(&result.json_data)?;
    match rows.first() {
        Some(row) => decode_project_groups(&row.value),
        None => Ok(Vec::new()),
    }
}

/// Replaces the stored project groups with one complete, validated list.
/// 用一份经过校验的完整列表替换已保存的项目分组。
///
/// # Parameters
/// 参数
/// `host_runtime` owns the application database; `groups` is the complete list in display order.
/// `host_runtime` 持有应用数据库；`groups` 是按显示顺序排列的完整列表。
///
/// # Returns
/// 返回值
/// Returns `Ok(())` after the value is written.
/// 值写入后返回 `Ok(())`。
pub async fn persist_project_groups(
    host_runtime: HostRuntimeHandle,
    groups: &[ProjectGroup],
) -> Result<(), String> {
    let value = encode_project_groups(groups)?;
    local_sql::execute_batch_async(
        &host_runtime.local,
        LogicalDatabase::AppState,
        r#"
INSERT INTO app_metadata (key, value)
VALUES (?1, ?2)
ON CONFLICT(key) DO UPDATE SET value = excluded.value;
"#,
        vec![vec![
            SqliteValue::Text(PROJECT_GROUPS_METADATA_KEY.to_string()),
            SqliteValue::Text(value),
        ]],
    )
    .await
    .map_err(|error| format!("Failed to write the project groups: {error}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds one group with borrowed member ids.
    /// 使用借用的成员标识构建一个分组。
    fn group(id: &str, name: &str, project_ids: &[&str]) -> ProjectGroup {
        ProjectGroup {
            id: id.to_string(),
            name: name.to_string(),
            project_ids: project_ids.iter().map(|id| (*id).to_string()).collect(),
        }
    }

    #[test]
    fn stored_groups_round_trip_with_their_order() {
        let groups = vec![group("g2", "Second", &["p3"]), group("g1", "First", &["p2", "p1"])];
        let value = encode_project_groups(&groups).expect("encode");
        assert_eq!(decode_project_groups(&value).expect("decode"), groups);
    }

    #[test]
    fn other_format_versions_are_rejected() {
        let value = r#"{"version":2,"groups":[]}"#;
        assert!(decode_project_groups(value).is_err(), "version 2 must be rejected");
    }

    #[test]
    fn a_project_cannot_belong_to_two_groups() {
        let groups = vec![group("g1", "A", &["p1"]), group("g2", "B", &["p1"])];
        assert!(validate_project_groups(&groups).is_err());
        assert!(encode_project_groups(&groups).is_err());
    }

    #[test]
    fn names_are_trimmed_and_bounded() {
        assert_eq!(normalize_project_group_name("  Work  ").expect("trim"), "Work");
        assert!(normalize_project_group_name("   ").is_err());
        let long = "x".repeat(PROJECT_GROUP_NAME_MAX_CHARS + 1);
        assert!(normalize_project_group_name(&long).is_err());
        assert!(validate_project_groups(&[group("g1", " padded ", &[])]).is_err());
    }
}
