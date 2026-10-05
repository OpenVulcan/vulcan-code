import assert from "node:assert/strict";
import test from "node:test";
import { createRequire } from "node:module";
import { readFileSync } from "node:fs";
import { createPluginPanel, formatMoney, presetTimeRange } from "./index.mjs";
import { createPluginQueryClient, invalidatePluginQueries } from "../../../apps/vulcan-code-web/src/service/plugin-ui-query-runtime.ts";
import { SUPPORTED_LOCALES } from "../../../apps/vulcan-code-web/src/i18n-locales.ts";

// require resolves the existing host test dependencies without adding a plugin bundle dependency.
// require 解析现有宿主测试依赖，不向插件包添加依赖。
const require = createRequire(new URL("../../../apps/vulcan-code-web/package.json", import.meta.url));
// React and JSDOM provide the real host renderer and an isolated browser document.
// React 与 JSDOM 提供真实宿主渲染器与隔离浏览器文档。
const React = require("react");
const { JSDOM } = require("jsdom");
// load parses actual plugin manifests and catalogs through the existing host test dependency.
// load 通过现有宿主测试依赖解析真实插件清单与词表。
const { load } = require("js-yaml");

// Select one real calendar shortcut and confirm the dialog's exact interval.
// 选择真实自然日快捷项，并确认对话框中的精确区间。
// panel owns the mounted DOM; label identifies the shortcut; resolves after commit.
// panel 持有挂载 DOM；label 标识快捷项；提交后完成。
async function selectRange(panel, label) {
  await React.act(async () => panel.container.querySelector('button[aria-label="Time range"]').click());
  await React.act(async () => [...panel.container.querySelectorAll("button")].find(button => button.textContent === label).click());
  await React.act(async () => [...panel.container.querySelectorAll("button")].find(button => button.textContent === "Confirm").click());
}

test("money preserves unknown, zero, tiny values, carry and large exact digits", () => {
  for (const [input, expected] of [
    [null, "-"], ["", "-"], ["NaN", "-"], ["-1", "-"], ["1e3", "-"],
    ["0", "0.00"], ["0.000000", "0.00"], ["0.00000001", "<0.0001"],
    ["0.000365468432134", "≈0.0004"], ["0.1", "0.1"],
    ["0.99999", "≈1.00"], ["1.2300", "1.23"],
    ["123456789012345678901.125", "≈123,456,789,012,345,678,901.13"],
  ]) assert.equal(formatMoney(input, "en-US"), expected, String(input));
});

// Check special branches as well as exact large values against comma-decimal locales.
// 对逗号小数语言验证特殊分支和精确大金额，避免常规分支通过而零值回退英文格式。
test("money localizes zero, tiny values and exact rounding without losing stored digits", () => {
  for (const locale of ["fr-FR", "de-DE", "es-ES", "pt-BR", "ru-RU"]) {
    assert.equal(formatMoney("0", locale), "0,00", locale);
    assert.equal(formatMoney("0.00000001", locale), "<0,0001", locale);
    assert.equal(formatMoney("0.99999", locale), "≈1,00", locale);
    assert.equal(formatMoney("123456789012345678901.125", locale).replace(/[.\s\u00a0\u202f]/g, ""), "≈123456789012345678901,13", locale);
  }
});

