// @vitest-environment jsdom
import React, { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ProjectSummary, ProjectTerminalCatalog, ProjectTerminalSummary } from "@/types";
import { ProjectFloatingTerminalLayer } from "./project-floating-terminal-layer";

// service doubles never create a host, write input, or access a real project.
// 服务替身不会创建宿主、写入输入或访问真实项目。
const service = vi.hoisted(() => ({ list: vi.fn(), create: vi.fn(), rename: vi.fn(), stop: vi.fn() }));
const focus = vi.hoisted(() => vi.fn());
vi.mock("@/service/terminal-service", () => ({
  listProjectTerminals: service.list, createProjectTerminal: service.create,
  renameProjectTerminal: service.rename,
  stopProjectTerminal: service.stop,
}));
vi.mock("@/components/terminal/use-terminal-display-connection-epoch", () => ({
  useTerminalDisplayConnectionState: () => ({ connectionAvailable: false, connectionEpoch: 0 }),
}));
vi.mock("@/components/terminal/terminal-viewport", () => ({ TerminalViewport: () => null }));
vi.mock("@/components/workspace/workspace-input-owner", () => ({
  releaseProjectTerminalInputOwner: vi.fn(), setWorkspaceInputOwner: focus,
}));
vi.mock("@/i18n", () => ({ useI18n: () => ({ t: (key: string) => key }) }));
vi.mock("@/components/workspace/project-floating-layer", () => ({
  ProjectFloatingLayer: (props: { expanded: boolean; children: React.ReactNode }) => props.expanded ? props.children : null,
}));

// project returns the explicit identities consumed by the terminal presentation.
// project 返回终端展示消费的明确标识。
function project(id: string): ProjectSummary {
  return { id, name: id, path: `/synthetic/${id}` } as ProjectSummary;
}
// terminal returns a stopped synthetic tab so display attachment cannot run.
// terminal 返回已停止的合成标签，保证显示挂接不会运行。
function terminal(projectId: string, id: string): ProjectTerminalSummary {
  return { id, projectId, name: id, shellId: "synthetic-shell", cwd: `/synthetic/${projectId}`,
    status: "stopped", exitCode: null, lastSeq: 0, droppedBeforeSeq: 0, stateRevision: 1,
    lastAcceptedInputSeq: 0, rows: 24, cols: 80 };
}
// catalog constructs the actual service response schema with one explicit default shell.
// catalog 按真实服务响应结构构造一个明确默认 Shell。
function catalog(projectId: string, terminals: ProjectTerminalSummary[] = []): ProjectTerminalCatalog {
  return { projectId, terminals, runningCountRevision: 1,
    shells: [{ id: "synthetic-shell", label: "Synthetic shell", default: true }] };
}
// pending exposes one deferred synthetic response to control completion order.
// pending 暴露一个延迟合成响应以控制完成顺序。
function pending<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: Error) => void;
  const promise = new Promise<T>((accept, fail) => { resolve = accept; reject = fail; });
  return { promise, resolve, reject };
}

