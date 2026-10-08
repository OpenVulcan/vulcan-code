import { describe, expect, it } from "vitest";
import type { ToolExecutionConversationBlock } from "@/components/conversation/conversation-tool-execution-types";
import { hydrateCommandExecutionPresentation, resolveCommandExecutionPresentation } from "./command-execution-presentation";

// shellCommandBlock builds one visible command result with an authoritative process exit code.
// shellCommandBlock 构建一条带权威进程退出码的可见命令结果。
// exitCode supplies host metadata and body supplies model output; returns the result block list.
// exitCode 提供宿主元数据，body 提供模型输出；返回结果块列表。
function shellCommandBlock(exitCode: number, body = ""): ToolExecutionConversationBlock[] {
  return [{
    body,
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
  it.each([
    { exitCode: 0, stdout: "\u001b[32mfile.ts\u001b[0m\nREADME.md", stderr: "" },
    { exitCode: 1, stdout: "", stderr: "command failed" },
    { exitCode: 0, stdout: "result", stderr: "warning" },
    { exitCode: 0, stdout: "", stderr: "" },
  ])("retains rendered streams with compact command metadata: %j", ({ exitCode, stdout, stderr }) => {
    // body follows the shell producer format, including omitted sections for empty streams.
    // body 遵循 shell 生产者格式，空输出流不生成对应区段。
    const body = [
      "Command: example-command",
      `Exit code: ${exitCode}`,
      ...(stdout ? [`\n[stdout]\n${stdout}`] : []),
      ...(stderr ? [`\n[stderr]\n${stderr}`] : []),
    ].join("\n");
    // blocks include the request before the result, as in the persisted V1 conversation projection.
    // blocks 与持久化 V1 会话投影一致，在结果之前包含请求块。
    const blocks = shellCommandBlock(exitCode, body).flatMap((block) => [
      {
        ...block,
        id: `${block.id}:request`,
        kind: "tool-call" as const,
        body: JSON.stringify({ command: "example-command" }),
      },
      block,
    ]);
    // presentation must combine metadata with both streams instead of reporting empty output.
    // presentation 必须合并元数据与两种输出流，不能将实际输出误判为空。
    const presentation = resolveCommandExecutionPresentation(blocks, "completed");

    expect(presentation).toMatchObject({
      command: "example-command",
      exitCode,
      state: exitCode === 0 ? "completed" : "failed",
      stdout,
      stderr,
    });
  });

  it("preserves host command identity and plugin routing when reading output text", () => {
    // blocks deliberately use a different rendered command to verify host metadata stays authoritative.
    // blocks 特意使用不同的正文命令，验证宿主元数据仍保持权威。
    const blocks = shellCommandBlock(0, "Command: routed-command\nExit code: 0\n\n[stdout]\nresult")
      .map((block) => ({
        ...block,
        rawMetadata: {
          host_result: {
            kind: "command_run",
            payload: {
              command: "routed-command",
              exit_code: 0,
              hook_transform: {
                original_command: "original-command",
                effective_command: "routed-command",
                contributions: [{ badge: "route", label: "Router", value: "test-plugin" }],
              },
            },
          },
        },
      }));

    expect(resolveCommandExecutionPresentation(blocks, "completed")).toMatchObject({
      command: "original-command",
      effectiveCommand: "routed-command",
      pluginContributions: [{ badge: "route", label: "Router", value: "test-plugin" }],
      stdout: "result",
      stderr: "",
    });
  });

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
