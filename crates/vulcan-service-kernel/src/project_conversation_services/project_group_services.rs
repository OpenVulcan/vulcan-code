//! Project tree service handlers: create, rename, delete and lay out user-named project groups, pin sessions,
//! projects and groups, order the pinned section, and add a project directly into a group.
//! 项目树服务处理器：创建、重命名、删除用户命名的项目分组并调整其布局，置顶会话、项目与分组，排列置顶分区，
//! 以及把项目直接添加到某分组。
//!
//! Every handler reads the stored tree, applies one validated change, writes the complete tree back, and returns
//! the refreshed workbench snapshot, which is also broadcast so every open window follows. Pinned and archived
//! members are kept in storage but are not part of a layout request, so pinning or archiving a project never
//! loses its group. Every project outside the user groups belongs to the default group.
//! 每个处理器读取已保存的项目树、应用一次经过校验的变更、写回完整项目树，并返回刷新后的工作台快照；该快照同时
//! 广播，使所有打开的窗口一同更新。置顶与归档成员保留在存储中但不属于布局请求，因此置顶或归档项目不会丢失其分组。
//! 不属于任何用户分组的项目都归入默认分组。

use super::project_services::{
    RetainedProjectLayoutEntry, controller_runtime_and_project_layout_from_state,
    project_layout_after_pin_toggle,
};
use super::*;
use vulcan_service_api::{ProjectGroupLayoutEntry, ProjectTreeItemKind, ProjectTreePinSummary};
use vulcan_workspace_store::workspace_store::{
    DEFAULT_PROJECT_GROUP_ID, ProjectGroup, ProjectTree, ProjectTreePin, ProjectTreePinKind,
    load_project_tree, normalize_project_group_name, persist_project_tree,
};

/// Creates a group and moves the given visible projects into it. The group is placed right after `after_group_id`
/// (the default group included), or right above the default group when no anchor is given.
/// 创建分组并把给定的可见项目移入其中。分组放在 `after_group_id`（含默认分组）之后；未给出锚点时放在默认分组正上方。
///
/// # Parameters
/// 参数
/// `name` is the requested group name; `project_ids` are visible projects to move into the new group;
/// `after_group_id` optionally names the group the new one follows.
/// `name` 是请求的分组名称；`project_ids` 是要移入新分组的可见项目；`after_group_id` 可选地指定新分组所跟随的分组。
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
    after_group_id: Option<String>,
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
    let tree = load_project_tree(&controller_runtime)?;
    let pins = tree.pins.clone();
    let mut slots = tree.into_slots();
    for group in slots.iter_mut().flatten() {
        group
            .project_ids
            .retain(|project_id| !project_ids.contains(project_id));
    }
    let position = match after_group_id.as_deref() {
        Some(anchor) => {
            slot_position(&slots, anchor).ok_or_else(|| {
                format!("Failed to create the project group: {anchor} does not exist.")
            })? + 1
        }
        None => slot_position(&slots, DEFAULT_PROJECT_GROUP_ID)
            .expect("the slots always hold the default group"),
    };
    slots.insert(
        position,
        Some(ProjectGroup {
            id: Uuid::new_v4().to_string(),
            name,
            project_ids,
        }),
    );
    persist_project_tree(controller_runtime, &ProjectTree::from_slots(slots, pins)?).await?;
    refreshed_and_broadcast(&app, state).await
}

/// Renames one existing user group; the default group has no stored name.
/// 重命名一个已存在的用户分组；默认分组没有保存的名称。
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
    let mut tree = load_project_tree(&controller_runtime)?;
    let group = tree
        .groups
        .iter_mut()
        .find(|group| group.id == group_id)
        .ok_or_else(|| format!("Failed to rename the project group: {group_id} does not exist."))?;
    group.name = name;
    persist_project_tree(controller_runtime, &tree).await?;
    refreshed_and_broadcast(&app, state).await
}

