import * as React from "react";
import { RotateCcw } from "lucide-react";
import { PageNotice } from "@/components/page";
import { errorMessage } from "@/lib/display-format";
import { useI18n } from "@/i18n";
import { loadSessionSubagentModels, saveSessionSubagentModels } from "@/service/subagent-models-service";
import type { ProviderSummary, SessionSubagentModelsSnapshot, SubagentModelEntry } from "@/types";
import { Button } from "@/ui-kit/primitives/button";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/ui-kit/primitives/dialog";
import { StreamPlaceholder } from "@/ui-kit/primitives/stream-placeholder";
import { toast } from "@/ui-kit/primitives/toast";
import {
  sameSubagentModels,
  SubagentModelEditor,
  subagentModelNotesFit,
  trimmedSubagentModels,
  useSubagentModelDirectory,
} from "./subagent-model-editor";

// SubagentModelsSessionDialogProps names the session being edited and the model directory.
// SubagentModelsSessionDialogProps 指明正在编辑的会话与模型目录。
export interface SubagentModelsSessionDialogProps {
  conversationId: string;
  providers: ProviderSummary[];
  // Display name of the model this session currently runs.
  // 本会话当前所用模型的显示名称。
  sessionModelLabel?: string;
  onClose: () => void;
}

/** Edits one session's own allowlist. A session that follows the system list starts from a copy of it, and
 * saving turns it into the session's own list; restoring returns the session to the system list.
 * 编辑一个会话的自有白名单。跟随系统清单的会话从其副本开始，保存后成为会话自有清单；恢复则使会话重新跟随系统清单。 */
export default function SubagentModelsSessionDialog(props: SubagentModelsSessionDialogProps) {
  const { t } = useI18n();
  const directory = useSubagentModelDirectory(props.providers);
  const [snapshot, setSnapshot] = React.useState<SessionSubagentModelsSnapshot | null>(null);
  const [models, setModels] = React.useState<SubagentModelEntry[]>([]);
  // Keep the inclusion draft paired with its list until the whole policy is saved.
  // 在完整策略保存之前，包含草稿始终与对应清单配对。
  const [includeSessionModel, setIncludeSessionModel] = React.useState(false);
  const [loadError, setLoadError] = React.useState<string | null>(null);
  const [saving, setSaving] = React.useState(false);
  const load = React.useCallback(async () => {
    setLoadError(null);
    try {
      const loaded = await loadSessionSubagentModels(props.conversationId);
      setSnapshot(loaded);
      setModels(loaded.models ?? loaded.systemModels);
      setIncludeSessionModel(loaded.includeSessionModel ?? loaded.systemIncludeSessionModel);
    } catch (error) {
      setLoadError(errorMessage(error));
    }
  }, [props.conversationId]);
  React.useEffect(() => { void load(); }, [load]);
  const save = async (next: SubagentModelEntry[] | null) => {
    setSaving(true);
    try {
      await saveSessionSubagentModels(props.conversationId, next === null ? null : trimmedSubagentModels(next), next === null ? null : includeSessionModel);
      toast.success(t(next === null ? "subagentModels.session.restored" : "subagentModels.saved"));
      props.onClose();
    } catch (error) {
      toast.error(t("subagentModels.saveFailed"), { description: errorMessage(error) });
    } finally {
      setSaving(false);
    }
  };
  const following = snapshot !== null && snapshot.models === null;
  // A following session can always be saved, since saving gives it its own list; a custom one only when changed.
  // 跟随系统的会话总可保存，因为保存会赋予其自有清单；自定义会话只在有改动时可保存。
  const changed = snapshot !== null && (following || includeSessionModel !== snapshot.includeSessionModel || !sameSubagentModels(models, snapshot.models ?? []));
  const fits = snapshot === null || subagentModelNotesFit(models, snapshot.maxNoteChars);
  return (
    <Dialog open onOpenChange={(open) => { if (!open) props.onClose(); }}>
      <DialogContent data-subagent-models-session="" scrollMode="fixed-footer" size="lg">
        <DialogHeader>
          <DialogTitle>{t("subagentModels.session.dialogTitle")}</DialogTitle>
          <DialogDescription>{t("subagentModels.session.dialogDetail")}</DialogDescription>
        </DialogHeader>
        <div className="vk-sam-dialog-body vk-dialog-body">
          {loadError ? (
            <PageNotice actions={<Button onClick={() => void load()} size="sm" type="button" variant="outline">{t("composer.capability.retry")}</Button>}
              details={loadError} detailsLabel={t("settings.errors.showErrorDetails")} tone="error">
              {t("subagentModels.loadFailed")}
            </PageNotice>
          ) : !snapshot ? (
            <StreamPlaceholder count={3} label={t("subagentModels.loading")} shape="rows" />
          ) : <>
            {following ? (
              <div className="vk-sam-follow" data-sam-follows="">
                <span>{t("subagentModels.session.followNotice")}</span>
                {snapshot.systemModels.length > 0 ? (
                  <ul aria-label={t("subagentModels.system.title")}>
                    {snapshot.systemModels.map((entry) => {
                      const key = `${entry.providerId}/${entry.modelId}`;
                      return <li key={key}>{directory.get(key)?.modelName ?? entry.modelId}</li>;
                    })}
                  </ul>
                ) : null}
              </div>
            ) : null}
            <SubagentModelEditor disabled={saving} maxModels={snapshot.maxModels} maxNoteChars={snapshot.maxNoteChars}
              includeSessionModel={includeSessionModel} onIncludeSessionModelChange={setIncludeSessionModel}
              models={models} onChange={setModels} providers={props.providers} scope="session" sessionModelLabel={props.sessionModelLabel} />
          </>}
        </div>
        <DialogFooter>
          {snapshot && !following ? (
            <Button disabled={saving} onClick={() => void save(null)} size="sm" type="button" variant="ghost">
              <RotateCcw size={14} />{t("subagentModels.session.restore")}
            </Button>
          ) : null}
          <Button disabled={saving} onClick={props.onClose} size="sm" type="button" variant="outline">{t("subagentModels.session.cancel")}</Button>
          <Button data-subagent-models-session-save="" disabled={!changed || !fits || saving} onClick={() => void save(models)} size="sm" type="button">
            {following ? t("subagentModels.session.saveCustom") : t("subagentModels.save")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
