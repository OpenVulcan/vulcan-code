// @vitest-environment jsdom
import React, { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { useFloatingContextDismiss } from "./use-floating-context-dismiss";

let host: HTMLDivElement;
let root: Root;
let opener: HTMLButtonElement;
let portal: HTMLDivElement;

beforeEach(() => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  opener = document.createElement("button");
  portal = document.createElement("div");
  host = document.createElement("div");
  document.body.append(opener, host, portal);
  root = createRoot(host);
  opener.focus();
});

afterEach(async () => {
  await act(async () => root.unmount());
  opener.remove();
  portal.remove();
  host.remove();
  vi.unstubAllGlobals();
});

// Stage renders the covered page area (with a composer input) beside the panel host (with a filter input).
// Stage 渲染被覆盖的页面区域（含输入框）以及旁边的面板宿主（含筛选输入框）。
function Stage(props: { active: boolean; onDismiss: () => void }) {
  const hostRef = React.useRef<HTMLDivElement | null>(null);
  const coveredRef = React.useRef<HTMLDivElement | null>(null);
  useFloatingContextDismiss({ active: props.active, hostRef, coveredRef, onDismiss: props.onDismiss });
  return (
    <>
      <div ref={coveredRef} data-testid="covered"><input data-testid="composer" /></div>
      <div ref={hostRef} data-testid="panel"><input data-testid="filter" /></div>
    </>
  );
}

const byTestId = (id: string) => document.querySelector<HTMLElement>(`[data-testid="${id}"]`)!;
const escape = (target: EventTarget) => target.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true }));
const press = (target: EventTarget, button = 0) => target.dispatchEvent(new MouseEvent("pointerdown", { button, bubbles: true, cancelable: true }));

it("closes on Escape from inside the panel and hands focus back to the opener", async () => {
  const onDismiss = vi.fn();
  await act(async () => root.render(<Stage active onDismiss={onDismiss} />));
  byTestId("filter").focus();

  escape(byTestId("filter"));

  expect(onDismiss).toHaveBeenCalledOnce();
  expect(document.activeElement).toBe(opener);
});

it("leaves Escape alone when another handler consumed it or focus is elsewhere on the page", async () => {
  const onDismiss = vi.fn();
  await act(async () => root.render(<Stage active onDismiss={onDismiss} />));
  const filter = byTestId("filter");
  filter.addEventListener("keydown", (event) => event.preventDefault(), { once: true });

  escape(filter);
  escape(byTestId("composer"));
  expect(onDismiss).not.toHaveBeenCalled();

  escape(document.body);
  expect(onDismiss).toHaveBeenCalledOnce();
});

it("closes on a primary press over the covered page only", async () => {
  const onDismiss = vi.fn();
  await act(async () => root.render(<Stage active onDismiss={onDismiss} />));

  press(byTestId("panel"));
  press(portal);
  press(byTestId("composer"), 2);
  expect(onDismiss).not.toHaveBeenCalled();

  press(byTestId("composer"));
  expect(onDismiss).toHaveBeenCalledOnce();
});

it("does nothing while the panel is docked or closed", async () => {
  const onDismiss = vi.fn();
  await act(async () => root.render(<Stage active={false} onDismiss={onDismiss} />));

  escape(document.body);
  press(byTestId("covered"));

  expect(onDismiss).not.toHaveBeenCalled();
});
