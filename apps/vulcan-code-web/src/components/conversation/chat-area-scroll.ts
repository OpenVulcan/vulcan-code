// ConversationScrollSnapshot stores one semantic transcript viewport restoration point.
// ConversationScrollSnapshot 保存单个转录界面的语义视口恢复点。
export interface ConversationScrollSnapshot {
  // Stable business-row key visible at the top of the viewport, or null when none is available.
  // 视口顶部可见的稳定业务行键；不可用时为 null。
  anchorKey: string | null;
  // Saved anchor-row top relative to the viewport top in CSS pixels.
  // 保存的锚点行顶部相对视口顶部的 CSS 像素偏移。
  anchorTop: number;
  // Native scrollTop used as the deterministic coarse fallback.
  // 作为确定性粗恢复回退值的原生 scrollTop。
  scrollTop: number;
  // Whether subsequent geometry growth must remain pinned to the transcript tail.
  // 后续几何增长是否必须持续贴住转录尾部。
  followTail: boolean;
}

// ConversationViewportAnchorSnapshot stores one visible row anchor before layout changes.
// ConversationViewportAnchorSnapshot 保存布局变化前的一条可见行锚点。
export interface ConversationViewportAnchorSnapshot {
  // Stable row key captured from the timeline Turn boundary.
  // 从时间线 Turn 边界捕获的稳定行键。
  rowKey: string;
  // Row top offset relative to the scroll viewport top.
  // 行顶部相对滚动视口顶部的偏移。
  offsetTop: number;
}

// conversationDistanceFromBottom returns the current native distance below the viewport.
// conversationDistanceFromBottom 返回当前视口下方的原生距离。
// Parameters: element is the owning transcript scrollport.
// 参数：element 是转录所属的滚动视口。
// Returns: a non-negative CSS-pixel distance from viewport bottom to content bottom.
// 返回值：从视口底部到内容底部的非负 CSS 像素距离。
export function conversationDistanceFromBottom(element: HTMLElement): number {
  return Math.max(0, element.scrollHeight - element.scrollTop - element.clientHeight);
}

// captureConversationViewportAnchor captures the top visible Turn through hit testing before falling back to a DOM scan.
// captureConversationViewportAnchor 优先通过命中测试捕获顶部可见 Turn，随后回退到 DOM 扫描。
// Parameters: element is the owning transcript scrollport.
// 参数：element 是转录所属的滚动视口。
// Returns: the top visible stable row and offset, or null when no business row intersects the viewport.
// 返回值：顶部可见稳定行及其偏移；没有业务行与视口相交时返回 null。
export function captureConversationViewportAnchor(element: HTMLElement): ConversationViewportAnchorSnapshot | null {
  // viewportRect is the native geometry boundary used by both the fast and fallback paths.
  // viewportRect 是快速路径与回退路径共同使用的原生几何边界。
  const viewportRect = element.getBoundingClientRect();
  // fastAnchor uses browser hit testing to avoid scanning a long mounted transcript during normal scrolling.
  // fastAnchor 使用浏览器命中测试，避免在正常滚动期间扫描长转录。
  const fastAnchor = captureConversationViewportAnchorFromPoint(element, viewportRect);
  if (fastAnchor) {
    return fastAnchor;
  }
  // rows is the mounted Turn collection used only when hit testing is unavailable or obscured.
  // rows 是仅在命中测试不可用或被遮挡时使用的已挂载 Turn 集合。
  const rows = element.querySelectorAll<HTMLElement>("[data-conversation-row-key]");
  for (const row of rows) {
    // anchor is non-null only when this exact row intersects the visual viewport.
    // anchor 仅在当前精确行与视觉视口相交时非空。
    const anchor = conversationViewportAnchorForRow(element, row, viewportRect);
    if (anchor) {
      return anchor;
    }
  }
  return null;
}