// Mount the actual plugin against an isolated DOM and controlled action responses.
// 将真实插件挂载到隔离 DOM 与受控 Action 响应。
// invoke handles declared actions; strict controls StrictMode; locale and messages project host localization.
// invoke 处理声明的 Action；strict 控制 StrictMode；locale 与 messages 投影宿主本地化。
// Returns the container and asynchronous cleanup.
// 返回容器与异步清理函数。
async function mount(invoke, strict = false, locale = "en-US", messages = {}) {
  // Use the real host coordinator so StrictMode tests exercise the shared subscription contract.
  // 使用真实宿主协调器，让 StrictMode 测试覆盖共享订阅契约。
  invalidatePluginQueries("usage-test");
  const client = createPluginQueryClient({ pluginId: "usage-test", runtimeIdentity: crypto.randomUUID(), workspacePath: null, conversationId: null },
    ["settings-get", "dashboard-get", "pricing-get", "statistics-clear-preview", "settings-update", "statistics-clear-apply"].map(id => ({ id, effect: id.endsWith("update") || id.endsWith("apply") ? "mutation" : "read", resultCacheMs: 0 })), invoke);
  // dom owns only this test's browser environment.
  // dom 仅持有当前测试的浏览器环境。
  const dom = new JSDOM("<!doctype html><html><body><div id='root'></div></body></html>");
  globalThis.window = dom.window;
  globalThis.document = dom.window.document;
  globalThis.IS_REACT_ACT_ENVIRONMENT = true;
  // plain and Button preserve children and the native interaction semantics of host components.
  // plain 与 Button 保留宿主组件的子节点和原生交互语义。
  const plain = ({ children }) => React.createElement("div", null, children);
  const Button = ({ children, variant, size, ...props }) => React.createElement("button", props, children);
  // Expose exact projected calendar labels without recreating the host's visual layout.
  // 暴露准确投影的日历名称，不重新实现宿主视觉布局。
  const ActivityHeatmap = ({ cells }) => React.createElement("div", { "data-test-heatmap": true }, cells.map((cell) => React.createElement("span", { key: cell.date, "aria-label": cell.label })));
  // SelectControl preserves the host's declared value-change contract.
  // SelectControl 保留宿主声明的值变更契约。
  const SelectControl = ({ ariaLabel, value, options, onValueChange, disabled }) => React.createElement("select", {
    "aria-label": ariaLabel, value, disabled, onChange: (event) => onValueChange(event.currentTarget.value),
  }, options.map((option) => React.createElement("option", { key: option.value, value: option.value }, option.label)));
  // PageTabs keeps each tab a native button whose text is the exact label, plus the trailing actions.
  // PageTabs 使每个标签保持为文本即精确标签的原生按钮，并保留尾部操作。
  const PageTabs = ({ ariaLabel, options, onValueChange, actions }) => React.createElement("div", null, React.createElement("nav", { "aria-label": ariaLabel },
    ...options.map((option) => React.createElement("button", { key: option.value, type: "button", disabled: option.disabled, onClick: () => onValueChange(option.value) }, option.label))),
    actions);
  // Page kit stubs keep the semantic landmarks the plugin relies on: sections, notices, tables and lazily mounted panels.
  // 页面套件桩保留插件依赖的语义结构：区块、提示、表格与延迟挂载的面板。
  const PageSection = ({ title, description, status, actions, footer, children }) => React.createElement("section", { "data-page-section": "" },
    React.createElement("h3", null, title), description, status, actions, children, footer);
  const PageNotice = ({ tone, title, actions, details, children }) => React.createElement("div", { role: tone === "error" ? "alert" : "status" },
    title, children, details ? React.createElement("pre", null, details) : null, actions);
  const PageTabPanel = ({ active, children }) => {
    const [visited, setVisited] = React.useState(active);
    if (active && !visited) setVisited(true);
    return active || visited ? React.createElement("div", { hidden: !active }, children) : null;
  };
  const PageTable = ({ columns, rows, empty }) => rows.length === 0 && empty !== undefined ? empty : React.createElement("table", null,
    React.createElement("tbody", null, rows.map((row) => React.createElement("tr", { key: row.key, "aria-selected": row.selected || undefined },
      columns.map((column) => React.createElement("td", { key: column.key }, row.cells[column.key]))))));
  const PageFacts = ({ items, className }) => React.createElement("dl", { className },
    items.map((item) => React.createElement(React.Fragment, { key: item.key }, React.createElement("dt", null, item.label), React.createElement("dd", null, item.value))));
  const PageToolbar = ({ actions, children }) => React.createElement("div", null, children, actions);
  const Input = ({ density, ...props }) => React.createElement("input", props);
  const Checkbox = ({ checked, onCheckedChange }) => React.createElement("input", { type: "checkbox", checked, onChange: (event) => onCheckedChange(event.currentTarget.checked) });
  // sdk supplies only the formally used host surface.
  // sdk 仅提供正式使用的宿主接口。
  const sdk = {
    React, locale, invoke: client.invoke, t: (key, fallback) => messages[key] ?? fallback,
    components: { PageBody: plain, PageToolbar, PageTabs, PageTabPanel, PageSection, PageHint: plain, PageNotice, PageEmptyState: ({ title }) => React.createElement("p", null, title),
      PageFacts, PageTable, MetricCard: plain, MetricGrid: plain, Badge: plain, Button, Input, Checkbox, SelectControl, StatusPill: plain, StreamPlaceholder: plain,
      ActivityHeatmap, ActivityHeatmapLegend: plain, TimeSeriesLineChart: plain, ChartLegend: plain,
      HelpTooltip: ({ ariaLabel }) => React.createElement("button", { "aria-label": ariaLabel }),
      Dialog: ({ open, children }) => open ? React.createElement("div", null, children) : null,
      DialogContent: plain, DialogHeader: plain, DialogTitle: plain, DialogDescription: plain, DialogFooter: plain },
  };
  // root mounts the real hook-based plugin component.
  // root 挂载真实的基于 Hook 的插件组件。
  const root = require("react-dom/client").createRoot(document.getElementById("root"));
  await React.act(async () => root.render(React.createElement(strict ? React.StrictMode : React.Fragment, null,
    React.createElement(createPluginPanel(sdk)))));
  return { container: document.getElementById("root"), close: async () => {
    await React.act(async () => root.unmount());
    client.dispose();
    dom.window.close();
  } };
}

