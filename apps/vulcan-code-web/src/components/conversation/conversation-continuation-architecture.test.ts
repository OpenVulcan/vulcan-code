import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

// readContinuationSource reads one source file relative to this continuation architecture contract.
// readContinuationSource 读取相对于本继续流程架构契约的一份源文件。
// Parameters: relativePath is the exact repository-relative sibling path.
// 参数：relativePath 是精确的仓库相对同级路径。
// Returns: UTF-8 source text used for continuation ownership assertions.
// 返回值：用于继续流程所有权断言的 UTF-8 源码文本。
function readContinuationSource(relativePath: string): string {
  return readFileSync(fileURLToPath(new URL(relativePath, import.meta.url)), "utf8");
}

describe("conversation continuation architecture", () => {
  it("keeps client-id creation inside the failure path that unlocks the Continue button", () => {
    // chatArea is the exact component source that owns Continue button locking and feedback.
    // chatArea 是持有继续按钮锁定与反馈逻辑的精确组件源码。
    const chatArea = readContinuationSource("./chat-area.tsx");
    // handlerStart locates the beginning of the continuation callback under review.
    // handlerStart 定位待审核继续回调的起点。
    const handlerStart = chatArea.indexOf("const handleContinueConversation = React.useCallback");
    // handlerEnd bounds the callback before the next top-level component value.
    // handlerEnd 在下一个组件顶层值之前界定回调终点。
    const handlerEnd = chatArea.indexOf("const subagentNavigation", handlerStart);
    // handlerSource isolates the exact callback so unrelated try blocks cannot satisfy assertions.
    // handlerSource 隔离精确回调，避免无关 try 块满足断言。
    const handlerSource = chatArea.slice(handlerStart, handlerEnd);
    // tryStart is the failure boundary that must precede every fallible claim setup action.
    // tryStart 是必须先于所有可能失败的领取准备动作的失败边界。
    const tryStart = handlerSource.indexOf("try {");
    // clientIdCreation locates the fallible Web Crypto request-id generation call.
    // clientIdCreation 定位可能失败的 Web Crypto 请求标识生成调用。
    const clientIdCreation = handlerSource.indexOf("globalThis.crypto.randomUUID()");
    // catchStart locates the user-visible failure classification branch.
    // catchStart 定位用户可见的失败分类分支。
    const catchStart = handlerSource.indexOf("} catch {");
    // rejectionUnlock locates the state transition that unlocks a rejected Continue attempt.
    // rejectionUnlock 定位解除失败继续尝试锁定的状态转换。
    const rejectionUnlock = handlerSource.indexOf("if (!accepted) {");

    expect(handlerStart).toBeGreaterThanOrEqual(0);
    expect(handlerEnd).toBeGreaterThan(handlerStart);
    expect(tryStart).toBeGreaterThanOrEqual(0);
    expect(clientIdCreation).toBeGreaterThan(tryStart);
    expect(catchStart).toBeGreaterThan(clientIdCreation);
    expect(rejectionUnlock).toBeGreaterThan(catchStart);
    expect(handlerSource).toContain("setContinuationClaimKey(null)");
    expect(handlerSource).toContain('kind: "failed"');
  });
});
