// @vitest-environment jsdom
import React, { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { I18nProvider } from "@/i18n";
import { TooltipProvider } from "@/components/ui/tooltip";
import { ComposerAttachmentMenu } from "./composer-attachment-controls";

// Independent context/knowledge owners have their own interaction suites; this harness owns the add menu itself.
// 独立的上下文与知识库归属已有交互测试；本测试仅持有加号菜单本身。
vi.mock("@/components/features/code-context/vce-project-controls", () => ({ VceComposerMenu: () => null }));
vi.mock("./composer-knowledge-control", () => ({ ComposerKnowledgeControl: () => null }));

// MenuProps is the production menu contract; callbacks record exact public intents without host writes.
// MenuProps 是生产菜单契约；回调记录精确公开意图，不写入宿主。
type MenuProps = React.ComponentProps<typeof ComposerAttachmentMenu>;
// host and root isolate real Radix portal and focus state per test.
// host 与 root 为各项测试隔离真实 Radix 浮层及焦点状态。
let host: HTMLDivElement;
let root: Root;
// props preserves declared controlled values until the owner explicitly projects a new state.
// props 保留明确的受控值，直到所属组件明确投影新状态。
let props: MenuProps;
beforeEach(() => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  vi.stubGlobal("ResizeObserver", class { observe() {} unobserve() {} disconnect() {} });
  host = document.createElement("div"); document.body.append(host); root = createRoot(host);
  props = {
    capabilities: [], thinkingDisplayMode: "popup_only", planModeEnabled: false,
    sessionVmmPolicy: { precheckMode: "enabled", memoryWritebackMode: "disabled" }, vmmAvailable: true, vmmStatus: "not_ready",
    onOpenMemorySettings: vi.fn(), onPick: vi.fn(), onThinkingDisplayModeChange: vi.fn(), onPlanModeChange: vi.fn(),
    onVmmPolicyChange: vi.fn(), onOpenGoal: vi.fn(), projectId: "synthetic", conversationId: null, onOpenKnowledge: vi.fn(),
    outputStyleCatalog: { defaultId: "balanced", selectedId: "balanced", entries: [
      { id: "balanced", labelZh: "平衡结构", labelEn: "Balanced", constraintLevel: "balanced", promptCharCount: 815, isDefault: true },
      { id: "structured", labelZh: "严格结构", labelEn: "Structured", constraintLevel: "strong", promptCharCount: 2464, isDefault: false },
    ] }, onOutputStyleChange: vi.fn(),
  };
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); vi.unstubAllGlobals(); });

/** Mounts controlled production props and opens the add menu through keyboard activation.
 * 挂载受控生产属性，通过键盘激活打开加号菜单。
 * No parameters; returns once the real menu commits its focus effects.
 * 无参数；真实菜单提交焦点副作用后返回。
 */
async function openMenu() {
  await act(async () => root.render(<I18nProvider initialLocale="zh-CN"><TooltipProvider><ComposerAttachmentMenu {...props} /></TooltipProvider></I18nProvider>));
  await act(async () => host.querySelector<HTMLButtonElement>("button")!.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true })));
}

/** Finds an exact visible menu entry and opens its real submenu using ArrowRight.
 * 查找精确可见菜单项，并用右方向键打开真实子菜单。
 * Parameter: label is the exact rendered label; returns after submenu effects commit.
 * 参数：label 是精确渲染标签；子菜单副作用提交后返回。
 */
