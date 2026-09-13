// @vitest-environment jsdom
import React, { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import type { PluginSummary } from "@/types";
import type { LoadedPluginUiModule } from "@/service/plugin-ui-module-runtime";
import { loadPersistentPluginUiModule, invalidatePersistentPluginUiModules } from "@/service/plugin-ui-module-runtime";
import { DynamicPluginMountPanel } from "./dynamic-plugin-mount-panel";

// translate remains stable so renderer updates do not create an unrelated module reload.
// translate 保持稳定，避免渲染器更新触发无关模块重载。
const translate = (key: string) => key;
vi.mock("@/i18n", () => ({ useI18n: () => ({ locale: "en-US", t: translate }) }));
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
  return { source: { pluginId: id, mountId: "control", exportName: "create", messagesByLocale: {} } as LoadedPluginUiModule["source"],
    namespace: { create: () => () => <div>{id} loaded</div> } };
}

it("clears a failed factory cache before explicitly retrying the authenticated module", async () => {
  const broken = loaded("fixture"); broken.namespace.create = () => { throw new Error("factory failed"); };
  vi.mocked(loadPersistentPluginUiModule).mockResolvedValueOnce(broken).mockResolvedValueOnce(loaded("fixture"));
  await act(async () => root.render(<DynamicPluginMountPanel plugin={{ id: "fixture" } as PluginSummary} mount={mount} />));
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
  await act(async () => root.render(<DynamicPluginMountPanel plugin={{ id: "old" } as PluginSummary} mount={mount} />));
  await act(async () => root.render(<DynamicPluginMountPanel plugin={{ id: "new" } as PluginSummary} mount={mount} />));
  await act(async () => complete(loaded("old")));
  await vi.waitFor(async () => { await act(async () => undefined); expect(host.textContent).toBe("new loaded"); });
});
