// @vitest-environment jsdom
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import type { PluginEnvironmentInstallTaskStatus } from "@/service/skills-actions-service";
import type { PluginSummary } from "@/types";
import type { SkillActionProps } from "./skill-area-types";
import { usePluginEnvironmentTasks } from "./use-plugin-environment-tasks";

// running is an unfinished Core task that stays unfinished for the whole test.
// running 是在整个测试期间保持未完成状态的 Core 任务。
const running: PluginEnvironmentInstallTaskStatus = {
  taskId: "task-1", pluginId: "plugin-a", generation: "g1", state: "running", stage: "preparing",
};
// plugins and actions keep stable identities so only the hook's own state can retrigger effects.
// plugins 与 actions 保持稳定身份，只有钩子自身状态能重新触发副作用。
const plugins = [{ id: "plugin-a" }] as unknown as PluginSummary[];
// installStatus stops answering after a bound so a self-retriggering poll fails by count, not by hanging.
// installStatus 超过上限后不再应答，使自我重触发的轮询以次数失败而不是挂起。
const answerRunning = (): Promise<PluginEnvironmentInstallTaskStatus> =>
  installStatus.mock.calls.length > 10 ? new Promise(() => {}) : Promise.resolve({ ...running });
const installStatus = vi.fn(answerRunning);
// ready reports the environment ready only after Core has been asked for the task status.
// ready 仅在已向 Core 查询任务状态后报告环境就绪。
const ready = vi.fn(async () => installStatus.mock.calls.length > 0);
const actions = {
  onPluginEnvironmentReady: ready,
  onPluginEnvironmentInstallStatus: installStatus,
  onPluginEnvironmentInstallStatusForPlugin: vi.fn(async () => ({ ...running })),
} as unknown as SkillActionProps;
const onError = vi.fn();
let host: HTMLDivElement;
let root: Root;
let state: ReturnType<typeof usePluginEnvironmentTasks>;

// Probe mounts the production hook; returns no DOM.
// Probe 挂载生产钩子；不返回 DOM。
function Probe() { state = usePluginEnvironmentTasks(actions, plugins, onError); return null; }

beforeEach(() => {
  vi.useFakeTimers();
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  installStatus.mockReset(); installStatus.mockImplementation(answerRunning);
  host = document.createElement("div"); document.body.append(host); root = createRoot(host);
});
afterEach(async () => {
  await act(async () => root.unmount());
  host.remove(); vi.unstubAllGlobals(); vi.useRealTimers();
});

// Each status response must wait for the poll interval instead of immediately starting the next request.
// 每次状态响应都必须等待轮询间隔，而不是立即发起下一次请求。
it("polls an unfinished install task once per interval", async () => {
  await act(async () => root.render(<Probe />));
  // Flush many promise turns without advancing the clock; a self-retriggering poll would keep requesting.
  // 不推进时钟而冲刷多轮 Promise；自我重触发的轮询会持续发起请求。
  for (let turn = 0; turn < 20; turn += 1) await act(async () => { await Promise.resolve(); });
  expect(installStatus).toHaveBeenCalledTimes(1);
  await act(async () => { await vi.advanceTimersByTimeAsync(1000); });
  expect(installStatus).toHaveBeenCalledTimes(2);
});

// A task finishing between polls must publish its readiness without waiting for the slower catalog refresh.
// 在两次轮询之间完成的任务必须发布其就绪状态，而不等待较慢的目录刷新。
it("publishes readiness when a polled task succeeds", async () => {
  installStatus.mockImplementation(async () => ({ ...running, state: "succeeded", stage: "completed" }));
  await act(async () => root.render(<Probe />));
  for (let turn = 0; turn < 5; turn += 1) await act(async () => { await Promise.resolve(); });
  expect(state.installTasksByPlugin["plugin-a"]?.state).toBe("succeeded");
  expect(state.environmentReadyByPlugin["plugin-a"]).toBe(true);
});
