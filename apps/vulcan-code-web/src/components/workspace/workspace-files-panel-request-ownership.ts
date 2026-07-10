import React from "react";

// WorkspaceFilesOwnedRequest identifies one async request inside a committed file-panel scope.
// WorkspaceFilesOwnedRequest 标识已提交文件面板作用域内的一次异步请求。
export interface WorkspaceFilesOwnedRequest {
  // Monotonic request id unique to this mounted controller.
  // 当前挂载控制器内唯一的单调请求标识。
  requestId: number;
  // File-panel route and filter scope that created the request.
  // 创建该请求的文件面板路由与过滤作用域。
  scopeKey: string;
}

// WorkspaceFilesRequestOwnership controls result ownership for directory and preview requests.
// WorkspaceFilesRequestOwnership 控制目录与预览请求的结果所有权。
export interface WorkspaceFilesRequestOwnership {
  // Starts one directory request or returns null when the same directory is already loading in this scope.
  // 启动一次目录请求；同一作用域内该目录已在加载时返回 null。
  beginDirectoryRequest: (directoryKey: string) => WorkspaceFilesOwnedRequest | null;
  // Returns whether a directory request may still mutate the committed panel state.
  // 返回目录请求是否仍可修改已提交的面板状态。
  ownsDirectoryRequest: (directoryKey: string, request: WorkspaceFilesOwnedRequest) => boolean;
  // Releases a current directory request and returns whether its loading marker should be cleared.
  // 释放当前目录请求，并返回是否应清除其加载标记。
  finishDirectoryRequest: (directoryKey: string, request: WorkspaceFilesOwnedRequest) => boolean;
  // Starts a preview request and invalidates any older preview request in the same scope.
  // 启动预览请求，并使同一作用域内更早的预览请求失效。
  beginPreviewRequest: () => WorkspaceFilesOwnedRequest | null;
  // Returns whether a preview request may still mutate the committed panel state.
  // 返回预览请求是否仍可修改已提交的面板状态。
  ownsPreviewRequest: (request: WorkspaceFilesOwnedRequest) => boolean;
  // Releases a current preview request and returns whether its loading marker should be cleared.
  // 释放当前预览请求，并返回是否应清除其加载标记。
  finishPreviewRequest: (request: WorkspaceFilesOwnedRequest) => boolean;
  // Invalidates the active preview request after selection or route state is cleared locally.
  // 本地清除选择或路由状态后，使当前预览请求失效。
  invalidatePreviewRequest: () => void;
}

