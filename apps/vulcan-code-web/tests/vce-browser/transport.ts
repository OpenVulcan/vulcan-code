import type { PublicAiInternalInferenceCandidate, PublicAiOperation, PublicAiSettingsSnapshot,
  VceGlobalConfigurationStatus } from "../../src/types";

// Mutable fixture scenario is controlled only by the isolated page, never persisted or sent over a network.
// 可变夹具场景仅由隔离页面控制，绝不持久化或发送网络请求。
let diagnostics = false;
// Deliberately unbroken provider detail exercises actual browser min-content and wrapping behavior.
// 故意不含断点的供应商详情用于验证真实浏览器最小内容宽度及换行行为。
const diagnostic = `VCE_BROWSER_DIAGNOSTIC_${"very_long_provider_model_route_".repeat(12)}`;

// Select whether subsequent reads expose long diagnostics; accepts the desired state and returns nothing.
// 选择后续读取是否暴露长诊断；接收目标状态，无返回值。
export function setDiagnosticScenario(enabled: boolean): void { diagnostics = enabled; }

// Build one exact operation-specific fixture candidate; returns shared production selector data.
// 构建一个精确操作专属夹具候选；返回通用生产选择器数据。
function candidate(operation: PublicAiOperation, account: string): PublicAiInternalInferenceCandidate {
  return { providerDisplayName: "浏览器验证供应商 Browser Provider", serviceEntryDisplayName: "Isolated Service",
    connectionDisplayName: `验证账号 ${account}`, modelDisplayName: `LongModel-${operation}-2026-extended-context-multilingual-version`,
    route: { provider_id: "browser-fixture", service_entry_id: "isolated", connection_id: account,
      model_id: `model-${operation}`, operation, usage_scope: "internal_background",
      binding: operation === "conversation_respond" ? { kind: "protocol", protocol_profile_id: "fixture-chat" }
        : { kind: "action", action_binding_id: `fixture-${operation}` } },
    catalogGeneration: 1, enabled: true, usageScopes: ["internal_background"], overrides: {},
    capabilities: { operations: [operation], structured_output: "json_object", reasoning: "native",
      reasoning_control: "toggle", reasoning_efforts: ["none", "adaptive"], evidence_revision: "isolated-browser-test",
      prompt_cache: { provider_default_behavior: "disabled", supports_managed_explicit: false, supported_ttl_options: [] },
      model_parameters: [],
      embedding: operation === "embedding_create" ? { tasks: ["provider_default"], output_kinds: ["dense"],
        encodings: ["float"], dimensions: [512, 1024, 2048], normalization: "l2", distance_metric: "cosine" } : undefined,
      rerank: operation === "rerank_documents" ? { supports_return_content: false, score_semantics: "relevance",
        truncation: ["provider_default"] } : undefined,
    } };
}

// Only these two snapshot fields are consumed by the production ContextEngine page under test.
// 被测生产语义引擎页面仅消费这两个快照字段。
const candidates = [candidate("embedding_create", "Alpha"), candidate("embedding_create", "Beta"),
  candidate("conversation_respond", "Alpha"), candidate("rerank_documents", "Alpha")];

// Return the exact page-consumed snapshot slice, without claiming to simulate a whole backend settings response.
// 返回页面消费的精确快照切片，不声称模拟完整后端设置响应。
export async function loadPublicAiSettings(): Promise<Pick<PublicAiSettingsSnapshot, "inferenceProfiles" | "internalInferenceCandidates">> {
  return { internalInferenceCandidates: candidates, inferenceProfiles: { vce: { embedding: candidates[0]!.route,
    embedding_dimensions: 1024, llm_routes: { "code_retrieval.query_rewrite": candidates[2]!.route },
    rerank: candidates[3]!.route } } };
}

// Return a bounded fixture readiness state; takes no input and never inspects user configuration.
// 返回有界夹具就绪状态；无输入，绝不检查用户配置。
export async function loadVceConfigurationStatus(): Promise<VceGlobalConfigurationStatus> {
  const capability = { configured: true, available: !diagnostics, issue: diagnostics ? diagnostic : undefined };
  return { ready: !diagnostics, consumerProfileId: "vce", queryRewritePurposeId: "code_retrieval.query_rewrite",
    reasoningPolicy: "fixed_off", embedding: capability, queryRewrite: capability, rerank: capability,
    issues: diagnostics ? [diagnostic] : [] };
}

// Reject every mutation explicitly: this layout fixture must never resemble a successful real settings save.
// 显式拒绝所有修改：此布局夹具绝不伪装成真实设置保存成功。
export async function savePublicAiInferenceRoute(): Promise<never> { throw new Error(diagnostic); }
// Reject paid model tests in this fixture; no parameters are dispatched and no results are fabricated.
// 在此夹具中拒绝付费模型测试；不派发参数，也不伪造结果。
export async function testPublicAiInferenceRoute(): Promise<never> { throw new Error(diagnostic); }
// Reject model compatibility preflight; this surface is for browser layout and selector interactions only.
// 拒绝模型兼容预检；此界面仅用于浏览器布局与选择器交互。
export async function preflightVceEmbeddingRoute(): Promise<never> { throw new Error(diagnostic); }
