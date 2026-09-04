import { describe, expect, it } from "vitest";
import {
  emptyVceManagementPage,
  materializeVceManagementPage,
  nextVceManagementPageRequest,
  previousVceManagementPageRequest,
} from "./code-context-pagination";

describe("VCE management pagination", () => {
  // Empty state is an explicit first page and cannot move in either direction.
  // 空状态是明确的第一页，不能向任一方向移动。
  it("starts without speculative cursors", () => {
    const page = emptyVceManagementPage<string>();
    expect(page).toEqual({
      items: [],
      cursor: null,
      nextCursor: null,
      total: null,
      previousCursors: [],
    });
    expect(nextVceManagementPageRequest(page)).toBeNull();
    expect(previousVceManagementPageRequest(page)).toBeNull();
  });

  // Forward and backward navigation preserves opaque cursor identity exactly.
  // 前进与后退导航精确保留不透明游标身份。
  it("round-trips opaque cursors without decoding them", () => {
    const first = materializeVceManagementPage(
      { items: ["first"], nextCursor: "opaque:next", total: 2 },
      { cursor: null, previousCursors: [] },
    );
    const forward = nextVceManagementPageRequest(first);
    expect(forward).toEqual({ cursor: "opaque:next", previousCursors: [null] });

    const second = materializeVceManagementPage(
      { items: ["second"], nextCursor: null, total: 2 },
      forward!,
    );
    expect(nextVceManagementPageRequest(second)).toBeNull();
    expect(previousVceManagementPageRequest(second)).toEqual({
      cursor: null,
      previousCursors: [],
    });
  });
});
