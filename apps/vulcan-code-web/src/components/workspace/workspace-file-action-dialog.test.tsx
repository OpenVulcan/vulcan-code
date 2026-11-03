// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import WorkspaceFileActionDialog from "./workspace-file-action-dialog";
import type { WorkspaceFileAction } from "./workspace-file-actions";

vi.mock("@/i18n", () => ({ useI18n: () => ({ t: (key: string) => key }) }));
const toast = vi.hoisted(() => ({ info: vi.fn(), error: vi.fn() }));
vi.mock("@/ui-kit/primitives/toast", () => ({ toast }));
const outsideService = vi.hoisted(() => ({
  loadSessionOutsideFileAccess: vi.fn(),
  loadWorkspaceFileKind: vi.fn(),
  saveSessionOutsideFileAccess: vi.fn(async () => undefined),
}));
vi.mock("@/service/outside-file-access-service", () => outsideService);
const fileWindows = vi.hoisted(() => ({ openFileWindow: vi.fn() }));
vi.mock("@/components/file-windows/file-window-store", () => fileWindows);

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
  vi.clearAllMocks();
});

// stubBridge installs a desktop local file bridge that reports `inspection` for every path.
// stubBridge 安装一个对所有路径都报告 `inspection` 的桌面本地文件桥。
function stubBridge(inspection: { exists: boolean; opensWithDefaultApp: boolean }) {
  const bridge = {
    inspect: vi.fn(async () => inspection),
    openWithDefaultApp: vi.fn(async () => undefined),
    showInFolder: vi.fn(async () => undefined),
  };
  vi.stubGlobal("vulcanDesktop", { localFile: bridge });
  return bridge;
}

// runAction mounts the dialog for one request and returns its completion callback.
// runAction 为单个请求挂载对话框，并返回其完成回调。
async function runAction(action: WorkspaceFileAction, path = "D:\\files\\target.bin") {
  const onDone = vi.fn();
  await act(async () => root.render(
    <WorkspaceFileActionDialog onDone={onDone} request={{ id: 1, path, action, sessionId: "session-1" }} />,
  ));
  return onDone;
}

// outsideAccess makes the session report `access` and the service classify every non-viewer file as `kind`.
// outsideAccess 使会话报告 `access`，并使服务将所有非查看器文件分类为 `kind`。
function outsideAccess(access: "deny" | "prompt" | "allow", kind: "code" | "other" = "code") {
  outsideService.loadSessionOutsideFileAccess.mockResolvedValue({ access, session: null, system: access });
  outsideService.loadWorkspaceFileKind.mockResolvedValue(kind);
}

// button finds a dialog button by its label key.
// button 按文案键查找对话框按钮。
function button(label: string): HTMLButtonElement | undefined {
  return [...document.querySelectorAll<HTMLButtonElement>("button")].find((entry) => entry.textContent === label);
}

it("opens an allowed file at once without asking", async () => {
  const bridge = stubBridge({ exists: true, opensWithDefaultApp: true });
  const onDone = await runAction("open");
  expect(bridge.openWithDefaultApp).toHaveBeenCalledExactlyOnceWith("D:\\files\\target.bin");
  expect(onDone).toHaveBeenCalledOnce();
  expect(button("fileActions.openWithDefaultApp")).toBeUndefined();
});

it("asks before opening, offering the default program and the folder", async () => {
  const bridge = stubBridge({ exists: true, opensWithDefaultApp: true });
  const onDone = await runAction("prompt");
  expect(bridge.openWithDefaultApp).not.toHaveBeenCalled();
  await act(async () => button("fileActions.openWithDefaultApp")!.click());
  expect(bridge.openWithDefaultApp).toHaveBeenCalledExactlyOnceWith("D:\\files\\target.bin");
  expect(onDone).toHaveBeenCalledOnce();
});

it("offers only the folder for a runnable file, even when asked to open it", async () => {
  const bridge = stubBridge({ exists: true, opensWithDefaultApp: false });
  const onDone = await runAction("open");
  expect(button("fileActions.openWithDefaultApp")).toBeUndefined();
  await act(async () => button("fileActions.showInFolder")!.click());
  expect(bridge.openWithDefaultApp).not.toHaveBeenCalled();
  expect(bridge.showInFolder).toHaveBeenCalledExactlyOnceWith("D:\\files\\target.bin");
  expect(onDone).toHaveBeenCalledOnce();
});

it("reports a missing file without a dialog", async () => {
  const bridge = stubBridge({ exists: false, opensWithDefaultApp: false });
  const onDone = await runAction("reveal");
  expect(bridge.showInFolder).not.toHaveBeenCalled();
  expect(toast.info).toHaveBeenCalledWith("workspaceFiles.openPathMissing");
  expect(onDone).toHaveBeenCalledOnce();
});

it("keeps an outside file closed when the session denies outside files", async () => {
  outsideAccess("deny");
  const onDone = await runAction("outside", "D:\\files\\report.pdf");
  expect(toast.info).toHaveBeenCalledWith("outsideFile.denied", expect.objectContaining({ description: "D:\\files\\report.pdf" }));
  expect(fileWindows.openFileWindow).not.toHaveBeenCalled();
  expect(onDone).toHaveBeenCalledOnce();
});

it("opens an outside file in a window at once when the session allows outside files", async () => {
  outsideAccess("allow");
  const onDone = await runAction("outside", "D:\\files\\notes.txt");
  expect(fileWindows.openFileWindow).toHaveBeenCalledExactlyOnceWith("session-1", "D:\\files\\notes.txt");
  expect(button("fileActions.open")).toBeUndefined();
  expect(onDone).toHaveBeenCalledOnce();
});

it("asks before opening an outside file and can stop asking for the session", async () => {
  stubBridge({ exists: true, opensWithDefaultApp: true });
  outsideAccess("prompt");
  const onDone = await runAction("outside", "D:\\files\\report.pdf");
  expect(fileWindows.openFileWindow).not.toHaveBeenCalled();
  expect(button("fileActions.openWithDefaultApp")).toBeDefined();
  await act(async () => document.querySelector<HTMLButtonElement>("[role=checkbox]")!.click());
  await act(async () => button("fileActions.open")!.click());
  expect(outsideService.saveSessionOutsideFileAccess).toHaveBeenCalledExactlyOnceWith("session-1", "allow");
  expect(fileWindows.openFileWindow).toHaveBeenCalledExactlyOnceWith("session-1", "D:\\files\\report.pdf");
  expect(onDone).toHaveBeenCalledOnce();
});

it("hands an outside file no viewer reads to the default program prompt", async () => {
  const bridge = stubBridge({ exists: true, opensWithDefaultApp: true });
  outsideAccess("allow", "other");
  const onDone = await runAction("outside");
  expect(fileWindows.openFileWindow).not.toHaveBeenCalled();
  await act(async () => button("fileActions.openWithDefaultApp")!.click());
  expect(bridge.openWithDefaultApp).toHaveBeenCalledExactlyOnceWith("D:\\files\\target.bin");
  expect(onDone).toHaveBeenCalledOnce();
});

it("tells the web client the file cannot be shown and offers its path", async () => {
  const onDone = await runAction("prompt");
  expect(toast.info).toHaveBeenCalledWith("fileActions.unsupportedTitle", expect.objectContaining({ description: "D:\\files\\target.bin" }));
  expect(onDone).toHaveBeenCalledOnce();
});
