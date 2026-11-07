import { expect, it } from "vitest";
import type { Announcement, AnnouncementsView } from "@/service/feedback-service";
import { bannerAnnouncement, withAnnouncementMark } from "./feedback-center-runtime";

// announcement builds one synthetic announcement; only the fields under test vary.
// announcement 构建一条合成公告；只有被测字段不同。
function announcement(id: string, patch: Partial<Announcement> = {}): Announcement {
  return {
    id, kind: "notice", priority: "normal", startsAt: "2026-11-01T00:00:00Z", endsAt: null, locale: "en-US",
    title: id, body: "", link: null, seen: false, dismissed: false, ...patch,
  };
}

// view wraps announcements as an enabled, online document.
// view 将公告包装为已启用、在线的文档。
function view(announcements: Announcement[]): AnnouncementsView {
  return { enabled: true, region: "global", offline: false, announcements, unseen: announcements.filter((item) => !item.seen).length };
}

it("recomputes the unseen count after marking announcements seen", () => {
  const next = withAnnouncementMark(view([announcement("a"), announcement("b"), announcement("c", { seen: true })]), ["a"], "seen");
  expect(next.unseen).toBe(1);
  expect(next.announcements.find((item) => item.id === "a")?.seen).toBe(true);
  expect(next.announcements.find((item) => item.id === "b")?.seen).toBe(false);
});

it("shows the newest important announcement whose banner is still open", () => {
  const announcements = [
    announcement("normal", { startsAt: "2026-11-06T00:00:00Z" }),
    announcement("older", { priority: "important", startsAt: "2026-11-02T00:00:00Z" }),
    announcement("newer", { priority: "important", startsAt: "2026-11-05T00:00:00Z" }),
    announcement("closed", { priority: "important", startsAt: "2026-11-06T00:00:00Z", dismissed: true }),
  ];
  expect(bannerAnnouncement(view(announcements))?.id).toBe("newer");
  expect(bannerAnnouncement(withAnnouncementMark(view(announcements), ["newer", "older"], "dismissed"))).toBeNull();
  expect(bannerAnnouncement({ ...view(announcements), enabled: false })).toBeNull();
  expect(bannerAnnouncement(null)).toBeNull();
});
