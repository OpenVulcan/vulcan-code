import React from "react";
import {
  captureConversationScrollSnapshot,
  conversationScrollTopForSnapshot,
  conversationViewportAllowsTailFollow,
  type ConversationScrollSnapshot,
} from "@/components/conversation/chat-area-scroll";
import {
  CONVERSATION_LAYOUT_WILL_CHANGE_EVENT,
  CONVERSATION_MANUAL_VIEWPORT_INTERACTION_EVENT,
  type ConversationManualViewportInteractionDetail,
} from "@/components/conversation/conversation-layout-events";

// UseConversationScrollControllerOptions defines the single owner of one mounted transcript scrollport.
// UseConversationScrollControllerOptions 定义单个已挂载转录滚动视口的唯一所有者。
export interface UseConversationScrollControllerOptions {
  // Whether the transcript contains a materialized scroll surface.
  // 转录是否包含已实体化的滚动界面。
  active: boolean;
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
}

// ProgrammaticScrollTarget describes one in-flight native scroll operation owned by the controller.
// ProgrammaticScrollTarget 描述一个由控制器拥有的进行中原生滚动操作。
interface ProgrammaticScrollTarget {
  // Requested clamped scrollTop destination.
  // 请求的已约束 scrollTop 目标。
  top: number;
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
  // pendingFrameRef owns the sole cancellable animation frame for this scrollport.
  // pendingFrameRef 拥有当前滚动视口唯一可取消的动画帧。
  const pendingFrameRef = React.useRef<number | null>(null);
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

  // scheduleSnapshotRestore coalesces any pending geometry correction into one animation frame.
  // scheduleSnapshotRestore 将任意待处理几何校正合并到一个动画帧。
  // Parameters: surfaceId and snapshot bind the correction to one exact business surface; revision optionally guards reader movement.
  // 参数：surfaceId 与 snapshot 将校正绑定到一个精确业务界面；revision 可选地防止覆盖阅读者移动。
  // Returns: nothing.
  // 返回值：无。
  const scheduleSnapshotRestore = React.useCallback((
    surfaceId: string,
    snapshot: ConversationScrollSnapshot,
    revision: number | null = null,
  ): void => {
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
  }, [cancelPendingFrame, restoreSnapshot, writeCurrentSnapshot]);

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
    // previousTop is the last ledger position used to infer motion toward or away from an owned target.
    // previousTop 是账本最后位置，用于推断位移是朝向还是背离已拥有目标。
    const previousTop = observedScrollTopRef.current;
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
      cancelPendingFrame();
      publishFollowTail(conversationViewportAllowsTailFollow(element));
    }
    writeCurrentSnapshot(element, surfaceId);
  }, [cancelPendingFrame, publishFollowTail, writeCurrentSnapshot]);

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
    publishFollowTail(true);
    writeScrollTop(element, element.scrollHeight, behavior);
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
    renderedSurfaceIdRef.current = surfaceId;
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
    writeCurrentSnapshot(element, surfaceId);
  }, [
    cancelPendingFrame,
    options.active,
    options.readSnapshot,
    options.scrollRef,
    options.surfaceId,
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
    // content is the single transcript content root; individual Turn observers are deliberately absent.
    // content 是唯一转录内容根；刻意不设置逐 Turn 观察器。
    const content = element.querySelector<HTMLElement>(".conversation-scroll-content");
    if (!content) {
      return;
    }
    // observer coalesces every content and width reflow into the controller's sole correction frame.
    // observer 将所有内容与宽度重排合并到控制器唯一校正帧。
    const observer = new ResizeObserver(() => {
      scheduleStoredRestore();
    });
    observer.observe(content);
    return () => observer.disconnect();
  }, [options.active, options.scrollRef, options.surfaceId, scheduleStoredRestore]);

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
    // handleManualViewportInteraction transfers ownership only for the exact transcript containing the focused control.
    // handleManualViewportInteraction 仅为包含聚焦控件的精确转录转移所有权。
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
      publishFollowTail(false);
      // snapshot freezes the reader-owned viewport before the fold mutation changes geometry.
      // snapshot 在折叠修改改变几何前固定阅读者拥有的视口。
      const snapshot = writeCurrentSnapshot(element, detail.surfaceId);
      scheduleSnapshotRestore(detail.surfaceId, snapshot, userRevisionRef.current);
    };
    window.addEventListener(CONVERSATION_MANUAL_VIEWPORT_INTERACTION_EVENT, handleManualViewportInteraction);
    return () => window.removeEventListener(CONVERSATION_MANUAL_VIEWPORT_INTERACTION_EVENT, handleManualViewportInteraction);
  }, [cancelPendingFrame, publishFollowTail, scheduleSnapshotRestore, writeCurrentSnapshot]);

  React.useEffect(() => () => cancelPendingFrame(), [cancelPendingFrame]);

  return React.useMemo(() => ({
    captureAndScheduleLayoutRestore,
    handleScroll,
    preserveHistoryPrepend,
    remember,
    scheduleStoredRestore,
    scrollToBottom,
  }), [
    captureAndScheduleLayoutRestore,
    handleScroll,
    preserveHistoryPrepend,
    remember,
    scheduleStoredRestore,
    scrollToBottom,
  ]);
}
