// @vitest-environment jsdom
import React, { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import type { PublicAiInferenceRoute, PublicAiInternalInferenceCandidate } from "@/types";

// The component must reach the shared public-AI transaction named by the plan; the
// mock records the exact request so the consumer, purpose, and operation are asserted
// rather than assumed.
// 组件必须走方案点名的共享公共 AI 事务；mock 记录精确请求，使消费者、用途与操作可被断言
// 而不是假定。
const loadPublicAiSettings = vi.fn();
const savePublicAiInferenceRoute = vi.fn();
vi.mock("@/service/settings-actions-service", () => ({
  loadPublicAiSettings: () => loadPublicAiSettings(),
  savePublicAiInferenceRoute: (request: unknown) => savePublicAiInferenceRoute(request),
}));
vi.mock("@/i18n", () => ({ useI18n: () => ({ locale: "en-US", t: (key: string) => key }) }));

// route builds one exact route; the fields are the ones the identity key derives from.
// route 构建一条精确路由；字段即身份键所推导的那些。
function route(
  operation: string,
  modelId: string,
  usageScope = "internal_background",
): PublicAiInferenceRoute {
  return { provider_id: "provider", service_entry_id: "entry", connection_id: "connection",
    model_id: modelId, binding: { kind: "protocol", protocol_profile_id: "protocol" },
    operation, usage_scope: usageScope } as unknown as PublicAiInferenceRoute;
}

// candidate pairs one route with the display metadata the picker renders.
// candidate 将一条路由与其选择器渲染所需的展示元数据配对。
function candidate(
  operation: string,
  modelId: string,
  enabled = true,
  usageScope = "internal_background",
): PublicAiInternalInferenceCandidate {
  return { providerDisplayName: "Provider", serviceEntryDisplayName: "Entry",
    connectionDisplayName: "Connection", modelDisplayName: modelId,
    enabled, usageScopes: [usageScope], overrides: {}, capabilities: {},
    route: route(operation, modelId, usageScope) } as unknown as PublicAiInternalInferenceCandidate;
}

it("offers only the candidates whose operation each picker configures", async () => {
  const { luaskillsLlmCandidates, luaskillsEmbeddingCandidates } =
    await import("./lua-skills-inference-routes");
  const candidates = [candidate("conversation_respond", "chat-model"),
    candidate("embedding_create", "embed-model"), candidate("rerank_documents", "rerank-model")];

  expect(luaskillsLlmCandidates(candidates).map((item) => item.route.model_id))
    .toEqual(["chat-model"]);
  expect(luaskillsEmbeddingCandidates(candidates).map((item) => item.route.model_id))
    .toEqual(["embed-model"]);
});

it("excludes disabled candidates and routes owned by another usage scope", async () => {
  const { luaskillsLlmCandidates, luaskillsEmbeddingCandidates } =
    await import("./lua-skills-inference-routes");
  const candidates = [candidate("conversation_respond", "enabled-chat"),
    candidate("conversation_respond", "disabled-chat", false),
    candidate("conversation_respond", "external-chat", true, "external_service"),
    candidate("embedding_create", "enabled-embed"),
    candidate("embedding_create", "disabled-embed", false),
    candidate("embedding_create", "external-embed", true, "external_service")];

  expect(luaskillsLlmCandidates(candidates).map((item) => item.route.model_id))
    .toEqual(["enabled-chat"]);
  expect(luaskillsEmbeddingCandidates(candidates).map((item) => item.route.model_id))
    .toEqual(["enabled-embed"]);
});

it("derives the selected keys from the committed profile, not from model names", async () => {
  const { luaskillsSelectedRouteKeys } = await import("./lua-skills-inference-routes");
  const keys = luaskillsSelectedRouteKeys({
    llm_routes: { temporary_llm: route("conversation_respond", "chat-model") },
    embedding: route("embedding_create", "embed-model"),
  } as never);
  // The key carries the exact binding identity, so two models on one connection can
  // never collapse into the same selection.
  // 键携带精确绑定身份，因此同一连接上的两个模型绝不会坍缩成同一选择。
  expect(keys.llm).toContain("chat-model");
  expect(keys.llm).toContain("protocol:protocol");
  expect(keys.embedding).toContain("embed-model");
  expect(keys.llm).not.toBe(keys.embedding);
});

it("treats an absent LuaSkills profile as no configured route", async () => {
  const { luaskillsSelectedRouteKeys } = await import("./lua-skills-inference-routes");
  // LuaSkills is optional, so a missing profile must render as unselected rather than
  // as an error.
  // LuaSkills 是可选的，因此缺失 Profile 必须渲染为未选中而不是错误。
  expect(luaskillsSelectedRouteKeys(undefined)).toEqual({ llm: "", embedding: "" });
});

let root: Root;
let host: HTMLDivElement;
beforeEach(() => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  // JSDOM has no layout observer; this inert stub supports the real Combobox lifecycle.
  // JSDOM 没有布局观察器；此空实现用于支持真实 Combobox 生命周期。
  vi.stubGlobal("ResizeObserver", class { observe() {} unobserve() {} disconnect() {} });
  // JSDOM does not scroll command items; the stub preserves selection behavior under test.
  // JSDOM 不滚动命令项；此替身保留本测试关注的选择行为。
  Element.prototype.scrollIntoView = vi.fn();
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
});
afterEach(async () => {
  await act(async () => root.unmount());
  host.remove();
  vi.unstubAllGlobals();
});

