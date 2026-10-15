// @vitest-environment jsdom
import React, { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { KnowledgeContent } from "./knowledge-content";
import { knowledgeDocumentFixture, knowledgeLibraryFixture } from "./knowledge-test-fixtures";

// Each synthetic read preserves the production directory/document ownership boundary.
// 每个合成读取保留生产目录及文档归属边界。
const service = vi.hoisted(() => ({ list: vi.fn(), read: vi.fn(), search: vi.fn() }));
vi.mock("@/service/knowledge-base-service", () => ({ listManagedKnowledgeDocuments: service.list, readManagedKnowledgeDocument: service.read, searchManagedKnowledgeLibrary: service.search }));
vi.mock("@/i18n", () => ({ useI18n: () => ({ t: (key: string) => key, locale: "en-US" }) }));
// This root owns the mounted content surface and portals.
// 此根拥有挂载内容区域及门户。
let root: Root;
// A detached host isolates DOM effects.
// 独立宿主隔离 DOM 副作用。
let host: HTMLDivElement;
beforeEach(() => {
  vi.resetAllMocks(); vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  service.list.mockImplementation(({ path }: { path: string }) => Promise.resolve({ entries: path === "" ? [{ kind: "folder", path: "notes", description: "Folder purpose", documentCount: 1, document: null }] : [{ kind: "document", path: knowledgeDocumentFixture.path, description: "", documentCount: 1, document: knowledgeDocumentFixture }], nextCursor: null, headChangeSeq: 1 }));
  service.read.mockImplementation(({ startLine }: { startLine: number }) => Promise.resolve({ document: { ...knowledgeDocumentFixture, lineCount: 9 }, content: "Body " + startLine, startLine, endLine: startLine === 1 ? 6 : 9, nextLine: startLine === 1 ? 7 : null }));
  host = document.createElement("div"); document.body.append(host); root = createRoot(host);
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); vi.unstubAllGlobals(); });

it("reads folders lazily, uses stable document IDs and follows the actual bounded line continuation", async () => {
  await act(async () => root.render(<KnowledgeContent library={knowledgeLibraryFixture} />));
  expect(service.list).toHaveBeenCalledExactlyOnceWith({ libraryId: knowledgeLibraryFixture.id, path: "", cursor: null, depth: 1 });
  expect(service.read).not.toHaveBeenCalled();
  await act(async () => host.querySelector<HTMLButtonElement>('[aria-expanded="false"]')!.click());
  expect(service.list).toHaveBeenLastCalledWith({ libraryId: knowledgeLibraryFixture.id, path: "notes", cursor: null, depth: 1 });
  // The stable title identifies the document row independently of tree position.
  // 稳定标题独立于树位置标识文档行。
  const documentButton = [...host.querySelectorAll("button")].find((entry) => entry.textContent?.includes(knowledgeDocumentFixture.title))!;
  await act(async () => documentButton.click());
  expect(service.read).toHaveBeenCalledExactlyOnceWith({ libraryId: knowledgeLibraryFixture.id, documentId: knowledgeDocumentFixture.id, startLine: 1 });
  // Only the body reader's next control is inside the labeled reader region.
  // 带标签的阅读区域内仅有正文阅读器的下一页控件。
  const reader = host.querySelector('section[aria-label="knowledge.view"]')!;
  const next = [...reader.querySelectorAll<HTMLButtonElement>("button")].find((entry) => entry.textContent === "knowledge.page.next")!;
  await act(async () => next.click());
  expect(service.read).toHaveBeenLastCalledWith({ libraryId: knowledgeLibraryFixture.id, documentId: knowledgeDocumentFixture.id, startLine: 7 });
  expect(reader.textContent).toContain("Body 7");
  expect(document.querySelector('textarea, [contenteditable="true"]')).toBeNull();
  expect(service.search).not.toHaveBeenCalled();
});

it("revokes a paid search result on mode changes without automatically issuing another query", async () => {
  service.search.mockResolvedValue({ queries: ["text"], hits: [], libraries: [], failures: [] });
  await act(async () => root.render(<KnowledgeContent library={knowledgeLibraryFixture} />));
  // Native React input dispatch follows the actual controlled-field contract.
  // 原生 React 输入派发遵循真实受控字段契约。
  const input = host.querySelector<HTMLInputElement>('input[aria-label="knowledge.content.search"]')!;
  await act(async () => { Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(input, "text"); input.dispatchEvent(new Event("input", { bubbles: true })); });
  await act(async () => host.querySelector("form")!.dispatchEvent(new Event("submit", { bubbles: true, cancelable: true })));
  expect(service.search).toHaveBeenCalledExactlyOnceWith({ libraryId: knowledgeLibraryFixture.id, query: "text" });
  await act(async () => root.render(<KnowledgeContent library={{ ...knowledgeLibraryFixture, settings: { indexMode: "browse", embedding: null } }} />));
  expect(service.search).toHaveBeenCalledTimes(1);
  expect(host.querySelector("form")).toBeNull();
});
