// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { I18nProvider } from "@/i18n";
import type { WorkspaceFileDirectoryListing, WorkspaceFileEntrySummary } from "@/types";
import {
  consumeWorkspaceFileLineReveal,
  peekWorkspaceFileLineReveal,
  requestWorkspaceFileOpen,
  useWorkspaceFileOpenRequests,
} from "./workspace-file-open-request";

let host: HTMLDivElement;
let root: Root;
beforeEach(() => {
  host = document.createElement("div"); document.body.append(host); root = createRoot(host);
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); vi.unstubAllGlobals(); });

const file = { name: "a.ts", path: "/repo/src/a.ts", relativePath: "src/a.ts", kind: "code", isDirectory: false, hidden: false, hasChildren: false } as WorkspaceFileEntrySummary;

// Panel stands in for the files panel: it serves tab "files-1" of the /repo project.
// Panel 代替文件面板：服务 /repo 项目的 "files-1" 页签。
function Panel(props: { workspacePath?: string; load: (directory: string | null) => Promise<WorkspaceFileDirectoryListing>; select: (entry: WorkspaceFileEntrySummary) => void }) {
  useWorkspaceFileOpenRequests({ tabId: "files-1", workspacePath: props.workspacePath ?? "/repo", ready: true, load: props.load, select: props.select });
  return null;
}

it("opens a request made before the panel mounted, lists the file's directory and records its lines", async () => {
  const load = vi.fn(async () => ({ entries: [file] }) as WorkspaceFileDirectoryListing);
  const select = vi.fn();
  requestWorkspaceFileOpen("files-1", "/repo/src/a.ts", { start: 4, end: 9 });
  await act(async () => root.render(<I18nProvider initialLocale="en-US"><Panel load={load} select={select} /></I18nProvider>));
  expect(load).toHaveBeenCalledWith("/repo/src");
  expect(select).toHaveBeenCalledWith(file);
  const reveal = peekWorkspaceFileLineReveal(file.path);
  expect(reveal?.lines).toEqual({ start: 4, end: 9 });
  expect(peekWorkspaceFileLineReveal("/repo/src/b.ts")).toBeNull();
  consumeWorkspaceFileLineReveal(reveal!.id);
  expect(peekWorkspaceFileLineReveal(file.path)).toBeNull();
});

it("ignores requests for another tab and leaves them waiting", async () => {
  const load = vi.fn(async () => ({ entries: [file] }) as WorkspaceFileDirectoryListing);
  const select = vi.fn();
  await act(async () => root.render(<I18nProvider initialLocale="en-US"><Panel load={load} select={select} /></I18nProvider>));
  await act(async () => requestWorkspaceFileOpen("files-2", "src/a.ts"));
  expect(load).not.toHaveBeenCalled();
  await act(async () => requestWorkspaceFileOpen("files-1", "src/a.ts"));
  expect(select).toHaveBeenCalledWith(file);
  expect(peekWorkspaceFileLineReveal(file.path)).toBeNull();
});

// Root ownership must survive a route request delivered before React switches the visible tree.
// 打开请求早于 React 切换可见文件树时必须保留根目录归属。
it("waits for the attached root before opening its file and revealing its line range", async () => {
  // The attached repository has its own file with the same name and relative path.
  // 附属仓库拥有同名且相对路径相同的文件。
  const attachedFile = { ...file, path: "/sibling/src/a.ts" };
  const load = vi.fn(async () => ({ entries: [attachedFile] }) as WorkspaceFileDirectoryListing);
  const select = vi.fn();
  await act(async () => root.render(<I18nProvider initialLocale="en-US"><Panel load={load} select={select} /></I18nProvider>));
  await act(async () => requestWorkspaceFileOpen("files-1", attachedFile.path, { start: 3, end: 5 }, "/sibling"));
  expect(load).not.toHaveBeenCalled();
  await act(async () => root.render(<I18nProvider initialLocale="en-US"><Panel workspacePath="/sibling" load={load} select={select} /></I18nProvider>));
  expect(load).toHaveBeenCalledExactlyOnceWith("/sibling/src");
  expect(select).toHaveBeenCalledExactlyOnceWith(attachedFile);
  const reveal = peekWorkspaceFileLineReveal(attachedFile.path);
  expect(reveal?.lines).toEqual({ start: 3, end: 5 });
  if (reveal) consumeWorkspaceFileLineReveal(reveal.id);
});
