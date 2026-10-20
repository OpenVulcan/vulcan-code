import * as React from "react";
import { toast } from "@/ui-kit/primitives/toast";
import { PageEmptyState, PageHint, PageNotice, PageRow, PageSection } from "@/components/page";
import { Button } from "@/ui-kit/primitives/button";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/ui-kit/primitives/select";
import { StreamPlaceholder } from "@/ui-kit/primitives/stream-placeholder";
import { Switch } from "@/ui-kit/primitives/switch";
import { Textarea } from "@/ui-kit/primitives/textarea";
import { useI18n, type TranslationKey } from "@/i18n";
import { errorMessage } from "@/lib/display-format";
import {
  loadProjectReviewRules,
  loadReviewRulesSystemDefault,
  saveProjectReviewRules,
  saveReviewRulesSystemDefault,
} from "@/service/permission-judge-service";
import type { ProjectReviewRulesSnapshot, ProjectSummary, ReviewRules, ReviewRulesSystemDefaultSnapshot } from "@/types";
import "@/ui-kit/styles/features/settings-forms.css"

// GROUPS lists the rule groups in the order the reviewer applies them, with their labels and hints.
// GROUPS 按复核器应用顺序列出规则分组及其标签与说明。
const GROUPS: readonly { key: keyof ReviewRules; label: TranslationKey; hint: TranslationKey; placeholder: TranslationKey }[] = [
  {
    key: "deny",
    label: "settings.permissionJudge.reviewRules.deny",
    hint: "settings.permissionJudge.reviewRules.denyHint",
    placeholder: "settings.permissionJudge.reviewRules.denyPlaceholder",
  },
  {
    key: "ask",
    label: "settings.permissionJudge.reviewRules.ask",
    hint: "settings.permissionJudge.reviewRules.askHint",
    placeholder: "settings.permissionJudge.reviewRules.askPlaceholder",
  },
  {
    key: "allow",
    label: "settings.permissionJudge.reviewRules.allow",
    hint: "settings.permissionJudge.reviewRules.allowHint",
    placeholder: "settings.permissionJudge.reviewRules.allowPlaceholder",
  },
];

// trimmed returns the rules as they are saved.
// trimmed 返回保存时的规则形态。
function trimmed(rules: ReviewRules): ReviewRules {
  return { allow: rules.allow.trim(), ask: rules.ask.trim(), deny: rules.deny.trim() };
}

// sameRules tells whether two rule sets save the same text.
// sameRules 判断两组规则保存后的文本是否相同。
function sameRules(left: ReviewRules, right: ReviewRules): boolean {
  const a = trimmed(left);
  const b = trimmed(right);
  return a.allow === b.allow && a.ask === b.ask && a.deny === b.deny;
}

/** Review rules page: natural-language rules the reviewing LLM applies before its own guidance, as a
 * system default and per-project replacements. The next review reads the saved rules.
 * 复核规则页：复核 LLM 在自身指引之前应用的自然语言规则，包括系统默认值与各项目的替换值。下一次复核即
 * 读取已保存的规则。 */
export function PermissionJudgeReviewRulesPage(props: { projects: readonly ProjectSummary[] }) {
  return <>
    <SystemRulesSection />
    <ProjectRulesSection projects={props.projects} />
  </>;
}

// RulesEditor edits the three rule groups.
// RulesEditor 编辑三个规则分组。
function RulesEditor(props: { rules: ReviewRules; maxChars: number; onChange: (rules: ReviewRules) => void; scope: string }) {
  const { t } = useI18n();
  return <>
    {GROUPS.map((group) => <PageRow key={group.key} label={t(group.label)} description={t(group.hint)} stacked>
      <Textarea aria-label={t(group.label)} data-review-rules-group={`${props.scope}-${group.key}`} density="large"
        maxLength={props.maxChars} placeholder={t(group.placeholder)} value={props.rules[group.key]}
        onChange={(event) => props.onChange({ ...props.rules, [group.key]: event.target.value })} />
    </PageRow>)}
  </>;
}

// SystemRulesSection edits the system default rules.
// SystemRulesSection 编辑系统默认规则。
function SystemRulesSection() {
  const { t } = useI18n();
  const [snapshot, setSnapshot] = React.useState<ReviewRulesSystemDefaultSnapshot | null>(null);
  const [rules, setRules] = React.useState<ReviewRules>({ allow: "", ask: "", deny: "" });
  const [loadError, setLoadError] = React.useState<string | null>(null);
  const [saving, setSaving] = React.useState(false);
  const load = React.useCallback(async () => {
    setLoadError(null);
    try {
      const loaded = await loadReviewRulesSystemDefault();
      setSnapshot(loaded);
      setRules(loaded.rules);
    } catch (error) {
      setLoadError(errorMessage(error));
    }
  }, []);
  React.useEffect(() => { void load(); }, [load]);
  const save = async () => {
    setSaving(true);
    try {
      const saved = await saveReviewRulesSystemDefault(trimmed(rules));
      setSnapshot(saved);
      setRules(saved.rules);
      toast.success(t("settings.permissionJudge.reviewRules.saved"));
    } catch (error) {
      toast.error(t("settings.permissionJudge.reviewRules.saveFailed"), { description: errorMessage(error) });
    } finally {
      setSaving(false);
    }
  };
  return <PageSection layout="rows" data-review-rules-system="" title={t("settings.permissionJudge.reviewRules.systemTitle")}
    description={t("settings.permissionJudge.reviewRules.systemHint")}
    footer={snapshot ? <Button type="button" size="sm" data-review-rules-system-save=""
      disabled={saving || sameRules(rules, snapshot.rules)} onClick={() => void save()}>
      {t("settings.permissionJudge.reviewRules.save")}
    </Button> : undefined}>
    {loadError ? <PageNotice tone="error" details={loadError} detailsLabel={t("settings.errors.showErrorDetails")}
      actions={<Button type="button" size="sm" variant="outline" onClick={() => void load()}>{t("composer.capability.retry")}</Button>}>
      {t("settings.permissionJudge.reviewRules.loadFailed")}
    </PageNotice> : !snapshot ? <StreamPlaceholder label={t("settings.permissionJudge.reviewRules.loading")} shape="rows" count={3} />
      : <RulesEditor scope="system" rules={rules} maxChars={snapshot.maxChars} onChange={setRules} />}
  </PageSection>;
}

