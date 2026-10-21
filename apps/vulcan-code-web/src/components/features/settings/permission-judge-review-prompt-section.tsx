import * as React from "react";
import { toast } from "@/ui-kit/primitives/toast";
import { PageNotice, PageRow, PageSection } from "@/components/page";
import { Button } from "@/ui-kit/primitives/button";
import { StreamPlaceholder } from "@/ui-kit/primitives/stream-placeholder";
import { Textarea } from "@/ui-kit/primitives/textarea";
import { useI18n } from "@/i18n";
import { errorMessage } from "@/lib/display-format";
import { loadReviewPromptSystemDefault, saveReviewPromptSystemDefault } from "@/service/review-prompt-service";
import type { ReviewPromptSystemDefault } from "@/types";
import "@/ui-kit/styles/features/settings-forms.css"

/** System default additional review prompt: text the reviewer reads while the user is present, used by every
 * session that keeps no prompt of its own. The next review reads the saved text.
 * 系统默认附加复核提示词：用户在场时复核器读取的文本，供所有没有自有提示词的会话使用。下一次复核即读取
 * 已保存的文本。 */
export function ReviewPromptSystemSection() {
  const { t } = useI18n();
  const [snapshot, setSnapshot] = React.useState<ReviewPromptSystemDefault | null>(null);
  const [text, setText] = React.useState("");
  const [loadError, setLoadError] = React.useState<string | null>(null);
  const [saving, setSaving] = React.useState(false);
  const load = React.useCallback(async () => {
    setLoadError(null);
    try {
      const loaded = await loadReviewPromptSystemDefault();
      setSnapshot(loaded);
      setText(loaded.prompt);
    } catch (error) {
      setLoadError(errorMessage(error));
    }
  }, []);
  React.useEffect(() => { void load(); }, [load]);
  const save = async () => {
    setSaving(true);
    try {
      const saved = await saveReviewPromptSystemDefault(text.trim());
      setSnapshot(saved);
      setText(saved.prompt);
      toast.success(t("settings.permissionJudge.reviewPrompt.saved"));
    } catch (error) {
      toast.error(t("settings.permissionJudge.reviewPrompt.saveFailed"), { description: errorMessage(error) });
    } finally {
      setSaving(false);
    }
  };
  return <PageSection layout="rows" data-review-prompt-system="" title={t("settings.permissionJudge.reviewPrompt.systemTitle")}
    description={t("settings.permissionJudge.reviewPrompt.systemHint")}
    footer={snapshot ? <Button type="button" size="sm" data-review-prompt-system-save=""
      disabled={saving || text.trim() === snapshot.prompt || [...text.trim()].length > snapshot.maxChars} onClick={() => void save()}>
      {t("settings.permissionJudge.reviewRules.save")}
    </Button> : undefined}>
    {loadError ? <PageNotice tone="error" details={loadError} detailsLabel={t("settings.errors.showErrorDetails")}
      actions={<Button type="button" size="sm" variant="outline" onClick={() => void load()}>{t("composer.capability.retry")}</Button>}>
      {t("settings.permissionJudge.reviewPrompt.loadFailed")}
    </PageNotice> : !snapshot ? <StreamPlaceholder label={t("settings.permissionJudge.reviewRules.loading")} shape="rows" count={1} /> : (
      <PageRow label={t("settings.permissionJudge.reviewPrompt.prompt")} stacked>
        <Textarea aria-label={t("settings.permissionJudge.reviewPrompt.systemTitle")} density="large" maxLength={snapshot.maxChars}
          placeholder={t("permissionMode.reviewPrompt.placeholder")} value={text} onChange={(event) => setText(event.target.value)} />
      </PageRow>
    )}
  </PageSection>;
}
