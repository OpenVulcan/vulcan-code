import assert from "node:assert/strict";
import test, { beforeEach } from "node:test";
import { createPluginQueryClient, invalidatePluginQueries } from "../../../apps/vulcan-code-web/src/service/plugin-ui-query-runtime.ts";
import { createRequire } from "node:module";
import { readFile } from "node:fs/promises";
import { SUPPORTED_LOCALES } from "../../../apps/vulcan-code-web/src/i18n-locales.ts";

// Dependencies come from the existing host; no runtime plugin bundle dependency is added.
// 依赖来自现有宿主，不添加运行时插件打包依赖。
const require = createRequire(new URL("../../../apps/vulcan-code-web/package.json", import.meta.url));
const React = require("react");
const { JSDOM } = require("jsdom");
// loadYaml parses the shipped locale and manifest files with the existing host dependency.
// loadYaml 使用宿主现有依赖解析实际发布的语言与清单文件。
const { load: loadYaml } = require("js-yaml");
// manifest is the exact plugin declaration whose locale assets must cover the host registry.
// manifest 是语言资源必须覆盖宿主语言注册表的实际插件清单。
const manifest = loadYaml(await readFile(new URL("../plugin.yaml", import.meta.url), "utf8"));
// catalogs contains every host language's actual package translations, indexed by locale code.
// catalogs 按语言代码保存每种宿主语言的实际包内翻译。
const catalogs = new Map(await Promise.all(SUPPORTED_LOCALES.map(async ({ code }) => [code,
  loadYaml(await readFile(new URL(`../${manifest.locales[code]}`, import.meta.url), "utf8"))])));
// defaults and layout are the shipped configuration defaults and declarative settings layout.
// defaults 与 layout 是随包发布的配置默认值与声明式设置版面。
const defaults = JSON.parse(await readFile(new URL("../config/defaults.json", import.meta.url), "utf8"));
const layout = JSON.parse(await readFile(new URL("./settings.json", import.meta.url), "utf8"));
// moduleId keeps each test's cache isolated while allowing explicit same-generation remount tests.
// moduleId 隔离每个测试的缓存，并允许明确的同代次重新挂载测试。
let moduleId = 0;
beforeEach(() => invalidatePluginQueries("rtk-test"));

// mount renders the statistics page against controlled actions and page-kit stand-ins.
// mount 使用受控 Action 与页面套件替身渲染统计页面。
// invoke supplies action responses; factory retains the module cache; locale selects a real catalog. Returns DOM and cleanup.
// invoke 提供 Action 响应；factory 保留模块缓存；locale 选择实际翻译。返回 DOM 与清理函数。
async function mount(invoke, factory = null, locale = null) {
  // Exercise the production query coordinator; only the backend action boundary is controlled.
  // 使用生产查询协调器；仅控制后端 Action 边界。
  const client = createPluginQueryClient({ pluginId: "rtk-test", runtimeIdentity: "fixture:1", workspacePath: null, conversationId: null }, [
    { id: "statistics-get", effect: "read", resultCacheMs: 60_000 },
  ], invoke);
  const dom = new JSDOM("<!doctype html><div id='root'></div>");
  Object.assign(globalThis, { window: dom.window, document: dom.window.document, IS_REACT_ACT_ENVIRONMENT: true });
  // Page-kit stand-ins render every slot as text; the chart and table keep the cells the assertions read.
  // 页面套件替身将每个插槽渲染为文字；图表与表格保留断言读取的单元格。
  const block = (props) => React.createElement("div", null, props.title, props.status, props.description, props.actions, props.label, props.value, props.children, props.footer);
  const Button = ({ children, size, variant, ...props }) => React.createElement("button", props, children);
  const Metadata = ({ label, onActivate, ariaLabel }) => React.createElement("button", { onClick: onActivate, "aria-label": ariaLabel }, label);
  const MetricGrid = ({ children }) => React.createElement("div", { className: "metric-grid" }, children);
  const TimeSeriesLineChart = ({ series }) => React.createElement("div", { className: "chart" },
    ...series[0].points.map((point) => React.createElement("small", { key: point.key }, point.key)));
  const PageTable = ({ rows, columns, empty }) => rows.length === 0 ? empty : React.createElement("table", null, React.createElement("tbody", null,
    ...rows.map((row) => React.createElement("tr", { key: row.key }, ...columns.map((column) => React.createElement("td", { key: column.key }, row.cells[column.key]))))));
  const createPanel = factory ?? (await import(`./index.mjs?test=${++moduleId}`)).createPluginPanel;
  const sdk = { React, invoke: client.invoke, locale: locale ?? "en-US",
    formatNumber: (value) => locale ? new Intl.NumberFormat(locale).format(value) : String(value),
    t: (key, fallback) => locale ? catalogs.get(locale).messages[key] ?? fallback : fallback,
    notifications: { error: () => {} },
    components: { Button, MetricCard: block, MetricGrid, PageBody: block, PageEmptyState: block, PageHint: block, PageNotice: block, PageSection: block, PageTable,
      PageToolbar: block, PluginMetadataLink: Metadata, StatusPill: block, StreamPlaceholder: block, TextDocumentDialog: () => null, TimeSeriesLineChart } };
  const root = require("react-dom/client").createRoot(document.getElementById("root"));
  await React.act(async () => root.render(React.createElement(createPanel(sdk))));
  return { container: document.getElementById("root"), factory: createPanel, close: async () => {
    await React.act(async () => root.unmount()); client.dispose(); dom.window.close();
  } };
}

