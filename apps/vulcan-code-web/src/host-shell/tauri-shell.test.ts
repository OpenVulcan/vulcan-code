import { beforeEach, describe, expect, it, vi } from "vitest";
import { TauriShellClient } from "./tauri-shell";

// invokeMock observes native commands without displaying real system notifications.
// invokeMock 观察原生命令，不展示真实系统通知。
const { invokeMock } = vi.hoisted(() => ({ invokeMock: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: invokeMock }));

beforeEach(() => { invokeMock.mockReset(); });

describe("native notification authority", () => {
  // Native reads must remain independent from the JS notification shim cache across client restarts.
  // 原生读取在客户端重建后仍须独立于 JS 通知垫片缓存。
  it("reads permission from the native command for each new client", async () => {
    invokeMock.mockResolvedValue(true);
    expect(await new TauriShellClient().getNotificationPermission()).toBe("granted");
    expect(await new TauriShellClient().getNotificationPermission()).toBe("granted");
    expect(invokeMock.mock.calls).toEqual([
      ["notification_permission"],
      ["notification_permission"],
    ]);
  });

  // Negative and undecided permission facts must retain distinct states.
  // 拒绝与未决定权限事实必须保持不同状态。
  it("distinguishes denial from a pending permission decision", async () => {
    // shell holds one client across changing native permission snapshots.
    // shell 在原生权限快照变化时保持同一客户端。
    const shell = new TauriShellClient();
    invokeMock.mockResolvedValueOnce(false).mockResolvedValueOnce(null).mockResolvedValueOnce(true);
    expect(await shell.getNotificationPermission()).toBe("denied");
    expect(await shell.getNotificationPermission()).toBe("default");
    expect(await shell.getNotificationPermission()).toBe("granted");
  });

  // Native delivery rejections must reach the endpoint instead of being reported as success.
  // 原生投递拒绝必须传递到端点，不能被报告为成功。
  it("awaits the native notify command and propagates IPC failure", async () => {
    invokeMock.mockRejectedValue(new Error("notification transport unavailable"));
    await expect(new TauriShellClient().showLocalNotification({ id: "test", title: "Title", body: "Body" }))
      .rejects.toThrow("notification transport unavailable");
    expect(invokeMock).toHaveBeenCalledWith("show_notification", { options: { title: "Title", body: "Body" } });
  });

  // The plugin's two explicit prompt variants map to the shared undecided state.
  // 插件的两种明确询问状态映射为共享未决定状态。
  it.each(["prompt", "prompt-with-rationale"])("normalizes %s after an explicit permission action", async (permission) => {
    invokeMock.mockResolvedValue(permission);
    expect(await new TauriShellClient().requestNotificationPermission()).toBe("default");
    expect(invokeMock).toHaveBeenCalledWith("request_notification_permission");
  });
});
