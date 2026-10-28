// @vitest-environment jsdom
import { act } from "react";
import { createRoot } from "react-dom/client";
import { expect, it, vi } from "vitest";
import { getLuaSkillsHubSkill, searchLuaSkillsHub } from "@/service/skills-actions-service";
import type { LuaSkillsHubContent, LuaSkillsHubSearchPage } from "@/types";
import { LuaSkillHubPicker } from "./lua-skill-hub-picker";
import { LuaSkillInstallDialog } from "./lua-skills-panel";
import type { SkillActionProps } from "./skill-area-types";

vi.mock("@/i18n", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/i18n")>()),
  useI18n: () => ({ locale: "en-US", t: (key: string) => key }),
}));
vi.mock("@/service/skills-actions-service", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/service/skills-actions-service")>()),
  searchLuaSkillsHub: vi.fn(),
  getLuaSkillsHubSkill: vi.fn(),
}));

// HUB_DEBOUNCE_WAIT_MS outlasts the picker's search debounce.
// HUB_DEBOUNCE_WAIT_MS 长于选择器的搜索防抖时长。
const HUB_DEBOUNCE_WAIT_MS = 400;

// settle waits past the search debounce inside act so the resulting render is committed.
// settle 在 act 内等待超过搜索防抖时长，使随后的渲染得到提交。
async function settle(): Promise<void> {
  await act(async () => { await new Promise((resolve) => setTimeout(resolve, HUB_DEBOUNCE_WAIT_MS)); });
}

// chooseSourceType opens the production Select by keyboard and picks the exact visible option.
// chooseSourceType 通过键盘打开生产 Select 并选择准确的可见选项。
async function chooseSourceType(option: string): Promise<void> {
  const trigger = document.querySelector<HTMLElement>('.lua-skill-install-field [role="combobox"]')!;
  await act(async () => { trigger.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowDown", bubbles: true })); });
  const entry = [...document.querySelectorAll<HTMLElement>('[role="option"]')].find((candidate) => candidate.textContent === option);
  if (!entry) throw new Error(`Expected source type ${option}; check the install dialog options`);
  await act(async () => { entry.dispatchEvent(new KeyboardEvent("keydown", { key: "Enter", bubbles: true })); });
}

// hubPage builds one search page with the given skill identifiers.
// hubPage 使用给定技能标识构建一页搜索结果。
function hubPage(skillIds: string[], nextCursor: string | null): LuaSkillsHubSearchPage {
  return {
    hubBaseUrl: "https://hub.example",
    nextCursor,
    items: skillIds.map((skillId) => ({
      skillId, publisher: "fixture", displayName: skillId, summary: "", repositoryUrl: "", homepageUrl: "",
      license: "MIT", tags: [], latestVersion: "1.0.0", updatedAt: "2026-10-01T00:00:00Z", content: null,
    })),
  };
}

it("keeps a failed install editable and blocks duplicate submits and dismissal during retry", async () => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  vi.mocked(searchLuaSkillsHub).mockResolvedValue(hubPage([], null));
  const host = document.createElement("div"); document.body.append(host);
  const root = createRoot(host);
  let complete!: () => void;
  const install = vi.fn().mockRejectedValueOnce(new Error("source unavailable"))
    .mockImplementation(() => new Promise<void>((resolve) => { complete = resolve; }));
  const close = vi.fn();
  try {
    await act(async () => root.render(<LuaSkillInstallDialog open actions={{ onInstallLuaSkill: install } as unknown as SkillActionProps} onOpenChange={close} />));
    await chooseSourceType("GitHub");
    const input = document.querySelector<HTMLInputElement>(".lua-skill-install-field input")!;
    await act(async () => {
      Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(input, "fixture/skill");
      input.dispatchEvent(new Event("input", { bubbles: true }));
    });
    const submit = document.querySelector<HTMLButtonElement>('button[type="submit"]')!;
    expect(submit.form).toBe(document.querySelector("form"));
    await act(async () => submit.click());
    expect(install).toHaveBeenCalledWith("", "fixture/skill", "github");
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
    await act(async () => root.unmount()); host.remove(); vi.unstubAllGlobals(); vi.mocked(searchLuaSkillsHub).mockReset();
  }
});

it("installs the selected Hub skill by identifier and pages through Hub results", async () => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  vi.mocked(searchLuaSkillsHub)
    .mockResolvedValueOnce(hubPage(["weather", "clock"], "2"))
    .mockResolvedValueOnce(hubPage(["notes"], null));
  const host = document.createElement("div"); document.body.append(host);
  const root = createRoot(host);
  const install = vi.fn().mockResolvedValue(undefined);
  const close = vi.fn();
  try {
    await act(async () => root.render(<LuaSkillInstallDialog open actions={{ onInstallLuaSkill: install } as unknown as SkillActionProps} onOpenChange={close} />));
    await settle();
    expect(searchLuaSkillsHub).toHaveBeenLastCalledWith("", null, "en-US");
    const results = () => [...document.querySelectorAll<HTMLButtonElement>(".lua-skill-hub-result")];
    expect(results().map((row) => row.getAttribute("aria-selected"))).toEqual(["false", "false"]);
    const submit = document.querySelector<HTMLButtonElement>('button[type="submit"]')!;
    expect(submit.disabled).toBe(true);

    const loadMore = [...document.querySelectorAll("button")].find((button) => button.textContent === "skills.lua.hubLoadMore")!;
    await act(async () => loadMore.click());
    expect(searchLuaSkillsHub).toHaveBeenLastCalledWith("", "2", "en-US");
    expect(results()).toHaveLength(3);

    const weather = results().find((row) => row.textContent?.includes("weather"))!;
    await act(async () => weather.click());
    expect(weather.getAttribute("aria-selected")).toBe("true");
    expect(submit.disabled).toBe(false);
    await act(async () => submit.click());
    expect(install).toHaveBeenCalledWith("", "weather", "official_hub");
    expect(close).toHaveBeenCalledWith(false);
  } finally {
    await act(async () => root.unmount()); host.remove(); vi.unstubAllGlobals(); vi.mocked(searchLuaSkillsHub).mockReset();
  }
});