/// Deletes one user group; its projects return to the default group and its pin is removed.
/// 删除一个用户分组；其项目回到默认分组，其置顶被移除。
///
/// # Parameters
/// 参数
/// `group_id` names the group to delete; the default group cannot be deleted.
/// `group_id` 指定要删除的分组；默认分组不能删除。
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
    let tree = load_project_tree(&controller_runtime)?;
    let mut pins = tree.pins.clone();
    pins.retain(|pin| !(pin.kind == ProjectTreePinKind::Group && pin.id == group_id));
    let mut slots = tree.into_slots();
    let before = slots.len();
    slots.retain(|slot| slot.as_ref().is_none_or(|group| group.id != group_id));
    if slots.len() == before {
        return Err(format!(
            "Failed to delete the project group: {group_id} does not exist."
        ));
    }
    persist_project_tree(controller_runtime, &ProjectTree::from_slots(slots, pins)?).await?;
    refreshed_and_broadcast(&app, state).await
}

/// Applies a complete layout of the regular (unpinned) projects: the order of every group, the default group
/// included, and each group's members.
/// 应用普通（未置顶）项目的完整布局：所有分组（含默认分组）的顺序及各分组成员。
///
/// # Parameters
/// 参数
/// `groups` lists every group exactly once in the new order, the default group under `default`; together the
/// entries must name every visible unpinned project exactly once.
/// `groups` 按新顺序恰好列出每个分组一次，默认分组为 `default`；所有条目合起来必须恰好包含每个可见未置顶项目一次。
///
/// # Returns
/// 返回值
/// Returns the refreshed workbench snapshot, or an error naming the first inconsistency.
/// 返回刷新后的工作台快照，或说明第一个不一致之处的错误。
pub(crate) async fn service_set_project_groups_layout(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    groups: Vec<ProjectGroupLayoutEntry>,
) -> Result<WorkbenchSnapshot, String> {
    let (controller_runtime, layout) = controller_runtime_and_project_layout_from_state(&state)?;
    let stored = load_project_tree(&controller_runtime)?;
    let next = project_tree_after_layout(&stored, &layout, &groups)?;
    // regular_order follows the tree so the persisted section order matches what is shown.
    // regular_order 跟随项目树，使持久化的分区顺序与显示一致。
    let regular_order = groups
        .iter()
        .flat_map(|group| group.project_ids.iter())
        .cloned()
        .collect::<Vec<_>>();
    let pinned_order = layout
        .iter()
        .filter(|project| project.pinned)
        .map(|project| project.id.clone())
        .collect::<Vec<_>>();
    persist_project_tree(controller_runtime.clone(), &next).await?;
    vulcan_workspace_store::workspace_store::persist_project_order_update(
        controller_runtime,
        &pinned_order,
        &regular_order,
    )
    .await?;
    refreshed_and_broadcast(&app, state).await
}

/// Pins or unpins one session, project or group. The item keeps its place in the tree and is shown in the pinned
/// section while pinned; a new pin goes to the end of that section. A project's pin also sets its pinned flag.
/// 置顶或取消置顶一个会话、项目或分组。条目在树中的归属不变，置顶期间显示在置顶分区；新置顶排在该分区末尾。
/// 项目的置顶同时设置其置顶标记。
///
/// # Parameters
/// 参数
/// `kind` and `id` name the item; `pinned` is the requested state.
/// `kind` 与 `id` 指定条目；`pinned` 是请求的状态。
///
/// # Returns
/// 返回值
/// Returns the refreshed workbench snapshot, or an error when the item is not visible.
/// 返回刷新后的工作台快照；条目不可见时返回错误。
pub(crate) async fn service_set_project_tree_pinned(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    kind: ProjectTreeItemKind,
    id: String,
    pinned: bool,
) -> Result<WorkbenchSnapshot, String> {
    let (controller_runtime, layout) = controller_runtime_and_project_layout_from_state(&state)?;
    let mut tree = load_project_tree(&controller_runtime)?;
    let kind = stored_pin_kind(kind);
    match kind {
        ProjectTreePinKind::Session => {
            if pinned && !visible_session_exists(&state, &id)? {
                return Err(format!("Failed to pin the session: {id} is not visible."));
            }
        }
        ProjectTreePinKind::Group => {
            if !tree.has_group(&id) {
                return Err(format!(
                    "Failed to pin the project group: {id} does not exist."
                ));
            }
        }
        ProjectTreePinKind::Project => {
            let project = layout
                .iter()
                .find(|project| project.id == id)
                .ok_or_else(|| format!("Failed to pin the project: {id} is not visible."))?;
            if project.pinned != pinned {
                let (next_pinned, sections) = project_layout_after_pin_toggle(&layout, &id)?;
                vulcan_workspace_store::workspace_store::persist_project_pinned_and_order_update(
                    controller_runtime.clone(),
                    &id,
                    next_pinned,
                    &sections.pinned,
                    &sections.regular,
                )
                .await?;
            }
        }
    }
    tree.set_pinned(kind, &id, pinned);
    persist_project_tree(controller_runtime, &tree).await?;
    refreshed_and_broadcast(&app, state).await
}

