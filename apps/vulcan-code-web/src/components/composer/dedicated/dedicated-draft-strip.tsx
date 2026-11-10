import { Folder, FolderPlus } from "lucide-react";
import {
  SegmentedControl,
  SegmentedControlItem,
} from "@/ui-kit/primitives/choice-controls";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectSeparator,
  SelectTrigger,
  SelectValue,
} from "@/ui-kit/primitives/select";
import { useI18n } from "@/i18n";
import type { DedicatedToolAccess } from "@/service/conversation-event-window-client";
import type { DraftComposerContextValue } from "./draft-composer-context";
import type { DedicatedDraftState, DraftSessionKind } from "./dedicated-draft-store";

// NEW_PROJECT_OPTION is the picker row that opens the folder picker instead of choosing a project; the
// draft keeps its current project because the row never becomes the selected value.
// NEW_PROJECT_OPTION 是打开文件夹选择器而不是选择项目的行；该行不会成为选中值，因此草稿保留当前项目。
const NEW_PROJECT_OPTION = "__new_project__";

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
  // handleProjectChoice routes the project picker's rows: one existing project, or the folder-picker action.
  // handleProjectChoice 分发项目选择器的各行：某个已有项目，或文件夹选择器动作。
  const handleProjectChoice = (value: string) => {
    if (value === NEW_PROJECT_OPTION) {
      props.draft.onCreateProject();
      return;
    }
    props.draft.onChangeProject(value);
  };
  return (
    <div className="vk-draft-strip">
      <Select disabled={props.disabled} onValueChange={handleProjectChoice} value={props.draft.projectId}>
        <SelectTrigger aria-label={t("composer.draft.project")} size="sm">
          <Folder size={14} aria-hidden="true" />
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          {props.draft.projects.map((project) => (
            <SelectItem key={project.id} value={project.id}>{project.name}</SelectItem>
          ))}
          <SelectSeparator />
          <SelectItem value={NEW_PROJECT_OPTION}>
            <FolderPlus size={14} aria-hidden="true" />
            {t("sidebar.newProject")}
          </SelectItem>
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
