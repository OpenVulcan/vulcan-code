import React from "react";
import {
  GlobalActions,
  WindowControls,
} from "@/components/shell/app-chrome";
import { NATIVE_WINDOW_BUTTONS } from "@/app/boot-startup-screen";
import { nativeWindowButtonsStrip } from "@/components/shell/navigation-layout";
import { VulcanWordmark } from "@/components/shell/vulcan-wordmark";
import { shellClient } from "@/service/webui-service-runtime";
import { useI18n, type LocaleCode } from "@/i18n";
import { toast } from "@/ui-kit/primitives/toast";
import { useAppearance } from "@/ui-kit/appearance";
import type { StartupSnapshot } from "@/types";

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
  // shellChromeActions contains the app menu and the optional desktop window controls.
  // shellChromeActions 包含应用菜单与可选的桌面窗口控制。
  shellChromeActions: React.ReactNode;
  // showWindowControls reports whether the current runtime supports custom window controls.
  // showWindowControls 表示当前运行时是否支持自定义窗口控制。
  showWindowControls: boolean;
  // nativeButtonsInset is the title strip width the operating system's window buttons cover from the window's
  // left edge, in pixels; null when no buttons sit in the strip (other platforms, full screen, or not yet placed).
  // nativeButtonsInset 是操作系统窗口按钮从窗口左边缘起在标题条中占用的宽度（像素）；标题条中没有按钮时为 null
  // （其他平台、全屏或尚未放置）。
  nativeButtonsInset: number | null;
  // windowDragRegionEnabled reports whether native titlebar dragging is safe for the current window state.
  // windowDragRegionEnabled 表示当前窗口状态下是否可以安全启用原生标题栏拖动。
  windowDragRegionEnabled: boolean;
  // handleTitlebarDoubleClick toggles maximize, or native full screen on macOS, from non-interactive titlebar space.
  // handleTitlebarDoubleClick 从非交互标题栏区域切换最大化；macOS 上切换原生全屏。
  handleTitlebarDoubleClick: (event: React.MouseEvent<HTMLElement>) => void;
}

// shouldEnableWindowDragRegion prevents a native drag while the window is maximized or full screen, where the
// native region would also swallow the double click the title strip handles itself.
// shouldEnableWindowDragRegion 防止窗口在最大化或全屏时启动原生拖动；此时原生区域还会吞掉标题条自行处理的双击。
export function shouldEnableWindowDragRegion(
  showWindowControls: boolean,
  isWindowMaximized: boolean,
  isWindowFullScreen: boolean,
): boolean {
  return showWindowControls && !isWindowMaximized && !isWindowFullScreen;
}

