import { CircleCheck, CircleX, RefreshCw, type LucideIcon } from "lucide-react";

import type { ConversationBlock } from "@/components/conversation/conversation-model";
import { useI18n } from "@/i18n";

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
      className="provider-retry-card"
      data-state={props.state}
      role="status"
    >
      <Icon aria-hidden="true" className="provider-retry-icon" size={16} />
      <div className="provider-retry-content">
        <strong>{label}</strong>
        <span>{props.block.body}</span>
      </div>
    </section>
  );
}
