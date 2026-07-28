import type {
  PublicAiAccountUsage,
  PublicAiConnectionAddRequest,
  PublicAiConnectionUpdateRequest,
  PublicAiCustomProviderSaveRequest,
  PublicAiModelOverrides,
  PublicAiOAuthChallenge,
  PublicAiOAuthCommitRequest,
  PublicAiOAuthStartRequest,
  PublicAiOAuthTransaction,
  PublicAiSettingsSnapshot,
  ProviderModelSecondaryRouteSelection,
  ProviderModelSecondaryRouteSummary,
  WorkbenchSnapshot,
} from "@/types";
import {
  hasServiceRuntime,
  serviceClient,
} from "@/service/webui-service-runtime";
import { loadWorkbenchSnapshot } from "@/service/workbench-snapshot-service";

// loadPublicAiSettings loads persisted connections and memory-only catalogs.
// loadPublicAiSettings 加载持久化连接与仅内存目录。
export async function loadPublicAiSettings(): Promise<PublicAiSettingsSnapshot> {
  return serviceClient.request<PublicAiSettingsSnapshot>("service_public_ai_settings_snapshot");
}

// startPublicAiOAuth starts one provider-declared browser authorization flow.
// startPublicAiOAuth 启动一个供应商声明的浏览器授权流程。
export async function startPublicAiOAuth(
  request: PublicAiOAuthStartRequest,
): Promise<PublicAiOAuthChallenge> {
  return serviceClient.request<PublicAiOAuthChallenge>("service_start_public_ai_oauth", { request });
}

// getPublicAiOAuthStatus reads one process-local authorization transaction.
// getPublicAiOAuthStatus 读取一个进程内授权事务。
export async function getPublicAiOAuthStatus(transactionId: string): Promise<PublicAiOAuthTransaction> {
  return serviceClient.request<PublicAiOAuthTransaction>("service_public_ai_oauth_status", {
    transactionId,
  });
}

// cancelPublicAiOAuth cancels one active browser authorization transaction.
// cancelPublicAiOAuth 取消一个活动浏览器授权事务。
export async function cancelPublicAiOAuth(transactionId: string): Promise<PublicAiOAuthTransaction> {
  return serviceClient.request<PublicAiOAuthTransaction>("service_cancel_public_ai_oauth", {
    transactionId,
  });
}

// commitPublicAiOAuthConnection persists one successful authorization as a connection.
// commitPublicAiOAuthConnection 将一个成功授权持久化为连接。
export async function commitPublicAiOAuthConnection(
  request: PublicAiOAuthCommitRequest,
): Promise<PublicAiSettingsSnapshot> {
  return serviceClient.request<PublicAiSettingsSnapshot>(
    "service_commit_public_ai_oauth_connection",
    { request },
  );
}

// addPublicAiConnection securely saves and synchronizes one credential connection.
// addPublicAiConnection 安全保存并同步一个凭据连接。
export async function addPublicAiConnection(
  request: PublicAiConnectionAddRequest,
): Promise<PublicAiSettingsSnapshot> {
  return serviceClient.request<PublicAiSettingsSnapshot>("service_add_public_ai_connection", { request });
}

// updatePublicAiConnection updates one credential-scoped route.
// updatePublicAiConnection 更新一个凭据作用域路由。
export async function updatePublicAiConnection(
  request: PublicAiConnectionUpdateRequest,
): Promise<PublicAiSettingsSnapshot> {
  return serviceClient.request<PublicAiSettingsSnapshot>("service_update_public_ai_connection", { request });
}

// removePublicAiConnection removes one credential and its secure secret.
// removePublicAiConnection 移除一个凭据及其安全秘密。
export async function removePublicAiConnection(request: {
  connectionId: string;
  replacementDefaultConnectionId?: string;
}): Promise<PublicAiSettingsSnapshot> {
  return serviceClient.request<PublicAiSettingsSnapshot>("service_remove_public_ai_connection", { request });
}

// savePublicAiCustomProvider creates or replaces one user-defined compatible provider.
// savePublicAiCustomProvider 创建或替换一个用户自定义兼容供应商。
export async function savePublicAiCustomProvider(
  request: PublicAiCustomProviderSaveRequest,
): Promise<PublicAiSettingsSnapshot> {
  return serviceClient.request<PublicAiSettingsSnapshot>(
    "service_save_public_ai_custom_provider",
    { request },
  );
}

// removePublicAiCustomProvider removes one unreferenced user-defined provider.
// removePublicAiCustomProvider 移除一个未被引用的用户自定义供应商。
export async function removePublicAiCustomProvider(
  providerId: string,
): Promise<PublicAiSettingsSnapshot> {
  return serviceClient.request<PublicAiSettingsSnapshot>(
    "service_remove_public_ai_custom_provider",
    { request: { providerId } },
  );
}

// syncPublicAiConnection refreshes one model catalog into memory.
// syncPublicAiConnection 将一个模型目录刷新到内存。
export async function syncPublicAiConnection(connectionId: string): Promise<PublicAiSettingsSnapshot> {
  return serviceClient.request<PublicAiSettingsSnapshot>("service_sync_public_ai_connection", { connectionId });
}