describe("project terminal controls", () => {
  let root: Root;
  let host: HTMLDivElement;
  beforeEach(() => {
    Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
    vi.clearAllMocks();
    host = document.createElement("div"); document.body.append(host); root = createRoot(host);
  });
  afterEach(async () => { await act(async () => root.unmount()); host.remove(); });
  // render selects an explicit project without consulting any persisted configuration.
  // render 选择明确项目，不查询任何持久化配置。
  async function render(id: string, projects = [project(id)], nonce = 1, action: "open" | "toggle" = "open") {
    await act(async () => root.render(<ProjectFloatingTerminalLayer project={project(id)} projects={projects}
      minimizeAnchorRef={{ current: null }} openRequest={{ projectId: id, nonce, action }} />));
  }
  // button resolves the visible control by its accessible name.
  // button 根据无障碍名称解析可见控件。
  function button(label: string) {
    return host.querySelector<HTMLButtonElement>(`button[aria-label="${label}"]`)!;
  }

  it("retries a failed initial catalog without creating a terminal", async () => {
    service.list.mockRejectedValueOnce(new Error("synthetic read failure")).mockResolvedValue(catalog("retry"));
    await render("retry");
    expect(host.textContent).toContain("synthetic read failure");
    expect(host.textContent).not.toContain("workspaceAudit.terminalLoading");
    await act(async () => host.querySelector<HTMLButtonElement>("button")!.click());
    expect(service.list).toHaveBeenCalledTimes(2);
    expect(button("workspaceAudit.create").disabled).toBe(false);
    expect(service.create).not.toHaveBeenCalled();
  });

  it("coalesces creation and does not steal focus after a project switch", async () => {
    const creation = pending<ProjectTerminalCatalog>();
    service.list.mockImplementation(async (id: string) => catalog(id));
    service.create.mockReturnValue(creation.promise);
    const projects = [project("create-a"), project("create-b")];
    await render("create-a", projects);
    await act(async () => { button("workspaceAudit.create").click(); button("workspaceAudit.create").click(); });
    expect(service.create).toHaveBeenCalledTimes(1);
    await render("create-b", projects);
    await act(async () => creation.resolve(catalog("create-a", [terminal("create-a", "new-a")])));
    expect(focus).not.toHaveBeenCalled();
    expect(host.textContent).not.toContain("new-a");
  });

  it("locks the rename input while saving and preserves its draft on failure", async () => {
    const rename = pending<ProjectTerminalCatalog>();
    service.list.mockResolvedValue(catalog("rename", [terminal("rename", "draft-name")]));
    service.rename.mockReturnValue(rename.promise);
    await render("rename");
    await act(async () => button("workspaceAudit.editName").click());
    await act(async () => { button("workspaceAudit.saveName").click(); button("workspaceAudit.saveName").click(); });
    expect(service.rename).toHaveBeenCalledTimes(1);
    expect(host.querySelector<HTMLInputElement>("input")!.disabled).toBe(true);
    await act(async () => rename.reject(new Error("synthetic save failure")));
    expect(host.querySelector<HTMLInputElement>("input")!.value).toBe("draft-name");
    expect(host.querySelector<HTMLInputElement>("input")!.disabled).toBe(false);
    expect(host.textContent).toContain("synthetic save failure");
  });

  it("does not reclaim keyboard ownership when creation finishes after minimizing", async () => {
    const creation = pending<ProjectTerminalCatalog>();
    service.list.mockResolvedValue(catalog("minimized"));
    service.create.mockReturnValue(creation.promise);
    await render("minimized");
    await act(async () => button("workspaceAudit.create").click());
    await render("minimized", [project("minimized")], 2, "toggle");
    await act(async () => creation.resolve(catalog("minimized", [terminal("minimized", "hidden-tab")])));
    expect(focus).not.toHaveBeenCalled();
  });

  it("ignores a catalog read that finishes after a newer creation", async () => {
    const olderRead = pending<ProjectTerminalCatalog>();
    service.list.mockResolvedValueOnce(catalog("ordered")).mockReturnValueOnce(olderRead.promise);
    service.create.mockResolvedValue(catalog("ordered", [terminal("ordered", "retained-new-tab")]));
    await render("ordered");
    await render("ordered", [project("ordered")], 2, "toggle");
    await render("ordered", [project("ordered")], 3);
    await act(async () => button("workspaceAudit.create").click());
    await act(async () => olderRead.resolve(catalog("ordered")));
    expect(host.textContent).toContain("retained-new-tab");
  });

  it("supports vertical arrow navigation with one tabbable selected terminal", async () => {
    service.list.mockResolvedValue(catalog("keyboard", [terminal("keyboard", "first"), terminal("keyboard", "second")]));
    await render("keyboard");
    const first = host.querySelector<HTMLButtonElement>('button[role="tab"]')!;
    await act(async () => first.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowDown", bubbles: true })));
    const selected = host.querySelector<HTMLButtonElement>('button[role="tab"][aria-selected="true"]')!;
    expect(selected.textContent).toContain("second");
    expect(selected.tabIndex).toBe(0);
    expect(first.tabIndex).toBe(-1);
    expect(host.querySelector('[role="tabpanel"]')?.getAttribute("aria-labelledby")).toBe(selected.id);
  });

  it("keeps the exact stop confirmation and failure visible when the service rejects", async () => {
    const stopped = pending<void>();
    service.list.mockResolvedValue(catalog("stop", [{ ...terminal("stop", "running-target"), status: "running" }]));
    service.stop.mockReturnValue(stopped.promise);
    await render("stop");
    await act(async () => button("workspaceAudit.stop").click());
    const dialog = document.querySelector('[role="alertdialog"]')!;
    const confirm = Array.from(dialog.querySelectorAll("button")).find((entry) => entry.textContent === "workspaceAudit.stop")!;
    await act(async () => { confirm.click(); confirm.click(); });
    expect(service.stop).toHaveBeenCalledExactlyOnceWith("stop", "running-target");
    await act(async () => stopped.reject(new Error("synthetic stop failure")));
    expect(document.querySelector('[role="alertdialog"] [role="alert"]')?.textContent).toBe("synthetic stop failure");
    expect(document.querySelector('[role="alertdialog"]')).not.toBeNull();
  });
});
