// @vitest-environment jsdom
import React, { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { I18nProvider } from "@/i18n";
import { TooltipProvider } from "@/components/ui/tooltip";
import { ComposerAttachmentMenu } from "./composer-attachment-controls";
import { ComposerSettingsHostContext } from "./composer-settings-host";
import { ToolCollapsePreferencesContext } from "@/components/conversation/tool-collapse-preferences";

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
    onVmmPolicyChange: vi.fn(), onOpenGoal: vi.fn(), projectId: "synthetic", conversationId: null, onOpenKnowledge: vi.fn(),
    outputStyleCatalog: { defaultId: "balanced", selectedId: "balanced", entries: [
      { id: "balanced", labelZh: "平衡结构", labelEn: "Balanced", constraintLevel: "balanced", promptCharCount: 815, isDefault: true },
      { id: "structured", labelZh: "严格结构", labelEn: "Structured", constraintLevel: "strong", promptCharCount: 2464, isDefault: false },
    ] }, onOutputStyleChange: vi.fn(),
    answerMode: { mode: "standard", saving: false, awaitingRequest: false, available: true, select: vi.fn(), flush: vi.fn(async () => true) },
  };
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); footer.remove(); vi.unstubAllGlobals(); });

/** Mounts controlled production props and opens one menu through keyboard activation.
 * 挂载受控生产属性，通过键盘激活打开一个菜单。
 * Parameter kind selects footer settings or composer add; returns after focus effects commit.
 * 参数 kind 选择底栏设置或输入区添加；焦点副作用提交后返回。
 */
async function openMenu(kind: "settings" | "add" = "settings") {
  await act(async () => root.render(<I18nProvider initialLocale="zh-CN"><TooltipProvider>
    <ToolCollapsePreferencesContext.Provider value={{ defaultEnabled: true, activeConversationId: props.conversationId }}>
      <ComposerSettingsHostContext.Provider value={footer}><ComposerAttachmentMenu {...props} /></ComposerSettingsHostContext.Provider>
    </ToolCollapsePreferencesContext.Provider>
  </TooltipProvider></I18nProvider>));
  await act(async () => (kind === "settings" ? footer : host).querySelector<HTMLButtonElement>("button")!.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true })));
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
  props.conversationId = "thinking-inherit-menu";
  await openMenu(); await openSubmenu("思考弹窗");
  // Inheritance is a distinct choice even when the global value currently enables popups.
  // 即使全局值当前开启弹窗，继承仍是独立选择。
  const choices = Array.from(document.querySelectorAll<HTMLElement>('[role="menuitemradio"]'));
  expect(choices).toHaveLength(3);
  expect(choices.map((choice) => choice.textContent)).toEqual(["开启", "关闭", "继承系统"]);
  expect(choices.map((choice) => choice.getAttribute("aria-checked"))).toEqual(["false", "false", "true"]);
  await act(async () => choices[1].click());
  expect(props.onThinkingDisplayModeChange).toHaveBeenCalledWith("inline_output");
});

it.each([
  ["回答前回忆", "precheck", ["false", "true", "false"]],
  ["回答后记住", "writeback", ["false", "false", "true"]],
] as const)("retains editable %s policy while memory is not ready", async (label, policy, checked) => {
  await openMenu(); await openSubmenu("记忆选项"); await openSubmenu(label);
  // choices retain inherit/enabled/disabled as separate, source-declared policy states.
  // choices 保留源码声明的继承、启用与关闭三种独立策略状态。
  const choices = Array.from(document.querySelectorAll<HTMLElement>('[role="menuitemradio"]'));
  expect(choices.map((choice) => choice.getAttribute("aria-checked"))).toEqual(checked);
  await act(async () => choices[0].click());
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
  expect(host.querySelector('.workspace-settings-button')).toBeNull();
  expect(Array.from(document.querySelectorAll('[role="menuitem"], [role="menuitemcheckbox"]'), entry => entry.textContent)).toEqual(["附件", "计划模式", "目标"]);
  expect(document.querySelector('[role="menu"]')?.textContent).not.toContain("记忆选项");
});

it("switches only the active conversation's tool folding and can restore the system default", async () => {
  // Storage is owned by the test, not the user's real browser profile.
  // 存储归测试所有，不使用用户真实浏览器配置。
  vi.stubGlobal("localStorage", { getItem: () => null, setItem: vi.fn() });
  props.conversationId = "footer-folding-a";
  await openMenu(); await openSubmenu("工具折叠");
  // choices use a nested radio group, not an independent toggle and reset command.
  // choices 使用嵌套单选组，而非独立开关和重置命令。
  const choices = Array.from(document.querySelectorAll<HTMLElement>('[role="menuitemradio"]'));
  expect(choices.map((item) => item.textContent)).toEqual(["关闭折叠", "开启折叠", "继承系统"]);
  expect(choices.map((item) => item.getAttribute("aria-checked"))).toEqual(["false", "false", "true"]);
  await act(async () => choices[0].click());
  await openMenu(); await openSubmenu("工具折叠");
  expect(document.querySelector('[role="menuitemradio"][aria-checked="true"]')?.textContent).toBe("关闭折叠");
  await act(async () => document.activeElement!.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true })));
  props.conversationId = "footer-folding-b";
  await openMenu(); await openSubmenu("工具折叠");
  expect(document.querySelector('[role="menuitemradio"][aria-checked="true"]')?.textContent).toBe("继承系统");
  await act(async () => document.activeElement!.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true })));
  props.conversationId = "footer-folding-a";
  await openMenu(); await openSubmenu("工具折叠");
  expect(document.querySelector('[role="menuitemradio"][aria-checked="true"]')?.textContent).toBe("关闭折叠");
  // reset removes this session's explicit override.
  // reset 移除此会话的明确覆盖值。
  const reset = Array.from(document.querySelectorAll<HTMLElement>('[role="menuitemradio"]'))
    .find((item) => item.textContent === "继承系统")!;
  await act(async () => reset.click());
  await openMenu(); await openSubmenu("工具折叠");
  expect(document.querySelector('[role="menuitemradio"][aria-checked="true"]')?.textContent).toBe("继承系统");
});

it("closes relocated options when the active session changes and uses the new controlled selection", async () => {
  await openMenu(); await openSubmenu("输出风格");
  props.conversationId = "new-owner";
  props.sessionOutputStyle = { overrideStyleId: "structured", desiredStyleId: "structured", appliedStyleId: "structured", pendingActivation: false };
  await act(async () => root.render(<I18nProvider initialLocale="zh-CN"><TooltipProvider><ComposerSettingsHostContext.Provider value={footer}><ComposerAttachmentMenu {...props} /></ComposerSettingsHostContext.Provider></TooltipProvider></I18nProvider>));
  expect(document.querySelector('[role="menu"]')).toBeNull();
  await openMenu(); await openSubmenu("输出风格");
  expect(document.querySelector('[role="menuitemradio"][aria-checked="true"]')?.textContent).toContain("严格结构");
});
