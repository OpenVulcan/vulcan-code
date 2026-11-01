import * as React from "react";
import { PageActionBar, PageBody, PageGuide, PageNotice, PageSection } from "@/components/page";
import { errorMessage } from "@/lib/display-format";
import { formatMessage, useI18n } from "@/i18n";
import { loadSubagentModels, saveSubagentModels } from "@/service/subagent-models-service";
import type { ProviderSummary, SubagentModelEntry, SubagentModelsSnapshot } from "@/types";
import { Badge } from "@/ui-kit/primitives/badge";
import { Button } from "@/ui-kit/primitives/button";
import { StreamPlaceholder } from "@/ui-kit/primitives/stream-placeholder";
import { toast } from "@/ui-kit/primitives/toast";
import {
  sameSubagentModels,
  SubagentModelEditor,
  subagentModelNotesFit,
  trimmedSubagentModels,
} from "./subagent-model-editor";

/** Edits the system policy used by sessions without an override; provider props supply model choices.
 * 编辑未设置覆盖的会话所使用的系统策略；供应商参数提供模型选项。
 * Returns policy controls; inclusion and grants are read together when the next turn is prepared.
 * 返回策略控件；下一个回合准备时一并读取包含选择与授权清单。 */
export function SubagentModelsSettingsArea(props: { providers: ProviderSummary[] }) {
  const { t } = useI18n();
  const [snapshot, setSnapshot] = React.useState<SubagentModelsSnapshot | null>(null);
  const [models, setModels] = React.useState<SubagentModelEntry[]>([]);
  // The server owns the default; this draft becomes editable only after hydration.
  // 默认值由服务端持有；此草稿仅在水合后可编辑。
  const [includeSessionModel, setIncludeSessionModel] = React.useState(false);
  const [loadError, setLoadError] = React.useState<string | null>(null);
  const [saving, setSaving] = React.useState(false);
  const load = React.useCallback(async () => {
    setLoadError(null);
    try {
      const loaded = await loadSubagentModels();
      setSnapshot(loaded);
      setModels(loaded.models);
      setIncludeSessionModel(loaded.includeSessionModel);
    } catch (error) {
      setLoadError(errorMessage(error));
    }
  }, []);
  React.useEffect(() => { void load(); }, [load]);
  const save = async () => {
    setSaving(true);
    try {
      const saved = await saveSubagentModels(trimmedSubagentModels(models), includeSessionModel);
      setSnapshot(saved);
      setModels(saved.models);
      setIncludeSessionModel(saved.includeSessionModel);
      toast.success(t("subagentModels.saved"));
    } catch (error) {
      toast.error(t("subagentModels.saveFailed"), { description: errorMessage(error) });
    } finally {
      setSaving(false);
    }
  };
  const dirty = snapshot !== null && (includeSessionModel !== snapshot.includeSessionModel || !sameSubagentModels(models, snapshot.models));
  const fits = snapshot === null || subagentModelNotesFit(models, snapshot.maxNoteChars);
  return (
    <PageBody>
      <PageSection
        data-subagent-models-system=""
        description={t("subagentModels.system.detail")}
        guide={<PageGuide items={[
          { term: t("subagentModels.guide.sees"), detail: t("subagentModels.guide.seesDetail") },
          { term: t("subagentModels.guide.blocked"), detail: t("subagentModels.guide.blockedDetail") },
          { term: t("subagentModels.guide.session"), detail: t("subagentModels.guide.sessionDetail") },
        ]} />}
        status={snapshot ? <Badge variant="secondary">{formatMessage(t("subagentModels.count"), { count: snapshot.models.length })}</Badge> : undefined}
        title={t("subagentModels.system.title")}
      >
        {loadError ? (
          <PageNotice actions={<Button onClick={() => void load()} size="sm" type="button" variant="outline">{t("composer.capability.retry")}</Button>}
            details={loadError} detailsLabel={t("settings.errors.showErrorDetails")} tone="error">
            {t("subagentModels.loadFailed")}
          </PageNotice>
        ) : !snapshot ? (
          <StreamPlaceholder count={3} label={t("subagentModels.loading")} shape="rows" />
        ) : (
          <SubagentModelEditor disabled={saving} maxModels={snapshot.maxModels} maxNoteChars={snapshot.maxNoteChars}
            includeSessionModel={includeSessionModel} onIncludeSessionModelChange={setIncludeSessionModel}
            models={models} onChange={setModels} providers={props.providers} scope="system" />
        )}
      </PageSection>
      {snapshot ? (
        <PageActionBar
          actions={<>
            <Button disabled={!dirty || saving} onClick={() => { setModels(snapshot.models); setIncludeSessionModel(snapshot.includeSessionModel); }} size="sm" type="button" variant="ghost">
              {t("subagentModels.discard")}
            </Button>
            <Button data-subagent-models-save="" disabled={!dirty || !fits || saving} onClick={() => void save()} size="sm" type="button">
              {t("subagentModels.save")}
            </Button>
          </>}
          pending={dirty}
        >
          {!fits ? <span role="status">{formatMessage(t("subagentModels.noteTooLong"), { max: snapshot.maxNoteChars })}</span>
            : dirty ? <span role="status">{t("subagentModels.pending")}</span> : null}
        </PageActionBar>
      ) : null}
    </PageBody>
  );
}