// button finds one button by its exact visible text.
// button 按精确可见文字查找一个按钮。
function button(panel, label) { return [...panel.container.querySelectorAll("button")].find((item) => item.textContent === label); }

// The declarative layout offers every route once, with the shipped defaults as the curated group.
// 声明式版面恰好提供每条路由一次，并以随包默认值作为精选分组。
test("settings layout route groups are disjoint and the curated group equals the default routes", () => {
  const fields = layout.sections.flatMap((section) => section.items).filter((item) => item.control === "checklist");
  assert.deepEqual(fields.map((field) => [field.key, field.selection, field.visible_when.values]),
    [["enabledRoutes", "include", ["selected"]], ["disabledRoutes", "exclude", ["all"]]]);
  for (const field of fields) {
    const values = field.option_groups.flatMap((group) => group.values);
    assert.equal(new Set(values).size, values.length, `${field.key} offers a route twice`);
    assert.deepEqual(field.option_groups.find((group) => group.id === "curated").values, defaults.enabledRoutes);
  }
  assert.deepEqual(fields[0].option_groups, fields[1].option_groups);
});

// Shipped translations cover host languages and retain exact interpolation contracts.
// 发布翻译覆盖宿主语言，并保留精确的插值契约。
test("all host locales have complete translated catalogs and required assets", () => {
  // baseline is the English source catalog, not a second manually maintained key registry.
  // baseline 为英文源翻译，不另行维护第二份键注册表。
  const baseline = catalogs.get("en-US");
  assert.equal(manifest.locales.en, manifest.locales["en-US"]);
  for (const { code } of SUPPORTED_LOCALES) {
    const catalog = catalogs.get(code);
    assert.ok(manifest.required_files.includes(manifest.locales[code]), `${code} locale must be packaged`);
    assert.equal(catalog.name, "Vulcan RTK");
    assert.equal(typeof catalog.capability, "string");
    assert.deepEqual(Object.keys(catalog.messages).sort(), Object.keys(baseline.messages).sort(), code);
    // key and value identify one real translation whose placeholders must match its source contract.
    // key 与 value 标识一条实际翻译，其占位符必须符合源契约。
    for (const [key, value] of Object.entries(catalog.messages)) {
      assert.equal(typeof value, "string", `${code}:${key}`);
      assert.ok(value.trim(), `${code}:${key} must be translated`);
      assert.deepEqual(value.match(/\{\{[^}]+\}\}/g) ?? [], baseline.messages[key].match(/\{\{[^}]+\}\}/g) ?? [], `${code}:${key} placeholders`);
    }
    if (code !== "en-US") {
      assert.notEqual(catalog.capability, baseline.capability, `${code} capability must be translated`);
      assert.notEqual(catalog.messages["stats.refresh"], baseline.messages["stats.refresh"], `${code} actions must be translated`);
    }
  }
});

