import { browserFallbackSnapshot, browserFallbackStartup } from "../../src/service/browser-fallback-workbench";
import { mediaDeliverySettingsFixture } from "../../src/test-fixtures/media-delivery";
import { createVceActivityFixture } from "../../src/components/features/code-context/vce-activity-test-fixture";
import { auditPluginCatalog } from "./plugin-fixtures";
import { auditMcpServers } from "./mcp-fixtures";
import type { KnowledgeLibrarySummary, PublicAiSettingsSnapshot, PublicAiProviderDescriptor, PublicAiConnection,
  PublicAiModel, StartupSnapshot, WorkbenchSnapshot, VmmNativeProjectCatalogSummary } from "../../src/types";

// AuditScenario selects synthetic data only; no scenario loads an installation, database or credential.
// AuditScenario 仅选择合成数据；任何场景均不加载安装目录、数据库或凭据。
export type AuditScenario = "empty" | "invalid-default" | "long-list";
// scenario is selected once from this isolated page URL before production components mount.
// scenario 在生产组件挂载前由此隔离页面网址一次性选择。
export const scenario: AuditScenario = typeof window !== "undefined" && new URLSearchParams(window.location.search).get("scenario") === "long-list"
  ? "long-list" : typeof window !== "undefined" && new URLSearchParams(window.location.search).get("scenario") === "invalid-default" ? "invalid-default" : "empty";

// createAuditWorkbench clones the existing fully typed fallback and replaces its identity with fictional audit data.
// createAuditWorkbench 克隆既有完整类型回退数据，并使用虚构验收数据替换身份。
// Parameters: selected controls density. Returns: detached production workbench contract.
// 参数：selected 控制密度。返回值：独立的生产工作台契约。
export function createAuditWorkbench(selected: AuditScenario): WorkbenchSnapshot {
  // value preserves all production-required fields already maintained by the fallback fixture.
  // value 保留回退夹具已经维护的全部生产必需字段。
  const value = browserFallbackSnapshot();
  value.revision = 1; value.uiLocale = "zh-CN"; value.appVersion = "UI-AUDIT · synthetic";
  value.projects = Array.from({ length: selected === "long-list" ? 36 : selected === "empty" ? 0 : 2 }, (_, index) => ({
    id: `audit-project-${index + 1}`, name: `验收项目 ${index + 1}${selected === "long-list" ? " · 长名称与嵌套目录布局验证" : ""}`,
    path: `X:/ui-audit/fictional-workspace/project-${index + 1}`, pinned: index === 0, vceEnabled: true,
  }));
  value.expandedProjectIds = value.projects.map((project) => project.id);
  value.conversations = []; value.archivedConversations = []; value.archivedProjects = []; value.selectedConversationId = null;
  value.commandSessions = []; value.toolCalls = []; value.pendingPrompts = []; value.sessionVmmPoliciesByConversation = [];
  value.providers = Array.from({ length: selected === "long-list" ? 24 : selected === "empty" ? 0 : 2 }, (_, index) => {
    // The chat picker consumes credential-scoped ProviderSummary rows, not the public AI provider directory.
    // 聊天选择器消费凭据作用域 ProviderSummary 行，而非公共 AI 供应商目录。
    const connection = auditConnection(index + 1);
    const provider = auditProvider(index + 1);
    return { id: connection.id, sourceProviderId: provider.id, connectionId: connection.id,
      accountName: connection.display_name, name: provider.display_name, providerDisplayName: provider.display_name, status: "ready", configured: true,
      models: Array.from({ length: selected === "long-list" ? 35 : 3 }, (_, modelIndex) => {
        const model = auditModel(modelIndex + 1);
        return { id: model.id, name: model.display_name, enabled: true, preferredConnection: true,
          inputModalities: model.capabilities.input_resources, routeRevision: "ui-audit-v1" };
      }),
    };
  });
  value.mcpTools = []; value.plugins = auditPluginCatalog();
  value.mcpServers = auditMcpServers(selected);
  value.vmmMemory.selectedProjectId = null; value.vmmMemory.selectedConversationId = null;
  value.vmmMemory.projectScopes = value.projects.map((project) => ({ projectId: project.id, name: project.name, path: project.path, hasOverride: false, memoryEnabled: selected !== "empty" }));
  value.vmmMemory.globalDraft.enabled = selected !== "empty";
  value.vmmMemory.projectDraft.enabled = selected !== "empty";
  value.vmmMemory.effective.enabled = selected !== "empty";
  value.vmmMemory.experienceState.deployment = selected === "empty" ? "disabled" : "native";
  value.vmmMemory.runtime.backendState = "native";
  value.vmmMemory.notice = "隔离验收夹具：所有记录为合成数据，未连接记忆服务。";
  if (selected === "empty") { value.agentSkills.records = []; value.subagentCatalog.definitions = []; value.luaSkills.skills = []; }
  if (selected === "long-list") {
    // Original typed records provide exact optional-field ownership for repeated long catalog rows.
    // 原有强类型记录为重复长目录行提供准确的可选字段归属。
    const agent = value.subagentCatalog.definitions[0];
    const skill = value.agentSkills.records[0];
    if (agent) value.subagentCatalog.definitions = Array.from({ length: 45 }, (_, index) => ({ ...structuredClone(agent), agentId: `audit-agent-${index + 1}`, name: `验收智能体 ${index + 1} · 长名称`, sourceKey: `audit-agent-${index + 1}`, definitionFile: `X:/ui-audit/agents/agent-${index + 1}.md` }));
    if (skill) value.agentSkills.records = Array.from({ length: 45 }, (_, index) => ({ ...structuredClone(skill), skillKey: `system:audit-skill-${index + 1}`, name: `audit-skill-${index + 1}`, skillFile: `X:/ui-audit/skills/audit-skill-${index + 1}/SKILL.md` }));
  }
  return value;
}

