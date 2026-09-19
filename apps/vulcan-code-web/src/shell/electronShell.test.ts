// Regression tests for the Electron preload bridge contract and the shutdown-dialog boundary.
// Electron 预加载桥契约与关闭弹窗边界的回归测试。

import { afterEach, describe, expect, it, vi } from "vitest";

import { readElectronDesktopBridge } from "@/shell/electronBridge";
import { createElectronShellClient } from "@/shell/electronShell";
import { isElectronRuntime } from "@/shell/electronRuntime";
import { isDesktopShutdownDialogRuntime, isTauriRuntime, applyShellHostMarker, detectShellHost } from "@/shell/runtime";

// installDesktopBridge mounts one candidate preload surface on the global window.
// installDesktopBridge 在全局 window 上挂载一个候选预加载表面。
function installDesktopBridge(desktop: Record<string, unknown> | undefined): void {
  const globalWindow = globalThis as unknown as { window?: Record<string, unknown> };
  const target = (globalWindow.window ?? {}) as Record<string, unknown>;
  if (desktop === undefined) {
    delete target.vulcanDesktop;
  } else {
    target.vulcanDesktop = { desktop };
  }
  globalWindow.window = target;
}

// completeBridge returns one bridge implementing the full documented contract.
// completeBridge 返回实现完整文档契约的桥。
function completeBridge(): Record<string, unknown> {
  return {
    nativeNotificationsAvailable: true,
    openDeveloperTools: vi.fn(async () => {}),
    minimizeWindow: vi.fn(async () => {}),
    toggleWindowMaximize: vi.fn(async () => {}),
    isWindowMaximized: vi.fn(async () => true),
    closeWindow: vi.fn(async () => {}),
    subscribeWindowResize: vi.fn(async () => () => {}),
    isWindowMinimized: vi.fn(async () => true),
    isWindowFocused: vi.fn(async () => false),
    isWindowVisible: vi.fn(async () => true),
    subscribeWindowFocusChange: vi.fn(async () => () => {}),
    getNotificationPermission: vi.fn(async () => "granted"),
    requestNotificationPermission: vi.fn(async () => "granted"),
    showNotification: vi.fn(async () => {}),
    subscribeNotificationActivation: vi.fn(async () => () => {}),
  };
}

afterEach(() => {
  installDesktopBridge(undefined);
});

describe("readElectronDesktopBridge", () => {
  it("returns a fully implemented bridge", () => {
    const desktop = completeBridge();
    installDesktopBridge(desktop);

    expect(readElectronDesktopBridge()).not.toBeNull();
    expect(isElectronRuntime()).toBe(true);
  });

  it("rejects the historical mismatched shape instead of failing later at call time", () => {
    // The removed duplicate contract used toggleMaximize/isMaximized/close/onResized names.
    // 已删除的重复契约曾使用 toggleMaximize/isMaximized/close/onResized 名称。
    installDesktopBridge({
      minimizeWindow: vi.fn(async () => {}),
      toggleMaximize: vi.fn(async () => {}),
      isMaximized: vi.fn(async () => true),
      close: vi.fn(async () => {}),
      onResized: vi.fn(async () => () => {}),
    });

    expect(readElectronDesktopBridge()).toBeNull();
    expect(isElectronRuntime()).toBe(false);
  });

  it("rejects a partially implemented bridge", () => {
    const desktop = completeBridge();
    delete desktop.showNotification;
    installDesktopBridge(desktop);

    expect(readElectronDesktopBridge()).toBeNull();
  });

  it("reports no bridge in a plain browser", () => {
    installDesktopBridge(undefined);

    expect(readElectronDesktopBridge()).toBeNull();
    expect(isElectronRuntime()).toBe(false);
  });
});

