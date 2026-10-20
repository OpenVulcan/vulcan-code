// @vitest-environment jsdom
import * as React from "react";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { toast } from "@/ui-kit/primitives/toast";
import { TooltipProvider } from "@/ui-kit/primitives/tooltip";
import { DiffPanelToolbar } from "./diff-review-toolbar";
import { DiffReviewFloatingGitActions } from "./DiffReviewPanel/floating-git-actions";
import { DiffFileCard } from "./diff-file-card";
import { parseUnifiedDiff } from "./parseDiff";
import type { DiffRenderPreferences } from "./types";

vi.mock("@/i18n", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/i18n")>();
  const t = (key: import("@/i18n").TranslationKey) => actual.readLocaleMessage("en-US", key);
  return { ...actual, useI18n: () => ({ locale: "en-US", setLocale: () => undefined, t }) };
});
vi.mock("@/ui-kit/primitives/toast", () => ({ toast: { error: vi.fn() } }));
vi.mock("./diff-highlight", () => ({ diffLanguageForFile: () => null, useHighlightLanguage: () => false, useHighlightedDiffFileRevision: () => 0, useHighlightedDiffWindowRevision: () => 0 }));
vi.mock("./diff-render-worker-client", async () => {
  const { hunkSeparatorControlsHtml, hunkSeparatorLabelHtml } = await import("./diff-render-hunk-controls");
  // Use production-generated hunk controls; only the worker transport is replaced.
  // 使用生产代码生成的 hunk 控件，仅替换 Worker 传输。
  const html = `<div data-expand-index="0">${hunkSeparatorControlsHtml("120 unmodified lines", 0, 0, "line-info", { expandUpHunkIndex: 0, expandDownHunkIndex: 1 })}${hunkSeparatorLabelHtml("120 unmodified lines", 0, 0)}</div>`;
  return { usePlainDiffFileHtml: () => html, useHighlightedDiffFileHtml: () => "", usePlainDiffRowsHtml: () => html, useHighlightedDiffRowsHtml: () => "" };
});

// Synthetic parser output keeps the complete production diff shape without touching Git.
// 合成解析输出保持完整生产差异结构，不触及 Git。
const snapshot = parseUnifiedDiff("diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1 +1 @@\n-before\n+after\n");
const preferences: DiffRenderPreferences = { inlineDiff: true, overflowMode: "scroll", hunkSeparatorStyle: "line-info", indicatorStyle: "bars" };
let host: HTMLDivElement;
let root: Root;
beforeEach(() => {
  vi.clearAllMocks();
  Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
  host = document.createElement("div"); document.body.append(host); root = createRoot(host);
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); });

// Render actual controls under their standard floating-content provider.
// 在标准浮层 Provider 下渲染真实控件。
async function render(content: React.ReactNode) {
  await act(async () => root.render(<TooltipProvider>{content}</TooltipProvider>));
}

it("announces clipboard rejection and permits a later retry through the actual menu", async () => {
  const copy = vi.fn().mockRejectedValueOnce(new Error("Synthetic clipboard failure")).mockResolvedValue(undefined);
  await render(<DiffPanelToolbar snapshot={snapshot} selection={{ mode: "unstaged" }} viewMode="split" renderPreferences={preferences} onViewModeChange={vi.fn()} onRenderPreferencesChange={vi.fn()} onCopyRaw={copy} />);
  // Open the real Radix menu with its pointer event contract.
  // 按真实 Radix 指针事件契约打开菜单。
  const openMenu = async () => act(async () => document.querySelector<HTMLButtonElement>('[title="More diff actions"]')!.dispatchEvent(new MouseEvent("pointerdown", { bubbles: true, button: 0 })));
  const copyItem = () => [...document.querySelectorAll<HTMLElement>('[role="menuitem"]')].find((item) => item.textContent === "Copy raw diff")!;
  await openMenu();
  expect(copyItem()).toBeTruthy();
  await act(async () => copyItem().click());
  expect(copy).toHaveBeenCalledExactlyOnceWith(snapshot.rawDiff);
  expect(toast.error).toHaveBeenCalledWith("Synthetic clipboard failure");
  await openMenu(); await act(async () => copyItem().click());
  expect(copy).toHaveBeenCalledTimes(2);
});

