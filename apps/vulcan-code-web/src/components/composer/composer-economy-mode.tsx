import React from "react";
import { Gauge, Settings2 } from "lucide-react";
import { useI18n } from "@/i18n";
import { serviceClient } from "@/service/webui-service-runtime";
import type { ConversationEventLiveDeltaV1 } from "@/service/conversation-event-window-client";
import { Button } from "@/ui-kit/primitives/button";
import { SegmentedControl, SegmentedControlItem } from "@/ui-kit/primitives/choice-controls";
import { SessionSettingsGroup, SessionSettingsRow } from "./composer-session-settings-parts";
import { EconomySettingsNavigation } from "./composer-settings-host";

/** Committed preference and its optimistic concurrency token.
 * 已提交偏好及其并发更新修订。 */
interface Preference {
  enabled: boolean;
  pending: boolean;
  revision: number;
}

/** Displays the draft selection or reads and updates the durable preference for the supplied session.
 * 显示草稿选择，或读取并更新指定会话的持久偏好。 */
export function ComposerEconomyMode(props: { sessionId: string | null; draftEnabled?: boolean; setDraft?: (enabled: boolean) => void }) {
  const { t } = useI18n();
  const navigate = React.useContext(EconomySettingsNavigation);
  const [preference, setPreference] = React.useState<Preference>();
  const [busy, setBusy] = React.useState(false);
  const [error, setError] = React.useState("");
  const sessionId = props.sessionId;
  React.useEffect(() => {
    if (!sessionId) return;
    let mounted = true;
    let refreshTimer: ReturnType<typeof setTimeout> | undefined;
    /** Reloads committed state after a durable notification while the session settings are visible.
     * 会话设置打开时，在可靠发布通知之后重新读取已提交状态。 */
    async function refresh() {
      try {
        const value = await serviceClient.request<Preference>("service_session_economy_mode", { sessionId });
        if (mounted) setPreference((current) => current && (current.revision > value.revision
          || (current.revision === value.revision && !current.pending && value.pending)) ? current : value);
      } catch (reason) { if (mounted) setError(String(reason)); }
    }
    const subscription = serviceClient.subscribe<ConversationEventLiveDeltaV1>("conversation.event.delta.v1", (event) => {
      if (event.session_id !== sessionId || event.durability !== "durable" || refreshTimer !== undefined) return;
      refreshTimer = setTimeout(() => { refreshTimer = undefined; void refresh(); }, 150);
    });
    void subscription.then(refresh, (reason: unknown) => {
      if (mounted) { setError(String(reason)); void refresh(); }
    });
    return () => {
      mounted = false;
      clearTimeout(refreshTimer);
      void subscription.then((unsubscribe) => unsubscribe()).catch(() => {});
    };
  }, [sessionId]);

  /** Switches a materialized conversation using its observed revision, or updates only the draft.
   * 按已观察修订切换持久会话，或仅更新当前草稿。 */
  async function change(enabled: boolean) {
    if (!sessionId) { props.setDraft?.(enabled); return; }
    if (!preference || busy || preference.enabled === enabled) return;
    setBusy(true);
    setError("");
    try {
      setPreference(await serviceClient.request<Preference>("service_set_session_economy_mode", {
        sessionId, enabled, expectedRevision: preference.revision,
      }));
    } catch (reason) {
      setError(String(reason));
      try { setPreference(await serviceClient.request<Preference>("service_session_economy_mode", { sessionId })); }
      catch (reloadError) { setError(String(reloadError)); }
    } finally { setBusy(false); }
  }

  return <SessionSettingsGroup icon={<Gauge size={14} />} section="economy" title={t("economy.title")}
    action={navigate && <Button className="vk-ss-sub-action" onClick={navigate} size="xs" type="button" variant="ghost"><Settings2 size={13} />{t("economy.settings")}</Button>}>
    <SessionSettingsRow description={preference?.pending ? t("economy.pending") : t("economy.switchHint")}>
      <SegmentedControl aria-label={t("economy.title")} disabled={busy || (sessionId ? !preference : !props.setDraft)}
        value={(sessionId ? preference?.enabled : props.draftEnabled) ? "economy" : "normal"} onValueChange={(value) => void change(value === "economy")}>
        <SegmentedControlItem value="normal">{t("economy.disabled")}</SegmentedControlItem>
        <SegmentedControlItem value="economy">{t("economy.enabled")}</SegmentedControlItem>
      </SegmentedControl>
    </SessionSettingsRow>
    {error && <p className="vk-field-error" role="alert">{error}</p>}
  </SessionSettingsGroup>;
}
