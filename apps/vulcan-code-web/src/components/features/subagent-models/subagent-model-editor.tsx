import * as React from "react";
import { ArrowDown, ArrowUp, Bot, Lock, Plus, Trash2, TriangleAlert } from "lucide-react";
import "@/ui-kit/styles/features/subagent-models.css";
import {
  groupModelPickerChoices,
  modelPickerChoicesFromProviders,
  ModelPickerControl,
  type ModelPickerModelGroup,
} from "@/components/composer/model-picker-control";
import { ProviderIcon } from "@/components/features/settings/provider-icon";
import { PageEmptyState } from "@/components/page";
import { localizedProviderName } from "@/components/features/settings/provider-presentation";
import { formatMessage, useI18n } from "@/i18n";
import type { ProviderSummary, SubagentModelEntry } from "@/types";
import { IconButton } from "@/ui-kit/primitives/button";
import { Textarea } from "@/ui-kit/primitives/textarea";
import { toast } from "@/ui-kit/primitives/toast";

// entryKey identifies one granted model; provider ids never contain "/", model ids may.
// entryKey 标识一个授权模型；Provider 标识不含 "/"，模型标识可以包含。
export function subagentModelEntryKey(entry: Pick<SubagentModelEntry, "providerId" | "modelId">): string {
  return `${entry.providerId}/${entry.modelId}`;
}

// sameSubagentModels tells whether two lists save the same content, notes compared as saved (trimmed).
// sameSubagentModels 判断两份清单保存后的内容是否相同，备注按保存形态（去除首尾空白）比较。
export function sameSubagentModels(left: readonly SubagentModelEntry[], right: readonly SubagentModelEntry[]): boolean {
  return left.length === right.length && left.every((entry, index) => (
    subagentModelEntryKey(entry) === subagentModelEntryKey(right[index]) && entry.note.trim() === right[index].note.trim()
  ));
}

// trimmedSubagentModels returns the list as it is saved.
// trimmedSubagentModels 返回保存时的清单形态。
export function trimmedSubagentModels(models: readonly SubagentModelEntry[]): SubagentModelEntry[] {
  return models.map((entry) => ({ ...entry, note: entry.note.trim() }));
}

// useSubagentModelDirectory resolves granted models to their current display names; missing ones are unavailable.
// useSubagentModelDirectory 将授权模型解析为当前显示名称；找不到的即为不可用。
export function useSubagentModelDirectory(providers: ProviderSummary[]): Map<string, ModelPickerModelGroup> {
  const { t } = useI18n();
  return React.useMemo(() => {
    const groups = groupModelPickerChoices(modelPickerChoicesFromProviders(providers).map((choice) => ({
      ...choice,
      providerName: localizedProviderName(choice.providerId, choice.providerName, t),
    })));
    return new Map(groups.map((group) => [subagentModelEntryKey(group), group]));
  }, [providers, t]);
}

// noteLength counts Unicode scalar values, the unit the service limit uses.
// noteLength 按 Unicode 标量值计数，与服务端限制单位一致。
function noteLength(note: string): number {
  return Array.from(note.trim()).length;
}

// subagentModelNotesFit tells whether every note fits the service limit, so saving is offered only when it can succeed.
// subagentModelNotesFit 判断每条说明是否都在服务端限制内，只在能成功时提供保存。
export function subagentModelNotesFit(models: readonly SubagentModelEntry[], maxNoteChars: number): boolean {
  return models.every((entry) => noteLength(entry.note) <= maxNoteChars);
}

// SubagentModelEditorProps configures one editable allowlist.
// SubagentModelEditorProps 配置一份可编辑白名单。
export interface SubagentModelEditorProps {
  // Current list in display order and its change callback.
  // 按显示顺序排列的当前清单及其变更回调。
  models: SubagentModelEntry[];
  onChange: (models: SubagentModelEntry[]) => void;
  // Providers carrying the enabled model rows.
  // 携带已启用模型行的 Provider 列表。
  providers: ProviderSummary[];
  // Bounds reported by the service.
  // 服务端报告的边界。
  maxNoteChars: number;
  maxModels: number;
  // Names the session model when the editor belongs to one session.
  // 编辑器属于某个会话时给出会话模型名称。
  sessionModelLabel?: string;
  disabled?: boolean;
  // Distinguishes system and session editors for tests and focus targets.
  // 为测试与聚焦目标区分系统与会话编辑器。
  scope: "system" | "session";
}

