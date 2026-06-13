import { hasRenderableDiff, parseUnifiedDiff } from "@/diff/parseDiff";
import type {
  DiffPanelSnapshot,
  DiffReviewSelection,
  WorkspaceDiffSnapshotResponse,
} from "@/diff/types";
import type {
  ConversationMessage,
  ConversationSummary,
  ProjectSummary,
  WorkbenchSnapshot,
} from "@/types";

// WorkspaceReviewDraftConversation carries a local draft conversation route.
// WorkspaceReviewDraftConversation 承载本地草稿会话路由。
export interface WorkspaceReviewDraftConversation {
  // Draft conversation id used by the selected route.
  // 当前选中路由使用的草稿会话标识。
  id: string;
  // Project id that owns the draft conversation.
  // 草稿会话所属的项目标识。
  projectId: string;
}

// WorkspaceReviewConversationBlock is the minimal conversation block shape needed by diff extraction.
// WorkspaceReviewConversationBlock 是 diff 提取所需的最小会话块结构。
export interface WorkspaceReviewConversationBlock {
  // Block kind projected from a transcript message.
  // 从转录消息投影出的块类型。
  kind: string;
  // Block body text.
  // 块正文文本。
  body: string;
}

// activeWorkbenchSurfaceId returns the currently visible parent or subagent conversation id.
// activeWorkbenchSurfaceId 返回当前可见的父会话或子代理会话标识。
export function activeWorkbenchSurfaceId(workbench: WorkbenchSnapshot, selectedConversationId: string | null | undefined): string | null {
  const parentConversationId = selectedConversationId ?? workbench.selectedConversationId ?? null;
  if (workbench.subagentNavigation?.parentConversationId === parentConversationId) {
    return workbench.subagentNavigation.activeConversationId ?? parentConversationId;
  }
  return parentConversationId;
}

// selectedConversationProjectId returns the project id for the currently visible conversation or draft.
// selectedConversationProjectId 返回当前可见会话或草稿所属的项目标识。
export function selectedConversationProjectId(
  workbench: WorkbenchSnapshot,
  selectedConversationId: string | null | undefined,
  draftConversation?: WorkspaceReviewDraftConversation | null,
): string | null {
  if (draftConversation && selectedConversationId === draftConversation.id) {
    return draftConversation.projectId;
  }
  const activeSurfaceId = activeWorkbenchSurfaceId(workbench, selectedConversationId);
  const parentConversationId = selectedConversationId ?? workbench.selectedConversationId ?? null;
  const conversation = activeSurfaceId
    ? workbench.conversations.find((item) => item.id === activeSurfaceId)
      ?? workbench.conversations.find((item) => item.id === parentConversationId)
    : null;
  if (conversation?.projectId) {
    return conversation.projectId;
  }
  return conversation ? projectForConversation(workbench, conversation)?.id ?? null : workbench.projects[0]?.id ?? null;
}

// selectedConversationWorkspacePath returns the project path for the currently visible conversation or draft.
// selectedConversationWorkspacePath 返回当前可见会话或草稿所属的项目路径。
export function selectedConversationWorkspacePath(
  workbench: WorkbenchSnapshot,
  selectedConversationId: string | null | undefined,
  draftConversation?: WorkspaceReviewDraftConversation | null,
): string | null {
  if (draftConversation && selectedConversationId === draftConversation.id) {
    return workbench.projects.find((project) => project.id === draftConversation.projectId)?.path ?? null;
  }
  const activeSurfaceId = activeWorkbenchSurfaceId(workbench, selectedConversationId);
  const parentConversationId = selectedConversationId ?? workbench.selectedConversationId ?? null;
  if (!activeSurfaceId) {
    return workbench.projects[0]?.path ?? null;
  }
  const conversation = workbench.conversations.find((item) => item.id === activeSurfaceId)
    ?? workbench.conversations.find((item) => item.id === parentConversationId);
  if (!conversation) {
    return workbench.projects[0]?.path ?? null;
  }
  return projectForConversation(workbench, conversation)?.path ?? null;
}

