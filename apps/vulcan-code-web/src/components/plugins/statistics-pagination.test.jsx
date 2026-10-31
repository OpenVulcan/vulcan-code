// @vitest-environment jsdom
import React, { act } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { createPluginPanel } from "../../../../../plugins/vulcan-usage-statistics/ui/index.mjs";
import * as pageKit from "@/components/page";

// Keep the real plugin state machine while replacing only host design-system presentation.
// 保留真实插件状态机，仅替换宿主设计系统展示。
const h = React.createElement;
const components = {
  // The real page kit renders sections, tables and notices so selectors follow the production structure.
  // 真实页面套件渲染区块、表格与提示，使选择器遵循生产结构。
  ...pageKit,
  Button: ({ children, onClick, disabled, type, ...rest }) => h("button", { onClick, disabled, type, "aria-pressed": rest["aria-pressed"] }, children),
  Badge: ({ children }) => h("span", null, children),
  Input: ({ density, ...props }) => h("input", props),
  Checkbox: () => null,
  StatusPill: ({ label }) => h("span", null, label),
  PageTabs: ({ ariaLabel, options, onValueChange, actions }) => h("div", null, h("nav", { "aria-label": ariaLabel }, ...options.map((option) => h("button", { key: option.value, type: "button", disabled: option.disabled, onClick: () => onValueChange(option.value) }, option.label))), actions),
  StreamPlaceholder: ({ label }) => (label ? h("span", { role: "status" }, label) : null),
  SelectControl: ({ ariaLabel, options, value, onValueChange }) => h("select", { "aria-label": ariaLabel, value, onChange: (event) => onValueChange(event.target.value) }, options.map((option) => h("option", { key: option.value, value: option.value }, option.label))),
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
    // Whole-tab views answer at once; only paged table projections stay pending for the test to settle.
    // 整页视图立即返回；只有分页表格投影保持挂起，由测试决定其结果。
    if (action === "dashboard-get" && ["overview", "providers", "recent"].includes(args.view)) return Promise.resolve({ ...dashboard, view: args.view, providers: [] });
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

// section returns the visible page-kit region titled label.
// section 返回标题为 label 的可见页面套件区域。
function section(label) {
  const region = [...host.querySelectorAll('[role="region"]')].find((entry) => entry.querySelector(":is(h2, h3, h4)")?.textContent === label && !entry.closest("[hidden]"));
  expect(region).toBeTruthy();
  return region;
}

// choose selects one option of the labelled select inside scope and flushes the production effects.
// choose 在 scope 内选择带标签下拉框的一个选项，并执行生产 Effect。
async function choose(label, value, scope = host) {
  const select = scope.querySelector(`select[aria-label="${label}"]`);
  expect(select).toBeTruthy();
  await act(async () => { select.value = value; select.dispatchEvent(new Event("change", { bubbles: true })); });
}

it("requests isolated table pages and rejects stale category responses", async () => {
  await click("recent");
  // Capture the mounted card so a whole-dashboard clear cannot masquerade as local refresh.
  // 捕获已挂载卡片，防止整份仪表盘清空伪装成局部刷新。
  const recent = section("Recent physical attempts");
  await choose("Model types", "embedding", recent);
  expect(requests[0].args.view).toBe("recent-page");
  await choose("Model types", "rerank", recent);
  expect(requests[1].args.recentModelType).toBe("rerank");
  await act(async () => requests[1].resolve({ recentAttempts: [], recentCount: 75 }));
  await act(async () => requests[0].resolve({ recentAttempts: [], recentCount: 999 }));
  expect(recent.textContent).toContain("0 / 75");
  expect(recent.textContent).not.toContain("999");
  // Leaving and returning to the committed category supersedes the pending request without another one,
  // and its late response is ignored.
  // 离开再回到已提交分类会取代挂起请求而不再发新请求，其迟到的响应被忽略。
  await choose("Model types", "embedding", recent);
  expect(requests).toHaveLength(3);
  await choose("Model types", "rerank", recent);
  expect(requests).toHaveLength(3);
  expect(recent.getAttribute("aria-busy")).toBe("false");
  await act(async () => requests[2].resolve({ recentAttempts: [], recentCount: 999 }));
  expect(recent.textContent).toContain("0 / 75");
});

it("keeps a failed daily page inside its own section and retries it", async () => {
  await click("providers");
  const daily = section("Model usage");
  await click("Next", daily);
  expect(requests[0].args.view).toBe("daily-page");
  expect(requests[0].args.dailyOffset).toBe(100);
  expect(daily.getAttribute("aria-busy")).toBe("true");
  await act(async () => requests[0].reject(new Error("page read failed")));
  expect(daily.textContent).toContain("page read failed");
  await click("Retry", daily);
  await act(async () => requests[1].resolve({ daily: [], dailyCount: 150 }));
  expect(daily.textContent).toContain("100 / 150");
  expect(daily.textContent).not.toContain("page read failed");
});

it("keeps pricing rows and committed page numbers during loading, failure and retry", async () => {
  await click("Model pricing");
  await act(async () => vi.advanceTimersByTime(200));
  // Distinct rows identify the exact committed pricing page through asynchronous replacements.
  // 不同记录在异步替换期间标识精确的已提交定价页。
  const prices = (name) => ({ totalCount: 60, currency: "USD", models: [{ modelId: name, source: "unpriced", prices: null }] });
  await act(async () => requests[0].resolve(prices("first-page-model")));
  const pricing = section("Model reference prices");
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
  // Each tab's card receives its own aggregate or attempt response through the real host boundary.
  // 每个标签页的卡片通过真实宿主边界分别接收聚合或请求响应。
  // expectTiming checks one rendered table; card is the section that owns it.
  // expectTiming 检查一张已渲染表格；card 是其所属区块。
  const expectTiming = (card) => {
    // Each table must retain duration for every operation while only conversation requests expose token timing.
    // 每张表都必须保留全部操作的耗时，仅对话请求展示 Token 计时。
    const cells = card.querySelectorAll(".plugin-statistics-latency");
    expect(cells).toHaveLength(3);
    for (const cell of [cells[0], cells[1]]) {
      expect(cell.textContent).toContain("0.250s");
      expect(cell.textContent).not.toContain("First token");
      expect(cell.textContent).not.toContain("tokens/s");
    }
    expect(cells[2].textContent).toContain("First token");
    expect(cells[2].textContent).toContain("20.0 tokens/s");
  };
  await click("providers");
  const daily = section("Model usage");
  await click("Next", daily);
  await act(async () => requests[0].resolve({ daily: rows, dailyCount: 103 }));
  expectTiming(daily);
  await click("recent");
  const recent = section("Recent physical attempts");
  await choose("Model types", "embedding", recent);
  await act(async () => requests[1].resolve({ recentAttempts: rows, recentCount: 3 }));
  expectTiming(recent);
});
