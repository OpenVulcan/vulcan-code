// @vitest-environment jsdom
import React, { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import type { LuaSkillSummary, LuaSkillsOperationProgressSummary } from "@/types";
import type { SkillActionProps } from "./skill-area-types";
import { LuaSkillsPanel } from "./lua-skills-panel";

vi.mock("@/i18n", () => ({ useI18n: () => ({ locale: "zh-CN", t: (key: string) => key }) }));

// host owns the panel while the shared modal mounts into its document portal.
// host 持有面板，共享弹窗挂载到文档门户。
let host: HTMLDivElement;
// root controls the React lifecycle for each isolated interaction test.
// root 控制每项隔离交互测试的 React 生命周期。
let root: Root;
// update is the deferred service action triggered by the real package button.
// update 是真实技能包按钮触发的延迟服务动作。
let update: ReturnType<typeof vi.fn<(skillId: string) => Promise<void>>>;
// complete releases the current update RPC without fabricating a service event.
// complete 结束当前更新 RPC，不伪造服务事件。
let complete: () => void;
// reject fails the current update RPC independently of progress delivery.
// reject 使当前更新 RPC 失败，与进度投递相互独立。
let reject: (error: Error) => void;
// actions keeps the panel transport identity stable across progress renders.
// actions 在进度重渲染期间保持面板传输身份稳定。
let actions: SkillActionProps;

// managedSkill declares the package fields read by the actual list and update button.
// managedSkill 声明真实列表与更新按钮读取的技能包字段。
const managedSkill: LuaSkillSummary = {
  skillId: "vulcan-file", displayName: "vulcan-file", version: "1.0.0", rootName: "user",
  enabled: true, tools: [], entryCount: 0, managed: true, systemOwned: false, skillDir: "D:/fixture/vulcan-file",
  sourceType: "github", sourceLocator: "LuaSkills/vulcan-file",
};

beforeEach(() => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
  update = vi.fn(() => new Promise<void>((resolve, rejectRequest) => { complete = resolve; reject = rejectRequest; }));
  actions = { onUpdateLuaSkill: update, onPollLuaSkillConfigEvents: () => new Promise(() => {}) } as unknown as SkillActionProps;
});

afterEach(async () => {
  await act(async () => root.unmount());
  host.remove();
  vi.unstubAllGlobals();
});

// progress builds a live update snapshot with explicit per-test protocol overrides.
// progress 使用每项测试显式指定的协议覆盖值构建实时更新快照。
// Parameters: overrides replaces selected event fields. Returns: a complete service progress summary.
// 参数：overrides 替换选中的事件字段。返回值：完整服务进度摘要。
function progress(overrides: Partial<LuaSkillsOperationProgressSummary> = {}): LuaSkillsOperationProgressSummary {
  return { correlationId: "update-1", action: "update", inFlight: true, outcome: null,
    overflowed: false, sawUpstreamTerminal: false, phases: 1, skillId: "vulcan-file", ...overrides };
}

// renderPanel delivers a service snapshot to the actual page while preserving local request state.
// renderPanel 向真实页面投递服务快照，同时保留本地请求状态。
// Parameters: operationProgress is the current service operation or null. Returns: the completed React render.
// 参数：operationProgress 是当前服务操作或 null。返回值：完成的 React 渲染。
async function renderPanel(operationProgress: LuaSkillsOperationProgressSummary | null) {
  await act(async () => root.render(<LuaSkillsPanel actions={actions} skills={[managedSkill]} operationProgress={operationProgress} />));
}

// updateButton locates the real package update control for click and busy-state assertions.
// updateButton 定位真实技能包更新控件，用于点击与忙碌状态断言。
// Returns: the required button; a missing control fails the test immediately.
// 返回值：必需的按钮；控件缺失时立即使测试失败。
function updateButton(): HTMLButtonElement {
  // button belongs to the package row, outside the progress portal.
  // button 属于技能包行，位于进度门户之外。
  const button = host.querySelector<HTMLButtonElement>('button[aria-label="skills.action.update"]');
  if (!button) throw new Error("Missing skill update button");
  return button;
}

// dialog locates the visible modal without treating page-level text as progress feedback.
// dialog 定位可见弹窗，不把页面级文字当作进度反馈。
// Returns: the required dialog element; absence fails the test.
// 返回值：必需的弹窗元素；不存在时使测试失败。
function dialog(): Element {
  // modal is the shared Radix dialog rendered through the document portal.
  // modal 是通过文档门户渲染的共享 Radix 弹窗。
  const modal = document.querySelector('[role="dialog"]');
  if (!modal) throw new Error("Missing update progress dialog");
  return modal;
}

