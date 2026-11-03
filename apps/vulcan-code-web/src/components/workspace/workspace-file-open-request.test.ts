import { describe, expect, it } from "vitest";
import {
  findRequestedFileEntry,
  markdownLinkFileTarget,
  opensAsWebPage,
  parseFilePathMention,
  resolveWorkspaceFileTarget,
  routeWorkspaceFileOpen,
} from "./workspace-file-open-request";
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

describe("routeWorkspaceFileOpen", () => {
  it("opens files a viewer reads in a window by their absolute path, inside the project or not", () => {
    expect(routeWorkspaceFileOpen("/home/u/repo", "/home/u/repo/docs/report.pdf")).toEqual({ kind: "window", path: "/home/u/repo/docs/report.pdf" });
    expect(routeWorkspaceFileOpen("D:\\work\\repo", "docs/chart.png")).toEqual({ kind: "window", path: "D:\\work\\repo\\docs\\chart.png" });
    expect(routeWorkspaceFileOpen("/home/u/repo", " /home/u/Downloads/report.pdf ")).toEqual({ kind: "window", path: "/home/u/Downloads/report.pdf" });
    expect(routeWorkspaceFileOpen(null, "D:/docs/budget.xlsx")).toEqual({ kind: "window", path: "D:/docs/budget.xlsx" });
  });

  it("shows other project files in the files panel and prompts for other files outside it", () => {
    expect(routeWorkspaceFileOpen("/home/u/repo", "/home/u/repo/assets/logo.psd")).toEqual({ kind: "files" });
    expect(routeWorkspaceFileOpen("/home/u/repo", "src/main.ts")).toEqual({ kind: "files" });
    expect(routeWorkspaceFileOpen(null, "docs/report.pdf")).toEqual({ kind: "files" });
    expect(routeWorkspaceFileOpen("/home/u/repo", "/home/u/notes.txt")).toEqual({ kind: "action", path: "/home/u/notes.txt", action: "prompt" });
    expect(routeWorkspaceFileOpen(null, "C:\\tools\\setup.exe")).toEqual({ kind: "action", path: "C:\\tools\\setup.exe", action: "prompt" });
  });

  it("acts on the absolute path for explicit actions, placing project-relative paths in the project", () => {
    expect(routeWorkspaceFileOpen("D:\\work\\repo", "src/app/main.ts", "open")).toEqual({ kind: "action", path: "D:\\work\\repo\\src\\app\\main.ts", action: "open" });
    expect(routeWorkspaceFileOpen("/home/u/repo/", "README.md", "reveal")).toEqual({ kind: "action", path: "/home/u/repo/README.md", action: "reveal" });
    expect(routeWorkspaceFileOpen(null, "/home/u/notes.txt", "open")).toEqual({ kind: "action", path: "/home/u/notes.txt", action: "open" });
    expect(routeWorkspaceFileOpen("/home/u/repo", "docs/report.pdf", "dock")).toEqual({ kind: "viewer", path: "/home/u/repo/docs/report.pdf" });
    expect(routeWorkspaceFileOpen(null, "docs/a.md", "open")).toEqual({ kind: "files" });
  });
});

describe("markdownLinkFileTarget", () => {
  it("reads project files with their lines from paths, line suffixes, fragments and file URLs", () => {
    expect(markdownLinkFileTarget("crates/a/src/lib.rs")).toEqual({ path: "crates/a/src/lib.rs", lines: null });
    expect(markdownLinkFileTarget("src/app.ts:12-20")).toEqual({ path: "src/app.ts", lines: { start: 12, end: 20 } });
    expect(markdownLinkFileTarget("src/app.ts#L12-L20")).toEqual({ path: "src/app.ts", lines: { start: 12, end: 20 } });
    expect(markdownLinkFileTarget("make.ps1:3-5")).toEqual({ path: "make.ps1", lines: { start: 3, end: 5 } });
    expect(markdownLinkFileTarget("./make.ps1:3")).toEqual({ path: "make.ps1", lines: { start: 3, end: 3 } });
    expect(markdownLinkFileTarget("src/my%20notes.md")).toBeNull();
    expect(markdownLinkFileTarget("file:///D:/repo/a.ts")).toEqual({ path: "D:/repo/a.ts", lines: null });
  });

  it("leaves web links, in-page anchors and folders as ordinary links", () => {
    expect(markdownLinkFileTarget("https://example.com/a.ts")).toBeNull();
    expect(markdownLinkFileTarget("mailto:a@example.com")).toBeNull();
    expect(markdownLinkFileTarget("#usage")).toBeNull();
    expect(markdownLinkFileTarget("//example.com/a.ts")).toBeNull();
    expect(markdownLinkFileTarget("src/components/")).toBeNull();
  });
});

describe("parseFilePathMention", () => {
  it("reads folder paths with an extension and an optional line or line range", () => {
    expect(parseFilePathMention("src/app.ts")).toEqual({ path: "src/app.ts", lines: null });
    expect(parseFilePathMention("src/app.ts:12")).toEqual({ path: "src/app.ts", lines: { start: 12, end: 12 } });
    expect(parseFilePathMention("crates/a/lib.rs:4-9")).toEqual({ path: "crates/a/lib.rs", lines: { start: 4, end: 9 } });
    expect(parseFilePathMention("D:\\repo\\main.rs:3")).toEqual({ path: "D:\\repo\\main.rs", lines: { start: 3, end: 3 } });
    expect(parseFilePathMention("src/app.ts:9-4")).toEqual({ path: "src/app.ts", lines: null });
  });

  it("leaves non-path code alone: bare names, members, URLs, commands, globs and fenced text", () => {
    for (const text of ["README.md", "obj.method", "https://x.dev/a.js", "npm run build", "src/*.ts", "src/app", "v1.2/3.4", "src/app.ts\n"]) {
      expect(parseFilePathMention(text)).toBeNull();
    }
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

describe("opensAsWebPage", () => {
  it("picks web pages, with or without lines or fragments, and leaves other files alone", () => {
    expect(["out/a.html", "a.HTM:3-5", "D:/x/b.xhtml#top", "c.html:4:2"].every(opensAsWebPage)).toBe(true);
    expect(["notes.txt", "src/app.ts:12", "page.html.md", "html/readme"].some(opensAsWebPage)).toBe(false);
  });
});