describe("ElectronShellClient", () => {
  it("exposes developer tools only when the current preload provides the action", async () => {
    const desktop = completeBridge();
    installDesktopBridge(desktop);
    const client = createElectronShellClient();

    await client.openDeveloperTools?.();
    expect(desktop.openDeveloperTools).toHaveBeenCalledOnce();

    delete desktop.openDeveloperTools;
    installDesktopBridge(desktop);
    expect(readElectronDesktopBridge()).not.toBeNull();
    expect(createElectronShellClient().openDeveloperTools).toBeUndefined();
  });

  it("reports source Electron as notification-unsupported when its shortcut identity is shared", async () => {
    // The main process denies native toasts for generic electron.exe; the renderer must agree.
    // 主进程拒绝通用 electron.exe 的原生通知；渲染进程也必须报告相同能力。
    const desktop = completeBridge();
    desktop.nativeNotificationsAvailable = false;
    installDesktopBridge(desktop);
    const client = createElectronShellClient();

    expect(client.getNotificationCapability()).toEqual({
      shellKind: "electron", supported: false, activationSupported: false,
    });
  });

  it("delegates every window action to the bridge and never to a missing method", async () => {
    const desktop = completeBridge();
    installDesktopBridge(desktop);
    const client = createElectronShellClient();

    await client.minimizeWindow();
    await client.toggleWindowMaximize();
    await client.closeWindow();
    await client.subscribeWindowResize(() => {});

    expect(await client.isWindowMaximized()).toBe(true);
    expect(client.supportsWindowControls()).toBe(true);
    expect(desktop.minimizeWindow).toHaveBeenCalledTimes(1);
    expect(desktop.toggleWindowMaximize).toHaveBeenCalledTimes(1);
    expect(desktop.closeWindow).toHaveBeenCalledTimes(1);
    expect(desktop.subscribeWindowResize).toHaveBeenCalledTimes(1);
  });

  it("derives presentation state from authoritative native window facts", async () => {
    const desktop = completeBridge();
    installDesktopBridge(desktop);
    const client = createElectronShellClient();

    expect(await client.getNotificationPresentationState()).toEqual({
      foreground: false,
      focused: false,
      visible: true,
    });
  });

  it("reports a missing bridge instead of silently pretending to support controls", async () => {
    installDesktopBridge(undefined);
    const client = createElectronShellClient();

    expect(client.supportsWindowControls()).toBe(false);
    expect(client.getNotificationCapability().supported).toBe(false);
    expect(await client.getNotificationPermission()).toBe("unsupported");
    await expect(client.minimizeWindow()).rejects.toThrow("Electron desktop bridge is unavailable.");
  });
});

describe("detectShellHost and applyShellHostMarker", () => {
  it("resolves the electron host from a complete bridge and publishes the marker", () => {
    installDesktopBridge(completeBridge());
    const documentStub = { documentElement: { dataset: {} as Record<string, string> } };
    const globalScope = globalThis as unknown as { document?: unknown };
    globalScope.document = documentStub;

    try {
      expect(detectShellHost()).toBe("electron");
      expect(applyShellHostMarker()).toBe("electron");
      // The CSS drag-region rules are scoped to this exact marker value.
      // CSS 拖动区域规则正好限定在该标记值上。
      expect(documentStub.documentElement.dataset.vulcanHost).toBe("electron");
    } finally {
      delete globalScope.document;
    }
  });

  it("resolves the browser host when no desktop bridge exists", () => {
    installDesktopBridge(undefined);

    expect(detectShellHost()).toBe("browser");
  });
});

describe("isDesktopShutdownDialogRuntime", () => {
  it("is false for an Electron host so GUI close never requests service shutdown", () => {
    // The durable-close dialog drives the Tauri service drain; Electron must not adopt it.
    // 耐久关闭弹窗驱动 Tauri 服务排空；Electron 不得采用它。
    installDesktopBridge(completeBridge());

    expect(isElectronRuntime()).toBe(true);
    expect(isTauriRuntime()).toBe(false);
    expect(isDesktopShutdownDialogRuntime()).toBe(false);
  });

  it("is true only for a Tauri host", () => {
    const globalWindow = globalThis as unknown as { window?: Record<string, unknown> };
    const target = (globalWindow.window ?? {}) as Record<string, unknown>;
    target.__TAURI_INTERNALS__ = {};
    globalWindow.window = target;

    try {
      expect(isTauriRuntime()).toBe(true);
      expect(isDesktopShutdownDialogRuntime()).toBe(true);
    } finally {
      delete target.__TAURI_INTERNALS__;
    }
  });
});
