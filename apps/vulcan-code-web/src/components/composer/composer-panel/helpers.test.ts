import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ComposerAttachmentSummary } from "@/types";

const DRAFT_STORAGE_PREFIX = "vulcan-code.composer-draft.v1:";

// createMemoryStorage provides the exact synchronous Storage surface used by the draft helpers.
// createMemoryStorage 提供草稿辅助函数使用的精确同步 Storage 表面。
// Returns: one isolated in-memory browser storage implementation.
// 返回值：一个隔离的内存浏览器存储实现。
function createMemoryStorage(): Storage {
  const values = new Map<string, string>();
  return {
    get length() {
      return values.size;
    },
    clear() {
      values.clear();
    },
    getItem(key: string) {
      return values.get(key) ?? null;
    },
    key(index: number) {
      return Array.from(values.keys())[index] ?? null;
    },
    removeItem(key: string) {
      values.delete(key);
    },
    setItem(key: string, value: string) {
      values.set(key, value);
    },
  };
}

describe("composer draft persistence", () => {
  let storage: Storage;

  beforeEach(() => {
    storage = createMemoryStorage();
    vi.stubGlobal("window", { localStorage: storage });
    vi.resetModules();
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("restores canonical attachment facts after a module restart without transient delivery state", async () => {
    const firstRuntime = await import("./helpers");
    const attachment: ComposerAttachmentSummary = {
      kind: "image",
      fileName: "screen.png",
      localPath: "D:/managed/screen.png",
      resourcePath: ".vulcan/attachments/asset-a/screen.png",
      sourcePath: "D:/source/screen.png",
      assetId: "asset-a",
      byteLen: 512,
      sourceSha256: "a".repeat(64),
      deliveryPlan: "upload_reference",
      previewLocalPath: "D:/managed/preview.jpg",
      mimeType: "image/png",
    };
    firstRuntime.writeComposerSessionMemoryDraft("conversation:session-a", "inspect", [attachment]);

    const persisted = storage.getItem(`${DRAFT_STORAGE_PREFIX}conversation:session-a`);
    expect(persisted).not.toBeNull();
    expect(persisted).not.toContain("deliveryPlan");

    vi.resetModules();
    const restartedRuntime = await import("./helpers");
    expect(restartedRuntime.readComposerSessionMemoryDraft("conversation:session-a")).toEqual({
      input: "inspect",
      attachments: [{
        kind: "image",
        fileName: "screen.png",
        localPath: "D:/managed/screen.png",
        resourcePath: ".vulcan/attachments/asset-a/screen.png",
        sourcePath: "D:/source/screen.png",
        assetId: "asset-a",
        byteLen: 512,
        sourceSha256: "a".repeat(64),
        previewLocalPath: "D:/managed/preview.jpg",
        mimeType: "image/png",
      }],
    });
  });

  it("rejects an invalid versioned payload without guessing fields or deleting its bytes", async () => {
    const rejected = JSON.stringify({ version: 1, input: "inspect", attachments: [{ assetId: "asset-a" }] });
    storage.setItem(`${DRAFT_STORAGE_PREFIX}new:project-a`, rejected);
    const runtime = await import("./helpers");

    expect(runtime.readComposerSessionMemoryDraft("new:project-a")).toEqual({
      input: "",
      attachments: [],
    });
    expect(storage.getItem(`${DRAFT_STORAGE_PREFIX}new:project-a`)).toBe(rejected);
  });
});
