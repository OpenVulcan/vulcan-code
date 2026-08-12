import type {
  CommandSessionDisplayAttachment,
  ProjectTerminalCatalog,
  ProjectTerminalDisplayAttachment,
} from "@/types";
import { resolveBrowserServiceClientId } from "@/service/browser-service-discovery";
import { hasServiceRuntime, serviceClient } from "@/service/webui-service-runtime";

// attachCommandSessionDisplay performs the single initial query and enables active pushes.
// attachCommandSessionDisplay 执行唯一一次初始查询并启用主动推送。
export async function attachCommandSessionDisplay(
  commandSessionId: string,
  ownerSessionId?: string | null,
): Promise<CommandSessionDisplayAttachment | null> {
  if (!hasServiceRuntime()) return null;
  return serviceClient.request<CommandSessionDisplayAttachment>("service_attach_command_session_display", {
    commandSessionId,
    displayClientId: resolveBrowserServiceClientId(),
    ownerSessionId: ownerSessionId ?? null,
  });
}

// detachCommandSessionDisplay stops content delivery as soon as the AI terminal is hidden.
// detachCommandSessionDisplay 在 AI 终端隐藏后立即停止内容交付。
export async function detachCommandSessionDisplay(subscriptionId: string): Promise<boolean> {
  if (!hasServiceRuntime()) return false;
  return serviceClient.request<boolean>("service_detach_command_session_display", { subscriptionId });
}

// listProjectTerminals loads shells and terminals for one explicit active project.
// listProjectTerminals 为一个显式活动项目加载 Shell 与终端。
export async function listProjectTerminals(projectId: string): Promise<ProjectTerminalCatalog | null> {
  if (!hasServiceRuntime()) return null;
  return serviceClient.request<ProjectTerminalCatalog>("service_list_project_terminals", { projectId });
}

// createProjectTerminal starts one project-owned terminal in an isolated host.
// createProjectTerminal 在隔离宿主中启动一个项目级终端。
export async function createProjectTerminal(request: {
  projectId: string;
  name?: string | null;
  shellId: string;
  rows: number;
  cols: number;
}): Promise<ProjectTerminalCatalog | null> {
  if (!hasServiceRuntime()) return null;
  return serviceClient.request<ProjectTerminalCatalog>("service_create_project_terminal", { request });
}

// ProjectTerminalInputEventRequest carries exactly one xterm input representation.
// ProjectTerminalInputEventRequest 承载恰好一种 xterm 输入表示。
export type ProjectTerminalInputEventRequest =
  | { utf8Data: string; binaryDataBase64?: never }
  | { utf8Data?: never; binaryDataBase64: string };

// sendProjectTerminalInputBatch submits ordered input and returns the highest acknowledged sequence.
// sendProjectTerminalInputBatch 提交有序输入并返回已确认的最高序号。
export async function sendProjectTerminalInputBatch(
  projectId: string,
  terminalId: string,
  firstClientSeq: number,
  events: ProjectTerminalInputEventRequest[],
): Promise<number> {
  if (!hasServiceRuntime()) return firstClientSeq - 1;
  return serviceClient.request<number>("service_send_project_terminal_input", {
    request: {
      projectId,
      terminalId,
      firstClientSeq,
      events,
    },
  });
}

// resizeProjectTerminal applies xterm's measured character grid to the PTY.
// resizeProjectTerminal 将 xterm 测量的字符网格应用到 PTY。
export async function resizeProjectTerminal(
  projectId: string,
  terminalId: string,
  rows: number,
  cols: number,
): Promise<void> {
  if (!hasServiceRuntime()) return;
  await serviceClient.request<void>("service_resize_project_terminal", {
    request: { projectId, terminalId, rows, cols },
  });
}

// renameProjectTerminal changes terminal metadata without restarting its isolated host.
// renameProjectTerminal 更改终端元数据，而不重启其隔离宿主。
export async function renameProjectTerminal(
  projectId: string,
  terminalId: string,
  name: string,
): Promise<ProjectTerminalCatalog | null> {
  if (!hasServiceRuntime()) return null;
  return serviceClient.request<ProjectTerminalCatalog>("service_rename_project_terminal", {
    request: { projectId, terminalId, name },
  });
}

// stopProjectTerminal stops the child while retaining the terminal tab and output.
// stopProjectTerminal 停止子进程，同时保留终端标签与输出。
export async function stopProjectTerminal(projectId: string, terminalId: string): Promise<void> {
  if (!hasServiceRuntime()) return;
  await serviceClient.request<void>("service_stop_project_terminal", { projectId, terminalId });
}

// closeProjectTerminal removes one exact terminal from its project container.
// closeProjectTerminal 从项目容器移除一个精确终端。
export async function closeProjectTerminal(
  projectId: string,
  terminalId: string,
): Promise<ProjectTerminalCatalog | null> {
  if (!hasServiceRuntime()) return null;
  return serviceClient.request<ProjectTerminalCatalog>("service_close_project_terminal", {
    projectId,
    terminalId,
  });
}

// attachProjectTerminalDisplay performs one initial query and enables active pushes while visible.
// attachProjectTerminalDisplay 执行一次初始查询，并仅在可见时启用主动推送。
export async function attachProjectTerminalDisplay(
  projectId: string,
  terminalId: string,
): Promise<ProjectTerminalDisplayAttachment | null> {
  if (!hasServiceRuntime()) return null;
  return serviceClient.request<ProjectTerminalDisplayAttachment>("service_attach_project_terminal_display", {
    projectId,
    terminalId,
    displayClientId: resolveBrowserServiceClientId(),
  });
}

// detachProjectTerminalDisplay stops content delivery as soon as the project terminal is hidden.
// detachProjectTerminalDisplay 在项目终端隐藏后立即停止内容交付。
export async function detachProjectTerminalDisplay(subscriptionId: string): Promise<boolean> {
  if (!hasServiceRuntime()) return false;
  return serviceClient.request<boolean>("service_detach_project_terminal_display", { subscriptionId });
}
