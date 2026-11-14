import React from "react";
import {
  captureConversationScrollSnapshot,
  conversationScrollTopForSnapshot,
  conversationViewportAllowsTailFollow,
  type ConversationScrollSnapshot,
} from "@/components/conversation/chat-area-scroll";
import {
  CONVERSATION_AUTOMATIC_FOLD_EVENT,
  CONVERSATION_LAYOUT_SETTLED_EVENT,
  CONVERSATION_LAYOUT_WILL_CHANGE_EVENT,
  CONVERSATION_MANUAL_VIEWPORT_INTERACTION_EVENT,
  CONVERSATION_SCROLLBAR_POSITION_EVENT,
  CONVERSATION_TURN_WINDOW_LAYOUT_EVENT,
  type ConversationLayoutSettledDetail,
  type ConversationManualViewportInteractionDetail,
  type ConversationScrollbarPosition,
  type ConversationTurnWindowLayoutDetail,
} from "@/components/conversation/conversation-layout-events";
import { motionTokenMs } from "@/lib/motion-tokens";

// UseConversationScrollControllerOptions defines the single owner of one mounted transcript scrollport.
// UseConversationScrollControllerOptions 定义单个已挂载转录滚动视口的唯一所有者。
export interface UseConversationScrollControllerOptions {
  // Whether the transcript contains a materialized scroll surface.
  // 转录是否包含已实体化的滚动界面。
  active: boolean;
  // Whether automatic reflow should keep the followed tail's height while the session is running.
  // 会话运行期间，自动重排是否保留已锁底的尾部高度。
  holdRunningTailHeight?: boolean;
  // Reports tail ownership changes to the scroll-to-bottom affordance.
  // 向滚动到底部操作入口报告尾部所有权变化。
  onFollowTailChange: (followTail: boolean) => void;
  // Reads one exact surface snapshot without creating fallback state.
  // 读取一个精确界面快照且不创建兜底状态。
  readSnapshot: (surfaceId: string) => ConversationScrollSnapshot | null;
  // Shared transcript scroll element.
  // 共享转录滚动元素。
  scrollRef: React.RefObject<HTMLElement | null>;
  // Stable active conversation or child-task surface identity.
  // 稳定的活动会话或子任务界面身份。
  surfaceId: string | null;
  // Stores one exact surface snapshot in runtime memory.
  // 在运行期记忆中保存一个精确界面快照。
  writeSnapshot: (surfaceId: string, snapshot: ConversationScrollSnapshot) => void;
}

// ConversationScrollController exposes every authorized transcript scroll mutation.
// ConversationScrollController 暴露全部获准的转录滚动修改入口。
export interface ConversationScrollController {
  // Captures the current semantic viewport and schedules one post-layout correction.
  // 捕获当前语义视口并调度一次布局后校正。
  captureAndScheduleLayoutRestore: () => void;
  // Handles native scroll events through observed movement rather than device-event guesses.
  // 通过已观察位移而不是设备事件猜测处理原生滚动事件。
  handleScroll: () => void;
  // Preserves the current anchor across asynchronous history prepend unless the reader moves.
  // 在异步历史前置期间保留当前锚点，除非阅读者发生移动。
  preserveHistoryPrepend: (load: () => Promise<boolean>) => Promise<boolean>;
  // Stores the current semantic viewport immediately.
  // 立即保存当前语义视口。
  remember: () => void;
  // Restores the latest stored semantic viewport in one cancellable frame.
  // 在一个可取消帧中恢复最新保存的语义视口。
  scheduleStoredRestore: () => void;
  // Transfers ownership to tail following and moves to the latest content.
  // 将所有权切换为尾部跟随并移动到最新内容。
  scrollToBottom: (behavior?: ScrollBehavior) => void;
  // Releases tail following and moves one mounted transcript anchor to the top of the viewport; a text range,
  // such as a find match, goes a third of the way down instead so the text around it stays in sight.
  // 释放尾部跟随，并将一个已挂载转录锚点移动到视口顶部；文本范围（例如查找匹配）则移动到视口三分之一处，
  // 使其周围的文本仍然可见。
  revealAnchor: (anchor: HTMLElement | Range, behavior?: ScrollBehavior) => void;
}

// CONVERSATION_ANCHOR_REVEAL_TOP_GAP_PX keeps a revealed anchor slightly below the scrollport top edge.
// CONVERSATION_ANCHOR_REVEAL_TOP_GAP_PX 让被定位锚点略低于滚动视口顶部边缘。
export const CONVERSATION_ANCHOR_REVEAL_TOP_GAP_PX = 16;

// CONVERSATION_TAIL_RESERVE_MAX_SHARE caps the blank tail reserve at two fifths of the visible transcript
// area, so at least three fifths keep showing content after a large collapse.
// CONVERSATION_TAIL_RESERVE_MAX_SHARE 将空白尾部预留限制为可见转录区域的五分之二，使大幅折叠后至少五分之三
// 仍显示内容。
export const CONVERSATION_TAIL_RESERVE_MAX_SHARE = 0.4;

// conversationTailReserveHeight keeps a manual fold from pulling the viewport bottom up: only the part
// of the shrinkage that would leave blank space under the viewport is filled, up to the cap.
// conversationTailReserveHeight 防止手动折叠把视口底部向上拉：只填补收缩后会在视口下方留空的部分，
// 且不超过上限。
// Parameters: foldLimit is the transcript height the viewport bottom reached when the fold began
// (content plus reserve, minus the distance still scrollable below), contentHeight is the current
// transcript height, and cap is the largest allowed reserve.
// 参数：foldLimit 是折叠开始时视口底部所在的转录高度（内容加预留，减去下方仍可滚动的距离），
// contentHeight 是当前转录高度，cap 是允许的最大预留。
// Returns: the reserve height in CSS pixels.
// 返回值：以 CSS 像素表示的预留高度。
export function conversationTailReserveHeight(foldLimit: number, contentHeight: number, cap: number): number {
  return Math.max(0, Math.min(cap, Math.ceil(foldLimit - contentHeight)));
}

// ProgrammaticScrollTarget describes one in-flight native scroll operation owned by the controller.
// ProgrammaticScrollTarget 描述一个由控制器拥有的进行中原生滚动操作。
interface ProgrammaticScrollTarget {
  // Requested clamped scrollTop destination.
  // 请求的已约束 scrollTop 目标。
  top: number;
}

// ConversationLayoutTransaction preserves one immutable reader anchor through animated geometry changes.
// ConversationLayoutTransaction 在动画几何变化期间保留单个不可变阅读锚点。
interface ConversationLayoutTransaction {
  // Exact transcript surface that owns the transaction.
  // 持有当前事务的精确转录界面。
  surfaceId: string;
  // Pre-mutation semantic viewport snapshot.
  // 修改前的语义视口快照。
  snapshot: ConversationScrollSnapshot;
  // Reader revision that invalidates the transaction after manual movement.
  // 阅读者手动移动后使事务失效的修订号。
  userRevision: number;
}

// ConversationObservedScrollMovement is the ownership decision derived from native position changes.
// ConversationObservedScrollMovement 是从原生位置变化推导出的所有权决策。
export interface ConversationObservedScrollMovement {
  // Whether the reader moved independently from the registered programmatic target.
  // 阅读者是否独立于已登记程序化目标发生了移动。
  readerMoved: boolean;
  // Whether the registered target should remain active for another native smooth-scroll event.
  // 已登记目标是否应在下一次原生平滑滚动事件中继续有效。
  retainTarget: boolean;
}

// ConversationResizeCorrectionMode selects the only eligible response to aggregate content resize.
// ConversationResizeCorrectionMode 选择聚合内容尺寸变化唯一允许的响应。
export type ConversationResizeCorrectionMode = "transaction" | "tail" | "none";

