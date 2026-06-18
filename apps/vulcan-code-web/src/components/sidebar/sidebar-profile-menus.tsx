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
              aria-label="打开设置"
              className="sidebar-brand-settings-button"
              onClick={() => props.onSelectArea("settings")}
              title="设置"
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
  return (
    <SidebarMenu>
      <SidebarMenuItem>
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <SidebarMenuButton className="sidebar-profile-button sidebar-user-button" tooltip="本地用户">
              <span aria-hidden="true" className="sidebar-user-avatar">V</span>
              <span className="vcn-sidebar-label sidebar-profile-label">
                <strong>本地用户</strong>
                <small>Vulcan Code</small>
              </span>
              <ChevronsUpDown className="sidebar-profile-chevron" size={15} />
            </SidebarMenuButton>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end" className="sidebar-profile-menu" side="right" sideOffset={6}>
            <DropdownMenuLabel className="sidebar-user-menu-label">
              <span aria-hidden="true" className="sidebar-user-avatar">V</span>
              <span>
                <strong>本地用户</strong>
                <small>Vulcan Code</small>
              </span>
            </DropdownMenuLabel>
            <DropdownMenuSeparator />
            <DropdownMenuGroup>
              <DropdownMenuItem className="sidebar-profile-menu-item">
                <BadgeCheck size={15} />
                <span>账户</span>
              </DropdownMenuItem>
              <DropdownMenuItem className="sidebar-profile-menu-item" onClick={props.onOpenSettings}>
                <Settings size={15} />
                <span>设置</span>
              </DropdownMenuItem>
            </DropdownMenuGroup>
            <DropdownMenuSeparator />
            <DropdownMenuItem className="sidebar-profile-menu-item muted">
              <LogOut size={15} />
              <span>退出登录</span>
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      </SidebarMenuItem>
    </SidebarMenu>
  );
}
