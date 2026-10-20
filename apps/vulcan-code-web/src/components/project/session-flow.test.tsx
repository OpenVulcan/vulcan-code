// @vitest-environment jsdom
import * as React from "react";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { I18nProvider } from "@/i18n";
import { SidebarProvider } from "@/ui-kit/shell/sidebar";
import { TooltipProvider } from "@/ui-kit/primitives/tooltip";
import type { ConversationSummary } from "@/types";
import { ProjectSidebar, sessionWhen } from "./project-sidebar";
import { setSidebarFlow } from "./sidebar-flow";

// NOW is a fixed local noon so calendar buckets do not depend on when the test runs.
// NOW 是固定的本地正午，使日历分组不依赖测试运行时间。
const NOW = new Date(2026, 9, 19, 12, 0, 0).getTime();
const DAY = 24 * 60 * 60 * 1000;

// session builds one conversation fixture updated a given number of milliseconds before NOW.
// session 构建一个在 NOW 之前给定毫秒数更新的会话测试数据。
function session(id: string, projectId: string, ageMs: number): ConversationSummary {
  return { id, title: `title-${id}`, status: "idle", projectId, lastUpdatedAt: String(NOW - ageMs) };
}

let container: HTMLDivElement;
let root: Root;
beforeEach(() => {
  Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
  vi.useFakeTimers({ now: NOW, toFake: ["Date"] });
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
});
afterEach(async () => {
  await act(async () => root.unmount());
  container.remove();
  vi.useRealTimers();
});

// mount renders the panel with two projects; the first is active.
// mount 渲染含两个项目的面板，第一个为活动项目。
async function mount(onCreateDedicatedConversation = vi.fn()) {
  const noop = () => undefined;
  await act(async () => root.render(
    <I18nProvider initialLocale="en-US"><TooltipProvider><SidebarProvider defaultOpen>
      <ProjectSidebar
        activeProjectId="a"
        conversations={[session("a1", "a", 60_000), session("a2", "a", DAY + 60_000), session("a3", "a", 40 * DAY), session("b1", "b", 60_000)]}
        onConversationMenuAction={noop}
        onCreateConversation={noop}
        onCreateDedicatedConversation={onCreateDedicatedConversation}
        onCreateProject={noop}
        onExpandedProjectIdsChange={noop}
        onProjectMenuAction={noop}
        onReorderProjects={noop}
        onProjectGroupAction={async () => undefined}
        onSelectConversation={noop}
        onSelectProject={noop}
        pinnedConversationIds={new Set()}
        projects={[{ id: "a", name: "Alpha", path: "D:/a" }, { id: "b", name: "Beta", path: "D:/b" }]}
        rail={null}
        selectedConversationId={null}
      />
    </SidebarProvider></TooltipProvider></I18nProvider>,
  ));
}

const rowTitles = () => [...container.querySelectorAll(".vk-tree-session .vk-tree-title")].map((title) => title.textContent);

it("buckets sessions by local calendar day, unknown times last", () => {
  expect(sessionWhen(session("x", "a", 60_000), NOW)).toBe("today");
  expect(sessionWhen(session("x", "a", 13 * 60 * 60 * 1000), NOW)).toBe("yesterday");
  expect(sessionWhen(session("x", "a", 3 * DAY), NOW)).toBe("week");
  expect(sessionWhen(session("x", "a", 20 * DAY), NOW)).toBe("month");
  expect(sessionWhen(session("x", "a", 90 * DAY), NOW)).toBe("older");
  expect(sessionWhen({ id: "x", title: "x", status: "idle" }, NOW)).toBe("older");
});

it("lists only the current project's sessions in recency groups and filters them by title", async () => {
  setSidebarFlow("session");
  await mount();
  expect(rowTitles()).toEqual(["title-a1", "title-a2", "title-a3"]);
  expect(container.querySelectorAll(".vk-session-group")).toHaveLength(3);
  const search = container.querySelector<HTMLInputElement>(".vk-nav-tools input")!;
  await act(async () => {
    const setValue = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!;
    setValue.call(search, "a2");
    search.dispatchEvent(new Event("input", { bubbles: true }));
  });
  expect(rowTitles()).toEqual(["title-a2"]);
});

it("switches every surface to the code flow from the brand switch", async () => {
  setSidebarFlow("session");
  await mount();
  const [sessionButton, codeButton] = container.querySelectorAll<HTMLButtonElement>(".vk-flow-switch button");
  expect(sessionButton.getAttribute("aria-pressed")).toBe("true");
  await act(async () => codeButton.click());
  expect(codeButton.getAttribute("aria-pressed")).toBe("true");
  expect(container.querySelector(".vk-project-tree")).not.toBeNull();
  expect(rowTitles()).toContain("title-b1");
});

it("opens a dedicated assistant draft in the current project", async () => {
  setSidebarFlow("session");
  const onCreateDedicatedConversation = vi.fn();
  await mount(onCreateDedicatedConversation);
  const actions = container.querySelectorAll<HTMLButtonElement>(".vk-sidebar-actions button");
  await act(async () => actions[actions.length - 1].click());
  expect(onCreateDedicatedConversation).toHaveBeenCalledExactlyOnceWith(expect.objectContaining({ id: "a" }));
});
