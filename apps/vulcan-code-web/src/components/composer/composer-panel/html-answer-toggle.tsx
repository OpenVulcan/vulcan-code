import { CodeXml } from "lucide-react";
import { Button } from "@/components/ui/button";
import { useI18n } from "@/i18n";
import type { useComposerHtmlAnswers } from "./use-composer-html-answers";

/** Render a labelled, keyboard-accessible preference button; props supply controller and busy gate.
 * 渲染带文字且可键盘操作的偏好按钮；props 提供控制器和忙碌门控。 */
export function HtmlAnswerToggle(props: {
  /** Owner-scoped HTML preference controller.
   * 按归属限定的 HTML 偏好控制器。 */
  controller: ReturnType<typeof useComposerHtmlAnswers>;
  /** Existing composer mutation barrier.
   * 既有输入区修改屏障。 */
  disabled: boolean;
}) {
  // t resolves button state and assistance from the shared catalog.
  // t 从共享目录解析按钮状态与帮助。
  const { t } = useI18n();
  const { enabled, saving, available, awaitingRequest, toggle } = props.controller;
  return <Button
    aria-pressed={enabled}
    aria-busy={saving}
    disabled={props.disabled || saving || !available}
    onClick={toggle}
    size="sm"
    title={t(awaitingRequest ? "htmlAnswers.pending" : "htmlAnswers.hint")}
    type="button"
    variant={enabled ? "secondary" : "ghost"}
    className={enabled ? "text-primary" : undefined}
  >
    <CodeXml size={15} aria-hidden="true" />
    {t("htmlAnswers.label")}
    {awaitingRequest ? <span aria-label={t("htmlAnswers.pending")}>•</span> : null}
  </Button>;
}
