import React from "react";

// WorkspaceReviewOwnedRequest identifies one async request inside a committed project review route.
// WorkspaceReviewOwnedRequest 标识已提交项目审核路由内的一次异步请求。
export interface WorkspaceReviewOwnedRequest {
  // Project-level review route that created the request.
  // 创建该请求的项目级审核路由。
  routeKey: string;
  // Monotonic request id unique to this mounted controller.
  // 当前挂载控制器内唯一的单调请求标识。
  requestId: number;
}

// WorkspaceReviewRequestOwnership controls refresh, file-detail, and Git-mutation result ownership.
// WorkspaceReviewRequestOwnership 控制刷新、文件详情与 Git 变更的结果所有权。
export interface WorkspaceReviewRequestOwnership {
  // Starts the latest full diff refresh for the active project route.
  // 为当前项目路由启动最新一次完整 diff 刷新。
  beginRefreshRequest: () => WorkspaceReviewOwnedRequest | null;
  // Returns whether a full diff refresh may still update the active project route.
  // 返回完整 diff 刷新是否仍可更新当前项目路由。
  ownsRefreshRequest: (request: WorkspaceReviewOwnedRequest) => boolean;
  // Releases the active full diff refresh and reports whether loading may be cleared.
  // 释放当前完整 diff 刷新，并报告是否可以清除加载状态。
  finishRefreshRequest: (request: WorkspaceReviewOwnedRequest) => boolean;
  // Invalidates the active full diff refresh after its base Git state or source becomes obsolete.
  // 基础 Git 状态或来源失效后，使当前完整 diff 刷新失效。
  invalidateRefreshRequest: () => void;
  // Starts one file-detail request or returns null when that detail is already loading.
  // 启动一次文件详情请求；该详情已在加载时返回 null。
  beginDetailRequest: (detailKey: string) => WorkspaceReviewOwnedRequest | null;
  // Returns whether a file-detail request may still update the active project snapshot.
  // 返回文件详情请求是否仍可更新当前项目快照。
  ownsDetailRequest: (detailKey: string, request: WorkspaceReviewOwnedRequest) => boolean;
  // Releases the exact file-detail request that still owns its detail key.
  // 释放仍拥有对应详情键的准确文件详情请求。
  finishDetailRequest: (detailKey: string, request: WorkspaceReviewOwnedRequest) => boolean;
  // Invalidates every file-detail request after the base snapshot or selection changes.
  // 基础快照或选择变化后使全部文件详情请求失效。
  invalidateDetailRequests: () => void;
  // Starts one exclusive Git mutation for the active project route.
  // 为当前项目路由启动一次独占 Git 变更。
  beginMutationRequest: () => WorkspaceReviewOwnedRequest | null;
  // Returns whether a Git mutation may still update the active review panel.
  // 返回 Git 变更是否仍可更新当前审核面板。
  ownsMutationRequest: (request: WorkspaceReviewOwnedRequest) => boolean;
  // Releases the active Git mutation and reports whether pending state may be cleared.
  // 释放当前 Git 变更，并报告是否可以清除 pending 状态。
  finishMutationRequest: (request: WorkspaceReviewOwnedRequest) => boolean;
}

