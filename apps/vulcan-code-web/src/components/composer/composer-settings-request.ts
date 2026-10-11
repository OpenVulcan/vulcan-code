import React from "react";
import { DropdownMenuSub } from "@/components/ui/dropdown-menu";

// ComposerSettingsSection names one conversation-settings submenu a slash command can open.
// ComposerSettingsSection 命名 slash 指令可打开的一个对话设置子菜单。
export type ComposerSettingsSection =
  | "style"
  | "answer"
  | "knowledge"
  | "semantic"
  | "tools"
  | "thinking"
  | "memory";

// ComposerSettingsRequest asks the settings menu to open, optionally at one submenu.
// ComposerSettingsRequest 请求打开设置菜单，可选地直接打开某个子菜单。
export interface ComposerSettingsRequest {
  // Submenu to open, or null for the menu itself.
  // 要打开的子菜单；为 null 时只打开菜单本身。
  section: ComposerSettingsSection | null;
  // Distinguishes repeated requests for the same section.
  // 区分对同一子菜单的重复请求。
  nonce: number;
}

// ComposerSettingsRequestContext carries the section requested while the settings menu is open.
// ComposerSettingsRequestContext 在设置菜单打开期间携带被请求的子菜单。
export const ComposerSettingsRequestContext = React.createContext<ComposerSettingsSection | null>(null);

// useRequestedSettingsSub opens one submenu when a slash command requested it.
// useRequestedSettingsSub 在 slash 指令请求时打开对应子菜单。
// Parameters: section identifies the submenu; onOpenChange observes every open transition,
// including the programmatic one that Radix does not report.
// 参数：section 标识子菜单；onOpenChange 观察每次打开状态变化，包括 Radix 不会上报的程序化打开。
// Returns: controlled props for DropdownMenuSub.
// 返回值：DropdownMenuSub 的受控属性。
function useRequestedSettingsSub(
  section: ComposerSettingsSection,
  onOpenChange?: (open: boolean) => void,
): { open: boolean; onOpenChange: (open: boolean) => void } {
  const requested = React.useContext(ComposerSettingsRequestContext) === section;
  const [open, setOpen] = React.useState(false);
  const observer = React.useRef(onOpenChange);
  observer.current = onOpenChange;
  React.useEffect(() => {
    if (requested) {
      setOpen(true);
      observer.current?.(true);
    }
  }, [requested]);
  const change = React.useCallback((next: boolean) => {
    setOpen(next);
    observer.current?.(next);
  }, []);
  return { open, onOpenChange: change };
}

// RequestedSettingsSub is a DropdownMenuSub that a slash command can open directly.
// RequestedSettingsSub 是可由 slash 指令直接打开的 DropdownMenuSub。
export function RequestedSettingsSub(props: {
  // Submenu identity matched against the active request.
  // 与当前请求匹配的子菜单标识。
  section: ComposerSettingsSection;
  // Optional observer of open transitions.
  // 可选的打开状态观察者。
  onOpenChange?: (open: boolean) => void;
  children?: React.ReactNode;
}) {
  const controlled = useRequestedSettingsSub(props.section, props.onOpenChange);
  return React.createElement(DropdownMenuSub, controlled, props.children);
}