// startup reuses the production fallback startup contract without running bootstrap discovery.
// startup 复用生产回退启动契约，不运行启动发现。
export const startup: StartupSnapshot = { ...browserFallbackStartup, phase: "ui-audit", uiLocale: "zh-CN" };
// workbench is the sole in-memory workbench authority for the current isolated page.
// workbench 是当前隔离页面唯一的内存工作台权威。
export const workbench = createAuditWorkbench(scenario);

// auditProvider creates one protocol-declared fictional provider; reserved .invalid origins are never requested.
// auditProvider 创建一个声明协议的虚构供应商；保留域名 .invalid 绝不被请求。
// Parameters: index selects stable identity. Returns: the exact provider descriptor.
// 参数：index 选择稳定身份。返回值：精确供应商描述。
function auditProvider(index: number): PublicAiProviderDescriptor {
  return { id: `custom:audit-${index}`, display_name: `验收供应商 ${index} · 长名称服务入口`, kind: "custom", category: "unclassified",
    credential_directory_visibility: "configured_only", credential_registration: "direct", service_entries: [{
      id: "compatible-api", display_name: "隔离兼容入口", kind: "conversation", base_url: "https://audit.invalid/v1",
      credential_profiles: [{ id: "api-key", display_name: "合成凭据", credential_mode: "token", billing_kind: "pay_as_you_go", account_usage_query: "unsupported" }],
      authentication: "bearer_authorization", model_discovery: "manual",
      protocol_routes: [{ protocol_profile_id: "open_ai_chat", endpoint_path: "/chat/completions", verified: true }],
    }] };
}
// auditConnection follows the current exact credential-scoped connection contract with no real secret.
// auditConnection 遵循当前准确的凭据作用域连接契约，不含真实秘密。
// Parameters: index selects its provider. Returns: a synthetic local account descriptor.
// 参数：index 选择其供应商。返回值：合成本地账号描述。
function auditConnection(index: number): PublicAiConnection {
  return { id: `audit-account-${index}`, provider_id: `custom:audit-${index}`, service_entry_id: "compatible-api", credential_profile_id: "api-key",
    billing_kind: "pay_as_you_go", usage_capability: "unsupported", usage_requires_management_key: false, has_management_key: false,
    credential_mode: "token", credential_id: `synthetic-credential-${index}`, display_name: `验收账号 ${index}`, enabled: true,
    prompt_cache_by_protocol: {}, pending_validation: false };
}
// auditModel supplies declared capabilities and evidence for real model editors and catalog selectors.
// auditModel 提供已声明能力及证据，供真实模型编辑器和目录选择器使用。
// Parameters: index selects stable model identity. Returns: one typed model record.
// 参数：index 选择稳定模型身份。返回值：一条强类型模型记录。
function auditModel(index: number): PublicAiModel {
  return { id: `audit-chat-${index}`, display_name: `验收模型 ${index} · 长上下文多语言展示`, capabilities: {
    operations: ["conversation_respond"], context_limit: 128000, output_limit: 8192, input_resources: ["image"], output_resources: [],
    evidence_revision: "ui-audit-v1", reasoning: "native", reasoning_control: "effort_selection", reasoning_efforts: ["none", "low", "high"],
    prompt_cache: { provider_default_behavior: "disabled", supports_managed_explicit: false, supported_ttl_options: [] }, model_parameters: [],
  }, capability_evidence: { context_limit_declared: true, output_limit_declared: true, input_modalities_declared: true,
    output_modalities_declared: true, supports_text_input: true, supports_text_output: true } };
}

