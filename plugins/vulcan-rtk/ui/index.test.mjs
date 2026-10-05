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
// settings uses the real declared default shape in an isolated in-memory copy.
// settings 使用真实声明的默认结构，但只保留隔离内存副本。
const settings = JSON.parse(await readFile(new URL("../config/defaults.json", import.meta.url), "utf8"));
// moduleId keeps each test's cache isolated while allowing explicit same-generation remount tests.
// moduleId 隔离每个测试的缓存，并允许明确的同代次重新挂载测试。
let moduleId = 0;
beforeEach(() => invalidatePluginQueries("rtk-test"));

// mount renders the authoritative plugin against controlled actions and native interaction adapters.
// mount 使用受控 Action 与原生交互适配器渲染权威插件。
// invoke supplies action responses; strict replays effects; factory retains the module cache; locale selects a real catalog. Returns DOM and cleanup.
// invoke 提供 Action 响应；strict 重放 Effect；factory 保留模块缓存；locale 选择实际翻译。返回 DOM 与清理函数。
async function mount(invoke, strict = false, factory = null, locale = null) {
  // Exercise the production coordinator; only the backend action boundary is controlled.
  // 使用生产协调器；仅控制后端 Action 边界。
  const client = createPluginQueryClient({ pluginId: "rtk-test", runtimeIdentity: "fixture:1", workspacePath: null, conversationId: null }, [
    { id: "settings-get", effect: "read", resultCacheMs: 0 },
    { id: "statistics-get", effect: "read", resultCacheMs: 60_000 },
    { id: "settings-update", effect: "mutation", resultCacheMs: 0 },
  ], invoke);
  const dom = new JSDOM("<!doctype html><div id='root'></div>");
  Object.assign(globalThis, { window: dom.window, document: dom.window.document, IS_REACT_ACT_ENVIRONMENT: true });
  // Host adapters retain accessible native controls without claiming host-layout verification.
  // 宿主适配器保留可访问原生控件，不声称验证宿主布局。
  const plain = ({ children, label }) => React.createElement("div", null, label, children);
  const Button = ({ children, size, variant, ...props }) => React.createElement("button", props, children);
  const Metadata = ({ label, onActivate, ariaLabel }) => React.createElement("button", { onClick: onActivate, "aria-label": ariaLabel }, label);
  // PageTabs keeps each tab a native button whose text is the exact label, plus the trailing actions.
  // PageTabs 使每个标签保持为文本即精确标签的原生按钮，并保留尾部操作。
  const PageTabs = ({ ariaLabel, options, onValueChange, actions }) => React.createElement("div", null, React.createElement("nav", { "aria-label": ariaLabel },
    ...options.map((option) => React.createElement("button", { key: option.value, type: "button", disabled: option.disabled, onClick: () => onValueChange(option.value) }, option.label))),
    actions);
  const createPanel = factory ?? (await import(`./index.mjs?test=${++moduleId}`)).createPluginPanel;
  // sdk preserves existing English fixtures and optionally renders an actual package locale with host-equivalent formatting.
  // sdk 保留原英文夹具，也可用宿主等效格式化方法渲染实际包内语言。
  const sdk = { React, invoke: client.invoke, locale: locale ?? "en-US",
    formatNumber: (value) => locale ? new Intl.NumberFormat(locale).format(value) : String(value),
    t: (key, fallback) => locale ? catalogs.get(locale).messages[key] ?? fallback : fallback,
    components: { Button, Badge: plain, StatusPill: plain, PageTabs, HelpTooltip: plain, PluginMetadataLink: Metadata, TextDocumentDialog: plain, StreamPlaceholder: plain } };
  const root = require("react-dom/client").createRoot(document.getElementById("root"));
  await React.act(async () => root.render(React.createElement(strict ? React.StrictMode : React.Fragment, null, React.createElement(createPanel(sdk)))));
  return { container: document.getElementById("root"), factory: createPanel, close: async () => {
    await React.act(async () => root.unmount()); client.dispose(); dom.window.close();
  } };
}

