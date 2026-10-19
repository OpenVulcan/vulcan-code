import React from "react";
import { Ellipsis, MessagesSquare, PanelLeftOpen, Pin, Settings } from "lucide-react";
import { toast } from "@/ui-kit/primitives/toast";

import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/ui-kit/primitives/dropdown-menu";
import { useSidebar, useSidebarNavigation } from "@/ui-kit/shell/sidebar";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/ui-kit/primitives/tooltip";
import { SidebarUserMenu } from "@/components/sidebar/sidebar-profile-menus";
import { VulcanSymbol } from "@/components/shell/vulcan-symbol";
import { formatMessage, useI18n } from "@/i18n";
import { readActivityRailPins, saveActivityRailPins } from "./activity-rail-preferences";
import { ActivityRailFlyout } from "./activity-rail-flyout";

// ActivityRailEntry describes one dynamic destination that the user may pin to the rail.
// ActivityRailEntry 描述一个可由用户固定到轨道的动态目标。
export type ActivityRailEntry = {
  // Stable id used for pin persistence; plugin pages use their manifest-derived sub area id.
  // 用于固定状态持久化的稳定标识；插件页面使用由清单生成的子区域标识。
  id: string;
  // Localized destination label.
  // 本地化的目标名称。
  label: string;
  // Icon shown on the rail and in the overflow menu.
  // 显示在轨道与溢出菜单中的图标。
  icon: React.ReactNode;
  // Whether the destination is the page currently shown.
  // 该目标是否为当前显示的页面。
  active: boolean;
  // Broader entry whose highlight yields to this entry while this entry is pinned.
  // 当本入口已固定时，让出高亮的上级入口。
  parentId?: string;
  // Opens the destination.
  // 打开该目标。
  onSelect: () => void;
};

