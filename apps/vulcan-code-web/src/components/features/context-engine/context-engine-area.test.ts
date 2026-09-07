import { describe, expect, it } from "vitest";

import type { PublicAiInternalInferenceCandidate, PublicAiOperation } from "@/types";

import { vceCandidateEligible, vceRoutePickerOptions } from "./context-engine-area";

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

describe("ContextEngine candidate contract", () => {
  // Route options reuse the generic combobox and expose every human-searchable model identity.
  // 路由选项复用通用组合框，并暴露全部便于人类搜索的模型身份。
  it("builds searchable model route options", () => {
    // option is the exact UI projection of one backend candidate.
    // option 是一个后端候选项的精确界面投影。
    const [option] = vceRoutePickerOptions([candidateFixture("embedding_create")]);

    expect(option.label).toBe("Model · Account");
    expect(option.description).toBe("Provider · Service");
    expect(option.keywords).toEqual(expect.arrayContaining([
      "model",
      "Provider",
      "provider",
      "Account",
      "account",
      "Service",
      "service",
      "protocol",
    ]));
  });

  // Embedding accepts either task-specific projection or one declared provider-default task.
  // 向量候选项接受任务专用投影或已声明的 Provider 默认任务。
  it("accepts the host-supported provider-default embedding contract", () => {
    const candidate = candidateFixture("embedding_create");
    candidate.capabilities.embedding = {
      tasks: ["provider_default"],
      output_kinds: ["dense"],
      encodings: ["float"],
      dimensions: [1_024],
      default_dimensions: 1_024,
      normalization: "l2",
      distance_metric: "cosine",
    };

    expect(vceCandidateEligible(candidate, "embedding")).toBe(true);
    candidate.capabilities.embedding.dimensions = [];
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

  // Rerank accepts both explicit no-truncation and an exact provider-default contract.
  // 重排同时接受显式禁用截断与精确的 Provider 默认契约。
  it("accepts the declared provider-default rerank contract", () => {
    const candidate = candidateFixture("rerank_documents");
    candidate.capabilities.rerank = {
      truncation: ["provider_default"],
      supports_return_content: false,
      score_semantics: "higher_is_better",
    };

    expect(vceCandidateEligible(candidate, "rerank")).toBe(true);
    candidate.capabilities.rerank.truncation = ["disabled"];
    expect(vceCandidateEligible(candidate, "rerank")).toBe(true);
    candidate.capabilities.rerank.truncation = [];
    expect(vceCandidateEligible(candidate, "rerank")).toBe(false);
  });
});
