import React from "react";
import type { ComposerTraceCallback } from "@/components/composer/composer-control-types";
import type { ComposerQueuedDraftReplacementToken } from "@/components/composer/composer-panel/use-composer-session-draft";

// PendingPromptRecoveryOptions are the draft controller hooks a stalled queue edit recovers through.
// PendingPromptRecoveryOptions 是卡住的队列编辑用于恢复的草稿控制器钩子。
interface PendingPromptRecoveryOptions {
  // Captured owner of a queue edit whose RPC acknowledgement was ambiguous; null when nothing is pending.
  // RPC 确认不明确的队列编辑所捕获的所有者；无待恢复项时为 null。
  token: ComposerQueuedDraftReplacementToken | null;
  // Stable token setter; the token is cleared once the owner has been reloaded authoritatively.
  // 稳定的令牌设置器；所有者完成权威重载后清除令牌。
  setToken: React.Dispatch<React.SetStateAction<ComposerQueuedDraftReplacementToken | null>>;
  recover: (token: ComposerQueuedDraftReplacementToken) => Promise<unknown>;
  // Releases both the synchronous and the rendered edit gates.
  // 同时释放同步与已渲染的编辑门控。
  releaseEditGate: () => void;
  onTrace?: ComposerTraceCallback;
}

// usePendingPromptRecovery retries reloading the captured owner with backoff and releases the edit gate only after
// an authoritative response.
// usePendingPromptRecovery 以退避方式重试重载捕获的所有者，并仅在获得权威响应后释放编辑门控。
export function usePendingPromptRecovery(options: PendingPromptRecoveryOptions) {
  const { token, setToken, recover: recoverOwner, releaseEditGate, onTrace } = options;
  React.useEffect(() => {
    if (!token) {
      return;
    }
    // disposed prevents a recovery result from releasing a later component owner.
    // disposed 防止恢复结果释放较晚的组件owner。
    let disposed = false;
    // retryTimer owns only the next bounded reconnect attempt.
    // retryTimer 仅持有下一次有界重连尝试。
    let retryTimer: number | null = null;
    // retryDelayMs backs off repeated local-service outages without allowing stale draft writes.
    // retryDelayMs 对重复本地服务中断进行退避，同时不允许陈旧草稿写入。
    let retryDelayMs = 250;
    // recover reloads the exact captured owner and releases the gate only after an authoritative response.
    // recover 重新加载精确捕获owner，并仅在获得权威响应后释放门控。
    const recover = async () => {
      try {
        await recoverOwner(token);
        if (disposed) {
          return;
        }
        setToken(null);
        releaseEditGate();
      } catch (error) {
        if (disposed) {
          return;
        }
        onTrace?.("pending_prompt.edit_recovery_waiting", {
          draftSlotKey: token.owner.draftSlotKey,
          error: error instanceof Error ? error.message : String(error),
        });
        retryTimer = window.setTimeout(() => { void recover(); }, retryDelayMs);
        retryDelayMs = Math.min(retryDelayMs * 2, 5_000);
      }
    };
    void recover();
    return () => {
      disposed = true;
      if (retryTimer !== null) {
        window.clearTimeout(retryTimer);
      }
    };
  }, [onTrace, recoverOwner, releaseEditGate, setToken, token]);
}
