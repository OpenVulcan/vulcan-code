// @vitest-environment jsdom
import React, { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { I18nProvider, readLocaleMessage } from "@/i18n";
import { createPluginSettingsPage, type PluginSettingsHost } from "./plugin-settings-page";
import { checkedValues, parsePluginSettingsLayout, storedValues, valueIssue, type PluginSettingsField, type PluginSettingsLayout } from "./plugin-settings-layout";

let root: Root;
let host: HTMLDivElement;

beforeEach(() => {
  (globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
  host = document.createElement("div");
  document.body.appendChild(host);
  root = createRoot(host);
});

afterEach(async () => {
  await act(async () => root.unmount());
  host.remove();
});

// layout exercises a switch, a bounded number, a conditional checklist, a credential and an action.
// layout 覆盖开关、有界数字、条件清单、凭据与操作。
const layout: PluginSettingsLayout = {
  schema_version: 1,
  config: { read_action: "settings-get", write_action: "settings-update" },
  sections: [
    { id: "behavior", title_message_key: "behavior", items: [
      { type: "field", key: "fast", control: "switch", label_message_key: "fast" },
      { type: "field", key: "timeout", control: "number", label_message_key: "timeout" },
      { type: "field", key: "mode", control: "select", label_message_key: "mode", options: [{ value: "selected" }, { value: "all" }] },
      { type: "field", key: "routes", control: "checklist", label_message_key: "routes", selection: "exclude",
        visible_when: { key: "mode", values: ["all"] }, option_groups: [{ id: "main", title_message_key: "group", values: ["git", "ls"] }] },
    ] },
    { id: "auth", title_message_key: "auth", items: [
      { type: "credential", id: "key", label_message_key: "key", status_action: "credential-status", set_action: "credential-set", set_argument: "apiKey" },
      { type: "action", id: "test", action: "connection-test", label_message_key: "test", success_message_key: "tested" },
    ] },
  ],
};

// configSchema is the plugin configuration schema served beside the layout.
// configSchema 是与版面一同提供的插件配置 Schema。
const configSchema = { type: "object", properties: {
  fast: { type: "boolean" },
  timeout: { type: "integer", minimum: 10, maximum: 60 },
  mode: { type: "string", enum: ["selected", "all"] },
  routes: { type: "array", items: { type: "string" } },
} };

// en resolves one host message from the English catalog.
// en 从英文目录解析一条宿主文案。
const en = (key: Parameters<typeof readLocaleMessage>[1]) => readLocaleMessage("en-US", key);

// mountPage renders the page with a scripted invoke and returns the mock for assertions.
// mountPage 使用脚本化 invoke 渲染页面，并返回用于断言的模拟函数。
async function mountPage(config: Record<string, unknown>) {
  const invoke = vi.fn(async (actionId: string, argumentsValue: unknown) => {
    if (actionId === "settings-get") return config;
    if (actionId === "settings-update") return argumentsValue;
    if (actionId === "credential-status") return { configured: false };
    if (actionId === "credential-set") return { configured: true, validationStatus: "untested" };
    return {};
  });
  const settingsHost: PluginSettingsHost = { invoke: invoke as PluginSettingsHost["invoke"], message: (key) => `msg:${key}`, openExternalUrl: async () => undefined,
    readTextAsset: async () => "", subscribe: () => () => undefined };
  const Page = createPluginSettingsPage(layout, configSchema, settingsHost);
  await act(async () => root.render(<I18nProvider initialLocale="en-US"><Page /></I18nProvider>));
  return invoke;
}

// button finds one button by its exact visible text.
// button 按精确可见文字查找一个按钮。
function button(text: string): HTMLButtonElement {
  const found = [...host.querySelectorAll<HTMLButtonElement>("button")].find((candidate) => candidate.textContent === text);
  if (!found) throw new Error(`Expected button ${text}; the label may have changed`);
  return found;
}

// type replaces an input value the way React observes user typing.
// type 以 React 能观察到的方式替换输入值。
async function type(input: HTMLInputElement, value: string) {
  const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!;
  await act(async () => { setter.call(input, value); input.dispatchEvent(new Event("input", { bubbles: true })); });
}

// Verifies sections and rows come from the layout and saving sends the complete edited configuration.
// 验证区块与行来自版面，保存时发送完整的已编辑配置。
it("renders the layout through the page kit and saves the complete draft", async () => {
  const invoke = await mountPage({ fast: false, timeout: 30, mode: "selected", routes: [] });
  expect([...host.querySelectorAll(".page-section h3")].map((heading) => heading.textContent)).toEqual(["msg:behavior", "msg:auth"]);
  // The checklist stays hidden until its controlling field selects "all".
  // 在控制字段选择 all 之前，清单保持隐藏。
  expect(host.textContent).not.toContain("msg:routes");
  expect(button(en("plugins.settings.save")).disabled).toBe(true);
  await act(async () => host.querySelector<HTMLButtonElement>('[role="switch"]')!.click());
  expect(host.textContent).toContain(en("plugins.settings.unsaved"));
  await act(async () => button(en("plugins.settings.save")).click());
  expect(invoke).toHaveBeenCalledWith("settings-update", { fast: true, timeout: 30, mode: "selected", routes: [] });
  expect(host.textContent).toContain(en("plugins.settings.saved"));
});

// Verifies an out-of-range number shows its bounds and blocks saving.
// 验证超出范围的数字显示边界并阻止保存。
it("blocks saving while a field violates its schema", async () => {
  await mountPage({ fast: false, timeout: 30, mode: "selected", routes: [] });
  await type(host.querySelector<HTMLInputElement>('input[inputmode="numeric"]')!, "5");
  expect(host.querySelector('[role="alert"]')?.textContent).toContain("10");
  expect(button(en("plugins.settings.save")).disabled).toBe(true);
  expect(host.textContent).toContain(en("plugins.settings.invalid"));
});

// Verifies a credential is written once and the input is cleared without ever reading the secret back.
// 验证凭据仅写入一次，输入框随后清空，且从不回读密钥。
it("writes credentials through the declared set action", async () => {
  const invoke = await mountPage({ fast: false, timeout: 30, mode: "selected", routes: [] });
  const secret = host.querySelector<HTMLInputElement>('input[type="password"]')!;
  await type(secret, "ctx7sk-secret");
  await act(async () => button(en("plugins.settings.credentialSave")).click());
  expect(invoke).toHaveBeenCalledWith("credential-set", { apiKey: "ctx7sk-secret" });
  expect(host.querySelector<HTMLInputElement>('input[type="password"]')!.value).toBe("");
  expect(host.textContent).toContain(en("plugins.settings.credentialConfigured"));
  await act(async () => button("msg:test").click());
  expect(invoke).toHaveBeenCalledWith("connection-test", {});
  expect(host.textContent).toContain("msg:tested");
});

// A status refresh must preserve edits both when it succeeds and when the status endpoint fails.
// 状态刷新成功或状态端点失败时都必须保留编辑。
it.each([false, true])("preserves a draft after a connection test (status fails: %s)", async (statusFails) => {
  // The initial config is persisted, while the switch edit remains an unsaved draft.
  // 初始配置已持久化，开关编辑仍为未保存草稿。
  const invoke = await mountPage({ fast: false, timeout: 30, mode: "selected", routes: [] });
  await act(async () => host.querySelector<HTMLButtonElement>('[role="switch"]')!.click());
  invoke.mockImplementationOnce(async () => ({})).mockImplementationOnce(async () => {
    if (statusFails) throw new Error("synthetic status failure");
    return { configured: true };
  });
  await act(async () => button("msg:test").click());
  expect(host.querySelector('[role="switch"]')!.getAttribute("aria-checked")).toBe("true");
  expect(button(en("plugins.settings.save")).disabled).toBe(false);
  expect(invoke.mock.calls.filter(([action]) => action === "settings-get")).toHaveLength(1);
  if (statusFails) expect(host.textContent).toContain(en("plugins.settings.actionFailed"));
  else expect(host.textContent).toContain(en("plugins.settings.credentialConfigured"));
  await act(async () => button(en("plugins.settings.save")).click());
  expect(invoke).toHaveBeenLastCalledWith("settings-update", { fast: true, timeout: 30, mode: "selected", routes: [] });
});

// Verifies exclude checklists store unchecked offered values and keep values the layout does not offer.
// 验证排除型清单保存未勾选的提供值，并保留版面未提供的值。
it("maps checklist selection modes without dropping unknown stored values", () => {
  const field = layout.sections[0]!.items![3] as PluginSettingsField;
  expect([...checkedValues(field, ["git", "custom"])]).toEqual(["ls"]);
  expect(storedValues(field, ["git", "custom"], new Set(["git"]))).toEqual(["custom", "ls"]);
  expect(valueIssue(configSchema.properties.timeout, 61)).toEqual({ kind: "range", min: 10, max: 60 });
  expect(() => parsePluginSettingsLayout(JSON.stringify({ ...layout, schema_version: 2 }))).toThrow("schema_version 1");
});
