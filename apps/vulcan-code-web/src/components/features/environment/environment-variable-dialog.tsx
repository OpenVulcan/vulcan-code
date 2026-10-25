import * as React from "react";
import { errorMessage } from "@/lib/display-format";
import { formatMessage, useI18n } from "@/i18n";
import { SecretValueInput } from "@/components/environment/secret-value-input";
import { checkEnvironmentVariableName, saveEnvironmentVariable } from "@/service/environment-variables-service";
import type {
  EnvironmentVariableEntry,
  EnvironmentVariableKindKey,
  EnvironmentVariableNameCheck,
  EnvironmentVariableRules,
  EnvironmentVariablesSnapshot,
  ProjectSummary,
} from "@/types";
import { Button } from "@/ui-kit/primitives/button";
import { Checkbox } from "@/ui-kit/primitives/checkbox";
import { SegmentedControl, SegmentedControlItem } from "@/ui-kit/primitives/choice-controls";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/ui-kit/primitives/dialog";
import { Input } from "@/ui-kit/primitives/input";
import { Textarea } from "@/ui-kit/primitives/textarea";
import { toast } from "@/ui-kit/primitives/toast";

// NAME_CHECK_DELAY_MS waits for typing to pause before asking the service about a name.
// NAME_CHECK_DELAY_MS 等待输入停顿后再向服务查询名称。
const NAME_CHECK_DELAY_MS = 250;

// EnvironmentVariableDialogProps edit one variable, or create one when `variable` is absent.
// EnvironmentVariableDialogProps 编辑一条变量；`variable` 缺失时新建一条。
export interface EnvironmentVariableDialogProps {
  variable?: EnvironmentVariableEntry;
  rules: EnvironmentVariableRules;
  projects: ProjectSummary[];
  onSaved: (snapshot: EnvironmentVariablesSnapshot) => void;
  onClose: () => void;
}

/** Variable editor: a checked name, a masked value, a kind, a description and where it applies.
 * A secret's stored value is never sent back, so editing leaves the value empty and an empty value keeps it.
 * 变量编辑器：经过检查的名称、遮蔽的值、类型、说明及适用范围。密钥的存储值从不回传，因此编辑时值为空，留空即保留。 */
