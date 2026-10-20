// @vitest-environment jsdom
import React, { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import type { ComposerExternalCommand } from "@/components/composer/composer-external-commands";
import type { ElectronLocalBrowserBridge, ElectronLocalBrowserElementPick } from "@/host-shell/electron-bridge";
import { useBrowserElementPick, type BrowserElementPickState } from "./use-browser-element-pick";

// t echoes a template whose placeholders are all visible, so the inserted text exposes every value.
// t 返回所有占位符均可见的模板，使插入文本暴露每个取值。
const t = () => "{url}|{selector}|{tag}|{text}|{viewportWidth}x{viewportHeight}|{x},{y},{width}x{height}";

// PICK is one element as the native picker reports it.
// PICK 是原生选择器报告的一个元素。
const PICK: ElectronLocalBrowserElementPick = {
  url: "https://example.com/",
  title: "Example",
  selector: "main > button:nth-of-type(2)",
  tag: "button",
  text: "Sign in",
  rect: { x: 10.4, y: 20.6, width: 99.5, height: 30 },
  viewport: { width: 1280, height: 800 },
  screenshot: new Uint8Array([137, 80, 78, 71]),
};

let host: HTMLDivElement;
let root: Root;
let state: BrowserElementPickState;
let commands: ComposerExternalCommand[];
const record = (event: Event) => commands.push((event as CustomEvent<ComposerExternalCommand>).detail);

beforeEach(() => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
  commands = [];
  window.addEventListener("vulcan:composer-command", record);
});

afterEach(async () => {
  await act(async () => root.unmount());
  window.removeEventListener("vulcan:composer-command", record);
  host.remove();
  vi.unstubAllGlobals();
});

// Probe exposes the hook's state for one bridge and tab.
// Probe 为一个桥与标签暴露该 hook 的状态。
function Probe(props: { bridge: ElectronLocalBrowserBridge; tabId: string; onError: (message: string) => void }) {
  state = useBrowserElementPick(props.bridge, props.tabId, t, props.onError);
  return null;
}

// fakeBridge resolves picks through `resolvePick` so a test controls when the user clicks.
// fakeBridge 通过 `resolvePick` 完成选择，由测试控制用户何时点击。
function fakeBridge() {
  let resolvePick: (value: ElectronLocalBrowserElementPick | null) => void = () => {};
  const bridge = {
    pickElement: vi.fn(() => new Promise<ElectronLocalBrowserElementPick | null>((resolve) => { resolvePick = resolve; })),
    cancelElementPick: vi.fn(async () => resolvePick(null)),
  } as unknown as ElectronLocalBrowserBridge;
  return { bridge, resolve: (value: ElectronLocalBrowserElementPick | null) => resolvePick(value) };
}

it("hands the picked element and its annotated screenshot to the composer", async () => {
  const { bridge, resolve } = fakeBridge();
  await act(async () => root.render(<Probe bridge={bridge} tabId="tab-1" onError={vi.fn()} />));
  expect(state.available).toBe(true);

  await act(async () => state.toggle());
  expect(state.picking).toBe(true);
  await act(async () => resolve(PICK));

  expect(state.picking).toBe(false);
  expect(commands).toHaveLength(1);
  const command = commands[0];
  if (command.kind !== "insert") throw new Error(`expected an insert command, got ${command.kind}`);
  expect(command.text).toBe('https://example.com/|main > button:nth-of-type(2)|button| "Sign in"|1280x800|10,21,100x30');
  expect(command.files).toHaveLength(1);
  expect(command.files?.[0].type).toBe("image/png");
  expect(command.files?.[0].name).toMatch(/^browser-element-\d{8}T\d{6}Z\.png$/);
});

it("cancels a running pick instead of starting another, and inserts nothing", async () => {
  const { bridge } = fakeBridge();
  await act(async () => root.render(<Probe bridge={bridge} tabId="tab-1" onError={vi.fn()} />));
  await act(async () => state.toggle());
  await act(async () => state.toggle());

  expect(bridge.pickElement).toHaveBeenCalledTimes(1);
  expect(bridge.cancelElementPick).toHaveBeenCalledExactlyOnceWith("tab-1");
  expect(state.picking).toBe(false);
  expect(commands).toHaveLength(0);
});

it("ends a running pick when the active page changes", async () => {
  const { bridge } = fakeBridge();
  await act(async () => root.render(<Probe bridge={bridge} tabId="tab-1" onError={vi.fn()} />));
  await act(async () => state.toggle());
  await act(async () => root.render(<Probe bridge={bridge} tabId="tab-2" onError={vi.fn()} />));

  expect(bridge.cancelElementPick).toHaveBeenCalledExactlyOnceWith("tab-1");
  expect(commands).toHaveLength(0);
});
