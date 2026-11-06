// Workspace chrome for the project home: the stage page that replaces the chat while a home is open, and the project
// crumb before a conversation title that opens the home.
// 项目首页的工作区外壳部分：首页打开时替代聊天的舞台页面，以及位于会话标题前、用于打开首页的项目入口。
import { X } from "lucide-react";
import { IconButton } from "@/ui-kit/primitives/button";
import { formatMessage, useI18n } from "@/i18n";
import { AssistantDockButton } from "@/components/assistant/assistant-dock-button";
import { ProjectIcon } from "@/components/project/project-icons";
import type { WorkspacePageDescriptor } from "@/components/workspace/workspace-page-descriptor";
import type { WorkspaceShellProps } from "@/components/workspace/workspace-shell/types";

// useWorkspaceProjectHome derives the project home stage page, the title crumb and the sidebar home props.
// useWorkspaceProjectHome 推导项目首页舞台页面、标题入口与侧栏首页属性。
//
// Parameters:
// 参数：
// props are the shell props; draftProjectId owns the visible draft when no durable conversation is selected.
// props 是外壳属性；draftProjectId 是未选中持久会话时可见草稿的所属项目。
export function useWorkspaceProjectHome(props: WorkspaceShellProps, draftProjectId: string | null) {
  const { t } = useI18n();
  // home is the project home covering the chat stage; it never shows over embedded feature pages.
  // home 是覆盖聊天舞台的项目首页；不会显示在内嵌功能页面之上。
  const home = props.mainView === "chat" ? props.projectHome ?? null : null;
  // crumbProject owns the visible conversation or draft and becomes the title crumb that opens its home.
  // crumbProject 是当前可见会话或草稿的所属项目，作为标题前的入口打开其首页。
  const crumbProjectId = props.selectedConversation?.projectId ?? draftProjectId;
  const crumbProject = props.mainView === "chat" && !home && crumbProjectId && props.onOpenProjectHome
    ? props.workbench.projects.find((project) => project.id === crumbProjectId) ?? null
    : null;
  const crumbLabel = crumbProject ? formatMessage(t("projectHome.openFor"), { project: crumbProject.name }) : "";
  const page: WorkspacePageDescriptor | null = home ? {
    content: home.content,
    detail: home.project.path,
    headerActions: (
      <IconButton aria-label={t("projectHome.close")} title={t("projectHome.close")} onClick={home.onClose}><X /></IconButton>
    ),
    headerRightActions: <AssistantDockButton />,
    title: home.project.name,
  } : null;
  return {
    // chatStage is true while the chat (not the project home) owns the stage.
    // chatStage 在聊天（而非项目首页）占据舞台时为 true。
    chatStage: props.mainView === "chat" && !home,
    crumb: crumbProject ? (
      <button aria-label={crumbLabel} className="vk-stage-crumb" title={crumbLabel} type="button" onClick={() => props.onOpenProjectHome?.(crumbProject)}>
        <ProjectIcon project={crumbProject} size={14} />
        <span>{crumbProject.name}</span>
      </button>
    ) : null,
    home,
    page,
    sidebar: { homeProjectId: home?.project.id ?? null, onOpenProjectHome: props.onOpenProjectHome },
  };
}
