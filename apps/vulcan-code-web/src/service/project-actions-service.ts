import type {
  ProjectDirectoryListing,
  ProjectGroupLayoutEntry,
  ProjectSessionSortMode,
  ProjectTreeItemKind,
  ProjectTreePin,
  WorkbenchSnapshot,
} from "@/types";
import {
  hasServiceRuntime,
  serviceClient,
} from "@/service/webui-service-runtime";

// ProjectIconChoice carries the persisted project icon override selected by the UI.
// ProjectIconChoice 承载 UI 选择的项目图标持久化覆盖。
export interface ProjectIconChoice {
  // pack identifies the icon source pack.
  // pack 标识图标来源包。
  pack: string;
  // key identifies the icon inside the source pack.
  // key 标识来源包内的具体图标。
  key: string;
}

// openProjectDirectoryPath asks the service to reveal one filesystem folder.
// openProjectDirectoryPath 请求服务打开单个文件系统文件夹。
export async function openProjectDirectoryPath(path: string): Promise<string | null> {
  return serviceClient.request<string>("service_open_project_directory_path", { path });
}

// openProjectDirectory asks the service to reveal one project folder.
// openProjectDirectory 请求服务打开单个项目文件夹。
export async function openProjectDirectory(projectId: string): Promise<string | null> {
  return serviceClient.request<string>("service_open_project_directory", { projectId });
}

// browseProjectDirectories loads one service-host directory for the WebUI picker.
// browseProjectDirectories 为 WebUI 选择器加载单个服务宿主目录。
export async function browseProjectDirectories(path: string | null): Promise<ProjectDirectoryListing> {
  if (!hasServiceRuntime()) {
    throw new Error("Browser service is unavailable, so project directories cannot be browsed.");
  }
  return serviceClient.request<ProjectDirectoryListing>("service_browse_project_directories", { path });
}

// addProjectFromPath registers one service-host directory as a project.
// addProjectFromPath 将单个服务宿主目录注册为项目。
export async function addProjectFromPath(path: string): Promise<WorkbenchSnapshot | null> {
  if (!hasServiceRuntime()) {
    throw new Error("Browser service is unavailable, so the project cannot be added.");
  }
  return serviceClient.request<WorkbenchSnapshot | null>("service_add_project_from_path", { path });
}

// SelfFixWorkspace is the source directory prepared for an on-device fix, already added as a project.
// SelfFixWorkspace 是为本机修复准备的源码目录，已作为项目添加。
export interface SelfFixWorkspace {
  projectId: string;
  path: string;
  // Whether the directory already holds a checkout, which the conversation fetches instead of cloning.
  // 目录是否已有检出；会话会改为拉取而不是克隆。
  existingCheckout: boolean;
  installedVersion: string;
}

// prepareSelfFixWorkspace creates or checks `folderName` under `parentDir` and adds it as a project; nothing is cloned.
// prepareSelfFixWorkspace 在 `parentDir` 下创建或检查 `folderName` 并将其加为项目；不进行克隆。
export async function prepareSelfFixWorkspace(parentDir: string, folderName: string): Promise<SelfFixWorkspace> {
  if (!hasServiceRuntime()) {
    throw new Error("Browser service is unavailable, so the source directory cannot be prepared.");
  }
  return serviceClient.request<SelfFixWorkspace>("service_prepare_self_fix_workspace", { parentDir, folderName });
}

// toggleProjectPinned asks the desktop shell to toggle one project pinned state.
// toggleProjectPinned 请求桌面壳切换单个项目的置顶状态。
export async function toggleProjectPinned(projectId: string): Promise<void> {
  if (!hasServiceRuntime()) {
    throw new Error("Service runtime is unavailable; project pinning was not changed.");
  }
  await serviceClient.request<void>("service_toggle_project_pinned", { projectId });
}