// settings is an exact synthetic catalog with one deliberately zero-priced cache dimension.
// settings 是一个精确合成目录，其中缓存维度有明确零价格。
const settings = { modelOverrides: {}, catalogModels: [{ modelId: "fixture-model", prices: {
  inputPerMillion: "1", outputPerMillion: "2", cacheReadPerMillion: "0", cacheWritePerMillion: "0",
} }] };

// pricingDirectory is a complete declared page, independent from dashboard availability.
// pricingDirectory 是完整声明分页，与仪表盘是否可用无关。
const pricingDirectory = { currency: "USD", totalCount: 1, models: [{ modelId: "fixture-model", attempts: 0, source: "catalog", prices: settings.catalogModels[0].prices }] };

// openPricing follows the real task navigation and allows its debounced directory read to settle.
// openPricing 遵循真实任务导航，并等待防抖目录读取完成。
// panel is the mounted plugin fixture; returns after the directory page has resolved.
// panel 是已挂载插件夹具；目录分页完成后返回。
async function openPricing(panel) {
  await React.act(async () => panel.container.querySelector("nav button:nth-child(3)").click());
  await React.act(async () => new Promise((resolve) => window.setTimeout(resolve, 250)));
}

test("price configuration remains usable while the database action is pending", async () => {
  // dashboardPromise intentionally remains pending until after configuration is inspected.
  // dashboardPromise 在检查配置前保持待定。
  const dashboardPromise = new Promise(() => {});
  const panel = await mount((action) => action === "settings-get" ? Promise.resolve(settings) : action === "pricing-get" ? Promise.resolve(pricingDirectory) : dashboardPromise);
  assert.equal(panel.container.querySelectorAll('.plugin-statistics-price-editor input').length, 0);
  await openPricing(panel);
  assert.match(panel.container.textContent, /fixture-model/);
  await React.act(async () => [...panel.container.querySelectorAll("button")].find((button) => button.textContent === "fixture-model").click());
  assert.equal([...panel.container.querySelectorAll("button")].find((button) => button.textContent === "Save override").disabled, true);
  await panel.close();
});

test("a database failure does not discard a loaded price catalog", async () => {
  const panel = await mount((action) => action === "settings-get" ? Promise.resolve(settings) : action === "pricing-get" ? Promise.resolve(pricingDirectory) : Promise.reject(new Error("isolated query failure")));
  await openPricing(panel);
  assert.match(panel.container.textContent, /fixture-model/);
  assert.match(panel.container.textContent, /isolated query failure/);
  await panel.close();
});

// dashboard supplies a schema-shaped empty snapshot with visible exact revision evidence.
// dashboard 提供符合 Schema 形状的空快照，并带可见精确修订证据。
const dashboard = {
  generatedAtMs: Date.now(), schemaRevision: 1, aggregateRevision: 1, healthStatus: "ready",
  consumerIds: ["webui"], dailyCount: 0, recentCount: 0,
  daily: [], recentAttempts: [], accounts: [], heatmap: [], series: [],
  summary: { attempts: 0, successes: 0, reported: 0, totalTokens: 0,
    inputTokens: null, outputTokens: null, cacheReadTokens: null, cacheWriteTokens: null,
    costs: [{ currency: "USD", amount: null, pricedRows: 0, incompleteRows: 0, unpricedRows: 0 }] },
  pricingCatalog: { currency: "USD", entryCount: 1, overrideCount: 0 },
};

