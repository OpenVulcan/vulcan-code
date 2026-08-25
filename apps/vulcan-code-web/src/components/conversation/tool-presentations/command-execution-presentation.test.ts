import { describe, expect, it } from "vitest";
import type { ToolExecutionConversationBlock } from "@/components/conversation/conversation-tool-execution-types";
import { hydrateCommandExecutionPresentation, resolveCommandExecutionPresentation } from "./command-execution-presentation";

// shellCommandBlock builds one visible command result with an authoritative process exit code.
// shellCommandBlock 构建一条带权威进程退出码的可见命令结果。
function shellCommandBlock(exitCode: number): ToolExecutionConversationBlock[] {
  return [{
    body: "",
    id: "tool-row",
    kind: "tool-result",
    rawMetadata: {
      tool_name: "shell_command",
      host_result: {
        kind: "command_run",
        payload: {
          command: "example-command",
          exit_code: exitCode,
        },
      },
    },
    summary: "",
    title: "shell_command",
    toolCallId: "tool-call",
    toolName: "shell_command",
  }];
}

describe("command execution presentation", () => {
  it("marks a returned non-zero command as failed in the live card", () => {
    // presentation receives the same compact command_run payload used by the live card.
    // presentation 接收实时卡片使用的同一份紧凑 command_run 载荷。
    const presentation = resolveCommandExecutionPresentation(
      shellCommandBlock(1),
      "completed",
    );

    expect(presentation).toMatchObject({
      command: "example-command",
      exitCode: 1,
      state: "failed",
    });
  });

  it("keeps a zero-exit command completed", () => {
    // presentation proves successful process completion remains unchanged.
    // presentation 证明成功的进程终态保持不变。
    const presentation = resolveCommandExecutionPresentation(
      shellCommandBlock(0),
      "completed",
    );

    expect(presentation).toMatchObject({ exitCode: 0, state: "completed" });
  });

  it("corrects a historical generic completion when hidden detail reports a non-zero exit", () => {
    // presentation models a historical visible row persisted before its terminal detail arrived.
    // presentation 模拟在终态详情到达前持久化的历史可见行。
    const presentation = resolveCommandExecutionPresentation(
      shellCommandBlock(0),
      "completed",
    );
    // hydrated applies the authoritative hidden detail loaded during history reconstruction.
    // hydrated 应用历史重建期间加载的权威隐藏详情。
    const hydrated = hydrateCommandExecutionPresentation(presentation!, {
      body: "",
      metadata: {
        display: {
          kind: "shell_command",
          command: "example-command",
          exit_code: 1,
          stdout: "",
          stderr: "failed",
        },
      },
    });

    expect(hydrated).toMatchObject({ exitCode: 1, state: "failed" });
  });
});