// ActivityRail renders the always-visible primary navigation beside the collapsible panel.
// ActivityRail 渲染位于可折叠面板旁、始终可见的主导航轨道。
export function ActivityRail(props: {
  // Every destination currently available, in rail order; unavailable plugin pages are simply absent.
  // 当前可用的全部目标，按轨道顺序排列；不可用的插件页面不会出现。
  entries: readonly ActivityRailEntry[];
  // Whether the workspace conversation surface is the current page; the panel beside the rail is then the session panel.
  // 工作台会话界面是否为当前页面；此时轨道旁的面板即会话面板。
  sessionsActive: boolean;
  // Whether the settings area is the current page.
  // 设置区域是否为当前页面。
  settingsActive: boolean;
  // Opens the workspace conversation surface.
  // 打开工作台会话界面。
  onSelectSessions: () => void;
  // Opens the settings area.
  // 打开设置区域。
  onSelectSettings: () => void;
  // The session panel's own tree, shown in the flyout while the session area is open with its panel collapsed.
  // 会话面板自身的会话树；处于会话区域且面板已折叠时显示在浮层中。
  sessionTree: React.ReactNode;
  // Controls shown in the session flyout heading, such as the panel flow switch.
  // 显示在会话浮层标题中的控件，例如面板流切换。
  sessionTreeActions?: React.ReactNode;
  // Selected conversation; choosing another one inside the flyout closes it.
  // 当前会话；在浮层中选择其他会话时浮层关闭。
  selectedConversationId: string | null;
  // Whether the brand mark should be a native window drag region.
  // 品牌标记是否作为原生窗口拖动区域。
  titlebarDragRegion?: boolean;
  // Handles title bar double-click maximize or restore behavior.
  // 处理标题栏双击最大化或还原行为。
  onTitlebarDoubleClick?: (event: React.MouseEvent<HTMLElement>) => void;
}) {
  const { t } = useI18n();
  // sidebar is the panel beside this rail: sessions, a feature menu, or the settings tree.
  // sidebar 是轨道旁的面板：会话面板、功能菜单或设置树。
  const sidebar = useSidebar();
  // navigate closes the phone drawer before leaving for the chosen destination.
  // navigate 在前往选定目标前关闭手机抽屉。
  const navigate = useSidebarNavigation();
  // pins is the user's explicit rail layout, or the in-memory default before any choice.
  // pins 是用户明确选择的轨道布局；尚未选择时为内存默认值。
  const [pins, setPins] = React.useState<readonly string[]>(readActivityRailPins);
  const pinnedIds = React.useMemo(() => new Set(pins), [pins]);
  const pinnedEntries = props.entries.filter((entry) => pinnedIds.has(entry.id));
  const overflowEntries = props.entries.filter((entry) => !pinnedIds.has(entry.id));
  // highlightedId picks the most specific active pinned entry, so a pinned plugin page outranks its parent area.
  // highlightedId 选择最具体的已固定激活入口，使已固定插件页面优先于其上级区域。
  const highlightedId = pinnedEntries.find((entry) => entry.active && entry.parentId)?.id
    ?? pinnedEntries.find((entry) => entry.active)?.id
    ?? null;
  // overflowActive lights the overflow trigger when the current page is reachable only through it.
  // overflowActive 在当前页面只能经由溢出菜单到达时点亮溢出按钮。
  const overflowActive = highlightedId === null && overflowEntries.some((entry) => entry.active);

  // togglePin records an explicit user choice; storage failure keeps the previous layout visible.
  // togglePin 记录用户的明确选择；存储失败时保留原有布局。
  const togglePin = (entryId: string) => {
    const next = pinnedIds.has(entryId) ? pins.filter((id) => id !== entryId) : [...pins, entryId];
    try {
      saveActivityRailPins(next);
      setPins(next);
    } catch (error) {
      toast.error(formatMessage(t("activityRail.pinSaveFailed"), { error: error instanceof Error ? error.message : String(error) }));
    }
  };

  // selectSessions toggles the session panel when already there; it is the only panel that collapses.
  // selectSessions 在已位于会话界面时切换会话面板；它是唯一可折叠的面板。
  const selectSessions = () => {
    if (!sidebar.isMobile && props.sessionsActive) {
      sidebar.toggleSidebar();
      return;
    }
    navigate(props.onSelectSessions)();
  };

  // sessionFlyoutEnabled previews sessions on hover only inside the session area while its panel is collapsed;
  // on any other page the sessions button simply navigates back to the session area.
  // sessionFlyoutEnabled 仅在处于会话区域且会话面板已折叠时通过悬停预览会话；
  // 在其他页面中，会话按钮只负责返回会话区域。
  const sessionFlyoutEnabled = !sidebar.isMobile && props.sessionsActive && sidebar.state === "collapsed";
  const sessionsButton = (
    <ActivityRailButton
      active={props.sessionsActive}
      icon={<MessagesSquare size={18} />}
      label={t("area.chat")}
      onClick={selectSessions}
      tooltip={!sessionFlyoutEnabled}
    />
  );

  return (
    <nav aria-label={t("activityRail.label")} className="vk-rail">
      <div
        className="vk-rail-brand"
        data-tauri-drag-region={props.titlebarDragRegion ? "" : undefined}
        onDoubleClick={props.titlebarDragRegion ? undefined : props.onTitlebarDoubleClick}
      >
        <VulcanSymbol className="vk-rail-logo" />
      </div>
      <div className="vk-rail-group" data-scroll="">
        {sessionFlyoutEnabled ? (
          <ActivityRailFlyout
            content={props.sessionTree}
            dismissKey={props.selectedConversationId}
            headerActions={props.sessionTreeActions}
            footer={{ label: t("activityRail.showSessionPanel"), icon: <PanelLeftOpen size={15} />, onSelect: () => sidebar.setOpen(true) }}
            title={t("activityRail.conversations")}
          >
            {sessionsButton}
          </ActivityRailFlyout>
        ) : sessionsButton}
        <span aria-hidden="true" className="vk-rail-divider" />
        {pinnedEntries.map((entry) => (
          <ActivityRailButton
            active={entry.id === highlightedId}
            icon={entry.icon}
            key={entry.id}
            label={entry.label}
            onClick={navigate(entry.onSelect)}
          />
        ))}
        <DropdownMenu>
          <Tooltip>
            <TooltipTrigger asChild>
              <DropdownMenuTrigger asChild>
                <button
                  aria-label={t("activityRail.more")}
                  aria-current={overflowActive ? "page" : undefined}
                  className="vk-rail-btn"
                  data-active={overflowActive ? "true" : "false"}
                  type="button"
                >
                  <Ellipsis size={18} />
                </button>
              </DropdownMenuTrigger>
            </TooltipTrigger>
            <TooltipContent side="right">{t("activityRail.more")}</TooltipContent>
          </Tooltip>
          <DropdownMenuContent align="start" side="right" sideOffset={8}>
            {overflowEntries.length > 0 ? (
              <>
                <DropdownMenuGroup>
                  {overflowEntries.map((entry) => (
                    <DropdownMenuItem
                      data-current={entry.active ? "" : undefined}
                      key={entry.id}
                      onSelect={navigate(entry.onSelect)}
                    >
                      {entry.icon}
                      <span>{entry.label}</span>
                    </DropdownMenuItem>
                  ))}
                </DropdownMenuGroup>
                <DropdownMenuSeparator />
              </>
            ) : null}
            <DropdownMenuLabel>
              <Pin size={13} />
              <span>{t("activityRail.customize")}</span>
            </DropdownMenuLabel>
            <DropdownMenuGroup>
              {props.entries.map((entry) => (
                <DropdownMenuCheckboxItem
                  checked={pinnedIds.has(entry.id)}
                  key={entry.id}
                  // Keep the menu open so several entries can be checked in one visit.
                  // 保持菜单打开，便于一次勾选多个入口。
                  onSelect={(event) => event.preventDefault()}
                  onCheckedChange={() => togglePin(entry.id)}
                >
                  {entry.icon}
                  <span>{entry.label}</span>
                </DropdownMenuCheckboxItem>
              ))}
            </DropdownMenuGroup>
          </DropdownMenuContent>
        </DropdownMenu>
      </div>
      <div className="vk-rail-group" data-end="">
        <ActivityRailButton
          active={props.settingsActive}
          icon={<Settings size={18} />}
          label={t("area.settings")}
          onClick={navigate(props.onSelectSettings)}
        />
        <SidebarUserMenu onOpenSettings={navigate(props.onSelectSettings)} />
      </div>
    </nav>
  );
}

// ActivityRailButton renders one square rail destination with a right-side tooltip.
// ActivityRailButton 渲染单个方形轨道目标，并在右侧显示提示。
function ActivityRailButton({ active, icon, label, onClick, tooltip = true }: {
  // Whether the destination is current.
  // 该目标是否为当前页面。
  active: boolean;
  // Destination icon.
  // 目标图标。
  icon: React.ReactNode;
  // Accessible label and tooltip text.
  // 无障碍名称与提示文本。
  label: string;
  // Activates the destination.
  // 激活该目标。
  onClick: () => void;
  // Whether to show the hover tooltip; a flyout trigger supplies its own titled surface instead.
  // 是否显示悬停提示；浮层触发器自带标题浮层，因此不需要。
  tooltip?: boolean;
}) {
  const button = (
    <button
      aria-current={active ? "page" : undefined}
      aria-label={label}
      className="vk-rail-btn"
      data-active={active ? "true" : "false"}
      onClick={onClick}
      type="button"
    >
      {icon}
    </button>
  );
  if (!tooltip) {
    return button;
  }
  return (
    <Tooltip>
      <TooltipTrigger asChild>{button}</TooltipTrigger>
      <TooltipContent side="right">{label}</TooltipContent>
    </Tooltip>
  );
}