// ConversationResizeOwnershipDecision describes whether resized geometry restores tail ownership.
// ConversationResizeOwnershipDecision 描述尺寸变化后的几何状态是否恢复尾部所有权。
export interface ConversationResizeOwnershipDecision {
  // Whether the current resize newly adopts tail ownership.
  // 当前尺寸变化是否新接管尾部所有权。
  adoptedTail: boolean;
  // Scroll correction allowed after the ownership decision.
  // 所有权决策后允许执行的滚动校正。
  correctionMode: ConversationResizeCorrectionMode;
}

// conversationResizeCorrectionMode prevents ordinary reader-owned reflow from restoring every frame.
// conversationResizeCorrectionMode 防止普通阅读者持有的重排在每帧执行恢复。
// Parameters: followTail reports tail ownership and transactionActive reports an exact manual layout transaction.
// 参数：followTail 表示尾部所有权，transactionActive 表示精确手动布局事务。
// Returns: transaction correction first, tail correction second, or no scroll write.
// 返回值：优先返回事务校正，其次返回尾部校正，否则不执行滚动写入。
export function conversationResizeCorrectionMode(
  followTail: boolean,
  transactionActive: boolean,
): ConversationResizeCorrectionMode {
  if (transactionActive) {
    return "transaction";
  }
  return followTail ? "tail" : "none";
}

// conversationResizeOwnershipDecision makes physical bottom geometry authoritative after content resize.
// conversationResizeOwnershipDecision 在内容尺寸变化后让实际底部几何状态具有权威性。
// Parameters: followTail is current ownership, transactionActive marks a fold anchor, and viewportAtBottom is native geometry.
// 参数：followTail 是当前所有权，transactionActive 标记折叠锚点，viewportAtBottom 是原生几何状态。
// Returns: whether tail ownership is newly adopted and which correction mode remains valid.
// 返回值：是否新接管尾部所有权，以及仍有效的校正模式。
export function conversationResizeOwnershipDecision(
  followTail: boolean,
  transactionActive: boolean,
  viewportAtBottom: boolean,
): ConversationResizeOwnershipDecision {
  if (!followTail && viewportAtBottom) {
    return {
      adoptedTail: true,
      correctionMode: "tail",
    };
  }
  return {
    adoptedTail: false,
    correctionMode: conversationResizeCorrectionMode(followTail, transactionActive),
  };
}

// reconcileConversationNativeScrollClamp acknowledges a position clipped by a shrinking native scroll range.
// reconcileConversationNativeScrollClamp 确认因原生滚动范围缩小而被截断的位置。
// Parameters: previousTop is the ledger position; actualTop and maxScrollTop describe the current native geometry.
// 参数：previousTop 是账本位置；actualTop 与 maxScrollTop 描述当前原生几何。
// Returns: the clipped observation only at the reduced boundary; otherwise preserves the reader-movement baseline.
// 返回值：仅在缩小后的边界返回截断观察值；否则保留用于识别阅读者移动的基准。
export function reconcileConversationNativeScrollClamp(
  previousTop: number,
  actualTop: number,
  maxScrollTop: number,
): number {
  return previousTop > maxScrollTop && Math.abs(actualTop - maxScrollTop) <= 1
    ? actualTop
    : previousTop;
}

// classifyConversationObservedScrollMovement compares native motion with the controller's explicit target ledger.
// classifyConversationObservedScrollMovement 将原生位移与控制器显式目标账本进行比较。
// Parameters: previousTop and actualTop are consecutive observations; targetTop is the owned destination or null.
// 参数：previousTop 与 actualTop 是连续观察值；targetTop 是已拥有目标，或为 null。
// Returns: whether reader ownership changed and whether a smooth target remains active.
// 返回值：阅读者所有权是否变化，以及平滑目标是否继续有效。
export function classifyConversationObservedScrollMovement(
  previousTop: number,
  actualTop: number,
  targetTop: number | null,
): ConversationObservedScrollMovement {
  if (targetTop === null) {
    return {
      readerMoved: Math.abs(actualTop - previousTop) > 0.5,
      retainTarget: false,
    };
  }
  // reachedTarget closes the programmatic operation at its native destination.
  // reachedTarget 在原生目标处结束程序化操作。
  const reachedTarget = Math.abs(actualTop - targetTop) <= 1;
  if (reachedTarget) {
    return { readerMoved: false, retainTarget: false };
  }
  // movedTowardTarget preserves ownership across intermediate native smooth-scroll events.
  // movedTowardTarget 在原生平滑滚动的中间事件中保持所有权。
  const movedTowardTarget = Math.abs(targetTop - actualTop) < Math.abs(targetTop - previousTop);
  return {
    readerMoved: !movedTowardTarget && Math.abs(actualTop - previousTop) > 0.5,
    retainTarget: movedTowardTarget,
  };
}

