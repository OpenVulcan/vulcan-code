import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import { I18nProvider } from "@/i18n";
import { ConversationTreeRow } from "@/components/project/project-sidebar-conversation-row";
import type { ConversationSummary } from "@/types";

// noop stands in for row actions the markup checks never trigger.
// noop 代替标记检查从不触发的行操作。
const noop = () => undefined;

// rowMarkup renders one unselected tree row for the given summary.
// rowMarkup 为给定摘要渲染一个未选中的树行。
function rowMarkup(conversation: ConversationSummary): string {
  return renderToStaticMarkup(
    <I18nProvider initialLocale="en-US">
      <ConversationTreeRow compactPinned={false} conversation={conversation} isPinned={false} isSelected={false}
        locale="en-US" nowMs={0} onAction={noop} onSelectConversation={noop} />
    </I18nProvider>,
  );
}

describe("ConversationTreeRow", () => {
  // The unread flag alone decides the unread dot; a leftover unread attention must not keep it after reading.
  // 未读圆点只由未读标志决定；已读后残留的未读关注不得使其保留。
  it("shows the unread dot from the flag, not from leftover unread attention", () => {
    const base: ConversationSummary = { id: "c1", title: "Probe", status: "Completed" };
    const attention = { kind: "unread", priority: 10, label: "Unread activity" };
    expect(rowMarkup({ ...base, unread: true })).toContain('data-tone="info"');
    expect(rowMarkup({ ...base, unread: false, attention })).not.toContain('data-tone="info"');
    expect(rowMarkup({ ...base, unread: false, attention: { ...attention, kind: "subagent_attention" } }))
      .toContain('data-tone="info"');
  });
});
