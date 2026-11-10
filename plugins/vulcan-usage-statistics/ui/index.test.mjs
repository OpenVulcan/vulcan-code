import assert from "node:assert/strict";
import test from "node:test";
import { createRequire } from "node:module";
import { readFileSync } from "node:fs";
import { formatMoney, presetTimeRange } from "./index.mjs";
import { createPluginQueryClient, invalidatePluginQueries } from "../../../apps/vulcan-code-web/src/service/plugin-ui-query-runtime.ts";
// Read the authoritative registry without depending on the WebUI bundler's aliases.
// 直接读取权威注册表，不依赖 WebUI 打包器的别名。
const SUPPORTED_LOCALES = JSON.parse(readFileSync(new URL("../../../crates/vulcan-i18n/catalogs/locales.json", import.meta.url), "utf8")).locales;

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
    ["0", "0.00"], ["0.000000", "0.00"], ["0.00000001", "<0.001"],
    ["0.000365468432134", "<0.001"], ["0.001", "0.001"], ["0.1", "0.1"],
    ["0.129933358208955", "≈0.13"], ["0.8705535", "≈0.871"],
    ["0.99999", "≈1.00"], ["1.2300", "1.23"], ["1.2345", "≈1.235"],
    ["123456789012345678901.125", "123,456,789,012,345,678,901.125"],
  ]) assert.equal(formatMoney(input, "en-US"), expected, String(input));
});

// Check special branches as well as exact large values against comma-decimal locales.
// 对逗号小数语言验证特殊分支和精确大金额，避免常规分支通过而零值回退英文格式。
test("money localizes zero, tiny values and exact rounding without losing stored digits", () => {
  for (const locale of ["fr-FR", "de-DE", "es-ES", "pt-BR", "ru-RU"]) {
    assert.equal(formatMoney("0", locale), "0,00", locale);
    assert.equal(formatMoney("0.00000001", locale), "<0,001", locale);
    assert.equal(formatMoney("0.99999", locale), "≈1,00", locale);
    assert.equal(formatMoney("123456789012345678901.125", locale).replace(/[.\s\u00a0\u202f]/g, ""), "123456789012345678901,125", locale);
  }
});

// Mount the actual plugin against an isolated DOM and controlled action responses.
// 将真实插件挂载到隔离 DOM 与受控 Action 响应。
// invoke handles actions, strict controls StrictMode, and factory optionally reuses a loaded plugin module.
// invoke 处理 Action，strict 控制 StrictMode，factory 可复用已加载的插件模块。
// locale and messages project host localization.
// locale 和 messages 投影宿主本地化。
// Returns the container and asynchronous cleanup.
// 返回容器与异步清理函数。
async function mount(invoke, strict = false, locale = "en-US", messages = {}, factory, renderTooltipContent = false) {
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
  // TimeSeriesLineChart exposes the exact points and labels passed to the host chart.
  // TimeSeriesLineChart 暴露传入宿主图表的准确数据点和标签。
  const TimeSeriesLineChart = ({ ariaLabel, series }) => React.createElement("div", { "aria-label": ariaLabel, "data-series": JSON.stringify(series) });
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
    formatTokenCount: (value) => `tok:${value}`,
    components: { PageBody: plain, PageToolbar, PageTabs, PageTabPanel, PageSection, PageHint: plain, PageNotice, PageEmptyState: ({ title }) => React.createElement("p", null, title),
      PageFacts, PageTable, MetricCard: plain, MetricGrid: plain, Badge: plain, Button, Input, Checkbox, SelectControl, StatusPill: plain, StreamPlaceholder: plain,
      ActivityHeatmap, ActivityHeatmapLegend: plain, TimeSeriesLineChart, ChartLegend: plain,
      HelpTooltip: ({ ariaLabel, content }) => React.createElement(React.Fragment, null,
        React.createElement("button", { "aria-label": ariaLabel }),
        renderTooltipContent ? React.createElement("div", { hidden: true }, content) : null),
      Dialog: ({ open, children }) => open ? React.createElement("div", null, children) : null,
      // Preserve the dialog's class so tests can address its header, scrollable form and footer together.
      // 保留对话框类名，让测试可同时访问标题、可滚动表单和底部操作区。
      DialogContent: ({ className, children }) => React.createElement("div", { className }, children),
      DialogHeader: plain, DialogTitle: plain, DialogDescription: plain, DialogFooter: plain },
  };
  // root mounts the real hook-based plugin component.
  // root 挂载真实的基于 Hook 的插件组件。
  const root = require("react-dom/client").createRoot(document.getElementById("root"));
  // createPluginPanel starts with fresh module memory unless the test is navigating within one app lifetime.
  // createPluginPanel 使用全新的模块内存，除非测试模拟同一次应用运行期间的导航。
  const createPluginPanel = factory ?? (await import(`./index.mjs?instance=${crypto.randomUUID()}`)).createPluginPanel;
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
const pricingDirectory = { currency: "USD", totalCount: 1, models: [{ modelId: "fixture-model", attempts: 0, source: "catalog",
  aliases: [], effectivePrices: { USD: settings.catalogModels[0].prices } }] };

