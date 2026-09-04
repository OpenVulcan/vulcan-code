import { describe, expect, it } from "vitest";

import type { PublicAiInternalInferenceCandidate, PublicAiOperation } from "@/types";

import { vceCandidateEligible } from "./vce-built-in-plugin-panel";

// candidateFixture builds one exact internal-background catalog candidate.
// candidateFixture 构建一个精确的内部后台目录候选项。
function candidateFixture(operation: PublicAiOperation): PublicAiInternalInferenceCandidate {
  return {
    providerDisplayName: "Provider",
    serviceEntryDisplayName: "Service",
    connectionDisplayName: "Account",
    modelDisplayName: "Model",
    route: {
      provider_id: "provider",
      service_entry_id: "service",
      connection_id: "account",
      model_id: "model",
      binding: { kind: "protocol", protocol_profile_id: "protocol" },
      operation,
      usage_scope: "internal_background",
    },
    catalogGeneration: 1,
    enabled: true,
    usageScopes: ["internal_background"],
    capabilities: {
      operations: [operation],
      structured_output: "json_object",
      evidence_revision: "test-revision",
      reasoning: "unknown",
      reasoning_control: "unknown",
      reasoning_efforts: [],
      prompt_cache: {
        provider_default_behavior: "disabled",
        supports_managed_explicit: false,
        supported_ttl_options: [],
      },
      model_parameters: [],
    },
    overrides: {},
  };
}

describe("VCE built-in plugin candidate contract", () => {
  // Embedding requires document/query dense-float output and a deterministic dimension.
  // 向量候选项要求文档/查询稠密浮点输出与确定维度。
  it("accepts only a complete deterministic embedding contract", () => {
    const candidate = candidateFixture("embedding_create");
    candidate.capabilities.embedding = {
      tasks: ["document", "query"],
      output_kinds: ["dense"],
      encodings: ["float"],
      dimensions: [],
      default_dimensions: 1_024,
      normalization: "l2",
      distance_metric: "cosine",
    };

    expect(vceCandidateEligible(candidate, "embedding")).toBe(true);
    candidate.capabilities.embedding.default_dimensions = undefined;
    expect(vceCandidateEligible(candidate, "embedding")).toBe(false);
  });

  // Query rewriting accepts host-validatable JSON only when reasoning can be fixed off.
  // 查询改写仅接受能够固定关闭推理且可由宿主校验 JSON 的模型。
  it("rejects an LLM that cannot explicitly disable reasoning", () => {
    const candidate = candidateFixture("conversation_respond");
    candidate.capabilities.structured_output = "json_schema";
    candidate.capabilities.reasoning_control = "fixed_on";
    candidate.capabilities.reasoning_efforts = ["high"];

    expect(vceCandidateEligible(candidate, "query_rewrite")).toBe(false);
    candidate.capabilities.reasoning_control = "toggle";
    candidate.capabilities.reasoning_efforts = ["none", "high"];
    expect(vceCandidateEligible(candidate, "query_rewrite")).toBe(true);
    candidate.capabilities.structured_output = "json_object";
    expect(vceCandidateEligible(candidate, "query_rewrite")).toBe(true);
    candidate.capabilities.structured_output = "unsupported";
    expect(vceCandidateEligible(candidate, "query_rewrite")).toBe(false);
  });

  // Rerank must preserve oversized-input rejection instead of truncating code silently.
  // 重排必须保留超限拒绝，不能静默截断代码。
  it("requires disabled rerank truncation", () => {
    const candidate = candidateFixture("rerank_documents");
    candidate.capabilities.rerank = {
      truncation: ["provider_default"],
      supports_return_content: false,
      score_semantics: "higher_is_better",
    };

    expect(vceCandidateEligible(candidate, "rerank")).toBe(false);
    candidate.capabilities.rerank.truncation = ["disabled"];
    expect(vceCandidateEligible(candidate, "rerank")).toBe(true);
  });
});
