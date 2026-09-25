// @vitest-environment jsdom
import React, { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { I18nProvider } from "@/i18n";
import { browserFallbackSnapshot } from "@/service/browser-fallback-workbench";
import { loadWorkspaceGitCatalog } from "@/service/workbench-snapshot-service";
import type { DiffReviewPanel } from "@/diff/diff-review-panel";
import type { WorkspaceGitRepositoryCatalog } from "@/diff/types";
import { WorkspaceReviewPanelHost } from "./workspace-review-panel-host";

// Capture the real host boundary while leaving all project/repository effects and action ownership active.
// 捕获真实宿主边界，同时保留全部项目/仓库副作用及动作归属逻辑。
let panel: React.ComponentProps<typeof DiffReviewPanel>;
vi.mock("@/diff/diff-review-panel", () => ({ DiffReviewPanel: (props: React.ComponentProps<typeof DiffReviewPanel>) => { panel = props; return null; } }));
vi.mock("@/state/conversation-event-window-react", () => ({ useSelectedConversationEventWindowSnapshot: () => ({ owner: null }) }));
vi.mock("@/service/workbench-snapshot-service", () => ({ loadWorkspaceGitCatalog: vi.fn() }));
vi.mock("sonner", () => ({ toast: { error: vi.fn(), success: vi.fn() } }));
// The transport-free parser uses the same production parsing function without requiring a browser Worker.
// 无传输解析器使用同一生产解析函数，不依赖浏览器 Worker。
vi.mock("@/diff/diff-parse-worker-client", async () => {
  const { parseUnifiedDiff } = await import("@/diff/parseDiff");
  return { parseUnifiedDiffAsync: async (...args: Parameters<typeof parseUnifiedDiff>) => parseUnifiedDiff(...args) };
});

// Each case owns an isolated DOM and unique workspace cache identity.
// 每条用例持有隔离文档与唯一工作区缓存身份。
let host: HTMLDivElement;
let root: Root;
let sequence = 0;
beforeEach(() => {
  vi.clearAllMocks();
  vi.stubGlobal("IS_REACT_ACT_ENVIRONMENT", true);
  host = document.createElement("div"); document.body.append(host); root = createRoot(host);
});
afterEach(async () => { await act(async () => root.unmount()); host.remove(); vi.unstubAllGlobals(); });

/** Builds exact typed project, catalog and rejecting unused IO; returns production host props.
 * 构建精确强类型项目、目录与明确拒绝的未使用输入输出；返回生产宿主属性。 */
function fixture(): React.ComponentProps<typeof WorkspaceReviewPanelHost> {
  const workbench = browserFallbackSnapshot();
  const path = `D:/synthetic-review-${++sequence}`;
  workbench.projects = [{ id: path, name: "Audit", path }];
  workbench.conversations = [{ id: path, title: "Audit", status: "idle", projectId: path }];
  const catalog: WorkspaceGitRepositoryCatalog = { projectRoot: path, repositories: ["repo-a", "repo-b"].map(repositoryKey => ({ repositoryKey, repositoryRoot: `${path}/${repositoryKey}`, name: repositoryKey, relativePath: repositoryKey, isPrimary: repositoryKey === "repo-a", monitoringEnabled: true })) };
  vi.mocked(loadWorkspaceGitCatalog).mockResolvedValue(catalog);
  const rejectUnused = async (): Promise<never> => { throw new Error("Unexpected audit IO"); };
  return { workbench, selectedConversationId: path, selectedDraftConversation: null, reviewFocusRequest: { sequence: 0, repositoryKey: null }, onCopyRawDiff: rejectUnused,
    loadWorkspaceDiffSnapshot: vi.fn(async () => ({ repositoryRoot: path, unifiedDiff: "", changedFiles: 0, additions: 0, deletions: 0 })),
    loadWorkspaceDiffFileDetail: rejectUnused, loadWorkspaceDiffSearch: rejectUnused, runWorkspaceGitFileAction: rejectUnused };
}

/** Mounts the real host and settles repository discovery and cache effects for the given props.
 * 挂载真实宿主，并等待给定属性的仓库发现与缓存副作用结束。 */
async function render(props: React.ComponentProps<typeof WorkspaceReviewPanelHost>): Promise<void> {
  await act(async () => root.render(<I18nProvider initialLocale="zh-CN"><WorkspaceReviewPanelHost {...props} /></I18nProvider>));
}

it("keeps a first-time repository choice and restores it after closing the panel", async () => {
  const props = fixture();
  await render(props);
  expect(panel.selection?.repositoryKey).toBe("repo-a");
  await act(async () => panel.onSelectionChange?.({ mode: "staged", repositoryKey: "repo-b" }));
  expect(panel.selection).toEqual({ mode: "staged", repositoryKey: "repo-b" });
  expect(props.loadWorkspaceDiffSnapshot).toHaveBeenLastCalledWith(props.workbench.projects[0].path, props.workbench.projects[0].id, props.selectedConversationId, { mode: "staged", repositoryKey: "repo-b" });
  await act(async () => root.render(null));
  await render(props);
  expect(panel.selection).toEqual({ mode: "staged", repositoryKey: "repo-b" });
});

it("returns rejected mutations to the awaiting control and releases its pending state", async () => {
  const props = fixture();
  props.runWorkspaceGitFileAction = vi.fn(async () => { throw new Error("Synthetic Git denied"); });
  await render(props);
  await act(async () => { await expect(panel.onGitFileAction?.("stageAll")).rejects.toThrow("Synthetic Git denied"); });
  expect(panel.gitFileActionPending).toBe(false);
  expect(props.runWorkspaceGitFileAction).toHaveBeenCalledExactlyOnceWith(props.workbench.projects[0].path, props.workbench.projects[0].id, props.selectedConversationId, { action: "stageAll", files: [], repositoryKey: "repo-a" });
});

it("does not refresh an old repository after awaiting shared Git state", async () => {
  const props = fixture();
  let releaseShared!: () => void;
  props.runWorkspaceGitFileAction = vi.fn<React.ComponentProps<typeof WorkspaceReviewPanelHost>["runWorkspaceGitFileAction"]>(async () => ({ repositoryRoot: props.workbench.projects[0].path, action: "stageAll", affectedPathCount: 0, summary: "Synthetic staged" }));
  props.onWorkspaceGitStateChanged = vi.fn(() => new Promise<void>(resolve => { releaseShared = resolve; }));
  await render(props);
  let action: void | Promise<void>;
  await act(async () => { action = panel.onGitFileAction?.("stageAll"); });
  await act(async () => panel.onSelectionChange?.({ mode: "unstaged", repositoryKey: "repo-b" }));
  const callsBefore = vi.mocked(props.loadWorkspaceDiffSnapshot).mock.calls.length;
  await act(async () => { releaseShared(); await action; });
  expect(panel.selection?.repositoryKey).toBe("repo-b");
  expect(vi.mocked(props.loadWorkspaceDiffSnapshot).mock.calls).toHaveLength(callsBefore);
});