// openPricing follows the real task navigation and allows its debounced directory read to settle.
// openPricing 遵循真实任务导航，并等待防抖目录读取完成。
// panel is the mounted plugin fixture; returns after the directory page has resolved.
// panel 是已挂载插件夹具；目录分页完成后返回。
async function openPricing(panel) {
  await React.act(async () => panel.container.querySelector("nav button:nth-child(4)").click());
  await React.act(async () => new Promise((resolve) => window.setTimeout(resolve, 250)));
}

// Confirm that a sub-cent media tariff stays precise in the reference tooltip and outside Token prices.
// 验证不足一分钱的媒体单价在参考提示中保留精度，且不进入 Token 单价栏。
test("lists regional media prices in their original units", async () => {
  // mediaPricing is a synthetic row with distinct rates small enough to expose rounding loss.
  // mediaPricing 使用不同地区的微小测试单价，便于发现错误舍入。
  const mediaPricing = { ...pricingDirectory, models: [{
    modelId: "fixture-audio", attempts: 0, source: "unpriced", aliases: [], effectivePrices: {},
    nativeUnitRates: [{ unit: "input_second", beijingCny: "0.00012", singaporeCny: "0.00034" }],
  }] };
  // panel mounts the real pricing table with the public action payload.
  // panel 用公开 Action 返回的数据挂载真实定价表。
  const panel = await mount(
    (action) => Promise.resolve(action === "settings-get" ? settings : action === "pricing-get" ? mediaPricing : dashboard),
    false, "en-US", {}, undefined, true,
  );
  try {
    await openPricing(panel);
    // row locates the entry by model identity so directory order does not affect the assertion.
    // row 按模型标识定位条目，避免目录顺序影响断言。
    const row = [...panel.container.querySelectorAll("tr")].find((element) => element.textContent.includes("fixture-audio"));
    assert.ok(row);
    assert.equal([...row.cells].filter((cell) => cell.textContent.includes("0.00012")).length, 1);
    assert.match(row.querySelector("dd").textContent, /0\.00012.*0\.00034/);
  } finally {
    await panel.close();
  }
});

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

// Reopening the panel shares only committed range memory, while a fresh module starts at local midnight.
// 重新打开面板只复用已确认的时间范围，全新模块从本地零点开始。
test("remembers the committed range until the plugin module is reloaded", async () => {
  // factory represents the persistent namespace held by the plugin host during navigation.
  // factory 代表导航期间插件宿主持有的持久命名空间。
  const factory = (await import("./index.mjs?instance=range-lifetime")).createPluginPanel;
  // reads records exactly the filters forwarded to the action.
  // reads 记录转发给 Action 的确切筛选条件。
  const reads = [];
  // invoke returns isolated statistics and records every dashboard read.
  // invoke 返回隔离统计并记录每次仪表盘读取。
  const invoke = async (action, args) => {
    if (action === "settings-get") return settings;
    reads.push(args);
    return dashboard;
  };
  // initial, reopened and restarted distinguish initial, navigation and restart lifetimes.
  // initial、reopened 与 restarted 分别表示首次挂载、导航和重启的生命周期。
  const initial = await mount(invoke, false, "en-US", {}, factory);
  try {
    assert.equal(reads.at(-1).startAtMs, presetTimeRange(0).startAtMs);
    assert.ok(reads.at(-1).endAtMs > reads.at(-1).startAtMs && reads.at(-1).endAtMs <= Date.now());
    await selectRange(initial, "7 days");
  } finally { await initial.close(); }
  // committed captures the exact confirmed range, including the fixed end instant.
  // committed 捕获确认后的完整范围，包括固定的结束时刻。
  const committed = reads.at(-1);
  const reopened = await mount(invoke, false, "en-US", {}, factory);
  try {
    assert.deepEqual(reads.at(-1), committed);
  } finally { await reopened.close(); }
  const restarted = await mount(invoke);
  try {
    assert.equal(reads.at(-1).startAtMs, presetTimeRange(0).startAtMs);
    assert.ok(reads.at(-1).endAtMs > reads.at(-1).startAtMs && reads.at(-1).endAtMs <= Date.now());
  } finally { await restarted.close(); }
});

