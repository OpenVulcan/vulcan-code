import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";

// readConversationSource reads one source file relative to this architecture contract.
// readConversationSource 读取相对当前架构契约的一份源码文件。
// Parameters: relativePath is the exact repository-relative sibling path.
// 参数：relativePath 是精确的仓库相对同级路径。
// Returns: UTF-8 source text used for ownership assertions.
// 返回值：用于所有权断言的 UTF-8 源码文本。
function readConversationSource(relativePath: string): string {
  return readFileSync(fileURLToPath(new URL(relativePath, import.meta.url)), "utf8");
}

describe("conversation scroll architecture", () => {
  it("keeps every transcript scroll mutation inside the single controller", () => {
    // controller owns the only authorized native mutation and the only correction frame.
    // controller 拥有唯一获准的原生修改与唯一校正帧。
    const controller = readConversationSource("./chat-area/conversation-scroll-controller.ts");
    // chatArea delegates shell-level behavior without writing native position directly.
    // chatArea 委派外壳级行为，且不直接写入原生位置。
    const chatArea = readConversationSource("./chat-area.tsx");
    // workspace renders business rows and delegates pagination preservation.
    // workspace 渲染业务行并委派分页位置保留。
    const workspace = readConversationSource("./conversation-workspace.tsx");

    expect(controller).toContain("element.scrollTop = nextTop");
    expect(controller.match(/requestAnimationFrame/g)).toHaveLength(1);
    expect(chatArea).not.toMatch(/\.scrollTop\s*=|\.scrollTo\(/);
    expect(workspace).not.toMatch(/\.scrollTop\s*=|\.scrollTo\(|requestAnimationFrame|ResizeObserver/);
  });

  it("derives reader ownership from native movement without device-specific scroll guesses", () => {
    // workspace must not intercept wheel, touch, or scrollbar pointer intent.
    // workspace 不得拦截滚轮、触摸或滚动条指针意图。
    const workspace = readConversationSource("./conversation-workspace.tsx");
    // controller contains the observed-position ownership classifier.
    // controller 包含观察位置所有权分类器。
    const controller = readConversationSource("./chat-area/conversation-scroll-controller.ts");

    expect(workspace).not.toMatch(/onWheel|onTouchStart|onPointerDown|ConversationTimelineUserScrollDirection/);
    expect(controller).toContain("classifyConversationObservedScrollMovement");
    expect(controller).toContain('addEventListener("scrollend"');
    expect(controller).toContain("CONVERSATION_MANUAL_VIEWPORT_INTERACTION_EVENT");
    expect(controller).toContain("detail.surfaceId !== currentOptions.surfaceId");
  });

  it("uses stable row subscriptions without per-Turn measurement caches", () => {
    // seat subscribes to one exact row snapshot and contains no measurement observer.
    // seat 订阅一份精确行快照，且不包含测量观察器。
    const seat = readConversationSource("./conversation-turn-seat.tsx");
    // workspace commits row projections after render instead of mutating the Store during render.
    // workspace 在渲染后提交行投影，而不是在渲染期间修改 Store。
    const workspace = readConversationSource("./conversation-workspace.tsx");
    // layout memory retains semantic scroll and expansion state only.
    // layout memory 仅保留语义滚动与展开状态。
    const layoutMemory = readConversationSource("./conversation-layout-restore.ts");
    // anchor helper prefers native hit testing for long mounted transcripts.
    // 锚点辅助函数针对长已挂载转录优先使用原生命中测试。
    const scrollHelpers = readConversationSource("./chat-area-scroll.ts");

    expect(seat).toContain("useSyncExternalStore");
    expect(seat).toContain("ConversationTurnSeatList");
    expect(seat).toContain("previous.rowKeys === next.rowKeys");
    expect(seat).not.toMatch(/ResizeObserver|minHeight|estimatedHeight|initialHeight/);
    expect(layoutMemory).not.toContain("turnHeightsByKey");
    expect(scrollHelpers).toContain("elementsFromPoint");
    expect(workspace).not.toContain("const turnOrder = timelineProjectionStore.reconcile");
    expect(workspace).toContain("React.useLayoutEffect(() => {\n    timelineProjectionStore.reconcile");
  });

  it("retains independent semantic and expansion memory for every subagent tab", () => {
    // contextPanel owns child-tab memory above the active content component that unmounts during tab switches.
    // contextPanel 在活动内容组件之上拥有子页签记忆，活动内容会在页签切换时卸载。
    const contextPanel = readConversationSource("../workspace/WorkspaceContextPanel.tsx");

    expect(contextPanel).toContain("subagentScrollSnapshots");
    expect(contextPanel).toContain("subagentExpandedStates");
    expect(contextPanel).toContain("args.expandedStates.current.get(taskId) ?? null");
    expect(contextPanel).not.toContain("handleNoopExpandedStateChange");
  });
});
