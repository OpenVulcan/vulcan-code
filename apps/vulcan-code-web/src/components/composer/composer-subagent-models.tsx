import React from "react";
import { Bot, RotateCcw, Settings2 } from "lucide-react";

import { errorMessage } from "@/lib/display-format";
import { formatMessage, useI18n } from "@/i18n";
import { loadSessionSubagentModels, saveSessionSubagentModels } from "@/service/subagent-models-service";
import type { SessionSubagentModelsSnapshot } from "@/types";
import { Button } from "@/ui-kit/primitives/button";
import { SegmentedControl, SegmentedControlItem } from "@/ui-kit/primitives/choice-controls";
import { toast } from "@/ui-kit/primitives/toast";

import { SessionSettingsGroup, SessionSettingsRow } from "./composer-session-settings-parts";

// ComposerSubagentModelsBlock is the session's quick allowlist setting: follow the system list or keep its own,
// with editing in a dialog and a one-click return to the system default.
// ComposerSubagentModelsBlock 是会话的白名单快速设置：跟随系统清单或保留自己的清单，在对话框中编辑，并可一键恢复系统默认。
// Parameters: conversationId is the persisted session (drafts have none); onEdit opens the editor dialog.
// 参数：conversationId 为已持久化会话（草稿没有）；onEdit 打开编辑对话框。
export function ComposerSubagentModelsBlock(props: { conversationId: string | null; onEdit: () => void }) {
  const { t } = useI18n();
  const [snapshot, setSnapshot] = React.useState<SessionSubagentModelsSnapshot | null>(null);
  const [error, setError] = React.useState<string | null>(null);
  const [saving, setSaving] = React.useState(false);
  React.useEffect(() => {
    if (!props.conversationId) return;
    let current = true;
    loadSessionSubagentModels(props.conversationId).then((loaded) => {
      if (current) setSnapshot(loaded);
    }, (reason: unknown) => {
      if (current) setError(errorMessage(reason));
    });
    return () => { current = false; };
  }, [props.conversationId]);
  // apply saves the session's own list, or null to follow the system list again.
  // apply 保存会话自有清单；null 表示重新跟随系统清单。
  const apply = async (custom: boolean) => {
    if (!props.conversationId || !snapshot) return;
    setSaving(true);
    try {
      const saved = await saveSessionSubagentModels(props.conversationId, custom ? snapshot.systemModels : null, custom ? snapshot.systemIncludeSessionModel : null);
      setSnapshot(saved);
      if (!custom) toast.success(t("subagentModels.session.restored"));
    } catch (reason) {
      toast.error(t("subagentModels.saveFailed"), { description: errorMessage(reason) });
    } finally {
      setSaving(false);
    }
  };
  const custom = snapshot?.models != null;
  const effective = snapshot ? snapshot.models ?? snapshot.systemModels : [];
  const description = !props.conversationId
    ? t("subagentModels.session.draft")
    : error ?? (snapshot
      ? formatMessage(t(custom ? "subagentModels.session.customSummary" : "subagentModels.session.followSummary"), { count: effective.length })
      : t("subagentModels.loading"));
  return (
    <SessionSettingsGroup
      action={snapshot ? (
        <Button className="vk-ss-sub-action" onClick={props.onEdit} size="xs" type="button" variant="ghost">
          <Settings2 size={13} />{t("subagentModels.session.edit")}
        </Button>
      ) : undefined}
      count={snapshot ? effective.length : undefined}
      icon={<Bot size={14} />}
      section="subagents"
      title={t("subagentModels.session.title")}
    >
      <SessionSettingsRow description={description}>
        <SegmentedControl aria-busy={saving} aria-label={t("subagentModels.session.title")} disabled={!snapshot || saving}
          onValueChange={(value) => void apply(value === "custom")} value={custom ? "custom" : "inherit"}>
          <SegmentedControlItem value="inherit">{t("subagentModels.session.follow")}</SegmentedControlItem>
          <SegmentedControlItem value="custom">{t("subagentModels.session.custom")}</SegmentedControlItem>
        </SegmentedControl>
      </SessionSettingsRow>
      {custom ? (
        <div className="vk-ss-actions">
          <Button disabled={saving} onClick={() => void apply(false)} size="xs" type="button" variant="ghost">
            <RotateCcw size={13} />{t("subagentModels.session.restore")}
          </Button>
        </div>
      ) : null}
    </SessionSettingsGroup>
  );
}
