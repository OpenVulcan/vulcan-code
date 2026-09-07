// @vitest-environment jsdom
import React, { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { renderToStaticMarkup } from "react-dom/server";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { VceJobSummary } from "@/types";
import { VceJobObserverProvider } from "./vce-job-observer-context";
import { useVceProjectJob } from "./use-vce-project-job";
import { useVceActionScope } from "./use-vce-action-scope";

// Substitute only service IO; real React subscriptions, lifetimes and command capture stay intact.
// 仅替换服务输入输出；真实 React 订阅、生命周期和命令捕获保持完整。
const rpc = vi.hoisted(() => ({ load: vi.fn(), subscribe: vi.fn(), cancel: vi.fn(), stop: vi.fn() }));
vi.mock("@/service/vce-service", () => ({ loadVceJob: rpc.load, subscribeVceJobChanged: rpc.subscribe, cancelVceJob: rpc.cancel }));

// Keep isolated DOM ownership, observations and child mounting counts for each test.
// 为每项测试保留独立文档归属、观察和子节点挂载次数。
let host: HTMLDivElement;
let root: Root;
let views: Record<string, ReturnType<typeof useVceProjectJob>>;
// Capture independent settings/maintenance ownership under the same service composition.
// 在同一服务组合下捕获独立设置及维护归属。
let actions: Record<string, () => boolean>;
let childMounts: number;
let childUnmounts: number;

// Construct a complete receipt for projectId with explicit overrides; returns independent metadata.
// 使用明确 overrides 为 projectId 构建完整回执；返回独立元数据。
function receipt(projectId = "project-a", overrides: Partial<VceJobSummary> = {}): VceJobSummary {
  return { projectId, jobId: "job-1", kind: "synchronize", state: "running", sequence: 2,
    createdAtUnixMs: 1000, updatedAtUnixMs: 2000, sourceCheckpointVersion: 1,
    resultCheckpointVersion: null, progress: null, followupRequested: false, error: null, ...overrides };
}

// Observe projectId using the real hook under name; returns its visible exact job sequence.
// 在 name 下使用真实钩子观察 projectId；返回其可见精确作业序号。
function Probe(props: { name: string; projectId: string | null }) {
  // job is captured during each committed test render for explicit UI actions.
  // job 在每次测试渲染期间捕获，用于明确界面操作。
  const job = useVceProjectJob(props.projectId);
  // The production action guard shares service identity but never creates another subscription.
  // 生产操作保护共享服务身份，但绝不创建另一份订阅。
  const isCurrentAction = useVceActionScope(props.projectId);
  actions[props.name] = isCurrentAction;
  views[props.name] = job;
  return <output data-view={props.name}>{job.job?.sequence ?? "empty"}</output>;
}

// Model a stateful conversation child; takes no arguments and returns no DOM.
// 模拟有状态会话子节点；无参数，不返回文档节点。
// Mount counters detect accidentally keying the entire chat tree to service restarts.
// 挂载计数检测错误地将整棵聊天树身份绑定到服务重启。
function ConversationProbe() {
  React.useEffect(() => { childMounts += 1; return () => { childUnmounts += 1; }; }, []);
  return null;
}

// Compose a real workbench provider with named views; returns stable children across host epochs.
// 使用命名视图组合真实工作台提供者；返回跨宿主代次保持稳定的子节点。
// epoch is existing workbench generation; projects maps each exact view to its current project.
// epoch 为既有工作台代次；projects 将每个精确视图映射到其当前项目。
function Harness(props: { epoch: number; projects: Record<string, string | null> }) {
  return <VceJobObserverProvider serviceEpoch={props.epoch}>
    <ConversationProbe />
    {Object.entries(props.projects).map(([name, projectId]) => <Probe key={name} name={name} projectId={projectId} />)}
  </VceJobObserverProvider>;
}

beforeEach(() => {
  vi.useFakeTimers(); vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true); vi.resetAllMocks();
  rpc.load.mockResolvedValue(null); rpc.subscribe.mockResolvedValue(rpc.stop);
  views = {}; actions = {}; childMounts = 0; childUnmounts = 0;
  host = document.createElement("div"); document.body.append(host); root = createRoot(host);
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); vi.useRealTimers(); vi.unstubAllGlobals(); });

