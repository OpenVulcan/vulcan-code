// @vitest-environment jsdom
import React, { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import * as service from "@/service/markdown-memory-service";
import type { MarkdownMemoryImportPreview, MarkdownMemoryOverview, PackageFileListing } from "@/types";
import { MemoryTransferSection } from "./markdown-memory-transfer";

// Catalog keys stand in for wording so assertions never depend on it.
// 以目录键代替文案，断言从不依赖措辞。
vi.mock("@/i18n", () => ({
  useI18n: () => ({ t: (key: string) => key, locale: "en-US" }),
  formatMessage: (text: string) => text,
}));

// A synthetic directory holding one package; nothing touches a real filesystem.
// 含一个导出包的合成目录；不触及真实文件系统。
const listing: PackageFileListing = {
  directory: { path: "X:/synthetic", parentPath: null, canSelect: true, entries: [], roots: [] },
  packages: [{ name: "memory.zip", path: "X:/synthetic/memory.zip", byteSize: 64 }],
};

// One packaged project and a space conflict whose imported side is newer.
// 一个包内项目，以及导入侧较新的空间层冲突。
function preview(withProject: boolean): MarkdownMemoryImportPreview {
  return {
    sha256: "c".repeat(64),
    exportedAt: "2026-10-20T08:00Z",
    projects: withProject ? [
      { project: { id: "p-a", name: "Alpha", paths: ["X:/alpha"] }, candidates: [{ memoryProjectId: "p-local", name: "Alpha", reasons: ["same_path"] }] },
      { project: { id: "p-b", name: "Beta", paths: ["X:/beta"] }, candidates: [] },
    ] : [],
    scopes: [{
      scope: "space", packageProjectId: null, memoryProjectId: null, same: 1, added: [], duplicates: [], newSections: [],
      conflicts: [{
        id: "n1", path: ["Rules", "Shell"],
        local: { title: "Shell", body: "old", updated: "2026-10-01T00:00Z" },
        imported: { title: "Shell", body: "new", updated: "2026-10-02T00:00Z" },
        newer: "imported",
      }],
    }],
  };
}

let root: Root;
let host: HTMLDivElement;
beforeEach(() => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
  vi.spyOn(service, "browseMarkdownMemoryPackages").mockResolvedValue(listing);
  vi.spyOn(service, "loadMarkdownMemory").mockResolvedValue({ projects: [] } as unknown as MarkdownMemoryOverview);
});
afterEach(async () => {
  await act(async () => root.unmount());
  host.remove();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

// Finds a listbox option by its visible name; fails naming the missing label.
function option(label: string): HTMLElement {
  const found = [...document.querySelectorAll<HTMLElement>("[role=\"option\"]")].find((entry) => entry.textContent?.startsWith(label));
  if (found === undefined) throw new Error("Missing option: " + label);
  return found;
}

// Finds a button by its catalog key; fails naming the missing label.
// 按目录键或文件名查找按钮；缺失时报出该标签。
function button(label: string): HTMLButtonElement {
  const found = [...document.querySelectorAll("button")].find((entry) => entry.textContent?.startsWith(label));
  if (found === undefined) throw new Error("Missing control: " + label);
  return found;
}

// Opens the import picker, picks the package and waits for the import dialog.
// 打开导入选择器、选取导出包并等待导入对话框。
async function openImport(): Promise<void> {
  await act(async () => root.render(<MemoryTransferSection onImported={() => undefined} />));
  await act(async () => button("memory.markdown.transfer.import").click());
  await act(async () => option("memory.zip").click());
  await act(async () => button("memory.markdown.transfer.preview").click());
}

describe("Markdown memory import", () => {
  it("defaults each conflict to its newer side and sends the reviewed resolutions", async () => {
    vi.spyOn(service, "previewMarkdownMemoryImport").mockResolvedValue(preview(false));
    const apply = vi.spyOn(service, "importMarkdownMemory")
      .mockResolvedValue({ backupDirectory: "X:/backup", added: 0, replaced: 1, unchanged: 1, createdProjects: [] });
    await openImport();
    expect(document.querySelector("[data-memory-import-review]")).not.toBeNull();
    await act(async () => button("memory.markdown.transfer.apply").click());
    expect(apply).toHaveBeenCalledWith({
      archivePath: "X:/synthetic/memory.zip",
      expectedSha256: "c".repeat(64),
      projects: [],
      resolutions: [{ scope: "space", nodeId: "n1", choice: "take_imported" }],
    });
    expect(document.querySelector("[data-memory-import-report]")).not.toBeNull();
  });

  it("starts every packaged project unset and previews only once all have a target", async () => {
    const previewSpy = vi.spyOn(service, "previewMarkdownMemoryImport")
      .mockImplementation(async (_path, projects) => preview(projects.length === 0));
    await openImport();
    expect(document.querySelector("[data-memory-import-projects]")).not.toBeNull();
    expect(button("memory.markdown.transfer.next").disabled).toBe(true);
    await act(async () => button("memory.markdown.transfer.createRest").click());
    await act(async () => button("memory.markdown.transfer.next").click());
    expect(previewSpy).toHaveBeenLastCalledWith("X:/synthetic/memory.zip", [
      { packageProjectId: "p-a", target: { action: "create" } },
      { packageProjectId: "p-b", target: { action: "create" } },
    ]);
    expect(document.querySelector("[data-memory-import-review]")).not.toBeNull();
  });
});
