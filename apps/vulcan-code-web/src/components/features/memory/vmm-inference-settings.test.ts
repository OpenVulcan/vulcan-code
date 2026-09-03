import { describe, expect, it } from "vitest"

import type {
  PublicAiInferenceRoute,
  PublicAiInternalInferenceCandidate,
  PublicAiSettingsSnapshot,
} from "@/types"

import {
  VMM_REQUIRED_LLM_PURPOSES,
  vmmRequiredInferenceReady,
} from "./vmm-inference-settings"

// testRoute builds one exact internal-background route for a required native VMM operation.
// testRoute 为必需的原生 VMM 操作构建一条精确内部后台路由。
function testRoute(
  operation: PublicAiInferenceRoute["operation"],
  suffix: string,
): PublicAiInferenceRoute {
  return {
    provider_id: "provider",
    service_entry_id: "entry",
    connection_id: `connection-${suffix}`,
    model_id: `model-${suffix}`,
    binding: operation === "conversation_respond"
      ? { kind: "protocol", protocol_profile_id: "openai-chat" }
      : { kind: "action", action_binding_id: "embedding-action" },
    operation,
    usage_scope: "internal_background",
  }
}

// testCandidate turns one route into an enabled catalog-backed inference candidate.
// testCandidate 将一条路由转换为已启用且由目录支撑的推理候选。
function testCandidate(route: PublicAiInferenceRoute): PublicAiInternalInferenceCandidate {
  return {
    providerDisplayName: "Provider",
    serviceEntryDisplayName: "Entry",
    connectionDisplayName: route.connection_id,
    modelDisplayName: route.model_id,
    route,
    catalogGeneration: 1,
    enabled: true,
    usageScopes: ["internal_background"],
    capabilities: {
      reasoning_control: "toggle",
      reasoning_efforts: ["none"],
    },
    overrides: {},
  } as PublicAiInternalInferenceCandidate
}

// readySnapshot builds the smallest exact VMM profile that satisfies first-run initialization.
// readySnapshot 构建满足首次初始化条件的最小精确 VMM 配置档。
function readySnapshot(): PublicAiSettingsSnapshot {
  const embedding = testRoute("embedding_create", "embedding")
  const llmRoutes = Object.fromEntries(
    VMM_REQUIRED_LLM_PURPOSES.map((purpose) => [
      purpose,
      testRoute("conversation_respond", purpose),
    ]),
  )
  return {
    internalInferenceCandidates: [
      testCandidate(embedding),
      ...Object.values(llmRoutes).map(testCandidate),
    ],
    inferenceProfiles: {
      vmm: {
        embedding,
        llm_routes: llmRoutes,
      },
    },
    inferenceRouteDiagnostics: [],
  } as unknown as PublicAiSettingsSnapshot
}

describe("VMM first-run inference readiness", () => {
  it("requires embedding and every foundational LLM route", () => {
    const snapshot = readySnapshot()
    expect(vmmRequiredInferenceReady(snapshot)).toBe(true)

    delete snapshot.inferenceProfiles.vmm.llm_routes?.postaction_l2
    expect(vmmRequiredInferenceReady(snapshot)).toBe(false)
  })

  it("rejects a disabled route or an authoritative diagnostic", () => {
    const disabledSnapshot = readySnapshot()
    disabledSnapshot.internalInferenceCandidates[0].enabled = false
    expect(vmmRequiredInferenceReady(disabledSnapshot)).toBe(false)

    const diagnosedSnapshot = readySnapshot()
    diagnosedSnapshot.inferenceRouteDiagnostics.push({
      consumerProfileId: "vmm",
      purposeId: "precheck_l1",
      operation: "conversation_respond",
      route: diagnosedSnapshot.inferenceProfiles.vmm.llm_routes!.precheck_l1,
      reason: "route unavailable",
    })
    expect(vmmRequiredInferenceReady(diagnosedSnapshot)).toBe(false)
  })
})
