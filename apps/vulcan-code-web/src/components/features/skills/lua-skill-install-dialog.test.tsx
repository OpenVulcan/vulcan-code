// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, vi } from "vitest";
import { LuaSkillInstallDialog } from "./lua-skills-panel";
import type { SkillActionProps } from "./skill-area-types";

vi.mock("@/i18n", () => ({ useI18n: () => ({ locale: "en-US", t: (key: string) => key }) }));

it("keeps a failed install editable and blocks duplicate submits and dismissal during retry", async () => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  const host = document.createElement("div"); document.body.append(host);
  const root = createRoot(host);
  let complete!: () => void;
  const install = vi.fn().mockRejectedValueOnce(new Error("source unavailable"))
    .mockImplementation(() => new Promise<void>((resolve) => { complete = resolve; }));
  const close = vi.fn();
  try {
    await act(async () => root.render(<LuaSkillInstallDialog open actions={{ onInstallLuaSkill: install } as unknown as SkillActionProps} onOpenChange={close} />));
    const input = document.querySelector<HTMLInputElement>(".lua-skill-install-field input")!;
    await act(async () => {
      Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(input, "fixture/skill");
      input.dispatchEvent(new Event("input", { bubbles: true }));
    });
    const submit = document.querySelector<HTMLButtonElement>('button[type="submit"]')!;
    expect(submit.form).toBe(document.querySelector("form"));
    await act(async () => submit.click());
    expect(document.body.textContent).toContain("source unavailable");
    expect(input.value).toBe("fixture/skill");
    expect(input.disabled).toBe(false);
    expect(close).not.toHaveBeenCalled();
    await act(async () => { submit.click(); submit.click(); });
    expect(install).toHaveBeenCalledTimes(2);
    expect(input.disabled).toBe(true);
    await act(async () => document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true })));
    expect(close).not.toHaveBeenCalled();
    await act(async () => complete());
    expect(close).toHaveBeenCalledWith(false);
  } finally {
    await act(async () => root.unmount()); host.remove(); vi.unstubAllGlobals();
  }
});
