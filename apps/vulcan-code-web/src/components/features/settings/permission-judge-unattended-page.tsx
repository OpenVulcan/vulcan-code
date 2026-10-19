import * as React from "react";
import { toast } from "@/ui-kit/primitives/toast";
import { PageEmptyState, PageHint, PageNotice, PageRow, PageSection } from "@/components/page";
import { Button } from "@/ui-kit/primitives/button";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/ui-kit/primitives/select";
import { StreamPlaceholder } from "@/ui-kit/primitives/stream-placeholder";
import { Switch } from "@/ui-kit/primitives/switch";
import { Textarea } from "@/ui-kit/primitives/textarea";
import { useI18n } from "@/i18n";
import { errorMessage } from "@/lib/display-format";
import {
  loadProjectUnattendedDefault,
  loadUnattendedSystemDefault,
  saveProjectUnattendedDefault,
  saveUnattendedSystemDefault,
} from "@/service/permission-judge-service";
import type { ProjectSummary, ProjectUnattendedDefaultSnapshot, UnattendedSystemDefaultSnapshot } from "@/types";
import "@/ui-kit/styles/features/settings-forms.css"

/** Unattended defaults page: the system deny policy and each project's replacement. They only
 * prefill the dialog that turns unattended mode on; sessions keep the text the user confirmed.
 * 无人值守默认值页：系统拒绝策略与各项目的替换值。它们只预填开启无人值守模式的对话框；会话保留
 * 用户确认的文本。 */
export function PermissionJudgeUnattendedPage(props: { projects: readonly ProjectSummary[] }) {
  return <>
    <SystemDefaultSection />
    <ProjectDefaultSection projects={props.projects} />
  </>;
}

// SystemDefaultSection edits the system default deny policy.
// SystemDefaultSection 编辑系统默认拒绝策略。
function SystemDefaultSection() {
  const { t } = useI18n();
  const [snapshot, setSnapshot] = React.useState<UnattendedSystemDefaultSnapshot | null>(null);
  const [text, setText] = React.useState("");
  const [loadError, setLoadError] = React.useState<string | null>(null);
  const [saving, setSaving] = React.useState(false);
  const load = React.useCallback(async () => {
    setLoadError(null);
    try {
      const loaded = await loadUnattendedSystemDefault();
      setSnapshot(loaded);
      setText(loaded.denyPolicy);
    } catch (error) {
      setLoadError(errorMessage(error));
    }
  }, []);
  React.useEffect(() => { void load(); }, [load]);
  const save = async () => {
    setSaving(true);
    try {
      const saved = await saveUnattendedSystemDefault(text.trim());
      setSnapshot(saved);
      setText(saved.denyPolicy);
      toast.success(t("settings.permissionJudge.unattended.saved"));
    } catch (error) {
      toast.error(t("settings.permissionJudge.unattended.saveFailed"), { description: errorMessage(error) });
    } finally {
      setSaving(false);
    }
  };
  return <PageSection layout="rows" data-unattended-system="" title={t("settings.permissionJudge.unattended.systemTitle")}
    description={t("settings.permissionJudge.unattended.systemHint")}
    footer={snapshot ? <Button type="button" size="sm" data-unattended-system-save=""
      disabled={saving || text.trim() === snapshot.denyPolicy} onClick={() => void save()}>
      {t("settings.permissionJudge.unattended.save")}
    </Button> : undefined}>
    {loadError ? <PageNotice tone="error" details={loadError} detailsLabel={t("settings.errors.showErrorDetails")}
      actions={<Button type="button" size="sm" variant="outline" onClick={() => void load()}>{t("composer.capability.retry")}</Button>}>
      {t("settings.permissionJudge.unattended.loadFailed")}
    </PageNotice> : !snapshot ? <StreamPlaceholder label={t("settings.permissionJudge.unattended.loading")} shape="rows" count={1} /> : <>
      <PageRow label={t("settings.permissionJudge.unattended.policy")} stacked>
        <Textarea aria-label={t("settings.permissionJudge.unattended.systemTitle")} density="large" maxLength={snapshot.maxChars}
          placeholder={t("permissionMode.unattended.placeholder")} value={text} onChange={(event) => setText(event.target.value)} />
      </PageRow>
    </>}
  </PageSection>;
}

