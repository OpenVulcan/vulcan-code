// @vitest-environment jsdom
import * as React from "react";
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, vi } from "vitest";
import { Combobox } from "./combobox";

// A busy parent must close the real popup and prevent old options from remaining interactive.
// 父组件忙碌时必须关闭真实弹层，防止旧选项继续接受交互。
it("closes an open popup when disabled and does not reopen after becoming enabled", async () => {
  // host owns only a local picker with two stable identities and duplicate display labels.
  // host 仅持有本地下拉框，其两个选项标识稳定且显示名称相同。
  const host = document.createElement("div"); document.body.append(host);
  const root = createRoot(host);
  const changed = vi.fn();
  const pickerOptions = [{ value: "first-id", label: "相同名称", keywords: ["alpha"] }, { value: "second-id", label: "相同名称", keywords: ["beta"] }];
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  vi.stubGlobal("ResizeObserver", class { observe() {} unobserve() {} disconnect() {} });
  // scrollDescriptor preserves the environment because jsdom does not implement layout scrolling.
  // scrollDescriptor 保留环境原值，因为 jsdom 不实现布局滚动。
  const scrollDescriptor = Object.getOwnPropertyDescriptor(Element.prototype, "scrollIntoView");
  Object.defineProperty(Element.prototype, "scrollIntoView", { configurable: true, value: () => {} });
  // render mounts the production picker with a controlled disabled flag.
  // render 挂载生产选择器，并由受控标志控制禁用状态。
  const render = (disabled: boolean) => root.render(<Combobox options={pickerOptions} disabled={disabled} ariaLabel="验收模型" placeholder="选择模型" searchPlaceholder="搜索模型" emptyText="无匹配" onValueChange={changed} />);
  try {
    await act(async () => render(false));
    await act(async () => host.querySelector<HTMLButtonElement>('[role="combobox"]')!.click());
    expect(document.querySelectorAll('[cmdk-item]')).toHaveLength(2);
    expect(document.querySelector('input[aria-label="搜索模型"]')).not.toBeNull();
    await act(async () => render(true));
    expect(document.querySelector('[cmdk-item]')).toBeNull();
    expect(host.querySelector('[role="combobox"]')?.getAttribute("aria-expanded")).toBe("false");
    await act(async () => render(false));
    expect(document.querySelector('[cmdk-item]')).toBeNull();
    expect(changed).not.toHaveBeenCalled();
  } finally {
    await act(async () => root.unmount()); host.remove(); vi.unstubAllGlobals();
    if (scrollDescriptor) Object.defineProperty(Element.prototype, "scrollIntoView", scrollDescriptor);
    else Reflect.deleteProperty(Element.prototype, "scrollIntoView");
  }
});

// Opening the list keeps keyboard focus on the chosen option, so Enter right away leaves the choice unchanged.
// 打开列表时键盘焦点停在已选项上，立即回车不会改变选择。
it("starts keyboard focus on the chosen option", async () => {
  const host = document.createElement("div"); document.body.append(host);
  const root = createRoot(host);
  const changed = vi.fn();
  const pickerOptions = [{ value: "first-id", label: "First" }, { value: "second-id", label: "Second" }];
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  vi.stubGlobal("ResizeObserver", class { observe() {} unobserve() {} disconnect() {} });
  const scrollDescriptor = Object.getOwnPropertyDescriptor(Element.prototype, "scrollIntoView");
  Object.defineProperty(Element.prototype, "scrollIntoView", { configurable: true, value: () => {} });
  try {
    await act(async () => root.render(<Combobox options={pickerOptions} value="second-id" ariaLabel="picker" placeholder="pick" searchPlaceholder="search" emptyText="none" onValueChange={changed} />));
    await act(async () => host.querySelector<HTMLButtonElement>('[role="combobox"]')!.click());
    const input = document.querySelector<HTMLInputElement>('input[aria-label="search"]')!;
    await act(async () => { input.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true, cancelable: true })); });
    expect(changed).toHaveBeenCalledWith("second-id");
  } finally {
    await act(async () => root.unmount()); host.remove(); vi.unstubAllGlobals();
    if (scrollDescriptor) Object.defineProperty(Element.prototype, "scrollIntoView", scrollDescriptor);
    else Reflect.deleteProperty(Element.prototype, "scrollIntoView");
  }
});
