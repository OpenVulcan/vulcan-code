import assert from "node:assert/strict";
import test from "node:test";
import { createRequire } from "node:module";
import { readFile } from "node:fs/promises";
import { createPluginPanel } from "./settings.mjs";
import { SUPPORTED_LOCALES } from "../../../apps/vulcan-code-web/src/i18n-locales.ts";

// Browser settings tests use the same React and DOM dependencies as the host WebUI.
// 浏览器设置测试使用与宿主 WebUI 相同的 React 和 DOM 依赖。
const require = createRequire(new URL("../../../apps/vulcan-code-web/package.json", import.meta.url));
const React = require("react");
const { JSDOM } = require("jsdom");
const defaults = JSON.parse(await readFile(new URL("../config/defaults.json", import.meta.url), "utf8"));

// mount renders the real plugin panel with a controlled plugin Action adapter.
// mount 使用受控插件 Action 适配器渲染真实插件面板。
// invoke handles actions, strict enables effect replay and messages supplies catalog text; returns DOM and cleanup.
// invoke 处理 Action，strict 启用 Effect 重放，messages 提供目录文案；返回 DOM 与清理函数。
async function mount(invoke, strict = false, messages = {}) {
  const dom = new JSDOM("<!doctype html><div id='root'></div>");
  Object.assign(globalThis, {
    window: dom.window, document: dom.window.document, IS_REACT_ACT_ENVIRONMENT: true,
  });
  const Button = ({ children, ...props }) => React.createElement("button", props, children);
  const labels = {
    actionTimeoutLabel: "Action timeout (ms)",
    navigationTimeoutLabel: "Navigation timeout (ms)",
    settleTimeoutLabel: "Settle time (ms)",
    ...messages,
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

test("all host locales translate console levels while preserving configuration values", async () => {
  // yaml reads actual catalogs through an existing host dependency.
  // yaml 通过既有宿主依赖读取实际语言目录。
  const yaml = require("js-yaml");
  // localeCodes derives coverage from the authoritative host language registry.
  // localeCodes 从权威宿主语言注册表派生覆盖范围。
  const localeCodes = SUPPORTED_LOCALES.map((entry) => entry.code);
  for (const locale of localeCodes) {
    // catalog contains the exact localized messages and writes captures the unchanged action contract.
    // catalog 包含准确的本地化文案，writes 捕获未改变的 Action 契约。
    const catalog = yaml.load(await readFile(new URL(`../locales/${locale === "en-US" ? "en" : locale}.yaml`, import.meta.url), "utf8"));
    const writes = [];
    // panel uses the real UI while substituting only explicit settings actions.
    // panel 使用真实界面，仅替换明确的设置 Action。
    const panel = await mount(async (action, values) => {
      if (action === "settings-get") return defaults;
      writes.push(values);
      return values;
    }, false, catalog.messages);
    try {
      // select is the rendered control whose visible text differs from its protocol values.
      // select 是已渲染的控件，其可见文字与协议值不同。
      const select = panel.container.querySelector("select");
      assert.equal(select.getAttribute("aria-label"), catalog.messages.consoleLevelLabel, locale);
      assert.deepEqual([...select.options].map((option) => option.value), ["error", "warning", "info", "debug"]);
      for (const option of select.options) assert.equal(option.textContent, catalog.messages[`consoleLevel_${option.value}`], locale);
      await React.act(async () => panel.container.querySelector("button").click());
      assert.equal(writes[0].consoleLevel, "warning", locale);
      assert.ok(panel.container.textContent.includes(catalog.messages.settingsSaved), locale);
    } finally { await panel.close(); }
  }
});

test("invalid read and persisted settings use localized errors", async () => {
  // messages makes translation observable without depending on the action error protocol.
  // messages 使翻译可观察，不依赖 Action 错误协议。
  const messages = { settingsResponseInvalid: "Réponse invalide", settingsPersistedInvalid: "Paramètres enregistrés invalides" };
  // invalidRead exercises the localized validation failure before a draft is accepted.
  // invalidRead 验证接受草稿前的本地化校验失败。
  const invalidRead = await mount(async () => ({}), false, messages);
  try { assert.equal(invalidRead.container.querySelector('[role="alert"]').textContent, messages.settingsResponseInvalid); }
  finally { await invalidRead.close(); }
  // invalidSave exercises the localized validation failure after a malformed save response.
  // invalidSave 验证保存响应格式错误后的本地化校验失败。
  const invalidSave = await mount(async (action) => action === "settings-get" ? defaults : {}, false, messages);
  try {
    await React.act(async () => invalidSave.container.querySelector("button").click());
    assert.equal(invalidSave.container.querySelector('[role="alert"]').textContent, messages.settingsPersistedInvalid);
  } finally { await invalidSave.close(); }
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
