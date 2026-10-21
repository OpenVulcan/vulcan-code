import { describe, expect, it } from "vitest";
import { findRequestedFileEntry, resolveWorkspaceFileTarget } from "./workspace-file-open-request";
import { emptyWorkspaceContextTabState, focusWorkspaceFilesTab, openWorkspaceContextTab } from "./workspace-shell/context-tabs";
import type { WorkspaceFileDirectoryListing, WorkspaceFileEntrySummary } from "@/types";

const t = (key: string) => key;

function entry(name: string, isDirectory = false): WorkspaceFileEntrySummary {
  return { name, path: `/repo/${name}`, relativePath: name, kind: isDirectory ? "directory" : "code", isDirectory, hidden: false, hasChildren: isDirectory } as WorkspaceFileEntrySummary;
}

describe("resolveWorkspaceFileTarget", () => {
  it("places project-relative paths under the root in its separator style", () => {
    expect(resolveWorkspaceFileTarget("D:\\work\\repo", "src/app/main.ts")).toEqual({ directory: "D:\\work\\repo\\src\\app", name: "main.ts" });
    expect(resolveWorkspaceFileTarget("/home/u/repo/", "./README.md")).toEqual({ directory: null, name: "README.md" });
  });

  it("accepts absolute paths inside the project, comparing drive-letter roots without case", () => {
    expect(resolveWorkspaceFileTarget("D:\\work\\repo", "d:/Work/repo/src/lib.rs")).toEqual({ directory: "D:\\work\\repo\\src", name: "lib.rs" });
    expect(resolveWorkspaceFileTarget("/home/u/repo", "/home/u/repo/a/b.txt")).toEqual({ directory: "/home/u/repo/a", name: "b.txt" });
  });

  it("rejects paths outside the project, climbing paths and the root itself", () => {
    expect(resolveWorkspaceFileTarget("/home/u/repo", "/home/u/repository/x.ts")).toBeNull();
    expect(resolveWorkspaceFileTarget("/home/u/repo", "/Home/u/repo/x.ts")).toBeNull();
    expect(resolveWorkspaceFileTarget("D:\\work\\repo", "src/../../secret.txt")).toBeNull();
    expect(resolveWorkspaceFileTarget("D:\\work\\repo", "D:\\work\\repo")).toBeNull();
  });
});

describe("findRequestedFileEntry", () => {
  it("prefers the exact name, falls back to a case-only difference and skips directories", () => {
    const listing = { entries: [entry("readme.md"), entry("README.md"), entry("Docs", true)] } as WorkspaceFileDirectoryListing;
    expect(findRequestedFileEntry(listing, "README.md")?.name).toBe("README.md");
    expect(findRequestedFileEntry(listing, "Readme.MD")?.name).toBe("readme.md");
    expect(findRequestedFileEntry(listing, "docs")).toBeNull();
  });
});

describe("focusWorkspaceFilesTab", () => {
  it("keeps an active files tab, else activates the first files tab, else opens one", () => {
    const withFiles = openWorkspaceContextTab(emptyWorkspaceContextTabState(), "files", t);
    const filesId = withFiles.activeTabId;
    expect(focusWorkspaceFilesTab(withFiles, t)).toEqual(withFiles);

    const reviewActive = openWorkspaceContextTab(withFiles, "review", t);
    expect(focusWorkspaceFilesTab(reviewActive, t)).toEqual({ ...reviewActive, activeTabId: filesId });

    const opened = focusWorkspaceFilesTab(openWorkspaceContextTab(emptyWorkspaceContextTabState(), "review", t), t);
    expect(opened.tabs.find((tab) => tab.id === opened.activeTabId)?.kind).toBe("files");
  });
});