it("renders both route pickers and no error when no route is configured", async () => {
  loadPublicAiSettings.mockResolvedValue({ inferenceProfiles: {},
    internalInferenceCandidates: [candidate("conversation_respond", "chat-model"),
      candidate("embedding_create", "embed-model")] });
  const { LuaSkillsInferenceRoutes } = await import("./lua-skills-inference-routes");
  await act(async () => root.render(<LuaSkillsInferenceRoutes />));
  await act(async () => {});
  // Both purposes are offered independently, so one missing route cannot hide the other.
  // 两个用途各自独立提供，因此缺少一条路由不会隐藏另一条。
  const triggers = document.querySelectorAll("[role=\"combobox\"]");
  expect(triggers.length).toBe(2);
  expect(document.body.textContent).toContain("Internal model routes");
  expect(document.body.textContent).not.toContain("alert");
});

it("surfaces a load failure instead of silently showing unselected pickers", async () => {
  loadPublicAiSettings.mockRejectedValue(new Error("snapshot unavailable"));
  const { LuaSkillsInferenceRoutes } = await import("./lua-skills-inference-routes");
  await act(async () => root.render(<LuaSkillsInferenceRoutes />));
  await act(async () => {});
  expect(document.body.textContent).toContain("snapshot unavailable");
});

// configuredSnapshot is the committed shape that enables the clear controls: one exact
// route for each purpose, so the buttons are no longer disabled by an empty key.
// configuredSnapshot 是启用清除控件所需的已提交形状：每个用途各一条精确路由，因此按钮
// 不再因空键而禁用。
function configuredSnapshot() {
  return {
    inferenceProfiles: {
      luaskills: { llm_routes: { temporary_llm: route("conversation_respond", "chat-model") },
        embedding: route("embedding_create", "embed-model") },
    },
    internalInferenceCandidates: [candidate("conversation_respond", "chat-model"),
      candidate("embedding_create", "embed-model")],
    inferenceRouteDiagnostics: [],
  };
}

it("retains an unavailable route visibly, reports its reason, and keeps clear enabled", async () => {
  const retained = route("conversation_respond", "retained-chat");
  loadPublicAiSettings.mockResolvedValue({
    inferenceProfiles: { luaskills: { llm_routes: { temporary_llm: retained } } },
    internalInferenceCandidates: [candidate("conversation_respond", "replacement-chat")],
    inferenceRouteDiagnostics: [{ consumerProfileId: "luaskills",
      purposeId: "temporary_llm", operation: "conversation_respond", route: retained,
      reason: "configured connection is disabled" }],
  });
  const { LuaSkillsInferenceRoutes } = await import("./lua-skills-inference-routes");
  await act(async () => root.render(<LuaSkillsInferenceRoutes />));
  await act(async () => {});

  expect(document.body.textContent).toContain("retained-chat · connection");
  expect(document.body.textContent).toContain("configured connection is disabled");
  expect(document.body.textContent).toContain("Select an available route or clear");
  expect(clearButton()?.hasAttribute("disabled")).toBe(false);
  expect(savePublicAiInferenceRoute).not.toHaveBeenCalled();
});

