// @vitest-environment jsdom
import { TooltipProvider } from "@/components/ui/tooltip";
import { toast } from "sonner";
import React, { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { PublicAiInferenceRoute, PublicAiInferenceTestResult, PublicAiSettingsSnapshot, VceGlobalConfigurationStatus } from "@/types";
import { createVceActivityFixture } from "../code-context/vce-activity-test-fixture";
import { ContextEngineArea } from "./context-engine-area";

// Replace transport only; the real global settings surface owns observation ordering.
// 仅替换传输；真实全局设置界面负责观察顺序。
vi.mock("sonner", () => ({ toast: { success: vi.fn(), error: vi.fn() } }));
const rpc = vi.hoisted(() => ({ settings: vi.fn(), readiness: vi.fn(), save: vi.fn(), test: vi.fn() }));
vi.mock("@/service/settings-actions-service", () => ({ loadPublicAiSettings: rpc.settings, savePublicAiInferenceRoute: rpc.save, testPublicAiInferenceRoute: rpc.test }));
vi.mock("@/service/vce-service", () => ({ loadVceConfigurationStatus: rpc.readiness }));
vi.mock("@/i18n", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/i18n")>();
  const t = (key: import("@/i18n").TranslationKey) => actual.readLocaleMessage("zh-CN", key);
  return { ...actual, useI18n: () => ({ locale: "zh-CN", setLocale: () => undefined, t }) };
});

// The transport fixture includes exactly the snapshot fields read by this settings page.
// 传输夹具仅包含本设置页实际读取的快照字段。
const snapshot: Pick<PublicAiSettingsSnapshot, "inferenceProfiles" | "internalInferenceCandidates"> = {
  inferenceProfiles: {}, internalInferenceCandidates: [],
};
// Each test owns and disposes one real React root without any provider calls.
// 每项测试持有并释放一个真实 React 根，不调用供应商。
let root: Root;
// host is the private DOM container, removed after every test.
// host 为私有文档容器，每项测试后移除。
let host: HTMLDivElement;

beforeEach(() => {
  vi.resetAllMocks();
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  host = document.createElement("div"); document.body.append(host); root = createRoot(host);
  rpc.settings.mockResolvedValue(snapshot);
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); vi.unstubAllGlobals(); });

// Click the visible read-only recheck action and settle immediate effects.
// 点击可见只读重新检查操作，等待即时副作用完成。
// No parameters or result; missing controls fail instead of invoking a guessed callback.
// 无参数及返回值；控件缺失时失败，不调用猜测的回调。
async function recheck() {
  // button is identified by its exact visible label, never its DOM index.
  // button 按精确可见标签标识，不依赖文档顺序。
  const button = [...host.querySelectorAll("button")].find((element) => element.textContent?.trim() === "重新检查");
  expect(button).toBeDefined();
  await act(async () => button!.click());
}

