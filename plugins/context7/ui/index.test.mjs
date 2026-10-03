import assert from "node:assert/strict";
import test from "node:test";
import { createRequire } from "node:module";
import { createPluginPanel } from "./index.mjs";

// require uses existing host dependencies without adding a plugin runtime library.
// require 使用既有宿主依赖，不向插件运行时增加库。
const require = createRequire(new URL("../../../apps/vulcan-code-web/package.json", import.meta.url));
// React and JSDOM mount actual plugin hooks inside an isolated document.
// React 和 JSDOM 在隔离文档内挂载真实插件钩子。
const React = require("react");
const { JSDOM } = require("jsdom");
// settings and bootstrap are exact synthetic non-secret action projections.
// settings 和 bootstrap 是准确的合成非密钥动作投影。
const settings = { defaultFastMode: true, requestTimeoutMs: 30000, retentionDays: 90 };
const bootstrap = { settings, credential: { configured: false, validationStatus: "unknown" }, usage: null, usageError: "Synthetic usage unavailable" };

// mount exposes the real panel and its declared background event without running network or storage effects.
// mount 暴露真实面板及其声明后台事件，不执行网络或存储副作用。
// invoke handles explicit synthetic actions; returns the DOM, refresh callback, notifications and cleanup.
// invoke 处理明确的合成动作；返回 DOM、刷新回调、通知及清理函数。
async function mount(invoke) {
  // dom and originals preserve the test runner's global browser state.
  // dom 和 originals 保留测试运行器的全局浏览器状态。
  const dom = new JSDOM("<!doctype html><div id='root'></div>");
  const originals = new Map(["window", "document", "IS_REACT_ACT_ENVIRONMENT"].map((key) => [key, Object.getOwnPropertyDescriptor(globalThis, key)]));
  globalThis.window = dom.window; globalThis.document = dom.window.document; globalThis.IS_REACT_ACT_ENVIRONMENT = true;
  // notifications and eventHandler capture only the public SDK boundaries.
  // notifications 和 eventHandler 仅捕获公开 SDK 边界。
  const notifications = [];
  let eventHandler = () => {};
  // plain and Button retain visible labels and native button semantics.
  // plain 和 Button 保留可见标签及原生按钮语义。
  const plain = ({ children, title, label, value }) => React.createElement("div", null, children ?? [title, label, value].filter(Boolean).join(" "));
  const Button = ({ children, variant, size, ...props }) => React.createElement("button", props, children);
  // Input and Switch keep native field semantics; Switch maps the host checked contract onto a checkbox.
  // Input 和 Switch 保留原生字段语义；Switch 把宿主 checked 契约映射到复选框。
  const Input = ({ density, ...props }) => React.createElement("input", props);
  const Switch = ({ checked, onCheckedChange, ...props }) => React.createElement("input", { ...props, type: "checkbox", checked, onChange: (event) => onCheckedChange(event.currentTarget.checked) });
  // SelectControl forwards the host's exact value-change contract through a native select.
  // SelectControl 通过原生下拉框转发宿主准确的值变更契约。
  const SelectControl = ({ ariaLabel, value, options, onValueChange, disabled }) => React.createElement("select", {
    "aria-label": ariaLabel, value, disabled, onChange: (event) => onValueChange(event.currentTarget.value),
  }, options.map((option) => React.createElement("option", { key: option.value, value: option.value }, option.label)));
  // sdk supplies no network, credentials or real event bus.
  // sdk 不提供网络、凭据或真实事件总线。
  const sdk = { React, locale: "en-US", invoke, t: (_key, fallback) => fallback, formatNumber: String,
    components: { Badge: plain, Button, Input, Switch, MetricCard: plain, SelectControl, ErrorState: plain, EmptyState: plain, StreamPlaceholder: plain,
      HelpTooltip: ({ ariaLabel }) => React.createElement("button", { "aria-label": ariaLabel }) },
    events: { subscribe: (_name, callback) => { eventHandler = callback; return () => { eventHandler = () => {}; }; } },
    notifications: { success: (message) => notifications.push(message), error: (message) => notifications.push(message) },
  };
  // root owns the actual hook-based panel under React StrictMode.
  // root 在 React 严格模式下持有真实钩子面板。
  const root = require("react-dom/client").createRoot(document.getElementById("root"));
  await React.act(async () => root.render(React.createElement(React.StrictMode, null, React.createElement(createPluginPanel(sdk)))));
  return { container: document.getElementById("root"), notifications, refresh: () => eventHandler(), close: async () => {
    await React.act(async () => root.unmount()); dom.window.close();
    for (const [key, descriptor] of originals) { if (descriptor) Object.defineProperty(globalThis, key, descriptor); else delete globalThis[key]; }
  } };
}

test("partial usage failure keeps authentication visible and secondary explanations collapsed", async () => {
  // calls records the actual declared actions issued by the mounted plugin.
  // calls 记录挂载插件发出的真实声明动作。
  const calls = [];
  const panel = await mount(async (action) => { calls.push(action); return bootstrap; });
  try {
    assert.deepEqual(calls, ["bootstrap-get"]);
    assert.ok(panel.container.querySelector('input[type="password"]'));
    assert.equal(panel.container.querySelector('input[type="password"]').disabled, false);
    assert.ok([...panel.container.querySelectorAll("button")].find((button) => button.textContent === "Retry"));
    assert.ok(panel.container.querySelector('[aria-label="Local request statistics"]'));
    assert.ok(panel.container.querySelector('[aria-label="Fast mode by default"]'));
  } finally { await panel.close(); }
});

test("retains settings drafts across background refresh and accepts only one concurrent save", async () => {
  // saveCount and rejectSave represent an acknowledged command boundary without writing any configuration.
  // saveCount 和 rejectSave 表示命令确认边界，不写入任何配置。
  let saveCount = 0;
  let rejectSave;
  const panel = await mount(async (action, args) => {
    if (action === "bootstrap-get") return bootstrap;
    if (action === "settings-update") {
      saveCount += 1;
      assert.equal(args.defaultFastMode, false);
      return new Promise((_, reject) => { rejectSave = reject; });
    }
    throw new Error(`Unexpected action: ${action}`);
  });
  try {
    // checkbox and save are the actual controls rendered by the plugin.
    // checkbox 和 save 是插件实际渲染的控件。
    const checkbox = panel.container.querySelector('input[type="checkbox"]');
    const save = [...panel.container.querySelectorAll("button")].find((button) => button.textContent === "Save settings");
    assert.equal(save.disabled, true);
    await React.act(async () => checkbox.click());
    assert.equal(checkbox.checked, false);
    await React.act(async () => panel.refresh());
    assert.equal(checkbox.checked, false);
    assert.equal(save.disabled, false);
    await React.act(async () => { save.click(); save.click(); });
    assert.equal(saveCount, 1);
    await React.act(async () => rejectSave(new Error("Synthetic settings failure")));
    assert.equal(save.disabled, false);
    assert.equal(checkbox.checked, false);
    assert.ok(panel.notifications.includes("The operation failed. Please retry."));
  } finally { await panel.close(); }
});