// Locale affects projected text only; arithmetic, stored facts and CSS proportions stay exact.
// 语言只影响投影文本；计算、存储事实和 CSS 占比保持精确。
test("French host locale formats percentage spacing, cache share and translated panel labels", async () => {
  // messages is the declared host translation projection, independent from plugin calculations.
  // messages 为声明的宿主译文投影，与插件计算相互独立。
  const messages = {
    "sections.overview": "Vue d’ensemble", "sections.details": "Détails d’utilisation",
    "tokens.composition": "Entrée {{input}} · Sortie {{output}}",
    "heatmap.tooltip": "{{date}} : {{value}} tokens · {{requests}} requêtes · {{successRate}} de réussite · {{model}}",
  };
  const panel = await mount((action) => Promise.resolve(action === "settings-get" ? settings : {
    ...dashboard, summary: { ...dashboard.summary, inputTokens: 1200, outputTokens: 300, cacheReadTokens: 400 },
    heatmap: [{ date: new Date().toLocaleDateString("en-CA"), value: 1500, requests: 3, successes: 2, topModelName: "fixture-model", topProviderName: "fixture-provider" }],
  }), false, "fr-FR", messages);
  try {
    assert.match(panel.container.textContent, /Vue d’ensemble/);
    assert.match(panel.container.textContent, /Détails d’utilisation/);
    const bar = panel.container.querySelector('.plugin-statistics-token-bar[role="img"]');
    assert.equal(bar.getAttribute("aria-label"), "Entrée 80,0\u00a0% · Sortie 20,0\u00a0%");
    assert.equal(bar.children[0].style.width, "80%");
    assert.equal(bar.children[1].style.width, "20%");
    assert.match(panel.container.querySelector(".plugin-statistics-cache-facts").textContent, /33,3\u00a0%/);
    const labels = [...panel.container.querySelectorAll('[data-test-heatmap] [aria-label]')].map((cell) => cell.getAttribute("aria-label"));
    assert.ok(labels.some((label) => label.includes("66,7\u00a0% de réussite")));
    assert.doesNotMatch(panel.container.textContent, /80\.0%|33\.3%/);
  } finally { await panel.close(); }
});

// Validate each current host language with its real manifest-selected catalog and mounted UI.
// 使用清单实际选择的词表及已挂载界面验证宿主当前每种语言。
test("every host locale renders real translations, percentage measurements and special money values", async () => {
  // manifest owns locale paths; the shared host registry owns the supported language set.
  // manifest 拥有语言路径；共享宿主注册表拥有支持语言集合。
  const manifest = load(readFileSync(new URL("../plugin.yaml", import.meta.url), "utf8"));
  for (const { code: locale } of SUPPORTED_LOCALES) {
    // catalog is the actual plugin resource selected for this language, including the shared English file.
    // catalog 为此语言实际选择的插件资源，包含共用的英文文件。
    const catalog = load(readFileSync(new URL(`../${manifest.locales[locale]}`, import.meta.url), "utf8"));
    // separator keeps the assertion independent from any English-only punctuation assumption.
    // separator 使断言不依赖仅适用英文的标点假设。
    const separator = new Intl.NumberFormat(locale).formatToParts(1.1).find((part) => part.type === "decimal").value;
    assert.equal(formatMoney("0", locale), `0${separator}00`, locale);
    assert.equal(formatMoney("0.00000001", locale), `<0${separator}0001`, locale);
    const panel = await mount((action) => Promise.resolve(action === "settings-get" ? settings : {
      ...dashboard, summary: { ...dashboard.summary, inputTokens: 1200, outputTokens: 300, cacheReadTokens: 400 },
      heatmap: [{ date: new Date().toLocaleDateString("en-CA"), value: 1500, requests: 3, successes: 2, topModelName: "fixture-model", topProviderName: "fixture-provider" }],
    }), false, locale, catalog.messages);
    try {
      assert.match(panel.container.textContent, new RegExp(catalog.messages["sections.overview"].replace(/[.*+?^${}()|[\]\\]/g, "\\$&")), locale);
      assert.ok([...panel.container.querySelectorAll("nav button")].some((button) => button.textContent === catalog.messages["sections.details"]), locale);
      // percent keeps the locale's exact whitespace and suffix in accessibility labels.
      // percent 在无障碍标签中保留语言精确的空白与后缀。
      const percent = new Intl.NumberFormat(locale, { style: "percent", minimumFractionDigits: 1, maximumFractionDigits: 1 });
      const expected = catalog.messages["tokens.composition"].replace("{{input}}", percent.format(0.8)).replace("{{output}}", percent.format(0.2));
      const bar = panel.container.querySelector('.plugin-statistics-token-bar[role="img"]');
      assert.equal(bar.getAttribute("aria-label"), expected, locale);
      assert.equal(bar.children[0].style.width, "80%", locale);
      assert.ok(panel.container.querySelector(".plugin-statistics-cache-facts").textContent.includes(percent.format(1 / 3)), locale);
      const labels = [...panel.container.querySelectorAll('[data-test-heatmap] [aria-label]')].map((cell) => cell.getAttribute("aria-label"));
      assert.ok(labels.some((label) => label.includes(percent.format(2 / 3))), locale);
    } finally { await panel.close(); }
  }
});

