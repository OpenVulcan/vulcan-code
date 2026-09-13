import { AuditTransport } from "./runtime";
export type { BrowserServiceConnectionDetail, BrowserServiceConnectionStatus } from "../../src/service/browser-service-discovery";
// Preserve the existing event name while preventing all production discovery imports.
// 保留既有事件名称，同时阻止所有生产发现导入。
export const BROWSER_SERVICE_CONNECTION_EVENT = "vulcan:browser-service-connection";
// Runtime presence describes the injected in-memory transport, never an actual service endpoint.
// 运行时存在表示注入的内存传输，绝非实际服务地址。
export function isBrowserServiceRuntime(): boolean { return true; }
// Resolve resource URLs to this isolated origin; no protected service port or token is available.
// 将资源地址解析到此隔离源；不存在受保护服务端口或令牌。
export function resolveBrowserServiceBaseUrl(): string { return typeof window === "undefined" ? "http://127.0.0.1:17618" : window.location.origin; }
// No fixture carries an access token.
// 任何夹具均不携带访问令牌。
export function resolveBrowserServiceAccessToken(): null { return null; }
// Fixed display identity never enters an actual terminal attachment.
// 固定显示身份绝不进入实际终端挂接。
export function resolveBrowserServiceClientId(): string { return "ui-audit-client"; }
// Only the in-memory transport is connected; no network lifecycle exists.
// 仅内存传输已连接，不存在网络生命周期。
export function currentBrowserServiceConnectionStatus(): "connected" { return "connected"; }
// BrowserServiceTransport aliases the in-memory class for legacy barrel exports.
// BrowserServiceTransport 为旧汇总导出别名到内存类。
export { AuditTransport as BrowserServiceTransport };
// Create only the isolated transport when a component uses the service barrel directly.
// 当组件直接使用服务汇总导出时，仅创建隔离传输。
export function createBrowserServiceTransport(): AuditTransport { return new AuditTransport(); }