// useWorkspaceReviewRequestOwnership binds review async results to the committed project route.
// useWorkspaceReviewRequestOwnership 将审核异步结果绑定到已提交的项目路由。
// Parameters: routeKey identifies the project-level review cache bucket currently represented by the panel.
// 参数：routeKey 标识面板当前代表的项目级审核缓存桶。
// Returns: ownership operations for latest refresh, keyed details, and exclusive Git mutation requests.
// 返回值：完整刷新、逐键详情和独占 Git 变更请求的所有权操作。
export function useWorkspaceReviewRequestOwnership(routeKey: string): WorkspaceReviewRequestOwnership {
  // activeRouteKeyRef records the latest project route whose render has committed.
  // activeRouteKeyRef 记录最近一次已提交渲染所属的项目路由。
  const activeRouteKeyRef = React.useRef(routeKey);
  // nextRequestIdRef provides collision-free ownership ids across route changes.
  // nextRequestIdRef 在路由变化之间提供无冲突的所有权标识。
  const nextRequestIdRef = React.useRef(0);
  // activeRefreshRequestRef owns the only full refresh allowed to update the active route.
  // activeRefreshRequestRef 持有唯一允许更新当前路由的完整刷新。
  const activeRefreshRequestRef = React.useRef<WorkspaceReviewOwnedRequest | null>(null);
  // detailRequestsByKeyRef owns at most one request for each lazy file detail.
  // detailRequestsByKeyRef 为每个懒加载文件详情最多持有一个请求。
  const detailRequestsByKeyRef = React.useRef(new Map<string, WorkspaceReviewOwnedRequest>());
  // activeMutationRequestRef owns the only Git mutation allowed to update panel state.
  // activeMutationRequestRef 持有唯一允许更新面板状态的 Git 变更。
  const activeMutationRequestRef = React.useRef<WorkspaceReviewOwnedRequest | null>(null);

  React.useLayoutEffect(() => {
    // A committed project change invalidates every writer from the previous review route.
    // 已提交项目变化会使先前审核路由的全部写入者失效。
    activeRouteKeyRef.current = routeKey;
    activeRefreshRequestRef.current = null;
    detailRequestsByKeyRef.current.clear();
    activeMutationRequestRef.current = null;
  }, [routeKey]);

  // createOwnedRequest creates one immutable token only for the currently committed route.
  // createOwnedRequest 仅为当前已提交路由创建一个不可变令牌。
  const createOwnedRequest = React.useCallback((): WorkspaceReviewOwnedRequest | null => {
    if (activeRouteKeyRef.current !== routeKey) {
      return null;
    }
    // request is captured by every async continuation that needs to prove state ownership.
    // request 由所有需要证明状态所有权的异步延续逻辑捕获。
    const request = {
      routeKey,
      requestId: nextRequestIdRef.current + 1,
    };
    nextRequestIdRef.current = request.requestId;
    return request;
  }, [routeKey]);

  // beginRefreshRequest replaces refresh ownership so only the latest full snapshot may win.
  // beginRefreshRequest 替换刷新所有权，确保只有最新完整快照可以生效。
  const beginRefreshRequest = React.useCallback((): WorkspaceReviewOwnedRequest | null => {
    // request is the new latest full-refresh token.
    // request 是新的最新完整刷新令牌。
    const request = createOwnedRequest();
    if (request) {
      activeRefreshRequestRef.current = request;
    }
    return request;
  }, [createOwnedRequest]);

  // ownsRefreshRequest verifies route and latest-refresh identity.
  // ownsRefreshRequest 校验路由与最新刷新身份。
  const ownsRefreshRequest = React.useCallback((request: WorkspaceReviewOwnedRequest): boolean => {
    return activeRouteKeyRef.current === request.routeKey
      && activeRefreshRequestRef.current?.requestId === request.requestId;
  }, []);

  // finishRefreshRequest releases only the refresh that still owns loading state.
  // finishRefreshRequest 仅释放仍拥有加载状态的刷新。
  const finishRefreshRequest = React.useCallback((request: WorkspaceReviewOwnedRequest): boolean => {
    if (activeRouteKeyRef.current !== request.routeKey
      || activeRefreshRequestRef.current?.requestId !== request.requestId) {
      return false;
    }
    activeRefreshRequestRef.current = null;
    return true;
  }, []);

  // invalidateRefreshRequest removes the current full-refresh writer without affecting other request classes.
  // invalidateRefreshRequest 移除当前完整刷新写入者，且不影响其他请求类别。
  const invalidateRefreshRequest = React.useCallback(() => {
    activeRefreshRequestRef.current = null;
  }, []);

  // beginDetailRequest claims one lazy file-detail key in the active route.
  // beginDetailRequest 在当前路由中认领一个懒加载文件详情键。
  const beginDetailRequest = React.useCallback((detailKey: string): WorkspaceReviewOwnedRequest | null => {
    if (detailRequestsByKeyRef.current.has(detailKey)) {
      return null;
    }
    // request is the new owner of this lazy file-detail key.
    // request 是该懒加载文件详情键的新拥有者。
    const request = createOwnedRequest();
    if (request) {
      detailRequestsByKeyRef.current.set(detailKey, request);
    }
    return request;
  }, [createOwnedRequest]);

  // ownsDetailRequest verifies route and per-detail identity.
  // ownsDetailRequest 校验路由与逐详情身份。
  const ownsDetailRequest = React.useCallback((
    detailKey: string,
    request: WorkspaceReviewOwnedRequest,
  ): boolean => {
    return activeRouteKeyRef.current === request.routeKey
      && detailRequestsByKeyRef.current.get(detailKey)?.requestId === request.requestId;
  }, []);

  // finishDetailRequest releases only the request that still owns its detail key.
  // finishDetailRequest 仅释放仍拥有对应详情键的请求。
  const finishDetailRequest = React.useCallback((
    detailKey: string,
    request: WorkspaceReviewOwnedRequest,
  ): boolean => {
    if (activeRouteKeyRef.current !== request.routeKey
      || detailRequestsByKeyRef.current.get(detailKey)?.requestId !== request.requestId) {
      return false;
    }
    detailRequestsByKeyRef.current.delete(detailKey);
    return true;
  }, []);

  // invalidateDetailRequests removes all lazy-detail writers after their base snapshot becomes obsolete.
  // invalidateDetailRequests 在基础快照失效后移除全部懒加载详情写入者。
  const invalidateDetailRequests = React.useCallback(() => {
    detailRequestsByKeyRef.current.clear();
  }, []);

  // beginMutationRequest synchronously gates duplicate Git mutations for the active route.
  // beginMutationRequest 为当前路由同步拦截重复 Git 变更。
  const beginMutationRequest = React.useCallback((): WorkspaceReviewOwnedRequest | null => {
    if (activeMutationRequestRef.current) {
      return null;
    }
    // request is the exclusive Git mutation token for the active project route.
    // request 是当前项目路由的独占 Git 变更令牌。
    const request = createOwnedRequest();
    if (request) {
      activeMutationRequestRef.current = request;
    }
    return request;
  }, [createOwnedRequest]);

  // ownsMutationRequest verifies route and exclusive mutation identity.
  // ownsMutationRequest 校验路由与独占变更身份。
  const ownsMutationRequest = React.useCallback((request: WorkspaceReviewOwnedRequest): boolean => {
    return activeRouteKeyRef.current === request.routeKey
      && activeMutationRequestRef.current?.requestId === request.requestId;
  }, []);

  // finishMutationRequest releases only the mutation that still owns pending state.
  // finishMutationRequest 仅释放仍拥有 pending 状态的变更。
  const finishMutationRequest = React.useCallback((request: WorkspaceReviewOwnedRequest): boolean => {
    if (activeRouteKeyRef.current !== request.routeKey
      || activeMutationRequestRef.current?.requestId !== request.requestId) {
      return false;
    }
    activeMutationRequestRef.current = null;
    return true;
  }, []);

  return React.useMemo(() => ({
    beginDetailRequest,
    beginMutationRequest,
    beginRefreshRequest,
    finishDetailRequest,
    finishMutationRequest,
    finishRefreshRequest,
    invalidateDetailRequests,
    invalidateRefreshRequest,
    ownsDetailRequest,
    ownsMutationRequest,
    ownsRefreshRequest,
  }), [
    beginDetailRequest,
    beginMutationRequest,
    beginRefreshRequest,
    finishDetailRequest,
    finishMutationRequest,
    finishRefreshRequest,
    invalidateDetailRequests,
    invalidateRefreshRequest,
    ownsDetailRequest,
    ownsMutationRequest,
    ownsRefreshRequest,
  ]);
}
