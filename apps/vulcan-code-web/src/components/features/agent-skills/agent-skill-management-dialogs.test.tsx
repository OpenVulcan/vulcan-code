// @vitest-environment jsdom
import React, { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import type { AgentSkillCategorySummary, AgentSkillRecordSummary } from "@/types";
import { AgentSkillCategoryManagementDialog, AgentSkillEditorDialog } from "./agent-skill-management-dialogs";

// The i18n mock resolves real English messages so the dialog renders without a provider.
// i18n 模拟解析真实英文消息，使对话框无需提供器即可渲染。
vi.mock("@/i18n", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/i18n")>();
  const t = (key: import("@/i18n").TranslationKey) => actual.readLocaleMessage("en-US", key);
  return { ...actual, useI18n: () => ({ locale: "en-US", setLocale: () => undefined, t }) };
});

// category is a complete synthetic category; no real skill catalog or filesystem is accessed.
// category 是完整合成分类；不访问真实技能目录或文件系统。
const category: AgentSkillCategorySummary = { categoryId: "uncategorized", name: "Uncategorized",
  emoji: "📁", color: "#64748B", order: 0, builtin: true, locales: {} };
// root and host own each isolated React dialog test.
// root 与 host 持有每个隔离 React 弹窗测试。
let root: Root;
let host: HTMLDivElement;
beforeEach(() => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  host = document.createElement("div"); document.body.append(host); root = createRoot(host);
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); vi.unstubAllGlobals(); });

// saveButton resolves the actual portaled control through its visible label.
// saveButton 根据可见标签解析真实传送门控件。
// No parameters; returns the unique save action or fails the test.
// 无参数；返回唯一保存动作，否则测试失败。
function saveButton(): HTMLButtonElement {
  const button = [...document.querySelectorAll("button")].find((value) => value.textContent === "Save skill");
  expect(button).toBeDefined();
  return button!;
}

it("allows an unchanged draft to retry a transient save failure", async () => {
  const save = vi.fn().mockRejectedValueOnce(new Error("temporary service failure")).mockResolvedValue(undefined);
  const close = vi.fn();
  await act(async () => root.render(<AgentSkillEditorDialog categories={[category]} locale="en-US" open copy={false} record={null} onSave={save} onOpenChange={close} />));
  await act(async () => saveButton().click());
  expect(document.body.textContent).toContain("temporary service failure");
  expect(saveButton().disabled).toBe(false);
  await act(async () => saveButton().click());
  expect(save).toHaveBeenCalledTimes(2);
  expect(close).toHaveBeenCalledWith(false);
});

it("retains edited fields when a catalog refresh replaces category objects", async () => {
  const save = vi.fn().mockResolvedValue(undefined);
  const close = vi.fn();
  await act(async () => root.render(<AgentSkillEditorDialog categories={[category]} locale="en-US" open copy={false} record={null} onSave={save} onOpenChange={close} />));
  const input = document.querySelector<HTMLInputElement>('input[value="new-skill"]')!;
  await act(async () => {
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(input, "edited-skill");
    input.dispatchEvent(new Event("input", { bubbles: true }));
  });
  await act(async () => root.render(<AgentSkillEditorDialog categories={[{ ...category }]} locale="en-US" open copy={false} record={null} onSave={save} onOpenChange={close} />));
  expect(input.value).toBe("edited-skill");
  await act(async () => saveButton().click());
  expect(save.mock.calls[0][0].name).toBe("edited-skill");
});

it("blocks duplicate submission and dismissal until the active write resolves", async () => {
  let complete!: () => void;
  const save = vi.fn(() => new Promise<void>((resolve) => { complete = resolve; }));
  const close = vi.fn();
  await act(async () => root.render(<AgentSkillEditorDialog categories={[category]} locale="en-US" open copy={false} record={null} onSave={save} onOpenChange={close} />));
  const button = saveButton();
  await act(async () => { button.click(); button.click(); });
  expect(save).toHaveBeenCalledTimes(1);
  expect(document.querySelector("fieldset")!.disabled).toBe(true);
  await act(async () => document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true })));
  expect(close).not.toHaveBeenCalled();
  await act(async () => complete());
  expect(close).toHaveBeenCalledWith(false);
});

