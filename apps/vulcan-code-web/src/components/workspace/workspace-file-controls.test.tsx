// @vitest-environment jsdom
import React, { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { WorkspaceFileTree, type WorkspaceFileTreeNode } from "./workspace-file-tree";
import { WorkspaceFilePreview } from "./workspace-files-preview";
import { useWorkspaceFilesRequestOwnership, type WorkspaceFilesRequestOwnership } from "./workspace-files-panel-request-ownership";
vi.mock("@/i18n", () => ({ useI18n: () => ({ t: (key: string) => key }) }));
vi.mock("@/components/conversation/conversation-markdown", () => ({ MarkdownMessage: () => null,
  MarkdownImageWorkspacePathProvider: ({ children }: { children: React.ReactNode }) => children }));
vi.mock("@/diff/diff-highlight", () => ({ diffLanguageForFile: () => null,
  useHighlightedDiffFileRevision: () => 0, useHighlightLanguage: () => false }));

// tree has nested directories whose initial expansion is explicitly empty.
// tree 包含初始明确折叠的多级目录。
const tree: WorkspaceFileTreeNode<string> = { path: "root", name: "root", children: [
  { path: "root/a", name: "a", children: [
    { path: "root/a/b", name: "b", children: [{ path: "root/a/b/file", name: "file", file: "file", children: [] }] },
  ] },
] };
describe("workspace file controls", () => {
  let root: Root;
  let host: HTMLDivElement;
  beforeEach(() => {
    Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
    host = document.createElement("div"); document.body.append(host); root = createRoot(host);
  });
  afterEach(async () => { await act(async () => root.unmount()); host.remove(); });
  it("expands every actual directory even when the initial open set is empty", async () => {
    const onOpen = vi.fn();
    const defaultOpen = new Set<string>();
    await act(async () => root.render(<WorkspaceFileTree tree={tree} activeFileKey={null} filter=""
      defaultOpenDirectoryPaths={defaultOpen} onFilterChange={vi.fn()} onSelectFile={vi.fn()}
      fileKey={(file) => file} onDirectoryOpenStateChange={onOpen} />));
    expect(host.querySelector('.vk-ftree-row[data-kind="file"]')).toBeNull();
    await act(async () => host.querySelector<HTMLButtonElement>('button[aria-label="workspaceFileTree.expandAll"]')!.click());
    expect(host.querySelector('.vk-ftree-row[data-kind="file"]')?.textContent).toContain("file");
    expect(onOpen.mock.calls).toEqual([["root/a", true], ["root/a/b", true]]);
    expect(host.querySelectorAll('.vk-ftree-row[data-kind="dir"][aria-expanded="true"]')).toHaveLength(2);
  });
  it("marks the open file and hands a chosen file to the owner", async () => {
    const onSelect = vi.fn();
    await act(async () => root.render(<WorkspaceFileTree tree={tree} activeFileKey="file" filter=""
      onFilterChange={vi.fn()} onSelectFile={onSelect} fileKey={(file) => file} />));
    const row = host.querySelector<HTMLButtonElement>('.vk-ftree-row[data-kind="file"]')!;
    expect(row.getAttribute("aria-current")).toBe("true");
    await act(async () => row.click());
    expect(onSelect).toHaveBeenCalledExactlyOnceWith("file");
  });
  it("revokes directory and preview response ownership after unmount", async () => {
    let ownership!: WorkspaceFilesRequestOwnership;
    // Owner exposes the real hook while never making a filesystem request.
    // Owner 暴露真实 Hook，但不发起文件系统请求。
    function Owner() { ownership = useWorkspaceFilesRequestOwnership("synthetic"); return null; }
    await act(async () => root.render(<Owner />));
    const directory = ownership.beginDirectoryRequest("root")!;
    const preview = ownership.beginPreviewRequest()!;
    await act(async () => root.render(null));
    expect(ownership.ownsDirectoryRequest("root", directory)).toBe(false);
    expect(ownership.ownsPreviewRequest(preview)).toBe(false);
  });
  it("keeps the failed file visible and retries only when explicitly requested", async () => {
    const retry = vi.fn();
    await act(async () => root.render(<WorkspaceFilePreview entry={{ name: "file.txt", path: "/synthetic/file.txt",
      relativePath: "file.txt", kind: "code", isDirectory: false, hidden: false, hasChildren: false }}
      missingPath={null} preview={null} mediaPermissionEntry={null} loading={false} error="synthetic preview failure"
      resolveMediaSourceUrl={(path) => path} onAllowMediaPreview={vi.fn()} onRetryPreview={retry} />));
    expect(host.querySelector('[role="alert"]')?.textContent).toContain("synthetic preview failure");
    expect(retry).not.toHaveBeenCalled();
    await act(async () => host.querySelector<HTMLButtonElement>("button")!.click());
    expect(retry).toHaveBeenCalledOnce();
  });
});
