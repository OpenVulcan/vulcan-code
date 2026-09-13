// @vitest-environment jsdom
import React, { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { ComposerKnowledgeControl, type ComposerKnowledgeControlProps } from "./composer-knowledge-control";
import { DropdownMenu, DropdownMenuContent, DropdownMenuTrigger } from "@/components/ui/dropdown-menu";
import type { KnowledgeLibrarySummary, SessionKnowledgeLibrarySelectionSummary } from "@/types";

// services isolate asynchronous selection ownership from real libraries and conversations.
// services 将异步选择归属与真实知识库、会话隔离。
const services = vi.hoisted(() => ({ libraries: vi.fn(), bindings: vi.fn(), selection: vi.fn(), save: vi.fn(), subscribe: vi.fn() }));
vi.mock("@/service/knowledge-base-service", () => ({ listKnowledgeLibraries: services.libraries,
  listProjectKnowledgeBindings: services.bindings, getSessionKnowledgeLibraries: services.selection,
  setSessionKnowledgeLibraries: services.save, subscribeKnowledgeProjectBindingsChanged: services.subscribe }));
// translate returns stable semantic keys; key identifies the requested message, returned unchanged.
// translate 返回稳定语义键；key 标识请求的文案，原样返回。
function translate(key: string): string { return key; }
vi.mock("@/i18n", () => ({ useI18n: () => ({ t: translate }) }));
// library provides the complete current settings contract without any model access.
// library 提供完整的当前设置契约，不访问模型。
const library: KnowledgeLibrarySummary = {
  id: "library-a", name: "Library A", description: "", lifecycleState: "active", documentCount: 0, chunkCount: 0, indexStatus: "ready", createdAtMs: 1, updatedAtMs: 1,
  settings: { retrievalMode: "raw_bm25", fallbackPolicy: "explicit_only", curation: { enabled: false, promptVersion: "knowledge-display-v1" }, displayIndex: { status: "not_created", generatedDocumentCount: 0 }, embedding: { enabled: false, embeddingBatchSize: 10, batchSizeSource: "fallback_default" }, vectorIndex: { status: "disabled" } },
};
// inherited is the authoritative default projection for the synthetic project.
// inherited 是合成项目的权威默认投影。
const inherited: SessionKnowledgeLibrarySelectionSummary = { mode: "inherit", libraryIds: [], effectiveLibraryIds: ["library-a"] };
// root and host own only this test's mounted document.
// root 与 host 仅持有本测试挂载的文档。
let root: Root;
let host: HTMLDivElement;
beforeEach(() => {
  vi.resetAllMocks(); vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  vi.stubGlobal("ResizeObserver", class { observe() {} unobserve() {} disconnect() {} });
  Element.prototype.scrollIntoView = vi.fn();
  services.libraries.mockResolvedValue([library, { ...library, id: "library-b", name: "Library B" }]);
  services.bindings.mockResolvedValue([{ projectId: "project-a", libraryId: "library-a", defaultEnabled: true }]);
  services.selection.mockResolvedValue(inherited); services.save.mockResolvedValue(undefined); services.subscribe.mockReturnValue(() => {});
  host = document.createElement("div"); document.body.append(host); root = createRoot(host);
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); vi.unstubAllGlobals(); });

