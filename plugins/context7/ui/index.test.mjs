import assert from "node:assert/strict";
import test from "node:test";
import { createRequire } from "node:module";
import { readFileSync } from "node:fs";
import { createPluginPanel } from "./index.mjs";
import { SUPPORTED_LOCALES } from "../../../apps/vulcan-code-web/src/i18n-locales.ts";

// require uses existing host dependencies without adding a plugin runtime library.
// require 使用既有宿主依赖，不向插件运行时增加库。
const require = createRequire(new URL("../../../apps/vulcan-code-web/package.json", import.meta.url));
// React and JSDOM mount actual plugin hooks inside an isolated document.
// React 和 JSDOM 在隔离文档内挂载真实插件钩子。
const React = require("react");
const { JSDOM } = require("jsdom");
// reset and usage are explicit non-secret quota and local statistics projections.
// reset 和 usage 是明确的非密钥配额及本机统计投影。
const reset = 1791187200;
const usage = {
  revision: 3, rangeDays: 30, credential: { configured: false, validationStatus: "unknown" },
  localStats: { requests: 1234, successes: 1230, failures: 4, averageDurationMs: 1234 },
  quotaSnapshot: { quotaTier: "free", quotaRemaining: 750, quotaLimit: 1000, quotaResetUnixSec: reset },
};

// mount renders the real usage page with page-kit stand-ins that keep visible text and native control semantics.
// mount 使用保留可见文字与原生控件语义的页面套件替身渲染真实用量页面。
// invoke handles synthetic actions, locale and messages select the catalog; returns the DOM, refresh trigger and cleanup.
// invoke 处理合成动作，locale 和 messages 选择语言目录；返回 DOM、刷新触发器与清理函数。
async function mount(invoke, locale = "en-US", messages = {}) {
  const dom = new JSDOM("<!doctype html><div id='root'></div>");
  const originals = new Map(["window", "document", "IS_REACT_ACT_ENVIRONMENT"].map((key) => [key, Object.getOwnPropertyDescriptor(globalThis, key)]));
  globalThis.window = dom.window; globalThis.document = dom.window.document; globalThis.IS_REACT_ACT_ENVIRONMENT = true;
  let eventHandler = () => {};
  // block renders every slot prop and children as plain text containers.
  // block 将全部插槽属性与子节点渲染为纯文本容器。
  const block = (props) => React.createElement("div", null, props.title, props.status, props.description, props.actions, props.label, props.value, props.children, props.footer);
  const Button = ({ children, variant, size, ...props }) => React.createElement("button", props, children);
  const SelectControl = ({ ariaLabel, value, options, onValueChange, disabled }) => React.createElement("select", {
    "aria-label": ariaLabel, value, disabled, onChange: (event) => onValueChange(event.currentTarget.value),
  }, options.map((option) => React.createElement("option", { key: option.value, value: option.value }, option.label)));
  const sdk = { React, locale, invoke, t: (key, fallback) => messages[key] ?? fallback, formatNumber: (value) => new Intl.NumberFormat(locale).format(value),
    components: { Badge: block, Button, MetricCard: block, MetricGrid: block, PageBody: block, PageEmptyState: block, PageNotice: block, PageSection: block,
      SelectControl, StreamPlaceholder: block },
    events: { subscribe: (_name, callback) => { eventHandler = callback; return () => { eventHandler = () => {}; }; } },
    notifications: { success: () => {}, error: () => {} },
    openExternalUrl: async () => {},
  };
  const root = require("react-dom/client").createRoot(document.getElementById("root"));
  await React.act(async () => root.render(React.createElement(createPluginPanel(sdk))));
  return { container: document.getElementById("root"), refresh: () => eventHandler(), close: async () => {
    await React.act(async () => root.unmount()); dom.window.close();
    for (const [key, descriptor] of originals) { if (descriptor) Object.defineProperty(globalThis, key, descriptor); else delete globalThis[key]; }
  } };
}

test("reads usage for the selected range and re-reads on range change and invalidation", async () => {
  const calls = [];
  const panel = await mount(async (action, args) => { calls.push([action, args.rangeDays]); return usage; });
  try {
    assert.deepEqual(calls, [["usage-get", 30]]);
    const select = panel.container.querySelector('select[aria-label="Local range"]');
    await React.act(async () => { select.value = "90"; select.dispatchEvent(new window.Event("change", { bubbles: true })); });
    await React.act(async () => panel.refresh());
    assert.deepEqual(calls.slice(1), [["usage-get", 90], ["usage-get", 90]]);
  } finally { await panel.close(); }
});

test("a failed read shows a retryable notice instead of stale numbers", async () => {
  let fail = true;
  const panel = await mount(async () => { if (fail) throw new Error("Synthetic usage failure"); return usage; });
  try {
    assert.ok(panel.container.textContent.includes("Context7 data could not be loaded."));
    assert.ok(panel.container.textContent.includes("No quota response"));
    fail = false;
    const retry = [...panel.container.querySelectorAll("button")].find((button) => button.textContent === "Retry");
    await React.act(async () => retry.click());
    assert.ok(!panel.container.textContent.includes("Context7 data could not be loaded."));
  } finally { await panel.close(); }
});

test("all host locales render translated labels and regional dates, numbers and units", async () => {
  const yaml = require("js-yaml");
  for (const locale of SUPPORTED_LOCALES.map((entry) => entry.code)) {
    // catalog is the exact authored catalog rather than an English fallback fixture.
    // catalog 是准确的已创作语言目录，而非英语回退测试数据。
    const catalog = yaml.load(readFileSync(new URL(`../locales/${locale === "en-US" ? "en" : locale}.yaml`, import.meta.url), "utf8"));
    const panel = await mount(async () => usage, locale, catalog.messages);
    try {
      assert.ok(panel.container.textContent.includes(catalog.messages.localStatsTitle), locale);
      assert.ok(panel.container.textContent.includes(new Intl.DateTimeFormat(locale, { day: "numeric", month: "short", year: "numeric" }).format(new Date(reset * 1000))), locale);
      assert.ok(panel.container.textContent.includes(new Intl.NumberFormat(locale).format(1234)), locale);
      assert.ok(panel.container.textContent.includes(new Intl.NumberFormat(locale, { style: "unit", unit: "millisecond", unitDisplay: "short" }).format(1234)), locale);
      assert.ok([...panel.container.querySelectorAll("option")].some((option) => option.textContent === new Intl.NumberFormat(locale, { style: "unit", unit: "day", unitDisplay: "long" }).format(90)), locale);
    } finally { await panel.close(); }
  }
});
