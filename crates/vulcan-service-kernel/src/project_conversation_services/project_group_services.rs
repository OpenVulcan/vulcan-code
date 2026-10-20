//! Project group service handlers: create, rename, dissolve, and lay out user-named project groups.
//! 项目分组服务处理器：创建、重命名、解散用户命名的项目分组并调整其布局。
//!
//! Every handler reads the stored groups, applies one validated change, writes the complete list back,
//! and returns the refreshed workbench snapshot, which is also broadcast so every open window follows. Pinned and archived members are kept in storage but
//! are not part of a layout request, so pinning or archiving a project never loses its group.
//! 每个处理器读取已保存分组、应用一次经过校验的变更、写回完整列表，并返回刷新后的工作台快照；该快照同时
//! 广播，使所有打开的窗口一同更新。
//! 置顶与归档成员保留在存储中但不属于布局请求，因此置顶或归档项目不会丢失其分组。

use super::project_services::{
    RetainedProjectLayoutEntry, controller_runtime_and_project_layout_from_state,
};
use super::*;
use vulcan_service_api::ProjectGroupLayoutEntry;
use vulcan_workspace_store::workspace_store::{
    ProjectGroup, load_project_groups, normalize_project_group_name, persist_project_groups,
};

/// Creates a group at the end of the group list and moves the given visible projects into it.
/// 在分组列表末尾创建分组，并把给定的可见项目移入其中。
///
/// # Parameters
/// 参数
/// `name` is the requested group name; `project_ids` are visible projects to move into the new group.
/// `name` 是请求的分组名称；`project_ids` 是要移入新分组的可见项目。
///
/// # Returns
/// 返回值
/// Returns the refreshed workbench snapshot.
/// 返回刷新后的工作台快照。
pub(crate) async fn service_create_project_group(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    name: String,
    project_ids: Vec<String>,
) -> Result<WorkbenchSnapshot, String> {
    let name = normalize_project_group_name(&name)?;
    let (controller_runtime, layout) = controller_runtime_and_project_layout_from_state(&state)?;
    let visible = layout
        .iter()
        .map(|project| project.id.as_str())
        .collect::<BTreeSet<_>>();
    if let Some(unknown) = project_ids
        .iter()
        .find(|project_id| !visible.contains(project_id.as_str()))
    {
        return Err(format!(
            "Failed to create the project group: project {unknown} is not visible."
        ));
    }
    let mut groups = load_project_groups(&controller_runtime)?;
    for group in &mut groups {
        group
            .project_ids
            .retain(|project_id| !project_ids.contains(project_id));
    }
    groups.push(ProjectGroup {
        id: Uuid::new_v4().to_string(),
        name,
        project_ids,
    });
    persist_project_groups(controller_runtime, &groups).await?;
    refreshed_and_broadcast(&app, state).await
}

/// Renames one existing group.
/// 重命名一个已存在的分组。
///
/// # Parameters
/// 参数
/// `group_id` names the group; `name` is the requested new name.
/// `group_id` 指定分组；`name` 是请求的新名称。
///
/// # Returns
/// 返回值
/// Returns the refreshed workbench snapshot, or an error when the group does not exist.
/// 返回刷新后的工作台快照；分组不存在时返回错误。
pub(crate) async fn service_rename_project_group(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    group_id: String,
    name: String,
) -> Result<WorkbenchSnapshot, String> {
    let name = normalize_project_group_name(&name)?;
    let controller_runtime = controller_runtime_from_state(&state)?;
    let mut groups = load_project_groups(&controller_runtime)?;
    let group = groups
        .iter_mut()
        .find(|group| group.id == group_id)
        .ok_or_else(|| format!("Failed to rename the project group: {group_id} does not exist."))?;
    group.name = name;
    persist_project_groups(controller_runtime, &groups).await?;
    refreshed_and_broadcast(&app, state).await
}

