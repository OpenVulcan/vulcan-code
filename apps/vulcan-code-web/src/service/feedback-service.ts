import { serviceClient } from "@/service/webui-service-runtime";
import type { OpenVulcanRegion } from "@/service/openvulcan-account-service";

// FeedbackKind is what the user sends: an idea or a problem report.
// FeedbackKind 是用户发送的内容类型：想法或问题报告。
export type FeedbackKind = "suggestion" | "bug";

// FeedbackSource names the entry that produced a draft; the server only counts it.
// FeedbackSource 指明产生草稿的入口；服务端只做统计。
export type FeedbackSource = "manual" | "error_card" | "ai_draft";

// FeedbackStatus is the triage state the maintainers set on a report.
// FeedbackStatus 是维护者为报告设置的处理状态。
export type FeedbackStatus = "received" | "triaged" | "accepted" | "duplicate" | "wont_fix" | "done";

// FeedbackErrorFacts are the failure facts of a session error card, as the transcript parser read them.
// FeedbackErrorFacts 是转录解析器读出的会话错误卡失败事实。
export interface FeedbackErrorFacts {
  category: string;
  subcategory?: string | null;
  stage?: string | null;
  httpStatus?: number | null;
  providerId?: string | null;
  modelId?: string | null;
  upstreamCode?: string | null;
  // Request id; it also selects the log lines of this request.
  // 请求编号；同时用于选取本次请求的日志行。
  requestId?: string | null;
  retryable?: boolean | null;
  message?: string | null;
}

// FeedbackDraft is the whole draft the window holds; the service filters it before anything is shown.
// FeedbackDraft 是窗口持有的完整草稿；服务端在展示任何内容之前先过滤。
export interface FeedbackDraft {
  // Id of an earlier preparation; reusing it keeps one idempotency key across retries.
  // 先前准备的编号；复用它使多次重试保持同一幂等键。
  draftId?: string | null;
  kind: FeedbackKind;
  source: FeedbackSource;
  title: string;
  body: string;
  allowContact: boolean;
  includeClientInfo: boolean;
  includeLog: boolean;
  error?: FeedbackErrorFacts | null;
}

// FeedbackPreview is the exact request a send would transmit, after on-device filtering.
// FeedbackPreview 是设备端过滤后、发送时将传输的精确请求。
export interface FeedbackPreview {
  draftId: string;
  // SHA-256 of the exact bytes; sending requires it back unchanged.
  // 精确字节的 SHA-256；发送时必须原样传回。
  digest: string;
  // Pretty-printed request with the same content as the bytes sent.
  // 与发送字节内容相同的美化请求。
  payload: string;
  byteLength: number;
  // Replacements per rule category; never the replaced text.
  // 各规则类别的替换次数；绝不包含被替换的文本。
  redactionHits: Record<string, number>;
  logLineCount: number;
  signedIn: boolean;
}

// FeedbackReceipt confirms a sent report.
// FeedbackReceipt 确认一份已发送的报告。
export interface FeedbackReceipt {
  feedbackId: string;
  status: FeedbackStatus;
  createdAt: string;
}

// FeedbackSummary is one row of the user's own reports, as the user center returns it.
// FeedbackSummary 是用户中心返回的用户本人报告中的一行。
export interface FeedbackSummary {
  id: string;
  kind: FeedbackKind;
  status: FeedbackStatus;
  title: string;
  created_at: string;
  updated_at: string;
  reply_count: number;
}

// FeedbackPage is one page of the user's reports, newest first.
// FeedbackPage 是用户报告的一页，按从新到旧排列。
export interface FeedbackPage {
  feedback: FeedbackSummary[];
  next_cursor: string | null;
}

// FeedbackReply is one public maintainer reply.
// FeedbackReply 是一条公开的维护者回复。
export interface FeedbackReply {
  body: string;
  created_at: string;
}

// FeedbackLink is one pull request the user linked to a report.
// FeedbackLink 是用户关联到报告的一个拉取请求。
export interface FeedbackLink {
  kind: "github_pr";
  url: string;
  created_at: string;
}

// FeedbackDetail is one report with its replies and links.
// FeedbackDetail 是一份报告及其回复与链接。
export interface FeedbackDetail extends FeedbackSummary {
  body: string;
  allow_contact: boolean;
  // Whether the attachments were already removed by retention.
  // 附件是否已因保留期限被删除。
  attachments_purged: boolean;
  replies: FeedbackReply[];
  links: FeedbackLink[];
}

// AnnouncementLink is the action of an announcement.
// AnnouncementLink 是公告的操作。
export interface AnnouncementLink {
  kind: "app_update" | "url" | "feedback";
  url?: string;
  feedback_id?: string;
}

// Announcement is one announcement in the interface language, with its local marks.
// Announcement 是界面语言下的一条公告及其本地标记。
export interface Announcement {
  id: string;
  kind: "update" | "event" | "maintenance" | "notice";
  priority: "normal" | "important";
  startsAt: string;
  endsAt: string | null;
  locale: string;
  title: string;
  // Markdown body.
  // Markdown 正文。
  body: string;
  link: AnnouncementLink | null;
  seen: boolean;
  dismissed: boolean;
}

// AnnouncementsView lists the announcements that target this installation.
// AnnouncementsView 列出定向到本安装的公告。
export interface AnnouncementsView {
  enabled: boolean;
  region: OpenVulcanRegion | null;
  // The user center could not be reached and the list is the last one fetched.
  // 无法连接用户中心，列表为最近一次获取的结果。
  offline: boolean;
  announcements: Announcement[];
  unseen: number;
}

