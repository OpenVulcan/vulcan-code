import * as React from "react";
import { Badge } from "@/ui-kit/primitives/badge";
import { Button } from "@/ui-kit/primitives/button";
import { Checkbox } from "@/ui-kit/primitives/checkbox";
import { Input } from "@/ui-kit/primitives/input";
import { Switch } from "@/ui-kit/primitives/switch";
import { HelpTooltip, SelectControl } from "@/components/plugins/plugin-host-components";
import { PageRow } from "@/components/page";
import { formatMessage, useI18n } from "@/i18n";
import {
  checkedValues, checklistUniverse, formatOptionValue, numberInputValue, storedValues, valueIssue,
  type JsonSchemaNode, type PluginCredentialStatus, type PluginSettingsCredential, type PluginSettingsField,
} from "./plugin-settings-layout";
import type { PluginMessage } from "./plugin-settings-host";

export type { PluginMessage };

// SettingsFieldRowProps binds one declarative field to its draft value.
// SettingsFieldRowProps 将一个声明式字段绑定到其草稿值。
interface SettingsFieldRowProps {
  field: PluginSettingsField;
  schema: JsonSchemaNode | undefined;
  value: unknown;
  disabled: boolean;
  message: PluginMessage;
  onChange: (value: unknown) => void;
}

// SettingsFieldRow renders one field as a PageRow with the control chosen by the layout.
// SettingsFieldRow 将一个字段渲染为 PageRow，控件由版面选定。
export function SettingsFieldRow(props: SettingsFieldRowProps) {
  const { t, locale } = useI18n();
  const { field } = props;
  const id = React.useId();
  const label = props.message(field.label_message_key);
  const issue = valueIssue(props.schema, props.value);
  // text keeps the typed characters of numeric inputs so partial entries such as "-" are not lost.
  // text 保留数值输入的已键入字符，使「-」等中间输入不会丢失。
  const [text, setText] = React.useState(() => (props.value === undefined || props.value === null ? "" : String(props.value)));
  React.useEffect(() => {
    if (numberInputValue(props.schema, text) !== props.value) setText(props.value === undefined || props.value === null ? "" : String(props.value));
    // Only external value changes (load, save, discard) resync the typed text.
    // 只有外部值变化（读取、保存、放弃）才重新同步已键入文字。
  }, [props.value]);
  const issueText = issue === null ? null
    : issue.kind === "range" ? formatMessage(t("plugins.settings.valueRange"), { min: issue.min, max: issue.max })
      : t("plugins.settings.valueInvalid");
  const description = <>
    {field.description_message_key ? <span>{props.message(field.description_message_key)}</span> : null}
    {issueText ? <span className="plugin-settings-issue" role="alert">{issueText}</span> : null}
  </>;
  const placeholder = field.placeholder_message_key ? props.message(field.placeholder_message_key) : undefined;
  let control: React.ReactNode;
  switch (field.control) {
    case "switch":
      control = <Switch aria-label={label} checked={props.value === true} disabled={props.disabled} id={id}
        onCheckedChange={(checked) => props.onChange(checked === true)} />;
      break;
    case "select":
      control = <SelectControl ariaLabel={label} className="plugin-settings-select" disabled={props.disabled}
        onValueChange={(selected) => props.onChange(field.options?.find((option) => String(option.value) === selected)?.value)}
        options={(field.options ?? []).map((option) => ({
          value: String(option.value),
          label: option.label_message_key ? props.message(option.label_message_key) : formatOptionValue(option.value, field.value_format, locale),
        }))}
        value={props.value === undefined ? "" : String(props.value)} />;
      break;
    case "text":
    case "number":
      control = <Input aria-invalid={issue !== null} className="plugin-settings-input" disabled={props.disabled} id={id}
        inputMode={field.control === "number" ? "numeric" : undefined} onChange={(event) => {
          setText(event.target.value);
          props.onChange(field.control === "number" ? numberInputValue(props.schema, event.target.value) : event.target.value);
        }} placeholder={placeholder} spellCheck={false} type="text" value={field.control === "number" ? text : String(props.value ?? "")} />;
      break;
    case "checklist":
      return <PageRow description={description} label={label} stacked>
        <SettingsChecklist disabled={props.disabled} field={field} label={label} message={props.message} onChange={props.onChange} value={props.value} />
      </PageRow>;
  }
  return <PageRow description={description} htmlFor={field.control === "select" ? undefined : id} label={label}>{control}</PageRow>;
}

