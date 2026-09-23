import type { ReactNode } from "react";

// SettingsPageWidth selects the content measure: forms read best narrow, lists and tables use the full pane.
// SettingsPageWidth 选择内容宽度：表单窄栏更易读，列表与表格使用整个面板。
export type SettingsPageWidth = "narrow" | "medium" | "wide";

// SettingsPage is the single frame every settings page renders inside: title, one-line description, optional actions, body.
// SettingsPage 是所有设置页共用的唯一框架：标题、一句说明、可选操作与正文。
// Parameters: title and description come from the navigation registry; width picks the content measure.
// 参数：title 与 description 来自导航注册表；width 选择内容宽度。
export function SettingsPage(props: {
  // Page heading (h1).
  // 页面标题（h1）。
  title: string;
  // One-line explanation under the heading.
  // 标题下方的一句说明。
  description: string;
  // Content measure; defaults to medium.
  // 内容宽度，默认 medium。
  width?: SettingsPageWidth;
  // Page-level actions aligned with the heading.
  // 与标题对齐的页面级操作。
  actions?: ReactNode;
  // Page body.
  // 页面正文。
  children: ReactNode;
}) {
  return (
    <section className="settings-page" data-width={props.width ?? "medium"}>
      <header className="settings-page-header">
        <div className="settings-page-heading">
          <h1>{props.title}</h1>
          <p>{props.description}</p>
        </div>
        {props.actions ? <div className="settings-page-actions">{props.actions}</div> : null}
      </header>
      <div className="settings-page-body">{props.children}</div>
    </section>
  );
}
