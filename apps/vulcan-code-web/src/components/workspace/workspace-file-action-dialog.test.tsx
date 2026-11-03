// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import WorkspaceFileActionDialog from "./workspace-file-action-dialog";
import type { WorkspaceFileAction } from "./workspace-file-actions";

vi.mock("@/i18n", () => ({ useI18n: () => ({ t: (key: string) => key }) }));
const toast = vi.hoisted(() => ({ info: vi.fn(), error: vi.fn() }));
vi.mock("@/ui-kit/primitives/toast", () => ({ toast }));

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
async function runAction(action: WorkspaceFileAction) {
  const onDone = vi.fn();
  await act(async () => root.render(
    <WorkspaceFileActionDialog onDone={onDone} request={{ id: 1, path: "D:\\files\\target.bin", action }} />,
  ));
  return onDone;
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

it("tells the web client the file cannot be shown and offers its path", async () => {
  const onDone = await runAction("prompt");
  expect(toast.info).toHaveBeenCalledWith("fileActions.unsupportedTitle", expect.objectContaining({ description: "D:\\files\\target.bin" }));
  expect(onDone).toHaveBeenCalledOnce();
});