it("requires category deletion confirmation and retains a failed confirmation for retry", async () => {
  const remove = vi.fn().mockRejectedValueOnce(new Error("temporary deletion failure")).mockResolvedValue(undefined);
  await act(async () => root.render(<AgentSkillCategoryManagementDialog categories={[{ ...category, builtin: false, categoryId: "custom" }]} locale="en-US" open onSave={async () => {}} onDelete={remove} onOpenChange={() => {}} />));
  await act(async () => document.querySelector<HTMLButtonElement>('button[aria-label="Delete category"]')!.click());
  expect(remove).not.toHaveBeenCalled();
  const dialog = document.querySelector('[role="alertdialog"]')!;
  const confirm = [...dialog.querySelectorAll("button")].find((button) => button.textContent === "Delete")!;
  await act(async () => confirm.click());
  expect(dialog.textContent).toContain("temporary deletion failure");
  expect(confirm.disabled).toBe(false);
  await act(async () => confirm.click());
  expect(remove).toHaveBeenCalledTimes(2);
  expect(remove).toHaveBeenLastCalledWith("custom");
  expect(document.querySelector('[role="alertdialog"]')).toBeNull();
});

// typeInto replaces one labelled field value through React's native input setter.
// typeInto 通过 React 原生输入设置器替换一个带标签字段的值。
// Parameters: label is the visible field label without the required marker; value is the new text.
// 参数：label 为不含必填标记的可见字段标签；value 为新文本。
async function typeInto(label: string, value: string): Promise<void> {
  const field = [...document.querySelectorAll("label")].find((candidate) => candidate.querySelector("span")?.textContent?.replace(/ \*$/u, "") === label);
  expect(field, `field "${label}" should be rendered`).toBeDefined();
  const input = field!.querySelector("input")!;
  await act(async () => {
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(input, value);
    input.dispatchEvent(new Event("input", { bubbles: true }));
  });
}

// buttonWithText resolves one portaled button by its exact visible text.
// buttonWithText 按精确可见文本解析一个传送门按钮。
function buttonWithText(text: string): HTMLButtonElement {
  const button = [...document.querySelectorAll("button")].find((value) => value.textContent === text);
  expect(button, `button "${text}" should be rendered`).toBeDefined();
  return button!;
}

it("saves a user category with one name and description and no per-language fields", async () => {
  const save = vi.fn().mockResolvedValue(undefined);
  await act(async () => root.render(<AgentSkillCategoryManagementDialog categories={[category]} locale="en-US" open onSave={save} onDelete={async () => {}} onOpenChange={() => {}} />));
  await act(async () => buttonWithText("Add category").click());
  expect(document.body.textContent).not.toContain("English name");
  expect(document.body.textContent).not.toContain("中文名称");
  await typeInto("category-id", "research");
  await typeInto("Name", "Research");
  await typeInto("Description", "Reading and summarizing sources.");
  await act(async () => buttonWithText("Save category").click());
  expect(save).toHaveBeenCalledTimes(1);
  expect(save.mock.calls[0][0]).toMatchObject({ categoryId: "research", name: "Research", description: "Reading and summarizing sources." });
  expect(save.mock.calls[0][0]).not.toHaveProperty("locales");
});

it("copies a localized system skill into a single-language draft in the viewer's language", async () => {
  const save = vi.fn().mockResolvedValue(undefined);
  // systemSkill is a synthetic bundled skill whose Chinese display metadata comes only from its locale map.
  // systemSkill 是合成的内置技能，其中文展示元数据仅来自语言映射。
  const systemSkill: AgentSkillRecordSummary = {
    skillKey: "system:security-review", name: "security-review", displayName: "Security review",
    description: "Review code against security practices.",
    locales: { "zh-CN": { displayName: "安全审查", shortDescription: "按照安全最佳实践审查代码。" } },
    scopeKey: "system", canEdit: false, canDelete: false, canToggle: true, canCopy: true, canRestoreDefault: false,
    effective: true, enabled: true, categoryId: "uncategorized", tags: [],
    standard: { description: "Use when reviewing code for security issues.", metadata: {} },
    origin: { kind: "bundled", managed: true }, resources: { scripts: 0, references: 0, assets: 0 },
    skillFile: "SKILL.md", baseDir: "skills/security-review", body: "# Instructions\n",
  };
  await act(async () => root.render(<AgentSkillEditorDialog categories={[category]} locale="zh-CN" open copy record={systemSkill} onSave={save} onOpenChange={() => {}} />));
  expect(document.body.textContent).not.toContain("English name");
  await act(async () => saveButton().click());
  const request = save.mock.calls[0][0];
  expect(request.interface).toMatchObject({ displayName: "安全审查", shortDescription: "按照安全最佳实践审查代码。" });
  expect(request.interface).not.toHaveProperty("locales");
});
