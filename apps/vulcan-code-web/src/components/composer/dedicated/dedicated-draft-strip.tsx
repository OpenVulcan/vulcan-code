import { Folder } from "lucide-react";
import {
  SegmentedControl,
  SegmentedControlItem,
} from "@/components/ui/choice-controls";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { useI18n } from "@/i18n";
import type { DedicatedToolAccess } from "@/service/conversation-event-window-client";
import type { DraftComposerContextValue } from "./draft-composer-context";
import type { DedicatedDraftState, DraftSessionKind } from "./dedicated-draft-store";

/** Chip row above an unsent draft: owning project, session kind, and dedicated tool access.
 * 未发送草稿上方的标签行：所属项目、会话种类与专职工具开放方式。
 *
 * @param props.draft workbench draft facts; props.state and props.onChange own the local choices.
 *                    工作台草稿事实；props.state 与 props.onChange 持有本地选择。
 * @param props.disabled freezes every chip while the first submission is in flight. */
export function DedicatedDraftStrip(props: {
  draft: DraftComposerContextValue;
  state: DedicatedDraftState;
  disabled: boolean;
  onChange: (update: (current: DedicatedDraftState) => DedicatedDraftState) => void;
}) {
  const { t } = useI18n();
  return (
    <div className="dedicated-composer-tab">
      <Select disabled={props.disabled} onValueChange={props.draft.onChangeProject} value={props.draft.projectId}>
        <SelectTrigger aria-label={t("composer.draft.project")} className="h-6 max-w-64 border-0 bg-transparent px-2 shadow-none" size="sm">
          <Folder size={14} aria-hidden="true" />
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          {props.draft.projects.map((project) => (
            <SelectItem key={project.id} value={project.id}>{project.name}</SelectItem>
          ))}
        </SelectContent>
      </Select>
      <SegmentedControl
        aria-label={t("composer.draft.kind")}
        disabled={props.disabled}
        onValueChange={(kind) => props.onChange((current) => ({ ...current, kind: kind as DraftSessionKind }))}
        value={props.state.kind}
      >
        <SegmentedControlItem value="work">{t("composer.draft.kindWork")}</SegmentedControlItem>
        <SegmentedControlItem value="dedicated">{t("composer.draft.kindDedicated")}</SegmentedControlItem>
      </SegmentedControl>
      {props.state.kind === "dedicated" ? (
        <SegmentedControl
          aria-label={t("composer.draft.toolAccess")}
          disabled={props.disabled}
          onValueChange={(access) => props.onChange((current) => ({ ...current, toolAccess: access as DedicatedToolAccess }))}
          value={props.state.toolAccess}
        >
          <SegmentedControlItem title={t("composer.draft.toolAccessNoneHint")} value="none">
            {t("composer.draft.toolAccessNone")}
          </SegmentedControlItem>
          <SegmentedControlItem title={t("composer.draft.toolAccessStandardHint")} value="standard">
            {t("composer.draft.toolAccessStandard")}
          </SegmentedControlItem>
        </SegmentedControl>
      ) : null}
    </div>
  );
}