it("opens on click before events, ignores previous results, blocks duplicate clicks, and retains completion", async () => {
  // previous models the old operation retained by the initial catalog snapshot.
  // previous 模拟初始目录快照保留的旧操作。
  const previous = progress({ correlationId: "old-update", inFlight: false, outcome: "succeeded", percent: 100 });
  await renderPanel(previous);
  expect(document.querySelector('[role="dialog"]')).toBeNull();
  await act(async () => { updateButton().click(); updateButton().click(); });
  expect(update).toHaveBeenCalledExactlyOnceWith("vulcan-file");
  expect(dialog().textContent).toContain("正在准备更新");
  expect(dialog().textContent).toContain("vulcan-file");
  expect(dialog().querySelector('[role="progressbar"]')?.hasAttribute("aria-valuenow")).toBe(false);
  expect(dialog().textContent).not.toContain("100%");
  expect(host.textContent).not.toContain("正在更新技能包");
  await act(async () => document.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true })));
  expect(dialog().textContent).toContain("正在更新技能包");
  await renderPanel(progress({ latestPhase: "downloading_archive", percent: 42, bytesDone: 420, bytesTotal: 1000 }));
  expect(dialog().textContent).toContain("正在下载技能包");
  expect(dialog().textContent).toContain("42%");
  expect(dialog().textContent).toContain("420 / 1,000 字节");
  expect(dialog().querySelector('[role="progressbar"]')?.getAttribute("aria-valuenow")).toBe("42");
  await renderPanel(progress({ latestPhase: "extracting_archive" }));
  expect(dialog().textContent).toContain("正在解压技能包");
  expect(dialog().textContent).not.toContain("42%");
  await renderPanel(progress({ inFlight: false, outcome: "succeeded", sawUpstreamTerminal: true, bytesDone: 420, bytesTotal: 1000 }));
  expect(dialog().textContent).toContain("正在更新技能包");
  await act(async () => complete());
  expect(dialog().textContent).toContain("技能包更新完成");
  expect(dialog().textContent).not.toContain("420 / 1,000 字节");
  expect(dialog().querySelector('[role="progressbar"]')?.getAttribute("aria-valuenow")).toBe("100");
  // close dismisses only the visible completed operation.
  // close 只关闭当前可见的已完成操作。
  const close = [...dialog().querySelectorAll("button")].find((button) => button.textContent === "关闭");
  if (!close) throw new Error("Missing completion close button");
  await act(async () => close.click());
  await renderPanel(progress({ inFlight: false, outcome: "succeeded" }));
  expect(document.querySelector('[role="dialog"]')).toBeNull();
  await act(async () => updateButton().click());
  expect(dialog().textContent).toContain("正在准备更新");
  expect(dialog().textContent).not.toContain("100%");
  await act(async () => complete());
});

it.each([0, 100])("shows measured %s percent without treating a running download as update completion", async (percent) => {
  await renderPanel(progress({ latestPhase: "downloading_archive", percent }));
  expect(dialog().querySelector('[role="progressbar"]')?.getAttribute("aria-valuenow")).toBe(String(percent));
  expect(dialog().textContent).toContain(`${percent}%`);
  expect(dialog().textContent).not.toContain("技能包更新完成");
  expect(updateButton().disabled).toBe(true);
});

it("keeps source resolution and downloads without totals indeterminate", async () => {
  await renderPanel(progress({ latestPhase: "resolving_source" }));
  expect(dialog().textContent).toContain("正在解析更新来源");
  expect(dialog().querySelector('[role="progressbar"]')?.hasAttribute("aria-valuenow")).toBe(false);
  await renderPanel(progress({ latestPhase: "downloading_archive", bytesDone: 1024, bytesTotal: null, percent: null }));
  expect(dialog().textContent).toContain("1,024 字节");
  expect(dialog().textContent).not.toContain("%");
});

it("shows service failure and dropped phase feedback inside the modal", async () => {
  await renderPanel(progress());
  await renderPanel(progress({ inFlight: false, outcome: "failed", message: "Skill package operation failed.", overflowed: true }));
  expect(dialog().textContent).toContain("技能包更新失败");
  expect(dialog().textContent).toContain("Skill package operation failed.");
  expect(dialog().textContent).toContain("部分阶段记录已被丢弃");
  expect(host.textContent).not.toContain("Skill package operation failed.");
  expect(dialog().querySelector('[role="alert"]')).not.toBeNull();
  expect(updateButton().disabled).toBe(false);
});

it("keeps a request failure visible in the modal even when no service event arrives", async () => {
  await renderPanel(null);
  await act(async () => updateButton().click());
  await act(async () => reject(new Error("source unavailable")));
  expect(dialog().textContent).toContain("技能包更新失败");
  expect(dialog().querySelector("details")?.textContent).toContain("source unavailable");
  expect(host.querySelector('[role="alert"]')).toBeNull();
  expect(updateButton().disabled).toBe(false);
});

it("does not attach another package's measured progress to the clicked update", async () => {
  await renderPanel(null);
  await act(async () => updateButton().click());
  await renderPanel(progress({ skillId: "another-skill", latestPhase: "downloading_archive", percent: 90 }));
  expect(dialog().textContent).toContain("vulcan-file");
  expect(dialog().textContent).not.toContain("90%");
  await renderPanel(progress({ correlationId: "update-2", percent: 25 }));
  expect(dialog().textContent).toContain("25%");
  await renderPanel(progress({ correlationId: "update-2", inFlight: false, outcome: "succeeded" }));
  await act(async () => complete());
});
