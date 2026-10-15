import { describe, expect, it } from "vitest";

import { knowledgeEmbeddingCandidates, knowledgeRouteFromCandidate, knowledgeRouteKey, knowledgeSettingsFromDraft } from "./knowledge-mode";
import type { PublicAiInternalInferenceCandidate } from "@/types/public-ai";

// candidateFixture builds one exact OpenCode Go candidate for shared-pool filtering tests.
// candidateFixture 为共享模型池筛选测试构建一条精确 OpenCode Go 候选。
function candidateFixture(
  operation: PublicAiInternalInferenceCandidate["route"]["operation"],
  usageScope: PublicAiInternalInferenceCandidate["route"]["usage_scope"],
  enabled: boolean,
): PublicAiInternalInferenceCandidate {
  return {
    providerDisplayName: "OpenCode Go",
    serviceEntryDisplayName: "Go API",
    connectionDisplayName: "Test account",
    modelDisplayName: "DeepSeek V4 Flash",
    route: {
      provider_id: "opencode-go",
      service_entry_id: "go-api",
      connection_id: "go-account",
      model_id: operation === "embedding_create" ? "embedding-model" : "deepseek-v4-flash",
      binding: operation === "embedding_create"
        ? { kind: "action", action_binding_id: "embedding" }
        : { kind: "protocol", protocol_profile_id: "openai-responses.v1" },
      operation,
      usage_scope: usageScope,
    },
    catalogGeneration: 1,
    enabled,
    usageScopes: ["internal_background", "external_service"],
    capabilities: {
      operations: [operation],
      input_resources: [],
      output_resources: [],
      evidence_revision: "test",
      reasoning: "unknown",
      reasoning_control: "unknown",
      reasoning_efforts: [],
      prompt_cache: {
        provider_default_behavior: "test",
        supports_managed_explicit: false,
        supported_ttl_options: [],
      },
      model_parameters: [],
      embedding: operation === "embedding_create" ? {
        tasks: ["query", "document"],
        output_kinds: ["dense"],
        encodings: ["float"],
        dimensions: [768],
        normalization: "l2",
        distance_metric: "cosine",
      } : undefined,
    },
    overrides: {},
  };
}

describe("knowledge internal model pool", () => {
  it("does not admit conversation models as embedding routes", () => {
    expect(knowledgeEmbeddingCandidates([candidateFixture("conversation_respond", "internal_background", true)])).toEqual([]);
  });

  it("requires enabled background scope and an embedding capability", () => {
    const enabledEmbedding = candidateFixture("embedding_create", "internal_background", true);
    expect(knowledgeEmbeddingCandidates([
      enabledEmbedding,
      candidateFixture("embedding_create", "external_service", true),
      candidateFixture("embedding_create", "internal_background", false),
    ])).toEqual([enabledEmbedding]);
  });

  it("requires explicit modes and exact supported dimensions without guessing", () => {
    // This admitted route is the sole source of embedding settings.
    // 此准入路由为嵌入设置唯一来源。
    const [candidate] = knowledgeEmbeddingCandidates([candidateFixture("embedding_create", "internal_background", true)]);
    expect(knowledgeSettingsFromDraft(null, candidate, "768")).toBeNull();
    expect(knowledgeSettingsFromDraft("browse", undefined, "")).toEqual({ indexMode: "browse", embedding: null });
    expect(knowledgeSettingsFromDraft("hybrid", undefined, "768")).toBeNull();
    for (const dimensions of ["", "0", "768.5", "512", "Infinity", "NaN"]) expect(knowledgeSettingsFromDraft("vector", candidate, dimensions)).toBeNull();
    expect(knowledgeSettingsFromDraft("hybrid", candidate, "768")).toEqual({ indexMode: "hybrid", embedding: { route: knowledgeRouteFromCandidate(candidate), dimensions: 768, batchSize: null } });
    for (const batch of ["0", "1.5", "NaN", "4294967296"]) expect(knowledgeSettingsFromDraft("vector", candidate, "768", batch)).toBeNull();
  });

  it("does not admit unsupported declared metrics, tasks or encodings", () => {
    // A copy changes only a verified capability member for each admission probe.
    // 每个准入探测的副本仅更改已验证能力成员。
    const base = candidateFixture("embedding_create", "internal_background", true);
    if (base.capabilities.embedding === undefined) throw new Error("Synthetic embedding capability missing");
    for (const patch of [{ distance_metric: "unknown" }, { tasks: ["query"] }, { encodings: [] }, { output_kinds: [] }, { maximum_inputs: 0 }]) {
      expect(knowledgeEmbeddingCandidates([{ ...base, capabilities: { ...base.capabilities, embedding: { ...base.capabilities.embedding, ...patch } } }])).toEqual([]);
    }
  });

  it("retains route identity without delimiter collisions", () => {
    // IDs may contain punctuation; tuple serialization preserves field boundaries.
    // 身份可含标点；元组序列化保留字段边界。
    const route = knowledgeRouteFromCandidate(candidateFixture("embedding_create", "internal_background", true));
    expect(knowledgeRouteKey({ ...route, providerId: "a:b", modelId: "c" })).not.toBe(knowledgeRouteKey({ ...route, providerId: "a", modelId: "b:c" }));
  });
});
