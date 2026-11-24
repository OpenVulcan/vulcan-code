import * as React from "react";

import { errorMessage } from "@/lib/display-format";
import { requestCoreBrowser, type CoreBrowserLease, type CoreBrowserSession } from "@/service/core-browser-service";
import type { BrowserDomRevision, BrowserDomViewport } from "./browser-dom-assets";

// Group nearby inputs on one lease and release it when the gesture becomes idle.
// 相邻输入共用一个租约，手势空闲时释放。
const INPUT_IDLE_MS = 150;

// Deliberate input may wait for AI, hover only uses a free browser, and releases reuse the current lease.
// 主动输入可等待 AI，悬停只使用空闲浏览器，释放事件复用当前租约。
type ControlAdmission = "interaction" | "idle" | "existing";

// BrowserInputResult is the outcome reported by Core for one dispatched input.
// BrowserInputResult 是 Core 返回的一条已派发输入的结果。
interface BrowserInputResult {
  outcome: "accepted" | "applied" | "rejected" | "unknown";
  reason?: string;
}

// QueuedBrowserInput captures the exact browser, page, and geometry before admission.
// QueuedBrowserInput 在准入前固定精确的浏览器、页面及几何。
export interface QueuedBrowserInput {
  session: CoreBrowserSession;
  pageId: string;
  // The source document may change while an input waits for authorization.
  // 输入等待授权时，源文档可能发生变化。
  documentId: string;
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

// ActiveControl owns a temporary lease and the inputs confirmed by Chromium.
// ActiveControl 持有临时租约及 Chromium 已确认的输入。
interface ActiveControl {
  lease: CoreBrowserLease;
  heartbeat: number;
  keys: Set<string>;
  pointerDown: boolean;
}

// BrowserInputQueueOptions supplies the live selection, projected geometry and gesture state.
// BrowserInputQueueOptions 提供实时选择、投影几何和手势状态。
interface BrowserInputQueueOptions {
  selectedSession: CoreBrowserSession | null;
  frameRef: React.RefObject<(BrowserDomViewport & Pick<BrowserDomRevision, "documentId">) | null>;
  projectionSuspended: React.RefObject<boolean>;
  ownerSessionRef: React.RefObject<string>;
  selectedSessionRef: React.RefObject<string | null>;
  selectedPageRef: React.RefObject<string | null>;
  componentMounted: React.RefObject<boolean>;
  pendingPointerMove: React.RefObject<QueuedBrowserInput | null>;
  heldKeys: React.RefObject<Set<string>>;
  heldPointer: React.RefObject<HeldBrowserPointer | null>;
  setError: (message: string | null) => void;
}

// Queues human commands for the live panel and borrows control only while it dispatches them.
// 为实时面板排队人工命令，仅在派发命令期间取得控制权。
// The returned methods schedule input, run a toolbar action, or discard a departed selection's work.
// 返回的方法用于调度输入、执行工具栏操作或丢弃已离开选择的工作。
export function useBrowserInputQueue({
  selectedSession, frameRef, projectionSuspended, ownerSessionRef, selectedSessionRef, selectedPageRef,
  componentMounted, pendingPointerMove, heldKeys, heldPointer, setError,
}: BrowserInputQueueOptions) {
  // These refs keep browser authority and queue order independent of visual re-renders.
  // 这些引用让浏览器权限与队列顺序独立于视觉重渲染。
  const holder = React.useRef<string | null>(null);
  const active = React.useRef<ActiveControl | null>(null);
  const tail = React.useRef<Promise<void>>(Promise.resolve());
  const generation = React.useRef(0);
  const idleTimer = React.useRef<number | null>(null);
  const inputSequence = React.useRef(0);
  const moveQueued = React.useRef(false);

  // Returns true if this view can still publish feedback for the session's owner.
  // 若此视图仍能为该会话归属发布反馈，则返回 true。
  function currentOwner(session: CoreBrowserSession): boolean {
    return componentMounted.current && ownerSessionRef.current === session.ownerSessionId;
  }

  // Cancels the scheduled lease release before another action joins the queue.
  // 另一项操作进入队列前取消已安排的租约释放。
  function cancelIdleRelease(): void {
    if (idleTimer.current !== null) window.clearTimeout(idleTimer.current);
    idleTimer.current = null;
  }

  // Clears only this control instance and its heartbeat.
  // 只清除此控制实例及其心跳。
  function forgetControl(control: ActiveControl): void {
    if (active.current !== control) return;
    active.current = null;
    window.clearInterval(control.heartbeat);
  }

  // Releases the grant under the owner that acquired it, even after switching away.
  // 即使已切换视图，也在当初获取授权的归属下释放凭据。
  async function releaseLease({ session, leaseSecret }: CoreBrowserLease): Promise<void> {
    await requestCoreBrowser(session.ownerSessionId, "browser.lease.release", { sessionId: session.sessionId, leaseSecret });
  }

  // Releases the active grant and reports only errors for the still-visible target.
  // 释放活动授权，只报告仍可见目标的错误。
  async function releaseActive(): Promise<void> {
    const control = active.current;
    if (!control) return;
    forgetControl(control);
    const { session } = control.lease;
    try {
      await releaseLease(control.lease);
    } catch (failure) {
      if (currentOwner(session) && selectedSessionRef.current === session.sessionId) setError(errorMessage(failure));
    }
  }

  // Discards unsent actions and returns control after the write already in progress settles.
  // 丢弃未发送操作，并在正在执行的写入结束后交还控制权。
  function resetControl(): void {
    generation.current += 1;
    cancelIdleRelease();
    pendingPointerMove.current = null;
    heldKeys.current.clear();
    heldPointer.current = null;
    tail.current = tail.current.then(releaseActive);
  }

  // Returns control after the queue drains and there are no physical presses left.
  // 队列排空且没有仍按住的输入后交还控制权。
  function releaseAtIdle(drained: Promise<void>): void {
    if (tail.current !== drained || !active.current || heldPointer.current || heldKeys.current.size > 0) return;
    idleTimer.current = window.setTimeout(() => {
      idleTimer.current = null;
      if (tail.current === drained) tail.current = drained.then(releaseActive);
    }, INPUT_IDLE_MS);
  }

  // Enqueues one exact action. The relevance check runs before and after lease acquisition.
  // 排队一次精确操作。租约获取前和获取后均检查操作目标是否仍然有效。
  // Passive motion never displaces automation, even if the UI projection is stale.
  // 即使界面投影已过时，悬停移动也不会撤销自动化。
  function enqueue<T>(
    session: CoreBrowserSession,
    stillCurrent: () => boolean,
    operation: (control: ActiveControl) => Promise<T>,
    admission: ControlAdmission = "interaction",
  ): Promise<T | undefined> {
    cancelIdleRelease();
    const capturedGeneration = generation.current;
    const valid = () => capturedGeneration === generation.current && currentOwner(session) && stillCurrent();
    const result = tail.current.then(async () => {
      if (!valid()) return;
      const held = active.current?.lease.session;
      if (held && (held.sessionId !== session.sessionId || held.hostGeneration !== session.hostGeneration
        || held.ownerSessionId !== session.ownerSessionId)) await releaseActive();
      if (!valid()) return;
      let control = active.current;
      if (!control) {
        if (admission === "existing") return;
        holder.current ??= "webui:" + crypto.randomUUID();
        let lease: CoreBrowserLease;
        try {
          lease = await requestCoreBrowser<CoreBrowserLease>(session.ownerSessionId, "browser.lease.acquire", {
            sessionId: session.sessionId, holder: holder.current, forInteraction: admission === "interaction",
          });
        } catch (failure) {
          if (admission === "idle") return;
          // A refused gesture must not replay queued input when another controller later becomes idle.
          // 手势被拒后，不能在其他控制者随后空闲时重放其排队输入。
          if (valid()) resetControl();
          throw failure;
        }
        if (lease.session.sessionId !== session.sessionId || lease.session.ownerSessionId !== session.ownerSessionId)
          throw new Error("Browser control returned a different session.");
        if (!valid() || lease.session.hostGeneration !== session.hostGeneration) {
          await releaseLease(lease);
          if (valid()) throw new Error("Browser restarted before the interaction was admitted.");
          return;
        }
        // Renew only an admitted operation or gesture. Reset removes this timer before releasing the grant.
        // 仅为已准入的操作或手势续期。重置会在释放授权前清除此计时器。
        const heartbeat = window.setInterval(() => {
          void requestCoreBrowser(session.ownerSessionId, "browser.lease.heartbeat", {
            sessionId: session.sessionId, leaseSecret: lease.leaseSecret,
          }).catch(failure => {
            if (active.current?.lease !== lease) return;
            if (currentOwner(session)) setError(errorMessage(failure));
            resetControl();
          });
        }, lease.heartbeatIntervalMs);
        control = { lease, heartbeat, keys: new Set(), pointerDown: false };
        active.current = control;
      }
      return operation(control);
    });
    const drained = result.then(() => undefined, () => undefined);
    tail.current = drained;
    void drained.then(() => releaseAtIdle(drained));
    return result;
  }

  // Runs a toolbar or prompt action on the same control queue as page input.
  // 让工具栏或提示操作与页面输入共用同一个控制队列。
  // A successful whole-browser close consumes its lease, so only local cleanup is needed.
  // 成功关闭整个浏览器会消费租约，只需清理本地状态。
  function runWithControl<T>(
    session: CoreBrowserSession, stillCurrent: () => boolean, operation: (lease: CoreBrowserLease) => Promise<T>,
    closesSession = false,
  ): Promise<T | undefined> {
    return enqueue(session, stillCurrent, async control => {
      const result = await operation(control.lease);
      if (closesSession) forgetControl(control);
      return result;
    });
  }

  // Captures current projected coordinates and the mutable operation's exact identity.
  // 捕获当前投影坐标及写入操作的精确身份。
  function captureInput(kind: string, payload: Record<string, unknown>): QueuedBrowserInput | null {
    const frame = frameRef.current;
    const pageId = selectedPageRef.current;
    if (!selectedSession || selectedSession.sessionId !== selectedSessionRef.current || !pageId || !frame
      || frame.pageId !== pageId || projectionSuspended.current) return null;
    return { session: selectedSession, pageId, documentId: frame.documentId, pageGeneration: frame.pageGeneration,
      viewportGeneration: frame.viewportGeneration, kind, payload };
  }

  // Dispatches one event in queue order. Only confirmed presses can produce matching releases.
  // 按队列顺序派发一条事件。只有确认送达的按下操作才会派发对应释放。
  async function sendInput(input: QueuedBrowserInput, passive = false): Promise<boolean> {
    const { session, pageId, kind, payload } = input;
    const release = (kind === "key" || kind === "pointer") && payload.type === "up";
    // Input captured before a wait must never be replayed on a different document or geometry.
    // 等待前捕获的输入绝不能在其他文档或几何位置重放。
    const isCurrent = () => {
      if (selectedSessionRef.current !== session.sessionId) return false;
      if (release) return true;
      const frame = frameRef.current;
      return !projectionSuspended.current && selectedPageRef.current === pageId && frame?.pageId === pageId
        && frame.documentId === input.documentId && frame.pageGeneration === input.pageGeneration
        && frame.viewportGeneration === input.viewportGeneration;
    };
    const key = kind === "key" && typeof payload.key === "string" ? payload.key : null;
    try {
      return (await enqueue(session, isCurrent, async control => {
        if (release && (key !== null ? !control.keys.has(key) : !control.pointerDown)) return false;
        // A broken response can leave a key or button pressed. End this gesture and release all inputs.
        // 响应中断可能使按键或按钮保持按下。结束本次手势并释放全部输入。
        let result: BrowserInputResult;
        try {
          result = await requestCoreBrowser<BrowserInputResult>(session.ownerSessionId, "browser.input.dispatch", {
            sessionId: session.sessionId, pageId, leaseSecret: control.lease.leaseSecret,
            pageGeneration: input.pageGeneration, viewportGeneration: input.viewportGeneration,
            sequence: String(++inputSequence.current), kind, payload,
          });
        } catch (failure) {
          if (active.current === control) resetControl();
          throw failure;
        }
        if (result.outcome !== "applied") {
          if (isCurrent() && currentOwner(session)) setError(result.reason ?? ("Input " + result.outcome));
          if (result.outcome === "unknown" || result.reason === "lease_required" || result.reason === "lease_blocked"
            || release) resetControl();
          return false;
        }
        if (key !== null && release) control.keys.delete(key);
        if (key !== null && payload.type === "down") control.keys.add(key);
        if (kind === "pointer" && payload.type === "down") control.pointerDown = true;
        if (kind === "pointer" && release) control.pointerDown = false;
        if (!passive && isCurrent() && currentOwner(session)) setError(null);
        return true;
      }, release ? "existing" : passive ? "idle" : "interaction")) === true;
    } catch (failure) {
      if (isCurrent() && currentOwner(session)) setError(errorMessage(failure));
      return false;
    }
  }

  // Coalesces high-frequency pointer positions before they enter the action queue.
  // 在高频指针位置进入操作队列前合并。
  function flushPendingPointerMove(): void {
    if (moveQueued.current || !pendingPointerMove.current) return;
    moveQueued.current = true;
    void tail.current.then(() => {
      const latest = pendingPointerMove.current;
      pendingPointerMove.current = null;
      if (latest) return sendInput(latest, true);
    }).finally(() => {
      moveQueued.current = false;
      flushPendingPointerMove();
    });
  }

  // Tracks drag motion and idle hover. Deliberate input can still enter while automation owns the page.
  // 跟踪拖拽移动与空闲悬停。自动化控制页面期间，仍可准入主动输入。
  function queuePointerMove(payload: Record<string, unknown>): void {
    if (!heldPointer.current && !active.current && selectedSession?.automationState === "attached") return;
    const input = captureInput("pointer", payload);
    if (!input) return;
    pendingPointerMove.current = input;
    flushPendingPointerMove();
  }

  // Flushes the last drag position before the next input and returns its applied status.
  // 在下一次输入前派发最后一个拖拽位置，返回该输入是否已生效。
  function queueInput(kind: string, payload: Record<string, unknown>): Promise<boolean> {
    const pending = pendingPointerMove.current;
    if (pending) {
      pendingPointerMove.current = null;
      void sendInput(pending, true);
    }
    const input = captureInput(kind, payload);
    return input ? sendInput(input) : Promise.resolve(false);
  }

  return { queueInput, queuePointerMove, runWithControl, resetControl };
}
