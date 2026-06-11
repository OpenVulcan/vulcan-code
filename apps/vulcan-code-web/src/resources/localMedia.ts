import { resolveBrowserServiceAccessToken, resolveBrowserServiceBaseUrl } from "@/service";

// encodeBase64UrlUtf8 converts a string to URL-safe base64.
// encodeBase64UrlUtf8 将字符串转换为 URL 安全的 base64。
export function encodeBase64UrlUtf8(value: string): string {
  const bytes = new TextEncoder().encode(value);
  let binary = "";
  bytes.forEach((byte) => {
    binary += String.fromCharCode(byte);
  });
  return btoa(binary).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/g, "");
}

// localPathFromFileUrl extracts a filesystem path from a legacy file URL.
// localPathFromFileUrl 从旧版 file URL 中提取文件系统路径。
export function localPathFromFileUrl(value: string): string | null {
  try {
    const url = new URL(value);
    if (url.protocol !== "file:") {
      return null;
    }
    const decodedPath = decodeURIComponent(url.pathname);
    return decodedPath.replace(/^\/([A-Za-z]:\/)/, "$1").replace(/\//g, "\\");
  } catch {
    return null;
  }
}

// encodedMediaAssetRouteFromUrl extracts an existing encoded media route from legacy local media URLs.
// encodedMediaAssetRouteFromUrl 从旧版本地媒体 URL 中提取已有编码媒体路由。
export function encodedMediaAssetRouteFromUrl(value: string): string | null {
  try {
    const url = new URL(value);
    const isTauriMediaProtocol = url.protocol === "vulcan-media:";
    const isTauriMediaHost = (url.protocol === "http:" || url.protocol === "https:") && url.hostname.toLowerCase() === "vulcan-media.localhost";
    if (!isTauriMediaProtocol && !isTauriMediaHost) {
      return null;
    }
    if (!url.pathname.startsWith("/asset/")) {
      return null;
    }
    return url.pathname;
  } catch {
    return null;
  }
}

// browserMediaAssetUrlFromEncodedRoute converts one encoded media route to the browser service URL.
// browserMediaAssetUrlFromEncodedRoute 将单个编码媒体路由转换为浏览器服务 URL。
function browserMediaAssetUrlFromEncodedRoute(encodedRoute: string): string {
  const token = resolveBrowserServiceAccessToken();
  const tokenQuery = token ? `?token=${encodeURIComponent(token)}` : "";
  return `${resolveBrowserServiceBaseUrl()}${encodedRoute}${tokenQuery}`;
}

// localMediaSourceUrl converts local media paths into URLs that WebUI can load without file:// access.
// localMediaSourceUrl 将本地媒体路径转换为 WebUI 可加载且不依赖 file:// 的资源路由。
export function localMediaSourceUrl(path: string): string {
  const existingEncodedRoute = encodedMediaAssetRouteFromUrl(path);
  if (existingEncodedRoute) {
    return browserMediaAssetUrlFromEncodedRoute(existingEncodedRoute);
  }
  if (/^(https?:|data:|blob:|asset:)/i.test(path)) {
    return path;
  }
  const localPath = /^file:/i.test(path) ? localPathFromFileUrl(path) : path;
  if (!localPath) {
    return "";
  }
  const encodedPath = encodeBase64UrlUtf8(localPath);
  return browserMediaAssetUrlFromEncodedRoute(`/asset/${encodedPath}`);
}
