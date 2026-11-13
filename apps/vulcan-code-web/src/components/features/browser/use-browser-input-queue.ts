import * as React from "react";

import { requestCoreBrowser, type CoreBrowserLease } from "@/service/core-browser-service";
import type { BrowserDomViewport } from "./browser-dom-assets";

// BrowserInputResult describes the Core-confirmed outcome of one input action.
// BrowserInputResult 描述一条输入操作经 Core 确认的结果。
interface BrowserInputResult {
  outcome: "accepted" | "applied" | "rejected" | "unknown";
  reason?: string;
}

// QueuedBrowserInput binds an input to the exact Core owner, browser, page, and control generation.
// QueuedBrowserInput 将输入绑定到精确的 Core 归属、浏览器、页面与控制代次。
export interface QueuedBrowserInput {
  ownerSessionId: string;
  sessionId: string;
  pageId: string;
  leaseSecret: string;
  pageGeneration: string;
  viewportGeneration: string;
  kind: string;
  payload: Record<string, unknown>;
}

// HeldBrowserPointer remembers the pressed button and last page coordinates until release.
// HeldBrowserPointer 记录已按下按钮及最后页面坐标，直至指针释放。
export interface HeldBrowserPointer {
  pointerId: number;
  button: "left" | "middle" | "right";
  x: number;
  y: number;
}

// BrowserInputQueueOptions shares the panel's live selection, credential and frame references with the queue.
// BrowserInputQueueOptions 将面板的实时选择、凭据与画面引用共享给输入队列。
export interface BrowserInputQueueOptions {
  // Current control grant; cleared synchronously when Core revokes it.
  // 当前控制授权；Core 撤销时同步清空。
  leaseRef: React.RefObject<CoreBrowserLease | null>;
  // Last accepted frame, which defines page coordinates and input generations.
  // 最近接受的画面，定义页面坐标与输入代次。
  frameRef: React.RefObject<BrowserDomViewport | null>;
  // Whether input must wait for a newer frame after navigation or resize.
  // 导航或调整尺寸后输入是否必须等待更新画面。
  projectionSuspended: React.RefObject<boolean>;
  // Trusted conversation owner committed before paint.
  // 在绘制前提交的可信会话归属。
  ownerSessionRef: React.RefObject<string>;
  // Selected Core browser identity committed before paint.
  // 在绘制前提交的选定 Core 浏览器身份。
  selectedSessionRef: React.RefObject<string | null>;
  // Selected Core page identity committed before paint.
  // 在绘制前提交的选定 Core 页面身份。
  selectedPageRef: React.RefObject<string | null>;
  // Latest coalesced drag position, also cleared by panel selection resets.
  // 最新合并的拖拽位置，也会被面板选择重置清空。
  pendingPointerMove: React.RefObject<QueuedBrowserInput | null>;
  // Publishes a visible input failure.
  // 发布可见的输入失败。
  setError: (message: string | null) => void;
  // Drops the control grant after Core reports it lost.
  // Core 报告授权丢失后丢弃控制授权。
  setLease: (lease: CoreBrowserLease | null) => void;
}