// render mounts the actual submenu primitives; props select the exact draft or durable owner.
// render 挂载真实子菜单原语；props 选择精确草稿或持久会话归属。
// Returns after React has applied the current service reads.
// 在 React 应用当前服务读取后返回。
async function render(props: Partial<ComposerKnowledgeControlProps> = {}): Promise<void> {
  await act(async () => root.render(<DropdownMenu defaultOpen><DropdownMenuTrigger>Plus</DropdownMenuTrigger><DropdownMenuContent>
    <ComposerKnowledgeControl projectId="project-a" conversationId="session-a" onOpenKnowledge={vi.fn()} {...props} />
  </DropdownMenuContent></DropdownMenu>));
}
// openSubmenu opens the real knowledge submenu using its standard keyboard action.
// openSubmenu 使用标准键盘动作打开真实知识库子菜单。
// Returns after the submenu is mounted and its explicit refresh settles.
// 子菜单挂载且其显式刷新结束后返回。
async function openSubmenu(): Promise<void> {
  await act(async () => { document.querySelector('[aria-label="knowledge.composer.aria"]')!.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowRight", bubbles: true })); });
}
// item finds one exact menu label; label is visible copy, returning its current element.
// item 查找精确菜单文案；label 为可见文案，返回当前元素。
function item(label: string): HTMLElement {
  const match = [...document.querySelectorAll<HTMLElement>('[role^="menuitem"]')].find((entry) => entry.textContent?.trim() === label);
  expect(match, `Missing menu item: ${label}`).toBeDefined();
  return match!;
}

it("restores project defaults after an explicit session override through the same menu", async () => {
  services.selection.mockResolvedValue({ mode: "explicit", libraryIds: ["library-b"], effectiveLibraryIds: ["library-b"] });
  await render(); await openSubmenu();
  expect(item("knowledge.composer.inherit").getAttribute("aria-checked")).toBe("false");
  services.selection.mockResolvedValue(inherited);
  await act(async () => item("knowledge.composer.inherit").click());
  expect(services.save).toHaveBeenCalledExactlyOnceWith("project-a", "session-a", "inherit", []);
  expect(item("Library A").getAttribute("aria-checked")).toBe("true");
  expect(item("Library B").getAttribute("aria-checked")).toBe("false");
});

it("keeps draft inheritance and explicit library choices local without session RPCs", async () => {
  const change = vi.fn();
  await render({ conversationId: null, onDraftSelectionChange: change }); await openSubmenu();
  await act(async () => item("Library B").click());
  expect(change).toHaveBeenLastCalledWith({ mode: "explicit", libraryIds: ["library-a", "library-b"] });
  await act(async () => item("knowledge.composer.inherit").click());
  expect(change).toHaveBeenLastCalledWith({ mode: "inherit", libraryIds: [] });
  expect(item("Library B").getAttribute("aria-checked")).toBe("false");
  expect(services.save).not.toHaveBeenCalled(); expect(services.selection).not.toHaveBeenCalled();
});

it("clears the old project count while the new owner is loading and reports failure instead of emptiness", async () => {
  await render(); await openSubmenu();
  let reject!: (reason: Error) => void;
  services.libraries.mockImplementation(() => new Promise((_, fail) => { reject = fail; }));
  await render({ projectId: "project-b", conversationId: "session-b" });
  expect(document.querySelector('[aria-label="knowledge.composer.aria"]')?.textContent).toBe("knowledge.composer.label");
  expect(document.body.textContent).not.toContain("Library A");
  expect(document.body.textContent).toContain("knowledge.composer.loading");
  await act(async () => reject(new Error("library read failed")));
  expect(document.querySelector('[role="alert"]')).not.toBeNull();
  expect(document.body.textContent).not.toContain("knowledge.composer.noneAuthorized");
  expect(services.save).not.toHaveBeenCalled();
});

it("serializes writes and refreshes the new owner after an old session write completes", async () => {
  await render(); await openSubmenu();
  let finish!: () => void;
  services.save.mockImplementation(() => new Promise<void>((resolve) => { finish = resolve; }));
  await act(async () => { item("Library B").click(); item("Library B").click(); });
  expect(services.save).toHaveBeenCalledExactlyOnceWith("project-a", "session-a", "explicit", ["library-a", "library-b"]);
  await render({ projectId: "project-b", conversationId: "session-b" });
  services.libraries.mockResolvedValue([{ ...library, id: "library-c", name: "Library C" }]);
  services.selection.mockImplementation(async (id: string) => id === "session-b"
    ? { mode: "explicit", libraryIds: ["library-c"], effectiveLibraryIds: ["library-c"] } : inherited);
  await act(async () => finish());
  expect(item("Library C").getAttribute("aria-checked")).toBe("true");
  expect(document.body.textContent).not.toContain("Library A");
});

it("retries only reads after a committed write whose authoritative reload failed", async () => {
  await render(); await openSubmenu();
  services.selection.mockRejectedValueOnce(new Error("refresh failed after write"));
  await act(async () => item("Library B").click());
  expect(services.save).toHaveBeenCalledTimes(1);
  expect(document.querySelector('[role="alert"]')?.textContent).toBe("knowledge.composer.reloadFailed");
  expect(document.body.textContent).not.toContain("refresh failed after write");
  await act(async () => item("knowledge.composer.details").click());
  expect(document.body.textContent).toContain("refresh failed after write");
  await act(async () => item("knowledge.composer.retry").click());
  expect(services.save).toHaveBeenCalledTimes(1);
  expect(item("Library A").getAttribute("aria-checked")).toBe("true");
  expect(document.activeElement).toBe(item("knowledge.composer.inherit"));
});

it("keeps management accessible without a project", async () => {
  const manage = vi.fn();
  await render({ projectId: null, conversationId: null, onOpenKnowledge: manage }); await openSubmenu();
  await act(async () => item("knowledge.composer.manage").click());
  expect(manage).toHaveBeenCalledOnce(); expect(services.save).not.toHaveBeenCalled();
});

it("disables a draft that has no callback rather than pretending it was persisted", async () => {
  await render({ conversationId: null }); await openSubmenu();
  expect(item("Library B").getAttribute("aria-disabled")).toBe("true");
  expect(item("knowledge.composer.inherit").getAttribute("aria-disabled")).toBe("true");
  await act(async () => item("Library B").click());
  expect(services.save).not.toHaveBeenCalled();
  expect(item("Library B").getAttribute("aria-checked")).toBe("false");
});

it("keeps current effective libraries when inheritance is turned off", async () => {
  await render(); await openSubmenu();
  services.selection.mockResolvedValue({ mode: "explicit", libraryIds: ["library-a"], effectiveLibraryIds: ["library-a"] });
  await act(async () => item("knowledge.composer.inherit").click());
  expect(services.save).toHaveBeenCalledExactlyOnceWith("project-a", "session-a", "explicit", ["library-a"]);
  expect(item("knowledge.composer.inherit").getAttribute("aria-checked")).toBe("false");
  expect(item("Library A").getAttribute("aria-checked")).toBe("true");
});

it("rejects a stale read after switching conversations and recovers a rejected write by reads only", async () => {
  await render(); await openSubmenu();
  let oldRead!: (value: SessionKnowledgeLibrarySelectionSummary) => void;
  services.selection.mockImplementationOnce(() => new Promise((resolve) => { oldRead = resolve; }));
  await render({ conversationId: "old-slow" });
  services.selection.mockResolvedValue({ mode: "explicit", libraryIds: [], effectiveLibraryIds: [] });
  await render({ conversationId: "new-current" });
  await act(async () => oldRead(inherited));
  expect(item("Library A").getAttribute("aria-checked")).toBe("false");
  services.save.mockRejectedValueOnce(new Error("write rejected"));
  await act(async () => item("Library A").click());
  expect(document.querySelector('[role="alert"]')?.textContent).toBe("knowledge.composer.saveFailed");
  await act(async () => item("knowledge.composer.retry").click());
  expect(services.save).toHaveBeenCalledExactlyOnceWith("project-a", "new-current", "explicit", ["library-a"]);
  expect(item("Library A").getAttribute("aria-checked")).toBe("false");
});

it("does not start deferred or confirmation reads after a menu owner unmounts", async () => {
  await render(); await openSubmenu();
  let finish!: () => void;
  services.save.mockImplementation(() => new Promise<void>((resolve) => { finish = resolve; }));
  await act(async () => item("Library B").click());
  await act(async () => services.subscribe.mock.calls.at(-1)![0]("project-a"));
  const readsBeforeUnmount = services.selection.mock.calls.length;
  const listsBeforeUnmount = services.libraries.mock.calls.length;
  await act(async () => root.render(null));
  await act(async () => finish());
  expect(services.selection).toHaveBeenCalledTimes(readsBeforeUnmount);
  expect(services.libraries).toHaveBeenCalledTimes(listsBeforeUnmount);
});