/// Dissolves one group; its projects become ungrouped and are not otherwise changed.
/// 解散一个分组；其项目变为未分组，其他方面不变。
///
/// # Parameters
/// 参数
/// `group_id` names the group to dissolve.
/// `group_id` 指定要解散的分组。
///
/// # Returns
/// 返回值
/// Returns the refreshed workbench snapshot, or an error when the group does not exist.
/// 返回刷新后的工作台快照；分组不存在时返回错误。
pub(crate) async fn service_delete_project_group(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    group_id: String,
) -> Result<WorkbenchSnapshot, String> {
    let controller_runtime = controller_runtime_from_state(&state)?;
    let mut groups = load_project_groups(&controller_runtime)?;
    let before = groups.len();
    groups.retain(|group| group.id != group_id);
    if groups.len() == before {
        return Err(format!(
            "Failed to delete the project group: {group_id} does not exist."
        ));
    }
    persist_project_groups(controller_runtime, &groups).await?;
    refreshed_and_broadcast(&app, state).await
}

/// Applies a complete layout of the regular (unpinned) project section: group order, each group's
/// members, and the order of ungrouped projects.
/// 应用普通（未置顶）项目分区的完整布局：分组顺序、各分组成员以及未分组项目的顺序。
///
/// # Parameters
/// 参数
/// `groups` lists every existing group exactly once in the new order; `ungrouped_project_ids` lists
/// the remaining regular projects. Together they must name every visible unpinned project exactly once.
/// `groups` 按新顺序恰好列出每个已存在分组一次；`ungrouped_project_ids` 列出其余普通项目。两者合起来
/// 必须恰好包含每个可见未置顶项目一次。
///
/// # Returns
/// 返回值
/// Returns the refreshed workbench snapshot, or an error naming the first inconsistency.
/// 返回刷新后的工作台快照，或说明第一个不一致之处的错误。
pub(crate) async fn service_set_project_groups_layout(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    groups: Vec<ProjectGroupLayoutEntry>,
    ungrouped_project_ids: Vec<String>,
) -> Result<WorkbenchSnapshot, String> {
    let (controller_runtime, layout) = controller_runtime_and_project_layout_from_state(&state)?;
    let stored = load_project_groups(&controller_runtime)?;
    let next = project_groups_after_layout(&stored, &layout, &groups, &ungrouped_project_ids)?;
    // regular_order keeps ungrouped projects first so the persisted section order matches the tree.
    // regular_order 让未分组项目在前，使持久化的分区顺序与项目树一致。
    let regular_order = ungrouped_project_ids
        .iter()
        .chain(groups.iter().flat_map(|group| group.project_ids.iter()))
        .cloned()
        .collect::<Vec<_>>();
    let pinned_order = layout
        .iter()
        .filter(|project| project.pinned)
        .map(|project| project.id.clone())
        .collect::<Vec<_>>();
    persist_project_groups(controller_runtime.clone(), &next).await?;
    vulcan_workspace_store::workspace_store::persist_project_order_update(
        controller_runtime,
        &pinned_order,
        &regular_order,
    )
    .await?;
    refreshed_and_broadcast(&app, state).await
}

/// Refreshes the workbench after a group change and broadcasts it to every connected window.
/// 分组变更后刷新工作台，并广播给每个已连接窗口。
async fn refreshed_and_broadcast(
    app: &ServiceEventEmitter,
    state: SharedServiceBridgeState,
) -> Result<WorkbenchSnapshot, String> {
    let workbench = refresh_workbench_snapshot(state).await?;
    emit_workbench_event(app, &workbench);
    Ok(workbench)
}

