// @vitest-environment jsdom
import React, { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, vi } from "vitest";
import type { LuaSkillSummary } from "@/types";
import type { SkillActionProps } from "./skill-area-types";
import { LuaSkillToolsDialog } from "./lua-skill-card";

vi.mock("@/i18n", () => ({ useI18n: () => ({ locale: "en-US", t: (key: string) => key }) }));

it("blocks dismissal during a tool write and restores current host authority on failure", async () => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  const host = document.createElement("div"); document.body.append(host); const root = createRoot(host);
  let reject!: (reason: Error) => void;
  const save = vi.fn(() => new Promise<void>((_resolve, fail) => { reject = fail; }));
  const close = vi.fn();
  const tool = { localName: "search", canonicalName: "fixture.search", description: "Fixture", registrationHelp: "Fixture", registrationLabel: "Explicit", registrationState: "explicit" as const };
  const skill = { skillId: "fixture", displayName: "Fixture", tools: [tool] } as LuaSkillSummary;
  const actions = { onSetLuaSkillToolRegistrationState: save } as unknown as SkillActionProps;
  try {
    await act(async () => root.render(<LuaSkillToolsDialog open busy={false} skill={skill} actions={actions} onOpenChange={close} />));
    const implicit = [...document.querySelectorAll("button")].find((button) => button.textContent === "skills.lua.toolImplicit")!;
    await act(async () => { implicit.click(); implicit.click(); });
    expect(save).toHaveBeenCalledTimes(1);
    await act(async () => document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true })));
    expect(close).not.toHaveBeenCalled();
    await act(async () => root.render(<LuaSkillToolsDialog open busy={false} skill={{ ...skill, tools: [{ ...tool, registrationState: "disabled" }] }} actions={actions} onOpenChange={close} />));
    await act(async () => reject(new Error("write rejected")));
    expect(document.body.textContent).toContain("write rejected");
    const disabled = [...document.querySelectorAll("button")].find((button) => button.textContent === "skills.lua.toolDisabled")!;
    expect(disabled.getAttribute("data-state")).toBe("on");
    expect(implicit.disabled).toBe(false);
  } finally {
    await act(async () => root.unmount()); host.remove(); vi.unstubAllGlobals();
  }
});
