//! Project tree: user-named, ordered groups of projects, the position of the default group, and the pinned
//! section shown at the top of the workbench project tree.
//! 项目树：由用户命名并排序的项目分组、默认分组的位置，以及显示在工作台项目树顶部的置顶分区。
//!
//! The tree is one versioned JSON value in `app_metadata`. Every project outside the user groups belongs to the
//! default group, which has no stored member list, cannot be renamed or deleted, and moves among the user groups
//! by its index. Group membership may name projects that are currently archived; readers project the tree onto
//! the visible projects, and a restored project returns to its group. Pins keep their item's membership; they only
//! show the item in the pinned section, in the stored order. Project pins are owned by the project row's pinned
//! flag, so the pin list only orders them. An unknown format version is rejected rather than reinterpreted.
//! 项目树以一个带版本的 JSON 值保存在 `app_metadata` 中。不属于任何用户分组的项目都归入默认分组；默认分组
//! 没有保存的成员列表，不能重命名或删除，并通过索引在用户分组之间移动。分组成员可以引用当前已归档的项目；
//! 读取方把项目树投影到可见项目上，恢复的项目会回到原分组。置顶不改变条目的归属，只让条目按保存顺序显示在
//! 置顶分区中。项目的置顶状态由项目行的置顶标记持有，因此置顶列表只为其排序。未知格式版本会被拒绝，不做重新解释。

use super::*;
use serde::Serialize;

/// Metadata key holding the serialized project tree.
/// 保存序列化项目树的元数据键。
const PROJECT_GROUPS_METADATA_KEY: &str = "workspace.project_groups";

/// Current serialized project-tree format version.
/// 当前项目树序列化格式版本。
pub const PROJECT_GROUPS_FORMAT_VERSION: u32 = 1;

/// Longest accepted group name, counted in characters after trimming.
/// 允许的最长分组名称，按去除首尾空白后的字符数计算。
pub const PROJECT_GROUP_NAME_MAX_CHARS: usize = 80;

/// Reserved identifier of the default group that holds every project outside the user groups.
/// 默认分组的保留标识；该分组容纳不属于任何用户分组的项目。
pub const DEFAULT_PROJECT_GROUP_ID: &str = "default";

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

/// Kind of item shown in the pinned section.
/// 显示在置顶分区中的条目种类。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectTreePinKind {
    /// A session (conversation).
    /// 会话。
    Session,
    /// A project.
    /// 项目。
    Project,
    /// A user group or the default group.
    /// 用户分组或默认分组。
    Group,
}

/// One item of the pinned section.
/// 置顶分区中的一个条目。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectTreePin {
    /// Kind of the pinned item.
    /// 置顶条目的种类。
    pub kind: ProjectTreePinKind,
    /// Identifier of the pinned item within its kind.
    /// 置顶条目在其种类内的标识。
    pub id: String,
}

/// Complete stored project tree.
/// 完整的已保存项目树。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ProjectTree {
    /// User groups in display order, without the default group.
    /// 按显示顺序排列的用户分组，不含默认分组。
    pub groups: Vec<ProjectGroup>,
    /// Position of the default group among the user groups, from 0 (first) to `groups.len()` (last).
    /// 默认分组在用户分组之间的位置，从 0（最前）到 `groups.len()`（最后）。
    pub default_group_index: usize,
    /// Pinned section in display order.
    /// 按显示顺序排列的置顶分区。
    pub pins: Vec<ProjectTreePin>,
}

/// One slot of the combined group order: a user group, or `None` for the default group.
/// 合并分组顺序中的一个位置：用户分组，或以 `None` 表示的默认分组。
pub type ProjectGroupSlot = Option<ProjectGroup>;

impl ProjectTree {
    /// Returns every group in display order, the default group as `None`.
    /// 按显示顺序返回所有分组，默认分组为 `None`。
    pub fn into_slots(self) -> Vec<ProjectGroupSlot> {
        let mut slots = self.groups.into_iter().map(Some).collect::<Vec<_>>();
        slots.insert(self.default_group_index.min(slots.len()), None);
        slots
    }

    /// Rebuilds the groups and the default position from a combined order that holds exactly one `None`.
    /// 由恰好包含一个 `None` 的合并顺序重建分组与默认位置。
    ///
    /// # Parameters
    /// 参数
    /// `slots` is the combined group order; `pins` is kept as the pinned section.
    /// `slots` 是合并分组顺序；`pins` 作为置顶分区保留。
    ///
    /// # Returns
    /// 返回值
    /// Returns the tree, or an error when the default group is missing or repeated.
    /// 返回项目树；默认分组缺失或重复时返回错误。
    pub fn from_slots(
        slots: Vec<ProjectGroupSlot>,
        pins: Vec<ProjectTreePin>,
    ) -> Result<Self, String> {
        let defaults = slots.iter().filter(|slot| slot.is_none()).count();
        if defaults != 1 {
            return Err(String::from(
                "The project group order must contain the default group exactly once",
            ));
        }
        let default_group_index = slots
            .iter()
            .position(Option::is_none)
            .expect("one default slot was counted");
        Ok(Self {
            groups: slots.into_iter().flatten().collect(),
            default_group_index,
            pins,
        })
    }