// button finds one exact visible text contract in this plugin fixture.
// button 在插件夹具中找到精确文本契约。
// panel owns the DOM; label is exact button text. Returns the matching native control.
// panel 持有 DOM；label 是精确按钮文本。返回匹配的原生控件。
function button(panel, label) { return [...panel.container.querySelectorAll("button")].find((item) => item.textContent === label); }

// Failed settings can be explicitly retried without reloading statistics or fabricating defaults.
// 设置失败可明确重试，不重载统计或编造默认值。
test("settings retry is independent, effect replay shares reads, and search preserves routes", async () => {
  const calls = [];
  let reads = 0;
  const panel = await mount((action) => {
    calls.push(action);
    if (action === "settings-get") return ++reads === 1 ? Promise.reject(new Error("settings unavailable")) : Promise.resolve(settings);
    return Promise.reject(new Error("statistics unavailable"));
  }, true);
  try {
    assert.equal(reads, 1);
    assert.equal(calls.filter((item) => item === "statistics-get").length, 0);
    assert.equal(button(panel, "Save").disabled, true);
    await React.act(async () => button(panel, "Retry").click());
    assert.equal(reads, 2);
    assert.doesNotMatch(panel.container.textContent, /settings unavailable/);
    await React.act(async () => button(panel, "statistics").click());
    assert.equal(calls.filter((item) => item === "statistics-get").length, 1);
    assert.match(panel.container.textContent, /statistics unavailable/);
    await React.act(async () => button(panel, "routes").click());
    const selected = panel.container.querySelectorAll('input[type="checkbox"]:checked').length;
    const search = panel.container.querySelector('input[type="search"]');
    const setter = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, "value").set;
    await React.act(async () => { setter.call(search, "no-route-matches"); search.dispatchEvent(new window.Event("input", { bubbles: true })); });
    assert.match(panel.container.querySelector('[role="status"]').textContent, /No matching routes/);
    await React.act(async () => { setter.call(search, ""); search.dispatchEvent(new window.Event("input", { bubbles: true })); });
    assert.equal(panel.container.querySelectorAll('input[type="checkbox"]:checked').length, selected);
  } finally { await panel.close(); }
});

// Shipped translations cover host languages and retain exact interpolation contracts.
// 发布翻译覆盖宿主语言，并保留精确的插值契约。
test("all host locales have complete translated catalogs and required assets", () => {
  // baseline is the English source catalog, not a second manually maintained key registry.
  // baseline 为英文源翻译，不另行维护第二份键注册表。
  const baseline = catalogs.get("en-US");
  assert.equal(manifest.locales.en, manifest.locales["en-US"]);
  for (const { code } of SUPPORTED_LOCALES) {
    // catalog is the actual declared asset for this host language.
    // catalog 为此宿主语言实际声明的资源。
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
      assert.notEqual(catalog.messages.save, baseline.messages.save, `${code} actions must be translated`);
    }
  }
});

