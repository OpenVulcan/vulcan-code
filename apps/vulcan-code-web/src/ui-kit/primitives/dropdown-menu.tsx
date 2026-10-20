import * as React from "react";
import {
  MenuCheckboxItem,
  MenuContent,
  MenuGroup,
  MenuItem,
  MenuLabel,
  MenuRadioGroup,
  MenuRadioItem,
  MenuRoot,
  MenuSeparator,
  MenuShortcut,
  MenuSub,
  MenuSubContent,
  MenuSubTrigger,
  MenuTrigger,
} from "./menu";

// Dropdown menus open from a button and list commands for it; the engine lives in ./menu.
// 下拉菜单从按钮打开并列出该按钮的命令；引擎位于 ./menu。

// DropdownMenu holds the open state of one dropdown.
// DropdownMenu 保存单个下拉菜单的打开状态。
function DropdownMenu(props: Omit<React.ComponentProps<typeof MenuRoot>, "slot">) {
  return <MenuRoot slot="dropdown-menu" {...props} />;
}

// DropdownMenuPortal is kept for API compatibility; menu surfaces always render in their own layer.
// DropdownMenuPortal 为兼容 API 而保留；菜单表面始终渲染在独立层中。
function DropdownMenuPortal({ children }: { children?: React.ReactNode; container?: HTMLElement | null; forceMount?: boolean }) {
  return <>{children}</>;
}

const DropdownMenuTrigger = MenuTrigger;
const DropdownMenuContent = MenuContent;
const DropdownMenuGroup = MenuGroup;
const DropdownMenuLabel = MenuLabel;
const DropdownMenuItem = MenuItem;
const DropdownMenuCheckboxItem = MenuCheckboxItem;
const DropdownMenuRadioGroup = MenuRadioGroup;
const DropdownMenuRadioItem = MenuRadioItem;
const DropdownMenuSeparator = MenuSeparator;
const DropdownMenuShortcut = MenuShortcut;
const DropdownMenuSub = MenuSub;
const DropdownMenuSubTrigger = MenuSubTrigger;
const DropdownMenuSubContent = MenuSubContent;

export {
  DropdownMenu,
  DropdownMenuPortal,
  DropdownMenuTrigger,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuLabel,
  DropdownMenuItem,
  DropdownMenuCheckboxItem,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuSeparator,
  DropdownMenuShortcut,
  DropdownMenuSub,
  DropdownMenuSubTrigger,
  DropdownMenuSubContent,
};