/// Orders the pinned section. Pins that are stored but not shown (an archived session, for example) keep their
/// storage and follow the requested order.
/// 排列置顶分区。已保存但未显示的置顶（例如已归档会话）仍保留存储，并排在请求顺序之后。
///
/// # Parameters
/// 参数
/// `pins` lists every shown pinned item exactly once in the new order.
/// `pins` 按新顺序恰好列出每个已显示的置顶条目一次。
///
/// # Returns
/// 返回值
/// Returns the refreshed workbench snapshot, or an error when the request does not match the shown pins.
/// 返回刷新后的工作台快照；请求与已显示的置顶不一致时返回错误。
pub(crate) async fn service_set_project_tree_pins_order(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    pins: Vec<ProjectTreePinSummary>,
) -> Result<WorkbenchSnapshot, String> {
    let (controller_runtime, layout) = controller_runtime_and_project_layout_from_state(&state)?;
    let shown = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?
        .workbench
        .project_tree
        .pins
        .clone();
    let requested = pins
        .iter()
        .map(|pin| (stored_pin_kind(pin.kind), pin.id.as_str()))
        .collect::<BTreeSet<_>>();
    let expected = shown
        .iter()
        .map(|pin| (stored_pin_kind(pin.kind), pin.id.as_str()))
        .collect::<BTreeSet<_>>();
    if requested.len() != pins.len() || requested != expected {
        return Err(String::from(
            "Failed to order the pinned section: the order must name every pinned item exactly once.",
        ));
    }
    let mut tree = load_project_tree(&controller_runtime)?;
    let hidden = tree
        .pins
        .iter()
        .filter(|pin| !requested.contains(&(pin.kind, pin.id.as_str())))
        .cloned()
        .collect::<Vec<_>>();
    tree.pins = pins
        .iter()
        .map(|pin| ProjectTreePin {
            kind: stored_pin_kind(pin.kind),
            id: pin.id.clone(),
        })
        .chain(hidden)
        .collect();
    let pinned_order = pins
        .iter()
        .filter(|pin| pin.kind == ProjectTreeItemKind::Project)
        .map(|pin| pin.id.clone())
        .collect::<Vec<_>>();
    let regular_order = layout
        .iter()
        .filter(|project| !project.pinned)
        .map(|project| project.id.clone())
        .collect::<Vec<_>>();
    persist_project_tree(controller_runtime.clone(), &tree).await?;
    vulcan_workspace_store::workspace_store::persist_project_order_update(
        controller_runtime,
        &pinned_order,
        &regular_order,
    )
    .await?;
    refreshed_and_broadcast(&app, state).await
}

