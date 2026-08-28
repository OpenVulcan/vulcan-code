import * as React from "react";

import {
  BROWSER_SERVICE_CONNECTION_EVENT,
  type BrowserServiceConnectionDetail,
} from "@/service";
import { createConversationEventWindowClient } from "@/service/conversation-event-window-client";
import {
  ConversationEventWindowController,
  type ConversationEventWindowControllerSnapshot,
} from "@/service/conversation-event-window-controller";
import type { SelectedConversationEventWindowCommands } from "@/service/selected-conversation-event-window-registry";
import { serviceClient } from "@/service/webui-service-runtime";
import { useConversationEventWindowSnapshot } from "@/state/conversation-event-window-react";

/** Inputs for one component-owned durable V1 session event window.
 * 单个组件持有的可靠 V1 会话事件窗口输入。 */
export interface ConversationSessionEventWindowOwnerV1Options {
  /** Exact session identity, or null while the route is incomplete.
   * 精确会话身份；路由不完整时为空。 */
  sessionId: string | null;
  /** Resolves the exact backend session locator before hydration.
   * 在 hydration 前解析精确后端会话定位器。 */
  prepare?: (() => Promise<void>) | null;
}

/** React state exposed by one component-owned V1 event-window controller.
 * 单个组件持有的 V1 事件窗口控制器暴露的 React 状态。 */
export interface ConversationSessionEventWindowOwnerV1 {
  /** Atomic controller snapshot for the exact session owner.
   * 精确会话归属的原子控制器快照。 */
  snapshot: ConversationEventWindowControllerSnapshot;
  /** Commands permanently bound to this controller instance.
   * 永久绑定到当前控制器实例的命令。 */
  commands: SelectedConversationEventWindowCommands;
  /** Whether locator preparation or initial hydration is still running.
   * 定位准备或首次 hydration 是否仍在执行。 */
  loading: boolean;
  /** Exact preparation, hydration, or reconnect failure text.
   * 精确的准备、hydration 或重连失败文本。 */
  error: string | null;
}

/** Owns one independent V1 event window for a non-selected visible session.
 * 为一个非当前选中但可见的会话持有独立 V1 事件窗口。
 *
 * @param options exact session identity and optional locator preparation.
 *                精确会话身份与可选定位准备。
 * @returns the atomic snapshot, owner-bound commands, and loading state.
 *          原子快照、归属绑定命令与加载状态。 */
export function useConversationSessionEventWindowOwnerV1(
  options: ConversationSessionEventWindowOwnerV1Options,
): ConversationSessionEventWindowOwnerV1 {
  // controller is replaced only when the exact visible session identity changes.
  // controller 仅在精确可见会话身份变化时替换。
  const controller = React.useMemo(
    () => new ConversationEventWindowController(createConversationEventWindowClient(serviceClient)),
    [options.sessionId],
  );
  // snapshot is read through React's tear-free external-store contract.
  // snapshot 通过 React 无撕裂外部存储契约读取。
  const snapshot = useConversationEventWindowSnapshot(controller);
  // loading covers preparation and the first authoritative hydration as one boundary.
  // loading 将准备与首次权威 hydration 作为一个边界覆盖。
  const [loading, setLoading] = React.useState(Boolean(options.sessionId));
  // error preserves the latest exact owner failure without falling back to legacy messages.
  // error 保留最近一次精确归属失败，不回退到旧消息。
  const [error, setError] = React.useState<string | null>(null);
  // hydratedRef prevents reconnect recovery before an owner baseline exists.
  // hydratedRef 防止在归属基线建立前执行重连恢复。
  const hydratedRef = React.useRef(false);
  // wasDisconnectedRef distinguishes a real recovery from the initial connected event.
  // wasDisconnectedRef 区分真实恢复与首次 connected 事件。
  const wasDisconnectedRef = React.useRef(false);
  // commands never retarget after the session identity changes.
  // commands 在会话身份变化后绝不重定向。
  const commands = React.useMemo<SelectedConversationEventWindowCommands>(() => ({
    rehydrate: () => options.sessionId
      ? controller.hydrate(options.sessionId)
      : Promise.reject(new Error("conversation event window owner has no durable session")),
    prependOlder: () => controller.prependOlder(),
  }), [controller, options.sessionId]);

  React.useEffect(() => {
    // active prevents a late preparation or hydration result from mutating replacement state.
    // active 防止迟到的准备或 hydration 结果修改替代状态。
    let active = true;
    hydratedRef.current = false;
    wasDisconnectedRef.current = false;
    setError(null);
    if (!options.sessionId) {
      setLoading(false);
      return () => controller.dispose();
    }
    setLoading(true);
    const sessionId = options.sessionId;
    void (async () => {
      try {
        await options.prepare?.();
        if (!active) return;
        const hydrated = await controller.hydrate(sessionId);
        if (!active) return;
        if (!hydrated) {
          throw new Error("V1 session hydration was superseded before publication.");
        }
        hydratedRef.current = true;
      } catch (ownerError) {
        if (active) {
          setError(ownerError instanceof Error ? ownerError.message : String(ownerError));
        }
      } finally {
        if (active) setLoading(false);
      }
    })();
    return () => {
      active = false;
      controller.dispose();
    };
  }, [controller, options.prepare, options.sessionId]);

  React.useEffect(() => {
    /** Recovers only after one real disconnect and one established owner baseline.
     * 仅在真实断链且归属基线已经建立后执行恢复。 */
    const handleConnectionStatus = (event: Event): void => {
      const detail = (event as CustomEvent<BrowserServiceConnectionDetail>).detail;
      if (detail.status === "disconnected" || detail.status === "reconnecting") {
        wasDisconnectedRef.current = true;
        return;
      }
      if (!wasDisconnectedRef.current || !hydratedRef.current) return;
      wasDisconnectedRef.current = false;
      void controller.recoverConnection(false).catch((recoveryError) => {
        setError(recoveryError instanceof Error ? recoveryError.message : String(recoveryError));
      });
    };
    window.addEventListener(BROWSER_SERVICE_CONNECTION_EVENT, handleConnectionStatus);
    return () => window.removeEventListener(BROWSER_SERVICE_CONNECTION_EVENT, handleConnectionStatus);
  }, [controller]);

  return { snapshot, commands, loading, error };
}
