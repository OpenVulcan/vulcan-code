// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import type { TranslationKey } from "@/i18n";
import type { JudgeBackend, ProjectPermissionJudgePolicySnapshot, ProjectSummary, WorkbenchSnapshot } from "@/types";
import { projectSettingsSections } from "./project-settings-utils";

// The translator returns keys, so assertions name text by catalog key instead of wording.
// 翻译函数返回键名，使断言以目录键而非文案定位文本。
vi.mock("@/i18n", async (importOriginal) => {
  const actual = await importOriginal<typeof import("@/i18n")>();
  const t = (key: string) => key;
  return { ...actual, useI18n: () => ({ locale: "en-US", setLocale: () => undefined, t }) };
});

// Every service call is synthetic; nothing reaches a running service.
// 所有服务调用均为合成；不会访问运行中的服务。
const service = vi.hoisted(() => ({
  loadProjectPermissionJudgePolicy: vi.fn(),
  saveProjectPermissionJudgePolicy: vi.fn(),
}));
vi.mock("@/service/permission-judge-service", () => service);

const { FOLLOW_DEFAULT, ProjectPermissionJudgeSection, nextPolicyOptions } = await import("./project-permission-judge-section");

// policy builds a project's choices against a model with two categories.
// policy 构建项目针对含两个类别之模型的选择。
function policy(chosen: Record<string, string | null>, unsupported: Record<string, string> = {}): ProjectPermissionJudgePolicySnapshot {
  return {
    projectId: "project-a",
    options: [
      { key: "cross_project_writes", defaultMode: "allow", modes: ["allow", "review"], chosenMode: chosen.cross_project_writes ?? null },
      { key: "database_reset", defaultMode: "confirm", modes: ["confirm", "allow"], chosenMode: chosen.database_reset ?? null },
    ],
    unsupportedOptions: unsupported,
    issue: null,
  };
}

let host: HTMLDivElement;
let root: Root;

beforeEach(() => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
});
afterEach(async () => {
  await act(async () => root.unmount());
  host.remove();
  vi.unstubAllGlobals();
  vi.clearAllMocks();
});

// The project navigation offers the permission strategy only while a judge is configured.
// 仅在判定器已配置时，项目导航才提供权限策略。
it("lists the permission strategy only while a judge is configured", () => {
  const project = { id: "project-a", pinned: false } as ProjectSummary;
  const has = (backend: JudgeBackend | null) => projectSettingsSections(
    project,
    { conversations: [], agentSkills: { records: [] }, permissionJudgeBackend: backend } as unknown as WorkbenchSnapshot,
    (key: TranslationKey) => key,
    null,
  ).some((section) => section.id === "permission-judge");
  expect(has("local")).toBe(true);
  expect(has("cloud")).toBe(true);
  expect(has(null)).toBe(false);
});

// A change keeps the other current choices; following the default removes the choice.
// 修改保留其他当前选择；跟随默认值会移除该选择。
it("saves the complete choices after one change", () => {
  const snapshot = policy({ cross_project_writes: "review" });
  expect(nextPolicyOptions(snapshot, "database_reset", "allow")).toEqual({ cross_project_writes: "review", database_reset: "allow" });
  expect(nextPolicyOptions(snapshot, "cross_project_writes", FOLLOW_DEFAULT)).toEqual({});
});

// Only the categories of the model in use get a control; kept choices are noted, and opening saves nothing.
// 只有使用中模型的类别有控件；保留的选择会被注明，打开时不保存任何内容。
it("shows only the categories of the model in use", async () => {
  service.loadProjectPermissionJudgePolicy.mockResolvedValue(policy({}, { retired: "allow" }));
  await act(async () => root.render(<ProjectPermissionJudgeSection projectId="project-a" />));
  expect(service.loadProjectPermissionJudgePolicy).toHaveBeenCalledExactlyOnceWith("project-a");
  const keys = Array.from(host.querySelectorAll<HTMLElement>("[data-judge-option]")).map((node) => node.dataset.judgeOption);
  expect(keys).toEqual(["cross_project_writes", "database_reset"]);
  expect(host.textContent).toContain("projectSettings.permissionJudge.unsupported");
  expect(service.saveProjectPermissionJudgePolicy).not.toHaveBeenCalled();
});
