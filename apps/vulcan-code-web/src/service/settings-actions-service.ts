import type {
  ProviderModelSecondaryRouteSelection,
  ProviderModelSecondaryRouteSummary,
  RtkGainStatsResponse,
  WorkbenchSnapshot,
} from "@/types";
import {
  hasServiceRuntime,
  serviceClient,
} from "@/service/webui-service-runtime";

// RTK_GAIN_STATS_REQUEST_TIMEOUT_MS bounds the settings-page statistics request so the UI cannot stay loading forever.
// RTK_GAIN_STATS_REQUEST_TIMEOUT_MS 限定设置页统计请求耗时，避免界面永久停留在读取状态。
const RTK_GAIN_STATS_REQUEST_TIMEOUT_MS = 8000;

// WorkbenchModelChoice carries the provider and model selected by the composer model picker.
// WorkbenchModelChoice 承载输入区模型选择器选中的 Provider 与模型。
export interface WorkbenchModelChoice {
  // providerId identifies the selected provider configuration.
  // providerId 标识选中的 Provider 配置。
  providerId: string;
  // modelId identifies the selected provider model.
  // modelId 标识选中的 Provider 模型。
  modelId: string;
}

// selectWorkbenchModel persists the active provider/model pair through the desktop service.
// selectWorkbenchModel 通过桌面服务持久化当前激活的 Provider/模型组合。
export async function selectWorkbenchModel(choice: WorkbenchModelChoice): Promise<WorkbenchSnapshot | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<WorkbenchSnapshot>("service_select_workbench_model", {
    providerId: choice.providerId,
    modelId: choice.modelId,
  });
}

// listWorkbenchModelSecondaryRoutes loads secondary route choices for one provider/model pair.
// listWorkbenchModelSecondaryRoutes 加载单个 Provider/模型组合的二级路由选项。
export async function listWorkbenchModelSecondaryRoutes(
  providerId: string,
  modelId: string,
): Promise<ProviderModelSecondaryRouteSummary[]> {
  if (!hasServiceRuntime()) {
    return [];
  }
  return serviceClient.request<ProviderModelSecondaryRouteSummary[]>("service_list_workbench_model_secondary_routes", {
    providerId,
    modelId,
  });
}

// selectWorkbenchModelSecondaryRoute persists one secondary route or clears it back to Auto.
// selectWorkbenchModelSecondaryRoute 持久化单个二级路由，或清除回 Auto。
export async function selectWorkbenchModelSecondaryRoute(
  providerId: string,
  modelId: string,
  route: ProviderModelSecondaryRouteSelection | null,
): Promise<WorkbenchSnapshot | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<WorkbenchSnapshot>("service_select_workbench_model_secondary_route", {
    request: {
      providerId,
      modelId,
      route,
    },
  });
}

// selectWorkbenchReasoningEffort persists one reasoning selection through the desktop service router.
// selectWorkbenchReasoningEffort 通过桌面服务路由持久化单个思考档位选择。
export async function selectWorkbenchReasoningEffort(effort: string): Promise<WorkbenchSnapshot | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<WorkbenchSnapshot>("service_select_workbench_reasoning_effort", { effort });
}

// selectWorkbenchThinkingDisplayMode persists one thinking display preference through the desktop service router.
// selectWorkbenchThinkingDisplayMode 通过桌面服务路由持久化单个思考展示偏好。
export async function selectWorkbenchThinkingDisplayMode(mode: string): Promise<WorkbenchSnapshot | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<WorkbenchSnapshot>("service_select_workbench_thinking_display_mode", { mode });
}

// selectWebuiLocale persists one WebUI locale through the desktop service router.
// selectWebuiLocale 通过桌面服务路由持久化单个 WebUI 语言。
export async function selectWebuiLocale(locale: string): Promise<WorkbenchSnapshot | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<WorkbenchSnapshot>("service_select_webui_locale", { locale });
}

// saveProviderConfiguration persists one provider form through the desktop service.
// saveProviderConfiguration 通过桌面服务持久化单个 Provider 表单。
export async function saveProviderConfiguration(request: { providerId: string; fields: Record<string, string> }): Promise<WorkbenchSnapshot | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<WorkbenchSnapshot>("service_save_provider_configuration", { request });
}

// removeProviderConfiguration removes one provider through the desktop service.
// removeProviderConfiguration 通过桌面服务移除单个 Provider。
export async function removeProviderConfiguration(providerId: string): Promise<WorkbenchSnapshot | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<WorkbenchSnapshot>("service_remove_provider_configuration", { providerId });
}

// saveProviderEnabledModels persists one provider enabled-model list.
// saveProviderEnabledModels 持久化单个 Provider 的启用模型列表。
export async function saveProviderEnabledModels(request: { providerId: string; enabledModelIds: string[] }): Promise<WorkbenchSnapshot | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<WorkbenchSnapshot>("service_save_provider_enabled_models", { request });
}