// The chart consumes the server's exact hourly projection and fills empty hours with zeros.
// 图表使用服务端的精确小时投影，并为没有数据的小时补零。
test("plots hourly usage with empty buckets and clock labels", async () => {
  // start and hour define three buckets in the same local-day range.
  // start 与 hour 定义同一本地自然日范围的三个分桶。
  const start = presetTimeRange(0).startAtMs;
  const hour = 3_600_000;
  // panel receives two non-adjacent points of one model.
  // panel 收到同一个模型的两个不相邻数据点。
  const panel = await mount(async (action) => action === "settings-get" ? settings : {
    ...dashboard,
    hourlySeries: { endAtMs: start + 2 * hour + 1, points: [
      { hourStartAtMs: start, seriesKey: "fixture", label: "Fixture", value: 2 },
      { hourStartAtMs: start + 2 * hour, seriesKey: "fixture", label: "Fixture", value: 5 },
    ] },
  });
  try {
    // chart and points capture the real plugin's SDK chart output.
    // chart 和 points 捕获真实插件的 SDK 图表输出。
    const chart = panel.container.querySelector("[data-series]");
    const [{ points }] = JSON.parse(chart.dataset.series);
    assert.match(chart.getAttribute("aria-label"), /Hourly/);
    assert.deepEqual(points.map((point) => point.value), [2, 0, 5]);
    assert.deepEqual(points.map((point) => Number(point.key)), [start, start + hour, start + 2 * hour]);
    assert.ok(points.every((point) => /^\d{2}:\d{2}$/.test(point.label) && point.detailLabel.length > point.label.length));
  } finally { await panel.close(); }
});

// Locale affects projected text only; arithmetic, stored facts and CSS proportions stay exact.
// 语言只影响投影文本；计算、存储事实和 CSS 占比保持精确。
test("French host locale formats percentage spacing, cache share and translated panel labels", async () => {
  // messages is the declared host translation projection, independent from plugin calculations.
  // messages 为声明的宿主译文投影，与插件计算相互独立。
  const messages = {
    "sections.overview": "Vue d’ensemble", "sections.providers": "Par fournisseur",
    "tokens.composition": "Entrée {{input}} · Sortie {{output}}",
    "heatmap.tooltip": "{{date}} : {{value}} tokens · {{requests}} requêtes · {{successRate}} de réussite · {{model}}",
  };
  const panel = await mount((action) => Promise.resolve(action === "settings-get" ? settings : {
    ...dashboard, summary: { ...dashboard.summary, inputTokens: 1200, outputTokens: 300, cacheReadTokens: 400 },
    heatmap: [{ date: new Date().toLocaleDateString("en-CA"), value: 1500, requests: 3, successes: 2, topModelName: "fixture-model", topProviderName: "fixture-provider" }],
  }), false, "fr-FR", messages);
  try {
    assert.match(panel.container.textContent, /Vue d’ensemble/);
    assert.match(panel.container.textContent, /Par fournisseur/);
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
    assert.equal(formatMoney("0.00000001", locale), `<0${separator}001`, locale);
    const panel = await mount((action) => Promise.resolve(action === "settings-get" ? settings : {
      ...dashboard, summary: { ...dashboard.summary, inputTokens: 1200, outputTokens: 300, cacheReadTokens: 400 },
      heatmap: [{ date: new Date().toLocaleDateString("en-CA"), value: 1500, requests: 3, successes: 2, topModelName: "fixture-model", topProviderName: "fixture-provider" }],
    }), false, locale, catalog.messages);
    try {
      assert.match(panel.container.textContent, new RegExp(catalog.messages["sections.overview"].replace(/[.*+?^${}()|[\]\\]/g, "\\$&")), locale);
      assert.ok([...panel.container.querySelectorAll("nav button")].some((button) => button.textContent === catalog.messages["sections.providers"]), locale);
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
    return argumentsValue.startAtMs === presetTimeRange(0).startAtMs ? oldRequest : Promise.resolve({ ...dashboard, aggregateRevision: 777 });
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
    return Promise.resolve({ ...dashboard, aggregateRevision: args.startAtMs === presetTimeRange(0).startAtMs ? 30 : 7 });
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
    assert.equal(reads.at(-1).view, "providers");
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
      attempts: 1, source: "unpriced", aliases: [], effectivePrices: {},
    }] });
  });
  await React.act(async () => panel.container.querySelector("nav button:nth-child(4)").click());
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
        assert.match(balance.querySelector(".plugin-statistics-cache-facts").textContent, /tok:400 · 33.3%/);
      }
      if (inputTokens === null) assert.match(balance.querySelector('[data-token-dimension="inputTokens"]').textContent, /-/);
      else assert.match(balance.querySelector('[data-token-dimension="inputTokens"]').textContent, new RegExp(`tok:${inputTokens}`));
      assert.doesNotMatch(balance.textContent, /NaN|Infinity/);
    } finally { await panel.close(); }
  }
});