describe("ContextEngine observation lifetime", () => {
  // An embedding summary requires the current explicit dimension and the actual executed model identity.
  // 向量摘要要求当前明确维度与实际执行的模型身份一致。
  // change selects a stable result, a concurrent dimension update or a mismatched execution; returns after UI assertions.
  // change 选择稳定结果、并发维度更新或执行身份不匹配；界面断言后返回。
  it.each(["unchanged", "dimension", "identity"] as const)("checks embedding test ownership for %s configuration", async (change) => {
    // The configured slot supplies explicit dimensions even when there are no selectable catalog candidates.
    // 即使没有可选目录候选，已配置槽位仍提供明确维度。
    const route: PublicAiInferenceRoute = { provider_id: "provider", service_entry_id: "service",
      connection_id: "account", model_id: "embedding-model", operation: "embedding_create",
      usage_scope: "internal_background", binding: { kind: "protocol", protocol_profile_id: "embedding" } };
    rpc.settings.mockResolvedValue({ ...snapshot, inferenceProfiles: { vce: { llm_routes: {},
      embedding: route, embedding_dimensions: 1024 } } });
    rpc.readiness.mockResolvedValue(createVceActivityFixture().configuration);
    // Hold the production test call while the next authoritative read is independently controlled.
    // 保留生产测试调用，同时独立控制下一次权威读取。
    let resolveTest!: (value: PublicAiInferenceTestResult) => void;
    rpc.test.mockReturnValueOnce(new Promise<PublicAiInferenceTestResult>((resolve) => { resolveTest = resolve; }));
    await act(async () => root.render(<TooltipProvider><ContextEngineArea workbenchRevision={1} /></TooltipProvider>));
    // The explicit saved-model control is the sole entry into this mocked provider test.
    // 明确的已保存模型控件是本模拟供应商测试的唯一入口。
    const test = [...host.querySelectorAll("button")].find((button) => !button.disabled && button.getAttribute("aria-label") === "测试连通性");
    expect(test?.disabled).toBe(false);
    await act(async () => test!.click());
    if (change === "dimension") rpc.settings.mockResolvedValue({ ...snapshot, inferenceProfiles: { vce: {
      llm_routes: {}, embedding: route, embedding_dimensions: 2048 } } });
    await act(async () => resolveTest({ operation: "embedding_create", providerId: "provider", serviceEntryId: "service",
      connectionId: "account", modelId: change === "identity" ? "different-model" : "embedding-model",
      dimensions: 1024, resultCount: 1, durationMs: 2300 }));
    if (change === "unchanged") expect(toast.success).toHaveBeenCalledWith("测试连通性成功", { description: "1 条结果 · 1024 维 · 2.3 s" });
    else {
      expect(host.textContent).not.toContain("1 条结果 · 1024 维 · 2.3 s");
      expect(toast.error).toHaveBeenCalledWith("测试连通性失败", { description: expect.stringContaining("模型配置或测试目标已变化") });
      expect(toast.success).not.toHaveBeenCalled();
    }
    expect(rpc.test).toHaveBeenCalledExactlyOnceWith({ consumerProfileId: "vce", operation: "embedding_create" });
    expect(rpc.save).not.toHaveBeenCalled();
  });

  // Completed and in-flight test results must not be presented under a subsequently selected model.
  // 已完成及在途测试结果不得显示在随后选择的模型之下。
  // timing chooses when the older response resolves; returns after verifying current model and result ownership.
  // timing 选择旧响应完成时机；验证当前模型及结果归属后返回。
  it.each(["completed", "in_flight", "refresh_race"] as const)("retires a %s model test when another configuration is observed", async (timing) => {
    // The saved rerank route uses the exact shared contract; candidates are unnecessary for testing a saved route.
    // 已保存重排路由使用精确共享契约；测试已保存路由不要求选择器候选。
    const route: PublicAiInferenceRoute = { provider_id: "provider", service_entry_id: "service",
      connection_id: "account", model_id: "tested-model", operation: "rerank_documents",
      usage_scope: "internal_background", binding: { kind: "protocol", protocol_profile_id: "rerank" } };
    rpc.settings.mockResolvedValue({ ...snapshot, inferenceProfiles: { vce: { llm_routes: {}, rerank: route } } });
    rpc.readiness.mockResolvedValue(createVceActivityFixture().configuration);
    // Result identity is returned by execution, not reconstructed from the currently visible picker.
    // 结果身份由执行返回，不从当前可见选择器重建。
    const result: PublicAiInferenceTestResult = { operation: "rerank_documents", providerId: "provider",
      serviceEntryId: "service", connectionId: "account", modelId: "tested-model", resultCount: 2, durationMs: 1700 };
    // The controlled promise keeps the real page mutation pending without any paid provider execution.
    // 受控承诺使真实页面修改保持等待，不执行付费供应商请求。
    let resolveTest!: (value: PublicAiInferenceTestResult) => void;
    rpc.test.mockReturnValueOnce(new Promise<PublicAiInferenceTestResult>((resolve) => { resolveTest = resolve; }));
    await act(async () => root.render(<TooltipProvider><ContextEngineArea workbenchRevision={1} /></TooltipProvider>));
    // Select the only non-embedding test button by its exact visible label.
    // 按精确可见标签选择唯一非向量测试按钮。
    const test = [...host.querySelectorAll("button")].find((button) => !button.disabled && button.getAttribute("aria-label") === "测试连通性");
    expect(test).toBeDefined();
    await act(async () => test!.click());
    expect(rpc.test).toHaveBeenCalledExactlyOnceWith({ consumerProfileId: "vce", operation: "rerank_documents" });
    if (timing === "completed") {
      await act(async () => resolveTest(result));
      expect(toast.success).toHaveBeenCalledWith("测试连通性成功", { description: "2 条结果 · 1.7 s" });
    }
    rpc.settings.mockResolvedValue({ ...snapshot, inferenceProfiles: { vce: { llm_routes: {},
      rerank: { ...route, model_id: "newer-model" } } } });
    // The final variant discovers the change only in the test's own post-execution reload.
    // 最后一种情况仅在测试自身执行后的重读中发现变化。
    if (timing !== "refresh_race") await act(async () => root.render(<TooltipProvider><ContextEngineArea workbenchRevision={2} /></TooltipProvider>));
    if (timing !== "completed") await act(async () => resolveTest(result));
    expect(host.textContent).toContain("newer-model");
    expect(host.textContent).not.toContain("2 条结果 · 1.7 s");
    expect(toast.success).toHaveBeenCalledTimes(timing === "completed" ? 1 : 0);
    expect(rpc.test).toHaveBeenCalledTimes(1);
    expect(rpc.save).not.toHaveBeenCalled();
  });

  // A save acknowledgement can precede another window's edit; render fresh authority afterward.
  // 保存确认之后可能已有其他窗口编辑；之后渲染重新读取的权威状态。
  // No inputs or return value; only a human click invokes the route mutation.
  // 无输入及返回值；仅人为点击调用路由修改。
  it("reloads current routes after saving instead of replaying the save snapshot", async () => {
    // route is an exact persisted LLM route; no executable candidate or provider is needed to clear it.
    // route 为精确已保存 LLM 路由；清除它不需要可执行候选或供应商。
    const route: PublicAiInferenceRoute = { provider_id: "provider", service_entry_id: "service",
      connection_id: "account", model_id: "original-model", operation: "conversation_respond",
      usage_scope: "internal_background", binding: { kind: "protocol", protocol_profile_id: "chat" } };
    rpc.settings.mockResolvedValueOnce({ ...snapshot, inferenceProfiles: {
      vce: { llm_routes: { "code_retrieval.query_rewrite": route } },
    } });
    rpc.readiness.mockResolvedValue(createVceActivityFixture().configuration);
    await act(async () => root.render(<TooltipProvider><ContextEngineArea workbenchRevision={1} /></TooltipProvider>));
    // Only the configured LLM has an enabled clear action in this fixture.
    // 本夹具中仅已配置的 LLM 具有可用清除操作。
    const clear = [...host.querySelectorAll("button")].find((button) => !button.disabled && button.getAttribute("aria-label") === "清除");
    expect(clear).toBeDefined();
    rpc.save.mockResolvedValueOnce(snapshot);
    rpc.settings.mockResolvedValueOnce({ ...snapshot, inferenceProfiles: {
      vce: { llm_routes: { "code_retrieval.query_rewrite": { ...route, model_id: "newer-window-model" } } },
    } });
    await act(async () => clear!.click());
    expect(rpc.save).toHaveBeenCalledExactlyOnceWith({ consumerProfileId: "vce",
      purposeId: "code_retrieval.query_rewrite", operation: "conversation_respond", route: null });
    expect(rpc.settings).toHaveBeenCalledTimes(2);
    expect(host.textContent).toContain("newer-window-model");
    expect(host.textContent).not.toContain("original-model");
  });

  // A newer workbench publication supersedes even an explicit recheck still in flight.
  // 较新的工作台发布也会替代仍在途的明确重新检查。
  // No inputs or return value; verifies both read entry points share the same response authority.
  // 无输入及返回值；验证两个读取入口共用响应权威。
  it("retires an explicit recheck when a newer workbench publication is read", async () => {
    rpc.readiness.mockResolvedValueOnce(createVceActivityFixture().configuration);
    await act(async () => root.render(<TooltipProvider><ContextEngineArea workbenchRevision={1} /></TooltipProvider>));
    // resolveOld holds the user-triggered read across the later publication.
    // resolveOld 使用户触发读取跨越稍后的发布。
    let resolveOld!: (status: VceGlobalConfigurationStatus) => void;
    rpc.readiness.mockReturnValueOnce(new Promise<VceGlobalConfigurationStatus>((resolve) => { resolveOld = resolve; }));
    await recheck();
    // closed is supplied by the later workbench read, not fabricated by the component.
    // closed 由稍后的工作台读取提供，不由组件编造。
    const closed = createVceActivityFixture().configuration;
    closed.ready = false; closed.embedding.available = false;
    rpc.readiness.mockResolvedValueOnce(closed);
    await act(async () => root.render(<TooltipProvider><ContextEngineArea workbenchRevision={2} /></TooltipProvider>));
    await act(async () => resolveOld(createVceActivityFixture().configuration));
    expect(host.textContent).toContain("已关闭");
    expect(host.textContent).not.toContain("已就绪");
  });

  // Current errors remain actionable; ordering protection must not swallow a live failure.
  // 当前错误仍可处理；顺序保护不能吞掉有效失败。
  // No inputs or return value; a deliberate retry restores the current status and clears the error.
  // 无输入及返回值；主动重试恢复当前状态并清除错误。
  it("shows a current read failure and clears it only after a successful retry", async () => {
    rpc.readiness.mockRejectedValueOnce(new Error("current catalog failure"));
    await act(async () => root.render(<TooltipProvider><ContextEngineArea workbenchRevision={1} /></TooltipProvider>));
    expect(host.textContent).toContain("current catalog failure");
    rpc.readiness.mockResolvedValueOnce(createVceActivityFixture().configuration);
    await recheck();
    expect(host.textContent).not.toContain("current catalog failure");
    expect(host.textContent).toContain("已就绪");
  });

  // Old success and old rejection both lose authority when a newer deliberate read succeeds.
  // 较新的主动读取成功后，旧成功与旧失败均失去权威。
  // outcome chooses the delayed transport completion; no persistence or model calls occur.
  // outcome 选择迟到传输终态；不持久化、不调用模型。
  it.each(["success", "failure"] as const)("discards an initial %s arriving after a newer recheck", async (outcome) => {
    // The old request is held until the newer read has published the closed gate.
    // 保留旧请求，直到较新读取已经发布关闭闸门。
    let resolveOld!: (status: VceGlobalConfigurationStatus) => void;
    // rejectOld exercises the same ordering with an obsolete transport error.
    // rejectOld 用过期传输错误验证同一顺序。
    let rejectOld!: (error: Error) => void;
    rpc.readiness.mockReturnValueOnce(new Promise<VceGlobalConfigurationStatus>((resolve, reject) => {
      resolveOld = resolve; rejectOld = reject;
    }));
    await act(async () => root.render(<TooltipProvider><ContextEngineArea workbenchRevision={1} /></TooltipProvider>));
    // closed is a newer authoritative model revocation, not an inferred provider failure.
    // closed 为较新的权威模型撤销，不是推测的供应商失败。
    const closed = createVceActivityFixture().configuration;
    closed.ready = false;
    closed.embedding.available = false;
    rpc.readiness.mockResolvedValueOnce(closed);
    await recheck();
    expect(host.textContent).toContain("已关闭");
    await act(async () => {
      if (outcome === "success") resolveOld(createVceActivityFixture().configuration);
      else rejectOld(new Error("obsolete catalog failure"));
    });
    expect(host.textContent).toContain("已关闭");
    expect(host.textContent).not.toContain("已就绪");
    expect(host.textContent).not.toContain("obsolete catalog failure");
  });
});
