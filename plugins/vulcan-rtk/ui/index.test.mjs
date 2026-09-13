import assert from "node:assert/strict";
import test from "node:test";
import { createRequire } from "node:module";
import { readFile } from "node:fs/promises";

// Dependencies come from the existing host; no runtime plugin bundle dependency is added.
// 依赖来自现有宿主，不添加运行时插件打包依赖。
const require = createRequire(new URL("../../../apps/vulcan-code-web/package.json", import.meta.url));
const React = require("react");
const { JSDOM } = require("jsdom");
// settings uses the real declared default shape in an isolated in-memory copy.
// settings 使用真实声明的默认结构，但只保留隔离内存副本。
const settings = JSON.parse(await readFile(new URL("../config/defaults.json", import.meta.url), "utf8"));
// moduleId keeps each test's cache isolated while allowing explicit same-generation remount tests.
// moduleId 隔离每个测试的缓存，并允许明确的同代次重新挂载测试。
let moduleId = 0;

// mount renders the authoritative plugin against controlled actions and native interaction adapters.
// mount 使用受控 Action 与原生交互适配器渲染权威插件。
// invoke supplies action responses; strict replays effects; factory optionally retains the module cache. Returns the DOM and cleanup.
// invoke 提供 Action 响应；strict 重放 Effect；factory 可保留模块缓存。返回 DOM 与清理函数。
async function mount(invoke, strict = false, factory = null) {
  const dom = new JSDOM("<!doctype html><div id='root'></div>");
  Object.assign(globalThis, { window: dom.window, document: dom.window.document, IS_REACT_ACT_ENVIRONMENT: true });
  // Host adapters retain accessible native controls without claiming host-layout verification.
  // 宿主适配器保留可访问原生控件，不声称验证宿主布局。
  const plain = ({ children, label }) => React.createElement("div", null, label, children);
  const Button = ({ children, size, variant, ...props }) => React.createElement("button", props, children);
  const Metadata = ({ label, onActivate, ariaLabel }) => React.createElement("button", { onClick: onActivate, "aria-label": ariaLabel }, label);
  const createPanel = factory ?? (await import(`./index.mjs?test=${++moduleId}`)).createPluginPanel;
  const sdk = { React, invoke, locale: "en-US", formatNumber: String, t: (_key, fallback) => fallback,
    components: { Button, Badge: plain, StatusPill: plain, HelpTooltip: plain, PluginMetadataLink: Metadata, TextDocumentDialog: plain } };
  const root = require("react-dom/client").createRoot(document.getElementById("root"));
  await React.act(async () => root.render(React.createElement(strict ? React.StrictMode : React.Fragment, null, React.createElement(createPanel(sdk)))));
  return { container: document.getElementById("root"), factory: createPanel, close: async () => {
    await React.act(async () => root.unmount()); dom.window.close();
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
    assert.equal(calls.filter((item) => item === "statistics-get").length, 1);
    assert.equal(button(panel, "Save").disabled, true);
    await React.act(async () => button(panel, "Retry").click());
    assert.equal(reads, 2);
    assert.doesNotMatch(panel.container.textContent, /settings unavailable/);
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
test("obsolete mount results cannot replace a later statistics snapshot", async () => {
  let finishOld;
  const old = await mount((action) => action === "settings-get" ? Promise.resolve(settings) : new Promise((resolve) => { finishOld = resolve; }));
  const factory = old.factory;
  await old.close();
  const fresh = { available: true, summary: { total_saved: 777 }, daily: [], byCommand: [] };
  const panel = await mount((action) => Promise.resolve(action === "settings-get" ? settings : fresh), false, factory);
  await React.act(async () => finishOld({ ...fresh, summary: { total_saved: 111 } }));
  await panel.close();
  let statisticReads = 0;
  const finalPanel = await mount((action) => { if (action === "statistics-get") statisticReads += 1; return Promise.resolve(settings); }, false, factory);
  try {
    assert.equal(statisticReads, 0);
    assert.match(finalPanel.container.querySelector(".rtk-gain-summary-grid").textContent, /777/);
    assert.doesNotMatch(finalPanel.container.querySelector(".rtk-gain-summary-grid").textContent, /111/);
  } finally { await finalPanel.close(); }
});
