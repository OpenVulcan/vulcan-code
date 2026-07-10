import React from "react";

// WorkspaceMemoryOwnedRequest identifies one asynchronous memory-panel operation and its originating route.
// WorkspaceMemoryOwnedRequest 标识一次异步记忆面板操作及其发起路由。
export interface WorkspaceMemoryOwnedRequest {
  // Route key for the selected conversation and its VMM project scope.
  // 当前选中会话及其 VMM 项目作用域对应的路由键。
  routeKey: string;
  // Monotonic request id unique to this mounted ownership controller.
  // 当前挂载所有权控制器内唯一的单调请求标识。
  requestId: number;
}

// WorkspaceMemoryRequestOwnership controls destructive mutations and latest-wins turn lookups.
// WorkspaceMemoryRequestOwnership 控制破坏性变更与仅最新结果生效的回合查询。
export interface WorkspaceMemoryRequestOwnership {
  // Starts one exclusive delete or suppression request for the committed memory route.
  // 为已提交的记忆路由启动一次独占删除或抑制请求。
  beginMutationRequest: () => WorkspaceMemoryOwnedRequest | null;
  // Returns whether a mutation may still update the visible memory route.
  // 返回一次变更是否仍可更新当前可见记忆路由。
  ownsMutationRequest: (request: WorkspaceMemoryOwnedRequest) => boolean;
  // Releases the exact mutation that still owns the visible busy state.
  // 释放仍拥有可见忙碌状态的准确变更请求。
  finishMutationRequest: (request: WorkspaceMemoryOwnedRequest) => boolean;
  // Starts a turn lookup and replaces ownership held by any older lookup.
  // 启动一次回合查询，并替换任何旧查询持有的所有权。
  beginLookupRequest: () => WorkspaceMemoryOwnedRequest | null;
  // Returns whether a lookup is still the latest request for the visible route.
  // 返回一次查询是否仍是当前可见路由的最新请求。
  ownsLookupRequest: (request: WorkspaceMemoryOwnedRequest) => boolean;
  // Releases the exact lookup that still owns the visible dialog result.
  // 释放仍拥有可见弹窗结果的准确查询请求。
  finishLookupRequest: (request: WorkspaceMemoryOwnedRequest) => boolean;
  // Invalidates the current lookup when its dialog is explicitly closed.
  // 查询弹窗被明确关闭时使当前查询失效。
  invalidateLookupRequest: () => void;
}

// workspaceMemoryPanelRouteKey builds the state boundary for one selected conversation and project scope.
// workspaceMemoryPanelRouteKey 为一个选中会话及项目作用域构建状态边界。
// Parameters: conversationId is the selected persisted conversation; projectId is its resolved VMM project scope.
// 参数：conversationId 是选中的已持久化会话；projectId 是其解析后的 VMM 项目作用域。
// Returns: a deterministic key that preserves null values without ambiguous string concatenation.
// 返回值：保留空值且不存在模糊字符串拼接的确定性键。
export function workspaceMemoryPanelRouteKey(
  conversationId: string | null,
  projectId: string | null,
): string {
  return JSON.stringify([conversationId, projectId]);
}

