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
  loadUnattendedSystemDefault: vi.fn(),
  saveUnattendedSystemDefault: vi.fn(),
  loadProjectUnattendedDefault: vi.fn(),
  saveProjectUnattendedDefault: vi.fn(),
}));
vi.mock("@/service/permission-judge-service", () => service);

const { PermissionJudgeUnattendedPage } = await import("./permission-judge-unattended-page");

let host: HTMLDivElement;
let root: Root;
const projects = [{ id: "p1", name: "One" }] as unknown as ProjectSummary[];

beforeEach(() => {
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  service.loadUnattendedSystemDefault.mockResolvedValue({ denyPolicy: "no push", maxChars: 4000 });
  service.loadProjectUnattendedDefault.mockResolvedValue({
    projectId: "p1", denyPolicy: null, systemDenyPolicy: "no push", maxChars: 4000,
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

// Verifies the system default saves the trimmed text only after it changes.
// 验证系统默认值仅在修改后保存去除首尾空白的文本。
it("saves the edited system default", async () => {
  service.saveUnattendedSystemDefault.mockResolvedValue({ denyPolicy: "no reset", maxChars: 4000 });
  await act(async () => root.render(<PermissionJudgeUnattendedPage projects={projects} />));
  const save = host.querySelector<HTMLButtonElement>("[data-unattended-system-save]")!;
  expect(save.disabled).toBe(true);
  const area = host.querySelector<HTMLTextAreaElement>("[data-unattended-system] textarea")!;
  expect(area.value).toBe("no push");
  await act(async () => typeInto(area, "  no reset  "));
  expect(save.disabled).toBe(false);
  await act(async () => save.click());
  expect(service.saveUnattendedSystemDefault).toHaveBeenCalledExactlyOnceWith("no reset");
});

// Verifies a project follows the system default until it turns on its own policy.
// 验证项目在开启自有策略之前跟随系统默认值。
it("lets a project replace the system default", async () => {
  service.saveProjectUnattendedDefault.mockResolvedValue({
    projectId: "p1", denyPolicy: "no push", systemDenyPolicy: "no push", maxChars: 4000,
  });
  await act(async () => root.render(<PermissionJudgeUnattendedPage projects={projects} />));
  const section = host.querySelector<HTMLElement>("[data-unattended-project]")!;
  expect(service.loadProjectUnattendedDefault).toHaveBeenCalledWith("p1");
  expect(section.querySelector("[data-unattended-follows]")).not.toBeNull();
  expect(section.querySelector("textarea")).toBeNull();
  await act(async () => section.querySelector<HTMLButtonElement>('[role="switch"]')!.click());
  expect(service.saveProjectUnattendedDefault).toHaveBeenCalledExactlyOnceWith("p1", "no push");
  expect(section.querySelector<HTMLTextAreaElement>("textarea")?.value).toBe("no push");
  expect(section.querySelector("[data-unattended-follows]")).toBeNull();
});
