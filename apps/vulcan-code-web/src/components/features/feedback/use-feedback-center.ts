import * as React from "react";
import { hasServiceRuntime } from "@/service/webui-service-runtime";
import type { AnnouncementsView, MessageSummary } from "@/service/feedback-service";
import type { FeedbackCenterController, FeedbackSeed, MessageCenterTab } from "./feedback-center-context";
import type { FeedbackCenterRuntime } from "./feedback-center-runtime";

// useFeedbackCenter owns the open state of the feedback window and the message center. The readers that fill their
// badges load on first use, keeping them out of the initial workbench bundle.
// useFeedbackCenter 持有反馈窗口与消息中心的打开状态。填充徽标的读取者在首次使用时加载，使其不进入初始工作台包。
// Parameters: ready gates service access; serviceEpoch restarts reading for a new service; connectionId is the signed-in
// account, which decides personal messages and the announcement region.
// 参数：ready 门控服务访问；serviceEpoch 为新服务重新开始读取；connectionId 是已登录账户，决定个人消息与公告区域。
export function useFeedbackCenter(ready: boolean, serviceEpoch: number, connectionId: string | null): FeedbackCenterController {
  const [feedbackSeed, setFeedbackSeed] = React.useState<FeedbackSeed | null>(null);
  const [messageCenterTab, setMessageCenterTab] = React.useState<MessageCenterTab | null>(null);
  const [summary, setSummary] = React.useState<MessageSummary | null>(null);
  const [announcements, setAnnouncements] = React.useState<AnnouncementsView | null>(null);
  // runtime is the reader of the current service and account; null before it loads or without a service.
  // runtime 是当前服务与账户的读取者；加载前或没有服务时为 null。
  const runtime = React.useRef<FeedbackCenterRuntime | null>(null);

  React.useEffect(() => {
    if (!ready || !hasServiceRuntime()) return;
    let live = true;
    void import("./feedback-center-runtime").then((module) => {
      if (live) runtime.current = module.startFeedbackCenter(Boolean(connectionId), { setSummary, setAnnouncements });
    });
    return () => {
      live = false;
      runtime.current?.stop();
      runtime.current = null;
    };
  }, [ready, serviceEpoch, connectionId]);

  return React.useMemo<FeedbackCenterController>(() => ({
    feedbackSeed,
    openFeedback: (seed) => setFeedbackSeed(seed ?? { source: "manual" }),
    closeFeedback: () => setFeedbackSeed(null),
    messageCenterTab,
    openMessageCenter: (tab) => setMessageCenterTab(tab ?? ((summary?.unread ?? 0) > 0 ? "messages" : "announcements")),
    closeMessageCenter: () => setMessageCenterTab(null),
    summary,
    announcements,
    runtime,
    setSummary,
  }), [feedbackSeed, messageCenterTab, summary, announcements]);
}
