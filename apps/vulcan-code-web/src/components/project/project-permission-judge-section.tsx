import React from "react";
import { ShieldQuestion } from "lucide-react";
import { toast } from "@/ui-kit/primitives/toast";
import { Button } from "@/ui-kit/primitives/button";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/ui-kit/primitives/select";
import { StreamPlaceholder } from "@/ui-kit/primitives/stream-placeholder";
import { MODE_KEYS, OPTION_HINT_KEYS, OPTION_KEYS, label } from "@/components/features/settings/permission-judge-labels";
import { formatMessage, useI18n } from "@/i18n";
import { errorMessage } from "@/lib/display-format";
import { loadProjectPermissionJudgePolicy, saveProjectPermissionJudgePolicy } from "@/service/permission-judge-service";
import type { ProjectPermissionJudgePolicySnapshot } from "@/types";
import "@/ui-kit/styles/features/settings-forms.css"

// FOLLOW_DEFAULT is the select value that drops the project's choice; policy modes are plain words.
// FOLLOW_DEFAULT 是移除项目选择的下拉值；策略档位都是普通单词。
export const FOLLOW_DEFAULT = "@default";

// nextPolicyOptions is the complete save map after one change: every current choice of the model
// in use, with `key` set to `mode` or removed to follow the default.
// nextPolicyOptions 是一次修改后的完整保存映射：使用中模型的全部当前选择，`key` 设为 `mode`，或移除以跟随默认值。
export function nextPolicyOptions(snapshot: ProjectPermissionJudgePolicySnapshot, key: string, mode: string): Record<string, string> {
  const options: Record<string, string> = {};
  for (const option of snapshot.options) {
    if (option.chosenMode !== null) options[option.key] = option.chosenMode;
  }
  if (mode === FOLLOW_DEFAULT) delete options[key];
  else options[key] = mode;
  return options;
}

// ProjectPermissionJudgePolicy is the loaded state of one project's category choices.
// ProjectPermissionJudgePolicy 是单个项目类别选择的加载状态。
export interface ProjectPermissionJudgePolicy {
  snapshot: ProjectPermissionJudgePolicySnapshot | null;
  loadError: string | null;
  // saving is the category key being saved.
  // saving 是正在保存的类别键。
  saving: string | null;
  load: () => Promise<void>;
  save: (key: string, mode: string) => Promise<void>;
}

// useProjectPermissionJudgePolicy reads one project's choices for the model in use and saves each
// change at once; responses of an older project or read are dropped.
// useProjectPermissionJudgePolicy 读取单个项目针对使用中模型的选择，并立即保存每次修改；旧项目或旧读取的响应会被丢弃。
export function useProjectPermissionJudgePolicy(projectId: string | null): ProjectPermissionJudgePolicy {
  const { t } = useI18n();
  const [snapshot, setSnapshot] = React.useState<ProjectPermissionJudgePolicySnapshot | null>(null);
  const [loadError, setLoadError] = React.useState<string | null>(null);
  const [saving, setSaving] = React.useState<string | null>(null);
  // sequenceRef drops responses of an older project or an older read.
  // sequenceRef 丢弃旧项目或旧读取的响应。
  const sequenceRef = React.useRef(0);

  const load = React.useCallback(async () => {
    const sequence = ++sequenceRef.current;
    setSnapshot(null);
    setLoadError(null);
    if (projectId === null) return;
    try {
      const next = await loadProjectPermissionJudgePolicy(projectId);
      if (sequenceRef.current === sequence) setSnapshot(next);
    } catch (cause) {
      if (sequenceRef.current === sequence) setLoadError(errorMessage(cause));
    }
  }, [projectId]);

  React.useEffect(() => {
    void load();
    return () => { sequenceRef.current += 1; };
  }, [load]);

  // save writes one change and shows the snapshot the service returns.
  // save 写入一次修改并显示服务返回的快照。
  const save = async (key: string, mode: string) => {
    if (!snapshot || saving !== null) return;
    const sequence = sequenceRef.current;
    setSaving(key);
    try {
      const next = await saveProjectPermissionJudgePolicy(snapshot.projectId, nextPolicyOptions(snapshot, key, mode));
      if (sequenceRef.current === sequence) {
        setSnapshot(next);
        toast.success(t("projectSettings.permissionJudge.saved"));
      }
    } catch (cause) {
      if (sequenceRef.current === sequence) toast.error(errorMessage(cause));
    } finally {
      if (sequenceRef.current === sequence) setSaving(null);
    }
  };

  return { snapshot, loadError, saving, load, save };
}

