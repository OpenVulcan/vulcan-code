// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import type { JudgeState } from "@/types";
import { useSetupGuide } from "./use-setup-guide";

let host: HTMLDivElement;
let root: Root;
let guide: ReturnType<typeof useSetupGuide>;

// Harness renders the actual hook for one judge state and readiness.
// Harness 为一个判定状态与就绪状态渲染真实钩子。
function Harness(props: { state: JudgeState | null | undefined; ready: boolean }) {
  guide = useSetupGuide(props.state, props.ready);
  return null;
}

beforeEach(() => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  // A Map-backed store stands in for the browser storage Node does not provide.
  // 以 Map 支撑的存储替代 Node 未提供的浏览器存储。
  const values = new Map<string, string>();
  vi.stubGlobal("localStorage", {
    getItem: (key: string) => values.get(key) ?? null,
    setItem: (key: string, value: string) => { values.set(key, value); },
  });
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
});
afterEach(async () => {
  await act(async () => root.unmount());
  host.remove();
  vi.unstubAllGlobals();
});

// The guide opens by itself once, only after startup and only while the judge is unset.
// 引导只在启动完成后、判定仍未设定时自动打开一次。
it("opens once on its own while the judge is unset", async () => {
  await act(async () => root.render(<Harness state={null} ready={false} />));
  expect(guide.open).toBe(false);
  await act(async () => root.render(<Harness state={undefined} ready />));
  expect(guide.open).toBe(false);
  await act(async () => root.render(<Harness state={null} ready />));
  expect(guide.open).toBe(true);
  await act(async () => guide.setOpen(false));
  await act(async () => root.unmount());
  root = createRoot(host);
  await act(async () => root.render(<Harness state={null} ready />));
  expect(guide.open).toBe(false);
  await act(async () => guide.show());
  expect(guide.open).toBe(true);
});

// A decided judge never opens the guide by itself.
// 已决定的判定绝不会自动打开引导。
it("stays closed once the judge is decided", async () => {
  await act(async () => root.render(<Harness state="off" ready />));
  expect(guide.open).toBe(false);
});
