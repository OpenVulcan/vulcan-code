import * as React from "react";
import { Bell, MessageSquarePlus } from "lucide-react";
import { PageBody, PageRow, PageSection } from "@/components/page";
import { SettingsErrorNotice } from "@/components/features/settings/settings-error-notice";
import { Button } from "@/ui-kit/primitives/button";
import { Switch } from "@/ui-kit/primitives/switch";
import { useI18n } from "@/i18n";
import { errorMessage } from "@/lib/display-format";
import { loadMessageCenterSettings, saveMessageCenterSettings, type MessageCenterSettings } from "@/service/feedback-service";
import { FeedbackCenterContext } from "./feedback-center-context";

// FeedbackSettingsArea holds the message-center choice and the entries to the feedback window and message center.
// FeedbackSettingsArea 承载消息中心选项，以及进入反馈窗口与消息中心的入口。
export function FeedbackSettingsArea() {
  const { t } = useI18n();
  const center = React.useContext(FeedbackCenterContext);
  const [settings, setSettings] = React.useState<MessageCenterSettings | null>(null);
  const [busy, setBusy] = React.useState(false);
  const [error, setError] = React.useState("");

  React.useEffect(() => {
    let live = true;
    loadMessageCenterSettings()
      .then((next) => { if (live) setSettings(next); })
      .catch((cause) => { if (live) setError(errorMessage(cause)); });
    return () => { live = false; };
  }, []);

  const save = async (next: MessageCenterSettings) => {
    setBusy(true);
    setError("");
    try {
      setSettings(await saveMessageCenterSettings(next));
      await center?.runtime.current?.refresh();
    } catch (cause) {
      setError(errorMessage(cause));
    } finally {
      setBusy(false);
    }
  };

  return (
    <PageBody>
      {error ? <SettingsErrorNotice message={error} /> : null}
      <PageSection layout="rows" title={t("settings.feedbackMessages.messages")} description={t("settings.feedbackMessages.messagesDescription")}>
        <PageRow label={t("settings.feedbackMessages.receiveAnnouncements")} description={t("settings.feedbackMessages.receiveAnnouncementsDescription")}>
          <Switch aria-label={t("settings.feedbackMessages.receiveAnnouncements")} checked={settings?.receiveAnnouncements ?? false}
            disabled={busy || !settings} onCheckedChange={(checked) => void save({ receiveAnnouncements: checked })} />
        </PageRow>
        {center ? (
          <PageRow label={t("messageCenter.title")} description={t("messageCenter.description")}>
            <Button size="sm" variant="outline" onClick={() => center.openMessageCenter()}><Bell aria-hidden="true" />{t("settings.feedbackMessages.openMessageCenter")}</Button>
          </PageRow>
        ) : null}
      </PageSection>
      {center ? (
        <PageSection layout="rows" title={t("settings.feedbackMessages.feedback")} description={t("feedback.privacy")}>
          <PageRow label={t("feedback.menu")} description={t("feedback.description")}>
            <Button size="sm" variant="outline" onClick={() => center.openFeedback()}><MessageSquarePlus aria-hidden="true" />{t("feedback.menu")}</Button>
          </PageRow>
        </PageSection>
      ) : null}
    </PageBody>
  );
}
