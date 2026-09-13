import * as React from "react";
import type { ToolCallEvent } from "@/service/conversation-event-display-projection";
import { resolveV1ToolDomainPresentation } from "./tool-presentations/v1-tool-domain-presentation";
import { ConversationV1ToolDomainBody, ConversationV1ToolRawDetails } from "./conversation-v1-tool-domain";

/** Projects the canonical event after the user opens its result card.
 * 用户展开结果卡后，投影规范事件。
 * Parameters: event is the exact current V1 revision; id owns the raw-detail fold. Returns the readable result.
 * 参数：event 是当前精确 V1 修订；id 标识原始详情折叠。返回可读结果。
 */
export default function ConversationBranchToolDomain(props: { event: ToolCallEvent; id: string }) {
  // Recompute on an event revision, without retaining a previous result during streaming updates.
  // 事件修订时重新计算，不在流式更新期间保留旧结果。
  const domain = React.useMemo(() => resolveV1ToolDomainPresentation(props.event), [props.event]);
  return <>
    <ConversationV1ToolDomainBody domain={domain} />
    <ConversationV1ToolRawDetails id={props.id} input={props.event.input} output={props.event.output} />
  </>;
}