/// Adds a directory as a project and places it at the end of one group. A directory that already is a visible
/// project is only moved into the group.
/// 将目录添加为项目并放到某分组末尾。已是可见项目的目录只会被移入该分组。
///
/// # Parameters
/// 参数
/// `path` is the chosen directory; `group_id` names the target group, the default group included.
/// `path` 是所选目录；`group_id` 指定目标分组，含默认分组。
///
/// # Returns
/// 返回值
/// Returns the refreshed workbench snapshot, or an error when the group or directory is not usable.
/// 返回刷新后的工作台快照；分组或目录不可用时返回错误。
pub(crate) async fn service_add_project_to_group(
    app: ServiceEventEmitter,
    state: SharedServiceBridgeState,
    path: PathBuf,
    group_id: String,
) -> Result<WorkbenchSnapshot, String> {
    let controller_runtime = controller_runtime_from_state(&state)?;
    if !load_project_tree(&controller_runtime)?.has_group(&group_id) {
        return Err(format!(
            "Failed to add the project: project group {group_id} does not exist."
        ));
    }
    // binding_notice belongs only to this add response, so it is carried over to the caller's snapshot.
    // binding_notice 只属于本次添加响应，因此转交给调用方的快照。
    let binding_notice = service_add_project_from_path(state.clone(), path.clone())
        .await?
        .and_then(|workbench| workbench.vmm_memory.project_binding_notice);
    let project_id = visible_project_id_for_path(&state, &path)?.ok_or_else(|| {
        String::from("Failed to add the project: the directory is not a visible project.")
    })?;
    let mut tree = load_project_tree(&controller_runtime)?;
    tree.place_project(&project_id, &group_id)?;
    persist_project_tree(controller_runtime, &tree).await?;
    let mut workbench = refreshed_and_broadcast(&app, state).await?;
    workbench.vmm_memory.project_binding_notice = binding_notice;
    Ok(workbench)
}

/// Refreshes the workbench after a tree change and broadcasts it to every connected window.
/// 项目树变更后刷新工作台，并广播给每个已连接窗口。
async fn refreshed_and_broadcast(
    app: &ServiceEventEmitter,
    state: SharedServiceBridgeState,
) -> Result<WorkbenchSnapshot, String> {
    let workbench = refresh_workbench_snapshot(state).await?;
    emit_workbench_event(app, &workbench);
    Ok(workbench)
}

/// Finds one group in the combined order; `default` finds the default group.
/// 在合并顺序中查找某分组；`default` 查找默认分组。
fn slot_position(slots: &[Option<ProjectGroup>], group_id: &str) -> Option<usize> {
    slots.iter().position(|slot| match slot {
        Some(group) => group.id == group_id,
        None => group_id == DEFAULT_PROJECT_GROUP_ID,
    })
}

/// Converts a wire pin kind into its stored form.
/// 将线协议置顶种类转换为存储形式。
fn stored_pin_kind(kind: ProjectTreeItemKind) -> ProjectTreePinKind {
    match kind {
        ProjectTreeItemKind::Session => ProjectTreePinKind::Session,
        ProjectTreeItemKind::Project => ProjectTreePinKind::Project,
        ProjectTreeItemKind::Group => ProjectTreePinKind::Group,
    }
}

/// Whether one session is visible in the retained workspace snapshot.
/// 某会话在保留的工作区快照中是否可见。
fn visible_session_exists(
    state: &SharedServiceBridgeState,
    session_id: &str,
) -> Result<bool, String> {
    let guard = state
        .lock()
        .map_err(|error| format!("failed to lock service state: {error}"))?;
    let workspace = guard.workspace_snapshot.as_ref().ok_or_else(|| {
        String::from("service is still starting; workspace snapshot is not ready")
    })?;
    Ok(workspace
        .sessions
        .iter()
        .any(|session| session.id.to_string() == session_id))
}

