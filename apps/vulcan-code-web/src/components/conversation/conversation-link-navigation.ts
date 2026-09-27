import type { MouseEvent } from "react";

// resolveConversationLinkTarget selects an ordinary transcript website or file hyperlink for desktop routing.
// resolveConversationLinkTarget 选择普通会话网页或文件超链接，交给桌面路由。
// event is the captured click and origin is the privileged workbench origin; returns its URL or null for native handling.
// event 是捕获的点击，origin 是特权工作台来源；返回链接 URL，不应接管时返回 null。
export function resolveConversationLinkTarget(event: MouseEvent<HTMLElement>, origin: string): URL | null {
  if (event.defaultPrevented || event.button !== 0 || event.ctrlKey || event.metaKey
    || event.shiftKey || event.altKey || !(event.target instanceof Element)) return null;
  // Downloads, resource previews and adjacent controls retain their original handlers.
  // 下载、资源预览和相邻控件保留原来的处理器。
  const anchor = event.target.closest<HTMLAnchorElement>("a[href]");
  if (!anchor || !anchor.closest(".conversation-scroll-content") || !event.currentTarget.contains(anchor)
    || anchor.hasAttribute("download") || anchor.closest(".conversation-v1-resource")) return null;
  try {
    // An empty sanitized href must not turn into the current workbench URL.
    // 清理后的空 href 不得变成当前工作台 URL。
    const destination = new URL(anchor.href);
    return ["http:", "https:", "file:"].includes(destination.protocol) && destination.origin !== origin
      ? destination : null;
  } catch {
    return null;
  }
}