/// Validates a regular-section layout request and builds the complete stored group list from it.
/// 校验普通分区布局请求，并据此构建完整的已保存分组列表。
///
/// Members that are not visible regular projects (pinned or archived) keep their stored group and are
/// appended after the requested members.
/// 不是可见普通项目的成员（置顶或已归档）保留其已保存分组，并追加在请求成员之后。
fn project_groups_after_layout(
    stored: &[ProjectGroup],
    layout: &[RetainedProjectLayoutEntry],
    requested: &[ProjectGroupLayoutEntry],
    ungrouped_project_ids: &[String],
) -> Result<Vec<ProjectGroup>, String> {
    let stored_ids = stored
        .iter()
        .map(|group| group.id.as_str())
        .collect::<BTreeSet<_>>();
    let requested_ids = requested
        .iter()
        .map(|group| group.group_id.as_str())
        .collect::<BTreeSet<_>>();
    if requested_ids.len() != requested.len() || requested_ids != stored_ids {
        return Err(String::from(
            "Failed to lay out project groups: the layout must name every existing group exactly once.",
        ));
    }
    let regular = layout
        .iter()
        .filter(|project| !project.pinned)
        .map(|project| project.id.as_str())
        .collect::<BTreeSet<_>>();
    let placed = requested
        .iter()
        .flat_map(|group| group.project_ids.iter())
        .chain(ungrouped_project_ids.iter())
        .map(String::as_str)
        .collect::<Vec<_>>();
    let placed_set = placed.iter().copied().collect::<BTreeSet<_>>();
    if placed_set.len() != placed.len() || placed_set != regular {
        return Err(String::from(
            "Failed to lay out project groups: the layout must place every unpinned visible project exactly once.",
        ));
    }
    Ok(requested
        .iter()
        .map(|entry| {
            let previous = stored
                .iter()
                .find(|group| group.id == entry.group_id)
                .expect("requested group ids equal the stored group ids");
            let mut project_ids = entry.project_ids.clone();
            project_ids.extend(
                previous
                    .project_ids
                    .iter()
                    .filter(|project_id| !regular.contains(project_id.as_str()))
                    .cloned(),
            );
            ProjectGroup {
                id: previous.id.clone(),
                name: previous.name.clone(),
                project_ids,
            }
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stored(id: &str, members: &[&str]) -> ProjectGroup {
        ProjectGroup {
            id: id.to_string(),
            name: id.to_uppercase(),
            project_ids: members.iter().map(|member| (*member).to_string()).collect(),
        }
    }

    fn entry(id: &str, members: &[&str]) -> ProjectGroupLayoutEntry {
        ProjectGroupLayoutEntry {
            group_id: id.to_string(),
            project_ids: members.iter().map(|member| (*member).to_string()).collect(),
        }
    }

    fn project(id: &str, pinned: bool) -> RetainedProjectLayoutEntry {
        RetainedProjectLayoutEntry {
            id: id.to_string(),
            pinned,
        }
    }

    fn ids(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_string()).collect()
    }

    #[test]
    fn layout_reorders_groups_moves_members_and_keeps_hidden_members() {
        // "archived" is stored in g1 but is not visible; "pin" is pinned and stays in g2.
        let stored_groups = vec![stored("g1", &["a", "archived"]), stored("g2", &["pin"])];
        let layout = vec![project("a", false), project("b", false), project("pin", true)];
        let next = project_groups_after_layout(
            &stored_groups,
            &layout,
            &[entry("g2", &["a"]), entry("g1", &[])],
            &ids(&["b"]),
        )
        .expect("complete layout");
        assert_eq!(next, vec![stored("g2", &["a", "pin"]), stored("g1", &["archived"])]);
    }

    #[test]
    fn layout_must_name_every_group_and_place_every_regular_project_once() {
        let stored_groups = vec![stored("g1", &[]), stored("g2", &[])];
        let layout = vec![project("a", false), project("b", false), project("p", true)];
        let missing_group =
            project_groups_after_layout(&stored_groups, &layout, &[entry("g1", &["a"])], &ids(&["b"]));
        assert!(missing_group.is_err());
        let missing_project = project_groups_after_layout(
            &stored_groups,
            &layout,
            &[entry("g1", &["a"]), entry("g2", &[])],
            &[],
        );
        assert!(missing_project.is_err());
        let duplicate = project_groups_after_layout(
            &stored_groups,
            &layout,
            &[entry("g1", &["a"]), entry("g2", &["a"])],
            &ids(&["b"]),
        );
        assert!(duplicate.is_err());
        let pinned_in_layout = project_groups_after_layout(
            &stored_groups,
            &layout,
            &[entry("g1", &["a", "p"]), entry("g2", &[])],
            &ids(&["b"]),
        );
        assert!(pinned_in_layout.is_err());
    }
}
