import { describe, expect, it } from "vitest";
import {
  captureConversationScrollSnapshot,
  captureConversationViewportAnchor,
  conversationScrollTopForSnapshot,
  conversationViewportAllowsTailFollow,
} from "./chat-area-scroll";

// createRect builds the minimal DOMRect geometry used by scroll contract tests.
// createRect 构建滚动契约测试使用的最小 DOMRect 几何对象。
// Parameters: top and bottom describe the vertical bounds in viewport coordinates.
// 参数：top 与 bottom 描述视口坐标中的垂直边界。
// Returns: a DOMRect-compatible object with deterministic geometry.
// 返回值：具有确定几何信息的 DOMRect 兼容对象。
function createRect(top: number, bottom: number): DOMRect {
  return {
    bottom,
    height: bottom - top,
    left: 0,
    right: 800,
    top,
    width: 800,
    x: 0,
    y: top,
    toJSON: () => ({}),
  };
}

// createRow builds one stable transcript Turn with fixed viewport geometry.
// createRow 构建一条具有固定视口几何的稳定转录 Turn。
// Parameters: key is the business identity and top/bottom are its visual bounds.
// 参数：key 是业务身份；top 与 bottom 是其视觉边界。
// Returns: an HTMLElement-compatible row used by the scroll helpers.
// 返回值：供滚动辅助函数使用的 HTMLElement 兼容行。
function createRow(key: string, top: number, bottom: number): HTMLElement {
  return {
    closest: () => null,
    dataset: { conversationRowKey: key },
    getBoundingClientRect: () => createRect(top, bottom),
  } as unknown as HTMLElement;
}

// createScrollElement builds one mutable scrollport without requiring a browser environment.
// createScrollElement 构建一个无需浏览器环境的可变滚动视口。
// Parameters: geometry describes the scroll range, viewport, mounted rows, and optional hit-test result.
// 参数：geometry 描述滚动范围、视口、已挂载行与可选命中测试结果。
// Returns: an HTMLElement-compatible scroll owner.
// 返回值：HTMLElement 兼容滚动所有者。
function createScrollElement(geometry: {
  clientHeight: number;
  hitRow?: HTMLElement | null;
  rows?: HTMLElement[];
  scrollHeight: number;
  scrollTop: number;
  viewportBottom?: number;
  viewportTop?: number;
}): HTMLElement {
  // rows is the exact mounted business-row collection exposed to the helper.
  // rows 是暴露给辅助函数的精确已挂载业务行集合。
  const rows = geometry.rows ?? [];
  // element is assigned before hit-test closures so contains can use exact identity.
  // element 在命中测试闭包前赋值，使 contains 可以使用精确身份。
  const element = {
    clientHeight: geometry.clientHeight,
    contains: (candidate: Node) => rows.includes(candidate as unknown as HTMLElement),
    getBoundingClientRect: () => createRect(
      geometry.viewportTop ?? 0,
      geometry.viewportBottom ?? geometry.clientHeight,
    ),
    ownerDocument: geometry.hitRow === undefined
      ? undefined
      : {
          elementsFromPoint: () => geometry.hitRow
            ? [{ closest: () => geometry.hitRow }]
            : [],
        },
    querySelectorAll: () => rows,
    scrollHeight: geometry.scrollHeight,
    scrollTop: geometry.scrollTop,
  } as unknown as HTMLElement;
  return element;
}

