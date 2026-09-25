// @vitest-environment jsdom
import React, { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import type { PluginSummary } from "@/types";
import type { LoadedPluginUiModule } from "@/service/plugin-ui-module-runtime";
import { loadPersistentPluginUiModule, invalidatePersistentPluginUiModules } from "@/service/plugin-ui-module-runtime";
import { DynamicPluginMountPanel } from "./dynamic-plugin-mount-panel";

vi.mock("@/i18n", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/i18n")>();
  const t = (key: import("@/i18n").TranslationKey) => actual.readLocaleMessage("en-US", key);
  return { ...actual, useI18n: () => ({ locale: "en-US", setLocale: () => undefined, t }) };
});
vi.mock("@/service/plugin-ui-module-runtime", () => ({ loadPersistentPluginUiModule: vi.fn(), invalidatePersistentPluginUiModules: vi.fn() }));
// mount is the exact contribution identity used by this isolated loader fixture.
// mount 是隔离加载夹具使用的精确贡献身份。
const mount = { id: "control" } as NonNullable<PluginSummary["extension"]>["uiMounts"][number];
let root: Root;
let host: HTMLDivElement;
beforeEach(() => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true); vi.clearAllMocks();
  host = document.createElement("div"); document.body.append(host); root = createRoot(host);
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); vi.unstubAllGlobals(); });

// loaded returns the authenticated identity and a synthetic component factory for one plugin.
// loaded 返回单个插件的认证身份与合成组件工厂。
// id selects the plugin; returns a loader result without executing an external module.
// id 选择插件；返回加载结果且不执行外部模块。
function loaded(id: string): LoadedPluginUiModule {
  return { source: { pluginId: id, mountId: "control", exportName: "create", source: "", messagesByLocale: {}, runtimeIdentity: "fixture:1", actionPolicies: [] },
    namespace: { create: () => () => <div>{id} loaded</div> } };
}

// plugin supplies the authenticated extension incarnation required by the real mount boundary.
// plugin 提供真实挂载边界要求的已认证扩展运行实例。
function plugin(id: string): PluginSummary {
  return { id, extension: { runtimeIdentity: "fixture:1", uiMounts: [mount] } } as PluginSummary;
}

it("clears a failed factory cache before explicitly retrying the authenticated module", async () => {
  const broken = loaded("fixture"); broken.namespace.create = () => { throw new Error("factory failed"); };
  vi.mocked(loadPersistentPluginUiModule).mockResolvedValueOnce(broken).mockResolvedValueOnce(loaded("fixture"));
  await act(async () => root.render(<DynamicPluginMountPanel plugin={plugin("fixture")} mount={mount} />));
  await vi.waitFor(async () => { await act(async () => undefined); expect(host.querySelector('[role="alert"]')?.textContent).toContain("factory failed"); });
  await act(async () => [...host.querySelectorAll("button")].find((button) => button.textContent === "Retry")!.click());
  expect(invalidatePersistentPluginUiModules).toHaveBeenCalledWith("fixture");
  expect(loadPersistentPluginUiModule).toHaveBeenCalledTimes(2);
  await vi.waitFor(async () => { await act(async () => undefined); expect(host.textContent).toBe("fixture loaded"); });
});

it("ignores an old plugin module that resolves after navigation", async () => {
  let complete!: (value: LoadedPluginUiModule) => void;
  vi.mocked(loadPersistentPluginUiModule).mockImplementationOnce(() => new Promise((resolve) => { complete = resolve; }))
    .mockResolvedValueOnce(loaded("new"));
  await act(async () => root.render(<DynamicPluginMountPanel plugin={plugin("old")} mount={mount} />));
  await act(async () => root.render(<DynamicPluginMountPanel plugin={plugin("new")} mount={mount} />));
  await act(async () => complete(loaded("old")));
  await vi.waitFor(async () => { await act(async () => undefined); expect(host.textContent).toBe("new loaded"); });
});

it("rebuilds the plugin UI with the host-selected conversation when the route changes", async () => {
  // module reads only SDK context, proving the plugin need not infer the active route.
  // module 只读取 SDK 上下文，证明插件无需自行推断当前路由。
  const module = loaded("fixture");
  module.namespace.create = (sdk: {
    context: { conversationId: string | null; clientHost: string };
    components: { CoreBrowserArea: unknown };
  }) => (
    () => <div>{sdk.context.conversationId}:{sdk.context.clientHost}:{typeof sdk.components.CoreBrowserArea}</div>
  );
  vi.mocked(loadPersistentPluginUiModule).mockResolvedValue(module);
  await act(async () => root.render(<DynamicPluginMountPanel plugin={plugin("fixture")} mount={mount} conversationId="conversation-a" />));
  await vi.waitFor(async () => { await act(async () => undefined); expect(host.textContent).toBe("conversation-a:browser:function"); });
  await act(async () => root.render(<DynamicPluginMountPanel plugin={plugin("fixture")} mount={mount} conversationId="conversation-b" />));
  await vi.waitFor(async () => { await act(async () => undefined); expect(host.textContent).toBe("conversation-b:browser:function"); });
});