// Zero requests must remain distinct from measured requests that all failed.
// 零请求必须与已观测请求全部失败明确区分。
test("empty heatmap dates have no success-rate denominator", async () => {
  const panel = await mount((action) => Promise.resolve(action === "settings-get" ? settings : dashboard));
  try {
    const labels = [...panel.container.querySelectorAll('[data-test-heatmap] [aria-label]')].map((cell) => cell.getAttribute("aria-label"));
    assert.ok(labels.length > 0);
    for (const label of labels) {
      assert.match(label, /0 requests · — success/);
      assert.doesNotMatch(label, /0\.0% success/);
    }
  } finally { await panel.close(); }
});

test("StrictMode shares initial reads and older range results cannot replace newer results", async () => {
  // finishOld completes the first range only after a new range has rendered.
  // finishOld 仅在新范围渲染后完成首次范围请求。
  let finishOld;
  const oldRequest = new Promise((resolve) => { finishOld = resolve; });
  const calls = [];
  const panel = await mount((action, argumentsValue) => {
    calls.push({ action, argumentsValue });
    if (action === "settings-get") return Promise.resolve(settings);
    return argumentsValue.endAtMs === null ? oldRequest : Promise.resolve({ ...dashboard, aggregateRevision: 777 });
  }, true);
  assert.equal(calls.filter((call) => call.action === "settings-get").length, 1);
  assert.equal(calls.filter((call) => call.action === "dashboard-get").length, 1);
  await selectRange(panel, "7 days");
  assert.match(panel.container.textContent, /Revision 777/);
  await React.act(async () => finishOld({ ...dashboard, aggregateRevision: 111 }));
  assert.match(panel.container.textContent, /Revision 777/);
  assert.doesNotMatch(panel.container.textContent, /Revision 111/);
  await panel.close();
});

// Retain a proven snapshot only for the same filter identity, and label it when a refresh fails.
// 仅在筛选身份相同时保留已验证快照，并在刷新失败时注明。
test("same-filter refresh retains labelled previous results until a successful reread", async () => {
  let reads = 0;
  const panel = await mount((action) => {
    if (action === "settings-get") return Promise.resolve(settings);
    reads += 1;
    if (reads === 2) return Promise.reject(new Error("statistics changed during loading"));
    return Promise.resolve({ ...dashboard, aggregateRevision: reads === 1 ? 701 : 702 });
  });
  try {
    await React.act(async () => panel.container.querySelector('button[aria-label="Refresh"]').click());
    assert.match(panel.container.textContent, /Revision 701/);
    assert.match(panel.container.textContent, /Showing the previous result/);
    assert.match(panel.container.textContent, /statistics changed during loading/);
    await React.act(async () => panel.container.querySelector('button[aria-label="Refresh"]').click());
    assert.match(panel.container.textContent, /Revision 702/);
    assert.doesNotMatch(panel.container.textContent, /Revision 701|Showing the previous result|statistics changed during loading/);
  } finally { await panel.close(); }
});

