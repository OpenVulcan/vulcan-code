// Recent output of a running shell_command call, read while its card is open. Core keeps only a small display tail
// in memory and pushes the latest lines; nothing here is stored, and once the call finishes the card shows its result.
// 运行中 shell_command 调用的最近输出，在其卡片展开期间读取。Core 只在内存中保留一段小的显示尾部并推送最新的行；
// 这里不保存任何内容，调用结束后卡片显示其结果。
import * as React from "react";
import { acquireVisibleTerminalDisplayListener } from "@/components/terminal/terminal-display-listener";
import { useTerminalDisplayConnectionState } from "@/components/terminal/use-terminal-display-connection-epoch";
import { SHELL_COMMAND_OUTPUT_CHANGED_TOPIC } from "@/service/shell-command-output-events";
import { attachShellCommandOutput, detachShellCommandOutput } from "@/service/terminal-service";
import { serviceClient } from "@/service/webui-service-runtime";
import type { ShellCommandOutputChangedEvent } from "@/types";

// ShellCommandLiveTarget names the exact running call.
// ShellCommandLiveTarget 指明精确的运行中调用。
export interface ShellCommandLiveTarget {
  // Session that owns the call.
  // 持有该调用的会话。
  sessionId: string;
  // API call that issued it.
  // 发出该调用的 API 调用。
  apiCallId: number;
  // Tool call within that API call.
  // 该 API 调用内的工具调用。
  toolCallId: number;
}

// ShellCommandLiveOutput is what the card shows.
// ShellCommandLiveOutput 是卡片显示的内容。
export interface ShellCommandLiveOutput {
  // Most recent lines, oldest first.
  // 最近的行，按从旧到新排列。
  lines: string[];
  // Number of most recent lines shown at most.
  // 最多显示的最近行数。
  lineLimit: number;
  // Whether Core still runs the command; false also when this Core never ran it (for example after a restart).
  // Core 是否仍在运行该命令；本 Core 从未运行过它时（例如重启后）同样为 false。
  running: boolean;
}

/** Follows the recent output of one running call while mounted; null until the first reading arrives.
 * 挂载期间跟随一次运行中调用的最近输出；首次读取到达前为 null。 */
export function useShellCommandLiveOutput(target: ShellCommandLiveTarget): ShellCommandLiveOutput | null {
  const { connectionAvailable, connectionEpoch } = useTerminalDisplayConnectionState();
  const [output, setOutput] = React.useState<ShellCommandLiveOutput | null>(null);
  const { apiCallId, sessionId, toolCallId } = target;

  React.useEffect(() => {
    if (!connectionAvailable) return;
    let cancelled = false;
    let subscriptionId: string | null = null;
    let unsubscribe: (() => void) | null = null;
    // Events arriving before the attach reply are held so none is lost; only this card's subscription applies.
    // 挂接回复之前到达的事件会被暂存而不丢失；只应用本卡片的订阅。
    const pending: ShellCommandOutputChangedEvent[] = [];
    const apply = (event: ShellCommandOutputChangedEvent) => {
      if (cancelled || event.subscriptionId !== subscriptionId) return;
      setOutput({ lineLimit: event.lineLimit, lines: event.lines, running: event.running });
    };
    const release = () => {
      const retainedUnsubscribe = unsubscribe;
      unsubscribe = null;
      retainedUnsubscribe?.();
      const retainedSubscriptionId = subscriptionId;
      subscriptionId = null;
      if (retainedSubscriptionId) void detachShellCommandOutput(retainedSubscriptionId);
    };
    const attach = async () => {
      try {
        const listening = await acquireVisibleTerminalDisplayListener(
          () => serviceClient.subscribe<ShellCommandOutputChangedEvent>(SHELL_COMMAND_OUTPUT_CHANGED_TOPIC, (event) => {
            if (subscriptionId === null) pending.push(event);
            else apply(event);
          }),
          () => cancelled,
          (retainedUnsubscribe) => {
            unsubscribe = retainedUnsubscribe;
          },
        );
        if (!listening) return;
        const attachment = await attachShellCommandOutput(sessionId, apiCallId, toolCallId);
        subscriptionId = attachment?.subscriptionId ?? null;
        if (cancelled || !attachment) {
          release();
          return;
        }
        setOutput({ lineLimit: attachment.lineLimit, lines: attachment.lines, running: attachment.running });
        for (const event of pending) apply(event);
        if (!attachment.subscriptionId) release();
      } catch (error) {
        release();
        console.warn("Failed to follow the running command output.", error);
      }
    };
    void attach();
    return () => {
      cancelled = true;
      release();
    };
  }, [apiCallId, connectionAvailable, connectionEpoch, sessionId, toolCallId]);

  return output;
}
