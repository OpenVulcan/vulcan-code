// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import type { ProjectSummary } from "@/types";

// The translator returns keys, so assertions name controls by catalog key instead of wording.
// 翻译函数返回键名，使断言以目录键而非文案定位控件。
vi.mock("@/i18n", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/i18n")>();
  const t = (key: string) => key;
  return { ...actual, useI18n: () => ({ locale: "en-US", setLocale: () => undefined, t }) };
});
vi.mock("@/ui-kit/primitives/toast", () => ({ toast: { success: vi.fn(), error: vi.fn() } }));

// Every service call is synthetic; nothing reaches a running service.
// 所有服务调用均为合成；不会访问运行中的服务。
const service = vi.hoisted(() => ({
  loadReviewRulesSystemDefault: vi.fn(),
  saveReviewRulesSystemDefault: vi.fn(),
  loadProjectReviewRules: vi.fn(),
  saveProjectReviewRules: vi.fn(),
}));
vi.mock("@/service/permission-judge-service", () => service);
// The page also hosts the review prompt section, whose service is synthetic as well.
// 页面同时包含复核提示词区块，其服务同样为合成。
vi.mock("@/service/review-prompt-service", () => ({
  loadReviewPromptSystemDefault: vi.fn().mockResolvedValue({ prompt: "", maxChars: 4000 }),
  saveReviewPromptSystemDefault: vi.fn(),
}));

const { PermissionJudgeReviewRulesPage } = await import("./permission-judge-review-rules-page");

let host: HTMLDivElement;
let root: Root;
const projects = [{ id: "p1", name: "One" }] as unknown as ProjectSummary[];
const systemRules = { allow: "reading logs", ask: "", deny: "force push" };

beforeEach(() => {
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  service.loadReviewRulesSystemDefault.mockResolvedValue({ rules: systemRules, maxChars: 4000 });
  service.loadProjectReviewRules.mockResolvedValue({
    projectId: "p1", rules: null, systemRules, maxChars: 4000,
  });
});
afterEach(async () => {
  await act(async () => root.unmount());
  host.remove();
  vi.unstubAllGlobals();
  vi.clearAllMocks();
});

// typeInto changes a React-controlled textarea the way a user edit does.
// typeInto 以用户编辑的方式修改受 React 控制的文本框。
function typeInto(element: HTMLTextAreaElement, value: string) {
  Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value")!.set!.call(element, value);
  element.dispatchEvent(new Event("input", { bubbles: true }));
}

// Verifies the system rules save all three trimmed groups only after one changes.
// 验证系统规则仅在某一组修改后保存全部三组去除首尾空白的文本。
it("saves the edited system rules", async () => {
  service.saveReviewRulesSystemDefault.mockResolvedValue({
    rules: { ...systemRules, ask: "installing packages" }, maxChars: 4000,
  });
  await act(async () => root.render(<PermissionJudgeReviewRulesPage projects={projects} />));
  const save = host.querySelector<HTMLButtonElement>("[data-review-rules-system-save]")!;
  expect(save.disabled).toBe(true);
  const deny = host.querySelector<HTMLTextAreaElement>('[data-review-rules-group="system-deny"]')!;
  expect(deny.value).toBe("force push");
  const ask = host.querySelector<HTMLTextAreaElement>('[data-review-rules-group="system-ask"]')!;
  await act(async () => typeInto(ask, "  installing packages  "));
  expect(save.disabled).toBe(false);
  await act(async () => save.click());
  expect(service.saveReviewRulesSystemDefault).toHaveBeenCalledExactlyOnceWith({
    allow: "reading logs", ask: "installing packages", deny: "force push",
  });
});

// Verifies a project follows the system rules until it keeps its own, starting from a copy of them.
// 验证项目在保留自有规则之前跟随系统规则，开启时以系统规则的副本开始。
it("lets a project replace the system rules", async () => {
  service.saveProjectReviewRules.mockResolvedValue({
    projectId: "p1", rules: systemRules, systemRules, maxChars: 4000,
  });
  await act(async () => root.render(<PermissionJudgeReviewRulesPage projects={projects} />));
  const section = host.querySelector<HTMLElement>("[data-review-rules-project]")!;
  expect(service.loadProjectReviewRules).toHaveBeenCalledWith("p1");
  expect(section.querySelector("[data-review-rules-follows]")).not.toBeNull();
  expect(section.querySelector("textarea")).toBeNull();
  await act(async () => section.querySelector<HTMLButtonElement>('[role="switch"]')!.click());
  expect(service.saveProjectReviewRules).toHaveBeenCalledExactlyOnceWith("p1", systemRules);
  expect(section.querySelector<HTMLTextAreaElement>('[data-review-rules-group="project-allow"]')?.value).toBe("reading logs");
  expect(section.querySelector("[data-review-rules-follows]")).toBeNull();
});

// Verifies turning project rules off saves null so the project follows the system rules again.
// 验证关闭项目规则时保存 null，使项目重新跟随系统规则。
it("returns a project to the system rules", async () => {
  service.loadProjectReviewRules.mockResolvedValue({
    projectId: "p1", rules: { allow: "", ask: "", deny: "anything" }, systemRules, maxChars: 4000,
  });
  service.saveProjectReviewRules.mockResolvedValue({ projectId: "p1", rules: null, systemRules, maxChars: 4000 });
  await act(async () => root.render(<PermissionJudgeReviewRulesPage projects={projects} />));
  const section = host.querySelector<HTMLElement>("[data-review-rules-project]")!;
  await act(async () => section.querySelector<HTMLButtonElement>('[role="switch"]')!.click());
  expect(service.saveProjectReviewRules).toHaveBeenCalledExactlyOnceWith("p1", null);
  expect(section.querySelector("[data-review-rules-follows]")).not.toBeNull();
});