// useWorkspaceFilesRequestOwnership binds async file results to the committed route and filter scope.
// useWorkspaceFilesRequestOwnership 将异步文件结果绑定到已提交的路由与过滤作用域。
// Parameters: scopeKey identifies one cache route plus the backend directory-filter fingerprint.
// 参数：scopeKey 标识一个缓存路由及其后端目录过滤指纹。
// Returns: request ownership operations for coalescing directories and enforcing latest-preview wins.
// 返回值：用于合并目录请求并强制最新预览生效的请求所有权操作。
export function useWorkspaceFilesRequestOwnership(scopeKey: string): WorkspaceFilesRequestOwnership {
  // activeScopeKeyRef records the latest scope whose render has committed.
  // activeScopeKeyRef 记录最近一次已提交渲染所属的作用域。
  const activeScopeKeyRef = React.useRef(scopeKey);
  // nextRequestIdRef provides collision-free ownership ids across scope changes.
  // nextRequestIdRef 在作用域变化之间提供无冲突的所有权标识。
  const nextRequestIdRef = React.useRef(0);
  // directoryRequestsByKeyRef owns at most one request for each directory in the active scope.
  // directoryRequestsByKeyRef 在当前作用域内为每个目录最多持有一个请求。
  const directoryRequestsByKeyRef = React.useRef(new Map<string, WorkspaceFilesOwnedRequest>());
  // activePreviewRequestRef owns the only preview result currently allowed to update the panel.
  // activePreviewRequestRef 持有当前唯一允许更新面板的预览结果。
  const activePreviewRequestRef = React.useRef<WorkspaceFilesOwnedRequest | null>(null);

  React.useLayoutEffect(() => {
    // A committed scope change invalidates every old async writer before passive hydration effects run.
    // 已提交作用域变化会在被动水合副作用运行前使全部旧异步写入者失效。
    activeScopeKeyRef.current = scopeKey;
    directoryRequestsByKeyRef.current.clear();
    activePreviewRequestRef.current = null;
  }, [scopeKey]);

  // beginDirectoryRequest claims one directory key in the current committed scope.
  // beginDirectoryRequest 在当前已提交作用域中认领一个目录键。
  const beginDirectoryRequest = React.useCallback((directoryKey: string): WorkspaceFilesOwnedRequest | null => {
    if (activeScopeKeyRef.current !== scopeKey || directoryRequestsByKeyRef.current.has(directoryKey)) {
      return null;
    }
    // request is the immutable ownership token captured by the async directory continuation.
    // request 是异步目录延续逻辑捕获的不可变所有权令牌。
    const request = {
      requestId: nextRequestIdRef.current + 1,
      scopeKey,
    };
    nextRequestIdRef.current = request.requestId;
    directoryRequestsByKeyRef.current.set(directoryKey, request);
    return request;
  }, [scopeKey]);

  // ownsDirectoryRequest verifies both committed scope and per-directory request identity.
  // ownsDirectoryRequest 同时校验已提交作用域与逐目录请求身份。
  const ownsDirectoryRequest = React.useCallback((
    directoryKey: string,
    request: WorkspaceFilesOwnedRequest,
  ): boolean => {
    return activeScopeKeyRef.current === request.scopeKey
      && directoryRequestsByKeyRef.current.get(directoryKey)?.requestId === request.requestId;
  }, []);

  // finishDirectoryRequest releases only the exact request that still owns its directory key.
  // finishDirectoryRequest 仅释放仍拥有对应目录键的准确请求。
  const finishDirectoryRequest = React.useCallback((
    directoryKey: string,
    request: WorkspaceFilesOwnedRequest,
  ): boolean => {
    if (activeScopeKeyRef.current !== request.scopeKey
      || directoryRequestsByKeyRef.current.get(directoryKey)?.requestId !== request.requestId) {
      return false;
    }
    directoryRequestsByKeyRef.current.delete(directoryKey);
    return true;
  }, []);

  // beginPreviewRequest replaces preview ownership so slower prior selections cannot overwrite the latest file.
  // beginPreviewRequest 替换预览所有权，避免更慢的旧选择覆盖最新文件。
  const beginPreviewRequest = React.useCallback((): WorkspaceFilesOwnedRequest | null => {
    if (activeScopeKeyRef.current !== scopeKey) {
      return null;
    }
    // request is the immutable ownership token captured by the async preview continuation.
    // request 是异步预览延续逻辑捕获的不可变所有权令牌。
    const request = {
      requestId: nextRequestIdRef.current + 1,
      scopeKey,
    };
    nextRequestIdRef.current = request.requestId;
    activePreviewRequestRef.current = request;
    return request;
  }, [scopeKey]);

  // ownsPreviewRequest verifies that one preview is still the active writer for the committed scope.
  // ownsPreviewRequest 校验预览是否仍是已提交作用域的当前写入者。
  const ownsPreviewRequest = React.useCallback((request: WorkspaceFilesOwnedRequest): boolean => {
    return activeScopeKeyRef.current === request.scopeKey
      && activePreviewRequestRef.current?.requestId === request.requestId;
  }, []);

  // finishPreviewRequest releases only the preview request that still owns panel loading state.
  // finishPreviewRequest 仅释放仍拥有面板加载状态的预览请求。
  const finishPreviewRequest = React.useCallback((request: WorkspaceFilesOwnedRequest): boolean => {
    if (activeScopeKeyRef.current !== request.scopeKey
      || activePreviewRequestRef.current?.requestId !== request.requestId) {
      return false;
    }
    activePreviewRequestRef.current = null;
    return true;
  }, []);

  // invalidatePreviewRequest removes preview ownership after a local non-request state transition.
  // invalidatePreviewRequest 在本地非请求状态转换后移除预览所有权。
  const invalidatePreviewRequest = React.useCallback(() => {
    activePreviewRequestRef.current = null;
  }, []);

  return React.useMemo(() => ({
    beginDirectoryRequest,
    beginPreviewRequest,
    finishDirectoryRequest,
    finishPreviewRequest,
    invalidatePreviewRequest,
    ownsDirectoryRequest,
    ownsPreviewRequest,
  }), [
    beginDirectoryRequest,
    beginPreviewRequest,
    finishDirectoryRequest,
    finishPreviewRequest,
    invalidatePreviewRequest,
    ownsDirectoryRequest,
    ownsPreviewRequest,
  ]);
}
