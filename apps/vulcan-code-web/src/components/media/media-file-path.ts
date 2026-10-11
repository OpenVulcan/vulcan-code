import {
  decodeBase64UrlUtf8,
  encodedMediaAssetRouteFromUrl,
  localPathFromFileUrl,
} from "@/resources/localMedia";
import { resolveBrowserServiceBaseUrl } from "@/service/browser-service-discovery";
import type { ResourceId } from "@/service/conversation-event-window-store";
import {
  sessionResourceFilePath,
  sessionResourceFromContentUrl,
} from "@/service/conversation-session-resources-v1";

/** Encoded local-media route prefix shared by the desktop protocol and the browser service.
 * 桌面协议与浏览器服务共用的编码本地媒体路由前缀。 */
const MEDIA_ASSET_ROUTE = "/asset/";

/** Where one displayed media file lives: a host path, or a session resource only the service can resolve.
 * 一个展示中的媒体文件所在位置：宿主路径，或只能由服务解析的会话资源。 */
export type MediaFileLocator =
  | { kind: "path"; path: string }
  | { kind: "resource"; sessionId: string; resourceId: ResourceId };

/** Classifies one media locator without any I/O; remote and inline sources have no file.
 * 不进行任何 I/O 地分类媒体定位值；远程与内联来源没有文件。
 *
 * @param value a host path, file URL, local media URL, or session-resource content URL.
 *              宿主路径、file URL、本地媒体 URL 或会话资源内容 URL。
 * @returns the file locator, or null when the value names no local file.
 *          文件定位；不指向本地文件时返回 null。 */
export function mediaFileLocator(value: string | null | undefined): MediaFileLocator | null {
  const trimmed = value?.trim() ?? "";
  if (!trimmed) return null;
  const resource = sessionResourceFromContentUrl(trimmed);
  if (resource) return { kind: "resource", ...resource };
  if (/^file:/i.test(trimmed)) {
    const path = localPathFromFileUrl(trimmed);
    return path ? { kind: "path", path } : null;
  }
  const route = encodedMediaAssetRouteFromUrl(trimmed) ?? serviceAssetRoute(trimmed);
  if (route) {
    const path = decodeBase64UrlUtf8(route.slice(MEDIA_ASSET_ROUTE.length));
    return path ? { kind: "path", path } : null;
  }
  if (/^[a-z][a-z0-9+.-]*:\/\//i.test(trimmed) || /^(data|blob):/i.test(trimmed)) return null;
  return { kind: "path", path: trimmed };
}

/** Resolves the host file path behind one media locator.
 * 解析一个媒体定位值背后的宿主文件路径。
 *
 * @param locator the classified locator from \`mediaFileLocator\`.
 *                由 \`mediaFileLocator\` 分类得到的定位。
 * @returns the absolute host path; rejects when the service cannot resolve the resource.
 *          宿主绝对路径；服务无法解析资源时拒绝。 */
export async function resolveMediaFilePath(locator: MediaFileLocator): Promise<string> {
  return locator.kind === "path" ? locator.path : sessionResourceFilePath(locator.sessionId, locator.resourceId);
}

/** Returns the encoded asset route of one browser-service media URL.
 * 返回一个浏览器服务媒体 URL 的编码资产路由。 */
function serviceAssetRoute(value: string): string | null {
  try {
    const url = new URL(value);
    if (url.origin !== new URL(resolveBrowserServiceBaseUrl()).origin) return null;
    return url.pathname.startsWith(MEDIA_ASSET_ROUTE) ? url.pathname : null;
  } catch {
    return null;
  }
}