// captureConversationScrollSnapshot records native position, semantic anchor, and tail ownership together.
// captureConversationScrollSnapshot 同时记录原生位置、语义锚点与尾部所有权。
// Parameters: element is the scrollport; followTail is the controller's current ownership state.
// 参数：element 是滚动视口；followTail 是控制器当前的所有权状态。
// Returns: one complete immutable restoration snapshot.
// 返回值：一份完整不可变恢复快照。
export function captureConversationScrollSnapshot(
  element: HTMLElement,
  followTail: boolean,
): ConversationScrollSnapshot {
  // anchor is unnecessary while tail ownership is explicit and active.
  // 尾部所有权已明确且生效时不需要锚点。
  const anchor = followTail ? null : captureConversationViewportAnchor(element);
  return {
    anchorKey: anchor?.rowKey ?? null,
    anchorTop: anchor?.offsetTop ?? 0,
    followTail,
    scrollTop: element.scrollTop,
  };
}

// conversationScrollTopForSnapshot calculates one restore target without mutating the DOM.
// conversationScrollTopForSnapshot 在不修改 DOM 的情况下计算一个恢复目标。
// Parameters: element is the current scrollport; snapshot is the saved semantic viewport state.
// 参数：element 是当前滚动视口；snapshot 是已保存的语义视口状态。
// Returns: a clamped scrollTop target using tail, semantic anchor, then native position order.
// 返回值：按尾部、语义锚点、原生位置顺序得到的已约束 scrollTop 目标。
export function conversationScrollTopForSnapshot(
  element: HTMLElement,
  snapshot: ConversationScrollSnapshot,
): number {
  // maxScrollTop is the browser-valid native scroll range for the current geometry.
  // maxScrollTop 是当前几何下浏览器有效的原生滚动范围。
  const maxScrollTop = Math.max(0, element.scrollHeight - element.clientHeight);
  if (snapshot.followTail) {
    return maxScrollTop;
  }
  if (snapshot.anchorKey) {
    // rows is scanned only during restoration, never on every streaming frame.
    // rows 仅在恢复期间扫描，不会在每个流式帧中扫描。
    const rows = element.querySelectorAll<HTMLElement>("[data-conversation-row-key]");
    // viewportRect anchors row geometry to the owning scrollport coordinate space.
    // viewportRect 将行几何锚定到所属滚动视口坐标空间。
    const viewportRect = element.getBoundingClientRect();
    for (const row of rows) {
      if (row.dataset.conversationRowKey !== snapshot.anchorKey) {
        continue;
      }
      // rowRect plus the current scrollTop yields the row's stable content-space position.
      // rowRect 与当前 scrollTop 共同得到该行稳定的内容空间位置。
      const rowRect = row.getBoundingClientRect();
      const rowContentTop = rowRect.top - viewportRect.top + element.scrollTop;
      return clampConversationScrollTop(rowContentTop - snapshot.anchorTop, maxScrollTop);
    }
  }
  return clampConversationScrollTop(snapshot.scrollTop, maxScrollTop);
}

// conversationViewportAllowsTailFollow returns whether the timeline is at or near the bottom.
// conversationViewportAllowsTailFollow 返回时间线是否位于底部或接近底部。
// Parameters: element is the owning transcript scrollport.
// 参数：element 是转录所属的滚动视口。
// Returns: true when content fits or no more than twenty-four CSS pixels remain below the viewport.
// 返回值：当内容可容纳或视口下方不超过二十四个 CSS 像素时返回 true。
export function conversationViewportAllowsTailFollow(element: HTMLElement): boolean {
  // distanceFromBottom is the exact native distance used by the shared tail threshold.
  // distanceFromBottom 是共享尾部阈值使用的精确原生距离。
  const distanceFromBottom = conversationDistanceFromBottom(element);
  return element.scrollHeight <= element.clientHeight + 24 || distanceFromBottom <= 24;
}

