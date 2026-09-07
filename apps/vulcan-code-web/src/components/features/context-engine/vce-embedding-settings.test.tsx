// @vitest-environment jsdom
import React, { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { PublicAiInternalInferenceCandidate, VceEmbeddingPreflight } from "@/types";
import { VceEmbeddingSettings } from "./vce-embedding-settings";

// Replace transport only; the production searchable model filter and editor remain under test.
// 仅替换传输；生产可搜索模型过滤器及编辑器仍接受测试。
const rpc = vi.hoisted(() => ({ preflight: vi.fn(), save: vi.fn() }));
vi.mock("@/service/vce-service", () => ({ preflightVceEmbeddingRoute: rpc.preflight }));
vi.mock("@/service/settings-actions-service", () => ({ savePublicAiInferenceRoute: rpc.save }));
vi.mock("@/i18n", () => ({ useI18n: () => ({ locale: "zh-CN" }) }));

// Build exact route fixtures with identical model names but distinct providers/accounts.
// 构建模型名相同、供应商及账号不同的精确路由夹具。
// provider selects ownership; returns a complete shared-filter candidate with explicit dimensions.
// provider 选择归属；返回含明确维度的完整通用过滤器候选。
function candidate(provider: string): PublicAiInternalInferenceCandidate {
  return { providerDisplayName: provider, serviceEntryDisplayName: "Service", connectionDisplayName: `Account ${provider}`,
    modelDisplayName: "Same Model", route: { provider_id: provider, service_entry_id: "service",
      connection_id: `account-${provider}`, model_id: "same-model", binding: { kind: "protocol", protocol_profile_id: "embedding" },
      operation: "embedding_create", usage_scope: "internal_background" }, catalogGeneration: 1, enabled: true,
    usageScopes: ["internal_background"], overrides: {}, capabilities: { operations: ["embedding_create"],
      structured_output: "unsupported", evidence_revision: "fixture", reasoning: "unknown", reasoning_control: "unknown",
      reasoning_efforts: [], prompt_cache: { provider_default_behavior: "disabled", supports_managed_explicit: false,
        supported_ttl_options: [] }, model_parameters: [], embedding: { tasks: ["provider_default"], output_kinds: ["dense"],
        encodings: ["float"], dimensions: [512, 1024, 2048], normalization: "l2", distance_metric: "cosine" } } };
}

// Build a read-only preflight wire fixture; no index/provider execution is simulated as successful.
// 构建只读预检线协议夹具；不将索引或供应商执行模拟为成功。
// provider identifies the target and hard marks an unwaivable constraint. Returns one exact project impact.
// provider 标识目标，hard 标识不可免除限制。返回一个精确项目影响。
function preflight(provider = "B", hard = false): VceEmbeddingPreflight {
  const target = candidate(provider);
  if (!target.capabilities.embedding) throw new Error("fixture must declare embedding");
  return { candidate: { route: target.route, dimension: hard ? 2048 : 1024, physicalFingerprint: "target-fingerprint",
    capabilities: target.capabilities.embedding }, revision: "review-revision",
    projects: [{ projectName: "Project A", compatible: false, impact: { scope: { project_id: "project-a",
      root_hash: "a".repeat(64), namespace_id: "namespace-a", checkpoint_chain_id: "chain-a", original_fingerprint: "source-a",
      index_schema_version: 1, dimension: 1024, preprocessing_version: "preprocess-v1", chunker_version: "chunker-v1" },
      checkpointVersion: 7, sourceFingerprints: ["source-a"], confirmationRequired: ["source-a"], rejectedSources: [],
      hardRebuildReason: hard ? "Embedding dimensions changed from 1024 to 2048" : null } }] };
}

// Isolated real React root and explicit side-effect observers.
// 隔离真实 React 根及明确副作用观察器。
let root: Root;
let host: HTMLDivElement;
const saved = vi.fn();
const busy = vi.fn();
const testModel = vi.fn();

// Mount saved settings while retaining the same component identity during external updates.
// 挂载已保存设置，在外部更新期间保留同一组件身份。
// dimension is explicit saved intent or absence. Returns after effects settle, without triggering requests.
// dimension 为明确已保存意图或缺失。副作用稳定后返回，不触发请求。
async function render(dimension?: number) {
  await act(async () => root.render(<VceEmbeddingSettings route={candidate("A").route} dimension={dimension}
    candidates={[candidate("A"), candidate("B")]} status={{ configured: true, available: true }} disabled={false}
    testing={false} onSaved={saved} onBusyChange={busy} onTest={testModel} />));
}

// Find one exact button by its rendered text; throw when the production UI does not expose it.
// 按渲染文本查找一个精确按钮；生产界面未暴露时抛错。
// text is the user label. Returns the real DOM button, not a mocked callback.
// text 为用户标签。返回真实文档按钮，不返回模拟回调。
function button(text: string): HTMLButtonElement {
  const found = [...host.querySelectorAll("button")].find((item) => item.textContent === text);
  if (!found) throw new Error(`Missing button: ${text}`);
  return found;
}

// Click one verified DOM element and await React effects; missing controls fail the test.
// 点击一个已验证文档元素并等待 React 副作用；控件缺失时测试失败。
// element is the queried user control. Returns when its immediate asynchronous state settles.
// element 为查得的用户控件。其即时异步状态稳定后返回。
async function click(element: Element | null) {
  if (!(element instanceof HTMLElement)) throw new Error("Missing interactive control");
  await act(async () => element.click());
}

beforeEach(() => {
  vi.resetAllMocks();
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  // JSDOM has no layout observer; this shim supplies only browser geometry plumbing to cmdk.
  // JSDOM 没有布局观察器；此替身仅向通用过滤器提供浏览器几何接口。
  vi.stubGlobal("ResizeObserver", class {
    // Register no layout because this environment computes no geometry; no inputs or returned observations.
    // 此环境不计算几何，因此不注册布局；无输入及返回观察结果。
    observe() {}
    // Release no layout registration; no inputs or return value.
    // 释放空布局注册；无输入及返回值。
    unobserve() {}
    // Disconnect the geometry-only test shim; no inputs or return value.
    // 断开仅几何测试替身；无输入及返回值。
    disconnect() {}
  });
  vi.stubGlobal("scrollIntoView", vi.fn());
  Object.defineProperty(HTMLElement.prototype, "scrollIntoView", { configurable: true, value: vi.fn() });
  rpc.save.mockRejectedValue(new Error("预检已过期，请重新检查"));
  rpc.preflight.mockResolvedValue(preflight());
  host = document.createElement("div"); document.body.append(host); root = createRoot(host);
});
afterEach(async () => {
  await act(async () => root.unmount()); host.remove();
  Reflect.deleteProperty(HTMLElement.prototype, "scrollIntoView"); vi.unstubAllGlobals();
});

describe("ContextEngine embedding draft workflow", () => {
  // A dimension suggestion never persists a model or starts a provider request on mount.
  // 维度建议绝不在挂载时持久化模型或启动供应商请求。
  it("suggests an explicit dimension without an automatic save or test", async () => {
    await render();
    expect(host.querySelector('[aria-label="向量维度"]')?.textContent).toBe("1024");
    expect(button("测试已保存模型").disabled).toBe(true);
    expect(rpc.preflight).not.toHaveBeenCalled(); expect(rpc.save).not.toHaveBeenCalled();
    expect(testModel).not.toHaveBeenCalled();
  });
  // Same-name provider selection remains a draft until every project has an explicit unchecked decision.
  // 同名供应商选择保持草稿，直到每个项目获得明确且不预勾选的决定。
  it("uses the real searchable picker and requires explicit compatibility before save", async () => {
    await render(1024);
    await click(host.querySelector('[aria-label="Embedding 向量模型"]'));
    expect(document.querySelector('[placeholder="筛选模型、供应商或账号…"]')).not.toBeNull();
    const option = [...document.querySelectorAll('[role="option"]')].find((item) => item.textContent?.includes("Account B"));
    await click(option ?? null);
    expect(rpc.preflight).not.toHaveBeenCalled(); expect(rpc.save).not.toHaveBeenCalled();
    await click(button("检查变更"));
    expect(rpc.preflight).toHaveBeenCalledWith(candidate("B").route, 1024);
    expect(button("确认保存").disabled).toBe(true);
    expect([...host.querySelectorAll<HTMLInputElement>('input[type="radio"]')].every((input) => !input.checked)).toBe(true);
    await click(host.querySelector('input[type="radio"]'));
    await click(button("确认保存"));
    expect(rpc.save).toHaveBeenCalledWith({ consumerProfileId: "vce", operation: "embedding_create", route: candidate("B").route,
      embeddingDimensions: 1024, vceEmbeddingConfirmation: { preflightRevision: "review-revision",
        decisions: [{ projectId: "project-a", decision: "compatible" }] } });
    expect(host.querySelector('[role="alert"]')?.textContent).toContain("预检已过期");
    expect(host.querySelector('[aria-label="模型变更确认"]')).toBeNull(); expect(saved).not.toHaveBeenCalled();
    expect(testModel).not.toHaveBeenCalled(); expect(busy).toHaveBeenLastCalledWith(false);
  });
  // Cancelling a populated compatibility review must neither save nor retain a checked decision.
  // 取消已填写的兼容审查不得保存，也不得保留勾选决定。
  // No inputs or output; the real picker and cancel button operate on an unsaved route draft.
  // 无输入或输出；真实选择器和取消按钮操作未保存的路由草稿。
  it("cancels reviewed compatibility without saving and requires fresh decisions on reopening", async () => {
    await render(1024);
    await click(host.querySelector('[aria-label="Embedding 向量模型"]'));
    // The candidate is selected from the actual searchable popup by its unique account identity.
    // 按唯一账号身份从实际可搜索弹窗选择候选。
    const option = [...document.querySelectorAll('[role="option"]')].find((item) => item.textContent?.includes("Account B"));
    if (!option) throw new Error("Account B fixture candidate is missing");
    await click(option);
    await click(button("检查变更"));
    await click(host.querySelector('input[type="radio"]'));
    expect(button("确认保存").disabled).toBe(false);
    await click(button("取消"));
    expect(host.querySelector('[aria-label="模型变更确认"]')).toBeNull();
    expect(rpc.save).not.toHaveBeenCalled();
    expect(saved).not.toHaveBeenCalled();
    expect(testModel).not.toHaveBeenCalled();
    await click(button("检查变更"));
    expect(button("确认保存").disabled).toBe(true);
    expect([...host.querySelectorAll<HTMLInputElement>('input[type="radio"]')].every((input) => !input.checked)).toBe(true);
    await click(button("取消"));
    await click(button("还原已保存设置"));
    expect(host.querySelector('[aria-label="向量维度"]')?.textContent).toBe("1024");
    expect(rpc.save).not.toHaveBeenCalled();
    expect(busy).toHaveBeenLastCalledWith(false);
  });

  // Existing configured routes can be reviewed again after a pending compatibility decision.
  // 已配置路由在兼容决定待定后可以再次检查。
  it("permits rechecking an unchanged model and exposes only rebuild for hard incompatibility", async () => {
    rpc.preflight.mockResolvedValue(preflight("A", true));
    await render(2048);
    await click(button("检查项目兼容性"));
    expect(host.querySelectorAll('input[type="radio"]')).toHaveLength(1);
    expect(host.textContent).not.toContain("确认同一向量空间");
    expect(button("确认保存").disabled).toBe(true);
    expect(rpc.save).not.toHaveBeenCalled();
  });
  // Clearing is a draft and explicit removal confirmation, not an immediate engine mutation.
  // 清除属于草稿及明确移除确认，不立即修改引擎。
  it("keeps clearing reversible until the explicit save", async () => {
    await render(1024);
    await click(button("清除"));
    expect(rpc.save).not.toHaveBeenCalled();
    await click(button("还原已保存设置"));
    expect(host.querySelector('[aria-label="向量维度"]')?.textContent).toBe("1024");
    await click(button("清除"));
    rpc.preflight.mockResolvedValue({ candidate: null, projects: [], revision: "remove-review" } satisfies VceEmbeddingPreflight);
    await click(button("检查变更"));
    expect(rpc.preflight).toHaveBeenCalledWith(null, undefined);
    await click(button("确认保存"));
    expect(rpc.save).toHaveBeenCalledWith(expect.objectContaining({ route: null, embeddingDimensions: undefined,
      vceEmbeddingConfirmation: { preflightRevision: "remove-review", decisions: [] } }));
    expect(testModel).not.toHaveBeenCalled();
  });
  // External committed changes invalidate delayed preflight results and release the exact mutation gate.
  // 外部已提交更改使延迟预检结果失效，并释放精确修改闸门。
  it("ignores a delayed review for superseded saved settings", async () => {
    let resolve!: (value: VceEmbeddingPreflight) => void;
    rpc.preflight.mockReturnValue(new Promise<VceEmbeddingPreflight>((complete) => { resolve = complete; }));
    await render(1024); await click(button("检查项目兼容性"));
    await render(2048);
    await act(async () => resolve(preflight()));
    expect(host.querySelector('[aria-label="向量维度"]')?.textContent).toBe("2048");
    expect(host.querySelector('[aria-label="模型变更确认"]')).toBeNull();
    expect(rpc.save).not.toHaveBeenCalled(); expect(busy).toHaveBeenLastCalledWith(false);
  });
});
