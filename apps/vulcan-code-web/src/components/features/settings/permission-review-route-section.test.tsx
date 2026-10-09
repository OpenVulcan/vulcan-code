// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import type { PublicAiInternalInferenceCandidate, PublicAiSettingsSnapshot } from "@/types";

// The translator returns keys, so assertions name controls by catalog key instead of wording.
// 翻译函数返回键名，使断言以目录键而非文案定位控件。
vi.mock("@/i18n", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/i18n")>();
  const t = (key: string) => key;
  return { ...actual, useI18n: () => ({ locale: "en-US", setLocale: () => undefined, t }) };
});
vi.mock("sonner", () => ({ toast: { success: vi.fn() } }));

// Every service call is synthetic; nothing reaches a running service.
// 所有服务调用均为合成；不会访问运行中的服务。
const service = vi.hoisted(() => ({
  loadPublicAiSettings: vi.fn(),
  savePublicAiInferenceRoute: vi.fn(),
}));
vi.mock("@/service/settings-actions-service", () => service);

const { PermissionReviewRouteSection, permissionReviewCandidates } = await import("./permission-review-route-section");

let host: HTMLDivElement;
let root: Root;

beforeEach(() => {
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
});
afterEach(async () => {
  await act(async () => root.unmount());
  host.remove();
  vi.unstubAllGlobals();
  vi.clearAllMocks();
});

// candidate builds one internal conversation route with the given JSON capability.
// candidate 构建一条具有指定 JSON 能力的内部对话路由。
function candidate(modelId: string, structuredOutput: "json_schema" | "unsupported"): PublicAiInternalInferenceCandidate {
  return {
    providerDisplayName: "P", serviceEntryDisplayName: "S", connectionDisplayName: "C", modelDisplayName: modelId,
    route: {
      provider_id: "p", service_entry_id: "s", connection_id: "c", model_id: modelId,
      binding: { kind: "protocol", protocol_profile_id: "chat" },
      operation: "conversation_respond", usage_scope: "internal_background",
    },
    catalogGeneration: 1, enabled: true, usageScopes: ["internal_background"],
    capabilities: { structured_output: structuredOutput, reasoning_control: "effort_selection", reasoning_efforts: ["low", "high"] },
    overrides: {},
  } as unknown as PublicAiInternalInferenceCandidate;
}

it("offers only internal routes that answer with JSON", () => {
  const usable = candidate("usable", "json_schema");
  expect(permissionReviewCandidates([usable, candidate("plain", "unsupported")])).toEqual([usable]);
});

it("clears the saved review route through the shared route transaction", async () => {
  const usable = candidate("usable", "json_schema");
  const configured = {
    internalInferenceCandidates: [usable],
    inferenceProfiles: {
      permission_review: {
        llm_routes: { tool_call_review: usable.route },
        llm_reasoning_efforts: { tool_call_review: "high" },
      },
    },
    inferenceRouteDiagnostics: [],
  } as unknown as PublicAiSettingsSnapshot;
  service.loadPublicAiSettings.mockResolvedValue(configured);
  service.savePublicAiInferenceRoute.mockResolvedValue({ ...configured, inferenceProfiles: {} });

  await act(async () => root.render(<PermissionReviewRouteSection />));
  const clear = [...host.querySelectorAll("button")]
    .find((button) => button.textContent === "settings.permissionJudge.review.clear");
  expect(clear).toBeDefined();
  await act(async () => clear?.click());

  expect(service.savePublicAiInferenceRoute).toHaveBeenCalledExactlyOnceWith({
    consumerProfileId: "permission_review",
    purposeId: "tool_call_review",
    operation: "conversation_respond",
    route: null,
    reasoningEffort: undefined,
  });
  expect([...host.querySelectorAll("button")]
    .some((button) => button.textContent === "settings.permissionJudge.review.clear")).toBe(false);
});
