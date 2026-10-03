import assert from "node:assert/strict";
import test from "node:test";
import { createRequire } from "node:module";
import { readFile } from "node:fs/promises";
import { createPluginPanel } from "./settings.mjs";

// Browser settings tests use the same React and DOM dependencies as the host WebUI.
// 浏览器设置测试使用与宿主 WebUI 相同的 React 和 DOM 依赖。
const require = createRequire(new URL("../../../apps/vulcan-code-web/package.json", import.meta.url));
const React = require("react");
const { JSDOM } = require("jsdom");
const defaults = JSON.parse(await readFile(new URL("../config/defaults.json", import.meta.url), "utf8"));

// mount renders the real plugin panel with a controlled plugin Action adapter.
// mount 使用受控插件 Action 适配器渲染真实插件面板。
// invoke handles actions and strict enables React effect replay; returns the DOM and cleanup.
// invoke 处理 Action，strict 启用 React Effect 重放；返回 DOM 与清理函数。
async function mount(invoke, strict = false) {
  const dom = new JSDOM("<!doctype html><div id='root'></div>");
  Object.assign(globalThis, {
    window: dom.window, document: dom.window.document, IS_REACT_ACT_ENVIRONMENT: true,
  });
  const Button = ({ children, ...props }) => React.createElement("button", props, children);
  const labels = {
    actionTimeoutLabel: "Action timeout (ms)",
    navigationTimeoutLabel: "Navigation timeout (ms)",
    settleTimeoutLabel: "Settle time (ms)",
  };
  const sdk = { React, invoke, t: (key, fallback) => labels[key] ?? fallback, components: { Button, StreamPlaceholder: ({ label }) => React.createElement("p", { role: "status" }, label) } };
  const panel = createPluginPanel(sdk);
  const root = require("react-dom/client").createRoot(document.getElementById("root"));
  await React.act(async () => root.render(React.createElement(
    strict ? React.StrictMode : React.Fragment, null, React.createElement(panel),
  )));
  return {
    container: document.getElementById("root"),
    close: async () => {
      await React.act(async () => root.unmount());
      dom.window.close();
    },
  };
}

// changeInput performs the browser's native input event on an accessible controlled field.
// changeInput 对可访问的受控字段执行浏览器原生输入事件。
// panel is the mounted DOM, label selects the field, and value is the new string.
// panel 是已挂载 DOM，label 选择字段，value 是新字符串。
function changeInput(panel, label, value) {
  const input = panel.container.querySelector(`input[aria-label="${label}"]`);
  const setter = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, "value").set;
  setter.call(input, value);
  input.dispatchEvent(new window.Event("input", { bubbles: true }));
}

test("settings read, complete validation, and persisted response", async () => {
  const writes = [];
  const panel = await mount((action, argumentsValue) => {
    if (action === "settings-get") return Promise.resolve(defaults);
    writes.push(argumentsValue);
    return Promise.resolve(argumentsValue);
  });
  try {
    assert.equal(panel.container.querySelectorAll('input[inputmode="numeric"]').length, 3);
    assert.equal(panel.container.querySelector('select[aria-label="Console level"]').value, "warning");
    await React.act(async () => changeInput(panel, "Action timeout (ms)", "99"));
    assert.equal(panel.container.querySelector("button").disabled, true);
    assert.match(panel.container.textContent, /Enter valid timeouts/);
    await React.act(async () => changeInput(panel, "Action timeout (ms)", "7000"));
    await React.act(async () => panel.container.querySelector("button").click());
    assert.deepEqual(writes, [{ ...defaults, actionTimeoutMs: "7000" }]);
    assert.match(panel.container.textContent, /Settings saved/);
  } finally { await panel.close(); }
});

test("effect replay shares the read and a rejected write retains the draft", async () => {
  let reads = 0;
  let rejectWrite;
  const writes = [];
  const panel = await mount((action, argumentsValue) => {
    if (action === "settings-get") {
      reads += 1;
      return Promise.resolve(defaults);
    }
    writes.push(argumentsValue);
    return new Promise((_resolve, reject) => { rejectWrite = reject; });
  }, true);
  try {
    assert.equal(reads, 1);
    await React.act(async () => changeInput(panel, "Navigation timeout (ms)", "80000"));
    await React.act(async () => {
      panel.container.querySelector("button").click();
      panel.container.querySelector("button").click();
    });
    assert.equal(writes.length, 1);
    await React.act(async () => rejectWrite(new Error("save rejected")));
    assert.match(panel.container.textContent, /save rejected/);
    assert.equal(panel.container.querySelector('input[aria-label="Navigation timeout (ms)"]').value, "80000");
    assert.equal(panel.container.querySelector("button").disabled, false);
  } finally { await panel.close(); }
});