// Keep the summed reference amount visible while marking a priced group with missing usage.
// 保留已汇总的参考金额，同时标记存在用量缺口的已定价分组。
test("overview marks partial usage even when every model row has a reference amount", async () => {
  // snapshot supplies two estimated rows, one with incomplete attempt-level coverage.
  // snapshot 提供两行可估算结果，其中一行的请求级覆盖不完整。
  const snapshot = {
    ...dashboard,
    summary: { ...dashboard.summary, costs: [{
      currency: "USD", amount: "7.0150016", pricedRows: 2, incompleteRows: 0, unpricedRows: 0, hasPartialUsage: true,
    }] },
  };
  // panel uses a sentinel translation to assert the partial coverage state without locking wording.
  // panel 使用测试专用译文验证部分覆盖状态，避免绑定界面措辞。
  const panel = await mount((action) => Promise.resolve(action === "settings-get" ? settings : snapshot),
    false, "en-US", { "cost.partial": "fixture-partial-coverage" });
  try {
    assert.match(panel.container.textContent, /fixture-partial-coverage/);
    assert.equal(panel.container.querySelector(".plugin-statistics-cost-value").title, snapshot.summary.costs[0].amount);
  } finally { await panel.close(); }
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
      models: args.search ? [] : [{ modelId: "unknown-model", attempts: 1, source: "unpriced", aliases: [], effectivePrices: {} }] });
  });
  try {
    await openPricing(panel);
    assert.equal(panel.container.querySelector(".plugin-statistics-price-dialog"), null);
    const table = [...panel.container.querySelectorAll("button")].find((button) => button.textContent === "unknown-model").closest("table");
    await React.act(async () => [...table.querySelectorAll("button")].find((button) => button.textContent === "unknown-model").click());
    const editor = panel.container.querySelector(".plugin-statistics-price-dialog");
    assert.match(editor.textContent, /Edit model · unknown-model/);
    assert.match(editor.textContent, /Unpriced/);
    assert.doesNotMatch(editor.textContent, /Catalog default/);
    assert.equal([...editor.querySelectorAll("input")].every((input) => input.value === ""), true);
    assert.equal([...editor.querySelectorAll("button")].find((button) => button.textContent === "Save override").disabled, true);
    // setter uses native input editing so React receives the exact decimal string rather than a number.
    // setter 使用原生输入编辑，让 React 接收到精确十进制字符串而不是数字。
    const setter = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, "value").set;
    const decimals = ["0.0000000000000000000000001", "2.12345678901234567890", "0", "1.50"];
    // Only the USD inputs are explicit in this legacy-compatible write.
    // 此次兼容历史配置的写入仅显式填写美元字段。
    const usdInputs = [...editor.querySelectorAll('input[aria-label$="USD"]')];
    await React.act(async () => usdInputs.forEach((input, index) => {
      setter.call(input, decimals[index]); input.dispatchEvent(new window.Event("input", { bubbles: true }));
    }));
    await React.act(async () => {
      const search = panel.container.querySelector('input[aria-label="Search models"]');
      setter.call(search, "no-match"); search.dispatchEvent(new window.Event("input", { bubbles: true }));
    });
    await React.act(async () => new Promise((resolve) => window.setTimeout(resolve, 250)));
    assert.match(editor.textContent, /Edit model · unknown-model/);
    assert.deepEqual(usdInputs.map((input) => input.value), decimals);
    assert.equal([...editor.querySelectorAll('input[aria-label$="CNY"]')].every((input) => input.value === ""), true);
    await React.act(async () => [...editor.querySelectorAll("button")].find((button) => button.textContent === "Save override").click());
    assert.deepEqual(saves, [{ modelOverrides: { "unknown-model": {
      inputPerMillion: decimals[0], outputPerMillion: decimals[1], cacheReadPerMillion: decimals[2], cacheWritePerMillion: decimals[3],
    } } }]);
    assert.equal(panel.container.querySelector(".plugin-statistics-price-dialog"), null);
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

