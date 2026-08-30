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
  /** Indexed parent session identity required by immutable embedded-task snapshots.
   * 不可变嵌入式任务快照所需的已索引父会话身份。 */
  parentSessionId: string | null;
  /** Whether this exact child still owns a live runtime handle and subscription.
   * 当前精确子会话是否仍持有实时运行时句柄与订阅。 */
  live: boolean;
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
 * @param options exact child identity, indexed parent identity, and live ownership state.
 *                精确子会话身份、已索引父会话身份与实时归属状态。
 * @returns the atomic snapshot, owner-bound commands, and loading state.
 *          原子快照、归属绑定命令与加载状态。 */
export function useConversationSessionEventWindowOwnerV1(
  options: ConversationSessionEventWindowOwnerV1Options,
): ConversationSessionEventWindowOwnerV1 {
  // eventWindowClient owns both ordinary live hydration and exact embedded snapshot reads.
  // eventWindowClient 同时持有普通实时 hydration 与精确嵌入式快照读取。
  const eventWindowClient = React.useMemo(
    () => createConversationEventWindowClient(serviceClient),
    [],
  );
  // controller is replaced when the exact visible session or its live/static ownership changes.
  // controller 在精确可见会话或其实时/静态归属变化时替换。
  const controller = React.useMemo(
    () => new ConversationEventWindowController(eventWindowClient),
    [eventWindowClient, options.live, options.sessionId],
  );
  // controllerOwnershipRef tracks the latest effect lease so React StrictMode replay cannot dispose a reused controller.
  // controllerOwnershipRef 跟踪最新 effect 租约，避免 React StrictMode 重放销毁仍被复用的控制器。
  const controllerOwnershipRef = React.useRef({
    controller,
    generation: 0,
  });
  if (controllerOwnershipRef.current.controller !== controller) {
    controllerOwnershipRef.current = {
      controller,
      generation: controllerOwnershipRef.current.generation,
    };
  }
  // snapshot is read through React's tear-free external-store contract.
  // snapshot 通过 React 无撕裂外部存储契约读取。
  const snapshot = useConversationEventWindowSnapshot(controller);
  // loading covers the first live hydration or one-shot persisted snapshot read.
  // loading 覆盖首次实时 hydration 或一次性持久化快照读取。
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
    rehydrate: async () => {
      if (!options.sessionId) {
        throw new Error("conversation event window owner has no durable session");
      }
      // Closed children are immutable one-shot snapshots; reopening the tab creates the next explicit read.
      // 已关闭子会话是不可变的一次性快照；重新打开页签才会创建下一次显式读取。
      return options.live ? controller.rehydrate(options.sessionId) : false;
    },
    prependOlder: () => options.live ? controller.prependOlder() : Promise.resolve(false),
  }), [controller, options.live, options.sessionId]);

  React.useEffect(() => {
    // generation identifies this exact effect lease for deferred disposal validation.
    // generation 标识本次精确 effect 租约，供延迟销毁校验使用。
    const generation = controllerOwnershipRef.current.generation + 1;
    controllerOwnershipRef.current = { controller, generation };
    return () => {
      // StrictMode immediately renews the same controller lease before this microtask executes.
      // StrictMode 会在本微任务执行前立即续租同一个控制器。
      queueMicrotask(() => {
        // latestOwnership is the authoritative owner after any replacement or StrictMode replay.
        // latestOwnership 是替换或 StrictMode 重放后的权威归属。
        const latestOwnership = controllerOwnershipRef.current;
        if (
          latestOwnership.controller !== controller
          || latestOwnership.generation === generation
        ) {
          controller.dispose();
        }
      });
    };
  }, [controller]);

  React.useEffect(() => {
    // active prevents a late preparation or hydration result from mutating replacement state.
    // active 防止迟到的准备或 hydration 结果修改替代状态。
    let active = true;
    hydratedRef.current = false;
    wasDisconnectedRef.current = false;
    setError(null);
    if (!options.sessionId) {
      setLoading(false);
      return () => {
        active = false;
      };
    }
    setLoading(true);
    const sessionId = options.sessionId;
    void (async () => {
      try {
        // Live children hydrate through their resident owner; closed children receive one persisted window only.
        // 实时子会话通过常驻归属 hydration；已关闭子会话仅接收一份持久化窗口。
        const hydrated = options.live
          ? await controller.hydrate(sessionId)
          : await (async () => {
            if (!options.parentSessionId) {
              throw new Error("closed subagent snapshot has no indexed parent session");
            }
            const response = await eventWindowClient.hydrateEmbeddedTaskSnapshot(
              options.parentSessionId,
              sessionId,
            );
            if (!active) return false;
            return controller.hydrateSnapshot(response);
          })();
        if (!active) return;
        if (!hydrated) {
          throw new Error("V1 session hydration was superseded before publication.");
        }
        hydratedRef.current = options.live;
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
    };
  }, [controller, eventWindowClient, options.live, options.parentSessionId, options.sessionId]);

  React.useEffect(() => {
    // active binds asynchronous reconnect failures to this exact controller effect, not its replacement.
    // active 将异步重连失败绑定到本次精确控制器 effect，而不是其替代页面。
    let active = true;
    /** Recovers only after one real disconnect and one established owner baseline.
     * 仅在真实断链且归属基线已经建立后执行恢复。 */
    const handleConnectionStatus = (event: Event): void => {
      const detail = (event as CustomEvent<BrowserServiceConnectionDetail>).detail;
      if (detail.status === "disconnected" || detail.status === "reconnecting") {
        wasDisconnectedRef.current = true;
        return;
      }
      if (!options.live || !wasDisconnectedRef.current || !hydratedRef.current) return;
      wasDisconnectedRef.current = false;
      void controller.recoverConnection(false).catch((recoveryError) => {
        if (active) {
          setError(recoveryError instanceof Error ? recoveryError.message : String(recoveryError));
        }
      });
    };
    window.addEventListener(BROWSER_SERVICE_CONNECTION_EVENT, handleConnectionStatus);
    return () => {
      active = false;
      window.removeEventListener(BROWSER_SERVICE_CONNECTION_EVENT, handleConnectionStatus);
    };
  }, [controller, options.live]);

  return { snapshot, commands, loading, error };
}
