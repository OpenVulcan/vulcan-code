import * as React from "react";
import { PageEmptyState, PageHint, PageNotice, PageRow, PageSection } from "@/components/page";
import { Button } from "@/ui-kit/primitives/button";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/ui-kit/primitives/select";
import { StreamPlaceholder } from "@/ui-kit/primitives/stream-placeholder";
import { MODE_KEYS, OPTION_HINT_KEYS, OPTION_KEYS, label } from "@/components/features/settings/permission-judge-labels";
import { FOLLOW_DEFAULT, useProjectPermissionJudgePolicy } from "@/components/project/project-permission-judge-section";
import { formatMessage, useI18n } from "@/i18n";
import type { ProjectSummary } from "@/types";
import "@/ui-kit/styles/features/settings-forms.css"

/** Category authorization page: choose a project, then how the judge treats each category of the
 * model in use there, such as database resets. Choices live in the project and save at once.
 * 类别授权页：选择项目，再选择判定在该项目中如何处理使用中模型的各个类别（例如数据库重置）。选择保存在
 * 项目中并立即生效。 */
export function PermissionJudgePolicyPage(props: { projects: readonly ProjectSummary[] }) {
  const { t } = useI18n();
  const [projectId, setProjectId] = React.useState<string | null>(props.projects[0]?.id ?? null);
  // A removed project falls back to the first one still listed.
  // 已移除的项目回退到仍在列表中的第一个项目。
  const current = props.projects.some((project) => project.id === projectId) ? projectId : props.projects[0]?.id ?? null;
  const { snapshot, loadError, saving, load, save } = useProjectPermissionJudgePolicy(current);

  if (current === null) {
    return <PageSection title={t("projectSettings.permissionJudge.title")}>
      <PageEmptyState title={t("settings.permissionJudge.policy.noProjects")} />
    </PageSection>;
  }

  const unsupported = snapshot ? Object.keys(snapshot.unsupportedOptions) : [];
  return <PageSection layout="rows" data-judge-policy="" title={t("projectSettings.permissionJudge.title")}
    description={t("projectSettings.permissionJudge.description")}
    actions={<Select value={current} onValueChange={setProjectId}>
      <SelectTrigger aria-label={t("settings.permissionJudge.policy.project")} className="vk-form-w-auto" size="sm"><SelectValue /></SelectTrigger>
      <SelectContent>
        {props.projects.map((project) => <SelectItem key={project.id} value={project.id}>{project.name}</SelectItem>)}
      </SelectContent>
    </Select>}>
    {loadError ? <PageNotice tone="error" details={loadError} detailsLabel={t("settings.errors.showErrorDetails")}
      actions={<Button type="button" size="sm" variant="outline" onClick={() => void load()}>{t("composer.capability.retry")}</Button>}>
      {t("projectSettings.permissionJudge.loadFailed")}
    </PageNotice> : !snapshot ? <StreamPlaceholder label={t("projectSettings.permissionJudge.loading")} shape="rows" count={3} /> : <>
      {snapshot.issue ? <PageNotice tone="warning">{t("projectSettings.permissionJudge.issue")} {snapshot.issue.detail}</PageNotice> : null}
      {snapshot.options.length === 0 ? <PageEmptyState title={t("projectSettings.permissionJudge.empty")} /> : null}
      {snapshot.options.map((option) => (
        <PageRow key={option.key} data-judge-option={option.key} label={label(t, OPTION_KEYS, option.key)}
          description={Object.hasOwn(OPTION_HINT_KEYS, option.key) ? t(OPTION_HINT_KEYS[option.key]) : undefined}>
          <Select disabled={saving !== null} value={option.chosenMode ?? FOLLOW_DEFAULT}
            onValueChange={(mode) => void save(option.key, mode)}>
            <SelectTrigger aria-label={label(t, OPTION_KEYS, option.key)} className="vk-form-w-md" size="sm"><SelectValue /></SelectTrigger>
            <SelectContent>
              <SelectItem value={FOLLOW_DEFAULT}>
                {formatMessage(t("projectSettings.permissionJudge.followDefault"), { mode: label(t, MODE_KEYS, option.defaultMode) })}
              </SelectItem>
              {option.modes.map((mode) => <SelectItem key={mode} value={mode}>{label(t, MODE_KEYS, mode)}</SelectItem>)}
            </SelectContent>
          </Select>
        </PageRow>
      ))}
      {unsupported.length > 0 ? <PageHint className="vk-form-inset">
        {formatMessage(t("projectSettings.permissionJudge.unsupported"), { options: unsupported.map((key) => label(t, OPTION_KEYS, key)).join(", ") })}
      </PageHint> : null}
    </>}
  </PageSection>;
}
