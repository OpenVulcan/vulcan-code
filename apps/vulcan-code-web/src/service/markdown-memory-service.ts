import { serviceClient } from "@/service/webui-service-runtime";
import type {
  MarkdownMemoryFile,
  MarkdownMemoryOperation,
  MarkdownMemoryOverview,
  MarkdownMemoryProject,
  MarkdownMemoryProjectAction,
  MarkdownMemoryScope,
  MarkdownMemoryTarget,
  MemoryInjectionBudget,
  MemoryMode,
  MemorySettingsView,
} from "@/types";

// loadMemorySettings reads the global memory mode and injection budget.
// loadMemorySettings 读取全局记忆模式与注入预算。
export async function loadMemorySettings(): Promise<MemorySettingsView> {
  return serviceClient.request<MemorySettingsView>("service_get_memory_settings", {});
}

// saveMemorySettings saves the mode and budget; VMM is switched off in memory only, never in its saved configuration.
// saveMemorySettings 保存模式与预算；VMM 只在内存中关闭，从不改写其已保存配置。
export async function saveMemorySettings(request: {
  mode: MemoryMode;
  budget: MemoryInjectionBudget;
}): Promise<MemorySettingsView> {
  return serviceClient.request<MemorySettingsView>("service_save_memory_settings", { request });
}

// loadMarkdownMemory reads every scope the page shows for a host project or an explicit memory project.
// loadMarkdownMemory 为宿主项目或显式记忆项目读取页面显示的全部层。
export async function loadMarkdownMemory(target: MarkdownMemoryTarget): Promise<MarkdownMemoryOverview> {
  return serviceClient.request<MarkdownMemoryOverview>("service_load_markdown_memory", {
    request: markdownMemoryTargetRequest(target),
  });
}

// writeMarkdownMemory applies operations as the user, all or nothing, and returns the refreshed page.
// writeMarkdownMemory 以用户身份整体应用操作，并返回刷新后的页面。
export async function writeMarkdownMemory(
  target: MarkdownMemoryTarget,
  operations: MarkdownMemoryOperation[],
): Promise<MarkdownMemoryOverview> {
  return serviceClient.request<MarkdownMemoryOverview>("service_write_markdown_memory", {
    request: { ...markdownMemoryTargetRequest(target), operations },
  });
}

// readMarkdownMemoryFile reads one whole file for the fallback editor.
// readMarkdownMemoryFile 为兜底编辑器读取整个文件。
export async function readMarkdownMemoryFile(
  scope: MarkdownMemoryScope,
  memoryProjectId: string | null,
): Promise<MarkdownMemoryFile> {
  return serviceClient.request<MarkdownMemoryFile>("service_read_markdown_memory_file", {
    request: memoryProjectId ? { scope, memoryProjectId } : { scope },
  });
}

// replaceMarkdownMemoryFile replaces one file only if it still holds the text the editor loaded.
// replaceMarkdownMemoryFile 仅当文件仍是编辑器加载时的文本才替换它。
export async function replaceMarkdownMemoryFile(request: {
  scope: MarkdownMemoryScope;
  memoryProjectId: string | null;
  expected: string;
  text: string;
}): Promise<MarkdownMemoryFile> {
  const { memoryProjectId, ...rest } = request;
  return serviceClient.request<MarkdownMemoryFile>("service_replace_markdown_memory_file", {
    request: memoryProjectId ? { ...rest, memoryProjectId } : rest,
  });
}

// applyMarkdownMemoryProjectAction links, creates, ignores, toggles or renames a memory project.
// applyMarkdownMemoryProjectAction 关联、新建、忽略、开关或重命名记忆项目。
export async function applyMarkdownMemoryProjectAction(
  action: MarkdownMemoryProjectAction,
): Promise<MarkdownMemoryProject[]> {
  return serviceClient.request<MarkdownMemoryProject[]>("service_apply_markdown_memory_project_action", {
    request: action,
  });
}

// markdownMemoryTargetRequest drops empty selections; the service DTO rejects unknown fields only.
// markdownMemoryTargetRequest 去除空选择；服务端 DTO 只拒绝未知字段。
function markdownMemoryTargetRequest(target: MarkdownMemoryTarget): MarkdownMemoryTarget {
  return {
    ...(target.projectId ? { projectId: target.projectId } : {}),
    ...(target.memoryProjectId ? { memoryProjectId: target.memoryProjectId } : {}),
  };
}
