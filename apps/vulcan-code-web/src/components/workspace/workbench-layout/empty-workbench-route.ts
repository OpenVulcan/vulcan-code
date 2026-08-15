import type { DraftConversationSelection } from "@/components/workspace/workspace-resizable-body";
import type { WorkbenchSnapshot } from "@/types";

// EmptyWorkbenchDraftRoute describes the draft and selection that replace an empty active conversation list.
// EmptyWorkbenchDraftRoute 描述活动会话列表为空时用于接管界面的草稿与选择状态。
export interface EmptyWorkbenchDraftRoute {
  // draftConversation is the local-only conversation owned by the target project.
  // draftConversation 是归属目标项目的仅本地会话。
  draftConversation: DraftConversationSelection;
  // selectedConversationId is the exact draft identity that the main conversation surface must render.
  // selectedConversationId 是主会话界面必须渲染的精确草稿标识。
  selectedConversationId: string;
}

// resolveEmptyWorkbenchDraftRoute resolves the new-conversation route after the active session collection becomes empty.
// resolveEmptyWorkbenchDraftRoute 在活动会话集合变空后解析新建会话路由。
//
// Workbench supplies the authoritative active projects and conversations, preferredProjectId identifies the last active project,
// and currentDraft preserves an already-open draft only when it belongs to that same project.
// Workbench 提供权威活动项目与会话，preferredProjectId 标识最后活动项目，currentDraft 仅在归属同一项目时保留已有草稿。
//
// Returns null while an active conversation exists or no project can own a draft; otherwise returns one coherent draft selection route.
// 当仍有活动会话或没有项目可承载草稿时返回 null；否则返回一份草稿与选择一致的新建会话路由。
export function resolveEmptyWorkbenchDraftRoute(
  workbench: Pick<WorkbenchSnapshot, "conversations" | "projects">,
  preferredProjectId: string | null,
  currentDraft: DraftConversationSelection | null,
): EmptyWorkbenchDraftRoute | null {
  if (workbench.conversations.length > 0) {
    return null;
  }
  // preferredProject is the still-visible project where the user most recently stayed.
  // preferredProject 是用户最近停留且当前仍可见的项目。
  const preferredProject = preferredProjectId
    ? workbench.projects.find((project) => project.id === preferredProjectId) ?? null
    : null;
  // targetProject falls back to the first active project only when the previous project no longer exists.
  // targetProject 仅在先前项目已不存在时回退到首个活动项目。
  const targetProject = preferredProject ?? workbench.projects[0] ?? null;
  if (!targetProject) {
    return null;
  }
  // draftConversation keeps unsent local state for the target project and replaces drafts owned by another project.
  // draftConversation 保留目标项目尚未发送的本地状态，并替换归属其他项目的草稿。
  const draftConversation = currentDraft?.projectId === targetProject.id
    ? currentDraft
    : {
      id: `draft-${targetProject.id}-${Date.now()}`,
      projectId: targetProject.id,
      projectName: targetProject.name,
    };
  return {
    draftConversation,
    selectedConversationId: draftConversation.id,
  };
}