    /// Group identifiers in display order, the default group included under `DEFAULT_PROJECT_GROUP_ID`.
    /// 按显示顺序排列的分组标识，默认分组以 `DEFAULT_PROJECT_GROUP_ID` 计入。
    pub fn ordered_group_ids(&self) -> Vec<&str> {
        let mut ids = self
            .groups
            .iter()
            .map(|group| group.id.as_str())
            .collect::<Vec<_>>();
        ids.insert(
            self.default_group_index.min(ids.len()),
            DEFAULT_PROJECT_GROUP_ID,
        );
        ids
    }

    /// Whether one identifier names a user group or the default group.
    /// 某标识是否指向用户分组或默认分组。
    pub fn has_group(&self, group_id: &str) -> bool {
        group_id == DEFAULT_PROJECT_GROUP_ID || self.groups.iter().any(|group| group.id == group_id)
    }

    /// Whether one item is in the pinned section.
    /// 某条目是否位于置顶分区。
    pub fn is_pinned(&self, kind: ProjectTreePinKind, id: &str) -> bool {
        self.pins.iter().any(|pin| pin.kind == kind && pin.id == id)
    }

    /// Pins one item at the end of the pinned section, or removes its pin.
    /// 将一个条目置顶到置顶分区末尾，或移除其置顶。
    pub fn set_pinned(&mut self, kind: ProjectTreePinKind, id: &str, pinned: bool) {
        self.pins.retain(|pin| !(pin.kind == kind && pin.id == id));
        if pinned {
            self.pins.push(ProjectTreePin {
                kind,
                id: id.to_string(),
            });
        }
    }

    /// Moves one project into a group, out of every other group; the default group stores no members.
    /// 将一个项目移入某分组并移出其他所有分组；默认分组不保存成员。
    pub fn place_project(&mut self, project_id: &str, group_id: &str) -> Result<(), String> {
        if !self.has_group(group_id) {
            return Err(format!("Project group {group_id} does not exist"));
        }
        for group in &mut self.groups {
            group.project_ids.retain(|member| member != project_id);
            if group.id == group_id {
                group.project_ids.push(project_id.to_string());
            }
        }
        Ok(())
    }
}

