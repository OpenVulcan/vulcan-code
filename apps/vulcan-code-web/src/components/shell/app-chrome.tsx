import React from "react";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSub,
  DropdownMenuSubContent,
  DropdownMenuSubTrigger,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { StatusPill } from "@/components/shared/status";
import { areaIconMap } from "@/components/features/feature-shell";
import { useI18n, type LocaleCode } from "@/i18n";
import { cn } from "@/lib/utils";
import { createShellClient } from "@/shell";
import type {
  FeatureAreaSummary,
  MainAreaId,
  StartupSnapshot,
} from "@/types";
import { toast } from "sonner";
import {
  Check,
  ChevronDown,
  ChevronLeft,
  ChevronRight,
  Minimize2,
  MinusIcon,
  Moon,
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

// BrandHeader renders the product brand area.
// BrandHeader 渲染产品品牌区域。
export function BrandHeader(props: {
  // Whether the brand block should behave as a native Tauri drag region.
  // 品牌块是否应作为 Tauri 原生拖动区域。
  dragRegion?: boolean;
}) {
  return (
    <div className="brand" data-tauri-drag-region={props.dragRegion ? "" : undefined}>
      <img alt="" aria-hidden="true" className="brand-logo app-brand-logo" src="/vulcan-logo.svg" />
      <div>
        <strong>Vulcan Code</strong>
      </div>
    </div>
  );
}

// FeatureNav renders the primary workbench navigation.
// FeatureNav 渲染工作台主导航。
export function FeatureNav(props: {
  // Feature summaries rendered as the primary navigation buttons.
  // 渲染为主导航按钮的功能区摘要。
  areas: FeatureAreaSummary[];
  // Currently selected top-level feature area.
  // 当前选中的顶层功能区。
  activeArea: MainAreaId;
  // Selects one top-level feature area.
  // 选择一个顶层功能区。
  onSelect: (area: MainAreaId) => void;
}) {
  // t translates navigation accessibility labels.
  // t 翻译导航无障碍标签。
  const { t } = useI18n();
  // scrollRef points to the horizontal navigation viewport.
  // scrollRef 指向横向导航视口。
  const scrollRef = React.useRef<HTMLElement | null>(null);
  // navOverflow stores whether each arrow can move the navigation viewport.
  // navOverflow 保存左右箭头是否还能移动导航视口。
  const [navOverflow, setNavOverflow] = React.useState({ left: false, right: false });

  // updateNavOverflow refreshes arrow enabled state from the scroll viewport.
  // updateNavOverflow 根据滚动视口刷新箭头启用状态。
  const updateNavOverflow = React.useCallback(() => {
    const element = scrollRef.current;
    if (!element) {
      setNavOverflow({ left: false, right: false });
      return;
    }
    const maxScrollLeft = element.scrollWidth - element.clientWidth;
    setNavOverflow({
      left: element.scrollLeft > 2,
      right: element.scrollLeft < maxScrollLeft - 2,
    });
  }, []);

  // scrollNav moves the navigation viewport by one compact page.
  // scrollNav 按一个紧凑页宽移动导航视口。
  const scrollNav = React.useCallback((direction: -1 | 1) => {
    const element = scrollRef.current;
    if (!element) {
      return;
    }
    element.scrollBy({ left: direction * Math.max(120, element.clientWidth * 0.7), behavior: "smooth" });
    window.setTimeout(updateNavOverflow, 180);
  }, [updateNavOverflow]);

  React.useEffect(() => {
    updateNavOverflow();
    window.addEventListener("resize", updateNavOverflow);
    return () => window.removeEventListener("resize", updateNavOverflow);
  }, [props.areas.length, updateNavOverflow]);

  return (
    <div className="feature-nav-shell">
      {navOverflow.left ? (
        <Button
          aria-label={t("appChrome.scrollFeaturesLeft")}
          className="feature-nav-arrow"
          onClick={() => scrollNav(-1)}
          size="icon-sm"
          type="button"
          variant="ghost"
        >
          <ChevronLeft size={15} />
        </Button>
      ) : null}
      <nav className="feature-nav" aria-label={t("sidebar.overview")} onScroll={updateNavOverflow} ref={scrollRef}>
        {props.areas.map((area) => (
          <Button
            className="feature-button"
            key={area.id}
            onClick={() => props.onSelect(area.id)}
            size="sm"
            title={area.description}
            type="button"
            variant={props.activeArea === area.id ? "secondary" : "ghost"}
          >
            {areaIconMap[area.id]}
            <span>{area.label}</span>
          </Button>
        ))}
      </nav>
      {navOverflow.right ? (
        <Button
          aria-label={t("appChrome.scrollFeaturesRight")}
          className="feature-nav-arrow"
          onClick={() => scrollNav(1)}
          size="icon-sm"
          type="button"
          variant="ghost"
        >
          <ChevronRight size={15} />
        </Button>
      ) : null}
    </div>
  );
}

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
      <DropdownMenuContent align="end" className="app-chrome-menu" side="bottom" sideOffset={0}>
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
          <DropdownMenuSubContent className="app-chrome-menu-sub">
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
            <span className="app-chrome-menu-language-mark">{props.locale === "zh-CN" ? "ZH" : "EN"}</span>
            <span>{t("global.language")}</span>
            <small>{props.locale === "zh-CN" ? t("appChrome.language.zh") : "English"}</small>
          </DropdownMenuSubTrigger>
          <DropdownMenuSubContent className="app-chrome-menu-sub">
            <DropdownMenuItem onSelect={() => props.onChangeLocale("zh-CN")}>
              <span className="app-chrome-menu-language-mark">ZH</span>
              <span>{t("appChrome.language.zh")}</span>
              {props.locale === "zh-CN" ? <Check className="app-chrome-menu-check" size={14} /> : null}
            </DropdownMenuItem>
            <DropdownMenuItem onSelect={() => props.onChangeLocale("en-US")}>
              <span className="app-chrome-menu-language-mark">EN</span>
              <span>English</span>
              {props.locale === "en-US" ? <Check className="app-chrome-menu-check" size={14} /> : null}
            </DropdownMenuItem>
          </DropdownMenuSubContent>
        </DropdownMenuSub>
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
