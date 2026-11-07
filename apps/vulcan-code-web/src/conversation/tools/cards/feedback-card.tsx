// Feedback card for the AI `feedback` tool, loaded with the first feedback call. A draft shows its kind, title and
// the start of its body, and opens the feedback window where the user reviews the filtered preview and sends it; the
// AI never sends anything. A pull request link shows the address it linked.
// AI `feedback` 工具的反馈卡，随第一次反馈调用加载。草稿展示其类型、标题与正文开头，并打开反馈窗口，由用户在其中
// 检查过滤后的预览并发送；AI 从不发送任何内容。拉取请求关联展示所关联的地址。
import * as React from "react";
import { MessageSquareText } from "lucide-react";
import { useI18n } from "@/i18n";
import { FeedbackCenterContext } from "@/components/features/feedback/feedback-center-context";
import type { FeedbackKind } from "@/service/feedback-service";
import { Button } from "@/ui-kit/primitives/button";
import { ToolFrame, type ToolCardProps } from "../tool-card";
import { objectOf, textOf } from "../tool-call";
import { ErrorNote } from "../views/result-views";

// EXCERPT_CHARS bounds the body excerpt; the full body is edited in the feedback window.
// EXCERPT_CHARS 限定正文摘录长度；完整正文在反馈窗口中编辑。
const EXCERPT_CHARS = 280;

// FeedbackDraft is the reviewed draft a successful `draft` call returned.
// FeedbackDraft 是成功的 `draft` 调用返回、待检查的草稿。
export interface FeedbackDraft {
  kind: FeedbackKind;
  title: string;
  body: string;
}

// feedbackDraft reads the draft from the host result; anything else, including a failed call, yields null.
// feedbackDraft 从宿主结果读取草稿；其他任何情况（含失败调用）返回 null。
export function feedbackDraft(call: ToolCardProps["call"]): FeedbackDraft | null {
  if (call.isError || call.host?.kind !== "feedback" || call.host.payload.status !== "awaiting_user") return null;
  const draft = objectOf(call.host.payload.draft);
  const kind = textOf(draft?.kind);
  const title = textOf(draft?.title);
  const body = textOf(draft?.body);
  return (kind === "bug" || kind === "suggestion") && title && body ? { body, kind, title } : null;
}

// excerpt shortens the body to one paragraph-sized preview.
// excerpt 将正文缩短为一段大小的预览。
function excerpt(body: string): string {
  const text = body.replace(/\s+/g, " ").trim();
  return text.length > EXCERPT_CHARS ? `${text.slice(0, EXCERPT_CHARS).trimEnd()}…` : text;
}

/** Renders one `feedback` call; a draft starts open so its review button is visible.
 * 渲染一次 `feedback` 调用；草稿默认展开，使其检查按钮可见。 */
export default function FeedbackCard(props: ToolCardProps) {
  const { t } = useI18n();
  const center = React.useContext(FeedbackCenterContext);
  const { call } = props;
  const draft = feedbackDraft(call);
  const link = textOf(call.input.action) === "link_pr";
  // userOpen keeps the user's own toggle; until then a draft is open.
  // userOpen 保存用户自己的展开选择；在此之前草稿保持展开。
  const [userOpen, setUserOpen] = React.useState<boolean | null>(null);
  const onOpenChange = React.useCallback((next: boolean) => {
    setUserOpen(next);
    props.onOpenChange(next);
  }, [props]);
  const target = link ? textOf(call.input.url) ?? "" : draft?.title ?? textOf(call.input.title) ?? "";
  return (
    <ToolFrame
      {...props}
      icon={MessageSquareText}
      onOpenChange={onOpenChange}
      open={userOpen ?? (props.open || draft !== null)}
      state={call.state}
      target={target}
      targetTitle={target}
      verb={t(link ? "toolCard.feedback.linkPr" : "toolCard.feedback.draft")}
    >
      {call.isError ? <ErrorNote text={call.modelOutput} /> : draft ? (
        <>
          <p className="vk-tool-note">{excerpt(draft.body)}</p>
          <div className="vk-tool-foot">
            <span>{t(draft.kind === "bug" ? "feedback.kind.bug" : "feedback.kind.suggestion")} · {t("toolCard.feedback.notSent")}</span>
            {center ? (
              <Button
                onClick={() => center.openFeedback({ body: draft.body, kind: draft.kind, source: "ai_draft", title: draft.title })}
                size="xs"
                type="button"
                variant="primary"
              >
                {t("toolCard.feedback.review")}
              </Button>
            ) : null}
          </div>
        </>
      ) : null}
    </ToolFrame>
  );
}
