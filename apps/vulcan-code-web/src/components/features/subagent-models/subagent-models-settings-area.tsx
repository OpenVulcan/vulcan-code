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

/** Subagent models page: the system default allowlist every session follows unless it keeps its own list.
 * The session model is always allowed on top; the next spawn reads the saved list.
 * 子代理模型页：所有会话默认跟随的系统白名单，除非会话保留了自己的清单。会话模型始终额外允许；下一次派生读取
 * 已保存的清单。 */
export function SubagentModelsSettingsArea(props: { providers: ProviderSummary[] }) {
  const { t } = useI18n();
  const [snapshot, setSnapshot] = React.useState<SubagentModelsSnapshot | null>(null);
  const [models, setModels] = React.useState<SubagentModelEntry[]>([]);
  const [loadError, setLoadError] = React.useState<string | null>(null);
  const [saving, setSaving] = React.useState(false);
  const load = React.useCallback(async () => {
    setLoadError(null);
    try {
      const loaded = await loadSubagentModels();
      setSnapshot(loaded);
      setModels(loaded.models);
    } catch (error) {
      setLoadError(errorMessage(error));
    }
  }, []);
  React.useEffect(() => { void load(); }, [load]);
  const save = async () => {
    setSaving(true);
    try {
      const saved = await saveSubagentModels(trimmedSubagentModels(models));
      setSnapshot(saved);
      setModels(saved.models);
      toast.success(t("subagentModels.saved"));
    } catch (error) {
      toast.error(t("subagentModels.saveFailed"), { description: errorMessage(error) });
    } finally {
      setSaving(false);
    }
  };
  const dirty = snapshot !== null && !sameSubagentModels(models, snapshot.models);
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
            models={models} onChange={setModels} providers={props.providers} scope="system" />
        )}
      </PageSection>
      {snapshot ? (
        <PageActionBar
          actions={<>
            <Button disabled={!dirty || saving} onClick={() => setModels(snapshot.models)} size="sm" type="button" variant="ghost">
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
