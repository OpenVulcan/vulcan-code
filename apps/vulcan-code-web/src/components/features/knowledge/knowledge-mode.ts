import type { TranslationKey } from "@/i18n";
import type { KnowledgeIndexMode, KnowledgeLibrarySettingsRequest, KnowledgeRouteReferenceRequest } from "@/types/knowledge-base";
import type { PublicAiEmbeddingCapabilities, PublicAiInternalInferenceCandidate } from "@/types/public-ai";

// Each mode's label and cost explanation resolve only through the shared language catalogs.
// 每种模式的标签及成本说明仅通过共享语言目录解析。
export const KNOWLEDGE_MODE_COPY: Record<KnowledgeIndexMode, { label: TranslationKey; description: TranslationKey }> = {
  browse: { label: "knowledge.mode.browse", description: "knowledge.mode.browse.description" },
  bm25: { label: "knowledge.mode.bm25", description: "knowledge.mode.bm25.description" },
  vector: { label: "knowledge.mode.vector", description: "knowledge.mode.vector.description" },
  hybrid: { label: "knowledge.mode.hybrid", description: "knowledge.mode.hybrid.description" },
};

// A selected candidate has a declared embedding capability owned by its exact model.
// 已选择候选具有其精确模型拥有的声明嵌入能力。
export type KnowledgeEmbeddingCandidate = PublicAiInternalInferenceCandidate & {
  // Preserve all model capabilities while proving the embedding member exists.
  // 保留全部模型能力，同时证明嵌入成员存在。
  capabilities: PublicAiInternalInferenceCandidate["capabilities"] & { embedding: PublicAiEmbeddingCapabilities };
};

// Resolves enabled background candidates with explicit dense-float, task and metric support.
// 解析明确支持稠密浮点、任务及度量的已启用后台候选。
// Parameters: candidates is the authoritative settings snapshot. Returns admitted UI choices; the server revalidates on save.
// 参数：candidates 为权威设置快照。返回准入界面选项；服务端在保存时复核。
export function knowledgeEmbeddingCandidates(candidates: PublicAiInternalInferenceCandidate[]): KnowledgeEmbeddingCandidate[] {
  return candidates.filter((candidate): candidate is KnowledgeEmbeddingCandidate => {
    // This optional member is defined by the shared model capability contract.
    // 此可选成员由共享模型能力契约定义。
    const capability = candidate.capabilities.embedding;
    if (capability === undefined) return false;
    return candidate.enabled && candidate.route.operation === "embedding_create"
      && candidate.route.usage_scope === "internal_background"
      && capability.output_kinds.includes("dense") && capability.encodings.includes("float")
      && (capability.tasks.includes("query") || capability.tasks.includes("provider_default"))
      && (capability.tasks.includes("document") || capability.tasks.includes("provider_default"))
      && capability.maximum_inputs !== 0;
  });
}

// Projects the selected model's sole declared binding into the exact browser request shape.
// 将选定模型唯一声明绑定投影为精确浏览器请求形状。
// Parameters: candidate owns provider/entry/connection/model/binding IDs. Returns no host authorization fields.
// 参数：candidate 拥有供应商、入口、连接、模型及绑定身份。返回值不含宿主授权字段。
export function knowledgeRouteFromCandidate(candidate: PublicAiInternalInferenceCandidate): KnowledgeRouteReferenceRequest {
  // All identifiers come from this single authoritative route object.
  // 全部身份来自此单一权威路由对象。
  const route = candidate.route;
  return { providerId: route.provider_id, serviceEntryId: route.service_entry_id, connectionId: route.connection_id,
    modelId: route.model_id, bindingKind: route.binding.kind,
    protocol: route.binding.kind === "protocol" ? route.binding.protocol_profile_id : route.binding.action_binding_id };
}

// Serializes a non-secret route tuple without delimiter collisions.
// 序列化非秘密路由元组，避免分隔符碰撞。
// Parameters: route is an exact admitted request route. Returns its stable UI identity.
// 参数：route 为精确准入请求路由。返回稳定界面身份。
export function knowledgeRouteKey(route: KnowledgeRouteReferenceRequest): string {
  return JSON.stringify([route.providerId, route.serviceEntryId, route.connectionId, route.modelId, route.bindingKind, route.protocol]);
}

// Validates a human mode/dimension draft against the exact selected model without guessing a replacement.
// 根据精确选定模型验证人工模式及维度草稿，不推测替代值。
// Parameters: mode/candidate may be unselected; dimensionText and optional batchText retain exact human inputs. Returns ready settings or null.
// 参数：mode 及 candidate 可未选择；dimensionText 及可选 batchText 保留精确人工输入。返回就绪设置或空值。
export function knowledgeSettingsFromDraft(mode: KnowledgeIndexMode | null, candidate: KnowledgeEmbeddingCandidate | undefined, dimensionText: string, batchText = ""): KnowledgeLibrarySettingsRequest | null {
  if (mode === null) return null;
  if (mode === "browse" || mode === "bm25") return { indexMode: mode, embedding: null };
  if (candidate === undefined || dimensionText.trim() === "") return null;
  // The dimension must fit the service's declared u32 format as well as model-specific constraints.
  // 维度必须符合服务声明的 u32 格式及模型特定约束。
  const dimensions = Number(dimensionText);
  // The capability object belongs only to the selected candidate.
  // 能力对象仅属于选定候选。
  const capability = candidate.capabilities.embedding;
  if (!Number.isInteger(dimensions) || dimensions <= 0 || dimensions >= 2 ** 32
    || (capability.dimensions.length > 0 && !capability.dimensions.includes(dimensions))
    || (capability.minimum_dimensions !== undefined && dimensions < capability.minimum_dimensions)
    || (capability.maximum_dimensions !== undefined && dimensions > capability.maximum_dimensions)) return null;
  // Omission asks the server for its declared default; an explicit batch is never silently replaced.
  // 省略请求服务声明默认值；显式批量绝不被静默替换。
  const batch = batchText.trim() === "" ? null : Number(batchText);
  if (batch !== null && (!Number.isInteger(batch) || batch <= 0 || batch >= 2 ** 32
    || (capability.maximum_inputs !== undefined && batch > capability.maximum_inputs))) return null;
  return { indexMode: mode, embedding: { route: knowledgeRouteFromCandidate(candidate), dimensions, batchSize: batch } };
}
