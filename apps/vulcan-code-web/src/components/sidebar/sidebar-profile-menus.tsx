import React from "react";
import { CircleDot, GitPullRequest, MessageSquare, Settings, UserRound } from "lucide-react";
import { AccountAvatar } from "@/components/features/account/account-avatar";
import { OpenVulcanAccountContext } from "@/components/features/account/openvulcan-account-context";
import { FeedbackCenterContext } from "@/components/features/feedback/feedback-center-context";
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
import { openExternalUrl } from "@/service/project-actions-service";
import { toast } from "@/ui-kit/primitives/toast";

// VULCAN_CODE_REPOSITORY is the public repository that takes GitHub suggestions and pull requests.
// VULCAN_CODE_REPOSITORY 是接收 GitHub 建议与拉取请求的公开仓库。
const VULCAN_CODE_REPOSITORY = "https://github.com/OpenVulcan/vulcan-code";

// GITHUB_ISSUE_URL opens a new issue; GITHUB_PULL_REQUEST_URL opens the compare page that starts a pull request.
// GITHUB_ISSUE_URL 打开新建 Issue；GITHUB_PULL_REQUEST_URL 打开用于发起拉取请求的比较页。
const GITHUB_ISSUE_URL = `${VULCAN_CODE_REPOSITORY}/issues/new`;
const GITHUB_PULL_REQUEST_URL = `${VULCAN_CODE_REPOSITORY}/compare`;

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
  // feedback opens the feedback window where the workbench provides it; the message center has its own rail entry.
  // feedback 在工作台提供时打开反馈窗口；消息中心有自己的轨道入口。
  const feedback = React.useContext(FeedbackCenterContext);
  // label uses the verified public name when connected, otherwise the existing local-user label.
  // label 在已连接时使用验证过的公开名称，否则使用既有本地用户标签。
  const label = account?.snapshot?.displayName ?? t("sidebar.user.local");
  // openGitHub opens one repository page in the system browser and reports a host failure.
  // openGitHub 在系统浏览器中打开一个仓库页面，并报告宿主失败。
  const openGitHub = (url: string) => {
    void openExternalUrl(url).catch((error: unknown) => {
      toast.error(`${t("sidebar.user.openLinkFailed")}${error instanceof Error ? error.message : String(error)}`);
    });
  };
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
          {feedback ? <DropdownMenuItem onSelect={() => feedback.openFeedback()}>
            <MessageSquare size={15} />
            <span>{t("feedback.menu")}</span>
          </DropdownMenuItem> : null}
        </DropdownMenuGroup>
        <DropdownMenuSeparator />
        <DropdownMenuGroup>
          <DropdownMenuItem onSelect={() => openGitHub(GITHUB_ISSUE_URL)}>
            <CircleDot size={15} />
            <span>{t("sidebar.user.githubIssue")}</span>
          </DropdownMenuItem>
          <DropdownMenuItem onSelect={() => openGitHub(GITHUB_PULL_REQUEST_URL)}>
            <GitPullRequest size={15} />
            <span>{t("sidebar.user.githubPullRequest")}</span>
          </DropdownMenuItem>
        </DropdownMenuGroup>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
