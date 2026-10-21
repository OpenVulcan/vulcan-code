import React from "react";
import {
  AlertDialog,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/ui-kit/primitives/alert-dialog";
import { Button } from "@/ui-kit/primitives/button";
import { Textarea } from "@/ui-kit/primitives/textarea";
import { toast } from "@/ui-kit/primitives/toast";
import { formatMessage, useI18n } from "@/i18n";
import { errorMessage } from "@/lib/display-format";
import { loadSessionReviewPrompt, saveSessionReviewPrompt } from "@/service/review-prompt-service";
import type { SessionReviewPrompt } from "@/types";

/** Dialog that edits one session's additional review prompt: the text the reviewer reads while the user is
 * present. A session follows the system default until it saves its own text, and can return to it.
 * 编辑一个会话附加复核提示词的对话框：用户在场时复核器读取的文本。会话在保存自有文本之前跟随系统默认值，
 * 也可以恢复跟随。 */
export function ComposerReviewPromptDialog(props: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  sessionId: string | null;
}) {
  const { t } = useI18n();
  const fieldId = React.useId();
  const [snapshot, setSnapshot] = React.useState<SessionReviewPrompt | null>(null);
  const [text, setText] = React.useState("");
  const [loadFailed, setLoadFailed] = React.useState(false);
  const [saving, setSaving] = React.useState(false);
  const { open, sessionId } = props;

  // Opening reads the session's own prompt, else the system default it follows.
  // 打开时读取会话自有提示词，否则读取其跟随的系统默认值。
  React.useEffect(() => {
    if (!open || sessionId === null) return;
    let cancelled = false;
    setSnapshot(null);
    setLoadFailed(false);
    setText("");
    void loadSessionReviewPrompt(sessionId).then((loaded) => {
      if (cancelled) return;
      setSnapshot(loaded);
      setText(loaded.prompt ?? loaded.systemPrompt);
    }).catch(() => {
      if (!cancelled) setLoadFailed(true);
    });
    return () => { cancelled = true; };
  }, [open, sessionId]);

  const save = async (prompt: string | null) => {
    if (sessionId === null) return;
    setSaving(true);
    try {
      const saved = await saveSessionReviewPrompt(sessionId, prompt);
      setSnapshot(saved);
      props.onOpenChange(false);
    } catch (error) {
      toast.error(t("permissionMode.reviewPrompt.saveFailed"), { description: errorMessage(error) });
    } finally {
      setSaving(false);
    }
  };
  const custom = snapshot?.prompt != null;
  const saved = snapshot === null ? null : snapshot.prompt ?? snapshot.systemPrompt;
  const tooLong = snapshot !== null && [...text.trim()].length > snapshot.maxChars;

  return (
    <AlertDialog open={open} onOpenChange={props.onOpenChange}>
      <AlertDialogContent size="md" data-review-prompt-dialog="">
        <AlertDialogHeader>
          <AlertDialogTitle>{t("permissionMode.reviewPrompt.title")}</AlertDialogTitle>
          <AlertDialogDescription>{t("permissionMode.reviewPrompt.body")}</AlertDialogDescription>
        </AlertDialogHeader>
        <div className="vk-field">
          <div className="vk-field-row">
            <label className="vk-field-label" htmlFor={fieldId}>{t("permissionMode.reviewPrompt.label")}</label>
            {snapshot !== null ? <span className="vk-field-hint" data-review-prompt-source={custom ? "session" : "system"}>
              {t(custom ? "permissionMode.reviewPrompt.sourceSession" : "permissionMode.reviewPrompt.sourceSystem")}
            </span> : null}
          </div>
          <Textarea
            id={fieldId}
            data-review-prompt-text=""
            disabled={snapshot === null}
            maxLength={snapshot?.maxChars}
            placeholder={t("permissionMode.reviewPrompt.placeholder")}
            rows={4}
            value={text}
            onChange={(event) => setText(event.target.value)}
          />
          <div className="vk-field-row">
            <span className="vk-field-hint" data-tone="warn">{loadFailed ? t("permissionMode.reviewPrompt.loadFailed") : null}</span>
            {snapshot !== null ? <span className="vk-field-hint" data-numeric="">
              {formatMessage(t("permissionMode.unattended.counter"), { count: [...text].length, max: snapshot.maxChars })}
            </span> : null}
          </div>
        </div>
        <AlertDialogFooter>
          <AlertDialogCancel>{t("permissionMode.cancel")}</AlertDialogCancel>
          {custom ? <Button type="button" variant="outline" data-review-prompt-follow="" disabled={saving}
            onClick={() => void save(null)}>
            {t("permissionMode.reviewPrompt.followSystem")}
          </Button> : null}
          <Button type="button" data-review-prompt-save="" disabled={snapshot === null || saving || tooLong || text.trim() === saved}
            onClick={() => void save(text.trim())}>
            {t("permissionMode.reviewPrompt.save")}
          </Button>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}
