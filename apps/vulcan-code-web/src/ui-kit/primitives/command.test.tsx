// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, describe, expect, it, vi } from "vitest";

import { Command, CommandInput, CommandItem, CommandList } from "./command";

let root: Root | null = null;
let container: HTMLDivElement | null = null;

afterEach(async () => {
  if (root) await act(async () => root!.unmount());
  container?.remove();
  root = null;
  container = null;
});

// mount renders three rows and reports which row Enter selects.
// mount 渲染三行，并报告回车选中的行。
async function mount(defaultValue?: string): Promise<{ input: HTMLInputElement; onSelect: ReturnType<typeof vi.fn> }> {
  Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
  Element.prototype.scrollIntoView = vi.fn();
  const onSelect = vi.fn();
  container = document.createElement("div");
  document.body.append(container);
  root = createRoot(container);
  await act(async () => root!.render(
    <Command defaultValue={defaultValue}>
      <CommandInput aria-label="filter" />
      <CommandList>
        {["alpha", "beta", "gamma"].map((value) => <CommandItem key={value} onSelect={onSelect} value={value}>{value}</CommandItem>)}
      </CommandList>
    </Command>,
  ));
  return { input: container.querySelector("input")!, onSelect };
}

// press sends one key to the search field.
// press 向搜索框发送一次按键。
async function press(input: HTMLInputElement, key: string): Promise<void> {
  await act(async () => { input.dispatchEvent(new KeyboardEvent("keydown", { key, bubbles: true, cancelable: true })); });
}

// type replaces the search text through the input's change event.
// type 通过输入框的变更事件替换搜索文本。
async function type(input: HTMLInputElement, text: string): Promise<void> {
  const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!;
  await act(async () => {
    setter.call(input, text);
    input.dispatchEvent(new Event("input", { bubbles: true }));
  });
}

describe("Command active row", () => {
  it("starts on the first row without a default", async () => {
    const { input, onSelect } = await mount();
    await press(input, "Enter");
    expect(onSelect).toHaveBeenCalledWith("alpha");
  });

  it("starts on the default row and keyboard movement continues from it", async () => {
    const { input, onSelect } = await mount("beta");
    await press(input, "Enter");
    expect(onSelect).toHaveBeenLastCalledWith("beta");
    await press(input, "ArrowDown");
    await press(input, "Enter");
    expect(onSelect).toHaveBeenLastCalledWith("gamma");
  });

  it("moves to the first match once a search is typed", async () => {
    const { input, onSelect } = await mount("gamma");
    await type(input, "a");
    await press(input, "Enter");
    expect(onSelect).toHaveBeenLastCalledWith("alpha");
  });
});
