import React from "react";
import {
  BadgeCheck,
  ChevronsUpDown,
  LogOut,
  Settings,
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
} from "@/components/ui/sidebar";
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
}) {
  // t resolves stable WebUI copy keys for the active language.
  // t 根据当前语言解析稳定的 WebUI 文案键。
  const { t } = useI18n();
  return (
    <SidebarMenu>
      <SidebarMenuItem>
        <div className="sidebar-profile-button sidebar-brand-button sidebar-brand-static" aria-label="Vulcan Code">
          <span aria-hidden="true" className="brand-mark sidebar-brand-mark">V</span>
          <span className="vcn-sidebar-label sidebar-profile-label">
            <strong>Vulcan Code</strong>
            <small>{props.currentLabel}</small>
          </span>
          {props.activeArea !== "settings" ? (
            <button
              aria-label={t("global.openSettings")}
              className="sidebar-brand-settings-button"
              onClick={() => props.onSelectArea("settings")}
              title={t("area.settings")}
              type="button"
            >
              <Settings size={15} />
            </button>
          ) : null}
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
              <DropdownMenuItem className="sidebar-profile-menu-item">
                <BadgeCheck size={15} />
                <span>{t("sidebar.user.account")}</span>
              </DropdownMenuItem>
              <DropdownMenuItem className="sidebar-profile-menu-item" onClick={props.onOpenSettings}>
                <Settings size={15} />
                <span>{t("area.settings")}</span>
              </DropdownMenuItem>
            </DropdownMenuGroup>
            <DropdownMenuSeparator />
            <DropdownMenuItem className="sidebar-profile-menu-item muted">
              <LogOut size={15} />
              <span>{t("sidebar.user.logout")}</span>
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      </SidebarMenuItem>
    </SidebarMenu>
  );
}