// renameProject asks the desktop shell to apply a new project display name.
// renameProject 请求桌面壳应用新的项目显示名称。
export async function renameProject(projectId: string, name: string): Promise<WorkbenchSnapshot | null> {
  if (!hasServiceRuntime()) {
    throw new Error("Service runtime is unavailable; the project was not renamed.");
  }
  return serviceClient.request<WorkbenchSnapshot>("service_rename_project", { projectId, name });
}

// setProjectIcon asks the desktop shell to persist one project icon override.
// setProjectIcon 请求桌面壳持久化单个项目图标覆盖。
export async function setProjectIcon(projectId: string, icon: ProjectIconChoice): Promise<void> {
  if (!hasServiceRuntime()) {
    throw new Error("Service runtime is unavailable; the project icon was not saved.");
  }
  await serviceClient.request<void>("service_set_project_icon", {
    projectId,
    iconPack: icon.pack,
    iconKey: icon.key,
  });
}

// resetProjectIcon asks the desktop shell to clear one project icon override.
// resetProjectIcon 请求桌面壳清除单个项目图标覆盖。
export async function resetProjectIcon(projectId: string): Promise<void> {
  if (!hasServiceRuntime()) {
    throw new Error("Service runtime is unavailable; the project icon was not reset.");
  }
  await serviceClient.request<void>("service_reset_project_icon", { projectId });
}

// setProjectSessionSortMode persists how one project orders its unpinned sessions.
// setProjectSessionSortMode 持久化一个项目排列非置顶会话的方式。
export async function setProjectSessionSortMode(
  projectId: string,
  mode: ProjectSessionSortMode,
): Promise<WorkbenchSnapshot> {
  if (!hasServiceRuntime()) {
    throw new Error("Service runtime is unavailable; the session sort mode was not saved.");
  }
  return serviceClient.request<WorkbenchSnapshot>("service_set_project_session_sort_mode", { projectId, mode });
}

// reorderProjectSessions persists the manual top-to-bottom order of one project's unpinned sessions.
// reorderProjectSessions 持久化一个项目非置顶会话自上而下的手动顺序。
export async function reorderProjectSessions(
  projectId: string,
  conversationIds: string[],
): Promise<WorkbenchSnapshot> {
  if (!hasServiceRuntime()) {
    throw new Error("Service runtime is unavailable; the session order was not saved.");
  }
  return serviceClient.request<WorkbenchSnapshot>("service_reorder_project_sessions", { projectId, conversationIds });
}

// setExpandedTaskProjects asks the desktop shell to persist expanded project rows.
// setExpandedTaskProjects 请求桌面壳持久化已展开的项目行。
export async function setExpandedTaskProjects(projectIds: string[]): Promise<WorkbenchSnapshot | null> {
  if (!hasServiceRuntime()) {
    throw new Error("Service runtime is unavailable; project expansion was not saved.");
  }
  return serviceClient.request<WorkbenchSnapshot>("service_set_expanded_task_projects", { projectIds });
}

// createProjectGroup creates a named group right after afterGroupId, or right above the default group, and moves
// the given projects into it.
// createProjectGroup 在 afterGroupId 之后（未给出时在默认分组正上方）创建命名分组，并把给定项目移入其中。
export async function createProjectGroup(name: string, projectIds: string[], afterGroupId?: string): Promise<WorkbenchSnapshot> {
  if (!hasServiceRuntime()) {
    throw new Error("Service runtime is unavailable; the project group was not created.");
  }
  return serviceClient.request<WorkbenchSnapshot>("service_create_project_group", { name, projectIds, afterGroupId });
}

// renameProjectGroup renames one existing project group.
// renameProjectGroup 重命名一个已存在的项目分组。
export async function renameProjectGroup(groupId: string, name: string): Promise<WorkbenchSnapshot> {
  if (!hasServiceRuntime()) {
    throw new Error("Service runtime is unavailable; the project group was not renamed.");
  }
  return serviceClient.request<WorkbenchSnapshot>("service_rename_project_group", { groupId, name });
}

