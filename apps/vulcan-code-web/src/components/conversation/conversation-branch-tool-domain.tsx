import * as React from "react";
import { useI18n } from "@/i18n";
import type { ToolCallEvent } from "@/service/conversation-event-display-projection";
import { resolveV1ToolDomainPresentation } from "./tool-presentations/v1-tool-domain-presentation";
import { ConversationV1GenericToolDetails, ConversationV1ToolDomainBody } from "./conversation-v1-tool-domain";

/** Projects the canonical event after the user opens its result card.
 * 用户展开结果卡后，投影规范事件。
 * Parameters: event is the current V1 revision; id owns detail state; tabbed selects generic-tool tabs. Returns the readable result.
 * 参数：event 是当前 V1 修订；id 标识详情状态；tabbed 选择通用工具标签页。返回可读结果。
 */
export default function ConversationBranchToolDomain(props: { event: ToolCallEvent; id: string; tabbed?: boolean }) {
  // Recompute on an event revision, without retaining a previous result during streaming updates.
  // 事件修订时重新计算，不在流式更新期间保留旧结果。
  const { t } = useI18n();
  const domain = React.useMemo(() => resolveV1ToolDomainPresentation(props.event, t), [props.event, t]);
  if (props.tabbed) return <ConversationV1GenericToolDetails event={props.event} domain={domain} id={props.id} />;
  return <ConversationV1ToolDomainBody domain={domain} />;
}
