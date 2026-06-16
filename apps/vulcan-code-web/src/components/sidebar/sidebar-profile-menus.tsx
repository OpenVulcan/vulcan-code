import React from "react";
import {
  BadgeCheck,
  BrainCircuit,
  Check,
  ChevronsUpDown,
  Hammer,
  House,
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

// SidebarBrandMenu renders the shadcn sidebar-07 style product switcher.
// SidebarBrandMenu 渲染 shadcn sidebar-07 风格的产品切换入口。
export function SidebarBrandMenu(props: {
  // Active top-level area highlighted in the brand menu.
  // 品牌菜单中高亮的当前顶层区域。
  activeArea: MainAreaId;
  // Label shown as the compact current area subtitle.
  // 作为紧凑当前区域副标题展示的文案。
  currentLabel: string;
  // Selects one top-level area from the brand menu.
  // 从品牌菜单中选择一个顶层区域。
  onSelectArea: (area: MainAreaId) => void;
}) {
  const menuItems = sidebarBrandMenuItems();
  return (
    <SidebarMenu>
      <SidebarMenuItem>
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <SidebarMenuButton className="sidebar-profile-button sidebar-brand-button" tooltip="Vulcan Code">
              <span aria-hidden="true" className="brand-mark sidebar-brand-mark">V</span>
              <span className="vcn-sidebar-label sidebar-profile-label">
                <strong>Vulcan Code</strong>
                <small>{props.currentLabel}</small>
              </span>
              <ChevronsUpDown className="sidebar-profile-chevron" size={15} />
            </SidebarMenuButton>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="start" className="sidebar-profile-menu" side="right" sideOffset={6}>
            <DropdownMenuLabel>切换区域</DropdownMenuLabel>
            <DropdownMenuGroup>
              {menuItems.map((item) => {
                const Icon = item.icon;
                return (
                  <DropdownMenuItem
                    className="sidebar-profile-menu-item"
                    key={item.id}
                    onClick={() => props.onSelectArea(item.id)}
                  >
                    <Icon size={15} />
                    <span>{item.label}</span>
                    {props.activeArea === item.id ? <Check className="sidebar-profile-menu-check" size={14} /> : null}
                  </DropdownMenuItem>
                );
              })}
            </DropdownMenuGroup>
          </DropdownMenuContent>
        </DropdownMenu>
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

// sidebarBrandMenuItems returns stable top-level navigation choices.
// sidebarBrandMenuItems 返回稳定的顶层导航选项。
function sidebarBrandMenuItems(): Array<{ id: MainAreaId; label: string; icon: React.ComponentType<{ size?: number }> }> {
  return [
    { id: "chat", label: "工作台", icon: House },
    { id: "skills", label: "技能", icon: Hammer },
    { id: "memory", label: "记忆", icon: BrainCircuit },
    { id: "settings", label: "设置", icon: Settings },
  ];
}