// Verify that currency and schedule edits consolidate the selected alias without altering unrelated prices.
// 验证币种和时段编辑归并所选别名，同时保留无关模型的价格。
test("price editor saves a shared alias tariff and preserves the exchange rate", async () => {
  // configured is the same historical price under an explicit alias and a family variant.
  // configured 是显式别名和系列变体下相同的历史价格。
  const configured = settings.catalogModels[0].prices;
  const initial = { ...settings, modelAliases: {
      "fixture-alias": "fixture-model", "fixture-model": "fixture-model", "fixture-model-advanced": "fixture-model-advanced",
    },
    modelPrefixes: { "fixture-family": "fixture-model" },
    modelOverrides: {
      "aggregator/vendor/Fixture-Alias": configured,
      "aggregator/vendor/Fixture-Family": configured,
      "aggregator/vendor/Fixture-Family-0309": configured,
      "aggregator/vendor/Fixture-Model-Preview-20261008": configured,
      "aggregator/vendor/Fixture-Model-Variant": configured,
      "aggregator/vendor/Fixture-Model-Advanced-Variant": configured,
      unrelated: configured,
    } };
  // Stable catalog keys allow this test to ignore the exact wording.
  // 使用稳定文案键，使测试不依赖具体措辞。
  const messages = Object.fromEntries([
    "exchangeRate", "saveExchangeRate", "enableOffPeak", "offPeak", "windowStart", "windowEnd",
    "windowDays", "addWindow", "save", "contextThreshold", "addContextTier",
  ].map((key) => [`pricing.${key}`, `fixture-${key}`]));
  // writes records the exact full configuration sent through the declared settings action.
  // writes 记录通过已声明设置 Action 提交的完整精确配置。
  const writes = [];
  const panel = await mount((action, args) => {
    if (action === "settings-get") return Promise.resolve(initial);
    if (action === "dashboard-get") return Promise.resolve(dashboard);
    if (action === "pricing-get") return Promise.resolve(pricingDirectory);
    writes.push(args);
    return Promise.resolve({ ...initial, ...args });
  }, false, "en-US", messages);
  try {
    await openPricing(panel);
    // change drives React through the native field setter and input event.
    // change 通过原生字段赋值和输入事件驱动 React。
    const change = async (label, value) => {
      const input = panel.container.querySelector(`input[aria-label="${label}"]`);
      const setter = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, "value").set;
      await React.act(async () => { setter.call(input, value); input.dispatchEvent(new window.Event("input", { bubbles: true })); });
    };
    // click triggers the button identified by a stable fixture label.
    // click 触发由稳定夹具标签标识的按钮。
    const click = async (label) => React.act(async () =>
      [...panel.container.querySelectorAll("button")].find((button) => button.textContent === label).click());
    await change("fixture-exchangeRate", "7.2");
    await click("fixture-saveExchangeRate");
    await click("fixture-model");
    await click("fixture-addContextTier");
    await change("fixture-contextThreshold · 1", "262144");
    await change("fixture-contextThreshold · 1 · Uncached input · USD", "8");
    await click("fixture-enableOffPeak");
    await change("fixture-windowStart 1", "00:00");
    await change("fixture-windowEnd 1", "02:00");
    await click("fixture-addWindow");
    await change("fixture-windowStart 2", "00:00");
    await change("fixture-windowEnd 2", "24:00");
    // The second window selects the weekend and explicitly prices its input in CNY.
    // 第二条时段选择周末，并显式填写人民币输入单价。
    const days = panel.container.querySelector('select[aria-label="fixture-windowDays 2"]');
    await React.act(async () => { days.value = "weekend"; days.dispatchEvent(new window.Event("change", { bubbles: true })); });
    await change("fixture-offPeak · Uncached input · CNY", "6");
    await change("fixture-offPeak · Uncached input · USD", "");
    await React.act(async () => panel.container.querySelector('button[aria-label="fixture-offPeak · fixture-addContextTier"]').click());
    await change("fixture-offPeak · fixture-contextThreshold · 1", "262144");
    await change("fixture-offPeak · fixture-contextThreshold · 1 · Uncached input · CNY", "12");
    await click("fixture-save");
    assert.equal(writes.length, 2);
    assert.equal(writes[1].cnyPerUsd, "7.2");
    assert.deepEqual(writes[1].modelOverrides.unrelated, configured);
    assert.deepEqual(writes[1].modelOverrides["aggregator/vendor/Fixture-Model-Advanced-Variant"], configured);
    assert.equal(Object.hasOwn(writes[1].modelOverrides, "aggregator/vendor/Fixture-Alias"), false);
    assert.equal(Object.hasOwn(writes[1].modelOverrides, "aggregator/vendor/Fixture-Family"), false);
    assert.equal(Object.hasOwn(writes[1].modelOverrides, "aggregator/vendor/Fixture-Family-0309"), false);
    assert.equal(Object.hasOwn(writes[1].modelOverrides, "aggregator/vendor/Fixture-Model-Preview-20261008"), false);
    assert.equal(Object.hasOwn(writes[1].modelOverrides, "aggregator/vendor/Fixture-Model-Variant"), false);
    assert.deepEqual(writes[1].modelOverrides["fixture-model"].contextTiers, [
      { aboveInputTokens: 262144, prices: { ...configured, inputPerMillion: "8" } },
    ]);
    const tariff = writes[1].modelOverrides["fixture-model"].offPeak;
    assert.deepEqual(tariff.windows, [
      { days: "weekdays", start: "00:00", end: "02:00" },
      { days: "weekend", start: "00:00", end: "24:00" },
    ]);
    assert.equal(tariff.prices.inputPerMillionCny, "6");
    assert.equal(Object.hasOwn(tariff.prices, "inputPerMillion"), false);
    assert.deepEqual(tariff.contextTiers, [
      { aboveInputTokens: 262144, prices: { ...tariff.prices, inputPerMillionCny: "12" } },
    ]);
  } finally { await panel.close(); }
});

