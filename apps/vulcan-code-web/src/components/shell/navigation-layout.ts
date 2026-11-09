// readLengthToken resolves one registered kit length token to pixels, on a matching element or the document root.
// readLengthToken 在匹配元素或文档根节点上将一个已注册的 kit 长度令牌解析为像素。
// name is the custom property name; selector picks where it resolves, the root when omitted or unmatched.
// Returns pixels, or 0 where no layout engine resolves the token.
// name 为自定义属性名；selector 选择解析位置，省略或无匹配时为根节点。返回像素值；没有布局引擎解析该令牌时返回 0。
function readLengthToken(name: string, selector?: string): number {
  if (typeof document === "undefined") {
    return 0;
  }
  const element = (selector ? document.querySelector(selector) : null) ?? document.documentElement;
  const value = Number.parseFloat(getComputedStyle(element).getPropertyValue(name));
  return Number.isFinite(value) ? value : 0;
}

// navigationColumnWidth reads the left navigation footprint from the kit tokens for width-based layout decisions.
// The tokens in ui-kit/styles/tokens.css are the only width source; they scale with the text tier. The panel width
// resolves on the mounted panel, since feature areas narrow it there.
// navigationColumnWidth 从 kit 令牌读取左侧导航占用宽度，供基于宽度的布局判断使用。
// ui-kit/styles/tokens.css 中的令牌是唯一宽度来源，并随文字档位缩放。面板宽度在已挂载的面板上解析，因为功能区会在那里将其收窄。
// isMobile hides the column behind a drawer; panelOpen adds the panel beside the rail. Returns pixels.
// isMobile 表示导航收入抽屉；panelOpen 表示轨道旁面板展开。返回像素值。
export function navigationColumnWidth(isMobile: boolean, panelOpen: boolean): number {
  if (isMobile) {
    return 0;
  }
  return readLengthToken("--vk-rail-w") + (panelOpen ? readLengthToken("--vk-sidebar-w", ".vk-shell-frame > .vk-sidebar") : 0);
}

// conversationReservedWidth reads the stage width the conversation keeps before the context panel floats and
// the navigation folds. It follows the reading-width preset and the text tier through the kit token.
// conversationReservedWidth 读取会话在上下文面板转为浮动、导航折叠之前保留的舞台宽度。
// 该值通过 kit 令牌跟随阅读宽度档位与文字档位。
// Returns pixels, or 0 where no layout engine resolves the token.
// 返回像素值；没有布局引擎解析该令牌时返回 0。
export function conversationReservedWidth(): number {
  return readLengthToken("--vk-reserve-w");
}

// nativeWindowButtonsStrip reads where the operating system's window buttons go: the title strip beside the
// activity rail. Both lengths follow the text tier through the kit tokens.
// nativeWindowButtonsStrip 读取操作系统窗口按钮所在的位置：活动轨道旁的标题条。两个长度都通过 kit 令牌跟随文字档位。
// Returns the rail width as the strip's left edge and the title strip height, in pixels.
// 返回作为标题条左边缘的轨道宽度及标题条高度，单位为像素。
export function nativeWindowButtonsStrip(): { left: number; height: number } {
  return { left: readLengthToken("--vk-rail-w"), height: readLengthToken("--vk-titlebar-h") };
}