// ProjectRulesSection chooses a project, then whether it follows the system rules or keeps its own.
// ProjectRulesSection 选择项目，再决定其跟随系统规则还是保留自己的规则。
function ProjectRulesSection(props: { projects: readonly ProjectSummary[] }) {
  const { t } = useI18n();
  const [projectId, setProjectId] = React.useState<string | null>(props.projects[0]?.id ?? null);
  const current = props.projects.some((project) => project.id === projectId) ? projectId : props.projects[0]?.id ?? null;
  const [snapshot, setSnapshot] = React.useState<ProjectReviewRulesSnapshot | null>(null);
  const [rules, setRules] = React.useState<ReviewRules>({ allow: "", ask: "", deny: "" });
  const [loadError, setLoadError] = React.useState<string | null>(null);
  const [saving, setSaving] = React.useState(false);
  const load = React.useCallback(async () => {
    if (current === null) return;
    setLoadError(null);
    setSnapshot(null);
    try {
      const loaded = await loadProjectReviewRules(current);
      setSnapshot(loaded);
      setRules(loaded.rules ?? loaded.systemRules);
    } catch (error) {
      setLoadError(errorMessage(error));
    }
  }, [current]);
  React.useEffect(() => { void load(); }, [load]);
  const save = async (next: ReviewRules | null) => {
    if (current === null) return;
    setSaving(true);
    try {
      const saved = await saveProjectReviewRules(current, next === null ? null : trimmed(next));
      setSnapshot(saved);
      setRules(saved.rules ?? saved.systemRules);
      toast.success(t("settings.permissionJudge.reviewRules.saved"));
    } catch (error) {
      toast.error(t("settings.permissionJudge.reviewRules.saveFailed"), { description: errorMessage(error) });
    } finally {
      setSaving(false);
    }
  };

  if (current === null) {
    return <PageSection title={t("settings.permissionJudge.reviewRules.projectTitle")}>
      <PageEmptyState title={t("settings.permissionJudge.policy.noProjects")} />
    </PageSection>;
  }
  const own = snapshot?.rules ?? null;
  const systemEmpty = snapshot !== null && sameRules(snapshot.systemRules, { allow: "", ask: "", deny: "" });
  return <PageSection layout="rows" data-review-rules-project="" title={t("settings.permissionJudge.reviewRules.projectTitle")}
    description={t("settings.permissionJudge.reviewRules.projectHint")}
    footer={snapshot && own ? <Button type="button" size="sm" data-review-rules-project-save=""
      disabled={saving || sameRules(rules, own)} onClick={() => void save(rules)}>
      {t("settings.permissionJudge.reviewRules.save")}
    </Button> : undefined}
    actions={<Select value={current} onValueChange={setProjectId}>
      <SelectTrigger aria-label={t("settings.permissionJudge.policy.project")} className="vk-form-w-auto" size="sm"><SelectValue /></SelectTrigger>
      <SelectContent>
        {props.projects.map((project) => <SelectItem key={project.id} value={project.id}>{project.name}</SelectItem>)}
      </SelectContent>
    </Select>}>
    {loadError ? <PageNotice tone="error" details={loadError} detailsLabel={t("settings.errors.showErrorDetails")}
      actions={<Button type="button" size="sm" variant="outline" onClick={() => void load()}>{t("composer.capability.retry")}</Button>}>
      {t("settings.permissionJudge.reviewRules.loadFailed")}
    </PageNotice> : !snapshot ? <StreamPlaceholder label={t("settings.permissionJudge.reviewRules.loading")} shape="rows" count={2} /> : <>
      <PageRow label={t("settings.permissionJudge.reviewRules.custom")} description={t("settings.permissionJudge.reviewRules.customHint")}>
        <Switch aria-label={t("settings.permissionJudge.reviewRules.custom")} checked={own !== null} disabled={saving}
          onCheckedChange={(checked) => void save(checked ? snapshot.systemRules : null)} />
      </PageRow>
      {own !== null ? <RulesEditor scope="project" rules={rules} maxChars={snapshot.maxChars} onChange={setRules} />
        : <PageHint className="vk-form-inset" data-review-rules-follows="">
          {systemEmpty ? t("settings.permissionJudge.reviewRules.followsEmpty") : t("settings.permissionJudge.reviewRules.follows")}
        </PageHint>}
    </>}
  </PageSection>;
}
