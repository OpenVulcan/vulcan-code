// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { PanelTab } from "./panel-tab";

// host and root own one tab render per test.
// host 与 root 在每个测试中持有一次页签渲染。
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

it("selects through the tab button and closes without selecting", async () => {
  const onSelect = vi.fn();
  const onClose = vi.fn();
  await act(async () => root.render(
    <div role="tablist">
      <PanelTab active={false} closeLabel="Close" label="Files" title="Files" onClose={onClose} onSelect={onSelect} />
    </div>,
  ));
  const tab = host.querySelector<HTMLButtonElement>('[role="tab"]')!;
  expect(tab.getAttribute("aria-selected")).toBe("false");
  expect(tab.tabIndex).toBe(-1);
  expect(tab.contains(host.querySelector('[aria-label="Close"]'))).toBe(false);
  await act(async () => host.querySelector<HTMLButtonElement>('[aria-label="Close"]')!.click());
  expect(onClose).toHaveBeenCalledOnce();
  expect(onSelect).not.toHaveBeenCalled();
  await act(async () => tab.click());
  expect(onSelect).toHaveBeenCalledOnce();
});

it("disables both buttons and lets tabProps replace the roving index", async () => {
  await act(async () => root.render(
    <PanelTab active={false} closeLabel="Close" disabled label="Page" tabProps={{ tabIndex: 0, "data-page-id": "p1" }}
      title="Page" onClose={() => {}} onSelect={() => {}} />,
  ));
  const buttons = [...host.querySelectorAll("button")];
  expect(buttons.every((button) => button.disabled)).toBe(true);
  const tab = host.querySelector<HTMLButtonElement>('[role="tab"]')!;
  expect(tab.tabIndex).toBe(0);
  expect(tab.dataset.pageId).toBe("p1");
});