// Real localized panels preserve numeric units, calendar dates, and translations across all host languages.
// 实际本地化面板在所有宿主语言中保留数字单位、日历日期与翻译。
test("statistics render every host locale without English-only labels or numeric formatting", async () => {
  for (const { code } of SUPPORTED_LOCALES) {
    // catalog supplies the exact language used by this mounted panel.
    // catalog 提供此挂载面板实际使用的语言。
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
    // panel mounts the production module with localized controls and deterministic action responses.
    // panel 使用本地化控件与确定的 Action 响应挂载生产模块。
    const panel = await mount((action) => Promise.resolve(action === "settings-get" ? settings : statistics), false, null, code);
    try {
      assert.ok(button(panel, catalog.messages.save), `${code} save button`);
      await React.act(async () => button(panel, catalog.messages["sections.statistics"]).click());
      assert.ok(panel.container.querySelector(".rtk-command-stats-header").textContent.includes(catalog.messages["stats.topCommands"].replace("{{count}}", new Intl.NumberFormat(code).format(3))), code);
      assert.ok(panel.container.querySelector(".rtk-command-table").textContent.includes(new Intl.NumberFormat(code, { style: "percent", minimumFractionDigits: 1, maximumFractionDigits: 1 }).format(0.125)), `${code} percent`);
      assert.ok(panel.container.querySelector(".rtk-command-table").textContent.includes(new Intl.NumberFormat(code, { style: "unit", unit: "second", unitDisplay: "short", minimumFractionDigits: 1, maximumFractionDigits: 1 }).format(1.25)), `${code} duration`);
      assert.ok(panel.container.querySelector(".rtk-gain-summary-grid").textContent.includes(new Intl.NumberFormat(code, { minimumFractionDigits: 1, maximumFractionDigits: 1 }).format(2.5)), `${code} ratio`);
      assert.equal(panel.container.querySelector(".plugin-stat-chart-column small").textContent, new Intl.DateTimeFormat(code, { month: "numeric", day: "numeric", timeZone: "UTC" }).format(new Date("2026-10-05T00:00:00Z")), `${code} daily`);
      await React.act(async () => button(panel, catalog.messages["stats.weekly"]).click());
      assert.ok(panel.container.querySelector(".plugin-stat-chart-column small").textContent.includes(new Intl.DateTimeFormat(code, { month: "numeric", day: "numeric", timeZone: "UTC" }).format(new Date("2026-10-11T00:00:00Z"))), `${code} weekly`);
      await React.act(async () => button(panel, catalog.messages["stats.monthly"]).click());
      assert.equal(panel.container.querySelector(".plugin-stat-chart-column small").textContent, new Intl.DateTimeFormat(code, { year: "numeric", month: "short", timeZone: "UTC" }).format(new Date("2026-10-01T00:00:00Z")), `${code} monthly`);
    } finally { await panel.close(); }
  }
});

// Same-event writes are serialized and a rejected write retains the exact visible draft for manual retry.
// 同事件写入被串行化，写入拒绝保留精确可见草稿供手工重试。
test("save holds a synchronous lock and preserves the route draft on rejection", async () => {
  const writes = [];
  let rejectSave;
  const panel = await mount((action, args) => {
    if (action === "settings-get") return Promise.resolve(settings);
    if (action === "statistics-get") return Promise.resolve({ available: false });
    writes.push(args);
    return new Promise((_resolve, reject) => { rejectSave = reject; });
  });
  try {
    await React.act(async () => panel.container.querySelector('input[type="checkbox"]').click());
    await React.act(async () => { button(panel, "Save").click(); button(panel, "Save").click(); });
    assert.equal(writes.length, 1);
    assert.deepEqual(writes[0], { ...settings, enabled: false });
    assert.equal([...panel.container.querySelectorAll('input[type="checkbox"]')].every((input) => input.disabled), true);
    await React.act(async () => rejectSave(new Error("save rejected")));
    assert.match(panel.container.textContent, /save rejected/);
    assert.equal(panel.container.querySelector('input[type="checkbox"]').checked, false);
    assert.equal(button(panel, "Save").disabled, false);
    assert.equal(writes.length, 1);
  } finally { await panel.close(); }
});

// A completed request from an unmounted panel cannot overwrite the current generation's shared statistics cache.
// 已卸载面板的请求完成结果不能覆盖当前代次的共享统计缓存。
test("navigation shares a pending statistic and retains its result after every subscriber leaves", async () => {
  let finishOld;
  const old = await mount((action) => action === "settings-get" ? Promise.resolve(settings) : new Promise((resolve) => { finishOld = resolve; }));
  const factory = old.factory;
  await React.act(async () => button(old, "statistics").click());
  await old.close();
  const fresh = { available: true, summary: { total_saved: 777 }, daily: [], byCommand: [] };
  const panel = await mount((action) => Promise.resolve(action === "settings-get" ? settings : fresh), false, factory);
  await React.act(async () => button(panel, "statistics").click());
  await panel.close();
  await React.act(async () => finishOld(fresh));
  let statisticReads = 0;
  const finalPanel = await mount((action) => { if (action === "statistics-get") statisticReads += 1; return Promise.resolve(settings); }, false, factory);
  try {
    await React.act(async () => button(finalPanel, "statistics").click());
    assert.equal(statisticReads, 0);
    assert.match(finalPanel.container.querySelector(".rtk-gain-summary-grid").textContent, /777/);
    assert.doesNotMatch(finalPanel.container.querySelector(".rtk-gain-summary-grid").textContent, /111/);
  } finally { await finalPanel.close(); }
});
