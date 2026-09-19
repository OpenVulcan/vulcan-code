// @vitest-environment jsdom
import { afterEach, describe, expect, it, vi } from "vitest";
import { BrowserShellClient } from "@/shell/browserShell";

afterEach(() => {
  vi.unstubAllGlobals();
});

describe("BrowserShellClient notifications", () => {
  it("requests permission from the owning browser page", async () => {
    // requestPermission represents the standard Web Notification permission prompt.
    // requestPermission 表示标准 Web Notification 权限提示。
    const requestPermission = vi.fn(async () => "granted" as NotificationPermission);
    vi.stubGlobal("Notification", {
      permission: "default",
      requestPermission,
    });

    // permission is the result returned to the settings action in this browser client.
    // permission 是返回给当前浏览器客户端设置操作的结果。
    const permission = await new BrowserShellClient().requestNotificationPermission();
    expect(permission).toBe("granted");
    expect(requestPermission).toHaveBeenCalledOnce();
  });

  it("reports unsupported without a browser Notification API", async () => {
    vi.stubGlobal("Notification", undefined);
    expect(await new BrowserShellClient().requestNotificationPermission()).toBe("unsupported");
  });

  it("presents and activates a notification through the current browser page", async () => {
    // presented records notifications constructed in this browser page only.
    // presented 只记录当前浏览器页面构造的通知。
    const presented: Array<{ title: string; options: NotificationOptions; onclick: (() => void) | null; close: ReturnType<typeof vi.fn> }> = [];
    // PageNotification models the browser-owned constructor and click callback.
    // PageNotification 模拟浏览器拥有的构造器与点击回调。
    class PageNotification {
      // permission and requestPermission are the current page's authorization API.
      // permission 与 requestPermission 是当前页面的授权 API。
      static permission = "granted";
      static requestPermission = vi.fn(async () => "granted");
      // title and options preserve the exact request submitted to this page.
      // title 与 options 保留提交给当前页面的精确请求。
      readonly title: string;
      readonly options: NotificationOptions;
      // onclick is installed by BrowserShellClient after construction.
      // onclick 由 BrowserShellClient 在构造后安装。
      onclick: (() => void) | null = null;
      // close records cleanup after a user clicks the browser notification.
      // close 记录用户点击浏览器通知后的清理。
      close = vi.fn();

      // constructor stores the browser notification payload and makes the instance observable.
      // constructor 保存浏览器通知载荷，并使实例可供测试观察。
      constructor(title: string, options: NotificationOptions) {
        this.title = title;
        this.options = options;
        presented.push(this);
      }
    }
    vi.stubGlobal("Notification", PageNotification);
    const focus = vi.spyOn(window, "focus").mockImplementation(() => undefined);
    const shell = new BrowserShellClient();
    const activation = vi.fn();
    await shell.subscribeNotificationActivation(activation);

    await shell.showLocalNotification({ id: "notification:run-1", title: "Browser page", body: "Done" });
    expect(presented).toHaveLength(1);
    expect(presented[0].title).toBe("Browser page");
    expect(presented[0].options).toMatchObject({ body: "Done", tag: "notification:run-1" });
    presented[0].onclick?.();
    expect(activation).toHaveBeenCalledWith({ notificationId: "notification:run-1" });
    expect(focus).toHaveBeenCalledOnce();
    expect(presented[0].close).toHaveBeenCalledOnce();
    focus.mockRestore();
  });
});
