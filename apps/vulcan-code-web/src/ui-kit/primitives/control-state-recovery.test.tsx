// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { renderToStaticMarkup } from "react-dom/server";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { SegmentedControl, SegmentedControlItem } from "./choice-controls";
import { Progress } from "./progress";
import { Alert } from "./alert";

// host and root isolate focus recovery checks from all other application mounts.
// host 和 root 将焦点恢复检查与其他应用挂载隔离。
let host: HTMLDivElement;
let root: Root;
beforeEach(() => { host = document.createElement("div"); document.body.append(host); root = createRoot(host); vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true); });
afterEach(async () => { await act(async () => root.unmount()); host.remove(); vi.unstubAllGlobals(); });

it("allows keyboard entry when the retained selected choice is disabled", async () => {
  // changed observes the actual primitive selection rather than reproducing focus logic.
  // changed 观察真实底层组件选择，不复刻焦点逻辑。
  const changed = vi.fn();
  await act(async () => root.render(<SegmentedControl value="deleted" onValueChange={changed} aria-label="Context window">
    <SegmentedControlItem value="default">Default</SegmentedControlItem>
    <SegmentedControlItem value="deleted" disabled>Unavailable</SegmentedControlItem>
    <SegmentedControlItem value="large">Large</SegmentedControlItem>
  </SegmentedControl>));
  // group is the primitive's keyboard entry point while no enabled item is selected.
  // group 是没有可用已选项时底层组件提供的键盘入口。
  const group = host.querySelector<HTMLElement>('[role="radiogroup"]')!;
  expect(group.tabIndex).toBe(0);
  await act(async () => group.focus());
  expect(document.activeElement?.textContent).toBe("Default");
  expect(changed).not.toHaveBeenCalled();
  await act(async () => (document.activeElement as HTMLButtonElement).click());
  expect(changed).toHaveBeenCalledExactlyOnceWith("default");
});

it("keeps progress accessibility synchronized with non-default scales and unknown values", () => {
  // determined and unknown represent a measured count and a transfer with no measured length.
  // determined 和 unknown 分别表示已测量计数和未测量长度的传输。
  const determined = renderToStaticMarkup(<Progress value={32} max={50} />);
  const unknown = renderToStaticMarkup(<Progress />);
  expect(determined).toContain('aria-valuenow="32"');
  expect(determined).toContain('aria-valuemax="50"');
  expect(determined).toContain('translateX(-36%)');
  expect(unknown).not.toContain("aria-valuenow");
  expect(unknown).toContain('data-state="indeterminate"');
});

it("renders the same default alert surface whether the variant is explicit or omitted", () => {
  expect(renderToStaticMarkup(<Alert>Notice</Alert>)).toBe(renderToStaticMarkup(<Alert variant="default">Notice</Alert>));
});
