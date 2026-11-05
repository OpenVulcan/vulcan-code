import React from "react";
import { Settings, UserRound } from "lucide-react";
import { AccountAvatar } from "@/components/features/account/account-avatar";
import { OpenVulcanAccountContext } from "@/components/features/account/openvulcan-account-context";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/ui-kit/primitives/dropdown-menu";
import { Tooltip, TooltipContent, TooltipTrigger } from "@/ui-kit/primitives/tooltip";
import { VulcanWordmark } from "@/components/shell/vulcan-wordmark";
import { useI18n } from "@/i18n";

// SidebarBrandMenu renders the panel title strip as one line: the wordmark on the logo's centre line and the current area on the right.
// The brand mark and settings entry live on the activity rail.
// SidebarBrandMenu 将面板标题条渲染为单行：字标与标志中线对齐，当前区域位于右侧。品牌标记与设置入口位于活动轨道上。
export function SidebarBrandMenu(props: {
  // Label shown as the compact current area subtitle; omitted where the actions already name the area.
  // 作为紧凑当前区域副标题展示的文案；操作已表明区域时省略。
  currentLabel?: string;
  // Whether the title should behave as a native window drag region.
  // 标题是否应作为原生窗口拖动区域。
  titlebarDragRegion?: boolean;
  // Handles title bar double-click maximize or restore behavior.
  // 处理标题栏双击最大化或还原行为。
  onTitlebarDoubleClick?: (event: React.MouseEvent<HTMLElement>) => void;
  // Compact entry placed right after the wordmark, such as the update entry; it must stop its own
  // double clicks so they do not reach the title strip.
  // 紧跟字标放置的紧凑入口，例如更新入口；它必须自行截止双击，避免传到标题条。
  afterWordmark?: React.ReactNode;
  // Controls placed after the drag area, such as the panel flow switch.
  // 放在拖动区域之后的控件，例如面板流切换。
  actions?: React.ReactNode;
}) {
  return (
    <div className="vk-brand" aria-label="Vulcan Code">
      <div
        className="vk-brand-drag"
        data-tauri-drag-region={props.titlebarDragRegion ? "" : undefined}
        onDoubleClick={props.titlebarDragRegion ? undefined : props.onTitlebarDoubleClick}
      >
        <VulcanWordmark className="vk-wordmark" label="Vulcan Code" />
        {props.afterWordmark}
        {props.currentLabel ? <small>{props.currentLabel}</small> : null}
      </div>
      {props.actions}
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
  // account supplies the global identity and the reusable sign-in entry.
  // account 提供全局身份及可重复进入的登录入口。
  const account = React.useContext(OpenVulcanAccountContext);
  // label uses the verified public name when connected, otherwise the existing local-user label.
  // label 在已连接时使用验证过的公开名称，否则使用既有本地用户标签。
  const label = account?.snapshot?.displayName ?? t("sidebar.user.local");
  return (
    <DropdownMenu>
      <Tooltip>
        <TooltipTrigger asChild>
          <DropdownMenuTrigger asChild>
            <button aria-label={label} className="vk-rail-btn" type="button">
              <AccountAvatar name={account?.snapshot?.displayName} className="vk-rail-avatar" />
            </button>
          </DropdownMenuTrigger>
        </TooltipTrigger>
        <TooltipContent side="right">{label}</TooltipContent>
      </Tooltip>
      <DropdownMenuContent align="end" side="right" sideOffset={8}>
        <DropdownMenuLabel className="vk-account-label">
          <AccountAvatar name={account?.snapshot?.displayName} className="vk-rail-avatar" />
          <span>
            <strong>{label}</strong>
            <small>Vulcan Code</small>
          </span>
        </DropdownMenuLabel>
        <DropdownMenuSeparator />
        <DropdownMenuGroup>
          {account ? <DropdownMenuItem onSelect={() => account.setOpen(true)}>
            <UserRound size={15} />
            <span>{account.snapshot?.connectionId ? t("openvulcan.account.manage") : t("openvulcan.account.login")}</span>
          </DropdownMenuItem> : null}
          <DropdownMenuItem onSelect={props.onOpenSettings}>
            <Settings size={15} />
            <span>{t("area.settings")}</span>
          </DropdownMenuItem>
        </DropdownMenuGroup>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