// SubagentModelEditor edits an allowlist: a pinned session-model card, one card per granted model with its note, and the add bar.
// SubagentModelEditor 编辑一份白名单：置顶的会话模型卡片、每个授权模型一张带说明的卡片，以及添加栏。
export function SubagentModelEditor(props: SubagentModelEditorProps) {
  const { t } = useI18n();
  const directory = useSubagentModelDirectory(props.providers);
  const list = React.useRef<HTMLOListElement>(null);
  // focusNote moves focus to one card's note after it is added or moved, so keyboard work continues in place.
  // focusNote 在卡片添加或移动后把焦点移到其说明框，使键盘操作原地继续。
  const focusNote = (key: string) => window.requestAnimationFrame(() => {
    const notes = list.current?.querySelectorAll<HTMLTextAreaElement>("[data-sam-note]") ?? [];
    [...notes].find((note) => note.dataset.samNote === key)?.focus();
  });
  const update = (index: number, note: string) => props.onChange(props.models.map((entry, at) => at === index ? { ...entry, note } : entry));
  const move = (index: number, offset: -1 | 1) => {
    const next = [...props.models];
    const [entry] = next.splice(index, 1);
    next.splice(index + offset, 0, entry);
    props.onChange(next);
    focusNote(subagentModelEntryKey(entry));
  };
  const remove = (index: number) => props.onChange(props.models.filter((_, at) => at !== index));
  const add = (group: Pick<ModelPickerModelGroup, "providerId" | "modelId" | "modelName">) => {
    const key = subagentModelEntryKey(group);
    if (props.models.some((entry) => subagentModelEntryKey(entry) === key)) {
      toast.info(formatMessage(t("subagentModels.duplicate"), { model: group.modelName }));
      focusNote(key);
      return;
    }
    props.onChange([...props.models, { providerId: group.providerId, modelId: group.modelId, note: "" }]);
    focusNote(key);
  };
  const full = props.models.length >= props.maxModels;
  return (
    <div className="vk-sam" data-sam-scope={props.scope}>
      <div className="vk-sam-anchor">
        <span aria-hidden="true" className="vk-sam-glyph"><Lock /></span>
        <div className="vk-sam-anchor-text">
          <div className="vk-sam-anchor-head">
            <strong>{props.sessionModelLabel
              ? formatMessage(t("subagentModels.sessionModel.current"), { model: props.sessionModelLabel })
              : t("subagentModels.sessionModel.title")}</strong>
            <span className="vk-sam-tag">{t("subagentModels.sessionModel.always")}</span>
          </div>
          <p>{t("subagentModels.sessionModel.detail")}</p>
        </div>
      </div>
      {props.models.length === 0 ? (
        <PageEmptyState data-sam-empty="" description={t("subagentModels.empty.detail")} icon={<Bot />}
          title={t("subagentModels.empty.title")} />
      ) : (
        <ol className="vk-sam-list" ref={list}>
          {props.models.map((entry, index) => {
            const key = subagentModelEntryKey(entry);
            const group = directory.get(key);
            const length = noteLength(entry.note);
            const name = group?.modelName ?? entry.modelId;
            const noteId = `sam-${props.scope}-${index}`;
            return (
              <li className="vk-sam-card" data-sam-model={key} data-unavailable={group ? undefined : ""} key={key}>
                <div className="vk-sam-card-head">
                  <span aria-hidden="true" className="vk-sam-index">{String(index + 1).padStart(2, "0")}</span>
                  <ProviderIcon className="vk-sam-icon" providerId={entry.providerId} size={22} />
                  <div className="vk-sam-name">
                    <strong title={name}>{name}</strong>
                    <small title={key}>{group?.providerName ?? entry.providerId} · <code>{key}</code></small>
                  </div>
                  {group ? null : (
                    <span className="vk-sam-tag" data-tone="warn" title={t("subagentModels.unavailableHint")}>
                      <TriangleAlert aria-hidden="true" />{t("subagentModels.unavailable")}
                    </span>
                  )}
                  <div className="vk-sam-tools">
                    <IconButton aria-label={formatMessage(t("subagentModels.moveUp"), { model: name })} disabled={props.disabled || index === 0}
                      onClick={() => move(index, -1)} size="xs" type="button"><ArrowUp size={14} /></IconButton>
                    <IconButton aria-label={formatMessage(t("subagentModels.moveDown"), { model: name })} disabled={props.disabled || index === props.models.length - 1}
                      onClick={() => move(index, 1)} size="xs" type="button"><ArrowDown size={14} /></IconButton>
                    <IconButton aria-label={formatMessage(t("subagentModels.remove"), { model: name })} disabled={props.disabled}
                      onClick={() => remove(index)} size="xs" type="button"><Trash2 size={14} /></IconButton>
                  </div>
                </div>
                <div className="vk-sam-note">
                  <Textarea aria-describedby={`${noteId}-foot`} aria-label={formatMessage(t("subagentModels.note.label"), { model: name })}
                    data-sam-note={key} density="compact" disabled={props.disabled} id={noteId}
                    onChange={(event) => update(index, event.currentTarget.value)}
                    placeholder={t("subagentModels.note.placeholder")} value={entry.note} />
                  <div className="vk-sam-note-foot" id={`${noteId}-foot`}>
                    <span>{group ? t("subagentModels.note.hint") : t("subagentModels.unavailableHint")}</span>
                    <span className="vk-sam-count" data-tone={length > props.maxNoteChars ? "bad" : length > props.maxNoteChars * 0.9 ? "warn" : undefined}>
                      {length} / {props.maxNoteChars}
                    </span>
                  </div>
                </div>
              </li>
            );
          })}
        </ol>
      )}
      <div className="vk-sam-add">
        <Plus aria-hidden="true" />
        <ModelPickerControl
          ariaLabel={t("subagentModels.add")}
          disabled={props.disabled || full}
          fallbackLabel={t("subagentModels.add")}
          filterPlaceholder={t("composer.modelPicker.filter")}
          noMatchesLabel={t("composer.modelPicker.noMatches")}
          onValueChange={(choice) => add(choice)}
          providers={props.providers}
          selectionScope="model"
          value=""
        />
        <span className="vk-sam-add-hint">{full ? t("subagentModels.full") : t("subagentModels.addHint")}</span>
        <span className="vk-sam-add-limit">{props.models.length} / {props.maxModels}</span>
      </div>
    </div>
  );
}