async function openSubmenu(label: string) {
  // trigger follows the actual visible text rather than index order across nested portals.
  // trigger 遵循实际可见文本，不依赖嵌套浮层的索引顺序。
  const trigger = Array.from(document.querySelectorAll<HTMLElement>('[role="menuitem"]')).find((entry) => entry.textContent === label)!;
  expect(trigger).toBeDefined();
  await act(async () => { trigger.focus(); trigger.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowRight", bubbles: true })); });
}

it.each([null, "structured"])("exposes the exact draft output-style selection %s without implying immediate activation", async (style) => {
  props.draftOutputStyleId = style;
  await openMenu(); await openSubmenu("输出风格");
  // choices are mutually exclusive radio entries exposed to keyboard and assistive navigation.
  // choices 是提供给键盘与辅助导航的互斥单选菜单项。
  const choices = Array.from(document.querySelectorAll<HTMLElement>('[role="menuitemradio"]'));
  expect(choices).toHaveLength(3);
  expect(choices.map((choice) => choice.getAttribute("aria-checked"))).toEqual(style === null ? ["true", "false", "false"] : ["false", "false", "true"]);
  await act(async () => choices[style === null ? 2 : 0].click());
  expect(props.onOutputStyleChange).toHaveBeenCalledWith(style === null ? "structured" : null);
});

it("shows an existing session's pending style rather than selecting a different draft value", async () => {
  props.conversationId = "synthetic-session";
  props.draftOutputStyleId = "balanced";
  props.sessionOutputStyle = { overrideStyleId: "structured", desiredStyleId: "structured", appliedStyleId: "balanced", pendingActivation: true };
  await openMenu(); await openSubmenu("输出风格");
  expect(document.querySelector('[role="menuitemradio"][aria-checked="true"]')?.textContent).toContain("严格结构");
  expect(document.querySelector('[role="status"]')?.textContent).toBe("已保存，在新会话或上下文压缩后生效。");
  expect(props.onOutputStyleChange).not.toHaveBeenCalled();
});

it("exposes thinking choices as radios and preserves the exact typed selection", async () => {
  await openMenu(); await openSubmenu("思考弹窗");
  // The popup has exactly two choices, independently of model reasoning effort.
  // 弹窗只有两个选项，与模型推理强度独立。
  const choices = Array.from(document.querySelectorAll<HTMLElement>('[role="menuitemradio"]'));
  expect(choices).toHaveLength(2);
  expect(choices.map((choice) => choice.textContent)).toEqual(["开启", "关闭"]);
  expect(choices.map((choice) => choice.getAttribute("aria-checked"))).toEqual(["true", "false"]);
  await act(async () => choices[1].click());
  expect(props.onThinkingDisplayModeChange).toHaveBeenCalledWith("inline_output");
});

it.each([
  ["前置记忆注入", "precheck", ["false", "true", "false"]],
  ["后置记忆提交", "writeback", ["false", "false", "true"]],
] as const)("retains editable %s policy while memory is not ready", async (label, policy, checked) => {
  await openMenu(); await openSubmenu("VMM 记忆"); await openSubmenu(label);
  // choices retain inherit/enabled/disabled as separate, source-declared policy states.
  // choices 保留源码声明的继承、启用与关闭三种独立策略状态。
  const choices = Array.from(document.querySelectorAll<HTMLElement>('[role="menuitemradio"]'));
  expect(choices.map((choice) => choice.getAttribute("aria-checked"))).toEqual(checked);
  await act(async () => choices[0].click());
  expect(props.onVmmPolicyChange).toHaveBeenCalledWith(policy, "inherit");
});

it("keeps empty attachments explicit and dispatches plan and goal actions independently", async () => {
  await openMenu(); await openSubmenu("附件");
  expect(document.querySelector('[role="menuitem"][data-disabled]')).not.toBeNull();
  expect(props.onPick).not.toHaveBeenCalled();
  await act(async () => document.activeElement!.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true })));
  await openMenu();
  // plan is the actual controlled checkbox; selecting it leaves the menu open for further actions.
  // plan 是真实受控复选菜单项；选择后保留菜单以便继续操作。
  const plan = document.querySelector<HTMLElement>('[role="menuitemcheckbox"]')!;
  await act(async () => plan.click());
  expect(props.onPlanModeChange).toHaveBeenCalledWith(true);
  // goal is the exact goal-editor entry, not any nested configuration control.
  // goal 是精确目标编辑入口，不是其他嵌套配置控件。
  const goal = Array.from(document.querySelectorAll<HTMLElement>('[role="menuitem"]')).find((entry) => entry.textContent === "目标")!;
  await act(async () => goal.click());
  expect(props.onOpenGoal).toHaveBeenCalledTimes(1);
  expect(props.onVmmPolicyChange).not.toHaveBeenCalled();
});

it.each(["image", "video", "audio", "pdf", "file"])("dispatches the enabled %s file-picker intent exactly once", async (kind) => {
  props.capabilities = [{ kind, label: kind, enabled: true, native: true, bridged: false, accept: "" }];
  await openMenu(); await openSubmenu("附件");
  // item is the sole declared kind in the visible attachment submenu, independent of localized labels.
  // item 是可见附件子菜单中唯一声明的类型，与本地化标签独立。
  const item = document.querySelector<HTMLElement>('[data-slot="dropdown-menu-sub-content"] [role="menuitem"]')!;
  await act(async () => item.click());
  expect(props.onPick).toHaveBeenCalledExactlyOnceWith(kind);
  expect(props.onOpenGoal).not.toHaveBeenCalled();
});
