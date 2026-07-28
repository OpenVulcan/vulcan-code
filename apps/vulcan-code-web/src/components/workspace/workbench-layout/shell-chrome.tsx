import React from "react";
import {
  GlobalActions,
  WindowControls,
} from "@/components/shell/app-chrome";
import { shellClient } from "@/service/webui-service-runtime";
import type { LocaleCode } from "@/i18n";
import type { StartupSnapshot } from "@/types";
import {
  initialThemeMode,
  type ThemeMode,
} from "./types";

// WorkbenchShellChromeOptions carries the app-level chrome inputs owned by the workbench layout.
// WorkbenchShellChromeOptions 携带由工作台布局持有的应用级 chrome 输入。
interface WorkbenchShellChromeOptions {
  // startup is forwarded to the global action menu for startup diagnostics.
  // startup 会转发给全局操作菜单，用于启动诊断。
  startup: StartupSnapshot;
  // locale is the currently selected UI locale.
  // locale 是当前选中的界面语言。
  locale: LocaleCode;
  // onChangeLocale updates the currently selected UI locale.
  // onChangeLocale 更新当前选中的界面语言。
  onChangeLocale: (locale: LocaleCode) => void;
}

// WorkbenchShellChromeState exposes reusable titlebar and shell chrome render state.
// WorkbenchShellChromeState 暴露可复用的标题栏与外壳 chrome 渲染状态。
interface WorkbenchShellChromeState {
  // shellChromeActions contains the app-wide global actions and optional Tauri window controls.
  // shellChromeActions 包含应用级全局操作与可选 Tauri 窗口控制。
  shellChromeActions: React.ReactNode;
  // showWindowControls reports whether the current runtime supports custom window controls.
  // showWindowControls 表示当前运行时是否支持自定义窗口控制。
  showWindowControls: boolean;
  // windowDragRegionEnabled reports whether native titlebar dragging is safe for the current window state.
  // windowDragRegionEnabled 表示当前窗口状态下是否可以安全启用原生标题栏拖动。
  windowDragRegionEnabled: boolean;
  // handleTitlebarDoubleClick toggles maximize from non-interactive titlebar space.
  // handleTitlebarDoubleClick 从非交互标题栏区域切换最大化。
  handleTitlebarDoubleClick: (event: React.MouseEvent<HTMLElement>) => void;
}

// shouldEnableWindowDragRegion prevents Tauri from starting a drag while the window is maximized.
// shouldEnableWindowDragRegion 防止 Tauri 在窗口最大化时启动拖动流程。
export function shouldEnableWindowDragRegion(
  showWindowControls: boolean,
  isWindowMaximized: boolean,
): boolean {
  return showWindowControls && !isWindowMaximized;
}

// useWorkbenchShellChrome owns the theme and Tauri titlebar state for WorkbenchLayout.
// useWorkbenchShellChrome 为 WorkbenchLayout 持有主题与 Tauri 标题栏状态。
export function useWorkbenchShellChrome({
  startup,
  locale,
  onChangeLocale,
}: WorkbenchShellChromeOptions): WorkbenchShellChromeState {
  // theme stores the current shadcn visual theme.
  // theme 保存当前 shadcn 视觉主题。
  const [theme, setTheme] = React.useState<ThemeMode>(() => initialThemeMode());
  // showWindowControls enables the custom titlebar only inside the Tauri desktop shell.
  // showWindowControls 仅在 Tauri 桌面壳内启用自定义标题栏。
  const showWindowControls = shellClient.supportsWindowControls();
  // isWindowMaximized tracks the current Tauri window maximize state for the titlebar button.
  // isWindowMaximized 跟踪当前 Tauri 窗口最大化状态，用于标题栏按钮。
  const [isWindowMaximized, setIsWindowMaximized] = React.useState(false);
  // windowDragRegionEnabled disables native dragging before a maximized titlebar can corrupt its restore bounds.
  // windowDragRegionEnabled 在最大化标题栏可能破坏还原边界前禁用原生拖动。
  const windowDragRegionEnabled = shouldEnableWindowDragRegion(
    showWindowControls,
    isWindowMaximized,
  );
  // refreshWindowMaximizedState synchronizes the local titlebar state with Tauri.
  // refreshWindowMaximizedState 将本地标题栏状态与 Tauri 同步。
  const refreshWindowMaximizedState = React.useCallback(async () => {
    setIsWindowMaximized(await shellClient.isWindowMaximized());
  }, []);
  // toggleWindowMaximize toggles native maximize state and refreshes the icon.
  // toggleWindowMaximize 切换原生最大化状态并刷新图标。
  const toggleWindowMaximize = React.useCallback(async () => {
    await shellClient.toggleWindowMaximize();
    window.setTimeout(() => void refreshWindowMaximizedState(), 40);
  }, [refreshWindowMaximizedState]);
  // handleTitlebarDoubleClick toggles maximize only from non-interactive titlebar space.
  // handleTitlebarDoubleClick 仅在非交互标题栏区域双击时切换最大化。
  const handleTitlebarDoubleClick = React.useCallback((event: React.MouseEvent<HTMLElement>) => {
    if (!showWindowControls) {
      return;
    }
    const target = event.target instanceof Element ? event.target : null;
    if (target?.closest("[data-no-drag], button, a, input, textarea, select, [role='button'], [role='menuitem']")) {
      return;
    }
    void toggleWindowMaximize();
  }, [showWindowControls, toggleWindowMaximize]);

  React.useEffect(() => {
    document.documentElement.classList.toggle("dark", theme === "dark");
    document.documentElement.style.colorScheme = theme;
    window.localStorage.setItem("vulcan-webui-theme", theme);
  }, [theme]);

  React.useEffect(() => {
    if (!showWindowControls) {
      return;
    }
    void refreshWindowMaximizedState();
  }, [refreshWindowMaximizedState, showWindowControls]);

  React.useEffect(() => {
    if (!showWindowControls) {
      return;
    }

    let cancelled = false;
    let unlisten: null | (() => void) = null;

    // subscribeWindowResize keeps the restore icon aligned when native drag-region double click changes state.
    // subscribeWindowResize 在原生拖动区域双击改变窗口状态后同步还原图标。
    async function subscribeWindowResize() {
      if (cancelled) {
        return;
      }
      unlisten = await shellClient.subscribeWindowResize(() => {
        void refreshWindowMaximizedState();
      });
    }

    void subscribeWindowResize();
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [refreshWindowMaximizedState, showWindowControls]);

  // shellChromeActions moves app-wide status and window controls into the shared content header.
  // shellChromeActions 将应用级状态与窗口控制移动到共享内容标题栏中。
  const shellChromeActions = (
    <div className="app-chrome-actions" data-no-drag>
      {showWindowControls ? (
        <WindowControls
          leadingControl={(
            <GlobalActions
              startup={startup}
              theme={theme}
              locale={locale}
              onChangeLocale={onChangeLocale}
              onToggleTheme={() => setTheme((current) => current === "dark" ? "light" : "dark")}
            />
          )}
          isMaximized={isWindowMaximized}
          onMaximizeToggle={toggleWindowMaximize}
          onMaximizeStateRefresh={refreshWindowMaximizedState}
        />
      ) : (
        <GlobalActions
          startup={startup}
          theme={theme}
          locale={locale}
          onChangeLocale={onChangeLocale}
          onToggleTheme={() => setTheme((current) => current === "dark" ? "light" : "dark")}
        />
      )}
    </div>
  );

  return {
    shellChromeActions,
    showWindowControls,
    windowDragRegionEnabled,
    handleTitlebarDoubleClick,
  };
}
