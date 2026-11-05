import type { PublicAiConnection, PublicAiModel, PublicAiProviderDescriptor, PublicAiSettingsSnapshot } from "@/types";
import { mediaDeliverySettingsFixture } from "./media-delivery";

// fixtureProvider creates one protocol-declared fictional provider; reserved .invalid origins are never requested.
// fixtureProvider 创建一个声明协议的虚构供应商；保留域名 .invalid 绝不被请求。
// Parameters: index selects stable identity. Returns: the exact provider descriptor.
// 参数：index 选择稳定身份。返回值：精确供应商描述。
function fixtureProvider(index: number): PublicAiProviderDescriptor {
  return { id: `custom:audit-${index}`, display_name: `验收供应商 ${index} · 长名称服务入口`, kind: "custom", category: "unclassified",
    credential_directory_visibility: "configured_only", credential_registration: "direct", service_entries: [{
      id: "compatible-api", display_name: "隔离兼容入口", kind: "conversation", base_url: "https://audit.invalid/v1",
      credential_profiles: [{ id: "api-key", display_name: "合成凭据", credential_mode: "token", billing_kind: "pay_as_you_go", account_usage_query: "unsupported" }],
      authentication: "bearer_authorization", model_discovery: "manual",
      protocol_routes: [{ protocol_profile_id: "open_ai_chat", endpoint_path: "/chat/completions", verified: true }],
    }] };
}

// fixtureConnection follows the current exact credential-scoped connection contract with no real secret.
// fixtureConnection 遵循当前准确的凭据作用域连接契约，不含真实秘密。
// Parameters: index selects its provider. Returns: a synthetic local account descriptor.
// 参数：index 选择其供应商。返回值：合成本地账号描述。
function fixtureConnection(index: number): PublicAiConnection {
  return { id: `audit-account-${index}`, provider_id: `custom:audit-${index}`, service_entry_id: "compatible-api", credential_profile_id: "api-key",
    billing_kind: "pay_as_you_go", usage_capability: "unsupported", usage_requires_management_key: false, has_management_key: false,
    credential_mode: "token", credential_id: `synthetic-credential-${index}`, display_name: `验收账号 ${index}`, enabled: true,
    pending_validation: false };
}

// fixtureModel supplies declared capabilities and evidence for real model editors and catalog selectors.
// fixtureModel 提供已声明能力及证据，供真实模型编辑器和目录选择器使用。
// Parameters: index selects stable model identity. Returns: one typed model record.
// 参数：index 选择稳定模型身份。返回值：一条强类型模型记录。
function fixtureModel(index: number): PublicAiModel {
  return { id: `audit-chat-${index}`, display_name: `验收模型 ${index} · 长上下文多语言展示`, capabilities: {
    operations: ["conversation_respond"], context_limit: 128000, output_limit: 8192, input_resources: ["image"], output_resources: [],
    evidence_revision: "ui-audit-v1", reasoning: "native", reasoning_control: "effort_selection", reasoning_efforts: ["none", "low", "high"],
    prompt_cache: { provider_default_behavior: "disabled", supports_managed_explicit: false, supported_ttl_options: [] }, model_parameters: [],
  }, capability_evidence: { context_limit_declared: true, output_limit_declared: true, input_modalities_declared: true,
    output_modalities_declared: true, supports_text_input: true, supports_text_output: true } };
}