// clearButton resolves the question-answering clear control by its visible label.
// clearButton 按可见标签定位问答路由的清除控件。
function clearButton(): HTMLButtonElement | undefined {
  return [...document.querySelectorAll("button")]
    .find((button) => button.textContent === "Clear question-answering route");
}

it("reports a committed clear instead of leaving the operator without confirmation", async () => {
  loadPublicAiSettings.mockResolvedValue(configuredSnapshot());
  savePublicAiInferenceRoute.mockResolvedValue(configuredSnapshot());
  const { LuaSkillsInferenceRoutes } = await import("./lua-skills-inference-routes");
  await act(async () => root.render(<LuaSkillsInferenceRoutes />));
  await act(async () => {});

  const clear = clearButton();
  // A guard clause narrows the type, which `expect(...).toBeTruthy()` cannot do: without
  // it the file type-checks as possibly-undefined even though the test would pass.
  // 守卫子句负责类型收窄，这是 `expect(...).toBeTruthy()` 做不到的：缺了它，即便测试通过，
  // 该文件在类型检查中仍是 possibly-undefined。
  if (!clear) throw new Error("the clear control must be offered for a configured route");
  await act(async () => { clear.click(); });
  await act(async () => {});

  // The clear notice is a separate branch from the success notice, so it is asserted on
  // its own rather than inferred from the request having been sent.
  // 清除提示与保存成功提示是两个分支，因此单独断言，而不是从「请求已发出」推断。
  expect(document.body.textContent).toContain("Cleared; the capability is now disabled.");
});

it("surfaces a save failure and releases the control it disabled", async () => {
  loadPublicAiSettings.mockResolvedValue(configuredSnapshot());
  savePublicAiInferenceRoute.mockRejectedValue(new Error("route transaction rejected"));
  const { LuaSkillsInferenceRoutes } = await import("./lua-skills-inference-routes");
  await act(async () => root.render(<LuaSkillsInferenceRoutes />));
  await act(async () => {});

  const clear = clearButton();
  if (!clear) throw new Error("the clear control must be offered for a configured route");
  await act(async () => { clear.click(); });
  await act(async () => {});

  expect(document.body.textContent).toContain("route transaction rejected");
  // A rejected save must release the control it disabled, or the operator cannot retry.
  // 被拒绝的保存必须释放它禁用的控件，否则操作者无法重试。
  expect(clearButton()?.hasAttribute("disabled")).toBe(false);
});

it("keeps the committed route selected when a replacement save is rejected", async () => {
  const initial = configuredSnapshot();
  initial.internalInferenceCandidates.push(candidate("conversation_respond", "replacement-chat"));
  loadPublicAiSettings.mockResolvedValue(initial);
  savePublicAiInferenceRoute.mockRejectedValue(new Error("replacement rejected"));
  const { LuaSkillsInferenceRoutes } = await import("./lua-skills-inference-routes");
  await act(async () => root.render(<LuaSkillsInferenceRoutes />));
  await act(async () => {});

  const trigger = document.querySelector<HTMLButtonElement>("[role=\"combobox\"]");
  if (!trigger) throw new Error("the LLM route picker must be rendered");
  expect(trigger.textContent).toContain("chat-model");
  await act(async () => { trigger.click(); });
  const replacement = [...document.querySelectorAll<HTMLElement>("[cmdk-item]")]
    .find((item) => item.textContent?.includes("replacement-chat"));
  if (!replacement) throw new Error("the replacement route must be offered");
  await act(async () => { replacement.click(); });
  await act(async () => {});

  expect(document.body.textContent).toContain("replacement rejected");
  expect(trigger.textContent).toContain("chat-model");
  expect(trigger.textContent).not.toContain("replacement-chat");
});