// A late configuration write must refresh the current view and range, not the closure captured before saving.
// 晚到的配置写入必须刷新当前视图和范围，不能使用保存前捕获的闭包。
test("price-save completion refreshes the currently selected dashboard scope", async () => {
  let finishSave;
  const reads = [];
  const panel = await mount((action, args) => {
    if (action === "settings-get") return Promise.resolve(settings);
    if (action === "pricing-get") return Promise.resolve(pricingDirectory);
    if (action === "settings-update") return new Promise((resolve) => { finishSave = () => resolve({ ...settings, modelOverrides: args.modelOverrides }); });
    reads.push(args);
    return Promise.resolve({ ...dashboard, aggregateRevision: args.endAtMs === null ? 30 : 7 });
  });
  try {
    await openPricing(panel);
    await React.act(async () => [...panel.container.querySelectorAll("button")].find((button) => button.textContent === "fixture-model").click());
    // Enter a changed decimal through the same controlled input used by the actual price editor.
    // 通过实际价格编辑器的同一受控输入填写修改后的十进制值。
    const input = panel.container.querySelector('.plugin-statistics-price-editor input');
    const setter = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, "value").set;
    await React.act(async () => { setter.call(input, "1.2345"); input.dispatchEvent(new window.Event("input", { bubbles: true })); });
    await React.act(async () => [...panel.container.querySelectorAll("button")].find((button) => button.textContent === "Save override").click());
    assert.equal(typeof finishSave, "function");
    await React.act(async () => panel.container.querySelector("nav button:nth-child(2)").click());
    await selectRange(panel, "7 days");
    await React.act(async () => finishSave());
    assert.equal(reads.at(-1).startAtMs, presetTimeRange(7).startAtMs);
    assert.equal(typeof reads.at(-1).endAtMs, "number");
    assert.equal(reads.at(-1).view, "details");
    assert.match(panel.container.textContent, /Revision 7\b/);
    assert.doesNotMatch(panel.container.textContent, /Revision 30\b/);
  } finally { await panel.close(); }
});

// A retry must recover failed price configuration without depending on the statistics action.
// 重试必须恢复失败的价格配置，且不依赖统计 Action。
test("price settings retry recovers independently from a failed database", async () => {
  let settingsCalls = 0;
  const panel = await mount((action) => {
    if (action === "settings-get") {
      settingsCalls += 1;
      return settingsCalls === 1 ? Promise.reject(new Error("settings unavailable")) : Promise.resolve(settings);
    }
    if (action === "pricing-get") return Promise.resolve(pricingDirectory);
    return Promise.reject(new Error("database unavailable"));
  });
  assert.match(panel.container.textContent, /settings unavailable/);
  assert.match(panel.container.textContent, /database unavailable/);
  // Each failed read owns its retry; recover only the price settings notice.
  // 每个失败读取各自拥有重试；只恢复价格设置提示。
  const settingsNotice = [...panel.container.querySelectorAll('[role="alert"]')].find((notice) => notice.textContent.includes("settings unavailable"));
  await React.act(async () => [...settingsNotice.querySelectorAll("button")].find((button) => button.textContent === "Retry").click());
  assert.equal(settingsCalls, 2);
  assert.doesNotMatch(panel.container.textContent, /settings unavailable/);
  await openPricing(panel);
  assert.match(panel.container.textContent, /fixture-model/);
  assert.match(panel.container.textContent, /database unavailable/);
  await panel.close();
});

// Consumer selection must survive the deliberate removal of stale dashboard data during loading.
// 消费者选择必须在加载期间主动清除过期仪表盘数据时保持稳定。
test("consumer selection survives a pending filtered dashboard read", async () => {
  const calls = [];
  const panel = await mount((action, argumentsValue) => {
    if (action === "settings-get") return Promise.resolve(settings);
    calls.push(argumentsValue);
    return argumentsValue.consumerId === "webui" ? new Promise(() => {}) : Promise.resolve(dashboard);
  });
  await React.act(async () => {
    const select = panel.container.querySelector('select[aria-label="Consumer"]');
    select.value = "webui";
    select.dispatchEvent(new window.Event("change", { bubbles: true }));
  });
  assert.equal(calls.at(-1).consumerId, "webui");
  assert.equal(calls.length, 2);
  await panel.close();
});