// ProjectPermissionJudgeSection edits how the permission judge treats each category of the model
// in use for one project. Each change saves at once; choices for categories the model no longer
// has are kept by the service.
// ProjectPermissionJudgeSection 编辑权限判定在单个项目中如何处理使用中模型的各个类别。每次修改立即
// 保存；模型已不再有的类别的选择由服务保留。
export function ProjectPermissionJudgeSection(props: { projectId: string }) {
  const { t } = useI18n();
  const { snapshot, loadError, saving, load, save } = useProjectPermissionJudgePolicy(props.projectId);
  const labelId = React.useId();

  const unsupported = snapshot ? Object.keys(snapshot.unsupportedOptions) : [];
  return (
    <div className="project-settings-section" data-project-section="permission-judge">
      <div className="project-settings-summary">
        <ShieldQuestion size={22} />
        <div>
          <strong>{t("projectSettings.permissionJudge.title")}</strong>
          <span>{t("projectSettings.permissionJudge.description")}</span>
        </div>
      </div>
      {loadError ? (
        <div role="alert" className="vk-form-stack">
          <p className="vk-field-error vk-form-break">{t("projectSettings.permissionJudge.loadFailed")} {loadError}</p>
          <Button className="vk-form-fit" type="button" variant="outline" onClick={() => void load()}>{t("composer.capability.retry")}</Button>
        </div>
      ) : !snapshot ? (
        <StreamPlaceholder label={t("projectSettings.permissionJudge.loading")} shape="rows" count={3} />
      ) : (
        <div className="vk-form-loose">
          {snapshot.issue ? (
            <p role="alert" className="vk-field-error vk-form-break">
              {t("projectSettings.permissionJudge.issue")} {snapshot.issue.detail}
            </p>
          ) : null}
          {snapshot.options.length === 0 ? (
            <p className="vk-form-text">{t("projectSettings.permissionJudge.empty")}</p>
          ) : snapshot.options.map((option) => (
            <div className="vk-field" data-judge-option={option.key} key={option.key}>
              <span className="vk-field-label" id={`${labelId}-${option.key}`}>{label(t, OPTION_KEYS, option.key)}</span>
              {Object.hasOwn(OPTION_HINT_KEYS, option.key) ? <span className="vk-field-hint">{t(OPTION_HINT_KEYS[option.key])}</span> : null}
              <Select
                disabled={saving !== null}
                onValueChange={(mode) => void save(option.key, mode)}
                value={option.chosenMode ?? FOLLOW_DEFAULT}
              >
                <SelectTrigger aria-labelledby={`${labelId}-${option.key}`} className="vk-form-w-xl">
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value={FOLLOW_DEFAULT}>
                    {formatMessage(t("projectSettings.permissionJudge.followDefault"), { mode: label(t, MODE_KEYS, option.defaultMode) })}
                  </SelectItem>
                  {option.modes.map((mode) => <SelectItem key={mode} value={mode}>{label(t, MODE_KEYS, mode)}</SelectItem>)}
                </SelectContent>
              </Select>
            </div>
          ))}
          {unsupported.length > 0 ? (
            <p className="vk-form-text">
              {formatMessage(t("projectSettings.permissionJudge.unsupported"), { options: unsupported.map((key) => label(t, OPTION_KEYS, key)).join(", ") })}
            </p>
          ) : null}
        </div>
      )}
    </div>
  );
}
