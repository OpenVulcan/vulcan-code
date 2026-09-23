import React from "react";
import {
  ChevronsUpDown,
  Settings,
  Undo2,
} from "lucide-react";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import {
  SidebarMenu,
  SidebarMenuButton,
  SidebarMenuItem,
  useSidebar,
} from "@/components/ui/sidebar";
import { VulcanSymbol } from "@/components/shell/vulcan-symbol";
import { useI18n } from "@/i18n";
import type { MainAreaId } from "@/types";

// SidebarBrandMenu renders the static product header and optional settings shortcut.
// SidebarBrandMenu 渲染静态产品头部与可选设置快捷入口。
export function SidebarBrandMenu(props: {
  // Active top-level area used to hide redundant shortcuts.
  // 用于隐藏重复快捷入口的当前顶层区域。
  activeArea: MainAreaId;
  // Label shown as the compact current area subtitle.
  // 作为紧凑当前区域副标题展示的文案。
  currentLabel: string;
  // Selects one top-level area from header shortcuts.
  // 从头部快捷入口选择一个顶层区域。
  onSelectArea: (area: MainAreaId) => void;
  // Whether the brand content should behave as a native window drag region.
  // 品牌内容是否应作为原生窗口拖动区域。
  titlebarDragRegion?: boolean;
  // Handles title bar double-click maximize or restore behavior.
  // 处理标题栏双击最大化或还原行为。
  onTitlebarDoubleClick?: (event: React.MouseEvent<HTMLElement>) => void;
}) {
  // t resolves stable WebUI copy keys for the active language.
  // t 根据当前语言解析稳定的 WebUI 文案键。
  const { t } = useI18n();
  // state hides the clipped header shortcut from both pointer and keyboard navigation in icon mode.
  // state 在图标模式中同时从指针和键盘导航移除被裁剪的头部快捷入口。
  const { state } = useSidebar();
  return (
    <SidebarMenu>
      <SidebarMenuItem>
        <div className="sidebar-profile-button sidebar-brand-button sidebar-brand-static" aria-label="Vulcan Code">
          <div
            className="sidebar-brand-drag-region"
            data-tauri-drag-region={props.titlebarDragRegion ? "" : undefined}
            onDoubleClick={props.titlebarDragRegion ? undefined : props.onTitlebarDoubleClick}
          >
            <VulcanSymbol className="brand-logo sidebar-brand-logo" />
            <span className="vcn-sidebar-label sidebar-profile-label">
              <strong>Vulcan Code</strong>
              <small>{props.currentLabel}</small>
            </span>
          </div>
          {/* The header action keeps settings and back navigation in the same expanded slot. */}
          {/* 头部动作让设置与返回导航共用展开态的同一位置。 */}
          {state === "expanded" ? <button
            aria-label={props.activeArea === "settings" ? t("settings.backToWorkspace") : t("global.openSettings")}
            className="sidebar-brand-settings-button"
            onClick={() => props.onSelectArea(props.activeArea === "settings" ? "chat" : "settings")}
            onDoubleClick={(event) => event.stopPropagation()}
            title={props.activeArea === "settings" ? t("settings.backToWorkspace") : t("area.settings")}
            type="button"
          >
            {props.activeArea === "settings" ? <Undo2 size={15} /> : <Settings size={15} />}
          </button> : null}
        </div>
      </SidebarMenuItem>
    </SidebarMenu>
  );
}

// SidebarUserMenu renders the fixed bottom user account switcher.
// SidebarUserMenu 渲染固定在底部的用户账户入口。
export function SidebarUserMenu(props: {
  // Opens the settings area from the user menu.
  // 从用户菜单打开设置区域。
  onOpenSettings: () => void;
}) {
  // t resolves stable WebUI copy keys for the active language.
  // t 根据当前语言解析稳定的 WebUI 文案键。
  const { t } = useI18n();
  return (
    <SidebarMenu>
      <SidebarMenuItem>
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <SidebarMenuButton className="sidebar-profile-button sidebar-user-button" tooltip={t("sidebar.user.local")}>
              <span aria-hidden="true" className="sidebar-user-avatar">V</span>
              <span className="vcn-sidebar-label sidebar-profile-label">
                <strong>{t("sidebar.user.local")}</strong>
                <small>Vulcan Code</small>
              </span>
              <ChevronsUpDown className="sidebar-profile-chevron" size={15} />
            </SidebarMenuButton>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end" className="sidebar-profile-menu" side="right" sideOffset={6}>
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
      </SidebarMenuItem>
    </SidebarMenu>
  );
}