// useWorkbenchShellChrome owns the desktop titlebar state for WorkbenchLayout.
// useWorkbenchShellChrome 为 WorkbenchLayout 持有桌面标题栏状态。
export function useWorkbenchShellChrome({
  startup,
  locale,
  onChangeLocale,
}: WorkbenchShellChromeOptions): WorkbenchShellChromeState {
  // t translates shell failures without exposing backend details as the main message.
  // t 翻译壳层失败提示，避免将后端详情作为主要文案。
  const { t } = useI18n();
  // showWindowControls enables the custom titlebar only inside the Tauri desktop shell.
  // showWindowControls 仅在 Tauri 桌面壳内启用自定义标题栏。
  const showWindowControls = shellClient.supportsWindowControls();
  // nativeWindowButtons keeps the system buttons, with their tiling and full-screen menu, in place of the custom ones.
  // nativeWindowButtons 保留系统按钮及其平铺与全屏菜单，代替自绘按钮。
  const nativeWindowButtons = showWindowControls && NATIVE_WINDOW_BUTTONS;
  // isWindowMaximized tracks the current Tauri window maximize state for the titlebar button.
  // isWindowMaximized 跟踪当前 Tauri 窗口最大化状态，用于标题栏按钮。
  const [isWindowMaximized, setIsWindowMaximized] = React.useState(false);
  // isWindowFullScreen tracks native full screen, where macOS moves its window buttons out of the title strip.
  // isWindowFullScreen 跟踪原生全屏；此时 macOS 将窗口按钮移出标题条。
  const [isWindowFullScreen, setIsWindowFullScreen] = React.useState(false);
  // nativeButtonsInset is the right edge of the placed macOS window buttons, or null before they are placed.
  // nativeButtonsInset 是已放置 macOS 窗口按钮的右边缘；放置之前为 null。
  const [nativeButtonsInset, setNativeButtonsInset] = React.useState<number | null>(null);
  // appearance changes the text tier, which resizes the title strip the buttons are centred in.
  // appearance 改变文字档位，从而改变按钮居中所在标题条的尺寸。
  const appearance = useAppearance();
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
    isWindowFullScreen,
  );
  // refreshWindowMaximizedState synchronizes the local titlebar state with Tauri.
  // refreshWindowMaximizedState 将本地标题栏状态与 Tauri 同步。
  const refreshWindowMaximizedState = React.useCallback(async () => {
    // owner and request ensure an older native reply cannot overwrite a newer resize state.
    // owner 和 request 确保旧原生回包不能覆盖较新的窗口尺寸状态。
    const owner = lifecycle.current;
    const request = ++owner.request;
    const [maximized, fullScreen] = await Promise.all([
      shellClient.isWindowMaximized(),
      shellClient.isWindowFullScreen(),
    ]);
    if (owner.active && owner.request === request) {
      setIsWindowMaximized(maximized);
      setIsWindowFullScreen(fullScreen);
    }
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
  // toggleTitlebarAction is the double-click action: native full screen where the system draws the window buttons
  // (the green button's action on macOS), maximize elsewhere.
  // toggleTitlebarAction 是双击操作：系统自绘窗口按钮时为原生全屏（macOS 绿色按钮的操作），其他情况为最大化。
  const toggleTitlebarAction = React.useCallback(async () => {
    if (!nativeWindowButtons) {
      await toggleWindowMaximize();
      return;
    }
    if (maximizing.current) return;
    maximizing.current = true;
    try {
      await shellClient.toggleWindowFullScreen();
    } finally {
      maximizing.current = false;
    }
  }, [nativeWindowButtons, toggleWindowMaximize]);
  // handleTitlebarDoubleClick toggles the titlebar action only from non-interactive titlebar space.
  // handleTitlebarDoubleClick 仅在非交互标题栏区域双击时执行标题栏操作。
  const handleTitlebarDoubleClick = React.useCallback((event: React.MouseEvent<HTMLElement>) => {
    if (!showWindowControls) {
      return;
    }
    const target = event.target instanceof Element ? event.target : null;
    if (target?.closest("[data-no-drag], button, a, input, textarea, select, [role='button'], [role='menuitem']")) {
      return;
    }
    void toggleTitlebarAction().catch(reportWindowError);
  }, [showWindowControls, toggleTitlebarAction, reportWindowError]);

  React.useEffect(() => {
    if (!showWindowControls) {
      return;
    }
    void refreshWindowMaximizedState().catch(reportWindowError);
  }, [refreshWindowMaximizedState, showWindowControls, reportWindowError]);

  // The macOS window buttons follow the title strip whenever the text tier resizes it, and are placed again on
  // leaving full screen, where the system had moved them.
  // macOS 窗口按钮在文字档位改变标题条尺寸时随之移动；离开全屏时重新放置，因为系统曾移动过它们。
  React.useEffect(() => {
    if (!nativeWindowButtons || isWindowFullScreen) {
      return;
    }
    let cancelled = false;
    void shellClient.placeNativeWindowButtons(nativeWindowButtonsStrip())
      .then((inset) => { if (!cancelled) setNativeButtonsInset(inset); })
      .catch((error: unknown) => { if (!cancelled) reportWindowError(error); });
    return () => { cancelled = true; };
  }, [nativeWindowButtons, isWindowFullScreen, appearance.text, reportWindowError]);

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

  // buttonsInStrip reports the macOS window buttons occupying the title strip's left corner.
  // buttonsInStrip 表示 macOS 窗口按钮占据标题条左角。
  const buttonsInStrip = nativeWindowButtons && !isWindowFullScreen && nativeButtonsInset !== null;
  // shellChromeActions holds the app menu and, in a desktop window, the window controls at the strip edge. While the
  // macOS buttons occupy the panel's corner, the wordmark moves to the strip's right end as window drag space.
  // shellChromeActions 包含应用菜单，桌面窗口中还包含位于标题条边缘的窗口控制。macOS 按钮占据面板角落时，
  // 字标移到标题条右端，作为窗口拖动空间。
  const shellChromeActions = (
    <>
      <div className="vk-stage-chrome" data-no-drag>
        <GlobalActions startup={startup} locale={locale} onChangeLocale={onChangeLocale} />
        {showWindowControls && !nativeWindowButtons ? (
          <WindowControls
            isMaximized={isWindowMaximized}
            onMaximizeToggle={toggleWindowMaximize}
            onMaximizeStateRefresh={refreshWindowMaximizedState}
          />
        ) : null}
      </div>
      {buttonsInStrip ? (
        <span
          className="vk-stage-wordmark"
          data-tauri-drag-region={windowDragRegionEnabled ? "" : undefined}
          onDoubleClick={windowDragRegionEnabled ? undefined : handleTitlebarDoubleClick}
        >
          <VulcanWordmark className="vk-wordmark" label="Vulcan Code" />
        </span>
      ) : null}
    </>
  );

  return {
    shellChromeActions,
    showWindowControls,
    // Full screen moves the macOS buttons into the slide-down system title bar, so nothing is reserved then.
    // 全屏时 macOS 按钮移入下滑的系统标题栏，因此此时不预留任何空间。
    nativeButtonsInset: buttonsInStrip ? nativeButtonsInset : null,
    windowDragRegionEnabled,
    handleTitlebarDoubleClick,
  };
}

