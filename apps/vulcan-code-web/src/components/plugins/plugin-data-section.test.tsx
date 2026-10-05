// @vitest-environment jsdom
import React, { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { TooltipProvider } from "@/components/ui/tooltip";
import { I18nProvider, readLocaleMessage } from "@/i18n";
import { createPluginSettingsPage, type PluginSettingsHost } from "./plugin-settings-page";
import type { PluginSettingsLayout } from "./plugin-settings-layout";

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

// layout is a paged page: a header, an inline switch tab and a statistics tab backed by one read action.
// layout 是分页页面：标题区、行内开关标签，以及由单个读取 Action 支撑的统计标签。
const layout: PluginSettingsLayout = {
  schema_version: 1,
  config: { read_action: "settings-get", write_action: "settings-update" },
  header: {
    status: { ready_message_key: "ready", unavailable_message_key: "down" },
    links: [{ url: "https://github.com/example/plugin", label_message_key: "repo", icon: "github" }],
    documents: [{ id: "license", asset: "LICENSE", label_message_key: "license", title_message_key: "license", icon: "license" }],
  },
  tabs: [
    { id: "general", title_message_key: "tab_general", sections: ["general"] },
    { id: "stats", title_message_key: "tab_stats", sections: ["stats"] },
  ],
  sections: [
    { id: "general", title_message_key: "general", layout: "inline", items: [
      { type: "field", key: "fast", control: "switch", label_message_key: "fast", description_message_key: "fast_help" },
    ] },
    {
      id: "stats", title_message_key: "stats",
      data: { action: "stats-get", refreshable: true, available_path: "available", error_path: "error", unavailable_message_key: "unavailable" },
      blocks: [
        { type: "metrics", web: { variant: "tiles", columns: 2 }, items: [
          { label_message_key: "saved", path: "summary.saved", format: "number" },
          { label_message_key: "capacity", ratio: { numerator: "summary.input", denominator: "summary.output" }, format: "ratio" },
        ] },
        { type: "chart", value_label_message_key: "saved", value_path: "saved", empty_message_key: "empty", limit: 2, periods: [
          { id: "daily", label_message_key: "daily", path: "daily", label_paths: ["date"], label_format: "date" },
          { id: "monthly", label_message_key: "monthly", path: "monthly", label_paths: ["month"], label_format: "month" },
        ] },
        { type: "table", path: "rows", title_message_key: "rows", count_message_key: "top", rank: true, empty_message_key: "empty", columns: [
          { path: "name", label_message_key: "name", format: "code" },
          { path: "saved", label_message_key: "impact", format: "bar" },
        ] },
      ],
    },
  ],
};

// statistics is one read result with three daily points, so the chart limit keeps only the latest two.
// statistics 是包含三个每日数据点的读取结果，因此图表上限只保留最近两个。
const statistics = {
  available: true, error: null,
  summary: { saved: 1200, input: 900, output: 300 },
  daily: [{ date: "2026-09-01", saved: 10 }, { date: "2026-09-02", saved: 20 }, { date: "2026-09-03", saved: 40 }],
  monthly: [{ month: "2026-09", saved: 70 }],
  rows: [{ name: "git", saved: 50 }, { name: "ls", saved: 25 }],
};

// mountPage renders the paged page with a scripted invoke whose statistics result the caller chooses.
// mountPage 使用脚本化 invoke 渲染分页页面，统计结果由调用方指定。
async function mountPage(result: unknown) {
  const invoke = vi.fn(async (actionId: string) => actionId === "settings-get" ? { fast: false } : result);
  const settingsHost: PluginSettingsHost = {
    invoke: invoke as PluginSettingsHost["invoke"], message: (key) => key === "top" ? "Top {{count}}" : `msg:${key}`,
    openExternalUrl: async () => undefined, readTextAsset: async () => "", subscribe: () => () => undefined,
  };
  const Page = createPluginSettingsPage(layout, { type: "object", properties: { fast: { type: "boolean" } } }, settingsHost);
  await act(async () => root.render(<I18nProvider initialLocale="en-US"><TooltipProvider><Page /></TooltipProvider></I18nProvider>));
  return invoke;
}

// button finds one button by its exact visible text.
// button 按精确可见文字查找一个按钮。
function button(text: string): HTMLButtonElement {
  const found = [...host.querySelectorAll<HTMLButtonElement>("button")].find((candidate) => candidate.textContent === text);
  if (!found) throw new Error(`Expected button ${text}; the label may have changed`);
  return found;
}

// Verifies tabs and header chrome come from the layout and the read action runs only when its tab first opens.
// 验证标签与标题区来自版面，读取 Action 仅在其标签首次打开时执行。
it("renders tabs, header status and inline switches, loading data lazily", async () => {
  const invoke = await mountPage(statistics);
  expect([...host.querySelectorAll("nav button")].map((tab) => tab.textContent)).toEqual(["msg:tab_general", "msg:tab_stats"]);
  expect(host.querySelector(".plugin-settings-header")?.textContent).toContain("msg:ready");
  expect(host.querySelector(".plugin-settings-header")?.textContent).toContain("msg:repo");
  expect(host.querySelector(".plugin-settings-inline-option")?.textContent).toContain("msg:fast");
  expect(invoke).not.toHaveBeenCalledWith("stats-get", expect.anything(), expect.anything());
  await act(async () => button("msg:tab_stats").click());
  expect(invoke).toHaveBeenCalledWith("stats-get", {}, { refresh: false });
  // The data tab has no editable fields, so the commit bar disappears.
  // 数据标签没有可编辑字段，因此提交栏消失。
  expect(host.textContent).not.toContain(readLocaleMessage("en-US", "plugins.settings.save"));
});

// Verifies tiles, the limited chart, local period switching and the ranked table with its count and impact bars.
// 验证方块、受限图表、本地周期切换，以及带计数与影响条的排名表格。
it("renders metrics, chart periods and tables from one result", async () => {
  const invoke = await mountPage(statistics);
  await act(async () => button("msg:tab_stats").click());
  expect([...host.querySelectorAll(".plugin-data-tile strong")].map((value) => value.textContent)).toEqual(["1,200", "3.0×"]);
  expect(host.querySelectorAll(".plugin-bar-chart-column")).toHaveLength(2);
  const calls = invoke.mock.calls.length;
  await act(async () => button("msg:monthly").click());
  expect(host.querySelectorAll(".plugin-bar-chart-column")).toHaveLength(1);
  expect(invoke.mock.calls).toHaveLength(calls);
  expect(host.querySelector(".plugin-data-heading")?.textContent).toContain("Top 2");
  expect([...host.querySelectorAll("tbody code")].map((cell) => cell.textContent)).toEqual(["git", "ls"]);
  expect([...host.querySelectorAll<HTMLElement>(".plugin-data-bar > span")].map((bar) => bar.style.width)).toEqual(["100%", "50%"]);
  await act(async () => button(readLocaleMessage("en-US", "plugins.data.refresh")).click());
  expect(invoke).toHaveBeenLastCalledWith("stats-get", {}, { refresh: true });
});

// Verifies an unavailable result shows the declared message with the reported reason instead of empty blocks.
// 验证不可用结果显示已声明文案与报告原因，而不是空数据块。
it("shows the unavailable reason when the result reports it", async () => {
  await mountPage({ ...statistics, available: false, error: "binary missing" });
  await act(async () => button("msg:tab_stats").click());
  expect(host.textContent).toContain("msg:unavailable");
  expect(host.textContent).toContain("binary missing");
  expect(host.querySelector(".plugin-data-tile")).toBeNull();
});