it("restores search focus on Escape and does not route button Enter to the next match", async () => {
  const next = vi.fn();
  await render(<DiffPanelToolbar snapshot={snapshot} selection={{ mode: "unstaged" }} viewMode="split" renderPreferences={preferences} onViewModeChange={vi.fn()} onRenderPreferencesChange={vi.fn()} onSearchQueryChange={vi.fn()} onSearchSubmit={next} onSearchPrevious={vi.fn()} canSearchPrevious canSearchNext />);
  const trigger = document.querySelector<HTMLButtonElement>('[aria-label="Open diff search"]')!;
  await act(async () => trigger.click());
  const previous = document.querySelector<HTMLButtonElement>('[title="Previous result"]')!;
  await act(async () => previous.dispatchEvent(new KeyboardEvent("keydown", { bubbles: true, key: "Enter" })));
  expect(next).not.toHaveBeenCalled();
  const input = document.querySelector<HTMLInputElement>('[aria-label="Search diff"]')!;
  await act(async () => { input.focus(); input.dispatchEvent(new KeyboardEvent("keydown", { bubbles: true, key: "Enter" })); });
  expect(next).toHaveBeenCalledOnce();
  await act(async () => input.dispatchEvent(new KeyboardEvent("keydown", { bubbles: true, key: "Escape" })));
  expect(document.activeElement).toBe(trigger);
  expect(document.querySelector('[aria-label="Search diff"]')).toBeNull();
});

it("locks floating Git actions until rejection and then permits retry", async () => {
  let reject!: (error: Error) => void;
  const action = vi.fn(() => new Promise<void>((_resolve, fail) => { reject = fail; }));
  await render(<DiffReviewFloatingGitActions fileCount={1} mode="unstaged" selectedFile={snapshot.files[0]} busy={false} onAction={action} />);
  const stage = [...host.querySelectorAll<HTMLButtonElement>("button")].find((button) => button.textContent === "Stage this file")!;
  await act(async () => { stage.click(); stage.click(); });
  expect(action).toHaveBeenCalledTimes(1);
  expect(stage.disabled).toBe(true);
  await act(async () => reject(new Error("Synthetic staging failure")));
  expect(stage.disabled).toBe(false);
  expect(toast.error).toHaveBeenCalledWith("Synthetic staging failure");
});

it("routes focusable Enter and Space hunk controls to their exact direction and index", async () => {
  const load = vi.fn();
  const file = snapshot.files[0];
  await render(<DiffFileCard file={file} collapsed={false} scrollContainerRef={{ current: host }} scrollState={{ scrollTop: 0, clientHeight: 600, revision: 0 }} viewMode="unified" renderPreferences={preferences} onLoadFileDetail={load} onCollapsedChange={vi.fn()} />);
  const up = host.querySelector<HTMLElement>('[data-expand-up]')!;
  const down = host.querySelector<HTMLElement>('[data-expand-down]')!;
  expect(up.tabIndex).toBe(0); expect(up.getAttribute("aria-label")).toBe("Expand context above");
  await act(async () => { up.focus(); up.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true })); });
  expect(load).toHaveBeenLastCalledWith(file, 53, { hunkIndex: 0, direction: "up" });
  await act(async () => down.dispatchEvent(new KeyboardEvent("keydown", { key: " ", bubbles: true })));
  expect(load).toHaveBeenLastCalledWith(file, 53, { hunkIndex: 1, direction: "down" });
  expect(host.querySelector(".diff-file-header")?.getAttribute("aria-expanded")).toBe("true");
});
