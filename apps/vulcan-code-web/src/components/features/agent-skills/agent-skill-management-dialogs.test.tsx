// @vitest-environment jsdom
import React, { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import type { AgentSkillCategorySummary } from "@/types";
import { AgentSkillCategoryManagementDialog, AgentSkillEditorDialog } from "./agent-skill-management-dialogs";

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
