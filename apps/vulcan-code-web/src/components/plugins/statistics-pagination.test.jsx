// @vitest-environment jsdom
import React, { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { createPluginPanel } from "../../../../../plugins/vulcan-usage-statistics/ui/index.mjs";

// Keep the real plugin state machine while replacing only host design-system presentation.
// 保留真实插件状态机，仅替换宿主设计系统展示。
const h = React.createElement;
const components = {
  Button: ({ children, onClick, disabled, type, ...rest }) => h("button", { onClick, disabled, type, "aria-pressed": rest["aria-pressed"] }, children),
  Badge: ({ children }) => h("span", null, children),
  StatusPill: ({ label }) => h("span", null, label),
  PageTabs: ({ ariaLabel, options, onValueChange, actions }) => h("div", null, h("nav", { "aria-label": ariaLabel }, ...options.map((option) => h("button", { key: option.value, type: "button", disabled: option.disabled, onClick: () => onValueChange(option.value) }, option.label))), actions),
  EmptyState: ({ title }) => h("span", null, title),
  ErrorState: ({ title }) => h("span", null, title),
  SelectControl: ({ options, value, onValueChange }) => h("select", { value, onChange: (event) => onValueChange(event.target.value) }, options.map((option) => h("option", { key: option.value, value: option.value }, option.label))),
  Dialog: () => null,
  MetricCard: () => null,
  ActivityHeatmap: () => null,
  ActivityHeatmapLegend: () => null,
  TimeSeriesLineChart: () => null,
  ChartLegend: () => null,
  HelpTooltip: () => null,
};
// Fixture data follows the full dashboard contract before isolated page responses are applied.
// 夹具在应用独立分页响应前遵循完整仪表盘契约。
const dashboard = {
  view: "details", healthStatus: "ready", generatedAtMs: 123456, aggregateRevision: 1,
  daily: [], dailyCount: 200, recentAttempts: [], recentCount: 100, consumerIds: [],
  heatmap: [], series: [], accounts: [],
  pricingCatalog: { currency: "USD", entryCount: 0, overrideCount: 0 },
  summary: { attempts: 0, successes: 0, reported: 0, totalTokens: 0, inputTokens: 0, outputTokens: 0, cacheReadTokens: 0, cacheWriteTokens: 0, costs: [] },
};
// Each test owns an isolated DOM root and manually completed host requests.
// 每项测试持有隔离 DOM 根节点及手动完成的宿主请求。
let root;
let host;
let requests;
beforeEach(async () => {
  vi.useFakeTimers();
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  host = document.createElement("div"); document.body.append(host); root = createRoot(host);
  requests = [];
  // Host calls never access a service, provider, real settings or user statistics.
  // 宿主调用不访问服务、供应商、真实设置或用户统计。
  const invoke = (action, args) => {
    if (action === "settings-get") return Promise.resolve({ modelOverrides: {}, catalogModels: [] });
    if (action === "dashboard-get" && ["overview", "details"].includes(args.view)) return Promise.resolve({ ...dashboard, view: args.view });
    return new Promise((resolve, reject) => requests.push({ action, args, resolve, reject }));
  };
  const Panel = createPluginPanel({ React, components, locale: "en", t: (_, fallback) => fallback, invoke });
  await act(async () => root.render(h(Panel)));
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); vi.useRealTimers(); vi.unstubAllGlobals(); });

// Click one visible labeled control and flush the production effects.
// 点击一个可见的有标签控件并执行生产 Effect。
// label/scope identify the exact control; returns after React settles synchronous work.
// label/scope 标识精确控件；在 React 完成同步工作后返回。
async function click(label, scope = host) {
  const button = [...scope.querySelectorAll("button")].find((entry) => entry.textContent === label && !entry.closest("[hidden]"));
  expect(button).toBeTruthy();
  await act(async () => button.click());
}

it("requests isolated table pages, retains neighboring cards and rejects stale category responses", async () => {
  await click("details");
  // Capture mounted cards so a whole-dashboard clear cannot masquerade as local refresh.
  // 捕获已挂载卡片，防止整份仪表盘清空伪装成局部刷新。
  const [daily, recent] = [...host.querySelectorAll(".plugin-statistics-paged-card")].filter((card) => !card.hidden);
  await click("Embedding models", recent);
  expect(requests[0].args.view).toBe("recent-page");
  expect(daily.hidden).toBe(false);
  expect(daily.getAttribute("aria-busy")).toBe("false");
  await click("Reranking models", recent);
  expect(requests[1].args.recentModelType).toBe("rerank");
  await act(async () => requests[1].resolve({ recentAttempts: [], recentCount: 75 }));
  await act(async () => requests[0].resolve({ recentAttempts: [], recentCount: 999 }));
  expect(recent.textContent).toContain("0 / 75");
  expect(recent.textContent).not.toContain("999");
  await click("Next", daily);
  expect(requests[2].args.view).toBe("daily-page");
  expect(requests[2].args.dailyOffset).toBe(100);
  expect(recent.getAttribute("aria-busy")).toBe("false");
  await act(async () => requests[2].reject(new Error("page read failed")));
  expect(daily.textContent).toContain("page read failed");
  expect(recent.textContent).not.toContain("page read failed");
  await click("Retry", daily);
  await act(async () => requests[3].resolve({ daily: [], dailyCount: 150 }));
  expect(daily.textContent).toContain("100 / 150");
  await click("Embedding models", recent);
  expect(requests).toHaveLength(5);
  await click("Reranking models", recent);
  expect(requests).toHaveLength(5);
  expect(recent.getAttribute("aria-busy")).toBe("false");
  await act(async () => requests[4].resolve({ recentAttempts: [], recentCount: 999 }));
  expect(recent.textContent).toContain("0 / 75");
});