// saveProviderModelOverride persists one provider model capability override.
// saveProviderModelOverride 持久化单个 Provider 模型能力覆盖。
export async function saveProviderModelOverride(request: {
  providerId: string;
  modelId: string;
  reasoning: boolean | null;
  attachment: boolean | null;
  contextLimit: number | null;
  inputLimit: number | null;
  outputLimit: number | null;
  inputModalities: string[] | null;
  outputModalities: string[] | null;
  reasoningProfileJson: string | null;
}): Promise<WorkbenchSnapshot | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<WorkbenchSnapshot>("service_save_provider_model_override", { request });
}

// saveGitCommitSettings persists Git commit settings through the desktop service.
// saveGitCommitSettings 通过桌面服务持久化 Git 提交设置。
export async function saveGitCommitSettings(request: {
  newBranchPrefix: string;
  summaryModel: { mode: "session" } | { mode: "fixed"; providerId: string; modelId: string };
  summaryExtraPrompt: string;
  useRtkSummary: boolean;
}): Promise<WorkbenchSnapshot | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<WorkbenchSnapshot>("service_save_git_commit_settings", { request });
}

// saveRtkRoutingSettings persists RTK command-routing settings through the desktop service.
// saveRtkRoutingSettings 通过桌面服务持久化 RTK 命令路由设置。
export async function saveRtkRoutingSettings(request: {
  enabled: boolean;
  routeMode: "disabled" | "selected" | "all";
  enabledRoutes: string[];
  disabledRoutes: string[];
  telemetryDisabled: boolean;
}): Promise<WorkbenchSnapshot | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<WorkbenchSnapshot>("service_save_rtk_routing_settings", { request });
}

// getRtkGainStats loads cached-display RTK token-savings statistics.
// getRtkGainStats 加载用于缓存展示的 RTK Token 节省统计。
export async function getRtkGainStats(): Promise<RtkGainStatsResponse | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  // controller aborts the browser-service RPC when the statistics bridge does not answer in time.
  // controller 在统计桥接未及时返回时中止浏览器服务 RPC。
  const controller = new AbortController();
  // timeoutId stores the timeout handle so a completed request can release it promptly.
  // timeoutId 保存超时句柄，便于请求完成后及时释放。
  const timeoutId = globalThis.setTimeout(() => {
    controller.abort();
  }, RTK_GAIN_STATS_REQUEST_TIMEOUT_MS);
  try {
    return await serviceClient.request<RtkGainStatsResponse>("service_get_rtk_gain_stats", {}, {
      signal: controller.signal,
    });
  } catch (error) {
    if (controller.signal.aborted) {
      throw new Error("RTK gain statistics request timed out. Please retry or check the local service.");
    }
    throw error;
  } finally {
    globalThis.clearTimeout(timeoutId);
  }
}

// saveAiCapabilityVendor persists one shared AI capability vendor.
// saveAiCapabilityVendor 持久化单个共享 AI 能力供应商。
export async function saveAiCapabilityVendor(request: {
  providerId?: never;
  vendorId: string;
  enabled: boolean;
  apiKey: string;
  baseUrl: string;
  region: string;
  connectionMode?: string | null;
}): Promise<WorkbenchSnapshot | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<WorkbenchSnapshot>("service_save_ai_capability_vendor", { request });
}

// removeAiCapabilityVendor removes one shared AI capability vendor.
// removeAiCapabilityVendor 移除单个共享 AI 能力供应商。
export async function removeAiCapabilityVendor(vendorId: string): Promise<WorkbenchSnapshot | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<WorkbenchSnapshot>("service_remove_ai_capability_vendor", { vendorId });
}

// setAiCapabilityTargetEnabled persists one AI capability target enabled state.
// setAiCapabilityTargetEnabled 持久化单个 AI 能力目标启用状态。
export async function setAiCapabilityTargetEnabled(
  request: { capabilityKind: string; vendorId: string; targetAlias: string; enabled: boolean },
): Promise<WorkbenchSnapshot | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<WorkbenchSnapshot>("service_set_ai_capability_target_enabled", { request });
}

// saveAiCapabilityTargets persists one ordered AI capability target list.
// saveAiCapabilityTargets 持久化单个有序 AI 能力目标列表。
export async function saveAiCapabilityTargets(request: {
  capabilityKind: string;
  targets: Array<{ vendorId: string; targetAlias: string; modelId: string; enabled: boolean }>;
}): Promise<WorkbenchSnapshot | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<WorkbenchSnapshot>("service_save_ai_capability_targets", { request });
}

// saveMcpServerConfiguration persists one MCP server through the desktop service.
// saveMcpServerConfiguration 通过桌面服务持久化单个 MCP 服务器。
export async function saveMcpServerConfiguration(request: Record<string, unknown>): Promise<WorkbenchSnapshot | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<WorkbenchSnapshot>("service_save_mcp_server_configuration", { request });
}

// removeMcpServerConfiguration removes one MCP server through the desktop service.
// removeMcpServerConfiguration 通过桌面服务移除单个 MCP 服务器。
export async function removeMcpServerConfiguration(serverId: string): Promise<WorkbenchSnapshot | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<WorkbenchSnapshot>("service_remove_mcp_server_configuration", { serverId });
}