// hubContent builds one content node in the requested Spanish.
// hubContent 构建一个请求西班牙语的内容节点。
function hubContent(kind: LuaSkillsHubContent["kind"], summary: string, readme: string | null): LuaSkillsHubContent {
  return {
    requestedLocale: "es-ES", locale: kind === "translation" ? "es-ES" : "en-US", kind, sourceLocale: "en-US",
    summary, intro: `${summary} intro`, features: [`${summary} feature`], readme,
  };
}

it("shows translated summaries and switches the detail drawer between translation and original", async () => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  const page = hubPage(["weather"], null);
  page.items[0]!.summary = "Forecasts";
  page.items[0]!.content = hubContent("translation", "Pronósticos", null);
  vi.mocked(searchLuaSkillsHub).mockResolvedValue(page);
  vi.mocked(getLuaSkillsHubSkill).mockResolvedValue({
    hubBaseUrl: "https://hub.example",
    skill: { ...page.items[0]!, content: hubContent("translation", "Pronósticos", "# Tiempo") },
    original: hubContent("original", "Forecasts", null),
    latest: { version: "1.0.0", readme: "# Weather", publishedAt: "2026-10-01T00:00:00Z" },
  });
  const host = document.createElement("div"); document.body.append(host);
  const root = createRoot(host);
  const select = vi.fn();
  try {
    await act(async () => root.render(<LuaSkillHubPicker disabled={false} selectedSkillId="weather" onSelect={select} />));
    await settle();
    const result = document.querySelector(".lua-skill-hub-result")!;
    expect(result.textContent).toContain("Pronósticos");
    expect(result.textContent).not.toContain("Forecasts");
    expect(result.textContent).toContain("skills.lua.hubTranslatedBadge");

    const button = (label: string) => [...document.querySelectorAll("button")].find((candidate) => candidate.textContent === label);
    await act(async () => button("skills.lua.hubDetails")!.click());
    await settle();
    expect(getLuaSkillsHubSkill).toHaveBeenCalledWith("weather", "en-US");
    expect(document.body.textContent).toContain("Pronósticos feature");
    expect(document.body.textContent).toContain("Tiempo");

    await act(async () => button("skills.lua.hubShowOriginal")!.click());
    await settle();
    expect(document.body.textContent).toContain("Forecasts feature");
    expect(document.body.textContent).toContain("Weather");
    expect(document.body.textContent).not.toContain("Tiempo");
    expect(button("skills.lua.hubShowTranslation")).toBeDefined();
  } finally {
    await act(async () => root.unmount()); host.remove(); vi.unstubAllGlobals();
    vi.mocked(searchLuaSkillsHub).mockReset(); vi.mocked(getLuaSkillsHubSkill).mockReset();
  }
});