// projectForConversation resolves a conversation owner project with the first project as fallback.
// projectForConversation 解析会话所属项目，并使用首个项目作为兜底。
function projectForConversation(
  workbench: WorkbenchSnapshot,
  conversation: ConversationSummary,
): ProjectSummary | null {
  const fallbackProject = workbench.projects[0] ?? null;
  return workbench.projects.find((project) => project.id === conversation.projectId) ?? fallbackProject;
}

// diffSnapshotFromWorkspaceResponse converts the backend Git response into renderer-ready state.
// diffSnapshotFromWorkspaceResponse 将后端 Git 响应转换为可渲染状态。
export function diffSnapshotFromWorkspaceResponse(response: WorkspaceDiffSnapshotResponse): DiffPanelSnapshot {
  const snapshot = parseUnifiedDiff(response.unifiedDiff, "workspace", response.repositoryRoot);
  return {
    ...snapshot,
    notice: response.notice ?? snapshot.notice,
    stats: {
      files: response.changedFiles || snapshot.stats.files,
      additions: response.additions || snapshot.stats.additions,
      deletions: response.deletions || snapshot.stats.deletions,
    },
    currentBranch: response.currentBranch ?? null,
    defaultBranch: response.defaultBranch ?? null,
    selectedBaseBranch: response.selectedBaseBranch ?? null,
    selectedCompareBranch: response.selectedCompareBranch ?? null,
    selectedCommitHash: response.selectedCommitHash ?? null,
    branches: response.branches ?? [],
    recentCommits: response.recentCommits ?? [],
  };
}

// diffReviewSelectionStableKey builds a compact stable key for one Git review selection.
// diffReviewSelectionStableKey 为单个 Git 审核选择构建紧凑稳定键。
export function diffReviewSelectionStableKey(selection: DiffReviewSelection): string {
  return [
    selection.mode,
    selection.baseBranch?.trim() ?? "",
    selection.compareBranch?.trim() ?? "",
    selection.commitHash?.trim() ?? "",
  ].join("\u001f");
}

// latestConversationDiffSnapshot resolves the newest structured diff from the selected conversation cache.
// latestConversationDiffSnapshot 从当前选中会话缓存中解析最新的结构化 diff。
export function latestConversationDiffSnapshot(
  workbench: WorkbenchSnapshot,
  selectedConversationId: string | null | undefined,
  parseBlocks: (message: ConversationMessage) => WorkspaceReviewConversationBlock[],
): DiffPanelSnapshot | null {
  const activeSurfaceId = activeWorkbenchSurfaceId(workbench, selectedConversationId);
  if (!activeSurfaceId) {
    return null;
  }
  const messages = workbench.messages.filter((message) => message.conversationId === activeSurfaceId);
  for (const message of [...messages].reverse()) {
    const blocks = parseBlocks(message);
    const diffBlock = [...blocks].reverse().find((block) => block.kind === "diff" && hasRenderableDiff(block.body));
    if (diffBlock) {
      return parseUnifiedDiff(diffBlock.body, diffPanelSourceForMessage(message), "");
    }
  }
  return null;
}

// diffPanelSourceForMessage derives a display source from structured message metadata.
// diffPanelSourceForMessage 从结构化消息元数据中推导展示来源。
function diffPanelSourceForMessage(message: ConversationMessage): DiffPanelSnapshot["source"] {
  const hostKind = metadataString(message.metadata, ["kind", "host_result.kind", "hostResult.kind"]);
  if (hostKind === "change_set") {
    return "change_set";
  }
  return "message";
}

// metadataString reads one nested metadata string by candidate paths.
// metadataString 按候选路径读取单个嵌套元数据字符串。
function metadataString(metadata: unknown, paths: string[]): string | null {
  for (const path of paths) {
    const value = metadataValue(metadata, path.split("."));
    if (typeof value === "string" && value.trim()) {
      return value.trim();
    }
  }
  return null;
}

// metadataValue reads one nested metadata value by path parts.
// metadataValue 按路径片段读取单个嵌套元数据值。
function metadataValue(metadata: unknown, paths: string[]): unknown {
  let current = metadata;
  for (const path of paths) {
    if (!current || typeof current !== "object" || !(path in current)) {
      return null;
    }
    current = (current as Record<string, unknown>)[path];
  }
  return current;
}