// useConversationScrollController owns surface memory, layout anchoring, tail following, and every scrollTop write.
// useConversationScrollController 统一拥有界面记忆、布局锚定、尾部跟随与全部 scrollTop 写入。
// Parameters: options declares the active surface, DOM owner, memory operations, and tail-state observer.
// 参数：options 声明活动界面、DOM 所有者、记忆操作与尾部状态观察器。
// Returns: stable handlers for scroll events, layout changes, history prepend, memory, and tail navigation.
// 返回值：滚动事件、布局变化、历史前置、记忆与尾部导航的稳定处理器。
export function useConversationScrollController(
  options: UseConversationScrollControllerOptions,
): ConversationScrollController {
  // followTailRef is the controller's explicit viewport ownership state.
  // followTailRef 是控制器显式的视口所有权状态。
  const followTailRef = React.useRef(true);
  // observedScrollTopRef is the last native position processed by the controller ledger.
  // observedScrollTopRef 是控制器账本处理的最后一个原生位置。
  const observedScrollTopRef = React.useRef(0);
  // programmaticTargetRef identifies movements initiated through the sole mutation path.
  // programmaticTargetRef 标识通过唯一修改路径发起的位移。
  const programmaticTargetRef = React.useRef<ProgrammaticScrollTarget | null>(null);
  // An unloaded jump keeps its exact target until the observer materializes that turn.
  // 跳向未挂载回合时，保留精确目标直至观察器挂载正文。
  const pendingTurnRevealRef = React.useRef<{ shell: HTMLElement; revision: number } | null>(null);
  // The pointer owns a normalized position while the scrollbar is being dragged.
  // 拖动滚动条时，由指针控制归一化进度。
  const scrollbarProgressRef = React.useRef<number | null>(null);
  // pendingFrameRef owns the sole cancellable animation frame for this scrollport.
  // pendingFrameRef 拥有当前滚动视口唯一可取消的动画帧。
  const pendingFrameRef = React.useRef<number | null>(null);
  // layoutTransactionRef exists only while one manual fold animation owns viewport correction.
  // layoutTransactionRef 仅在单次手动折叠动画持有视口校正时存在。
  const layoutTransactionRef = React.useRef<ConversationLayoutTransaction | null>(null);
  // surfaceRestoreRef holds a restored reading position until the reader acts, so content that finishes
  // rendering after a surface switch is re-anchored instead of a temporary bottom clamp adopting the tail.
  // surfaceRestoreRef 在阅读者操作前保持已恢复的阅读位置，使界面切换后才完成渲染的内容重新锚定，
  // 而不是由暂时裁剪到的底部接管尾部跟随。
  const surfaceRestoreRef = React.useRef<ConversationLayoutTransaction | null>(null);
  // userRevisionRef invalidates asynchronous restores when observed reader movement occurs.
  // userRevisionRef 在观察到阅读者移动时使异步恢复失效。
  const userRevisionRef = React.useRef(0);
  // renderedSurfaceIdRef remembers which business surface currently owns the shared DOM node.
  // renderedSurfaceIdRef 记住当前由哪个业务界面拥有共享 DOM 节点。
  const renderedSurfaceIdRef = React.useRef<string | null>(null);
  // latestOptionsRef exposes current callbacks to stable native and observer handlers.
  // latestOptionsRef 向稳定原生与观察器处理器暴露当前回调。
  const latestOptionsRef = React.useRef(options);
  latestOptionsRef.current = options;
  // tailTotalRef is the transcript-plus-reserve height after the last reconciliation.
  // tailTotalRef 是上次协调后的“转录加预留”高度。
  const tailTotalRef = React.useRef(0);
  // foldLimitRef is the transcript height under the viewport bottom when a manual fold began; null
  // outside a fold, when the reserve may only shrink.
  // foldLimitRef 是手动折叠开始时视口底部所在的转录高度；折叠之外为 null，此时预留只能缩小。
  const foldLimitRef = React.useRef<number | null>(null);

  // reconcileTailReserve keeps a manual fold from pulling the content above down to the bottom.
  // reconcileTailReserve 防止手动折叠把上方内容一下子拉到底部。
  // Automatic reflow can shrink a running transcript without announcing a fold. Preserve the full
  // followed height so even a tall tool can be replaced without a backward jump.
  // 会话运行时，自动重排也可能在没有折叠通知的情况下缩短转录。保留完整锁底高度，使高工具卡被替换时也不会回跳。
  // Parameters: element is the scrollport of the active surface.
  // 参数：element 是活动界面的滚动视口。
  // Returns: nothing.
  // 返回值：无。
  const reconcileTailReserve = React.useCallback((element: HTMLElement): void => {
    const reserve = element.querySelector<HTMLElement>(".vk-transcript-tail");
    const timeline = element.querySelector<HTMLElement>(".vk-transcript-inner > .vk-timeline");
    if (!reserve || !timeline) {
      return;
    }
    const current = reserve.offsetHeight;
    const contentHeight = timeline.offsetHeight;
    let next: number;
    if (followTailRef.current && latestOptionsRef.current.holdRunningTailHeight && !layoutTransactionRef.current) {
      next = Math.max(0, tailTotalRef.current - contentHeight);
    } else if (foldLimitRef.current !== null) {
      // visibleArea excludes the sticky footer that covers the bottom of the scrollport.
      // visibleArea 不含覆盖滚动视口底部的粘性页脚。
      const footer = element.querySelector<HTMLElement>("[data-conversation-scroll-footer]");
      const visibleArea = Math.max(0, element.clientHeight - (footer?.offsetHeight ?? 0));
      next = conversationTailReserveHeight(
        foldLimitRef.current,
        contentHeight,
        Math.floor(visibleArea * CONVERSATION_TAIL_RESERVE_MAX_SHARE),
      );
    } else if (followTailRef.current) {
      // New content consumes the reserve by exactly its own growth, so the followed page keeps its height
      // when a folded tool batch is replaced by a shorter one, and the content above never jumps.
      // 新内容按其自身增长量消耗预留，使折叠的工具批次被更短的批次替换时页面保持高度，上方内容不跳动。
      next = Math.max(0, Math.min(current, tailTotalRef.current - contentHeight));
    } else {
      const onScreen = element.getBoundingClientRect().bottom - reserve.getBoundingClientRect().top;
      next = Math.max(0, Math.min(current, Math.ceil(onScreen)));
    }
    if (next !== current) {
      reserve.style.height = `${next}px`;
    }
    tailTotalRef.current = contentHeight + next;
  }, []);

  // beginFoldReserve records where the viewport bottom sits before a manual fold changes the transcript.
  // beginFoldReserve 在手动折叠改变转录前记录视口底部所在位置。
  // Parameters: element is the scrollport of the active surface.
  // 参数：element 是活动界面的滚动视口。
  // Returns: nothing.
  // 返回值：无。
  const beginFoldReserve = React.useCallback((element: HTMLElement): void => {
    const reserve = element.querySelector<HTMLElement>(".vk-transcript-tail");
    const timeline = element.querySelector<HTMLElement>(".vk-transcript-inner > .vk-timeline");
    if (!reserve || !timeline) {
      return;
    }
    const below = Math.max(0, element.scrollHeight - element.scrollTop - element.clientHeight);
    foldLimitRef.current = timeline.offsetHeight + reserve.offsetHeight - below;
  }, []);

  // publishFollowTail updates the ownership ledger and visible affordance together.
  // publishFollowTail 同时更新所有权账本与可见操作入口。
  // Parameters: followTail is the exact next ownership state.
  // 参数：followTail 是精确的下一所有权状态。
  // Returns: nothing.
  // 返回值：无。
  const publishFollowTail = React.useCallback((followTail: boolean): void => {
    if (followTailRef.current === followTail) {
      return;
    }
    followTailRef.current = followTail;
    latestOptionsRef.current.onFollowTailChange(followTail);
  }, []);

  // writeCurrentSnapshot stores the ledger's current semantic position for the rendered surface.
  // writeCurrentSnapshot 为当前已渲染界面保存账本中的语义位置。
  // Parameters: element is the current scrollport and surfaceId is its exact business owner.
  // 参数：element 是当前滚动视口；surfaceId 是其精确业务所有者。
  // Returns: the immutable snapshot that was stored.
  // 返回值：已保存的不可变快照。
  const writeCurrentSnapshot = React.useCallback((
    element: HTMLElement,
    surfaceId: string,
  ): ConversationScrollSnapshot => {
    // An archived anchor can arrive after the retained event window. Keep its original offset until its body mounts.
    // 历史锚点可能晚于保留的事件窗口到达。正文挂载前保留其原始偏移。
    const restore = surfaceRestoreRef.current;
    if (!followTailRef.current && restore?.surfaceId === surfaceId
      && restore.userRevision === userRevisionRef.current && restore.snapshot.anchorKey) {
      // A real body, rather than a provisional shell, can represent the saved offset.
      // 只有真实正文能表示保存的偏移，占位外壳不能。
      const mountedRows = element.querySelectorAll<HTMLElement>("[data-conversation-row-key]:not([data-conversation-turn-placeholder])");
      if (!Array.from(mountedRows).some((row) => row.dataset.conversationRowKey === restore.snapshot.anchorKey)) {
        return restore.snapshot;
      }
    }
    // snapshot captures the exact DOM position under the current ownership ledger.
    // snapshot 在当前所有权账本下捕获精确 DOM 位置。
    const snapshot = captureConversationScrollSnapshot(element, followTailRef.current);
    latestOptionsRef.current.writeSnapshot(surfaceId, snapshot);
    return snapshot;
  }, []);

  // cancelPendingFrame releases the sole scheduled correction before replacement or unmount.
  // cancelPendingFrame 在替换或卸载前释放唯一已调度校正。
  // Parameters: none.
  // 参数：无。
  // Returns: nothing.
  // 返回值：无。
  const cancelPendingFrame = React.useCallback((): void => {
    if (pendingFrameRef.current === null) {
      return;
    }
    window.cancelAnimationFrame(pendingFrameRef.current);
    pendingFrameRef.current = null;
  }, []);

  // writeScrollTop is the only controller path authorized to mutate native transcript position.
  // writeScrollTop 是控制器内唯一获准修改原生转录位置的路径。
  // Parameters: element is the scrollport, target is an unclamped position, and behavior selects native animation.
  // 参数：element 是滚动视口；target 是未约束位置；behavior 选择原生动画。
  // Returns: the clamped target registered in the programmatic ledger.
  // 返回值：已登记到程序化账本的约束后目标。
  const writeScrollTop = React.useCallback((
    element: HTMLElement,
    target: number,
    behavior: ScrollBehavior = "auto",
  ): number => {
    // maxScrollTop is the native valid upper bound for the current geometry.
    // maxScrollTop 是当前几何下原生有效上界。
    const maxScrollTop = Math.max(0, element.scrollHeight - element.clientHeight);
    // nextTop is finite and clamped before it enters the ownership ledger.
    // nextTop 在进入所有权账本前已保证有限并完成约束。
    const nextTop = Number.isFinite(target) ? Math.max(0, Math.min(maxScrollTop, target)) : 0;
    programmaticTargetRef.current = { top: nextTop };
    if (behavior === "smooth") {
      element.scrollTo({ behavior, top: nextTop });
    } else {
      element.scrollTop = nextTop;
      observedScrollTopRef.current = element.scrollTop;
      if (Math.abs(element.scrollTop - nextTop) <= 1) {
        programmaticTargetRef.current = null;
      }
    }
    return nextTop;
  }, []);

  // Apply the held track position to the latest scrollable range.
  // 将指针保持的轨道进度应用到最新滚动范围。
  const applyScrollbarPosition = React.useCallback((element: HTMLElement, progress: number): void => {
    const max = Math.max(0, element.scrollHeight - element.clientHeight);
    writeScrollTop(element, progress * max);
  }, [writeScrollTop]);

  // restoreSnapshot applies one semantic snapshot through the sole native mutation path.
  // restoreSnapshot 通过唯一原生修改路径应用一份语义快照。
  // Parameters: element is the current scrollport and snapshot is the saved state for its exact surface.
  // 参数：element 是当前滚动视口；snapshot 是其精确界面的已保存状态。
  // Returns: nothing.
  // 返回值：无。
  const restoreSnapshot = React.useCallback((
    element: HTMLElement,
    snapshot: ConversationScrollSnapshot,
  ): void => {
    publishFollowTail(snapshot.followTail);
    writeScrollTop(element, conversationScrollTopForSnapshot(element, snapshot));
  }, [publishFollowTail, writeScrollTop]);

  // scheduleSnapshotRestore aligns a followed tail before paint and defers reader restoration.
  // scheduleSnapshotRestore 在绘制前对齐锁底位置，并延后恢复阅读位置。
  // Parameters: surfaceId and snapshot bind the correction to one exact business surface; revision optionally guards reader movement.
  // 参数：surfaceId 与 snapshot 将校正绑定到一个精确业务界面；revision 可选地防止覆盖阅读者移动。
  // Returns: nothing.
  // 返回值：无。
  const scheduleSnapshotRestore = React.useCallback((
    surfaceId: string,
    snapshot: ConversationScrollSnapshot,
    revision: number | null = null,
  ): void => {
    if (revision !== null && userRevisionRef.current !== revision) {
      return;
    }
    // currentOptions and element bind the synchronous geometry observation to the scheduled surface.
    // currentOptions 与 element 将同步几何观察绑定到已调度界面。
    const currentOptions = latestOptionsRef.current;
    const element = currentOptions.scrollRef.current;
    if (currentOptions.active && currentOptions.surfaceId === surfaceId
      && element?.dataset.conversationSurfaceId === surfaceId) {
      if (snapshot.followTail && followTailRef.current && programmaticTargetRef.current === null) {
        // ResizeObserver runs after this frame's animation callbacks. Another frame would let new content
        // paint under the composer even though the reader is still following the tail.
        // ResizeObserver 在本帧动画回调之后执行。继续等一帧会让新内容在锁底时仍被输入框盖住。
        cancelPendingFrame();
        reconcileTailReserve(element);
        if (element.scrollHeight - element.clientHeight - element.scrollTop > 0.5
          || Math.abs(element.scrollTop - snapshot.scrollTop) > 0.5) {
          restoreSnapshot(element, snapshot);
          writeCurrentSnapshot(element, surfaceId);
        }
        return;
      }
      // Composer shrink can clip scrollTop before the prompt grows the range again, ahead of the native scroll event.
      // 输入区收缩可能先截断 scrollTop，随后提示词再次增大范围，而原生滚动事件尚未到达。
      // Record that exact clipping now so the delayed event cannot cancel tail ownership as a supposed reader move.
      // 立即记录这次精确截断，防止延迟事件将其当成阅读者移动并取消尾部所有权。
      observedScrollTopRef.current = reconcileConversationNativeScrollClamp(
        observedScrollTopRef.current,
        element.scrollTop,
        Math.max(0, element.scrollHeight - element.clientHeight),
      );
    }
    cancelPendingFrame();
    pendingFrameRef.current = window.requestAnimationFrame(() => {
      pendingFrameRef.current = null;
      if (revision !== null && userRevisionRef.current !== revision) {
        return;
      }
      // currentOptions verifies the scheduled operation still belongs to the active surface.
      // currentOptions 验证已调度操作仍属于活动界面。
      const currentOptions = latestOptionsRef.current;
      // element is accepted only when its rendered dataset still matches the scheduled business surface.
      // element 仅在其已渲染数据集仍匹配已调度业务界面时被接受。
      const element = currentOptions.scrollRef.current;
      if (!currentOptions.active || currentOptions.surfaceId !== surfaceId || !element) {
        return;
      }
      if (element.dataset.conversationSurfaceId !== surfaceId) {
        return;
      }
      restoreSnapshot(element, snapshot);
      writeCurrentSnapshot(element, surfaceId);
    });
  }, [cancelPendingFrame, reconcileTailReserve, restoreSnapshot, writeCurrentSnapshot]);

  // remember captures the current mounted surface without changing ownership or position.
  // remember 捕获当前已挂载界面，且不改变所有权或位置。
  // Parameters: none.
  // 参数：无。
  // Returns: nothing.
  // 返回值：无。
  const remember = React.useCallback((): void => {
    // currentOptions supplies the exact active surface and shared scroll owner.
    // currentOptions 提供精确活动界面与共享滚动所有者。
    const currentOptions = latestOptionsRef.current;
    // element is ignored while empty/loading surfaces do not own a materialized transcript.
    // element 在空白或加载界面不拥有实体化转录时被忽略。
    const element = currentOptions.scrollRef.current;
    if (!currentOptions.active || !currentOptions.surfaceId || !element) {
      return;
    }
    if (element.dataset.conversationSurfaceId !== currentOptions.surfaceId) {
      return;
    }
    writeCurrentSnapshot(element, currentOptions.surfaceId);
  }, [writeCurrentSnapshot]);

  // handleScroll classifies observed movement against the programmatic target ledger.
  // handleScroll 根据程序化目标账本对观察到的位移进行分类。
  // Parameters: none; native geometry is read from the shared scroll ref.
  // 参数：无；原生几何通过共享滚动引用读取。
  // Returns: nothing.
  // 返回值：无。
  const handleScroll = React.useCallback((): void => {
    // currentOptions resolves the surface that owns this native event.
    // currentOptions 解析拥有当前原生事件的界面。
    const currentOptions = latestOptionsRef.current;
    // element is the mounted shared scroll owner at event time.
    // element 是事件发生时已挂载的共享滚动所有者。
    const element = currentOptions.scrollRef.current;
    const surfaceId = element?.dataset.conversationSurfaceId ?? null;
    if (!element || !surfaceId || surfaceId !== currentOptions.surfaceId) {
      return;
    }
    // actualTop is the native observed position that decides ownership.
    // actualTop 是决定所有权的原生观察位置。
    const actualTop = element.scrollTop;
    // While the pointer owns the track, scroll events only acknowledge the latest geometry.
    // 指针控制轨道期间，滚动事件只确认最新几何位置。
    if (scrollbarProgressRef.current !== null) {
      observedScrollTopRef.current = actualTop;
      return;
    }
    // previousTop is the last ledger position used to infer motion toward or away from an owned target.
    // previousTop 是账本最后位置，用于推断位移是朝向还是背离已拥有目标。
    const previousTop = reconcileConversationNativeScrollClamp(
      observedScrollTopRef.current,
      actualTop,
      Math.max(0, element.scrollHeight - element.clientHeight),
    );
    // target is non-null only while a controller-owned operation is in flight.
    // target 仅在控制器拥有的操作进行中时非空。
    const target = programmaticTargetRef.current;
    // movement is the deterministic ownership decision for the two consecutive native observations.
    // movement 是两次连续原生观察值对应的确定性所有权决策。
    const movement = classifyConversationObservedScrollMovement(previousTop, actualTop, target?.top ?? null);
    if (!movement.retainTarget) {
      programmaticTargetRef.current = null;
    }
    observedScrollTopRef.current = actualTop;
    if (movement.readerMoved) {
      userRevisionRef.current += 1;
      layoutTransactionRef.current = null;
      foldLimitRef.current = null;
      cancelPendingFrame();
      publishFollowTail(conversationViewportAllowsTailFollow(element));
    }
    if (!followTailRef.current) {
      reconcileTailReserve(element);
    }
    writeCurrentSnapshot(element, surfaceId);
  }, [cancelPendingFrame, publishFollowTail, reconcileTailReserve, writeCurrentSnapshot]);

  // captureAndScheduleLayoutRestore preserves the current semantic viewport across one known geometry change.
  // captureAndScheduleLayoutRestore 在一次已知几何变化中保留当前语义视口。
  // Parameters: none.
  // 参数：无。
  // Returns: nothing.
  // 返回值：无。
  const captureAndScheduleLayoutRestore = React.useCallback((): void => {
    // currentOptions selects the exact mounted business surface before its geometry changes.
    // currentOptions 在几何变化前选择精确已挂载业务界面。
    const currentOptions = latestOptionsRef.current;
    // element owns the semantic anchor captured synchronously before the scheduled correction.
    // element 拥有在已调度校正前同步捕获的语义锚点。
    const element = currentOptions.scrollRef.current;
    if (!currentOptions.active || !currentOptions.surfaceId || !element) {
      return;
    }
    if (element.dataset.conversationSurfaceId !== currentOptions.surfaceId) {
      return;
    }
    // Child composer layout effects run before this owner's surface restoration effect on navigation.
    // 导航时，输入框子组件的布局副作用先于当前所有者的界面恢复副作用执行。
    // Until restoration completes, the incoming DOM still carries the previous surface's tail ownership.
    // 在恢复完成之前，新界面的 DOM 仍沿用上一界面的尾部所有权状态。
    // Preserve the saved snapshot instead of replacing it with this uninitialized geometry.
    // 保留已存快照，避免将其替换为尚未初始化的几何状态。
    if (renderedSurfaceIdRef.current !== currentOptions.surfaceId) {
      return;
    }
    // snapshot freezes the current business anchor before the geometry mutation commits.
    // snapshot 在几何修改提交前固定当前业务锚点。
    const snapshot = writeCurrentSnapshot(element, currentOptions.surfaceId);
    scheduleSnapshotRestore(currentOptions.surfaceId, snapshot);
  }, [scheduleSnapshotRestore, writeCurrentSnapshot]);

  // scheduleStoredRestore reuses the latest surface snapshot for geometry changes observed after mutation.
  // scheduleStoredRestore 为修改后观察到的几何变化复用最新界面快照。
  // Parameters: none.
  // 参数：无。
  // Returns: nothing.
  // 返回值：无。
  const scheduleStoredRestore = React.useCallback((): void => {
    // currentOptions selects the active business surface and its exact stored state.
    // currentOptions 选择活动业务界面及其精确保存状态。
    const currentOptions = latestOptionsRef.current;
    if (!currentOptions.active || !currentOptions.surfaceId) {
      return;
    }
    // snapshot is absent only for an unseen surface, which is initialized by the surface effect.
    // snapshot 仅在未见界面中缺失，而该情况由界面 effect 初始化。
    const snapshot = currentOptions.readSnapshot(currentOptions.surfaceId);
    if (!snapshot) {
      return;
    }
    scheduleSnapshotRestore(currentOptions.surfaceId, snapshot);
  }, [scheduleSnapshotRestore]);

  // scrollToBottom transfers viewport ownership to tail following through one optional native animation.
  // scrollToBottom 通过一次可选原生动画将视口所有权切换到尾部跟随。
  // Parameters: behavior selects immediate or smooth native movement.
  // 参数：behavior 选择立即或平滑原生移动。
  // Returns: nothing.
  // 返回值：无。
  const scrollToBottom = React.useCallback((behavior: ScrollBehavior = "smooth"): void => {
    // currentOptions resolves the active surface at user action time.
    // currentOptions 解析用户操作时的活动界面。
    const currentOptions = latestOptionsRef.current;
    // element is the exact current mutation target.
    // element 是当前精确修改目标。
    const element = currentOptions.scrollRef.current;
    if (!currentOptions.active || !currentOptions.surfaceId || !element) {
      return;
    }
    cancelPendingFrame();
    surfaceRestoreRef.current = null;
    publishFollowTail(true);
    writeScrollTop(element, element.scrollHeight, behavior);
    writeCurrentSnapshot(element, currentOptions.surfaceId);
  }, [cancelPendingFrame, publishFollowTail, writeCurrentSnapshot, writeScrollTop]);

  // revealAnchor transfers viewport ownership to the reader and moves one mounted anchor near the viewport top.
  // revealAnchor 将视口所有权交还阅读者，并把一个已挂载锚点移动到视口顶部附近。
  // Parameters: anchor is a descendant of the current scrollport; behavior selects immediate or smooth native movement.
  // 参数：anchor 是当前滚动视口的后代节点；behavior 选择立即或平滑原生移动。
  // Returns: nothing.
  // 返回值：无。
  const revealAnchor = React.useCallback((anchor: HTMLElement | Range, behavior: ScrollBehavior = "smooth"): void => {
    // currentOptions resolves the active surface at user action time.
    // currentOptions 解析用户操作时的活动界面。
    const currentOptions = latestOptionsRef.current;
    // element is the exact current mutation target.
    // element 是当前精确修改目标。
    const element = currentOptions.scrollRef.current;
    if (!currentOptions.active || !currentOptions.surfaceId || !element || !element.contains(anchor instanceof Range ? anchor.startContainer : anchor)) {
      return;
    }
    // anchorOffset is the anchor's current distance from the scrollport's visible top edge.
    // anchorOffset 是锚点当前距滚动视口可见顶部边缘的距离。
    const anchorOffset = anchor.getBoundingClientRect().top - element.getBoundingClientRect().top;
    userRevisionRef.current += 1;
    layoutTransactionRef.current = null;
    cancelPendingFrame();
    publishFollowTail(false);
    const topGap = anchor instanceof Range ? element.clientHeight / 3 : CONVERSATION_ANCHOR_REVEAL_TOP_GAP_PX;
    // Jump directly to an unloaded shell so intermediate turns do not mount during a long animation.
    // 直接跳转到未挂载的外壳，避免长距离动画途中挂载中间回合。
    const unloaded = anchor instanceof HTMLElement && anchor.hasAttribute("data-conversation-turn-placeholder");
    pendingTurnRevealRef.current = unloaded ? { shell: anchor, revision: userRevisionRef.current } : null;
    writeScrollTop(element, element.scrollTop + anchorOffset - topGap, unloaded ? "auto" : behavior);
    writeCurrentSnapshot(element, currentOptions.surfaceId);
  }, [cancelPendingFrame, publishFollowTail, writeCurrentSnapshot, writeScrollTop]);

  // preserveHistoryPrepend holds the semantic anchor unless reader movement invalidates the async operation.
  // preserveHistoryPrepend 在阅读者位移使异步操作失效前保持语义锚点。
  // Parameters: load performs one exact older-history request and reports whether rows were prepended.
  // 参数：load 执行一次精确更早历史请求，并报告是否已前置行。
  // Returns: whether history was loaded; an eligible viewport restoration is scheduled before resolution.
  // 返回值：是否已加载历史；符合条件的视口恢复会在 Promise 结束前完成调度。
  const preserveHistoryPrepend = React.useCallback(async (
    load: () => Promise<boolean>,
  ): Promise<boolean> => {
    // currentOptions binds the operation to the currently rendered business surface.
    // currentOptions 将操作绑定到当前已渲染业务界面。
    const currentOptions = latestOptionsRef.current;
    // element supplies the pre-request semantic anchor.
    // element 提供请求前语义锚点。
    const element = currentOptions.scrollRef.current;
    if (!currentOptions.active || !currentOptions.surfaceId || !element) {
      return false;
    }
    // surfaceId remains fixed even if navigation occurs while the request is in flight.
    // surfaceId 即使请求期间发生导航也保持固定。
    const surfaceId = currentOptions.surfaceId;
    // snapshot is the only geometry input used after older rows mount.
    // snapshot 是更早行挂载后使用的唯一几何输入。
    const snapshot = writeCurrentSnapshot(element, surfaceId);
    // revision invalidates the restore when native reader movement is observed before completion.
    // revision 在完成前观察到原生阅读者移动时使恢复失效。
    const revision = userRevisionRef.current;
    // loaded is the authoritative backend pagination result.
    // loaded 是权威后端分页结果。
    const loaded = await load();
    if (!loaded || latestOptionsRef.current.surfaceId !== surfaceId || userRevisionRef.current !== revision) {
      return loaded;
    }
    scheduleSnapshotRestore(surfaceId, snapshot, revision);
    return true;
  }, [scheduleSnapshotRestore, writeCurrentSnapshot]);

  React.useEffect(() => {
    // Bind the listener to this owner so a child transcript cannot adjust the main transcript.
    // 将监听器绑定到当前所有者，防止子转录调整主转录。
    const element = options.scrollRef.current;
    const surfaceId = options.surfaceId;
    if (!options.active || !element || !surfaceId) return;

    // Restore the pre-swap row offset through the existing ledger, including during history prepend.
    // 通过现有账本恢复切换前的行偏移，包括历史前插期间。
    const handleWindowLayout = (event: Event): void => {
      if (element.dataset.conversationSurfaceId !== surfaceId) return;
      const progress = scrollbarProgressRef.current;
      if (progress !== null) {
        applyScrollbarPosition(element, progress);
        return;
      }
      if (programmaticTargetRef.current) return;
      if (followTailRef.current) {
        scheduleStoredRestore();
        return;
      }
      // A returning reader's original offset becomes usable when the target turn gains its real height.
      // 目标回合取得真实高度后，恢复会话时保存的原始行内偏移才能生效。
      const surfaceRestore = surfaceRestoreRef.current;
      if (surfaceRestore?.surfaceId === surfaceId && surfaceRestore.userRevision === userRevisionRef.current) {
        restoreSnapshot(element, surfaceRestore.snapshot);
        return;
      }
      // A surface or fold restore already has an authoritative anchor of its own.
      // 界面恢复或折叠恢复已有自己的权威锚点。
      if (pendingFrameRef.current !== null) return;
      // A jump targets the requested shell even when the preceding row intersects its top margin.
      // 即使上一回合与跳转顶部留白相交，仍以请求的外壳为跳转目标。
      const reveal = pendingTurnRevealRef.current;
      if (reveal?.revision === userRevisionRef.current && element.contains(reveal.shell)
        && !reveal.shell.hasAttribute("data-conversation-turn-placeholder")) {
        pendingTurnRevealRef.current = null;
        writeScrollTop(element, element.scrollTop + reveal.shell.getBoundingClientRect().top
          - element.getBoundingClientRect().top - CONVERSATION_ANCHOR_REVEAL_TOP_GAP_PX);
        writeCurrentSnapshot(element, surfaceId);
        return;
      }
      const { anchor } = (event as CustomEvent<ConversationTurnWindowLayoutDetail>).detail;
      if (!anchor) return;
      writeScrollTop(element, conversationScrollTopForSnapshot(element, {
        anchorKey: anchor.rowKey,
        anchorTop: anchor.offsetTop,
        scrollTop: element.scrollTop,
        followTail: false,
      }));
      writeCurrentSnapshot(element, surfaceId);
    };
    // Pointer movement takes ownership from reading restores until release.
    // 指针移动接管阅读位置恢复，直至松手。
    const handleScrollbarPosition = (event: Event): void => {
      if (element.dataset.conversationSurfaceId !== surfaceId) return;
      const { progress, dragging } = (event as CustomEvent<ConversationScrollbarPosition>).detail;
      if (scrollbarProgressRef.current === null) {
        userRevisionRef.current += 1;
        layoutTransactionRef.current = null;
        pendingTurnRevealRef.current = null;
        foldLimitRef.current = null;
        cancelPendingFrame();
        publishFollowTail(false);
      }
      scrollbarProgressRef.current = dragging ? progress : null;
      applyScrollbarPosition(element, progress);
      if (!dragging) {
        // Release any unused fold space before settling against the final scroll range.
        // 先释放未用的折叠预留，再按最终滚动范围定位。
        reconcileTailReserve(element);
        applyScrollbarPosition(element, progress);
        publishFollowTail(conversationViewportAllowsTailFollow(element));
        writeCurrentSnapshot(element, surfaceId);
      }
    };
    element.addEventListener(CONVERSATION_TURN_WINDOW_LAYOUT_EVENT, handleWindowLayout);
    element.addEventListener(CONVERSATION_SCROLLBAR_POSITION_EVENT, handleScrollbarPosition);
    return () => {
      scrollbarProgressRef.current = null;
      element.removeEventListener(CONVERSATION_TURN_WINDOW_LAYOUT_EVENT, handleWindowLayout);
      element.removeEventListener(CONVERSATION_SCROLLBAR_POSITION_EVENT, handleScrollbarPosition);
    };
  }, [options.active, options.scrollRef, options.surfaceId, applyScrollbarPosition, cancelPendingFrame,
    publishFollowTail, reconcileTailReserve, restoreSnapshot, scheduleStoredRestore, writeCurrentSnapshot, writeScrollTop]);

  React.useLayoutEffect(() => {
    // surfaceId is the exact active business identity after the latest React commit.
    // surfaceId 是最近 React 提交后的精确活动业务身份。
    const surfaceId = options.surfaceId;
    // element is the shared scroll owner now representing that identity.
    // element 是当前表示该身份的共享滚动所有者。
    const element = options.scrollRef.current;
    if (!options.active || !surfaceId || !element) {
      // An unmaterialized surface must restore again even when the same business id remounts later.
      // 未实体化界面必须在同一业务标识稍后重新挂载时再次执行恢复。
      cancelPendingFrame();
      renderedSurfaceIdRef.current = null;
      return;
    }
    if (renderedSurfaceIdRef.current === surfaceId) {
      return;
    }
    cancelPendingFrame();
    layoutTransactionRef.current = null;
    renderedSurfaceIdRef.current = surfaceId;
    pendingTurnRevealRef.current = null;
    // A newly rendered surface starts without the previous surface's tail reserve.
    // 新渲染的界面不继承上一界面的尾部预留。
    element.querySelector<HTMLElement>(".vk-transcript-tail")?.style.setProperty("height", "0px");
    tailTotalRef.current = 0;
    foldLimitRef.current = null;
    observedScrollTopRef.current = element.scrollTop;
    programmaticTargetRef.current = null;
    // savedSnapshot restores a previously read position; unseen surfaces start with explicit tail ownership.
    // savedSnapshot 恢复此前阅读位置；未见界面以显式尾部所有权开始。
    const savedSnapshot = options.readSnapshot(surfaceId) ?? {
      anchorKey: null,
      anchorTop: 0,
      followTail: true,
      scrollTop: element.scrollHeight,
    };
    restoreSnapshot(element, savedSnapshot);
    reconcileTailReserve(element);
    surfaceRestoreRef.current = savedSnapshot.followTail
      ? null
      : { surfaceId, snapshot: savedSnapshot, userRevision: userRevisionRef.current };
    // Keep the original keyed snapshot until the restored position can be captured against mounted content.
    // 在能按已挂载正文捕获恢复位置之前，保留原始带键快照。
    if (savedSnapshot.followTail || !savedSnapshot.anchorKey) writeCurrentSnapshot(element, surfaceId);
  }, [
    cancelPendingFrame,
    options.active,
    options.readSnapshot,
    options.scrollRef,
    options.surfaceId,
    reconcileTailReserve,
    restoreSnapshot,
    writeCurrentSnapshot,
  ]);

  React.useLayoutEffect(() => {
    // element is the active transcript whose content geometry is observed as one aggregate surface.
    // element 是作为一个聚合界面观察内容几何的活动转录。
    const element = options.scrollRef.current;
    if (!options.active || !options.surfaceId || !element || typeof ResizeObserver === "undefined") {
      return;
    }
    // surfaceId freezes the non-null business owner used by asynchronous resize callbacks.
    // surfaceId 冻结异步尺寸回调使用的非空业务所有者。
    const surfaceId = options.surfaceId;
    // content is the single transcript content root; individual Turn observers are deliberately absent.
    // content 是唯一转录内容根；刻意不设置逐 Turn 观察器。
    const content = element.querySelector<HTMLElement>(".vk-transcript-inner");
    if (!content) {
      return;
    }
    // observer corrects only explicit tail ownership or one active manual layout transaction.
    // observer 仅校正显式尾部所有权或单个活动手动布局事务。
    const observer = new ResizeObserver(() => {
      // During dragging the pointer stays authoritative even if streaming changes the total height.
      // 拖动期间即使流式输出改变总高度，仍以指针位置为准。
      const progress = scrollbarProgressRef.current;
      if (progress !== null) {
        applyScrollbarPosition(element, progress);
        return;
      }
      // The reserve absorbs shrinkage before any tail correction reads the new bottom.
      // 在任何尾部校正读取新底部之前，先由预留吸收收缩。
      reconcileTailReserve(element);
      // A restored reading position keeps its anchor while the switched-in content finishes rendering.
      // 切换进来的内容完成渲染期间，已恢复的阅读位置保持其锚点。
      const restore = surfaceRestoreRef.current;
      if (restore && restore.surfaceId === surfaceId && restore.userRevision === userRevisionRef.current) {
        scheduleSnapshotRestore(restore.surfaceId, restore.snapshot, restore.userRevision);
        return;
      }
      surfaceRestoreRef.current = null;
      // transaction keeps one immutable pre-mutation anchor instead of chasing snapshots rewritten every frame.
      // transaction 保持单个不可变修改前锚点，避免追逐每帧重写的快照。
      const transaction = layoutTransactionRef.current;
      const transactionActive = Boolean(
        transaction
        && transaction.surfaceId === surfaceId
        && transaction.userRevision === userRevisionRef.current
      );
      // ownershipDecision lets a browser-clamped bottom cancel a stale reader-owned fold transaction.
      // ownershipDecision 允许浏览器裁剪到的新底部取消过期的阅读者折叠事务。
      const ownershipDecision = conversationResizeOwnershipDecision(
        followTailRef.current,
        transactionActive,
        conversationViewportAllowsTailFollow(element),
      );
      if (ownershipDecision.adoptedTail) {
        layoutTransactionRef.current = null;
        publishFollowTail(true);
        writeCurrentSnapshot(element, surfaceId);
      }
      // correctionMode is resolved only after bottom geometry has had a chance to restore ownership.
      // correctionMode 仅在底部几何状态获得恢复所有权机会后解析。
      const correctionMode = ownershipDecision.correctionMode;
      if (correctionMode === "transaction" && transaction) {
        scheduleSnapshotRestore(
          transaction.surfaceId,
          transaction.snapshot,
          transaction.userRevision,
        );
        return;
      }
      if (correctionMode === "tail") {
        scheduleStoredRestore();
      }
    });
    observer.observe(content);
    return () => observer.disconnect();
  }, [
    options.active,
    options.scrollRef,
    options.surfaceId,
    applyScrollbarPosition,
    publishFollowTail,
    reconcileTailReserve,
    scheduleSnapshotRestore,
    scheduleStoredRestore,
    writeCurrentSnapshot,
  ]);

  React.useLayoutEffect(() => {
    // element and surfaceId bind mutation delivery to this mounted transcript, including leaf-only commits.
    // element 与 surfaceId 将修改派发绑定到当前已挂载转录，包括仅发生在叶子的提交。
    const element = options.scrollRef.current;
    const surfaceId = options.surfaceId;
    if (!options.active || !surfaceId || !element || typeof MutationObserver === "undefined") {
      return;
    }
    // content is the same aggregate root observed for persistent geometry changes.
    // content 是观察持久几何变化的同一个聚合根节点。
    const content = element.querySelector<HTMLElement>(".vk-transcript-inner");
    if (!content) {
      return;
    }
    // A remove/measure/insert commit can clip scrollTop yet restore the original size, producing no resize.
    // 移除／测量／插入提交可截断 scrollTop 却恢复原尺寸，因此不产生尺寸变化通知。
    // Acknowledge that displaced owned tail in the mutation microtask before the queued native scroll event.
    // 在排队的原生滚动事件前，通过修改微任务确认已拥有尾部发生的位移。
    // ResizeObserver handles ordinary growth before paint. This observer also catches a native clamp with no resize.
    // 普通增长由尺寸观察器在绘制前处理。此观察器还会捕获未触发尺寸变化的原生滚动截断。
    const observer = new MutationObserver(() => {
      if (latestOptionsRef.current.surfaceId !== surfaceId
        || element.dataset.conversationSurfaceId !== surfaceId
        || !followTailRef.current) {
        return;
      }
      // A collapse lands here before the resize notification; absorb it so the tail does not move.
      // 折叠先于尺寸通知到达此处；先吸收收缩，使尾部不移动。
      reconcileTailReserve(element);
      if (Math.abs(element.scrollTop - observedScrollTopRef.current) <= 0.5) {
        return;
      }
      scheduleStoredRestore();
    });
    observer.observe(content, {
      childList: true,
      subtree: true,
      characterData: true,
      attributes: true,
      attributeFilter: ["class", "style", "hidden"],
    });
    return () => observer.disconnect();
  }, [options.active, options.scrollRef, options.surfaceId, reconcileTailReserve, scheduleStoredRestore]);

  React.useEffect(() => {
    // element owns the native smooth-scroll lifecycle for this exact transcript.
    // element 拥有当前精确转录的原生平滑滚动生命周期。
    const element = options.scrollRef.current;
    if (!options.active || !options.surfaceId || !element) {
      return;
    }
    // handleScrollEnd closes a smooth target that the browser stopped before reaching its destination.
    // handleScrollEnd 关闭浏览器在到达目标前停止的平滑滚动目标。
    const handleScrollEnd = (): void => {
      // target remains registered only when the native motion ended away from its owned destination.
      // target 仅在原生位移未到达所属目的地即结束时仍保持注册。
      const target = programmaticTargetRef.current;
      if (!target) {
        return;
      }
      // reachedTarget distinguishes a normal smooth completion from reader interruption.
      // reachedTarget 区分正常平滑完成与阅读者中断。
      const reachedTarget = Math.abs(element.scrollTop - target.top) <= 1;
      programmaticTargetRef.current = null;
      observedScrollTopRef.current = element.scrollTop;
      if (!reachedTarget) {
        userRevisionRef.current += 1;
        cancelPendingFrame();
        publishFollowTail(conversationViewportAllowsTailFollow(element));
      }
      // surfaceId binds the terminal native event to the still-active business transcript.
      // surfaceId 将最终原生事件绑定到仍处于活动状态的业务转录。
      const surfaceId = element.dataset.conversationSurfaceId;
      if (surfaceId && surfaceId === latestOptionsRef.current.surfaceId) {
        writeCurrentSnapshot(element, surfaceId);
      }
    };
    element.addEventListener("scrollend", handleScrollEnd);
    return () => element.removeEventListener("scrollend", handleScrollEnd);
  }, [cancelPendingFrame, options.active, options.scrollRef, options.surfaceId, publishFollowTail, writeCurrentSnapshot]);

  React.useEffect(() => {
    // handleLayoutWillChange captures the exact semantic viewport before collapse or shell geometry mutation.
    // handleLayoutWillChange 在折叠或外壳几何修改前捕获精确语义视口。
    const handleLayoutWillChange = (): void => {
      captureAndScheduleLayoutRestore();
    };
    window.addEventListener(CONVERSATION_LAYOUT_WILL_CHANGE_EVENT, handleLayoutWillChange);
    return () => window.removeEventListener(CONVERSATION_LAYOUT_WILL_CHANGE_EVENT, handleLayoutWillChange);
  }, [captureAndScheduleLayoutRestore]);

  React.useEffect(() => {
    // handleManualViewportInteraction preserves the current ownership while the focused tool changes geometry.
    // handleManualViewportInteraction 在聚焦工具改变几何时保留当前滚动所有权。
    const handleManualViewportInteraction = (event: Event): void => {
      // detail carries the exact committed surface identity resolved by the fold action.
      // detail 携带折叠操作从已提交 DOM 解析出的精确界面身份。
      const detail = (event as CustomEvent<ConversationManualViewportInteractionDetail>).detail;
      // currentOptions selects the one controller allowed to respond to this manual action.
      // currentOptions 选择唯一允许响应当前手动操作的控制器。
      const currentOptions = latestOptionsRef.current;
      if (!detail?.surfaceId || detail.surfaceId !== currentOptions.surfaceId) {
        return;
      }
      // element is the committed scrollport containing the focused manual control.
      // element 是包含聚焦手动控件的已提交滚动视口。
      const element = currentOptions.scrollRef.current;
      if (!element || element.dataset.conversationSurfaceId !== detail.surfaceId) {
        return;
      }
      cancelPendingFrame();
      programmaticTargetRef.current = null;
      userRevisionRef.current += 1;
      beginFoldReserve(element);
      // A fold click changes layout, not scroll intent: preserve tail ownership or the existing reader anchor.
      // 折叠点击只改变布局，不改变滚动意图：保留锁底状态或既有阅读锚点。
      const snapshot = writeCurrentSnapshot(element, detail.surfaceId);
      layoutTransactionRef.current = {
        surfaceId: detail.surfaceId,
        snapshot,
        userRevision: userRevisionRef.current,
      };
      scheduleSnapshotRestore(detail.surfaceId, snapshot, userRevisionRef.current);
    };
    window.addEventListener(CONVERSATION_MANUAL_VIEWPORT_INTERACTION_EVENT, handleManualViewportInteraction);
    return () => window.removeEventListener(CONVERSATION_MANUAL_VIEWPORT_INTERACTION_EVENT, handleManualViewportInteraction);
  }, [beginFoldReserve, cancelPendingFrame, scheduleSnapshotRestore, writeCurrentSnapshot]);

  React.useEffect(() => {
    // automaticFoldTimer releases a fold window that no settled event closes, such as a running tool
    // finishing while another one keeps the group body open.
    // automaticFoldTimer 释放没有稳定事件来关闭的折叠窗口，例如另一个工具仍使分组正文打开时某个工具完成。
    let automaticFoldTimer: number | null = null;
    // handleAutomaticFold lets a self-started fold use the same tail reserve as a manual one, but only
    // while the tail is followed: a reader elsewhere in the history keeps the existing anchor behavior.
    // handleAutomaticFold 让自行发起的折叠与手动折叠共用尾部预留，但仅限锁底期间：位于历史其他位置的阅读者保持既有锚点行为。
    const handleAutomaticFold = (event: Event): void => {
      const detail = (event as CustomEvent<ConversationManualViewportInteractionDetail>).detail;
      const currentOptions = latestOptionsRef.current;
      if (!detail?.surfaceId || detail.surfaceId !== currentOptions.surfaceId || !followTailRef.current) {
        return;
      }
      const element = currentOptions.scrollRef.current;
      if (!element || element.dataset.conversationSurfaceId !== detail.surfaceId) {
        return;
      }
      beginFoldReserve(element);
      if (automaticFoldTimer !== null) window.clearTimeout(automaticFoldTimer);
      automaticFoldTimer = window.setTimeout(() => {
        automaticFoldTimer = null;
        const current = latestOptionsRef.current.scrollRef.current;
        if (foldLimitRef.current !== null && current) {
          reconcileTailReserve(current);
          foldLimitRef.current = null;
        }
      }, motionTokenMs("--vk-t-med") + 80);
    };
    window.addEventListener(CONVERSATION_AUTOMATIC_FOLD_EVENT, handleAutomaticFold);
    return () => {
      window.removeEventListener(CONVERSATION_AUTOMATIC_FOLD_EVENT, handleAutomaticFold);
      if (automaticFoldTimer !== null) window.clearTimeout(automaticFoldTimer);
    };
  }, [beginFoldReserve, reconcileTailReserve]);

  React.useEffect(() => {
    // handleLayoutSettled applies one final correction and releases per-frame transaction ownership.
    // handleLayoutSettled 应用一次最终校正，并释放逐帧事务所有权。
    const handleLayoutSettled = (event: Event): void => {
      // transaction is absent for programmatic or unrelated geometry changes.
      // transaction 对程序化或无关几何变化为空。
      const transaction = layoutTransactionRef.current;
      // detail prevents another mounted transcript from releasing this surface's transaction.
      // detail 防止其他已挂载转录界面释放当前界面的事务。
      const detail = (event as CustomEvent<ConversationLayoutSettledDetail>).detail;
      // The fold window absorbs its final geometry, then the reserve may only shrink again.
      // 折叠窗口吸收最终几何后关闭，此后预留只能缩小。
      const element = latestOptionsRef.current.scrollRef.current;
      if (foldLimitRef.current !== null && element && detail?.surfaceId
        && detail.surfaceId === latestOptionsRef.current.surfaceId) {
        reconcileTailReserve(element);
        foldLimitRef.current = null;
      }
      if (
        !transaction
        || !detail?.surfaceId
        || detail.surfaceId !== transaction.surfaceId
        || transaction.surfaceId !== latestOptionsRef.current.surfaceId
      ) {
        return;
      }
      layoutTransactionRef.current = null;
      scheduleSnapshotRestore(
        transaction.surfaceId,
        transaction.snapshot,
        transaction.userRevision,
      );
    };
    window.addEventListener(CONVERSATION_LAYOUT_SETTLED_EVENT, handleLayoutSettled);
    return () => window.removeEventListener(CONVERSATION_LAYOUT_SETTLED_EVENT, handleLayoutSettled);
  }, [reconcileTailReserve, scheduleSnapshotRestore]);

  React.useEffect(() => () => {
    layoutTransactionRef.current = null;
    cancelPendingFrame();
  }, [cancelPendingFrame]);

  return React.useMemo(() => ({
    captureAndScheduleLayoutRestore,
    handleScroll,
    preserveHistoryPrepend,
    remember,
    revealAnchor,
    scheduleStoredRestore,
    scrollToBottom,
  }), [
    captureAndScheduleLayoutRestore,
    handleScroll,
    preserveHistoryPrepend,
    remember,
    revealAnchor,
    scheduleStoredRestore,
    scrollToBottom,
  ]);
}
