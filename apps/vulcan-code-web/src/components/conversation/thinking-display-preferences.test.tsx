// @vitest-environment jsdom
import React, { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { I18nProvider } from "@/i18n";
import { useThinkingDisplayMode } from "./chat-area/visual-state";
import {
  ThinkingDisplaySystemContext, moveConversationThinkingPopupOverride,
  setConversationThinkingPopupOverride, useThinkingDisplayPreference,
} from "./thinking-display-preferences";
import { ThinkingPopupSettingsSection } from "@/components/features/settings/thinking-popup-settings";
import type { ThinkingDisplayMode } from "./thinking-display";

// Test-owned DOM and storage keep all verification separate from real user settings.
// 测试自有 DOM 与存储使所有验证与真实用户设置隔离。
let host: HTMLDivElement;
let root: Root;
beforeEach(() => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  // records emulates the browser's persistent document for this isolated test.
  // records 模拟此隔离测试中的浏览器持久文档。
  const records = new Map<string, string>();
  vi.stubGlobal("localStorage", {
    getItem: (key: string) => records.get(key) ?? null,
    setItem: (key: string, value: string) => { records.set(key, value); },
  });
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
});
afterEach(async () => {
  await act(async () => root.unmount());
  host.remove();
  vi.unstubAllGlobals();
});

/** Exercises ChatArea's exact selection hook; props identify the surface and service-owned default.
 * 验证 ChatArea 的实际选择钩子；props 标识界面及服务持有的默认值。
 */
function Probe(props: { id: string; global: ThinkingDisplayMode }) {
  // state uses the same session-scoped callback wired to the composer.
  // state 使用接入输入区的同一个会话作用域回调。
  const state = useThinkingDisplayMode({ conversationId: props.id, authoritativeMode: props.global });
  return <div data-session={props.id}><output>{state.mode}</output>
    <button onClick={() => state.selectMode("inline_output")}>关闭</button>
    <button onClick={() => setConversationThinkingPopupOverride(props.id, null)}>继承</button>
  </div>;
}

it("keeps inherited sessions live with global changes while isolating explicit choices", async () => {
  // render changes only the authoritative global default.
  // render 仅改变权威全局默认值。
  const render = async (global: ThinkingDisplayMode) => act(async () => root.render(<>
    <Probe id="popup-a" global={global} /><Probe id="popup-b" global={global} />
  </>));
  await render("popup_only");
  await act(async () => host.querySelector<HTMLButtonElement>("button")!.click());
  expect(Array.from(host.querySelectorAll("output"), (node) => node.textContent)).toEqual(["inline_output", "popup_only"]);
  await render("inline_output");
  expect(Array.from(host.querySelectorAll("output"), (node) => node.textContent)).toEqual(["inline_output", "inline_output"]);
  await render("popup_only");
  expect(Array.from(host.querySelectorAll("output"), (node) => node.textContent)).toEqual(["inline_output", "popup_only"]);
  await act(async () => host.querySelectorAll<HTMLButtonElement>("button")[1].click());
  expect(Array.from(host.querySelectorAll("output"), (node) => node.textContent)).toEqual(["popup_only", "popup_only"]);
});

it("moves draft popup choices to the acknowledged session without copying defaults", async () => {
  setConversationThinkingPopupOverride("popup-draft", false);
  moveConversationThinkingPopupOverride("popup-draft", "popup-persisted");
  await act(async () => root.render(<>
    <Probe id="popup-persisted" global="popup_only" /><Probe id="popup-draft" global="popup_only" />
  </>));
  expect(Array.from(host.querySelectorAll("output"), (node) => node.textContent)).toEqual(["inline_output", "popup_only"]);
  expect(JSON.parse(localStorage.getItem("vulcan-thinking-popup-overrides-v1")!)).toMatchObject({ "popup-persisted": false });
});

it("saves the global popup mode while keeping explicit session popup choices", async () => {
  setConversationThinkingPopupOverride("popup-explicit", true);
  // save records only global settings-page writes.
  // save 仅记录全局设置页面写入。
  const save = vi.fn(async (_mode: ThinkingDisplayMode) => true);
  /** Owns a synthetic service snapshot and exposes a session preview; returns the real global editor.
   * 持有合成服务快照并展示会话预览；返回真实全局编辑器。
   */
  function Harness() {
    // global is replaced only after a successful service save.
    // global 仅在服务保存成功后替换。
    const [global, setGlobal] = React.useState<ThinkingDisplayMode>("popup_only");
    const session = useThinkingDisplayPreference("popup-explicit", global);
    return <I18nProvider initialLocale="zh-CN"><ThinkingDisplaySystemContext.Provider value={{
      mode: global, save: async (mode) => {
        const saved = await save(mode);
        if (saved) setGlobal(mode);
        return saved;
      },
    }}><ThinkingPopupSettingsSection /><output>{session.mode}</output></ThinkingDisplaySystemContext.Provider></I18nProvider>;
  }
  await act(async () => root.render(<Harness />));
  await act(async () => host.querySelector<HTMLButtonElement>('[role="switch"]')!.click());
  expect(save).toHaveBeenCalledExactlyOnceWith("inline_output");
  expect(host.querySelector('[role="switch"]')?.getAttribute("aria-checked")).toBe("false");
  expect(host.querySelector("output")?.textContent).toBe("popup_only");
});

it("retains the saved global mode and reports rejected persistence", async () => {
  await act(async () => root.render(<I18nProvider initialLocale="zh-CN">
    <ThinkingDisplaySystemContext.Provider value={{ mode: "first_inline_with_popup", save: async () => false }}>
      <ThinkingPopupSettingsSection />
    </ThinkingDisplaySystemContext.Provider>
  </I18nProvider>));
  await act(async () => host.querySelector<HTMLButtonElement>('[role="switch"]')!.click());
  expect(host.querySelector('[role="switch"]')?.getAttribute("aria-checked")).toBe("true");
  expect(host.querySelector('[role="alert"]')?.textContent).toContain("系统默认值未更改");
});
