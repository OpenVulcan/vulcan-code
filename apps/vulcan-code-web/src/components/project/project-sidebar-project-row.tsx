import React from "react";
import {
  ChevronDown,
  ChevronRight,
  SquarePen,
} from "lucide-react";

import {
  SidebarMenu,
  SidebarMenuAction,
  SidebarMenuButton,
  SidebarMenuItem,
} from "@/components/ui/sidebar";
import type { LocaleCode } from "@/i18n";
import { cn } from "@/lib/utils";
import type { ConversationSummary, ProjectSummary } from "@/types";

import { ProjectIcon } from "./project-icons";
import {
  CollapsedProjectMenu,
  ProjectContextMenu,
} from "./project-sidebar-menus";
import { useVerticalSortableItem } from "./project-sidebar-sortable";
import type { ConversationMenuAction, ProjectMenuAction } from "./project-sidebar-types";

// SortableProjectRow renders one dnd-kit sortable project folder and its conversations.
// SortableProjectRow 渲染一个 dnd-kit 可排序项目文件夹及其会话。
export function SortableProjectRow(props: {
  // Project represented by this sortable row.
  // 当前可排序行代表的项目。
  project: ProjectSummary;
  // Conversations that belong to the project.
  // 归属于当前项目的会话列表。
  conversations: ConversationSummary[];
  // Whether the project folder is expanded.
  // 当前项目文件夹是否展开。
  isOpen: boolean;
  // Whether every conversation under this project should be displayed.
  // 当前项目下是否显示全部会话。
  isConversationListExpanded: boolean;
  // Whether the selected conversation belongs to this project.
  // 当前选中会话是否归属于该项目。
  hasSelectedConversation: boolean;
  // Current sidebar collapsed or expanded state.
  // 当前侧栏的折叠或展开状态。
  sidebarState: "collapsed" | "expanded";
  // Currently selected conversation id.
  // 当前选中的会话标识。
  selectedConversationId: string | null;
  // Active locale used for empty labels.
  // 用于空状态标签的当前语言。
  locale: LocaleCode;
  // Checks whether a conversation is pinned.
  // 检查某个会话是否置顶。
  isPinnedConversation: (conversationId: string) => boolean;
  // Toggles the project folder expansion.
  // 切换项目文件夹展开状态。
  onToggleOpen: (projectId: string) => void;
  // Toggles the conversation list overflow rows for this project.
  // 切换当前项目会话列表的溢出行。
  onToggleConversationList: (projectId: string) => void;
  // Opens a new draft conversation for the project.
  // 为项目打开一个新的草稿会话。
  onCreateConversation: (project: ProjectSummary) => void;
  // Handles project context menu actions.
  // 处理项目右键菜单动作。
  onProjectMenuAction: (action: ProjectMenuAction, project: ProjectSummary) => void;
  // Handles conversation context menu actions.
  // 处理会话右键菜单动作。
  onConversationMenuAction: (action: ConversationMenuAction, conversation: ConversationSummary) => void;
  // Selects one conversation.
  // 选择一个会话。
  onSelectConversation: (conversationId: string) => void;
  // Renders one conversation row with the shared sidebar style.
  // 使用共享侧栏样式渲染单个会话行。
  renderConversationRow: (conversation: ConversationSummary, compactPinned?: boolean) => React.ReactNode;
}) {
  const sortable = useVerticalSortableItem(props.project.id);
  const folderName = projectFolderName(props.project.path);
  const showOriginalFolderName = folderName.length > 0 && folderName !== props.project.name;
  const conversationPreviewLimit = 5;
  const hasConversationOverflow = props.conversations.length > conversationPreviewLimit;
  const visibleConversations = hasConversationOverflow && !props.isConversationListExpanded
    ? props.conversations.slice(0, conversationPreviewLimit)
    : props.conversations;
  const conversationOverflowLabel = props.isConversationListExpanded
    ? props.locale === "zh-CN" ? "折叠显示" : "Collapse"
    : props.locale === "zh-CN" ? "展开显示" : "Show more";
  return (
    <SidebarMenuItem
      className={cn("project-tree-item", sortable.isDragging && "drag-source")}
      ref={sortable.setNodeRef}
      style={sortable.sortableStyle}
    >
      {props.sidebarState === "collapsed" ? (
        <CollapsedProjectMenu
          conversations={props.conversations}
          onCreateConversation={props.onCreateConversation}
          onSelectConversation={props.onSelectConversation}
          project={props.project}
          hasSelectedConversation={props.hasSelectedConversation}
          selectedConversationId={props.selectedConversationId}
        />
      ) : (
        <div className={cn("project-folder-row", sortable.isDragging && "drag-source-row")} {...sortable.attributes} {...sortable.listeners}>
          <ProjectContextMenu project={props.project} onAction={props.onProjectMenuAction}>
            <SidebarMenuButton
              aria-expanded={props.isOpen}
              className="project-folder-button"
              onClick={() => props.onToggleOpen(props.project.id)}
              tooltip={`${props.project.name}\n${props.project.path}`}
            >
              <ProjectIcon isOpen={props.isOpen} project={props.project} size={16} />
              <span className="vcn-sidebar-label project-folder-label">
                <strong>{props.project.name}</strong>
                {showOriginalFolderName ? <small>{folderName}</small> : null}
              </span>
              <span className="project-folder-arrow" aria-hidden="true">
                {props.isOpen ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
              </span>
            </SidebarMenuButton>
          </ProjectContextMenu>
          <SidebarMenuAction
            aria-label={`在 ${props.project.name} 中创建新会话`}
            className="project-create-conversation"
            onClick={(event) => {
              event.stopPropagation();
              props.onCreateConversation(props.project);
            }}
            data-project-row-action
            showOnHover
            title="新建会话"
            type="button"
          >
            <SquarePen size={13} />
          </SidebarMenuAction>
        </div>
      )}
      {props.isOpen && props.sidebarState !== "collapsed" ? (
        <SidebarMenu className="project-conversation-list">
          {props.conversations.length > 0 ? (
            <>
              {visibleConversations.map((conversation) => props.renderConversationRow(conversation))}
              {hasConversationOverflow ? (
                <SidebarMenuItem>
                  <button
                    className="conversation-list-toggle"
                    onClick={(event) => {
                      event.stopPropagation();
                      props.onToggleConversationList(props.project.id);
                    }}
                    type="button"
                  >
                    {conversationOverflowLabel}
                  </button>
                </SidebarMenuItem>
              ) : null}
            </>
          ) : (
            <SidebarMenuItem>
              <div className="conversation-empty-row">{props.locale === "zh-CN" ? "暂无对话" : "No conversations"}</div>
            </SidebarMenuItem>
          )}
        </SidebarMenu>
      ) : null}
    </SidebarMenuItem>
  );
}

// projectFolderName extracts the folder name from a platform path.
// projectFolderName 从平台路径中提取文件夹名称。
function projectFolderName(path: string): string {
  return path.split(/[\\/]/).map((segment) => segment.trim()).filter(Boolean).pop() ?? "";
}
