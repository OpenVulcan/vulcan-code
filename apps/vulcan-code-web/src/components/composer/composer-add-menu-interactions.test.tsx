// @vitest-environment jsdom
import React, { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { I18nProvider } from "@/i18n";
import { TooltipProvider } from "@/ui-kit/primitives/tooltip";
import { ComposerAttachmentMenu } from "./composer-attachment-controls";
import { ComposerSettingsHostContext } from "./composer-settings-host";
import { ToolCollapsePreferencesContext } from "@/components/conversation/tool-collapse-preferences";
vi.mock("./composer-knowledge-control", () => ({ ComposerKnowledgeControl: () => null }));

// MenuProps is the production menu contract; callbacks record exact public intents without host writes.
// MenuProps 是生产菜单契约；回调记录精确公开意图，不写入宿主。
type MenuProps = React.ComponentProps<typeof ComposerAttachmentMenu>;
// host and root isolate real Radix portal and focus state per test.
// host 与 root 为各项测试隔离真实 Radix 浮层及焦点状态。
let host: HTMLDivElement;
let root: Root;
// footer owns only the relocated DOM slot, not the menu state.
// footer 仅持有迁移后的 DOM 插槽，不持有菜单状态。
let footer: HTMLDivElement;
// props preserves declared controlled values until the owner explicitly projects a new state.
// props 保留明确的受控值，直到所属组件明确投影新状态。
let props: MenuProps;
beforeEach(() => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  vi.stubGlobal("ResizeObserver", class { observe() {} unobserve() {} disconnect() {} });
  host = document.createElement("div"); document.body.append(host); root = createRoot(host);
  footer = document.createElement("div"); document.body.append(footer);
  props = {
    capabilities: [], thinkingDisplayMode: "popup_only", planModeEnabled: false,
    sessionVmmPolicy: { precheckMode: "enabled", memoryWritebackMode: "disabled" }, vmmAvailable: true, vmmStatus: "not_ready",
    onOpenMemorySettings: vi.fn(), onPick: vi.fn(), onThinkingDisplayModeChange: vi.fn(), onPlanModeChange: vi.fn(),
    onVmmPolicyChange: vi.fn(), onOpenGoal: vi.fn(), projectId: "synthetic", conversationId: null, providers: [], onOpenKnowledge: vi.fn(),
    outputStyleCatalog: { defaultId: "balanced", selectedId: "balanced", entries: [
      { id: "balanced", labelZh: "平衡结构", labelEn: "Balanced", constraintLevel: "balanced", promptCharCount: 815, isDefault: true },
      { id: "structured", labelZh: "严格结构", labelEn: "Structured", constraintLevel: "strong", promptCharCount: 2464, isDefault: false },
    ] }, onOutputStyleChange: vi.fn(),
    answerMode: { mode: "standard", saving: false, awaitingRequest: false, available: true, select: vi.fn(), flush: vi.fn(async () => true) },
  };
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); footer.remove(); vi.unstubAllGlobals(); });

/** Mounts controlled production props.
 * 挂载受控生产属性。
 */
async function mount() {
  await act(async () => root.render(<I18nProvider initialLocale="zh-CN"><TooltipProvider>
    <ToolCollapsePreferencesContext.Provider value={{ defaultEnabled: true, activeConversationId: props.conversationId }}>
      <ComposerSettingsHostContext.Provider value={footer}><ComposerAttachmentMenu {...props} /></ComposerSettingsHostContext.Provider>
    </ToolCollapsePreferencesContext.Provider>
  </TooltipProvider></I18nProvider>));
}

/** Mounts the props and opens either the footer settings panel (by click) or the composer add menu (by keyboard).
 * 挂载属性并打开底栏设置面板（点击）或输入区添加菜单（键盘）。
 * Parameter kind selects footer settings or composer add; returns after focus effects commit.
 * 参数 kind 选择底栏设置或输入区添加；焦点副作用提交后返回。
 */