// queryPublicAiConnectionUsage queries one exact account without local persistence.
// queryPublicAiConnectionUsage 查询一个精确账号且不本地持久化。
export async function queryPublicAiConnectionUsage(connectionId: string): Promise<PublicAiAccountUsage> {
  const usage = await serviceClient.request<PublicAiAccountUsage>(
    "service_query_public_ai_connection_usage",
    { connectionId },
  );
  if (!usage || typeof usage !== "object" || !Array.isArray(usage.quota_windows)) {
    throw new Error("Public AI account usage response has an invalid quota_windows contract.");
  }
  return usage;
}

// savePublicAiUsageRefreshSettings persists the background usage refresh interval.
// savePublicAiUsageRefreshSettings 持久化后台用量刷新间隔。
export async function savePublicAiUsageRefreshSettings(intervalSeconds: number): Promise<PublicAiSettingsSnapshot> {
  return serviceClient.request<PublicAiSettingsSnapshot>(
    "service_save_public_ai_usage_refresh_settings",
    { request: { intervalSeconds } },
  );
}

// selectPublicAiModel persists one exact model and account selection.
// selectPublicAiModel 持久化一个精确模型与账号选择。
export async function selectPublicAiModel(request: {
  providerId: string;
  modelId: string;
  connectionId: string;
  // conversationId scopes the choice to one persisted session; null updates the new-session default.
  // conversationId 将选择限定到一个持久化会话；null 则更新新会话默认值。
  conversationId?: string | null;
}): Promise<PublicAiSettingsSnapshot> {
  return serviceClient.request<PublicAiSettingsSnapshot>("service_select_public_ai_model", { request });
}

// savePublicAiModelOverrides saves local model narrowing and registered parameters.
// savePublicAiModelOverrides 保存本地模型收窄与注册参数。
export async function savePublicAiModelOverrides(request: {
  providerId: string;
  modelId: string;
  protocolId: string;
  overrides: PublicAiModelOverrides;
}): Promise<PublicAiSettingsSnapshot> {
  return serviceClient.request<PublicAiSettingsSnapshot>("service_save_public_ai_model_overrides", { request });
}

// savePublicAiDefaultConnection changes the explicit initial account for one provider.
// savePublicAiDefaultConnection 修改一个供应商的显式初始账号。
export async function savePublicAiDefaultConnection(request: {
  providerId: string;
  connectionId: string;
}): Promise<PublicAiSettingsSnapshot> {
  return serviceClient.request<PublicAiSettingsSnapshot>("service_save_public_ai_default_connection", { request });
}

// WorkbenchModelChoice carries the provider and model selected by the composer model picker.
// WorkbenchModelChoice 承载输入区模型选择器选中的 Provider 与模型。
export interface WorkbenchModelChoice {
  // providerId identifies the selected provider configuration.
  // providerId 标识选中的 Provider 配置。
  providerId: string;
  // connectionId identifies the exact credential account selected for this model.
  // connectionId 标识为此模型选定的精确凭据账号。
  connectionId: string;
  // modelId identifies the selected provider model.
  // modelId 标识选中的 Provider 模型。
  modelId: string;
}

// selectWorkbenchModel persists one model account for a session or the new-session default.
// selectWorkbenchModel 为会话或新会话默认值持久化一个模型账号。
export async function selectWorkbenchModel(
  choice: WorkbenchModelChoice,
  conversationId: string | null,
): Promise<WorkbenchSnapshot | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  if (!choice.connectionId) {
    throw new Error("The selected model does not identify a public AI credential connection.");
  }
  await selectPublicAiModel({
    providerId: choice.providerId,
    modelId: choice.modelId,
    connectionId: choice.connectionId,
    conversationId,
  });
  return loadWorkbenchSnapshot();
}

// listWorkbenchModelSecondaryRoutes loads secondary route choices for one provider/model pair.
// listWorkbenchModelSecondaryRoutes 加载单个 Provider/模型组合的二级路由选项。
export async function listWorkbenchModelSecondaryRoutes(
  providerId: string,
  modelId: string,
  forceRefresh: boolean,
): Promise<ProviderModelSecondaryRouteSummary[]> {
  if (!hasServiceRuntime()) {
    return [];
  }
  return serviceClient.request<ProviderModelSecondaryRouteSummary[]>("service_list_workbench_model_secondary_routes", {
    providerId,
    modelId,
    forceRefresh,
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

// saveGitCommitSettings persists Git commit settings through the desktop service.
// saveGitCommitSettings 通过桌面服务持久化 Git 提交设置。
export async function saveGitCommitSettings(request: {
  newBranchPrefix: string;
  summaryModel: { mode: "session" } | { mode: "fixed"; providerId: string; modelId: string };
  summaryExtraPrompt: string;
}): Promise<WorkbenchSnapshot | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<WorkbenchSnapshot>("service_save_git_commit_settings", { request });
}

// saveProviderRetrySettings persists Provider retry rules through the desktop service.
// saveProviderRetrySettings 通过桌面服务持久化 Provider 重试规则。
export async function saveProviderRetrySettings(request: {
  enabled: boolean;
  maxRetries: number;
  initialDelayMs: number;
  maxDelayMs: number;
  backoffFactorMilli: number;
  jitterPercent: number;
  respectRetryAfter: boolean;
  retry429: boolean;
  rateLimit: {
    maxRetries: number;
    initialDelayMs: number;
    maxDelayMs: number;
    backoffFactorMilli: number;
  };
}): Promise<WorkbenchSnapshot | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<WorkbenchSnapshot>("service_save_provider_retry_settings", { request });
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