describe("VCE workbench observer composition", () => {
  // Real React views share active polling and cancellation state without resetting the chat.
  // 真实 React 视图共享活动轮询和取消状态，不重置聊天。
  it("shares one observation between floating and project-setting views", async () => {
    rpc.load.mockResolvedValue(receipt());
    await act(async () => root.render(<Harness epoch={1} projects={{ floating: "project-a", settings: "project-a" }} />));
    expect(rpc.load).toHaveBeenCalledTimes(1); expect(rpc.subscribe).toHaveBeenCalledTimes(1); expect(vi.getTimerCount()).toBe(1);
    expect(views.floating.job).toBe(views.settings.job);
    let finish!: (value: VceJobSummary) => void;
    rpc.cancel.mockImplementation(() => new Promise<VceJobSummary>((resolve) => { finish = resolve; }));
    let first!: Promise<void>; let second!: Promise<void>;
    await act(async () => { first = views.floating.cancel(); second = views.settings.cancel(); });
    expect(rpc.cancel).toHaveBeenCalledTimes(1); expect(views.floating.cancelPending).toBe(true); expect(views.settings.cancelPending).toBe(true);
    await act(async () => { finish(receipt("project-a", { state: "cancelled", sequence: 3 })); await Promise.all([first, second]); });
    expect(views.floating.active).toBe(false); expect(views.settings.cancelPending).toBe(false);
    await act(async () => root.render(<Harness epoch={1} projects={{ floating: "project-a" }} />));
    expect(rpc.stop).not.toHaveBeenCalled(); expect(childMounts).toBe(1);
    await act(async () => vi.advanceTimersByTimeAsync(5000)); expect(rpc.load).toHaveBeenCalledTimes(2);
    await act(async () => root.render(<Harness epoch={1} projects={{}} />));
    expect(rpc.stop).toHaveBeenCalledTimes(1); expect(vi.getTimerCount()).toBe(0); expect(rpc.cancel).toHaveBeenCalledTimes(1);
  });

  // Reconnection replaces the store but retains the conversation child's React identity.
  // 重连替换仓库，但保留会话子节点的 React 身份。
  it("resets old observations on the workbench epoch without remounting conversation children", async () => {
    rpc.load.mockResolvedValueOnce(receipt("project-a", { sequence: 80 }));
    await act(async () => root.render(<Harness epoch={1} projects={{ floating: "project-a" }} />));
    const oldEvents = rpc.subscribe.mock.calls[0][0]; const retired = views.floating;
    // Retain the exact pre-restart settings action; project ID alone must not reactivate it.
    // 保留重启前的精确设置操作；单靠项目编号不得重新激活它。
    const retiredAction = actions.floating;
    expect(retiredAction()).toBe(true);
    rpc.load.mockResolvedValueOnce(receipt("project-a", { sequence: 1 }));
    await act(async () => root.render(<Harness epoch={2} projects={{ floating: "project-a" }} />));
    expect(views.floating.job?.sequence).toBe(1); expect(rpc.stop).toHaveBeenCalledTimes(1);
    await act(async () => { oldEvents(receipt("project-a", { sequence: 100 })); retired.receive(receipt("project-a", { sequence: 101 })); await retired.cancel(); });
    expect(views.floating.job?.sequence).toBe(1); expect(rpc.cancel).not.toHaveBeenCalled();
    expect(retiredAction()).toBe(false); expect(actions.floating()).toBe(true);
    expect(childMounts).toBe(1); expect(childUnmounts).toBe(0); expect(vi.getTimerCount()).toBe(1);
  });

  // Another mounted view may retain project A while this view goes A-B-A; old commands stay retired.
  // 另一已挂载视图可在此视图甲乙甲切换时保留甲项目；旧命令仍保持退役。
  it("rejects stale view callbacks after A-to-B-to-A navigation", async () => {
    rpc.load.mockImplementation(async (projectId: string) => receipt(projectId));
    await act(async () => root.render(<Harness epoch={1} projects={{ floating: "project-a", retained: "project-a" }} />));
    const retired = views.floating;
    // A returning project receives a new action lifetime even while another view kept its store alive.
    // 即使其他视图保持仓库存活，返回项目也会获得新操作生命周期。
    const retiredAction = actions.floating;
    await act(async () => root.render(<Harness epoch={1} projects={{ floating: "project-b", retained: "project-a" }} />));
    await act(async () => root.render(<Harness epoch={1} projects={{ floating: "project-a", retained: "project-a" }} />));
    await act(async () => { retired.receive(receipt("project-a", { sequence: 99 })); await retired.cancel(); });
    expect(views.floating.job?.sequence).toBe(2); expect(rpc.cancel).not.toHaveBeenCalled();
    expect(retiredAction()).toBe(false); expect(actions.floating()).toBe(true);
    expect(rpc.load).toHaveBeenCalledTimes(2); expect(rpc.subscribe).toHaveBeenCalledTimes(1);
    expect(() => views.floating.receive(receipt("project-b"))).toThrow("身份不匹配");
  });

  // A handler captured from the old displayed job cannot cancel the next job in the same project.
  // 从旧展示作业捕获的处理函数不能取消同项目下一个作业。
  it("never redirects a stale cancel button to a newer job", async () => {
    rpc.load.mockResolvedValue(receipt());
    await act(async () => root.render(<Harness epoch={1} projects={{ floating: "project-a" }} />));
    const staleCancel = views.floating.cancel;
    await act(async () => rpc.subscribe.mock.calls[0][0](receipt("project-a", { jobId: "job-2", createdAtUnixMs: 5000 })));
    await act(async () => staleCancel()); expect(rpc.cancel).not.toHaveBeenCalled();
    expect(views.floating.job?.jobId).toBe("job-2");
  });

  // StrictMode's development cleanup cannot leak a second stream or leave a project unobserved.
  // 严格模式的开发清理不能泄漏第二个事件流或使项目失去观察。
  it("keeps one live stream and timer under StrictMode effect replay", async () => {
    rpc.load.mockResolvedValue(receipt());
    await act(async () => root.render(<React.StrictMode><Harness epoch={1} projects={{ floating: "project-a", settings: "project-a" }} /></React.StrictMode>));
    expect(views.floating.job?.sequence).toBe(2); expect(views.settings.job).toBe(views.floating.job);
    expect(rpc.subscribe).toHaveBeenCalledTimes(1); expect(vi.getTimerCount()).toBe(1);
    expect(actions.floating()).toBe(true);
    // Keep the active predicate across unmount to verify synchronous ownership retirement.
    // 跨卸载保留活动判断函数，验证同步归属退役。
    const retiredAction = actions.floating;
    await act(async () => root.render(null)); expect(rpc.stop).toHaveBeenCalledTimes(1); expect(vi.getTimerCount()).toBe(0);
    expect(retiredAction()).toBe(false);
    expect(rpc.cancel).not.toHaveBeenCalled();
  });

  // SSR does not schedule metadata reads, while missing workbench composition fails explicitly.
  // 服务端渲染不调度元数据读取，而缺失工作台组合会明确失败。
  it("is inert during SSR and rejects an unprovided hook", () => {
    expect(renderToStaticMarkup(<Harness epoch={1} projects={{ floating: "project-a" }} />)).toContain("empty");
    expect(rpc.load).not.toHaveBeenCalled(); expect(rpc.subscribe).not.toHaveBeenCalled(); expect(vi.getTimerCount()).toBe(0);
    expect(actions.floating()).toBe(false);
    expect(() => renderToStaticMarkup(<Probe name="missing" projectId="project-a" />)).toThrow("VceJobObserverProvider is missing");
  });
});