async function openMenu(kind: "settings" | "add" = "settings") {
  await mount();
  if (kind === "settings") {
    if (!document.querySelector(".vk-ss")) await act(async () => footer.querySelector<HTMLButtonElement>("button")!.click());
    return;
  }
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

/** Returns the radios of one settings choice group, named by its section id or its accessible label.
 * 返回某个设置选择组中的单选项，以区块标识或无障碍标签指定该组。
 */
function radios(name: string): HTMLElement[] {
  const group = document.querySelector(`[data-ss-section="${name}"] [role="radiogroup"], [role="radiogroup"][aria-label="${name}"]`);
  expect(group, `Missing choice group: ${name}`).not.toBeNull();
  return Array.from(group!.querySelectorAll<HTMLElement>('[role="radio"]'));
}

/** Returns the value of the checked radio in one settings choice group.
 * 返回某个设置选择组中已选中单选项的值。
 */
function checkedChoice(name: string): string | null | undefined {
  return radios(name).find((choice) => choice.getAttribute("aria-checked") === "true")?.getAttribute("value");
}

/** Returns the values of every radio in one settings choice group, in order.
 * 按顺序返回某个设置选择组中每个单选项的值。
 */
function choiceValues(name: string): Array<string | null> {
  return radios(name).map((choice) => choice.getAttribute("value"));
}

/** Returns the output-style select trigger.
 * 返回输出风格选择框的触发器。
 */
function styleTrigger(): HTMLButtonElement {
  const trigger = document.querySelector<HTMLButtonElement>('[role="combobox"][aria-label="输出风格"]');
  expect(trigger, "Missing output-style select").not.toBeNull();
  return trigger!;
}

it.each([null, "structured"])("exposes the exact draft output-style selection %s without implying immediate activation", async (style) => {
  props.draftOutputStyleId = style;
  await openMenu();
  expect(styleTrigger().textContent).toBe(style === null ? "继承全局" : "严格结构");
  await act(async () => styleTrigger().click());
  // choices are the inherited choice followed by every catalog style.
  // choices 依次为继承选项与目录中的每种风格。
  const choices = Array.from(document.querySelectorAll<HTMLElement>('[role="option"]'));
  expect(choices).toHaveLength(3);
  expect(choices.map((choice) => choice.getAttribute("aria-selected"))).toEqual(style === null ? ["true", "false", "false"] : ["false", "false", "true"]);
  await act(async () => choices[style === null ? 2 : 0].click());
  expect(props.onOutputStyleChange).toHaveBeenCalledWith(style === null ? "structured" : null);
});

it("shows an existing session's pending style rather than selecting a different draft value", async () => {
  props.conversationId = "synthetic-session";
  props.draftOutputStyleId = "balanced";
  props.sessionOutputStyle = { overrideStyleId: "structured", desiredStyleId: "structured", appliedStyleId: "balanced", pendingActivation: true };
  await openMenu();
  expect(styleTrigger().textContent).toBe("严格结构");
  expect(document.querySelector(".vk-ss")?.textContent).toContain("已保存，在新会话或上下文压缩后生效。");
  expect(props.onOutputStyleChange).not.toHaveBeenCalled();
});

it("exposes thinking choices as radios and preserves the exact typed selection", async () => {
  props.conversationId = "thinking-inherit-menu";
  await openMenu();
  // Inheritance is a distinct choice even when the global value currently enables popups.
  // 即使全局值当前开启弹窗，继承仍是独立选择。
  const choices = radios("thinking");
  expect(choiceValues("thinking")).toEqual(["inherit", "popup_only", "inline_output"]);
  expect(checkedChoice("thinking")).toBe("inherit");
  await act(async () => choices[2].click());
  expect(props.onThinkingDisplayModeChange).toHaveBeenCalledWith("inline_output");
});

it.each([
  ["回答前回忆", "precheck", "enabled"],
  ["回答后记住", "writeback", "disabled"],
] as const)("retains editable %s policy while memory is not ready", async (label, policy, checked) => {
  await openMenu();
  expect(document.querySelector(".vk-ss")?.textContent).toContain("记忆尚未就绪");
  // choices retain inherit/enabled/disabled as separate, source-declared policy states.
  // choices 保留源码声明的继承、启用与关闭三种独立策略状态。
  expect(checkedChoice(label)).toBe(checked);
  await act(async () => radios(label)[0].click());
  expect(props.onVmmPolicyChange).toHaveBeenCalledWith(policy, "inherit");
});

it("keeps empty attachments explicit and dispatches plan and goal actions independently", async () => {
  await openMenu("add"); await openSubmenu("附件");
  expect(document.querySelector('[role="menuitem"][data-disabled]')).not.toBeNull();
  expect(props.onPick).not.toHaveBeenCalled();
  await act(async () => document.activeElement!.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true })));
  await openMenu("add");
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
  await openMenu("add"); await openSubmenu("附件");
  // item is the sole declared kind in the visible attachment submenu, independent of localized labels.
  // item 是可见附件子菜单中唯一声明的类型，与本地化标签独立。
  const item = document.querySelector<HTMLElement>('[data-slot="dropdown-menu-sub-content"] [role="menuitem"]')!;
  await act(async () => item.click());
  expect(props.onPick).toHaveBeenCalledExactlyOnceWith(kind);
  expect(props.onOpenGoal).not.toHaveBeenCalled();
});

it("moves settings to the footer and leaves only attachment, plan and goal in the add menu", async () => {
  await openMenu("add");
  expect(footer.querySelector('button')?.textContent).toBe("会话设置");
  expect(host.querySelector('[data-action="session-settings"]')).toBeNull();
  expect(Array.from(document.querySelectorAll('[role="menuitem"], [role="menuitemcheckbox"]'), entry => entry.textContent)).toEqual(["附件", "计划模式", "目标"]);
  expect(document.querySelector('[role="menu"]')?.textContent).not.toContain("记忆选项");
});

it("switches only the active conversation's tool folding and can restore the system default", async () => {
  // Storage is owned by the test, not the user's real browser profile.
  // 存储归测试所有，不使用用户真实浏览器配置。
  vi.stubGlobal("localStorage", { getItem: () => null, setItem: vi.fn() });
  props.conversationId = "footer-folding-a";
  await openMenu();
  expect(choiceValues("tools")).toEqual(["inherit", "enabled", "disabled"]);
  expect(checkedChoice("tools")).toBe("inherit");
  await act(async () => radios("tools")[2].click());
  expect(checkedChoice("tools")).toBe("disabled");
  // Each owner change remounts the panel closed, so it is reopened for the next owner.
  // 每次归属变化都会让面板重新挂载并关闭，因此为下一个归属重新打开。
  props.conversationId = "footer-folding-b";
  await openMenu();
  expect(checkedChoice("tools")).toBe("inherit");
  props.conversationId = "footer-folding-a";
  await openMenu();
  expect(checkedChoice("tools")).toBe("disabled");
  // reset removes this session's explicit override.
  // reset 移除此会话的明确覆盖值。
  await act(async () => radios("tools")[0].click());
  expect(checkedChoice("tools")).toBe("inherit");
});

it("closes the panel when the active session changes and uses the new controlled selection", async () => {
  await openMenu();
  props.conversationId = "new-owner";
  props.sessionOutputStyle = { overrideStyleId: "structured", desiredStyleId: "structured", appliedStyleId: "structured", pendingActivation: false };
  await mount();
  expect(document.querySelector(".vk-ss")).toBeNull();
  await openMenu();
  expect(styleTrigger().textContent).toBe("严格结构");
});
