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
  testPublicAiInferenceRoute: vi.fn(),
}));
vi.mock("@/service/settings-actions-service", () => service);

const { PermissionJudgeCloudSection, permissionJudgeCloudCandidates } = await import("./permission-judge-cloud-section");

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

// candidate builds one enabled internal route for the given operation and binding kind.
// candidate 构建一条指定操作与绑定类型的已启用内部路由。
function candidate(modelId: string, operation: string, bindingKind: "action" | "protocol"): PublicAiInternalInferenceCandidate {
  return {
    providerDisplayName: "P", serviceEntryDisplayName: "S", connectionDisplayName: "C", modelDisplayName: modelId,
    route: {
      provider_id: "p", service_entry_id: "s", connection_id: "c", model_id: modelId,
      binding: bindingKind === "action"
        ? { kind: "action", action_id: "system_one" }
        : { kind: "protocol", protocol_profile_id: "chat" },
      operation, usage_scope: "internal_background",
    },
    catalogGeneration: 1, enabled: true, usageScopes: ["internal_background"], capabilities: {}, overrides: {},
  } as unknown as PublicAiInternalInferenceCandidate;
}

// buttonByKey finds a button whose text is exactly a catalog key.
// buttonByKey 查找文本恰为某个目录键的按钮。
function buttonByKey(key: string): HTMLButtonElement | undefined {
  return Array.from(host.querySelectorAll("button")).find((button) => button.textContent === key);
}

// configured builds a snapshot whose judge profile holds the given decision route.
// configured 构建判定配置档持有指定判定路由的快照。
function configured(decision: PublicAiInternalInferenceCandidate): PublicAiSettingsSnapshot {
  return {
    internalInferenceCandidates: [decision],
    inferenceProfiles: { permission_judge: { decision: decision.route } },
    inferenceRouteDiagnostics: [],
  } as unknown as PublicAiSettingsSnapshot;
}

it("offers only internal decision routes", () => {
  const decision = candidate("jev", "decision_evaluate", "action");
  expect(permissionJudgeCloudCandidates([
    decision,
    candidate("chat", "conversation_respond", "protocol"),
    candidate("rerank", "rerank_documents", "action"),
  ])).toEqual([decision]);
});

it("reports the saved route and tests it with one decision request", async () => {
  const decision = candidate("jev", "decision_evaluate", "action");
  service.loadPublicAiSettings.mockResolvedValue(configured(decision));
  service.testPublicAiInferenceRoute.mockResolvedValue({ modelId: "jev", durationMs: 320 });
  const onConfiguredChange = vi.fn();

  await act(async () => root.render(<PermissionJudgeCloudSection onConfiguredChange={onConfiguredChange} />));
  expect(onConfiguredChange).toHaveBeenLastCalledWith(true);
  await act(async () => buttonByKey("settings.permissionJudge.cloud.test")!.click());

  expect(service.testPublicAiInferenceRoute).toHaveBeenCalledExactlyOnceWith({
    consumerProfileId: "permission_judge",
    operation: "decision_evaluate",
  });
  expect(host.querySelector('[role="status"]')).not.toBeNull();
});

it("clears the saved route through the shared route transaction", async () => {
  const decision = candidate("jev", "decision_evaluate", "action");
  service.loadPublicAiSettings.mockResolvedValue(configured(decision));
  service.savePublicAiInferenceRoute.mockResolvedValue({ ...configured(decision), inferenceProfiles: {} });
  const onConfiguredChange = vi.fn();

  await act(async () => root.render(<PermissionJudgeCloudSection onConfiguredChange={onConfiguredChange} />));
  await act(async () => buttonByKey("settings.permissionJudge.cloud.clear")!.click());

  expect(service.savePublicAiInferenceRoute).toHaveBeenCalledExactlyOnceWith({
    consumerProfileId: "permission_judge",
    operation: "decision_evaluate",
    route: null,
  });
  expect(onConfiguredChange).toHaveBeenLastCalledWith(false);
  expect(buttonByKey("settings.permissionJudge.cloud.clear")).toBeUndefined();
});