/// Versioned envelope written to `app_metadata`. The default position and the pins were added within version 1,
/// so a value written before them reads as "default group last, nothing pinned".
/// 写入 `app_metadata` 的带版本封装。默认位置与置顶在版本 1 内加入，因此早于它们写入的值读作“默认分组在最后、
/// 没有置顶”。
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredProjectTree {
    version: u32,
    groups: Vec<ProjectGroup>,
    #[serde(default)]
    default_group_index: Option<usize>,
    #[serde(default)]
    pins: Vec<ProjectTreePin>,
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

/// Checks that group ids are unique, non-empty and not reserved, names are valid, no project is in two groups,
/// the default position is in range, and every pin is unique and names an existing group when it pins one.
/// 检查分组标识唯一、非空且未使用保留值，名称有效，没有项目同时属于两个分组，默认位置在范围内，且每个置顶唯一、
/// 置顶分组时指向已存在的分组。
///
/// # Parameters
/// 参数
/// `tree` is the complete tree about to be stored or that was just read.
/// `tree` 是即将保存或刚刚读取的完整项目树。
///
/// # Returns
/// 返回值
/// Returns `Ok(())` when the tree is consistent, otherwise the first violation.
/// 项目树一致时返回 `Ok(())`，否则返回第一个违规项。
pub fn validate_project_tree(tree: &ProjectTree) -> Result<(), String> {
    let mut group_ids = BTreeSet::new();
    let mut project_ids = BTreeSet::new();
    for group in &tree.groups {
        if group.id.trim().is_empty() {
            return Err(String::from("Project group id must not be empty"));
        }
        if group.id == DEFAULT_PROJECT_GROUP_ID {
            return Err(format!(
                "Project group id {DEFAULT_PROJECT_GROUP_ID} is reserved"
            ));
        }
        if !group_ids.insert(group.id.as_str()) {
            return Err(format!("Duplicate project group id: {}", group.id));
        }
        if normalize_project_group_name(&group.name)? != group.name {
            return Err(format!(
                "Project group name is not normalized: {}",
                group.id
            ));
        }
        for project_id in &group.project_ids {
            if !project_ids.insert(project_id.as_str()) {
                return Err(format!(
                    "Project {project_id} belongs to more than one group"
                ));
            }
        }
    }
    if tree.default_group_index > tree.groups.len() {
        return Err(format!(
            "Default project group position {} is out of range",
            tree.default_group_index
        ));
    }
    let mut pins = BTreeSet::new();
    for pin in &tree.pins {
        if pin.id.trim().is_empty() {
            return Err(String::from("Pinned item id must not be empty"));
        }
        if !pins.insert((pin.kind, pin.id.as_str())) {
            return Err(format!("Item {} is pinned more than once", pin.id));
        }
        if pin.kind == ProjectTreePinKind::Group && !tree.has_group(&pin.id) {
            return Err(format!("Pinned project group {} does not exist", pin.id));
        }
    }
    Ok(())
}

/// Serializes the complete tree into the versioned metadata value.
/// 将完整项目树序列化为带版本的元数据值。
fn encode_project_tree(tree: &ProjectTree) -> Result<String, String> {
    validate_project_tree(tree)?;
    serde_json::to_string(&StoredProjectTree {
        version: PROJECT_GROUPS_FORMAT_VERSION,
        groups: tree.groups.clone(),
        default_group_index: Some(tree.default_group_index),
        pins: tree.pins.clone(),
    })
    .map_err(|error| format!("Failed to serialize the project tree: {error}"))
}

/// Parses the versioned metadata value, rejecting other versions and inconsistent content.
/// 解析带版本的元数据值，拒绝其他版本与不一致的内容。
fn decode_project_tree(value: &str) -> Result<ProjectTree, String> {
    let stored = serde_json::from_str::<StoredProjectTree>(value)
        .map_err(|error| format!("Failed to parse the project tree: {error}"))?;
    if stored.version != PROJECT_GROUPS_FORMAT_VERSION {
        return Err(format!(
            "Unsupported project tree format version {} (expected {PROJECT_GROUPS_FORMAT_VERSION})",
            stored.version
        ));
    }
    let tree = ProjectTree {
        default_group_index: stored.default_group_index.unwrap_or(stored.groups.len()),
        groups: stored.groups,
        pins: stored.pins,
    };
    validate_project_tree(&tree)?;
    Ok(tree)
}

/// Loads the stored project tree; a workspace that never saved one has no groups and no pins.
/// 读取已保存的项目树；从未保存过的工作区没有分组也没有置顶。
///
/// # Parameters
/// 参数
/// `host_runtime` owns the application database.
/// `host_runtime` 持有应用数据库。
///
/// # Returns
/// 返回值
/// Returns the tree, or an error when the stored value cannot be read.
/// 返回项目树；无法读取已保存值时返回错误。
pub fn load_project_tree(host_runtime: &HostRuntimeHandle) -> Result<ProjectTree, String> {
    let result = local_sql::query_json(
        host_runtime.local.as_ref(),
        LogicalDatabase::AppState,
        "SELECT value FROM app_metadata WHERE key = ?1;",
        &[SqliteValue::Text(PROJECT_GROUPS_METADATA_KEY.to_string())],
    )
    .map_err(|error| format!("Failed to query the project tree: {error}"))?;
    let rows = decode_query_rows::<StoredAppMetadataRow>(&result.json_data)?;
    match rows.first() {
        Some(row) => decode_project_tree(&row.value),
        None => Ok(ProjectTree::default()),
    }
}

/// Replaces the stored project tree with one complete, validated tree.
/// 用一棵经过校验的完整项目树替换已保存的项目树。
///
/// # Parameters
/// 参数
/// `host_runtime` owns the application database; `tree` is the complete tree.
/// `host_runtime` 持有应用数据库；`tree` 是完整项目树。
///
/// # Returns
/// 返回值
/// Returns `Ok(())` after the value is written.
/// 值写入后返回 `Ok(())`。
pub async fn persist_project_tree(
    host_runtime: HostRuntimeHandle,
    tree: &ProjectTree,
) -> Result<(), String> {
    let value = encode_project_tree(tree)?;
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
    .map_err(|error| format!("Failed to write the project tree: {error}"))?;
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

    /// Builds one pin.
    /// 构建一个置顶条目。
    fn pin(kind: ProjectTreePinKind, id: &str) -> ProjectTreePin {
        ProjectTreePin {
            kind,
            id: id.to_string(),
        }
    }

    /// Builds a tree from groups, the default position and pins.
    /// 由分组、默认位置与置顶构建项目树。
    fn tree(
        groups: Vec<ProjectGroup>,
        default_group_index: usize,
        pins: Vec<ProjectTreePin>,
    ) -> ProjectTree {
        ProjectTree {
            groups,
            default_group_index,
            pins,
        }
    }

    #[test]
    fn stored_tree_round_trips_with_its_order() {
        let value = tree(
            vec![
                group("g2", "Second", &["p3"]),
                group("g1", "First", &["p2", "p1"]),
            ],
            1,
            vec![
                pin(ProjectTreePinKind::Session, "s1"),
                pin(ProjectTreePinKind::Group, "g1"),
            ],
        );
        let encoded = encode_project_tree(&value).expect("encode");
        assert_eq!(decode_project_tree(&encoded).expect("decode"), value);
    }

    #[test]
    fn a_value_without_position_or_pins_puts_the_default_group_last() {
        let decoded = decode_project_tree(
            r#"{"version":1,"groups":[{"id":"g1","name":"A","project_ids":[]}]}"#,
        )
        .expect("decode");
        assert_eq!(decoded, tree(vec![group("g1", "A", &[])], 1, Vec::new()));
        assert_eq!(
            decoded.ordered_group_ids(),
            vec!["g1", DEFAULT_PROJECT_GROUP_ID]
        );
    }

    #[test]
    fn other_format_versions_are_rejected() {
        let value = r#"{"version":2,"groups":[]}"#;
        assert!(
            decode_project_tree(value).is_err(),
            "version 2 must be rejected"
        );
    }

    #[test]
    fn inconsistent_trees_are_rejected() {
        let two_groups = tree(
            vec![group("g1", "A", &["p1"]), group("g2", "B", &["p1"])],
            0,
            Vec::new(),
        );
        assert!(
            encode_project_tree(&two_groups).is_err(),
            "a project in two groups"
        );
        let reserved = tree(
            vec![group(DEFAULT_PROJECT_GROUP_ID, "A", &[])],
            0,
            Vec::new(),
        );
        assert!(
            validate_project_tree(&reserved).is_err(),
            "reserved group id"
        );
        assert!(
            validate_project_tree(&tree(Vec::new(), 1, Vec::new())).is_err(),
            "position out of range"
        );
        let twice = vec![
            pin(ProjectTreePinKind::Session, "s"),
            pin(ProjectTreePinKind::Session, "s"),
        ];
        assert!(
            validate_project_tree(&tree(Vec::new(), 0, twice)).is_err(),
            "duplicate pin"
        );
        let missing = vec![pin(ProjectTreePinKind::Group, "gone")];
        assert!(
            validate_project_tree(&tree(Vec::new(), 0, missing)).is_err(),
            "pin of a missing group"
        );
        let default_pin = vec![pin(ProjectTreePinKind::Group, DEFAULT_PROJECT_GROUP_ID)];
        assert!(
            validate_project_tree(&tree(Vec::new(), 0, default_pin)).is_ok(),
            "the default group can be pinned"
        );
    }

    #[test]
    fn slots_keep_the_default_position() {
        let value = tree(
            vec![group("g1", "A", &[]), group("g2", "B", &[])],
            1,
            Vec::new(),
        );
        let mut slots = value.clone().into_slots();
        assert_eq!(
            slots.iter().map(Option::is_none).collect::<Vec<_>>(),
            vec![false, true, false]
        );
        slots.rotate_left(1);
        let moved = ProjectTree::from_slots(slots, Vec::new()).expect("one default slot");
        assert_eq!(
            moved.ordered_group_ids(),
            vec![DEFAULT_PROJECT_GROUP_ID, "g2", "g1"]
        );
        assert!(ProjectTree::from_slots(vec![Some(group("g1", "A", &[]))], Vec::new()).is_err());
    }

    #[test]
    fn placing_a_project_moves_it_between_groups() {
        let mut value = tree(
            vec![group("g1", "A", &["p1"]), group("g2", "B", &[])],
            2,
            Vec::new(),
        );
        value.place_project("p1", "g2").expect("g2 exists");
        assert_eq!(value.groups[0].project_ids, Vec::<String>::new());
        assert_eq!(value.groups[1].project_ids, vec!["p1"]);
        value
            .place_project("p1", DEFAULT_PROJECT_GROUP_ID)
            .expect("default exists");
        assert!(
            value
                .groups
                .iter()
                .all(|group| group.project_ids.is_empty())
        );
        assert!(value.place_project("p1", "gone").is_err());
    }

    #[test]
    fn names_are_trimmed_and_bounded() {
        assert_eq!(
            normalize_project_group_name("  Work  ").expect("trim"),
            "Work"
        );
        assert!(normalize_project_group_name("   ").is_err());
        let long = "x".repeat(PROJECT_GROUP_NAME_MAX_CHARS + 1);
        assert!(normalize_project_group_name(&long).is_err());
        assert!(
            validate_project_tree(&tree(vec![group("g1", " padded ", &[])], 0, Vec::new()))
                .is_err()
        );
    }
}
