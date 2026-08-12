import { beforeEach, describe, expect, it } from "vitest";

import {
  getWorkspaceInputOwner,
  releaseProjectTerminalInputOwner,
  setWorkspaceInputOwner,
  transferProjectTerminalInputOwner,
} from "./workspace-input-owner";

describe("workspace input owner", () => {
  beforeEach(() => setWorkspaceInputOwner({ kind: "none" }));

  // Verifies lifecycle release cannot clear a different terminal's explicit ownership.
  // 验证生命周期释放不会清除另一个终端的明确所有权。
  it("releases only the exact terminal owner", () => {
    setWorkspaceInputOwner({ kind: "project_terminal", terminalId: "terminal-a" });
    releaseProjectTerminalInputOwner("terminal-b");
    expect(getWorkspaceInputOwner()).toEqual({ kind: "project_terminal", terminalId: "terminal-a" });
    releaseProjectTerminalInputOwner("terminal-a");
    expect(getWorkspaceInputOwner()).toEqual({ kind: "none" });
  });

  // Verifies a terminal-tab selection transfers ownership without a transient composer route.
  // 验证终端标签选择会转移所有权，且不会短暂路由到输入区。
  it("transfers ownership across terminal tabs", () => {
    setWorkspaceInputOwner({ kind: "project_terminal", terminalId: "terminal-a" });
    transferProjectTerminalInputOwner("terminal-a", "terminal-b");
    expect(getWorkspaceInputOwner()).toEqual({ kind: "project_terminal", terminalId: "terminal-b" });
  });
});
