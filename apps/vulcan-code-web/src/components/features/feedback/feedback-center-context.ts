import { createContext, type RefObject } from "react";
import type { SystemProviderError } from "@/components/conversation/conversation-system-error-card";
import type { AnnouncementsView, FeedbackKind, FeedbackSource, MessageSummary } from "@/service/feedback-service";
import type { FeedbackCenterRuntime } from "./feedback-center-runtime";

// FeedbackSeed is what an entry knows when it opens the feedback window; the user edits everything else.
// FeedbackSeed 是入口打开反馈窗口时已知的内容；其余内容由用户编辑。
export interface FeedbackSeed {
  // Entry that opened the window.
  // 打开窗口的入口。
  source: FeedbackSource;
  kind?: FeedbackKind;
  title?: string;
  body?: string;
  // Failure of the error card that opened the window; the window derives the report title and attached facts.
  // 打开窗口的错误卡的失败信息；窗口由其得出报告标题与附带事实。
  systemError?: SystemProviderError | null;
}

// MessageCenterTab names one page of the message center.
// MessageCenterTab 指明消息中心的一页。
export type MessageCenterTab = "announcements" | "messages" | "feedback";

// FeedbackCenterController opens the feedback window and the message center and carries their badge state.
// FeedbackCenterController 打开反馈窗口与消息中心，并携带其徽标状态。
export interface FeedbackCenterController {
  // Seed of the open feedback window; null while it is closed.
  // 已打开反馈窗口的初始内容；关闭时为 null。
  feedbackSeed: FeedbackSeed | null;
  // Opens the feedback window; a manual suggestion when no seed is given.
  // 打开反馈窗口；未给出初始内容时为手动建议。
  openFeedback: (seed?: FeedbackSeed) => void;
  closeFeedback: () => void;
  // Page of the open message center; null while it is closed.
  // 已打开消息中心的页面；关闭时为 null。
  messageCenterTab: MessageCenterTab | null;
  // Opens the message center; without a tab it opens unread messages first, otherwise announcements.
  // 打开消息中心；未指定页面时有未读消息则打开消息页，否则打开公告页。
  openMessageCenter: (tab?: MessageCenterTab) => void;
  closeMessageCenter: () => void;
  // Latest personal-message summary; null until the first read.
  // 最近的个人消息摘要；首次读取前为 null。
  summary: MessageSummary | null;
  // Latest announcements; null until the first read.
  // 最近的公告；首次读取前为 null。
  announcements: AnnouncementsView | null;
  // Running reader of both, which also records seen and dismissed announcements; null before it loads or without a
  // service, when callers skip the action.
  // 两者的运行中读取者，也负责记录已看与已关闭的公告；加载前或没有服务时为 null，调用方此时跳过该操作。
  runtime: RefObject<FeedbackCenterRuntime | null>;
  // Replaces the summary, such as with the unread count a mark-read call returned.
  // 替换摘要，例如应用标记已读调用返回的未读数。
  setSummary: (summary: MessageSummary) => void;
}

// Null denotes a surface without the application message center, such as a component preview.
// null 表示未挂载应用消息中心的界面，例如组件预览。
export const FeedbackCenterContext = createContext<FeedbackCenterController | null>(null);
