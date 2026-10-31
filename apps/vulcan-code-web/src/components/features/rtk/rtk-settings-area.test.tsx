// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { I18nProvider } from "@/i18n";
import { loadRtkSettings, loadRtkStatistics, saveRtkSettings } from "@/service/rtk-service";
import type { RtkSettings, RtkSettingsSnapshot, RtkStatisticsSnapshot } from "@/types";
import { checkedRoutes, RtkSettingsArea, storedRoutes } from "./rtk-settings-area";

vi.mock("@/service/rtk-service", () => ({
  clearRtkStatistics: vi.fn(),
  loadRtkSettings: vi.fn(),
  loadRtkStatistics: vi.fn(),
  saveRtkSettings: vi.fn(),
}));

// settings is a `selected`-mode configuration that also keeps one route outside the offered catalog.
// settings 是 `selected` 模式配置，并保留一条可选目录之外的路由。
const settings: RtkSettings = {
  enabled: true,
  routeMode: "selected",
  enabledRoutes: ["custom", "git"],
  disabledRoutes: [],
  telemetryDisabled: true,
  gitCommitContextEnabled: false,
};
const snapshot: RtkSettingsSnapshot = { settings, curatedRoutes: ["git", "grep"], extendedRoutes: ["docker"] };
const unavailable: RtkStatisticsSnapshot = {
  available: false,
  error: "RTK executable not found",
  summary: { totalCommands: 0, totalInput: 0, totalOutput: 0, totalSaved: 0, avgSavingsPct: 0, oversizedExcluded: 0 },
  daily: [], weekly: [], monthly: [], byCommand: [],
};

let host: HTMLDivElement;
let root: Root;
beforeEach(() => {
  host = document.createElement("div"); document.body.append(host); root = createRoot(host);
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  vi.mocked(loadRtkSettings).mockResolvedValue(snapshot);
  vi.mocked(loadRtkStatistics).mockResolvedValue(unavailable);
  vi.mocked(saveRtkSettings).mockImplementation(async (saved) => ({ ...snapshot, settings: saved }));
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); vi.unstubAllGlobals(); vi.clearAllMocks(); });

async function mount() {
  await act(async () => root.render(<I18nProvider initialLocale="en-US"><RtkSettingsArea /></I18nProvider>));
  await act(async () => {});
}

it("saves the edited switches as one complete settings object", async () => {
  await mount();
  const save = host.querySelector<HTMLButtonElement>("[data-rtk-save]")!;
  expect(save.disabled).toBe(true);
  await act(async () => host.querySelector<HTMLButtonElement>('[data-rtk-setting="gitCommitContextEnabled"]')!.click());
  expect(save.disabled).toBe(false);
  await act(async () => save.click());
  expect(saveRtkSettings).toHaveBeenCalledExactlyOnceWith({ ...settings, gitCommitContextEnabled: true });
  expect(save.disabled).toBe(true);
});

it("projects each route mode onto the list it owns and keeps routes outside the catalog", () => {
  const universe = ["docker", "git", "grep"];
  expect([...checkedRoutes(settings, universe)]).toEqual(["git"]);
  expect(storedRoutes(settings, universe, new Set(["git", "grep"])).enabledRoutes).toEqual(["custom", "git", "grep"]);
  const all: RtkSettings = { ...settings, routeMode: "all", disabledRoutes: ["legacy"] };
  expect([...checkedRoutes(all, universe)]).toEqual(universe);
  const unchecked = storedRoutes(all, universe, new Set(["git"]));
  expect(unchecked.disabledRoutes).toEqual(["docker", "grep", "legacy"]);
  expect(unchecked.enabledRoutes).toEqual(settings.enabledRoutes);
});
