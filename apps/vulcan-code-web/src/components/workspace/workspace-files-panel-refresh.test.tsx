// @vitest-environment jsdom
import React, { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import type { WorkspaceFileDirectoryListing, WorkspaceFileEntrySummary, WorkspaceFileReadResponse } from "@/types";
import { useWorkspaceFilesRequestOwnership } from "./workspace-files-panel-request-ownership";
import { useWorkspaceFilesRefresh, type WorkspaceFilesOpenFile } from "./workspace-files-panel-refresh";

let host: HTMLDivElement;
let root: Root;

beforeEach(() => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
});

afterEach(async () => {
  await act(async () => root.unmount());
  host.remove();
  vi.unstubAllGlobals();
});

const entry = (path: string, isDirectory: boolean) => ({ path, name: path.split("/").pop(), isDirectory }) as WorkspaceFileEntrySummary;
const listing = (path: string, entries: WorkspaceFileEntrySummary[]) => ({ path, entries }) as WorkspaceFileDirectoryListing;
const preview = (content: string) => ({ path: "/w/a.ts", content, truncated: false, sizeBytes: content.length }) as WorkspaceFileReadResponse;

// Stage holds directory state the way the panel does and loads listings from a mutable fake disk.
// Stage 以面板的方式保存目录状态，并从可变的模拟磁盘加载目录列表。
function Stage(props: {
  disk: Map<string, WorkspaceFileDirectoryListing>;
  openFile?: Pick<WorkspaceFilesOpenFile, "entry" | "preview" | "load" | "setPreview">;
  initial: WorkspaceFileDirectoryListing[];
  onState: (directories: Map<string, WorkspaceFileDirectoryListing>, refresh: () => void, refreshing: boolean) => void;
}) {
  const [directories, setDirectories] = React.useState(() => new Map(props.initial.map((item) => [item.path, item])));
  const loadDirectory = React.useCallback(async (directoryPath: string | null) => {
    // Like the panel, a failed load reports itself and resolves without a listing.
    // 与面板一致，加载失败时自行报告并不带列表地完成。
    const loaded = props.disk.get(directoryPath ?? "/w");
    if (!loaded) return undefined;
    setDirectories((current) => new Map(current).set(loaded.path, loaded));
    return loaded;
  }, [props.disk]);
  const ownership = useWorkspaceFilesRequestOwnership("scope");
  const openFile = { entry: null, preview: null, load: async () => preview(""), setPreview: () => {}, ...props.openFile, ownership };
  const { refresh, refreshing } = useWorkspaceFilesRefresh({
    workspacePath: "/w", projectId: null, enabled: true, directories, loadDirectory, setDirectories, openFile,
  });
  props.onState(directories, refresh, refreshing);
  return null;
}

it("reloads every loaded directory still reachable and drops the ones that disappeared", async () => {
  const disk = new Map([
    ["/w", listing("/w", [entry("/w/src", true), entry("/w/new.txt", false)])],
    ["/w/src", listing("/w/src", [entry("/w/src/a.ts", false), entry("/w/src/b.ts", false)])],
  ]);
  const initial = [
    listing("/w", [entry("/w/src", true), entry("/w/old", true)]),
    listing("/w/src", [entry("/w/src/a.ts", false)]),
    listing("/w/old", [entry("/w/old/x.ts", false)]),
  ];
  let state = new Map<string, WorkspaceFileDirectoryListing>();
  let refresh = () => {};
  await act(async () => root.render(<Stage disk={disk} initial={initial} onState={(next, run) => { state = next; refresh = run; }} />));

  await act(async () => refresh());

  expect([...state.keys()].sort()).toEqual(["/w", "/w/src"]);
  expect(state.get("/w")?.entries.map((item) => item.path)).toEqual(["/w/src", "/w/new.txt"]);
  expect(state.get("/w/src")?.entries).toHaveLength(2);
});

it("keeps every listing when a directory fails to reload", async () => {
  const disk = new Map<string, WorkspaceFileDirectoryListing>();
  const initial = [listing("/w", [entry("/w/src", true)]), listing("/w/src", [])];
  let state = new Map<string, WorkspaceFileDirectoryListing>();
  let refresh = () => {};
  let refreshing = false;
  await act(async () => root.render(<Stage disk={disk} initial={initial} onState={(next, run, busy) => { state = next; refresh = run; refreshing = busy; }} />));

  await act(async () => refresh());

  expect([...state.keys()].sort()).toEqual(["/w", "/w/src"]);
  expect(refreshing).toBe(false);
});

it("rereads the open file and replaces its preview only when the text changed", async () => {
  const disk = new Map([["/w", listing("/w", [entry("/w/a.ts", false)])]]);
  const initial = [listing("/w", [entry("/w/a.ts", false)])];
  const setPreview = vi.fn();
  let text = "same";
  let refresh = () => {};
  const openFile = { entry: entry("/w/a.ts", false), preview: preview("same"), load: async () => preview(text), setPreview };
  await act(async () => root.render(<Stage disk={disk} initial={initial} openFile={openFile} onState={(_, run) => { refresh = run; }} />));

  await act(async () => refresh());
  expect(setPreview).not.toHaveBeenCalled();

  text = "changed";
  await act(async () => refresh());
  expect(setPreview).toHaveBeenCalledWith(expect.objectContaining({ content: "changed" }));
});