// Real localized pages preserve numeric units, calendar dates, and translations across all host languages.
// 实际本地化页面在所有宿主语言中保留数字单位、日历日期与翻译。
test("statistics render every host locale with regional numbers, units and dates", async () => {
  for (const { code } of SUPPORTED_LOCALES) {
    const catalog = catalogs.get(code);
    // statistics includes all period shapes observed from the packaged RTK binary.
    // statistics 包含从随包 RTK 二进制实际观察到的全部周期格式。
    const statistics = { available: true, generated_at_ms: 1_791_158_400_000,
      summary: { total_saved: 12345, avg_savings_pct: 12.5, total_input: 2500, total_output: 1000 },
      daily: [{ date: "2026-10-05", saved_tokens: 100, savings_pct: 12.5 }],
      weekly: [{ week_start: "2026-10-05", week_end: "2026-10-11", saved_tokens: 100, savings_pct: 12.5 }],
      monthly: [{ month: "2026-10", saved_tokens: 100, savings_pct: 12.5 }],
      byCommand: [{ command: "git", count: 1234, saved_tokens: 100, avg_savings_pct: 12.5, avg_time_ms: 1250 },
        { command: "cargo", count: 2, saved_tokens: 50, avg_savings_pct: 25, avg_time_ms: 25 },
        { command: "rg", count: 3, saved_tokens: 30, avg_savings_pct: 50, avg_time_ms: 61000 }] };
    const panel = await mount(() => Promise.resolve(statistics), null, code);
    try {
      const table = panel.container.querySelector("table").textContent;
      const dayFormat = new Intl.DateTimeFormat(code, { month: "numeric", day: "numeric", timeZone: "UTC" });
      assert.ok(panel.container.textContent.includes(catalog.messages["stats.topCommands"].replace("{{count}}", new Intl.NumberFormat(code).format(3))), code);
      assert.ok(table.includes(new Intl.NumberFormat(code, { style: "percent", minimumFractionDigits: 1, maximumFractionDigits: 1 }).format(0.125)), `${code} percent`);
      assert.ok(table.includes(new Intl.NumberFormat(code, { style: "unit", unit: "second", unitDisplay: "short", minimumFractionDigits: 1, maximumFractionDigits: 1 }).format(1.25)), `${code} duration`);
      assert.ok(panel.container.querySelector(".metric-grid").textContent.includes(new Intl.NumberFormat(code, { minimumFractionDigits: 1, maximumFractionDigits: 1 }).format(2.5)), `${code} ratio`);
      assert.equal(panel.container.querySelector(".chart small").textContent, dayFormat.format(new Date("2026-10-05T00:00:00Z")), `${code} daily`);
      await React.act(async () => button(panel, catalog.messages["stats.weekly"]).click());
      assert.ok(panel.container.querySelector(".chart small").textContent.includes(dayFormat.format(new Date("2026-10-11T00:00:00Z"))), `${code} weekly`);
      await React.act(async () => button(panel, catalog.messages["stats.monthly"]).click());
      assert.equal(panel.container.querySelector(".chart small").textContent,
        new Intl.DateTimeFormat(code, { year: "numeric", month: "short", timeZone: "UTC" }).format(new Date("2026-10-01T00:00:00Z")), `${code} monthly`);
    } finally { await panel.close(); }
  }
});

// A failed read is retryable and an unavailable report explains itself instead of showing zeros.
// 读取失败可重试；不可用报告给出说明，而不是显示零值。
test("read failures are retryable and unavailable statistics show their reason", async () => {
  let fail = true;
  const panel = await mount(() => fail ? Promise.reject(new Error("statistics unavailable")) : Promise.resolve({ available: false, error: "rtk gain failed" }));
  try {
    assert.match(panel.container.textContent, /Unable to load RTK statistics/);
    fail = false;
    await React.act(async () => button(panel, "Retry").click());
    assert.doesNotMatch(panel.container.textContent, /Unable to load RTK statistics/);
    assert.match(panel.container.textContent, /rtk gain failed/);
    assert.equal(panel.container.querySelector(".metric-grid"), null);
  } finally { await panel.close(); }
});

// A completed request from an unmounted page cannot overwrite the current generation's shared statistics cache.
// 已卸载页面的请求完成结果不能覆盖当前代次的共享统计缓存。
test("remounts share a pending statistic and retain its result after every subscriber leaves", async () => {
  let finishOld;
  const old = await mount(() => new Promise((resolve) => { finishOld = resolve; }));
  const factory = old.factory;
  await old.close();
  const fresh = { available: true, summary: { total_saved: 777 }, daily: [], byCommand: [] };
  const panel = await mount(() => Promise.resolve(fresh), factory);
  await panel.close();
  await React.act(async () => finishOld(fresh));
  let statisticReads = 0;
  const finalPanel = await mount(() => { statisticReads += 1; return Promise.resolve({ available: true, summary: { total_saved: 111 } }); }, factory);
  try {
    assert.equal(statisticReads, 0);
    assert.match(finalPanel.container.querySelector(".metric-grid").textContent, /777/);
    assert.doesNotMatch(finalPanel.container.querySelector(".metric-grid").textContent, /111/);
  } finally { await finalPanel.close(); }
});