// captureConversationViewportAnchorFromPoint resolves the top visible Turn through native hit testing.
// captureConversationViewportAnchorFromPoint 通过原生命中测试解析顶部可见 Turn。
// Parameters: element is the scrollport and viewportRect is its already-read geometry.
// 参数：element 是滚动视口；viewportRect 是已读取的视口几何。
// Returns: a stable row anchor when hit testing reaches a Turn owned by this scrollport.
// 返回值：命中测试到达当前滚动视口拥有的 Turn 时返回稳定行锚点。
function captureConversationViewportAnchorFromPoint(
  element: HTMLElement,
  viewportRect: DOMRect,
): ConversationViewportAnchorSnapshot | null {
  // ownerDocument supplies elementsFromPoint in real browser environments.
  // ownerDocument 在真实浏览器环境中提供 elementsFromPoint。
  const ownerDocument = element.ownerDocument;
  if (!ownerDocument || typeof ownerDocument.elementsFromPoint !== "function") {
    return null;
  }
  // probeY samples immediately inside the viewport so a partially visible top row remains selected.
  // probeY 在视口内部紧邻顶部处采样，使部分可见的顶部行仍能被选中。
  const probeY = Math.min(viewportRect.bottom - 1, viewportRect.top + 1);
  // probeXs cover centered transcript content and both responsive edges without touching the scrollbar gutter.
  // probeXs 覆盖居中转录内容与两侧响应式边缘，同时避开滚动条槽。
  const probeXs = [
    viewportRect.left + viewportRect.width / 2,
    Math.min(viewportRect.right - 24, viewportRect.left + 48),
    Math.max(viewportRect.left + 24, viewportRect.right - 48),
  ];
  for (const probeX of probeXs) {
    for (const hit of ownerDocument.elementsFromPoint(probeX, probeY)) {
      // row is the nearest stable Turn boundary behind overlays at the probe point.
      // row 是探测点处覆盖层后方最近的稳定 Turn 边界。
      const row = hit.closest<HTMLElement>("[data-conversation-row-key]");
      if (!row || !element.contains(row)) {
        continue;
      }
      // anchor verifies visual intersection before accepting the hit-tested row.
      // anchor 在接受命中行前验证其视觉相交关系。
      const anchor = conversationViewportAnchorForRow(element, row, viewportRect);
      if (anchor) {
        return anchor;
      }
    }
  }
  return null;
}

// conversationViewportAnchorForRow converts one intersecting Turn into a semantic anchor.
// conversationViewportAnchorForRow 将一条相交 Turn 转换为语义锚点。
// Parameters: element owns the row, row is the candidate, and viewportRect is the scrollport geometry.
// 参数：element 拥有该行；row 是候选行；viewportRect 是滚动视口几何。
// Returns: a stable anchor for an intersecting keyed row, otherwise null.
// 返回值：相交且带键的行返回稳定锚点，否则返回 null。
function conversationViewportAnchorForRow(
  element: HTMLElement,
  row: HTMLElement,
  viewportRect: DOMRect,
): ConversationViewportAnchorSnapshot | null {
  if (!element.contains(row)) {
    return null;
  }
  // rowKey is the business identity carried by the stable Turn boundary.
  // rowKey 是稳定 Turn 边界携带的业务身份。
  const rowKey = row.dataset.conversationRowKey;
  if (!rowKey) {
    return null;
  }
  // rowRect determines whether the candidate contributes visible pixels.
  // rowRect 判断候选行是否贡献了可见像素。
  const rowRect = row.getBoundingClientRect();
  if (rowRect.bottom <= viewportRect.top || rowRect.top >= viewportRect.bottom) {
    return null;
  }
  return {
    offsetTop: rowRect.top - viewportRect.top,
    rowKey,
  };
}

// clampConversationScrollTop constrains one target to the current native scroll range.
// clampConversationScrollTop 将一个目标限制在当前原生滚动范围内。
// Parameters: target is the requested scrollTop and maxScrollTop is the current upper bound.
// 参数：target 是请求的 scrollTop；maxScrollTop 是当前上界。
// Returns: a finite clamped scrollTop value.
// 返回值：有限且已约束的 scrollTop 值。
function clampConversationScrollTop(target: number, maxScrollTop: number): number {
  if (!Number.isFinite(target)) {
    return 0;
  }
  return Math.max(0, Math.min(maxScrollTop, target));
}