// deleteProjectGroup deletes one group; its projects return to the default group.
// deleteProjectGroup 删除一个分组；其项目回到默认分组。
export async function deleteProjectGroup(groupId: string): Promise<WorkbenchSnapshot> {
  if (!hasServiceRuntime()) {
    throw new Error("Service runtime is unavailable; the project group was not dissolved.");
  }
  return serviceClient.request<WorkbenchSnapshot>("service_delete_project_group", { groupId });
}

// setProjectGroupsLayout saves the complete unpinned layout: every group in order, the default group included,
// with its members.
// setProjectGroupsLayout 保存完整的未置顶布局：按顺序排列的所有分组（含默认分组）及其成员。
export async function setProjectGroupsLayout(groups: ProjectGroupLayoutEntry[]): Promise<WorkbenchSnapshot> {
  if (!hasServiceRuntime()) {
    throw new Error("Service runtime is unavailable; the project group layout was not saved.");
  }
  return serviceClient.request<WorkbenchSnapshot>("service_set_project_groups_layout", { groups });
}

// setProjectTreePinned pins or unpins one session, project or group in the project tree.
// setProjectTreePinned 在项目树中置顶或取消置顶一个会话、项目或分组。
export async function setProjectTreePinned(kind: ProjectTreeItemKind, id: string, pinned: boolean): Promise<WorkbenchSnapshot> {
  if (!hasServiceRuntime()) {
    throw new Error("Service runtime is unavailable; the pin was not saved.");
  }
  return serviceClient.request<WorkbenchSnapshot>("service_set_project_tree_pinned", { kind, id, pinned });
}

// setProjectTreePinsOrder saves the order of the pinned section; it must name every shown pin once.
// setProjectTreePinsOrder 保存置顶分区的顺序；必须恰好包含每个已显示的置顶一次。
export async function setProjectTreePinsOrder(pins: ProjectTreePin[]): Promise<WorkbenchSnapshot> {
  if (!hasServiceRuntime()) {
    throw new Error("Service runtime is unavailable; the pinned order was not saved.");
  }
  return serviceClient.request<WorkbenchSnapshot>("service_set_project_tree_pins_order", { pins });
}

// addProjectToGroup adds a directory as a project in one group; an existing project only moves into the group.
// addProjectToGroup 将目录作为项目添加到某分组；已存在的项目只会移入该分组。
export async function addProjectToGroup(path: string, groupId: string): Promise<WorkbenchSnapshot> {
  if (!hasServiceRuntime()) {
    throw new Error("Service runtime is unavailable; the project was not added.");
  }
  return serviceClient.request<WorkbenchSnapshot>("service_add_project_to_group", { path, groupId });
}

// archiveProjectConversations asks the desktop shell to archive all conversations under one project.
// archiveProjectConversations 请求桌面壳归档单个项目下的全部会话。
export async function archiveProjectConversations(projectId: string): Promise<WorkbenchSnapshot | null> {
  if (!hasServiceRuntime()) {
    throw new Error("Service runtime is unavailable; project conversations were not archived.");
  }
  return serviceClient.request<WorkbenchSnapshot>("service_archive_project_conversations", { projectId });
}

// removeProject archives conversations and removes one project row from the active sidebar.
// removeProject 归档会话并从活动侧栏移除单个项目记录。
export async function removeProject(projectId: string): Promise<WorkbenchSnapshot | null> {
  if (!hasServiceRuntime()) {
    throw new Error("Service runtime is unavailable; the project was not removed.");
  }
  return serviceClient.request<WorkbenchSnapshot>("service_remove_project", { projectId });
}

// renameConversation asks the desktop shell to apply a new conversation display name.
// renameConversation 请求桌面壳应用新的会话显示名称。
export async function renameConversation(conversationId: string, name: string): Promise<WorkbenchSnapshot | null> {
  if (!hasServiceRuntime()) {
    throw new Error("Service runtime is unavailable; the conversation was not renamed.");
  }
  return serviceClient.request<WorkbenchSnapshot>("service_rename_conversation", { conversationId, name });
}

