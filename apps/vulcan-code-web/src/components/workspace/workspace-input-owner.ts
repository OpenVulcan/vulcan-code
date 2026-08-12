// WorkspaceInputOwner identifies the exclusive WebUI destination for ordinary workspace input.
// WorkspaceInputOwner 标识工作区普通输入在 WebUI 中的独占目标。
export type WorkspaceInputOwner =
  | { readonly kind: "none" }
  | { readonly kind: "composer" }
  | { readonly kind: "project_terminal"; readonly terminalId: string }
  | { readonly kind: "modal" };

// workspaceInputOwner stores the exact owner selected by explicit pointer or lifecycle actions.
// workspaceInputOwner 保存由明确指针或生命周期动作选中的精确所有者。
let workspaceInputOwner: WorkspaceInputOwner = { kind: "none" };

// getWorkspaceInputOwner returns the current explicit workspace input owner.
// getWorkspaceInputOwner 返回当前明确的工作区输入所有者。
// Returns the current immutable owner snapshot without consulting document.activeElement.
// 返回当前不可变所有者快照，不读取 document.activeElement。
export function getWorkspaceInputOwner(): WorkspaceInputOwner {
  return workspaceInputOwner;
}

// setWorkspaceInputOwner replaces the current owner after one explicit UI routing decision.
// setWorkspaceInputOwner 在一次明确界面路由决策后替换当前所有者。
// `owner` is the exact destination; this function returns no value.
// `owner` 是精确目标；本函数无返回值。
export function setWorkspaceInputOwner(owner: WorkspaceInputOwner): void {
  workspaceInputOwner = owner;
}

// releaseProjectTerminalInputOwner releases ownership only when the exact terminal still owns input.
// releaseProjectTerminalInputOwner 仅在精确终端仍持有输入时释放所有权。
// `terminalId` is the terminal being hidden or closed; unrelated owners remain unchanged.
// `terminalId` 是正在隐藏或关闭的终端；无关所有者保持不变。
export function releaseProjectTerminalInputOwner(terminalId: string): void {
  if (workspaceInputOwner.kind === "project_terminal" && workspaceInputOwner.terminalId === terminalId) {
    workspaceInputOwner = { kind: "none" };
  }
}

// transferProjectTerminalInputOwner preserves terminal ownership while the user changes the visible terminal tab.
// transferProjectTerminalInputOwner 在用户切换可见终端标签时保留终端输入所有权。
// `currentTerminalId` is the hidden tab and `nextTerminalId` is the newly selected tab.
// `currentTerminalId` 是被隐藏标签，`nextTerminalId` 是新选中的标签。
export function transferProjectTerminalInputOwner(currentTerminalId: string, nextTerminalId: string): void {
  if (workspaceInputOwner.kind === "project_terminal"
    && workspaceInputOwner.terminalId === currentTerminalId) {
    workspaceInputOwner = { kind: "project_terminal", terminalId: nextTerminalId };
  }
}

// routeWorkspaceInputOwnerFromPointer applies the explicit owner represented by one pointer target.
// routeWorkspaceInputOwnerFromPointer 应用单个指针目标所表示的明确所有者。
// `target` is the captured pointer event target; preserved terminal controls intentionally keep the prior owner.
// `target` 是捕获的指针事件目标；标记为保留的终端控件会有意保持原所有者。
export function routeWorkspaceInputOwnerFromPointer(target: EventTarget | null): void {
  if (!(target instanceof Element)) {
    workspaceInputOwner = { kind: "none" };
    return;
  }
  const terminalTarget = target.closest<HTMLElement>("[data-workspace-input-terminal-id]");
  if (terminalTarget) {
    const terminalId = terminalTarget.dataset.workspaceInputTerminalId;
    workspaceInputOwner = terminalId
      ? { kind: "project_terminal", terminalId }
      : { kind: "none" };
    return;
  }
  if (target.closest('[role="dialog"], [role="alertdialog"]')) {
    workspaceInputOwner = { kind: "modal" };
    return;
  }
  if (target.closest('[data-workspace-input-owner="composer"]')) {
    workspaceInputOwner = { kind: "composer" };
    return;
  }
  if (target.closest('[data-workspace-input-preserve-owner="true"]')) {
    return;
  }
  workspaceInputOwner = { kind: "none" };
}
