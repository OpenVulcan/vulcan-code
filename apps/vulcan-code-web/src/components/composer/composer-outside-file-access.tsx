import React from "react";
import { FolderOpen } from "lucide-react";

import { OUTSIDE_FILE_ACCESS_OPTIONS } from "@/components/file-windows/outside-file-access-options";
import { errorMessage } from "@/lib/display-format";
import { formatMessage, useI18n } from "@/i18n";
import { loadSessionOutsideFileAccess, saveSessionOutsideFileAccess } from "@/service/outside-file-access-service";
import type { OutsideWorkspaceFileAccess, SessionOutsideFileAccess } from "@/types";
import { SegmentedControl, SegmentedControlItem } from "@/ui-kit/primitives/choice-controls";
import { toast } from "@/ui-kit/primitives/toast";

import { SessionSettingsGroup, SessionSettingsRow } from "./composer-session-settings-parts";

// ComposerOutsideFileAccessBlock chooses how this session opens files outside its workspace, or follows the system
// default. The prompt's "don't ask again" saves "allow" here too.
// ComposerOutsideFileAccessBlock 选择本会话如何打开其工作区之外的文件，或跟随系统默认值。提示中的“不再提示”同样在此
// 保存为 "allow"。
// Parameters: conversationId is the persisted session (drafts have none).
// 参数：conversationId 为已持久化会话（草稿没有）。
export function ComposerOutsideFileAccessBlock(props: { conversationId: string | null }) {
  const { t } = useI18n();
  const [snapshot, setSnapshot] = React.useState<SessionOutsideFileAccess | null>(null);
  const [error, setError] = React.useState<string | null>(null);
  const [saving, setSaving] = React.useState(false);
  React.useEffect(() => {
    setSnapshot(null);
    setError(null);
    if (!props.conversationId) return;
    let current = true;
    loadSessionOutsideFileAccess(props.conversationId).then((loaded) => {
      if (current) setSnapshot(loaded);
    }, (reason: unknown) => {
      if (current) setError(errorMessage(reason));
    });
    return () => { current = false; };
  }, [props.conversationId]);
  // apply stores the session's own choice, or null to follow the system default.
  // apply 保存会话自有选择；null 表示跟随系统默认值。
  const apply = async (access: OutsideWorkspaceFileAccess | null) => {
    if (!props.conversationId || !snapshot || access === snapshot.session) return;
    setSaving(true);
    try {
      setSnapshot(await saveSessionOutsideFileAccess(props.conversationId, access));
    } catch (reason) {
      toast.error(t("settings.outsideFiles.saveFailed"), { description: errorMessage(reason) });
    } finally {
      setSaving(false);
    }
  };
  const systemLabel = snapshot ? t(OUTSIDE_FILE_ACCESS_OPTIONS.find((option) => option.value === snapshot.system)?.labelKey ?? "settings.outsideFiles.prompt") : "";
  const description = !props.conversationId
    ? t("sessionSettings.outsideFilesDraft")
    : error ?? (snapshot ? formatMessage(t("sessionSettings.outsideFilesHint"), { label: systemLabel }) : t("workspaceFilePreview.loading"));
  return (
    <SessionSettingsGroup icon={<FolderOpen size={14} />} section="outside-files" title={t("settings.outsideFiles.title")}>
      <SessionSettingsRow description={description}>
        <SegmentedControl aria-busy={saving} aria-label={t("settings.outsideFiles.title")} disabled={!snapshot || saving}
          onValueChange={(value) => void apply(value === "inherit" ? null : value as OutsideWorkspaceFileAccess)} value={snapshot?.session ?? "inherit"}>
          <SegmentedControlItem value="inherit">{t("composer.inheritSystem")}</SegmentedControlItem>
          {OUTSIDE_FILE_ACCESS_OPTIONS.map((option) => (
            <SegmentedControlItem key={option.value} value={option.value}>{t(option.labelKey)}</SegmentedControlItem>
          ))}
        </SegmentedControl>
      </SessionSettingsRow>
    </SessionSettingsGroup>
  );
}
