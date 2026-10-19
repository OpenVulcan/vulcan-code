import React from "react";
import { Button } from "@/ui-kit/primitives/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSub,
  DropdownMenuSubContent,
  DropdownMenuSubTrigger,
  DropdownMenuTrigger,
} from "@/ui-kit/primitives/dropdown-menu";
import { StatusPill } from "@/components/shared/status";
import { useI18n, type LocaleCode } from "@/i18n";
import { SUPPORTED_LOCALES } from "@/i18n-locales";
import { createShellClient } from "@/host-shell";
import type { StartupSnapshot } from "@/types";
import { toast } from "@/ui-kit/primitives/toast";
import {
  Check,
  ChevronDown,
  Minimize2,
  MinusIcon,
  Moon,
  PanelRightOpen,
  RefreshCw,
  SquareIcon,
  Sun,
  X,
} from "lucide-react";

// GITHUB_REPOSITORY_URL points to the public project repository.
// GITHUB_REPOSITORY_URL 指向公开项目仓库。
const GITHUB_REPOSITORY_URL = "https://github.com/OpenVulcan/vulcan-code";

// shellClient is the shell chrome facade used by local window controls.
// shellClient 是本地窗口控制使用的壳层 chrome 门面。
const shellClient = createShellClient();

// AppChromeThemeMode describes the theme modes accepted by the shell chrome controls.
// AppChromeThemeMode 描述外壳 chrome 控件接受的主题模式。
export type AppChromeThemeMode = "light" | "dark";

