import React from "react";
import { CircleCheck, CircleX, RefreshCw, type LucideIcon } from "lucide-react";

import type { ConversationBlock } from "@/components/conversation/conversation-model";
import { useI18n } from "@/i18n";
import type { ProviderRetryCountdown } from "@/state/conversation-live-stream-store";

// ProviderRetryDisplayState describes the exact lifecycle shown by one retained retry notice.
// ProviderRetryDisplayState 描述一条已保留重试提示展示的精确生命周期。
export type ProviderRetryDisplayState = "retrying" | "succeeded" | "failed";

// ProviderRetryCardProps contains the parsed retry block and its validated display state.
// ProviderRetryCardProps 包含已解析的重试块及其经过验证的展示状态。
interface ProviderRetryCardProps {
  // Block retains the original retry reason and wait description.
  // block 保留原始重试原因与等待说明。
  block: ConversationBlock;
  // State is produced only from the backend-owned provider_retry_state field.
  // state 仅由后端持有的 provider_retry_state 字段生成。
  state: ProviderRetryDisplayState;
}

// RETRY_STATE_ICON binds every retry lifecycle state to one stable visual icon.
// RETRY_STATE_ICON 将每个重试生命周期状态绑定到一个稳定视觉图标。
const RETRY_STATE_ICON: Record<ProviderRetryDisplayState, LucideIcon> = {
  retrying: RefreshCw,
  succeeded: CircleCheck,
  failed: CircleX,
};

// Resolves one exact Provider retry state from retained transcript metadata.
// 从已保留转录元数据中解析一个精确的 Provider 重试状态。
///
// Parameters: metadata is the raw backend-owned message metadata.
// 参数：metadata 是后端持有的原始消息元数据。
// Returns: a supported retry state, or null when the message is not a Provider retry notice.
// 返回值：受支持的重试状态；消息并非 Provider 重试提示时返回 null。
export function providerRetryDisplayState(metadata: unknown): ProviderRetryDisplayState | null {
  if (!metadata || typeof metadata !== "object" || Array.isArray(metadata)) {
    return null;
  }
  // value is the exact lifecycle field written by the retry projection.
  // value 是重试投影写入的精确生命周期字段。
  const value = (metadata as Record<string, unknown>).provider_retry_state;
  return value === "retrying" || value === "succeeded" || value === "failed" ? value : null;
}

// Renders one persistent Provider retry lifecycle card inside the transcript.
// 在转录中渲染一张持久化的 Provider 重试生命周期卡片。
///
// Parameters: props contains the original detail text and validated lifecycle state.
// 参数：props 包含原始详情文本与经过验证的生命周期状态。
// Returns: one non-interactive status card whose color changes in place.
// 返回值：一张颜色会原位变化的非交互状态卡片。
export function ProviderRetryCard(props: ProviderRetryCardProps) {
  // t localizes the three explicit retry lifecycle labels.
  // t 对三个明确的重试生命周期标签进行本地化。
  const { t } = useI18n();
  // Icon visually distinguishes waiting, success, and failure without relying on color alone.
  // Icon 在不只依赖颜色的情况下区分等待、成功与失败。
  const Icon = RETRY_STATE_ICON[props.state];
  // label is the user-facing terminal or active state title.
  // label 是面向用户的终态或活动状态标题。
  const label = props.state === "retrying"
    ? t("conversation.providerRetry.retrying")
    : props.state === "succeeded"
      ? t("conversation.providerRetry.succeeded")
      : t("conversation.providerRetry.failed");

  return (
    <section
      aria-label={label}
      className="vk-note"
      data-retry-state={props.state}
      data-tone={props.state === "retrying" ? "warn" : props.state === "succeeded" ? "ok" : "bad"}
      role="status"
    >
      <Icon aria-hidden="true" />
      <div className="vk-note-text">
        <strong>{label}</strong>
        <span>{props.block.body}</span>
      </div>
    </section>
  );
}

// COUNTDOWN_TICK_MS is the refresh interval of the visible whole-second countdown.
// COUNTDOWN_TICK_MS 是可见整秒倒计时的刷新间隔。
const COUNTDOWN_TICK_MS = 250;

// remainingRetrySeconds converts a local retry deadline into whole seconds left, never below zero.
// remainingRetrySeconds 将本地重试截止时间换算为剩余整秒数，最小为零。
// Parameters: retryAtMs is the local deadline and nowMs the current local clock.
// 参数：retryAtMs 是本地截止时间，nowMs 是当前本地时钟。
// Returns: the rounded-up number of seconds before the retry starts.
// 返回值：重试开始前向上取整的秒数。
export function remainingRetrySeconds(retryAtMs: number, nowMs: number): number {
  return Math.max(0, Math.ceil((retryAtMs - nowMs) / 1000));
}

// ProviderRetryCountdownLine renders the live wait before the next automatic Provider retry.
// ProviderRetryCountdownLine 渲染下一次 Provider 自动重试前的实时等待。
// Parameters: countdown is the locally anchored retry wait from the live stream store.
// 参数：countdown 是实时流存储中以本地时钟锚定的重试等待。
// Returns: one polite live status line that ticks until the retry request starts.
// 返回值：一行礼貌播报的实时状态，持续跳动直到重试请求开始。
export function ProviderRetryCountdownLine(props: { countdown: ProviderRetryCountdown }) {
  const { t } = useI18n();
  const { retryAtMs } = props.countdown;
  const [nowMs, setNowMs] = React.useState(() => Date.now());
  React.useEffect(() => {
    setNowMs(Date.now());
    const timer = window.setInterval(() => {
      const current = Date.now();
      setNowMs(current);
      if (current >= retryAtMs) window.clearInterval(timer);
    }, COUNTDOWN_TICK_MS);
    return () => window.clearInterval(timer);
  }, [retryAtMs]);
  const text = t("conversation.providerRetry.countdown")
    .replace("{seconds}", String(remainingRetrySeconds(retryAtMs, nowMs)))
    .replace("{attempt}", String(props.countdown.attempt))
    .replace("{max}", String(props.countdown.maxRetries));
  return (
    <div
      aria-busy="true"
      aria-live="polite"
      className="vk-reason-status"
      data-provider-retry-countdown=""
      title={props.countdown.reason}
    >
      <div className="vk-reason-line vk-shimmer-text">{text}</div>
    </div>
  );
}
