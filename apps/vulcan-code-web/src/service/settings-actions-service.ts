import type {
  ContextCompactionWindowNodeId,
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
  PublicAiModelCandidateDraft,
  PublicAiModelCandidatesRecord,
  PublicAiCustomModelValidationResult,
  PublicAiCustomModelProtocolPreference,
  PublicAiModelCapabilityOverrides,
  PublicAiContextWindowPolicyOverrides,
  PublicAiMediaDeliveryPolicy,
  PublicAiContextCompactionWindowConfiguration,
  PublicAiInferenceRoute,
  PublicAiInferenceTestResult,
  PublicAiReasoningEffort,
  PublicAiInternalModelOverrides,
  PublicAiOperation,
  PublicAiProviderToolCapabilityKind,
  PublicAiMediaUnderstandingBackendSelection,
  PublicAiProviderExtensionMode,
  PublicAiProviderExtensionModeSelection,
  PublicAiProviderToolTestResult,
  PublicAiProviderToolTargetSelection,
  PublicAiSessionProviderToolSettings,
  PublicAiOAuthChallenge,
  PublicAiOAuthCommitRequest,
  PublicAiOAuthStartRequest,
  PublicAiOAuthTransaction,
  ProviderTaskSummary,
  PublicAiSettingsSnapshot,
  ProviderManifestStatus,
  ProviderManifestCandidatePreview,
  ProviderRollbackReport,
  ProviderSyncReport,
  ProviderModelSecondaryRouteSelection,
  ProviderModelSecondaryRouteSummary,
  OutputStyleCatalogSummary,
  SystemPromptScope,
  SystemPromptSettingsSnapshot,
  WorkbenchSnapshot,
} from "@/types";
import {
  hasServiceRuntime,
  serviceClient,
} from "@/service/webui-service-runtime";
import { loadWorkbenchSnapshot } from "@/service/workbench-snapshot-service";
import type { AnswerMode } from "@/service/answer-mode";

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

// savePromptAnswerMode persists the global answer mode copied into conversations created afterwards.
// savePromptAnswerMode 持久化之后新建会话所复制的全局回答模式。
export async function savePromptAnswerMode(mode: AnswerMode): Promise<AnswerMode | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<AnswerMode>("service_save_prompt_answer_mode", { request: { mode } });
}

// loadSystemPromptSettings reads the built-in, user, and optional project base system prompt layers.
// loadSystemPromptSettings 读取内置、用户与可选项目的基础系统提示词层级。
export async function loadSystemPromptSettings(projectId: string | null): Promise<SystemPromptSettingsSnapshot> {
  return serviceClient.request<SystemPromptSettingsSnapshot>("service_system_prompt_settings", {
    projectId,
  });
}

// saveSystemPrompt persists one user or project override; the next model request uses it.
// saveSystemPrompt 持久化一个用户或项目覆盖；下一次模型请求即使用它。
export async function saveSystemPrompt(
  scope: SystemPromptScope,
  projectId: string | null,
  text: string,
): Promise<SystemPromptSettingsSnapshot> {
  return serviceClient.request<SystemPromptSettingsSnapshot>("service_save_system_prompt", {
    request: { scope, projectId, text },
  });
}

// resetSystemPrompt removes one user or project override so the next lower layer takes effect.
// resetSystemPrompt 移除一个用户或项目覆盖，使下一较低层级生效。
export async function resetSystemPrompt(
  scope: SystemPromptScope,
  projectId: string | null,
): Promise<SystemPromptSettingsSnapshot> {
  return serviceClient.request<SystemPromptSettingsSnapshot>("service_reset_system_prompt", {
    request: { scope, projectId },
  });
}

// sameTurnPublicAiSettingsRead is the snapshot read started in the current event-loop turn, if any.
// sameTurnPublicAiSettingsRead 是当前事件循环回合内已发起的快照读取（若有）。
let sameTurnPublicAiSettingsRead: Promise<PublicAiSettingsSnapshot> | null = null;

