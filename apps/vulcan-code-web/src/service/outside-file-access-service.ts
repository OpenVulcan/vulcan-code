import { serviceClient } from "@/service/webui-service-runtime";
import type { OutsideWorkspaceFileAccess, SessionOutsideFileAccess, WorkbenchSnapshot, WorkspaceFileEntryKind, WorkspaceFileReadResponse } from "@/types";

// saveOutsideWorkspaceFileAccess saves the system default for sessions without their own choice.
// saveOutsideWorkspaceFileAccess 保存未自行选择的会话所用的系统默认值。
export async function saveOutsideWorkspaceFileAccess(access: OutsideWorkspaceFileAccess): Promise<WorkbenchSnapshot> {
  return serviceClient.request<WorkbenchSnapshot>("service_save_outside_workspace_file_access", { access });
}

// loadSessionOutsideFileAccess reads one session's own choice next to the system default.
// loadSessionOutsideFileAccess 读取一个会话的自有选择及系统默认值。
export async function loadSessionOutsideFileAccess(conversationId: string): Promise<SessionOutsideFileAccess> {
  return serviceClient.request<SessionOutsideFileAccess>("service_session_outside_file_access", { conversationId });
}

// saveSessionOutsideFileAccess saves one session's own choice, or returns it to the system default with null.
// saveSessionOutsideFileAccess 保存一个会话的自有选择；null 使其恢复跟随系统默认值。
export async function saveSessionOutsideFileAccess(conversationId: string, access: OutsideWorkspaceFileAccess | null): Promise<SessionOutsideFileAccess> {
  return serviceClient.request<SessionOutsideFileAccess>("service_save_session_outside_file_access", { conversationId, access });
}

// loadWorkspaceFileKind classifies a file path by name the way file previews do, without reading the file.
// loadWorkspaceFileKind 按名称对文件路径分类（与文件预览一致），不读取文件。
export async function loadWorkspaceFileKind(filePath: string): Promise<WorkspaceFileEntryKind> {
  return serviceClient.request<WorkspaceFileEntryKind>("workspace.files.kind", { filePath });
}

// loadOutsideTextFile reads the bounded text preview of a file outside any workspace at an absolute path.
// loadOutsideTextFile 读取任一工作区之外绝对路径处文件的有界文本预览。
export async function loadOutsideTextFile(filePath: string): Promise<WorkspaceFileReadResponse> {
  return serviceClient.request<WorkspaceFileReadResponse>("workspace.files.readOutside", { filePath });
}