// A provider can copy a tariff, edit one amount and save it independently of the shared rate.
// 供应商可以复制费率并修改单项金额，保存时与共享汇率分开。
test("copies a tariff into a new provider scope and uses the shared exchange rate", async () => {
  // The alternate template includes both a time window and an input-length tier.
  // 备用模板同时包含时段和输入长度档位。
  const prices = { inputPerMillion: "3", outputPerMillion: "4", cacheReadPerMillion: "0", cacheWritePerMillion: "0" };
  const template = { ...prices, contextTiers: [{ aboveInputTokens: 1000, prices }],
    offPeak: { utcOffsetMinutes: 480, windows: [{ days: "weekend", start: "00:00", end: "24:00" }], prices } };
  const initial = { ...settings, defaultCnyPerUsd: "6.7",
    catalogModels: [...settings.catalogModels, { modelId: "template-model", prices: template }],
    providerOverrides: { "existing-provider": { "fixture-model": prices } } };
  // The mock preserves the action's full-document write semantics and records scoped reads.
  // 模拟调用保留 Action 的整份文档写入语义，并记录带范围的读取。
  const writes = [];
  const reads = [];
  const messages = Object.fromEntries(["providerScope", "providerId", "template", "applyTemplate",
    "exchangeRate", "saveExchangeRate", "save"].map((key) => [`pricing.${key}`, `fixture-${key}`]));
  const panel = await mount((action, args) => {
    if (action === "settings-get") return Promise.resolve(initial);
    if (action === "dashboard-get") return Promise.resolve(dashboard);
    if (action === "pricing-get") {
      reads.push(args);
      return Promise.resolve({ ...pricingDirectory, providerIds: ["existing-provider"] });
    }
    writes.push(args);
    return Promise.resolve({ ...initial, ...args });
  }, false, "en-US", messages);
  try {
    await openPricing(panel);
    // Drive the native inputs and selects through the same events as the host stubs.
    // 使用与宿主模拟控件一致的事件驱动原生输入框及下拉框。
    const change = async (label, value, tag = "input") => {
      const control = panel.container.querySelector(`${tag}[aria-label="${label}"]`);
      const setter = Object.getOwnPropertyDescriptor(Object.getPrototypeOf(control), "value").set;
      await React.act(async () => {
        setter.call(control, value);
        control.dispatchEvent(new window.Event(tag === "input" ? "input" : "change", { bubbles: true }));
      });
    };
    const click = async (label) => React.act(async () =>
      [...panel.container.querySelectorAll("button")].find((button) => button.textContent === label).click());
    assert.equal(panel.container.querySelector('input[aria-label="fixture-exchangeRate"]').value, "6.7");
    await change("fixture-providerScope", "manual", "select");
    await change("fixture-providerId", "custom-new");
    await React.act(async () => new Promise((resolve) => window.setTimeout(resolve, 250)));
    assert.equal(reads.at(-1).providerId, "custom-new");
    await click("fixture-model");
    await change("fixture-template", "catalog:template-model", "select");
    await click("fixture-applyTemplate");
    await change("Output · CNY", "29");
    await click("fixture-save");
    assert.deepEqual(writes[0], { modelOverrides: {}, providerOverrides: {
      ...initial.providerOverrides, "custom-new": { "fixture-model": { ...template, outputPerMillionCny: "29" } },
    } });
    await change("fixture-exchangeRate", "7.1");
    await click("fixture-saveExchangeRate");
    assert.deepEqual(writes[1], { ...writes[0], cnyPerUsd: "7.1" });
  } finally { await panel.close(); }
});