export function EnvironmentVariableDialog(props: EnvironmentVariableDialogProps) {
  const { t } = useI18n();
  const existing = props.variable;
  const [name, setName] = React.useState(existing?.name ?? "");
  const [value, setValue] = React.useState(existing?.kind === "plain" ? existing.value : "");
  const [kind, setKind] = React.useState<EnvironmentVariableKindKey>(existing?.kind ?? "secret");
  const [description, setDescription] = React.useState(existing?.description ?? "");
  const [global, setGlobal] = React.useState(existing?.global ?? true);
  const [projectIds, setProjectIds] = React.useState<string[]>(existing?.projectIds ?? []);
  const [check, setCheck] = React.useState<EnvironmentVariableNameCheck | null>(null);
  const [saving, setSaving] = React.useState(false);
  const trimmedName = name.trim();

  // The latest typed name wins; an older answer that arrives late is ignored.
  // 以最新输入的名称为准；迟到的旧回答被忽略。
  React.useEffect(() => {
    if (!trimmedName) {
      setCheck(null);
      return;
    }
    let current = true;
    const timer = window.setTimeout(() => {
      checkEnvironmentVariableName(trimmedName)
        .then((result) => { if (current) setCheck(result); })
        .catch(() => { if (current) setCheck(null); });
    }, NAME_CHECK_DELAY_MS);
    return () => { current = false; window.clearTimeout(timer); };
  }, [trimmedName]);

  const checked = check?.name === trimmedName ? check : null;
  const valueBytes = new TextEncoder().encode(value).length;
  const valueFits = valueBytes <= props.rules.valueMaxBytes;
  const descriptionFits = [...description].length <= props.rules.descriptionMaxChars;
  // Editing keeps the stored value when the field is left empty; creating needs a value.
  // 编辑时值留空即保留存储值；新建时需要值。
  const keepsValue = existing !== undefined && value.length === 0;
  const valid = trimmedName.length > 0 && !checked?.invalidReason && (keepsValue || value.length > 0)
    && valueFits && descriptionFits && (global || projectIds.length > 0);

  const toggleProject = (projectId: string, selected: boolean) => {
    setProjectIds((current) => selected ? [...current, projectId] : current.filter((id) => id !== projectId));
  };
  const save = async () => {
    setSaving(true);
    try {
      props.onSaved(await saveEnvironmentVariable({
        id: existing?.id,
        name: trimmedName,
        value: keepsValue ? undefined : value,
        kind,
        description: description.trim(),
        global,
        projectIds: global ? [] : projectIds,
      }));
      toast.success(t("environment.saved"));
    } catch (error) {
      toast.error(t("environment.saveFailed"), { description: errorMessage(error) });
    } finally {
      setSaving(false);
    }
  };

  return (
    <Dialog open onOpenChange={(open) => { if (!open && !saving) props.onClose(); }}>
      <DialogContent data-environment-dialog="" scrollMode="fixed-footer" size="lg">
        <DialogHeader>
          <DialogTitle>{existing ? t("environment.dialog.editTitle") : t("environment.dialog.createTitle")}</DialogTitle>
          <DialogDescription>{t("environment.dialog.detail")}</DialogDescription>
        </DialogHeader>
        <div className="vk-env-form">
          <section className="vk-env-block">
            <label className="vk-env-block-title" htmlFor="environment-variable-name">{t("environment.field.name")}</label>
            <Input
              aria-invalid={Boolean(checked?.invalidReason)}
              autoComplete="off"
              className="vk-env-mono"
              disabled={saving}
              id="environment-variable-name"
              maxLength={props.rules.nameMaxChars}
              onChange={(event) => setName(event.target.value)}
              placeholder={`${props.rules.recommendedPrefix}NAME`}
              spellCheck={false}
              value={name}
            />
            {checked?.invalidReason ? <p className="vk-field-error" role="alert">{checked.invalidReason}</p> : null}
            {checked && !checked.invalidReason && checked.definedBySystem ? (
              <p className="vk-env-note" data-tone="warn" data-defined-by-system="">{t("environment.definedBySystemNote")}</p>
            ) : null}
            {checked && !checked.invalidReason && !checked.hasRecommendedPrefix ? (
              <p className="vk-env-note" data-recommend-prefix="">
                {formatMessage(t("environment.field.prefixHint"), { prefix: props.rules.recommendedPrefix })}
              </p>
            ) : null}
          </section>
          <section className="vk-env-block">
            <span className="vk-env-block-title">{t("environment.field.kind")}</span>
            <SegmentedControl aria-label={t("environment.field.kind")} disabled={saving}
              onValueChange={(next) => setKind(next as EnvironmentVariableKindKey)} value={kind}>
              <SegmentedControlItem value="secret">{t("environment.kind.secret")}</SegmentedControlItem>
              <SegmentedControlItem value="plain">{t("environment.kind.plain")}</SegmentedControlItem>
            </SegmentedControl>
            <p className="vk-env-note">{kind === "secret" ? t("environment.kind.secretDetail") : t("environment.kind.plainDetail")}</p>
          </section>
          <section className="vk-env-block">
            <label className="vk-env-block-title" htmlFor="environment-variable-value">{t("environment.field.value")}</label>
            <SecretValueInput
              disabled={saving}
              id="environment-variable-value"
              onValueChange={setValue}
              placeholder={existing ? t("environment.value.keepPlaceholder") : t("environment.value.placeholder")}
              value={value}
            />
            {!valueFits ? (
              <p className="vk-field-error" role="alert">{formatMessage(t("environment.field.valueTooLong"), { max: props.rules.valueMaxBytes })}</p>
            ) : null}
          </section>
          <section className="vk-env-block">
            <label className="vk-env-block-title" htmlFor="environment-variable-description">{t("environment.field.description")}</label>
            <Textarea disabled={saving} id="environment-variable-description" onChange={(event) => setDescription(event.target.value)}
              placeholder={t("environment.field.descriptionPlaceholder")} rows={2} value={description} />
            {!descriptionFits ? (
              <p className="vk-field-error" role="alert">{formatMessage(t("environment.field.descriptionTooLong"), { max: props.rules.descriptionMaxChars })}</p>
            ) : null}
          </section>
          <section className="vk-env-block" data-environment-scope="">
            <span className="vk-env-block-title">{t("environment.field.appliesTo")}</span>
            <SegmentedControl aria-label={t("environment.field.appliesTo")} disabled={saving}
              onValueChange={(next) => setGlobal(next === "global")} value={global ? "global" : "projects"}>
              <SegmentedControlItem value="global">{t("environment.scope.global")}</SegmentedControlItem>
              <SegmentedControlItem value="projects">{t("environment.scope.projects")}</SegmentedControlItem>
            </SegmentedControl>
            {!global ? (
              props.projects.length === 0 ? <p className="vk-env-note">{t("environment.field.noProjects")}</p> : (
                <div className="vk-env-checklist">
                  {props.projects.map((project) => (
                    <label className="vk-env-check" key={project.id}>
                      <Checkbox checked={projectIds.includes(project.id)} disabled={saving}
                        onCheckedChange={(selected) => toggleProject(project.id, selected === true)} />
                      <span>{project.name}<small>{project.path}</small></span>
                    </label>
                  ))}
                </div>
              )
            ) : null}
            {!global && projectIds.length === 0 ? <p className="vk-field-error" role="alert">{t("environment.field.projectsRequired")}</p> : null}
          </section>
        </div>
        <DialogFooter>
          <Button disabled={saving} onClick={props.onClose} size="sm" type="button" variant="outline">{t("environment.cancel")}</Button>
          <Button data-environment-save="" disabled={!valid || saving} onClick={() => void save()} size="sm" type="button">
            {t("environment.save")}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
