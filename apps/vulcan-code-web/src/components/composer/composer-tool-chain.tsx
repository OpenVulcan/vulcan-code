import React from "react";
import { Workflow } from "lucide-react";

import { errorMessage } from "@/lib/display-format";
import { useI18n } from "@/i18n";
import { loadSessionToolChain, saveSessionToolChain, type SessionToolChainSnapshot } from "@/service/tool-chain-service";
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from "@/ui-kit/primitives/alert-dialog";
import { SegmentedControl, SegmentedControlItem } from "@/ui-kit/primitives/choice-controls";
import { toast } from "@/ui-kit/primitives/toast";

import { SessionSettingsGroup, SessionSettingsRow } from "./composer-session-settings-parts";

// ComposerToolChainBlock switches the tool_chain tool for this session. Once the session has sent requests, switching
// changes its tool list and breaks the cached context, so the user confirms first and is pointed at compaction.
// ComposerToolChainBlock 为本会话开关 tool_chain 工具。会话发送过请求后，切换会改变工具清单并打破缓存上下文，
// 因此先由用户确认，并建议先压缩。
// Parameters: conversationId is the persisted session (drafts have none).
// 参数：conversationId 为已持久化会话（草稿没有）。
export function ComposerToolChainBlock(props: { conversationId: string | null }) {
  const { t } = useI18n();
  const [snapshot, setSnapshot] = React.useState<SessionToolChainSnapshot | null>(null);
  const [error, setError] = React.useState<string | null>(null);
  const [saving, setSaving] = React.useState(false);
  // confirming holds the requested value while the context-break question is open.
  // confirming 在打破上下文的确认打开期间保存所请求的值。
  const [confirming, setConfirming] = React.useState<boolean | null>(null);
  React.useEffect(() => {
    setSnapshot(null);
    setError(null);
    if (!props.conversationId) return;
    let current = true;
    loadSessionToolChain(props.conversationId).then((loaded) => {
      if (current) setSnapshot(loaded);
    }, (reason: unknown) => {
      if (current) setError(errorMessage(reason));
    });
    return () => { current = false; };
  }, [props.conversationId]);
  // apply stores the session's own choice.
  // apply 保存会话自有选择。
  const apply = async (enabled: boolean) => {
    if (!props.conversationId) return;
    setSaving(true);
    try {
      setSnapshot(await saveSessionToolChain(props.conversationId, enabled));
    } catch (reason) {
      toast.error(t("toolChain.session.saveFailed"), { description: errorMessage(reason) });
    } finally {
      setSaving(false);
    }
  };
  // request asks for confirmation once requests were sent, otherwise applies at once.
  // request 在已发送请求时先请求确认，否则立即应用。
  const request = (enabled: boolean) => {
    if (!snapshot || enabled === snapshot.enabled) return;
    if (snapshot.requestsStarted) setConfirming(enabled);
    else void apply(enabled);
  };
  const description = !props.conversationId
    ? t("toolChain.session.draft")
    : error ?? (snapshot
      ? t(snapshot.followsSystem ? "toolChain.session.followSummary" : "toolChain.session.customSummary")
      : t("toolChain.session.loading"));
  return (
    <SessionSettingsGroup icon={<Workflow size={14} />} section="tool-chain" title={t("toolChain.session.title")}>
      <SessionSettingsRow description={description}>
        <SegmentedControl aria-busy={saving} aria-label={t("toolChain.session.title")} disabled={!snapshot || saving}
          onValueChange={(value) => request(value === "on")} value={snapshot?.enabled ? "on" : "off"}>
          <SegmentedControlItem value="on">{t("toolChain.session.on")}</SegmentedControlItem>
          <SegmentedControlItem value="off">{t("toolChain.session.off")}</SegmentedControlItem>
        </SegmentedControl>
      </SessionSettingsRow>
      <AlertDialog open={confirming !== null} onOpenChange={(open) => { if (!open) setConfirming(null); }}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>{t("toolChain.session.confirmTitle")}</AlertDialogTitle>
            <AlertDialogDescription>{t("toolChain.session.confirmDescription")}</AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>{t("toolChain.session.confirmCancel")}</AlertDialogCancel>
            <AlertDialogAction onClick={() => { if (confirming !== null) void apply(confirming); setConfirming(null); }}>
              {t("toolChain.session.confirm")}
            </AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </SessionSettingsGroup>
  );
}