// useBrowserInputQueue orders page input behind one promise chain and coalesces drag moves.
// useBrowserInputQueue 将页面输入排在同一 Promise 链之后，并合并拖拽移动。
// Parameters: options supplies live panel references. Returns: the queue entry points.
// 参数：options 提供实时面板引用。返回值：队列入口。
export function useBrowserInputQueue({
  leaseRef,
  frameRef,
  projectionSuspended,
  ownerSessionRef,
  selectedSessionRef,
  selectedPageRef,
  pendingPointerMove,
  setError,
  setLease,
}: BrowserInputQueueOptions) {
  // inputSequence tracks the sender sequence required by Core's input protocol.
  // inputSequence 跟踪 Core 输入协议要求的发送方序号。
  const inputSequence = React.useRef(0);
  // inputQueue preserves down/up order across asynchronous HTTP requests.
  // inputQueue 在异步 HTTP 请求之间保持按下和释放的顺序。
  const inputQueue = React.useRef<Promise<boolean>>(Promise.resolve(true));
  // pointerMoveQueued ensures at most one coalescing worker waits in the input queue.
  // pointerMoveQueued 确保输入队列中最多只有一个移动事件合并任务。
  const pointerMoveQueued = React.useRef(false);

  // captureInput freezes the current owner, page, lease, and frame for one input event.
  // captureInput 为一条输入固定当前归属、页面、租约和画面；无可用授权时返回空值。
  function captureInput(kind: string, payload: Record<string, unknown>): QueuedBrowserInput | null {
    const currentLease = leaseRef.current;
    const currentFrame = frameRef.current;
    const sessionId = selectedSessionRef.current;
    const pageId = selectedPageRef.current;
    if (!currentLease || !currentFrame || projectionSuspended.current || !sessionId || !pageId
      || currentLease.session.sessionId !== sessionId || currentFrame.pageId !== pageId) return null;
    return {
      ownerSessionId: ownerSessionRef.current, sessionId, pageId,
      leaseSecret: currentLease.leaseSecret,
      pageGeneration: currentFrame.pageGeneration,
      viewportGeneration: currentFrame.viewportGeneration,
      kind, payload,
    };
  }

  // inputStillAuthorized permits only key or pointer release to finish on a previously selected page.
  // inputStillAuthorized 只允许按键或指针释放在先前选中的页面完成；返回当前授权是否仍有效。
  function inputStillAuthorized(input: QueuedBrowserInput): boolean {
    // releasingInput never authorizes an old conversation or an old browser lease.
    // releasingInput 不会授权旧会话或旧浏览器租约。
    const releasingInput = (input.kind === "key" || input.kind === "pointer") && input.payload.type === "up";
    return ownerSessionRef.current === input.ownerSessionId
      && selectedSessionRef.current === input.sessionId
      && leaseRef.current?.leaseSecret === input.leaseSecret
      && (releasingInput || selectedPageRef.current === input.pageId);
  }

  // deliverInput sends one captured event in queue order and publishes only its still-current outcome.
  // deliverInput 按队列顺序发送一条固定的输入，并仅发布仍属当前选择的结果。
  async function deliverInput(input: QueuedBrowserInput): Promise<boolean> {
    if (!inputStillAuthorized(input)) return false;
    // sequence is assigned at dispatch time so coalesced moves leave no sender gaps.
    // sequence 在实际派发时分配，使合并的移动事件不留下发送方序号空洞。
    const sequence = String(++inputSequence.current);
    try {
      const result = await requestCoreBrowser<BrowserInputResult>(input.ownerSessionId, "browser.input.dispatch", {
        sessionId: input.sessionId,
        pageId: input.pageId,
        leaseSecret: input.leaseSecret,
        pageGeneration: input.pageGeneration,
        viewportGeneration: input.viewportGeneration,
        sequence,
        kind: input.kind,
        payload: input.payload,
      });
      if (result.outcome !== "applied" && inputStillAuthorized(input)) {
        setError(result.reason ?? `Input ${result.outcome}`);
        if (result.outcome === "unknown" || result.reason === "lease_required" || result.reason === "lease_blocked") {
          leaseRef.current = null;
          setLease(null);
        }
      }
      return result.outcome === "applied";
    } catch (failure) {
      if (inputStillAuthorized(input)) setError(String(failure));
      // Transport failure does not prove lease loss; Core will reject stale credentials.
      // 传输失败不能证明租约已丢失；过期凭据会由 Core 明确拒绝。
      return false;
    }
  }

  // queueCapturedInput appends one exact event behind all earlier browser inputs.
  // queueCapturedInput 将一条精确输入追加在此前所有浏览器输入之后。
  function queueCapturedInput(input: QueuedBrowserInput): void {
    inputQueue.current = inputQueue.current.then(() => deliverInput(input));
  }

  // flushPendingPointerMove schedules one latest drag position and repeats only when a newer one exists.
  // flushPendingPointerMove 调度一个最新拖拽位置，只有出现更新位置时才继续调度。
  function flushPendingPointerMove(): void {
    if (pointerMoveQueued.current || !pendingPointerMove.current) return;
    pointerMoveQueued.current = true;
    inputQueue.current = inputQueue.current.then(async () => {
      // latest replaces any earlier move that has not entered Core's dispatch.
      // latest 替换尚未进入 Core 派发的所有更早移动事件。
      const latest = pendingPointerMove.current;
      pendingPointerMove.current = null;
      return latest ? deliverInput(latest) : true;
    }).finally(() => {
      pointerMoveQueued.current = false;
      if (pendingPointerMove.current) flushPendingPointerMove();
    });
  }

  // queuePointerMove keeps one pending drag point instead of accumulating one HTTP request per event.
  // queuePointerMove 仅保留一个待派发拖拽位置，避免为每次指针移动累积 HTTP 请求。
  function queuePointerMove(payload: Record<string, unknown>): void {
    const input = captureInput("pointer", payload);
    if (!input) return;
    pendingPointerMove.current = input;
    flushPendingPointerMove();
  }

  // queueInput preserves down/up, key, text, and wheel order around coalesced pointer moves.
  // queueInput 在合并指针移动的同时保持按下、抬起、按键、文字和滚轮的顺序。
  function queueInput(kind: string, payload: Record<string, unknown>): Promise<boolean> {
    // An intervening non-move event makes the last pending move observable before that event.
    // 插入的非移动事件必须排在最后一个待派发移动事件之后。
    const pending = pendingPointerMove.current;
    if (pending) {
      pendingPointerMove.current = null;
      queueCapturedInput(pending);
    }
    const input = captureInput(kind, payload);
    if (!input) return Promise.resolve(false);
    queueCapturedInput(input);
    return inputQueue.current;
  }

  return { queueInput, queuePointerMove };
}
