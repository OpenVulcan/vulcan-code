import React from "react";
import { Settings } from "lucide-react";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/components/ui/tooltip";
import { VulcanWordmark } from "@/components/shell/vulcan-wordmark";
import { useI18n } from "@/i18n";

// SidebarBrandMenu renders the panel title strip as one line: the wordmark on the logo's centre line and the current area on the right.
// The brand mark and settings entry live on the activity rail.
// SidebarBrandMenu 将面板标题条渲染为单行：字标与标志中线对齐，当前区域位于右侧。品牌标记与设置入口位于活动轨道上。
export function SidebarBrandMenu(props: {
  // Label shown as the compact current area subtitle.
  // 作为紧凑当前区域副标题展示的文案。
  currentLabel: string;
  // Whether the title should behave as a native window drag region.
  // 标题是否应作为原生窗口拖动区域。
  titlebarDragRegion?: boolean;
  // Handles title bar double-click maximize or restore behavior.
  // 处理标题栏双击最大化或还原行为。
  onTitlebarDoubleClick?: (event: React.MouseEvent<HTMLElement>) => void;
}) {
  return (
    <div className="sidebar-profile-button sidebar-brand-button sidebar-brand-static" aria-label="Vulcan Code">
      <div
        className="sidebar-brand-drag-region"
        data-tauri-drag-region={props.titlebarDragRegion ? "" : undefined}
        onDoubleClick={props.titlebarDragRegion ? undefined : props.onTitlebarDoubleClick}
      >
        <span className="vcn-sidebar-label sidebar-profile-label sidebar-brand-label">
          <VulcanWordmark className="sidebar-brand-wordmark" label="Vulcan Code" />
          <small>{props.currentLabel}</small>
        </span>
      </div>
    </div>
  );
}

// SidebarUserMenu renders the user account entry pinned to the bottom of the activity rail.
// SidebarUserMenu 渲染固定在活动轨道底部的用户账户入口。
export function SidebarUserMenu(props: {
  // Opens the settings area from the user menu.
  // 从用户菜单打开设置区域。
  onOpenSettings: () => void;
}) {
  // t resolves stable WebUI copy keys for the active language.
  // t 根据当前语言解析稳定的 WebUI 文案键。
  const { t } = useI18n();
  return (
    <DropdownMenu>
      <Tooltip>
        <TooltipTrigger asChild>
          <DropdownMenuTrigger asChild>
            <button aria-label={t("sidebar.user.local")} className="activity-rail-button sidebar-user-button" type="button">
              <span aria-hidden="true" className="sidebar-user-avatar">V</span>
            </button>
          </DropdownMenuTrigger>
        </TooltipTrigger>
        <TooltipContent side="right">{t("sidebar.user.local")}</TooltipContent>
      </Tooltip>
      <DropdownMenuContent align="end" className="sidebar-profile-menu" side="right" sideOffset={8}>
        <DropdownMenuLabel className="sidebar-user-menu-label">
          <span aria-hidden="true" className="sidebar-user-avatar">V</span>
          <span>
            <strong>{t("sidebar.user.local")}</strong>
            <small>Vulcan Code</small>
          </span>
        </DropdownMenuLabel>
        <DropdownMenuSeparator />
        <DropdownMenuGroup>
          <DropdownMenuItem className="sidebar-profile-menu-item" onSelect={props.onOpenSettings}>
            <Settings size={15} />
            <span>{t("area.settings")}</span>
          </DropdownMenuItem>
        </DropdownMenuGroup>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
