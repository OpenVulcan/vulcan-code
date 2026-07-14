import type {
  ProjectDirectoryListing,
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

// toggleProjectPinned asks the desktop shell to toggle one project pinned state.
// toggleProjectPinned 请求桌面壳切换单个项目的置顶状态。
export async function toggleProjectPinned(projectId: string): Promise<void> {
  if (!hasServiceRuntime()) {
    return;
  }
  await serviceClient.request<void>("service_toggle_project_pinned", { projectId });
}

// reorderProjects asks the desktop shell to persist one same-section project order.
// reorderProjects 请求桌面壳持久化同一分区内的项目顺序。
export async function reorderProjects(projectIds: string[]): Promise<WorkbenchSnapshot | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<WorkbenchSnapshot>("service_reorder_projects", { projectIds });
}

// renameProject asks the desktop shell to apply a new project display name.
// renameProject 请求桌面壳应用新的项目显示名称。
export async function renameProject(projectId: string, name: string): Promise<WorkbenchSnapshot | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<WorkbenchSnapshot>("service_rename_project", { projectId, name });
}

// setProjectIcon asks the desktop shell to persist one project icon override.
// setProjectIcon 请求桌面壳持久化单个项目图标覆盖。
export async function setProjectIcon(projectId: string, icon: ProjectIconChoice): Promise<void> {
  if (!hasServiceRuntime()) {
    return;
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
    return;
  }
  await serviceClient.request<void>("service_reset_project_icon", { projectId });
}

// setExpandedTaskProjects asks the desktop shell to persist expanded project rows.
// setExpandedTaskProjects 请求桌面壳持久化已展开的项目行。
export async function setExpandedTaskProjects(projectIds: string[]): Promise<WorkbenchSnapshot | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<WorkbenchSnapshot>("service_set_expanded_task_projects", { projectIds });
}

// archiveProjectConversations asks the desktop shell to archive all conversations under one project.
// archiveProjectConversations 请求桌面壳归档单个项目下的全部会话。
export async function archiveProjectConversations(projectId: string): Promise<WorkbenchSnapshot | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<WorkbenchSnapshot>("service_archive_project_conversations", { projectId });
}

// removeProject archives conversations and removes one project row from the active sidebar.
// removeProject 归档会话并从活动侧栏移除单个项目记录。
export async function removeProject(projectId: string): Promise<WorkbenchSnapshot | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<WorkbenchSnapshot>("service_remove_project", { projectId });
}

// renameConversation asks the desktop shell to apply a new conversation display name.
// renameConversation 请求桌面壳应用新的会话显示名称。
export async function renameConversation(conversationId: string, name: string): Promise<WorkbenchSnapshot | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<WorkbenchSnapshot>("service_rename_conversation", { conversationId, name });
}

// archiveConversation asks the desktop shell to archive one conversation.
// archiveConversation 请求桌面壳归档单个会话。
export async function archiveConversation(conversationId: string): Promise<WorkbenchSnapshot | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<WorkbenchSnapshot>("service_archive_conversation", { conversationId });
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

// openExternalUrl asks the service to open one trusted external URL.
// openExternalUrl 请求服务打开单个可信外部链接。
// EXTERNAL_URL_OPENED is the exact host result for a completed shell navigation request.
// EXTERNAL_URL_OPENED 是 Shell 导航请求完成时返回的精确宿主结果。
const EXTERNAL_URL_OPENED = "opened";

// EXTERNAL_URL_CONFIRMATION_REQUIRED is the exact host result for a blocked non-allowlisted domain.
// EXTERNAL_URL_CONFIRMATION_REQUIRED 是非白名单域名被拦截时返回的精确宿主结果。
const EXTERNAL_URL_CONFIRMATION_REQUIRED = "confirmation_required";

// ExternalUrlConfirmationRequiredError carries the exact URL awaiting a trusted user gesture.
// ExternalUrlConfirmationRequiredError 携带等待可信用户操作的精确 URL。
export class ExternalUrlConfirmationRequiredError extends Error {
  // url is the exact safe HTTPS URL rejected only by the domain allowlist.
  // url 是仅因域名白名单而被拒绝的精确安全 HTTPS URL。
  readonly url: string;

  // Creates one typed confirmation request for the trusted host UI.
  // 为可信宿主界面创建一个类型化确认请求。
  //
  // url is the exact safe HTTPS URL returned by the service decision.
  // url 是服务决策返回的精确安全 HTTPS URL。
  // Returns a typed error consumed only by the trusted UI layer.
  // 返回一个仅由可信界面层处理的类型化错误。
  constructor(url: string) {
    super("External URL requires explicit user confirmation.");
    this.name = "ExternalUrlConfirmationRequiredError";
    this.url = url;
  }
}

// Opens one HTTPS external URL or requests a trusted UI confirmation for a non-allowlisted domain.
// 打开一个 HTTPS 外部链接，或为非白名单域名请求可信界面确认。
//
// url is the exact target and userConfirmed records a direct trusted-UI confirmation gesture.
// url 是精确目标，userConfirmed 记录可信界面的直接确认操作。
// Returns after opening, or throws a typed confirmation request or host failure.
// 打开后返回，或抛出类型化确认请求及宿主失败。
export async function openExternalUrl(url: string, userConfirmed = false): Promise<void> {
  // result is the exact stable host decision for this URL and confirmation state.
  // result 是宿主针对当前 URL 与确认状态返回的精确稳定决策。
  const result = await serviceClient.request<string>("service_open_external_url", { url, userConfirmed });
  if (result === EXTERNAL_URL_OPENED) {
    return;
  }
  if (result === EXTERNAL_URL_CONFIRMATION_REQUIRED) {
    throw new ExternalUrlConfirmationRequiredError(url);
  }
  throw new Error(`Unexpected external URL service result: ${result}`);
}