// useWorkspaceMemoryRequestOwnership binds every async continuation to the committed memory-panel route.
// useWorkspaceMemoryRequestOwnership 将每个异步延续绑定到已提交的记忆面板路由。
// Parameters: routeKey identifies the selected conversation and VMM project scope currently rendered by the panel.
// 参数：routeKey 标识面板当前渲染的选中会话与 VMM 项目作用域。
// Returns: ownership operations for exclusive mutations and latest-wins lookups.
// 返回值：用于独占变更和仅最新查询生效的所有权操作。
export function useWorkspaceMemoryRequestOwnership(routeKey: string): WorkspaceMemoryRequestOwnership {
  // activeRouteKeyRef records the memory route whose render has committed.
  // activeRouteKeyRef 记录渲染已经提交的记忆路由。
  const activeRouteKeyRef = React.useRef<string | null>(routeKey);
  // nextRequestIdRef provides collision-free request ids across route changes.
  // nextRequestIdRef 在路由切换之间提供无冲突请求标识。
  const nextRequestIdRef = React.useRef(0);
  // activeMutationRequestRef owns the only destructive operation allowed to update visible state.
  // activeMutationRequestRef 持有唯一允许更新可见状态的破坏性操作。
  const activeMutationRequestRef = React.useRef<WorkspaceMemoryOwnedRequest | null>(null);
  // activeLookupRequestRef owns the latest turn lookup allowed to update the lookup dialog.
  // activeLookupRequestRef 持有允许更新查询弹窗的最新回合查询。
  const activeLookupRequestRef = React.useRef<WorkspaceMemoryOwnedRequest | null>(null);

  React.useLayoutEffect(() => {
    // A committed route change invalidates every writer created for the previous conversation.
    // 已提交的路由切换会使先前会话创建的全部写入者失效。
    activeRouteKeyRef.current = routeKey;
    activeMutationRequestRef.current = null;
    activeLookupRequestRef.current = null;
    return () => {
      if (activeRouteKeyRef.current === routeKey) {
        activeRouteKeyRef.current = null;
        activeMutationRequestRef.current = null;
        activeLookupRequestRef.current = null;
      }
    };
  }, [routeKey]);

  // createOwnedRequest creates one immutable token only for the currently committed route.
  // createOwnedRequest 仅为当前已提交路由创建一个不可变令牌。
  const createOwnedRequest = React.useCallback((): WorkspaceMemoryOwnedRequest | null => {
    if (activeRouteKeyRef.current !== routeKey) {
      return null;
    }
    // request is captured by each asynchronous continuation that must prove state ownership.
    // request 由每个必须证明状态所有权的异步延续逻辑捕获。
    const request = {
      routeKey,
      requestId: nextRequestIdRef.current + 1,
    };
    nextRequestIdRef.current = request.requestId;
    return request;
  }, [routeKey]);

  // beginMutationRequest synchronously excludes duplicate destructive operations before React state commits.
  // beginMutationRequest 在 React 状态提交前同步排除重复的破坏性操作。
  const beginMutationRequest = React.useCallback((): WorkspaceMemoryOwnedRequest | null => {
    if (activeMutationRequestRef.current) {
      return null;
    }
    // request becomes the exclusive mutation owner for this route.
    // request 成为该路由的独占变更所有者。
    const request = createOwnedRequest();
    if (request) {
      activeMutationRequestRef.current = request;
    }
    return request;
  }, [createOwnedRequest]);

  // ownsMutationRequest verifies both the originating route and exclusive mutation identity.
  // ownsMutationRequest 同时校验发起路由与独占变更身份。
  const ownsMutationRequest = React.useCallback((request: WorkspaceMemoryOwnedRequest): boolean => {
    return activeRouteKeyRef.current === request.routeKey
      && activeMutationRequestRef.current?.requestId === request.requestId;
  }, []);

  // finishMutationRequest releases only the mutation that still owns the busy state.
  // finishMutationRequest 仅释放仍拥有忙碌状态的变更请求。
  const finishMutationRequest = React.useCallback((request: WorkspaceMemoryOwnedRequest): boolean => {
    if (activeRouteKeyRef.current !== request.routeKey
      || activeMutationRequestRef.current?.requestId !== request.requestId) {
      return false;
    }
    activeMutationRequestRef.current = null;
    return true;
  }, []);

  // beginLookupRequest replaces the previous lookup so slower responses cannot overwrite newer selections.
  // beginLookupRequest 替换先前查询，避免较慢响应覆盖较新的选择。
  const beginLookupRequest = React.useCallback((): WorkspaceMemoryOwnedRequest | null => {
    // request becomes the latest lookup owner for this route.
    // request 成为该路由的最新查询所有者。
    const request = createOwnedRequest();
    if (request) {
      activeLookupRequestRef.current = request;
    }
    return request;
  }, [createOwnedRequest]);

  // ownsLookupRequest verifies both the originating route and latest lookup identity.
  // ownsLookupRequest 同时校验发起路由与最新查询身份。
  const ownsLookupRequest = React.useCallback((request: WorkspaceMemoryOwnedRequest): boolean => {
    return activeRouteKeyRef.current === request.routeKey
      && activeLookupRequestRef.current?.requestId === request.requestId;
  }, []);

  // finishLookupRequest releases only the lookup that still owns the dialog result.
  // finishLookupRequest 仅释放仍拥有弹窗结果的查询请求。
  const finishLookupRequest = React.useCallback((request: WorkspaceMemoryOwnedRequest): boolean => {
    if (activeRouteKeyRef.current !== request.routeKey
      || activeLookupRequestRef.current?.requestId !== request.requestId) {
      return false;
    }
    activeLookupRequestRef.current = null;
    return true;
  }, []);

  // invalidateLookupRequest prevents a closed dialog from being reopened by its pending response.
  // invalidateLookupRequest 防止已关闭弹窗被其尚未返回的响应重新打开。
  const invalidateLookupRequest = React.useCallback(() => {
    activeLookupRequestRef.current = null;
  }, []);

  // ownership keeps the public controller identity stable while its operations remain unchanged.
  // ownership 在操作保持不变时维持公共控制器身份稳定。
  const ownership = React.useMemo<WorkspaceMemoryRequestOwnership>(() => ({
    beginMutationRequest,
    ownsMutationRequest,
    finishMutationRequest,
    beginLookupRequest,
    ownsLookupRequest,
    finishLookupRequest,
    invalidateLookupRequest,
  }), [
    beginLookupRequest,
    beginMutationRequest,
    finishLookupRequest,
    finishMutationRequest,
    invalidateLookupRequest,
    ownsLookupRequest,
    ownsMutationRequest,
  ]);
  return ownership;
}