// archiveConversation asks the desktop shell to archive one conversation.
// archiveConversation 请求桌面壳归档单个会话。
export async function archiveConversation(conversationId: string): Promise<WorkbenchSnapshot | null> {
  if (!hasServiceRuntime()) {
    throw new Error("Service runtime is unavailable; the conversation was not archived.");
  }
  return serviceClient.request<WorkbenchSnapshot>("service_archive_conversation", { conversationId });
}

// restoreArchivedConversations restores the selected archived conversations as one lifecycle transaction.
// restoreArchivedConversations 将选中的归档会话作为一次生命周期事务恢复。
export async function restoreArchivedConversations(conversationIds: string[]): Promise<WorkbenchSnapshot | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<WorkbenchSnapshot>("service_restore_archived_conversations", { conversationIds });
}

// deleteArchivedConversations permanently deletes the selected archived conversations as one lifecycle transaction.
// deleteArchivedConversations 将选中的归档会话作为一次生命周期事务彻底删除。
export async function deleteArchivedConversations(conversationIds: string[]): Promise<WorkbenchSnapshot | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<WorkbenchSnapshot>("service_delete_archived_conversations", { conversationIds });
}

// deleteUnsupportedConversation deletes one active conversation whose stored format this build cannot read.
// deleteUnsupportedConversation 删除一个存储格式本构建无法读取的活动会话。
export async function deleteUnsupportedConversation(conversationId: string): Promise<WorkbenchSnapshot> {
  if (!hasServiceRuntime()) {
    throw new Error("Service runtime is unavailable; the conversation was not deleted.");
  }
  return serviceClient.request<WorkbenchSnapshot>("service_delete_unsupported_conversation", { conversationId });
}

// isUnsupportedSessionFormatError recognizes the storage code for a session written by an older development format.
// isUnsupportedSessionFormatError 识别由旧开发格式写入的会话所对应的存储错误码。
export function isUnsupportedSessionFormatError(error: unknown): boolean {
  return (error instanceof Error ? error.message : String(error)).includes("unsupported_development_format");
}

// markConversationRead asks the desktop shell to persist one conversation as read.
// markConversationRead 请求桌面壳将单个会话持久化标记为已读。
export async function markConversationRead(conversationId: string): Promise<WorkbenchSnapshot | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<WorkbenchSnapshot>("service_mark_conversation_read", { conversationId });
}

// selectConversation asks the desktop shell to update its active conversation marker.
// selectConversation 请求桌面壳同步当前活跃会话标记。
export async function selectConversation(conversationId: string): Promise<WorkbenchSnapshot | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<WorkbenchSnapshot>("service_select_conversation", { conversationId });
}

// EXTERNAL_URL_OPENED is the exact host result for a completed shell navigation request.
// EXTERNAL_URL_OPENED 是 Shell 导航请求完成时返回的精确宿主结果。
const EXTERNAL_URL_OPENED = "opened";

// openExternalUrl asks the service to open one HTTPS external URL in the system browser.
// openExternalUrl 请求服务在系统浏览器中打开一个 HTTPS 外部链接。
//
// url is the exact target; the service rejects only non-HTTPS URLs and URLs carrying credentials.
// url 是精确目标；服务只拒绝非 HTTPS 链接与携带凭据的链接。
// Returns after opening, or throws the host failure.
// 打开后返回，否则抛出宿主失败。
export async function openExternalUrl(url: string): Promise<void> {
  // result is the exact stable host result for this URL.
  // result 是宿主针对该 URL 返回的精确稳定结果。
  const result = await serviceClient.request<string>("service_open_external_url", { url });
  if (result !== EXTERNAL_URL_OPENED) {
    throw new Error(`Unexpected external URL service result: ${result}`);
  }
}
