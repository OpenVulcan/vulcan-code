import * as React from "react";
import { useI18n } from "@/i18n";
import type { HtmlAnswerCard as CardComponent } from "./html-answer-card";

// Ordinary Markdown never loads the viewer controls, authorization adapter or card stylesheet.
// 普通 Markdown 绝不加载查看控制、授权适配器或卡片样式。
const DeferredCard = React.lazy(() => import("./html-answer-card").then((module) => ({ default: module.HtmlAnswerCard })));

/** Defer the HTML-only surface behind a local placeholder; never suspend the surrounding transcript.
 * 通过局部占位延迟加载 HTML 专属界面；绝不挂起周围会话。
 * Inputs mirror the real card; returns a nonexecuting label until its module is ready.
 * 输入镜像真实卡片；模块就绪前返回不执行页面的标签。 */
export function HtmlAnswerLazyCard(props: React.ComponentProps<typeof CardComponent>) {
  const { t } = useI18n();
  return <HtmlCardLoadBoundary fallback={<div role="alert"><p>{t("htmlAnswers.cannotOpen")}</p>
    <details><summary>{t("htmlAnswers.source")}</summary><pre>{props.source}</pre></details></div>}>
    <React.Suspense fallback={<p role="status">{t("htmlAnswers.label")}</p>}><DeferredCard {...props} /></React.Suspense>
  </HtmlCardLoadBoundary>;
}

/** Contain chunk or card failures locally; a missing optional renderer cannot erase the transcript.
 * 将分块或卡片失败限制在局部；可选渲染器缺失不能抹去会话。
 * Props supply inert source fallback and children; state records only whether this card failed.
 * 属性提供不执行的源码回退及子节点；状态仅记录当前卡片是否失败。 */
class HtmlCardLoadBoundary extends React.Component<{ fallback: React.ReactNode; children: React.ReactNode }, { failed: boolean }> {
  /** Failure state belongs to this keyed source lifetime.
   * 失败状态属于当前带身份键的源码生命周期。 */
  state = { failed: false };

  /** Capture failure without persisting or logging its possibly sensitive details.
   * 捕获失败，不保存或记录可能敏感的错误详情。 */
  static getDerivedStateFromError(): { failed: boolean } { return { failed: true }; }

  /** Return the original plaintext fallback after failure, otherwise the deferred card.
   * 失败后返回原始纯文本回退，否则返回延迟卡片。 */
  render(): React.ReactNode { return this.state.failed ? this.props.fallback : this.props.children; }
}