// loadPublicAiSettings loads persisted connections and memory-only catalogs. Readers mounting together (the
// composer controls of a newly opened session) share one request; a read issued in a later turn, such as the
// reload after a save, always starts a fresh request.
// loadPublicAiSettings 加载持久化连接与仅内存目录。同时挂载的读取方（新打开会话的输入区控件）共用一次请求；
// 之后回合发起的读取（例如保存后的重新加载）总是发起新请求。
export function loadPublicAiSettings(): Promise<PublicAiSettingsSnapshot> {
  if (sameTurnPublicAiSettingsRead) return sameTurnPublicAiSettingsRead;
  const read = serviceClient.request<PublicAiSettingsSnapshot>("service_public_ai_settings_snapshot");
  sameTurnPublicAiSettingsRead = read;
  setTimeout(() => {
    if (sameTurnPublicAiSettingsRead === read) sameTurnPublicAiSettingsRead = null;
  }, 0);
  return read;
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

// savePublicAiConnectionEnabled reversibly includes or excludes one stored credential from runtime work.
// savePublicAiConnectionEnabled 以可逆方式让一个已保存凭据参与或退出运行时工作。
export async function savePublicAiConnectionEnabled(request: {
  connectionId: string;
  enabled: boolean;
}): Promise<PublicAiSettingsSnapshot> {
  return serviceClient.request<PublicAiSettingsSnapshot>(
    "service_save_public_ai_connection_enabled",
    { request },
  );
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

// listPublicAiModelCandidates returns fetched models of one system account that are not added yet.
// listPublicAiModelCandidates 返回一个系统账号中已拉取但尚未添加的模型。
export async function listPublicAiModelCandidates(
  connectionId: string,
): Promise<PublicAiModelCandidatesRecord> {
  return serviceClient.request<PublicAiModelCandidatesRecord>(
    "service_list_public_ai_model_candidates",
    { request: { connectionId } },
  );
}

// preparePublicAiModelCandidate returns a prefilled definition; saving uses savePublicAiCustomModels.
// preparePublicAiModelCandidate 返回预填定义；保存走 savePublicAiCustomModels。
export async function preparePublicAiModelCandidate(
  connectionId: string,
  modelId: string,
): Promise<PublicAiModelCandidateDraft> {
  return serviceClient.request<PublicAiModelCandidateDraft>(
    "service_prepare_public_ai_model_candidate",
    { request: { connectionId, modelId } },
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

// savePublicAiProviderPriority persists one exact provider ordering value.
// savePublicAiProviderPriority 持久化一个精确供应商排序值。
export async function savePublicAiProviderPriority(
  providerId: string,
  priority: number,
): Promise<PublicAiSettingsSnapshot> {
  return serviceClient.request<PublicAiSettingsSnapshot>(
    "service_save_public_ai_provider_priority",
    { request: { providerId, priority } },
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

// savePublicAiContextCompactionWindowNodes atomically persists all five fixed node values.
// savePublicAiContextCompactionWindowNodes 原子持久化全部五个固定节点值。
export async function savePublicAiContextCompactionWindowNodes(
  nodes: PublicAiContextCompactionWindowConfiguration,
): Promise<PublicAiSettingsSnapshot> {
  return serviceClient.request<PublicAiSettingsSnapshot>(
    "service_save_public_ai_context_compaction_window_nodes",
    { request: { nodes } },
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

// savePublicAiModelEnabled persists one provider-model conversation choice.
// savePublicAiModelEnabled 持久化一个供应商模型会话选择。
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

// savePublicAiModelSetEnabled saves one conversation choice for several models of one provider atomically.
// savePublicAiModelSetEnabled 原子地为同一供应商的多个模型保存一项会话选择。
export async function savePublicAiModelSetEnabled(request: {
  providerId: string;
  modelIds: string[];
  enabled: boolean;
}): Promise<PublicAiSettingsSnapshot> {
  return serviceClient.request<PublicAiSettingsSnapshot>(
    "service_save_public_ai_model_set_enabled",
    { request },
  );
}

// savePublicAiInternalModelSetEnabled saves one internal-use choice for several models of one provider atomically.
// savePublicAiInternalModelSetEnabled 原子地为同一供应商的多个模型保存一项内部使用选择。
export async function savePublicAiInternalModelSetEnabled(request: {
  providerId: string;
  modelIds: string[];
  enabled: boolean;
}): Promise<PublicAiSettingsSnapshot> {
  return serviceClient.request<PublicAiSettingsSnapshot>(
    "service_save_public_ai_internal_model_set_enabled",
    { request },
  );
}

// savePublicAiConnectionModelSetEnabled saves one access choice for several models of one account atomically.
// savePublicAiConnectionModelSetEnabled 原子地为一个账号的多个模型保存一项访问选择。
export async function savePublicAiConnectionModelSetEnabled(request: {
  connectionId: string;
  modelIds: string[];
  enabled: boolean;
}): Promise<PublicAiSettingsSnapshot> {
  return serviceClient.request<PublicAiSettingsSnapshot>(
    "service_save_public_ai_connection_model_set_enabled",
    { request },
  );
}

// savePublicAiInternalModelEnabled persists one model-level non-conversation internal-use switch.
// savePublicAiInternalModelEnabled 持久化一个模型级非会话内部使用开关。
export async function savePublicAiInternalModelEnabled(request: {
  providerId: string;
  modelId: string;
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
  reasoningEffort?: PublicAiReasoningEffort;
}): Promise<PublicAiSettingsSnapshot> {
  return serviceClient.request<PublicAiSettingsSnapshot>(
    "service_save_public_ai_inference_route",
    { request },
  );
}

// testPublicAiInferenceRoute executes one real embedding, rerank or decision request through its
// saved route.
// testPublicAiInferenceRoute 通过已保存路由执行一次真实向量、重排或判定请求。
export async function testPublicAiInferenceRoute(request: {
  consumerProfileId: string;
  operation: "embedding_create" | "rerank_documents" | "decision_evaluate";
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

// savePublicAiMultimodalUnderstandingCandidate replaces one shared provider-model admission switch.
// savePublicAiMultimodalUnderstandingCandidate 替换一个共享供应商模型准入开关。
export async function savePublicAiMultimodalUnderstandingCandidate(request: {
  providerId: string;
  modelId: string;
  enabled: boolean;
}): Promise<PublicAiSettingsSnapshot> {
  return serviceClient.request<PublicAiSettingsSnapshot>(
    "service_save_public_ai_multimodal_understanding_candidate",
    { request },
  );
}

// savePublicAiMediaUnderstandingSelection replaces one image, video, or audio understanding backend.
// savePublicAiMediaUnderstandingSelection 替换一项图片、视频或音频理解后端。
export async function savePublicAiMediaUnderstandingSelection(request: {
  capability: PublicAiProviderToolCapabilityKind;
  selection: PublicAiMediaUnderstandingBackendSelection | null;
}): Promise<PublicAiSettingsSnapshot> {
  return serviceClient.request<PublicAiSettingsSnapshot>(
    "service_save_public_ai_media_understanding_selection",
    { request },
  );
}

// savePublicAiProviderExtensionTargetSet replaces one complete native extension-model set.
// savePublicAiProviderExtensionTargetSet 替换一项完整原生扩展模型集合。
export async function savePublicAiProviderExtensionTargetSet(request: {
  capability: PublicAiProviderToolCapabilityKind;
  selections: PublicAiProviderToolTargetSelection[];
}): Promise<PublicAiSettingsSnapshot> {
  return serviceClient.request<PublicAiSettingsSnapshot>(
    "service_save_public_ai_provider_extension_target_set",
    { request },
  );
}

// savePublicAiProviderExtensionMode sets one capability's system default mode and fixed model.
// savePublicAiProviderExtensionMode 设置一项能力的系统默认模式及固定模型。
// Single mode without a model is the default and clears the stored entry; multi-model lists are kept.
// 未选模型的单模型模式即默认值并清除已存条目；多模型列表保持不变。
export async function savePublicAiProviderExtensionMode(request: {
  capability: PublicAiProviderToolCapabilityKind;
  selection: PublicAiProviderExtensionModeSelection;
}): Promise<PublicAiSettingsSnapshot> {
  return serviceClient.request<PublicAiSettingsSnapshot>(
    "service_save_public_ai_provider_extension_mode",
    { request },
  );
}

// savePublicAiProviderExtensionChoice writes one model-extension choice into the store of the given mode:
// single mode keeps only the latest selection as the fixed model, multi mode replaces the whole set.
// savePublicAiProviderExtensionChoice 将一次模型扩展选择写入指定模式的存储：
// 单模型模式只把最近一次选择保留为固定模型，多模型模式替换整个集合。
export async function savePublicAiProviderExtensionChoice(request: {
  capability: PublicAiProviderToolCapabilityKind;
  mode: PublicAiProviderExtensionMode;
  selections: PublicAiProviderToolTargetSelection[];
}): Promise<PublicAiSettingsSnapshot> {
  const latest = request.selections[request.selections.length - 1];
  return request.mode === "single"
    ? savePublicAiProviderExtensionMode({
        capability: request.capability,
        selection: { mode: "single", ...(latest ? { single_target: latest } : {}) },
      })
    : savePublicAiProviderExtensionTargetSet({ capability: request.capability, selections: request.selections });
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

// listProviderTasks reads every retained provider asynchronous task, newest first.
// listProviderTasks 读取全部保留的供应商异步任务，最新的在前。
export async function listProviderTasks(): Promise<ProviderTaskSummary[]> {
  return serviceClient.request<ProviderTaskSummary[]>("service_list_provider_tasks", {});
}

// cancelProviderTask asks the provider to cancel one task by its stable identity.
// cancelProviderTask 按稳定标识请求供应商取消一个任务。
export async function cancelProviderTask(task: {
  executionId: string;
  // ownerSessionId repeats the owner shown in the list so a stale view cannot cancel a task that changed hands.
  // ownerSessionId 重复列表中显示的所有者，使过期视图无法取消已易主的任务。
  ownerSessionId: string | null;
}): Promise<ProviderTaskSummary> {
  return serviceClient.request<ProviderTaskSummary>("service_cancel_provider_task", {
    executionId: task.executionId,
    ownerSessionId: task.ownerSessionId,
  });
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

// PublicAiSessionProviderToolSettingsWire admits the sole older response shape that predates exact media routes.
// PublicAiSessionProviderToolSettingsWire 接纳唯一一种早于精确媒体路由的旧响应结构。
type PublicAiSessionProviderToolSettingsWire = Omit<
  PublicAiSessionProviderToolSettings,
  "mediaUnderstandingSelections"
> & {
  // The field is absent only when an older service process answers a newer WebUI.
  // 仅当旧版服务进程响应新版 WebUI 时该字段才会缺失。
  mediaUnderstandingSelections?: Partial<
    Record<PublicAiProviderToolCapabilityKind, PublicAiMediaUnderstandingBackendSelection>
  >;
};

// normalizePublicAiSessionProviderToolSettings upgrades one versioned service response at the transport boundary.
// normalizePublicAiSessionProviderToolSettings 在传输边界升级一个版本化服务响应。
// The input is the exact wire payload; the return value always owns a complete media-route map.
// 输入为精确线路负载；返回值始终持有完整媒体路由映射。
function normalizePublicAiSessionProviderToolSettings(
  settings: PublicAiSessionProviderToolSettingsWire,
): PublicAiSessionProviderToolSettings {
  return {
    ...settings,
    mediaUnderstandingSelections: settings.mediaUnderstandingSelections ?? {},
  };
}

// savePublicAiNewSessionContextWindowDefault persists the default for future conversations.
// savePublicAiNewSessionContextWindowDefault 持久化未来新建会话的默认窗口。
export async function savePublicAiNewSessionContextWindowDefault(
  nodeId: ContextCompactionWindowNodeId | null,
): Promise<PublicAiSettingsSnapshot> {
  return serviceClient.request<PublicAiSettingsSnapshot>(
    "service_save_public_ai_new_session_context_window_default",
    { request: { nodeId } },
  );
}

// loadPublicAiSessionProviderToolSelections loads direct and extension targets for one session.
// loadPublicAiSessionProviderToolSelections 加载单个会话生效的直连与扩展目标。
export async function loadPublicAiSessionProviderToolSelections(
  conversationId: string,
): Promise<PublicAiSessionProviderToolSettings> {
  // settings is normalized before any component can observe the versioned wire shape.
  // settings 在任何组件观察到版本化线路结构前完成规范化。
  const settings = await serviceClient.request<PublicAiSessionProviderToolSettingsWire>(
    "service_public_ai_session_provider_tool_selections",
    { conversationId },
  );
  return normalizePublicAiSessionProviderToolSettings(settings);
}

// Saves complete `policy` unchanged and returns the backend's published settings snapshot.
// 原样保存完整 `policy`，返回后端已发布的设置快照。
// expectedRevision is the editor's last read fingerprint; stale writes fail without overwriting newer values.
// expectedRevision 是编辑器最后读取的指纹；陈旧写入失败，不覆盖较新值。
export async function savePublicAiMediaDeliveryPolicy(
  policy: PublicAiMediaDeliveryPolicy,
  expectedRevision: string,
): Promise<PublicAiSettingsSnapshot> {
  return serviceClient.request<PublicAiSettingsSnapshot>(
    "service_save_public_ai_media_delivery_policy",
    { request: { policy, expected_revision: expectedRevision } },
  );
}

// Explicitly restores backend defaults and returns their snapshot without inventing browser defaults.
// 显式恢复后端默认值并返回其快照，不在浏览器中自造默认值。
// expectedRevision binds the reset to the values the user saw; errors leave backend settings unchanged.
// expectedRevision 把重置绑定到用户看到的值；出错时后端设置保持不变。
export async function resetPublicAiMediaDeliveryPolicy(expectedRevision: string): Promise<PublicAiSettingsSnapshot> {
  return serviceClient.request<PublicAiSettingsSnapshot>(
    "service_reset_public_ai_media_delivery_policy",
    { request: { expected_revision: expectedRevision } },
  );
}

// Rebuilds capability settings for conversationId to global defaults and returns the effective snapshot.
// 将 conversationId 的能力设置重建为全局默认值，并返回生效快照。
// Only an explicit user action may call this; history, goals and other configuration are not reset.
// 仅显式用户操作可以调用；不重置历史、目标及其他配置。
export async function rebuildPublicAiSessionProviderToolSelections(
  conversationId: string,
): Promise<PublicAiSessionProviderToolSettings> {
  // settings follows the same response-version boundary as the ordinary load action.
  // settings 与普通加载动作遵循相同响应版本边界。
  const settings = await serviceClient.request<PublicAiSessionProviderToolSettingsWire>(
    "service_rebuild_public_ai_session_provider_tool_selections",
    { conversationId },
  );
  return normalizePublicAiSessionProviderToolSettings(settings);
}

// savePublicAiSessionProviderToolSelections replaces all session-level direct and extension targets.
// savePublicAiSessionProviderToolSelections 替换全部会话级直连与扩展目标。
export async function savePublicAiSessionProviderToolSelections(request: {
  conversationId: string;
  enabledCapabilities: PublicAiProviderToolCapabilityKind[];
  selections: Partial<
    Record<PublicAiProviderToolCapabilityKind, PublicAiProviderToolTargetSelection>
  >;
  extensionTargetSets: Partial<
    Record<PublicAiProviderToolCapabilityKind, PublicAiProviderToolTargetSelection[]>
  >;
  extensionModes: Partial<
    Record<PublicAiProviderToolCapabilityKind, PublicAiProviderExtensionModeSelection>
  >;
  mediaUnderstandingSelections: Partial<
    Record<PublicAiProviderToolCapabilityKind, PublicAiMediaUnderstandingBackendSelection>
  >;
}): Promise<PublicAiSessionProviderToolSettings> {
  // settings may still come from an older process during a development hot reload.
  // settings 在开发热重载期间仍可能来自旧版进程。
  const settings = await serviceClient.request<PublicAiSessionProviderToolSettingsWire>(
    "service_save_public_ai_session_provider_tool_selections",
    { request },
  );
  return normalizePublicAiSessionProviderToolSettings(settings);
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

// Saves the explicit system default; enabled controls folding, and the returned snapshot is authoritative.
// 保存明确的系统默认值；enabled 控制折叠，返回的快照具有权威性。
export async function saveToolCollapseDefault(enabled: boolean): Promise<WorkbenchSnapshot> {
  return serviceClient.request<WorkbenchSnapshot>("service_save_tool_collapse_default", { enabled });
}

// selectWebuiLocale persists one WebUI locale through the desktop service router.
// selectWebuiLocale 通过桌面服务路由持久化单个 WebUI 语言。
export async function selectWebuiLocale(locale: string): Promise<WorkbenchSnapshot | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<WorkbenchSnapshot>("service_select_webui_locale", { locale });
}

// saveDefaultCommandShell persists the shell shared by the integrated terminal and the command tools; null picks automatically.
// saveDefaultCommandShell 持久化集成终端与命令工具共用的 Shell；null 表示自动选择。
export async function saveDefaultCommandShell(shellId: string | null): Promise<WorkbenchSnapshot | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<WorkbenchSnapshot>("service_save_default_command_shell", { request: { shellId } });
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

// Persists full recording and optionally independent session-error recording; omitted errorsEnabled preserves its state.
// 持久化完整记录及可选的独立会话错误记录；省略 errorsEnabled 会保留原状态。
// Returns the refreshed workbench, or null when no service runtime is available.
// 返回刷新后的工作台；没有服务运行时时返回 null。
export async function saveProviderRequestTraceSettings(enabled: boolean, errorsEnabled?: boolean): Promise<WorkbenchSnapshot | null> {
  if (!hasServiceRuntime()) {
    return null;
  }
  return serviceClient.request<WorkbenchSnapshot>("service_save_provider_request_trace_settings", {
    request: { enabled, errorsEnabled },
  });
}

// openSelectedProviderApiTraceDirectory opens the exact selected session trace folder.
// openSelectedProviderApiTraceDirectory 打开当前选中会话的精确追踪目录。
export async function openSelectedProviderApiTraceDirectory(): Promise<string> {
  return serviceClient.request<string>("service_open_selected_provider_api_trace_directory");
}

// clearSelectedProviderApiTraceDirectory removes only the exact selected session trace folder.
// clearSelectedProviderApiTraceDirectory 仅移除当前选中会话的精确追踪目录。
export async function clearSelectedProviderApiTraceDirectory(): Promise<boolean> {
  return serviceClient.request<boolean>("service_clear_selected_provider_api_trace_directory");
}

// saveMcpServerConfiguration persists one MCP server through the desktop service.
// saveMcpServerConfiguration 通过桌面服务持久化单个 MCP 服务器。
export async function saveMcpServerConfiguration(request: Record<string, unknown>): Promise<WorkbenchSnapshot | null> {
  if (!hasServiceRuntime()) {
    throw new Error("MCP runtime is unavailable; the server configuration was not saved.");
  }
  return serviceClient.request<WorkbenchSnapshot>("service_save_mcp_server_configuration", { request });
}

// removeMcpServerConfiguration removes one MCP server through the desktop service.
// removeMcpServerConfiguration 通过桌面服务移除单个 MCP 服务器。
export async function removeMcpServerConfiguration(serverId: string): Promise<WorkbenchSnapshot | null> {
  if (!hasServiceRuntime()) {
    throw new Error("MCP runtime is unavailable; the server configuration was not removed.");
  }
  return serviceClient.request<WorkbenchSnapshot>("service_remove_mcp_server_configuration", { serverId });
}
