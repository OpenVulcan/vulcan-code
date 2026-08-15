import { describe, expect, it } from "vitest";

import {
  knowledgeCurationCandidates,
  knowledgeEmbeddingCandidates,
} from "./KnowledgeLibraryCreateDialog";
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
        tasks: [],
        output_kinds: [],
        encodings: [],
        dimensions: [768],
        normalization: "l2",
        distance_metric: "cosine",
      } : undefined,
    },
    overrides: {},
  };
}

describe("knowledge internal model pool", () => {
  it("shows an enabled OpenCode Go LLM independently from conversation publication", () => {
    const enabledBackground = candidateFixture(
      "conversation_respond",
      "internal_background",
      true,
    );
    const candidates = [
      enabledBackground,
      candidateFixture("conversation_respond", "external_service", true),
      candidateFixture("conversation_respond", "internal_background", false),
    ];

    expect(knowledgeCurationCandidates(candidates)).toEqual([enabledBackground]);
  });

  it("requires enabled background scope and an embedding capability", () => {
    const enabledEmbedding = candidateFixture("embedding_create", "internal_background", true);
    expect(knowledgeEmbeddingCandidates([
      enabledEmbedding,
      candidateFixture("embedding_create", "external_service", true),
      candidateFixture("embedding_create", "internal_background", false),
    ])).toEqual([enabledEmbedding]);
  });
});