// GlobalActions renders app-wide controls in the primary menu bar.
// GlobalActions 渲染主菜单栏中的全局控制。
export function GlobalActions(props: {
  // Startup snapshot used for the compact online indicator.
  // 紧凑在线状态指示使用的启动快照。
  startup: StartupSnapshot;
  // Current shadcn theme mode.
  // 当前 shadcn 主题模式。
  theme: AppChromeThemeMode;
  // Current UI language.
  // 当前 UI 语言。
  locale: LocaleCode;
  // Updates the current UI language.
  // 更新当前 UI 语言。
  onChangeLocale: (locale: LocaleCode) => void;
  // Toggles between light and dark theme.
  // 在亮色与暗色主题之间切换。
  onToggleTheme: () => void;
}) {
  // t translates global action labels.
  // t 翻译全局操作标签。
  const { t } = useI18n();
  // activeLocaleEntry labels the current language in the menu trigger.
  // activeLocaleEntry 在菜单触发器中标注当前语言。
  const activeLocaleEntry = SUPPORTED_LOCALES.find((entry) => entry.code === props.locale) ?? SUPPORTED_LOCALES[0];
  // openDeveloperTools is present only in a desktop host that exposes the native action.
  // openDeveloperTools 仅在桌面宿主提供原生操作时存在。
  const openDeveloperTools = shellClient.openDeveloperTools;
  // reloadWorkbench refreshes this client document like F5 without restarting Core.
  // reloadWorkbench 像 F5 一样刷新当前客户端页面，不重启 Core。
  const reloadWorkbench = React.useCallback(() => window.location.reload(), []);
  // isPreviewMode detects fallback browser data instead of a live service bridge.
  // isPreviewMode 检测当前是否为浏览器回退预览数据而非真实服务桥接。
  const isPreviewMode = props.startup.phase === "browser-preview";
  // statusLabel translates internal startup phases into compact user-facing labels.
  // statusLabel 将内部启动阶段转换为面向用户的紧凑显示文案。
  const statusLabel = isPreviewMode ? t("global.preview") : props.startup.ready ? t("global.online") : props.startup.phase;
  // statusKind keeps preview visually distinct from a live ready service.
  // statusKind 让预览状态与真实在线服务在视觉上保持区分。
  const statusKind = isPreviewMode ? "warning" : props.startup.ready ? "ready" : "running";
  // selectTheme switches theme only when the requested mode differs from the current mode.
  // selectTheme 仅在目标主题与当前主题不同时切换主题。
  const selectTheme = React.useCallback((theme: AppChromeThemeMode) => {
    if (props.theme !== theme) {
      props.onToggleTheme();
    }
  }, [props]);
  // menuPointerInteractionRef records whether the current menu closure originated from a pointer action inside the menu.
  // menuPointerInteractionRef 记录当前菜单关闭是否源于菜单内部的鼠标操作。
  const menuPointerInteractionRef = React.useRef(false);
  // markMenuPointerInteraction marks main-menu and submenu pointer interactions before Radix closes the root menu.
  // markMenuPointerInteraction 在 Radix 关闭根菜单前标记主菜单与子菜单的鼠标操作。
  const markMenuPointerInteraction = React.useCallback(() => {
    menuPointerInteractionRef.current = true;
  }, []);
  // markMenuKeyboardInteraction preserves the normal trigger focus return for keyboard-driven closure.
  // markMenuKeyboardInteraction 为键盘触发的关闭保留正常的触发按钮焦点回送。
  const markMenuKeyboardInteraction = React.useCallback(() => {
    menuPointerInteractionRef.current = false;
  }, []);
  // handleMenuCloseAutoFocus suppresses only the focus return caused by a pointer selection inside this menu.
  // handleMenuCloseAutoFocus 仅阻止该菜单内部鼠标选择导致的焦点回送。
  const handleMenuCloseAutoFocus = React.useCallback<
    NonNullable<React.ComponentProps<typeof DropdownMenuContent>["onCloseAutoFocus"]>
  >((event) => {
    if (menuPointerInteractionRef.current) {
      menuPointerInteractionRef.current = false;
      event.preventDefault();
    }
  }, []);

  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button
          aria-label={t("appChrome.menu")}
          className="window-control-button app-chrome-menu-button"
          size="icon"
          type="button"
          variant="ghost"
        >
          <ChevronDown size={16} />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent
        align="end"
        className="app-chrome-menu"
        onCloseAutoFocus={handleMenuCloseAutoFocus}
        onKeyDownCapture={markMenuKeyboardInteraction}
        onPointerDownCapture={markMenuPointerInteraction}
        side="bottom"
        sideOffset={0}
      >
        <DropdownMenuItem className="app-chrome-menu-status" disabled>
          <span>{t("appChrome.status")}</span>
          <StatusPill status={statusKind} label={statusLabel} />
        </DropdownMenuItem>
        <DropdownMenuSub>
          <DropdownMenuSubTrigger className="app-chrome-menu-row">
            {props.theme === "dark" ? <Moon size={15} /> : <Sun size={15} />}
            <span>{t("appChrome.theme")}</span>
            <small>{props.theme === "dark" ? t("appChrome.theme.dark") : t("appChrome.theme.light")}</small>
          </DropdownMenuSubTrigger>
          <DropdownMenuSubContent
            className="app-chrome-menu-sub"
            onKeyDownCapture={markMenuKeyboardInteraction}
            onPointerDownCapture={markMenuPointerInteraction}
          >
            <DropdownMenuItem onSelect={() => selectTheme("light")}>
              <Sun size={15} />
              <span>{t("appChrome.theme.light")}</span>
              {props.theme === "light" ? <Check className="app-chrome-menu-check" size={14} /> : null}
            </DropdownMenuItem>
            <DropdownMenuItem onSelect={() => selectTheme("dark")}>
              <Moon size={15} />
              <span>{t("appChrome.theme.dark")}</span>
              {props.theme === "dark" ? <Check className="app-chrome-menu-check" size={14} /> : null}
            </DropdownMenuItem>
          </DropdownMenuSubContent>
        </DropdownMenuSub>
        <DropdownMenuSub>
          <DropdownMenuSubTrigger className="app-chrome-menu-row">
            <span className="app-chrome-menu-language-mark">{activeLocaleEntry.mark}</span>
            <span>{t("global.language")}</span>
            <small>{activeLocaleEntry.nativeName}</small>
          </DropdownMenuSubTrigger>
          <DropdownMenuSubContent
            className="app-chrome-menu-sub"
            onKeyDownCapture={markMenuKeyboardInteraction}
            onPointerDownCapture={markMenuPointerInteraction}
          >
            {SUPPORTED_LOCALES.map((entry) => (
              <DropdownMenuItem key={entry.code} lang={entry.code} onSelect={() => props.onChangeLocale(entry.code)}>
                <span className="app-chrome-menu-language-mark">{entry.mark}</span>
                <span>{entry.nativeName}</span>
                {props.locale === entry.code ? <Check className="app-chrome-menu-check" size={14} /> : null}
              </DropdownMenuItem>
            ))}
          </DropdownMenuSubContent>
        </DropdownMenuSub>
        <DropdownMenuItem className="app-chrome-menu-row" onSelect={reloadWorkbench}>
          <RefreshCw size={15} />
          <span>{t("appChrome.reload")}</span>
        </DropdownMenuItem>
        {openDeveloperTools ? (
          <DropdownMenuItem
            className="app-chrome-menu-row"
            onSelect={() => {
              void openDeveloperTools().catch((error) => {
                toast.error(t("appChrome.developerToolsFailed"), {
                  description: error instanceof Error ? error.message : String(error),
                });
              });
            }}
          >
            <PanelRightOpen size={15} />
            <span>{t("appChrome.developerTools")}</span>
          </DropdownMenuItem>
        ) : null}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

// WindowControls renders Tauri-only custom minimize, maximize, and close buttons.
// WindowControls 渲染仅 Tauri 使用的自定义最小化、最大化与关闭按钮。
export function WindowControls(props: {
  // Optional control rendered before minimize, maximize, and close.
  // 渲染在最小化、最大化、关闭按钮之前的可选控件。
  leadingControl?: React.ReactNode;
  // Whether the current Tauri window is maximized.
  // 当前 Tauri 窗口是否处于最大化状态。
  isMaximized: boolean;
  // Toggles maximize or restore using the shared titlebar behavior.
  // 使用共享标题栏行为切换最大化或还原。
  onMaximizeToggle: () => Promise<void>;
  // Refreshes maximize state after native window actions.
  // 在原生窗口操作后刷新最大化状态。
  onMaximizeStateRefresh: () => Promise<void>;
}) {
  // t translates window control labels and error messages.
  // t 翻译窗口控制标签与错误消息。
  const { t } = useI18n();
  // runWindowAction invokes one native window command and reports failures as toasts.
  // runWindowAction 调用单个原生窗口命令，并用 toast 报告失败。
  const runWindowAction = React.useCallback(async (action: "minimize" | "close") => {
    try {
      if (action === "minimize") {
        await shellClient.minimizeWindow();
      } else {
        await shellClient.closeWindow();
      }
      await props.onMaximizeStateRefresh();
    } catch (error) {
      toast.error(`${t("appChrome.windowActionFailed")}${error instanceof Error ? error.message : String(error)}`);
    }
  }, [props, t]);

  // handleMaximizeClick shares the same maximize/restore behavior as titlebar double-click.
  // handleMaximizeClick 与标题栏双击共享同一套最大化/还原行为。
  const handleMaximizeClick = React.useCallback(async () => {
    try {
      await props.onMaximizeToggle();
    } catch (error) {
      toast.error(`${t("appChrome.windowActionFailed")}${error instanceof Error ? error.message : String(error)}`);
    }
  }, [props, t]);

  return (
    <div className="window-controls" data-no-drag>
      {props.leadingControl}
      <Button
        aria-label={t("appChrome.window.minimize")}
        className="window-control-button"
        onClick={() => void runWindowAction("minimize")}
        size="icon"
        type="button"
        variant="ghost"
      >
        <MinusIcon size={16} />
      </Button>
      <Button
        aria-label={props.isMaximized ? t("appChrome.window.restore") : t("appChrome.window.maximize")}
        className="window-control-button"
        onClick={() => void handleMaximizeClick()}
        size="icon"
        type="button"
        variant="ghost"
      >
        {props.isMaximized ? <Minimize2 size={15} /> : <SquareIcon size={13} />}
      </Button>
      <Button
        aria-label={t("appChrome.window.close")}
        className="window-control-button close"
        onClick={() => void runWindowAction("close")}
        size="icon"
        type="button"
        variant="ghost"
      >
        <X size={16} />
      </Button>
    </div>
  );
}

// RepositoryLink renders the statusbar GitHub repository link.
// RepositoryLink 渲染状态栏 GitHub 仓库链接。
export function RepositoryLink(props: {
  // Opens the repository URL through the active host shell.
  // 通过当前宿主壳打开仓库地址。
  onOpenExternalUrl: (url: string) => Promise<void>;
}) {
  // t translates repository link accessibility labels.
  // t 翻译仓库链接无障碍标签。
  const { t } = useI18n();
  // handleClick opens the repository externally in Tauri and in a new tab in browsers.
  // handleClick 在 Tauri 中外部打开仓库，在浏览器中打开新标签。
  const handleClick = React.useCallback((event: React.MouseEvent<HTMLAnchorElement>) => {
    event.preventDefault();
    void props.onOpenExternalUrl(GITHUB_REPOSITORY_URL).catch((error) => {
      toast.error(`${t("appChrome.githubOpenFailed")}${error instanceof Error ? error.message : String(error)}`);
    });
  }, [props, t]);

  return (
    <a
      aria-label={t("appChrome.githubAria")}
      className="repository-link"
      href={GITHUB_REPOSITORY_URL}
      onClick={handleClick}
      rel="noreferrer"
      target="_blank"
    >
      <GitHubMark />
      <span>OpenVulcan/vulcan-code</span>
    </a>
  );
}

// GitHubMark renders the filled GitHub logo mark used in the statusbar.
// GitHubMark 渲染状态栏使用的填充版 GitHub 标识。
function GitHubMark() {
  return (
    <svg
      aria-hidden="true"
      className="repository-github-mark"
      focusable="false"
      viewBox="0 0 16 16"
    >
      <path
        fill="currentColor"
        d="M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82A7.65 7.65 0 0 1 8 3.87c.68 0 1.36.09 2 .26 1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93-.01 2.2 0 .21.15.46.55.38A8.01 8.01 0 0 0 16 8c0-4.42-3.58-8-8-8Z"
      />
    </svg>
  );
}