// SettingsChecklist renders a filter with a selection count, then each group as a header with bulk actions over a bordered option grid.
// SettingsChecklist 渲染带已选计数的筛选框，再将每组渲染为带批量操作的标题与带边框的选项网格。
function SettingsChecklist(props: { field: PluginSettingsField; value: unknown; label: string; disabled: boolean; message: PluginMessage; onChange: (value: unknown) => void }) {
  const { t } = useI18n();
  const [filter, setFilter] = React.useState("");
  const checked = checkedValues(props.field, props.value);
  const universe = checklistUniverse(props.field);
  const query = filter.trim().toLowerCase();
  // update stores the next checked set through the field's selection mode.
  // update 按字段的选择模式保存新的勾选集合。
  const update = (next: Set<string>) => props.onChange(storedValues(props.field, props.value, next));
  const groups = (props.field.option_groups ?? []).map((group) => ({ group, visible: group.values.filter((value) => value.toLowerCase().includes(query)) }));
  return <div className="plugin-settings-checklist">
    <div className="plugin-settings-checklist-toolbar">
      {universe.length > 12 && <Input aria-label={t("plugins.settings.filter")} className="plugin-settings-input" density="compact" type="search"
        onChange={(event) => setFilter(event.target.value)} placeholder={t("plugins.settings.filter")} value={filter} />}
      <Badge variant="outline">{formatMessage(t("plugins.settings.selectedCount"), { selected: universe.filter((value) => checked.has(value)).length, total: universe.length })}</Badge>
    </div>
    {groups.every(({ visible }) => visible.length === 0) ? <p className="plugin-settings-checklist-none" role="status">{t("plugins.settings.noMatches")}</p> : null}
    {groups.map(({ group, visible }) => {
      if (visible.length === 0) return null;
      return <fieldset className="plugin-settings-checklist-group" disabled={props.disabled} key={group.id}>
        <legend className="vk-sr-only">{props.message(group.title_message_key)}</legend>
        <div aria-hidden className="plugin-settings-checklist-header">
          <strong>{props.message(group.title_message_key)}</strong>
        </div>
        <div className="plugin-settings-checklist-actions">
          <Button onClick={() => update(new Set([...checked, ...group.values]))} size="xs" type="button" variant="ghost">{t("plugins.settings.selectAll")}</Button>
          <Button onClick={() => update(new Set([...checked].filter((value) => !group.values.includes(value))))} size="xs" type="button" variant="ghost">
            {t("plugins.settings.clear")}</Button>
        </div>
        <div className="plugin-settings-checklist-options">
          {visible.map((value) => <label className="plugin-settings-checklist-option" key={value} title={value}>
            <Checkbox checked={checked.has(value)} onCheckedChange={(state) => {
              const next = new Set(checked);
              if (state === true) next.add(value); else next.delete(value);
              update(next);
            }} />
            <span>{value}</span>
          </label>)}
        </div>
      </fieldset>;
    })}
  </div>;
}

// SettingsInlineOption renders one switch field of an inline section as a compact checkbox with its description in help.
// SettingsInlineOption 将行内区块的一个开关字段渲染为紧凑复选框，并把说明放入帮助提示。
export function SettingsInlineOption(props: { field: PluginSettingsField; value: unknown; disabled: boolean; message: PluginMessage; onChange: (value: unknown) => void }) {
  const label = props.message(props.field.label_message_key);
  return <div className="plugin-settings-inline-option">
    <label>
      <Checkbox checked={props.value === true} disabled={props.disabled} onCheckedChange={(state) => props.onChange(state === true)} />
      <span>{label}</span>
    </label>
    {props.field.description_message_key ? <HelpTooltip ariaLabel={label} content={props.message(props.field.description_message_key)} /> : null}
  </div>;
}

// SettingsCredentialRowProps binds one credential item to its status and mutations.
// SettingsCredentialRowProps 将一个凭据条目绑定到其状态与变更操作。
interface SettingsCredentialRowProps {
  item: PluginSettingsCredential;
  status: PluginCredentialStatus | null;
  busy: boolean;
  message: PluginMessage;
  onSave: (secret: string) => Promise<boolean>;
  onDelete: () => Promise<boolean>;
}

// SettingsCredentialRow renders a write-only password input; the stored secret is never read back.
// SettingsCredentialRow 渲染只写密码输入框；已保存的密钥永不回读。
export function SettingsCredentialRow(props: SettingsCredentialRowProps) {
  const { t } = useI18n();
  const id = React.useId();
  const [secret, setSecret] = React.useState("");
  const [deleteArmed, setDeleteArmed] = React.useState(false);
  const status = props.status;
  const validation = status?.validationStatus === "valid" ? <Badge variant="success">{t("plugins.settings.credentialValid")}</Badge>
    : status?.validationStatus === "invalid" ? <Badge variant="destructive">{t("plugins.settings.credentialInvalid")}</Badge>
      : status?.configured && status.validationStatus === "untested" ? <Badge variant="outline">{t("plugins.settings.credentialUntested")}</Badge> : null;
  const label = <>
    {props.message(props.item.label_message_key)}{" "}
    {status && <Badge variant={status.configured ? "secondary" : "outline"}>{t(status.configured ? "plugins.settings.credentialConfigured" : "plugins.settings.credentialMissing")}</Badge>}{" "}
    {validation}
  </>;
  return <PageRow description={props.item.description_message_key ? props.message(props.item.description_message_key) : undefined} htmlFor={id} label={label}>
    <Input autoComplete="off" className="plugin-settings-input" disabled={props.busy || status === null} id={id}
      onChange={(event) => setSecret(event.target.value)} placeholder={props.item.placeholder_message_key ? props.message(props.item.placeholder_message_key) : undefined}
      spellCheck={false} type="password" value={secret} />
    <Button disabled={props.busy || status === null || secret.length === 0} onClick={() => void props.onSave(secret).then((saved) => { if (saved) setSecret(""); })} size="sm" type="button">
      {t(status?.configured ? "plugins.settings.credentialReplace" : "plugins.settings.credentialSave")}</Button>
    {props.item.delete_action && status?.configured ? <Button disabled={props.busy} onClick={() => {
      if (!deleteArmed) { setDeleteArmed(true); return; }
      setDeleteArmed(false);
      void props.onDelete();
    }} onBlur={() => setDeleteArmed(false)} size="sm" type="button" variant={deleteArmed ? "destructive" : "outline"}>
      {t(deleteArmed ? "plugins.settings.credentialConfirmDelete" : "plugins.settings.credentialDelete")}</Button> : null}
  </PageRow>;
}
