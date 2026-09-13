// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { I18nProvider } from "@/i18n";
import * as knowledgeService from "@/service/knowledge-base-service";
import { KnowledgeBaseArea } from "./KnowledgeBaseArea";

import { findKnowledgePackageNameConflict, KnowledgePackageImportDialog } from "./KnowledgePackageDialogs";
import type { KnowledgeLibrarySummary, KnowledgePackageInspection } from "@/types";

// Only synthetic React trees participate in these destructive-confirmation tests.
// 这些删除确认测试仅使用合成 React 树。
const roots: Root[] = [];
Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
afterEach(async () => {
  await act(async () => roots.splice(0).forEach((root) => root.unmount()));
  document.body.replaceChildren();
  vi.restoreAllMocks();
});

// mountImport uses a fully typed detached package and existing library, without filesystem calls.
// mountImport 使用完整类型的独立知识库包与现有库，不发起文件系统调用。
async function mountImport(onImport: (name: string, strategy: "create" | "replace", target: string | null) => Promise<void>, onOpenChange = vi.fn()) {
  const library = libraryFixture("existing-id", "Alpha");
  const inspection: KnowledgePackageInspection = { archivePath: "X:/synthetic/alpha.vkb", package: {
    format: "vulcan-knowledge-package", packageVersion: 1, sourceLibraryId: "source-id", name: "Alpha", description: "",
    schemaVersion: 1, documentCount: 2, chunkCount: 3, indexStatus: "ready", exportedAtMs: 1, contentSha256: "0".repeat(64), contentBytes: 100,
  } };
  const container = document.createElement("div");
  const root = createRoot(container);
  roots.push(root); document.body.append(container);
  await act(async () => root.render(createElement(I18nProvider, { initialLocale: "en-US",
    children: createElement(KnowledgePackageImportDialog, { inspection, libraries: [library], busy: false, onImport, onOpenChange }),
  })));
  return { root, inspection, library, onOpenChange };
}

// openReplacement selects the explicit strategy and opens the shared second confirmation.
// openReplacement 选择明确策略并打开共享第二次确认。
async function openReplacement(): Promise<void> {
  // Native radio inputs are controlled by checked/name, so locate the replacement label directly.
  // 原生单选由 checked/name 控制，因此直接定位覆盖策略标签。
  const radio = document.querySelector<HTMLInputElement>('label.danger input[type="radio"]')!;
  await act(async () => radio.click());
  await act(async () => Array.from(document.querySelectorAll("button")).find((button) => button.textContent === "Replace existing library")!.click());
}

// libraryFixture builds one minimal authoritative library summary for conflict tests.
// libraryFixture 为冲突测试构建一个最小权威知识库摘要。
function libraryFixture(id: string, name: string): KnowledgeLibrarySummary {
  return {
    id,
    name,
    description: "",
    settings: {
      retrievalMode: "raw_bm25",
      fallbackPolicy: "explicit_only",
      curation: { enabled: false, promptVersion: "knowledge-display-v1" },
      displayIndex: { status: "not_created", generatedDocumentCount: 0 },
      embedding: {
        enabled: false,
        embeddingBatchSize: 10,
        batchSizeSource: "fallback_default",
      },
      vectorIndex: { status: "disabled" },
    },
    lifecycleState: "active",
    documentCount: 0,
    chunkCount: 0,
    indexStatus: "ready",
    createdAtMs: 1,
    updatedAtMs: 1,
  };
}

describe("findKnowledgePackageNameConflict", () => {
  it("matches the backend trim and non-locale lowercase contract", () => {
    const existing = libraryFixture("library-1", "项目资料");
    expect(findKnowledgePackageNameConflict([existing], "  项目资料  ")).toBe(existing);
  });

  it("returns the exact conflicting target and leaves unique names available", () => {
    const first = libraryFixture("library-1", "Alpha");
    const second = libraryFixture("library-2", "Beta");
    expect(findKnowledgePackageNameConflict([first, second], "bEtA")).toBe(second);
    expect(findKnowledgePackageNameConflict([first, second], "Gamma")).toBeNull();
    expect(findKnowledgePackageNameConflict([first, second], "   ")).toBeNull();
  });
});

describe("package replacement confirmation", () => {
  it("requires the second confirmation and preserves the exact target and draft after failure", async () => {
    let reject!: (reason: Error) => void;
    const onImport = vi.fn(() => new Promise<void>((_, rejectRequest) => { reject = rejectRequest; }));
    const onOpenChange = vi.fn();
    await mountImport(onImport, onOpenChange);
    await openReplacement();
    expect(onImport).not.toHaveBeenCalled();
    const confirmation = document.querySelector('[role="alertdialog"]')!;
    expect(confirmation.textContent).toContain("Alpha");
    const confirm = Array.from(confirmation.querySelectorAll("button")).find((button) => button.textContent === "Replace existing library")!;
    await act(async () => { confirm.click(); confirm.click(); });
    expect(onImport).toHaveBeenCalledExactlyOnceWith("Alpha", "replace", "existing-id");
    await act(async () => reject(new Error("synthetic import failure")));
    expect(document.querySelector('[role="alertdialog"]')?.textContent).toContain("synthetic import failure");
    expect(onOpenChange).not.toHaveBeenCalled();
    expect(document.querySelector<HTMLInputElement>('.knowledge-package-name input')?.value).toBe("Alpha");
  });
});

describe("library deletion confirmation", () => {
  it("names the frozen library and leaves failed deletion open without duplicate requests", async () => {
    const library = libraryFixture("delete-id", "Deletion target");
    vi.spyOn(knowledgeService, "listKnowledgeLibraries").mockResolvedValue([library]);
    vi.spyOn(knowledgeService, "listKnowledgeSources").mockResolvedValue([]);
    vi.spyOn(knowledgeService, "getKnowledgeImportJob").mockResolvedValue(null);
    vi.spyOn(knowledgeService, "getKnowledgeIndexIntegrity").mockRejectedValue(new Error("synthetic unavailable inspection"));
    let reject!: (reason: Error) => void;
    const remove = vi.spyOn(knowledgeService, "deleteKnowledgeLibrary").mockImplementation(() => new Promise<void>((_, rejectRequest) => { reject = rejectRequest; }));
    const container = document.createElement("div");
    const root = createRoot(container);
    roots.push(root); document.body.append(container);
    await act(async () => root.render(createElement(I18nProvider, { initialLocale: "en-US", children: createElement(KnowledgeBaseArea) })));
    await act(async () => Array.from(container.querySelectorAll("button")).find((button) => button.textContent === "Delete")!.click());
    const confirmation = document.querySelector('[role="alertdialog"]')!;
    expect(confirmation.textContent).toContain("Deletion target");
    expect(remove).not.toHaveBeenCalled();
    const confirm = Array.from(confirmation.querySelectorAll("button")).find((button) => button.textContent === "Delete")!;
    await act(async () => { confirm.click(); confirm.click(); });
    expect(remove).toHaveBeenCalledExactlyOnceWith("delete-id");
    await act(async () => reject(new Error("synthetic delete failure")));
    expect(document.querySelector('[role="alertdialog"]')?.textContent).toContain("synthetic delete failure");
    expect(container.textContent).toContain("Deletion target");
  });
});
