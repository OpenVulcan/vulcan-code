import React from "react";
import {
  DropdownMenu,
  DropdownMenuCheckboxItem,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuSeparator,
  DropdownMenuSub,
  DropdownMenuSubContent,
  DropdownMenuSubTrigger,
  DropdownMenuTrigger,
} from "@/ui-kit/primitives/dropdown-menu";
import { useI18n, type LocaleCode, type TranslationKey } from "@/i18n";
import { SUPPORTED_LOCALES } from "@/i18n-locales";
import { createShellClient } from "@/host-shell";
import type { StartupSnapshot } from "@/types";
import { toast } from "@/ui-kit/primitives/toast";
import {
  APPEARANCE_TEXT_TIERS,
  APPEARANCE_THEMES,
  APPEARANCE_WIDTHS,
  setAppearance,
  useAppearance,
  type AppearancePreferences,
  type AppearanceTheme,
} from "@/ui-kit/appearance";
import {
  Activity,
  ChevronDown,
  FoldHorizontal,
  Copy,
  Minus,
  Moon,
  MoveHorizontal,
  PanelRightOpen,
  RefreshCw,
  Rocket,
  Square,
  Sun,
  SunMoon,
  Type,
  UnfoldHorizontal,
  X,
} from "lucide-react";

// GITHUB_REPOSITORY_URL points to the public project repository.
// GITHUB_REPOSITORY_URL 指向公开项目仓库。
const GITHUB_REPOSITORY_URL = "https://github.com/OpenVulcan/vulcan-code";

// shellClient is the shell chrome facade used by local window controls.
// shellClient 是本地窗口控制使用的壳层 chrome 门面。
const shellClient = createShellClient();

// APPEARANCE_THEME_ICONS gives every theme the glyph shown in its menu row, trigger and settings choice.
// APPEARANCE_THEME_ICONS 为每个主题提供菜单行、触发器与设置选项中显示的图形。
export const APPEARANCE_THEME_ICONS: Record<AppearanceTheme, typeof Sun> = { auto: SunMoon, light: Sun, dark: Moon, command: Rocket };

// useChooseAppearance applies one explicit appearance choice and reports a refused save once.
// useChooseAppearance 应用一项明确的外观选择，存储被拒绝时提示一次。
export function useChooseAppearance() {
  const { t } = useI18n();
  return React.useCallback(<K extends keyof AppearancePreferences>(key: K, value: AppearancePreferences[K]) => {
    if (!setAppearance(key, value)) {
      toast.error(t("appChrome.appearanceSaveFailed"), { id: "appearance-save-failed" });
    }
  }, [t]);
}

