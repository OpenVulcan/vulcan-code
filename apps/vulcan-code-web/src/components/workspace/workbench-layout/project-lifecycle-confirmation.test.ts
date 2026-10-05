// @vitest-environment jsdom
import React, { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import type { ConversationSummary, ProjectSummary, WorkbenchSnapshot } from "@/types";
import { useConversationMenuActions } from "./use-conversation-menu-actions";
import { useProjectLifecycleConfirmation } from "./use-project-lifecycle-confirmation";

// service isolates the lifecycle RPCs called by the production hooks.
// service 隔离生产 hook 调用的生命周期 RPC。
const service = vi.hoisted(() => ({
  archiveConversation: vi.fn(),
  archiveProjectConversations: vi.fn(),
  removeProject: vi.fn(),
  openProjectDirectoryPath: vi.fn(),
}));
vi.mock("@/service/project-actions-service", () => service);
vi.mock("sonner", () => ({ toast: { error: vi.fn(), success: vi.fn() } }));

// workbench contains one visible conversation owned by the confirmed project.
// workbench 包含一个归属于待确认项目的可见会话。
const workbench = { conversations: [{ id: "existing", projectId: "p" }] } as unknown as WorkbenchSnapshot;
// project is the dialog target shared by archive and removal.
// project 是归档与移除共用的弹窗目标。
const project = { id: "p", name: "Project" } as ProjectSummary;
// translate returns keys so failures stay observable without locale catalogs.
// translate 直接返回键名，使失败在无语言目录时仍可观察。
const translate = (key: string) => key;

// root hosts the probe component that runs the hook under test.
// root 承载运行被测 hook 的探针组件。
let root: Root;
// container is the detached DOM node owned by root.
// container 是 root 持有的游离 DOM 节点。
let container: HTMLDivElement;

beforeEach(() => {
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  container = document.createElement("div");
  root = createRoot(container);
});

afterEach(() => {
  act(() => root.unmount());
  vi.unstubAllGlobals();
  vi.clearAllMocks();
});

// renderHook mounts the hook once and returns an accessor for its latest result.
// renderHook 挂载 hook 一次，并返回读取其最新结果的访问器。
function renderHook<T>(hook: () => T): () => T {
  // latest stores the value returned by the most recent render.
  // latest 保存最近一次渲染返回的值。
  let latest: T | undefined;
  // Probe runs the hook inside a real React component.
  // Probe 在真实 React 组件中运行 hook。
  function Probe(): null {
    latest = hook();
    return null;
  }
  act(() => root.render(React.createElement(Probe)));
  return () => latest as T;
}

for (const kind of ["archive", "remove"] as const) {
  // Verify duplicate confirmation and a missing authoritative snapshot cannot mutate local lifecycle state.
  // 验证重复确认与缺失权威快照不能变更本地生命周期状态。
  it(`${kind} confirmation excludes duplicate calls and preserves current state on a null response`, async () => {
    // finish delays the service response past a second synchronous confirmation click.
    // finish 将服务响应延迟至第二次同步确认点击之后。
    let finish!: (value: null) => void;
    // rpc is the lifecycle RPC selected by this case.
    // rpc 是本用例选择的生命周期 RPC。
    const rpc = kind === "archive" ? service.archiveProjectConversations : service.removeProject;
    rpc.mockImplementation(() => new Promise<null>((resolve) => { finish = resolve; }));
    // apply and target observe forbidden preview updates and premature dialog closure.
    // apply 和 target 观察不应发生的预览更新与提前关闭。
    const apply = vi.fn(() => true);
    const target = vi.fn();
    const result = renderHook(() => useProjectLifecycleConfirmation({
      translate, workbenchState: workbench,
      archiveProjectTarget: project, setArchiveProjectTarget: target,
      removeProjectTarget: project, setRemoveProjectTarget: target,
      applyProjectActionSnapshot: apply, applyLifecycleResponseSnapshot: vi.fn(() => false),
    }));
    // confirm selects the production handler for this case at call time.
    // confirm 在调用时选择本用例对应的生产处理器。
    const confirm = () => (kind === "archive" ? result().handleArchiveProjectConfirm() : result().handleRemoveProjectConfirm());
    // pending spans the first confirmation; the second must return without another RPC.
    // pending 覆盖第一次确认；第二次必须返回而不再次调用 RPC。
    let pending!: Promise<void>;
    await act(async () => {
      pending = confirm();
      await confirm();
    });
    expect(rpc).toHaveBeenCalledOnce();
    await act(async () => {
      finish(null);
      await pending;
    });
    expect(apply).not.toHaveBeenCalled();
    expect(target).not.toHaveBeenCalled();
    expect(kind === "archive" ? result().archiveProjectSubmitting : result().removeProjectSubmitting).toBe(false);
    // The lock is released: a later confirmation reaches the service again.
    // 锁已释放：之后的确认会再次到达服务。
    await act(async () => {
      const retry = confirm();
      finish(null);
      await retry;
    });
    expect(rpc).toHaveBeenCalledTimes(2);
  });
}

// A missing conversation archive snapshot must not apply a stale whole-workbench preview.
// 会话归档快照缺失时不得应用旧的完整工作台预览。
it("retains the selected conversation when archive returns no authoritative snapshot", async () => {
  service.archiveConversation.mockResolvedValue(null);
  // apply observes the absence of local snapshot writes.
  // apply 观察没有发生本地快照写入。
  const apply = vi.fn(() => true);
  const result = renderHook(() => useConversationMenuActions({
    translate, workbenchState: { conversations: [] } as unknown as WorkbenchSnapshot, setRenameTarget: vi.fn(),
    applyProjectActionSnapshot: apply, applyLifecycleResponseSnapshot: vi.fn(() => false),
  }));
  // conversation is the archived sidebar row.
  // conversation 是被归档的侧栏行。
  const conversation = { id: "c", title: "Current" } as ConversationSummary;
  await act(() => result().handleConversationMenuAction("archive", conversation));
  expect(apply).not.toHaveBeenCalled();
  // The pending lock is cleared, so the user can retry the archive.
  // 待处理锁已清除，用户可以重试归档。
  await act(() => result().handleConversationMenuAction("archive", conversation));
  expect(service.archiveConversation).toHaveBeenCalledTimes(2);
});