// A shared model alias can still have a separately editable provider tariff.
// 共享目录中的型号别名仍可拥有独立可编辑的供应商单价。
test("edits an independently priced provider snapshot without rewriting its shared group", async () => {
  // initial represents a shared alias with an independently quoted provider snapshot.
  // initial 表示共享别名与供应商单独报价并存。
  const initial = { ...settings,
    modelAliases: { "fixture-model-snapshot": "fixture-model" },
    providerCatalogModels: { vendor: { "fixture-model-snapshot": {
      inputPerMillionCny: "2", outputPerMillionCny: "4", cacheReadPerMillionCny: "0.25", cacheWritePerMillionCny: "0",
    } } } };
  // writes records the exact provider ID and model key accepted from the editor.
  // writes 记录编辑器提交的精确供应商标识与模型键。
  const writes = [];
  // panel mounts the production price editor with isolated fixture settings.
  // panel 使用隔离的夹具设置挂载生产定价编辑器。
  const panel = await mount((action, args) => {
    if (action === "settings-get") return Promise.resolve(initial);
    if (action === "dashboard-get") return Promise.resolve(dashboard);
    if (action === "pricing-get") return Promise.resolve({ ...pricingDirectory, providerIds: ["vendor"],
      models: [{ ...pricingDirectory.models[0], modelId: "fixture-model-snapshot" }] });
    writes.push(args);
    return Promise.resolve({ ...initial, ...args });
  }, false, "en-US", { "pricing.providerScope": "fixture-scope", "pricing.save": "fixture-save", "pricing.cacheRead": "fixture-cache-read" });
  try {
    await openPricing(panel);
    // scope selects the provider's quote instead of the shared alias.
    // scope 选择供应商报价，而不是共享别名。
    const scope = panel.container.querySelector('select[aria-label="fixture-scope"]');
    await React.act(async () => { scope.value = "provider:vendor"; scope.dispatchEvent(new window.Event("change", { bubbles: true })); });
    await React.act(async () => new Promise((resolve) => window.setTimeout(resolve, 250)));
    await React.act(async () => [...panel.container.querySelectorAll("button")].find((button) => button.textContent === "fixture-model-snapshot").click());
    assert.equal(panel.container.querySelector('input[aria-label="fixture-cache-read · CNY"]').value, "0.25");
    await React.act(async () => [...panel.container.querySelectorAll("button")].find((button) => button.textContent === "fixture-save").click());
    assert.deepEqual(writes[0]?.providerOverrides?.vendor, initial.providerCatalogModels.vendor);
    assert.deepEqual(writes[0]?.modelOverrides, initial.modelOverrides);
  } finally { await panel.close(); }
});

