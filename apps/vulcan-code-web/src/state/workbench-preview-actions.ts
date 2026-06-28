import type {
  ConversationMenuAction,
  ProjectMenuAction,
} from "@/components/project/ProjectSidebar";
import type { ProjectIconSelection } from "@/components/project/project-icons";
import type {
  ProjectSummary,
  WorkbenchSnapshot,
} from "@/types";

// applyPreviewProjectAction simulates project-menu commands for browser-only preview mode.
// applyPreviewProjectAction 为纯浏览器预览模式模拟项目菜单命令。
export function applyPreviewProjectAction(
  workbench: WorkbenchSnapshot,
  projectId: string,
  action: ProjectMenuAction,
): { snapshot: WorkbenchSnapshot; message: string } {
  const project = workbench.projects.find((candidate) => candidate.id === projectId);
  if (!project) {
    return { snapshot: workbench, message: "The project was not found." };
  }

  if (action === "toggle-pin") {
    const pinned = !project.pinned;
    return {
      snapshot: {
        ...workbench,
        projects: workbench.projects.map((candidate) => candidate.id === projectId ? { ...candidate, pinned } : candidate),
      },
      message: pinned ? `Pinned project ${project.name}.` : `Unpinned project ${project.name}.`,
    };
  }

  const archivedConversationIds = new Set(
    workbench.conversations
      .filter((conversation) => conversation.projectId === projectId)
      .map((conversation) => conversation.id),
  );
  const conversations = workbench.conversations.filter((conversation) => !archivedConversationIds.has(conversation.id));
  const selectedConversationId = archivedConversationIds.has(workbench.selectedConversationId ?? "")
    ? conversations[0]?.id ?? null
    : workbench.selectedConversationId;
  const projects = action === "remove-project"
    ? workbench.projects.filter((candidate) => candidate.id !== projectId)
    : workbench.projects;
  const expandedProjectIds = action === "remove-project"
    ? (workbench.expandedProjectIds ?? []).filter((candidate) => candidate !== projectId)
    : workbench.expandedProjectIds;

  return {
    snapshot: {
      ...workbench,
      projects,
      expandedProjectIds,
      conversations,
      selectedConversationId,
      messages: workbench.messages.filter((message) => !archivedConversationIds.has(message.conversationId)),
    },
    message: action === "remove-project"
      ? `Archived and removed project ${project.name}.`
      : `Archived conversations under project ${project.name}.`,
  };
}

// applyPreviewProjectRename simulates project rename for browser-only preview mode.
// applyPreviewProjectRename 为纯浏览器预览模式模拟项目重命名。
export function applyPreviewProjectRename(
  workbench: WorkbenchSnapshot,
  projectId: string,
  name: string,
): { snapshot: WorkbenchSnapshot } {
  return {
    snapshot: {
      ...workbench,
      projects: workbench.projects.map((candidate) => candidate.id === projectId ? { ...candidate, name } : candidate),
    },
  };
}

// applyPreviewProjectIcon simulates project icon persistence for browser-only preview mode.
// applyPreviewProjectIcon 为纯浏览器预览模式模拟项目图标持久化。
export function applyPreviewProjectIcon(
  workbench: WorkbenchSnapshot,
  projectId: string,
  icon: ProjectIconSelection | null,
): { snapshot: WorkbenchSnapshot } {
  return {
    snapshot: {
      ...workbench,
      projects: workbench.projects.map((candidate) => candidate.id === projectId ? {
        ...candidate,
        iconPack: icon?.pack ?? null,
        iconKey: icon?.key ?? null,
      } : candidate),
    },
  };
}

// applyPreviewConversationRename simulates conversation rename for browser-only preview mode.
// applyPreviewConversationRename 为纯浏览器预览模式模拟会话重命名。
export function applyPreviewConversationRename(
  workbench: WorkbenchSnapshot,
  conversationId: string,
  title: string,
): { snapshot: WorkbenchSnapshot } {
  return {
    snapshot: {
      ...workbench,
      conversations: workbench.conversations.map((conversation) => (
        conversation.id === conversationId ? { ...conversation, title } : conversation
      )),
    },
  };
}

// applyPreviewProjectOrder simulates same-section project ordering in browser preview mode.
// applyPreviewProjectOrder 在浏览器预览模式下模拟同分区项目排序。
export function applyPreviewProjectOrder(workbench: WorkbenchSnapshot, orderedProjectIds: string[]): { snapshot: WorkbenchSnapshot } {
  if (orderedProjectIds.length === 0) {
    return { snapshot: workbench };
  }

  const orderedSet = new Set(orderedProjectIds);
  const sectionPinned = Boolean(workbench.projects.find((project) => project.id === orderedProjectIds[0])?.pinned);
  const orderedProjects: ProjectSummary[] = [];
  for (const projectId of orderedProjectIds) {
    const project = workbench.projects.find((candidate) => candidate.id === projectId);
    if (project && Boolean(project.pinned) === sectionPinned) {
      orderedProjects.push(project);
    }
  }
  const remainingSectionProjects = workbench.projects.filter((project) => (
    Boolean(project.pinned) === sectionPinned && !orderedSet.has(project.id)
  ));
  const oppositeSectionProjects = workbench.projects.filter((project) => Boolean(project.pinned) !== sectionPinned);
  const nextSectionProjects = [...orderedProjects, ...remainingSectionProjects];
  const nextProjects = sectionPinned
    ? [...nextSectionProjects, ...oppositeSectionProjects]
    : [...oppositeSectionProjects, ...nextSectionProjects];
  return { snapshot: { ...workbench, projects: nextProjects } };
}

// applyPreviewConversationAction simulates conversation-menu commands for browser-only preview mode.
// applyPreviewConversationAction 为纯浏览器预览模式模拟会话菜单命令。
export function applyPreviewConversationAction(
  workbench: WorkbenchSnapshot,
  conversationId: string,
  action: ConversationMenuAction,
): { snapshot: WorkbenchSnapshot } {
  if (action !== "archive") {
    return { snapshot: workbench };
  }
  const conversations = workbench.conversations.filter((conversation) => conversation.id !== conversationId);
  const selectedConversationId = workbench.selectedConversationId === conversationId
    ? conversations[0]?.id ?? null
    : workbench.selectedConversationId;
  return {
    snapshot: {
      ...workbench,
      conversations,
      selectedConversationId,
      messages: workbench.messages.filter((message) => message.conversationId !== conversationId),
    },
  };
}