/// Validates a layout request and builds the complete stored tree from it.
/// 校验布局请求，并据此构建完整的已保存项目树。
///
/// Members that are not visible regular projects (pinned or archived) keep their stored group and are appended
/// after the requested members.
/// 不是可见普通项目的成员（置顶或已归档）保留其已保存分组，并追加在请求成员之后。
fn project_tree_after_layout(
    stored: &ProjectTree,
    layout: &[RetainedProjectLayoutEntry],
    requested: &[ProjectGroupLayoutEntry],
) -> Result<ProjectTree, String> {
    let stored_ids = stored
        .ordered_group_ids()
        .into_iter()
        .collect::<BTreeSet<_>>();
    let requested_ids = requested
        .iter()
        .map(|group| group.group_id.as_str())
        .collect::<BTreeSet<_>>();
    if requested_ids.len() != requested.len() || requested_ids != stored_ids {
        return Err(String::from(
            "Failed to lay out project groups: the layout must name every group, the default group included, exactly once.",
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
        .map(String::as_str)
        .collect::<Vec<_>>();
    let placed_set = placed.iter().copied().collect::<BTreeSet<_>>();
    if placed_set.len() != placed.len() || placed_set != regular {
        return Err(String::from(
            "Failed to lay out project groups: the layout must place every unpinned visible project exactly once.",
        ));
    }
    let slots = requested
        .iter()
        .map(|entry| {
            if entry.group_id == DEFAULT_PROJECT_GROUP_ID {
                return None;
            }
            let previous = stored
                .groups
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
            Some(ProjectGroup {
                id: previous.id.clone(),
                name: previous.name.clone(),
                project_ids,
            })
        })
        .collect::<Vec<_>>();
    ProjectTree::from_slots(slots, stored.pins.clone())
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

    fn tree(groups: Vec<ProjectGroup>, default_group_index: usize) -> ProjectTree {
        ProjectTree {
            groups,
            default_group_index,
            pins: Vec::new(),
        }
    }

    #[test]
    fn layout_reorders_groups_moves_members_and_keeps_hidden_members() {
        // "archived" is stored in g1 but is not visible; "pin" is pinned and stays in g2.
        let stored_tree = tree(
            vec![stored("g1", &["a", "archived"]), stored("g2", &["pin"])],
            2,
        );
        let layout = vec![
            project("a", false),
            project("b", false),
            project("pin", true),
        ];
        let next = project_tree_after_layout(
            &stored_tree,
            &layout,
            &[
                entry(DEFAULT_PROJECT_GROUP_ID, &["b"]),
                entry("g2", &["a"]),
                entry("g1", &[]),
            ],
        )
        .expect("complete layout");
        assert_eq!(
            next,
            tree(
                vec![stored("g2", &["a", "pin"]), stored("g1", &["archived"])],
                0
            )
        );
    }

    #[test]
    fn layout_must_name_every_group_and_place_every_regular_project_once() {
        let stored_tree = tree(vec![stored("g1", &[]), stored("g2", &[])], 2);
        let layout = vec![project("a", false), project("b", false), project("p", true)];
        let attempt = |entries: &[ProjectGroupLayoutEntry]| {
            project_tree_after_layout(&stored_tree, &layout, entries)
        };
        let missing_default = attempt(&[entry("g1", &["a", "b"]), entry("g2", &[])]);
        assert!(missing_default.is_err());
        let missing_group =
            attempt(&[entry("g1", &["a"]), entry(DEFAULT_PROJECT_GROUP_ID, &["b"])]);
        assert!(missing_group.is_err());
        let missing_project = attempt(&[
            entry("g1", &["a"]),
            entry("g2", &[]),
            entry(DEFAULT_PROJECT_GROUP_ID, &[]),
        ]);
        assert!(missing_project.is_err());
        let duplicate = attempt(&[
            entry("g1", &["a"]),
            entry("g2", &["a"]),
            entry(DEFAULT_PROJECT_GROUP_ID, &["b"]),
        ]);
        assert!(duplicate.is_err());
        let pinned_in_layout = attempt(&[
            entry("g1", &["a", "p"]),
            entry("g2", &[]),
            entry(DEFAULT_PROJECT_GROUP_ID, &["b"]),
        ]);
        assert!(pinned_in_layout.is_err());
    }

    #[test]
    fn slot_position_finds_user_groups_and_the_default_group() {
        let slots = tree(vec![stored("g1", &[]), stored("g2", &[])], 1).into_slots();
        assert_eq!(slot_position(&slots, "g1"), Some(0));
        assert_eq!(slot_position(&slots, DEFAULT_PROJECT_GROUP_ID), Some(1));
        assert_eq!(slot_position(&slots, "g2"), Some(2));
        assert_eq!(slot_position(&slots, "gone"), None);
    }
}
