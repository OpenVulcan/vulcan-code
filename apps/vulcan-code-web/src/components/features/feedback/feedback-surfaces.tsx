import * as React from "react";
import AnnouncementBanner from "./announcement-banner";
import { FeedbackCenterContext } from "./feedback-center-context";
import FeedbackDialog from "./feedback-dialog";
import MessageCenterSheet from "./message-center-sheet";

// FeedbackSurfaces renders the open feedback window, the open message center and the announcement banner. The
// workbench loads them as one deferred module so the first screen carries a single loader for all three.
// FeedbackSurfaces 渲染已打开的反馈窗口、已打开的消息中心与公告横幅。工作台把它们作为一个延迟模块加载，
// 使首屏只为三者携带一个加载器。
export default function FeedbackSurfaces() {
  const center = React.useContext(FeedbackCenterContext);
  if (!center) return null;
  return (
    <>
      {center.feedbackSeed ? <FeedbackDialog /> : null}
      {center.messageCenterTab ? <MessageCenterSheet /> : null}
      <AnnouncementBanner />
    </>
  );
}