describe("conversation scroll contract", () => {
  it("calculates a pinned surface target at the valid rendered tail", () => {
    // element has a nine-hundred-pixel valid native range.
    // element 拥有九百像素的有效原生滚动范围。
    const element = createScrollElement({ clientHeight: 300, scrollHeight: 1_200, scrollTop: 240 });

    expect(conversationScrollTopForSnapshot(element, {
      anchorKey: null,
      anchorTop: 0,
      followTail: true,
      scrollTop: 240,
    })).toBe(900);
  });

  it("calculates a semantic row target at its saved visual offset without a coarse write", () => {
    // row is currently lower than the saved forty-pixel viewport offset.
    // row 当前低于已保存的四十像素视口偏移位置。
    const row = createRow("turn-2", 260, 360);
    // element starts at two hundred pixels in a viewport whose document top is one hundred.
    // element 从二百像素开始，且其文档视口顶部为一百像素。
    const element = createScrollElement({
      clientHeight: 500,
      rows: [row],
      scrollHeight: 2_000,
      scrollTop: 200,
      viewportBottom: 600,
      viewportTop: 100,
    });

    expect(conversationScrollTopForSnapshot(element, {
      anchorKey: "turn-2",
      anchorTop: 40,
      followTail: false,
      scrollTop: 50,
    })).toBe(320);
    expect(element.scrollTop).toBe(200);
  });

  it("uses and clamps the native scrollTop when the semantic row is absent", () => {
    // element exposes no matching row and a seven-hundred-pixel valid range.
    // element 不暴露匹配行，且有效范围为七百像素。
    const element = createScrollElement({ clientHeight: 300, scrollHeight: 1_000, scrollTop: 50 });

    expect(conversationScrollTopForSnapshot(element, {
      anchorKey: "missing-turn",
      anchorTop: 20,
      followTail: false,
      scrollTop: 2_000,
    })).toBe(700);
  });

  it("captures the first intersecting mounted row through the fallback scan", () => {
    // rows contains one row above the viewport followed by the first visible Turn.
    // rows 包含一条位于视口上方的行，以及随后第一条可见 Turn。
    const rows = [createRow("turn-1", 20, 80), createRow("turn-2", 120, 220)];
    // element begins at one hundred pixels so only the second row intersects it.
    // element 从一百像素处开始，因此只有第二行与其相交。
    const element = createScrollElement({
      clientHeight: 400,
      rows,
      scrollHeight: 1_500,
      scrollTop: 300,
      viewportBottom: 500,
      viewportTop: 100,
    });

    expect(captureConversationViewportAnchor(element)).toEqual({ offsetTop: 20, rowKey: "turn-2" });
  });

  it("prefers elementsFromPoint for the normal top-row capture path", () => {
    // hitRow is the exact visible Turn returned behind any overlay at the viewport probe.
    // hitRow 是视口探测点覆盖层后方返回的精确可见 Turn。
    const hitRow = createRow("turn-fast", 8, 180);
    const element = createScrollElement({
      clientHeight: 300,
      hitRow,
      rows: [hitRow],
      scrollHeight: 1_000,
      scrollTop: 100,
    });

    expect(captureConversationViewportAnchor(element)).toEqual({ offsetTop: 8, rowKey: "turn-fast" });
  });

  it("captures semantic state only when the reader owns the viewport", () => {
    // row is the top visible business Turn for the reader-owned snapshot.
    // row 是阅读者拥有快照时顶部可见的业务 Turn。
    const row = createRow("turn-reader", -10, 120);
    const element = createScrollElement({
      clientHeight: 300,
      rows: [row],
      scrollHeight: 1_000,
      scrollTop: 340,
    });

    expect(captureConversationScrollSnapshot(element, false)).toEqual({
      anchorKey: "turn-reader",
      anchorTop: -10,
      followTail: false,
      scrollTop: 340,
    });
    expect(captureConversationScrollSnapshot(element, true)).toEqual({
      anchorKey: null,
      anchorTop: 0,
      followTail: true,
      scrollTop: 340,
    });
  });

  it("allows tail follow only inside the twenty-four-pixel threshold", () => {
    // nearTail leaves exactly twenty-four pixels below the viewport.
    // nearTail 在视口下方恰好保留二十四像素。
    const nearTail = createScrollElement({ clientHeight: 300, scrollHeight: 1_000, scrollTop: 676 });
    // awayFromTail leaves twenty-five pixels and remains reader-owned.
    // awayFromTail 保留二十五像素，并继续由阅读者控制。
    const awayFromTail = createScrollElement({ clientHeight: 300, scrollHeight: 1_000, scrollTop: 675 });

    expect(conversationViewportAllowsTailFollow(nearTail)).toBe(true);
    expect(conversationViewportAllowsTailFollow(awayFromTail)).toBe(false);
  });
});
