import type {
  PublicAiAccountUsage,
  PublicAiConnectionAddRequest,
  PublicAiConnectionUpdateRequest,
  PublicAiCustomProviderSaveRequest,
  PublicAiCustomProviderProbeResult,
  PublicAiCustomProviderModelDefaults,
  PublicAiCustomModelTemplateDecision,
  PublicAiCustomModelTemplateDecisionBatchItem,
  PublicAiCustomModelTemplateSuggestionsRecord,
  PublicAiCommonModelTemplateSuggestion,
  PublicAiCustomModelDefinition,
  PublicAiCustomModelValidationResult,
  PublicAiCustomModelProtocolPreference,
  PublicAiModelCapabilityOverrides,
  PublicAiContextWindowPolicyOverrides,
  PublicAiModelOverrides,
  PublicAiInferenceRoute,
  PublicAiInferenceTestResult,
  PublicAiInternalModelOverrides,
  PublicAiOperation,
  PublicAiProviderToolCapabilityKind,
  PublicAiProviderToolTestResult,
  PublicAiProviderToolTargetSelection,
  PublicAiSessionProviderToolSettings,
  PublicAiOAuthChallenge,
  PublicAiOAuthCommitRequest,
  PublicAiOAuthStartRequest,
  PublicAiOAuthTransaction,
  PublicAiSettingsSnapshot,
  ProviderManifestStatus,
  ProviderManifestCandidatePreview,
  ProviderRollbackReport,
  ProviderSyncReport,
  ProviderUsageDashboard,
  ProviderModelSecondaryRouteSelection,
  ProviderModelSecondaryRouteSummary,
  OutputStyleCatalogSummary,
  WorkbenchSnapshot,
} from "@/types";
import {
  hasServiceRuntime,
  serviceClient,
} from "@/service/webui-service-runtime";
import { loadWorkbenchSnapshot } from "@/service/workbench-snapshot-service";

// loadOutputStyleCatalog reads the packaged prompt-style catalog and persisted global selection.
// loadOutputStyleCatalog 读取打包的提示词风格目录与已持久化全局选择。
export async function loadOutputStyleCatalog(): Promise<OutputStyleCatalogSummary | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<OutputStyleCatalogSummary>("service_output_style_catalog");
}

// savePromptOutputStyle persists the global default for future sessions and compaction boundaries.
// savePromptOutputStyle 持久化供未来会话与压缩边界使用的全局默认风格。
export async function savePromptOutputStyle(styleId: string): Promise<OutputStyleCatalogSummary | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<OutputStyleCatalogSummary>("service_save_prompt_output_style", {
    request: { styleId },
  });
}

// loadPublicAiSettings loads persisted connections and memory-only catalogs.
// loadPublicAiSettings 加载持久化连接与仅内存目录。
export async function loadPublicAiSettings(): Promise<PublicAiSettingsSnapshot> {
  return serviceClient.request<PublicAiSettingsSnapshot>("service_public_ai_settings_snapshot");
}

// loadProviderUsageDashboard loads native VLDB-backed usage, cost, and account facts.
// loadProviderUsageDashboard 加载由原生 VLDB 支撑的用量、成本与账户事实。
export async function loadProviderUsageDashboard(): Promise<ProviderUsageDashboard> {
  return serviceClient.request<ProviderUsageDashboard>("service_provider_usage_dashboard");
}

