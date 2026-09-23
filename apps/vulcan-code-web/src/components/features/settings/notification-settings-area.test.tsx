// @vitest-environment jsdom
import * as React from "react";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { NotificationSettingsArea } from "./notification-settings-area";
import { defaultNotificationPreferences, loadNotificationPreferences, saveNotificationPreferences } from "@/notifications/preferences";

// shell mocks native calls so UI verification never sends an actual notification.
// shell 模拟原生调用，使界面验证永不发送真实通知。
const { shell } = vi.hoisted(() => ({ shell: {
  getNotificationCapability: vi.fn(() => ({ supported: true, shellKind: "tauri", activationSupported: false })),
  getNotificationPermission: vi.fn(),
  requestNotificationPermission: vi.fn(),
  subscribeNotificationPresentationChange: vi.fn(),
  showLocalNotification: vi.fn(),
} }));
vi.mock("@/service/webui-service-runtime", () => ({ shellClient: shell }));
vi.mock("@/i18n", () => ({ useI18n: () => ({ locale: "zh-CN", t: (key: string) => key }) }));

// container and root own a fresh isolated client mount for every scenario.
// container 与 root 为每个场景持有新的隔离客户端挂载。
let container: HTMLDivElement;
let root: Root;

beforeEach(() => {
  vi.clearAllMocks();
  shell.getNotificationCapability.mockReturnValue({ supported: true, shellKind: "tauri", activationSupported: false });
  // storageEntries supplies isolated device storage because Node also exposes an optional localStorage global.
  // storageEntries 提供隔离设备存储，避免 Node 自身的可选 localStorage 全局与浏览器环境冲突。
  const storageEntries = new Map<string, string>();
  vi.stubGlobal("localStorage", {
    getItem: (key: string) => storageEntries.get(key) ?? null,
    setItem: vi.fn((key: string, value: string) => { storageEntries.set(key, value); }),
  });
  shell.getNotificationPermission.mockResolvedValue("granted");
  shell.requestNotificationPermission.mockResolvedValue("granted");
  shell.subscribeNotificationPresentationChange.mockResolvedValue(() => undefined);
  shell.showLocalNotification.mockResolvedValue(undefined);
  container = document.createElement("div");
  document.body.appendChild(container);
  root = createRoot(container);
  Object.assign(globalThis, { IS_REACT_ACT_ENVIRONMENT: true });
});
afterEach(async () => {
  await act(async () => root.unmount());
  container.remove();
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

// renderSettings flushes permission reads from the actual component lifecycle.
// renderSettings 刷新实际组件生命周期中的权限读取。
// Parameters: none. Returns once rendering and queued permission work settle.
// 参数：无。返回值：渲染与已排队权限工作结束后完成。
async function renderSettings(): Promise<void> {
  await act(async () => { root.render(<NotificationSettingsArea />); });
}

describe("notification preferences and permission UI", () => {
  // A pending submission must lock duplicate actions and recover after an explicit native failure.
  // 正在投递时必须锁住重复操作，并在明确的原生失败后恢复操作能力。
  it("locks a pending test once and restores controls after delivery fails", async () => {
    // preferences enables only this isolated test client, without granting real system permission.
    // preferences 仅启用当前隔离测试客户端，不授予真实系统权限。
    const preferences = defaultNotificationPreferences();
    preferences.channels.local.enabled = true;
    saveNotificationPreferences(preferences);
    // rejectDelivery keeps the native result pending until both immediate clicks have been processed.
    // rejectDelivery 保持原生结果未完成，直到两个连续点击均处理完毕。
    let rejectDelivery!: (reason: Error) => void;
    shell.showLocalNotification.mockImplementationOnce(() => new Promise<void>((_resolve, reject) => {
      rejectDelivery = reject;
    }));
    await renderSettings();
    // testButton and localToggle are the two public actions that must not overlap the pending send.
    // testButton 与 localToggle 是不能和进行中投递重叠的两个公开操作。
    const testButton = Array.from(container.querySelectorAll<HTMLButtonElement>("button"))
      .find((button) => button.textContent?.includes("发一条试试"))!;
    const localToggle = container.querySelector<HTMLButtonElement>('[role="switch"]')!;
    await act(async () => { testButton.click(); testButton.click(); });
    expect(shell.showLocalNotification).toHaveBeenCalledTimes(1);
    expect(testButton.disabled).toBe(true);
    expect(localToggle.disabled).toBe(true);
    await act(async () => { rejectDelivery(new Error("native delivery rejected")); });
    expect(testButton.disabled).toBe(false);
    expect(localToggle.disabled).toBe(false);
    expect(container.querySelector('[role="alert"]')?.textContent).toContain("native delivery rejected");
    expect(loadNotificationPreferences().channels.local.enabled).toBe(true);
    expect(shell.requestNotificationPermission).not.toHaveBeenCalled();
  });

  // A failed native policy lookup must not impersonate a still-pending permission prompt.
  // 原生策略查询失败不能冒充仍在等待的权限请求。
  it("reports permission query failure and recovers through an explicit recheck", async () => {
    const preferences = defaultNotificationPreferences();
    preferences.channels.local.enabled = true;
    saveNotificationPreferences(preferences);
    shell.getNotificationPermission.mockRejectedValueOnce(new Error("Read Windows notification policy: 0x80070490"));
    await renderSettings();
    expect(container.textContent).toContain("权限检查失败");
    expect(container.textContent).not.toContain("等待权限");
    const retry = Array.from(container.querySelectorAll<HTMLButtonElement>("button")).find((button) => button.textContent === "重新检查")!;
    await act(async () => retry.click());
    expect(container.textContent).toContain("已启用");
    expect(container.querySelector('[role="alert"]')).toBeNull();
    expect(shell.requestNotificationPermission).not.toHaveBeenCalled();
  });

  // Restart restoration must preserve enabled intent without asking for already granted permission.
  // 重启恢复必须保留启用意图，不为已经获得的权限重复请求。
  it("restores enabled preferences on mount without prompting", async () => {
    // preferences represents the device-local document written during the previous run.
    // preferences 表示上次运行写入的设备本地文档。
    const preferences = defaultNotificationPreferences();
    preferences.channels.local.enabled = true;
    saveNotificationPreferences(preferences);
    await renderSettings();
    expect(container.querySelector('[role="switch"]')?.getAttribute("aria-checked")).toBe("true");
    expect(shell.requestNotificationPermission).not.toHaveBeenCalled();
    expect(container.textContent).toContain("已启用");
  });

  // A saved Web preference cannot reopen the browser permission prompt without a user gesture.
  // 已保存的网页偏好不能在缺少用户操作时重新打开浏览器权限弹框。
  it("waits for an explicit click before requesting saved browser permission", async () => {
    // preferences represents the enabled state restored into this exact browser client.
    // preferences 表示恢复到当前精确浏览器客户端的开启状态。
    const preferences = defaultNotificationPreferences();
    preferences.channels.local.enabled = true;
    saveNotificationPreferences(preferences);
    shell.getNotificationCapability.mockReturnValue({ supported: true, shellKind: "browser", activationSupported: true });
    shell.getNotificationPermission.mockResolvedValue("default");
    await renderSettings();
    expect(shell.requestNotificationPermission).not.toHaveBeenCalled();
    // requestButton is visible because this client still lacks authorization.
    // requestButton 可见是因为当前客户端尚未获得授权。
    const requestButton = Array.from(container.querySelectorAll<HTMLButtonElement>("button"))
      .find((button) => button.textContent === "允许通知")!;
    await act(async () => requestButton.click());
    expect(shell.requestNotificationPermission).toHaveBeenCalledOnce();
    expect(container.textContent).toContain("已启用");
  });

  // An enable click must call the browser request directly, before a separate asynchronous read.
  // 开启点击必须直接调用浏览器申请，不得先经过另一次异步读取。
  it("requests permission directly from a newly enabled browser client", async () => {
    shell.getNotificationCapability.mockReturnValue({ supported: true, shellKind: "browser", activationSupported: true });
    shell.getNotificationPermission.mockResolvedValue("default");
    await renderSettings();
    // readsBeforeClick captures the completed mount-time policy read.
    // readsBeforeClick 记录挂载时已完成的策略读取次数。
    const readsBeforeClick = shell.getNotificationPermission.mock.calls.length;
    const toggle = container.querySelector<HTMLButtonElement>('[role="switch"]')!;
    await act(async () => toggle.click());
    expect(shell.requestNotificationPermission).toHaveBeenCalledOnce();
    expect(shell.getNotificationPermission.mock.calls.length).toBe(readsBeforeClick);
    expect(loadNotificationPreferences().channels.local.enabled).toBe(true);
  });

  // Revocation blocks delivery but must not erase the saved intent or prevent explicit disabling.
  // 撤销权限阻止投递，但不能抹掉已保存意图或阻止用户明确关闭。
  it("allows disabling a persisted channel after permission is revoked", async () => {
    // preferences holds the formerly enabled channel.
    // preferences 持有此前已启用的通道。
    const preferences = defaultNotificationPreferences();
    preferences.channels.local.enabled = true;
    saveNotificationPreferences(preferences);
    shell.getNotificationPermission.mockResolvedValue("denied");
    await renderSettings();
    // toggle is the user's local channel preference, independent from permission.
    // toggle 是独立于权限的用户本地通道偏好。
    const toggle = container.querySelector<HTMLButtonElement>('[role="switch"]')!;
    expect(toggle.disabled).toBe(false);
    await act(async () => toggle.click());
    expect(loadNotificationPreferences().channels.local.enabled).toBe(false);
    expect(shell.requestNotificationPermission).not.toHaveBeenCalled();
  });

  // A failed native test must be visible, not silently converted into a success message.
  // 原生测试失败必须可见，不能静默转成成功消息。
  it("shows test delivery rejection and keeps preferences", async () => {
    // preferences enables the explicit test action.
    // preferences 启用明确测试操作。
    const preferences = defaultNotificationPreferences();
    preferences.channels.local.enabled = true;
    saveNotificationPreferences(preferences);
    shell.showLocalNotification.mockRejectedValue(new Error("native unavailable"));
    await renderSettings();
    // testButton is found by its user-facing purpose rather than DOM position.
    // testButton 按面向用户的用途定位，不依赖 DOM 位置。
    const testButton = Array.from(container.querySelectorAll<HTMLButtonElement>("button")).find((button) => button.textContent?.includes("发一条试试"))!;
    await act(async () => testButton.click());
    expect(container.querySelector('[role="alert"]')?.textContent).toContain("native unavailable");
    expect(loadNotificationPreferences().channels.local.enabled).toBe(true);
    expect(container.textContent).not.toContain("测试已提交给系统");
  });

  // A failed preflight must invalidate the old permission fact and never submit the toast.
  // 发送前检查失败必须使旧权限事实失效，且绝不提交通知。
  it("reports failed test preflight as unknown permission instead of enabled delivery", async () => {
    // preferences enables this manual test without changing the permission authority.
    // preferences 启用此次手动测试，不改变权限权威。
    const preferences = defaultNotificationPreferences();
    preferences.channels.local.enabled = true;
    saveNotificationPreferences(preferences);
    await renderSettings();
    shell.getNotificationPermission.mockRejectedValueOnce(new Error("policy unavailable"));
    // testButton triggers the real component's immediately preceding policy read.
    // testButton 触发真实组件在发送前的即时策略读取。
    const testButton = Array.from(container.querySelectorAll<HTMLButtonElement>("button")).find((button) => button.textContent?.includes("发一条试试"))!;
    await act(async () => testButton.click());
    expect(shell.showLocalNotification).not.toHaveBeenCalled();
    expect(container.textContent).toContain("权限检查失败");
    expect(container.textContent).not.toContain("已启用");
    expect(testButton.disabled).toBe(true);
    expect(loadNotificationPreferences().channels.local.enabled).toBe(true);
  });

  // Persistence failures must not display an enabled switch that will be lost at restart.
  // 持久化失败不能显示重启后会丢失的已启用开关。
  it("keeps the previous switch value when local storage fails", async () => {
    await renderSettings();
    vi.mocked(window.localStorage.setItem).mockImplementation(() => { throw new Error("storage unavailable"); });
    // toggle remains unchanged until durable preference writing succeeds.
    // toggle 在耐久偏好写入成功前保持不变。
    const toggle = container.querySelector<HTMLButtonElement>('[role="switch"]')!;
    await act(async () => toggle.click());
    expect(toggle.getAttribute("aria-checked")).toBe("false");
    expect(container.querySelector('[role="alert"]')?.textContent).toContain("storage unavailable");
    expect(shell.requestNotificationPermission).not.toHaveBeenCalled();
  });
});
