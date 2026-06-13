import {
  createBrowserServiceTransport,
  createServiceClient,
  isBrowserServiceRuntime,
} from "@/service";
import { createShellClient } from "@/shell";

// serviceClient is the shared business service facade used by the WebUI.
// serviceClient 是 WebUI 共享使用的业务服务门面。
export const serviceClient = createServiceClient(
  createBrowserServiceTransport(),
);

// shellClient is the shared client-host facade used by the WebUI.
// shellClient 是 WebUI 共享使用的客户端宿主门面。
export const shellClient = createShellClient();

// BROWSER_SERVICE_CONNECTION_TOAST_ID keeps reconnect notifications collapsed into one toast.
// BROWSER_SERVICE_CONNECTION_TOAST_ID 将重连通知折叠到同一个 toast。
export const BROWSER_SERVICE_CONNECTION_TOAST_ID = "browser-service-connection";

// hasServiceRuntime returns whether the page can use a backend service transport.
// hasServiceRuntime 返回页面是否可以使用后端服务传输。
export function hasServiceRuntime(): boolean {
  return isBrowserServiceRuntime();
}

// serviceBootstrapErrorIsRetryable returns whether the initial service load should be retried.
// serviceBootstrapErrorIsRetryable 判断初始服务加载失败是否应该自动重试。
export function serviceBootstrapErrorIsRetryable(error: unknown): boolean {
  const message = error instanceof Error ? error.message : String(error);
  return message.includes("Browser service is unavailable")
    || message.includes("Failed to fetch")
    || message.includes("NetworkError")
    || message.includes("Load failed");
}