// The pricing directory must use its declared full-history API rather than the current dashboard page.
// 定价目录必须使用声明的全历史接口，而不是当前仪表盘分页。
test("used-price directory reads and paginates the full server filter", async () => {
  const calls = [];
  const panel = await mount((action, argumentsValue) => {
    if (action === "settings-get") return Promise.resolve(settings);
    if (action === "dashboard-get") return Promise.resolve(dashboard);
    calls.push(argumentsValue);
    return Promise.resolve({ currency: "USD", totalCount: 30, models: [{
      modelId: argumentsValue.offset === 0 ? "history-only-1" : "history-only-26",
      attempts: 1, source: "unpriced", prices: null,
    }] });
  });
  await React.act(async () => panel.container.querySelector("nav button:nth-child(3)").click());
  await React.act(async () => new Promise((resolve) => window.setTimeout(resolve, 250)));
  await React.act(async () => {
    const select = panel.container.querySelector('select[aria-label="Price filter"]');
    select.value = "used";
    select.dispatchEvent(new window.Event("change", { bubbles: true }));
  });
  await React.act(async () => new Promise((resolve) => window.setTimeout(resolve, 250)));
  assert.deepEqual(calls.at(-1), { search: "", filter: "used", offset: 0 });
  assert.match(panel.container.textContent, /history-only-1/);
  assert.match(panel.container.textContent, /1–25 \/ 30/);
  await React.act(async () => [...panel.container.querySelectorAll("button")].find((button) => button.textContent === "Next" && !button.disabled).click());
  await React.act(async () => new Promise((resolve) => window.setTimeout(resolve, 250)));
  assert.deepEqual(calls.at(-1), { search: "", filter: "used", offset: 25 });
  assert.match(panel.container.textContent, /history-only-26/);
  await panel.close();
});

// Overview proportions use only the reported disjoint dimensions; unknown and true-zero input never generate a fabricated ratio.
// 概览比例仅使用已报告且互不重叠的维度；未知或真实零输入绝不生成编造的比例。
test("overview shows input/output proportions without adding overlapping cache counts", async () => {
  for (const [inputTokens, outputTokens, expectedLabel] of [[1200, 300, "Input 80.0% · Output 20.0%"], [null, 300, null], [0, 0, null]]) {
    const panel = await mount((action) => Promise.resolve(action === "settings-get" ? settings : { ...dashboard,
      summary: { ...dashboard.summary, inputTokens, outputTokens, cacheReadTokens: 400, cacheWriteTokens: 50, totalTokens: 1500 } }));
    try {
      const balance = panel.container.querySelector(".plugin-statistics-balance");
      assert.equal(balance.querySelectorAll(":scope > [data-page-section]").length, 2);
      const bar = balance.querySelector('[role="img"]');
      if (expectedLabel === null) assert.equal(bar, null);
      else {
        assert.equal(bar.getAttribute("aria-label"), expectedLabel);
        assert.equal(bar.children[0].style.width, "80%");
        assert.equal(bar.children[1].style.width, "20%");
        assert.match(balance.querySelector(".plugin-statistics-cache-facts").textContent, /400 · 33.3%/);
      }
      if (inputTokens === null) assert.match(balance.querySelector('[data-token-dimension="inputTokens"]').textContent, /-/);
      assert.doesNotMatch(balance.textContent, /NaN|Infinity/);
    } finally { await panel.close(); }
  }
});