it("keeps pricing rows and committed page numbers during loading, failure and retry", async () => {
  await click("Model pricing");
  await act(async () => vi.advanceTimersByTime(200));
  // Distinct rows identify the exact committed pricing page through asynchronous replacements.
  // 不同记录在异步替换期间标识精确的已提交定价页。
  const prices = (name) => ({ totalCount: 60, currency: "USD", models: [{ modelId: name, source: "unpriced", prices: null }] });
  await act(async () => requests[0].resolve(prices("first-page-model")));
  const pricing = host.querySelector('.plugin-statistics-paged-card:not([hidden])');
  const firstRow = pricing.querySelector("tbody tr");
  await click("Next", pricing);
  expect(pricing.querySelector("tbody tr")).toBe(firstRow);
  expect(pricing.textContent).toContain("1–25 / 60");
  await act(async () => vi.advanceTimersByTime(200));
  expect(requests[1].args.offset).toBe(25);
  await act(async () => requests[1].reject(new Error("pricing read failed")));
  expect(pricing.querySelector("tbody tr")).toBe(firstRow);
  await click("Retry", pricing);
  await act(async () => vi.advanceTimersByTime(200));
  await act(async () => requests[2].resolve(prices("second-page-model")));
  expect(pricing.textContent).toContain("second-page-model");
  expect(pricing.textContent).toContain("26–50 / 60");
  expect(pricing.textContent).not.toContain("first-page-model");
});

// Exercise the actual table renderers with operation identities, independent of model naming.
// 使用操作标识验证真实表格渲染，不依赖模型名称判断类型。
it("hides token generation timing for embeddings and reranking in both tables", async () => {
  // Nonzero token timing deliberately proves that unsupported metrics are hidden even in historical records.
  // 故意提供非零 Token 计时，证明历史记录即使含值也不会显示不适用指标。
  const rows = ["embedding_create", "rerank_documents", "conversation_respond"].map((operation) => ({
    providerId: "provider", serviceEntryId: "entry", consumerId: "consumer", modelId: operation,
    operation, attemptId: operation, terminalAtMs: 123456, status: "succeeded",
    attemptCount: 1, successfulAttemptCount: 1,
    inputTokens: 10, uncachedInputTokens: 10, outputTokens: null, cacheReadTokens: null,
    cacheWriteTokens: null, reasoningTokens: null, totalTokens: 10,
    requestDurationMs: 250, firstTokenLatencyMs: 100, outputDurationMs: 150,
    outputTokensPerSecond: 20, timedAttemptCount: 1,
    estimatedCost: null, estimatedCurrency: null, pricingDetails: null, pricingMatchStatus: "unpriced",
  }));
  await click("details");
  // These stable cards receive independent aggregate and attempt responses through the real host boundary.
  // 通过真实宿主边界向稳定卡片分别提供聚合和请求响应。
  const [daily, recent] = [...host.querySelectorAll(".plugin-statistics-paged-card")].filter((card) => !card.hidden);
  await click("Next", daily);
  await act(async () => requests[0].resolve({ daily: rows, dailyCount: 103 }));
  await click("Embedding models", recent);
  await act(async () => requests[1].resolve({ recentAttempts: rows, recentCount: 3 }));
  for (const card of [daily, recent]) {
    // Each table must retain duration for every operation while only conversation requests expose token timing.
    // 每张表都必须保留全部操作的耗时，仅对话请求展示 Token 计时。
    const cells = card.querySelectorAll(".plugin-statistics-latency");
    expect(cells).toHaveLength(3);
    for (const cell of [cells[0], cells[1]]) {
      expect(cell.textContent).toContain("0.250s");
      expect(cell.textContent).not.toContain("First token");
      expect(cell.textContent).not.toContain("t/s");
    }
    expect(cells[2].textContent).toContain("First token");
    expect(cells[2].textContent).toContain("20.0 t/s");
  }
});
