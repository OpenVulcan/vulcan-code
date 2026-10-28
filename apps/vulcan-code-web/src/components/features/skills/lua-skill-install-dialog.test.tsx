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

// HUB_SETTLE_WAIT_MS outlasts the deferred first load, pending answers and any typing debounce a regression could add.
// HUB_SETTLE_WAIT_MS 长于推迟的首次载入、待处理的响应，以及回归可能加入的任何输入防抖。
const HUB_SETTLE_WAIT_MS = 400;

// settle waits inside act so pending loads resolve and the resulting render is committed.
// settle 在 act 内等待，使待处理的载入完成并提交随后的渲染。
async function settle(): Promise<void> {
  await act(async () => { await new Promise((resolve) => setTimeout(resolve, HUB_SETTLE_WAIT_MS)); });
}

// typeInto sets an input's value the way React observes a user edit.
// typeInto 以 React 能观察到用户编辑的方式设置输入框的值。
async function typeInto(input: HTMLInputElement, value: string): Promise<void> {
  await act(async () => {
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(input, value);
    input.dispatchEvent(new Event("input", { bubbles: true }));
  });
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

it("searches the Hub only when Enter is pressed and never submits the install form", async () => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  vi.mocked(searchLuaSkillsHub).mockResolvedValue(hubPage(["weather"], "2"));
  const host = document.createElement("div"); document.body.append(host);
  const root = createRoot(host);
  const install = vi.fn().mockResolvedValue(undefined);
  try {
    await act(async () => root.render(<LuaSkillInstallDialog open actions={{ onInstallLuaSkill: install } as unknown as SkillActionProps} onOpenChange={vi.fn()} />));
    await settle();
    expect(searchLuaSkillsHub).toHaveBeenCalledTimes(1);
    expect(searchLuaSkillsHub).toHaveBeenLastCalledWith("", null, "en-US");
    const weather = document.querySelector<HTMLButtonElement>(".lua-skill-hub-result")!;
    await act(async () => weather.click());

    const input = document.querySelector<HTMLInputElement>(".lua-skill-hub-search input")!;
    await typeInto(input, "clo");
    await typeInto(input, "clock");
    await settle();
    expect(searchLuaSkillsHub).toHaveBeenCalledTimes(1);

    const enter = new KeyboardEvent("keydown", { key: "Enter", bubbles: true, cancelable: true });
    await act(async () => { input.dispatchEvent(enter); });
    await settle();
    expect(enter.defaultPrevented).toBe(true);
    expect(searchLuaSkillsHub).toHaveBeenCalledTimes(2);
    expect(searchLuaSkillsHub).toHaveBeenLastCalledWith("clock", null, "en-US");
    expect(install).not.toHaveBeenCalled();

    // Further typing does not move "Load more" off the submitted query.
    // 继续输入不会让“加载更多”偏离已提交的查询。
    await typeInto(input, "clockwork");
    const loadMore = [...document.querySelectorAll("button")].find((button) => button.textContent === "skills.lua.hubLoadMore")!;
    await act(async () => loadMore.click());
    expect(searchLuaSkillsHub).toHaveBeenCalledTimes(3);
    expect(searchLuaSkillsHub).toHaveBeenLastCalledWith("clock", "2", "en-US");
  } finally {
    await act(async () => root.unmount()); host.remove(); vi.unstubAllGlobals(); vi.mocked(searchLuaSkillsHub).mockReset();
  }
});

it("shows the refined intro as a tooltip on focus only for results that have one", async () => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  const page = hubPage(["weather", "clock"], null);
  page.items[0]!.content = { ...hubContent("original", "Forecasts", null), intro: "fixture-intro-marker" };
  page.items[1]!.content = { ...hubContent("original", "Time", null), intro: "" };
  vi.mocked(searchLuaSkillsHub).mockResolvedValue(page);
  const host = document.createElement("div"); document.body.append(host);
  const root = createRoot(host);
  try {
    await act(async () => root.render(<LuaSkillHubPicker disabled={false} selectedSkillId="" onSelect={vi.fn()} />));
    await settle();
    const row = (skillId: string) => [...document.querySelectorAll<HTMLButtonElement>(".lua-skill-hub-result")]
      .find((candidate) => candidate.textContent?.includes(skillId))!;
    const tooltip = () => document.querySelector('[role="tooltip"]');

    expect(row("weather").getAttribute("data-slot")).toBe("tooltip-trigger");
    expect(row("clock").hasAttribute("data-slot")).toBe(false);
    expect(row("weather").textContent).not.toContain("fixture-intro-marker");

    await act(async () => row("weather").focus());
    expect(tooltip()?.textContent).toBe("fixture-intro-marker");
    expect(row("weather").getAttribute("aria-describedby")).toBe(tooltip()?.id);

    await act(async () => row("clock").focus());
    expect(tooltip()).toBeNull();
  } finally {
    await act(async () => root.unmount()); host.remove(); vi.unstubAllGlobals(); vi.mocked(searchLuaSkillsHub).mockReset();
  }
});