// importPublicAiCcSwitchPricing atomically replaces the lowest-precedence catalog from one exact cc-switch document.
// importPublicAiCcSwitchPricing 从一份精确 cc-switch 文档原子替换最低优先级价格目录。
export async function importPublicAiCcSwitchPricing(
  sourceId: string,
  json: string,
): Promise<number> {
  return serviceClient.request<number>("service_import_public_ai_cc_switch_pricing", {
    request: { sourceId, json },
  });
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

// probePublicAiCustomProvider validates one candidate without persisting secrets or provider state.
// probePublicAiCustomProvider 验证一个候选项且不持久化秘密或供应商状态。
export async function probePublicAiCustomProvider(
  request: PublicAiCustomProviderSaveRequest,
): Promise<PublicAiCustomProviderProbeResult> {
  return serviceClient.request<PublicAiCustomProviderProbeResult>(
    "service_probe_public_ai_custom_provider",
    { request },
  );
}

// validatePublicAiCustomModel executes one explicit potentially billable model request.
// validatePublicAiCustomModel 执行一次显式且可能计费的模型请求。
export async function validatePublicAiCustomModel(request: {
  providerId: string;
  connectionId: string;
  modelId: string;
}): Promise<PublicAiCustomModelValidationResult> {
  return serviceClient.request<PublicAiCustomModelValidationResult>(
    "service_validate_public_ai_custom_model",
    { request },
  );
}

// removePublicAiCustomProvider confirms cleanup for providerId and returns the refreshed settings snapshot.
// removePublicAiCustomProvider 确认清理 providerId，并返回刷新后的设置快照。
export async function removePublicAiCustomProvider(providerId: string): Promise<PublicAiSettingsSnapshot> {
  return serviceClient.request<PublicAiSettingsSnapshot>(
    "service_remove_public_ai_custom_provider",
    { request: { providerId, confirmCredentialDeletion: true } },
  );
}

// savePublicAiCustomModels replaces the complete credential-independent custom model set.
// savePublicAiCustomModels 替换完整的凭据无关自定义模型集合。
export async function savePublicAiCustomModels(
  providerId: string,
  models: PublicAiCustomModelDefinition[],
  templateDecisions: PublicAiCustomModelTemplateDecisionBatchItem[] = [],
): Promise<PublicAiSettingsSnapshot> {
  return serviceClient.request<PublicAiSettingsSnapshot>(
    "service_save_public_ai_custom_models",
    { request: { providerId, models, templateDecisions } },
  );
}

// savePublicAiCustomModelProtocolPreferences replaces the per-model protocol preferences for one provider.
// savePublicAiCustomModelProtocolPreferences 替换一个供应商的逐模型协议偏好。
export async function savePublicAiCustomModelProtocolPreferences(
  providerId: string,
  preferences: { modelId: string; preference: PublicAiCustomModelProtocolPreference }[],
): Promise<PublicAiSettingsSnapshot> {
  return serviceClient.request<PublicAiSettingsSnapshot>(
    "service_save_public_ai_custom_model_protocol_preferences",
    { request: { providerId, preferences } },
  );
}

// savePublicAiCustomProviderModelDefaults persists the safe provider-wide fallback contract.
// savePublicAiCustomProviderModelDefaults 持久化安全的供应商级回退契约。
export async function savePublicAiCustomProviderModelDefaults(request: {
  providerId: string;
  defaults?: PublicAiCustomProviderModelDefaults;
}): Promise<PublicAiSettingsSnapshot> {
  return serviceClient.request<PublicAiSettingsSnapshot>(
    "service_save_public_ai_custom_provider_model_defaults",
    { request },
  );
}

// getPublicAiCustomModelTemplateSuggestions returns local hints without network access.
// getPublicAiCustomModelTemplateSuggestions 在不访问网络的情况下返回本地提示。
export async function getPublicAiCustomModelTemplateSuggestions(
  modelId: string,
): Promise<PublicAiCommonModelTemplateSuggestion[]> {
  return serviceClient.request<PublicAiCommonModelTemplateSuggestion[]>(
    "service_public_ai_custom_model_template_suggestions",
    { request: { modelId } },
  );
}

// getPublicAiCustomModelTemplateSuggestionsBatch returns bounded local hints in one RPC.
// getPublicAiCustomModelTemplateSuggestionsBatch 通过一次 RPC 返回有界本地提示。
export async function getPublicAiCustomModelTemplateSuggestionsBatch(
  modelIds: string[],
): Promise<PublicAiCustomModelTemplateSuggestionsRecord[]> {
  return serviceClient.request<PublicAiCustomModelTemplateSuggestionsRecord[]>(
    "service_public_ai_custom_model_template_suggestions_batch",
    { request: { modelIds } },
  );
}

// savePublicAiCustomModelTemplateDecision persists or clears one reviewed template choice.
// savePublicAiCustomModelTemplateDecision 持久化或清除一项已审查模板选择。
export async function savePublicAiCustomModelTemplateDecision(request: {
  providerId: string;
  modelId: string;
  decision?: PublicAiCustomModelTemplateDecision;
}): Promise<PublicAiSettingsSnapshot> {
  return serviceClient.request<PublicAiSettingsSnapshot>(
    "service_save_public_ai_custom_model_template_decision",
    { request },
  );
}

// savePublicAiCustomModelTemplateDecisionsBatch atomically persists reviewed suggestions.
// savePublicAiCustomModelTemplateDecisionsBatch 原子持久化已审查模板建议。
export async function savePublicAiCustomModelTemplateDecisionsBatch(request: {
  providerId: string;
  decisions: PublicAiCustomModelTemplateDecisionBatchItem[];
}): Promise<PublicAiSettingsSnapshot> {
  return serviceClient.request<PublicAiSettingsSnapshot>(
    "service_save_public_ai_custom_model_template_decisions_batch",
    { request },
  );
}

// savePublicAiCustomModelConnectionAvailability controls one manual model on one account.
// savePublicAiCustomModelConnectionAvailability 控制一个手工模型在一个账号上的可用性。
export async function savePublicAiCustomModelConnectionAvailability(request: {
  connectionId: string;
  modelId: string;
  enabled?: boolean;
}): Promise<PublicAiSettingsSnapshot> {
  return serviceClient.request<PublicAiSettingsSnapshot>(
    "service_save_public_ai_custom_model_connection_availability",
    { request },
  );
}

// savePublicAiConnectionModelEnabled persists one exact account-model access decision.
// savePublicAiConnectionModelEnabled 持久化一项精确账号模型访问决定。
export async function savePublicAiConnectionModelEnabled(request: {
  connectionId: string;
  modelId: string;
  enabled: boolean;
}): Promise<PublicAiSettingsSnapshot> {
  return serviceClient.request<PublicAiSettingsSnapshot>(
    "service_save_public_ai_connection_model_enabled",
    { request },
  );
}

// savePublicAiConnectionModelsEnabled replaces one account's inherited model access state.
// savePublicAiConnectionModelsEnabled 替换一个账号继承的模型访问状态。
export async function savePublicAiConnectionModelsEnabled(request: {
  connectionId: string;
  enabled: boolean;
}): Promise<PublicAiSettingsSnapshot> {
  return serviceClient.request<PublicAiSettingsSnapshot>(
    "service_save_public_ai_connection_models_enabled",
    { request },
  );
}

// savePublicAiModelCapabilityOverrides saves one capability difference or restores defaults.
// savePublicAiModelCapabilityOverrides 保存一项能力差异或恢复默认值。
export async function savePublicAiModelCapabilityOverrides(request: {
  providerId: string;
  modelId: string;
  overrides?: PublicAiModelCapabilityOverrides;
}): Promise<PublicAiSettingsSnapshot> {
  return serviceClient.request<PublicAiSettingsSnapshot>(
    "service_save_public_ai_model_capability_overrides",
    { request },
  );
}

// savePublicAiProviderCapabilityOverrides saves one supplier-level difference or restores inheritance.
// savePublicAiProviderCapabilityOverrides 保存一项供应商级差异或恢复继承。
export async function savePublicAiProviderCapabilityOverrides(request: {
  providerId: string;
  overrides?: PublicAiModelCapabilityOverrides;
}): Promise<PublicAiSettingsSnapshot> {
  return serviceClient.request<PublicAiSettingsSnapshot>(
    "service_save_public_ai_provider_capability_overrides",
    { request },
  );
}

// syncPublicAiConnection refreshes one model catalog into memory.
// syncPublicAiConnection 将一个模型目录刷新到内存。
export async function syncPublicAiConnection(connectionId: string): Promise<PublicAiSettingsSnapshot> {
  return serviceClient.request<PublicAiSettingsSnapshot>("service_sync_public_ai_connection", { connectionId });
}

// getProviderManifestStatus returns the typed per-provider manifest status snapshot (plan §R10).
// getProviderManifestStatus 返回 typed 逐供应商清单状态快照（方案 §R10）。
export async function getProviderManifestStatus(): Promise<ProviderManifestStatus[]> {
  const statuses = await serviceClient.request<ProviderManifestStatus[]>(
    "service_get_provider_manifest_status",
    {},
  );
  if (!statuses || !Array.isArray(statuses)) {
    throw new Error("Provider manifest status response has an invalid contract.");
  }
  return statuses;
}

// previewProviderManifest verifies and diffs one signed candidate without mutating local state.
// previewProviderManifest 验证并对比一个签名候选，且不修改本地状态。
export async function previewProviderManifest(providerId: string): Promise<ProviderManifestCandidatePreview> {
  return serviceClient.request<ProviderManifestCandidatePreview>(
    "service_preview_provider_manifest",
    { providerId },
  );
}

// syncProviderManifest synchronizes exactly one provider through the reviewed production flow.
// syncProviderManifest 通过已审核生产流程同步精确一个供应商。
export async function syncProviderManifest(providerId: string): Promise<ProviderSyncReport> {
  return serviceClient.request<ProviderSyncReport>("service_sync_provider_manifest", { providerId });
}

// rollbackProviderManifest explicitly rolls one provider back to its previous layer.
// rollbackProviderManifest 显式把一个供应商回滚到 previous 层。
export async function rollbackProviderManifest(providerId: string): Promise<ProviderRollbackReport> {
  return serviceClient.request<ProviderRollbackReport>("service_rollback_provider_manifest", { providerId });
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

// savePublicAiContextWindowPolicyOverrides persists the system-level Codex window defaults.
// savePublicAiContextWindowPolicyOverrides 持久化系统级 Codex 窗口默认值。
export async function savePublicAiContextWindowPolicyOverrides(
  overrides: PublicAiContextWindowPolicyOverrides,
): Promise<PublicAiSettingsSnapshot> {
  return serviceClient.request<PublicAiSettingsSnapshot>(
    "service_save_public_ai_context_window_policy_overrides",
    { request: { overrides } },
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
  overrides?: PublicAiModelOverrides;
}): Promise<PublicAiSettingsSnapshot> {
  return serviceClient.request<PublicAiSettingsSnapshot>("service_save_public_ai_model_overrides", { request });
}

// savePublicAiModelEnabled persists one provider-model access choice.
// savePublicAiModelEnabled 持久化一个供应商模型访问选择。
export async function savePublicAiModelEnabled(request: {
  providerId: string;
  modelId: string;
  enabled: boolean;
  restore?: boolean;
}): Promise<PublicAiSettingsSnapshot> {
  return serviceClient.request<PublicAiSettingsSnapshot>(
    "service_save_public_ai_model_enabled",
    { request },
  );
}

// savePublicAiProviderModelsEnabled atomically applies one enabled state to every synchronized model of a provider.
// savePublicAiProviderModelsEnabled 原子地为一个供应商的全部已同步模型应用同一启用状态。
export async function savePublicAiProviderModelsEnabled(request: {
  providerId: string;
  enabled: boolean;
}): Promise<PublicAiSettingsSnapshot> {
  return serviceClient.request<PublicAiSettingsSnapshot>(
    "service_save_public_ai_provider_models_enabled",
    { request },
  );
}

// savePublicAiInternalModelEnabled persists one operation-scoped internal model switch.
// savePublicAiInternalModelEnabled 持久化一个操作作用域内部模型开关。
export async function savePublicAiInternalModelEnabled(request: {
  providerId: string;
  modelId: string;
  operation: PublicAiOperation;
  enabled: boolean;
}): Promise<PublicAiSettingsSnapshot> {
  return serviceClient.request<PublicAiSettingsSnapshot>(
    "service_save_public_ai_internal_model_enabled",
    { request },
  );
}

// savePublicAiInternalModelOverrides replaces one operation-scoped internal default set.
// savePublicAiInternalModelOverrides 替换一组操作作用域内部默认值。
export async function savePublicAiInternalModelOverrides(request: {
  providerId: string;
  modelId: string;
  operation: PublicAiOperation;
  overrides: PublicAiInternalModelOverrides;
}): Promise<PublicAiSettingsSnapshot> {
  return serviceClient.request<PublicAiSettingsSnapshot>(
    "service_save_public_ai_internal_model_overrides",
    { request },
  );
}

// savePublicAiInferenceRoute replaces one exact consumer-profile inference route.
// savePublicAiInferenceRoute 替换一条精确消费者配置档推理路由。
export async function savePublicAiInferenceRoute(request: {
  consumerProfileId: string;
  purposeId?: string;
  operation: PublicAiOperation;
  route: PublicAiInferenceRoute | null;
}): Promise<PublicAiSettingsSnapshot> {
  return serviceClient.request<PublicAiSettingsSnapshot>(
    "service_save_public_ai_inference_route",
    { request },
  );
}

// testPublicAiInferenceRoute executes one real embedding or rerank request through its saved route.
// testPublicAiInferenceRoute 通过已保存路由执行一次真实向量或重排请求。
export async function testPublicAiInferenceRoute(request: {
  consumerProfileId: string;
  operation: "embedding_create" | "rerank_documents";
}): Promise<PublicAiInferenceTestResult> {
  return serviceClient.request<PublicAiInferenceTestResult>(
    "service_test_public_ai_inference_route",
    { request },
  );
}

// savePublicAiProviderToolSelection replaces the sole target for one semantic capability.
// savePublicAiProviderToolSelection 替换一项语义能力唯一的目标。
export async function savePublicAiProviderToolSelection(request: {
  capability: PublicAiProviderToolCapabilityKind;
  selection: PublicAiProviderToolTargetSelection | null;
}): Promise<PublicAiSettingsSnapshot> {
  return serviceClient.request<PublicAiSettingsSnapshot>(
    "service_save_public_ai_provider_tool_selection",
    { request },
  );
}

// testPublicAiProviderTool executes one exact settings-row diagnostic without changing defaults.
// testPublicAiProviderTool 在不修改默认项的情况下执行一个精确设置行诊断。
export async function testPublicAiProviderTool(request: {
  // capability selects the closed search or extraction diagnostic.
  // capability 选择封闭的搜索或网页抓取诊断。
  capability: PublicAiProviderToolCapabilityKind;
  // selection freezes the exact credential connection and provider target.
  // selection 固定精确凭据连接与供应商目标。
  selection: PublicAiProviderToolTargetSelection;
  // arguments follow the selected target's declared provider-native schema.
  // arguments 遵循所选目标声明的供应商原生 Schema。
  arguments: Record<string, unknown>;
}): Promise<PublicAiProviderToolTestResult> {
  return serviceClient.request<PublicAiProviderToolTestResult>(
    "service_test_public_ai_provider_tool",
    { request },
  );
}

// savePublicAiProviderToolCapabilityEnabled changes one system-level capability switch.
// savePublicAiProviderToolCapabilityEnabled 修改一个系统级能力开关。
export async function savePublicAiProviderToolCapabilityEnabled(request: {
  capability: PublicAiProviderToolCapabilityKind;
  enabled: boolean;
}): Promise<PublicAiSettingsSnapshot> {
  return serviceClient.request<PublicAiSettingsSnapshot>(
    "service_save_public_ai_provider_tool_capability_enabled",
    { request },
  );
}

// loadPublicAiSessionProviderToolSelections loads the effective single-select targets for one session.
// loadPublicAiSessionProviderToolSelections 加载单个会话生效的单选目标。
export async function loadPublicAiSessionProviderToolSelections(
  conversationId: string,
): Promise<PublicAiSessionProviderToolSettings> {
  return serviceClient.request(
    "service_public_ai_session_provider_tool_selections",
    { conversationId },
  );
}

// savePublicAiSessionProviderToolSelections replaces all session-level single-select targets.
// savePublicAiSessionProviderToolSelections 替换全部会话级单选目标。
export async function savePublicAiSessionProviderToolSelections(request: {
  conversationId: string;
  enabledCapabilities: PublicAiProviderToolCapabilityKind[];
  selections: Partial<
    Record<PublicAiProviderToolCapabilityKind, PublicAiProviderToolTargetSelection>
  >;
}): Promise<PublicAiSessionProviderToolSettings> {
  return serviceClient.request(
    "service_save_public_ai_session_provider_tool_selections",
    { request },
  );
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

// selectWorkbenchReasoningEffort persists one reasoning selection for an exact materialized conversation.
// selectWorkbenchReasoningEffort 为一个精确的已物化会话持久化思考档位选择。
// Parameters: effort is the provider-neutral level and conversationId is the immutable session owner.
// 参数：effort 是供应商中立档位，conversationId 是不可变会话归属。
// Returns: the committed Workbench snapshot, or null when no desktop Service runtime exists.
// 返回值：已提交的 Workbench 快照；桌面 Service 运行时不存在时返回 null。
export async function selectWorkbenchReasoningEffort(
  effort: string,
  conversationId: string,
): Promise<WorkbenchSnapshot | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<WorkbenchSnapshot>("service_select_workbench_reasoning_effort", {
    conversationId,
    effort,
  });
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
