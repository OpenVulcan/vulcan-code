// @vitest-environment jsdom
import React, { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import {
  ToolCollapsePreferencesContext, moveConversationToolCollapseOverride,
  setConversationToolCollapseOverride, useToolCollapsePreference,
} from "./tool-collapse-preferences";
import { ToolCollapseSettingsSection } from "@/components/features/settings/tool-collapse-settings";
import { I18nProvider } from "@/i18n";

// Test-owned DOM never connects to a real service or saved user configuration.
// 测试自有 DOM 不连接真实服务或已保存用户配置。
let host: HTMLDivElement;
let root: Root;
beforeEach(() => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  // Memory-backed storage keeps assertions independent of Node's optional native localStorage.
  // 内存存储让断言不依赖 Node 可选的原生 localStorage。
  const stored = new Map<string, string>();
  vi.stubGlobal("localStorage", {
    getItem: (key: string) => stored.get(key) ?? null,
    setItem: (key: string, value: string) => { stored.set(key, value); },
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

/** Shows the exact effective preference; sessionId selects a surface and undefined selects the footer.
 * 显示精确生效偏好；sessionId 选择界面，undefined 选择底栏。
 */
function Probe(props: { sessionId?: string }) {
  // preference is resolved by the production hook.
  // preference 由生产钩子解析。
  const preference = useToolCollapsePreference(props.sessionId);
  return <output data-session={preference.scope}>{String(preference.enabled)}</output>;
}

it("isolates sessions, follows defaults only without overrides, and restores inheritance", async () => {
  // render applies an authoritative default without touching explicit session choices.
  // render 应用权威默认值，不触碰明确会话选择。
  const render = async (defaultEnabled: boolean, activeConversationId = "pref-a") => act(async () => root.render(
    <ToolCollapsePreferencesContext.Provider value={{ defaultEnabled, activeConversationId }}>
      <Probe /><Probe sessionId="pref-b" />
    </ToolCollapsePreferencesContext.Provider>,
  ));
  await render(true);
  expect(host.textContent).toBe("truetrue");
  await act(async () => setConversationToolCollapseOverride("pref-a", false));
  expect(host.textContent).toBe("falsetrue");
  await render(false);
  await act(async () => setConversationToolCollapseOverride("pref-b", true));
  expect(host.textContent).toBe("falsetrue");
  await render(true);
  expect(host.textContent).toBe("falsetrue");
  await act(async () => setConversationToolCollapseOverride("pref-a", null));
  expect(host.textContent).toBe("truetrue");
  await render(false);
  expect(host.textContent).toBe("falsetrue");
  expect(JSON.parse(localStorage.getItem("vulcan-tool-collapse-overrides-v1")!)).toMatchObject({ "pref-b": true });
});

it("transfers a submitted draft choice without leaking to another session", async () => {
  setConversationToolCollapseOverride("draft-folding-test", false);
  moveConversationToolCollapseOverride("draft-folding-test", "persisted-folding-test");
  await act(async () => root.render(<ToolCollapsePreferencesContext.Provider value={{ defaultEnabled: true, activeConversationId: "persisted-folding-test" }}>
    <Probe /><Probe sessionId="draft-folding-test" /><Probe sessionId="other-folding-test" />
  </ToolCollapsePreferencesContext.Provider>));
  expect(host.textContent).toBe("falsetruetrue");
});

it("shows save errors and retains the authoritative system default", async () => {
  // saveDefault simulates a rejected service mutation without changing its authoritative snapshot.
  // saveDefault 模拟被拒绝的服务修改，不改变权威快照。
  const saveDefault = vi.fn(async () => { throw new Error("synthetic save failure"); });
  await act(async () => root.render(<I18nProvider initialLocale="zh-CN">
    <ToolCollapsePreferencesContext.Provider value={{ defaultEnabled: true, activeConversationId: "pref-b", saveDefault }}>
      <ToolCollapseSettingsSection />
    </ToolCollapsePreferencesContext.Provider>
  </I18nProvider>));
  expect(host.querySelector('[role="switch"]')?.getAttribute("aria-checked")).toBe("true");
  await act(async () => host.querySelector<HTMLButtonElement>('[role="switch"]')!.click());
  expect(saveDefault).toHaveBeenCalledExactlyOnceWith(false);
  expect(host.querySelector('[role="alert"]')?.textContent).toBe("synthetic save failure");
  expect(host.querySelector('[role="switch"]')?.getAttribute("aria-checked")).toBe("true");
});

it("saves a changed system default and does not overwrite an explicit session choice", async () => {
  // Harness adopts the saved authoritative value like the workbench snapshot merge.
  // Harness 像工作台快照合并一样采用已保存权威值。
  function Harness() {
    // enabled owns this test's synthetic system default.
    // enabled 持有本测试的合成系统默认值。
    const [enabled, setEnabled] = React.useState(true);
    return <I18nProvider initialLocale="zh-CN"><ToolCollapsePreferencesContext.Provider value={{
      defaultEnabled: enabled, activeConversationId: "pref-b",
      saveDefault: async (next) => { setEnabled(next); },
    }}><ToolCollapseSettingsSection /><Probe /></ToolCollapsePreferencesContext.Provider></I18nProvider>;
  }
  setConversationToolCollapseOverride("pref-b", true);
  await act(async () => root.render(<Harness />));
  await act(async () => host.querySelector<HTMLButtonElement>('[role="switch"]')!.click());
  expect(host.querySelector('[role="switch"]')?.getAttribute("aria-checked")).toBe("false");
  expect(host.querySelector("output")?.textContent).toBe("true");
});
