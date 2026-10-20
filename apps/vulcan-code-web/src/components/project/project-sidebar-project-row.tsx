import React from "react";
import {
  ChevronsDown,
  ChevronsUp,
  GripVertical,
  Plus,
  SquarePen,
} from "lucide-react";

import { SidebarMenu, SidebarMenuItem } from "@/ui-kit/shell/sidebar";
import { useI18n, formatMessage } from "@/i18n";
import type { ConversationSummary, ProjectSummary } from "@/types";

import { ProjectIcon } from "./project-icons";
import type { ProjectRowPlacement } from "./project-sidebar-groups";
import { ProjectContextMenu } from "./project-sidebar-menus";
import { useVerticalSortableItem } from "./project-sidebar-sortable";
import type { ConversationMenuAction, ProjectMenuAction } from "./project-sidebar-types";

// SortableProjectRow renders one dnd-kit sortable project folder and its conversations.
// SortableProjectRow 渲染一个 dnd-kit 可排序项目文件夹及其会话。
export function SortableProjectRow(props: {
  // Project represented by this sortable row.
  // 当前可排序行代表的项目。
  project: ProjectSummary;
  // Move-to-group entries; null for pinned projects, which show in the pinned section.
  // “移动到分组”入口；置顶项目显示在置顶分区中，因此为 null。
  placement: ProjectRowPlacement | null;
  // Sortable id; defaults to the project id. Pinned rows sort among the pinned items under their own ids.
  // 可排序标识；默认为项目标识。置顶行以自身标识在置顶条目之间排序。
  sortableId?: string;
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
  const sortable = useVerticalSortableItem(props.sortableId ?? props.project.id);
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
    <button
      aria-expanded={props.isOpen}
      className="vk-tree-row"
      data-active={props.isActiveProject ? "" : undefined}
      data-contains-selection={props.hasSelectedConversation ? "" : undefined}
      onClick={() => {
        props.onSelectProject(props.project.id);
        props.onToggleOpen(props.project.id);
      }}
      title={`${props.project.name}
${props.project.path}`}
      type="button"
    >
      <span className="vk-tree-icon"><ProjectIcon isOpen={props.isOpen} project={props.project} size={15} /></span>
      <strong>
        {props.project.name}
        {showOriginalFolderName ? <small>{folderName}</small> : null}
      </strong>
    </button>
  );
  return (
    <SidebarMenuItem
      className="vk-tree-project"
      data-dragging={sortable.isDragging ? "" : undefined}
      ref={sortable.setNodeRef}
      style={sortable.sortableStyle}
    >
      <div className="vk-tree-item" data-kind="project">
        <ProjectContextMenu placement={props.placement} project={props.project} onAction={props.onProjectMenuAction}>
          {projectFolderButton}
        </ProjectContextMenu>
        <span className="vk-tree-actions">
          <button
            aria-label={t("projectRow.createConversationAria").replace("{project}", props.project.name)}
            className="vk-icon-btn"
            data-action="create"
            data-size="sm"
            onClick={() => props.onCreateConversation(props.project)}
            title={t("projectMenu.newConversation")}
            type="button"
          >
            <SquarePen />
          </button>
          <button
            ref={sortable.setActivatorNodeRef}
            {...sortable.attributes}
            {...sortable.listeners}
            aria-label={reorderLabel}
            className="vk-icon-btn vk-tree-grip"
            data-size="sm"
            title={reorderLabel}
            type="button"
          >
            <GripVertical aria-hidden="true" />
          </button>
        </span>
      </div>
      {/* An open folder always shows what it holds, so an open empty project never reads as closed; its empty row
          is the shortcut to its first session. */}
      {/* 打开的文件夹始终显示其内容，使打开的空项目不会被误读为已关闭；其空行即是创建首个会话的快捷入口。 */}
      {props.isOpen ? (
        <SidebarMenu className="vk-tree-children">
          {props.conversations.length > 0 ? (
            <>
              {visibleConversations.map((conversation) => props.renderConversationRow(conversation))}
              {hasConversationOverflow ? (
                <SidebarMenuItem>
                  <button
                    aria-expanded={props.isConversationListExpanded}
                    className="vk-btn vk-tree-more"
                    data-size="sm"
                    data-variant="ghost"
                    onClick={() => props.onToggleConversationList(props.project.id)}
                    type="button"
                  >
                    {props.isConversationListExpanded ? <ChevronsUp aria-hidden="true" /> : <ChevronsDown aria-hidden="true" />}
                    {conversationOverflowLabel}
                  </button>
                </SidebarMenuItem>
              ) : null}
            </>
          ) : (
            <SidebarMenuItem className="vk-tree-item">
              <button
                className="vk-tree-session vk-tree-start"
                onClick={() => props.onCreateConversation(props.project)}
                title={t("projectMenu.newConversation")}
                type="button"
              >
                <span className="vk-tree-lead"><Plus aria-hidden="true" /></span>
                <span className="vk-tree-title"><span>{t("projectRow.noConversations")}</span></span>
                <small className="vk-tree-time">{t("projectMenu.newConversation")}</small>
              </button>
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