// ProjectDefaultSection chooses a project, then whether it follows the system default or keeps
// its own text.
// ProjectDefaultSection 选择项目，再决定其跟随系统默认值还是保留自己的文本。
function ProjectDefaultSection(props: { projects: readonly ProjectSummary[] }) {
  const { t } = useI18n();
  const [projectId, setProjectId] = React.useState<string | null>(props.projects[0]?.id ?? null);
  const current = props.projects.some((project) => project.id === projectId) ? projectId : props.projects[0]?.id ?? null;
  const [snapshot, setSnapshot] = React.useState<ProjectUnattendedDefaultSnapshot | null>(null);
  const [text, setText] = React.useState("");
  const [loadError, setLoadError] = React.useState<string | null>(null);
  const [saving, setSaving] = React.useState(false);
  const load = React.useCallback(async () => {
    if (current === null) return;
    setLoadError(null);
    setSnapshot(null);
    try {
      const loaded = await loadProjectUnattendedDefault(current);
      setSnapshot(loaded);
      setText(loaded.denyPolicy ?? loaded.systemDenyPolicy);
    } catch (error) {
      setLoadError(errorMessage(error));
    }
  }, [current]);
  React.useEffect(() => { void load(); }, [load]);
  const save = async (denyPolicy: string | null) => {
    if (current === null) return;
    setSaving(true);
    try {
      const saved = await saveProjectUnattendedDefault(current, denyPolicy);
      setSnapshot(saved);
      setText(saved.denyPolicy ?? saved.systemDenyPolicy);
      toast.success(t("settings.permissionJudge.unattended.saved"));
    } catch (error) {
      toast.error(t("settings.permissionJudge.unattended.saveFailed"), { description: errorMessage(error) });
    } finally {
      setSaving(false);
    }
  };

  if (current === null) {
    return <PageSection title={t("settings.permissionJudge.unattended.projectTitle")}>
      <PageEmptyState title={t("settings.permissionJudge.policy.noProjects")} />
    </PageSection>;
  }
  const custom = snapshot?.denyPolicy != null;
  return <PageSection layout="rows" data-unattended-project="" title={t("settings.permissionJudge.unattended.projectTitle")}
    description={t("settings.permissionJudge.unattended.projectHint")}
    footer={snapshot && custom ? <Button type="button" size="sm" data-unattended-project-save=""
      disabled={saving || text.trim() === snapshot.denyPolicy} onClick={() => void save(text.trim())}>
      {t("settings.permissionJudge.unattended.save")}
    </Button> : undefined}
    actions={<Select value={current} onValueChange={setProjectId}>
      <SelectTrigger aria-label={t("settings.permissionJudge.policy.project")} className="vk-form-w-auto" size="sm"><SelectValue /></SelectTrigger>
      <SelectContent>
        {props.projects.map((project) => <SelectItem key={project.id} value={project.id}>{project.name}</SelectItem>)}
      </SelectContent>
    </Select>}>
    {loadError ? <PageNotice tone="error" details={loadError} detailsLabel={t("settings.errors.showErrorDetails")}
      actions={<Button type="button" size="sm" variant="outline" onClick={() => void load()}>{t("composer.capability.retry")}</Button>}>
      {t("settings.permissionJudge.unattended.loadFailed")}
    </PageNotice> : !snapshot ? <StreamPlaceholder label={t("settings.permissionJudge.unattended.loading")} shape="rows" count={2} /> : <>
      <PageRow label={t("settings.permissionJudge.unattended.custom")} description={t("settings.permissionJudge.unattended.customHint")}>
        <Switch aria-label={t("settings.permissionJudge.unattended.custom")} checked={custom} disabled={saving}
          onCheckedChange={(checked) => void save(checked ? snapshot.systemDenyPolicy : null)} />
      </PageRow>
      {custom ? <>
        <PageRow label={t("settings.permissionJudge.unattended.policy")} stacked>
          <Textarea aria-label={t("settings.permissionJudge.unattended.projectTitle")} density="large" maxLength={snapshot.maxChars}
            placeholder={t("permissionMode.unattended.placeholder")} value={text} onChange={(event) => setText(event.target.value)} />
        </PageRow>
      </> : <PageHint className="vk-form-inset" data-unattended-follows="">
        {snapshot.systemDenyPolicy === "" ? t("settings.permissionJudge.unattended.followsEmpty") : t("settings.permissionJudge.unattended.follows")}
        {snapshot.systemDenyPolicy === "" ? null : <span className="vk-form-quote">{snapshot.systemDenyPolicy}</span>}
      </PageHint>}
    </>}
  </PageSection>;
}