// Provider selection narrows only the Provider tab; recent attempts never inherit it.
// Provider 选择只收窄供应商分类，最近请求不会继承该筛选。
test("provider selection scopes the provider tab without narrowing recent attempts", async () => {
  const reads = [];
  const panel = await mount((action, args) => {
    if (action === "settings-get") return Promise.resolve(settings);
    reads.push(args);
    return Promise.resolve({ ...dashboard, providers: [{ providerId: "provider-a", attempts: 3, successes: 3, totalTokens: 10 }] });
  });
  try {
    await React.act(async () => panel.container.querySelector("nav button:nth-child(2)").click());
    assert.equal(reads.at(-1).view, "providers");
    assert.equal(reads.at(-1).providerId, "");
    await React.act(async () => panel.container.querySelector('button[aria-pressed="false"]').click());
    assert.equal(reads.at(-1).providerId, "provider-a");
    await React.act(async () => panel.container.querySelector("nav button:nth-child(3)").click());
    assert.equal(reads.at(-1).view, "recent");
    assert.equal("providerId" in reads.at(-1), false);
  } finally { await panel.close(); }
});

// Distinguish service entries even when every other visible grouping field matches.
// 即使其他可见分组字段完全相同，也应区分服务入口。
test("shows service entries and keeps partial estimates visible for the same model", async () => {
  // Both records have the same model and consumer but different routes and cost coverage.
  // 两条记录的模型和调用方相同，接入入口与费用覆盖情况不同。
  const shared = {
    providerId: "openai", modelId: "fixture-model", consumerId: "webui", operation: "conversation_respond",
    attemptCount: 1, successfulAttemptCount: 1, timedAttemptCount: 0,
    inputTokens: 10, uncachedInputTokens: 10, outputTokens: null, totalTokens: 10,
    cacheReadTokens: null, cacheWriteTokens: null, reasoningTokens: null,
    requestDurationMs: null, firstTokenLatencyMs: null, outputDurationMs: null, outputTokensPerSecond: null,
    pricingMatchStatus: "incomplete_usage", estimatedCurrency: "USD",
    pricingDetails: { currency: "USD", dimensions: [], hasPartialUsage: true },
  };
  // One group has enough usage for a subtotal; the other has no calculable amount.
  // 一组有足够用量可估算小计，另一组没有可计算的金额。
  const daily = [
    { ...shared, serviceEntryId: "codex-subscription", estimatedCost: "0.129933358208955" },
    { ...shared, serviceEntryId: "public-api", estimatedCost: null },
  ];
  // Sentinel translations check the displayed route and coverage without fixing user-facing wording.
  // 使用测试专用译文验证接入入口和覆盖状态，避免绑定面向用户的措辞。
  const panel = await mount(async (action) => action === "settings-get" ? settings
    : { ...dashboard, view: "providers", daily, dailyCount: daily.length },
    false, "en-US", {
      "serviceEntry.codex-subscription": "fixture-subscription",
      "serviceEntry.public-api": "fixture-api",
      "cost.partial": "fixture-partial",
      "pricing.incompleteUsage": "fixture-incomplete",
    });
  try {
    await React.act(async () => panel.container.querySelector("nav button:nth-child(2)").click());
    // Locate each row by its recorded identity regardless of sort order.
    // 按已记录的入口标识定位每行，避免依赖排序位置。
    const subscription = panel.container.querySelector('small[title="openai · codex-subscription"]').closest("tr");
    const api = panel.container.querySelector('small[title="openai · public-api"]').closest("tr");
    assert.match(subscription.textContent, /fixture-subscription/);
    assert.match(subscription.querySelector(".plugin-statistics-estimate-cell").textContent, /≈\$0\.13 · fixture-partial/);
    assert.match(api.textContent, /fixture-api/);
    assert.equal(api.querySelector(".plugin-statistics-estimate-cell").textContent, "fixture-incomplete");
  } finally { await panel.close(); }
});
