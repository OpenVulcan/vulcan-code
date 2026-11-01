import { serviceClient } from "@/service/webui-service-runtime";
import type { SessionSubagentModelsSnapshot, SubagentModelEntry, SubagentModelsSnapshot } from "@/types";

// loadSubagentModels reads the system default subagent model allowlist.
// loadSubagentModels 读取系统默认子代理模型白名单。
export async function loadSubagentModels(): Promise<SubagentModelsSnapshot> {
  return serviceClient.request<SubagentModelsSnapshot>("service_subagent_models", {});
}

// saveSubagentModels replaces the system default allowlist; sessions with their own list keep it.
// saveSubagentModels 替换系统默认白名单；拥有自有清单的会话保留其清单。
export async function saveSubagentModels(models: SubagentModelEntry[], includeSessionModel: boolean): Promise<SubagentModelsSnapshot> {
  return serviceClient.request<SubagentModelsSnapshot>("service_save_subagent_models", { request: { models, includeSessionModel } });
}

// loadSessionSubagentModels reads one session's own allowlist next to the system list.
// loadSessionSubagentModels 读取一个会话的自有白名单及系统清单。
export async function loadSessionSubagentModels(sessionId: string): Promise<SessionSubagentModelsSnapshot> {
  return serviceClient.request<SessionSubagentModelsSnapshot>("service_session_subagent_models", { sessionId });
}

// saveSessionSubagentModels saves one session's own list, or returns it to the system list with null.
// saveSessionSubagentModels 保存一个会话的自有清单；null 使其恢复跟随系统清单。
export async function saveSessionSubagentModels(
  sessionId: string,
  models: SubagentModelEntry[] | null,
  includeSessionModel: boolean | null,
): Promise<SessionSubagentModelsSnapshot> {
  return serviceClient.request<SessionSubagentModelsSnapshot>("service_save_session_subagent_models", {
    request: { sessionId, models, includeSessionModel },
  });
}
