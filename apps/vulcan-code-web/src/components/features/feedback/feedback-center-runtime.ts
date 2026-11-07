import { listAnnouncements, loadMessageSummary, markAnnouncements, type Announcement, type AnnouncementsView, type MessageSummary } from "@/service/feedback-service";

// MESSAGE_SUMMARY_POLL_MS is how often the unread count of personal messages is read while signed in.
// MESSAGE_SUMMARY_POLL_MS 是登录期间读取个人消息未读数的间隔。
export const MESSAGE_SUMMARY_POLL_MS = 60_000;

// ANNOUNCEMENT_POLL_MS is how often announcements are read; an unchanged document costs one 304 answer.
// ANNOUNCEMENT_POLL_MS 是读取公告的间隔；文档未变化时只需一次 304 应答。
export const ANNOUNCEMENT_POLL_MS = 10 * 60_000;

// FeedbackCenterSinks receive what the background readers observe.
// FeedbackCenterSinks 接收后台读取者观察到的内容。
export interface FeedbackCenterSinks {
  setSummary: (summary: MessageSummary) => void;
  setAnnouncements: (update: (current: AnnouncementsView | null) => AnnouncementsView | null) => void;
}

// FeedbackCenterRuntime is one running reader; it stops when the service, account or workbench changes.
// FeedbackCenterRuntime 是一个运行中的读取者；服务、账户或工作台变化时停止。
export interface FeedbackCenterRuntime {
  stop: () => void;
  refresh: () => Promise<void>;
  markSeen: (ids: string[]) => Promise<void>;
  dismissBanner: (id: string) => Promise<void>;
}

// withAnnouncementMark returns the view with one mark set on the given announcements and the unseen count recomputed.
// withAnnouncementMark 返回为给定公告设置一个标记并重新计算未看数后的视图。
export function withAnnouncementMark(view: AnnouncementsView, ids: readonly string[], mark: "seen" | "dismissed"): AnnouncementsView {
  const targets = new Set(ids);
  const announcements = view.announcements.map((announcement) => targets.has(announcement.id) ? { ...announcement, [mark]: true } : announcement);
  return { ...view, announcements, unseen: announcements.filter((announcement) => !announcement.seen).length };
}

// bannerAnnouncement returns the newest important announcement whose banner the user has not closed.
// bannerAnnouncement 返回用户尚未关闭横幅的最新重要公告。
export function bannerAnnouncement(view: AnnouncementsView | null): Announcement | null {
  if (!view?.enabled) return null;
  const candidates = view.announcements.filter((announcement) => announcement.priority === "important" && !announcement.dismissed);
  return candidates.reduce<Announcement | null>((newest, announcement) => !newest || Date.parse(announcement.startsAt) > Date.parse(newest.startsAt) ? announcement : newest, null);
}

// startFeedbackCenter reads announcements and, for a signed-in account, the message summary, now and periodically.
// Background failures keep the last state: the badge is a hint, and every page reports its own errors.
// startFeedbackCenter 立即并定期读取公告，已登录时还读取消息摘要。后台失败保留上次状态：徽标只是提示，
// 每个页面各自报告错误。
export function startFeedbackCenter(signedIn: boolean, sinks: FeedbackCenterSinks): FeedbackCenterRuntime {
  let live = true;
  const readSummary = async () => {
    try {
      const next = await loadMessageSummary();
      if (live) sinks.setSummary(next);
    } catch {
      // The badge keeps its last value; the messages page shows the failure when opened.
      // 徽标保留上次的值；打开消息页时会显示失败原因。
    }
  };
  const readAnnouncements = async () => {
    try {
      const next = await listAnnouncements();
      if (live) sinks.setAnnouncements(() => next);
    } catch {
      // Same as above: the announcements page reports its own failure.
      // 同上：公告页自行报告失败。
    }
  };
  void readAnnouncements();
  void readSummary();
  const announcementTimer = setInterval(() => void readAnnouncements(), ANNOUNCEMENT_POLL_MS);
  // Signed-out users have no personal messages, so only a signed-in account is polled.
  // 未登录用户没有个人消息，因此只轮询已登录账户。
  const summaryTimer = signedIn ? setInterval(() => void readSummary(), MESSAGE_SUMMARY_POLL_MS) : undefined;
  const mark = async (ids: string[], kind: "seen" | "dismissed") => {
    if (ids.length === 0) return;
    sinks.setAnnouncements((current) => current ? withAnnouncementMark(current, ids, kind) : current);
    await markAnnouncements(ids, kind === "dismissed");
  };
  return {
    stop: () => {
      live = false;
      clearInterval(announcementTimer);
      if (summaryTimer) clearInterval(summaryTimer);
    },
    refresh: async () => { await Promise.all([readAnnouncements(), readSummary()]); },
    markSeen: (ids) => mark(ids, "seen"),
    dismissBanner: (id) => mark([id], "dismissed"),
  };
}