// publicAiSettingsWithInvalidDefaultFixture returns two configured providers whose active default model no longer exists.
// publicAiSettingsWithInvalidDefaultFixture 返回两个已配置供应商，其当前默认模型已不存在。
// Account 1 also has backup, Responses-protocol and restricted variants so access differences stay observable.
// 账号 1 另有备用、Responses 协议及受限变体，使访问差异保持可观测。
// Parameters: none. Returns: a fully typed detached settings snapshot.
// 参数：无。返回值：完整强类型独立设置快照。
export function publicAiSettingsWithInvalidDefaultFixture(): PublicAiSettingsSnapshot {
  // value carries all required fields, avoiding unsafe partial casts in production page inputs.
  // value 携带全部必需字段，避免在生产页面输入中使用不安全的局部类型断言。
  const value: PublicAiSettingsSnapshot = {
    usageRefreshIntervalSeconds: 300, mediaDelivery: mediaDeliverySettingsFixture(), contextWindowPolicyOverrides: {}, contextWindowPolicyDefaults: { effective_context_window_percent: 95, auto_compact_percent: 90, auto_compact_token_limit: null, auto_compact_scope: "total", fallback_buffer_tokens: 0 },
    manualModelDefaults: { contextLimit: 200000, outputLimit: 32768 }, newSessionContextCompactionWindowNodeId: null, modelContextWindowDefaults: [], modelContextWindowCandidates: [], customContextWindowMinTokens: 100000,
    contextCompactionWindowNodes: { nodes: [
      { id: "compression-window-1", tokens: 200000 }, { id: "compression-window-2", tokens: 256000 },
      { id: "compression-window-3", tokens: 272000 }, { id: "compression-window-4", tokens: 373000 }, { id: "compression-window-5", tokens: 500000 },
    ] }, providerPriorities: {}, usageCache: {}, providers: [], connections: [], providerDefaults: {}, catalogs: {}, sourceCatalogs: {}, modelSupportCatalogs: {},
    catalogStatuses: {}, catalogErrors: {}, modelAccessRecords: [], providerToolTemplateModels: [],
    connectionModelAccessPolicyRecords: [], customModels: {}, customProviderSystemModelDefaults: { context_limit: 200000, output_limit: 32768,
      input_resources: [], reasoning: "native", reasoning_control: "effort_selection", reasoning_efforts: ["none", "minimal", "low", "medium", "high", "xhigh"] },
    providerToolTargets: [], providerToolSelections: {}, providerToolEnabledCapabilities: [], providerExtensionModes: {}, providerExtensionTargetSets: {},
    internalInferenceCandidates: [], inferenceProfiles: {}, inferenceRouteDiagnostics: [], internalModelOverrides: {}, modelRouteRecords: [],
  };
  for (let index = 1; index <= 2; index += 1) {
    // connection and models are synthetic records owned by this exact provider.
    // connection 与 models 是由此准确供应商持有的合成记录。
    const connection = fixtureConnection(index);
    const models = Array.from({ length: 3 }, (_, modelIndex) => fixtureModel(modelIndex + 1));
    value.providers.push(fixtureProvider(index)); value.connections.push(connection);
    value.catalogs[connection.id] = { connection_id: connection.id, models, generation: 1, page_count: 1, discovery_schema_revision: "ui-audit-v1" };
    value.sourceCatalogs[connection.id] = structuredClone(value.catalogs[connection.id]);
    value.modelSupportCatalogs[connection.id] = structuredClone(value.catalogs[connection.id]);
    value.catalogStatuses[connection.id] = { state: "ready", model_count: models.length, generation: 1, observed_at_ms: 1 };
    // Model management consumes resolved route records in addition to the discovered/support catalogs.
    // 模型管理除发现与支持目录外，还消费已解析路由记录。
    value.modelRouteRecords!.push(...models.map((model) => ({ providerId: connection.provider_id, connectionId: connection.id,
      serviceEntryId: connection.service_entry_id, modelId: model.id, protocolId: "openai-chat-completions.v1",
      availableProtocols: ["openai-chat-completions.v1", "openai-responses.v1"], bindingSource: "custom_user" as const, manifestRevision: 1, executable: true })));
    value.modelAccessRecords.push(...models.map((model) => ({ providerId: connection.provider_id, modelId: model.id, enabled: true, defaultEnabled: true })));
  }
  // Extra accounts share one exact provider/model while retaining protocol and effective-access differences.
  // 额外账号共享一个精确供应商模型，同时保留协议及有效访问差异。
  for (const variant of ["backup", "responses", "restricted"] as const) {
    const connection = { ...fixtureConnection(1), id: `audit-account-1-${variant}`, credential_id: `synthetic-credential-1-${variant}`,
      display_name: `验收账号 1 · ${variant === "backup" ? "备用账号" : variant === "responses" ? "Responses 协议账号" : "模型 1 停用账号"}` };
    const supportModels = [fixtureModel(1)];
    const effectiveModels = variant === "restricted" ? [] : supportModels;
    value.connections.push(connection);
    value.catalogs[connection.id] = { connection_id: connection.id, models: effectiveModels, generation: 1, page_count: 1, discovery_schema_revision: "ui-audit-v1" };
    value.sourceCatalogs[connection.id] = { ...structuredClone(value.catalogs[connection.id]), models: structuredClone(supportModels) };
    value.modelSupportCatalogs[connection.id] = structuredClone(value.sourceCatalogs[connection.id]);
    value.catalogStatuses[connection.id] = { state: "ready", model_count: effectiveModels.length, generation: 1, observed_at_ms: 1 };
    value.modelRouteRecords!.push({ providerId: connection.provider_id, connectionId: connection.id, serviceEntryId: connection.service_entry_id,
      modelId: "audit-chat-1", protocolId: variant === "responses" ? "openai-responses.v1" : "openai-chat-completions.v1",
      availableProtocols: ["openai-chat-completions.v1", "openai-responses.v1"], bindingSource: "custom_user", manifestRevision: 1, executable: true });
  }
  value.active = { provider_id: "custom:audit-1", connection_id: "audit-account-1", model_id: "deleted-default-model" };
  value.providerDefaults["custom:audit-1"] = "deleted-default-model";
  // Only image understanding has admitted synthetic routes; video/audio deliberately keep empty alternatives.
  // 仅图像理解有准入合成路由；视频与音频有意保留空替代项。
  const model = fixtureModel(1);
  const selection = { provider_id: "custom:audit-1", connection_id: "audit-account-1", model_id: model.id };
  const providerSelection = { connection_id: "audit-account-1", target_id: "audit-image-understand" };
  value.multimodalUnderstandingCandidateModels = [{ providerId: selection.provider_id, modelId: selection.model_id }];
  value.mediaUnderstandingModelCandidates = [{ providerDisplayName: fixtureProvider(1).display_name, serviceEntryDisplayName: "隔离兼容入口",
    connectionDisplayName: fixtureConnection(1).display_name, modelDisplayName: model.display_name, selection, catalogGeneration: 1, capabilities: model.capabilities }];
  value.providerToolTargets = [{ connectionId: providerSelection.connection_id, connectionDisplayName: fixtureConnection(1).display_name,
    manifestSourceLayer: "bundled", target: { target_id: providerSelection.target_id, provider_id: selection.provider_id,
      service_entry_id: "compatible-api", credential_profile_ids: ["api-key"], upstream_target_id: "audit-image-understand", display_name: "隔离图像理解专用目标",
      contract: { tool_id: "image_understand", description: "Understand an image in the synthetic audit fixture.", capability: "image_understand",
        input_schema: { type: "object" }, output: { kind: "resources", source_path: "$.data", normalized_result_path: "$.resources" } } } }];
  value.mediaUnderstandingSelections = { image_understand: { mode: "model_replacement", selection: { ...selection, model_id: "deleted-default-model" } } };
  value.mediaUnderstandingBackendOptions = { image_understand: { provider_tool: providerSelection, model_replacement: { selection } } };
  value.providerToolEnabledCapabilities = ["image_understand"];
  return value;
}
