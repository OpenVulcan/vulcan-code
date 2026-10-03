import type { ReactNode } from "react";

// SettingsPage is the single frame every settings page renders inside. The feature header above carries the page
// title and description, so the frame holds only the scrolling body and every page uses the full pane width.
// SettingsPage 是所有设置页共用的唯一框架。上方功能页头承载页面标题与说明，因此框架只包含滚动正文，
// 且所有页面均使用整个面板宽度。
// Parameters: children is the routed page body.
// 参数：children 为路由到的页面正文。
export function SettingsPage(props: {
  // Page body.
  // 页面正文。
  children: ReactNode;
}) {
  return (
    <section className="settings-page">
      <div className="settings-page-body">{props.children}</div>
    </section>
  );
}