// createAuditPublicAi uses the smallest complete existing settings fixture and adds explicit invalid defaults when requested.
// createAuditPublicAi 使用既有最小完整设置夹具，并按需加入明确失效默认项。
// Parameters: selected controls catalog density. Returns: a fully typed detached settings snapshot.
// 参数：selected 控制目录密度。返回值：完整强类型独立设置快照。
export function createAuditPublicAi(selected: AuditScenario): PublicAiSettingsSnapshot {
  // value carries all required fields, avoiding unsafe partial casts in production page inputs.
  // value 携带全部必需字段，避免在生产页面输入中使用不安全的局部类型断言。
  const value: PublicAiSettingsSnapshot = {
    usageRefreshIntervalSeconds: 300, mediaDelivery: mediaDeliverySettingsFixture(), contextWindowPolicyOverrides: {},
    contextCompactionWindowNodes: { nodes: [
      { id: "compression-window-1", tokens: 200000 }, { id: "compression-window-2", tokens: 256000 },
      { id: "compression-window-3", tokens: 272000 }, { id: "compression-window-4", tokens: 373000 }, { id: "compression-window-5", tokens: 500000 },
    ] }, providerPriorities: {}, usageCache: {}, providers: [], connections: [], providerDefaults: {}, catalogs: {}, sourceCatalogs: {}, modelSupportCatalogs: {},
    catalogStatuses: {}, catalogErrors: {}, modelOverrides: {}, modelOverrideRecords: [], modelAccessRecords: [], providerToolTemplateModels: [],
    connectionModelAccessPolicyRecords: [], customModels: {}, customProviderSystemModelDefaults: { context_limit: 200000, output_limit: 32768,
      input_resources: [], reasoning: "native", reasoning_control: "effort_selection", reasoning_efforts: ["none", "minimal", "low", "medium", "high", "xhigh"] },
    providerToolTargets: [], providerToolSelections: {}, providerToolEnabledCapabilities: [], providerExtensionTargetSets: {},
    internalInferenceCandidates: [], inferenceProfiles: {}, inferenceRouteDiagnostics: [], internalModelOverrides: {}, modelRouteRecords: [],
  };
  for (let index = 1; index <= (selected === "long-list" ? 24 : selected === "empty" ? 0 : 2); index += 1) {
    // connection and models are synthetic records owned by this exact provider.
    // connection 与 models 是由此准确供应商持有的合成记录。
    const connection = auditConnection(index);
    const models = Array.from({ length: selected === "long-list" ? 35 : 3 }, (_, modelIndex) => auditModel(modelIndex + 1));
    value.providers.push(auditProvider(index)); value.connections.push(connection);
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
  if (selected !== "empty") {
    // Extra accounts share one exact provider/model while retaining protocol and effective-access differences.
    // 额外账号共享一个精确供应商模型，同时保留协议及有效访问差异。
    for (const variant of ["backup", "responses", "restricted"] as const) {
      const connection = { ...auditConnection(1), id: `audit-account-1-${variant}`, credential_id: `synthetic-credential-1-${variant}`,
        display_name: `验收账号 1 · ${variant === "backup" ? "备用账号" : variant === "responses" ? "Responses 协议账号" : "模型 1 停用账号"}` };
      const supportModels = [auditModel(1)];
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
    const model = auditModel(1);
    const selection = { provider_id: "custom:audit-1", connection_id: "audit-account-1", model_id: model.id };
    const providerSelection = { connection_id: "audit-account-1", target_id: "audit-image-understand" };
    value.multimodalUnderstandingCandidateModels = [{ providerId: selection.provider_id, modelId: selection.model_id }];
    value.mediaUnderstandingModelCandidates = [{ providerDisplayName: auditProvider(1).display_name, serviceEntryDisplayName: "隔离兼容入口",
      connectionDisplayName: auditConnection(1).display_name, modelDisplayName: model.display_name, selection, catalogGeneration: 1, capabilities: model.capabilities }];
    value.providerToolTargets = [{ connectionId: providerSelection.connection_id, connectionDisplayName: auditConnection(1).display_name,
      manifestSourceLayer: "bundled", target: { target_id: providerSelection.target_id, provider_id: selection.provider_id,
        service_entry_id: "compatible-api", credential_profile_ids: ["api-key"], upstream_target_id: "audit-image-understand", display_name: "隔离图像理解专用目标",
        contract: { tool_id: "image_understand", description: "Understand an image in the synthetic audit fixture.", capability: "image_understand",
          input_schema: { type: "object" }, output: { kind: "resources", source_path: "$.data", normalized_result_path: "$.resources" } } } }];
    value.mediaUnderstandingSelections = { image_understand: { mode: "model_replacement", selection: { ...selection, model_id: "deleted-default-model" } } };
    value.mediaUnderstandingBackendOptions = { image_understand: { provider_tool: providerSelection, model_replacement: { selection } } };
    value.providerToolEnabledCapabilities = ["image_understand"];
  }
  return value;
}
// publicAi is detached synthetic authority for all settings pages in the same audit scenario.
// publicAi 是同一验收场景下全部设置页面使用的独立合成权威。
export const publicAi = createAuditPublicAi(scenario);

// knowledgeLibraries uses real disabled model-policy fields and no document bodies or file IO.
// knowledgeLibraries 使用真实禁用模型策略字段，不包含文档正文或文件输入输出。
export const knowledgeLibraries: KnowledgeLibrarySummary[] = Array.from({ length: scenario === "long-list" ? 32 : scenario === "empty" ? 0 : 2 }, (_, index) => ({
  id: `audit-library-${index + 1}`, name: `验收知识库 ${index + 1}`, description: "隔离合成资料库 · 未连接真实数据库",
  lifecycleState: "active", documentCount: 0, chunkCount: 0, indexStatus: "ready", createdAtMs: 1, updatedAtMs: 1,
  settings: { retrievalMode: "raw_bm25", fallbackPolicy: "explicit_only", curation: { enabled: false, promptVersion: "ui-audit" },
    displayIndex: { status: "not_created", generatedDocumentCount: 0 }, embedding: { enabled: false, embeddingBatchSize: 1, batchSizeSource: "fallback_default" },
    vectorIndex: { status: "disabled" } },
}));
// nativeCatalog is scoped by stable Vulcan project tags and fictional native owner IDs.
// nativeCatalog 按稳定 Vulcan 项目标签及虚构原生归属 ID 定义。
export const nativeCatalog: VmmNativeProjectCatalogSummary = {
  projects: workbench.projects.map((project, index) => ({ projectId: index + 1, name: project.name, tag: project.id, protected: false, ownerVulcanProjectIds: [project.id], linkedVulcanProjectIds: [] })),
  bindings: workbench.projects.map((project, index) => ({ vulcanProjectId: project.id, ownerProjectId: index + 1, linkedProjectIds: [] })),
};
// auditActivity provides a known unavailable route while preserving the production lightweight project contract.
// auditActivity 在保留生产轻量项目契约的同时提供明确不可用路由。
// Parameters: projectId owns the response. Returns: synthetic read-only activity.
// 参数：projectId 拥有响应。返回值：合成只读活动。
export function auditActivity(projectId: string) {
  // activity starts from the complete fixture maintained alongside production VCE types.
  // activity 从与生产 VCE 类型共同维护的完整夹具开始。
  const activity = createVceActivityFixture(projectId);
  activity.queryAvailable = false; activity.configuration.ready = false; activity.configuration.issues = ["隔离验收：未连接模型路由"];
  activity.status.presentFiles = 0; activity.status.checkpointBlobs = 0;
  return activity;
}
