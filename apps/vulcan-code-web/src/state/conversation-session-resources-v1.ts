import React from "react";

import {
  sessionResourceClientV1,
  type SessionResourceRpcV1,
  type SessionResourceSummaryV1,
} from "@/service/conversation-session-resources-v1";
import type { ResourceId } from "@/service/conversation-event-window-store";

/** Immutable metadata snapshot for resources referenced by one selected V1 session window.
 * 一个选中 V1 会话窗口所引用资源的不可变元数据快照。 */
export interface ConversationSessionResourceCatalogV1 {
  /** Exact session owner or null when no durable session is selected.
   * 精确会话所有者；没有选中持久化会话时为 null。 */
  sessionId: string | null;
  /** Loading state for the current exact owner and resource set.
   * 当前精确所有者与资源集合的加载状态。 */
  status: "idle" | "loading" | "ready" | "failed";
  /** Metadata indexed only after the complete ordered response validates.
   * 仅在完整有序响应校验通过后建立索引的元数据。 */
  resources: ReadonlyMap<ResourceId, SessionResourceSummaryV1>;
  /** Explicit diagnostic for the current owner; stale owner errors are discarded.
   * 当前所有者的显式诊断；过期所有者错误会被丢弃。 */
  error: string | null;
  /** Builds one authenticated opaque content URL for the exact owner.
   * 为精确所有者构建一个经过鉴权的不透明内容 URL。 */
  contentUrl(resourceId: ResourceId): string;
}

/** Empty immutable resource index shared by ownerless snapshots.
 * 无所有者快照共享的空不可变资源索引。 */
const EMPTY_RESOURCE_INDEX: ReadonlyMap<ResourceId, SessionResourceSummaryV1> = new Map();

/** Resolves all unique resources referenced by one canonical V1 window as one owner-bound batch.
 * 将一个 canonical V1 窗口引用的全部唯一资源解析为一个绑定所有者的批次。
 *
 * @param sessionId exact selected V1 session identity.
 *                  精确选中 V1 会话身份。
 * @param resourceIds resource identities collected in canonical event order.
 *                    按 canonical 事件顺序收集的资源身份。
 * @param client injectable resource client used by production and tests.
 *               供生产与测试注入的资源客户端。
 * @returns one stale-safe immutable resource catalog.
 *          一个过期安全的不可变资源目录。 */
export function useConversationSessionResourcesV1(
  sessionId: string | null,
  resourceIds: readonly ResourceId[],
  client: SessionResourceRpcV1 = sessionResourceClientV1,
): ConversationSessionResourceCatalogV1 {
  // uniqueResourceIds preserves first occurrence order while bounding one metadata RPC.
  // uniqueResourceIds 保留首次出现顺序，同时将元数据请求限制为一次。
  const uniqueResourceIds = React.useMemo(() => {
    const seen = new Set<ResourceId>();
    const values: ResourceId[] = [];
    for (const currentResourceId of resourceIds) {
      if (!seen.has(currentResourceId)) {
        seen.add(currentResourceId);
        values.push(currentResourceId);
      }
    }
    return values;
  }, [resourceIds]);
  // requestIdentity changes only when owner or the exact unique ordered set changes.
  // requestIdentity 仅在所有者或精确唯一有序集合变化时变化。
  const requestIdentity = JSON.stringify([sessionId, uniqueResourceIds]);
  const [state, setState] = React.useState<{
    identity: string;
    status: ConversationSessionResourceCatalogV1["status"];
    resources: ReadonlyMap<ResourceId, SessionResourceSummaryV1>;
    error: string | null;
  }>(() => ({
    identity: requestIdentity,
    status: sessionId === null || uniqueResourceIds.length === 0 ? "ready" : "loading",
    resources: EMPTY_RESOURCE_INDEX,
    error: null,
  }));

  React.useEffect(() => {
    let active = true;
    if (sessionId === null || uniqueResourceIds.length === 0) {
      setState({
        identity: requestIdentity,
        status: "ready",
        resources: EMPTY_RESOURCE_INDEX,
        error: null,
      });
      return () => {
        active = false;
      };
    }
    setState({
      identity: requestIdentity,
      status: "loading",
      resources: EMPTY_RESOURCE_INDEX,
      error: null,
    });
    void client.list(sessionId, uniqueResourceIds).then(
      (response) => {
        if (!active) {
          return;
        }
        const resources = new Map<ResourceId, SessionResourceSummaryV1>();
        for (const summary of response.resources) {
          resources.set(summary.resource_id, summary);
        }
        setState({
          identity: requestIdentity,
          status: "ready",
          resources,
          error: null,
        });
      },
      (error: unknown) => {
        if (!active) {
          return;
        }
        setState({
          identity: requestIdentity,
          status: "failed",
          resources: EMPTY_RESOURCE_INDEX,
          error: error instanceof Error ? error.message : String(error),
        });
      },
    );
    return () => {
      active = false;
    };
  // requestIdentity fully represents sessionId and the ordered unique resource values.
  // requestIdentity 完整表示 sessionId 与有序唯一资源值，避免仅数组引用变化时重复加载。
  }, [client, requestIdentity]);

  // effectiveState hides the previous owner synchronously during React render before its effect runs.
  // effectiveState 在 effect 运行前的 React 渲染阶段同步隐藏上一所有者。
  const effectiveState = state.identity === requestIdentity
    ? state
    : {
      identity: requestIdentity,
      status: sessionId === null || uniqueResourceIds.length === 0 ? "ready" as const : "loading" as const,
      resources: EMPTY_RESOURCE_INDEX,
      error: null,
    };
  const contentUrl = React.useCallback((currentResourceId: ResourceId) => {
    if (sessionId === null) {
      throw new Error("Project resource content requires a selected V1 session owner.");
    }
    return client.contentUrl(sessionId, currentResourceId);
  }, [client, sessionId]);

  return {
    sessionId,
    status: effectiveState.status,
    resources: effectiveState.resources,
    error: effectiveState.error,
    contentUrl,
  };
}
