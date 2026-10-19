// readLengthToken resolves one registered kit length token to pixels on the document root.
// readLengthToken 在文档根节点上将一个已注册的 kit 长度令牌解析为像素。
// name is the custom property name. Returns pixels, or 0 where no layout engine resolves the token.
// name 为自定义属性名。返回像素值；没有布局引擎解析该令牌时返回 0。
function readLengthToken(name: string): number {
  if (typeof document === "undefined") {
    return 0;
  }
  const value = Number.parseFloat(getComputedStyle(document.documentElement).getPropertyValue(name));
  return Number.isFinite(value) ? value : 0;
}

// navigationColumnWidth reads the left navigation footprint from the kit tokens for width-based layout decisions.
// The tokens in ui-kit/styles/tokens.css are the only width source; they scale with the text tier.
// navigationColumnWidth 从 kit 令牌读取左侧导航占用宽度，供基于宽度的布局判断使用。
// ui-kit/styles/tokens.css 中的令牌是唯一宽度来源，并随文字档位缩放。
// isMobile hides the column behind a drawer; panelOpen adds the panel beside the rail. Returns pixels.
// isMobile 表示导航收入抽屉；panelOpen 表示轨道旁面板展开。返回像素值。
export function navigationColumnWidth(isMobile: boolean, panelOpen: boolean): number {
  if (isMobile) {
    return 0;
  }
  return readLengthToken("--vk-rail-w") + (panelOpen ? readLengthToken("--vk-sidebar-w") : 0);
}