// An explicit directory selection opens the editor before the table and retains exact unsaved input through filters.
// 明确目录选择在表格前打开编辑器，并在筛选过程中保留精确未保存输入。
test("pricing requires an explicit target, preserves its draft across filters and reports unpriced source", async () => {
  const saves = [];
  const panel = await mount((action, args) => {
    if (action === "settings-get") return Promise.resolve(settings);
    if (action === "dashboard-get") return Promise.resolve(dashboard);
    if (action === "settings-update") { saves.push(args); return Promise.resolve({ ...settings, modelOverrides: args.modelOverrides }); }
    return Promise.resolve({ currency: "USD", totalCount: args.search ? 0 : 1,
      models: args.search ? [] : [{ modelId: "unknown-model", attempts: 1, source: "unpriced", prices: null }] });
  });
  try {
    await openPricing(panel);
    assert.equal(panel.container.querySelector(".plugin-statistics-price-editor"), null);
    const table = [...panel.container.querySelectorAll("button")].find((button) => button.textContent === "unknown-model").closest("table");
    await React.act(async () => [...table.querySelectorAll("button")].find((button) => button.textContent === "unknown-model").click());
    const editor = panel.container.querySelector(".plugin-statistics-price-editor");
    assert.match(editor.textContent, /Edit model · unknown-model/);
    assert.match(editor.textContent, /Unpriced/);
    assert.doesNotMatch(editor.textContent, /Catalog default/);
    assert.equal([...editor.querySelectorAll("input")].every((input) => input.value === ""), true);
    assert.equal([...editor.querySelectorAll("button")].find((button) => button.textContent === "Save override").disabled, true);
    // setter uses native input editing so React receives the exact decimal string rather than a number.
    // setter 使用原生输入编辑，让 React 接收到精确十进制字符串而不是数字。
    const setter = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, "value").set;
    const decimals = ["0.0000000000000000000000001", "2.12345678901234567890", "0", "1.50"];
    await React.act(async () => [...editor.querySelectorAll("input")].forEach((input, index) => {
      setter.call(input, decimals[index]); input.dispatchEvent(new window.Event("input", { bubbles: true }));
    }));
    await React.act(async () => {
      const search = panel.container.querySelector('input[aria-label="Search models"]');
      setter.call(search, "no-match"); search.dispatchEvent(new window.Event("input", { bubbles: true }));
    });
    await React.act(async () => new Promise((resolve) => window.setTimeout(resolve, 250)));
    assert.match(editor.textContent, /Edit model · unknown-model/);
    assert.deepEqual([...editor.querySelectorAll("input")].map((input) => input.value), decimals);
    await React.act(async () => [...editor.querySelectorAll("button")].find((button) => button.textContent === "Save override").click());
    assert.deepEqual(saves, [{ modelOverrides: { "unknown-model": {
      inputPerMillion: decimals[0], outputPerMillion: decimals[1], cacheReadPerMillion: decimals[2], cacheWritePerMillion: decimals[3],
    } } }]);
    assert.equal(panel.container.querySelector(".plugin-statistics-price-editor"), null);
    assert.match(panel.container.textContent, /Saved model price override/);
  } finally { await panel.close(); }
});

// Exact price writes remain single-flight even when two events arrive before the disabled render.
// 即使禁用渲染前连续收到两个事件，精确定价写入也保持单次执行。
test("price save rejects duplicate events and retains failed exact decimals", async () => {
  const writes = [];
  let rejectWrite;
  const panel = await mount((action, args) => {
    if (action === "settings-get") return Promise.resolve(settings);
    if (action === "dashboard-get") return Promise.resolve(dashboard);
    if (action === "pricing-get") return Promise.resolve(pricingDirectory);
    writes.push(args);
    return new Promise((_resolve, reject) => { rejectWrite = reject; });
  });
  try {
    await openPricing(panel);
    await React.act(async () => [...panel.container.querySelectorAll("button")].find((button) => button.textContent === "fixture-model").click());
    const input = panel.container.querySelector('.plugin-statistics-price-editor input');
    const setter = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, "value").set;
    await React.act(async () => { setter.call(input, "0.12345678901234567890"); input.dispatchEvent(new window.Event("input", { bubbles: true })); });
    const save = [...panel.container.querySelectorAll("button")].find((button) => button.textContent === "Save override");
    await React.act(async () => { save.click(); save.click(); });
    assert.equal(writes.length, 1);
    assert.equal(writes[0].modelOverrides["fixture-model"].inputPerMillion, "0.12345678901234567890");
    await React.act(async () => rejectWrite(new Error("isolated write rejected")));
    assert.equal(input.value, "0.12345678901234567890");
    assert.equal(save.disabled, false);
    assert.match(panel.container.textContent, /isolated write rejected/);
  } finally { await panel.close(); }
});
