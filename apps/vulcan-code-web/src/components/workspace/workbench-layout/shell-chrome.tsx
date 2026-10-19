import React from "react";
import {
  GlobalActions,
  WindowControls,
} from "@/components/shell/app-chrome";
import { shellClient } from "@/service/webui-service-runtime";
import { useI18n, type LocaleCode } from "@/i18n";
import { toast } from "sonner";
import type { StartupSnapshot } from "@/types";
import { resolvedDark, setAppearance, useAppearance } from "@/ui-kit/appearance";
import type { ThemeMode } from "./types";

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
  // t translates shell failures without exposing backend details as the main message.
  // t 翻译壳层失败提示，避免将后端详情作为主要文案。
  const { t } = useI18n();
  // theme is the resolved light or dark rendering of the shared appearance choice.
  // theme 是共享外观选择解析后的明暗呈现。
  const appearance = useAppearance();
  const theme: ThemeMode = resolvedDark(appearance.theme) ? "dark" : "light";
  // showWindowControls enables the custom titlebar only inside the Tauri desktop shell.
  // showWindowControls 仅在 Tauri 桌面壳内启用自定义标题栏。
  const showWindowControls = shellClient.supportsWindowControls();
  // isWindowMaximized tracks the current Tauri window maximize state for the titlebar button.
  // isWindowMaximized 跟踪当前 Tauri 窗口最大化状态，用于标题栏按钮。
  const [isWindowMaximized, setIsWindowMaximized] = React.useState(false);
  // lifecycle tracks request ownership across mount cleanup, including Strict Mode remounts.
  // lifecycle 跟踪挂载清理前后的请求归属，包括严格模式重新挂载。
  const lifecycle = React.useRef({ active: true, request: 0 });
  // maximizing blocks overlapping native toggles from rapid button and titlebar activation.
  // maximizing 阻止快速点击按钮和标题栏导致原生切换命令重叠。
  const maximizing = React.useRef(false);
  // reportWindowError gives ignored native callbacks the same visible failure path as buttons.
  // reportWindowError 为原生回调提供与按钮相同的可见失败提示。
  const reportWindowError = React.useCallback((error: unknown) => {
    toast.error(t("appChrome.windowActionFailed"), {
      description: error instanceof Error ? error.message : String(error),
      id: "window-action-failed",
    });
  }, [t]);
  React.useEffect(() => {
    lifecycle.current = { active: true, request: 0 };
    // mountedLifecycle is invalidated even if a later mount already owns the ref.
    // mountedLifecycle 即使在后续挂载接管引用后，也会被原清理逻辑失效。
    const mountedLifecycle = lifecycle.current;
    return () => { mountedLifecycle.active = false; };
  }, []);
  // windowDragRegionEnabled disables native dragging before a maximized titlebar can corrupt its restore bounds.
  // windowDragRegionEnabled 在最大化标题栏可能破坏还原边界前禁用原生拖动。
  const windowDragRegionEnabled = shouldEnableWindowDragRegion(
    showWindowControls,
    isWindowMaximized,
  );
  // refreshWindowMaximizedState synchronizes the local titlebar state with Tauri.
  // refreshWindowMaximizedState 将本地标题栏状态与 Tauri 同步。
  const refreshWindowMaximizedState = React.useCallback(async () => {
    // owner and request ensure an older native reply cannot overwrite a newer resize state.
    // owner 和 request 确保旧原生回包不能覆盖较新的窗口尺寸状态。
    const owner = lifecycle.current;
    const request = ++owner.request;
    const maximized = await shellClient.isWindowMaximized();
    if (owner.active && owner.request === request) setIsWindowMaximized(maximized);
  }, []);
  // toggleWindowMaximize toggles native maximize state and refreshes the icon.
  // toggleWindowMaximize 切换原生最大化状态并刷新图标。
  const toggleWindowMaximize = React.useCallback(async () => {
    if (maximizing.current) return;
    maximizing.current = true;
    try {
      await shellClient.toggleWindowMaximize();
      await refreshWindowMaximizedState();
    } finally {
      maximizing.current = false;
    }
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
    void toggleWindowMaximize().catch(reportWindowError);
  }, [showWindowControls, toggleWindowMaximize, reportWindowError]);

  // toggleTheme persists only the user's explicit selection; denied storage keeps the current session usable.
  // toggleTheme 仅持久化用户明确选择；存储被拒绝时仍允许当前会话切换主题。
  const toggleTheme = React.useCallback(() => {
    // nextTheme is applied locally even when persistence is unavailable.
    // nextTheme 即使无法持久化也会在当前客户端生效。
    const nextTheme = theme === "dark" ? "light" : "dark";
    if (!setAppearance("theme", nextTheme)) {
      toast.error(t("appChrome.themeSaveFailed"), { id: "theme-save-failed" });
    }
  }, [theme, t]);

  React.useEffect(() => {
    if (!showWindowControls) {
      return;
    }
    void refreshWindowMaximizedState().catch(reportWindowError);
  }, [refreshWindowMaximizedState, showWindowControls, reportWindowError]);

  React.useEffect(() => {
    if (!showWindowControls) {
      return;
    }

    // cancelled and unlisten retain exact ownership of the asynchronous native subscription.
    // cancelled 和 unlisten 保留异步原生订阅的精确归属。
    let cancelled = false;
    let unlisten: null | (() => void) = null;

    // subscribeWindowResize keeps the restore icon aligned when native drag-region double click changes state.
    // subscribeWindowResize 在原生拖动区域双击改变窗口状态后同步还原图标。
    async function subscribeWindowResize() {
      if (cancelled) {
        return;
      }
      // release must run immediately if native setup finishes after component cleanup.
      // 若原生初始化在组件清理后完成，release 必须立即执行。
      const release = await shellClient.subscribeWindowResize(() => {
        if (!cancelled) void refreshWindowMaximizedState().catch(reportWindowError);
      });
      if (cancelled) release();
      else unlisten = release;
    }

    void subscribeWindowResize().catch((error) => { if (!cancelled) reportWindowError(error); });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [refreshWindowMaximizedState, showWindowControls, reportWindowError]);

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
              onToggleTheme={toggleTheme}
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
          onToggleTheme={toggleTheme}
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