// MessageSummary is the unread state of the signed-in user's personal messages.
// MessageSummary 是已登录用户个人消息的未读状态。
export interface MessageSummary {
  signedIn: boolean;
  available: boolean;
  unread: number;
  latestId: string | null;
}

// PersonalMessage is one message addressed to the signed-in user.
// PersonalMessage 是发给已登录用户的一条消息。
export interface PersonalMessage {
  id: string;
  kind: "feedback_reply" | "feedback_status" | "direct" | "account";
  title: string;
  body: string;
  created_at: string;
  read_at: string | null;
  ref: { feedback_id?: string; feedback_status?: FeedbackStatus };
}

// MessagePage is one page of personal messages, newest first.
// MessagePage 是个人消息的一页，按从新到旧排列。
export interface MessagePage {
  messages: PersonalMessage[];
  next_cursor: string | null;
}

// MarkReadResult reports how many messages changed and how many stay unread.
// MarkReadResult 报告变更的消息数与仍未读的消息数。
export interface MarkReadResult {
  updated: number;
  unread: number;
}

// MessageCenterSettings are the user's message-center choices.
// MessageCenterSettings 是用户的消息中心选择。
export interface MessageCenterSettings {
  receiveAnnouncements: boolean;
}

// prepareFeedback filters a draft on this device and returns the exact request to preview.
// prepareFeedback 在本设备过滤草稿，并返回待预览的精确请求。
export function prepareFeedback(draft: FeedbackDraft): Promise<FeedbackPreview> {
  return serviceClient.request("service_prepare_feedback", { draft });
}

// submitFeedback sends the exact previewed bytes; the digest proves which preview the user saw.
// submitFeedback 发送预览过的精确字节；摘要证明用户看到的是哪一份预览。
export function submitFeedback(draftId: string, digest: string): Promise<FeedbackReceipt> {
  return serviceClient.request("service_submit_feedback", { draftId, digest });
}

// listFeedback reads one page of the signed-in user's reports.
// listFeedback 读取已登录用户报告的一页。
export function listFeedback(cursor?: string | null): Promise<FeedbackPage> {
  return serviceClient.request("service_list_feedback", cursor ? { cursor } : {});
}

// readFeedback reads one report with its replies and links.
// readFeedback 读取一份报告及其回复与链接。
export function readFeedback(feedbackId: string): Promise<FeedbackDetail> {
  return serviceClient.request("service_feedback_detail", { feedbackId });
}

// linkFeedbackPullRequest links a Vulcan Code pull request to a report and returns all its links.
// linkFeedbackPullRequest 将 Vulcan Code 拉取请求关联到报告，并返回其全部链接。
export function linkFeedbackPullRequest(feedbackId: string, url: string): Promise<{ links: FeedbackLink[] }> {
  return serviceClient.request("service_link_feedback_pr", { feedbackId, url });
}

// deleteFeedback permanently deletes one of the user's reports.
// deleteFeedback 永久删除用户的一份报告。
export function deleteFeedback(feedbackId: string): Promise<void> {
  return serviceClient.request("service_delete_feedback", { feedbackId });
}

// listAnnouncements reads the announcements that target this installation.
// listAnnouncements 读取定向到本安装的公告。
export function listAnnouncements(): Promise<AnnouncementsView> {
  return serviceClient.request("service_list_announcements");
}

// markAnnouncements records announcements as seen, or their banners as closed.
// markAnnouncements 把公告记为已看，或把其横幅记为已关闭。
export function markAnnouncements(announcementIds: string[], dismissed: boolean): Promise<void> {
  return serviceClient.request("service_mark_announcements", { announcementIds, dismissed });
}

// loadMessageSummary reads the unread state of personal messages.
// loadMessageSummary 读取个人消息的未读状态。
export function loadMessageSummary(): Promise<MessageSummary> {
  return serviceClient.request("service_message_summary");
}

// listMessages reads one page of personal messages.
// listMessages 读取个人消息的一页。
export function listMessages(cursor?: string | null, unreadOnly = false): Promise<MessagePage> {
  return serviceClient.request("service_list_messages", { ...(cursor ? { cursor } : {}), unreadOnly });
}

// markMessagesRead marks the given messages read.
// markMessagesRead 将给定消息标记为已读。
export function markMessagesRead(ids: string[]): Promise<MarkReadResult> {
  return serviceClient.request("service_mark_messages_read", { ids });
}

// markAllMessagesRead marks every message up to and including `latestId` read.
// markAllMessagesRead 将 `latestId` 及之前的全部消息标记为已读。
export function markAllMessagesRead(latestId: string): Promise<MarkReadResult> {
  return serviceClient.request("service_mark_messages_read", { allBefore: latestId });
}

// loadMessageCenterSettings reads the message-center settings.
// loadMessageCenterSettings 读取消息中心设置。
export function loadMessageCenterSettings(): Promise<MessageCenterSettings> {
  return serviceClient.request("service_message_center_settings");
}

// saveMessageCenterSettings saves the message-center settings.
// saveMessageCenterSettings 保存消息中心设置。
export function saveMessageCenterSettings(settings: MessageCenterSettings): Promise<MessageCenterSettings> {
  return serviceClient.request("service_save_message_center_settings", { settings });
}
