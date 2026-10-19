import React from "react";
import {
  ChevronDown,
  ChevronRight,
  SquarePen,
  GripVertical,
} from "lucide-react";

import {
  SidebarMenu,
  SidebarMenuAction,
  SidebarMenuButton,
  SidebarMenuItem,
} from "@/components/ui/sidebar";
import { useI18n, formatMessage } from "@/i18n";
import { Button } from "@/ui-kit/primitives/button";
import { cn } from "@/lib/utils";
import type { ConversationSummary, ProjectSummary } from "@/types";

import { ProjectIcon } from "./project-icons";
import { ProjectContextMenu } from "./project-sidebar-menus";
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
  // Whether this project owns the project-level floating tools.
  // 当前项目是否持有项目级浮动工具。
  isActiveProject: boolean;
  // Currently selected conversation id.
  // 当前选中的会话标识。
  selectedConversationId: string | null;
  // Checks whether a conversation is pinned.
  // 检查某个会话是否置顶。
  isPinnedConversation: (conversationId: string) => boolean;
  // Toggles the project folder expansion.
  // 切换项目文件夹展开状态。
  onToggleOpen: (projectId: string) => void;
  // Selects this project without changing the current conversation.
  // 选择当前项目且不改变当前会话。
  onSelectProject: (projectId: string) => void;
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
  const { t } = useI18n();
  // reorderLabel names the project and explains the independent keyboard sort interaction.
  // reorderLabel 标明项目并说明独立的键盘排序交互。
  const reorderLabel = formatMessage(t("project.sidebar.reorderProject"), { name: props.project.name });
  const sortable = useVerticalSortableItem(props.project.id);
  const folderName = projectFolderName(props.project.path);
  const showOriginalFolderName = folderName.length > 0 && folderName !== props.project.name;
  const conversationPreviewLimit = 5;
  const hasConversationOverflow = props.conversations.length > conversationPreviewLimit;
  const visibleConversations = hasConversationOverflow && !props.isConversationListExpanded
    ? props.conversations.slice(0, conversationPreviewLimit)
    : props.conversations;
  const conversationOverflowLabel = props.isConversationListExpanded
    ? t("projectRow.collapseConversations")
    : t("projectRow.showMoreConversations");
  const projectFolderButton = (
    <SidebarMenuButton
      aria-expanded={props.isOpen}
      className={cn(
        "project-folder-button",
        props.isActiveProject && "project-folder-active",
      )}
      onClick={() => {
        props.onSelectProject(props.project.id);
        props.onToggleOpen(props.project.id);
      }}
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
  );
  return (
    <SidebarMenuItem
      className={cn("project-tree-item", sortable.isDragging && "drag-source")}
      ref={sortable.setNodeRef}
      style={sortable.sortableStyle}
    >
      <div
        className={cn("project-folder-row", sortable.isDragging && "drag-source-row")}
      >
        <Button
          ref={sortable.setActivatorNodeRef}
          {...sortable.attributes}
          {...sortable.listeners}
          aria-label={reorderLabel}
          title={reorderLabel}
          className="project-reorder-handle"
          variant="ghost"
          size="icon-xs"
          type="button"
        >
          <GripVertical aria-hidden="true" />
        </Button>
        <ProjectContextMenu project={props.project} onAction={props.onProjectMenuAction}>
          {projectFolderButton}
        </ProjectContextMenu>
        <SidebarMenuAction
          aria-label={t("projectRow.createConversationAria").replace("{project}", props.project.name)}
          className="project-create-conversation"
          onClick={(event) => {
            event.stopPropagation();
            props.onCreateConversation(props.project);
          }}
          data-project-row-action
          showOnHover
          title={t("projectMenu.newConversation")}
          type="button"
        >
          <SquarePen size={13} />
        </SidebarMenuAction>
      </div>
      {/* Empty projects show the placeholder once, on the active project only, so long project lists stay quiet. */}
      {/* 空项目只在当前项目上显示一次占位行，避免长项目列表中重复出现。 */}
      {props.isOpen && (props.conversations.length > 0 || props.isActiveProject) ? (
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
              <div className="conversation-empty-row">{t("projectRow.noConversations")}</div>
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
