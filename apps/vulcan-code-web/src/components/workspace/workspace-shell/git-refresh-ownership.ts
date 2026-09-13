// WorkspaceGitRefreshTicket identifies one refresh without conflating projects or later requests.
// WorkspaceGitRefreshTicket 标识单次刷新，不混淆不同项目或后续请求。
export interface WorkspaceGitRefreshTicket {
  // workspacePath is the exact cache key captured by the initiating action.
  // workspacePath 是发起动作捕获的精确缓存键。
  workspacePath: string;
  // requestId is a monotonically increasing local request generation.
  // requestId 是本地单调递增的请求代次。
  requestId: number;
}

/** Creates ownership for independent workspace refreshes; returns begin and ownership predicates.
 * 创建各工作区独立刷新所有权；返回开始请求及所有权判断方法。
 * Parameters: none; each factory call owns a fresh project generation map without IO.
 * 参数：无；每次工厂调用持有全新的项目代次映射，不执行输入输出。 */
export function createWorkspaceGitRefreshOwnership() {
  // generations keeps only the latest acknowledged start for each exact workspace cache key.
  // generations 为每个精确工作区缓存键仅保留最近一次开始的请求。
  const generations = new Map<string, number>();
  // sequence avoids reusing a generation when another project starts a refresh.
  // sequence 避免其他项目开始刷新时重用请求代次。
  let sequence = 0;
  return {
    /** Starts a refresh for workspacePath and returns its immutable ownership ticket.
     * 为 workspacePath 开始刷新，并返回不可混淆的所有权凭据。
     * Parameters: workspacePath is the exact cache key. Returns its newly allocated request identity.
     * 参数：workspacePath 是精确缓存键。返回新分配的请求身份。 */
    begin(workspacePath: string): WorkspaceGitRefreshTicket {
      const requestId = ++sequence;
      generations.set(workspacePath, requestId);
      return { workspacePath, requestId };
    },
    /** Tests whether ticket may update its cache; unrelated project refreshes do not invalidate it.
     * 判断 ticket 能否更新其缓存；其他项目的刷新不会使它失效。
     * Parameters: ticket is the captured refresh identity. Returns true only for that project's latest start.
     * 参数：ticket 是捕获的刷新身份。仅在它属于该项目最近一次开始时返回真。 */
    owns(ticket: WorkspaceGitRefreshTicket): boolean {
      return generations.get(ticket.workspacePath) === ticket.requestId;
    },
    /** Tests whether ticket may update the visible repository selection for selectedWorkspacePath.
     * 判断 ticket 能否更新 selectedWorkspacePath 的可见仓库选择。
     * Parameters: ticket is the completed request; selectedWorkspacePath is the currently displayed project or null.
     * 参数：ticket 是完成的请求；selectedWorkspacePath 是当前显示的项目或 null。
     * Returns true only when both project identity and its latest request generation still match.
     * 仅在项目身份及其最近请求代次仍同时匹配时返回真。 */
    ownsSelection(ticket: WorkspaceGitRefreshTicket, selectedWorkspacePath: string | null): boolean {
      return selectedWorkspacePath === ticket.workspacePath && generations.get(ticket.workspacePath) === ticket.requestId;
    },
  };
}
