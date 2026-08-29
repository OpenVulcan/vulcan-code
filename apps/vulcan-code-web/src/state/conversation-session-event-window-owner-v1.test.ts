import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

/** Reads the event-window owner hook source for lifecycle architecture assertions.
 * 读取事件窗口 owner hook 源码用于生命周期架构断言。
 *
 * Parameters: none.
 * 参数：无。
 * Returns: normalized source text for the hook module.
 * 返回值：hook 模块的规范化源码文本。
 */
function ownerHookSource(): string {
  return readFileSync(
    fileURLToPath(new URL("./conversation-session-event-window-owner-v1.ts", import.meta.url)),
    "utf8",
  ).replace(/\r\n/g, "\n");
}

describe("conversation session event-window owner lifecycle", () => {
  it("disposes controllers only when their exact controller instance leaves ownership", () => {
    const source = ownerHookSource();
    const disposalMatches = source.match(/controller\.dispose\(\)/g) ?? [];

    expect(disposalMatches).toHaveLength(1);
    expect(source).toContain("return () => controller.dispose();\n  }, [controller]);");
    expect(source).not.toContain("active = false;\n      controller.dispose();");
  });

  it("loads closed child transcripts as one-shot snapshots without preparation or live paging", () => {
    const source = ownerHookSource();

    expect(source).not.toContain("options.prepare");
    expect(source).toContain("eventWindowClient.hydrateEmbeddedTaskSnapshot(");
    expect(source).toContain("return controller.hydrateSnapshot(response);");
    expect(source).toContain("return options.live ? controller.rehydrate(options.sessionId) : false;");
    expect(source).toContain("prependOlder: () => options.live ? controller.prependOlder() : Promise.resolve(false)");
    expect(source).toContain("if (!options.live || !wasDisconnectedRef.current || !hydratedRef.current) return;");
  });
});
