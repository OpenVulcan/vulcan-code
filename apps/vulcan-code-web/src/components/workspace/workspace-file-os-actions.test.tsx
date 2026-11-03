// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { WorkspaceFileOsActions } from "./workspace-file-os-actions";

vi.mock("@/i18n", () => ({ useI18n: () => ({ t: (key: string) => key }) }));
vi.mock("@/ui-kit/primitives/toast", () => ({ toast: { error: vi.fn() } }));

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

// stubBridge installs a desktop local file bridge that reports whether files open with their default program.
// stubBridge 安装一个桌面本地文件桥，报告文件能否用默认程序打开。
function stubBridge(opensWithDefaultApp: boolean) {
  const bridge = {
    inspect: vi.fn(async () => ({ exists: true, opensWithDefaultApp })),
    openWithDefaultApp: vi.fn(async () => undefined),
    showInFolder: vi.fn(async () => undefined),
  };
  vi.stubGlobal("vulcanDesktop", { localFile: bridge });
  return bridge;
}

// labels lists the button label keys shown for one file.
// labels 列出为单个文件显示的按钮文案键。
async function labels(path: string): Promise<string[]> {
  await act(async () => root.render(<WorkspaceFileOsActions path={path} />));
  return [...host.querySelectorAll("button")].map((button) => button.textContent ?? "");
}

it("opens a file with its default program or shows it in its folder on the desktop", async () => {
  const bridge = stubBridge(true);
  expect(await labels("D:\\repo\\design.psd")).toEqual(["fileActions.openWithDefaultApp", "fileActions.showInFolder"]);
  await act(async () => host.querySelector("button")!.click());
  expect(bridge.openWithDefaultApp).toHaveBeenCalledExactlyOnceWith("D:\\repo\\design.psd");
});

it("offers only the folder for a runnable file", async () => {
  stubBridge(false);
  expect(await labels("D:\\repo\\setup.exe")).toEqual(["fileActions.showInFolder"]);
});

it("offers to copy the path on the web client", async () => {
  const writeText = vi.fn(async () => undefined);
  vi.stubGlobal("navigator", { clipboard: { writeText } });
  expect(await labels("/home/u/repo/design.psd")).toEqual(["fileActions.copyPath"]);
  await act(async () => host.querySelector("button")!.click());
  expect(writeText).toHaveBeenCalledExactlyOnceWith("/home/u/repo/design.psd");
});