// GlobalActions renders the app menu: service status, appearance, language and client actions.
// GlobalActions 渲染应用菜单：服务状态、外观、语言与客户端操作。
export function GlobalActions(props: {
  // Startup snapshot used for the compact online indicator.
  // 紧凑在线状态指示使用的启动快照。
  startup: StartupSnapshot;
  // Current UI language.
  // 当前 UI 语言。
  locale: LocaleCode;
  // Updates the current UI language.
  // 更新当前 UI 语言。
  onChangeLocale: (locale: LocaleCode) => void;
}) {
  // t translates global action labels.
  // t 翻译全局操作标签。
  const { t } = useI18n();
  // appearance is the live local appearance choice; choose applies one change.
  // appearance 是实时的本地外观选择；choose 应用一项变更。
  const appearance = useAppearance();
  const choose = useChooseAppearance();
  // activeLocaleEntry labels the current language in the menu trigger.
  // activeLocaleEntry 在菜单触发器中标注当前语言。
  const activeLocaleEntry = SUPPORTED_LOCALES.find((entry) => entry.code === props.locale) ?? SUPPORTED_LOCALES[0];
  // openDeveloperTools is present only in a desktop host that exposes the native action.
  // openDeveloperTools 仅在桌面宿主提供原生操作时存在。
  const openDeveloperTools = shellClient.openDeveloperTools;
  // isPreviewMode detects fallback browser data instead of a live service bridge.
  // isPreviewMode 检测当前是否为浏览器回退预览数据而非真实服务桥接。
  const isPreviewMode = props.startup.phase === "browser-preview";
  // statusLabel translates internal startup phases into compact user-facing labels.
  // statusLabel 将内部启动阶段转换为面向用户的紧凑显示文案。
  const statusLabel = isPreviewMode ? t("global.preview") : props.startup.ready ? t("global.online") : props.startup.phase;
  // statusTone keeps preview visually distinct from a live ready service.
  // statusTone 让预览状态与真实在线服务在视觉上保持区分。
  const statusTone = isPreviewMode ? "warn" : props.startup.ready ? "ok" : "ember";
  const ThemeIcon = APPEARANCE_THEME_ICONS[appearance.theme];
  // menuPointerInteractionRef records whether the current menu closure originated from a pointer action inside the menu.
  // menuPointerInteractionRef 记录当前菜单关闭是否源于菜单内部的鼠标操作。
  const menuPointerInteractionRef = React.useRef(false);
  const markMenuPointerInteraction = React.useCallback(() => { menuPointerInteractionRef.current = true; }, []);
  const markMenuKeyboardInteraction = React.useCallback(() => { menuPointerInteractionRef.current = false; }, []);
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
  // subProps keeps pointer and keyboard tracking on every submenu surface.
  // subProps 在每个子菜单表面上保持鼠标与键盘追踪。
  const subProps = { onKeyDownCapture: markMenuKeyboardInteraction, onPointerDownCapture: markMenuPointerInteraction };

  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <button aria-label={t("appChrome.menu")} className="vk-icon-btn" type="button">
          <ChevronDown />
        </button>
      </DropdownMenuTrigger>
      <DropdownMenuContent
        align="end"
        className="vk-app-menu"
        onCloseAutoFocus={handleMenuCloseAutoFocus}
        side="bottom"
        sideOffset={4}
        {...subProps}
      >
        <DropdownMenuLabel className="vk-app-menu-status">
          <span>{t("appChrome.status")}</span>
          <span className="vk-badge" data-tone={statusTone}>{statusLabel}</span>
        </DropdownMenuLabel>
        <DropdownMenuSeparator />
        <DropdownMenuSub>
          <DropdownMenuSubTrigger>
            <ThemeIcon />
            <span>{t("appChrome.theme")}</span>
            <span className="vk-menu-meta">{t(`appChrome.theme.${appearance.theme}` as TranslationKey)}</span>
          </DropdownMenuSubTrigger>
          <DropdownMenuSubContent {...subProps}>
            <DropdownMenuRadioGroup value={appearance.theme} onValueChange={(value) => choose("theme", value as AppearanceTheme)}>
              {APPEARANCE_THEMES.map((theme) => {
                const Icon = APPEARANCE_THEME_ICONS[theme];
                return (
                  <DropdownMenuRadioItem key={theme} value={theme}>
                    <Icon />
                    <span>{t(`appChrome.theme.${theme}` as TranslationKey)}</span>
                  </DropdownMenuRadioItem>
                );
              })}
            </DropdownMenuRadioGroup>
          </DropdownMenuSubContent>
        </DropdownMenuSub>
        <DropdownMenuSub>
          <DropdownMenuSubTrigger>
            <Type />
            <span>{t("appChrome.text")}</span>
            <span className="vk-menu-meta">{t(`appChrome.text.${appearance.text}` as TranslationKey)}</span>
          </DropdownMenuSubTrigger>
          <DropdownMenuSubContent {...subProps}>
            <DropdownMenuRadioGroup value={appearance.text} onValueChange={(value) => choose("text", value as AppearancePreferences["text"])}>
              {APPEARANCE_TEXT_TIERS.map((tier) => (
                <DropdownMenuRadioItem key={tier} value={tier}>
                  <span>{t(`appChrome.text.${tier}` as TranslationKey)}</span>
                </DropdownMenuRadioItem>
              ))}
            </DropdownMenuRadioGroup>
          </DropdownMenuSubContent>
        </DropdownMenuSub>
        <DropdownMenuSub>
          <DropdownMenuSubTrigger>
            <MoveHorizontal />
            <span>{t("appChrome.width")}</span>
            <span className="vk-menu-meta">{t(`appChrome.width.${appearance.width}` as TranslationKey)}</span>
          </DropdownMenuSubTrigger>
          <DropdownMenuSubContent {...subProps}>
            <ReadingWidthItems />
          </DropdownMenuSubContent>
        </DropdownMenuSub>
        <DropdownMenuCheckboxItem
          checked={appearance.motion === "reduce"}
          onCheckedChange={(checked) => choose("motion", checked ? "reduce" : "system")}
        >
          <Activity />
          <span>{t("appChrome.motion.reduce")}</span>
        </DropdownMenuCheckboxItem>
        <DropdownMenuSeparator />
        <DropdownMenuSub>
          <DropdownMenuSubTrigger>
            <span className="vk-lang-mark">{activeLocaleEntry.mark}</span>
            <span>{t("global.language")}</span>
            <span className="vk-menu-meta">{activeLocaleEntry.nativeName}</span>
          </DropdownMenuSubTrigger>
          <DropdownMenuSubContent {...subProps}>
            <DropdownMenuRadioGroup value={props.locale} onValueChange={(value) => props.onChangeLocale(value as LocaleCode)}>
              {SUPPORTED_LOCALES.map((entry) => (
                <DropdownMenuRadioItem key={entry.code} lang={entry.code} value={entry.code}>
                  <span className="vk-lang-mark">{entry.mark}</span>
                  <span>{entry.nativeName}</span>
                </DropdownMenuRadioItem>
              ))}
            </DropdownMenuRadioGroup>
          </DropdownMenuSubContent>
        </DropdownMenuSub>
        <DropdownMenuItem onSelect={() => window.location.reload()}>
          <RefreshCw />
          <span>{t("appChrome.reload")}</span>
        </DropdownMenuItem>
        {openDeveloperTools ? (
          <DropdownMenuItem
            onSelect={() => {
              void openDeveloperTools().catch((error) => {
                toast.error(t("appChrome.developerToolsFailed"), {
                  description: error instanceof Error ? error.message : String(error),
                });
              });
            }}
          >
            <PanelRightOpen />
            <span>{t("appChrome.developerTools")}</span>
          </DropdownMenuItem>
        ) : null}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

// ReadingWidthItems lists the conversation reading-width presets as one exclusive choice.
// The app menu and the conversation title strip both offer it.
// ReadingWidthItems 以单选方式列出会话阅读宽度预设；应用菜单与会话标题条共用。
export function ReadingWidthItems() {
  const { t } = useI18n();
  const appearance = useAppearance();
  const choose = useChooseAppearance();
  return (
    <DropdownMenuRadioGroup value={appearance.width} onValueChange={(value) => choose("width", value as AppearancePreferences["width"])}>
      {APPEARANCE_WIDTHS.map((width) => (
        <DropdownMenuRadioItem key={width} value={width}>
          <span>{t(`appChrome.width.${width}` as TranslationKey)}</span>
        </DropdownMenuRadioItem>
      ))}
    </DropdownMenuRadioGroup>
  );
}

// WindowControls renders the desktop window's minimize, maximize and close buttons at the strip's right edge.
// WindowControls 在标题条右端渲染桌面窗口的最小化、最大化与关闭按钮。
export function WindowControls(props: {
  // Whether the current desktop window is maximized.
  // 当前桌面窗口是否处于最大化状态。
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
  // reportFailure shows one native window failure.
  // reportFailure 显示一次原生窗口操作失败。
  const reportFailure = React.useCallback((error: unknown) => {
    toast.error(`${t("appChrome.windowActionFailed")}${error instanceof Error ? error.message : String(error)}`);
  }, [t]);
  // runWindowAction invokes one native window command and reports failures as toasts.
  // runWindowAction 调用单个原生窗口命令，并用 toast 报告失败。
  const runWindowAction = React.useCallback(async (action: "minimize" | "close") => {
    try {
      if (action === "minimize") await shellClient.minimizeWindow();
      else await shellClient.closeWindow();
      await props.onMaximizeStateRefresh();
    } catch (error) {
      reportFailure(error);
    }
  }, [props, reportFailure]);
  const maximizeLabel = props.isMaximized ? t("appChrome.window.restore") : t("appChrome.window.maximize");

  return (
    <div className="vk-winctl" data-no-drag>
      <button aria-label={t("appChrome.window.minimize")} className="vk-win-btn" onClick={() => void runWindowAction("minimize")} title={t("appChrome.window.minimize")} type="button">
        <Minus />
      </button>
      <button aria-label={maximizeLabel} className="vk-win-btn" onClick={() => void props.onMaximizeToggle().catch(reportFailure)} title={maximizeLabel} type="button">
        {props.isMaximized ? <Copy data-restore="" /> : <Square />}
      </button>
      <button aria-label={t("appChrome.window.close")} className="vk-win-btn" data-action="close" onClick={() => void runWindowAction("close")} title={t("appChrome.window.close")} type="button">
        <X />
      </button>
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
  // handleClick opens the repository externally in desktop hosts and in a new tab in browsers.
  // handleClick 在桌面宿主中外部打开仓库，在浏览器中打开新标签。
  const handleClick = React.useCallback((event: React.MouseEvent<HTMLAnchorElement>) => {
    event.preventDefault();
    void props.onOpenExternalUrl(GITHUB_REPOSITORY_URL).catch((error) => {
      toast.error(`${t("appChrome.githubOpenFailed")}${error instanceof Error ? error.message : String(error)}`);
    });
  }, [props, t]);

  return (
    <a
      aria-label={t("appChrome.githubAria")}
      className="vk-status-item"
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
    <svg aria-hidden="true" className="vk-brand-glyph" focusable="false" viewBox="0 0 16 16">
      <path
        fill="currentColor"
        d="M8 0C3.58 0 0 3.58 0 8c0 3.54 2.29 6.53 5.47 7.59.4.07.55-.17.55-.38 0-.19-.01-.82-.01-1.49-2.01.37-2.53-.49-2.69-.94-.09-.23-.48-.94-.82-1.13-.28-.15-.68-.52-.01-.53.63-.01 1.08.58 1.23.82.72 1.21 1.87.87 2.33.66.07-.52.28-.87.51-1.07-1.78-.2-3.64-.89-3.64-3.95 0-.87.31-1.59.82-2.15-.08-.2-.36-1.02.08-2.12 0 0 .67-.21 2.2.82A7.65 7.65 0 0 1 8 3.87c.68 0 1.36.09 2 .26 1.53-1.04 2.2-.82 2.2-.82.44 1.1.16 1.92.08 2.12.51.56.82 1.27.82 2.15 0 3.07-1.87 3.75-3.65 3.95.29.25.54.73.54 1.48 0 1.07-.01 1.93-.01 2.2 0 .21.15.46.55.38A8.01 8.01 0 0 0 16 8c0-4.42-3.58-8-8-8Z"
      />
    </svg>
  );
}

// ReadingWidthMenu is the conversation title strip's reading-width switch.
// ReadingWidthMenu 是会话标题条上的阅读宽度切换。
export function ReadingWidthMenu() {
  const { t } = useI18n();
  const appearance = useAppearance();
  const label = `${t("appChrome.width")} · ${t(`appChrome.width.${appearance.width}` as TranslationKey)}`;
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <button aria-label={label} className="vk-icon-btn" title={label} type="button">
          {appearance.width === "full" ? <FoldHorizontal /> : <UnfoldHorizontal />}
        </button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" side="bottom" sideOffset={4}>
        <ReadingWidthItems />
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
