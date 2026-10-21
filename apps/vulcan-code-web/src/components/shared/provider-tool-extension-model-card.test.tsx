// @vitest-environment jsdom

import { act } from "react";
import { createRoot } from "react-dom/client";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";

import { TooltipProvider } from "@/ui-kit/primitives/tooltip";
import { ProviderToolExtensionModelCard } from "./provider-tool-extension-model-card";

describe("ProviderToolExtensionModelCard", () => {
  // Both settings surfaces must retain the same compact tooltip and source hierarchy.
  // 两个设置界面必须保持相同的紧凑 Tooltip 与来源层级。
  it("places the checkbox beside two compact title and control rows", () => {
    // markup captures the exact shared card hierarchy without browser-specific layout noise.
    // markup 捕获精确的共享卡片层级，不引入浏览器特定布局噪音。
    const markup = renderToStaticMarkup(
      <TooltipProvider>
        <ProviderToolExtensionModelCard
          accountControl={<button type="button">minimax</button>}
          characteristics="Fast generation"
          checked
          checkboxDisabled={false}
          constraints="约束：Schema；Required arguments: prompt."
          limitations="限制：No capability beyond the returned schema."
          modelId="MiniMax-Hailuo-2.3-Fast"
          onCheckedChange={vi.fn()}
          providerId="minimax"
          summary="Generate one MP4 video and wait for provider completion."
          title="MiniMax Hailuo 2.3 Fast"
        />
      </TooltipProvider>,
    );
    // checkboxOffset identifies the first control in the card header.
    // checkboxOffset 标识卡片标题中的第一个控件。
    const checkboxOffset = markup.indexOf("type=\"checkbox\"");
    // titleOffset identifies the model title following the checkbox.
    // titleOffset 标识勾选框之后的模型标题。
    const titleOffset = markup.indexOf('<strong title="MiniMax Hailuo 2.3 Fast">MiniMax Hailuo 2.3 Fast</strong>');
    // firstTooltipOffset identifies the first profile icon placed in the title row.
    // firstTooltipOffset 标识标题行中的第一个档案图标。
    const firstTooltipOffset = markup.indexOf('data-extension-profile-tooltip="summary"');
    // controlsRowOffset identifies the second row shared by identity and the account control.
    // controlsRowOffset 标识由模型标识与账号控件共享的第二行。
    const controlsRowOffset = markup.indexOf('data-extension-card-row="controls"');
    // identityOffset identifies the low-priority provider/model text in the second row.
    // identityOffset 标识第二行中的低优先级供应商/模型文本。
    const identityOffset = markup.indexOf("data-extension-card-identity");
    // accountOffset identifies the compact account control at the right of the second row.
    // accountOffset 标识第二行右侧的紧凑账号控件。
    const accountOffset = markup.indexOf('data-extension-card-section="account"');
    // tooltipCount proves that description, constraints, and limitations all use icon triggers.
    // tooltipCount 证明说明、约束和限制均使用图标触发器。
    const tooltipCount = markup.match(/data-extension-profile-tooltip=/g)?.length ?? 0;

    expect(checkboxOffset).toBeGreaterThanOrEqual(0);
    expect(titleOffset).toBeGreaterThan(checkboxOffset);
    expect(firstTooltipOffset).toBeGreaterThan(titleOffset);
    expect(tooltipCount).toBe(3);
    expect(controlsRowOffset).toBeGreaterThan(firstTooltipOffset);
    expect(identityOffset).toBeGreaterThan(controlsRowOffset);
    expect(accountOffset).toBeGreaterThan(identityOffset);
    expect(markup).not.toContain('data-extension-card-section="summary"');
    expect(markup).not.toContain('data-extension-card-section="constraints"');
    expect(markup).not.toContain('data-extension-card-section="limitations"');
    expect(markup).not.toContain('data-extension-card-section="source"');
    expect(markup).toContain("minimax/MiniMax-Hailuo-2.3-Fast");
    expect(markup).toContain(">minimax</button>");
  });

  // Card clicks toggle selection while dedicated controls retain their own behavior.
  // 卡片点击切换选择状态，同时独立控件保留各自行为。
  it("toggles from card content but excludes account and tooltip controls", () => {
    // onCheckedChange records selection requests emitted by the shared card.
    // onCheckedChange 记录共享卡片发出的选择变更请求。
    const onCheckedChange = vi.fn();
    // container hosts the interactive React tree in the jsdom document.
    // container 在 jsdom 文档中承载可交互的 React 树。
    const container = document.createElement("div");
    // root owns the mounted card lifecycle for this interaction test.
    // root 管理本交互测试中已挂载卡片的生命周期。
    const root = createRoot(container);

    act(() => {
      root.render(
        <TooltipProvider>
          <ProviderToolExtensionModelCard
            accountControl={<button type="button">minimax</button>}
            characteristics={null}
            checked={false}
            checkboxDisabled={false}
            constraints="Schema constraints"
            limitations="Model limitations"
            modelId="speech-02"
            onCheckedChange={onCheckedChange}
            providerId="minimax"
            summary="Model summary"
            title="MiniMax Speech 02"
          />
        </TooltipProvider>,
      );
    });

    // title is ordinary card content and must toggle the unchecked model on.
    // title 是普通卡片内容，必须将未勾选模型切换为勾选。
    const title = container.querySelector("strong") as HTMLElement;
    act(() => title.click());
    expect(onCheckedChange).toHaveBeenCalledOnce();
    expect(onCheckedChange).toHaveBeenCalledWith(true);

    onCheckedChange.mockClear();
    // Only the native checkbox owns selection semantics; the containing article does not duplicate its tab stop.
    // 仅原生复选框持有选择语义；外层文章容器不重复占用键盘停靠点。
    const card = container.querySelector("article") as HTMLElement;
    const checkbox = container.querySelector<HTMLInputElement>('input[type="checkbox"]')!;
    expect(card.getAttribute("role")).toBeNull();
    expect(card.hasAttribute("tabindex")).toBe(false);
    expect(container.querySelectorAll('input[type="checkbox"], [role="checkbox"]')).toHaveLength(1);
    expect(checkbox.tabIndex).toBe(0);
    act(() => checkbox.click());
    expect(onCheckedChange).toHaveBeenCalledExactlyOnceWith(true);

    onCheckedChange.mockClear();
    // accountRegion includes both the selector and its surrounding click area.
    // accountRegion 同时覆盖账号选择器及其周围点击区域。
    const accountRegion = container.querySelector(
      '[data-extension-card-section="account"]',
    ) as HTMLElement;
    act(() => accountRegion.click());
    expect(onCheckedChange).not.toHaveBeenCalled();

    // tooltipButton represents every profile icon's dedicated button behavior.
    // tooltipButton 代表每个档案图标自身的按钮行为。
    const tooltipButton = container.querySelector(
      '[data-extension-profile-tooltip="summary"]',
    ) as HTMLElement;
    act(() => tooltipButton.click());
    expect(onCheckedChange).not.toHaveBeenCalled();

    act(() => root.unmount());
  });

  it("keeps disabled selection inert while the independent account control remains operable", () => {
    // Selection and account callbacks own separate actions even while only the selection is disabled.
    // 即使只有选择被禁用，选择与账号回调仍持有独立操作。
    const onCheckedChange = vi.fn();
    const onAccount = vi.fn();
    const container = document.createElement("div");
    const root = createRoot(container);
    try {
      act(() => root.render(<TooltipProvider><ProviderToolExtensionModelCard
        accountControl={<button type="button" onClick={onAccount}>Account</button>}
        characteristics={null} checked checkboxDisabled constraints="Constraints" limitations="Limitations"
        modelId="model" onCheckedChange={onCheckedChange} providerId="provider" summary="Summary" title="Model"
      /></TooltipProvider>));
      act(() => container.querySelector<HTMLElement>("strong")!.click());
      act(() => container.querySelector<HTMLInputElement>('input[type="checkbox"]')!.click());
      act(() => container.querySelector<HTMLButtonElement>('[data-extension-card-section="account"] button')!.click());
      expect(onCheckedChange).not.toHaveBeenCalled();
      expect(onAccount).toHaveBeenCalledOnce();
      expect(container.querySelector<HTMLInputElement>('input[type="checkbox"]')!.checked).toBe(true);
    } finally { act(() => root.unmount()); }
  });
});
