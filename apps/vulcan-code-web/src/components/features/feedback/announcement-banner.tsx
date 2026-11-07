import * as React from "react";
import { Megaphone, X } from "lucide-react";
import { Button } from "@/ui-kit/primitives/button";
import { useI18n } from "@/i18n";
import { FeedbackCenterContext } from "./feedback-center-context";
import { bannerAnnouncement } from "./feedback-center-runtime";
import "@/ui-kit/styles/features/feedback.css";

// AnnouncementBanner shows the newest important announcement above the workbench until the user closes it; the full
// text lives in the message center. It renders nothing when no announcement qualifies.
// AnnouncementBanner 在工作台上方显示最新的重要公告，直到用户关闭；完整内容位于消息中心。没有符合条件的公告时不渲染。
export default function AnnouncementBanner() {
  const center = React.useContext(FeedbackCenterContext);
  const { t } = useI18n();
  const announcement = bannerAnnouncement(center?.announcements ?? null);
  if (!center || !announcement) return null;
  return (
    <div className="vk-announcement-banner" role="status">
      <Megaphone aria-hidden="true" />
      <strong>{announcement.title}</strong>
      <Button size="xs" variant="outline" onClick={() => center.openMessageCenter("announcements")}>{t("messageCenter.details")}</Button>
      <Button size="icon-xs" variant="ghost" aria-label={t("messageCenter.closeBanner")}
        onClick={() => void center.runtime.current?.dismissBanner(announcement.id).catch(() => undefined)}>
        <X aria-hidden="true" />
      </Button>
    </div>
  );
}
